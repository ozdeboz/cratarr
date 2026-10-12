//! HTTP API.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::Deserialize;
use serde_json::json;

use crate::app::{App, SettingsView, SourceView, StateView, StatusView, ToggleError};
use crate::settings::SourceConfig;
use crate::playlists::{Repair, Repairs};

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/state", get(state))
        .route("/api/cover/{source}/{id}", get(cover))
        .route("/api/playlists/{id}/toggle", post(toggle))
        .route("/api/playlists", post(create_playlist))
        .route("/api/playlists/{id}", delete(delete_playlist))
        .route("/api/playlists/{id}/rename", post(rename_playlist))
        .route("/api/playlists/{id}/restore", post(restore_playlist))
        .route("/api/albums/duplicates", get(album_duplicates))
        .route("/api/duplicates", get(duplicates))
        .route("/api/duplicates/dedupe", post(dedupe))
        .route("/api/repairs", get(repairs))
        .route("/api/repairs/apply", post(apply_repairs))
        .route("/api/undo", post(undo))
        .route("/api/status", get(status))
        .route("/api/incoming", get(incoming))
        .route("/api/incoming/cover/{source}/{album}", get(incoming_cover))
        .route("/api/settings", get(get_settings).put(save_settings))
        .route("/api/settings/preview", post(preview))
        .fallback(crate::web::serve)
        .with_state(app)
}

async fn health(State(app): State<Arc<App>>) -> Result<Json<serde_json::Value>, ApiError> {
    let n = app.playlist_count().await.map_err(ToggleError::from)?;
    Ok(Json(json!({ "status": "ok", "playlists": n })))
}

/// `?source=<id>`: follow that source; absent (or unknown): whichever started playing most recently.
#[derive(Deserialize)]
struct Follow {
    source: Option<String>,
}

async fn state(State(app): State<Arc<App>>, Query(f): Query<Follow>) -> Result<Json<StateView>, ApiError> {
    Ok(Json(app.state(f.source.as_deref()).await.map_err(ToggleError::from)?))
}

async fn toggle(State(app): State<Arc<App>>, Path(id): Path<String>, Query(f): Query<Follow>) -> Result<Json<StateView>, ApiError> {
    Ok(Json(app.toggle(&id, f.source.as_deref()).await?))
}

async fn cover(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>, headers: HeaderMap) -> Response {
    use crate::covers::Outcome;
    let inm = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok());
    let cache = (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=86400"));
    let tag = |etag: &str| HeaderValue::from_str(etag).unwrap_or(HeaderValue::from_static("\"\""));
    match app.cover(&source, &id, inm).await {
        Outcome::NotFound => (StatusCode::NOT_FOUND, Json(json!({ "error": "no cover" }))).into_response(),
        Outcome::NotModified { etag } => (StatusCode::NOT_MODIFIED, [cache, (header::ETAG, tag(&etag))]).into_response(),
        Outcome::Image { content_type, etag, bytes } => (
            [cache, (header::ETAG, tag(&etag)), (header::CONTENT_TYPE, HeaderValue::from_static(content_type))],
            bytes,
        )
            .into_response(),
    }
}

async fn incoming(State(app): State<Arc<App>>, Query(f): Query<Follow>) -> Json<crate::app::IncomingView> {
    Json(app.incoming(f.source.as_deref()).await)
}

async fn incoming_cover(State(app): State<Arc<App>>, Path((source, album)): Path<(String, String)>) -> Response {
    match app.incoming_cover(&source, &album).await {
        Some((content_type, bytes)) => {
            let ct = HeaderValue::from_str(&content_type).unwrap_or(HeaderValue::from_static("application/octet-stream"));
            ([(header::CONTENT_TYPE, ct), (header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600"))], bytes).into_response()
        }
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "no cover" }))).into_response(),
    }
}

async fn repairs(State(app): State<Arc<App>>) -> Result<Json<Repairs>, ApiError> {
    Ok(Json(app.repairs().await.map_err(ToggleError::from)?))
}

