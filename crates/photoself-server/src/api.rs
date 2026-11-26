use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;

use axum::body::Body;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use photoself_core::model::{ItemQuery, Library, MediaItem, MediaKind, ScanStats};
use photoself_core::thumb;
use serde::Deserialize;
use serde_json::{json, Value};
use tower::ServiceExt;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeFile;
use tower_http::trace::TraceLayer;

use crate::auth;
use crate::state::AppState;
use crate::ui;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/api/v1/info", get(info))
        .route("/api/v1/items", get(list_items))
        .route("/api/v1/items/{id}", get(get_item).patch(patch_item))
        .route("/api/v1/items/{id}/thumb/{size}", get(get_thumb))
        .route("/api/v1/items/{id}/media", get(get_media))
        .route("/api/v1/scan", post(scan_handler))
        .route("/api/v1/jobs", get(list_jobs))
        .route("/api/v1/jobs/{id}", get(get_job))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
        .with_state(state.clone());

    Router::new()
        .route("/healthz", get(healthz))
        .merge(api)
        .fallback_service(ui::service(&state))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    pub fn internal(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self::internal(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(json!({
            "type": "about:blank",
            "title": self.status.canonical_reason().unwrap_or("Error"),
            "status": self.status.as_u16(),
            "detail": self.message,
        }));
        let mut response = (self.status, body).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            "application/problem+json".parse().unwrap(),
        );
        response
    }
}

async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn info(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let db = state.db.clone();
    let count = tokio::task::spawn_blocking(move || db.count_items())
        .await
        .map_err(ApiError::internal)??;

    let libraries: Vec<Value> = state
        .config
        .libraries
        .iter()
        .filter(|library| library.enabled)
        .map(|library| {
            json!({
                "id": library.id,
                "name": if library.name.is_empty() { &library.id } else { &library.name },
                "read_only": library.read_only,
            })
        })
        .collect();

    Ok(Json(json!({
        "name": "photoSelf",
        "version": env!("CARGO_PKG_VERSION"),
        "mode": state.config.server.mode,
        "auth_mode": state.config.auth.mode,
        "item_count": count,
        "thumb_sizes": state.config.thumbnails.sizes,
        "libraries": libraries,
    })))
}

#[derive(Debug, Deserialize)]
struct ListParams {
    library: Option<String>,
    kind: Option<String>,
    q: Option<String>,
    favorite: Option<bool>,
    from: Option<String>,
    to: Option<String>,
    cursor: Option<String>,
    limit: Option<i64>,
}

async fn list_items(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, ApiError> {
    let query = ItemQuery {
        library_id: params.library,
        kind: params.kind,
        q: params.q,
        favorite: params.favorite,
        from: params.from,
        to: params.to,
        cursor: params.cursor,
        limit: params.limit.unwrap_or(100),
    };

    let db = state.db.clone();
    let (items, next_cursor) = tokio::task::spawn_blocking(move || db.list_items(&query))
        .await
        .map_err(ApiError::internal)??;

    let items: Vec<Value> = items.iter().map(|item| item_dto(&state, item)).collect();
    Ok(Json(json!({
        "items": items,
        "next_cursor": next_cursor,
    })))
}

async fn get_item(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let item = fetch_item(&state, &id).await?;
    Ok(Json(item_dto(&state, &item)))
}

#[derive(Debug, Deserialize)]
struct PatchItem {
    favorite: Option<bool>,
}

async fn patch_item(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<PatchItem>,
) -> Result<Json<Value>, ApiError> {
    let Some(favorite) = body.favorite else {
        return Err(ApiError::bad_request("no supported fields in request"));
    };

    let db = state.db.clone();
    let item_id = id.clone();
    let changed = tokio::task::spawn_blocking(move || db.set_favorite(&item_id, favorite))
        .await
        .map_err(ApiError::internal)??;

    if !changed {
        return Err(ApiError::not_found("item not found"));
    }

    let item = fetch_item(&state, &id).await?;
    Ok(Json(item_dto(&state, &item)))
}

async fn get_thumb(
    State(state): State<AppState>,
    AxumPath((id, size)): AxumPath<(String, u32)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if !state.config.thumbnails.sizes.contains(&size) {
        return Err(ApiError::not_found("unsupported thumbnail size"));
    }

    let item = fetch_item(&state, &id).await?;
    if item.kind == MediaKind::Video && !state.config.video.poster_enabled {
        return Err(ApiError::not_found("video posters are disabled"));
    }
    let destination = thumb::cache_path(
        &state.config.thumb_dir(),
        &item.cache_key(),
        size,
        &state.config.thumbnails.format,
    );

    if !destination.is_file() {
        if let Some(sidecar) = sidecar_path(&state, &item, size) {
            if sidecar.is_file() {
                install_sidecar(sidecar, destination.clone()).await?;
            }
        }
    }

    if !destination.is_file() {
        let source = media_path(&state, &item)?;
        let permit = state
            .thumb_sem
            .clone()
            .acquire_owned()
            .await
            .map_err(ApiError::internal)?;

        let item = item.clone();
        let destination = destination.clone();
        let source = source.clone();
        let thumbs = state.config.thumbnails.clone();
        let ffmpeg = state.config.ffmpeg.clone();

        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            thumb::ensure_thumbnail(&item, &source, &destination, size, &thumbs, &ffmpeg)
        })
        .await
        .map_err(ApiError::internal)??;
    }

    serve_file(destination, headers).await
}

