//! 0.17's step (decision 0100, step 7; C44): what 0.16 knew of each record's references, moved once into the catalogue's
//! tables at the first start of 0.17, before anything reads them.
//!
//! - **What it reads**, the user data as 0.16.2 left it, writing nothing there but its report: `record_references` (0.14's
//!   choices), `album_matches` (matching's finds), `discogs_facts`' album rows, `bandcamp_found`'s pages, `mb_releases`'
//!   Discogs links; and in the catalogue, the release and release group each record's files name.
//! - **What it writes**, the catalogue, in one transaction (`Catalogue::carry_references`): `"references"` rows; `records`'
//!   `counting` and `counted_by_hand`, `checked_by_hand`; `confirmed`. No list: a match keeps its files' names and none of
//!   MusicBrainz's titles, so a list is read when its record is opened. `kind` stays as 0.16 left it, none: nothing reads it,
//!   and a record had shows the type its files' tags and the release-type rule give at every build.
//! - **Once, safe to repeat:** its mark (`meta`, holding the report) is the transaction's last write, so a step cut off wrote
//!   nothing and runs whole at the next start. The report then goes into the user data's migration report from the mark, which
//!   a start finding the mark without it does again. Every write finds its row there the second time and leaves it.
//! - **Old tables kept** one version, read by nothing else: none is emptied or dropped.
//! - **One release on two records** (G1): who keeps it goes by the order the parts run in: a link pasted by hand, a certain
//!   match (MusicBrainz matched the record's tracks to it), the files' own (the older record first), a likely match.

use anyhow::Result;

use crate::catalogue::Catalogue;
use crate::catalogue::records::{Carry, CarriedChoice, CarriedDiscogs, CarriedLinks, CarriedMatch, CarriedPage, Ref};
use crate::userdata::UserData;

/// The migration report's part, as `/api/admin/migration` lists it.
pub const PART: &str = "references";

/// The step at a start: done before, or done now. Blocking.
pub fn run(cat: &Catalogue, ud: &UserData) -> Result<()> {
    run_with(cat, ud, &|_| Ok(()))
}

/// `run`, `after` hearing each part done: an error from it stops the step there, as a stop would (what the tests cut it by).
fn run_with(cat: &Catalogue, ud: &UserData, after: &dyn Fn(&str) -> Result<()>) -> Result<()> {
    if let Some(report) = cat.references_carried()? {
        if !ud.migration_report()?.contains_key(PART) {
            ud.set_migration_part(PART, &report)?;
        }
        return Ok(());
    }
    let carry = carry_of(&ud.kept_for_017()?);
    let report = cat.carry_references(&carry, after)?;
    after("catalogue")?;
    ud.set_migration_part(PART, &report)?;
    tracing::info!(
        target: "library",
        references = report["references"].as_u64().unwrap_or(0),
        counted_by_hand = report["counted_by_hand"].as_u64().unwrap_or(0),
        counted_by_a_match = report["counted_by_a_match"].as_u64().unwrap_or(0),
        checked_by_hand = report["checked_by_hand"].as_u64().unwrap_or(0),
        "0.17's step: the records' references moved into the catalogue"
    );
    Ok(())
}

