//! RT01 (the owner's decision, "Retention of records not had"): records not had (from discographies, lists and requests, with
//! no tracks) pile up three to one against records had. One that no list showed and nobody opened for six months is forgotten,
//! with its references and credits; a tombstone keeps its id, so an old link answers "no longer known", and no id is given
//! again.
//!
//! - **Never while something points at it.** The catalogue keeps what it holds itself (`Catalogue::forget_quiet`: a choice or
//!   a reference by hand, a confirmation, a merge, an expected folder, a retired record, a link from before 0.16, a request's
//!   shared name); this gathers the rest: every request's record, whatever its status, and every record on Soulseek's list
//!   (`protected`).
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
use crate::userdata::UserData;

/// When the pass last ran, in the catalogue's notes (`meta`): at most once a day, across restarts.
const RAN_AT: &str = "retention_at";
const DAY: i64 = 86_400;

/// What an album asked for by its id answers once its record was forgotten (RT01).
pub const NO_LONGER_KNOWN: &str = "This album is no longer known: nobody had opened it or seen it in a list for six months.";

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

/// The forgetting at `now`, where due (`due`): what keeps a record gathered, what nothing keeps forgotten, and the pass's time
/// written. The records forgotten; None: not due.
pub async fn pass(cat: Arc<Catalogue>, ud: Arc<UserData>, now: i64) -> Result<Option<Vec<i64>>> {
    let gone = tokio::task::spawn_blocking(move || -> Result<Option<Vec<i64>>> {
        if !due(&cat, now) {
            return Ok(None);
        }
        let gone = cat.forget_quiet(now, &protected(&ud)?)?;
        cat.set_meta(RAN_AT, &now.to_string());
        Ok(Some(gone))
    })
    .await??;
    if let Some(gone) = gone.as_ref().filter(|g| !g.is_empty()) {
        tracing::info!(target: "library", count = gone.len(), "records not had that no list showed and nobody opened for six months forgotten");
    }
    Ok(gone)
}

/// The pass looked at hourly, from ten minutes after the start (the start's own work first), run where due. Started once
/// Soulseek's list is read (`soulseek::start`), whose items keep their records.
pub fn start(cat: Arc<Catalogue>, ud: Arc<UserData>) {
    tokio::spawn(async move {
        let every = std::time::Duration::from_secs(3600);
        let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + std::time::Duration::from_secs(600), every);
        loop {
            tick.tick().await;
            if let Err(e) = pass(cat.clone(), ud.clone(), super::moment::now_secs()).await {
                tracing::warn!(target: "library", "records not had: the forgetting did not run: {e:#}");
            }
        }
    });
}

/// What Cratarr's album `id` answers where it cannot be read: a record forgotten is no longer known (410), by its tombstone;
/// any other, None, answered as before. Blocking.
pub fn unknown_album(cat: &Catalogue, id: &str) -> Option<ToggleError> {
    match cat.forgotten_at(id) {
        Ok(Some(_)) => Some(ToggleError::Gone(NO_LONGER_KNOWN.into())),
        Ok(None) => None,
        Err(e) => {
            tracing::debug!(target: "library", album = id, "whether the album was forgotten could not be read: {e:#}");
            None
        }
    }
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

    /// RT01: a request keeps its record, whatever its status: one cancelled long ago too.
    #[test]
    fn a_requested_record_is_never_forgotten() {
        let (cat, ud) = (Catalogue::open(None, Vec::new()).unwrap(), UserData::open(None).unwrap());
        // Ids of its own: Soulseek's list is one to the process, other tests' items on it.
        cat.number_above(83_000).unwrap();
        let (asked, other) = (unseen(&cat, "Undertow"), unseen(&cat, "Open Water"));
        let request = crate::userdata::Request {
            id: 0,
            user: "me".into(),
            album_key: "glass harbour|undertow".into(),
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
            record: Some(asked),
        };
        ud.add_request(&request).unwrap();
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
        let (cat, ud) = (Arc::new(Catalogue::open(None, Vec::new()).unwrap()), Arc::new(UserData::open(None).unwrap()));
        cat.number_above(82_000).unwrap();
        let first = unseen(&cat, "Undertow");
        assert_eq!(pass(cat.clone(), ud.clone(), NOW).await.unwrap(), Some(vec![first]), "never run: due");
        let second = unseen(&cat, "Open Water");
        assert_eq!(pass(cat.clone(), ud.clone(), NOW + 3600).await.unwrap(), None, "an hour later: not due");
        assert!(cat.record(second).unwrap().is_some());
        assert_eq!(pass(cat.clone(), ud.clone(), NOW + DAY).await.unwrap(), Some(vec![second]), "a day later");
    }
}
