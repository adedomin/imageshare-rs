use std::{
    pin::Pin,
    task::{Context, Poll},
};

use axum::response::Response;
use pin_project::pin_project;

#[pin_project]
pub struct EarlyRetFut<I> {
    #[pin]
    inner: EarlyRetFutType<I>,
}

#[pin_project(project = EarlyRetFutTypeProj)]
pub enum EarlyRetFutType<I> {
    Next {
        #[pin]
        fut: I,
    },
    Early {
        resp: Option<Response>,
    },
}

impl<I> EarlyRetFut<I> {
    pub fn new_early(resp: Response) -> Self {
        Self {
            inner: EarlyRetFutType::Early { resp: Some(resp) },
        }
    }

    pub fn new_next(fut: I) -> Self {
        Self {
            inner: EarlyRetFutType::Next { fut },
        }
    }
}

impl<I, E> Future for EarlyRetFut<I>
where
    I: Future<Output = Result<Response, E>>,
{
    type Output = Result<Response, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().inner.project() {
            EarlyRetFutTypeProj::Next { fut } => fut.poll(cx),
            EarlyRetFutTypeProj::Early { resp } => Poll::Ready(Ok(resp
                .take()
                .expect("option used for take() out of projection."))),
        }
    }
}
