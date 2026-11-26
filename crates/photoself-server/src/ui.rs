use std::path::PathBuf;

use axum::Router;
#[cfg(not(feature = "embed-ui"))]
use tower_http::services::{ServeDir, ServeFile};

use crate::state::AppState;

#[cfg(not(feature = "embed-ui"))]
pub fn service(state: &AppState) -> Router {
    let dir = PathBuf::from(&state.config.server.ui_dir);
    let index = dir.join("index.html");

    if !index.is_file() {
        tracing::warn!(
            ui_dir = %dir.display(),
            "UI build not found; serving API only (run `npm run build` in ui/)"
        );
    }

    let serve_dir = ServeDir::new(&dir).not_found_service(ServeFile::new(index));
    Router::new().fallback_service(serve_dir)
}

#[cfg(feature = "embed-ui")]
pub fn service(_state: &AppState) -> Router {
    Router::new().fallback(embedded_ui)
}

#[cfg(feature = "embed-ui")]
#[derive(rust_embed::RustEmbed)]
#[folder = "../../ui/dist/"]
struct UiAssets;

#[cfg(feature = "embed-ui")]
async fn embedded_ui(uri: axum::http::Uri) -> axum::response::Response {
    use std::path::Path;

    use axum::http::header;
    use axum::response::IntoResponse;

    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    let (body, name) = match UiAssets::get(path) {
        Some(content) => (content.data.into_owned(), Path::new(path).to_path_buf()),
        None => match UiAssets::get("index.html") {
            Some(content) => (content.data.into_owned(), PathBuf::from("index.html")),
            None => return (axum::http::StatusCode::NOT_FOUND, "UI not built").into_response(),
        },
    };

    let mime = mime_guess::from_path(&name)
        .first_or_octet_stream()
        .to_string();

    let cache = if name.extension().and_then(|value| value.to_str()) == Some("html") {
        "no-cache"
    } else {
        "public, max-age=31536000, immutable"
    };

    (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, cache.to_string()),
        ],
        body,
    )
        .into_response()
}
