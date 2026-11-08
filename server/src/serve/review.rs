//! Reviewing Incoming albums by Cratarr's album id (decision 0030), for Home Assistant's voice commands: promote
//! (Incoming's Keep: into a main library, `?to=` or the server's first) and reject (Incoming's Delete: to the trash,
//! and Lidarr stops fetching it), each undoable through Incoming's own undo, which is what "take that back" calls.
//! Incoming itself works by the media server's album; the album is found there by its folder, as Incoming finds its
//! own (the server's path ends with the album's folder). Incoming's behaviour is unchanged.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use serde_json::json;

use super::Serve;
use super::copies::Kind;
use crate::userdata::Review;

pub const PROMOTED: &str = "promoted";
pub const REJECTED: &str = "rejected";
pub const UNREVIEWED: &str = "unreviewed";

/// In a review's undo key: a Keep queued for conversion, by the server's album id (its job has the real key).
pub const JOB: &str = "job:";

pub fn router(serve: Arc<Serve>) -> Router {
    Router::new()
        // Reviewing what comes in is the administrators' (decision 0051), as under `/api/incoming/`. On these routes alone:
        // `layer` holds a router's fallback too, and `merge` hands that on, so whatever no route took would be fenced.
        .merge(
            Router::new()
                .route("/api/library/albums/{album}/promote", post(promote))
                .route("/api/library/albums/{album}/reject", post(reject))
                .route("/api/library/albums/{album}/review", delete(unreview))
                .route_layer(axum::middleware::from_fn(super::access::admin_or_key)),
        )
        .route("/api/library/incoming", get(incoming))
        .with_state(serve)
}

fn fail(status: StatusCode, message: impl Into<String>) -> Response {
    (status, axum::Json(json!({ "error": crate::app::plain(message.into()) }))).into_response()
}

fn failed(e: impl std::fmt::Display) -> Response {
    fail(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// An album's review state as the API shows it: its review when it has one, "unreviewed" while it waits in Incoming,
/// null for an album of the curated library. `waiting`: the catalogue folder of its copy in Incoming, what is reviewed.
pub fn state_of(serve: &Serve, album: &str, waiting: Option<i64>, in_incoming: bool) -> anyhow::Result<Option<String>> {
    if let Some(r) = serve.userdata.review(album, waiting)? {
        return Ok(Some(r.state));
    }
    Ok(in_incoming.then(|| UNREVIEWED.to_string()))
}

/// The album as the library has it: (name, album artist, its catalogue folder, its folder path).
async fn find(serve: &Arc<Serve>, id: &str) -> Result<Option<(String, String, i64, String)>, Response> {
    let (s, id) = (serve.clone(), id.to_string());
    tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let Some(album) = s.library.albums_by_id(std::slice::from_ref(&id))?.pop() else { return Ok(None) };
        // What is reviewed is what waits in Incoming, whatever else holds the record.
        let copies = s.library.copies_of(&id)?;
        let Some(copy) = copies.iter().find(|c| c.kind == Kind::Incoming).or(copies.first()) else { return Ok(None) };
        Ok(Some((album.name, album.artist, copy.folder, copy.dir.clone())))
    })
    .await
    .map_err(failed)?
    .map_err(failed)
}

/// The Incoming album at this place, as Incoming knows it: (server, the server's album id, its library). Cratarr's own says
/// where it lies; another server's is known by its path's end.
async fn in_incoming(serve: &Serve, folder: i64, dir: &str) -> Option<(String, String, Option<String>)> {
    let incoming = serve.incoming()?;
    let view = incoming.list(None).await;
    let tail = format!("/{}", dir.trim_matches('/'));
    let here = |a: &&crate::source::Album| match a.copies.first() {
        Some(c) => c.folder == folder && c.dir == dir,
        None => a.path.as_deref().is_some_and(|p| p.trim_end_matches('/').ends_with(&tail)),
    };
    view.sources.iter().find_map(|s| s.albums.iter().find(here).map(|a| (s.id.clone(), a.id.clone(), a.library.clone())))
}

fn answer(name: &str, artist: &str, review: &str, changed: bool) -> Response {
    axum::Json(json!({ "name": name, "album_artist": artist, "review": review, "changed": changed })).into_response()
}

/// Whether a catalogue folder's records are kept: a main library's or own music's, not Incoming's.
fn in_main(serve: &Serve, folder: i64) -> bool {
    serve.library.kind_of(folder).0 != Kind::Incoming
}

