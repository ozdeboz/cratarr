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
use crate::playlists::{Edited, PlaylistView, Repair, Repairs, Store};
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
    /// The lines of the viewer's favourite albums (decision 0024, amended 2026-10-19), when asked for.
    favourite_albums: Option<HashSet<String>>,
}

impl CrateLibrary {
    /// Whether these conditions can be worked out: what they need of the favourites is known.
    fn knows(&self, conditions: &[crates::Condition]) -> bool {
        (!crates::needs_favourites(conditions) || self.favourites.is_some())
            && (!crates::needs_favourite_albums(conditions) || self.favourite_albums.is_some())
    }

    fn select(&self, conditions: &[crates::Condition]) -> Vec<String> {
        let w = crates::World { membership: &self.membership, favourites: self.favourites.as_ref(), favourite_albums: self.favourite_albums.as_ref() };
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
    /// Its favourite albums toggle's setting (decision 0024, amended 2026-10-19); `counts` are counted with it.
    pub favourite_albums: crates::Favourites,
    /// How many of its tracks were left out by hand (decision 0024, amended 2026-10-19); `counts` leave them out.
    pub left_out: usize,
    /// How many tracks it has with each setting of the toggle (the page shows the one it is set to, and the next at
    /// once on a tap); `None` where the favourites are not known (no server connected).
    pub counts: CrateCounts,
    pub permanent: bool,
    /// What its card shows, as its favourites toggle is set; `None` where that is not known (favourites not read).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<CrateStats>,
    /// A picture of its own was set (`GET /api/crates/{id}/image`).
    pub image: bool,
}

/// A crate's card (the owner's, 2026-10-16): its size and length, how much of it was never played, what came this week,
/// its top genres and a few of its albums' covers.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct CrateStats {
    pub tracks: usize,
    pub seconds: u64,
    /// Never played by the owner (Cratarr's own plays); `None` while there is no owner to ask about.
    pub unplayed: Option<usize>,
    /// Came in the last seven days.
    pub new_week: usize,
    /// Its three most common genres, most common first.
    pub genres: Vec<String>,
    /// Up to four of its albums, for a mosaic: those with most of its tracks first, then the newest.
    pub covers: Vec<CoverRef>,
}

/// A playlist's facts for the Playlists page's table (decision 0042): how long, its top genres, how much of it the
/// owner never played and when they last played any of it, and when its file last changed.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct PlaylistStats {
    pub seconds: u64,
    /// Its three most common genres (its tracks' albums'), most common first.
    pub genres: Vec<String>,
    /// Never played by the owner (Cratarr's own plays); `None` while there is no owner to ask about.
    pub unplayed: Option<usize>,
    /// The latest play of any of its tracks (epoch seconds).
    pub last_played: Option<i64>,
    /// When its file last changed (epoch seconds).
    pub changed: i64,
}

/// A playlist's facts from its lines (library-relative), the Music folder's songs and the owner's plays.
fn playlist_stats(
    lines: &[String],
    songs: &HashMap<String, crate::serve::library::RelSong>,
    plays: Option<&HashMap<String, (u32, Option<i64>)>>,
    changed: i64,
) -> PlaylistStats {
    let known: Vec<&crate::serve::library::RelSong> = lines.iter().filter_map(|l| songs.get(l)).collect();
    PlaylistStats {
        seconds: known.iter().filter_map(|s| s.duration_ms).filter(|d| *d > 0).map(|d| d as u64).sum::<u64>() / 1000,
        genres: crates::top_genres(known.iter().map(|s| s.genres.as_slice()), 3),
        unplayed: plays.map(|p| known.iter().filter(|s| !p.contains_key(&s.song)).count()),
        last_played: plays.and_then(|p| known.iter().filter_map(|s| p.get(&s.song).and_then(|(_, at)| *at)).max()),
        changed,
    }
}

/// The playlists' facts as last worked out, and what they were worked out from (the playlists' files, the library's
/// build, the owner's plays): the table asks again whenever the list changes, and the answer is kept until one of
/// those does.
type StatsKept = std::sync::Mutex<Option<(String, std::collections::BTreeMap<String, PlaylistStats>)>>;

/// An album's cover, as `/api/cover/{source}/{album_id}` serves it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CoverRef {
    pub source: String,
    pub album_id: String,
}

/// A crate's picture's file name: its id where that is a plain name, else the id in hex.
fn image_file(id: &str) -> String {
    if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        format!("{id}.jpg")
    } else {
        format!("x{}.jpg", id.bytes().map(|b| format!("{b:02x}")).collect::<String>())
    }
}

