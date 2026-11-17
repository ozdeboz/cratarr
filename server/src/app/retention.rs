//! RT01 (the owner's decision, "Retention of records not had"): records not had (from discographies, lists and requests, with
//! no tracks) pile up three to one against records had. One that no list showed and nobody opened for six months is forgotten,
//! with its references and credits; a tombstone keeps its id, so an old link answers "no longer known", and no id is given
//! again. In the same daily pass (RT01b), the kept lists of a record not had go after thirty days unopened, read again when
//! asked for, and an artist left with nothing is forgotten as a record is.
//!
//! - **Never while something points at it.** The catalogue keeps what it holds itself (`Catalogue::forget_quiet`: a choice or
//!   a reference by hand, a confirmation, a merge, an expected folder, a retired record, a link from before 0.16, a request's
//!   shared name); this gathers the rest: every request's record, whatever its status, and every record on Soulseek's list
//!   (`protected`). An artist is kept by a record's credit, a reference by hand or confirmed, an id from before 0.16
//!   (`Catalogue::forget_quiet_artists`), and by a track's credit, anything the user data names them in (a favourite, plays, a
//!   rating, words) and the stores in the data folder (every account's crates, their seeds and conditions; the queues):
//!   `protected_artists`.
//! - **Seen** is a list read: MusicBrainz's or Discogs' sees what it lists (`Catalogue::keep_listing`) and the artist's records
//!   only Bandcamp knows (`Catalogue::bandcamp_only_seen`); Bandcamp's, no longer read whole (O4), sees nothing. A page showing
//!   a record sees nothing; opening it does (C54).
//! - **Once a day:** looked at hourly, run where the catalogue's note says the last pass is a day old or more (`pass`), so a
//!   restart does not run it again.

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Result;

use super::ToggleError;
use crate::catalogue::Catalogue;
use crate::serve::Serve;
use crate::userdata::UserData;

/// When the pass last ran, in the catalogue's notes (`meta`): at most once a day, across restarts.
const RAN_AT: &str = "retention_at";
const DAY: i64 = 86_400;

/// What an album asked for by its id answers once its record was forgotten (RT01).
pub const NO_LONGER_KNOWN: &str = "This album is no longer known: nobody had opened it or seen it in a list for six months.";
/// What an artist asked for by their id answers once forgotten (RT01b).
pub const ARTIST_NO_LONGER_KNOWN: &str = "This artist is no longer known.";

/// Whether the pass is due at `now`: never run, or last run a day ago or more.
fn due(cat: &Catalogue, now: i64) -> bool {
    cat.meta(RAN_AT).and_then(|v| v.parse::<i64>().ok()).is_none_or(|at| now - at >= DAY)
}

/// What keeps a record not had from outside the catalogue, however long nobody saw it: every request's record, whatever its
/// status (`UserData::request_records`), and every record on Soulseek's list. Blocking.
fn protected(ud: &UserData) -> Result<BTreeSet<i64>> {
    let mut kept: BTreeSet<i64> = ud.request_records()?.into_iter().filter_map(|(_, record)| record).collect();
    kept.extend(super::soulseek::item_records());
    Ok(kept)
}

