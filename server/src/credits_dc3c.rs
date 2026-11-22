//! SCENARIOS-CRED (DC3c, credits) through the whole router (TS item 5's in-process harness): one credits store of facts with their
//! sources, merged by id, no source ranked; the record's answer carries them for every account; a GET asks no outside source, and
//! every outside read is a runner job begun by an open or a schedule. The stand-ins answer MusicBrainz and Discogs as the scenarios'
//! fixtures say and count what each is asked. Fixture ids: SCENARIOS-CRED's `mb:<x>` and `dc:<x>` as hex ids.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::app::reach::stand_ins::{Asked, StandIns};
use crate::catalogue::records::Ref;
use crate::whole_router::Whole;

/// ALL's MusicBrainz release (`mb:rel-all`) and release group, and its people's MusicBrainz ids.
const REL_ALL: &str = "c0000000-0000-4000-8000-000000000401";
const RG_ALL: &str = "c0000000-0000-4000-8000-0000000b0401";
const YT: &str = "c0000000-0000-4000-8000-0000000a0401";
const KL: &str = "c0000000-0000-4000-8000-0000000a0402";
const GA: &str = "c0000000-0000-4000-8000-0000000a0405";
const NF: &str = "c0000000-0000-4000-8000-0000000a0406";
const MD: &str = "c0000000-0000-4000-8000-0000000a0407";
/// ALL's Discogs master (MusicBrainz's link) and main release, and its people's Discogs ids.
const M401: u64 = 401;
const R401: u64 = 4010;
const DC_YT: u64 = 90401;
const DC_KL: u64 = 90402;
const DC_LK: u64 = 90408;

/// `path` asked by that way in (a cookie, or a key's authorization) with `body` (null: none): the answer's status and JSON.
async fn ask(w: &Whole, by: (&str, &str), method: Method, path: &str, body: Value) -> (StatusCode, Value) {
    let body = if body.is_null() { Body::empty() } else { Body::from(body.to_string()) };
    w.send(by, method, path, body).await
}

/// Waits (bounded) until `done` holds, or fails naming `what`.
async fn until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..1500 {
        if done() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("waited for {what}");
}

/// Whether the runner has no record's read and no Discogs job running or waiting (another test's jobs aside).
fn idle() -> bool {
    crate::runner::runner().rows().iter().all(|r| r.kind != crate::app::references::READ_KIND && !r.kind.contains("discogs") && !r.kind.contains("credits"))
}

/// The accounts of SCENARIOS-DC1's fixture: root (the harness's administrator `me`), alice and bob (users), carol (a user with no
/// rights), and root's reading key. Each as its way in.
struct Accounts {
    root: String,
    alice: String,
    carol: String,
    key: String,
}

fn accounts(w: &Whole) -> Accounts {
    for (name, admin) in [("alice", false), ("bob", false), ("carol", false), ("dave", true)] {
        w.served.users.from_directory(name, admin, None).unwrap();
    }
    Accounts { root: w.session("me"), alice: w.session("alice"), carol: w.session("carol"), key: w.key("me", "read") }
}

/// The process's one library set to a test's (its jobs read it) while the test runs, unset at its end, a failed one's too: a
/// test that reads the process as having none (`home_assistant`'s) never sees it.
struct Instance;

impl Instance {
    fn set(w: &Whole) -> Instance {
        crate::serve::set_instance(w.served.clone());
        Instance
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        crate::serve::clear_instance();
    }
}

/// The ways in, by name, for the four viewers CRED.3 and CRED.11 read with.
fn viewers(a: &Accounts) -> Vec<(&'static str, (&'static str, String))> {
    vec![("alice", ("cookie", a.alice.clone())), ("carol", ("cookie", a.carol.clone())), ("key", ("authorization", a.key.clone())), ("root", ("cookie", a.root.clone()))]
}

/// "ALL" as SCENARIOS-CRED's fixture: 12 FLAC files in the server's Music, every one with ALBUMARTIST Yann Tiersen (`mb:yt`), the
/// release `mb:rel-all`, COMPOSER Yann Tiersen, PERFORMER "Yann Tiersen (piano)", PRODUCER and MIXER Gwen Arzel; tracks 4 and 9
/// "Yann Tiersen feat. Kerys Lae" with ARTISTS and both ids, PERFORMER "Kerys Lae (vocals)" too; track 7 "Yann Tiersen & Ewen
/// Morvan" with ARTISTS and Yann Tiersen's id alone. Yann Tiersen and Kerys Lae hold their Discogs ids (MusicBrainz's link, as
/// their monthly read keeps it). Its album id and its files' paths.
fn plant_all(w: &Whole) -> (String, Vec<std::path::PathBuf>) {
    plant_all_with(w, &[])
}

/// `plant_all`, with other files beside it in the one walk and build: each (its path in Music, its tags).
fn plant_all_with(w: &Whole, others: &[(String, Vec<(String, String)>)]) -> (String, Vec<std::path::PathBuf>) {
    let cat = w.app.library.catalogue();
    let mut files = Vec::new();
    for t in 1..=12u32 {
        let rel = format!("Yann Tiersen/ALL/{t:02} Track {t}.flac");
        let path = w.dir().join(&rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &rel).unwrap();
        files.push((rel, path, t));
    }
    for (rel, _) in others {
        let path = w.dir().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, rel).unwrap();
    }
    cat.walk_all();
    cat.pretend_read_for_tests();
    for (rel, tags) in others {
        let tags: Vec<(&str, &str)> = tags.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        cat.tag_for_tests(0, rel, &tags);
    }
    for (rel, _, t) in &files {
        let (title, number) = (format!("Track {t}"), t.to_string());
        let mut tags: Vec<(&str, &str)> = vec![
            ("album", "ALL"),
            ("albumartist", "Yann Tiersen"),
            ("musicbrainz_albumartistid", YT),
            ("musicbrainz_albumid", REL_ALL),
            ("genre", "Modern Classical"),
            ("title", title.as_str()),
            ("tracknumber", number.as_str()),
            ("discnumber", "1"),
            ("composer", "Yann Tiersen"),
            ("performer", "Yann Tiersen (piano)"),
            ("producer", "Gwen Arzel"),
            ("mixer", "Gwen Arzel"),
        ];
        match t {
            4 | 9 => tags.extend([
                ("artist", "Yann Tiersen feat. Kerys Lae"),
                ("artists", "Yann Tiersen"),
                ("artists", "Kerys Lae"),
                ("musicbrainz_artistid", YT),
                ("musicbrainz_artistid", KL),
                ("performer", "Kerys Lae (vocals)"),
            ]),
            7 => tags.extend([("artist", "Yann Tiersen & Ewen Morvan"), ("artists", "Yann Tiersen"), ("artists", "Ewen Morvan"), ("musicbrainz_artistid", YT)]),
            _ => tags.extend([("artist", "Yann Tiersen"), ("artists", "Yann Tiersen"), ("musicbrainz_artistid", YT)]),
        }
        cat.tag_for_tests(0, rel, &tags);
    }
    cat.identify().unwrap();
    w.served.refresh().unwrap();
    let id = w.served.library.album_in(0, "Yann Tiersen/ALL").unwrap().expect("ALL built");
    // `ar-401` and `ar-402` hold their MusicBrainz ids (the files') and their Discogs ids (MusicBrainz's link, by the monthly read).
    for (mbid, dc, name) in [(YT, DC_YT, "Yann Tiersen"), (KL, DC_KL, "Kerys Lae")] {
        let artist = cat.artist_by_reference(&Ref::new("musicbrainz", "artist", mbid), name, "files", true).unwrap();
        let held = cat.artist_by_reference(&Ref::new("discogs", "artist", &dc.to_string()), name, "search", true).unwrap();
        assert_eq!(artist, held, "{name}: one artist holds both ids (the fixture)");
    }
    // `ar-403` Ewen Morvan, as the pass makes a track artist the files give no id for.
    cat.artist_for("Ewen Morvan").unwrap();
    w.served.refresh().unwrap();
    (id, files.into_iter().map(|(_, p, _)| p).collect())
}

/// One MusicBrainz artist-credit entry.
fn credit(id: &str, name: &str, join: &str) -> Value {
    json!({ "name": name, "joinphrase": join, "artist": { "id": id, "name": name, "sort-name": name } })
}

/// A relationship of an artist, with its attributes.
fn rel(kind: &str, id: &str, name: &str, attributes: &[&str]) -> Value {
    json!({ "type": kind, "target-type": "artist", "direction": "backward", "attributes": attributes, "artist": { "id": id, "name": name, "sort-name": name } })
}

/// `mb:rel-all` as the works pass's one question gets it (rec/works.rs's `INC`): the release credited to Yann Tiersen, tracks 4 and
/// 9 "Yann Tiersen feat. Kerys Lae"; the release's producer, mix (Gwen Arzel) and mastering (Nils Fréjus); each recording's piano
/// (Yann Tiersen), the vocal on 4 and 9 (Kerys Lae), the violin with "guest" on 2 and 3 (Maëlle Dour); each recording a performance
/// of a work Yann Tiersen composed; the release group linking Discogs' master 401.
fn release_all() -> Value {
    let tracks: Vec<Value> = (1..=12u32)
        .map(|t| {
            let artists = if matches!(t, 4 | 9) { json!([credit(YT, "Yann Tiersen", " feat. "), credit(KL, "Kerys Lae", "")]) } else { json!([credit(YT, "Yann Tiersen", "")]) };
            let mut relations = vec![rel("instrument", YT, "Yann Tiersen", &["piano"])];
            if matches!(t, 4 | 9) {
                relations.push(rel("vocal", KL, "Kerys Lae", &[]));
            }
            if matches!(t, 2 | 3) {
                relations.push(rel("instrument", MD, "Maëlle Dour", &["guest", "violin"]));
            }
            let work = format!("c0000000-0000-4000-8000-0000000c04{t:02}");
            relations.push(json!({ "type": "performance", "target-type": "work", "direction": "forward", "attributes": [],
                "work": { "id": work, "title": format!("Track {t}"), "relations": [rel("composer", YT, "Yann Tiersen", &[])] } }));
            json!({ "id": format!("c0000000-0000-4000-8000-0000000d04{t:02}"), "position": t, "number": t.to_string(), "title": format!("Track {t}"),
                "length": 200_000, "artist-credit": artists,
                "recording": { "id": format!("c0000000-0000-4000-8000-0000000e04{t:02}"), "title": format!("Track {t}"), "length": 200_000,
                    "artist-credit": artists, "relations": relations } })
        })
        .collect();
    json!({
        "id": REL_ALL, "title": "ALL", "date": "2019", "status": "Official",
        "artist-credit": [credit(YT, "Yann Tiersen", "")],
        "genres": [{ "name": "modern classical", "count": 3 }],
        "relations": [rel("producer", GA, "Gwen Arzel", &[]), rel("mix", GA, "Gwen Arzel", &[]), rel("mastering", NF, "Nils Fréjus", &[])],
        "release-group": { "id": RG_ALL, "title": "ALL", "primary-type": "Album", "secondary-types": [],
            "relations": [{ "type": "discogs", "url": { "resource": format!("https://www.discogs.com/master/{M401}") } }] },
        "media": [{ "position": 1, "format": "Digital Media", "track-count": 12, "tracks": tracks }]
    })
}

