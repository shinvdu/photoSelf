use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path as AxumPath, Query, State};
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use photoself_core::model::{
    ItemQuery, Library, MediaItem, MediaKind, ScanStats, StoredLibrary, TrashEntry,
};
use photoself_core::{scan, thumb, util, video};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
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
        .route(
            "/api/v1/items/{id}",
            get(get_item).patch(patch_item).delete(delete_item),
        )
        .route("/api/v1/items/{id}/thumb/{size}", get(get_thumb))
        .route("/api/v1/items/{id}/media", get(get_media))
        .route("/api/v1/items/{id}/derived", get(get_derived))
        .route("/api/v1/items/{id}/transcode", post(transcode_item))
        .route("/api/v1/items/{id}/tags", post(add_item_tag))
        .route("/api/v1/items/{id}/tags/{tag_id}", delete(remove_item_tag))
        .route("/api/v1/tags", get(list_tags).post(create_tag))
        .route("/api/v1/tags/{id}", delete(delete_tag))
        .route("/api/v1/albums", get(list_albums).post(create_album))
        .route("/api/v1/albums/{id}", get(get_album).delete(delete_album))
        .route(
            "/api/v1/albums/{id}/items",
            get(album_items).post(add_album_item),
        )
        .route(
            "/api/v1/albums/{id}/items/{item_id}",
            delete(remove_album_item),
        )
        .route(
            "/api/v1/upload",
            post(upload).layer(DefaultBodyLimit::max(
                (state.config.write.max_upload_bytes as usize).saturating_add(1024 * 1024),
            )),
        )
        .route("/api/v1/trash", get(list_trash))
        .route("/api/v1/trash/{id}/restore", post(restore_trash))
        .route("/api/v1/trash/{id}", delete(purge_trash))
        .route("/api/v1/scan", post(scan_handler))
        .route(
            "/api/v1/libraries",
            get(list_libraries).post(create_library),
        )
        .route(
            "/api/v1/libraries/{id}",
            patch(update_library).delete(delete_library),
        )
        .route("/api/v1/libraries/{id}/scan", post(scan_library))
        .route(
            "/api/v1/libraries/import-config",
            post(import_config_libraries),
        )
        .route("/api/v1/fs/dirs", get(browse_dirs))
        .route("/api/v1/jobs", get(list_jobs))
        .route("/api/v1/jobs/{id}", get(get_job))
        .layer(middleware::from_fn(csrf_guard))
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

async fn csrf_guard(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let method = request.method();
    let needs_guard = method == Method::POST
        || method == Method::PUT
        || method == Method::PATCH
        || method == Method::DELETE;
    if needs_guard
        && request
            .headers()
            .get("x-requested-with")
            .and_then(|value| value.to_str().ok())
            != Some("photoSelf")
    {
        return (StatusCode::FORBIDDEN, "missing X-Requested-With: photoSelf").into_response();
    }
    next.run(request).await
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
        .libraries_snapshot()
        .into_iter()
        .filter(|library| library.enabled)
        .map(|library| {
            json!({
                "id": library.id,
                "name": library.name,
                "read_only": library.read_only,
            })
        })
        .collect();

    let manage_available = state.config.manage.enabled
        && (!state.config.manage.require_auth || state.config.auth.mode != "none");

    Ok(Json(json!({
        "name": "photoSelf",
        "version": env!("CARGO_PKG_VERSION"),
        "mode": state.config.server.mode,
        "auth_mode": state.config.auth.mode,
        "item_count": count,
        "thumb_sizes": state.config.thumbnails.sizes,
        "write_enabled": state.config.write.enabled,
        "trash_enabled": state.config.trash.enabled,
        "transcode_enabled": state.config.video.transcode_enabled,
        "manage_enabled": manage_available,
        "libraries": libraries,
    })))
}

