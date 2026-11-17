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
//!   folder (every account's crates, their seeds and conditions; the queues): `protected_artists`.
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

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Result;

use super::ToggleError;
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

/// What an album asked for by its id answers once its record was forgotten (RT01).
pub const NO_LONGER_KNOWN: &str = "This album is no longer known: nobody had opened it or seen it in a list for six months.";
/// What an artist asked for by their id answers once forgotten (RT01b).
pub const ARTIST_NO_LONGER_KNOWN: &str = "This artist is no longer known.";

/// Whether the pass is due at `now`: never run, or last run a day ago or more.
fn due(cat: &Catalogue, now: i64) -> bool {
    cat.meta(RAN_AT).and_then(|v| v.parse::<i64>().ok()).is_none_or(|at| now - at >= DAY)
}

/// Whether the forgetting of records waits at `now` (Z9): a pass more than `LATE` after the last makes it wait a whole month of
/// re-reads from now (`REREAD_MONTH`), as the re-read has fallen behind and what it has not come to again would look unseen; a
/// wait already there is lengthened, never cut short. Noted in the catalogue (`WAITS_UNTIL`). Before the pass is noted.
/// Blocking.
fn forgetting_waits(cat: &Catalogue, now: i64) -> bool {
    let noted = |key: &str| cat.meta(key).and_then(|v| v.parse::<i64>().ok());
    let mut until = noted(WAITS_UNTIL).unwrap_or(0);
    if noted(RAN_AT).is_some_and(|at| now - at > LATE) {
        until = until.max(now + REREAD_MONTH);
        cat.set_meta(WAITS_UNTIL, &until.to_string());
    }
    now < until
}

/// What keeps a record not had from outside the catalogue, however long nobody saw it: every request's record, whatever its
/// status (`UserData::request_records`), every record on Soulseek's list, and, as an artist is kept (`protected_artists`, Z5),
/// anything the user data names it in (`UserData::records_named`: a heart, plays, a rating, words, tags) and the stores in the
/// data folder `data` (`stores_name`). Blocking.
fn protected(ud: &UserData, data: Option<&std::path::Path>) -> Result<BTreeSet<i64>> {
    let mut kept: BTreeSet<i64> = ud.request_records()?.into_iter().filter_map(|(_, record)| record).collect();
    kept.extend(super::soulseek::item_records());
    kept.extend(ud.records_named()?);
    kept.extend(stores_name(data, |text| crate::serve::ids::records_named(text).collect())?);
    Ok(kept)
}

/// What keeps an artist with no record from outside the catalogue (RT01b): a track's credit (Cratarr's own library holds it),
/// anything the user data names them in (`UserData::artists_named`), and the stores in the data folder (`stores_name`). Blocking.
fn protected_artists(served: &Serve) -> Result<BTreeSet<i64>> {
    let mut kept = served.userdata.artists_named()?;
    kept.extend(served.library.artists_credited()?);
    kept.extend(stores_name(served.data_dir.as_deref(), |text| crate::serve::ids::artists_named(text).collect())?);
    Ok(kept)
}

