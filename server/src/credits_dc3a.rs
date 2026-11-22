//! DC3a 3a-1, the record's one credit rule, through the whole router (TS item 5's in-process harness): SCENARIOS-DC3 F54.2, F54.3;
//! SCENARIOS-NAMES NAMES.19 (its chips, pages and index), NAMES.21, NAMES.22, NAMES.24's DC3 half; SCENARIOS-CRED CRED.25's record
//! half and CRED.27 (q2), (q3)'s served lines; SCENARIOS-COUPLING COU.10 (a), (b), (d) and COU.13 (b)'s credit half. The record pass
//! decides each record's credit once (`record_artists`, its phrases and its printed extras); the record's answer, the served library
//! (OpenSubsonic's rows), the artist pages and the device's index read it. Out and the crates' Artist filter keep reading the files'
//! text in this run (3a-3 moves them with the step).

use axum::body::Body;
use axum::http::{Method, StatusCode};
use serde_json::Value;

use crate::catalogue::credits::CreditRow;
use crate::catalogue::records::Ref;
use crate::whole_router::Whole;

async fn ask(w: &Whole, by: (&str, &str), method: Method, path: &str) -> (StatusCode, Value) {
    w.send(by, method, path, Body::empty()).await
}

/// One file to plant: its path in Music and its tags.
type File = (String, Vec<(String, String)>);

/// A record's files: `dir/NN title.flac` in Music, each with the record's tags and its own.
fn record_files(dir: &str, album: &str, common: &[(&str, &str)], tracks: &[Vec<(&str, &str)>]) -> Vec<File> {
    tracks
        .iter()
        .enumerate()
        .map(|(i, own)| {
            let t = i + 1;
            let mut tags: Vec<(String, String)> = vec![("album".into(), album.into()), ("tracknumber".into(), t.to_string()), ("discnumber".into(), "1".into())];
            if !own.iter().any(|(k, _)| *k == "title") {
                tags.push(("title".into(), format!("Track {t}")));
            }
            tags.extend(common.iter().map(|(k, v)| (k.to_string(), v.to_string())));
            tags.extend(own.iter().map(|(k, v)| (k.to_string(), v.to_string())));
            (format!("{dir}/{t:02} Track {t}.flac"), tags)
        })
        .collect()
}

/// The files written, walked, read and tagged as the catalogue keeps them; the record pass and the served build run.
fn plant(w: &Whole, files: &[File]) {
    let cat = w.app.library.catalogue();
    for (rel, _) in files {
        let path = w.dir().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, rel).unwrap();
    }
    cat.walk_all();
    cat.pretend_read_for_tests();
    for (rel, tags) in files {
        let tags: Vec<(&str, &str)> = tags.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        cat.tag_for_tests(0, rel, &tags);
    }
    cat.identify().unwrap();
    w.served.refresh().unwrap();
}

/// The pass and the build again (a source's answer kept since).
fn again(w: &Whole) {
    w.app.library.catalogue().identify().unwrap();
    w.served.refresh().unwrap();
}

/// The served album of `dir` in Music.
fn album(w: &Whole, dir: &str) -> String {
    w.served.library.album_in(0, dir).unwrap().unwrap_or_else(|| panic!("{dir} built"))
}

/// Its record's catalogue id.
fn record(album: &str) -> i64 {
    crate::serve::ids::record_number(album).expect("a record's album id")
}

/// The record's answer for `by`, from the Library.
async fn view(w: &Whole, by: (&str, &str), album: &str) -> Value {
    let (status, v) = ask(w, by, Method::GET, &format!("/api/records/{album}?from=library")).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    v
}

/// The record's artists as its answer gives them: (id, name).
fn artists_of(v: &Value) -> Vec<(String, String)> {
    v["properties"]["artists"]["value"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| (a["id"].as_str().unwrap_or_default().to_string(), a["name"].as_str().unwrap_or_default().to_string()))
        .collect()
}

/// The artist of `name` here, by id (`ar-<n>`).
fn artist(w: &Whole, name: &str) -> String {
    crate::serve::ids::artist(w.app.library.catalogue().artist_of(name).unwrap().unwrap_or_else(|| panic!("an artist {name}")))
}

/// An artist page's own records and those it appears on, by album id.
async fn page(w: &Whole, by: (&str, &str), id: &str) -> (Vec<String>, Vec<String>) {
    let (status, v) = ask(w, by, Method::GET, &format!("/api/artists/cratarr?id={id}")).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let ids = |k: &str| v[k].as_array().into_iter().flatten().filter_map(|a| a["id"].as_str().map(str::to_string)).collect::<Vec<_>>();
    (ids("albums"), ids("appears_on"))
}

