//! PL1 (SCENARIOS-DC2 §11): Keep and Replace never guess Music's layout. A placing before the target library's layout is known
//! waits for it; the last inferred layout is kept per library across a restart; none to infer names the default in the plan, and
//! the administrator's yes saves it.

use std::path::Path;
use std::sync::Arc;

use super::tests::{folder, on_disk};
use super::*;
use crate::app::library::{Layout, learning_words};

/// What the Settings page asks of the playlists: none here.
struct NoPlaylists;

impl crate::app::settings_base::SettingsPlaylists for NoPlaylists {
    fn count(&self) -> crate::app::settings_base::Later<'_, anyhow::Result<usize>> {
        Box::pin(async { Ok(0) })
    }
}

/// The settings over the service, as the app holds them: what works a library's layout out (`infer_naming`).
fn settings_of(svc: &Arc<IncomingService>) -> crate::app::SettingsService {
    crate::app::SettingsService::new(svc.lib.clone(), crate::ports::disk(), Arc::new(NoPlaylists), svc.clone(), String::new(), None, crate::settings::Origin::Env)
}

/// Music: `albums` albums under "Artist n/Album n/", two files each, as the catalogue reads them: the first `titled` of every ten
/// files carry their TITLE so far.
fn music_of(cat: &crate::catalogue::Catalogue, music: &Path, albums: usize, titled: usize) {
    let mut rels = Vec::new();
    for n in 0..albums {
        for t in 1..=2u32 {
            let rel = format!("Artist {n}/Album {n}/{t:02} Song {t}.flac");
            std::fs::create_dir_all(music.join(&rel).parent().unwrap()).unwrap();
            std::fs::write(music.join(&rel), &rel).unwrap();
            rels.push((rel, n, t));
        }
    }
    cat.walk_all();
    cat.pretend_read_for_tests();
    titles(cat, &rels, titled);
}

/// Music's files' tags: every one's album, artist and number; the first `titled` of every ten its TITLE.
fn titles(cat: &crate::catalogue::Catalogue, rels: &[(String, usize, u32)], titled: usize) {
    for (i, (rel, n, t)) in rels.iter().enumerate() {
        let (artist, album, number, title) = (format!("Artist {n}"), format!("Album {n}"), t.to_string(), format!("Song {t}"));
        let mut tags = vec![("albumartist", artist.as_str()), ("artist", artist.as_str()), ("album", album.as_str()), ("tracknumber", number.as_str())];
        if i % 10 < titled {
            tags.push(("title", title.as_str()));
        }
        cat.tag_for_tests(0, rel, &tags);
    }
}

/// Music's 40 albums' files, as `music_of` writes them.
fn rels_of(albums: usize) -> Vec<(String, usize, u32)> {
    (0..albums).flat_map(|n| (1..=2u32).map(move |t| (format!("Artist {n}/Album {n}/{t:02} Song {t}.flac"), n, t))).collect()
}

/// An arrival in Incoming (catalogue folder 1): Glass Harbour's "Tide Tables" (2012), two tracks, its tags read.
fn arrival(cat: &crate::catalogue::Catalogue, inc: &Path) -> Located {
    for (t, title) in [(1, "Slack Water"), (2, "Neap")] {
        let rel = format!("Glass Harbour/Tide Tables/{t:02} {title}.flac");
        std::fs::create_dir_all(inc.join(&rel).parent().unwrap()).unwrap();
        std::fs::write(inc.join(&rel), &rel).unwrap();
    }
    cat.walk_all();
    cat.pretend_read_for_tests();
    for (t, title) in [(1, "Slack Water"), (2, "Neap")] {
        let rel = format!("Glass Harbour/Tide Tables/{t:02} {title}.flac");
        let number = t.to_string();
        let tags = [("albumartist", "Glass Harbour"), ("artist", "Glass Harbour"), ("album", "Tide Tables"), ("title", title), ("tracknumber", number.as_str()), ("originalyear", "2012"), ("date", "2012")];
        cat.tag_for_tests(1, &rel, &tags);
    }
    let album = source::Album { id: "al-11".into(), title: "Tide Tables".into(), artist: Some("Glass Harbour".into()), path: Some("Glass Harbour/Tide Tables".into()), tracks: 2, ..Default::default() };
    Located { source: "cratarr".into(), album, dir: IncomingDir { folder: 1, dir: "Glass Harbour/Tide Tables".into(), release_group: None }, root: inc.to_path_buf() }
}

