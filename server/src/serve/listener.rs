//! Who a stream is for, and the file that plays for them (`copies::copy_for`). Made once per request, before any blocking
//! work (a task handed work does not carry the request, `who`), and held through it: a stream, its waveform, its beats
//! and its gain are five requests a track, and a wind asks for up to 64 tracks at once.

use std::sync::Arc;

use super::api::Song;
use super::copies::Viewer;
use super::ids;
use super::library::Library;
use crate::catalogue::Catalogue;

/// Who asks. The default is no one, with no library to ask: what the app's tests without one have, and any file will do.
#[derive(Clone, Default)]
pub struct Listener {
    /// None: no one in particular (a signed link while signing in is required, work of its own): the row copy.
    pub account: Option<String>,
    /// The catalogue folders of the libraries they have off.
    pub off: Vec<i64>,
    /// The library this one request asks to hear, whoever asks: an intake card's snippets are of the copy waiting, and a queue
    /// started from a copy asks for it on every stream. It lasts as long as the request.
    pub from: Option<i64>,
    pub(super) library: Option<Arc<Library>>,
}

impl Listener {
    /// As `copy_for` takes it: none for no one in particular, unless the request named a copy.
    pub fn viewer(&self) -> Option<Viewer> {
        (self.account.is_some() || self.from.is_some()).then_some(Viewer { from: self.from })
    }

    /// The file a server's item is for this listener: Cratarr's own id names a track, and they hear its copy that plays;
    /// another server's item names one file. Blocking.
    pub fn place(&self, cat: &Catalogue, source: &str, item: &str) -> Option<(i64, String)> {
        if source != crate::source::cratarr::KIND.id {
            return cat.place_of_source(source, item);
        }
        let track = cat.track_of_source(source, item)?;
        self.among(track, cat.files_of(track))
    }

    /// Whether what this listener hears of a server's item is one of `files`: a record's copies hold one track, so the
    /// track alone cannot say. Not known counts as yes, which stops a player and never leaves one on a file that goes.
    /// Blocking.
    pub fn hears_one_of(&self, cat: &Catalogue, source: &str, item: &str, files: &[(i64, String)]) -> bool {
        self.place(cat, source, item).is_none_or(|heard| files.contains(&heard))
    }

    /// Which of a track's present files plays for this listener. With one, that one: whichever way `copy_for` went, it is
    /// the only file there is, so most streams never wait on the library. Blocking.
    pub fn among(&self, track: i64, mut files: Vec<(i64, String)>) -> Option<(i64, String)> {
        match &self.library {
            Some(library) if files.len() > 1 => {
                let song = ids::song(track);
                library.file_for(&song, &files, self).unwrap_or_else(|e| {
                    tracing::warn!(target: "library", song = %song, reason = %format!("{e:#}"), "which copy plays not read: the first file");
                    files.first().cloned()
                })
            }
            _ if files.is_empty() => None,
            _ => Some(files.swap_remove(0)),
        }
    }

