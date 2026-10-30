//! One import (decision 0067): what a source brings, found in the library the same way for every source, and what an
//! import would do or did, said the same way. A source reads what it has (a listening service its listens, a server
//! its counts and stars) and hands each item's track over as a [`Track`]; [`Matcher`] finds it, and [`Tally`] keeps the
//! count and what was not found.
//!
//! Checked against beets (its Last.fm import and its autotagger's string distance, the owner 2026-10-24): the same order
//! (recording, then artist or album with the title), and a second, looser try as beets' weighting reads names: accents
//! folded, "&" as "and", what is in brackets, a "feat." part and a leading "the" left out. No substring guesses: beets
//! falls back to them, and they find the wrong "Intro".

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

/// A track as a source knows it.
#[derive(Debug, Clone, Default)]
pub struct Track<'a> {
    /// MusicBrainz's recording, where the source knows it.
    pub recording: Option<&'a str>,
    pub artist: &'a str,
    pub title: &'a str,
    pub album: Option<&'a str>,
    pub length_secs: Option<f64>,
}

/// A name to match by: lower case, letters and digits only.
pub fn key(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// A name to match by when its own key finds nothing: accents folded, "&" as "and", without what is in brackets, a
/// "feat." part or a leading "the" ("Beyoncé feat. Jay-Z" is "beyonce"; "The Wall (Remastered)" is "wall").
pub fn loose(s: &str) -> String {
    let lower: String = s.chars().map(fold).collect::<String>().to_lowercase().replace('&', " and ");
    let mut kept = String::new();
    let mut depth = 0u32;
    for c in lower.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => kept.push(c),
            _ => {}
        }
    }
    let words: Vec<&str> = kept.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let end = words.iter().position(|w| matches!(*w, "feat" | "ft" | "featuring")).unwrap_or(words.len());
    let start = usize::from(words.first() == Some(&"the") && end > 1);
    let out: String = words[start..end].concat();
    if out.is_empty() { key(s) } else { out }
}

/// A letter without its accent (the Latin ones; others as they are).
fn fold(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' => 'A',
        'ç' | 'ć' | 'č' => 'c',
        'Ç' | 'Ć' | 'Č' => 'C',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ė' | 'Ę' | 'Ě' => 'E',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' => 'i',
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ī' | 'Į' => 'I',
        'ñ' | 'ń' | 'ň' => 'n',
        'Ñ' | 'Ń' | 'Ň' => 'N',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' | 'Ő' => 'O',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ū' | 'Ů' | 'Ű' => 'U',
        'ý' | 'ÿ' => 'y',
        'Ý' | 'Ÿ' => 'Y',
        'š' | 'ś' => 's',
        'Š' | 'Ś' => 'S',
        'ž' | 'ź' | 'ż' => 'z',
        'Ž' | 'Ź' | 'Ż' => 'Z',
        'ł' => 'l',
        'Ł' => 'L',
        'ř' => 'r',
        'Ř' => 'R',
        'đ' => 'd',
        'Đ' => 'D',
        'ß' => 's',
        '’' | '‘' | '`' => '\'',
        c => c,
    }
}

/// A song of the library, as a candidate: its id and length (seconds).
type Candidate = (String, Option<f64>);

/// The library's songs, to find a source's tracks in: by recording, by artist and title, by album and title (each by
/// its own key, then loosely); the length picks among several of one name (a live take beside the album's). And its
/// albums, by name and artist.
#[derive(Default)]
pub struct Matcher {
    by_recording: HashMap<String, String>,
    by_artist: HashMap<(String, String), Vec<Candidate>>,
    by_album: HashMap<(String, String), Vec<Candidate>>,
    loose_artist: HashMap<(String, String), Vec<Candidate>>,
    loose_album: HashMap<(String, String), Vec<Candidate>>,
    /// Albums by (name, album artist), then loosely; and by name alone, where only one album has it.
    albums: HashMap<(String, String), String>,
    loose_albums: HashMap<(String, String), String>,
    album_names: HashMap<String, Vec<String>>,
}

