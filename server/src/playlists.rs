//! The playlists: `.m3u` files in one folder. Reads are cheap (small text files, cached by modification stamp),
//! writes are careful: re-read right before the edit, back the file up, write a temporary file next to it and
//! rename it into place, so a crash or a concurrent edit can never leave a half-written playlist.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

use crate::m3u;

const BACKUP_DIR: &str = ".cratarr-backups";
const KEEP_BACKUPS: usize = 20;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlaylistView {
    pub id: String,
    pub name: String,
    pub member: bool,
    /// Tracks that resolve to a file in the library (what players show), i.e. entries minus `dead`.
    pub count: usize,
    /// Entries that point at a file that does not exist.
    pub dead: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("unknown playlist")]
    Unknown,
    #[error("that repair is no longer offered")]
    NotOffered,
    #[error("there is no earlier version to go back to")]
    NothingToUndo,
    #[error("a playlist with that name already exists")]
    Exists,
    #[error("that is not a usable playlist name")]
    BadName,
    #[error(transparent)]
    Io(#[from] anyhow::Error),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Toggled {
    pub added: bool,
}

/// One proposed fix: in `playlist`, replace the line `from` with `to`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Repair {
    pub playlist: String,
    pub from: String,
    pub to: String,
}

/// A playlist that lists the same entry more than once.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DupGroup {
    pub playlist: String,
    /// How many lines `Store::dedupe` would remove.
    pub extra: usize,
    pub entries: Vec<DupEntry>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DupEntry {
    pub entry: String,
    pub copies: usize,
}

/// Two playlists that share tracks (the old app's "Proximity"). `similarity` is shared / union (Jaccard), so a big
/// playlist does not look related to everything just by being big.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Related {
    pub a: String,
    pub b: String,
    pub shared: usize,
    pub similarity: f64,
}

/// Figures across all playlists, from the playlist files alone.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct PlaylistStats {
    pub playlists: usize,
    /// Entry lines, repeats included.
    pub lines: usize,
    /// Different tracks across all playlists.
    pub distinct_tracks: usize,
    /// Tracks that are in exactly one playlist.
    pub in_one_playlist: usize,
    /// Entries pointing at a missing file, summed over playlists.
    pub missing: usize,
    /// The tracks in the most playlists, most first (at most ten).
    pub most_shared: Vec<(String, usize)>,
}

/// Every pair of playlists sharing at least `min_shared` tracks, most similar first.
pub fn relate(playlists: &[(&str, &HashSet<String>)], min_shared: usize) -> Vec<Related> {
    let mut out = Vec::new();
    for (i, (a, ea)) in playlists.iter().enumerate() {
        for (b, eb) in &playlists[i + 1..] {
            let (small, large) = if ea.len() <= eb.len() { (ea, eb) } else { (eb, ea) };
            let shared = small.iter().filter(|e| large.contains(*e)).count();
            if shared == 0 || shared < min_shared {
                continue;
            }
            let union = ea.len() + eb.len() - shared;
            out.push(Related { a: a.to_string(), b: b.to_string(), shared, similarity: shared as f64 / union as f64 });
        }
    }
    out.sort_by(|x, y| y.similarity.total_cmp(&x.similarity).then(y.shared.cmp(&x.shared)).then(x.a.cmp(&y.a)));
    out
}

/// Figures for a set of playlists: (name, entry lines, missing count).
pub fn playlist_stats(playlists: &[(&str, &[String], usize)]) -> PlaylistStats {
    let mut per_track: HashMap<&str, usize> = HashMap::new();
    let mut lines = 0;
    let mut missing = 0;
    for (_, ls, dead) in playlists {
        lines += ls.len();
        missing += dead;
        let distinct: HashSet<&str> = ls.iter().map(String::as_str).collect();
        for t in distinct {
            *per_track.entry(t).or_default() += 1;
        }
    }
    let mut most: Vec<(String, usize)> = per_track.iter().filter(|(_, n)| **n > 1).map(|(t, n)| (t.to_string(), *n)).collect();
    most.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    most.truncate(10);
    PlaylistStats {
        playlists: playlists.len(),
        lines,
        distinct_tracks: per_track.len(),
        in_one_playlist: per_track.values().filter(|n| **n == 1).count(),
        missing,
        most_shared: most,
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Repairs {
    pub safe: Vec<Repair>,
    pub review: Vec<Repair>,
}

struct Cached {
    stamp: (SystemTime, u64),
    entries: HashSet<String>,
    /// Every entry line, in file order (duplicates kept).
    lines: Vec<String>,
    /// How many entries point at a missing file; `None` until checked (it changes with the library, not the file).
    dead: Option<usize>,
}

impl Cached {
    fn parse(stamp: (SystemTime, u64), text: &str) -> Self {
        let lines: Vec<String> = m3u::entries(text).collect();
        Self { stamp, entries: lines.iter().cloned().collect(), lines, dead: None }
    }
}

/// True when `entry` (library-relative) names an existing file under `dir`. Absolute paths and `..` never resolve.
fn entry_exists(dir: &Path, entry: &str) -> bool {
    let p = Path::new(entry);
    let safe = !p.is_absolute() && p.components().all(|c| matches!(c, std::path::Component::Normal(_)));
    safe && dir.join(p).is_file()
}

fn dead_count(dir: &Path, lines: &[String]) -> usize {
    lines.iter().filter(|l| !entry_exists(dir, l)).count()
}

const AUDIO: [&str; 8] = ["mp3", "flac", "m4a", "ogg", "opus", "wav", "wma", "aac"];

fn is_audio(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, e)| AUDIO.contains(&e.to_lowercase().as_str()))
}

