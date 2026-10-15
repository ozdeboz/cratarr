//! Playlists (decision 0011): the Music folder's playlist files, edits and repairs, Play, and telling the servers
//! about changes.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;

use super::ToggleError;
use super::library::{Library, label_of};
use std::collections::{HashMap, HashSet};

use crate::crates::{self, CrateDef};
use crate::journal::{Change, EventId, Journal, TextChange, fingerprint};
use crate::playlists::{DupGroup, Edited, PlaylistView, Repair, Repairs};
use crate::source::{self, PlaylistChange, PlaylistEvent};
use crate::sync_state::Kind;

/// The most tracks one play sends: Finamp holds at most 100 in its queue (seen 2026-10-12; user: "we go by 100").
pub const PLAY_MAX: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayMode {
    Play,
    Shuffle,
    Next,
    Last,
}

/// A playlist found by a spoken name, as its tracks on a server (decision 0015): the caller plays them itself.
#[derive(Debug, Serialize)]
pub struct ByName {
    /// The playlist it was taken for; `None` when nothing fits what was said.
    pub playlist: Option<String>,
    /// The server's ids of its tracks, in order or shuffled, at most the limit asked for.
    pub tracks: Vec<String>,
}

/// What crates are worked out on (see `PlaylistService::crate_library`).
/// A page of a crate's rows: the crate's size, and (on the first page) its genres, most shared first.
#[derive(Serialize)]
pub struct CratePage {
    pub source: String,
    pub total: usize,
    pub rows: Vec<PlaylistRow>,
    pub genres: Vec<String>,
}

struct CrateLibrary {
    source: String,
    files: Arc<Vec<crates::Facts>>,
    membership: HashMap<String, Vec<String>>,
    favourites: Option<HashSet<String>>,
}

impl CrateLibrary {
    fn select(&self, conditions: &[crates::Condition]) -> Vec<String> {
        let w = crates::World { membership: &self.membership, favourites: self.favourites.as_ref() };
        crates::select(&self.files, conditions, &w).into_iter().map(str::to_string).collect()
    }
}

/// A crate as the Crates page lists it (decision 0024); `count` is `None` when not known (the server's favourites could
/// not be read).
#[derive(Debug, Serialize)]
pub struct CrateView {
    pub id: String,
    pub name: String,
    /// Its favourites toggle's setting (decision 0024, amended).
    pub favourites: crates::Favourites,
    /// How many tracks it has with each setting of the toggle (the page shows the one it is set to, and the next at
    /// once on a tap); `None` where the favourites are not known (no server connected).
    pub counts: CrateCounts,
    pub permanent: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CrateCounts {
    pub all: Option<usize>,
    pub only: Option<usize>,
    pub none: Option<usize>,
}

/// One line of a playlist as its view shows it (decision 0023). `entry` is the line as the `.m3u` has it (library-
/// relative), `file` its file name; the rest is what the catalogue knows, `None` where it does not.
#[derive(Debug, Serialize, PartialEq)]
pub struct PlaylistRow {
    pub entry: String,
    pub file: String,
    /// The server's track id, when the file is matched to one.
    pub id: Option<String>,
    pub name: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_secs: Option<f64>,
    /// No such file in the Music folder.
    pub missing: bool,
}

#[derive(Debug, Serialize)]
pub struct PlayResult {
    /// "Finamp on Lenovo Tab".
    pub player: String,
    /// Tracks sent (at most PLAY_MAX).
    pub sent: usize,
    /// Playable tracks in the playlist.
    pub total: usize,
    /// Lines whose track is missing or not known to the server yet; left out.
    pub unknown: usize,
}

/// Shuffle in place (Fisher–Yates) with a small generator seeded from the clock; good enough for a playlist.
fn shuffle<T>(items: &mut [T]) {
    let mut x = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
    for i in (1..items.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        items.swap(i, (x % (i as u64 + 1)) as usize);
    }
}

/// Whether `entries` only reorders or leaves out lines of `expected` (each line at most as often as it was there).
fn is_rearrangement(expected: &[String], entries: &[String]) -> bool {
    let mut left: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for e in expected {
        *left.entry(e.as_str()).or_default() += 1;
    }
    entries.iter().all(|e| match left.get_mut(e.as_str()) {
        Some(n) if *n > 0 => {
            *n -= 1;
            true
        }
        _ => false,
    })
}

/// The safe repairs of the playlists `within` (all of them without), matched as the store names playlists: ignoring case.
fn safe_within(safe: Vec<Repair>, within: Option<&[String]>) -> Vec<Repair> {
    match within {
        None => safe,
        Some(w) => safe.into_iter().filter(|r| w.iter().any(|p| p.eq_ignore_ascii_case(&r.playlist))).collect(),
    }
}

#[cfg(test)]
mod undo_tests {
    use super::*;

    /// The service over an empty Music folder of its own, no servers, the journal and the crates in memory.
    async fn service() -> (PathBuf, PlaylistService) {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("cratarr-undo-{}-{n}", std::process::id()));
        // Its playlist folder from the start (decision 0014), so the store never moves meanwhile.
        std::fs::create_dir_all(d.join(crate::playlists::PLAYLIST_DIR)).unwrap();
        let lib = Library::new(None, Some(d.to_string_lossy().into_owned()), Vec::new(), Default::default(), None).await.unwrap();
        let journal = Arc::new(Journal::open(None).unwrap());
        let state = crate::sync_state::SyncState::open(None).unwrap();
        let sync = Arc::new(super::super::sync::SyncService::new(lib.clone(), state, journal.clone()));
        (d.join(crate::playlists::PLAYLIST_DIR), PlaylistService::new(lib, sync, journal))
    }

