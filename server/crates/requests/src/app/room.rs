//! Room (decision 0084; AL01): what counts against each user's room, read when asked from the catalogue's rows by one rule
//! (SCENARIOS-RUNS-13-14's R.0). An administrator has none (OQ7). It stores nothing: no queue, no state file, no timer (the light
//! rule).
//!
//! What counts against user U, in bytes:
//! - each present file in U's own music (`<own_music>/<U>`, its switch on or off: OQ-R2), unless it is free of room
//!   (`catalogue::FREE_OF_ROOM`: another file holds the same bytes and shares its blocks, in a main library, or in own music as
//!   the one that pays for them; Q5, Q9, (d), OQ-R1);
//! - each present file of a copy in an Incoming library of a record U asked for first, approved, the copy come at or after U
//!   asked (R32, `Requester::of_copy`): only arrived music counts (Q2), whole or partly there.
//!
//! Never a main library's files, anything in a trash (Q4: at once), a parked folder's, nor a fetcher's own download folders (no
//! catalogue folder). Full is `used >= size` (Q3: until under).
//!
//! The disk floor (AL03, 0084) watches the real disk where users' music lands (Q4): below it, users' new Wants wait and Soulseek
//! starts no transfer of theirs; administrators' go on. It is read where the requests' round, a Want, Soulseek's fetch and the health
//! answer run, and keeps only whether the server is short, in the user data's meta (OQ-F3), so a restart reads it as it was.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use super::requests::Requester;
use crate::catalogue::{Catalogue, InUse};
use crate::serve::Serve;
use crate::serve::users::User;
use crate::settings::LibraryKind;
use crate::userdata::UserData;

/// A GB as room counts it (OQ-R5): 10^9 bytes.
pub const GB: u64 = 1_000_000_000;
/// A user's room where an administrator set none (0084).
pub const ROOM: u64 = 100 * GB;

/// The GB a room's or the floor's body gives (`{"gb": …}`, AL04.4): a number from 0, with decimals; none for anything else (text, `null`,
/// none at all, a negative), which the routes refuse with 400 in their own words, never the 422 a typed body would get. Pure.
pub fn gb_given(body: &serde_json::Value) -> Option<f64> {
    body.get("gb").and_then(serde_json::Value::as_f64).filter(|gb| gb.is_finite() && *gb >= 0.0)
}

/// A user's room: its size and what counts against it, in bytes, and whether it is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Room {
    pub size: u64,
    pub used: u64,
    pub full: bool,
}

/// `user`'s room with `used` counted against it: none for an administrator (OQ7; a room set on one is kept for when it is a user
/// again, OQ-R3). Pure.
pub fn room_of(user: &User, used: u64) -> Option<Room> {
    (!user.admin).then(|| {
        let size = user.room_bytes.unwrap_or(ROOM);
        Room { size, used, full: used >= size }
    })
}

/// Where room reads, by catalogue folder: each own-music folder with the account it is named for (switched on or off, OQ-R2),
/// the main libraries', and Incoming's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Places {
    pub own: Vec<(i64, String)>,
    pub main: Vec<i64>,
    pub incoming: Vec<i64>,
}

impl Places {
    /// From each catalogue folder's library (`Library::kinds`) and the folders in use: a parked folder is none of them.
    pub fn of(kinds: &[(i64, LibraryKind)], folders: &InUse) -> Self {
        let mut places = Places::default();
        for (folder, kind) in kinds {
            let Some(path) = folders.get(*folder) else { continue };
            match kind {
                LibraryKind::Main => places.main.push(*folder),
                LibraryKind::Incoming => places.incoming.push(*folder),
                LibraryKind::Own => {
                    if let Some(account) = path.file_name() {
                        places.own.push((*folder, account.to_string_lossy().into_owned()));
                    }
                }
            }
        }
        places
    }
}

/// What counts against each account's room (R.0), all at once: one read of the catalogue (`Catalogue::room_rows`), with who asked
/// first for what (`asked`, as `requests::requesters` has it). An account with nothing counted is not there. Blocking.
pub fn used_by(cat: &Catalogue, places: &Places, asked: &HashMap<i64, Requester>) -> Result<HashMap<String, u64>> {
    let own: Vec<i64> = places.own.iter().map(|(folder, _)| *folder).collect();
    let records: Vec<i64> = asked.keys().copied().collect();
    let rows = cat.room_rows(&own, &places.main, &places.incoming, &records)?;
    let mut used: HashMap<String, u64> = HashMap::new();
    for (folder, bytes) in rows.own {
        if let Some((_, account)) = places.own.iter().find(|(f, _)| *f == folder) {
            *used.entry(account.clone()).or_default() += bytes;
        }
    }
    for copy in rows.copies {
        if let Some(r) = asked.get(&copy.record).filter(|r| r.of_copy(copy.came)) {
            *used.entry(r.user.clone()).or_default() += copy.bytes;
        }
    }
    Ok(used)
}

/// The rooms of `users` on this server, every one from one read (`used_by`, AL01.11), each by its name: none for an
/// administrator. Who asked first for what is the requesters' kept read (R16, the Albums page's). Blocking.
pub fn rooms(serve: &Serve, users: &[User]) -> Result<HashMap<String, Room>> {
    if users.iter().all(|u| u.admin) {
        return Ok(HashMap::new());
    }
    if !serve.has_catalogue() {
        return Ok(users.iter().filter_map(|u| Some((u.name.clone(), room_of(u, 0)?))).collect());
    }
    rooms_in(serve.catalogue(), &serve.library.kinds(), &super::requests::requesters(serve), users)
}

/// [`rooms`] from the catalogue `cat`, each catalogue folder's library as `kinds` has it, and who asked first for what (`asked`):
/// one read (`used_by`). What a Want reads to wait or go (AL02), who asked read then. Blocking.
pub fn rooms_in(cat: &Catalogue, kinds: &[(i64, LibraryKind)], asked: &HashMap<i64, Requester>, users: &[User]) -> Result<HashMap<String, Room>> {
    let used = used_by(cat, &Places::of(kinds, &cat.folders()), asked)?;
    Ok(users.iter().filter_map(|u| Some((u.name.clone(), room_of(u, used.get(&u.name).copied().unwrap_or(0))?))).collect())
}

// ---- The disk floor (AL03)

/// The floor where an administrator set none (0084).
pub const FLOOR: u64 = 200 * GB;
/// How far above the floor the disks must be before the server is no longer short (OQ-F3): it changes once per crossing, never
/// flapping at the floor.
pub const MARGIN: u64 = 10 * GB;
/// The floor an administrator set, in bytes, as the user data's meta keeps it.
const FLOOR_KEY: &str = "room:floor";
/// Whether the server is short of space, as last read (OQ-F3), in the user data's meta: `SHORT`, `CLEAR`, or `UNREADABLE` (the
/// disks' free space could not be read, said once in the log).
const SHORT_KEY: &str = "room:short-of-space";
const SHORT: &str = "short";
const CLEAR: &str = "clear";
const UNREADABLE: &str = "unreadable";

/// The free space of the disk a folder is on (OQ-F2): statvfs on the server (`folders::free`); the tests' stand in. And the disk's
/// size (statvfs' total), which a new install's floor is a share of (PL4).
pub trait Disks: Send + Sync {
    fn free(&self, dir: &Path) -> Option<u64>;
    fn size(&self, _dir: &Path) -> Option<u64> {
        None
    }
}

/// The disks themselves.
pub struct TheDisks;

impl Disks for TheDisks {
    fn free(&self, dir: &Path) -> Option<u64> {
        crate::folders::free(dir)
    }
}

/// The floor and the disks as just read (AL03): the floor, the lowest free space of the disks users' music lands on (none where none
/// could be read), and whether the server is short of space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Space {
    pub floor: u64,
    pub free: Option<u64>,
    pub short: bool,
    /// No floor saved: the floor is the default (PL4: "10% of the smallest disk it watches").
    pub default: bool,
}

/// The floor in bytes: an administrator's, else `FLOOR`. Blocking.
pub fn floor(ud: &UserData) -> u64 {
    ud.meta(FLOOR_KEY).ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(FLOOR)
}

/// The floor an administrator saved, in bytes, where one is. Blocking.
pub fn saved_floor(ud: &UserData) -> Option<u64> {
    ud.meta(FLOOR_KEY).ok().flatten().and_then(|v| v.parse().ok())
}

