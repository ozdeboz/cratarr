//! 0.17's step (decision 0100, step 7; C44): what 0.16 knew of each record's references, moved once into the catalogue's
//! tables at the first start of 0.17, before anything reads them.
//!
//! - **What it reads**, the user data as 0.16.2 left it, writing nothing there but its report: `record_references` (0.14's
//!   choices), `album_matches` (matching's finds), `discogs_facts`' album rows, `bandcamp_found`'s pages, `mb_releases`'
//!   Discogs links; and in the catalogue, the release and release group each record's files name. Every row read is in the
//!   report: carried, or in a list that says why not (no record, nothing found, not read, ...).
//! - **What it writes**, the catalogue (`Catalogue::carry_references`): `"references"` rows; `records`' `counting` and
//!   `counted_by_hand` as chosen by hand, `checked_by_hand`; `confirmed`; `lists_to_read`. No list: a match keeps its files'
//!   names and none of MusicBrainz's titles. So no count either: a record whose certain match's release it now holds waits
//!   off Health's references item until the background asker reads the release's list behind any page's question
//!   (`references::lists_pass`), and MusicBrainz counts only where it agrees with the files. `kind` stays as 0.16 left it,
//!   none: nothing reads it, and a record had shows the type its files' tags and the release-type rule give at every build.
//! - **In parts, each once, safe to repeat:** each part (`records::PARTS_017`) is a transaction of its own whose last write is
//!   its mark (`meta`, holding its part of the report), so a part cut off wrote nothing and runs at the next start, the parts
//!   before it done; a data folder that ran an earlier version runs the parts it lacks. The report goes into the user data's
//!   migration report from the marks, as one, whenever a part ran, or where it is missing. Every write finds its row there
//!   the second time and leaves it.
//! - **Failed, it stops the start**, as 0.16's first start does: what it has not carried would be read as never known. The
//!   error names the part that failed or could not begin; a failure outside the parts says so, and that with no mark to set
//!   the way back is a restore (W5). Each part says in the log as it begins (W1).
//! - **Old tables kept** one version, read by nothing else: none is emptied or dropped.
//! - **The discographies** (C48b, a part of its own): each artist's albums and EPs as MusicBrainz listed them (`discography`)
//!   become records not had, but for those a record holds already by its release group, credited to the artist and found
//!   `link`; the artist's reference is read when the discography was. `related_artists` and the matches are read for the names
//!   of artists nothing here names. Before the pages, so a record not had gets the page kept by its group. `discography` stays
//!   as it was, read by Discover as before.
//! - **The waveforms in three bands** (M1, a part of its own after the references, `Catalogue::band_waves_apart`): copied out of
//!   `analysis_shape` into `analysis_band_wave` in batches, so the analysis pass asks which files have none without reading the
//!   visuals' signals stored before them (1.3 GB on production). The shape's rows are never written.
//! - **The credits of the records you have** (P3, W3, a part of its own after `band_wave`, `credits`): who and which labels their
//!   files credit, and what they name at MusicBrainz, kept for their records as a build's record pass keeps them, once. Nothing
//!   forces a build at the upgrade (the layout and the files as they were), so until the first file changed the records you have
//!   would be credited to nobody: Discover's missing albums empty, the paced re-read idle, and the album artists left to
//!   retention's guard alone (W2). The shape is not read (M1); no record is given, made or moved.
//! - **The doubles of one name** (F2.11, OQ-F2a (a); F11.1; the last part, `fold`, after `credits`): the live records of one artist
//!   (by id) and one title's key (`identity::title_key`) that are one album folded into the oldest, had first, as an artist's own list
//!   joins a row since F2: what the parts before could not join (a had record is credited only by `credits`) and what preview 1 made.
//!   Two had records whose copies pair track for track are one record of two copies (the owner: "One record, two copies"). What the
//!   user data names by a record that went follows the one it went into, once; what names its tracks, at the first build.
//! - **One release on two records** (G1): who keeps it goes by the order the parts run in: a link pasted by hand, a certain
//!   match (MusicBrainz matched the record's tracks to it; two on one release, the older record), the files' own (the older
//!   record first), a likely match. A retired record holds none.

use anyhow::{Context, Result};

use crate::catalogue::records::{CREDITS_017, FOLD_017, JOINS, Carry, CarriedChoice, CarriedDiscography, CarriedDiscogs, CarriedLinks, CarriedMatch, CarriedPage, CarriedRequest, Ref};
use crate::catalogue::{Catalogue, FolderRules};
use crate::serve::library::FolderRows;
use crate::settings::LibraryKind;
use crate::userdata::UserData;

/// The migration report's part, as `/api/admin/migration` lists it.
pub const PART: &str = "references";

/// What 0.16 said on an approved request it could not hand to Soulseek, for want of a release group.
const SOULSEEK_016: &str = "Not handed over to Soulseek: no release group to judge it by";

/// The mark that 0.16's word on Soulseek is off the requests (R29): a part of the migration report, what came off in it.
const SOULSEEK_WORD: &str = "requests: 0.16's word on Soulseek";

/// The mark that 0.16's Want covers that fail are off the request rows (Y5): a part of the migration report, listing each.
const REQUEST_COVERS: &str = "requests: 0.16's covers that fail";

/// The report's list of the requests the step closed as asked twice (X7): an account's second open request of one record (W6).
const ASKED_TWICE: &str = "requests_closed_as_asked_twice";

/// A request closed as asked twice, as the report lists it: the request, the one kept, their record.
fn asked_twice_line(request: i64, kept: i64, record: i64) -> serde_json::Value {
    serde_json::json!({ "request": request, "kept": kept, "record": record })
}

/// Those closed as asked twice added to the report's list; none, nothing added.
fn with_asked_twice(report: &mut serde_json::Value, closed: Vec<serde_json::Value>) {
    if closed.is_empty() {
        return;
    }
    match report.get_mut(ASKED_TWICE).and_then(serde_json::Value::as_array_mut) {
        Some(list) => list.extend(closed),
        None => report[ASKED_TWICE] = closed.into(),
    }
}

/// 0.16's word that it could not hand a request to Soulseek taken off every request it is on, once (R29, N12).
fn clear_soulseek_word(ud: &UserData) -> Result<()> {
    if ud.migration_report()?.contains_key(SOULSEEK_WORD) {
        return Ok(());
    }
    let cleared = ud.clear_fetched_clause(SOULSEEK_016)?;
    ud.set_migration_part(SOULSEEK_WORD, &serde_json::json!({ "cleared": cleared }))
}

/// 0.16's Want covers that fail W6's check (any address could be one then) taken off the request rows too, once, under their
/// mark (Y5): the requests part keeps them off the records (O3, `want_covers_dropped`), but each row's own `cover` still reached
/// every viewer of the Requests page. Each listed in the mark's part, as "<request> <address>"; a cover that passes stays.
fn drop_016_request_covers(ud: &UserData) -> Result<()> {
    if ud.migration_report()?.contains_key(REQUEST_COVERS) {
        return Ok(());
    }
    ud.drop_request_covers(super::requests::cover_kept, REQUEST_COVERS)?;
    Ok(())
}

/// The requests with no record still (the step's map unreadable, R19: F12), or naming one the catalogue no longer knows (a
/// catalogue restored from a backup older than the request, fresh track ids: F14), each given the record its release group and
/// names find, as a Want's is (`requests::record_of_album`). At every start; after the first, none is left. Blocking.
fn give_again(cat: &Catalogue, ud: &UserData, asked_twice: &mut Vec<serde_json::Value>) -> Result<()> {
    let all = ud.request_records()?;
    let known = cat.requested(&all.iter().filter_map(|(_, record)| *record).collect::<Vec<_>>())?;
    for (id, record) in all {
        if record.is_some_and(|r| known.contains_key(&r)) {
            continue;
        }
        let Some(r) = ud.request(id)? else { continue };
        let given = super::requests::record_of_album(cat, r.release_group.as_deref(), &r.artist, &r.album, r.year, r.cover.clone())?;
        if let Some(kept) = ud.set_request_record(id, given)? {
            asked_twice.push(asked_twice_line(id, kept, given));
        }
        tracing::info!(target: "library", request = id, record = given, "a request given its record again");
    }
    Ok(())
}

/// The step at a start: done before, or done now. `kinds` (each catalogue folder's library, in the libraries' order) and `rules`
/// (the tag rules) as the library's builds have them: the part `credits` makes the albums a build makes. Blocking.
pub fn run(cat: &Catalogue, ud: &UserData, kinds: &[(i64, LibraryKind)], rules: &FolderRules) -> Result<()> {
    run_with(cat, ud, &|_| Ok(()))?;
    band_waves(cat)?;
    credits(cat, kinds, rules, &|| Ok(()))?;
    fold(cat, ud, kinds, &|| Ok(()))
}

/// The part `credits` (P3, W3): what the files of the records you have name and credit, kept for their records once, as a build's
/// record pass keeps them where no album needs a record (`Catalogue::keep_files_names`). Each folder's rows read once, all but
/// the shape (`Catalogue::rows_to_credit`); the albums made as the library makes them (`album_copies`) and ranked as a build ranks
/// them (`kinds`); each album's record the first its tracks hold, as the rows read say, so no question is asked per album; written
/// in one transaction whose last write is its mark (`Catalogue::credit_files_017`). It says it begins, and how many records it
/// credited. `after` hears it done, inside its transaction (what the tests cut it by). Blocking.
pub(crate) fn credits(cat: &Catalogue, kinds: &[(i64, LibraryKind)], rules: &FolderRules, after: &dyn Fn() -> Result<()>) -> Result<()> {
    let named = || format!("0.17's step, part {CREDITS_017}");
    if cat.part_done_017(CREDITS_017).with_context(named)? {
        return Ok(());
    }
    tracing::info!(target: "library", "0.17's step, part {CREDITS_017}");
    let started = std::time::Instant::now();
    let folders = cat
        .folders()
        .iter()
        .map(|(folder, root)| Ok(FolderRows { folder, root: root.display().to_string(), rows: cat.rows_to_credit(folder)? }))
        .collect::<Result<Vec<_>>>()
        .with_context(named)?;
    let held: std::collections::HashMap<i64, i64> = folders.iter().flat_map(|f| &f.rows).filter_map(|r| Some((r.track?, r.record?))).collect();
    let mut albums = crate::serve::library::album_copies(&folders, rules);
    drop(folders);
    albums.iter_mut().for_each(|a| a.rank = crate::serve::copies::keeping(crate::serve::library::kind_in(kinds, a.folder).0));
    let records: Vec<Option<i64>> = albums.iter().map(|a| a.tracks.iter().find_map(|(track, ..)| held.get(track).copied())).collect();
    if let Some(credited) = cat.credit_files_017(&albums, &records, after).with_context(named)? {
        tracing::info!(target: "library", records = credited, secs = started.elapsed().as_secs(), "0.17's step: the records you have credited as their files credit them");
    }
    Ok(())
}

/// The user data's part of the fold (F2.11): a part of the migration report of its own, written once what it names by a record that
/// went follows the one it went into; it lists each merge (`folded`).
const FOLDED: &str = "references: the doubles of one name folded";

/// The part `fold` (F2.11, OQ-F2a (a); F11.1), the step's last: the doubles of one name folded once (`Catalogue::fold_017`): what the
/// parts before could not join (a had record is credited only by `credits`) and what preview 1 made. It says it begins, and how many
/// it folded; a part that fails stops the start, as the others do. Then the user data follows (`follow_folded`). `after` hears it done,
/// inside its transaction (what the tests cut it by). `kinds` as `run` has them: the two-copies join reads them (RG2). Blocking.
pub(crate) fn fold(cat: &Catalogue, ud: &UserData, kinds: &[(i64, LibraryKind)], after: &dyn Fn() -> Result<()>) -> Result<()> {
    let named = || format!("0.17's step, part {FOLD_017}");
    if !cat.part_done_017(FOLD_017).with_context(named)? {
        tracing::info!(target: "library", "0.17's step, part {FOLD_017}");
        if let Some(folded) = cat.fold_017(kinds, after).with_context(named)? {
            tracing::info!(target: "library", folded = folded.len(), "0.17's step: the doubles of one name folded");
        }
    }
    // Said in the log where it fails, and the start goes on: it is tried again at the next (as the requests' repairs are).
    if let Err(e) = follow_folded(cat, ud) {
        tracing::error!(target: "library", reason = %format!("{e:#}"), "0.17's step: what the user data names by a folded record has not followed it; tried again at the next start");
    }
    Ok(())
}

/// What the user data names by a record the fold merged away (hearts and ratings, what was added and imported, a copy's review and what
/// was heard of it, what was said by hand: `UserData::follow_records`) follows the record it went into, once, under its own part of the
/// migration report (`FOLDED`), which lists each merge. Its tracks' (plays, hearts, playlist entries) follow at the first build, as a
/// take's do (`track_moved`, `Serve::follow_taken`); a request reads its record as merged now, and Soulseek's item follows its request.
/// Blocking.
fn follow_folded(cat: &Catalogue, ud: &UserData) -> Result<()> {
    if ud.migration_report()?.contains_key(FOLDED) {
        return Ok(());
    }
    let Some(folded) = cat.folded_017()? else { return Ok(()) };
    let pairs: Vec<(String, String)> = folded.iter().map(|(from, into)| (crate::serve::ids::record(*from), crate::serve::ids::record(*into))).collect();
    let mut report = serde_json::to_value(ud.follow_records(&pairs)?)?;
    report["folded"] = pairs.iter().map(|(from, into)| format!("{from} into {into}")).collect::<Vec<_>>().into();
    ud.set_migration_part(FOLDED, &report)
}

/// The part `band_wave` (M1): in `catalogue`, below the step, for its tests (SPLIT-PLAN P7).
pub use crate::catalogue::band_waves;

/// What a failure outside the parts says (W5): no part's mark holds what was done, so nothing tells the next start what was half
/// done, and the way back is the backup taken before the upgrade.
const OUTSIDE: &str = "outside its parts (no mark to set: restore)";

/// The context of a failure outside the parts: what the step was doing, and that only a restore undoes it (W5).
fn outside(doing: &str) -> String {
    format!("0.17's step, {doing}, {OUTSIDE}")
}

/// `run`, `after` hearing each part done: an error from it stops the step there, as a stop would (what the tests cut it by).
/// "catalogue" is after the parts, before the report.
fn run_with(cat: &Catalogue, ud: &UserData, after: &dyn Fn(&str) -> Result<()>) -> Result<()> {
    let ran = if cat.references_to_carry().with_context(|| outside("reading which parts are done"))?.is_empty() {
        Vec::new()
    } else {
        let kept = ud.kept_for_017().with_context(|| outside("reading what 0.16 kept"))?;
        cat.carry_references(&carry_of(&kept), after)?
    };
    // The records the step gave the requests, into the user data where it has none yet (C49a): at every start, so a step cut
    // off between its part and here gives them at the next.
    let mut asked_twice = Vec::new();
    for (id, record) in cat.request_records_017().with_context(|| outside("reading the records it gave the requests"))? {
        // An account's second open request of one record, closed as asked twice (W6), is no longer silent (X7): the report lists it.
        if let (_, Some(kept)) = ud.give_request_record(id, record).with_context(|| outside("giving the requests their records"))? {
            asked_twice.push(asked_twice_line(id, kept, record));
        }
    }
    // Repairs of the requests, each said in the log where it fails, and the start goes on: it is tried again at the next (R29, N12).
    // 0.16's word that it could not hand a request to Soulseek, untrue since 0.17's Soulseek takes in every record (C51b), comes off
    // once, under its mark, nothing claimed in its place: whether Soulseek searches for it is Soulseek's own row's to say. 0.16's
    // Want covers that fail come off the rows once, after the requests part has listed them off the records (Y5).
    if let Err(e) = clear_soulseek_word(ud) {
        tracing::error!(target: "library", reason = %format!("{e:#}"), "0.17's step: 0.16's word on Soulseek could not be taken off the requests; tried again at the next start");
    }
    if let Err(e) = drop_016_request_covers(ud) {
        tracing::error!(target: "library", reason = %format!("{e:#}"), "0.17's step: 0.16's request covers that fail could not be taken off; tried again at the next start");
    }
    if let Err(e) = give_again(cat, ud, &mut asked_twice) {
        tracing::error!(target: "library", reason = %format!("{e:#}"), "0.17's step: the requests could not all be given their records again; tried again at the next start");
    }
    if !asked_twice.is_empty() {
        tracing::info!(target: "library", count = asked_twice.len(), "0.17's step: requests closed as asked twice, each an account's second open request of one record");
    }
    if ran.is_empty()
        && let Some((_, mut report)) = ud.migration_report().with_context(|| outside("reading the migration report"))?.remove(PART)
    {
        // Done before: those closed now join its report.
        if !asked_twice.is_empty() {
            with_asked_twice(&mut report, asked_twice);
            ud.set_migration_part(PART, &report).with_context(|| outside("writing its report"))?;
        }
        return Ok(());
    }
    after("catalogue")?;
    let Some(mut report) = cat.references_carried().with_context(|| outside("reading its report"))? else { return Ok(()) };
    with_asked_twice(&mut report, asked_twice);
    ud.set_migration_part(PART, &report).with_context(|| outside("writing its report"))?;
    if !ran.is_empty() {
        let references = report["tables"].as_array().and_then(|t| t.iter().find(|t| t["name"] == "references")).and_then(|t| t["after"].as_u64()).unwrap_or(0);
        tracing::info!(
            target: "library",
            parts = ran.join(", "),
            references,
            counted_by_hand = report["counted_by_hand"].as_u64().unwrap_or(0),
            lists_to_read = report["lists_to_read"].as_u64().unwrap_or(0),
            checked_by_hand = report["checked_by_hand"].as_u64().unwrap_or(0),
            "0.17's step: the records' references moved into the catalogue"
        );
    }
    Ok(())
}

