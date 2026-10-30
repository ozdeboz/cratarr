//! The playlists: `.m3u` files in one folder. Reads are cheap (small text files, cached by modification stamp),
//! writes are careful: re-read right before the edit, back the file up, write a temporary file next to it and
//! rename it into place, so a crash or a concurrent edit can never leave a half-written playlist.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

use crate::journal::fingerprint;
use crate::m3u;

pub const BACKUP_DIR: &str = ".cratarr-backups";
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
    /// Where it is kept in step (decision 0014); filled in by the playlist service.
    pub links: Vec<crate::sync_state::LinkView>,
    /// Other names it is spoken as (decision 0015); filled in by the playlist service.
    pub spoken: Vec<String>,
    /// Whose it is, shared or not, and whether it is the asking account's own to change (decision 0051); filled in by
    /// the playlist service.
    pub owner: String,
    pub shared: bool,
    pub mine: bool,
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
    /// `prefix`: what the file's lines carry in front of a Music-relative entry ("../" in `.playlists`).
    fn parse(stamp: (SystemTime, u64), text: &str, prefix: &str) -> Self {
        let lines: Vec<String> = m3u::entries(text).map(|l| unprefixed(&l, prefix)).collect();
        Self { stamp, entries: lines.iter().cloned().collect(), lines, dead: None }
    }
}

/// True when `entry` (library-relative) names an existing file under `dir`. Absolute paths and `..` never resolve.
pub(crate) fn entry_exists(dir: &Path, entry: &str) -> bool {
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
pub(crate) fn similarity(a: &str, b: &str) -> f64 {
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

/// The playlist folder: where the playlists live (decisions 0012, 0014), inside Music. Its name starts with a dot so
/// the media server does not import the files (its copies are made by the sync links). Lines there carry "../" in
/// front of the Music-relative entry.
pub const PLAYLIST_DIR: &str = ".playlists";

fn unprefixed(line: &str, prefix: &str) -> String {
    line.strip_prefix(prefix).unwrap_or(line).to_string()
}

/// A playlist's file before and after a change: what undoing it needs (decision 0026).
#[derive(Debug, Clone, PartialEq)]
pub struct Edited {
    pub id: String,
    pub before: String,
    pub after: String,
}

pub struct Store {
    /// Where the `.m3u` files are.
    dir: PathBuf,
    /// The Music folder: entries are relative to it.
    base: PathBuf,
    /// What lines carry in front of an entry: "" when the files are in Music itself, "../" in `.playlists`, "../../" in
    /// an account's folder there.
    prefix: String,
    /// Whose playlists (decision 0051): another account's, in its folder; `None`: the first administrator's.
    account: Option<String>,
    cache: RwLock<HashMap<String, Cached>>,
    write: Mutex<()>,
}

/// Whether an account's name can name its playlists' folder: plain, as the directory's names are.
pub fn account_folder_ok(account: &str) -> bool {
    !account.is_empty() && account.len() <= 64 && !account.starts_with('.') && account.chars().all(|c| c.is_ascii_alphanumeric() || "._-@".contains(c))
}

/// Backup copies kept next to the playlists ("Love.bak.m3u") are not playlists: they are left alone, unlisted.
fn is_backup_copy(id: &str) -> bool {
    id.to_lowercase().ends_with(".bak")
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
        && !is_backup_copy(name)
        && !name.chars().any(|c| c.is_control() || ":*?\"<>|".contains(c))
}

/// The nearest name `valid_new_name` takes, for a playlist named elsewhere (a server's): reserved characters become
/// "-", leading dots and spaces and trailing dots and spaces go, long names are cut. "Playlist" when nothing is left.
/// Told every playlist write: its name, and its entries before and after. Set once at start (the user data keeps when
/// each entry came, for the panel's Added order).
pub type OnWrite = Box<dyn Fn(&str, &[String], &[String]) + Send + Sync>;
static ON_WRITE: std::sync::OnceLock<OnWrite> = std::sync::OnceLock::new();

pub fn on_write(told: OnWrite) {
    let _ = ON_WRITE.set(told);
}

/// Told when a playlist comes (`true`: made, renamed to, brought back) or goes (deleted, renamed away), by its key: the
/// change log (decision 0087). Its writes are told `on_write`. Set once at start.
pub type OnNamed = Box<dyn Fn(&str, bool) + Send + Sync>;
static ON_NAMED: std::sync::OnceLock<OnNamed> = std::sync::OnceLock::new();

pub fn on_named(told: OnNamed) {
    let _ = ON_NAMED.set(told);
}

pub fn usable_name(wanted: &str) -> String {
    let mut s: String = wanted.chars().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect();
    s = s.trim_start_matches(['.', ' ']).to_string();
    // Room for a " 99" when the name is taken.
    while s.len() > 96 {
        s.pop();
    }
    s = s.trim_end_matches(['.', ' ']).to_string();
    if is_backup_copy(&s) {
        s.push_str(" playlist");
    }
    if s.is_empty() { "Playlist".into() } else { s }
}

/// `name`, or "name 2", "name 3"…: the first that `taken` says is free. A name that ends in a number counts on from
/// it when the number is small ("Chill 2" taken: "Chill 3"; "Blink 182" taken: "Blink 182 2").
pub fn free_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_string();
    }
    let (stem, from) = match name.rsplit_once(' ') {
        Some((stem, n)) if !stem.is_empty() && n.len() <= 2 && n.bytes().all(|b| b.is_ascii_digit()) => (stem, n.parse::<u32>().unwrap_or(1) + 1),
        _ => (name, 2),
    };
    (from.max(2)..).map(|n| format!("{stem} {n}")).find(|n| !taken(n)).unwrap_or_default()
}

