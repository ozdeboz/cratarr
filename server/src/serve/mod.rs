//! Cratarr as an OpenSubsonic server (decision 0030): the library the API shows, its ids, who may log in, and the
//! `/rest` endpoints. Copied and translated code keeps a notice naming where it came from.

pub mod access;
pub mod accounts;
pub mod along;
pub mod api;
pub mod art;
pub mod auth_check;
pub mod copies;
pub mod files;
pub mod guest;
pub mod ids;
pub mod import;
pub mod index_rows;
pub mod ldap;
pub mod library;
pub mod library_api;
pub mod listener;
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
    // A playlist made, renamed, deleted or brought back: logged (decision 0087; its writes are logged as they are
    // written); one shared that goes (deleted, renamed away), told to the others as gone.
    crate::playlists::on_named(Box::new(|id, there, shared| {
        let Some(s) = instance() else { return };
        if !there && shared {
            s.playlist_gone_for_others(id);
        }
        if let Err(e) = s.userdata.playlist_changed(id, crate::userdata::offline::Op::of(there), shared) {
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
    /// One device's index made at a time, and what the library's log was last brought up to (`app::device_index`).
    device: std::sync::Mutex<crate::app::device_index::Settled>,
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
        let users = users::Users::open(data_dir)?;
        let sessions = sessions::Sessions::open(data_dir.map(|d| d.join("sessions.sqlite")).as_deref())?;
        // No word on the requirement: on, whatever else the folder holds (decision 0043, amended). Off is only what
        // Settings says with the password: a file lost from a restore, or deleted as corrupt, must not open the server to
        // anyone as its owner. With accounts here it is lost, not new, and its keys with it: say so, as every client is
        // refused from now on.
        if sessions.required().is_none() {
            sessions.set_required(true)?;
            if !users.names().is_empty() {
                tracing::warn!(target: "signin", "sessions.sqlite says nothing of the sign-in: required, and the keys it held are gone");
            }
        }
        // The environment makes the account. Its password from then on is what the account says: set here at every start, one changed
        // in Account came back.
        if let (Some(u), Some(p)) = (env("CRATARR_SUBSONIC_USER"), env("CRATARR_SUBSONIC_PASSWORD"))
            && !u.trim().is_empty()
            && !p.is_empty()
            && users.get(u.trim()).is_none_or(|u| u.password.is_empty())
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
            device: Default::default(),
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
            Some(said) => {
                let new = self.users.get(a).is_none();
                match self.users.from_directory(a, said.admin, said.display_name.as_deref()) {
                    Ok(u) => {
                        if new {
                            self.others_own_off(&u.name);
                        }
                        u.active.then_some(u.name)
                    }
                    Err(e) => {
                        tracing::warn!(target: "signin", user = %a, "the account from the directory could not be kept: {e:#}");
                        None
                    }
                }
            }
            None => self.users.get(a).filter(|u| u.active).map(|u| u.name),
        };
        if let Some(account) = self.directory.sign_in(name, password, &account, now()).await {
            return Some(account);
        }
        // All a directory's account holds here is its app password, for OpenSubsonic apps: never a way past the directory
        // that just said no, or a password changed there goes on working, and one a third-party app leaked signs in as an
        // administrator. Refused after the check, so how long it takes tells nothing (`Users::sign_in`).
        let directory_on = self.directory.config().enabled;
        self.users.sign_in(name, password).filter(|u| !(directory_on && u.directory)).map(|u| u.name)
    }

    /// Whether `password` is `account`'s, for what asks for it again (`access::reauthenticate`): whether it signs in as that
    /// account by any name it goes by. A session knows the account, and the directory may know it by another name; and a
    /// name may sign in as another account than the one of its name, whose password this is not.
    pub async fn password_of(&self, account: &str, password: &str) -> bool {
        for name in self.directory.names_of(account) {
            if self.sign_in(&name, password).await.as_deref() == Some(account) {
                return true;
            }
        }
        false
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


    /// The own-music folder as Settings keeps it (0101 point 9).
    pub fn own_music(&self) -> Option<PathBuf> {
        let dir = self.data_dir.as_ref()?;
        let saved = crate::settings::load(&dir.join("settings.json"), &|k: &str| std::env::var(k).ok()).ok().flatten()?;
        saved.sources.iter().find_map(|s| s.own_music())
    }

    /// Everyone else's own music switched off for `account`, new here (0101 point 6: others' start off).
    pub fn others_own_off(&self, account: &str) {
        let Some(own) = self.own_music() else { return };
        for folder in std::fs::read_dir(own).map(|d| d.flatten().map(|e| e.path()).collect::<Vec<_>>()).unwrap_or_default() {
            let Some(name) = folder.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
            if !folder.is_dir() || name.starts_with('.') || name == account {
                continue;
            }
            let key = format!("{}/{}", crate::source::cratarr::KIND.id, folder.display());
            if let Err(e) = self.userdata.switch_library_off(&[account.to_string()], &key) {
                tracing::warn!("{name}'s music not switched off for {account}: {e:#}");
            }
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

    /// The song an id stands for now: one a take retired names the track that took it, and a queue or an app's list can hold
    /// it long after (`Listener::song` plays it). Any other id is as given. Blocking.
    pub fn song_now(&self, id: &str) -> String {
        let taken = self.catalogue_cell.get().and_then(|c| c.track_of_source(crate::source::cratarr::KIND.id, id));
        taken.map_or_else(|| id.to_string(), ids::song)
    }

    /// The catalogue. `main` attaches it before anything that uses it runs: the API is served, and the library
    /// followed, only after.
    pub fn catalogue(&self) -> &Arc<Catalogue> {
        self.catalogue_cell.get().expect("the catalogue is attached before Cratarr serves")
    }

    /// Every artist's id for life by `ids::artist_key`: none until the catalogue is attached. Blocking.
    pub fn artist_ids(&self) -> Result<std::collections::HashMap<String, i64>> {
        self.catalogue_cell.get().map_or_else(|| Ok(Default::default()), |c| c.artist_ids())
    }

    /// An artist's id as served, by their name as any file or service writes it: none for a name with no id. Blocking.
    pub fn artist_id(&self, name: &str) -> Result<Option<String>> {
        Ok(self.catalogue_cell.get().map(|c| c.artist_of(name)).transpose()?.flatten().map(ids::artist))
    }

    pub fn set_playlists(&self, service: Arc<crate::app::PlaylistService>) {
        let _ = self.playlists.set(service);
    }

    pub fn playlists(&self) -> Option<&Arc<crate::app::PlaylistService>> {
        self.playlists.get()
    }

    /// A shared playlist (by its id) gone from the others' sight (unshared, deleted): every account but its owner is
    /// told it as deleted (decision 0087).
    pub fn playlist_gone_for_others(&self, key: &str) {
        // Its owner's account: another's by its name, '' the first administrator's.
        let owner = match self.userdata.playlist_owner(key).ok().flatten() {
            Some(account) if !account.is_empty() => Some(account),
            _ => self.users.owner(),
        };
        let others: Vec<String> = self.users.names().into_iter().filter(|n| Some(n) != owner.as_ref()).collect();
        if let Err(e) = self.userdata.playlist_gone_for(key, &others) {
            tracing::warn!("playlist {key}: its going not logged for the others: {e:#}");
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

    /// Tests elsewhere: the covers' stamps as a build's end leaves them.
    #[cfg(test)]
    pub fn covers_read_again_for_tests(&self) {
        self.covers_built();
    }

    /// Tests elsewhere: the time of the last build moved on, as a build's end moves it.
    #[cfg(test)]
    pub fn build_passed_for_tests(&self) {
        *self.built_at.write().unwrap() += 1;
    }

    /// A device's index being made: until this goes, another waits, as two would hold twice what one does. Blocking.
    pub fn device_turn(&self) -> std::sync::MutexGuard<'_, crate::app::device_index::Settled> {
        self.device.lock().unwrap_or_else(|p| p.into_inner())
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
        // The libraries as they are now: `main` set them before the first build, a change in Settings arrives here.
        if let Some(incoming) = self.incoming() {
            self.library.set_kinds(incoming.folder_kinds());
        }
        // Left over from a build that could not finish it: with nothing changed there may be no other build to ask.
        if let Err(e) = self.follow_taken() {
            tracing::warn!(target: "library", reason = %format!("{e:#}"), "what names taken tracks has not followed them yet");
        }
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
        // A record's facts set by hand, and the albums found missing tracks (0.13: typed as their whole release).
        let overrides = self.userdata.overrides()?;
        let set = self.userdata.overrides_stamp(None)?;
        let mut incomplete: Vec<(String, String)> = self.userdata.albums_incomplete()?;
        incomplete.sort_unstable();
        // The catalogue is only looked at after it finished a pass (or the rules, added dates or original years changed):
        // reading its files' summary every minute kept its 200 MB file in memory for nothing.
        let inputs = (
            self.catalogue().last_pass(),
            format!(
                "{:?}|{}|{}|{}|{}|{}|{}|{}",
                self.library.kinds(),
                serde_json::to_string(&rules)?,
                added.len() as i64 + added.values().sum::<i64>(),
                originals.len() as u64 + originals.values().map(|y| u64::from(*y)).sum::<u64>(),
                extra_genres.len() + extra_genres.values().map(Vec::len).sum::<usize>(),
                release_types.len() + release_types.values().map(|t| usize::from(t.0.is_some()) + t.1.len()).sum::<usize>(),
                incomplete.iter().map(|(a, f)| format!("{a}={f}")).collect::<Vec<_>>().join(","),
                set
            ),
        );
        if self.checked.lock().unwrap_or_else(|p| p.into_inner()).as_ref() == Some(&inputs) {
            return Ok(false);
        }
        self.library.set_added(added);
        self.library.set_originals(originals);
        self.library.set_extra_genres(extra_genres);
        self.library.set_release_types(release_types);
        self.library.set_incomplete(incomplete.iter().filter_map(|(a, f)| Some((a.clone(), whole_of(f)?))).collect());
        self.library.set_overrides(overrides);
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

    /// What names a track the build's take retired follows the track that took it (N1: a retired id can come back with its
    /// plays, and then be taken). The one place for it: another store that names tracks joins here, before the mark. Marked
    /// once all have, so a stop before it goes over them again, and each must change nothing the second time. Blocking.
    fn follow_taken(&self) -> Result<()> {
        let moved = self.catalogue().tracks_to_follow()?;
        if !moved.is_empty() {
            let rows = self.userdata.move_tracks(&moved)?;
            let songs = crate::moved::Moved { ids: moved.iter().map(|(old, id)| (ids::song(*old), ids::song(*id))).collect(), ..Default::default() };
            self.userdata.move_filters(&songs)?;
            // `main` hands them over before the first build: only a test has none. A store's lock is a task's, and every way
            // here is a blocking thread, which can wait for one.
            if let Some(playlists) = self.playlists() {
                tokio::runtime::Handle::current().block_on(playlists.follow_tracks(&songs))?;
            }
            self.catalogue().tracks_followed(&moved)?;
            tracing::info!(target: "library", tracks = moved.len(), rows = ?rows, "what named taken tracks followed them");
        }
        Ok(())
    }

    /// When this build ended: past the build before even inside one second, as what was read between the two is kept by this, and
    /// would pass for read after both. A reading of the crates so kept stands for five minutes, and a device's rows with it.
    fn build_stamped(&self) -> i64 {
        let mut at = self.built_at.write().unwrap_or_else(|p| p.into_inner());
        *at = now().max(*at + 1);
        *at
    }

    /// After the library was built (whole, or `folders` of it): when, the plays and stars that follow songs to new ids, a
    /// line in the log; with changed songs, the catalogue links the built-in source's ids and those who keep its covers'
    /// stamps are told; and those who wait on a build. Blocking.
    fn built_now(&self, why: &str, whole: bool, folders: usize, songs_changed: bool) -> Result<()> {
        let at = self.build_stamped();
        // Before the persistent ids: they would move what the merge means to add up. Failing, the next check goes over it.
        if let Err(e) = self.follow_taken() {
            tracing::warn!(target: "library", reason = %format!("{e:#}"), "what names taken tracks has not followed them yet");
        }
        // A track away with its library is still that track: no song holds it for now, and one tagged alike is another record's.
        let mut songs: std::collections::HashSet<String> = self.library.all_song_ids()?.into_iter().collect();
        songs.extend(self.catalogue().tracks_alive()?.into_iter().map(ids::song));
        let moved = self.userdata.follow_pids(&self.library.pids()?, &songs)?;
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

    /// Who a stream for `account` is for (none: no one in particular). No lock on the way: the libraries as they stand,
    /// and the account's switches as kept.
    pub fn listener(&self, account: Option<&str>) -> listener::Listener {
        let mut who = listener::Listener { library: Some(self.library.clone()), ..Default::default() };
        let Some(name) = account else { return who };
        who.account = Some(name.to_string());
        if let Some(incoming) = self.incoming() {
            who.off = incoming.folders_off(&self.userdata.hidden_libraries_kept(name));
        }
        who
    }

    /// Which file of a song plays for `who`: the copy `copy_for` chose, else the row copy. Blocking.
    pub fn file_for(&self, song: &str, who: &listener::Listener) -> Result<Option<(i64, String)>> {
        let files = ids::track_of(song).map(|t| self.catalogue().files_of(t)).unwrap_or_default();
        self.library.file_for(song, &files, who)
    }

    /// The file of a song, as it plays for `account`.
    pub async fn song_path(self: &Arc<Self>, song: &str, account: Option<&str>) -> Result<Option<PathBuf>> {
        let (me, id, who) = (self.clone(), song.to_string(), self.listener(account));
        let found = tokio::task::spawn_blocking(move || me.file_for(&id, &who)).await??;
        let folders = self.catalogue().folders();
        Ok(found.and_then(|(folder, rel)| folders.get(folder).map(|root| root.join(rel))))
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
        let print = cover.as_deref().and_then(|c| c.split_once('_')).map(|(_, p)| p.to_string()).unwrap_or_default();
        Ok(self.art_at(folder, &dir, first)?.map(|(ct, bytes)| (ct, bytes, print)))
    }

    /// The picture of what lies at (`folder`, `dir`): its cover file, else what is inside `first`, its first file.
    fn art_at(&self, folder: i64, dir: &str, first: Option<String>) -> Result<Option<(String, Vec<u8>)>> {
        let Some(root) = self.library.roots().into_iter().find(|(f, _)| *f == folder).map(|(_, r)| PathBuf::from(r)) else {
            return Ok(None);
        };
        if let Some(file) = art::cover_file(&root.join(dir))
            && std::fs::metadata(&file).is_ok_and(|m| m.len() <= art::MAX_BYTES)
            && let Ok(bytes) = std::fs::read(&file)
        {
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let ct = crate::covers::content_type(name).unwrap_or_else(|| art::sniff(&bytes));
            return Ok(Some((ct.to_string(), bytes)));
        }
        Ok(first.and_then(|rel| art::embedded(&root.join(rel))))
    }

    /// An album's cover image as it is: (content type, bytes); `None` when it has none.
    pub async fn cover_bytes(self: &Arc<Self>, album: &str) -> Result<Option<(String, Vec<u8>)>> {
        let (me, album) = (self.clone(), album.to_string());
        Ok(tokio::task::spawn_blocking(move || me.art_of(&album)).await??.map(|(ct, bytes, _)| (ct, bytes)))
    }

    /// A record's cover made small for a device (`art::jpeg_at`), a decode at a time as `cover`'s are; none where it has none or
    /// it cannot be read.
    pub async fn small_cover(self: &Arc<Self>, album: &str, side: u32, quality: u8) -> Result<Option<Vec<u8>>> {
        let Some((_, bytes)) = self.cover_bytes(album).await? else { return Ok(None) };
        let _turn = self.resizing.acquire().await?;
        Ok(tokio::task::spawn_blocking(move || art::jpeg_at(&bytes, side, quality)).await?.ok())
    }

    /// The cover of a record's copy in `library`, as `cover_bytes` has it; none where it has no copy there.
    pub async fn copy_cover_bytes(self: &Arc<Self>, album: &str, library: &str) -> Result<Option<(String, Vec<u8>)>> {
        let (me, album, library) = (self.clone(), album.to_string(), library.to_string());
        tokio::task::spawn_blocking(move || {
            let Some(folder) = me.folder_of_library(&library) else { return Ok(None) };
            let Some(copy) = me.library.copies_of(&album)?.into_iter().find(|c| c.folder == folder) else { return Ok(None) };
            let first = me.library.first_file_of(&album, folder, &copy.dir)?;
            me.art_at(folder, &copy.dir, first)
        })
        .await?
    }

    /// A record's copy in `library`, else with none named its copy waiting in Incoming, as (catalogue folder, folder in it).
    /// Blocking.
    pub fn waiting_copy(&self, album: &str, library: Option<&str>) -> Result<Option<(i64, String)>> {
        let copies = self.library.copies_of(album)?;
        let place = |c: &copies::Copy| (c.folder, c.dir.clone());
        match library {
            Some(l) => {
                let folder = self.folder_of_library(l);
                let copy = copies.iter().find(|c| Some(c.folder) == folder).ok_or_else(|| anyhow::anyhow!("It has no copy in that library"))?;
                Ok(Some(place(copy)))
            }
            None => Ok(copies.iter().find(|c| c.kind == copies::Kind::Incoming).map(place)),
        }
    }

    /// What a record's copy at (`folder`, `dir`) lacks: the row copy's are the library's, and another's only its own files can
    /// say, as `albums` holds the row copy's facts. None where it lacks nothing, or is not there. Blocking.
    pub fn lacks_at(&self, album: &str, folder: i64, dir: &str) -> Result<Option<library::Lacks>> {
        if self.library.row_copy(album)?.is_some_and(|c| c.folder == folder && c.dir == dir) {
            return self.library.lacks_of(album);
        }
        let Some((_, artist, name)) = self.library.album_names(album)? else { return Ok(None) };
        let tags: Vec<crate::naming::Tags> = self.catalogue().album_tags(folder, dir)?.into_iter().map(|(_, t)| crate::naming::Tags(t)).collect();
        let on_all = |n: &str| tags.iter().all(|t| t.first(n).is_some());
        let on_any = |n: &str| tags.iter().any(|t| t.first(n).is_some());
        let first = self.library.first_file_of(album, folder, dir)?;
        let lacks = library::Lacks {
            id: album.to_string(),
            name,
            artist,
            cover: self.art_at(folder, dir, first)?.is_none(),
            year: !tags.iter().any(|t| t.release_year().is_some()),
            genre: !on_any("genre"),
            album_artist: !on_all("albumartist"),
            track_no: !on_all("tracknumber"),
            musicbrainz: !on_any("musicbrainz_albumid"),
        };
        Ok(lacks.any().then_some(lacks))
    }

    /// The catalogue folder of one of Cratarr's own libraries: a library's id is its folder.
    pub fn folder_of_library(&self, library: &str) -> Option<i64> {
        self.library.roots().into_iter().find(|(_, r)| std::path::Path::new(r) == std::path::Path::new(library)).map(|(f, _)| f)
    }

    /// The record a song is in; none for a song not held. Blocking.
    pub fn album_of_song(&self, song: &str) -> Option<String> {
        self.library.songs_by_id(&[song.to_string()]).ok()?.pop().map(|s| s.album_id)
    }

    /// A record's cover at most `size` pixels on its longer side, for a list or a grid (finding 86; the player and a panel keep the
    /// whole one): made once, then from the disk cache, which a Subsonic client's sizes share. Its print is the library's word, so
    /// a size made before costs no read of the picture, and the original is read only at the resizer's turn: a grid's asks waiting
    /// for it hold nothing (finding 87). A record with no print yet is made each time, as nothing could tell its cache entry stale.
    pub async fn sized_cover(self: &Arc<Self>, album: &str, size: u32) -> Result<Option<(String, Vec<u8>)>> {
        let key = self.cover_stamps().get(album).map(|print| format!("{album}:{print}:{size}"));
        if let Some(small) = key.as_deref().and_then(|k| self.art_cache.get(k)) {
            return Ok(Some(("image/jpeg".to_string(), small)));
        }
        let _turn = self.resizing.acquire().await?;
        // Made while this one waited.
        if let Some(small) = key.as_deref().and_then(|k| self.art_cache.get(k)) {
            return Ok(Some(("image/jpeg".to_string(), small)));
        }
        let (me, album) = (self.clone(), album.to_string());
        tokio::task::spawn_blocking(move || -> Result<Option<(String, Vec<u8>)>> {
            let Some((ct, bytes, _)) = me.art_of(&album)? else { return Ok(None) };
            // A picture the resizer cannot read, or one small enough already, goes as it is.
            Ok(Some(match art::resize(&bytes, size).ok().flatten() {
                Some(small) => {
                    if let Some(key) = &key {
                        me.art_cache.put(key, &small);
                    }
                    ("image/jpeg".to_string(), small)
                }
                None => (ct, bytes),
            }))
        })
        .await?
    }

    /// An album's (or a song's, or an artist's) cover, at most `size` pixels on its longer side when asked. An id with
    /// the current fingerprint may be kept for good; any other is to be checked again (Navidrome's rule, see `art`).
    pub async fn cover(self: &Arc<Self>, id: &str, size: Option<u32>, headers: &HeaderMap) -> Result<Response> {
        let (me, want) = (self.clone(), id.to_string());
        let (album, asked) = tokio::task::spawn_blocking(move || me.art_album(&want)).await??;
        let Some(album) = album else { return Ok(StatusCode::NOT_FOUND.into_response()) };
        // The library's word on its print: a 304 costs no read of the picture (finding 87).
        let print = self.cover_stamps().get(&album).cloned().unwrap_or_default();
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
            return Ok((StatusCode::NOT_MODIFIED, head("image/jpeg")).into_response());
        }
        let found = match size {
            Some(size) => self.sized_cover(&album, size).await?,
            None => self.cover_bytes(&album).await?,
        };
        Ok(match found {
            Some((ct, bytes)) => (head(&ct), bytes).into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::copies::Kind;
    use crate::userdata::{Heard, Kind as Item};
    use std::path::Path;

    /// A server on two kept folders with a catalogue of them, for a test named `name`.
    fn on_two_folders(name: &str) -> ([PathBuf; 2], Arc<Catalogue>, Arc<Serve>) {
        let roots = [0, 1].map(|n| std::env::temp_dir().join(format!("cratarr-serve-{name}-{n}-{}", std::process::id())));
        for root in &roots {
            std::fs::remove_dir_all(root).ok();
            std::fs::create_dir_all(root).unwrap();
        }
        let cat = Arc::new(Catalogue::open(None, roots.to_vec()).unwrap());
        let serve = Serve::open(None, Rules::default(), &|_| None).unwrap();
        serve.library.set_kinds(vec![(0, Kind::Main), (1, Kind::Main)]);
        serve.attach_catalogue(cat.clone());
        (roots, cat, serve)
    }

    /// These files, (path, title), of one album, as the catalogue holds them once read and identified.
    fn add(cat: &Catalogue, root: &Path, folder: i64, files: &[(&str, &str)]) {
        for (rel, _) in files {
            std::fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
            std::fs::write(root.join(rel), rel).unwrap();
        }
        cat.walk_all();
        cat.pretend_read_for_tests();
        for (rel, title) in files {
            cat.tag_for_tests(folder, rel, &[("album", "Album"), ("albumartist", "Artist"), ("title", title)]);
        }
        cat.identify().unwrap();
    }

    /// A check that finds something new: `last_pass` is in whole seconds, and a check at the second of the last sees nothing.
    fn checked_again(serve: &Serve) {
        std::thread::sleep(Duration::from_millis(1100));
        serve.catalogue().changed_outside_pass();
        serve.refresh().unwrap();
    }

    fn plays_of(serve: &Serve, song: &str) -> u32 {
        serve.userdata.get("me", Item::Song, &[song.to_string()]).unwrap().get(song).map_or(0, |a| a.play_count)
    }

    /// N1: `identify` hands a new file an id retired before there were records, plays and all, and the take then takes it. The
    /// plays go to the record's track at the same build, and nothing is left to follow.
    #[test]
    fn a_retired_id_with_plays_keeps_them_when_taken() {
        let (roots, cat, serve) = on_two_folders("retired");
        add(&cat, &roots[0], 0, &[("A/01 One.flac", "One")]);
        add(&cat, &roots[1], 1, &[("Old/01 One.flac", "One"), ("Stays/09 Stays.flac", "Stays")]);
        cat.tag_for_tests(1, "Stays/09 Stays.flac", &[("album", "Another"), ("albumartist", "Artist"), ("title", "Stays")]);
        let old = cat.track_of(1, "Old/01 One.flac").unwrap();
        let song = ids::song(old);
        serve.userdata.record("me", (&song, "p"), "al-x", &[], &Heard::counted(100)).unwrap();
        std::fs::remove_file(roots[1].join("Old/01 One.flac")).unwrap();
        cat.walk_all();
        cat.gone_long_ago_for_tests();
        cat.identify().unwrap();
        assert!(cat.track_of_source(crate::source::cratarr::KIND.id, &song).is_none(), "retired");

        serve.refresh().unwrap();
        let track = cat.track_of(0, "A/01 One.flac").unwrap();
        let kept = ids::song(track);
        serve.userdata.record("me", (&kept, "q"), "al-x", &[], &Heard::counted(200)).unwrap();

        add(&cat, &roots[1], 1, &[("Back/01 One.flac", "One")]);
        assert_eq!(cat.track_of(1, "Back/01 One.flac"), Some(old), "its name and length: the old id, with what it held");
        checked_again(&serve);
        assert_eq!(cat.track_of(1, "Back/01 One.flac"), Some(track), "taken");
        let a = serve.userdata.get("me", Item::Song, &[kept.clone()]).unwrap().remove(&kept).unwrap();
        assert_eq!((a.play_count, a.play_date), (2, Some(200)), "both plays, once each");
        assert_eq!(plays_of(&serve, &song), 0, "none left on the id that went");
        assert!(cat.tracks_to_follow().unwrap().is_empty(), "followed");
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// A5: a track on the Missing list is another by a person's word: its plays go to it at once, each once, and an old id in a queue
    /// plays what it chose.
    #[test]
    fn a_track_reconnected_by_hand_brings_its_plays_with_it() {
        let (roots, cat, serve) = on_two_folders("reconnected");
        add(&cat, &roots[0], 0, &[("A/01 One.flac", "One"), ("A/02 Two.flac", "Two")]);
        serve.refresh().unwrap();
        let (old, chosen) = (cat.track_of(0, "A/01 One.flac").unwrap(), cat.track_of(0, "A/02 Two.flac").unwrap());
        let (was, is) = (ids::song(old), ids::song(chosen));
        serve.userdata.record("me", (&was, "p"), "al-x", &[], &Heard::counted(100)).unwrap();
        serve.userdata.record("me", (&is, "q"), "al-x", &[], &Heard::counted(200)).unwrap();
        std::fs::remove_file(roots[0].join("A/01 One.flac")).unwrap();
        cat.walk_all();
        cat.gone_long_ago_for_tests();
        cat.identify().unwrap();
        cat.reconnect(old, chosen).unwrap();
        serve.refresh().unwrap();
        assert_eq!((plays_of(&serve, &is), plays_of(&serve, &was)), (2, 0), "both plays on the one chosen, none left");
        assert!(cat.tracks_to_follow().unwrap().is_empty(), "followed");
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// Q8: the copy with more tracks keeps what points at the record, even from a later folder. The two copies' songs are tagged
    /// alike, one persistent id, and a build must not move the plays to the copy that left by it.
    #[test]
    fn a_copy_that_leaves_takes_nothing_that_points_at_the_record() {
        let (roots, cat, serve) = on_two_folders("leaves");
        let two = [("A/01 One.flac", "One"), ("A/02 Two.flac", "Two")];
        add(&cat, &roots[0], 0, &two);
        add(&cat, &roots[1], 1, &two);
        serve.refresh().unwrap();
        let track = cat.track_of(1, "A/01 One.flac").unwrap();
        assert_eq!(cat.track_of(0, "A/01 One.flac"), Some(track));
        let song = ids::song(track);
        let (pid, _, _) = serve.library.play_targets(&song).unwrap().unwrap();
        serve.userdata.record("me", (&song, &pid), "al-x", &[], &Heard::counted(100)).unwrap();

        add(&cat, &roots[1], 1, &[("A/03 Three.flac", "Three")]);
        checked_again(&serve);
        let left = ids::song(cat.track_of(0, "A/01 One.flac").unwrap());
        assert_ne!(left, song, "the first folder's copy left");
        assert_eq!((plays_of(&serve, &song), plays_of(&serve, &left)), (1, 0), "the play stays on the record's track");
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// The same for copies tagged otherwise, each with a persistent id of its own: `pids` cannot tell them for one track, and the
    /// row copy's id goes with the copy that left. An id for life names its track, so what is on one no one lost stays.
    #[test]
    fn a_copy_tagged_otherwise_that_leaves_takes_nothing_that_points_at_the_record() {
        let (roots, cat, serve) = on_two_folders("leaves-tagged");
        let two = [("A/01 One.flac", "One"), ("A/02 Two.flac", "Two")];
        add(&cat, &roots[0], 0, &two);
        add(&cat, &roots[1], 1, &two);
        for (rel, title) in two {
            cat.tag_for_tests(0, rel, &[("album", "Album"), ("albumartist", "Artist"), ("title", title), ("date", "1999")]);
        }
        serve.refresh().unwrap();
        let track = cat.track_of(1, "A/01 One.flac").unwrap();
        assert_eq!(cat.track_of(0, "A/01 One.flac"), Some(track));
        let song = ids::song(track);
        let (pid, _, _) = serve.library.play_targets(&song).unwrap().unwrap();
        serve.userdata.record("me", (&song, &pid), "al-x", &[], &Heard::counted(100)).unwrap();

        add(&cat, &roots[1], 1, &[("A/03 Three.flac", "Three")]);
        checked_again(&serve);
        let left = ids::song(cat.track_of(0, "A/01 One.flac").unwrap());
        assert_ne!(left, song, "the first folder's copy left");
        assert_eq!((plays_of(&serve, &song), plays_of(&serve, &left)), (1, 0), "the play stays on the record's track");
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// An old id plays on, in a queue no take reached, and what is heard of it counts on the track that took it.
    #[test]
    fn a_play_on_a_taken_id_counts_on_the_track_that_took_it() {
        let (roots, cat, serve) = on_two_folders("heard-taken");
        let one = [("A/01 One.flac", "One")];
        add(&cat, &roots[0], 0, &one);
        add(&cat, &roots[1], 1, &one);
        serve.refresh().unwrap();
        let stays = ids::song(cat.track_of(0, "A/01 One.flac").unwrap());
        let old = ids::song(*cat.moved_tracks().unwrap().keys().next().expect("the second copy's id was taken"));
        assert!(crate::app::listening::heard_blocking(&serve, "me", &old, &Heard::counted(100)).unwrap(), "heard");
        assert_eq!((plays_of(&serve, &stays), plays_of(&serve, &old)), (1, 0));
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// An app's saved list can hold an old id too: a heart or a rating left on it sits on an id no list shows, and once followed
    /// nothing moves it.
    #[tokio::test]
    async fn a_heart_and_a_rating_on_a_taken_id_land_on_the_track_that_took_it() {
        let (roots, cat, serve) = on_two_folders("star-taken");
        let one = [("A/01 One.flac", "One")];
        add(&cat, &roots[0], 0, &one);
        add(&cat, &roots[1], 1, &one);
        serve.refresh().unwrap();
        let stays = ids::song(cat.track_of(0, "A/01 One.flac").unwrap());
        let old = ids::song(*cat.moved_tracks().unwrap().keys().next().expect("the second copy's id was taken"));
        serve.users.set("me", "a password").unwrap();
        let me = serve.users.get("me").unwrap();
        for (method, query) in [("star", format!("id={old}")), ("setRating", format!("id={old}&rating=4"))] {
            let said = routes::handle(&serve, &me, method, &routes::Params::parse(Some(&query), &[]), &HeaderMap::new()).await;
            assert!(said.is_ok(), "{method}");
        }
        let a = serve.userdata.get("me", Item::Song, &[stays.clone()]).unwrap().remove(&stays).unwrap_or_default();
        assert_eq!((a.starred_at.is_some(), a.rating), (true, 4));
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// What names a taken track moves once, whatever stops in between: the mark comes after, and going over it changes nothing.
    #[test]
    fn a_stop_before_the_mark_loses_nothing_and_counts_nothing_twice() {
        let (roots, cat, serve) = on_two_folders("mark");
        let one = [("A/01 One.flac", "One")];
        add(&cat, &roots[0], 0, &one);
        add(&cat, &roots[1], 1, &one);
        // Which of two new copies stays is the first album met, by its hash: both have a play to bring.
        for folder in [0, 1] {
            let song = ids::song(cat.track_of(folder, "A/01 One.flac").unwrap());
            serve.userdata.record("me", (&song, "p"), "al-x", &[], &Heard::counted(100)).unwrap();
        }
        serve.refresh().unwrap();
        let stays = ids::song(cat.track_of(0, "A/01 One.flac").unwrap());
        assert_eq!(cat.track_of(1, "A/01 One.flac"), ids::track_of(&stays));
        assert_eq!(plays_of(&serve, &stays), 2);
        // Stopped before the mark: the user data moved, the catalogue says it has not. Nothing changed, so no build will ask.
        let moved = cat.moved_tracks().unwrap().into_iter().collect::<Vec<_>>();
        cat.not_followed_for_tests();
        assert_eq!(cat.tracks_to_follow().unwrap(), moved);
        assert!(!serve.refresh().unwrap(), "nothing to build");
        assert_eq!(plays_of(&serve, &stays), 2, "not a play more");
        assert!(cat.tracks_to_follow().unwrap().is_empty(), "the next check went over it");
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// A library's folder renamed, every file as it was: only its path says the library is to be built again, and until it is
    /// nothing is listed there and no cover found, as both go by the folders it was built with.
    #[test]
    fn a_library_renamed_is_built_again_though_no_file_changed() {
        let (roots, cat, serve) = on_two_folders("renamed");
        add(&cat, &roots[1], 1, &[("A/01 One.flac", "One")]);
        checked_again(&serve);
        let renamed = roots[1].with_extension("renamed");
        std::fs::remove_dir_all(&renamed).ok();
        std::fs::rename(&roots[1], &renamed).unwrap();
        cat.set_folders(vec![roots[0].clone(), renamed.clone()]).unwrap();
        checked_again(&serve);
        assert!(serve.library.roots().contains(&(1, renamed.display().to_string())), "{:?}", serve.library.roots());
        std::fs::remove_dir_all(&roots[0]).ok();
        std::fs::remove_dir_all(&renamed).ok();
    }

    /// Two records tagged alike, a track more making the second one, are told apart by their ids alone. One's library taken away, a
    /// persistent id on one song reads as a rebuilt catalogue would, and a play went over to the other record and never came back.
    #[test]
    fn what_lies_on_a_track_away_with_its_library_stays_on_it() {
        let (roots, cat, serve) = on_two_folders("away");
        add(&cat, &roots[0], 0, &[("A/01 One.flac", "One")]);
        add(&cat, &roots[1], 1, &[("A/01 One.flac", "One"), ("A/02 Two.flac", "Two")]);
        checked_again(&serve);
        let song_in = |folder: i64| ids::song(cat.track_of(folder, "A/01 One.flac").unwrap());
        let (here, away) = (song_in(0), song_in(1));
        assert_ne!(here, away, "two records");
        crate::app::listening::heard_blocking(&serve, "me", &away, &Heard::counted(100)).unwrap();
        cat.set_folders(vec![roots[0].clone()]).unwrap();
        checked_again(&serve);
        assert_eq!((plays_of(&serve, &away), plays_of(&serve, &here)), (1, 0), "on its own track, and nothing on the other");
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    #[test]
    fn two_builds_inside_a_second_are_two_stamps() {
        let s = Serve::open(None, Rules::default(), &|_| None).unwrap();
        let (first, second) = (s.build_stamped(), s.build_stamped());
        assert!(second > first, "{first} then {second}");
        assert_eq!(s.built_at(), second);
    }

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

/// An album's whole release as the missing-tracks check kept it (`app::incomplete::Incomplete`, JSON): its track count,
/// and its length and longest track where known.
fn whole_of(found: &str) -> Option<library::Whole> {
    #[derive(serde::Deserialize)]
    struct Found {
        of: usize,
        #[serde(default)]
        length_ms: Option<u64>,
        #[serde(default)]
        longest_ms: Option<u64>,
    }
    let f: Found = serde_json::from_str(found).ok()?;
    Some(library::Whole { tracks: f.of, duration_ms: f.length_ms, longest_ms: f.longest_ms })
}
