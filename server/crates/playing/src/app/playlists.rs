//! Playlists (decision 0011): the Music folder's playlist files, edits and repairs, Play, and telling the servers
//! about changes. The crates are over this service (`crates`, SP3-U): it keeps their stores, the favourites heard, and the
//! journal with the Undo of every change, the playlists' and the crates' alike.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;

use super::ToggleError;
use super::library::{Library, label_of};
use std::collections::{HashMap, HashSet};

use crate::crates;
use crate::serve::listener::Listener;
use crate::journal::{Change, EventId, Journal, TextChange, fingerprint};
use crate::playlists::{Edited, PlaylistView, Repair, Repairs, Store};
use crate::source::{self, PlaylistChange};
use crate::sync_state::Kind;

mod ops;
pub use ops::{OpsAnswer, PlaylistOps};

/// The most tracks one play sends: Finamp holds at most 100 in its queue (seen 2026-10-12; user: "we go by 100").
pub const PLAY_MAX: usize = 100;

/// A playlist found by a spoken name, as its tracks on a server (decision 0015): the caller plays them itself.
#[derive(Debug, Serialize)]
pub struct ByName {
    /// The playlist it was taken for; `None` when nothing fits what was said.
    pub playlist: Option<String>,
    /// The server's ids of its tracks, in order or shuffled, at most the limit asked for.
    pub tracks: Vec<String>,
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

pub use crate::serve::playlists_row::PlaylistRow;

#[derive(Debug, Serialize)]
pub struct PlayResult {
    /// "Finamp on Lenovo Tab".
    pub player: String,
    /// Tracks sent (at most 500; to a Jellyfin player, PLAY_MAX).
    pub sent: usize,
    /// Playable tracks in the playlist.
    pub total: usize,
    /// Lines whose track is missing or not known to the server yet; left out.
    pub unknown: usize,
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

/// What an export needs of a file (its names, its format for the estimates) from the catalogue's row of it.
fn export_facts(row: crate::catalogue::FileRow, format: Option<String>, kbps: Option<u32>) -> crate::export::Facts {
    crate::export::Facts {
        artist: row.artist,
        title: row.title,
        album: row.album,
        album_artist: row.album_artist,
        format,
        kbps,
        duration_ms: row.duration_ms.filter(|d| *d > 0).map(|d| d as u64),
    }
}

#[cfg(test)]
mod card_tests {
    use super::*;
    use crate::serve::library::RelSong;

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

/// The playlist service over an empty Music folder of its own, no servers, the journal and the crates in memory: for the
/// tests here and the crates' (`crates::undo_tests`).
#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    use super::*;

    /// The service over an empty Music folder of its own, no servers, the journal and the crates in memory.
    pub async fn service() -> (PathBuf, PlaylistService) {
        service_with(None).await
    }

    /// `service`, with Cratarr's own library as `main` hands it.
    pub async fn service_with(served: Option<Arc<crate::serve::Serve>>) -> (PathBuf, PlaylistService) {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("cratarr-undo-{}-{n}", std::process::id()));
        // A Music folder of its own, with the playlist folder 0.15 kept its playlists in (decision 0014).
        std::fs::create_dir_all(d.join(crate::playlists::PLAYLIST_DIR)).unwrap();
        let db = served.as_ref().map(|s| s.userdata.clone()).unwrap_or_else(|| Arc::new(crate::userdata::UserData::open(None).unwrap()));
        let lib = Library::new(None, Some(d.to_string_lossy().into_owned()), Vec::new(), Default::default(), None, db, None).await.unwrap();
        let journal = Arc::new(Journal::open(None).unwrap());
        let state = crate::sync_state::SyncState::open(None).unwrap();
        let sync = Arc::new(super::super::sync::SyncService::new(lib.clone(), state, journal.clone()));
        (d.join(crate::playlists::PLAYLIST_DIR), PlaylistService::new(lib, sync, journal, served))
    }

    pub fn refused(r: Result<Option<String>, ToggleError>) -> String {
        match r {
            Err(ToggleError::Refused(m)) => m,
            other => panic!("not refused: {other:?}"),
        }
    }
}

#[cfg(test)]
mod undo_tests {
    use super::*;
    use super::testing::{refused, service, service_with};

    /// A service reads the library it was made with, not a global one (the audit, 0.10): two in one process, each
    /// with its own, do not see each other's.
    #[tokio::test]
    async fn a_service_reads_its_own_library_and_not_another() {
        let open = || crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        let (one, other) = (open(), open());
        let (d1, s1) = service_with(Some(one.clone())).await;
        let (d2, s2) = service_with(Some(other)).await;
        s1.create("Chill").await.unwrap();
        let chill = s1.id_of_name("Chill").unwrap();
        one.userdata.set_playlist_shared(&chill, true).unwrap();
        let (d3, none) = service().await;
        assert!(s1.shared_keys().contains(&chill), "shared in its own library");
        assert!(s2.shared_keys().is_empty(), "the other library shares nothing");
        assert!(none.shared_keys().is_empty(), "nor does a service without one");
        for d in [d1, d2, d3] {
            std::fs::remove_dir_all(&d).ok();
        }
    }

    #[tokio::test]
    async fn each_account_has_its_own_playlists() {
        let (d, s) = service().await;
        let ann = s.lib.store_for(Some("ann")).unwrap();
        assert!(ann.view(None).await.unwrap().is_empty(), "none until her first");
        ann.create("Chill").await.unwrap();
        ann.replace_entries_if("Chill", &[], &["A/1.mp3".to_string()]).await.unwrap();
        assert_eq!(ann.entries("Chill").await.unwrap(), ["A/1.mp3"], "Music-relative, as everyone's");
        // The first administrator's are as they were: hers is not among them, and one of the same name is another.
        s.create("Chill").await.unwrap();
        let shown: Vec<(String, bool)> = s.view(None).await.unwrap().into_iter().map(|p| (p.name, p.mine)).collect();
        assert_eq!(shown, [("Chill".to_string(), true)]);
        assert!(s.entries("Chill").await.unwrap().is_empty());
        // Ids tell them apart everywhere (decision 0101); Cratarr's own work covers every account's.
        let top = s.lib.store_for(None).unwrap();
        let (mine, hers) = (top.key("Chill"), ann.key("Chill"));
        assert!(mine != hers && crate::userdata::playlists::is_id(&mine) && crate::userdata::playlists::is_id(&hers));
        let all: Vec<String> = s.lib.all_stores().iter().map(|st| st.key("Chill")).collect();
        assert_eq!(all, [mine.clone(), hers.clone()]);
        // Another's, not shared, is as if there were none, by its id or by name; and it is never another's to change.
        assert!(matches!(s.reach(&hers), Err(ToggleError::UnknownPlaylist)));
        assert!(matches!(s.reach("ann/Chill"), Err(ToggleError::UnknownPlaylist)));
        assert!(matches!(s.set_shared(&hers, true), Err(ToggleError::UnknownPlaylist)));
        assert_eq!(s.reach(&mine).unwrap().name, "Chill", "one's own by its id");
        std::fs::remove_dir_all(d.parent().unwrap()).ok();
    }

    #[tokio::test]
    async fn an_undo_is_undone_by_its_redo_and_happens_only_once() {
        let (d, s) = service().await;
        let lines = |v: &[&str]| v.iter().map(|l| l.to_string()).collect::<Vec<_>>();
        let store = s.lib.store_for(None).unwrap();
        let read = |n: &'static str| {
            let store = store.clone();
            async move { store.text(n).await }
        };
        let made = s.create_with("Chill", &lines(&["A/1.mp3", "A/2.mp3"])).await.unwrap().unwrap();
        let edited = s.edit("Chill", &lines(&["A/1.mp3", "A/2.mp3"]), &lines(&["A/2.mp3"])).await.unwrap().unwrap();
        assert_eq!(read("Chill").await.as_deref(), Some("A/2.mp3\n"));

