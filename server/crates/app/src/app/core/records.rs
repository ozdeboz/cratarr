//! The record as one object, read (SCENARIOS-DC1 RV1-RV4, HD13, HD14, CN4, CN6, LV2, RS4, RS5; OBJECT-MAP-2 §2.1): what the
//! stores hold of a record, read once, for the domain's entity, card, row and chip (`cratarr_domain::record`) to work out.
//!
//! - **The rev first** (LV2): the change log's newest rev is read before any of the record's rows, so a change after the read
//!   always carries a higher rev, and the page reads again.
//! - **One read** (HD14): the record, its credits and labels, its copies by the present rule (the holding's), their quality and
//!   its Bandcamp page come from one reader transaction (`Catalogue::record_read`): a version line never names a copy the holding
//!   counts gone.
//! - **A merged record answers as the one it went into** (RV3); a forgotten or never made one is none (RV4).
//! - **The copy that plays** is `copy_for`'s, over the place's set: the one rule the library plays by.

use std::collections::HashMap;

use anyhow::Result;
use cratarr_domain::arrival::Arrival;
use cratarr_domain::can::{ArrivalFacts, Viewer};
use cratarr_domain::holding::{Fetcher, Library};
use cratarr_domain::record::{CopyOf, EditionOf, LibraryOf, RecordFacts, Seen, entity};
use cratarr_domain::view::{Entity, Named, RecordProperties};
use cratarr_domain::when::{Place, library_set};
use cratarr_domain::words::quality;

use crate::serve::Serve;
use crate::serve::copies::{Copy, Kind, copy_for};

/// What a record's view is read with, beside the record: the libraries as the viewer names them, each library's kind in the
/// libraries' order, the conversions at work (by catalogue folder), and who asked for an arrival (`requests::requesters`).
#[derive(Debug, Clone, Default)]
pub struct Around {
    pub libraries: Vec<LibraryOf>,
    pub kinds: Vec<(i64, Kind)>,
    pub jobs: HashMap<i64, u64>,
    pub asked: HashMap<i64, crate::serve::requests_kept::Requester>,
    /// The open requests of the record: each asker and the request's number.
    pub requests: Vec<(String, i64)>,
    pub editions: Vec<EditionOf>,
}