/// The served album as OpenSubsonic reads it: its artist line, its artists (id, name) and whether it is a compilation.
fn served(w: &Whole, album: &str) -> (String, Vec<(String, String)>, bool) {
    let a = w.served.library.album(album).unwrap().unwrap_or_else(|| panic!("{album} served"));
    (a.artist.clone(), a.artists.iter().map(|r| (r.id.clone(), r.name.clone())).collect(), a.is_compilation)
}

/// The served songs of an album by track: each one's line and its artists by name.
fn songs(w: &Whole, album: &str) -> Vec<(Option<u32>, String, Vec<String>)> {
    let mut out: Vec<(Option<u32>, String, Vec<String>)> =
        w.served.library.songs_of_albums(&[album.to_string()]).unwrap().into_iter().map(|s| (s.track, s.artist.clone(), s.artists.iter().map(|a| a.name.clone()).collect())).collect();
    out.sort();
    out
}

/// The device's index's record: its artist line, its artists by id and whether it is a compilation.
fn indexed(w: &Whole, album: &str) -> (String, Vec<i64>, bool) {
    let libraries = w.app.library.index_libraries(&w.served);
    let (index, _) = crate::app::device_index::whole_of(&w.served, &libraries, None).unwrap();
    let r = index.record(record(album)).unwrap_or_else(|| panic!("{album} in the index"));
    (r.artist.clone(), r.artist_ids.clone(), r.compilation)
}

/// `n` files of one artist's own record (the artist credited elsewhere, by its files).
fn own_record(dir: &str, album: &str, artist: &str, n: usize) -> Vec<File> {
    record_files(dir, album, &[("albumartist", artist), ("artist", artist)], &vec![Vec::new(); n])
}

/// F54.2: Chicago Waves, ALBUMARTIST "Carlos Niño & Miguel Atwood-Ferguson", ARTIST "Carlos Niño" on 10 files and "Miguel
/// Atwood-Ferguson" on 1, no ids, is two artists' record: two chips, each opening its artist; the card's text "Carlos Niño & Miguel
/// Atwood-Ferguson"; it is among each one's own records; tracks 1-10 credit Carlos Niño, the eleventh Miguel Atwood-Ferguson; the
/// compound (`ar-303`) loses its credit; OpenSubsonic and the device's index agree. Never under "Appears on" for either; never the
/// compound still credited. Red first: one compound chip, opening the compound.
#[tokio::test]
async fn chicago_waves_is_two_artists_record() {
    let w = Whole::open("dc3a-f54-2").await;
    let me = w.session("me");
    let by = ("cookie", me.as_str());
    let cat = w.app.library.catalogue();
    let compound = cat.artist_for("Carlos Niño & Miguel Atwood-Ferguson").unwrap();
    let mut files = own_record("Carlos Niño/Flutes", "Flutes, Echoes, It's All Happening!", "Carlos Niño", 2);
    let mut tracks: Vec<Vec<(&str, &str)>> = vec![vec![("artist", "Carlos Niño")]; 10];
    tracks.push(vec![("artist", "Miguel Atwood-Ferguson")]);
    files.extend(record_files("Carlos Niño/Chicago Waves", "Chicago Waves (IA11 Edition)", &[("albumartist", "Carlos Niño & Miguel Atwood-Ferguson")], &tracks));
    plant(&w, &files);
    let waves = album(&w, "Carlos Niño/Chicago Waves");
    let (cn, maf) = (artist(&w, "Carlos Niño"), artist(&w, "Miguel Atwood-Ferguson"));
    let v = view(&w, by, &waves).await;
    assert_eq!(artists_of(&v), [(cn.clone(), "Carlos Niño".to_string()), (maf.clone(), "Miguel Atwood-Ferguson".to_string())], "two chips: {}", v["properties"]["artists"]);
    assert_eq!(v["properties"]["credit"], "Carlos Niño & Miguel Atwood-Ferguson", "the one formatter's text");
    assert!(!cat.record_artists(record(&waves)).unwrap().contains(&compound), "the compound loses its credit");
    for (who, id) in [("Carlos Niño", &cn), ("Miguel Atwood-Ferguson", &maf)] {
        let (own, on) = page(&w, by, id).await;
        assert!(own.contains(&waves), "{who}'s own records hold Chicago Waves: {own:?}");
        assert!(!on.contains(&waves), "{who}: never under Appears on");
    }
    let tracks = songs(&w, &waves);
    assert!(tracks[..10].iter().all(|(_, _, a)| a == &["Carlos Niño"]), "tracks 1-10: {tracks:?}");
    assert_eq!(tracks[10].2, ["Miguel Atwood-Ferguson"], "the eleventh");
    let (line, served_artists, _) = served(&w, &waves);
    assert_eq!(line, "Carlos Niño & Miguel Atwood-Ferguson");
    assert_eq!(served_artists.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(), [cn.clone(), maf.clone()], "OpenSubsonic agrees");
    let (text, ids, _) = indexed(&w, &waves);
    assert_eq!((text.as_str(), ids), ("Carlos Niño & Miguel Atwood-Ferguson", vec![crate::serve::ids::artist_number(&cn).unwrap(), crate::serve::ids::artist_number(&maf).unwrap()]), "the device agrees");
}