/// PL1's Given and first When: Music holds 40 albums under "Artist/Album", nothing saved or kept, 30% of its titles read. A Keep
/// into Music waits, "Learning how … names its folders", queued and passed over; a Replace's plan waits alike. Once the titles are
/// read the layout is inferred as "Artist/Album", the waiting Keep goes on, and the plan places under it. Never a file placed while
/// the layout is pending. Red first: Keep places at once under `DEFAULT`.
#[tokio::test]
async fn a_placing_waits_until_the_librarys_layout_is_known() {
    let (music, inc) = (folder("pl1-wait-music"), folder("pl1-wait-inc"));
    let svc = Arc::new(on_disk(&music, &inc).await);
    svc.lib.set_saved_template(None);
    let settings = settings_of(&svc);
    let cat = svc.lib.catalogue();
    music_of(&cat, &music, 40, 3);
    let at = arrival(&cat, &inc);
    settings.infer_naming().await;
    let words = learning_words(&svc.library_name("cratarr", 0));
    assert_eq!(svc.lib.layout(0), Layout::Pending, "30% of the titles read: still being learnt");
    let (_, plan) = svc.plan_keep(&at, 0, None).await.unwrap();
    assert!(matches!(&plan, Err(KeepRefused::Learning(w)) if *w == words), "the plan waits: {plan:?}");
    let kept = svc.move_located("cratarr", at, 0, crate::settings::Quality::None, None).await.expect("the Keep is taken, to wait");
    assert!(kept.queued && kept.undo.is_none(), "queued, not placed");
    let job = svc.jobs().into_iter().max_by_key(|j| j.id).expect("its job");
    assert_eq!((job.state, job.why.as_deref()), (queue::State::Waiting, Some(words.as_str())), "it says what it waits for");
    assert!(svc.queue.next().is_none(), "never placed while the layout is pending");
    assert!(!music.join("Glass Harbour").exists(), "nothing placed");
    // Music's titles read whole, the pass done: the layout Music follows, and the Keep goes on by it.
    titles(&cat, &rels_of(40), 10);
    cat.set_passed_for_tests(true);
    settings.infer_naming().await;
    let Layout::Inferred(t) = svc.lib.layout(0) else { panic!("inferred: {:?}", svc.lib.layout(0)) };
    assert!(t.to_string().starts_with("{albumartist}/{album}/"), "Artist/Album: {t}");
    assert_eq!(svc.queue.next(), Some(job.id), "woken, it goes on");
    let at = arrival(&cat, &inc);
    let (_, plan) = svc.plan_keep(&at, 0, None).await.unwrap();
    assert_eq!(plan.expect("placed by the layout").folder, "Glass Harbour/Tide Tables");
    for d in [music, inc] {
        std::fs::remove_dir_all(d).ok();
    }
}

/// PL1's restart: the layout Music was inferred to follow is kept with its time, so the next start places at once by it while its
/// own inference runs; that inference replaces it where the library's layout has changed. Never pending after a restart where a
/// layout was kept. Red first: the inferred template lives in memory only.
#[tokio::test]
async fn the_last_inferred_layout_survives_a_restart() {
    let (data, music, inc) = (folder("pl1-restart-data"), folder("pl1-restart-music"), folder("pl1-restart-inc"));
    let start = |data: std::path::PathBuf, music: std::path::PathBuf, inc: std::path::PathBuf| async move {
        let db = Arc::new(crate::userdata::UserData::open(None).unwrap());
        let lib = Library::new(Some(data.to_string_lossy().into_owned()), Some(music.to_string_lossy().into_owned()), Vec::new(), Default::default(), None, db, None).await.unwrap();
        lib.catalogue().set_folders(vec![music, inc]).unwrap();
        lib
    };
    {
        let lib = start(data.clone(), music.clone(), inc.clone()).await;
        music_of(&lib.catalogue(), &music, 40, 10);
        let albums_found = crate::naming::layout_of(&names(&lib.catalogue()), true).expect("decided").expect("a layout");
        assert!(!lib.set_layout(0, Some(albums_found), 1_800_000_000), "decided for the first time");
        assert!(matches!(lib.layout(0), Layout::Inferred(_)));
    }
    let lib = start(data.clone(), music.clone(), inc.clone()).await;
    let Layout::Inferred(t) = lib.layout(0) else { panic!("kept across the restart: {:?}", lib.layout(0)) };
    assert!(t.to_string().starts_with("{albumartist}/{album}/"), "{t}");
    assert!(!lib.layout_decided(0), "this start's inference still to run");
    // This start's inference finds the library's layout changed: it replaces the kept one.
    let other: crate::naming::Template = "{album}/{track:02} {title}".parse().unwrap();
    lib.set_layout(0, Some(other.clone()), 1_800_000_600);
    assert_eq!(lib.layout(0), Layout::Inferred(other.clone()));
    drop(lib);
    let lib = start(data.clone(), music.clone(), inc.clone()).await;
    assert_eq!(lib.layout(0), Layout::Inferred(other), "the newer one kept");
    for d in [data, music, inc] {
        std::fs::remove_dir_all(d).ok();
    }
}

