//! The `/rest` endpoints (decision 0030). Requests are read from the query string and, for POST, a form body (Navidrome
//! treats form fields as query parameters, `server/subsonic/middlewares.go`); `getAlbum.view` is `getAlbum`. Every
//! answer is the JSON envelope with HTTP 200, errors included; endpoints not implemented answer 501, as Navidrome's.
//! XML (Subsonic's default when `f` is missing) is not offered yet: the clients in view ask for JSON.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::{Value, json};

use super::Serve;
use super::api::{self, Album, Artist, Song};
use super::read::AlbumOrder;
use super::users::User;
use crate::userdata::offline::{ChangeKind, Op, item_id};
use crate::userdata::{Annotation, Kind};

/// A request's parameters; a name may repeat (`id=1&id=2`).
pub struct Params(Vec<(String, String)>);

impl Params {
    pub fn parse(query: Option<&str>, body: &[u8]) -> Self {
        let mut pairs: Vec<(String, String)> = query.map(|q| serde_urlencoded::from_str(q).unwrap_or_default()).unwrap_or_default();
        if !body.is_empty()
            && let Ok(more) = serde_urlencoded::from_bytes::<Vec<(String, String)>>(body)
        {
            pairs.extend(more);
        }
        Self(pairs)
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    pub fn all(&self, name: &str) -> Vec<String> {
        self.0.iter().filter(|(n, _)| n == name).map(|(_, v)| v.clone()).collect()
    }

    fn number(&self, name: &str) -> Option<i64> {
        self.get(name).and_then(|v| v.trim().parse().ok())
    }

    fn size(&self, name: &str, default: usize, max: usize) -> usize {
        self.number(name).map_or(default, |n| (n.max(0) as usize).min(max))
    }
}

/// What a handler answers: a JSON body, or bytes (a stream, a cover).
pub enum Answer {
    Json(Value),
    Raw(Response),
}

/// A handler's failure, as a Subsonic error code and message.
pub struct Failed(pub(super) u32, pub(super) String);

impl<E: std::fmt::Display> From<E> for Failed {
    fn from(e: E) -> Self {
        Failed(0, format!("{e:#}"))
    }
}

pub(super) fn missing(name: &str) -> Failed {
    Failed(10, format!("Required parameter is missing: {name}"))
}

/// What the account's permissions do not let its apps do (decision 0051): "not authorized for the given operation".
fn not_permitted(what: &str) -> Failed {
    Failed(50, format!("This account may not {what}."))
}

pub(super) fn not_found(what: &str) -> Failed {
    Failed(70, format!("{what} not found"))
}

pub fn router(serve: Arc<Serve>) -> Router {
    Router::new()
        .route("/rest/{method}", get(rest).post(rest))
        .route("/api/subsonic/password", axum::routing::post(change_password))
        // The administrators' (decision 0051): the accounts, and the import, which reads another server's users'
        // listening into an account.
        .merge(
            Router::new()
                .route("/api/subsonic/accounts", get(accounts).post(create_account))
                .route("/api/subsonic/import", get(super::import::status).post(super::import::start).delete(super::import::undo))
                .route("/api/subsonic/import/sources", get(super::import::sources))
                .layer(axum::middleware::from_fn(super::access::admin)),
        )
        .with_state(serve.clone())
        .merge(super::auth_check::router(serve.clone()))
        .merge(super::signin::router(serve.clone()))
        .merge(super::access::router(serve.clone()))
        .merge(super::accounts::router(serve.clone()))
        .merge(super::ldap::router(serve.clone()))
        .merge(super::library_api::router(serve))
}

/// The Settings page's Subsonic section: the accounts, and creating the first one (decision 0030).
async fn accounts(State(serve): State<Arc<Serve>>) -> Response {
    axum::Json(json!({ "accounts": serve.users.names() })).into_response()
}

#[derive(serde::Deserialize)]
struct NewAccount {
    username: String,
    password: String,
}

async fn create_account(State(serve): State<Arc<Serve>>, axum::Json(a): axum::Json<NewAccount>) -> Response {
    match serve.users.create_first(&a.username, &a.password) {
        Ok(()) => axum::Json(json!({ "accounts": serve.users.names() })).into_response(),
        Err(why) => (StatusCode::CONFLICT, axum::Json(json!({ "error": why }))).into_response(),
    }
}

#[derive(serde::Deserialize)]
struct NewPassword {
    username: String,
    current: String,
    password: String,
}

/// One's own account's (an administrator's: any, knowing its password).
async fn change_password(State(serve): State<Arc<Serve>>, headers: HeaderMap, axum::Json(p): axum::Json<NewPassword>) -> Response {
    if !super::access::administering() && serve.current_user().as_deref() != Some(p.username.as_str()) {
        return (StatusCode::FORBIDDEN, axum::Json(json!({ "error": "Only your own account's." }))).into_response();
    }
    match serve.users.change_password(&p.username, &p.current, &p.password) {
        Ok(()) => {
            // The account's other signed-in devices sign in again with the new password (decision 0039).
            let keep = super::signin::session_of(&serve, &headers).map(|s| s.id);
            if let Err(e) = serve.sessions.revoke_user(&p.username, keep.as_deref()) {
                tracing::warn!("sessions: {}'s not ended after the new password: {e:#}", p.username);
            }
            axum::Json(json!({ "accounts": serve.users.names() })).into_response()
        }
        Err(why) => (StatusCode::BAD_REQUEST, axum::Json(json!({ "error": why }))).into_response(),
    }
}

async fn rest(
    State(serve): State<Arc<Serve>>,
    Path(method): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    peer: Option<axum::Extension<axum::extract::ConnectInfo<std::net::SocketAddr>>>,
    body: Bytes,
) -> Response {
    let params = Params::parse(query.as_deref(), if is_form(&headers) { &body } else { &[] });
    let method = method.strip_suffix(".view").unwrap_or(&method).to_string();
    // Repeated wrong passwords hold the account and the client back a while (decision 0051), as the sign-in page does.
    let client = super::signin::client(&headers, peer.map(|p| p.0.0));
    let keys = [format!("client {client}"), format!("account {}", params.get("u").unwrap_or("").to_lowercase())];
    if let Some(wait) = serve.throttle.wait(&keys, super::now()) {
        let message = format!("Too many tries. Try again in {} min.", (wait + 59) / 60);
        return json_response(api::error(40, &message));
    }
    let user = match serve.users.check(params.get("u"), params.get("t"), params.get("s"), params.get("p")) {
        Ok(u) => {
            serve.throttle.succeeded(&keys);
            u
        }
        Err(refused) => {
            // A Subsonic app with a wrong or missing login (decision 0045): the app's own name (`c`), never what it sent.
            let (user, app) = (params.get("u").unwrap_or(""), params.get("c").unwrap_or(""));
            if refused == super::users::Refused::Wrong {
                serve.throttle.failed(&keys, super::now());
            }
            tracing::warn!(target: "subsonic", client, path = %format!("/rest/{method}"), user, app, code = refused.code(), reason = refused.message(), "login refused");
            return json_response(api::error(refused.code(), refused.message()));
        }
    };
    // As its account (decision 0051): its own players, favourites and plays. An app controls; it administers nothing.
    let who = super::who::Who { user: user.name.clone(), admin: user.admin, scope: super::who::Scope::Control, home: super::proxy::at_home(&client) };
    match super::who::acting_as(who, handle(&serve, &user, &method, &params, &headers)).await {
        Ok(Answer::Json(body)) => json_response(api::ok(body)),
        Ok(Answer::Raw(r)) => r,
        Err(Failed(501, m)) => (StatusCode::NOT_IMPLEMENTED, axum::Json(api::error(0, &m))).into_response(),
        Err(Failed(code, message)) => json_response(api::error(code, &message)),
    }
}

fn is_form(headers: &HeaderMap) -> bool {
    headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|c| c.starts_with("application/x-www-form-urlencoded"))
}