impl Matcher {
    /// The library's songs, from Cratarr's catalogue (`library.sqlite`).
    pub fn read(library: &Path) -> Result<Self> {
        Self::read_songs(library, true)
    }

    /// The library's albums alone, as [`Matcher::find_album`] reads them; it finds no track (`find`). What knowing
    /// whether the library has an album needs: one entry an album, where the songs' four maps (some 70 MB on the NAS's
    /// library) have four a song (0.11.1: /api/discover's memory, 2026-10-30).
    pub fn read_albums(library: &Path) -> Result<Self> {
        Self::read_songs(library, false)
    }

    /// The same rows in the same order either way, so `find_album` answers alike from both.
    fn read_songs(library: &Path, songs: bool) -> Result<Self> {
        let conn = crate::db::open_read_only(library)?;
        let mut st = conn.prepare("SELECT id, title, artist, album, mbid, duration_ms, album_id, album_artist FROM songs")?;
        let mut m = Matcher::default();
        let rows = st.query_map([], |r| {
            Ok((
                (r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?),
                (r.get::<_, Option<String>>(4)?, r.get::<_, Option<i64>>(5)?, r.get::<_, String>(6)?, r.get::<_, Option<String>>(7)?),
            ))
        })?;
        for row in rows {
            let ((id, title, artist, album), (mbid, ms, album_id, album_artist)) = row?;
            if songs {
                m.add(&id, &title, artist.as_deref(), &album, mbid.as_deref(), ms.map(|ms| ms as f64 / 1000.0));
            }
            m.add_album(&album_id, &album, album_artist.as_deref().or(artist.as_deref()));
        }
        Ok(m)
    }

    /// The library's albums (`read_albums`), one reading shared (0.11.1: /api/discover's memory, 2026-10-30: twelve
    /// Discover requests at once each read the whole library's songs, twice, and filled the NAS's 768 MB). Those asking
    /// at once wait for one reading; it is kept while the library is not built again (`built`: `Serve::built_at`), a
    /// minute at most. Without `built`, read now and not kept. Blocking.
    pub fn library_albums(library: &Path, built: Option<i64>) -> Result<std::sync::Arc<Self>> {
        static KEPT: std::sync::Mutex<KeptAlbums> = std::sync::Mutex::new(None);
        match built {
            Some(built) => kept_or_read(&KEPT, library, built, super::moment::now_secs(), || Self::read_albums(library)),
            None => Ok(std::sync::Arc::new(Self::read_albums(library)?)),
        }
    }

    pub fn add(&mut self, id: &str, title: &str, artist: Option<&str>, album: &str, mbid: Option<&str>, length_secs: Option<f64>) {
        if let Some(m) = mbid.filter(|m| !m.is_empty()) {
            self.by_recording.entry(m.to_lowercase()).or_insert_with(|| id.to_string());
        }
        if let Some(a) = artist {
            self.by_artist.entry((key(a), key(title))).or_default().push((id.to_string(), length_secs));
        }
        self.by_album.entry((key(album), key(title))).or_default().push((id.to_string(), length_secs));
        if let Some(a) = artist {
            self.loose_artist.entry((loose(a), loose(title))).or_default().push((id.to_string(), length_secs));
        }
        self.loose_album.entry((loose(album), loose(title))).or_default().push((id.to_string(), length_secs));
    }

    /// An album, once per song of it.
    pub fn add_album(&mut self, id: &str, name: &str, artist: Option<&str>) {
        let artist = artist.unwrap_or_default();
        if self.albums.insert((key(name), key(artist)), id.to_string()).is_none() {
            let same = self.album_names.entry(key(name)).or_default();
            if !same.iter().any(|x| x == id) {
                same.push(id.to_string());
            }
        }
        self.loose_albums.entry((loose(name), loose(artist))).or_insert_with(|| id.to_string());
    }