/// A Discogs artist entry of a tracklist or a credit.
fn dc_artist(id: u64, name: &str, role: &str, tracks: &str, join: &str) -> Value {
    json!({ "id": id, "name": name, "anv": "", "join": join, "role": role, "tracks": tracks, "resource_url": format!("https://api.discogs.com/artists/{id}") })
}

/// Discogs' master 401 and its main release: A1–A6 and B1–B6 (the 12 tracks, each 200 s), A4 and B3 by Yann Tiersen and Kerys Lae;
/// the extra artists "Piano – Yann Tiersen", "Vocals – Kerys Lae" (A4, B3) and "Artwork – Lou Kerjean".
fn discogs_all(path: &str) -> Option<Value> {
    if path == format!("/masters/{M401}") {
        return Some(json!({ "id": M401, "title": "ALL", "main_release": R401, "year": 2019, "genres": ["Classical"], "styles": ["Modern Classical"],
            "artists": [dc_artist(DC_YT, "Yann Tiersen", "", "", "")] }));
    }
    if path == format!("/releases/{R401}") {
        let tracklist: Vec<Value> = (1..=12u32)
            .map(|t| {
                let position = if t <= 6 { format!("A{t}") } else { format!("B{}", t - 6) };
                let mut track = json!({ "position": position, "type_": "track", "title": format!("Track {t}"), "duration": "3:20" });
                if matches!(t, 4 | 9) {
                    track["artists"] = json!([dc_artist(DC_YT, "Yann Tiersen", "", "", "Feat."), dc_artist(DC_KL, "Kerys Lae", "", "", "")]);
                }
                track
            })
            .collect();
        return Some(json!({ "id": R401, "title": "ALL", "year": 2019, "master_id": M401,
            "artists": [dc_artist(DC_YT, "Yann Tiersen", "", "", "")],
            "extraartists": [dc_artist(DC_YT, "Yann Tiersen", "Piano", "", ""), dc_artist(DC_KL, "Kerys Lae", "Vocals", "A4, B3", ""), dc_artist(DC_LK, "Lou Kerjean", "Artwork", "", "")],
            "labels": [], "genres": ["Classical"], "styles": ["Modern Classical"], "tracklist": tracklist }));
    }
    None
}

/// The stand-ins' answers for ALL: MusicBrainz's release, Discogs' master and release; anything else none (never a server's error,
/// which would hold a source back for every test in the process).
fn all_answers(a: &Asked) -> Option<(u16, String)> {
    match a.source {
        "musicbrainz" if a.path == format!("/ws/2/release/{REL_ALL}") => Some((200, release_all().to_string())),
        "musicbrainz" if a.path.starts_with("/ws/2/release-group/") => Some((200, json!({ "id": RG_ALL, "relations": [{ "type": "discogs", "url": { "resource": format!("https://www.discogs.com/master/{M401}") } }] }).to_string())),
        "discogs" => Some(discogs_all(&a.path).map_or((404, String::new()), |v| (200, v.to_string()))),
        _ => Some((404, String::new())),
    }
}

/// The server's Discogs key and secret saved (the owner's: the server's key).
fn discogs_key(w: &Whole) {
    w.served.userdata.set_import_choice(crate::app::discogs::KEY, "abcdefgh12345678").unwrap();
    w.served.userdata.set_import_choice(crate::app::discogs::SECRET, "abcdefgh87654321").unwrap();
}

/// One credit of the answer's `credits`, as the table writes it: role, the person (an artist here by id, or Listed by its source's
/// id), its detail, its tracks (none: every track) and its sources.
fn row(role: &str, person: &Value, detail: Option<&str>, tracks: &[u32]) -> (String, Value, Option<String>, Vec<u32>) {
    (role.to_string(), person.clone(), detail.map(str::to_string), tracks.to_vec())
}

/// The answer's credits, each person by role in order: (role, person, detail, tracks) and its `by`, in the order they come.
fn credits_of(answer: &Value) -> Vec<((String, Value, Option<String>, Vec<u32>), Vec<String>)> {
    let Some(roles) = answer["properties"]["credits"].as_array() else { panic!("the entity has no credits: {}", answer["properties"]) };
    let mut out = Vec::new();
    for r in roles {
        let role = r["role"].as_str().unwrap_or_default().to_string();
        for p in r["people"].as_array().into_iter().flatten() {
            let tracks: Vec<u32> = p["tracks"].as_array().into_iter().flatten().filter_map(|t| t.as_u64().map(|t| t as u32)).collect();
            let by: Vec<String> = p["by"].as_array().into_iter().flatten().filter_map(|b| b.as_str().map(str::to_string)).map(|b| if b.starts_with("files@") { "files".to_string() } else { b }).collect();
            out.push(((role.clone(), p["person"].clone(), p["detail"].as_str().map(str::to_string), tracks), by));
        }
    }
    out
}