    fn refused(r: Result<Option<String>, ToggleError>) -> String {
        match r {
            Err(ToggleError::Refused(m)) => m,
            other => panic!("not refused: {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_undo_is_undone_by_its_redo_and_happens_only_once() {
        let (d, s) = service().await;
        let lines = |v: &[&str]| v.iter().map(|l| l.to_string()).collect::<Vec<_>>();
        let read = |n: &str| std::fs::read_to_string(d.join(format!("{n}.m3u"))).ok();
        let made = s.create_with("Chill", &lines(&["A/1.mp3", "A/2.mp3"])).await.unwrap().unwrap();
        let edited = s.edit("Chill", &lines(&["A/1.mp3", "A/2.mp3"]), &lines(&["A/2.mp3"])).await.unwrap().unwrap();
        assert_eq!(read("Chill").as_deref(), Some("../A/2.mp3\n"));

        let redo = s.undo(&edited).await.unwrap().unwrap();
        assert_eq!(read("Chill").as_deref(), Some("../A/1.mp3\n../A/2.mp3\n"), "the edit undone");
        assert_eq!(refused(s.undo(&edited).await), "That was undone already.");
        let again = s.undo(&redo).await.unwrap().unwrap();
        assert_eq!(read("Chill").as_deref(), Some("../A/2.mp3\n"), "redone, as a change of its own");
        s.undo(&again).await.unwrap();
        assert_eq!(read("Chill").as_deref(), Some("../A/1.mp3\n../A/2.mp3\n"), "and the redo undone");

        let unmade = s.undo(&made).await.unwrap().unwrap();
        assert_eq!(read("Chill"), None, "the new playlist set aside");
        s.undo(&unmade).await.unwrap();
        assert_eq!(read("Chill").as_deref(), Some("../A/1.mp3\n../A/2.mp3\n"), "and back");
        assert_eq!(refused(s.undo("c999").await), "That can no longer be undone.");
        assert_eq!(refused(s.undo("nonsense").await), "That can no longer be undone.");
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn nothing_is_undone_when_it_changed_since() {
        let (d, s) = service().await;
        let one = vec!["A/1.mp3".to_string()];
        let made = s.create_with("Chill", &one).await.unwrap().unwrap();
        let cleared = s.edit("Chill", &one, &[]).await.unwrap().unwrap();
        // Another device (or a sync) changes it meanwhile.
        std::fs::write(d.join("Chill.m3u"), "../B/9.mp3\n").unwrap();
        assert_eq!(refused(s.undo(&cleared).await), "It changed since; nothing was undone.");
        assert_eq!(refused(s.undo(&made).await), "It changed since; nothing was undone.");
        assert_eq!(std::fs::read_to_string(d.join("Chill.m3u")).unwrap(), "../B/9.mp3\n", "left alone");
        std::fs::write(d.join("Chill.m3u"), "").unwrap();
        s.undo(&cleared).await.unwrap();
        assert_eq!(std::fs::read_to_string(d.join("Chill.m3u")).unwrap(), "../A/1.mp3\n", "a refused undo can be tried again");

        let renamed = s.rename("Chill", "Calm").await.unwrap().unwrap();
        s.create("Chill").await.unwrap();
        assert_eq!(refused(s.undo(&renamed).await), "It changed since; nothing was undone.", "the old name is taken");
        std::fs::remove_file(d.join("Chill.m3u")).unwrap();
        s.undo(&renamed).await.unwrap();
        assert!(d.join("Chill.m3u").exists() && !d.join("Calm.m3u").exists());

        let deleted = s.delete("Chill").await.unwrap().unwrap();
        s.create("Chill").await.unwrap();
        assert_eq!(refused(s.undo(&deleted).await), "It changed since; nothing was undone.", "a new one has the name");
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn crates_and_spoken_names_are_put_back() {
        let (d, s) = service().await;
        let cond = |v: &str| crates::Condition { axis: "genre".into(), not: false, value: crates::Value::AnyOf { values: vec![v.into()] } };
        let (id, made) = s.crate_save(None, "Jazz", vec![cond("Jazz")]).await.unwrap();
        let (_, changed) = s.crate_save(Some(&id), "Jazzy", vec![cond("Bebop")]).await.unwrap();
        s.undo(&changed.unwrap()).await.unwrap();
        assert_eq!(s.crate_def(&id).await.unwrap().name, "Jazz");
        let gone = s.crate_delete(&id).await.unwrap().unwrap();
        assert!(s.crate_def(&id).await.is_err());
        s.undo(&gone).await.unwrap();
        assert_eq!(s.crate_def(&id).await.unwrap().conditions, vec![cond("Jazz")], "back with its own id");
        s.crate_save(Some(&id), "Jazz", vec![cond("Swing")]).await.unwrap();
        assert_eq!(refused(s.undo(&made.unwrap()).await), "It changed since; nothing was undone.");

        s.create("Chill").await.unwrap();
        let said = s.set_spoken("Chill", &["calm".into()]).await.unwrap().unwrap();
        assert_eq!(s.set_spoken("Chill", &["calm".into()]).await.unwrap(), None, "no change, no token");
        s.undo(&said).await.unwrap();
        assert!(s.sync.spoken().remove("Chill").unwrap_or_default().is_empty());
        std::fs::remove_dir_all(&d).ok();
    }
}

#[cfg(test)]
mod edit_tests {
    use super::{Repair, is_rearrangement, safe_within};

    #[test]
    fn fixing_all_in_the_repair_tab_touches_only_the_selected_playlists() {
        let r = |p: &str| Repair { playlist: p.into(), from: format!("{p}.mp3"), to: format!("{p}.flac") };
        let all = vec![r("Chill"), r("Jazz"), r("chill")];
        assert_eq!(safe_within(all.clone(), None).len(), 3, "no selection: every safe repair");
        let only = safe_within(all, Some(&["CHILL".into()]));
        assert_eq!(only.iter().map(|x| x.playlist.as_str()).collect::<Vec<_>>(), ["Chill", "chill"]);
        assert!(safe_within(vec![r("Jazz")], Some(&[])).is_empty(), "an empty selection fixes nothing");
    }

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn an_edit_may_only_reorder_or_leave_out_its_own_lines() {
        let was = v(&["a", "b", "b", "c"]);
        assert!(is_rearrangement(&was, &v(&["c", "b", "a", "b"])), "reordered");
        assert!(is_rearrangement(&was, &v(&["b", "c"])), "some left out");
        assert!(is_rearrangement(&was, &[]), "all left out");
        assert!(!is_rearrangement(&was, &v(&["a", "d"])), "a line it did not have");
        assert!(!is_rearrangement(&was, &v(&["a", "a"])), "a line more often than it was there");
    }
}

pub struct PlaylistService {
    lib: Arc<Library>,
    /// Keeps the user's own server playlists in step (decision 0012).
    sync: Arc<super::sync::SyncService>,
    /// One's own crates (decision 0024).
    crate_store: crates::CrateStore,
    /// Where every change is kept for its undo (decision 0026).
    journal: Arc<Journal>,
    /// The favourite tracks' lines for the crates (see `favourite_lines`).
    favourites: Arc<FavouritesKept>,
}

/// How long the favourites heard are answered before they are asked for again (in the background).
const FAVOURITES_KEEP: std::time::Duration = std::time::Duration::from_secs(60);

/// The media server's favourites as the crates last heard them, and the one asking for them now.
#[derive(Default)]
struct FavouritesKept {
    last: std::sync::Mutex<Option<Kept>>,
    asking: Arc<tokio::sync::Mutex<()>>,
    /// Where what was heard is kept between runs (`favourites.json` in the data folder), so the first Crates after a
    /// start answers at once instead of waiting for the server (a minute or more for thousands of favourites).
    file: Option<std::path::PathBuf>,
}

/// `favourites.json`: the last answer, by server.
#[derive(Serialize, serde::Deserialize)]
struct KeptFile {
    source: String,
    lines: Vec<String>,
}

#[derive(Clone)]
struct Kept {
    at: std::time::Instant,
    source: String,
    /// The last of Cratarr's own changes (`crates::favourite_changed`) the answer holds.
    upto: u64,
    /// `None`: the server did not answer.
    lines: Option<Arc<HashSet<String>>>,
}

impl FavouritesKept {
    /// What the last run heard, if kept: due to be asked again at once (in the background).
    fn load(file: Option<std::path::PathBuf>) -> Self {
        let last = file.as_ref().and_then(|f| std::fs::read(f).ok()).and_then(|b| serde_json::from_slice::<KeptFile>(&b).ok()).map(|k| {
            let now = std::time::Instant::now();
            Kept { at: now.checked_sub(FAVOURITES_KEEP).unwrap_or(now), source: k.source, upto: 0, lines: Some(Arc::new(k.lines.into_iter().collect())) }
        });
        Self { last: std::sync::Mutex::new(last), asking: Arc::default(), file }
    }

    /// Keep an answer for the next run. A failure costs only the wait after the next start.
    async fn save(&self, source: &str, lines: &Arc<HashSet<String>>) {
        let Some(file) = self.file.clone() else { return };
        let body = KeptFile { source: source.to_string(), lines: lines.iter().cloned().collect() };
        let written = tokio::task::spawn_blocking(move || -> Result<()> {
            let tmp = file.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec(&body)?)?;
            std::fs::rename(&tmp, &file)?;
            Ok(())
        });
        if let Ok(Err(e)) = written.await {
            tracing::warn!("keeping the favourites for the next start: {e:#}");
        }
    }

    fn kept(&self, source: &str) -> Option<Kept> {
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone().filter(|k| k.source == source)
    }

    /// Ask the server (the caller holds `asking`). When it does not answer, what was heard before is kept, for another
    /// minute before asking again.
    async fn ask(&self, c: Arc<crate::catalogue::Catalogue>, source: &str, from: &Arc<dyn source::Source>) -> Kept {
        let upto = crates::favourite_changes_made();
        let lines = match from.favorite_tracks().await {
            Ok(ids) => {
                let s = source.to_string();
                match tokio::task::spawn_blocking(move || c.rels_of(&s, 0, &ids)).await {
                    Ok(Ok(lines)) => Some(Arc::new(lines)),
                    Ok(Err(e)) => {
                        tracing::warn!("favourites for the crates: {e:#}");
                        None
                    }
                    Err(e) => {
                        tracing::warn!("favourites for the crates: {e}");
                        None
                    }
                }
            }
            Err(e) => {
                tracing::warn!("favourites for the crates: {e:#}");
                None
            }
        };
        if let Some(l) = &lines {
            self.save(source, l).await;
        }
        let now = std::time::Instant::now();
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let kept = match (lines, last.clone().filter(|k| k.source == source && k.lines.is_some())) {
            (Some(lines), _) => {
                crates::favourite_changes_held(source, upto);
                Kept { at: now, source: source.to_string(), upto, lines: Some(lines) }
            }
            (None, Some(before)) => Kept { at: now, ..before },
            (None, None) => Kept { at: now, source: source.to_string(), upto, lines: None },
        };
        *last = Some(kept.clone());
        kept
    }

    /// The kept lines with Cratarr's own changes made since applied.
    async fn with_changes(kept: Kept, c: Arc<crate::catalogue::Catalogue>) -> Option<HashSet<String>> {
        let lines = kept.lines?;
        let changes = crates::favourite_changes_since(&kept.source, kept.upto);
        if changes.is_empty() {
            return Some((*lines).clone());
        }
        let s = kept.source;
        let lines = tokio::task::spawn_blocking(move || {
            let mut lines = (*lines).clone();
            for (track, on) in changes {
                let of = c.rels_of(&s, 0, std::slice::from_ref(&track))?;
                if on {
                    lines.extend(of);
                } else {
                    lines.retain(|l| !of.contains(l));
                }
            }
            anyhow::Ok(lines)
        });
        lines.await.ok()?.ok()
    }
}

impl PlaylistService {
    pub(super) fn new(lib: Arc<Library>, sync: Arc<super::sync::SyncService>, journal: Arc<Journal>) -> Self {
        let crate_store = crates::CrateStore::new(lib.data_dir());
        let favourites = Arc::new(FavouritesKept::load(lib.data_dir().map(|d| std::path::PathBuf::from(d).join("favourites.json"))));
        Self { lib, sync, crate_store, journal, favourites }
    }

    /// Ask the media server for the favourites once, at start, when a crate shows by them: the first visit to Crates
    /// then need not wait the minute that may take.
    pub async fn warm_favourites(&self) {
        let Ok(set) = self.crate_store.all_favourites().await else { return };
        let needed = crates::permanent().into_iter().chain(self.crate_store.list().await.unwrap_or_default()).any(|c| {
            crates::needs_favourites(&c.conditions) || set.get(&c.id).copied().unwrap_or(crates::Favourites::None) != crates::Favourites::All
        });
        if let (true, Ok((sid, source))) = (needed, self.source_of(None)) {
            self.favourite_lines(&sid, &source).await;
        }
    }

    /// Keep a change in the journal; its undo token (decision 0026). A journal that cannot be written costs only the
    /// undo: the change itself is done.
    fn keep(&self, change: Change) -> Option<String> {
        match self.journal.record_change(super::library::epoch_now(), &change) {
            Ok(id) => Some(format!("c{id}")),
            Err(e) => {
                tracing::warn!("journal: cannot keep a change for its undo: {e:#}");
                None
            }
        }
    }

    /// Keep the playlists' files that changed (`None` when none did).
    fn keep_texts(&self, edits: Vec<Edited>) -> Option<String> {
        let changes: Vec<TextChange> = edits
            .into_iter()
            .filter(|e| e.before != e.after)
            .map(|e| TextChange { left: fingerprint(&e.after), playlist: e.id, before: e.before })
            .collect();
        (!changes.is_empty()).then(|| self.keep(Change::Texts { changes })).flatten()
    }

    /// Undo a change by its token (decision 0026): only while what it left is still there (else "It changed since",
    /// and nothing is undone), and only once. Returns the redo's token: the undo is a change of its own.
    pub async fn undo(&self, token: &str) -> Result<Option<String>, ToggleError> {
        let gone = || ToggleError::Refused("That can no longer be undone.".into());
        let id: EventId = token.strip_prefix('c').and_then(|n| n.parse().ok()).ok_or_else(gone)?;
        let (change, undone) = self.journal.change(id)?.ok_or_else(gone)?;
        if undone || !self.journal.claim_change(id, super::library::epoch_now())? {
            return Err(ToggleError::Refused("That was undone already.".into()));
        }
        match self.reverse(change).await {
            Ok(redo) => Ok(self.keep(redo)),
            Err(e) => {
                if let Err(j) = self.journal.release_change(id) {
                    tracing::warn!("journal: {j:#}");
                }
                Err(e)
            }
        }
    }

    /// Put back what a change replaced, after checking that everything it left is still there. Returns the change
    /// that undoes this undo.
    async fn reverse(&self, change: Change) -> Result<Change, ToggleError> {
        let changed = || ToggleError::Refused("It changed since; nothing was undone.".into());
        let store = self.lib.store()?;
        let left = |text: Option<String>| text.map(|t| fingerprint(&t));
        match change {
            Change::Texts { changes } => {
                for c in &changes {
                    if left(store.text(&c.playlist).await) != Some(c.left) {
                        return Err(changed());
                    }
                }
                let mut redo = Vec::new();
                for c in changes {
                    match store.put_back_if(&c.playlist, c.left, &c.before).await? {
                        Some(e) => redo.push(TextChange { left: fingerprint(&e.after), playlist: e.id, before: e.before }),
                        None => tracing::warn!("undo: {} changed while it was being put back", c.playlist),
                    }
                }
                self.tell_sources(redo.iter().map(|c| (c.playlist.clone(), PlaylistChange::Edited)).collect());
                Ok(Change::Texts { changes: redo })
            }
            Change::Created { playlists } => {
                for (name, l) in &playlists {
                    if left(store.text(name).await) != Some(*l) {
                        return Err(changed());
                    }
                }
                let mut gone = Vec::new();
                for (name, l) in playlists {
                    if store.delete_if(&name, l).await? {
                        gone.push(name);
                    }
                }
                self.tell_sources(gone.iter().map(|n| (n.clone(), PlaylistChange::Deleted)).collect());
                Ok(Change::Deleted { playlists: gone })
            }
            Change::Deleted { playlists } => {
                for name in &playlists {
                    if store.text(name).await.is_some() {
                        return Err(changed());
                    }
                }
                let mut back = Vec::new();
                for name in playlists {
                    let text = store.restore_deleted(&name).await?;
                    self.sync.restore_links(&name).await?;
                    back.push((name, fingerprint(&text)));
                }
                Ok(Change::Created { playlists: back })
            }
            Change::Renamed { from, to } => {
                if store.text(&to).await.is_none() || store.text(&from).await.is_some() {
                    return Err(changed());
                }
                self.rename_unkept(&to, &from).await?;
                Ok(Change::Renamed { from: to, to: from })
            }
            Change::Spoken { playlist, before, after } => {
                if self.sync.spoken().remove(&playlist).unwrap_or_default() != after || store.text(&playlist).await.is_none() {
                    return Err(changed());
                }
                self.sync.set_spoken(&playlist, &before)?;
                Ok(Change::Spoken { playlist, before: after, after: before })
            }
            Change::Crate { before, after } => {
                let Some(id) = after.as_ref().or(before.as_ref()).map(|c| c.id.clone()) else { return Err(changed()) };
                if self.crate_store.list().await?.into_iter().find(|c| c.id == id) != after {
                    return Err(changed());
                }
                match &before {
                    Some(b) => self.crate_store.put(b.clone()).await.map_err(ToggleError::Refused)?,
                    None => {
                        self.crate_store.delete(&id).await?;
                    }
                }
                Ok(Change::Crate { before: after, after: before })
            }
        }
    }

    /// Every playlist, marked where it holds `entry` (the playing track's line), with its links.
    pub(super) async fn view(&self, entry: Option<&str>) -> Result<Vec<PlaylistView>> {
        let Some(store) = self.lib.store_opt() else { return Ok(Vec::new()) };
        let mut list = store.view(entry).await?;
        let (mut links, mut spoken) = (self.sync.links(), self.sync.spoken());
        for p in &mut list {
            p.links = links.remove(&p.id).unwrap_or_default();
            p.spoken = spoken.remove(&p.id).unwrap_or_default();
        }
        Ok(list)
    }

    /// The playlist meant by what was said, as its tracks on a server (decision 0015): Cratarr is the playlist server;
    /// a voice assistant asks it and plays the tracks itself. `limit` at most PLAY_MAX.
    pub async fn by_name(&self, said: &str, shuffled: bool, limit: usize, want: Option<&str>) -> Result<ByName, ToggleError> {
        let list = self.view(None).await?;
        let named: Vec<crate::voice::Named> = list.iter().map(|p| crate::voice::Named { name: &p.id, spoken: &p.spoken }).collect();
        let Some(id) = crate::voice::find(crate::voice::spoken_name(said), &named).map(str::to_string) else {
            return Ok(ByName { playlist: None, tracks: Vec::new() });
        };
        let (_, _, mut tracks, _) = self.server_ids(&id, want).await?;
        if shuffled {
            shuffle(&mut tracks);
        }
        tracks.truncate(limit.clamp(1, PLAY_MAX));
        Ok(ByName { playlist: Some(id), tracks })
    }

    /// Set the other names a playlist is spoken as. Blank ones are dropped. Returns the undo token (none when nothing
    /// changed).
    pub async fn set_spoken(&self, id: &str, names: &[String]) -> Result<Option<String>, ToggleError> {
        if self.lib.store()?.entries(id).await.is_err() {
            return Err(ToggleError::UnknownPlaylist);
        }
        let names: Vec<String> = names.iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
        let before = self.sync.spoken().remove(id).unwrap_or_default();
        self.sync.set_spoken(id, &names)?;
        Ok((before != names).then(|| self.keep(Change::Spoken { playlist: id.to_string(), before, after: names })).flatten())
    }

    /// What a playlist can be linked to.
    pub async fn targets(&self) -> Vec<super::sync::TargetChoice> {
        self.sync.choices().await
    }

    /// Link a playlist to a target (decision 0014), or change how it is kept.
    pub async fn link(&self, id: &str, target: &str) -> Result<(), ToggleError> {
        let kind = Kind::from_key(target).ok_or_else(|| ToggleError::Refused("not a target".into()))?;
        self.sync.link(id, kind).await
    }

    /// Take a link off: its copy on the target is deleted.
    pub async fn unlink(&self, id: &str, target: &str) -> Result<(), ToggleError> {
        let kind = Kind::from_key(target).ok_or_else(|| ToggleError::Refused("not a target".into()))?;
        self.sync.unlink(id, &kind).await
    }

    /// Make `entry` a member of the playlist or not (`Some`), or flip it (`None`, older pages). Only a real change
    /// is passed on.
    pub(super) async fn set_entry(&self, playlist_id: &str, entry: &str, member: Option<bool>) -> Result<(), ToggleError> {
        let store = self.lib.store()?;
        let changed = match member {
            Some(m) => store.set_member(playlist_id, entry, m).await?,
            None => {
                store.toggle(playlist_id, entry).await?;
                true
            }
        };
        if changed {
            self.tell_sources(vec![(playlist_id.to_string(), PlaylistChange::Edited)]);
        }
        Ok(())
    }

    /// Play a playlist on the server's player: its tracks in file order (or shuffled), looked up in the catalogue,
    /// at most PLAY_MAX of them. `want` = the server to use (default: the first connected one).
    pub async fn play(&self, id: &str, mode: PlayMode, want: Option<&str>) -> Result<PlayResult, ToggleError> {
        let (active, source, mut ids, unknown) = self.server_ids(id, want).await?;
        let total = ids.len();
        if total == 0 {
            // Usually the catalogue is still matching files to the server (a first fill takes minutes): say so.
            let c = self.lib.catalogue();
            let status = tokio::task::spawn_blocking(move || c.status()).await.ok().and_then(Result::ok);
            let why = match status {
                Some(st) if st.linked.get(&active.cfg.id).copied().unwrap_or(0) < st.files => format!(
                    "Cratarr is still matching your music with {} ({} of {} tracks); try again in a few minutes",
                    label_of(&active.cfg),
                    st.linked.get(&active.cfg.id).copied().unwrap_or(0),
                    st.files
                ),
                _ => format!("none of this playlist's tracks are on {}", label_of(&active.cfg)),
            };
            return Err(ToggleError::CannotPlay(why));
        }
        if mode == PlayMode::Shuffle {
            shuffle(&mut ids);
        }
        ids.truncate(PLAY_MAX);
        let how = match mode {
            PlayMode::Play | PlayMode::Shuffle => source::PlayHow::Now,
            PlayMode::Next => source::PlayHow::Next,
            PlayMode::Last => source::PlayHow::Last,
        };
        let player = source.play_tracks(&ids, how).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))?;
        Ok(PlayResult { player, sent: ids.len(), total, unknown })
    }

    /// A playlist's tracks for the local player (decision 0022): the server they are known on, their ids in file order,
    /// and how many lines it does not know.
    pub async fn track_ids(&self, id: &str, want: Option<&str>) -> Result<(String, Vec<String>, usize), ToggleError> {
        let (active, _, ids, unknown) = self.server_ids(id, want).await?;
        Ok((active.cfg.id.clone(), ids, unknown))
    }

    /// A playlist's rows for the playlist view (decision 0023), in file order: each entry with what the catalogue knows
    /// of its file (the tags, the length, the server's id when matched) and whether the file is there. One read of the
    /// catalogue, no request to the server. Returns the server the ids are for.
    pub async fn rows(&self, id: &str, want: Option<&str>) -> Result<(String, Vec<PlaylistRow>), ToggleError> {
        let entries = self.lib.store()?.entries(id).await?;
        let sources = self.lib.connected();
        let source = want
            .and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .map(|(a, _)| a.cfg.id.clone())
            .unwrap_or_default();
        let rows = self.rows_of(entries, &source).await?;
        Ok((source, rows))
    }

    /// Rows for these lines (a playlist's, or a page of a crate's), with what the catalogue knows of their files.
    async fn rows_of(&self, entries: Vec<String>, source: &str) -> Result<Vec<PlaylistRow>, ToggleError> {
        let (c, sid, music) = (self.lib.catalogue(), source.to_string(), self.lib.music());
        let rows = tokio::task::spawn_blocking(move || -> Result<Vec<PlaylistRow>> {
            let facts = c.file_rows(&sid, 0, &entries)?;
            let dir = music.map(std::path::PathBuf::from);
            Ok(entries
                .into_iter()
                .zip(facts)
                .map(|(entry, f)| PlaylistRow {
                    missing: !dir.as_deref().is_some_and(|d| crate::playlists::entry_exists(d, &entry)),
                    file: entry.rsplit('/').next().unwrap_or(&entry).to_string(),
                    entry,
                    id: f.track_id,
                    name: f.title,
                    artist: f.artist,
                    album: f.album,
                    duration_secs: f.duration_ms.map(|ms| ms as f64 / 1000.0),
                })
                .collect())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        Ok(rows)
    }

    /// An export's plan (decision 0007): the playlists' lines as files in the Music folder, each file once, with its
    /// size and the tags its name in one folder needs. Read from the disk and the catalogue; no request to the server.
    pub async fn export_plan(&self, names: &[String]) -> Result<crate::export::Plan, ToggleError> {
        let music = self.lib.music().ok_or(ToggleError::NoMusicFolder)?;
        let store = self.lib.store()?;
        let mut lists = Vec::new();
        for n in names {
            lists.push((n.clone(), store.entries(n).await?));
        }
        let c = self.lib.catalogue();
        let plan = tokio::task::spawn_blocking(move || {
            crate::export::plan(std::path::Path::new(&music), lists, |rels| {
                Ok(c.file_rows("", 0, rels)?.into_iter().map(|f| (f.artist, f.title)).collect())
            })
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        Ok(plan)
    }

    /// The server (`want`, else the first connected) and its id.
    fn source_of(&self, want: Option<&str>) -> Result<(String, Arc<dyn source::Source>), ToggleError> {
        let sources = self.lib.connected();
        want.and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .map(|(a, s)| (a.cfg.id.clone(), s.clone()))
            .ok_or_else(|| ToggleError::Refused("No server is connected.".into()))
    }

    /// Every crate (the permanent ones first, then one's own) with how many tracks it holds now (decision 0024).
    pub async fn crates(&self, want: Option<&str>) -> Result<Vec<CrateView>, ToggleError> {
        let own = self.crate_store.list().await?;
        let all: Vec<(bool, CrateDef)> = crates::permanent().into_iter().map(|c| (true, c)).chain(own.into_iter().map(|c| (false, c))).collect();
        let set = self.crate_store.all_favourites().await?;
        // The favourites are asked for only when a crate shows by them (its conditions, or its toggle): asking the
        // media server for every favourite is slow, and a list of crates that need none must not wait for it. The
        // other settings' counts are then unknown (null) until it is asked.
        let needed = all.iter().any(|(_, c)| {
            crates::needs_favourites(&c.conditions) || set.get(&c.id).copied().unwrap_or(crates::Favourites::None) != crates::Favourites::All
        });
        let lib = self.crate_library(want, needed).await?;
        let count = |conditions: Vec<crates::Condition>| {
            (!crates::needs_favourites(&conditions) || lib.favourites.is_some()).then(|| lib.select(&conditions).len())
        };
        Ok(all
            .into_iter()
            .map(|(permanent, c)| CrateView {
                counts: CrateCounts {
                    all: count(crates::Favourites::All.with(&c.conditions)),
                    only: count(crates::Favourites::Only.with(&c.conditions)),
                    none: count(crates::Favourites::None.with(&c.conditions)),
                },
                favourites: set.get(&c.id).copied().unwrap_or(crates::Favourites::None),
                id: c.id,
                name: c.name,
                permanent,
            })
            .collect())
    }

    /// A crate's definition: a permanent one, or one's own.
    pub async fn crate_def(&self, id: &str) -> Result<CrateDef, ToggleError> {
        if let Some(c) = crates::permanent().into_iter().find(|c| c.id == id) {
            return Ok(c);
        }
        self.crate_store.list().await?.into_iter().find(|c| c.id == id).ok_or(ToggleError::UnknownPlaylist)
    }

    /// The lines these conditions select, newest first, and the server they are for.
    async fn crate_lines_of(&self, conditions: &[crates::Condition], want: Option<&str>) -> Result<(String, Vec<String>), ToggleError> {
        let (lib, lines) = self.crate_selection(conditions, want).await?;
        Ok((lib.source, lines))
    }

    /// The library a crate is worked out from, and its lines.
    async fn crate_selection(&self, conditions: &[crates::Condition], want: Option<&str>) -> Result<(CrateLibrary, Vec<String>), ToggleError> {
        let lib = self.crate_library(want, crates::needs_favourites(conditions)).await?;
        if crates::needs_favourites(conditions) && lib.favourites.is_none() {
            return Err(ToggleError::CannotPlay("The server's favourites could not be read.".into()));
        }
        let lines = lib.select(conditions);
        Ok((lib, lines))
    }

    /// A page of a crate's rows (it can hold thousands): from `offset`, at most `limit`, with the crate's size.
    /// With the panel's favourites toggle (`fav`).
    /// A page of a crate's rows, as its favourites toggle is set (`fav`, else as saved).
    pub async fn crate_rows(
        &self,
        id: &str,
        want: Option<&str>,
        offset: usize,
        limit: usize,
        fav: Option<crates::Favourites>,
    ) -> Result<CratePage, ToggleError> {
        let def = self.crate_def(id).await?;
        let fav = match fav {
            Some(f) => f,
            None => self.crate_store.favourites(id).await?,
        };
        self.crate_page(&fav.with(&def.conditions), want, offset, limit).await
    }

    /// Sets a crate's favourites toggle (decision 0024, amended): one setting per crate, for every device. Not journaled:
    /// a tap undoes it by tapping again.
    pub async fn set_crate_favourites(&self, id: &str, fav: crates::Favourites) -> Result<(), ToggleError> {
        self.crate_def(id).await?;
        Ok(self.crate_store.set_favourites(id, fav).await?)
    }

    /// The builder's live preview: an unsaved crate's size and first rows (decision 0024, issue 6).
    pub async fn crate_page(&self, conditions: &[crates::Condition], want: Option<&str>, offset: usize, limit: usize) -> Result<CratePage, ToggleError> {
        let (lib, lines) = self.crate_selection(conditions, want).await?;
        // On the first page only: the panel shows them on top.
        let genres = if offset == 0 {
            let chosen: HashSet<&str> = lines.iter().map(String::as_str).collect();
            crates::top_genres(lib.files.iter().filter(|f| chosen.contains(f.rel.as_str())).map(|f| f.genres.as_slice()), super::playing::GENRES_SHOWN)
        } else {
            Vec::new()
        };
        let source = lib.source;
        let total = lines.len();
        let page: Vec<String> = lines.into_iter().skip(offset).take(limit).collect();
        let rows = self.rows_of(page, &source).await?;
        Ok(CratePage { source, total, rows, genres })
    }

    /// Make playlist: the crate's tracks now, as a new, ordinary playlist (a snapshot; decision 0024).
    /// The crate as its favourites toggle is set. Returns how many tracks it got, and the undo token.
    pub async fn crate_to_playlist(&self, id: &str, want: Option<&str>, name: &str) -> Result<(usize, Option<String>), ToggleError> {
        let def = self.crate_def(id).await?;
        let fav = self.crate_store.favourites(id).await?;
        let (_, lines) = self.crate_lines_of(&fav.with(&def.conditions), want).await?;
        if lines.is_empty() {
            return Err(ToggleError::Refused("That crate is empty.".into()));
        }
        let undo = self.create_with(name, &lines).await?;
        Ok((lines.len(), undo))
    }

    /// One's own crate, if there is one with that id.
    async fn own_crate(&self, id: &str) -> Result<Option<CrateDef>> {
        Ok(self.crate_store.list().await?.into_iter().find(|c| c.id == id))
    }

    /// Saves one's own crate (a new one without `id`); returns its id and the undo token.
    pub async fn crate_save(&self, id: Option<&str>, name: &str, conditions: Vec<crates::Condition>) -> Result<(String, Option<String>), ToggleError> {
        if id.is_some_and(crates::is_permanent) {
            return Err(ToggleError::Refused("A permanent crate cannot be changed.".into()));
        }
        let before = match id {
            Some(id) => self.own_crate(id).await?,
            None => None,
        };
        let id = self.crate_store.save(id, name, conditions).await.map_err(ToggleError::Refused)?;
        let after = self.own_crate(&id).await?;
        let undo = (before != after).then(|| self.keep(Change::Crate { before, after })).flatten();
        Ok((id, undo))
    }

    /// Returns the undo token.
    pub async fn crate_delete(&self, id: &str) -> Result<Option<String>, ToggleError> {
        if crates::is_permanent(id) {
            return Err(ToggleError::Refused("A permanent crate cannot be deleted.".into()));
        }
        let before = self.own_crate(id).await?;
        if !self.crate_store.delete(id).await? {
            return Err(ToggleError::UnknownPlaylist);
        }
        Ok(self.keep(Change::Crate { before, after: None }))
    }

    /// The axes and their domains in this library, for the builder (decision 0024, issue 4).
    pub async fn crate_axes(&self, want: Option<&str>) -> Result<Vec<crates::AxisView>, ToggleError> {
        let lib = self.crate_library(want, false).await?;
        let playlists: Vec<String> = self.lib.store()?.snapshot().await?.into_iter().map(|(name, _, _)| name).collect();
        Ok(crates::axes(&lib.files, &playlists))
    }

    /// What a crate is worked out on: Music's files newest first with their facts, which playlists hold each line, and
    /// (when needed) the favourites, kept for a minute (decision 0024, issue 5).
    async fn crate_library(&self, want: Option<&str>, favourites: bool) -> Result<CrateLibrary, ToggleError> {
        let (sid, source) = self.source_of(want)?;
        // First, so a request waiting for them holds nothing large meanwhile.
        let favourites = if favourites { self.favourite_lines(&sid, &source).await } else { None };
        let c = self.lib.catalogue();
        let files = tokio::task::spawn_blocking(move || c.crate_facts(0)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        let mut membership: HashMap<String, Vec<String>> = HashMap::new();
        for (name, lines, _) in self.lib.store()?.snapshot().await? {
            for line in lines {
                membership.entry(line).or_default().push(name.clone());
            }
        }
        Ok(CrateLibrary { source: sid, files, membership, favourites })
    }

    /// The favourite tracks' lines, from the media server. What was heard last is answered at once, with Cratarr's own
    /// changes since applied; when it is a minute old, it is asked for again in the background. Only the first asking
    /// (nothing heard yet from this server) waits. One asks at a time: Jellyfin took a minute for 4,786 favourites
    /// (2026-10-14), and asking again while it works makes it slower. A server that does not answer is asked again a
    /// minute later.
    async fn favourite_lines(&self, sid: &str, source: &Arc<dyn source::Source>) -> Option<HashSet<String>> {
        let f = &self.favourites;
        let kept = match f.kept(sid) {
            Some(k) => {
                if k.at.elapsed() >= FAVOURITES_KEEP
                    && let Ok(turn) = f.asking.clone().try_lock_owned()
                {
                    let (f, c, s, from) = (f.clone(), self.lib.catalogue(), sid.to_string(), source.clone());
                    tokio::spawn(async move {
                        let _turn = turn;
                        f.ask(c, &s, &from).await;
                    });
                }
                k
            }
            None => {
                let _turn = f.asking.lock().await;
                match f.kept(sid) {
                    Some(k) => k,
                    None => f.ask(self.lib.catalogue(), sid, source).await,
                }
            }
        };
        FavouritesKept::with_changes(kept, self.lib.catalogue()).await
    }

    /// A playlist's tracks as a server's ids, in file order (`want` = the server to use, default the first connected
    /// one), and how many lines it does not know (missing files, or not linked yet).
    async fn server_ids(
        &self,
        id: &str,
        want: Option<&str>,
    ) -> Result<(Arc<super::library::Active>, Arc<dyn source::Source>, Vec<String>, usize), ToggleError> {
        let entries = self.lib.store()?.entries(id).await?;
        let sources = self.lib.connected();
        let (active, source) = want
            .and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .cloned()
            .ok_or_else(|| ToggleError::CannotPlay("no server is connected".into()))?;
        let (c, sid) = (self.lib.catalogue(), active.cfg.id.clone());
        let found = tokio::task::spawn_blocking(move || c.track_ids(&sid, 0, &entries))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))??;
        let unknown = found.iter().filter(|f| f.is_none()).count();
        Ok((active, source, found.into_iter().flatten().collect(), unknown))
    }

    /// After an edit: the playlist's copies follow (the shared one at once, the others with the next pass).
    fn tell_sources(&self, changes: Vec<(String, PlaylistChange)>) {
        self.sync.changed(changes.into_iter().map(|(name, _)| name).collect());
    }

    /// 0 when there is no Music folder.
    pub async fn count(&self) -> Result<usize> {
        Ok(self.view(None).await?.len())
    }

    pub async fn repairs(&self) -> Result<Repairs> {
        self.lib.store()?.repairs().await
    }

    /// Apply the repairs the client chose (`None` = every safe one, or only those of the playlists `within`). The store
    /// only accepts pairs it currently offers. The sources are then told about each changed playlist. Returns the
    /// changed playlists and the undo token (one for all of them).
    pub async fn apply_repairs(&self, only: Option<Repair>, within: Option<&[String]>) -> Result<(Vec<String>, Option<String>), ToggleError> {
        let wanted = match only {
            Some(one) => vec![one],
            None => safe_within(self.lib.store()?.repairs().await?.safe, within),
        };
        let edits = self.lib.store()?.apply_repairs(&wanted).await?;
        let changed: Vec<String> = edits.iter().map(|e| e.id.clone()).collect();
        self.tell_sources(changed.iter().map(|n| (n.clone(), PlaylistChange::Edited)).collect());
        Ok((changed, self.keep_texts(edits)))
    }

    /// Album folders that look like the same release twice, each group compared. Read-only. Track details come
    /// from the first source that knows the folder.
    pub async fn album_duplicates(&self) -> Result<Vec<crate::albums::AlbumGroup>> {
        use crate::albums::*;
        let Some(music) = self.lib.music().map(PathBuf::from) else { return Ok(Vec::new()) };
        let store = self.lib.store()?;
        let dir = music.clone();
        let folders: Vec<String> = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
            Ok(std::fs::read_dir(&dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .collect())
        })
        .await??;
        let sources = self.lib.connected();
        let mut out = Vec::new();
        for group in candidate_groups(&folders) {
            let mut albums = Vec::new();
            for folder in &group {
                let (m, f) = (music.clone(), folder.clone());
                let (files, cover) = tokio::task::spawn_blocking(move || read_folder(&m, &f)).await??;
                let mut known = Default::default();
                for (a, s) in &sources {
                    match s.album_details(folder).await {
                        Ok(k) if !k.is_empty() => {
                            known = k;
                            break;
                        }
                        Ok(_) => {}
                        Err(e) => tracing::warn!("source {} has no details for {folder:?}: {e:#}", a.cfg.id),
                    }
                }
                let lines = store.lines_in_folder(folder).await?;
                albums.push(build_view(folder, files, cover, &known, lines));
            }
            let comparisons = albums.iter().skip(1).map(|b| compare(&albums[0], b)).collect();
            let suggestion = suggest(&albums);
            out.push(AlbumGroup { albums, comparisons, suggestion });
        }
        Ok(out)
    }

    pub async fn duplicates(&self) -> Result<Vec<DupGroup>> {
        self.lib.store()?.duplicates().await
    }

    /// Remove repeated entries from one playlist (the first copy of each stays). Returns how many went, and the undo
    /// token.
    pub async fn dedupe(&self, id: &str) -> Result<(usize, Option<String>), ToggleError> {
        let (removed, edit) = self.lib.store()?.dedupe(id).await?;
        if removed > 0 {
            self.tell_sources(vec![(id.to_string(), PlaylistChange::Edited)]);
        }
        Ok((removed, self.keep_texts(vec![edit])))
    }

    /// A new playlist is Shared, as playlists were before sync links (decision 0014). Returns the undo token.
    pub async fn create(&self, name: &str) -> Result<Option<String>, ToggleError> {
        self.lib.store()?.create(name).await?;
        self.sync.new_playlist(name).await?;
        Ok(self.keep(Change::Created { playlists: vec![(name.to_string(), fingerprint(""))] }))
    }

    /// A playlist edited in its view (decision 0023): its lines put in a new order or left out, as one write. Only the
    /// lines it has (`expected`, as the view showed them) can be kept; it is refused when the playlist changed since.
    /// Returns the undo token (none when nothing changed).
    pub async fn edit(&self, id: &str, expected: &[String], entries: &[String]) -> Result<Option<String>, ToggleError> {
        if !is_rearrangement(expected, entries) {
            return Err(ToggleError::Refused("An edit can only reorder or remove the playlist's own lines.".into()));
        }
        let Some(edit) = self.lib.store()?.replace_entries_if(id, expected, entries).await? else {
            return Err(ToggleError::Refused("The playlist changed meanwhile: look at it again.".into()));
        };
        self.tell_sources(vec![(id.to_string(), PlaylistChange::Edited)]);
        Ok(self.keep_texts(vec![edit]))
    }

    /// The server (`want`, else the first connected) a playlist favourite is kept on.
    fn favourites_source(&self, want: Option<&str>) -> Result<Arc<dyn source::Source>, ToggleError> {
        let sources = self.lib.connected();
        want.and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .map(|(_, s)| s.clone())
            .ok_or_else(|| ToggleError::Refused("No server is connected.".into()))
    }

    /// Which playlists (by name) are favourites, as Finamp's playlist hearts (decision 0023).
    pub async fn favorites(&self, want: Option<&str>) -> Result<std::collections::BTreeMap<String, bool>, ToggleError> {
        let s = self.favourites_source(want)?;
        s.playlist_favorites().await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// Makes a playlist a favourite or not (every copy on the server).
    pub async fn set_favorite(&self, id: &str, want: Option<&str>, on: bool) -> Result<(), ToggleError> {
        let s = self.favourites_source(want)?;
        s.set_playlist_favorite(id, on).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// A new playlist holding these entries, in order (a playlist made from the queue, decision 0019). Returns the
    /// undo token.
    pub async fn create_with(&self, name: &str, entries: &[String]) -> Result<Option<String>, ToggleError> {
        let store = self.lib.store()?;
        store.create(name).await?;
        // Filled only while still empty: never over an edit made since it was created.
        let Some(filled) = store.replace_entries_if(name, &[], entries).await? else {
            return Err(ToggleError::Refused("The new playlist changed while it was being filled.".into()));
        };
        self.sync.new_playlist(name).await?;
        Ok(self.keep(Change::Created { playlists: vec![(name.to_string(), fingerprint(&filled.after))] }))
    }

    /// Returns the undo token.
    pub async fn rename(&self, from: &str, to: &str) -> Result<Option<String>, ToggleError> {
        self.rename_unkept(from, to).await?;
        Ok(self.keep(Change::Renamed { from: from.to_string(), to: to.to_string() }))
    }

    async fn rename_unkept(&self, from: &str, to: &str) -> Result<(), ToggleError> {
        let store = self.lib.store()?;
        // Through the sync, so the server copies keep following the playlist (decision 0012).
        self.sync.rename(from, to, store.rename(from, to)).await?;
        self.tell_sources(vec![(from.to_string(), PlaylistChange::Deleted), (to.to_string(), PlaylistChange::Created)]);
        Ok(())
    }

    /// The store keeps it aside; its undo brings it back with the links it had. Returns the undo token.
    pub async fn delete(&self, id: &str) -> Result<Option<String>, ToggleError> {
        self.lib.store()?.delete(id).await?;
        self.tell_sources(vec![(id.to_string(), PlaylistChange::Deleted)]);
        Ok(self.keep(Change::Deleted { playlists: vec![id.to_string()] }))
    }

    /// Re-check which playlist entries point at missing files (the library changes without the playlists).
    pub async fn refresh_dead(&self) {
        if let Some(store) = self.lib.store_opt() {
            store.refresh_dead().await;
        }
    }
}

/// Best effort, in the background: every connected source hears about changed playlist files, each with the file's
/// path as it sees it.
pub(super) fn tell_sources(lib: &Library, changes: Vec<(String, PlaylistChange)>) {
    let Some(music) = lib.music() else { return };
    for (a, s) in lib.connected() {
        let events: Vec<PlaylistEvent> = changes
            .iter()
            .map(|(name, change)| PlaylistEvent { name: name.clone(), path: a.placement.playlist_path(&music, name), change: *change })
            .collect();
        tokio::spawn(async move {
            if let Err(e) = s.playlists_changed(events).await {
                tracing::warn!("source {}: could not pass on playlist changes: {e:#}", a.cfg.id);
            }
        });
    }
}

#[cfg(test)]
mod favourites_tests {
    use super::*;

    #[tokio::test]
    async fn the_favourites_heard_are_kept_for_the_next_start_and_asked_again_at_once() {
        let dir = std::env::temp_dir().join(format!("cratarr-fav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("favourites.json");
        let first = FavouritesKept::load(Some(file.clone()));
        assert!(first.kept("jellyfin").is_none(), "nothing kept yet");
        first.save("jellyfin", &Arc::new(["A/1.flac".to_string(), "B/2.mp3".to_string()].into_iter().collect())).await;

        let next = FavouritesKept::load(Some(file));
        let k = next.kept("jellyfin").expect("kept from the last run");
        assert_eq!(k.lines.as_deref().map(|l| l.len()), Some(2));
        assert!(k.at.elapsed() >= FAVOURITES_KEEP, "due to be asked again");
        assert!(next.kept("other").is_none(), "only for the server it came from");
        std::fs::remove_dir_all(&dir).ok();
    }
}
