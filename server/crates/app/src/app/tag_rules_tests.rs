//! Each library's own tag rules (GEN-DESIGN §4, OQ-GEN-22; the owner, 2026-11-20, "Per library in 0.18"), on the settings
//! store, the folder rules every reader picks from, and the readers: GEN.7 (one split, given each value's rule; readers by
//! the row's folder), GEN.62 (the upgrade's copy) and GEN.63 (a library's own rules, the default, "&" refused).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app::library::folder_rules_of;
use crate::catalogue::normalise::{AMPERSAND, Alias};
use crate::catalogue::{Catalogue, Encoding, FolderRules, InUse, Rules};
use crate::serve::Serve;
use crate::serve::copies::Kind;
use crate::serve::library::{FolderRows, Library as Served};
use crate::settings::{self, LibraryKind, LibrarySetting, SourceConfig};

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn path(p: &Path) -> String {
    p.display().to_string()
}

/// The install's alias of GEN.7 and GEN.62: Hip-Hop is Hip Hop.
fn hip_hop() -> Vec<Alias> {
    vec![Alias { from: s(&["Hip-Hop"]), to: "Hip Hop".into() }]
}

/// GEN.62's install, as 0.17 kept its one set.
fn install_017() -> Encoding {
    Encoding { genre_separators: s(&["/", ";", ","]), genre_keep: s(&["Drum & Bass/Jungle"]), artist_separators: s(&["/", ";"]), artist_keep: s(&["AC/DC", "30/70"]) }
}

/// Three libraries (MUSIC, IN, ARCHIVE), an account's own music, and a library since removed whose folder the catalogue
/// still holds, each a folder with one file: (root, the folders in that order).
fn folders(name: &str) -> (PathBuf, [PathBuf; 5]) {
    let root = crate::retag::fixtures::dir(name);
    let all = [root.join("music"), root.join("in"), root.join("archive"), root.join("own").join("ann"), root.join("gone")];
    for d in &all {
        std::fs::create_dir_all(d.join("A")).unwrap();
        std::fs::write(d.join("A/01.flac"), "a").unwrap();
    }
    (root, all)
}

/// The settings' libraries with their folders, as the folder rules take them (each library one folder of its own here).
fn listed<'a>(sources: &'a [SourceConfig], paths: &'a [Vec<String>]) -> Vec<(&'a Encoding, &'a [String])> {
    sources[0].libraries.0.iter().zip(paths).map(|(l, p)| (l.tag_rules.as_ref().expect("every library its own"), p.as_slice())).collect()
}

/// A 0.17 settings file (version 4): MUSIC, IN and ARCHIVE in Cratarr's own source, own music, and GEN.62's one set of tag
/// rules with its alias.
fn settings_017(file: &Path, libraries: &[PathBuf], own: &Path) {
    let body = serde_json::json!({
        "version": 4,
        "sources": [{
            "id": "cratarr", "kind": "cratarr", "enabled": true,
            "libraries": [{ "id": path(&libraries[0]), "kind": "main" }, { "id": path(&libraries[1]), "kind": "incoming" }, { "id": path(&libraries[2]), "kind": "main" }],
            "values": { "folders": [path(&libraries[0]), path(&libraries[1]), path(&libraries[2])], "own_music": path(own) },
        }],
        "tag_rules": {
            "genre_separators": ["/", ";", ","], "genre_keep": ["Drum & Bass/Jungle"], "artist_separators": ["/", ";"], "artist_keep": ["AC/DC", "30/70"],
            "genre_aliases": [{ "from": ["Hip-Hop"], "to": "Hip Hop" }],
        },
    });
    std::fs::write(file, serde_json::to_vec_pretty(&body).unwrap()).unwrap();
}

/// The served library built from each folder's rows by `rules`: (its genres, its artists).
fn served_by(cat: &Catalogue, rules: &FolderRules) -> (Vec<(String, u32, u32)>, Vec<String>) {
    let lib = Served::open(None).unwrap();
    let rows: Vec<FolderRows> = cat.folders().iter().map(|(folder, root)| FolderRows { folder, root: path(root), rows: cat.served_rows(folder).unwrap() }).collect();
    lib.refresh(&rows, rules).unwrap();
    let mut artists: Vec<String> = lib.artist_names().unwrap().into_iter().map(|(n, _)| n).collect();
    artists.sort();
    (lib.genres().unwrap(), artists)
}

