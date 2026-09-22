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
use std::{path::Path, sync::Arc};

use crate::middleware::contentlen::{ClaimedLen, HeaderSizeLim};
use crate::middleware::earlyretfut::ConsumeBody;
use crate::models::dropfs::{DropFsGuard, build_with_base};
use crate::models::webdata::WebData;
use crate::models::{api::ApiError, mime::detect_ext};
use axum::Extension;
use axum::body::{Body, BodyDataStream};
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    routing::post,
};
use futures_util::stream::StreamExt;
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufWriter},
};
use tower::ServiceBuilder;

async fn get_ext(
    mut body: BodyDataStream,
) -> Result<(BodyDataStream, Bytes, &'static str), ApiError> {
    let initial = body
        .next()
        .await
        .ok_or(ApiError::new_with_status(
            StatusCode::UNPROCESSABLE_ENTITY,
            "No bytes read.",
        ))?
        .map_err(ApiError::new)?;
    // If we read less than 12 bytes, we should reject this request on principle of it being too slow or weird.
    if let Some(ext) = detect_ext(&initial) {
        Ok((body, initial, ext))
    } else {
        // attempt to consume rest of body.
        let done = ConsumeBody::new(body).await;
        Err(ApiError::new_with_status(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Unsupported image or video format.",
        )
        .should_close_conn(!done))
    }
}

pub fn payload_too_large(typ: &'static str, lim: usize, close: bool) -> ApiError {
    ApiError::new(format!("Your {typ} is too large! limit: {lim} bytes."))
        .status(StatusCode::PAYLOAD_TOO_LARGE)
        .should_close_conn(close)
}

async fn upload_img(
    State(webdata): State<Arc<WebData>>,
    claimed_len: Option<Extension<ClaimedLen>>,
    body: Body,
) -> Result<ApiError, ApiError> {
    let WebData {
        link_prefix,
        image: storage,
        ..
    } = webdata.as_ref();
    let Some(Extension(ClaimedLen(siz))) = claimed_len else {
        return Err(ApiError::new_with_status(
            StatusCode::BAD_REQUEST,
            "Image endpoint requires a Content-Length header.",
        ));
    };
    let (mut body, initial_read, ext) = get_ext(body.into_data_stream()).await?;

    let fname = storage.gen_new_fname(ext);
    // by entering into the cache, we're asking for at least `siz` reservation.
    storage.push(fname.clone(), siz);

    let upload = build_with_base(storage.get_base(), &fname);
    let fguard = DropFsGuard::new(&upload);
    {
        let mut written: usize = 0;
        // if we collide with file names, better to just overwrite.
        let mut file = BufWriter::new(File::create(&upload).await?);
        // write our mime detect read.
        written += initial_read.len();
        file.write_all(&initial_read).await?;
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(ApiError::new)?;
            written += chunk.len();
            // Technically forms can be sent with Transfer-Encoding: chunked.
            // So we must guard against large reads.
            if written > siz {
                let done = ConsumeBody::new(body).await;
                return Err(payload_too_large("image", siz, !done));
            }
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
    };
    fguard.defuse();
    Ok(ApiError::new_ok(format!("{link_prefix}/i/{fname}")))
}

// #[cfg(not(feature = "serve-files"))]
// const FILE_ERR_MSG: &str = r###"You are expected to use a Reverse Proxy to host imageshare if you disable the `serve-files` feature.
// To serve the /i folder, Please see the example nginx snippet:

// ```nginx.conf
// location /internal/i/ {
//     internal;
//     add_header X-Content-Type-Options nosniff;
//     # assumes you use the default image path
//     alias /var/lib/imageshare-rs/i/;
// }
// ```
// "###;

#[cfg(not(feature = "serve-files"))]
pub const NGINX_X_ACCEL_REDIRECT: http::HeaderName =
    http::HeaderName::from_static("x-accel-redirect");

#[cfg(not(feature = "serve-files"))]
async fn get_nginx_accel_redir(path: http::Uri) -> axum::response::Response {
    let p = path.path();
    axum::response::Response::builder()
        .status(http::StatusCode::OK)
        .header(NGINX_X_ACCEL_REDIRECT, format!("/internal{p}"))
        .body(().into())
        .unwrap()
}

pub fn upload_route(lim: usize) -> Router<Arc<WebData>> {
    Router::new().route("/upload", post(upload_img)).layer(
        ServiceBuilder::new()
            .layer(DefaultBodyLimit::disable())
            .layer(HeaderSizeLim::from(lim)),
    )
}

pub fn serve_route<P: AsRef<Path>>(_p: P) -> Router<Arc<WebData>> {
    let r = Router::new();
    #[cfg(feature = "serve-files")]
    let r = r.nest_service(
        "/i",
        tower_http::services::ServeDir::new(_p).with_buf_chunk_size(256 * 1024),
    );
    #[cfg(not(feature = "serve-files"))]
    let r = r.route("/i/{*any}", axum::routing::get(get_nginx_accel_redir));
    r
}