/// Going Home's files: ALBUMARTIST "Carlos Niño & Friends", ALBUMARTISTS and ARTISTS [Carlos Niño, Friends]; with `ids` (production's
/// shape, H7), MUSICBRAINZ_ALBUMARTISTID and MUSICBRAINZ_ARTISTID `mb:cn` and `mb:fr`.
fn going_home(ids: Option<(&'static str, &'static str)>) -> Vec<File> {
    let mut common: Vec<(&str, &str)> = vec![
        ("albumartist", "Carlos Niño & Friends"),
        ("albumartists", "Carlos Niño"),
        ("albumartists", "Friends"),
        ("artist", "Carlos Niño & Friends"),
        ("artists", "Carlos Niño"),
        ("artists", "Friends"),
    ];
    if let Some((cn, fr)) = ids {
        common.extend([("musicbrainz_albumartistid", cn), ("musicbrainz_albumartistid", fr), ("musicbrainz_artistid", cn), ("musicbrainz_artistid", fr)]);
    }
    record_files("Carlos Niño/Going Home", "Going Home", &common, &vec![Vec::new(); 3])
}

const MB_CN: &str = "c0000000-0000-4000-8000-0000000a3010";
const MB_FR: &str = "c0000000-0000-4000-8000-0000000a3040";

/// F54.3 (its record, tracks and artists' half; the Artist filter's values and Out's entries are 3a-3's, Members DC3b's): "Carlos
/// Niño & Friends" stays one credit, "Friends" never made an artist. The fixture's shape: one record artist, the artist of the whole
/// line (`ar-305`). Production's shape (H7): one record artist, the holder of `mb:cn` (`ar-301`), its text "Carlos Niño & Friends" on
/// every surface; `mb:fr` held by no one. In both, its tracks' credits the same as the record's; `ar-304` "Friends" loses its credits;
/// no "Friends" artist row, chip or search row. Never "Friends" made an artist from a credit's part; never `ar-305` given `mb:cn` or
/// `mb:fr`; never production's shape credited to `ar-305` by its line's name. Red first: "Friends" credited at position 1.
#[tokio::test]
async fn a_placeholder_is_never_an_artist() {
    for production in [false, true] {
        let w = Whole::open(if production { "dc3a-f54-3b" } else { "dc3a-f54-3a" }).await;
        let me = w.session("me");
        let by = ("cookie", me.as_str());
        let cat = w.app.library.catalogue();
        let friends = cat.artist_for("Friends").unwrap();
        let whole = cat.artist_for("Carlos Niño & Friends").unwrap();
        let mut files = own_record("Carlos Niño/Flutes", "Flutes, Echoes, It's All Happening!", "Carlos Niño", 2);
        if production {
            for f in files.iter_mut() {
                f.1.push(("musicbrainz_albumartistid".into(), MB_CN.into()));
            }
        }
        files.extend(going_home(production.then_some((MB_CN, MB_FR))));
        plant(&w, &files);
        let home = album(&w, "Carlos Niño/Going Home");
        let cn = artist(&w, "Carlos Niño");
        let want = if production { cn.clone() } else { crate::serve::ids::artist(whole) };
        let v = view(&w, by, &home).await;
        let shape = if production { "production's" } else { "the fixture's" };
        assert_eq!(artists_of(&v).into_iter().map(|(id, _)| id).collect::<Vec<_>>(), [want.clone()], "{shape} shape: one record artist");
        assert_eq!(v["properties"]["credit"], "Carlos Niño & Friends", "{shape}: its text the files' whole line");
        assert!(!cat.record_artists(record(&home)).unwrap().contains(&friends), "{shape}: Friends loses its credits");
        for (track, line, credited) in songs(&w, &home) {
            assert_eq!(line, "Carlos Niño & Friends", "{shape}: track {track:?}'s line");
            assert!(!credited.iter().any(|a| a == "Friends"), "{shape}: track {track:?} never credits Friends: {credited:?}");
        }
        let (text, served_artists, _) = served(&w, &home);
        assert_eq!((text.as_str(), served_artists.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>()), ("Carlos Niño & Friends", vec![want.clone()]), "{shape}: OpenSubsonic agrees");
        let listed = w.served.library.album_artists_index().unwrap();
        assert!(!listed.iter().any(|a| a.name == "Friends"), "{shape}: no Friends artist row");
        let (artists_found, _, _) = w.served.library.search("friends", (20, 0), (0, 0), (0, 0), None).unwrap();
        assert!(!artists_found.iter().any(|a| a.name == "Friends"), "{shape}: no Friends search row");
        if production {
            assert!(cat.subject_of(&Ref::new("musicbrainz", "artist", MB_FR)).unwrap().is_empty(), "mb:fr held by no one");
            let whole_refs = cat.subject_of(&Ref::new("musicbrainz", "artist", MB_CN)).unwrap();
            assert_eq!(whole_refs, [cn.clone()], "mb:cn is Carlos Niño's, never the whole line's");
        }
    }
}