#[derive(Debug, Deserialize)]
struct ListParams {
    library: Option<String>,
    kind: Option<String>,
    q: Option<String>,
    favorite: Option<bool>,
    tag: Option<String>,
    album: Option<String>,
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
        tag: params.tag,
        album_id: params.album,
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

fn manage_allowed(state: &AppState) -> Result<(), ApiError> {
    if !state.config.manage.enabled {
        return Err(ApiError::forbidden("library management is disabled"));
    }
    if state.config.manage.require_auth && state.config.auth.mode == "none" {
        return Err(ApiError::forbidden(
            "library management requires authentication",
        ));
    }
    Ok(())
}

fn protected_dirs(config: &photoself_core::Config) -> Vec<PathBuf> {
    let mut protected = Vec::new();
    for dir in [config.data_dir(), config.thumb_dir(), config.trash_dir()] {
        if let Ok(path) = std::fs::canonicalize(&dir) {
            protected.push(path);
        }
    }
    if let Some(sidecar) = config.thumbnails.sidecar_dir.as_ref() {
        if let Ok(path) = std::fs::canonicalize(sidecar) {
            protected.push(path);
        }
    }
    protected
}

fn library_json(row: &StoredLibrary, counts: &HashMap<String, i64>) -> Value {
    json!({
        "id": row.library.id,
        "name": row.library.name,
        "path": row.library.root_path.to_string_lossy(),
        "read_only": row.library.read_only,
        "enabled": row.library.enabled,
        "ignore_globs": row.library.ignore_globs,
        "source": row.source,
        "item_count": counts.get(&row.library.id).copied().unwrap_or(0),
    })
}

async fn list_libraries(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    manage_allowed(&state)?;
    let db = state.db.clone();
    let (rows, counts) = tokio::task::spawn_blocking(
        move || -> anyhow::Result<(Vec<StoredLibrary>, HashMap<String, i64>)> {
            Ok((db.list_libraries()?, db.library_item_counts()?))
        },
    )
    .await
    .map_err(ApiError::internal)??;
    let libraries: Vec<Value> = rows.iter().map(|row| library_json(row, &counts)).collect();
    Ok(Json(json!({ "libraries": libraries })))
}

#[derive(Debug, Deserialize)]
struct CreateLibraryRequest {
    name: Option<String>,
    path: String,
    read_only: Option<bool>,
    ignore_globs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct UpdateLibraryRequest {
    name: Option<String>,
    read_only: Option<bool>,
    enabled: Option<bool>,
    ignore_globs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct DirsQuery {
    path: Option<String>,
}

fn clean_ignore_globs(globs: Option<Vec<String>>) -> Result<Vec<String>, ApiError> {
    let list =
        globs.unwrap_or_else(|| photoself_core::config::LibraryConfig::default().ignore_globs);
    if list.len() > 50 {
        return Err(ApiError::bad_request("too many ignore globs"));
    }
    let mut cleaned = Vec::new();
    for glob in list {
        let glob = glob.trim();
        if glob.is_empty() {
            continue;
        }
        if glob.chars().count() > 200 {
            return Err(ApiError::bad_request("ignore glob is too long"));
        }
        cleaned.push(glob.to_string());
    }
    Ok(cleaned)
}

fn clean_library_name(name: Option<&str>, path: &Path, id: &str) -> Result<String, ApiError> {
    let candidate = name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            path.file_name()
                .map(|component| component.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| id.to_string());
    if candidate.chars().count() > 100 {
        return Err(ApiError::bad_request("library name is too long"));
    }
    Ok(candidate)
}

/// Resolve and validate a path before adding it as a library. All filesystem
/// work happens on the blocking pool with a timeout so a stale network mount
/// cannot wedge the API.
async fn validate_library_path(state: &AppState, raw: &str) -> Result<PathBuf, ApiError> {
    let raw = raw.trim().to_string();
    if raw.is_empty() {
        return Err(ApiError::bad_request("path is required"));
    }
    if !Path::new(&raw).is_absolute() {
        return Err(ApiError::bad_request("path must be absolute"));
    }
    let roots = state.manage_roots.clone();
    let libraries = state.libraries_snapshot();
    let protected = protected_dirs(&state.config);
    let task = tokio::task::spawn_blocking(move || -> anyhow::Result<PathBuf> {
        let path =
            std::fs::canonicalize(&raw).with_context(|| format!("cannot resolve path {raw}"))?;
        let metadata =
            std::fs::metadata(&path).with_context(|| format!("cannot stat {}", path.display()))?;
        if !metadata.is_dir() {
            anyhow::bail!("path is not a directory");
        }
        if roots.is_empty() {
            anyhow::bail!("no manage.allowed_roots configured");
        }
        if !roots.iter().any(|root| path.starts_with(root)) {
            anyhow::bail!("path is outside manage.allowed_roots");
        }
        for protected_dir in &protected {
            if path == *protected_dir || path.starts_with(protected_dir) {
                anyhow::bail!("path overlaps photoSelf data directories");
            }
        }
        for library in &libraries {
            if let Ok(existing) = std::fs::canonicalize(&library.root_path) {
                if path == existing {
                    anyhow::bail!("path already belongs to library '{}'", library.id);
                }
                if path.starts_with(&existing) || existing.starts_with(&path) {
                    anyhow::bail!("path overlaps library '{}'", library.id);
                }
            }
        }
        std::fs::read_dir(&path).with_context(|| format!("{} is not readable", path.display()))?;
        Ok(path)
    });
    match tokio::time::timeout(std::time::Duration::from_secs(5), task).await {
        Err(_) => Err(ApiError::bad_request("path check timed out")),
        Ok(joined) => joined
            .map_err(ApiError::internal)?
            .map_err(|error| ApiError::bad_request(error.to_string())),
    }
}

async fn create_library(
    State(state): State<AppState>,
    Json(payload): Json<CreateLibraryRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    manage_allowed(&state)?;
    let path = validate_library_path(&state, &payload.path).await?;
    let id = format!("lib-{}", &uuid::Uuid::now_v7().simple().to_string()[..12]);
    let name = clean_library_name(payload.name.as_deref(), &path, &id)?;
    let library = Library {
        id: id.clone(),
        name,
        root_path: path,
        read_only: payload.read_only.unwrap_or(true),
        enabled: true,
        ignore_globs: clean_ignore_globs(payload.ignore_globs)?,
    };
    let db = state.db.clone();
    let insert = library.clone();
    tokio::task::spawn_blocking(move || db.insert_library(&insert, "ui"))
        .await
        .map_err(ApiError::internal)??;
    state.reload_libraries();
    let scan_job = start_scan(state.clone(), Some(id)).ok();
    let row = StoredLibrary {
        library,
        source: "ui".to_string(),
    };
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "library": library_json(&row, &HashMap::new()),
            "scan_job": scan_job,
        })),
    ))
}

async fn update_library(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(payload): Json<UpdateLibraryRequest>,
) -> Result<Json<Value>, ApiError> {
    manage_allowed(&state)?;
    let mut library = state
        .library(&id)
        .ok_or_else(|| ApiError::not_found("library not found"))?;
    if let Some(name) = payload.name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(ApiError::bad_request("library name must not be empty"));
        }
        if trimmed.chars().count() > 100 {
            return Err(ApiError::bad_request("library name is too long"));
        }
        library.name = trimmed.to_string();
    }
    if let Some(read_only) = payload.read_only {
        library.read_only = read_only;
    }
    if let Some(enabled) = payload.enabled {
        library.enabled = enabled;
    }
    if payload.ignore_globs.is_some() {
        library.ignore_globs = clean_ignore_globs(payload.ignore_globs)?;
    }
    let db = state.db.clone();
    let updated = library.clone();
    tokio::task::spawn_blocking(move || db.update_library(&updated))
        .await
        .map_err(ApiError::internal)??;
    state.reload_libraries();
    let db = state.db.clone();
    let id = id.clone();
    let (row, counts) = tokio::task::spawn_blocking(
        move || -> anyhow::Result<(Option<StoredLibrary>, HashMap<String, i64>)> {
            Ok((db.get_library(&id)?, db.library_item_counts()?))
        },
    )
    .await
    .map_err(ApiError::internal)??;
    let row = row.ok_or_else(|| ApiError::not_found("library not found"))?;
    Ok(Json(library_json(&row, &counts)))
}

