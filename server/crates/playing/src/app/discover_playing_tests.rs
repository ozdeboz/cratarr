//! `discover`'s tests that open a record's page, moved up out of records for the split's phase 2: an artist's Bandcamp list
//! joining their records (the page through `PlayingService`), and every list naming a record's cover as its page does
//! (`playing::page_at`). Both sit above records. Their bodies are as they were; `super::super::` paths are named from the
//! crate.

use std::sync::Arc;

use crate::app::discover::missing_tests::{
    THEIR_GROUPS, THEIR_PAGES, THEIR_TITLES, count_records, holder_of, settled, their_bandcamp, their_had, their_had_hold, their_musicbrainz,
};
use crate::app::discover::{FoundAlbum, Missing, readings};
use crate::catalogue::Catalogue;
use crate::catalogue::records::{Listing, Ref};

// ---- An artist's Bandcamp list joins the records of its names (from the missing list's tests)

/// The live records (merged into none).
fn count_live(cat: &Catalogue) -> i64 {
    cat.db_for_tests().query_row("SELECT count(*) FROM records WHERE merged_into IS NULL", [], |r| r.get(0)).unwrap()
}

/// The switches as the test sets them, put back when it ends, however it ends.
struct Switched(Vec<(&'static str, bool)>);

impl Switched {
    fn to(set: &[(&'static str, bool)]) -> Self {
        Switched(set.iter().map(|(name, on)| (*name, crate::app::switches::set_for_tests(name, *on))).collect())
    }
}

impl Drop for Switched {
    fn drop(&mut self) {
        for (name, on) in &self.0 {
            crate::app::switches::set_for_tests(name, *on);
        }
    }
}

/// F2.1 (the finding): Kite Theory's records not had from MusicBrainz's list; with MusicBrainz and Discogs switched off and
/// Bandcamp on (staging's switches), Bandcamp's list of the same four, "Answered" as "Answered EP", joins them: no record made,
/// each page on its record alone (how `link`), the settled list four rows titled as before, each with its page. Then "Answered"
/// is opened (F1.11's F2 case): MusicBrainz and Discogs answer switched off, Bandcamp with its list, which the record page shows.
/// Staging's had halves too (F2.9 on the artist page's path): their two untagged had albums, which MusicBrainz's list and
/// Bandcamp's join, none of them listed as missing. Never a record made beside one of its names, a title changed by a list, a
/// page on two records.
#[tokio::test]
async fn an_artists_bandcamp_list_joins_the_records_of_its_names() {
    use crate::app::switches::{BANDCAMP, DISCOGS, MUSICBRAINZ, TEST_STATE};
    use crate::catalogue::identity::RefTrack;
    use crate::serve::ids;
    let _one = TEST_STATE.lock().await;
    let d = std::env::temp_dir().join(format!("cratarr-f2-joined-{}", std::process::id()));
    std::fs::create_dir_all(d.join(crate::playlists::PLAYLIST_DIR)).unwrap();
    let lib = crate::app::Library::new(None, Some(d.to_string_lossy().into_owned()), Vec::new(), Default::default(), None, Arc::new(crate::userdata::UserData::open(None).unwrap()), None).await.unwrap();
    let cat = lib.catalogue();
    let at = Arc::as_ptr(&cat) as usize;
    readings().lock().unwrap().retain(|(c, ..), _| *c != at);
    let artist = cat.artist_for("Kite Theory").unwrap();
    let had = their_had(&cat, artist);
    let first = count_live(&cat);
    settled(&cat, artist, &[their_musicbrainz()]).await;
    assert_eq!(count_live(&cat), first + 4, "MusicBrainz's list: its four records not had made, Kyverdale Road joined the had album");
    let theirs: Vec<i64> = THEIR_GROUPS.iter().map(|g| holder_of(&cat, &Ref::new("musicbrainz", "release-group", g))).collect();
    let answered = theirs[3];
    let _switched = Switched::to(&[(MUSICBRAINZ, false), (DISCOGS, false), (BANDCAMP, true)]);
    let (before, live) = (count_records(&cat), count_live(&cat));
    let rows = settled(&cat, artist, &[their_bandcamp()]).await;
    assert_eq!(count_live(&cat), live, "no record made beside one of its names");
    assert_eq!(count_records(&cat), before + 2, "but the two rows joined to had albums, each a record of its own merged into its had album (F2.9-hand)");
    for (record, page) in theirs.iter().zip(THEIR_PAGES) {
        assert_eq!(cat.subject_of(&Ref::new("bandcamp", "album", page)).unwrap(), [ids::record(*record)], "{page}: on its record alone");
        let how = cat.references_of(&ids::record(*record)).unwrap().into_iter().find(|(r, _)| r.source == "bandcamp").map(|(_, row)| row.how);
        assert_eq!(how.as_deref(), Some("link"), "{page}");
    }
    let mut listed: Vec<(String, String, Option<String>)> = rows.iter().map(|m| (m.record.clone(), m.title.clone(), m.bandcamp.clone())).collect();
    listed.sort();
    let mut expected: Vec<(String, String, Option<String>)> =
        theirs.iter().zip(THEIR_TITLES).zip(THEIR_PAGES).map(|((record, title), page)| (ids::record(*record), title.to_string(), Some(page.to_string()))).collect();
    expected.sort();
    assert_eq!(listed, expected, "four rows, titled as before, each with its page; no had album among them");
    their_had_hold(&cat, had);
    // "Answered" opened: MusicBrainz and Discogs switched off, Bandcamp's list read through its page.
    let tracks = vec![
        RefTrack { disc: 1, position: 1, title: "Lamplight".into(), length_ms: Some(200_000), recording: None },
        RefTrack { disc: 1, position: 2, title: "Morning".into(), length_ms: Some(180_000), recording: None },
    ];
    let read = |r: Ref| {
        let list = tracks.clone();
        async move {
            match r.source.as_str() {
                "musicbrainz" => Err(anyhow::Error::from(crate::app::references::SwitchedOff("MusicBrainz"))),
                "discogs" => Err(anyhow::Error::from(crate::app::references::SwitchedOff("Discogs"))),
                "bandcamp" if r.kind == "album" && r.value == THEIR_PAGES[3] => Ok(Some((list, None, Default::default()))),
                _ => Err(anyhow::anyhow!("not expected: {r:?}")),
            }
        }
    };
    let judged = crate::app::references::tracklist(&cat, answered, read).await.unwrap().expect("a list to judge by");
    assert_eq!(judged.timed.map(|(source, _)| source).as_deref(), Some("bandcamp"), "Bandcamp's list");
    let journal = Arc::new(crate::journal::Journal::open(None).unwrap());
    let sync = Arc::new(crate::app::sync::SyncService::new(lib.clone(), crate::sync_state::SyncState::open(None).unwrap(), journal.clone()));
    let playing = crate::app::PlayingService::new(lib.clone(), Arc::new(crate::app::PlaylistService::new(lib.clone(), sync, journal, None)));
    let page = playing.record_view(&ids::record(answered)).await.unwrap().expect("its page");
    let shown: Vec<Option<String>> = page.rows.iter().map(|r| r.name.clone()).collect();
    assert_eq!(shown, [Some("Lamplight".to_string()), Some("Morning".to_string())], "Answered opens with Bandcamp's tracks");
    assert_eq!(page.title, "Answered", "no list renames it");
    std::fs::remove_dir_all(&d).ok();
}

// ---- Every list names a record's cover by the record (the cover's tests)

const GROUP: &str = "00000000-0000-4000-8000-000000000f19";

/// F16.1: every list names a record's cover by the record, `/api/albums/cratarr/al-<n>/cover`, merges followed: the artist's
/// missing list, the record's page, Beyond's rows with a record (a row no record holds keeps its source's picture). The label
/// page's and the requests' rows: their own modules' tests. Never a list naming another address for a record than its page does.
#[tokio::test]
async fn every_list_names_a_records_cover_by_the_record() {
    let _state = crate::app::references::TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let artist = cat.artist_for("Kite Theory").unwrap();
    let row = |title: &str, refs: Vec<Ref>| Listing { title: title.into(), artist: "Kite Theory".into(), kind: Some("album".into()), refs, ..Listing::default() };
    let refs = vec![Ref::new("musicbrainz", "release-group", GROUP), Ref::new("discogs", "master", "5000040"), Ref::new("bandcamp", "album", "https://kitetheory.bandcamp.com/album/night-walks")];
    let record = cat.keep_listing(Some(artist), &[row("Night Walks", refs)], "link").unwrap()[0];
    let own = crate::app::cover_cache::record_address(record);
    let missing: Vec<String> = cat.missing_of(artist).unwrap().into_iter().map(|q| Missing::of(q).cover).collect();
    assert_eq!(missing, [own.clone()], "the missing list");
    let none = |_: Ref| std::future::ready(Ok::<Option<crate::app::references::Read>, anyhow::Error>(None));
    let t = crate::app::moment::now_secs();
    let Some(crate::app::playing::RecordPage::NotHad(view)) = crate::app::playing::page_at(&cat, record, t, none).await.unwrap() else { panic!("a page") };
    assert_eq!(view.more.cover.as_deref(), Some(own.as_str()), "the record's page");
    let found = |group: &str| FoundAlbum { release_group: group.into(), title: "Night Walks".into(), artist: "Kite Theory".into(), year: Some(2020), kind: Some("Album".into()), cover: format!("/api/covers/release-group/{group}"), have: false };
    let elsewhere = "00000000-0000-4000-8000-000000000f20";
    let albums = [found(GROUP), found(elsewhere)];
    let known = crate::app::beyond::known(&cat, &albums, &[], &[]).unwrap();
    let rows = crate::app::beyond::albums(&albums, &[], &known, None);
    assert_eq!(rows.iter().map(|r| r.cover.clone()).collect::<Vec<_>>(), [Some(own.clone()), Some(format!("/api/covers/release-group/{elsewhere}"))], "Beyond's rows");
    // Merged since: a list asked by the old record names the record it is now.
    let older = cat.keep_listing(Some(artist), &[row("Night Walks (Live)", vec![Ref::new("discogs", "release", "3600060")])], "link").unwrap()[0];
    cat.merge_records(older, record).unwrap();
    let q = cat.requested(&[older]).unwrap().remove(&older).unwrap();
    assert_eq!(Missing::of(q).cover, own, "merges followed");
}
