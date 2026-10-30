//! HTTP API.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use serde::Deserialize;
use serde_json::json;

use crate::app::{App, SettingsView, SourceView, StateView, StatusView, ToggleError, TrackStateView};
use crate::ctx::{ChangeAt, Ctx};
use crate::userdata::offline::{ChangeKind, Op, item_id};
use crate::settings::SourceConfig;
use crate::playlists::{Repair, Repairs};

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/state", get(state))
        .route("/api/events", get(events))
        .route("/api/events/players", get(players_events))
        .route("/api/cover/{source}/{id}", get(cover))
        .route("/api/stream/{source}/{id}", get(stream))
        .route("/api/stream/{source}/{id}/rewind", get(stream_rewind))
        .route("/api/stream/{source}/{id}/wind", get(stream_wind))
        .route("/api/file/{source}/{id}", get(file))
        .route("/api/track/{source}/{id}/gain", get(track_gain))
        .route("/api/offline", post(offline_facts))
        .route("/api/offline/prepare", post(offline_prepare))
        .route("/api/players", get(players))
        .route("/api/groups/{group}/visuals", get(group_visuals).put(put_group_visuals))
        .route("/api/visuals/snapshots", get(visual_snapshots).post(add_visual_snapshot))
        .route("/api/visuals/snapshots/{id}", axum::routing::patch(rename_visual_snapshot).delete(delete_visual_snapshot))
        .route("/api/visuals/ratings", get(visual_ratings).post(add_visual_rating))
        .route("/api/visuals/limits", get(visual_limits))
        .route("/api/players/{name}/log", get(player_log_last).post(player_log_add))
        .route("/api/players/{name}", get(player))
        .route("/api/players/{name}/command", post(player_command))
        .route("/api/players/{name}/state", axum::routing::put(player_state))
        .route("/api/players/{name}/sample", post(player_sample))
        .route("/api/players/{name}/delay", axum::routing::put(player_delay))
        .route("/api/players/{name}/shared", axum::routing::put(player_shared))
        .route("/api/groups/{leader}/listen.wav", get(group_listen))
        .route("/api/groups/{leader}/listen.pcm", get(group_frames))
        .route("/api/time", get(time_now))
        .route("/api/sync/measure", get(sync_measure))
        .route("/api/radio/next", post(radio_next))
        .route("/api/players/{name}/connect", get(player_connect))
        .route("/api/track/{source}/{id}", get(track_info))
        .route("/api/track/{source}/{id}/playlists", get(track_state))
        .route("/api/track/{source}/{id}/details", get(track_details))
        .route("/api/track/{source}/{id}/waveform", get(track_waveform))
        .route("/api/track/{source}/{id}/band-wave", get(track_band_wave))
        .route("/api/track/{source}/{id}/spectrum", get(track_spectrum))
        .route("/api/track/{source}/{id}/beats", get(track_beats))
        .route("/api/track/{source}/{id}/bands", get(track_bands))
        .route("/api/track/{source}/{id}/playlists/{playlist}", post(track_toggle))
        .route("/api/track/{source}/{id}/favorite", post(track_favorite))
        .route("/api/track/{source}/{id}/played", post(track_played))
        .route("/api/plays", post(plays))
        .route("/api/history/listening", get(history_listening))
        .route("/api/favorites/{source}", post(favorites))
        .route("/api/favorites/{source}/albums", get(favorite_albums))
        .route("/api/favorites/{source}/artists", get(favorite_artists))
        .route("/api/favorites/{source}/tracks", get(favorite_tracks))
        .route("/api/queue/add", post(queue_add))
        .route("/api/queue/jump", post(queue_jump))
        .route("/api/queue/playlist", post(queue_playlist))
        .route("/api/play", post(play_collection))
        .route("/api/playlists/{id}/covers", get(playlist_covers))
        .route("/api/playlists/{id}/similar", get(similar_playlists))
        .route("/api/playlists/{id}/tracks", get(playlist_tracks))
        .route("/api/playlists/{id}/rows", get(playlist_rows))
        .route("/api/export/preview", post(export_preview))
        .route("/api/export", get(export_download).post(export_prepare))
        .route("/api/export/{token}", get(export_token))
        .route("/api/playlists/favorites", get(playlist_favorites))
        .route("/api/crates", get(crates).post(crate_create))
        .route("/api/crates/axes", get(crate_axes))
        .route("/api/crates/preview", post(crate_preview))
        .route("/api/filter", post(filter_list))
        .route("/api/crates/{id}", get(crate_get).put(crate_update).delete(crate_delete))
        .route("/api/crates/{id}/rows", get(crate_rows))
        .route("/api/crates/{id}/similar", get(similar_crates))
        .route("/api/crates/{id}/tempo", get(crate_tempo))
        .route("/api/like-curve", get(like_curve).post(like_curve_within))
        .route("/api/tempo", post(tempo_of).put(adjust_tempo))
        .route("/api/playlists/stats", get(playlist_stats))
        .route("/api/crates/{id}/playlist", post(crate_playlist))
        .route("/api/crates/{id}/favourites", put(crate_favourites))
        .route("/api/crates/{id}/favourite-albums", put(crate_favourite_albums))
        .route("/api/crates/{id}/left-out", get(crate_left_out).post(crate_leave_out).delete(crate_put_back))
        .route("/api/crates/{id}/fit", get(crate_fit).put(set_crate_fit))
        .route("/api/crates/{id}/not-artist", post(crate_not_artist))
        .route("/api/crates/{id}/not-artist/{key}", delete(crate_not_artist_back))
        .route("/api/excluded", get(excluded).post(exclude).delete(include_again))
        .route("/api/me/excluded-artists", get(excluded_artists))
        .route("/api/me/excluded-artists/{key}", put(exclude_artist).delete(include_artist))
        .route(
            "/api/crates/{id}/image",
            get(crate_image).put(crate_image_set).delete(crate_image_remove).layer(axum::extract::DefaultBodyLimit::max(32 << 20)),
        )
        .route("/api/playlists/{id}/favorite", post(playlist_favorite))
        .route("/api/playlists/{id}/entries", put(edit_playlist))
        .route("/api/playlists/{id}/ops", post(playlist_ops))
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
        .route("/api/playlists/{id}/shared", put(share_playlist))
        .route("/api/playlists/{id}/copy", post(copy_playlist))
        .route("/api/playlists/by-name", get(playlist_by_name))
        .route("/api/playlists/{id}/spoken", put(set_spoken))
        .route("/api/album-uses/{source}", get(album_uses))
        .route("/api/artists/{source}", get(artist_view))
        .route("/api/artists/{source}/favorite", get(artist_is_favorite).post(artist_favorite))
        .route("/api/albums/{source}/{album}", get(album_view))
        .route("/api/albums/tracks", post(albums_tracks))
        .route("/api/kept", post(kept_ask))
        .route("/api/albums/{source}/{album}/cover", get(album_cover))
        .route("/api/covers/release-group/{rg}", get(release_group_cover))
        .route("/api/covers/stamps", get(cover_stamps))
        .route("/api/albums/{source}/{album}/similar", get(similar_albums))
        .route("/api/albums/{source}/{album}/play", post(play_album))
        .route("/api/albums/{source}/{album}/favorite", post(album_favorite))
        .route("/api/albums/{source}/{album}/rating", get(album_rating).post(set_album_rating))
        .route("/api/collection", get(collection))
        .route("/api/albums", get(albums))
        // Classical (0.10): its composers, a composer's records, a track's other versions; everyone's.
        .route("/api/classical/composers", get(classical_composers))
        .route("/api/classical/composers/{key}", get(classical_composer))
        .route("/api/tracks/{source}/{id}/versions", get(track_versions))
        // Works and movements (0.12, decision 0086): a composer's works, a work with its recordings; everyone's.
        .route("/api/classical/composers/{key}/works", get(composer_works))
        .route("/api/works/{mbid}", get(work_view))
        .route("/api/status", get(status))
        // What this install can do (0.11, decision 0085): behind the sign-in, as the open health check must not say.
        .route("/api/capabilities", get(capabilities))
        .route("/api/changes", get(changes))
        .route("/api/incoming", get(incoming))
        .route("/api/insights", get(insights))
        // A playlist's repairs, as its edits: everyone's while playlists are (decision 0051's third stage gives them
        // owners). The undo: a playlist's or crate's for everyone, the library's for the administrators (`undo`).
        .route("/api/repairs", get(repairs))
        .route("/api/repairs/apply", post(apply_repairs))
        .route("/api/repairs/relink", post(relink_line))
        .route("/api/undo", post(undo))
        .route("/api/batch/playlists/favorite", post(playlists_favorite))
        .route("/api/batch/playlists/delete", post(playlists_delete))
        .route("/api/batch/playlists/link", post(playlists_link))
        .route("/api/batch/playlists/rows", post(playlists_rows))

        // For everyone: the version and when it started (About), the servers and their libraries (the filter): what
        // the server's settings showed every page, without the rest of them (decision 0051).
        .route("/api/about", get(about))
        .route("/api/libraries", get(libraries))
        // The account's own, a key's too (Home Assistant tells the weather with one): Home's tiles and
        // the taste profile, an artist's photo and story.
        .route("/api/history/profile", get(taste_profile))
        .route("/api/home/digging", get(home_digging))
        .route("/api/home/artists", get(home_artists))
        .route("/api/artists/{id}/photo", get(artist_photo))
        .route("/api/artists/about", get(artist_about))
        .route("/api/artists/related", get(artist_related))
        .route("/api/search", get(search_all))
        .route("/api/search/beyond", get(search_beyond))
        .route("/api/genres/aliases", get(genre_aliases))
        .route("/api/artists/picks", get(artist_picks))
        .route("/api/crates/fits", get(crate_fits))
        .route("/api/home/heard", post(home_heard))
        .route("/api/home/weather", post(home_weather))
        .route("/api/home/moment", get(home_moment))
        .route("/api/home/new", get(home_new))
        .route("/api/home/back", get(home_back))
        .route("/api/home/week", get(home_week))
        // Each account's own: its tasks, and its listening services (decision 0068: anyone's, not the administrators'
        // only). Last.fm's way back carries its sign-in's code and needs no sign-in (`serve::access::public`).
        .route("/api/tasks", get(own_tasks))
        .route("/api/tasks/{id}", delete(stop_task))
        .route("/api/tracks/describe", post(describe_tracks))
        .route("/api/scrobble", get(scrobble_targets))
        .route("/api/scrobble/listenbrainz", post(scrobble_listenbrainz))
        .route("/api/scrobble/lastfm", post(scrobble_lastfm))
        .route("/api/scrobble/lastfm/callback/{code}", get(scrobble_lastfm_callback))
        .route("/api/scrobble/{target}", axum::routing::put(scrobble_on).delete(scrobble_forget))
        // Discover (decision 0069): each account's own.
        .route("/api/discover", get(discover))
        .route("/api/discover/refresh", post(discover_refresh))
        .route("/api/discover/around", get(discover_around))
        .route("/api/discover/around/next", post(discover_around_next))
        .route("/api/discover/want", post(discover_want))
        .route("/api/requests/want", post(want_album))
        .route("/api/artists/missing", get(artist_missing))
        .route("/api/discover/missing", get(discover_missing))
        .route("/api/musicbrainz/search", get(musicbrainz_search))
        .route("/api/discover/excluded", get(discover_excluded).delete(discover_take_back))
        .route("/api/bandcamp/find", get(bandcamp_find))
        .route("/api/discover/no", post(discover_no))
        .route("/api/discover/hide", post(discover_hide))
        .route("/api/discover/artist", get(discover_artist))
        .route("/api/discover/artist/photo", get(discover_artist_photo))
        .route("/api/discover/artist/{name}/not", post(discover_artist_not))
        .route("/api/discover/artist/albums", get(discover_artist_albums))
        .route("/api/preview/bandcamp", get(preview_bandcamp))
        .route("/api/preview/bandcamp/artist", get(preview_bandcamp_artist))
        .route("/api/requests", get(requests))
        .route("/api/spotify", get(spotify_state).delete(spotify_disconnect))
        .route("/api/spotify/client", put(spotify_client))
        .route("/api/spotify/connect", post(spotify_connect))
        .route("/api/spotify/callback", get(spotify_callback))
        .route("/api/spotify/sync", post(spotify_sync))
        .route("/api/spotify/import", delete(spotify_forget_import))
        .route("/api/spotify/albums", get(spotify_albums))
        .route("/api/spotify/tracks", get(spotify_tracks))
        .route("/api/spotify/playlists", get(spotify_playlists))
        .route("/api/spotify/playlists/{id}", get(spotify_playlist))
        .route("/api/spotify/artists", get(spotify_artists))
        .route("/api/spotify/cover/{kind}/{id}", get(spotify_cover))
        .route("/api/requests/watching", get(requests_watching))
        .route("/api/requests/decide", post(requests_decide))
        .route("/api/requests/{id}/cancel", post(requests_cancel))
        // One's own listening imported into one's own account (0.9.7, the owner: a personal concern; the account is the
        // session's, never a parameter). Another server's users' import stays the administrators' (serve::routes).
        .route("/api/history/listenbrainz", get(listenbrainz_kept).post(import_listenbrainz).delete(undo_listenbrainz))
        .route("/api/history/listenbrainz/preview", post(preview_listenbrainz))
        .route("/api/history/lastfm", get(lastfm_kept).post(import_lastfm).delete(undo_lastfm))
        .route("/api/history/lastfm/preview", post(preview_lastfm))
        .route("/api/import/file", post(import_file).delete(undo_file).layer(axum::extract::DefaultBodyLimit::max(256 << 20)))
        // The Library's Tracks, everyone's (it sat with the administrators' by mistake since 0.9.4).
        .route("/api/library/tracks", get(library_tracks))
        // The administrators' (decision 0051): the server's settings, what comes in, the library's changes and its
        // health, the folders, the diagnostics.
        .merge(
            Router::new()
            .route("/api/diagnostics", get(diagnostics_rows))
            .route("/api/diagnostics/download", get(diagnostics_download))
            .route("/api/diagnostics/levels", get(diagnostics_levels).put(set_diagnostics_levels))
            .route("/api/players/settings", get(play_on_settings).put(save_play_on_settings))
            .route("/api/library/find", get(library_find))
        .route("/api/analysis/import", post(analysis_import).layer(axum::extract::DefaultBodyLimit::disable()))
            .route("/api/playlists/{id}/link", put(link_playlist).delete(unlink_playlist))
            .route("/api/sync/targets", get(sync_targets))
            .route("/api/albums/{source}/{album}/move", get(move_preview).post(move_album))
            .route("/api/albums/{source}/{album}/delete", post(delete_album))
            .route("/api/albums/duplicates", get(album_duplicates))
            .route("/api/incoming/delete/{source}/{album}", post(delete_incoming))
            .route("/api/incoming/keep/{source}/{album}", get(keep_preview).post(keep_incoming))
            .route("/api/incoming/replace/{source}/{album}", get(replace_preview).post(replace_incoming))
            .route("/api/incoming/jobs", get(incoming_jobs))
            .route("/api/incoming/jobs/{id}/cancel", post(cancel_job))
            .route("/api/settings", get(get_settings).put(save_settings))
            .route("/api/settings/preview", post(preview))
            .route("/api/settings/lidarr", post(preview_lidarr))
            .route("/api/settings/lidarr/options", get(lidarr_options))
            .route("/api/settings/lastfm", get(get_lastfm_key).put(put_lastfm_key))
            .route("/api/settings/discogs", get(get_discogs_keys).put(put_discogs_keys))
            .route("/api/settings/musicbrainz", get(musicbrainz_status).put(musicbrainz_switch))
            .route("/api/server/restart", post(restart_server))
            .route("/api/settings/connections/state", get(connections_state))
            .route(
                "/api/albums/{source}/{album}/covers",
                get(album_covers).post(set_album_cover).put(upload_album_cover).layer(axum::extract::DefaultBodyLimit::max(13 << 20)),
            )
            .route("/api/albums/{source}/{album}/covers/look", post(look_album_covers))
            .route("/api/albums/{source}/{album}/renumber/look", post(look_renumber))
            .route("/api/albums/{source}/{album}/renumber", post(renumber_album))
            .route("/api/health/fixes", get(health_fixes))
            .route("/api/health/fixes/apply", post(apply_fixes))
            .route("/api/albums/{source}/{album}/intake", get(album_intake))
            .route("/api/albums/{source}/{album}/release", get(album_release))
            .route("/api/albums/{source}/{album}/discogs", get(album_discogs))
            // A record marked classical or not (0.10), library-wide.
            .route("/api/albums/{source}/{album}/classical", put(mark_classical))
            .route("/api/artists/discogs", get(artist_discogs))
            .route("/api/track-search", post(start_track_search))
            .route("/api/track-search/{id}", get(track_search))
            .route("/api/track-search/{id}/fetch", post(start_track_fetch))
            .route("/api/track-fetch/{id}", get(track_fetch).delete(drop_track_fetch))
            .route("/api/track-fetch/{id}/add", post(add_fetched_track))
            .route("/api/track-files", get(track_files))
            .route("/api/track-files/fetch", post(start_file_fetch))
            .route("/api/settings/slskd", get(get_slskd).put(put_slskd))
            .route("/api/settings/server-runtime", get(get_runtime).put(put_runtime))
            .route("/api/settings/acoustid", get(get_acoustid).put(put_acoustid))
            .route("/api/settings/naming", get(naming))
            .route("/api/catalogue", get(catalogue_status))
            .route("/api/health/report", get(health_report))
            .route("/api/folders", get(browse_folders))
            .route("/api/folders/status", get(folder_status))
            .route("/api/folders/suggested", get(suggested_folders))
            .route("/api/catalogue/problems", get(catalogue_problems))
                .layer(axum::middleware::from_fn(crate::serve::access::admin)),
        )
        .fallback(crate::web::serve)
        .with_state(app)
}

/// An answer slower than this (ms, until its head: a stream's body is not counted) is written down.
const SLOW_MS: u128 = 1000;

/// Every answer carries its request's id (`X-Request-Id`) and how long it took to start (`X-Response-Time-Ms`), and the
/// request runs in a `request` span with its id and the app's connection id (`X-Cratarr-Connection`), so the events it
/// logs carry both (decision 0045). Outermost: the sign-in layer's refusals have them too. First a peer that is not a
/// proxy named loses its forwarding headers (serve::proxy); then the client is counted (serve::rate); an answer slower
/// than [SLOW_MS] is a warning in the log, with what was asked (its method and route, `route_of`) and its status.
pub async fn request_ids(mut req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    use tracing::Instrument;
    let peer = req.extensions().get::<axum::extract::ConnectInfo<std::net::SocketAddr>>().map(|c| c.0);
    crate::serve::proxy::screen(req.headers_mut(), peer.map(|p| p.ip()));
    let (id, conn) = {
        let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok()).filter(|v| plain_id(v)).map(str::to_string);
        (header("x-request-id").unwrap_or_else(new_request_id), header("x-cratarr-connection"))
    };
    let client = crate::serve::signin::client(req.headers(), peer);
    crate::serve::rate::RATES.seen(&client, conn.as_deref(), req.uri().path());
    let asked = crate::serve::rate::kind_of(req.uri().path());
    let route = route_of(req.method(), req.extensions().get::<axum::extract::MatchedPath>().map(|m| m.as_str()), req.uri().path());
    let span = tracing::info_span!("request", request = %id, conn = tracing::field::Empty);
    if let Some(c) = &conn {
        span.record("conn", c.as_str());
    }
    let started = std::time::Instant::now();
    let seen = crate::memwatch::request(asked.as_str());
    let mut res = next.run(req).instrument(span.clone()).await;
    drop(seen);
    let ms = started.elapsed().as_millis();
    if ms > SLOW_MS {
        let status = res.status().as_u16();
        span.in_scope(|| tracing::warn!(target: "server", client, path = route.as_str(), status, ms = ms as u64, "slow response"));
    }
    if let Ok(v) = HeaderValue::from_str(&id) {
        res.headers_mut().insert("x-request-id", v);
    }
    res.headers_mut().insert("x-response-time-ms", HeaderValue::from(ms as u64));
    res
}

/// What a slow answer was asked for, for the log: its method and the route that took it (`GET
/// /api/stream/{source}/{id}/wind`), so a wind is not taken for a stream (0.10.2: the slow answers of 2026-10-30; the
/// path's first three parts said `/api/stream/cratarr`). Where no route took it (the web app's pages, the fallback),
/// the path's first three parts, as before. The runaway count (serve::rate) keeps its own key.
fn route_of(method: &axum::http::Method, matched: Option<&str>, path: &str) -> String {
    format!("{method} {}", matched.map_or_else(|| crate::serve::rate::kind_of(path), str::to_string))
}

/// The diagnostics log (decision 0045), off the async threads; refused while it is not open.
async fn with_log<T: Send + 'static>(f: impl FnOnce(&crate::diagnostics::Diagnostics) -> anyhow::Result<T> + Send + 'static) -> Result<T, ApiError> {
    let Some(d) = crate::diagnostics::get() else { return Err(ToggleError::Refused("The diagnostics log is not open.".into()).into()) };
    let done = tokio::task::spawn_blocking(move || f(&d)).await.map_err(anyhow::Error::from).and_then(|r| r);
    done.map_err(|e| ToggleError::Upstream(e).into())
}

/// The log, newest first, by player and area (`place`, `area`, `level`, `since`, `before`, `limit`).
async fn diagnostics_rows(Query(q): Query<crate::diagnostics::Query>) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(level) = &q.level
        && !LEVELS.contains(&level.as_str())
    {
        return Err(ToggleError::Refused(format!("No level is called {level}.")).into());
    }
    let rows = with_log(move |d| d.query(&q)).await?;
    Ok(Json(json!({ "rows": rows })))
}

/// One file for a bug report (decision 0045 item 5): the log with client addresses masked, the players, and the settings
/// without secrets; a webhook's address keeps only its scheme and host (its path is its secret).
async fn diagnostics_download(State(app): State<Arc<App>>) -> Result<Response, ApiError> {
    let log = with_log(|d| d.download()).await?;
    let settings = app.settings.view().await.map_err(ToggleError::from)?;
    let mut play_on = app.play_on.settings_view();
    for w in &mut play_on.webhooks {
        *w = w.find("://").and_then(|i| w[i + 3..].find('/').map(|j| format!("{}/…", &w[..i + 3 + j]))).unwrap_or_else(|| "…".into());
    }
    let made = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let body = json!({
        "cratarr": env!("CARGO_PKG_VERSION"),
        "made": made,
        "levels": crate::diagnostics::levels(),
        "players": app.play_on.list_for(None),
        "play_on": play_on,
        "settings": settings,
        "log": log,
    });
    let name = format!("cratarr-diagnostics-{}.json", made / 1000);
    let headers = [(header::CONTENT_TYPE, HeaderValue::from_static("application/json")), (header::CONTENT_DISPOSITION, HeaderValue::from_str(&attachment(&name)).map_err(anyhow::Error::from).map_err(ToggleError::from)?)];
    Ok((headers, body.to_string()).into_response())
}

const LEVELS: [&str; 5] = ["error", "warn", "info", "debug", "trace"];

/// What is logged now, by area: the base, and what was added for a while (until when).
fn levels_view() -> serde_json::Value {
    let now = crate::diagnostics::levels();
    let areas: serde_json::Map<String, serde_json::Value> = now
        .as_ref()
        .map(|l| l.directives.split(',').filter_map(|d| d.split_once('=')).filter(|(a, _)| crate::diagnostics::AREAS.contains(a)).map(|(a, l)| (a.to_string(), l.into())).collect())
        .unwrap_or_default();
    json!({ "areas": crate::diagnostics::AREAS, "levels": LEVELS, "now": areas, "until": now.and_then(|l| l.until) })
}

async fn diagnostics_levels() -> Json<serde_json::Value> {
    Json(levels_view())
}

#[derive(Deserialize)]
struct LevelsBody {
    /// An area's level for a while (`{"playon": "debug"}`); none: back to the base now.
    #[serde(default)]
    areas: std::collections::BTreeMap<String, String>,
    /// For how long (hours; at most a week).
    #[serde(default = "an_hour")]
    hours: u64,
}

fn an_hour() -> u64 {
    1
}

/// More (or less) logged by area for a while, then the base again by itself, without a restart (decision 0045).
async fn set_diagnostics_levels(Json(b): Json<LevelsBody>) -> Result<Json<serde_json::Value>, ApiError> {
    let mut added = Vec::new();
    for (area, level) in &b.areas {
        if !crate::diagnostics::AREAS.contains(&area.as_str()) {
            return Err(ToggleError::Refused(format!("No area is called {area}.")).into());
        }
        if !LEVELS.contains(&level.as_str()) {
            return Err(ToggleError::Refused(format!("No level is called {level}.")).into());
        }
        added.push(format!("{area}={level}"));
    }
    let holds = std::time::Duration::from_secs(b.hours.clamp(1, 24 * 7) * 3600);
    crate::diagnostics::set_levels(&added.join(","), holds).map_err(ToggleError::from)?;
    Ok(Json(levels_view()))
}

#[derive(Deserialize)]
struct LogOf {
    /// The name of the player's log (a new one each time its log starts over).
    log: String,
}

/// The last line Cratarr holds of a player's own log (decision 0045 item 2): the player sends the lines after it.
async fn player_log_last(State(app): State<Arc<App>>, Path(name): Path<String>, Query(q): Query<LogOf>) -> Result<Json<serde_json::Value>, ApiError> {
    own_player(&app, &name)?;
    let last = with_log(move |d| d.last_line(&name, &q.log)).await?;
    Ok(Json(json!({ "last": last })))
}

#[derive(Deserialize)]
struct LogLines {
    log: String,
    lines: Vec<crate::diagnostics::DeviceLine>,
}

/// Lines of a player's own log, their times on Cratarr's clock: added after the last one held, which is the answer.
async fn player_log_add(State(app): State<Arc<App>>, Path(name): Path<String>, Json(b): Json<LogLines>) -> Result<Json<serde_json::Value>, ApiError> {
    own_player(&app, &name)?;
    let last = with_log(move |d| d.add_lines(&name, &b.log, &b.lines)).await?;
    Ok(Json(json!({ "last": last })))
}

/// A player of the request's own, or one not known yet (decision 0051): another's is as if there were none; one shared
/// with everyone (0.9.8) is played on but its log stays its owner's.
fn own_player(app: &App, name: &str) -> Result<(), ApiError> {
    Ok(app.play_on.owned(name)?)
}

/// An id from outside is taken only as a short plain token: it goes into logs and headers.
fn plain_id(v: &str) -> bool {
    (1..=64).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn new_request_id() -> String {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).expect("the operating system's random source");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Open to anyone: that Cratarr answers, and which install it is (the app checks its local address is the same
/// Cratarr as its public one before the page there may talk to it), and its page's build (`web`, decision 0085: an app
/// with its own copy of the page serves the newer).
async fn health(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    let n = app.playlists.count().await.map_err(ToggleError::from)?;
    let instance = ctx.served.as_ref().and_then(|s| s.sessions.instance_id().ok());
    Ok(Json(json!({ "status": "ok", "playlists": n, "instance": instance, "web": crate::web::page_build() })))
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

/// Cratarr's own players (Play On) as server-sent events: `players`, the same JSON as `GET /api/players`, at once and on
/// every change. Unlike `/api/events` it leaves the servers' players unwatched, so a client that stays connected for
/// good (Home Assistant) costs nothing while nothing changes.
async fn players_events(
    State(app): State<Arc<App>>,
) -> axum::response::sse::Sse<impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    use tokio::sync::broadcast::error::RecvError;
    use tokio_stream::StreamExt;
    let mut changed = app.play_on.subscribe();
    // The sync groups' visuals (decision 0046, amended): each change as it comes (`visuals`), with the group's whole
    // settings; behind, every group's as it is now.
    let mut visuals = app.visuals.subscribe();
    let (out, rx) = tokio::sync::mpsc::channel::<(&'static str, String)>(8);
    tokio::spawn(crate::serve::who::carried(async move {
        let mut sent = String::new();
        loop {
            // With the account's queue requests' version (decision 0079): a request of theirs answered, or one to them,
            // reaches their pages even where no player of theirs changed.
            let requests = app.play_on.requests.version_of(crate::app::whose_players().as_deref());
            let now = json!({ "players": app.play_on.list(), "queue_requests": requests }).to_string();
            if now != sent {
                if out.send(("players", now.clone())).await.is_err() {
                    return;
                }
                sent = now;
            }
            // Behind (`Lagged`): the list now says it all.
            tokio::select! {
                r = changed.recv() => if matches!(r, Err(RecvError::Closed)) { return },
                v = visuals.recv() => {
                    let all = match v {
                        Ok(c) => vec![c],
                        Err(RecvError::Lagged(_)) => app.visuals.all(),
                        Err(RecvError::Closed) => return,
                    };
                    for c in all.into_iter().filter(|c| own_group(&app, &c.group)) {
                        if out.send(("visuals", json!(c).to_string())).await.is_err() {
                            return;
                        }
                    }
                }
                () = out.closed() => return,
            }
        }
    }));
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|(kind, d)| Ok(axum::response::sse::Event::default().event(kind).data(d)));
    axum::response::sse::Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}

fn refused(status: StatusCode, error: impl Into<String>) -> Response {
    (status, Json(json!({ "error": error.into() }))).into_response()
}

/// The visuals shared by everything that shows one player's music (decision 0046, amended): a sync group's, by its
/// leader, or a player's on its own, by its name (its own page and any page driving it, as the owner's phone drives
/// the salon tablet). 404 until one of them has started them.
async fn group_visuals(State(app): State<Arc<App>>, Path(group): Path<String>) -> Response {
    if !own_group(&app, &group) {
        return refused(StatusCode::NOT_FOUND, "That group has no visuals yet.");
    }
    match app.visuals.get(&group) {
        Some(g) => Json(g).into_response(),
        None => refused(StatusCode::NOT_FOUND, "That group has no visuals yet."),
    }
}

/// `base` 0: start the group's visuals, refused (409) where it has them already; else the version the page last had
/// (the last change wins). `writer`: the page's own id, so it can tell its own changes when they come back.
#[derive(Deserialize)]
struct GroupVisualsBody {
    base: u64,
    settings: serde_json::Map<String, serde_json::Value>,
    writer: String,
}

async fn put_group_visuals(State(app): State<Arc<App>>, Path(group): Path<String>, Json(b): Json<GroupVisualsBody>) -> Response {
    use crate::app::visuals::Refused;
    if !own_group(&app, &group) {
        return refused(StatusCode::NOT_FOUND, format!("No player or group is called {group}."));
    }
    match app.visuals.put(&group, b.base, b.settings, &b.writer) {
        Ok(version) => Json(json!({ "version": version })).into_response(),
        Err(Refused::Exists) => refused(StatusCode::CONFLICT, "That group has its visuals already."),
        Err(Refused::TooLarge) => refused(StatusCode::PAYLOAD_TOO_LARGE, "Those settings are too large to share."),
    }
}

/// Whether `group` is one of the request's own players, or a group one of them plays in (decision 0051).
fn own_group(app: &App, group: &str) -> bool {
    app.play_on.list().iter().any(|p| p.name == group || p.group.as_deref() == Some(group))
}

/// Whose snapshots Cratarr's own calls keep: the first administrator's (as its other calls act on).
fn snapshot_keeper(ctx: &Ctx) -> Result<(Arc<crate::serve::Serve>, String), Response> {
    let Some(served) = ctx.served.clone() else { return Err(refused(StatusCode::NOT_FOUND, "Cratarr's own library is not served here.")) };
    let Some(me) = ctx.account() else { return Err(refused(StatusCode::CONFLICT, "There is no account yet: make one in Settings.")) };
    Ok((served, me))
}

fn failed(e: anyhow::Error) -> Response {
    tracing::error!("visual snapshots: {e:#}");
    refused(StatusCode::INTERNAL_SERVER_ERROR, "The snapshots could not be read or written.")
}

/// The visuals' snapshots (decision 0046, amended), the newest first.
async fn visual_snapshots(ctx: Ctx) -> Response {
    let (served, me) = match snapshot_keeper(&ctx) {
        Ok(k) => k,
        Err(r) => return r,
    };
    // The owner's snapshots are every account's library (the owner, 2026-10-22); each account's own beside them.
    match served.userdata.snapshots(&me, served.users.owner().as_deref()) {
        Ok(all) => Json(all).into_response(),
        Err(e) => failed(e),
    }
}

/// The longest snapshot name kept.
const SNAPSHOT_NAME: usize = 200;

fn snapshot_name(name: &str) -> Result<String, Response> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > SNAPSHOT_NAME {
        return Err(refused(StatusCode::BAD_REQUEST, format!("A snapshot's name is 1 to {SNAPSHOT_NAME} characters.")));
    }
    Ok(name.to_string())
}

#[derive(Deserialize)]
struct NewSnapshot {
    name: String,
    /// Opaque here: whatever the page keeps (with its own version, `v`).
    settings: serde_json::Value,
    /// A small picture of it, a data URL (`snapshot_picture`).
    #[serde(default)]
    picture: Option<String>,
}

/// The largest picture a snapshot keeps (its data URL's length).
const SNAPSHOT_PICTURE: usize = 48 * 1024;

/// A snapshot's picture as sent: a JPEG or PNG data URL, small.
fn snapshot_picture(picture: Option<&str>) -> Result<Option<&str>, Response> {
    let Some(p) = picture else { return Ok(None) };
    if !(p.starts_with("data:image/jpeg;base64,") || p.starts_with("data:image/png;base64,")) {
        return Err(refused(StatusCode::BAD_REQUEST, "A snapshot's picture is a JPEG or PNG data URL."));
    }
    if p.len() > SNAPSHOT_PICTURE {
        return Err(refused(StatusCode::PAYLOAD_TOO_LARGE, "That picture is too large to keep."));
    }
    Ok(Some(p))
}

async fn add_visual_snapshot(ctx: Ctx, Json(b): Json<NewSnapshot>) -> Response {
    let (served, me) = match snapshot_keeper(&ctx) {
        Ok(k) => k,
        Err(r) => return r,
    };
    let name = match snapshot_name(&b.name) {
        Ok(n) => n,
        Err(r) => return r,
    };
    if b.settings.to_string().len() > crate::app::visuals::MOST_BYTES {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "That snapshot is too large to keep.");
    }
    let picture = match snapshot_picture(b.picture.as_deref()) {
        Ok(p) => p,
        Err(r) => return r,
    };
    match served.userdata.add_snapshot(&me, &name, &b.settings, picture) {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(e) => failed(e),
    }
}