/// What keeps an artist with no record from outside the catalogue (RT01b): a track's credit (Cratarr's own library holds it),
/// anything the user data names them in (`UserData::artists_named`), and the stores in the data folder (`library::id_stores`:
/// every account's crates, their seeds and conditions; the queues). A store not made yet names nobody; one that cannot be
/// read may name anyone, so nobody is forgotten this time. Blocking.
fn protected_artists(served: &Serve) -> Result<BTreeSet<i64>> {
    let mut kept = served.userdata.artists_named()?;
    kept.extend(served.library.artists_credited()?);
    for store in served.data_dir.as_deref().map(super::library::id_stores).unwrap_or_default() {
        match std::fs::read_to_string(&store) {
            Ok(text) => kept.extend(crate::serve::ids::artists_named(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(anyhow::Error::from(e).context(format!("{} cannot be read", store.display()))),
        }
    }
    Ok(kept)
}

/// What a pass let go of: the records not had (RT01), their kept lists and the artists left with nothing (RT01b).
#[derive(Debug, Default, PartialEq)]
pub struct Forgotten {
    pub records: Vec<i64>,
    pub lists: usize,
    pub artists: Vec<i64>,
}

/// The forgetting at `now`, where due (`due`): what keeps a record and an artist gathered first, then the records nothing keeps
/// forgotten, the kept lists let go of, the artists left with nothing forgotten, and the pass's time written. What went; None:
/// not due.
pub async fn pass(served: Arc<Serve>, now: i64) -> Result<Option<Forgotten>> {
    let done = tokio::task::spawn_blocking(move || -> Result<Option<Forgotten>> {
        let cat = served.catalogue();
        if !due(cat, now) {
            return Ok(None);
        }
        let (kept, artists_kept) = (protected(&served.userdata)?, protected_artists(&served)?);
        let records = cat.forget_quiet(now, &kept)?;
        let lists = cat.forget_quiet_lists(now, &kept)?;
        let artists = cat.forget_quiet_artists(now, &artists_kept)?;
        if !artists.is_empty() {
            // The library's names without theirs: a name credited again is given a new id, never one forgotten.
            served.library.set_artists(cat.artist_ids()?);
        }
        cat.set_meta(RAN_AT, &now.to_string());
        Ok(Some(Forgotten { records, lists, artists }))
    })
    .await??;
    if let Some(f) = &done {
        if !f.records.is_empty() {
            tracing::info!(target: "library", count = f.records.len(), "records not had that no list showed and nobody opened for six months forgotten");
        }
        if f.lists > 0 {
            tracing::info!(target: "library", count = f.lists, "kept lists of records not had, unopened for thirty days, let go of");
        }
        if !f.artists.is_empty() {
            tracing::info!(target: "library", count = f.artists.len(), "artists left with nothing forgotten");
        }
    }
    Ok(done)
}

/// The pass looked at hourly, from ten minutes after the start (the start's own work first), run where due. Started once
/// Soulseek's list is read (`soulseek::start`), whose items keep their records, and the catalogue attached.
pub fn start(served: Arc<Serve>) {
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
        assert_eq!(cat.forget_quiet(NOW, &protected(&ud).unwrap()).unwrap(), [other]);
    }

    /// RT01: a record on Soulseek's list keeps it, as the pass gathers what keeps a record.
    #[test]
    fn a_soulseek_item_keeps_its_record() {
        let (cat, ud) = (Catalogue::open(None, Vec::new()).unwrap(), UserData::open(None).unwrap());
        cat.number_above(81_000).unwrap();
        let (listed, other) = (unseen(&cat, "Undertow"), unseen(&cat, "Open Water"));
        crate::app::soulseek::list_for_tests("ss-rt01a", listed);
        assert_eq!(cat.forget_quiet(NOW, &protected(&ud).unwrap()).unwrap(), [other]);
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
        let records = |done: Option<Forgotten>| done.map(|f| f.records);
        let first = unseen(&cat, "Undertow");
        assert_eq!(records(pass(served.clone(), NOW).await.unwrap()), Some(vec![first]), "never run: due");
        let second = unseen(&cat, "Open Water");
        assert_eq!(records(pass(served.clone(), NOW + 3600).await.unwrap()), None, "an hour later: not due");
        assert!(cat.record(second).unwrap().is_some());
        assert_eq!(records(pass(served.clone(), NOW + DAY).await.unwrap()), Some(vec![second]), "a day later");
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
        assert_eq!(cat.forget_quiet_lists(NOW, &protected(&ud).unwrap()).unwrap(), 1);
        assert!(cat.reference_tracks(quiet).unwrap().is_empty(), "unopened thirty days: let go of");
        assert!(cat.record(quiet).unwrap().is_some(), "the record stays");
        for kept in [opened, fresh, asked] {
            assert_eq!(cat.reference_tracks(kept).unwrap().len(), 1, "{kept} keeps its list");
        }
    }

    /// RT01b: an artist whose only record was forgotten goes in the same pass, their tombstone written, and their id answers
    /// "no longer known"; one a track credits stays, and so does one a kept record credits. Named again, they are a new artist.
    #[tokio::test]
    async fn an_artist_left_with_nothing_is_forgotten() {
        let served = served(None, 85_000);
        let cat = served.catalogue().clone();
        let [lonely, featured, listed] = ["Orbit Lane", "Tape Club", "Glass Harbour"].map(|n| cat.artist_for(n).unwrap());
        let listing = |title: &str| crate::catalogue::records::Listing { title: title.into(), kind: Some("album".into()), ..Default::default() };
        let gone = cat.keep_listing(Some(lonely), &[listing("Undertow")], "link").unwrap()[0];
        let kept = cat.keep_listing(Some(listed), &[listing("Two Rooms")], "link").unwrap()[0];
        cat.db_for_tests().execute("UPDATE records SET seen_at = ?2 WHERE id = ?1", rusqlite::params![gone, NOW - 7 * MONTH]).unwrap();
        cat.db_for_tests().execute("UPDATE records SET seen_at = ?2 WHERE id = ?1", rusqlite::params![kept, NOW - MONTH]).unwrap();
        // Tape Club, credited on a track of the library and on no record.
        served.library.db().execute("INSERT INTO song_artists (song_id, artist_id, name, position) VALUES ('tr-1', ?1, 'Tape Club', 0)", [ids::artist(featured)]).unwrap();
        let done = pass(served.clone(), NOW).await.unwrap().expect("due");
        assert_eq!((done.records, done.artists), (vec![gone], vec![lonely]));
        assert_eq!(cat.forgotten_at(&ids::artist(lonely)).unwrap(), Some(NOW), "their tombstone");
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
        let like: crate::crates::Condition =
            serde_json::from_value(serde_json::json!({ "axis": "like", "value": { "kind": "like", "of": [ids::artist(seeded)], "count": 20 } })).unwrap();
        crate::crates::CrateStore::of_account(dir.to_str(), "ann").save(None, "Like Kite Theory", vec![like]).await.unwrap();
        assert_eq!(cat.forget_quiet_artists(NOW, &protected_artists(&served).unwrap()).unwrap(), [other]);
        drop((cat, served));
        std::fs::remove_dir_all(&dir).ok();
    }
}
