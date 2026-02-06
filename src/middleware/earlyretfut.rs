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

use axum::{
    body::{BodyDataStream, HttpBody},
    response::Response,
};
use futures_util::ready;
use http::{HeaderValue, header::CONNECTION};
use pin_project_lite::pin_project;

pin_project! {
    /// Future that attempts to consume the remaining bytes in an axum Body.
    pub struct ConsumeBody {
        #[pin]
        body: BodyDataStream,
        read: usize,
    }
}

impl ConsumeBody {
    pub fn new(body: BodyDataStream) -> Self {
        Self { body, read: 0 }
    }
}

const MAX_DRAIN_BODY: usize = 128 * 1024 * 1024 /* 128 MiB */;

impl Future for ConsumeBody {
    type Output = bool;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let mut body = this.body;
        let read = this.read;

        // To prevent TCP|UDS connection reset issues, we should consume the body.
        while *read < MAX_DRAIN_BODY {
            match ready!(body.as_mut().poll_frame(cx)) {
                Some(Ok(frame)) => {
                    if let Ok(bytes) = frame.into_data() {
                        *read += bytes.len();
                    }
                }
                // abort on err
                Some(Err(_)) => return Poll::Ready(false),
                // read entire body.
                None => return Poll::Ready(true),
            };
        }
        // read up to MAX_DRAIN_BODY or more.
        Poll::Ready(false)
    }
}

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
            #[pin]
            body: ConsumeBody,
        },
    }
}

impl<I> EarlyRetFut<I> {
    pub fn new_early(resp: Response, body: BodyDataStream) -> Self {
        Self {
            inner: EarlyRetFutType::Early {
                resp: Some(resp),
                body: ConsumeBody::new(body),
            },
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
            EarlyRetFutTypeProj::Early { resp, body } => {
                let done = ready!(body.poll(cx));
                let mut resp = resp.take().expect("Already Responded.");
                if !done {
                    resp.headers_mut()
                        .insert(CONNECTION, HeaderValue::from_static("close"));
                }
                Poll::Ready(Ok(resp))
            }
        }
    }
}
