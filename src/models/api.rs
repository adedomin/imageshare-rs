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
use std::fmt::Display;

use axum::{
    extract::{multipart::MultipartError, rejection::StringRejection},
    response::IntoResponse,
};
use http::{
    HeaderName, HeaderValue, Response, StatusCode,
    header::{CONNECTION, CONTENT_TYPE},
};
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct ApiError {
    #[serde(skip)]
    code: StatusCode,
    #[serde(skip)]
    close: bool,
    status: &'static str,
    msg: String,
}

const FALLBACK: &[u8] = br##"{ "status": "critical", "msg": "failed to serialize api message." }"##;

pub const JSON_TYPE: (HeaderName, HeaderValue) =
    (CONTENT_TYPE, HeaderValue::from_static("application/json"));

impl ApiError {
    pub fn new<T: Display>(msg: T) -> Self {
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            close: false,
            status: "error",
            msg: msg.to_string(),
        }
    }

    pub fn new_ok<T: Display>(msg: T) -> Self {
        ApiError {
            code: StatusCode::OK,
            close: false,
            status: "ok",
            msg: msg.to_string(),
        }
    }

    pub fn new_with_status<T: Display>(code: StatusCode, msg: T) -> Self {
        ApiError {
            code,
            close: false,
            status: if code.is_success() { "ok" } else { "error" },
            msg: msg.to_string(),
        }
    }

    pub fn status(self, code: StatusCode) -> Self {
        Self { code, ..self }
    }

    pub fn close_conn(self) -> Self {
        Self {
            close: true,
            ..self
        }
    }

    pub fn to_json(&self) -> Vec<u8> {
        match serde_json::to_vec(&self) {
            Ok(ok) => ok,
            Err(_) => FALLBACK.to_owned(),
        }
    }
}

impl From<StringRejection> for ApiError {
    fn from(value: StringRejection) -> Self {
        ApiError::new_with_status(value.status(), value)
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        eprintln!("ERR: unexpected I/O error: {e}");
        Self::new(e).close_conn()
    }
}

impl From<MultipartError> for ApiError {
    fn from(e: MultipartError) -> Self {
        Self::new(e).close_conn()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let res = Response::builder()
            .status(self.code)
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let res = if self.close {
            res.header(CONNECTION, "close")
        } else {
            res
        };
        res.body(self.to_json().into()).unwrap()
    }
}
