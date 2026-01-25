use std::task::{Context, Poll};

use axum::{
    extract::Request,
    response::{IntoResponse, Response},
};
use http::{
    HeaderMap, StatusCode,
    header::{HOST, ORIGIN},
};
use tower::{Layer, Service};

use crate::{middleware::futs::EarlyRetFut, models::api::ApiError};

const SEC_FETCH_SITE: &str = "Sec-Fetch-Site";
const SEC_FETCH_SITE_ALLOWED: &str = "same-origin";

/// Checks if Origin's schemaless value matches the Host header.
/// Any of the headers being missing is an automatic pass because it's assumed it is a weird custom client,
/// such as a phone app or a curl user.
fn origin_check(headers: &HeaderMap) -> Option<bool> {
    let origin = headers.get(ORIGIN).and_then(|v| v.to_str().ok())?;
    // Origin: <scheme>://<host>:<port>
    let (_, origin) = origin.split_once("://")?;
    // Host: <host>:<port>
    let host = headers.get(HOST).and_then(|v| v.to_str().ok())?;
    Some(origin == host)
}

/// Check if Sec-Fetch-Site is set and reject all non same-origin requests.
/// Any of the headers being missing is an automatic pass because it's assumed it is a weird custom client,
/// such as a phone app or a curl user.
fn sec_fetch_site_check(headers: &HeaderMap) -> Option<bool> {
    headers
        .get(SEC_FETCH_SITE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == SEC_FETCH_SITE_ALLOWED)
}

/// A Tower Layer that checks HTTP Headers Sec-Fetch-Site: same-origin or Origin == Host.
#[derive(Clone)]
pub struct HeaderCsrf;

/// A Tower Service that checks HTTP Headers Sec-Fetch-Site: same-origin or Origin == Host.
#[derive(Clone)]
pub struct HeaderCsrfMiddle<S> {
    inner: S,
}

impl<S> Layer<S> for HeaderCsrf {
    type Service = HeaderCsrfMiddle<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::Service { inner }
    }
}

impl<S> Service<Request> for HeaderCsrfMiddle<S>
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
        if !req.method().is_safe() {
            let headers = req.headers();
            if !sec_fetch_site_check(headers)
                .unwrap_or_else(|| origin_check(headers).unwrap_or(true))
            {
                return EarlyRetFut::new_early(
                    ApiError::new_with_status(StatusCode::FORBIDDEN, "CSRF failure.")
                        .into_response(),
                );
            }
        }
        EarlyRetFut::new_next(self.inner.call(req))
    }
}