/// A snapshot renamed, given a picture, or saved over with the settings now (any of them).
#[derive(Deserialize)]
struct SnapshotChange {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    picture: Option<String>,
    /// Opaque here, as when it was saved.
    #[serde(default)]
    settings: Option<serde_json::Value>,
}

async fn rename_visual_snapshot(ctx: Ctx, Path(id): Path<String>, Json(b): Json<SnapshotChange>) -> Response {
    let (served, me) = match snapshot_keeper(&ctx) {
        Ok(k) => k,
        Err(r) => return r,
    };
    let name = match b.name.as_deref().map(snapshot_name).transpose() {
        Ok(n) => n,
        Err(r) => return r,
    };
    let picture = match snapshot_picture(b.picture.as_deref()) {
        Ok(p) => p,
        Err(r) => return r,
    };
    if b.settings.as_ref().is_some_and(|s| s.to_string().len() > crate::app::visuals::MOST_BYTES) {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "That snapshot is too large to keep.");
    }
    match served.userdata.change_snapshot(&me, &id, name.as_deref(), picture, b.settings.as_ref()) {
        Ok(Some(snapshot)) => Json(snapshot).into_response(),
        Ok(None) => refused(StatusCode::NOT_FOUND, "There is no such snapshot."),
        Err(e) => failed(e),
    }
}

/// The owner's pairwise ratings of the visuals' behaviours (the governor's calibration), the oldest first.
async fn visual_ratings(ctx: Ctx) -> Response {
    let (served, me) = match snapshot_keeper(&ctx) {
        Ok(k) => k,
        Err(r) => return r,
    };
    match served.userdata.ratings(&me) {
        Ok(all) => Json(all).into_response(),
        Err(e) => failed(e),
    }
}

/// `a` and `b`: what was shown (their recipes, opaque here); `choice`: "a", "b", "same" or "neither".
#[derive(Deserialize)]
struct NewRating {
    a: serde_json::Value,
    b: serde_json::Value,
    choice: String,
    #[serde(default)]
    track: Option<String>,
    #[serde(default)]
    context: serde_json::Value,
}

/// The most a rating may hold, as JSON (bytes).
const RATING_BYTES: usize = 32 * 1024;

async fn add_visual_rating(ctx: Ctx, Json(b): Json<NewRating>) -> Response {
    let (served, me) = match snapshot_keeper(&ctx) {
        Ok(k) => k,
        Err(r) => return r,
    };
    if !["a", "b", "same", "neither"].contains(&b.choice.as_str()) {
        return refused(StatusCode::BAD_REQUEST, "A rating's choice is a, b, same or neither.");
    }
    if b.a.to_string().len() + b.b.to_string().len() + b.context.to_string().len() > RATING_BYTES {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "That rating is too large to keep.");
    }
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    let rating = crate::userdata::Rating { at, a: b.a, b: b.b, choice: b.choice, track: b.track, context: b.context };
    match served.userdata.add_rating(&me, &rating) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => failed(e),
    }
}

/// What the visuals learned from the owner's thumbs (decision 0048), kept as it is since decision 0057 dropped the
/// learning (read only, for whenever it returns); 404 while nothing is.
async fn visual_limits(ctx: Ctx) -> Response {
    let (served, me) = match snapshot_keeper(&ctx) {
        Ok(k) => k,
        Err(r) => return r,
    };
    match served.userdata.learned(&me) {
        Ok(Some(l)) => Json(l).into_response(),
        Ok(None) => refused(StatusCode::NOT_FOUND, "Nothing learned yet."),
        Err(e) => failed(e),
    }
}

async fn delete_visual_snapshot(ctx: Ctx, Path(id): Path<String>) -> Response {
    let (served, me) = match snapshot_keeper(&ctx) {
        Ok(k) => k,
        Err(r) => return r,
    };
    match served.userdata.delete_snapshot(&me, &id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => refused(StatusCode::NOT_FOUND, "There is no such snapshot."),
        Err(e) => failed(e),
    }
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

async fn favorite(ctx: Ctx, Query(f): Query<Favorite>) -> Result<Json<StateView>, ApiError> {
    let view = ctx.app.playing.set_favorite(f.source.as_deref(), f.on).await?;
    // Made now (decision 0087): the track is known only once it is done.
    if let Some(np) = &view.now_playing
        && let Some(w) = LastWins::of(&ctx, ChangeAt(None), ChangeKind::Favourite, item_id("track", &np.source, &np.id))
    {
        w.made(Op::of(f.on));
    }
    Ok(Json(view))
}

/// A favourite's or a rating's write, the last change winning (0.12, decision 0087): the account's, the item as the
/// change log names it, and the change's time (`X-Cratarr-At`, else now). None without an account to keep it for:
/// then every write is made, as before.
struct LastWins {
    ud: Arc<crate::userdata::UserData>,
    account: String,
    kind: ChangeKind,
    item: String,
    at: i64,
}

impl LastWins {
    fn of(ctx: &Ctx, at: ChangeAt, kind: ChangeKind, item: String) -> Option<Self> {
        let account = ctx.account()?;
        Some(Self { ud: ctx.served.as_ref()?.userdata.clone(), account, kind, item, at: at.or_now() })
    }

    /// Whether a later change was made already: this one is then not made. Not readable: made.
    fn older(&self) -> bool {
        self.ud.change_is_older(&self.account, self.kind, &self.item, self.at).unwrap_or_else(|e| {
            tracing::warn!("{}: when it last changed not read: {e:#}", self.item);
            false
        })
    }

    /// Made: its time kept, and the change logged.
    fn made(&self, op: Op) {
        if let Err(e) = self.ud.mark_changed(&self.account, self.kind, &self.item, op, self.at) {
            tracing::warn!("{}: its change not kept: {e:#}", self.item);
        }
    }
}

/// Whether `w`'s change is older than one made already (none: made).
fn is_older(w: &Option<LastWins>) -> bool {
    w.as_ref().is_some_and(LastWins::older)
}

/// An answer saying whether the change was made (decision 0087): `applied`, where the device sent its time.
fn applied(mut v: serde_json::Value, at: ChangeAt, made: bool) -> Json<serde_json::Value> {
    if at.sent()
        && let Some(o) = v.as_object_mut()
    {
        o.insert("applied".into(), made.into());
    }
    Json(v)
}

fn json_of(v: impl serde::Serialize) -> Result<serde_json::Value, ApiError> {
    serde_json::to_value(v).map_err(|e| ApiError::from(ToggleError::from(anyhow::Error::from(e))))
}

async fn toggle(State(app): State<Arc<App>>, Path(id): Path<String>, Query(f): Query<Membership>) -> Result<Json<StateView>, ApiError> {
    Ok(Json(app.playing.toggle(&id, f.source.as_deref(), f.member).await?))
}

async fn cover(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>, headers: HeaderMap) -> Response {
    use crate::covers::Outcome;
    if source == crate::bandcamp::SOURCE {
        return bandcamp_cover(&id).await;
    }
    let inm = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok());
    // Asked again each time, answered by its ETag (0.9.7): a changed cover shows at once, an unchanged one costs a 304.
    let cache = (header::CACHE_CONTROL, HeaderValue::from_static("public, no-cache"));
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
    ([cache], Json(gain_view(gain.as_ref()))).into_response()
}

/// A track's gains as the page and the app read them, with what levelling turns it down by (0.9.8: shown on the page,
/// applied by the app and the browser), by track and by album; null where the gain is not known (the page's choice for
/// unmeasured tracks decides).
fn gain_view(gain: Option<&crate::serve::api::ReplayGain>) -> serde_json::Value {
    let mut v = gain.map_or(json!({}), |g| serde_json::to_value(g).unwrap_or_default());
    v["trackDown"] = json!(gain.and_then(|g| turned_down(g.track_gain, g.track_peak)));
    v["albumDown"] = json!(gain.and_then(|g| turned_down(g.album_gain.or(g.track_gain), g.album_peak.or(g.track_peak))));
    v
}

/// Levelling's rule (decision 0035): the gain, never past the peak (no clipping) and never up: the dB a track is turned
/// down by (0 or less); none without a gain.
fn turned_down(gain: Option<f64>, peak: Option<f64>) -> Option<f64> {
    let gain = gain?;
    let room = peak.filter(|p| *p > 0.0).map_or(f64::INFINITY, |p| -20.0 * p.log10());
    Some((gain.min(room).min(0.0) * 100.0).round() / 100.0)
}

/// A track as Cratarr's library serves it: its gains and tempo from its analysis, else from its file's tags.
async fn served_song(app: &App, source: &str, id: &str) -> Option<crate::serve::api::Song> {
    let served = app.served.clone()?;
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

/// Classical's index (0.10), off the request's thread.
async fn classical_index(served: Arc<crate::serve::Serve>) -> Result<Arc<crate::app::classical::Index>, ToggleError> {
    tokio::task::spawn_blocking(move || crate::app::classical::index(&served))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))
}

/// The composers of the library's classical records (0.10): `[{key, name, sort_name, records, photo?}]`, every
/// spelling of one composer one.
async fn classical_composers(ctx: Ctx) -> Result<Json<Vec<crate::app::classical::Composer>>, ApiError> {
    let ix = classical_index(ctx.serve()?).await?;
    Ok(Json(ix.composers.clone()))
}

/// A composer's classical records (0.10): `{key, name, records: [album id]}`, by their key or one given out before
/// that leads to them; 404 for none.
async fn classical_composer(ctx: Ctx, Path(key): Path<String>) -> Response {
    let found = match ctx.serve() {
        Ok(s) => classical_index(s).await,
        Err(e) => Err(e),
    };
    match found {
        Ok(ix) => match ix.composer(&key) {
            Some(c) => Json(json!({ "key": c.key, "name": c.name, "records": c.albums })).into_response(),
            None => (StatusCode::NOT_FOUND, Json(json!({ "error": "No such composer." }))).into_response(),
        },
        Err(e) => ApiError::from(e).into_response(),
    }
}

/// A composer's works (0.12, decision 0086): `[{work, title, key?, catalogue?, parts, recordings}]`, their top-level
/// works (a work with no parent, or none known yet), the most recorded first, then by title; by their key or one given
/// out before that leads to them; 404 for none.
async fn composer_works(ctx: Ctx, Path(key): Path<String>) -> Response {
    let found = match ctx.serve() {
        Ok(s) => classical_index(s).await,
        Err(e) => Err(e),
    };
    match found {
        Ok(ix) => match ix.composer_works(&key) {
            Some(w) => Json(w).into_response(),
            None => (StatusCode::NOT_FOUND, Json(json!({ "error": "No such composer." }))).into_response(),
        },
        Err(e) => ApiError::from(e).into_response(),
    }
}

/// A work (0.12, decision 0086): `{title, composers, parent?, parts, recordings}`, each recording's tracks in the
/// work's order (play them with `POST /api/play`, `kind: tracks`); 404 for a work the library holds no track of.
async fn work_view(ctx: Ctx, Path(mbid): Path<String>) -> Response {
    let found = match ctx.serve() {
        Ok(s) => classical_index(s).await,
        Err(e) => Err(e),
    };
    match found {
        Ok(ix) => match ix.work(&mbid) {
            Some(w) => Json(w).into_response(),
            None => (StatusCode::NOT_FOUND, Json(json!({ "error": "No such work in the library." }))).into_response(),
        },
        Err(e) => ApiError::from(e).into_response(),
    }
}

#[derive(Deserialize)]
struct ClassicalMark {
    #[serde(default)]
    mark: Option<String>,
}

/// A record marked classical or not by hand (0.10; `null`: as Cratarr finds it), library-wide, outlasting every pass:
/// `{classical, classical_mark}`. The pages showing albums are told.
async fn mark_classical(ctx: Ctx, Path((source, album)): Path<(String, String)>, Json(b): Json<ClassicalMark>) -> Result<Json<serde_json::Value>, ApiError> {
    use crate::app::classical::Mark;
    if source != crate::source::cratarr::KIND.id {
        return Err(ToggleError::Refused("Only Cratarr's own library's records are marked.".into()).into());
    }
    let mark = match b.mark.as_deref() {
        None => None,
        Some(m) => Some(Mark::of(m).ok_or_else(|| ToggleError::BadSettings("A mark is classical or not.".into()))?),
    };
    let s = ctx.serve()?;
    let (classical, kept) = tokio::task::spawn_blocking(move || -> anyhow::Result<(bool, Option<&'static str>)> {
        let base = s.library.album_base(&album)?.ok_or_else(|| anyhow::anyhow!("That record is not in the library."))?;
        s.userdata.set_classical_mark(&base, mark.map(Mark::as_str), crate::app::moment::now_secs())?;
        let ix = crate::app::classical::index(&s)?;
        let r = ix.records.get(&album);
        Ok((r.is_some_and(|r| r.classical), r.and_then(|r| r.mark).map(Mark::as_str)))
    })
    .await
    .map_err(|e| ToggleError::Refused(format!("{e}")))?
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    ctx.app.collection.tell_changed();
    Ok(Json(json!({ "classical": classical, "classical_mark": kept })))
}

/// A track's Other versions (0.10): its MusicBrainz work, the work's composers, and the library's other tracks of it,
/// with their performers; 404 where the track has no work linked.
async fn track_versions(ctx: Ctx, Path((source, id)): Path<(String, String)>) -> Response {
    let none = || (StatusCode::NOT_FOUND, Json(json!({ "error": "This track has no work linked." }))).into_response();
    if source != crate::source::cratarr::KIND.id {
        return none();
    }
    let Ok(served) = ctx.serve() else { return none() };
    match classical_index(served).await {
        Ok(ix) => match ix.versions(&id, &source) {
            Some(v) => Json(v).into_response(),
            None => none(),
        },
        Err(e) => ApiError::from(e).into_response(),
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

/// A track's waveform in three bands (`analysis::band_wave`, the full-screen player's): a format byte (1), then 512
/// bins of the low band (below 150 Hz), 512 of the mid and 512 of the high (above 2.5 kHz), each its RMS as 0..=255 of
/// the loudest bin of any band. Worked out the first time it is asked for (a second or two), kept after. 404 for a
/// track Cratarr does not keep the file of; 503 when it cannot be worked out now. An ETag of its bytes.
async fn track_band_wave(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>, headers: HeaderMap) -> Response {
    let cat = app.library.catalogue();
    let c = cat.clone();
    let Some((folder, rel)) = tokio::task::spawn_blocking(move || {
        let track = c.track_of_source(&source, &id)?;
        c.file_of(track)
    })
    .await
    .ok()
    .flatten() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(wave) = crate::app::analysis_pass::band_wave(cat, folder, rel).await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    kept_bytes(wave.0, 'b', &headers)
}

/// A track's spectrum (`analysis::spectrum`, the full-screen player's bands): 32 bands 20 times a second, as kept. Worked
/// out the first time it is asked for (a few seconds) and kept after. 404 for a track Cratarr does not keep the file
/// of; 503 when it cannot be worked out now. An ETag of its bytes.
async fn track_spectrum(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>, headers: HeaderMap) -> Response {
    let cat = app.library.catalogue();
    let c = cat.clone();
    let Some((folder, rel)) = tokio::task::spawn_blocking(move || {
        let track = c.track_of_source(&source, &id)?;
        c.file_of(track)
    })
    .await
    .ok()
    .flatten() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(spectrum) = crate::app::analysis_pass::spectrum(cat, folder, rel).await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    kept_bytes(spectrum.0, 's', &headers)
}

/// Bytes kept for a track (its bands, its spectrum), with an ETag of them (`tag` before the crc) and asked again each
/// time (no-cache).
fn kept_bytes(bytes: Vec<u8>, tag: char, headers: &HeaderMap) -> Response {
    let etag = format!("\"{tag}{:08x}\"", crate::opus_copy::crc32(&bytes));
    let mut res = if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(etag.as_str()) {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        ([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response()
    };
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
    if let Ok(v) = HeaderValue::from_str(&etag) {
        h.insert(header::ETAG, v);
    }
    res
}

/// A track's beats (decision 0046): five bytes a beat, its time in milliseconds from the start (u32, little-endian), then
/// its strength (0..=255); none for a track without a tempo. Worked out when first asked for, where its analysis has
/// none yet (about a second; `app::analysis_pass::beats`). 404 for a track Cratarr does not keep the file of; 503 when
/// they cannot be worked out now. It changes with a new analysis, and once when Beat this!'s beats replace the
/// analysis' own (decision 0049, amended): an ETag of when the analysis was made and whose beats they are; kept a long
/// time once they are Beat this!'s, asked again each time until then.
async fn track_beats(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>, Query(k): Query<Kept>, headers: HeaderMap) -> Response {
    for_visuals(app, source, id, headers, false, k.stored).await
}

/// `?stored=1` (0.12, a track's menu and its snippet): only what the analysis kept, 404 where there is none, never a
/// decode now (a menu opened on each of many tracks must not queue a decode each on the server).
#[derive(Deserialize)]
struct Kept {
    #[serde(default, deserialize_with = "flag")]
    stored: bool,
}

/// `1`/`true` (or present and empty) as true; absent, `0`, `false` as false.
fn flag<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let s = <Option<String> as serde::Deserialize>::deserialize(d)?;
    Ok(s.is_some_and(|v| !matches!(v.as_str(), "0" | "false")))
}

/// A track's signals for the visuals (decision 0046, amended), as `analysis::signals::Signals` keeps them (format 3):
/// a format byte (3) and the number of sections, then each section: its id, kind (0 series, 1 hits, 2 marks, 3 the
/// band table), rate in tenths of Hz (0: a value a beat of `/beats`), channels, count (u32) and scale (10 bytes,
/// numbers little-endian), and its bytes (series: `count` frames of a byte a channel; hits: for each channel its
/// number (u32), then each one's time (ms, u32) and strength; marks: `count` times (ms, u32) with a byte each; the band
/// table: each band's low and high edge, Hz, u16, 0 open). The sections:
/// - 0 the bands (sub, bass, low-mid, mid, presence, air);
/// - 1 the loudness as heard, 10 a second (EBU R128 momentary, thirds of a dB from -70 LUFS: the file's own, before
///   any levelling);
/// - each band: 2 level (20 a second), 3 flux (20), 4 hits, 5 percussive and 6 harmonic level (10), 7 percussive hits,
///   8 crest (5), 9 width (5), 18 swell (10);
/// - the whole track: 10 spectral centroid (10), 11 chroma (5 × 12), 12 harmonic change (10) and 13 its peaks, 14
///   novelty (a value a beat, or 2 a second without beats) and 15 section boundaries, 16 each beat's place in its bar
///   (with how sure, in its scale byte; none without enough beats), 17 swell and 19 build-up (10).
///
/// A page skips the sections it does not know. Empty where the decode did not cover the whole file. Worked out and kept
/// as `track_beats`, in the same decode; served under its old name, `bands`.
async fn track_bands(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>, Query(k): Query<Kept>, headers: HeaderMap) -> Response {
    for_visuals(app, source, id, headers, true, k.stored).await
}

/// A track's beats, or its bands: as kept or worked out now (`app::analysis_pass::visual`).
async fn for_visuals(app: Arc<App>, source: String, id: String, headers: HeaderMap, bands: bool, stored: bool) -> Response {
    let cat = app.library.catalogue();
    let c = cat.clone();
    let file = tokio::task::spawn_blocking(move || {
        let track = c.track_of_source(&source, &id)?;
        c.file_of(track)
    })
    .await
    .ok()
    .flatten();
    let Some((folder, rel)) = file else { return StatusCode::NOT_FOUND.into_response() };
    // Only what is kept: none is a 404, not a decode.
    if stored {
        let (c, r) = (cat.clone(), rel.clone());
        let kept = tokio::task::spawn_blocking(move || crate::app::analysis_pass::kept(&c, folder, &r)).await.unwrap_or((None, None));
        let kept = if bands { kept.1.map(|s| s.0) } else { kept.0.map(|b| b.0) };
        if kept.is_none() {
            return StatusCode::NOT_FOUND.into_response();
        }
    }
    // Not now: the file did not decode, or its turn or decode took too long; the page may ask again later.
    let got = if bands {
        crate::app::analysis_pass::signals(cat.clone(), folder, rel.clone()).await.map(|b| b.0)
    } else {
        crate::app::analysis_pass::beats(cat.clone(), folder, rel.clone()).await.map(|b| b.0)
    };
    let Some(bytes) = got else { return StatusCode::SERVICE_UNAVAILABLE.into_response() };
    let (stamp, final_beats) =
        tokio::task::spawn_blocking(move || (cat.analysis_stamp(folder, &rel), cat.beats_are_beat_this(folder, &rel))).await.unwrap_or_default();
    // The bands' form in theirs: a copy kept from another form never answers for this one; nor one from before Beat
    // this!'s beats (decision 0049, amended).
    let kind = if bands { format!("s{}", crate::analysis::signals::FORMAT) } else { "b".to_string() };
    let by = if final_beats { "-bt" } else { "" };
    let etag = stamp.map(|(at, version)| format!("\"{kind}-{version}-{at}{by}\""));
    if etag.is_some() && headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == etag.as_deref() {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    let mut res = ([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response();
    let h = res.headers_mut();
    if let Some(v) = etag.and_then(|e| HeaderValue::from_str(&e).ok()) {
        // The analysis' own beats may yet give way to Beat this!'s: asked again each time (a 304 while they stay).
        let keep = if final_beats { "private, max-age=2592000" } else { "private, no-cache" };
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static(keep));
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

async fn track_details(ctx: Ctx, Path((source, id)): Path<(String, String)>) -> Response {
    let app = ctx.app.clone();
    if source == crate::bandcamp::SOURCE {
        return bandcamp_details(&id).await;
    }
    // Its tempo as the library serves it (the BPM tag, else the analysis').
    let bpm = served_song(&app, &source, &id).await.and_then(|s| s.bpm);
    let cat = app.library.catalogue();
    // Whose plays these are: the listener's (read here, as the blocking work below has no request of its own).
    let listener = ctx.served.clone().and_then(|s| ctx.account().map(|u| (s, u)));
    let read = tokio::task::spawn_blocking(move || -> Option<serde_json::Value> {
        let track = cat.track_of_source(&source, &id)?;
        let (folder, rel) = cat.file_of(track)?;
        let root = cat.folders().get(folder as usize)?.clone();
        let path = root.join(&rel);
        let meta = std::fs::metadata(&path).ok();
        let modified = meta.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs());
        let (file, _) = crate::catalogue::tags::read(&path).ok()?;
        let tags: Vec<serde_json::Value> = file.items.iter().map(|(raw, name, value)| json!({ "name": name.unwrap_or(raw), "value": value })).collect();
        // The listener's plays of it, and AudioMuse's facts (decision 0054): energy, key, style, mood, danceability.
        let song = crate::serve::ids::song(track);
        let plays = listener
            .as_ref()
            .and_then(|(s, u)| s.userdata.get(u, crate::userdata::Kind::Song, std::slice::from_ref(&song)).ok())
            .and_then(|mut m| m.remove(&song));
        // Its moods: those it stands out on against the library's (the crates' facts, kept).
        let stand_out = crate::app::audiomuse::stand_out(cat.crate_facts().ok().into_iter().flat_map(|f| f.iter().filter_map(|x| x.clap_moods).collect::<Vec<_>>()));
        let mut muse = crate::app::audiomuse::details(cat.audiomuse_of(track).as_ref(), &stand_out);
        let muse = muse.as_object_mut().map(std::mem::take).unwrap_or_default();
        let mut details = json!({
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
            "play_count": plays.as_ref().map(|a| a.play_count),
            "last_played": plays.as_ref().and_then(|a| a.play_date),
        });
        if let Some(d) = details.as_object_mut() {
            d.extend(muse);
        }
        Some(details)
    });
    match read.await.ok().flatten() {
        Some(details) => Json(details).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "That track's file cannot be read." }))).into_response(),
    }
}

/// A Bandcamp preview's details (decision 0071): no file of Cratarr's, so what its album's page says of it, the stream
/// as Bandcamp serves it (MP3, 128 kb/s), its page in place of a path (the owner, 2026-10-25: Info failed on it).
async fn bandcamp_details(id: &str) -> Response {
    let found = match crate::bandcamp::read_id(id) {
        Some((url, n)) => crate::bandcamp::Bandcamp::get().album(&url).await.ok().map(|a| (a, n)),
        None => None,
    };
    let Some((a, n)) = found else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "That track's page on Bandcamp cannot be read." }))).into_response();
    };
    let t = a.tracks.iter().find(|t| t.n == n);
    let mut tags = vec![
        json!({ "name": "Title", "value": t.map(|t| t.title.clone()).unwrap_or_default() }),
        json!({ "name": "Artist", "value": t.and_then(|t| t.artist.clone()).unwrap_or_else(|| a.artist.clone()) }),
        json!({ "name": "Album", "value": a.title }),
    ];
    if n > 0 {
        tags.push(json!({ "name": "Track", "value": n.to_string() }));
    }
    Json(json!({
        "path": t.and_then(|t| t.page.clone()).unwrap_or_else(|| a.url.clone()),
        "folder": null,
        "format": "MP3",
        "bitrate_kbps": crate::bandcamp::BYTES_PER_SEC * 8 / 1000,
        "sample_rate_hz": null,
        "bit_depth": null,
        "channels": null,
        "duration_ms": t.and_then(|t| t.duration_secs).map(|s| (s * 1000.0).round() as u64),
        "bpm": null,
        "size": null,
        "modified": null,
        "added": null,
        "genres": [],
        "tags": tags,
    }))
    .into_response()
}

/// An album's view in the panel (decision 0023), and its tracks for the local player (decision 0022).
/// An album as the panel shows it. One that left Incoming is followed: where it is now, or what became of it.
/// Kept on this device (decision 0038): what the app's kept items hold now, with their versions.
async fn kept_ask(State(app): State<Arc<App>>, Json(ask): Json<crate::app::kept::Ask>) -> Json<serde_json::Value> {
    Json(json!({ "items": crate::app::kept::ask(&app, ask).await }))
}

#[derive(Deserialize)]
struct AlbumRef {
    source: String,
    id: String,
}

#[derive(Deserialize)]
struct AlbumsTracks {
    albums: Vec<AlbumRef>,
}

/// Most albums one call reads.
const ALBUMS_TRACKS_MOST: usize = 20_000;

/// Several albums' tracks in one call (0.9.4, the owner: Select › All's menu on 2,884 albums said "cannot reach"):
/// the albums in the order given, each one's in play order; Cratarr's own library read in one go, a few queries.
async fn albums_tracks(State(app): State<Arc<App>>, Json(b): Json<AlbumsTracks>) -> Result<Json<serde_json::Value>, ApiError> {
    if b.albums.len() > ALBUMS_TRACKS_MOST {
        return Err(ToggleError::Refused(format!("At most {ALBUMS_TRACKS_MOST} albums at once.")).into());
    }
    let albums: Vec<(String, String)> = b.albums.into_iter().map(|a| (a.source, a.id)).collect();
    Ok(Json(json!({ "tracks": app.playing.tracks_of_albums(&albums).await })))
}

async fn album_view(ctx: Ctx, Path((source, album)): Path<(String, String)>) -> Result<Json<crate::app::AlbumView>, ApiError> {
    use crate::app::AlbumFate;
    let app = ctx.app.clone();
    if source == crate::bandcamp::SOURCE {
        return Ok(Json(bandcamp_album_view(&ctx, &album).await?));
    }
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

/// A Bandcamp album in the panel (decision 0071, the owner 2026-10-25: "I don't see why not"): its tracks as its page
/// gives them, each played through the relay as a preview's; the ones Bandcamp does not stream shown, not playable.
async fn bandcamp_album_view(ctx: &Ctx, album: &str) -> Result<crate::app::AlbumView, ToggleError> {
    let (url, _) = crate::bandcamp::read_id(album).ok_or_else(|| ToggleError::Refused("That is not an album on Bandcamp.".into()))?;
    let a = crate::bandcamp::Bandcamp::get().album(&url).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    // What Discover knows of it, where it suggested it (the owner, 2026-10-25: "as much as our own panel").
    let known = match ctx.served.clone().and_then(|s| ctx.account().map(|a| (s, a))) {
        Some((s, account)) => {
            let page = a.url.clone();
            tokio::task::spawn_blocking(move || crate::app::discover::suggestion_on_bandcamp(&s, &account, &page)).await.ok().and_then(Result::ok).flatten()
        }
        None => None,
    };
    let year = a
        .released
        .as_deref()
        .and_then(|d| d.get(..4)?.parse::<u32>().ok())
        .or_else(|| known.as_ref().and_then(|k| k.year).and_then(|y| u32::try_from(y).ok()))
        .filter(|y| *y > 0);
    let genres = known.as_ref().map(|k| k.genres.clone()).unwrap_or_default();
    let tags: Vec<String> = a.tags.iter().filter(|t| !genres.iter().any(|g| g.eq_ignore_ascii_case(t))).cloned().collect();
    let mut links = vec![crate::app::discover::Link { name: "Bandcamp".into(), url: a.url.clone() }];
    links.extend(known.as_ref().map(|k| k.links.clone()).unwrap_or_default().into_iter().filter(|l| l.name != "Bandcamp"));
    let more = crate::app::AlbumMore {
        released: a.released.clone(),
        label: a.label.clone(),
        about: a.about.clone(),
        credits: a.credits.clone(),
        tags,
        links,
        listeners: known.as_ref().and_then(|k| k.listeners),
    };
    let rows = a
        .tracks
        .iter()
        .map(|t| {
            let id = crate::bandcamp::track_id(&a.url, t.n);
            crate::app::PlaylistRow {
                entry: id.clone(),
                file: t.title.clone(),
                id: t.stream.is_some().then_some(id),
                name: Some(t.title.clone()),
                artist: Some(t.artist.clone().unwrap_or_else(|| a.artist.clone())),
                album: Some(a.title.clone()),
                duration_secs: t.duration_secs,
                track: Some(t.n),
                album_id: Some(album.to_string()),
                missing: t.stream.is_none(),
                ..Default::default()
            }
        })
        .collect();
    Ok(crate::app::AlbumView {
        source: crate::bandcamp::SOURCE.into(),
        id: album.to_string(),
        title: a.title,
        artist: Some(a.artist),
        year,
        genres,
        format: Some("MP3".into()),
        bitrate_kbps: Some((crate::bandcamp::BYTES_PER_SEC * 8 / 1000) as u32),
        rows,
        added: None,
        played: None,
        plays: None,
        more,
    })
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

/// A track's heart, the last change winning (decision 0087): an older one answers the state as it stands.
async fn track_favorite(
    ctx: Ctx,
    at: ChangeAt,
    Path((source, id)): Path<(String, String)>,
    Query(f): Query<Favorite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let w = LastWins::of(&ctx, at, ChangeKind::Favourite, item_id("track", &source, &id));
    if is_older(&w) {
        let now: TrackStateView = ctx.app.playing.track_state(&source, &id).await?;
        return Ok(applied(json_of(now)?, at, false));
    }
    let view = ctx.app.playing.track_favorite(&source, &id, f.on).await?;
    if let Some(w) = &w {
        w.made(Op::of(f.on));
    }
    Ok(applied(json_of(view)?, at, true))
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

/// The artist panel's heart (`?name=&on=`), the last change winning (decision 0087).
async fn artist_favorite(
    ctx: Ctx,
    at: ChangeAt,
    Path(source): Path<String>,
    Query(q): Query<ArtistName>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let w = LastWins::of(&ctx, at, ChangeKind::Favourite, item_id("artist", &source, q.name.trim()));
    if is_older(&w) {
        return Ok(applied(json!({ "favorite": ctx.app.playing.artist_is_favorite(&source, &q.name).await? }), at, false));
    }
    let favorite = ctx.app.playing.artist_favorite(&source, &q.name, q.on).await?;
    if let Some(w) = &w {
        w.made(Op::of(q.on));
    }
    Ok(applied(json!({ "favorite": favorite }), at, true))
}

/// Every album the owner played or rated on a server, by id: plays, last played, rating (the Library's orders and
/// filters, decided 2026-10-16).
async fn album_uses(
    State(app): State<Arc<App>>,
    Path(source): Path<String>,
) -> Result<Json<std::collections::BTreeMap<String, crate::source::AlbumUse>>, ApiError> {
    Ok(Json(app.playing.album_uses(&source).await?))
}

/// The album panel's heart: an album a favourite or not, the last change winning (decision 0087).
async fn album_favorite(
    ctx: Ctx,
    at: ChangeAt,
    Path((source, album)): Path<(String, String)>,
    Query(f): Query<Favorite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let w = LastWins::of(&ctx, at, ChangeKind::Favourite, item_id("album", &source, &album));
    if is_older(&w) {
        let now = ctx.app.playing.favorite_albums(&source).await?.contains(&album);
        return Ok(applied(json!({ "favorite": now }), at, false));
    }
    let favorite = ctx.app.playing.album_favorite(&source, &album, f.on).await?;
    if let Some(w) = &w {
        w.made(Op::of(f.on));
    }
    Ok(applied(json!({ "favorite": favorite }), at, true))
}

/// An album's rating, 0 to 5 (`null`: the server keeps none): the album menu's stars.
async fn album_rating(State(app): State<Arc<App>>, Path((source, album)): Path<(String, String)>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "rating": app.playing.album_rating(&source, &album).await? })))
}

#[derive(Deserialize)]
struct Stars {
    stars: u8,
}

