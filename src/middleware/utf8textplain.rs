use std::{
    pin::Pin,
    task::{Context, Poll},
};

use axum::{extract::Request, response::Response};
use http::HeaderValue;
use pin_project_lite::pin_project;
use tower::{Layer, Service};

pin_project! {
    pub struct Utf8TextPlainFut<I> {
        #[pin]
        inner: I
    }
}

impl<I, E, ResBody> Future for Utf8TextPlainFut<I>
where
    I: Future<Output = Result<Response<ResBody>, E>>,
{
    type Output = Result<Response<ResBody>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        match this.inner.poll(cx) {
            Poll::Ready(Ok(mut res)) => {
                if let Some(ct) = res.headers_mut().get_mut(http::header::CONTENT_TYPE) {
                    *ct = HeaderValue::from_static("text/plain; charset=utf-8");
                }
                Poll::Ready(Ok(res))
            }
            other => other,
        }
    }
}

/// A Tower Layer that rewrites content-type to use charset=utf-8
#[derive(Clone)]
pub struct Utf8TextPlain;

/// A Tower Service that rewrites content-type to use charset=utf-8
#[derive(Clone)]
pub struct Utf8TextPlainService<S> {
    inner: S,
}

impl<S> Layer<S> for Utf8TextPlain {
    type Service = Utf8TextPlainService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::Service { inner }
    }
}

impl<S, ResBody> Service<Request> for Utf8TextPlainService<S>
where
    S: Service<Request, Response = Response<ResBody>> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Utf8TextPlainFut<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let inner = self.inner.call(req);
        Utf8TextPlainFut { inner }
    }
}