/// `07` or `1-07` at the start of a file name, followed by a space.
fn track_key(name: &str) -> Option<&str> {
    let end = name.find(' ')?;
    let key = &name[..end];
    let ok = !key.is_empty()
        && key.split('-').count() <= 2
        && key.split('-').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    ok.then_some(key)
}

/// Dice similarity of the letter-and-digit bigrams of two titles, 0.0 to 1.0.
fn similarity(a: &str, b: &str) -> f64 {
    let grams = |s: &str| -> Vec<(char, char)> {
        let c: Vec<char> = s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
        c.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let (mut ga, gb) = (grams(a), grams(b));
    if ga.is_empty() || gb.is_empty() {
        return 0.0;
    }
    let total = (ga.len() + gb.len()) as f64;
    let mut shared = 0;
    for g in gb {
        if let Some(i) = ga.iter().position(|x| *x == g) {
            ga.swap_remove(i);
            shared += 1;
        }
    }
    2.0 * shared as f64 / total
}

/// Title part of a file name: without the track number and the extension.
fn title_of(name: &str) -> &str {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    track_key(stem).map_or(stem, |k| stem[k.len()..].trim_start())
}

const REVIEW_SIMILARITY: f64 = 0.7;

/// For a dead entry `Folder/File.ext`, look in its folder for a repair: `(true, to)` when the same name exists
/// with another extension (unambiguously), `(false, to)` when exactly one file with the same track number has a
/// similar title. `None` when there is nothing certain enough to offer.
fn find_repair(dir: &Path, entry: &str, listings: &mut HashMap<String, Vec<String>>) -> Option<(bool, String)> {
    let (folder, file) = entry.rsplit_once('/')?;
    let p = Path::new(folder);
    if p.is_absolute() || !p.components().all(|c| matches!(c, std::path::Component::Normal(_))) {
        return None;
    }
    let names = listings.entry(folder.to_string()).or_insert_with(|| {
        std::fs::read_dir(dir.join(p))
            .map(|rd| rd.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| is_audio(n)).collect())
            .unwrap_or_default()
    });
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let same_stem: Vec<&String> =
        names.iter().filter(|n| n.as_str() != file && n.rsplit_once('.').is_some_and(|(s, _)| s == stem)).collect();
    if same_stem.len() == 1 {
        return Some((true, format!("{folder}/{}", same_stem[0])));
    }
    if !same_stem.is_empty() {
        return None;
    }
    let key = track_key(file)?;
    let wanted = title_of(file);
    let close: Vec<&String> = names
        .iter()
        .filter(|n| n.as_str() != file && track_key(n) == Some(key) && similarity(title_of(n), wanted) >= REVIEW_SIMILARITY)
        .collect();
    (close.len() == 1).then(|| (false, format!("{folder}/{}", close[0])))
}

pub struct Store {
    dir: PathBuf,
    cache: RwLock<HashMap<String, Cached>>,
    write: Mutex<()>,
}

/// A playlist id is its file name without `.m3u`; it must never be able to point outside the folder.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() < 200 && !id.starts_with('.') && !id.contains(['/', '\\', '\0'])
}