/// A record's view, read: the rev it was read at and its facts. None: no such record, or one forgotten (RV4).
pub fn read(serve: &Serve, id: i64, around: &Around, viewer: &Viewer, place: &Place, off: &[i64]) -> Result<Option<(i64, RecordFacts)>> {
    let rev = serve.userdata.change_rev()?;
    let library_of = |folder: i64| around.libraries.iter().find(|l| l.folder == folder).map_or(Library::Main, |l| l.library.clone());
    let Some(read) = serve.catalogue().record_read(id, &library_of)? else { return Ok(None) };
    let row = &read.row;
    let shortest = read.copies.shortest;
    // When each copy came, as the library first saw it (the Library list's `added`, which a request's say over an arrival goes by,
    // R32): the built library's, by its catalogue folder; else the earliest file time.
    let mut came: HashMap<i64, i64> = HashMap::new();
    for c in serve.library.copies_of(&crate::serve::ids::record(row.id)).unwrap_or_default() {
        if let Some(added) = c.added {
            let first = came.entry(c.folder).or_insert(added);
            *first = (*first).min(added);
        }
    }
    let copies: Vec<CopyOf> = read
        .copies
        .copies
        .iter()
        .map(|facts| {
            let q = read.quality.get(&facts.folder).cloned().unwrap_or_default();
            let added = came.get(&facts.folder).copied().or(q.added);
            let arrival = matches!(facts.library, Library::Incoming { .. }).then(|| ArrivalFacts {
                state: Arrival::Waiting,
                asked_by: around.asked.get(&row.id).filter(|r| r.of_copy(added)).map(|r| vec![r.user.clone()]).unwrap_or_default(),
                by: None,
            });
            CopyOf {
                facts: facts.clone(),
                quality: (facts.present > 0).then(|| quality(q.format.as_deref(), q.min_kbps)),
                added: added.unwrap_or(0),
                job: around.jobs.get(&facts.folder).copied(),
                arrival,
            }
        })
        .collect();
    // The copy that plays here: `copy_for` over the present copies in the place's set, whoever asks (a viewer).
    let on = library_set(place, off);
    let choosing: Vec<Copy> = copies
        .iter()
        .filter(|c| c.facts.present > 0)
        .map(|c| {
            let q = read.quality.get(&c.facts.folder).cloned().unwrap_or_default();
            let (kind, order) = crate::serve::library::kind_in(&around.kinds, c.facts.folder);
            Copy {
                folder: c.facts.folder,
                dir: String::new(),
                kind,
                order,
                format: q.format,
                min_kbps: q.min_kbps,
                bits: None,
                rate: None,
                short: shortest.is_some_and(|of| c.facts.paired < of),
                added: q.added,
            }
        })
        .collect();
    let plays = copy_for(&choosing, Some(&crate::serve::copies::Viewer::default()), &on.without).map(|c| c.folder);
    // What Replace in may put in a main library's place (RS4, RS7; the Library's rules, `copies::replace_offers` and `against`):
    // someone's own better copy (RY01), and an arrival better than the copy that plays (RS13's `against`), by folder.
    let mut replacing: Vec<(i64, i64)> = crate::serve::copies::replace_offers(&choosing, &on.without).into_iter().map(|(own, main)| (own.folder, main.folder)).collect();
    for arrival in choosing.iter().filter(|c| c.kind == Kind::Incoming) {
        if let Some((true, row)) = crate::serve::copies::against(&choosing, arrival)
            && row.kind == Kind::Main
        {
            replacing.push((arrival.folder, row.folder));
        }
    }
    let facts = RecordFacts {
        id: row.id,
        title: row.title.clone(),
        kind: row.kind.clone(),
        secondary: Vec::new(),
        year: row.year,
        artists: read.artists.iter().map(|(id, name)| Named { id: crate::serve::ids::artist(*id), name: name.clone(), catno: None }).collect(),
        labels: labels_of(&read.labels),
        genres: row.genres.clone(),
        cover: row.cover.clone(),
        hand: Vec::new(),
        copies,
        shortest,
        only_guesses: read.copies.only_guesses,
        picked: read.copies.picked,
        lists: read.lists.clone(),
        pick: read.pick.clone(),
        plays,
        // The preview's album as the page's Bandcamp album view asks it: its `bc.` id (PAGE3-REPORT), never the page's address.
        preview: read.bandcamp.as_deref().map(|page| crate::bandcamp::track_id(page, 0)),
        requests: around.requests.iter().map(|(asker, n)| (asker.clone(), n.to_string())).collect(),
        replacing,
        editions: around.editions.clone(),
        libraries: around.libraries.clone(),
    };
    let _ = viewer;
    Ok(Some((rev, facts)))
}

/// A record's labels as its view names them (A2.5): one a label, its catalogue numbers joined.
fn labels_of(labels: &[(i64, String, String)]) -> Vec<Named> {
    let mut out: Vec<Named> = Vec::new();
    for (id, name, catno) in labels {
        let id = crate::serve::ids::label(*id);
        match out.iter_mut().find(|l| l.id == id) {
            Some(shown) if !catno.is_empty() => {
                let joined = shown.catno.take().map_or_else(|| catno.clone(), |c| format!("{c}, {catno}"));
                shown.catno = Some(joined);
            }
            Some(_) => {}
            None => out.push(Named { id, name: name.clone(), catno: Some(catno.clone()).filter(|c| !c.is_empty()) }),
        }
    }
    out
}

/// The record's entity for this viewer here (RV1): read, then worked out. None: no such record (RV4).
pub fn record_entity(serve: &Serve, id: i64, around: &Around, viewer: &Viewer, place: &Place, off: &[i64]) -> Result<Option<Entity<RecordProperties>>> {
    let Some((rev, facts)) = read(serve, id, around, viewer, place, off)? else { return Ok(None) };
    Ok(Some(entity(&facts, &Seen { viewer, place, off, rev })))
}

