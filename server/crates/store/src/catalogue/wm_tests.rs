//! SCENARIOS-WM's catalogue half: a file's title from its name and the pairing without tags (WM.25), a stamp naming no record,
//! artist or label (WM.5), and the arrival's identity by stronger evidence (WM.8-WM.17, WM.23), on the catalogue's own rows.

use std::path::PathBuf;

use cratarr_domain::facts::Reference;
use cratarr_rules::holding::Library;
use rusqlite::params;

use super::Catalogue;
use super::holding::Copies;
use super::identity::RefTrack;
use crate::serve::ids;

const MUSIC: i64 = 0;
const IN: i64 = 1;
const IN2: i64 = 5;

pub(super) fn library(folder: i64) -> Library {
    match folder {
        IN => Library::Incoming { fetcher: cratarr_rules::holding::Fetcher::Lidarr, uploader: None },
        IN2 => Library::Incoming { fetcher: cratarr_rules::holding::Fetcher::Soulseek, uploader: None },
        _ => Library::Main,
    }
}

fn catalogue() -> Catalogue {
    Catalogue::open(None, Vec::new()).unwrap()
}

fn record(cat: &Catalogue, title: &str, artist: &str) -> i64 {
    let db = cat.db();
    db.execute("INSERT INTO records (born, title, artist) VALUES (1, ?1, ?2)", [title, artist]).unwrap();
    db.last_insert_rowid()
}

/// A list, in order: (title, length in ms).
fn list(tracks: &[(&str, i64)]) -> Vec<RefTrack> {
    tracks.iter().zip(1..).map(|((title, ms), position)| RefTrack { disc: 1, position, title: title.to_string(), length_ms: Some(*ms), recording: None }).collect()
}

/// A list kept under a reference held `how` (the request's group: "asked").
fn keep(cat: &Catalogue, record: i64, value: &str, tracks: &[RefTrack]) {
    let r = Reference::new("musicbrainz", "release", value);
    cat.db()
        .execute("INSERT INTO \"references\" (subject, source, kind, value, at, how) VALUES (?1, ?2, ?3, ?4, 1, 'asked')", params![ids::record(record), r.source, r.kind, r.value])
        .unwrap();
    cat.keep_list(record, &r, tracks, 1).unwrap();
}

/// The record's tracks, one at each place of its list.
fn tracks(cat: &Catalogue, record: i64, n: u32) -> Vec<i64> {
    let db = cat.db();
    (1..=n)
        .map(|place| {
            db.execute("INSERT INTO tracks (born, record, disc, number) VALUES (1, ?1, 1, ?2)", params![record, place]).unwrap();
            db.last_insert_rowid()
        })
        .collect()
}

/// A file of track `track` at `rel` in `folder`: its length in ms where read, its tags.
fn file(cat: &Catalogue, folder: i64, rel: &str, track: i64, ms: Option<i64>, tags: &[(&str, &str)]) {
    let db = cat.db();
    db.execute("INSERT INTO files (folder, rel, size, mtime, track, duration_ms) VALUES (?1, ?2, 1, 1, ?3, ?4)", params![folder, rel, track, ms]).unwrap();
    for (i, (name, value)) in tags.iter().enumerate() {
        db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (?1, ?2, ?3, ?4, ?4, ?5)", params![folder, rel, i as i64, name, value]).unwrap();
    }
}

fn of(cat: &Catalogue, record: i64) -> Copies {
    cat.copies_of(&[record], &library).unwrap().remove(&record).unwrap_or_default()
}

/// A copy's pairs: its folder's `paired`.
fn paired(c: &Copies, folder: i64) -> u16 {
    c.copies.iter().find(|c| c.folder == folder).map_or(0, |c| c.paired)
}

/// Mumia's list, T1-T4: 3:10, 4:20, 5:30, 6:40, more than 4 s apart.
const MUMIA: [(&str, i64); 4] = [("T1", 190_000), ("T2", 260_000), ("T3", 330_000), ("T4", 400_000)];

/// Mumia `al-3211` with its list kept and MUSIC's copy, tagged; its tracks.
fn mumia(cat: &Catalogue) -> (i64, Vec<i64>) {
    let r = record(cat, "Mumia", "Polo & Pan");
    keep(cat, r, "R3211", &list(&MUMIA));
    let t = tracks(cat, r, 4);
    for ((title, ms), (track, n)) in MUMIA.iter().zip(t.iter().zip(1..)) {
        file(cat, MUSIC, &format!("Polo & Pan/Mumia/0{n} {title}.mp3"), *track, Some(*ms), &[("title", title), ("tracknumber", &n.to_string()), ("album", "Mumia")]);
    }
    (r, t)
}

/// WM.25 (b)-(d), (f) (T-WM-7): a file with no TITLE pairs by its name's titles, else by its length alone within the slack, at its
/// place where its place is known; a file whose TITLE differs never by its length; one with no title and no length nowhere. Red
/// first: `copies_of` read TITLE alone: (b) Partly (T2 "T2 www." and the untitled fourth unpaired), (c) and (d) nothing paired.
#[test]
fn a_file_without_a_title_pairs_by_its_name_or_its_length() {
    let cat = catalogue();
    // (b) the IN2 copy on al-3211: "T1", "T2 www.", "T3", and the fourth untitled, named by the site.
    let (r, t) = mumia(&cat);
    let in2 = [("b.mp3", Some("T1")), ("c.mp3", Some("T2 www.")), ("a.mp3", Some("T3")), ("T4 - www.electronicfresh.com.mp3", None)];
    for ((name, title), (track, (_, ms))) in in2.iter().zip(t.iter().zip(MUMIA)) {
        let tags: Vec<(&str, &str)> = title.iter().map(|t| ("title", *t)).chain([("album", "www.electronicfresh.com")]).collect();
        file(&cat, IN2, &format!("soulseek/Polo & Pan/Mumia (2023) 2/{name}"), *track, Some(ms + 500), &tags);
    }
    let c = of(&cat, r);
    assert_eq!(paired(&c, IN2), 4, "(b) the fourth by its name's title, T2 cleaned: whole");
    assert_eq!(paired(&c, MUSIC), 4);

    // (c) every tag stripped: T4 by its name, T1-T3 by their lengths alone (no title, no place), each the one track within the slack.
    let cat = catalogue();
    let (r, t) = mumia(&cat);
    for (name, (track, (_, ms))) in ["b.mp3", "c.mp3", "a.mp3", "T4 - www.electronicfresh.com.mp3"].iter().zip(t.iter().zip(MUMIA)) {
        file(&cat, IN2, &format!("soulseek/Polo & Pan/Mumia (2023) 2/{name}"), *track, Some(ms - 800), &[]);
    }
    assert_eq!(paired(&of(&cat, r), IN2), 4, "(c) the same pairs as with tags");

    // (d) Tidewater's IN2 copy, every tag stripped: W1-W10 by their names' titles and places.
    let cat = catalogue();
    let tide: Vec<(String, i64)> = (1..=10).map(|n| (format!("W{n}"), 150_000 + n * 9_000)).collect();
    let tide_list: Vec<(&str, i64)> = tide.iter().map(|(t, ms)| (t.as_str(), *ms)).collect();
    let r = record(&cat, "Tidewater", "Kite Theory");
    keep(&cat, r, "R151", &list(&tide_list));
    let t = tracks(&cat, r, 10);
    for ((title, ms), (track, n)) in tide.iter().zip(t.iter().zip(1..)) {
        file(&cat, IN2, &format!("soulseek/Kite Theory/Tidewater/{n:02} {title}.mp3"), *track, Some(*ms), &[]);
    }
    assert_eq!(paired(&of(&cat, r), IN2), 10, "(d) as tagged");

    // (f) on Deep Water's list: a file titled "Breakwater" at track 3's length; "x.flac", no title and no length; "07 x.flac", no
    // title, within track 3's slack but at place 7, which the list has none of. Each alone in its copy: none pairs.
    let cat = catalogue();
    let deep = [("Breakwater", 200_000), ("Sea Wall", 230_000), ("Night Pier", 260_000), ("Harbour Bell", 290_000)];
    let r = record(&cat, "Deep Water", "Glass Harbour");
    keep(&cat, r, "R157", &list(&deep));
    let t = tracks(&cat, r, 4);
    file(&cat, MUSIC, "Glass Harbour/Deep Water/x1.flac", t[2], Some(260_500), &[("title", "Breakwater")]);
    file(&cat, IN, "Glass Harbour/Deep Water/x.flac", t[2], None, &[]);
    file(&cat, IN2, "Glass Harbour/Deep Water/07 x.flac", t[2], Some(260_500), &[]);
    let c = of(&cat, r);
    assert_eq!((paired(&c, MUSIC), paired(&c, IN), paired(&c, IN2)), (0, 0, 0), "a title that differs refuses; no title and no length pairs nowhere; never away from its place");
}