async fn delete_library(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    manage_allowed(&state)?;
    if state.scanning.load(Ordering::SeqCst) {
        return Err(ApiError::conflict(
            "a scan is running; try again after it finishes",
        ));
    }
    state
        .library(&id)
        .ok_or_else(|| ApiError::not_found("library not found"))?;
    let db = state.db.clone();
    let id = id.clone();
    let removed = tokio::task::spawn_blocking(move || db.delete_library(&id))
        .await
        .map_err(ApiError::internal)??;
    state.reload_libraries();
    Ok(Json(json!({ "removed": true, "removed_items": removed })))
}

async fn scan_library(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    manage_allowed(&state)?;
    state
        .library(&id)
        .ok_or_else(|| ApiError::not_found("library not found"))?;
    let job = start_scan(state, Some(id)).map_err(|error| {
        let message = error.to_string();
        if message.contains("already running") {
            ApiError::conflict(message)
        } else {
            ApiError::internal(error)
        }
    })?;
    Ok(Json(json!({ "job_id": job })))
}

async fn import_config_libraries(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    manage_allowed(&state)?;
    let libraries = state.config.libraries.clone();
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || db.import_config_libraries(&libraries))
        .await
        .map_err(ApiError::internal)??;
    state.reload_libraries();
    Ok(Json(json!({
        "imported": state.libraries_snapshot().len(),
    })))
}

async fn browse_dirs(
    State(state): State<AppState>,
    Query(query): Query<DirsQuery>,
) -> Result<Json<Value>, ApiError> {
    manage_allowed(&state)?;
    let roots = state.manage_roots.clone();
    if roots.is_empty() {
        return Err(ApiError::bad_request("no manage.allowed_roots configured"));
    }
    let raw = query.path.unwrap_or_default();
    if raw.trim().is_empty() {
        let entries: Vec<Value> = roots
            .iter()
            .map(|root| {
                json!({
                    "name": root
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| root.to_string_lossy().to_string()),
                    "path": root.to_string_lossy(),
                })
            })
            .collect();
        return Ok(Json(json!({
            "path": Value::Null,
            "parent": Value::Null,
            "entries": entries,
        })));
    }

    let roots_for_base = roots.clone();
    let raw_for_base = raw.clone();
    let base = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::task::spawn_blocking(move || -> anyhow::Result<PathBuf> {
            let path = std::fs::canonicalize(&raw_for_base)?;
            if !roots_for_base.iter().any(|root| path.starts_with(root)) {
                anyhow::bail!("path is outside manage.allowed_roots");
            }
            if !path.is_dir() {
                anyhow::bail!("path is not a directory");
            }
            Ok(path)
        }),
    )
    .await
    .map_err(|_| ApiError::bad_request("directory listing timed out"))?
    .map_err(ApiError::internal)?
    .map_err(|error| ApiError::bad_request(error.to_string()))?;

    let roots_for_entries = roots.clone();
    let protected_for_entries = protected_dirs(&state.config);
    let base_for_entries = base.clone();
    let entries = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<Value>> {
            let mut entries = Vec::new();
            for entry in std::fs::read_dir(&base_for_entries)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let canonical = match std::fs::canonicalize(entry.path()) {
                    Ok(path) => path,
                    Err(_) => continue,
                };
                if !roots_for_entries
                    .iter()
                    .any(|root| canonical.starts_with(root))
                {
                    continue;
                }
                if protected_for_entries
                    .iter()
                    .any(|dir| canonical == *dir || canonical.starts_with(dir))
                {
                    continue;
                }
                match std::fs::metadata(&canonical) {
                    Ok(metadata) if metadata.is_dir() => {}
                    _ => continue,
                }
                entries.push(json!({
                    "name": name,
                    "path": canonical.to_string_lossy(),
                }));
            }
            entries.sort_by(|left, right| {
                left["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_lowercase()
                    .cmp(&right["name"].as_str().unwrap_or_default().to_lowercase())
            });
            Ok(entries)
        }),
    )
    .await
    .map_err(|_| ApiError::bad_request("directory listing timed out"))?
    .map_err(ApiError::internal)?
    .map_err(|error| ApiError::bad_request(error.to_string()))?;

    let parent = if roots.iter().any(|root| root == &base) {
        Value::Null
    } else {
        base.parent()
            .filter(|parent| roots.iter().any(|root| parent.starts_with(root)))
            .map(|parent| Value::String(parent.to_string_lossy().to_string()))
            .unwrap_or(Value::Null)
    };
    Ok(Json(json!({
        "path": base.to_string_lossy(),
        "parent": parent,
        "entries": entries,
    })))
}

fn write_allowed(state: &AppState) -> Result<(), ApiError> {
    if !state.config.write.enabled {
        return Err(ApiError::forbidden("write mode is disabled"));
    }
    Ok(())
}

fn library_config(state: &AppState, id: &str) -> Result<photoself_core::model::Library, ApiError> {
    state
        .library(id)
        .ok_or_else(|| ApiError::not_found("library not found"))
}

fn writable_library(
    state: &AppState,
    id: &str,
) -> Result<photoself_core::model::Library, ApiError> {
    write_allowed(state)?;
    let library = library_config(state, id)?;
    if library.read_only {
        return Err(ApiError::forbidden("library is read-only"));
    }
    Ok(library)
}

fn cover_url(state: &AppState, album: &photoself_core::model::Album) -> Option<String> {
    let item = state.db.get_item(album.cover_item_id.as_deref()?).ok()??;
    let size = state
        .config
        .thumbnails
        .sizes
        .first()
        .copied()
        .unwrap_or(320);
    Some(thumb_url(state, &item, size))
}

fn album_json(state: &AppState, album: &photoself_core::model::Album) -> Value {
    json!({
        "id": album.id,
        "name": album.name,
        "description": album.description,
        "cover_item_id": album.cover_item_id,
        "cover_url": cover_url(state, album),
        "item_count": album.item_count,
        "created_at": album.created_at,
    })
}

async fn list_tags(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let db = state.db.clone();
    let tags = tokio::task::spawn_blocking(move || db.list_tags())
        .await
        .map_err(ApiError::internal)??;
    Ok(Json(json!({ "tags": tags })))
}