/// The floor where none is saved (PL4), from the disks the floor reads (`dirs`).
pub fn default_floor(_disks: &dyn Disks, _dirs: &[PathBuf]) -> u64 {
    FLOOR
}

/// How far above `floor` the disks must be before the server is no longer short (OQ-F3, PL4).
pub fn margin(_floor: u64) -> u64 {
    MARGIN
}

/// The floor an administrator sets, in bytes. Blocking.
pub fn set_floor(ud: &UserData, bytes: u64) -> Result<()> {
    ud.set_meta(FLOOR_KEY, &bytes.to_string())
}

/// The folders the floor reads on `serve` (OQ-F2): those users' music lands on, each Incoming library's (Lidarr's, Soulseek's, an
/// Uploads library's) and the own-music folder; never a main library's, nor the data folder. Blocking.
pub fn floor_folders(serve: &Serve) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if serve.has_catalogue() {
        let folders = serve.catalogue().folders();
        let places = Places::of(&serve.library.kinds(), &folders);
        dirs.extend(places.incoming.iter().filter_map(|f| folders.get(*f).cloned()));
    }
    dirs.extend(serve.own_music());
    dirs.sort();
    dirs.dedup();
    dirs
}

/// The disks read against the floor now (AL03): the lowest free space of `dirs`' disks (OQ-F2); short below the floor, no longer
/// short only from the floor plus `MARGIN`, and between the two as it was (OQ-F3). Free space none of them could be read for is not
/// short, said once in the log. Whether it is short is kept in the user data's meta when it changes, and said in the log; once it is
/// no longer short, the Soulseek fetches it held go (`soulseek::space_again`). One read at a time. Blocking.
pub fn space(ud: &UserData, disks: &dyn Disks, dirs: &[PathBuf]) -> Space {
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|p| p.into_inner());
    let floor = floor(ud);
    let was = ud.meta(SHORT_KEY).ok().flatten().unwrap_or_else(|| CLEAR.to_string());
    let free = dirs.iter().filter_map(|d| disks.free(d)).min();
    let now = match free {
        None if dirs.is_empty() => CLEAR,
        None => UNREADABLE,
        Some(f) if f < floor => SHORT,
        Some(f) if f >= floor.saturating_add(MARGIN) => CLEAR,
        Some(_) if was == SHORT => SHORT,
        Some(_) => CLEAR,
    };
    if was != now {
        if let Err(e) = ud.set_meta(SHORT_KEY, now) {
            tracing::warn!(target: "room", reason = %format!("{e:#}"), "whether the server is short of space was not kept");
        }
        let (free_text, floor_text) = (size_text(free.unwrap_or(0)), size_text(floor));
        match now {
            SHORT => tracing::warn!(target: "room", free = %free_text, floor = %floor_text, "short of space: users' new Wants wait"),
            UNREADABLE => tracing::warn!(target: "room", "the free space where users' music lands could not be read: not taken as short"),
            _ if was == SHORT => tracing::info!(target: "room", free = %free_text, floor = %floor_text, "space again: users' Wants go"),
            _ => {}
        }
        if was == SHORT {
            super::soulseek::space_again();
        }
    }
    let default = ud.meta(FLOOR_KEY).ok().flatten().is_none();
    Space { floor, free, short: now == SHORT, default }
}

/// Whether the server is short of space now ([`space`]). Blocking.
pub fn short(ud: &UserData, disks: &dyn Disks, dirs: &[PathBuf]) -> bool {
    space(ud, disks, dirs).short
}

/// [`space`] on `serve`, its floor's folders (`floor_folders`). Blocking.
pub fn space_on(serve: &Serve, disks: &dyn Disks) -> Space {
    space(&serve.userdata, disks, &floor_folders(serve))
}

/// Whether `serve` is short of space now ([`short`], its floor's folders). Blocking.
pub fn short_on(serve: &Serve, disks: &dyn Disks) -> bool {
    short(&serve.userdata, disks, &floor_folders(serve))
}

/// What the trashes on the floor's disks hold (AL03.3, Q4): the bytes of every file moved into the trash of an Incoming library or of
/// anyone's own music, not emptied yet; emptying them frees that. Blocking.
pub fn in_the_trash(serve: &Serve) -> Result<u64> {
    if !serve.has_catalogue() {
        return Ok(0);
    }
    let cat = serve.catalogue();
    let places = Places::of(&serve.library.kinds(), &cat.folders());
    let folders: Vec<i64> = places.incoming.iter().copied().chain(places.own.iter().map(|(f, _)| *f)).collect();
    cat.trash_bytes(&folders)
}

/// A size as Health says it (AL03.3): in GB from one GB up ("150 GB", "199.9 GB"), else in MB or KB.
pub fn size_text(bytes: u64) -> String {
    let (n, unit) = match bytes {
        b if b >= GB => (b as f64 / GB as f64, "GB"),
        b if b >= 1_000_000 => (b as f64 / 1e6, "MB"),
        b => (b as f64 / 1e3, "KB"),
    };
    let s = format!("{n:.1}");
    format!("{} {unit}", s.strip_suffix(".0").unwrap_or(&s))
}

#[cfg(any(test, feature = "test-support"))]
pub mod tests {
    // Shared with the tests of the crates above (test-support): the helpers only its own tests call are idle there.
    #![cfg_attr(not(test), allow(dead_code, unused_imports))]
    use super::*;
    use crate::app::requests::{APPROVED, CANCELLED, DECLINED, DELIVERED, REJECTED, requesters_in};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    pub(crate) const MUSIC: i64 = 0;
    pub(crate) const LIDARR: i64 = 1;
    pub(crate) const SOULSEEK: i64 = 2;
    pub(crate) const ANN: i64 = 3;
    pub(crate) const BOB: i64 = 4;
    pub(crate) const EVE: i64 = 5;
    const ROOT: i64 = 6;
    const CARL: i64 = 7;
    /// `room_shape`'s catalogue folders, in their order.
    pub(crate) const FOLDERS: [&str; 8] = ["/library/music", "/library/lidarr", "/library/soulseek", "/own/ann", "/own/bob", "/own/eve", "/own/root", "/own/carl"];
    pub(crate) fn gb(n: f64) -> u64 {
        (n * GB as f64).round() as u64
    }

    /// The libraries of `room_shape`'s folders, as the library gives them.
    pub(crate) fn kinds() -> Vec<(i64, LibraryKind)> {
        let kind = |folder: i64| match folder {
            MUSIC => LibraryKind::Main,
            LIDARR | SOULSEEK => LibraryKind::Incoming,
            _ => LibraryKind::Own,
        };
        (0..FOLDERS.len() as i64).map(|f| (f, kind(f))).collect()
    }