/// `{"kind":"safe"}` applies every safe repair; `{"kind":"one", playlist, from, to}` applies a single offered one.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum ApplyRequest {
    Safe,
    One { playlist: String, from: String, to: String },
}

async fn apply_repairs(State(app): State<Arc<App>>, Json(req): Json<ApplyRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let only = match req {
        ApplyRequest::Safe => None,
        ApplyRequest::One { playlist, from, to } => Some(Repair { playlist, from, to }),
    };
    let changed = app.apply_repairs(only).await?;
    Ok(Json(json!({ "playlists": changed })))
}

#[derive(Deserialize)]
struct NameRequest {
    name: String,
}

#[derive(Deserialize)]
struct RenameRequest {
    to: String,
}

#[derive(Deserialize)]
struct PlaylistRequest {
    playlist: String,
}

async fn create_playlist(State(app): State<Arc<App>>, Json(req): Json<NameRequest>) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    app.create_playlist(&req.name).await?;
    Ok((StatusCode::CREATED, Json(json!({ "playlist": req.name }))))
}

async fn rename_playlist(State(app): State<Arc<App>>, Path(id): Path<String>, Json(req): Json<RenameRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    app.rename_playlist(&id, &req.to).await?;
    Ok(Json(json!({ "playlist": req.to })))
}

async fn delete_playlist(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    app.delete_playlist(&id).await?;
    Ok(Json(json!({ "deleted": id })))
}

async fn restore_playlist(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    app.restore_playlist(&id).await?;
    Ok(Json(json!({ "restored": id })))
}

async fn album_duplicates(State(app): State<Arc<App>>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "groups": app.album_duplicates().await.map_err(ToggleError::from)? })))
}

async fn duplicates(State(app): State<Arc<App>>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!(app.duplicates().await.map_err(ToggleError::from)?)))
}

async fn dedupe(State(app): State<Arc<App>>, Json(req): Json<PlaylistRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let removed = app.dedupe(&req.playlist).await?;
    Ok(Json(json!({ "removed": removed })))
}

#[derive(Deserialize)]
struct UndoRequest {
    playlists: Vec<String>,
}

async fn undo(State(app): State<Arc<App>>, Json(req): Json<UndoRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let restored = app.undo(req.playlists).await?;
    Ok(Json(json!({ "restored": restored })))
}

pub struct ApiError(ToggleError);

impl From<ToggleError> for ApiError {
    fn from(e: ToggleError) -> Self {
        Self(e)
    }
}

async fn status(State(app): State<Arc<App>>) -> Json<StatusView> {
    Json(app.status().await)
}

async fn get_settings(State(app): State<Arc<App>>) -> Result<Json<SettingsView>, ApiError> {
    Ok(Json(app.settings().await.map_err(ToggleError::from)?))
}

#[derive(Deserialize)]
struct SaveBody {
    sources: Vec<SourceConfig>,
}

async fn save_settings(State(app): State<Arc<App>>, Json(b): Json<SaveBody>) -> Result<Json<SettingsView>, ApiError> {
    Ok(Json(app.save_settings(b.sources).await?))
}

#[derive(Deserialize)]
struct PreviewBody {
    source: SourceConfig,
}

async fn preview(State(app): State<Arc<App>>, Json(b): Json<PreviewBody>) -> Result<Json<SourceView>, ApiError> {
    Ok(Json(app.preview(b.source).await?))
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            ToggleError::NothingPlaying
            | ToggleError::NotInLibrary
            | ToggleError::NotOffered
            | ToggleError::NothingToUndo
            | ToggleError::Exists => StatusCode::CONFLICT,
            ToggleError::BadName | ToggleError::BadSettings(_) => StatusCode::BAD_REQUEST,
            ToggleError::SettingsReadOnly => StatusCode::CONFLICT,
            ToggleError::UnknownPlaylist => StatusCode::NOT_FOUND,
            ToggleError::Upstream(_) => StatusCode::BAD_GATEWAY,
        };
        if matches!(self.0, ToggleError::Upstream(_)) {
            tracing::error!("{:#}", self.0);
        }
        (status, Json(json!({ "error": self.0.to_string() }))).into_response()
    }
}
