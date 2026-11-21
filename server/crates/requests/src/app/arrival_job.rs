//! An arrival's job (SCENARIOS-F53 F53.7, T-F53-7; SCENARIOS-LOOKUPS LK.B1, LK.S1): for an arrival whose record has no sure
//! reference (no item, no release its files name: OQ-F53-1 (a)), MusicBrainz's matching and Discogs' search by its names, each once,
//! as a runner job (Cratarr's, keyed by the record), never by a read (AMENDMENTS 23:55: a GET makes nothing).
//!
//! - **Begun when Cratarr first knows the arrival:** the record pass's build that finds it in an Incoming library (`watch`), placed or
//!   dropped in by hand (T-F53-7).
//! - **What it keeps:** each candidate as the record's reference found by a search (`how` "search", not confirmed: a guess, VR8),
//!   its list where the source gives one, its title and year as that reference's facts; no "certain" label, never `take_reference`,
//!   no count worked out (VR10). A candidate a copy in a main library or own music pairs whole with is a reference at each read
//!   (02:32, C15): nothing stored says so.
//! - **It reads no Bandcamp page** (decision 0109, rule 1; F1): no search, and no list through a link either.
//! - **Each search once:** a mark per record and source, kept, so the row says the searches answered (red only then: T-F53-2).

use std::sync::Arc;

use anyhow::{Context, Result};

use super::references_door::References;
use crate::catalogue::identity::Ref;

/// The job's kind: matching's (`runner::kinds`: Activity calls it "Matching").
pub const KIND: &str = super::matching::KIND;
/// A MusicBrainz candidate below this search score is none (0 to 100).
const LEAST_SCORE: i64 = 80;
/// Of what each search finds, how many are read (each a request).
const CANDIDATES: usize = 3;
/// Where a source's search for a record is marked done (the user data's choices, `arrival:searched:<source>:<al-n>`).
const SEARCHED: &str = "arrival:searched:";

fn mark(source: &str, subject: &str) -> String {
    format!("{SEARCHED}{source}:{subject}")
}

/// Whether the arrival's job asked `source` about the record `subject` (`al-<n>`). Blocking.
pub fn searched(ud: &crate::userdata::UserData, subject: &str, source: &str) -> bool {
    ud.import_choice(&mark(source, subject)).ok().flatten().is_some()
}

/// Whether the record holds a sure reference: one by its files, a link, a hand, or a guess confirmed (OQ-F53-1 (a)). Blocking.
fn sure(cat: &crate::catalogue::Catalogue, subject: &str) -> Result<bool> {
    Ok(cat.references_of(subject)?.iter().any(|(_, row)| row.how != "search" || row.confirmed))
}

/// The arrival's job for `record`, begun where it is due: its record holds no sure reference and a switched-on source of the two
/// isn't asked yet. One running is followed, never doubled. Its task.
pub async fn begin(served: Arc<crate::serve::Serve>, record: i64) -> Option<crate::runner::Task> {
    let door = super::references_door::door();
    begin_through(served, record, door).await
}

/// [`begin`], asking through `door`.
pub async fn begin_through(served: Arc<crate::serve::Serve>, record: i64, door: Arc<dyn References>) -> Option<crate::runner::Task> {
    let subject = crate::serve::ids::record(record);
    let (s, sub) = (served.clone(), subject.clone());
    let due = crate::run::spawn_blocking(move || -> Result<bool> {
        let left = super::intake_card::SEARCHED.iter().any(|source| on(source) && !searched(&s.userdata, &sub, source));
        Ok(left && !sure(s.catalogue(), &sub)?)
    })
    .await
    .ok()?
    .ok()?;
    if !due {
        return None;
    }
    let handle = match crate::runner::runner().begin_keyed(KIND, &subject, None) {
        Ok(h) => h,
        Err(Ok(running)) => return Some(running),
        Err(Err(_)) => return None,
    };
    crate::runner::jobs::run(Arc::new(handle), move |h| async move {
        h.step("Looking for the arrival's release", 0, None);
        work(served, door, record).await
    })
    .ok()
}

fn on(source: &str) -> bool {
    use super::switches::{DISCOGS, MUSICBRAINZ, is_on};
    match source {
        "musicbrainz" => is_on(MUSICBRAINZ),
        "discogs" => is_on(DISCOGS),
        _ => false,
    }
}