    /// The album a source names: by name and artist, then loosely, then by name alone where only one has it.
    pub fn find_album(&self, name: &str, artist: Option<&str>) -> Option<&str> {
        let artist = artist.unwrap_or_default();
        self.albums
            .get(&(key(name), key(artist)))
            .or_else(|| self.loose_albums.get(&(loose(name), loose(artist))))
            .map(String::as_str)
            .or_else(|| self.album_names.get(&key(name)).filter(|a| a.len() == 1).map(|a| a[0].as_str()))
    }

    /// The song a source's track is: its recording's, else its artist's and title's, else its album's and title's; of
    /// several, the one nearest its length (the first, without one).
    pub fn find(&self, t: &Track) -> Option<&str> {
        t.recording
            .and_then(|r| self.by_recording.get(&r.to_lowercase()).map(String::as_str))
            .or_else(|| self.by_artist.get(&(key(t.artist), key(t.title))).and_then(|c| nearest(c, t.length_secs)))
            .or_else(|| t.album.and_then(|a| self.by_album.get(&(key(a), key(t.title)))).and_then(|c| nearest(c, t.length_secs)))
            .or_else(|| self.loose_artist.get(&(loose(t.artist), loose(t.title))).and_then(|c| nearest(c, t.length_secs)))
            .or_else(|| t.album.and_then(|a| self.loose_album.get(&(loose(a), loose(t.title)))).and_then(|c| nearest(c, t.length_secs)))
    }
}

/// Of several songs of one name, the one nearest `length` (the first, without one).
fn nearest(c: &[Candidate], length: Option<f64>) -> Option<&str> {
    let by = |x: &Candidate| match (x.1, length) {
        (Some(a), Some(b)) => (a - b).abs(),
        _ => f64::MAX,
    };
    c.iter().min_by(|a, b| by(a).total_cmp(&by(b))).map(|x| x.0.as_str())
}

/// The library's albums as last read (`Matcher::library_albums`): the file, the build it was read after, when, and them.
type KeptAlbums = Option<(std::path::PathBuf, i64, std::time::Instant, std::sync::Arc<Matcher>)>;

/// The longest the library's albums are kept, the library built again or not (0.11.1: /api/discover's memory).
const ALBUMS_KEEP: std::time::Duration = std::time::Duration::from_secs(60);

/// `slot`'s albums where they were read from `library` after build `built` and are young enough, else `read` and kept
/// there. The slot is held while reading, so those asking meanwhile wait and share the one reading instead of each
/// holding their own. One read within a second of its build (`now`) is not kept: `built` counts seconds, and a second
/// build that same second would not show (0.11.1: /api/discover's memory, 2026-10-30). Blocking.
fn kept_or_read(
    slot: &std::sync::Mutex<KeptAlbums>,
    library: &Path,
    built: i64,
    now: i64,
    read: impl FnOnce() -> Result<Matcher>,
) -> Result<std::sync::Arc<Matcher>> {
    let mut kept = slot.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((file, at_build, at, m)) = kept.as_ref()
        && file == library
        && *at_build == built
        && at.elapsed() < ALBUMS_KEEP
    {
        return Ok(m.clone());
    }
    // The old reading goes before the new one is made: never two at once.
    *kept = None;
    let m = std::sync::Arc::new(read()?);
    if now - built > 1 {
        *kept = Some((library.to_path_buf(), built, std::time::Instant::now(), m.clone()));
    }
    Ok(m)
}

/// How many of what was not found an import lists (the most items first).
const MISSING: usize = 100;

/// What an import would do (a preview) or did: how many items the source had, how many were found in the library, how
/// many of those are new, and what was not found.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Tally {
    pub items: usize,
    pub found: usize,
    pub new: usize,
    /// Not in the library, the most items first (at most 100): "artist – title" and how many items.
    pub missing: Vec<Missing>,
    /// Kinds of track not found, in all (beyond the ones listed).
    pub missing_tracks: usize,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Missing {
    pub artist: String,
    pub title: String,
    pub count: usize,
}