/// Perspeeks: Various Artists, Secede on tracks 1, 2, 5, 7 and Kettel on 3, 4, 6, 8, no ids, no reference.
fn perspeeks() -> Vec<File> {
    let tracks: Vec<Vec<(&str, &str)>> = [1, 2, 3, 4, 5, 6, 7, 8].iter().map(|t| vec![("artist", if [1, 2, 5, 7].contains(t) { "Secede" } else { "Kettel" })]).collect();
    record_files("Various Artists/Perspeeks", "Perspeeks", &[("albumartist", "Various Artists")], &tracks)
}

/// NAMES.19 (its chips, pages and index; Out and the Artist filter are 3a-3's) with NAMES.18 (a)'s store half: Perspeeks' artists
/// are Secede and Kettel, its line "Secede & Kettel" (NAMES.24), each chip opening its artist; each artist's page lists it among their
/// own records, not under "Appears on"; the served artist rows count it for each, and the Various Artists artist no longer holds it;
/// it is no compilation; the device's index and the server agree. Never Perspeeks under "Appears on"; never a reader still naming its
/// artist "Various Artists". Red first: Perspeeks is credited to Various Artists, a compilation.
#[tokio::test]
async fn a_splits_artists_own_it_everywhere() {
    let w = Whole::open("dc3a-names-19").await;
    let me = w.session("me");
    let by = ("cookie", me.as_str());
    let mut files = own_record("Secede/Bolt", "Bolt", "Secede", 1);
    files.extend(own_record("Kettel/Myam James", "Myam James", "Kettel", 1));
    files.extend(perspeeks());
    plant(&w, &files);
    let p = album(&w, "Various Artists/Perspeeks");
    let (secede, kettel) = (artist(&w, "Secede"), artist(&w, "Kettel"));
    let v = view(&w, by, &p).await;
    assert_eq!(artists_of(&v), [(secede.clone(), "Secede".to_string()), (kettel.clone(), "Kettel".to_string())], "its chips");
    assert_eq!(v["properties"]["credit"], "Secede & Kettel");
    for id in [&secede, &kettel] {
        let (own, on) = page(&w, by, id).await;
        assert!(own.contains(&p) && !on.contains(&p), "{id}: Perspeeks among their own records: {own:?} / {on:?}");
    }
    if let Some(va) = w.app.library.catalogue().artist_of("Various Artists").unwrap() {
        let (own, _) = page(&w, by, &crate::serve::ids::artist(va)).await;
        assert!(!own.contains(&p), "the Various Artists list no longer holds it");
    }
    let (line, served_artists, compilation) = served(&w, &p);
    assert_eq!((line.as_str(), served_artists.len(), compilation), ("Secede & Kettel", 2, false), "OpenSubsonic: its two artists, no compilation");
    let (text, ids, compilation) = indexed(&w, &p);
    assert_eq!((text.as_str(), ids.len(), compilation), ("Secede & Kettel", 2, false), "the device agrees");
    assert!(!v["properties"]["secondary"].as_array().into_iter().flatten().any(|s| s["value"] == "compilation"), "no compilation: {}", v["properties"]["secondary"]);
}

