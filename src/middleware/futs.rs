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
use std::{
    pin::Pin,
    task::{Context, Poll},
};

use axum::response::Response;
use pin_project_lite::pin_project;

pin_project! {
    pub struct EarlyRetFut<I> {
        #[pin]
        inner: EarlyRetFutType<I>,
    }
}

pin_project! {
    #[project = EarlyRetFutTypeProj]
    pub enum EarlyRetFutType<I> {
        Next {
            #[pin]
            fut: I,
        },
        Early {
            resp: Option<Response>,
        },
    }
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