fn json_response(v: Value) -> Response {
    axum::Json(v).into_response()
}

/// Runs blocking library work off the async threads.
pub(super) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> Result<T, Failed> {
    tokio::task::spawn_blocking(f).await.map_err(|e| Failed(0, e.to_string()))?.map_err(Failed::from)
}

/// The music folder a request limits itself to (`musicFolderId`: Cratarr's folder number plus one).
fn folder_of(p: &Params) -> Option<i64> {
    p.number("musicFolderId").filter(|n| *n > 0).map(|n| n - 1)
}

pub async fn handle(serve: &Arc<Serve>, user: &User, method: &str, p: &Params, headers: &HeaderMap) -> Result<Answer, Failed> {
    let s = serve.clone();
    let me = user.name.clone();
    match method {
        "ping" => Ok(Answer::Json(json!({}))),
        "getLicense" => Ok(Answer::Json(json!({ "license": { "valid": true } }))),
        "getOpenSubsonicExtensions" => Ok(Answer::Json(json!({ "openSubsonicExtensions": [
            { "name": "formPost", "versions": [1] },
            { "name": "indexBasedQueue", "versions": [1] },
            { "name": "playbackReport", "versions": [1] },
        ] }))),
        // Its roles as its permissions say (decision 0051): keeping tracks offline, playlists, public share links.
        "getUser" => Ok(Answer::Json(json!({ "user": {
            "username": user.name, "adminRole": user.admin, "settingsRole": user.admin, "streamRole": true,
            "downloadRole": user.permissions.offline, "playlistRole": user.permissions.playlists, "coverArtRole": true,
            "commentRole": false, "podcastRole": false, "shareRole": user.permissions.links, "jukeboxRole": false,
            "uploadRole": false, "videoConversionRole": false, "scrobblingEnabled": true,
            "folder": (0..serve.catalogue().folders().len()).map(|i| i + 1).collect::<Vec<_>>(),
        } }))),
        "getMusicFolders" => {
            let folders: Vec<Value> = serve
                .catalogue()
                .folders()
                .iter()
                .enumerate()
                .map(|(i, f)| json!({ "id": i + 1, "name": f.file_name().map_or_else(|| f.display().to_string(), |n| n.to_string_lossy().to_string()) }))
                .collect();
            Ok(Answer::Json(json!({ "musicFolders": { "musicFolder": folders } })))
        }
        "getScanStatus" => Ok(Answer::Json(json!({ "scanStatus": { "scanning": serve.catalogue().doing().is_some() } }))),
        "startScan" => {
            serve.catalogue().poke();
            Ok(Answer::Json(json!({ "scanStatus": { "scanning": true } })))
        }
        "getArtists" | "getIndexes" => {
            // getIndexes: nothing new since the client's copy (Navidrome's `ifModifiedSince`, `server/subsonic/browsing.go`).
            let built = serve.built_at();
            if method == "getIndexes" && p.number("ifModifiedSince").is_some_and(|since| since >= built * 1000) {
                return Ok(Answer::Json(json!({ "indexes": { "lastModified": built * 1000, "ignoredArticles": "The A An" } })));
            }
            let mut artists = blocking(move || s.library.album_artists_index()).await?;
            annotate_artists(serve, &me, &mut artists)?;
            let mut index: Vec<(String, Vec<Artist>)> = Vec::new();
            for a in artists {
                let letter = a.sort_name.chars().next().map_or("#".to_string(), |c| if c.is_alphabetic() { c.to_uppercase().collect() } else { "#".into() });
                match index.last_mut() {
                    Some((l, list)) if *l == letter => list.push(a),
                    _ => index.push((letter, vec![a])),
                }
            }
            let index: Vec<Value> = index.into_iter().map(|(name, artist)| json!({ "name": name, "artist": artist })).collect();
            let key = if method == "getIndexes" { "indexes" } else { "artists" };
            Ok(Answer::Json(json!({ key: { "ignoredArticles": "The A An", "lastModified": built * 1000, "index": index } })))
        }
        "getArtist" => {
            let id = p.get("id").ok_or_else(|| missing("id"))?.to_string();
            let mut artist = blocking(move || s.library.artist(&id)).await?.ok_or_else(|| not_found("Artist"))?;
            if let Some(albums) = artist.album.as_mut() {
                annotate_albums(serve, &me, albums)?;
            }
            annotate_artists(serve, &me, std::slice::from_mut(&mut artist))?;
            Ok(Answer::Json(json!({ "artist": artist })))
        }
        // What online services know (biography, pictures, top songs). Navidrome asks Last.fm and Spotify when they are
        // set up and answers empty without them; Cratarr has none, so it answers so too. Similar artists come from the
        // radio's choice over the owner's own library (decision 0036).
        "getArtistInfo" | "getArtistInfo2" => {
            let id = p.get("id").ok_or_else(|| missing("id"))?.to_string();
            let count = p.size("count", 20, 100);
            // An unknown artist answers so before the radio's scorer runs.
            let (lib, wanted) = (s.library.clone(), id.clone());
            let artist = blocking(move || lib.artist(&wanted)).await?.ok_or_else(|| not_found("Artist"))?;
            let similar = crate::app::radio::similar_artists(&id, count).await.unwrap_or_else(|e| {
                tracing::warn!(target: "subsonic", id = %id, reason = %e, "similar artists failed");
                Vec::new()
            });
            let lib = s.library.clone();
            let mut found = blocking(move || {
                let mut out = Vec::new();
                for a in &similar {
                    out.extend(lib.artist(a)?);
                }
                Ok(out)
            })
            .await?;
            for a in found.iter_mut() {
                a.album = None;
            }
            annotate_artists(serve, &me, &mut found)?;
            let mut info = json!({ "similarArtist": found });
            if let Some(mbid) = artist.music_brainz_id {
                info["musicBrainzId"] = mbid.into();
            }
            let mut out = json!({});
            out[if method == "getArtistInfo" { "artistInfo" } else { "artistInfo2" }] = info;
            Ok(Answer::Json(out))
        }
        "getAlbumInfo" | "getAlbumInfo2" => {
            let id = p.get("id").ok_or_else(|| missing("id"))?.to_string();
            let album = blocking(move || s.library.album(&id)).await?.ok_or_else(|| not_found("Album"))?;
            let mut info = json!({});
            if let Some(mbid) = album.music_brainz_id {
                info["musicBrainzId"] = mbid.into();
            }
            Ok(Answer::Json(json!({ "albumInfo": info })))
        }
        "getTopSongs" => Ok(Answer::Json(json!({ "topSongs": { "song": [] } }))),
        // Songs like a song, an album or an artist: the radio's Similar over the owner's own library (decision 0036).
        "getSimilarSongs" | "getSimilarSongs2" => {
            let id = p.get("id").ok_or_else(|| missing("id"))?.to_string();
            let count = p.size("count", 50, 50);
            let ids = crate::app::radio::similar_songs(&id, count).await.unwrap_or_else(|e| {
                tracing::warn!(target: "subsonic", id = %id, reason = %e, "similar songs failed");
                Vec::new()
            });
            let mut songs = blocking(move || s.library.songs_by_id(&ids)).await?;
            annotate_songs(serve, &me, &mut songs)?;
            let mut out = json!({});
            out[if method == "getSimilarSongs" { "similarSongs" } else { "similarSongs2" }] = json!({ "song": songs });
            Ok(Answer::Json(out))
        }
        "getAlbum" => {
            let id = p.get("id").ok_or_else(|| missing("id"))?.to_string();
            let mut album = blocking(move || s.library.album(&id)).await?.ok_or_else(|| not_found("Album"))?;
            if let Some(songs) = album.song.as_mut() {
                annotate_songs(serve, &me, songs)?;
            }
            annotate_albums(serve, &me, std::slice::from_mut(&mut album))?;
            Ok(Answer::Json(json!({ "album": album })))
        }
        "getSong" => {
            let id = p.get("id").ok_or_else(|| missing("id"))?.to_string();
            let mut songs = blocking(move || s.library.songs_by_id(&[id])).await?;
            annotate_songs(serve, &me, &mut songs)?;
            let song = songs.pop().ok_or_else(|| not_found("Song"))?;
            Ok(Answer::Json(json!({ "song": song })))
        }
        "getGenres" => {
            let genres = blocking(move || s.library.genres()).await?;
            let genres: Vec<Value> = genres.into_iter().map(|(name, songs, albums)| json!({ "value": name, "songCount": songs, "albumCount": albums })).collect();
            Ok(Answer::Json(json!({ "genres": { "genre": genres } })))
        }
        "getAlbumList2" | "getAlbumList" => {
            let kind = p.get("type").ok_or_else(|| missing("type"))?.to_string();
            let size = p.size("size", 10, 500);
            let offset = p.size("offset", 0, usize::MAX);
            let folder = folder_of(p);
            let mut albums = match kind.as_str() {
                "starred" | "frequent" | "recent" | "highest" => {
                    let ids = serve.userdata.album_list(&me, &kind, size, offset)?;
                    blocking(move || s.library.albums_by_id(&ids)).await?
                }
                other => {
                    let order = match other {
                        "newest" => AlbumOrder::Newest,
                        "alphabeticalByName" => AlbumOrder::ByName,
                        "alphabeticalByArtist" => AlbumOrder::ByArtist,
                        "random" => AlbumOrder::Random,
                        "byYear" => AlbumOrder::ByYear {
                            from: p.number("fromYear").ok_or_else(|| missing("fromYear"))?.max(0) as u32,
                            to: p.number("toYear").ok_or_else(|| missing("toYear"))?.max(0) as u32,
                        },
                        "byGenre" => AlbumOrder::ByGenre(p.get("genre").ok_or_else(|| missing("genre"))?.to_string()),
                        _ => return Err(Failed(0, format!("Unknown list type: {other}"))),
                    };
                    blocking(move || s.library.album_list(&order, size, offset, folder)).await?
                }
            };
            annotate_albums(serve, &me, &mut albums)?;
            if method == "getAlbumList" {
                // The folder-based form: each album as a directory.
                let albums: Vec<Value> = albums.iter().map(album_as_directory).collect();
                return Ok(Answer::Json(json!({ "albumList": { "album": albums } })));
            }
            Ok(Answer::Json(json!({ "albumList2": { "album": albums } })))
        }
        "getRandomSongs" => {
            let size = p.size("size", 10, 500);
            let genre = p.get("genre").map(str::to_string);
            let years = (p.number("fromYear").map(|y| y.max(0) as u32), p.number("toYear").map(|y| y.max(0) as u32));
            let folder = folder_of(p);
            // Never what the listener excluded from automatic picks (Cratarr's decision 0024, amended 2026-10-19).
            let excluded = match serve.playlists() {
                Some(pl) => pl.excluded_songs_of(Some(me.as_str())).await.unwrap_or_default(),
                None => Default::default(),
            };
            let more = excluded.len();
            let mut songs = blocking(move || s.library.random_songs(size + more, genre.as_deref(), years, folder)).await?;
            songs.retain(|x| !excluded.contains(&x.id));
            songs.truncate(size);
            annotate_songs(serve, &me, &mut songs)?;
            Ok(Answer::Json(json!({ "randomSongs": { "song": songs } })))
        }
        "getSongsByGenre" => {
            let genre = p.get("genre").ok_or_else(|| missing("genre"))?.to_string();
            let size = p.size("count", 10, 500);
            let offset = p.size("offset", 0, usize::MAX);
            let folder = folder_of(p);
            let mut songs = blocking(move || s.library.songs_by_genre(&genre, size, offset, folder)).await?;
            annotate_songs(serve, &me, &mut songs)?;
            Ok(Answer::Json(json!({ "songsByGenre": { "song": songs } })))
        }
        "getStarred2" | "getStarred" => {
            let (songs, albums, artists) =
                (serve.userdata.starred(&me, Kind::Song)?, serve.userdata.starred(&me, Kind::Album)?, serve.userdata.starred(&me, Kind::Artist)?);
            let (mut songs, mut albums, mut artists) = blocking(move || {
                let artists: Vec<Artist> = artists.iter().filter_map(|id| s.library.artist(id).ok().flatten()).map(|mut a| {
                    a.album = None;
                    a
                }).collect();
                Ok((s.library.songs_by_id(&songs)?, s.library.albums_by_id(&albums)?, artists))
            })
            .await?;
            annotate_songs(serve, &me, &mut songs)?;
            annotate_albums(serve, &me, &mut albums)?;
            annotate_artists(serve, &me, &mut artists)?;
            let key = if method == "getStarred2" { "starred2" } else { "starred" };
            Ok(Answer::Json(json!({ key: { "artist": artists, "album": albums, "song": songs } })))
        }
        "search3" | "search2" => {
            let query = p.get("query").unwrap_or("").to_string();
            let counts = (
                (p.size("artistCount", 20, 500), p.size("artistOffset", 0, usize::MAX)),
                (p.size("albumCount", 20, 500), p.size("albumOffset", 0, usize::MAX)),
                (p.size("songCount", 20, 500), p.size("songOffset", 0, usize::MAX)),
            );
            let folder = folder_of(p);
            let (mut artists, mut albums, mut songs) = blocking(move || s.library.search(&query, counts.0, counts.1, counts.2, folder)).await?;
            annotate_artists(serve, &me, &mut artists)?;
            annotate_albums(serve, &me, &mut albums)?;
            annotate_songs(serve, &me, &mut songs)?;
            let key = if method == "search3" { "searchResult3" } else { "searchResult2" };
            Ok(Answer::Json(json!({ key: { "artist": artists, "album": albums, "song": songs } })))
        }
        "star" | "unstar" => {
            let on = method == "star";
            let songs = p.all("id");
            let (albums, artists) = (p.all("albumId"), p.all("artistId"));
            // `id` may name an album or an artist too (the folder-based form).
            let (mut song_ids, mut album_ids, mut artist_ids) = (Vec::new(), albums, artists);
            for id in songs {
                match id.get(..3) {
                    Some(super::ids::ALBUM) => album_ids.push(id),
                    Some(super::ids::ARTIST) => artist_ids.push(id),
                    _ => song_ids.push(id),
                }
            }
            serve.userdata.set_starred(&me, Kind::Song, &song_ids, on)?;
            serve.userdata.set_starred(&me, Kind::Album, &album_ids, on)?;
            serve.userdata.set_starred(&me, Kind::Artist, &artist_ids, on)?;
            // Changes made now (decision 0087): an older one an app's outbox sends later does not undo them.
            let marked = song_ids.iter().map(|id| ("track", id.clone()))
                .chain(album_ids.iter().map(|id| ("album", id.clone())))
                .chain(artist_ids.iter().filter_map(|id| serve.library.artist_name(id).ok().flatten()).map(|n| ("artist", n)));
            for (what, id) in marked.collect::<Vec<_>>() {
                mark_now(serve, &me, ChangeKind::Favourite, &item_id(what, CRATARR, &id), Op::of(on));
            }
            Ok(Answer::Json(json!({})))
        }
        "setRating" => {
            let id = p.get("id").ok_or_else(|| missing("id"))?;
            let rating = p.number("rating").ok_or_else(|| missing("rating"))?.clamp(0, 5) as u8;
            let kind = match id.get(..3) {
                Some(super::ids::ALBUM) => Kind::Album,
                Some(super::ids::ARTIST) => Kind::Artist,
                _ => Kind::Song,
            };
            serve.userdata.set_rating(&me, kind, id, rating)?;
            let named = match kind {
                Kind::Album => Some(("album", id.to_string())),
                Kind::Artist => serve.library.artist_name(id).ok().flatten().map(|n| ("artist", n)),
                _ => Some(("track", id.to_string())),
            };
            if let Some((what, id)) = named {
                mark_now(serve, &me, ChangeKind::Rating, &item_id(what, CRATARR, &id), Op::of(rating > 0));
            }
            Ok(Answer::Json(json!({})))
        }
        "scrobble" => {
            // A submission counts a play; `submission=false` says what plays now (`getNowPlaying`).
            if p.get("submission").is_some_and(|v| v == "false") {
                super::queues::now_playing_from_scrobble(serve, user, p).await?;
            } else {
                let times = p.all("time");
                for (i, id) in p.all("id").into_iter().enumerate() {
                    let at = times.get(i).and_then(|t| t.parse::<i64>().ok()).map_or_else(now, |ms| ms / 1000);
                    // One way in for listening (decision 0068).
                    crate::app::listening::heard(serve, &me, &id, &crate::userdata::Heard::counted(at)).await?;
                }
            }
            Ok(Answer::Json(json!({})))
        }
        "createBookmark" => {
            let id = p.get("id").ok_or_else(|| missing("id"))?.to_string();
            let position = p.number("position").ok_or_else(|| missing("position"))?.max(0) as u64;
            let lib = serve.library.clone();
            let song = id.clone();
            let (pid, _, _) = blocking(move || lib.play_targets(&song)).await?.ok_or_else(|| not_found("Song"))?;
            serve.userdata.set_bookmark(&me, &id, Some(&pid), position, p.get("comment").unwrap_or_default(), p.get("c"))?;
            Ok(Answer::Json(json!({})))
        }
        "getBookmarks" => {
            let marks = serve.userdata.bookmarks(&me)?;
            let ids: Vec<String> = marks.iter().map(|b| b.item.clone()).collect();
            let mut songs = blocking(move || s.library.songs_by_id(&ids)).await?;
            annotate_songs(serve, &me, &mut songs)?;
            // Bookmarks of songs the library no longer has are kept, not shown (as Navidrome's join leaves them out).
            let out: Vec<Value> = marks
                .iter()
                .filter_map(|b| {
                    let song = songs.iter().find(|x| x.id == b.item)?;
                    Some(json!({ "position": b.position_ms, "username": me, "comment": b.comment, "created": api::iso(b.created),
                        "changed": api::iso(b.changed), "entry": song }))
                })
                .collect();
            Ok(Answer::Json(json!({ "bookmarks": { "bookmark": out } })))
        }
        "deleteBookmark" => {
            let id = p.get("id").ok_or_else(|| missing("id"))?;
            serve.userdata.delete_bookmark(&me, id)?;
            Ok(Answer::Json(json!({})))
        }
        "getPlayQueue" => super::queues::get(serve, user, false).await,
        "getPlayQueueByIndex" => super::queues::get(serve, user, true).await,
        "savePlayQueue" => super::queues::save(serve, user, p, false),
        "savePlayQueueByIndex" => super::queues::save(serve, user, p, true),
        "getNowPlaying" => super::queues::now_playing(serve, user).await,
        "reportPlayback" => super::queues::report_playback(serve, user, p).await,
        "download" if !user.permissions.offline => Err(not_permitted("keep tracks offline")),
        "createPlaylist" | "updatePlaylist" | "deletePlaylist" if !user.permissions.playlists => Err(not_permitted("change playlists")),
        "stream" | "download" => {
            let id = p.get("id").ok_or_else(|| missing("id"))?.to_string();
            let path = serve.song_path(&id).await?.ok_or_else(|| not_found("Song"))?;
            Ok(Answer::Raw(super::files::serve_file(&path, headers, method == "download").await))
        }
        "getCoverArt" => {
            let id = p.get("id").ok_or_else(|| missing("id"))?.to_string();
            let size = p.number("size").filter(|n| *n > 0).map(|n| n as u32);
            Ok(Answer::Raw(serve.cover(&id, size, headers).await?))
        }
        "getPlaylists" => {
            let service = playlist_service(serve)?;
            let all = service.snapshot().await.map_err(toggle_failed)?;
            let mut out = Vec::with_capacity(all.len());
            for (name, lines, _) in all {
                let lines = serve.held.shown(&held_key(&service, &name)?, lines);
                let songs = shown_songs(serve, &lines).await?;
                out.push(playlist_json(serve, &service, &name, &lines, &songs)?);
            }
            Ok(Answer::Json(json!({ "playlists": { "playlist": out } })))
        }
        "getPlaylist" => {
            let service = playlist_service(serve)?;
            let (name, lines) = playlist_named(serve, &service, p.get("id")).await?;
            let lines = serve.held.shown(&held_key(&service, &name)?, lines);
            let mut songs = shown_songs(serve, &lines).await?;
            annotate_songs(serve, &me, &mut songs)?;
            let mut body = playlist_json(serve, &service, &name, &lines, &songs)?;
            // Asked by a name it had: answered under that id, the one the client keeps (its next listing has the new).
            body["id"] = p.get("id").unwrap_or_default().into();
            body["entry"] = serde_json::to_value(&songs)?;
            Ok(Answer::Json(json!({ "playlist": body })))
        }
        "createPlaylist" => {
            let service = playlist_service(serve)?;
            let add = lines_of_songs(serve, &p.all("songId")).await?;
            let name = match p.get("playlistId") {
                // With an id: the playlist's songs become these (lines of missing files stay).
                Some(id) => {
                    let (name, lines) = playlist_named(serve, &service, Some(id)).await?;
                    // Its songs replaced: an emptying held for it comes to the same.
                    serve.held.take(&held_key(&service, &name)?);
                    let shown = shown_flags(serve, &lines).await?;
                    let all: Vec<usize> = (0..shown.iter().filter(|s| **s).count()).collect();
                    let new = super::playlists::edited(&lines, &shown, &all, &add);
                    if new != lines {
                        service.set_entries(&name, &lines, &new).await.map_err(toggle_failed)?;
                    }
                    name
                }
                None => {
                    let wanted = p.get("name").filter(|n| !n.trim().is_empty()).ok_or_else(|| missing("name"))?;
                    let taken: Vec<String> = service.snapshot().await.map_err(toggle_failed)?.into_iter().map(|(n, _, _)| n).collect();
                    let name = super::playlists::new_name(wanted, &taken);
                    service.create_with(&name, &add).await.map_err(toggle_failed)?;
                    serve.userdata.playlist_made(&held_key(&service, &name)?)?;
                    name
                }
            };
            let lines = service.entries(&name).await.map_err(toggle_failed)?;
            let mut songs = shown_songs(serve, &lines).await?;
            annotate_songs(serve, &me, &mut songs)?;
            let mut body = playlist_json(serve, &service, &name, &lines, &songs)?;
            body["entry"] = serde_json::to_value(&songs)?;
            Ok(Answer::Json(json!({ "playlist": body })))
        }
        "updatePlaylist" => {
            let service = playlist_service(serve)?;
            let (name, _) = playlist_named(serve, &service, p.get("playlistId")).await?;
            let key = held_key(&service, &name)?;
            let remove: Vec<usize> = p.all("songIndexToRemove").iter().filter_map(|i| i.trim().parse().ok()).collect();
            let add = lines_of_songs(serve, &p.all("songIdToAdd")).await?;
            let renamed = p.get("name").filter(|n| !n.trim().is_empty() && *n != name);
            if !remove.is_empty() || !add.is_empty() {
                let lines = service.entries(&name).await.map_err(toggle_failed)?;
                let emptied = serve.held.take(&key).filter(|e| e.before == lines).map(|e| e.after);
                let shown = shown_flags(serve, &lines).await?;
                match super::playlists::update_step(&lines, &shown, emptied, &remove, &add) {
                    super::playlists::Step::Hold(new) if renamed.is_none() => hold_emptying(serve, &service, &name, lines, new),
                    super::playlists::Step::Hold(new) | super::playlists::Step::Write(new) => {
                        service.set_entries(&name, &lines, &new).await.map_err(toggle_failed)?;
                    }
                    super::playlists::Step::Unchanged => {}
                }
            } else if renamed.is_some() {
                write_held(&service, &name, serve.held.take(&key)).await;
            }
            if let Some(wanted) = renamed {
                let taken: Vec<String> =
                    service.snapshot().await.map_err(toggle_failed)?.into_iter().map(|(n, _, _)| n).filter(|n| *n != name).collect();
                let new_name = super::playlists::new_name(wanted, &taken);
                if new_name != name {
                    service.rename(&name, &new_name).await.map_err(toggle_failed)?;
                }
            }
            Ok(Answer::Json(json!({})))
        }
        "deletePlaylist" => {
            let service = playlist_service(serve)?;
            let (name, _) = playlist_named(serve, &service, p.get("id")).await?;
            let key = held_key(&service, &name)?;
            serve.held.take(&key);
            service.delete(&name).await.map_err(toggle_failed)?;
            serve.userdata.playlist_gone(&key)?;
            Ok(Answer::Json(json!({})))
        }
        _ => Err(Failed(501, format!("{method} is not implemented"))),
    }
}