/// A catalogue on real folders (MUSIC, IN; IN2 is the second's place in these tests), its files read as a pass reads them.
struct Folders {
    roots: Vec<PathBuf>,
    cat: Catalogue,
}

impl Folders {
    fn new(name: &str) -> Self {
        let roots: Vec<PathBuf> = [0, 1].iter().map(|n| std::env::temp_dir().join(format!("cratarr-wm-{name}-{n}-{}", std::process::id()))).collect();
        for root in &roots {
            std::fs::remove_dir_all(root).ok();
            std::fs::create_dir_all(root).unwrap();
        }
        let cat = Catalogue::open(None, roots.clone()).unwrap();
        Folders { roots, cat }
    }

    /// These files in `folder` (0 or 1), each (path, its tags), read and identified, each 200 s long.
    fn add(&self, folder: i64, files: &[(&str, Vec<(&str, &str)>)]) {
        let root = &self.roots[usize::try_from(folder).unwrap()];
        for (rel, _) in files {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, rel).unwrap();
        }
        self.cat.walk_all();
        self.cat.pretend_read_for_tests();
        for (rel, tags) in files {
            self.cat.tag_for_tests(folder, rel, tags);
        }
        self.cat.identify().unwrap();
    }

    /// Every album as a build reads them now.
    fn albums(&self) -> Vec<super::AlbumCopy> {
        let folders: Vec<crate::serve::library::FolderRows> = self
            .cat
            .folders()
            .iter()
            .map(|(folder, root)| crate::serve::library::FolderRows { folder, root: root.display().to_string(), rows: self.cat.served_rows(folder).unwrap() })
            .collect();
        crate::serve::library::album_copies(&folders, &super::FolderRules::default())
    }

    /// A record pass over every album.
    fn pass(&self) {
        self.cat.assign_records(&self.albums()).unwrap();
    }

    fn record_of(&self, folder: i64, rel: &str) -> Option<i64> {
        let track = self.cat.track_of(folder, rel)?;
        self.cat.db().query_row("SELECT record FROM tracks WHERE id = ?1", [track], |r| r.get(0)).ok().flatten()
    }
}

impl Drop for Folders {
    fn drop(&mut self) {
        self.roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }
}

const STAMP: &str = "www.electronicfresh.com";

/// WM.5: a stamp never names a record, an artist or a label. (a) a folder alone, every ALBUM the site: its record is titled as a
/// file with no ALBUM titles it, never the site; (b) a record of two copies whose first-ranked copy's ALBUM is the site stays
/// "Mumia", the next copy that names it speaking (FA2); (c) an ALBUMARTIST "flacsite.net" beside ARTIST "Kite Theory": the album's
/// artist is Kite Theory, and no artist "flacsite.net" is born; (d) a LABEL that is the site: no label. Red first: each named by
/// the stamp (production's `al-3215`, "www.electronicfresh.com").
#[test]
fn a_stamp_names_no_record_artist_or_label() {
    // (a), against its twin with no ALBUM at all.
    let titled = |album: Option<&str>| -> String {
        let f = Folders::new(if album.is_some() { "wm5a" } else { "wm5a-twin" });
        let tags: Vec<(&str, &str)> = album.map(|a| ("album", a)).into_iter().chain([("artist", "Red Axes, Polo & Pan"), ("title", "T1")]).collect();
        f.add(1, &[("soulseek/Polo & Pan/Mumia (2023) 2/b.mp3", tags)]);
        f.pass();
        let r = f.record_of(1, "soulseek/Polo & Pan/Mumia (2023) 2/b.mp3").expect("a record");
        f.cat.record(r).unwrap().unwrap().title
    };
    let (stamped, twin) = (titled(Some(STAMP)), titled(None));
    assert_eq!(stamped, twin, "(a) titled as a file with no ALBUM");
    assert!(!stamped.contains("electronicfresh"));

    // (b) MUSIC's "Mumia", then the stamped IN copy holding its tracks, ranked first.
    let f = Folders::new("wm5b");
    let music: Vec<(String, Vec<(&str, String)>)> = (1..=2).map(|n| (format!("Polo & Pan/Mumia/0{n} T{n}.mp3"), vec![("album", "Mumia".to_string()), ("albumartist", "Polo & Pan".to_string()), ("title", format!("T{n}")), ("tracknumber", n.to_string())])).collect();
    for (rel, tags) in &music {
        let tags: Vec<(&str, &str)> = tags.iter().map(|(n, v)| (*n, v.as_str())).collect();
        f.add(0, &[(rel.as_str(), tags)]);
    }
    f.pass();
    let r = f.record_of(0, "Polo & Pan/Mumia/01 T1.mp3").unwrap();
    f.add(1, &[("soulseek/Mumia/b.mp3", vec![("album", STAMP), ("artist", "Polo & Pan"), ("title", "T1")]), ("soulseek/Mumia/c.mp3", vec![("album", STAMP), ("artist", "Polo & Pan"), ("title", "T2")])]);
    for (n, rel) in ["soulseek/Mumia/b.mp3", "soulseek/Mumia/c.mp3"].iter().enumerate() {
        let track = f.cat.track_of(0, &music[n].0).unwrap();
        f.cat.give_track_for_tests(1, rel, track);
    }
    let mut albums = f.albums();
    albums.iter_mut().filter(|a| a.folder == 1).for_each(|a| a.rank = 9);
    f.cat.keep_files_names(&albums).unwrap();
    assert_eq!(f.cat.record(r).unwrap().unwrap().title, "Mumia", "(b) a copy that names nothing gives way");

    // (c) and (d).
    let f = Folders::new("wm5c");
    f.add(1, &[("Kite Theory/Tidewater/01 W1.flac", vec![("album", "Tidewater"), ("albumartist", "flacsite.net"), ("artist", "Kite Theory"), ("title", "W1"), ("label", STAMP)])]);
    f.pass();
    let r = f.record_of(1, "Kite Theory/Tidewater/01 W1.flac").unwrap();
    let kite = f.cat.artist_of("Kite Theory").unwrap().expect("(c) Kite Theory credited");
    assert_eq!(f.cat.record_artists(r).unwrap(), [kite], "(c) the album's artist from the credit");
    assert_eq!(f.cat.artist_of("flacsite.net").unwrap(), None, "(c) no artist born of a stamp");
    assert!(f.cat.record_labels(r).unwrap().is_empty(), "(d) no label");
}

/// Every Rust source under a crate's `src`, by its path from `crates/`.
fn sources(dir: &std::path::Path, from: &std::path::Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).unwrap().map(Result::unwrap) {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, from, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let name = path.strip_prefix(from).unwrap().to_string_lossy().replace(char::from(92), "/");
            out.push((name, std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n")));
        }
    }
}

