//! Cratarr as an OpenSubsonic server (decision 0030): the library the API shows, its ids, who may log in, and the
//! `/rest` endpoints. Copied and translated code keeps a notice naming where it came from.

pub mod api;
pub mod files;
pub mod ids;
pub mod import;
pub mod library;
pub mod library_api;
pub mod playlists;
pub mod read;
pub mod review;
pub mod routes;
pub mod users;

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::Result;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::catalogue::{Catalogue, Rules};
use crate::userdata::UserData;

/// How often the library is checked against the catalogue (a cheap check; it is built again only when files changed).
const CHECK_EVERY: Duration = Duration::from_secs(60);
/// The first check after Cratarr starts.
const FIRST_AFTER: Duration = Duration::from_secs(90);

/// The server, once `main` has opened it: what the built-in Cratarr source reads (plugins are made from their settings
/// alone, decision 0005).
static INSTANCE: std::sync::OnceLock<Arc<Serve>> = std::sync::OnceLock::new();

pub fn set_instance(serve: Arc<Serve>) {
    let _ = INSTANCE.set(serve);
}

pub fn instance() -> Option<Arc<Serve>> {
    INSTANCE.get().cloned()
}

pub struct Serve {
    /// Cratarr's data folder, left out of the folders offered as libraries.
    pub data_dir: Option<PathBuf>,
    pub library: Arc<library::Library>,
    pub userdata: Arc<UserData>,
    pub users: users::Users,
    /// Attached once the app has opened it (`attach_catalogue`); the server opens before the app, so the built-in
    /// Cratarr source can answer while the app starts (decision 0032).
    catalogue_cell: std::sync::OnceLock<Arc<Catalogue>>,
    /// Imports from media servers into the user data (decision 0030, amended 2026-10-15).
    pub import: import::Importer,
    rules: RwLock<Rules>,
    /// When the library was last built, epoch seconds (`getIndexes`' `lastModified`).
    built_at: RwLock<i64>,
    /// Told each time the library is built (its time): views kept from the built-in source can read it again.
    built: tokio::sync::watch::Sender<i64>,
    /// Cratarr's playlist service, once the app is up (`set_playlists`): the Subsonic playlists are its files.
    playlists: std::sync::OnceLock<Arc<crate::app::PlaylistService>>,
    /// Cratarr's Incoming, once the app is up (`set_incoming`): promote and reject are its Keep and Delete.
    incoming: std::sync::OnceLock<Arc<crate::app::IncomingService>>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Serve {
    /// The databases and logins in the data folder (`None`: in memory). Logins from `CRATARR_SUBSONIC_USER` and
    /// `CRATARR_SUBSONIC_PASSWORD` are added (or their password set) at start.
    pub fn open(data_dir: Option<&std::path::Path>, rules: Rules, env: &dyn Fn(&str) -> Option<String>) -> Result<Arc<Self>> {
        let library = Arc::new(library::Library::open(data_dir.map(|d| d.join("library.sqlite")).as_deref())?);
        let userdata = Arc::new(UserData::open(data_dir.map(|d| d.join("userdata.sqlite")).as_deref())?);
        let users = users::Users::open(data_dir)?;
        if let (Some(u), Some(p)) = (env("CRATARR_SUBSONIC_USER"), env("CRATARR_SUBSONIC_PASSWORD"))
            && !u.trim().is_empty()
            && !p.is_empty()
        {
            users.set(u.trim(), &p)?;
        }
        Ok(Arc::new(Self {
            data_dir: data_dir.map(std::path::Path::to_path_buf),
            library,
            userdata,
            users,
            catalogue_cell: Default::default(),
            import: import::Importer::default(),
            rules: RwLock::new(rules),
            built_at: RwLock::new(now()),
            built: tokio::sync::watch::channel(0).0,
            playlists: Default::default(),
            incoming: Default::default(),
        }))
    }

    pub fn attach_catalogue(&self, catalogue: Arc<Catalogue>) {
        let _ = self.catalogue_cell.set(catalogue);
    }

    /// Whether the catalogue is attached yet (the built-in source asks before using it).
    pub fn has_catalogue(&self) -> bool {
        self.catalogue_cell.get().is_some()
    }

    /// The catalogue. `main` attaches it before anything that uses it runs: the API is served, and the library
    /// followed, only after.
    pub fn catalogue(&self) -> &Arc<Catalogue> {
        self.catalogue_cell.get().expect("the catalogue is attached before Cratarr serves")
    }

    pub fn set_playlists(&self, service: Arc<crate::app::PlaylistService>) {
        let _ = self.playlists.set(service);
    }

    pub fn playlists(&self) -> Option<&Arc<crate::app::PlaylistService>> {
        self.playlists.get()
    }

    pub fn set_incoming(&self, service: Arc<crate::app::IncomingService>) {
        let _ = self.incoming.set(service);
    }

    pub fn incoming(&self) -> Option<&Arc<crate::app::IncomingService>> {
        self.incoming.get()
    }

    /// Changes each time the library is built (to the time it was): for views kept from the built-in source.
    #[allow(dead_code)] // for Collection and Incoming to read the built-in source again (decision 0032)
    pub fn subscribe_built(&self) -> tokio::sync::watch::Receiver<i64> {
        self.built.subscribe()
    }

    pub fn built_at(&self) -> i64 {
        *self.built_at.read().unwrap_or_else(|p| p.into_inner())
    }

    /// Builds the library again when the catalogue's files changed; after a rebuilt catalogue, user data follows its
    /// songs to their new ids. Blocking.
    pub fn refresh(&self) -> Result<bool> {
        let rules = self.rules.read().unwrap_or_else(|p| p.into_inner()).clone();
        self.library.set_added(self.userdata.added()?);
        let built = self.library.refresh_from(self.catalogue(), &rules)?;
        if built {
            let at = now();
            *self.built_at.write().unwrap_or_else(|p| p.into_inner()) = at;
            let moved = self.userdata.follow_pids(&self.library.pids()?)?;
            if moved > 0 {
                tracing::info!("subsonic: {moved} songs' plays and stars followed them to new ids");
            }
            let (songs, albums, artists) = self.library.counts()?;
            tracing::info!("subsonic: library built, {songs} songs, {albums} albums, {artists} artists");
            // The built-in Cratarr source's ids are this library's: have the catalogue link them now, and tell those
            // who keep what it listed.
            self.catalogue().poke();
            self.built.send_replace(at);
        }
        Ok(built)
    }

    /// Keeps the library in step with the catalogue, in the background. The first check waits a little, so it does not
    /// add to the work Cratarr does as it starts (the playlist sync reads every playlist then), unless the library is
    /// empty.
    pub fn follow(self: &Arc<Self>) {
        let me = self.clone();
        tokio::spawn(async move {
            // An empty library (the first start, or a new layout) is built at once: until then the built-in source
            // has no albums to show.
            if me.library.counts().map(|(songs, _, _)| songs > 0).unwrap_or(true) {
                tokio::time::sleep(FIRST_AFTER).await;
            }
            let mut first = true;
            loop {
                let s = me.clone();
                match tokio::task::spawn_blocking(move || s.refresh()).await {
                    Ok(Err(e)) => tracing::warn!("subsonic: could not build the library: {e:#}"),
                    Err(e) => tracing::warn!("subsonic: the library build stopped: {e}"),
                    // Once ready after a start, the catalogue links the built-in source's ids without waiting for its
                    // next pass (a build pokes it too).
                    Ok(Ok(built)) if first && !built => me.catalogue().poke(),
                    Ok(Ok(_)) => {}
                }
                first = false;
                tokio::time::sleep(CHECK_EVERY).await;
            }
        });
    }

    /// The file of a song.
    pub async fn song_path(&self, song: &str) -> Result<Option<PathBuf>> {
        let lib = self.library.clone();
        let id = song.to_string();
        let found = tokio::task::spawn_blocking(move || lib.file_of(&id)).await??;
        let folders = self.catalogue().folders();
        Ok(found.and_then(|(folder, rel)| folders.get(folder as usize).map(|root| root.join(rel))))
    }

    /// An album's cover image from its folder: (content type, bytes); `None` when it has none.
    pub async fn cover_bytes(&self, album: &str) -> Result<Option<(String, Vec<u8>)>> {
        let lib = self.library.clone();
        let id = album.to_string();
        let dir = tokio::task::spawn_blocking(move || lib.album_dir(&id)).await??;
        let folders = self.catalogue().folders();
        let Some((root, dir)) = dir.and_then(|(f, d)| folders.get(f as usize).map(|r| (r.clone(), d))) else { return Ok(None) };
        Ok(match crate::covers::resolve(&root, &format!("{dir}/cover"), None).await {
            crate::covers::Outcome::Image { content_type, bytes, .. } => Some((content_type.to_string(), bytes)),
            _ => None,
        })
    }

    /// An album's (or an artist's, or a song's) cover, from the album's folder.
    pub async fn cover(&self, id: &str, _size: Option<u32>, headers: &HeaderMap) -> Result<Response> {
        let lib = self.library.clone();
        let id = id.to_string();
        let dir = tokio::task::spawn_blocking(move || -> Result<Option<(i64, String)>> {
            let album = if id.starts_with(ids::ARTIST) {
                lib.artist(&id)?.and_then(|a| a.album.and_then(|albums| albums.last().map(|al| al.id.clone())))
            } else if id.starts_with(ids::SONG) {
                lib.songs_by_id(&[id.clone()])?.pop().map(|s| s.album_id)
            } else {
                Some(id.clone())
            };
            match album {
                Some(a) => lib.album_dir(&a),
                None => Ok(None),
            }
        })
        .await??;
        let folders = self.catalogue().folders();
        let Some((root, dir)) = dir.and_then(|(f, d)| folders.get(f as usize).map(|r| (r.clone(), d))) else {
            return Ok(StatusCode::NOT_FOUND.into_response());
        };
        let inm = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok());
        let cache = (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=86400"));
        Ok(match crate::covers::resolve(&root, &format!("{dir}/cover"), inm).await {
            crate::covers::Outcome::NotFound => StatusCode::NOT_FOUND.into_response(),
            crate::covers::Outcome::NotModified { etag } => {
                (StatusCode::NOT_MODIFIED, [cache, (header::ETAG, HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("\"\"")))]).into_response()
            }
            crate::covers::Outcome::Image { content_type, etag, bytes } => (
                [
                    cache,
                    (header::ETAG, HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("\"\""))),
                    (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
                ],
                bytes,
            )
                .into_response(),
        })
    }
}
