//! Beyond (decision 0076, C57): MusicBrainz's albums and artists and Discogs' masters for the words, as search shows them past
//! what Cratarr knows. Its results are a cache, not records (Q33, OQ12 (a)): kept a few minutes per query in memory
//! (`discover::musicbrainz_found`, `discogs::search_masters`), and nothing is made when it answers.
//!
//! - **No name merge.** A MusicBrainz row and a Discogs row are one only where a record holds both the release group and the
//!   master; two albums or two artists of one name from the sources stay two.
//! - **Here by reference alone.** An album row names the record that holds its release group or its master (`record`), as merged
//!   now, and is had by the one had rule on that record. An artist row names the artist who holds their MusicBrainz id
//!   (`artist`), had where the library has albums by them. A name alone names no record.
//! - **Had through its names, for display alone (Z6).** A row no record holds is had where the had rule's names clause would hold
//!   it: a live had record of its folded artist and title, holding no reference that contradicts the row's, the name one album's
//!   (Y1). It names no record, and nothing is made or tied.
//! - **Made at open:** an album through `POST /api/records/open` (C56, `labels::open_record`), an artist through
//!   `POST /api/artists/open` (`open_artist`), never by a name, but for the one live artist of the name who holds no MusicBrainz
//!   id (Z2): the monthly read gives them it by their name anyway.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use serde::Serialize;

use super::discogs::FoundMaster;
use super::discover::{FoundAlbum, FoundArtist};
use super::requests::{Mark, Marks};
use crate::catalogue::Catalogue;
use crate::catalogue::records::{ByReference, Ref};
use crate::serve::ids;

/// An album Beyond lists (C57).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BeyondAlbum {
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub cover: Option<String>,
    /// Its MusicBrainz release group, and Discogs' master with its page there (shown with "Data from Discogs"): what
    /// `POST /api/records/open` takes.
    pub release_group: Option<String>,
    pub discogs: Option<u64>,
    pub discogs_url: Option<String>,
    /// Where it was found: `musicbrainz`, `discogs`, or both where a record ties them.
    pub sources: Vec<&'static str>,
    /// The record that holds its reference (`al-<n>`), as merged now; none until it is opened.
    pub record: Option<String>,
    /// Whether the library has it: the one had rule on its record.
    pub have: bool,
    /// An open request's mark: its record's, else by its release group or names as a list with no record finds one.
    pub request: Option<Mark>,
}

/// An artist Beyond lists (C57): MusicBrainz's, by their MusicBrainz id.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BeyondArtist {
    pub mbid: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disambiguation: Option<String>,
    /// The artist who holds that MusicBrainz id (`ar-<n>`); none until they are opened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    /// Whether the library has albums by that artist.
    pub have: bool,
}

/// What the catalogue says of Beyond's rows (`known`): each MusicBrainz album's record and each Discogs master's, in their order,
/// those records read, and each artist's id and whether the library has albums by them.
#[derive(Debug, Clone, Default)]
pub struct Known {
    pub groups: Vec<Option<i64>>,
    pub masters: Vec<Option<i64>>,
    /// Each MusicBrainz album's and each Discogs master's had record through its names, where no record holds it (Z6).
    pub groups_named: Vec<Option<i64>>,
    pub masters_named: Vec<Option<i64>>,
    pub here: ByReference,
}

/// A MusicBrainz id as MusicBrainz writes it.
fn mbid(id: &str) -> String {
    id.trim().to_ascii_lowercase()
}

