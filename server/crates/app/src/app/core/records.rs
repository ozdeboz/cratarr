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
    // A had record's type is the one its copy was built with (the release type rule over its deciding copy, RF1), as the Library
    // list and the device's index serve it: one rule for the Head, the rows and the index (VR39); else the record's own.
    let album = crate::serve::ids::record(row.id);
    let built = if row.had { serve.record_types(std::slice::from_ref(&album)).remove(&album).and_then(|(kind, _)| kind) } else { None };
    // VR49: its secondaries' sources: the hand's list (final for its kind), each service's per reference (a guess's counting nowhere),
    // and the rule's, as the build heard them for a record you have.
    let set = serve.userdata.overrides_of(&album)?;
    let hand_secondary = set.secondary.clone();
    let heard = if row.had { serve.library.secondaries_heard(&album)? } else { None };
    let mut secondaries = secondaries_said(&read);
    if let Some(h) = &heard {
        let reference = h.mbid.as_deref().map(|m| cratarr_domain::facts::Reference::new("musicbrainz", "release", m));
        let guessed = reference.as_ref().is_some_and(|r| read.guesses.contains(r));
        secondaries.extend(h.musicbrainz.clone().map(|words| cratarr_domain::facts::Said { reference, source: "musicbrainz".into(), value: Some(words), guessed }));
        secondaries.extend(h.discogs.clone().map(|words| cratarr_domain::facts::Said { reference: None, source: "discogs".into(), value: Some(words), guessed: false }));
    }
    let facts = RecordFacts {
        id: row.id,
        // COU.13 (d): a title set by hand wins, with Reset.
        title: set.title.clone().unwrap_or_else(|| row.title.clone()),
        kind: built.or_else(|| row.kind.clone()),
        secondary: hand_secondary.clone().unwrap_or_default(),
        year: row.year,
        // NAMES.10: each credited artist by its id, named by its one served name (the spelling credited most beside its id), never
        // the record's files' string; its birth name only where the library serves it no rows.
        artists: {
            let ids: Vec<String> = read.artists.iter().map(|(id, _)| crate::serve::ids::artist(*id)).collect();
            let served = serve.library.names_of(&ids).unwrap_or_default();
            read.artists.iter().zip(ids).map(|((_, name), id)| Named { name: served.get(&id).cloned().unwrap_or_else(|| name.clone()), id, catno: None }).collect()
        },
        labels: labels_of(&read.labels),
        genres: row.genres.clone(),
        cover: row.cover.clone(),
        hand: hand_secondary.iter().map(|_| "secondary").chain(set.title.iter().map(|_| "title")).map(str::to_string).collect(),
        copies,
        shortest,
        only_guesses: read.copies.only_guesses,
        picked: read.copies.picked,
        lists: read.lists.clone(),
        pick: read.pick.clone(),
        years: said_of(&read, "year"),
        types: said_of(&read, "type"),
        secondaries,
        rule_secondary: heard.map(|h| h.rule).unwrap_or_default(),
        // The release type rule over the picked list's titles and lengths (VR39, M10): the one rule the build types an album by.
        picked_kind: read.pick.as_ref().and_then(|p| read.lists.iter().find(|l| &l.reference == p)).and_then(|l| {
            let lengths: Vec<Option<u64>> = l.tracks.iter().map(|t| t.length_ms.and_then(|ms| u64::try_from(ms).ok())).collect();
            crate::serve::library::type_by_files(&row.title, &lengths)
        }),
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

/// Each source's value of one of a record's facts, per reference (`kind:value`, none for a fact no reference gives), each a guess's
/// where its reference is one (VR8): what `resolve()` reads (VR39). A value that does not read as one is none.
fn said_of<T: std::str::FromStr>(read: &crate::catalogue::holding::RecordRead, fact: &str) -> Vec<cratarr_domain::facts::Said<T>> {
    read.said
        .iter()
        .filter(|s| s.fact == fact)
        .map(|s| {
            let reference = s.of.split_once(':').map(|(kind, value)| cratarr_domain::facts::Reference::new(&s.source, kind, value));
            let guessed = reference.as_ref().is_some_and(|r| read.guesses.contains(r));
            cratarr_domain::facts::Said { reference, source: s.source.clone(), value: s.value.as_deref().and_then(|v| v.parse().ok()), guessed }
        })
        .collect()
}

/// Each source's secondary types of a record kept in the facts layer (`secondary`, a JSON list of words), per reference, each a
/// guess's where its reference is one (VR49, VR8). A value that does not read as a list is none.
fn secondaries_said(read: &crate::catalogue::holding::RecordRead) -> Vec<cratarr_domain::facts::Said<Vec<String>>> {
    read.said
        .iter()
        .filter(|s| s.fact == "secondary")
        .map(|s| {
            let reference = s.of.split_once(':').map(|(kind, value)| cratarr_domain::facts::Reference::new(&s.source, kind, value));
            let guessed = reference.as_ref().is_some_and(|r| read.guesses.contains(r));
            let value = s.value.as_deref().and_then(|v| serde_json::from_str::<Vec<String>>(v).ok());
            cratarr_domain::facts::Said { reference, source: s.source.clone(), value, guessed }
        })
        .collect()
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

    /// NAMES.10 (Q1 (a)): the record's credits by id; its line and chips read each credited artist's one served name, the spelling
    /// credited most beside its id ("The Jimi Hendrix Experience" for `ar-202`, though the record's own files say "Jimi Hendrix"),
    /// joined by the one formatter; with no served row, its catalogue name. Red first: the line read the record's files' string and
    /// the catalogue's birth name.
    #[test]
    fn a_records_artist_line_reads_its_artists_names() {
        let serve = served();
        let id = night_ferry(&serve, &[(MUSIC, "FLAC", 900)]);
        let cat = serve.catalogue();
        let (person, group) = (cat.artist_for("Jimi Hendrix").unwrap(), cat.artist_for("Glass Harbour").unwrap());
        cat.db_for_tests().execute("UPDATE records SET artist = 'Jimi Hendrix' WHERE id = ?1", [id]).unwrap();
        for (position, artist) in [group, person].iter().enumerate() {
            cat.db_for_tests().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, ?3, 'files')", rusqlite::params![id, artist, position as i64]).unwrap();
        }
        let line = |serve: &Serve| {
            let e = answer(serve, id, &alice(), &Place::Library, &[]);
            let named: Vec<(String, String)> = e.properties.artists.value.iter().map(|n| (n.id.clone(), n.name.clone())).collect();
            (named, e.properties.credit.clone())
        };
        let ids = (crate::serve::ids::artist(group), crate::serve::ids::artist(person));
        assert_eq!(line(&serve), (vec![(ids.0.clone(), "Glass Harbour".into()), (ids.1.clone(), "Jimi Hendrix".into())], "Glass Harbour & Jimi Hendrix".to_string()), "no served row: its own name");
        // The served library names the group's id by the spelling credited most beside it (a box set's forty files).
        serve.library.db().execute("INSERT INTO artists (id, name, album_count, song_count, order_name) VALUES (?1, 'The Jimi Hendrix Experience', 1, 40, 'jimi hendrix experience')", [&ids.1]).unwrap();
        let (named, credit) = line(&serve);
        assert_eq!(named[1], (ids.1.clone(), "The Jimi Hendrix Experience".to_string()), "by its id, its one name");
        assert_eq!(credit, "Glass Harbour & The Jimi Hendrix Experience", "never the files' string");
    }

    /// NAMES.15's DC1a half, a guard: the record's sheet offers no files line, plan or run for a name or a credit (OQ-NAMES-2 (a),
    /// OQ-NAMES-6 (a): writing is 0.19's), whoever opens it, whatever its files say beside its credit.
    #[test]
    fn names_write_nothing_in_018() {
        let serve = served();
        let id = night_ferry(&serve, &[(MUSIC, "FLAC", 900)]);
        let cat = serve.catalogue();
        let group = cat.artist_for("The Jimi Hendrix Experience").unwrap();
        cat.db_for_tests().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'files')", rusqlite::params![id, group]).unwrap();
        for viewer in [root(), alice()] {
            let e = answer(&serve, id, &viewer, &Place::Library, &[]);
            for a in &e.actions {
                let said = format!("{} {} {}", a.name, a.title, a.href).to_lowercase();
                assert!(!["write", "retag", "tags", "rename", "credit"].iter().any(|w| said.contains(w)), "{said}");
            }
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

    /// A record's sources' lists and facts as a read keeps them, on its references.
    fn tracks(n: u32, ms: i64) -> Vec<crate::catalogue::identity::RefTrack> {
        (1..=n).map(|p| crate::catalogue::identity::RefTrack { disc: 1, position: p, title: format!("Track {p}"), length_ms: Some(ms), recording: None }).collect()
    }

    fn held(serve: &Serve, record: i64, refs: &[(&str, &str, &str, &str)]) {
        for (source, kind, value, how) in refs {
            serve.catalogue().set_reference(&crate::serve::ids::record(record), &crate::catalogue::records::Ref::new(source, kind, value), how).unwrap();
        }
    }

    fn says(serve: &Serve, record: i64, fact: &str, source: &str, kind: &str, value: &str, said: &str) {
        let of = crate::catalogue::facts::of_ref(kind, value);
        serve.catalogue().said(record, fact, source, &of, Some(said), 1).unwrap();
    }

    /// VR39 (FA13, M10, OQ-VR-9 (c); the entity's half of the domain's `resolve_year`/`resolve_type`): Salt Lines, not had,
    /// MusicBrainz 2014 on `G70`, Discogs 2015 on `M70`: its year two versions, none chosen, and the Head none; Bandcamp's 2014 on
    /// `P70` joins MusicBrainz's version; a guess's year (`R70s`, found by its names) counts in nothing. Its type: none while its
    /// sources differ ("Album" twice beside Bandcamp's "EP"), theirs once all agree, its why said (revised by the owner, 2026-11-21
    /// 13:59: no majority). Low Ceiling, one each: no type until root picks Discogs' list
    /// (11 tracks, 52:00), then the release type rule's over it, "by the picked list", and the picked release's year. A had record's
    /// year is its files', its type the rule's. Red first: the year one value with no versions, the type the record's own column.
    /// VR49 (the entity's half; the domain's `a_records_secondaries_are_one_set_naming_their_sources`): (c) Salt Lines, not had:
    /// MusicBrainz's Live on `G70`, Discogs' Live and Remix on `M70`: one set, each word naming its sources; (e) Thin Walls: its only
    /// reference a guess saying Live: none; (d) Low Ceiling: Discogs' Compilation; root's Live by hand, alone; after Reset,
    /// Compilation [discogs]. Red first: the view carried no secondaries.
    #[test]
    fn a_records_secondaries_name_their_sources_on_its_view() {
        let serve = served();
        let words = |e: &Entity<RecordProperties>| e.properties.secondary.iter().flatten().map(|v| (v.value.clone(), v.by.join(","))).collect::<Vec<_>>();
        let pairs = |p: &[(&str, &str)]| p.iter().map(|(w, by)| (w.to_string(), by.to_string())).collect::<Vec<_>>();
        let salt = serve.catalogue().record_for_tests("Salt Lines", 0);
        held(&serve, salt, &[("musicbrainz", "release-group", "G70", "link"), ("discogs", "master", "M70", "link")]);
        says(&serve, salt, "secondary", "musicbrainz", "release-group", "G70", r#"["live"]"#);
        says(&serve, salt, "secondary", "discogs", "master", "M70", r#"["live","remix"]"#);
        let e = answer(&serve, salt, &alice(), &Place::Search, &[]);
        assert_eq!(words(&e), pairs(&[("live", "musicbrainz,discogs"), ("remix", "discogs")]), "(c)");
        let thin = serve.catalogue().record_for_tests("Thin Walls", 0);
        held(&serve, thin, &[("musicbrainz", "release", "R80s", "search")]);
        says(&serve, thin, "secondary", "musicbrainz", "release", "R80s", r#"["live"]"#);
        assert!(words(&answer(&serve, thin, &alice(), &Place::Search, &[])).is_empty(), "(e) a guess counts nowhere");
        let low = serve.catalogue().record_for_tests("Low Ceiling", 0);
        held(&serve, low, &[("discogs", "release", "D72", "link")]);
        says(&serve, low, "secondary", "discogs", "release", "D72", r#"["compilation"]"#);
        let album = crate::serve::ids::record(low);
        serve.userdata.set_overrides(std::slice::from_ref(&album), &[("secondary".to_string(), Some(serde_json::json!(["live"])))], 1).unwrap();
        assert_eq!(words(&answer(&serve, low, &root(), &Place::Search, &[])), pairs(&[("live", "hand")]), "(d) the hand's list, final");
        serve.userdata.set_overrides(std::slice::from_ref(&album), &[("secondary".to_string(), None)], 2).unwrap();
        assert_eq!(words(&answer(&serve, low, &root(), &Place::Search, &[])), pairs(&[("compilation", "discogs")]), "(d) after Reset");
    }

    #[test]
    fn a_records_year_and_type_are_resolved_none_chosen() {
        let serve = served();
        let salt = serve.catalogue().record_for_tests("Salt Lines", 0);
        held(&serve, salt, &[("musicbrainz", "release-group", "G70", "link"), ("discogs", "master", "M70", "link"), ("bandcamp", "album", "P70", "link"), ("musicbrainz", "release", "R70s", "search")]);
        says(&serve, salt, "year", "musicbrainz", "release-group", "G70", "2014");
        says(&serve, salt, "year", "discogs", "master", "M70", "2015");
        says(&serve, salt, "year", "musicbrainz", "release", "R70s", "1999");
        let versions = |e: &Entity<RecordProperties>| e.properties.year.versions.iter().map(|v| (v.value, v.by.join(","))).collect::<Vec<_>>();
        let e = answer(&serve, salt, &alice(), &Place::Search, &[]);
        assert_eq!(e.properties.year.value, None, "the Head shows no year until a hand picks");
        assert_eq!(versions(&e), [(Some(2014), "musicbrainz".to_string()), (Some(2015), "discogs".to_string())], "a guess's 1999 is none");
        says(&serve, salt, "year", "bandcamp", "album", "P70", "2014");
        let e = answer(&serve, salt, &alice(), &Place::Search, &[]);
        assert_eq!(versions(&e), [(Some(2014), "musicbrainz,bandcamp".to_string()), (Some(2015), "discogs".to_string())]);
        for (source, kind, value, said) in [("musicbrainz", "release-group", "G70", "Album"), ("discogs", "master", "M70", "Album"), ("bandcamp", "album", "P70", "EP")] {
            says(&serve, salt, "type", source, kind, value, said);
        }
        let e = answer(&serve, salt, &alice(), &Place::Search, &[]);
        assert_eq!((e.properties.kind.value.clone(), e.properties.type_why.as_str()), (None, ""), "Album, Album, EP: none until a hand picks");
        says(&serve, salt, "type", "bandcamp", "album", "P70", "Album");
        let e = answer(&serve, salt, &alice(), &Place::Search, &[]);
        assert_eq!((e.properties.kind.value.as_deref(), e.properties.type_why.as_str()), (Some("Album"), "by its sources"), "all agree");
        let low = serve.catalogue().record_for_tests("Low Ceiling", 0);
        held(&serve, low, &[("musicbrainz", "release-group", "G72", "link"), ("discogs", "release", "D72", "link")]);
        says(&serve, low, "type", "musicbrainz", "release-group", "G72", "EP");
        says(&serve, low, "type", "discogs", "release", "D72", "Album");
        says(&serve, low, "year", "musicbrainz", "release-group", "G72", "2015");
        says(&serve, low, "year", "discogs", "release", "D72", "2016");
        let e = answer(&serve, low, &alice(), &Place::Search, &[]);
        assert_eq!((e.properties.kind.value.clone(), e.properties.type_why.as_str()), (None, ""), "one each: no tie broken by a source's name");
        let discogs = crate::catalogue::lists::Reference::new("discogs", "release", "D72");
        serve.catalogue().keep_list(low, &discogs, &tracks(11, 52 * 60_000 / 11), 1).unwrap();
        serve.catalogue().pick_list(low, Some(&discogs)).unwrap();
        let e = answer(&serve, low, &root(), &Place::Search, &[]);
        assert_eq!((e.properties.kind.value.as_deref(), e.properties.type_why.as_str()), (Some("album"), "by the picked list"));
        assert_eq!((e.properties.year.value, e.properties.year.versions.len()), (Some(2016), 2), "the picked release's year, the versions still said");
        let had = night_ferry(&serve, &[(MUSIC, "FLAC", 900)]);
        serve.catalogue().db_for_tests().execute("UPDATE records SET year = 2014, kind = 'album' WHERE id = ?1", [had]).unwrap();
        says(&serve, had, "year", "discogs", "release", "D42", "2015");
        let e = answer(&serve, had, &alice(), &Place::Library, &[]);
        assert_eq!((e.properties.year.value, e.properties.year.by.first().map(String::as_str)), (Some(2014), Some("files")), "FA1: the files decide");
        assert!(e.properties.year.versions.is_empty(), "Discogs' 2015 no version of a had record");
        assert_eq!(e.properties.type_why, "by the release type rule");
    }

    /// VR6 and FA7 (twins, one test; the record entity's half): Answered, not had, shows MusicBrainz's 9 alone, one version, no
    /// Versions section; (a) Bandcamp's 9 agreeing joins it, still one; (b) Discogs' 11 makes two versions, none marked; (c)
    /// MusicBrainz's re-read of the same release gives 10, that reference's row replaced (one row a reference): three, "9 tracks ·
    /// Bandcamp", "10 tracks · MusicBrainz", "11 tracks · Discogs"; (d) Discogs switched off: its row kept with its `at`, still a
    /// version (the entity reads what is kept, never a source). Each step logged (LV1). Never the first list keeping its mark
    /// because it came first. Red first: no Tracklist lines (part 1: copies and editions alone).
    #[test]
    fn a_later_list_joins_or_splits_the_versions() {
        let serve = served();
        let cat = serve.catalogue().clone();
        let answered = cat.record_for_tests("Answered", 0);
        let (mb, bc, dc) = (
            crate::catalogue::lists::Reference::new("musicbrainz", "release", "R7"),
            crate::catalogue::lists::Reference::new("bandcamp", "album", "P7"),
            crate::catalogue::lists::Reference::new("discogs", "release", "D7"),
        );
        let lines = || -> Vec<(Option<u16>, String, bool)> {
            answer(&serve, answered, &root(), &Place::Search, &[]).entities.iter().map(|v| (v.tracks, v.sources.join(", "), v.plays)).collect()
        };
        let logged = |before: i64| {
            serve.refresh().unwrap();
            let now = serve.userdata.change_rev().unwrap();
            assert!(now > before, "each step one change row (LV1): {now} > {before}");
            now
        };
        let rev = serve.userdata.change_rev().unwrap();
        cat.keep_list(answered, &mb, &tracks(9, 200_000), 1).unwrap();
        let rev = logged(rev);
        assert!(lines().is_empty(), "one version: no Versions section");
        cat.keep_list(answered, &bc, &tracks(9, 200_000), 2).unwrap();
        let rev = logged(rev);
        assert!(lines().is_empty(), "(a) Bandcamp's agreeing list joins it: one version");
        cat.keep_list(answered, &dc, &tracks(11, 200_000), 3).unwrap();
        let rev = logged(rev);
        assert_eq!(lines(), [(Some(9), "MusicBrainz, Bandcamp".to_string(), false), (Some(11), "Discogs".to_string(), false)], "(b) two, none marked");
        cat.keep_list(answered, &mb, &tracks(10, 200_000), 4).unwrap();
        logged(rev);
        let three = [(Some(9), "Bandcamp".to_string(), false), (Some(10), "MusicBrainz".to_string(), false), (Some(11), "Discogs".to_string(), false)];
        assert_eq!(lines(), three, "(c) the release's row replaced: three");
        assert_eq!(cat.lists_kept(answered).unwrap().iter().filter(|l| l.reference == mb).count(), 1, "(c) one row a reference");
        let was = crate::app::switches::set_for_tests(crate::app::switches::DISCOGS, false);
        let lines_off = lines();
        crate::app::switches::set_for_tests(crate::app::switches::DISCOGS, was);
        assert_eq!(lines_off, three, "(d) switched off: Discogs' list still a version");
        assert_eq!(cat.lists_kept(answered).unwrap().iter().find(|l| l.reference == dc).map(|l| l.at), Some(3), "(d) kept with its at");
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
