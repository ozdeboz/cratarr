//! The library (decision 0011): the servers in use and where their music lies, the folders behind the libraries in
//! use, the Music folder's playlist store and the catalogue. The other services read it; changes to the servers go
//! through `switch_to`, one at a time.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::naming::Template;
use serde::Serialize;
use tokio::sync::Mutex;

use super::ToggleError;
use super::first_start::FirstStart;
use crate::catalogue::Catalogue;
use crate::placement::{self, Placement};
use crate::playlists::Store;
use crate::settings::{Folders, Libraries, LibraryKind, LibrarySetting, Quality, SourceConfig};
use crate::source::{self, Source};

/// One enabled source, for the page's source picker.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SourceTab {
    pub id: String,
    pub label: String,
    pub playing: bool,
}

pub(crate) fn epoch_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub(super) fn label_of(cfg: &SourceConfig) -> String {
    source::kind(&cfg.kind).map_or_else(|| cfg.kind.clone(), |k| k.label.to_string())
}

/// The catalogue's folders: the Music folder first (id 0), then the incoming ones.
fn catalogue_folders(folders: &Folders) -> Vec<PathBuf> {
    folders.all().into_iter().map(PathBuf::from).collect()
}

/// The folders mounted into the container, without the data folder.
fn mounted(data_dir: Option<&str>) -> Vec<String> {
    crate::mounts::list(&data_dir.into_iter().collect::<Vec<_>>())
}

/// Every folder a server's music is looked for in: the mounted ones, and the Music folder in use (which need not be a
/// mount point of its own).
fn place_folders(data_dir: Option<&str>, music: Option<&str>) -> Vec<String> {
    let mut out = mounted(data_dir);
    if let Some(m) = music {
        if !out.iter().any(|f| f == m) {
            out.push(m.to_string());
        }
    }
    out
}

/// Where the libraries in use keep their files, as Cratarr sees them (decision 0009). The Music folder is the first
/// one a Music library was found in; `last_music` stands in while no server has answered (or none has a Music
/// library yet).
pub(super) fn derive_folders(live: &[Arc<Active>], last_music: Option<&str>) -> Folders {
    let found = |a: &Active, ids: &[String]| -> Vec<String> {
        ids.iter()
            .filter_map(|id| a.placement.libraries.iter().find(|l| &l.id == id))
            .flat_map(|l| l.folders.clone())
            .collect()
    };
    let up: Vec<&Arc<Active>> = live.iter().filter(|a| a.source.is_some()).collect();
    let main = up.iter().flat_map(|a| found(a, &a.main())).collect();
    let own = up.iter().flat_map(|a| found(a, &a.of(LibraryKind::Own))).collect();
    let incoming = up.iter().flat_map(|a| found(a, &a.incoming())).collect();
    folders_of(main, own, incoming, last_music)
}

/// The catalogue's folders from the main and Incoming libraries' (0101 point 1: several main libraries, equal, the
/// first taking every fallback; `last_music` stands in while none has answered): each folder once, a main library's
/// never again as Incoming's.
fn folders_of(main: Vec<String>, own: Vec<String>, incoming: Vec<String>, last_music: Option<&str>) -> Folders {
    let mut seen = std::collections::HashSet::new();
    let main: Vec<String> = main.into_iter().filter(|f| seen.insert(f.clone())).collect();
    let music = main.first().cloned().or_else(|| last_music.map(str::to_string));
    let more_main: Vec<String> = main.into_iter().filter(|f| Some(f) != music.as_ref()).collect();
    let own: Vec<String> = own.into_iter().filter(|f| seen.insert(f.clone())).collect();
    let mut incoming: Vec<String> = incoming.into_iter().filter(|f| Some(f) != music.as_ref() && !more_main.contains(f) && !own.contains(f)).collect();
    incoming.sort();
    incoming.dedup();
    Folders { music, more_main, own, incoming }
}

/// The folders a server's libraries switched off leave out (decision 0024, amended 2026-10-23: a crate draws from
/// every library switched on). `without`: `source/library` keys, as the page's library switches keep them; only this
/// server's count. A folder is left out when every library of the server in use that keeps files there (as
/// `derive_folders` finds them) is switched off; a folder none of them keeps stays.
pub(super) fn folders_switched_off(a: &Active, without: &[String]) -> Vec<String> {
    let off = |library: &str| without.iter().any(|k| k.strip_prefix(a.cfg.id.as_str()).and_then(|r| r.strip_prefix('/')) == Some(library));
    let (mut on, mut out) = (std::collections::BTreeSet::new(), std::collections::BTreeSet::new());
    for l in a.libraries().iter().filter_map(|u| a.placement.libraries.iter().find(|l| l.id == u.id)) {
        let side = if off(&l.id) { &mut out } else { &mut on };
        side.extend(l.folders.iter().cloned());
    }
    out.into_iter().filter(|f| !on.contains(f)).collect()
}

/// A configured source and, when it is enabled and its settings work, the connection and where its music lies.
pub(super) struct Active {
    pub(super) cfg: SourceConfig,
    pub(super) source: Option<Arc<dyn Source>>,
    pub(super) placement: Placement,
    pub(super) error: Option<String>,
}

/// A library in use (0101 point 1), as every reader takes it (`Active::libraries`). `owner`: own music's account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Used {
    pub(super) id: String,
    pub(super) kind: LibraryKind,
    pub(super) owner: Option<String>,
}

impl Active {
    /// The libraries in use, in order: the settings' list, then (Cratarr's own source) each account's own music (0101
    /// point 9, from the own-music folder). What a library is, every reader asks here, never the list itself.
    pub(super) fn libraries(&self) -> Vec<Used> {
        let mut used: Vec<Used> = self.cfg.libraries.0.iter().map(|l| Used { id: l.id.clone(), kind: l.kind, owner: None }).collect();
        if let Some(own) = self.cfg.own_music() {
            used.extend(self.placement.libraries.iter().filter(|l| std::path::Path::new(&l.id).parent() == Some(own.as_path())).map(|l| Used {
                id: l.id.clone(),
                kind: LibraryKind::Own,
                owner: std::path::Path::new(&l.id).file_name().map(|n| n.to_string_lossy().into_owned()),
            }));
        }
        used
    }

    /// The ids of the libraries of `kind`, in order.
    pub(super) fn of(&self, kind: LibraryKind) -> Vec<String> {
        self.libraries().into_iter().filter(|u| u.kind == kind).map(|u| u.id).collect()
    }