/// Rates an album, 0 (none) to 5, the last change winning (decision 0087).
async fn set_album_rating(
    ctx: Ctx,
    at: ChangeAt,
    Path((source, album)): Path<(String, String)>,
    Query(q): Query<Stars>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let w = LastWins::of(&ctx, at, ChangeKind::Rating, item_id("album", &source, &album));
    if is_older(&w) {
        return Ok(applied(json!({ "rating": ctx.app.playing.album_rating(&source, &album).await? }), at, false));
    }
    let rating = ctx.app.playing.set_album_rating(&source, &album, q.stars).await?;
    if let Some(w) = &w {
        w.made(Op::of(rating > 0));
    }
    Ok(applied(json!({ "rating": rating }), at, true))
}

/// Home's Favourite tracks: the favourite tracks of Cratarr's own library, as rows, at most `FAVOURITE_ROWS` (the
/// owner, 2026-10-24). Another server's are not offered (their ids are not Cratarr's lines).
async fn favorite_tracks(State(app): State<Arc<App>>, Path(source): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    const FAVOURITE_ROWS: usize = 100;
    if source != crate::source::cratarr::KIND.id {
        return Err(ToggleError::Refused("Only Cratarr's own library lists its favourite tracks here.".into()).into());
    }
    let mut ids = app.playing.favorite_tracks(&source).await?;
    ids.truncate(FAVOURITE_ROWS);
    Ok(Json(json!({ "source": source, "rows": app.playlists.song_rows(ids).await? })))
}

/// The favourite albums of a connected server (their ids): the Library's Favourites only and the album panel's heart.
async fn favorite_albums(State(app): State<Arc<App>>, Path(source): Path<String>) -> Result<Json<Vec<String>>, ApiError> {
    Ok(Json(app.playing.favorite_albums(&source).await?))
}

/// The favourite artists of a connected server, by name (0.9.15-1): the Library's Artists, Favourites only.
async fn favorite_artists(State(app): State<Arc<App>>, Path(source): Path<String>) -> Result<Json<Vec<String>>, ApiError> {
    Ok(Json(app.playing.favorite_artists(&source).await?))
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
        let arrange = if self.tracks && self.flat { crate::export::Arrange::Flat } else { crate::export::Arrange::Library };
        Ok(crate::export::Options { tracks: self.tracks, arrange, combine: combined(self.combine.as_deref())?, ..Default::default() })
    }
}

/// The combined playlist's name, trimmed; an empty one is no name.
fn combined(c: Option<&str>) -> Result<Option<String>, ApiError> {
    match c.map(str::trim) {
        Some("") => Err(ToggleError::BadName.into()),
        c => Ok(c.map(str::to_string)),
    }
}

/// An export (decision 0058, one step for every object): what (playlists by name, or a list of tracks by their
/// servers' ids with a name), its format and its layout. For playlists, with their tracks or only the playlist files,
/// and as they are or combined into one; for tracks, with a playlist file of them or not.
#[derive(Deserialize, Clone)]
struct Export {
    what: ExportWhat,
    #[serde(default)]
    format: ExportFormat,
    /// Opus's, kb/s: 96, 128, 160 (the default) or 256.
    #[serde(default)]
    bitrate: Option<u32>,
    #[serde(default)]
    layout: ExportLayout,
    #[serde(default = "yes")]
    tracks: bool,
    #[serde(default)]
    combine: Option<String>,
    #[serde(default)]
    m3u: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum ExportWhat {
    Playlists { names: Vec<String> },
    Tracks { name: String, tracks: Vec<ExportTrack> },
}

#[derive(Deserialize, Clone)]
struct ExportTrack {
    source: String,
    id: String,
}

#[derive(Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "kebab-case")]
enum ExportFormat {
    #[default]
    Original,
    FlacCd,
    Opus,
    #[serde(rename = "mp3-320")]
    Mp3Cbr320,
    #[serde(rename = "mp3-v0")]
    Mp3V0,
}

#[derive(Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "lowercase")]
enum ExportLayout {
    Flat,
    #[default]
    Folders,
}

impl Export {
    fn options(&self) -> Result<crate::export::Options, ApiError> {
        use crate::export::{Arrange, Format, OPUS_RATES, Options};
        let format = match self.format {
            ExportFormat::Original => Format::Original,
            ExportFormat::FlacCd => Format::FlacCd,
            ExportFormat::Opus => match self.bitrate.unwrap_or(160) {
                kbps if OPUS_RATES.contains(&kbps) => Format::Opus(kbps),
                _ => return Err(ToggleError::Refused("Choose 96, 128, 160 or 256 kb/s.".into()).into()),
            },
            ExportFormat::Mp3Cbr320 => Format::Mp3Cbr320,
            ExportFormat::Mp3V0 => Format::Mp3V0,
        };
        let arrange = match self.layout {
            ExportLayout::Flat => Arrange::Flat,
            ExportLayout::Folders => Arrange::Folders,
        };
        Ok(match &self.what {
            ExportWhat::Playlists { names } => {
                if names.is_empty() {
                    return Err(ToggleError::Refused("Choose at least one playlist.".into()).into());
                }
                Options { tracks: self.tracks, arrange, combine: combined(self.combine.as_deref())?, format, lists: true }
            }
            ExportWhat::Tracks { tracks, .. } => {
                if tracks.is_empty() {
                    return Err(ToggleError::Refused("There are no tracks to export.".into()).into());
                }
                Options { tracks: true, arrange, combine: None, format, lists: self.m3u }
            }
        })
    }

    async fn plan(&self, app: &App) -> Result<crate::export::Plan, ApiError> {
        Ok(match &self.what {
            ExportWhat::Playlists { names } => app.playlists.export_plan(names).await?,
            ExportWhat::Tracks { name, tracks } => {
                let name = if name.trim().is_empty() { "Tracks" } else { name.trim() };
                app.playlists.export_tracks(name, tracks.iter().map(|t| (t.source.clone(), t.id.clone())).collect()).await?
            }
        })
    }
}

/// The preview's body: an export of decision 0058, or the Export tab's playlists (decision 0007).
#[derive(Deserialize)]
#[serde(untagged)]
enum PreviewOf {
    Export(Export),
    Playlists(ExportRequest),
}

/// The exports prepared for a download, by token.
static EXPORTS: std::sync::LazyLock<std::sync::Mutex<crate::export::Tokens<Export>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(crate::export::Tokens::new()));

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// An export made ready for its download (decision 0058): checked, then kept for a few minutes under a token; the
/// download is `GET /api/export/{token}`, a plain link, so the browser shows its progress. The answer names the
/// download (`name`): the zip's, or the one file's own when the export is a single file (`export::download`; a lone
/// track's header is read for it, to know whether it converts).
async fn export_prepare(State(app): State<Arc<App>>, Json(e): Json<Export>) -> Result<Json<serde_json::Value>, ApiError> {
    let o = e.options()?;
    let plan = e.plan(&app).await?;
    let name = tokio::task::spawn_blocking(move || crate::export::download(&plan, &o, |t| crate::export::kind_of(&t.path)).name().to_string())
        .await
        .map_err(|e| ToggleError::Upstream(anyhow::anyhow!("{e}")))?;
    let token = crate::export::new_token();
    let answer = json!({ "token": token, "name": name });
    EXPORTS.lock().unwrap_or_else(|p| p.into_inner()).put(token, unix_now(), e);
    Ok(Json(answer))
}

/// A prepared export's download; 404 once its token has expired.
async fn export_token(State(app): State<Arc<App>>, Path(token): Path<String>) -> Result<Response, ApiError> {
    let Some(e) = EXPORTS.lock().unwrap_or_else(|p| p.into_inner()).get(&token, unix_now()) else {
        return Ok(refused(StatusCode::NOT_FOUND, "That export has expired. Start it again."));
    };
    let o = e.options()?;
    let plan = e.plan(&app).await?;
    export_response(&app, plan, o).await
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

/// An export's size before starting: its tracks, the missing ones left out, and with tracks its bytes (about, for a
/// format that converts: `estimate`).
async fn export_preview(State(app): State<Arc<App>>, Json(body): Json<PreviewOf>) -> Result<Json<crate::export::Preview>, ApiError> {
    let (plan, o) = match body {
        PreviewOf::Export(e) => {
            let o = e.options()?;
            (e.plan(&app).await?, o)
        }
        PreviewOf::Playlists(r) => {
            let o = r.options()?;
            (app.playlists.export_plan(&r.playlists).await?, o)
        }
    };
    Ok(Json(crate::export::preview(&plan, &o)))
}

/// The Export tab's download (decision 0007), its playlists and options in the address.
async fn export_download(State(app): State<Arc<App>>, axum::extract::RawQuery(q): axum::extract::RawQuery) -> Result<Response, ApiError> {
    let r = ExportRequest::from_query(q.as_deref().unwrap_or(""));
    let o = r.options()?;
    let plan = app.playlists.export_plan(&r.playlists).await?;
    export_response(&app, plan, o).await
}

/// The export itself, as a download (decision 0058: several files are a zip, one file is that file; `export::download`
/// decides): two files or more as one streamed `.zip`, its length told when every file goes as it is; one alone as
/// itself, under its own name and type: a playlist file or the report from memory, a track as it is or converted first
/// (made in the work folder, then sent with its length and removed). A lone track that cannot be converted is refused:
/// its name already says the format asked for, and a file alone has no report to say why it went as it was.
async fn export_response(app: &App, plan: crate::export::Plan, o: crate::export::Options) -> Result<Response, ApiError> {
    use crate::export::Download;
    let joined = |e: tokio::task::JoinError| ToggleError::Upstream(anyhow::anyhow!("{e}"));
    let (plan, o, download) = tokio::task::spawn_blocking(move || {
        let d = crate::export::download(&plan, &o, |t| crate::export::kind_of(&t.path));
        (plan, o, d)
    })
    .await
    .map_err(joined)?;
    let work = app.playlists.export_work();
    Ok(match download {
        Download::Zip { name } => {
            let (len, rx) = crate::export::stream(plan, o, work, unix_now());
            (download_headers(&name, len), axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).into_response()
        }
        Download::Text { name, body } => (download_headers(&name, Some(body.len() as u64)), body).into_response(),
        Download::Track { at, how, name } => {
            let (t, rt) = (plan.tracks[at].clone(), tokio::runtime::Handle::current());
            match tokio::task::spawn_blocking(move || crate::export::send_track(&t, how, &work, &rt)).await.map_err(joined)? {
                Ok((len, rx)) => {
                    (download_headers(&name, Some(len)), axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).into_response()
                }
                Err(e) => {
                    tracing::warn!("Export: {name}: {e:#}");
                    refused(StatusCode::INTERNAL_SERVER_ERROR, not_sent(how))
                }
            }
        }
    })
}

/// Why a lone track was not sent: it could not be converted to the format asked for, or (as it is) not read.
fn not_sent(how: crate::export::Treatment) -> &'static str {
    use crate::export::Treatment;
    match how {
        Treatment::AsIs => "That track's file could not be read.",
        Treatment::CdFlac => "That track could not be converted to CD quality.",
        Treatment::Opus(_) => "That track could not be converted to Opus.",
        Treatment::Mp3(_) => "That track could not be converted to MP3.",
    }
}

/// An export download's headers: its name (the zip's, or the one file's own), its type by that name's extension, and
/// its length when it is known.
fn download_headers(name: &str, len: Option<u64>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&attachment(name)) {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(crate::export::content_type(name)));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(len) = len {
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    }
    headers
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

#[derive(Deserialize)]
struct TracksQuery {
    #[serde(default)]
    q: String,
    /// The Library's filter (0.9.15-1): its switches, and its conditions as JSON (a crate's, decision 0061).
    #[serde(default)]
    favourites: bool,
    #[serde(default)]
    unplayed: bool,
    #[serde(default)]
    top_rated: bool,
    #[serde(default)]
    filter: Option<String>,
    #[serde(default)]
    order: Option<String>,
    #[serde(default)]
    down: bool,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    limit: Option<usize>,
}

/// The Library page's Tracks (0.9.4): Cratarr's own library's tracks, a page of them (`offset`, `limit` up to 500) in
/// an `order` (title, artist, album, added, year, plays, duration; `down` the other way), filtered by the words `q`,
/// from the libraries switched on (`without=<source>/<library>`, as for crates); with how many in all.
async fn library_tracks(State(app): State<Arc<App>>, Query(t): Query<TracksQuery>, RawQuery(raw): RawQuery) -> Result<Json<serde_json::Value>, ApiError> {
    let without = without_of(raw.as_deref());
    let conditions = match t.filter.as_deref().filter(|f| !f.trim().is_empty()) {
        Some(f) => serde_json::from_str(f).map_err(|e| ToggleError::Refused(format!("That filter cannot be read: {e}.")))?,
        None => Vec::new(),
    };
    let narrow = crate::app::tracks::Narrow { favourites: t.favourites, unplayed: t.unplayed, top_rated: t.top_rated, conditions };
    let listed = crate::app::tracks::list(&app, &t.q, t.order.as_deref().unwrap_or("album"), t.down, &without, narrow, t.offset, t.limit.unwrap_or(100)).await?;
    Ok(Json(json!({ "items": listed.rows, "total": listed.total, "all": listed.all, "offset": t.offset })))
}

/// The libraries a crate leaves out (decision 0024, amended 2026-10-23: a crate draws from every library switched on):
/// `without=<source>/<library>`, once for each, as the page's library switches keep them. None: every library.
fn without_of(query: Option<&str>) -> Vec<String> {
    let pairs = query.unwrap_or("").split('&').filter_map(|p| p.split_once('='));
    pairs.filter(|(k, _)| form_decode(k) == "without").map(|(_, v)| form_decode(v)).filter(|v| !v.is_empty()).collect()
}

/// The permanent crates and their sizes (decision 0024), from the libraries switched on (`without`).
async fn crates(State(app): State<Arc<App>>, Query(f): Query<Follow>, RawQuery(q): RawQuery) -> Result<Json<Vec<crate::app::CrateView>>, ApiError> {
    Ok(Json(app.playlists.crates(f.source.as_deref(), &without_of(q.as_deref())).await?))
}

/// A page of a crate's rows: `offset` and `limit` (at most 500, default 200; the page says the limit it kept); its
/// libraries switched off come as `without` (`without_of`), the filter on it (decision 0061) as `filter`, its
/// conditions as JSON.
#[derive(Deserialize)]
struct Page {
    source: Option<String>,
    #[serde(default)]
    offset: usize,
    limit: Option<usize>,
    /// The favourites toggle's setting the page shows (it may be on its way to the server); the saved one without.
    favourites: Option<crate::crates::Favourites>,
    /// The favourite albums toggle's, likewise (decision 0024, amended 2026-10-19).
    favourite_albums: Option<crate::crates::Favourites>,
    filter: Option<String>,
    /// The order the list shows (decision 0060: own, title, artist, albumArtist, album, year, track, length, tempo,
    /// plays, lastPlayed, added), `down` turned round; applied before the rows are paged.
    #[serde(default)]
    order: crate::app::CrateOrder,
    #[serde(default)]
    down: bool,
}

impl Page {
    /// How the page looks at the crate: its toggles, the libraries switched off (the raw query's `without`) and the
    /// filter on it.
    fn look(&self, query: Option<&str>) -> Result<crate::app::CrateLook, ApiError> {
        let filter = match self.filter.as_deref().filter(|f| !f.trim().is_empty()) {
            Some(f) => serde_json::from_str(f).map_err(|e| ToggleError::Refused(format!("That filter cannot be read: {e}.")))?,
            None => Vec::new(),
        };
        Ok(crate::app::CrateLook {
            favourites: self.favourites,
            favourite_albums: self.favourite_albums,
            without: without_of(query),
            filter,
            order: self.order,
            down: self.down,
        })
    }
}

async fn crate_rows(State(app): State<Arc<App>>, Path(id): Path<String>, Query(q): Query<Page>, RawQuery(raw): RawQuery) -> Result<Response, ApiError> {
    let limit = q.limit.unwrap_or(200).clamp(1, 500);
    Ok(Json(app.playlists.crate_rows(&id, q.source.as_deref(), q.offset, limit, &q.look(raw.as_deref())?).await?).into_response())
}

/// A crate's tempo spread (decision 0041): its tracks' 5-BPM bands, counted on the server.
async fn crate_tempo(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(q): Query<Page>,
    RawQuery(raw): RawQuery,
) -> Result<Json<crate::crates::TempoBands>, ApiError> {
    Ok(Json(app.playlists.crate_tempo(&id, q.source.as_deref(), &q.look(raw.as_deref())?).await?))
}

#[derive(serde::Deserialize)]
struct LikeCurveQuery {
    /// The seeds, comma-separated: song ids, and (0.11, quick crates) albums' and artists' (`al-…`, `ar-…`).
    of: String,
    source: Option<String>,
    /// The libraries switched off (`source/library`, comma-separated), as a crate's tracks leave them out.
    without: Option<String>,
    /// The seeds themselves left out, as a `fresh` Sounds like leaves them (Sounds like your week).
    #[serde(default)]
    fresh: bool,
    /// An artist seed's own tracks kept, as a Sounds like's `own` (0.11, quick crates); left out by default.
    own: Option<bool>,
    /// How much each seed counts, by `of`'s order, comma-separated (Sounds like your week); none, all alike.
    weights: Option<String>,
}

/// How alike the tracks that sound most like some seeds are, closest first (the Sounds like curve).
async fn like_curve(State(app): State<Arc<App>>, Query(q): Query<LikeCurveQuery>) -> Result<Json<crate::app::LikeCurve>, ApiError> {
    let of: Vec<String> = q.of.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
    let without: Vec<String> = q.without.as_deref().unwrap_or("").split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
    let weights: Vec<f32> = q.weights.as_deref().unwrap_or("").split(',').filter_map(|s| s.trim().parse().ok()).collect();
    Ok(Json(app.playlists.like_curve(&of, &weights, q.source.as_deref(), &without, q.fresh, q.own, &[]).await?))
}

/// The Sounds like curve within a crate's other conditions (0.11, quick crates; the owner: "Filtered curve in 0.11"):
/// the same as the query's, its seeds as a list, and the crate's other conditions as the preview sends them, so the
/// curve counts only the tracks the crate can pick among. A Sounds like among them is passed over.
#[derive(Deserialize)]
struct LikeCurveWithin {
    of: Vec<String>,
    #[serde(default)]
    without: Vec<String>,
    #[serde(default)]
    fresh: bool,
    own: Option<bool>,
    #[serde(default)]
    weights: Vec<f32>,
    #[serde(default)]
    conditions: Vec<crate::crates::Condition>,
}

async fn like_curve_within(State(app): State<Arc<App>>, Query(f): Query<Follow>, Json(b): Json<LikeCurveWithin>) -> Result<Json<crate::app::LikeCurve>, ApiError> {
    Ok(Json(app.playlists.like_curve(&b.of, &b.weights, f.source.as_deref(), &b.without, b.fresh, b.own, &b.conditions).await?))
}

#[derive(serde::Deserialize)]
struct TempoBody {
    source: String,
    ids: Vec<String>,
    /// Absent when only asking.
    #[serde(default)]
    adjust: Option<crate::app::tempo::Adjust>,
}

/// Tracks' tempo for Adjust BPM: each one's BPM now and Cratarr's measure.
async fn tempo_of(State(app): State<Arc<App>>, Json(b): Json<TempoBody>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "tracks": app.playlists.tempo_of(&b.source, &b.ids).await? })))
}

/// Adjust BPM: halve, double or reset the tracks' tempo (administrators; written into the files).
async fn adjust_tempo(State(app): State<Arc<App>>, Json(b): Json<TempoBody>) -> Result<Json<serde_json::Value>, ApiError> {
    let how = b.adjust.ok_or_else(|| ApiError::from(ToggleError::Refused("Say how: half, double or reset.".into())))?;
    let (tracks, undo) = app.playlists.adjust_tempo(&b.source, &b.ids, how).await?;
    Ok(Json(json!({ "tracks": tracks, "undo": undo })))
}

/// Every playlist's facts for the Playlists page's table (decision 0042), by id.
async fn playlist_stats(State(app): State<Arc<App>>) -> Result<Json<std::collections::BTreeMap<String, crate::app::PlaylistStats>>, ApiError> {
    Ok(Json(app.playlists.playlist_stats().await?))
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

/// A crate as its view shows it, with the artists it leaves out by "Artist is not" (decision 0083: Not in this crate),
/// each with the key that takes them back.
#[derive(serde::Serialize)]
struct CrateGot {
    #[serde(flatten)]
    def: crate::crates::CrateDef,
    not_artists: Vec<crate::app::NotArtist>,
}

async fn crate_get(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<CrateGot>, ApiError> {
    let mut def = app.playlists.crate_def(&id).await?;
    // Home's crates are a list of lines (Value::Within), not conditions anyone sets: sent without them (those the
    // account added to them stay, decision 0083).
    if crate::crates::home_crate(&id) {
        def.conditions.retain(|c| !matches!(c.value, crate::crates::Value::Within { .. }));
    }
    let not_artists = app.playlists.crate_not_artists(&id).await?;
    Ok(Json(CrateGot { def, not_artists }))
}

/// An artist, by their name as the page shows it.
#[derive(Deserialize)]
struct ArtistNamed {
    #[serde(default)]
    name: Option<String>,
}

/// Not in this crate (decision 0083, the owner 2026-10-30): "Artist is not `name`" added to the crate, once (one's own
/// crate's filter, as set by hand; a fixed crate's conditions the account added). The artists it leaves out so, and
/// the undo token (null when it left them out already).
async fn crate_not_artist(State(app): State<Arc<App>>, Path(id): Path<String>, Json(b): Json<ArtistNamed>) -> Result<Json<serde_json::Value>, ApiError> {
    let name = b.name.unwrap_or_default();
    let (not_artists, undo) = app.playlists.crate_not_artist(&id, &name).await?;
    Ok(Json(json!({ "crate": id, "not_artists": not_artists, "undo": undo })))
}

/// An artist a crate leaves out taken back, by their key: what it still leaves out, and the undo token (null when it
/// named them nowhere).
async fn crate_not_artist_back(State(app): State<Arc<App>>, Path((id, key)): Path<(String, String)>) -> Result<Json<serde_json::Value>, ApiError> {
    let (not_artists, undo) = app.playlists.crate_not_artist_back(&id, &key).await?;
    Ok(Json(json!({ "crate": id, "not_artists": not_artists, "undo": undo })))
}

/// The artists the account excluded everywhere (decision 0083), the newest first: `[{key, name, at}]`.
async fn excluded_artists(State(app): State<Arc<App>>) -> Result<Json<Vec<crate::userdata::ExcludedArtist>>, ApiError> {
    Ok(Json(app.playlists.excluded_artist_list().await?))
}

/// Exclude everywhere (decision 0083): the artist left out of every automatic pick of the account's, by any credit.
/// The key kept is their name's (`app::excluded_artists::key`; without a name, the path's is read as the name). Them as
/// listed, and the undo token (null when listed already).
async fn exclude_artist(State(app): State<Arc<App>>, Path(key): Path<String>, Json(b): Json<ArtistNamed>) -> Result<Json<serde_json::Value>, ApiError> {
    let name = b.name.filter(|n| !n.trim().is_empty()).unwrap_or(key);
    let (a, undo) = app.playlists.exclude_artist(&name).await?;
    Ok(Json(json!({ "key": a.key, "name": a.name, "at": a.at, "undo": undo })))
}

/// An artist taken back off the account's everywhere list: picked again. Their key, and the undo token (null when they
/// were not on it).
async fn include_artist(State(app): State<Arc<App>>, Path(key): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let (key, undo) = app.playlists.include_artist(&key).await?;
    Ok(Json(json!({ "key": key, "undo": undo })))
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

/// The axes the builder draws its controls from, with their domains in the libraries switched on (`without`).
async fn crate_axes(State(app): State<Arc<App>>, Query(f): Query<Follow>, RawQuery(q): RawQuery) -> Result<Json<Vec<crate::crates::AxisView>>, ApiError> {
    Ok(Json(app.playlists.crate_axes(f.source.as_deref(), &without_of(q.as_deref())).await?))
}

/// The builder's live preview: an unsaved crate's size and first rows, from the libraries not in `without`.
#[derive(Deserialize)]
struct Preview {
    conditions: Vec<crate::crates::Condition>,
    #[serde(default)]
    without: Vec<String>,
}

async fn crate_preview(State(app): State<Arc<App>>, Query(q): Query<Page>, Json(p): Json<Preview>) -> Result<Response, ApiError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let look = crate::app::CrateLook { without: p.without, order: q.order, down: q.down, ..Default::default() };
    Ok(Json(app.playlists.crate_page(&p.conditions, q.source.as_deref(), q.offset, limit, &look).await?).into_response())
}

/// The Filter sheet (decision 0061): which of the list in view's items pass these conditions, how many of how many,
/// and the list's axes with their values and counts.
async fn filter_list(
    State(app): State<Arc<App>>,
    Query(f): Query<Follow>,
    Json(r): Json<crate::app::filter::Request>,
) -> Result<Json<crate::app::filter::Filtered>, ApiError> {
    Ok(Json(crate::app::filter::filter(&app, f.source.as_deref(), r).await?))
}

/// Make playlist: the crate's tracks now, as the panel shows them (its favourites toggle), from the libraries not in
/// `without`.
#[derive(Deserialize)]
struct CratePlaylistRequest {
    name: String,
    #[serde(default)]
    without: Vec<String>,
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

/// Tracks of a crate, by their ids on a server (its default when not said), and albums of Cratarr's own library by id.
#[derive(Deserialize)]
struct LeftOutRequest {
    source: Option<String>,
    #[serde(default)]
    ids: Vec<String>,
    #[serde(default)]
    albums: Vec<String>,
}

/// Takes tracks or whole albums out of a crate by hand (decision 0024, amended 2026-10-19): they stay out over its
/// conditions and toggles, an album's tracks now and later.
/// Every ranked crate whose fit holds anything, the account's (Settings' Musical taste).
async fn crate_fits(State(app): State<Arc<App>>) -> Result<Json<Vec<crate::app::FitSummary>>, ApiError> {
    Ok(Json(app.playlists.crate_fits().await?))
}

/// A ranked crate's fit (its Not for sheet): the traits learned from what was turned down for it, its rules.
async fn crate_fit(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<crate::app::FitView>, ApiError> {
    Ok(Json(app.playlists.crate_fit(&id).await?))
}

async fn set_crate_fit(State(app): State<Arc<App>>, Path(id): Path<String>, Json(fit): Json<crate::crates::Fit>) -> Result<Json<serde_json::Value>, ApiError> {
    app.playlists.set_crate_fit(&id, fit).await?;
    Ok(Json(json!({})))
}

async fn crate_leave_out(State(app): State<Arc<App>>, Path(id): Path<String>, Json(req): Json<LeftOutRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let (n, undo) = app.playlists.crate_leave_out(&id, req.source.as_deref(), &req.ids, &req.albums, true).await?;
    Ok(Json(json!({ "left_out": n, "undo": undo })))
}

/// Puts tracks or albums left out of a crate back.
async fn crate_put_back(State(app): State<Arc<App>>, Path(id): Path<String>, Json(req): Json<LeftOutRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let (n, undo) = app.playlists.crate_leave_out(&id, req.source.as_deref(), &req.ids, &req.albums, false).await?;
    Ok(Json(json!({ "left_out": n, "undo": undo })))
}

/// A crate's tracks left out by hand, as rows, and its albums left out, for its editor.
async fn crate_left_out(State(app): State<Arc<App>>, Path(id): Path<String>, Query(f): Query<Follow>) -> Result<Json<serde_json::Value>, ApiError> {
    let (source, rows, albums) = app.playlists.crate_left_out(&id, f.source.as_deref()).await?;
    Ok(Json(json!({ "source": source, "rows": rows, "albums": albums })))
}

/// Tracks, by their ids on a server (its default when not said).
#[derive(Deserialize)]
struct ExcludeRequest {
    source: Option<String>,
    ids: Vec<String>,
}

/// Excludes tracks from every automatic pick (decision 0024, amended 2026-10-19: global exclusions).
async fn exclude(State(app): State<Arc<App>>, Json(req): Json<ExcludeRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let (n, undo) = app.playlists.exclude(req.source.as_deref(), &req.ids, true).await?;
    Ok(Json(json!({ "excluded": n, "undo": undo })))
}

/// Puts excluded tracks back.
async fn include_again(State(app): State<Arc<App>>, Json(req): Json<ExcludeRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let (n, undo) = app.playlists.exclude(req.source.as_deref(), &req.ids, false).await?;
    Ok(Json(json!({ "excluded": n, "undo": undo })))
}

/// The tracks excluded everywhere: how many, and as rows, for the Library's list.
async fn excluded(State(app): State<Arc<App>>, Query(f): Query<Follow>) -> Result<Json<serde_json::Value>, ApiError> {
    let (source, n, rows) = app.playlists.excluded_rows(f.source.as_deref()).await?;
    Ok(Json(json!({ "source": source, "excluded": n, "rows": rows })))
}

#[derive(Deserialize)]
struct CrateFavouriteAlbumsRequest {
    favourite_albums: crate::crates::Favourites,
}

/// A crate's favourite albums toggle (decision 0024, amended 2026-10-19): `all`, `only` or `none`, kept for every
/// device, as the favourites toggle is.
async fn crate_favourite_albums(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(req): Json<CrateFavouriteAlbumsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    app.playlists.set_crate_favourite_albums(&id, req.favourite_albums).await?;
    Ok(Json(json!({ "crate": id, "favourite_albums": req.favourite_albums })))
}

async fn crate_playlist(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(f): Query<Follow>,
    Json(req): Json<CratePlaylistRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let (added, undo) = app.playlists.crate_to_playlist(&id, f.source.as_deref(), &req.name, &req.without).await?;
    Ok((StatusCode::CREATED, Json(json!({ "playlist": req.name, "added": added, "undo": undo }))))
}

/// Which playlists are favourites (by name), as Finamp's playlist hearts (decision 0023).
async fn playlist_favorites(State(app): State<Arc<App>>, Query(f): Query<Follow>) -> Result<Json<std::collections::BTreeMap<String, bool>>, ApiError> {
    Ok(Json(app.playlists.favorites(f.source.as_deref()).await?))
}

/// A playlist's heart, the last change winning (decision 0087).
async fn playlist_favorite(
    ctx: Ctx,
    at: ChangeAt,
    Path(id): Path<String>,
    Query(f): Query<Favorite>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let w = LastWins::of(&ctx, at, ChangeKind::Favourite, format!("playlist:{id}"));
    if is_older(&w) {
        let now = ctx.app.playlists.favorites(f.source.as_deref()).await?.get(&id).copied().unwrap_or(false);
        return Ok(applied(json!({ "playlist": id, "favorite": now }), at, false));
    }
    ctx.app.playlists.set_favorite(&id, f.source.as_deref(), f.on).await?;
    if let Some(w) = &w {
        w.made(Op::of(f.on));
    }
    Ok(applied(json!({ "playlist": id, "favorite": f.on }), at, true))
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

/// A playlist's edits made offline, as operations on its entries (decision 0087): merged on it as it is now; answers
/// the change log's rev and the playlist's entries with their tracks on `?source=`.
async fn playlist_ops(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(f): Query<Follow>,
    Json(b): Json<crate::app::PlaylistOps>,
) -> Result<Json<crate::app::OpsAnswer>, ApiError> {
    Ok(Json(app.playlists.apply_ops(&id, f.source.as_deref(), b.ops).await?))
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
    /// Only those the playlist does not hold yet (0.9.4).
    #[serde(default)]
    only_new: bool,
}

async fn append_tracks(State(app): State<Arc<App>>, Path(id): Path<String>, Json(a): Json<Append>) -> Result<Json<serde_json::Value>, ApiError> {
    let (added, undo) = app.playlists.append_tracks(&id, &a.source, &a.ids, a.only_new).await?;
    Ok(Json(json!({ "playlist": id, "added": added, "undo": undo })))
}

/// A playlist's tracks for the local player: the server they are on and their ids in file order (decision 0022).
async fn playlist_tracks(State(app): State<Arc<App>>, Path(id): Path<String>, Query(f): Query<Follow>) -> Result<Response, ApiError> {
    let (source, ids, unknown) = app.playlists.track_ids(&id, f.source.as_deref()).await?;
    Ok(Json(json!({ "source": source, "ids": ids, "unknown": unknown })).into_response())
}

#[derive(Deserialize)]
struct StreamFrom {
    /// Start at the frame holding this moment (milliseconds; FLAC and Opus), for seeking.
    #[serde(default)]
    from_ms: u64,
    /// `opus`: a lossless track as Opus (a phone's stream, the owner 2026-10-24); `alac` or `mp3` (0.9.8): for a browser
    /// that takes neither FLAC nor Opus in MP4. A lossy one comes as it is.
    #[serde(default, rename = "as")]
    as_: Option<String>,
    /// The stream from its start kept whole (0.9.8, the installed web app's offline play): a length and ranges, the
    /// same bytes and headers; a conflict while it is still being made (`offline/prepare` says when it is ready).
    #[serde(default)]
    whole: Option<u8>,
    /// `1`: an MP3 starts near `from_ms` too, at a frame boundary, `x-start-ms` saying that frame's time (0.9.14; asked
    /// for, as a page that does not read the header would place the sound at 0).
    #[serde(default)]
    mp3_from: Option<u8>,
}

/// A track's audio for the local player (decision 0022): FLAC as fragmented MP4, MP3 as it is, read from Cratarr's own
/// mount one piece at a time (a fragment of about a second, or 64 KB) so memory stays small. With `?as=opus` (a
/// phone), a lossless track as Opus in fragmented MP4 instead.
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

/// A command to one of Cratarr's players: 204, or for a shuffle or a clear that changed what was to come, its Undo to
/// send back as it is (`{"undo": {"command": "restore", "tracks": [...]}}`).
async fn player_command(State(app): State<Arc<App>>, Path(name): Path<String>, Json(c): Json<crate::app::PlayOnCommand>) -> Result<Response, ApiError> {
    Ok(match app.play_on.answer(&name, c).await? {
        Some(undo) => Json(json!({ "undo": undo })).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    })
}

async fn player_state(State(app): State<Arc<App>>, Path(name): Path<String>, Json(r): Json<crate::app::PlayerReport>) -> Result<StatusCode, ApiError> {
    app.play_on.report_off(name, r).await?;
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
async fn group_listen(ctx: Ctx, Path(leader): Path<String>, Query(q): Query<Listening>) -> Response {
    let app = ctx.app.clone();
    let Some(snap) = ctx.snapcast() else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "Sync play is not served here." }))).into_response();
    };
    let delay = u32::from(app.play_on.delay_of(&q.player));
    let body = axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(snap.listen(&leader, &q.player, delay)));
    ([(header::CONTENT_TYPE, HeaderValue::from_static("audio/wav")), (header::CACHE_CONTROL, HeaderValue::from_static("no-store"))], body).into_response()
}