    /// AL01's shape (`room_shape`): a server whose catalogue holds Music (folder 0, main), Lidarr's Incoming (1, naming no fetcher,
    /// as production's), Soulseek's (2), and own music: ann's (3), bob's (4), eve's (5), root's (6) and carl's (7). Rows only,
    /// written with their sizes, no file behind them. The data folder `data` where given (its users file, as found there).
    pub(crate) fn room_shape_in(data: Option<&Path>, catalogue: Option<&Path>) -> Arc<Serve> {
        let serve = Serve::open(data, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(Arc::new(Catalogue::open(catalogue, FOLDERS.map(PathBuf::from).to_vec()).unwrap()));
        serve.library.set_kinds(kinds());
        serve
    }

    pub(crate) fn room_shape() -> Arc<Serve> {
        room_shape_in(None, None)
    }

    /// Disks with room to spare, wherever asked: what a test not about the floor hands a Want with (AL03).
    pub(crate) struct Roomy;

    impl Disks for Roomy {
        fn free(&self, _: &Path) -> Option<u64> {
            Some(u64::MAX / 2)
        }
    }

    /// The scenarios' disks stand-in (AL03): each folder's disk's free space as a test sets it (none: unreadable), each ask
    /// recorded; asked of a folder not in its map, it panics.
    #[derive(Default)]
    pub(crate) struct Stand {
        free: std::sync::Mutex<HashMap<PathBuf, Option<u64>>>,
        size: std::sync::Mutex<HashMap<PathBuf, u64>>,
        asked: std::sync::Mutex<Vec<PathBuf>>,
    }

    impl Stand {
        /// Folders and the free space of their disks, in GB.
        pub(crate) fn of(free: &[(&str, f64)]) -> Self {
            let stand = Stand::default();
            for (dir, n) in free {
                stand.set(dir, Some(*n));
            }
            stand
        }

        /// A folder's disk now `n` GB free; none: its free space unreadable.
        pub(crate) fn set(&self, dir: &str, n: Option<f64>) {
            self.free.lock().unwrap().insert(PathBuf::from(dir), n.map(gb));
        }

        /// A folder's disk's size, in GB (PL4).
        pub(crate) fn sized(self, dir: &str, n: f64) -> Self {
            self.size.lock().unwrap().insert(PathBuf::from(dir), gb(n));
            self
        }

        /// The folders it was asked of, each once.
        pub(crate) fn asked(&self) -> Vec<PathBuf> {
            let mut asked = self.asked.lock().unwrap().clone();
            asked.sort();
            asked.dedup();
            asked
        }
    }

    impl Disks for Stand {
        fn free(&self, dir: &Path) -> Option<u64> {
            self.asked.lock().unwrap().push(dir.to_path_buf());
            let known = self.free.lock().unwrap().get(dir).copied();
            known.unwrap_or_else(|| panic!("asked of a disk no user's music lands on: {}", dir.display()))
        }

        fn size(&self, dir: &Path) -> Option<u64> {
            self.size.lock().unwrap().get(dir).copied()
        }
    }

    /// PL4 (the owner, 2026-11-21 13:23, "A share of the disk"): with no floor saved, the floor is 10% of the smallest disk it watches,
    /// at most 200 GB, and its margin 10% of the floor, at least 1 GB and at most 10 GB: (a) one 500 GB disk under the Incoming folders
    /// and own music, 50 GB with a 5 GB margin; (b) Incoming on 4 TB and own music on 1 TB, 100 GB with a 10 GB margin; a saved floor
    /// always wins, and one under 100 GB clears 10% above it, not 10 GB. Never above 200 GB by default.
    #[test]
    fn a_new_installs_floor_is_a_tenth_of_its_disk() {
        let (incoming, own) = (PathBuf::from("/srv/incoming"), PathBuf::from("/srv/own"));
        let dirs = [incoming.clone(), own.clone()];
        let one = Stand::of(&[("/srv/incoming", 400.0), ("/srv/own", 400.0)]).sized("/srv/incoming", 500.0).sized("/srv/own", 500.0);
        assert_eq!((default_floor(&one, &dirs), margin(default_floor(&one, &dirs))), (gb(50.0), gb(5.0)), "(a) a tenth of 500 GB");
        let two = Stand::of(&[("/srv/incoming", 3000.0), ("/srv/own", 900.0)]).sized("/srv/incoming", 4000.0).sized("/srv/own", 1000.0);
        assert_eq!((default_floor(&two, &dirs), margin(default_floor(&two, &dirs))), (gb(100.0), gb(10.0)), "(b) a tenth of the smaller disk");
        let big = Stand::of(&[("/srv/incoming", 9000.0)]).sized("/srv/incoming", 16_000.0);
        assert_eq!(default_floor(&big, &[incoming.clone()]), FLOOR, "never above 200 GB by default");
        assert_eq!(margin(gb(30.0)), gb(3.0), "a saved 30 GB floor clears 10% above it");
        assert_eq!(margin(gb(5.0)), gb(1.0), "at least 1 GB");
        // The floor the disks are read against: the default, short below it, and an administrator's floor wins.
        let serve = room_shape();
        let small = Stand::of(&[("/srv/incoming", 40.0), ("/srv/own", 300.0)]).sized("/srv/incoming", 500.0).sized("/srv/own", 500.0);
        let read = space(&serve.userdata, &small, &dirs);
        assert_eq!((read.floor, read.short, read.default), (gb(50.0), true, true), "40 GB free under a 50 GB floor: short");
        set_floor(&serve.userdata, gb(30.0)).unwrap();
        let read = space(&serve.userdata, &small, &dirs);
        assert_eq!((read.floor, read.short, read.default), (gb(30.0), false, false), "a saved floor wins");
        let _ = own;
    }

    /// What counts against each account's room now: the rule's one read, who asked first read now.
    fn used_now(serve: &Serve) -> HashMap<String, u64> {
        let (cat, kinds) = (serve.catalogue(), serve.library.kinds());
        used_by(cat, &Places::of(&kinds, &cat.folders()), &requesters_in(cat, &serve.userdata, &kinds)).unwrap()
    }

    pub(crate) fn used(serve: &Serve, account: &str) -> u64 {
        used_now(serve).get(account).copied().unwrap_or(0)
    }

    /// Every account's count added up: one file in two accounts' room, or twice in one's, shows here.
    pub(crate) fn total(serve: &Serve) -> u64 {
        used_now(serve).values().sum()
    }

    /// A file's row with no file behind it: on `track`, `size` bytes, its time `mtime`, shared as a copy said, come at `came`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn put_file(cat: &Catalogue, folder: i64, rel: &str, track: Option<i64>, size: u64, mtime: i64, shared: bool, came: i64) {
        let track = track.map_or("NULL".to_string(), |t| t.to_string());
        cat.execute_for_tests(&format!(
            "INSERT INTO files (folder, rel, size, mtime, state, track, shared, came) VALUES ({folder}, '{rel}', {size}, {mtime}, 1, {track}, {}, {came})",
            i64::from(shared)
        ));
    }