    /// The main libraries' ids, in order: the first takes every fallback (0101 point 1).
    pub(super) fn main(&self) -> Vec<String> {
        self.of(LibraryKind::Main)
    }

    /// The Incoming libraries' ids, in order.
    pub(super) fn incoming(&self) -> Vec<String> {
        self.of(LibraryKind::Incoming)
    }

    /// Where `account`'s own music lies, made or not, as a library's id; none without an own-music folder.
    pub(super) fn own_of(&self, account: &str) -> Option<String> {
        self.cfg.own_music().map(|own| own.join(account).display().to_string())
    }

    /// The libraries whose records are kept: the main ones and everyone's own music, out of a fetcher's reach.
    pub(super) fn kept(&self) -> Vec<String> {
        self.libraries().into_iter().filter(|u| u.kind != LibraryKind::Incoming).map(|u| u.id).collect()
    }

    /// What library `id` is; none for one not in use.
    pub(super) fn kind_of(&self, id: &str) -> Option<LibraryKind> {
        self.libraries().into_iter().find(|u| u.id == id).map(|u| u.kind)
    }

    /// Whether library `id` is in use.
    pub(super) fn used(&self, id: &str) -> bool {
        self.kind_of(id).is_some()
    }

    /// The quality limit a move into `library` follows: its own, else its kind's; none for one not in use.
    pub(super) fn quality_of(&self, library: &str) -> Quality {
        self.cfg.libraries.get(library).and_then(|l| l.quality).or_else(|| self.kind_of(library).map(LibraryKind::quality)).unwrap_or(Quality::None)
    }

    /// The dot of `library`'s albums (`settings::colour_among`).
    pub(super) fn colour_of(&self, library: &str) -> Option<String> {
        let used = self.libraries();
        crate::settings::colour_among(used.iter().map(|u| (u.id.as_str(), u.kind)), library, self.cfg.libraries.get(library).and_then(|l| l.colour.as_deref()))
    }

    pub(super) async fn start(cfg: SourceConfig, folders: &[String]) -> Self {
        if !cfg.enabled {
            return Self { cfg, source: None, placement: Placement::default(), error: None };
        }
        let connected = source::kind(&cfg.kind)
            .ok_or_else(|| anyhow::anyhow!("unknown kind of source {:?}", cfg.kind))
            .and_then(|k| (k.connect)(&cfg.values, false));
        let source: Arc<dyn Source> = match connected {
            Ok(s) => Arc::from(s),
            Err(e) => {
                tracing::warn!("source {}: {e:#}", cfg.id);
                return Self { cfg, source: None, placement: Placement::default(), error: Some(format!("{e:#}")) };
            }
        };
        let placement = placement::place(&*source, folders, &cfg.libraries).await.unwrap_or_else(|e| {
            tracing::warn!("source {}: cannot look at its libraries yet: {e:#}", cfg.id);
            Placement::default()
        });
        if placement.roots.is_empty() {
            tracing::warn!("source {}: none of its music found in Cratarr's folders (yet)", cfg.id);
        }
        Self { cfg, source: Some(source), placement, error: None }
    }

    /// The playlist line for a track: its path in the Music folder, if it lies there.
    pub(super) fn entry(&self, music: Option<&str>, track: &source::Track) -> Option<String> {
        self.placement.entry(music?, track.path.as_deref())
    }

    /// A track's playlist line, for any track Cratarr can see, in whichever library it lies: its path in the folder lines are
    /// made from (`InUse::first`), or for one elsewhere its path from there ("../lidarr-music/X/01.flac"). Keep and Replace move
    /// such a line along with the file, and Delete takes it out (decision 0011 §6).
    pub(super) fn line(&self, folders: &crate::catalogue::InUse, track: &source::Track) -> Option<String> {
        let (_, first) = folders.first()?;
        folders.iter().find_map(|(_, f)| {
            let rel = self.placement.entry(&f.to_string_lossy(), track.path.as_deref())?;
            crate::playlists::relative_to(first, &f.join(rel))
        })
    }
}

/// The user data's keys for the catalogue its track ids are of, and the highest id given out (0101).
const CATALOGUE_KEY: &str = "catalogue";
const TOP_KEY: &str = "catalogue_top";

/// What the user data remembers of the catalogue, against the catalogue open now.
#[derive(Debug, PartialEq, Eq)]
enum Ids {
    /// Nothing yet (a first start, or 0.16's first): this one is remembered.
    Remember,
    Same,
    /// Another catalogue: the ids the user data holds are not this one's.
    Lost,
}

fn ids_state(remembered: Option<&str>, generation: &str) -> Ids {
    match remembered {
        None => Ids::Remember,
        Some(g) if g == generation => Ids::Same,
        Some(_) => Ids::Lost,
    }
}

/// The highest record or artist id for life the JSON stores in the data folder name: crates, everyone's, the players'
/// queues and the jobs waiting. A store that cannot be read names nothing.
fn top_in_stores(data: &Path) -> i64 {
    let mut stores: Vec<PathBuf> = ["crates.json", "queues.json", "convert-queue.json", "track-intake.json"].into_iter().map(|f| data.join(f)).collect();
    if let Ok(users) = std::fs::read_dir(data.join("users")) {
        stores.extend(users.flatten().map(|u| u.path().join("crates.json")));
    }
    stores.iter().filter_map(|f| std::fs::read_to_string(f).ok()).map(|text| crate::serve::ids::highest_named(&text)).max().unwrap_or(0)
}

/// A fresh start of lost track ids: where the new ids begin, and what still names an old track (rows per table).
#[derive(Debug, serde::Serialize)]
pub struct FreshIds {
    pub from: i64,
    pub references: Vec<(String, u64)>,
}

pub struct Library {
    live: std::sync::RwLock<Arc<Vec<Arc<Active>>>>,
    data_dir: Option<String>,
    /// Where the libraries in use keep their files, and the playlists of the Music folder (swapped in whole when
    /// either changes).
    folders: std::sync::RwLock<(Folders, Option<Arc<Store>>)>,
    /// The other accounts' playlists (decision 0051), by account, as opened (each in the Music folder of its day).
    account_stores: std::sync::RwLock<std::collections::HashMap<String, Arc<Store>>>,
    /// Where the playlists are kept (decision 0101): the user data.
    playlists_db: Arc<crate::userdata::UserData>,
    /// Every audio file Cratarr can see, and the servers' ids for them (decision 0008).
    catalogue: Arc<Catalogue>,
    /// One change to the servers at a time, so two pages (or the background placement retry) cannot interleave.
    pub(super) changing: Mutex<()>,
    /// How tag values are split when read (decision 0008); saved with the sources.
    tag_rules: std::sync::RwLock<crate::catalogue::Rules>,
    /// The naming template of Music (decision 0013): (saved, the one the library follows once worked out).
    naming: std::sync::RwLock<(Option<Template>, Option<Template>)>,
    /// Servers that stopped processing changes, and their library scans as last seen (see `watch_servers`).
    watch: std::sync::Mutex<Watch>,
}