/// A group's sound as stamped records, for a page in a browser to play each at its moment (decision 0037, amended
/// 2026-10-23; `Snapcast::frames`).
async fn group_frames(ctx: Ctx, Path(leader): Path<String>, Query(q): Query<Listening>) -> Response {
    let app = ctx.app.clone();
    let Some(snap) = ctx.snapcast() else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "Sync play is not served here." }))).into_response();
    };
    if let Err(e) = app.play_on.reached(&q.player) {
        return ApiError::from(e).into_response();
    }
    let delay = u32::from(app.play_on.delay_of(&q.player));
    let body = axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(snap.frames(&leader, &q.player, delay)));
    ([(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream")), (header::CACHE_CONTROL, HeaderValue::from_static("no-store"))], body).into_response()
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
    headers: HeaderMap,
) -> Result<axum::response::sse::Sse<impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>>, ApiError> {
    use tokio_stream::StreamExt;
    let commands = if q.kind.as_deref() == Some("browser") {
        app.play_on.connect_browser(&name, q.page.as_deref())?
    } else {
        // The id the app gave this connection (decision 0045), for the log.
        let conn = headers.get("x-cratarr-connection").and_then(|v| v.to_str().ok()).filter(|v| plain_id(v));
        app.play_on.connect_app(&name, conn)?
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
/// seeks in the file itself and plays gapless from its own headers. With `?as=opus`, a lossless track's smaller copy:
/// the app's stream on mobile data and its smaller downloads, kept on the NAS (`opus_copies`) so it has a length and
/// ranges too.
async fn file(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>, Query(q): Query<FileAs>, headers: HeaderMap) -> Response {
    // A preview as it is, whatever form was asked: it is lossy already, and never copied (decision 0071).
    if source == crate::bandcamp::SOURCE {
        return bandcamp_relay(&id, headers.get(header::RANGE).and_then(|v| v.to_str().ok()), 0).await;
    }
    let cat = app.library.catalogue();
    let source_id = (source.clone(), id.clone());
    let found = tokio::task::spawn_blocking(move || crate::stream::locate(&cat, &source, &id)).await.ok().flatten();
    let Some((path, _, _)) = found else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "That track is not in the library." }))).into_response();
    };
    if q.as_.as_deref() == Some("opus") {
        let gain = replay_gain_of(&app, &source_id.0, &source_id.1).await;
        let copy = match &app.opus_copies {
            Some(copies) => copies.serve(path.clone(), copy_gains(gain.as_ref()), &headers).await,
            None => opus_copy(path.clone(), gain).await,
        };
        if let Some(copy) = copy {
            return copy;
        }
    }
    crate::serve::files::serve_file(&path, &headers, false).await
}

/// The gains written into a track's Opus copy (its name says them: the same gains, the same copy).
fn copy_gains(g: Option<&crate::serve::api::ReplayGain>) -> (Option<f64>, Option<f64>) {
    g.map_or((None, None), |g| (g.track_gain, g.album_gain))
}

/// The most tracks one offline call takes (an album, a playlist).
const OFFLINE_MOST: usize = 500;

#[derive(Deserialize)]
struct OfflineTracks {
    tracks: Vec<TrackRefOf>,
    /// "opus" (the default: lossless as its Opus copy, the rest as it is), "original", or "stream": the form the
    /// browser player streams (`/api/stream`), kept whole (`?whole=1`), with `opus` for a lossless track as Opus.
    #[serde(default, rename = "as")]
    as_: Option<String>,
    #[serde(default)]
    opus: bool,
    /// The stream's form (0.9.8 page): "opus", "alac", "mp3", "original", or "aac" (not made: such tracks say so);
    /// over `opus`.
    #[serde(default)]
    form: Option<String>,
}

/// What form an offline call asks for.
#[derive(Clone, Copy)]
enum OfflineAs {
    /// The file (`/api/file`), lossless as its Opus copy where `opus`.
    File { opus: bool },
    /// The browser player's stream kept whole, lossless in the form asked for.
    Stream(Lossless),
    /// A form Cratarr does not make (yet): each track says so.
    Unmade(&'static str),
}

impl OfflineTracks {
    fn form(&self) -> OfflineAs {
        match self.as_.as_deref() {
            Some("stream") => match self.form.as_deref() {
                Some("aac") => OfflineAs::Unmade("AAC is not made: ask for ALAC or MP3."),
                Some(f) => match Lossless::of(Some(f)) {
                    Some(l) => OfflineAs::Stream(l),
                    None => OfflineAs::Unmade("That form is not made."),
                },
                None => OfflineAs::Stream(if self.opus { Lossless::Opus } else { Lossless::Original }),
            },
            Some("original") => OfflineAs::File { opus: false },
            _ => OfflineAs::File { opus: true },
        }
    }
}

#[derive(Deserialize, Clone)]
struct TrackRefOf {
    source: String,
    id: String,
}

/// A track's file as a device keeps it for offline play: where to fetch it, as what, and whether it is ready (a copy
/// being made has no length yet: the device waits for it).
async fn offline_file(app: &App, t: &TrackRefOf, form: OfflineAs) -> serde_json::Value {
    if let Some(refused) = never_offline(t) {
        return refused;
    }
    let cat = app.library.catalogue();
    let (source, id) = (t.source.clone(), t.id.clone());
    let Some((path, kind, _)) = tokio::task::spawn_blocking(move || crate::stream::locate(&cat, &source, &id)).await.ok().flatten() else {
        return json!({ "source": t.source, "id": t.id, "offline": false, "error": "That track is not in the library." });
    };
    let opus = match form {
        OfflineAs::Unmade(why) => return json!({ "source": t.source, "id": t.id, "offline": false, "error": why }),
        OfflineAs::File { opus } => opus,
        OfflineAs::Stream(wanted) => {
            let url = format!("/api/stream/{}/{}?whole=1{}", form_urlencoded_segment(&t.source), form_urlencoded_segment(&t.id), wanted.query());
            let Some(sf) = stream_form(&path, kind, wanted).await else {
                // An MP3 streams as it is.
                let bytes = tokio::fs::metadata(&path).await.ok().map(|m| m.len());
                return json!({ "source": t.source, "id": t.id, "offline": true, "file": { "url": url, "type": "audio/mpeg", "as": "stream", "ready": true, "bytes": bytes } });
            };
            let kept = match &app.opus_copies {
                Some(copies) => copies.prepare_stream(path, sf).await,
                None => None,
            };
            let Some(kept) = kept else {
                return json!({ "source": t.source, "id": t.id, "offline": false, "error": "Cratarr cannot keep this track." });
            };
            let kind = match sf {
                crate::opus_copies::Form::Opus { .. } => "audio/mp4; codecs=\"opus\"",
                crate::opus_copies::Form::Alac => "audio/mp4; codecs=\"alac\"",
                crate::opus_copies::Form::Mp3 => "audio/mpeg",
                crate::opus_copies::Form::Flac => "audio/mp4",
            };
            return json!({ "source": t.source, "id": t.id, "offline": true, "file": { "url": url, "type": kind, "as": "stream", "ready": kept.is_some(), "bytes": kept } });
        }
    };
    let base = format!("/api/file/{}/{}", form_urlencoded_segment(&t.source), form_urlencoded_segment(&t.id));
    if opus && let Some(copies) = &app.opus_copies {
        let gain = replay_gain_of(app, &t.source, &t.id).await;
        if let Some(kept) = copies.prepare(path.clone(), copy_gains(gain.as_ref())).await {
            return json!({ "source": t.source, "id": t.id, "offline": true, "file": {
                "url": format!("{base}?as=opus"), "type": "audio/ogg", "as": "opus", "ready": kept.is_some(), "bytes": kept,
            }});
        }
    }
    let bytes = tokio::fs::metadata(&path).await.ok().map(|m| m.len());
    let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
    let kind = match ext.as_str() {
        "flac" => "audio/flac",
        "mp3" => "audio/mpeg",
        "m4a" | "alac" | "aac" => "audio/mp4",
        "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    };
    json!({ "source": t.source, "id": t.id, "offline": true, "file": { "url": base, "type": kind, "as": "original", "ready": true, "bytes": bytes } })
}

/// A track never kept on a device nor copied (a Bandcamp preview, decision 0071): its offline answer, before any file
/// or Opus copy is looked for.
fn never_offline(t: &TrackRefOf) -> Option<serde_json::Value> {
    (t.source == crate::bandcamp::SOURCE).then(|| json!({ "source": t.source, "id": t.id, "offline": false, "error": crate::bandcamp::NOT_KEPT }))
}

/// A path segment, percent-encoded.
fn form_urlencoded_segment(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// Offline play (0.9.8, the installed web app): the tracks' files readied, each with where to fetch it and whether it
/// is ready yet; asked again until all are.
async fn offline_prepare(State(app): State<Arc<App>>, Json(b): Json<OfflineTracks>) -> Result<Json<serde_json::Value>, ApiError> {
    if b.tracks.len() > OFFLINE_MOST {
        return Err(ToggleError::Refused(format!("At most {OFFLINE_MOST} tracks at once.")).into());
    }
    let form = b.form();
    let mut out = Vec::with_capacity(b.tracks.len());
    for t in &b.tracks {
        out.push(offline_file(&app, t, form).await);
    }
    Ok(Json(json!({ "tracks": out })))
}

/// Offline play's facts (0.9.8): each track's row (as the Tracks list has it, with the account's plays), its gains and
/// what levelling turns it down by, its cover's and waveform's addresses (fetched and kept as they are), and its file
/// (as `offline/prepare` gives it).
async fn offline_facts(ctx: Ctx, Json(b): Json<OfflineTracks>) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    if b.tracks.len() > OFFLINE_MOST {
        return Err(ToggleError::Refused(format!("At most {OFFLINE_MOST} tracks at once.")).into());
    }
    let form = b.form();
    let own: Vec<String> = b.tracks.iter().filter(|t| t.source == crate::source::cratarr::KIND.id).map(|t| t.id.clone()).collect();
    let (rows, stamps) = match ctx.served.clone() {
        Some(served) => tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let mut rows = served.library.track_rows_by_id(&own)?;
            if let Some(user) = served.current_user() {
                crate::app::tracks::of_account(&served, &user, &mut rows)?;
            }
            Ok((rows, served.cover_stamps()))
        })
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?,
        None => (Vec::new(), Arc::default()),
    };
    let rows: std::collections::HashMap<String, crate::serve::read::TrackRow> = rows.into_iter().map(|r| (r.id.clone(), r)).collect();
    let mut out = Vec::with_capacity(b.tracks.len());
    for t in &b.tracks {
        let mut v = offline_file(&app, t, form).await;
        if v["offline"] != json!(true) {
            out.push(v);
            continue;
        }
        let (s, i) = (form_urlencoded_segment(&t.source), form_urlencoded_segment(&t.id));
        let row = rows.get(&t.id).filter(|_| t.source == crate::source::cratarr::KIND.id);
        if let Some(r) = row {
            v["track"] = serde_json::to_value(r).unwrap_or_default();
            let stamp = stamps.get(&r.album_id).map(|p| format!("?v={p}")).unwrap_or_default();
            v["cover"] = format!("/api/albums/{s}/{}/cover{stamp}", form_urlencoded_segment(&r.album_id)).into();
        } else {
            v["cover"] = format!("/api/cover/{s}/{i}").into();
        }
        v["gain"] = gain_view(replay_gain_of(&app, &t.source, &t.id).await.as_ref());
        v["waveform"] = format!("/api/track/{s}/{i}/waveform").into();
        out.push(v);
    }
    Ok(Json(json!({ "tracks": out })))
}

/// A lossless file as Ogg Opus, made as it is sent (no length, no ranges: downloads take it whole). `None` for a file
/// that is not lossless.
async fn opus_copy(path: std::path::PathBuf, gain: Option<crate::serve::api::ReplayGain>) -> Option<Response> {
    let gains = gain.map_or((None, None), |g| (g.track_gain, g.album_gain));
    let probe = path.clone();
    let lossless = tokio::task::spawn_blocking(move || crate::convert::Source::open(&probe).ok().flatten().is_some()).await.ok()?;
    if !lossless {
        return None;
    }
    let permit = crate::opus_copy::COPIES.acquire().await.ok()?;
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

async fn stream(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>, Query(q): Query<StreamFrom>, headers: HeaderMap) -> Response {
    use crate::stream::{FlacToMp4, Kind};
    if source == crate::bandcamp::SOURCE {
        // Its MP3 as it is, whatever form was asked (`as`, `whole`): it is lossy already. A moment to start at, as the
        // place in the file it is at; else the player's own range.
        return bandcamp_relay(&id, headers.get(header::RANGE).and_then(|v| v.to_str().ok()), q.from_ms).await;
    }
    let cat = app.library.catalogue();
    let found = tokio::task::spawn_blocking(move || crate::stream::locate(&cat, &source, &id)).await.ok().flatten();
    let Some((path, kind, duration_ms)) = found else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "That track is not in the library." }))).into_response();
    };
    if q.whole == Some(1) {
        return whole_stream(&app, path, kind, duration_ms, Lossless::of(q.as_.as_deref()).unwrap_or(Lossless::Original), &headers).await;
    }
    if q.as_.as_deref() == Some("opus")
        && let Some(opus) = opus_stream(path.clone(), q.from_ms, false).await
    {
        return opus;
    }
    // ALAC or MP3 for a browser that takes neither FLAC nor Opus in MP4 (0.9.8); a lossy file comes as it is.
    if let Some(l @ (Lossless::Alac | Lossless::Mp3)) = Lossless::of(q.as_.as_deref())
        && let Some(r) = lossless_as(path.clone(), q.from_ms, l == Lossless::Mp3).await
    {
        return r;
    }
    let Some(kind) = kind else {
        // Neither FLAC nor MP3 (ALAC, AAC, WAV, AIFF, Ogg Vorbis): decoded here and sent as Opus, which every browser's
        // stream takes.
        if let Some(opus) = opus_stream(path, q.from_ms, true).await {
            return opus;
        }
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, Json(json!({ "error": "This file's format does not play here." }))).into_response();
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
            // From its first frame (0.9.8: an ID3v2 tag with a large cover held the sound back by seconds on a phone's
            // connection); its LAME header frame comes first, as the page trims by it. Asked to start later
            // (`mp3_from`): at the frame near that moment, its time said, not after every byte before it.
            let (p, from) = (path.clone(), (q.mp3_from == Some(1) && q.from_ms > 0).then_some(q.from_ms));
            let (skip, start) = tokio::task::spawn_blocking(move || {
                let skip = crate::analysis::after_id3(&p);
                (skip, from.and_then(|ms| crate::stream::mp3_from(&p, skip, ms)))
            })
            .await
            .unwrap_or((0, None));
            let skip = start.map_or(skip, |(byte, ms)| {
                headers.insert("x-start-ms", HeaderValue::from(ms));
                byte
            });
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncSeekExt};
                let Ok(mut file) = tokio::fs::File::open(&path).await else { return };
                if skip > 0 && file.seek(std::io::SeekFrom::Start(skip)).await.is_err() {
                    return;
                }
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

/// The form a lossless track is asked for in (`as`): as it is, or Opus, ALAC or MP3 (0.9.8: the page picks by what the
/// browser's media source takes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lossless {
    Original,
    Opus,
    Alac,
    Mp3,
}

impl Lossless {
    fn of(s: Option<&str>) -> Option<Self> {
        Some(match s {
            None | Some("original") => Self::Original,
            Some("opus") => Self::Opus,
            Some("alac") => Self::Alac,
            Some("mp3") => Self::Mp3,
            Some(_) => return None,
        })
    }

    /// Its part of a stream's address.
    fn query(self) -> &'static str {
        match self {
            Self::Original => "",
            Self::Opus => "&as=opus",
            Self::Alac => "&as=alac",
            Self::Mp3 => "&as=mp3",
        }
    }
}

/// The form `/api/stream` sends a track in from its start, for keeping whole (0.9.8): a lossless one in the form asked
/// for, FLAC as fragmented MP4, MP3 as it is (`None`), anything else as Opus.
async fn stream_form(path: &std::path::Path, kind: Option<crate::stream::Kind>, wanted: Lossless) -> Option<crate::opus_copies::Form> {
    use crate::opus_copies::Form;
    use crate::stream::Kind;
    let p = path.to_path_buf();
    let lossless = || async move { tokio::task::spawn_blocking(move || crate::convert::Source::open(&p).ok().flatten().is_some()).await.unwrap_or(false) };
    let made = match wanted {
        Lossless::Original => None,
        Lossless::Opus => Some(Form::Opus { lossy_too: false }),
        Lossless::Alac => Some(Form::Alac),
        Lossless::Mp3 => Some(Form::Mp3),
    };
    if let Some(form) = made
        && lossless().await
    {
        return Some(form);
    }
    match kind {
        Some(Kind::Flac) => Some(Form::Flac),
        Some(Kind::Mp3) => None,
        None => Some(Form::Opus { lossy_too: true }),
    }
}

/// The stream kept whole (`?whole=1`): the kept form with its headers, length and ranges; an MP3 as its file.
async fn whole_stream(app: &App, path: std::path::PathBuf, kind: Option<crate::stream::Kind>, duration_ms: Option<u64>, wanted: Lossless, headers: &HeaderMap) -> Response {
    let Some(form) = stream_form(&path, kind, wanted).await else {
        let mut r = crate::serve::files::serve_file(&path, headers, false).await;
        r.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/mpeg"));
        if let Some(ms) = duration_ms {
            r.headers_mut().insert("x-duration-ms", HeaderValue::from(ms));
        }
        return r;
    };
    let Some(copies) = &app.opus_copies else {
        return (StatusCode::CONFLICT, Json(json!({ "error": "Cratarr keeps no copies without its data folder." }))).into_response();
    };
    match copies.serve_stream(path, form, headers).await {
        Some(r) => r,
        None => (StatusCode::CONFLICT, Json(json!({ "error": "Not ready yet." }))).into_response(),
    }
}

/// A lossless track as ALAC in fragmented MP4, or as MP3 (`stream_forms`), made as it is sent from `from_ms`; `None`
/// when it is not lossless (it streams as it is). The headers place it, as the forms say.
async fn lossless_as(path: std::path::PathBuf, from_ms: u64, mp3: bool) -> Option<Response> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(2);
    let (said, heard) = tokio::sync::oneshot::channel::<crate::stream_forms::Head>();
    tokio::task::spawn_blocking(move || {
        let failed = tx.clone();
        let out = ChannelWriter { tx, buf: Vec::with_capacity(ChannelWriter::PIECE) };
        let told = |h| {
            let _ = said.send(h);
        };
        let made = if mp3 { crate::stream_forms::mp3(&path, from_ms, told, out) } else { crate::stream_forms::alac_mp4(&path, from_ms, told, out) };
        if let Err(e) = made {
            // A closed body: the page stopped reading (a skip, a seek).
            let stopped = e.downcast_ref::<std::io::Error>().is_some_and(|e| e.kind() == std::io::ErrorKind::BrokenPipe);
            if !stopped {
                tracing::warn!("{} stream of {}: {e:#}", if mp3 { "MP3" } else { "ALAC" }, path.display());
                let _ = failed.blocking_send(Err(std::io::Error::other(format!("{e:#}"))));
            }
        }
    });
    let head = heard.await.ok()?;
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    for (k, v) in head {
        if let (Ok(k), Ok(v)) = (header::HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(&v)) {
            headers.insert(k, v);
        }
    }
    Some((headers, axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).into_response())
}

/// Where Tape rewind starts, and how it sounds (`rewind::Shape`, the device's four controls; each left out as it is
/// out of the box): its length (ms), its cruising speed, each piece's length (ms), the share spinning up and braking take.
#[derive(Deserialize)]
struct RewindFrom {
    from_ms: u64,
    ms: Option<u64>,
    speed: Option<f64>,
    piece_ms: Option<u64>,
    ramp: Option<f64>,
}

/// How long Tape rewind and a wind wait for a turn ([`crate::app::analysis_pass::wind_turn`]) before the player goes on
/// without them.
const REWIND_TURN: std::time::Duration = std::time::Duration::from_secs(2);

/// Tape rewind (0.9.9, `rewind`): the track heard backwards from `from_ms` to its start in exactly `ms`, as 16-bit WAV
/// made as it is sent. 404 for a track Cratarr does not keep the file of; 503 while another decode holds the turn: the
/// player then starts the track over without it.
async fn stream_rewind(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>, Query(q): Query<RewindFrom>) -> Response {
    // Each step timed from the request (0.9.11, the owner: a rewind took far longer than its clip; where it goes).
    let asked = std::time::Instant::now();
    let since = move || asked.elapsed().as_millis() as u64;
    let what = format!("{source}/{id}");
    if source == crate::bandcamp::SOURCE {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": crate::bandcamp::NO_TAPE }))).into_response();
    }
    let cat = app.library.catalogue();
    let found = tokio::task::spawn_blocking(move || crate::stream::locate(&cat, &source, &id)).await.ok().flatten();
    let Some((path, _, duration_ms)) = found else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "That track is not in the library." }))).into_response();
    };
    let located = since();
    let Some(turn) = crate::app::analysis_pass::wind_turn(REWIND_TURN).await else {
        tracing::warn!(target: "rewind", track = %what, located, waited = since() - located, "the wind's turn did not come: not rendered");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "error": "Other winds hold the turns." }))).into_response();
    };
    let turned = since();
    let from_ms = duration_ms.map_or(q.from_ms, |d| q.from_ms.min(d));
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<axum::body::Bytes, std::io::Error>>();
    let (said, heard) = tokio::sync::oneshot::channel();
    let what2 = what.clone();
    tokio::task::spawn_blocking(move || {
        let _turn = turn;
        let _job = crate::memwatch::job(format!("tape rewind: {}", path.display()));
        let what = what2;
        let mut sound = match crate::analysis::Reading::open(&path) {
            Ok(s) => s,
            Err(e) => {
                let _ = said.send(Err(format!("{e:#}")));
                return;
            }
        };
        let d = crate::rewind::Shape::default();
        let shape = crate::rewind::Shape { ms: q.ms.unwrap_or(d.ms), speed: q.speed.unwrap_or(d.speed), piece_ms: q.piece_ms.unwrap_or(d.piece_ms), ramp: q.ramp.unwrap_or(d.ramp) };
        let rewind = crate::rewind::Rewind::new(sound.rate(), sound.channels(), from_ms, shape);
        let opened = since();
        let _ = said.send(Ok(rewind.bytes()));
        // The whole clip is a few MB at most: made as fast as it goes, the turn not held while the player reads.
        let (mut first, mut blocks) = (None, 0u32);
        let rendered = rewind.render(&mut sound, |b| {
            // The header goes first; the first block of sound after it.
            if blocks == 1 {
                first = Some(since());
            }
            blocks += 1;
            tx.send(Ok(b.into())).is_ok()
        });
        if let Err(e) = rendered {
            tracing::warn!("tape rewind of {}: {e:#}", path.display());
            let _ = tx.send(Err(std::io::Error::other(format!("{e:#}"))));
        }
        tracing::info!(target: "rewind", track = %what, from_ms, ms = shape.ms, speed = shape.speed, piece_ms = shape.piece_ms, ramp = shape.ramp,
            located, turn_wait = turned - located, opened, first_block = first, rendered = since(), blocks, "rendered (ms since the request)");
    });
    let bytes = match heard.await {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": e }))).into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/wav"));
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(bytes));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    // When the player has taken the last of it (handed to the connection): how fast it reads.
    let mut taken = 0u64;
    let body = tokio_stream::StreamExt::map(tokio_stream::wrappers::UnboundedReceiverStream::new(rx), move |chunk| {
        if let Ok(b) = &chunk {
            taken += b.len() as u64;
            if taken == bytes {
                tracing::info!(target: "rewind", track = %what, bytes, taken_at = since(), "taken (ms since the request)");
            }
        }
        chunk
    });
    (headers, axum::body::Body::from_stream(body)).into_response()
}

/// A wind (decision 0074): from where, which way, how fast, how much quieter; to where (none: the edge); condensed to a
/// wall length with the player's envelope (all four of `fit_ms`, `attack_ms`, `release_ms`, `piece_ms`), else open-ended.
#[derive(Deserialize)]
struct WindAsk {
    from_ms: u64,
    dir: String,
    #[serde(default)]
    speed: Option<f64>,
    #[serde(default)]
    gain_db: Option<f64>,
    #[serde(default)]
    to_ms: Option<u64>,
    #[serde(default)]
    fit_ms: Option<u64>,
    #[serde(default)]
    attack_ms: Option<u64>,
    #[serde(default)]
    release_ms: Option<u64>,
    #[serde(default)]
    piece_ms: Option<u64>,
    #[serde(default, rename = "as")]
    as_: Option<String>,
    #[serde(default)]
    through: Option<String>,
    /// A hold that speeds up (0.9.12): its speed doubles every `accel_s` seconds of the stream, up to `max` (64 at most).
    #[serde(default)]
    accel_s: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
    /// `1`: the wind's plan alone, as JSON, no sound (a page's player cannot read the stream's headers).
    #[serde(default)]
    map: Option<u8>,
    /// `1` with `fit_ms`: a flying wind to a point (`wind::Fit::fly`), its rate resting on the track's own speed.
    #[serde(default)]
    fly: Option<u8>,
    /// A hold's flight on past `max` to this (×), heard at `max` in pieces of `piece_ms` (0.9.14); `x-flight-max` says
    /// it was.
    #[serde(default)]
    flight_max: Option<f64>,
    /// A let-go's brake (0.9.14-3, `wind::Brake`): from `brake_v0` × to `brake_to` × over `brake_ms`, the stream from
    /// `brake_at_ms` into it, heard at most at `max` (pieces of `piece_ms` above), ending at its aim. Played at 1×.
    #[serde(default)]
    brake_v0: Option<f64>,
    #[serde(default)]
    brake_to: Option<f64>,
    #[serde(default)]
    brake_ms: Option<u64>,
    #[serde(default)]
    brake_at_ms: Option<u64>,
}

/// The most tracks a hold winds on into past its own (0.9.14-2; 8 before: a hold back stopped at the start of the
/// eighth track before). Each is opened only as the wind reaches it.
const WIND_THROUGH: usize = 64;

/// Winding (decision 0074): the track from `from_ms` backwards or forwards at a constant speed, as 16-bit WAV made as
/// the player reads it (a hold lasts as long as it reads). Its headers place it: `x-wind-speed` (lower than asked where
/// a condensed wind got there sooner), `x-wind-dir`, and `x-wind-map`, knots `[[stream_ms, track, track_ms], …]`, linear
/// between, a cut two knots at one moment. 404 where it cannot be rendered (the player jumps), 503 while another decode
/// holds the turn, 400 for what is not there yet (another form than WAV, winding through the queue).
async fn stream_wind(State(app): State<Arc<App>>, Path((source, id)): Path<(String, String)>, Query(q): Query<WindAsk>) -> Response {
    use crate::wind::{Ask, Dir, Fit, Wind};
    let asked = std::time::Instant::now();
    let since = move || asked.elapsed().as_millis() as u64;
    let what = format!("{source}/{id}");
    let refused = |why: &str| (StatusCode::BAD_REQUEST, Json(json!({ "error": why }))).into_response();
    // Not rendered, as any track Cratarr keeps no file of: the player jumps.
    if source == crate::bandcamp::SOURCE {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": crate::bandcamp::NO_TAPE }))).into_response();
    }
    if q.as_.as_deref().is_some_and(|f| f != "wav") {
        return refused("Only WAV winds yet.");
    }
    // Winding on through the queue (0074): the tracks in the way, nearest first (`source:id` or `source/id`), at most
    // [WIND_THROUGH]; a hold only, to this track's edge.
    let through: Vec<(String, String)> = q
        .through
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .filter_map(|t| t.trim().split_once([':', '/']).map(|(s, i)| (s.to_string(), i.to_string())))
        .take(WIND_THROUGH)
        .collect();
    if !through.is_empty() && (q.fit_ms.is_some() || q.to_ms.is_some()) {
        return refused("Only a hold winds on through the queue.");
    }
    let dir = match q.dir.as_str() {
        "back" => Dir::Back,
        "forward" => Dir::Forward,
        _ => return refused("dir is back or forward."),
    };
    let fit = match (q.fit_ms, q.attack_ms, q.release_ms, q.piece_ms) {
        (Some(ms), attack, release, piece) => {
            Some(Fit { ms, attack_ms: attack.unwrap_or(0), release_ms: release.unwrap_or(0), piece_ms: piece.unwrap_or(350), fly: q.fly == Some(1) })
        }
        (None, ..) => None,
    };
    // Flying past what is heard (0.9.14, `flight_max`): heard at most at `max`, the flight on to `flight_max` in pieces.
    let flight = q.flight_max.filter(|f| f.is_finite());
    let accel = q.accel_s.filter(|p| p.is_finite() && *p > 0.0).map(|p| {
        let max = q.max.filter(|m| m.is_finite()).unwrap_or(64.0).clamp(1.0, 64.0);
        match flight.filter(|f| *f > max) {
            Some(f) => crate::wind::Accel { period_s: p.clamp(0.2, 30.0), max: f.min(1024.0), audio: Some((max, q.piece_ms.unwrap_or(350).clamp(50, 2_000))), brake: None },
            None => crate::wind::Accel { period_s: p.clamp(0.2, 30.0), max, audio: None, brake: None },
        }
    });
    // A let-go's brake (0.9.14-3): the plan's curve carried by the stream, heard at most at `max`.
    let brake = match (q.brake_v0.filter(|v| v.is_finite() && *v > 0.0), q.brake_ms.filter(|ms| *ms > 0)) {
        (Some(v0), Some(ms)) => {
            let to = q.brake_to.filter(|t| t.is_finite()).unwrap_or(0.0).clamp(0.0, v0);
            let heard = q.max.filter(|m| m.is_finite()).unwrap_or(16.0).clamp(1.0, 64.0);
            let secs = ms.min(10_000) as f64 / 1000.0;
            let at = (q.brake_at_ms.unwrap_or(0) as f64 / 1000.0).min(secs);
            Some(crate::wind::Accel {
                period_s: 1.0,
                max: v0.min(1024.0),
                audio: (v0 > heard).then(|| (heard, q.piece_ms.unwrap_or(350).clamp(50, 2_000))),
                brake: Some(crate::wind::Brake { v0: v0.min(1024.0), to, secs, at }),
            })
        }
        _ => None,
    };
    let accel = brake.or(accel);
    let ask = Ask { dir, from_ms: q.from_ms, to_ms: q.to_ms, speed: q.speed.unwrap_or(6.0), gain_db: q.gain_db.unwrap_or(-12.0), fit, accel };
    let cat = app.library.catalogue();
    let found = tokio::task::spawn_blocking(move || crate::stream::locate(&cat, &source, &id)).await.ok().flatten();
    let Some((path, _, duration_ms)) = found else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "That track is not in the library." }))).into_response();
    };
    let located = since();
    // The tracks in the way Cratarr has the files of; the first it has not (a preview, another server's) ends the chain.
    // All in one hold of the catalogue, not three takes of its lock for each of up to 64 (0.10.2: the slow answers of
    // 2026-10-30); their lengths place the stream (`x-wind-map`), so they are known before its headers.
    let cat = app.library.catalogue();
    let joins: Vec<(std::path::PathBuf, Option<u64>)> = if through.is_empty() {
        Vec::new()
    } else {
        tokio::task::spawn_blocking(move || crate::stream::locate_through(&cat, &through)).await.unwrap_or_default()
    };
    let joined = since();
    // The plan alone (0.9.13, `map=1`): where each moment of the stream stands in the queue's tracks, its speed and length;
    // read from the files' headers, nothing decoded.
    if q.map == Some(1) {
        let plan = tokio::task::spawn_blocking(move || -> anyhow::Result<serde_json::Value> {
            let first = crate::analysis::Reading::open(&path)?;
            // The tracks in the way planned at the stream's rate (their own does not change the plan, `Wind::at_rate`).
            let mut tracks = vec![(first.rate(), first.channels(), duration_ms)];
            tracks.extend(joins.iter().map(|(_, d)| (first.rate(), first.channels(), *d)));
            let winds = if tracks.len() > 1 { crate::wind::chain(&tracks, ask)? } else { vec![Wind::new(tracks[0].0, tracks[0].1, duration_ms, ask)?] };
            let map: Vec<serde_json::Value> = crate::wind::chain_map(&winds).into_iter().map(|(c, i, t)| json!([c, i, t])).collect();
            let length: f64 = winds.iter().map(Wind::ms).sum();
            Ok(json!({ "map": map, "speed": winds[0].speed(), "length_ms": length.round() as u64, "flight_max": ask.accel.and_then(|a| a.audio.map(|_| a.max)) }))
        })
        .await;
        return match plan {
            // A flight honoured says so here too (0.9.14): a page's player cannot read the stream's headers.
            Ok(Ok(v)) => {
                let mut r = Json(v).into_response();
                if let Some(f) = accel.filter(|a| a.audio.is_some() && a.brake.is_none()).map(|a| a.max)
                    && let Ok(h) = HeaderValue::from_str(&f.to_string())
                {
                    r.headers_mut().insert("x-flight-max", h);
                }
                r
            }
            Ok(Err(e)) => (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("{e:#}") }))).into_response(),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
    }
    let Some(turn) = crate::app::analysis_pass::wind_turn(REWIND_TURN).await else {
        tracing::warn!(target: "rewind", track = %what, located, through = joins.len(), joined, waited = since() - joined, "wind: the wind's turn did not come");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "error": "Other winds hold the turns." }))).into_response();
    };
    // When its turn came, and when its first sound went (the app gives a wind 1.5 s to sound).
    let turned = since();
    // Paced by the player's reading: a hold may last minutes, and only a few blocks wait for it at a time.
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(16);
    let (said, heard) = tokio::sync::oneshot::channel();
    let what2 = what.clone();
    tokio::task::spawn_blocking(move || {
        let _turn = turn;
        let _job = crate::memwatch::job(format!("wind: {}", path.display()));
        let sound = match crate::analysis::Reading::open(&path) {
            Ok(s) => s,
            Err(e) => {
                let _ = said.send(Err((StatusCode::UNPROCESSABLE_ENTITY, format!("{e:#}"))));
                return;
            }
        };
        let opened = since();
        // The tracks it winds on into (0.9.14-2, up to [WIND_THROUGH]): planned at the stream's rate, each opened only as
        // the wind reaches it, at its own (`Wind::at_rate`); never all at once.
        let mut tracks = vec![(sound.rate(), sound.channels(), duration_ms)];
        tracks.extend(joins.iter().map(|(_, d)| (sound.rate(), sound.channels(), *d)));
        let mut own = Some(sound);
        let winds = if tracks.len() > 1 { crate::wind::chain(&tracks, ask) } else { Wind::new(tracks[0].0, tracks[0].1, duration_ms, ask).map(|w| vec![w]) };
        let winds = match winds {
            Ok(w) => w,
            Err(e) => {
                let _ = said.send(Err((StatusCode::BAD_REQUEST, format!("{e:#}"))));
                return;
            }
        };
        let map: Vec<serde_json::Value> = crate::wind::chain_map(&winds).into_iter().map(|(c, i, t)| json!([c, i, t])).collect();
        let data: u64 = winds.iter().map(Wind::data).sum();
        let (rate, channels) = winds[0].stream();
        let _ = said.send(Ok((44 + data, winds[0].speed(), serde_json::Value::Array(map).to_string())));
        // Its headers on their way, and what each step before them took (0.10.2: the slow answers of 2026-10-30: a
        // wind's slow start showed only as a slow "stream", and its own line only when it ended).
        tracing::info!(target: "rewind", track = %what2, through = joins.len(), locate = located, locate_through = joined - located, turn_wait = turned - joined,
            open = opened - turned, headers = since(), "wind: headers (ms; headers since the request)");
        let mut blocks = 0u32;
        let mut first: Option<u64> = None;
        let mut send = |b: Vec<u8>| {
            blocks += 1;
            if blocks == 2 {
                first = Some(since());
            }
            tx.blocking_send(Ok(b.into())).is_ok()
        };
        let rendered = (|| -> anyhow::Result<()> {
            if !send(crate::rewind::wav_header(rate, channels, data)) {
                return Ok(());
            }
            // One shaper for the whole stream: a rising hold sounds alike across the queue's joins.
            let mut shaper = crate::wind::Shaper::new(channels);
            for (i, w) in winds.iter().enumerate() {
                let (mut s, w) = match own.take() {
                    Some(s) => (s, None),
                    None => match crate::analysis::Reading::open(&joins[i - 1].0) {
                        Ok(s) => {
                            let r = s.rate();
                            (s, Some(w.at_rate(r)))
                        }
                        // One that will not open now: silence for its way, so the stream stays as long as its map says.
                        Err(e) => {
                            tracing::warn!("wind: {} not opened: {e:#}", joins[i - 1].0.display());
                            let mut left = w.data() as usize;
                            while left > 0 {
                                let n = left.min(64 * 1024);
                                if !send(vec![0; n]) {
                                    return Ok(());
                                }
                                left -= n;
                            }
                            continue;
                        }
                    },
                };
                if !w.as_ref().unwrap_or(&winds[i]).sound(&mut s, &mut shaper, &mut send)? {
                    break;
                }
            }
            Ok(())
        })();
        if let Err(e) = rendered {
            tracing::warn!("wind of {}: {e:#}", path.display());
            let _ = tx.blocking_send(Err(std::io::Error::other(format!("{e:#}"))));
        }
        tracing::info!(target: "rewind", track = %what2, from_ms = ask.from_ms, to_ms = ?ask.to_ms, dir = ?ask.dir, speed = ask.speed, fit = ?ask.fit,
            tracks = winds.len(), blocks, turned, first, ended = since(), "wind (ms since the request)");
    });
    let (bytes, speed, map) = match heard.await {
        Ok(Ok(h)) => h,
        Ok(Err((status, e))) => return (status, Json(json!({ "error": e }))).into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/wav"));
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(bytes));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert("x-wind-dir", HeaderValue::from_static(if q.dir == "back" { "back" } else { "forward" }));
    if let Some(f) = accel.filter(|a| a.audio.is_some() && a.brake.is_none()).map(|a| a.max)
        && let Ok(v) = HeaderValue::from_str(&f.to_string())
    {
        headers.insert("x-flight-max", v);
    }
    if let Ok(v) = HeaderValue::from_str(&format!("{speed:.4}")) {
        headers.insert("x-wind-speed", v);
    }
    if let Ok(v) = HeaderValue::from_str(&map) {
        headers.insert("x-wind-map", v);
    }
    (headers, axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).into_response()
}

