//! The library (decision 0011): the servers in use and where their music lies, the folders behind the libraries in
//! use, the Music folder's playlist store and the catalogue. The other services read it; changes to the servers go
//! through `switch_to`, one at a time.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use crate::naming::Template;
use serde::Serialize;
use tokio::sync::Mutex;

use super::ToggleError;
use crate::catalogue::Catalogue;
use crate::placement::{self, Placement};
use crate::playlists::Store;
use crate::settings::{Folders, SourceConfig};
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
    let music = up.iter().flat_map(|a| found(a, &a.cfg.libraries.music)).next().or_else(|| last_music.map(str::to_string));
    let mut incoming: Vec<String> =
        up.iter().flat_map(|a| found(a, &a.cfg.libraries.incoming)).filter(|f| Some(f) != music.as_ref()).collect();
    incoming.sort();
    incoming.dedup();
    Folders { music, incoming }
}

/// The folders a server's libraries switched off leave out (decision 0024, amended 2026-10-23: a crate draws from
/// every library switched on). `without`: `source/library` keys, as the page's library switches keep them; only this
/// server's count. A folder is left out when every library of the server in use that keeps files there (as
/// `derive_folders` finds them) is switched off; a folder none of them keeps stays.
pub(super) fn folders_switched_off(a: &Active, without: &[String]) -> Vec<String> {
    let off = |library: &str| without.iter().any(|k| k.strip_prefix(a.cfg.id.as_str()).and_then(|r| r.strip_prefix('/')) == Some(library));
    let used = a.cfg.libraries.music.iter().chain(&a.cfg.libraries.incoming);
    let (mut on, mut out) = (std::collections::BTreeSet::new(), std::collections::BTreeSet::new());
    for l in used.filter_map(|id| a.placement.libraries.iter().find(|l| &l.id == id)) {
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

impl Active {
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
        let placement = placement::place(&*source, folders).await.unwrap_or_else(|e| {
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

    /// A track's playlist line, for any track Cratarr can see: its path in the Music folder, or for one in an Incoming
    /// folder its path from there ("../lidarr-music/X/01.flac"). Keep and Replace move such a line along with the file,
    /// and Delete takes it out (decision 0011 §6).
    pub(super) fn line(&self, folders: &crate::settings::Folders, track: &source::Track) -> Option<String> {
        let music = folders.music.as_deref()?;
        if let Some(e) = self.placement.entry(music, track.path.as_deref()) {
            return Some(e);
        }
        folders.incoming.iter().find_map(|f| {
            let rel = self.placement.entry(f, track.path.as_deref())?;
            crate::playlists::relative_to(std::path::Path::new(music), &std::path::Path::new(f).join(rel))
        })
    }
}

pub struct Library {
    live: std::sync::RwLock<Arc<Vec<Arc<Active>>>>,
    data_dir: Option<String>,
    /// Where the libraries in use keep their files, and the playlists of the Music folder (swapped in whole when
    /// either changes).
    folders: std::sync::RwLock<(Folders, Option<Arc<Store>>)>,
    /// The other accounts' playlists (decision 0051), by account, as opened (each in the Music folder of its day).
    account_stores: std::sync::RwLock<std::collections::HashMap<String, Arc<Store>>>,
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
/// A "stopped processing" notice that nothing confirmed again is dropped after this long.
const STUCK_SECS: i64 = 30 * 60;

impl Library {
    /// `last_music`: where the Music libraries were last found (or CRATARR_PLAYLIST_DIR), used until a server answers.
    pub(super) async fn new(
        data_dir: Option<String>,
        last_music: Option<String>,
        sources: Vec<SourceConfig>,
        tag_rules: crate::catalogue::Rules,
        naming: Option<String>,
    ) -> Result<Arc<Self>> {
        let naming = naming.and_then(|t| {
            t.parse::<Template>().inspect_err(|e| tracing::warn!("the saved naming template is not usable ({e}); using the library's")).ok()
        });
        let places = place_folders(data_dir.as_deref(), last_music.as_deref());
        let mut active = Vec::new();
        for s in sources {
            active.push(Active::start(s, &places).await);
        }
        // First run: no Music library chosen yet, so the libraries found in CRATARR_PLAYLIST_DIR are Music (shown so on
        // the Settings page, kept once saved).
        if let Some(m) = last_music.as_deref().filter(|_| active.iter().all(|a| a.cfg.libraries.music.is_empty())) {
            for a in active.iter_mut().filter(|a| a.source.is_some()) {
                let incoming = a.cfg.libraries.incoming.clone();
                a.cfg.libraries.music =
                    a.placement.libraries_in(m).map(|l| l.id.clone()).filter(|id| !incoming.contains(id)).collect();
            }
        }
        let active: Vec<Arc<Active>> = active.into_iter().map(Arc::new).collect();
        let folders = derive_folders(&active, last_music.as_deref());
        let db = data_dir.as_ref().map(|d| PathBuf::from(d).join("catalogue.sqlite"));
        let catalogue = Arc::new(Catalogue::open(db.as_deref(), catalogue_folders(&folders))?);
        let store = folders.music.as_ref().map(|m| Arc::new(Store::in_music(m)));
        Ok(Arc::new(Self {
            live: std::sync::RwLock::new(Arc::new(active)),
            data_dir,
            folders: std::sync::RwLock::new((folders, store)),
            account_stores: std::sync::RwLock::default(),
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

    /// Some enabled server has an Incoming library: the header's `incoming` and the capability list's.
    pub(super) fn incoming_on(&self) -> bool {
        self.live().iter().any(|a| a.cfg.enabled && !a.cfg.libraries.incoming.is_empty())
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
    pub(super) fn folders_off(&self, without: &[String]) -> Vec<i64> {
        if without.is_empty() {
            return Vec::new();
        }
        let off: Vec<std::path::PathBuf> = self.connected().into_iter().flat_map(|(a, _)| folders_switched_off(&a, without)).map(std::path::PathBuf::from).collect();
        self.catalogue().folders().iter().enumerate().filter(|(_, f)| off.contains(f)).map(|(i, _)| i as i64).collect()
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

    /// The name of the library the Music folder belongs to, as its server calls it: what Keep and Replace say they
    /// put albums in. None while no server says (or Cratarr does not see its files).
    pub(super) fn music_name(&self) -> Option<String> {
        let music = self.music()?;
        self.connected().iter().find_map(|(a, _)| {
            a.placement.libraries.iter().find(|l| a.cfg.libraries.music.contains(&l.id) && l.folders.contains(&music)).map(|l| l.name.clone())
        })
    }

    /// The Music folder's playlists, if one is set.
    pub(super) fn store_opt(&self) -> Option<Arc<Store>> {
        self.folders.read().unwrap_or_else(|p| p.into_inner()).1.clone()
    }

    pub(super) fn store(&self) -> Result<Arc<Store>, ToggleError> {
        self.store_opt().ok_or(ToggleError::NoMusicFolder)
    }

    /// One account's playlists (decision 0051): the first administrator's (`None`) are the Music folder's, as they
    /// were before accounts; another's are in a folder of its own in the playlist folder.
    pub(super) fn store_for(&self, account: Option<&str>) -> Result<Arc<Store>, ToggleError> {
        let top = self.store()?;
        let Some(account) = account else { return Ok(top) };
        if !top.in_playlist_dir() {
            return Err(ToggleError::Refused("Other users' playlists need the playlist folder (.playlists) in Music.".into()));
        }
        if !crate::playlists::account_folder_ok(account) {
            return Err(ToggleError::Refused(format!("{account} cannot name a folder for its playlists.")));
        }
        if let Some(s) = self.account_stores.read().unwrap_or_else(|p| p.into_inner()).get(account).filter(|s| s.base() == top.base()) {
            return Ok(s.clone());
        }
        let s = Arc::new(Store::of_account(top.base(), account));
        self.account_stores.write().unwrap_or_else(|p| p.into_inner()).insert(account.to_string(), s.clone());
        Ok(s)
    }

    /// Every account's playlists, for Cratarr's own work over all of them (albums that move, dead entries): the first
    /// administrator's, and each other account's that has its folder.
    pub(super) fn all_stores(&self) -> Vec<Arc<Store>> {
        let Some(top) = self.store_opt() else { return Vec::new() };
        let mut out = vec![top.clone()];
        if !top.in_playlist_dir() {
            return out;
        }
        let first = crate::serve::instance().and_then(|s| s.users.owner());
        let Ok(dirs) = std::fs::read_dir(top.dir()) else { return out };
        let mut accounts: Vec<String> = dirs
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|a| crate::playlists::account_folder_ok(a) && Some(a) != first.as_ref())
            .collect();
        accounts.sort();
        out.extend(accounts.iter().filter_map(|a| self.store_for(Some(a)).ok()));
        out
    }

    /// Open the Music folder's playlists again (after they moved into `.playlists`).
    pub(super) fn reopen_store(&self) {
        let mut slot = self.folders.write().unwrap_or_else(|p| p.into_inner());
        slot.1 = slot.0.music.as_ref().map(|m| Arc::new(Store::in_music(m)));
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
            tokio::task::spawn_blocking(move || c.set_folders(dirs)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
            let mut slot = self.folders.write().unwrap_or_else(|p| p.into_inner());
            if folders.music != old.music {
                slot.1 = folders.music.as_ref().map(|m| Arc::new(Store::in_music(m)));
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
                    || a.cfg.libraries.music.iter().any(|id| {
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
                    let placement = placement::place(&**s, &places).await.unwrap_or_default();
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
        cfg.libraries.music = vec!["m".into()];
        cfg.libraries.incoming = vec!["in".into(), "both".into()];
        let placement = Placement {
            roots: Vec::new(),
            libraries: vec![placed("m", &["/music"]), placed("in", &["/incoming"]), placed("both", &["/incoming", "/new"]), placed("unused", &["/films"])],
        };
        let a = Active { cfg, source: None, placement, error: None };
        let off = |keys: &[&str]| folders_switched_off(&a, &keys.iter().map(|k| k.to_string()).collect::<Vec<_>>());
        assert!(off(&[]).is_empty(), "nothing switched off: every library");
        assert_eq!(off(&["jf/m"]), ["/music"]);
        assert_eq!(off(&["jf/in"]), Vec::<String>::new(), "another library on keeps /incoming");
        assert_eq!(off(&["jf/in", "jf/both"]), ["/incoming", "/new"]);
        assert!(off(&["other/m", "jf/unused", "jfx/m"]).is_empty(), "another server's, an unused library's: nothing");
    }
}