/// CRED.3, "ALL" end to end: its card, its head, its track rows and its Credits section, the same for alice, carol, the reading key
/// and root. The works pass has read `mb:rel-all`; alice's open's job has read Discogs (`dc:R401`, by MusicBrainz's link). Then: the
/// card, row and chip "Yann Tiersen"; the head one chip, `ar-401`; tracks 4 and 9 "Yann Tiersen feat. Kerys Lae", track 7 "Yann
/// Tiersen & Ewen Morvan"; the answer's `credits` in the one order (T-CRED-7's roles, CRED.18's first track then name, as corrected
/// 2026-11-22 05:50): artist Ewen Morvan (7, files); featuring Kerys Lae (4, 9; files, MusicBrainz, Discogs); composer Yann Tiersen
/// (files, MusicBrainz); performer Yann Tiersen piano (files, MusicBrainz, Discogs), Maëlle Dour violin (2, 3; MusicBrainz, Listed),
/// Kerys Lae vocals (4, 9; files, MusicBrainz, Discogs); producer and mixer Gwen Arzel (Listed `mb:ga`; files, MusicBrainz);
/// mastering Nils Fréjus (Listed `mb:nf`; MusicBrainz); "Artwork" Lou Kerjean (Listed `dc:lk`; Discogs). The Listed persons' open
/// for every account but the reading key. Never Kerys Lae, Ewen Morvan, Maëlle Dour or Gwen Arzel on the card, row, chip or head;
/// never Yann Tiersen under "artist"; never a credit missing for carol or the key; never an artist made for Gwen Arzel, Nils Fréjus,
/// Maëlle Dour or Lou Kerjean; never a file written. Red first: the entity has no credits (dom/view.rs), the files' PRODUCER and
/// MIXER were never kept, MusicBrainz's producer, mix and mastering are dropped at the parse, and no open reads Discogs.
#[tokio::test]
async fn all_shows_one_artist_and_every_credit_by_role() {
    let _state = crate::app::references::TestState::set().await;
    let w = Whole::open("cred-3").await;
    let _instance = Instance::set(&w);
    let a = accounts(&w);
    discogs_key(&w);
    let stand_ins = StandIns::set(Arc::new(all_answers)).await;
    let (all, paths) = plant_all(&w);
    let bytes: Vec<Vec<u8>> = paths.iter().map(|p| std::fs::read(p).unwrap()).collect();
    let cat = w.app.library.catalogue();
    let artist = |name: &str| cat.artist_of(name).unwrap().map(crate::serve::ids::artist);
    let (yt, kl, em) = (artist("Yann Tiersen").expect("ar-401"), artist("Kerys Lae").expect("ar-402"), artist("Ewen Morvan").expect("ar-403"));
    // The works pass reads `mb:rel-all` (its one question), as it runs in the background.
    crate::app::works::pass().await.unwrap();
    assert!(stand_ins.asked().iter().any(|q| q.source == "musicbrainz" && q.path == format!("/ws/2/release/{REL_ALL}")), "the works pass asked mb:rel-all");
    w.served.refresh().unwrap();
    // alice's open of the record begins its Discogs read, a job (CRED.11), by MusicBrainz's link.
    let (status, opened) = ask(&w, ("cookie", &a.alice), Method::POST, "/api/records/open", json!({ "record": all })).await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    until("alice's open's Discogs read", || stand_ins.count("discogs") >= 2 && idle()).await;
    w.served.refresh().unwrap();
    let path = format!("/api/records/{all}?from=library");
    let listed = |source: &str, id: &str| json!({ "listed": { "source": source, "id": id } });
    let here = |id: &str| json!({ "id": id });
    let want = vec![
        (row("artist", &here(&em), None, &[7]), vec!["files"]),
        (row("featuring", &here(&kl), None, &[4, 9]), vec!["files", "musicbrainz", "discogs"]),
        (row("composer", &here(&yt), None, &[]), vec!["files", "musicbrainz"]),
        (row("performer", &here(&yt), Some("piano"), &[]), vec!["files", "musicbrainz", "discogs"]),
        (row("performer", &listed("musicbrainz", MD), Some("violin"), &[2, 3]), vec!["musicbrainz"]),
        (row("performer", &here(&kl), Some("vocals"), &[4, 9]), vec!["files", "musicbrainz", "discogs"]),
        (row("producer", &listed("musicbrainz", GA), None, &[]), vec!["files", "musicbrainz"]),
        (row("mixer", &listed("musicbrainz", GA), None, &[]), vec!["files", "musicbrainz"]),
        (row("mastering", &listed("musicbrainz", NF), None, &[]), vec!["musicbrainz"]),
        (row("Artwork", &listed("discogs", &DC_LK.to_string()), None, &[]), vec!["discogs"]),
    ];
    let want: Vec<_> = want.into_iter().map(|(r, by)| (r, by.into_iter().map(str::to_string).collect::<Vec<_>>())).collect();
    let mut seen: Vec<(&str, Value)> = Vec::new();
    for (who, by) in viewers(&a) {
        let (status, answer) = ask(&w, (by.0, &by.1), Method::GET, &path, Value::Null).await;
        assert_eq!(status, StatusCode::OK, "{who}: {answer}");
        let p = &answer["properties"];
        // The card, row and chip, and the head: the record's artists alone (NAMES.20's order 1), by id.
        assert_eq!(p["credit"], "Yann Tiersen", "{who}: the record's credit, its card's text");
        let heads: Vec<(&str, &str)> = p["artists"]["value"].as_array().into_iter().flatten().map(|x| (x["id"].as_str().unwrap_or(""), x["name"].as_str().unwrap_or(""))).collect();
        assert_eq!(heads, [(yt.as_str(), "Yann Tiersen")], "{who}: the head's one chip");
        let got = credits_of(&answer);
        assert_eq!(got, want, "{who}: the Credits, in the one order, each with its sources");
        assert!(!got.iter().any(|((role, person, _, _), _)| role == "artist" && person["id"] == yt.as_str()), "{who}: Yann Tiersen never under \"artist\"");
        // The Listed persons' open: drawn for every account but the reading key (CRED.12; CN1's H1). By a MusicBrainz id, as
        // trunk's open takes; a Discogs id's open (Lou Kerjean's) is DC3a's M4 (the coordinator, 2026-11-22, Q3).
        for r in p["credits"].as_array().into_iter().flatten() {
            for person in r["people"].as_array().into_iter().flatten().filter(|x| x["person"]["listed"]["source"] == "musicbrainz") {
                assert_eq!(person["open"].is_object(), who != "key", "{who}: {person}");
            }
        }
        seen.push((who, p["credits"].clone()));
    }
    let strip = |v: &Value| -> Value {
        let mut v = v.clone();
        for r in v.as_array_mut().into_iter().flatten() {
            for p in r["people"].as_array_mut().into_iter().flatten() {
                p.as_object_mut().map(|o| o.remove("open"));
            }
        }
        v
    };
    for (who, credits) in &seen[1..] {
        assert_eq!(strip(credits), strip(&seen[0].1), "{who}: the same credits as alice's (facts, not lists)");
    }
    // The served album and its songs: the card's line and the track rows (OBJ5, NAMES.23; the printed phrase, CRED.27).
    let album = w.served.library.album(&all).unwrap().expect("ALL served");
    assert_eq!(album.artist, "Yann Tiersen", "the card's line");
    assert_eq!(album.artists.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Yann Tiersen"], "the card's artists");
    let songs = w.served.library.songs_of_albums(std::slice::from_ref(&all)).unwrap();
    for s in &songs {
        let line = match s.track {
            Some(4 | 9) => "Yann Tiersen feat. Kerys Lae",
            Some(7) => "Yann Tiersen & Ewen Morvan",
            _ => "Yann Tiersen",
        };
        assert_eq!(s.artist, line, "track {:?}'s line", s.track);
    }
    // No artist made for a Listed person, by the pass, the build, a job or a GET; no file written.
    for name in ["Gwen Arzel", "Nils Fréjus", "Maëlle Dour", "Lou Kerjean"] {
        assert_eq!(cat.artist_of(name).unwrap(), None, "no artist made for {name}");
    }
    for (p, b) in paths.iter().zip(&bytes) {
        assert_eq!(&std::fs::read(p).unwrap(), b, "{} unchanged", p.display());
    }
}

/// How many times a path of a source was asked.
fn asked_path(s: &StandIns, source: &str, path: &str) -> usize {
    s.asked().iter().filter(|a| a.source == source && a.path == path).count()
}

/// The answer's credits' roles.
fn roles(answer: &Value) -> Vec<String> {
    answer["properties"]["credits"].as_array().into_iter().flatten().filter_map(|r| r["role"].as_str().map(str::to_string)).collect()
}

/// CRED.11 (kind 2): Discogs' credits are shown to every account, and any account's open asks, as a job, once while fresh (the
/// owner's rules, 2026-11-21 04:16 and 04:43). (a) alice, carol, the key and root GET ALL before anyone opens it: Discogs asked 0
/// times, root's GET too, each answering at once with the credits stored. (b) alice's open answers at once while Discogs is held,
/// and the job it begins reads Discogs once, by MusicBrainz's link. (c) dave's open while alice's read runs asks nothing more. (d)
/// carol's open once it landed asks nothing (fresh), and alice, carol, the key and root see Discogs' credits ("Artwork"). (h) 31
/// days on, Discogs switched off: bob's open asks nothing and queues nothing, the stored credits stay. (i) switched on again: carol's
/// open asks once; `GET /api/albums/cratarr/{id}/discogs` asks nothing and answers what is stored, to alice too. Never a Discogs read
/// started by a GET, any account's; never Discogs asked twice for one due read or while fresh; never while switched off; never a
/// stored credit hidden from any account; never a GET or an open waiting on Discogs. Red first: no open reads Discogs; the Discogs
/// route reads on its GET and sits in the administrators' block (api.rs:335, :377).
#[tokio::test]
async fn discogs_credits_show_to_all_and_ask_at_any_accounts_open() {
    let _state = crate::app::references::TestState::set().await;
    let w = Whole::open("cred-11").await;
    let _instance = Instance::set(&w);
    let a = accounts(&w);
    let (bob, dave) = (w.session("bob"), w.session("dave"));
    discogs_key(&w);
    let stand_ins = StandIns::set(Arc::new(all_answers)).await;
    let (all, _) = plant_all(&w);
    crate::app::works::pass().await.unwrap();
    w.served.refresh().unwrap();
    let master = format!("/masters/{M401}");
    let path = format!("/api/records/{all}?from=library");
    // (a) Every viewer's GET before any open: nothing asked of Discogs.
    stand_ins.clear();
    for (who, by) in viewers(&a) {
        let (status, answer) = tokio::time::timeout(std::time::Duration::from_secs(10), ask(&w, (by.0, &by.1), Method::GET, &path, Value::Null)).await.expect("a GET never waits");
        assert_eq!(status, StatusCode::OK, "{who}: {answer}");
        assert!(roles(&answer).contains(&"composer".to_string()), "{who}: the stored credits at once (the files', MusicBrainz's): {answer}");
    }
    assert_eq!(stand_ins.count("discogs"), 0, "(a) no GET starts a Discogs read, root's neither");
    // (b) alice's open, Discogs held: answered at once; its job reads Discogs once it answers.
    stand_ins.hold("discogs");
    let open = json!({ "record": all });
    let (status, opened) = tokio::time::timeout(std::time::Duration::from_secs(10), ask(&w, ("cookie", &a.alice), Method::POST, "/api/records/open", open.clone())).await.expect("the open never waits on Discogs");
    assert_eq!(status, StatusCode::OK, "{opened}");
    until("alice's open's job asking Discogs", || asked_path(&stand_ins, "discogs", &master) == 1).await;
    // (c) dave's open while it runs: nothing more.
    let (status, _) = ask(&w, ("cookie", &dave), Method::POST, "/api/records/open", open.clone()).await;
    assert_eq!(status, StatusCode::OK);
    stand_ins.let_go();
    until("alice's read landed", || idle()).await;
    assert_eq!(asked_path(&stand_ins, "discogs", &master), 1, "(c) one read per record at a time");
    // (d) carol's open, fresh: nothing; every viewer sees Discogs' credits.
    let (status, _) = ask(&w, ("cookie", &a.carol), Method::POST, "/api/records/open", open.clone()).await;
    assert_eq!(status, StatusCode::OK);
    until("carol's open settled", || idle()).await;
    assert_eq!(asked_path(&stand_ins, "discogs", &master), 1, "(d) fresh facts: nothing asked");
    w.served.refresh().unwrap();
    for (who, by) in viewers(&a) {
        let (_, answer) = ask(&w, (by.0, &by.1), Method::GET, &path, Value::Null).await;
        assert!(roles(&answer).contains(&"Artwork".to_string()), "(d) {who} sees Discogs' credits: {}", answer["properties"]["credits"]);
    }
    // (h) 31 days on, Discogs off: bob's open asks nothing and queues nothing; the stored credits stay.
    let subject = format!("album:{all}");
    let kept = w.served.userdata.discogs_facts(&subject).unwrap().expect("ALL's Discogs facts kept");
    let old = crate::app::moment::now_secs() - 31 * 86_400;
    w.served.userdata.set_discogs_facts(&subject, kept.1.as_deref(), old).unwrap();
    crate::app::switches::set_for_tests(crate::app::switches::DISCOGS, false);
    let (status, _) = ask(&w, ("cookie", &bob), Method::POST, "/api/records/open", open.clone()).await;
    assert_eq!(status, StatusCode::OK);
    until("bob's open settled", || idle()).await;
    assert_eq!(asked_path(&stand_ins, "discogs", &master), 1, "(h) switched off: nothing asked");
    assert!(crate::runner::runner().rows().iter().all(|r| !r.kind.contains("discogs")), "(h) nothing queued for later");
    let (_, answer) = ask(&w, ("cookie", &bob), Method::GET, &path, Value::Null).await;
    assert!(roles(&answer).contains(&"Artwork".to_string()), "(h) the stored credits stay and show");
    // (i) On again: carol's open asks once; the route's GET asks nothing and answers what is stored, to alice too.
    crate::app::switches::set_for_tests(crate::app::switches::DISCOGS, true);
    let (status, _) = ask(&w, ("cookie", &a.carol), Method::POST, "/api/records/open", open.clone()).await;
    assert_eq!(status, StatusCode::OK);
    until("carol's read", || asked_path(&stand_ins, "discogs", &master) == 2 && idle()).await;
    let before = stand_ins.count("discogs");
    let (status, facts) = ask(&w, ("cookie", &a.alice), Method::GET, &format!("/api/albums/cratarr/{all}/discogs"), Value::Null).await;
    assert_eq!(status, StatusCode::OK, "(i) the route answers every account: {facts}");
    assert_eq!(facts["master"], M401, "(i) the facts stored: {facts}");
    assert_eq!(stand_ins.count("discogs"), before, "(i) the route's GET asks nothing");
}

/// Every row count CRED.21 watches: the catalogue's artists, references, lists, record artists and labels, facts and credits; the
/// user data's Discogs facts.
fn counts(w: &Whole) -> Vec<(&'static str, i64)> {
    let cat = w.app.library.catalogue();
    let db = cat.db_for_tests();
    let mut out: Vec<(&'static str, i64)> = ["artists", "\"references\"", "reference_tracks", "record_artists", "record_labels", "facts", "credits"]
        .into_iter()
        .map(|t| (t, db.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0)).unwrap()))
        .collect();
    drop(db);
    out.push(("discogs_facts", w.served.userdata.discogs_facts_of("").unwrap().len() as i64));
    out
}