/// Each track's genres in the crates' facts (what the words store reads a file's words from), by `rules`.
fn facts_by(cat: &Catalogue, rules: &FolderRules) -> BTreeMap<i64, Vec<String>> {
    cat.set_folder_rules(rules.clone());
    cat.crate_facts().unwrap().iter().map(|f| (f.track.unwrap(), f.genres.clone())).collect()
}

/// Insights' figures of every folder's tracks, by `rules`.
fn insights_by(cat: &Catalogue, rules: &FolderRules) -> crate::insights::LibraryStats {
    let rows: Vec<crate::catalogue::TrackRow> = cat.folders().ids().flat_map(|f| cat.track_rows(f).unwrap()).map(|mut r| {
        r.rel = format!("{}:{}", r.folder, r.rel);
        r
    }).collect();
    crate::insights::library_stats(&rows, rules, &HashSet::new())
}

/// GEN.62: the upgrade copies the install's one set of tag rules into every library and keeps it as the default; the aliases
/// stay the install's; nothing splits differently by it, in any reader; the second run changes nothing.
#[test]
fn the_step_copies_the_installs_tag_rules_into_every_library() {
    let (root, all) = folders("gen62-step");
    let file = root.join("settings.json");
    settings_017(&file, &all[..3], &root.join("own"));
    let report = settings::tag_rules_migration(&file).unwrap().expect("a 0.17 file has a part to report");
    assert_eq!(report["tables"][0]["after"].as_u64(), Some(3), "{report}");

    let saved = settings::load(&file, &|_| None).unwrap().unwrap();
    for l in &saved.sources[0].libraries.0 {
        assert_eq!(l.tag_rules.as_ref(), Some(&install_017()), "{}: a copy of the install's encoding, never the default's nor empty", l.id);
    }
    assert_eq!(saved.tag_rules.encoding, install_017(), "the same as the default for new libraries and own music");
    assert_eq!((saved.tag_rules.genre_aliases.clone(), saved.tag_rules.artist_aliases.clone()), (hip_hop(), vec![]), "the aliases the install's, unchanged");

    // The second run: written as 0.18's, nothing left to copy, read the same; no library holds an alias.
    settings::save(&file, &saved).unwrap();
    assert_eq!(settings::tag_rules_migration(&file).unwrap(), None);
    assert_eq!(settings::load(&file, &|_| None).unwrap().unwrap(), saved);
    let written: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(written["version"].as_u64(), Some(u64::from(settings::VERSION)));
    for l in written["sources"][0]["libraries"].as_array().unwrap() {
        assert!(l["tag_rules"].is_object() && l["tag_rules"].get("genre_aliases").is_none() && l["tag_rules"].get("artist_aliases").is_none(), "{l}");
    }

    // In each library, the own music and the removed library's folder, a file with these values.
    let cat = Catalogue::open(None, all.to_vec()).unwrap();
    cat.walk_all();
    cat.pretend_read_for_tests();
    for folder in 0..5 {
        cat.writer_for_tests().execute("INSERT INTO tracks (id, born) VALUES (?1, 5)", [folder + 1]).unwrap();
        cat.give_track_for_tests(folder, "A/01.flac", folder + 1);
        cat.tag_for_tests(folder, "A/01.flac", &[("genre", "Drum & Bass/Jungle; Hip-Hop"), ("genre", "Funk / Soul, Jazz"), ("artist", "AC/DC; 30/70"), ("album", "Shore")]);
    }
    let paths: Vec<Vec<String>> = all[..3].iter().map(|p| vec![path(p)]).collect();
    let copies = folder_rules_of(saved.tag_rules.clone(), listed(&saved.sources, &paths), &cat.folders());
    let one = FolderRules::from(saved.tag_rules.clone());
    assert_ne!(copies, one, "the copies are each library's own");

    // The served library's genres and artists (the device index and OpenSubsonic read it): the same with the copies.
    let (genres, artists) = served_by(&cat, &copies);
    assert_eq!((genres.clone(), artists.clone()), served_by(&cat, &one));
    let names: Vec<&str> = genres.iter().map(|(g, _, _)| g.as_str()).collect();
    assert_eq!(names, ["Drum & Bass/Jungle", "Funk", "Hip Hop", "Jazz", "Soul"], "the alias after the split");
    assert!(["30/70", "AC/DC"].iter().all(|a| artists.iter().any(|x| x == a)), "{artists:?}");
    // The words store's files' words, and Insights.
    let facts = facts_by(&cat, &copies);
    assert_eq!(facts, facts_by(&cat, &one));
    assert!(facts.values().all(|g| g == &s(&["Drum & Bass/Jungle", "Hip-Hop", "Funk", "Soul", "Jazz"])), "{facts:?}");
    assert_eq!(insights_by(&cat, &copies), insights_by(&cat, &one));
    std::fs::remove_dir_all(&root).ok();
}

