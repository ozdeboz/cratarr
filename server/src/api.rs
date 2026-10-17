//! HTTP API.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use serde::Deserialize;
use serde_json::json;

use crate::app::{App, SettingsView, SourceView, StateView, StatusView, ToggleError, TrackStateView};
use crate::settings::SourceConfig;
use crate::playlists::{Repair, Repairs};

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/state", get(state))
        .route("/api/events", get(events))
        .route("/api/cover/{source}/{id}", get(cover))
        .route("/api/stream/{source}/{id}", get(stream))
        .route("/api/file/{source}/{id}", get(file))
        .route("/api/track/{source}/{id}/gain", get(track_gain))
        .route("/api/players", get(players))
        .route("/api/players/settings", get(play_on_settings).put(save_play_on_settings))
        .route("/api/players/{name}", get(player))
        .route("/api/players/{name}/command", post(player_command))
        .route("/api/players/{name}/state", axum::routing::put(player_state))
        .route("/api/players/{name}/sample", post(player_sample))
        .route("/api/players/{name}/delay", axum::routing::put(player_delay))
        .route("/api/groups/{leader}/listen.wav", get(group_listen))
        .route("/api/time", get(time_now))
        .route("/api/sync/measure", get(sync_measure))
        .route("/api/radio/next", post(radio_next))
        .route("/api/players/{name}/connect", get(player_connect))
        .route("/api/track/{source}/{id}", get(track_info))
        .route("/api/track/{source}/{id}/playlists", get(track_state))
        .route("/api/track/{source}/{id}/details", get(track_details))
        .route("/api/track/{source}/{id}/waveform", get(track_waveform))
        .route("/api/analysis/import", post(analysis_import).layer(axum::extract::DefaultBodyLimit::disable()))
        .route("/api/track/{source}/{id}/playlists/{playlist}", post(track_toggle))
        .route("/api/track/{source}/{id}/favorite", post(track_favorite))
        .route("/api/track/{source}/{id}/played", post(track_played))
        .route("/api/plays", post(plays))
        .route("/api/favorites/{source}", post(favorites))
        .route("/api/favorites/{source}/albums", get(favorite_albums))
        .route("/api/queue/add", post(queue_add))
        .route("/api/queue/jump", post(queue_jump))
        .route("/api/queue/playlist", post(queue_playlist))
        .route("/api/playlists/{id}/covers", get(playlist_covers))
        .route("/api/playlists/{id}/tracks", get(playlist_tracks))
        .route("/api/playlists/{id}/rows", get(playlist_rows))
        .route("/api/export/preview", post(export_preview))
        .route("/api/export", get(export_download))
        .route("/api/playlists/favorites", get(playlist_favorites))
        .route("/api/crates", get(crates).post(crate_create))
        .route("/api/crates/axes", get(crate_axes))
        .route("/api/crates/preview", post(crate_preview))
        .route("/api/crates/{id}", get(crate_get).put(crate_update).delete(crate_delete))
        .route("/api/crates/{id}/rows", get(crate_rows))
        .route("/api/crates/{id}/playlist", post(crate_playlist))
        .route("/api/crates/{id}/favourites", put(crate_favourites))
        .route(
            "/api/crates/{id}/image",
            get(crate_image).put(crate_image_set).delete(crate_image_remove).layer(axum::extract::DefaultBodyLimit::max(32 << 20)),
        )
        .route("/api/playlists/{id}/favorite", post(playlist_favorite))
        .route("/api/playlists/{id}/entries", put(edit_playlist))
        .route("/api/playlists/{id}/repeats", delete(remove_repeats))
        .route("/api/playlists/{id}/append", post(append_tracks))
        .route("/api/playlists/{id}/toggle", post(toggle))
        .route("/api/favorite", post(favorite))
        .route("/api/playlists/{id}/play", post(play_playlist))
        .route("/api/player/handover", post(handover))
        .route("/api/player/{command}", post(transport))
        .route("/api/playlists", post(create_playlist))
        .route("/api/playlists/{id}", delete(delete_playlist))
        .route("/api/playlists/{id}/rename", post(rename_playlist))
        .route("/api/playlists/{id}/link", put(link_playlist).delete(unlink_playlist))
        .route("/api/sync/targets", get(sync_targets))
        .route("/api/playlists/by-name", get(playlist_by_name))
        .route("/api/playlists/{id}/spoken", put(set_spoken))
        .route("/api/album-uses/{source}", get(album_uses))
        .route("/api/artists/{source}", get(artist_view))
        .route("/api/artists/{source}/favorite", get(artist_is_favorite).post(artist_favorite))
        .route("/api/albums/{source}/{album}", get(album_view))
        .route("/api/kept", post(kept_ask))
        .route("/api/albums/{source}/{album}/cover", get(album_cover))
        .route("/api/albums/{source}/{album}/play", post(play_album))
        .route("/api/albums/{source}/{album}/favorite", post(album_favorite))
        .route("/api/albums/{source}/{album}/rating", get(album_rating).post(set_album_rating))
        .route("/api/collection", get(collection))
        .route("/api/albums", get(albums))
        .route("/api/albums/{source}/{album}/move", get(move_preview).post(move_album))
        .route("/api/albums/{source}/{album}/delete", post(delete_album))
        .route("/api/albums/duplicates", get(album_duplicates))
        .route("/api/repairs", get(repairs))
        .route("/api/repairs/apply", post(apply_repairs))
        .route("/api/undo", post(undo))
        .route("/api/status", get(status))
        .route("/api/incoming", get(incoming))
        .route("/api/incoming/delete/{source}/{album}", post(delete_incoming))
        .route("/api/incoming/keep/{source}/{album}", get(keep_preview).post(keep_incoming))
        .route("/api/incoming/replace/{source}/{album}", get(replace_preview).post(replace_incoming))
        .route("/api/incoming/jobs", get(incoming_jobs))
        .route("/api/incoming/jobs/{id}/cancel", post(cancel_job))
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