/// A track as Opus in fragmented MP4, made as it is sent: a lossless one for a phone's stream, and with `lossy_too` one
/// the browser cannot take as it is. `None` when it makes none (a lossy file for a phone streams as it is). The headers
/// place it: 48 kHz, the encoder's delay (`x-pre-skip`) and the track's length.
async fn opus_stream(path: std::path::PathBuf, from_ms: u64, lossy_too: bool) -> Option<Response> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(2);
    let (said, heard) = tokio::sync::oneshot::channel();
    tokio::task::spawn_blocking(move || {
        let failed = tx.clone();
        let out = ChannelWriter { tx, buf: Vec::with_capacity(ChannelWriter::PIECE) };
        let told = |h| {
            let _ = said.send(h);
        };
        if let Err(e) = crate::opus_copy::stream_mp4(&path, from_ms, lossy_too, told, out) {
            // A closed body: the page stopped reading (a skip, a seek).
            let stopped = e.downcast_ref::<std::io::Error>().is_some_and(|e| e.kind() == std::io::ErrorKind::BrokenPipe);
            if !stopped {
                tracing::warn!("Opus stream of {}: {e:#}", path.display());
                let _ = failed.blocking_send(Err(std::io::Error::other(format!("{e:#}"))));
            }
        }
    });
    let head = heard.await.ok()?;
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/mp4; codecs=\"opus\""));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert("x-sample-rate", HeaderValue::from(48_000u32));
    headers.insert("x-pre-skip", HeaderValue::from(head.pre_skip));
    if let Some(total) = head.total {
        headers.insert("x-total-samples", HeaderValue::from(total));
    }
    Some((headers, axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).into_response())
}

async fn incoming(State(app): State<Arc<App>>, Query(f): Query<Follow>) -> Json<crate::app::IncomingView> {
    Json(app.incoming.list(f.source.as_deref()).await)
}

async fn collection(State(app): State<Arc<App>>, Query(f): Query<Follow>) -> Json<crate::app::CollectionView> {
    let mut v = app.collection.list(f.source.as_deref()).await;
    // Classical (0.10), as the Library's albums say it.
    for s in &mut v.sources {
        crate::app::classical::tell(&s.id, &mut s.albums).await;
    }
    Json(v)
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

/// Replace's ask (0.9.5): the year where the tags lack it, and the cover: `keep` the one in Music (the default, the
/// owner's choice), or the `new` copy's.
#[derive(Deserialize, Default)]
struct ReplaceBody {
    #[serde(default)]
    year: Option<String>,
    #[serde(default)]
    cover: Option<String>,
    /// Replace and carry over (0.9.7): the album's MusicBrainz ids and numbers written into the new copy first.
    #[serde(default)]
    carry: bool,
}

async fn replace_incoming(
    State(app): State<Arc<App>>,
    Path((source, album)): Path<(String, String)>,
    body: Option<Json<ReplaceBody>>,
) -> Result<Json<crate::app::KeepResult>, ApiError> {
    let b = body.map(|Json(b)| b).unwrap_or_default();
    let keep = match b.cover.as_deref() {
        None | Some("keep") => true,
        Some("new") => false,
        Some(_) => return Err(ToggleError::Refused("cover is keep or new".into()).into()),
    };
    Ok(Json(app.incoming.replace(&source, &album, b.year.as_deref(), keep, b.carry).await?))
}

/// Albums waiting for their hi-res files to be converted before Keep or Replace, and those just finished.
async fn incoming_jobs(State(app): State<Arc<App>>) -> Json<Vec<crate::app::JobView>> {
    Json(app.incoming.jobs())
}

async fn cancel_job(State(app): State<Arc<App>>, Path(id): Path<u64>) -> Result<Json<()>, ApiError> {
    Ok(Json(app.incoming.cancel(id).await?))
}

#[derive(Deserialize)]
struct CoverStamp {
    #[serde(default)]
    v: Option<String>,
}

/// An album's cover. Asked by its stamp (`?v=`, 0.9.7): kept for good, a changed cover having another address; else
/// for an hour (another server's album, a device's picture).
async fn album_cover(State(app): State<Arc<App>>, Path((source, album)): Path<(String, String)>, Query(q): Query<CoverStamp>) -> Response {
    match app.playing.album_cover(&source, &album).await {
        Some((content_type, bytes)) => {
            let ct = HeaderValue::from_str(&content_type).unwrap_or(HeaderValue::from_static("application/octet-stream"));
            let keep = if q.v.is_some_and(|v| !v.is_empty()) { "private, max-age=31536000, immutable" } else { "private, max-age=3600" };
            ([(header::CONTENT_TYPE, ct), (header::CACHE_CONTROL, HeaderValue::from_static(keep))], bytes).into_response()
        }
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "no cover" }))).into_response(),
    }
}

/// Every album's cover stamp in Cratarr's own library (0.9.7), as `{"<source>/<album>": "<stamp>"}`: the page puts it
/// on each cover's address; the live channel's `cover` tells it of changes.
async fn cover_stamps(ctx: Ctx) -> Json<serde_json::Value> {
    let app = ctx.app.clone();
    let Some(serve) = ctx.served.clone() else { return Json(json!({})) };
    let stamps = tokio::task::spawn_blocking(move || serve.cover_stamps()).await.unwrap_or_default();
    let mut out = serde_json::Map::new();
    for source in app.library.own_sources() {
        for (album, v) in stamps.iter() {
            out.insert(format!("{source}/{album}"), json!(v));
        }
    }
    Json(serde_json::Value::Object(out))
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
struct RelinkRequest {
    playlist: String,
    from: String,
    to: String,
}

/// A playlist's missing line pointed at a track Health found for it (its missing tracks; no repair offered it).
async fn relink_line(State(app): State<Arc<App>>, Json(req): Json<RelinkRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let (changed, undo) = app.playlists.relink(Repair { playlist: req.playlist, from: req.from, to: req.to }).await?;
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

#[derive(Deserialize)]
struct SharedRequest {
    shared: bool,
}

/// One's own playlist shared with everyone, or no longer (decision 0051).
async fn share_playlist(State(app): State<Arc<App>>, Path(id): Path<String>, Json(req): Json<SharedRequest>) -> Result<StatusCode, ApiError> {
    app.playlists.set_shared(&id, req.shared)?;
    Ok(StatusCode::NO_CONTENT)
}

/// A copy of a playlist (another's shared one, or one's own) among one's own, under a free name.
async fn copy_playlist(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let (name, undo) = app.playlists.copy(&id).await?;
    Ok((StatusCode::CREATED, Json(json!({ "playlist": name, "undo": undo }))))
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
    Ok(Json(crate::app::play::by_name(&app, &q.name, q.shuffle, q.limit.unwrap_or(50), q.source.as_deref()).await?))
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


/// Every domain's findings (decision 0063): the Health page's, and Home's System status's most critical and most recent.
async fn health_report(State(app): State<Arc<App>>) -> Json<crate::app::health::Report> {
    Json(app.health_report().await)
}

async fn album_duplicates(State(app): State<Arc<App>>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "groups": app.playlists.album_duplicates().await.map_err(ToggleError::from)? })))
}

/// Several playlists at once (0.9.4, the owner: one call for what is done to many ticked playlists, not one each).
/// Under `/api/batch/`: a word under `/api/playlists/` would hide a playlist of that name.
#[derive(Deserialize)]
struct ManyPlaylists {
    ids: Vec<String>,
    #[serde(default)]
    on: bool,
    #[serde(default)]
    source: Option<String>,
    /// A link's target (`link`); `shared` where none.
    #[serde(default)]
    target: Option<String>,
}

/// Most playlists one call acts on.
const PLAYLISTS_MOST: usize = 5_000;

impl ManyPlaylists {
    fn checked(self) -> Result<Self, ApiError> {
        if self.ids.len() > PLAYLISTS_MOST {
            return Err(ToggleError::Refused(format!("At most {PLAYLISTS_MOST} playlists at once.")).into());
        }
        Ok(self)
    }
}

/// One playlist an action on several could not do, and why.
fn refusal(id: &str, e: &ToggleError) -> serde_json::Value {
    json!({ "id": id, "error": crate::app::plain(e.to_string()) })
}

/// Several playlists favourited, or not (`on`), each as its own heart (decision 0023): those done, those not and why.
async fn playlists_favorite(ctx: Ctx, Json(b): Json<ManyPlaylists>) -> Result<Json<serde_json::Value>, ApiError> {
    let b = b.checked()?;
    let (mut done, mut not) = (Vec::new(), Vec::new());
    for id in &b.ids {
        match ctx.app.playlists.set_favorite(id, b.source.as_deref(), b.on).await {
            Ok(()) => {
                // Made now (decision 0087).
                if let Some(w) = LastWins::of(&ctx, ChangeAt(None), ChangeKind::Favourite, format!("playlist:{id}")) {
                    w.made(Op::of(b.on));
                }
                done.push(id.clone())
            }
            Err(e) => not.push(refusal(id, &e)),
        }
    }
    Ok(Json(json!({ "favorite": b.on, "done": done, "failed": not })))
}

/// Several playlists deleted, each as its own delete: those deleted, the undo tokens of them all (for `/api/undo` at
/// once: one Undo for the lot), those not and why.
async fn playlists_delete(State(app): State<Arc<App>>, Json(b): Json<ManyPlaylists>) -> Result<Json<serde_json::Value>, ApiError> {
    let b = b.checked()?;
    let (mut deleted, mut undo, mut not) = (Vec::new(), Vec::new(), Vec::new());
    for id in &b.ids {
        match app.playlists.delete(id).await {
            Ok(token) => {
                deleted.push(id.clone());
                undo.extend(token);
            }
            Err(e) => not.push(refusal(id, &e)),
        }
    }
    Ok(Json(json!({ "deleted": deleted, "undo": undo, "failed": not })))
}

/// Several playlists linked to a target (`on`), or the links taken off, each as its own (the Sync tab's Shared: target
/// `shared`): those done, those not and why.
async fn playlists_link(State(app): State<Arc<App>>, Json(b): Json<ManyPlaylists>) -> Result<Json<serde_json::Value>, ApiError> {
    let b = b.checked()?;
    let target = b.target.clone().unwrap_or_else(|| "shared".to_string());
    let (mut done, mut not) = (Vec::new(), Vec::new());
    for id in &b.ids {
        let r = if b.on { app.playlists.link(id, &target).await } else { app.playlists.unlink(id, &target).await };
        match r {
            Ok(()) => done.push(id.clone()),
            Err(e) => not.push(refusal(id, &e)),
        }
    }
    Ok(Json(json!({ "target": target, "on": b.on, "done": done, "failed": not })))
}

/// Several playlists' rows for their views (the Repair tab), each as `/api/playlists/{id}/rows` answers: in the order
/// asked, one that cannot be read with why.
async fn playlists_rows(State(app): State<Arc<App>>, Json(b): Json<ManyPlaylists>) -> Result<Json<serde_json::Value>, ApiError> {
    let b = b.checked()?;
    let mut out = Vec::with_capacity(b.ids.len());
    for id in &b.ids {
        out.push(match app.playlists.rows(id, b.source.as_deref()).await {
            Ok((source, rows)) => json!({ "id": id, "source": source, "rows": rows }),
            Err(e) => refusal(id, &e),
        });
    }
    Ok(Json(json!({ "playlists": out })))
}

/// One action's undo tokens (decision 0026): one, or one per playlist of an action on several.
#[derive(Deserialize)]
struct UndoRequest {
    tokens: Vec<String>,
}

/// The one undo (decision 0026): every token an action returned, undone newest first. Playlist and crate changes
/// answer with their redo tokens, in the order they were made: undone newest first in turn, the redo does the action
/// again in its own order (an edit before the rename that followed it). Incoming's tokens (its event ids) answer with
/// the albums' titles, and are the administrators' to undo, as their changes are (decision 0051).
async fn undo(ctx: Ctx, Json(req): Json<UndoRequest>) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    // Playlists' and crates' (`c…`) and Discover's (`d…`, the signed-in account's own) are anyone's; Incoming's are an
    // administrator's.
    if req.tokens.iter().any(|t| !t.starts_with('c') && !t.starts_with('d')) && !crate::serve::access::administering() {
        return Err(ToggleError::Forbidden("This is for administrators.".into()).into());
    }
    let (mut redo, mut restored) = (Vec::new(), Vec::new());
    for token in req.tokens.iter().rev() {
        if token.starts_with('d') {
            let (serve, account) = discoverer(&ctx)?;
            let t = token.clone();
            redo.push(
                tokio::task::spawn_blocking(move || crate::app::discover::undo(&serve, &account, &t))
                    .await
                    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
                    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?,
            );
        } else if token.starts_with('c') {
            redo.extend(app.playlists.undo(token).await?);
        } else if token.starts_with(crate::app::covers::UNDO) {
            crate::app::covers::undo(token).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
        } else if token.starts_with(crate::app::renumber::UNDO) {
            crate::app::renumber::undo(token).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
        } else if token.starts_with(crate::app::fixes::UNDO) {
            crate::app::fixes::undo(token).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
        } else if token.starts_with(crate::app::track_fetch::UNDO) {
            crate::app::track_fetch::undo(token).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
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

/// What this install can do (0.11, decision 0085), asked once a page load: the code's features and what is set up and
/// switched on, names only (`app::capabilities`).
async fn capabilities(ctx: Ctx) -> Result<Json<crate::app::capabilities::Capabilities>, ApiError> {
    Ok(Json(crate::app::capabilities::read(&ctx.app, ctx.served.clone()).await.map_err(ToggleError::from)?))
}

/// What changed since a device last looked (decision 0087): `?since=<rev>&kinds=playlist,favourite,rating,excluded`.
#[derive(Deserialize)]
struct ChangesQuery {
    since: Option<i64>,
    kinds: Option<String>,
}

async fn changes(ctx: Ctx, Query(q): Query<ChangesQuery>) -> Result<Json<crate::app::changes::Changes>, ApiError> {
    Ok(Json(crate::app::changes::read(&ctx.app, ctx.serve()?, q.since, q.kinds.as_deref()).await?))
}

#[derive(Deserialize)]
struct NamingQuery {
    template: Option<String>,
}

/// A naming template measured against Music: the one typed (`?template=`), else the one in use.
async fn naming(State(app): State<Arc<App>>, Query(q): Query<NamingQuery>) -> Result<Json<crate::app::NamingView>, ApiError> {
    Ok(Json(app.settings.naming(q.template.as_deref()).await?))
}

/// The version and when this run started, for everyone (About).
async fn about(State(app): State<Arc<App>>) -> Result<Json<serde_json::Value>, ApiError> {
    let v = app.settings.view().await.map_err(ToggleError::from)?;
    // Its licence and where its source is (the AGPL offers the source to whoever uses it over the network, 0029).
    Ok(Json(json!({ "version": v.version, "started": v.started, "licence": env!("CARGO_PKG_LICENSE"), "source": env!("CARGO_PKG_REPOSITORY") })))
}

/// The servers and their music libraries, each with its use, for everyone (the filter): no secrets, values or folders.
async fn libraries(State(app): State<Arc<App>>) -> Result<Json<serde_json::Value>, ApiError> {
    let v = app.settings.view().await.map_err(ToggleError::from)?;
    let sources: Vec<serde_json::Value> = v
        .sources
        .iter()
        .map(|s| json!({ "id": s.id, "label": s.label, "status": { "server_name": s.status.server_name }, "server_libraries": s.server_libraries, "libraries": s.libraries }))
        .collect();
    Ok(Json(json!({ "sources": sources })))
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

/// `?path=`: a folder of Cratarr's (none: the top, its mounts).
#[derive(Deserialize)]
struct FolderAt {
    path: Option<String>,
}

/// The folder browser (decision 0044): what Cratarr can see at a place, to add a folder.
async fn browse_folders(Query(q): Query<FolderAt>) -> Response {
    match tokio::task::spawn_blocking(move || crate::folders::browse(q.path.as_deref(), &crate::folders::roots())).await {
        Ok(Ok(listing)) => Json(listing).into_response(),
        Ok(Err(why)) => (StatusCode::BAD_REQUEST, Json(json!({ "error": why }))).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// The music folders Cratarr found under its mounts that are not chosen yet, nor inside or around a chosen one
/// (decision 0044): the browser's suggestions.
async fn suggested_folders(State(app): State<Arc<App>>) -> Response {
    let chosen = app.library.chosen_folders();
    match tokio::task::spawn_blocking(move || crate::source::cratarr::suggested_folders(&chosen)).await {
        Ok(found) => Json(json!({ "folders": found.iter().map(|p| p.display().to_string()).collect::<Vec<_>>() })).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// A folder's row (decision 0044): whether it is there and writable, its free space, and the tracks Cratarr knows in it.
async fn folder_status(State(app): State<Arc<App>>, Query(q): Query<FolderAt>) -> Response {
    let Some(path) = q.path.filter(|p| !p.trim().is_empty()) else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Which folder?" }))).into_response();
    };
    let cat = app.library.catalogue();
    let looked = tokio::task::spawn_blocking(move || {
        let roots = crate::folders::roots();
        // A folder that is gone is still told about (its row says so); one outside the mounts is not looked at.
        let there = crate::folders::inside(&path, &roots);
        if there.is_none() && std::path::Path::new(&path).exists() {
            return Err("Cratarr cannot see that folder.".to_string());
        }
        let dir = there.unwrap_or_else(|| std::path::PathBuf::from(&path));
        let tracks = cat.tracks_per_folder().ok().and_then(|all| all.into_iter().find(|(p, _)| *p == dir || p.as_path() == std::path::Path::new(&path)).map(|(_, n)| n));
        Ok((crate::folders::status(&dir), tracks))
    })
    .await;
    match looked {
        Ok(Ok((s, tracks))) => Json(json!({ "exists": s.exists, "writable": s.writable, "free": s.free, "tracks": tracks })).into_response(),
        Ok(Err(why)) => (StatusCode::BAD_REQUEST, Json(json!({ "error": why }))).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
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
    mode: crate::app::play::How,
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

/// Plays a crate, a playlist, an album, an artist or tracks: the server picks at most 500 of them (app/play.rs), answers
/// with them, and queues them itself when asked (`on`).
async fn play_collection(
    State(app): State<Arc<App>>,
    Query(f): Query<Follow>,
    Json(r): Json<crate::app::play::Request>,
) -> Result<Json<crate::app::play::Played>, ApiError> {
    Ok(Json(crate::app::play::play(&app, f.source.as_deref(), r).await?))
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

#[derive(Deserialize)]
struct ListenBrainzImport {
    /// The ListenBrainz user whose listens are the asking account's.
    user: String,
}

/// The asking account's listening history from ListenBrainz into its log (decision 0064, amended): how many listens
/// there were, matched to the library, and new to the log.
/// Whose history a ListenBrainz import goes into, and whose listens (decision 0067).
fn listenbrainz_who(ctx: &Ctx, b: &ListenBrainzImport) -> Result<(Arc<crate::serve::Serve>, String, String), ApiError> {
    let serve = ctx.serve()?;
    let account = ctx.account().ok_or_else(|| ToggleError::Refused("Sign in first: the history is an account's own.".into()))?;
    let user = b.user.trim();
    if user.is_empty() {
        return Err(ToggleError::Refused("Say whose ListenBrainz listens.".into()).into());
    }
    Ok((serve, account, user.to_string()))
}

/// The asking account's tasks (`app::tasks`): running, or ended within the hour, with their results.
async fn own_tasks(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    ctx.serve()?;
    let account = ctx.account().ok_or_else(|| ToggleError::Refused("Sign in first: tasks are an account's own.".into()))?;
    Ok(Json(json!({ "tasks": crate::app::tasks::tasks().of(&account) })))
}

/// One's own running task asked to stop (0.9.6: a cover picked ends its look); refused when it is not running. Work that
/// cannot stop part-way runs on.
async fn stop_task(ctx: Ctx, Path(id): Path<u64>) -> Result<StatusCode, ApiError> {
    ctx.serve()?;
    let account = ctx.account().ok_or_else(|| ToggleError::Refused("Sign in first: tasks are an account's own.".into()))?;
    if crate::app::tasks::tasks().stop(&account, id) { Ok(StatusCode::NO_CONTENT) } else { Err(ToggleError::Refused("That task is not running.".into()).into()) }
}

#[derive(Deserialize)]
struct DescribeBody {
    tracks: Vec<crate::app::describe::Asked>,
}

/// What these tracks are (0.9.6, decision 0033 amended: a queue holds identities, Cratarr describes them), in the order
/// asked, with the signed-in account's favourites; for the browser's player and the app's notification.
async fn describe_tracks(ctx: Ctx, Json(b): Json<DescribeBody>) -> Result<Json<serde_json::Value>, ApiError> {
    if b.tracks.len() > crate::app::describe::MOST {
        return Err(ToggleError::Refused(format!("At most {} tracks at once.", crate::app::describe::MOST)).into());
    }
    let account = ctx.account();
    let tracks = crate::app::describe::describe(b.tracks, account).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "tracks": tracks })))
}

/// Long work begun as a task: 202 with the task, which the page follows (`app::tasks`).
fn started(task: Result<crate::app::tasks::Task, String>) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let task = task.map_err(ToggleError::Refused)?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "task": task }))))
}

fn progress_of(h: Arc<crate::app::tasks::Handle>) -> impl Fn(&str, u64, Option<u64>) + Send + Sync {
    move |name, done, total| h.step(name, done, total)
}

/// Refused while the asking account keeps no play history (0.9.5, Tracking off): listening is not brought in either.
fn keeps_history(ctx: &Ctx) -> Result<(), ApiError> {
    match ctx.served.as_ref().and_then(|s| ctx.account().map(|me| s.userdata.tracking(&me))) {
        Some(false) => Err(ToggleError::Refused("Play history is off for this account (Settings › Tracking).".into()).into()),
        _ => Ok(()),
    }
}

async fn import_listenbrainz(ctx: Ctx, Json(b): Json<ListenBrainzImport>) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    keeps_history(&ctx)?;
    bring_listenbrainz(&ctx, b, false)
}

async fn preview_listenbrainz(ctx: Ctx, Json(b): Json<ListenBrainzImport>) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    bring_listenbrainz(&ctx, b, true)
}

/// The ListenBrainz user an account imports from, kept (an import choice, as Last.fm's): its next import, and
/// Discover without a scrobble connection (the owner, 2026-10-24).
fn listenbrainz_choice(account: &str) -> String {
    format!("listenbrainz:{account}")
}

/// The ListenBrainz user the asking account imported from last.
async fn listenbrainz_kept(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let serve = ctx.serve()?;
    let account = ctx.account().ok_or_else(|| ToggleError::Refused("Sign in first: the history is an account's own.".into()))?;
    let ud = serve.userdata.clone();
    let user = tokio::task::spawn_blocking(move || ud.import_choice(&listenbrainz_choice(&account))).await.ok().and_then(Result::ok).flatten();
    Ok(Json(json!({ "user": user })))
}

fn bring_listenbrainz(ctx: &Ctx, b: ListenBrainzImport, preview: bool) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let (serve, account, user) = listenbrainz_who(ctx, &b)?;
    // Kept for next time, and for Discover.
    if let Err(e) = serve.userdata.set_import_choice(&listenbrainz_choice(&account), &user) {
        tracing::warn!("import: the ListenBrainz user could not be kept: {e:#}");
    }
    started(crate::app::tasks::spawn("import:listenbrainz", &account.clone(), preview, move |h| async move {
        let progress = progress_of(h);
        if preview {
            crate::app::listenbrainz::preview(&serve, &account, &user, &progress).await
        } else {
            crate::app::listenbrainz::import(&serve, &account, &user, &progress).await
        }
    }))
}

/// Every open request's mark (0.9.5), for lists of albums not in the library; none when they cannot be read (the list
/// is shown without them).
async fn request_marks(app: &App, serve: &crate::serve::Serve) -> Option<Arc<std::collections::HashMap<String, crate::app::requests::Mark>>> {
    crate::app::requests::marks(app, serve).await.inspect_err(|e| tracing::debug!("the requests' marks not read: {e:#}")).ok()
}

/// The asking account and Cratarr's own library, for Discover (decision 0069).
fn discoverer(ctx: &Ctx) -> Result<(Arc<crate::serve::Serve>, String), ApiError> {
    let serve = ctx.serve()?;
    let account = ctx.account().ok_or_else(|| ToggleError::Refused("Sign in first: Discover is an account's own.".into()))?;
    Ok((serve, account))
}

/// Discover (decision 0069): the account's suggestions as last found, which of its services it asks, and whether Want
/// can fetch through Lidarr. Looked up again, as a task the page follows, when the last look is a day old.
async fn discover(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    let (serve, account) = discoverer(&ctx)?;
    let (s, a) = (serve.clone(), account.clone());
    let (found, who) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> { Ok((crate::app::discover::kept(&s, &a)?, crate::app::discover::who(&s, &a)?)) })
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    let services = json!({ "listenbrainz": who.listenbrainz.is_some(), "lastfm": who.lastfm.is_some() });
    let stale = found.as_ref().is_none_or(|f| crate::app::moment::now_secs() - f.at > crate::app::discover::FRESH_SECS);
    let running = crate::app::tasks::tasks().of(&account).iter().any(|t| t.kind == crate::app::discover::KIND && t.finished.is_none());
    // Every account looks: the library's discographies need no service (0069 amended, 0.9).
    if stale && !running {
        let _ = crate::app::discover::start(&app, serve.clone(), &account, who);
    }
    // Each card's request, if it was asked for, with its album's state (decision 0070).
    let keys: Vec<String> = found.as_ref().map(|f| f.albums.iter().map(|s| s.key.clone()).collect()).unwrap_or_default();
    let asked: serde_json::Map<String, serde_json::Value> = crate::app::requests::of_suggestions(&app, &serve, &account, &keys)
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .into_iter()
        .map(|(k, v)| (k, json!(v)))
        .collect();
    let may = crate::app::requests::rights(&serve);
    // The cards by one of the account's favourite artists (its starred artists, by name as the library keys them), for
    // the filter's "Leave out favourite artists" (the owner, 2026-10-24).
    let starred: std::collections::HashSet<String> =
        serve.userdata.starred(&account, crate::userdata::Kind::Artist).map_err(|e| ToggleError::Refused(format!("{e:#}")))?.into_iter().collect();
    let favourite_artists: Vec<&str> = found
        .as_ref()
        .map(|f| f.albums.iter().filter(|s| starred.contains(&crate::serve::ids::artist(&s.artist))).map(|s| s.key.as_str()).collect())
        .unwrap_or_default();
    let mut found = json!(found);
    if let Some(cards) = found.get_mut("albums").and_then(|a| a.as_array_mut()) {
        mark_cards(&app, &serve, cards).await;
    }
    Ok(Json(json!({
        "found": found,
        "services": services,
        "lidarr": crate::app::discover::lidarr(&app).is_some(),
        "requests": asked,
        "may_request": may.request,
        "favourite_artists": favourite_artists,
    })))
}

/// Discover looked up again now: 202 with its task.
async fn discover_refresh(ctx: Ctx) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let app = ctx.app.clone();
    let (serve, account) = discoverer(&ctx)?;
    let (s, a) = (serve.clone(), account.clone());
    let who = tokio::task::spawn_blocking(move || crate::app::discover::who(&s, &a))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    started(crate::app::discover::start(&app, serve, &account, who))
}

/// Each card's mark (0.9.5): any account's open request for its album, wanted or coming.
async fn mark_cards(app: &App, serve: &crate::serve::Serve, cards: &mut [serde_json::Value]) {
    let Some(marks) = request_marks(app, serve).await else { return };
    for c in cards {
        let mark = crate::app::requests::mark_of(
            &marks,
            c["release_group"].as_str().or_else(|| c["key"].as_str()),
            c["artist"].as_str().unwrap_or_default(),
            c["album"].as_str().unwrap_or_default(),
        );
        if let Some(m) = mark {
            c["request"] = json!(m);
        }
    }
}

/// Around artists you play (0.9.7, app::around): the group showing, its seeds and their suggestions, and whether it is
/// still being made (asked again until it is not).
async fn discover_around(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    let (serve, account) = discoverer(&ctx)?;
    let shown = crate::app::around::showing(&app, &serve, &account).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(around_json(&app, &serve, shown).await))
}

/// Around's Refresh: on to the next group, the three most played artists not shown yet.
async fn discover_around_next(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    let (serve, account) = discoverer(&ctx)?;
    let shown = crate::app::around::next(&app, &serve, &account).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(around_json(&app, &serve, shown).await))
}