/// GEN.7: each reader finds a row's rules by its folder, through the one folder rules: a library's folder its encoding,
/// own music and a library since removed the default, a value of no file the default; the aliases the install's for all.
/// Serve holds them, and a change of one library's rules changes that library's reading alone.
#[test]
fn folder_rules_pick_each_rows_library_and_the_default_for_the_rest() {
    let (root, all) = folders("gen7-folder-rules");
    let cat = Arc::new(Catalogue::open(None, all.to_vec()).unwrap());
    cat.walk_all();
    let in_use: InUse = cat.folders();
    let id = |p: &Path| in_use.id_of(p).unwrap();
    let default = Encoding { genre_separators: s(&["/"]), ..Encoding::default() };
    let install = Rules { encoding: default.clone(), genre_aliases: hip_hop(), ..Rules::default() };
    let music = Encoding::default();
    let incoming = Encoding { genre_keep: s(&["Funk / Soul"]), ..Encoding::default() };
    let archive = Encoding { genre_separators: s(&[";"]), ..Encoding::default() };
    let paths: Vec<Vec<String>> = all[..3].iter().map(|p| vec![path(p)]).collect();
    let rules = folder_rules_of(install.clone(), [(&music, paths[0].as_slice()), (&incoming, paths[1].as_slice()), (&archive, paths[2].as_slice())], &in_use);
    assert_eq!(rules.encoding(id(&all[0])), &music);
    assert_eq!(rules.encoding(id(&all[1])), &incoming);
    assert_eq!(rules.encoding(id(&all[2])), &archive);
    assert_eq!(rules.encoding(id(&all[3])), &default, "own music: the default");
    assert_eq!(rules.encoding(id(&all[4])), &default, "a library since removed: the default");
    // A folder two libraries list reads by the first listed.
    let twice = folder_rules_of(install.clone(), [(&archive, paths[0].as_slice()), (&music, paths[0].as_slice())], &in_use);
    assert_eq!(twice.encoding(id(&all[0])), &archive);

    // The same value in each folder, as each reads it; the alias after the split, from the install's one list.
    let value = s(&["Drum/Bass; Hip-Hop, Funk / Soul"]);
    let read = |p: &Path| rules.values(id(p), "genre", &value);
    assert_eq!(read(&all[0]), ["Drum", "Bass", "Hip Hop", "Funk", "Soul"]);
    assert_eq!(read(&all[1]), ["Drum", "Bass", "Hip Hop", "Funk / Soul"]);
    assert_eq!(read(&all[2]), ["Drum/Bass", "Hip-Hop, Funk / Soul"]);
    assert_eq!(read(&all[3]), ["Drum", "Bass; Hip-Hop, Funk", "Soul"]);
    assert_eq!(read(&all[4]), read(&all[3]));
    // A value of no file (a request's artist): the default.
    assert_eq!(rules.install().values("albumartist", &s(&["A / B; C"])), ["A", "B", "C"]);

    // Serve holds them where it held the one set, and its check builds the served library by them (which the device index
    // and OpenSubsonic read): a change of ARCHIVE's rules changes ARCHIVE's reading alone, at the next check.
    cat.pretend_read_for_tests();
    for (n, p) in all.iter().enumerate() {
        let track = n as i64 + 1;
        cat.writer_for_tests().execute("INSERT INTO tracks (id, born) VALUES (?1, 5)", [track]).unwrap();
        cat.give_track_for_tests(id(p), "A/01.flac", track);
        let album = format!("Record {n}");
        cat.tag_for_tests(id(p), "A/01.flac", &[("title", "One"), ("artist", "The Band"), ("album", album.as_str()), ("genre", value[0].as_str())]);
    }
    let serve = Serve::open(None, install.clone(), &|_| None).unwrap();
    let kinds = [Kind::Main, Kind::Incoming, Kind::Main, Kind::Own, Kind::Main];
    serve.library.set_kinds(all.iter().zip(kinds).map(|(p, k)| (id(p), k)).collect());
    serve.attach_catalogue(cat.clone());
    serve.set_rules(rules.clone());
    assert_eq!(serve.rules(), rules);
    assert_eq!(cat.folder_rules(), rules, "the crates' facts read by them too");
    let in_folder = |genre: &str, p: &Path| serve.library.songs_by_genre(genre, 10, 0, Some(id(p))).unwrap().len();
    assert!(serve.refresh().unwrap());
    assert_eq!((in_folder("Drum", &all[0]), in_folder("Funk / Soul", &all[1]), in_folder("Drum/Bass", &all[2]), in_folder("Drum", &all[2])), (1, 1, 1, 0));
    assert_eq!(in_folder("Bass; Hip-Hop, Funk", &all[3]), 1, "own music by the default");
    let archive_slash = Encoding { genre_separators: s(&[";", "/"]), ..Encoding::default() };
    serve.set_rules(folder_rules_of(install, [(&music, paths[0].as_slice()), (&incoming, paths[1].as_slice()), (&archive_slash, paths[2].as_slice())], &in_use));
    assert!(serve.refresh().unwrap(), "other rules: built again");
    assert_eq!((in_folder("Drum", &all[0]), in_folder("Funk / Soul", &all[1]), in_folder("Drum/Bass", &all[2]), in_folder("Drum", &all[2])), (1, 1, 0, 1), "ARCHIVE's alone changed");
    assert_eq!(in_folder("Bass; Hip-Hop, Funk", &all[3]), 1);
    std::fs::remove_dir_all(&root).ok();
}