/// A track's ReplayGain (decision 0035): its track and album gains (dB) and peaks (linear), from its analysis, else
/// its file's tags, as Cratarr's library serves it; `{}` where none is known. The players level by it.
async fn track_gain(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>) -> Response {
    let gain = replay_gain_of(&app, &source, &id).await;
    // Short-lived: an analysis comes, and the gain with it.
    let cache = (header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=600"));
    ([cache], Json(gain.map_or(json!({}), |g| serde_json::to_value(g).unwrap_or_default()))).into_response()
}

/// A track as Cratarr's library serves it: its gains and tempo from its analysis, else from its file's tags.
async fn served_song(app: &App, source: &str, id: &str) -> Option<crate::serve::api::Song> {
    let served = crate::serve::instance()?;
    let (cat, source, id) = (app.library.catalogue(), source.to_string(), id.to_string());
    tokio::task::spawn_blocking(move || {
        let song = if source == crate::source::cratarr::KIND.id {
            Some(id)
        } else {
            cat.track_of_source(&source, &id).and_then(|t| served.library.song_ids(t).ok().flatten()).map(|s| s.song_id)
        }?;
        served.library.songs_by_id(&[song]).ok()?.into_iter().next()
    })
    .await
    .ok()
    .flatten()
}

/// A track's ReplayGain as Cratarr's library serves it (its analysis, else its file's tags).
async fn replay_gain_of(app: &App, source: &str, id: &str) -> Option<crate::serve::api::ReplayGain> {
    served_song(app, source, id).await?.replay_gain
}

/// A track's title, artist and album, for the local player's queue (decision 0022).
async fn track_info(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>) -> Response {
    let Some((_, src)) = app.library.connected_sources().into_iter().find(|(s, _)| *s == source) else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "That server is not connected." }))).into_response();
    };
    match src.track(&id).await {
        Ok(t) => Json(json!({ "id": t.id, "name": t.title, "artist": t.artist, "album": t.album, "album_id": t.album_id }))
            .into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
}

/// The Library page's one list (decision 0034): every album of the used libraries, with its library and facts.
async fn albums(State(app): State<Arc<App>>, Query(f): Query<Follow>) -> Json<crate::app::AlbumsView> {
    Json(app.albums(f.source.as_deref()).await)
}

/// A track's file as it is (the track menu's Details): where it lies, its audio, size and dates, and every tag, in the
/// file's order (a tag Cratarr knows by its Picard name, any other by its own key).
/// A track's waveform (decision 0035): its analysis' 512 bins, each its RMS (0..=255 of the track's loudest bin), as
/// bytes; the seekbar draws it. 404 until the track is analysed (or while its file changed since, or when the decode
/// stopped early). It changes only
/// with a new analysis: an ETag of when that was made, and kept a long time.
async fn track_waveform(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>, headers: HeaderMap) -> Response {
    let cat = app.library.catalogue();
    let found = tokio::task::spawn_blocking(move || {
        let track = cat.track_of_source(&source, &id)?;
        let (folder, rel) = cat.file_of(track)?;
        let analysis = cat.analysis_of(folder, &rel)?;
        // None from a decode that stopped early: a few seconds' waveform stretched over the whole track (2026-10-16).
        if analysis.complete == Some(false) {
            return None;
        }
        let (at, version) = cat.analysis_stamp(folder, &rel)?;
        let etag = format!("\"{version}-{at}\"");
        if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(etag.as_str()) {
            return Some((etag, None));
        }
        Some((etag, Some(analysis.waveform)))
    })
    .await
    .ok()
    .flatten();
    let Some((etag, bins)) = found else { return StatusCode::NOT_FOUND.into_response() };
    let mut res = match bins {
        None => StatusCode::NOT_MODIFIED.into_response(),
        Some(b) => ([(header::CONTENT_TYPE, "application/octet-stream")], b).into_response(),
    };
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=2592000"));
    if let Ok(v) = HeaderValue::from_str(&etag) {
        h.insert(header::ETAG, v);
    }
    res
}

#[derive(Deserialize)]
struct ImportTo {
    /// The library folder as the server sees it (`/media/Music`).
    folder: String,
}

/// Analyses made elsewhere (decision 0035: the ThinkBook's pass), as JSON lines for one library folder, read as they
/// come (tens of MB): each taken while its file is as it was analysed. Answers with the counts.
async fn analysis_import(
    State(app): State<Arc<App>>,
    Query(q): Query<ImportTo>,
    body: axum::body::Body,
) -> Result<Json<crate::app::analysis_import::ImportCounts>, ApiError> {
    use crate::app::analysis_import::{ImportCounts, folder_of, import_batch};
    use tokio_stream::StreamExt;
    const BATCH: usize = 500;
    let cat = app.library.catalogue();
    let folder = folder_of(&cat, &q.folder).ok_or_else(|| ToggleError::Refused(format!("{} is not one of the library's folders.", q.folder)))?;
    let flush = |lines: Vec<Vec<u8>>, counts: ImportCounts| {
        let cat = cat.clone();
        async move {
            tokio::task::spawn_blocking(move || {
                let mut counts = counts;
                import_batch(&cat, folder, &lines, &mut counts).map(|_| counts)
            })
            .await
            .map_err(|e| ToggleError::Upstream(anyhow::anyhow!("{e}")))?
            .map_err(ToggleError::from)
        }
    };
    let mut counts = ImportCounts::default();
    let (mut rest, mut lines): (Vec<u8>, Vec<Vec<u8>>) = (Vec::new(), Vec::new());
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ToggleError::Refused(format!("The upload broke off: {e}")))?;
        rest.extend_from_slice(&chunk);
        while let Some(i) = rest.iter().position(|b| *b == b'\n') {
            lines.push(rest.drain(..=i).collect());
        }
        if lines.len() >= BATCH {
            counts = flush(std::mem::take(&mut lines), counts).await?;
        }
    }
    if !rest.is_empty() {
        lines.push(rest);
    }
    counts = flush(lines, counts).await?;
    tracing::info!("analysis import into {}: {counts:?}", q.folder);
    Ok(Json(counts))
}

async fn track_details(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>) -> Response {
    // Its tempo as the library serves it (the BPM tag, else the analysis').
    let bpm = served_song(&app, &source, &id).await.and_then(|s| s.bpm);
    let cat = app.library.catalogue();
    let read = tokio::task::spawn_blocking(move || -> Option<serde_json::Value> {
        let track = cat.track_of_source(&source, &id)?;
        let (folder, rel) = cat.file_of(track)?;
        let root = cat.folders().get(folder as usize)?.clone();
        let path = root.join(&rel);
        let meta = std::fs::metadata(&path).ok();
        let modified = meta.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs());
        let (file, _) = crate::catalogue::tags::read(&path).ok()?;
        let tags: Vec<serde_json::Value> = file.items.iter().map(|(raw, name, value)| json!({ "name": name.unwrap_or(raw), "value": value })).collect();
        Some(json!({
            "path": rel,
            "folder": root.file_name().map(|n| n.to_string_lossy().into_owned()),
            "format": file.format,
            "bitrate_kbps": file.bitrate_kbps,
            "sample_rate_hz": file.sample_rate_hz,
            "bit_depth": file.bit_depth,
            "channels": file.channels,
            "duration_ms": file.duration_ms,
            "bpm": bpm,
            "size": meta.map(|m| m.len()),
            "modified": modified,
            "added": cat.born_of(track),
            // As the crates' genre axis reads them: what a genre in the panel matches.
            "genres": cat.genres_of_ids(&source, std::slice::from_ref(&id)).pop().unwrap_or_default(),
            "tags": tags,
        }))
    });
    match read.await.ok().flatten() {
        Some(details) => Json(details).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "That track's file cannot be read." }))).into_response(),
    }
}

