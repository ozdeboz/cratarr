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
//! - **Failed, it stops the start**, as 0.16's first start does: what it has not carried would be read as never known.
//! - **Old tables kept** one version, read by nothing else: none is emptied or dropped.
//! - **The discographies** (C48b, a part of its own): each artist's albums and EPs as MusicBrainz listed them (`discography`)
//!   become records not had, but for those a record holds already by its release group, credited to the artist and found
//!   `link`; the artist's reference is read when the discography was. `related_artists` and the matches are read for the names
//!   of artists nothing here names. Before the pages, so a record not had gets the page kept by its group. `discography` stays
//!   as it was, read by Discover as before.
//! - **One release on two records** (G1): who keeps it goes by the order the parts run in: a link pasted by hand, a certain
//!   match (MusicBrainz matched the record's tracks to it; two on one release, the older record), the files' own (the older
//!   record first), a likely match. A retired record holds none.

use anyhow::Result;

use crate::catalogue::Catalogue;
use crate::catalogue::records::{JOINS, Carry, CarriedChoice, CarriedDiscography, CarriedDiscogs, CarriedLinks, CarriedMatch, CarriedPage, Ref};
use crate::userdata::UserData;

/// The migration report's part, as `/api/admin/migration` lists it.
pub const PART: &str = "references";

/// The step at a start: done before, or done now. Blocking.
pub fn run(cat: &Catalogue, ud: &UserData) -> Result<()> {
    run_with(cat, ud, &|_| Ok(()))
}