/// Counts as a source's items go by, then [`Tally`].
#[derive(Default)]
pub struct Counter {
    items: usize,
    found: usize,
    new: usize,
    missing: HashMap<(String, String), (String, String, usize)>,
}

impl Counter {
    /// One item: found (and new, or there already) or not.
    pub fn found(&mut self, new: bool) {
        self.items += 1;
        self.found += 1;
        self.new += usize::from(new);
    }

    pub fn missed(&mut self, t: &Track) {
        self.items += 1;
        let e = self.missing.entry((key(t.artist), key(t.title))).or_insert_with(|| (t.artist.to_string(), t.title.to_string(), 0));
        e.2 += 1;
    }

    pub fn tally(self) -> Tally {
        let kinds = self.missing.len();
        let mut missing: Vec<Missing> = self.missing.into_values().map(|(artist, title, count)| Missing { artist, title, count }).collect();
        missing.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.artist.cmp(&b.artist)).then_with(|| a.title.cmp(&b.title)));
        missing.truncate(MISSING);
        Tally { items: self.items, found: self.found, new: self.new, missing, missing_tracks: kinds }
    }
}

/// A listen an import brings into the history (decision 0067): its track, when, and its own id there.
pub struct Listened<'a> {
    pub track: Track<'a>,
    pub at: i64,
    pub event: String,
}

/// Another listen of the same song this near (seconds) is the same listen (a scrobbler that fed two services).
const SAME: i64 = 60;