/// An album's view in the panel (decision 0023), and its tracks for the local player (decision 0022).
/// An album as the panel shows it. One that left Incoming is followed: where it is now, or what became of it.
/// Kept on this device (decision 0038): what the app's kept items hold now, with their versions.
async fn kept_ask(State(app): State<Arc<App>>, Json(ask): Json<crate::app::kept::Ask>) -> Json<serde_json::Value> {
    Json(json!({ "items": crate::app::kept::ask(&app, ask).await }))
}

async fn album_view(State(app): State<Arc<App>>, Path((source, album)): Path<(String, String)>) -> Result<Json<crate::app::AlbumView>, ApiError> {
    use crate::app::AlbumFate;
    let failed = match app.playing.album_view(&source, &album).await {
        Ok(v) if !v.rows.is_empty() => return Ok(Json(v)),
        Ok(v) => Ok(v),
        Err(e) => Err(e),
    };
    match app.incoming.album_fate(&source, &album).await {
        Some(AlbumFate::Moved(now)) if now != album => Ok(Json(app.playing.album_view(&source, &now).await?)),
        Some(AlbumFate::NotYet(title)) => Err(ToggleError::Refused(format!("{title} is in Music now. The server has not added it yet.")).into()),
        Some(AlbumFate::Deleted(title)) => Err(ToggleError::Refused(format!("{title} was deleted.")).into()),
        _ => Ok(Json(failed?)),
    }
}

/// A track's playlists and favourite, for the local player's track (decision 0022).
async fn track_state(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>) -> Result<Json<TrackStateView>, ApiError> {
    Ok(Json(app.playing.track_state(&source, &id).await?))
}

async fn track_toggle(
    State(app): State<Arc<App>>,
    Path((source, id, playlist)): Path<(String, String, String)>,
    Query(f): Query<Membership>,
) -> Result<Json<TrackStateView>, ApiError> {
    Ok(Json(app.playing.track_toggle(&source, &id, &playlist, f.member).await?))
}

async fn track_favorite(
    State(app): State<Arc<App>>,
    Path((source, id)): Path<(String, String)>,
    Query(f): Query<Favorite>,
) -> Result<Json<TrackStateView>, ApiError> {
    Ok(Json(app.playing.track_favorite(&source, &id, f.on).await?))
}

#[derive(Deserialize)]
struct ArtistName {
    name: String,
    #[serde(default)]
    on: bool,
}

/// An artist for the panel (decided 2026-10-16): by name, as the albums credit them (`?name=`).
async fn artist_view(
    State(app): State<Arc<App>>,
    Path(source): Path<String>,
    Query(q): Query<ArtistName>,
) -> Result<Json<crate::app::ArtistView>, ApiError> {
    Ok(Json(app.playing.artist_view(&source, &q.name).await?))
}

/// Whether an artist is a favourite (`?name=`): the heart on an album's artist chip; null where the server keeps none.
async fn artist_is_favorite(
    State(app): State<Arc<App>>,
    Path(source): Path<String>,
    Query(q): Query<ArtistName>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "favorite": app.playing.artist_is_favorite(&source, &q.name).await? })))
}

/// The artist panel's heart (`?name=&on=`).
async fn artist_favorite(
    State(app): State<Arc<App>>,
    Path(source): Path<String>,
    Query(q): Query<ArtistName>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "favorite": app.playing.artist_favorite(&source, &q.name, q.on).await? })))
}

/// Every album the owner played or rated on a server, by id: plays, last played, rating (the Library's orders and
/// filters, decided 2026-10-16).
async fn album_uses(
    State(app): State<Arc<App>>,
    Path(source): Path<String>,
) -> Result<Json<std::collections::BTreeMap<String, crate::source::AlbumUse>>, ApiError> {
    Ok(Json(app.playing.album_uses(&source).await?))
}

/// The album panel's heart: an album a favourite or not.
async fn album_favorite(
    State(app): State<Arc<App>>,
    Path((source, album)): Path<(String, String)>,
    Query(f): Query<Favorite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "favorite": app.playing.album_favorite(&source, &album, f.on).await? })))
}

/// An album's rating, 0 to 5 (`null`: the server keeps none): the album menu's stars.
async fn album_rating(State(app): State<Arc<App>>, Path((source, album)): Path<(String, String)>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "rating": app.playing.album_rating(&source, &album).await? })))
}

#[derive(Deserialize)]
struct Stars {
    stars: u8,
}

/// Rates an album, 0 (none) to 5.
async fn set_album_rating(
    State(app): State<Arc<App>>,
    Path((source, album)): Path<(String, String)>,
    Query(q): Query<Stars>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "rating": app.playing.set_album_rating(&source, &album, q.stars).await? })))
}

/// The favourite albums of a connected server (their ids): the Library's Favourites only and the album panel's heart.
async fn favorite_albums(State(app): State<Arc<App>>, Path(source): Path<String>) -> Result<Json<Vec<String>>, ApiError> {
    Ok(Json(app.playing.favorite_albums(&source).await?))
}

/// Which of these tracks (ids on that server) are favourites: the hearts on a list's rows (decision 0023).
#[derive(Deserialize)]
struct Ids {
    ids: Vec<String>,
}

async fn favorites(
    State(app): State<Arc<App>>,
    Path(source): Path<String>,
    Json(b): Json<Ids>,
) -> Result<Json<std::collections::BTreeMap<String, bool>>, ApiError> {
    Ok(Json(app.playing.favorites(&source, &b.ids).await?))
}

/// A playlist's rows for its view (decision 0023): the server the ids are for, and each line with what is known of it.
async fn playlist_rows(State(app): State<Arc<App>>, Path(id): Path<String>, Query(f): Query<Follow>) -> Result<Response, ApiError> {
    let (source, rows) = app.playlists.rows(&id, f.source.as_deref()).await?;
    Ok(Json(json!({ "source": source, "rows": rows })).into_response())
}