/// Each library as the viewer names it, by its catalogue folder (`Serve::folder_of_library`): their own music "Your music",
/// another's by its name, the holding's kind of each (an Incoming one with its fetcher).
pub fn libraries_of(serve: &Serve, used: &[(String, String, crate::settings::LibraryKind, Option<String>, Option<String>)], viewer: &str) -> Vec<LibraryOf> {
    used.iter()
        .filter_map(|(id, name, kind, owner, fetcher)| {
            let folder = serve.folder_of_library(id)?;
            let mine = owner.as_deref() == Some(viewer) && !viewer.is_empty();
            let library = match kind {
                crate::settings::LibraryKind::Main => Library::Main,
                crate::settings::LibraryKind::Incoming => Library::Incoming {
                    fetcher: match fetcher.as_deref() {
                        Some("soulseek") => Fetcher::Soulseek,
                        Some("uploads") => Fetcher::Uploads,
                        _ => Fetcher::Lidarr,
                    },
                    uploader: None,
                },
                crate::settings::LibraryKind::Own => Library::Own(owner.clone().unwrap_or_default()),
            };
            let name = if mine { crate::settings::YOUR_MUSIC.to_string() } else { name.clone() };
            Some(LibraryOf { folder, id: id.clone(), name, library })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::Rules;
    use cratarr_domain::can::May;
    use cratarr_domain::view::{Is, Section};

    const MUSIC: i64 = 0;
    const IN: i64 = 1;
    const OWN_A: i64 = 2;
    const MORE: i64 = 4;
    const ALL: May = May { request: true, offline: true, playlists: true };

    fn root() -> Viewer {
        Viewer::new("root", true, None, May::default(), None)
    }

    fn alice() -> Viewer {
        Viewer::new("alice", false, None, ALL, Some(OWN_A))
    }

    /// The fixture's libraries: MUSIC and MORE main, IN Lidarr's Incoming, OWN-A alice's own music.
    fn around(viewer: &str) -> Around {
        let lib = |folder: i64, id: &str, name: &str, library: Library| LibraryOf { folder, id: id.into(), name: name.into(), library };
        let own_name = if viewer == "alice" { "Your music" } else { "alice's music" };
        Around {
            libraries: vec![
                lib(MUSIC, "/music/MUSIC", "MUSIC", Library::Main),
                lib(IN, "/music/IN", "IN", Library::Incoming { fetcher: Fetcher::Lidarr, uploader: None }),
                lib(OWN_A, "/music/own/alice", own_name, Library::Own("alice".into())),
                lib(MORE, "/music/MORE", "MORE", Library::Main),
            ],
            kinds: vec![(MUSIC, Kind::Main), (IN, Kind::Incoming), (OWN_A, Kind::Own), (MORE, Kind::Main)],
            ..Around::default()
        }
    }

    /// Night Ferry `al-<n>`: ten tracks, a copy in each folder named, the files' format and bitrate as given.
    fn night_ferry(serve: &Serve, copies: &[(i64, &str, i64)]) -> i64 {
        let cat = serve.catalogue();
        let record = cat.record_for_tests("Night Ferry", 10);
        for (folder, format, kbps) in copies {
            cat.copy_for_tests(record, *folder);
            cat.db_for_tests()
                .execute("UPDATE files SET format = ?2, bitrate = ?3 WHERE folder = ?1 AND rel LIKE 'copy-' || ?4 || '/%'", rusqlite::params![folder, format, kbps, record])
                .unwrap();
        }
        record
    }

    fn served() -> std::sync::Arc<Serve> {
        let serve = Serve::open(None, Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(std::sync::Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap()));
        serve
    }

    fn answer(serve: &Serve, id: i64, viewer: &Viewer, place: &Place, off: &[i64]) -> Entity<RecordProperties> {
        record_entity(serve, id, &around(&viewer.account), viewer, place, off).unwrap().expect("the record answers")
    }

    fn names(e: &Entity<RecordProperties>) -> Vec<String> {
        e.actions.iter().map(|a| a.name.clone()).collect()
    }

    /// RV1: one answer for a record: its class, id, the rev it was read at, its epoch, its properties, its state, its actions with
    /// their addresses, its sections in the Library's order, its two versions, and where it goes. Never a field of the old shapes
    /// (`not_had`, `have`, `album`, `subject`, `record`). Red first: no record entity (the record's answer is `AlbumView`'s, its
    /// state worked out on the page).
    #[test]
    fn a_record_is_one_entity() {
        let serve = served();
        let id = night_ferry(&serve, &[(MUSIC, "MP3", 320), (IN, "FLAC", 900)]);
        let before = serve.userdata.change_rev().unwrap();
        let e = answer(&serve, id, &alice(), &Place::Library, &[]);
        assert_eq!((e.class.as_str(), e.id.as_str()), ("record", format!("al-{id}").as_str()));
        assert_eq!(e.rev, before, "the change log's newest rev, read before the rows");
        assert!(!e.epoch.is_empty());
        assert_eq!((e.state.is, e.state.words.as_str()), (Is::Had, "In your library"));
        assert_eq!(e.properties.title.value, "Night Ferry");
        assert_eq!(e.sections.first(), Some(&Section::Head));
        assert_eq!(e.sections.get(1), Some(&Section::Play), "the Library's: the Play row right after the head");
        assert_eq!(e.entities.len(), 2, "MUSIC's copy and IN's: two versions");
        assert!(e.actions.iter().all(|a| a.href.starts_with("/api/")), "each action's address");
        let json = serde_json::to_value(&e).unwrap();
        let keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
        for key in ["class", "id", "rev", "epoch", "properties", "state", "actions", "sections", "entities", "links"] {
            assert!(keys.contains(&key), "{key}");
        }
        let text = json.to_string();
        for never in ["\"not_had\"", "\"have\"", "\"album\":", "\"subject\"", "\"record\":{"] {
            assert!(!text.contains(never), "{never} in {text}");
        }
    }

    /// RV2: a record not had has the same shape, its state saying what it is; no action that needs a copy.
    #[test]
    fn a_record_not_had_is_the_same_entity_with_its_own_state() {
        let serve = served();
        let had = night_ferry(&serve, &[(MUSIC, "FLAC", 900)]);
        let answered = serve.catalogue().record_for_tests("Answered", 0);
        let (a, b) = (answer(&serve, had, &alice(), &Place::Search, &[]), answer(&serve, answered, &alice(), &Place::Search, &[]));
        let keys = |e: &Entity<RecordProperties>| serde_json::to_value(&e.properties).unwrap().as_object().unwrap().keys().cloned().collect::<Vec<_>>();
        assert_eq!(keys(&a), keys(&b), "the same keys in both");
        assert_eq!((b.state.is, b.state.words.as_str()), (Is::NotHad, "Not in your library"));
        assert!(b.entities.is_empty());
        let offered = names(&b);
        assert!(offered.contains(&"want".to_string()));
        for never in ["favourite", "rating", "download", "sync", "delete", "edit", "play"] {
            assert!(!offered.iter().any(|n| n == never), "{never} on a record not had, no preview: {offered:?}");
        }
    }

    /// RV3: a merged record answers as the one it went into; its actions act on that one.
    #[test]
    fn a_merged_record_answers_as_the_one_it_went_into() {
        let serve = served();
        let into = night_ferry(&serve, &[(MUSIC, "FLAC", 900)]);
        let merged = serve.catalogue().record_for_tests("Night Ferry", 0);
        serve.catalogue().db_for_tests().execute("UPDATE records SET merged_into = ?2 WHERE id = ?1", [merged, into]).unwrap();
        let e = answer(&serve, merged, &alice(), &Place::Library, &[]);
        assert_eq!(e.id, format!("al-{into}"));
        assert!(e.actions.iter().all(|a| !a.href.contains(&format!("al-{merged}"))), "no address names the merged id");
        assert!(record_entity(&serve, 999_999, &around("alice"), &alice(), &Place::Library, &[]).unwrap().is_none(), "never made: none");
    }

    /// HD13: a copy goes while it is shown: the build marks every file gone, the log gets the record at a new rev, and its sheet
    /// says "Was in MUSIC until …", with no Play row and no Library section.
    #[test]
    fn a_copy_gone_logs_its_record() {
        let serve = served();
        let id = night_ferry(&serve, &[(MUSIC, "FLAC", 900)]);
        serve.refresh().unwrap();
        let shown = answer(&serve, id, &root(), &Place::Library, &[]);
        assert!(names(&shown).contains(&"play".to_string()));
        serve.catalogue().db_for_tests().execute("UPDATE files SET gone_at = 1000", []).unwrap();
        serve.refresh().unwrap();
        let after = answer(&serve, id, &root(), &Place::Library, &[]);
        assert!(after.rev > shown.rev, "logged at a new rev: {} then {}", shown.rev, after.rev);
        assert_eq!((after.state.is, after.state.words.as_str(), after.state.at), (Is::Retired, "Was in MUSIC until", Some(1000)));
        assert!(!names(&after).contains(&"play".to_string()), "never Play on a copy gone");
        assert!(!after.sections.contains(&Section::Library));
    }

    /// HD14: one read for one view: the holding, the versions and the Library section agree with themselves, a writer between the
    /// rev and the rows notwithstanding: the rev is read first, so the rows can only be newer than it says.
    #[test]
    fn a_view_reads_its_copies_once() {
        let serve = served();
        let id = night_ferry(&serve, &[(MUSIC, "FLAC", 900), (MORE, "MP3", 320)]);
        let e = answer(&serve, id, &root(), &Place::Library, &[]);
        assert_eq!(e.entities.len(), 2);
        // MORE's copy goes: the next read's holding, versions and Library section all lose it at once.
        serve.catalogue().db_for_tests().execute("UPDATE files SET gone_at = 1000 WHERE folder = ?1", [MORE]).unwrap();
        let after = answer(&serve, id, &root(), &Place::Library, &[]);
        assert!(after.entities.is_empty(), "one version left: no Versions section");
        assert!(after.actions.iter().all(|a| !a.title.contains("MORE") || a.title.starts_with("Move to MORE")), "no action of MORE's copy");
        assert_eq!(after.state.is, Is::Had);
    }

    /// CN4: the Play row is offered where a copy plays here, or a preview: with MUSIC off an Incoming copy does not play by itself
    /// (Play from IN in the Library section); with MUSIC and IN off, none; a record not had with a Bandcamp page, the preview's Play;
    /// without, none. The reading key plays too.
    #[test]
    fn the_play_row_is_offered_where_a_copy_plays_or_a_preview() {
        let serve = served();
        let id = night_ferry(&serve, &[(MUSIC, "MP3", 320), (IN, "FLAC", 900)]);
        let (alice, key) = (alice(), Viewer::new("root", true, Some(cratarr_domain::can::Scope::Read), May::default(), None));
        let all = answer(&serve, id, &alice, &Place::Library, &[]);
        assert!(names(&all).contains(&"play".to_string()));
        let music_off = answer(&serve, id, &alice, &Place::Library, &[MUSIC]);
        assert!(!names(&music_off).contains(&"play".to_string()), "an Incoming copy does not play by itself");
        assert!(music_off.actions.iter().any(|a| a.name == "play_from" && a.title == "Play from IN"), "{:?}", names(&music_off));
        let both_off = answer(&serve, id, &alice, &Place::Library, &[MUSIC, IN]);
        assert!(!names(&both_off).iter().any(|n| n == "play" || n == "play_from"));
        assert!(names(&answer(&serve, id, &key, &Place::Library, &[])).contains(&"play".to_string()), "the reading key plays");
        let answered = serve.catalogue().record_for_tests("Answered", 0);
        assert!(!names(&answer(&serve, answered, &alice, &Place::Search, &[])).contains(&"play".to_string()), "no preview, no Play");
        let page = "https://kitetheory.example.org/album/answered";
        serve.catalogue().set_reference(&format!("al-{answered}"), &crate::catalogue::identity::Ref::new("bandcamp", "album", page), "hand").unwrap();
        let previewed = answer(&serve, answered, &alice, &Place::Search, &[]);
        let play = previewed.actions.iter().find(|a| a.name == "play").expect("the preview's Play row");
        assert!(play.href.starts_with("/api/albums/bandcamp/"), "the preview's album view: {}", play.href);
    }

    /// LV2: a view's version is the rev it was read at: a copy deleted after the read is logged above it, so the cached view is
    /// stale and read again, MUSIC's alone then.
    #[test]
    fn a_view_is_versioned_by_the_log_it_was_read_at() {
        let serve = served();
        let id = night_ferry(&serve, &[(MUSIC, "FLAC", 900), (IN, "MP3", 320)]);
        serve.refresh().unwrap();
        let first = answer(&serve, id, &alice(), &Place::Library, &[]);
        assert_eq!(first.rev, serve.userdata.change_rev().unwrap());
        serve.catalogue().db_for_tests().execute("UPDATE files SET gone_at = 1000 WHERE folder = ?1", [IN]).unwrap();
        serve.refresh().unwrap();
        let logged = serve.userdata.change_rev().unwrap();
        assert!(logged > first.rev, "the delete is logged above the read: {logged} > {}", first.rev);
        let again = answer(&serve, id, &alice(), &Place::Library, &[]);
        assert!(again.rev >= logged);
        assert!(again.entities.is_empty(), "MUSIC's alone");
    }

    /// RS4: copies and editions are one Versions list, the copy that plays first and marked, the others to play from; a record of
    /// one version has none. RS5: the list follows the place's set: with MUSIC off from the Library, MUSIC's line goes and IN's is
    /// not marked; from a playlist the same (it follows the switch since 21:37).
    #[test]
    fn copies_and_editions_are_one_versions_list() {
        let serve = served();
        let id = night_ferry(&serve, &[(MUSIC, "MP3", 320), (IN, "FLAC", 900)]);
        let e = answer(&serve, id, &alice(), &Place::Library, &[]);
        let lines: Vec<(Option<String>, Option<String>, bool)> = e.entities.iter().map(|v| (v.library.clone(), v.quality.clone(), v.plays)).collect();
        assert_eq!(lines, [(Some("MUSIC".into()), Some("MP3 320".into()), true), (Some("IN".into()), Some("FLAC".into()), false)]);
        assert_eq!(e.entities.iter().filter(|v| v.plays).count(), 1, "one line plays");
        assert!(e.entities[0].action.is_none(), "the line that plays has no tap");
        let one = night_ferry(&serve, &[(MORE, "FLAC", 900)]);
        assert!(answer(&serve, one, &alice(), &Place::Library, &[]).entities.is_empty(), "one version: no section");
    }

    /// CHECK-DC1's ruling 6 (the cost): the copy reader reads every file's tags of the records it is asked (34k files, 670k tag rows on
    /// production, under 768 MB), so no list may call it once per row: each caller reads once for a whole list, or one record for one
    /// view, story or handler. Every caller is named here with its shape; a new one is added only with its own.
    #[test]
    fn the_copy_reader_is_read_once_per_list() {
        const READERS: [&str; 5] = ["catalogue().copies_of(", "cat.copies_of(", ".copies_held(", ".requested_held(", ".record_read("];
        // (file, calls): what each reads.
        let allowed: [(&str, usize, &str); 6] = [
            ("app/src/app/core/records.rs", 1, "one record: its view, or the handler deciding (`decided`)"),
            ("incoming/src/app/albums.rs", 1, "one record's copies judged for copying (`judged_copies`: VR28, by the holding's pairs)"),
            ("playing/src/app/device_index.rs", 1, "every record of the index at once, at its build"),
            ("requests/src/app/requests.rs", 2, "every open request's record at once (`records_held`); one request's story (`held_for`)"),
            ("requests/src/app/had_vectors.rs", 1, "the vectors' records at once (a test)"),
            ("store/src/catalogue/holding.rs", 0, "the readers themselves"),
        ];
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut found: Vec<(String, usize)> = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    // The readers themselves, and tests (a module's own, a file of them), read as they like.
                    let name = path.file_name().unwrap().to_string_lossy();
                    let code = if path.ends_with("store/src/catalogue/holding.rs") || name.contains("tests") {
                        ""
                    } else {
                        text.split("#[cfg(test)]\nmod tests").next().unwrap_or(&text)
                    };
                    let calls: usize = READERS.iter().map(|r| code.matches(r).count()).sum::<usize>();
                    if calls > 0 {
                        found.push((path.strip_prefix(&root).unwrap().to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"), calls));
                    }
                }
            }
        }
        for (file, calls) in &found {
            let named = allowed.iter().find(|(f, _, _)| f == file).unwrap_or_else(|| panic!("{file} reads a record's copies ({calls}): name it here, read once per list"));
            assert!(*calls <= named.1, "{file}: {calls} reads of a record's copies, {} named ({})", named.1, named.2);
        }
    }

    #[test]
    fn versions_follow_the_places_library_set() {
        let serve = served();
        let id = night_ferry(&serve, &[(MUSIC, "MP3", 320), (IN, "FLAC", 900), (MORE, "FLAC", 900)]);
        let off = answer(&serve, id, &alice(), &Place::Library, &[MUSIC]);
        let libraries: Vec<Option<String>> = off.entities.iter().map(|v| v.library.clone()).collect();
        assert_eq!(libraries, [Some("MORE".into()), Some("IN".into())], "MUSIC's gone; MORE plays, IN after it");
        assert!(off.entities.iter().all(|v| v.library.as_deref() != Some("IN") || !v.plays), "IN's never marked plays");
        let every = answer(&serve, id, &alice(), &Place::Work, &[MUSIC]);
        assert_eq!(every.entities.len(), 3, "a work counts every library");
    }
}
