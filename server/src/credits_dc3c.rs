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
    let cat = w.app.library.catalogue();
    let mut files = Vec::new();
    for t in 1..=12u32 {
        let rel = format!("Yann Tiersen/ALL/{t:02} Track {t}.flac");
        let path = w.dir().join(&rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &rel).unwrap();
        files.push((rel, path, t));
    }
    cat.walk_all();
    cat.pretend_read_for_tests();
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
    crate::serve::set_instance(w.served.clone());
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
        // The Listed persons' open: drawn for every account but the reading key (CRED.12; CN1's H1).
        for r in p["credits"].as_array().into_iter().flatten() {
            for person in r["people"].as_array().into_iter().flatten().filter(|x| x["person"]["listed"].is_object()) {
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