/// WM.7 (kind 6): one rule, in rules, where a file's tags become facts. Every server crate's read of a fact field's first value or
/// values (`first("album")`, `all("title")`, … WM.2's list) goes through the rules' `hygiene` (`Field::…`, the library's `fact`, the
/// catalogue's `fact_value`), but for those named here with their reason; the stamps table is one file, and no second list of
/// domains. Red first: no rule, every reader raw.
#[test]
fn fact_fields_are_read_through_one_rule() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&crates).unwrap().map(Result::unwrap) {
        let src = entry.path().join("src");
        if src.is_dir() {
            sources(&src, &crates, &mut files);
        }
    }
    assert!(files.iter().any(|(name, _)| name == "store/src/serve/library.rs"), "the readers are read by path");
    // Where a raw read stays, and why.
    let allowed: [(&str, &str); 6] = [
        ("store/src/serve/ids.rs", "the tag hash and the persistent ids read the raw tags, so no group or id moves by the rule (T-WM-2)"),
        ("store/src/serve/library.rs", "song()'s `raw_` album artist: the tag hash's (T-WM-2)"),
        ("base/src/app/renumber_base.rs", "Renumber's plan: a write, quiet until 0.19's writeback"),
        ("core/src/naming.rs", "the naming inference counts files with a TITLE, a presence and no fact (PL1's `layout_of`, moved from settings.rs)"),
        ("app/src/fixtures.rs", "the end-to-end fixtures write tags"),
        ("store/src/catalogue/wm_tests.rs", "this scan"),
    ];
    let fields = ["album", "title", "albumartist", "albumartists", "artist", "artists", "label", "catalognumber"];
    let mut raw = Vec::new();
    for (name, text) in &files {
        let tests_at = text.find("#[cfg(test)]\nmod tests").unwrap_or(text.len());
        for (n, line) in text[..tests_at].lines().enumerate() {
            let reads = fields.iter().any(|f| line.contains(&format!(".first(\"{f}\")")) || line.contains(&format!(".all(\"{f}\")")));
            let cleaned = ["hygiene", "Field::", "fact(", "fact_value(", "track_title("].iter().any(|m| line.contains(m));
            let why = allowed.iter().find(|(file, _)| file == name).map(|(_, why)| *why);
            let raw_hash = name == "store/src/serve/library.rs" && line.contains("raw_");
            if reads && !cleaned && (why.is_none() || (name == "store/src/serve/library.rs" && !raw_hash)) && !name.ends_with("_tests.rs") {
                raw.push(format!("{name}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(raw.is_empty(), "fact fields read around the one rule:\n{}", raw.join("\n"));
    // One stamps table: shared/stamps.json, read by the rules crate; no second list of domains in the server's sources.
    let lists: Vec<&String> = files.iter().filter(|(_, text)| text.contains("\"online\", \"site\"") || text.contains("\"xyz\", \"club\"")).map(|(name, _)| name).collect();
    assert!(lists.is_empty(), "a second list of domains: {lists:?}");
    assert!(cratarr_rules::hygiene::STAMPS.contains("\"domains\""));
}

/// WM.4, a guard: the catalogue's `tags` rows hold every value as read, the stamps among them, through a pass and a build; the
/// facts are worked out from those rows at each read, never stored in their place, and the file is never opened for writing.
#[test]
fn the_raw_tag_stays_and_the_facts_follow_the_rule() {
    let f = Folders::new("wm4");
    let rel = "Kelpe/Drome/01 Drome.flac";
    let tags = vec![("album", STAMP), ("albumartist", STAMP), ("title", "Drome www."), ("label", STAMP), ("comment", "Visit https://kelpe.bandcamp.com")];
    f.add(1, &[(rel, tags.clone())]);
    let rows = || -> Vec<(String, String)> {
        f.cat.db().prepare("SELECT name, value FROM tags WHERE folder = 1 AND rel = ?1 ORDER BY n").unwrap().query_map([rel], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
    };
    let before = rows();
    let bytes = std::fs::read(f.roots[1].join(rel)).unwrap();
    f.pass();
    let lib = crate::serve::library::Library::open(None).unwrap();
    crate::serve::library::testing::build_now(&lib, &f.cat);
    assert_eq!(rows(), before, "every raw value as read, the stamps among them");
    assert_eq!(before.iter().map(|(n, v)| (n.as_str(), v.as_str())).collect::<Vec<_>>(), tags, "nothing cleaned in place");
    assert_eq!(std::fs::read(f.roots[1].join(rel)).unwrap(), bytes, "the file as it was");
    let albums = f.albums();
    assert_eq!((albums[0].tracks.len(), albums[0].facts.title.as_str()), (1, ""), "the facts follow the rule at each read");
    let title: String = lib.db().query_row("SELECT title FROM songs", [], |r| r.get(0)).unwrap();
    assert_eq!(title, "Drome");
}

impl Folders {
    /// These files' lengths, as a read gives them.
    fn lengths(&self, folder: i64, files: &[(&str, i64)]) {
        for (rel, ms) in files {
            self.cat.db().execute("UPDATE files SET duration_ms = ?3 WHERE folder = ?1 AND rel = ?2", params![folder, rel, ms]).unwrap();
        }
    }

    fn records(&self) -> i64 {
        self.cat.db().query_row("SELECT count(*) FROM records", [], |r| r.get(0)).unwrap()
    }
}

const MUMIA_DIR: &str = "soulseek/Polo & Pan/Mumia (2023) 2";

/// Mumia `al-3211` in MUSIC (folder 0): T1-T4 tagged, its list kept (`G135`, "asked"). The record.
fn mumia_in_music(f: &Folders) -> i64 {
    let files: Vec<(String, Vec<(&str, String)>)> = MUMIA
        .iter()
        .zip(1..)
        .map(|((title, _), n)| (format!("Polo & Pan/Mumia/0{n} {title}.mp3"), vec![("album", "Mumia".to_string()), ("albumartist", "Polo & Pan".to_string()), ("title", title.to_string()), ("tracknumber", n.to_string())]))
        .collect();
    for (rel, tags) in &files {
        f.add(MUSIC, &[(rel.as_str(), tags.iter().map(|(n, v)| (*n, v.as_str())).collect())]);
    }
    f.lengths(MUSIC, &files.iter().zip(MUMIA).map(|((rel, _), (_, ms))| (rel.as_str(), ms)).collect::<Vec<_>>());
    f.pass();
    let r = f.record_of(MUSIC, &files[0].0).unwrap();
    keep(&f.cat, r, "R3211", &list(&MUMIA));
    r
}

/// `ss-140`'s folder in folder 1 (IN2 here), as the real case has it, or every tag stripped: `b.mp3` T1, `c.mp3` "T2 www.", `a.mp3`
/// T3, and the fourth untitled, named by the site, each within a second of its track.
fn electronicfresh(f: &Folders, stripped: bool) -> Vec<String> {
    let files = [("b.mp3", Some("T1")), ("c.mp3", Some("T2 www.")), ("a.mp3", Some("T3")), ("T4 - www.electronicfresh.com.mp3", None)];
    let rels: Vec<String> = files.iter().map(|(name, _)| format!("{MUMIA_DIR}/{name}")).collect();
    for (rel, (_, title)) in rels.iter().zip(files) {
        let mut tags = vec![("album", STAMP), ("artist", "Red Axes, Polo & Pan")];
        tags.extend(title.map(|t| ("title", t)));
        f.add(1, &[(rel.as_str(), if stripped { Vec::new() } else { tags })]);
    }
    f.lengths(1, &rels.iter().zip(MUMIA).map(|(rel, (_, ms))| (rel.as_str(), ms + 600)).collect::<Vec<_>>());
    rels
}

/// WM.10 (a), live (T-WM-3): the real case. Mumia's MUSIC copy holds T1-T4; `ss-140`'s placement names `al-3211`; the pass reads the
/// stamped folder: one record, two copies, each IN2 file the track it pairs with (the fourth by its name's title), titled "Mumia";
/// and the same verdict with every tag stripped from the IN2 files. Red first: two records, the second titled by the site (the
/// expectation counted only for a record with no track: `open`).
#[test]
fn a_second_copy_joins_its_items_record() {
    for stripped in [false, true] {
        let f = Folders::new(if stripped { "wm10-stripped" } else { "wm10" });
        let r = mumia_in_music(&f);
        f.cat.expect_record(1, MUMIA_DIR, r).unwrap();
        let before = f.records();
        let rels = electronicfresh(&f, stripped);
        f.pass();
        for (rel, n) in rels.iter().zip(1..) {
            let music = format!("Polo & Pan/Mumia/0{n} T{n}.mp3");
            assert_eq!(f.cat.track_of(1, rel), f.cat.track_of(MUSIC, &music), "stripped {stripped}: {rel} is T{n}");
        }
        assert_eq!(f.record_of(1, &rels[0]), Some(r));
        assert_eq!(f.records(), before, "no record made of it");
        assert_eq!(f.cat.record(r).unwrap().unwrap().title, "Mumia");
        let c = f.cat.copies_of(&[r], &library).unwrap().remove(&r).unwrap();
        assert_eq!(c.copies.iter().map(|c| (c.folder, c.paired)).collect::<Vec<_>>(), [(MUSIC, 4), (1, 4)], "stripped {stripped}");
    }
}

/// WM.8 (f): the placement names `al-3211`; the IN2 files' MUSICBRAINZ_ALBUMID names `R156`, `al-156`'s reference (not had), whose
/// list pairs with none of them. The pass puts the copy on `al-3211`. Red first: `open` refused the expectation (al-3211 holds tracks),
/// and the files' release joined the copy to `al-156`.
#[test]
fn provenance_outweighs_the_files_release() {
    let f = Folders::new("wm8f");
    let r = mumia_in_music(&f);
    let r156 = "00000000-0000-4000-8000-000000000156";
    let remixes = record(&f.cat, "Mumia Remixes", "Polo & Pan");
    f.cat.db()
        .execute("INSERT INTO \"references\" (subject, source, kind, value, at, how) VALUES (?1, 'musicbrainz', 'release', ?2, 1, 'asked')", params![ids::record(remixes), r156])
        .unwrap();
    f.cat.keep_list(remixes, &Reference::new("musicbrainz", "release", r156), &list(&[("Remix 1", 300_000), ("Remix 2", 310_000)]), 1).unwrap();
    f.cat.place_copy(1, MUMIA_DIR, r).unwrap();
    let rels = electronicfresh(&f, false);
    for rel in &rels {
        let mut tags: Vec<(String, String)> = f.cat.db().prepare("SELECT name, value FROM tags WHERE folder = 1 AND rel = ?1 ORDER BY n").unwrap().query_map([rel], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
        tags.push(("musicbrainz_albumid".into(), r156.into()));
        f.cat.tag_for_tests(1, rel, &tags.iter().map(|(n, v)| (n.as_str(), v.as_str())).collect::<Vec<_>>());
    }
    f.pass();
    assert_eq!(f.record_of(1, &rels[0]), Some(r), "the placement's record");
    assert!(!f.cat.db().prepare("SELECT 1 FROM tracks WHERE record = ?1").unwrap().exists([remixes]).unwrap(), "never al-156");
}

/// WM.11 (OQ-WM-2 (a)): Tidewater keeps W1-W10 and MUSIC holds W1-W7; Y2's second item placed W1-W10 in IN2, ALBUM the site, files
/// `01 W1.mp3` … : one record, its second copy, each file the track it pairs with, MUSIC's W1-W7 its own. Red first: a record made by
/// the tags, "www.mp3site.ru".
#[test]
fn an_arrival_completes_a_record_partly_there() {
    let f = Folders::new("wm11");
    let tide: Vec<(String, i64)> = (1..=10).map(|n| (format!("W{n}"), 150_000 + n * 9_000)).collect();
    let music: Vec<String> = (1..=7).map(|n| format!("Kite Theory/Tidewater/{n:02} W{n}.flac")).collect();
    for (rel, n) in music.iter().zip(1..) {
        f.add(MUSIC, &[(rel.as_str(), vec![("album", "Tidewater"), ("albumartist", "Kite Theory"), ("title", tide[n - 1].0.as_str()), ("tracknumber", n.to_string().as_str())])]);
    }
    f.lengths(MUSIC, &music.iter().zip(&tide).map(|(rel, (_, ms))| (rel.as_str(), *ms)).collect::<Vec<_>>());
    f.pass();
    let r = f.record_of(MUSIC, &music[0]).unwrap();
    keep(&f.cat, r, "R151", &list(&tide.iter().map(|(t, ms)| (t.as_str(), *ms)).collect::<Vec<_>>()));
    let dir = "soulseek/Kite Theory/Tidewater";
    f.cat.place_copy(1, dir, r).unwrap();
    let before = f.records();
    let arrived: Vec<String> = (1..=10).map(|n| format!("{dir}/{n:02} W{n}.mp3")).collect();
    for rel in &arrived {
        f.add(1, &[(rel.as_str(), vec![("album", "www.mp3site.ru"), ("artist", "Kite Theory")])]);
    }
    f.lengths(1, &arrived.iter().zip(&tide).map(|(rel, (_, ms))| (rel.as_str(), ms + 300)).collect::<Vec<_>>());
    f.pass();
    assert_eq!(f.records(), before, "no record, no edition made of it");
    for (n, rel) in arrived.iter().enumerate() {
        assert_eq!(f.record_of(1, rel), Some(r), "{rel}");
        if n < 7 {
            assert_eq!(f.cat.track_of(1, rel), f.cat.track_of(MUSIC, &music[n]), "{rel}: MUSIC's track");
        }
    }
    let c = f.cat.copies_of(&[r], &library).unwrap().remove(&r).unwrap();
    assert_eq!(c.copies.iter().map(|c| (c.folder, c.paired)).collect::<Vec<_>>(), [(MUSIC, 7), (1, 10)]);
    assert_eq!(f.cat.record(r).unwrap().unwrap().title, "Tidewater");
}

/// WM.12 (H6, VR36, OQ-WM-3 (a)): Long Pier keeps MusicBrainz's ten and Discogs' twelve, both timed; Lidarr's ten became its copy
/// first; then `ss-152`'s twelve, placed for it, every ALBUM the site: another record, linked to Long Pier as its edition by the
/// placement, named "Long Pier" by Kite Theory by the rule, with no list of its own; the second pass, the item and the week gone,
/// changes nothing. Red first: a record titled by the site, and no card (editions need a group or a title key).
#[test]
fn a_differing_arrival_is_an_edition_by_its_item() {
    let f = Folders::new("wm12");
    let pier: Vec<(String, i64)> = (1..=12).map(|n| (format!("P{n}"), 150_000 + n * 9_000)).collect();
    let r = record(&f.cat, "Long Pier", "Kite Theory");
    let refs = |n: usize| pier[..n].iter().map(|(t, ms)| (t.as_str(), *ms)).collect::<Vec<_>>();
    keep(&f.cat, r, "R152", &list(&refs(10)));
    f.cat.db().execute("INSERT INTO \"references\" (subject, source, kind, value, at, how) VALUES (?1, 'discogs', 'release', 'D152', 1, 'asked')", [ids::record(r)]).unwrap();
    f.cat.keep_list(r, &Reference::new("discogs", "release", "D152"), &list(&refs(12)), 1).unwrap();
    // Lidarr's ten, expected for it.
    f.cat.expect_record(MUSIC, "Kite Theory/Long Pier", r).unwrap();
    let lidarr: Vec<String> = (1..=10).map(|n| format!("Kite Theory/Long Pier/{n:02} P{n}.flac")).collect();
    for (rel, n) in lidarr.iter().zip(1..) {
        f.add(MUSIC, &[(rel.as_str(), vec![("album", "Long Pier"), ("albumartist", "Kite Theory"), ("title", pier[n - 1].0.as_str()), ("tracknumber", n.to_string().as_str())])]);
    }
    f.lengths(MUSIC, &lidarr.iter().zip(&pier).map(|(rel, (_, ms))| (rel.as_str(), *ms)).collect::<Vec<_>>());
    f.pass();
    assert_eq!(f.record_of(MUSIC, &lidarr[0]), Some(r));
    let dir = "soulseek/Kite Theory/Long Pier";
    f.cat.place_copy(1, dir, r).unwrap();
    let arrived: Vec<String> = (1..=12).map(|n| format!("{dir}/{n:02} P{n}.flac")).collect();
    for rel in &arrived {
        f.add(1, &[(rel.as_str(), vec![("album", STAMP), ("artist", "Kite Theory")])]);
    }
    f.lengths(1, &arrived.iter().zip(&pier).map(|(rel, (_, ms))| (rel.as_str(), ms + 300)).collect::<Vec<_>>());
    f.pass();
    let edition = f.record_of(1, &arrived[0]).expect("a record");
    assert_ne!(edition, r, "never one record of the ten and the twelve");
    assert!(arrived.iter().all(|rel| f.record_of(1, rel) == Some(edition)));
    let row = f.cat.record(edition).unwrap().unwrap();
    assert_eq!((row.title.as_str(), row.artist.as_str()), ("Long Pier", "Kite Theory"), "named by the record it was fetched for");
    let link: (i64, String) = f.cat.db().query_row("SELECT of, how FROM edition_links WHERE record = ?1", [edition], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(link, (r, "placement".to_string()));
    assert!(f.cat.reference_tracks(edition).unwrap().is_empty(), "Long Pier's lists stay its own");
    assert_eq!(f.cat.reference_tracks(r).unwrap().len(), 2);
    // The item cleared, the week passed: the expectation gone, the second pass changes nothing.
    f.cat.db().execute("DELETE FROM expected", []).unwrap();
    f.pass();
    assert_eq!((f.record_of(1, &arrived[0]), f.record_of(MUSIC, &lidarr[0])), (Some(edition), Some(r)));
    assert_eq!(f.cat.db().query_row("SELECT count(*) FROM edition_links", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    // Every surface shows its record's names (WM.3, OQ-WM-3 (a)): the served album of the edition, never the site or "[Unknown Album]".
    let lib = crate::serve::library::Library::open(None).unwrap();
    crate::serve::library::testing::build_now(&lib, &f.cat);
    let name: String = lib.db().query_row("SELECT name FROM albums WHERE id = ?1", [ids::record(edition)], |r| r.get(0)).unwrap();
    assert_eq!(name, "Long Pier");
    // One card: the two are editions of one album.
    let groups = f.cat.edition_groups(&[r, edition], &[MUSIC], &[]).unwrap();
    assert_eq!(groups, [vec![r, edition]], "one card, two versions");
}

/// WM.13 (T-WM-3): the placement row alone (no expectation, the item gone) is the evidence, at the first read a fortnight later and
/// at every pass after; a placement whose folder holds none of its files decides nothing for them. Red first: the expectation let go
/// after its week, and nothing else kept the placing.
#[test]
fn provenance_outlives_the_expectation_and_the_item() {
    let f = Folders::new("wm13");
    let r = mumia_in_music(&f);
    f.cat.place_copy(1, MUMIA_DIR, r).unwrap();
    let rels = electronicfresh(&f, false);
    f.pass();
    assert_eq!(f.record_of(1, &rels[0]), Some(r), "(a), (b): the row alone");
    f.pass();
    assert_eq!(f.record_of(1, &rels[3]), Some(r), "(c) a later pass");
    // (d) the files moved outside Cratarr into another IN2 folder: the placement names a folder that holds none of them.
    let g = Folders::new("wm13d");
    let r = mumia_in_music(&g);
    g.cat.place_copy(1, "soulseek/Polo & Pan/Elsewhere", r).unwrap();
    let rels = electronicfresh(&g, false);
    g.pass();
    assert_ne!(g.record_of(1, &rels[0]), Some(r), "(d) the next evidence decides");
}

/// WM.9 (a), (b): `al-150`, not had, its list kept; `ss-150` placed its folder, ALBUM "Harbour Lights [www.flacsite.net]": the copy is
/// `al-150`'s, titled "Harbour Lights"; merged into `al-149` before the pass, it is `al-149`'s. Red first: the title read with the
/// stamp. The join is a pin (C50).
#[test]
fn an_arrival_joins_its_items_record_and_drops_the_stamp() {
    for merged in [false, true] {
        let f = Folders::new(if merged { "wm9b" } else { "wm9a" });
        let lights: Vec<(String, i64)> = (1..=8).map(|n| (format!("H{n}"), 170_000 + n * 6_000)).collect();
        let r = record(&f.cat, "Harbour Lights", "Glass Harbour");
        keep(&f.cat, r, "R150", &list(&lights.iter().map(|(t, ms)| (t.as_str(), *ms)).collect::<Vec<_>>()));
        let dir = "soulseek/Glass Harbour/Harbour Lights";
        f.cat.expect_record(1, dir, r).unwrap();
        let into = record(&f.cat, "Harbour Lights", "Glass Harbour");
        if merged {
            f.cat.merge_records(r, into).unwrap();
        }
        let rels: Vec<String> = (1..=8).map(|n| format!("{dir}/{n:02} H{n}.flac")).collect();
        for (rel, (title, _)) in rels.iter().zip(&lights) {
            f.add(1, &[(rel.as_str(), vec![("album", "Harbour Lights [www.flacsite.net]"), ("albumartist", "Glass Harbour"), ("title", title)])]);
        }
        f.lengths(1, &rels.iter().zip(&lights).map(|(rel, (_, ms))| (rel.as_str(), *ms)).collect::<Vec<_>>());
        f.pass();
        let owner = if merged { into } else { r };
        assert_eq!(f.record_of(1, &rels[0]), Some(owner), "merged {merged}");
        assert_eq!(f.cat.record(owner).unwrap().unwrap().title, "Harbour Lights", "the stamp cut");
    }
}

/// Shore Notes `al-154`'s six, N1-N6 (lengths), as files of `dir`, ARTIST "Glass Harbour", every ALBUM `album`.
fn shore_notes(f: &Folders, folder: i64, dir: &str, album: Option<&str>) -> Vec<String> {
    let rels: Vec<String> = (1..=6).map(|n| format!("{dir}/{n:02} N{n}.flac")).collect();
    for (rel, n) in rels.iter().zip(1..) {
        let mut tags = vec![("artist", "Glass Harbour"), ("title", ["N1", "N2", "N3", "N4", "N5", "N6"][n - 1])];
        tags.extend(album.map(|a| ("album", a)));
        f.add(folder, &[(rel.as_str(), tags)]);
    }
    f.lengths(folder, &rels.iter().zip(1..).map(|(rel, n)| (rel.as_str(), 180_000 + n * 7_000)).collect::<Vec<_>>());
    rels
}

fn shore_list() -> Vec<RefTrack> {
    let names: Vec<(String, i64)> = (1..=6).map(|n| (format!("N{n}"), 180_000 + n * 7_000)).collect();
    list(&names.iter().map(|(t, ms)| (t.as_str(), *ms)).collect::<Vec<_>>())
}

/// WM.15 (T-WM-4): no item, no ids, the album a site's stamp: (a) the copy is `al-154`'s (not had, its list kept) by its artist and
/// its tracks; one whose album is a real title that differs ("Shore Notes II") never; (b) `al-154` had in MUSIC, a copy dropped into
/// IN: F11's join reads the stamp as no title and joins by the artist and every track paired. Red first: (a) `wanted_record` needed
/// the folded title, (b) F11 refused on the title key.
#[test]
fn titles_and_lengths_join_a_copy_whose_album_is_a_stamp() {
    let f = Folders::new("wm15a");
    let r = record(&f.cat, "Shore Notes", "Glass Harbour");
    keep(&f.cat, r, "R154", &shore_list());
    let rels = shore_notes(&f, 1, "Glass Harbour/Shore Notes", Some(STAMP));
    f.pass();
    assert_eq!(f.record_of(1, &rels[0]), Some(r), "(a) by its artist and its tracks");
    let g = Folders::new("wm15a2");
    let r = record(&g.cat, "Shore Notes", "Glass Harbour");
    keep(&g.cat, r, "R154", &shore_list());
    let rels = shore_notes(&g, 1, "Glass Harbour/Shore Notes II", Some("Shore Notes II"));
    g.pass();
    assert_ne!(g.record_of(1, &rels[0]), Some(r), "a title that differs speaks (X8)");

    // (b) had in MUSIC, its files naming no release; the stamped copy dropped into IN.
    let h = Folders::new("wm15b");
    let music = shore_notes(&h, MUSIC, "Glass Harbour/Shore Notes", Some("Shore Notes"));
    h.pass();
    let had = h.record_of(MUSIC, &music[0]).unwrap();
    let dropped = shore_notes(&h, 1, "Glass Harbour/Shore Notes (IN)", Some(STAMP));
    h.pass();
    let arrived = h.record_of(1, &dropped[0]).unwrap();
    let kinds = [(MUSIC, crate::settings::LibraryKind::Main), (1, crate::settings::LibraryKind::Incoming)];
    assert_eq!(h.cat.editions_of(&[arrived], &[MUSIC]).unwrap().get(&arrived), Some(&vec![had]), "(b) asked by its artist, its title none");
    assert_eq!(h.cat.join_copies(arrived, had, &kinds).unwrap(), Some((arrived.max(had), arrived.min(had))), "(b) one record, two copies");
    assert_eq!(h.record_of(1, &dropped[0]), Some(had));
}

/// WM.17 (a) with COU.17 (T-COU-16): `ss-158` and `ss-159` placed "Ocean Days" and "Glass Garden" for `al-158` and `al-159`, every file
/// ALBUM the site (and, (b), none at all), ARTIST "Polo & Pan", no ALBUMARTIST, no DATE: two copies, two records, each gathered by
/// its placement, no editions of each other; (e) a two-disc arrival under one placing folder is one copy. Red first: IN2's folder in
/// front of the hash, never the directory: one album of both folders.
#[test]
fn placed_arrivals_with_one_stamp_are_two_records_and_no_editions() {
    for no_album in [false, true] {
        let f = Folders::new(if no_album { "cou17b" } else { "wm17a" });
        let days: Vec<(String, i64)> = (1..=8).map(|n| (format!("O{n}"), 160_000 + n * 6_000)).collect();
        let garden: Vec<(String, i64)> = (1..=9).map(|n| (format!("G{n}"), 210_000 + n * 6_000)).collect();
        let (ocean, glass) = (record(&f.cat, "Ocean Days", "Polo & Pan"), record(&f.cat, "Glass Garden", "Polo & Pan"));
        keep(&f.cat, ocean, "R158", &list(&days.iter().map(|(t, ms)| (t.as_str(), *ms)).collect::<Vec<_>>()));
        keep(&f.cat, glass, "R159", &list(&garden.iter().map(|(t, ms)| (t.as_str(), *ms)).collect::<Vec<_>>()));
        let mut firsts = Vec::new();
        for (record, dir, tracks) in [(ocean, "soulseek/Polo & Pan/Ocean Days", &days), (glass, "soulseek/Polo & Pan/Glass Garden", &garden)] {
            f.cat.place_copy(1, dir, record).unwrap();
            let rels: Vec<String> = tracks.iter().enumerate().map(|(i, (t, _))| format!("{dir}/{:02} {t}.mp3", i + 1)).collect();
            for (rel, (title, _)) in rels.iter().zip(tracks.iter()) {
                let mut tags = vec![("artist", "Polo & Pan"), ("title", title.as_str())];
                if !no_album {
                    tags.push(("album", STAMP));
                }
                f.add(1, &[(rel.as_str(), tags)]);
            }
            f.lengths(1, &rels.iter().zip(tracks.iter()).map(|(rel, (_, ms))| (rel.as_str(), *ms)).collect::<Vec<_>>());
            firsts.push((record, rels));
        }
        assert_eq!(f.albums().iter().filter(|a| a.folder == 1).count(), 2, "each gathered by its placement");
        f.pass();
        for (record, rels) in &firsts {
            assert!(rels.iter().all(|rel| f.record_of(1, rel) == Some(*record)), "no_album {no_album}: each its own record");
        }
        assert!(f.cat.edition_groups(&[ocean, glass], &[], &[]).unwrap().is_empty(), "no editions of each other");
    }
    // (e) two discs under one placing folder: one copy.
    let f = Folders::new("cou17e");
    let r = record(&f.cat, "Long Water", "Glass Harbour");
    let dir = "soulseek/Glass Harbour/Long Water";
    f.cat.place_copy(1, dir, r).unwrap();
    f.add(1, &[(format!("{dir}/CD1/01 Intro.flac").as_str(), vec![("album", "Long Water"), ("discnumber", "1"), ("title", "Intro")]), (format!("{dir}/CD2/01 Tide.flac").as_str(), vec![("album", "Long Water (Disc 2)"), ("discnumber", "2"), ("title", "Tide")])]);
    assert_eq!(f.albums().iter().filter(|a| a.folder == 1).count(), 1, "(e) one copy across its disc folders");
}

/// WM.17 (b), (c), guards: the hand stands. A copy whose tracks a record holds (as a hand's split or join left them) stays that
/// record's though a placement names another: provenance judges only an album no record holds.
#[test]
fn a_stamp_never_joins_and_the_hand_stands() {
    let f = Folders::new("wm17bc");
    let r = mumia_in_music(&f);
    let rels = electronicfresh(&f, false);
    f.pass();
    let kept = f.record_of(1, &rels[0]).expect("a record of its own, as the hand left it");
    assert_ne!(kept, r);
    f.cat.place_copy(1, MUMIA_DIR, r).unwrap();
    f.pass();
    assert_eq!(f.record_of(1, &rels[0]), Some(kept), "provenance never over the hand");
}

/// WM.23's identity half (with WM.25 (e)): "Deep Water" `al-157` in MUSIC, its list kept, track 3 missing and track 2 worse. A fetched
/// file placed for it (its placement said before it appears, the file untouched): (a) tagged for a promo by another spelling, (b) no
/// tags at all ("a1b2c3.flac"), (d) as (a) with the copy in folder 1: the record's track 3, by its placement, its copy whole; (c) a
/// better copy for track 2: track 2's id. Red first: (b) nothing pairs an untitled file, and no placement exists.
#[test]
fn a_fetched_track_joins_its_record_untouched() {
    let deep = [("Breakwater", 200_000), ("Sea Wall", 230_000), ("Night Pier", 260_000), ("Harbour Bell", 290_000)];
    for (case, folder, placed, tags) in [
        ("a", MUSIC, "03 Night Pier.flac", vec![("album", "Deep Water (Promo)"), ("artist", "Glas Harbour")]),
        ("b", MUSIC, "a1b2c3.flac", Vec::new()),
        ("d", 1, "03 Night Pier.flac", vec![("album", "Deep Water (Promo)"), ("artist", "Glas Harbour")]),
    ] {
        let f = Folders::new(&format!("wm23{case}"));
        let dir = "Glass Harbour/Deep Water";
        let rels: Vec<(String, usize)> = [1usize, 2, 4].iter().map(|n| (format!("{dir}/0{n} {}.flac", deep[n - 1].0), *n)).collect();
        for (rel, n) in &rels {
            f.add(folder, &[(rel.as_str(), vec![("album", "Deep Water"), ("albumartist", "Glass Harbour"), ("title", deep[n - 1].0), ("tracknumber", n.to_string().as_str())])]);
        }
        f.lengths(folder, &rels.iter().map(|(rel, n)| (rel.as_str(), deep[n - 1].1)).collect::<Vec<_>>());
        f.pass();
        let r = f.record_of(folder, &rels[0].0).unwrap();
        keep(&f.cat, r, "R157", &list(&deep));
        let rel = format!("{dir}/{placed}");
        f.cat.place_file(folder, &rel, r, None, (1, 3)).unwrap();
        f.add(folder, &[(rel.as_str(), tags.clone())]);
        f.lengths(folder, &[(rel.as_str(), 260_400)]);
        f.pass();
        assert_eq!(f.record_of(folder, &rel), Some(r), "({case}) its record by its fetch");
        let c = f.cat.copies_of(&[r], &library).unwrap().remove(&r).unwrap();
        assert_eq!(c.copies.iter().map(|c| (c.folder, c.paired)).collect::<Vec<_>>(), [(folder, 4)], "({case}) track 3, its copy whole");
        assert_eq!(f.cat.record(r).unwrap().unwrap().artist, "Glass Harbour", "({case}) the credit as it was, never the file's spelling");
    }
    // (c) a better copy for track 2: its placement names track 2, so the file takes its id.
    let f = Folders::new("wm23c");
    let dir = "Glass Harbour/Deep Water";
    for (n, (title, _)) in deep.iter().enumerate() {
        let rel = format!("{dir}/0{} {title}.mp3", n + 1);
        f.add(MUSIC, &[(rel.as_str(), vec![("album", "Deep Water"), ("albumartist", "Glass Harbour"), ("title", title), ("tracknumber", (n + 1).to_string().as_str())])]);
    }
    f.pass();
    let old = format!("{dir}/02 Sea Wall.mp3");
    let (r, two) = (f.record_of(MUSIC, &old).unwrap(), f.cat.track_of(MUSIC, &old).unwrap());
    let better = format!("{dir}/02 Sea Wall.flac");
    f.cat.place_file(MUSIC, &better, r, Some(two), (1, 2)).unwrap();
    std::fs::remove_file(f.roots[0].join(&old)).unwrap();
    f.add(MUSIC, &[(better.as_str(), vec![("album", "Another Release"), ("title", "Sea Wall (Remastered)")])]);
    assert_eq!(f.cat.track_of(MUSIC, &better), Some(two), "(c) track 2's id: plays and hearts stay on it");
}

/// WM.22 (T-WM-5, T-WM-9): 0.18's step part `wm`, on 0.17's data. (b1) `al-3215`, made of `ss-140`'s folder by its tags and titled
/// by the site, its folder still expected for `al-3211` within its week: merged into `al-3211` by the placement the step seeds, its
/// files `al-3211`'s tracks; (b2) a record titled by a stamp with no evidence: titled as a file with no album titles it, listed;
/// a stamp beside a name cut. One IMMEDIATE transaction (a part cut off changed nothing), the raw tags byte for byte, run twice it does
/// nothing. Red first: no part.
#[test]
fn the_018_step_cleans_shown_values_once() {
    let f = Folders::new("wm22");
    let r = mumia_in_music(&f);
    let rels = electronicfresh(&f, false);
    f.pass();
    let made = f.record_of(1, &rels[0]).expect("0.17 made a record of it");
    assert_ne!(made, r);
    // As 0.17 left them: its title the site's, its folder expected (no placement then), and two more records named by stamps.
    let db = || f.cat.db();
    db().execute("UPDATE records SET title = 'www.electronicfresh.com' WHERE id = ?1", [made]).unwrap();
    db().execute("DELETE FROM placements", []).unwrap();
    db().execute("INSERT INTO expected (folder, dir, record, at) VALUES (1, ?1, ?2, strftime('%s','now'))", params![MUMIA_DIR, r]).unwrap();
    let lone = record(&f.cat, "flacsite.net", "Kite Theory");
    let lights = record(&f.cat, "Harbour Lights [www.flacsite.net]", "Glass Harbour");
    let raw = || -> Vec<(i64, String, String, String)> {
        db().prepare("SELECT folder, rel, name, value FROM tags ORDER BY folder, rel, n").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap().map(Result::unwrap).collect()
    };
    let tags = raw();
    let title = |id: i64| f.cat.record(id).unwrap().map(|r| r.title);

    // Cut off before its mark: nothing written.
    assert!(f.cat.step_018_wm(&[], &|| anyhow::bail!("cut off")).is_err());
    assert_eq!((title(made).as_deref(), title(lone).as_deref()), (Some("www.electronicfresh.com"), Some("flacsite.net")), "whole or nothing");

    let (report, merges) = f.cat.step_018_wm(&[], &|| Ok(())).unwrap().expect("run once");
    assert_eq!(merges, [(made, r)], "(b1) merged by its seeded placement");
    for (rel, n) in rels.iter().zip(1..) {
        assert_eq!(f.cat.track_of(1, rel), f.cat.track_of(MUSIC, &format!("Polo & Pan/Mumia/0{n} T{n}.mp3")), "{rel}: al-3211's track");
    }
    assert_eq!(title(r).as_deref(), Some("Mumia"));
    assert_eq!(title(lone).as_deref(), Some(super::records::UNKNOWN_ALBUM), "(b2) as a file with no album titles it");
    assert!(report.listed["made by a stamp, no placement known"].iter().any(|l| l.contains(&ids::record(lone))), "(b2) listed, nothing merged");
    assert_eq!(title(lights).as_deref(), Some("Harbour Lights"), "a stamp beside a name cut");
    assert_eq!(raw(), tags, "the raw tags byte for byte");
    assert!(f.cat.step_018_wm(&[], &|| Ok(())).unwrap().is_none(), "the second run does nothing");
}

const SALAMANDA_1: &str = "c18f3ccc-0000-4000-8000-000000000221";
const SALAMANDA_2: &str = "ced3fdc3-0000-4000-8000-000000000222";
const CARIBOU: &str = "00000000-0000-4000-8000-000000000321";

/// NAMES.25's fixture in MUSIC: (a) both Salamandas, each album's files naming its own id; (b) a Various Artists sampler, one track
/// "Salamanda" with `ced3fdc3…`, its twin with none; (c) Caribou's id spelled "Caribou" and "Manitoba"; (d) a "Various Artists"
/// album whose album-artist id is Caribou's; (e) "Glass Mirage", credited to the second "Mirage" by its Discogs reference, its files
/// "Mirage" with no ids. The artists, in C55's order: ar-221 (the first Salamanda) and the first Mirage before their namesakes.
fn namesakes(f: &Folders) -> (i64, i64, i64, i64) {
    let first_salamanda = f.cat.artist_for("Salamanda").unwrap();
    f.cat.set_reference(&ids::artist(first_salamanda), &super::records::Ref::new("musicbrainz", "artist", SALAMANDA_1), "files").unwrap();
    let (first_mirage, second_mirage) = {
        let db = f.cat.db();
        (super::artists::give_artist(&db, "mirage", "Mirage", 1).unwrap(), super::artists::give_artist(&db, "mirage", "Mirage", 2).unwrap())
    };
    let mirage = record(&f.cat, "Glass Mirage", "Mirage");
    f.cat.db().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'listing')", [mirage, second_mirage]).unwrap();
    f.cat.expect_record(MUSIC, "Mirage/Glass Mirage", mirage).unwrap();
    let file = |artist: &'static str, album_artist: &'static str, album: &'static str, artist_id: Option<&'static str>, album_artist_id: Option<&'static str>, title: &'static str| {
        let mut tags = vec![("artist", artist), ("albumartist", album_artist), ("album", album), ("title", title)];
        tags.extend(artist_id.map(|id| ("musicbrainz_artistid", id)));
        tags.extend(album_artist_id.map(|id| ("musicbrainz_albumartistid", id)));
        tags
    };
    f.add(MUSIC, &[
        ("Salamanda/Our Lair/01 Lair.flac", file("Salamanda", "Salamanda", "Our Lair", Some(SALAMANDA_1), Some(SALAMANDA_1), "Lair")),
        ("Salamanda/Allez/01 Allez.flac", file("Salamanda", "Salamanda", "Allez", Some(SALAMANDA_2), Some(SALAMANDA_2), "Allez")),
        ("Salamanda/Allez/02 Sphere.flac", file("Salamanda", "Salamanda", "Allez", Some(SALAMANDA_2), Some(SALAMANDA_2), "Sphere")),
        ("Various/Sampler/01 Seeds.flac", file("Salamanda", "Various Artists", "Sampler", Some(SALAMANDA_2), None, "Seeds")),
        ("Various/Sampler/02 Twin.flac", file("Salamanda", "Various Artists", "Sampler", None, None, "Twin")),
        ("Caribou/Andorra/01 Melody Day.flac", file("Caribou", "Caribou", "Andorra", Some(CARIBOU), Some(CARIBOU), "Melody Day")),
        ("Caribou/Andorra/02 Sandy.flac", file("Caribou", "Caribou", "Andorra", Some(CARIBOU), Some(CARIBOU), "Sandy")),
        ("Manitoba/Up in Flames/01 Skunks.flac", file("Manitoba", "Manitoba", "Up in Flames", None, Some(CARIBOU), "Skunks")),
        ("Various/Caribou Mix/01 Mix.flac", file("Caribou", "Various Artists", "Caribou Mix", None, Some(CARIBOU), "Mix")),
        ("Mirage/Glass Mirage/01 Glass.flac", file("Mirage", "Mirage", "Glass Mirage", None, None, "Glass")),
    ]);
    f.pass();
    (first_salamanda, first_mirage, second_mirage, mirage)
}

/// The served library's albums, (name, artist id, artist line), by name; and its artists (id, name), by id.
fn served(lib: &crate::serve::library::Library) -> (std::collections::BTreeMap<String, (String, String)>, Vec<(String, String)>) {
    let db = lib.db();
    let albums = db.prepare("SELECT name, artist_id, artist FROM albums").unwrap().query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?)))).unwrap().map(Result::unwrap).collect();
    let artists = db.prepare("SELECT id, name FROM artists ORDER BY id").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
    (albums, artists)
}

fn song_artist(lib: &crate::serve::library::Library, title: &str) -> Vec<String> {
    lib.db().prepare("SELECT sa.artist_id FROM songs s JOIN song_artists sa ON sa.song_id = s.id WHERE s.title = ?1 ORDER BY sa.position").unwrap().query_map([title], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

/// NAMES.25 (MATCH.4's track rule, T-DC3-17): artists are served by id, namesakes apart, spellings of one id together. (a) two
/// served Salamandas, each with its own albums and songs; (b) the sampler's track by its own id, its twin C55's first; (c) "Up in
/// Flames" under Caribou's id, no served row of the key "manitoba"; (d) a "Various Artists" album credited by its album-artist id;
/// (e) "Glass Mirage" under the second Mirage by its Discogs credit, never by the name; the same served ids from a whole build and a
/// restart's. DC1a changes ids, not names. Red first: every namesake served under its key's lowest id; (e) re-credited by the name.
#[test]
fn namesakes_are_served_apart() {
    let f = Folders::new("names25");
    let (salamanda, _, second_mirage, mirage) = namesakes(&f);
    let lib = crate::serve::library::Library::open(None).unwrap();
    crate::serve::library::testing::build_now(&lib, &f.cat);
    let (albums, artists) = served(&lib);
    let id = |a: &str| albums.get(a).unwrap_or_else(|| panic!("{a} served: {albums:?}")).0.clone();
    let salamandas: Vec<&(String, String)> = artists.iter().filter(|(_, n)| n == "Salamanda").collect();
    assert_eq!(salamandas.len(), 2, "(a) two served Salamandas: {artists:?}");
    assert_eq!(id("Our Lair"), ids::artist(salamanda), "(a) the first by its own id");
    assert_ne!(id("Allez"), id("Our Lair"), "(a) namesakes apart");
    assert_eq!(song_artist(&lib, "Sphere"), [id("Allez")]);
    assert_eq!(song_artist(&lib, "Seeds"), [id("Allez")], "(b) by its own id");
    assert_eq!(song_artist(&lib, "Twin"), [ids::artist(salamanda)], "(b) no id: C55's first");
    assert_eq!(id("Up in Flames"), id("Andorra"), "(c) one id spelled two ways");
    assert_eq!(song_artist(&lib, "Skunks"), [id("Andorra")]);
    assert!(!artists.iter().any(|(_, n)| n == "Manitoba"), "(c) no served row of the key manitoba: {artists:?}");
    assert_eq!(id("Caribou Mix"), id("Andorra"), "(d) as the catalogue credits it");
    assert_eq!(f.cat.record_artists(mirage).unwrap(), [second_mirage], "(e) the credit by id stays (T-COU-2)");
    assert_eq!(id("Glass Mirage"), ids::artist(second_mirage), "(e) by its Discogs credit, never the name");
    assert_eq!(albums["Allez"].1, "Salamanda", "DC1a changes ids, not names");
    // A restart's build (a library made afresh) serves the same ids.
    let again = crate::serve::library::Library::open(None).unwrap();
    crate::serve::library::testing::build_now(&again, &f.cat);
    assert_eq!(served(&again), (albums, artists), "every build path, one rule");
    // A synced device learns of each record served under other ids, once (the change log's record kind, no reset): a build that
    // serves Allez's artist anew owes Allez, though no catalogue row of its credit changed.
    let allez_file = "Salamanda/Allez/01 Allez.flac";
    let allez = f.record_of(MUSIC, allez_file).unwrap();
    lib.db().execute("UPDATE album_artists SET artist_id = ?1 WHERE album_id = ?2", params![ids::artist(salamanda), ids::record(allez)]).unwrap();
    f.cat.tag_for_tests(MUSIC, allez_file, &[("artist", "Salamanda"), ("albumartist", "Salamanda"), ("album", "Allez"), ("title", "Allez"), ("musicbrainz_artistid", SALAMANDA_2), ("musicbrainz_albumartistid", SALAMANDA_2), ("date", "2015")]);
    f.cat.db().execute("DELETE FROM record_owed", []).unwrap();
    crate::serve::library::testing::build_now(&lib, &f.cat);
    let owed: Vec<i64> = f.cat.db().prepare("SELECT record FROM record_owed").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    assert!(owed.contains(&allez), "served under another id: owed ({owed:?})");
}

/// NAMES.11's first half: every served reader names an artist by its id, one name each; search finds each Salamanda by its own row.
/// And NAMES.24's DC1a half: the served `albums.artist` is `credit_text` over the album's artists' served names. Red first: one
/// served artist for both Salamandas; the album line the files' ALBUMARTIST text.
#[test]
fn every_served_reader_names_an_artist_by_its_id() {
    let f = Folders::new("names11");
    namesakes(&f);
    f.add(MUSIC, &[
        ("Kite Theory/Twin Rooms/01 One.flac", vec![("albumartists", "Kite Theory"), ("albumartists", "Glass Harbour"), ("albumartist", "Kite Theory; Glass Harbour"), ("artist", "Kite Theory"), ("album", "Twin Rooms"), ("title", "One")]),
        ("Kite Theory/Three Rooms/01 Two.flac", vec![("albumartists", "Kite Theory"), ("albumartists", "Glass Harbour"), ("albumartists", "Low Tide Choir"), ("albumartist", "Kite Theory / Glass Harbour / Low Tide Choir"), ("artist", "Kite Theory"), ("album", "Three Rooms"), ("title", "Two")]),
    ]);
    f.pass();
    let lib = crate::serve::library::Library::open(None).unwrap();
    crate::serve::library::testing::build_now(&lib, &f.cat);
    let (albums, artists) = served(&lib);
    for (aid, name) in &artists {
        assert_eq!(lib.artist_name(aid).unwrap().as_deref(), Some(name.as_str()), "one name by its id");
    }
    let db = lib.db();
    let found: Vec<String> = db.prepare("SELECT a.id FROM artists_fts f JOIN artists a ON a.n = f.rowid WHERE artists_fts MATCH '\"salamanda\"' ORDER BY a.id").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(found.len(), 2, "search finds both, each by its own row");
    drop(db);
    assert_eq!(albums["Twin Rooms"].1, "Kite Theory & Glass Harbour", "NAMES.24: the one formatter");
    assert_eq!(albums["Three Rooms"].1, "Kite Theory, Glass Harbour & Low Tide Choir");
    assert_eq!(albums["Our Lair"].1, "Salamanda", "one artist: its name");
}

/// WM.14, a pin (green before WM's build, kept green by it): "Glass Rooms" `al-153`, had in MUSIC, its files naming release `R153`; a
/// folder dropped into IN by hand (no item, no placement) whose ten files name `R153`, their titles and lengths agreeing, every ALBUM
/// a site's stamp: one record, two copies (FA17), the IN copy its arrival, whatever its album tag.
#[test]
fn a_files_release_joins_a_had_record_whatever_its_album_tag() {
    let f = Folders::new("wm14");
    let release = "00000000-0000-4000-8000-000000000153";
    let files = |dir: &str, album: &'static str| -> Vec<(String, Vec<(&'static str, String)>)> {
        (1..=10).map(|n| (format!("{dir}/{n:02} Q{n}.flac"), vec![("album", album.to_string()), ("albumartist", "Glass Harbour".to_string()), ("title", format!("Q{n}")), ("tracknumber", n.to_string()), ("musicbrainz_albumid", release.to_string())])).collect()
    };
    let add = |folder: i64, list: &[(String, Vec<(&'static str, String)>)]| {
        for (rel, tags) in list {
            f.add(folder, &[(rel.as_str(), tags.iter().map(|(n, v)| (*n, v.as_str())).collect())]);
        }
        f.lengths(folder, &list.iter().zip(1..).map(|((rel, _), n)| (rel.as_str(), 170_000 + n * 8_000)).collect::<Vec<_>>());
    };
    let music = files("Glass Harbour/Glass Rooms", "Glass Rooms");
    add(MUSIC, &music);
    f.pass();
    let r = f.record_of(MUSIC, &music[0].0).unwrap();
    let dropped = files("Dropped/Glass Rooms", "www.flacsite.net");
    add(1, &dropped);
    f.pass();
    assert!(dropped.iter().all(|(rel, _)| f.record_of(1, rel) == Some(r)), "one record, two copies");
    assert_eq!(f.cat.record(r).unwrap().unwrap().title, "Glass Rooms");
}
