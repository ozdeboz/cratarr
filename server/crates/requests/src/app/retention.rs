//! RT01 (the owner's decision, "Retention of records not had"): records not had (from discographies, lists and requests, with
//! no tracks) pile up three to one against records had. One that no list showed and nobody opened for six months is forgotten,
//! with its references and credits; a tombstone keeps its id, so an old link answers "no longer known", and no id is given
//! again. In the same daily pass (RT01b), the kept lists of a record not had go after thirty days unopened, read again when
//! asked for, and an artist left with nothing is forgotten as a record is. Before them (RT01c, F3), the library's artists'
//! MusicBrainz and Discogs lists are read again, a month's thirtieth a day, so what is read counts as seen. After them (C56), the
//! labels' rows read thirty days ago go: a cache, not records (Q33).
//!
//! - **Never while something points at it.** The catalogue keeps what it holds itself (`Catalogue::forget_quiet`: a choice or
//!   a reference by hand, a confirmation, a merge, an expected folder, a retired record, a link from before 0.16, a request's
//!   shared name); this gathers the rest: every request's record, whatever its status, every record on Soulseek's list, and
//!   what the user data and the stores in the data folder name, as for an artist (`protected`, Z5). An artist is kept by a
//!   record's credit, a reference by hand or confirmed, an id from before 0.16 (`Catalogue::forget_quiet_artists`), and by a
//!   track's credit, anything the user data names them in (a favourite, plays, a rating, words) and the stores in the data
//!   folder (every account's crates, their seeds and conditions; the queues): `protected_artists`. A track's credit counts its
//!   files out of the library's sight too, in a trash or a parked library (RF16, `credited_aside`): an Undo, or the library brought
//!   back, finds its guests as themselves.
//! - **Artists wait for the library** (W2): a track's credit is held in Cratarr's own library alone, so no artist is forgotten
//!   until it has been built whole at its layout, and while it credits nobody though the catalogue holds live tracks
//!   (`credits_kept`). One of another layout, or library.sqlite deleted as a repair, holds no credit until its first build ends;
//!   a restart with it built waits for nothing. Records not had and kept lists go as ever.
//! - **Seen** is a list read: each sees what it lists (`Catalogue::keep_listing`), Bandcamp's too (Z4, the owner's rule: an
//!   artist page's reads refresh what they show), and MusicBrainz's or Discogs' the artist's records only Bandcamp knows
//!   (`Catalogue::bandcamp_only_seen`), as Bandcamp's lists only its search box's first answers (O4). A page showing a record sees
//!   nothing; opening it does (C54). The paced re-read (`reread`) comes to each library artist once a month, never to Bandcamp's
//!   list: a library artist neither MusicBrainz nor Discogs knows has their records only Bandcamp knows seen instead (Z4); a list
//!   it cannot read (an outage, a switch, a pause) keeps that artist's records not had seen instead.
//! - **Once a day:** looked at hourly, run where the catalogue's note says the last pass is a day old or more (`pass`), so a
//!   restart does not run it again.
//! - **After a long gap** (Z9: an outage, a clock that jumped forward): a pass more than three days after the last re-reads as
//!   ever, but forgets no record until a whole month of re-reads has passed (`forgetting_waits`), as what the re-read has not
//!   come to again would look unseen.
//! - **A clock that ran ahead and was put right** (U9): a pass or a wait noted more than a day ahead of now is no reason to wait for
//!   it. The pass runs, and the forgetting waits a month from now at most.

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Result;

use super::discover::{ArtistAsked, ArtistList};
use crate::catalogue::Catalogue;
use crate::serve::Serve;
use crate::userdata::UserData;

/// When the pass last ran, in the catalogue's notes (`meta`): at most once a day, across restarts.
const RAN_AT: &str = "retention_at";
const DAY: i64 = 86_400;
/// Until when the forgetting of records waits (Z9), in the catalogue's notes: across restarts.
const WAITS_UNTIL: &str = "retention_waits_until";
/// How long after the last pass a pass is late (Z9): the re-read, a thirtieth of the artists a day, has fallen behind.
const LATE: i64 = 3 * DAY;
/// A whole month of the paced re-read (RT01c): each library artist come to again.
const REREAD_MONTH: i64 = 30 * DAY;

pub use super::retention_base::{ARTIST_NO_LONGER_KNOWN, NO_LONGER_KNOWN, unknown_album, unknown_artist};
use super::retention_protected::{protected_here, stores_name};

/// Whether the pass is due at `now`: never run, last run a day ago or more, or noted more than a day ahead of now (U9: a clock that
/// ran ahead, put right since, would hold every pass off till it caught up).
fn due(cat: &Catalogue, now: i64) -> bool {
    cat.meta(RAN_AT).and_then(|v| v.parse::<i64>().ok()).is_none_or(|at| now - at >= DAY || at - now > DAY)
}

/// Whether the forgetting of records waits at `now` (Z9): a pass more than `LATE` after the last makes it wait a whole month of
/// re-reads from now (`REREAD_MONTH`), as the re-read has fallen behind and what it has not come to again would look unseen; a
/// wait already there is lengthened, never cut short, but to a month from now at most (U9: one noted by a clock that ran ahead
/// would otherwise hold for as long as it ran ahead). Noted in the catalogue (`WAITS_UNTIL`). Before the pass is noted. Blocking.
fn forgetting_waits(cat: &Catalogue, now: i64) -> bool {
    let noted = |key: &str| cat.meta(key).and_then(|v| v.parse::<i64>().ok());
    let was = noted(WAITS_UNTIL).unwrap_or(0);
    let mut until = was.min(now + REREAD_MONTH);
    if noted(RAN_AT).is_some_and(|at| now - at > LATE) {
        until = until.max(now + REREAD_MONTH);
    }
    if until != was {
        cat.set_meta(WAITS_UNTIL, &until.to_string());
    }
    now < until
}

/// What keeps a record not had from outside the catalogue, however long nobody saw it: every request's record, whatever its
/// status (`UserData::request_records`), every record on Soulseek's list, and, as an artist is kept (`protected_artists`, Z5),
/// anything the user data names it in (`UserData::records_named`: a heart, plays, a rating, words, tags) and the stores in the
/// data folder `data` (`stores_name`). Blocking.
pub fn protected(ud: &UserData, data: Option<&std::path::Path>) -> Result<BTreeSet<i64>> {
    let mut kept = protected_here(ud, data)?;
    kept.extend(super::soulseek::item_records());
    Ok(kept)
}

/// The artists Cratarr's own library credits (`Library::artists_credited`: a track's credit is held there alone), where they can
/// be trusted to keep every artist a track credits (W2): the library built whole at its layout (`Library::built`), and crediting
/// someone while the catalogue holds live tracks (a build that read no rows, every library off at that moment, credits nobody).
/// None: no artist is forgotten this pass. Blocking.
fn credits_kept(served: &Serve) -> Result<Option<BTreeSet<i64>>> {
    if !served.library.built() {
        return Ok(None);
    }
    let credited = served.library.artists_credited()?;
    Ok((!credited.is_empty() || !served.catalogue().holds_live_tracks()?).then_some(credited))
}

/// What keeps an artist with no record from outside the catalogue (RT01b): a track's credit (`credited`, what Cratarr's own library
/// credits, and what a live track's files out of its sight credit, `credited_aside`), anything the user data names them in
/// (`UserData::artists_named`), and the stores in the data folder (`stores_name`). Blocking.
fn protected_artists(served: &Serve, credited: BTreeSet<i64>) -> Result<BTreeSet<i64>> {
    let mut kept = served.userdata.artists_named()?;
    kept.extend(credited);
    kept.extend(credited_aside(served)?);
    kept.extend(stores_name(served.data_dir.as_deref(), |text| crate::serve::ids::artists_named(text).collect())?);
    Ok(kept)
}

