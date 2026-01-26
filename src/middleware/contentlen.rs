use std::task::{Context, Poll};

use axum::{
    extract::Request,
    response::{IntoResponse, Response},
};
use http::{HeaderMap, StatusCode, header::CONTENT_LENGTH};
use tower::{Layer, Service};

use crate::{middleware::futs::EarlyRetFut, models::api::ApiError};

/// A Tower Layer that checks HTTP Header Content-Length and rejects requests that are too large.
#[derive(Clone)]
pub struct HeaderSizeLim(usize);

impl From<usize> for HeaderSizeLim {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

/// A Tower Service that checks HTTP Header Content-Length and rejects requests that are too large.
#[derive(Clone)]
pub struct HeaderSizeLimMiddle<S> {
    inner: S,
    siz: usize,
}

impl<S> Layer<S> for HeaderSizeLim {
    type Service = HeaderSizeLimMiddle<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::Service { inner, siz: self.0 }
    }
}

/// check content-length and make sure the stated size is less than or equal to the limit.
fn check_len(headers: &HeaderMap, lim: usize) -> bool {
    // hyper kills connections with multiple, conflicting content-lengths.
    // Should be safe to read the first one.
    match headers.get(CONTENT_LENGTH).map(|v| {
        v.to_str()
            .or(Err(()))
            .and_then(|v| v.parse::<usize>().or(Err(())))
    }) {
        Some(Ok(len)) => len <= lim,
        // shouldn't happen; parse error.
        Some(Err(_)) => false,
        // no header.
        None => true,
    }
    // TODO: What should we do when we get Transfer-Encoding: chunked && Content-Lenght: NUM ???
}

impl<S> Service<Request> for HeaderSizeLimMiddle<S>
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

    fn call(&mut self, req: Request) -> Self::Future {
        // only non-safe should have *any* body.
        if !req.method().is_safe() {
            let lim = self.siz;
            let headers = req.headers();
            if !check_len(headers, lim) {
                return EarlyRetFut::new_early(
                    ApiError::new_with_status(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!("Your request is too large: limit {lim} bytes."),
                    )
                    .into_response(),
                );
            }
        }
        EarlyRetFut::new_next(self.inner.call(req))
    }
}
