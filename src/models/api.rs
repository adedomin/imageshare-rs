use std::fmt::Display;

use axum::{extract::rejection::StringRejection, response::IntoResponse};
use http::{HeaderName, HeaderValue, Response, StatusCode, header::CONTENT_TYPE};
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct ApiError {
    #[serde(skip)]
    code: StatusCode,
    status: &'static str,
    msg: String,
}

const FALLBACK: &[u8] = br##"{ "status": "critical", "msg": "failed to serialize api message." }"##;

pub const JSON_TYPE: (HeaderName, HeaderValue) = (
    CONTENT_TYPE,
    HeaderValue::from_static("application/json; charset=utf-8"),
);

impl ApiError {
    pub fn new<T: Display>(msg: T) -> Self {
        ApiError {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            status: "error",
            msg: msg.to_string(),
        }
    }

    pub fn new_ok<T: Display>(msg: T) -> Self {
        ApiError {
            code: StatusCode::OK,
            status: "ok",
            msg: msg.to_string(),
        }
    }

    pub fn new_with_status<T: Display>(code: StatusCode, msg: T) -> Self {
        ApiError {
            code,
            status: if code.is_success() { "ok" } else { "error" },
            msg: msg.to_string(),
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

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        Response::builder()
            .status(self.code)
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .body(self.to_json().into())
            .unwrap()
    }
}
