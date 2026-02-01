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
use std::path::PathBuf;
use std::{path::Path, sync::Arc};

use crate::middleware::contentlen::HeaderSizeLim;
use crate::models::webdata::WebData;
use crate::models::{api::ApiError, mime::detect_ext};
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Multipart, State, multipart::Field},
    http::StatusCode,
    routing::post,
};
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufWriter},
};
use tower::ServiceBuilder;

pub struct UploadGuard<'a> {
    inner: Option<&'a Path>,
}

pub fn background_rm_file(del: PathBuf) {
    tokio::task::spawn_blocking(move || {
        _ = std::fs::remove_file(del);
    });
}

impl<'a> UploadGuard<'a> {
    pub fn new<T: AsRef<Path> + 'a>(path: &'a T) -> Self {
        Self {
            inner: Some(path.as_ref()),
        }
    }

    pub fn defuse(mut self) -> &'a Path {
        self.inner.take().unwrap()
    }
}

impl<'a> Drop for UploadGuard<'a> {
    fn drop(&mut self) {
        if let Some(path) = self.inner {
            background_rm_file(path.to_path_buf());
        }
    }
}

async fn det_ext(f: &mut Field<'_>) -> Result<(Bytes, &'static str), ApiError> {
    let initial = f
        .chunk()
        .await
        .map_err(ApiError::new)?
        .ok_or(ApiError::new_with_status(
            StatusCode::UNPROCESSABLE_ENTITY,
            "No bytes read.",
        ))?;
    // if somehow chunk returns less than 12 bytes, we should reject this request on principle of it being too slow.
    if let Some(ext) = detect_ext(&initial) {
        Ok((initial, ext))
    } else {
        Err(ApiError::new_with_status(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Only images and videos are supported.",
        ))
    }
}

pub fn payload_too_large(typ: &'static str, lim: usize) -> ApiError {
    ApiError::new_with_status(
        StatusCode::PAYLOAD_TOO_LARGE,
        format!("Your {typ} is too large! limit: {lim} bytes."),
    )
}

async fn upload_img(
    State(webdata): State<Arc<WebData>>,
    mut multipart: Multipart,
) -> Result<ApiError, ApiError> {
    if let Some(mut field) = multipart.next_field().await? {
        let (initial_read, ext) = det_ext(&mut field).await?;
        let fname = webdata.image.gen_new_fname(ext);
        let mut upload = webdata.image.get_base();
        upload.push(&fname);
        // if the file fails beyond this point, it will be stale in the FIFO. oh well.
        if let Some(del) = webdata.image.push(&upload) {
            background_rm_file(del);
        }

        let fguard = UploadGuard::new(&upload);
        {
            let max_siz = webdata.image.get_max_siz();
            let mut written: usize = 0;
            // if we collide with file names, better to just overwrite.
            let mut file = BufWriter::new(File::create(&upload).await?);
            // write our mime detect read.
            written += initial_read.len();
            file.write_all(&initial_read).await?;
            while let Some(chunk) = field.chunk().await? {
                written += chunk.len();
                // Technically forms can be sent with Transfer-Encoding: chunked.
                // So we must guard against large reads.
                if written > max_siz {
                    return Err(payload_too_large("image", max_siz));
                }
                file.write_all(&chunk).await?;
            }
            _ = file.flush().await;
        }
        _ = fguard.defuse();
        Ok(ApiError::new_ok(format!(
            "{}/i/{fname}",
            webdata.link_prefix
        )))
    } else {
        Err(ApiError::new_with_status(
            StatusCode::BAD_REQUEST,
            "No fields.",
        ))
    }
}

#[cfg(not(feature = "serve-files"))]
const FILE_ERR_MSG: &str = r###"
You are expected to use a Reverse Proxy to host imageshare if you disable the `serve-files` feature.
To serve the /i folder, Please see the example nginx snippet:

```nginx.conf
# assumes you use the default image path
location /i/ {
    add_header X-Content-Type-Options nosniff;
    alias /var/lib/imageshare-rs;
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
    let r = Router::new().route("/upload", post(upload_img)).layer(
        ServiceBuilder::new()
            .layer(DefaultBodyLimit::disable())
            .layer(HeaderSizeLim::from(webdata.image.get_max_siz())),
    );
    #[cfg(feature = "serve-files")]
    let r = r.nest_service(
        "/i",
        tower_http::services::ServeDir::new(webdata.image.get_base())
            .with_buf_chunk_size(256 * 1024),
    );
    #[cfg(not(feature = "serve-files"))]
    let r = r.route("/i/{*any}", axum::routing::get(get_file_err));
    r
}