    /// A track as the library serves it, with the numbers of the file that plays for this listener: its gains, tempo and
    /// format are that file's, not the row copy's. Blocking.
    pub fn song(&self, cat: &Catalogue, source: &str, item: &str) -> Option<Song> {
        let library = self.library.as_ref()?;
        // An id a take retired names the track that took it: a queue can hold one.
        let id = if source == crate::source::cratarr::KIND.id { cat.track_of_source(source, item).map_or(item.to_string(), ids::song) } else { library.song_ids(cat.track_of_source(source, item)?).ok()??.song_id };
        let mut song = library.songs_by_id(&[id]).ok()?.into_iter().next()?;
        if let Some((folder, rel)) = self.place(cat, source, item)
            && let Err(e) = library.as_played(&mut song, folder, &rel)
        {
            tracing::warn!(target: "library", song = %song.id, reason = %format!("{e:#}"), "the numbers of the file that plays not read");
        }
        Some(song)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::catalogue::Rules;
    use crate::serve::copies::Kind;
    use crate::source::cratarr::KIND;

    const FLAC: &str = "A/01 One.flac";
    const MP3: &str = "B/01 One.mp3";

    /// One track with a file in each of two folders, as the take leaves it: the catalogue gives both one track and the
    /// library's `copy_files` and `copies` name one album. Folder 0 holds a FLAC, folder 1 an MP3 with its own numbers.
    struct Two {
        roots: [std::path::PathBuf; 2],
        cat: Catalogue,
        lib: Arc<Library>,
        song: String,
    }

    impl Drop for Two {
        fn drop(&mut self) {
            for r in &self.roots {
                std::fs::remove_dir_all(r).ok();
            }
        }
    }

    fn two(name: &str) -> Two {
        let root = |n: u8| std::env::temp_dir().join(format!("cratarr-listener-{name}-{n}-{}", std::process::id()));
        let roots = [root(0), root(1)];
        for (r, rel) in roots.iter().zip([FLAC, MP3]) {
            std::fs::remove_dir_all(r).ok();
            std::fs::create_dir_all(r.join(rel).parent().unwrap()).unwrap();
            std::fs::write(r.join(rel), rel).unwrap();
        }
        let cat = Catalogue::open(None, roots.to_vec()).unwrap();
        cat.walk_all();
        cat.pretend_read_for_tests();
        for (folder, rel) in [(0, FLAC), (1, MP3)] {
            cat.tag_for_tests(folder, rel, &[("album", "X"), ("albumartist", "A"), ("title", "One")]);
        }
        cat.identify().unwrap();
        let lib = Arc::new(Library::open(None).unwrap());
        lib.set_kinds(vec![(0, Kind::Main), (1, Kind::Main)]);
        let (_, print) = lib.change(&cat, &Rules::default()).unwrap();
        lib.build_from(&cat, &Rules::default(), &print).unwrap();
        let (song, album): (String, String) = lib.db().query_row("SELECT song, album FROM copy_files WHERE folder = 0", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        lib.db()
            .execute_batch(&format!(
                "UPDATE copies SET format = 'FLAC', min_kbps = 900, bits = 16, rate = 44100 WHERE folder = 0;
                 UPDATE copies SET album = '{album}', format = 'MP3', min_kbps = 192, bits = NULL, rate = 44100 WHERE folder = 1;
                 UPDATE copy_files SET rg_track_gain = -7.0, rg_track_peak = 0.9, bpm = 120, suffix = 'flac', kbps = 900 WHERE folder = 0;
                 UPDATE copy_files SET song = '{song}', album = '{album}', rg_track_gain = -3.0, rg_track_peak = 0.5, bpm = 121, suffix = 'mp3', kbps = 192 WHERE folder = 1;"
            ))
            .unwrap();
        cat.set_tracks(&[((1, MP3.to_string()), ids::track_of(&song))]).unwrap();
        Two { roots, cat, lib, song }
    }

    impl Two {
        fn who(&self, account: Option<&str>, off: &[i64]) -> Listener {
            Listener { account: account.map(str::to_string), off: off.to_vec(), library: Some(self.lib.clone()), ..Listener::default() }
        }

        fn plays(&self, who: &Listener) -> Option<(i64, String)> {
            who.place(&self.cat, KIND.id, &self.song)
        }
    }

    fn at(folder: i64) -> Option<(i64, String)> {
        Some((folder, if folder == 0 { FLAC } else { MP3 }.to_string()))
    }

    /// A queue can hold an id a take retired: it still names the song, and what is heard of it is what plays for the listener.
    #[test]
    fn an_id_a_take_retired_names_the_track_that_took_it() {
        let t = two("taken");
        let old = ids::song(*t.cat.moved_tracks().unwrap().keys().next().expect("one of the two was taken as the library was built"));
        assert_ne!(old, t.song);
        let who = t.who(Some("ann"), &[0]);
        assert_eq!(who.song(&t.cat, KIND.id, &old).map(|s| s.id), Some(t.song.clone()));
        assert_eq!(who.place(&t.cat, KIND.id, &old), at(1));
    }

    #[test]
    fn a_stream_plays_the_copy_copy_for_chose() {
        let t = two("chose");
        assert_eq!(t.plays(&t.who(None, &[])), at(0), "no one: the row copy");
        assert_eq!(t.plays(&t.who(Some("ann"), &[])), at(0), "the best kept copy");
        assert_eq!(t.plays(&t.who(Some("ann"), &[0])), at(1), "the other copy is off");

        let (path, kind, ms) = crate::stream::locate(&t.cat, &t.who(Some("ann"), &[0]), KIND.id, &t.song).unwrap();
        assert_eq!((path, matches!(kind, Some(crate::stream::Kind::Mp3)), ms), (t.roots[1].join(MP3), true, Some(200_000)));
        let (path, kind, _) = crate::stream::locate(&t.cat, &t.who(None, &[]), KIND.id, &t.song).unwrap();
        assert_eq!((path, matches!(kind, Some(crate::stream::Kind::Flac))), (t.roots[0].join(FLAC), true));
        let through = [(KIND.id.to_string(), t.song.clone()), (KIND.id.to_string(), t.song.clone())];
        let joins = crate::stream::locate_through(&t.cat, &t.who(Some("ann"), &[0]), &through);
        assert_eq!(joins, [(t.roots[1].join(MP3), Some(200_000)), (t.roots[1].join(MP3), Some(200_000))], "a wind goes through the same file");
    }

    /// CR17: what an account hears goes by the files alone, so the same library makes the same choice for anyone, and an own copy
    /// of a lower quality is no reason to hear it.
    #[test]
    fn an_account_hears_the_better_copy_though_the_other_is_its_own() {
        let t = two("own");
        t.lib.set_kinds(vec![(0, Kind::Main), (1, Kind::Own)]);
        assert_eq!(t.plays(&t.who(Some("ann"), &[])), at(0), "a FLAC in Music over the MP3 of their own");
        assert_eq!(t.plays(&t.who(Some("ann"), &[0])), at(1), "their own, with Music off");
        t.lib.db().execute_batch("UPDATE copies SET format = 'MP3', min_kbps = 128, bits = NULL WHERE folder = 0; UPDATE copies SET format = 'FLAC', min_kbps = 900, bits = 16 WHERE folder = 1").unwrap();
        assert_eq!(t.plays(&t.who(Some("ann"), &[])), at(1), "their own, the better now");
        assert_eq!(t.plays(&t.who(None, &[])), at(0), "for no one the row copy: one user's FLAC is not what Music shows");
    }

    /// A library of one folder on the disk, of these files: each (its path, its album, its title).
    pub(crate) fn built(name: &str, files: &[(&str, &str, &str)]) -> (std::path::PathBuf, Catalogue, Library) {
        let root = std::env::temp_dir().join(format!("cratarr-listener-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        for (rel, ..) in files {
            std::fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
            std::fs::write(root.join(rel), rel).unwrap();
        }
        let cat = Catalogue::open(None, vec![root.clone()]).unwrap();
        cat.walk_all();
        cat.pretend_read_for_tests();
        for (rel, album, title) in files {
            cat.tag_for_tests(0, rel, &[("album", album), ("albumartist", "A"), ("title", title)]);
        }
        cat.identify().unwrap();
        let lib = Library::open(None).unwrap();
        lib.set_kinds(vec![(0, Kind::Main)]);
        let (_, print) = lib.change(&cat, &Rules::default()).unwrap();
        lib.build_from(&cat, &Rules::default(), &print).unwrap();
        (root, cat, lib)
    }

    /// `albums.dir` is what a copy's files share. Over two folders of an artist it is the artist's, and a Delete of that folder
    /// would take every other album in it: only what is the copy's alone may go whole.
    #[test]
    fn a_dir_holding_another_records_audio_is_refused() {
        let (root, _cat, lib) = built(
            "alone",
            &[("Artist/Live/1.flac", "Spread", "One"), ("Artist/Aside/1.flac", "Lone", "Two"), ("Artist/Studio/1.flac", "Spread", "Three"), ("Box/CD1/1.flac", "Set", "Four"), ("Box/CD2/1.flac", "Set", "Five")],
        );
        let album = |dir: &str| lib.album_in(0, dir).unwrap().unwrap();
        let all = ["Artist/Live/1.flac", "Artist/Aside/1.flac", "Artist/Studio/1.flac"].map(String::from);
        let spread = album("Artist");
        assert!(!lib.copy_holds_all(&spread, 0, "Artist", &all).unwrap(), "the other album lies under it, and in a copy of its own folder");
        let (only_its_own, set) = (["Artist/Live/1.flac", "Artist/Studio/1.flac"].map(String::from), album("Box"));
        assert!(!lib.copy_holds_all(&spread, 0, "Artist", &only_its_own).unwrap(), "another copy lies inside it, whatever the disk was seen to hold");
        assert!(lib.copy_holds_all(&set, 0, "Box", &["Box/CD1/1.flac", "Box/CD2/1.flac"].map(String::from)).unwrap(), "two discs of one copy are that copy");
        assert!(!lib.copy_holds_all(&set, 0, "Box", &["Box/CD1/1.flac", "Box/CD3/new.flac"].map(String::from)).unwrap(), "a file not built yet is no file of it");
        std::fs::remove_dir_all(&root).ok();
    }

    /// An intake card's snippets are of the copy waiting, which by `copy_for` plays for no one, and a Play from's queue of the copy
    /// it started from: the stream names it, for that stream alone, and a library off still wins.
    #[test]
    fn a_stream_that_names_a_copy_hears_that_copy_while_its_library_is_on() {
        let t = two("named");
        let named = |account: Option<&str>, off: &[i64]| Listener { from: Some(1), ..t.who(account, off) };
        assert_eq!(t.plays(&t.who(Some("ann"), &[])), at(0), "named by no one: the best");
        assert_eq!(t.plays(&named(Some("ann"), &[])), at(1));
        assert_eq!(t.plays(&named(None, &[])), at(1), "no account to go by, and it is still what was asked");
        assert_eq!(t.plays(&named(Some("ann"), &[1])), at(0), "a copy in a library off never plays");
    }

    /// Both copies hold the track that plays. By the track a Delete of the copy no one hears stops the player, and it never
    /// starts again; by the file, only the copy they hear does.
    #[test]
    fn deleting_a_copy_not_playing_leaves_the_player() {
        let t = two("leaves");
        let (flac, mp3) = (vec![(0, FLAC.to_string())], vec![(1, MP3.to_string())]);
        let on_the_flac = t.who(Some("ann"), &[]);
        assert!(!on_the_flac.hears_one_of(&t.cat, KIND.id, &t.song, &mp3), "the copy going is not the one they hear");
        assert!(on_the_flac.hears_one_of(&t.cat, KIND.id, &t.song, &flac));
        let on_the_mp3 = t.who(Some("ann"), &[0]);
        assert!(on_the_mp3.hears_one_of(&t.cat, KIND.id, &t.song, &mp3), "as each hears it");
        assert!(!on_the_mp3.hears_one_of(&t.cat, KIND.id, &t.song, &flac));
        assert!(on_the_flac.hears_one_of(&t.cat, KIND.id, "tr-999999", &mp3), "not known: stopped, the safe side");
    }

    #[test]
    fn a_copy_lacking_the_track_is_passed_over() {
        let t = two("lacking");
        t.lib.db().execute("DELETE FROM copy_files WHERE folder = 0", []).unwrap();
        assert_eq!(t.plays(&t.who(Some("ann"), &[])), at(1), "the better copy, were it there");
    }

    /// Home's crates play every library, and a stream cannot tell where it came from: with no copy for this listener the
    /// row copy plays, as it does today (Q3, N8).
    #[test]
    fn where_nothing_plays_for_them_the_row_copy_does() {
        let t = two("nothing");
        assert_eq!(t.plays(&t.who(Some("ann"), &[0, 1])), at(0));
    }

    /// A file walked since the last build has no row in the library: it is there, and a stream must not lose it.
    #[test]
    fn a_track_with_one_file_needs_no_word_from_the_library() {
        let t = two("one");
        t.cat.set_tracks(&[((1, MP3.to_string()), None)]).unwrap();
        t.lib.db().execute_batch("DELETE FROM copy_files; DELETE FROM copies; DELETE FROM songs;").unwrap();
        assert_eq!(t.plays(&t.who(Some("ann"), &[0])), at(0));
    }

    #[test]
    fn another_servers_item_names_its_own_file() {
        let t = two("server");
        t.cat.record_links("jf", "music", &[("j-1".into(), 1, MP3.into())], 1).unwrap();
        assert_eq!(t.cat.track_of_source("jf", "j-1"), ids::track_of(&t.song));
        assert_eq!(t.who(None, &[]).place(&t.cat, "jf", "j-1"), at(1), "not the lowest folder's, nor the copy that plays");
        assert_eq!(t.who(None, &[]).place(&t.cat, "jf", "j-unknown"), None);
    }

    #[test]
    fn the_gain_is_the_file_that_plays() {
        let t = two("gain");
        let gain = |who: &Listener| who.song(&t.cat, KIND.id, &t.song).and_then(|s| s.replay_gain).and_then(|g| g.track_gain);
        assert_eq!(gain(&t.who(None, &[])), Some(-7.0));
        assert_eq!(gain(&t.who(Some("ann"), &[0])), Some(-3.0), "an MP3 is levelled by its own gain, not the FLAC's");
        let song = t.who(Some("ann"), &[0]).song(&t.cat, KIND.id, &t.song).unwrap();
        assert_eq!((song.suffix.as_str(), song.bit_rate, song.bpm, song.content_type.as_str()), ("mp3", Some(192), Some(121), "audio/mpeg"));
        let song = t.who(None, &[]).song(&t.cat, KIND.id, &t.song).unwrap();
        assert_eq!((song.suffix.as_str(), song.bit_rate, song.bpm), ("flac", Some(900), Some(120)));
    }

    #[test]
    fn the_waveform_is_the_file_that_plays() {
        let t = two("wave");
        for (folder, rel, level) in [(0, FLAC, 1u8), (1, MP3, 2)] {
            let a = crate::analysis::Analysis {
                version: crate::analysis::VERSION,
                duration_secs: 200.0,
                loudness_lufs: -14.0,
                track_gain_db: -4.0,
                track_peak: 0.9,
                blocks: crate::analysis::Loudness(vec![(1, 40)]),
                bpm: Some(120.0),
                bpm_confidence: 0.9,
                waveform: vec![level; crate::analysis::WAVEFORM_BINS],
                complete: Some(true),
                beats: None,
                rhythm: None,
            };
            let (_, size, mtime) = t.cat.file_to_analyse(folder, rel).unwrap();
            t.cat.store_analysis(folder, rel, size, mtime, &a).unwrap();
        }
        let wave = |who: &Listener, asked: Option<&str>| crate::api::waveform_for(&t.cat, who, KIND.id, &t.song, asked).unwrap();
        let (row, other) = (t.who(None, &[]), t.who(Some("ann"), &[0]));
        let (tag_row, bins_row) = wave(&row, None);
        let (tag_other, bins_other) = wave(&other, None);
        assert_eq!((bins_row.unwrap()[0], bins_other.unwrap()[0]), (1, 2));
        assert_ne!(tag_row, tag_other, "a browser that kept one file's must not be told it is still good for the other");
        assert_eq!(wave(&other, Some(&tag_other)).1, None, "not modified");
        assert!(wave(&other, Some(&tag_row)).1.is_some());
    }}
