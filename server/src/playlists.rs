//! The playlists (decisions 0012, 0101): kept in the user data (`crate::userdata::playlists`), each one's `.m3u` copy
//! written out after every change into a folder of its own, never read back. Until 0.15 they were `.m3u` files in
//! Music's playlist folder; those are brought in once (`import`) and left where they are.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

use crate::journal::fingerprint;
use crate::m3u;
use crate::playlist_ops::Entries;
use crate::userdata::UserData;
use crate::catalogue::Catalogue;
use crate::userdata::playlists::{Named, Playlist, Tracks, Written};

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

/// Whether a playlist line's file is there: in the folder lines are made from, or, for a line that climbs out of it, in the
/// folder it names (`Catalogue::path_of_line`). Blocking.
pub(crate) fn line_exists(cat: &Catalogue, line: &str) -> bool {
    if line.starts_with("../") {
        return cat.path_of_line(line).is_some_and(|p| p.is_file());
    }
    cat.folders().first().is_some_and(|(_, first)| entry_exists(first, line))
}

/// Where an entry leads when its own file went, or no longer holds its track, as a copy that left its record takes tracks of
/// its own: its track's line now (decision 0101 point 11: an entry is its track). None while its file is there holding it, or
/// where its track has none. Blocking.
fn led_to(cat: &Catalogue, line: &str, track: &str) -> Option<String> {
    if line_exists(cat, line) && cat.track_of_line(line).is_none_or(|held| held == track) {
        return None;
    }
    let now = cat.row_line(track.strip_prefix(crate::serve::ids::SONG)?.parse().ok()?)?;
    (now != line).then_some(now)
}

/// True when `entry` (library-relative) names an existing file under `dir`. Absolute paths and `..` never resolve.
pub(crate) fn entry_exists(dir: &Path, entry: &str) -> bool {
    let p = Path::new(entry);
    let safe = !p.is_absolute() && p.components().all(|c| matches!(c, std::path::Component::Normal(_)));
    safe && dir.join(p).is_file()
}