#[derive(Debug, Deserialize)]
struct CreateTag {
    name: String,
    color: Option<String>,
}

async fn create_tag(
    State(state): State<AppState>,
    Json(body): Json<CreateTag>,
) -> Result<Json<Value>, ApiError> {
    let db = state.db.clone();
    let tag = tokio::task::spawn_blocking(move || db.create_tag(&body.name, body.color.as_deref()))
        .await
        .map_err(ApiError::internal)??;
    Ok(Json(json!(tag)))
}

async fn delete_tag(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<StatusCode, ApiError> {
    let db = state.db.clone();
    let removed = tokio::task::spawn_blocking(move || db.delete_tag(id))
        .await
        .map_err(ApiError::internal)??;
    if !removed {
        return Err(ApiError::not_found("tag not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct TagLink {
    tag_id: i64,
}

async fn add_item_tag(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<TagLink>,
) -> Result<Json<Value>, ApiError> {
    let _ = fetch_item(&state, &id).await?;
    let db = state.db.clone();
    let tag_id = body.tag_id;
    let exists = tokio::task::spawn_blocking(move || db.tag_exists(tag_id))
        .await
        .map_err(ApiError::internal)??;
    if !exists {
        return Err(ApiError::not_found("tag not found"));
    }
    let db = state.db.clone();
    let item_id = id.clone();
    tokio::task::spawn_blocking(move || db.add_item_tag(&item_id, tag_id))
        .await
        .map_err(ApiError::internal)??;
    let item = fetch_item(&state, &id).await?;
    Ok(Json(item_dto(&state, &item)))
}

async fn remove_item_tag(
    State(state): State<AppState>,
    AxumPath((id, tag_id)): AxumPath<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
    let _ = fetch_item(&state, &id).await?;
    let db = state.db.clone();
    let exists = tokio::task::spawn_blocking(move || db.tag_exists(tag_id))
        .await
        .map_err(ApiError::internal)??;
    if !exists {
        return Err(ApiError::not_found("tag not found"));
    }
    let db = state.db.clone();
    let item_id = id.clone();
    tokio::task::spawn_blocking(move || db.remove_item_tag(&item_id, tag_id))
        .await
        .map_err(ApiError::internal)??;
    let item = fetch_item(&state, &id).await?;
    Ok(Json(item_dto(&state, &item)))
}

async fn list_albums(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let db = state.db.clone();
    let albums = tokio::task::spawn_blocking(move || db.list_albums())
        .await
        .map_err(ApiError::internal)??;
    let albums: Vec<Value> = albums
        .iter()
        .map(|album| album_json(&state, album))
        .collect();
    Ok(Json(json!({ "albums": albums })))
}

#[derive(Debug, Deserialize)]
struct CreateAlbum {
    name: String,
    description: Option<String>,
}

async fn create_album(
    State(state): State<AppState>,
    Json(body): Json<CreateAlbum>,
) -> Result<Json<Value>, ApiError> {
    let db = state.db.clone();
    let album = tokio::task::spawn_blocking(move || {
        db.create_album(&body.name, body.description.as_deref())
    })
    .await
    .map_err(ApiError::internal)??;
    Ok(Json(album_json(&state, &album)))
}

async fn get_album(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let db = state.db.clone();
    let album = tokio::task::spawn_blocking(move || db.get_album(&id))
        .await
        .map_err(ApiError::internal)??;
    album
        .map(|album| Json(album_json(&state, &album)))
        .ok_or_else(|| ApiError::not_found("album not found"))
}

async fn delete_album(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<StatusCode, ApiError> {
    let db = state.db.clone();
    let removed = tokio::task::spawn_blocking(move || db.delete_album(&id))
        .await
        .map_err(ApiError::internal)??;
    if !removed {
        return Err(ApiError::not_found("album not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn album_items(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, ApiError> {
    let db = state.db.clone();
    let album_id = id.clone();
    let exists = tokio::task::spawn_blocking(move || db.get_album(&album_id))
        .await
        .map_err(ApiError::internal)??
        .is_some();
    if !exists {
        return Err(ApiError::not_found("album not found"));
    }
    let query = ItemQuery {
        library_id: params.library,
        kind: params.kind,
        q: params.q,
        favorite: params.favorite,
        tag: params.tag,
        album_id: Some(id),
        from: params.from,
        to: params.to,
        cursor: params.cursor,
        limit: params.limit.unwrap_or(200),
    };
    let db = state.db.clone();
    let (items, next_cursor) = tokio::task::spawn_blocking(move || db.list_items(&query))
        .await
        .map_err(ApiError::internal)??;
    let items: Vec<Value> = items.iter().map(|item| item_dto(&state, item)).collect();
    Ok(Json(json!({ "items": items, "next_cursor": next_cursor })))
}

#[derive(Debug, Deserialize)]
struct AlbumItemLink {
    item_id: String,
}

async fn add_album_item(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<AlbumItemLink>,
) -> Result<StatusCode, ApiError> {
    let _ = fetch_item(&state, &body.item_id).await?;
    let db = state.db.clone();
    let check_db = db.clone();
    let check_id = id.clone();
    let exists = tokio::task::spawn_blocking(move || check_db.get_album(&check_id))
        .await
        .map_err(ApiError::internal)??
        .is_some();
    if !exists {
        return Err(ApiError::not_found("album not found"));
    }
    let album_id = id.clone();
    let item_id = body.item_id;
    tokio::task::spawn_blocking(move || db.add_album_item(&album_id, &item_id))
        .await
        .map_err(ApiError::internal)??;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_album_item(
    State(state): State<AppState>,
    AxumPath((id, item_id)): AxumPath<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let db = state.db.clone();
    let check_id = id.clone();
    let exists = tokio::task::spawn_blocking(move || db.get_album(&check_id))
        .await
        .map_err(ApiError::internal)??
        .is_some();
    if !exists {
        return Err(ApiError::not_found("album not found"));
    }
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || db.remove_album_item(&id, &item_id))
        .await
        .map_err(ApiError::internal)??;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct UploadParams {
    overwrite: Option<bool>,
}

async fn upload(
    State(state): State<AppState>,
    Query(params): Query<UploadParams>,
    mut multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    write_enabled_check(&state)?;

    let mut library_id: Option<String> = None;
    let mut subdir: Option<String> = None;
    let mut file_name: Option<String> = None;
    let mut temp: Option<PathBuf> = None;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?
    {
        match field.name() {
            Some("library") => {
                library_id = Some(
                    field
                        .text()
                        .await
                        .map_err(|error| ApiError::bad_request(error.to_string()))?,
                );
            }
            Some("subdir") => {
                subdir = Some(
                    field
                        .text()
                        .await
                        .map_err(|error| ApiError::bad_request(error.to_string()))?,
                );
            }
            Some("file") => {
                file_name = field.file_name().map(ToOwned::to_owned);
                let tmp = state
                    .config
                    .tmp_dir()
                    .join(format!(".upload-{}", uuid::Uuid::now_v7()));
                tokio::fs::create_dir_all(state.config.tmp_dir())
                    .await
                    .map_err(ApiError::internal)?;
                let mut out = tokio::fs::File::create(&tmp)
                    .await
                    .map_err(ApiError::internal)?;
                let mut written: u64 = 0;
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|error| ApiError::bad_request(error.to_string()))?
                {
                    written += chunk.len() as u64;
                    if written > state.config.write.max_upload_bytes {
                        let _ = tokio::fs::remove_file(&tmp).await;
                        return Err(ApiError::bad_request("upload exceeds max_upload_bytes"));
                    }
                    out.write_all(&chunk).await.map_err(ApiError::internal)?;
                }
                out.flush().await.map_err(ApiError::internal)?;
                temp = Some(tmp);
            }
            _ => {}
        }
    }

    let library = writable_library(
        &state,
        library_id
            .as_deref()
            .ok_or_else(|| ApiError::bad_request("missing multipart field: library"))?,
    )?;
    let file_name = file_name
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("missing multipart file"))?;
    let temp = temp.ok_or_else(|| ApiError::bad_request("empty upload"))?;

    let safe_name = sanitize_file_name(file_name)?;
    let mut rel_path = safe_name.clone();
    if let Some(subdir) = subdir.as_deref().filter(|value| !value.trim().is_empty()) {
        let clean = sanitize_subdir(subdir)?;
        rel_path = format!("{clean}/{safe_name}");
    }
    if util::is_supported_path(Path::new(&rel_path)).is_none() {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(ApiError::bad_request("unsupported media extension"));
    }

    let root = library.root_path.clone();
    let target = root.join(&rel_path);
    if target.exists() && !params.overwrite.unwrap_or(false) {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(ApiError::conflict("file already exists"));
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(ApiError::internal)?;
        // A subdir may be an existing symlink pointing outside the library.
        let root_canon = tokio::fs::canonicalize(&root)
            .await
            .map_err(ApiError::internal)?;
        let parent_canon = tokio::fs::canonicalize(parent)
            .await
            .map_err(ApiError::internal)?;
        if !parent_canon.starts_with(&root_canon) {
            let _ = tokio::fs::remove_file(&temp).await;
            return Err(ApiError::forbidden("upload path escapes library root"));
        }
    }

    let staging = root.join(format!(".upload-staging-{}", uuid::Uuid::now_v7()));
    if let Err(error) = tokio::fs::copy(&temp, &staging).await {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(ApiError::internal(error));
    }
    let _ = tokio::fs::remove_file(&temp).await;
    // rename() replaces an existing target atomically on Linux; never delete
    // the old file before the new one is in place.
    if let Err(error) = tokio::fs::rename(&staging, &target).await {
        let _ = tokio::fs::remove_file(&staging).await;
        return Err(ApiError::internal(error));
    }

    let config = state.config.clone();
    let db = state.db.clone();
    let scan_path = target.clone();
    let scan_rel = rel_path.clone();
    let lib = library;
    let item = tokio::task::spawn_blocking(move || {
        scan::scan_file(&config, &db, &lib, &scan_path, &scan_rel)
    })
    .await
    .map_err(ApiError::internal)??;
    Ok(Json(item_dto(&state, &item)))
}

fn write_enabled_check(state: &AppState) -> Result<(), ApiError> {
    write_allowed(state)
}

fn sanitize_file_name(name: &str) -> Result<String, ApiError> {
    let path = Path::new(name);
    if path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(ApiError::bad_request("invalid file name"));
    }
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." {
        return Err(ApiError::bad_request("invalid file name"));
    }
    Ok(name.to_string())
}

fn sanitize_subdir(value: &str) -> Result<String, ApiError> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ApiError::bad_request("invalid subdir"));
    }
    Ok(value.trim_matches('/').to_string())
}

async fn delete_item(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let item = fetch_item(&state, &id).await?;
    let library = writable_library(&state, &item.library_id)?;
    if !state.config.trash.enabled {
        return Err(ApiError::forbidden("trash is disabled"));
    }
    let source = media_path(&state, &item)?;
    let trash_id = uuid::Uuid::now_v7().to_string();
    let trash_dir = state.config.trash_dir().join(&library.id);
    tokio::fs::create_dir_all(&trash_dir)
        .await
        .map_err(ApiError::internal)?;
    let filename = item.filename().to_string();
    let trash_path = trash_dir.join(format!(
        "{}-{}",
        &trash_id[..8],
        filename.replace(['/', '\\'], "_")
    ));
    let deleted_at = util::mtime_to_utc(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_secs() as i64)
            .unwrap_or(0),
    );
    let entry = TrashEntry {
        id: trash_id.clone(),
        item_id: item.id.clone(),
        library_id: library.id.clone(),
        rel_path: item.rel_path.clone(),
        filename,
        trash_path: trash_path.to_string_lossy().to_string(),
        deleted_at,
    };

    // Record the trash entry before touching the file. If the move fails we
    // remove the row again; if the process dies in between, the file is still
    // intact at its original path and a later scan heals the item.
    let db = state.db.clone();
    let entry_for_db = entry.clone();
    tokio::task::spawn_blocking(move || db.add_trash(&entry_for_db))
        .await
        .map_err(ApiError::internal)??;

    let source_copy = source.clone();
    let trash_copy = trash_path.clone();
    let moved = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        std::fs::copy(&source_copy, &trash_copy)?;
        std::fs::remove_file(&source_copy)?;
        Ok(())
    })
    .await;

    if let Err(error) = moved {
        let db = state.db.clone();
        let id = trash_id.clone();
        let _ = tokio::task::spawn_blocking(move || db.delete_trash_row(&id)).await;
        return Err(ApiError::internal(error));
    }

    let db = state.db.clone();
    let item_id = item.id.clone();
    tokio::task::spawn_blocking(move || db.set_missing(&item_id, true))
        .await
        .map_err(ApiError::internal)??;

    Ok(Json(json!({ "trash_id": trash_id })))
}

