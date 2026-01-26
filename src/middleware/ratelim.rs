use axum::{
    extract::{ConnectInfo, Request},
    response::{IntoResponse, Response},
};
use governor::{
    Quota, RateLimiter,
    clock::Clock,
    state::{InMemoryState, NotKeyed, StateStore},
};
use http::{StatusCode, header::RETRY_AFTER};
use std::{
    hash::{BuildHasher, RandomState},
    net::{IpAddr, SocketAddr},
    sync::Arc,
    task::{Context, Poll},
};
use tower::{Layer, Service};

use crate::{
    config::Ratelim,
    middleware::futs::EarlyRetFut,
    models::api::{ApiError, JSON_TYPE},
};

#[derive(Clone)]
struct BucketRateLimState {
    trust_headers: bool,
    ratelim: Arc<RateLimiter<IpAddr, BucketStateStore, governor::clock::MonotonicClock>>,
}

#[derive(Clone)]
pub struct BucketRatelim {
    state: Option<BucketRateLimState>,
}

pub struct BucketStateStore(RandomState, Vec<InMemoryState>);

impl StateStore for BucketStateStore {
    type Key = IpAddr;

    fn measure_and_replace<T, F, E>(&self, key: &Self::Key, f: F) -> Result<T, E>
    where
        F: Fn(Option<governor::nanos::Nanos>) -> Result<(T, governor::nanos::Nanos), E>,
    {
        let ip_partial = match key.to_canonical() {
            IpAddr::V4(ipv4) => u64::from(ipv4.to_bits()),
            // what should we do for subnets?
            // ip/48 is probably the most encompassing
            // ip/56 are common for some residential ISPs.
            // devices will be given a ip/64 for SLAAC at a minimum.
            IpAddr::V6(ipv6) => (ipv6.to_bits() >> 64) as u64,
        };
        let ip_partial = self.0.hash_one(ip_partial) as usize % self.1.len();
        self.1[ip_partial].measure_and_replace(&NotKeyed::NonKey, f)
    }
}

impl From<Option<Ratelim>> for BucketRatelim {
    fn from(rl: Option<Ratelim>) -> Self {
        let Some(rl) = rl else {
            return Self { state: None };
        };
        let quota = Quota::with_period(rl.secs())
            .expect("ratelim config is always nonzero.")
            .allow_burst(rl.burst());
        let state = BucketStateStore(
            RandomState::new(),
            (0..rl.bucket_size())
                .map(|_| InMemoryState::default())
                .collect(),
        );
        Self {
            state: Some(BucketRateLimState {
                trust_headers: rl.trust_headers(),
                ratelim: Arc::new(RateLimiter::new(
                    quota,
                    state,
                    governor::clock::MonotonicClock,
                )),
            }),
        }
    }
}

#[derive(Clone)]
pub struct BucketRatelimMiddle<S> {
    inner: S,
    state: Option<BucketRateLimState>,
}

impl<S> Layer<S> for BucketRatelim {
    type Service = BucketRatelimMiddle<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::Service {
            inner,
            state: self.state.clone(),
        }
    }
}

impl<S> Service<Request> for BucketRatelimMiddle<S>
where
    S: Service<Request, Response = Response> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = EarlyRetFut<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        // if None, disabled.
        let Some(state) = self.state.take() else {
            return EarlyRetFut::new_next(self.inner.call(request));
        };
        let get_ip = || {
            request
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|f| f.ip())
        };
        let ip = if state.trust_headers {
            let ip = request
                .headers()
                .get("X-Real-IP")
                .and_then(|hv| hv.to_str().ok())
                .and_then(|hv| hv.parse().ok());
            if ip.is_none() { get_ip() } else { ip }
        } else {
            get_ip()
        };
        let Some(ip) = ip else {
            return EarlyRetFut::new_early(
                ApiError::new("X-Real-IP header missing or failed to extract IpAddr from Request.")
                    .into_response(),
            );
        };

        if let Err(not_until) = state.ratelim.check_key(&ip) {
            // bit weird... especially since NotUntil has a private field start.
            let start = state.ratelim.clock().now();
            let retry_after = not_until.wait_time_from(start).as_secs();

            EarlyRetFut::new_early(
                Response::builder()
                    .status(StatusCode::TOO_MANY_REQUESTS)
                    .header(JSON_TYPE.0, JSON_TYPE.1)
                    .header(RETRY_AFTER, retry_after)
                    .body(
                        ApiError::new(format!("Retry after {retry_after} seconds."))
                            .to_json()
                            .into(),
                    )
                    .unwrap(),
            )
        } else {
            EarlyRetFut::new_next(self.inner.call(request))
        }
    }
}