async fn get_media(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let item = fetch_item(&state, &id).await?;
    let path = media_path(&state, &item)?;
    serve_file(path, headers).await
}

async fn scan_handler(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let job_id = start_scan(state, None).map_err(|error| {
        if error.to_string().contains("already") {
            ApiError::conflict(error.to_string())
        } else {
            ApiError::internal(error)
        }
    })?;
    Ok(Json(json!({ "job_id": job_id })))
}

async fn list_jobs(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let db = state.db.clone();
    let jobs = tokio::task::spawn_blocking(move || db.list_jobs(20))
        .await
        .map_err(ApiError::internal)??;
    Ok(Json(json!({ "jobs": jobs })))
}

async fn get_job(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let db = state.db.clone();
    let job = tokio::task::spawn_blocking(move || db.get_job(&id))
        .await
        .map_err(ApiError::internal)??;
    job.map(|job| Json(json!(job)))
        .ok_or_else(|| ApiError::not_found("job not found"))
}

async fn fetch_item(state: &AppState, id: &str) -> Result<MediaItem, ApiError> {
    let db = state.db.clone();
    let id = id.to_string();
    let item = tokio::task::spawn_blocking(move || db.get_item(&id))
        .await
        .map_err(ApiError::internal)??;
    item.ok_or_else(|| ApiError::not_found("item not found"))
}

fn media_path(state: &AppState, item: &MediaItem) -> Result<PathBuf, ApiError> {
    let library = state
        .config
        .libraries
        .iter()
        .find(|library| library.id == item.library_id)
        .ok_or_else(|| ApiError::not_found("library not found"))?;

    let root = PathBuf::from(&library.path)
        .canonicalize()
        .map_err(|_| ApiError::not_found("library root not found"))?;
    let candidate = root.join(&item.rel_path);
    let canonical = candidate
        .canonicalize()
        .map_err(|_| ApiError::not_found("media file not found"))?;

    if !canonical.starts_with(&root) {
        return Err(ApiError::forbidden("path escapes library root"));
    }
    Ok(canonical)
}

async fn serve_file(path: PathBuf, headers: HeaderMap) -> Result<Response, ApiError> {
    if !path.is_file() {
        return Err(ApiError::not_found("file not found"));
    }

    let mut builder = Request::builder().method(Method::GET).uri("/");
    {
        let out = builder
            .headers_mut()
            .ok_or_else(|| ApiError::internal("failed to build request"))?;
        for name in [
            header::RANGE,
            header::IF_MODIFIED_SINCE,
            header::IF_NONE_MATCH,
            header::ACCEPT_ENCODING,
        ] {
            if let Some(value) = headers.get(&name) {
                out.insert(name, value.clone());
            }
        }
    }

    let request = builder.body(Body::empty()).map_err(ApiError::internal)?;
    let response = ServeFile::new(path)
        .oneshot(request)
        .await
        .map_err(ApiError::internal)?;
    let (parts, body) = response.into_parts();
    Ok(Response::from_parts(parts, Body::new(body)))
}

fn item_dto(state: &AppState, item: &MediaItem) -> Value {
    let size = state
        .config
        .thumbnails
        .sizes
        .first()
        .copied()
        .unwrap_or(320);
    let preview_size = state
        .config
        .thumbnails
        .sizes
        .iter()
        .max()
        .copied()
        .unwrap_or(size);

    let thumbnail_url = thumb_url(state, item, size);
    let media_url = media_url(state, item);
    // Images are shown in the lightbox through a large JPEG thumbnail so that
    // formats the browser cannot decode (e.g. HEIC) still display when a
    // sidecar preview exists. Videos keep the original media URL.
    let preview_url = if item.kind == MediaKind::Image {
        thumb_url(state, item, preview_size)
    } else {
        media_url.clone()
    };

    json!({
        "id": item.id,
        "library_id": item.library_id,
        "kind": item.kind.as_str(),
        "filename": item.filename(),
        "rel_path": item.rel_path,
        "size": item.size,
        "mtime": item.mtime,
        "width": item.width,
        "height": item.height,
        "duration_ms": item.duration_ms,
        "taken_at": item.taken_at,
        "camera": item.camera,
        "favorite": item.favorite,
        "thumb_url": thumbnail_url,
        "media_url": media_url,
        "preview_url": preview_url,
    })
}

