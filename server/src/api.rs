//! HTTP API.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;

use crate::app::{App, StateView, ToggleError};
use crate::playlists::{Repair, Repairs};

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/state", get(state))
        .route("/api/cover/{id}", get(cover))
        .route("/api/playlists/{id}/toggle", post(toggle))
        .route("/api/repairs", get(repairs))
        .route("/api/repairs/apply", post(apply_repairs))
        .route("/api/undo", post(undo))
        .fallback(crate::web::serve)
        .with_state(app)
}

async fn health(State(app): State<Arc<App>>) -> Result<Json<serde_json::Value>, ApiError> {
    let n = app.playlist_count().await.map_err(ToggleError::from)?;
    Ok(Json(json!({ "status": "ok", "playlists": n })))
}

async fn state(State(app): State<Arc<App>>) -> Result<Json<StateView>, ApiError> {
    Ok(Json(app.state().await.map_err(ToggleError::from)?))
}

async fn toggle(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<StateView>, ApiError> {
    Ok(Json(app.toggle(&id).await?))
}

async fn cover(State(app): State<Arc<App>>, Path(id): Path<String>, headers: HeaderMap) -> Response {
    use crate::covers::Outcome;
    let inm = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok());
    let cache = (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=86400"));
    let tag = |etag: &str| HeaderValue::from_str(etag).unwrap_or(HeaderValue::from_static("\"\""));
    match app.cover(&id, inm).await {
        Outcome::NotFound => (StatusCode::NOT_FOUND, Json(json!({ "error": "no cover" }))).into_response(),
        Outcome::NotModified { etag } => (StatusCode::NOT_MODIFIED, [cache, (header::ETAG, tag(&etag))]).into_response(),
        Outcome::Image { content_type, etag, bytes } => (
            [cache, (header::ETAG, tag(&etag)), (header::CONTENT_TYPE, HeaderValue::from_static(content_type))],
            bytes,
        )
            .into_response(),
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

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            ToggleError::NothingPlaying | ToggleError::NotInLibrary | ToggleError::NotOffered | ToggleError::NothingToUndo => {
                StatusCode::CONFLICT
            }
            ToggleError::UnknownPlaylist => StatusCode::NOT_FOUND,
            ToggleError::Upstream(_) => StatusCode::BAD_GATEWAY,
        };
        if matches!(self.0, ToggleError::Upstream(_)) {
            tracing::error!("{:#}", self.0);
        }
        (status, Json(json!({ "error": self.0.to_string() }))).into_response()
    }
}
