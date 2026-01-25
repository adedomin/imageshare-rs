use axum::{
    Router,
    extract::Path,
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

fn get_ext(uri_path: &str) -> Option<&str> {
    uri_path.rsplit('/').next().and_then(|fname| {
        let mut itr = fname.rsplitn(2, '.');
        let ext = itr.next();
        let base = itr.next();
        match base {
            None | Some("") => None,
            _ => ext,
        }
    })
}

async fn static_content(Path(path): Path<String>) -> Static {
    let ext = get_ext(&path).unwrap_or("");
    get_static_file_from(&CLIENT_DIR, &path, ext)
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
        .route("/public/{*path}", get(static_content))
}
