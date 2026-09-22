use std::{
    sync::Arc,
    task::{Context, Poll},
};

use axum::{extract::Request, response::Response};
use http::Method;
use tower::{Layer, Service};

use crate::models::webdata::WebData;

#[derive(Clone)]
pub struct RefreshCache {
    state: Arc<WebData>,
}

impl RefreshCache {
    pub fn new(state: Arc<WebData>) -> Self {
        Self { state }
    }
}

#[derive(Clone)]
pub struct RefreshCacheMiddle<S> {
    state: Arc<WebData>,
    inner: S,
}

impl<S> Layer<S> for RefreshCache {
    type Service = RefreshCacheMiddle<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::Service {
            state: self.state.clone(),
            inner,
        }
    }
}

impl<S> Service<Request> for RefreshCacheMiddle<S>
where
    S: Service<Request, Response = Response> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        if matches!(req.method(), &Method::GET) {
            let uri = req.uri().path();
            if let Some(fname) = uri.strip_prefix("/p/") {
                self.state.paste.refresh(fname);
            } else if let Some(fname) = uri.strip_prefix("/i/") {
                self.state.image.refresh(fname);
            }
        }
        self.inner.call(req)
    }
}
