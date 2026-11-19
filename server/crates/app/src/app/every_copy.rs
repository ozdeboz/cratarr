//! What every writer does to a record of two copies, on real files: a fix is the record's, so it reaches every copy, and its
//! Undo takes it out of every copy (0101). The copies name their files differently on purpose: what links them is the track.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::App;
use super::file_runs::{self, Proposal};
use crate::catalogue::normalise::Alias;
use crate::catalogue::{Catalogue, Rules};
use crate::serve::Serve;
use crate::serve::copies::Kind;

const KEPT: [(&str, &str); 2] = [("Kept/01 One", "One"), ("Kept/02 Two", "Two")];
const WAITING: [(&str, &str); 2] = [("Waiting/one", "One"), ("Waiting/two", "Two")];

/// A record of two copies: FLACs in a main library, which play, and MP3s in Incoming under other names.
pub(super) struct Copies {
    root: PathBuf,
    pub serve: Arc<Serve>,
    pub album: String,
    pub flacs: Vec<PathBuf>,
    pub mp3s: Vec<PathBuf>,
}

impl Drop for Copies {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

impl Copies {
    pub fn files(&self) -> Vec<PathBuf> {
        let mut all: Vec<PathBuf> = self.flacs.iter().chain(&self.mp3s).cloned().collect();
        all.sort();
        all
    }