/// Listens into `account`'s history, logged as heard from `context` (`lastfm`, `file:spotify`, …): finished, not
/// counted (counts come from a server's import), `player` named. One the history has already, from anywhere (the same
/// song within a minute), is not added twice. For a preview, nothing written. Blocking.
pub fn into_history(
    serve: &crate::serve::Serve,
    songs: &Matcher,
    account: &str,
    context: &str,
    player: &str,
    listens: &[Listened],
    preview: bool,
) -> Result<Tally> {
    let mut heard: HashMap<String, Vec<i64>> = HashMap::new();
    for l in serve.userdata.heard_since(account, 0)? {
        heard.entry(l.song).or_default().push(l.at);
    }
    let near = |heard: &HashMap<String, Vec<i64>>, song: &str, at: i64| heard.get(song).is_some_and(|ts| ts.iter().any(|t| (t - at).abs() <= SAME));
    let mut c = Counter::default();
    for l in listens {
        let Some(song) = songs.find(&l.track) else {
            c.missed(&l.track);
            continue;
        };
        if near(&heard, song, l.at) {
            c.found(false);
            continue;
        }
        if !preview {
            let Some((pid, album, artists)) = serve.library.play_targets(song)? else {
                c.missed(&l.track);
                continue;
            };
            let h = crate::userdata::Heard {
                at: l.at,
                heard_secs: l.track.length_secs,
                length_secs: l.track.length_secs,
                ended: crate::userdata::Ended::Finished,
                counts: false,
                player: Some(player.to_string()),
                output: None,
                context: Some(context.to_string()),
                event: Some(l.event.clone()),
                followed: false,
            };
            if !serve.userdata.record(account, (song, &pid), &album, &artists, &h)? {
                c.found(false);
                continue;
            }
        }
        heard.entry(song.to_string()).or_default().push(l.at);
        c.found(true);
    }
    Ok(c.tally())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track<'a>(title: &'a str, artist: &'a str, album: Option<&'a str>, recording: Option<&'a str>, length: Option<f64>) -> Track<'a> {
        Track { recording, artist, title, album, length_secs: length }
    }

    /// The process' peak memory so far (Linux), in MB.
    fn peak_mb() -> f64 {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let kb: f64 = status.lines().find(|l| l.starts_with("VmHWM:")).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        kb / 1024.0
    }

    /// What an import's matching costs in memory, on a real library (`CRATARR_LIBRARY`: a copy of `library.sqlite`) and
    /// 80,000 listens: `cargo test --release -- --ignored matcher_memory --nocapture`.
    #[test]
    #[ignore]
    fn matcher_memory() {
        let Ok(library) = std::env::var("CRATARR_LIBRARY") else { return };
        let before = peak_mb();
        let songs = Matcher::read(Path::new(&library)).unwrap();
        let read = peak_mb();
        let names: Vec<(String, String, String)> = (0..80_000).map(|i| (format!("Artist {}", i % 3000), format!("Title {i}"), format!("Album {}", i % 9000))).collect();
        let found = names.iter().filter(|(a, t, al)| songs.find(&track(t, a, Some(al), None, None)).is_some()).count();
        println!("peak before {before:.0} MB, after reading the library {read:.0} MB, after 80k lookups {:.0} MB ({found} found)", peak_mb());
    }

    #[test]
    fn a_track_is_found_by_recording_then_artist_then_album_and_its_length_picks_among_namesakes() {
        let mut m = Matcher::default();
        m.add("tr-1", "These Fruits, These Vegetables", Some("Photay"), "Two EPs", Some("F4F4"), Some(240.0));
        m.add("tr-2", "Untitled", Some("DJ Healer"), "Prisoner of Love", None, Some(300.0));
        m.add("tr-3", "Intro", None, "Some Album", None, None);
        m.add("tr-4", "Untitled", Some("DJ Healer"), "Live", None, Some(420.0));
        assert_eq!(m.find(&track("Other name", "Other", None, Some("f4f4"), None)), Some("tr-1"), "by recording, any case");
        assert_eq!(m.find(&track("untitled", "DJ  Healer", None, None, None)), Some("tr-2"), "by artist and title, punctuation aside");
        assert_eq!(m.find(&track("Untitled", "DJ Healer", None, None, Some(415.0))), Some("tr-4"), "the nearest length");
        assert_eq!(m.find(&track("Intro", "Someone", Some("Some Album"), None, None)), Some("tr-3"), "by album and title");
        assert_eq!(m.find(&track("Nothing", "Nobody", None, None, None)), None);
    }

    #[test]
    fn loosely_accents_brackets_feat_and_the_count_for_nothing() {
        assert_eq!(loose("Beyoncé feat. Jay-Z"), "beyonce");
        assert_eq!(loose("The Wall (Remastered 2011)"), "wall");
        assert_eq!(loose("Simon & Garfunkel"), "simonandgarfunkel");
        assert_eq!(loose("The The"), "the", "a name that is only \"the\" keeps it");
        assert_eq!(loose("(Untitled)"), "untitled", "nothing but brackets: its own key");
        let mut m = Matcher::default();
        m.add("tr-1", "Halo", Some("Beyoncé"), "I Am... Sasha Fierce", None, None);
        m.add_album("al-1", "Homogenic", Some("Björk"));
        m.add_album("al-2", "Debut", Some("Björk"));
        assert_eq!(m.find(&track("Halo (Live)", "Beyonce", None, None, None)), Some("tr-1"), "loosely, when the key finds nothing");
        assert_eq!(m.find_album("Homogenic", Some("Bjork")), Some("al-1"));
        assert_eq!(m.find_album("Debut", Some("Someone else")), Some("al-2"), "by name alone, the only one so named");
    }

    #[test]
    fn a_tally_counts_and_lists_what_was_not_found_the_most_first() {
        let mut c = Counter::default();
        c.found(true);
        c.found(false);
        for _ in 0..3 {
            c.missed(&track("B", "Band", None, None, None));
        }
        c.missed(&track("A", "Act", None, None, None));
        let t = c.tally();
        assert_eq!((t.items, t.found, t.new, t.missing_tracks), (6, 2, 1, 2));
        assert_eq!(t.missing[0], Missing { artist: "Band".into(), title: "B".into(), count: 3 });
    }

    #[test]
    fn the_albums_alone_find_every_album_the_whole_reading_does_and_no_track() {
        let dir = std::env::temp_dir().join(format!("cratarr-import-albums-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("library.sqlite");
        let _ = std::fs::remove_file(&file);
        let db = rusqlite::Connection::open(&file).unwrap();
        db.execute_batch(
            "CREATE TABLE songs (id TEXT, title TEXT, artist TEXT, album TEXT, mbid TEXT, duration_ms INTEGER, album_id TEXT, album_artist TEXT);
             INSERT INTO songs VALUES ('tr-1', 'Joga', 'Björk', 'Homogenic', 'm1', 300000, 'al-1', NULL);
             INSERT INTO songs VALUES ('tr-2', 'Human Behaviour', 'Björk', 'Debut', NULL, 250000, 'al-2', 'Björk');
             INSERT INTO songs VALUES ('tr-3', 'Intro', 'Guest', 'Hits', NULL, NULL, 'al-3', 'Various Artists');
             INSERT INTO songs VALUES ('tr-4', 'Intro', 'Other', 'Hits', NULL, NULL, 'al-4', 'Someone');",
        )
        .unwrap();
        drop(db);
        let (whole, albums) = (Matcher::read(&file).unwrap(), Matcher::read_albums(&file).unwrap());
        for (name, artist) in [("Homogenic", "Bjork"), ("Debut", "Anyone"), ("Hits", "Various Artists"), ("Hits", "Nobody"), ("Post", "Björk")] {
            assert_eq!(albums.find_album(name, Some(artist)), whole.find_album(name, Some(artist)), "{name} by {artist}");
        }
        assert_eq!(albums.find_album("Homogenic", None), Some("al-1"));
        assert!(whole.find(&track("Joga", "Björk", None, None, None)).is_some());
        assert_eq!(albums.find(&track("Joga", "Björk", None, Some("m1"), None)), None, "no songs read");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn those_asking_at_once_share_one_reading_kept_until_the_library_is_built_again() {
        let slot = std::sync::Mutex::new(None);
        let reads = std::sync::atomic::AtomicUsize::new(0);
        let library = Path::new("/data/library.sqlite");
        let read = || -> Result<Matcher> {
            reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(50));
            let mut m = Matcher::default();
            m.add_album("al-1", "Moon Safari", Some("Air"));
            Ok(m)
        };
        // Twelve at once (the Discover page's blinks): one reading, the same one for all.
        let (shared, reading) = (&slot, &read);
        let got: Vec<std::sync::Arc<Matcher>> = std::thread::scope(|s| {
            let asks: Vec<_> = (0..12).map(|_| s.spawn(move || kept_or_read(shared, library, 100, 200, reading).unwrap())).collect();
            asks.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(got.iter().all(|m| std::sync::Arc::ptr_eq(m, &got[0])));
        assert_eq!(got[0].find_album("Moon Safari", Some("AIR")), Some("al-1"));
        // Built again: read again. Another library file: read again.
        kept_or_read(&slot, library, 150, 200, read).unwrap();
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 2);
        kept_or_read(&slot, Path::new("/elsewhere/library.sqlite"), 150, 200, read).unwrap();
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 3);
        // Read within a second of its build: not kept, the next asks again.
        kept_or_read(&slot, library, 199, 200, read).unwrap();
        kept_or_read(&slot, library, 199, 200, read).unwrap();
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 5);
        // A reading that fails is not kept either.
        assert!(kept_or_read(&slot, library, 120, 200, || anyhow::bail!("gone")).is_err());
        kept_or_read(&slot, library, 120, 200, read).unwrap();
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 6);
    }
}