fn dead_count(cat: &Catalogue, lines: &[String]) -> usize {
    lines.iter().filter(|l| !line_exists(cat, l)).count()
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

/// A playlist's text (`text_of`) before and after a change: what undoing it needs (decision 0026).
#[derive(Debug, Clone, PartialEq)]
pub struct Edited {
    pub id: String,
    pub before: String,
    pub after: String,
}

/// Whether an account's name can name its playlists' folder: plain, as the directory's names are.
pub fn account_folder_ok(account: &str) -> bool {
    !account.is_empty() && account.len() <= 64 && !account.starts_with('.') && account.chars().all(|c| c.is_ascii_alphanumeric() || "._-@".contains(c))
}

/// Backup copies kept next to the playlists ("Love.bak.m3u") are not playlists: they are left alone, unlisted.
fn is_backup_copy(id: &str) -> bool {
    id.to_lowercase().ends_with(".bak")
}

/// A playlist id is its name; it names its `.m3u` copy, so it must never be able to point outside its folder.
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

/// A playlist's lines as its text: one Music-relative path a line. What its `.m3u` copy holds, and what an undo
/// keeps (decision 0026).
pub fn text_of(lines: &[String]) -> String {
    lines.iter().map(|l| format!("{l}\n")).collect()
}

pub struct Store {
    db: Arc<UserData>,
    /// The library's files, whose tracks the entries are (decision 0101 point 11).
    cat: Arc<Catalogue>,
    /// Whose playlists (decision 0051): another account's; `None`: the first administrator's.
    account: Option<String>,
    /// The Music folder: entries are relative to it.
    base: PathBuf,
    /// Where the playlists' `.m3u` copies are written after each change (decision 0101), another account's in a folder
    /// of its own there; none: not written.
    out: Option<PathBuf>,
    /// How many of each playlist's entries point at a missing file, by the playlist's id, with the print of the lines
    /// counted: they change with the library, not the playlist (`refresh_dead`).
    dead: RwLock<HashMap<String, (u64, usize)>>,
    write: Mutex<()>,
}

/// Told when a playlist comes (`true`: made, renamed, brought back) or goes (deleted), by its key (its id), and whether
/// others see it (shared): the change log (decision 0087). Its writes are logged as they are written. Set once at
/// start.
pub type OnNamed = Box<dyn Fn(&str, bool, bool) + Send + Sync>;
static ON_NAMED: std::sync::OnceLock<OnNamed> = std::sync::OnceLock::new();

pub fn on_named(told: OnNamed) {
    let _ = ON_NAMED.set(told);
}

impl Store {
    /// One account's playlists (`None`: the first administrator's), their entries relative to `base` (Music).
    pub fn open(db: Arc<UserData>, cat: Arc<Catalogue>, account: Option<&str>, base: impl Into<PathBuf>, out: Option<PathBuf>) -> Self {
        Self {
            db,
            cat,
            account: account.map(str::to_string),
            base: base.into(),
            out,
            dead: RwLock::new(HashMap::new()),
            write: Mutex::new(()),
        }
    }

    /// The first administrator's playlists over the Music folder, their `.m3u` files brought in first (once:
    /// `import`).
    pub async fn in_music(db: Arc<UserData>, cat: Arc<Catalogue>, music: impl Into<PathBuf>, out: Option<PathBuf>) -> Result<Self> {
        let music = music.into();
        let (d, c, m, o) = (db.clone(), cat.clone(), music.clone(), out.clone());
        tokio::task::spawn_blocking(move || import(&d, &c, &m, o.as_deref())).await??;
        Ok(Self::open(db, cat, None, music, out))
    }

    /// Tests: the playlists of `dir`'s `.m3u` files, entries relative to it, in a database of their own, their copies
    /// written back out there.
    #[cfg(test)]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let db = Arc::new(UserData::open(None).expect("an in-memory database"));
        let cat = Arc::new(Catalogue::open(None, vec![dir.clone()]).expect("an in-memory catalogue"));
        import(&db, &cat, &dir, Some(&dir)).expect("the playlists brought in");
        Self::open(db, cat, None, dir.clone(), Some(dir))
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    /// Whose playlists these are: another account's, or (`None`) the first administrator's.
    pub fn account(&self) -> Option<&str> {
        self.account.as_deref()
    }

    /// The owner as the database keeps it: '' the first administrator.
    fn owner(&self) -> String {
        self.account.clone().unwrap_or_default()
    }

    /// A playlist's key in Cratarr's own records and to every client (the change log, favourites, its spoken names,
    /// what a move of its tracks left): its id (decision 0101), which a rename keeps; the one of that name there now,
    /// else the last set aside. A name no playlist ever had is its own key (nothing is kept for it).
    pub fn key(&self, name: &str) -> String {
        self.db.playlist_id(&self.owner(), name).ok().flatten().unwrap_or_else(|| name.to_string())
    }


    /// The database's answer, its work done off the async threads.
    async fn ask<T: Send + 'static>(&self, f: impl FnOnce(&UserData, &str) -> Result<T> + Send + 'static) -> Result<T> {
        let (db, owner) = (self.db.clone(), self.owner());
        tokio::task::spawn_blocking(move || f(&db, &owner)).await?
    }

    /// Whether there is such a playlist.
    pub fn has(&self, id: &str) -> bool {
        valid_id(id) && self.db.playlist(&self.owner(), id).ok().flatten().is_some()
    }

    /// Every playlist with its lines, as kept.
    async fn all(&self) -> Result<Vec<(Playlist, Vec<String>)>> {
        self.ask(|db, owner| db.playlists(owner)).await
    }

    /// The dead count of each of `lists`, by id: counted again where its lines changed since.
    async fn dead_of(&self, lists: &[(Playlist, Vec<String>)]) -> HashMap<String, usize> {
        let known = self.dead.read().await.clone();
        let stale: Vec<(String, u64, Vec<String>)> = lists
            .iter()
            .map(|(p, lines)| (p.id.clone(), fingerprint(&text_of(lines)), lines.clone()))
            .filter(|(id, print, _)| known.get(id).is_none_or(|(p, _)| p != print))
            .collect();
        let mut out: HashMap<String, usize> = known.iter().map(|(id, (_, n))| (id.clone(), *n)).collect();
        if !stale.is_empty() {
            let cat = self.cat.clone();
            let counted = tokio::task::spawn_blocking(move || {
                stale.into_iter().map(|(id, print, lines)| (id, print, dead_count(&cat, &lines))).collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default();
            let mut cache = self.dead.write().await;
            for (id, print, n) in counted {
                cache.insert(id.clone(), (print, n));
                out.insert(id, n);
            }
        }
        out
    }

    /// Every playlist, name-sorted (case-insensitive), flagged by whether it contains `entry`.
    pub async fn view(&self, entry: Option<&str>) -> Result<Vec<PlaylistView>> {
        let lists = self.all().await?;
        let dead = self.dead_of(&lists).await;
        let mut out: Vec<PlaylistView> = lists
            .into_iter()
            .map(|(p, lines)| {
                let dead = dead.get(&p.id).copied().unwrap_or(0);
                PlaylistView {
                    member: entry.is_some_and(|t| lines.iter().any(|l| l == t)),
                    count: lines.len().saturating_sub(dead),
                    dead,
                    id: p.id,
                    name: p.name,
                    links: Vec::new(),
                    spoken: Vec::new(),
                    owner: String::new(),
                    shared: false,
                    mine: true,
                }
            })
            .collect();
        out.sort_by_key(|p| p.name.to_lowercase());
        Ok(out)
    }

    /// Every playlist's lines (library-relative) and when it last changed (epoch seconds), by its id, in name order.
    pub async fn lines_of_all(&self) -> Result<Vec<(String, i64, Vec<String>)>> {
        let mut all = self.all().await?;
        all.sort_by_key(|(p, _)| p.name.to_lowercase());
        Ok(all.into_iter().map(|(p, lines)| (p.id, p.changed, lines)).collect())
    }

    /// Re-check every playlist's dead entries against the library (files appear and disappear without the
    /// playlist changing), an entry whose file went led first to where its track's file is now (`follow_tracks`).
    /// Meant to run every minute or so in the background.
    pub async fn refresh_dead(&self) {
        self.follow_tracks().await;
        let Ok(lists) = self.all().await else { return };
        let cat = self.cat.clone();
        let counted = tokio::task::spawn_blocking(move || {
            lists.into_iter().map(|(p, lines)| (p.id, fingerprint(&text_of(&lines)), dead_count(&cat, &lines))).collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        let mut cache = self.dead.write().await;
        cache.clear();
        for (id, print, n) in counted {
            cache.insert(id, (print, n));
        }
    }

    /// Entries whose file went, led to where their track's file is now (decision 0101 point 11: an entry is its
    /// track; a file renamed or moved where Cratarr did not move it, a copy of the record playing now), each such
    /// playlist's copy written out again; and entries without a track given the one their line names, once the
    /// catalogue knows its file (written before the catalogue read it, at the import of a library not read yet).
    async fn follow_tracks(&self) {
        let _one_writer = self.write.lock().await;
        let (db, cat, owner) = (self.db.clone(), self.cat.clone(), self.owner());
        let done = tokio::task::spawn_blocking(move || -> Result<Vec<(String, Vec<String>)>> {
            let given: Vec<(String, String, String)> = db
                .untracked_entries(&owner)?
                .into_iter()
                .filter(|(_, _, line)| line_exists(&cat, line))
                .filter_map(|(name, entry, line)| Some((name, entry, cat.track_of_line(&line)?)))
                .collect();
            if !given.is_empty() {
                db.give_tracks(&owner, &given)?;
            }
            let mut moves = Vec::new();
            for (name, entry, line, track) in db.tracked_entries(&owner)? {
                if let Some(now) = led_to(&cat, &line, &track) {
                    moves.push((name, entry, now));
                }
            }
            if moves.is_empty() {
                return Ok(Vec::new());
            }
            db.relink_playlists(&owner, &moves)
        })
        .await;
        match done {
            Ok(Ok(changed)) => {
                for (name, lines) in changed {
                    self.write_out(&name, Some(&lines)).await;
                }
            }
            Ok(Err(e)) => tracing::warn!("playlists: entries not led to their tracks' files: {e:#}"),
            Err(e) => tracing::warn!("playlists: entries not led to their tracks' files: {e}"),
        }
    }

    /// A playlist's lines in order. Read-only.
    pub async fn entries(&self, id: &str) -> Result<Vec<String>, StoreError> {
        if !valid_id(id) {
            return Err(StoreError::Unknown);
        }
        let name = id.to_string();
        self.ask(move |db, owner| Ok(db.playlist(owner, &name)?.map(|(_, lines)| lines))).await?.ok_or(StoreError::Unknown)
    }

    /// A playlist's lines as they play now: an entry whose file went reads where its track's file is, as the next
    /// `follow_tracks` will write it (decision 0101 point 11). A copy deleted leaves its lines on the other copy with no gap
    /// for that minute to fall into. Read-only.
    pub async fn led_entries(&self, id: &str) -> Result<Vec<String>, StoreError> {
        if !valid_id(id) {
            return Err(StoreError::Unknown);
        }
        let (name, cat) = (id.to_string(), self.cat.clone());
        let led = self.ask(move |db, owner| {
            let Some((_, lines)) = db.playlist(owner, &name)? else { return Ok(None) };
            let tracks = db.playlist_tracks(owner, &name)?.unwrap_or_default();
            let tracks = tracks.into_iter().chain(std::iter::repeat(None));
            Ok(Some(lines.into_iter().zip(tracks).map(|(line, track)| track.and_then(|t| led_to(&cat, &line, &t)).unwrap_or(line)).collect()))
        });
        led.await?.ok_or(StoreError::Unknown)
    }

    /// A playlist's entries by their ids (decision 0087), and when each one there came (epoch s; none for one held
    /// before the dates were kept), in order.
    pub async fn entries_with_ids(&self, id: &str) -> Result<(Entries, Vec<Option<i64>>), StoreError> {
        if !valid_id(id) {
            return Err(StoreError::Unknown);
        }
        let name = id.to_string();
        self.ask(move |db, owner| db.playlist_entries(owner, &name)).await?.ok_or(StoreError::Unknown)
    }

    /// The tracks of a playlist's entries, in order (decision 0101 point 11): what a missing one was.
    pub async fn tracks(&self, id: &str) -> Result<Vec<Option<String>>, StoreError> {
        if !valid_id(id) {
            return Err(StoreError::Unknown);
        }
        let name = id.to_string();
        self.ask(move |db, owner| db.playlist_tracks(owner, &name)).await?.ok_or(StoreError::Unknown)
    }

    /// A playlist as its text (`text_of`), for its copies and its undo; `None` when there is no such playlist.
    pub async fn text(&self, id: &str) -> Option<String> {
        self.entries(id).await.ok().map(|lines| text_of(&lines))
    }

    /// When a playlist was made and last changed (epoch s), as Subsonic clients see it.
    pub fn dates(&self, id: &str) -> Option<(i64, i64)> {
        self.db.playlist(&self.owner(), id).ok().flatten().map(|(p, _)| (p.created, p.changed))
    }

    /// The playlist's lines become `lines` while it holds `expected` (`None`: whatever it holds), its copy written out.
    /// The caller holds the write lock. `None`: it no longer held `expected`, and nothing was written.
    async fn put(&self, id: &str, expected: Option<Vec<String>>, lines: Vec<String>) -> Result<Option<Edited>, StoreError> {
        let (name, l, cat) = (id.to_string(), lines.clone(), self.cat.clone());
        match self.ask(move |db, owner| db.write_playlist(owner, &name, expected.as_deref(), &l, &tracks_of(&cat, &l))).await? {
            Written::Unknown => Err(StoreError::Unknown),
            Written::Stale => Ok(None),
            Written::Done { before } => Ok(Some(self.written(id, before, lines).await)),
        }
    }

    /// A write done: its copy written out where it changed anything; the playlist before and after.
    async fn written(&self, id: &str, before: Vec<String>, after: Vec<String>) -> Edited {
        if before != after {
            self.write_out(id, Some(&after)).await;
        }
        Edited { id: id.to_string(), before: text_of(&before), after: text_of(&after) }
    }

    /// The playlist's lines as kept now; the caller holds the write lock.
    async fn lines(&self, id: &str) -> Result<Vec<String>, StoreError> {
        self.entries(id).await
    }

    /// Tells `on_named` that playlist `id` (its name) came (`true`) or went, shared or not, by its key.
    fn named(&self, id: &str, there: bool, shared: bool) {
        if let Some(told) = ON_NAMED.get() {
            told(&self.key(id), there, shared);
        }
    }

    /// Whether the playlist a key is of is shared, as it is now or as it went.
    async fn shared(&self, id: &str) -> bool {
        let key = self.key(id);
        self.ask(move |db, _| db.is_playlist_shared(&key)).await.unwrap_or(false)
    }

    /// Make the playlist exactly `entries`, in order. Returns false when it already was.
    /// Tests only: the sync uses `set_entries_if`, which never overwrites an edit it has not seen.
    #[cfg(test)]
    pub async fn set_entries(&self, id: &str, entries: &[String]) -> Result<bool, StoreError> {
        let _one_writer = self.write.lock().await;
        let e = self.put(id, None, entries.to_vec()).await?.ok_or(StoreError::Unknown)?;
        Ok(e.before != e.after)
    }

    /// As `set_entries`, but only when the playlist still holds `expected` (else nothing is written and false is
    /// returned): the sync must not overwrite an edit made while it was merging.
    pub async fn set_entries_if(&self, id: &str, expected: &[String], entries: &[String]) -> Result<bool, StoreError> {
        Ok(self.replace_entries_if(id, expected, entries).await?.is_some())
    }

    /// As `set_entries_if`, saying what the playlist was and became (`None`: it no longer held `expected`).
    pub async fn replace_entries_if(&self, id: &str, expected: &[String], entries: &[String]) -> Result<Option<Edited>, StoreError> {
        if !valid_id(id) {
            return Err(StoreError::Unknown);
        }
        let _one_writer = self.write.lock().await;
        self.put(id, Some(expected.to_vec()), entries.to_vec()).await
    }

    /// The playlist's entries become `e` (its operations applied, decision 0087), only while it holds `expected`;
    /// the operations' ids kept as `account`'s, a refused one with why (`ops`). `None`: it no longer held `expected`.
    pub async fn put_entries_if(
        &self,
        id: &str,
        expected: &[String],
        e: &Entries,
        account: &str,
        ops: &[(String, Option<String>)],
    ) -> Result<Option<Edited>, StoreError> {
        if !valid_id(id) {
            return Err(StoreError::Unknown);
        }
        let _one_writer = self.write.lock().await;
        let (name, expected, e, account, ops) = (id.to_string(), expected.to_vec(), e.clone(), account.to_string(), ops.to_vec());
        let (after, cat) = (e.lines(), self.cat.clone());
        let tracks = move |lines: &[String]| tracks_of(&cat, lines);
        match self.ask(move |db, owner| db.write_playlist_entries(owner, &name, &expected, &e, &account, &ops, &tracks(&e.lines()))).await? {
            Written::Unknown => Err(StoreError::Unknown),
            Written::Stale => Ok(None),
            Written::Done { before } => Ok(Some(self.written(id, before, after).await)),
        }
    }

    /// Put a playlist back to `text` (an undo, decision 0026), only while it is still what the change left (`left`,
    /// its print); `None` when it changed since.
    pub async fn put_back_if(&self, id: &str, left: u64, text: &str) -> Result<Option<Edited>, StoreError> {
        let _one_writer = self.write.lock().await;
        let now = self.lines(id).await?;
        if fingerprint(&text_of(&now)) != left {
            return Ok(None);
        }
        self.put(id, Some(now), m3u::entries(text).collect()).await
    }

    /// Delete a playlist (as `delete`) only while it is still what a change left (`left`); false when it changed since.
    pub async fn delete_if(&self, id: &str, left: u64) -> Result<bool, StoreError> {
        let _one_writer = self.write.lock().await;
        if fingerprint(&text_of(&self.lines(id).await?)) != left {
            return Ok(false);
        }
        self.delete_unlocked(id).await?;
        Ok(true)
    }

    /// Make `entry` a member of the playlist (`member`) or not. Idempotent: asking twice for the same state changes
    /// nothing the second time, so a repeated or retried request is harmless. Returns whether the playlist changed.
    pub async fn set_member(&self, id: &str, entry: &str, member: bool) -> Result<bool, StoreError> {
        let _one_writer = self.write.lock().await;
        let lines = self.lines(id).await?;
        let new: Vec<String> = match (lines.iter().any(|l| l == entry), member) {
            (false, true) => lines.iter().cloned().chain([entry.to_string()]).collect(),
            (true, false) => lines.iter().filter(|l| *l != entry).cloned().collect(),
            _ => return Ok(false),
        };
        self.put(id, Some(lines), new).await?;
        Ok(true)
    }

    /// Add `entry` to the playlist, or remove it if it is already there.
    pub async fn toggle(&self, id: &str, entry: &str) -> Result<Toggled, StoreError> {
        let _one_writer = self.write.lock().await;
        let lines = self.lines(id).await?;
        let added = !lines.iter().any(|l| l == entry);
        let new: Vec<String> =
            if added { lines.iter().cloned().chain([entry.to_string()]).collect() } else { lines.iter().filter(|l| *l != entry).cloned().collect() };
        self.put(id, Some(lines), new).await?;
        Ok(Toggled { added })
    }

    /// Dead entries that can be repaired, across all playlists: `safe` (same file name, other extension: the
    /// library holds the track in another format) and `review` (same track number, similar but different title).
    pub async fn repairs(&self) -> Result<Repairs> {
        let snapshot: Vec<(String, Vec<String>)> = self.all().await?.into_iter().map(|(p, lines)| (p.name, lines)).collect();
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
    /// with paths). One write per playlist. Returns the playlists that changed, each as it was and is.
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
        let lines = self.entries(&r.playlist).await?;
        let dead = lines.iter().any(|l| *l == r.from) && !entry_exists(&self.base, &r.from);
        if !dead || !entry_exists(&self.base, &r.to) {
            return Err(StoreError::NotOffered);
        }
        self.write_repairs(std::slice::from_ref(r)).await
    }

    /// Repairs written: one write per playlist.
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
            let before = self.lines(&id).await?;
            let mut lines = before.clone();
            for r in repairs {
                // A dead line pointed at a file the playlist already lists would become a repeat nobody asked for:
                // it goes instead (decision 0025, amended 2026-10-16: repeats only when asked for).
                if lines.contains(&r.to) {
                    lines.retain(|l| *l != r.from);
                } else {
                    for l in lines.iter_mut().filter(|l| **l == r.from) {
                        *l = r.to.clone();
                    }
                }
            }
            if lines != before
                && let Some(e) = self.put(&id, Some(before), lines).await?
            {
                changed.push(e);
            }
        }
        Ok(changed)
    }

    /// Give every playlist the entries `edit` returns for it (`None`: leave it as it is). Returns the playlists that
    /// changed.
    pub async fn edit_all(&self, edit: impl Fn(&str, &[String]) -> Option<Vec<String>>) -> Result<Vec<String>> {
        let _one_writer = self.write.lock().await;
        let mut changed = Vec::new();
        for (p, lines) in self.all().await? {
            let Some(new) = edit(&p.name, &lines) else { continue };
            if new == lines {
                continue;
            }
            match self.put(&p.name, Some(lines), new).await {
                Ok(Some(_)) => changed.push(p.name),
                // Gone or changed since the listing.
                Ok(None) | Err(StoreError::Unknown) => {}
                Err(e) => tracing::warn!("{}: {e:#}", p.name),
            }
        }
        Ok(changed)
    }

    /// Playlist lines that point into the album folder `folder`, per playlist (playlists without any are left out).
    pub async fn lines_in_folder(&self, folder: &str) -> Result<Vec<(String, usize)>> {
        let prefix = format!("{folder}/");
        let mut out: Vec<(String, usize)> = self
            .all()
            .await?
            .into_iter()
            .filter_map(|(p, lines)| {
                let n = lines.iter().filter(|l| l.starts_with(&prefix)).count();
                (n > 0).then_some((p.name, n))
            })
            .collect();
        out.sort_by_key(|(id, _)| id.to_lowercase());
        Ok(out)
    }

    /// Every playlist's lines and missing-track count, by name (for Insights).
    pub async fn snapshot(&self) -> Result<Vec<(String, Vec<String>, usize)>> {
        let lists = self.all().await?;
        let dead = self.dead_of(&lists).await;
        let mut out: Vec<(String, Vec<String>, usize)> =
            lists.into_iter().map(|(p, lines)| (p.name, lines, dead.get(&p.id).copied().unwrap_or(0))).collect();
        out.sort_by_key(|(id, _, _)| id.to_lowercase());
        Ok(out)
    }

    /// Take the repeats out of one playlist, the first copy of each track staying: the user's "Remove repeats"
    /// (decision 0025, amended 2026-10-16). `None` when it held none.
    pub async fn remove_repeats(&self, id: &str) -> Result<Option<Edited>, StoreError> {
        let _one_writer = self.write.lock().await;
        let lines = self.lines(id).await?;
        let mut seen = HashSet::new();
        let deduped: Vec<String> = lines.iter().filter(|l| seen.insert(l.as_str())).cloned().collect();
        if deduped.len() == lines.len() {
            return Ok(None);
        }
        self.put(id, Some(lines), deduped).await
    }

    /// Add `entries` at the playlist's end, in order, whether or not it holds them already: the user chose Add again
    /// (decision 0025, amended 2026-10-16). Membership asks use `set_member`, which never makes a repeat.
    pub async fn append(&self, id: &str, entries: &[String]) -> Result<Edited, StoreError> {
        let _one_writer = self.write.lock().await;
        let lines = self.lines(id).await?;
        if entries.is_empty() {
            let text = text_of(&lines);
            return Ok(Edited { id: id.to_string(), before: text.clone(), after: text });
        }
        let new: Vec<String> = lines.iter().chain(entries).cloned().collect();
        self.put(id, Some(lines), new).await?.ok_or(StoreError::Unknown)
    }

    /// Create an empty playlist. Never takes the name of another (whatever the case).
    pub async fn create(&self, name: &str) -> Result<(), StoreError> {
        if !valid_new_name(name) {
            return Err(StoreError::BadName);
        }
        let _one_writer = self.write.lock().await;
        let n = name.to_string();
        if self.ask(move |db, owner| db.create_playlist(owner, &n, &[], &Tracks::new())).await?.is_none() {
            return Err(StoreError::Exists);
        }
        self.write_out(name, Some(&[])).await;
        self.named(name, true, false);
        Ok(())
    }

    /// Rename a playlist: it keeps all else. Never takes the name of another.
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), StoreError> {
        if !valid_id(from) {
            return Err(StoreError::Unknown);
        }
        if !valid_new_name(to) {
            return Err(StoreError::BadName);
        }
        let _one_writer = self.write.lock().await;
        let (f, t) = (from.to_string(), to.to_string());
        let lines = match self.ask(move |db, owner| db.rename_playlist(owner, &f, &t)).await? {
            Named::Unknown => return Err(StoreError::Unknown),
            Named::Taken => return Err(StoreError::Exists),
            Named::Done(lines) => lines,
        };
        self.write_out(from, None).await;
        self.write_out(to, Some(&lines)).await;
        // Its id stays: to everyone who sees it, it changed.
        let shared = self.shared(to).await;
        self.named(to, true, shared);
        Ok(())
    }

    /// Delete a playlist. It is set aside, never destroyed, so `restore_deleted` can bring it back.
    pub async fn delete(&self, id: &str) -> Result<(), StoreError> {
        if !valid_id(id) {
            return Err(StoreError::Unknown);
        }
        let _one_writer = self.write.lock().await;
        self.delete_unlocked(id).await
    }

    /// `delete`; the caller holds the write lock.
    async fn delete_unlocked(&self, id: &str) -> Result<(), StoreError> {
        let name = id.to_string();
        let Some((_, shared)) = self.ask(move |db, owner| db.delete_playlist(owner, &name)).await? else { return Err(StoreError::Unknown) };
        self.write_out(id, None).await;
        self.named(id, false, shared);
        Ok(())
    }

    /// Bring back the playlist of that name last deleted (refuses if one has the name again). Returns its text.
    pub async fn restore_deleted(&self, id: &str) -> Result<String, StoreError> {
        if !valid_id(id) {
            return Err(StoreError::Unknown);
        }
        let _one_writer = self.write.lock().await;
        let name = id.to_string();
        let lines = match self.ask(move |db, owner| db.restore_playlist(owner, &name)).await? {
            Named::Unknown => return Err(StoreError::NothingToUndo),
            Named::Taken => return Err(StoreError::Exists),
            Named::Done(lines) => lines,
        };
        self.write_out(id, Some(&lines)).await;
        let shared = self.shared(id).await;
        self.named(id, true, shared);
        Ok(text_of(&lines))
    }

    /// The playlist's `.m3u` copy written out as `lines` (decision 0101: a readable copy beside the database's backup,
    /// never read back), or taken away (`None`). A temporary file renamed into place, so a copy is never half written;
    /// one that cannot be written costs only the copy.
    async fn write_out(&self, name: &str, lines: Option<&[String]>) {
        let Some(out) = &self.out else { return };
        let dir = match &self.account {
            Some(a) => out.join(a),
            None => out.clone(),
        };
        let (name, text) = (name.to_string(), lines.map(text_of));
        let done = tokio::task::spawn_blocking(move || -> Result<()> {
            let path = dir.join(format!("{name}.m3u"));
            let Some(text) = text else {
                return match std::fs::remove_file(&path) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
                    _ => Ok(()),
                };
            };
            std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
            let tmp = dir.join(format!(".{name}.m3u.cratarr-tmp"));
            std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
            std::fs::rename(&tmp, &path).with_context(|| format!("writing {}", path.display()))?;
            Ok(())
        })
        .await;
        match done {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!("a playlist's copy not written: {e:#}"),
            Err(e) => tracing::warn!("a playlist's copy not written: {e}"),
        }
    }
}

