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
use crate::playlists::{DupGroup, PlaylistView, Repair, Repairs};
use crate::source::{self, PlaylistChange, PlaylistEvent};
use crate::sync_state::{Conflict, Direction, Kind};

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
struct CrateLibrary {
    source: String,
    files: Vec<crates::Facts>,
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
    pub count: Option<usize>,
    pub permanent: bool,
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
    /// The favourite tracks' lines for the crates: when, for which server, at which change count (see
    /// `favourite_lines`).
    favourites_kept: tokio::sync::Mutex<Option<(std::time::Instant, String, u64, HashSet<String>)>>,
}

impl PlaylistService {
    pub(super) fn new(lib: Arc<Library>, sync: Arc<super::sync::SyncService>) -> Self {
        let crate_store = crates::CrateStore::new(lib.data_dir());
        Self { lib, sync, crate_store, favourites_kept: tokio::sync::Mutex::new(None) }
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

    /// Set the other names a playlist is spoken as. Blank ones are dropped.
    pub async fn set_spoken(&self, id: &str, names: &[String]) -> Result<(), ToggleError> {
        if self.lib.store()?.entries(id).await.is_err() {
            return Err(ToggleError::UnknownPlaylist);
        }
        let names: Vec<String> = names.iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
        self.sync.set_spoken(id, &names)
    }

    /// What a playlist can be linked to.
    pub async fn targets(&self) -> Vec<super::sync::TargetChoice> {
        self.sync.choices().await
    }

    /// Link a playlist to a target (decision 0014), or change how it is kept.
    pub async fn link(&self, id: &str, target: &str, direction: Direction, conflict: Conflict) -> Result<(), ToggleError> {
        let kind = Kind::from_key(target).ok_or_else(|| ToggleError::Refused("not a target".into()))?;
        self.sync.link(id, kind, direction, conflict).await
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
        let lib = self.crate_library(want, all.iter().any(|(_, c)| crates::needs_favourites(&c.conditions))).await?;
        Ok(all
            .into_iter()
            .map(|(permanent, c)| CrateView {
                count: if crates::needs_favourites(&c.conditions) && lib.favourites.is_none() { None } else { Some(lib.select(&c.conditions).len()) },
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
        let lib = self.crate_library(want, crates::needs_favourites(conditions)).await?;
        if crates::needs_favourites(conditions) && lib.favourites.is_none() {
            return Err(ToggleError::CannotPlay("The server's favourites could not be read.".into()));
        }
        Ok((lib.source.clone(), lib.select(conditions)))
    }

    /// A page of a crate's rows (it can hold thousands): from `offset`, at most `limit`, with the crate's size.
    /// With the panel's favourites toggle (`fav`).
    pub async fn crate_rows(
        &self,
        id: &str,
        want: Option<&str>,
        offset: usize,
        limit: usize,
        fav: crates::Favourites,
    ) -> Result<(String, usize, Vec<PlaylistRow>), ToggleError> {
        let def = self.crate_def(id).await?;
        self.crate_page(&fav.with(&def.conditions), want, offset, limit).await
    }

    /// The builder's live preview: an unsaved crate's size and first rows (decision 0024, issue 6).
    pub async fn crate_page(
        &self,
        conditions: &[crates::Condition],
        want: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<(String, usize, Vec<PlaylistRow>), ToggleError> {
        let (source, lines) = self.crate_lines_of(conditions, want).await?;
        let total = lines.len();
        let page: Vec<String> = lines.into_iter().skip(offset).take(limit).collect();
        let rows = self.rows_of(page, &source).await?;
        Ok((source, total, rows))
    }

    /// Make playlist: the crate's tracks now, as a new, ordinary playlist (a snapshot; decision 0024).
    pub async fn crate_to_playlist(&self, id: &str, want: Option<&str>, name: &str, fav: crates::Favourites) -> Result<usize, ToggleError> {
        let def = self.crate_def(id).await?;
        let (_, lines) = self.crate_lines_of(&fav.with(&def.conditions), want).await?;
        if lines.is_empty() {
            return Err(ToggleError::Refused("That crate is empty.".into()));
        }
        self.create_with(name, &lines).await?;
        Ok(lines.len())
    }

    /// Saves one's own crate (a new one without `id`); returns its id.
    pub async fn crate_save(&self, id: Option<&str>, name: &str, conditions: Vec<crates::Condition>) -> Result<String, ToggleError> {
        if id.is_some_and(crates::is_permanent) {
            return Err(ToggleError::Refused("A permanent crate cannot be changed.".into()));
        }
        self.crate_store.save(id, name, conditions).await.map_err(ToggleError::Refused)
    }

    pub async fn crate_delete(&self, id: &str) -> Result<(), ToggleError> {
        if crates::is_permanent(id) {
            return Err(ToggleError::Refused("A permanent crate cannot be deleted.".into()));
        }
        if !self.crate_store.delete(id).await? {
            return Err(ToggleError::UnknownPlaylist);
        }
        Ok(())
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
        let c = self.lib.catalogue();
        let files = tokio::task::spawn_blocking(move || c.crate_facts(0)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        let mut membership: HashMap<String, Vec<String>> = HashMap::new();
        for (name, lines, _) in self.lib.store()?.snapshot().await? {
            for line in lines {
                membership.entry(line).or_default().push(name.clone());
            }
        }
        let favourites = if favourites { self.favourite_lines(&sid, &source).await } else { None };
        Ok(CrateLibrary { source: sid, files, membership, favourites })
    }

    /// The favourite tracks' lines, from the media server, kept a minute (or until Cratarr changes a favourite).
    async fn favourite_lines(&self, sid: &str, source: &Arc<dyn source::Source>) -> Option<HashSet<String>> {
        const KEEP: std::time::Duration = std::time::Duration::from_secs(60);
        let generation = crates::favourites_generation();
        if let Some((at, s, g, lines)) = self.favourites_kept.lock().await.as_ref()
            && at.elapsed() < KEEP
            && s == sid
            && *g == generation
        {
            return Some(lines.clone());
        }
        let ids = match source.favorite_tracks().await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!("favourites for the crates: {e:#}");
                return None;
            }
        };
        let (c, s) = (self.lib.catalogue(), sid.to_string());
        let lines = tokio::task::spawn_blocking(move || c.rels_of(&s, 0, &ids)).await.ok()?.ok()?;
        *self.favourites_kept.lock().await = Some((std::time::Instant::now(), sid.to_string(), generation, lines.clone()));
        Some(lines)
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
    /// only accepts pairs it currently offers. The sources are then told about each changed playlist.
    pub async fn apply_repairs(&self, only: Option<Repair>, within: Option<&[String]>) -> Result<Vec<String>, ToggleError> {
        let wanted = match only {
            Some(one) => vec![one],
            None => safe_within(self.lib.store()?.repairs().await?.safe, within),
        };
        let changed = self.lib.store()?.apply_repairs(&wanted).await?;
        self.tell_sources(changed.iter().map(|n| (n.clone(), PlaylistChange::Edited)).collect());
        Ok(changed)
    }

    /// Put each playlist back as it was before its last change.
    pub async fn undo(&self, playlists: Vec<String>) -> Result<usize, ToggleError> {
        let mut done = Vec::new();
        for id in playlists {
            self.lib.store()?.undo(&id).await?;
            done.push(id);
        }
        let n = done.len();
        self.tell_sources(done.into_iter().map(|n| (n, PlaylistChange::Edited)).collect());
        Ok(n)
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

    /// Remove repeated entries from one playlist (the first copy of each stays).
    pub async fn dedupe(&self, id: &str) -> Result<usize, ToggleError> {
        let removed = self.lib.store()?.dedupe(id).await?;
        if removed > 0 {
            self.tell_sources(vec![(id.to_string(), PlaylistChange::Edited)]);
        }
        Ok(removed)
    }

    /// A new playlist is Shared, as playlists were before sync links (decision 0014).
    pub async fn create(&self, name: &str) -> Result<(), ToggleError> {
        self.lib.store()?.create(name).await?;
        self.sync.new_playlist(name).await
    }

    /// A playlist edited in its view (decision 0023): its lines put in a new order or left out, as one write. Only the
    /// lines it has (`expected`, as the view showed them) can be kept; it is refused when the playlist changed since.
    pub async fn edit(&self, id: &str, expected: &[String], entries: &[String]) -> Result<(), ToggleError> {
        if !is_rearrangement(expected, entries) {
            return Err(ToggleError::Refused("An edit can only reorder or remove the playlist's own lines.".into()));
        }
        if !self.lib.store()?.set_entries_if(id, expected, entries).await? {
            return Err(ToggleError::Refused("The playlist changed meanwhile: look at it again.".into()));
        }
        self.tell_sources(vec![(id.to_string(), PlaylistChange::Edited)]);
        Ok(())
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

    /// A new playlist holding these entries, in order (a playlist made from the queue, decision 0019).
    pub async fn create_with(&self, name: &str, entries: &[String]) -> Result<(), ToggleError> {
        let store = self.lib.store()?;
        store.create(name).await?;
        // Filled only while still empty: never over an edit made since it was created.
        if !store.set_entries_if(name, &[], entries).await? {
            return Err(ToggleError::Refused("The new playlist changed while it was being filled.".into()));
        }
        self.sync.new_playlist(name).await
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<(), ToggleError> {
        let store = self.lib.store()?;
        // Through the sync, so the server copies keep following the playlist (decision 0012).
        self.sync.rename(from, to, store.rename(from, to)).await?;
        self.tell_sources(vec![(from.to_string(), PlaylistChange::Deleted), (to.to_string(), PlaylistChange::Created)]);
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<(), ToggleError> {
        self.lib.store()?.delete(id).await?;
        self.tell_sources(vec![(id.to_string(), PlaylistChange::Deleted)]);
        Ok(())
    }

    /// It comes back with the links it had.
    pub async fn restore(&self, id: &str) -> Result<(), ToggleError> {
        self.lib.store()?.restore_deleted(id).await?;
        self.sync.restore_links(id).await
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