/// `run`, `after` hearing each part done: an error from it stops the step there, as a stop would (what the tests cut it by).
/// "catalogue" is after the parts, before the report.
fn run_with(cat: &Catalogue, ud: &UserData, after: &dyn Fn(&str) -> Result<()>) -> Result<()> {
    let ran = if cat.references_to_carry()?.is_empty() { Vec::new() } else { cat.carry_references(&carry_of(&ud.kept_for_017()?), after)? };
    if ran.is_empty() && ud.migration_report()?.contains_key(PART) {
        return Ok(());
    }
    after("catalogue")?;
    let Some(report) = cat.references_carried()? else { return Ok(()) };
    ud.set_migration_part(PART, &report)?;
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
    Carry {
        chosen,
        matches,
        discogs,
        pages,
        links,
        discographies,
        read: vec![
            ("record_references", kept.chosen.len() as u64),
            ("album_matches", kept.matches.len() as u64),
            ("discogs_facts: albums", kept.discogs.len() as u64),
            ("bandcamp_found", kept.pages.len() as u64),
            ("mb_releases", kept.releases.len() as u64),
            ("discography", kept.discographies.len() as u64),
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

    /// FX01's data folder, its catalogue opened with its one library (`/music`) in use.
    struct Data(PathBuf);

    impl Data {
        fn new(name: &str) -> Self {
            let d = std::env::temp_dir().join(format!("cratarr-step-017-{name}-{}", std::process::id()));
            std::fs::remove_dir_all(&d).ok();
            crate::fixtures::data_0_16(&d).unwrap();
            Self(d)
        }

        fn open(&self) -> (Catalogue, UserData) {
            let cat = Catalogue::open(Some(&self.0.join("catalogue.sqlite")), vec![PathBuf::from("/music")]).unwrap();
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
                assert_eq!(err.to_string(), format!("stopped after {part}"));
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

    /// R2: each part has a mark of its own, so a data folder that ran an earlier version of the step, one without a part, runs
    /// that part at its next start and not the parts it ran: here the pages part is the one it lacked, and a link pasted that
    /// was let go of since stays let go of. The report is written again, with the new part's.
    #[test]
    fn a_later_part_runs_where_an_earlier_part_is_marked() {
        let data = Data::new("later-part");
        data.run();
        let pages = data.sql::<i64>("catalogue.sqlite", "SELECT count(*) FROM \"references\" WHERE source = 'bandcamp'");
        assert!(pages > 0);
        data.batch(
            "catalogue.sqlite",
            "DELETE FROM \"references\" WHERE source = 'bandcamp'; DELETE FROM meta WHERE key = '017:pages';
             DELETE FROM \"references\" WHERE subject = 'al-2' AND source = 'discogs' AND how = 'hand';",
        );
        data.batch("userdata.sqlite", "UPDATE migration_report SET report = '{}' WHERE part = 'references'");
        data.run();
        assert_eq!(data.sql::<i64>("catalogue.sqlite", "SELECT count(*) FROM \"references\" WHERE source = 'bandcamp'"), pages, "the part it lacked ran");
        assert_eq!(data.sql::<i64>("catalogue.sqlite", "SELECT count(*) FROM \"references\" WHERE subject = 'al-2' AND source = 'discogs' AND how = 'hand'"), 0, "a part done did not run again");
        assert_eq!(data.marks().len(), PARTS_017.len());
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
        assert!(listed(report, "bandcamp_found").1.is_empty(), "Harbour Lights' found its record not had (C48b): {report}");
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

    /// R5, measuring: the step over a catalogue as large as production's (5,000 records of 8 tracks, 40,000 files with ten
    /// tags each, 400,000 tags), with a match for 3,000 records, two in three certain, and 2,000 each of Discogs' facts,
    /// Bandcamp's pages and MusicBrainz's links; then what every build now reads of the tags (`keep_files_names`). Run by hand:
    /// `rt.sh test -- --ignored --nocapture the_step_at_production_size`.
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
                        ("date", "2012".to_string()),
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
            tx.commit().unwrap();
        }
        let made = made.elapsed();
        let step = std::time::Instant::now();
        run(&cat, &ud).unwrap();
        let step = step.elapsed();
        assert!(cat.references_to_carry().unwrap().is_empty());
        let references: i64 = Connection::open(d.join("catalogue.sqlite")).unwrap().query_row("SELECT count(*) FROM \"references\"", [], |r| r.get(0)).unwrap();
        let build = std::time::Instant::now();
        cat.keep_files_names(&[]).unwrap();
        let build = build.elapsed();
        eprintln!("0.17's step at production size: {step:?} for {references} references (made in {made:?}); what a build reads of the tags: {build:?}");
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
        assert!(!cat.unreferenced(true).unwrap().contains(&12), "Undertow waits for its list");

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
        assert_eq!(counting(&cat, 7).as_deref(), Some("musicbrainz"), "Signal's list agrees with its file");
        assert_eq!(counting(&cat, 12), None, "Undertow's does not");
        assert_eq!(counting(&cat, 10), None, "no list");
        assert!(cat.unreferenced(true).unwrap().contains(&12), "on Health's item now");
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
        assert_eq!(listed(report, "bandcamp_found").0, 0, "Harbour Lights', a record not had now (C48b)");
        assert_eq!(
            differed(report, "references: held by another record"),
            [format!("al-9 musicbrainz:release:{}", mb(2)), format!("al-11 musicbrainz:release:{FERRY}"), format!("al-4 musicbrainz:release:{}", mb(1)), "al-4 discogs:release:2000003".to_string()],
            "Two Rooms' hand, Ferry Lights' older copy, Undated's tag and its search"
        );
        assert_eq!(differed(report, "references: a retired record's"), [format!("al-6 musicbrainz:release:{}", mb(6)), "al-6 discogs:release:2000006".to_string()]);
        assert_eq!((report["counted_by_hand"].as_u64(), report["checked_by_hand"].as_u64()), (Some(4), Some(2)));
        assert_eq!(report["counted_by_a_match"], serde_json::Value::Null, "the step claims no count (R3)");
        assert_eq!(data.sql::<i64>("userdata.sqlite", "SELECT count(*) FROM record_references"), 6, "kept one version");
    }
}