async fn list_trash(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    if !state.config.trash.enabled {
        return Ok(Json(json!({ "entries": [] })));
    }
    let db = state.db.clone();
    let entries = tokio::task::spawn_blocking(move || db.list_trash())
        .await
        .map_err(ApiError::internal)??;
    let entries: Vec<Value> = entries
        .iter()
        .map(|entry| {
            json!({
                "id": entry.id,
                "item_id": entry.item_id,
                "library_id": entry.library_id,
                "rel_path": entry.rel_path,
                "filename": entry.filename,
                "deleted_at": entry.deleted_at,
            })
        })
        .collect();
    Ok(Json(json!({ "entries": entries })))
}

async fn restore_trash(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<StatusCode, ApiError> {
    write_allowed(&state)?;
    let db = state.db.clone();
    let entry = tokio::task::spawn_blocking(move || db.get_trash(&id))
        .await
        .map_err(ApiError::internal)??
        .ok_or_else(|| ApiError::not_found("trash entry not found"))?;
    let library = library_config(&state, &entry.library_id)?;
    let target = library.root_path.join(&entry.rel_path);
    if target.exists() {
        return Err(ApiError::conflict("target path already exists"));
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(ApiError::internal)?;
    }
    let trash_path = PathBuf::from(&entry.trash_path);
    // Copy back first and keep the trash copy until the database transaction
    // commits. If the DB update fails, remove the restored target and keep the
    // trash entry intact so the operation can be retried.
    tokio::task::spawn_blocking({
        let trash_path = trash_path.clone();
        let target = target.clone();
        move || -> anyhow::Result<()> {
            std::fs::copy(&trash_path, &target)?;
            Ok(())
        }
    })
    .await
    .map_err(ApiError::internal)??;

    let db = state.db.clone();
    let item_id = entry.item_id.clone();
    let trash_id = entry.id.clone();
    let finished =
        tokio::task::spawn_blocking(move || db.complete_trash_restore(&item_id, &trash_id))
            .await
            .map_err(ApiError::internal)?;

    if let Err(error) = finished {
        let _ = tokio::fs::remove_file(&target).await;
        return Err(ApiError::internal(error));
    }

    if let Err(error) = tokio::fs::remove_file(&trash_path).await {
        tracing::warn!(path = %trash_path.display(), %error, "failed to remove restored trash copy");
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn purge_trash(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<StatusCode, ApiError> {
    write_allowed(&state)?;
    let db = state.db.clone();
    let entry = tokio::task::spawn_blocking(move || db.get_trash(&id))
        .await
        .map_err(ApiError::internal)??
        .ok_or_else(|| ApiError::not_found("trash entry not found"))?;
    let trash_path = PathBuf::from(&entry.trash_path);
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        if trash_path.exists() {
            std::fs::remove_file(&trash_path)?;
        }
        Ok(())
    })
    .await
    .map_err(ApiError::internal)??;
    let db = state.db.clone();
    let trash_id = entry.id.clone();
    tokio::task::spawn_blocking(move || db.delete_trash_row(&trash_id))
        .await
        .map_err(ApiError::internal)??;
    Ok(StatusCode::NO_CONTENT)
}

async fn transcode_item(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    if !state.config.video.transcode_enabled {
        return Err(ApiError::forbidden("video transcoding is disabled"));
    }
    let item = fetch_item(&state, &id).await?;
    if item.kind != MediaKind::Video {
        return Err(ApiError::bad_request("only videos can be transcoded"));
    }
    let output = state
        .config
        .cache_dir()
        .join("derived")
        .join(format!("{}-web.mp4", item.cache_key()));
    if output.is_file() {
        let db = state.db.clone();
        let item_id = item.id.clone();
        let path = output.to_string_lossy().to_string();
        tokio::task::spawn_blocking(move || db.set_derivative(&item_id, "web", &path))
            .await
            .map_err(ApiError::internal)??;
        return Ok(Json(json!({ "ready": true, "job_id": null })));
    }

    let source = media_path(&state, &item)?;
    let job_id = state.db.create_job("transcode")?;
    let db = state.db.clone();
    let config = state.config.clone();
    let sem = state.transcode_sem.clone();
    let job = job_id.clone();
    tokio::spawn(async move {
        let permit = match sem.acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => return,
        };
        let _permit = permit;
        let _ = db.update_job(&job, "running", 0, 1, Some("transcoding"), None);
        let item_id = item.id.clone();
        let output_for_task = output.clone();
        let result = tokio::task::spawn_blocking(move || {
            video::transcode(&source, &output_for_task, &config.ffmpeg, &config.video)
        })
        .await;
        match result {
            Ok(Ok(())) => {
                let path = output.to_string_lossy().to_string();
                let _ = db.set_derivative(&item_id, "web", &path);
                let _ = db.update_job(&job, "completed", 1, 1, Some("web version ready"), None);
            }
            Ok(Err(error)) => {
                let _ = db.update_job(
                    &job,
                    "failed",
                    0,
                    1,
                    Some("transcode failed"),
                    Some(&error.to_string()),
                );
            }
            Err(error) => {
                let _ = db.update_job(
                    &job,
                    "failed",
                    0,
                    1,
                    Some("transcode task failed"),
                    Some(&error.to_string()),
                );
            }
        }
    });
    Ok(Json(json!({ "ready": false, "job_id": job_id })))
}

async fn get_derived(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let item = fetch_item(&state, &id).await?;
    let db = state.db.clone();
    let item_id = item.id.clone();
    let path = tokio::task::spawn_blocking(move || db.get_derivative(&item_id, "web"))
        .await
        .map_err(ApiError::internal)??
        .ok_or_else(|| ApiError::not_found("web version not generated"))?;
    serve_file(PathBuf::from(path), headers).await
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
        .library(&item.library_id)
        .ok_or_else(|| ApiError::not_found("library not found"))?;

    let root = library
        .root_path
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
    let tags = state.db.tags_for_item(&item.id).unwrap_or_default();
    let play_url = if item.kind == MediaKind::Video {
        state
            .db
            .get_derivative(&item.id, "web")
            .ok()
            .flatten()
            .map(|_| format!("/api/v1/items/{}/derived", item.id))
    } else {
        None
    };
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
        "play_url": play_url,
        "tags": tags,
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
        let tmp = destination.with_file_name(format!(
            ".sidecar-tmp-{}-{}.jpg",
            std::process::id(),
            uuid::Uuid::now_v7()
        ));
        let result = (|| -> anyhow::Result<()> {
            std::fs::copy(&source, &tmp)?;
            std::fs::rename(&tmp, &destination)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
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
    let library_snapshot = state.libraries_snapshot();

    tokio::spawn(async move {
        let scan_db = db.clone();
        let scan_job = job.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<ScanStats> {
            let libraries: Vec<Library> = library_snapshot
                .into_iter()
                .filter(|library| library.enabled)
                .filter(|library| {
                    library_id
                        .as_ref()
                        .map(|id| id == &library.id)
                        .unwrap_or(true)
                })
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Method, Request as HttpRequest, StatusCode};
    use photoself_core::scan;
    use photoself_core::{Config, Db};
    use std::io::Write;
    use tower::ServiceExt;

    struct TestApp {
        app: Router,
        root: PathBuf,
        config: Config,
        db: Db,
    }

    impl Drop for TestApp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn setup(name: &str, transcode: bool) -> TestApp {
        let root =
            std::env::temp_dir().join(format!("photoself-it-{}-{}", name, uuid::Uuid::now_v7()));
        let photos = root.join("photos");
        let data = root.join("data");
        std::fs::create_dir_all(&photos).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let config_path = root.join("config.toml");
        let toml = format!(
            r#"
[server]
listen = "127.0.0.1:0"

[data]
dir = "{data}"

[[libraries]]
id = "lib"
name = "Test"
path = "{photos}"
read_only = false

[write]
enabled = true
max_upload_bytes = 10485760

[trash]
enabled = true

[thumbnails]
sizes = [320]
concurrency = 1

[ffmpeg]
enabled = {transcode}
ffmpeg_path = "ffmpeg"
ffprobe_path = "ffprobe"

[video]
transcode_enabled = {transcode}
transcode_concurrency = 1
max_height = 240
transcode_max_width = 320

[auth]
mode = "none"

[manage]
enabled = true
require_auth = false
allowed_roots = ["{root}"]

[scan]
on_start = false
interval = "0"
hash = true
probe_videos = true
"#,
            data = data.display(),
            photos = photos.display(),
            root = root.display(),
        );
        std::fs::write(&config_path, toml).unwrap();
        let config = Config::load(&config_path).unwrap();
        config.ensure_dirs().unwrap();
        let db = Db::open(&config.db_path()).unwrap();
        db.seed_config_libraries(&config.libraries).unwrap();
        let state = AppState::new(config.clone(), db.clone());
        let app = router(state);
        TestApp {
            app,
            root,
            config,
            db,
        }
    }

    fn upload_request(name: &str, data: &[u8], csrf: bool) -> HttpRequest<Body> {
        let boundary = "photoself-test-boundary";
        let mut body = Vec::new();
        write!(
            body,
            "--{boundary}\r\nContent-Disposition: form-data; name=\"library\"\r\n\r\nlib\r\n"
        )
        .unwrap();
        write!(
            body,
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: image/jpeg\r\n\r\n"
        )
        .unwrap();
        body.extend_from_slice(data);
        write!(body, "\r\n--{boundary}--\r\n").unwrap();

        let mut builder = HttpRequest::post("/api/v1/upload").header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        );
        if csrf {
            builder = builder.header("x-requested-with", "photoSelf");
        }
        builder.body(Body::from(body)).unwrap()
    }

    fn json_request(method: Method, path: &str, body: Value, csrf: bool) -> HttpRequest<Body> {
        let mut builder = HttpRequest::builder()
            .method(method)
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json");
        if csrf {
            builder = builder.header("x-requested-with", "photoSelf");
        }
        builder.body(Body::from(body.to_string())).unwrap()
    }

    async fn call(app: &Router, request: HttpRequest<Body>) -> (StatusCode, Vec<u8>) {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap()
            .to_vec();
        (status, bytes)
    }

    async fn call_json(app: &Router, request: HttpRequest<Body>) -> (StatusCode, Value) {
        let (status, bytes) = call(app, request).await;
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    async fn upload(app: &Router, name: &str) -> String {
        let (status, body) = call_json(app, upload_request(name, b"not-an-image", true)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["id"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn upload_delete_restore_and_csrf() {
        let fixture = setup("upload", false);
        let (status, _) = call(&fixture.app, upload_request("a.jpg", b"x", false)).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "write request without CSRF header"
        );

        let id = upload(&fixture.app, "a.jpg").await;
        assert!(fixture.root.join("photos/a.jpg").is_file());

        let (status, body) = call_json(
            &fixture.app,
            json_request(
                Method::DELETE,
                &format!("/api/v1/items/{id}"),
                Value::Null,
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(!fixture.root.join("photos/a.jpg").exists());

        let (status, trash) = call_json(
            &fixture.app,
            HttpRequest::get("/api/v1/trash")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let entries = trash["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert!(
            entries[0].get("trash_path").is_none(),
            "absolute path leaked"
        );
        let trash_id = entries[0]["id"].as_str().unwrap();

        let (status, _) = call(
            &fixture.app,
            json_request(
                Method::POST,
                &format!("/api/v1/trash/{trash_id}/restore"),
                Value::Null,
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(fixture.root.join("photos/a.jpg").is_file());

        let (_, trash) = call_json(
            &fixture.app,
            HttpRequest::get("/api/v1/trash")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert!(trash["entries"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn tags_albums_and_validation() {
        let fixture = setup("tags", false);
        let id = upload(&fixture.app, "tag.jpg").await;

        let (status, tag) = call_json(
            &fixture.app,
            json_request(
                Method::POST,
                "/api/v1/tags",
                json!({"name": "travel"}),
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let tag_id = tag["id"].as_i64().unwrap();

        let (status, item) = call_json(
            &fixture.app,
            json_request(
                Method::POST,
                &format!("/api/v1/items/{id}/tags"),
                json!({"tag_id": tag_id}),
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(item["tags"].as_array().unwrap().len(), 1);

        let (status, _) = call(
            &fixture.app,
            json_request(
                Method::POST,
                &format!("/api/v1/items/{id}/tags"),
                json!({"tag_id": 99999}),
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, album) = call_json(
            &fixture.app,
            json_request(
                Method::POST,
                "/api/v1/albums",
                json!({"name": "Summer"}),
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let album_id = album["id"].as_str().unwrap();

        let (status, _) = call(
            &fixture.app,
            json_request(
                Method::POST,
                &format!("/api/v1/albums/{album_id}/items"),
                json!({"item_id": id}),
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, items) = call_json(
            &fixture.app,
            HttpRequest::get(format!("/api/v1/albums/{album_id}/items"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(items["items"].as_array().unwrap().len(), 1);

        let (status, _) = call(
            &fixture.app,
            json_request(
                Method::POST,
                &format!("/api/v1/albums/{album_id}/items"),
                json!({"item_id": "missing"}),
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = call(
            &fixture.app,
            HttpRequest::get("/api/v1/albums/missing/items")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn transcode_job_produces_play_url() {
        if std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_err()
        {
            eprintln!("ffmpeg not installed; skipping transcode test");
            return;
        }
        let fixture = setup("transcode", true);
        let video = fixture.root.join("photos/clip.mp4");
        let ok = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=160x120:rate=5",
                "-t",
                "1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-y",
            ])
            .arg(&video)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("ffmpeg could not create fixture; skipping transcode test");
            return;
        }
        let library = fixture.config.libraries[0].to_library();
        let item =
            scan::scan_file(&fixture.config, &fixture.db, &library, &video, "clip.mp4").unwrap();

        let (status, body) = call_json(
            &fixture.app,
            json_request(
                Method::POST,
                &format!("/api/v1/items/{}/transcode", item.id),
                Value::Null,
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let job_id = body["job_id"].as_str().unwrap().to_string();

        let mut completed = false;
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let (status, job) = call_json(
                &fixture.app,
                HttpRequest::get(format!("/api/v1/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            if job["state"] == "completed" {
                completed = true;
                break;
            }
            if job["state"] == "failed" {
                panic!("transcode failed: {job}");
            }
        }
        assert!(completed, "transcode did not complete");

        let (status, item) = call_json(
            &fixture.app,
            HttpRequest::get(format!("/api/v1/items/{}", item.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let play_url = item["play_url"].as_str().unwrap();
        let (status, bytes) = call(
            &fixture.app,
            HttpRequest::get(play_url).body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(!bytes.is_empty());
    }

    #[tokio::test]
    async fn manage_libraries_add_validate_delete_and_import() {
        let fixture = setup("manage", false);
        let photos2 = fixture.root.join("photos2");
        std::fs::create_dir_all(&photos2).unwrap();

        let (status, body) = call_json(
            &fixture.app,
            HttpRequest::get("/api/v1/libraries")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["libraries"].as_array().unwrap().len(), 1);
        assert_eq!(body["libraries"][0]["source"], "config");

        let payload = json!({
            "path": photos2.to_string_lossy(),
            "name": "Second",
            "read_only": true,
        });
        let (status, body) = call_json(
            &fixture.app,
            json_request(Method::POST, "/api/v1/libraries", payload.clone(), true),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let id = body["library"]["id"].as_str().unwrap().to_string();
        assert_eq!(body["library"]["source"], "ui");
        assert_eq!(body["library"]["read_only"], true);
        // The add triggers a background scan; let it finish before mutating.
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;

        let (status, _) = call_json(
            &fixture.app,
            json_request(Method::POST, "/api/v1/libraries", payload, true),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = call_json(
            &fixture.app,
            json_request(
                Method::POST,
                "/api/v1/libraries",
                json!({"path": "/tmp"}),
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = call_json(
            &fixture.app,
            json_request(
                Method::POST,
                "/api/v1/libraries",
                json!({"path": photos2.to_string_lossy()}),
                false,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, body) = call_json(
            &fixture.app,
            HttpRequest::get(format!("/api/v1/fs/dirs?path={}", fixture.root.display()))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == "photos2"));

        let (status, body) = call_json(
            &fixture.app,
            json_request(
                Method::PATCH,
                &format!("/api/v1/libraries/{id}"),
                json!({"enabled": false}),
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["enabled"], false);

        let (status, body) = call_json(
            &fixture.app,
            json_request(
                Method::DELETE,
                &format!("/api/v1/libraries/{id}"),
                json!({}),
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["removed"], true);

        let (status, body) = call_json(
            &fixture.app,
            json_request(Method::DELETE, "/api/v1/libraries/lib", json!({}), true),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");

        // Config seeding must not resurrect a library removed from the UI.
        fixture
            .db
            .seed_config_libraries(&fixture.config.libraries)
            .unwrap();
        assert!(fixture.db.list_libraries().unwrap().is_empty());

        // Explicit re-import brings it back.
        let (status, body) = call_json(
            &fixture.app,
            json_request(
                Method::POST,
                "/api/v1/libraries/import-config",
                json!({}),
                true,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["imported"], 1);
        assert_eq!(fixture.db.list_libraries().unwrap().len(), 1);
    }
}