/// A FLAC in `dir` with one track's tags, its genre `genre`, in album `album`.
fn tagged(dir: &Path, album: &str, genre: &str) -> PathBuf {
    use lofty::tag::ItemKey;
    std::fs::create_dir_all(dir).unwrap();
    let path = crate::retag::fixtures::flac(dir, "01 One");
    let set = |t: &mut lofty::tag::Tag| {
        for (key, value) in [(ItemKey::AlbumTitle, album), (ItemKey::AlbumArtist, "The Band"), (ItemKey::TrackArtist, "The Band"), (ItemKey::TrackTitle, "One"), (ItemKey::TrackNumber, "1"), (ItemKey::Genre, genre)] {
            t.insert_text(key, value.to_string());
        }
    };
    crate::retag::rewrite(&path, set, |_| Ok(())).unwrap();
    path
}

/// GEN.7 (app): Health's rule fix splits each file by its library's rules: with the install's alias Hip-Hop → Hip Hop, a
/// MUSIC file's GENRE "Hip-Hop/Folk, World, & Country" is proposed as "Hip Hop; Folk; World; Country" (read, never applied:
/// a `fix:` plan is quiet in 0.18), and the same value in ARCHIVE's files (";" alone) is one word, so no fix is proposed.
#[test]
fn the_rule_fix_splits_by_its_librarys_rules() {
    let root = crate::retag::fixtures::dir("gen7-rule-fix");
    let (music, archive) = (root.join("music"), root.join("archive"));
    let value = "Hip-Hop/Folk, World, & Country";
    let (in_music, in_archive) = (tagged(&music.join("Shore"), "Shore", value), tagged(&archive.join("Dock"), "Dock", value));
    let cat = Arc::new(Catalogue::open(None, vec![music, archive]).unwrap());
    cat.walk_all();
    cat.reread(&[in_music.clone(), in_archive.clone()]).unwrap();
    cat.identify().unwrap();
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let install = Rules { genre_aliases: hip_hop(), ..Rules::default() };
    let serve = Serve::open(Some(&data), install.clone(), &|_| None).unwrap();
    serve.library.set_kinds(vec![(0, Kind::Main), (1, Kind::Main)]);
    serve.attach_catalogue(cat);
    let archive_rules = Encoding { genre_separators: s(&[";"]), ..Encoding::default() };
    serve.set_rules(FolderRules::new(install, BTreeMap::from([(0, Encoding::default()), (1, archive_rules)])));
    serve.refresh().unwrap();
    let (shore, dock) = (serve.library.album_in(0, "Shore").unwrap().unwrap(), serve.library.album_in(1, "Dock").unwrap().unwrap());

    let fix = crate::app::fixes::rule_fix(&serve, &shore, None).unwrap().expect("MUSIC's file: the alias names its Hip-Hop");
    let genre: Vec<(&Path, Option<&str>)> =
        fix.writes.iter().flat_map(|(p, set)| set.iter().filter(|(n, _)| *n == "genre").map(move |(_, v)| (p.as_path(), v.as_deref()))).collect();
    assert_eq!(genre, [(in_music.as_path(), Some("Hip Hop; Folk; World; Country"))]);
    assert!(fix.writes.iter().all(|(p, _)| p == &in_music), "never ARCHIVE's file");
    assert!(crate::app::fixes::rule_fix(&serve, &dock, None).unwrap().is_none(), "ARCHIVE's file: one word, which no alias names");
    let listed = crate::app::fixes::ruled_albums(&serve).unwrap();
    assert!(listed.contains(&shore) && !listed.contains(&dock), "{listed:?}");
    // Never "& Country" in any proposal.
    assert!(!format!("{:?}{:?}", fix.writes, fix.why).contains("& Country"));
    std::fs::remove_dir_all(&root).ok();
}

