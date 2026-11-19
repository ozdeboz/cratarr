//! Cratarr's own library calls (decision 0030), for what the Subsonic API has no word for and for callers that cannot
//! log in the Subsonic way (Home Assistant's templates have no md5): album tags, favourites, a health check, a song's
//! facts and its file. Plain JSON under `/api/library`, behind the sign-in as the rest of `/api`; errors are a status
//! with `{"error": "…"}`. They act for the account asking; what is everyone's takes an administrator or a key of theirs.
//!
//! Ids are the ones the Subsonic API hands out: songs `tr-…`, albums `al-…`.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::json;

use super::Serve;
use crate::userdata::Kind;

/// How long the health check waits for the databases before it says they are not answering.
pub const HEALTH_TIMEOUT: Duration = Duration::from_secs(10);

pub fn router(serve: Arc<Serve>) -> Router {
    Router::new()
        .route("/api/library/health", get(health))
        .route("/api/library/albums/{album}", get(album))
        .route("/api/library/albums/{album}/resume", get(resume).delete(forget_resume))
        // An album's tags are the household's (`userdata::tags`): changes to the library, the administrators' (decision
        // 0051), and Home Assistant's. What is the account's own here, its hearts and its resume points, stays open to it.
        .merge(
            Router::new()
                .route("/api/library/albums/{album}/tags", axum::routing::delete(clear_tags))
                .route("/api/library/albums/{album}/tags/{tag}", post(add_tag).delete(remove_tag))
                .route_layer(axum::middleware::from_fn(super::access::admin_or_key)),
        )
        .route("/api/library/tags", get(all_tags))
        .route("/api/library/tags/{tag}", get(tagged))
        .route("/api/library/tracks/{track}", get(track))
        .route("/api/library/tracks/{track}/favourite", post(favourite).delete(unfavourite))
        .route("/api/library/tracks/{track}/file", get(file))
        .with_state(serve.clone())
        .merge(super::review::router(serve))
}

fn fail(status: StatusCode, message: impl Into<String>) -> Response {
    (status, axum::Json(json!({ "error": crate::app::plain(message.into()) }))).into_response()
}

fn failed(e: impl std::fmt::Display) -> Response {
    fail(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))
}

/// Blocking work off the async threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> Result<T, Response> {
    tokio::task::spawn_blocking(f).await.map_err(failed)?.map_err(failed)
}

/// 200 with counts when both databases answer within `HEALTH_TIMEOUT`, else 503: a running process with a stuck
/// database must not look healthy.
async fn health(State(serve): State<Arc<Serve>>) -> Response {
    let s = serve.clone();
    let check = tokio::task::spawn_blocking(move || -> anyhow::Result<(u64, u64)> {
        let counts = s.library.health()?;
        s.userdata.health()?;
        Ok(counts)
    });
    match tokio::time::timeout(HEALTH_TIMEOUT, check).await {
        Ok(Ok(Ok((songs, albums)))) => axum::Json(json!({ "songs": songs, "albums": albums })).into_response(),
        Ok(Ok(Err(e))) => fail(StatusCode::SERVICE_UNAVAILABLE, format!("{e:#}")),
        Ok(Err(e)) => fail(StatusCode::SERVICE_UNAVAILABLE, e.to_string()),
        Err(_) => fail(StatusCode::SERVICE_UNAVAILABLE, "the library database is not answering"),
    }
}

/// An album with its tags and review state ("unreviewed", "promoted", "rejected"; null for an album of the curated
/// library).
async fn album(State(serve): State<Arc<Serve>>, Path(id): Path<String>) -> Response {
    let s = serve.clone();
    let found = blocking(move || {
        let Some(album) = s.library.albums_by_id(std::slice::from_ref(&id))?.pop() else { return Ok(None) };
        let in_incoming = s.library.in_incoming(&id)?;
        let waiting = s.library.copies_of(&id)?.into_iter().find(|c| c.kind == super::copies::Kind::Incoming).map(|c| c.folder);
        let review = super::review::state_of(&s, &id, waiting, in_incoming)?;
        Ok(Some((album, s.userdata.tags_of(&id)?, review)))
    })
    .await;
    match found {
        Ok(Some((a, tags, review))) => {
            axum::Json(json!({ "id": a.id, "name": a.name, "album_artist": a.artist, "tags": tags, "review": review })).into_response()
        }
        Ok(None) => fail(StatusCode::NOT_FOUND, "no such album"),
        Err(r) => r,
    }
}