/// A crate's card from its lines (`now`: epoch seconds).
fn crate_stats(
    lines: &[String],
    facts: &HashMap<&str, &crates::Facts>,
    songs: &HashMap<String, crate::serve::library::RelSong>,
    played: Option<&HashSet<String>>,
    now: i64,
) -> CrateStats {
    const WEEK: i64 = 7 * 86_400;
    // What Cratarr's first look at the library found was there before it: not new, however young Cratarr is.
    let first_look = songs.values().filter_map(|s| s.created).min().unwrap_or(0) + 86_400;
    let known: Vec<&crate::serve::library::RelSong> = lines.iter().filter_map(|l| songs.get(l)).collect();
    let mut genres: Vec<(String, usize)> = Vec::new();
    for f in lines.iter().filter_map(|l| facts.get(l.as_str())) {
        for g in &f.genres {
            match genres.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(g)) {
                Some((_, n)) => *n += 1,
                None => genres.push((g.clone(), 1)),
            }
        }
    }
    genres.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    // Albums: (album, its tracks here, its newest).
    let mut albums: Vec<(&str, usize, i64)> = Vec::new();
    for s in &known {
        let at = s.created.unwrap_or(0);
        match albums.iter_mut().find(|(a, _, _)| *a == s.album) {
            Some((_, n, newest)) => {
                *n += 1;
                *newest = (*newest).max(at);
            }
            None => albums.push((&s.album, 1, at)),
        }
    }
    albums.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)).then(a.0.cmp(b.0)));
    CrateStats {
        tracks: lines.len(),
        seconds: known.iter().filter_map(|s| s.duration_ms).filter(|d| *d > 0).map(|d| d as u64).sum::<u64>() / 1000,
        unplayed: played.map(|p| known.iter().filter(|s| !p.contains(&s.song)).count()),
        new_week: known.iter().filter(|s| s.created.is_some_and(|c| c >= now - WEEK && c > first_look)).count(),
        genres: genres.into_iter().take(3).map(|(g, _)| g).collect(),
        covers: albums
            .into_iter()
            .take(4)
            .map(|(a, _, _)| CoverRef { source: crate::source::cratarr::KIND.id.to_string(), album_id: a.to_string() })
            .collect(),
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CrateCounts {
    pub all: Option<usize>,
    pub only: Option<usize>,
    pub none: Option<usize>,
}

/// One line of a playlist as its view shows it (decision 0023). `entry` is the line as the `.m3u` has it (library-
/// relative), `file` its file name; the rest is what the catalogue knows, `None` where it does not.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct PlaylistRow {
    pub entry: String,
    pub file: String,
    /// The server's track id, when the file is matched to one.
    pub id: Option<String>,
    pub name: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_secs: Option<f64>,
    /// The panel's orders (decision 0025's 2026-10-16 amendment): from the file's tags,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    /// its tempo (the BPM tag, else the analysis', decision 0035),
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bpm: Option<u32>,
    /// the user's plays of the track and when it was last played,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_played: Option<i64>,
    /// and when the entry came into the playlist (none for one held before Cratarr kept that).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<i64>,
    /// Its album's id where it is known: an artist's rows, and a playlist's or a crate's on Cratarr's own source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_id: Option<String>,
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
mod card_tests {
    use super::*;
    use crate::serve::library::RelSong;

    #[test]
    fn a_crates_card_counts_its_lines() {
        let now = 1_000_000_000;
        let facts = [
            crates::Facts { rel: "A/1.flac".into(), genres: vec!["Dub".into(), "Reggae".into()], year: None, bpm: None },
            crates::Facts { rel: "A/2.flac".into(), genres: vec!["dub".into()], year: None, bpm: None },
            crates::Facts { rel: "B/1.flac".into(), genres: vec!["Jazz".into()], year: None, bpm: None },
        ];
        let by: HashMap<&str, &crates::Facts> = facts.iter().map(|f| (f.rel.as_str(), f)).collect();
        let song = |id: &str, album: &str, ms: i64, created: i64| RelSong { song: id.into(), album: album.into(), duration_ms: Some(ms), created: Some(created), genres: Vec::new() };
        let songs: HashMap<String, RelSong> = [
            ("old.flac".to_string(), song("tr-0", "al-z", 1_000, now - 300 * 86_400)),
            ("A/1.flac".to_string(), song("tr-1", "al-a", 200_000, now - 30 * 86_400)),
            ("A/2.flac".to_string(), song("tr-2", "al-a", 100_500, now - 30 * 86_400)),
            ("B/1.flac".to_string(), song("tr-3", "al-b", 60_000, now - 86_400)),
        ]
        .into_iter()
        .collect();
        let played: HashSet<String> = ["tr-1".to_string()].into_iter().collect();
        let lines: Vec<String> = ["A/1.flac", "A/2.flac", "B/1.flac", "gone.flac"].iter().map(|s| s.to_string()).collect();
        let s = crate_stats(&lines, &by, &songs, Some(&played), now);
        assert_eq!((s.tracks, s.seconds, s.unplayed, s.new_week), (4, 360, Some(2), 1));
        assert_eq!(s.genres, vec!["Dub", "Jazz", "Reggae"], "spelt as first met, counted regardless of capitals");
        assert_eq!(s.covers.iter().map(|c| c.album_id.as_str()).collect::<Vec<_>>(), vec!["al-a", "al-b"]);
        assert_eq!(crate_stats(&lines, &by, &songs, None, now).unplayed, None, "no owner: not known");
        assert_eq!(image_file("c-12ab"), "c-12ab.jpg");
        assert_eq!(image_file("../x"), "x2e2e2f78.jpg");
    }

    #[test]
    fn a_playlists_facts_count_its_lines() {
        let song = |id: &str, ms: i64, genres: &[&str]| RelSong {
            song: id.into(),
            album: "al".into(),
            duration_ms: Some(ms),
            created: None,
            genres: genres.iter().map(|g| g.to_string()).collect(),
        };
        let songs: HashMap<String, RelSong> = [
            ("A/1.flac".to_string(), song("tr-1", 200_000, &["Dub", "Reggae"])),
            ("A/2.flac".to_string(), song("tr-2", 100_500, &["dub"])),
            ("B/1.flac".to_string(), song("tr-3", 60_000, &["Jazz"])),
        ]
        .into_iter()
        .collect();
        let plays: HashMap<String, (u32, Option<i64>)> = [("tr-1".to_string(), (3, Some(500))), ("tr-3".to_string(), (1, Some(900)))].into_iter().collect();
        let lines: Vec<String> = ["A/1.flac", "A/2.flac", "B/1.flac", "gone.flac"].iter().map(|s| s.to_string()).collect();
        let s = playlist_stats(&lines, &songs, Some(&plays), 42);
        assert_eq!((s.seconds, s.unplayed, s.last_played, s.changed), (360, Some(1), Some(900), 42));
        assert_eq!(s.genres, vec!["Dub", "Jazz", "Reggae"]);
        let none = playlist_stats(&lines, &songs, None, 42);
        assert_eq!((none.unplayed, none.last_played), (None, None), "no owner: not known");
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
    async fn each_account_has_its_own_playlists_in_a_folder_of_its_own() {
        let (d, s) = service().await;
        let ann = s.lib.store_for(Some("ann")).unwrap();
        assert!(ann.view(None).await.unwrap().is_empty(), "none until her first");
        ann.create("Chill").await.unwrap();
        ann.replace_entries_if("Chill", &[], &["A/1.mp3".to_string()]).await.unwrap();
        assert_eq!(std::fs::read_to_string(d.join("ann/Chill.m3u")).unwrap(), "../../A/1.mp3
", "Music-relative, from her folder");
        // The first administrator's are as they were: hers is not among them, and one of the same name is another.
        s.create("Chill").await.unwrap();
        let mine: Vec<(String, bool)> = s.view(None).await.unwrap().into_iter().map(|p| (p.id, p.mine)).collect();
        assert_eq!(mine, [("Chill".to_string(), true)]);
        assert!(s.entries("Chill").await.unwrap().is_empty());
        // Keys tell them apart in Cratarr's own records; Cratarr's own work covers every account's.
        let top = s.lib.store_for(None).unwrap();
        assert_eq!((top.key("Chill"), ann.key("Chill")), ("Chill".to_string(), "ann/Chill".to_string()));
        assert_eq!((ann.name_of_key("ann/Chill"), ann.name_of_key("Chill"), top.name_of_key("ann/Chill")), (Some("Chill"), None, None));
        let all: Vec<String> = s.lib.all_stores().iter().map(|st| st.key("Chill")).collect();
        assert_eq!(all, ["Chill", "ann/Chill"]);
        // Another's, not shared, is as if there were none; and it is never another's to change.
        assert!(matches!(s.reach("ann/Chill"), Err(ToggleError::UnknownPlaylist)));
        assert!(matches!(s.set_shared("ann/Chill", true), Err(ToggleError::UnknownPlaylist)));
        std::fs::remove_dir_all(d.parent().unwrap()).ok();
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
    crate_stores: CrateStores,
    /// Where every change is kept for its undo (decision 0026).
    journal: Arc<Journal>,
    /// The favourite tracks' lines for the crates (see `favourite_lines`).
    favourites: Arc<FavouritesKept>,
    /// The playlists' table's facts (see `playlist_stats`).
    stats: StatsKept,
}

/// The account the running request is for, whose favourites the crates show.
fn asking_user() -> Option<String> {
    crate::serve::instance().and_then(|s| s.current_user())
}

/// The lines whose album has the asking account's heart on (decision 0024, amended 2026-10-19): the Library's album
/// favourites, the viewer's own whoever's crate it is, and Cratarr's own library telling each line's album, whatever
/// server the crate is shown for. `None` when they cannot be read (no library, no account).
async fn favourite_album_lines() -> Option<HashSet<String>> {
    let serve = crate::serve::instance()?;
    let me = serve.current_user()?;
    tokio::task::spawn_blocking(move || {
        let albums: HashSet<String> = serve.userdata.starred(&me, crate::userdata::Kind::Album).ok()?.into_iter().collect();
        if albums.is_empty() {
            return Some(HashSet::new());
        }
        let songs = serve.library.songs_by_rel(0).ok()?;
        Some(songs.into_iter().filter(|(_, s)| albums.contains(&s.album)).map(|(rel, _)| rel).collect())
    })
    .await
    .ok()
    .flatten()
}

/// The account whose playlists and crates the running request's are (decision 0051): another account's, or `None` for
/// the first administrator's (theirs are the ones from before accounts), as for Cratarr's own work.
pub(crate) fn asking_account() -> Option<String> {
    let s = crate::serve::instance()?;
    let me = s.current_user()?;
    (Some(&me) != s.users.owner().as_ref()).then_some(me)
}

/// Each account's crates (decision 0051: one's own, not shared): the first administrator's in `crates.json` as
/// before, another's in `users/<account>/crates.json`. One store each, as each keeps its crates in memory.
struct CrateStores {
    data_dir: Option<String>,
    first: Arc<crates::CrateStore>,
    others: std::sync::Mutex<HashMap<String, Arc<crates::CrateStore>>>,
}

impl CrateStores {
    fn new(data_dir: Option<&str>) -> Self {
        Self { data_dir: data_dir.map(str::to_string), first: Arc::new(crates::CrateStore::new(data_dir)), others: Default::default() }
    }

    fn of(&self, account: Option<&str>) -> Result<Arc<crates::CrateStore>, ToggleError> {
        let Some(account) = account else { return Ok(self.first.clone()) };
        if !crate::playlists::account_folder_ok(account) {
            return Err(ToggleError::Refused(format!("{account} cannot name a folder for its crates.")));
        }
        let mut others = self.others.lock().unwrap_or_else(|p| p.into_inner());
        let store = others.entry(account.to_string()).or_insert_with(|| Arc::new(crates::CrateStore::of_account(self.data_dir.as_deref(), account)));
        Ok(store.clone())
    }
}

/// The account a playlist owner's name stands for (`None`: the first administrator).
fn account_of(owner: &str) -> Option<String> {
    let first = crate::serve::instance().and_then(|s| s.users.owner());
    (Some(owner) != first.as_deref()).then(|| owner.to_string())
}

/// The name shown as the owner of an account's playlists.
fn owner_name(account: Option<&str>) -> String {
    account.map(str::to_string).or_else(|| crate::serve::instance().and_then(|s| s.users.owner())).unwrap_or_default()
}

/// A playlist as the asking account names it (decision 0051): its own by name, another's shared one as `owner/name`.
pub struct Reached {
    pub store: Arc<Store>,
    /// Its name among its owner's playlists.
    pub name: String,
    /// Whose it is (shown), whether it is shared, and whether it is the asking account's own to change.
    pub owner: String,
    pub shared: bool,
    pub mine: bool,
}

impl Reached {
    /// Its key in Cratarr's own records (`Store::key`).
    pub fn key(&self) -> String {
        self.store.key(&self.name)
    }
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

/// `favourites.json`: the last answer, by server and whose favourites they are.
#[derive(Serialize, serde::Deserialize)]
struct KeptFile {
    source: String,
    #[serde(default)]
    user: Option<String>,
    lines: Vec<String>,
}

#[derive(Clone)]
struct Kept {
    at: std::time::Instant,
    source: String,
    /// Whose favourites (decision 0051): the account that asked.
    user: Option<String>,
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
            Kept { at: now.checked_sub(FAVOURITES_KEEP).unwrap_or(now), source: k.source, user: k.user, upto: 0, lines: Some(Arc::new(k.lines.into_iter().collect())) }
        });
        Self { last: std::sync::Mutex::new(last), asking: Arc::default(), file }
    }

    /// Keep an answer for the next run. A failure costs only the wait after the next start.
    async fn save(&self, source: &str, user: Option<String>, lines: &Arc<HashSet<String>>) {
        let Some(file) = self.file.clone() else { return };
        let body = KeptFile { source: source.to_string(), user, lines: lines.iter().cloned().collect() };
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
        let user = asking_user();
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone().filter(|k| k.source == source && k.user == user)
    }

    /// Ask the server (the caller holds `asking`). When it does not answer, what was heard before is kept, for another
    /// minute before asking again.
    async fn ask(&self, c: Arc<crate::catalogue::Catalogue>, source: &str, from: &Arc<dyn source::Source>) -> Kept {
        let (upto, user) = (crates::favourite_changes_made(), asking_user());
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
            self.save(source, user.clone(), l).await;
        }
        let now = std::time::Instant::now();
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let kept = match (lines, last.clone().filter(|k| k.source == source && k.user == user && k.lines.is_some())) {
            (Some(lines), _) => {
                crates::favourite_changes_held(source, upto);
                Kept { at: now, source: source.to_string(), user, upto, lines: Some(lines) }
            }
            (None, Some(before)) => Kept { at: now, ..before },
            (None, None) => Kept { at: now, source: source.to_string(), user, upto, lines: None },
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
        let crate_stores = CrateStores::new(lib.data_dir());
        let favourites = Arc::new(FavouritesKept::load(lib.data_dir().map(|d| std::path::PathBuf::from(d).join("favourites.json"))));
        Self { lib, sync, crate_stores, journal, favourites, stats: StatsKept::default() }
    }

    /// Every playlist's facts for the Playlists page's table (decision 0042), by id. Worked out again only when a
    /// playlist's file, the library's build or the owner's plays changed since.
    pub async fn playlist_stats(&self) -> Result<std::collections::BTreeMap<String, PlaylistStats>, ToggleError> {
        let lists = self.visible_lines().await?;
        let Some(serve) = crate::serve::instance() else { return Ok(Default::default()) };
        let stamps: String = lists.iter().map(|(id, changed, lines)| format!("{id}:{changed}:{};", lines.len())).collect();
        let s = serve.clone();
        // Read here: the blocking task does not carry the request's account.
        let me = serve.current_user();
        let key_and_owner = tokio::task::spawn_blocking(move || {
            let plays = me.as_ref().and_then(|me| s.userdata.plays_print(me).ok());
            (format!("{stamps}|{}|{plays:?}", s.built_at()), me)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        let (key, me) = key_and_owner;
        if let Some((k, kept)) = self.stats.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
            && *k == key
        {
            return Ok(kept.clone());
        }
        let worked = tokio::task::spawn_blocking(move || {
            let songs = serve.library.songs_by_rel(0).unwrap_or_default();
            let plays = me.and_then(|me| serve.userdata.song_plays(&me).ok());
            lists.into_iter().map(|(id, changed, lines)| (id, playlist_stats(&lines, &songs, plays.as_ref(), changed))).collect::<std::collections::BTreeMap<_, _>>()
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        let answer = worked.clone();
        *self.stats.lock().unwrap_or_else(|p| p.into_inner()) = Some((key, worked));
        Ok(answer)
    }

    /// Ask the media server for the favourites once, at start, when a crate shows by them: the first visit to Crates
    /// then need not wait the minute that may take.
    pub async fn warm_favourites(&self) {
        let first = self.crate_stores.first.clone();
        let Ok(set) = first.all_favourites().await else { return };
        let needed = crates::permanent().into_iter().chain(first.list().await.unwrap_or_default()).any(|c| {
            crates::needs_favourites(&c.conditions) || set.get(&c.id).copied().unwrap_or(crates::Favourites::None) != crates::Favourites::All
        });
        if let (true, Ok((sid, source))) = (needed, self.source_of(None)) {
            self.favourite_lines(&sid, &source).await;
        }
    }

    /// Keep a change in the journal; its undo token (decision 0026). A journal that cannot be written costs only the
    /// undo: the change itself is done.
    fn keep(&self, change: Change) -> Option<String> {
        match self.journal.record_change(super::library::epoch_now(), &change, asking_account().as_deref()) {
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
        let (change, undone, account) = self.journal.change(id)?.ok_or_else(gone)?;
        // One's own changes only (decision 0051).
        if account != asking_account() {
            return Err(gone());
        }
        if undone || !self.journal.claim_change(id, super::library::epoch_now())? {
            return Err(ToggleError::Refused("That was undone already.".into()));
        }
        match self.reverse(change, account.as_deref()).await {
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
    async fn reverse(&self, change: Change, account: Option<&str>) -> Result<Change, ToggleError> {
        let changed = || ToggleError::Refused("It changed since; nothing was undone.".into());
        let store = self.lib.store_for(account)?;
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
                self.tell_sources(&store, redo.iter().map(|c| (c.playlist.clone(), PlaylistChange::Edited)).collect());
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
                self.tell_sources(&store, gone.iter().map(|n| (n.clone(), PlaylistChange::Deleted)).collect());
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
                    if store.account().is_none() {
                        self.sync.restore_links(&name).await?;
                    }
                    back.push((name, fingerprint(&text)));
                }
                Ok(Change::Created { playlists: back })
            }
            Change::Renamed { from, to } => {
                if store.text(&to).await.is_none() || store.text(&from).await.is_some() {
                    return Err(changed());
                }
                self.rename_unkept(&store, &to, &from).await?;
                Ok(Change::Renamed { from: to, to: from })
            }
            Change::Spoken { playlist, before, after } => {
                let key = store.key(&playlist);
                if self.sync.spoken().remove(&key).unwrap_or_default() != after || store.text(&playlist).await.is_none() {
                    return Err(changed());
                }
                self.sync.set_spoken(&key, &before)?;
                Ok(Change::Spoken { playlist, before: after, after: before })
            }
            Change::Crate { before, after } => {
                let Some(id) = after.as_ref().or(before.as_ref()).map(|c| c.id.clone()) else { return Err(changed()) };
                let crates = self.crate_stores.of(account)?;
                if crates.list().await?.into_iter().find(|c| c.id == id) != after {
                    return Err(changed());
                }
                match &before {
                    Some(b) => crates.put(b.clone()).await.map_err(ToggleError::Refused)?,
                    None => {
                        crates.delete(&id).await?;
                    }
                }
                Ok(Change::Crate { before: after, after: before })
            }
            Change::LeftOut { crate_id, before, after } => {
                let crates = self.crate_stores.of(account)?;
                if crates.left_out(&crate_id).await? != after {
                    return Err(changed());
                }
                crates.set_left_out(&crate_id, before.clone()).await?;
                Ok(Change::LeftOut { crate_id, before: after, after: before })
            }
        }
    }

    /// Every playlist, marked where it holds `entry` (the playing track's line), with its links.
    /// The asking account's own, and the ones others share (decision 0051), by the ids it knows them by.
    pub(super) async fn view(&self, entry: Option<&str>) -> Result<Vec<PlaylistView>> {
        let Ok(own) = self.lib.store_for(asking_account().as_deref()) else { return Ok(Vec::new()) };
        let shared = self.shared_keys();
        let mut list = own.view(entry).await?;
        let (mut links, mut spoken) = (self.sync.links(), self.sync.spoken());
        let mine = owner_name(own.account());
        for p in &mut list {
            let key = own.key(&p.id);
            // Kept in step with other servers: the first administrator's only.
            if own.account().is_none() {
                p.links = links.remove(&p.id).unwrap_or_default();
            }
            p.spoken = spoken.remove(&key).unwrap_or_default();
            (p.owner, p.shared, p.mine) = (mine.clone(), shared.contains(&key), true);
        }
        for store in self.others(&own) {
            if !shared.iter().any(|k| store.name_of_key(k).is_some()) {
                continue;
            }
            let owner = owner_name(store.account());
            for mut p in store.view(entry).await? {
                if shared.contains(&store.key(&p.id)) {
                    p.id = format!("{owner}/{}", p.name);
                    (p.owner, p.shared, p.mine) = (owner.clone(), true, false);
                    list.push(p);
                }
            }
        }
        list.sort_by_key(|p| p.name.to_lowercase());
        Ok(list)
    }

    /// The other accounts' playlists, beside `own`.
    fn others(&self, own: &Store) -> Vec<Arc<Store>> {
        self.lib.all_stores().into_iter().filter(|s| s.account() != own.account()).collect()
    }

    /// The keys of the playlists their owners share.
    fn shared_keys(&self) -> HashSet<String> {
        let Some(serve) = crate::serve::instance() else { return HashSet::new() };
        serve.userdata.shared_playlists().inspect_err(|e| tracing::warn!("the shared playlists not read: {e:#}")).unwrap_or_default()
    }

    /// `id` as the asking account names it (decision 0051), to read: its own playlist by name, or another's shared
    /// one as `owner/name` (one not shared is as if there were none).
    pub fn reach(&self, id: &str) -> Result<Reached, ToggleError> {
        let me = asking_account();
        let (account, name) = match id.split_once('/') {
            Some((owner, name)) => (account_of(owner), name),
            None => (me.clone(), id),
        };
        let store = self.lib.store_for(account.as_deref())?;
        let shared = self.shared_keys().contains(&store.key(name));
        let mine = account == me;
        if !mine && !shared {
            return Err(ToggleError::UnknownPlaylist);
        }
        Ok(Reached { owner: owner_name(store.account()), store, name: name.to_string(), shared, mine })
    }

    /// `id` to change: the asking account's own only (another's shared one is played and copied).
    fn own(&self, id: &str) -> Result<(Arc<Store>, String), ToggleError> {
        let r = self.reach(id)?;
        if !r.mine {
            return Err(ToggleError::Forbidden("Only its owner changes it: copy it into your own.".into()));
        }
        Ok((r.store, r.name))
    }

    /// The asking account's playlists and the ones others share, with their lines and when each last changed, by id.
    async fn visible_lines(&self) -> Result<Vec<(String, i64, Vec<String>)>, ToggleError> {
        let Ok(own) = self.lib.store_for(asking_account().as_deref()) else { return Ok(Vec::new()) };
        let mut out = own.lines_of_all().await?;
        let shared = self.shared_keys();
        for store in self.others(&own) {
            if !shared.iter().any(|k| store.name_of_key(k).is_some()) {
                continue;
            }
            let owner = owner_name(store.account());
            for (name, changed, lines) in store.lines_of_all().await? {
                if shared.contains(&store.key(&name)) {
                    out.push((format!("{owner}/{name}"), changed, lines));
                }
            }
        }
        Ok(out)
    }

    /// Shares one's own playlist with everyone, or stops (decision 0051): they see it, play it and copy it.
    pub fn set_shared(&self, id: &str, on: bool) -> Result<(), ToggleError> {
        let (store, name) = self.own(id)?;
        if !store.path_exists(&name) {
            return Err(ToggleError::UnknownPlaylist);
        }
        let serve = crate::serve::instance().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        // As the account's permissions say (decision 0051); stopping is always one's own to do.
        let may = serve.current_user().and_then(|u| serve.users.get(&u)).is_none_or(|u| u.permissions.share);
        if on && !may {
            return Err(ToggleError::Forbidden("This account may not share playlists.".into()));
        }
        Ok(serve.userdata.set_playlist_shared(&store.key(&name), on)?)
    }

    /// A copy of a playlist (another's shared one, or one's own) among one's own, under a free name. Returns its name
    /// and the undo token.
    pub async fn copy(&self, id: &str) -> Result<(String, Option<String>), ToggleError> {
        let from = self.reach(id)?;
        let entries = from.store.entries(&from.name).await?;
        let own = self.lib.store_for(asking_account().as_deref())?;
        let taken: HashSet<String> = own.view(None).await?.into_iter().map(|p| p.id.to_lowercase()).collect();
        let name = crate::playlists::free_name(&from.name, |n| taken.contains(&n.to_lowercase()));
        let undo = self.create_with(&name, &entries).await?;
        Ok((name, undo))
    }

    /// A playlist id the asking account would know a key by (`Store::key`): its own by name, another's as `owner/name`.
    pub fn id_of_key(&self, key: &str) -> String {
        let me = asking_account();
        match self.lib.store_for(me.as_deref()).ok().and_then(|own| own.name_of_key(key).map(str::to_string)) {
            Some(name) => name,
            None => match key.split_once('/') {
                Some((account, name)) => format!("{account}/{name}"),
                None => format!("{}/{key}", owner_name(None)),
            },
        }
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
        let (store, name) = self.own(id)?;
        if store.entries(&name).await.is_err() {
            return Err(ToggleError::UnknownPlaylist);
        }
        let names: Vec<String> = names.iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
        let key = store.key(&name);
        let before = self.sync.spoken().remove(&key).unwrap_or_default();
        self.sync.set_spoken(&key, &names)?;
        Ok((before != names).then(|| self.keep(Change::Spoken { playlist: name, before, after: names })).flatten())
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
        let (store, name) = self.own(playlist_id)?;
        let changed = match member {
            Some(m) => store.set_member(&name, entry, m).await?,
            None => {
                store.toggle(&name, entry).await?;
                true
            }
        };
        if changed {
            self.tell_sources(&store, vec![(name, PlaylistChange::Edited)]);
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
        let r = self.reach(id)?;
        let entries = r.store.entries(&r.name).await?;
        let sources = self.lib.connected();
        let source = want
            .and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .map(|(a, _)| a.cfg.id.clone())
            .unwrap_or_default();
        let mut rows = self.rows_of(entries.clone(), &source).await?;
        // The user's plays of each track and when each entry came, as the user data keeps them (the panel's orders).
        if let Some(serve) = crate::serve::instance() {
            let (id, rs) = (r.key(), rows.iter().map(|r| r.id.clone().unwrap_or_default()).collect::<Vec<_>>());
            let me = serve.current_user();
            let facts = tokio::task::spawn_blocking(move || {
                let plays = match me {
                    Some(me) => serve.userdata.play_stats(&me, &rs).unwrap_or_default(),
                    None => Default::default(),
                };
                let added = serve.userdata.entries_added(&id, &entries).unwrap_or_default();
                (plays, added)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
            let (plays, added) = facts;
            for (i, row) in rows.iter_mut().enumerate() {
                if let Some((count, last)) = row.id.as_ref().and_then(|t| plays.get(t)) {
                    row.play_count = Some(*count);
                    row.last_played = *last;
                }
                row.added = added.get(i).copied().flatten();
            }
        }
        Ok((source, rows))
    }

    /// Rows for these lines (a playlist's, or a page of a crate's), with what the catalogue knows of their files.
    async fn rows_of(&self, entries: Vec<String>, source: &str) -> Result<Vec<PlaylistRow>, ToggleError> {
        let (c, sid, music) = (self.lib.catalogue(), source.to_string(), self.lib.music());
        let rows = tokio::task::spawn_blocking(move || -> Result<Vec<PlaylistRow>> {
            let facts = c.line_rows(&sid, &entries)?;
            // Cratarr's own tracks with their album, as the library has it (the app's downloads are told it, and find
            // their album and its cover by it).
            let albums = match crate::serve::instance() {
                Some(serve) if sid == crate::source::cratarr::KIND.id => {
                    let ids: Vec<String> = facts.iter().filter_map(|f| f.track_id.clone()).collect();
                    serve.library.albums_of(&ids)?
                }
                _ => Default::default(),
            };
            let dir = music.map(std::path::PathBuf::from);
            Ok(entries
                .into_iter()
                .zip(facts)
                .map(|(entry, f)| PlaylistRow {
                    album_id: f.track_id.as_ref().and_then(|t| albums.get(t).cloned()),
                    missing: !dir.as_deref().is_some_and(|d| crate::playlists::entry_exists(d, &entry)),
                    file: entry.rsplit('/').next().unwrap_or(&entry).to_string(),
                    entry,
                    id: f.track_id,
                    name: f.title,
                    artist: f.artist,
                    album: f.album,
                    duration_secs: f.duration_ms.map(|ms| ms as f64 / 1000.0),
                    album_artist: f.album_artist,
                    disc: f.disc,
                    track: f.track,
                    year: f.year,
                    bpm: f.bpm,
                    play_count: None,
                    last_played: None,
                    added: None,
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
        let mut lists = Vec::new();
        for n in names {
            let r = self.reach(n)?;
            lists.push((r.name.clone(), r.store.entries(&r.name).await?));
        }
        let c = self.lib.catalogue();
        let plan = tokio::task::spawn_blocking(move || {
            crate::export::plan(std::path::Path::new(&music), lists, |rels| {
                Ok(c.line_rows("", rels)?.into_iter().map(|f| (f.artist, f.title)).collect())
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
        let own = self.own_crates()?.list().await?;
        let all: Vec<(bool, CrateDef)> = crates::permanent().into_iter().map(|c| (true, c)).chain(own.into_iter().map(|c| (false, c))).collect();
        let set = self.own_crates()?.all_favourites().await?;
        let albums_set = self.own_crates()?.all_favourite_albums().await?;
        let albums_of = |id: &str| albums_set.get(id).copied().unwrap_or(crates::Favourites::All);
        let left_set = self.own_crates()?.all_left_out().await?;
        let none: Vec<String> = Vec::new();
        let left_of = |id: &str| left_set.get(id).unwrap_or(&none);
        // The favourites are asked for only when a crate shows by them (its conditions, or its toggle): asking the
        // media server for every favourite is slow, and a list of crates that need none must not wait for it. The
        // other settings' counts are then unknown (null) until it is asked.
        let needed = all.iter().any(|(_, c)| {
            crates::needs_favourites(&c.conditions) || set.get(&c.id).copied().unwrap_or(crates::Favourites::None) != crates::Favourites::All
        });
        let needed_albums = all.iter().any(|(_, c)| crates::needs_favourite_albums(&c.conditions) || albums_of(&c.id) != crates::Favourites::All);
        let lib = self.crate_library(want, needed, needed_albums).await?;
        let count = |conditions: Vec<crates::Condition>| lib.knows(&conditions).then(|| lib.select(&conditions).len());
        // The cards' facts: the songs of the Music folder, and what the account asking has played.
        let songs_played = match crate::serve::instance() {
            Some(serve) => {
                let me = serve.current_user();
                tokio::task::spawn_blocking(move || {
                let songs = serve.library.songs_by_rel(0).unwrap_or_default();
                let played = me.and_then(|me| serve.userdata.played(&me).ok());
                (songs, played)
                })
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?
            }
            None => (HashMap::new(), None),
        };
        let (songs, played) = songs_played;
        let facts: HashMap<&str, &crates::Facts> = lib.files.iter().map(|f| (f.rel.as_str(), f)).collect();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let images = self.crate_images();
        Ok(all
            .into_iter()
            .map(|(permanent, c)| {
                let favourites = set.get(&c.id).copied().unwrap_or(crates::Favourites::None);
                let favourite_albums = albums_of(&c.id);
                let left = left_of(&c.id);
                let shown = crates::leaving_out(left, &crates::shown(&c.conditions, favourites, favourite_albums));
                let stats = lib.knows(&shown).then(|| crate_stats(&lib.select(&shown), &facts, &songs, played.as_ref(), now));
                // Per setting of the favourites toggle, the albums toggle as it is set and the left-out list.
                let counted = |tracks: crates::Favourites| count(crates::leaving_out(left, &crates::shown(&c.conditions, tracks, favourite_albums)));
                CrateView {
                    counts: CrateCounts {
                        all: counted(crates::Favourites::All),
                        only: counted(crates::Favourites::Only),
                        none: counted(crates::Favourites::None),
                    },
                    favourites,
                    favourite_albums,
                    left_out: left.len(),
                    stats,
                    image: images.as_ref().is_some_and(|d| d.join(image_file(&c.id)).is_file()),
                    id: c.id,
                    name: c.name,
                    permanent,
                }
            })
            .collect())
    }

    /// Where crates' own pictures are kept (in the data folder); `None` without one.
    fn crate_images(&self) -> Option<std::path::PathBuf> {
        self.lib.data_dir().map(|d| std::path::PathBuf::from(d).join("crate-images"))
    }

    /// Sets a crate's own picture: any picture Cratarr can read, kept as JPEG at most 800 px on its longer side.
    pub async fn set_crate_image(&self, id: &str, data: Vec<u8>) -> Result<(), ToggleError> {
        self.crate_def(id).await?;
        self.may_picture(id)?;
        let Some(dir) = self.crate_images() else { return Err(ToggleError::SettingsReadOnly) };
        let file = dir.join(image_file(id));
        tokio::task::spawn_blocking(move || -> Result<()> {
            let jpeg = crate::serve::art::jpeg_within(&data, 800)?;
            std::fs::create_dir_all(&dir)?;
            let tmp = file.with_extension("jpg.tmp");
            std::fs::write(&tmp, jpeg)?;
            std::fs::rename(&tmp, &file)?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))
    }

    /// A crate's own picture and when it was set; `None` when it has none.
    pub async fn crate_image(&self, id: &str) -> Option<(Vec<u8>, std::time::SystemTime)> {
        self.crate_def(id).await.ok()?;
        let file = self.crate_images()?.join(image_file(id));
        tokio::task::spawn_blocking(move || {
            let modified = std::fs::metadata(&file).and_then(|m| m.modified()).ok()?;
            Some((std::fs::read(&file).ok()?, modified))
        })
        .await
        .ok()
        .flatten()
    }

    /// The crate shows its albums' covers again.
    pub async fn remove_crate_image(&self, id: &str) -> Result<(), ToggleError> {
        self.crate_def(id).await?;
        self.may_picture(id)?;
        if let Some(file) = self.crate_images().map(|d| d.join(image_file(id))) {
            match tokio::fs::remove_file(&file).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(anyhow::Error::from(e).into()),
            }
        }
        Ok(())
    }

    /// A built-in crate is everyone's: its picture is the administrators' to change (decision 0051).
    fn may_picture(&self, id: &str) -> Result<(), ToggleError> {
        let built_in = crates::permanent().iter().any(|c| c.id == id) || crates::genre_crate(id).is_some();
        if built_in && !crate::serve::access::administering() {
            return Err(ToggleError::Forbidden("A built-in crate's picture is for administrators.".into()));
        }
        Ok(())
    }

    /// A crate's definition: a permanent one, or one's own.
    pub async fn crate_def(&self, id: &str) -> Result<CrateDef, ToggleError> {
        if let Some(c) = crates::permanent().into_iter().find(|c| c.id == id).or_else(|| crates::genre_crate(id)) {
            return Ok(c);
        }
        self.own_crates()?.list().await?.into_iter().find(|c| c.id == id).ok_or(ToggleError::UnknownPlaylist)
    }

    /// The lines these conditions select, newest first, and the server they are for.
    async fn crate_lines_of(&self, conditions: &[crates::Condition], want: Option<&str>) -> Result<(String, Vec<String>), ToggleError> {
        let (lib, lines) = self.crate_selection(conditions, want).await?;
        Ok((lib.source, lines))
    }

    /// The library a crate is worked out from, and its lines.
    async fn crate_selection(&self, conditions: &[crates::Condition], want: Option<&str>) -> Result<(CrateLibrary, Vec<String>), ToggleError> {
        let lib = self.crate_library(want, crates::needs_favourites(conditions), crates::needs_favourite_albums(conditions)).await?;
        if crates::needs_favourites(conditions) && lib.favourites.is_none() {
            return Err(ToggleError::CannotPlay("The server's favourites could not be read.".into()));
        }
        if crates::needs_favourite_albums(conditions) && lib.favourite_albums.is_none() {
            return Err(ToggleError::CannotPlay("The favourite albums could not be read.".into()));
        }
        let lines = lib.select(conditions);
        Ok((lib, lines))
    }

    /// A page of a crate's rows (it can hold thousands): from `offset`, at most `limit`, with the crate's size, as its
    /// two toggles are set (`fav` and `albums`, else as saved).
    pub async fn crate_rows(
        &self,
        id: &str,
        want: Option<&str>,
        offset: usize,
        limit: usize,
        fav: Option<crates::Favourites>,
        albums: Option<crates::Favourites>,
    ) -> Result<CratePage, ToggleError> {
        let shown = self.crate_shown(id, fav, albums).await?;
        self.crate_page(&shown, want, offset, limit).await
    }

    /// A crate's conditions as its toggles show it: as given, else as saved.
    async fn crate_shown(&self, id: &str, fav: Option<crates::Favourites>, albums: Option<crates::Favourites>) -> Result<Vec<crates::Condition>, ToggleError> {
        let def = self.crate_def(id).await?;
        let fav = match fav {
            Some(f) => f,
            None => self.crate_favourites(id).await?,
        };
        let albums = match albums {
            Some(a) => a,
            None => self.own_crates()?.favourite_albums(id).await?,
        };
        let left = self.own_crates()?.left_out(id).await?;
        Ok(crates::leaving_out(&left, &crates::shown(&def.conditions, fav, albums)))
    }

    /// Tracks taken out of a crate by hand, or put back (`out` false), by their ids on a server (decision 0024, amended
    /// 2026-10-19): kept as their lines in the crate's left-out list. Returns how many it holds now and the undo token.
    pub async fn crate_leave_out(&self, id: &str, want: Option<&str>, ids: &[String], out: bool) -> Result<(usize, Option<String>), ToggleError> {
        self.crate_def(id).await?;
        let (sid, _) = self.source_of(want)?;
        let (c, s, ids) = (self.lib.catalogue(), sid.clone(), ids.to_vec());
        let lines = tokio::task::spawn_blocking(move || c.rels_of(&s, 0, &ids)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        let store = self.own_crates()?;
        let before = store.left_out(id).await?;
        let mut after = before.clone();
        if out {
            let mut new: Vec<String> = lines.into_iter().filter(|l| !before.contains(l)).collect();
            new.sort();
            after.extend(new);
        } else {
            after.retain(|l| !lines.contains(l));
        }
        if after == before {
            return Ok((after.len(), None));
        }
        store.set_left_out(id, after.clone()).await?;
        let undo = self.keep(Change::LeftOut { crate_id: id.to_string(), before, after: after.clone() });
        Ok((after.len(), undo))
    }

    /// A crate's tracks left out by hand, as rows on a server, for its editor to put back.
    pub async fn crate_left_out(&self, id: &str, want: Option<&str>) -> Result<(String, Vec<PlaylistRow>), ToggleError> {
        self.crate_def(id).await?;
        let (sid, _) = self.source_of(want)?;
        let lines = self.own_crates()?.left_out(id).await?;
        let rows = self.rows_of(lines, &sid).await?;
        Ok((sid, rows))
    }

    /// A crate's tempo spread (decision 0041), its tracks chosen as its rows are (the favourites toggle `fav`, else as
    /// saved): counted here, so a genre of thousands of tracks does not send them all to be counted.
    pub async fn crate_tempo(
        &self,
        id: &str,
        want: Option<&str>,
        fav: Option<crates::Favourites>,
        albums: Option<crates::Favourites>,
    ) -> Result<crates::TempoBands, ToggleError> {
        let shown = self.crate_shown(id, fav, albums).await?;
        let (lib, lines) = self.crate_selection(&shown, want).await?;
        let chosen: HashSet<&str> = lines.iter().map(String::as_str).collect();
        Ok(crates::tempo_bands(lib.files.iter().filter(|f| chosen.contains(f.rel.as_str())).map(|f| f.bpm)))
    }

    /// A crate's favourites toggle as set; a genre not set shows all its tracks (a crate not set shows no favourites).
    async fn crate_favourites(&self, id: &str) -> Result<crates::Favourites, ToggleError> {
        match self.own_crates()?.all_favourites().await?.get(id) {
            Some(f) => Ok(*f),
            None if crates::genre_crate(id).is_some() => Ok(crates::Favourites::All),
            None => Ok(self.own_crates()?.favourites(id).await?),
        }
    }

    /// Sets a crate's favourites toggle (decision 0024, amended): one setting per crate, for every device. Not journaled:
    /// a tap undoes it by tapping again.
    pub async fn set_crate_favourites(&self, id: &str, fav: crates::Favourites) -> Result<(), ToggleError> {
        self.crate_def(id).await?;
        Ok(self.own_crates()?.set_favourites(id, fav).await?)
    }

    /// Sets a crate's favourite albums toggle (decision 0024, amended 2026-10-19), as the favourites toggle is set.
    pub async fn set_crate_favourite_albums(&self, id: &str, albums: crates::Favourites) -> Result<(), ToggleError> {
        self.crate_def(id).await?;
        Ok(self.own_crates()?.set_favourite_albums(id, albums).await?)
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
        let shown = self.crate_shown(id, None, None).await?;
        let (_, lines) = self.crate_lines_of(&shown, want).await?;
        if lines.is_empty() {
            return Err(ToggleError::Refused("That crate is empty.".into()));
        }
        let undo = self.create_with(name, &lines).await?;
        Ok((lines.len(), undo))
    }

    /// One's own crate, if there is one with that id.
    async fn own_crate(&self, id: &str) -> Result<Option<CrateDef>> {
        Ok(self.own_crates()?.list().await?.into_iter().find(|c| c.id == id))
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
        let id = self.own_crates()?.save(id, name, conditions).await.map_err(ToggleError::Refused)?;
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
        if !self.own_crates()?.delete(id).await? {
            return Err(ToggleError::UnknownPlaylist);
        }
        Ok(self.keep(Change::Crate { before, after: None }))
    }

    /// The axes and their domains in this library, for the builder (decision 0024, issue 4).
    pub async fn crate_axes(&self, want: Option<&str>) -> Result<Vec<crates::AxisView>, ToggleError> {
        let lib = self.crate_library(want, false, false).await?;
        let playlists: Vec<String> = self.own_store()?.snapshot().await?.into_iter().map(|(name, _, _)| name).collect();
        Ok(crates::axes(&lib.files, &playlists))
    }

    /// What a crate is worked out on: Music's files newest first with their facts, which playlists hold each line, and
    /// (when needed) the favourites, kept for a minute (decision 0024, issue 5).
    async fn crate_library(&self, want: Option<&str>, favourites: bool, albums: bool) -> Result<CrateLibrary, ToggleError> {
        let (sid, source) = self.source_of(want)?;
        // First, so a request waiting for them holds nothing large meanwhile.
        let favourites = if favourites { self.favourite_lines(&sid, &source).await } else { None };
        let favourite_albums = if albums { favourite_album_lines().await } else { None };
        let c = self.lib.catalogue();
        let files = tokio::task::spawn_blocking(move || c.crate_facts(0)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        let mut membership: HashMap<String, Vec<String>> = HashMap::new();
        for (name, lines, _) in self.own_store()?.snapshot().await? {
            for line in lines {
                membership.entry(line).or_default().push(name.clone());
            }
        }
        Ok(CrateLibrary { source: sid, files, membership, favourites, favourite_albums })
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
                    // For the account asking: their favourites.
                    tokio::spawn(crate::serve::who::carried(async move {
                        let _turn = turn;
                        f.ask(c, &s, &from).await;
                    }));
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
        let r = self.reach(id)?;
        let entries = r.store.entries(&r.name).await?;
        let sources = self.lib.connected();
        let (active, source) = want
            .and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .cloned()
            .ok_or_else(|| ToggleError::CannotPlay("no server is connected".into()))?;
        let (c, sid) = (self.lib.catalogue(), active.cfg.id.clone());
        let found = tokio::task::spawn_blocking(move || c.line_track_ids(&sid, &entries))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))??;
        let unknown = found.iter().filter(|f| f.is_none()).count();
        Ok((active, source, found.into_iter().flatten().collect(), unknown))
    }

    /// After an edit: the playlist's copies follow (the shared one at once, the others with the next pass). Only the
    /// first administrator's are kept in step with other servers (decision 0051).
    fn tell_sources(&self, store: &Store, changes: Vec<(String, PlaylistChange)>) {
        if store.account().is_none() {
            self.sync.changed(changes.into_iter().map(|(name, _)| name).collect());
        }
    }

    /// The asking account's own playlists.
    fn own_store(&self) -> Result<Arc<Store>, ToggleError> {
        self.lib.store_for(asking_account().as_deref())
    }

    /// The asking account's own crates.
    fn own_crates(&self) -> Result<Arc<crates::CrateStore>, ToggleError> {
        self.crate_stores.of(asking_account().as_deref())
    }

    /// 0 when there is no Music folder.
    pub async fn count(&self) -> Result<usize> {
        Ok(self.view(None).await?.len())
    }

    /// One's own playlists' repairs.
    pub async fn repairs(&self) -> Result<Repairs> {
        self.own_store()?.repairs().await
    }

    /// Apply the repairs the client chose (`None` = every safe one, or only those of the playlists `within`). The store
    /// only accepts pairs it currently offers. The sources are then told about each changed playlist. Returns the
    /// changed playlists and the undo token (one for all of them).
    pub async fn apply_repairs(&self, only: Option<Repair>, within: Option<&[String]>) -> Result<(Vec<String>, Option<String>), ToggleError> {
        let store = self.own_store()?;
        let wanted = match only {
            Some(one) => vec![one],
            None => safe_within(store.repairs().await?.safe, within),
        };
        let edits = store.apply_repairs(&wanted).await?;
        let changed: Vec<String> = edits.iter().map(|e| e.id.clone()).collect();
        self.tell_sources(&store, changed.iter().map(|n| (n.clone(), PlaylistChange::Edited)).collect());
        Ok((changed, self.keep_texts(edits)))
    }

    /// Album folders that look like the same release twice, each group compared. Read-only. Track details come
    /// from the first source that knows the folder.
    pub async fn album_duplicates(&self) -> Result<Vec<crate::albums::AlbumGroup>> {
        use crate::albums::*;
        let Some(music) = self.lib.music().map(PathBuf::from) else { return Ok(Vec::new()) };
        let stores = self.lib.all_stores();
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
                // Every account's playlists that hold it, by their keys.
                let mut lines = Vec::new();
                for store in &stores {
                    lines.extend(store.lines_in_folder(folder).await?.into_iter().map(|(name, n)| (store.key(&name), n)));
                }
                albums.push(build_view(folder, files, cover, &known, lines));
            }
            let comparisons = albums.iter().skip(1).map(|b| compare(&albums[0], b)).collect();
            let suggestion = suggest(&albums);
            out.push(AlbumGroup { albums, comparisons, suggestion });
        }
        Ok(out)
    }

    /// A new playlist is Shared, as playlists were before sync links (decision 0014). Returns the undo token.
    pub async fn create(&self, name: &str) -> Result<Option<String>, ToggleError> {
        let store = self.own_store()?;
        store.create(name).await?;
        if store.account().is_none() {
            self.sync.new_playlist(name).await?;
        }
        Ok(self.keep(Change::Created { playlists: vec![(name.to_string(), fingerprint(""))] }))
    }

    /// A playlist edited in its view (decision 0023): its lines put in a new order or left out, as one write. Only the
    /// lines it has (`expected`, as the view showed them) can be kept; it is refused when the playlist changed since.
    /// Returns the undo token (none when nothing changed).
    pub async fn edit(&self, id: &str, expected: &[String], entries: &[String]) -> Result<Option<String>, ToggleError> {
        if !is_rearrangement(expected, entries) {
            return Err(ToggleError::Refused("An edit can only reorder or remove the playlist's own lines.".into()));
        }
        let (store, name) = self.own(id)?;
        let Some(edit) = store.replace_entries_if(&name, expected, entries).await? else {
            return Err(ToggleError::Refused("The playlist changed meanwhile: look at it again.".into()));
        };
        self.tell_sources(&store, vec![(name, PlaylistChange::Edited)]);
        Ok(self.keep_texts(vec![edit]))
    }

    /// Take the repeats out of one playlist, the first copy of each track staying: the user's "Remove repeats"
    /// (decision 0025, amended 2026-10-16). Returns the undo token, none when it held none.
    pub async fn remove_repeats(&self, id: &str) -> Result<Option<String>, ToggleError> {
        let (store, name) = self.own(id)?;
        let Some(edit) = store.remove_repeats(&name).await? else { return Ok(None) };
        self.tell_sources(&store, vec![(name, PlaylistChange::Edited)]);
        Ok(self.keep_texts(vec![edit]))
    }

    /// Add these tracks (ids on `source_id`, in order) at a playlist's end, whether or not it holds them already: the
    /// page asked and the user chose Add again (decision 0025, amended 2026-10-16). Tracks outside the library are left
    /// out. Returns how many were added and the undo token.
    pub async fn append_tracks(&self, id: &str, source_id: &str, ids: &[String]) -> Result<(usize, Option<String>), ToggleError> {
        let gone = || ToggleError::Refused("Those tracks are not on a connected server.".into());
        let (a, s) = self.lib.connected().into_iter().find(|(a, _)| a.cfg.id == source_id).ok_or_else(gone)?;
        let (store, name) = self.own(id)?;
        let folders = self.lib.folders();
        let mut entries = Vec::new();
        for track in ids {
            if !crate::covers::valid_item_id(track) {
                continue;
            }
            if let Ok(t) = s.track(track).await
                && let Some(e) = a.line(&folders, &t)
            {
                entries.push(e);
            }
        }
        if entries.is_empty() {
            return Err(ToggleError::NotInLibrary);
        }
        let edit = store.append(&name, &entries).await?;
        self.tell_sources(&store, vec![(name, PlaylistChange::Edited)]);
        Ok((entries.len(), self.keep_texts(vec![edit])))
    }

    /// The asking account's playlists and the ones others share, by id, with their lines and missing-track counts
    /// (the OpenSubsonic playlists, decision 0030).
    pub async fn snapshot(&self) -> Result<Vec<(String, Vec<String>, usize)>, ToggleError> {
        let Ok(own) = self.own_store() else { return Ok(Vec::new()) };
        let mut out = own.snapshot().await?;
        let shared = self.shared_keys();
        for store in self.others(&own) {
            if !shared.iter().any(|k| store.name_of_key(k).is_some()) {
                continue;
            }
            let owner = owner_name(store.account());
            out.extend(store.snapshot().await?.into_iter().filter(|(n, _, _)| shared.contains(&store.key(n))).map(|(n, l, d)| (format!("{owner}/{n}"), l, d)));
        }
        Ok(out)
    }

    /// Every account's playlists' lines, for what draws on all of them (Radio's sense of what goes together).
    pub async fn snapshot_all(&self) -> Result<Vec<(String, Vec<String>, usize)>, ToggleError> {
        let mut out = Vec::new();
        for store in self.lib.all_stores() {
            out.extend(store.snapshot().await?.into_iter().map(|(n, l, d)| (store.key(&n), l, d)));
        }
        Ok(out)
    }

    /// One playlist's lines (one's own, or a shared one).
    pub async fn entries(&self, id: &str) -> Result<Vec<String>, ToggleError> {
        let r = self.reach(id)?;
        Ok(r.store.entries(&r.name).await?)
    }

    /// A playlist's lines replaced as one write, as `edit`, but the new list may also hold lines it did not have (an
    /// OpenSubsonic client adding songs, decision 0030). The caller makes the lines from the catalogue, never from what
    /// a client sent. Refused when the playlist changed since `expected`. Repeats are kept as the client wrote them
    /// (decision 0025, amended 2026-10-16). Returns the undo token (none when nothing changed).
    pub async fn set_entries(&self, id: &str, expected: &[String], entries: &[String]) -> Result<Option<String>, ToggleError> {
        let (store, name) = self.own(id)?;
        let Some(edit) = store.replace_entries_if(&name, expected, entries).await? else {
            return Err(ToggleError::Refused("The playlist changed meanwhile: look at it again.".into()));
        };
        self.tell_sources(&store, vec![(name, PlaylistChange::Edited)]);
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
        let store = self.own_store()?;
        store.create(name).await?;
        // Filled only while still empty: never over an edit made since it was created.
        let Some(filled) = store.replace_entries_if(name, &[], entries).await? else {
            return Err(ToggleError::Refused("The new playlist changed while it was being filled.".into()));
        };
        if store.account().is_none() {
            self.sync.new_playlist(name).await?;
        }
        Ok(self.keep(Change::Created { playlists: vec![(name.to_string(), fingerprint(&filled.after))] }))
    }

    /// Returns the undo token.
    pub async fn rename(&self, from: &str, to: &str) -> Result<Option<String>, ToggleError> {
        let (store, from) = self.own(from)?;
        self.rename_unkept(&store, &from, to).await?;
        Ok(self.keep(Change::Renamed { from, to: to.to_string() }))
    }

    async fn rename_unkept(&self, store: &Store, from: &str, to: &str) -> Result<(), ToggleError> {
        if store.account().is_none() {
            // Through the sync, so the server copies keep following the playlist (decision 0012).
            self.sync.rename(from, to, store.rename(from, to)).await?;
        } else {
            store.rename(from, to).await?;
        }
        self.tell_sources(store, vec![(from.to_string(), PlaylistChange::Deleted), (to.to_string(), PlaylistChange::Created)]);
        // Subsonic clients keep a playlist's id (`pl-` and its name) across a rename (decision 0030); it stays shared.
        if let Some(serve) = crate::serve::instance() {
            serve.playlist_renamed(&store.key(from), &store.key(to));
        }
        Ok(())
    }

    /// The store keeps it aside; its undo brings it back with the links it had (unshared). Returns the undo token.
    pub async fn delete(&self, id: &str) -> Result<Option<String>, ToggleError> {
        let (store, name) = self.own(id)?;
        store.delete(&name).await?;
        if let Some(serve) = crate::serve::instance()
            && let Err(e) = serve.userdata.set_playlist_shared(&store.key(&name), false)
        {
            tracing::warn!("{name}: no longer shared, but that was not kept: {e:#}");
        }
        self.tell_sources(&store, vec![(name.clone(), PlaylistChange::Deleted)]);
        Ok(self.keep(Change::Deleted { playlists: vec![name] }))
    }

    /// Re-check which playlist entries point at missing files (the library changes without the playlists): every
    /// account's.
    pub async fn refresh_dead(&self) {
        for store in self.lib.all_stores() {
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
        first.save("jellyfin", None, &Arc::new(["A/1.flac".to_string(), "B/2.mp3".to_string()].into_iter().collect())).await;

        let next = FavouritesKept::load(Some(file));
        let k = next.kept("jellyfin").expect("kept from the last run");
        assert_eq!(k.lines.as_deref().map(|l| l.len()), Some(2));
        assert!(k.at.elapsed() >= FAVOURITES_KEEP, "due to be asked again");
        assert!(next.kept("other").is_none(), "only for the server it came from");
        std::fs::remove_dir_all(&dir).ok();
    }
}
