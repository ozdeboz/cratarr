//! Cratarr as an OpenSubsonic server (decision 0030): the library the API shows, its ids, who may log in, and the
//! `/rest` endpoints. Copied and translated code keeps a notice naming where it came from.

pub mod access;
pub mod accounts;
pub mod along;
pub mod api;
pub mod art;
pub mod auth_check;
pub mod files;
pub mod guest;
pub mod ids;
pub mod import;
pub mod ldap;
pub mod library;
pub mod library_api;
pub mod playlists;
pub mod proxy;
pub mod queues;
pub mod rate;
pub mod read;
pub mod review;
pub mod routes;
pub mod sessions;
pub mod signin;
pub mod users;
pub mod who;

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
/// New analyses alone build the library again at most this often (s): the analyser works through new albums a few
/// tracks a minute, and each full build is seconds of work and a full pass of the catalogue's file (2026-10-17: six
/// builds for four albums). Changed songs build at the next check.
const ANALYSES_EVERY: i64 = 600;
/// The first check after Cratarr starts.
const FIRST_AFTER: Duration = Duration::from_secs(90);

/// The server, once `main` has opened it: what the built-in Cratarr source reads (plugins are made from their settings
/// alone, decision 0005).
static INSTANCE: std::sync::OnceLock<Arc<Serve>> = std::sync::OnceLock::new();

pub fn set_instance(serve: Arc<Serve>) {
    let _ = INSTANCE.set(serve);
    // Every playlist write: when its entries came is kept (the panel's Added order), and the change logged (decision
    // 0087).
    crate::playlists::on_write(Box::new(|id, before, after| {
        let Some(s) = instance() else { return };
        if let Err(e) = s.userdata.entries_written(id, before, after) {
            tracing::warn!("playlist {id}: when its entries came not kept: {e:#}");
        }
        // Its lines keep their entry ids through Cratarr's own edits (decision 0087).
        if let Err(e) = s.userdata.entries_of(id, after) {
            tracing::warn!("playlist {id}: its entry ids not kept: {e:#}");
        }
        if let Err(e) = s.userdata.playlist_changed(id, crate::userdata::offline::Op::Changed) {
            tracing::warn!("playlist {id}: its change not logged: {e:#}");
        }
    }));
    // A playlist made, renamed, deleted or brought back: logged (decision 0087); one shared that goes (deleted, renamed
    // away), told to the others as gone.
    crate::playlists::on_named(Box::new(|id, there| {
        let Some(s) = instance() else { return };
        if !there && s.userdata.is_playlist_shared(id).unwrap_or(false) {
            s.playlist_gone_for_others(id);
        }
        if let Err(e) = s.userdata.playlist_changed(id, crate::userdata::offline::Op::of(there)) {
            tracing::warn!("playlist {id}: its change not logged: {e:#}");
        }
    }));
}

/// The server, for what has no handle to it. A request's code takes it from its `Ctx` (api.rs) or the service it holds
/// (the app's `served`, `PlaylistService::served`); this stays for what is not handed one: the built-in Cratarr source
/// (sources are made from their settings alone), work of its own started at boot (the passes that read the web, the
/// artist store's), the playlist write hook above, and free functions reached from many places, where a parameter
/// would run through dozens of signatures (the album edits run as tasks, Radio, the feature switches, signed links,
/// the cover cache).
pub fn instance() -> Option<Arc<Serve>> {
    INSTANCE.get().cloned()
}

pub struct Serve {
    /// Cratarr's data folder, left out of the folders offered as libraries.
    pub data_dir: Option<PathBuf>,
    pub library: Arc<library::Library>,
    pub userdata: Arc<UserData>,
    pub users: users::Users,
    /// Signed-in devices (decision 0039), and the sign-in page's failed tries.
    pub sessions: sessions::Sessions,
    /// A directory (LDAP) asked first at sign-in (decision 0040).
    pub directory: ldap::Directory,
    pub throttle: signin::Throttle,
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
    /// Cratarr's players, once the app is up (`set_play_on`): an account's devices, and what plays for getNowPlaying.
    play_on: std::sync::OnceLock<Arc<crate::app::PlayOn>>,
    /// Each album's cover stamp (0.9.7, `cover_stamps`), as of the last build; read from the library when first asked.
    stamps: RwLock<Option<Arc<std::collections::HashMap<String, String>>>>,
    /// The covers a build changed (`subscribe_covers`).
    covers: tokio::sync::broadcast::Sender<Arc<Vec<CoverChange>>>,
}

