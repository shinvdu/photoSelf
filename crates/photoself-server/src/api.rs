use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path as AxumPath, Query, State};
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use photoself_core::model::{ItemQuery, Library, MediaItem, MediaKind, ScanStats, TrashEntry};
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
        "write_enabled": state.config.write.enabled,
        "trash_enabled": state.config.trash.enabled,
        "transcode_enabled": state.config.video.transcode_enabled,
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

fn write_allowed(state: &AppState) -> Result<(), ApiError> {
    if !state.config.write.enabled {
        return Err(ApiError::forbidden("write mode is disabled"));
    }
    Ok(())
}

fn library_config(
    state: &AppState,
    id: &str,
) -> Result<photoself_core::config::LibraryConfig, ApiError> {
    state
        .config
        .libraries
        .iter()
        .find(|library| library.id == id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("library not found"))
}

fn writable_library(
    state: &AppState,
    id: &str,
) -> Result<photoself_core::config::LibraryConfig, ApiError> {
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
    let db = state.db.clone();
    let item_id = id.clone();
    tokio::task::spawn_blocking(move || db.add_item_tag(&item_id, body.tag_id))
        .await
        .map_err(ApiError::internal)??;
    let item = fetch_item(&state, &id).await?;
    Ok(Json(item_dto(&state, &item)))
}

async fn remove_item_tag(
    State(state): State<AppState>,
    AxumPath((id, tag_id)): AxumPath<(String, i64)>,
) -> Result<Json<Value>, ApiError> {
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
    let db = state.db.clone();
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

    let root = PathBuf::from(&library.path);
    let target = root.join(&rel_path);
    if target.exists() && !params.overwrite.unwrap_or(false) {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(ApiError::conflict("file already exists"));
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(ApiError::internal)?;
    }

    let staging = root.join(format!(".upload-staging-{}", uuid::Uuid::now_v7()));
    tokio::fs::copy(&temp, &staging)
        .await
        .map_err(ApiError::internal)?;
    let _ = tokio::fs::remove_file(&temp).await;
    if target.exists() && params.overwrite.unwrap_or(false) {
        tokio::fs::remove_file(&target)
            .await
            .map_err(ApiError::internal)?;
    }
    tokio::fs::rename(&staging, &target)
        .await
        .map_err(ApiError::internal)?;

    let config = state.config.clone();
    let db = state.db.clone();
    let scan_path = target.clone();
    let scan_rel = rel_path.clone();
    let lib = library.to_library();
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
    let source_copy = source.clone();
    let trash_copy = trash_path.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        std::fs::copy(&source_copy, &trash_copy)?;
        std::fs::remove_file(&source_copy)?;
        Ok(())
    })
    .await
    .map_err(ApiError::internal)??;

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
    let db = state.db.clone();
    let item_id = item.id.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        db.add_trash(&entry)?;
        db.set_missing(&item_id, true)?;
        Ok(())
    })
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
    let target = PathBuf::from(&library.path).join(&entry.rel_path);
    if target.exists() {
        return Err(ApiError::conflict("target path already exists"));
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(ApiError::internal)?;
    }
    let trash_path = PathBuf::from(&entry.trash_path);
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        std::fs::copy(&trash_path, &target)?;
        std::fs::remove_file(&trash_path)?;
        Ok(())
    })
    .await
    .map_err(ApiError::internal)??;
    let db = state.db.clone();
    let item_id = entry.item_id.clone();
    let trash_id = entry.id.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        db.set_missing(&item_id, false)?;
        db.delete_trash_row(&trash_id)?;
        Ok(())
    })
    .await
    .map_err(ApiError::internal)??;
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