/// The job's work: each source not asked yet asked by the record's names, its candidates read and kept as guesses, then marked.
async fn work(served: Arc<crate::serve::Serve>, door: Arc<dyn References>, record: i64) -> Result<()> {
    let cat = served.catalogue().clone();
    let c = cat.clone();
    let row = crate::run::spawn_blocking(move || c.record(record)).await??.context("No such record")?;
    let subject = crate::serve::ids::record(row.id);
    let files = {
        let c = cat.clone();
        crate::run::spawn_blocking(move || c.copy_files(record)).await??.into_values().map(|f| f.len()).max().unwrap_or(0)
    };
    for source in super::intake_card::SEARCHED {
        let (ud, sub) = (served.userdata.clone(), subject.clone());
        let asked = crate::run::spawn_blocking(move || searched(&ud, &sub, source)).await?;
        if asked || !on(source) {
            continue;
        }
        let found = match source {
            "musicbrainz" => door
                .find(&row.artist, &row.title)
                .await?
                .into_iter()
                .filter(|c| c.score >= LEAST_SCORE && c.tracks.is_none_or(|n| files == 0 || n == files))
                .take(CANDIDATES)
                .map(|c| Ref::new("musicbrainz", "release", &c.id))
                .collect::<Vec<_>>(),
            _ => door.find_masters(&row.artist, &row.title).await?.into_iter().take(CANDIDATES).map(|c| Ref::new("discogs", "master", &c.id)).collect(),
        };
        for r in found {
            let listing = door.tracklist(r.clone()).await?;
            let (c, sub, at) = (cat.clone(), subject.clone(), super::moment::now_secs());
            crate::run::spawn_blocking(move || -> Result<()> {
                // A guess: found by a search, not confirmed (VR8); held by another record already, it stays theirs.
                if !c.set_reference(&sub, &r, "search")? {
                    return Ok(());
                }
                let of = crate::catalogue::facts::of_ref(&r.kind, &r.value);
                if let Some(l) = &listing {
                    let reference = crate::catalogue::lists::Reference::new(&r.source, &r.kind, &r.value);
                    c.keep_list(record, &reference, &l.tracks, at)?;
                    if let Some(about) = &l.about {
                        c.said(record, "title", &r.source, &of, Some(&about.title), at)?;
                        if let Some(y) = about.year {
                            c.said(record, "year", &r.source, &of, Some(&y.to_string()), at)?;
                        }
                    }
                }
                Ok(())
            })
            .await??;
        }
        let (ud, key) = (served.userdata.clone(), mark(source, &subject));
        crate::run::spawn_blocking(move || ud.set_import_choice(&key, &super::moment::now_secs().to_string())).await??;
    }
    // The open section reads the row again by the change log (LV1).
    crate::run::spawn_blocking(move || cat.owe_records(&[record])).await??;
    Ok(())
}

/// The arrivals Cratarr knows now (the records with a copy in an Incoming library), each one's job begun where it is due. The tasks
/// begun or followed.
pub async fn arrivals_found(served: Arc<crate::serve::Serve>) -> Vec<crate::runner::Task> {
    let incoming: Vec<i64> = served.library.kinds().into_iter().filter(|(_, k)| *k == crate::settings::LibraryKind::Incoming).map(|(f, _)| f).collect();
    if incoming.is_empty() {
        return Vec::new();
    }
    let c = served.catalogue().clone();
    let Ok(Ok(records)) = crate::run::spawn_blocking(move || c.records_held_in(&incoming)).await else { return Vec::new() };
    let mut out = Vec::new();
    for record in records {
        if let Some(t) = begin(served.clone(), record).await {
            out.push(t);
        }
    }
    out
}

/// The record pass's arrivals (LK.S1's line: begun by the pass when it finds an arrival, T-F53-7): each build of the library, the
/// arrivals found, their jobs begun. Started once, by the server's start.
pub fn watch(served: Arc<crate::serve::Serve>) {
    let mut built = served.subscribe_built();
    crate::run::spawn(async move {
        loop {
            arrivals_found(served.clone()).await;
            if built.changed().await.is_err() {
                return;
            }
        }
    });
}