/// Cratarr's own library, as the change log names its items' source.
const CRATARR: &str = crate::source::cratarr::KIND.id;

/// A Subsonic app's change, made now (decision 0087): kept as the item's last change. Not kept costs only that.
fn mark_now(serve: &Serve, me: &str, kind: ChangeKind, item: &str, op: Op) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    if let Err(e) = serve.userdata.mark_changed(me, kind, item, op, now) {
        tracing::warn!("subsonic: the change of {item} not kept: {e:#}");
    }
}

fn playlist_service(serve: &Serve) -> Result<Arc<crate::app::PlaylistService>, Failed> {
    serve.playlists().cloned().ok_or_else(|| Failed(0, "Playlists are not ready yet".into()))
}

/// A playlist's key among everyone's (decision 0051: two accounts' playlists may have one name), for what is kept of
/// it here: an emptying held, when it was seen, its old names.
fn held_key(service: &crate::app::PlaylistService, id: &str) -> Result<String, Failed> {
    Ok(service.reach(id).map_err(toggle_failed)?.key())
}

/// Holds a playlist's emptying for the call that refills it (`playlists::Held`); written on its own when none comes,
/// as the account that asked.
fn hold_emptying(serve: &Arc<Serve>, service: &Arc<crate::app::PlaylistService>, name: &str, before: Vec<String>, after: Vec<String>) {
    let Ok(key) = held_key(service, name) else { return };
    let ticket = serve.held.hold(&key, before, after);
    let (serve, service, name) = (serve.clone(), service.clone(), name.to_string());
    tokio::spawn(super::who::carried(async move {
        tokio::time::sleep(super::playlists::REFILL_WAIT).await;
        write_held(&service, &name, serve.held.take_ticket(&key, ticket)).await;
    }));
}

