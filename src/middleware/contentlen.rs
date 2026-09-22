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
use std::task::{Context, Poll};

use axum::{
    extract::Request,
    response::{IntoResponse, Response},
};
use http::{HeaderMap, StatusCode, header::CONTENT_LENGTH};
use tower::{Layer, Service};

use crate::{middleware::earlyretfut::EarlyRetFut, models::api::ApiError};

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

fn get_len(headers: &HeaderMap) -> Option<usize> {
    let mut iter = headers.get_all(CONTENT_LENGTH).iter();
    // no try_reduce...
    let init = iter.next()?;
    // should not happen... sanity anyway.
    if iter.all(|v| init == v) {
        init.to_str().ok()?.parse::<usize>().ok()
    } else {
        None
    }
}

#[derive(Clone)]
pub struct ClaimedLen(pub usize);

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

    fn call(&mut self, mut req: Request) -> Self::Future {
        // only non-safe should have *any* body.
        if !req.method().is_safe()
            && let Some(len) = get_len(req.headers())
        {
            let lim = self.siz;
            if len > lim {
                return EarlyRetFut::new_early(
                    ApiError::new_with_status(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!("Your request is too large! limit {lim} bytes."),
                    )
                    .into_response(),
                    req.into_body().into_data_stream(),
                );
            }
            req.extensions_mut().insert(ClaimedLen(len));
        }
        EarlyRetFut::new_next(self.inner.call(req))
    }
}