/// What the user data's rows say, read: each row's JSON as 0.16 wrote it, a row whose JSON cannot be read named in the report.
fn carry_of(kept: &crate::userdata::KeptFor017) -> Carry {
    let mut c = Carry::default();
    let mut unread = |table: &'static str, key: &str| match c.unread.iter_mut().find(|(t, _)| *t == table) {
        Some((_, keys)) => keys.push(key.to_string()),
        None => c.unread.push((table, vec![key.to_string()])),
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
            Err(_) => unread("record_references", album),
        }
    }
    let mut matches = Vec::new();
    for (album, at, json) in &kept.matches {
        let Some(json) = json else { continue };
        match serde_json::from_str::<super::matching::Match>(json) {
            Ok(m) => matches.push(CarriedMatch { album: album.clone(), at: *at, release: m.release.to_ascii_lowercase(), group: m.release_group.map(|g| g.to_ascii_lowercase()), certain: m.certain, tracks: m.tracks.len() }),
            Err(_) => unread("album_matches", album),
        }
    }
    let mut discogs = Vec::new();
    for (subject, at, json) in &kept.discogs {
        let (Some(album), Some(json)) = (subject.strip_prefix("album:"), json) else { continue };
        match serde_json::from_str::<super::discogs::AlbumFacts>(json) {
            Ok(f) => discogs.push(CarriedDiscogs { album: album.to_string(), at: *at, master: f.master, release: f.release }),
            Err(_) => unread("discogs_facts: albums", subject),
        }
    }
    let pages = kept
        .pages
        .iter()
        .filter_map(|(key, at, page)| Some(CarriedPage { key: key.clone(), at: *at, page: crate::bandcamp::album_page(page.as_deref()?)? }))
        .collect();
    let mut links = Vec::new();
    for (release, at, json) in &kept.releases {
        match serde_json::from_str::<super::works::Stored>(json) {
            Ok(s) => {
                let discogs: Vec<Ref> = s.discogs.iter().filter_map(|l| l.split_once(':')).filter(|(kind, id)| matches!(*kind, "release" | "master") && id.parse::<u64>().is_ok()).map(|(kind, id)| Ref::new("discogs", kind, id)).collect();
                if !discogs.is_empty() {
                    links.push(CarriedLinks { release: release.to_ascii_lowercase(), at: *at, discogs });
                }
            }
            Err(_) => unread("mb_releases", release),
        }
    }
    c.read = vec![
        ("record_references", kept.chosen.len() as u64),
        ("album_matches", kept.matches.len() as u64),
        ("discogs_facts: albums", kept.discogs.iter().filter(|(s, ..)| s.starts_with("album:")).count() as u64),
        ("bandcamp_found", kept.pages.len() as u64),
        ("mb_releases", kept.releases.len() as u64),
    ];
    (c.chosen, c.matches, c.discogs, c.pages, c.links) = (chosen, matches, discogs, pages, links);
    c
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rusqlite::Connection;
    use rusqlite::types::Value;

    use super::*;

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

        /// `rows`, its mark and its report.
        fn left(&self) -> Vec<String> {
            [
                self.rows(),
                self.rows_of("catalogue.sqlite", "SELECT value FROM meta WHERE key = 'references_017'"),
                self.rows_of("userdata.sqlite", "SELECT report FROM migration_report WHERE part = 'references'"),
            ]
            .concat()
        }

        fn sql<T: rusqlite::types::FromSql>(&self, file: &str, sql: &str) -> T {
            Connection::open(self.0.join(file)).unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
        }
    }

    impl Drop for Data {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    /// The parts, in the order the step goes through them; "catalogue" is after its commit, before the report.
    const PARTS: [&str; 10] = ["hand", "certain", "files", "likely", "discogs", "links", "pages", "chosen", "counting", "catalogue"];

    /// Run twice, the second finds its mark and does nothing; its mark gone, it writes nothing new; cut off after any part, the
    /// next start ends where an uncut step does.
    #[test]
    fn the_references_step_is_safe_to_repeat() {
        let once = Data::new("once");
        {
            let (cat, ud) = once.open();
            run(&cat, &ud).unwrap();
        }
        let left = once.left();
        assert!(left.iter().any(|r| r.contains("\"hand\"")), "{left:?}");
        {
            let (cat, ud) = once.open();
            run(&cat, &ud).unwrap();
        }
        assert_eq!(once.left(), left, "its mark there: nothing");
        Connection::open(once.0.join("catalogue.sqlite")).unwrap().execute("DELETE FROM meta WHERE key = 'references_017'", []).unwrap();
        {
            let (cat, ud) = once.open();
            run(&cat, &ud).unwrap();
        }
        assert_eq!(once.rows(), left[..once.rows().len()], "its mark gone: every row found as it was left");
        for part in PARTS {
            let cut = Data::new(&format!("cut-{part}"));
            {
                let (cat, ud) = cut.open();
                let err = run_with(&cat, &ud, &|at| if at == part { anyhow::bail!("stopped after {at}") } else { Ok(()) }).unwrap_err();
                assert_eq!(err.to_string(), format!("stopped after {part}"));
            }
            if part != "catalogue" {
                assert_eq!(cut.sql::<i64>("catalogue.sqlite", "SELECT count(*) FROM \"references\""), 0, "{part}: one transaction, nothing of it");
            }
            {
                let (cat, ud) = cut.open();
                run(&cat, &ud).unwrap();
            }
            assert_eq!(cut.left(), left, "cut after {part}, then started again");
        }
    }

    fn mb(n: u32) -> String {
        format!("00000000-0000-4000-8000-{n:012}")
    }

    /// The samples of one of the report's `differed` lists.
    fn differed(report: &serde_json::Value, what: &str) -> Vec<String> {
        let list = report["differed"].as_array().unwrap().iter().find(|d| d["what"] == what).unwrap_or_else(|| panic!("no {what}: {report}"));
        list["sample"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
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
    /// record its files are now (record 8).
    #[test]
    fn a_retired_record_gives_its_release_to_a_live_one() {
        let data = Data::new("retired");
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        assert_eq!(cat.subject_of(&Ref::new("musicbrainz", "release", &mb(6))).unwrap(), ["al-8"]);
        assert_eq!(refs(&cat, 8), [by("musicbrainz", "release", &mb(6), "files", false)]);
        assert!(refs(&cat, 6).iter().all(|(_, kind, ..)| kind != "release"), "{:?}", refs(&cat, 6));
        let report = &ud.migration_report().unwrap()[PART].1;
        assert!(differed(report, "references: a retired record's").contains(&format!("al-6 musicbrainz:release:{}", mb(6))), "{report}");
    }

    fn refs(cat: &Catalogue, record: i64) -> Vec<(String, String, String, String, bool)> {
        cat.references_of(&crate::serve::ids::record(record)).unwrap().into_iter().map(|(r, row)| (r.source, r.kind, r.value, row.how, row.confirmed)).collect()
    }

    fn by(source: &str, kind: &str, value: &str, how: &str, confirmed: bool) -> (String, String, String, String, bool) {
        (source.into(), kind.into(), value.into(), how.into(), confirmed)
    }

    /// What 0.14 chose stands after the step: a source chosen, a link pasted, checked by hand, a guess confirmed; a retired
    /// record's too. A certain match counts where nothing was chosen. Who keeps one release two records' files name.
    #[test]
    fn a_chosen_source_survives_the_step() {
        let data = Data::new("chosen");
        let (cat, ud) = data.open();
        run(&cat, &ud).unwrap();
        let mb = |n: u32| format!("00000000-0000-4000-8000-{n:012}");
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
        // Undated: its one file names Tide Tables' release (a tag copied in error): the release stays where it is.
        assert!(refs(&cat, 4).is_empty());
        // Late Train OST: AcoustID counting, as chosen.
        assert_eq!(counted(5).0, chosen("acoustid"));
        // Partial Shore, retired: its choices all the same, and its master; no release (R1).
        assert_eq!(counted(6), (chosen("musicbrainz"), true, true, None));
        assert_eq!(refs(&cat, 6), [by("discogs", "master", "1000006", "search", true)], "Discogs confirmed in 0.14");
        // Signal: nothing chosen, a certain match over its one track: MusicBrainz counts, worked out.
        assert_eq!(counted(7), (chosen("musicbrainz"), false, false, None));
        assert!(cat.reference_tracks(7).unwrap().is_empty(), "no list: a match keeps none of MusicBrainz's titles");

        let report = &ud.migration_report().unwrap()[PART].1;
        let unmatched = |what: &str| report["unmatched"].as_array().unwrap().iter().find(|u| u["what"] == what).map(|u| u["count"].as_u64().unwrap()).unwrap_or(0);
        assert_eq!(unmatched("album_matches"), 1, "{report}");
        assert_eq!(unmatched("bandcamp_found"), 1, "Harbour Lights, a record not had: C48b's");
        assert_eq!(differed(report, "references: held by another record"), [format!("al-9 musicbrainz:release:{}", mb(2)), format!("al-4 musicbrainz:release:{}", mb(1))], "Two Rooms' hand, Undated's tag");
        assert_eq!(differed(report, "references: a retired record's"), [format!("al-6 musicbrainz:release:{}", mb(6)), "al-6 discogs:release:2000006".to_string()]);
        assert_eq!((report["counted_by_hand"].as_u64(), report["counted_by_a_match"].as_u64(), report["checked_by_hand"].as_u64()), (Some(4), Some(1), Some(2)));
        assert_eq!(data.sql::<i64>("userdata.sqlite", "SELECT count(*) FROM record_references"), 5, "kept one version");
    }
}