/// Writes an emptying that was held (`None`: nothing held).
async fn write_held(service: &crate::app::PlaylistService, name: &str, held: Option<super::playlists::Emptied>) {
    if let Some(e) = held
        && let Err(err) = service.set_entries(name, &e.before, &e.after).await
    {
        tracing::warn!(target: "subsonic", playlist = %name, reason = %err, "emptied playlist not written");
    }
}

/// The playlist a client means by an id, with its lines: by its name, else by a name it had (clients keep a playlist's
/// id across a rename).
async fn playlist_named(serve: &Serve, service: &crate::app::PlaylistService, id: Option<&str>) -> Result<(String, Vec<String>), Failed> {
    let name = playlist_name(id)?;
    match service.entries(&name).await {
        Err(crate::app::ToggleError::UnknownPlaylist) => {
            let was = held_key(service, &name)?;
            let now = service.id_of_key(&serve.userdata.playlist_alias(&was)?.ok_or_else(|| not_found("Playlist"))?);
            let lines = service.entries(&now).await.map_err(toggle_failed)?;
            Ok((now, lines))
        }
        found => Ok((name, found.map_err(toggle_failed)?)),
    }
}

fn playlist_name(id: Option<&str>) -> Result<String, Failed> {
    let id = id.ok_or_else(|| missing("id"))?;
    super::playlists::name_of(id).map(str::to_string).ok_or_else(|| not_found("Playlist"))
}

