use std::path::PathBuf;

use axum::{
    Router,
    extract::Request,
    http::{
        StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
    routing::get,
};
use include_dir::{Dir, include_dir};

const CLIENT_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/public");

enum Static {
    Content(&'static [u8], &'static str),
    NotFound,
}

impl IntoResponse for Static {
    fn into_response(self) -> axum::response::Response {
        let Self::Content(body, content_type) = self else {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body("No such file or directory.".into())
                .unwrap();
        };
        Response::builder().status(StatusCode::OK)
            .header(CACHE_CONTROL, "public, immutable, max-age=86400, stale-while-revalidate=1209600, stale-if-error=1209600")
            .header(CONTENT_TYPE, content_type)
            .body(body.into()).unwrap()
    }
}

pub const MIME: [(&str, &str); 4] = [
    ("css", "text/css; charset=utf-8"),
    ("html", "text/html; charset=utf-8"),
    ("js", "application/javascript; charset=utf-8"),
    ("ico", "image/x-icon"),
];

pub fn get_mime(ext: &str) -> &'static str {
    match MIME.iter().find(|(e, _)| ext == *e) {
        Some((_, t)) => t,
        None => "application/octet-string",
    }
}

fn get_static_file_from(d: &'static Dir, path: &str, ext: &str) -> Static {
    d.get_file(path)
        .map(|file| Static::Content(file.contents(), get_mime(ext)))
        .unwrap_or(Static::NotFound)
}

async fn static_content(req: Request) -> Static {
    let loc = req.uri().path();
    let Some(loc) = loc.strip_prefix("/public/") else {
        return Static::NotFound;
    };
    let locp = PathBuf::from(loc);
    let ext = locp.extension().unwrap_or_default().to_string_lossy();
    get_static_file_from(&CLIENT_DIR, loc, ext.as_ref())
}

async fn index_page() -> Static {
    get_static_file_from(&CLIENT_DIR, "index.html", "html")
}

async fn favicon() -> Static {
    get_static_file_from(&CLIENT_DIR, "favicon.ico", "ico")
}

pub fn routes<T: Send + Sync + Clone + 'static>() -> Router<T> {
    Router::new()
        .route("/", get(index_page))
        .route("/favicon.ico", get(favicon))
        .fallback(static_content)
}