/// An album's songs (its ids), or 404.
async fn album_songs(serve: &Arc<Serve>, id: String) -> Result<Vec<String>, Response> {
    let s = serve.clone();
    match blocking(move || Ok(s.library.album(&id)?.map(|a| a.song.unwrap_or_default().into_iter().map(|x| x.id).collect::<Vec<_>>()))).await {
        Ok(Some(songs)) => Ok(songs),
        Ok(None) => Err(fail(StatusCode::NOT_FOUND, "no such album")),
        Err(r) => Err(r),
    }
}

/// Where the owner left an album (Symfonium's Resume): the newest bookmark among its songs, kept by any app that
/// keeps bookmarks; `{"track": null}` when there is none.
async fn resume(State(serve): State<Arc<Serve>>, Path(album): Path<String>) -> Response {
    let songs = match album_songs(&serve, album).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let Some(owner) = serve.current_user() else { return axum::Json(json!({ "track": null })).into_response() };
    match serve.userdata.bookmarks(&owner) {
        // Newest first: the first of this album's is where it was left.
        Ok(marks) => match marks.into_iter().find(|b| songs.contains(&b.item)) {
            Some(b) => axum::Json(json!({ "track": b.item, "position_secs": b.position_ms as f64 / 1000.0 })).into_response(),
            None => axum::Json(json!({ "track": null })).into_response(),
        },
        Err(e) => failed(e),
    }
}

