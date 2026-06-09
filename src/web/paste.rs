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
use std::sync::Arc;

use axum::{
    Router,
    extract::{DefaultBodyLimit, State, rejection::StringRejection},
    response::Response,
    routing::post,
};

#[cfg(feature = "serve-files")]
use axum::extract::Path as ExtractPath;

use http::StatusCode;
use tower::ServiceBuilder;

use crate::{
    middleware::contentlen::HeaderSizeLim,
    models::{
        api::ApiError,
        dropfs::{DropFsGuard, background_rm_file},
        webdata::WebData,
    },
    web::image::payload_too_large,
};

fn handle_paste(r: Result<String, StringRejection>, lim: usize) -> Result<String, ApiError> {
    match r {
        Ok(paste) => Ok(paste),
        Err(e) if e.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            Err(payload_too_large("paste", lim, true))
        }
        Err(e) => Err(e.into()),
    }
}

async fn upload_paste(
    State(webdata): State<Arc<WebData>>,
    paste: Result<String, StringRejection>,
) -> Result<ApiError, ApiError> {
    let WebData {
        link_prefix,
        paste: storage,
        ..
    } = webdata.as_ref();
    let paste = handle_paste(paste, storage.get_max_siz())?;
    let fname = storage.gen_new_fname("txt");
    let mut upload = storage.get_base();
    upload.push(&fname);
    // if the file fails beyond this point, it will be stale in the FIFO. oh well.
    if let Some(del) = storage.push(&upload) {
        background_rm_file(del);
    }

    let fguard = DropFsGuard::new(&upload);
    tokio::fs::write(&upload, paste).await?;
    fguard.defuse();
    Ok(ApiError::new_ok(format!("{link_prefix}/p/{fname}")))
}

#[cfg(not(feature = "serve-files"))]
const FILE_ERR_MSG: &str = r###"
You are expected to use a Reverse Proxy to host imageshare if you disable the `serve-files` feature.
To serve the /p folder, Please see the example nginx snippet:

```nginx.conf
# assumes you use the default pastebin path
location /p/ {
    types { "text/plain; charset=utf-8" txt; }
    root /var/lib/imageshare-rs;
}
```
"###;

#[cfg(not(feature = "serve-files"))]
async fn get_file() -> Response {
    Response::builder()
        .status(http::StatusCode::OK)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf8")
        .body(FILE_ERR_MSG.into())
        .unwrap()
}

#[cfg(feature = "serve-files")]
async fn get_file(
    State(webdata): State<Arc<WebData>>,
    ExtractPath(path): ExtractPath<String>,
) -> Response {
    let mut b = webdata.paste.get_base();
    b.push(path);
    // NOTE: we already read in uploaded pastes into memory,
    // so we should be able to safely read them into memory to send.
    match tokio::fs::read(&b).await {
        Ok(b) => Response::builder()
            .status(http::StatusCode::OK)
            .header(http::header::CONTENT_TYPE, "text/plain; charset=utf8")
            // tower-http sets other conditional headers.
            // in this case we'll just tell the client that this basically does not change instead.
            .header(
                http::header::CACHE_CONTROL,
                "public, immutable, max-age=86400, stale-while-revalidate=1209600, stale-if-error=1209600",
            )
            .body(b.into())
            .unwrap(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Response::builder()
            .status(http::StatusCode::NOT_FOUND)
            .body(().into())
            .unwrap(),
        Err(e) => {
            eprintln!("ERR: unexpected I/O serving paste, {b:?}: {e}");
            Response::builder()
                .status(http::StatusCode::INTERNAL_SERVER_ERROR)
                .header(http::header::CONTENT_TYPE, "text/plain; charset=utf8")
                .body("I/O Error".into())
                .unwrap()
        }
    }
}

pub fn upload_route(lim: usize) -> Router<Arc<WebData>> {
    Router::new().route("/paste", post(upload_paste)).layer(
        ServiceBuilder::new()
            .layer(DefaultBodyLimit::max(lim))
            .layer(HeaderSizeLim::from(lim)),
    )
}

pub fn serve_route() -> Router<Arc<WebData>> {
    Router::new().route("/p/{any}", axum::routing::get(get_file))
}