async fn around_json(app: &App, serve: &crate::serve::Serve, (seeds, albums, making): (Vec<String>, Vec<crate::app::discover::Suggestion>, bool)) -> serde_json::Value {
    let mut cards: Vec<serde_json::Value> = albums.iter().map(|s| json!(s)).collect();
    mark_cards(app, serve, &mut cards).await;
    json!({ "seeds": seeds, "albums": cards, "making": making })
}

#[derive(Deserialize)]
struct DiscoverKey {
    key: String,
}

/// A suggestion wanted: a request (decision 0070), handed to Lidarr at once where the account may (an administrator's,
/// or Auto-approve), else waiting for an answer.
async fn discover_want(ctx: Ctx, Json(b): Json<DiscoverKey>) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    let (serve, account) = discoverer(&ctx)?;
    let r = crate::app::requests::ask(&app, &serve, &account, &b.key).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "request": r })))
}

#[derive(Deserialize)]
struct WantAlbum {
    #[serde(default)]
    release_group: Option<String>,
    artist: String,
    album: String,
    #[serde(default)]
    year: Option<i32>,
    #[serde(default)]
    cover: Option<String>,
}

/// Any album wanted (0.9.3: an artist's missing album), as Discover's Want: a request, handed to Lidarr where the
/// account may.
async fn want_album(ctx: Ctx, Json(b): Json<WantAlbum>) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    let (serve, account) = discoverer(&ctx)?;
    let r = crate::app::requests::ask_album(&app, &serve, &account, b.release_group, b.artist.trim(), b.album.trim(), b.year, b.cover)
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "request": r })))
}

/// An artist's missing albums (0.9.3): their albums and EPs the library has none of, newest first; read when asked
/// (a MusicBrainz call or two the first time, kept for every account).
async fn artist_missing(ctx: Ctx, Query(q): Query<AboutQuery>) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    let serve = ctx.serve()?;
    let mut albums = crate::app::discover::missing_albums(&serve, app.library.catalogue(), q.name.trim(), q.mbid.as_deref(), q.release_group.as_deref()).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    if let Some(marks) = request_marks(&app, &serve).await {
        for a in &mut albums {
            a.request = crate::app::requests::mark_of(&marks, Some(&a.release_group), q.name.trim(), &a.title);
        }
    }
    Ok(Json(json!({ "albums": albums, "looking": false })))
}

#[derive(Deserialize)]
struct MbSearch {
    #[serde(default)]
    q: String,
    #[serde(default)]
    limit: Option<usize>,
}

/// "Find more music" (0.9.3): the words looked up on MusicBrainz, albums and artists, each marked where the library
/// has it.
async fn musicbrainz_search(ctx: Ctx, Query(q): Query<MbSearch>) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    let serve = ctx.serve()?;
    let (albums, artists) = crate::app::discover::musicbrainz_search(&serve, app.library.catalogue(), &q.q, q.limit.unwrap_or(20))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "albums": albums, "artists": artists })))
}

/// Every missing album of the library's artists Discover has read (Insights › Discovery › Missing albums, 0.9.3).
async fn discover_missing(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    let serve = ctx.serve()?;
    let (mut albums, artists) = crate::app::discover::all_missing(&serve, app.library.catalogue()).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    // None by an artist the account excluded everywhere (decision 0083).
    if let Some(account) = ctx.account() {
        let s = serve.clone();
        let out = tokio::task::spawn_blocking(move || s.userdata.excluded_artists(&account))
            .await
            .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
            .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
        albums.retain(|a| !crate::app::excluded_artists::credits_excluded(&[a.artist.as_str()], &out));
    }
    if let Some(marks) = request_marks(&app, &serve).await {
        for a in &mut albums {
            a.request = crate::app::requests::mark_of(&marks, Some(&a.release_group), &a.artist, &a.title);
        }
    }
    Ok(Json(json!({ "albums": albums, "artists": artists })))
}

/// The account's Discover exclusions (Insights › Discovery › Discovery exclusions, 0.9.3): albums and artists.
async fn discover_excluded(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let (serve, account) = discoverer(&ctx)?;
    let (albums, artists) = tokio::task::spawn_blocking(move || crate::app::discover::exclusions(&serve, &account))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "albums": albums, "artists": artists })))
}

#[derive(Deserialize)]
struct TakeBack {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    artist: Option<String>,
}

/// An exclusion taken back, any time: an album suggested again, or an artist.
async fn discover_take_back(ctx: Ctx, Query(q): Query<TakeBack>) -> Result<StatusCode, ApiError> {
    let (serve, account) = discoverer(&ctx)?;
    tokio::task::spawn_blocking(move || crate::app::discover::take_back(&serve, &account, q.key.as_deref(), q.artist.as_deref()))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct FindOnBandcamp {
    artist: String,
    album: String,
    #[serde(default)]
    release_group: Option<String>,
}

/// An album's page on Bandcamp, looked up when it is opened (0.9.3), or null.
async fn bandcamp_find(ctx: Ctx, Query(q): Query<FindOnBandcamp>) -> Result<Json<serde_json::Value>, ApiError> {
    let serve = ctx.serve()?;
    let page = crate::app::discover::find_on_bandcamp(&serve, q.artist.trim(), q.album.trim(), q.release_group.as_deref())
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "page": page })))
}

#[derive(Deserialize)]
struct BandcampOf {
    url: String,
}

/// A Bandcamp album to play as a preview (decision 0071): its tracks as players hold them (`QueueTrack`), each
/// streamed and its cover shown through Cratarr's own addresses.
async fn preview_bandcamp(Query(q): Query<BandcampOf>) -> Result<Json<serde_json::Value>, ApiError> {
    let a = crate::bandcamp::Bandcamp::get().album(&q.url).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    let tracks = a.queue_tracks();
    if tracks.is_empty() {
        return Err(ToggleError::Refused("None of its tracks can be heard on Bandcamp.".into()).into());
    }
    Ok(Json(json!({ "album": a.title, "artist": a.artist, "url": a.url, "tracks": tracks })))
}

#[derive(Deserialize)]
struct BandcampArtistOf {
    name: String,
    #[serde(default)]
    mbid: Option<String>,
    /// Their own Bandcamp page where Discover found it (`artist_bandcamp`).
    #[serde(default)]
    page: Option<String>,
}

/// An artist's releases on Bandcamp kept a while, by name: Bandcamp is asked once for them.
static BANDCAMP_ARTISTS: std::sync::Mutex<Option<std::collections::HashMap<String, (std::time::Instant, serde_json::Value)>>> = std::sync::Mutex::new(None);
/// How long they are kept.
const BANDCAMP_ARTIST_FOR: std::time::Duration = std::time::Duration::from_secs(6 * 3600);
/// The most of an artist's releases one Listen plays.
const BANDCAMP_ARTIST_RELEASES: usize = 10;

/// Everything an artist has on Bandcamp to play as previews (New to you's Listen, the owner 2026-10-27): their own
/// page, its releases newest first (at most 10), each as `/api/preview/bandcamp` gives it. None found: no albums.
async fn preview_bandcamp_artist(Query(q): Query<BandcampArtistOf>) -> Result<Json<serde_json::Value>, ApiError> {
    let key = crate::app::import::key(q.name.trim());
    {
        let kept = BANDCAMP_ARTISTS.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((at, v)) = kept.as_ref().and_then(|k| k.get(&key))
            && at.elapsed() < BANDCAMP_ARTIST_FOR
        {
            return Ok(Json(v.clone()));
        }
    }
    let (page, releases) = crate::app::discover::bandcamp_artist(q.name.trim(), q.mbid.as_deref(), q.page.as_deref())
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    let bc = crate::bandcamp::Bandcamp::get();
    let mut albums = Vec::new();
    for (_, url) in releases.into_iter().take(BANDCAMP_ARTIST_RELEASES) {
        let Ok(a) = bc.album(&url).await else { continue };
        let tracks = a.queue_tracks();
        if tracks.is_empty() {
            continue;
        }
        let year = a.released.as_deref().and_then(|d| d.get(..4)).and_then(|y| y.parse::<i32>().ok());
        albums.push(json!({ "album": a.title, "url": a.url, "year": year, "tracks": tracks }));
    }
    let v = json!({ "artist": q.name.trim(), "page": page, "albums": albums });
    BANDCAMP_ARTISTS.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(Default::default).insert(key, (std::time::Instant::now(), v.clone()));
    Ok(Json(v))
}

/// The most of a preview's stream read before it is sent when it starts at a moment, to find its first whole frame
/// there (a frame is 417 or 418 bytes at 128 kb/s).
const PREVIEW_HEAD: usize = 4096;

/// A preview's track relayed from Bandcamp as it is sent (nothing kept): its MP3 as it is, whatever form a player asks
/// for, with its length from its album's page (`x-duration-ms`).
/// - From its start: its status, length and ranges as Bandcamp gives them (`range`, the player's own, passed on), so a
///   player can seek in it.
/// - From a moment (`from_ms`, the page's stream): from the place in the file it is at (128 kb/s throughout), at the
///   first whole frame there, its time said as a library MP3's is (`x-start-ms`), with no length of its own.
async fn bandcamp_relay(id: &str, range: Option<&str>, from_ms: u64) -> Response {
    let Some((url, n)) = crate::bandcamp::read_id(id).filter(|(_, n)| *n > 0) else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "No such preview." }))).into_response();
    };
    let at = (from_ms > 0).then(|| format!("bytes={}-", from_ms * crate::bandcamp::BYTES_PER_SEC / 1000));
    let (mut resp, duration) = match crate::bandcamp::Bandcamp::get().stream(&url, n, at.as_deref().or(range)).await {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("{e:#}") }))).into_response(),
    };
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/mpeg"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(d) = duration {
        headers.insert("x-duration-ms", HeaderValue::from((d * 1000.0) as u64));
    }
    // A moment asked and Bandcamp sent from there: from its first whole frame. Else as Bandcamp sent it (from its start
    // where it took no range: the player places it as a whole track).
    let moved = at.is_some() && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let (mut head, mut ended) = (Vec::new(), false);
    let mut status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::OK);
    if moved {
        while head.len() < PREVIEW_HEAD && !ended {
            match resp.chunk().await {
                Ok(Some(b)) => head.extend_from_slice(&b),
                Ok(None) => ended = true,
                Err(_) => return (StatusCode::BAD_GATEWAY, Json(json!({ "error": "Bandcamp stopped sending it." }))).into_response(),
            }
        }
        let skip = crate::stream::first_frame(&head).unwrap_or(0);
        head = head.split_off(skip);
        headers.insert("x-start-ms", HeaderValue::from(from_ms + skip as u64 * 1000 / crate::bandcamp::BYTES_PER_SEC));
        status = StatusCode::OK;
    } else {
        for h in [header::CONTENT_LENGTH, header::CONTENT_RANGE, header::ACCEPT_RANGES] {
            if let Some(v) = resp.headers().get(&h) {
                headers.insert(h, v.clone());
            }
        }
    }
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(4);
    tokio::spawn(async move {
        if !head.is_empty() && tx.send(Ok(axum::body::Bytes::from(head))).await.is_err() {
            return;
        }
        if ended {
            return;
        }
        loop {
            match resp.chunk().await {
                Ok(Some(b)) => {
                    if tx.send(Ok(b)).await.is_err() {
                        return;
                    }
                }
                Ok(None) => return,
                Err(e) => {
                    let _ = tx.send(Err(std::io::Error::other(e))).await;
                    return;
                }
            }
        }
    });
    (status, headers, axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).into_response()
}

/// A preview's cover: its album page's picture, kept by Cratarr (app::cover_cache).
async fn bandcamp_cover(id: &str) -> Response {
    let Some((page, _)) = crate::bandcamp::read_id(id) else { return (StatusCode::NOT_FOUND, Json(json!({ "error": "no cover" }))).into_response() };
    kept_cover(crate::app::cover_cache::Of::Bandcamp(page)).await
}

/// A cover Cratarr keeps for an album not in the library (0.9.5), as an image; 404 where it has none.
async fn kept_cover(of: crate::app::cover_cache::Of) -> Response {
    match crate::app::cover_cache::cover(&of).await {
        Ok(Some((bytes, mime))) => {
            ([(header::CONTENT_TYPE, HeaderValue::from_static(mime)), (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=604800"))], bytes).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({ "error": "no cover" }))).into_response(),
        Err(e) => {
            tracing::debug!("cover of {of:?}: {e:#}");
            (StatusCode::NOT_FOUND, Json(json!({ "error": "no cover" }))).into_response()
        }
    }
}

/// The Requests page (decision 0070): the account's requests, everyone's waiting ones for those who answer them.
async fn requests(ctx: Ctx) -> Result<Json<crate::app::requests::Listing>, ApiError> {
    let app = ctx.app.clone();
    let (serve, account) = discoverer(&ctx)?;
    Ok(Json(crate::app::requests::list(&app, &serve, &account).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?))
}

/// What Lidarr is looking for and fetching now; `null` without Lidarr.
async fn requests_watching(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    discoverer(&ctx)?;
    let w = crate::app::requests::watching(&app).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!(w)))
}

#[derive(Deserialize)]
struct RequestsDecide {
    ids: Vec<i64>,
    approve: bool,
    #[serde(default)]
    reason: Option<String>,
}

/// Requests answered, one or many: each id's outcome, in words.
async fn requests_decide(ctx: Ctx, Json(b): Json<RequestsDecide>) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    let (serve, account) = discoverer(&ctx)?;
    let out = crate::app::requests::decide(&app, &serve, &account, &b.ids, b.approve, b.reason.as_deref())
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!(out.into_iter().map(|(id, said)| json!({ "id": id, "said": said })).collect::<Vec<_>>())))
}

/// A request taken back.
async fn requests_cancel(ctx: Ctx, Path(id): Path<i64>) -> Result<StatusCode, ApiError> {
    let app = ctx.app.clone();
    let (serve, account) = discoverer(&ctx)?;
    crate::app::requests::cancel(&app, &serve, &account, id).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(StatusCode::NO_CONTENT)
}

/// A suggestion not wanted: it never comes back. Its undo token.
async fn discover_no(ctx: Ctx, Json(b): Json<DiscoverKey>) -> Result<Json<serde_json::Value>, ApiError> {
    let (serve, account) = discoverer(&ctx)?;
    let undo = tokio::task::spawn_blocking(move || crate::app::discover::not_interested(&serve, &account, &b.key))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "undo": undo })))
}

/// Not interested in an artist (the owner, 2026-10-29): they and every record of theirs leave Discover for good. Its
/// undo token.
async fn discover_artist_not(ctx: Ctx, Path(name): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let (serve, account) = discoverer(&ctx)?;
    let undo = tokio::task::spawn_blocking(move || crate::app::discover::artist_not_interested(&serve, &account, &name))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "undo": undo })))
}

/// A suggestion hidden: it leaves the list, its artist not held against it. Its undo token.
async fn discover_hide(ctx: Ctx, Json(b): Json<DiscoverKey>) -> Result<Json<serde_json::Value>, ApiError> {
    let (serve, account) = discoverer(&ctx)?;
    let undo = tokio::task::spawn_blocking(move || crate::app::discover::hide(&serve, &account, &b.key))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "undo": undo })))
}

/// The asking account and Cratarr's own library, for scrobbling (decision 0068).
fn scrobbler(ctx: &Ctx) -> Result<(Arc<crate::serve::Serve>, String), ApiError> {
    let serve = ctx.serve()?;
    let account = ctx.account().ok_or_else(|| ToggleError::Refused("Sign in first: scrobbling is an account's own.".into()))?;
    Ok((serve, account))
}

fn scrobble_refused(e: impl std::fmt::Display) -> ApiError {
    ToggleError::Refused(e.to_string()).into()
}

/// The asking account's listening services (Connections): whom it scrobbles to, never a secret.
async fn scrobble_targets(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let (serve, account) = scrobbler(&ctx)?;
    let targets = tokio::task::spawn_blocking(move || serve.userdata.scrobble_targets(&account)).await.map_err(scrobble_refused)?.map_err(|e| scrobble_refused(format!("{e:#}")))?;
    Ok(Json(json!({ "targets": targets })))
}

#[derive(Deserialize)]
struct ListenBrainzToken {
    token: String,
}

/// ListenBrainz connected with the user token from its settings page, checked first.
async fn scrobble_listenbrainz(ctx: Ctx, Json(b): Json<ListenBrainzToken>) -> Result<Json<serde_json::Value>, ApiError> {
    let (serve, account) = scrobbler(&ctx)?;
    let token = b.token.trim().to_string();
    let name = crate::app::scrobble::listenbrainz_user(&token).await.map_err(|e| scrobble_refused(format!("{e:#}")))?;
    let secret = serde_json::to_string(&crate::app::scrobble::Secret { token: Some(token), ..Default::default() }).map_err(scrobble_refused)?;
    let n = name.clone();
    tokio::task::spawn_blocking(move || serve.userdata.set_scrobble_target(&account, crate::app::scrobble::LISTENBRAINZ, &n, &secret))
        .await
        .map_err(scrobble_refused)?
        .map_err(|e| scrobble_refused(format!("{e:#}")))?;
    Ok(Json(json!({ "name": name })))
}

#[derive(Deserialize)]
struct LastFmKeys {
    api_key: String,
    api_secret: String,
    /// Where Last.fm's sign-in comes back to (this address's `/api/scrobble/lastfm/callback`), its code added.
    callback: String,
    /// Begun in the Android app: the way back ends in the app.
    #[serde(default)]
    app: bool,
}

/// Last.fm: the API key and its shared secret kept (not sending yet), and where to send the browser to sign in.
async fn scrobble_lastfm(ctx: Ctx, Json(b): Json<LastFmKeys>) -> Result<Json<serde_json::Value>, ApiError> {
    let (serve, account) = scrobbler(&ctx)?;
    let (key, secret) = (b.api_key.trim().to_string(), b.api_secret.trim().to_string());
    if key.is_empty() || secret.is_empty() {
        return Err(scrobble_refused("Last.fm needs the API key and its shared secret, from last.fm/api/account/create."));
    }
    let code = crate::app::scrobble::pending(crate::app::scrobble::Pending { account: account.clone(), app: b.app });
    let callback = format!("{}/{code}", b.callback.trim_end_matches('/'));
    let url = crate::app::scrobble::lastfm_sign_in(&key, &callback).map_err(|e| scrobble_refused(format!("{e:#}")))?;
    let kept = serde_json::to_string(&crate::app::scrobble::Secret { api_key: Some(key), api_secret: Some(secret), ..Default::default() }).map_err(scrobble_refused)?;
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        serve.userdata.set_scrobble_target(&account, crate::app::scrobble::LASTFM, "", &kept)?;
        serve.userdata.scrobble_broken(&account, crate::app::scrobble::LASTFM, "Not signed in at Last.fm yet")
    })
    .await
    .map_err(scrobble_refused)?
    .map_err(|e| scrobble_refused(format!("{e:#}")))?;
    Ok(Json(json!({ "url": url })))
}

#[derive(Deserialize)]
struct LastFmBack {
    token: String,
}

/// Last.fm's sign-in coming back: its token traded for a session key, and the browser back to Connections.
async fn scrobble_lastfm_callback(ctx: Ctx, Path(code): Path<String>, Query(q): Query<LastFmBack>) -> Response {
    let pending = crate::app::scrobble::take_pending(&code);
    let app = pending.as_ref().is_some_and(|p| p.app);
    let back = move |why: Option<String>| {
        let why = why.map(|w| format!("?scrobble={}", urlencoding_lite(&w))).unwrap_or_default();
        if app {
            // The Android app's own link: a page that opens it (and a link to tap, where the browser asks first).
            let to = format!("cratarr://connected/lastfm{why}");
            let page = format!(
                "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width\"><meta http-equiv=refresh content=\"0;url={to}\"><title>Cratarr</title><p style=\"font:16px system-ui;margin:2em\"><a href=\"{to}\">Back to Cratarr</a>"
            );
            return ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], page).into_response();
        }
        let to = format!("/#/settings?view=settings%3Atracking%2Flastfm{}", why.replacen('?', "&", 1));
        axum::response::Redirect::to(&to).into_response()
    };
    let Some(pending) = pending else { return back(Some("That sign-in has expired; sign in again".into())) };
    let Some(serve) = ctx.served.clone() else { return back(Some("Cratarr's own library is not open".into())) };
    let account = pending.account;
    let (ud, a) = (serve.userdata.clone(), account.clone());
    let kept = tokio::task::spawn_blocking(move || ud.scrobble_secret_any(&a, crate::app::scrobble::LASTFM)).await.ok().and_then(Result::ok).flatten();
    let Some(mut secret) = kept.and_then(|k| serde_json::from_str::<crate::app::scrobble::Secret>(&k).ok()) else {
        return back(Some("Give Last.fm's API key first".into()));
    };
    let (Some(key), Some(sec)) = (secret.api_key.clone(), secret.api_secret.clone()) else { return back(Some("Give Last.fm's API key first".into())) };
    match crate::app::scrobble::lastfm_session(&key, &sec, q.token.trim()).await {
        Ok((session, name)) => {
            secret.session = Some(session);
            let Ok(kept) = serde_json::to_string(&secret) else { return back(Some("Could not keep the sign-in".into())) };
            let ok = tokio::task::spawn_blocking(move || serve.userdata.set_scrobble_target(&account, crate::app::scrobble::LASTFM, &name, &kept)).await;
            back(match ok {
                Ok(Ok(())) => None,
                _ => Some("Could not keep the sign-in".into()),
            })
        }
        Err(e) => back(Some(format!("{e:#}"))),
    }
}

// ---- Spotify (decision 0073) ----

/// The signed-in account, for Spotify: its lists are its own.
fn spotify_account(ctx: &Ctx) -> Result<String, ApiError> {
    ctx.serve()?;
    Ok(ctx.account().ok_or_else(|| ToggleError::Refused("Sign in first: Spotify is an account's own.".into()))?)
}

/// Blocking Spotify work for the account, its failure one short line.
async fn spotify_blocking<T: Send + 'static>(ctx: &Ctx, work: impl FnOnce(String) -> anyhow::Result<T> + Send + 'static) -> Result<T, ApiError> {
    let account = spotify_account(ctx)?;
    tokio::task::spawn_blocking(move || work(account))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")).into())
}

/// The account's Spotify: its app, the live connection, the export, which lists show and how many.
async fn spotify_state(ctx: Ctx) -> Result<Json<crate::app::spotify::State>, ApiError> {
    Ok(Json(spotify_blocking(&ctx, |a| crate::app::spotify::state(&a)).await?))
}

#[derive(Deserialize)]
struct SpotifyClient {
    #[serde(default)]
    client_id: String,
}

/// The account's own Spotify app (its Client ID); empty removes it, and a new one disconnects.
async fn spotify_client(ctx: Ctx, Json(b): Json<SpotifyClient>) -> Result<Json<crate::app::spotify::State>, ApiError> {
    Ok(Json(spotify_blocking(&ctx, move |a| crate::app::spotify::set_client(&a, &b.client_id).and_then(|()| crate::app::spotify::state(&a))).await?))
}

#[derive(Deserialize)]
struct SpotifyConnect {
    /// Where Spotify comes back to: the page's own address's `/api/spotify/callback`, as listed on the app.
    callback: String,
    /// Begun in the Android app: the way back ends in the app.
    #[serde(default)]
    app: bool,
}

/// Where to send the browser to sign in to Spotify with the account's own app.
async fn spotify_connect(ctx: Ctx, Json(b): Json<SpotifyConnect>) -> Result<Json<serde_json::Value>, ApiError> {
    let url = spotify_blocking(&ctx, move |a| crate::app::spotify::connect(&a, &b.callback, b.app)).await?;
    Ok(Json(json!({ "url": url })))
}

#[derive(Deserialize)]
struct SpotifyBack {
    state: String,
    code: Option<String>,
    error: Option<String>,
}

/// Spotify's way back from a sign-in: kept for the account that began it, then the browser back to its Spotify page
/// (the app's by its own link), with what went wrong.
async fn spotify_callback(Query(q): Query<SpotifyBack>) -> Response {
    let (app, why) = crate::app::spotify::come_back(&q.state, q.code.as_deref(), q.error.as_deref()).await;
    let why = why.map(|w| format!("spotify_error={}", urlencoding_lite(&w)));
    if app {
        let to = format!("cratarr://connected/spotify{}", why.map(|w| format!("?{w}")).unwrap_or_default());
        let page = format!(
            "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width\"><meta http-equiv=refresh content=\"0;url={to}\"><title>Cratarr</title><p style=\"font:16px system-ui;margin:2em\"><a href=\"{to}\">Back to Cratarr</a>"
        );
        return ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], page).into_response();
    }
    let to = format!("/#/settings?view=settings%3Aaccount%2Fspotify{}", why.map(|w| format!("&{w}")).unwrap_or_default());
    axum::response::Redirect::to(&to).into_response()
}

/// Disconnects the account's Spotify (the export's lists stay).
async fn spotify_disconnect(ctx: Ctx) -> Result<Json<crate::app::spotify::State>, ApiError> {
    Ok(Json(spotify_blocking(&ctx, |a| crate::app::spotify::disconnect(&a).and_then(|()| crate::app::spotify::state(&a))).await?))
}

/// Reads the account's live lists again now; answers at once.
async fn spotify_sync(ctx: Ctx) -> Result<Json<crate::app::spotify::State>, ApiError> {
    let account = spotify_account(&ctx)?;
    tokio::spawn(crate::app::spotify::read_live(account.clone()));
    // The read marks itself running at once; a moment for it to.
    tokio::task::yield_now().await;
    Ok(Json(spotify_blocking(&ctx, |a| crate::app::spotify::state(&a)).await?))
}

/// Forgets the lists the account's Spotify export brought (its listens stay).
async fn spotify_forget_import(ctx: Ctx) -> Result<Json<crate::app::spotify::State>, ApiError> {
    Ok(Json(spotify_blocking(&ctx, |a| crate::app::spotify::forget_export(&a).and_then(|_| crate::app::spotify::state(&a))).await?))
}

#[derive(Deserialize, Default)]
struct SpotifyFilter {
    status: Option<String>,
    q: Option<String>,
}

async fn spotify_albums(ctx: Ctx, Query(f): Query<SpotifyFilter>) -> Result<Json<serde_json::Value>, ApiError> {
    let albums = spotify_blocking(&ctx, move |a| crate::app::spotify::albums(&a, f.status.as_deref(), f.q.as_deref())).await?;
    Ok(Json(json!({ "albums": albums })))
}

async fn spotify_tracks(ctx: Ctx, Query(f): Query<SpotifyFilter>) -> Result<Json<serde_json::Value>, ApiError> {
    let tracks = spotify_blocking(&ctx, move |a| crate::app::spotify::tracks(&a, f.status.as_deref(), f.q.as_deref())).await?;
    Ok(Json(json!({ "tracks": tracks })))
}

async fn spotify_playlists(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let playlists = spotify_blocking(&ctx, |a| crate::app::spotify::playlists(&a)).await?;
    Ok(Json(json!({ "playlists": playlists })))
}

async fn spotify_playlist(ctx: Ctx, Path(id): Path<String>) -> Result<Response, ApiError> {
    match spotify_blocking(&ctx, move |a| crate::app::spotify::playlist(&a, &id)).await? {
        Some(p) => Ok(Json(p).into_response()),
        None => Ok((StatusCode::NOT_FOUND, Json(json!({ "error": "That playlist is not in your Spotify lists." }))).into_response()),
    }
}

async fn spotify_artists(ctx: Ctx, Query(f): Query<SpotifyFilter>) -> Result<Json<serde_json::Value>, ApiError> {
    let artists = spotify_blocking(&ctx, move |a| crate::app::spotify::artists(&a, f.status.as_deref())).await?;
    Ok(Json(json!({ "artists": artists })))
}

/// An item's picture: Spotify's (or Cover Art Archive's) relayed, or Cratarr's own cover of the album it matched.
async fn spotify_cover(ctx: Ctx, Path((kind, id)): Path<(String, String)>) -> Result<Response, ApiError> {
    use crate::app::reach::Sent;
    use crate::app::spotify::Picture;
    let picture = spotify_blocking(&ctx, move |a| crate::app::spotify::picture(&a, &kind, &id)).await?;
    let url = match picture {
        None => return Ok(StatusCode::NOT_FOUND.into_response()),
        Some(Picture::Own(source, album)) => {
            let to = format!("/api/cover/{}/{}", urlencoding_lite(&source), urlencoding_lite(&album));
            return Ok(axum::response::Redirect::to(&to).into_response());
        }
        Some(Picture::Remote(url)) if crate::app::spotify::relayable(&url) => url,
        Some(Picture::Remote(_)) => return Ok(StatusCode::NOT_FOUND.into_response()),
    };
    let http = crate::app::spotify::web::http().map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    let Ok(r) = http.get(&url).sent().await else { return Ok(StatusCode::BAD_GATEWAY.into_response()) };
    if !r.status().is_success() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let kind = r.headers().get(header::CONTENT_TYPE).cloned().unwrap_or(HeaderValue::from_static("image/jpeg"));
    let Ok(bytes) = r.bytes().await else { return Ok(StatusCode::BAD_GATEWAY.into_response()) };
    Ok(([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=86400"))], bytes).into_response())
}

/// A short text safe in a page address.
fn urlencoding_lite(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

#[derive(Deserialize)]
struct SwitchedOn {
    on: bool,
}

/// A listening service switched on or off, its sign-in kept.
async fn scrobble_on(ctx: Ctx, Path(target): Path<String>, Json(b): Json<SwitchedOn>) -> Result<StatusCode, ApiError> {
    let (serve, account) = scrobbler(&ctx)?;
    let found = tokio::task::spawn_blocking(move || serve.userdata.set_scrobble_on(&account, &target, b.on))
        .await
        .map_err(scrobble_refused)?
        .map_err(|e| scrobble_refused(format!("{e:#}")))?;
    Ok(if found { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND })
}

/// A listening service disconnected, with what waited for it.
async fn scrobble_forget(ctx: Ctx, Path(target): Path<String>) -> Result<StatusCode, ApiError> {
    let (serve, account) = scrobbler(&ctx)?;
    tokio::task::spawn_blocking(move || serve.userdata.forget_scrobble_target(&account, &target)).await.map_err(scrobble_refused)?.map_err(|e| scrobble_refused(format!("{e:#}")))?;
    Ok(StatusCode::NO_CONTENT)
}

/// A Last.fm import as asked: whose scrobbles, and the API key made at Last.fm (else the one kept last time).
#[derive(Deserialize)]
struct LastFmImport {
    user: String,
    #[serde(default)]
    key: Option<String>,
    /// What comes in (0.9.2): the scrobbles, the loves, and whether un-loved tracks stop being favourites (not unless
    /// asked).
    #[serde(default)]
    what: crate::app::lastfm::Choice,
}

/// What a Last.fm import keeps for next time, per Cratarr account: the Last.fm user and the API key (decision 0067).
#[derive(serde::Serialize, Deserialize)]
struct LastFmKept {
    user: String,
    key: String,
}

fn lastfm_choice(account: &str) -> String {
    format!("lastfm:{account}")
}

/// Whose import, whose scrobbles and how Cratarr asks: with the account's own Last.fm sign-in (Connections) where it
/// has one, which also reads a private history; else with the key given (kept for next time), else the one kept.
async fn lastfm_who(ctx: &Ctx, b: LastFmImport) -> Result<(Arc<crate::serve::Serve>, String, String, crate::app::lastfm::Auth), ApiError> {
    let serve = ctx.serve()?;
    let account = ctx.account().ok_or_else(|| ToggleError::Refused("Sign in first: the history is an account's own.".into()))?;
    let user = b.user.trim().to_string();
    if user.is_empty() {
        return Err(ToggleError::Refused("Say whose Last.fm scrobbles.".into()).into());
    }
    let (ud, choice) = (serve.userdata.clone(), lastfm_choice(&account));
    let given = b.key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
    let key = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<String>> {
        let kept = ud.import_choice(&choice)?.and_then(|c| serde_json::from_str::<LastFmKept>(&c).ok()).map(|k| k.key);
        let key = given.or(kept);
        if let Some(k) = &key {
            ud.set_import_choice(&choice, &serde_json::to_string(&LastFmKept { user: user.clone(), key: k.clone() })?)?;
        }
        Ok(key)
    })
    .await
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    let (ud, a) = (serve.userdata.clone(), account.clone());
    let signed = tokio::task::spawn_blocking(move || ud.scrobble_secret(&a, crate::app::scrobble::LASTFM))
        .await
        .ok()
        .and_then(Result::ok)
        .flatten()
        .and_then(|s| serde_json::from_str::<crate::app::scrobble::Secret>(&s).ok())
        .and_then(|s| Some((s.api_key?, s.session?, s.api_secret?)));
    let auth = match (signed, key) {
        (Some((key, session, secret)), _) => crate::app::lastfm::Auth { key, signed: Some((session, secret)) },
        (None, Some(key)) => crate::app::lastfm::Auth { key, signed: None },
        (None, None) => {
            return Err(ToggleError::Refused("Last.fm needs an API key: make one at last.fm/api/account/create, or connect Last.fm under Connections.".into()).into());
        }
    };
    Ok((serve, account, b.user.trim().to_string(), auth))
}

/// The Last.fm user an account imported last, and whether a key is kept (never the key).
async fn lastfm_kept(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let serve = ctx.serve()?;
    let account = ctx.account().ok_or_else(|| ToggleError::Refused("Sign in first: the history is an account's own.".into()))?;
    let ud = serve.userdata.clone();
    let kept = tokio::task::spawn_blocking(move || ud.import_choice(&lastfm_choice(&account)))
        .await
        .ok()
        .and_then(Result::ok)
        .flatten()
        .and_then(|c| serde_json::from_str::<LastFmKept>(&c).ok());
    Ok(Json(json!({ "user": kept.as_ref().map(|k| k.user.clone()), "key": kept.is_some() })))
}

async fn import_lastfm(ctx: Ctx, Json(b): Json<LastFmImport>) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    bring_lastfm(&ctx, b, false).await
}

async fn preview_lastfm(ctx: Ctx, Json(b): Json<LastFmImport>) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    bring_lastfm(&ctx, b, true).await
}

async fn bring_lastfm(ctx: &Ctx, b: LastFmImport, preview: bool) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let what = b.what;
    if !what.scrobbles && !what.loves {
        return Err(ToggleError::Refused("Choose the scrobbles, the loved tracks, or both.".into()).into());
    }
    // The scrobbles are listening; the loved tracks are favourites, and come in either way.
    if what.scrobbles && !preview {
        keeps_history(ctx)?;
    }
    let (serve, account, user, auth) = lastfm_who(ctx, b).await?;
    started(crate::app::tasks::spawn("import:lastfm", &account.clone(), preview, move |h| async move {
        crate::app::lastfm::bring(&serve, &account, &user, &auth, what, preview, &progress_of(h)).await
    }))
}

