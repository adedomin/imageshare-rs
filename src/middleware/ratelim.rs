// Copyright (c) 2026, Anthony DeDominic <adedomin@gmail.com>
//
// Permission to use, copy, modify, and/or distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
use axum::{
    extract::{ConnectInfo, Request},
    response::{IntoResponse, Response},
};
use governor::{
    Quota, RateLimiter,
    clock::{Clock, MonotonicClock},
    nanos::Nanos,
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
    ratelim: Arc<RateLimiter<IpAddr, BucketStateStore, MonotonicClock>>,
}

#[derive(Clone)]
pub struct BucketRatelim {
    state: BucketRateLimState,
}

pub struct BucketStateStore(RandomState, Vec<InMemoryState>);

impl StateStore for BucketStateStore {
    type Key = IpAddr;

    fn measure_and_replace<T, F, E>(&self, key: &Self::Key, f: F) -> Result<T, E>
    where
        F: Fn(Option<Nanos>) -> Result<(T, Nanos), E>,
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

impl From<Ratelim> for BucketRatelim {
    fn from(rl: Ratelim) -> Self {
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
            state: BucketRateLimState {
                trust_headers: rl.trust_headers(),
                ratelim: Arc::new(RateLimiter::new(quota, state, MonotonicClock)),
            },
        }
    }
}

#[derive(Clone)]
pub struct BucketRatelimMiddle<S> {
    inner: S,
    state: BucketRateLimState,
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
        let get_ip = || {
            request
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|f| f.ip())
        };
        let ip = if self.state.trust_headers {
            request
                .headers()
                .get("X-Real-IP")
                .and_then(|hv| hv.to_str().ok())
                .and_then(|hv| hv.parse().ok())
        } else {
            None
        };
        let Some(ip) = ip.or_else(get_ip /* fallback */) else {
            return EarlyRetFut::new_early(
                ApiError::new("X-Real-IP header missing or failed to extract IpAddr from Request.")
                    .into_response(),
            );
        };

        if let Err(not_until) = self.state.ratelim.check_key(&ip) {
            // bit weird... especially since NotUntil has a private field start.
            let start = self.state.ratelim.clock().now();
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