async fn act(serve: Arc<Serve>, id: String, wanted: &'static str, to: Option<String>) -> Response {
    let found = match find(&serve, &id).await {
        Ok(found) => found,
        Err(r) => return r,
    };
    // Of the copy that waits, so a new one of a record promoted from elsewhere is unreviewed; with none, the last word stands.
    let waiting = found.as_ref().map(|f| f.2).filter(|folder| !in_main(&serve, *folder));
    if let Ok(Some(done)) = serve.userdata.review(&id, waiting) {
        if done.state == wanted {
            return answer(&done.name, &done.album_artist, wanted, false);
        }
        return fail(StatusCode::CONFLICT, format!("{} was {} already: take that back first", done.name, done.state));
    }
    let Some((name, artist, folder, dir)) = found else { return fail(StatusCode::NOT_FOUND, "no such album") };
    if in_main(&serve, folder) {
        return fail(StatusCode::CONFLICT, format!("{name} is in the library, not in Incoming"));
    }
    let Some(incoming) = serve.incoming().cloned() else { return fail(StatusCode::SERVICE_UNAVAILABLE, "Incoming is not ready yet") };
    let Some((source, server_album, library)) = in_incoming(&serve, folder, &dir).await else {
        return fail(StatusCode::CONFLICT, format!("{name} is not in Incoming"));
    };
    let undo = if wanted == PROMOTED {
        match incoming.keep(&source, &server_album, library.as_deref(), to.as_deref(), None).await {
            // A Keep that waits for its conversion gets its undo key when the job is done: remember the job's album.
            Ok(k) => Some(k.undo.unwrap_or_else(|| format!("{JOB}{server_album}"))),
            Err(e) => return fail(StatusCode::CONFLICT, e.to_string()),
        }
    } else {
        match incoming.delete(&source, &server_album, library.as_deref()).await {
            Ok(d) => Some(d.undo),
            Err(e) => return fail(StatusCode::CONFLICT, e.to_string()),
        }
    };
    let review = Review { album: id, folder, state: wanted.into(), at: now(), undo, name: name.clone(), album_artist: artist.clone() };
    if let Err(e) = serve.userdata.set_review(&review) {
        // Done, but not remembered: say so, so "take that back" is not promised.
        return failed(format!("{name} was {wanted}, but Cratarr could not remember it: {e:#}"));
    }
    answer(&name, &artist, wanted, true)
}

/// Where promote keeps an album (0101 point 2): a main library's id; none, the server's first main library.
#[derive(serde::Deserialize, Default)]
struct PromoteTo {
    #[serde(default)]
    to: Option<String>,
}

async fn promote(State(serve): State<Arc<Serve>>, Path(id): Path<String>, Query(q): Query<PromoteTo>) -> Response {
    act(serve, id, PROMOTED, q.to).await
}

async fn reject(State(serve): State<Arc<Serve>>, Path(id): Path<String>) -> Response {
    act(serve, id, REJECTED, None).await
}

/// Takes a promote or reject back: Incoming undoes the Keep or the Delete, and the album is unreviewed again.
async fn unreview(State(serve): State<Arc<Serve>>, Path(id): Path<String>) -> Response {
    let done = match serve.userdata.review(&id, None) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return match find(&serve, &id).await {
                Ok(Some((name, artist, folder, _))) if !in_main(&serve, folder) => answer(&name, &artist, UNREVIEWED, false),
                Ok(Some((name, _, _, _))) => fail(StatusCode::CONFLICT, format!("{name} is in the library, not in Incoming")),
                Ok(None) => fail(StatusCode::NOT_FOUND, "no such album"),
                Err(r) => r,
            };
        }
        Err(e) => return failed(e),
    };
    let Some(incoming) = serve.incoming().cloned() else { return fail(StatusCode::SERVICE_UNAVAILABLE, "Incoming is not ready yet") };
    let still = || fail(StatusCode::CONFLICT, format!("{} is still being converted; take it back once it is done", done.name));
    let key = match done.undo.as_deref() {
        // A queued Keep: its job has the undo key once done.
        Some(job) if job.starts_with(JOB) => {
            let album = &job[JOB.len()..];
            match incoming.jobs().into_iter().rev().find(|j| j.album_id == album).and_then(|j| j.undo) {
                Some(k) => k,
                None => return still(),
            }
        }
        Some(k) => k.to_string(),
        None => return still(),
    };
    let key = key.as_str();
    if let Err(e) = incoming.undo(key).await {
        return fail(StatusCode::CONFLICT, e.to_string());
    }
    if let Err(e) = serve.userdata.clear_review(&done.album, done.folder) {
        return failed(e);
    }
    answer(&done.name, &done.album_artist, UNREVIEWED, true)
}

#[derive(serde::Deserialize)]
struct IncomingQuery {
    review: Option<String>,
}

/// The albums waiting in Incoming, newest first, with their review state; `?review=unreviewed` leaves out the
/// reviewed ones.
async fn incoming(State(serve): State<Arc<Serve>>, Query(q): Query<IncomingQuery>) -> Response {
    let s = serve.clone();
    let read = tokio::task::spawn_blocking(move || -> anyhow::Result<_> { Ok((s.library.incoming_albums()?, s.library.waiting_copies()?)) }).await;
    let (albums, waiting) = match read {
        Ok(Ok(a)) => a,
        Ok(Err(e)) => return failed(e),
        Err(e) => return failed(e),
    };
    let mut out = Vec::new();
    for a in albums {
        let state = match state_of(&serve, &a.id, waiting.get(&a.id).copied(), true) {
            Ok(st) => st.unwrap_or_else(|| UNREVIEWED.into()),
            Err(e) => return failed(e),
        };
        if q.review.as_deref().is_some_and(|w| w != state) {
            continue;
        }
        out.push(json!({ "id": a.id, "name": a.name, "album_artist": a.artist, "added": a.created, "review": state }));
    }
    axum::Json(json!({ "albums": out })).into_response()
}