/// Everything imported from Last.fm out of the account again: its scrobbles, the stars its loved tracks set.
async fn undo_lastfm(ctx: Ctx, Json(b): Json<LastFmImport>) -> Result<Json<crate::app::lastfm::Undone>, ApiError> {
    let serve = ctx.serve()?;
    let account = ctx.account().ok_or_else(|| ToggleError::Refused("Sign in first: the history is an account's own.".into()))?;
    let user = b.user.trim().to_string();
    let undone = tokio::task::spawn_blocking(move || crate::app::lastfm::undo(&serve, &account, &user))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(undone))
}

#[derive(Deserialize)]
struct FileImport {
    /// The file's name, which says what kind it is.
    name: String,
    #[serde(default)]
    preview: bool,
}

/// A file into the asking account (decision 0067, step 4): its listens into the history, or a playlist of it; the file
/// is the body, kept nowhere.
async fn import_file(ctx: Ctx, Query(q): Query<FileImport>, body: axum::body::Bytes) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let app = ctx.app.clone();
    let serve = ctx.serve()?;
    let account = ctx.account().ok_or_else(|| ToggleError::Refused("Sign in first: an import is an account's own.".into()))?;
    started(crate::app::tasks::spawn("import:file", &account.clone(), q.preview, move |h| async move {
        crate::app::import_files::import(&app, &serve, &account, &q.name, &body, q.preview, &progress_of(h)).await
    }))
}

#[derive(Deserialize)]
struct FileUndo {
    /// `listenbrainz`, `spotify` or `lastfm`: the listens imported from such files.
    from: String,
}

/// The listens imported from files of one kind out of the account's history again.
async fn undo_file(ctx: Ctx, Query(q): Query<FileUndo>) -> Result<Json<serde_json::Value>, ApiError> {
    let serve = ctx.serve()?;
    let account = ctx.account().ok_or_else(|| ToggleError::Refused("Sign in first: an import is an account's own.".into()))?;
    let context = crate::app::import_files::context(&q.from);
    let removed = tokio::task::spawn_blocking(move || serve.userdata.forget_heard(&account, &context))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "removed": removed })))
}

/// Every listen imported from ListenBrainz out of the account's history again.
async fn undo_listenbrainz(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let serve = ctx.serve()?;
    let account = ctx.account().ok_or_else(|| ToggleError::Refused("Sign in first: the history is an account's own.".into()))?;
    let removed = tokio::task::spawn_blocking(move || crate::app::listenbrainz::undo(&serve, &account))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "removed": removed })))
}

#[derive(Deserialize)]
struct PicksQuery {
    name: String,
    without: Option<String>,
}

/// How many Top recommendations an artist's page shows.
const PICKS: usize = 5;

/// An artist's Top recommendations: their tracks that fit the asking account's taste best (song ids, best first).
async fn artist_picks(State(app): State<Arc<App>>, Query(q): Query<PicksQuery>) -> Result<Json<serde_json::Value>, ApiError> {
    let without: Vec<String> = q.without.as_deref().unwrap_or("").split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
    Ok(Json(json!({ "ids": app.playlists.artist_picks(&q.name, &without, PICKS).await? })))
}

/// The weather as Home Assistant tells it (the cratarr-ha integration, on each change): Home's moment.
async fn home_weather(ctx: Ctx, Json(w): Json<crate::app::moment::Weather>) -> Result<Json<serde_json::Value>, ApiError> {
    ctx.serve()?;
    ctx.account().ok_or_else(|| ToggleError::Refused("Sign in to tell the weather.".into()))?;
    crate::app::moment::tell(w, crate::app::moment::now_secs());
    Ok(Json(json!({})))
}

#[derive(Deserialize)]
struct MomentQuery {
    source: Option<String>,
    without: Option<String>,
    /// The page's clock: seconds east of UTC.
    tz: Option<i64>,
}

/// Home's moment: its name and a few albums near it, of the libraries switched on; its radio plays `like:now`.
async fn home_moment(State(app): State<Arc<App>>, Query(q): Query<MomentQuery>) -> Result<Json<crate::app::MomentView>, ApiError> {
    let without: Vec<String> = q.without.as_deref().unwrap_or("").split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
    Ok(Json(app.playlists.moment(q.source.as_deref(), &without, q.tz).await?))
}

/// The asking account's taste and what it avoids (Insights › Listening › Profile).
#[derive(serde::Deserialize)]
struct TasteQuery {
    source: Option<String>,
    /// The libraries switched off (`source/library`, comma-separated): Insights' switches.
    without: Option<String>,
    /// Insights' period (week, month, year, all; absent: all): the plays and skips within it.
    period: Option<crate::app::history::Period>,
    /// Sources switched off, comma-separated: favourites, playlists, plays, skipped, excluded (absent: all on).
    off: Option<String>,
}

#[derive(Deserialize)]
struct HeardAlbums {
    ids: Vec<String>,
}

/// Home's New for you (`app::home`).
async fn home_new(State(app): State<Arc<App>>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "albums": app.home_new().await? })))
}

#[derive(Deserialize)]
struct HomeBack {
    /// The asking page's player: its queues left.
    player: Option<String>,
}

/// Home's Jump back in (`app::home`).
async fn home_back(State(app): State<Arc<App>>, Query(q): Query<HomeBack>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "items": app.home_back(q.player.as_deref()).await? })))
}

/// Home's Sounds like your week (`app::home`), of the libraries switched on.
async fn home_week(State(app): State<Arc<App>>, Query(q): Query<TasteQuery>) -> Result<Json<serde_json::Value>, ApiError> {
    let without: Vec<String> = q.without.as_deref().unwrap_or("").split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
    Ok(Json(json!({ "week": app.home_week(q.source.as_deref(), without).await? })))
}

/// Of these albums, those the asking account heard 5% or more of (Home's New for you leaves them out).
async fn home_heard(State(app): State<Arc<App>>, Json(b): Json<HeardAlbums>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "heard": app.playlists.heard_albums(b.ids).await? })))
}

/// Home's Blast from the past (favourite tracks unplayed longest) and Grave digging (the albums most like the taste,
/// unplayed longest), of the libraries switched on.
async fn home_digging(State(app): State<Arc<App>>, Query(q): Query<TasteQuery>) -> Result<Json<crate::app::Digging>, ApiError> {
    let without: Vec<String> = q.without.as_deref().unwrap_or("").split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
    Ok(Json(app.playlists.digging(q.source.as_deref(), &without).await?))
}

/// How many artists Home's Artists to explore shows: the open one and the others.
const EXPLORE: usize = 7;
/// Of the artists to explore, how many not read yet are read on one visit while too few have a biography.
const READ_NOW: usize = 3;

/// Home's Artists to explore (the owner, 2026-10-23): artists of the libraries switched on with two albums or more, in a
/// new random order each time, only those TheAudioDB has a biography of (the owner, 2026-10-24); the first with a photo
/// open, each with its four biggest albums.
async fn home_artists(ctx: Ctx, Query(q): Query<TasteQuery>) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    let served = ctx.serve()?;
    let without: Vec<String> = q.without.as_deref().unwrap_or("").split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
    let on = app.insights.folders_on(q.source.as_deref(), &without);
    let lib = served.library.clone();
    let rows = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        // Home shows no album without a cover (the owner, 2026-10-24).
        let coverless = lib.coverless()?;
        Ok(lib.artist_albums()?.into_iter().filter(|r| !coverless.contains(&r.3)).collect::<Vec<_>>())
    })
    .await
    .map_err(anyhow::Error::from)
    .and_then(|r| r)
    .map_err(ToggleError::from)?;
    // Per artist, their albums in the libraries switched on, the biggest first.
    let mut by: std::collections::BTreeMap<String, (String, Option<String>, Vec<(i64, String)>)> = Default::default();
    for (artist, name, mbid, album, folder, tracks) in rows {
        if crate::crates::various(&name) || on.as_ref().is_some_and(|on| !on.contains(&folder)) {
            continue;
        }
        by.entry(artist).or_insert_with(|| (name, mbid, Vec::new())).2.push((tracks, album));
    }
    let mut pool: Vec<(String, String, Option<String>, Vec<String>)> = by
        .into_iter()
        .filter(|(_, (_, _, albums))| albums.len() >= 2)
        .map(|(id, (name, mbid, mut albums))| {
            albums.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            (id, name, mbid, albums.into_iter().take(4).map(|(_, a)| a).collect())
        })
        .collect();
    // A new order each time (a plain shuffle from the clock: nothing here needs to be unguessable).
    let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1;
    for i in (1..pool.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        pool.swap(i, (seed % (i as u64 + 1)) as usize);
    }
    // Only artists with a biography (the owner, 2026-10-24): those read with one first, in the shuffled order; while
    // too few, a few not read yet are read now (a handful a visit, so Home stays quick; the rest on later visits).
    let Some(store) = app.artists.clone() else { return Ok(Json(json!({ "artists": [], "credit": "TheAudioDB" }))) };
    let mut chosen: Vec<((String, String, Option<String>, Vec<String>), crate::app::artists::Info)> = Vec::new();
    let mut unread = Vec::new();
    for item in pool {
        if chosen.len() >= EXPLORE {
            break;
        }
        match store.get(&crate::app::artists::key(&item.1, item.2.as_deref())) {
            Some(info) if info.bio.is_some() => chosen.push((item, info)),
            Some(_) => {}
            None => unread.push(item),
        }
    }
    for item in unread.into_iter().take(READ_NOW) {
        if chosen.len() >= EXPLORE {
            break;
        }
        if let Some(info) = store.get_or_read(&item.1, item.2.as_deref()).await.filter(|i| i.bio.is_some()) {
            chosen.push((item, info));
        }
    }
    // The open one: the first with a photo, else the first.
    if let Some(i) = chosen.iter().position(|(_, info)| info.photo.is_some()) {
        chosen.swap(0, i);
    }
    let artists: Vec<serde_json::Value> = chosen
        .iter()
        .map(|((id, name, _, albums), info)| {
            json!({
                "id": id,
                "name": name,
                "albums": albums,
                "bio": info.bio.clone(),
                "photo": info.photo.is_some(),
            })
        })
        .collect();
    Ok(Json(json!({ "artists": artists, "credit": "TheAudioDB" })))
}

#[derive(Deserialize)]
struct FindQuery {
    #[serde(default)]
    q: String,
    /// A missing playlist line: found again by what it says (`app::refind`), not by words.
    line: Option<String>,
}

/// How many tracks a search of the library for a missing line weighs, and shows.
const REFIND_WEIGH: usize = 60;
const REFIND_SHOWN: usize = 10;

/// Tracks of Cratarr's own library for a playlist's missing line (Health's missing tracks, to point the line at one:
/// the owner, 2026-10-23), each with its playlist line. By the line (`line`; the owner, 2026-10-24: the words were too
/// strict): the same title at heart by the same artist, wherever it lies now, the best first, `sure` on the one
/// answer when there is one; else by words (`q`), at most 20, the library's search's order.
async fn library_find(ctx: Ctx, Query(q): Query<FindQuery>) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    let served = ctx.serve()?;
    let (lib, cat, query, line) = (served.library.clone(), app.library.catalogue(), q.q.clone(), q.line.clone());
    let found = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<serde_json::Value>> {
        let songs = match &line {
            Some(line) => {
                use crate::app::refind;
                let said = refind::said(line);
                let title = refind::core(&said.title);
                // By artist and title, then by the title alone (a track now on a compilation, its artist credited
                // otherwise there), each once.
                let mut seen = std::collections::HashSet::new();
                let mut scored = Vec::new();
                for words in [format!("{} {title}", refind::normal(&said.artist)), title.clone()] {
                    let (_, _, songs) = lib.search(words.trim(), (0, 0), (0, 0), (REFIND_WEIGH, 0), None)?;
                    for s in songs {
                        if seen.insert(s.id.clone())
                            && let Some(points) = refind::score(&said, &s.title, &s.artist, &s.album)
                        {
                            scored.push((points, s));
                        }
                    }
                }
                scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.title.cmp(&b.1.title)));
                scored.truncate(REFIND_SHOWN);
                scored
            }
            None => lib.search(query.trim(), (0, 0), (0, 0), (20, 0), None)?.2.into_iter().map(|s| (0, s)).collect(),
        };
        // The one answer: the best, alone at its points, with the same title and artist.
        let sure = line.is_some() && songs.first().is_some_and(|(p, _)| *p >= 7 && songs.get(1).is_none_or(|(q, _)| q < p));
        let songs: Vec<_> = songs.into_iter().map(|(_, s)| s).collect();
        let ids: Vec<String> = songs.iter().map(|s| s.id.clone()).collect();
        let places = lib.song_places(&ids)?;
        Ok(songs
            .into_iter()
            .filter_map(|s| {
                let (folder, rel) = places.get(&s.id)?;
                let line = cat.line_of(*folder, rel)?;
                Some(json!({ "id": s.id, "title": s.title, "artist": s.artist, "album": s.album, "album_id": s.album_id, "line": line }))
            })
            .enumerate()
            .map(|(i, mut t)| {
                t["sure"] = json!(sure && i == 0);
                t
            })
            .collect())
    })
    .await
    .map_err(anyhow::Error::from)
    .and_then(|r| r)
    .map_err(ToggleError::from)?;
    Ok(Json(json!(found)))
}

#[derive(Deserialize)]
struct AboutQuery {
    name: String,
    /// The artist's MusicBrainz id, or a release group of theirs, where known (0.9.6: an artist not in the library).
    #[serde(default)]
    mbid: Option<String>,
    #[serde(default)]
    release_group: Option<String>,
}

/// About an artist, for their page (the owner, 2026-10-23): TheAudioDB's description and whether there is a photo
/// (`/api/artists/{id}/photo`), read now when it never was. Nothing known: nulls.
async fn artist_about(ctx: Ctx, Query(q): Query<AboutQuery>) -> Json<serde_json::Value> {
    let app = ctx.app.clone();
    let found = async {
        let served = ctx.served.clone()?;
        let store = app.artists.clone()?;
        let lib = served.library.clone();
        let name = q.name.clone();
        let (id, mbid) = tokio::task::spawn_blocking(move || lib.artist_by_name(&name)).await.ok()?.ok()??;
        let info = store.get_or_read(&q.name, mbid.as_deref()).await?;
        Some(json!({ "id": id, "bio": info.bio, "photo": info.photo.is_some(), "photo_credit": info.credit }))
    }
    .await;
    Json(found.unwrap_or_else(|| json!({ "id": null, "bio": null, "photo": false })))
}

#[derive(Deserialize)]
struct SearchQuery {
    #[serde(default)]
    q: String,
    /// Which kinds, comma-separated (album, artist, track, genre, playlist, crate; work, 0.12; missing, requested,
    /// spotify, suggested, excluded); all when none.
    #[serde(default)]
    kinds: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    /// Show all (0076): a page of one kind from here.
    #[serde(default)]
    offset: Option<usize>,
}

/// One search, answered by kind and merged (decision 0072): `{ q, groups: { kind: { total, items } }, top }`.
async fn search_all(State(app): State<Arc<App>>, Query(s): Query<SearchQuery>) -> Result<Json<crate::app::search::Answer>, ApiError> {
    let kinds: Vec<String> = s.kinds.unwrap_or_default().split(',').map(str::trim).filter(|k| !k.is_empty()).map(str::to_string).collect();
    let answer = crate::app::search::search(&app, &s.q, &kinds, s.limit.unwrap_or(crate::app::search::LIMIT), s.offset.unwrap_or(0))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(answer))
}

/// The library's genres with the other names they go by (0.12: one matcher, genre aliases, the hand-kept list), those
/// with none left out: `{"<genre as in the library>": ["alias", …]}`, each genre's `aliases` for the page's matcher.
async fn genre_aliases(State(app): State<Arc<App>>) -> Result<Json<std::collections::BTreeMap<String, Vec<String>>>, ApiError> {
    Ok(Json(crate::app::search::genre_aliases(&app).await.map_err(ToggleError::from)?))
}

/// Beyond the library (0076, the unindexed universe): MusicBrainz's albums and Discogs' masters for the words, together,
/// one album once (by its artist and title), each with where it was found; what the library has marked `have`, what
/// is asked for its request. MusicBrainz's artists beside. A page of `limit` from `offset`; kept per query.
async fn search_beyond(ctx: Ctx, Query(s): Query<SearchQuery>) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    let serve = ctx.serve()?;
    let (limit, offset) = (s.limit.unwrap_or(20).clamp(1, 50), s.offset.unwrap_or(0).min(500));
    let q = s.q.trim().to_string();
    if crate::search::words(&q).is_empty() {
        return Ok(Json(json!({ "q": q, "albums": [], "artists": [] })));
    }
    let (mb, discogs) = tokio::join!(
        crate::app::discover::musicbrainz_search(&serve, app.library.catalogue(), &q, (offset + limit).min(100)),
        crate::app::discogs::search_masters(&q, limit, offset)
    );
    let (mb_albums, artists) = mb.unwrap_or_default();
    let discogs = discogs.unwrap_or_default();
    let library = tokio::task::spawn_blocking(crate::app::spotify::library).await.ok().and_then(Result::ok);
    let marks = request_marks(&app, &serve).await;
    let key = |artist: &str, title: &str| format!("{}|{}", crate::app::import::key(artist), crate::app::import::key(title));
    let mut order: Vec<String> = Vec::new();
    let mut by: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
    for a in mb_albums.into_iter().skip(offset).take(limit) {
        let k = key(&a.artist, &a.title);
        if by.contains_key(&k) {
            continue;
        }
        order.push(k.clone());
        by.insert(
            k,
            json!({ "title": a.title, "artist": a.artist, "year": a.year, "cover": a.cover, "release_group": a.release_group, "discogs": null,
                "discogs_url": null, "sources": ["musicbrainz"], "have": a.have, "request": marks.as_deref().and_then(|m| crate::app::requests::mark_of(m, Some(&a.release_group), &a.artist, &a.title)) }),
        );
    }
    for d in discogs {
        let k = key(&d.artist, &d.title);
        if let Some(found) = by.get_mut(&k) {
            found["sources"] = json!(["musicbrainz", "discogs"]);
            found["discogs"] = json!(d.id);
            found["discogs_url"] = json!(d.url);
            continue;
        }
        let have = library.as_ref().is_some_and(|l| l.has_album(&d.title, &d.artist));
        order.push(k.clone());
        by.insert(
            k,
            json!({ "title": d.title, "artist": d.artist, "year": d.year, "cover": d.cover, "release_group": null, "discogs": d.id,
                "discogs_url": d.url, "sources": ["discogs"], "have": have, "request": marks.as_deref().and_then(|m| crate::app::requests::mark_of(m, None, &d.artist, &d.title)) }),
        );
    }
    let albums: Vec<serde_json::Value> = order.into_iter().filter_map(|k| by.remove(&k)).collect();
    Ok(Json(json!({ "q": q, "albums": albums, "artists": if offset == 0 { json!(artists) } else { json!([]) } })))
}

/// Related artists to explore on an artist's page (0.9.2, app::related), by the artist's name: as last looked up, and
/// whether a look runs now (started here when none was kept or it is old). An artist without a MusicBrainz id has none.
async fn artist_related(ctx: Ctx, Query(q): Query<AboutQuery>) -> Result<Json<serde_json::Value>, ApiError> {
    let serve = ctx.serve()?;
    let (s, name) = (serve.clone(), q.name.trim().to_string());
    let found = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<(String, Option<crate::app::related::Found>, bool)>> {
        let Some((_, Some(mbid))) = s.library.artist_by_name(&name)? else { return Ok(None) };
        let (kept, due) = crate::app::related::kept(&s, &mbid)?;
        Ok(Some((mbid, kept, due)))
    })
    .await
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    let Some((mbid, kept, due)) = found else { return Ok(Json(json!({ "artists": [], "looking": false }))) };
    if due {
        crate::app::related::start(serve, q.name.trim().to_string(), mbid.clone());
    }
    let artists = kept.map(|f| f.artists).unwrap_or_default();
    Ok(Json(json!({ "artists": artists, "looking": crate::app::related::is_looking(&mbid) })))
}

/// An artist new to the library, for Discover's New to you (0069 amended, 0.9.1): TheAudioDB's description by name, as
/// the artist store reads it for the library's (read now when it never was), and whether there is a photo.
/// A name's MusicBrainz id where it is one of the library's artists: the artist store keeps those by it, as the
/// artist pages read them (the owner, 2026-10-25: the masonry's artist, one the library has, showed no photo).
async fn library_mbid(app: &App, name: &str) -> Option<String> {
    let lib = app.served.as_ref()?.library.clone();
    let name = name.to_string();
    tokio::task::spawn_blocking(move || lib.artist_by_name(&name)).await.ok()?.ok()??.1.filter(|m| !m.is_empty())
}

async fn discover_artist(State(app): State<Arc<App>>, Query(q): Query<AboutQuery>) -> Json<serde_json::Value> {
    let found = async {
        let store = app.artists.clone()?;
        let mbid = library_mbid(&app, q.name.trim()).await;
        let info = store.get_or_read(q.name.trim(), mbid.as_deref()).await?;
        Some(json!({ "bio": info.bio, "photo": info.photo.is_some(), "photo_credit": info.credit }))
    }
    .await;
    Json(found.unwrap_or_else(|| json!({ "bio": null, "photo": false })))
}

/// How many full-length studio albums an artist new to the library has released (0.9.3): its own call, as the first
/// count takes MusicBrainz a few seconds and the page's photo and description must not wait for it.
async fn discover_artist_albums(Query(q): Query<AboutQuery>) -> Json<serde_json::Value> {
    Json(json!({ "albums": crate::app::discover::artist_albums(q.name.trim()).await }))
}

/// That artist's photo, as the artist store kept it.
async fn discover_artist_photo(State(app): State<Arc<App>>, Query(q): Query<AboutQuery>) -> Response {
    let found = async {
        let store = app.artists.clone()?;
        let mbid = library_mbid(&app, q.name.trim()).await;
        let file = store.get(&crate::app::artists::key(q.name.trim(), mbid.as_deref()))?.photo?;
        tokio::fs::read(store.photo_path(&file)).await.ok()
    }
    .await;
    match found {
        Some(bytes) => ([(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg")), (header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=86400"))], bytes).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "no photo" }))).into_response(),
    }
}

/// An artist's photo, as the artist store kept it.
async fn artist_photo(ctx: Ctx, Path(id): Path<String>) -> Response {
    let app = ctx.app.clone();
    let found = async {
        let served = ctx.served.clone()?;
        let store = app.artists.clone()?;
        let lib = served.library.clone();
        let artist = tokio::task::spawn_blocking(move || lib.artist(&id)).await.ok()?.ok()??;
        let file = store.get(&crate::app::artists::key(&artist.name, artist.music_brainz_id.as_deref()))?.photo?;
        tokio::fs::read(store.photo_path(&file)).await.ok()
    }
    .await;
    match found {
        Some(bytes) => ([(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg")), (header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=86400"))], bytes).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "no photo" }))).into_response(),
    }
}

async fn taste_profile(State(app): State<Arc<App>>, Query(q): Query<TasteQuery>) -> Result<Json<crate::app::taste::TasteView>, ApiError> {
    let without: Vec<String> = q.without.as_deref().unwrap_or("").split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
    let off: Vec<String> = q.off.as_deref().unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
    Ok(Json(app.playlists.taste(q.source.as_deref(), &without, q.period.unwrap_or(crate::app::history::Period::All), &off).await?))
}

/// How many similar lists a list shows at its foot.
const SIMILAR_LISTS: usize = 12;

/// Albums like an album, of Cratarr's own library (the one similarity): `[{id, alike}]`, the closest first.
async fn similar_albums(Path((source, album)): Path<(String, String)>) -> Result<Json<serde_json::Value>, ApiError> {
    if source != "cratarr" {
        return Ok(Json(json!([])));
    }
    let found = crate::app::radio::similar_albums(&album, SIMILAR_LISTS).await?;
    Ok(Json(json!(found.into_iter().map(|(id, alike)| json!({ "id": id, "alike": alike })).collect::<Vec<_>>())))
}

/// Crates like a crate, among the account's own: `[{id, alike}]`, the closest first.
async fn similar_crates(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let found = crate::app::similar::crates_like(&app.playlists, &id, SIMILAR_LISTS).await?;
    Ok(Json(json!(found.into_iter().map(|(id, alike)| json!({ "id": id, "alike": alike })).collect::<Vec<_>>())))
}

/// Playlists like a playlist, among those the account sees: `[{id, alike}]`, the closest first.
async fn similar_playlists(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let found = app.playlists.similar_playlists(&id, SIMILAR_LISTS).await?;
    Ok(Json(json!(found.into_iter().map(|(id, alike)| json!({ "id": id, "alike": alike })).collect::<Vec<_>>())))
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
#[derive(Deserialize)]
struct ListeningQuery {
    period: Option<crate::app::history::Period>,
    /// The page's time zone: seconds east of UTC (its days begin at its midnight).
    #[serde(default)]
    tz: i64,
    /// The server followed, for the libraries switched on (`without` in the raw query).
    source: Option<String>,
}

/// Insights' Listening (decision 0065): the asking account's listening over a week, a month or a year, of the tracks in
/// the libraries switched on (`without`, Insights' switches).
async fn history_listening(ctx: Ctx, Query(q): Query<ListeningQuery>, RawQuery(raw): RawQuery) -> Result<Json<crate::app::history::Listening>, ApiError> {
    use crate::app::history::{self, Period, SongFacts};
    let app = ctx.app.clone();
    let served = ctx.serve()?;
    let user = ctx.account().or_else(|| served.users.owner()).ok_or_else(|| ToggleError::Refused("Nobody is signed in.".into()))?;
    let period = q.period.unwrap_or(Period::Week);
    let tz = q.tz.clamp(-14 * 3600, 14 * 3600);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    // Back far enough for its taste over time (further than the period), whatever the time zone.
    let since = period.reaches_back(now);
    let on = app.insights.folders_on(q.source.as_deref(), &without_of(raw.as_deref()));
    let listening = tokio::task::spawn_blocking(move || -> anyhow::Result<history::Listening> {
        let (mut rows, log_from) = history::heard(&served.userdata, &user, since)?;
        if let Some(on) = on {
            let mut ids: Vec<String> = rows.iter().map(|r| r.song.clone()).collect();
            ids.sort();
            ids.dedup();
            let folders = served.library.song_folders(&ids)?;
            rows.retain(|r| folders.get(&r.song).is_some_and(|f| on.contains(f)));
        }
        let mut ids: Vec<String> = rows.iter().map(|r| r.song.clone()).collect();
        ids.sort();
        ids.dedup();
        let facts = served
            .library
            .songs_by_id(&ids)?
            .into_iter()
            .map(|s| (s.id.clone(), SongFacts { title: s.title, artist: s.artist, album: s.album, album_id: s.album_id, genre: s.genre }))
            .collect();
        Ok(history::listening(&rows, &facts, period, now, tz, log_from))
    })
    .await
    .map_err(anyhow::Error::from)
    .and_then(|r| r)
    .map_err(ToggleError::from)?;
    Ok(Json(listening))
}

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
    Ok(Json(crate::app::play::play_playlist(&app, &id, b.mode, f.source.as_deref()).await?))
}

#[derive(Deserialize)]
struct PreviewBody {
    source: SourceConfig,
}

#[derive(Deserialize)]
struct LastfmKey {
    /// A new key (empty: taken away); absent: kept.
    #[serde(default)]
    key: Option<String>,
    /// Switched on or off (0.9.6), the key kept.
    #[serde(default)]
    enabled: Option<bool>,
}

/// The server's own Last.fm key (Discover's artists like each account's own most played, decision 0069 amended):
/// whether one is in use, and whether it was saved here (else `LASTFM_API_KEY` gives it). The key is never sent.
async fn get_lastfm_key(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let serve = ctx.serve()?;
    let ud = serve.userdata.clone();
    let (saved, used) = tokio::task::spawn_blocking(move || -> anyhow::Result<(bool, bool)> {
        let saved = ud.import_choice(crate::app::discover::SERVER_LASTFM)?.is_some_and(|k| !k.trim().is_empty());
        Ok((saved, crate::app::discover::server_lastfm_key(&ud)?.is_some()))
    })
    .await
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "set": used, "saved": saved, "enabled": crate::app::switches::is_on(crate::app::switches::LASTFM) })))
}

/// The server's own Last.fm key saved; an empty one takes it away.
async fn put_lastfm_key(ctx: Ctx, Json(b): Json<LastfmKey>) -> Result<Json<serde_json::Value>, ApiError> {
    let key = b.key.map(|k| k.trim().to_string());
    if key.as_deref().is_some_and(|k| !k.is_empty() && !crate::app::discover::lastfm_key_ok(k)) {
        return Err(ToggleError::Refused("A Last.fm API key is 32 letters and digits.".into()).into());
    }
    let serve = ctx.serve()?;
    let ud = serve.userdata.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        if let Some(key) = key {
            ud.set_import_choice(crate::app::discover::SERVER_LASTFM, &key)?;
        }
        if let Some(on) = b.enabled {
            crate::app::switches::set_on(crate::app::switches::LASTFM, on)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    get_lastfm_key(ctx).await
}

/// Whether the server's Discogs key and secret are saved (never them), and the limit a minute Discogs said last.
async fn get_discogs_keys(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let serve = ctx.serve()?;
    let ud = serve.userdata.clone();
    let set = tokio::task::spawn_blocking(move || crate::app::discogs::credentials(&ud))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .is_some();
    Ok(Json(json!({ "set": set, "per_minute": crate::app::discogs::per_minute(), "enabled": crate::app::switches::is_on(crate::app::switches::DISCOGS) })))
}

#[derive(Deserialize)]
struct DiscogsKeys {
    /// New ones (both empty: taken away); absent: kept.
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    secret: Option<String>,
    /// Switched on or off (0.9.6), the key and secret kept.
    #[serde(default)]
    enabled: Option<bool>,
}

/// The server's Discogs key and secret saved (0.9.5, the owner registers the app); both blank takes them away.
async fn put_discogs_keys(ctx: Ctx, Json(b): Json<DiscogsKeys>) -> Result<Json<serde_json::Value>, ApiError> {
    let keys = match (b.key, b.secret) {
        (None, None) => None,
        (key, secret) => Some((key.unwrap_or_default().trim().to_string(), secret.unwrap_or_default().trim().to_string())),
    };
    if let Some((key, secret)) = &keys
        && !(key.is_empty() && secret.is_empty())
        && !(crate::app::discogs::valid(key) && crate::app::discogs::valid(secret))
    {
        return Err(ToggleError::Refused("A Discogs key and secret are both letters and digits.".into()).into());
    }
    let serve = ctx.serve()?;
    let ud = serve.userdata.clone();
    let enabled = b.enabled;
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        if let Some((key, secret)) = keys {
            ud.set_import_choice(crate::app::discogs::KEY, &key)?;
            ud.set_import_choice(crate::app::discogs::SECRET, &secret)?;
        }
        if let Some(on) = enabled {
            crate::app::switches::set_on(crate::app::switches::DISCOGS, on)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    get_discogs_keys(ctx).await
}

#[derive(Deserialize)]
struct Shared {
    shared: bool,
}

/// A device shared with everyone on this Cratarr, or not (0.9.8): its owner's, or an administrator's, to set.
async fn player_shared(State(app): State<Arc<App>>, Path(name): Path<String>, Json(b): Json<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({ "shared": app.play_on.set_shared(&name, b.shared)? })))
}

// ---- Server settings that were the environment's only (0.9.8)

/// The proxies believed, the analysis pass, Beat this! and sync play's host, with what the environment set.
async fn get_runtime(ctx: Ctx) -> Result<Json<crate::app::runtime::View>, ApiError> {
    let serve = ctx.serve()?;
    let ud = serve.userdata.clone();
    Ok(Json(
        tokio::task::spawn_blocking(move || crate::app::runtime::view(&ud))
            .await
            .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
            .map_err(|e| ToggleError::Refused(format!("{e:#}")))?,
    ))
}

/// Saved and in force at once (sync play's host for the players asking from now on).
async fn put_runtime(ctx: Ctx, Json(c): Json<crate::app::runtime::Change>) -> Result<Json<crate::app::runtime::View>, ApiError> {
    let app = ctx.app.clone();
    let serve = ctx.serve()?;
    let ud = serve.userdata.clone();
    let host_changed = c.snapcast_host.is_some();
    let r = tokio::task::spawn_blocking(move || crate::app::runtime::save(&ud, &c))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(ToggleError::Refused)?;
    if host_changed {
        app.play_on.set_sync_host(r.snapcast_host.clone());
        app.play_on.set_lan_base(crate::app::runtime::lan_base(&r));
    }
    get_runtime(ctx).await
}

// ---- Discogs' facts (0.9.8)