/// The playlists' `.m3u` files as 0.15 kept them (decision 0014): in Music's playlist folder (`.playlists`), their lines
/// carrying "../", another account's in a folder of its own there, its lines carrying "../../"; without that folder, in
/// Music itself. Each as (owner, name, lines, when its file last changed in epoch s), backup copies and hidden files
/// left out. Blocking.
fn files_of(music: &Path) -> Result<Vec<(String, String, Vec<String>, i64)>> {
    let folder = music.join(PLAYLIST_DIR);
    let mut out = Vec::new();
    if !folder.is_dir() {
        read_folder(music, "", "", &mut out)?;
        return Ok(out);
    }
    read_folder(&folder, "", "../", &mut out)?;
    let mut accounts: Vec<(String, PathBuf)> = std::fs::read_dir(&folder)?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path())))
        .filter(|(a, _)| account_folder_ok(a))
        .collect();
    accounts.sort();
    for (account, dir) in accounts {
        read_folder(&dir, &account, "../../", &mut out)?;
    }
    Ok(out)
}

/// The playlists of one folder, `owner`'s, their lines with `prefix` taken off (`files_of`).
fn read_folder(dir: &Path, owner: &str, prefix: &str, out: &mut Vec<(String, String, Vec<String>, i64)>) -> Result<()> {
    let mut found: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .with_context(|| format!("cannot read the playlist folder {}", dir.display()))?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path())))
        .filter(|(n, _)| !n.starts_with('.') && n.to_lowercase().ends_with(".m3u"))
        .map(|(n, p)| (n[..n.len() - 4].to_string(), p))
        .filter(|(id, _)| !is_backup_copy(id) && valid_id(id))
        .collect();
    found.sort();
    for (name, path) in found {
        let Ok(bytes) = std::fs::read(&path) else {
            tracing::warn!("cannot read {}; left out", path.display());
            continue;
        };
        let stamp = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64);
        let lines = m3u::entries(&String::from_utf8_lossy(&bytes)).map(|l| unprefixed(&l, prefix)).collect();
        out.push((owner.to_string(), name, lines, stamp));
    }
    Ok(())
}

