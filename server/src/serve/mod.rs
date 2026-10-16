//! Cratarr as an OpenSubsonic server (decision 0030): the library the API shows, its ids, who may log in, and the
//! `/rest` endpoints. Copied and translated code keeps a notice naming where it came from.

pub mod api;
pub mod art;
pub mod auth_check;
pub mod files;
pub mod ids;
pub mod import;
pub mod library;
pub mod library_api;
pub mod playlists;
pub mod queues;
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
    // Every playlist write: when its entries came is kept (the panel's Added order).
    crate::playlists::on_write(Box::new(|id, before, after| {
        if let Some(s) = instance()
            && let Err(e) = s.userdata.entries_written(id, before, after)
        {
            tracing::warn!("playlist {id}: when its entries came not kept: {e:#}");
        }
    }));
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
    /// What the last check was made from: (the catalogue's last pass, the rules and added dates). The same again: no
    /// need to look at the catalogue.
    checked: std::sync::Mutex<Option<(Option<i64>, String)>>,
    /// Resized covers on disk (`art`).
    art_cache: art::Cache,
    /// One cover resized at a time: a client asks for hundreds at once, and decoding one takes tens of megabytes.
    resizing: tokio::sync::Semaphore,
    /// Cratarr's playlist service, once the app is up (`set_playlists`): the Subsonic playlists are its files.
    playlists: std::sync::OnceLock<Arc<crate::app::PlaylistService>>,
    /// Playlists a client emptied, waiting briefly for the call that refills them (`playlists::Held`).
    pub held: playlists::Held,
    /// What the Subsonic apps report playing (`getNowPlaying`).
    pub now_playing: queues::Sessions,
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
            checked: Default::default(),
            art_cache: art::Cache::new(data_dir.map(|d| d.join("cover-cache")), art::CACHE_BYTES),
            resizing: tokio::sync::Semaphore::new(1),
            playlists: Default::default(),
            held: Default::default(),
            now_playing: Default::default(),
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

    /// A playlist was renamed (every rename goes through the playlist service, which tells this): its old ids keep
    /// leading to it, as Subsonic clients keep a playlist's id across a rename.
    pub fn playlist_renamed(&self, from: &str, to: &str) {
        if let Err(e) = self.userdata.playlist_renamed(from, to) {
            tracing::warn!("subsonic: the rename of {from} to {to} not kept: {e:#}");
        }
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
        // The tag rules as saved in Settings, so a change there needs no restart.
        if let Some(dir) = &self.data_dir
            && let Ok(Some(saved)) = crate::settings::load(&dir.join("settings.json"), &|k: &str| std::env::var(k).ok())
        {
            *self.rules.write().unwrap_or_else(|p| p.into_inner()) = saved.tag_rules;
        }
        let rules = self.rules.read().unwrap_or_else(|p| p.into_inner()).clone();
        let added = self.userdata.added()?;
        // The catalogue is only looked at after it finished a pass (or the rules or added dates changed): reading its
        // files' summary every minute kept its 200 MB file in memory for nothing.
        let inputs = (self.catalogue().last_pass(), format!("{}|{}", serde_json::to_string(&rules)?, added.len() as i64 + added.values().sum::<i64>()));
        {
            let mut last = self.checked.lock().unwrap_or_else(|p| p.into_inner());
            if last.as_ref() == Some(&inputs) {
                return Ok(false);
            }
            *last = Some(inputs);
        }
        self.library.set_added(added);
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

    /// The album an art id (an album's, a song's or an artist's, maybe with a fingerprint after `_`) stands for, and the
    /// fingerprint asked for. Blocking.
    fn art_album(&self, id: &str) -> Result<(Option<String>, Option<String>)> {
        let (id, asked) = match id.split_once('_') {
            Some((a, p)) => (a.to_string(), Some(p.to_string())),
            None => (id.to_string(), None),
        };
        let lib = &self.library;
        let album = if id.starts_with(ids::ARTIST) {
            lib.artist(&id)?.and_then(|a| a.album.and_then(|albums| albums.last().map(|al| al.id.clone())))
        } else if id.starts_with(ids::SONG) {
            lib.songs_by_id(std::slice::from_ref(&id))?.pop().map(|s| s.album_id)
        } else {
            Some(id)
        };
        Ok((album, asked))
    }

    /// An album's art as it is: (content type, bytes, its fingerprint). The folder's cover file, else the picture in
    /// its first track. Blocking.
    fn art_of(&self, album: &str) -> Result<Option<(String, Vec<u8>, String)>> {
        let Some((folder, dir, cover, first)) = self.library.album_art(album)? else { return Ok(None) };
        let Some(root) = self.library.roots().into_iter().find(|(f, _)| *f == folder).map(|(_, r)| PathBuf::from(r)) else {
            return Ok(None);
        };
        let print = cover.as_deref().and_then(|c| c.split_once('_')).map(|(_, p)| p.to_string()).unwrap_or_default();
        if let Some(file) = art::cover_file(&root.join(&dir))
            && std::fs::metadata(&file).is_ok_and(|m| m.len() <= art::MAX_BYTES)
            && let Ok(bytes) = std::fs::read(&file)
        {
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let ct = crate::covers::content_type(name).unwrap_or_else(|| art::sniff(&bytes));
            return Ok(Some((ct.to_string(), bytes, print)));
        }
        Ok(first.and_then(|rel| art::embedded(&root.join(rel))).map(|(ct, bytes)| (ct, bytes, print)))
    }

    /// An album's cover image as it is: (content type, bytes); `None` when it has none.
    pub async fn cover_bytes(self: &Arc<Self>, album: &str) -> Result<Option<(String, Vec<u8>)>> {
        let (me, album) = (self.clone(), album.to_string());
        Ok(tokio::task::spawn_blocking(move || me.art_of(&album)).await??.map(|(ct, bytes, _)| (ct, bytes)))
    }

    /// An album's (or a song's, or an artist's) cover, at most `size` pixels on its longer side when asked. An id with
    /// the current fingerprint may be kept for good; any other is to be checked again (Navidrome's rule, see `art`).
    pub async fn cover(self: &Arc<Self>, id: &str, size: Option<u32>, headers: &HeaderMap) -> Result<Response> {
        let (me, want) = (self.clone(), id.to_string());
        let found = tokio::task::spawn_blocking(move || -> Result<Option<(String, String, Vec<u8>, String, Option<String>)>> {
            let (album, asked) = me.art_album(&want)?;
            let Some(album) = album else { return Ok(None) };
            Ok(me.art_of(&album)?.map(|(ct, bytes, print)| (album, ct, bytes, print, asked)))
        })
        .await??;
        let Some((album, ct, bytes, print, asked)) = found else { return Ok(StatusCode::NOT_FOUND.into_response()) };
        let etag = format!("\"{print}-{}\"", size.unwrap_or(0));
        let lasting = !print.is_empty() && asked.as_deref() == Some(print.as_str());
        let cache = HeaderValue::from_static(if lasting { "public, max-age=31536000, immutable" } else { "no-cache" });
        let head = |ct: &str| {
            [
                (header::CACHE_CONTROL, cache.clone()),
                (header::ETAG, HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("\"\""))),
                (header::CONTENT_TYPE, HeaderValue::from_str(ct).unwrap_or(HeaderValue::from_static("image/jpeg"))),
            ]
        };
        if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()).is_some_and(|h| crate::covers::etag_matches(h, &etag)) {
            return Ok((StatusCode::NOT_MODIFIED, head(&ct)).into_response());
        }
        let Some(size) = size else { return Ok((head(&ct), bytes).into_response()) };
        let key = format!("{album}:{print}:{size}");
        if let Some(small) = self.art_cache.get(&key) {
            return Ok((head("image/jpeg"), small).into_response());
        }
        let _turn = self.resizing.acquire().await?;
        let me = self.clone();
        let (small, bytes) = tokio::task::spawn_blocking(move || {
            // A picture the resizer cannot read, or one small enough already, goes as it is.
            let small = art::resize(&bytes, size).ok().flatten();
            if let Some(s) = &small {
                me.art_cache.put(&key, s);
            }
            (small, bytes)
        })
        .await?;
        Ok(match small {
            Some(s) => (head("image/jpeg"), s).into_response(),
            None => (head(&ct), bytes).into_response(),
        })
    }
}