/// A server that stopped processing changes (Jellyfin, 2026-10-13: after a playing file vanished, its library work
/// hung until it was restarted; no new music appeared). Noticed from what Cratarr sees, never by putting load on it:
/// its library scan not moving for `STALL_SECS`, or moved files never showing up (`server_stuck`). Cleared once a
/// library scan finishes after it was noticed (a restart ends the stuck one; the next finishes), or after `STUCK_SECS`.
#[derive(Default)]
struct Watch {
    /// By source: (since, what was seen).
    stuck: std::collections::HashMap<String, (i64, String)>,
    /// By source: the running scan's progress and since when it is at that.
    scans: std::collections::HashMap<String, (f64, i64)>,
}

/// A library scan at the same progress this long has stopped.
const STALL_SECS: i64 = 10 * 60;
/// Where the playlists' `.m3u` copies are written (decision 0101): `playlists` in Cratarr's data folder.
fn playlist_copies(data_dir: Option<&str>) -> Option<PathBuf> {
    data_dir.map(|d| PathBuf::from(d).join("playlists"))
}

/// The folder playlist lines are made from, where there is a Music folder to keep playlists over at all: the lowest in use
/// (`InUse::first`), and not the first main library, which goes by the list's order: a main library put first moves no line.
fn lines_folder(catalogue: &Catalogue, folders: &Folders) -> Option<String> {
    folders.music.as_ref()?;
    catalogue.folders().first().map(|(_, path)| path.display().to_string())
}

/// What names a library by its path follows it (`Catalogue::paths_to_follow`): a library switched off stays off, however it
/// came to another path. The catalogue's word stands until the user data has it, so a stop between them finishes at the next
/// start. Blocking.
fn follow_paths(catalogue: &Catalogue, userdata: &crate::userdata::UserData) -> Result<()> {
    let moved = catalogue.paths_to_follow()?;
    if !moved.is_empty() {
        tracing::info!("libraries changed path: {moved:?}");
        userdata.follow_library_paths(&moved)?;
        catalogue.paths_followed(&moved)?;
    }
    Ok(())
}

/// The first administrator's playlists over the Music folder (none without one), its `.m3u` files brought in first
/// (once). Ones that could not be brought in are tried again at the next start: the store opens meanwhile, so the rest
/// of Cratarr does.
async fn music_store(db: &Arc<crate::userdata::UserData>, cat: &Arc<Catalogue>, data_dir: Option<&str>, music: Option<&str>) -> Option<Arc<Store>> {
    let music = music?;
    let out = playlist_copies(data_dir);
    match Store::in_music(db.clone(), cat.clone(), music, out.clone()).await {
        Ok(s) => Some(Arc::new(s)),
        Err(e) => {
            tracing::error!("the playlists in {music} could not be brought in: {e:#}");
            Some(Arc::new(Store::open(db.clone(), cat.clone(), None, music, out)))
        }
    }
}

/// A "stopped processing" notice that nothing confirmed again is dropped after this long.
const STUCK_SECS: i64 = 30 * 60;

/// `Library::folder_kinds`, of these folders.
fn kinds_of(f: &Folders, in_use: &crate::catalogue::InUse) -> Vec<(i64, LibraryKind)> {
    let main = f.music.iter().chain(&f.more_main).map(|p| (p, LibraryKind::Main));
    let listed = main.chain(f.own.iter().map(|p| (p, LibraryKind::Own))).chain(f.incoming.iter().map(|p| (p, LibraryKind::Incoming)));
    listed.filter_map(|(p, kind)| in_use.iter().find(|(_, q)| q.as_path() == Path::new(p)).map(|(id, _)| (id, kind))).collect()
}

impl Library {
    /// `last_music`: where the Music libraries were last found (or CRATARR_PLAYLIST_DIR), used until a server answers.
    /// `playlists_db`: where the playlists are kept (the user data).
    pub(super) async fn new(
        data_dir: Option<String>,
        last_music: Option<String>,
        sources: Vec<SourceConfig>,
        tag_rules: crate::catalogue::Rules,
        naming: Option<String>,
        playlists_db: Arc<crate::userdata::UserData>,
    ) -> Result<Arc<Self>> {
        let naming = naming.and_then(|t| {
            t.parse::<Template>().inspect_err(|e| tracing::warn!("the saved naming template is not usable ({e}); using the library's")).ok()
        });
        let places = place_folders(data_dir.as_deref(), last_music.as_deref());
        let mut active = Vec::new();
        for s in sources {
            active.push(Active::start(s, &places).await);
        }
        // First run: no library chosen yet on any server, so every library each one has is a main library (0101 point
        // 1), in its order (shown so on the Settings page, kept once saved).
        if active.iter().all(|a| a.cfg.libraries.0.is_empty()) {
            for a in active.iter_mut().filter(|a| a.source.is_some()) {
                a.cfg.libraries = Libraries(a.placement.libraries.iter().map(|l| LibrarySetting::new(l.id.clone(), LibraryKind::Main)).collect());
            }
        }
        let active: Vec<Arc<Active>> = active.into_iter().map(Arc::new).collect();
        let folders = derive_folders(&active, last_music.as_deref());
        let db = data_dir.as_ref().map(|d| PathBuf::from(d).join("catalogue.sqlite"));
        let catalogue = Arc::new(Catalogue::open(db.as_deref(), catalogue_folders(&folders))?);
        // The catalogue's track ids are the truth (0101, the owner 2026-11-04): the user data remembers which catalogue
        // its ids are of, and the highest id given out. Another catalogue (a lost one made anew) gives no ids until it is
        // restored from the backup or an administrator starts afresh (`fresh_track_ids`).
        let state = match ids_state(playlists_db.meta(CATALOGUE_KEY)?.as_deref(), &catalogue.generation()) {
            // 0.15 remembered none, so a catalogue lost there reads as a first start: one that never gave an id, beside user
            // data that names them. Taken for new, the first start would write everything off as unmatched, for good.
            Ids::Remember if catalogue.top_id() == 0 && !playlists_db.track_references()?.0.is_empty() => Ids::Lost,
            state => state,
        };
        match state {
            Ids::Remember => playlists_db.set_meta(CATALOGUE_KEY, &catalogue.generation())?,
            // One restored from a backup has not given what it gave since, and those ids are named: its next ones start above them.
            Ids::Same => {
                let top: i64 = playlists_db.meta(TOP_KEY)?.and_then(|v| v.parse().ok()).unwrap_or(0);
                if catalogue.top_id() < top {
                    tracing::warn!("catalogue: older than the user data, which remembers id {top}: its next ids start above it");
                    catalogue.number_above(top)?;
                }
            }
            Ids::Lost => {
                tracing::error!("catalogue: its track ids were lost (catalogue.sqlite made anew): no ids until it is restored or started afresh");
                catalogue.set_ids_lost(true);
            }
        }
        // Here, as nothing has read a store yet, and no list has been built: `playlists::import` below reads what it leaves.
        let kinds = kinds_of(&folders, &catalogue.folders());
        let start = FirstStart { catalogue: catalogue.clone(), userdata: playlists_db.clone(), data: data_dir.as_deref().map(PathBuf::from), kinds, rules: tag_rules.clone() };
        start.run().await.context("the first start of 0.16 did not finish")?;
        follow_paths(&catalogue, &playlists_db).context("following the libraries' paths")?;
        if !catalogue.ids_lost() {
            playlists_db.set_meta(TOP_KEY, &catalogue.top_id().to_string())?;
        }
        let store = music_store(&playlists_db, &catalogue, data_dir.as_deref(), lines_folder(&catalogue, &folders).as_deref()).await;
        Ok(Arc::new(Self {
            live: std::sync::RwLock::new(Arc::new(active)),
            data_dir,
            folders: std::sync::RwLock::new((folders, store)),
            account_stores: std::sync::RwLock::default(),
            playlists_db,
            catalogue,
            changing: Mutex::new(()),
            tag_rules: std::sync::RwLock::new(tag_rules),
            naming: std::sync::RwLock::new((naming, None)),
            watch: std::sync::Mutex::new(Watch::default()),
        }))
    }