    /// A folder's rows as they stand, to see that nothing of an account's is written.
    pub(crate) fn rows_of(cat: &Catalogue, folder: i64) -> Vec<String> {
        let db = cat.writer_for_tests();
        let mut st = db
            .prepare("SELECT rel || '|' || ifnull(track, '') || '|' || size || '|' || mtime || '|' || shared || '|' || ifnull(came, '') || '|' || ifnull(trashed, '') FROM files WHERE folder = ?1 ORDER BY rel")
            .unwrap();
        let rows = st.query_map([folder], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        rows
    }

    /// `user`'s approved request of `record`, asked at `at`.
    fn asking(user: &str, record: i64, at: i64, group: Option<&str>) -> crate::userdata::Request {
        crate::userdata::Request {
            id: 0,
            user: user.into(),
            album_key: format!("k-{user}-{record}"),
            artist_key: "glass harbour".into(),
            release_group: group.map(str::to_string),
            album: format!("Record {record}"),
            artist: "Glass Harbour".into(),
            year: None,
            cover: None,
            at,
            status: APPROVED.into(),
            reason: None,
            decided_by: None,
            decided_at: None,
            fetched: None,
            record: Some(record),
        }
    }

    fn user(name: &str, admin: bool) -> User {
        User { admin, ..User::named(name) }
    }

    /// AL01.1: the rule's cases, each on its own rows (`room_shape`), 1 GB = 10^9 bytes. Never one file in two accounts' room nor
    /// twice in one's (`total`), nor Music's, a trash's or a parked folder's bytes counted.
    #[test]
    fn room_counts_by_the_one_rule() {
        // 1, 2, 3: ann's own file, no twin; then ann's copy of Music's M (same track, size and time, shared); then M rewritten.
        let serve = room_shape();
        let cat = serve.catalogue();
        put_file(cat, ANN, "A/1.flac", Some(1), gb(2.0), 10, false, 100);
        assert_eq!(used(&serve, "ann"), gb(2.0), "1: her own, no twin");
        put_file(cat, MUSIC, "M/1.flac", Some(2), gb(2.0), 20, false, 100);
        cat.copy_rows(&[((MUSIC, "M/1.flac".into()), (ANN, "M/1.flac".into()), true)]).unwrap();
        assert_eq!(used(&serve, "ann"), gb(2.0), "2: her copy of Music's M is free");
        assert_eq!(total(&serve), gb(2.0), "Music's bytes counted for no one");
        cat.rewrite_for_tests(MUSIC, "M/1.flac", gb(2.1), 30);
        assert_eq!(used(&serve, "ann"), gb(4.0), "3, Q5: in full from then");

        // 4: as 2, M into Music's trash, then back (its Undo).
        let serve = room_shape();
        let cat = serve.catalogue();
        put_file(cat, MUSIC, "M/1.flac", Some(2), gb(2.0), 20, false, 100);
        cat.copy_rows(&[((MUSIC, "M/1.flac".into()), (ANN, "M/1.flac".into()), true)]).unwrap();
        assert_eq!(used(&serve, "ann"), 0, "4: free");
        cat.mark_trashed(MUSIC, "M", "100/M").unwrap();
        assert_eq!(used(&serve, "ann"), gb(2.0), "4: M in Music's trash: in full");
        assert_eq!(total(&serve), gb(2.0), "the trash's bytes counted for no one");
        cat.unmark_trashed(MUSIC, "M", "100/M").unwrap();
        assert_eq!(used(&serve, "ann"), 0, "4: M back: free again");

        // 5: bob's own original B and Music's copy of it (RY01, or a Copy to into Music).
        let serve = room_shape();
        let cat = serve.catalogue();
        put_file(cat, BOB, "B/1.flac", Some(3), gb(2.0), 20, false, 100);
        cat.copy_rows(&[((BOB, "B/1.flac".into()), (MUSIC, "B/1.flac".into()), true)]).unwrap();
        assert_eq!(used(&serve, "bob"), 0, "5, Q9: Bob's original is free once Music holds a copy of it");
        assert_eq!(total(&serve), 0);

        // 6, 7: bob's B and ann's copy of it (T5); then B into bob's trash.
        let serve = room_shape();
        let cat = serve.catalogue();
        put_file(cat, BOB, "B/1.flac", Some(3), gb(2.0), 20, false, 100);
        cat.copy_rows(&[((BOB, "B/1.flac".into()), (ANN, "B/1.flac".into()), true)]).unwrap();
        assert_eq!((used(&serve, "bob"), used(&serve, "ann")), (gb(2.0), 0), "6: the original pays, the copy is free");
        assert_eq!(total(&serve), gb(2.0), "one set of bytes, paid once");
        cat.mark_trashed(BOB, "B", "300/B").unwrap();
        assert_eq!((used(&serve, "bob"), used(&serve, "ann")), (0, gb(2.0)), "7: B in bob's trash: ann's copy pays");
        assert_eq!(total(&serve), gb(2.0));

        // 8: ann's and eve's copies of Music's M, ann's first; then M gone.
        let serve = room_shape();
        let cat = serve.catalogue();
        put_file(cat, MUSIC, "M/1.flac", Some(2), gb(2.0), 20, false, 100);
        cat.copy_rows(&[((MUSIC, "M/1.flac".into()), (ANN, "M/1.flac".into()), true), ((MUSIC, "M/1.flac".into()), (EVE, "M/1.flac".into()), true)]).unwrap();
        cat.came_for_tests(ANN, 200);
        cat.came_for_tests(EVE, 300);
        assert_eq!((used(&serve, "ann"), used(&serve, "eve")), (0, 0), "8: both free while Music holds M");
        cat.execute_for_tests("UPDATE files SET gone_at = 400 WHERE folder = 0");
        assert_eq!((used(&serve, "ann"), used(&serve, "eve")), (gb(2.0), 0), "8, OQ-R1: the copy that came first pays, the other is free");
        assert_eq!(total(&serve), gb(2.0), "nobody free because each sees the other, nor both paying for one set");

        // 9: ann's copy of B made on a disk that could not share.
        let serve = room_shape();
        let cat = serve.catalogue();
        put_file(cat, BOB, "B/1.flac", Some(3), gb(2.0), 20, false, 100);
        cat.copy_rows(&[((BOB, "B/1.flac".into()), (ANN, "B/1.flac".into()), false)]).unwrap();
        assert_eq!((used(&serve, "bob"), used(&serve, "ann")), (gb(2.0), gb(2.0)), "9: both in full");

        // 10: ann's own folder parked, its rows kept.
        let serve = room_shape();
        let cat = serve.catalogue();
        put_file(cat, ANN, "A/1.flac", Some(1), gb(2.0), 10, false, 100);
        cat.set_folders(FOLDERS.iter().filter(|f| **f != "/own/ann").map(PathBuf::from).collect()).unwrap();
        assert_eq!(used(&serve, "ann"), 0, "10: a parked folder's bytes count nothing");
        assert_eq!(total(&serve), 0);
        let kept = cat.writer_for_tests().query_row("SELECT count(*) FROM files WHERE folder < 0 AND rel = 'A/1.flac'", [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(kept, 1, "10: its rows kept");

        // 11, 12: ann's request R (approved, asked at 1000) and a copy of R of three files in Lidarr's Incoming, come at 2000.
        let serve = room_shape();
        let cat = serve.catalogue();
        let r = cat.record_for_tests("Night Ferry", 3);
        cat.copy_for_tests(r, LIDARR);
        cat.execute_for_tests(&format!("UPDATE files SET size = {}, came = 2000 WHERE folder = {LIDARR}", gb(1.0)));
        let id = serve.userdata.add_request(&asking("ann", r, 1000, None)).unwrap();
        assert_eq!(used(&serve, "ann"), gb(3.0), "11: the copy's bytes");
        cat.came_for_tests(LIDARR, 500);
        assert_eq!(used(&serve, "ann"), 0, "11: a copy there before she asked is no requester's (R32)");
        cat.came_for_tests(LIDARR, 2000);
        for status in [DELIVERED, DECLINED, CANCELLED, REJECTED] {
            assert!(serve.userdata.set_request_status(id, APPROVED, status, None, None).unwrap());
            assert_eq!(used(&serve, "ann"), 0, "11: {status}");
            assert!(serve.userdata.set_request_status(id, status, APPROVED, None, None).unwrap());
        }
        assert_eq!(used(&serve, "ann"), gb(3.0));
        serve.userdata.add_request(&asking("bob", r, 1500, None)).unwrap();
        assert_eq!((used(&serve, "ann"), used(&serve, "bob")), (gb(3.0), 0), "12: the first asker's alone");
        assert_eq!(total(&serve), gb(3.0));
        cat.mark_trashed(LIDARR, &format!("copy-{r}"), "100/R").unwrap();
        assert_eq!((used(&serve, "ann"), total(&serve)), (0, 0), "11: the copy in the trash");

        // 14: root holds 300 GB of own music, and has no room.
        let serve = room_shape();
        let cat = serve.catalogue();
        put_file(cat, ROOT, "R/1.flac", Some(9), gb(300.0), 10, false, 100);
        let root = user("root", true);
        assert_eq!(room_of(&root, used(&serve, "root")), None, "14: no room, whatever he holds");
        assert!(rooms(&serve, &[root]).unwrap().is_empty());
    }

    /// AL01.2 (Q2): ann's room is 100 GB, 99 used by her own music, with 30 open Wants and nothing of them arrived: still 99, not
    /// full. One 3 GB album arrives in Lidarr's Incoming: 102; 3 of another's 10 files in Soulseek's: 102.9. Never a Want counted
    /// before its music arrives; slskd's and Lidarr's own download folders are no catalogue folder.
    #[test]
    fn only_arrived_music_counts() {
        let serve = room_shape();
        let cat = serve.catalogue();
        put_file(cat, ANN, "A/1.flac", Some(1), gb(99.0), 10, false, 100);
        let wants: Vec<i64> = (0..30).map(|n| cat.record_for_tests(&format!("Want {n}"), 10)).collect();
        for record in &wants {
            serve.userdata.add_request(&asking("ann", *record, 1000, Some("00000000-0000-4000-8000-000000000301"))).unwrap();
        }
        let ann = user("ann", false);
        assert_eq!(room_of(&ann, used(&serve, "ann")), Some(Room { size: gb(100.0), used: gb(99.0), full: false }), "nothing arrived");
        cat.copy_for_tests(wants[0], LIDARR);
        cat.execute_for_tests(&format!("UPDATE files SET size = {}, came = 2000 WHERE folder = {LIDARR}", gb(0.3)));
        assert_eq!(used(&serve, "ann"), gb(102.0), "one album arrived");
        cat.copy_for_tests(wants[1], SOULSEEK);
        cat.execute_for_tests(&format!(
            "DELETE FROM files WHERE folder = {SOULSEEK} AND rel NOT IN (SELECT rel FROM files WHERE folder = {SOULSEEK} ORDER BY rel LIMIT 3)"
        ));
        cat.execute_for_tests(&format!("UPDATE files SET size = {}, came = 2000 WHERE folder = {SOULSEEK}", gb(0.3)));
        assert_eq!(used(&serve, "ann"), gb(102.9), "three of its ten files: partly there counts");
        assert_eq!(room_of(&ann, used(&serve, "ann")).map(|r| r.full), Some(true));
    }

    /// AL01.3 (Q5, Q9, (d); INTERACTIONS I19), through the writers: root's Copy to of Music's "Two Rooms" into ann's own music,
    /// sharing its blocks, is free; Music's copy rewritten (a tag write's row): ann's in full; replaced by a FLAC (the track to the
    /// new file, the old one into Music's trash, as Replace's rows): in full; the Replace undone: free again; RY01's rows (bob's own
    /// FLAC copied into Music): bob's FLAC free, his `shared` still 0. Never room read from `files.shared` alone (ann's flag stays
    /// 1 throughout), nor a row of ann's or bob's written by any of this.
    #[test]
    fn a_free_copy_pays_when_its_twin_changes_or_leaves_and_is_free_again() {
        // Music's "Two Rooms", one track (the catalogue's first), and root's Copy to into ann's own music.
        let copied = || {
            let serve = room_shape();
            let cat = serve.catalogue().clone();
            cat.record_for_tests("Two Rooms", 1);
            put_file(&cat, MUSIC, "Two Rooms/1.flac", Some(1), gb(2.0), 20, false, 100);
            cat.copy_rows(&[((MUSIC, "Two Rooms/1.flac".into()), (ANN, "Two Rooms/1.flac".into()), true)]).unwrap();
            assert_eq!(used(&serve, "ann"), 0, "after the copy: free");
            (serve, cat)
        };
        let flag = |cat: &Catalogue| cat.writer_for_tests().query_row("SELECT shared FROM files WHERE folder = 3", [], |r| r.get::<_, i64>(0)).unwrap();

        // (i) Music's copy rewritten in place.
        let (serve, cat) = copied();
        let hers = rows_of(&cat, ANN);
        cat.rewrite_for_tests(MUSIC, "Two Rooms/1.flac", gb(2.1), 30);
        assert_eq!(used(&serve, "ann"), gb(2.0), "(i) in full");
        assert_eq!((flag(&cat), rows_of(&cat, ANN)), (1, hers), "(i) her flag still 1, her rows as they were");

        // (ii) Replace: a FLAC takes the track, the old file into Music's trash; (iii) its Undo.
        let (serve, cat) = copied();
        let hers = rows_of(&cat, ANN);
        put_file(&cat, MUSIC, "Two Rooms (FLAC)/1.flac", None, gb(3.0), 40, false, 200);
        cat.move_into_trash((MUSIC, "Two Rooms/1.flac"), (MUSIC, ".cratarr-trash/100/Two Rooms/1.flac"), "100").unwrap();
        cat.set_tracks(&[((MUSIC, ".cratarr-trash/100/Two Rooms/1.flac".into()), None), ((MUSIC, "Two Rooms (FLAC)/1.flac".into()), Some(1))]).unwrap();
        assert_eq!(used(&serve, "ann"), gb(2.0), "(ii) in full");
        assert_eq!((flag(&cat), rows_of(&cat, ANN)), (1, hers.clone()), "(ii) her flag still 1, her rows as they were");
        cat.move_files(MUSIC, ".cratarr-trash/100/Two Rooms/1.flac", MUSIC, "Two Rooms/1.flac").unwrap();
        cat.set_tracks(&[((MUSIC, "Two Rooms (FLAC)/1.flac".into()), None), ((MUSIC, "Two Rooms/1.flac".into()), Some(1))]).unwrap();
        assert_eq!(used(&serve, "ann"), 0, "(iii) the Replace undone: free again");
        assert_eq!(rows_of(&cat, ANN), hers);

        // (iv) RY01's rows: bob's own FLAC copied into Music in place of Music's MP3.
        use crate::journal::{Event, TrackMove};
        let serve = room_shape();
        let cat = serve.catalogue();
        let record = cat.record_for_tests("Drift", 1);
        put_file(cat, MUSIC, "Drift/1.mp3", Some(1), gb(0.1), 10, false, 100);
        put_file(cat, BOB, "Bob/Drift/1.flac", Some(1), gb(2.0), 30, false, 50);
        assert_eq!(used(&serve, "bob"), gb(2.0), "his FLAC, its own bytes");
        let his = rows_of(cat, BOB);
        let event = Event::AlbumReplacedFromCopy {
            source: "cratarr".into(),
            album_id: format!("al-{record}"),
            title: "Drift".into(),
            from_dir: (BOB, "Bob/Drift".into()),
            to_dir: (MUSIC, "Drift".into()),
            copies: vec![TrackMove { track: Some(1), from: (BOB, "Bob/Drift/1.flac".into()), to: (MUSIC, "Drift/1.flac".into()), was: None }],
            stamps: Vec::new(),
            replaced: vec![TrackMove { track: Some(1), from: (MUSIC, "Drift/1.mp3".into()), to: (MUSIC, ".cratarr-trash/100/Drift/1.mp3".into()), was: None }],
            token: "100/Drift".into(),
        };
        crate::app::replace_base::rows_of_copy(cat, &event, &[true]).unwrap();
        assert_eq!(used(&serve, "bob"), 0, "(iv), Q9: his FLAC free once Music holds its bytes");
        assert_eq!(rows_of(cat, BOB), his, "(iv) his rows, his shared flag 0 among them, as they were (OQ-E)");
        assert_eq!(total(&serve), 0, "Music's bytes are no one's");
    }

    /// AL01.4 (T5; INTERACTIONS I20): bob's own "No Papers" (2 GB, an original) copied for ann's Want into her own music, sharing
    /// its blocks: bob pays, ann not; bob's delete (his own music's trash): bob 0 and ann 2 GB at once; his Undo: as before; ann's
    /// tag write on her copy (her flag let go, another size): both pay, two sets of bytes. Never nobody paying for bytes held only in
    /// own music, nor both paying for one set.
    #[test]
    fn two_accounts_on_one_set_of_bytes_pay_once() {
        let serve = room_shape();
        let cat = serve.catalogue();
        let papers = "Salt Meridian/No Papers/1.mp3";
        put_file(cat, BOB, papers, Some(1), gb(2.0), 20, false, 100);
        cat.copy_rows(&[((BOB, papers.into()), (ANN, papers.into()), true)]).unwrap();
        let both = || (used(&serve, "bob"), used(&serve, "ann"));
        assert_eq!(both(), (gb(2.0), 0), "Bob's original pays, Ann's copy is free");
        cat.mark_trashed(BOB, "Salt Meridian/No Papers", "300/No Papers").unwrap();
        assert_eq!(both(), (0, gb(2.0)), "Bob deleted his: Ann's counts in full at once");
        cat.unmark_trashed(BOB, "Salt Meridian/No Papers", "300/No Papers").unwrap();
        assert_eq!(both(), (gb(2.0), 0), "his Undo");
        cat.rewrite_for_tests(ANN, papers, gb(2.1), 40);
        assert_eq!(both(), (gb(2.0), gb(2.1)), "two sets of bytes now: both pay");
    }

    /// AL01.8 (OQ7, OQ-R3): root (300 GB of own music) and carl (150 GB) are administrators, bob a user. Root and carl have no room,
    /// never full; the users' list has bob's alone. Carl made a user: 100 GB, 150 used, over; an administrator again: none; a room
    /// of 200 GB set on him meanwhile is kept, and his once he is a user again. Never an administrator's room read "full". Its Wants
    /// (AL02): root's is handed (the strict Lidarr expects his alone); carl's, a user over his room, waits, Lidarr not asked. Never an
    /// administrator's Want waiting for room.
    #[tokio::test]
    async fn an_administrator_has_no_room_and_a_user_made_of_one_has() {
        use crate::app::requests::tests::{Copies, Strict, acting, not_had};
        use crate::app::requests::{Hands, WAITS_FOR_ROOM, ask_record_with};
        const S: &str = "00000000-0000-4000-8000-000000000411";
        const C: &str = "00000000-0000-4000-8000-000000000412";
        let serve = room_shape();
        let cat = serve.catalogue();
        for (name, admin) in [("root", true), ("carl", true), ("bob", false)] {
            serve.users.create(name, "pw", admin).unwrap();
        }
        put_file(cat, ROOT, "R/1.flac", Some(11), gb(300.0), 10, false, 100);
        put_file(cat, CARL, "C/1.flac", Some(12), gb(150.0), 10, false, 100);
        let room = |name: &str| rooms(&serve, &[serve.users.get(name).unwrap()]).unwrap().get(name).copied();
        assert_eq!((room("root"), room("carl")), (None, None), "administrators: no room");
        let listed = rooms(&serve, &serve.users.all()).unwrap();
        assert_eq!(listed.keys().collect::<Vec<_>>(), ["bob"], "each user's room, none for the administrators");
        let (strict, house) = (Strict::expecting_wants(&[S]), Copies::none());
        let h = Hands { lidarr: Some(&strict), soulseek: false, house: &house, disks: &Roomy };
        let (s, c) = (not_had(&serve, "Signal", S), not_had(&serve, "Carl's Want", C));
        let roots = acting("root", true, ask_record_with(&h, &serve, "root", s)).await.unwrap();
        assert_ne!(roots.fetched.as_deref(), Some(WAITS_FOR_ROOM), "an administrator's Want never waits: {roots:?}");
        assert_eq!(strict.wanted(), [S], "root's handed: no room, whatever he holds");
        serve.users.set_admin("carl", false, Some("root")).unwrap();
        assert_eq!(room("carl"), Some(Room { size: gb(100.0), used: gb(150.0), full: true }), "a user now: 100 GB, 150 used, over");
        let carls = acting("carl", false, ask_record_with(&h, &serve, "carl", c)).await.unwrap();
        assert_eq!(serve.userdata.request(carls.id).unwrap().unwrap().fetched.as_deref(), Some(WAITS_FOR_ROOM), "a user over room: his new Want waits");
        assert_eq!(strict.wanted(), [S], "Lidarr not asked for carl's");
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
        serve.users.set_admin("carl", true, Some("root")).unwrap();
        assert_eq!(room("carl"), None, "an administrator again: none");
        serve.users.set_room("carl", gb(200.0)).unwrap();
        assert_eq!(room("carl"), None, "a room set on an administrator: none while he is one");
        serve.users.set_admin("carl", false, Some("root")).unwrap();
        assert_eq!(room("carl"), Some(Room { size: gb(200.0), used: gb(150.0), full: false }), "the 200 set, his as a user (OQ-R3)");
        assert_eq!(room("root"), None);
    }

    /// The libraries as the library lists them (`Active::libraries`) with own music switched off for `off` (0.17: the folder stays
    /// in use, no one's), each by its catalogue folder; and the own ones' owners.
    #[allow(clippy::type_complexity)]
    fn listed(cat: &Catalogue, off: &[&str]) -> (Vec<(i64, LibraryKind)>, Vec<(String, Option<String>)>) {
        use crate::settings::{Libraries, LibrarySetting, SourceConfig};
        let placed = |id: &str| crate::placement::PlacedLibrary { id: id.into(), name: id.into(), folders: vec![id.into()], paths: Vec::new(), read_only: false };
        let values = [(crate::settings::OWN_MUSIC.to_string(), serde_json::json!("/own")), (crate::settings::OWN_MUSIC_OFF.to_string(), serde_json::json!(off))].into_iter().collect();
        let libraries = Libraries(vec![
            LibrarySetting::new(FOLDERS[0], LibraryKind::Main),
            LibrarySetting::new(FOLDERS[1], LibraryKind::Incoming),
            LibrarySetting::new(FOLDERS[2], LibraryKind::Incoming),
        ]);
        let cfg = SourceConfig { id: "cratarr".into(), kind: crate::source::cratarr::KIND.id.into(), enabled: true, libraries, values };
        let placement = crate::placement::Placement { roots: Vec::new(), libraries: FOLDERS.iter().map(|f| placed(f)).collect() };
        let a = crate::app::library::Active { cfg, source: None, placement, error: None };
        let used = a.libraries();
        let folders = cat.folders();
        let kinds = used.iter().filter_map(|u| Some((folders.id_of(Path::new(&u.id))?, u.kind))).collect();
        (kinds, used.into_iter().filter(|u| u.kind == LibraryKind::Own).map(|u| (u.id, u.owner)).collect())
    }

    /// AL01.9 (INTERACTIONS top 7, I27; OQ-R2): ann's own music parked (its rows kept) counts nothing; eve's switched off holds
    /// 5 GB in her folder, a library in use with no owner, and counts 5 GB both while off and once on again. Never a parked file
    /// counted; never switching own music off a way to empty a room.
    #[test]
    fn parked_files_count_nothing_and_the_switch_changes_nothing() {
        let serve = room_shape();
        let cat = serve.catalogue();
        put_file(cat, ANN, "A/1.flac", Some(1), gb(2.0), 10, false, 100);
        put_file(cat, EVE, "E/1.flac", Some(2), gb(5.0), 10, false, 100);
        let (kinds, owners) = listed(cat, &["eve"]);
        assert!(owners.contains(&("/own/eve".to_string(), None)), "switched off: her folder in use, no one's: {owners:?}");
        serve.library.set_kinds(kinds);
        assert_eq!(used(&serve, "eve"), gb(5.0), "switched off: as before");
        let (kinds, owners) = listed(cat, &[]);
        assert!(owners.contains(&("/own/eve".to_string(), Some("eve".to_string()))));
        serve.library.set_kinds(kinds);
        assert_eq!(used(&serve, "eve"), gb(5.0), "on again: the same");
        assert_eq!(used(&serve, "ann"), gb(2.0));
        cat.set_folders(FOLDERS.iter().filter(|f| **f != "/own/ann").map(PathBuf::from).collect()).unwrap();
        let (kinds, _) = listed(cat, &[]);
        serve.library.set_kinds(kinds);
        assert_eq!(used(&serve, "ann"), 0, "parked: nothing");
        assert_eq!(total(&serve), gb(5.0), "a parked file counted for no one");
    }

    /// AL01.10 (Q3): a users file as 0.16 left production's (users asking with no yes of their own, accounts from the directory, an
    /// administrator with `"permissions": {}`), no room anywhere; bob's own music holds 130 GB. 0.17 starts: bob's room is 100 GB,
    /// 130 used, over; nothing removed or moved, the users file as it was. Never a file of bob's touched, nor a room over 100 GB
    /// given at the upgrade. Bob then Wants a record not had (AL02): it waits, Lidarr not asked, his rows and the file as they were.
    #[tokio::test]
    async fn over_room_at_the_upgrade_removes_nothing() {
        let data = std::env::temp_dir().join(format!("cratarr-room-upgrade-{}", std::process::id()));
        std::fs::remove_dir_all(&data).ok();
        std::fs::create_dir_all(&data).unwrap();
        let asks = r#"{ "offline": true, "playlists": true, "share": true, "links": false, "request": true, "auto_approve": false, "manage_requests": false }"#;
        let file = format!(
            r#"[
  {{ "name": "root", "password": "", "admin": true, "directory": true, "permissions": {{}} }},
  {{ "name": "carl", "password": "carl's", "admin": true, "permissions": {asks} }},
  {{ "name": "ann", "password": "", "directory": true, "permissions": {asks} }},
  {{ "name": "bob", "password": "", "directory": true, "permissions": {asks} }},
  {{ "name": "cy", "password": "", "directory": true, "permissions": {{ "offline": false, "playlists": false, "share": false, "links": false, "request": true, "auto_approve": false, "manage_requests": false }} }},
  {{ "name": "dee", "password": "dee's", "permissions": {asks} }},
  {{ "name": "eve", "password": "eve's", "permissions": {asks} }}
]"#
        );
        let users_file = data.join("subsonic-users.json");
        std::fs::write(&users_file, &file).unwrap();
        let serve = room_shape_in(Some(&data), None);
        let cat = serve.catalogue();
        for n in 0..13 {
            put_file(cat, BOB, &format!("B/{n}.flac"), Some(100 + n), gb(10.0), 10, false, 100);
        }
        let his = rows_of(cat, BOB);
        let all = serve.users.all();
        assert!(all.iter().all(|u| u.room_bytes.is_none()), "no room in the file: none set");
        let rooms = rooms(&serve, &all).unwrap();
        assert_eq!(rooms.get("bob"), Some(&Room { size: gb(100.0), used: gb(130.0), full: true }), "the default room, 130 used, over");
        assert_eq!(rooms.len(), 5, "each user's room; none for the two administrators");
        assert!(rooms.values().all(|r| r.size == ROOM), "no room over 100 GB given");
        // Bob's Want after the upgrade, over room (AL01.10's Want part): it waits, Lidarr asked nothing.
        use crate::app::requests::tests::{Copies, Strict, acting, not_had};
        use crate::app::requests::{Hands, WAITS_FOR_ROOM, ask_record_with};
        let (strict, house) = (Strict::expecting_nothing(), Copies::none());
        let h = Hands { lidarr: Some(&strict), soulseek: false, house: &house, disks: &Roomy };
        let wanted = not_had(&serve, "After the Upgrade", "00000000-0000-4000-8000-000000000421");
        let asked = acting("bob", false, ask_record_with(&h, &serve, "bob", wanted)).await.unwrap();
        assert_eq!(serve.userdata.request(asked.id).unwrap().unwrap().fetched.as_deref(), Some(WAITS_FOR_ROOM), "his Want waits");
        assert!(strict.wanted().is_empty() && strict.violations().is_empty(), "Lidarr asked nothing: {:?}", strict.violations());
        assert_eq!(rows_of(cat, BOB), his, "nothing of bob's removed or moved");
        assert_eq!(std::fs::read_to_string(&users_file).unwrap(), file, "the users file as it was");
        std::fs::remove_dir_all(&data).ok();
    }

    /// Rows at production's size (AL01.11; DATA-CENSUS-0.17): 34,461 files of 30 MB over Music (24,747), Lidarr's Incoming
    /// (9,328) and Soulseek's (386); bob's own music of 1,000 originals, ann's of 1,000 copies (500 of Music's files, 500 of
    /// bob's), sharing their blocks; 91 records of ten files each in Lidarr's Incoming, come after they were asked for, each asked
    /// for by one of the five users in turn, 79 of them with a group.
    fn production_rows(serve: &Serve) {
        let cat = serve.catalogue();
        let n = |to: i64| format!("WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {to})");
        cat.execute_for_tests(&format!("{} INSERT INTO tracks (id, born) SELECT i, 1 FROM n;", n(35_461)));
        cat.execute_for_tests(&format!(
            "{} INSERT INTO files (folder, rel, size, mtime, state, track, came) SELECT CASE WHEN i <= 24747 THEN 0 WHEN i <= 34075 THEN 1 ELSE 2 END,
               'A' || (i / 10) || '/' || i || '.flac', 30000000, 1, 1, i, 1000 FROM n;",
            n(34_461)
        ));
        cat.execute_for_tests(&format!(
            "{} INSERT INTO files (folder, rel, size, mtime, state, track, came, shared) SELECT {BOB}, 'B/' || i || '.flac', 30000000, 1, 1, i + 34461, 500, 0 FROM n;",
            n(1_000)
        ));
        cat.execute_for_tests(&format!(
            "INSERT INTO files (folder, rel, size, mtime, state, track, came, shared)
               SELECT {ANN}, 'C/' || rel, size, mtime, 1, track, 2000, 1 FROM files WHERE (folder = {MUSIC} AND track <= 500) OR (folder = {BOB} AND track <= 34961);"
        ));
        cat.execute_for_tests(&format!("{} INSERT INTO records (id, born, title, artist) SELECT i, 1, 'Record ' || i, 'Glass Harbour' FROM n;", n(91)));
        cat.execute_for_tests("UPDATE tracks SET record = (id - 24748) / 10 + 1 WHERE id BETWEEN 24748 AND 25657;");
        let users = ["ann", "bob", "cy", "dee", "eve"];
        for record in 1..=91i64 {
            let group = (record <= 79).then(|| format!("00000000-0000-4000-8000-{record:012}"));
            serve.userdata.add_request(&asking(users[(record as usize - 1) % 5], record, 500, group.as_deref())).unwrap();
        }
    }

    /// AL01.11: the rooms of all seven accounts, as the Users panel asks them, at production's size: one statement on a reader of
    /// its own, answered while a build holds the writer, its time printed; nothing of `analysis_shape` read (the table set aside
    /// for it); never a query per file or per request. Who asked first for what is the requesters' kept read (R16), the Albums
    /// page's, read once here before. The counts: ann's own copies all free, her 19 arrivals; bob's 1,000 originals and his 18.
    #[test]
    fn every_accounts_room_is_one_read() {
        let dir = std::env::temp_dir().join(format!("cratarr-room-one-read-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let serve = room_shape_in(None, Some(&dir.join("catalogue.sqlite")));
        production_rows(&serve);
        for (name, admin) in [("root", true), ("carl", true), ("ann", false), ("bob", false), ("cy", false), ("dee", false), ("eve", false)] {
            serve.users.create(name, "pw", admin).unwrap();
        }
        let cat = serve.catalogue();
        cat.execute_for_tests("ALTER TABLE analysis_shape RENAME TO shape_set_aside");
        assert_eq!(crate::app::requests::requesters(&serve).len(), 91, "who asked for what, as the Albums page reads it");
        let opened = cat.readers_for_tests();
        let build = cat.writer_for_tests();
        let (tx, rx) = std::sync::mpsc::channel();
        let s = serve.clone();
        std::thread::spawn(move || {
            let at = std::time::Instant::now();
            let rooms = rooms(&s, &s.users.all());
            tx.send((rooms.map_err(|e| e.to_string()), at.elapsed())).ok()
        });
        let (rooms, took) = rx.recv_timeout(std::time::Duration::from_secs(60)).expect("answered while a build holds the writer");
        drop(build);
        let rooms = rooms.unwrap();
        eprintln!("every account's room, 36,461 files and 91 requests: {took:?}");
        assert_eq!(cat.readers_for_tests() - opened, 1, "one reader for them all");
        assert_eq!(rooms.len(), 5, "the five users'; none for the administrators");
        let used = |name: &str| rooms[name].used;
        assert_eq!(used("ann"), 19 * 10 * 30_000_000, "her copies all free, her 19 arrivals");
        assert_eq!(used("bob"), 1_000 * 30_000_000 + 18 * 10 * 30_000_000, "his originals and his 18");
        assert_eq!((used("cy"), used("dee"), used("eve")), (18 * 300_000_000, 18 * 300_000_000, 18 * 300_000_000));
        let source = include_str!("../../../store/src/catalogue/mod.rs").replace("\r\n", "\n");
        let at = source.find("pub fn room_rows").unwrap();
        let body = &source[at..at + source[at..].find("\n    }\n").unwrap()];
        assert_eq!(body.matches("prepare").count(), 1, "one statement: {body}");
        let mine = include_str!("room.rs").replace("\r\n", "\n");
        let at = mine.find("pub fn used_by").unwrap();
        let used_by = &mine[at..at + mine[at..].find("\n}\n").unwrap()];
        assert_eq!(used_by.matches("room_rows(").count(), 1, "one read for every account: {used_by}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// RA4: room reads a rule of its own (`FREE_OF_ROOM`), never the backup's (`SHARED_WITH`, where only an original covers a
    /// copy), on the one test of the same bytes both hold: the other file's track, size and time, present, outside any trash.
    #[test]
    fn room_reads_its_own_rule_not_the_backups() {
        use crate::catalogue::{FREE_OF_ROOM, SHARED_WITH};
        let same = |o: &str, f: &str| format!("{o}.track = {f}.track AND {o}.size = {f}.size AND {o}.mtime = {f}.mtime AND {o}.folder >= 0 AND {o}.gone_at IS NULL AND {o}.trashed IS NULL");
        assert!(SHARED_WITH.contains(&same("o", "f")), "{SHARED_WITH}");
        assert!(FREE_OF_ROOM.contains(&same("g", "f")) && FREE_OF_ROOM.contains(&same("h", "g")), "{FREE_OF_ROOM}");
        assert!(!FREE_OF_ROOM.contains(":kept") && !SHARED_WITH.contains(":main") && !SHARED_WITH.contains(":own"));
        let source = include_str!("../../../store/src/catalogue/mod.rs").replace("\r\n", "\n");
        let at = source.find("pub fn room_rows").unwrap();
        let body = &source[at..at + source[at..].find("\n    }\n").unwrap()];
        assert!(body.contains("{FREE_OF_ROOM}") && !body.contains("SHARED_WITH") && !body.contains("shared_copies"), "{body}");
    }

    // AL03 (run 13): the disk floor (SCENARIOS-RUNS-13-14's AL03.4-AL03.6; the owner's Q4 and Q6; OQ-F2, OQ-F3).

    /// The lines `f` says in the log of the floor's own (`room`).
    fn floor_said(f: impl FnOnce()) -> Vec<String> {
        crate::diagnostics::logged_lines(f).into_iter().filter(|l| l.starts_with("room: ")).collect()
    }

    /// AL03.4 (OQ-F3): the floor 200 GB (none set: 0084's), the disk at 199.9, 200.1, 199.95, 205, a restart, 205 and 210.5 GB in
    /// turn, the flag read after each: on at 199.9, still on at 200.1, 199.95 and 205, after the restart too, off at 210.5 (the floor
    /// and 10 GB). So it changes twice, each said once in the log, and the user data's meta keeps it over the restart. Never flapping
    /// at the floor; never a restart turning it off.
    #[test]
    fn the_flag_flips_once_per_crossing() {
        let data = std::env::temp_dir().join(format!("cratarr-room-floor-{}", std::process::id()));
        std::fs::remove_dir_all(&data).ok();
        std::fs::create_dir_all(&data).unwrap();
        let dirs = [PathBuf::from(FOLDERS[1])];
        let stand = Stand::of(&[(FOLDERS[1], 199.9)]);
        let read = |serve: &Serve, n: f64, seen: &mut Vec<bool>| {
            stand.set(FOLDERS[1], Some(n));
            seen.push(short(&serve.userdata, &stand, &dirs));
        };
        let mut seen = Vec::new();
        let serve = room_shape_in(Some(&data), None);
        assert_eq!(floor(&serve.userdata), gb(200.0), "none set: 200 GB");
        let mut said = floor_said(|| {
            for n in [199.9, 200.1, 199.95, 205.0] {
                read(&serve, n, &mut seen);
            }
        });
        drop(serve);
        let serve = room_shape_in(Some(&data), None);
        assert_eq!(serve.userdata.meta(SHORT_KEY).unwrap().as_deref(), Some(SHORT), "kept in the user data's meta over the restart");
        said.extend(floor_said(|| {
            for n in [205.0, 210.5] {
                read(&serve, n, &mut seen);
            }
        }));
        assert_eq!(seen, [true, true, true, true, true, false], "on below the floor, off only at the floor and 10 GB, a restart changing nothing");
        assert_eq!(said.len(), 2, "twice, once per crossing: {said:?}");
        assert!(said[0].contains("short of space") && said[1].contains("space again"), "{said:?}");
        drop(serve);
        std::fs::remove_dir_all(&data).ok();
    }

    /// AL03.5 (Q4: the trash gives room back at once, and the disk when it is emptied): the disk 5 GB below the floor, ann over her
    /// room by her own music, her Want waiting for room. Her delete of 20 GB into her own music's trash: her room down at once and
    /// under (AL01.5), but the disk holds the trash, so the floor still reads below and her Want waits on, for space now, Lidarr asked
    /// nothing; Health's item says the trash holds 20 GB. The trash emptied (the purge, the disk 20 GB freer): no longer short, her
    /// Want goes at the round, and Health says nothing of space.
    #[tokio::test]
    async fn the_trash_frees_room_at_once_and_the_disk_when_emptied() {
        use crate::app::requests::tests::{Copies, Strict, acting, not_had, with_accounts};
        use crate::app::requests::{Hands, Holding, RequestState, WAITS_FOR_ROOM, WAITS_FOR_SPACE, ask_record_with, request_state, round_with};
        const G: &str = "00000000-0000-4000-8000-000000000471";
        let serve = room_shape();
        with_accounts(&serve);
        let cat = serve.catalogue();
        put_file(cat, ANN, "Big/1.flac", Some(1), gb(20.0), 10, false, 100);
        put_file(cat, ANN, "Rest/1.flac", Some(2), gb(85.0), 10, false, 100);
        let stand = Stand::of(&[(FOLDERS[1], 195.0), (FOLDERS[2], 195.0)]);
        let (strict, house) = (Strict::expecting_wants(&[G]), Copies::none());
        let h = Hands { lidarr: Some(&strict), soulseek: false, house: &house, disks: &stand };
        let r = not_had(&serve, "Night Ferry", G);
        let asked = acting("ann", false, ask_record_with(&h, &serve, "ann", r)).await.unwrap();
        let row = || serve.userdata.request(asked.id).unwrap().unwrap();
        assert_eq!((row().fetched.as_deref(), used(&serve, "ann")), (Some(WAITS_FOR_ROOM), gb(105.0)), "over her room: it waits for room");
        // Her delete, into her own music's trash.
        cat.mark_trashed(ANN, "Big", "100/Big").unwrap();
        assert_eq!(used(&serve, "ann"), gb(85.0), "her room down at once, under");
        assert!(short_on(&serve, &stand), "the disk holds the trash: still below the floor");
        round_with(&h, &serve, crate::app::moment::now_secs()).await.unwrap();
        assert_eq!(row().fetched.as_deref(), Some(WAITS_FOR_SPACE), "her Want waits on, for space now");
        assert_eq!(request_state(&row(), Holding::None, None, None), RequestState::WaitingForSpace);
        assert!(strict.wanted().is_empty(), "Lidarr asked nothing while short");
        let f = crate::app::health_requests::short_of_space(&serve, &stand).expect("Health says it");
        let detail = f.detail.clone().unwrap_or_default();
        assert!(detail.contains("195 GB free, below the floor of 200 GB") && detail.contains("The trash holds 20 GB: emptying it frees that"), "{detail}");
        // The purge: the trash emptied, the disk 20 GB freer, its rows gone at the next pass.
        cat.execute_for_tests("DELETE FROM files WHERE trashed IS NOT NULL");
        for d in [FOLDERS[1], FOLDERS[2]] {
            stand.set(d, Some(215.0));
        }
        assert!(!short_on(&serve, &stand), "the disk recovered");
        round_with(&h, &serve, crate::app::moment::now_secs()).await.unwrap();
        assert_eq!(strict.wanted(), [G], "her Want goes at the round");
        assert!(row().fetched.as_deref().is_some_and(|f| f.contains("Lidarr is looking for it")), "{:?}", row());
        assert!(crate::app::health_requests::short_of_space(&serve, &stand).is_none(), "Health says nothing of space");
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
    }

    /// AL03.6 (OQ-F2): Music alone on a disk with 50 GB free; Lidarr's and Soulseek's Incoming, an Uploads library and the own-music
    /// folder on one with 900 GB: Music's disk alone pauses no one. That disk at 150 GB: short. Its free space then unreadable: not
    /// short, said once in the log however often it is read. The stand-in is asked only of the Incoming libraries' folders (Uploads
    /// among them) and the own-music folder: never of Music's, of each account's own folder, nor of the data folder (it panics).
    #[test]
    fn the_floor_reads_the_disks_users_music_lands_on() {
        use crate::settings::{Libraries, Saved, SourceConfig};
        const UPLOADS: &str = "/library/uploads";
        let data = std::env::temp_dir().join(format!("cratarr-room-disks-{}", std::process::id()));
        std::fs::remove_dir_all(&data).ok();
        std::fs::create_dir_all(&data).unwrap();
        let values = [(crate::settings::OWN_MUSIC.to_string(), serde_json::json!("/own"))].into_iter().collect();
        let cfg = SourceConfig { id: "cratarr".into(), kind: crate::source::cratarr::KIND.id.into(), enabled: true, libraries: Libraries::default(), values };
        crate::settings::save(&data.join("settings.json"), &Saved { sources: vec![cfg], ..Saved::default() }).unwrap();
        let serve = Serve::open(Some(&data), crate::catalogue::Rules::default(), &|_| None).unwrap();
        let folders: Vec<PathBuf> = FOLDERS.iter().chain([&UPLOADS]).map(|f| PathBuf::from(*f)).collect();
        serve.attach_catalogue(Arc::new(Catalogue::open(None, folders).unwrap()));
        serve.library.set_kinds(kinds().into_iter().chain([(FOLDERS.len() as i64, LibraryKind::Incoming)]).collect());
        assert_eq!(serve.own_music(), Some(PathBuf::from("/own")));
        let lands = [FOLDERS[1], FOLDERS[2], UPLOADS, "/own"];
        let stand = Stand::of(&[(FOLDERS[0], 50.0)]);
        for d in lands {
            stand.set(d, Some(900.0));
        }
        assert!(!short_on(&serve, &stand), "Music's disk alone pauses no one");
        let mut where_music_lands: Vec<PathBuf> = lands.iter().map(|d| PathBuf::from(*d)).collect();
        where_music_lands.sort();
        assert_eq!(stand.asked(), where_music_lands, "only the Incoming libraries' folders and the own-music folder");
        for d in lands {
            stand.set(d, Some(150.0));
        }
        assert!(short_on(&serve, &stand), "the disk users' music lands on at 150 GB");
        for d in lands {
            stand.set(d, None);
        }
        let said = floor_said(|| {
            assert!(!short_on(&serve, &stand), "unreadable: not short");
            assert!(!short_on(&serve, &stand), "read again: still not");
        });
        assert_eq!(said.len(), 1, "said once in the log: {said:?}");
        assert!(said[0].contains("could not be read"), "{said:?}");
        assert_eq!(stand.asked(), where_music_lands, "never Music's, an account's own folder, nor the data folder");
        drop(serve);
        std::fs::remove_dir_all(&data).ok();
    }
}