/// What to export (decision 0007): playlists by name, with their tracks or not, in one folder or the library's, as they
/// are or combined into one of this name.
#[derive(Deserialize, Default)]
struct ExportRequest {
    playlists: Vec<String>,
    #[serde(default)]
    tracks: bool,
    #[serde(default)]
    flat: bool,
    #[serde(default)]
    combine: Option<String>,
}

impl ExportRequest {
    /// From the download's address: `playlist` repeated, `tracks=1`, `flat=1`, `combine=…`.
    fn from_query(q: &str) -> Self {
        let mut r = ExportRequest::default();
        for pair in q.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let v = form_decode(v);
            match k {
                "playlist" => r.playlists.push(v),
                "tracks" => r.tracks = v == "1",
                "flat" => r.flat = v == "1",
                "combine" => r.combine = Some(v),
                _ => {}
            }
        }
        r
    }

    fn options(&self) -> Result<crate::export::Options, ApiError> {
        if self.playlists.is_empty() {
            return Err(ToggleError::Refused("Choose at least one playlist.".into()).into());
        }
        let combine = match self.combine.as_deref().map(str::trim) {
            Some("") => return Err(ToggleError::BadName.into()),
            c => c.map(str::to_string),
        };
        Ok(crate::export::Options { tracks: self.tracks, flat: self.tracks && self.flat, combine })
    }
}

/// `application/x-www-form-urlencoded` value: `+` is a space, `%XX` a byte (UTF-8 as a whole).
fn form_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                    Ok(x) => {
                        out.push(x);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// An export's size before starting: its tracks, the missing ones left out, and with tracks its bytes.
async fn export_preview(State(app): State<Arc<App>>, Json(r): Json<ExportRequest>) -> Result<Json<crate::export::Preview>, ApiError> {
    let o = r.options()?;
    let plan = app.playlists.export_plan(&r.playlists).await?;
    Ok(Json(crate::export::preview(&plan, &o)))
}

/// The export itself, as a download: one playlist without tracks as its `.m3u`, anything else as one streamed `.zip`.
async fn export_download(State(app): State<Arc<App>>, axum::extract::RawQuery(q): axum::extract::RawQuery) -> Result<Response, ApiError> {
    let r = ExportRequest::from_query(q.as_deref().unwrap_or(""));
    let o = r.options()?;
    let plan = app.playlists.export_plan(&r.playlists).await?;
    let name = crate::export::download_name(&plan, &o);
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&attachment(&name)) {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if !crate::export::is_zip(&plan, &o) {
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/x-mpegurl"));
        return Ok((headers, crate::export::single_m3u(&plan, &o)).into_response());
    }
    let layout = crate::export::layout(&plan, &o);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (len, rx) = crate::export::stream(plan, layout, now);
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    Ok((headers, axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).into_response())
}

/// `Content-Disposition` for a download: an ASCII name for old clients and the UTF-8 one (RFC 6266).
fn attachment(name: &str) -> String {
    let ascii: String = name.chars().map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' { c } else { '_' }).collect();
    let utf8: String = name
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{utf8}")
}

/// The permanent crates and their sizes (decision 0024).
async fn crates(State(app): State<Arc<App>>, Query(f): Query<Follow>) -> Result<Json<Vec<crate::app::CrateView>>, ApiError> {
    Ok(Json(app.playlists.crates(f.source.as_deref()).await?))
}

/// A page of a crate's rows: `offset` and `limit` (at most 500, default 200).
#[derive(Deserialize)]
struct Page {
    source: Option<String>,
    #[serde(default)]
    offset: usize,
    limit: Option<usize>,
    /// The favourites toggle's setting the page shows (it may be on its way to the server); the saved one without.
    favourites: Option<crate::crates::Favourites>,
}

async fn crate_rows(State(app): State<Arc<App>>, Path(id): Path<String>, Query(q): Query<Page>) -> Result<Response, ApiError> {
    let limit = q.limit.unwrap_or(200).clamp(1, 500);
    Ok(Json(app.playlists.crate_rows(&id, q.source.as_deref(), q.offset, limit, q.favourites).await?).into_response())
}

/// One's own crate as the builder sends it (decision 0024).
#[derive(Deserialize)]
struct CrateBody {
    name: String,
    conditions: Vec<crate::crates::Condition>,
}

async fn crate_create(State(app): State<Arc<App>>, Json(b): Json<CrateBody>) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let (id, undo) = app.playlists.crate_save(None, &b.name, b.conditions).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id, "undo": undo }))))
}

async fn crate_update(State(app): State<Arc<App>>, Path(id): Path<String>, Json(b): Json<CrateBody>) -> Result<Json<serde_json::Value>, ApiError> {
    let (id, undo) = app.playlists.crate_save(Some(&id), &b.name, b.conditions).await?;
    Ok(Json(json!({ "id": id, "undo": undo })))
}