/// What the stores in the data folder `data` name (`library::id_stores`: every account's crates, their seeds and conditions; the
/// queues), each read by `read`. A store not made yet names nothing; one that cannot be read may name anything, so nothing is
/// forgotten this time. Blocking.
fn stores_name(data: Option<&std::path::Path>, read: fn(&str) -> Vec<i64>) -> Result<BTreeSet<i64>> {
    let mut named = BTreeSet::new();
    for store in data.map(super::library::id_stores).unwrap_or_default() {
        match std::fs::read_to_string(&store) {
            Ok(text) => named.extend(read(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(anyhow::Error::from(e).context(format!("{} cannot be read", store.display()))),
        }
    }
    Ok(named)
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
/// (C56).
#[derive(Debug, Default, PartialEq)]
pub struct Passed {
    pub reread: Reread,
    pub waits: bool,
    pub records: Vec<i64>,
    pub lists: usize,
    pub artists: Vec<i64>,
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
    let batch = tokio::task::spawn_blocking(move || c.rereads_due(now)).await??;
    let mut done = Reread::default();
    for (artist, due_at) in batch {
        let c = cat.clone();
        let Some((name, refs)) = tokio::task::spawn_blocking(move || c.artist_known(artist)).await?? else { continue };
        let asked = ArtistAsked { id: artist, name, refs: refs.into_iter().map(|(r, _)| r).collect() };
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
            tokio::task::spawn_blocking(move || -> Result<()> {
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
            tokio::task::spawn_blocking(move || c.bandcamp_only_seen(artist, now)).await??;
        }
    }
    Ok(done)
}

/// The day's pass at `now`, where due (`due`): the library artists' lists read again first (`reread`, RT01c), so what it reads
/// counts as seen; then what keeps a record and an artist gathered, the records nothing keeps forgotten (but while the forgetting
/// waits after a long gap, `forgetting_waits`, Z9), the kept lists let go of, the artists left with nothing forgotten, the
/// labels' rows read thirty days ago let go of (C56), and the pass's time written. What it came to; None: not due.
pub async fn pass(served: Arc<Serve>, now: i64) -> Result<Option<Passed>> {
    pass_reading(served, now, &super::discover::reread_sources(), super::switches::background_paused).await
}

/// `pass`, reading by `sources` and pausing by `paused`: the server's own, or a test's.
async fn pass_reading(served: Arc<Serve>, now: i64, sources: &[Arc<dyn ArtistList>], paused: fn() -> bool) -> Result<Option<Passed>> {
    let cat = served.catalogue().clone();
    let c = cat.clone();
    if !tokio::task::spawn_blocking(move || due(&c, now)).await? {
        return Ok(None);
    }
    let c = cat.clone();
    let waits = tokio::task::spawn_blocking(move || forgetting_waits(&c, now)).await?;
    let reread = reread(&cat, sources, paused, now).await?;
    let done = tokio::task::spawn_blocking(move || -> Result<Passed> {
        let cat = served.catalogue();
        let (kept, artists_kept) = (protected(&served.userdata, served.data_dir.as_deref())?, protected_artists(&served)?);
        let records = if waits { Vec::new() } else { cat.forget_quiet(now, &kept)? };
        let lists = cat.forget_quiet_lists(now, &kept)?;
        let artists = cat.forget_quiet_artists(now, &artists_kept)?;
        if !artists.is_empty() {
            // The library's names without theirs: a name credited again is given a new id, never one forgotten.
            served.library.set_artists(cat.artist_ids()?);
        }
        let label_rows = cat.forget_label_rows(now)?;
        cat.set_meta(RAN_AT, &now.to_string());
        Ok(Passed { reread, waits, records, lists, artists, label_rows })
    })
    .await??;
    if done.waits {
        tracing::info!(target: "library", "records not had: none forgotten until a month of re-reads has passed, the last pass long ago");
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
    tokio::spawn(async move {
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

/// Whether `subject` was forgotten (RT01): its tombstone. One that cannot be read is not, and is answered as before.
fn forgotten(cat: &Catalogue, subject: &str) -> bool {
    match cat.forgotten_at(subject) {
        Ok(at) => at.is_some(),
        Err(e) => {
            tracing::debug!(target: "library", subject, "whether it was forgotten could not be read: {e:#}");
            false
        }
    }
}

/// What Cratarr's album `id` answers where it cannot be read: a record forgotten is no longer known (410), by its tombstone;
/// any other, None, answered as before. Blocking.
pub fn unknown_album(cat: &Catalogue, id: &str) -> Option<ToggleError> {
    forgotten(cat, id).then(|| ToggleError::Gone(NO_LONGER_KNOWN.into()))
}

/// What Cratarr's artist `id` (`ar-<n>`) answers where nothing is known of them: one forgotten is no longer known (410), by
/// their tombstone (RT01b); any other, None, answered as before. Blocking.
pub fn unknown_artist(cat: &Catalogue, id: &str) -> Option<ToggleError> {
    forgotten(cat, id).then(|| ToggleError::Gone(ARTIST_NO_LONGER_KNOWN.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::ids;

    /// RT01's clock in the tests: now, and a month.
    const NOW: i64 = 1_800_000_000;
    const MONTH: i64 = 30 * 86_400;

    /// A record not had of `title`, last seen seven months before `NOW`.
    fn unseen(cat: &Catalogue, title: &str) -> i64 {
        let record = cat.record_for_tests(title, 0);
        cat.db_for_tests().execute("UPDATE records SET seen_at = ?2 WHERE id = ?1", rusqlite::params![record, NOW - 7 * MONTH]).unwrap();
        record
    }

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

    /// RT01: Cratarr's album forgotten answers "no longer known" (410), by its tombstone; one never known, as before.
    #[test]
    fn a_forgotten_record_answers_no_longer_known() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let gone = unseen(&cat, "Undertow");
        assert_eq!(cat.forget_quiet(NOW, &BTreeSet::new()).unwrap(), [gone]);
        let answer = unknown_album(&cat, &ids::record(gone)).expect("no longer known");
        assert_eq!(answer.to_string(), "This album is no longer known: nobody had opened it or seen it in a list for six months.");
        let status = axum::response::IntoResponse::into_response(crate::api::ApiError::from(answer)).status();
        assert_eq!(status, axum::http::StatusCode::GONE);
        assert!(unknown_album(&cat, &ids::record(gone + 1)).is_none(), "never known: answered as before");
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
    /// does one a kept record credits. Named again, they are a new artist.
    #[tokio::test]
    async fn an_artist_left_with_nothing_is_forgotten() {
        let served = served(None, 85_000);
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
        assert_eq!(cat.forget_quiet_artists(NOW, &protected_artists(&served).unwrap()).unwrap(), [other]);
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
        assert_eq!(cat.forget_quiet_artists(NOW, &protected_artists(&served).unwrap()).unwrap(), [other]);
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