/// The tracks `lines` name now (decision 0101 point 11), each line once. Blocking.
fn tracks_of(cat: &Catalogue, lines: &[String]) -> Tracks {
    let mut out = Tracks::new();
    for line in lines {
        if !out.contains_key(line) {
            out.insert(line.clone(), cat.track_of_line(line));
        }
    }
    out
}

/// The playlists' `.m3u` files brought into the database (0.16, decision 0101), once: the first time the Music folder
/// is known, each line as the track it names. Each is then written out as its copy into `out`. The files stay where
/// they are, never read again. Blocking.
pub fn import(db: &UserData, cat: &Catalogue, music: &Path, out: Option<&Path>) -> Result<()> {
    // Not there yet (a share not mounted): brought in once it is, never as if it held none.
    if db.playlists_imported()? || !music.is_dir() {
        return Ok(());
    }
    let found = files_of(music)?;
    let lines: Vec<String> = found.iter().flat_map(|(_, _, lines, _)| lines.iter().cloned()).collect();
    let came = db.import_playlists(&found, &tracks_of(cat, &lines))?;
    tracing::info!("{came} playlist(s) brought into the database from {}", music.display());
    let Some(out) = out else { return Ok(()) };
    for owner in db.playlist_owners()? {
        let dir = if owner.is_empty() { out.to_path_buf() } else { out.join(&owner) };
        for (p, lines) in db.playlists(&owner)? {
            std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
            std::fs::write(dir.join(format!("{}.m3u", p.name)), text_of(&lines)).with_context(|| format!("writing the copy of {}", p.name))?;
        }
    }
    Ok(())
}