    /// A server stopped processing changes: moved files never showed up in it, say. The page says so (`problems`).
    pub fn server_stuck(&self, source: &str, what: &str) {
        tracing::warn!("{source} has stopped processing changes: {what}");
        let mut w = self.watch.lock().unwrap_or_else(|p| p.into_inner());
        w.stuck.entry(source.to_string()).or_insert((epoch_now(), what.to_string()));
    }

    /// What the page should say about the servers, one line each.
    pub fn problems(&self) -> Vec<String> {
        let w = self.watch.lock().unwrap_or_else(|p| p.into_inner());
        let label = |id: &str| self.live().iter().find(|a| a.cfg.id == id).map(|a| label_of(&a.cfg)).unwrap_or_else(|| id.to_string());
        w.stuck.keys().map(|id| format!("{} has stopped processing changes. Restarting it fixes this.", label(id))).collect()
    }

    /// Look at each server's library scan (one cheap request each): one at the same progress for `STALL_SECS` has
    /// stopped; a scan that finished after a notice clears it.
    pub(super) async fn watch_servers(&self) {
        let now = epoch_now();
        for (id, s) in self.connected_sources() {
            let Ok(scan) = s.library_scan().await else { continue };
            let mut w = self.watch.lock().unwrap_or_else(|p| p.into_inner());
            let stalled = match scan.running {
                Some(p) => {
                    let (at, since) = *w.scans.entry(id.clone()).and_modify(|e| if e.0 != p { *e = (p, now) }).or_insert((p, now));
                    let _ = at;
                    now - since >= STALL_SECS
                }
                None => {
                    w.scans.remove(&id);
                    false
                }
            };
            if stalled {
                if !w.stuck.contains_key(&id) {
                    tracing::warn!("{id} has stopped processing changes: its library scan has not moved for {} min", STALL_SECS / 60);
                    w.stuck.insert(id.clone(), (now, "its library scan stopped moving".into()));
                }
            } else if let Some(&(since, _)) = w.stuck.get(&id)
                && (scan.last_end.is_some_and(|end| end > since) || now - since > STUCK_SECS)
            {
                tracing::info!("{id} processes changes again");
                w.stuck.remove(&id);
            }
        }
    }

    /// How Keep and Replace name an album in Music (decision 0013): the saved template, else the one the library
    /// already follows (worked out in the background, see `SettingsService::infer_naming`), else the default.
    pub fn template(&self) -> Template {
        let (saved, inferred) = &*self.naming.read().unwrap_or_else(|p| p.into_inner());
        saved.clone().or_else(|| inferred.clone()).unwrap_or_default()
    }