/// Deep Harbour (production's case, no ids): ALBUMARTIST "Commodo" on every file; ARTIST "Commodo / Alfa Mist" on 1-2 and "Anz /
/// Commodo / Alfa Mist" on 3-4.
fn deep_harbour() -> Vec<File> {
    let tracks: Vec<Vec<(&str, &str)>> = vec![
        vec![("artist", "Commodo / Alfa Mist"), ("title", "Hydra")],
        vec![("artist", "Commodo / Alfa Mist"), ("title", "Misty Bay")],
        vec![("artist", "Anz / Commodo / Alfa Mist"), ("title", "Echo")],
        vec![("artist", "Anz / Commodo / Alfa Mist"), ("title", "Undertow")],
    ];
    record_files("Commodo/Deep Harbour", "Deep Harbour", &[("albumartist", "Commodo")], &tracks)
}

/// NAMES.21: Deep Harbour is two people's album: its artists Commodo and Alfa Mist, two chips; Anz a credit of tracks 3 and 4 alone;
/// Commodo's and Alfa Mist's pages list it among their own records, Anz's under "Appears on"; the line "Commodo & Alfa Mist" (CRED.27
/// (q5), a pin). Never Anz a record artist; never Deep Harbour under "Appears on" on Alfa Mist's page. Red first: the chip shows Commodo
/// alone, and Alfa Mist's page lists the record only through its tracks.
#[tokio::test]
async fn every_tracks_artists_are_the_records() {
    let w = Whole::open("dc3a-names-21").await;
    let me = w.session("me");
    let by = ("cookie", me.as_str());
    plant(&w, &deep_harbour());
    let dh = album(&w, "Commodo/Deep Harbour");
    let (commodo, alfa, anz) = (artist(&w, "Commodo"), artist(&w, "Alfa Mist"), artist(&w, "Anz"));
    let v = view(&w, by, &dh).await;
    assert_eq!(artists_of(&v), [(commodo.clone(), "Commodo".to_string()), (alfa.clone(), "Alfa Mist".to_string())], "two chips");
    assert_eq!(v["properties"]["credit"], "Commodo & Alfa Mist");
    let tracks = songs(&w, &dh);
    assert!(tracks[..2].iter().all(|(_, _, a)| !a.contains(&"Anz".to_string())) && tracks[2..].iter().all(|(_, _, a)| a.contains(&"Anz".to_string())), "Anz on 3 and 4 alone: {tracks:?}");
    for (who, id) in [("Commodo", &commodo), ("Alfa Mist", &alfa)] {
        let (own, on) = page(&w, by, id).await;
        assert!(own.contains(&dh) && !on.contains(&dh), "{who}: Deep Harbour among their own records");
    }
    let (own, on) = page(&w, by, &anz).await;
    assert!(!own.contains(&dh) && on.contains(&dh), "Anz: under Appears on");
}

/// A MusicBrainz release credit as the works pass keeps it: each (name, its MusicBrainz id, the phrase after it) of the release's
/// artist credit, on every track (`word` artist; featuring after a "feat." phrase).
fn release_credit(entries: &[(&str, Option<&str>, &str)]) -> Vec<CreditRow> {
    let mut featuring = false;
    entries
        .iter()
        .map(|(name, id, phrase)| {
            let word = if featuring { "featuring" } else { "artist" };
            if matches!(phrase.trim().to_lowercase().as_str(), "feat." | "ft." | "featuring") {
                featuring = true;
            }
            CreditRow { name: name.to_string(), person: id.map(str::to_string), word: word.into(), phrase: Some(phrase.to_string()).filter(|p| !p.trim().is_empty()), ..Default::default() }
        })
        .collect()
}

/// Keeps `rows` as MusicBrainz's answer for the record's release `release` (its reference, by its files or a hand), and runs the pass.
fn kept_release(w: &Whole, album: &str, release: &str, how: &str, rows: &[CreditRow]) {
    let cat = w.app.library.catalogue();
    cat.set_reference(&crate::serve::ids::record(record(album)), &Ref::new("musicbrainz", "release", release), how).unwrap();
    cat.keep_credits(record(album), "musicbrainz", &format!("release:{release}"), Some(rows), false, 1).unwrap();
    again(w);
}