/// What the catalogue holds of what Beyond found (C57), by reference alone (`Catalogue::by_reference`), on one reader: nothing is
/// made. Blocking.
pub fn known(cat: &Catalogue, albums: &[FoundAlbum], masters: &[FoundMaster], artists: &[FoundArtist]) -> Result<Known> {
    let refs: Vec<(Ref, &str, &str)> = albums
        .iter()
        .map(|a| (Ref::new("musicbrainz", "release-group", &mbid(&a.release_group)), a.artist.as_str(), a.title.as_str()))
        .chain(masters.iter().map(|d| (Ref::new("discogs", "master", &d.id.to_string()), d.artist.as_str(), d.title.as_str())))
        .collect();
    let people: Vec<Ref> = artists.iter().map(|a| Ref::new("musicbrainz", "artist", &mbid(&a.mbid))).collect();
    let mut here = cat.by_reference(&refs, &people)?;
    let masters = here.records.split_off(albums.len().min(here.records.len()));
    let groups = std::mem::take(&mut here.records);
    let masters_named = here.named.split_off(albums.len().min(here.named.len()));
    let groups_named = std::mem::take(&mut here.named);
    Ok(Known { groups, masters, groups_named, masters_named, here })
}

impl Known {
    /// Whether a record holds this reference.
    fn holds(&self, record: Option<i64>, source: &str, kind: &str, value: &str) -> bool {
        record.and_then(|r| self.here.read.get(&r)).is_some_and(|q| q.refs.iter().any(|(s, k, v)| s == source && k == kind && v.eq_ignore_ascii_case(value)))
    }

    /// Whether a record is had: the one had rule's holder.
    fn had(&self, record: Option<i64>) -> bool {
        record.and_then(|r| self.here.read.get(&r)).is_some_and(|q| q.holder.is_some())
    }

    /// Whether a row is had: its record by the one had rule, else, where no record holds it, through its names (Z6).
    fn row_had(&self, record: Option<i64>, named: &[Option<i64>], i: usize) -> bool {
        match record {
            Some(_) => self.had(record),
            None => named.get(i).copied().flatten().is_some(),
        }
    }
}

/// An album row's request: its record's mark, else as a list with no record id finds one (`requests::mark_of`).
fn request_of(marks: Option<&Marks>, record: Option<i64>, release_group: Option<&str>, artist: &str, title: &str) -> Option<Mark> {
    let marks = marks?;
    match record {
        Some(r) => marks.of_record(r),
        None => super::requests::mark_of(marks, release_group, artist, title),
    }
}

/// Beyond's album rows (C57), MusicBrainz's first, then Discogs' masters, each album once by reference alone: a master goes on a
/// MusicBrainz row only where a record holds both its master and the row's release group (or both are one record), and rows
/// whose records are one record are one row. Never by a name: two albums of one name from two sources are two rows. Each names
/// its record and is had by the one had rule on it; a row no record holds is not had. Pure.
pub fn albums(mb: &[FoundAlbum], discogs: &[FoundMaster], known: &Known, marks: Option<&Marks>) -> Vec<BeyondAlbum> {
    let mut rows: Vec<BeyondAlbum> = Vec::new();
    // Each row's record, and the row each record is on.
    let mut records: Vec<Option<i64>> = Vec::new();
    let mut on: HashMap<i64, usize> = HashMap::new();
    let mut groups: HashSet<String> = HashSet::new();
    for (i, a) in mb.iter().enumerate() {
        let record = known.groups.get(i).copied().flatten();
        if !groups.insert(mbid(&a.release_group)) || record.is_some_and(|r| on.contains_key(&r)) {
            continue;
        }
        if let Some(r) = record {
            on.insert(r, rows.len());
        }
        records.push(record);
        rows.push(BeyondAlbum {
            title: a.title.clone(),
            artist: a.artist.clone(),
            year: a.year,
            cover: Some(a.cover.clone()).filter(|c| !c.is_empty()),
            release_group: Some(a.release_group.clone()),
            discogs: None,
            discogs_url: None,
            sources: vec!["musicbrainz"],
            record: record.map(ids::record),
            have: known.row_had(record, &known.groups_named, i),
            request: request_of(marks, record, Some(a.release_group.as_str()), &a.artist, &a.title),
        });
    }
    let mut masters: HashSet<u64> = HashSet::new();
    for (i, d) in discogs.iter().enumerate() {
        if !masters.insert(d.id) {
            continue;
        }
        let record = known.masters.get(i).copied().flatten();
        let master = d.id.to_string();
        let tied = match record.and_then(|r| on.get(&r).copied()) {
            Some(at) => Some(at),
            None => (0..rows.len()).find(|&j| {
                rows[j].discogs.is_none()
                    && rows[j].release_group.as_deref().is_some_and(|g| known.holds(records[j], "discogs", "master", &master) || known.holds(record, "musicbrainz", "release-group", g))
            }),
        };
        if let Some(at) = tied {
            // The album a MusicBrainz row is, or another master of a record listed already: one album once.
            let row = &mut rows[at];
            if row.discogs.is_none() {
                row.discogs = Some(d.id);
                row.discogs_url = Some(d.url.clone());
                if !row.sources.contains(&"discogs") {
                    row.sources.push("discogs");
                }
            }
            continue;
        }
        if let Some(r) = record {
            on.insert(r, rows.len());
        }
        records.push(record);
        rows.push(BeyondAlbum {
            title: d.title.clone(),
            artist: d.artist.clone(),
            year: d.year,
            cover: d.cover.clone(),
            release_group: None,
            discogs: Some(d.id),
            discogs_url: Some(d.url.clone()),
            sources: vec!["discogs"],
            record: record.map(ids::record),
            have: known.row_had(record, &known.masters_named, i),
            request: request_of(marks, record, None, &d.artist, &d.title),
        });
    }
    rows
}