async fn crate_get(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<crate::crates::CrateDef>, ApiError> {
    Ok(Json(app.playlists.crate_def(&id).await?))
}

async fn crate_delete(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let undo = app.playlists.crate_delete(&id).await?;
    Ok(Json(json!({ "deleted": id, "undo": undo })))
}

/// A crate's own picture (its card's), as JPEG.
async fn crate_image(State(app): State<Arc<App>>, Path(id): Path<String>, headers: HeaderMap) -> Response {
    let Some((bytes, modified)) = app.playlists.crate_image(&id).await else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "no picture" }))).into_response();
    };
    let etag = crate::covers::etag(modified, bytes.len() as u64);
    let tag = HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("\"\""));
    let cache = (header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()).is_some_and(|h| crate::covers::etag_matches(h, &etag)) {
        return (StatusCode::NOT_MODIFIED, [cache, (header::ETAG, tag)]).into_response();
    }
    ([cache, (header::ETAG, tag), (header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"))], bytes).into_response()
}

/// Sets a crate's own picture: the body is the picture (any Cratarr can read).
async fn crate_image_set(State(app): State<Arc<App>>, Path(id): Path<String>, body: axum::body::Bytes) -> Result<StatusCode, ApiError> {
    app.playlists.set_crate_image(&id, body.to_vec()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The crate shows its albums' covers again.
async fn crate_image_remove(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    app.playlists.remove_crate_image(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The axes the builder draws its controls from, with their domains in this library.
async fn crate_axes(State(app): State<Arc<App>>, Query(f): Query<Follow>) -> Result<Json<Vec<crate::crates::AxisView>>, ApiError> {
    Ok(Json(app.playlists.crate_axes(f.source.as_deref()).await?))
}

/// The builder's live preview: an unsaved crate's size and first rows.
#[derive(Deserialize)]
struct Preview {
    conditions: Vec<crate::crates::Condition>,
}

async fn crate_preview(State(app): State<Arc<App>>, Query(q): Query<Page>, Json(p): Json<Preview>) -> Result<Response, ApiError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    Ok(Json(app.playlists.crate_page(&p.conditions, q.source.as_deref(), q.offset, limit).await?).into_response())
}

/// Make playlist: the crate's tracks now, as the panel shows them (its favourites toggle).
#[derive(Deserialize)]
struct CratePlaylistRequest {
    name: String,
}

#[derive(Deserialize)]
struct CrateFavouritesRequest {
    favourites: crate::crates::Favourites,
}

/// A crate's favourites toggle (decision 0024, amended): `all`, `only` or `none`, kept for every device.
async fn crate_favourites(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(req): Json<CrateFavouritesRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    app.playlists.set_crate_favourites(&id, req.favourites).await?;
    Ok(Json(json!({ "crate": id, "favourites": req.favourites })))
}

async fn crate_playlist(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(f): Query<Follow>,
    Json(req): Json<CratePlaylistRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let (added, undo) = app.playlists.crate_to_playlist(&id, f.source.as_deref(), &req.name).await?;
    Ok((StatusCode::CREATED, Json(json!({ "playlist": req.name, "added": added, "undo": undo }))))
}

/// Which playlists are favourites (by name), as Finamp's playlist hearts (decision 0023).
async fn playlist_favorites(State(app): State<Arc<App>>, Query(f): Query<Follow>) -> Result<Json<std::collections::BTreeMap<String, bool>>, ApiError> {
    Ok(Json(app.playlists.favorites(f.source.as_deref()).await?))
}

async fn playlist_favorite(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(f): Query<Favorite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    app.playlists.set_favorite(&id, f.source.as_deref(), f.on).await?;
    Ok(Json(json!({ "playlist": id, "favorite": f.on })))
}

/// A playlist edited in its view (decision 0023): `entries` (its lines, reordered or some left out) replaces `expected`
/// (the lines as the view showed them); refused when the playlist changed since.
#[derive(Deserialize)]
struct Edit {
    expected: Vec<String>,
    entries: Vec<String>,
}

async fn edit_playlist(State(app): State<Arc<App>>, Path(id): Path<String>, Json(e): Json<Edit>) -> Result<Json<serde_json::Value>, ApiError> {
    let undo = app.playlists.edit(&id, &e.expected, &e.entries).await?;
    Ok(Json(json!({ "playlist": id, "entries": e.entries.len(), "undo": undo })))
}

/// Take a playlist's repeats out, the first copy of each track staying (decision 0025, amended 2026-10-16).
async fn remove_repeats(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let undo = app.playlists.remove_repeats(&id).await?;
    Ok(Json(json!({ "playlist": id, "undo": undo })))
}

/// Tracks to add at a playlist's end, whether or not it holds them already (the user chose Add again).
#[derive(Deserialize)]
struct Append {
    source: String,
    ids: Vec<String>,
}

async fn append_tracks(State(app): State<Arc<App>>, Path(id): Path<String>, Json(a): Json<Append>) -> Result<Json<serde_json::Value>, ApiError> {
    let (added, undo) = app.playlists.append_tracks(&id, &a.source, &a.ids).await?;
    Ok(Json(json!({ "playlist": id, "added": added, "undo": undo })))
}

/// A playlist's tracks for the local player: the server they are on and their ids in file order (decision 0022).
async fn playlist_tracks(State(app): State<Arc<App>>, Path(id): Path<String>, Query(f): Query<Follow>) -> Result<Response, ApiError> {
    let (source, ids, unknown) = app.playlists.track_ids(&id, f.source.as_deref()).await?;
    Ok(Json(json!({ "source": source, "ids": ids, "unknown": unknown })).into_response())
}

#[derive(Deserialize)]
struct StreamFrom {
    /// Start at the frame holding this moment (milliseconds; FLAC only), for seeking.
    #[serde(default)]
    from_ms: u64,
}

/// A track's audio for the local player (decision 0022): FLAC as fragmented MP4, MP3 as it is, read from Cratarr's own
/// mount one piece at a time (a fragment of about a second, or 64 KB) so memory stays small.
// --- Play On (decision 0031; app/playon.rs) ---

async fn players(State(app): State<Arc<App>>) -> Json<serde_json::Value> {
    Json(json!({ "players": app.play_on.list() }))
}

async fn player(State(app): State<Arc<App>>, Path(name): Path<String>) -> Result<Response, ApiError> {
    match app.play_on.get(&name) {
        Some(p) => Ok(Json(p).into_response()),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({ "error": format!("No player is called {name}.") }))).into_response()),
    }
}

async fn player_command(State(app): State<Arc<App>>, Path(name): Path<String>, Json(c): Json<crate::app::PlayOnCommand>) -> Result<StatusCode, ApiError> {
    app.play_on.command(&name, c).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn player_state(State(app): State<Arc<App>>, Path(name): Path<String>, Json(r): Json<crate::app::PlayerReport>) -> Result<StatusCode, ApiError> {
    app.play_on.report(&name, r)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Cratarr's time, for the players' clock exchange (sync play, decision 0037): epoch ms, to the microsecond.
async fn time_now() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "now_ms": crate::app::together::now_ms() }))
}

/// A player's answer to a measurement (sync play, step 1).
async fn player_sample(State(app): State<Arc<App>>, Path(name): Path<String>, Json(s): Json<crate::app::together::Sample>) -> Result<StatusCode, ApiError> {
    app.play_on.sample(&name, s)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct Listening {
    player: String,
}

/// A group's sound as one long WAV, for a player without Snapcast's player (a Kodi, decision 0037 amended).
async fn group_listen(State(app): State<Arc<App>>, Path(leader): Path<String>, Query(q): Query<Listening>) -> Response {
    let Some(snap) = crate::snapcast::Snapcast::instance() else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "Sync play is not served here." }))).into_response();
    };
    let delay = u32::from(app.play_on.delay_of(&q.player));
    let body = axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(snap.listen(&leader, &q.player, delay)));
    ([(header::CONTENT_TYPE, HeaderValue::from_static("audio/wav")), (header::CACHE_CONTROL, HeaderValue::from_static("no-store"))], body).into_response()
}

#[derive(serde::Deserialize)]
struct DelayBody {
    ms: u16,
}

/// A player's speaker delay (sync play, decision 0037): heard at once in a group.
async fn player_delay(State(app): State<Arc<App>>, Path(name): Path<String>, Json(b): Json<DelayBody>) -> Result<Json<serde_json::Value>, ApiError> {
    let ms = app.play_on.set_delay(&name, b.ms).await?;
    Ok(Json(json!({ "ms": ms })))
}