/// Music's albums as naming reads them (folder 0).
fn names(cat: &crate::catalogue::Catalogue) -> Vec<crate::naming::Album> {
    let mut albums: Vec<crate::naming::Album> = Vec::new();
    for (rel, tags) in cat.folder_tags(0, &crate::naming::TAGS).unwrap() {
        let (folder, file) = rel.rsplit_once('/').unwrap_or(("", &rel));
        if albums.last().is_none_or(|a| a.folder != folder) {
            albums.push(crate::naming::Album { folder: folder.to_string(), tracks: Vec::new() });
        }
        albums.last_mut().unwrap().tracks.push(crate::naming::Track { file: file.to_string(), tags: crate::naming::Tags(tags) });
    }
    albums.retain(|a| !a.folder.is_empty());
    albums
}

/// PL1's none to infer: an empty Music once the catalogue's pass is done, and a Music read whole with under 90% of its files
/// titled, each have none to infer; neither waits: the plan names `DEFAULT` (the preview's `layout`) and places by it, and the
/// Keep, the administrator's yes, saves it as the library's own. A saved template places at once. Never a layout used for a
/// library that neither saved it nor follows it without the plan saying so. Red first: nothing names or saves the layout.
#[tokio::test]
async fn a_library_mostly_untitled_asks_rather_than_waits() {
    for (case, albums) in [("empty", 0usize), ("untitled", 40)] {
        let (music, inc) = (folder(&format!("pl1-ask-{case}-music")), folder(&format!("pl1-ask-{case}-inc")));
        let svc = Arc::new(on_disk(&music, &inc).await);
        svc.lib.set_saved_template(None);
        let settings = settings_of(&svc);
        let cat = svc.lib.catalogue();
        music_of(&cat, &music, albums, 3);
        let at = arrival(&cat, &inc);
        settings.infer_naming().await;
        assert_eq!(svc.lib.layout(0), Layout::Pending, "{case}: before the pass is done it is still read");
        cat.set_passed_for_tests(true);
        settings.infer_naming().await;
        assert_eq!(svc.lib.layout(0), Layout::NoneToInfer, "{case}: read whole, nothing to infer");
        assert_eq!(svc.asked_layout("cratarr", 0).as_deref(), Some(crate::naming::DEFAULT), "{case}: the plan names the default and asks");
        let (_, plan) = svc.plan_keep(&at, 0, None).await.unwrap();
        assert_eq!(plan.expect("it does not wait").folder, "Glass Harbour - Tide Tables (2012)", "{case}");
        // The yes: the Keep saves the default as the library's own, whatever came of the files.
        let _ = svc.move_located("cratarr", at, 0, crate::settings::Quality::None, None).await;
        assert_eq!(svc.lib.layout(0), Layout::Saved(crate::naming::Template::default()), "{case}: the yes saved it");
        assert_eq!(svc.asked_layout("cratarr", 0), None, "{case}: asked no more");
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }
    // A saved template places at once, pending or not.
    let (music, inc) = (folder("pl1-saved-music"), folder("pl1-saved-inc"));
    let svc = Arc::new(on_disk(&music, &inc).await);
    svc.lib.set_saved_template(None);
    let saved: crate::naming::Template = "{albumartist}/{album}/{track:02} {title}".parse().unwrap();
    svc.lib.set_saved_template(Some(saved.clone()));
    assert_eq!(svc.lib.layout(0), Layout::Saved(saved));
    let at = arrival(&svc.lib.catalogue(), &inc);
    let (_, plan) = svc.plan_keep(&at, 0, None).await.unwrap();
    assert_eq!(plan.expect("at once").folder, "Glass Harbour/Tide Tables");
    for d in [music, inc] {
        std::fs::remove_dir_all(d).ok();
    }
}