/// The server's sources outside its tests, each with its path from the crates' folder.
fn sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    let mut files = Vec::new();
    walk(&crates, &mut files);
    files
        .into_iter()
        .filter(|p| !p.components().any(|c| c.as_os_str() == "tests") && !p.to_string_lossy().ends_with("_tests.rs") && !p.to_string_lossy().contains("tests_"))
        .map(|p| {
            let text = std::fs::read_to_string(&p).unwrap().replace('\r', "");
            // A module's tests are tests.
            let code = text.split("\n#[cfg(test)]\nmod tests").next().unwrap().to_string();
            (p.strip_prefix(&crates).unwrap().to_string_lossy().replace('\\', "/"), code)
        })
        .collect()
}

/// GEN.7 (kind 6): one split function on every path that turns a words field into words, given the rule of where the value
/// was written, and every reader of a file's tags taking the folder rules, never the one set.
#[test]
fn words_fields_have_one_splitter_given_its_rule() {
    let code = sources();
    assert!(code.len() > 100, "{} files", code.len());
    for (file, text) in &code {
        // Never a second splitter of words fields: a split on ';', ',' and '/' of its own (0.17's crate facts), nor any split
        // of characters beside a words field's name. (MusicBrainz's ids and a file's labels split so; they are no words.)
        assert!(!text.contains("split([';', ',', '/'])"), "{file}: a words field split its own way");
        for (n, line) in text.lines().enumerate() {
            let field = ["\"genre\"", "\"style\"", "\"mood\"", "\"grouping\""].iter().any(|f| line.contains(f));
            assert!(!(field && line.contains(".split([")), "{file}:{}: {line}", n + 1);
            // Never normalise's split given the genre separators: genres go by the one split, the owner's exceptions first.
            assert!(!(line.contains("genre_separators") && line.contains("normalise::split")), "{file}:{}: {line}", n + 1);
            assert!(!(line.contains("genre_separators") && line.contains(" split(") && !line.contains("genres::split")), "{file}:{}: {line}", n + 1);
        }
    }
    // The one split takes the separators and the keep list as its inputs, and reads no constant of them.
    let genres = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/src/genres.rs")).unwrap();
    assert!(genres.contains("pub fn split(value: &str, separators: &[String], keep: &[String]) -> Vec<String>"));
    assert!(!genres.contains("SEPARATORS"));
    let of = |file: &str| code.iter().find(|(f, _)| f == file).unwrap_or_else(|| panic!("{file}")).1.clone();
    // A words field's words: the one split, by the library's encoding.
    assert!(of("store/src/catalogue/normalise.rs").contains("cratarr_rules::genres::split(value, &self.genre_separators, &self.genre_keep)"));
    // The services' stored words (GEN.4): the one split, by each service's shipped rule, as the store reads them; a typed word
    // (GEN.25): the one split with no separators, the owner's exceptions alone.
    let store = of("base/src/app/words/store.rs");
    assert!(store.contains("genres::split(&s.word, &genres::rules::separators(s.source.rule()), &[])"), "the services' words");
    assert!(store.contains("genres::split(w, &genres::rules::separators(genres::rules::TYPED), &[])"), "a typed word");
    for (source, rule) in [("Discogs", "DISCOGS"), ("Musicbrainz", "MUSICBRAINZ"), ("Lastfm", "LASTFM"), ("Bandcamp", "BANDCAMP"), ("Audiomuse", "AUDIOMUSE")] {
        assert!(store.contains(&format!("Source::{source} => genres::rules::{rule},")), "{source}'s rule");
    }
    // Each reader of a file's tags takes the folder rules and picks by the row's folder.
    for (file, takes) in [
        ("store/src/catalogue/mod.rs", "fn read_crate_facts(db: &Connection, folders: &InUse, rules: &FolderRules,"),
        ("store/src/catalogue/mod.rs", "let encoding = rules.encoding(place.0);"),
        ("store/src/catalogue/mod.rs", "pub fn artists(&self, rules: &FolderRules, field: &str)"),
        ("store/src/serve/library.rs", "fn song<'a>(row: &'a ServedRow, folder: i64, rules: &FolderRules) -> Song<'a>"),
        ("store/src/serve/library.rs", "rules.values(folder, \"genre\""),
        ("store/src/insights.rs", "pub fn library_stats(rows: &[TrackRow], rules: &FolderRules,"),
        ("store/src/insights.rs", "pub fn proximity(rows: &[TrackRow], rules: &FolderRules,"),
        ("requests/src/app/retention.rs", "rules.values(*folder, from,"),
        ("incoming/src/app/fixes.rs", "parts_of(rules.encoding(folder), rules.install(),"),
        ("incoming/src/app/fixes.rs", "let encoding = rules.encoding(track.place.0);"),
        ("records/src/app/insights.rs", "self.lib.folder_rules()"),
        ("served/src/serve/mod.rs", "rules: RwLock<FolderRules>,"),
        // A value of no file: the default.
        ("requests/src/app/requests.rs", "serve.rules().install().values(\"albumartist\""),
    ] {
        assert!(of(file).contains(takes), "{file}: {takes}");
    }
    // None takes the one set: a reader's `rules: &Rules`.
    for file in ["store/src/catalogue/mod.rs", "store/src/serve/library.rs", "store/src/insights.rs", "requests/src/app/retention.rs", "incoming/src/app/fixes.rs", "requests/src/app/step_017.rs", "base/src/app/first_start.rs"] {
        assert!(!of(file).contains("rules: &Rules") && !of(file).contains("rules: &crate::catalogue::Rules"), "{file} reads by the one set");
    }
}