/// Every online player's place at one moment, and how far apart those on the same track are (sync play, step 1).
async fn sync_measure(State(app): State<Arc<App>>) -> Json<crate::app::together::Measure> {
    Json(app.play_on.measure().await)
}

/// The radio's next tracks for the player that plays (decision 0036): already in the radio lane.
async fn radio_next(Json(r): Json<crate::app::radio::NextRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let tracks = crate::app::radio::next(r).await?;
    Ok(Json(serde_json::json!({ "tracks": tracks })))
}

/// The player's own connection: the commands for it, as server-sent events (`command`).
#[derive(Deserialize)]
struct ConnectAs {
    /// `browser`: a page that plays here joins as a player while it is open (decision 0033; 0031 amended).
    #[serde(default)]
    kind: Option<String>,
    /// The page's own id (one per tab): another page may not take a name one holds.
    #[serde(default)]
    page: Option<String>,
}

async fn player_connect(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
    Query(q): Query<ConnectAs>,
) -> Result<axum::response::sse::Sse<impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>>, ApiError> {
    use tokio_stream::StreamExt;
    let commands = if q.kind.as_deref() == Some("browser") {
        app.play_on.connect_browser(&name, q.page.as_deref())?
    } else {
        app.play_on.connect_app(&name)?
    };
    let stream = commands.map(|c| Ok(axum::response::sse::Event::default().event("command").data(c)));
    Ok(axum::response::sse::Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15))))
}

async fn play_on_settings(State(app): State<Arc<App>>) -> Json<crate::app::PlayOnSettingsView> {
    Json(app.play_on.settings_view())
}

async fn save_play_on_settings(State(app): State<Arc<App>>, Json(s): Json<crate::app::PlayOnSettings>) -> Result<Json<crate::app::PlayOnSettingsView>, ApiError> {
    Ok(Json(app.play_on.save_settings(s).await?))
}

#[derive(Deserialize)]
struct FileAs {
    /// `opus`: a lossless track as a smaller copy (decision 0033, step 4); a lossy one comes as it is.
    #[serde(default, rename = "as")]
    as_: Option<String>,
}

/// A track's file as it is, with byte ranges: for a native player (the Android app's, decision 0031 amended), which
/// seeks in the file itself and plays gapless from its own headers. With `?as=opus`, the downloads' smaller copy.
async fn file(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>, Query(q): Query<FileAs>, headers: HeaderMap) -> Response {
    let cat = app.library.catalogue();
    let source_id = (source.clone(), id.clone());
    let found = tokio::task::spawn_blocking(move || crate::stream::locate(&cat, &source, &id)).await.ok().flatten();
    let Some((path, _, _)) = found else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "That track is not in the library." }))).into_response();
    };
    if q.as_.as_deref() == Some("opus")
        && let Some(copy) = opus_copy(path.clone(), replay_gain_of(&app, &source_id.0, &source_id.1).await).await
    {
        return copy;
    }
    crate::serve::files::serve_file(&path, &headers, false).await
}

/// At most this many copies are made at once: each keeps a core busy.
static COPIES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// A lossless file as Ogg Opus, made as it is sent (no length, no ranges: downloads take it whole). `None` for a file
/// that is not lossless.
async fn opus_copy(path: std::path::PathBuf, gain: Option<crate::serve::api::ReplayGain>) -> Option<Response> {
    let gains = gain.map_or((None, None), |g| (g.track_gain, g.album_gain));
    let probe = path.clone();
    let lossless = tokio::task::spawn_blocking(move || crate::convert::Source::open(&probe).ok().flatten().is_some()).await.ok()?;
    if !lossless {
        return None;
    }
    let permit = COPIES.acquire().await.ok()?;
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(4);
    tokio::task::spawn_blocking(move || {
        let failed = tx.clone();
        let out = ChannelWriter { tx, buf: Vec::with_capacity(ChannelWriter::PIECE) };
        if let Err(e) = crate::opus_copy::copy(&path, out, gains) {
            let stopped = e.downcast_ref::<std::io::Error>().is_some_and(|e| e.kind() == std::io::ErrorKind::BrokenPipe);
            if !stopped {
                tracing::warn!("Opus copy of {}: {e:#}", path.display());
                // The body has started: an error breaks it off, so the download fails instead of keeping half a file.
                let _ = failed.blocking_send(Err(std::io::Error::other(format!("{e:#}"))));
            }
        }
        drop(permit);
    });
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/ogg"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Some((headers, axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).into_response())
}

/// A blocking writer into a response body, a piece at a time; it fails once the receiver has gone (the download was
/// stopped), so the copy stops too.
struct ChannelWriter {
    tx: tokio::sync::mpsc::Sender<Result<axum::body::Bytes, std::io::Error>>,
    buf: Vec<u8>,
}

impl ChannelWriter {
    const PIECE: usize = 64 * 1024;

    fn send(&mut self) -> std::io::Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        let piece = std::mem::replace(&mut self.buf, Vec::with_capacity(Self::PIECE));
        self.tx.blocking_send(Ok(piece.into())).map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))
    }
}

impl std::io::Write for ChannelWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(data);
        if self.buf.len() >= Self::PIECE {
            self.send()?;
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.send()
    }
}

async fn stream(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>, Query(q): Query<StreamFrom>) -> Response {
    use crate::stream::{FlacToMp4, Kind};
    let cat = app.library.catalogue();
    let found = tokio::task::spawn_blocking(move || crate::stream::locate(&cat, &source, &id)).await.ok().flatten();
    let Some((path, kind, duration_ms)) = found else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "That track is not in the library." }))).into_response();
    };
    let Some(kind) = kind else {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, Json(json!({ "error": "Only FLAC and MP3 play here for now." }))).into_response();
    };
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(2);
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    match kind {
        Kind::Flac => {
            let from = q.from_ms;
            let opened = tokio::task::spawn_blocking(move || FlacToMp4::open(&path, from)).await;
            let mut flac = match opened {
                Ok(Ok(f)) => f,
                Ok(Err(e)) => return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": format!("{e:#}") }))).into_response(),
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/mp4"));
            let info = flac.info().clone();
            headers.insert("x-sample-rate", HeaderValue::from(info.sample_rate));
            if let Some(total) = info.total_samples {
                headers.insert("x-total-samples", HeaderValue::from(total));
            }
            tokio::task::spawn_blocking(move || {
                if tx.blocking_send(Ok(flac.init().into())).is_err() {
                    return;
                }
                loop {
                    let next = match flac.next_fragment() {
                        Ok(Some(f)) => Ok(f.into()),
                        Ok(None) => return,
                        Err(e) => Err(std::io::Error::other(format!("{e:#}"))),
                    };
                    let failed = next.is_err();
                    // A closed channel: the page stopped reading (a skip, a seek); stop reading the file too.
                    if tx.blocking_send(next).is_err() || failed {
                        return;
                    }
                }
            });
        }
        Kind::Mp3 => {
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/mpeg"));
            // An MP3 without a LAME/Xing header (constant bitrate) does not say its length: the page shows this one
            // (and can seek by it) until the whole track is in.
            if let Some(ms) = duration_ms {
                headers.insert("x-duration-ms", HeaderValue::from(ms));
            }
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                let Ok(mut file) = tokio::fs::File::open(&path).await else { return };
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    match file.read(&mut buf).await {
                        Ok(0) => return,
                        Ok(n) => {
                            if tx.send(Ok(axum::body::Bytes::copy_from_slice(&buf[..n]))).await.is_err() {
                                return;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e)).await;
                            return;
                        }
                    }
                }
            });
        }
    }
    (headers, axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).into_response()
}

