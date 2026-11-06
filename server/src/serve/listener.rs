//! Who a stream is for, and the file that plays for them (`copies::copy_for`). Made once per request, before any blocking
//! work (a task handed work does not carry the request, `who`), and held through it: a stream, its waveform, its beats
//! and its gain are five requests a track, and a wind asks for up to 64 tracks at once.

use std::collections::HashMap;
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
    /// Their own music's catalogue folder.
    pub own: Option<i64>,
    /// The catalogue folders of the libraries they have off.
    pub off: Vec<i64>,
    /// The library picked by hand, by record.
    pub picks: HashMap<String, i64>,
    pub(super) library: Option<Arc<Library>>,
}

impl Listener {
    /// As `copy_for` takes them, for this record; none for no one in particular.
    pub(super) fn viewer(&self, album: &str) -> Option<Viewer> {
        self.account.as_ref().map(|_| Viewer { own: self.own, pick: self.picks.get(album).copied() })
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
        let id = if source == crate::source::cratarr::KIND.id { item.to_string() } else { library.song_ids(cat.track_of_source(source, item)?).ok()??.song_id };
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
mod tests {
    use super::*;
    use crate::catalogue::Rules;
    use crate::serve::copies::Kind;
    use crate::source::cratarr::KIND;

    const FLAC: &str = "A/01 One.flac";
    const MP3: &str = "B/01 One.mp3";

    /// One track with a file in each of two folders, as the take will leave it: the catalogue gives both one track and
    /// the library's `copy_files` and `copies` name one album. Folder 0 holds a FLAC, folder 1 an MP3 with its own numbers.
    struct Two {
        roots: [std::path::PathBuf; 2],
        cat: Catalogue,
        lib: Arc<Library>,
        song: String,
        album: String,
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
        Two { roots, cat, lib, song, album }
    }

    impl Two {
        fn who(&self, account: Option<&str>, own: Option<i64>, off: &[i64]) -> Listener {
            Listener { account: account.map(str::to_string), own, off: off.to_vec(), picks: HashMap::new(), library: Some(self.lib.clone()) }
        }

        fn plays(&self, who: &Listener) -> Option<(i64, String)> {
            who.place(&self.cat, KIND.id, &self.song)
        }
    }

    fn at(folder: i64) -> Option<(i64, String)> {
        Some((folder, if folder == 0 { FLAC } else { MP3 }.to_string()))
    }

    #[test]
    fn a_stream_plays_the_copy_copy_for_chose() {
        let t = two("chose");
        assert_eq!(t.plays(&t.who(None, None, &[])), at(0), "no one: the row copy");
        assert_eq!(t.plays(&t.who(Some("ann"), None, &[])), at(0), "the best kept copy");
        assert_eq!(t.plays(&t.who(Some("ann"), Some(1), &[])), at(1), "their own music, at a lower quality");
        assert_eq!(t.plays(&t.who(Some("ann"), None, &[0])), at(1), "the other copy is off");
        let mut picked = t.who(Some("ann"), None, &[]);
        picked.picks.insert(t.album.clone(), 1);
        assert_eq!(t.plays(&picked), at(1));
        picked.account = None;
        assert_eq!(t.plays(&picked), at(0), "no one picked anything");

        let (path, kind, ms) = crate::stream::locate(&t.cat, &t.who(Some("ann"), Some(1), &[]), KIND.id, &t.song).unwrap();
        assert_eq!((path, matches!(kind, Some(crate::stream::Kind::Mp3)), ms), (t.roots[1].join(MP3), true, Some(200_000)));
        let (path, kind, _) = crate::stream::locate(&t.cat, &t.who(None, None, &[]), KIND.id, &t.song).unwrap();
        assert_eq!((path, matches!(kind, Some(crate::stream::Kind::Flac))), (t.roots[0].join(FLAC), true));
        let through = [(KIND.id.to_string(), t.song.clone()), (KIND.id.to_string(), t.song.clone())];
        let joins = crate::stream::locate_through(&t.cat, &t.who(Some("ann"), Some(1), &[]), &through);
        assert_eq!(joins, [(t.roots[1].join(MP3), Some(200_000)), (t.roots[1].join(MP3), Some(200_000))], "a wind goes through the same file");
    }

    #[test]
    fn a_copy_lacking_the_track_is_passed_over() {
        let t = two("lacking");
        t.lib.db().execute("DELETE FROM copy_files WHERE folder = 1", []).unwrap();
        assert_eq!(t.plays(&t.who(Some("ann"), Some(1), &[])), at(0));
    }

    /// Home's crates play every library, and a stream cannot tell where it came from: with no copy for this listener the
    /// row copy plays, as it does today (Q3, N8).
    #[test]
    fn where_nothing_plays_for_them_the_row_copy_does() {
        let t = two("nothing");
        assert_eq!(t.plays(&t.who(Some("ann"), None, &[0, 1])), at(0));
    }

    /// A file walked since the last build has no row in the library: it is there, and a stream must not lose it.
    #[test]
    fn a_track_with_one_file_needs_no_word_from_the_library() {
        let t = two("one");
        t.cat.set_tracks(&[((1, MP3.to_string()), None)]).unwrap();
        t.lib.db().execute_batch("DELETE FROM copy_files; DELETE FROM copies; DELETE FROM songs;").unwrap();
        assert_eq!(t.plays(&t.who(Some("ann"), Some(1), &[0])), at(0));
    }

    #[test]
    fn another_servers_item_names_its_own_file() {
        let t = two("server");
        t.cat.record_links("jf", "music", &[("j-1".into(), 1, MP3.into())], 1).unwrap();
        assert_eq!(t.cat.track_of_source("jf", "j-1"), ids::track_of(&t.song));
        assert_eq!(t.who(None, None, &[]).place(&t.cat, "jf", "j-1"), at(1), "not the lowest folder's, nor the copy that plays");
        assert_eq!(t.who(None, None, &[]).place(&t.cat, "jf", "j-unknown"), None);
    }

    #[test]
    fn the_gain_is_the_file_that_plays() {
        let t = two("gain");
        let gain = |who: &Listener| who.song(&t.cat, KIND.id, &t.song).and_then(|s| s.replay_gain).and_then(|g| g.track_gain);
        assert_eq!(gain(&t.who(None, None, &[])), Some(-7.0));
        assert_eq!(gain(&t.who(Some("ann"), Some(1), &[])), Some(-3.0), "an MP3 is levelled by its own gain, not the FLAC's");
        let song = t.who(Some("ann"), Some(1), &[]).song(&t.cat, KIND.id, &t.song).unwrap();
        assert_eq!((song.suffix.as_str(), song.bit_rate, song.bpm, song.content_type.as_str()), ("mp3", Some(192), Some(121), "audio/mpeg"));
        let song = t.who(None, None, &[]).song(&t.cat, KIND.id, &t.song).unwrap();
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
        let (row, own) = (t.who(None, None, &[]), t.who(Some("ann"), Some(1), &[]));
        let (tag_row, bins_row) = wave(&row, None);
        let (tag_own, bins_own) = wave(&own, None);
        assert_eq!((bins_row.unwrap()[0], bins_own.unwrap()[0]), (1, 2));
        assert_ne!(tag_row, tag_own, "a browser that kept one file's must not be told it is still good for the other");
        assert_eq!(wave(&own, Some(&tag_own)).1, None, "not modified");
        assert!(wave(&own, Some(&tag_row)).1.is_some());
    }}
