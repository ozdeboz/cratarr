//! The record's picture in Cratarr's own store, on every surface (SCENARIOS-COUPLING COU.11, COU.12 (b), COU.20; SCENARIOS-DC1
//! FA14): Change cover on the one path, its Undo and Reset, Replace's cover question, 0.17's picks at the step; no file written.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cratarr_domain::can::{May, Viewer};

use crate::app::commands::Commands;
use super::{CoverKind, PickCover};
use crate::catalogue::{Catalogue, FolderRules, Rules};
use crate::serve::Serve;
use crate::serve::copies::Kind;

const MUSIC: usize = 0;
const IN: usize = 1;

/// An 8 × 8 picture of one colour, as PNG.
fn png(red: u8) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(8, 8, image::Rgb([red, 40, 90]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// A served library over two real folders, MUSIC (main) and IN (Lidarr's Incoming).
struct Lib {
    roots: Vec<PathBuf>,
    cat: Arc<Catalogue>,
    serve: Arc<Serve>,
}

impl Lib {
    fn new(name: &str) -> Self {
        let roots: Vec<PathBuf> = (0..2).map(|n| std::env::temp_dir().join(format!("cratarr-covers-{name}-{n}-{}", std::process::id()))).collect();
        for r in &roots {
            std::fs::remove_dir_all(r).ok();
            std::fs::create_dir_all(r).unwrap();
        }
        let cat = Arc::new(Catalogue::open(None, roots.clone()).unwrap());
        let serve = Serve::open(None, Rules::default(), &|_| None).unwrap();
        serve.library.set_kinds(vec![(0, Kind::Main), (1, Kind::Incoming)]);
        serve.attach_catalogue(cat.clone());
        Lib { roots, cat, serve }
    }

    /// Night Ferry's copy in `folder`: two real FLACs, `inside` the picture inside each, `beside` a cover file; read and built.
    fn night_ferry(&self, folder: usize, inside: Option<&[u8]>, beside: Option<&[u8]>) -> PathBuf {
        let dir = self.roots[folder].join("Glass Harbour/Night Ferry");
        std::fs::create_dir_all(&dir).unwrap();
        let tracks = [crate::retag::fixtures::flac(&dir, "01 N1"), crate::retag::fixtures::flac(&dir, "02 N2")];
        if let Some(p) = inside {
            assert!(crate::covers_inside::replace(&tracks, Some((p, "image/png"))).is_empty());
        }
        if let Some(b) = beside {
            std::fs::write(dir.join("cover.png"), b).unwrap();
        }
        self.cat.walk_all();
        self.cat.pretend_read_for_tests();
        for (n, title) in [(1, "N1"), (2, "N2")] {
            let rel = format!("Glass Harbour/Night Ferry/0{n} {title}.flac");
            let number = n.to_string();
            let tags = [("album", "Night Ferry"), ("albumartist", "Glass Harbour"), ("artist", "Glass Harbour"), ("title", title), ("tracknumber", number.as_str())];
            self.cat.tag_for_tests(folder as i64, &rel, &tags);
        }
        self.cat.identify().unwrap();
        self.built();
        dir
    }

    fn built(&self) {
        let (_, print) = self.serve.library.change(&self.cat, &FolderRules::default()).unwrap();
        self.serve.library.build_from(&self.cat, &FolderRules::default(), &print).unwrap();
        self.serve.build_passed_for_tests();
    }

    fn album(&self) -> String {
        self.serve.library.album_ids().unwrap().into_iter().next().expect("one album")
    }

    fn record(&self) -> i64 {
        crate::serve::ids::record_number(&self.album()).unwrap()
    }

    /// Every file under both folders, by path, with its bytes: what "no file changes" compares.
    fn files(&self) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(d: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else {
                    out.push((p.clone(), std::fs::read(&p).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        self.roots.iter().for_each(|r| walk(r, &mut out));
        out.sort();
        out
    }

    /// The one path, Change cover's Undo registered.
    fn path(&self) -> Commands {
        let journal = Arc::new(crate::journal::Journal::open(None).unwrap());
        let path = Commands::new(journal, Some(self.serve.clone()), Arc::new(|| 14));
        path.register(Arc::new(CoverKind { serve: self.serve.clone() }));
        path
    }
}

impl Drop for Lib {
    fn drop(&mut self) {
        self.roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }
}

fn root() -> Viewer {
    Viewer::new("root", true, None, May::default(), None)
}

/// Every record-level surface's picture of `album`: the page's and OpenSubsonic's by album (the resolver), the list's size and the
/// device's small one (each against the same picture made so), and a song's (OpenSubsonic's `getCoverArt` by song id, the media
/// notification's `/api/cover/{song}`), as bytes.
async fn surfaces(serve: &Arc<Serve>, album: &str) -> Vec<(&'static str, Option<Vec<u8>>)> {
    let page = serve.cover_bytes(album).await.unwrap().map(|(_, b)| b);
    let song = serve.library.songs_of_albums(&[album.to_string()]).unwrap().into_iter().next().expect("a song").id;
    let by_song = serve.cover(&song, None, &axum::http::HeaderMap::new()).await.unwrap();
    let by_song = axum::body::to_bytes(by_song.into_body(), usize::MAX).await.unwrap().to_vec();
    let chosen_for_song = serve.has_chosen_cover(album).unwrap();
    vec![("page and getCoverArt by album", page), ("getCoverArt by song", Some(by_song)), ("a song shows its record's chosen cover", Some(vec![u8::from(chosen_for_song)]))]
}

/// COU.11 (FA14): Change cover keeps the chosen picture in Cratarr's store, a hand fact with its source, who and when; every
/// surface shows it, from the one resolver, its stamp moved; no file changes; Undo gives back the cover shown before; Reset drops the
/// choice and the files' picture shows; a copy shown as a copy shows its own files' picture; the services off, it still shows.
/// Red first: Change cover wrote the picture into and beside every copy's files, and nothing kept the choice.
#[tokio::test]
async fn a_chosen_cover_is_kept_and_shown_everywhere_without_a_file() {
    let lib = Lib::new("cou11");
    let (theirs, inside, picked, again) = (png(10), png(20), png(200), png(220));
    lib.night_ferry(MUSIC, Some(&inside), Some(&theirs));
    let (album, record) = (lib.album(), lib.record());
    let before = lib.files();
    let stamp = |l: &Lib| l.serve.cover_stamps().get(&album).cloned();
    let was = stamp(&lib);
    assert_eq!(lib.serve.cover_bytes(&album).await.unwrap().map(|(_, b)| b), Some(theirs.clone()), "the files' picture first");
    let path = lib.path();
    let pick = PickCover { serve: lib.serve.clone(), record, picture: Some((picked.clone(), "coverartarchive".into())) };
    let answer = path.run(&root(), None, &pick).await.unwrap();
    let kept = lib.cat.kept_cover(record).unwrap().expect("kept in the store");
    assert_eq!((kept.bytes.clone(), kept.source.as_str(), kept.how.as_str(), kept.who.as_deref()), (picked.clone(), "coverartarchive", "hand", Some("root")));
    for (surface, shows) in surfaces(&lib.serve, &album).await {
        let want = if surface.starts_with("a song") { vec![1] } else { picked.clone() };
        assert_eq!(shows, Some(want), "{surface}");
    }
    let small = lib.serve.small_cover(&album, 160, 75).await.unwrap().unwrap();
    assert_eq!(small, crate::serve::art::jpeg_at(&picked, 160, 75).unwrap(), "the device's covers feed");
    assert_ne!(stamp(&lib), was, "its stamp moves: each surface and the device take it at once");
    assert_eq!(lib.files(), before, "no file changes: every byte and every folder's listing as before");
    let music = lib.roots[MUSIC].display().to_string();
    assert_eq!(lib.serve.copy_cover_bytes(&album, &music).await.unwrap().map(|(_, b)| b), Some(theirs.clone()), "a copy as a copy: its own files' picture");
    // The services off: the kept bytes still show (T-COU-7).
    for s in [crate::app::switches::MUSICBRAINZ, crate::app::switches::DISCOGS] {
        crate::app::switches::set_for_tests(s, false);
    }
    assert_eq!(lib.serve.cover_bytes(&album).await.unwrap().map(|(_, b)| b), Some(picked.clone()), "with the services off");
    for s in [crate::app::switches::MUSICBRAINZ, crate::app::switches::DISCOGS] {
        crate::app::switches::set_for_tests(s, true);
    }
    // Undo: the cover shown before.
    let token = answer.undo.expect("its Undo");
    let undone = path.undo(&root(), &[token], &HashMap::new()).await;
    assert!(undone.refused.is_empty(), "{undone:?}");
    assert_eq!(lib.serve.cover_bytes(&album).await.unwrap().map(|(_, b)| b), Some(theirs.clone()), "Undo: the files' picture again");
    // Picked again, then Reset: the files' picture shows; then picked once more over it.
    path.run(&root(), None, &PickCover { serve: lib.serve.clone(), record, picture: Some((picked.clone(), "upload".into())) }).await.unwrap();
    let reset = path.run(&root(), None, &PickCover { serve: lib.serve.clone(), record, picture: None }).await.unwrap();
    assert!(reset.undo.is_some(), "a Reset is a Change too");
    assert_eq!(lib.serve.cover_bytes(&album).await.unwrap().map(|(_, b)| b), Some(theirs.clone()), "Reset: the files' picture");
    path.run(&root(), None, &PickCover { serve: lib.serve.clone(), record, picture: Some((again.clone(), "upload".into())) }).await.unwrap();
    assert_eq!(lib.serve.cover_bytes(&album).await.unwrap().map(|(_, b)| b), Some(again.clone()));
    assert_eq!(lib.files(), before, "still no file changed");
}

/// COU.11's merge and retention (T-COU-20, RT01): of two records' picks meeting in a merge, the later stays and the other is kept as
/// a candidate; a record forgotten takes its picks with it. Red first: no store.
#[test]
fn a_merge_keeps_the_later_pick_and_retention_forgets_it() {
    let cat = Catalogue::open(None, Vec::new()).unwrap();
    let (a, b) = (cat.record_for_tests("Night Ferry", 0), cat.record_for_tests("Night Ferry (Remaster)", 0));
    let cover = |bytes: Vec<u8>, at: i64| crate::catalogue::covers::Cover {
        mime: "image/png".into(),
        bytes,
        source: "upload".into(),
        who: Some("root".into()),
        at,
        how: "hand".into(),
        run: None,
        change: None,
    };
    cat.keep_cover(a, &cover(png(1), 100)).unwrap();
    cat.keep_cover(b, &cover(png(2), 200)).unwrap();
    cat.merge_records(b, a).unwrap();
    assert_eq!(cat.kept_cover(a).unwrap().map(|c| c.bytes), Some(png(2)), "the later pick stays, whichever way the merge runs");
    assert_eq!(cat.cover_candidates(a).unwrap(), [("upload".to_string(), 100)], "the other kept as a candidate");
    assert_eq!(cat.kept_cover(b).unwrap().map(|c| c.at), Some(200), "the merged id answers as the one it went into");
    // RT01: a record forgotten takes its pick and candidates with it.
    let quiet = cat.record_for_tests("Far Shore", 0);
    cat.keep_cover(quiet, &cover(png(3), 300)).unwrap();
    let now = crate::app::moment::now_secs();
    cat.forget_quiet(1, &Default::default()).unwrap();
    let gone = cat.forget_quiet(now + 400 * 86_400, &Default::default()).unwrap();
    assert!(gone.contains(&quiet), "forgotten: {gone:?}");
    let kept: i64 = cat.db_for_tests().query_row("SELECT count(*) FROM record_covers WHERE record = ?1", [quiet], |r| r.get(0)).unwrap();
    assert_eq!(kept, 0, "its pick forgotten with it");
    assert!(cat.kept_cover(a).unwrap().is_some(), "a record in use keeps its own");
}

/// COU.12 (b) (T-COU-8, T-COU-15): Replace's cover question. (b1) no hand pick: "keep the old one" makes the old copy's picture the
/// record's chosen cover, kept before the old files go; (b2) "take the new copy's": no choice made, the new files' picture shows;
/// (b3) a hand's pick stays whichever is asked. Nothing is written into the new copy. Red first: `keep_cover` wrote the old picture
/// into and beside the new copy, and a pick lived in the files only.
#[test]
fn one_picture_everywhere_and_the_pick_wins() {
    let lib = Lib::new("cou12b");
    let (old, new, picked) = (png(30), png(60), png(90));
    let old_dir = lib.night_ferry(MUSIC, None, Some(&old));
    let new_dir = lib.night_ferry(IN, Some(&new), None);
    let record = lib.record();
    let before = std::fs::read_dir(&new_dir).unwrap().count();
    // (b1)
    assert!(crate::app::incoming::keep_cover(&lib.cat, record, &old_dir, "files@MUSIC").unwrap(), "(b1) kept");
    let kept = lib.cat.kept_cover(record).unwrap().unwrap();
    assert_eq!((kept.bytes, kept.how.as_str(), kept.source.as_str()), (old.clone(), "kept", "files@MUSIC"));
    assert_eq!(std::fs::read_dir(&new_dir).unwrap().count(), before, "(b1) nothing beside the new files");
    assert!(crate::serve::art::cover_file(&new_dir).is_none());
    // (b2): no choice: the new files' picture shows once the old copy goes (here, its kept cover let go: the answer "take the new").
    lib.cat.drop_cover(record).unwrap();
    std::fs::remove_dir_all(&old_dir).unwrap();
    lib.cat.walk_all();
    lib.cat.identify().unwrap();
    lib.built();
    assert_eq!(lib.cat.kept_cover(record).unwrap(), None, "(b2) no choice made");
    // (b3): a hand's pick stays, whichever is asked.
    let hand = crate::catalogue::covers::Cover { mime: "image/png".into(), bytes: picked.clone(), source: "upload".into(), who: Some("root".into()), at: 5, how: "hand".into(), run: None, change: None };
    lib.cat.keep_cover(record, &hand).unwrap();
    assert!(!crate::app::incoming::keep_cover(&lib.cat, record, &new_dir, "files@IN").unwrap(), "(b3) the hand's pick untouched");
    assert_eq!(lib.cat.kept_cover(record).unwrap().map(|c| c.bytes), Some(picked));
}

/// COU.20 (T-COU-21): 0.17's Change cover picks become hand picks at 0.18's step: C1 (kept, not undone) the record's chosen cover,
/// at its run's time, its bytes the file it wrote; C2's run pruned: listed, its record's files' picture showing; a later Replace
/// keeps it; Undo of C1 drops it; the step writes no file and runs once. Red first: no store and no part.
#[test]
fn the_017_cover_picks_become_hand_picks_at_the_step() {
    let lib = Lib::new("cou20");
    let (theirs, c1) = (png(70), png(140));
    lib.night_ferry(MUSIC, None, Some(&theirs));
    let record = lib.record();
    let data = std::env::temp_dir().join(format!("cratarr-cou20-data-{}", std::process::id()));
    std::fs::remove_dir_all(&data).ok();
    let (kept1, kept2) = (data.join("cover-undo/1000-0"), data.join("cover-undo/900-0"));
    for (k, bytes) in [(&kept1, &c1), (&kept2, &png(99))] {
        std::fs::create_dir_all(k).unwrap();
        std::fs::write(k.join("new"), bytes).unwrap();
    }
    let journal = crate::journal::Journal::open(None).unwrap();
    let write = |kept: &Path| crate::journal::FileWrite {
        album: crate::serve::ids::record(record),
        path: kept.to_path_buf(),
        track: None,
        before: Vec::new(),
        after: Vec::new(),
        from: Vec::new(),
        undone: false,
        whole: Some(crate::journal::Whole { op: "cover".into(), ms: None, kept: Some(kept.to_path_buf()), planned: String::new(), wrote: None }),
    };
    let run = crate::journal::FileRun { what: "cover".into(), title: "Cover · Night Ferry".into(), records: Vec::new(), files: vec![write(&kept1)], failed: Vec::new(), planned: Vec::new() };
    let c1_id = journal.record_run_writing(1000, &run, Some("root"), i64::MAX).unwrap();
    journal.run_ended(c1_id);
    let before = lib.files();
    let (picks, unknown) = crate::app::step_018::cover_picks(&journal, Some(&data)).unwrap();
    assert_eq!(unknown, ["900-0"], "C2's run was pruned: listed, never a pick");
    let report = lib.cat.step_018_covers(&picks, &unknown, &|| Ok(())).unwrap().expect("its first run");
    assert_eq!(report.counts.get("0.17 cover picks carried"), Some(&1));
    let kept = lib.cat.kept_cover(record).unwrap().unwrap();
    assert_eq!((kept.bytes.clone(), kept.how.as_str(), kept.at, kept.run), (c1.clone(), "017", 1000, Some(c1_id)), "by the hand, at its run's time");
    assert!(lib.cat.step_018_covers(&picks, &unknown, &|| Ok(())).unwrap().is_none(), "once");
    assert_eq!(lib.files(), before, "the step writes no file");
    // A later Replace taking the new copy's keeps it; Undo of C1 drops the pick it became.
    assert!(!crate::app::incoming::keep_cover(&lib.cat, record, &lib.roots[MUSIC], "files@MUSIC").unwrap(), "a hand pick wins");
    assert_eq!(lib.cat.drop_covers_of_run(c1_id).unwrap(), [record]);
    assert_eq!(lib.cat.kept_cover(record).unwrap(), None, "the cover shown before C1 shows");
    std::fs::remove_dir_all(&data).ok();
}

/// COU.12 (c) (T-COU-15, T-COU-22): "Two Piers", its pictures inside its files only; the placed `01 Pier.flac`, no picture inside,
/// sorts first by path: the record's cover is the picture inside `02 Landing.flac`, on every surface. Red first: the copy's picture was
/// its first track's by path, the placed one with none.
#[tokio::test]
async fn a_placed_file_is_no_source_of_its_records_picture() {
    let lib = Lib::new("cou12c");
    let picture = png(120);
    let dir = lib.roots[MUSIC].join("Glass Harbour/Two Piers (2018)");
    std::fs::create_dir_all(&dir).unwrap();
    let (pier, landing) = (crate::retag::fixtures::flac(&dir, "01 Pier"), crate::retag::fixtures::flac(&dir, "02 Landing"));
    assert!(crate::covers_inside::replace(std::slice::from_ref(&landing), Some((&picture, "image/png"))).is_empty());
    let record = lib.cat.record_for_tests("Two Piers", 0);
    lib.cat.place_file(0, "Glass Harbour/Two Piers (2018)/01 Pier.flac", record, None, (1, 1)).unwrap();
    lib.cat.walk_all();
    lib.cat.pretend_read_for_tests();
    for (path, title, n) in [(&pier, "Pier", "1"), (&landing, "Landing", "2")] {
        let rel = format!("Glass Harbour/Two Piers (2018)/{}", path.file_name().unwrap().to_string_lossy());
        lib.cat.tag_for_tests(0, &rel, &[("album", "Two Piers"), ("albumartist", "Glass Harbour"), ("title", title), ("tracknumber", n)]);
    }
    lib.cat.identify().unwrap();
    lib.built();
    let album = lib.album();
    assert_eq!(lib.serve.cover_bytes(&album).await.unwrap().map(|(_, b)| b), Some(picture.clone()), "the picture inside the files that speak");
    assert!(lib.serve.cover_stamps().get(&album).is_some_and(|s| !s.is_empty()), "a stamp: OpenSubsonic and the device show it");
}

/// COU.12 (a) (D.1's pictures row, T-COU-7): "Low Tide", had, no picture in its files or beside them; the Cover Art Archive has its
/// release group's front: every surface shows it, kept in Cratarr's store as it first answered, said as its source; it stays with
/// the Archive off (nothing asked again); a hand's pick wins over it. Red first: the page alone showed what the cover cache held,
/// OpenSubsonic and the device none.
#[tokio::test]
async fn a_record_with_no_picture_shows_the_first_to_answer() {
    let lib = Lib::new("cou12a");
    lib.night_ferry(MUSIC, None, None);
    let (album, record) = (lib.album(), lib.record());
    lib.cat.set_reference(&album, &crate::catalogue::records::Ref::new("musicbrainz", "release-group", "G305"), "files").unwrap();
    assert_eq!(lib.serve.cover_bytes(&album).await.unwrap(), None, "no picture of its own");
    let front = png(77);
    let asked = std::sync::atomic::AtomicUsize::new(0);
    let fetch = |p: crate::app::cover_cache::Picture| {
        asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let front = front.clone();
        async move { Ok(matches!(p, crate::app::cover_cache::Picture::Group(g) if g == "G305").then_some(front)) }
    };
    let shown = crate::app::cover_cache::first_answer_with(lib.cat.clone(), &album, &fetch).await.unwrap();
    assert_eq!(shown.map(|(b, _)| b), Some(front.clone()));
    let kept = lib.cat.kept_cover(record).unwrap().unwrap();
    assert_eq!((kept.how.as_str(), kept.source.as_str()), ("first", "coverartarchive"), "kept, said as its source");
    lib.built();
    for (surface, shows) in surfaces(&lib.serve, &album).await.into_iter().take(2) {
        assert_eq!(shows, Some(front.clone()), "{surface}: one picture everywhere");
    }
    assert!(crate::app::cover_cache::first_answer_with(lib.cat.clone(), &album, &fetch).await.unwrap().is_none(), "kept: not asked again");
    assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 1);
    // A hand's pick over it.
    let picked = png(150);
    lib.path().run(&root(), None, &PickCover { serve: lib.serve.clone(), record, picture: Some((picked.clone(), "upload".into())) }).await.unwrap();
    assert_eq!(lib.serve.cover_bytes(&album).await.unwrap().map(|(_, b)| b), Some(picked));
}