/// The artists a live track's files out of the library's sight credit (RF16, the review of run 13, SA1): in a trash (an Undo can bring
/// them back), in a parked library (it comes back as itself, A3), or gone a while; a build leaves them out, so the library's credits
/// lack their guests. Each file's artists as the library credits them (`artists` else `artist`, `albumartists` else `albumartist`,
/// through the rules' values), by their key. Blocking.
fn credited_aside(served: &Serve) -> Result<BTreeSet<i64>> {
    let files = served.catalogue().artist_tags_set_aside()?;
    if files.is_empty() {
        return Ok(BTreeSet::new());
    }
    let (rules, known) = (served.rules(), served.catalogue().artist_ids()?);
    let mut kept = BTreeSet::new();
    for (folder, tags) in &files {
        let all = |key: &str| tags.iter().filter(|(k, _)| k == key).map(|(_, v)| v.clone()).collect::<Vec<String>>();
        for (list, one) in [("artists", "artist"), ("albumartists", "albumartist")] {
            let from = if all(list).is_empty() { one } else { list };
            for name in rules.values(*folder, from, &all(from)) {
                if let Some(id) = known.get(&crate::serve::ids::artist_key(&name)) {
                    kept.insert(*id);
                }
            }
        }
    }
    Ok(kept)
}

/// What the day's re-read came to (RT01c), by (artist, source): the lists read, and those it could not read (failed, switched
/// off, the background paused), whose artist's records not had it kept seen instead.
#[derive(Debug, Default, PartialEq)]
pub struct Reread {
    pub read: Vec<(i64, &'static str)>,
    pub kept_seen: Vec<(i64, &'static str)>,
}

/// What a pass came to: the day's re-read (RT01c), whether the forgetting of records waited (Z9), and what it let go of: the
/// records not had (RT01), their kept lists and the artists left with nothing (RT01b), and the labels' rows read thirty days ago
/// (C56); and whether the artists waited for the library (W2, `credits_kept`).
#[derive(Debug, Default, PartialEq)]
pub struct Passed {
    pub reread: Reread,
    pub waits: bool,
    pub records: Vec<i64>,
    pub lists: usize,
    pub artists: Vec<i64>,
    pub artists_wait: bool,
    pub label_rows: usize,
}

/// The day's re-read of the library artists' lists (RT01c, F3), paced: the thirtieth of them come to longest ago
/// (`Catalogue::rereads_due`), one artist at a time, each due source in turn, behind any page's question to MusicBrainz
/// (`musicbrainz::behind`) and in the source's one turn (`discover::reread_one`). A list it cannot read (the read fails or runs
/// past the cut, the source is switched off or not among `sources`, the background is paused) keeps the artist's records not
/// had seen at `now` instead, so an outage, a switch or a pause never makes an album forgotten. A library artist that the lists
/// read now do not know (Z4: one only Bandcamp knows, whose list it never reads) has their records only Bandcamp knows seen at
/// `now`, as a list that knows them sees them (`discover::read_one`). Each is come to again a month later, whatever came of it.
/// One re-read at a time in the process.
async fn reread(cat: &Arc<Catalogue>, sources: &[Arc<dyn ArtistList>], paused: fn() -> bool, now: i64) -> Result<Reread> {
    static ONE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _one = ONE.lock().await;
    let c = cat.clone();
    let batch = crate::run::spawn_blocking(move || c.rereads_due(now)).await??;
    let mut done = Reread::default();
    for (artist, due_at) in batch {
        let c = cat.clone();
        let Some((name, refs)) = crate::run::spawn_blocking(move || c.artist_known(artist)).await?? else { continue };
        let asked = ArtistAsked::of(artist, name, refs);
        // Whether a list read now knew them, and whether one did not (Z4).
        let (mut known, mut unknown) = (false, false);
        for source in due_at {
            let readable = sources.iter().find(|s| s.source() == source).filter(|s| s.on() && !paused());
            let read = match readable {
                Some(s) => match super::musicbrainz::behind(super::discover::reread_one(cat, &asked, s.as_ref())).await {
                    Ok(true) => {
                        known = true;
                        true
                    }
                    Ok(false) => {
                        unknown = true;
                        true
                    }
                    Err(e) => {
                        tracing::info!(target: "discover", artist, source, "an artist's list not read again: {e:#}");
                        false
                    }
                },
                None => false,
            };
            let c = cat.clone();
            crate::run::spawn_blocking(move || -> Result<()> {
                if !read {
                    c.artist_records_seen(artist, now)?;
                }
                c.reread_tried(artist, source, now)
            })
            .await??;
            if read { done.read.push((artist, source)) } else { done.kept_seen.push((artist, source)) }
        }
        // Z4: no list read now knows them: their records only Bandcamp knows (or no source) seen all the same, so a library artist
        // only Bandcamp knows keeps what their page's Bandcamp list showed. One a list knows had them seen by it.
        if unknown && !known {
            let c = cat.clone();
            crate::run::spawn_blocking(move || c.bandcamp_only_seen(artist, now)).await??;
        }
    }
    Ok(done)
}

/// The day's pass at `now`, where due (`due`): the library artists' lists read again first (`reread`, RT01c), so what it reads
/// counts as seen; then what keeps a record and an artist gathered, the records nothing keeps forgotten (but while the forgetting
/// waits after a long gap, `forgetting_waits`, Z9), the kept lists let go of, the artists left with nothing forgotten (but while
/// Cratarr's own library's credits cannot be trusted, `credits_kept`, W2), the labels' rows read thirty days ago let go of (C56),
/// and the pass's time written. What it came to; None: not due.
pub async fn pass(served: Arc<Serve>, now: i64) -> Result<Option<Passed>> {
    pass_reading(served, now, &super::discover::reread_sources(), super::switches::background_paused).await
}

/// `pass`, reading by `sources` and pausing by `paused`: the server's own, or a test's.
async fn pass_reading(served: Arc<Serve>, now: i64, sources: &[Arc<dyn ArtistList>], paused: fn() -> bool) -> Result<Option<Passed>> {
    let cat = served.catalogue().clone();
    let c = cat.clone();
    if !crate::run::spawn_blocking(move || due(&c, now)).await? {
        return Ok(None);
    }
    let c = cat.clone();
    let waits = crate::run::spawn_blocking(move || forgetting_waits(&c, now)).await?;
    let reread = reread(&cat, sources, paused, now).await?;
    let done = crate::run::spawn_blocking(move || -> Result<Passed> {
        let cat = served.catalogue();
        let kept = protected(&served.userdata, served.data_dir.as_deref())?;
        let records = if waits { Vec::new() } else { cat.forget_quiet(now, &kept)? };
        let lists = cat.forget_quiet_lists(now, &kept)?;
        let credited = credits_kept(&served)?;
        let artists_wait = credited.is_none();
        let artists = match credited {
            Some(credited) => cat.forget_quiet_artists(now, &protected_artists(&served, credited)?)?,
            None => Vec::new(),
        };
        if !artists.is_empty() {
            // The library's names without theirs: a name credited again is given a new id, never one forgotten.
            served.library.set_artists(cat.artist_ids()?);
        }
        let label_rows = cat.forget_label_rows(now)?;
        cat.set_meta(RAN_AT, &now.to_string());
        Ok(Passed { reread, waits, records, lists, artists, artists_wait, label_rows })
    })
    .await??;
    if done.waits {
        tracing::info!(target: "library", "records not had: none forgotten until a month of re-reads has passed, the last pass long ago");
    }
    if done.artists_wait {
        tracing::info!(target: "library", "artists: none forgotten until the library is built");
    }
    if !done.reread.read.is_empty() || !done.reread.kept_seen.is_empty() {
        tracing::info!(target: "library", read = done.reread.read.len(), kept_seen = done.reread.kept_seen.len(), "the library's artists' lists read again, a month's share");
    }
    if !done.records.is_empty() {
        tracing::info!(target: "library", count = done.records.len(), "records not had that no list showed and nobody opened for six months forgotten");
    }
    if done.lists > 0 {
        tracing::info!(target: "library", count = done.lists, "kept lists of records not had, unopened for thirty days, let go of");
    }
    if !done.artists.is_empty() {
        tracing::info!(target: "library", count = done.artists.len(), "artists left with nothing forgotten");
    }
    if done.label_rows > 0 {
        tracing::info!(target: "library", count = done.label_rows, "labels' rows read thirty days ago let go of");
    }
    Ok(Some(done))
}

/// The pass looked at hourly, from ten minutes after the start (the start's own work first), run where due. Started once
/// Soulseek's list is read (`soulseek::start`), whose items keep their records, and the catalogue attached; started once, so
/// there is one re-reader.
pub fn start(served: Arc<Serve>) {
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    crate::run::spawn(async move {
        let every = std::time::Duration::from_secs(3600);
        let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + std::time::Duration::from_secs(600), every);
        loop {
            tick.tick().await;
            if let Err(e) = pass(served.clone(), super::moment::now_secs()).await {
                tracing::warn!(target: "library", "records not had: the forgetting did not run: {e:#}");
            }
        }
    });
}

/// Test helpers, for this module's tests and those moved up to the crates above (SPLIT-PLAN 3.2).
#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    use super::*;