/// Beyond's artist rows (C57), as MusicBrainz lists them, each by their MusicBrainz id: two of one name are two. Each names the
/// artist who holds that id, had where the library has albums by them; never by a name. Pure.
pub fn artists(found: &[FoundArtist], known: &Known) -> Vec<BeyondArtist> {
    found
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let here = known.here.artists.get(i).copied().flatten();
            BeyondArtist { mbid: a.mbid.clone(), name: a.name.clone(), disambiguation: a.disambiguation.clone(), artist: here.map(|(id, _)| ids::artist(id)), have: here.is_some_and(|(_, had)| had) }
        })
        .collect()
}

/// An artist of Beyond's opened (C57, OQ12 (a): kept when opened): the artist who holds their MusicBrainz id; else the one live
/// artist of the name who holds none (Z2); else one of their own made now, though another has the name (C55: the name's next
/// number). The one opened holds it (`Catalogue::artist_by_reference`). Never by the name alone: without a MusicBrainz id, or a
/// name, none. Blocking.
pub fn open_artist(cat: &Catalogue, id: &str, name: &str) -> Result<Option<i64>> {
    let (id, name) = (mbid(id), name.trim());
    if !super::discogs::is_mbid(&id) || name.is_empty() {
        return Ok(None);
    }
    Ok(Some(cat.artist_by_reference(&Ref::new("musicbrainz", "artist", &id), name, "search")?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROUP_1: &str = "00000000-0000-4000-8000-000000000571";
    const GROUP_2: &str = "00000000-0000-4000-8000-000000000572";
    const GROUP_3: &str = "00000000-0000-4000-8000-000000000573";
    const ARTIST_1: &str = "00000000-0000-4000-8000-000000000581";
    const ARTIST_2: &str = "00000000-0000-4000-8000-000000000582";
    const ARTIST_3: &str = "00000000-0000-4000-8000-000000000583";
    const ARTIST_4: &str = "00000000-0000-4000-8000-000000000584";

    fn album(group: &str, title: &str, artist: &str) -> FoundAlbum {
        FoundAlbum { release_group: group.into(), title: title.into(), artist: artist.into(), year: Some(2020), kind: Some("Album".into()), cover: format!("/api/covers/release-group/{group}"), have: false }
    }

    fn master(id: u64, title: &str, artist: &str) -> FoundMaster {
        FoundMaster { id, title: title.into(), artist: artist.into(), year: Some(2020), cover: None, url: format!("https://www.discogs.com/master/{id}") }
    }

    fn person(id: &str, name: &str) -> FoundArtist {
        FoundArtist { mbid: id.into(), name: name.into(), disambiguation: None, have: false }
    }

    /// A record of this title by Kite Theory holding these references; had with `tracks`.
    fn record(cat: &Catalogue, title: &str, tracks: usize, refs: &[(&str, &str, &str)]) -> i64 {
        let record = cat.record_for_tests(title, tracks);
        cat.db_for_tests().execute("UPDATE records SET artist = 'Kite Theory' WHERE id = ?1", [record]).unwrap();
        for (source, kind, value) in refs {
            cat.set_reference(&ids::record(record), &Ref::new(source, kind, value), "link").unwrap();
        }
        record
    }

    fn rows(cat: &Catalogue, mb: &[FoundAlbum], discogs: &[FoundMaster]) -> Vec<BeyondAlbum> {
        albums(mb, discogs, &known(cat, mb, discogs, &[]).unwrap(), None)
    }

    /// C57: an album MusicBrainz and Discogs both find by one name is two rows, unless a record holds both its release group and
    /// its master; two release groups of one name are two rows; two artists of one name are two.
    #[test]
    fn beyond_never_merges_by_name() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let mb = [album(GROUP_1, "Night Walks", "Kite Theory"), album(GROUP_2, "Night Walks", "Kite Theory")];
        let discogs = [master(3001, "Night Walks", "Kite Theory")];
        let sources = |rows: &[BeyondAlbum]| rows.iter().map(|r| (r.release_group.clone(), r.discogs, r.sources.clone())).collect::<Vec<_>>();
        assert_eq!(
            sources(&rows(&cat, &mb, &discogs)),
            [(Some(GROUP_1.to_string()), None, vec!["musicbrainz"]), (Some(GROUP_2.to_string()), None, vec!["musicbrainz"]), (None, Some(3001), vec!["discogs"])],
            "no reference ties them"
        );
        // A record of the master alone, and another of the release group: still two.
        record(&cat, "Night Walks", 0, &[("discogs", "master", "3001")]);
        let walks = record(&cat, "Night Walks", 0, &[("musicbrainz", "release-group", GROUP_2)]);
        assert_eq!(rows(&cat, &mb, &discogs).len(), 3, "two records, two albums");
        // The release group's record holds the master too: one row, found in both.
        cat.set_reference(&ids::record(walks), &Ref::new("discogs", "master", "3001"), "link").unwrap();
        assert_eq!(
            sources(&rows(&cat, &mb, &discogs)),
            [(Some(GROUP_1.to_string()), None, vec!["musicbrainz"]), (Some(GROUP_2.to_string()), Some(3001), vec!["musicbrainz", "discogs"])],
            "a record ties them"
        );
        let people = [person(ARTIST_1, "Rone"), person(ARTIST_2, "Rone")];
        let listed = artists(&people, &known(&cat, &[], &[], &people).unwrap());
        assert_eq!(listed.iter().map(|a| a.mbid.as_str()).collect::<Vec<_>>(), [ARTIST_1, ARTIST_2], "two of one name are two");
    }

    /// C57: an album row names the record that holds its release group or its master, as merged now, and is had by the one had
    /// rule on it; a row of a had album's names that no reference ties to it names none, and is had through its names alone
    /// (Z6: the untagged Lamplight); artists by their MusicBrainz id alone.
    #[test]
    fn a_beyond_row_names_its_record_by_reference_only() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let had = record(&cat, "Night Walks", 2, &[("musicbrainz", "release-group", GROUP_1)]);
        let wanted = record(&cat, "Paper Kites", 0, &[("discogs", "master", "3002")]);
        let merged = record(&cat, "Undertow", 0, &[("musicbrainz", "release-group", GROUP_2)]);
        let into = record(&cat, "Undertow (Deluxe)", 0, &[]);
        cat.merge_records(merged, into).unwrap();
        // Had by its names, untagged: Beyond's rows of those names name no record, and are had through their names (Z6).
        record(&cat, "Lamplight", 2, &[]);
        let mb = [album(&GROUP_1.to_uppercase(), "Night Walks", "Kite Theory"), album(GROUP_2, "Undertow", "Kite Theory"), album(GROUP_3, "Lamplight", "Kite Theory")];
        let discogs = [master(3002, "Paper Kites", "Kite Theory"), master(3003, "Lamplight", "Kite Theory")];
        let listed = rows(&cat, &mb, &discogs);
        let named: Vec<(&str, Option<String>, bool)> = listed.iter().map(|r| (r.title.as_str(), r.record.clone(), r.have)).collect();
        assert_eq!(
            named,
            [
                ("Night Walks", Some(ids::record(had)), true),
                ("Undertow", Some(ids::record(into)), false),
                ("Lamplight", None, true),
                ("Paper Kites", Some(ids::record(wanted)), false),
                ("Lamplight", None, true),
            ],
            "by reference alone, merges followed; a name alone is no record, and had through its names"
        );
        // An artist by their MusicBrainz id: had where the library has albums by them; one of the same name with no id is not them.
        let theirs = cat.artist_for("Kite Theory").unwrap();
        cat.db_for_tests().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'files')", rusqlite::params![had, theirs]).unwrap();
        let people = [person(ARTIST_1, "Kite Theory")];
        assert_eq!(artists(&people, &known(&cat, &[], &[], &people).unwrap())[0].artist, None, "the library's Kite Theory holds no MusicBrainz id");
        cat.set_reference(&ids::artist(theirs), &Ref::new("musicbrainz", "artist", ARTIST_1), "files").unwrap();
        let listed = artists(&people, &known(&cat, &[], &[], &people).unwrap());
        assert_eq!((listed[0].artist.clone(), listed[0].have), (Some(ids::artist(theirs)), true));
    }

    /// C57 (OQ12 (a)): an artist of Beyond's opened is the artist who holds their MusicBrainz id, else one made then with it, never
    /// one of the same name that holds another id, nor one of two of the name (Z2): a second of a name is numbered 2; the same id
    /// opened again is the same artist.
    #[test]
    fn opening_a_beyond_artist_never_merges_by_name() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let first = open_artist(&cat, ARTIST_1, "Rone").unwrap().expect("made");
        assert_eq!(open_artist(&cat, &ARTIST_1.to_uppercase(), "Rone").unwrap(), Some(first), "the same id: the same artist");
        let second = open_artist(&cat, ARTIST_2, "Rone").unwrap().expect("made");
        assert_ne!(first, second, "another id of the same name: another artist");
        let number = |artist: i64| cat.artist_named(artist).unwrap().unwrap().number;
        assert_eq!((number(first), number(second)), (Some(1), Some(2)));
        // Two artists of a name with no MusicBrainz id: which one, nothing says, so neither (Z2).
        let (theirs, twin) = (cat.artist_for("Kite Theory").unwrap(), cat.artist_apart("Kite Theory").unwrap());
        let opened = open_artist(&cat, ARTIST_3, "Kite Theory").unwrap().expect("made");
        assert!(opened != theirs && opened != twin);
        assert_eq!(number(opened), Some(3));
        // One who holds the id by their files is the one opened.
        let harbour = cat.artist_for("Glass Harbour").unwrap();
        cat.set_reference(&ids::artist(harbour), &Ref::new("musicbrainz", "artist", ARTIST_4), "files").unwrap();
        assert_eq!(open_artist(&cat, ARTIST_4, "Glass Harbour").unwrap(), Some(harbour));
        // Never by the name alone.
        assert_eq!(open_artist(&cat, "", "Rone").unwrap(), None);
        assert_eq!(open_artist(&cat, "not an id", "Rone").unwrap(), None);
        let known_by = |artist: i64| cat.references_of(&ids::artist(artist)).unwrap().into_iter().map(|(r, row)| (r.value, row.how)).collect::<Vec<_>>();
        assert_eq!(known_by(second), [(ARTIST_2.to_string(), "search".to_string())], "their MusicBrainz id kept");
    }

    /// Z2 (the review's finding 2): with no artist holding the id opened, the one live artist of its name who holds no MusicBrainz
    /// artist id is them, given it as `search`, as the monthly read finds them by their name: not a second of the name. One holding
    /// another id, or two of the name, are not: a new artist.
    #[test]
    fn a_beyond_artist_is_the_library_artist_of_its_name_with_no_id() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let grouch = cat.artist_for("Grouch").unwrap();
        assert_eq!(open_artist(&cat, ARTIST_1, "grouch").unwrap(), Some(grouch), "the library's Grouch, not Grouch (2)");
        let holds = |artist: i64| cat.references_of(&ids::artist(artist)).unwrap().into_iter().map(|(r, row)| (r.value, row.how)).collect::<Vec<_>>();
        assert_eq!(holds(grouch), [(ARTIST_1.to_string(), "search".to_string())], "given the id, as the monthly read would");
        let number = |artist: i64| cat.artist_named(artist).unwrap().unwrap().number;
        assert_eq!(number(grouch), Some(1));
        assert_eq!(open_artist(&cat, ARTIST_1, "Grouch").unwrap(), Some(grouch), "held: the same artist");
        let other = open_artist(&cat, ARTIST_2, "Grouch").unwrap().expect("made");
        assert_ne!(other, grouch, "the one of the name holds another id: another artist");
        assert_eq!(number(other), Some(2));
        let (club, twin) = (cat.artist_for("Tape Club").unwrap(), cat.artist_apart("Tape Club").unwrap());
        let made = open_artist(&cat, ARTIST_3, "Tape Club").unwrap().expect("made");
        assert!(made != club && made != twin, "two of the name with no id: neither");
        assert_eq!(number(made), Some(3));
    }

    /// Z2: the files' MusicBrainz artist id takes it from an artist who holds it by `search` alone and is credited on no had record
    /// (one opened from Beyond while the name was two artists'); never from one credited on a had record, nor from one holding it
    /// by their files or by hand.
    #[test]
    fn a_files_artist_id_takes_it_from_a_beyond_artist() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (grouch, twin) = (cat.artist_for("Grouch").unwrap(), cat.artist_apart("Grouch").unwrap());
        let beyond = open_artist(&cat, ARTIST_1, "Grouch").unwrap().expect("made");
        assert!(beyond != grouch && beyond != twin);
        let files = |artist: i64, id: &str| cat.set_reference(&ids::artist(artist), &Ref::new("musicbrainz", "artist", id), "files").unwrap();
        let holds = |artist: i64| cat.references_of(&ids::artist(artist)).unwrap().into_iter().map(|(r, row)| (r.value, row.how)).collect::<Vec<_>>();
        assert!(files(grouch, ARTIST_1), "taken from the Beyond artist");
        assert_eq!(holds(grouch), [(ARTIST_1.to_string(), "files".to_string())]);
        assert!(holds(beyond).is_empty(), "let go of");
        // Credited on a had record: kept.
        let credited = open_artist(&cat, ARTIST_2, "Grouch").unwrap().expect("made");
        let album = record(&cat, "Night Walks", 2, &[]);
        cat.db_for_tests().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'listing')", rusqlite::params![album, credited]).unwrap();
        assert!(!files(twin, ARTIST_2), "credited on a had record: kept");
        assert_eq!(holds(credited), [(ARTIST_2.to_string(), "search".to_string())]);
        // Held by the files, or by hand: kept.
        assert!(!files(twin, ARTIST_1), "held by the files: kept");
        cat.set_reference(&ids::artist(beyond), &Ref::new("musicbrainz", "artist", ARTIST_3), "hand").unwrap();
        assert!(!files(twin, ARTIST_3), "held by hand: kept");
    }

    /// Z6 (the review's finding 6): a row no record holds is had where the had rule's names clause would hold it, a live had record
    /// of its folded artist and title holding no reference that contradicts the row's; it names no record, and nothing is made or
    /// tied. Not where that record holds another release group, nor where the name is two albums' (Y1), nor for another artist.
    #[test]
    fn a_beyond_row_of_an_untagged_owned_album_is_had_and_names_no_record() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        record(&cat, "Lamplight", 2, &[]);
        record(&cat, "Signal", 2, &[("musicbrainz", "release-group", GROUP_2)]);
        record(&cat, "Two Rooms", 2, &[]);
        record(&cat, "Two Rooms", 0, &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000574")]);
        let count = |table: &str| cat.db_for_tests().query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| r.get::<_, i64>(0)).unwrap();
        let before = [count("records"), count("references"), count("record_labels"), count("record_artists")];
        let mb = [
            album(GROUP_1, "Lamplight", "Kite Theory"),
            album(GROUP_3, "Signal", "Kite Theory"),
            album("00000000-0000-4000-8000-000000000575", "Two Rooms", "Kite Theory"),
            album("00000000-0000-4000-8000-000000000576", "Lamplight", "Tape Club"),
        ];
        let discogs = [master(3005, "LAMPLIGHT", "kite theory")];
        let listed = rows(&cat, &mb, &discogs);
        let had: Vec<(&str, &str, Option<String>, bool)> = listed.iter().map(|r| (r.title.as_str(), r.artist.as_str(), r.record.clone(), r.have)).collect();
        assert_eq!(
            had,
            [
                ("Lamplight", "Kite Theory", None, true),
                ("Signal", "Kite Theory", None, false),
                ("Two Rooms", "Kite Theory", None, false),
                ("Lamplight", "Tape Club", None, false),
                ("LAMPLIGHT", "kite theory", None, true),
            ],
            "had through its names alone, folded; another release group, a name of two albums and another artist are not"
        );
        assert_eq!(listed.len(), 5, "no merge by names: the two Lamplights of Kite Theory are two rows");
        assert_eq!([count("records"), count("references"), count("record_labels"), count("record_artists")], before, "nothing made or tied");
    }

    /// C57: Beyond's answer makes nothing: no record, artist or reference, whatever it finds.
    #[test]
    fn beyond_makes_nothing_when_it_answers() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        record(&cat, "Night Walks", 2, &[("musicbrainz", "release-group", GROUP_1)]);
        cat.artist_for("Kite Theory").unwrap();
        let count = |table: &str| cat.db_for_tests().query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| r.get::<_, i64>(0)).unwrap();
        let before = [count("records"), count("artists"), count("references"), count("record_artists")];
        let mb = [album(GROUP_1, "Night Walks", "Kite Theory"), album(GROUP_2, "Undertow", "Kite Theory")];
        let discogs = [master(3001, "Paper Kites", "Kite Theory")];
        let people = [person(ARTIST_1, "Kite Theory"), person(ARTIST_2, "Tape Club")];
        let here = known(&cat, &mb, &discogs, &people).unwrap();
        let (listed, found) = (albums(&mb, &discogs, &here, None), artists(&people, &here));
        assert_eq!((listed.len(), found.len()), (3, 2));
        assert_eq!(listed.iter().filter(|a| a.record.is_some()).count(), 1, "the one a record holds");
        assert!(found.iter().all(|a| a.artist.is_none()), "no artist holds their ids");
        assert_eq!([count("records"), count("artists"), count("references"), count("record_artists")], before, "nothing made");
    }
}