/// CRED.21: a GET makes no artist, keeps no reference, ties nothing, and asks no outside source (the owner's 04:43 rule). ALL and
/// "Quiet Fields" (8 files, no genre, no reference; a Discogs search by its names would find a master), their Discogs facts due.
/// Each GET that reads credits or reached `keep_discogs`: the record's answer (alice, root), the album Discogs route (alice, root),
/// the album plan `intake:{id}` for Quiet Fields and the library plans `fix:library:genre` and `:year` (root). Before and after
/// each, the row counts of the catalogue and the user data are the same, and the stand-ins count no Discogs or MusicBrainz ask.
/// Never `keep_discogs` reached from a GET. Red first: three GET paths reach `discogs_facts::album` and `keep_discogs` (the album
/// route, the album plan's `of_album`, the library plans' `fill`).
#[tokio::test]
async fn a_credits_get_makes_and_keeps_nothing() {
    let _state = crate::app::references::TestState::set().await;
    let w = Whole::open("cred-21").await;
    let _instance = Instance::set(&w);
    let a = accounts(&w);
    discogs_key(&w);
    let stand_ins = StandIns::set(Arc::new(|q: &Asked| match q.source {
        "discogs" if q.path == "/database/search" => Some((200, json!({ "results": [{ "id": 441, "title": "Quiet Pine - Quiet Fields" }] }).to_string())),
        _ => all_answers(q),
    }))
    .await;
    // Quiet Fields: 8 files, no genre, no MusicBrainz release, no Discogs reference; planted with ALL.
    let quiet_files: Vec<(String, Vec<(String, String)>)> = (1..=8u32)
        .map(|t| {
            let tags = [("album", "Quiet Fields".to_string()), ("albumartist", "Quiet Pine".into()), ("artist", "Quiet Pine".into()), ("title", format!("Field {t}")), ("tracknumber", t.to_string())];
            (format!("Quiet Pine/Quiet Fields/{t:02} Field {t}.flac"), tags.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
        })
        .collect();
    let (all, _) = plant_all_with(&w, &quiet_files);
    let quiet = w.served.library.album_in(0, "Quiet Pine/Quiet Fields").unwrap().expect("Quiet Fields built");
    crate::app::works::pass().await.unwrap();
    w.served.refresh().unwrap();
    stand_ins.clear();
    let gets: Vec<(&str, String, (&str, String))> = vec![
        ("the record's answer", format!("/api/records/{all}?from=library"), ("cookie", a.alice.clone())),
        ("the record's answer", format!("/api/records/{all}?from=library"), ("cookie", a.root.clone())),
        ("the album Discogs route", format!("/api/albums/cratarr/{all}/discogs"), ("cookie", a.alice.clone())),
        ("the album Discogs route", format!("/api/albums/cratarr/{all}/discogs"), ("cookie", a.root.clone())),
        ("the album Discogs route", format!("/api/albums/cratarr/{quiet}/discogs"), ("cookie", a.root.clone())),
        ("the album plan", format!("/api/file-changes/plans/intake:{quiet}"), ("cookie", a.root.clone())),
        ("the library plans", "/api/file-changes/plans/fix:library:genre".to_string(), ("cookie", a.root.clone())),
        ("the library plans", "/api/file-changes/plans/fix:library:year".to_string(), ("cookie", a.root.clone())),
    ];
    for (what, path, by) in gets {
        let before = counts(&w);
        let (status, answer) = tokio::time::timeout(std::time::Duration::from_secs(20), ask(&w, (by.0, &by.1), Method::GET, &path, Value::Null)).await.expect("a GET never waits on a source");
        // What the GET left behind, given the time to run (a task it began would ask now).
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        assert_ne!(status, StatusCode::FORBIDDEN, "{what} {path}: every account's: {answer}");
        assert_eq!(counts(&w), before, "{what} {path}: rows written by a GET (a fact, a reference, a list, a credit, Discogs' facts)");
        if what.starts_with("the album plan") || what.starts_with("the library plans") {
            // Not counted here (CRED.21, the recheck's M-A): a plan's other asks, matching's find and the cover look, are the GET
            // inventory's (GEN.58 refuses the plan GETs, lane C; the cover look is `POST …/covers/look`, LK.B3). What CRED.21 counts
            // is Discogs' facts read (`discogs_facts::album`), whose release ask no cover look makes.
            assert!(!stand_ins.asked().iter().any(|q| q.source == "discogs" && q.path.starts_with("/releases/")), "{what} {path}: Discogs' facts read by a GET: {:?}", stand_ins.asked());
        } else {
            assert_eq!(stand_ins.count("discogs"), 0, "{what} {path}: Discogs asked by a GET: {:?}", stand_ins.asked());
            assert_eq!(stand_ins.count("musicbrainz"), 0, "{what} {path}: MusicBrainz asked by a GET: {:?}", stand_ins.asked());
        }
        stand_ins.clear();
    }
}

/// Northern Rooms' release (`mb:rel-nr`) and Discogs' release of it, and its people's ids (SCENARIOS-CRED's fixture).
const REL_NR: &str = "c0000000-0000-4000-8000-000000000411";
const OH: &str = "c0000000-0000-4000-8000-0000000a0411";
const SV: &str = "c0000000-0000-4000-8000-0000000a0412";
const DB1: &str = "c0000000-0000-4000-8000-0000000a0413";
const DB2: &str = "c0000000-0000-4000-8000-0000000a0414";
const PO: &str = "c0000000-0000-4000-8000-0000000a0415";
const RF: &str = "c0000000-0000-4000-8000-0000000a0418";
const ML1: &str = "c0000000-0000-4000-8000-0000000a0419";
const ML2: &str = "c0000000-0000-4000-8000-0000000a041a";
const R411: u64 = 4110;
const DC_OH: u64 = 90411;
const DC_SV: u64 = 90412;

/// "Northern Rooms" as SCENARIOS-CRED's fixture: 6 FLAC files, ALBUMARTIST Rowan Fell (`mb:rf`), MUSICBRAINZ_ALBUMID `mb:rel-nr`,
/// each track's ARTISTS ["Rowan Fell", "Pale Orchad"] (the typo) on 1–5 and ["Rowan Fell", "Pale Orchard"] on 6, with their ids;
/// `extra` tags on every file. The artists here: `ar-411` Ólafur Hrafnsson (`mb:oh`), two Daniel Bergs (`mb:db1`, `mb:db2`), Pale
/// Orchard (`mb:po`), Mara Lind (`mb:ml1`), Glass Harbour, two Kelpes. Its album id and record number.
fn plant_northern_rooms(w: &Whole, extra: &[(&str, &str)]) -> (String, i64) {
    let cat = w.app.library.catalogue();
    let mut rels = Vec::new();
    for t in 1..=6u32 {
        let rel = format!("Rowan Fell/Northern Rooms/{t:02} Room {t}.flac");
        let path = w.dir().join(&rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &rel).unwrap();
        rels.push((rel, t));
    }
    cat.walk_all();
    cat.pretend_read_for_tests();
    for (rel, t) in &rels {
        let (title, number) = (format!("Room {t}"), t.to_string());
        let other = if *t == 6 { "Pale Orchard" } else { "Pale Orchad" };
        let mut tags: Vec<(&str, &str)> = vec![
            ("album", "Northern Rooms"),
            ("albumartist", "Rowan Fell"),
            ("musicbrainz_albumartistid", RF),
            ("musicbrainz_albumid", REL_NR),
            ("title", title.as_str()),
            ("tracknumber", number.as_str()),
            ("artist", "Rowan Fell / Pale Orchard"),
            ("artists", "Rowan Fell"),
            ("artists", other),
            ("musicbrainz_artistid", RF),
            ("musicbrainz_artistid", PO),
        ];
        tags.extend_from_slice(extra);
        cat.tag_for_tests(0, rel, &tags);
    }
    cat.identify().unwrap();
    w.served.refresh().unwrap();
    let album = w.served.library.album_in(0, "Rowan Fell/Northern Rooms").unwrap().expect("Northern Rooms built");
    let by = |id: &str, name: &str| cat.artist_by_reference(&Ref::new("musicbrainz", "artist", id), name, "files", true).unwrap();
    by(OH, "Ólafur Hrafnsson");
    by(PO, "Pale Orchard");
    by(ML1, "Mara Lind");
    let berg = cat.artist_for("Daniel Berg").unwrap();
    cat.artist_apart("Daniel Berg").unwrap();
    cat.set_reference(&crate::serve::ids::artist(berg), &Ref::new("musicbrainz", "artist", DB1), "hand").unwrap();
    cat.artist_for("Glass Harbour").unwrap();
    cat.artist_for("Kelpe").unwrap();
    cat.artist_apart("Kelpe").unwrap();
    w.served.refresh().unwrap();
    let record = crate::serve::ids::record_number(&album).unwrap();
    (album, record)
}

/// A service's credit as the credits store keeps it: on every track.
fn said(person: &str, name: &str, word: &str) -> crate::catalogue::credits::CreditRow {
    crate::catalogue::credits::CreditRow { person: Some(person.to_string()), name: name.to_string(), word: word.to_string(), ..Default::default() }
}

/// The people under one role: (person, name, number, by).
fn people(answer: &Value, role: &str) -> Vec<(Value, String, Option<i64>, Vec<String>)> {
    answer["properties"]["credits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["role"] == role)
        .flat_map(|r| r["people"].as_array().cloned().unwrap_or_default())
        .map(|p| {
            let by = p["by"].as_array().into_iter().flatten().filter_map(|b| b.as_str()).map(|b| if b.starts_with("files@") { "files".to_string() } else { b.to_string() }).collect();
            (p["person"].clone(), p["name"].as_str().unwrap_or_default().to_string(), p["number"].as_i64(), by)
        })
        .collect()
}

/// A record's rows owed to the change log now.
fn owed(w: &Whole, record: i64) -> i64 {
    w.app.library.catalogue().db_for_tests().query_row("SELECT count(*) FROM record_owed WHERE record = ?1", [record], |r| r.get(0)).unwrap()
}

/// CRED.8: two sources name one person by two spellings, merged by id, never by a name. Northern Rooms: (b) MusicBrainz's mastering
/// "Ólafur Hrafnsson" (`mb:oh`, held by `ar-411`) and Discogs' "Olafur Hrafnsson" (`dc:oh`) before `ar-411` holds `dc:oh`: two
/// credits, one per source; then `ar-411` holds it (its monthly read, a job): one, logged once; (a) one credit, mastering, by
/// `ar-411`'s one name, by MusicBrainz and Discogs; (c) MusicBrainz's and Discogs' producer "Sanne Vos", no artist here and no link:
/// two, each opening its own reference; (d) the files' track artists "Pale Orchad" (1–5) and "Pale Orchard" (6), each beside
/// `mb:po`, held by Pale Orchard: one "artist" credit, by its one name, on every track. Never two people joined by a name; never one
/// shown twice once an artist here holds both ids; never "Pale Orchad" or "Olafur Hrafnsson" shown as an artist's name. Red first:
/// no store; Discogs' credits keep no id.
#[tokio::test]
async fn credits_merge_by_id_never_by_name() {
    let _state = crate::app::references::TestState::set().await;
    let w = Whole::open("cred-8").await;
    let _instance = Instance::set(&w);
    let a = accounts(&w);
    let (album, record) = plant_northern_rooms(&w, &[]);
    let cat = w.app.library.catalogue();
    let now = crate::app::moment::now_secs();
    cat.keep_credits(record, "musicbrainz", &format!("release:{REL_NR}"), Some(&[said(OH, "Ólafur Hrafnsson", "mastering"), said(SV, "Sanne Vos", "producer")]), false, now).unwrap();
    cat.keep_credits(record, "discogs", &format!("release:{R411}"), Some(&[said(&DC_OH.to_string(), "Olafur Hrafnsson", "Mastered By"), said(&DC_SV.to_string(), "Sanne Vos", "Producer")]), false, now).unwrap();
    w.served.refresh().unwrap();
    let path = format!("/api/records/{album}?from=library");
    let get = || async { ask(&w, ("cookie", &a.alice), Method::GET, &path, Value::Null).await.1 };
    let oh = cat.artist_of("Ólafur Hrafnsson").unwrap().map(crate::serve::ids::artist).expect("ar-411");
    // (b) Before ar-411 holds dc:oh: one credit per source.
    let answer = get().await;
    let mastering = people(&answer, "mastering");
    assert_eq!(mastering.len(), 2, "(b) two, one per source, until ar-411 holds dc:oh: {mastering:?}");
    // Its monthly read keeps dc:oh: one credit, the record's change logged once.
    assert_eq!(owed(&w, record), 0, "nothing owed before");
    cat.set_reference(&oh, &Ref::new("discogs", "artist", &DC_OH.to_string()), "search").unwrap();
    assert_eq!(owed(&w, record), 1, "(b) the record's change logged once, as ar-411 came to hold dc:oh");
    w.served.refresh().unwrap();
    let answer = get().await;
    // (a) One credit, by ar-411's one name, by both sources.
    assert_eq!(people(&answer, "mastering"), [(json!({ "id": oh }), "Ólafur Hrafnsson".to_string(), None, vec!["musicbrainz".to_string(), "discogs".to_string()])], "(a)");
    // (c) Two Sanne Vos, each Listed by its own source's id.
    let sv: Vec<Value> = people(&answer, "producer").into_iter().map(|p| p.0).collect();
    assert_eq!(sv, [json!({ "listed": { "source": "musicbrainz", "id": SV } }), json!({ "listed": { "source": "discogs", "id": DC_SV.to_string() } })], "(c) never joined by the name");
    // (d) One "artist" credit, Pale Orchard by its one name, on every track.
    let po = cat.artist_of("Pale Orchard").unwrap().map(crate::serve::ids::artist).expect("Pale Orchard");
    assert_eq!(people(&answer, "artist"), [(json!({ "id": po }), "Pale Orchard".to_string(), None, vec!["files".to_string()])], "(d)");
    let text = answer["properties"]["credits"].to_string();
    assert!(!text.contains("Pale Orchad") && !text.contains("\"Olafur Hrafnsson\""), "(d), (a): a spelling is never shown as an artist's name: {text}");
}

/// CRED.9 (kind 1): namesakes are credited apart; a name the files give alone. Northern Rooms, its files saying ENGINEER "Daniel
/// Berg", COMPOSER "Rena Abt", PRODUCER "Glass Harbour" and "Kelpe", and ENGINEER "Mara Lind". (b) with no source naming an
/// engineer: "Daniel Berg" a name alone, text, neither artist (two here hold that name); (d) "Rena Abt" text, nothing made; (e)
/// Glass Harbour, the one artist of that name, a chip; (f) "Kelpe", two here: text. Then MusicBrainz credits engineer `mb:db2` and
/// engineer `mb:ml2` "Mara Lind": (c), (a) one credit, `ar-413`, "Daniel Berg" numbered 2, by the files and MusicBrainz (the same
/// record's source names who the files' name means); (g) one credit, Listed `mb:ml2`, by the files and MusicBrainz: the same record's
/// evidence before a library-wide name, so `ar-419` Mara Lind gets none. Never a name-only credit given to C55's first; `mb:db2`'s on
/// `ar-412`; an artist made for "Rena Abt"; a chip for (b), (d) or (f); (g) split in two or given to `ar-419`. Red first: no store.
#[tokio::test]
async fn namesakes_are_credited_apart() {
    let _state = crate::app::references::TestState::set().await;
    let w = Whole::open("cred-9").await;
    let _instance = Instance::set(&w);
    let a = accounts(&w);
    let extra = [("engineer", "Daniel Berg"), ("composer", "Rena Abt"), ("producer", "Glass Harbour"), ("producer", "Kelpe"), ("engineer", "Mara Lind")];
    let (album, record) = plant_northern_rooms(&w, &extra);
    let cat = w.app.library.catalogue();
    let bergs: Vec<i64> = cat.db_for_tests().prepare("SELECT id FROM artists WHERE name = 'Daniel Berg' ORDER BY id").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(bergs.len(), 2, "the fixture: two Daniel Bergs");
    cat.set_reference(&crate::serve::ids::artist(bergs[1]), &Ref::new("musicbrainz", "artist", DB2), "hand").unwrap();
    let ml1 = cat.artist_of("Mara Lind").unwrap().map(crate::serve::ids::artist).expect("ar-419");
    let path = format!("/api/records/{album}?from=library");
    let answer = ask(&w, ("cookie", &a.alice), Method::GET, &path, Value::Null).await.1;
    let named = |role: &str, name: &str| people(&answer, role).into_iter().find(|p| p.1 == name).unwrap_or_else(|| panic!("{role} {name}: {}", answer["properties"]["credits"]));
    assert_eq!(named("engineer", "Daniel Berg").0, Value::Null, "(b) a name alone: text, neither artist");
    assert_eq!(named("composer", "Rena Abt").0, Value::Null, "(d) text");
    assert_eq!(cat.artist_of("Rena Abt").unwrap(), None, "(d) nothing made");
    let gh = cat.artist_of("Glass Harbour").unwrap().map(crate::serve::ids::artist).expect("Glass Harbour");
    assert_eq!(named("producer", "Glass Harbour").0, json!({ "id": gh }), "(e) the one artist of that name");
    assert_eq!(named("producer", "Kelpe").0, Value::Null, "(f) two Kelpes: text");
    let now = crate::app::moment::now_secs();
    cat.keep_credits(record, "musicbrainz", &format!("release:{REL_NR}"), Some(&[said(DB2, "Daniel Berg", "engineer"), said(ML2, "Mara Lind", "engineer")]), false, now).unwrap();
    w.served.refresh().unwrap();
    let answer = ask(&w, ("cookie", &a.alice), Method::GET, &path, Value::Null).await.1;
    let engineers = people(&answer, "engineer");
    let berg2 = crate::serve::ids::artist(bergs[1]);
    assert_eq!(
        engineers,
        [
            (json!({ "id": berg2 }), "Daniel Berg".to_string(), Some(2), vec!["files".to_string(), "musicbrainz".to_string()]),
            (json!({ "listed": { "source": "musicbrainz", "id": ML2 } }), "Mara Lind".to_string(), None, vec!["files".to_string(), "musicbrainz".to_string()]),
        ],
        "(a), (c), (g)"
    );
    assert!(!answer["properties"]["credits"].to_string().contains(&format!("\"{ml1}\"")), "(g) ar-419 credited by its name");
}

/// CRED.10: a role only one source gives shows with it. ALL with Discogs' "Artwork – Lou Kerjean" and "Liner Notes – Ana Ríos" (a
/// word the roles table has no row for), MusicBrainz's mastering, the files' mixer: each under its role, its one source in `by`;
/// "Liner Notes" under Discogs' own word, after the table's roles. Never a credit hidden until a second source names it, a role
/// dropped for having no row, a credit marked as differing. Red first: AlbumAbout shows Discogs' roles only, to administrators.
#[tokio::test]
async fn a_credit_one_source_gives_shows_with_it() {
    let _state = crate::app::references::TestState::set().await;
    let w = Whole::open("cred-10").await;
    let _instance = Instance::set(&w);
    let a = accounts(&w);
    let (all, _) = plant_all(&w);
    let record = crate::serve::ids::record_number(&all).unwrap();
    let cat = w.app.library.catalogue();
    let now = crate::app::moment::now_secs();
    cat.keep_credits(record, "musicbrainz", &format!("release:{REL_ALL}"), Some(&[said(NF, "Nils Fréjus", "mastering")]), false, now).unwrap();
    cat.keep_credits(record, "discogs", &format!("release:{R401}"), Some(&[said(&DC_LK.to_string(), "Lou Kerjean", "Artwork"), said("90409", "Ana Ríos", "Liner Notes")]), false, now).unwrap();
    w.served.refresh().unwrap();
    let answer = ask(&w, ("cookie", &a.carol), Method::GET, &format!("/api/records/{all}?from=library"), Value::Null).await.1;
    let r = roles(&answer);
    let at = |role: &str| r.iter().position(|x| x == role).unwrap_or_else(|| panic!("{role} missing: {r:?}"));
    assert!(at("mastering") < at("Artwork") && at("mastering") < at("Liner Notes"), "the words no row names after the table's roles: {r:?}");
    for (role, by) in [("mastering", "musicbrainz"), ("Artwork", "discogs"), ("Liner Notes", "discogs"), ("mixer", "files")] {
        let p = people(&answer, role);
        assert_eq!(p.len(), 1, "{role}: {p:?}");
        assert_eq!(p[0].3, [by.to_string()], "{role} with its one source");
    }
    assert!(!answer["properties"]["credits"].to_string().contains("versions"), "no credit marked as a version");
}

/// CRED.12: a credited person with no artist here is Listed, and made at an open. ALL's Listed persons (Maëlle Dour `mb:md`, Gwen
/// Arzel `mb:ga`, Nils Fréjus `mb:nf`, Lou Kerjean `dc:lk`). (a) the panel's GET: each Listed with its name, and no artist, number,
/// reference or record artist made (counted before and after). (b) alice taps Maëlle Dour (`POST /api/artists/open` with her
/// MusicBrainz id and name; alice no administrator, Beyond never listed her: T-CRED-9, a stored credit names her): her artist is
/// made, kept at open, and the credit names it; one `record` change-log row tells ALL's panel. (d) again: the same artist. (f) a day
/// later the forgetting keeps her while a credit names her (T-CRED-10). Never an artist made by a GET; never two artists for
/// `mb:md`; never an artist forgotten while a credit names it. (c), an open by Discogs' id, is DC3a's M4. Red first: there are no
/// credits; `open_artist` refuses a non-administrator unless Beyond listed the id within a day (api.rs:6716-6719).
#[tokio::test]
async fn a_credited_person_is_made_at_its_open() {
    let _state = crate::app::references::TestState::set().await;
    let w = Whole::open("cred-12").await;
    let _instance = Instance::set(&w);
    let a = accounts(&w);
    let stand_ins = StandIns::set(Arc::new(|_: &Asked| Some((404, String::new())))).await;
    let (all, _) = plant_all(&w);
    let record = crate::serve::ids::record_number(&all).unwrap();
    let cat = w.app.library.catalogue();
    let now = crate::app::moment::now_secs();
    let mut violin = said(MD, "Maëlle Dour", "instrument");
    violin.attributes = vec!["guest".into(), "violin".into()];
    violin.track = vec![(1, 2), (1, 3)];
    cat.keep_credits(record, "musicbrainz", &format!("release:{REL_ALL}"), Some(&[violin, said(GA, "Gwen Arzel", "producer"), said(NF, "Nils Fréjus", "mastering")]), false, now).unwrap();
    cat.keep_credits(record, "discogs", &format!("release:{R401}"), Some(&[said(&DC_LK.to_string(), "Lou Kerjean", "Artwork")]), false, now).unwrap();
    w.served.refresh().unwrap();
    let path = format!("/api/records/{all}?from=library");
    // (a) The panel's GET: Listed persons, nothing made.
    let before = counts(&w);
    let answer = ask(&w, ("cookie", &a.alice), Method::GET, &path, Value::Null).await.1;
    assert_eq!(counts(&w), before, "(a) a GET makes nothing");
    let md = people(&answer, "performer").into_iter().find(|p| p.1 == "Maëlle Dour").expect("Maëlle Dour");
    assert_eq!(md.0, json!({ "listed": { "source": "musicbrainz", "id": MD } }), "(a) Listed by her MusicBrainz id");
    // (b) alice taps her.
    let (status, opened) = ask(&w, ("cookie", &a.alice), Method::POST, "/api/artists/open", json!({ "mbid": MD, "name": "Maëlle Dour" })).await;
    assert_eq!(status, StatusCode::OK, "(b) alice may open a person a stored credit names: {opened}");
    let artist = opened["artist"].as_str().expect("her artist").to_string();
    assert_eq!(owed(&w, record), 1, "(b) one record change-log row tells ALL's panel");
    w.served.refresh().unwrap();
    let answer = ask(&w, ("cookie", &a.alice), Method::GET, &path, Value::Null).await.1;
    let md = people(&answer, "performer").into_iter().find(|p| p.1 == "Maëlle Dour").expect("Maëlle Dour");
    assert_eq!(md.0, json!({ "id": artist }), "(b) the credit now names her artist");
    // (d) Again: the same artist.
    let (_, again) = ask(&w, ("cookie", &a.alice), Method::POST, "/api/artists/open", json!({ "mbid": MD, "name": "Maëlle Dour" })).await;
    assert_eq!(again["artist"], artist.as_str(), "(d) no second artist");
    // (f) A day later, the forgetting keeps her while a credit names her.
    let id = crate::serve::ids::artist_number(&artist).expect("an artist's id");
    let gone = cat.forget_quiet_artists(crate::app::moment::now_secs() + 3 * 86_400, &Default::default()).unwrap();
    assert!(!gone.contains(&id), "(f) kept while a credit names her: {gone:?}");
    drop(stand_ins);
}

/// CRED.17's Run, pinned (the coordinator, 2026-11-22, Q4): the catalogue's 0.18 step parts run in `step_018::PARTS`' order, `credits`
/// last, so DC3a inserts NAMES.16's part and UPG.1's before it ("credits resolve to artist ids"), never after. Read from main.rs's
/// own calls, in the order they stand.
#[test]
fn the_step_parts_run_in_order_credits_last() {
    use crate::catalogue::step_018::{COVERS, CREDITS, FACTS, FOLD, PARTS, WM};
    let main = include_str!("main.rs");
    let at = |call: &str| main.find(call).unwrap_or_else(|| panic!("main.rs calls {call}"));
    let calls = [(FACTS, at("step_018_facts(&served)")), (WM, at(".step_018_wm(")), (FOLD, at(".fold_018(")), (COVERS, at(".step_018_covers(")), (CREDITS, at("step_018_credits(&served)"))];
    let mut ran: Vec<(usize, &str)> = calls.iter().map(|(part, i)| (*i, *part)).collect();
    ran.sort();
    assert_eq!(ran.into_iter().map(|(_, p)| p).collect::<Vec<_>>(), PARTS.to_vec(), "the parts in PARTS' order");
    assert_eq!(PARTS.last(), Some(&CREDITS), "credits last: NAMES.16's part and UPG.1's go before it");
}

/// CRED.13 (kind 6), the server's half, a guard, and CRED.25's server half (the Credits section and the track credits): no credit
/// widens a record's artists. ALL with every source's credits: its served album artist, the entity's `credit` and artists, and the
/// record's artists in the catalogue stay Yann Tiersen alone. "Same Boat" (3 files, ALBUMARTIST Rowan Fell, every track "Rowan Fell
/// feat. Oda Brenn" with ARTISTS both): its record's artist is Rowan Fell alone; Oda Brenn a Featuring credit on every track, never
/// under "artist", never a record artist, though every track credits her. Never a role credit among the record's artists, its
/// `credit`, its served `artist` string; never a featured artist raised to the record. Today: a guard (the card, row and chip are
/// NAMES'); a naive build feeding credits into the record's artists breaks it.
#[tokio::test]
async fn credits_never_widen_a_records_artists() {
    let _state = crate::app::references::TestState::set().await;
    let w = Whole::open("cred-13").await;
    let _instance = Instance::set(&w);
    let a = accounts(&w);
    let boat: Vec<(String, Vec<(String, String)>)> = (1..=3u32)
        .map(|t| {
            let tags = [
                ("album", "Same Boat".to_string()),
                ("albumartist", "Rowan Fell".into()),
                ("artist", "Rowan Fell feat. Oda Brenn".into()),
                ("artists", "Rowan Fell".into()),
                ("artists", "Oda Brenn".into()),
                ("title", format!("Oar {t}")),
                ("tracknumber", t.to_string()),
            ];
            (format!("Rowan Fell/Same Boat/{t:02} Oar {t}.flac"), tags.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
        })
        .collect();
    let (all, _) = plant_all_with(&w, &boat);
    let record = crate::serve::ids::record_number(&all).unwrap();
    let cat = w.app.library.catalogue();
    let now = crate::app::moment::now_secs();
    cat.keep_credits(record, "musicbrainz", &format!("release:{REL_ALL}"), Some(&[said(GA, "Gwen Arzel", "producer"), said(NF, "Nils Fréjus", "mastering")]), false, now).unwrap();
    cat.keep_credits(record, "discogs", &format!("release:{R401}"), Some(&[said(&DC_LK.to_string(), "Lou Kerjean", "Artwork")]), false, now).unwrap();
    w.served.refresh().unwrap();
    let before = cat.record_artists(record).unwrap();
    let same_boat = w.served.library.album_in(0, "Rowan Fell/Same Boat").unwrap().expect("Same Boat built");
    for (album, artist) in [(all.clone(), "Yann Tiersen"), (same_boat, "Rowan Fell")] {
        let answer = ask(&w, ("cookie", &a.alice), Method::GET, &format!("/api/records/{album}?from=library"), Value::Null).await.1;
        let p = &answer["properties"];
        assert_eq!(p["credit"], artist, "{album}: the entity's credit");
        let heads: Vec<&str> = p["artists"]["value"].as_array().into_iter().flatten().filter_map(|x| x["name"].as_str()).collect();
        assert_eq!(heads, [artist], "{album}: the head");
        assert_eq!(w.served.library.album(&album).unwrap().expect("served").artist, artist, "{album}: the served album artist");
        if artist == "Rowan Fell" {
            let featuring = people(&answer, "featuring");
            assert_eq!(featuring.len(), 1, "Same Boat: Oda Brenn featuring: {}", p["credits"]);
            assert_eq!(featuring[0].1, "Oda Brenn");
            let on_every = p["credits"].as_array().into_iter().flatten().filter(|r| r["role"] == "featuring").all(|r| r["people"][0]["tracks"].is_null());
            assert!(on_every, "on every track");
            assert!(people(&answer, "artist").iter().all(|p| p.1 != "Oda Brenn"), "never under \"artist\"");
        }
    }
    assert_eq!(cat.record_artists(record).unwrap(), before, "no credit makes a record artist");
}

/// CRED.14 (kind 6), a guard: role credits feed no list, filter, Out, play, search or holding. ALL's producer Gwen Arzel (the files',
/// MusicBrainz's) and its other role credits leave the record's artists and its songs' artists as they were: Gwen Arzel credited on
/// no song and no record, no artist made of her. Never a role credit read as an artist credit by `record_artists` or the served
/// songs. Today: green (there are no credits); a naive build writing credits into `song_artists` or `record_artists` breaks it.
#[tokio::test]
async fn role_credits_change_no_list_or_rule() {
    let _state = crate::app::references::TestState::set().await;
    let w = Whole::open("cred-14").await;
    let _instance = Instance::set(&w);
    let (all, _) = plant_all(&w);
    let record = crate::serve::ids::record_number(&all).unwrap();
    let cat = w.app.library.catalogue();
    let songs = |w: &Whole| -> Vec<Vec<String>> {
        w.served.library.songs_of_albums(std::slice::from_ref(&all)).unwrap().into_iter().map(|s| s.artists.into_iter().map(|a| a.name).collect()).collect()
    };
    let (artists, song_artists) = (cat.record_artists(record).unwrap(), songs(&w));
    cat.keep_credits(record, "musicbrainz", &format!("release:{REL_ALL}"), Some(&[said(GA, "Gwen Arzel", "producer"), said(MD, "Maëlle Dour", "instrument")]), false, crate::app::moment::now_secs())
        .unwrap();
    w.served.refresh().unwrap();
    assert_eq!(cat.record_artists(record).unwrap(), artists, "record_artists unchanged");
    assert_eq!(songs(&w), song_artists, "the songs' artists unchanged");
    assert!(!songs(&w).iter().flatten().any(|n| n == "Gwen Arzel" || n == "Maëlle Dour"), "no role credit on a song");
    assert_eq!(cat.artist_of("Gwen Arzel").unwrap(), None, "no artist made");
}

/// CRED.20, a guard: nothing is written into files. ALL with credits its files lack (mastering, artwork, the violin): root's panel
/// offers no action for a credit, and the record's Health names no credit the files lack; the files' bytes unchanged. Never a tag
/// written, a plan kind for credits, a files line or a Write for a credit. Today: green (nothing writes credits).
#[tokio::test]
async fn credits_write_nothing_in_018() {
    let _state = crate::app::references::TestState::set().await;
    let w = Whole::open("cred-20").await;
    let _instance = Instance::set(&w);
    let a = accounts(&w);
    let (all, paths) = plant_all(&w);
    let bytes: Vec<Vec<u8>> = paths.iter().map(|p| std::fs::read(p).unwrap()).collect();
    let record = crate::serve::ids::record_number(&all).unwrap();
    let cat = w.app.library.catalogue();
    let now = crate::app::moment::now_secs();
    cat.keep_credits(record, "musicbrainz", &format!("release:{REL_ALL}"), Some(&[said(NF, "Nils Fréjus", "mastering"), said(MD, "Maëlle Dour", "instrument")]), false, now).unwrap();
    cat.keep_credits(record, "discogs", &format!("release:{R401}"), Some(&[said(&DC_LK.to_string(), "Lou Kerjean", "Artwork")]), false, now).unwrap();
    w.served.refresh().unwrap();
    let answer = ask(&w, ("cookie", &a.root), Method::GET, &format!("/api/records/{all}?from=library"), Value::Null).await.1;
    let actions = answer["actions"].to_string().to_lowercase();
    assert!(!actions.contains("credit"), "no action offered for a credit: {actions}");
    let (_, health) = ask(&w, ("cookie", &a.root), Method::GET, &format!("/api/albums/cratarr/{all}/health"), Value::Null).await;
    assert!(!health.to_string().to_lowercase().contains("credit"), "no Health item for credits the files lack: {health}");
    for (p, b) in paths.iter().zip(&bytes) {
        assert_eq!(&std::fs::read(p).unwrap(), b, "{} unchanged", p.display());
    }
}

/// "Tide Lines"' release group, its MusicBrainz release, and its people's ids (SCENARIOS-CRED's fixture).
const RG_TL: &str = "c0000000-0000-4000-8000-0000000b0431";
const REL_TL: &str = "c0000000-0000-4000-8000-000000000431";
const SV_TL: &str = "c0000000-0000-4000-8000-0000000a0431";
const JP: &str = "c0000000-0000-4000-8000-0000000a0432";
const MH: &str = "c0000000-0000-4000-8000-0000000a0433";

/// Tide Lines as its sources answer: MusicBrainz's release (producer Sanne Vos, double bass Jonah Pike on 1–3, Mira Holt the
/// composer of every work), its group browsed to it; Discogs' master 431, its main release 4310 (A1–A3, B1–B2) and its credits.
fn tide_lines_answers(a: &Asked) -> Option<(u16, String)> {
    let five = (1..=5u32).map(|t| format!("Line {t}")).collect::<Vec<_>>();
    match a.source {
        "musicbrainz" if a.path == "/ws/2/release" => Some((200, json!({ "releases": [{ "id": REL_TL, "status": "Official", "media": [{ "track-count": 5 }] }] }).to_string())),
        "musicbrainz" if a.path == format!("/ws/2/release/{REL_TL}") => {
            let tracks: Vec<Value> = five
                .iter()
                .enumerate()
                .map(|(i, title)| {
                    let mut relations = vec![json!({ "type": "performance", "target-type": "work", "work": { "id": format!("w-tl-{i}"), "title": title, "relations": [rel("composer", MH, "Mira Holt", &[])] } })];
                    if i < 3 {
                        relations.push(rel("instrument", JP, "Jonah Pike", &["double bass"]));
                    }
                    json!({ "id": format!("t-tl-{i}"), "position": i + 1, "title": title, "length": 200_000, "artist-credit": [credit("rf-tl", "Rowan Fell", "")],
                        "recording": { "id": format!("r-tl-{i}"), "relations": relations } })
                })
                .collect();
            Some((200, json!({ "id": REL_TL, "title": "Tide Lines", "artist-credit": [credit("rf-tl", "Rowan Fell", "")], "relations": [rel("producer", SV_TL, "Sanne Vos", &[])],
                "release-group": { "id": RG_TL }, "media": [{ "position": 1, "tracks": tracks }] }).to_string()))
        }
        "discogs" if a.path == "/masters/431" => Some((200, json!({ "id": 431, "main_release": 4310, "genres": ["Jazz"], "styles": [] }).to_string())),
        "discogs" if a.path == "/releases/4310" => {
            let positions = ["A1", "A2", "A3", "B1", "B2"];
            let tracklist: Vec<Value> = positions.iter().zip(&five).map(|(p, t)| json!({ "position": p, "type_": "track", "title": t, "duration": "3:20" })).collect();
            Some((200, json!({ "id": 4310, "master_id": 431, "artists": [dc_artist(4301, "Rowan Fell", "", "", "")], "tracklist": tracklist,
                "extraartists": [
                    dc_artist(9431, "Sanne Vos", "Producer, Mixed By", "", ""),
                    dc_artist(9432, "Jonah Pike", "Double Bass, Vocals", "A1 to A3", ""),
                    dc_artist(9433, "Mandy Parnell (2)", "Engineer [Mastering]", "", ""),
                    dc_artist(9434, "Mira Holt", "Instruments [Synthesizer, Drum Machine]", "B2", "")
                ] }).to_string()))
        }
        _ => Some((404, String::new())),
    }
}

/// CRED.6 (b), CRED.5 (c), T-CRED-8: a record not had keeps the credits of the answers its open's jobs read anyway. alice opens
/// "Tide Lines" (not had, by its release group and Discogs master `dc:M431`); before the jobs land the panel holds no credit and its
/// GET asked nothing. The open's Discogs job reads `dc:R431` and keeps its credits: Sanne Vos producer and mixer, Jonah Pike double
/// bass and vocals on 1–3, Mandy Parnell (2) mastering (its mark kept), Mira Holt synthesizer and drum machine on B2 (5), each Listed
/// by its Discogs id. The reader the open's record read is handed (`read_reference_kept`) keeps a release's credits too, for the
/// record its reference names. No artist made for any of them. Never the credits read by a GET. (CRED.5 (b)'s question and parse are
/// `a_release_read_asks_the_works_question_and_carries_every_credit`, records: this harness's MusicBrainz web service is no
/// stand-in's, and its page read waits 1 s, less than MusicBrainz's spacing between a release group's two questions.) Red first:
/// a not-had record's Discogs read kept only the release's main artists (rec/references.rs:414-425), and no reader kept credits.
#[tokio::test]
async fn a_record_not_had_keeps_its_answers_credits() {
    let _state = crate::app::references::TestState::set().await;
    let w = Whole::open("cred-6b").await;
    let _instance = Instance::set(&w);
    let a = accounts(&w);
    discogs_key(&w);
    let stand_ins = StandIns::set(Arc::new(tide_lines_answers)).await;
    stand_ins.hold("musicbrainz");
    stand_ins.hold("discogs");
    let open = json!({ "release_group": RG_TL, "discogs_master": 431, "title": "Tide Lines", "artist": "Rowan Fell" });
    let (status, opened) = ask(&w, ("cookie", &a.alice), Method::POST, "/api/records/open", open).await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    let record = opened["id"].as_str().or(opened["record"].as_str()).map(str::to_string).unwrap_or_else(|| panic!("the record opened: {opened}"));
    let path = format!("/api/records/{record}?from=search");
    // The open's jobs' first questions, held where they are asked (this harness runs no route in a scope, so an ask's scope can't
    // say whose it is): whatever is asked after them, while they are held, is the GET's.
    until("the open's first questions", || stand_ins.count("musicbrainz") >= 1 && stand_ins.count("discogs") >= 1).await;
    let before = stand_ins.asked().len();
    let (_, answer) = ask(&w, ("cookie", &a.alice), Method::GET, &path, Value::Null).await;
    assert_eq!(answer["properties"]["credits"], json!([]), "(c) before the jobs: no credit");
    let after = stand_ins.asked();
    assert_eq!(after.len(), before, "(c) the GET asked nothing: {:?}", &after[before..]);
    stand_ins.let_go();
    let (cat, n) = (w.app.library.catalogue(), crate::serve::ids::record_number(&record).expect("a record's id"));
    let discogs_kept = || cat.credits_kept(n).unwrap_or_default().iter().any(|k| k.source == "discogs");
    until("the open's jobs", || idle() && discogs_kept()).await;
    let (_, answer) = ask(&w, ("cookie", &a.alice), Method::GET, &path, Value::Null).await;
    let persons = |role: &str| people(&answer, role).into_iter().map(|p| p.0).collect::<Vec<_>>();
    let dc = |id: &str| json!({ "listed": { "source": "discogs", "id": id } });
    assert_eq!(persons("producer"), [dc("9431")], "Sanne Vos producer: {}", answer["properties"]["credits"]);
    assert_eq!(persons("mixer"), [dc("9431")], "and mixer");
    assert_eq!(persons("mastering"), [dc("9433")]);
    assert_eq!(people(&answer, "mastering")[0].1, "Mandy Parnell (2)", "its namesake mark kept");
    let performed: Vec<(String, Value, Option<Vec<u64>>)> = answer["properties"]["credits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["role"] == "performer")
        .flat_map(|r| r["people"].as_array().cloned().unwrap_or_default())
        .map(|p| (p["detail"].as_str().unwrap_or_default().to_string(), p["person"].clone(), p["tracks"].as_array().map(|t| t.iter().filter_map(Value::as_u64).collect())))
        .collect();
    for (detail, who, tracks) in [("double bass", "9432", vec![1, 2, 3]), ("vocals", "9432", vec![1, 2, 3]), ("synthesizer", "9434", vec![5]), ("drum machine", "9434", vec![5])] {
        assert!(performed.contains(&(detail.to_string(), dc(who), Some(tracks.clone()))), "{detail} by {who} on {tracks:?}: {performed:?}");
    }
    // The record read's reader keeps a release's credits for the record its reference names (T-CRED-8), whoever reads it.
    cat.keep_credits(n, "discogs", "release:4310", None, false, crate::app::moment::now_secs()).unwrap();
    assert!(!discogs_kept(), "forgotten, to be read again");
    let reader = crate::app::references::read_reference_kept(Some(w.served.clone()));
    reader(Ref::new("discogs", "master", "431")).await.expect("Discogs answers").expect("Tide Lines' list");
    assert!(discogs_kept(), "kept again by the reader");
    for name in ["Sanne Vos", "Jonah Pike", "Mira Holt", "Mandy Parnell"] {
        assert_eq!(cat.artist_of(name).unwrap(), None, "no artist made for {name}");
    }
}

/// Files for a record of `n` tracks titled "{word} 1"… by `artist`, no genre, no reference: (its path in Music, its tags).
fn plain_record(artist: &str, title: &str, word: &str, n: u32) -> Vec<(String, Vec<(String, String)>)> {
    (1..=n)
        .map(|t| {
            let tags = [("album", title.to_string()), ("albumartist", artist.into()), ("artist", artist.into()), ("title", format!("{word} {t}")), ("tracknumber", t.to_string())];
            (format!("{artist}/{title}/{t:02} {word} {t}.flac"), tags.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
        })
        .collect()
}

/// A Discogs release of `n` tracks titled "{word} 1"…, 3:20 each, with one extra artist.
fn release_of_n(id: u64, master: u64, word: &str, n: u32, extra: Value) -> String {
    let tracklist: Vec<Value> = (1..=n).map(|t| json!({ "position": format!("{t}"), "type_": "track", "title": format!("{word} {t}"), "duration": "3:20" })).collect();
    json!({ "id": id, "master_id": master, "tracklist": tracklist, "genres": ["Jazz"], "styles": ["Cool Jazz"], "extraartists": [extra] }).to_string()
}

/// CRED.6 (d), CRED.11 (g), T-CRED-28: the one scheduled Discogs job reads the records that show no genre, each once while fresh,
/// by Discogs' search for its names where it has no link; a name-found master is a reference only where today's pairing confirms it
/// whole (the owner's 02:32 rule, HS01). "Still Water" (7 files): its master's list pairs whole, so it is kept as the record's
/// reference and "Producer – Ines Valk" shows. "Quiet Fields" (8 files): its master's list of 6 doesn't pair: a guess, no reference
/// kept, its credits kept unshown, none shown on Quiet Fields. Run again: nothing asked (one due marker). Never a name-found master's
/// credits shown before the pairing confirms it; never one kept as a reference. Red first: `genres_pass` and `types_pass` asked by
/// name each with their own marker, keeping no credits; `fill` kept what it found as references with `how` "search".
#[tokio::test]
async fn the_one_scheduled_discogs_job_keeps_a_guess_unshown() {
    let _state = crate::app::references::TestState::set().await;
    let w = Whole::open("cred-11g").await;
    let _instance = Instance::set(&w);
    let a = accounts(&w);
    discogs_key(&w);
    let stand_ins = StandIns::set(Arc::new(|q: &Asked| {
        Some(match (q.source, q.path.as_str()) {
            ("discogs", "/database/search") if q.query.contains("Quiet") => (200, json!({ "results": [{ "id": 441, "title": "Quiet Pine - Quiet Fields" }] }).to_string()),
            ("discogs", "/database/search") if q.query.contains("Still") => (200, json!({ "results": [{ "id": 442, "title": "Still Pine - Still Water" }] }).to_string()),
            ("discogs", "/masters/441") => (200, json!({ "id": 441, "main_release": 4410, "genres": ["Jazz"] }).to_string()),
            ("discogs", "/masters/442") => (200, json!({ "id": 442, "main_release": 4420, "genres": ["Jazz"] }).to_string()),
            ("discogs", "/releases/4410") => (200, release_of_n(4410, 441, "Field", 6, dc_artist(9441, "Tom Lerat", "Mixed By", "", ""))),
            ("discogs", "/releases/4420") => (200, release_of_n(4420, 442, "Water", 7, dc_artist(9442, "Ines Valk", "Producer", "", ""))),
            _ => (404, String::new()),
        })
    }))
    .await;
    let mut files = plain_record("Quiet Pine", "Quiet Fields", "Field", 8);
    files.extend(plain_record("Still Pine", "Still Water", "Water", 7));
    let (all, _) = plant_all_with(&w, &files);
    let _ = all;
    let quiet = w.served.library.album_in(0, "Quiet Pine/Quiet Fields").unwrap().expect("Quiet Fields");
    let still = w.served.library.album_in(0, "Still Pine/Still Water").unwrap().expect("Still Water");
    crate::app::discogs_facts::scheduled_pass(w.served.clone()).await.unwrap();
    let cat = w.app.library.catalogue();
    let discogs_refs = |album: &str| cat.references_of(album).unwrap().into_iter().filter(|(r, _)| r.source == "discogs").map(|(r, _)| r.value).collect::<Vec<_>>();
    assert_eq!(discogs_refs(&still), ["442", "4420"], "Still Water: paired whole, its master and release its references");
    assert!(discogs_refs(&quiet).is_empty(), "Quiet Fields: a guess, no reference kept");
    let q = crate::serve::ids::record_number(&quiet).unwrap();
    let kept = cat.credits_kept(q).unwrap();
    assert!(!kept.is_empty() && kept.iter().all(|k| k.guess), "Quiet Fields' credits kept unshown: {kept:?}");
    let quiet_answer = ask(&w, ("cookie", &a.alice), Method::GET, &format!("/api/records/{quiet}?from=library"), Value::Null).await.1;
    assert!(people(&quiet_answer, "mixer").is_empty(), "none shown on Quiet Fields: {}", quiet_answer["properties"]["credits"]);
    let still_answer = ask(&w, ("cookie", &a.alice), Method::GET, &format!("/api/records/{still}?from=library"), Value::Null).await.1;
    assert_eq!(people(&still_answer, "producer").into_iter().map(|p| p.1).collect::<Vec<_>>(), ["Ines Valk"], "Still Water's credit counts");
    let asked = stand_ins.count("discogs");
    crate::app::discogs_facts::scheduled_pass(w.served.clone()).await.unwrap();
    assert_eq!(stand_ins.count("discogs"), asked, "run again while fresh: nothing asked");
}

/// "Night Signals"' people (SCENARIOS-CRED's fixture: `mb:rf`, `mb:ob`).
const RF_NS: &str = "c0000000-0000-4000-8000-0000000a0495";
const OB: &str = "c0000000-0000-4000-8000-0000000a0496";

/// Night Signals as its files give it (6 files; the fixture's MP3s, tagged here as the catalogue keeps them): ALBUMARTIST Rowan
/// Fell; each track's ARTIST, with no ARTISTS list unless said: 1 "Rowan Fell feat. Oda Brenn"; 2 "Rowan Fell", titled "Harbour
/// (feat. Tamsin Kell)"; 3 "Rowan Fell / Oda Brenn"; 4 "Rowan Fell ft. Oda Brenn"; 5 "Rowan Fell & Oda Brenn" with ARTISTS both;
/// 6 "Rowan Fell; Oda Brenn".
fn night_signals() -> Vec<(String, Vec<(String, String)>)> {
    let lines = [
        ("Rowan Fell feat. Oda Brenn", "Signal 1", false),
        ("Rowan Fell", "Harbour (feat. Tamsin Kell)", false),
        ("Rowan Fell / Oda Brenn", "Signal 3", false),
        ("Rowan Fell ft. Oda Brenn", "Signal 4", false),
        ("Rowan Fell & Oda Brenn", "Signal 5", true),
        ("Rowan Fell; Oda Brenn", "Signal 6", false),
    ];
    lines
        .iter()
        .enumerate()
        .map(|(i, (artist, title, list))| {
            let t = i + 1;
            let mut tags: Vec<(String, String)> = [
                ("album", "Night Signals"),
                ("albumartist", "Rowan Fell"),
                ("musicbrainz_albumartistid", RF_NS),
                ("artist", artist),
                ("title", title),
                ("musicbrainz_artistid", RF_NS),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
            tags.push(("tracknumber".into(), t.to_string()));
            if t != 2 {
                tags.push(("musicbrainz_artistid".into(), OB.into()));
            }
            if *list {
                tags.extend([("artists".to_string(), "Rowan Fell".to_string()), ("artists".into(), "Oda Brenn".into())]);
            }
            (format!("Rowan Fell/Night Signals/{t:02} Signal {t}.mp3"), tags)
        })
        .collect()
}

/// CRED.26's Credits rows and line text (the coordinator's Q7: DC3c's half), CRED.25's track credits, CRED.28's stored title. Night
/// Signals' Credits section lists Oda Brenn under "Featuring" on tracks 1 and 4 (by her MusicBrainz id, from the files), Tamsin Kell
/// under "Featuring" on track 2 (from its title); never Oda Brenn featuring on 3, 5 or 6. The served track lines: 1 "Rowan Fell
/// feat. Oda Brenn", 2 "Rowan Fell feat. Tamsin Kell", 4 "Rowan Fell ft. Oda Brenn", 5 "Rowan Fell & Oda Brenn" (`credit_text` over
/// `files_list`: each entry's phrase). The stored and served title of track 2 is "Harbour (feat. Tamsin Kell)", whole, and the
/// served search finds it by "Tamsin"; the trim is `title_shown`'s, on screen alone. Never a featured artist under "artist"; never a
/// stored title cut. Tracks 3 and 6 (a separator in one value) are split by DC3a's `credit_parts` (CRED.19 (b), (c)): not asserted.
/// Red first: no featuring role existed, and the served line was `credit_text` over the names with one joining phrase.
#[tokio::test]
async fn night_signals_features_oda_brenn_on_1_and_4() {
    let _state = crate::app::references::TestState::set().await;
    let w = Whole::open("cred-26").await;
    let _instance = Instance::set(&w);
    let a = accounts(&w);
    plant_all_with(&w, &night_signals());
    let ns = w.served.library.album_in(0, "Rowan Fell/Night Signals").unwrap().expect("Night Signals built");
    let answer = ask(&w, ("cookie", &a.alice), Method::GET, &format!("/api/records/{ns}?from=library"), Value::Null).await.1;
    let featuring: Vec<(String, Value, Option<Vec<u64>>)> = answer["properties"]["credits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["role"] == "featuring")
        .flat_map(|r| r["people"].as_array().cloned().unwrap_or_default())
        .map(|p| (p["name"].as_str().unwrap_or_default().to_string(), p["person"].clone(), p["tracks"].as_array().map(|t| t.iter().filter_map(Value::as_u64).collect())))
        .collect();
    let oda: Vec<&(String, Value, Option<Vec<u64>>)> = featuring.iter().filter(|f| f.0 == "Oda Brenn").collect();
    assert_eq!(oda.len(), 1, "one Oda Brenn featuring: {}", answer["properties"]["credits"]);
    assert_eq!(oda[0].1, json!({ "listed": { "source": "musicbrainz", "id": OB } }), "by her MusicBrainz id");
    assert_eq!(oda[0].2, Some(vec![1, 4]), "on 1 and 4, never 3, 5 or 6");
    let tamsin: Vec<&(String, Value, Option<Vec<u64>>)> = featuring.iter().filter(|f| f.0 == "Tamsin Kell").collect();
    assert_eq!(tamsin.iter().map(|t| t.2.clone()).collect::<Vec<_>>(), [Some(vec![2])], "Tamsin Kell on 2, from its title");
    assert!(people(&answer, "artist").iter().all(|p| p.1 != "Tamsin Kell"), "a title's guest never under \"artist\"");
    let songs = w.served.library.songs_of_albums(std::slice::from_ref(&ns)).unwrap();
    for (track, line) in [(1, "Rowan Fell feat. Oda Brenn"), (2, "Rowan Fell feat. Tamsin Kell"), (4, "Rowan Fell ft. Oda Brenn"), (5, "Rowan Fell & Oda Brenn")] {
        let s = songs.iter().find(|s| s.track == Some(track)).unwrap_or_else(|| panic!("track {track}"));
        assert_eq!(s.artist, line, "track {track}'s line");
    }
    // CRED.28's guard: stored and served whole, searched by every word.
    let harbour = songs.iter().find(|s| s.track == Some(2)).unwrap();
    assert_eq!(harbour.title, "Harbour (feat. Tamsin Kell)", "the served title whole");
    let stored: Vec<String> = w.app.library.catalogue().album_tags(0, "Rowan Fell/Night Signals").unwrap().into_iter().flat_map(|(_, tags)| tags).filter(|(k, v)| k == "title" && v.starts_with("Harbour")).map(|(_, v)| v).collect();
    assert_eq!(stored, ["Harbour (feat. Tamsin Kell)"], "the stored title whole");
    let (_, _, found) = w.served.library.search("Tamsin", (0, 0), (0, 0), (20, 0), None).unwrap();
    assert!(found.iter().any(|s| s.title == "Harbour (feat. Tamsin Kell)"), "search finds it by its guest: {:?}", found.iter().map(|s| &s.title).collect::<Vec<_>>());
}