fn toggle_failed(e: crate::app::ToggleError) -> Failed {
    match e {
        crate::app::ToggleError::UnknownPlaylist => not_found("Playlist"),
        // Another's playlist (decision 0051): "not authorized for the given operation".
        crate::app::ToggleError::Forbidden(m) => Failed(50, m),
        other => Failed(0, other.to_string()),
    }
}

/// Which of a playlist's lines are songs the library has (the others are not shown).
async fn shown_flags(serve: &Arc<Serve>, lines: &[String]) -> Result<Vec<bool>, Failed> {
    let cat = serve.catalogue().clone();
    let lines = lines.to_vec();
    blocking(move || Ok(lines.iter().map(|l| super::playlists::song_of_line(&cat, l).is_some()).collect())).await
}

/// A playlist's songs in order: its lines the library has.
async fn shown_songs(serve: &Arc<Serve>, lines: &[String]) -> Result<Vec<Song>, Failed> {
    let (cat, lib) = (serve.catalogue().clone(), serve.library.clone());
    let lines = lines.to_vec();
    blocking(move || {
        let ids: Vec<String> = lines.iter().filter_map(|l| super::playlists::song_of_line(&cat, l)).collect();
        lib.songs_by_id(&ids)
    })
    .await
}

/// The playlist lines of songs a client named; songs the library does not know are left out.
async fn lines_of_songs(serve: &Arc<Serve>, songs: &[String]) -> Result<Vec<String>, Failed> {
    let cat = serve.catalogue().clone();
    let songs = songs.to_vec();
    blocking(move || Ok(songs.iter().filter_map(|s| super::playlists::line_of_song(&cat, s)).collect())).await
}