        let redo = s.undo(&edited).await.unwrap().unwrap();
        assert_eq!(read("Chill").await.as_deref(), Some("A/1.mp3\nA/2.mp3\n"), "the edit undone");
        assert_eq!(refused(s.undo(&edited).await), "That was undone already.");
        let again = s.undo(&redo).await.unwrap().unwrap();
        assert_eq!(read("Chill").await.as_deref(), Some("A/2.mp3\n"), "redone, as a change of its own");
        s.undo(&again).await.unwrap();
        assert_eq!(read("Chill").await.as_deref(), Some("A/1.mp3\nA/2.mp3\n"), "and the redo undone");

        let unmade = s.undo(&made).await.unwrap().unwrap();
        assert_eq!(read("Chill").await, None, "the new playlist set aside");
        s.undo(&unmade).await.unwrap();
        assert_eq!(read("Chill").await.as_deref(), Some("A/1.mp3\nA/2.mp3\n"), "and back");
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
        let store = s.lib.store_for(None).unwrap();
        let nine = vec!["B/9.mp3".to_string()];
        store.set_entries_if("Chill", &[], &nine).await.unwrap();
        assert_eq!(refused(s.undo(&cleared).await), "It changed since; nothing was undone.");
        assert_eq!(refused(s.undo(&made).await), "It changed since; nothing was undone.");
        assert_eq!(store.entries("Chill").await.unwrap(), nine, "left alone");
        store.set_entries_if("Chill", &nine, &[]).await.unwrap();
        s.undo(&cleared).await.unwrap();
        assert_eq!(store.entries("Chill").await.unwrap(), one, "a refused undo can be tried again");

        let renamed = s.rename("Chill", "Calm").await.unwrap().unwrap();
        s.create("Chill").await.unwrap();
        assert_eq!(refused(s.undo(&renamed).await), "It changed since; nothing was undone.", "the old name is taken");
        store.delete("Chill").await.unwrap();
        s.undo(&renamed).await.unwrap();
        assert!(store.has("Chill") && !store.has("Calm"));

        let deleted = s.delete("Chill").await.unwrap().unwrap();
        s.create("Chill").await.unwrap();
        assert_eq!(refused(s.undo(&deleted).await), "It changed since; nothing was undone.", "a new one has the name");
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
    pub(crate) lib: Arc<Library>,
    /// Keeps the user's own server playlists in step (decision 0012).
    pub(crate) sync: Arc<super::sync::SyncService>,
    /// One's own crates (decision 0024).
    pub(crate) crate_stores: CrateStores,
    /// Where every change is kept for its undo (decision 0026).
    pub(crate) journal: Arc<Journal>,
    /// The favourite tracks' lines for the crates (see `favourite_lines`).
    favourites: Arc<FavouritesKept>,
    /// The playlists' table's facts (see `playlist_stats`).
    stats: StatsKept,
    /// Cratarr's own library (decision 0030): its plays, stars and shared playlists, and who asks. `main` hands it; a
    /// test may not.
    pub(crate) served: Option<Arc<crate::serve::Serve>>,
}

/// The account the running request is for, whose favourites the crates show.
pub(crate) fn asking_user(served: Option<&crate::serve::Serve>) -> Option<String> {
    served.and_then(|s| s.current_user())
}

/// Cratarr's own library's songs by their playlist lines, in every folder in use: Music's and the Incoming ones' (a
/// crate draws from both; an Incoming song's line climbs out of Music, `Catalogue::line_of`). Blocking.
pub(crate) fn songs_by_line(serve: &crate::serve::Serve, cat: &crate::catalogue::Catalogue) -> HashMap<String, crate::serve::library::RelSong> {
    let mut out = HashMap::new();
    for folder in cat.folders().ids() {
        for (rel, song) in serve.library.songs_by_rel(folder).unwrap_or_default() {
            if let Some(line) = cat.line_of(folder, &rel) {
                out.insert(line, song);
            }
        }
    }
    // `songs` holds a track's row copy alone: a playlist line on another copy is the same song.
    for f in cat.crate_facts().iter().flat_map(|facts| facts.iter()).filter(|f| !f.lines.is_empty()) {
        if let Some(song) = out.get(&f.rel).cloned() {
            out.extend(f.lines.iter().map(|l| (l.clone(), song.clone())));
        }
    }
    out
}

pub(super) use crate::playlists::line_exists;

/// The account whose playlists and crates the running request's are (decision 0051): another account's, or `None` for
/// the first administrator's (theirs are the ones from before accounts), as for Cratarr's own work.
pub(crate) fn asking_account(served: Option<&crate::serve::Serve>) -> Option<String> {
    let s = served?;
    let me = s.current_user()?;
    (Some(&me) != s.users.owner().as_ref()).then_some(me)
}

/// Each account's crates (decision 0051: one's own, not shared): the first administrator's in `crates.json` as
/// before, another's in `users/<account>/crates.json`. One store each, as each keeps its crates in memory.
pub(crate) struct CrateStores {
    data_dir: Option<String>,
    pub(crate) first: Arc<crates::CrateStore>,
    others: std::sync::Mutex<HashMap<String, Arc<crates::CrateStore>>>,
}

impl CrateStores {
    fn new(data_dir: Option<&str>) -> Self {
        Self { data_dir: data_dir.map(str::to_string), first: Arc::new(crates::CrateStore::new(data_dir)), others: Default::default() }
    }

    pub(crate) fn of(&self, account: Option<&str>) -> Result<Arc<crates::CrateStore>, ToggleError> {
        let Some(account) = account else { return Ok(self.first.clone()) };
        if !crate::playlists::account_folder_ok(account) {
            return Err(ToggleError::Refused(format!("{account} cannot name a folder for its crates.")));
        }
        let mut others = self.others.lock().unwrap_or_else(|p| p.into_inner());
        let store = others.entry(account.to_string()).or_insert_with(|| Arc::new(crates::CrateStore::of_account(self.data_dir.as_deref(), account)));
        Ok(store.clone())
    }

