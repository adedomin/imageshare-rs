use std::{path::Path, sync::Arc};

use crate::config::{Ratelim, WebData};
use crate::middleware::csrf::HeaderCsrf;
use crate::middleware::ratelim::BucketRatelim;
use crate::models::{api::ApiError, mime::detect_ext};
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Multipart, State, multipart::Field},
    http::StatusCode,
    routing::post,
};
use axum_extra::TypedHeader;
use axum_extra::headers::ContentLength;
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufWriter},
};
use tower::ServiceBuilder;

struct UploadGuard<'a> {
    inner: Option<&'a Path>,
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
            _ = std::fs::remove_file(path);
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

fn payload_too_large(lim: usize) -> ApiError {
    ApiError::new_with_status(
        StatusCode::PAYLOAD_TOO_LARGE,
        format!("Your Image is too large! (limit: {lim} B)"),
    )
}

async fn upload_img(
    content_len: Option<TypedHeader<ContentLength>>,
    State(webdata): State<Arc<WebData>>,
    mut multipart: Multipart,
) -> Result<ApiError, ApiError> {
    let max_siz = webdata.image.get_max_siz();
    if let Some(content_len) = content_len
        && content_len.0.0 > max_siz as u64
    {
        return Err(payload_too_large(max_siz));
    }
    if let Some(mut field) = multipart.next_field().await.map_err(ApiError::new)? {
        let (initial_read, ext) = det_ext(&mut field).await?;
        let fname = webdata.image.gen_new_fname(ext);
        let mut upload = webdata.image.get_base();
        upload.push(&fname);
        // if the file fails beyond this point, it will be stale in the FIFO. oh well.
        if let Some(del) = webdata.image.push(&upload) {
            _ = tokio::task::spawn_blocking(move || {
                _ = std::fs::remove_file(del);
            });
        }

        let fguard = UploadGuard::new(&upload);
        {
            let mut written: usize = 0;
            let mut file = BufWriter::new(File::create_new(&upload).await.map_err(|e| {
                log::error!("FS File upload err: {e}");
                ApiError::new(e)
            })?);
            // write our mime detect read.
            written += initial_read.len();
            file.write_all(&initial_read).await.map_err(ApiError::new)?;
            while let Some(chunk) = field.chunk().await.map_err(ApiError::new)? {
                written += chunk.len();
                if written > max_siz {
                    return Err(payload_too_large(max_siz));
                }
                file.write_all(&chunk).await.map_err(ApiError::new)?;
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
location = /i {
    root /var/lib/imageshare/i;
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

pub fn routes<T: AsRef<std::path::Path>>(
    image_path: T,
    ratelim: Option<Ratelim>,
) -> Router<Arc<WebData>> {
    let r = Router::new().route("/upload", post(upload_img)).layer(
        ServiceBuilder::new()
            .layer(DefaultBodyLimit::disable())
            .layer(HeaderCsrf)
            .layer(BucketRatelim::from(ratelim)),
    );
    #[cfg(feature = "serve-files")]
    let r = r.nest_service(
        "/i",
        tower_http::services::ServeDir::new(image_path).with_buf_chunk_size(256 * 1024),
    );
    #[cfg(not(feature = "serve-files"))]
    let r = r.route("/i/{*any}", axum::routing::get(get_file_err));
    r
}