/// The settings after the upgrade (GEN.62), and the folders' paths: (root, the folders, the settings as read).
fn upgraded(name: &str) -> (PathBuf, [PathBuf; 5], settings::Saved) {
    let (root, all) = folders(name);
    let file = root.join("settings.json");
    settings_017(&file, &all[..3], &root.join("own"));
    let saved = settings::load(&file, &|_| None).unwrap().unwrap();
    (root, all, saved)
}

/// A save of `drafts` as Settings makes it: checked (`merge`), each library its tag rules (`fill_tag_rules`).
fn saved_as(before: &[SourceConfig], drafts: Vec<SourceConfig>, default: &Encoding) -> Result<Vec<SourceConfig>, settings::ChangeError> {
    let mut merged = crate::app::settings::merge(before, drafts)?;
    settings::fill_tag_rules(&mut merged, before, default);
    Ok(merged)
}

/// GEN.63: a library's tag rules are its own: an edit of one changes no other, nor the default; the default is for new
/// libraries (as it stands when one is added) and own music (and a folder no library holds); the artists' encoding moves
/// with the genres'; the aliases stay the install's.
#[test]
fn a_librarys_tag_rules_are_its_own_and_the_default_is_for_new_libraries_and_own_music() {
    let (root, all, saved) = upgraded("gen63-own-rules");
    let before = saved.sources.clone();
    let library = |sources: &[SourceConfig], p: &Path| sources[0].libraries.get(&path(p)).unwrap().tag_rules.clone().unwrap();
    let cat = Catalogue::open(None, all.to_vec()).unwrap();
    let id = |p: &Path| cat.folders().id_of(p).unwrap();
    let rules_of = |install: &Rules, sources: &[SourceConfig]| {
        let paths: Vec<Vec<String>> = sources[0].libraries.0.iter().map(|l| vec![l.id.clone()]).collect();
        folder_rules_of(install.clone(), listed(sources, &paths), &cat.folders())
    };

    // (1) ARCHIVE's genre separators ";" alone, its artists' its own too.
    let mut drafts = before.clone();
    let archive = drafts[0].libraries.0.iter_mut().find(|l| l.id == path(&all[2])).unwrap();
    archive.tag_rules = Some(Encoding { genre_separators: s(&[";"]), artist_separators: s(&[";"]), artist_keep: s(&["Cos/Mes"]), ..install_017() });
    let after = saved_as(&before, drafts, &saved.tag_rules.encoding).unwrap();
    assert_eq!(library(&after, &all[2]).genre_separators, [";"]);
    assert_eq!((library(&after, &all[2]).artist_separators, library(&after, &all[2]).artist_keep), (s(&[";"]), s(&["Cos/Mes"])));
    assert_eq!((library(&after, &all[0]), library(&after, &all[1])), (install_017(), install_017()), "MUSIC's and IN's unchanged");
    let rules = rules_of(&saved.tag_rules, &after);
    let rows = [
        ("Funk / Soul; Electronic", &["Funk / Soul", "Electronic"][..]),
        ("Jazz/Rock/Folk, World, & Country", &["Jazz/Rock/Folk, World, & Country"][..]),
        ("Drum/Bass; Jazz", &["Drum/Bass", "Jazz"][..]),
        ("RnB/Swing; Jazz", &["R&B", "New Jack Swing", "Jazz"][..]),
    ];
    for (value, archive_reads) in rows {
        assert_eq!(rules.values(id(&all[2]), "genre", &s(&[value])), archive_reads, "ARCHIVE: GEN.3's column");
        assert_eq!(rules.values(id(&all[0]), "genre", &s(&[value])), rules.values(id(&all[1]), "genre", &s(&[value])));
        assert_eq!(rules.values(id(&all[0]), "genre", &s(&[value])), FolderRules::from(saved.tag_rules.clone()).values(id(&all[0]), "genre", &s(&[value])), "MUSIC reads as before");
    }
    assert_eq!(rules.encoding(id(&all[3])), &install_017(), "the default unchanged");

    // (2) The default "/" alone: own music and a library since removed read by it; no library's own changes.
    let default = Encoding { genre_separators: s(&["/"]), ..install_017() };
    assert_eq!(default.check(), Ok(()));
    let install = Rules { encoding: default.clone(), ..saved.tag_rules.clone() };
    let again = saved_as(&after, after.clone(), &default).unwrap();
    assert_eq!(again, after, "the default's edit changes no library's own");
    let rules = rules_of(&install, &again);
    for p in [&all[3], &all[4]] {
        assert_eq!(rules.values(id(p), "genre", &s(&["Electronic; Downtempo/Ambient"])), ["Electronic; Downtempo", "Ambient"], "{}", p.display());
    }
    assert_eq!(rules.values(id(&all[0]), "genre", &s(&["Electronic; Downtempo/Ambient"])), ["Electronic", "Downtempo", "Ambient"]);

    // (3) NEW starts from the default as it stood when it was added; a later change of the default leaves it as it was.
    let new = root.join("new");
    let mut drafts = again.clone();
    drafts[0].libraries.0.push(LibrarySetting::new(path(&new), LibraryKind::Main));
    let with_new = saved_as(&again, drafts, &default).unwrap();
    assert_eq!(library(&with_new, &new), default);
    let later = Encoding { genre_separators: s(&[";"]), ..default.clone() };
    let after_later = saved_as(&with_new, with_new.clone(), &later).unwrap();
    assert_eq!(library(&after_later, &new), default, "NEW as it was");
    // A library sent with no rules of its own (an older page) keeps its saved ones.
    let mut bare = after_later.clone();
    bare[0].libraries.0.iter_mut().for_each(|l| l.tag_rules = None);
    assert_eq!(saved_as(&after_later, bare, &later).unwrap(), after_later);

    // The aliases are the install's: a library sent with aliases keeps none of them.
    let sent: LibrarySetting = serde_json::from_value(serde_json::json!({ "id": "x", "kind": "main", "tag_rules": { "genre_separators": [";"], "genre_aliases": [{ "from": ["a"], "to": "b" }] } })).unwrap();
    let back = serde_json::to_value(&sent).unwrap();
    assert!(back["tag_rules"].get("genre_aliases").is_none(), "{back}");
    std::fs::remove_dir_all(&root).ok();
}