async fn incoming(State(app): State<Arc<App>>, Query(f): Query<Follow>) -> Json<crate::app::IncomingView> {
    Json(app.incoming.list(f.source.as_deref()).await)
}

async fn collection(State(app): State<Arc<App>>, Query(f): Query<Follow>) -> Json<crate::app::CollectionView> {
    Json(app.collection.list(f.source.as_deref()).await)
}

/// Play an album of a connected server on its player; answers with the player's label.
async fn play_album(State(app): State<Arc<App>>, Path((source, album)): Path<(String, String)>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "player": app.playing.play_album(&source, &album).await? })))
}

async fn delete_incoming(
    State(app): State<Arc<App>>,
    Path((source, album)): Path<(String, String)>,
) -> Result<Json<crate::app::DeleteResult>, ApiError> {
    Ok(Json(app.incoming.delete(&source, &album).await?))
}


#[derive(Deserialize, Default)]
struct KeepYear {
    /// Stands in for a missing release year (the page asks for it).
    #[serde(default)]
    year: Option<String>,
}

/// Where a move goes (decision 0034): a library's id, and the year when the album's tags lack it.
#[derive(Deserialize)]
struct MoveTo {
    to: String,
    #[serde(default)]
    year: Option<String>,
}

async fn delete_album(State(app): State<Arc<App>>, Path((source, album)): Path<(String, String)>) -> Result<Json<crate::app::DeleteResult>, ApiError> {
    Ok(Json(app.delete_album(&source, &album).await?))
}

async fn move_preview(State(app): State<Arc<App>>, Path((source, album)): Path<(String, String)>, Query(q): Query<MoveTo>) -> Result<Json<crate::app::KeepPreview>, ApiError> {
    Ok(Json(app.move_preview(&source, &album, &q.to, q.year.as_deref()).await?))
}

async fn move_album(State(app): State<Arc<App>>, Path((source, album)): Path<(String, String)>, Json(b): Json<MoveTo>) -> Result<Json<crate::app::KeepResult>, ApiError> {
    Ok(Json(app.move_album(&source, &album, &b.to, b.year.as_deref()).await?))
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

async fn album_cover(State(app): State<Arc<App>>, Path((source, album)): Path<(String, String)>) -> Response {
    match app.playing.album_cover(&source, &album).await {
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

/// `{"kind":"safe"}` applies every safe repair (`"playlists": [...]`: only theirs, the panel's Repair tab, decision
/// 0025); `{"kind":"one", playlist, from, to}` applies a single offered one.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum ApplyRequest {
    Safe {
        #[serde(default)]
        playlists: Option<Vec<String>>,
    },
    One { playlist: String, from: String, to: String },
}

async fn apply_repairs(State(app): State<Arc<App>>, Json(req): Json<ApplyRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let (only, within) = match req {
        ApplyRequest::Safe { playlists } => (None, playlists),
        ApplyRequest::One { playlist, from, to } => (Some(Repair { playlist, from, to }), None),
    };
    let (changed, undo) = app.playlists.apply_repairs(only, within.as_deref()).await?;
    Ok(Json(json!({ "playlists": changed, "undo": undo })))
}

#[derive(Deserialize)]
struct NameRequest {
    name: String,
}

#[derive(Deserialize)]
struct RenameRequest {
    to: String,
}

async fn create_playlist(State(app): State<Arc<App>>, Json(req): Json<NameRequest>) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let undo = app.playlists.create(&req.name).await?;
    Ok((StatusCode::CREATED, Json(json!({ "playlist": req.name, "undo": undo }))))
}

async fn rename_playlist(State(app): State<Arc<App>>, Path(id): Path<String>, Json(req): Json<RenameRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let undo = app.playlists.rename(&id, &req.to).await?;
    Ok(Json(json!({ "playlist": req.to, "undo": undo })))
}

async fn delete_playlist(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let undo = app.playlists.delete(&id).await?;
    Ok(Json(json!({ "deleted": id, "undo": undo })))
}

#[derive(Deserialize)]
struct ByNameQuery {
    name: String,
    #[serde(default)]
    shuffle: bool,
    /// Default 50, at most 100.
    limit: Option<usize>,
    source: Option<String>,
}

/// The playlist meant by a spoken name, as its tracks' server ids (decision 0015). `playlist` is null when nothing
/// fits; that is an answer, not an error.
async fn playlist_by_name(State(app): State<Arc<App>>, Query(q): Query<ByNameQuery>) -> Result<Json<crate::app::ByName>, ApiError> {
    Ok(Json(app.playlists.by_name(&q.name, q.shuffle, q.limit.unwrap_or(50), q.source.as_deref()).await?))
}

#[derive(Deserialize)]
struct Spoken {
    names: Vec<String>,
}

/// The other names a playlist is spoken as, exactly these.
async fn set_spoken(State(app): State<Arc<App>>, Path(id): Path<String>, Json(b): Json<Spoken>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "undo": app.playlists.set_spoken(&id, &b.names).await? })))
}

/// What a playlist can be linked to (decision 0014).
async fn sync_targets(State(app): State<Arc<App>>) -> Json<Vec<crate::app::TargetChoice>> {
    Json(app.playlists.targets().await)
}

#[derive(Deserialize)]
struct LinkQuery {
    target: String,
}

/// Link a playlist to a target, or change how it is kept (the state wanted: safe to repeat).
async fn link_playlist(State(app): State<Arc<App>>, Path(id): Path<String>, Query(q): Query<LinkQuery>) -> Result<Json<()>, ApiError> {
    Ok(Json(app.playlists.link(&id, &q.target).await?))
}