    /// RT01's clock in the tests: now, and a month.
    pub const NOW: i64 = 1_800_000_000;

    pub const MONTH: i64 = 30 * 86_400;

    /// A record not had of `title`, last seen seven months before `NOW`.
    pub fn unseen(cat: &Catalogue, title: &str) -> i64 {
        let record = cat.record_for_tests(title, 0);
        cat.db_for_tests().execute("UPDATE records SET seen_at = ?2 WHERE id = ?1", rusqlite::params![record, NOW - 7 * MONTH]).unwrap();
        record
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::testing::*;
    use crate::serve::ids;

    /// Cratarr's own library, in `dir` where given (else in memory), its catalogue in memory with ids of its own above `floor`:
    /// Soulseek's list is one to the process, other tests' items on it.
    fn served(dir: Option<&std::path::Path>, floor: i64) -> Arc<Serve> {
        let served = Serve::open(dir, crate::catalogue::Rules::default(), &|_| None).unwrap();
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        cat.number_above(floor).unwrap();
        served.attach_catalogue(cat);
        served
    }

    /// A data folder of the test's own, empty.
    fn data_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-retention-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Every artist made so far born a month before `NOW`, whatever the real clock says (Z7: `artist_for` gives them its now, and
    /// from mid-January 2027 that is within a day of `NOW`).
    fn born_a_month_before(cat: &Catalogue) {
        cat.db_for_tests().execute("UPDATE artists SET born = ?1", [NOW - MONTH]).unwrap();
    }

    /// A pass that reads no list (no source given, nothing paused): what is due to be read again keeps its records seen.
    async fn quiet_pass(served: &Arc<Serve>, now: i64) -> Option<Passed> {
        pass_reading(served.clone(), now, &[], || false).await.unwrap()
    }

    /// Cratarr's own library built from its catalogue, as a start's check builds it (`Library::change`, `build_from`).
    fn build(served: &Serve) {
        let rules = crate::catalogue::FolderRules::default();
        let (_, print) = served.library.change(served.catalogue(), &rules).unwrap();
        served.library.build_from(served.catalogue(), &rules, &print).unwrap();
    }

    /// `quiet_pass` on a runtime of its own, on this thread, and what it logged (`diagnostics::logged_lines`).
    fn logged_pass(served: &Arc<Serve>, now: i64) -> (Passed, Vec<String>) {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let mut done = None;
        let lines = crate::diagnostics::logged_lines(|| done = runtime.block_on(quiet_pass(served, now)));
        (done.expect("due"), lines)
    }

    /// P2's day: three days before `NOW`.
    const BORN: i64 = NOW - 3 * DAY;
    /// Both Shores' two files in Music.
    const SHORES: [&str; 2] = ["Salt Meridian/Both Shores (2020)/01 Low Bank.flac", "Salt Meridian/Both Shores (2020)/02 Far Bank.flac"];
    /// What the pass says while the artists wait for the library (W2).
    const ARTISTS_WAIT: &str = "library: artists: none forgotten until the library is built";

    /// P2's library (W2): Music holds Both Shores by Salt Meridian, its second track featuring Ferro Lane; Salt Meridian, Ferro Lane
    /// and Nobody (credited nowhere, named by nothing) born three days before `NOW`, and Old Lane whose last credit went the day
    /// before `NOW`; none credited on a record (before P3); a record not had unseen for seven months, nothing keeping it. Cratarr's
    /// own library in `dir`, where one of another layout was built (a layout change: its tables dropped at open), not built since.
    struct Shores {
        served: Arc<Serve>,
        cat: Arc<Catalogue>,
        salt: i64,
        ferro: i64,
        nobody: i64,
        old: i64,
        quiet: i64,
    }

    fn shores(dir: &std::path::Path, floor: i64) -> Shores {
        rusqlite::Connection::open(dir.join("library.sqlite"))
            .unwrap()
            .execute_batch("CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT INTO meta VALUES ('built_from', 'an older layout'); PRAGMA user_version = 1;")
            .unwrap();
        let cat = Arc::new(Catalogue::open(None, vec![std::path::PathBuf::from("/music")]).unwrap());
        cat.number_above(floor).unwrap();
        cat.execute_for_tests(&format!(
            "INSERT INTO files (folder, rel, size, mtime, state, format, bitrate, duration_ms, came) VALUES
               (0, '{}', 24000000, 1760000000, 1, 'FLAC', 900, 200000, 1760000000), (0, '{}', 24001000, 1760000000, 1, 'FLAC', 900, 210000, 1760000000)",
            SHORES[0], SHORES[1]
        ));
        let album = [("album", "Both Shores"), ("albumartist", "Salt Meridian")];
        cat.tag_for_tests(0, SHORES[0], &[("title", "Low Bank"), ("artist", "Salt Meridian"), ("tracknumber", "1"), album[0], album[1]]);
        cat.tag_for_tests(
            0,
            SHORES[1],
            &[("title", "Far Bank"), ("artist", "Salt Meridian feat. Ferro Lane"), ("artists", "Salt Meridian"), ("artists", "Ferro Lane"), ("tracknumber", "2"), album[0], album[1]],
        );
        cat.identify().unwrap();
        let [salt, ferro, nobody, old] = ["Salt Meridian", "Ferro Lane", "Nobody", "Old Lane"].map(|n| cat.artist_for(n).unwrap());
        {
            let db = cat.db_for_tests();
            db.execute("UPDATE artists SET born = ?1", [BORN]).unwrap();
            db.execute("UPDATE artists SET born = ?2 WHERE id = ?1", rusqlite::params![old, NOW - MONTH]).unwrap();
            db.execute("INSERT INTO artists_bare (artist, since) VALUES (?1, ?2)", rusqlite::params![old, NOW - DAY]).unwrap();
        }
        let quiet = unseen(&cat, "Undertow");
        let served = Serve::open(Some(dir), crate::catalogue::Rules::default(), &|_| None).unwrap();
        served.attach_catalogue(cat.clone());
        Shores { served, cat, salt, ferro, nobody, old, quiet }
    }

    fn count(cat: &Catalogue, sql: &str) -> i64 {
        cat.db_for_tests().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn bare(cat: &Catalogue) -> Vec<(i64, i64)> {
        let db = cat.db_for_tests();
        let mut st = db.prepare("SELECT artist, since FROM artists_bare ORDER BY artist").unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    }

    /// P2.1 (W2): Cratarr's own library of another layout, its tables dropped at open and not built since: the pass forgets no
    /// artist (no tombstone, no row gone, no day's grace changed) and says so once, while the record not had goes as RT01 says and
    /// the pass is noted: only the artists wait.
    #[test]
    fn an_unbuilt_library_forgets_no_artist() {
        let dir = data_dir("unbuilt");
        let s = shores(&dir, 95_100);
        assert!(!s.served.library.built(), "a layout change: not built");
        let (before, was_bare) = (count(&s.cat, "SELECT count(*) FROM artists"), bare(&s.cat));
        let (done, lines) = logged_pass(&s.served, NOW);
        assert!(done.artists.is_empty() && done.artists_wait, "{done:?}");
        assert_eq!(count(&s.cat, "SELECT count(*) FROM artists"), before, "every artist's row kept");
        assert_eq!(count(&s.cat, "SELECT count(*) FROM forgotten WHERE subject LIKE 'ar-%'"), 0, "no artist's tombstone");
        assert_eq!(bare(&s.cat), was_bare, "no day's grace changed");
        assert_eq!(lines.iter().filter(|l| *l == ARTISTS_WAIT).count(), 1, "said once: {lines:#?}");
        assert_eq!(done.records, [s.quiet], "the records not had go as before");
        assert_eq!(s.cat.meta(RAN_AT), Some(NOW.to_string()), "the pass noted");
        drop(s);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// P2.2 (W2): P2.1's library built after its pass: a day on, a track's credit keeps Ferro Lane, Salt Meridian is kept, and
    /// Nobody goes, his day counted from when he was made (the pass that waited gave him none), with Old Lane, whose last credit went
    /// two days before.
    #[test]
    fn after_the_build_the_credits_keep_and_the_rest_go() {
        let dir = data_dir("built-after");
        let s = shores(&dir, 95_200);
        assert!(logged_pass(&s.served, NOW).0.artists_wait);
        build(&s.served);
        assert!(s.served.library.built());
        let (done, lines) = logged_pass(&s.served, NOW + DAY);
        assert!(!done.artists_wait && !lines.iter().any(|l| l == ARTISTS_WAIT), "{lines:#?}");
        assert_eq!(done.artists, [s.nobody, s.old], "those with nothing");
        assert_eq!(s.cat.forgotten_at(&ids::artist(s.nobody)).unwrap(), Some(NOW + DAY), "his tombstone");
        for kept in [s.ferro, s.salt] {
            assert_eq!(count(&s.cat, &format!("SELECT count(*) FROM artists WHERE id = {kept}")), 1, "{kept} kept");
        }
        assert!(s.served.library.artists_credited().unwrap().contains(&s.ferro), "a track's credit");
        drop(s);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Both Shores' folder.
    const BOTH_SHORES: &str = "Salt Meridian/Both Shores (2020)";
    /// The MusicBrainz id Ferro Lane holds by his files (RF16): one forgetting him would delete.
    const FERRO_ID: &str = "00000000-0000-4000-8000-000000000951";

    /// Tide Tables by Glass Harbour, one file in catalogue folder `folder`: an album the library credits beside the guest's.
    fn tide_tables(cat: &Catalogue, folder: i64) {
        let rel = "Glass Harbour/Tide Tables (2012)/01 Ebb.flac";
        cat.execute_for_tests(&format!(
            "INSERT INTO files (folder, rel, size, mtime, state, format, bitrate, duration_ms, came) VALUES ({folder}, '{rel}', 25000000, 1760000000, 1, 'FLAC', 900, 220000, 1760000000)"
        ));
        cat.tag_for_tests(folder, rel, &[("title", "Ebb"), ("artist", "Glass Harbour"), ("tracknumber", "1"), ("album", "Tide Tables"), ("albumartist", "Glass Harbour")]);
        cat.identify().unwrap();
    }

    /// Both Shores' two files in catalogue folder `folder`, tagged as `shores` tags them: its second track featuring Ferro Lane.
    fn both_shores_in(cat: &Catalogue, folder: i64) {
        cat.execute_for_tests(&format!(
            "INSERT INTO files (folder, rel, size, mtime, state, format, bitrate, duration_ms, came) VALUES
               ({folder}, '{}', 24000000, 1760000000, 1, 'FLAC', 900, 200000, 1760000000), ({folder}, '{}', 24001000, 1760000000, 1, 'FLAC', 900, 210000, 1760000000)",
            SHORES[0], SHORES[1]
        ));
        let album = [("album", "Both Shores"), ("albumartist", "Salt Meridian")];
        cat.tag_for_tests(folder, SHORES[0], &[("title", "Low Bank"), ("artist", "Salt Meridian"), ("tracknumber", "1"), album[0], album[1]]);
        cat.tag_for_tests(
            folder,
            SHORES[1],
            &[("title", "Far Bank"), ("artist", "Salt Meridian feat. Ferro Lane"), ("artists", "Salt Meridian"), ("artists", "Ferro Lane"), ("tracknumber", "2"), album[0], album[1]],
        );
        cat.identify().unwrap();
    }

    fn references(cat: &Catalogue, artist: i64) -> usize {
        cat.references_of(&ids::artist(artist)).unwrap().len()
    }

    /// RF16 (the review of run 13, SA1): a guest's album in a trash. Music holds Both Shores (its second track featuring Ferro Lane,
    /// credited nowhere else) and Tide Tables by Glass Harbour; Ferro Lane holds a MusicBrainz id by his files. Ann deletes Both Shores:
    /// its files go into Music's trash, and the next build leaves them out while it still credits Glass Harbour, so the library's credits
    /// are trusted. The day's pass keeps Ferro Lane and Salt Meridian, whom a live track's files in the trash credit (Nobody, with
    /// nothing, still goes). The Undo brings the album back: the same artist ids, credited again, and no reference deleted.
    #[test]
    fn a_guest_of_an_album_in_the_trash_is_kept_for_its_undo() {
        let dir = data_dir("guest-trash");
        let s = shores(&dir, 95_600);
        tide_tables(&s.cat, 0);
        s.cat.set_reference(&ids::artist(s.ferro), &crate::catalogue::records::Ref::new("musicbrainz", "artist", FERRO_ID), "files").unwrap();
        build(&s.served);
        let refs = references(&s.cat, s.ferro);
        assert_eq!(refs, 1);
        let token = crate::trash::token(NOW, BOTH_SHORES);
        assert!(s.cat.mark_trashed(0, BOTH_SHORES, &token).unwrap() > 0, "Both Shores into Music's trash");
        build(&s.served);
        let credited = s.served.library.artists_credited().unwrap();
        assert!(!credited.contains(&s.ferro) && !credited.is_empty(), "the build leaves the trash out, and credits Glass Harbour: {credited:?}");
        let (done, _) = logged_pass(&s.served, NOW);
        assert!(!done.artists_wait && done.artists.contains(&s.nobody), "the pass forgets: {done:?}");
        for (artist, name) in [(s.ferro, "Ferro Lane"), (s.salt, "Salt Meridian")] {
            assert!(!done.artists.contains(&artist), "{name} kept: {done:?}");
            assert!(s.cat.forgotten_at(&ids::artist(artist)).unwrap().is_none(), "{name}: no tombstone");
        }
        assert_eq!(references(&s.cat, s.ferro), refs, "no reference deleted");
        assert!(s.cat.unmark_trashed(0, BOTH_SHORES, &token).unwrap() > 0, "the Undo");
        build(&s.served);
        let credited = s.served.library.artists_credited().unwrap();
        assert!(credited.contains(&s.ferro) && credited.contains(&s.salt), "credited again: {credited:?}");
        assert_eq!((s.cat.artist_of("Ferro Lane").unwrap(), s.cat.artist_of("Salt Meridian").unwrap()), (Some(s.ferro), Some(s.salt)), "the same ids");
        assert_eq!(references(&s.cat, s.ferro), refs, "his reference with him");
        drop(s);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// RF16 (SA1), a parked library (A3: it comes back as itself): Both Shores lies in a second library, taken out of use (out of
    /// Settings, or not there at a start with its mount not up); Music, still built, credits Glass Harbour. The day's pass keeps Ferro
    /// Lane and Salt Meridian, and the library brought back credits them as themselves, Ferro Lane's reference kept.
    #[test]
    fn a_guest_of_a_parked_library_is_kept_for_its_return() {
        let dir = data_dir("guest-parked");
        let (music, second) = (std::path::PathBuf::from("/music"), std::path::PathBuf::from("/second-rf16"));
        let cat = Arc::new(Catalogue::open(None, vec![music.clone(), second.clone()]).unwrap());
        cat.number_above(95_700).unwrap();
        tide_tables(&cat, 0);
        both_shores_in(&cat, 1);
        let [salt, ferro, nobody] = ["Salt Meridian", "Ferro Lane", "Nobody"].map(|n| cat.artist_for(n).unwrap());
        cat.db_for_tests().execute("UPDATE artists SET born = ?1", [BORN]).unwrap();
        cat.set_reference(&ids::artist(ferro), &crate::catalogue::records::Ref::new("musicbrainz", "artist", FERRO_ID), "files").unwrap();
        let served = Serve::open(Some(&dir), crate::catalogue::Rules::default(), &|_| None).unwrap();
        served.attach_catalogue(cat.clone());
        served.library.set_kinds(vec![(0, crate::settings::LibraryKind::Main), (1, crate::settings::LibraryKind::Main)]);
        build(&served);
        assert!(served.library.artists_credited().unwrap().contains(&ferro));
        cat.set_folders(vec![music.clone()]).unwrap();
        build(&served);
        let credited = served.library.artists_credited().unwrap();
        assert!(!credited.contains(&ferro) && !credited.is_empty(), "parked: left out of the build, which credits Glass Harbour: {credited:?}");
        let (done, _) = logged_pass(&served, NOW);
        assert!(!done.artists_wait && done.artists.contains(&nobody), "the pass forgets: {done:?}");
        for (artist, name) in [(ferro, "Ferro Lane"), (salt, "Salt Meridian")] {
            assert!(!done.artists.contains(&artist), "{name} kept: {done:?}");
            assert!(cat.forgotten_at(&ids::artist(artist)).unwrap().is_none(), "{name}: no tombstone");
        }
        assert_eq!(references(&cat, ferro), 1, "no reference deleted");
        cat.set_folders(vec![music, second]).unwrap();
        build(&served);
        let credited = served.library.artists_credited().unwrap();
        assert!(credited.contains(&ferro) && credited.contains(&salt), "back, credited as themselves: {credited:?}");
        assert_eq!((cat.artist_of("Ferro Lane").unwrap(), cat.artist_of("Salt Meridian").unwrap()), (Some(ferro), Some(salt)), "the same ids");
        drop((served, cat));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// P2.3 (W2): a restart with the library built before and no file changed builds nothing, and the pass forgets all the same:
    /// "no build in this start" is not "not built".
    #[test]
    fn a_restart_with_the_library_built_lets_the_pass_forget() {
        let dir = data_dir("restarted");
        let Shores { served, cat, salt, ferro, nobody, .. } = shores(&dir, 95_300);
        build(&served);
        drop(served);
        let served = Serve::open(Some(&dir), crate::catalogue::Rules::default(), &|_| None).unwrap();
        served.attach_catalogue(cat.clone());
        assert_eq!(served.library.change(&cat, &crate::catalogue::FolderRules::default()).unwrap().0, crate::serve::library::Change::None, "nothing to build");
        let (done, _) = logged_pass(&served, NOW);
        assert!(!done.artists_wait);
        assert!(done.artists.contains(&nobody), "Nobody forgotten: {done:?}");
        assert!(!done.artists.contains(&ferro) && !done.artists.contains(&salt), "{done:?}");
        drop((served, cat));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// P2.4 (W2): built, but crediting nobody (a build that read no rows: every library off at that moment) while the catalogue
    /// holds live tracks: no artist forgotten, and the pass says so; a new install, no live track and no credit: forgotten as RT01
    /// says, nothing to keep them.
    #[test]
    fn empty_credits_with_live_tracks_forget_no_artist() {
        let off = served(None, 95_400);
        let cat = off.catalogue().clone();
        cat.record_for_tests("Two Rooms", 2);
        let nobody = cat.artist_for("Nobody").unwrap();
        born_a_month_before(&cat);
        build(&off);
        assert!(off.library.built() && off.library.artists_credited().unwrap().is_empty() && cat.holds_live_tracks().unwrap());
        let (done, lines) = logged_pass(&off, NOW);
        assert!(done.artists.is_empty() && done.artists_wait, "{done:?}");
        assert_eq!(lines.iter().filter(|l| *l == ARTISTS_WAIT).count(), 1, "{lines:#?}");
        assert!(cat.forgotten_at(&ids::artist(nobody)).unwrap().is_none());

        let new = served(None, 95_500);
        let cat = new.catalogue().clone();
        let nobody = cat.artist_for("Nobody").unwrap();
        born_a_month_before(&cat);
        build(&new);
        let (done, lines) = logged_pass(&new, NOW);
        assert_eq!((done.artists, done.artists_wait), (vec![nobody], false), "nothing to keep them");
        assert!(!lines.iter().any(|l| l == ARTISTS_WAIT), "{lines:#?}");
    }

    /// P2.5 (W2, W3): day one: 0.16's data, its had records' files tagged in production's shapes, one track crediting a guest, and
    /// Nobody as 0.16 left them; 0.17's step (its credits part with it); no library.sqlite. The first pass, before any build,
    /// forgets no artist: the album artists are credited on their records (P3), the guest kept by the guard. Built, a pass a day on
    /// forgets those with nothing alone, Nobody among them and the guest not.
    #[test]
    fn day_one_forgets_no_artist_of_the_library() {
        let d = data_dir("day-one");
        crate::catalogue::records::data_0_16(&d).unwrap();
        let cat = Arc::new(Catalogue::open(Some(&d.join("catalogue.sqlite")), vec![std::path::PathBuf::from("/music"), std::path::PathBuf::from(crate::fixtures_base::BOB_OWN)]).unwrap());
        crate::fixtures_base::tag_0_16_shapes(&cat);
        let own = cat.folders().id_of(std::path::Path::new(crate::fixtures_base::BOB_OWN)).unwrap();
        let kinds = vec![(0, crate::settings::LibraryKind::Main), (own, crate::settings::LibraryKind::Own)];
        let [guest, nobody] = ["Ferro Lane", "Nobody"].map(|n| cat.artist_for(n).unwrap());
        born_a_month_before(&cat);
        let ud = UserData::open(Some(&d.join("userdata.sqlite"))).unwrap();
        super::super::step_017::run(&cat, &ud, &kinds, &crate::catalogue::FolderRules::default()).unwrap();
        drop(ud);
        let glass = cat.artist_of("Glass Harbour").unwrap().unwrap();
        assert_eq!(count(&cat, &format!("SELECT count(*) FROM record_artists WHERE record = 1 AND artist = {glass} AND position = 0 AND how = 'files'")), 1, "P3: credited");
        assert!(!d.join("library.sqlite").exists());
        let served = Serve::open(Some(&d), crate::catalogue::Rules::default(), &|_| None).unwrap();
        served.attach_catalogue(cat.clone());
        served.library.set_kinds(kinds);

        let (done, _) = logged_pass(&served, NOW);
        assert!(done.artists.is_empty() && done.artists_wait, "{done:?}");
        assert_eq!(count(&cat, "SELECT count(*) FROM forgotten WHERE subject LIKE 'ar-%'"), 0, "no artist's tombstone on day one");

        build(&served);
        let credited = served.library.artists_credited().unwrap();
        assert!(credited.contains(&guest), "the guest's track credits them");
        let named = served.userdata.artists_named().unwrap();
        let nothing: Vec<i64> = {
            let db = cat.db_for_tests();
            let mut st = db
                .prepare(
                    "SELECT a.id FROM artists a WHERE NOT EXISTS (SELECT 1 FROM record_artists r WHERE r.artist = a.id)
                       AND NOT EXISTS (SELECT 1 FROM \"references\" f WHERE f.subject = 'ar-' || a.id AND (f.how = 'hand' OR f.confirmed = 1))
                       AND a.id NOT IN (SELECT id FROM artist_moved) ORDER BY a.id",
                )
                .unwrap();
            st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).filter(|a| !credited.contains(a) && !named.contains(a)).collect()
        };
        assert!(nothing.contains(&nobody) && !nothing.contains(&guest), "{nothing:?}");
        let (done, _) = logged_pass(&served, NOW + DAY);
        assert_eq!(done.artists, nothing, "those with nothing, and no other");
        let gone: Vec<String> = {
            let db = cat.db_for_tests();
            let mut st = db.prepare("SELECT subject FROM forgotten WHERE subject LIKE 'ar-%' ORDER BY subject").unwrap();
            st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
        };
        let mut expected: Vec<String> = nothing.iter().map(|a| ids::artist(*a)).collect();
        expected.sort();
        assert_eq!(gone, expected);
        drop((served, cat));
        std::fs::remove_dir_all(&d).ok();
    }

    /// `count` library artists (`Artist <n>`), each credited first on a had record of their own.
    fn library_artists(cat: &Catalogue, count: usize) -> Vec<i64> {
        (0..count)
            .map(|n| {
                let (artist, record) = (cat.artist_for(&format!("Artist {n}")).unwrap(), cat.record_for_tests(&format!("Album {n}"), 1));
                cat.db_for_tests().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'files')", rusqlite::params![record, artist]).unwrap();
                artist
            })
            .collect()
    }

    /// A list source of the test's own: who it was asked for, in turn; it knows nobody, or it fails.
    struct Stub {
        source: &'static str,
        fails: bool,
        asked: std::sync::Mutex<Vec<i64>>,
    }

    impl ArtistList for Stub {
        fn source(&self) -> &'static str {
            self.source
        }

        fn read<'a>(&'a self, a: &'a ArtistAsked) -> crate::source::Fut<'a, crate::app::discover::ListRead> {
            self.asked.lock().unwrap().push(a.id);
            let answer: Result<crate::app::discover::ListRead> = if self.fails { Err(anyhow::anyhow!("out of reach")) } else { Ok(None) };
            Box::pin(async move { answer })
        }
    }

    fn stub(source: &'static str, fails: bool) -> Arc<Stub> {
        Arc::new(Stub { source, fails, asked: Default::default() })
    }

    /// A request for `record`, cancelled long ago.
    fn asked_for(record: i64) -> crate::userdata::Request {
        crate::userdata::Request {
            id: 0,
            user: "me".into(),
            album_key: format!("glass harbour|{record}"),
            artist_key: "glass harbour".into(),
            release_group: None,
            album: "Undertow".into(),
            artist: "Glass Harbour".into(),
            year: None,
            cover: None,
            at: 1,
            status: "cancelled".into(),
            reason: None,
            decided_by: None,
            decided_at: Some(2),
            fetched: None,
            record: Some(record),
        }
    }

    /// RT01: a request keeps its record, whatever its status: one cancelled long ago too.
    #[test]
    fn a_requested_record_is_never_forgotten() {
        let (cat, ud) = (Catalogue::open(None, Vec::new()).unwrap(), UserData::open(None).unwrap());
        // Ids of its own: Soulseek's list is one to the process, other tests' items on it.
        cat.number_above(83_000).unwrap();
        let (asked, other) = (unseen(&cat, "Undertow"), unseen(&cat, "Open Water"));
        ud.add_request(&asked_for(asked)).unwrap();
        assert_eq!(cat.forget_quiet(NOW, &protected(&ud, None).unwrap()).unwrap(), [other]);
    }

    /// RT01: a record on Soulseek's list keeps it, as the pass gathers what keeps a record.
    #[test]
    fn a_soulseek_item_keeps_its_record() {
        let (cat, ud) = (Catalogue::open(None, Vec::new()).unwrap(), UserData::open(None).unwrap());
        cat.number_above(81_000).unwrap();
        let (listed, other) = (unseen(&cat, "Undertow"), unseen(&cat, "Open Water"));
        crate::app::soulseek::list_for_tests("ss-rt01a", listed);
        assert_eq!(cat.forget_quiet(NOW, &protected(&ud, None).unwrap()).unwrap(), [other]);
    }

    /// Z5: a record not had that the user data names (a heart, a rating) or a store in the data folder names (a queue) is never
    /// forgotten, as an artist so named is not; one nothing names goes.
    #[test]
    fn a_hearted_record_not_had_is_never_forgotten() {
        let dir = data_dir("records-named");
        let (cat, ud) = (Catalogue::open(None, Vec::new()).unwrap(), UserData::open(None).unwrap());
        cat.number_above(92_000).unwrap();
        let [hearted, rated, queued, other] = ["Undertow", "Open Water", "Two Rooms", "Tide Tables"].map(|t| unseen(&cat, t));
        ud.set_starred("ann", crate::userdata::Kind::Album, &[ids::record(hearted)], true).unwrap();
        ud.set_rating("ann", crate::userdata::Kind::Album, &ids::record(rated), 4).unwrap();
        std::fs::write(dir.join("queues.json"), format!("{{\"ann\":[\"{}/1.1\"]}}", ids::record(queued))).unwrap();
        assert_eq!(cat.forget_quiet(NOW, &protected(&ud, Some(dir.as_path())).unwrap()).unwrap(), [other]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// RT01: the pass runs once a day, by the catalogue's note of when it ran: never run, it runs; an hour later it does not; a
    /// day later it does again.
    #[tokio::test]
    async fn the_pass_runs_once_a_day() {
        let served = served(None, 82_000);
        let cat = served.catalogue().clone();
        let records = |done: Option<Passed>| done.map(|f| f.records);
        let first = unseen(&cat, "Undertow");
        assert_eq!(records(quiet_pass(&served, NOW).await), Some(vec![first]), "never run: due");
        let second = unseen(&cat, "Open Water");
        assert_eq!(records(quiet_pass(&served, NOW + 3600).await), None, "an hour later: not due");
        assert!(cat.record(second).unwrap().is_some());
        assert_eq!(records(quiet_pass(&served, NOW + DAY).await), Some(vec![second]), "a day later");
    }

    /// RT01b: the kept list of a record not had nothing keeps goes thirty days after it was read where nobody opened the record
    /// since; one opened since stays, one read since stays, and a requested record's stays (Soulseek judges by it).
    #[test]
    fn a_kept_list_unopened_for_30_days_goes() {
        let (cat, ud) = (Catalogue::open(None, Vec::new()).unwrap(), UserData::open(None).unwrap());
        cat.number_above(84_000).unwrap();
        let [quiet, opened, fresh, asked] = ["Undertow", "Open Water", "Two Rooms", "Tide Tables"].map(|t| cat.record_for_tests(t, 0));
        let list = [crate::catalogue::identity::RefTrack { disc: 1, position: 1, title: "Neap".into(), length_ms: Some(180_000), recording: None }];
        for record in [quiet, opened, fresh, asked] {
            cat.set_reference_tracks(record, "musicbrainz", &list).unwrap();
        }
        {
            let db = cat.db_for_tests();
            db.execute("UPDATE reference_tracks SET at = ?1", [NOW - 45 * DAY]).unwrap();
            db.execute("UPDATE reference_tracks SET at = ?2 WHERE record = ?1", rusqlite::params![fresh, NOW - 10 * DAY]).unwrap();
            db.execute("UPDATE records SET opened_at = ?2 WHERE id = ?1", rusqlite::params![opened, NOW - 10 * DAY]).unwrap();
        }
        ud.add_request(&asked_for(asked)).unwrap();
        assert_eq!(cat.forget_quiet_lists(NOW, &protected(&ud, None).unwrap()).unwrap(), 1);
        assert!(cat.reference_tracks(quiet).unwrap().is_empty(), "unopened thirty days: let go of");
        assert!(cat.record(quiet).unwrap().is_some(), "the record stays");
        for kept in [opened, fresh, asked] {
            assert_eq!(cat.reference_tracks(kept).unwrap().len(), 1, "{kept} keeps its list");
        }
    }

    /// RT01b: an artist whose only record was forgotten goes a day later (Z10: their day counts from when their last credit went,
    /// not from their birth), their tombstone written, and their id answers "no longer known"; one a track credits stays, and so
    /// does one a kept record credits. Named again, they are a new artist. The library built first (W2).
    #[tokio::test]
    async fn an_artist_left_with_nothing_is_forgotten() {
        let served = served(None, 85_000);
        build(&served);
        let cat = served.catalogue().clone();
        let [lonely, featured, listed] = ["Orbit Lane", "Tape Club", "Glass Harbour"].map(|n| cat.artist_for(n).unwrap());
        born_a_month_before(&cat);
        let listing = |title: &str| crate::catalogue::records::Listing { title: title.into(), kind: Some("album".into()), ..Default::default() };
        let gone = cat.keep_listing(Some(lonely), &[listing("Undertow")], "link").unwrap()[0];
        let kept = cat.keep_listing(Some(listed), &[listing("Two Rooms")], "link").unwrap()[0];
        cat.db_for_tests().execute("UPDATE records SET seen_at = ?2 WHERE id = ?1", rusqlite::params![gone, NOW - 7 * MONTH]).unwrap();
        cat.db_for_tests().execute("UPDATE records SET seen_at = ?2 WHERE id = ?1", rusqlite::params![kept, NOW - MONTH]).unwrap();
        // Tape Club, credited on a track of the library and on no record.
        served.library.db().execute("INSERT INTO song_artists (song_id, artist_id, name, position) VALUES ('tr-1', ?1, 'Tape Club', 0)", [ids::artist(featured)]).unwrap();
        let done = quiet_pass(&served, NOW).await.expect("due");
        assert_eq!((done.records, done.artists), (vec![gone], vec![]), "their last credit went now: kept the day");
        let done = quiet_pass(&served, NOW + DAY).await.expect("due");
        assert_eq!(done.artists, [lonely], "a day on, still with nothing");
        assert_eq!(cat.forgotten_at(&ids::artist(lonely)).unwrap(), Some(NOW + DAY), "their tombstone");
        let answer = unknown_artist(&cat, &ids::artist(lonely)).expect("no longer known");
        assert_eq!(answer.to_string(), "This artist is no longer known.");
        assert!(unknown_artist(&cat, &ids::artist(featured)).is_none(), "known: answered as before");
        assert!(cat.artist_for("Orbit Lane").unwrap() > lonely, "named again: a new id, never theirs");
    }

    /// RT01b: an artist someone hearted is never forgotten, though nothing credits them.
    #[test]
    fn a_favourite_artist_is_never_forgotten() {
        let served = served(None, 86_000);
        let cat = served.catalogue().clone();
        let [hearted, other] = ["Kite Theory", "Noor Field"].map(|n| cat.artist_for(n).unwrap());
        born_a_month_before(&cat);
        served.userdata.set_starred("ann", crate::userdata::Kind::Artist, &[ids::artist(hearted)], true).unwrap();
        assert_eq!(cat.forget_quiet_artists(NOW, &protected_artists(&served, served.library.artists_credited().unwrap()).unwrap()).unwrap(), [other]);
    }

    /// RT01b: an artist a crate is seeded by (Sounds like them), in any account's crates, is never forgotten.
    #[tokio::test]
    async fn a_crate_seed_keeps_its_artist() {
        let dir = data_dir("crate-seed");
        let served = served(Some(&dir), 87_000);
        let cat = served.catalogue().clone();
        let [seeded, other] = ["Kite Theory", "Noor Field"].map(|n| cat.artist_for(n).unwrap());
        born_a_month_before(&cat);
        let like: crate::crates::Condition =
            serde_json::from_value(serde_json::json!({ "axis": "like", "value": { "kind": "like", "of": [ids::artist(seeded)], "count": 20 } })).unwrap();
        crate::crates::CrateStore::of_account(dir.to_str(), "ann").save(None, "Like Kite Theory", vec![like]).await.unwrap();
        assert_eq!(cat.forget_quiet_artists(NOW, &protected_artists(&served, served.library.artists_credited().unwrap()).unwrap()).unwrap(), [other]);
        drop((cat, served));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// RT01c (F3): with sixty library artists all due, a day's pass reads two of them, a thirtieth, the oldest first (never read
    /// before read longest ago), at each source due; the next day's pass reads the next two.
    #[tokio::test]
    async fn the_reread_spreads_over_the_month() {
        let served = served(None, 88_000);
        let cat = served.catalogue().clone();
        let artists = library_artists(&cat, 60);
        // Every list read 31 days ago, but the 41st artist's never and the 11th's 45 days ago.
        {
            let db = cat.db_for_tests();
            for (n, artist) in artists.iter().enumerate().filter(|(n, _)| *n != 40) {
                let read_at = if n == 10 { NOW - 45 * DAY } else { NOW - 31 * DAY };
                for source in ["musicbrainz", "discogs"] {
                    db.execute(
                        "INSERT INTO \"references\" (subject, source, kind, value, at, how, read_at) VALUES (?1, ?2, 'artist', ?3, 1, 'files', ?4)",
                        rusqlite::params![ids::artist(*artist), source, format!("{source}-{artist}"), read_at],
                    )
                    .unwrap();
                }
            }
        }
        let (musicbrainz, discogs) = (stub("musicbrainz", false), stub("discogs", false));
        let sources: Vec<Arc<dyn ArtistList>> = vec![musicbrainz.clone(), discogs.clone()];
        let done = pass_reading(served.clone(), NOW, &sources, || false).await.unwrap().expect("due");
        let first = vec![artists[40], artists[10]];
        assert_eq!(*musicbrainz.asked.lock().unwrap(), first, "a thirtieth, the oldest first");
        assert_eq!(*discogs.asked.lock().unwrap(), first);
        assert_eq!(done.reread.read.len(), 4);
        pass_reading(served.clone(), NOW + DAY, &sources, || false).await.unwrap().expect("due");
        assert_eq!(*musicbrainz.asked.lock().unwrap(), [artists[40], artists[10], artists[0], artists[1]], "the next day, the next two");
    }

    /// RT01c: a list the re-read cannot read (its source fails) keeps the artist's records not had seen at the pass's now, so the
    /// same pass's forgetting keeps them; the artist is come to again a month later, not the next day.
    #[tokio::test]
    async fn a_failed_reread_keeps_records_seen() {
        let served = served(None, 89_000);
        let cat = served.catalogue().clone();
        let artist = library_artists(&cat, 1)[0];
        let listing = crate::catalogue::records::Listing { title: "Undertow".into(), kind: Some("album".into()), ..Default::default() };
        let quiet = cat.keep_listing(Some(artist), &[listing], "link").unwrap()[0];
        cat.db_for_tests().execute("UPDATE records SET seen_at = ?2 WHERE id = ?1", rusqlite::params![quiet, NOW - 7 * MONTH]).unwrap();
        let (musicbrainz, discogs) = (stub("musicbrainz", true), stub("discogs", true));
        let sources: Vec<Arc<dyn ArtistList>> = vec![musicbrainz.clone(), discogs.clone()];
        let done = pass_reading(served.clone(), NOW, &sources, || false).await.unwrap().expect("due");
        assert_eq!(done.reread.kept_seen, [(artist, "musicbrainz"), (artist, "discogs")]);
        assert!(done.records.is_empty(), "not forgotten by the same pass");
        let seen: Option<i64> = cat.db_for_tests().query_row("SELECT seen_at FROM records WHERE id = ?1", [quiet], |r| r.get(0)).unwrap();
        assert_eq!(seen, Some(NOW), "seen at the pass's now");
        pass_reading(served.clone(), NOW + DAY, &sources, || false).await.unwrap().expect("due");
        assert_eq!(musicbrainz.asked.lock().unwrap().len(), 1, "come to again a month later, not the next day");
    }

    /// Z4: a library artist neither MusicBrainz nor Discogs knows (one only Bandcamp knows, whose list the re-read never reads):
    /// both read, their record only Bandcamp knows is seen at the pass's now and stays; their record Discogs names, which no list
    /// showed, goes.
    #[tokio::test]
    async fn a_library_artists_bandcamp_only_records_stay() {
        use crate::catalogue::records::{Listing, Ref};
        let served = served(None, 91_000);
        let cat = served.catalogue().clone();
        let artist = library_artists(&cat, 1)[0];
        let listing = |title: &str, r: Ref| Listing { title: title.into(), kind: Some("album".into()), refs: vec![r], ..Default::default() };
        let kept = cat.keep_listing(Some(artist), &[listing("Night Walks", Ref::new("bandcamp", "album", "https://kitetheory.bandcamp.com/album/night-walks"))], "link").unwrap()[0];
        let gone = cat.keep_listing(Some(artist), &[listing("Paper Kites", Ref::new("discogs", "master", "3001"))], "link").unwrap()[0];
        cat.db_for_tests().execute("UPDATE records SET seen_at = ?1 WHERE id IN (?2, ?3)", rusqlite::params![NOW - 7 * MONTH, kept, gone]).unwrap();
        let sources: Vec<Arc<dyn ArtistList>> = vec![stub("musicbrainz", false), stub("discogs", false)];
        let done = pass_reading(served.clone(), NOW, &sources, || false).await.unwrap().expect("due");
        assert_eq!(done.reread.read, [(artist, "musicbrainz"), (artist, "discogs")], "both read: neither knows them");
        assert_eq!(done.records, [gone], "the record only Bandcamp knows stays");
        let seen: Option<i64> = cat.db_for_tests().query_row("SELECT seen_at FROM records WHERE id = ?1", [kept], |r| r.get(0)).unwrap();
        assert_eq!(seen, Some(NOW), "seen at the pass's now");
    }

    /// Z9: a pass more than three days after the last (an outage, a clock that jumped forward) re-reads as ever, but forgets no
    /// record until a whole month of re-reads has passed, the daily passes meanwhile too; then it forgets, and a pass three days
    /// after the last is not late.
    #[tokio::test]
    async fn a_late_pass_rereads_but_waits_a_month_to_forget() {
        let served = served(None, 93_000);
        let cat = served.catalogue().clone();
        let artists = library_artists(&cat, 2);
        let musicbrainz = stub("musicbrainz", false);
        let sources: Vec<Arc<dyn ArtistList>> = vec![musicbrainz.clone(), stub("discogs", false)];
        assert!(pass_reading(served.clone(), NOW, &sources, || false).await.unwrap().is_some_and(|p| !p.waits), "the first pass");
        let quiet = unseen(&cat, "Undertow");
        let late = NOW + 10 * DAY;
        let done = pass_reading(served.clone(), late, &sources, || false).await.unwrap().expect("due");
        assert_eq!((done.waits, done.records.len()), (true, 0), "ten days on: it waits");
        assert_eq!(*musicbrainz.asked.lock().unwrap(), artists, "and re-reads as ever: the next artist");
        for day in 1..30 {
            let done = pass_reading(served.clone(), late + day * DAY, &sources, || false).await.unwrap().expect("due");
            assert!(done.waits && done.records.is_empty(), "day {day}: still waits");
        }
        assert!(cat.record(quiet).unwrap().is_some());
        let done = pass_reading(served.clone(), late + 30 * DAY, &sources, || false).await.unwrap().expect("due");
        assert_eq!((done.waits, done.records), (false, vec![quiet]), "a month of re-reads on: forgotten");
        let next = unseen(&cat, "Open Water");
        let done = pass_reading(served.clone(), late + 33 * DAY, &sources, || false).await.unwrap().expect("due");
        assert_eq!((done.waits, done.records), (false, vec![next]), "three days after the last: not late");
    }

    /// U9 (the review of 8c/8d, finding 9): a pass noted seven months ahead, and a wait noted from that clock (it ran ahead, and was
    /// put right): the pass runs, the wait is cut to a month from now, and a month of daily passes on it forgets again.
    #[tokio::test]
    async fn a_pass_noted_in_the_future_runs_and_waits_a_month() {
        let served = served(None, 94_000);
        let cat = served.catalogue().clone();
        library_artists(&cat, 2);
        let sources: Vec<Arc<dyn ArtistList>> = vec![stub("musicbrainz", false), stub("discogs", false)];
        cat.set_meta(RAN_AT, &(NOW + 7 * MONTH).to_string());
        cat.set_meta(WAITS_UNTIL, &(NOW + 8 * MONTH).to_string());
        let quiet = unseen(&cat, "Undertow");
        let done = pass_reading(served.clone(), NOW, &sources, || false).await.unwrap().expect("due: noted in the future");
        assert!(done.waits && done.records.is_empty(), "it waits");
        assert_eq!(cat.meta(WAITS_UNTIL), Some((NOW + REREAD_MONTH).to_string()), "a month from now, not from the clock that ran ahead");
        for day in 1..30 {
            let done = pass_reading(served.clone(), NOW + day * DAY, &sources, || false).await.unwrap().expect("due");
            assert!(done.waits && done.records.is_empty(), "day {day}: still waits");
        }
        let done = pass_reading(served.clone(), NOW + 30 * DAY, &sources, || false).await.unwrap().expect("due");
        assert_eq!((done.waits, done.records), (false, vec![quiet]), "a month on: forgotten");
    }

    /// RT01c: with the background paused, the re-read reads nothing, and keeps the artist's records not had seen instead.
    #[tokio::test]
    async fn a_paused_reread_reads_nothing_and_keeps_records_seen() {
        let served = served(None, 90_000);
        let cat = served.catalogue().clone();
        let artist = library_artists(&cat, 1)[0];
        let listing = crate::catalogue::records::Listing { title: "Undertow".into(), kind: Some("album".into()), ..Default::default() };
        let quiet = cat.keep_listing(Some(artist), &[listing], "link").unwrap()[0];
        cat.db_for_tests().execute("UPDATE records SET seen_at = ?2 WHERE id = ?1", rusqlite::params![quiet, NOW - 7 * MONTH]).unwrap();
        let musicbrainz = stub("musicbrainz", false);
        let sources: Vec<Arc<dyn ArtistList>> = vec![musicbrainz.clone()];
        let done = pass_reading(served.clone(), NOW, &sources, || true).await.unwrap().expect("due");
        assert!(musicbrainz.asked.lock().unwrap().is_empty(), "paused: nothing asked");
        assert_eq!(done.reread.kept_seen, [(artist, "musicbrainz"), (artist, "discogs")], "Discogs' not given: kept seen too");
        assert!(done.records.is_empty(), "kept seen, not forgotten");
    }
}