/// GEN.63 (4), the app's part: "&" is no genre separator. A library's or the default's genre separators holding it are
/// refused with the reason, by the settings' check and by a save, and nothing is saved.
#[tokio::test]
async fn ampersand_is_no_genre_separator() {
    let (root, all, saved) = upgraded("gen63-ampersand");
    for separators in [&["&"][..], &["/", "&"], &[" & ", ";"], &["&;"]] {
        assert_eq!(Encoding { genre_separators: s(separators), ..Encoding::default() }.check(), Err(AMPERSAND), "{separators:?}");
        let mut drafts = saved.sources.clone();
        drafts[0].libraries.0[2].tag_rules = Some(Encoding { genre_separators: s(separators), ..install_017() });
        let refused = saved_as(&saved.sources, drafts, &saved.tag_rules.encoding).unwrap_err();
        assert!(refused.to_string().contains(AMPERSAND), "{refused}");
    }
    assert_eq!(Encoding { artist_separators: s(&["&"]), ..Encoding::default() }.check(), Ok(()), "an artist's separators are the artists' own rules");

    // Through the settings' save, as the page's: refused, and the rules before stay.
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let dir = data.to_string_lossy().into_owned();
    let cfg = crate::config::Config::from_lookup(|k| (k == "CRATARR_DATA_DIR").then(|| dir.clone())).unwrap();
    std::fs::create_dir_all(all[0].join(crate::playlists::PLAYLIST_DIR)).unwrap();
    let app = crate::app::App::new(cfg, settings::Saved { music_folder: Some(path(&all[0])), ..Default::default() }, settings::Origin::Env, None).await.unwrap();
    let before = app.settings.tag_rules();
    let with = Rules { encoding: Encoding { genre_separators: s(&["/", "&"]), ..Encoding::default() }, ..Rules::default() };
    let refused = app.settings.save(Vec::new(), Some(with), None, None, None).await.unwrap_err();
    assert!(refused.to_string().contains(AMPERSAND), "{refused}");
    let mut draft = saved.sources[0].clone();
    draft.libraries.0[0].tag_rules = Some(Encoding { genre_separators: s(&["&"]), ..Encoding::default() });
    let refused = app.settings.save(vec![draft], None, None, None, None).await.unwrap_err();
    assert!(refused.to_string().contains(AMPERSAND), "{refused}");
    assert_eq!(app.settings.tag_rules(), before);
    assert!(!data.join("settings.json").exists(), "nothing saved");
    std::fs::remove_dir_all(&root).ok();
}
