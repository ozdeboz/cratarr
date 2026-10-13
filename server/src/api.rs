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
        .route("/api/events", get(events))
        .route("/api/cover/{source}/{id}", get(cover))
        .route("/api/playlists/{id}/toggle", post(toggle))
        .route("/api/favorite", post(favorite))
        .route("/api/playlists/{id}/play", post(play_playlist))
        .route("/api/player/{command}", post(transport))
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
        .route("/api/incoming/delete/{source}/{album}", post(delete_incoming))
        .route("/api/incoming/undo", post(undo_incoming))
        .route("/api/incoming/keep/{source}/{album}", get(keep_preview).post(keep_incoming))
        .route("/api/incoming/replace/{source}/{album}", get(replace_preview).post(replace_incoming))
        .route("/api/incoming/jobs", get(incoming_jobs))
        .route("/api/incoming/jobs/{id}/cancel", post(cancel_job))
        .route("/api/incoming/play/{source}/{album}", post(play_incoming))
        .route("/api/settings", get(get_settings).put(save_settings))
        .route("/api/settings/preview", post(preview))
        .route("/api/insights", get(insights))
        .route("/api/settings/lidarr", post(preview_lidarr))
        .route("/api/settings/naming", get(naming))
        .route("/api/catalogue", get(catalogue_status))
        .route("/api/catalogue/problems", get(catalogue_problems))
        .fallback(crate::web::serve)
        .with_state(app)
}

async fn health(State(app): State<Arc<App>>) -> Result<Json<serde_json::Value>, ApiError> {
    let n = app.playlists.count().await.map_err(ToggleError::from)?;
    Ok(Json(json!({ "status": "ok", "playlists": n })))
}

/// `?source=<id>`: follow that source; absent (or unknown): whichever started playing most recently.
#[derive(Deserialize)]
struct Follow {
    source: Option<String>,
}

/// `member`: the state wanted (idempotent); absent, the entry is flipped.
#[derive(Deserialize)]
struct Membership {
    source: Option<String>,
    member: Option<bool>,
}

/// The page's live channel (server-sent events, app/events.rs): `state`, `jobs` and `changed`, each when it changes.
async fn events(
    State(app): State<Arc<App>>,
    Query(f): Query<Follow>,
) -> axum::response::sse::Sse<impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    use tokio_stream::StreamExt;
    let rx = crate::app::events(&app, f.source);
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx)
        .map(|m| Ok(axum::response::sse::Event::default().event(m.name).data(m.data)));
    axum::response::sse::Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}

async fn state(State(app): State<Arc<App>>, Query(f): Query<Follow>) -> Result<Json<StateView>, ApiError> {
    Ok(Json(app.playing.state(f.source.as_deref()).await.map_err(ToggleError::from)?))
}

/// `on`: the state wanted, so a retried request changes nothing more.
#[derive(Deserialize)]
struct Favorite {
    source: Option<String>,
    on: bool,
}

async fn favorite(State(app): State<Arc<App>>, Query(f): Query<Favorite>) -> Result<Json<StateView>, ApiError> {
    Ok(Json(app.playing.set_favorite(f.source.as_deref(), f.on).await?))
}

async fn toggle(State(app): State<Arc<App>>, Path(id): Path<String>, Query(f): Query<Membership>) -> Result<Json<StateView>, ApiError> {
    Ok(Json(app.playing.toggle(&id, f.source.as_deref(), f.member).await?))
}

async fn cover(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>, headers: HeaderMap) -> Response {
    use crate::covers::Outcome;
    let inm = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok());
    let cache = (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=86400"));
    let tag = |etag: &str| HeaderValue::from_str(etag).unwrap_or(HeaderValue::from_static("\"\""));
    match app.playing.cover(&source, &id, inm).await {
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
    Json(app.incoming.list(f.source.as_deref()).await)
}

async fn play_incoming(State(app): State<Arc<App>>, Path((source, album)): Path<(String, String)>) -> Response {
    match app.incoming.play(&source, &album).await {
        Ok(player) => Json(json!({ "player": player })).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
}

async fn delete_incoming(
    State(app): State<Arc<App>>,
    Path((source, album)): Path<(String, String)>,
) -> Result<Json<crate::app::DeleteResult>, ApiError> {
    Ok(Json(app.incoming.delete(&source, &album).await?))
}

#[derive(Deserialize)]
struct UndoBody {
    undo: String,
}

/// Undo an Incoming action (Delete, Keep, Replace) by the key it returned.
async fn undo_incoming(State(app): State<Arc<App>>, Json(b): Json<UndoBody>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "restored": app.incoming.undo(&b.undo).await? })))
}