/// The playlists Music itself holds, by name (the media server's copies, decision 0014), backup copies left out.
/// Blocking.
pub fn music_playlists(music: &Path) -> Result<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(music)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| !n.starts_with('.') && n.to_lowercase().ends_with(".m3u") && !is_backup_copy(&n[..n.len() - 4]))
        .map(|n| n[..n.len() - 4].to_string())
        .collect();
    names.sort();
    Ok(names)
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
        let s = d.store();
        let v = s.view(Some("A/1.mp3")).await.unwrap();
        // Each by its own id (decision 0101), name-sorted.
        assert_eq!(
            v,
            vec![
                PlaylistView { id: s.key("Analogic"), name: "Analogic".into(), member: false, count: 1, dead: 0, links: Vec::new(), spoken: Vec::new(), owner: String::new(), shared: false, mine: true },
                PlaylistView { id: s.key("chill"), name: "chill".into(), member: true, count: 2, dead: 0, links: Vec::new(), spoken: Vec::new(), owner: String::new(), shared: false, mine: true },
            ]
        );
        assert!(v.iter().all(|p| crate::userdata::playlists::is_id(&p.id)));
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
    async fn the_files_of_0_15_are_brought_in_once_and_left_where_they_are() {
        let d = Dir::new();
        d.touch("A/1.mp3");
        std::fs::create_dir_all(d.0.join(".playlists/ann")).unwrap();
        d.write(".playlists/Chill.m3u", "#EXTM3U\n../A/1.mp3\n../gone/2.mp3\n");
        d.write(".playlists/Chill.bak.m3u", "../A/1.mp3\n");
        d.write(".playlists/ann/Evening.m3u", "../../A/1.mp3\n");
        d.write("Chill.m3u", "A/1.mp3\n");
        let (db, out) = (Arc::new(UserData::open(None).unwrap()), d.0.join("copies"));
        let cat = Arc::new(Catalogue::open(None, vec![d.0.clone()]).unwrap());
        let s = Store::in_music(db.clone(), cat.clone(), &d.0, Some(out.clone())).await.unwrap();
        let v = s.view(Some("A/1.mp3")).await.unwrap();
        assert_eq!(v.iter().map(|p| (p.name.as_str(), p.member, p.count, p.dead)).collect::<Vec<_>>(), [("Chill", true, 1, 1)]);
        assert_eq!(s.entries("Chill").await.unwrap(), ["A/1.mp3", "gone/2.mp3"], "Music-relative, as before");
        let ann = Store::open(db.clone(), cat.clone(), Some("ann"), &d.0, Some(out.clone()));
        assert_eq!(ann.entries("Evening").await.unwrap(), ["A/1.mp3"], "another account's, from its folder");
        // Written out as copies; the files stay as they were and are never read again.
        assert_eq!(std::fs::read_to_string(out.join("Chill.m3u")).unwrap(), "A/1.mp3\ngone/2.mp3\n");
        assert_eq!(std::fs::read_to_string(out.join("ann/Evening.m3u")).unwrap(), "A/1.mp3\n");
        s.set_member("Chill", "B/3.mp3", true).await.unwrap();
        assert_eq!(std::fs::read_to_string(out.join("Chill.m3u")).unwrap(), "A/1.mp3\ngone/2.mp3\nB/3.mp3\n");
        assert_eq!(d.read(".playlists/Chill.m3u"), "#EXTM3U\n../A/1.mp3\n../gone/2.mp3\n");
        d.write(".playlists/Chill.m3u", "../B/9.mp3\n");
        let again = Store::in_music(db, cat, &d.0, Some(out)).await.unwrap();
        assert_eq!(again.entries("Chill").await.unwrap(), ["A/1.mp3", "gone/2.mp3", "B/3.mp3"], "brought in once");
        assert_eq!(music_playlists(&d.0).unwrap(), ["Chill"], "Music's own copies, by name");
    }

    #[tokio::test]
    async fn an_entry_written_before_the_catalogue_read_its_file_gets_its_track_in_the_minute_pass() {
        let d = Dir::new();
        d.touch("A/1.mp3");
        let cat = Arc::new(Catalogue::open(None, vec![d.0.clone()]).unwrap());
        let db = Arc::new(UserData::open(None).unwrap());
        let s = Store::open(db.clone(), cat.clone(), None, &d.0, None);
        s.create("Mix").await.unwrap();
        s.set_member("Mix", "A/1.mp3", true).await.unwrap();
        assert!(db.tracked_entries("").unwrap().is_empty(), "the catalogue has not read its file yet");
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        s.refresh_dead().await;
        let track = cat.track_of_line("A/1.mp3").expect("the record's track");
        assert_eq!(db.tracked_entries("").unwrap().into_iter().map(|(_, _, l, t)| (l, t)).collect::<Vec<_>>(), [("A/1.mp3".to_string(), track)]);
        assert_eq!(s.entries("Mix").await.unwrap(), ["A/1.mp3"], "its line as it was");
    }

    #[tokio::test]
    async fn each_entry_is_the_track_its_line_names_and_stays_it_when_its_file_goes() {
        let d = Dir::new();
        d.touch("A/1.mp3");
        let cat = Arc::new(Catalogue::open(None, vec![d.0.clone()]).unwrap());
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        let track = cat.track_of_line("A/1.mp3").expect("the record's track");
        let db = Arc::new(UserData::open(None).unwrap());
        let s = Store::open(db.clone(), cat, None, &d.0, None);
        s.create("Mix").await.unwrap();
        s.set_member("Mix", "A/1.mp3", true).await.unwrap();
        s.set_member("Mix", "Gone/2.mp3", true).await.unwrap();
        let tracked = |db: &UserData| db.tracked_entries("").unwrap().into_iter().map(|(_, _, l, t)| (l, t)).collect::<Vec<_>>();
        assert_eq!(tracked(&db), [("A/1.mp3".to_string(), track.clone())], "a line no file is at is no track's");
        // Its file gone: the entry stays that track's, missing (decision 0101 point 11).
        std::fs::remove_file(d.0.join("A/1.mp3")).unwrap();
        s.set_member("Mix", "C/3.mp3", true).await.unwrap();
        s.refresh_dead().await;
        assert_eq!(tracked(&db), [("A/1.mp3".to_string(), track)]);
        assert_eq!(s.view(None).await.unwrap()[0].dead, 3);
    }

    /// A line in another library plays as any other: read as dead, a list of five that all played said one track and four dead.
    #[tokio::test]
    async fn only_a_line_with_no_file_is_dead() {
        let d = Dir::new();
        d.touch("Music/A/1.mp3");
        d.touch("own/A/2.mp3");
        let cat = Arc::new(Catalogue::open(None, vec![d.0.join("Music"), d.0.join("own")]).unwrap());
        let s = Store::open(Arc::new(UserData::open(None).unwrap()), cat, None, d.0.join("Music"), None);
        s.create("Mix").await.unwrap();
        for line in ["A/1.mp3", "../own/A/2.mp3", "../own/A/gone.mp3"] {
            s.set_member("Mix", line, true).await.unwrap();
        }
        let listed = s.view(None).await.unwrap().remove(0);
        assert_eq!((listed.count, listed.dead), (2, 1), "when it is written");
        s.refresh_dead().await;
        let listed = s.view(None).await.unwrap().remove(0);
        assert_eq!((listed.count, listed.dead), (2, 1), "and at the minute's pass");
    }

    #[tokio::test]
    async fn deleting_a_copy_leaves_its_lines_playing() {
        let d = Dir::new();
        d.touch("Music/A/1.mp3");
        d.touch("own/A/1.mp3");
        let cat = Arc::new(Catalogue::open(None, vec![d.0.join("Music"), d.0.join("own")]).unwrap());
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        let track = cat.track_of(0, "A/1.mp3").unwrap();
        cat.give_track_for_tests(1, "A/1.mp3", track);
        let s = Store::open(Arc::new(UserData::open(None).unwrap()), cat.clone(), None, d.0.join("Music"), None);
        s.create("Mix").await.unwrap();
        s.set_member("Mix", "../own/A/1.mp3", true).await.unwrap();
        s.refresh_dead().await;
        assert_eq!(s.entries("Mix").await.unwrap(), ["../own/A/1.mp3"], "a line on a copy that is there stays on it");
        assert_eq!(s.led_entries("Mix").await.unwrap(), ["../own/A/1.mp3"]);
        // That copy goes. Its line reads the other at once, and plays: nothing waits for the minute's pass.
        std::fs::remove_file(d.0.join("own/A/1.mp3")).unwrap();
        cat.walk_all();
        let led = s.led_entries("Mix").await.unwrap();
        assert_eq!(led, ["A/1.mp3"]);
        let song = crate::serve::ids::song(track);
        assert_eq!(cat.line_track_ids(crate::source::cratarr::KIND.id, &led).unwrap(), [Some(song)]);
        assert!(line_exists(&cat, &led[0]), "not missing");
        assert_eq!(s.entries("Mix").await.unwrap(), ["../own/A/1.mp3"], "reading writes nothing");
        s.refresh_dead().await;
        assert_eq!(s.entries("Mix").await.unwrap(), ["A/1.mp3"], "the pass writes where it read");
    }

    /// A copy that left its record (Q8) took tracks of its own, so a line on its file no longer holds the entry's track: an
    /// entry is its track, and it goes to the copy that stayed.
    #[tokio::test]
    async fn an_entry_on_a_copy_that_left_its_record_goes_to_its_track() {
        let d = Dir::new();
        d.touch("Music/A/1.mp3");
        d.touch("own/A/1.mp3");
        let cat = Arc::new(Catalogue::open(None, vec![d.0.join("Music"), d.0.join("own")]).unwrap());
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        let (track, own) = (cat.track_of(0, "A/1.mp3").unwrap(), cat.track_of(1, "A/1.mp3").unwrap());
        cat.give_track_for_tests(1, "A/1.mp3", track);
        let s = Store::open(Arc::new(UserData::open(None).unwrap()), cat.clone(), None, d.0.join("Music"), None);
        s.create("Mix").await.unwrap();
        s.set_member("Mix", "../own/A/1.mp3", true).await.unwrap();
        s.refresh_dead().await;
        assert_eq!(s.entries("Mix").await.unwrap(), ["../own/A/1.mp3"], "a line on a copy holding its track stays on it");
        cat.give_track_for_tests(1, "A/1.mp3", own);
        assert_eq!(s.led_entries("Mix").await.unwrap(), ["A/1.mp3"]);
        s.refresh_dead().await;
        assert_eq!(s.entries("Mix").await.unwrap(), ["A/1.mp3"]);
    }

    #[tokio::test]
    async fn a_music_folder_not_there_yet_brings_nothing_in_until_it_is() {
        let d = Dir::new();
        let db = Arc::new(UserData::open(None).unwrap());
        let cat = Arc::new(Catalogue::open(None, vec![d.0.clone()]).unwrap());
        Store::in_music(db.clone(), cat.clone(), d.0.join("not mounted"), None).await.unwrap();
        assert!(!db.playlists_imported().unwrap(), "never as if it held none");
        d.write("Chill.m3u", "A/1.mp3\n");
        let s = Store::in_music(db.clone(), cat, &d.0, None).await.unwrap();
        assert_eq!(s.entries("Chill").await.unwrap(), ["A/1.mp3"]);
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
    async fn a_copy_edited_elsewhere_is_never_read_back() {
        let d = Dir::new();
        d.write("a.m3u", "A/1.mp3\n");
        let s = d.store();
        d.write("a.m3u", "A/1.mp3\nB/2.mp3\n");
        assert!(!s.view(Some("B/2.mp3")).await.unwrap()[0].member);
        s.toggle("a", "C/3.mp3").await.unwrap();
        assert_eq!(d.read("a.m3u"), "A/1.mp3\nC/3.mp3\n", "the playlist as kept, written out again");
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
    async fn applying_a_repair_rewrites_only_that_line() {
        let d = Dir::new();
        d.touch("Alb (2020)/01 A - One.flac");
        d.touch("Alb (2020)/02 A - Two.mp3");
        d.write("p.m3u", "#c\nAlb (2020)/01 A - One.mp3\r\nAlb (2020)/02 A - Two.mp3\r\n");
        let s = d.store();
        let r = s.repairs().await.unwrap();
        let changed = s.apply_repairs(&r.safe).await.unwrap();
        assert_eq!(changed.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["p"]);
        assert_eq!(changed[0].before, "Alb (2020)/01 A - One.mp3\nAlb (2020)/02 A - Two.mp3\n", "what undo puts back");
        assert_eq!(d.read("p.m3u"), "Alb (2020)/01 A - One.flac\nAlb (2020)/02 A - Two.mp3\n");
        let v = s.view(None).await.unwrap();
        assert_eq!((v[0].count, v[0].dead), (2, 0));
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
    async fn a_new_playlist_is_created_empty_and_never_takes_anothers_name() {
        let d = Dir::new();
        d.write("old.m3u", "A/1.mp3\n");
        let s = d.store();
        s.create("Late Night").await.unwrap();
        assert_eq!(d.read("Late Night.m3u"), "");
        assert!(s.view(None).await.unwrap().iter().any(|p| p.name == "Late Night"));
        assert!(matches!(s.create("old").await, Err(StoreError::Exists)));
        assert!(matches!(s.create("OLD").await, Err(StoreError::Exists)), "whatever the case");
        assert_eq!(s.entries("old").await.unwrap(), ["A/1.mp3"]);
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
        let names: Vec<String> = s.view(None).await.unwrap().into_iter().map(|p| p.name).collect();
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
}