/// A name for a new playlist: usable on the NAS and over SMB from Windows (no reserved characters, no leading dot,
/// no trailing dot or space), and not a path.
fn valid_new_name(name: &str) -> bool {
    valid_id(name)
        && name.len() <= 100
        && name == name.trim()
        && !name.ends_with('.')
        && !name.chars().any(|c| c.is_control() || ":*?\"<>|".contains(c))
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into(), cache: RwLock::new(HashMap::new()), write: Mutex::new(()) }
    }

    fn path_of(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.m3u"))
    }

    /// `(id, path, stamp)` for every visible `.m3u` file directly in the folder.
    async fn list(&self) -> Result<Vec<(String, PathBuf, (SystemTime, u64))>> {
        let mut rd = tokio::fs::read_dir(&self.dir)
            .await
            .with_context(|| format!("cannot read the playlist folder {}", self.dir.display()))?;
        let mut out = Vec::new();
        while let Some(e) = rd.next_entry().await? {
            let name = e.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with('.') || !name.to_lowercase().ends_with(".m3u") {
                continue;
            }
            let Ok(meta) = tokio::fs::metadata(e.path()).await else { continue };
            if !meta.is_file() {
                continue;
            }
            let id = name[..name.len() - 4].to_string();
            out.push((id, e.path(), (meta.modified().unwrap_or(UNIX_EPOCH), meta.len())));
        }
        Ok(out)
    }

    /// Every playlist, name-sorted (case-insensitive), flagged by whether it contains `entry`.
    pub async fn view(&self, entry: Option<&str>) -> Result<Vec<PlaylistView>> {
        let files = self.list().await?;
        let mut cache = self.cache.write().await;
        cache.retain(|id, _| files.iter().any(|f| &f.0 == id));
        let mut out = Vec::with_capacity(files.len());
        for (id, path, stamp) in files {
            if !cache.get(&id).is_some_and(|c| c.stamp == stamp) {
                match tokio::fs::read(&path).await {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        cache.insert(id.clone(), Cached::parse(stamp, &text));
                    }
                    Err(e) => {
                        tracing::warn!("cannot read {}: {e}", path.display());
                        continue;
                    }
                }
            }
            if cache[&id].dead.is_none() {
                let (dir, lines) = (self.dir.clone(), cache[&id].lines.clone());
                let dead = tokio::task::spawn_blocking(move || dead_count(&dir, &lines)).await.unwrap_or(0);
                cache.get_mut(&id).expect("just inserted").dead = Some(dead);
            }
            let c = &cache[&id];
            let member = entry.is_some_and(|t| c.entries.contains(t));
            let dead = c.dead.unwrap_or(0);
            out.push(PlaylistView { name: id.clone(), member, count: c.lines.len().saturating_sub(dead), dead, id });
        }
        out.sort_by_key(|p| p.name.to_lowercase());
        Ok(out)
    }

    /// Re-check every playlist's dead entries against the library (files appear and disappear without the
    /// playlist changing). Meant to run every minute or so in the background.
    pub async fn refresh_dead(&self) {
        let snapshot: Vec<(String, (SystemTime, u64), Vec<String>)> =
            self.cache.read().await.iter().map(|(id, c)| (id.clone(), c.stamp, c.lines.clone())).collect();
        let dir = self.dir.clone();
        let counted = tokio::task::spawn_blocking(move || {
            snapshot.into_iter().map(|(id, stamp, lines)| (id, stamp, dead_count(&dir, &lines))).collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        let mut cache = self.cache.write().await;
        for (id, stamp, dead) in counted {
            if let Some(c) = cache.get_mut(&id).filter(|c| c.stamp == stamp) {
                c.dead = Some(dead);
            }
        }
    }

    /// Open a playlist for editing. The caller must hold the write lock.
    async fn load_for_edit(&self, id: &str) -> Result<(PathBuf, std::fs::Permissions, String), StoreError> {
        if !valid_id(id) {
            return Err(StoreError::Unknown);
        }
        let path = self.path_of(id);
        let meta = match tokio::fs::metadata(&path).await {
            Ok(m) if m.is_file() => m,
            _ => return Err(StoreError::Unknown),
        };
        let bytes = tokio::fs::read(&path).await.with_context(|| format!("reading {id}.m3u"))?;
        let text = String::from_utf8(bytes).map_err(|_| anyhow!("{id}.m3u is not valid UTF-8, refusing to edit it"))?;
        Ok((path, meta.permissions(), text))
    }

    /// Replace the playlist with `new_text`: back the old file up, write a temporary file next to it and rename it
    /// into place. The caller must hold the write lock.
    async fn commit(&self, id: &str, path: &Path, perms: std::fs::Permissions, new_text: &str) -> Result<()> {
        self.backup(id, path).await?;
        let tmp = self.dir.join(format!(".{id}.m3u.cratarr-tmp"));
        tokio::fs::write(&tmp, new_text).await.with_context(|| format!("writing {}", tmp.display()))?;
        tokio::fs::set_permissions(&tmp, perms).await.ok();
        if let Err(e) = tokio::fs::rename(&tmp, path).await {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(anyhow!(e).context(format!("replacing {id}.m3u")));
        }
        if let Ok(m) = tokio::fs::metadata(path).await {
            self.cache
                .write()
                .await
                .insert(id.to_string(), Cached::parse((m.modified().unwrap_or(UNIX_EPOCH), m.len()), new_text));
        }
        Ok(())
    }

    /// Add `entry` to the playlist, or remove it if it is already there.
    pub async fn toggle(&self, id: &str, entry: &str) -> Result<Toggled, StoreError> {
        let _one_writer = self.write.lock().await;
        let (path, perms, text) = self.load_for_edit(id).await?;
        let (new_text, added) = if m3u::contains(&text, entry) {
            (m3u::with_removed(&text, entry).0, false)
        } else {
            (m3u::with_added(&text, entry), true)
        };
        self.commit(id, &path, perms, &new_text).await?;
        Ok(Toggled { added })
    }

    /// Dead entries that can be repaired, across all playlists: `safe` (same file name, other extension: the
    /// library holds the track in another format) and `review` (same track number, similar but different title).
    pub async fn repairs(&self) -> Result<Repairs> {
        self.view(None).await?;
        let snapshot: Vec<(String, Vec<String>)> =
            self.cache.read().await.iter().map(|(id, c)| (id.clone(), c.lines.clone())).collect();
        let dir = self.dir.clone();
        let mut found = tokio::task::spawn_blocking(move || {
            let mut listings: HashMap<String, Vec<String>> = HashMap::new();
            let mut out = Repairs::default();
            for (playlist, lines) in snapshot {
                let mut seen = HashSet::new();
                for from in lines.iter().filter(|l| !entry_exists(&dir, l) && seen.insert(l.as_str())) {
                    if let Some((safe, to)) = find_repair(&dir, from, &mut listings) {
                        let r = Repair { playlist: playlist.clone(), from: from.clone(), to };
                        if safe {
                            out.safe.push(r);
                        } else {
                            out.review.push(r);
                        }
                    }
                }
            }
            out
        })
        .await?;
        let key = |r: &Repair| (r.playlist.to_lowercase(), r.from.clone());
        found.safe.sort_by_key(key);
        found.review.sort_by_key(key);
        Ok(found)
    }

    /// Apply repairs. Every requested pair must be one that is currently offered (the client is never trusted
    /// with paths). One backup and one write per playlist. Returns the playlists that changed.
    pub async fn apply_repairs(&self, wanted: &[Repair]) -> Result<Vec<String>, StoreError> {
        let offered = self.repairs().await?;
        let is_offered = |w: &Repair| offered.safe.iter().chain(&offered.review).any(|o| o == w);
        if wanted.iter().any(|w| !is_offered(w)) {
            return Err(StoreError::NotOffered);
        }
        let mut by_playlist: Vec<(String, Vec<&Repair>)> = Vec::new();
        for w in wanted {
            match by_playlist.iter_mut().find(|(p, _)| *p == w.playlist) {
                Some((_, v)) => v.push(w),
                None => by_playlist.push((w.playlist.clone(), vec![w])),
            }
        }
        let _one_writer = self.write.lock().await;
        let mut changed = Vec::new();
        for (id, repairs) in by_playlist {
            let (path, perms, mut text) = self.load_for_edit(&id).await?;
            let mut total = 0;
            for r in repairs {
                let (t, n) = m3u::with_replaced(&text, &r.from, &r.to);
                text = t;
                total += n;
            }
            if total > 0 {
                self.commit(&id, &path, perms, &text).await?;
                changed.push(id);
            }
        }
        Ok(changed)
    }

    /// Put the playlist back as it was before its last change (the newest backup). The current file is backed up
    /// first, so an undo can itself be undone.
    pub async fn undo(&self, id: &str) -> Result<(), StoreError> {
        let _one_writer = self.write.lock().await;
        let (path, perms, _) = self.load_for_edit(id).await?;
        let Some(backup) = self.newest_backup(id).await else {
            return Err(StoreError::NothingToUndo);
        };
        let bytes = tokio::fs::read(&backup).await.with_context(|| format!("reading the backup of {id}"))?;
        let text = String::from_utf8(bytes).map_err(|_| anyhow!("the backup of {id} is not valid UTF-8"))?;
        self.commit(id, &path, perms, &text).await?;
        Ok(())
    }

    /// Playlists that list the same entry more than once.
    /// Playlist lines that point into the album folder `folder`, per playlist (playlists without any are left out).
    pub async fn lines_in_folder(&self, folder: &str) -> Result<Vec<(String, usize)>> {
        self.view(None).await?;
        let prefix = format!("{folder}/");
        let cache = self.cache.read().await;
        let mut out: Vec<(String, usize)> = cache
            .iter()
            .filter_map(|(id, c)| {
                let n = c.lines.iter().filter(|l| l.starts_with(&prefix)).count();
                (n > 0).then(|| (id.clone(), n))
            })
            .collect();
        out.sort_by_key(|(id, _)| id.to_lowercase());
        Ok(out)
    }

    /// Playlists related through shared tracks, most similar first (for Insights).
    #[allow(dead_code)] // wired up with the Insights page
    pub async fn related(&self, min_shared: usize) -> Result<Vec<Related>> {
        self.view(None).await?;
        let cache = self.cache.read().await;
        let sets: Vec<(&str, &HashSet<String>)> = cache.iter().map(|(id, c)| (id.as_str(), &c.entries)).collect();
        Ok(relate(&sets, min_shared))
    }

    /// Figures across all playlists (for Insights).
    #[allow(dead_code)] // wired up with the Insights page
    pub async fn stats(&self) -> Result<PlaylistStats> {
        self.view(None).await?;
        let cache = self.cache.read().await;
        let all: Vec<(&str, &[String], usize)> =
            cache.iter().map(|(id, c)| (id.as_str(), c.lines.as_slice(), c.dead.unwrap_or(0))).collect();
        Ok(playlist_stats(&all))
    }

    pub async fn duplicates(&self) -> Result<Vec<DupGroup>> {
        self.view(None).await?;
        let cache = self.cache.read().await;
        let mut out = Vec::new();
        for (id, c) in cache.iter() {
            let mut counts: HashMap<&str, usize> = HashMap::new();
            for l in &c.lines {
                *counts.entry(l.as_str()).or_default() += 1;
            }
            let mut entries: Vec<DupEntry> = counts
                .into_iter()
                .filter(|(_, n)| *n > 1)
                .map(|(e, n)| DupEntry { entry: e.to_string(), copies: n })
                .collect();
            if entries.is_empty() {
                continue;
            }
            entries.sort_by(|a, b| a.entry.cmp(&b.entry));
            out.push(DupGroup { playlist: id.clone(), extra: entries.iter().map(|e| e.copies - 1).sum(), entries });
        }
        out.sort_by_key(|g| g.playlist.to_lowercase());
        Ok(out)
    }

    /// Remove repeated entries from a playlist, keeping the first copy of each. Returns how many lines went.
    pub async fn dedupe(&self, id: &str) -> Result<usize, StoreError> {
        let _one_writer = self.write.lock().await;
        let (path, perms, text) = self.load_for_edit(id).await?;
        let (new_text, removed) = m3u::with_deduped(&text);
        if removed > 0 {
            self.commit(id, &path, perms, &new_text).await?;
        }
        Ok(removed)
    }

    /// Create an empty playlist. Never overwrites an existing file.
    pub async fn create(&self, name: &str) -> Result<(), StoreError> {
        if !valid_new_name(name) {
            return Err(StoreError::BadName);
        }
        let _one_writer = self.write.lock().await;
        let path = self.path_of(name);
        // `create_new` fails if the file exists (also on a case-insensitive clash), so nothing is overwritten.
        match tokio::fs::OpenOptions::new().write(true).create_new(true).open(&path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(StoreError::Exists),
            Err(e) => Err(anyhow!(e).context(format!("creating {name}.m3u")).into()),
        }
    }

    /// Rename a playlist. The file keeps its content; nothing is overwritten.
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), StoreError> {
        if !valid_id(from) {
            return Err(StoreError::Unknown);
        }
        if !valid_new_name(to) {
            return Err(StoreError::BadName);
        }
        let _one_writer = self.write.lock().await;
        let (src, dst) = (self.path_of(from), self.path_of(to));
        if !tokio::fs::metadata(&src).await.is_ok_and(|m| m.is_file()) {
            return Err(StoreError::Unknown);
        }
        if tokio::fs::try_exists(&dst).await.unwrap_or(false) {
            return Err(StoreError::Exists);
        }
        self.backup(from, &src).await?;
        tokio::fs::rename(&src, &dst).await.with_context(|| format!("renaming {from}.m3u"))?;
        let mut cache = self.cache.write().await;
        cache.remove(from);
        Ok(())
    }

    /// Delete a playlist. The file is moved into the backup folder, never destroyed, so `restore_deleted` can bring it back.
    pub async fn delete(&self, id: &str) -> Result<(), StoreError> {
        let _one_writer = self.write.lock().await;
        let (path, _, _) = self.load_for_edit(id).await?;
        self.backup(id, &path).await?;
        tokio::fs::remove_file(&path).await.with_context(|| format!("deleting {id}.m3u"))?;
        self.cache.write().await.remove(id);
        Ok(())
    }

    /// Bring back a deleted playlist from its newest backup (refuses if a playlist of that name exists again).
    pub async fn restore_deleted(&self, id: &str) -> Result<(), StoreError> {
        if !valid_id(id) {
            return Err(StoreError::Unknown);
        }
        let _one_writer = self.write.lock().await;
        let path = self.path_of(id);
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Err(StoreError::Exists);
        }
        let Some(backup) = self.newest_backup(id).await else { return Err(StoreError::NothingToUndo) };
        let bytes = tokio::fs::read(&backup).await.with_context(|| format!("reading the backup of {id}"))?;
        tokio::fs::write(&path, bytes).await.with_context(|| format!("restoring {id}.m3u"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o664)).await;
        }
        Ok(())
    }

    /// The newest backup file of `id`, if any.
    async fn newest_backup(&self, id: &str) -> Option<PathBuf> {
        let prefix = format!("{id}.");
        let mut newest: Option<(u128, PathBuf)> = None;
        let mut rd = tokio::fs::read_dir(self.dir.join(BACKUP_DIR)).await.ok()?;
        while let Ok(Some(e)) = rd.next_entry().await {
            let name = e.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(mid) = name.strip_prefix(&prefix).and_then(|r| r.strip_suffix(".bak")) else { continue };
            if let Ok(n) = mid.parse::<u128>() {
                if newest.as_ref().is_none_or(|(m, _)| n > *m) {
                    newest = Some((n, e.path()));
                }
            }
        }
        newest.map(|(_, p)| p)
    }

    /// Copy the current file to the backup folder (named `<id>.<millis>.bak`) and keep only the newest few.
    async fn backup(&self, id: &str, path: &Path) -> Result<()> {
        let dir = self.dir.join(BACKUP_DIR);
        tokio::fs::create_dir_all(&dir).await.with_context(|| format!("creating {}", dir.display()))?;
        // Shared media folders are group-writable (and setgid); a container umask of 022 would otherwise lock the
        // group out of the backup folder.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = tokio::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o2775)).await;
        }
        let mut stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let target = loop {
            let t = dir.join(format!("{id}.{stamp}.bak"));
            if !tokio::fs::try_exists(&t).await.unwrap_or(false) {
                break t;
            }
            stamp += 1;
        };
        tokio::fs::copy(path, &target).await.with_context(|| format!("backing up {id}.m3u"))?;
        let prefix = format!("{id}.");
        let mut mine: Vec<(u128, PathBuf)> = Vec::new();
        let mut rd = tokio::fs::read_dir(&dir).await?;
        while let Some(e) = rd.next_entry().await? {
            let name = e.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(mid) = name.strip_prefix(&prefix).and_then(|r| r.strip_suffix(".bak")) else { continue };
            if let Ok(n) = mid.parse::<u128>() {
                mine.push((n, e.path()));
            }
        }
        mine.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
        for (_, old) in mine.into_iter().skip(KEEP_BACKUPS) {
            let _ = tokio::fs::remove_file(old).await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn set(v: &[&str]) -> HashSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn related_playlists_are_ranked_by_overlap_not_size() {
        let chill = set(&["a", "b", "c", "d"]);
        let lounge = set(&["a", "b", "c", "x"]);
        let everything = set(&["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"]);
        let other = set(&["z"]);
        let r = relate(&[("Chill", &chill), ("Lounge", &lounge), ("Everything", &everything), ("Other", &other)], 1);
        assert_eq!((r[0].a.as_str(), r[0].b.as_str(), r[0].shared), ("Chill", "Lounge", 3), "3 of 5 beats 4 of 12");
        assert!((r[0].similarity - 0.6).abs() < 1e-9);
        assert!(r.iter().all(|x| x.a != "Other" && x.b != "Other"), "no shared tracks: not related");
        assert_eq!(relate(&[("Chill", &chill), ("Lounge", &lounge)], 4).len(), 0, "below the minimum");
    }

    #[test]
    fn stats_count_distinct_tracks_and_repeats_once_per_playlist() {
        let a = vec!["x".to_string(), "y".to_string(), "x".to_string()];
        let b = vec!["x".to_string(), "z".to_string()];
        let s = playlist_stats(&[("A", a.as_slice(), 1), ("B", b.as_slice(), 0)]);
        assert_eq!((s.playlists, s.lines, s.distinct_tracks, s.in_one_playlist, s.missing), (2, 5, 3, 2, 1));
        assert_eq!(s.most_shared, vec![("x".to_string(), 2)], "a repeat within one playlist counts once");
    }

    static N: AtomicUsize = AtomicUsize::new(0);

    struct Dir(PathBuf);

    impl Dir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("cratarr-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn write(&self, name: &str, content: impl AsRef<[u8]>) {
            std::fs::write(self.0.join(name), content).unwrap();
        }
        fn read(&self, name: &str) -> String {
            std::fs::read_to_string(self.0.join(name)).unwrap()
        }
        /// Create an (empty) library file such as `A/1.mp3`.
        fn touch(&self, rel: &str) {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"").unwrap();
        }
        fn store(&self) -> Store {
            Store::new(&self.0)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn view_lists_visible_m3u_files_sorted_with_membership() {
        let d = Dir::new();
        d.touch("A/1.mp3");
        d.touch("B/2.mp3");
        d.write("chill.m3u", "A/1.mp3\nB/2.mp3\n");
        d.write("Analogic.m3u", "B/2.mp3\n");
        d.write("notes.txt", "A/1.mp3\n");
        d.write(".hidden.m3u", "A/1.mp3\n");
        std::fs::create_dir(d.0.join("Some Album (2020)")).unwrap();
        let v = d.store().view(Some("A/1.mp3")).await.unwrap();
        assert_eq!(
            v,
            vec![
                PlaylistView { id: "Analogic".into(), name: "Analogic".into(), member: false, count: 1, dead: 0 },
                PlaylistView { id: "chill".into(), name: "chill".into(), member: true, count: 2, dead: 0 },
            ]
        );
    }

    #[tokio::test]
    async fn entries_without_a_file_are_dead_and_not_counted() {
        let d = Dir::new();
        d.touch("A/1.mp3");
        d.write("a.m3u", "A/1.mp3\nGone/2.mp3\nA/3.flac\n");
        let v = d.store().view(None).await.unwrap();
        assert_eq!((v[0].count, v[0].dead), (1, 2));
    }

    #[tokio::test]
    async fn absolute_and_parent_paths_never_resolve() {
        let d = Dir::new();
        d.touch("A/1.mp3");
        d.write("a.m3u", "../A/1.mp3\n/etc/hostname\nA/../A/1.mp3\nA/1.mp3\n");
        let v = d.store().view(None).await.unwrap();
        assert_eq!((v[0].count, v[0].dead), (1, 3));
    }

    #[tokio::test]
    async fn the_background_check_notices_files_appearing() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\nB/2.mp3\n");
        let s = d.store();
        assert_eq!(s.view(None).await.unwrap()[0].dead, 2);
        d.touch("A/1.mp3");
        assert_eq!(s.view(None).await.unwrap()[0].dead, 2, "cached until the next background check");
        s.refresh_dead().await;
        let v = s.view(None).await.unwrap();
        assert_eq!((v[0].count, v[0].dead), (1, 1));
    }

    #[tokio::test]
    async fn adding_a_track_recounts_dead_entries() {
        let d = Dir::new();
        d.touch("B/2.mp3");
        d.write("a.m3u", "Gone/1.mp3\n");
        let s = d.store();
        assert_eq!((s.view(None).await.unwrap()[0].count, s.view(None).await.unwrap()[0].dead), (0, 1));
        s.toggle("a", "B/2.mp3").await.unwrap();
        let v = s.view(None).await.unwrap();
        assert_eq!((v[0].count, v[0].dead), (1, 1));
    }

    #[tokio::test]
    async fn no_track_highlights_nothing() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\n");
        assert!(d.store().view(None).await.unwrap().iter().all(|p| !p.member));
    }

    #[tokio::test]
    async fn toggle_adds_then_removes_and_the_view_follows() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\n");
        let s = d.store();
        assert_eq!(s.toggle("a", "B/2.mp3").await.unwrap(), Toggled { added: true });
        assert_eq!(d.read("a.m3u"), "A/1.mp3\nB/2.mp3\n");
        assert!(s.view(Some("B/2.mp3")).await.unwrap()[0].member);
        assert_eq!(s.toggle("a", "B/2.mp3").await.unwrap(), Toggled { added: false });
        assert_eq!(d.read("a.m3u"), "A/1.mp3\n");
        assert!(!s.view(Some("B/2.mp3")).await.unwrap()[0].member);
    }

    #[tokio::test]
    async fn the_count_follows_edits_and_ignores_comments_and_blanks() {
        let d = Dir::new();
        for f in ["A/1.mp3", "B/2.mp3", "C/3.mp3"] {
            d.touch(f);
        }
        d.write("a.m3u", "#EXTM3U\n\nA/1.mp3\nA/1.mp3\n# note\nB/2.mp3\n");
        let s = d.store();
        assert_eq!(s.view(None).await.unwrap()[0].count, 3, "duplicate lines count, comments and blanks do not");
        s.toggle("a", "C/3.mp3").await.unwrap();
        assert_eq!(s.view(None).await.unwrap()[0].count, 4);
        s.toggle("a", "A/1.mp3").await.unwrap();
        assert_eq!(s.view(None).await.unwrap()[0].count, 2, "removing drops every copy");
    }

    #[tokio::test]
    async fn crlf_files_stay_crlf() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\r\n");
        d.store().toggle("a", "B/2.mp3").await.unwrap();
        assert_eq!(d.read("a.m3u"), "A/1.mp3\r\nB/2.mp3\r\n");
    }

    #[tokio::test]
    async fn every_write_leaves_a_backup_of_the_previous_content() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\n");
        d.store().toggle("a", "B/2.mp3").await.unwrap();
        let backups: Vec<_> = std::fs::read_dir(d.0.join(BACKUP_DIR)).unwrap().collect();
        assert_eq!(backups.len(), 1);
        let b = std::fs::read_to_string(backups[0].as_ref().unwrap().path()).unwrap();
        assert_eq!(b, "A/1.mp3\n");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_backup_folder_is_group_writable() {
        use std::os::unix::fs::PermissionsExt;
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\n");
        d.store().toggle("a", "B/2.mp3").await.unwrap();
        let mode = std::fs::metadata(d.0.join(BACKUP_DIR)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o775);
    }

    #[tokio::test]
    async fn old_backups_are_pruned() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\n");
        let s = d.store();
        for _ in 0..(KEEP_BACKUPS + 6) {
            s.toggle("a", "B/2.mp3").await.unwrap();
        }
        assert_eq!(std::fs::read_dir(d.0.join(BACKUP_DIR)).unwrap().count(), KEEP_BACKUPS);
    }

    #[tokio::test]
    async fn no_temporary_file_is_left_behind() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\n");
        d.store().toggle("a", "B/2.mp3").await.unwrap();
        let names: Vec<String> = std::fs::read_dir(&d.0).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().all(|n| !n.contains("cratarr-tmp")), "{names:?}");
    }

    #[tokio::test]
    async fn bad_or_unknown_ids_are_refused() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\n");
        let s = d.store();
        for id in ["", "../a", "a/b", ".hidden", "nope", "a\\b"] {
            assert!(matches!(s.toggle(id, "X/1.mp3").await, Err(StoreError::Unknown)), "id {id:?}");
        }
        assert_eq!(d.read("a.m3u"), "A/1.mp3\n");
    }

    #[tokio::test]
    async fn outside_edits_are_noticed() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\n");
        let s = d.store();
        assert!(s.view(Some("B/2.mp3")).await.unwrap().iter().all(|p| !p.member));
        d.write("a.m3u", "A/1.mp3\nB/2.mp3\n");
        assert!(s.view(Some("B/2.mp3")).await.unwrap()[0].member);
    }

    #[tokio::test]
    async fn an_edit_always_starts_from_the_files_current_content() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\n");
        let s = d.store();
        s.view(None).await.unwrap();
        d.write("a.m3u", "A/1.mp3\nC/3.mp3\n"); // another tool edits the file after our view
        s.toggle("a", "B/2.mp3").await.unwrap();
        assert_eq!(d.read("a.m3u"), "A/1.mp3\nC/3.mp3\nB/2.mp3\n");
    }

    #[tokio::test]
    async fn files_that_are_not_utf8_are_never_rewritten() {
        let d = Dir::new();
        d.write("a.m3u", [0x41u8, 0x2f, 0xff, 0xfe, b'\n']);
        let err = d.store().toggle("a", "B/2.mp3").await.unwrap_err();
        assert!(err.to_string().contains("UTF-8"));
        assert_eq!(std::fs::read(d.0.join("a.m3u")).unwrap(), vec![0x41u8, 0x2f, 0xff, 0xfe, b'\n']);
    }

    #[test]
    fn track_keys_and_titles_are_read_from_file_names() {
        assert_eq!(track_key("07 Artist - Song.mp3"), Some("07"));
        assert_eq!(track_key("1-07 Artist - Song.mp3"), Some("1-07"));
        assert_eq!(track_key("Artist - Song.mp3"), None);
        assert_eq!(track_key("1-2-3 x.mp3"), None);
        assert_eq!(title_of("07 Artist - Song.mp3"), "Artist - Song");
    }

    #[test]
    fn similarity_tolerates_garbled_characters_but_not_other_songs() {
        assert!(similarity("Orizzonti Bagnati DellâAdriatico", "Orizzonti bagnati dell’Adriatico") >= REVIEW_SIMILARITY);
        assert!(similarity("Meltphace 6", "Strotha Tynhe") < REVIEW_SIMILARITY);
        assert_eq!(similarity("", "abc"), 0.0);
    }

    #[tokio::test]
    async fn the_same_name_with_another_extension_is_a_safe_repair() {
        let d = Dir::new();
        d.touch("Alb (2020)/01 A - One.flac");
        d.touch("Alb (2020)/02 A - Two.mp3");
        d.write("p.m3u", "Alb (2020)/01 A - One.mp3\nAlb (2020)/02 A - Two.mp3\n");
        let r = d.store().repairs().await.unwrap();
        assert_eq!(
            r.safe,
            vec![Repair { playlist: "p".into(), from: "Alb (2020)/01 A - One.mp3".into(), to: "Alb (2020)/01 A - One.flac".into() }]
        );
        assert!(r.review.is_empty());
    }

    #[tokio::test]
    async fn a_similar_title_on_the_same_track_number_needs_review() {
        let d = Dir::new();
        d.touch("Alb (2020)/01 A - Orizzonti bagnati dell’Adriatico.mp3");
        d.write("p.m3u", "Alb (2020)/01 A - Orizzonti Bagnati DellâAdriatico.mp3\n");
        let r = d.store().repairs().await.unwrap();
        assert!(r.safe.is_empty());
        assert_eq!(r.review.len(), 1);
        assert_eq!(r.review[0].to, "Alb (2020)/01 A - Orizzonti bagnati dell’Adriatico.mp3");
    }

    #[tokio::test]
    async fn dissimilar_ambiguous_and_absent_files_get_no_offer() {
        let d = Dir::new();
        d.touch("Alb (2020)/05 A - Strotha Tynhe.mp3"); // same number, unrelated title
        d.touch("Two (2020)/01 A - Same.flac"); // two candidates for one stem: ambiguous
        d.touch("Two (2020)/01 A - Same.m4a");
        d.write("p.m3u", "Alb (2020)/05 A - Meltphace 6.mp3\nTwo (2020)/01 A - Same.mp3\nNone (2020)/01 A - Gone.mp3\n");
        let r = d.store().repairs().await.unwrap();
        assert!(r.safe.is_empty() && r.review.is_empty(), "{r:?}");
    }

    #[tokio::test]
    async fn applying_a_repair_rewrites_only_that_line_and_keeps_a_backup() {
        let d = Dir::new();
        d.touch("Alb (2020)/01 A - One.flac");
        d.touch("Alb (2020)/02 A - Two.mp3");
        d.write("p.m3u", "#c\nAlb (2020)/01 A - One.mp3\r\nAlb (2020)/02 A - Two.mp3\r\n");
        let s = d.store();
        let r = s.repairs().await.unwrap();
        let changed = s.apply_repairs(&r.safe).await.unwrap();
        assert_eq!(changed, vec!["p"]);
        assert_eq!(d.read("p.m3u"), "#c\nAlb (2020)/01 A - One.flac\r\nAlb (2020)/02 A - Two.mp3\r\n");
        let v = s.view(None).await.unwrap();
        assert_eq!((v[0].count, v[0].dead), (2, 0));
        assert_eq!(std::fs::read_dir(d.0.join(BACKUP_DIR)).unwrap().count(), 1);
        assert!(s.repairs().await.unwrap().safe.is_empty());
    }

    #[tokio::test]
    async fn a_repair_that_is_not_currently_offered_is_refused() {
        let d = Dir::new();
        d.touch("Alb (2020)/01 A - One.flac");
        d.write("p.m3u", "Alb (2020)/01 A - One.mp3\n");
        let bogus = Repair { playlist: "p".into(), from: "Alb (2020)/01 A - One.mp3".into(), to: "../../etc/passwd".into() };
        assert!(d.store().apply_repairs(&[bogus]).await.is_err());
        assert_eq!(d.read("p.m3u"), "Alb (2020)/01 A - One.mp3\n");
    }

    #[tokio::test]
    async fn undo_restores_the_previous_version_and_can_itself_be_undone() {
        let d = Dir::new();
        d.touch("Alb (2020)/01 A - One.flac");
        d.write("p.m3u", "Alb (2020)/01 A - One.mp3\n");
        let s = d.store();
        let r = s.repairs().await.unwrap();
        s.apply_repairs(&r.safe).await.unwrap();
        assert_eq!(d.read("p.m3u"), "Alb (2020)/01 A - One.flac\n");
        s.undo("p").await.unwrap();
        assert_eq!(d.read("p.m3u"), "Alb (2020)/01 A - One.mp3\n");
        s.undo("p").await.unwrap();
        assert_eq!(d.read("p.m3u"), "Alb (2020)/01 A - One.flac\n", "undoing the undo");
    }

    #[tokio::test]
    async fn undo_without_a_backup_or_for_an_unknown_playlist_is_an_error() {
        let d = Dir::new();
        d.write("p.m3u", "A/1.mp3\n");
        let s = d.store();
        assert!(s.undo("p").await.is_err());
        assert!(matches!(s.undo("nope").await, Err(StoreError::Unknown)));
        assert!(matches!(s.undo("../p").await, Err(StoreError::Unknown)));
    }

    #[tokio::test]
    async fn duplicates_are_found_and_removed_keeping_the_first_copy() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\nB/2.mp3\nA/1.mp3\nA/1.mp3\n");
        d.write("clean.m3u", "A/1.mp3\n");
        let s = d.store();
        let dups = s.duplicates().await.unwrap();
        assert_eq!(dups.len(), 1);
        assert_eq!((dups[0].playlist.as_str(), dups[0].extra), ("a", 2));
        assert_eq!(dups[0].entries, vec![DupEntry { entry: "A/1.mp3".into(), copies: 3 }]);
        assert_eq!(s.dedupe("a").await.unwrap(), 2);
        assert_eq!(d.read("a.m3u"), "A/1.mp3\nB/2.mp3\n");
        assert!(s.duplicates().await.unwrap().is_empty());
        assert_eq!(s.dedupe("clean").await.unwrap(), 0);
        assert_eq!(std::fs::read_dir(d.0.join(BACKUP_DIR)).unwrap().count(), 1, "only the changed playlist is backed up");
    }

    #[tokio::test]
    async fn a_new_playlist_is_created_empty_and_never_overwrites() {
        let d = Dir::new();
        d.write("old.m3u", "A/1.mp3\n");
        let s = d.store();
        s.create("Late Night").await.unwrap();
        assert_eq!(d.read("Late Night.m3u"), "");
        assert!(s.view(None).await.unwrap().iter().any(|p| p.id == "Late Night"));
        assert!(matches!(s.create("old").await, Err(StoreError::Exists)));
        assert_eq!(d.read("old.m3u"), "A/1.mp3\n");
    }

    #[tokio::test]
    async fn unusable_playlist_names_are_refused() {
        let d = Dir::new();
        let s = d.store();
        for name in ["", " ", "a/b", "a\\b", ".hidden", "ends.", "ends ", " starts", "co:lon", "star*", "q?", "quo\"te", "a<b", "p|pe", "nul\0"] {
            assert!(matches!(s.create(name).await, Err(StoreError::BadName)), "{name:?}");
        }
        assert!(matches!(s.create(&"x".repeat(101)).await, Err(StoreError::BadName)));
        s.create("Ünïcode & spaces (2024)").await.unwrap();
    }

    #[tokio::test]
    async fn renaming_moves_the_file_and_refuses_clashes() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\n");
        d.write("b.m3u", "B/2.mp3\n");
        let s = d.store();
        s.rename("a", "c").await.unwrap();
        assert_eq!(d.read("c.m3u"), "A/1.mp3\n");
        assert!(!d.0.join("a.m3u").exists());
        assert!(matches!(s.rename("c", "b").await, Err(StoreError::Exists)));
        assert!(matches!(s.rename("c", "bad/name").await, Err(StoreError::BadName)));
        assert!(matches!(s.rename("nope", "z").await, Err(StoreError::Unknown)));
        assert!(matches!(s.rename("../c", "z").await, Err(StoreError::Unknown)));
        let names: Vec<String> = s.view(None).await.unwrap().into_iter().map(|p| p.id).collect();
        assert_eq!(names, vec!["b", "c"]);
    }

    #[tokio::test]
    async fn deleting_keeps_a_recoverable_copy() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\n");
        let s = d.store();
        s.delete("a").await.unwrap();
        assert!(!d.0.join("a.m3u").exists());
        assert!(s.view(None).await.unwrap().is_empty());
        s.restore_deleted("a").await.unwrap();
        assert_eq!(d.read("a.m3u"), "A/1.mp3\n");
        assert!(matches!(s.restore_deleted("a").await, Err(StoreError::Exists)));
        assert!(matches!(s.delete("nope").await, Err(StoreError::Unknown)));
        assert!(matches!(s.restore_deleted("never-existed").await, Err(StoreError::NothingToUndo)));
    }

    #[tokio::test]
    async fn a_missing_folder_is_an_error_not_an_empty_list() {
        let s = Store::new("/definitely/not/here");
        assert!(s.view(None).await.is_err());
    }
}