#[derive(Deserialize, Default)]
struct KeepYear {
    /// Stands in for a missing release year (the page asks for it).
    #[serde(default)]
    year: Option<String>,
}

async fn keep_preview(
    State(app): State<Arc<App>>,
    Path((source, album)): Path<(String, String)>,
    Query(q): Query<KeepYear>,
) -> Result<Json<crate::app::KeepPreview>, ApiError> {
    Ok(Json(app.incoming.keep_preview(&source, &album, q.year.as_deref()).await?))
}

async fn keep_incoming(
    State(app): State<Arc<App>>,
    Path((source, album)): Path<(String, String)>,
    body: Option<Json<KeepYear>>,
) -> Result<Json<crate::app::KeepResult>, ApiError> {
    let year = body.map(|Json(b)| b).unwrap_or_default().year;
    Ok(Json(app.incoming.keep(&source, &album, year.as_deref()).await?))
}

async fn replace_preview(
    State(app): State<Arc<App>>,
    Path((source, album)): Path<(String, String)>,
    Query(q): Query<KeepYear>,
) -> Result<Json<crate::app::ReplacePreview>, ApiError> {
    Ok(Json(app.incoming.replace_preview(&source, &album, q.year.as_deref()).await?))
}

async fn replace_incoming(
    State(app): State<Arc<App>>,
    Path((source, album)): Path<(String, String)>,
    body: Option<Json<KeepYear>>,
) -> Result<Json<crate::app::KeepResult>, ApiError> {
    let year = body.map(|Json(b)| b).unwrap_or_default().year;
    Ok(Json(app.incoming.replace(&source, &album, year.as_deref()).await?))
}

/// Albums waiting for their hi-res files to be converted before Keep or Replace, and those just finished.
async fn incoming_jobs(State(app): State<Arc<App>>) -> Json<Vec<crate::app::JobView>> {
    Json(app.incoming.jobs())
}

async fn cancel_job(State(app): State<Arc<App>>, Path(id): Path<u64>) -> Result<Json<()>, ApiError> {
    Ok(Json(app.incoming.cancel(id).await?))
}

async fn incoming_cover(State(app): State<Arc<App>>, Path((source, album)): Path<(String, String)>) -> Response {
    match app.incoming.cover(&source, &album).await {
        Some((content_type, bytes)) => {
            let ct = HeaderValue::from_str(&content_type).unwrap_or(HeaderValue::from_static("application/octet-stream"));
            ([(header::CONTENT_TYPE, ct), (header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600"))], bytes).into_response()
        }
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "no cover" }))).into_response(),
    }
}

async fn repairs(State(app): State<Arc<App>>) -> Result<Json<Repairs>, ApiError> {
    Ok(Json(app.playlists.repairs().await.map_err(ToggleError::from)?))
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
    let changed = app.playlists.apply_repairs(only).await?;
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
    app.playlists.create(&req.name).await?;
    Ok((StatusCode::CREATED, Json(json!({ "playlist": req.name }))))
}

async fn rename_playlist(State(app): State<Arc<App>>, Path(id): Path<String>, Json(req): Json<RenameRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    app.playlists.rename(&id, &req.to).await?;
    Ok(Json(json!({ "playlist": req.to })))
}

async fn delete_playlist(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    app.playlists.delete(&id).await?;
    Ok(Json(json!({ "deleted": id })))
}

async fn restore_playlist(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    app.playlists.restore(&id).await?;
    Ok(Json(json!({ "restored": id })))
}

async fn album_duplicates(State(app): State<Arc<App>>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "groups": app.playlists.album_duplicates().await.map_err(ToggleError::from)? })))
}

async fn duplicates(State(app): State<Arc<App>>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!(app.playlists.duplicates().await.map_err(ToggleError::from)?)))
}

async fn dedupe(State(app): State<Arc<App>>, Json(req): Json<PlaylistRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let removed = app.playlists.dedupe(&req.playlist).await?;
    Ok(Json(json!({ "removed": removed })))
}

#[derive(Deserialize)]
struct UndoRequest {
    playlists: Vec<String>,
}