/// What the user data's rows say, read: each row's JSON as 0.16 wrote it. A row that carries nothing is noted by why, for the
/// report: nothing found (asked in vain), a miss, a page that is no album's, a release with no Discogs link, JSON that cannot
/// be read.
fn carry_of(kept: &crate::userdata::KeptFor017) -> Carry {
    let mut noted: Vec<(&'static str, Vec<String>)> = Vec::new();
    let mut note = |what: &'static str, key: &str| match noted.iter_mut().find(|(w, _)| *w == what) {
        Some((_, keys)) => keys.push(key.to_string()),
        None => noted.push((what, vec![key.to_string()])),
    };
    let mut chosen = Vec::new();
    for (album, json) in &kept.chosen {
        match serde_json::from_str::<super::references::Chosen>(json) {
            Ok(k) => {
                let (links, unlinked): (Vec<_>, Vec<_>) = k.links.values().map(|url| (url, super::references::ref_of_link(url))).partition(|(_, r)| r.is_some());
                chosen.push(CarriedChoice {
                    album: album.clone(),
                    counting: k.counting,
                    checked: k.by_hand,
                    links: links.into_iter().filter_map(|(_, r)| r).collect(),
                    unlinked: unlinked.into_iter().map(|(url, _)| url.clone()).collect(),
                    confirmed: k.confirmed.into_iter().collect(),
                });
            }
            Err(_) => note("record_references: not read", album),
        }
    }
    let mut matches = Vec::new();
    for (album, at, json) in &kept.matches {
        let Some(json) = json else {
            note("album_matches: nothing found", album);
            continue;
        };
        match serde_json::from_str::<super::matching::Match>(json) {
            Ok(m) => matches.push(CarriedMatch { album: album.clone(), at: *at, release: m.release.to_ascii_lowercase(), group: m.release_group.map(|g| g.to_ascii_lowercase()), certain: m.certain }),
            Err(_) => note("album_matches: not read", album),
        }
    }
    let mut discogs = Vec::new();
    for (subject, at, json) in &kept.discogs {
        let Some(album) = subject.strip_prefix("album:") else { continue };
        let Some(json) = json else {
            note("discogs_facts: albums: nothing found", subject);
            continue;
        };
        match serde_json::from_str::<super::discogs::AlbumFacts>(json) {
            Ok(f) => discogs.push(CarriedDiscogs { album: album.to_string(), at: *at, master: f.master, release: f.release }),
            Err(_) => note("discogs_facts: albums: not read", subject),
        }
    }
    let mut pages = Vec::new();
    for (key, at, page) in &kept.pages {
        match page.as_deref().map(crate::bandcamp::album_page) {
            // A release group in capitals is the same group: the references keep ids as MusicBrainz writes them.
            Some(Some(page)) => pages.push(CarriedPage { key: if super::discogs::is_mbid(key) { key.to_ascii_lowercase() } else { key.clone() }, at: *at, page }),
            Some(None) => note("bandcamp_found: no album page", key),
            None => note("bandcamp_found: a miss", key),
        }
    }
    let mut links = Vec::new();
    for (release, at, json) in &kept.releases {
        match serde_json::from_str::<super::works::Stored>(json) {
            Ok(s) => {
                let discogs: Vec<Ref> = s.discogs.iter().filter_map(|l| l.split_once(':')).filter(|(kind, id)| matches!(*kind, "release" | "master") && id.parse::<u64>().is_ok()).map(|(kind, id)| Ref::new("discogs", kind, id)).collect();
                if discogs.is_empty() {
                    note("mb_releases: no Discogs link", release);
                } else {
                    links.push(CarriedLinks { release: release.to_ascii_lowercase(), at: *at, discogs });
                }
            }
            Err(_) => note("mb_releases: not read", release),
        }
    }
    // The discographies' artists by the names they may go by: a related artist's (MusicBrainz's own name for that id), then a
    // match's credit of one artist.
    let mut names: std::collections::HashMap<String, Vec<(String, &'static str)>> = std::collections::HashMap::new();
    for json in &kept.related {
        let Ok(found) = serde_json::from_str::<super::related::Found>(json) else { continue };
        for a in found.artists {
            if let Some(id) = a.mbid {
                names.entry(id.to_ascii_lowercase()).or_default().push((a.name, "link"));
            }
        }
    }
    for (_, _, json) in &kept.matches {
        let Some(Ok(m)) = json.as_deref().map(serde_json::from_str::<super::matching::Match>) else { continue };
        let lower = m.artist.to_lowercase();
        if let Some(id) = m.artist_id.filter(|_| !m.artist.is_empty() && !JOINS.iter().any(|j| lower.contains(j))) {
            names.entry(id.to_ascii_lowercase()).or_default().push((m.artist, "search"));
        }
    }
    let mut discographies = Vec::new();
    for (artist, at, groups) in &kept.discographies {
        let mbid = artist.to_ascii_lowercase();
        match super::discover::discography_listings(groups) {
            Ok(listings) => discographies.push(CarriedDiscography { names: names.get(&mbid).cloned().unwrap_or_default(), mbid, at: *at, listings }),
            Err(_) => note("discography: not read", artist),
        }
    }
    let requests = kept
        .requests
        .iter()
        .map(|(id, key, group, album, artist, year, cover, at)| CarriedRequest {
            id: *id,
            key: key.clone(),
            group: group.as_deref().map(str::trim).filter(|g| !g.is_empty()).map(str::to_ascii_lowercase),
            album: album.clone(),
            artist: artist.clone(),
            year: *year,
            cover: cover.clone(),
            at: *at,
        })
        .collect();
    Carry {
        chosen,
        matches,
        discogs,
        pages,
        links,
        discographies,
        requests,
        read: vec![
            ("record_references", kept.chosen.len() as u64),
            ("album_matches", kept.matches.len() as u64),
            ("discogs_facts: albums", kept.discogs.len() as u64),
            ("bandcamp_found", kept.pages.len() as u64),
            ("mb_releases", kept.releases.len() as u64),
            ("discography", kept.discographies.len() as u64),
            ("request", kept.requests.len() as u64),
        ],
        noted,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rusqlite::Connection;
    use rusqlite::types::Value;

    use super::*;
    use crate::catalogue::records::PARTS_017;

    /// The step as these tests run it: no word from the app on the libraries (the first folder Music, the others Incoming), the
    /// tag rules as they come.
    fn run(cat: &Catalogue, ud: &UserData) -> Result<()> {
        super::run(cat, ud, &[], &FolderRules::default())
    }

    /// FX01's data folder, its catalogue opened with its one library (`/music`) in use.
    struct Data(PathBuf);

    impl Data {
        fn new(name: &str) -> Self {
            let d = std::env::temp_dir().join(format!("cratarr-step-017-{name}-{}", std::process::id()));
            std::fs::remove_dir_all(&d).ok();
            crate::catalogue::records::data_0_16(&d).unwrap();
            Self(d)
        }

        fn open(&self) -> (Catalogue, UserData) {
            let cat = Catalogue::open(Some(&self.0.join("catalogue.sqlite")), vec![PathBuf::from("/music")]).unwrap();
            (cat, UserData::open(Some(&self.0.join("userdata.sqlite"))).unwrap())
        }

        /// The catalogue opened with these libraries in use (their folders in this order), and the user data.
        fn open_with(&self, roots: &[&str]) -> (Catalogue, UserData) {
            let cat = Catalogue::open(Some(&self.0.join("catalogue.sqlite")), roots.iter().map(|r| PathBuf::from(*r)).collect()).unwrap();
            (cat, UserData::open(Some(&self.0.join("userdata.sqlite"))).unwrap())
        }

        fn run(&self) {
            let (cat, ud) = self.open();
            run(&cat, &ud).unwrap();
        }

        fn rows_of(&self, file: &str, sql: &str) -> Vec<String> {
            let db = Connection::open(self.0.join(file)).unwrap();
            let mut st = db.prepare(sql).unwrap();
            let n = st.column_count();
            st.query_map([], |r| Ok((0..n).map(|i| format!("{:?}", r.get::<_, Value>(i).unwrap())).collect::<Vec<_>>().join("|"))).unwrap().map(Result::unwrap).collect()
        }

        /// What the step leaves in the tables, row by row, but for when: the references, the records' new columns, the lists,
        /// and the old table it read.
        fn rows(&self) -> Vec<String> {
            [
                self.rows_of("catalogue.sqlite", "SELECT subject, source, kind, value, how, confirmed, read_at FROM \"references\" ORDER BY subject, source, kind, value"),
                self.rows_of("catalogue.sqlite", "SELECT id, counting, counted_by_hand, checked_by_hand, kind FROM records ORDER BY id"),
                self.rows_of("catalogue.sqlite", "SELECT record, source FROM reference_tracks ORDER BY record, source"),
                self.rows_of("userdata.sqlite", "SELECT count(*) FROM record_references"),
                self.rows_of("userdata.sqlite", "SELECT id, record FROM request ORDER BY id"),
            ]
            .concat()
        }

        /// `rows`, its marks and its report.
        fn left(&self) -> Vec<String> {
            [
                self.rows(),
                self.rows_of("catalogue.sqlite", "SELECT key, value FROM meta WHERE key GLOB '017:*' ORDER BY key"),
                self.rows_of("userdata.sqlite", "SELECT report FROM migration_report WHERE part = 'references'"),
            ]
            .concat()
        }

        /// A query's one column of text.
        fn texts(&self, file: &str, sql: &str) -> Vec<String> {
            let db = Connection::open(self.0.join(file)).unwrap();
            let mut st = db.prepare(sql).unwrap();
            st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
        }

        /// The parts marked done.
        fn marks(&self) -> Vec<String> {
            self.texts("catalogue.sqlite", "SELECT substr(key, 5) FROM meta WHERE key GLOB '017:*' ORDER BY key")
        }

        fn sql<T: rusqlite::types::FromSql>(&self, file: &str, sql: &str) -> T {
            Connection::open(self.0.join(file)).unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
        }

        fn batch(&self, file: &str, sql: &str) {
            Connection::open(self.0.join(file)).unwrap().execute_batch(sql).unwrap();
        }

        /// P3's data: FX01 with its had records' files tagged in production's shapes (`fixtures::tag_0_16_shapes`), Signal's copy
        /// in bob's own music.
        fn tagged(name: &str) -> Self {
            let data = Self::new(name);
            crate::fixtures_base::tag_0_16_shapes(&data.open_both().0);
            data
        }

        /// The catalogue opened with Music and bob's own music in use, the user data, and each library's kind as the app says.
        fn open_both(&self) -> (Catalogue, UserData, Vec<(i64, LibraryKind)>) {
            let cat = Catalogue::open(Some(&self.0.join("catalogue.sqlite")), vec![PathBuf::from("/music"), PathBuf::from(crate::fixtures_base::BOB_OWN)]).unwrap();
            let own = cat.folders().id_of(std::path::Path::new(crate::fixtures_base::BOB_OWN)).unwrap();
            (cat, UserData::open(Some(&self.0.join("userdata.sqlite"))).unwrap(), vec![(0, LibraryKind::Main), (own, LibraryKind::Own)])
        }

        /// The credits the files give and the part keeps (P3): the records' artists and labels and the `files` references, but
        /// for when each reference was found, sorted.
        fn credit_rows(&self) -> Vec<String> {
            [
                self.rows_of("catalogue.sqlite", "SELECT record, artist, position, how FROM record_artists ORDER BY record, artist"),
                self.rows_of("catalogue.sqlite", "SELECT record, label, catno, how FROM record_labels ORDER BY record, label, catno"),
                self.rows_of("catalogue.sqlite", "SELECT subject, source, kind, value, how, confirmed, read_at FROM \"references\" WHERE how = 'files' ORDER BY subject, source, kind, value"),
            ]
            .concat()
        }

        /// `credit_rows`, when each reference was found too.
        fn credit_rows_with_at(&self) -> Vec<String> {
            [self.credit_rows(), self.rows_of("catalogue.sqlite", "SELECT subject, source, kind, value, at FROM \"references\" WHERE how = 'files' ORDER BY subject, source, kind, value")].concat()
        }

        /// The records' `files` credits: (record, artist, position), in order.
        fn files_credits(&self) -> Vec<(i64, i64, i64)> {
            let db = Connection::open(self.0.join("catalogue.sqlite")).unwrap();
            let mut st = db.prepare("SELECT record, artist, position FROM record_artists WHERE how = 'files' ORDER BY record, position").unwrap();
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
        }
    }

    /// The albums a build reads of the catalogue and gives the record pass (`Library::build_from`): each folder's served rows, the
    /// library's albums, ranked by the libraries' kinds.
    fn as_built(cat: &Catalogue, kinds: &[(i64, LibraryKind)]) -> Vec<crate::catalogue::AlbumCopy> {
        let folders: Vec<FolderRows> = cat.folders().iter().map(|(folder, root)| FolderRows { folder, root: root.display().to_string(), rows: cat.served_rows(folder).unwrap() }).collect();
        let mut albums = crate::serve::library::album_copies(&folders, &FolderRules::default());
        albums.iter_mut().for_each(|a| a.rank = crate::serve::copies::keeping(crate::serve::library::kind_in(kinds, a.folder).0));
        albums
    }

    /// The step without its credits part: what a data folder holds that ran it before the part came (P3.3), or the step before a
    /// build (P3.2).
    fn run_without_credits(cat: &Catalogue, ud: &UserData) {
        run_with(cat, ud, &|_| Ok(())).unwrap();
        band_waves(cat).unwrap();
    }

    /// The step without its part `fold`: the parts, the band waves and the credits (P3's tests compare a build with this; the fold's
    /// make their doubles after it).
    fn run_without_fold(cat: &Catalogue, ud: &UserData, kinds: &[(i64, LibraryKind)]) {
        run_without_credits(cat, ud);
        credits(cat, kinds, &FolderRules::default(), &|| Ok(())).unwrap();
    }

    /// The function whose head is `head` in `src`, to its closing brace, indented as its head is (the lines ended as a checkout
    /// may end them).
    fn body(src: &str, head: &str) -> String {
        let src = src.replace("\r\n", "\n");
        let from = src.find(head).unwrap_or_else(|| panic!("no {head}"));
        let indent = &src[src[..from].rfind('\n').map_or(0, |n| n + 1)..from];
        let to = src[from..].find(&format!("\n{indent}}}\n")).unwrap_or_else(|| panic!("no end of {head}"));
        src[from..from + to].to_string()
    }

    impl Drop for Data {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    /// Run twice, the second finds its marks and does nothing; its marks gone, it writes nothing new; cut off after any part,
    /// the parts before it are done and that one not, and the next start ends where an uncut step does.
    #[test]
    fn the_references_step_is_safe_to_repeat() {
        let once = Data::new("once");
        once.run();
        let left = once.left();
        assert!(left.iter().any(|r| r.contains("\"hand\"")), "{left:?}");
        once.run();
        assert_eq!(once.left(), left, "its marks there: nothing");
        once.batch("catalogue.sqlite", "DELETE FROM meta WHERE key GLOB '017:*'");
        once.run();
        assert_eq!(once.rows(), left[..once.rows().len()], "its marks gone: every row found as it was left");
        for (n, part) in PARTS_017.into_iter().chain(["catalogue"]).enumerate() {
            let cut = Data::new(&format!("cut-{part}"));
            {
                let (cat, ud) = cut.open();
                let err = run_with(&cat, &ud, &|at| if at == part { anyhow::bail!("stopped after {at}") } else { Ok(()) }).unwrap_err();
                // A part cut off is named as its failure would be (W1).
                let said = if part == "catalogue" { format!("stopped after {part}") } else { format!("0.17's step, part {part}: stopped after {part}") };
                assert_eq!(format!("{err:#}"), said);
            }
            let mut done: Vec<String> = PARTS_017[..n.min(PARTS_017.len())].iter().map(|p| p.to_string()).collect();
            done.sort();
            assert_eq!(cut.marks(), done, "cut after {part}: the parts before it done, that one not");
            if n == 0 {
                assert_eq!(cut.sql::<i64>("catalogue.sqlite", "SELECT count(*) FROM \"references\""), 0, "a part is one transaction: nothing of it");
            }
            cut.run();
            assert_eq!(cut.left(), left, "cut after {part}, then started again");
        }
    }

    /// A1.1.5 (a pin): FX01's step makes no record not had that holds Discogs references alone, so none is left on an artist's list
    /// by a Discogs row the rule no longer keeps; the step does give Discogs references (to had records).
    #[test]
    fn the_step_makes_no_record_of_discogs_references_alone() {
        let d = Data::new("discogs-alone");
        d.run();
        assert!(d.sql::<i64>("catalogue.sqlite", "SELECT count(*) FROM \"references\" WHERE source = 'discogs' AND subject GLOB 'al-*'") > 0, "Discogs references made");
        let alone: i64 = d.sql(
            "catalogue.sqlite",
            "SELECT count(*) FROM records r WHERE r.merged_into IS NULL AND NOT EXISTS (SELECT 1 FROM tracks t WHERE t.record = r.id)
               AND EXISTS (SELECT 1 FROM \"references\" x WHERE x.subject = 'al-' || r.id AND x.source = 'discogs')
               AND NOT EXISTS (SELECT 1 FROM \"references\" x WHERE x.subject = 'al-' || r.id AND x.source <> 'discogs')",
        );
        assert_eq!(alone, 0);
    }

    /// R2: each part has a mark of its own, so a data folder that ran an earlier version of the step, one without a part, runs
    /// that part at its next start and not the parts it ran: here the pages part is the one it lacked, and a link pasted that
    /// was let go of since stays let go of. The report is written again, with the new part's.
    #[test]
    fn a_later_part_runs_where_an_earlier_part_is_marked() {
        let data = Data::new("later-part");
        data.run();
        // The pages part's pages: a request's (After Hours') is the requests part's, done and not run again.
        let pages = data.sql::<i64>("catalogue.sqlite", "SELECT count(*) FROM \"references\" WHERE source = 'bandcamp' AND value NOT LIKE 'https://paperlantern.%'");
        assert!(pages > 0);
        data.batch(
            "catalogue.sqlite",
            "DELETE FROM \"references\" WHERE source = 'bandcamp' AND value NOT LIKE 'https://paperlantern.%'; DELETE FROM meta WHERE key = '017:pages';
             DELETE FROM \"references\" WHERE subject = 'al-2' AND source = 'discogs' AND how = 'hand';",
        );
        data.batch("userdata.sqlite", "UPDATE migration_report SET report = '{}' WHERE part = 'references'");
        data.run();
        assert_eq!(data.sql::<i64>("catalogue.sqlite", "SELECT count(*) FROM \"references\" WHERE source = 'bandcamp' AND value NOT LIKE 'https://paperlantern.%'"), pages, "the part it lacked ran");
        assert_eq!(data.sql::<i64>("catalogue.sqlite", "SELECT count(*) FROM \"references\" WHERE subject = 'al-2' AND source = 'discogs' AND how = 'hand'"), 0, "a part done did not run again");
        assert_eq!(data.marks().len(), PARTS_017.len() + 3, "and the band waves', the credits' and the fold's");
        let (_, ud) = data.open();
        let report = &ud.migration_report().unwrap()[PART].1;
        assert_eq!(table(report, "bandcamp_found").0, data.sql::<i64>("userdata.sqlite", "SELECT count(*) FROM bandcamp_found") as u64, "{report}");
    }

    fn mb(n: u32) -> String {
        format!("00000000-0000-4000-8000-{n:012}")
    }

    /// Ferry Lights' release and release group, which one of its matches names in capitals.
    const FERRY: &str = "abcdef00-0000-4000-8000-000000000020";
    const FERRY_GROUP: &str = "abcdef00-0000-4000-8000-000000000120";

    /// The samples of one of the report's `differed` lists.
    fn differed(report: &serde_json::Value, what: &str) -> Vec<String> {
        let list = report["differed"].as_array().unwrap().iter().find(|d| d["what"] == what).unwrap_or_else(|| panic!("no {what}: {report}"));
        list["sample"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
    }

    /// One of the report's `unmatched` lists: its count and samples (none: no such list).
    fn listed(report: &serde_json::Value, what: &str) -> (u64, Vec<String>) {
        report["unmatched"]
            .as_array()
            .unwrap()
            .iter()
            .find(|u| u["what"] == what)
            .map(|u| (u["count"].as_u64().unwrap(), u["sample"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()))
            .unwrap_or_default()
    }

    /// A table of the report: its rows before and those carried.
    fn table(report: &serde_json::Value, name: &str) -> (u64, u64) {
        let t = report["tables"].as_array().unwrap().iter().find(|t| t["name"] == name).unwrap_or_else(|| panic!("no table {name}: {report}"));
        (t["before"].as_u64().unwrap(), t["moved"].as_u64().unwrap())
    }

    /// R1: a certain match never takes what a hand pasted: al-2's MusicBrainz link stays where it was, record 9's certain match
    /// on that release notwithstanding, which keeps its release group.
    #[test]
    fn a_certain_match_never_takes_a_hand_link() {
        let data = Data::new("hand-link");
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        assert_eq!(cat.subject_of(&Ref::new("musicbrainz", "release", &mb(2))).unwrap(), ["al-2"]);
        assert_eq!(refs(&cat, 9), [by("musicbrainz", "release-group", &mb(102), "search", false)]);
        let report = &ud.migration_report().unwrap()[PART].1;
        assert!(differed(report, "references: held by another record").contains(&format!("al-9 musicbrainz:release:{}", mb(2))), "{report}");
        assert!(differed(report, "references: taken from another record").is_empty(), "{report}");
    }

    /// R1: a retired record holds no release: the link pasted while Partial Shore lived is not its, and the release goes to the
    /// record its files are now (record 8), as does the page Bandcamp was found by its names (R2: a retired record is not one
    /// of the records of a name).
    #[test]
    fn a_retired_record_gives_its_release_to_a_live_one() {
        let data = Data::new("retired");
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        assert_eq!(cat.subject_of(&Ref::new("musicbrainz", "release", &mb(6))).unwrap(), ["al-8"]);
        assert_eq!(
            refs(&cat, 8),
            [by("bandcamp", "album", "https://glassharbour.bandcamp.com/album/partial-shore", "search", false), by("musicbrainz", "release", &mb(6), "files", false)]
        );
        assert!(refs(&cat, 6).iter().all(|(_, kind, ..)| kind != "release"), "{:?}", refs(&cat, 6));
        let report = &ud.migration_report().unwrap()[PART].1;
        assert!(differed(report, "references: a retired record's").contains(&format!("al-6 musicbrainz:release:{}", mb(6))), "{report}");
    }

    /// R2: MusicBrainz's verified Discogs link goes before a Discogs search by names: Undated's search found Night Walks'
    /// release, which MusicBrainz links to Night Walks' release; Night Walks keeps it, and Undated's is refused.
    #[test]
    fn a_verified_link_beats_a_name_search() {
        let data = Data::new("verified");
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        assert_eq!(cat.subject_of(&Ref::new("discogs", "release", "2000003")).unwrap(), ["al-3"]);
        assert!(refs(&cat, 3).contains(&by("discogs", "release", "2000003", "link", false)), "{:?}", refs(&cat, 3));
        let report = &ud.migration_report().unwrap()[PART].1;
        assert!(differed(report, "references: held by another record").contains(&"al-4 discogs:release:2000003".to_string()), "{report}");
    }

    /// R2: two certain matches on one release, the older record keeps it, whatever order their keys come in (al-10 before
    /// al-11, and al-11 is the younger); both keep the release group, which is every edition's.
    #[test]
    fn two_certain_matches_on_one_release_go_by_age() {
        let data = Data::new("age");
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        assert_eq!(cat.subject_of(&Ref::new("musicbrainz", "release", FERRY)).unwrap(), ["al-10"]);
        assert_eq!(cat.subject_of(&Ref::new("musicbrainz", "release-group", FERRY_GROUP)).unwrap(), ["al-10", "al-11"]);
        let report = &ud.migration_report().unwrap()[PART].1;
        assert!(differed(report, "references: held by another record").contains(&format!("al-11 musicbrainz:release:{FERRY}")), "{report}");
    }

    /// R11: two 0.16 hand links on one release go by the records' age, the older keeping it, whatever their keys' order
    /// (al-10 before al-11, and al-11 is the younger): Ferry Lights' two copies were each pasted Discogs' release.
    #[test]
    fn two_hand_links_on_one_release_go_by_age() {
        let data = Data::new("hand-age");
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        assert_eq!(cat.subject_of(&Ref::new("discogs", "release", "2000020")).unwrap(), ["al-10"]);
        let report = &ud.migration_report().unwrap()[PART].1;
        assert!(differed(report, "references: held by another record").contains(&"al-11 discogs:release:2000020".to_string()), "{report}");
    }

    /// R11 (finding 4): the step run again (its marks gone) keeps what was chosen since: a link pasted on another record, and a
    /// source chosen by hand, stand over 0.16's.
    #[test]
    fn a_step_run_again_keeps_what_was_chosen_since() {
        let data = Data::new("chosen-since");
        data.run();
        {
            let (cat, _) = data.open();
            assert!(cat.set_reference("al-9", &Ref::new("musicbrainz", "release", &mb(2)), "hand").unwrap(), "pasted on the other copy");
            cat.set_counting(2, Some("musicbrainz"), true).unwrap();
        }
        data.batch("catalogue.sqlite", "DELETE FROM meta WHERE key GLOB '017:*'");
        data.run();
        let (cat, _) = data.open();
        assert_eq!(cat.subject_of(&Ref::new("musicbrainz", "release", &mb(2))).unwrap(), ["al-9"], "the link pasted since stands");
        let two = cat.record(2).unwrap().unwrap();
        assert_eq!((two.counting.as_deref(), two.counted_by_hand), (Some("musicbrainz"), true), "the source chosen since stands");
    }

    /// R11 (N14): a mark whose report cannot be read is a part done with its report missing: the start goes on, the part is not
    /// run again, and the migration report is the other parts'.
    #[test]
    fn an_unreadable_mark_does_not_stop_the_start() {
        let data = Data::new("unreadable-mark");
        data.run();
        data.batch("catalogue.sqlite", "UPDATE meta SET value = '{not a report' WHERE key = '017:pages'");
        data.batch("userdata.sqlite", "DELETE FROM migration_report WHERE part = 'references'");
        let (cat, ud) = data.open();
        run(&cat, &ud).expect("the start goes on");
        assert_eq!(data.marks().len(), PARTS_017.len() + 3, "no part run again (and the band waves', the credits' and the fold's)");
        let report = &ud.migration_report().unwrap()[PART].1;
        assert!(report["tables"].as_array().unwrap().iter().all(|t| t["name"] != "bandcamp_found"), "{report}");
        assert!(report["tables"].as_array().unwrap().iter().any(|t| t["name"] == "record_references"));
    }

    /// C49a: each request is given its record: by its release group (a record had, or a discography's record not had), else
    /// the oldest record had of its names, where they name one album (X8; X9: FX01's two copies of Two Rooms hold one release group,
    /// one album); written into the user data, and found as it was when the step runs again.
    #[test]
    fn a_request_finds_its_record() {
        let data = Data::new("request-record");
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        let record = |id: i64| ud.request(id).unwrap().unwrap().record;
        let of_group = |n: u32| -> Vec<i64> { cat.subject_of(&Ref::new("musicbrainz", "release-group", &mb(n))).unwrap().iter().filter_map(|s| crate::serve::ids::record_number(s)).collect() };
        assert_eq!(record(6), Some(1), "Tide Tables, by its group");
        assert_eq!(record(7), Some(2), "Two Rooms, no group: the oldest record had of its names, its two copies (2 and 9) one album (X9)");
        assert_eq!(of_group(111).len(), 1);
        assert_eq!(record(2), of_group(111).first().copied(), "Harbour Lights: the discography's record not had");
        assert_eq!(data.sql::<i64>("userdata.sqlite", "SELECT count(*) FROM request WHERE record IS NULL"), 0, "every request its record");
        let given = data.rows_of("userdata.sqlite", "SELECT id, record FROM request ORDER BY id");
        data.batch("catalogue.sqlite", "DELETE FROM meta WHERE key GLOB '017:*'");
        data.run();
        assert_eq!(data.rows_of("userdata.sqlite", "SELECT id, record FROM request ORDER BY id"), given, "the step again: each as it was");
    }

    /// R19 (F12): an unreadable map of the records the step gave the requests does not stop a start: it is left out, and a request
    /// still with no record is given one again, the one its names find.
    #[test]
    fn an_unreadable_request_map_does_not_stop_the_start() {
        let data = Data::new("request-map");
        data.run();
        let record = || data.sql::<Option<i64>>("userdata.sqlite", "SELECT record FROM request WHERE id = 8");
        let before = record();
        data.batch("catalogue.sqlite", "UPDATE meta SET value = 'not a map' WHERE key = 'requests_017'");
        data.batch("userdata.sqlite", "UPDATE request SET record = NULL WHERE id = 8");
        data.run();
        assert_eq!(record(), before);
    }

    /// R19 (F14): a request naming a record the catalogue no longer knows (a catalogue restored from a backup older than the
    /// request) is given one again at the next start: the one its release group finds.
    #[test]
    fn a_request_whose_record_is_unknown_gets_one_again() {
        let data = Data::new("request-unknown");
        data.run();
        let record = || data.sql::<i64>("userdata.sqlite", "SELECT record FROM request WHERE id = 6");
        let before = record();
        data.batch("userdata.sqlite", "UPDATE request SET record = 99999 WHERE id = 6");
        data.run();
        assert_eq!(record(), before);
    }

    /// R19 (F13): the requests part counts the pages it kept; a request with a release group whose page was found by it, its record
    /// made only by the requests part, gets the page too; a page the requests part carries is no page left over.
    #[test]
    fn the_requests_part_keeps_and_counts_its_pages_truly() {
        let data = Data::new("request-pages");
        let group = "00000000-0000-4000-8000-000000000117";
        data.batch(
            "userdata.sqlite",
            &format!(
                "INSERT INTO request (id, user, album_key, artist_key, release_group, album, artist, year, at, status) VALUES (13, 'ann', '{group}', 'kite theory', '{group}', 'Far Coast', 'Kite Theory', 2021, 1793951000, 'approved');
                 INSERT INTO bandcamp_found (album, at, page) VALUES ('{group}', 1793951000, 'https://kitetheory.bandcamp.com/album/far-coast');"
            ),
        );
        data.run();
        let record = data.sql::<i64>("userdata.sqlite", "SELECT record FROM request WHERE id = 13");
        let page = data.sql::<String>("catalogue.sqlite", &format!("SELECT value FROM \"references\" WHERE subject = 'al-{record}' AND source = 'bandcamp'"));
        assert_eq!(page, "https://kitetheory.bandcamp.com/album/far-coast");
        let (_, ud) = data.open();
        let report = &ud.migration_report().unwrap()[PART].1;
        assert_eq!(report["pages_for_requests"].as_u64(), Some(2), "After Hours' and Far Coast's: {report}");
        assert!(listed(report, "bandcamp_found").1.is_empty(), "{report}");
    }

    /// R22 (the owner's GROUCH request, as production's copy holds it, made up: FX01's 12): asked by the label's account with no
    /// release group, its split names fold to the record not had that the artist's discography made, holding the group; the page
    /// 0.16 found by that group is on it. Request 10, asking the group itself, is taken away, so the discography is the only way
    /// to the record, as on production.
    #[test]
    fn a_requests_page_keyed_by_its_records_group_is_kept() {
        let data = Data::new("request-group-page");
        data.batch("userdata.sqlite", "DELETE FROM request WHERE id = 10");
        data.run();
        let record = data.sql::<i64>("userdata.sqlite", "SELECT record FROM request WHERE id = 12");
        let named = data.sql::<String>("catalogue.sqlite", &format!("SELECT artist || ' - ' || title FROM records WHERE id = {record}"));
        let group = data.sql::<String>("catalogue.sqlite", &format!("SELECT value FROM \"references\" WHERE subject = 'al-{record}' AND source = 'musicbrainz' AND kind = 'release-group'"));
        assert_eq!((named.as_str(), group.as_str()), ("Tape Club - Long Player", "00000000-0000-4000-8000-000000000116"), "the discography's record, by its names");
        let page = data.sql::<String>("catalogue.sqlite", &format!("SELECT value FROM \"references\" WHERE subject = 'al-{record}' AND source = 'bandcamp'"));
        assert_eq!(page, "https://paperlantern.bandcamp.com/album/tape-club-long-player-paper-lantern");
    }

    /// W2 (B1): a request whose names only loosely match an owned album (brackets aside) is no longer given that album: it gets a
    /// record of its own, and the step lists it beside its look-alike for the rehearsal to check.
    #[test]
    fn the_step_lists_requests_beside_owned_look_alikes() {
        let data = Data::new("look-alikes");
        data.batch("userdata.sqlite", "INSERT INTO request (id, user, album_key, artist_key, album, artist, year, at, status) VALUES (15, 'ann', 'glassharbour|tidetablesdeluxe', 'glass harbour', 'Tide Tables (Deluxe)', 'Glass Harbour', 2012, 1793950900, 'approved')");
        data.run();
        let record = |id: i64| data.sql::<i64>("userdata.sqlite", &format!("SELECT record FROM request WHERE id = {id}"));
        assert_ne!(record(15), record(6), "not the owned Tide Tables");
        let (_, ud) = data.open();
        let report = &ud.migration_report().unwrap()[PART].1;
        let alikes: Vec<&str> = report["requests_beside_owned_look_alikes"].as_array().unwrap().iter().filter_map(serde_json::Value::as_str).collect();
        assert!(alikes.iter().any(|l| l.starts_with("15 Glass Harbour - Tide Tables (Deluxe) ~ ") && l.ends_with("- Tide Tables")), "{report}");
    }

    /// O3: a 0.16 Want's cover that fails W6's check (any address could be one then) is dropped by the step, each listed in its
    /// report; no record keeps one.
    #[test]
    fn the_step_drops_016_want_covers_that_fail_and_lists_them() {
        let data = Data::new("want-covers");
        data.run();
        let (_, ud) = data.open();
        let report = &ud.migration_report().unwrap()[PART].1;
        let dropped: Vec<&str> = report["want_covers_dropped"].as_array().unwrap().iter().filter_map(serde_json::Value::as_str).collect();
        assert_eq!(dropped, ["7 https://example.org/covers/two-rooms.jpg", "8 https://example.org/covers/quiet-lanes.jpg"], "{report}");
        assert_eq!(data.sql::<i64>("catalogue.sqlite", "SELECT count(*) FROM records WHERE cover LIKE 'https://example.org/%'"), 0);
    }

    /// Y5 (the review's finding 5): the step takes 0.16's Want covers that fail W6's check off the request rows too, whose own
    /// `cover` the Requests page drew for every viewer, each listed in its part of the report; a cover that passes stays.
    #[test]
    fn the_step_nulls_016_request_covers_that_fail() {
        let data = Data::new("request-covers");
        data.run();
        let cover = |id: i64| data.sql::<Option<String>>("userdata.sqlite", &format!("SELECT cover FROM request WHERE id = {id}"));
        assert_eq!((cover(7), cover(8)), (None, None), "Two Rooms' and Quiet Lanes', from example.org");
        assert_eq!(cover(1).as_deref(), Some("/api/covers/release-group/00000000-0000-4000-8000-000000000112"), "Cratarr's own stays");
        let (_, ud) = data.open();
        let parts = ud.migration_report().unwrap();
        let dropped: Vec<&str> = parts[REQUEST_COVERS].1["request_covers_dropped"].as_array().unwrap().iter().filter_map(serde_json::Value::as_str).collect();
        assert_eq!(dropped, ["7 https://example.org/covers/two-rooms.jpg", "8 https://example.org/covers/quiet-lanes.jpg"]);
    }

    /// X7: a part that fails names itself in the error, as the step's writes outside the parts do.
    #[test]
    fn a_failing_part_names_itself_in_the_error() {
        let data = Data::new("part-fails");
        let err = {
            // Opened first: opening makes the tables the step writes.
            let (cat, ud) = data.open();
            data.batch("catalogue.sqlite", "CREATE TRIGGER broken_part BEFORE INSERT ON \"references\" BEGIN SELECT RAISE(ABORT, 'broken by the test'); END;");
            run_with(&cat, &ud, &|_| Ok(())).unwrap_err()
        };
        let part = err.to_string().strip_prefix("0.17's step, part ").map(str::to_string);
        assert!(part.as_deref().is_some_and(|p| PARTS_017.contains(&p)), "{err:#}");
        assert!(format!("{err:#}").contains("broken by the test"), "{err:#}");
        let at = PARTS_017.iter().position(|p| Some(*p) == part.as_deref()).unwrap();
        let mut before: Vec<String> = PARTS_017[..at].iter().map(|p| p.to_string()).collect();
        before.sort();
        assert_eq!(data.marks(), before, "the parts before it done");
        // A write outside the parts: the requests given their records.
        let given = Data::new("records-fail");
        let (cat, ud) = given.open();
        given.batch("userdata.sqlite", "CREATE TRIGGER broken_record BEFORE UPDATE OF record ON request BEGIN SELECT RAISE(ABORT, 'broken by the test'); END;");
        assert_eq!(run_with(&cat, &ud, &|_| Ok(())).unwrap_err().to_string(), "0.17's step, giving the requests their records, outside its parts (no mark to set: restore)");
    }

    /// The parts the step said it began, in order (W1).
    fn parts_said(lines: &[String]) -> Vec<&str> {
        lines.iter().filter_map(|l| l.strip_prefix("library: 0.17's step, part ")).collect()
    }

    /// P1.1 (W1): each part says it begins, at info under `library`, in the parts' order and then `band_wave`, `credits` and `fold`
    /// (the last, OQ-F2a (a)), before anything
    /// else of it, and only the parts that run: a start cut off after `discogs` says the rest alone. Never a line per row.
    #[test]
    fn the_step_says_each_part_as_it_begins() {
        let data = Data::new("says-parts");
        let (cat, ud) = data.open();
        let lines = crate::diagnostics::logged_lines(|| run(&cat, &ud).unwrap());
        assert_eq!(parts_said(&lines), PARTS_017.iter().copied().chain(["band_wave", "credits", "fold"]).collect::<Vec<_>>(), "{lines:#?}");
        assert_eq!(parts_said(&lines[..1]), ["hand"], "the step's first line is its first part's: {lines:#?}");
        let at = |start: &str| lines.iter().position(|l| l.starts_with(start)).unwrap_or_else(|| panic!("no {start}: {lines:#?}"));
        assert!(at("library: 0.17's step, part requests") < at("library: 0.17's step: the records' references moved into the catalogue"), "{lines:#?}");
        assert!(at("library: 0.17's step, part band_wave") < at("library: 0.17's step: the waveforms in three bands apart"), "{lines:#?}");
        assert!(at("library: 0.17's step, part credits") < at("library: 0.17's step: the records you have credited"), "{lines:#?}");
        assert!(at("library: 0.17's step: the records you have credited") < at("library: 0.17's step, part fold"), "{lines:#?}");
        assert!(at("library: 0.17's step, part fold") < at("library: 0.17's step: the doubles of one name folded"), "{lines:#?}");
        let report = &ud.migration_report().unwrap()[PART].1;
        let read: u64 = report["tables"].as_array().unwrap().iter().map(|t| t["before"].as_u64().unwrap()).sum();
        assert!((lines.len() as u64) < read, "no line per row: {} lines, {read} rows read: {lines:#?}", lines.len());
        let step: Vec<&String> = lines.iter().filter(|l| l.starts_with("library: 0.17's step")).collect();
        assert_eq!(step.iter().collect::<std::collections::HashSet<_>>().len(), step.len(), "each said once: {lines:#?}");

        let cut = Data::new("says-parts-cut");
        let (cat, ud) = cut.open();
        run_with(&cat, &ud, &|p| if p == "discography" { anyhow::bail!("stopped at {p}") } else { Ok(()) }).unwrap_err();
        let mut done: Vec<String> = PARTS_017[..6].iter().map(|p| p.to_string()).collect();
        done.sort();
        assert_eq!(cut.marks(), done, "hand … discogs done");
        let lines = crate::diagnostics::logged_lines(|| run(&cat, &ud).unwrap());
        assert_eq!(parts_said(&lines), ["discography", "pages", "chosen", "requests", "band_wave", "credits", "fold"], "none of the parts marked: {lines:#?}");
        assert_eq!(parts_said(&lines).first(), Some(&"discography"));
    }

    /// P1.2 (W1): a part cut off is the last one said, and the error names it too; the parts before it done, it not; the next
    /// start says it first, runs no part marked, and ends with the report an uncut step writes.
    #[test]
    fn a_part_cut_off_is_the_last_one_said() {
        let data = Data::new("cut-said");
        {
            let (cat, ud) = data.open();
            let mut cut = None;
            let lines = crate::diagnostics::logged_lines(|| cut = run_with(&cat, &ud, &|p| if p == "discogs" { anyhow::bail!("cut") } else { Ok(()) }).err());
            let err = format!("{:#}", cut.expect("cut off"));
            assert_eq!(parts_said(&lines).last(), Some(&"discogs"), "{lines:#?}");
            assert!(err.contains("0.17's step, part discogs"), "{err}");
            assert!(PARTS_017.iter().filter(|p| **p != "discogs").all(|p| !err.contains(&format!("part {p}"))), "no other part: {err}");
        }
        let mut done: Vec<String> = PARTS_017[..5].iter().map(|p| p.to_string()).collect();
        done.sort();
        assert_eq!(data.marks(), done, "hand … links done, discogs not");
        let (cat, ud) = data.open();
        let lines = crate::diagnostics::logged_lines(|| run(&cat, &ud).unwrap());
        assert_eq!(parts_said(&lines).first(), Some(&"discogs"), "{lines:#?}");
        assert_eq!(parts_said(&lines), PARTS_017[5..].iter().copied().chain(["band_wave", "credits", "fold"]).collect::<Vec<_>>(), "no part marked runs again");
        let report = "SELECT report FROM migration_report WHERE part = 'references'";
        let uncut = Data::new("cut-said-uncut");
        uncut.run();
        assert_eq!(data.rows_of("userdata.sqlite", report), uncut.rows_of("userdata.sqlite", report), "the report an uncut step writes");
    }

    /// P3.1 (W3): after the step, before any build, every record a present file holds is credited as its files credit it (`files`):
    /// its album artists in tag order, Various Artists for a compilation, the untagged one by name to the first artist of that name
    /// (a second of the name made apart), the record of two copies once per artist and place; the two labelled their labels; the
    /// part marked with how many records it credited; and Discover's missing list not empty. No had record with an album artist
    /// in its files goes uncredited.
    #[test]
    fn the_step_credits_every_record_you_have() {
        let data = Data::tagged("credits");
        let (cat, ud, kinds) = data.open_both();
        let kite = cat.artist_for("Kite Theory").unwrap();
        assert!(cat.artist_apart("Kite Theory").unwrap() > kite, "a second Kite Theory");
        super::run(&cat, &ud, &kinds, &FolderRules::default()).unwrap();
        let artist = |name: &str| cat.artist_of(name).unwrap().unwrap_or_else(|| panic!("no {name}"));
        let held: Vec<i64> = data
            .texts("catalogue.sqlite", "SELECT DISTINCT CAST(t.record AS TEXT) FROM files f JOIN tracks t ON t.id = f.track WHERE f.folder >= 0 AND f.gone_at IS NULL AND f.trashed IS NULL ORDER BY t.record")
            .iter()
            .map(|r| r.parse().unwrap())
            .collect();
        assert_eq!(held, [1, 2, 3, 4, 5, 7, 8], "the records a present file holds");
        let expected: Vec<(i64, i64, i64)> = [
            (1, vec!["Glass Harbour"]),
            (2, vec!["Noor Field", "Orbit Lane"]),
            (3, vec!["Kite Theory"]),
            (4, vec!["Kite Theory"]),
            (5, vec!["Various Artists"]),
            (7, vec!["Noor Field"]),
            (8, vec!["Glass Harbour"]),
        ]
        .into_iter()
        .flat_map(|(record, names)| names.into_iter().enumerate().map(move |(position, name)| (record, name, position as i64)).collect::<Vec<_>>())
        .map(|(record, name, position)| (record, artist(name), position))
        .collect();
        assert_eq!(data.files_credits(), expected, "each record its files' album artists, in order, and no other");
        assert!(data.files_credits().iter().filter(|(record, ..)| *record == 4).all(|(_, a, _)| *a == kite), "the first of the name");
        assert_eq!(data.sql::<i64>("catalogue.sqlite", "SELECT count(*) FROM record_artists WHERE record = 7"), 1, "two copies: once");
        let labels = data.rows_of("catalogue.sqlite", "SELECT r.record, l.name, r.catno FROM record_labels r JOIN labels l ON l.id = r.label WHERE r.how = 'files' ORDER BY r.record");
        assert_eq!(labels, ["Integer(1)|Text(\"Quiet Pier\")|Text(\"QP 001\")", "Integer(2)|Text(\"Long Room\")|Text(\"LR-2\")"]);
        assert_eq!(cat.meta("017:credits").as_deref(), Some(r#"{"records":7}"#), "marked, with the records it credited");
        assert!(!cat.missing_everywhere().unwrap().0.is_empty(), "Discover's missing albums: Glass Harbour's discography");
    }

    /// P3.2 (W3): the part writes exactly what the first build's record pass writes (A: the step to it, as the fold after it merges
    /// records, F2.11; B: the step without it, then the build's
    /// `keep_files_names` over the albums a build reads), and a build after it finds nothing to write, `at` included.
    #[test]
    fn the_steps_credits_are_the_first_builds() {
        let (a, b) = (Data::tagged("credits-a"), Data::tagged("credits-b"));
        let (cat, ud, kinds) = a.open_both();
        run_without_fold(&cat, &ud, &kinds);
        {
            let (cat, ud, kinds) = b.open_both();
            run_without_credits(&cat, &ud);
            cat.keep_files_names(&as_built(&cat, &kinds)).unwrap();
        }
        assert!(a.credit_rows().iter().any(|r| r.ends_with("|Text(\"files\")")), "credits written");
        assert_eq!(a.credit_rows(), b.credit_rows(), "what the first build writes");
        let before = a.credit_rows_with_at();
        cat.keep_files_names(&as_built(&cat, &kinds)).unwrap();
        assert_eq!(a.credit_rows_with_at(), before, "the first build finds nothing to write");
    }

    /// P3.3 (W3): the part runs once: run again, it writes nothing and says nothing; cut off by its `after`, it wrote no credit and
    /// set no mark, and the next start completes it; a data folder whose step was done before the part came (every other mark,
    /// staging's after preview 1's first deploy) runs it at the next start, and the fold after it (F2.11).
    #[test]
    fn the_credits_part_runs_once_and_goes_on_after_a_cut() {
        let twice = Data::tagged("credits-twice");
        let (cat, ud, kinds) = twice.open_both();
        super::run(&cat, &ud, &kinds, &FolderRules::default()).unwrap();
        let (first, marks) = (twice.credit_rows_with_at(), twice.rows_of("catalogue.sqlite", "SELECT key, value FROM meta WHERE key GLOB '017:*' ORDER BY key"));
        assert!(twice.marks().contains(&"credits".to_string()));
        let lines = crate::diagnostics::logged_lines(|| super::run(&cat, &ud, &kinds, &FolderRules::default()).unwrap());
        assert_eq!(twice.credit_rows_with_at(), first, "run again: nothing written");
        assert_eq!(twice.rows_of("catalogue.sqlite", "SELECT key, value FROM meta WHERE key GLOB '017:*' ORDER BY key"), marks);
        assert!(parts_said(&lines).is_empty() && !lines.iter().any(|l| l.contains("credited")), "and nothing said: {lines:#?}");

        let cut = Data::tagged("credits-cut");
        let (cat, ud, kinds) = cut.open_both();
        run_without_credits(&cat, &ud);
        let err = credits(&cat, &kinds, &FolderRules::default(), &|| anyhow::bail!("cut")).unwrap_err();
        assert_eq!(format!("{err:#}"), "0.17's step, part credits: cut");
        let files = |data: &Data| data.sql::<i64>("catalogue.sqlite", "SELECT (SELECT count(*) FROM record_artists WHERE how = 'files') + (SELECT count(*) FROM record_labels WHERE how = 'files')");
        assert_eq!(files(&cut), 0, "cut off: no credit of the files written");
        assert!(!cut.marks().contains(&"credits".to_string()), "and no mark");
        super::run(&cat, &ud, &kinds, &FolderRules::default()).unwrap();
        assert!(cut.marks().contains(&"credits".to_string()));
        assert_eq!(cut.credit_rows(), twice.credit_rows(), "the next start completes it");

        let staged = Data::tagged("credits-staged");
        let (cat, ud, kinds) = staged.open_both();
        run_without_credits(&cat, &ud);
        let mut before: Vec<String> = PARTS_017.iter().map(|p| p.to_string()).chain(["band_wave".to_string()]).collect();
        before.sort();
        assert_eq!(staged.marks(), before, "every mark but the credits'");
        let lines = crate::diagnostics::logged_lines(|| super::run(&cat, &ud, &kinds, &FolderRules::default()).unwrap());
        assert_eq!(parts_said(&lines), ["credits", "fold"], "the part, and the fold after it: {lines:#?}");
        assert_eq!(staged.credit_rows(), twice.credit_rows());
    }

    /// Ferry Lights' younger copy (record 11) on disk, its files naming the release its older copy (10) holds by a certain match
    /// (G1: certain before the files).
    const FERRY_FILES: &str = "
        INSERT INTO files (folder, rel, size, mtime, state, format, bitrate, duration_ms, track, bits, rate, came) VALUES
          (0, 'Kite Theory/Ferry Lights (2016)/01 Pier.flac', 24000000, 1760000000, 1, 'FLAC', 900, 200000, 40, 16, 44100, 1760000000),
          (0, 'Kite Theory/Ferry Lights (2016)/02 Wake.flac', 24000000, 1760000000, 1, 'FLAC', 900, 200000, 41, 16, 44100, 1760000000);
        INSERT INTO tags (folder, rel, n, raw, name, value) SELECT folder, rel, 0, 'MUSICBRAINZ_ALBUMID', 'musicbrainz_albumid', 'abcdef00-0000-4000-8000-000000000020' FROM files WHERE rel LIKE 'Kite Theory/Ferry Lights (2016)/%';
        INSERT INTO tags (folder, rel, n, raw, name, value) SELECT folder, rel, 1, 'ALBUM', 'album', 'Ferry Lights' FROM files WHERE rel LIKE 'Kite Theory/Ferry Lights (2016)/%';
        INSERT INTO tags (folder, rel, n, raw, name, value) SELECT folder, rel, 2, 'ALBUMARTIST', 'albumartist', 'Kite Theory' FROM files WHERE rel LIKE 'Kite Theory/Ferry Lights (2016)/%';";

    /// P3.4 (W3): the part writes credits and nothing else: every track's record, the records (made, retired, merged, counted:
    /// all but the names its files give them), the files, every reference not the files', the records not had's listing credits
    /// and the requests' records as the step's other parts left them; a release a certain match holds stays its record's (G1);
    /// and a record had keeps a listing credit only where its files credit nobody (R17). Never `assign_records`.
    #[test]
    fn the_credits_part_writes_credits_alone() {
        let data = Data::tagged("credits-alone");
        data.batch("catalogue.sqlite", FERRY_FILES);
        let (cat, ud, kinds) = data.open_both();
        run_without_credits(&cat, &ud);
        let ferry = || cat.subject_of(&Ref::new("musicbrainz", "release", FERRY)).unwrap();
        assert_eq!(ferry(), ["al-10"], "held by the certain match, record 11's files naming it too");
        let snapshot = || {
            [
                data.rows_of("catalogue.sqlite", "SELECT id, record, retired_at FROM tracks ORDER BY id"),
                data.rows_of(
                    "catalogue.sqlite",
                    "SELECT id, born, retired_at, merged_into, year, kind, counting, counted_by_hand, checked_by_hand, seen_at, opened_at FROM records ORDER BY id",
                ),
                data.rows_of("catalogue.sqlite", "SELECT * FROM files ORDER BY folder, rel"),
                data.rows_of("catalogue.sqlite", "SELECT * FROM \"references\" WHERE how <> 'files' ORDER BY subject, source, kind, value"),
                data.rows_of(
                    "catalogue.sqlite",
                    "SELECT * FROM record_artists WHERE how = 'listing' AND record NOT IN (SELECT record FROM tracks WHERE record IS NOT NULL) ORDER BY record, artist",
                ),
                data.rows_of("userdata.sqlite", "SELECT id, record, status FROM request ORDER BY id"),
            ]
        };
        let before = snapshot();
        assert!(data.sql::<i64>("catalogue.sqlite", "SELECT count(*) FROM records WHERE retired_at IS NOT NULL") > 0, "a retired record");
        assert!(!before[4].is_empty(), "records not had with listing credits");
        assert!(data.sql::<i64>("userdata.sqlite", "SELECT count(*) FROM request WHERE status = 'approved'") > 0, "open requests");
        credits(&cat, &kinds, &FolderRules::default(), &|| Ok(())).unwrap();
        assert!(cat.part_done_017(CREDITS_017).unwrap() && !data.files_credits().is_empty(), "the part ran");
        let after = snapshot();
        for (n, what) in ["tracks", "records", "files", "references not the files'", "listing credits of records not had", "requests"].iter().enumerate() {
            assert_eq!(after[n], before[n], "{what} as they were");
        }
        assert_eq!(ferry(), ["al-10"], "the certain match keeps its release");
        let credited: std::collections::HashSet<i64> = data.files_credits().iter().map(|(record, ..)| *record).collect();
        for record in data.rows_of("catalogue.sqlite", "SELECT record FROM record_artists WHERE how = 'listing' AND record IN (SELECT record FROM tracks) ORDER BY record") {
            assert!(!credited.iter().any(|c| format!("Integer({c})") == record), "R17: a listing credit stays only where the files credit nobody: {record}");
        }
        let part = [body(include_str!("step_017.rs"), "pub(crate) fn credits("), body(include_str!("../../../store/src/catalogue/records.rs"), "pub fn credit_files_017(")];
        assert!(part.iter().all(|code| !code.contains("assign_records") && !code.contains("Self::assign(")), "the part gives no record");
    }

    /// P3.5 (W3, M1): the part reads each folder's rows once (`rows_to_credit`) and no statement it runs names `analysis_shape`:
    /// with the table set aside, it runs whole. The code says so too: its rows are read without the albums' gains, which the
    /// shape gives (`served_rows`' `album_gains_in`), and no question is asked per album.
    #[test]
    fn the_credits_part_reads_no_shape() {
        let data = Data::tagged("credits-no-shape");
        let (cat, ud, kinds) = data.open_both();
        run_without_credits(&cat, &ud);
        data.batch("catalogue.sqlite", "ALTER TABLE analysis_shape RENAME TO shape_set_aside");
        credits(&cat, &kinds, &FolderRules::default(), &|| Ok(())).expect("no statement names the shape");
        assert!(!data.files_credits().is_empty());
        let catalogue = include_str!("../../../store/src/catalogue/mod.rs");
        let reader = body(catalogue, "pub fn rows_to_credit(");
        assert!(reader.contains("PRESENT, false)"), "read without the albums' gains: {reader}");
        let part = body(include_str!("step_017.rs"), "pub(crate) fn credits(");
        assert_eq!(part.matches("rows_to_credit(").count(), 1, "one read a folder: {part}");
        for code in [part, reader, body(include_str!("../../../store/src/catalogue/records.rs"), "pub fn credit_files_017(")] {
            for named in ["analysis_shape", "SHAPE_JOIN", "album_gains_in", "served_rows", "keep_files_names("] {
                assert!(!code.contains(named), "{named} in {code}");
            }
        }
    }

    /// The catalogue's every table, row by row: what a step that stops before its parts leaves as it was.
    fn catalogue_rows(data: &Data) -> Vec<String> {
        let tables = data.texts("catalogue.sqlite", "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name");
        tables.iter().flat_map(|t| data.rows_of("catalogue.sqlite", &format!("SELECT * FROM \"{t}\"")).into_iter().map(move |r| format!("{t}: {r}"))).collect()
    }

    /// P5.1 (W5): what 0.16 kept unreadable (a table of it gone), the step stops saying so, outside its parts and to restore: no
    /// part's mark written, and the catalogue as it was.
    #[test]
    fn a_failure_reading_what_016_kept_says_restore() {
        let data = Data::new("kept-unread");
        let (cat, ud) = data.open();
        data.batch("userdata.sqlite", "DROP TABLE album_matches");
        let before = catalogue_rows(&data);
        let err = format!("{:#}", run(&cat, &ud).unwrap_err());
        assert!(err.contains("0.17's step, reading what 0.16 kept"), "{err}");
        assert!(err.contains("outside its parts (no mark to set: restore)"), "{err}");
        assert!(data.marks().is_empty(), "no mark: {:?}", data.marks());
        assert_eq!(cat.references_to_carry().unwrap(), PARTS_017, "every part still to run");
        assert_eq!(catalogue_rows(&data), before, "the catalogue as it was");
    }

    /// P5.2 (W5): a part that cannot begin (another writer holds the catalogue past its wait) names itself; the writer gone, the
    /// step runs whole.
    #[test]
    fn a_part_that_cannot_begin_names_itself() {
        let data = Data::new("cannot-begin");
        let (cat, ud) = data.open();
        let writer = Connection::open(data.0.join("catalogue.sqlite")).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        cat.writer_for_tests().busy_timeout(std::time::Duration::from_millis(200)).unwrap();
        let err = format!("{:#}", run(&cat, &ud).unwrap_err());
        assert!(err.contains("0.17's step, part hand"), "{err}");
        assert!(data.marks().is_empty(), "nothing done: {:?}", data.marks());
        writer.execute_batch("ROLLBACK").unwrap();
        run(&cat, &ud).unwrap();
        assert_eq!(data.marks().len(), PARTS_017.len() + 3, "the writer gone, every part (and the band waves', the credits' and the fold's)");
    }

    /// P5.3 (W5): a failure after the parts, the report not written (the user data gone read-only) or the requests not given
    /// their records, says so, outside its parts and to restore; every part's mark stays, and the next start, the user data
    /// writable, runs no part again and writes the report an uncut step writes.
    #[test]
    fn a_failure_after_the_parts_says_restore_and_keeps_the_parts() {
        let report = "SELECT report FROM migration_report WHERE part = 'references'";
        let uncut = Data::new("after-parts-uncut");
        {
            let (cat, ud) = uncut.open();
            run_with(&cat, &ud, &|_| Ok(())).unwrap();
        }
        let mut every: Vec<String> = PARTS_017.iter().map(|p| p.to_string()).collect();
        every.sort();
        for (case, doing) in [("read-only", "writing its report"), ("records", "giving the requests their records")] {
            let data = Data::new(&format!("after-parts-{case}"));
            {
                let (cat, ud) = data.open();
                if case == "records" {
                    data.batch("userdata.sqlite", "CREATE TRIGGER broken_record BEFORE UPDATE OF record ON request BEGIN SELECT RAISE(ABORT, 'broken by the test'); END;");
                }
                let err = run_with(&cat, &ud, &|at| {
                    if at == "catalogue" && case == "read-only" {
                        ud.read_only_for_tests();
                    }
                    Ok(())
                })
                .unwrap_err();
                let err = format!("{err:#}");
                assert!(err.contains(&format!("0.17's step, {doing}")), "{case}: {err}");
                assert!(err.contains("outside its parts (no mark to set: restore)"), "{case}: {err}");
            }
            assert_eq!(data.marks(), every, "{case}: every part's mark stays");
            assert!(data.rows_of("userdata.sqlite", report).is_empty(), "{case}: no report yet");
            data.batch("userdata.sqlite", "DROP TRIGGER IF EXISTS broken_record");
            let (cat, ud) = data.open();
            let lines = crate::diagnostics::logged_lines(|| run_with(&cat, &ud, &|_| Ok(())).unwrap());
            assert!(parts_said(&lines).is_empty(), "{case}: no part again: {lines:#?}");
            assert_eq!(data.marks(), every, "{case}");
            assert_eq!(data.rows_of("userdata.sqlite", report), uncut.rows_of("userdata.sqlite", report), "{case}: the report written");
            assert_eq!(data.sql::<i64>("userdata.sqlite", "SELECT count(*) FROM request WHERE record IS NULL"), 0, "{case}: every request its record");
        }
    }

    /// X7: a request the step closes as asked twice (an account's second open request of one record, W6) is listed in its report:
    /// the request, the one kept, their record.
    #[test]
    fn the_step_reports_each_request_closed_as_asked_twice() {
        let data = Data::new("asked-twice");
        data.batch("userdata.sqlite", "INSERT INTO request (id, user, album_key, artist_key, album, artist, year, at, status) VALUES (14, 'bob', 'orbitlane|quietlanes2', 'orbit lane', 'Quiet Lanes', 'Orbit Lane', 2017, 1793950900, 'approved')");
        data.run();
        let record = data.sql::<i64>("userdata.sqlite", "SELECT record FROM request WHERE id = 8");
        assert_eq!(data.sql::<String>("userdata.sqlite", "SELECT status || ' ' || reason FROM request WHERE id = 14"), "cancelled Asked twice");
        let (_, ud) = data.open();
        let report = &ud.migration_report().unwrap()[PART].1;
        assert_eq!(report[ASKED_TWICE], serde_json::json!([{ "request": 14, "kept": 8, "record": record }]), "{report}");
    }

    /// R14 (F3): two requests of one album with no release group share one record not had: the second finds what the first made.
    #[test]
    fn two_requests_of_one_album_without_a_group_share_a_record() {
        let data = Data::new("requests-share");
        data.batch("userdata.sqlite", "INSERT INTO request (id, user, album_key, artist_key, album, artist, year, at, status) VALUES (14, 'ann', 'orbitlane|quietlanes', 'orbit lane', 'Quiet Lanes', 'Orbit Lane', 2017, 1793950900, 'approved')");
        data.run();
        let record = |id: i64| data.sql::<i64>("userdata.sqlite", &format!("SELECT record FROM request WHERE id = {id}"));
        assert_eq!(record(14), record(8));
        assert_eq!(data.sql::<i64>("catalogue.sqlite", "SELECT count(*) FROM records WHERE title = 'Quiet Lanes'"), 1);
    }

    /// C51b, R29 (N12): 0.16's word that it could not hand a request to Soulseek (FX01's 7, 8, 11 and 12) comes off, once, under its
    /// mark, and nothing is claimed in its place: the page no longer shows them as not handed over, and Soulseek's own row says what
    /// it does; a word written since stays.
    #[test]
    fn a_request_0_16_could_not_hand_to_soulseek_loses_that_word() {
        let data = Data::new("soulseek-said");
        let said = |sql: &str| data.sql::<i64>("userdata.sqlite", &format!("SELECT count(*) FROM request WHERE {sql}"));
        assert_eq!(said("fetched LIKE 'Not handed over to Soulseek%'"), 4, "FX01 as 0.16 left it");
        data.run();
        assert_eq!(said("fetched LIKE '%Not handed over to Soulseek%'"), 0);
        assert_eq!(said("id IN (7, 8, 11, 12) AND fetched IS NULL"), 4, "nothing in its place");
        assert_eq!(said("id = 1 AND fetched = 'Soulseek searches for it'"), 1, "another word, as it was");
        data.batch("userdata.sqlite", "UPDATE request SET fetched = 'Not handed over to Soulseek: no release group to judge it by' WHERE id = 7");
        data.run();
        assert_eq!(said("id = 7 AND fetched IS NOT NULL"), 1, "once");
    }

    /// R29 (N12): the requests' repairs at a start that fail (0.16's word taken off, a record given again) are said in the log, and
    /// the start goes on; each is tried again at the next.
    #[test]
    fn a_failed_repair_of_requests_does_not_stop_the_start() {
        let data = Data::new("repair-fails");
        data.batch("userdata.sqlite", "CREATE TRIGGER broken_word BEFORE UPDATE OF fetched ON request BEGIN SELECT RAISE(ABORT, 'broken by the test'); END;");
        data.run();
        data.batch(
            "userdata.sqlite",
            "UPDATE request SET record = 99999 WHERE id = 6;
             CREATE TRIGGER broken_record BEFORE UPDATE OF record ON request WHEN NEW.record <> 99999 BEGIN SELECT RAISE(ABORT, 'broken by the test'); END;",
        );
        data.run();
        assert_eq!(data.sql::<i64>("userdata.sqlite", "SELECT record FROM request WHERE id = 6"), 99999, "left for the next start");
        assert_eq!(data.sql::<i64>("userdata.sqlite", "SELECT count(*) FROM request WHERE fetched LIKE 'Not handed over to Soulseek%'"), 4, "and the word");
    }

    /// C49a: a request with no release group and no record of its names gets a new record not had, credited to its artist; one
    /// a label's account named "Artist - Title (Label)" (S12, the owner's case's shape) gets the artist and title from its
    /// names, the label credited, and the Bandcamp page `bandcamp_found` kept by its names as its record's reference.
    #[test]
    fn a_request_with_no_release_group_gets_a_record() {
        let data = Data::new("request-new");
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        let record = |id: i64| ud.request(id).unwrap().unwrap().record.unwrap();
        let lanes = cat.record(record(8)).unwrap().unwrap();
        assert_eq!((lanes.title.as_str(), lanes.artist.as_str(), lanes.year, lanes.had), ("Quiet Lanes", "Orbit Lane", Some(2017), false));
        assert_eq!(cat.record_artists(lanes.id).unwrap(), [cat.artist_of("Orbit Lane").unwrap().unwrap()]);
        let hours = cat.record(record(11)).unwrap().unwrap();
        assert_eq!((hours.title.as_str(), hours.artist.as_str(), hours.had), ("After Hours", "Tape Club", false), "named by its title");
        assert_eq!(cat.record_artists(hours.id).unwrap(), [cat.artist_of("Tape Club").unwrap().unwrap()]);
        assert!(cat.label_records(cat.label_for("Paper Lantern").unwrap()).unwrap().contains(&(hours.id, String::new())), "the account its label (and Long Player's, FX01's 12)");
        assert!(refs(&cat, hours.id).contains(&by("bandcamp", "album", "https://paperlantern.bandcamp.com/album/after-hours", "search", false)), "{:?}", refs(&cat, hours.id));
        let report = &ud.migration_report().unwrap()[PART].1;
        assert_eq!((report["records_made_for_requests"].as_u64(), report["pages_for_requests"].as_u64()), (Some(3), Some(1)), "Quiet Lanes, Nowhere Fast, After Hours: {report}");
        assert_eq!(report["records_made_for"], serde_json::json!(["8 Orbit Lane - Quiet Lanes", "9 Tape Club - Nowhere Fast", "11 Tape Club - After Hours"]), "each, whole");
        assert_eq!(report["names_shared_by_several_records"], serde_json::json!([]), "X9: FX01's two copies of Two Rooms are one album, no clash");
        let by_names: Vec<&str> = report["requests_found_by_names"].as_array().unwrap().iter().filter_map(serde_json::Value::as_str).collect();
        assert!(by_names.iter().any(|r| r.starts_with("7 Noor Field - Two Rooms -> ")), "R21: each found by names, listed: {by_names:?}");
        assert!(by_names.iter().any(|r| r.starts_with("12 Paper Lantern - Tape Club - Long Player (Paper Lantern) -> ")), "{by_names:?}");
    }

    /// R5: an id in capitals is the same id: Ferry Lights' two matches name one release, one in capitals, and the older copy
    /// keeps it; Undertow's page, kept by its release group in capitals, is its.
    #[test]
    fn an_id_in_capitals_is_the_same_id() {
        let data = Data::new("capitals");
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        assert_eq!(cat.subject_of(&Ref::new("musicbrainz", "release", FERRY)).unwrap(), ["al-10"]);
        assert_eq!(cat.subject_of(&Ref::new("bandcamp", "album", "https://orbitlane.bandcamp.com/album/undertow")).unwrap(), ["al-12"]);
        let report = &ud.migration_report().unwrap()[PART].1;
        assert!(listed(report, "bandcamp_found").1.is_empty(), "a request's page, carried by the requests part, is no page left over (R19): {report}");
    }

    /// R5: a row keeps when it was first found: a match's, a page's and a link's as 0.16 found them, a hand's when the step
    /// first ran; found again (the step run again, its marks gone), each keeps its first `at`.
    #[test]
    fn a_row_keeps_its_at() {
        let data = Data::new("at");
        data.run();
        let at = |subject: &str, source: &str, kind: &str| -> i64 {
            data.sql("catalogue.sqlite", &format!("SELECT at FROM \"references\" WHERE subject = '{subject}' AND source = '{source}' AND kind = '{kind}'"))
        };
        assert_eq!(at("al-7", "musicbrainz", "release"), 1794300000, "Signal's certain match, when it was found");
        assert_eq!(at("al-1", "bandcamp", "album"), 1793900000, "the page, when it was found");
        assert_eq!(at("al-1", "discogs", "release"), 1793900000, "MusicBrainz's link, when it was read");
        data.batch("catalogue.sqlite", "UPDATE \"references\" SET at = 1 WHERE how = 'hand'; DELETE FROM meta WHERE key GLOB '017:*'");
        data.run();
        assert_eq!(at("al-2", "discogs", "release"), 1, "a hand's, found again: as first found");
        assert_eq!(at("al-7", "musicbrainz", "release"), 1794300000);
    }

    /// R5 and R10, measuring: the step over a catalogue as large as production's (5,000 records of 8 tracks by 700 artists,
    /// 40,000 files with ten tags each, 400,000 tags), with a match for 3,000 records, two in three certain, 2,000 each of
    /// Discogs' facts, Bandcamp's pages and MusicBrainz's links, and 700 discographies of 20 albums each (production kept 688),
    /// a third of them records had; then what a build does with the albums each time (`album_copies`, `keep_files_names`). Run
    /// in release by hand: `rt.sh release --ignored --nocapture the_step_at_production_size`.
    #[test]
    #[ignore]
    fn the_step_at_production_size() {
        let d = std::env::temp_dir().join(format!("cratarr-step-017-size-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        let (records, tracks, matches, others) = (5_000_i64, 8_i64, 3_000_i64, 2_000_i64);
        let id = |kind: u32, n: i64| format!("{kind:08x}-0000-4000-8000-{n:012}");
        let cat = Catalogue::open(Some(&d.join("catalogue.sqlite")), vec![PathBuf::from("/music")]).unwrap();
        let ud = UserData::open(Some(&d.join("userdata.sqlite"))).unwrap();
        let made = std::time::Instant::now();
        {
            let mut db = Connection::open(d.join("catalogue.sqlite")).unwrap();
            let tx = db.transaction().unwrap();
            tx.execute("INSERT OR IGNORE INTO folders (id, path, home) VALUES (0, '/music', 0)", []).unwrap();
            for r in 1..=records {
                tx.execute("INSERT INTO records (id, born, title, artist) VALUES (?1, 1760000000, ?2, ?3)", rusqlite::params![r, format!("Record {r}"), format!("Artist {}", r % 700)]).unwrap();
                for n in 1..=tracks {
                    let track = (r - 1) * tracks + n;
                    let rel = format!("Artist {}/Record {r}/{n:02} Track.flac", r % 700);
                    tx.execute("INSERT INTO tracks (id, born, record, disc, number) VALUES (?1, 1760000000, ?2, 1, ?3)", rusqlite::params![track, r, n]).unwrap();
                    tx.execute(
                        "INSERT INTO files (folder, rel, size, mtime, state, format, bitrate, duration_ms, track, bits, rate, came) VALUES (0, ?1, 24000000, 1760000000, 1, 'FLAC', 900, 200000, ?2, 16, 44100, 1760000000)",
                        rusqlite::params![rel, track],
                    )
                    .unwrap();
                    let tags = [
                        ("title", format!("Track {n}")),
                        ("artist", format!("Artist {}", r % 700)),
                        ("album", format!("Record {r}")),
                        ("albumartist", format!("Artist {}", r % 700)),
                        ("tracknumber", n.to_string()),
                        ("discnumber", "1".to_string()),
                        ("musicbrainz_albumartistid", id(3, r % 700)),
                        ("genre", "Electronic".to_string()),
                        ("musicbrainz_albumid", id(1, r)),
                        ("musicbrainz_releasegroupid", id(2, r)),
                    ];
                    for (i, (name, value)) in tags.iter().enumerate() {
                        tx.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, ?1, ?2, ?3, ?3, ?4)", rusqlite::params![rel, i as i64, name, value]).unwrap();
                    }
                }
            }
            tx.commit().unwrap();
            let mut db = Connection::open(d.join("userdata.sqlite")).unwrap();
            let tx = db.transaction().unwrap();
            for r in 1..=matches {
                let found = format!(
                    r#"{{"release":"{}","release_group":"{}","title":"Record {r}","artist":"Artist","artist_id":"{}","year":2012,"certain":{},"why":"","tracks":[],"lengths":[]}}"#,
                    id(1, r),
                    id(2, r),
                    id(3, r % 700),
                    r % 3 != 0
                );
                tx.execute("INSERT INTO album_matches (album, at, found) VALUES (?1, 1793900000, ?2)", rusqlite::params![format!("al-{r}"), found]).unwrap();
            }
            for r in 1..=others {
                tx.execute("INSERT INTO discogs_facts (subject, at, facts) VALUES (?1, 1793900000, ?2)", rusqlite::params![format!("album:al-{r}"), format!(r#"{{"master":{},"release":{},"genres":[],"styles":[],"labels":[],"credits":[]}}"#, 1_000_000 + r, 2_000_000 + r)]).unwrap();
                tx.execute("INSERT INTO bandcamp_found (album, at, page) VALUES (?1, 1793900000, ?2)", rusqlite::params![id(2, r + 1_000), format!("https://artist{r}.bandcamp.com/album/record-{r}")]).unwrap();
                tx.execute(
                    "INSERT INTO mb_releases (release, found, facts, at) VALUES (?1, 1, ?2, 1793900000)",
                    rusqlite::params![id(1, r + 2_000), format!(r#"{{"genres":[],"discogs":["release:{}","master:{}"],"performers":[],"performed":false,"tracks":[],"with_parents":true}}"#, 3_000_000 + r, 4_000_000 + r)],
                )
                .unwrap();
            }
            for r in 1..=500 {
                tx.execute("INSERT INTO record_references (album, chosen) VALUES (?1, ?2)", rusqlite::params![format!("al-{}", r * 7), r#"{"counting":"musicbrainz","by_hand":false,"links":{},"confirmed":["musicbrainz"]}"#]).unwrap();
            }
            for a in 0..700_i64 {
                // Each artist's records here first (their release groups), then albums the library has not.
                let had = (1..=records).filter(|r| r % 700 == a).map(|r| id(2, r));
                let groups: Vec<serde_json::Value> = had
                    .chain((0..20).map(|k| id(4, a * 20 + k)))
                    .take(20)
                    .enumerate()
                    .map(|(k, g)| serde_json::json!({"id": g, "title": format!("Album {a}-{k}"), "kind": "Album", "year": 2000 + k as i32, "genres": ["ambient"], "date": format!("{}-01-01", 2000 + k)}))
                    .collect();
                tx.execute("INSERT INTO discography (artist, at, groups) VALUES (?1, 1793900000, ?2)", rusqlite::params![id(3, a), serde_json::to_string(&groups).unwrap()]).unwrap();
            }
            tx.commit().unwrap();
        }
        let made = made.elapsed();
        let step = std::time::Instant::now();
        run_without_credits(&cat, &ud);
        let step = step.elapsed();
        let part = std::time::Instant::now();
        credits(&cat, &[], &FolderRules::default(), &|| Ok(())).unwrap();
        let part = part.elapsed();
        let folding = std::time::Instant::now();
        fold(&cat, &ud, &[], &|| Ok(())).unwrap();
        let folding = folding.elapsed();
        assert!(cat.part_done_017(CREDITS_017).unwrap() && cat.part_done_017(FOLD_017).unwrap());
        assert!(cat.references_to_carry().unwrap().is_empty());
        let references: i64 = Connection::open(d.join("catalogue.sqlite")).unwrap().query_row("SELECT count(*) FROM \"references\"", [], |r| r.get(0)).unwrap();
        let report = cat.references_carried().unwrap().unwrap();
        let copies_at = std::time::Instant::now();
        let folders: Vec<crate::serve::library::FolderRows> =
            cat.folders().iter().map(|(folder, root)| crate::serve::library::FolderRows { folder, root: root.display().to_string(), rows: cat.served_rows(folder).unwrap() }).collect();
        let copies = crate::serve::library::album_copies(&folders, &crate::catalogue::FolderRules::default());
        let copies_took = copies_at.elapsed();
        let build = std::time::Instant::now();
        cat.keep_files_names(&copies).unwrap();
        let build = build.elapsed();
        let again = std::time::Instant::now();
        cat.keep_files_names(&copies).unwrap();
        let again = again.elapsed();
        eprintln!(
            "0.17's step at production size: {step:?} for {references} references, {} records not had, {} found by their groups (made in {made:?}); its part credits: {part:?}; its part fold: {folding:?}; a build's albums: {} copies in {copies_took:?}; keep_files_names after the part: {build:?}, {again:?} with nothing changed",
            report["records_not_had"], report["records_found"], copies.len()
        );
        std::fs::remove_dir_all(&d).ok();
    }

    /// R2: every row the step reads is in its report: the rows of each table it reads (counted here, apart from the step),
    /// each carried or in a list saying why not; so too each link pasted and each source confirmed, of the records there are.
    #[test]
    fn every_row_the_step_reads_is_in_its_report() {
        let data = Data::new("every-row");
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        let report = &ud.migration_report().unwrap()[PART].1;
        let lists = |name: &str| -> u64 {
            report["unmatched"].as_array().unwrap().iter().filter(|u| u["what"].as_str().is_some_and(|w| w == name || w.starts_with(&format!("{name}: ")))).map(|u| u["count"].as_u64().unwrap()).sum()
        };
        for (name, sql) in [
            ("record_references", "SELECT count(*) FROM record_references"),
            ("album_matches", "SELECT count(*) FROM album_matches"),
            ("discogs_facts: albums", "SELECT count(*) FROM discogs_facts WHERE subject LIKE 'album:%'"),
            ("bandcamp_found", "SELECT count(*) FROM bandcamp_found"),
            ("mb_releases", "SELECT count(*) FROM mb_releases WHERE found <> 0"),
            ("discography", "SELECT count(*) FROM discography"),
            ("request", "SELECT count(*) FROM request"),
        ] {
            let rows = data.sql::<i64>("userdata.sqlite", sql) as u64;
            let (before, moved) = table(report, name);
            assert_eq!(before, rows, "{name}: every row read");
            assert_eq!(moved + lists(name), rows, "{name}: each carried or listed: {report}");
        }
        let records = data.texts("catalogue.sqlite", "SELECT 'al-' || id FROM records");
        let (mut links, mut confirmations) = (0, 0);
        for row in data.texts("userdata.sqlite", "SELECT album || ' ' || chosen FROM record_references") {
            let (album, json) = row.split_once(' ').unwrap();
            let (true, Ok(chosen)) = (records.iter().any(|r| r == album), serde_json::from_str::<serde_json::Value>(json)) else { continue };
            links += chosen["links"].as_object().map_or(0, |l| l.len()) as u64;
            confirmations += chosen["confirmed"].as_array().map_or(0, |c| c.len()) as u64;
        }
        for (name, n) in [("links pasted", links), ("confirmations", confirmations)] {
            let (before, moved) = table(report, name);
            assert_eq!(before, n, "{name}: each read");
            assert_eq!(moved + lists(name), n, "{name}: each carried or listed: {report}");
        }
        // FX01's: a link 0.16 kept that names nothing (a release's id cut short), a source confirmed of which the record holds
        // nothing, a page that is an artist's, a release MusicBrainz links to nothing on Discogs; and Discogs' old link to a
        // master, which names it.
        assert_eq!(listed(report, "links pasted: naming no reference").1, [format!("al-5 https://musicbrainz.org/release/{}", &mb(5)[..35])]);
        assert_eq!(listed(report, "confirmations: no reference").1, ["al-5 bandcamp"]);
        assert_eq!(listed(report, "bandcamp_found: no album page").1, ["noorfield|signal"]);
        assert_eq!(listed(report, "mb_releases: no Discogs link").1, [mb(7)]);
        assert_eq!(listed(report, "mb_releases").1, [mb(98)], "no record is it");
        for table in ["record_references", "album_matches", "mb_releases"] {
            assert_eq!(listed(report, &format!("{table}: not read")).0, 1, "{table}: {report}");
        }
        assert!(refs(&cat, 5).contains(&by("discogs", "master", "1000005", "hand", false)), "{:?}", refs(&cat, 5));
    }

    /// C48b: each artist's albums and EPs as MusicBrainz listed them are records: one a record holds already found by its
    /// release group, the rest records not had, credited to the artist, who is named by their files, a related artist's name or
    /// a match's; a Bandcamp page kept by a group, the record of it now.
    #[test]
    fn a_discography_becomes_records_not_had() {
        let data = Data::new("discography");
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        let of = |n: u32| -> Vec<i64> { cat.subject_of(&Ref::new("musicbrainz", "release-group", &mb(n))).unwrap().iter().filter_map(|s| crate::serve::ids::record_number(s)).collect() };
        assert_eq!((of(101), of(102), of(103)), (vec![1], vec![2, 9], vec![3]), "a group a record holds is that record");
        let [lights] = of(111)[..] else { panic!("{:?}", of(111)) };
        let lights = cat.record(lights).unwrap().unwrap();
        assert_eq!(
            (lights.title.as_str(), lights.artist.as_str(), lights.year, lights.kind.as_deref(), lights.released.as_deref(), lights.genres.as_slice(), lights.had),
            ("Harbour Lights", "Glass Harbour", Some(2014), Some("ep"), Some("2014-06"), &["ambient".to_string()][..], false)
        );
        let artist = |name: &str| cat.artist_of(name).unwrap().unwrap();
        for (n, name) in [(111, "Glass Harbour"), (112, "Glass Harbour"), (113, "Kite Theory"), (114, "Noor Field"), (115, "Tape Club"), (116, "Tape Club")] {
            let [record] = of(n)[..] else { panic!("{n}: {:?}", of(n)) };
            assert!(!cat.record(record).unwrap().unwrap().had, "{n}");
            assert_eq!(cat.record_artists(record).unwrap(), [artist(name)], "{n}");
        }
        assert!(cat.references_of(&crate::serve::ids::record(of(111)[0])).unwrap().iter().any(|(r, row)| r.source == "bandcamp" && row.how == "search"), "Harbour Lights' page, kept by its group");
        let named = |name: &str| cat.references_of(&crate::serve::ids::artist(artist(name))).unwrap().into_iter().map(|(r, row)| (r.value, row.how, row.read_at)).collect::<Vec<_>>();
        assert_eq!(named("Glass Harbour"), [(mb(201), "files".to_string(), Some(1793900000))], "as their files call them, read when the discography was");
        assert_eq!(named("Tape Club"), [(mb(204), "link".to_string(), Some(1793950000))], "a related artist's name");
        assert_eq!(named("Noor Field"), [(mb(202), "search".to_string(), Some(1793900000))], "a match's credit");
        let report = &ud.migration_report().unwrap()[PART].1;
        assert_eq!((report["records_found"].as_u64(), report["records_not_had"].as_u64(), report["listed"].as_u64()), (Some(3), Some(7), Some(10)), "{report}");
    }

    /// C48b: read again (its mark gone), the discographies find the records they made, and make none: by their release groups.
    #[test]
    fn a_second_read_finds_the_same_records() {
        let data = Data::new("discography-again");
        let records = "SELECT id, title, artist, kind FROM records ORDER BY id";
        data.run();
        let (first, refs) = (data.rows_of("catalogue.sqlite", records), data.rows());
        assert!(first.len() > 12, "{first:?}");
        data.batch("catalogue.sqlite", "DELETE FROM meta WHERE key = '017:discography'");
        data.run();
        assert_eq!(data.rows_of("catalogue.sqlite", records), first);
        assert_eq!(data.rows(), refs);
    }

    fn refs(cat: &Catalogue, record: i64) -> Vec<(String, String, String, String, bool)> {
        cat.references_of(&crate::serve::ids::record(record)).unwrap().into_iter().map(|(r, row)| (r.source, r.kind, r.value, row.how, row.confirmed)).collect()
    }

    fn by(source: &str, kind: &str, value: &str, how: &str, confirmed: bool) -> (String, String, String, String, bool) {
        (source.into(), kind.into(), value.into(), how.into(), confirmed)
    }

    /// R3: a certain match from 0.16 counts nothing until its list is read and agrees with the files: the step claims no count,
    /// and the record waits off Health's references item meanwhile. Read, Signal's list agrees and MusicBrainz counts; Undertow's
    /// names each track "(Remastered)", which its files do not: nothing counts, and Health lists it. Ferry Lights' release is
    /// no more (none read): nothing counts. Each waits no more.
    #[tokio::test]
    async fn a_certain_match_counts_nothing_until_its_list_agrees() {
        let data = Data::new("waits");
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        let counting = |cat: &Catalogue, record: i64| cat.record(record).unwrap().unwrap().counting;
        assert_eq!((counting(&cat, 7), counting(&cat, 12)), (None, None), "no count from 0.16's certain matches");
        assert_eq!(cat.lists_to_read(100).unwrap(), [(1, mb(1)), (7, mb(7)), (10, FERRY.to_string()), (12, mb(30))], "Two Rooms' is a hand's, and Ferry Lights' younger copy holds none");
        assert!(!cat.unreferenced(true, &[]).unwrap().contains(&12), "Undertow waits for its list");

        let cat = std::sync::Arc::new(cat);
        let titled = |titles: &[&str], ms: i64| -> Vec<crate::catalogue::identity::Listed> { titles.iter().map(|t| crate::catalogue::identity::Listed::of(t, ms)).collect() };
        let undertow = ["Ebb Line", "Slack", "Undertow"];
        let files: std::sync::Arc<dyn Fn(i64) -> anyhow::Result<Vec<crate::catalogue::identity::Listed>> + Send + Sync> = std::sync::Arc::new(move |record| {
            Ok(match record {
                7 => titled(&["Signal"], 180_000),
                12 => titled(&undertow, 200_000),
                _ => Vec::new(),
            })
        });
        let list = |titles: Vec<String>, ms: i64| -> Vec<crate::catalogue::identity::RefTrack> {
            titles.into_iter().zip(1..).map(|(title, position)| crate::catalogue::identity::RefTrack { disc: 1, position, title, length_ms: Some(ms), recording: None }).collect()
        };
        let read = |release: String| {
            let read = if release == mb(7) {
                Some(list(vec!["Signal".into()], 180_500))
            } else if release == mb(30) {
                Some(list(undertow.iter().map(|t| format!("{t} (Remastered)")).collect(), 200_000))
            } else if release == mb(1) {
                Some(list(["Slack Water", "Neap", "Spring Tide", "Ebb", "Flood", "Low Water"].iter().map(|t| t.to_string()).collect(), 190_000))
            } else {
                None
            };
            async move { Ok(read) }
        };
        assert_eq!(crate::app::references::read_lists(&cat, files.clone(), read, 100).await.unwrap(), 4);
        assert_eq!(counting(&cat, 7), None, "VR10, changed by design: Signal's list kept, nothing counted");
        assert_eq!(counting(&cat, 12), None, "Undertow's does not");
        assert_eq!(counting(&cat, 10), None, "no list");
        assert!(cat.unreferenced(true, &[]).unwrap().contains(&12), "on Health's item now");
        assert!(cat.lists_to_read(100).unwrap().is_empty());
        assert_eq!(crate::app::references::read_lists(&cat, files, |_| async { Ok(None) }, 100).await.unwrap(), 0, "nothing waits");
        let report = &ud.migration_report().unwrap()[PART].1;
        assert_eq!(report["lists_to_read"].as_u64(), Some(4), "{report}");
    }

    /// What 0.14 chose stands after the step: a source chosen, a link pasted, checked by hand, a guess confirmed; a retired
    /// record's too. Nothing counts that was not chosen (R3). Who keeps one release two records' files name.
    #[test]
    fn a_chosen_source_survives_the_step() {
        let data = Data::new("chosen");
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        let counted = |record: i64| cat.record(record).unwrap().map(|r| (r.counting, r.counted_by_hand, r.checked_by_hand, r.kind)).unwrap();
        let chosen = |src: &str| Some(src.to_string());

        // Tide Tables: "Use this" on MusicBrainz, confirmed; its certain match is the release its files name; Discogs by its
        // facts, made links by MusicBrainz's; its Bandcamp page by its release group.
        assert_eq!(counted(1), (chosen("musicbrainz"), true, false, None));
        assert_eq!(
            refs(&cat, 1),
            [
                by("bandcamp", "album", "https://glassharbour.bandcamp.com/album/tide-tables", "search", false),
                by("discogs", "master", "1000001", "link", false),
                by("discogs", "release", "2000001", "link", false),
                by("musicbrainz", "release", &mb(1), "files", true),
                by("musicbrainz", "release-group", &mb(101), "files", true),
            ]
        );
        // Two Rooms: a Discogs link pasted, counting, checked by hand; its files name their release.
        assert_eq!(counted(2), (chosen("discogs"), true, true, None));
        assert_eq!(refs(&cat, 2)[0], by("discogs", "release", "2000002", "hand", false));
        assert!(refs(&cat, 2).contains(&by("musicbrainz", "release", &mb(2), "hand", false)), "pasted, and its files name it too");
        // Night Walks EP: a Bandcamp guess confirmed, nothing chosen; a likely match the files agree with, Discogs by link.
        assert_eq!(counted(3), (None, false, false, None));
        assert!(refs(&cat, 3).contains(&by("bandcamp", "album", "https://kitetheory.bandcamp.com/album/night-walks", "search", true)));
        assert!(refs(&cat, 3).contains(&by("discogs", "release", "2000003", "link", false)));
        // Undated: its one file names Tide Tables' release (a tag copied in error), and Discogs' search by its names found Night
        // Walks': each stays where it is.
        assert!(refs(&cat, 4).is_empty());
        // Late Train OST: AcoustID counting, as chosen.
        assert_eq!(counted(5).0, chosen("acoustid"));
        // Partial Shore, retired: its choices all the same, and its master; no release (R1).
        assert_eq!(counted(6), (chosen("musicbrainz"), true, true, None));
        assert_eq!(refs(&cat, 6), [by("discogs", "master", "1000006", "search", true)], "Discogs confirmed in 0.14");
        // Signal: nothing chosen, a certain match over its one track: nothing counts until its list is read (R3).
        assert_eq!(counted(7), (None, false, false, None));
        assert!(cat.reference_tracks(7).unwrap().is_empty(), "no list: a match keeps none of MusicBrainz's titles");

        let report = &ud.migration_report().unwrap()[PART].1;
        assert_eq!(listed(report, "album_matches").0, 1, "{report}");
        assert!(listed(report, "bandcamp_found").1.is_empty(), "Harbour Lights' found its record not had (C48b); After Hours' is a request's, carried by the requests part (C49a, R19): {report}");
        assert_eq!(
            differed(report, "references: held by another record"),
            [
                "al-11 discogs:release:2000020".to_string(),
                format!("al-9 musicbrainz:release:{}", mb(2)),
                format!("al-11 musicbrainz:release:{FERRY}"),
                format!("al-4 musicbrainz:release:{}", mb(1)),
                "al-4 discogs:release:2000003".to_string()
            ],
            "Ferry Lights' younger copy's hand, Two Rooms' hand, Ferry Lights' older copy, Undated's tag and its search"
        );
        assert_eq!(differed(report, "references: a retired record's"), [format!("al-6 musicbrainz:release:{}", mb(6)), "al-6 discogs:release:2000006".to_string()]);
        assert_eq!((report["counted_by_hand"].as_u64(), report["checked_by_hand"].as_u64()), (Some(4), Some(2)));
        assert_eq!(report["counted_by_a_match"], serde_json::Value::Null, "the step claims no count (R3)");
        assert_eq!(data.sql::<i64>("userdata.sqlite", "SELECT count(*) FROM record_references"), 8, "kept one version");
    }

    /// alice's open request of `record` by Kite Theory, asked by its names alone (no group): its id.
    fn alice_asks(ud: &UserData, record: i64, album: &str) -> i64 {
        ud.add_request(&crate::userdata::Request {
            id: 0,
            user: "alice".into(),
            album_key: format!("kitetheory|{}", crate::catalogue::identity::fold(album)),
            artist_key: "kite theory".into(),
            release_group: None,
            album: album.into(),
            artist: "Kite Theory".into(),
            year: None,
            cover: None,
            at: 1_794_000_000,
            status: super::super::requests::APPROVED.into(),
            reason: None,
            decided_by: None,
            decided_at: None,
            fetched: None,
            record: Some(record),
        })
        .unwrap()
    }

    /// F2.10: 0.17's step makes no doubles of one name from 0.16's data. FX01 plus Glass Harbour's had "Salt Lines" (its files name no
    /// group; two long tracks, an album by the release type rule, as the discography's row says: U2's one kind class), their 0.16 discography listing "Salt Lines" (a group) and "Quiet Ferry" (another, not had), and a 0.16 Want "Quiet Ferry
    /// EP" with a Bandcamp page found by its names and no group. After the step: one live "Salt Lines", the had one, holding the group
    /// (the discography's record folded into it once `credits` credited it); the Want's record "Quiet Ferry" by its title's key
    /// (OQ-F2b (a)), holding the page, listed as found by names; the had record credited to its artist once. Never two live records of
    /// one artist and one title's key where one is not had (X8 counts those as one album, nothing contradicting here).
    #[test]
    fn the_step_makes_no_doubles_of_one_name() {
        use crate::catalogue::identity::title_key;
        let data = Data::new("no-doubles");
        let salt = "Glass Harbour/Salt Lines (2015)";
        data.batch(
            "catalogue.sqlite",
            &format!(
                "INSERT INTO records (id, born, title, artist, year) VALUES (20, 1760000700, 'Salt Lines', 'Glass Harbour', 2015);
                 INSERT INTO tracks (id, born, record, disc, number) VALUES (60, 1760000700, 20, 1, 1), (61, 1760000700, 20, 1, 2);
                 INSERT INTO files (folder, rel, size, mtime, state, format, bitrate, duration_ms, track, bits, rate, came) VALUES
                   (0, '{salt}/01 Brine.flac', 24000000, 1760000000, 1, 'FLAC', 900, 1500000, 60, 16, 44100, 1760000000),
                   (0, '{salt}/02 Kelp.flac', 24000000, 1760000000, 1, 'FLAC', 900, 1500000, 61, 16, 44100, 1760000000);
                 INSERT INTO tags (folder, rel, n, raw, name, value) SELECT folder, rel, 0, 'ALBUM', 'album', 'Salt Lines' FROM files WHERE rel LIKE '{salt}/%';
                 INSERT INTO tags (folder, rel, n, raw, name, value) SELECT folder, rel, 1, 'ALBUMARTIST', 'albumartist', 'Glass Harbour' FROM files WHERE rel LIKE '{salt}/%';
                 INSERT INTO tags (folder, rel, n, raw, name, value) SELECT folder, rel, 2, 'ARTIST', 'artist', 'Glass Harbour' FROM files WHERE rel LIKE '{salt}/%';"
            ),
        );
        let (salt_group, ferry_group) = (mb(119), mb(120));
        data.batch(
            "userdata.sqlite",
            &format!(
                "UPDATE discography SET groups = json_insert(groups, '$[#]', json('{{\"id\":\"{salt_group}\",\"title\":\"Salt Lines\",\"kind\":\"Album\",\"year\":2015,\"genres\":[],\"date\":\"2015\"}}'),
                   '$[#]', json('{{\"id\":\"{ferry_group}\",\"title\":\"Quiet Ferry\",\"kind\":\"Album\",\"year\":2017,\"genres\":[],\"date\":\"2017\"}}')) WHERE artist = '{}';
                 INSERT INTO request (id, user, album_key, artist_key, album, artist, year, at, status) VALUES (16, 'ann', 'glassharbour|quietferryep', 'glass harbour', 'Quiet Ferry EP', 'Glass Harbour', 2017, 1793951100, 'approved');
                 INSERT INTO bandcamp_found (album, at, page) VALUES ('glassharbour|quietferryep', 1793951100, 'https://glassharbour.bandcamp.com/album/quiet-ferry-ep');",
                mb(201)
            ),
        );
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        let live = |title: &str| -> Vec<i64> {
            data.texts("catalogue.sqlite", &format!("SELECT CAST(id AS TEXT) FROM records WHERE title = '{title}' AND merged_into IS NULL AND retired_at IS NULL ORDER BY id")).iter().map(|r| r.parse().unwrap()).collect()
        };
        assert_eq!(live("Salt Lines"), [20], "one live Salt Lines, the had one");
        assert!(refs(&cat, 20).iter().any(|(s, k, v, ..)| (s.as_str(), k.as_str(), v.as_str()) == ("musicbrainz", "release-group", salt_group.as_str())), "holding the group: {:?}", refs(&cat, 20));
        assert_eq!(data.sql::<i64>("catalogue.sqlite", "SELECT count(*) FROM record_artists WHERE record = 20"), 1, "credited to its artist once");
        let ferry = data.sql::<i64>("userdata.sqlite", "SELECT record FROM request WHERE id = 16");
        let row = cat.record(ferry).unwrap().unwrap();
        assert_eq!((row.title.as_str(), row.had), ("Quiet Ferry", false), "the Want's record, by its title's key (OQ-F2b (a))");
        let held = refs(&cat, row.id);
        assert!(held.iter().any(|(_, k, v, ..)| k == "release-group" && *v == ferry_group) && held.iter().any(|(s, _, v, ..)| s == "bandcamp" && v.ends_with("/quiet-ferry-ep")), "{held:?}");
        let report = &ud.migration_report().unwrap()[PART].1;
        assert!(report["requests_found_by_names"].as_array().unwrap().iter().filter_map(serde_json::Value::as_str).any(|l| l.starts_with("16 Glass Harbour - Quiet Ferry EP -> ")), "{report}");
        // Never: a record not had beside another live record of its artist and title's key.
        let mut names: std::collections::BTreeMap<(String, String), Vec<(i64, bool)>> = std::collections::BTreeMap::new();
        for row in data.rows_of("catalogue.sqlite", "SELECT a.artist, r.id, r.title FROM record_artists a JOIN records r ON r.id = a.record WHERE r.merged_into IS NULL AND r.retired_at IS NULL") {
            let cells: Vec<&str> = row.split('|').collect();
            let id: i64 = cells[1].trim_start_matches("Integer(").trim_end_matches(')').parse().unwrap();
            let title = cells[2].trim_start_matches("Text(\"").trim_end_matches("\")");
            names.entry((cells[0].to_string(), title_key(title))).or_default().push((id, cat.record(id).unwrap().unwrap().had));
        }
        // One album by the had rule (X8, X9, U2): a record not had that a had record of its names beside it holds, or two records not
        // had that no two release groups set apart.
        for ((artist, key), rs) in names.iter().filter(|(_, rs)| rs.len() > 1) {
            let not_had: Vec<i64> = rs.iter().filter(|(_, had)| !had).map(|(id, _)| *id).collect();
            let read = cat.requested(&not_had).unwrap();
            for id in &not_had {
                assert!(read[id].holder.is_none_or(|h| !rs.iter().any(|(o, _)| *o == h)), "{artist} {key}: {id} is had by the record of its names beside it: {rs:?}");
                for other in not_had.iter().filter(|o| *o != id) {
                    let (a, b) = (&read[id].groups, &read[other].groups);
                    assert!(!a.is_empty() && !b.is_empty() && a != b, "{artist} {key}: {id} and {other} are one album: {rs:?}");
                }
            }
        }
    }

    /// F2.11 (OQ-F2a (a)): the doubles preview 1 made, as staging's catalogue held them after its step and an opening of the artist,
    /// made directly after the step's other parts: Kite Theory's records not had from MusicBrainz's list (older, a group each) beside
    /// Bandcamp's of their names (newer, a page each), each list's row a record of its own then; "Harbour Road" three times (a had
    /// untagged record, one not had holding a group, one holding a page); a had album beside Bandcamp's record of it (staging's had
    /// halves); a pair whose newer holds alice's open request and bob's heart; "Orbital" by two groups (X8); a pair the hand's Discogs
    /// master sets apart; two had records of one name whose copies do not pair; one name of two artists of one name. The part `fold`,
    /// the step's last: each double merged into the oldest of its name, had first, references moved, the newer answering as the older;
    /// the three Harbour Roads the had one; a merge into a had record a join by names (F2.9-hand gives it back); alice's request read as
    /// the older (Soulseek's next round reads it so), bob's heart on it; each merge in the report; the rest untouched. Started again:
    /// no fold. Never a merge of two had records that are not one record's copies, across artists, or where X8 counts several albums;
    /// never an id that answers nothing, a heart or a request lost.
    #[test]
    fn the_fold_part_merges_the_doubles_of_one_name() {
        use crate::catalogue::records::Listing;
        use crate::serve::ids;
        use crate::userdata::Kind;
        let data = Data::new("fold");
        let (cat, ud) = data.open();
        run_without_fold(&cat, &ud, &[]);
        let (kite, apart) = (cat.artist_for("Kite Theory").unwrap(), cat.artist_apart("Kite Theory").unwrap());
        // As preview 1 kept a list's row: a record of its own (no join by names then), credited to the list's artist. Kept under
        // a name no artist has, then named and credited: F11.2's join of an artist-less row onto a had record of its names would
        // otherwise join it at the making, and the fold would have no double left to fold.
        let kept = |title: &str, kind: Option<&str>, refs: Vec<Ref>, artist: i64| -> i64 {
            let row = Listing { title: title.into(), artist: "Kite Theory as preview 1 kept it".into(), kind: kind.map(str::to_string), refs, ..Listing::default() };
            let record = cat.keep_listing(None, &[row], "link").unwrap()[0];
            cat.execute_for_tests(&format!("UPDATE records SET artist = 'Kite Theory' WHERE id = {record}; INSERT OR IGNORE INTO record_artists (record, artist, position, how) VALUES ({record}, {artist}, 0, 'listing');"));
            record
        };
        let group = |n: u32| vec![Ref::new("musicbrainz", "release-group", &mb(n))];
        let page = |slug: &str| vec![Ref::new("bandcamp", "album", &format!("https://kitetheory.bandcamp.com/album/{slug}"))];
        let had = |title: &str| -> i64 {
            let record = cat.record_for_tests(title, 3);
            cat.execute_for_tests(&format!("UPDATE records SET artist = 'Kite Theory' WHERE id = {record}; INSERT INTO record_artists (record, artist, position, how) VALUES ({record}, {kite}, 0, 'files');"));
            record
        };
        let field = (kept("The Long Field", Some("album"), group(401), kite), kept("The Long Field", None, page("the-long-field"), kite));
        let sky = (kept("Stone Sky", Some("album"), group(402), kite), kept("Stone Sky", None, page("stone-sky"), kite));
        let answered = (kept("Answered", Some("ep"), group(403), kite), kept("Answered EP", None, page("answered-ep"), kite));
        let road = (had("Harbour Road"), kept("Harbour Road", Some("album"), group(404), kite), kept("Harbour Road", None, page("harbour-road"), kite));
        let bridge = (had("Low Bridge"), kept("Low Bridge", None, page("low-bridge"), kite));
        let orbital = (kept("Orbital", Some("album"), group(405), kite), kept("Orbital", Some("album"), group(406), kite));
        let shift = (kept("Night Shift", Some("album"), [group(407), vec![Ref::new("discogs", "master", "3001")]].concat(), kite), kept("Night Shift", None, page("night-shift"), kite));
        assert!(cat.set_reference(&ids::record(shift.1), &Ref::new("discogs", "master", "3002"), "hand").unwrap(), "pasted by hand");
        let twins = (had("Twin Rooms"), had("Twin Rooms"));
        let coast = (kept("Far Coast", Some("album"), group(408), kite), kept("Far Coast", None, page("far-coast"), apart));
        let asked = alice_asks(&ud, sky.1, "Stone Sky");
        ud.set_starred("bob", Kind::Album, &[ids::record(sky.1)], true).unwrap();

        let lines = crate::diagnostics::logged_lines(|| run(&cat, &ud).unwrap());
        assert_eq!(parts_said(&lines), ["fold"], "the step's last part, alone: {lines:#?}");
        let now = |r: i64| cat.record(r).unwrap().expect("every id answers").id;
        for (what, (older, newer)) in [("The Long Field", field), ("Stone Sky", sky), ("Answered", answered)] {
            assert_eq!(now(newer), older, "{what}: the newer answers as the older");
            let held = refs(&cat, older);
            assert!(held.iter().any(|(s, ..)| s == "bandcamp") && held.iter().any(|(s, ..)| s == "musicbrainz"), "{what}: the references moved: {held:?}");
            assert_eq!(cat.record_artists(older).unwrap(), [kite], "{what}: credited once");
        }
        assert_eq!(cat.record(answered.1).unwrap().unwrap().title, "Answered", "the older's title (OQ-F2c)");
        assert_eq!((now(road.1), now(road.2)), (road.0, road.0), "the three Harbour Roads are one, the had one");
        assert_eq!(now(bridge.1), bridge.0, "Bandcamp's record of a had album joins it");
        let joined: Vec<i64> = data.texts("catalogue.sqlite", "SELECT CAST(record AS TEXT) FROM joined_by_names ORDER BY record").iter().map(|r| r.parse().unwrap()).collect();
        assert!([road.1, road.2, bridge.1].iter().all(|r| joined.contains(r)), "a merge into a had record is a join by names (F2.9-hand): {joined:?}");
        let request = ud.request(asked).unwrap().unwrap();
        assert_eq!(super::super::requests::records_of(&cat, &[request]).unwrap()[0].as_ref().map(|o| o.record), Some(sky.0), "alice's request reads the older");
        let hearts = ud.starred("bob", Kind::Album).unwrap();
        assert!(hearts.contains(&ids::record(sky.0)) && !hearts.contains(&ids::record(sky.1)), "bob's heart on the older: {hearts:?}");
        for r in [orbital.0, orbital.1, shift.0, shift.1, twins.0, twins.1, coast.0, coast.1] {
            assert_eq!(now(r), r, "untouched: X8, a hand's master, two had records not one record's copies, two artists of one name");
        }
        let merges = [(field.1, field.0), (sky.1, sky.0), (answered.1, answered.0), (road.1, road.0), (road.2, road.0), (bridge.1, bridge.0)];
        let report = &ud.migration_report().unwrap()[FOLDED].1;
        let folded: Vec<&str> = report["folded"].as_array().unwrap().iter().filter_map(serde_json::Value::as_str).collect();
        for (from, into) in merges {
            assert!(folded.contains(&format!("al-{from} into al-{into}").as_str()), "the report lists each merge: {folded:?}");
        }
        let marked = cat.folded_017().unwrap().expect("marked");
        assert!(merges.iter().all(|m| marked.contains(m)), "{marked:?}");
        let marks = data.rows_of("catalogue.sqlite", "SELECT key, value FROM meta WHERE key GLOB '017:*' ORDER BY key");
        let again = crate::diagnostics::logged_lines(|| run(&cat, &ud).unwrap());
        assert!(parts_said(&again).is_empty(), "started again: no fold, its mark set: {again:#?}");
        assert_eq!(data.rows_of("catalogue.sqlite", "SELECT key, value FROM meta WHERE key GLOB '017:*' ORDER BY key"), marks);
    }

    /// F11.1 (the owner: "One record, two copies"), at the step: Glass Harbour's "Low Tide" twice, had: an untagged MP3 in Music and the
    /// FLAC in Incoming (RG2: the two-copies join's one rule) tagged with its release, three tracks each that pair one for one as Replace pairs them (`incoming::pair`); hearts on two of the
    /// MP3's, a playlist naming one of each, a heart and a play on one of the FLAC's, bob's heart on the FLAC's album. "Drift Maps"
    /// twice, three tracks against four; "Pale Signal" twice, one credited to another artist of the name. The part `fold`: "Low Tide" is
    /// one record, the older, both copies on its tracks, the FLAC's references on it, the FLAC's id answering as it; its album's heart
    /// follows at once, what names the FLAC's tracks at the first build (`track_moved`; here moved as `Serve::follow_taken` moves it), none
    /// lost; the other two stay two, and an arrival's join (`Catalogue::join_copies`) refuses them too. Started again, nothing joins again.
    #[test]
    fn two_owned_copies_with_one_tracklist_are_one_record() {
        use crate::serve::ids;
        use crate::userdata::{Heard, Kind};
        let data = Data::new("one-record-two-copies");
        // RG2: the FLACs in Incoming, as F11.1 has it: the two-copies join reads a record kept in Incoming alone beside a main library's.
        let incoming = data.open_with(&["/music", "/incoming"]).0.folders().id_of(std::path::Path::new("/incoming")).unwrap();
        let album = |record: i64, born: i64, title: &str, dir: &str, form: &str, tracks: &[(i64, &str, i64)], more: &[(&str, &str)]| -> String {
            let mut sql = format!("INSERT INTO records (id, born, title, artist) VALUES ({record}, {born}, '{title}', 'Glass Harbour');");
            let folder = if form == "FLAC" { incoming } else { 0 };
            for (n, (track, name, ms)) in tracks.iter().enumerate() {
                let (number, rel) = ((n + 1).to_string(), format!("Glass Harbour/{dir}/{:02} {name}.{}", n + 1, form.to_lowercase()));
                sql += &format!("INSERT INTO tracks (id, born, record, disc, number) VALUES ({track}, {born}, {record}, 1, {number});");
                sql += &format!(
                    "INSERT INTO files (folder, rel, size, mtime, state, format, bitrate, duration_ms, track, bits, rate, came) VALUES ({folder}, '{rel}', 24000000, {born}, 1, '{form}', 900, {ms}, {track}, 16, 44100, {born});"
                );
                let tags = [("album", title), ("albumartist", "Glass Harbour"), ("artist", "Glass Harbour"), ("title", *name), ("tracknumber", number.as_str())];
                for (i, (k, v)) in tags.iter().chain(more).enumerate() {
                    sql += &format!("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES ({folder}, '{rel}', {i}, upper('{k}'), '{k}', '{v}');");
                }
            }
            sql
        };
        let release = mb(31);
        data.batch(
            "catalogue.sqlite",
            &[
                album(21, 1760000700, "Low Tide", "Low Tide (2014)", "MP3", &[(70, "Ebb", 180_000), (71, "Slack", 190_000), (72, "Flood", 200_000)], &[]),
                album(22, 1794900000, "Low Tide", "Low Tide (2014) [FLAC]", "FLAC", &[(73, "Ebb", 183_000), (74, "Slack", 193_000), (75, "Flood", 203_000)], &[("musicbrainz_albumid", release.as_str())]),
                album(23, 1760000800, "Drift Maps", "Drift Maps (2016)", "MP3", &[(76, "North", 180_000), (77, "South", 190_000), (78, "East", 200_000)], &[]),
                album(24, 1794900100, "Drift Maps", "Drift Maps (2016) [FLAC]", "FLAC", &[(79, "North", 180_000), (80, "South", 190_000), (81, "East", 200_000), (82, "West", 210_000)], &[]),
                album(25, 1760000900, "Pale Signal", "Pale Signal (2018)", "MP3", &[(83, "Glow", 180_000), (84, "Fade", 190_000)], &[]),
                album(26, 1794900200, "Pale Signal", "Pale Signal (2018) [FLAC]", "FLAC", &[(85, "Glow", 180_000), (86, "Fade", 190_000)], &[]),
            ]
            .concat(),
        );
        let (cat, ud) = data.open_with(&["/music", "/incoming"]);
        run_without_fold(&cat, &ud, &[]);
        let another = cat.artist_apart("Glass Harbour").unwrap();
        cat.execute_for_tests(&format!("UPDATE record_artists SET artist = {another} WHERE record = 26;"));
        let song = ids::song;
        ud.set_starred("bob", Kind::Song, &[song(70), song(71), song(73)], true).unwrap();
        ud.record("bob", (&song(73), "p"), "al-22", &[], &Heard::counted(100)).unwrap();
        let lines = ["one".to_string(), "two".to_string()];
        let tracks: std::collections::HashMap<String, Option<String>> = [(lines[0].clone(), Some(song(70))), (lines[1].clone(), Some(song(73)))].into_iter().collect();
        ud.create_playlist("bob", "Mix", &lines, &tracks).unwrap().expect("made");
        ud.set_starred("bob", Kind::Album, &[ids::record(22)], true).unwrap();

        let said = crate::diagnostics::logged_lines(|| run(&cat, &ud).unwrap());
        assert_eq!(parts_said(&said), ["fold"], "{said:#?}");
        assert_eq!(cat.record(22).unwrap().unwrap().id, 21, "one record, the older; the FLAC's id answers as it");
        let on = |dir: &str| data.texts("catalogue.sqlite", &format!("SELECT CAST(f.track AS TEXT) || ' ' || CAST(t.record AS TEXT) FROM files f JOIN tracks t ON t.id = f.track WHERE f.rel LIKE 'Glass Harbour/{dir}/%' ORDER BY f.rel"));
        assert_eq!(on("Low Tide (2014)"), ["70 21", "71 21", "72 21"]);
        assert_eq!(on("Low Tide (2014) [FLAC]"), ["70 21", "71 21", "72 21"], "two copies on one record's tracks, paired one for one");
        assert!(refs(&cat, 21).contains(&by("musicbrainz", "release", &release, "files", false)), "the FLAC's references on it: {:?}", refs(&cat, 21));
        assert!(ud.starred("bob", Kind::Album).unwrap().contains(&ids::record(21)), "the album's heart follows at once");
        let mut moved = cat.tracks_to_follow().unwrap();
        moved.sort_unstable();
        assert_eq!(moved, [(73, 70), (74, 71), (75, 72)], "what names the FLAC's tracks follows at the first build");
        ud.move_tracks(&moved).unwrap();
        let mut hearts = ud.starred("bob", Kind::Song).unwrap();
        hearts.sort();
        assert_eq!(hearts, [song(70), song(71)], "the hearts on the record's tracks, none lost");
        assert_eq!(ud.get("bob", Kind::Song, &[song(70)]).unwrap().get(&song(70)).map(|a| a.play_count), Some(1), "the play");
        assert_eq!(ud.playlist_tracks("bob", "Mix").unwrap().unwrap(), [Some(song(70)), Some(song(70))], "the playlist's entries");
        for ((a, b), what) in [((23, 24), "another tracklist: another edition"), ((25, 26), "two artist ids")] {
            assert_eq!((cat.record(a).unwrap().unwrap().id, cat.record(b).unwrap().unwrap().id), (a, b), "{what}: two records");
            assert_eq!(cat.join_copies(a, b, &[]).unwrap(), None, "{what}: an arrival joins them neither");
        }
        let again = crate::diagnostics::logged_lines(|| run(&cat, &ud).unwrap());
        assert!(parts_said(&again).is_empty(), "{again:#?}");
        assert_eq!(cat.join_copies(21, 22, &[]).unwrap(), None, "no second join of the pair");
        assert!(cat.tracks_to_follow().unwrap().iter().all(|m| moved.contains(m)), "nothing taken again");
    }

    /// RG2 (PA2): the two-copies join reads one rule, at the upgrade's fold as when a copy arrives (`Catalogue::join_copies`): a record
    /// kept in Incoming alone and one with a copy in a main library, never a copy in an account's own music. Glass Harbour's untagged
    /// "Low Tide" in Music beside bob's own rip of it (the same tracks; his year set by hand on his): two records, his facts on his alone;
    /// "Drift Maps" in bob's own music, the older, beside Music's: two, Music's tracks its own; "Pale Signal" twice in Music: one
    /// record (FA18, changed by design: a main library with itself is a main pair); "Brine Flats" in Music and in Incoming: one record, the older. RG1 between two had records: "Grey Cove" (a
    /// single by the rule) in Music beside Incoming's "Grey Cove LP": two. An arrival joins none of the first three either.
    #[test]
    fn the_two_copies_join_reads_main_and_incoming_alone() {
        use crate::serve::ids;
        let data = Data::new("two-copies-one-rule");
        let roots = ["/music", crate::fixtures_base::BOB_OWN, "/incoming"];
        let (own, incoming) = {
            let (cat, _) = data.open_with(&roots);
            let id = |root: &str| cat.folders().id_of(std::path::Path::new(root)).unwrap();
            (id(crate::fixtures_base::BOB_OWN), id("/incoming"))
        };
        let kinds = [(0, LibraryKind::Main), (own, LibraryKind::Own), (incoming, LibraryKind::Incoming)];
        let album = |record: i64, folder: i64, title: &str, names: [&str; 3]| -> String {
            let dir = format!("Glass Harbour/{title} {record}");
            let mut sql = format!("INSERT INTO records (id, born, title, artist) VALUES ({record}, {}, '{title}', 'Glass Harbour');", 1_760_000_000 + record);
            for (n, name) in names.iter().enumerate() {
                let (track, number) = (record * 10 + n as i64, (n + 1).to_string());
                let rel = format!("{dir}/{number:0>2} {name}.mp3");
                sql += &format!("INSERT INTO tracks (id, born, record, disc, number) VALUES ({track}, 1760000000, {record}, 1, {number});");
                sql += &format!(
                    "INSERT INTO files (folder, rel, size, mtime, state, format, bitrate, duration_ms, track, bits, rate, came) VALUES ({folder}, '{rel}', 8000000, 1760000000, 1, 'MP3', 320, {}, {track}, 16, 44100, 1760000000);",
                    180_000 + 10_000 * n
                );
                let tags = [("album", title), ("albumartist", "Glass Harbour"), ("artist", "Glass Harbour"), ("title", *name), ("tracknumber", number.as_str())];
                for (i, (k, v)) in tags.iter().enumerate() {
                    sql += &format!("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES ({folder}, '{rel}', {i}, upper('{k}'), '{k}', '{v}');");
                }
            }
            sql
        };
        let (tide, maps, signal, flats, cove) = (["Ebb", "Slack", "Flood"], ["North", "South", "East"], ["Glow", "Fade", "Dusk"], ["Brine", "Kelp", "Shoal"], ["Cliff", "Inlet", "Bay"]);
        data.batch(
            "catalogue.sqlite",
            &[
                album(41, 0, "Low Tide", tide),
                album(42, own, "Low Tide", tide),
                album(43, own, "Drift Maps", maps),
                album(44, 0, "Drift Maps", maps),
                album(45, 0, "Pale Signal", signal),
                album(46, 0, "Pale Signal", signal),
                album(47, 0, "Brine Flats", flats),
                album(48, incoming, "Brine Flats", flats),
                album(49, 0, "Grey Cove", cove),
                album(50, incoming, "Grey Cove LP", cove),
            ]
            .concat(),
        );
        let (cat, ud) = data.open_with(&roots);
        ud.set_overrides(&[ids::record(42)], &[("year".to_string(), Some(serde_json::json!(2014)))], 1).unwrap();
        super::run(&cat, &ud, &kinds, &FolderRules::default()).unwrap();
        let now = |r: i64| cat.record(r).unwrap().unwrap().id;
        for ((a, b), what) in [((41, 42), "Music's and bob's own"), ((43, 44), "bob's own, the older, and Music's"), ((49, 50), "a single and an LP of its name (RG1)")] {
            assert_eq!((now(a), now(b)), (a, b), "{what}: two records");
        }
        assert_eq!(now(48), 47, "Music's and Incoming's: one record, the older");
        // Changed by design (SCENARIOS-DC1 FA18, M11: RG2-wide, a main library with itself is a main pair; one tracklist is one record).
        assert_eq!(now(46), 45, "two in Music: one record, the older");
        assert_eq!(
            data.rows_of("userdata.sqlite", "SELECT album, field, value FROM record_overrides WHERE album IN ('al-41', 'al-42') ORDER BY album, field"),
            ["Text(\"al-42\")|Text(\"year\")|Text(\"2014\")"],
            "bob's facts on his record alone, none on the house's"
        );
        assert_eq!(
            data.texts("catalogue.sqlite", "SELECT DISTINCT CAST(t.record AS TEXT) FROM files f JOIN tracks t ON t.id = f.track WHERE f.folder = 0 AND f.rel LIKE 'Glass Harbour/Drift Maps%'"),
            ["44"],
            "Music's copy keeps its own tracks"
        );
        for (a, b) in [(41, 42), (43, 44)] {
            assert_eq!(cat.join_copies(a, b, &kinds).unwrap(), None, "al-{a} and al-{b}: an arrival joins them neither");
        }
    }

    /// Tags added to every file of an album of FX01 (`rels`, a LIKE pattern under Music), from tag number 40 on.
    fn tag_album(cat: &Catalogue, rels: &str, tags: &[(&str, &str)]) {
        let sql: String = tags
            .iter()
            .zip(40..)
            .map(|((name, value), n)| format!("INSERT INTO tags (folder, rel, n, raw, name, value) SELECT folder, rel, {n}, upper('{name}'), '{name}', '{value}' FROM files WHERE folder = 0 AND rel LIKE '{rels}';"))
            .collect();
        cat.execute_for_tests(&sql);
    }

    /// A2.1.5: (a) FX01's library agrees with itself: the part makes no artist apart. (b) FX01 with Partial Shore's files naming their
    /// album artist "Glass Harbour" with another MusicBrainz id, B, than Tide Tables' (Glass Harbour's own, which the step's
    /// discographies gave them): Tide Tables is Glass Harbour's; Partial Shore an artist of their own's, who holds B, listed in the
    /// part's report; Glass Harbour never holds B.
    #[test]
    fn the_credits_part_credits_each_files_id_to_its_own_artist() {
        let plain = Data::tagged("credits-plain");
        {
            let (cat, ud, kinds) = plain.open_both();
            super::run(&cat, &ud, &kinds, &FolderRules::default()).unwrap();
            let mark: serde_json::Value = serde_json::from_str(&cat.meta("017:credits").unwrap()).unwrap();
            assert!(mark.get("apart").is_none(), "(a) no artist made apart: {mark}");
        }
        const HARBOUR: &str = "00000000-0000-4000-8000-000000000201";
        const B: &str = "00000000-0000-4000-8000-000000000a12";
        let data = Data::tagged("credits-ids");
        let (cat, ud, kinds) = data.open_both();
        tag_album(&cat, "Glass Harbour/Partial Shore (2011)/%", &[("musicbrainz_albumartistid", B)]);
        super::run(&cat, &ud, &kinds, &FolderRules::default()).unwrap();
        let holder = |id: &str| -> Vec<String> { cat.subject_of(&crate::catalogue::records::Ref::new("musicbrainz", "artist", id)).unwrap() };
        let harbour = cat.artist_of("Glass Harbour").unwrap().unwrap();
        assert_eq!(holder(HARBOUR), [crate::serve::ids::artist(harbour)], "Glass Harbour holds their own id");
        let made = holder(B);
        assert_eq!(made.len(), 1, "one artist holds B");
        let made = crate::serve::ids::artist_number(&made[0]).unwrap();
        assert_ne!(made, harbour, "never on Glass Harbour");
        assert_eq!(data.sql::<i64>("catalogue.sqlite", "SELECT artist FROM record_artists WHERE record = 1 AND how = 'files'"), harbour, "Tide Tables: Glass Harbour's");
        assert_eq!(data.sql::<i64>("catalogue.sqlite", "SELECT artist FROM record_artists WHERE record = 8 AND how = 'files'"), made, "Partial Shore: the artist apart's");
        let mark: serde_json::Value = serde_json::from_str(&cat.meta("017:credits").unwrap()).unwrap();
        assert_eq!(mark["apart"], serde_json::json!([format!("{} Glass Harbour: musicbrainz {B}", crate::serve::ids::artist(made))]), "listed in the part's report");
    }

    /// A2.2.3 (production): FX01 with Partial Shore's files naming their album artist "Glass Hárbour" and Glass Harbour's MusicBrainz
    /// id: after the step, one credited artist for both spellings.
    #[test]
    fn the_step_credits_one_artist_for_spellings_of_one_id() {
        let data = Data::tagged("credits-spellings");
        let (cat, ud, kinds) = data.open_both();
        cat.execute_for_tests("UPDATE tags SET value = 'Glass H\u{e1}rbour' WHERE folder = 0 AND name = 'albumartist' AND rel LIKE 'Glass Harbour/Partial Shore (2011)/%';");
        tag_album(&cat, "Glass Harbour/Partial Shore (2011)/%", &[("musicbrainz_albumartistid", "00000000-0000-4000-8000-000000000201")]);
        super::run(&cat, &ud, &kinds, &FolderRules::default()).unwrap();
        let credited = data.texts("catalogue.sqlite", "SELECT DISTINCT CAST(artist AS TEXT) FROM record_artists WHERE record IN (1, 8) AND how = 'files'");
        assert_eq!(credited.len(), 1, "one artist for both spellings: {credited:?}");
        assert_eq!(credited[0].parse::<i64>().unwrap(), cat.artist_of("Glass Harbour").unwrap().unwrap());
    }

    /// A2.4.4 (production): FX01 with files naming "Warp", "Warp Records" and "Warp Records,Beat Records": after the step, the labels
    /// Warp and Beat Records, one each, the three records tied to Warp.
    #[test]
    fn the_step_makes_one_label_per_bare_name() {
        let data = Data::tagged("labels-bare");
        let (cat, ud, kinds) = data.open_both();
        tag_album(&cat, "Kite Theory/Night Walks EP (2020)/%", &[("label", "Warp")]);
        tag_album(&cat, "Kite Theory/Undated (no year)/%", &[("label", "Warp Records")]);
        tag_album(&cat, "Glass Harbour/Partial Shore (2011)/%", &[("label", "Warp Records,Beat Records")]);
        super::run(&cat, &ud, &kinds, &FolderRules::default()).unwrap();
        let named = data.texts("catalogue.sqlite", "SELECT name FROM labels WHERE name LIKE '%Warp%' OR name LIKE '%Beat%' ORDER BY name");
        assert_eq!(named, ["Beat Records", "Warp"], "one label per bare name");
        let warp = data.texts("catalogue.sqlite", "SELECT CAST(r.record AS TEXT) FROM record_labels r JOIN labels l ON l.id = r.label WHERE l.name = 'Warp' ORDER BY r.record");
        assert_eq!(warp, ["3", "4", "8"], "every record naming Warp tied to the one label");
    }

    /// F15.5: (a) FX01: no merge by a master or a group (0.16's data holds one record per group and masters on had records alone): no two
    /// records not had share one before the step, and none of the fold's merges joins two that did. (b) A catalogue as a preview left it:
    /// records not had X and Y of master 5000005 (two releases), credited to two artists' lists, and Z and W of one release group: Y into
    /// X and W into Z, listed in the fold's report; never a had record, never across two groups.
    #[test]
    fn the_fold_joins_records_not_had_of_one_master_or_group() {
        let plain = Data::new("fold-masters-plain");
        let shared = |data: &Data| -> Vec<String> {
            data.texts(
                "catalogue.sqlite",
                "SELECT a.subject || ' ' || b.subject FROM \"references\" a JOIN \"references\" b ON a.source = b.source AND a.kind = b.kind AND a.value = b.value AND a.subject < b.subject
                 WHERE a.subject GLOB 'al-*' AND b.subject GLOB 'al-*' AND ((a.source = 'discogs' AND a.kind = 'master') OR (a.source = 'musicbrainz' AND a.kind = 'release-group'))
                   AND NOT EXISTS (SELECT 1 FROM tracks t WHERE 'al-' || t.record IN (a.subject, b.subject) AND t.retired_at IS NULL)",
            )
        };
        {
            let (cat, ud) = plain.open();
            run_without_fold(&cat, &ud, &[]);
            assert!(shared(&plain).is_empty(), "(a) as the fold begins, no two records not had share a master or a group: {:?}", shared(&plain));
            fold(&cat, &ud, &[], &|| Ok(())).unwrap();
            assert!(shared(&plain).is_empty(), "(a) and none after it");
        }
        let data = Data::new("fold-masters");
        let (cat, ud) = data.open();
        let (harbour, field) = (cat.artist_for("Glass Harbour").unwrap(), cat.artist_for("Noor Field").unwrap());
        let made = |title: &str, artist: i64, refs: &[(&str, &str, &str)]| {
            let record = cat.record_for_tests(title, 0);
            cat.db_for_tests().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'listing')", rusqlite::params![record, artist]).unwrap();
            for (s, k, v) in refs {
                assert!(cat.set_reference(&crate::serve::ids::record(record), &crate::catalogue::records::Ref::new(s, k, v), "link").unwrap());
            }
            record
        };
        let x = made("Two Rooms", harbour, &[("discogs", "release", "3600006"), ("discogs", "master", "5000005"), ("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000f07")]);
        let y = made("Two Rooms", field, &[("discogs", "release", "3600007"), ("discogs", "master", "5000005")]);
        let g5 = "00000000-0000-4000-8000-000000000f05";
        let z = made("Undertow", harbour, &[("musicbrainz", "release-group", g5)]);
        let w = made("Undertow (Live)", field, &[("musicbrainz", "release-group", g5)]);
        let other = made("Two Rooms (Deluxe)", field, &[("discogs", "master", "5000005"), ("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000f06")]);
        run(&cat, &ud).unwrap();
        let end = |r: i64| cat.record(r).unwrap().unwrap().id;
        assert_eq!((end(y), end(w)), (x, z), "(b) Y into X, W into Z");
        let fold = cat.meta("017:fold").unwrap();
        for line in [format!("al-{y} into al-{x}"), format!("al-{w} into al-{z}")] {
            assert!(fold.contains(&line), "listed: {line} in {fold}");
        }
        assert_eq!(end(other), other, "a second release group: never folded by the master");
    }
}