    /// The song of the first FLAC, which both copies hold when they share their tracks.
    pub fn songs(&self) -> Vec<String> {
        self.serve.library.album_tracks(&self.album).unwrap().into_iter().map(|t| t.0).collect()
    }
}

/// Tags as a release has them, read for real by the catalogue.
fn tagged(path: &Path, title: &str, number: &str) {
    use lofty::tag::ItemKey;
    let set = |t: &mut lofty::tag::Tag| {
        for (key, value) in [
            (ItemKey::AlbumTitle, "Both Shores"),
            (ItemKey::AlbumArtist, "The Band"),
            (ItemKey::TrackArtist, "The Band"),
            (ItemKey::TrackTitle, title),
            (ItemKey::TrackNumber, number),
            (ItemKey::Genre, "Rock and Roll"),
        ] {
            t.insert_text(key, value.to_string());
        }
    };
    crate::retag::rewrite(path, set, |_| Ok(())).unwrap();
    crate::retag::set_bpm(path, Some(120)).unwrap();
}

fn made(root: &Path, library: &str, names: &[(&str, &str)], ext: &str) -> Vec<PathBuf> {
    names
        .iter()
        .enumerate()
        .map(|(n, (rel, title))| {
            let path = root.join(library).join(format!("{rel}.{ext}"));
            let dir = path.parent().unwrap();
            std::fs::create_dir_all(dir).unwrap();
            if ext == "flac" {
                crate::retag::fixtures::flac(dir, path.file_stem().unwrap().to_str().unwrap());
            } else {
                crate::retag::fixtures::mp3(&path);
                // Its ID3v1 tag with no genre: a zero there is Blues, and would be read beside whatever is written.
                let mut bytes = std::fs::read(&path).unwrap();
                *bytes.last_mut().unwrap() = 0xFF;
                std::fs::write(&path, bytes).unwrap();
            }
            tagged(&path, title, &(n + 1).to_string());
            path
        })
        .collect()
}

/// The record on disk, read and built. `shared`: both copies on one track each before the first build, by `set_tracks` (CR9);
/// else each comes with its own, and the build's take makes them one.
pub(super) fn two_copies(name: &str, shared: bool) -> Copies {
    let root = crate::retag::fixtures::dir(&format!("every-copy-{name}"));
    let (flacs, mp3s) = (made(&root, "music", &KEPT, "flac"), made(&root, "incoming", &WAITING, "mp3"));
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let cat = Arc::new(Catalogue::open(None, vec![root.join("music"), root.join("incoming")]).unwrap());
    cat.walk_all();
    cat.reread(&[flacs.clone(), mp3s.clone()].concat()).unwrap();
    cat.identify().unwrap();
    if shared {
        let tracks: Vec<_> = KEPT.iter().map(|(rel, _)| cat.track_of(0, &format!("{rel}.flac")).unwrap()).collect();
        let files: Vec<_> = WAITING.iter().zip(tracks).map(|((rel, _), track)| ((1, format!("{rel}.mp3")), Some(track))).collect();
        cat.set_tracks(&files).unwrap();
    }
    let rules = Rules { genre_aliases: vec![Alias { from: vec!["Rock and Roll".into()], to: "Rock".into() }], ..Rules::default() };
    let serve = Serve::open(Some(&data), rules, &|_| None).unwrap();
    serve.library.set_kinds(vec![(0, Kind::Main), (1, Kind::Incoming)]);
    serve.attach_catalogue(cat);
    serve.refresh().unwrap();
    let album = serve.library.album_in(0, "Kept").unwrap().expect("the copy that plays is built");
    assert_eq!(serve.library.copies_of(&album).unwrap().len(), 2, "one record, two copies");
    assert_eq!(serve.library.files_of_record(&album).unwrap().len(), 4);
    assert_eq!(serve.library.row_copy(&album).unwrap().unwrap().folder, 0);
    Copies { root, serve, album, flacs, mp3s }
}

/// An `App` with no server, for its journal and its runs.
pub(super) async fn app(tag: &str) -> Arc<App> {
    let d = crate::retag::fixtures::dir(&format!("every-copy-app-{tag}"));
    std::fs::create_dir_all(d.join(crate::playlists::PLAYLIST_DIR)).unwrap();
    let cfg = crate::config::Config::from_lookup(|_| None).unwrap();
    let saved = crate::settings::Saved { music_folder: Some(d.to_string_lossy().into_owned()), ..Default::default() };
    App::new(cfg, saved, crate::settings::Origin::Env, None).await.unwrap()
}

/// A proposal run to its end: its token. What it set aside is refused here, as every file should be written.
pub(super) async fn written(app: &Arc<App>, p: Proposal) -> String {
    let first = file_runs::start(app, p);
    for _ in 0..600 {
        // The list of runs holds every test's live runs: this one's own are in its journal.
        if !file_runs::runs(app, 0).unwrap().iter().any(|r| r.id == first.id) {
            let (id, _, run, _) = app.journal.file_runs_since(0).unwrap().into_iter().max_by_key(|r| r.0).unwrap();
            assert_eq!(run.failed, [], "every file written");
            return format!("{}{id}", file_runs::RUN);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the run did not end");
}

/// A run's Undo, to its end.
pub(super) async fn undone(app: &Arc<App>, token: &str) {
    let id: i64 = token[1..].parse().unwrap();
    file_runs::undo(app, token, None).await.unwrap();
    for _ in 0..600 {
        if app.journal.file_run(id).unwrap().unwrap().2 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the Undo did not end: {:?}", app.journal.file_run(id).unwrap());
}

/// A field's values in a file, as the tag reader reads them.
pub(super) fn tag(path: &Path, name: &str) -> Vec<String> {
    file_runs::fields_of(path, &[name]).unwrap().remove(name).unwrap()
}

fn paths(p: &Proposal) -> Vec<PathBuf> {
    let mut all: Vec<PathBuf> = p.files.iter().map(|f| f.path.clone()).collect();
    all.sort();
    all
}

/// A rule's fix of the record, as Health's sheet makes it.
fn rule_fix(c: &Copies, at: Option<&(i64, String)>) -> Proposal {
    let fix = super::fixes::rule_fix(&c.serve, &c.album, at).unwrap().expect("the rule names the genre otherwise");
    super::fix_plans::proposal(&c.serve, "fix:library:rules", &[fix], at).unwrap().0
}

/// Written, read back from each file, and put back: what every writer below must do.
async fn written_and_undone(tag_name: &str, app: &Arc<App>, c: &Copies, p: Proposal, want: &[&str]) {
    assert_eq!(paths(&p), c.files(), "every file of every copy");
    let was: Vec<Vec<String>> = c.files().iter().map(|f| tag(f, tag_name)).collect();
    let token = written(app, p).await;
    for f in c.files() {
        assert_eq!(tag(&f, tag_name), want, "{}", f.display());
    }
    undone(app, &token).await;
    assert_eq!(c.files().iter().map(|f| tag(f, tag_name)).collect::<Vec<_>>(), was, "as each file had it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fix_reaches_every_copy() {
    let (c, app) = (two_copies("fix", true), app("fix").await);
    let p = rule_fix(&c, None);
    assert_eq!(paths(&p), c.files());
    written(&app, p).await;
    for f in c.files() {
        assert_eq!(tag(&f, "genre"), ["Rock"], "{}", f.display());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn undo_takes_a_fix_out_of_every_copy() {
    let (c, app) = (two_copies("undo", true), app("undo").await);
    let token = written(&app, rule_fix(&c, None)).await;
    undone(&app, &token).await;
    for f in c.files() {
        assert_eq!(tag(&f, "genre"), ["Rock and Roll"], "{}", f.display());
    }
}

/// The "Before adding" sheet: the copy named, and nothing of the kept one.
#[tokio::test(flavor = "multi_thread")]
async fn a_fix_for_one_copy_stays_in_that_copy() {
    let (c, app) = (two_copies("one-copy", true), app("one-copy").await);
    let p = rule_fix(&c, Some(&(1, "Waiting".to_string())));
    let mut wanted = c.mp3s.clone();
    wanted.sort();
    assert_eq!(paths(&p), wanted);
    written(&app, p).await;
    assert!(c.flacs.iter().all(|f| tag(f, "genre") == ["Rock and Roll"]), "the kept copy left alone");
}

/// A track's own files come together, the file that plays first, read as that track whatever they are called.
#[test]
fn a_writers_files_are_every_copys_track_by_track() {
    let c = two_copies("order", true);
    let files = super::renumber::files_in(&c.serve, &c.album, None).unwrap();
    assert_eq!(files.iter().map(|t| (t.title.as_str(), t.file.as_str())).collect::<Vec<_>>(), [("One", "01 One.flac"), ("One", "one.mp3"), ("Two", "02 Two.flac"), ("Two", "two.mp3")]);
    assert_eq!(files[0].id, files[1].id);
    assert_ne!(files[0].id, files[2].id);
}

/// Renumber goes by track: each number reaches the track's file in every copy, and only that track's.
#[tokio::test(flavor = "multi_thread")]
async fn renumber_reaches_every_copy() {
    let (c, app) = (two_copies("renumber", true), app("renumber").await);
    let songs = c.songs();
    let preview = serde_json::json!({
        "from": "files",
        "totals": { "discs": 1, "tracks_per_disc": [5] },
        "release": {},
        "tracks": [{ "id": songs[0], "to": { "disc": 1, "track": 4 } }, { "id": songs[1], "to": { "disc": 1, "track": 5 } }],
    });
    let p = super::renumber::proposal(&c.serve, &c.album, &preview).unwrap();
    assert_eq!(paths(&p), c.files());
    let token = written(&app, p).await;
    let numbers = |files: &[PathBuf]| files.iter().map(|f| tag(f, "tracknumber")).collect::<Vec<_>>();
    assert_eq!(numbers(&c.flacs), [["4"], ["5"]]);
    assert_eq!(numbers(&c.mp3s), [["4"], ["5"]]);
    undone(&app, &token).await;
    assert_eq!(numbers(&c.flacs), [["1"], ["2"]]);
    assert_eq!(numbers(&c.mp3s), [["1"], ["2"]]);
}

/// A match was made on one copy's files, and the other copy calls them something else.
#[tokio::test(flavor = "multi_thread")]
async fn a_match_reaches_every_copy_by_its_track_not_its_file_name() {
    let (c, app) = (two_copies("match", true), app("match").await);
    let m = super::matching::Match {
        release: "11111111-1111-1111-1111-111111111111".into(),
        release_group: None,
        title: "Both Shores".into(),
        artist: "The Band".into(),
        artist_id: None,
        year: None,
        certain: true,
        why: "Its tracks".into(),
        tracks: vec![
            ("01 One.flac".into(), "22222222-2222-2222-2222-222222222222".into(), None),
            ("02 Two.flac".into(), "33333333-3333-3333-3333-333333333333".into(), None),
        ],
        lengths: Vec::new(),
    };
    let fix = super::matching::fix_of(&c.serve, "library:musicbrainz", &c.album, &m, None).unwrap().unwrap();
    let (p, _) = super::fix_plans::proposal(&c.serve, "fix:library:musicbrainz", &[fix], None).unwrap();
    assert_eq!(paths(&p), c.files());
    let token = written(&app, p).await;
    let ids = |files: &[PathBuf]| files.iter().map(|f| tag(f, "musicbrainz_trackid")).collect::<Vec<_>>();
    assert_eq!(ids(&c.mp3s), ids(&c.flacs), "each track's own recording, in both copies");
    assert_eq!(ids(&c.flacs), [["22222222-2222-2222-2222-222222222222"], ["33333333-3333-3333-3333-333333333333"]]);
    undone(&app, &token).await;
    assert!(c.files().iter().all(|f| tag(f, "musicbrainz_trackid").is_empty()));

    let fix = super::matching::fix_of(&c.serve, "library:musicbrainz", &c.album, &m, Some(&(1, "Waiting".to_string()))).unwrap().unwrap();
    assert_eq!(fix.writes.iter().map(|w| w.0.clone()).collect::<Vec<_>>(), c.mp3s, "one copy's alone, found by the track");
}

/// A tempo is the track's: Adjust BPM on it is every copy's.
#[tokio::test(flavor = "multi_thread")]
async fn a_tempo_reaches_every_copy() {
    let (c, app) = (two_copies("tempo", true), app("tempo").await);
    let (p, tempos) = super::tempo::proposal(&c.serve, &c.songs(), super::tempo::Adjust::Half).unwrap();
    assert_eq!(tempos.iter().map(|t| t.bpm).collect::<Vec<_>>(), [Some(60), Some(60)]);
    assert_eq!(p.title, "Tempo halved · 2 tracks", "tracks, not files");
    written_and_undone("bpm", &app, &c, p, &["60"]).await;
}

/// The album's facts into the files: its year, by the switch's sheet, on either kind of copy.
#[tokio::test(flavor = "multi_thread")]
async fn a_records_facts_reach_every_copy() {
    for shared in [true, false] {
        let (c, app) = (two_copies(&format!("facts-{shared}"), shared), app(&format!("facts-{shared}")).await);
        c.serve.library.db().execute("UPDATE albums SET year = 1999", []).unwrap();
        let p = super::file_facts::proposal(&c.serve, Some(std::slice::from_ref(&c.album))).unwrap();
        written_and_undone("date", &app, &c, p, &["1999"]).await;
    }
}

/// A file wrong in the copy that does not play is on the record's page all the same.
#[test]
fn a_copys_own_problem_is_the_records() {
    let c = two_copies("problem", true);
    c.serve.catalogue().issue_for_tests(1, "Waiting/two.mp3", "unreadable", "It stops early");
    let wrong = super::health::files_wrong(&c.serve, &c.album).unwrap();
    assert_eq!(wrong.len(), 1, "{wrong:?}");
    assert_eq!((wrong[0].0.as_str(), wrong[0].1.title.as_str(), wrong[0].1.files.as_slice()), ("unreadable", "Two", ["two.mp3".to_string()].as_slice()));
    assert_eq!(wrong[0].1.track.as_deref(), Some(c.songs()[1].as_str()));
}

/// A picture that decodes, as `covers` wants one, a shade each so two can be told apart.
pub(super) fn picture(shade: u8) -> Vec<u8> {
    let img = image::RgbImage::from_fn(16, 16, |x, y| image::Rgb([shade, x as u8 * 8, y as u8 * 8]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90).encode_image(&img).unwrap();
    out.into_inner()
}

/// The pictures a track holds, each as it lies.
fn inside(track: &Path) -> Vec<Vec<u8>> {
    use lofty::prelude::*;
    lofty::read_from_path(track).unwrap().tags().iter().flat_map(|t| t.pictures()).map(|p| p.data().to_vec()).collect()
}

fn kept_of(f: &crate::journal::FileWrite) -> &Path {
    f.whole.as_ref().unwrap().kept.as_ref().unwrap()
}

/// The kept copy with a cover of its own, beside its tracks and inside them, and the copy waiting with none: its picture.
fn the_kept_copy_has_a_cover(c: &Copies) -> Vec<u8> {
    let old = picture(10);
    std::fs::write(c.flacs[0].parent().unwrap().join("cover.jpg"), &old).unwrap();
    assert!(crate::covers_inside::replace(&c.flacs, Some((&old, "image/jpeg"))).is_empty());
    old
}

#[test]
fn a_cover_reaches_every_copy() {
    let c = two_copies("cover", true);
    the_kept_copy_has_a_cover(&c);
    let new = picture(200);
    let p = super::covers::proposal(&c.serve, &c.album, &new, "uploaded").unwrap();
    let dirs: Vec<&Path> = [&c.flacs[0], &c.mp3s[0]].map(|t| t.parent().unwrap()).to_vec();
    assert_eq!(p.files.iter().map(|f| f.path.as_path()).collect::<Vec<_>>(), dirs, "a file of the run a copy, the one that plays first");
    assert_eq!(p.files.iter().map(|f| f.before[0].1[0].as_str()).collect::<Vec<_>>(), ["its cover", "none"], "each as it stands");
    assert_ne!(kept_of(&p.files[0]), kept_of(&p.files[1]), "each copy's old pictures kept apart, however fast they are made");
    for f in &p.files {
        super::covers::write_kept(kept_of(f), || ()).unwrap();
    }
    for dir in dirs {
        assert_eq!(std::fs::read(dir.join("cover.jpg")).unwrap(), new, "{}", dir.display());
    }
    for t in c.files() {
        assert_eq!(inside(&t), [new.clone()], "the new picture alone: {}", t.display());
    }
}

/// Copies hold different pictures: each gets its own back, and a copy that had none has none.
#[test]
fn undo_takes_a_cover_out_of_every_copy() {
    let c = two_copies("cover-undo", true);
    let old = the_kept_copy_has_a_cover(&c);
    let p = super::covers::proposal(&c.serve, &c.album, &picture(200), "uploaded").unwrap();
    for f in &p.files {
        super::covers::write_kept(kept_of(f), || ()).unwrap();
    }
    for f in &p.files {
        super::covers::undo_kept(kept_of(f)).unwrap();
    }
    assert_eq!(std::fs::read(c.flacs[0].parent().unwrap().join("cover.jpg")).unwrap(), old);
    assert!(c.flacs.iter().all(|t| inside(t) == [old.clone()]), "the kept copy's own back");
    assert!(!c.mp3s[0].parent().unwrap().join("cover.jpg").exists(), "there was none beside");
    assert!(c.mp3s.iter().all(|t| inside(t).is_empty()), "there was none inside");
}

/// A copy's `dir` is only what its files share: what else lies under it is another record's, or nobody's.
#[test]
fn a_cover_goes_into_its_copys_own_files_alone() {
    let c = two_copies("cover-own", true);
    let dir = c.mp3s[0].parent().unwrap().join("Another Record");
    std::fs::create_dir_all(&dir).unwrap();
    crate::retag::fixtures::mp3(&dir.join("three.mp3"));
    let other = std::fs::read(dir.join("three.mp3")).unwrap();
    let p = super::covers::proposal(&c.serve, &c.album, &picture(200), "uploaded").unwrap();
    for f in &p.files {
        super::covers::write_kept(kept_of(f), || ()).unwrap();
    }
    assert_eq!(std::fs::read(dir.join("three.mp3")).unwrap(), other);
}

/// Health's cover fix: a file of the run a copy, and "Before adding"'s the copy named.
#[test]
fn a_cover_fix_is_a_write_a_copy() {
    let c = two_copies("cover-fix", true);
    let fix = super::fixes::Fix {
        id: format!("cover:{}", c.album),
        album: c.album.clone(),
        field: "cover",
        from: None,
        to: "https://coverartarchive.org/x.jpg".into(),
        source: "Cover Art Archive".into(),
        certainty: "certain",
        why: String::new(),
        writes: Vec::new(),
        candidates: Vec::new(),
    };
    let dir_of = |t: &PathBuf| t.parent().unwrap().to_path_buf();
    let every = super::fix_plans::proposal(&c.serve, "fix:library:cover", std::slice::from_ref(&fix), None).unwrap().0;
    assert_eq!(every.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>(), [dir_of(&c.flacs[0]), dir_of(&c.mp3s[0])]);
    assert_eq!(every.records.len(), 1, "one record");
    let one = super::fix_plans::proposal(&c.serve, "intake:al-1", &[fix], Some(&(1, "Waiting".to_string()))).unwrap().0;
    assert_eq!(one.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>(), [dir_of(&c.mp3s[0])]);
}

/// A length is one file's: the fix is of the file found wrong, whichever plays, and the track's other file is left alone.
#[test]
fn a_length_fix_stays_in_its_file() {
    for shared in [true, false] {
        let c = two_copies(&format!("length-{shared}"), shared);
        let (mp3, flac) = (&c.mp3s[1], &c.flacs[1]);
        let fix = super::length_fixes::fix(&c.serve, "length", 1, "Waiting/two.mp3").unwrap().expect("an MP3 that gives no length");
        let w = super::length_fixes::file_write(&c.serve, &fix).unwrap().unwrap();
        assert_eq!((&w.path, w.album.as_str()), (mp3, c.album.as_str()), "the file found wrong, not the row copy's");
        let (was, other) = (std::fs::read(mp3).unwrap(), std::fs::read(flac).unwrap());
        super::length_fixes::write("length", &w.path, None, |_| Ok(())).unwrap();
        assert_ne!(std::fs::read(mp3).unwrap(), was, "written");
        assert_eq!(std::fs::read(flac).unwrap(), other, "the other copy left alone");
    }
}