/// A playlist as OpenSubsonic shows it (`id`: as the asking account knows it): whose it is, shared (`public`) or not,
/// and read-only when it is another's (decision 0051).
fn playlist_json(serve: &Serve, service: &crate::app::PlaylistService, id: &str, lines: &[String], songs: &[Song]) -> Result<Value, Failed> {
    use md5::Digest;
    let r = service.reach(id).map_err(toggle_failed)?;
    let print = format!("{:x}", md5::Md5::digest(lines.join("\n").as_bytes()));
    let (created, changed) = serve.userdata.playlist_seen(&r.key(), &print)?;
    let mut v = json!({
        "id": super::playlists::id_of(id), "name": r.name, "owner": r.owner, "public": r.shared,
        "songCount": songs.len(), "duration": songs.iter().map(|s| s.duration).sum::<u64>(),
        "created": api::iso(created), "changed": api::iso(changed), "readonly": !r.mine,
    });
    if let Some(first) = songs.first() {
        v["coverArt"] = first.cover_art.clone().into();
    }
    Ok(v)
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// An album as the folder-based API shows it (`getAlbumList`): a directory.
fn album_as_directory(a: &Album) -> Value {
    json!({ "id": a.id, "parent": a.artist_id, "isDir": true, "title": a.name, "album": a.name, "artist": a.artist,
        "year": a.year, "genre": a.genre, "coverArt": a.cover_art, "created": a.created, "starred": a.starred,
        "userRating": a.user_rating, "playCount": a.play_count, "songCount": a.song_count, "duration": a.duration })
}

fn apply(a: Option<&Annotation>, starred: &mut Option<String>, plays: &mut Option<u32>, played: &mut Option<String>, rating: &mut Option<u8>) {
    let Some(a) = a else { return };
    *starred = a.starred_at.map(api::iso);
    *plays = (a.play_count > 0).then_some(a.play_count);
    *played = a.play_date.filter(|d| *d > 0).map(api::iso);
    *rating = (a.rating > 0).then_some(a.rating);
}

pub(super) fn annotate_songs(serve: &Serve, user: &str, songs: &mut [Song]) -> Result<(), Failed> {
    let ids: Vec<String> = songs.iter().map(|s| s.id.clone()).collect();
    let found = serve.userdata.get(user, Kind::Song, &ids)?;
    for s in songs {
        apply(found.get(&s.id), &mut s.starred, &mut s.play_count, &mut s.played, &mut s.user_rating);
    }
    Ok(())
}

fn annotate_albums(serve: &Serve, user: &str, albums: &mut [Album]) -> Result<(), Failed> {
    let ids: Vec<String> = albums.iter().map(|a| a.id.clone()).collect();
    let found = serve.userdata.get(user, Kind::Album, &ids)?;
    for a in albums {
        apply(found.get(&a.id), &mut a.starred, &mut a.play_count, &mut a.played, &mut a.user_rating);
    }
    Ok(())
}

fn annotate_artists(serve: &Serve, user: &str, artists: &mut [Artist]) -> Result<(), Failed> {
    let ids: Vec<String> = artists.iter().map(|a| a.id.clone()).collect();
    let found = serve.userdata.get(user, Kind::Artist, &ids)?;
    for a in artists {
        let (mut plays, mut played) = (None, None);
        apply(found.get(&a.id), &mut a.starred, &mut plays, &mut played, &mut a.user_rating);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::{Catalogue, Rules, ServedRow};
    use crate::userdata::AppQueue;
    use crate::serve::library::FolderRows;

    fn row(track: i64, rel: &str, tags: &[(&str, &str)]) -> ServedRow {
        ServedRow {
            rel: rel.into(),
            track: Some(track),
            born: Some(1_700_000_000 + track),
            format: Some("FLAC".into()),
            kbps: Some(900),
            duration_ms: Some(200_000),
            size: 1000,
            mtime: 1,
            tags: tags.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect(),
            analysis: None,
        }
    }

    fn server() -> (Arc<Serve>, User) {
        let cat = Arc::new(Catalogue::open(None, vec!["/music".into()]).unwrap());
        let serve = Serve::open(None, Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(cat);
        let t = |n: i64, title: &str| {
            row(n, &format!("Björk/Homogenic (1997)/0{n} {title}.flac"), &[
                ("title", title),
                ("artist", "Björk"),
                ("album", "Homogenic"),
                ("tracknumber", &n.to_string()),
                ("date", "1997"),
                ("genre", "Electronic"),
            ])
        };
        serve.library.refresh(&[FolderRows { folder: 0, root: "/music".into(), rows: vec![t(1, "Hunter"), t(2, "Jóga")] }], &Rules::default()).unwrap();
        serve.users.set("me", "pw").unwrap();
        let me = serve.users.check(Some("me"), None, None, Some("pw")).unwrap();
        (serve, me)
    }

    async fn call(serve: &Arc<Serve>, me: &User, method: &str, query: &str) -> Value {
        match handle(serve, me, method, &Params::parse(Some(query), &[]), &HeaderMap::new()).await {
            Ok(Answer::Json(v)) => v,
            Ok(Answer::Raw(_)) => panic!("{method} answered bytes"),
            Err(Failed(code, m)) => json!({ "error": code, "message": m }),
        }
    }

    #[tokio::test]
    async fn a_client_browses_the_library() {
        let (s, me) = server();
        assert_eq!(call(&s, &me, "ping", "").await, json!({}));
        let folders = call(&s, &me, "getMusicFolders", "").await;
        assert_eq!(folders["musicFolders"]["musicFolder"][0], json!({ "id": 1, "name": "music" }));
        let list = call(&s, &me, "getAlbumList2", "type=newest&size=5").await;
        let album = &list["albumList2"]["album"][0];
        assert_eq!((album["name"].as_str(), album["songCount"].as_u64(), album["year"].as_u64()), (Some("Homogenic"), Some(2), Some(1997)));
        let id = album["id"].as_str().unwrap().to_string();
        let full = call(&s, &me, "getAlbum", &format!("id={id}")).await;
        let songs = full["album"]["song"].as_array().unwrap();
        assert_eq!(songs.iter().map(|x| x["title"].as_str().unwrap()).collect::<Vec<_>>(), ["Hunter", "Jóga"]);
        assert_eq!(songs[0]["contentType"], "audio/flac");
        assert_eq!(songs[0]["duration"], 200);
        let artists = call(&s, &me, "getArtists", "").await;
        assert_eq!(artists["artists"]["index"][0]["name"], "B");
        let found = call(&s, &me, "search3", "query=joga").await;
        assert_eq!(found["searchResult3"]["song"][0]["title"], "Jóga");
        let everything = call(&s, &me, "search3", "query=&songCount=500").await;
        assert_eq!(everything["searchResult3"]["song"].as_array().unwrap().len(), 2, "an empty query lists all");
        let missing = call(&s, &me, "getAlbum", "id=al-nothing").await;
        assert_eq!(missing["error"], 70);
        let unknown = call(&s, &me, "getPodcasts", "").await;
        assert_eq!(unknown["error"], 501);
    }

    #[tokio::test]
    async fn stars_plays_and_the_play_queue_are_kept() {
        let (s, me) = server();
        let song = call(&s, &me, "search3", "query=hunter").await["searchResult3"]["song"][0]["id"].as_str().unwrap().to_string();
        let album = call(&s, &me, "getAlbumList2", "type=newest").await["albumList2"]["album"][0]["id"].as_str().unwrap().to_string();
        call(&s, &me, "star", &format!("id={song}&albumId={album}")).await;
        let starred = call(&s, &me, "getStarred2", "").await;
        assert_eq!(starred["starred2"]["song"][0]["id"], song.as_str());
        assert_eq!(starred["starred2"]["album"][0]["id"], album.as_str());
        assert!(starred["starred2"]["song"][0]["starred"].is_string());
        call(&s, &me, "scrobble", &format!("id={song}&time=1792065086000")).await;
        let again = call(&s, &me, "getSong", &format!("id={song}")).await;
        assert_eq!((again["song"]["playCount"].as_u64(), again["song"]["played"].as_str()), (Some(1), Some("2026-10-15T11:51:26Z")));
        let frequent = call(&s, &me, "getAlbumList2", "type=frequent").await;
        assert_eq!(frequent["albumList2"]["album"][0]["id"], album.as_str());
        call(&s, &me, "savePlayQueue", &format!("id={song}&current={song}&position=1500&c=Feishin")).await;
        let q = call(&s, &me, "getPlayQueue", "").await;
        assert_eq!((q["playQueue"]["current"].as_str(), q["playQueue"]["position"].as_u64()), (Some(song.as_str()), Some(1500)));
        assert_eq!(q["playQueue"]["entry"][0]["id"], song.as_str());
        call(&s, &me, "unstar", &format!("id={song}")).await;
        assert!(call(&s, &me, "getStarred2", "").await["starred2"]["song"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn each_app_gets_back_its_queue_and_the_last_saved_is_given() {
        let (s, me) = server();
        let album = call(&s, &me, "getAlbumList2", "type=newest").await["albumList2"]["album"][0]["id"].as_str().unwrap().to_string();
        let songs = call(&s, &me, "getAlbum", &format!("id={album}")).await;
        let ids: Vec<String> = songs["album"]["song"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap().to_string()).collect();
        let (h, j) = (ids[0].clone(), ids[1].clone());
        let ext = call(&s, &me, "getOpenSubsonicExtensions", "").await;
        let names: Vec<&str> = ext["openSubsonicExtensions"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"indexBasedQueue") && names.contains(&"playbackReport"), "{names:?}");
        // The same song twice: by index, the second copy plays.
        assert_eq!(call(&s, &me, "savePlayQueueByIndex", &format!("id={h}&id={j}&id={h}&currentIndex=2&position=1500.7&c=Feishin")).await, json!({}));
        let q = call(&s, &me, "getPlayQueueByIndex", "").await["playQueueByIndex"].clone();
        assert_eq!((q["currentIndex"].as_u64(), q["position"].as_u64(), q["changedBy"].as_str()), (Some(2), Some(1500), Some("Feishin")));
        assert_eq!(q["entry"].as_array().unwrap().len(), 3);
        assert_eq!(q["username"], "me");
        let q = call(&s, &me, "getPlayQueue", "").await["playQueue"].clone();
        assert_eq!(q["current"], h.as_str(), "by id: the song");
        assert_eq!(call(&s, &me, "savePlayQueueByIndex", &format!("id={h}&currentIndex=1&c=Feishin")).await["error"], 10, "not a place in it");
        // Another app's save is the last: it is given, and the first app's stays its own.
        let older = AppQueue { client: "Feishin".into(), songs: vec![h.clone(), j.clone()], current: Some(1), position_ms: 9, changed: 100 };
        s.userdata.save_app_queue("me", &older).unwrap();
        // One `id` with commas (Castafiore), and a gone song left out with the place moving along.
        call(&s, &me, "savePlayQueue", &format!("id=tr-999,{j}&current={j}&position=40&c=DSub")).await;
        let q = call(&s, &me, "getPlayQueueByIndex", "").await["playQueueByIndex"].clone();
        assert_eq!((q["changedBy"].as_str(), q["currentIndex"].as_u64(), q["entry"][0]["id"].as_str()), (Some("DSub"), Some(0), Some(j.as_str())));
        assert_eq!(s.userdata.app_queues("me").unwrap().iter().find(|q| q.client == "Feishin"), Some(&older));
        // A save without songs clears that app's; then the other is given.
        assert_eq!(call(&s, &me, "savePlayQueue", "c=DSub").await, json!({}));
        assert_eq!(call(&s, &me, "getPlayQueue", "").await["playQueue"]["changedBy"], "Feishin");
        assert_eq!(call(&s, &me, "savePlayQueue", "c=Feishin").await, json!({}));
        assert_eq!(call(&s, &me, "getPlayQueue", "").await, json!({}), "nothing saved");
    }

    #[tokio::test]
    async fn apps_reports_show_what_plays_and_a_stop_late_enough_counts() {
        let (s, me) = server();
        let song = call(&s, &me, "search3", "query=hunter").await["searchResult3"]["song"][0]["id"].as_str().unwrap().to_string();
        let plays = |v: &Value| v["song"]["playCount"].as_u64().unwrap_or(0);
        assert_eq!(call(&s, &me, "getNowPlaying", "").await, json!({ "nowPlaying": { "entry": [] } }));
        let report = |state: &str, at: u64, more: &str| format!("mediaId={song}&mediaType=song&positionMs={at}&state={state}&c=Feishin{more}");
        assert_eq!(call(&s, &me, "reportPlayback", &report("starting", 0, "")).await, json!({}));
        call(&s, &me, "reportPlayback", &report("playing", 30_000, "&playbackRate=1.0")).await;
        let now = call(&s, &me, "getNowPlaying", "").await["nowPlaying"]["entry"][0].clone();
        assert_eq!((now["id"].as_str(), now["playerName"].as_str(), now["state"].as_str()), (Some(song.as_str()), Some("Feishin"), Some("playing")));
        assert_eq!((now["username"].as_str(), now["minutesAgo"].as_u64()), (Some("me"), Some(0)));
        assert!(now["positionMs"].as_u64().unwrap() >= 30_000 && now["playerId"].as_i64().unwrap() >= 0);
        // Half of the 200 s song is the line.
        call(&s, &me, "reportPlayback", &report("stopped", 99_000, "")).await;
        assert_eq!(plays(&call(&s, &me, "getSong", &format!("id={song}")).await), 0, "stopped too early");
        assert_eq!(call(&s, &me, "getNowPlaying", "").await["nowPlaying"]["entry"], json!([]), "a stop ends it");
        call(&s, &me, "reportPlayback", &report("stopped", 100_000, "&ignoreScrobble=true")).await;
        assert_eq!(plays(&call(&s, &me, "getSong", &format!("id={song}")).await), 0, "told not to count");
        call(&s, &me, "reportPlayback", &report("stopped", 100_000, "")).await;
        assert_eq!(plays(&call(&s, &me, "getSong", &format!("id={song}")).await), 1);
        // Subsonic's "now playing" scrobble shows it and counts nothing.
        call(&s, &me, "scrobble", &format!("id={song}&submission=false&c=DSub")).await;
        let now = call(&s, &me, "getNowPlaying", "").await["nowPlaying"]["entry"][0].clone();
        assert_eq!((now["playerName"].as_str(), now["positionMs"].as_u64()), (Some("DSub"), Some(0)));
        assert_eq!(plays(&call(&s, &me, "getSong", &format!("id={song}")).await), 1);
        assert_eq!(call(&s, &me, "reportPlayback", "mediaId=tr-999&mediaType=song&positionMs=0&state=playing").await["error"], 70);
        assert_eq!(call(&s, &me, "reportPlayback", &report("rewinding", 0, "")).await["error"], 0);
        assert_eq!(call(&s, &me, "reportPlayback", &format!("mediaId={song}&mediaType=song&state=playing")).await["error"], 10);
    }

    #[tokio::test]
    async fn what_online_services_would_know_is_answered_empty() {
        let (s, me) = server();
        let artist = call(&s, &me, "getArtists", "").await["artists"]["index"][0]["artist"][0]["id"].as_str().unwrap().to_string();
        let info = call(&s, &me, "getArtistInfo2", &format!("id={artist}&count=20")).await;
        assert_eq!(info["artistInfo2"]["similarArtist"], json!([]));
        assert_eq!(call(&s, &me, "getArtistInfo2", "id=ar-nothing").await["error"], 70);
        let album = call(&s, &me, "getAlbumList2", "type=newest").await["albumList2"]["album"][0]["id"].as_str().unwrap().to_string();
        assert!(call(&s, &me, "getAlbumInfo2", &format!("id={album}")).await["albumInfo"].is_object());
        assert_eq!(call(&s, &me, "getSimilarSongs2", "id=tr-1").await, json!({ "similarSongs2": { "song": [] } }));
        assert_eq!(call(&s, &me, "getTopSongs", "artist=Björk").await, json!({ "topSongs": { "song": [] } }));
    }

    #[tokio::test]
    async fn a_position_is_kept_in_a_song_until_it_ends() {
        let (s, me) = server();
        let song = call(&s, &me, "search3", "query=hunter").await["searchResult3"]["song"][0]["id"].as_str().unwrap().to_string();
        assert_eq!(call(&s, &me, "createBookmark", &format!("id={song}&position=135000&c=Symfonium")).await, json!({}));
        let marks = call(&s, &me, "getBookmarks", "").await;
        let mark = &marks["bookmarks"]["bookmark"][0];
        assert_eq!((mark["position"].as_u64(), mark["username"].as_str(), mark["entry"]["id"].as_str()), (Some(135_000), Some("me"), Some(song.as_str())));
        assert_eq!(call(&s, &me, "createBookmark", "id=tr-999&position=1").await["error"], 70, "no such song");
        assert_eq!(call(&s, &me, "deleteBookmark", &format!("id={song}")).await, json!({}));
        assert_eq!(call(&s, &me, "deleteBookmark", &format!("id={song}")).await, json!({}), "safe to repeat");
        assert!(call(&s, &me, "getBookmarks", "").await["bookmarks"]["bookmark"].as_array().unwrap().is_empty());
    }

    #[test]
    fn parameters_repeat_and_forms_count() {
        let p = Params::parse(Some("id=1&id=2&u=me"), b"songId=3");
        assert_eq!(p.all("id"), ["1", "2"]);
        assert_eq!(p.get("songId"), Some("3"));
    }
}
