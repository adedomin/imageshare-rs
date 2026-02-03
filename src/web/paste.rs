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
    routing::post,
};
use http::StatusCode;
use tower::ServiceBuilder;

#[cfg(feature = "serve-files")]
use crate::middleware::utf8textplain::Utf8TextPlain;
use crate::{
    middleware::contentlen::HeaderSizeLim,
    models::{api::ApiError, webdata::WebData},
    web::image::{UploadGuard, background_rm_file, payload_too_large},
};

fn handle_paste(r: Result<String, StringRejection>, lim: usize) -> Result<String, ApiError> {
    match r {
        Ok(paste) => Ok(paste),
        Err(e) if e.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            Err(payload_too_large("paste", lim))
        }
        Err(e) => Err(e.into()),
    }
}

async fn upload_paste(
    State(webdata): State<Arc<WebData>>,
    paste: Result<String, StringRejection>,
) -> Result<ApiError, ApiError> {
    let paste = handle_paste(paste, webdata.paste.get_max_siz())?;
    let fname = webdata.paste.gen_new_fname("txt");
    let mut upload = webdata.paste.get_base();
    upload.push(&fname);
    // if the file fails beyond this point, it will be stale in the FIFO. oh well.
    if let Some(del) = webdata.paste.push(&upload) {
        background_rm_file(del);
    }

    let fguard = UploadGuard::new(&upload);
    tokio::fs::write(&upload, paste).await?;
    _ = fguard.defuse();
    Ok(ApiError::new_ok(format!(
        "{}/p/{fname}",
        webdata.link_prefix
    )))
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
async fn get_file_err() -> axum::response::Response {
    axum::response::Response::builder()
        .status(http::StatusCode::OK)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf8")
        .body(FILE_ERR_MSG.into())
        .unwrap()
}

pub fn routes(webdata: Arc<WebData>) -> Router<Arc<WebData>> {
    let lim = webdata.paste.get_max_siz();
    let r = Router::new().route("/paste", post(upload_paste)).layer(
        ServiceBuilder::new()
            .layer(DefaultBodyLimit::max(lim))
            .layer(HeaderSizeLim::from(lim)),
    );
    #[cfg(feature = "serve-files")]
    let r = r
        .nest_service(
            "/p",
            tower_http::services::ServeDir::new(webdata.paste.get_base())
                .with_buf_chunk_size(256 * 1024),
        )
        .layer(Utf8TextPlain);
    #[cfg(not(feature = "serve-files"))]
    let r = r.route("/p/{*any}", axum::routing::get(get_file_err));
    r
}