/// Take a link off; its copy on the target is deleted.
async fn unlink_playlist(State(app): State<Arc<App>>, Path(id): Path<String>, Query(q): Query<LinkQuery>) -> Result<Json<()>, ApiError> {
    Ok(Json(app.playlists.unlink(&id, &q.target).await?))
}


async fn album_duplicates(State(app): State<Arc<App>>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "groups": app.playlists.album_duplicates().await.map_err(ToggleError::from)? })))
}

/// One action's undo tokens (decision 0026): one, or one per playlist of an action on several.
#[derive(Deserialize)]
struct UndoRequest {
    tokens: Vec<String>,
}

/// The one undo (decision 0026): every token an action returned, undone newest first. Playlist and crate changes
/// answer with their redo tokens, in the order they were made: undone newest first in turn, the redo does the action
/// again in its own order (an edit before the rename that followed it). Incoming's tokens (its event ids) answer with
/// the albums' titles.
async fn undo(State(app): State<Arc<App>>, Json(req): Json<UndoRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let (mut redo, mut restored) = (Vec::new(), Vec::new());
    for token in req.tokens.iter().rev() {
        if token.starts_with('c') {
            redo.extend(app.playlists.undo(token).await?);
        } else {
            restored.push(app.incoming.undo(token).await?);
        }
    }
    Ok(Json(json!({ "redo": redo, "restored": restored })))
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

/// A track (`id`) or several (`ids`, a crate's: decision 0024) for the queue of the player that plays (decision 0019).
#[derive(Deserialize)]
struct QueueAdd {
    id: Option<String>,
    #[serde(default)]
    ids: Vec<String>,
    mode: QueueMode,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum QueueMode {
    Next,
    Last,
}

async fn queue_add(State(app): State<Arc<App>>, Query(f): Query<Follow>, Json(q): Json<QueueAdd>) -> Result<Json<serde_json::Value>, ApiError> {
    let how = match q.mode {
        QueueMode::Next => crate::source::PlayHow::Next,
        QueueMode::Last => crate::source::PlayHow::Last,
    };
    let ids: Vec<String> = q.id.into_iter().chain(q.ids).collect();
    if ids.is_empty() {
        return Err(ApiError(ToggleError::Refused("No track to add.".into())));
    }
    Ok(Json(json!({ "player": app.playing.queue_add(f.source.as_deref(), &ids, how).await? })))
}

#[derive(Deserialize)]
struct QueueJump {
    id: String,
}

async fn queue_jump(State(app): State<Arc<App>>, Query(f): Query<Follow>, Json(q): Json<QueueJump>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "player": app.playing.queue_jump(f.source.as_deref(), &q.id).await? })))
}

/// `ids`: the tracks (the local player's queue, on `source`); without, what the server's player plays and what comes next.
#[derive(Deserialize)]
struct QueuePlaylist {
    name: String,
    ids: Option<Vec<String>>,
    /// A track the queue plays twice stays twice (the page asked); else once.
    #[serde(default)]
    repeats: bool,
}

async fn queue_playlist(
    State(app): State<Arc<App>>,
    Query(f): Query<Follow>,
    Json(q): Json<QueuePlaylist>,
) -> Result<(StatusCode, Json<crate::app::FromQueue>), ApiError> {
    let made = app.playing.playlist_from_queue(f.source.as_deref(), &q.name, q.ids, q.repeats).await?;
    Ok((StatusCode::CREATED, Json(made)))
}

async fn playlist_covers(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(f): Query<Follow>,
) -> Result<Json<Vec<crate::app::AlbumRef>>, ApiError> {
    Ok(Json(app.playing.playlist_covers(&id, f.source.as_deref()).await?))
}

/// The local player played most of the track: one play counts (decision 0022).
async fn track_played(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>) -> Result<Json<serde_json::Value>, ApiError> {
    app.playing.count_play(&source, &id).await?;
    Ok(Json(json!({ "counted": true })))
}

#[derive(Deserialize)]
struct PlayEvents {
    events: Vec<crate::app::PlayEvent>,
}

/// Plays a player tells, late too (decision 0033, step 4): each counted once; the answer names those it may forget.
async fn plays(State(app): State<Arc<App>>, Json(p): Json<PlayEvents>) -> Json<serde_json::Value> {
    Json(json!({ "done": app.plays.take(&app.playing, p.events).await }))
}

/// The local player hands its queue to the server's player (decision 0019 / 0022): these tracks, from `index`, starting
/// `position_secs` into that one.
#[derive(Deserialize)]
struct Handover {
    ids: Vec<String>,
    index: usize,
    position_secs: f64,
}

async fn handover(State(app): State<Arc<App>>, Query(f): Query<Follow>, Json(h): Json<Handover>) -> Response {
    let sources = app.library.connected_sources();
    let found = match f.source.as_deref() {
        Some(want) => sources.into_iter().find(|(s, _)| s == want),
        None => sources.into_iter().next(),
    };
    let Some((_, source)) = found else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "No server is connected." }))).into_response();
    };
    let ids = h.ids.get(h.index..).unwrap_or_default();
    if ids.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Nothing to hand over." }))).into_response();
    }
    match source.play_from(ids, h.position_secs.max(0.0)).await {
        Ok(player) => Json(json!({ "player": player })).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
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

#[cfg(test)]
mod tests {
    use super::{ExportRequest, attachment, form_decode};

    #[test]
    fn the_export_address_names_playlists_and_options() {
        let r = ExportRequest::from_query("playlist=Chill&playlist=Caf%C3%A9+Jazz&tracks=1&flat=1&combine=Chill+%2B+Jazz");
        assert_eq!(r.playlists, ["Chill", "Café Jazz"]);
        assert!(r.tracks && r.flat);
        assert_eq!(r.combine.as_deref(), Some("Chill + Jazz"));
        assert_eq!(form_decode("100%"), "100%", "a lone % stays");
        let o = ExportRequest::from_query("playlist=A&flat=1").options().ok().unwrap();
        assert!(!o.flat, "one folder only applies with tracks");
        assert!(ExportRequest::from_query("tracks=1").options().is_err(), "no playlist");
        assert!(ExportRequest::from_query("playlist=A&combine=+").options().is_err(), "a blank combined name");
    }

    #[test]
    fn a_download_name_is_given_twice_ascii_and_utf8() {
        assert_eq!(attachment("Café \"x\".zip"), "attachment; filename=\"Caf_ _x_.zip\"; filename*=UTF-8''Caf%C3%A9%20%22x%22.zip");
    }
}