    /// (saved, inferred).
    pub(super) fn templates(&self) -> (Option<Template>, Option<Template>) {
        self.naming.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub(super) fn set_saved_template(&self, t: Option<Template>) {
        self.naming.write().unwrap_or_else(|p| p.into_inner()).0 = t;
    }

    pub(super) fn set_inferred_template(&self, t: Template) {
        self.naming.write().unwrap_or_else(|p| p.into_inner()).1 = Some(t);
    }

    pub(super) fn tag_rules(&self) -> crate::catalogue::Rules {
        self.tag_rules.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub(super) fn set_tag_rules(&self, rules: crate::catalogue::Rules) {
        *self.tag_rules.write().unwrap_or_else(|p| p.into_inner()) = rules;
    }

    /// Starts the ids afresh after they were lost (the owner's choice for a lost catalogue.sqlite, 0101): the new
    /// catalogue's tracks and records become the truth, numbered from the next million above every old id the user data
    /// remembers or names, in its tables and its JSON stores (an old id then names something missing, never something
    /// else). The maps of ids that moved went with the lost file, so nothing leads an old id to its new one. What names an
    /// old track is counted. Blocking; the administrators' (the API's guard).
    pub fn fresh_track_ids(&self) -> Result<FreshIds> {
        let ud = &self.playlists_db;
        let (references, named) = ud.track_references()?;
        let remembered: i64 = ud.meta(TOP_KEY)?.and_then(|v| v.parse().ok()).unwrap_or(0);
        let stores = self.data_dir.as_deref().map_or(0, |d| top_in_stores(Path::new(d)));
        let floor = (remembered.max(named).max(ud.top_named()?).max(stores) / 1_000_000 + 1) * 1_000_000;
        self.catalogue.number_above(floor)?;
        // Before the first start its records are still to come, and a catalogue saying they were given would leave every row unmatched.
        if ud.first_start_done()? {
            self.catalogue.first_records_over()?;
        }
        ud.set_meta(CATALOGUE_KEY, &self.catalogue.generation())?;
        ud.set_meta(TOP_KEY, &floor.to_string())?;
        self.catalogue.set_ids_lost(false);
        self.catalogue.poke();
        tracing::warn!("catalogue: track ids started afresh from {}", floor + 1);
        Ok(FreshIds { from: floor + 1, references: references.into_iter().map(|(table, rows)| (table.to_string(), rows)).collect() })
    }

    pub fn catalogue(&self) -> Arc<Catalogue> {
        self.catalogue.clone()
    }

    /// The connected servers by id, for the catalogue's background pass.
    pub fn connected_sources(&self) -> Vec<(String, Arc<dyn Source>)> {
        self.connected().into_iter().map(|(a, s)| (a.cfg.id.clone(), s)).collect()
    }

    /// Whether a server of this kind (`source::*::KIND.id`) is set up and switched on, answering or not (0.11, decision
    /// 0085: the capability list).
    pub(super) fn kind_on(&self, kind: &str) -> bool {
        self.live().iter().any(|a| a.cfg.enabled && a.cfg.kind == kind)
    }

    /// The libraries of the settings' list that their server, answering, does not list (`source/library`): the
    /// migration report's (decision 0101).
    pub fn unlisted_libraries(&self) -> Vec<String> {
        let mut out = Vec::new();
        for a in self.live().iter().filter(|a| a.source.is_some() && !a.placement.libraries.is_empty()) {
            out.extend(a.libraries().into_iter().filter(|u| !a.placement.libraries.iter().any(|p| p.id == u.id)).map(|u| format!("{}/{}", a.cfg.id, u.id)));
        }
        out
    }

    /// Some enabled server has an Incoming library: the header's `incoming` and the capability list's.
    pub(super) fn incoming_on(&self) -> bool {
        self.live().iter().any(|a| a.cfg.enabled && !a.incoming().is_empty())
    }

    /// The ids of Cratarr's own libraries (sources of its own kind): where its favourites are.
    pub fn own_sources(&self) -> Vec<String> {
        self.live().iter().filter(|a| a.cfg.kind == crate::source::cratarr::KIND.id).map(|a| a.cfg.id.clone()).collect()
    }

    /// Cratarr's own chosen folders (decision 0044), as saved.
    pub fn chosen_folders(&self) -> Vec<String> {
        self.live()
            .iter()
            .filter(|a| a.cfg.kind == crate::source::cratarr::KIND.id)
            .flat_map(|a| a.cfg.values.get("folders").and_then(|v| v.as_array()).cloned().unwrap_or_default())
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    }

    pub(super) fn live(&self) -> Arc<Vec<Arc<Active>>> {
        self.live.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// The catalogue folders of the libraries switched off (`without`, `<source>/<library>` keys as the pages keep them),
    /// by their place in it: what a list from Cratarr's own library leaves out.
    pub fn folders_off(&self, without: &[String]) -> Vec<i64> {
        if without.is_empty() {
            return Vec::new();
        }
        let off: Vec<std::path::PathBuf> = self.connected().into_iter().flat_map(|(a, _)| folders_switched_off(&a, without)).map(std::path::PathBuf::from).collect();
        self.catalogue().folders().iter().filter(|(_, f)| off.contains(f)).map(|(i, _)| i).collect()
    }

    pub(super) fn connected(&self) -> Vec<(Arc<Active>, Arc<dyn Source>)> {
        self.live().iter().filter_map(|a| a.source.clone().map(|s| (a.clone(), s))).collect()
    }

    pub(super) fn folders(&self) -> Folders {
        self.folders.read().unwrap_or_else(|p| p.into_inner()).0.clone()
    }

    pub(super) fn music(&self) -> Option<String> {
        self.folders.read().unwrap_or_else(|p| p.into_inner()).0.music.clone()
    }

    /// The catalogue's folders that are the main libraries', by their ids (0101 point 1: main libraries are equal; a
    /// folder's id says nothing of its kind).
    pub(super) fn main_folders(&self) -> Vec<i64> {
        let f = self.folders();
        let main: Vec<&std::path::Path> = f.music.iter().chain(&f.more_main).map(std::path::Path::new).collect();
        self.catalogue.folders().iter().filter(|(_, p)| main.contains(&p.as_path())).map(|(id, _)| id).collect()
    }

    /// Whether the catalogue's folder `folder` is a main library's: Keep's target, Replace's, out of Lidarr's reach.
    pub(super) fn is_main(&self, folder: i64) -> bool {
        self.main_folders().contains(&folder)
    }

    /// The catalogue folders whose records are kept (`Active::kept`): the main libraries' and own music's.
    pub(super) fn kept_folders(&self) -> Vec<i64> {
        let f = self.folders();
        let kept: Vec<&std::path::Path> = f.music.iter().chain(&f.more_main).chain(&f.own).map(std::path::Path::new).collect();
        self.catalogue.folders().iter().filter(|(_, p)| kept.contains(&p.as_path())).map(|(id, _)| id).collect()
    }

    /// Whether a catalogue folder's records are kept: not an Incoming library's.
    pub(super) fn is_kept(&self, folder: i64) -> bool {
        self.kept_folders().contains(&folder)
    }

    /// `account`'s own music's catalogue folder, while it is in use. Read only, and on every stream: `own_library` takes the
    /// lock of those who change the libraries, and makes the folder.
    pub fn own_folder(&self, account: &str) -> Option<i64> {
        self.live().iter().find_map(|a| {
            let used = a.libraries().into_iter().find(|u| u.kind == LibraryKind::Own && u.owner.as_deref() == Some(account))?;
            let place = a.placement.libraries.iter().find(|l| l.id == used.id)?.folders.first()?.clone();
            self.catalogue.folders().iter().find(|(_, p)| p.as_path() == std::path::Path::new(&place)).map(|(id, _)| id)
        })
    }

    /// What each catalogue folder's library is, in the libraries' order: the main ones, own music, Incoming. What the
    /// served library judges a copy by.
    pub fn folder_kinds(&self) -> Vec<(i64, LibraryKind)> {
        kinds_of(&self.folders(), &self.catalogue.folders())
    }

    /// `account`'s own music as a library, its id (0101 point 9): its subfolder of the own-music folder, made at its first
    /// use, when it starts off for everyone else (0101 point 6). None while no own-music folder is set.
    pub(super) async fn own_library(&self, account: &str) -> Result<Option<String>> {
        let _one_at_a_time = self.changing.lock().await;
        let live = self.live();
        let Some(at) = live.iter().position(|a| a.cfg.own_music().is_some()) else { return Ok(None) };
        let a = &live[at];
        let own = a.cfg.own_music().unwrap_or_default();
        anyhow::ensure!(placement::safe_relative(account) && !account.contains(['/', '\\']) && !account.starts_with('.'), "{account:?} can't name a folder");
        let folder = own.join(account);
        let id = folder.display().to_string();
        if a.used(&id) {
            return Ok(Some(id));
        }
        let Some(source) = a.source.clone() else { anyhow::bail!("Cratarr's own library isn't connected") };
        tokio::fs::create_dir_all(&folder).await?;
        let placement = placement::place(&*source, &self.place_folders(), &a.cfg.libraries).await?;
        let moved = Arc::new(Active { cfg: a.cfg.clone(), source: Some(source), placement, error: a.error.clone() });
        let key = format!("{}/{id}", a.cfg.id);
        let mut next: Vec<Arc<Active>> = live.iter().cloned().collect();
        next[at] = moved;
        self.switch_to(next).await?;
        if let Some(serve) = crate::serve::instance() {
            let others: Vec<String> = serve.users.names().into_iter().filter(|n| n != account).collect();
            let db = self.playlists_db.clone();
            tokio::task::spawn_blocking(move || db.switch_library_off(&others, &key)).await??;
        }
        Ok(Some(id))
    }

    /// The name of the library the Music folder belongs to, as its server calls it: what Keep and Replace say they
    /// put albums in. None while no server says (or Cratarr does not see its files).
    pub(super) fn music_name(&self) -> Option<String> {
        let music = self.music()?;
        self.connected().iter().find_map(|(a, _)| {
            a.placement.libraries.iter().find(|l| a.kind_of(&l.id) == Some(LibraryKind::Main) && l.folders.contains(&music)).map(|l| l.name.clone())
        })
    }

    /// Where the playlists are kept (the user data): a playlist by its id.
    pub(super) fn playlists_db(&self) -> &Arc<crate::userdata::UserData> {
        &self.playlists_db
    }

    /// The Music folder's playlists, if one is set.
    pub(super) fn store_opt(&self) -> Option<Arc<Store>> {
        self.folders.read().unwrap_or_else(|p| p.into_inner()).1.clone()
    }

    pub(super) fn store(&self) -> Result<Arc<Store>, ToggleError> {
        self.store_opt().ok_or(ToggleError::NoMusicFolder)
    }

    /// One account's playlists (decision 0051): the first administrator's (`None`) are the ones kept before accounts;
    /// another's are its own, their copies in a folder of their own.
    pub(super) fn store_for(&self, account: Option<&str>) -> Result<Arc<Store>, ToggleError> {
        let top = self.store()?;
        let Some(account) = account else { return Ok(top) };
        if !crate::playlists::account_folder_ok(account) {
            return Err(ToggleError::Refused(format!("{account} cannot name a folder for its playlists.")));
        }
        if let Some(s) = self.account_stores.read().unwrap_or_else(|p| p.into_inner()).get(account).filter(|s| s.base() == top.base()) {
            return Ok(s.clone());
        }
        let s = Arc::new(Store::open(self.playlists_db.clone(), self.catalogue.clone(), Some(account), top.base(), playlist_copies(self.data_dir.as_deref())));
        self.account_stores.write().unwrap_or_else(|p| p.into_inner()).insert(account.to_string(), s.clone());
        Ok(s)
    }

    /// Every account's playlists, for Cratarr's own work over all of them (albums that move, dead entries): the first
    /// administrator's, and each other account's that has any.
    pub(super) fn all_stores(&self) -> Vec<Arc<Store>> {
        let Some(top) = self.store_opt() else { return Vec::new() };
        let mut out = vec![top.clone()];
        let first = crate::serve::instance().and_then(|s| s.users.owner());
        let accounts = self.playlists_db.playlist_owners().inspect_err(|e| tracing::warn!("the playlists' owners not read: {e:#}")).unwrap_or_default();
        out.extend(
            accounts
                .iter()
                .filter(|a| !a.is_empty() && Some(*a) != first.as_ref())
                .filter_map(|a| self.store_for(Some(a)).ok()),
        );
        out
    }

    /// Where Cratarr keeps its own files (none: in memory only).
    pub(super) fn data_dir(&self) -> Option<&str> {
        self.data_dir.as_deref()
    }

    /// The folders a server's libraries are looked for in now.
    pub(super) fn place_folders(&self) -> Vec<String> {
        place_folders(self.data_dir.as_deref(), self.music().as_deref())
    }

    /// Put new sources in use, and follow where their libraries' files are: a new Music folder gets its own playlist
    /// store, and the catalogue follows the folders.
    pub(super) async fn switch_to(&self, next: Vec<Arc<Active>>) -> Result<()> {
        let old = self.folders();
        let folders = derive_folders(&next, old.music.as_deref());
        *self.live.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(next);
        if folders != old {
            tracing::info!("music folder {:?}, incoming folders {:?}", folders.music, folders.incoming);
            let (c, dirs) = (self.catalogue.clone(), catalogue_folders(&folders));
            let lines_were = lines_folder(&self.catalogue, &old);
            tokio::task::spawn_blocking(move || c.set_folders(dirs)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
            let (c, db) = (self.catalogue.clone(), self.playlists_db.clone());
            tokio::task::spawn_blocking(move || follow_paths(&c, &db)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
            let lines_from = lines_folder(&self.catalogue, &folders);
            let store = if lines_from != lines_were {
                Some(music_store(&self.playlists_db, &self.catalogue, self.data_dir.as_deref(), lines_from.as_deref()).await)
            } else {
                None
            };
            let mut slot = self.folders.write().unwrap_or_else(|p| p.into_inner());
            if let Some(store) = store {
                slot.1 = store;
            }
            slot.0 = folders;
        }
        Ok(())
    }

    /// Servers whose libraries could not be listed, or whose Music library's files were not found, get looked at
    /// again (called now and then): they may have been down, or still scanning. An Incoming library Cratarr cannot see
    /// is not retried: only a new mount (a restart) changes that.
    pub async fn retry_placement(&self) {
        let _one_at_a_time = self.changing.lock().await;
        let live = self.live();
        let unplaced = |a: &Active| {
            a.source.is_some()
                && (a.placement.libraries.is_empty()
                    || a.main().iter().any(|id| {
                        a.placement.libraries.iter().find(|l| &l.id == id).is_none_or(|l| l.folders.is_empty())
                    }))
        };
        if !live.iter().any(|a| unplaced(a)) {
            return;
        }
        let places = self.place_folders();
        let mut next = Vec::with_capacity(live.len());
        for a in live.iter() {
            match &a.source {
                Some(s) if unplaced(a) => {
                    let placement = placement::place(&**s, &places, &a.cfg.libraries).await.unwrap_or_default();
                    if placement.roots.len() > a.placement.roots.len() {
                        tracing::info!("source {}: found its music in more folders", a.cfg.id);
                    }
                    next.push(Arc::new(Active { cfg: a.cfg.clone(), source: Some(s.clone()), placement, error: a.error.clone() }));
                }
                _ => next.push(a.clone()),
            }
        }
        if let Err(e) = self.switch_to(next).await {
            tracing::warn!("cannot follow the music folders: {e:#}");
        }
    }
}

#[cfg(test)]
mod switched_off_tests {
    use super::*;

    /// A data folder of a test's own, and its user data.
    fn data_folder(name: &str) -> (PathBuf, Arc<crate::userdata::UserData>) {
        let d = std::env::temp_dir().join(format!("cratarr-library-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        let db = Arc::new(crate::userdata::UserData::open(Some(&d.join("userdata.sqlite"))).unwrap());
        (d, db)
    }

    async fn started(d: &Path, db: &Arc<crate::userdata::UserData>) -> Result<Arc<Library>> {
        Library::new(Some(d.to_string_lossy().to_string()), None, Vec::new(), Default::default(), None, db.clone()).await
    }

    fn lost(d: &Path) {
        for file in ["catalogue.sqlite", "catalogue.sqlite-wal", "catalogue.sqlite-shm"] {
            std::fs::remove_file(d.join(file)).ok();
        }
    }

    /// A catalogue lost after the first start and started afresh is the user data's, though it gave no record then: taken for one
    /// from before the first start, it would stop every start from then on.
    #[tokio::test]
    async fn a_lost_catalogue_started_afresh_after_the_first_start_still_starts() {
        let (d, db) = data_folder("afresh");
        drop(started(&d, &db).await.unwrap());
        assert!(db.first_start_done().unwrap());
        lost(&d);
        let lib = started(&d, &db).await.unwrap();
        assert!(lib.catalogue().ids_lost());
        lib.fresh_track_ids().unwrap();
        drop(lib);
        assert!(!started(&d, &db).await.expect("it starts").catalogue().ids_lost());
        std::fs::remove_dir_all(&d).ok();
    }

    /// Before the first start the records are still to come, and a mark that they were given would leave every row unmatched.
    #[tokio::test]
    async fn a_lost_catalogue_started_afresh_before_the_first_start_still_gives_its_records() {
        let (d, db) = data_folder("afresh-before");
        db.set_starred("ann", crate::userdata::Kind::Song, &["tr-5".to_string()], true).unwrap();
        let lib = started(&d, &db).await.unwrap();
        assert!(lib.catalogue().ids_lost() && !db.first_start_done().unwrap());
        lib.fresh_track_ids().unwrap();
        assert_eq!(lib.catalogue().first_records_done().unwrap(), None);
        std::fs::remove_dir_all(&d).ok();
    }

    /// A catalogue restored from before its last ids were given has not given them, and something names them: its next ids start
    /// above, so an old id names something missing, never something else.
    #[tokio::test]
    async fn a_catalogue_older_than_the_user_data_gives_no_id_twice() {
        let (d, db) = data_folder("older");
        drop(started(&d, &db).await.unwrap());
        db.set_meta(TOP_KEY, "500").unwrap();
        assert_eq!(started(&d, &db).await.unwrap().catalogue().top_id(), 500);
        std::fs::remove_dir_all(&d).ok();
    }

    /// 0.15 remembered no catalogue, so one lost there reads as a first start: every id in the user data would be written off as
    /// unmatched, and the once-guard would stand in the way of the restore. It waits, as a catalogue lost later does.
    #[tokio::test]
    async fn a_catalogue_lost_before_the_first_start_is_no_first_start() {
        let db = Arc::new(crate::userdata::UserData::open(None).unwrap());
        db.set_starred("ann", crate::userdata::Kind::Song, &["tr-5".to_string()], true).unwrap();
        let lib = Library::new(None, None, Vec::new(), Default::default(), None, db.clone()).await.unwrap();
        assert!(lib.catalogue().ids_lost());
        assert!(!db.first_start_done().unwrap());
        assert_eq!(db.meta(CATALOGUE_KEY).unwrap(), None, "the restored catalogue is the one remembered");

        // Nothing named: a new data folder, with its report.
        let db = Arc::new(crate::userdata::UserData::open(None).unwrap());
        let lib = Library::new(None, None, Vec::new(), Default::default(), None, db.clone()).await.unwrap();
        assert!(!lib.catalogue().ids_lost());
        assert!(db.first_start_done().unwrap());
    }

    /// CR3: crates, queues and waiting jobs name albums too, in a store no table scan sees.
    #[test]
    fn the_stores_in_the_data_folder_are_read_for_ids_in_use() {
        let d = std::env::temp_dir().join(format!("cratarr-top-stores-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(d.join("users/ann")).unwrap();
        assert_eq!(top_in_stores(&d), 0, "nothing there yet");
        std::fs::write(d.join("crates.json"), r#"{"crates":[{"conditions":[{"value":{"of":"al-12"}}]}]}"#).unwrap();
        assert_eq!(top_in_stores(&d), 12);
        std::fs::write(d.join("users/ann/crates.json"), r#"{"favourites":{"like:al-40":true}}"#).unwrap();
        assert_eq!(top_in_stores(&d), 40, "someone else's crates");
        std::fs::write(d.join("queues.json"), r#"[{"tracks":[{"album_id":"al-90","id":"tr-5000"}]}]"#).unwrap();
        assert_eq!(top_in_stores(&d), 90, "an album's, and no track's: those are `track_references`'");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn own_music_is_each_subfolder_with_its_account_on_cratarrs_own_source() {
        let placed = |id: &str| crate::placement::PlacedLibrary { id: id.into(), name: id.into(), folders: vec![id.into()], paths: Vec::new(), read_only: false };
        let values = [(crate::settings::OWN_MUSIC.to_string(), serde_json::json!("/own"))].into_iter().collect();
        let libraries = Libraries(vec![LibrarySetting::new("/music", LibraryKind::Main), LibrarySetting::new("/in", LibraryKind::Incoming)]);
        let cfg = SourceConfig { id: "cratarr".into(), kind: crate::source::cratarr::KIND.id.into(), enabled: true, libraries, values };
        let placement = Placement { roots: Vec::new(), libraries: vec![placed("/music"), placed("/in"), placed("/own/ann"), placed("/own/ann/deep"), placed("/elsewhere")] };
        let a = Active { cfg, source: None, placement, error: None };
        let own: Vec<(String, Option<String>)> = a.libraries().into_iter().filter(|u| u.kind == LibraryKind::Own).map(|u| (u.id, u.owner)).collect();
        assert_eq!(own, [("/own/ann".to_string(), Some("ann".to_string()))]);
        assert_eq!(a.kept(), ["/music", "/own/ann"]);
        assert_eq!(a.quality_of("/own/ann"), Quality::Cd, "as a main library, without a setting of its own");
        assert_eq!(a.colour_of("/own/ann").as_deref(), Some(crate::settings::PALETTE[2]), "after Incoming's and the main libraries'");
    }

    /// A second main library's track was on no list of folders here, so it could not be added to a playlist, one by one or among
    /// others, and nothing said so. The lines start from the lowest folder in use, whichever library the list puts first.
    #[test]
    fn a_track_has_a_line_in_every_library() {
        let root = |folder: &str| placement::FolderRoot { folder: folder.into(), root: format!("{folder}/") };
        let placement = Placement { roots: vec![root("/annex"), root("/music"), root("/second"), root("/in")], libraries: Vec::new() };
        let cfg = SourceConfig { id: "cratarr".into(), kind: crate::source::cratarr::KIND.id.into(), enabled: true, libraries: Default::default(), values: Default::default() };
        let a = Active { cfg, source: None, placement, error: None };
        let folders: crate::catalogue::InUse = [(0, "/music".into()), (1, "/second".into()), (2, "/in".into()), (3, "/annex".into())].into_iter().collect();
        let line = |path: &str| {
            let track = source::Track { id: "t".into(), title: "T".into(), artist: None, album: None, album_id: None, path: Some(path.into()), library_id: None };
            a.line(&folders, &track)
        };
        assert_eq!(line("/music/A/1.flac").as_deref(), Some("A/1.flac"));
        assert_eq!(line("/second/A/1.flac").as_deref(), Some("../second/A/1.flac"), "a second main library's");
        assert_eq!(line("/in/A/1.flac").as_deref(), Some("../in/A/1.flac"));
        assert_eq!(line("/annex/A/1.flac").as_deref(), Some("../annex/A/1.flac"), "one the list puts first, its id the highest");
        assert_eq!(line("/elsewhere/A/1.flac"), None);
    }

    /// 0101 point 1: a main library's folder is one of the catalogue's first ones, the main libraries' (Keep's and
    /// Replace's targets, out of Lidarr's reach); while no server has answered, the last main folder stands in.
    #[tokio::test]
    async fn the_main_libraries_folders_are_the_catalogues_first() {
        let d = std::env::temp_dir().join(format!("cratarr-mains-{}", std::process::id()));
        std::fs::create_dir_all(d.join(crate::playlists::PLAYLIST_DIR)).unwrap();
        let db = Arc::new(crate::userdata::UserData::open(None).unwrap());
        let lib = Library::new(None, Some(d.to_string_lossy().into_owned()), Vec::new(), Default::default(), None, db).await.unwrap();
        assert_eq!(lib.main_folders(), [0]);
        assert!(lib.is_main(0));
        assert!(!lib.is_main(1) && !lib.is_main(-1), "no folder there");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 0101 point 1: several main libraries, the first taking every fallback, each folder once, a main library's never
    /// again as Incoming's; the catalogue's folders the main ones' first, in order.
    #[test]
    fn several_main_libraries_come_first_each_folder_once() {
        let v = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let f = folders_of(v(&["/m1", "/m2", "/m1"]), v(&["/o1", "/m1", "/o1"]), v(&["/i2", "/m2", "/i1", "/o1"]), None);
        assert_eq!(f.music.as_deref(), Some("/m1"));
        assert_eq!(f.more_main, v(&["/m2"]));
        assert_eq!(f.own, v(&["/o1"]), "each folder once, a main library's never own music's");
        assert_eq!(f.incoming, v(&["/i1", "/i2"]));
        assert_eq!(f.all(), v(&["/m1", "/m2", "/o1", "/i1", "/i2"]));
        let f = folders_of(Vec::new(), Vec::new(), v(&["/i1"]), Some("/old"));
        assert_eq!(f.music.as_deref(), Some("/old"), "no main library has answered: the last one stands in");
    }

    /// 0101: the user data remembers which catalogue its ids are of; another one is lost ids.
    #[test]
    fn which_catalogue_the_ids_are_of() {
        assert_eq!(ids_state(None, "g1"), Ids::Remember);
        assert_eq!(ids_state(Some("g1"), "g1"), Ids::Same);
        assert_eq!(ids_state(Some("g1"), "g2"), Ids::Lost);
    }

    #[test]
    fn a_library_switched_off_leaves_out_the_folders_only_it_keeps() {
        let placed = |id: &str, folders: &[&str]| placement::PlacedLibrary {
            id: id.into(),
            name: id.into(),
            folders: folders.iter().map(|f| f.to_string()).collect(),
            paths: Vec::new(),
            read_only: false,
        };
        let mut cfg = SourceConfig { id: "jf".into(), kind: "jellyfin".into(), enabled: true, libraries: Default::default(), values: Default::default() };
        cfg.libraries = Libraries(vec![
            LibrarySetting::new("m", LibraryKind::Main),
            LibrarySetting::new("in", LibraryKind::Incoming),
            LibrarySetting::new("both", LibraryKind::Incoming),
        ]);
        let placement = Placement {
            roots: Vec::new(),
            libraries: vec![placed("m", &["/music"]), placed("in", &["/incoming"]), placed("both", &["/incoming", "/new"]), placed("unused", &["/films"])],
        };
        let a = Active { cfg, source: None, placement, error: None };
        assert_eq!(a.kept(), ["m"]);
        let off = |keys: &[&str]| folders_switched_off(&a, &keys.iter().map(|k| k.to_string()).collect::<Vec<_>>());
        assert!(off(&[]).is_empty(), "nothing switched off: every library");
        assert_eq!(off(&["jf/m"]), ["/music"]);
        assert_eq!(off(&["jf/in"]), Vec::<String>::new(), "another library on keeps /incoming");
        assert_eq!(off(&["jf/in", "jf/both"]), ["/incoming", "/new"]);
        assert!(off(&["other/m", "jf/unused", "jfx/m"]).is_empty(), "another server's, an unused library's: nothing");
    }
}
