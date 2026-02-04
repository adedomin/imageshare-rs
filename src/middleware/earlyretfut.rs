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
use http::{HeaderValue, header::CONNECTION};
use pin_project_lite::pin_project;

// use crate::middleware::contentlen::get_len;

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
            body: BodyDataStream,
            read: usize,
        },
    }
}

const MAX_DRAIN_BODY: usize = 128 * 1024 * 1024 /* 128 MiB */;

impl<I> EarlyRetFut<I> {
    pub fn new_early(resp: Response, body: BodyDataStream) -> Self {
        Self {
            inner: EarlyRetFutType::Early {
                resp: Some(resp),
                body,
                read: 0,
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
            EarlyRetFutTypeProj::Early {
                resp,
                mut body,
                read,
                // lim,
            } => {
                // To prevent TCP|UDS connection reset issues, we should consume the body.
                let mut done = false;
                while *read < MAX_DRAIN_BODY {
                    match body.as_mut().poll_frame(cx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Some(Ok(frame))) => {
                            if let Ok(bytes) = frame.into_data() {
                                *read += bytes.len();
                            }
                        }
                        Poll::Ready(None) => {
                            done = true;
                            break;
                        }
                        // abort on err
                        _ => break,
                    };
                }
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
