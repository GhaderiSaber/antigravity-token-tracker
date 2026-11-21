use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "src/web/dist/"]
pub struct FrontendAssets;

pub async fn static_handler(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let file_path = if path.is_empty() || path == "index.html" {
        "index.html"
    } else {
        path
    };

    match FrontendAssets::get(file_path) {
        Some(content) => {
            let mime = mime_guess::from_path(file_path).first_or_octet_stream();
            let mut headers = HeaderMap::new();
            if let Ok(val) = HeaderValue::from_str(mime.as_ref()) {
                headers.insert(CONTENT_TYPE, val);
            }
            if file_path.starts_with("assets/") {
                headers.insert(
                    CACHE_CONTROL,
                    HeaderValue::from_static("public, max-age=31536000, immutable"),
                );
            } else {
                headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            }
            (StatusCode::OK, headers, content.data).into_response()
        }
        None => {
            // Fallback to index.html for SPA routing
            match FrontendAssets::get("index.html") {
                Some(index_content) => {
                    let mut headers = HeaderMap::new();
                    headers.insert(
                        CONTENT_TYPE,
                        HeaderValue::from_static("text/html; charset=utf-8"),
                    );
                    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
                    (StatusCode::OK, headers, index_content.data).into_response()
                }
                None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
            }
        }
    }
}