impl Store {
    /// Playlists kept in `dir` itself, entries relative to it.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        Self { base: dir.clone(), dir, prefix: String::new(), account: None, cache: RwLock::new(HashMap::new()), write: Mutex::new(()) }
    }

    /// The Music folder's playlists: in its playlist folder (`.playlists`) once that exists, else in the folder
    /// itself.
    pub fn in_music(music: impl Into<PathBuf>) -> Self {
        let base = music.into();
        let dir = base.join(PLAYLIST_DIR);
        if dir.is_dir() {
            Self { dir, base, prefix: "../".into(), account: None, cache: RwLock::new(HashMap::new()), write: Mutex::new(()) }
        } else {
            Self::new(base)
        }
    }

    /// Another account's playlists (decision 0051): in its own folder in the playlist folder (`.playlists/<account>`,
    /// made with its first playlist), entries relative to Music as everyone's are.
    pub fn of_account(music: impl Into<PathBuf>, account: &str) -> Self {
        let base = music.into();
        let dir = base.join(PLAYLIST_DIR).join(account);
        Self { dir, base, prefix: "../../".into(), account: Some(account.to_string()), cache: RwLock::new(HashMap::new()), write: Mutex::new(()) }
    }

    /// In the playlist folder (`.playlists`), where other accounts' folders can be.
    pub fn in_playlist_dir(&self) -> bool {
        self.account.is_none() && self.prefix == "../"
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    /// Whose playlists these are: another account's, or (`None`) the first administrator's.
    pub fn account(&self) -> Option<&str> {
        self.account.as_deref()
    }

    /// A playlist's key in Cratarr's own records (when its entries came, its old names): its name in the first
    /// administrator's playlists, `account/name` in another's.
    pub fn key(&self, id: &str) -> String {
        match &self.account {
            Some(a) => format!("{a}/{id}"),
            None => id.to_string(),
        }
    }

    /// The name of this store's playlist a key is of (`key`'s inverse); `None` for another store's.
    pub fn name_of_key<'a>(&self, key: &'a str) -> Option<&'a str> {
        match &self.account {
            Some(a) => key.strip_prefix(a.as_str())?.strip_prefix('/'),
            None => (!key.contains('/')).then_some(key),
        }
    }

    /// A Music-relative entry as a line of this store's files.
    fn line(&self, entry: &str) -> String {
        format!("{}{entry}", self.prefix)
    }

    fn path_of(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.m3u"))
    }

    /// Whether playlist `id` has its file.
    pub fn path_exists(&self, id: &str) -> bool {
        valid_id(id) && self.path_of(id).is_file()
    }

    /// `(id, path, stamp)` for every visible `.m3u` file directly in the folder, backup copies left out.
    async fn list(&self) -> Result<Vec<(String, PathBuf, (SystemTime, u64))>> {
        let mut rd = match tokio::fs::read_dir(&self.dir).await {
            Ok(rd) => rd,
            // An account's folder comes with its first playlist.
            Err(e) if self.account.is_some() && e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(anyhow!(e).context(format!("cannot read the playlist folder {}", self.dir.display()))),
        };
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
            if is_backup_copy(&id) {
                continue;
            }
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
                        cache.insert(id.clone(), Cached::parse(stamp, &text, &self.prefix));
                    }
                    Err(e) => {
                        tracing::warn!("cannot read {}: {e}", path.display());
                        continue;
                    }
                }
            }
            if cache[&id].dead.is_none() {
                let (dir, lines) = (self.base.clone(), cache[&id].lines.clone());
                let dead = tokio::task::spawn_blocking(move || dead_count(&dir, &lines)).await.unwrap_or(0);
                cache.get_mut(&id).expect("just inserted").dead = Some(dead);
            }
            let c = &cache[&id];
            let member = entry.is_some_and(|t| c.entries.contains(t));
            let dead = c.dead.unwrap_or(0);
            out.push(PlaylistView {
                name: id.clone(),
                member,
                count: c.lines.len().saturating_sub(dead),
                dead,
                id,
                links: Vec::new(),
                spoken: Vec::new(),
                owner: String::new(),
                shared: false,
                mine: true,
            });
        }
        out.sort_by_key(|p| p.name.to_lowercase());
        Ok(out)
    }

    /// Every playlist's lines (library-relative) and when its file last changed (epoch seconds), as `view` lists them.
    pub async fn lines_of_all(&self) -> Result<Vec<(String, i64, Vec<String>)>> {
        let ids: Vec<String> = self.view(None).await?.into_iter().map(|p| p.id).collect();
        let cache = self.cache.read().await;
        Ok(ids
            .into_iter()
            .filter_map(|id| {
                let c = cache.get(&id)?;
                let changed = c.stamp.0.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
                Some((id, changed, c.lines.clone()))
            })
            .collect())
    }

    /// Re-check every playlist's dead entries against the library (files appear and disappear without the
    /// playlist changing). Meant to run every minute or so in the background.
    pub async fn refresh_dead(&self) {
        let snapshot: Vec<(String, (SystemTime, u64), Vec<String>)> =
            self.cache.read().await.iter().map(|(id, c)| (id.clone(), c.stamp, c.lines.clone())).collect();
        let dir = self.base.clone();
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

    /// A playlist's track lines in file order (comments and blanks left out). Read-only.
    pub async fn entries(&self, id: &str) -> Result<Vec<String>, StoreError> {
        if !valid_id(id) {
            return Err(StoreError::Unknown);
        }
        let path = self.path_of(id);
        match tokio::fs::metadata(&path).await {
            Ok(m) if m.is_file() => {}
            _ => return Err(StoreError::Unknown),
        }
        let bytes = tokio::fs::read(&path).await.with_context(|| format!("reading {id}.m3u"))?;
        Ok(m3u::entries(&String::from_utf8_lossy(&bytes)).map(|l| unprefixed(&l, &self.prefix)).collect())
    }

    /// A playlist's file as it is, for its copies; `None` when there is no such playlist.
    pub async fn text(&self, id: &str) -> Option<String> {
        if !valid_id(id) {
            return None;
        }
        let bytes = tokio::fs::read(self.path_of(id)).await.ok()?;
        Some(String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
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
    /// into place. The caller must hold the write lock. Written as given, repeats included: a playlist holds a track
    /// twice when it was asked to (decision 0025, amended 2026-10-16). Returns the text as written.
    async fn commit(&self, id: &str, path: &Path, perms: std::fs::Permissions, new_text: &str) -> Result<String> {
        let before: Vec<String> = match tokio::fs::read_to_string(path).await {
            Ok(t) => m3u::entries(&t).map(|l| unprefixed(&l, &self.prefix)).collect(),
            Err(_) => Vec::new(),
        };
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
                .insert(id.to_string(), Cached::parse((m.modified().unwrap_or(UNIX_EPOCH), m.len()), new_text, &self.prefix));
        }
        if let Some(told) = ON_WRITE.get() {
            let after: Vec<String> = m3u::entries(new_text).map(|l| unprefixed(&l, &self.prefix)).collect();
            told(&self.key(id), &before, &after);
        }
        Ok(new_text.to_string())
    }

    /// Tells `on_named` that playlist `id` came (`true`) or went.
    fn named(&self, id: &str, there: bool) {
        if let Some(told) = ON_NAMED.get() {
            told(&self.key(id), there);
        }
    }

    /// Make the playlist exactly `entries`, in order. Backed up like every edit. Returns false when it already was.
    /// Tests only: the sync uses `set_entries_if`, which never overwrites an edit it has not seen.
    #[cfg(test)]
    pub async fn set_entries(&self, id: &str, entries: &[String]) -> Result<bool, StoreError> {
        let _one_writer = self.write.lock().await;
        let (path, perms, text) = self.load_for_edit(id).await?;
        let lines: Vec<String> = entries.iter().map(|e| self.line(e)).collect();
        let new_text = m3u::with_entries(&text, &lines);
        if new_text == text {
            return Ok(false);
        }
        self.commit(id, &path, perms, &new_text).await?;
        Ok(true)
    }

    /// As `set_entries`, but only when the playlist still holds `expected` (else nothing is written and false is
    /// returned): the sync must not overwrite an edit made while it was merging.
    pub async fn set_entries_if(&self, id: &str, expected: &[String], entries: &[String]) -> Result<bool, StoreError> {
        Ok(self.replace_entries_if(id, expected, entries).await?.is_some())
    }

    /// As `set_entries_if`, saying what the file was and became (`None`: it no longer held `expected`).
    pub async fn replace_entries_if(&self, id: &str, expected: &[String], entries: &[String]) -> Result<Option<Edited>, StoreError> {
        let _one_writer = self.write.lock().await;
        let (path, perms, text) = self.load_for_edit(id).await?;
        if m3u::entries(&text).map(|l| unprefixed(&l, &self.prefix)).collect::<Vec<_>>() != expected {
            return Ok(None);
        }
        let lines: Vec<String> = entries.iter().map(|e| self.line(e)).collect();
        let mut new_text = m3u::with_entries(&text, &lines);
        if new_text != text {
            new_text = self.commit(id, &path, perms, &new_text).await?;
        }
        Ok(Some(Edited { id: id.to_string(), before: text, after: new_text }))
    }

    /// Put a playlist's file back to `text` (an undo, decision 0026), only while it is still what the change left
    /// (`left`, its fingerprint); `None` when it changed since. Committed like every edit.
    pub async fn put_back_if(&self, id: &str, left: u64, text: &str) -> Result<Option<Edited>, StoreError> {
        let _one_writer = self.write.lock().await;
        let (path, perms, now) = self.load_for_edit(id).await?;
        if fingerprint(&now) != left {
            return Ok(None);
        }
        let after = if now != text { self.commit(id, &path, perms, text).await? } else { now.clone() };
        Ok(Some(Edited { id: id.to_string(), before: now, after }))
    }

    /// Delete a playlist (as `delete`) only while it is still what a change left (`left`); false when it changed since.
    pub async fn delete_if(&self, id: &str, left: u64) -> Result<bool, StoreError> {
        let _one_writer = self.write.lock().await;
        let (path, _, now) = self.load_for_edit(id).await?;
        if fingerprint(&now) != left {
            return Ok(false);
        }
        self.backup(id, &path).await?;
        tokio::fs::remove_file(&path).await.with_context(|| format!("deleting {id}.m3u"))?;
        self.cache.write().await.remove(id);
        self.named(id, false);
        Ok(true)
    }

    /// Make `entry` a member of the playlist (`member`) or not. Idempotent: asking twice for the same state changes
    /// nothing the second time, so a repeated or retried request is harmless. Returns whether the file changed.
    pub async fn set_member(&self, id: &str, entry: &str, member: bool) -> Result<bool, StoreError> {
        let _one_writer = self.write.lock().await;
        let (path, perms, text) = self.load_for_edit(id).await?;
        let line = self.line(entry);
        let new_text = match (m3u::contains(&text, &line), member) {
            (false, true) => m3u::with_added(&text, &line),
            (true, false) => m3u::with_removed(&text, &line).0,
            _ => return Ok(false),
        };
        self.commit(id, &path, perms, &new_text).await?;
        Ok(true)
    }

    /// Add `entry` to the playlist, or remove it if it is already there.
    pub async fn toggle(&self, id: &str, entry: &str) -> Result<Toggled, StoreError> {
        let _one_writer = self.write.lock().await;
        let (path, perms, text) = self.load_for_edit(id).await?;
        let line = self.line(entry);
        let (new_text, added) = if m3u::contains(&text, &line) {
            (m3u::with_removed(&text, &line).0, false)
        } else {
            (m3u::with_added(&text, &line), true)
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
        let dir = self.base.clone();
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
    /// with paths). One backup and one write per playlist. Returns the playlists that changed, each as it was and is.
    pub async fn apply_repairs(&self, wanted: &[Repair]) -> Result<Vec<Edited>, StoreError> {
        let offered = self.repairs().await?;
        let is_offered = |w: &Repair| offered.safe.iter().chain(&offered.review).any(|o| o == w);
        if wanted.iter().any(|w| !is_offered(w)) {
            return Err(StoreError::NotOffered);
        }
        self.write_repairs(wanted).await
    }

    /// One dead line pointed at another file (Health's missing tracks found again; the owner, 2026-10-24): applied as a
    /// repair is, though no repair offered it (the caller found it by what the line says). A path is still never
    /// trusted: `from` must be a line of the playlist whose file is gone, `to` a file there is, both inside Music.
    pub async fn relink(&self, r: &Repair) -> Result<Vec<Edited>, StoreError> {
        let (_, _, text) = self.load_for_edit(&r.playlist).await?;
        let dead = m3u::contains(&text, &self.line(&r.from)) && !entry_exists(&self.base, &r.from);
        if !dead || !entry_exists(&self.base, &r.to) {
            return Err(StoreError::NotOffered);
        }
        self.write_repairs(std::slice::from_ref(r)).await
    }

    /// Repairs written: one backup and one write per playlist.
    async fn write_repairs(&self, wanted: &[Repair]) -> Result<Vec<Edited>, StoreError> {
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
            let (path, perms, before) = self.load_for_edit(&id).await?;
            let mut text = before.clone();
            let mut total = 0;
            for r in repairs {
                let (from, to) = (self.line(&r.from), self.line(&r.to));
                // A dead line pointed at a file the playlist already lists would become a repeat nobody asked for:
                // it goes instead (decision 0025, amended 2026-10-16: repeats only when asked for).
                let (t, n) = if m3u::contains(&text, &to) { m3u::with_removed(&text, &from) } else { m3u::with_replaced(&text, &from, &to) };
                text = t;
                total += n;
            }
            if total > 0 {
                let after = self.commit(&id, &path, perms, &text).await?;
                changed.push(Edited { id, before, after });
            }
        }
        Ok(changed)
    }

    /// Give every playlist the entries `edit` returns for it (`None`: leave it as it is), each backed up like every
    /// edit. A file that is not UTF-8 is left alone. Returns the playlists that changed.
    pub async fn edit_all(&self, edit: impl Fn(&str, &[String]) -> Option<Vec<String>>) -> Result<Vec<String>> {
        let _one_writer = self.write.lock().await;
        let mut changed = Vec::new();
        for (id, _, _) in self.list().await? {
            let (path, perms, text) = match self.load_for_edit(&id).await {
                Ok(f) => f,
                Err(StoreError::Unknown) => continue, // gone since the listing
                Err(e) => {
                    tracing::warn!("{id}: {e:#}");
                    continue;
                }
            };
            let entries: Vec<String> = m3u::entries(&text).map(|l| unprefixed(&l, &self.prefix)).collect();
            let Some(new) = edit(&id, &entries) else { continue };
            let lines: Vec<String> = new.iter().map(|e| self.line(e)).collect();
            let new_text = m3u::with_entries(&text, &lines);
            if new_text != text {
                self.commit(&id, &path, perms, &new_text).await?;
                changed.push(id);
            }
        }
        Ok(changed)
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

    /// Every playlist's lines and missing-track count, by name (for Insights).
    pub async fn snapshot(&self) -> Result<Vec<(String, Vec<String>, usize)>> {
        self.view(None).await?;
        let cache = self.cache.read().await;
        let mut out: Vec<(String, Vec<String>, usize)> =
            cache.iter().map(|(id, c)| (id.clone(), c.lines.clone(), c.dead.unwrap_or(0))).collect();
        out.sort_by_key(|(id, _, _)| id.to_lowercase());
        Ok(out)
    }

    /// Take the repeats out of one playlist, the first copy of each track staying: the user's "Remove repeats"
    /// (decision 0025, amended 2026-10-16). `None` when it held none.
    pub async fn remove_repeats(&self, id: &str) -> Result<Option<Edited>, StoreError> {
        let _one_writer = self.write.lock().await;
        let (path, perms, text) = self.load_for_edit(id).await?;
        let (deduped, removed) = m3u::with_deduped(&text);
        if removed == 0 {
            return Ok(None);
        }
        let after = self.commit(id, &path, perms, &deduped).await?;
        Ok(Some(Edited { id: id.to_string(), before: text, after }))
    }

    /// Add `entries` at the playlist's end, in order, whether or not it holds them already: the user chose Add again
    /// (decision 0025, amended 2026-10-16). Membership asks use `set_member`, which never makes a repeat.
    pub async fn append(&self, id: &str, entries: &[String]) -> Result<Edited, StoreError> {
        let _one_writer = self.write.lock().await;
        let (path, perms, text) = self.load_for_edit(id).await?;
        let new_text = entries.iter().fold(text.clone(), |t, e| m3u::with_added(&t, &self.line(e)));
        let after = if new_text != text { self.commit(id, &path, perms, &new_text).await? } else { text.clone() };
        Ok(Edited { id: id.to_string(), before: text, after })
    }

    /// Create an empty playlist. Never overwrites an existing file.
    pub async fn create(&self, name: &str) -> Result<(), StoreError> {
        if !valid_new_name(name) {
            return Err(StoreError::BadName);
        }
        let _one_writer = self.write.lock().await;
        let path = self.path_of(name);
        if self.account.is_some() {
            tokio::fs::create_dir_all(&self.dir).await.with_context(|| format!("making {}", self.dir.display()))?;
        }
        // `create_new` fails if the file exists (also on a case-insensitive clash), so nothing is overwritten.
        match tokio::fs::OpenOptions::new().write(true).create_new(true).open(&path).await {
            Ok(_) => {
                // Group-writable as the playlists around it, as `restore` does: the container's umask (022) made new
                // playlists 644 (seen 2026-10-15).
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o664)).await;
                }
                self.named(name, true);
                Ok(())
            }
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
        self.named(from, false);
        self.named(to, true);
        Ok(())
    }

    /// Delete a playlist. The file is moved into the backup folder, never destroyed, so `restore_deleted` can bring it back.
    pub async fn delete(&self, id: &str) -> Result<(), StoreError> {
        let _one_writer = self.write.lock().await;
        let (path, _, _) = self.load_for_edit(id).await?;
        self.backup(id, &path).await?;
        tokio::fs::remove_file(&path).await.with_context(|| format!("deleting {id}.m3u"))?;
        self.cache.write().await.remove(id);
        self.named(id, false);
        Ok(())
    }

    /// Bring back a deleted playlist from its newest backup (refuses if a playlist of that name exists again). Returns
    /// the file it came back with.
    pub async fn restore_deleted(&self, id: &str) -> Result<String, StoreError> {
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
        tokio::fs::write(&path, &bytes).await.with_context(|| format!("restoring {id}.m3u"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o664)).await;
        }
        self.named(id, true);
        Ok(String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
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

/// Bring the playlist files in Music into the playlist folder (`.playlists`, decision 0014): each is copied there
/// with "../" in front of its lines, so they still point at the same files, unless one of that name is there already.
/// The file in Music stays, as the playlist's shared copy (the media server keeps the playlist it imported). Returns
/// the name of every playlist found in Music. Backup copies (`*.bak.m3u`) are left out. Safe to run again. Blocking.
pub fn adopt_into_playlist_dir(music: &Path) -> Result<Vec<String>> {
    let folder = music.join(PLAYLIST_DIR);
    crate::moves::create_dirs_like_parent(&folder).with_context(|| format!("making {}", folder.display()))?;
    let mut names: Vec<String> = std::fs::read_dir(music)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| !n.starts_with('.') && n.to_lowercase().ends_with(".m3u") && !is_backup_copy(&n[..n.len() - 4]))
        .collect();
    names.sort();
    let mut out = Vec::new();
    for name in names {
        let dest = folder.join(&name);
        if !dest.exists() {
            let src = music.join(&name);
            let Ok(text) = std::fs::read_to_string(&src) else {
                tracing::warn!("{name} is not valid UTF-8; left out");
                continue;
            };
            let tmp = folder.join(format!(".{name}.cratarr-tmp"));
            std::fs::write(&tmp, m3u::with_prefix(&text, "../")).with_context(|| format!("writing {}", tmp.display()))?;
            if let Ok(meta) = std::fs::metadata(&src) {
                std::fs::set_permissions(&tmp, meta.permissions()).ok();
            }
            std::fs::rename(&tmp, &dest).with_context(|| format!("copying {name} into {PLAYLIST_DIR}"))?;
        }
        out.push(name[..name.len() - 4].to_string());
    }
    Ok(out)
}


/// `target` as a path relative to the folder `base` ("../lidarr-music/A/01.flac").
pub fn relative_to(base: &Path, target: &Path) -> Option<String> {
    let (b, t): (Vec<_>, Vec<_>) = (base.components().collect(), target.components().collect());
    let common = b.iter().zip(&t).take_while(|(x, y)| x == y).count();
    if common == 0 {
        return None;
    }
    let mut parts: Vec<String> = std::iter::repeat_n("..".to_string(), b.len() - common).collect();
    parts.extend(t[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
        d.write("Love.bak.m3u", "A/1.mp3\n");
        std::fs::create_dir(d.0.join("Some Album (2020)")).unwrap();
        let v = d.store().view(Some("A/1.mp3")).await.unwrap();
        assert_eq!(
            v,
            vec![
                PlaylistView { id: "Analogic".into(), name: "Analogic".into(), member: false, count: 1, dead: 0, links: Vec::new(), spoken: Vec::new(), owner: String::new(), shared: false, mine: true },
                PlaylistView { id: "chill".into(), name: "chill".into(), member: true, count: 2, dead: 0, links: Vec::new(), spoken: Vec::new(), owner: String::new(), shared: false, mine: true },
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
    async fn asking_for_the_same_membership_twice_changes_nothing_the_second_time() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3
");
        let s = d.store();
        assert!(s.set_member("a", "B/2.mp3", true).await.unwrap());
        assert!(!s.set_member("a", "B/2.mp3", true).await.unwrap(), "a retry adds nothing");
        assert_eq!(d.read("a.m3u"), "A/1.mp3
B/2.mp3
");
        assert!(s.set_member("a", "B/2.mp3", false).await.unwrap());
        assert!(!s.set_member("a", "B/2.mp3", false).await.unwrap());
        assert_eq!(d.read("a.m3u"), "A/1.mp3
");
    }

    #[tokio::test]
    async fn adopted_into_the_playlist_folder_everything_still_points_at_the_same_files() {
        let d = Dir::new();
        d.touch("A/1.mp3");
        d.write("Chill.m3u", "#EXTM3U\nA/1.mp3\ngone/2.mp3\n");
        d.write("Old.bak.m3u", "A/1.mp3\n");
        assert_eq!(adopt_into_playlist_dir(&d.0).unwrap(), vec!["Chill".to_string()]);
        assert_eq!(d.read(".playlists/Chill.m3u"), "#EXTM3U\n../A/1.mp3\n../gone/2.mp3\n");
        assert_eq!(d.read("Chill.m3u"), "#EXTM3U\nA/1.mp3\ngone/2.mp3\n", "the file in Music stays, as the shared copy");
        assert!(!d.0.join(".playlists/Old.bak.m3u").exists(), "backup copies are left out");
        // Run again: the one in .playlists is kept, whatever Music holds.
        d.write("Chill.m3u", "B/9.mp3\n");
        assert_eq!(adopt_into_playlist_dir(&d.0).unwrap(), vec!["Chill".to_string()]);
        assert_eq!(d.read(".playlists/Chill.m3u"), "#EXTM3U\n../A/1.mp3\n../gone/2.mp3\n");
        // The store finds them there, with Music-relative entries as before.
        let s = Store::in_music(&d.0);
        let v = s.view(Some("A/1.mp3")).await.unwrap();
        assert_eq!((v[0].name.as_str(), v[0].member, v[0].count, v[0].dead), ("Chill", true, 1, 1));
        assert_eq!(s.entries("Chill").await.unwrap(), vec!["A/1.mp3".to_string(), "gone/2.mp3".to_string()]);
        s.set_member("Chill", "B/3.mp3", true).await.unwrap();
        assert_eq!(d.read(".playlists/Chill.m3u"), "#EXTM3U\n../A/1.mp3\n../gone/2.mp3\n../B/3.mp3\n");
        assert_eq!(s.text("Chill").await.as_deref(), Some("#EXTM3U\n../A/1.mp3\n../gone/2.mp3\n../B/3.mp3\n"));
        assert_eq!(s.text("None").await, None);
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
        assert_eq!(s.view(None).await.unwrap()[0].count, 4, "a write keeps both copies of A");
        s.toggle("a", "A/1.mp3").await.unwrap();
        assert_eq!(s.view(None).await.unwrap()[0].count, 2, "taking A out takes every copy");
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
        assert_eq!(changed.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["p"]);
        assert_eq!(changed[0].before, "#c\nAlb (2020)/01 A - One.mp3\r\nAlb (2020)/02 A - Two.mp3\r\n", "what undo puts back");
        assert_eq!(d.read("p.m3u"), "#c\nAlb (2020)/01 A - One.flac\r\nAlb (2020)/02 A - Two.mp3\r\n");
        let v = s.view(None).await.unwrap();
        assert_eq!((v[0].count, v[0].dead), (2, 0));
        assert_eq!(std::fs::read_dir(d.0.join(BACKUP_DIR)).unwrap().count(), 1);
        assert!(s.repairs().await.unwrap().safe.is_empty());
    }

    #[tokio::test]
    async fn a_repair_onto_a_file_the_playlist_already_lists_takes_the_dead_line_out() {
        let d = Dir::new();
        d.touch("Alb (2020)/01 A - One.flac");
        d.write("p.m3u", "Alb (2020)/01 A - One.flac
Alb (2020)/01 A - One.mp3
");
        let s = d.store();
        let r = s.repairs().await.unwrap();
        assert_eq!(r.safe.len(), 1, "{r:?}");
        s.apply_repairs(&r.safe).await.unwrap();
        assert_eq!(d.read("p.m3u"), "Alb (2020)/01 A - One.flac
", "no repeat made by a repair");
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
    async fn a_dead_line_is_relinked_to_a_file_found_for_it_and_nothing_else() {
        let d = Dir::new();
        d.touch("Comp (2005)/1-07 Aphex Twin - Tha.mp3");
        d.touch("Alb (2020)/02 A - Here.mp3");
        d.write("p.m3u", "Aphex Twin/SAW (1992)/02 Aphex Twin - Tha.mp3\nAlb (2020)/02 A - Here.mp3\n");
        let s = d.store();
        let to = |from: &str, to: &str| Repair { playlist: "p".into(), from: from.into(), to: to.into() };
        assert!(s.relink(&to("Aphex Twin/SAW (1992)/02 Aphex Twin - Tha.mp3", "../../etc/passwd")).await.is_err(), "a path out of Music");
        assert!(s.relink(&to("Aphex Twin/SAW (1992)/02 Aphex Twin - Tha.mp3", "Comp (2005)/gone.mp3")).await.is_err(), "a file that is not there");
        assert!(s.relink(&to("Alb (2020)/02 A - Here.mp3", "Comp (2005)/1-07 Aphex Twin - Tha.mp3")).await.is_err(), "a line that is not dead");
        assert!(s.relink(&to("Not/in the playlist.mp3", "Comp (2005)/1-07 Aphex Twin - Tha.mp3")).await.is_err(), "a line it does not hold");
        assert_eq!(d.read("p.m3u"), "Aphex Twin/SAW (1992)/02 Aphex Twin - Tha.mp3\nAlb (2020)/02 A - Here.mp3\n", "nothing changed by those");
        let changed = s.relink(&to("Aphex Twin/SAW (1992)/02 Aphex Twin - Tha.mp3", "Comp (2005)/1-07 Aphex Twin - Tha.mp3")).await.unwrap();
        assert_eq!(changed.len(), 1);
        assert_eq!(d.read("p.m3u"), "Comp (2005)/1-07 Aphex Twin - Tha.mp3\nAlb (2020)/02 A - Here.mp3\n");
    }

    #[tokio::test]
    async fn a_file_is_put_back_only_while_it_is_what_the_change_left() {
        let d = Dir::new();
        d.write("p.m3u", "A/1.mp3\n");
        let s = d.store();
        let e = s.replace_entries_if("p", &["A/1.mp3".into()], &[]).await.unwrap().unwrap();
        assert_eq!((e.before.as_str(), e.after.as_str()), ("A/1.mp3\n", ""));
        let back = s.put_back_if("p", fingerprint(&e.after), &e.before).await.unwrap().unwrap();
        assert_eq!(d.read("p.m3u"), "A/1.mp3\n");
        assert_eq!(back.before, "", "what the redo puts back");
        assert_eq!(s.put_back_if("p", fingerprint(""), "").await.unwrap(), None, "it changed since: nothing is written");
        assert_eq!(d.read("p.m3u"), "A/1.mp3\n");
        assert!(matches!(s.put_back_if("nope", 0, "").await, Err(StoreError::Unknown)));
        assert!(!s.delete_if("p", fingerprint("")).await.unwrap(), "changed since");
        assert!(s.delete_if("p", fingerprint("A/1.mp3\n")).await.unwrap());
        assert_eq!(s.restore_deleted("p").await.unwrap(), "A/1.mp3\n", "kept aside, as a delete");
    }

    #[tokio::test]
    async fn remove_repeats_keeps_the_first_copy_of_one_playlist() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\nB/2.mp3\nA/1.mp3\nA/1.mp3\n");
        d.write("b.m3u", "A/1.mp3\nA/1.mp3\n");
        let s = d.store();
        let edit = s.remove_repeats("a").await.unwrap().unwrap();
        assert_eq!(d.read("a.m3u"), "A/1.mp3\nB/2.mp3\n");
        assert_eq!(edit.before, "A/1.mp3\nB/2.mp3\nA/1.mp3\nA/1.mp3\n", "as it was, for undo");
        assert_eq!(d.read("b.m3u"), "A/1.mp3\nA/1.mp3\n", "only the playlist asked for");
        assert!(s.remove_repeats("a").await.unwrap().is_none(), "none left");
    }

    #[tokio::test]
    async fn writes_keep_repeats_and_append_adds_again() {
        let d = Dir::new();
        d.write("p.m3u", "A/1.mp3\nB/2.mp3\n");
        let s = d.store();
        let e = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let edit = s.replace_entries_if("p", &e(&["A/1.mp3", "B/2.mp3"]), &e(&["B/2.mp3", "A/1.mp3", "B/2.mp3"])).await.unwrap().unwrap();
        assert_eq!(d.read("p.m3u"), "B/2.mp3\nA/1.mp3\nB/2.mp3\n", "as an app wrote it (decision 0025, amended 2026-10-16)");
        assert_eq!(edit.after, d.read("p.m3u"), "what was written, for undo");
        let back = s.put_back_if("p", fingerprint(&edit.after), "C/3.mp3\nC/3.mp3\n").await.unwrap().unwrap();
        assert_eq!(back.after, "C/3.mp3\nC/3.mp3\n");
        assert!(!s.set_member("p", "C/3.mp3", true).await.unwrap(), "a member asked for is never added twice");
        s.append("p", &e(&["C/3.mp3", "A/1.mp3"])).await.unwrap();
        assert_eq!(d.read("p.m3u"), "C/3.mp3\nC/3.mp3\nC/3.mp3\nA/1.mp3\n", "Add again: at the end, whatever it holds");
    }

    #[tokio::test]
    async fn a_new_playlist_is_created_empty_and_never_overwrites() {
        let d = Dir::new();
        d.write("old.m3u", "A/1.mp3\n");
        let s = d.store();
        s.create("Late Night").await.unwrap();
        assert_eq!(d.read("Late Night.m3u"), "");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(d.0.join("Late Night.m3u")).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o664, "group-writable whatever the umask");
        }
        assert!(s.view(None).await.unwrap().iter().any(|p| p.id == "Late Night"));
        assert!(matches!(s.create("old").await, Err(StoreError::Exists)));
        assert_eq!(d.read("old.m3u"), "A/1.mp3\n");
    }

    #[test]
    fn a_name_from_elsewhere_is_made_usable_and_free() {
        for (wanted, usable) in [
            ("AC/DC: Live?", "AC-DC- Live-"),
            ("  .hidden. ", "hidden"),
            ("", "Playlist"),
            ("...", "Playlist"),
            ("old.bak", "old.bak playlist"),
            ("Chill", "Chill"),
        ] {
            assert_eq!(usable_name(wanted), usable, "{wanted:?}");
            assert!(valid_new_name(&usable_name(wanted)), "{wanted:?}");
        }
        assert!(valid_new_name(&usable_name(&"é".repeat(80))));
        let taken = ["Chill", "Chill 2"];
        assert_eq!(free_name("Chill", |n| taken.contains(&n)), "Chill 3");
        assert_eq!(free_name("Calm", |n| taken.contains(&n)), "Calm");
        assert_eq!(free_name("Chill 2", |n| taken.contains(&n)), "Chill 3");
        assert_eq!(free_name("Blink 182", |n| n == "Blink 182"), "Blink 182 2");
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