/// NAMES.22 and NAMES.24's source rows through the record's answer: Deep Harbour's credit confirmed by a MusicBrainz release crediting
/// "Commodo & Alfa Mist" (`by` the files and MusicBrainz); a release crediting "Commodo" alone a version that differs ("Commodo ·
/// MusicBrainz"), none chosen, Alfa Mist kept; a confirming credit's own phrase printed ("Commodo x Alfa Mist") by the answer, the
/// served line and the device. Never Alfa Mist dropped; never one source's credit chosen over another's by its order. Red first: no
/// source's credit is kept as a version of the record's artists.
#[tokio::test]
async fn a_sources_credit_confirms_or_differs_and_drops_no_one() {
    const REL: &str = "c0000000-0000-4000-8000-000000002910";
    const COMMODO: &str = "c0000000-0000-4000-8000-0000000a2920";
    const ALFA: &str = "c0000000-0000-4000-8000-0000000a2930";
    let w = Whole::open("dc3a-names-22").await;
    let me = w.session("me");
    let by = ("cookie", me.as_str());
    plant(&w, &deep_harbour());
    let dh = album(&w, "Commodo/Deep Harbour");
    // (a) confirmed.
    kept_release(&w, &dh, REL, "hand", &release_credit(&[("Commodo", Some(COMMODO), " & "), ("Alfa Mist", Some(ALFA), "")]));
    let v = view(&w, by, &dh).await;
    let fact = &v["properties"]["artists"];
    assert_eq!(artists_of(&v).into_iter().map(|(_, n)| n).collect::<Vec<_>>(), ["Commodo", "Alfa Mist"], "(a) confirmed");
    let named: Vec<&str> = fact["by"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    assert!(named.iter().any(|b| b.starts_with("files")) && named.contains(&"musicbrainz"), "(a) by the files and MusicBrainz: {fact}");
    // (b) a version that differs, none chosen.
    kept_release(&w, &dh, REL, "hand", &release_credit(&[("Commodo", Some(COMMODO), "")]));
    let v = view(&w, by, &dh).await;
    assert_eq!(artists_of(&v).into_iter().map(|(_, n)| n).collect::<Vec<_>>(), ["Commodo", "Alfa Mist"], "(b) Alfa Mist kept");
    let versions = &v["properties"]["artists"]["versions"];
    assert!(
        versions.as_array().into_iter().flatten().any(|x| x["by"].as_array().is_some_and(|b| b.iter().any(|s| s == "musicbrainz")) && x["value"].as_array().is_some_and(|a| a.len() == 1 && a[0]["name"] == "Commodo")),
        "(b) MusicBrainz's \"Commodo\" a version: {versions}"
    );
    // NAMES.24: a confirming credit's own phrase.
    kept_release(&w, &dh, REL, "hand", &release_credit(&[("Commodo", Some(COMMODO), " x "), ("Alfa Mist", Some(ALFA), "")]));
    let v = view(&w, by, &dh).await;
    assert_eq!(v["properties"]["credit"], "Commodo x Alfa Mist", "the confirming source's phrase");
    assert_eq!(served(&w, &dh).0, "Commodo x Alfa Mist", "the served line");
    assert_eq!(indexed(&w, &dh).0, "Commodo x Alfa Mist", "the device");
}

/// NAMES.24's DC3 half: every surface names all of a record's artists from `credit_text`: Deep Harbour "Commodo & Alfa Mist", a
/// record of three "Commodo, Alfa Mist & Anz", Perspeeks "Secede & Kettel": the record's answer, the served album (OpenSubsonic) and
/// the device's index give the same text. Never a surface naming fewer of a record's artists than another. Red first: a card's line is
/// the files' album artist.
#[tokio::test]
async fn every_surface_names_all_of_a_records_artists() {
    let w = Whole::open("dc3a-names-24").await;
    let me = w.session("me");
    let by = ("cookie", me.as_str());
    let mut files = deep_harbour();
    files.extend(record_files("Commodo/Three Rooms", "Three Rooms", &[("albumartist", "Commodo"), ("artist", "Commodo / Alfa Mist / Anz")], &vec![Vec::new(); 2]));
    files.extend(perspeeks());
    plant(&w, &files);
    for (dir, text) in [("Commodo/Deep Harbour", "Commodo & Alfa Mist"), ("Commodo/Three Rooms", "Commodo, Alfa Mist & Anz"), ("Various Artists/Perspeeks", "Secede & Kettel")] {
        let a = album(&w, dir);
        assert_eq!(view(&w, by, &a).await["properties"]["credit"], text, "{dir}: the record's answer");
        assert_eq!(served(&w, &a).0, text, "{dir}: the served album");
        assert_eq!(indexed(&w, &a).0, text, "{dir}: the device's index");
    }
}

const RF: &str = "c0000000-0000-4000-8000-0000000a0495";
const TK: &str = "c0000000-0000-4000-8000-0000000a0497";

/// CRED.25's record half (with T-CRED-33) and CRED.27 (q2), (q3)'s served lines. Same Boat: its artist Rowan Fell alone, its card,
/// row and head "Rowan Fell", one chip, though Oda Brenn features on every track. Harbour: the release's own credit features Tamsin
/// Kell (the files' ALBUMARTIST and MusicBrainz's release credit): the record's artist Rowan Fell, Tamsin Kell featuring on the record,
/// the card, row and head printing "Rowan Fell feat. Tamsin Kell", the head with two entries; Harbour among Rowan Fell's own records and
/// never among anyone's own records but his. Night Signals' tracks 3 ("Rowan Fell / Oda Brenn") and 6 ("Rowan Fell; Oda Brenn") read
/// "Rowan Fell & Oda Brenn" (a separator never printed). Never a featured artist a record artist. Red first: no role at the record;
/// Harbour one artist "Rowan Fell feat. Tamsin Kell".
#[tokio::test]
async fn featuring_is_per_track_and_never_rises() {
    const REL_HARBOUR: &str = "c0000000-0000-4000-8000-000000004970";
    let w = Whole::open("dc3a-cred-25").await;
    let me = w.session("me");
    let by = ("cookie", me.as_str());
    let mut files = record_files(
        "Rowan Fell/Same Boat",
        "Same Boat",
        &[("albumartist", "Rowan Fell"), ("musicbrainz_albumartistid", RF), ("artist", "Rowan Fell feat. Oda Brenn")],
        &vec![Vec::new(); 3],
    );
    files.extend(record_files(
        "Rowan Fell/Harbour",
        "Harbour",
        &[("albumartist", "Rowan Fell feat. Tamsin Kell"), ("musicbrainz_albumartistid", RF), ("artist", "Rowan Fell"), ("musicbrainz_albumid", REL_HARBOUR)],
        &[vec![("title", "Harbour (feat. Tamsin Kell)")]],
    ));
    let lines = ["Rowan Fell feat. Oda Brenn", "Rowan Fell", "Rowan Fell / Oda Brenn", "Rowan Fell ft. Oda Brenn", "Rowan Fell & Oda Brenn", "Rowan Fell; Oda Brenn"];
    let tracks: Vec<Vec<(&str, &str)>> = lines.iter().map(|l| vec![("artist", *l)]).collect();
    files.extend(record_files("Rowan Fell/Night Signals", "Night Signals", &[("albumartist", "Rowan Fell"), ("musicbrainz_albumartistid", RF)], &tracks));
    plant(&w, &files);
    let (boat, harbour, signals) = (album(&w, "Rowan Fell/Same Boat"), album(&w, "Rowan Fell/Harbour"), album(&w, "Rowan Fell/Night Signals"));
    kept_release(&w, &harbour, REL_HARBOUR, "files", &release_credit(&[("Rowan Fell", Some(RF), " feat. "), ("Tamsin Kell", Some(TK), "")]));
    let rowan = artist(&w, "Rowan Fell");
    // Same Boat.
    let v = view(&w, by, &boat).await;
    assert_eq!(artists_of(&v), [(rowan.clone(), "Rowan Fell".to_string())], "Same Boat: one chip");
    assert_eq!((v["properties"]["credit"].as_str(), served(&w, &boat).0.as_str(), indexed(&w, &boat).0.as_str()), (Some("Rowan Fell"), "Rowan Fell", "Rowan Fell"), "Same Boat's card, row and head");
    // Harbour.
    let v = view(&w, by, &harbour).await;
    assert_eq!(artists_of(&v), [(rowan.clone(), "Rowan Fell".to_string())], "Harbour: Rowan Fell its one record artist");
    let head: Vec<(String, String)> = v["properties"]["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| (e["name"].as_str().unwrap_or_default().to_string(), e["role"].as_str().unwrap_or_default().to_string()))
        .collect();
    assert_eq!(head, [("Rowan Fell".to_string(), "main".to_string()), ("Tamsin Kell".to_string(), "featuring".to_string())], "the head's two entries: {}", v["properties"]);
    for text in [v["properties"]["credit"].as_str().unwrap_or_default().to_string(), served(&w, &harbour).0, indexed(&w, &harbour).0] {
        assert_eq!(text, "Rowan Fell feat. Tamsin Kell", "Harbour's card, row and head");
    }
    let (own, _) = page(&w, by, &rowan).await;
    assert!(own.contains(&harbour) && own.contains(&boat), "Rowan Fell's own records");
    if let Some(tamsin) = w.app.library.catalogue().artist_of("Tamsin Kell").unwrap() {
        let (own, _) = page(&w, by, &crate::serve::ids::artist(tamsin)).await;
        assert!(!own.contains(&harbour), "Tamsin Kell never under Own records");
    }
    if let Some(oda) = w.app.library.catalogue().artist_of("Oda Brenn").unwrap() {
        let (own, _) = page(&w, by, &crate::serve::ids::artist(oda)).await;
        assert!(!own.contains(&boat) && !own.contains(&signals), "Oda Brenn no record artist");
    }
    // CRED.27 (q2), (q3).
    let lines = songs(&w, &signals);
    assert_eq!((lines[2].1.as_str(), lines[5].1.as_str()), ("Rowan Fell & Oda Brenn", "Rowan Fell & Oda Brenn"), "tracks 3 and 6: {lines:?}");
}

/// COU.10 (a), (b) and (d), and COU.13 (b)'s credit half. (a) "Harbour Sampler", twelve artists, no ALBUMARTIST, no reference: Various
/// Artists, a compilation (NAMES.20 row 4); with R331 picked by hand, R331's credit, `by` musicbrainz (row 2). (b) "Kelpe Rooms", no
/// ALBUMARTIST, its tracks "Kelpe feat. Anz": Kelpe alone (row 3), the same as its "Various Artists" twin (the coordinator's
/// correction). (d) COMPILATION=1 with ALBUMARTIST Kelpe, every track Kelpe's, no reference: no compilation wherever read (the served
/// album, OpenSubsonic, the device's index, the record's answer). Never a compilation that holds only because a file says "Various
/// Artists" or COMPILATION=1; never two surfaces disagreeing. Red first: a missing ALBUMARTIST falls back to the first file's ARTIST;
/// the served compilation is COMPILATION's.
#[tokio::test]
async fn a_record_with_no_album_artist_is_credited_by_names_20() {
    const R331: &str = "c0000000-0000-4000-8000-000000003310";
    const HC: &str = "c0000000-0000-4000-8000-0000000a3311";
    let w = Whole::open("dc3a-cou-10").await;
    let me = w.session("me");
    let by = ("cookie", me.as_str());
    let names: Vec<String> = (1..=12).map(|n| format!("Harbour Player {n}")).collect();
    let sampler: Vec<Vec<(&str, &str)>> = names.iter().map(|n| vec![("artist", n.as_str())]).collect();
    let mut files = record_files("Samplers/Harbour Sampler", "Harbour Sampler", &[], &sampler);
    files.extend(record_files("Kelpe/Kelpe Rooms", "Kelpe Rooms", &[("artist", "Kelpe feat. Anz")], &vec![Vec::new(); 2]));
    files.extend(record_files("Various/Kelpe Rooms Twin", "Kelpe Rooms Twin", &[("albumartist", "Various Artists"), ("artist", "Kelpe feat. Anz")], &vec![Vec::new(); 2]));
    files.extend(record_files("Kelpe/Answered", "Answered", &[("albumartist", "Kelpe"), ("artist", "Kelpe"), ("compilation", "1")], &vec![Vec::new(); 3]));
    plant(&w, &files);
    // (a)
    let sampler = album(&w, "Samplers/Harbour Sampler");
    let v = view(&w, by, &sampler).await;
    assert_eq!(v["properties"]["credit"], "Various Artists", "(a) row 4");
    assert!(served(&w, &sampler).2, "(a) a compilation");
    // (b) and its twin.
    for dir in ["Kelpe/Kelpe Rooms", "Various/Kelpe Rooms Twin"] {
        let a = album(&w, dir);
        let v = view(&w, by, &a).await;
        assert_eq!(artists_of(&v).into_iter().map(|(_, n)| n).collect::<Vec<_>>(), ["Kelpe"], "(b) {dir}: Kelpe alone");
        assert_eq!(v["properties"]["credit"], "Kelpe", "(b) {dir}");
        assert!(!served(&w, &a).2, "(b) {dir}: no compilation");
    }
    // (d)
    let answered = album(&w, "Kelpe/Answered");
    assert!(!served(&w, &answered).2, "(d) the served album: no compilation by COMPILATION=1");
    assert!(!indexed(&w, &answered).2, "(d) the device's index: no compilation");
    let v = view(&w, by, &answered).await;
    assert!(!v["properties"]["secondary"].as_array().into_iter().flatten().any(|s| s["value"] == "compilation"), "(d) the record's answer: {}", v["properties"]["secondary"]);
    // COU.13 (b): R331 picked by hand; its credit is the record's, by musicbrainz.
    kept_release(&w, &sampler, R331, "hand", &release_credit(&[("Harbour Collective", Some(HC), "")]));
    let v = view(&w, by, &sampler).await;
    assert_eq!(v["properties"]["credit"], "Harbour Collective", "COU.13 (b): R331's credit");
    let fact = &v["properties"]["artists"];
    assert_eq!(fact["by"], serde_json::json!(["musicbrainz"]), "COU.13 (b): by musicbrainz: {fact}");
}