    /// Every account's store and whose it is (`None`: the first administrator's), one nobody has opened since the start too: its
    /// file is all there is of it, and it must read as the others do once someone does.
    pub(crate) fn every(&self) -> Vec<(Option<String>, Arc<crates::CrateStore>)> {
        let users = self.data_dir.iter().filter_map(|d| std::fs::read_dir(std::path::Path::new(d).join("users")).ok()).flatten().flatten();
        for account in users.filter_map(|u| u.file_name().into_string().ok()) {
            // A folder no account could have: nothing can open it.
            let _ = self.of(Some(&account));
        }
        let others = self.others.lock().unwrap_or_else(|p| p.into_inner());
        std::iter::once((None, self.first.clone())).chain(others.iter().map(|(account, store)| (Some(account.clone()), store.clone()))).collect()
    }
}

/// The account a playlist owner's name stands for (`None`: the first administrator).
pub(crate) fn account_of(served: Option<&crate::serve::Serve>, owner: &str) -> Option<String> {
    let first = served.and_then(|s| s.users.owner());
    (Some(owner) != first.as_deref()).then(|| owner.to_string())
}

/// The name shown as the owner of an account's playlists.
fn owner_name(served: Option<&crate::serve::Serve>, account: Option<&str>) -> String {
    account.map(str::to_string).or_else(|| served.and_then(|s| s.users.owner())).unwrap_or_default()
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
    /// Cratarr's own library, whose account asks (`asking_user`).
    served: Option<Arc<crate::serve::Serve>>,
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
    fn load(file: Option<std::path::PathBuf>, served: Option<Arc<crate::serve::Serve>>) -> Self {
        let last = file.as_ref().and_then(|f| std::fs::read(f).ok()).and_then(|b| serde_json::from_slice::<KeptFile>(&b).ok()).map(|k| {
            let now = std::time::Instant::now();
            Kept { at: now.checked_sub(FAVOURITES_KEEP).unwrap_or(now), source: k.source, user: k.user, upto: 0, lines: Some(Arc::new(k.lines.into_iter().collect())) }
        });
        Self { last: std::sync::Mutex::new(last), asking: Arc::default(), file, served }
    }

    /// Keep an answer for the next run. A failure costs only the wait after the next start.
    async fn save(&self, source: &str, user: Option<String>, lines: &Arc<HashSet<String>>) {
        let Some(file) = self.file.clone() else { return };
        let body = KeptFile { source: source.to_string(), user, lines: lines.iter().cloned().collect() };
        let written = crate::run::spawn_blocking(move || -> Result<()> {
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
        let user = asking_user(self.served.as_deref());
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone().filter(|k| k.source == source && k.user == user)
    }

    /// Ask the server (the caller holds `asking`). When it does not answer, what was heard before is kept, for another
    /// minute before asking again.
    async fn ask(&self, c: Arc<crate::catalogue::Catalogue>, source: &str, from: &Arc<dyn source::Source>) -> Kept {
        let (upto, user) = (crates::favourite_changes_made(), asking_user(self.served.as_deref()));
        let lines = match from.favorite_tracks().await {
            Ok(ids) => {
                let s = source.to_string();
                match crate::run::spawn_blocking(move || c.lines_of(&s, &ids)).await {
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
        let lines = crate::run::spawn_blocking(move || {
            let mut lines = (*lines).clone();
            for (track, on) in changes {
                let of = c.lines_of(&s, std::slice::from_ref(&track))?;
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
    pub fn new(lib: Arc<Library>, sync: Arc<super::sync::SyncService>, journal: Arc<Journal>, served: Option<Arc<crate::serve::Serve>>) -> Self {
        let crate_stores = CrateStores::new(lib.data_dir());
        let favourites = Arc::new(FavouritesKept::load(lib.data_dir().map(|d| std::path::PathBuf::from(d).join("favourites.json")), served.clone()));
        let s = Self { lib, sync, crate_stores, journal, favourites, stats: StatsKept::default(), served };
        s.moved_to_ids();
        s
    }

    /// Cratarr's own library, as the services built on this one (Play On, Playing, Insights) read it.
    pub(crate) fn served(&self) -> Option<Arc<crate::serve::Serve>> {
        self.served.clone()
    }

    /// The account whose playlists and crates the running request's are (`asking_account`).
    pub fn asking_account(&self) -> Option<String> {
        asking_account(self.served.as_deref())
    }

    /// Every playlist's facts for the Playlists page's table (decision 0042), by id. Worked out again only when a
    /// playlist's file, the library's build or the owner's plays changed since.
    pub async fn playlist_stats(&self) -> Result<std::collections::BTreeMap<String, PlaylistStats>, ToggleError> {
        let lists = self.visible_lines().await?;
        let Some(serve) = self.served.clone() else { return Ok(Default::default()) };
        let stamps: String = lists.iter().map(|(id, changed, lines)| format!("{id}:{changed}:{};", lines.len())).collect();
        let s = serve.clone();
        // Read here: the blocking task does not carry the request's account.
        let me = serve.current_user();
        let key_and_owner = crate::run::spawn_blocking(move || {
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
        let cat = self.lib.catalogue();
        let worked = crate::run::spawn_blocking(move || {
            let songs = songs_by_line(&serve, &cat);
            let plays = me.and_then(|me| serve.userdata.song_plays(&me).ok());
            lists.into_iter().map(|(id, changed, lines)| (id, playlist_stats(&lines, &songs, plays.as_ref(), changed))).collect::<std::collections::BTreeMap<_, _>>()
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        let answer = worked.clone();
        *self.stats.lock().unwrap_or_else(|p| p.into_inner()) = Some((key, worked));
        Ok(answer)
    }

    /// Keep a change in the journal; its undo token (decision 0026). A journal that cannot be written costs only the
    /// undo: the change itself is done.
    pub(crate) fn keep(&self, change: Change) -> Option<String> {
        match self.journal.record_change(super::library::epoch_now(), &change, asking_account(self.served.as_deref()).as_deref()) {
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
        if account != asking_account(self.served.as_deref()) {
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
            Change::Excluded { before, after } => {
                let crates = self.crate_stores.of(account)?;
                if crates.excluded().await? != after {
                    return Err(changed());
                }
                crates.set_excluded(before.clone()).await?;
                self.excluded_tracks_changed();
                Ok(Change::Excluded { before: after, after: before })
            }
            Change::Added { crate_id, before, after } => {
                let crates = self.crate_stores.of(account)?;
                if crates.added(&crate_id).await? != after {
                    return Err(changed());
                }
                crates.set_added(&crate_id, before.clone()).await?;
                Ok(Change::Added { crate_id, before: after, after: before })
            }
            Change::ArtistOut { user, key, name, out } => {
                let serve = self.served.clone().ok_or_else(changed)?;
                let (u, k, n) = (user.clone(), key.clone(), name.clone());
                crate::run::spawn_blocking(move || -> Result<(), ToggleError> {
                    if serve.userdata.excluded_artists(&u)?.contains(&k) != out {
                        return Err(changed());
                    }
                    if out {
                        serve.userdata.include_artist(&u, &k)?;
                    } else {
                        serve.userdata.exclude_artist(&u, &k, &n)?;
                    }
                    Ok(())
                })
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))??;
                Ok(Change::ArtistOut { user, key, name, out: !out })
            }
            Change::Tempo { tracks } => {
                // Each track's BPM tag as the change left it, else "it changed since"; then the tags before.
                if !crate::serve::access::administering() {
                    return Err(ToggleError::Forbidden("Adjusting a track's BPM is for administrators.".into()));
                }
                let cat = self.lib.catalogue();
                let back = crate::run::spawn_blocking(move || -> Result<Vec<super::tempo::TempoChange>, ToggleError> {
                    for t in &tracks {
                        let (_, _, path) = super::tempo::place(&cat, &t.source, &t.id).ok_or_else(changed)?;
                        let now = crate::retag::bpm(&path).and_then(|v| v.trim().parse::<f64>().ok()).map(|b| b.round() as u32);
                        if now != t.after {
                            return Err(changed());
                        }
                    }
                    let mut redo = Vec::new();
                    for t in tracks {
                        super::tempo::write(&cat, &t.source, &t.id, t.before)?;
                        redo.push(super::tempo::TempoChange { before: t.after, after: t.before, ..t });
                    }
                    Ok(redo)
                })
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))??;
                Ok(Change::Tempo { tracks: back })
            }
        }
    }

    /// Every playlist, marked where it holds `entry` (the playing track's line), with its links.
    /// The asking account's own, and the ones others share (decision 0051), each by its id (decision 0101).
    pub async fn view(&self, entry: Option<&str>) -> Result<Vec<PlaylistView>> {
        let Ok(own) = self.lib.store_for(asking_account(self.served.as_deref()).as_deref()) else { return Ok(Vec::new()) };
        let shared = self.shared_keys();
        let mut list = own.view(entry).await?;
        let (mut links, mut spoken) = (self.sync.links(), self.sync.spoken());
        let mine = owner_name(self.served.as_deref(), own.account());
        for p in &mut list {
            // Kept in step with other servers (by its name there): the first administrator's only.
            if own.account().is_none() {
                p.links = links.remove(&p.name).unwrap_or_default();
            }
            p.spoken = spoken.remove(&p.id).unwrap_or_default();
            (p.owner, p.shared, p.mine) = (mine.clone(), shared.contains(&p.id), true);
        }
        for store in self.others(&own) {
            let owner = owner_name(self.served.as_deref(), store.account());
            for mut p in store.view(entry).await? {
                if shared.contains(&p.id) {
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
        let Some(serve) = self.served.clone() else { return HashSet::new() };
        serve.userdata.shared_playlists().inspect_err(|e| tracing::warn!("the shared playlists not read: {e:#}")).unwrap_or_default()
    }

    /// `id` to read, as the asking account may (decision 0051): a playlist's id (decision 0101), its own or another's
    /// shared one (one not shared is as if there were none); else a name: its own playlist by name, another's as
    /// `owner/name` (by-name asks: Play On's, a Subsonic id from before 0.16).
    pub fn reach(&self, id: &str) -> Result<Reached, ToggleError> {
        let me = asking_account(self.served.as_deref());
        let found = if crate::userdata::playlists::is_id(id) { self.lib.playlists_db().playlist_by_id(id)? } else { None };
        let (account, name) = match found {
            Some(p) => ((!p.owner.is_empty()).then_some(p.owner), p.name),
            None => match id.split_once('/') {
                Some((owner, name)) => (account_of(self.served.as_deref(), owner), name.to_string()),
                None => (me.clone(), id.to_string()),
            },
        };
        let store = self.lib.store_for(account.as_deref())?;
        let shared = self.shared_keys().contains(&store.key(&name));
        let mine = account == me;
        if !mine && !shared {
            return Err(ToggleError::UnknownPlaylist);
        }
        Ok(Reached { owner: owner_name(self.served.as_deref(), store.account()), store, name, shared, mine })
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
    pub async fn visible_lines(&self) -> Result<Vec<(String, i64, Vec<String>)>, ToggleError> {
        let Ok(own) = self.lib.store_for(asking_account(self.served.as_deref()).as_deref()) else { return Ok(Vec::new()) };
        let mut out = own.lines_of_all().await?;
        let shared = self.shared_keys();
        for store in self.others(&own) {
            out.extend(store.lines_of_all().await?.into_iter().filter(|(id, _, _)| shared.contains(id)));
        }
        Ok(out)
    }

    /// The asking account's playlists' folder as the change log keeps it (decision 0087): its name, '' the first
    /// administrator's.
    pub fn own_folder(&self) -> String {
        asking_account(self.served.as_deref()).unwrap_or_default()
    }

    /// An account's tracks excluded everywhere changed under it: logged, as a change of its own is.
    pub(crate) fn excluded_tracks_moved(&self, account: Option<&str>) {
        let Some(serve) = self.served.as_deref() else { return };
        if let Some(account) = account.map(str::to_string).or_else(|| serve.users.owner())
            && let Err(e) = serve.userdata.excluded_changed(&account, "tracks")
        {
            tracing::warn!("the tracks excluded: their change not logged: {e:#}");
        }
    }

    /// The asking account's tracks excluded everywhere changed: logged (decision 0087).
    pub(crate) fn excluded_tracks_changed(&self) {
        if let Some(serve) = self.served.as_deref()
            && let Some(me) = serve.current_user()
            && let Err(e) = serve.userdata.excluded_changed(&me, "tracks")
        {
            tracing::warn!("the tracks excluded: their change not logged: {e:#}");
        }
    }

    /// Shares one's own playlist with everyone, or stops (decision 0051): they see it, play it and copy it.
    pub fn set_shared(&self, id: &str, on: bool) -> Result<(), ToggleError> {
        let (store, name) = self.own(id)?;
        if !store.has(&name) {
            return Err(ToggleError::UnknownPlaylist);
        }
        let serve = self.served.clone().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        // As the account's permissions say (decision 0051); stopping is always one's own to do.
        let may = serve.current_user().and_then(|u| serve.users.get(&u)).is_none_or(|u| u.permissions.share);
        if on && !may {
            return Err(ToggleError::Forbidden("This account may not share playlists.".into()));
        }
        let key = store.key(&name);
        // No longer shared: the others are told it is gone (decision 0087).
        if serve.userdata.set_playlist_shared(&key, on)? && !on {
            serve.playlist_gone_for_others(&key);
        }
        Ok(())
    }

    /// `name`, or the first free one after it ("Evening 2") among the asking account's own playlists.
    pub async fn free_own_name(&self, name: &str) -> Result<String, ToggleError> {
        let own = self.lib.store_for(asking_account(self.served.as_deref()).as_deref())?;
        let taken: HashSet<String> = own.view(None).await?.into_iter().map(|p| p.name.to_lowercase()).collect();
        Ok(crate::playlists::free_name(name, |n| taken.contains(&n.to_lowercase())))
    }

    /// A copy of a playlist (another's shared one, or one's own) among one's own, under a free name. Returns its id and
    /// the undo token.
    pub async fn copy(&self, id: &str) -> Result<(String, Option<String>), ToggleError> {
        let from = self.reach(id)?;
        let entries = from.store.entries(&from.name).await?;
        let own = self.lib.store_for(asking_account(self.served.as_deref()).as_deref())?;
        let taken: HashSet<String> = own.view(None).await?.into_iter().map(|p| p.name.to_lowercase()).collect();
        let name = crate::playlists::free_name(&from.name, |n| taken.contains(&n.to_lowercase()));
        let undo = self.create_with(&name, &entries).await?;
        Ok((own.key(&name), undo))
    }

    /// The names of the asking account's own playlists.
    pub async fn own_names(&self) -> Result<Vec<String>, ToggleError> {
        Ok(self.own_store()?.view(None).await?.into_iter().map(|p| p.name).collect())
    }

    /// The id of the asking account's own playlist of that name (decision 0101): what a made, copied or renamed playlist
    /// is answered by.
    pub fn id_of_name(&self, name: &str) -> Result<String, ToggleError> {
        Ok(self.own_store()?.key(name))
    }

    /// The id a key 0.15 knew a playlist by leads to (its name, `owner/name` another's, a name it had before a
    /// rename): what a page or a Subsonic client kept of it.
    pub fn moved(&self, old: &str) -> Option<String> {
        self.lib.playlists_db().moved_playlist(old).ok().flatten()
    }

    /// Every key 0.15 knew a playlist the asking account sees by, and the id it leads to (`GET /api/playlists/moved`):
    /// never another's not shared.
    pub fn all_moved(&self) -> Result<HashMap<String, String>, ToggleError> {
        let mut all = self.lib.playlists_db().playlists_moved()?;
        all.retain(|_, id| self.reach(id).is_ok());
        Ok(all)
    }

    /// What 0.15 kept by a playlist's key moves to its id (decision 0101), at the start, until none is left (one whose
    /// playlist is not brought in yet waits for it): its spoken names, and its suggestions' crate (`like:list:<key>`,
    /// in its owner's crates: what was turned down there and its fit).
    fn moved_to_ids(&self) {
        let db = self.lib.playlists_db().clone();
        // What moved, added to the migration report's playlists part (decision 0101) where anything did.
        let told = |db: &crate::userdata::UserData, field: &str, n: usize| {
            let fields = serde_json::Map::from_iter([(field.to_string(), serde_json::json!(n))]);
            if let Err(e) = db.add_to_migration_part("playlists", &fields) {
                tracing::warn!("the migration report: {field} not kept: {e:#}");
            }
        };
        let mut spoken = 0;
        for (old, names) in self.sync.spoken() {
            if crate::userdata::playlists::is_id(&old) {
                continue;
            }
            let Some(id) = self.moved(&old) else { continue };
            match self.sync.set_spoken(&id, &names).and_then(|_| self.sync.set_spoken(&old, &[])) {
                Ok(()) => spoken += 1,
                Err(e) => tracing::warn!("the spoken names of {old} not moved to its id: {e:#}"),
            }
        }
        if spoken > 0 {
            told(&db, "spoken_names_moved", spoken);
        }
        let moved = self.lib.playlists_db().playlists_moved().unwrap_or_default();
        let mut crates = Vec::new();
        for (old, id) in moved {
            // The crate was named by the playlist as its owner knew it: its name.
            let Some(owner) = self.lib.playlists_db().playlist_owner(&id).ok().flatten() else { continue };
            let name = old.split_once('/').map_or(old.as_str(), |(_, n)| n).to_string();
            let Ok(store) = self.crate_stores.of((!owner.is_empty()).then_some(owner.as_str())) else { continue };
            let like = |key: &str| format!("{}{}{key}", crates::LIKE, crates::LIST);
            crates.push((store, like(&name), like(&id)));
        }
        if crates.is_empty() {
            return;
        }
        crate::run::spawn(async move {
            let mut suggestions = 0;
            for (store, old, new) in crates {
                let moved = async {
                    let (left, fit) = (store.left_out(&old).await?, store.fit(&old).await?);
                    let any = !left.is_empty() || !fit.is_empty();
                    if !left.is_empty() {
                        store.set_left_out(&new, left).await?;
                        store.set_left_out(&old, crates::LeftOut::default()).await?;
                    }
                    if !fit.is_empty() {
                        store.set_fit(&new, fit).await?;
                        store.set_fit(&old, crates::Fit::default()).await?;
                    }
                    anyhow::Ok(any)
                };
                match moved.await {
                    Ok(true) => suggestions += 1,
                    Ok(false) => {}
                    Err(e) => tracing::warn!("{old}: what was turned down for it not moved to {new}: {e:#}"),
                }
            }
            if suggestions > 0 {
                told(&db, "suggestion_crates_moved", suggestions);
            }
        });
    }

    /// The playlist meant by what was said (decision 0015), by its name or one it is spoken as; none when nothing fits.
    /// Its tracks are chosen as for any play (app::play::by_name).
    pub async fn spoken(&self, said: &str) -> Result<Option<String>, ToggleError> {
        let list = self.view(None).await?;
        let named: Vec<crate::voice::Named> = list.iter().map(|p| crate::voice::Named { name: &p.name, spoken: &p.spoken }).collect();
        // By its name, and back to its id: one's own first, before another's of that name.
        let found = crate::voice::find(crate::voice::spoken_name(said), &named);
        Ok(found.and_then(|n| list.iter().find(|p| p.name == n)).map(|p| p.id.clone()))
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

    /// Why none of a playlist's tracks can be played on the server: usually the catalogue is still matching files to
    /// it (a first fill takes minutes).
    pub async fn why_none(&self, want: Option<&str>) -> Result<String, ToggleError> {
        let sources = self.lib.connected();
        let (active, _) = want
            .and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .cloned()
            .ok_or_else(|| ToggleError::CannotPlay("no server is connected".into()))?;
        let c = self.lib.catalogue();
        let status = crate::run::spawn_blocking(move || c.status()).await.ok().and_then(Result::ok);
        Ok(match status {
            Some(st) if st.linked.get(&active.cfg.id).copied().unwrap_or(0) < st.files => format!(
                "Cratarr is still matching your music with {} ({} of {} tracks); try again in a few minutes",
                label_of(&active.cfg),
                st.linked.get(&active.cfg.id).copied().unwrap_or(0),
                st.files
            ),
            _ => format!("none of this playlist's tracks are on {}", label_of(&active.cfg)),
        })
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
        let (entries, added) = r.store.entries_with_ids(&r.name).await?;
        let source = self.rows_source(want);
        let led = r.store.led_entries(&r.name).await?;
        let mut rows = self.rows_as(entries.lines(), led, &source).await?;
        // When each entry came (the panel's orders) and its id (decision 0087: what operations name it by), as the
        // playlist keeps them.
        let ids = entries.ids();
        for (i, row) in rows.iter_mut().enumerate() {
            row.added = added.get(i).copied().flatten();
            row.entry_id = ids.get(i).cloned();
        }
        // A missing entry the catalogue has no file of any more, by what its track was (decision 0101 point 11).
        if rows.iter().any(|row| row.missing && row.name.is_none()) {
            let tracks = r.store.tracks(&r.name).await?;
            let wanted: Vec<(usize, i64)> = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.missing && row.name.is_none())
                .filter_map(|(i, _)| Some((i, tracks.get(i)?.as_deref()?.strip_prefix(crate::serve::ids::SONG)?.parse().ok()?)))
                .collect();
            let cat = self.lib.catalogue();
            let found = crate::run::spawn_blocking(move || wanted.into_iter().filter_map(|(i, t)| Some((i, cat.remembered(t)?))).collect::<Vec<_>>())
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            for (i, was) in found {
                let row = &mut rows[i];
                (row.name, row.artist, row.album) = (was.title, row.artist.take().or(was.artist), row.album.take().or(was.album));
                row.duration_secs = row.duration_secs.or(was.duration_ms.map(|ms| ms as f64 / 1000.0));
            }
        }
        // The user's plays of each track, as the user data keeps them.
        if let Some(serve) = self.served.clone()
            && let Some(me) = serve.current_user()
        {
            let rs = rows.iter().map(|r| r.id.clone().unwrap_or_default()).collect::<Vec<_>>();
            let plays = crate::run::spawn_blocking(move || serve.userdata.play_stats(&me, &rs).unwrap_or_default())
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            for row in rows.iter_mut() {
                if let Some((count, last)) = row.id.as_ref().and_then(|t| plays.get(t)) {
                    row.play_count = Some(*count);
                    row.last_played = *last;
                }
            }
        }
        Ok((source, rows))
    }

    /// The server a playlist's rows name their tracks on: `want`, else the first connected.
    pub(super) fn rows_source(&self, want: Option<&str>) -> String {
        let sources = self.lib.connected();
        want.and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .map(|(a, _)| a.cfg.id.clone())
            .unwrap_or_default()
    }

    /// Rows for these lines (a playlist's, or a page of a crate's), with what the catalogue knows of their files.
    pub async fn rows_of(&self, entries: Vec<String>, source: &str) -> Result<Vec<PlaylistRow>, ToggleError> {
        self.rows_as(entries.clone(), entries, source).await
    }

    /// `rows_of`, each entry read at the line beside it in `at` (`Store::led_entries`): the entry stays what an edit names.
    async fn rows_as(&self, entries: Vec<String>, at: Vec<String>, source: &str) -> Result<Vec<PlaylistRow>, ToggleError> {
        let (c, sid, served) = (self.lib.catalogue(), source.to_string(), self.served.clone());
        let rows = crate::run::spawn_blocking(move || -> Result<Vec<PlaylistRow>> {
            let facts = c.line_rows(&sid, &at)?;
            // Cratarr's own tracks with their album, as the library has it (the app's downloads are told it, and find
            // their album and its cover by it).
            let (albums, album_tracks): (HashMap<String, String>, HashMap<String, u32>) = match served {
                Some(serve) if sid == crate::source::cratarr::KIND.id => {
                    let ids: Vec<String> = facts.iter().filter_map(|f| f.track_id.clone()).collect();
                    let albums = serve.library.albums_of(&ids)?;
                    // Each album's size, for the Records view's "3 of 10" (the owner, 2026-10-30).
                    let mut of: Vec<String> = albums.values().cloned().collect();
                    of.sort();
                    of.dedup();
                    let sizes = serve.library.albums_by_id(&of)?.into_iter().map(|a| (a.id, a.song_count)).collect();
                    (albums, sizes)
                }
                _ => Default::default(),
            };
            Ok(entries
                .into_iter()
                .zip(at)
                .zip(facts)
                .map(|((entry, at), f)| PlaylistRow {
                    album_type: None,
                    album_secondary: Vec::new(),
                    entry_id: None,
                    album_id: f.track_id.as_ref().and_then(|t| albums.get(t).cloned()),
                    album_tracks: f.track_id.as_ref().and_then(|t| albums.get(t)).and_then(|a| album_tracks.get(a).copied()),
                    missing: !line_exists(&c, &at),
                    file: at.rsplit('/').next().unwrap_or(&at).to_string(),
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
                    has_versions: None,
                    work: None,
                })
                .collect())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        // Each track with its record's type (GOTO.S1).
        let Some(serve) = self.served() else { return Ok(rows) };
        let rows = crate::run::spawn_blocking(move || {
            let mut rows = rows;
            serve.with_types(&mut rows);
            rows
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(rows)
    }

    /// An export's plan (decision 0007): the playlists' lines as files, each file once, with its size, the tags its name
    /// needs and its format for the estimates. Each line's file wherever the catalogue places it: the Music folder's, or
    /// another library's for a line that climbs out of it (an Incoming track), never outside the catalogue's folders.
    /// A line on a copy in a library `who` has off goes by its track to the copy that plays for them. Read from the disk
    /// and the catalogue; no request to the server.
    pub async fn export_plan(&self, names: &[String], who: &Listener) -> Result<crate::export::Plan, ToggleError> {
        self.lib.music().ok_or(ToggleError::NoMusicFolder)?;
        let mut lists = Vec::new();
        for n in names {
            let r = self.reach(n)?;
            lists.push((r.name.clone(), r.store.entries(&r.name).await?));
        }
        let (c, who) = (self.lib.catalogue(), who.clone());
        let plan = crate::run::spawn_blocking(move || {
            let folders = c.folders();
            let file_of_line = |line: &str| {
                let (folder, rel) = c.place_of_line(line)?;
                if !who.off.contains(&folder) {
                    return Some((folder, rel));
                }
                c.track_of(folder, &rel).and_then(|t| who.among(t, c.files_of(t))).or(Some((folder, rel)))
            };
            let place = |line: &str| {
                let (folder, rel) = file_of_line(line)?;
                Some((folders.get(folder)?.clone(), rel))
            };
            crate::export::plan(lists, place, |rels| {
                let rows = c.line_rows("", rels)?;
                Ok(rels
                    .iter()
                    .zip(rows)
                    .map(|(rel, row)| {
                        let (format, kbps) = file_of_line(rel).map(|(f, r)| c.format_of(f, &r)).unwrap_or_default();
                        export_facts(row, format, kbps)
                    })
                    .collect())
            })
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        Ok(plan)
    }

    /// An export's plan for a list of tracks by their servers' ids (decision 0058: an album, an artist's, a selection,
    /// a queue): each track's file wherever the catalogue has it (the Music folder or another library's), with what it
    /// knows of it. A track it cannot place is left out, named in the report by its server and id; so is a Bandcamp
    /// preview's.
    pub async fn export_tracks(&self, name: &str, tracks: Vec<(String, String)>, who: &Listener) -> Result<crate::export::Plan, ToggleError> {
        let who = who.clone();
        self.export_places(name, move |c: &crate::catalogue::Catalogue| {
            tracks.iter().map(|(s, id)| (format!("{s} {id}"), export_place(c, &who, s, id))).collect()
        })
        .await
    }

    /// An export's plan for files (folder, rel), each with the label the report names it by when it cannot be placed.
    async fn export_places(
        &self,
        name: &str,
        places: impl FnOnce(&crate::catalogue::Catalogue) -> Vec<(String, Option<(i64, String)>)> + Send + 'static,
    ) -> Result<crate::export::Plan, ToggleError> {
        let c = self.lib.catalogue();
        let name = name.to_string();
        let plan = crate::run::spawn_blocking(move || -> anyhow::Result<crate::export::Plan> {
            let folders = c.folders();
            let (labels, places): (Vec<String>, Vec<Option<(i64, String)>>) = places(&*c).into_iter().unzip();
            let rows = c.place_rows(&places)?;
            let found = labels
                .into_iter()
                .zip(places)
                .zip(rows)
                .map(|((label, place), row)| {
                    let (folder, rel) = place.ok_or(label)?;
                    let path = folders.get(folder).ok_or_else(|| rel.clone())?.join(&rel);
                    let (format, kbps) = c.format_of(folder, &rel);
                    Ok((path, rel, export_facts(row, format, kbps)))
                })
                .collect();
            Ok(crate::export::plan_files(&name, found))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        Ok(plan)
    }

    /// Where an export makes its converted files before sending them: the data folder's `export`, else the system's
    /// temporary folder.
    pub fn export_work(&self) -> std::path::PathBuf {
        match self.lib.data_dir() {
            Some(d) => std::path::Path::new(d).join("export"),
            None => std::env::temp_dir().join("cratarr-export"),
        }
    }

    /// The server (`want`, else the first connected) and its id.
    pub(super) fn source_of(&self, want: Option<&str>) -> Result<(String, Arc<dyn source::Source>), ToggleError> {
        let sources = self.lib.connected();
        want.and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .map(|(a, s)| (a.cfg.id.clone(), s.clone()))
            .ok_or_else(|| ToggleError::Refused("No server is connected.".into()))
    }

    /// Tracks' song ids, by their ids on a server. `going`: on their way into a list, so only what is there. Coming back
    /// out, a song id stands by itself: what was left out or excluded can be put back after its file went.
    pub(crate) async fn tracks_of(&self, want: Option<&str>, ids: &[String], going: bool) -> Result<HashSet<String>, ToggleError> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }
        let (sid, _) = self.source_of(want)?;
        let (c, ids) = (self.lib.catalogue(), ids.to_vec());
        let song = move |id: &String| match c.track_of_source(&sid, id) {
            Some(track) => Some(crate::serve::ids::song(track)),
            None => crate::serve::ids::track_of(id).filter(|_| !going).map(crate::serve::ids::song),
        };
        Ok(crate::run::spawn_blocking(move || ids.iter().filter_map(song).collect()).await.map_err(|e| anyhow::anyhow!("{e}"))?)
    }

    /// Tracks as rows on a server, in the order given: the lists of what is left out and excluded. A track whose file went
    /// shows what it was and keeps its song id, so it can be put back.
    pub(crate) async fn track_rows(&self, songs: Vec<String>, source: &str) -> Result<Vec<PlaylistRow>, ToggleError> {
        let (cat, ids) = (self.lib.catalogue(), songs.clone());
        let (lines, gone) = crate::run::spawn_blocking(move || {
            let of = |song: &String| crate::serve::playlists::line_of_song(&cat, song);
            let lines: Vec<String> = ids.iter().map(|song| of(song).unwrap_or_else(|| song.clone())).collect();
            let gone: HashMap<String, crate::catalogue::Remembered> =
                ids.iter().filter(|song| of(song).is_none()).filter_map(|song| Some((song.clone(), cat.remembered(crate::serve::ids::track_of(song)?)?))).collect();
            (lines, gone)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut rows = self.rows_of(lines, source).await?;
        for (row, song) in rows.iter_mut().zip(songs) {
            if let Some(was) = gone.get(&song) {
                row.name = was.title.clone();
                row.artist = was.artist.clone();
                row.album = was.album.clone();
                row.duration_secs = was.duration_ms.map(|ms| ms as f64 / 1000.0);
                row.id = Some(song);
            }
        }
        Ok(rows)
    }

    /// Tracks' tempo as Adjust BPM shows it (app::tempo): each one's BPM now and Cratarr's measure.
    pub async fn tempo_of(&self, source: &str, ids: &[String]) -> Result<Vec<super::tempo::TrackTempo>, ToggleError> {
        let (cat, source, ids) = (self.lib.catalogue(), source.to_string(), ids.to_vec());
        Ok(crate::run::spawn_blocking(move || ids.iter().filter_map(|id| super::tempo::tempo_of(&cat, &source, id)).collect())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?)
    }

    /// Adjust BPM (app::tempo): an administrator's, the library's for everyone. Writes each track's new BPM into its
    /// file and keeps the change for its undo; tracks with nothing to go from are left as they are. Returns the tracks'
    /// tempo after it, and the undo's token.
    pub async fn adjust_tempo(&self, source: &str, ids: &[String], how: super::tempo::Adjust) -> Result<(Vec<super::tempo::TrackTempo>, Option<String>), ToggleError> {
        if !crate::serve::access::administering() {
            return Err(ToggleError::Forbidden("Adjusting a track's BPM is for administrators.".into()));
        }
        debug_assert!(source != crate::source::cratarr::KIND.id, "Cratarr's own tracks' tempo is a run: tempo_run");
        let (cat, src, list) = (self.lib.catalogue(), source.to_string(), ids.to_vec());
        let changes = crate::run::spawn_blocking(move || -> Result<Vec<super::tempo::TempoChange>, ToggleError> {
            let mut changes = Vec::new();
            for id in &list {
                let Some(now) = super::tempo::tempo_of(&cat, &src, id) else { continue };
                let Some(to) = super::tempo::adjusted(&now, how) else { continue };
                let (_, _, path) = super::tempo::place(&cat, &src, id).ok_or_else(|| ToggleError::Refused("A track's file is not known.".into()))?;
                let before = crate::retag::bpm(&path).and_then(|v| v.trim().parse::<f64>().ok()).map(|b| b.round() as u32);
                if before == Some(to) {
                    continue;
                }
                super::tempo::write(&cat, &src, id, Some(to)).map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
                changes.push(super::tempo::TempoChange { source: src.clone(), id: id.clone(), before, after: Some(to) });
            }
            Ok(changes)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        let undo = if changes.is_empty() { None } else { self.keep(Change::Tempo { tracks: changes }) };
        Ok((self.tempo_of(source, ids).await?, undo))
    }

    /// One's own playlists' names: the membership axis' values.
    pub async fn own_playlist_names(&self) -> Result<Vec<String>, ToggleError> {
        Ok(self.own_store()?.snapshot().await?.into_iter().map(|(name, _, _)| name).collect())
    }

    /// The favourite tracks' lines, from the media server. What was heard last is answered at once, with Cratarr's own
    /// changes since applied; when it is a minute old, it is asked for again in the background. Only the first asking
    /// (nothing heard yet from this server) waits. One asks at a time: Jellyfin took a minute for 4,786 favourites
    /// (2026-10-14), and asking again while it works makes it slower. A server that does not answer is asked again a
    /// minute later.
    pub(crate) async fn favourite_lines(&self, sid: &str, source: &Arc<dyn source::Source>) -> Option<HashSet<String>> {
        let f = &self.favourites;
        let kept = match f.kept(sid) {
            Some(k) => {
                if k.at.elapsed() >= FAVOURITES_KEEP
                    && let Ok(turn) = f.asking.clone().try_lock_owned()
                {
                    let (f, c, s, from) = (f.clone(), self.lib.catalogue(), sid.to_string(), source.clone());
                    // For the account asking: their favourites.
                    crate::run::spawn(crate::serve::who::carried(async move {
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
        let entries = r.store.led_entries(&r.name).await?;
        let sources = self.lib.connected();
        let (active, source) = want
            .and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .cloned()
            .ok_or_else(|| ToggleError::CannotPlay("no server is connected".into()))?;
        let (c, sid) = (self.lib.catalogue(), active.cfg.id.clone());
        let found = crate::run::spawn_blocking(move || c.line_track_ids(&sid, &entries))
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
    pub(crate) fn own_store(&self) -> Result<Arc<Store>, ToggleError> {
        self.lib.store_for(asking_account(self.served.as_deref()).as_deref())
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

    /// A missing line pointed at a track found for it (Health's missing tracks; `Store::relink` checks both): the
    /// playlists that changed and the undo token, as a repair's.
    pub async fn relink(&self, one: Repair) -> Result<(Vec<String>, Option<String>), ToggleError> {
        let store = self.own_store()?;
        let edits = store.relink(&one).await?;
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
        let folders: Vec<String> = crate::run::spawn_blocking(move || -> Result<Vec<String>> {
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
                let (files, cover) = crate::run::spawn_blocking(move || read_folder(&m, &f)).await??;
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
                // Every account's playlists that hold it, as the page shows them: by name, another account's as
                // `account/name`.
                let mut lines = Vec::new();
                for store in &stores {
                    let shown = |name: String| store.account().map_or_else(|| name.clone(), |a| format!("{a}/{name}"));
                    lines.extend(store.lines_in_folder(folder).await?.into_iter().map(|(name, n)| (shown(name), n)));
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
    /// page asked and the user chose Add again (decision 0025, amended 2026-10-16). With `only_new` (0.9.4: many ticked
    /// tracks into a playlist in one call) those it holds already, and repeats among them, are left out instead. Tracks
    /// outside the library are left out. Returns how many were added and the undo token (none when nothing was).
    pub async fn append_tracks(&self, id: &str, source_id: &str, ids: &[String], only_new: bool) -> Result<(usize, Option<String>), ToggleError> {
        // A Bandcamp preview is never in a playlist (decision 0071): it is bought on Bandcamp.
        if source_id == crate::bandcamp::SOURCE {
            return Err(ToggleError::Refused(crate::bandcamp::NOT_KEPT.into()));
        }
        let gone = || ToggleError::Refused("Those tracks are not on a connected server.".into());
        let (a, s) = self.lib.connected().into_iter().find(|(a, _)| a.cfg.id == source_id).ok_or_else(gone)?;
        let (store, name) = self.own(id)?;
        let folders = self.lib.catalogue().folders();
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
        if only_new {
            let mut seen: HashSet<String> = store.entries(&name).await?.into_iter().collect();
            entries.retain(|e| seen.insert(e.clone()));
            if entries.is_empty() {
                return Ok((0, None));
            }
        }
        let edit = store.append(&name, &entries).await?;
        self.tell_sources(&store, vec![(name, PlaylistChange::Edited)]);
        Ok((entries.len(), self.keep_texts(vec![edit])))
    }

    /// The asking account's playlists and the ones others share, by id, with their lines and missing-track counts
    /// (the OpenSubsonic playlists, decision 0030).
    pub async fn snapshot(&self) -> Result<Vec<(String, Vec<String>, usize)>, ToggleError> {
        let Ok(own) = self.own_store() else { return Ok(Vec::new()) };
        let mut out: Vec<(String, Vec<String>, usize)> = own.snapshot().await?.into_iter().map(|(n, l, d)| (own.key(&n), l, d)).collect();
        let shared = self.shared_keys();
        for store in self.others(&own) {
            out.extend(store.snapshot().await?.into_iter().map(|(n, l, d)| (store.key(&n), l, d)).filter(|(id, _, _)| shared.contains(id)));
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

    /// One playlist's lines as they play now (`Store::led_entries`).
    pub async fn led_entries(&self, id: &str) -> Result<Vec<String>, ToggleError> {
        let r = self.reach(id)?;
        Ok(r.store.led_entries(&r.name).await?)
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

    /// The server (`want`, else the first connected) a playlist favourite is kept on, and whether it keeps them by the
    /// playlists' ids (Cratarr's own, decision 0101), not by its copies' names (a media server's).
    fn favourites_source(&self, want: Option<&str>) -> Result<(Arc<dyn source::Source>, bool), ToggleError> {
        let sources = self.lib.connected();
        want.and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .map(|(a, s)| (s.clone(), a.cfg.kind == crate::source::cratarr::KIND.id))
            .ok_or_else(|| ToggleError::Refused("No server is connected.".into()))
    }

    /// Which playlists (by id) are favourites, as Finamp's playlist hearts (decision 0023); a media server's by its
    /// copies' names, one's own of those names.
    pub async fn favorites(&self, want: Option<&str>) -> Result<std::collections::BTreeMap<String, bool>, ToggleError> {
        let (s, by_id) = self.favourites_source(want)?;
        let hearts = s.playlist_favorites().await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))?;
        if by_id {
            return Ok(hearts);
        }
        let own = self.own_store()?;
        Ok(hearts.into_iter().map(|(name, on)| (own.key(&name), on)).collect())
    }

    /// Makes a playlist (by id) a favourite or not (every copy on the server).
    pub async fn set_favorite(&self, id: &str, want: Option<&str>, on: bool) -> Result<(), ToggleError> {
        let (s, by_id) = self.favourites_source(want)?;
        let held = if by_id { id.to_string() } else { self.reach(id)?.name };
        s.set_playlist_favorite(&held, on).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
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
        // Its id stays, and with it its suggestions' crate (`like:list:<id>`) and what was turned down for it there.
        Ok(())
    }

    /// The store keeps it aside; its undo brings it back with the links it had (unshared). Returns the undo token.
    pub async fn delete(&self, id: &str) -> Result<Option<String>, ToggleError> {
        let (store, name) = self.own(id)?;
        store.delete(&name).await?;
        if let Some(serve) = self.served.clone()
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

/// Where an export finds a track by its server's id: the file that plays for `who`. Never a Bandcamp preview's (decision
/// 0071: played, never downloaded), whatever the catalogue holds: it is left out as missing.
fn export_place(c: &crate::catalogue::Catalogue, who: &Listener, source: &str, id: &str) -> Option<(i64, String)> {
    if source == crate::bandcamp::SOURCE {
        return None;
    }
    who.place(c, source, id)
}

#[cfg(test)]
mod export_place_tests {
    use super::*;

    #[test]
    fn a_bandcamp_preview_is_never_exported() {
        let root = std::env::temp_dir().join(format!("cratarr-export-place-{}", std::process::id()));
        std::fs::create_dir_all(root.join("A")).unwrap();
        std::fs::write(root.join("A/01.mp3"), "a").unwrap();
        let cat = crate::catalogue::Catalogue::open(None, vec![root.clone()]).unwrap();
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        // Even were a file linked to it.
        let link = [("t1".to_string(), 0, "A/01.mp3".to_string())];
        cat.record_links("jellyfin", "lib", &link, 1).unwrap();
        cat.record_links(crate::bandcamp::SOURCE, "lib", &link, 1).unwrap();
        let who = Listener::default();
        assert_eq!(export_place(&cat, &who, "jellyfin", "t1"), Some((0, "A/01.mp3".to_string())));
        assert_eq!(export_place(&cat, &who, crate::bandcamp::SOURCE, "t1"), None, "left out as missing");
        assert_eq!(export_place(&cat, &who, crate::bandcamp::SOURCE, "https://x.bandcamp.com/album/y#1"), None);
        std::fs::remove_dir_all(&root).ok();
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
        let first = FavouritesKept::load(Some(file.clone()), None);
        assert!(first.kept("jellyfin").is_none(), "nothing kept yet");
        first.save("jellyfin", None, &Arc::new(["A/1.flac".to_string(), "B/2.mp3".to_string()].into_iter().collect())).await;

        let next = FavouritesKept::load(Some(file), None);
        let k = next.kept("jellyfin").expect("kept from the last run");
        assert_eq!(k.lines.as_deref().map(|l| l.len()), Some(2));
        assert!(k.at.elapsed() >= FAVOURITES_KEEP, "due to be asked again");
        assert!(next.kept("other").is_none(), "only for the server it came from");
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod left_out_tests {
    use super::*;

    /// A take names no account: an account's crates are followed whether anyone opened them since the start or not, and a folder
    /// no account could have is left. With no data folder, the stores open are all there is.
    #[test]
    fn every_store_is_the_first_the_open_ones_and_those_only_on_disk() {
        let dir = std::env::temp_dir().join(format!("cratarr-every-store-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        for folder in ["users/ann", "users/.hidden", "users/bob"] {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
        }
        let stores = CrateStores::new(dir.to_str());
        stores.of(Some("cy")).unwrap();
        let mut accounts: Vec<Option<String>> = stores.every().into_iter().map(|(a, _)| a).collect();
        accounts.sort();
        assert_eq!(accounts, [None, Some("ann".to_string()), Some("bob".to_string()), Some("cy".to_string())]);
        let first = stores.every().into_iter().find(|(a, _)| a.is_none()).unwrap().1;
        assert!(Arc::ptr_eq(&first, &stores.first), "the store in use, or its memory would go on saying what the file no longer does");
        assert!(Arc::ptr_eq(&stores.every().into_iter().find(|(a, _)| a.as_deref() == Some("cy")).unwrap().1, &stores.of(Some("cy")).unwrap()));
        std::fs::remove_dir_all(&dir).ok();

        let memory = CrateStores::new(None);
        memory.of(Some("ann")).unwrap();
        assert_eq!(memory.every().len(), 2);
    }
}

#[cfg(test)]
mod library_tests {
    use super::*;

    #[test]
    fn an_incoming_line_is_there_when_its_file_is() {
        let root = std::env::temp_dir().join(format!("cratarr-lines-{}", std::process::id()));
        let (m, inc) = (root.join("Music"), root.join("in"));
        std::fs::create_dir_all(m.join("A")).unwrap();
        std::fs::create_dir_all(inc.join("X")).unwrap();
        std::fs::write(m.join("A/1.flac"), "a").unwrap();
        std::fs::write(inc.join("X/1.flac"), "x").unwrap();
        let cat = crate::catalogue::Catalogue::open(None, vec![m.clone(), inc.clone()]).unwrap();
        assert!(line_exists(&cat, "A/1.flac"));
        assert!(line_exists(&cat, "../in/X/1.flac"), "an Incoming track's line");
        assert!(!line_exists(&cat, "../in/X/2.flac"));
        assert!(!line_exists(&cat, "../elsewhere/X/1.flac"), "in no folder of the catalogue");
        std::fs::remove_dir_all(&root).ok();
    }
}