async fn undo(State(app): State<Arc<App>>, Json(req): Json<UndoRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let restored = app.playlists.undo(req.playlists).await?;
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

#[derive(Deserialize)]
struct NamingQuery {
    template: Option<String>,
}

/// A naming template measured against Music: the one typed (`?template=`), else the one in use.
async fn naming(State(app): State<Arc<App>>, Query(q): Query<NamingQuery>) -> Result<Json<crate::app::NamingView>, ApiError> {
    Ok(Json(app.settings.naming(q.template.as_deref()).await?))
}

async fn get_settings(State(app): State<Arc<App>>) -> Result<Json<SettingsView>, ApiError> {
    Ok(Json(app.settings.view().await.map_err(ToggleError::from)?))
}

#[derive(Deserialize)]
struct SaveBody {
    sources: Vec<SourceConfig>,
    /// Omitted: keep the saved ones.
    #[serde(default)]
    tag_rules: Option<crate::catalogue::Rules>,
    /// Omitted: keep the saved one; a blank address removes it.
    #[serde(default)]
    lidarr: Option<crate::settings::LidarrConfig>,
    /// Omitted: keep the saved number.
    #[serde(default)]
    trash_days: Option<u32>,
    /// Omitted: keep the saved template; blank: the one the library follows.
    #[serde(default)]
    naming: Option<String>,
}

async fn save_settings(State(app): State<Arc<App>>, Json(b): Json<SaveBody>) -> Result<Json<SettingsView>, ApiError> {
    Ok(Json(app.settings.save(b.sources, b.tag_rules, b.lidarr, b.trash_days, b.naming).await?))
}

async fn catalogue_status(State(app): State<Arc<App>>) -> Result<Json<crate::catalogue::Status>, ApiError> {
    let c = app.library.catalogue();
    let status = tokio::task::spawn_blocking(move || c.status()).await.map_err(|e| ToggleError::Upstream(anyhow::anyhow!("{e}")))?;
    Ok(Json(status.map_err(ToggleError::from)?))
}

async fn catalogue_problems(State(app): State<Arc<App>>) -> Result<Json<Vec<crate::catalogue::Problem>>, ApiError> {
    let c = app.library.catalogue();
    let list = tokio::task::spawn_blocking(move || c.problems(500)).await.map_err(|e| ToggleError::Upstream(anyhow::anyhow!("{e}")))?;
    Ok(Json(list.map_err(ToggleError::from)?))
}

#[derive(Deserialize)]
struct PlayBody {
    mode: crate::app::PlayMode,
}

async fn transport(
    State(app): State<Arc<App>>,
    Path(command): Path<String>,
    Query(f): Query<Follow>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let command: crate::source::Transport = serde_json::from_value(json!(command))
        .map_err(|_| ToggleError::CannotPlay("unknown player command (previous, play-pause, next)".into()))?;
    Ok(Json(json!({ "player": app.playing.transport(f.source.as_deref(), command).await? })))
}

async fn play_playlist(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(f): Query<Follow>,
    Json(b): Json<PlayBody>,
) -> Result<Json<crate::app::PlayResult>, ApiError> {
    Ok(Json(app.playlists.play(&id, b.mode, f.source.as_deref()).await?))
}

#[derive(Deserialize)]
struct PreviewBody {
    source: SourceConfig,
}

async fn preview_lidarr(
    State(app): State<Arc<App>>,
    Json(b): Json<crate::settings::LidarrConfig>,
) -> Result<Json<Option<crate::app::LidarrView>>, ApiError> {
    Ok(Json(app.settings.preview_lidarr(b).await?))
}

async fn insights(State(app): State<Arc<App>>) -> Result<Json<crate::insights::Insights>, ApiError> {
    Ok(Json(app.insights.get().await.map(|r| (*r).clone()).map_err(ToggleError::from)?))
}

async fn preview(State(app): State<Arc<App>>, Json(b): Json<PreviewBody>) -> Result<Json<SourceView>, ApiError> {
    Ok(Json(app.settings.preview(b.source).await?))
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
            ToggleError::SettingsReadOnly | ToggleError::CannotPlay(_) | ToggleError::Refused(_) | ToggleError::NoMusicFolder => StatusCode::CONFLICT,
            ToggleError::UnknownPlaylist => StatusCode::NOT_FOUND,
            ToggleError::Upstream(_) => StatusCode::BAD_GATEWAY,
        };
        if matches!(self.0, ToggleError::Upstream(_)) {
            tracing::error!("{:#}", self.0);
        }
        (status, Json(json!({ "error": self.0.to_string() }))).into_response()
    }
}