/// The album's resume point gone: its songs' bookmarks deleted.
async fn forget_resume(State(serve): State<Arc<Serve>>, Path(album): Path<String>) -> Response {
    let songs = match album_songs(&serve, album).await {
        Ok(s) => s,
        Err(r) => return r,
    };
    let Some(owner) = serve.current_user() else { return StatusCode::NO_CONTENT.into_response() };
    for song in &songs {
        if let Err(e) = serve.userdata.delete_bookmark(&owner, song) {
            return failed(e);
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

/// An album's name and the id its tags are kept under; `None` when it is not in the library.
async fn tag_key(serve: &Arc<Serve>, id: &str) -> Result<Option<(String, String)>, Response> {
    let s = serve.clone();
    let id = id.to_string();
    blocking(move || {
        let Some(album) = s.library.albums_by_id(std::slice::from_ref(&id))?.pop() else { return Ok(None) };
        Ok(Some((album.name, id)))
    })
    .await
}

async fn add_tag(State(serve): State<Arc<Serve>>, Path((id, tag)): Path<(String, String)>) -> Response {
    if crate::userdata::tags::clean(&tag).is_none() {
        return fail(StatusCode::BAD_REQUEST, "an empty tag");
    }
    match tag_key(&serve, &id).await {
        Ok(Some((name, key))) => match serve.userdata.add_tag(&key, &tag) {
            Ok(added) => axum::Json(json!({ "added": added, "name": name })).into_response(),
            Err(e) => failed(e),
        },
        Ok(None) => fail(StatusCode::NOT_FOUND, "no such album"),
        Err(r) => r,
    }
}

async fn remove_tag(State(serve): State<Arc<Serve>>, Path((id, tag)): Path<(String, String)>) -> Response {
    match tag_key(&serve, &id).await {
        Ok(Some((name, key))) => match serve.userdata.remove_tag(&key, &tag) {
            Ok(removed) => axum::Json(json!({ "removed": removed, "name": name })).into_response(),
            Err(e) => failed(e),
        },
        Ok(None) => fail(StatusCode::NOT_FOUND, "no such album"),
        Err(r) => r,
    }
}

async fn clear_tags(State(serve): State<Arc<Serve>>, Path(id): Path<String>) -> Response {
    match tag_key(&serve, &id).await {
        Ok(Some((name, key))) => match serve.userdata.clear_tags(&key) {
            Ok(removed) => axum::Json(json!({ "removed": removed, "name": name })).into_response(),
            Err(e) => failed(e),
        },
        Ok(None) => fail(StatusCode::NOT_FOUND, "no such album"),
        Err(r) => r,
    }
}

async fn all_tags(State(serve): State<Arc<Serve>>) -> Response {
    match serve.userdata.all_tags() {
        Ok(tags) => {
            let tags: Vec<_> = tags.into_iter().map(|(tag, albums)| json!({ "tag": tag, "albums": albums })).collect();
            axum::Json(json!({ "tags": tags })).into_response()
        }
        Err(e) => failed(e),
    }
}

/// The albums carrying a tag, most recently tagged first.
async fn tagged(State(serve): State<Arc<Serve>>, Path(tag): Path<String>) -> Response {
    let s = serve.clone();
    let found = blocking(move || {
        s.library.albums_by_id(&s.userdata.albums_tagged(&tag)?)
    })
    .await;
    match found {
        Ok(albums) => {
            let albums: Vec<_> = albums.into_iter().map(|a| json!({ "id": a.id, "name": a.name, "album_artist": a.artist })).collect();
            axum::Json(json!({ "albums": albums })).into_response()
        }
        Err(r) => r,
    }
}

async fn set_favourite(serve: Arc<Serve>, track: String, on: bool) -> Response {
    let Some(owner) = serve.current_user() else { return fail(StatusCode::CONFLICT, "there is no account yet") };
    let s = serve.clone();
    let id = track.clone();
    match blocking(move || Ok(!s.library.songs_by_id(std::slice::from_ref(&id))?.is_empty())).await {
        Ok(true) => match serve.userdata.set_starred(&owner, Kind::Song, std::slice::from_ref(&track), on) {
            Ok(()) => {
                // Made now (decision 0087): an older heart a page's outbox sends later does not undo it.
                let (item, now) = (crate::userdata::offline::item_id("track", crate::source::cratarr::KIND.id, &track), now_ms());
                let op = crate::userdata::offline::Op::of(on);
                if let Err(e) = serve.userdata.mark_changed(&owner, crate::userdata::offline::ChangeKind::Favourite, &item, op, now) {
                    tracing::warn!("the change of {item} not kept: {e:#}");
                }
                StatusCode::NO_CONTENT.into_response()
            }
            Err(e) => failed(e),
        },
        Ok(false) => fail(StatusCode::NOT_FOUND, "no such track"),
        Err(r) => r,
    }
}

/// A track's facts, for a caller that names what plays ("what year is this?"): title, artists, album, year, genres,
/// and whether it is a favourite of the caller's.
async fn track(State(serve): State<Arc<Serve>>, Path(track): Path<String>) -> Response {
    let s = serve.clone();
    // Read here: the blocking task does not carry the request's account.
    let me = serve.current_user();
    let found = blocking(move || {
        let Some(song) = s.library.songs_by_id(std::slice::from_ref(&track))?.pop() else { return Ok(None) };
        let favourite = match me {
            Some(owner) => s.userdata.get(&owner, Kind::Song, std::slice::from_ref(&song.id))?.get(&song.id).is_some_and(|a| a.starred_at.is_some()),
            None => false,
        };
        Ok(Some((song, favourite)))
    })
    .await;
    match found {
        Ok(Some((t, favourite))) => axum::Json(json!({
            "id": t.id, "title": t.title, "artist": t.display_artist, "album": t.album, "album_id": t.album_id,
            "album_artist": t.display_album_artist, "year": t.year, "genres": t.genres.iter().map(|g| &g.name).collect::<Vec<_>>(),
            "favourite": favourite,
        }))
        .into_response(),
        Ok(None) => fail(StatusCode::NOT_FOUND, "no such track"),
        Err(r) => r,
    }
}

async fn favourite(State(serve): State<Arc<Serve>>, Path(track): Path<String>) -> Response {
    set_favourite(serve, track, true).await
}

async fn unfavourite(State(serve): State<Arc<Serve>>, Path(track): Path<String>) -> Response {
    set_favourite(serve, track, false).await
}

/// A song's original file with byte ranges, for players that play Cratarr's own ids.
async fn file(State(serve): State<Arc<Serve>>, Path(track): Path<String>, headers: HeaderMap) -> Response {
    match serve.song_path(&track, serve.current_user().as_deref()).await {
        Ok(Some(path)) => super::files::serve_file(&path, &headers, false).await,
        Ok(None) => fail(StatusCode::NOT_FOUND, "no such track"),
        Err(e) => failed(e),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::{Catalogue, Rules, ServedRow};
    use crate::serve::library::FolderRows;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn server() -> Arc<Serve> {
        let cat = Arc::new(Catalogue::open(None, vec!["/music".into()]).unwrap());
        let serve = Serve::open(None, Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(cat);
        let row = |n: i64| ServedRow {
            rel: format!("A/X (2001)/0{n} t.flac"),
            track: Some(n),
            born: Some(1),
            size: 10,
            mtime: 1,
            tags: vec![
                ("title".into(), format!("t{n}")),
                ("artist".into(), "A".into()),
                ("album".into(), "X".into()),
                ("date".into(), "2001".into()),
                ("genre".into(), "Dub".into()),
            ],
            ..ServedRow::default()
        };
        serve.library.refresh(&[FolderRows { folder: 0, root: "/music".into(), rows: vec![row(1), row(2)] }], &Rules::default()).unwrap();
        serve.users.set("owner", "pw").unwrap();
        serve.sessions.set_required(false).unwrap();
        serve
    }

    async fn call(serve: &Arc<Serve>, method: &str, path: &str) -> (StatusCode, serde_json::Value) {
        let r = router(serve.clone()).oneshot(Request::builder().method(method).uri(path).body(Body::empty()).unwrap()).await.unwrap();
        let status = r.status();
        let body = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null))
    }

    /// A sign-in of `user`, as the header a browser sends.
    fn session(serve: &Arc<Serve>, user: &str) -> (&'static str, String) {
        let (token, _) = serve.sessions.create(user, "d", crate::serve::now()).unwrap();
        ("cookie", format!("{}={token}", crate::serve::signin::COOKIE))
    }

    /// An API key of `user`'s, as the header a client sends.
    fn key(serve: &Arc<Serve>, user: &str, scope: &str) -> (&'static str, String) {
        let (key, _) = serve.sessions.new_key("a client", Some(user), scope, None, crate::serve::now()).unwrap();
        ("authorization", format!("Bearer {key}"))
    }

    /// `path` asked that way through the access check, as `main` has it.
    async fn asked(serve: &Arc<Serve>, by: &(&'static str, String), method: &str, path: &str) -> StatusCode {
        let app = router(serve.clone()).layer(axum::middleware::from_fn_with_state(serve.clone(), crate::serve::access::require));
        app.oneshot(Request::builder().method(method).uri(path).header(by.0, by.1.as_str()).body(Body::empty()).unwrap()).await.unwrap().status()
    }

    #[tokio::test]
    async fn only_an_administrator_or_a_key_of_theirs_reviews_an_album() {
        let s = server();
        s.sessions.set_required(true).unwrap();
        s.users.from_directory("ann", false, None).unwrap();
        let id = s.library.song_ids(1).unwrap().unwrap().album_id;
        for (method, path) in [("POST", format!("/api/library/albums/{id}/promote")), ("POST", format!("/api/library/albums/{id}/reject")), ("DELETE", format!("/api/library/albums/{id}/review"))] {
            assert_eq!(asked(&s, &session(&s, "ann"), method, &path).await, StatusCode::FORBIDDEN, "{path}: an account");
            assert_eq!(asked(&s, &key(&s, "ann", "control"), method, &path).await, StatusCode::FORBIDDEN, "{path}: that account's key");
            assert_eq!(asked(&s, &key(&s, "owner", "read"), method, &path).await, StatusCode::FORBIDDEN, "{path}: a key that only reads");
            // Past the fence: this album is in the library, not in Incoming.
            assert_eq!(asked(&s, &session(&s, "owner"), method, &path).await, StatusCode::CONFLICT, "{path}: an administrator");
            assert_eq!(asked(&s, &key(&s, "owner", "control"), method, &path).await, StatusCode::CONFLICT, "{path}: Home Assistant's key");
        }
        assert_eq!(asked(&s, &session(&s, "ann"), "GET", "/api/library/incoming").await, StatusCode::OK, "what is coming in is everyone's to see");
        // Signing in off: the one user.
        s.sessions.set_required(false).unwrap();
        assert_eq!(call(&s, "POST", &format!("/api/library/albums/{id}/reject")).await.0, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn an_album_s_tags_are_an_administrator_s_and_a_heart_is_one_s_own() {
        let s = server();
        s.sessions.set_required(true).unwrap();
        s.users.from_directory("ann", false, None).unwrap();
        let id = s.library.song_ids(1).unwrap().unwrap().album_id;
        let (one, all) = (format!("/api/library/albums/{id}/tags/gig"), format!("/api/library/albums/{id}/tags"));
        let ann = session(&s, "ann");
        let ha = key(&s, "owner", "control");
        assert_eq!(asked(&s, &ha, "POST", &one).await, StatusCode::OK, "Home Assistant's key");
        for (method, path) in [("POST", &one), ("DELETE", &one), ("DELETE", &all)] {
            assert_eq!(asked(&s, &ann, method, path).await, StatusCode::FORBIDDEN, "{method} {path}: an account");
            assert_eq!(asked(&s, &key(&s, "ann", "control"), method, path).await, StatusCode::FORBIDDEN, "{method} {path}: that account's key");
            assert_eq!(asked(&s, &key(&s, "owner", "read"), method, path).await, StatusCode::FORBIDDEN, "{method} {path}: a key that only reads");
        }
        assert_eq!(s.userdata.all_tags().unwrap(), [("gig".to_string(), 1)], "left as it was");
        assert_eq!(asked(&s, &ann, "GET", "/api/library/tags").await, StatusCode::OK, "everyone reads them");
        assert_eq!(asked(&s, &session(&s, "owner"), "DELETE", &one).await, StatusCode::OK, "an administrator");

        // What is an account's own stays its own to change, and it is ann's that changes.
        assert_eq!(asked(&s, &ann, "POST", "/api/library/tracks/tr-1/favourite").await, StatusCode::NO_CONTENT);
        assert_eq!((s.userdata.starred("ann", Kind::Song).unwrap(), s.userdata.starred("owner", Kind::Song).unwrap()), (vec!["tr-1".to_string()], Vec::new()));
        assert_eq!(asked(&s, &ann, "DELETE", "/api/library/tracks/tr-1/favourite").await, StatusCode::NO_CONTENT);
        s.userdata.set_bookmark("ann", "tr-2", None, 1_000, "", None).unwrap();
        assert_eq!(asked(&s, &ann, "DELETE", &format!("/api/library/albums/{id}/resume")).await, StatusCode::NO_CONTENT);
        assert!(s.userdata.bookmarks("ann").unwrap().is_empty());
    }

    #[tokio::test]
    async fn home_assistant_tags_an_album_and_marks_a_favourite() {
        let s = server();
        let (status, health) = call(&s, "GET", "/api/library/health").await;
        assert_eq!((status, health["songs"].as_u64(), health["albums"].as_u64()), (StatusCode::OK, Some(2), Some(1)));
        let album = s.library.song_ids(1).unwrap().unwrap();
        assert_eq!((album.song_id.as_str(), album.album.as_str(), album.album_artist.as_str()), ("tr-1", "X", "A"));
        let id = album.album_id;
        let (status, a) = call(&s, "GET", &format!("/api/library/albums/{id}")).await;
        assert_eq!((status, a["name"].as_str(), a["tags"].as_array().map(Vec::len), a["review"].is_null()), (StatusCode::OK, Some("X"), Some(0), true));
        let (_, added) = call(&s, "POST", &format!("/api/library/albums/{id}/tags/Gig")).await;
        assert_eq!(added, serde_json::json!({ "added": true, "name": "X" }));
        let (_, again) = call(&s, "POST", &format!("/api/library/albums/{id}/tags/gig")).await;
        assert_eq!(again["added"], false);
        call(&s, "POST", &format!("/api/library/albums/{id}/tags/summer")).await;
        let (_, tagged) = call(&s, "GET", "/api/library/tags/GIG").await;
        assert_eq!(tagged["albums"][0]["id"], id.as_str());
        let (_, removed) = call(&s, "DELETE", &format!("/api/library/albums/{id}/tags/gig")).await;
        assert_eq!(removed["removed"], true);
        let (_, cleared) = call(&s, "DELETE", &format!("/api/library/albums/{id}/tags")).await;
        assert_eq!(cleared, serde_json::json!({ "removed": ["summer"], "name": "X" }));
        assert_eq!(call(&s, "POST", "/api/library/albums/al-nothing/tags/gig").await.0, StatusCode::NOT_FOUND);
        assert_eq!(call(&s, "POST", "/api/library/tracks/tr-1/favourite").await.0, StatusCode::NO_CONTENT);
        assert_eq!(call(&s, "POST", "/api/library/tracks/tr-1/favourite").await.0, StatusCode::NO_CONTENT, "safe to repeat");
        assert_eq!(s.userdata.starred("owner", Kind::Song).unwrap(), ["tr-1"], "the same store as Subsonic's star");
        assert_eq!(call(&s, "DELETE", "/api/library/tracks/tr-1/favourite").await.0, StatusCode::NO_CONTENT);
        assert!(s.userdata.starred("owner", Kind::Song).unwrap().is_empty());
        assert_eq!(call(&s, "POST", "/api/library/tracks/tr-9/favourite").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn home_assistant_reads_what_plays() {
        let s = server();
        call(&s, "POST", "/api/library/tracks/tr-2/favourite").await;
        let (status, t) = call(&s, "GET", "/api/library/tracks/tr-2").await;
        assert_eq!(status, StatusCode::OK);
        let album = s.library.song_ids(2).unwrap().unwrap().album_id;
        assert_eq!(t, serde_json::json!({ "id": "tr-2", "title": "t2", "artist": "A", "album": "X", "album_id": album,
            "album_artist": "A", "year": 2001, "genres": ["Dub"], "favourite": true }));
        assert_eq!(call(&s, "GET", "/api/library/tracks/tr-1").await.1["favourite"], false);
        assert_eq!(call(&s, "GET", "/api/library/tracks/tr-9").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn an_album_resumes_where_it_was_left() {
        let s = server();
        let id = s.library.song_ids(1).unwrap().unwrap().album_id;
        assert_eq!(call(&s, "GET", &format!("/api/library/albums/{id}/resume")).await.1, serde_json::json!({ "track": null }));
        s.userdata.set_bookmark("owner", "tr-2", None, 103_500, "", Some("Symfonium")).unwrap();
        let (status, r) = call(&s, "GET", &format!("/api/library/albums/{id}/resume")).await;
        assert_eq!((status, r), (StatusCode::OK, serde_json::json!({ "track": "tr-2", "position_secs": 103.5 })));
        assert_eq!(call(&s, "DELETE", &format!("/api/library/albums/{id}/resume")).await.0, StatusCode::NO_CONTENT);
        assert_eq!(call(&s, "GET", &format!("/api/library/albums/{id}/resume")).await.1["track"], serde_json::Value::Null);
        assert_eq!(call(&s, "GET", "/api/library/albums/al-nothing/resume").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn only_incoming_albums_are_reviewed() {
        let s = server();
        let id = s.library.song_ids(1).unwrap().unwrap().album_id;
        let (_, a) = call(&s, "GET", &format!("/api/library/albums/{id}")).await;
        assert!(a["review"].is_null(), "an album of the library has no review");
        let (status, refused) = call(&s, "POST", &format!("/api/library/albums/{id}/reject")).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(refused["error"], "X is in the library, not in Incoming");
        assert_eq!(call(&s, "POST", "/api/library/albums/al-nothing/promote").await.0, StatusCode::NOT_FOUND);
        assert_eq!(call(&s, "DELETE", &format!("/api/library/albums/{id}/review")).await.0, StatusCode::CONFLICT);
        let (status, list) = call(&s, "GET", "/api/library/incoming?review=unreviewed").await;
        assert_eq!((status, list["albums"].as_array().map(Vec::len)), (StatusCode::OK, Some(0)));
    }
}