/// An album's cover changed (0.9.7): its new stamp, none where it has no cover now. The live channel tells the pages
/// (app/events.rs `cover`), which ask for its cover again by the new stamp.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CoverChange {
    pub album: String,
    pub v: Option<String>,
}

/// The albums whose stamp differs between two builds, by id (none where the cover went).
fn cover_changes(before: &std::collections::HashMap<String, String>, now: &std::collections::HashMap<String, String>) -> Vec<CoverChange> {
    let mut out: Vec<CoverChange> =
        now.iter().filter(|(id, v)| before.get(*id) != Some(*v)).map(|(id, v)| CoverChange { album: id.clone(), v: Some(v.clone()) }).collect();
    out.extend(before.keys().filter(|id| !now.contains_key(*id)).map(|id| CoverChange { album: id.clone(), v: None }));
    out.sort_by(|a, b| a.album.cmp(&b.album));
    out
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
        // A new install (no logins kept yet) requires a sign-in everywhere from the start; one from before decision 0043
        // keeps it off until the owner switches it on, once its clients have their keys.
        let fresh = data_dir.is_some_and(|d| !d.join("subsonic-users.json").exists());
        let users = users::Users::open(data_dir)?;
        let sessions = sessions::Sessions::open(data_dir.map(|d| d.join("sessions.sqlite")).as_deref())?;
        if sessions.required().is_none() {
            sessions.set_required(fresh)?;
        }
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
            sessions,
            directory: ldap::Directory::open(data_dir)?,
            throttle: Default::default(),
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
            play_on: Default::default(),
            stamps: Default::default(),
            covers: tokio::sync::broadcast::channel(16).0,
        }))
    }

    /// The account a user name and password sign in as on the sign-in page (decision 0039). The one place an account
    /// is checked: the directory (LDAP), when one is set, is asked first (decision 0040), Cratarr's own accounts staying
    /// the fallback.
    pub async fn sign_in(&self, name: &str, password: &str) -> Option<String> {
        // The directory's members are Cratarr's users (decision 0051): an account made at the first sign-in, brought back
        // if it had left, an administrator as the directory says, with the display name it gives; asked only of an active
        // one when the directory was unreachable. Cratarr's own account of the same name (not from the directory) is that
        // account.
        let account = |a: &str, said: Option<ldap::Said>| match said {
            Some(said) => match self.users.from_directory(a, said.admin, said.display_name.as_deref()) {
                Ok(u) => u.active.then_some(u.name),
                Err(e) => {
                    tracing::warn!(target: "signin", user = %a, "the account from the directory could not be kept: {e:#}");
                    None
                }
            },
            None => self.users.get(a).filter(|u| u.active).map(|u| u.name),
        };
        if let Some(account) = self.directory.sign_in(name, password, &account, now()).await {
            return Some(account);
        }
        self.users.sign_in(name, password).map(|u| u.name)
    }

    /// The account the running request acts for (decision 0051): the one signed in, or, while signing in is not required
    /// (one user, decision 0043 off), the owner. `None` outside a request when signing in is required: work of its own
    /// must say whose it is.
    pub fn current_user(&self) -> Option<String> {
        self.user_for(who::current().as_ref())
    }

    /// The account `asking` acts for, as `current_user` says for the running request (a handler's `Ctx` hands its own).
    pub fn user_for(&self, asking: Option<&who::Who>) -> Option<String> {
        match asking {
            Some(w) => Some(w.user.clone()),
            None if !self.sign_in_required() => self.users.owner(),
            None => None,
        }
    }


    /// Whether every request needs a sign-in now (decision 0043): when switched on, and once there is an account to
    /// sign in with.
    pub fn sign_in_required(&self) -> bool {
        self.sessions.required().unwrap_or(false) && !self.users.names().is_empty()
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
    /// A shared playlist (by its key) gone from the others' sight (unshared, deleted, renamed away): every account but
    /// its owner is told it as deleted (decision 0087).
    pub fn playlist_gone_for_others(&self, key: &str) {
        let owner = match key.split_once('/') {
            Some((account, _)) => Some(account.to_string()),
            None => self.users.owner(),
        };
        let others: Vec<String> = self.users.names().into_iter().filter(|n| Some(n) != owner.as_ref()).collect();
        if let Err(e) = self.userdata.playlist_gone_for(key, &others) {
            tracing::warn!("playlist {key}: its going not logged for the others: {e:#}");
        }
    }

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

    pub fn set_play_on(&self, on: Arc<crate::app::PlayOn>) {
        let _ = self.play_on.set(on);
    }

    pub fn play_on(&self) -> Option<&Arc<crate::app::PlayOn>> {
        self.play_on.get()
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
        let originals = self.userdata.original_years()?;
        let extra_genres = self.userdata.discogs_genres()?;
        let release_types = self.userdata.release_types()?;
        // The catalogue is only looked at after it finished a pass (or the rules, added dates or original years changed):
        // reading its files' summary every minute kept its 200 MB file in memory for nothing.
        let inputs = (
            self.catalogue().last_pass(),
            format!(
                "{}|{}|{}|{}|{}",
                serde_json::to_string(&rules)?,
                added.len() as i64 + added.values().sum::<i64>(),
                originals.len() as u64 + originals.values().map(|y| u64::from(*y)).sum::<u64>(),
                extra_genres.len() + extra_genres.values().map(Vec::len).sum::<usize>(),
                release_types.len() + release_types.values().map(|t| usize::from(t.0.is_some()) + t.1.len()).sum::<usize>()
            ),
        );
        if self.checked.lock().unwrap_or_else(|p| p.into_inner()).as_ref() == Some(&inputs) {
            return Ok(false);
        }
        self.library.set_added(added);
        self.library.set_originals(originals);
        self.library.set_extra_genres(extra_genres);
        self.library.set_release_types(release_types);
        let (change, print) = self.library.change(self.catalogue(), &rules)?;
        // New analyses alone wait for their gap: not noted as checked, so the next check looks again, and builds once
        // the gap has passed (with every analysis that came meanwhile).
        if change == library::Change::Analyses && now() - self.built_at() < ANALYSES_EVERY {
            return Ok(false);
        }
        *self.checked.lock().unwrap_or_else(|p| p.into_inner()) = Some(inputs);
        if change == library::Change::None {
            return Ok(false);
        }
        let (whole, folders) = self.library.build_from(self.catalogue(), &rules, &print)?;
        let why = if change == library::Change::Analyses { "new analyses" } else { "files, tags or rules changed" };
        self.built_now(why, whole, folders, change == library::Change::Songs)?;
        Ok(true)
    }

    /// After the library was built (whole, or `folders` of it): when, the plays and stars that follow songs to new ids, a
    /// line in the log; with changed songs, the catalogue links the built-in source's ids and those who keep its covers'
    /// stamps are told; and those who wait on a build. Blocking.
    fn built_now(&self, why: &str, whole: bool, folders: usize, songs_changed: bool) -> Result<()> {
        let at = now();
        *self.built_at.write().unwrap_or_else(|p| p.into_inner()) = at;
        let moved = self.userdata.follow_pids(&self.library.pids()?)?;
        if moved > 0 {
            tracing::info!(target: "library", moved, "plays and stars followed songs to new ids");
        }
        let (songs, albums, artists) = self.library.counts()?;
        if whole {
            tracing::info!(target: "library", why, songs, albums, artists, "library built");
        } else {
            tracing::info!(target: "library", why, folders, songs, albums, artists, "library built again where it changed");
        }
        // The built-in Cratarr source's ids are this library's: have the catalogue link them now (new analyses change no
        // id, and a poke is a full pass of the folders), and tell those who keep what it listed.
        if songs_changed {
            self.catalogue().poke();
            self.covers_built();
        }
        self.built.send_replace(at);
        Ok(())
    }

    /// Files Cratarr wrote itself (0.9.7: Renumber, a cover, a tempo), read again now, and the library built from them
    /// at once: an album shows its change in seconds, not at the next pass. Where that fails, the next pass reads them.
    /// Blocking.
    pub fn reread(&self, paths: &[PathBuf]) {
        if let Err(e) = self.catalogue().reread(paths) {
            tracing::warn!(target: "library", reason = %format!("{e:#}"), "written files not read again at once");
            self.catalogue().poke();
            return;
        }
        // Their albums built again at once (0.9.8): they show in a moment, not after a whole build.
        match self.library.update_files(self.catalogue(), &self.rules(), paths) {
            Ok(0) => {}
            Ok(folders) => {
                if let Err(e) = self.built_now("Cratarr wrote to them", false, folders, true) {
                    tracing::warn!(target: "library", reason = %format!("{e:#}"), "after building written albums again");
                }
            }
            Err(e) => tracing::warn!(target: "library", reason = %format!("{e:#}"), "written albums not built again at once: the next check builds them"),
        }
    }

    /// The tag rules as last read (Settings › Libraries; their aliases name the harvested words too, 0.9.7).
    pub fn rules(&self) -> Rules {
        self.rules.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Every album's cover stamp (0.9.7): the page puts it on each cover's address, which may then be kept for good.
    pub fn cover_stamps(&self) -> Arc<std::collections::HashMap<String, String>> {
        if let Some(s) = self.stamps.read().unwrap_or_else(|p| p.into_inner()).as_ref() {
            return s.clone();
        }
        let read = Arc::new(self.library.cover_stamps().unwrap_or_else(|e| {
            tracing::warn!(target: "library", reason = %format!("{e:#}"), "cover stamps not read");
            Default::default()
        }));
        *self.stamps.write().unwrap_or_else(|p| p.into_inner()) = Some(read.clone());
        read
    }

    /// Every change of an album's cover from now on, a build's at once.
    pub fn subscribe_covers(&self) -> tokio::sync::broadcast::Receiver<Arc<Vec<CoverChange>>> {
        self.covers.subscribe()
    }

    /// After a build: the stamps read again, and the covers that changed told (none before any page asked for them).
    fn covers_built(&self) {
        let now = match self.library.cover_stamps() {
            Ok(s) => Arc::new(s),
            Err(e) => {
                tracing::warn!(target: "library", reason = %format!("{e:#}"), "cover stamps not read");
                return;
            }
        };
        let before = self.stamps.write().unwrap_or_else(|p| p.into_inner()).replace(now.clone());
        let Some(before) = before else { return };
        let changes = cover_changes(&before, &now);
        if !changes.is_empty() {
            let _ = self.covers.send(Arc::new(changes));
        }
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
                    Ok(Err(e)) => tracing::error!(target: "library", reason = %format!("{e:#}"), "library not built"),
                    Err(e) => tracing::error!(target: "library", reason = %e, "library build stopped"),
                    // Once ready after a start, the catalogue links the built-in source's ids without waiting for its
                    // next pass (a build pokes it too).
                    Ok(Ok(built)) if first && !built => me.catalogue().poke(),
                    Ok(Ok(_)) => {}
                }
                first = false;
                // Files Cratarr wrote and read again at once (`reread`) are built without waiting for the next check.
                let cat = me.catalogue().clone();
                tokio::select! {
                    _ = tokio::time::sleep(CHECK_EVERY) => {}
                    _ = cat.reread_done() => {}
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cover_changed_new_or_gone_is_told_an_unchanged_one_is_not() {
        let map = |pairs: &[(&str, &str)]| pairs.iter().map(|(a, v)| (a.to_string(), v.to_string())).collect::<std::collections::HashMap<_, _>>();
        let before = map(&[("al-1", "aaaa"), ("al-2", "bbbb"), ("al-3", "cccc")]);
        let now = map(&[("al-1", "aaaa"), ("al-2", "dddd"), ("al-4", "eeee")]);
        let told = cover_changes(&before, &now);
        let one = |album: &str, v: Option<&str>| CoverChange { album: album.into(), v: v.map(Into::into) };
        assert_eq!(told, vec![one("al-2", Some("dddd")), one("al-3", None), one("al-4", Some("eeee"))]);
        assert!(cover_changes(&now, &now).is_empty());
    }
}