/// An album's facts on Discogs: its master's genres, styles and year, its main release's labels and catalogue numbers
/// and its credits, its page; `{}` where Discogs has none. Asked now where not kept.
async fn album_discogs(ctx: Ctx, Path((source, album)): Path<(String, String)>) -> Result<Json<serde_json::Value>, ApiError> {
    own_album(&source)?;
    let served = ctx.serve()?;
    let facts = crate::app::discogs_facts::album(&served, &album).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(facts.map_or(json!({}), |f| serde_json::to_value(f).unwrap_or_default())))
}

#[derive(Deserialize)]
struct ArtistAsked {
    name: String,
    #[serde(default)]
    mbid: Option<String>,
}

/// An artist's facts on Discogs: profile (plain), real name, aliases, name variations, members and groups, links, page;
/// `{}` where Discogs has none.
async fn artist_discogs(ctx: Ctx, Query(q): Query<ArtistAsked>) -> Result<Json<serde_json::Value>, ApiError> {
    let served = ctx.serve()?;
    let facts = crate::app::discogs_facts::artist(&served, &q.name, q.mbid.as_deref()).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(facts.map_or(json!({}), |f| serde_json::to_value(f).unwrap_or_default())))
}

// ---- Single-track replacement (0.9.8)

/// The album's MusicBrainz release, each track with the file that holds it (none: missing).
async fn album_release(Path((source, album)): Path<(String, String)>) -> Result<Json<crate::app::track_fetch::ReleaseView>, ApiError> {
    own_album(&source)?;
    Ok(Json(crate::app::track_fetch::release(&album).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?))
}

#[derive(Deserialize)]
struct TrackRef {
    source: String,
    id: String,
}

#[derive(Deserialize)]
struct TrackSearch {
    album: TrackRef,
    recording: String,
    #[serde(default)]
    replace: Option<TrackRef>,
}

/// The album is Cratarr's own, and the track to replace on it.
fn own_track(b: &TrackSearch) -> Result<(), ApiError> {
    own_album(&b.album.source)?;
    if b.replace.as_ref().is_some_and(|r| r.source != b.album.source) {
        return Err(ToggleError::Refused("The track to replace is not on that album.".into()).into());
    }
    Ok(())
}

/// One slskd search for one of the release's tracks (never bulk).
async fn start_track_search(Json(b): Json<TrackSearch>) -> Result<Json<serde_json::Value>, ApiError> {
    own_track(&b)?;
    let id = crate::app::track_fetch::search(&b.album.id, &b.recording, b.replace.map(|r| r.id)).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "id": id })))
}

async fn track_search(Path(id): Path<String>) -> Result<Json<crate::app::track_fetch::SearchView>, ApiError> {
    Ok(Json(crate::app::track_fetch::search_view(&id).ok_or_else(|| ToggleError::Refused("That search is gone; search again.".into()))?))
}

#[derive(Deserialize)]
struct FetchOne {
    candidate: usize,
}

/// One of a search's files fetched and checked.
async fn start_track_fetch(Path(id): Path<String>, Json(b): Json<FetchOne>) -> Result<Json<serde_json::Value>, ApiError> {
    let fetch = crate::app::track_fetch::fetch(&id, b.candidate).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "id": fetch })))
}

async fn track_fetch(Path(id): Path<String>) -> Result<Json<crate::app::track_fetch::FetchView>, ApiError> {
    Ok(Json(crate::app::track_fetch::fetch_view(&id).ok_or_else(|| ToggleError::Refused("That download is gone.".into()))?))
}

async fn drop_track_fetch(Path(id): Path<String>) -> StatusCode {
    tokio::task::spawn_blocking(move || crate::app::track_fetch::drop_fetch(&id)).await.ok();
    StatusCode::NO_CONTENT
}

#[derive(Deserialize)]
struct FilesAsked {
    #[serde(default)]
    q: Option<String>,
}

/// The FLAC and MP3 files in Cratarr's slskd folder to take a track from (0.9.19); with `q`, best match first.
async fn track_files(Query(a): Query<FilesAsked>) -> Result<Json<Vec<crate::app::track_fetch::TrackFile>>, ApiError> {
    Ok(Json(crate::app::track_fetch::files(a.q).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?))
}

#[derive(Deserialize)]
struct FileFetch {
    /// Relative to Cratarr's slskd folder, as listed.
    path: String,
    #[serde(flatten)]
    track: TrackSearch,
}

/// One file already on the server taken as a fetch is: copied (the original stays) and checked, then Add or drop.
async fn start_file_fetch(Json(b): Json<FileFetch>) -> Result<Json<serde_json::Value>, ApiError> {
    own_track(&b.track)?;
    let t = b.track;
    let fetch = crate::app::track_fetch::fetch_file(&t.album.id, &t.recording, t.replace.map(|r| r.id), &b.path).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "id": fetch })))
}

/// A checked track into its album; Undo takes it out again (and puts back the one it replaced).
async fn add_fetched_track(Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let token = crate::app::track_fetch::add(&id).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "undo": [token] })))
}

/// slskd as set (never its key) and whether it answers: its version, or why not.
async fn get_slskd(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let serve = ctx.serve()?;
    let ud = serve.userdata.clone();
    let (url, key, folder) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        use crate::app::track_fetch::{SLSKD_FOLDER, SLSKD_KEY, SLSKD_URL};
        Ok((ud.import_choice(SLSKD_URL)?.unwrap_or_default(), ud.import_choice(SLSKD_KEY)?.unwrap_or_default(), ud.import_choice(SLSKD_FOLDER)?.unwrap_or_default()))
    })
    .await
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    let enabled = crate::app::switches::is_on(crate::app::switches::SLSKD);
    let mut out = json!({ "url": url, "set": !key.is_empty(), "folder": folder, "enabled": enabled });
    if enabled && !url.is_empty() && !key.is_empty() {
        match crate::slskd::Slskd::new(&url, &key) {
            Ok(s) => match s.version().await {
                Ok(v) => out["version"] = v.into(),
                Err(e) => out["error"] = format!("{e:#}").into(),
            },
            Err(e) => out["error"] = format!("{e:#}").into(),
        }
        if !folder.is_empty() && !std::path::Path::new(&folder).is_dir() {
            out["error"] = format!("{folder} is not a folder Cratarr can see").into();
        }
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
struct SlskdChoices {
    #[serde(default)]
    url: Option<String>,
    /// A new key (empty: taken away); absent: kept.
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
}

/// slskd's address, key and completed-downloads folder saved; switched on or off.
async fn put_slskd(ctx: Ctx, Json(b): Json<SlskdChoices>) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(url) = &b.url
        && !url.trim().is_empty()
        && !(url.trim().starts_with("http://") || url.trim().starts_with("https://"))
    {
        return Err(ToggleError::Refused("slskd's address starts with http:// or https://.".into()).into());
    }
    let serve = ctx.serve()?;
    let ud = serve.userdata.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        use crate::app::track_fetch::{SLSKD_FOLDER, SLSKD_KEY, SLSKD_URL};
        for (k, v) in [(SLSKD_URL, b.url), (SLSKD_KEY, b.key), (SLSKD_FOLDER, b.folder)] {
            if let Some(v) = v {
                ud.set_import_choice(k, v.trim())?;
            }
        }
        if let Some(on) = b.enabled {
            crate::app::switches::set_on(crate::app::switches::SLSKD, on)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
    .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    get_slskd(ctx).await
}

/// Whether the server's AcoustID key is saved (never it).
async fn get_acoustid(ctx: Ctx) -> Result<Json<serde_json::Value>, ApiError> {
    let serve = ctx.serve()?;
    let ud = serve.userdata.clone();
    let set = tokio::task::spawn_blocking(move || crate::app::track_fetch::acoustid_key(&ud))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .is_some();
    Ok(Json(json!({ "set": set })))
}

#[derive(Deserialize)]
struct AcoustidKey {
    /// A new key (empty: taken away).
    key: String,
}

/// The server's AcoustID application key saved (the owner registers the application).
async fn put_acoustid(ctx: Ctx, Json(b): Json<AcoustidKey>) -> Result<Json<serde_json::Value>, ApiError> {
    let key = b.key.trim().to_string();
    if !key.is_empty() && (!(4..=64).contains(&key.len()) || !key.chars().all(|c| c.is_ascii_alphanumeric())) {
        return Err(ToggleError::Refused("An AcoustID key is letters and digits.".into()).into());
    }
    let serve = ctx.serve()?;
    let ud = serve.userdata.clone();
    tokio::task::spawn_blocking(move || ud.set_import_choice(crate::app::track_fetch::ACOUSTID_KEY, &key))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    get_acoustid(ctx).await
}

/// Change cover (0.9.5): only for Cratarr's own library's albums, whose files Cratarr holds.
fn own_album(source: &str) -> Result<(), ApiError> {
    if source != crate::source::cratarr::KIND.id {
        return Err(ToggleError::Refused("Covers are changed for Cratarr's own library's albums.".into()).into());
    }
    Ok(())
}

/// An album's covers to choose from (its Discogs master's and releases', the Cover Art Archive's) and its own.
async fn album_covers(Path((source, album)): Path<(String, String)>) -> Result<Json<crate::app::covers::Covers>, ApiError> {
    own_album(&source)?;
    Ok(Json(crate::app::covers::candidates(&album).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?))
}

/// The covers looked for as a task (0.9.6), its progress on the live channel, its result as `GET …/covers` answers.
async fn look_album_covers(ctx: Ctx, Path((source, album)): Path<(String, String)>) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    own_album(&source)?;
    let account = ctx.account().ok_or_else(|| ToggleError::Refused("No one is signed in.".into()))?;
    started(crate::app::covers::start(&account, &album))
}

/// Upload cover (0.9.6): the picture sent as the body (JPEG, PNG or WebP) set as a chosen cover is; Undo as there.
async fn upload_album_cover(Path((source, album)): Path<(String, String)>, body: axum::body::Bytes) -> Result<Json<serde_json::Value>, ApiError> {
    own_album(&source)?;
    let token = crate::app::covers::upload(&album, body.to_vec()).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "undo": [token] })))
}

/// Renumber's look (0.9.6) as a task: its result the preview, what `POST …/renumber` would write.
async fn look_renumber(ctx: Ctx, Path((source, album)): Path<(String, String)>) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    own_album(&source)?;
    let account = ctx.account().ok_or_else(|| ToggleError::Refused("No one is signed in.".into()))?;
    started(crate::app::renumber::start(&account, &album))
}

/// The numbers shown in the preview written into the album's tracks; Undo puts the old ones back.
async fn renumber_album(Path((source, album)): Path<(String, String)>, Json(b): Json<crate::app::renumber::Asked>) -> Result<Json<serde_json::Value>, ApiError> {
    own_album(&source)?;
    let token = crate::app::renumber::apply(&album, b).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "undo": [token] })))
}

#[derive(Deserialize)]
struct FixesOf {
    finding: String,
}

/// A Health finding's fixes (0.9.7, `app::fixes`): each album's, the certain first, and how many are certain.
async fn health_fixes(ctx: Ctx, Query(q): Query<FixesOf>) -> Result<Json<serde_json::Value>, ApiError> {
    let app = ctx.app.clone();
    let fixes = crate::app::fixes::of_finding(&app, &q.finding).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    let certain = fixes.iter().filter(|f| f.certainty == "certain").count();
    // Album artist and MusicBrainz ids come from matches made in a task (`match`): the page asks again as it runs.
    let matching = ctx.account().is_some_and(|a| crate::app::matching::running(&a));
    Ok(Json(json!({ "fixes": fixes, "certain": certain, "matching": matching })))
}

/// Fixes to apply: these (`ids`), or every certain one of a finding (`certain`: its key).
#[derive(Deserialize)]
struct ApplyFixes {
    #[serde(default)]
    ids: Vec<String>,
    #[serde(default)]
    certain: Option<String>,
    /// Genre fixes' chosen genres (0.9.8), by fix id: written as several genres.
    #[serde(default)]
    chosen: std::collections::HashMap<String, Vec<String>>,
}

/// Fixes written into the files (0.9.7, an administrator's tap): `{undo: [token], applied, failed: [{id, why}]}`.
async fn apply_fixes(State(app): State<Arc<App>>, Json(b): Json<ApplyFixes>) -> Result<Json<serde_json::Value>, ApiError> {
    let refused = |e: anyhow::Error| ToggleError::Refused(format!("{e:#}"));
    let fixes = match &b.certain {
        Some(finding) => crate::app::fixes::of_finding(&app, finding).await.map_err(refused)?.into_iter().filter(|f| f.certainty == "certain").collect(),
        None => crate::app::fixes::by_ids(&app, &b.ids).await.map_err(refused)?,
    };
    let mut fixes = fixes;
    crate::app::fixes::choose(&mut fixes, &b.chosen).map_err(refused)?;
    if fixes.is_empty() {
        return Err(ToggleError::Refused("Nothing to fix: look again".into()).into());
    }
    // Written whole only (0.9.8: `applied` counted those tried, and said a fix was in when it had failed).
    let (tokens, written, failed) = crate::app::fixes::apply_all(fixes).await.map_err(refused)?;
    let failed: Vec<serde_json::Value> = failed.into_iter().map(|(id, why)| json!({ "id": id, "why": why })).collect();
    Ok(Json(json!({ "undo": tokens, "applied": written, "failed": failed })))
}

/// An arrival's intake (0.9.7): every fix the album would get before it is kept (year, genre, album artist,
/// MusicBrainz ids, cover), the certain first; `matching` while its match is looked for. Applied as Health's are.
async fn album_intake(State(app): State<Arc<App>>, Path((source, album)): Path<(String, String)>) -> Result<Json<serde_json::Value>, ApiError> {
    own_album(&source)?;
    let (mut fixes, matching) = crate::app::fixes::of_album(&app, &album).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    fixes.sort_by(|a, b| (a.certainty != "certain").cmp(&(b.certainty != "certain")).then_with(|| a.field.cmp(b.field)));
    let certain = fixes.iter().filter(|f| f.certainty == "certain").count();
    Ok(Json(json!({ "fixes": fixes, "certain": certain, "matching": matching })))
}

#[derive(Deserialize)]
struct SetCover {
    url: String,
}

/// An album's cover set to one of its candidates: beside its tracks and inside each; Undo puts the old one back.
async fn set_album_cover(Path((source, album)): Path<(String, String)>, Json(b): Json<SetCover>) -> Result<Json<serde_json::Value>, ApiError> {
    own_album(&source)?;
    let token = crate::app::covers::set(&album, &b.url).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    Ok(Json(json!({ "undo": [token] })))
}

/// How MusicBrainz stands (0.9.6, Settings › Connections): when last reached, the last failure, the background pass.
async fn musicbrainz_status() -> Result<Json<crate::app::musicbrainz::Status>, ApiError> {
    Ok(Json(crate::app::musicbrainz::status().await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?))
}

/// Every outside connection's state since the server started (0.9.6): when it last answered, its last failure, and
/// whether it is failing now (as Health says).
async fn connections_state() -> Json<serde_json::Value> {
    let now = crate::app::moment::now_secs();
    let failing: Vec<&str> = crate::app::reach::failing(now).into_iter().map(|(n, _)| n).collect();
    let all: Vec<serde_json::Value> = ["musicbrainz", "discogs", "lastfm", "listenbrainz", "coverartarchive", "bandcamp", "theaudiodb", "wikimedia"]
        .into_iter()
        .map(|name| {
            let s = crate::app::reach::state(name);
            json!({
                "name": name,
                "label": crate::app::reach::label(name),
                "last_ok": Some(s.last_ok).filter(|t| *t > 0),
                "last_error": s.last_error.map(|(at, message)| json!({ "at": at, "message": message })),
                "failing": failing.contains(&name),
            })
        })
        .collect();
    Json(json!({ "connections": all }))
}

/// Cratarr restarted from Settings (0.9.6, the owner: no command line needed): answered at once, then stopped as
/// `docker stop` stops it (`main`'s graceful stop); Docker's restart policy (unless-stopped) brings it back.
async fn restart_server() -> (StatusCode, Json<serde_json::Value>) {
    tracing::info!(target: "server", "restart asked from Settings");
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        crate::RESTART.notify_one();
    });
    (StatusCode::ACCEPTED, Json(json!({ "restarting": true })))
}

#[derive(Deserialize)]
struct Switched {
    enabled: bool,
}

/// MusicBrainz switched on or off (0.9.6): off, Cratarr asks it nothing (the background pass waits; Find more music
/// and Renumber say so).
async fn musicbrainz_switch(Json(b): Json<Switched>) -> Result<Json<crate::app::musicbrainz::Status>, ApiError> {
    tokio::task::spawn_blocking(move || crate::app::switches::set_on(crate::app::switches::MUSICBRAINZ, b.enabled))
        .await
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
    musicbrainz_status().await
}

/// A release group's cover for albums not in the library (0.9.5): to the Cover Art Archive's, else Discogs' master's;
/// 404 where neither has one.
async fn release_group_cover(Path(rg): Path<String>) -> Response {
    if !crate::app::discogs::is_mbid(&rg) {
        return StatusCode::NOT_FOUND.into_response();
    }
    kept_cover(crate::app::cover_cache::Of::ReleaseGroup(rg)).await
}

/// Lidarr's quality and metadata profiles and root folders (its page's choices for Want), of the saved Lidarr.
async fn lidarr_options(State(app): State<Arc<App>>) -> Result<Json<crate::lidarr::Options>, ApiError> {
    let l = crate::app::discover::lidarr(&app).ok_or_else(|| ToggleError::Refused("Lidarr is not connected, or is switched off.".into()))?;
    Ok(Json(l.options().await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?))
}

async fn preview_lidarr(
    State(app): State<Arc<App>>,
    Json(b): Json<crate::settings::LidarrConfig>,
) -> Result<Json<Option<crate::app::LidarrView>>, ApiError> {
    Ok(Json(app.settings.preview_lidarr(b).await?))
}

#[derive(Deserialize)]
struct InsightsQuery {
    source: Option<String>,
    period: Option<crate::app::history::Period>,
}

/// Insights (decision 0065): Collection's figures from the libraries switched on (`without`), of the server followed,
/// of the albums that came in the period (all when none is given).
async fn insights(State(app): State<Arc<App>>, Query(f): Query<InsightsQuery>, RawQuery(q): RawQuery) -> Result<Json<crate::insights::Insights>, ApiError> {
    let period = f.period.unwrap_or(crate::app::history::Period::All);
    Ok(Json(app.insights.get(f.source.as_deref(), &without_of(q.as_deref()), period).await.map(|r| (*r).clone()).map_err(ToggleError::from)?))
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
            ToggleError::SettingsReadOnly | ToggleError::CannotPlay(_) | ToggleError::Refused(_) | ToggleError::NoMusicFolder | ToggleError::NameTaken(_) => {
                StatusCode::CONFLICT
            }
            ToggleError::UnknownPlaylist => StatusCode::NOT_FOUND,
            ToggleError::Forbidden(_) | ToggleError::Fenced => StatusCode::FORBIDDEN,
            ToggleError::Upstream(_) => StatusCode::BAD_GATEWAY,
        };
        if matches!(self.0, ToggleError::Upstream(_)) {
            tracing::error!("{:#}", self.0);
        }
        let mut body = json!({ "error": crate::app::plain(self.0.to_string()) });
        // The device takes another name (decision 0051).
        if matches!(self.0, ToggleError::NameTaken(_)) {
            body["taken"] = true.into();
        }
        // Another user's shared device, away from home: the page says so as it says any refusal.
        if matches!(self.0, ToggleError::Fenced) {
            body["fenced"] = true.into();
        }
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::{ExportRequest, PreviewOf, attachment, form_decode};

    #[test]
    fn an_answer_says_whether_the_change_was_made_only_where_the_device_sent_its_time() {
        use super::{ChangeAt, applied};
        let v = || serde_json::json!({ "favorite": true });
        assert_eq!(applied(v(), ChangeAt(Some(5)), false).0, serde_json::json!({ "favorite": true, "applied": false }));
        assert_eq!(applied(v(), ChangeAt(Some(5)), true).0, serde_json::json!({ "favorite": true, "applied": true }));
        assert_eq!(applied(v(), ChangeAt(None), true).0, v(), "as before without it");
    }

    /// axum refuses to start on a path and method routed twice (0.8.3 did, on `/bands`, and never came up); no test
    /// builds the whole router, so this reads the table here instead: one route a line, its methods on it.
    #[test]
    fn levelling_only_turns_down_and_never_past_the_peak() {
        use super::turned_down;
        assert_eq!(turned_down(Some(-6.5), Some(0.9)), Some(-6.5));
        assert_eq!(turned_down(Some(3.0), Some(0.5)), Some(0.0), "never up");
        assert_eq!(turned_down(Some(-1.0), Some(1.2)), Some(-1.58), "never past the peak");
        assert_eq!(turned_down(None, Some(0.9)), None, "not known");
        assert_eq!(turned_down(Some(-4.0), None), Some(-4.0));
    }

    #[test]
    fn no_path_and_method_is_routed_twice() {
        let mut seen = std::collections::HashSet::new();
        for line in include_str!("api.rs").lines().filter(|l| l.trim_start().starts_with(".route(\"")) {
            let path = line.split('"').nth(1).unwrap();
            for (i, _) in line.match_indices('(') {
                let word = line[..i].rsplit(|c: char| !c.is_ascii_alphabetic()).next().unwrap();
                if ["get", "post", "put", "delete", "patch"].contains(&word) {
                    assert!(seen.insert((path.to_string(), word.to_string())), "{word} {path} is routed twice");
                }
            }
        }
        assert!(seen.len() > 100, "the table was not read: {}", seen.len());
    }

    #[tokio::test]
    async fn every_answer_carries_its_request_id() {
        use axum::body::Body;
        use tower::ServiceExt;
        let app = axum::Router::new().route("/x", axum::routing::get(|| async { "ok" })).layer(axum::middleware::from_fn(super::request_ids));
        let id_of = |r: axum::response::Response| r.headers()["x-request-id"].to_str().unwrap().to_string();
        let ask = |id: Option<&str>| {
            let mut b = axum::http::Request::get("/x");
            if let Some(id) = id {
                b = b.header("x-request-id", id);
            }
            b.body(Body::empty()).unwrap()
        };
        // A proxy's id is kept; without one, or with one that is not a plain token, Cratarr makes its own.
        assert_eq!(id_of(app.clone().oneshot(ask(Some("caddy-42"))).await.unwrap()), "caddy-42");
        let own = id_of(app.clone().oneshot(ask(None)).await.unwrap());
        assert!(own.len() == 16 && own.bytes().all(|b| b.is_ascii_hexdigit()), "{own}");
        assert_ne!(id_of(app.clone().oneshot(ask(Some("a b"))).await.unwrap()), "a b");
        assert_ne!(own, id_of(app.oneshot(ask(None)).await.unwrap()));
    }

    #[tokio::test]
    async fn a_slow_answer_is_named_by_its_method_and_route() {
        use axum::body::Body;
        use tower::ServiceExt;
        // Where request_ids runs (a layer around the routes), the route that took the request is known.
        let seen = |req: &axum::extract::Request| super::route_of(req.method(), req.extensions().get::<axum::extract::MatchedPath>().map(|m| m.as_str()), req.uri().path());
        // On one line: `no_path_and_method_is_routed_twice` reads each line that starts with a route as the table's.
        let wind = axum::Router::new().route("/api/stream/{source}/{id}/wind", axum::routing::get(|| async { "ok" }));
        let app = wind
            .fallback(|| async { "page" })
            .layer(axum::middleware::from_fn(move |req: axum::extract::Request, next: axum::middleware::Next| async move {
                let route = seen(&req);
                let mut res = next.run(req).await;
                res.headers_mut().insert("x-route", axum::http::HeaderValue::from_str(&route).unwrap());
                res
            }));
        let route = |uri: &str| {
            let app = app.clone();
            let uri = uri.to_string();
            async move { app.oneshot(axum::http::Request::get(uri.as_str()).body(Body::empty()).unwrap()).await.unwrap().headers()["x-route"].to_str().unwrap().to_string() }
        };
        assert_eq!(route("/api/stream/cratarr/tr-1/wind?dir=back").await, "GET /api/stream/{source}/{id}/wind");
        assert_eq!(route("/albums/some/where/deep").await, "GET /albums/some/where", "no route: the path's first three parts");
        assert_eq!(super::route_of(&axum::http::Method::PUT, Some("/api/players/{name}"), "/api/players/Phone"), "PUT /api/players/{name}");
    }

    #[test]
    fn the_export_address_names_playlists_and_options() {
        let r = ExportRequest::from_query("playlist=Chill&playlist=Caf%C3%A9+Jazz&tracks=1&flat=1&combine=Chill+%2B+Jazz");
        assert_eq!(r.playlists, ["Chill", "Café Jazz"]);
        assert!(r.tracks && r.flat);
        assert_eq!(r.combine.as_deref(), Some("Chill + Jazz"));
        assert_eq!(form_decode("100%"), "100%", "a lone % stays");
        let o = ExportRequest::from_query("playlist=A&flat=1").options().ok().unwrap();
        assert_eq!(o.arrange, crate::export::Arrange::Library, "one folder only applies with tracks");
        assert!(ExportRequest::from_query("tracks=1").options().is_err(), "no playlist");
        assert!(ExportRequest::from_query("playlist=A&combine=+").options().is_err(), "a blank combined name");
    }

    #[test]
    fn an_export_names_what_its_format_and_its_layout() {
        use crate::export::{Arrange, Format};
        let parse = |s: &str| match serde_json::from_str::<PreviewOf>(s).unwrap() {
            PreviewOf::Export(e) => e.options(),
            PreviewOf::Playlists(_) => panic!("read as the Export tab's"),
        };
        let o = parse(r#"{"what":{"kind":"tracks","name":"Alb","tracks":[{"source":"cratarr","id":"tr-1"}]},"format":"opus","bitrate":128,"layout":"flat","m3u":true}"#).ok().unwrap();
        assert!(o.tracks && o.lists && o.combine.is_none());
        assert_eq!((o.format, o.arrange), (Format::Opus(128), Arrange::Flat));
        let o = parse(r#"{"what":{"kind":"playlists","names":["A","B"]},"format":"flac-cd","tracks":false,"combine":"A + B"}"#).ok().unwrap();
        assert!(!o.tracks && o.lists);
        assert_eq!((o.format, o.arrange, o.combine.as_deref()), (Format::FlacCd, Arrange::Folders, Some("A + B")));
        let o = parse(r#"{"what":{"kind":"tracks","name":"Q","tracks":[{"source":"s","id":"1"}]}}"#).ok().unwrap();
        assert_eq!((o.format, o.lists), (Format::Original, false), "as they are, no playlist file unless asked");
        assert!(parse(r#"{"what":{"kind":"tracks","name":"Q","tracks":[{"source":"s","id":"1"}]},"format":"opus","bitrate":64}"#).is_err(), "a bit rate not offered");
        assert!(parse(r#"{"what":{"kind":"tracks","name":"Q","tracks":[]}}"#).is_err(), "no tracks");
        assert!(matches!(serde_json::from_str::<PreviewOf>(r#"{"playlists":["A"],"tracks":true}"#).unwrap(), PreviewOf::Playlists(_)), "the Export tab's body still");
        let o = parse(r#"{"what":{"kind":"tracks","name":"Q","tracks":[{"source":"s","id":"1"}]},"format":"mp3-320","bitrate":64}"#).ok().unwrap();
        assert_eq!(o.format, Format::Mp3Cbr320, "an MP3 takes no bit rate: any is let be");
        let o = parse(r#"{"what":{"kind":"tracks","name":"Q","tracks":[{"source":"s","id":"1"}]},"format":"mp3-v0"}"#).ok().unwrap();
        assert_eq!(o.format, Format::Mp3V0);
        assert!(serde_json::from_str::<PreviewOf>(r#"{"what":{"kind":"tracks","name":"Q","tracks":[]},"format":"mp3"}"#).is_err(), "no such format");
    }

    #[test]
    fn an_export_download_is_named_and_typed_by_its_one_file_or_its_zip() {
        use axum::http::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE};
        let value = |h: &axum::http::HeaderMap, k: axum::http::HeaderName| h.get(k).map(|v| v.to_str().unwrap().to_string());
        let h = super::download_headers("01 So What.mp3", Some(7));
        assert_eq!(value(&h, CONTENT_TYPE).as_deref(), Some("audio/mpeg"));
        assert_eq!(value(&h, CONTENT_LENGTH).as_deref(), Some("7"));
        assert_eq!(value(&h, CONTENT_DISPOSITION), Some(attachment("01 So What.mp3")), "the file's own name");
        let h = super::download_headers("Kind of Blue.zip", None);
        assert_eq!(value(&h, CONTENT_TYPE).as_deref(), Some("application/zip"));
        assert_eq!(value(&h, CONTENT_LENGTH), None, "a converted file's size is known only once it is made");
        assert_eq!(value(&super::download_headers("Chill.m3u", Some(3)), CONTENT_TYPE).as_deref(), Some("audio/x-mpegurl"));
        assert_eq!(super::not_sent(crate::export::Treatment::Mp3(crate::export::Mp3Kind::V0)), "That track could not be converted to MP3.");
        let old = r#"{"what":{"kind":"playlists","names":["A"]},"package":"files"}"#;
        assert!(serde_json::from_str::<super::Export>(old).is_ok(), "an older page's packaging is let be: one file or a zip all the same");
    }

    /// A fake Bandcamp: an album whose first track streams `frames` MP3 frames (128 kb/s, 417 bytes each) from a server
    /// of its own, which answers a range as Bandcamp's does. The album is taken as read: no page is asked for.
    async fn fake_bandcamp(page: &str, frames: usize) -> (String, Vec<u8>) {
        use axum::http::{HeaderMap, StatusCode, header};
        use axum::response::IntoResponse;
        let mut mp3: Vec<u8> = Vec::new();
        for _ in 0..frames {
            let mut f = vec![0xFF, 0xFB, 0x90, 0x64];
            f.resize(417, 0);
            mp3.extend(f);
        }
        let body = mp3.clone();
        let app = axum::Router::new().route(
            "/s",
            axum::routing::get(move |h: HeaderMap| {
                let body = body.clone();
                async move {
                    let from = h.get(header::RANGE).and_then(|v| v.to_str().ok()?.strip_prefix("bytes=")?.strip_suffix('-')?.parse::<usize>().ok());
                    match from {
                        Some(f) => {
                            let range = format!("bytes {f}-{}/{}", body.len() - 1, body.len());
                            (StatusCode::PARTIAL_CONTENT, [(header::CONTENT_RANGE, range)], body[f..].to_vec()).into_response()
                        }
                        None => (StatusCode::OK, [(header::ACCEPT_RANGES, "bytes".to_string())], body).into_response(),
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let stream = format!("http://{}/s", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        crate::bandcamp::Bandcamp::get().keep(crate::bandcamp::Album {
            url: page.into(),
            title: "Fake".into(),
            artist: "Faker".into(),
            cover: None,
            tracks: vec![crate::bandcamp::Track { n: 1, title: "One".into(), artist: None, duration_secs: Some(12.5), stream: Some(stream), page: None }],
            released: None,
            label: None,
            about: None,
            credits: None,
            tags: Vec::new(),
        });
        (crate::bandcamp::track_id(page, 1), mp3)
    }

    async fn body(r: axum::response::Response) -> Vec<u8> {
        axum::body::to_bytes(r.into_body(), usize::MAX).await.unwrap().to_vec()
    }

    /// What `/api/stream/bandcamp/{id}` and `/api/file/bandcamp/{id}` answer, whatever form a player asks for (`as`,
    /// `whole`): the preview relayed as it is.
    #[tokio::test]
    async fn a_preview_streams_through_cratarr_as_it_is_with_its_ranges() {
        use axum::http::StatusCode;
        let page = "https://relay.bandcamp.com/album/fake";
        let (id, mp3) = fake_bandcamp(page, 60).await;
        // From its start: its MP3, its length from its page, Bandcamp's own length and ranges.
        let r = super::bandcamp_relay(&id, None, 0).await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers()["content-type"], "audio/mpeg");
        assert_eq!(r.headers()["x-duration-ms"], "12500");
        assert_eq!(r.headers()["accept-ranges"], "bytes");
        assert_eq!(body(r).await, mp3);
        // A player's own range (the app's, Kodi's) passed on, and Bandcamp's answer to it.
        let r = super::bandcamp_relay(&id, Some("bytes=1000-"), 0).await;
        assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(r.headers()["content-range"], format!("bytes 1000-{}/{}", mp3.len() - 1, mp3.len()).as_str());
        assert_eq!(body(r).await, &mp3[1000..]);
        // From a moment (the page's seek, `from_ms`): one second is 16000 bytes in, the next whole frame (the 40th) 263
        // bytes on, 16 ms later; no length of its own.
        let r = super::bandcamp_relay(&id, Some("bytes=5-"), 1000).await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers()["x-start-ms"], "1016");
        assert!(r.headers().get("content-range").is_none());
        assert_eq!(body(r).await, &mp3[39 * 417..]);
        // Not a preview's track: nothing asked of Bandcamp.
        assert_eq!(super::bandcamp_relay("bc.!!", None, 0).await.status(), StatusCode::NOT_FOUND);
        let album = crate::bandcamp::track_id(page, 0);
        assert_eq!(super::bandcamp_relay(&album, None, 0).await.status(), StatusCode::NOT_FOUND, "the album is no track");
    }

    #[test]
    fn a_preview_is_never_kept_offline_nor_copied() {
        let t = |source: &str| super::TrackRefOf { source: source.into(), id: "bc.x".into() };
        let refused = super::never_offline(&t(crate::bandcamp::SOURCE)).unwrap();
        assert_eq!((refused["offline"].as_bool(), refused["error"].as_str()), (Some(false), Some(crate::bandcamp::NOT_KEPT)));
        assert!(super::never_offline(&t("cratarr")).is_none(), "a library track goes on to its file or copy");
    }

    #[test]
    fn a_download_name_is_given_twice_ascii_and_utf8() {
        assert_eq!(attachment("Café \"x\".zip"), "attachment; filename=\"Caf_ _x_.zip\"; filename*=UTF-8''Caf%C3%A9%20%22x%22.zip");
    }
}
