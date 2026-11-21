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
        ("base/src/app/settings.rs", "the naming inference counts files with a TITLE, a presence and no fact"),
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