fn thumb_url(state: &AppState, item: &MediaItem, size: u32) -> String {
    if state.config.media.serve_mode == "proxy" {
        let key = item.cache_key();
        let a = key.get(0..2).unwrap_or("00");
        let b = key.get(2..4).unwrap_or("00");
        let thumb_base = state.config.media.public_thumb_path.trim_end_matches('/');
        let format = if state.config.thumbnails.format.eq_ignore_ascii_case("webp") {
            "webp"
        } else {
            "jpg"
        };
        format!("{thumb_base}/{a}/{b}/{key}-{size}.{format}")
    } else {
        format!("/api/v1/items/{}/thumb/{size}", item.id)
    }
}

fn media_url(state: &AppState, item: &MediaItem) -> String {
    if state.config.media.serve_mode == "proxy" {
        let media_base = state.config.media.public_media_path.trim_end_matches('/');
        format!(
            "{media_base}/{}/{}",
            encode_rel_path(&item.library_id),
            encode_rel_path(&item.rel_path)
        )
    } else {
        format!("/api/v1/items/{}/media", item.id)
    }
}

fn sidecar_path(state: &AppState, item: &MediaItem, size: u32) -> Option<PathBuf> {
    let dir = state.config.thumbnails.sidecar_dir.as_ref()?;
    let library = Path::new(&item.library_id);
    let rel = Path::new(&item.rel_path);
    if library.is_absolute()
        || rel.is_absolute()
        || library
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || rel
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    Some(
        PathBuf::from(dir)
            .join(library)
            .join(format!("{}.{size}.jpg", item.rel_path)),
    )
}

async fn install_sidecar(source: PathBuf, destination: PathBuf) -> Result<(), ApiError> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        if destination.is_file() {
            return Ok(());
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = destination.with_file_name(format!(".sidecar-tmp-{}.jpg", std::process::id()));
        std::fs::copy(&source, &tmp)?;
        std::fs::rename(&tmp, &destination)?;
        Ok(())
    })
    .await
    .map_err(ApiError::internal)??;
    Ok(())
}

fn encode_rel_path(rel_path: &str) -> String {
    rel_path
        .split('/')
        .map(|segment| utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn start_scan(state: AppState, library_id: Option<String>) -> anyhow::Result<String> {
    if state
        .scanning
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        anyhow::bail!("a scan is already running");
    }

    let job_id = match state.db.create_job("scan") {
        Ok(id) => id,
        Err(error) => {
            state.scanning.store(false, Ordering::SeqCst);
            return Err(error);
        }
    };

    let config = state.config.clone();
    let db = state.db.clone();
    let scanning = state.scanning.clone();
    let job = job_id.clone();

    tokio::spawn(async move {
        let scan_db = db.clone();
        let scan_job = job.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<ScanStats> {
            let libraries: Vec<Library> = config
                .libraries
                .iter()
                .filter(|library| library.enabled)
                .filter(|library| {
                    library_id
                        .as_ref()
                        .map(|id| id == &library.id)
                        .unwrap_or(true)
                })
                .map(|library| library.to_library())
                .collect();

            if libraries.is_empty() {
                anyhow::bail!("no matching libraries to scan");
            }

            let mut total = ScanStats::default();
            for library in libraries {
                let progress_db = scan_db.clone();
                let progress_job = scan_job.clone();
                let progress = move |done: usize, total: usize| {
                    let _ = progress_db.update_job(
                        &progress_job,
                        "running",
                        done as i64,
                        total as i64,
                        Some("scanning"),
                        None,
                    );
                };
                let progress_ref: &dyn Fn(usize, usize) = &progress;
                let stats = photoself_core::scan::scan_library(
                    &config,
                    &scan_db,
                    &library,
                    Some(progress_ref),
                )?;
                total.scanned += stats.scanned;
                total.errors += stats.errors;
                total.missing += stats.missing;
            }
            Ok(total)
        })
        .await;

        scanning.store(false, Ordering::SeqCst);

        match result {
            Ok(Ok(stats)) => {
                let message = format!(
                    "scanned {}, errors {}, missing {}",
                    stats.scanned, stats.errors, stats.missing
                );
                let _ = db.update_job(&job, "completed", 100, 100, Some(&message), None);
                tracing::info!(job = %job, %message, "scan completed");
            }
            Ok(Err(error)) => {
                let message = error.to_string();
                let _ = db.update_job(&job, "failed", 0, 0, Some("scan failed"), Some(&message));
                tracing::error!(job = %job, %error, "scan failed");
            }
            Err(error) => {
                let message = error.to_string();
                let _ = db.update_job(
                    &job,
                    "failed",
                    0,
                    0,
                    Some("scan task failed"),
                    Some(&message),
                );
                tracing::error!(job = %job, %error, "scan task failed");
            }
        }
    });

    Ok(job_id)
}
