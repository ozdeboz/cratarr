//! The shared cases (tests/vectors/*.json): what each rule answers, for the server and for every other build of these
//! rules (the page's, through WebAssembly) to check against. Each case: its input, its seed where it draws, and the
//! answer expected. Crates and Radio are asked of one library (`library.json`), with what a listener did: a device runs
//! them over its own index, so those two are what the page's and the app's builds answer to (0101's point 27).
//!
//! Written again from the cases below with `CRATARR_RULES_WRITE=1 cargo test -p cratarr-rules -- --ignored write`: only
//! when a rule changes on purpose (then every other build must change with it).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use cratarr_rules::crates::{self, Alike, Avoid, Condition, Facts, Favourites, LikeMode, Target, Trait, Use, Value, View, World, credited_names};
use cratarr_rules::excluded_artists;
use cratarr_rules::ids::song;
use cratarr_rules::index::own::{Kept, List, Own};
use cratarr_rules::index::{self, Artist, Library, Rows};
use cratarr_rules::radio::{Ask, Index, Mode, RadioBy, Song, choose};
use cratarr_rules::similarity::{MOODS, Signal, Track, Weights, mean_sound};
use cratarr_rules::{ByHand, How, Lane, Picked, Rng, add_place, pick, restore_order, shuffle_order};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct RngCase {
    seed: u64,
    /// The first numbers, as strings (more than JavaScript's safe integers).
    next: Vec<String>,
    /// `below(10)` after those.
    below_10: Vec<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PickCase {
    name: String,
    n: usize,
    how: How,
    at: Option<usize>,
    most: usize,
    seed: u64,
    expected: Picked,
}

#[derive(Debug, Serialize, Deserialize)]
struct ShuffleCase {
    name: String,
    lanes: Vec<Lane>,
    current: Option<usize>,
    seed: u64,
    expected: Option<Vec<usize>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AddCase {
    name: String,
    lanes: Vec<Lane>,
    current: Option<usize>,
    by: ByHand,
    expected: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct RestoreCase {
    name: String,
    queue: Vec<String>,
    current: Option<usize>,
    before: Vec<String>,
    expected: Vec<usize>,
}

fn rng_case(seed: u64) -> RngCase {
    let mut r = Rng::seeded(seed);
    let next = (0..5).map(|_| r.next_u64().to_string()).collect();
    let below_10 = (0..10).map(|_| r.below(10)).collect();
    RngCase { seed, next, below_10 }
}

fn pick_case(name: &str, n: usize, how: How, at: Option<usize>, most: usize, seed: u64) -> PickCase {
    let expected = pick(n, how, at, most, &mut Rng::seeded(seed));
    PickCase { name: name.into(), n, how, at, most, seed, expected }
}

fn lanes(s: &str) -> Vec<Lane> {
    s.chars()
        .map(|c| match c {
            'w' => Lane::Own,
            'n' => Lane::Next,
            'r' => Lane::Radio,
            'a' => Lane::Autoplay,
            _ => Lane::Other,
        })
        .collect()
}

fn shuffle_case(name: &str, l: &str, current: Option<usize>, seed: u64) -> ShuffleCase {
    let lanes = lanes(l);
    let expected = shuffle_order(&lanes, current, &mut Rng::seeded(seed));
    ShuffleCase { name: name.into(), lanes, current, seed, expected }
}

fn add_case(name: &str, l: &str, current: Option<usize>, by: ByHand) -> AddCase {
    let lanes = lanes(l);
    let expected = add_place(&lanes, current, by);
    AddCase { name: name.into(), lanes, current, by, expected }
}

fn restore_case(name: &str, queue: &[&str], current: Option<usize>, before: &[&str]) -> RestoreCase {
    let (queue, before): (Vec<String>, Vec<String>) = (queue.iter().map(|s| s.to_string()).collect(), before.iter().map(|s| s.to_string()).collect());
    let expected = restore_order(&queue, current, &before);
    RestoreCase { name: name.into(), queue, current, before, expected }
}

// --- Crates and Radio (0101's point 27): one library, one listener, and what each is asked of them ---

/// One file of the shared library, in plain data, for any build to read: what `crates::Facts` holds of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct File {
    rel: String,
    track: i64,
    folder: i64,
    album: String,
    born: i64,
    artists: Vec<String>,
    credits: Vec<String>,
    credit_names: Vec<String>,
    genres: Vec<String>,
    styles: Vec<String>,
    moods: Vec<String>,
    tags: Vec<String>,
    labels: Vec<String>,
    release_types: Vec<String>,
    year: Option<f64>,
    bpm: Option<f64>,
    four: Option<f64>,
    meter: Option<String>,
    energy: Option<f64>,
    danceability: Option<f64>,
    clap_moods: Option<[f64; 5]>,
    sound: Option<Vec<f64>>,
    length: Option<f64>,
    quality: Option<String>,
    key: Option<String>,
}

/// What one listener did with a line.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Used {
    plays: u32,
    last: Option<i64>,
    rating: Option<u8>,
    skips: u32,
}

/// The listener the crates' cases look through, by lines.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Listener {
    favourites: Vec<String>,
    favourite_albums: Vec<String>,
    fits: Vec<String>,
    uses: BTreeMap<String, Used>,
}

/// What `crates.json` and `radio.json` read: the files newest first, as a caller hands them in, the playlists holding each
/// line, each artist's library id by their name, and the moment it is all asked at.
#[derive(Debug, Serialize, Deserialize)]
struct Shared {
    now: i64,
    files: Vec<File>,
    playlists: BTreeMap<String, Vec<String>>,
    artist_ids: BTreeMap<String, String>,
    listener: Listener,
}

/// Names as numbers, as first met. A number's order decides the order sums are made in, so a build must number alike.
#[derive(Default)]
struct Numbers(HashMap<String, u32>);

impl Numbers {
    fn of(&mut self, name: &str) -> u32 {
        let next = self.0.len() as u32;
        *self.0.entry(name.to_string()).or_insert(next)
    }
}

impl File {
    fn facts(&self) -> Facts {
        Facts {
            rel: self.rel.clone(),
            lines: Vec::new(),
            folders: vec![self.folder],
            born: Some(self.born),
            genres: self.genres.clone(),
            year: self.year,
            bpm: self.bpm,
            four: self.four.map(|x| x as f32),
            meter: self.meter.clone(),
            energy: self.energy,
            danceability: self.danceability,
            moods: self.moods.clone(),
            styles: self.styles.clone(),
            tags: self.tags.clone(),
            track: Some(self.track),
            sound: self.sound.as_ref().map(|s| s.iter().map(|x| *x as f32).collect()),
            artists: self.artists.clone(),
            credits: self.credits.clone(),
            credit_names: self.credit_names.clone(),
            clap_moods: self.clap_moods.map(|m| m.map(|x| x as f32)),
            length: self.length,
            quality: self.quality.clone(),
            key: self.key.clone(),
            release_types: self.release_types.clone(),
            album: Some(Arc::from(self.album.as_str())),
            labels: self.labels.clone(),
            file_words: None,
        }
    }
}

impl Shared {
    fn facts(&self) -> Vec<Facts> {
        self.files.iter().map(File::facts).collect()
    }

    fn membership(&self) -> HashMap<String, Vec<String>> {
        self.playlists.iter().map(|(line, lists)| (line.clone(), lists.clone())).collect()
    }

    fn alike(&self, files: &[Facts]) -> Alike {
        Alike::of(files, &self.membership(), &self.artist_ids.iter().map(|(n, id)| (n.clone(), id.clone())).collect())
    }

    fn index(&self) -> Index {
        self.index_of(&self.facts())
    }

    /// Radio's index of it, as a caller makes one: albums, artists and playlists numbered as first met, its facts on top.
    fn index_of(&self, files: &[Facts]) -> Index {
        let (mut albums, mut artists, mut lists) = (Numbers::default(), Numbers::default(), Numbers::default());
        let songs = self
            .files
            .iter()
            .map(|f| {
                let playlists = self.playlists.get(&f.rel).into_iter().flatten().map(|p| lists.of(p)).collect();
                let like = Track { playlists, artists: f.artists.iter().map(|a| artists.of(a)).collect(), ..Track::default() };
                Song { id: song(f.track), title: f.rel.clone(), album: f.album.clone(), album_id: f.album.clone(), album_n: albums.of(&f.album), like, folders: vec![f.folder], ..Song::default() }
            })
            .collect();
        Index::new(songs).with_facts(files)
    }

    /// A line's song.
    fn song_at(&self, line: &str) -> String {
        song(self.files.iter().find(|f| f.rel == line).expect("a line of the library").track)
    }

    /// A record's songs, in order.
    fn album(&self, id: &str) -> Vec<String> {
        self.files.iter().filter(|f| f.album == id).map(|f| song(f.track)).collect()
    }
}

/// One crate over the shared library: its conditions, and how it is looked at.
#[derive(Debug, Default, Serialize, Deserialize)]
struct CrateCase {
    name: String,
    conditions: Vec<Condition>,
    /// Nothing of the listener is known: the favourites could not be asked, nobody is signed in.
    not_known: bool,
    /// The catalogue folders switched off.
    off: Vec<i64>,
    /// What switching every library on would show.
    every: bool,
    /// "Similar by": what makes music alike, the first counting most; none, all alike.
    by: Vec<Signal>,
    expected: Vec<String>,
}

/// One pick from the shared library: what is asked, and the listener's marks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RadioCase {
    name: String,
    ask: Ask,
    /// `Ask::folders`, which an `Ask` read from a player never holds.
    folders: Option<Vec<i64>>,
    /// What it stays inside, as tracks; a caller finds their places.
    within: Option<Vec<String>>,
    recent: BTreeMap<String, i64>,
    favourites: Vec<String>,
    excluded: Vec<String>,
    taste: BTreeMap<String, f64>,
    skipped: BTreeMap<String, f64>,
    seed: u64,
    expected: Vec<String>,
}

fn crate_answer(lib: &Shared, files: &[Facts], alike: &Alike, c: &CrateCase) -> Vec<String> {
    let membership = lib.membership();
    let set = |lines: &[String]| -> HashSet<String> { lines.iter().cloned().collect() };
    let (favourites, albums, fits) = (set(&lib.listener.favourites), set(&lib.listener.favourite_albums), set(&lib.listener.fits));
    let uses: HashMap<String, Use> = lib.listener.uses.iter().map(|(line, u)| (line.clone(), Use { plays: u.plays, last: u.last, rating: u.rating, skips: u.skips })).collect();
    let known = !c.not_known;
    let world = World { membership: &membership, favourites: known.then_some(&favourites), favourite_albums: known.then_some(&albums), uses: known.then_some(&uses), now: lib.now, fits: known.then_some(&fits) };
    let (off, by): (HashSet<i64>, Weights) = (c.off.iter().copied().collect(), Weights::ranked(&c.by));
    View { files, switched_off: &off, world, by: &by, alike: || alike }.select(&c.conditions, c.every)
}

fn radio_answer(index: &Index, c: &RadioCase) -> Vec<String> {
    let mut theirs = index.clone();
    for s in &mut theirs.songs {
        s.favourite = c.favourites.contains(&s.id);
        s.excluded = c.excluded.contains(&s.id);
        s.taste = c.taste.get(&s.id).copied();
        s.skipped = c.skipped.get(&s.id).copied().unwrap_or(0.0);
    }
    let ask = Ask { folders: c.folders.clone(), ..c.ask.clone() };
    let within = c.within.as_ref().map(|ids| theirs.find(ids));
    let recent: HashMap<String, i64> = c.recent.iter().map(|(id, at)| (id.clone(), *at)).collect();
    let picks = choose(&theirs, &ask, &recent, within.as_deref(), &mut Rng::seeded(c.seed));
    picks.into_iter().map(|i| theirs.songs[i].id.clone()).collect()
}

/// The shared library's records: id, artist (none: a compilation, its tracks by `GUESTS`), year, genre, style, label, types,
/// how many tracks, its catalogue folder.
type Record = (&'static str, &'static str, f64, &'static str, &'static str, &'static str, &'static [&'static str], usize, i64);
const RECORDS: [Record; 11] = [
    ("al-harbour", "Dusk Harbour", 1994.0, "Ambient", "drone", "Kelp", &["Album"], 6, 0),
    ("al-tideline", "Dusk Harbour", 1998.0, "Ambient", "dub techno", "Kelp", &["Album"], 5, 0),
    ("al-lantern", "Lamp Theory", 2001.0, "Electronic", "idm", "Filament", &["Album"], 6, 0),
    ("al-ember", "Kiln & Ash", 2003.0, "Electronic", "downtempo", "Filament", &["EP"], 4, 0),
    ("al-sidings", "Rail Yard Quartet", 1975.0, "Jazz", "post-bop", "Junction", &["Album"], 6, 0),
    ("al-signal", "Rail Yard Quartet", 1978.0, "Jazz", "fusion", "Junction", &["Album", "Live"], 6, 0),
    ("al-moss", "Mossback", 2012.0, "Folk", "field recording", "Understorey", &["Album"], 5, 0),
    ("al-flare", "Vale Sisters", 2020.0, "Pop", "synthpop", "", &["Single"], 1, 0),
    ("al-various", "", 2015.0, "Electronic", "idm", "Filament", &["Album", "Compilation"], 4, 0),
    ("al-crossing", "Night Ferry", 2024.0, "Electronic", "idm", "", &["Album"], 4, 1),
    ("al-wake", "Night Ferry", 2025.0, "Ambient", "drone", "", &["EP"], 3, 2),
];
const GUESTS: [&str; 4] = ["Lamp Theory feat. Vale Sisters", "Mossback", "Dusk Harbour & Night Ferry", "Kiln & Ash"];
const NOW: i64 = 1_800_000_000;
const DAY: i64 = 86_400;

/// Every way of asking Radio: each mode, those from before the amendment too, and its inserts by all three.
const ASKS: [(Mode, RadioBy); 11] = [
    (Mode::Similar, RadioBy::Track),
    (Mode::Artist, RadioBy::Track),
    (Mode::Era, RadioBy::Track),
    (Mode::Random, RadioBy::Track),
    (Mode::Taste, RadioBy::Track),
    (Mode::Continuous, RadioBy::Track),
    (Mode::AlbumMix, RadioBy::Track),
    (Mode::Reshuffle, RadioBy::Track),
    (Mode::Radio, RadioBy::Track),
    (Mode::Radio, RadioBy::Artist),
    (Mode::Radio, RadioBy::Era),
];

/// The library the cases read. A record's tracks sit near one sound, so a Sounds like has something to find; every fifth
/// track was not heard, so what is not known has to count for nothing; numbers are rounded, to read as written everywhere.
fn shared_library() -> Shared {
    let three = |x: f64| (x * 1000.0).round() / 1000.0;
    let mut r = Rng::seeded(59);
    let (mut files, mut playlists, mut listener) = (Vec::new(), BTreeMap::new(), Listener::default());
    let mut n: i64 = 0;
    for (album, artist, year, genre, style, label, types, tracks, folder) in RECORDS {
        let centre: Vec<f64> = (0..8).map(|_| r.unit() - 0.5).collect();
        for t in 1..=tracks {
            n += 1;
            let rel = format!("{}/{t:02}.flac", album.trim_start_matches("al-"));
            let heard = n % 5 != 0;
            let artists = vec![if artist.is_empty() { GUESTS[t % GUESTS.len()] } else { artist }.to_string()];
            let (credits, credit_names) = excluded_artists::credited(&artists);
            let sound = heard.then(|| {
                let raw: Vec<f64> = centre.iter().map(|c| c + 0.3 * (r.unit() - 0.5)).collect();
                let length = raw.iter().map(|x| x * x).sum::<f64>().sqrt();
                raw.iter().map(|x| three(x / length)).collect()
            });
            let clap_moods: Option<[f64; 5]> = heard.then(|| [(); 5].map(|_| three(r.unit())));
            let moods = clap_moods.map_or(Vec::new(), |c| MOODS.iter().zip(c).filter(|(_, x)| *x > 0.6).map(|(m, _)| m.to_string()).collect());
            let word = |on: bool, word: &str| on.then(|| word.to_string());
            files.push(File {
                rel: rel.clone(),
                track: n,
                folder,
                album: album.to_string(),
                born: NOW - DAY * 3 * n,
                artists,
                credits,
                credit_names,
                genres: [word(true, genre), word(n % 3 == 0, "Experimental")].into_iter().flatten().collect(),
                styles: [word(heard, style), word(heard && n % 4 == 0, "minimal")].into_iter().flatten().collect(),
                moods,
                tags: [word(n % 2 == 0, "night"), word(n % 3 == 0, "rainy")].into_iter().flatten().collect(),
                labels: word(!label.is_empty(), label).into_iter().collect(),
                release_types: types.iter().map(|t| t.to_string()).collect(),
                year: (n % 11 != 0).then_some(year),
                bpm: (n % 7 != 0).then(|| (70 + r.below(90)) as f64),
                four: heard.then(|| three(r.unit())),
                meter: heard.then(|| ["4/4", "3/4"][(n % 2) as usize].to_string()),
                energy: heard.then(|| (100.0 * r.unit()).round()),
                danceability: heard.then(|| (100.0 * r.unit()).round()),
                clap_moods,
                sound,
                length: Some((120 + r.below(480)) as f64),
                quality: Some(if folder > 0 { "320 kbps" } else if n % 6 == 0 { "Under 192 kbps" } else { "Lossless" }.to_string()),
                key: (n % 5 != 1).then(|| ["D minor", "A minor", "C major", "F# major"][(n % 4) as usize].to_string()),
            });
            let lists: Vec<String> = [word(n % 3 == 0, "Road"), word(n % 4 == 1, "Mix")].into_iter().flatten().collect();
            if !lists.is_empty() {
                playlists.insert(rel.clone(), lists);
            }
            if n % 7 == 2 {
                listener.favourites.push(rel.clone());
            }
            if ["al-lantern", "al-moss"].contains(&album) {
                listener.favourite_albums.push(rel.clone());
            }
            if n % 4 == 0 || n % 4 == 3 {
                listener.fits.push(rel.clone());
            }
            if n % 2 == 1 {
                let used = Used { plays: (n % 9) as u32, last: (n % 9 != 0).then_some(NOW - DAY * (n % 40)), rating: (n % 3 == 1).then_some((1 + n % 5) as u8), skips: (n % 6) as u32 };
                listener.uses.insert(rel, used);
            }
        }
    }
    let facts: Vec<Facts> = files.iter().map(File::facts).collect();
    let artist_ids = credited_names(&facts).into_iter().map(|(name, key)| (name.to_string(), format!("ar-{}", key.replace(' ', "-")))).collect();
    Shared { now: NOW, files, playlists, artist_ids, listener }
}

/// What a Sounds like may say beyond its seeds and its count.
#[derive(Default)]
struct Also {
    min: Option<u8>,
    own: Option<bool>,
    mode: Option<LikeMode>,
    fresh: bool,
    weights: Vec<f32>,
    unlisted: bool,
    avoid: Avoid,
}

fn like(of: &[String], count: usize, a: Also) -> Value {
    Value::Like { of: of.to_vec(), count, min: a.min, own: a.own, mode: a.mode, fresh: a.fresh, weights: a.weights, unlisted: a.unlisted, avoid: a.avoid }
}

/// Every axis, what the crate's own conditions add unseen, every crate made when asked for, and the ways a Sounds like reads
/// its seeds: `expected` is filled in by `write`.
fn crate_cases(lib: &Shared) -> Vec<CrateCase> {
    let strings = |v: &[&str]| -> Vec<String> { v.iter().map(|s| s.to_string()).collect() };
    let any = |v: &[&str]| Value::AnyOf { values: strings(v) };
    let all = |v: &[&str]| Value::AllOf { values: strings(v) };
    let range = |min: Option<f64>, max: Option<f64>| Value::Range { min, max };
    let is = |axis: &str, value: Value| Condition::new(axis, false, value);
    let not = |axis: &str, value: Value| Condition::new(axis, true, value);
    let case = |name: &str, conditions: Vec<Condition>| CrateCase { name: name.to_string(), conditions, ..CrateCase::default() };
    let tracks = lib.album("al-sidings");
    let (one, two) = (std::slice::from_ref(&tracks[0]), [tracks[0].clone(), lib.album("al-moss")[0].clone()]);
    let album = ["al-harbour".to_string()];
    let artist = [lib.artist_ids["Lamp Theory"].clone()];
    let electronic = is("genre", any(&["Electronic"]));
    let mut cases = vec![
        case("a favourite", vec![is("favourite", Value::Yes)]),
        case("not a favourite", vec![not("favourite", Value::Yes)]),
        case("a favourite album", vec![is("favourite_album", Value::Yes)]),
        case("fits the taste", vec![is("fit", Value::Yes)]),
        case("in one playlist", vec![is("playlist", any(&["Road"]))]),
        case("in any playlist", vec![is("playlist", any(&[]))]),
        case("played three times or more", vec![is("plays", range(Some(3.0), None))]),
        case("skipped twice or more", vec![is("skips", range(Some(2.0), None))]),
        case("played in the last ten days", vec![is("played", range(None, Some(10.0)))]),
        case("not played for a month, or never", vec![is("played", range(Some(30.0), None))]),
        case("added in the last fortnight", vec![is("added", range(None, Some(14.0)))]),
        case("rated four or five", vec![is("rating", range(Some(4.0), None))]),
        case("a genre, in any case", vec![is("genre", any(&["ambient"]))]),
        case("either of two genres", vec![is("genre", any(&["Jazz", "Folk"]))]),
        case("both of two genres", vec![is("genre", all(&["Electronic", "Experimental"]))]),
        case("not both of two genres", vec![not("genre", all(&["Electronic", "Experimental"]))]),
        case("an artist by any credit", vec![is("artist", any(&["Vale Sisters"]))]),
        case("a record", vec![is("album", any(&["al-moss"]))]),
        case("a label, in any case", vec![is("label", any(&["filament"]))]),
        case("a type", vec![is("type", any(&["EP"]))]),
        case("either of two release types", vec![is("type", any(&["EP", "Single"]))]),
        case("a type of each layer", vec![is("type", any(&["Album", "Live"]))]),
        case("the years of a decade", vec![is("year", range(Some(1990.0), Some(1999.0)))]),
        case("not of a decade, a track of no year too", vec![not("year", range(Some(1990.0), Some(1999.0)))]),
        case("a tempo", vec![is("bpm", range(Some(100.0), Some(130.0)))]),
        case("four on the floor", vec![is("four", Value::Yes)]),
        case("a time signature", vec![is("meter", any(&["3/4"]))]),
        case("over five minutes", vec![is("length", range(Some(5.0), None))]),
        case("lossless", vec![is("quality", any(&["Lossless"]))]),
        case("a key", vec![is("key", any(&["d minor"]))]),
        case("a mood", vec![is("mood", any(&["sad"]))]),
        case("both of two moods", vec![is("mood", all(&["happy", "relaxed"]))]),
        case("calm", vec![is("energy", range(None, Some(40.0)))]),
        case("danceable", vec![is("danceability", range(Some(60.0), None))]),
        case("a style", vec![is("style", any(&["IDM"]))]),
        case("both of two styles", vec![is("style", all(&["idm", "minimal"]))]),
        case("a tag", vec![is("tag", any(&["rainy"]))]),
        case("both of two tags", vec![is("tag", all(&["night", "rainy"]))]),
        case("two conditions together", vec![electronic.clone(), is("year", range(Some(2000.0), None))]),
        case("a muted condition asks nothing", vec![electronic.clone(), Condition { enabled: false, ..is("year", range(Some(2000.0), None)) }]),
        case("an axis not known: nothing, as a crate from a newer version is narrower", vec![is("sparkle", any(&["yes"]))]),
        case("an axis not known, negated: everything", vec![not("sparkle", any(&["yes"]))]),
        case("no conditions: everything, newest first", Vec::new()),
        case("the favourites toggle, only", Favourites::Only.with(std::slice::from_ref(&electronic))),
        case("the favourites toggle, none", Favourites::None.with(std::slice::from_ref(&electronic))),
        case("the favourite albums toggle, only", Favourites::Only.with_albums(&[is("year", range(Some(2000.0), None))])),
        case("left out by hand", crates::leaving_out(&lib.listener.favourites.iter().map(|line| lib.song_at(line)).collect::<Vec<_>>(), std::slice::from_ref(&electronic))),
        case("an artist excluded everywhere", crates::leaving_out_artists(&HashSet::from([excluded_artists::key("Lamp Theory")]), std::slice::from_ref(&electronic))),
        case("the seed's own sound", vec![is("like", like(one, 8, Also::default()))]),
        case("like two seeds, blended", vec![is("like", like(&two, 8, Also::default()))]),
        case("like either of two seeds", vec![is("like", like(&two, 8, Also { mode: Some(LikeMode::Either), ..Also::default() }))]),
        case("two seeds weighed apart", vec![is("like", like(&two, 8, Also { weights: vec![1.0, 0.25], ..Also::default() }))]),
        case("the seeds left out", vec![is("like", like(&two, 8, Also { fresh: true, ..Also::default() }))]),
        case("alike by at least a half", vec![is("like", like(one, 30, Also { min: Some(50), ..Also::default() }))]),
        case("leaning to what is in no playlist", vec![is("like", like(one, 8, Also { unlisted: true, ..Also::default() }))]),
        case("like an album", vec![is("like", like(&album, 8, Also::default()))]),
        case("like an artist, theirs left out", vec![is("like", like(&artist, 8, Also::default()))]),
        case("like an artist, theirs kept", vec![is("like", like(&artist, 8, Also { own: Some(true), ..Also::default() }))]),
        case("not what it learned to avoid", vec![is("like", like(one, 8, Also { avoid: Avoid::of(vec![Trait::Style { name: "post-bop".into() }], 6), ..Also::default() }))]),
        case("the closest among what the other conditions let through", vec![electronic.clone(), is("like", like(one, 5, Also::default()))]),
        CrateCase { by: vec![Signal::Genres, Signal::Era], ..case("alike by genre, then era", vec![is("like", like(one, 8, Also::default()))]) },
        CrateCase { off: vec![1, 2], ..case("two libraries switched off", vec![is("genre", any(&["Ambient"]))]) },
        CrateCase { off: vec![1, 2], every: true, ..case("what switching them on would show", vec![is("genre", any(&["Ambient"]))]) },
        CrateCase { off: vec![0], ..case("a Sounds like over what is switched on", vec![is("like", like(one, 5, Also::default()))]) },
        CrateCase { not_known: true, ..case("favourites not known: nothing is one", vec![is("favourite", Value::Yes)]) },
        CrateCase { not_known: true, ..case("favourites not known, negated: everything", vec![not("favourite", Value::Yes)]) },
    ];
    for def in crates::permanent() {
        cases.push(case(&def.name, def.conditions));
    }
    for id in ["genre:Ambient", "style:idm", "mood:happy", "tag:night", "label:Filament"] {
        cases.push(case(id, crates::genre_crate(id).expect("a word's crate").conditions));
    }
    let mix: Vec<String> = lib.playlists.iter().filter(|(_, lists)| lists.contains(&"Mix".to_string())).map(|(line, _)| lib.song_at(line)).collect();
    let week = tracks.iter().take(3).zip([1.0, 0.5, 0.25]).map(|(t, w)| (t.clone(), w)).collect();
    let target = Target { energy: 30.0, mood: Some([0.2, 0.5, 0.7]) };
    for def in [
        crates::like_crate("like:one", "Like one track", one.to_vec(), 10),
        crates::playlist_crate("like:list:Mix", "Mix", mix, 10),
        crates::week_crate(crates::HOME_WEEK, week, 10),
        crates::moment_crate("like:now", "A rainy evening", target, 10),
        crates::moment_crate("like:now", "A morning with no mood", Target { energy: 80.0, mood: None }, 10),
    ] {
        cases.push(case(&def.name, def.conditions));
    }
    cases
}

/// Every way of asking, on two seeds, whole albums or tracks, favourites or none, and what else `choose` is handed.
fn radio_cases(lib: &Shared) -> Vec<RadioCase> {
    // An artist of two records, or Radio by artist has no whole album to give.
    let seeds = lib.album("al-sidings");
    let track = seeds[2].clone();
    let favourites: Vec<String> = lib.listener.favourites.iter().map(|l| lib.song_at(l)).collect();
    let taste: BTreeMap<String, f64> = lib.files.iter().map(|f| (song(f.track), (f.track * 37 % 100) as f64 / 100.0)).collect();
    let asked = |mode: Mode, by: RadioBy| {
        let seed = if mode == Mode::Radio { vec![track.clone()] } else { seeds.clone() };
        Ask { mode, by, exclude: seed.clone(), seed, last: Some(track.clone()), count: Some(5), ..Ask::default() }
    };
    let case = |name: &str, ask: Ask| RadioCase { name: name.to_string(), ask, favourites: favourites.clone(), seed: 7, ..RadioCase::default() };
    let similar = || asked(Mode::Similar, RadioBy::Track);
    let mut cases = Vec::new();
    for seed in [7, 1234] {
        for (mode, by) in ASKS {
            for (albums, skip_favourites) in [(false, false), (false, true), (true, false), (true, true)] {
                let name = format!("{mode:?} by {by:?}, {}, {}, seed {seed}", if albums { "whole albums" } else { "tracks" }, if skip_favourites { "no favourites" } else { "favourites" });
                let ask = Ask { albums, skip_favourites, ..asked(mode, by) };
                cases.push(RadioCase { seed, taste: if mode == Mode::Taste { taste.clone() } else { BTreeMap::new() }, ..case(&name, ask) });
            }
        }
    }
    let of = |albums: &[&str]| -> Vec<String> { albums.iter().flat_map(|a| lib.album(a)).collect() };
    let at = |tracks: Vec<String>, from: i64| -> BTreeMap<String, i64> { tracks.into_iter().enumerate().map(|(k, t)| (t, from + k as i64)).collect() };
    let inside = of(&["al-sidings", "al-signal"]);
    let tides: BTreeMap<String, f64> = of(&["al-tideline", "al-ember"]).into_iter().map(|t| (t, 1.0)).collect();
    let everything_but_two: Vec<String> = lib.files.iter().skip(2).map(|f| song(f.track)).filter(|t| !seeds.contains(t)).collect();
    let no_seed = |ask: Ask| Ask { seed: Vec::new(), exclude: Vec::new(), ..ask };
    // Autoplay after the first tracks of a seed: the rest of it, a favourite among them, is Random's to draw first.
    let drawing = || Ask { exclude: vec![seeds[0].clone()], ..asked(Mode::Random, RadioBy::Track) };
    let one_of_the_seed: Vec<String> = of(&["al-signal"]).into_iter().chain([seeds[3].clone()]).collect();
    // Half of each record: a crate of whole records could not tell the crate's tracks from the album's.
    let halves: Vec<String> = ["al-signal", "al-ember"].iter().map(|a| lib.album(a)).flat_map(|t| t[..t.len() / 2].to_vec()).collect();
    cases.extend([
        case("after the playing track alone", no_seed(similar())),
        case("with no seed and nothing playing", Ask { last: None, ..no_seed(similar()) }),
        case("one track", Ask { count: Some(1), ..similar() }),
        case("twelve tracks", Ask { count: Some(12), ..similar() }),
        case("no more than fifty, whatever is asked", Ask { count: Some(60), ..similar() }),
        case("by the listener's own Similar by", Ask { similar_by: Some(vec![Signal::Genres, Signal::Era]), ..similar() }),
        case("Radio by the listener's own Similar by", Ask { similar_by: Some(vec![Signal::Sound, Signal::Tempo]), ..asked(Mode::Radio, RadioBy::Track) }),
        case("Your taste with no taste yet: Similar", asked(Mode::Taste, RadioBy::Track)),
        RadioCase { skipped: tides.clone(), ..case("what is like the listener's skips comes less often", similar()) },
        RadioCase { skipped: tides, ..case("in Random too", asked(Mode::Random, RadioBy::Track)) },
        RadioCase { recent: at(of(&["al-ember", "al-various"]), NOW - DAY), ..case("what was played this week waits", similar()) },
        RadioCase { recent: at(everything_but_two, NOW - DAY), ..case("nothing else left: the longest ago come back", similar()) },
        RadioCase { excluded: of(&["al-ember"]), ..case("never what the listener excluded", similar()) },
        RadioCase { excluded: vec![lib.album("al-lantern")[1].clone()], ..case("nor in a whole album", Ask { albums: true, ..similar() }) },
        RadioCase { folders: Some(vec![1, 2]), ..case("only the libraries chosen, seeds from others still seed", similar()) },
        RadioCase { folders: Some(vec![9]), ..case("no library chosen holds anything: nothing", similar()) },
        RadioCase { within: Some(inside.clone()), ..case("inside a crate only its tracks play", similar()) },
        RadioCase { within: Some(inside.clone()), ..case("inside a crate, at random", asked(Mode::Random, RadioBy::Track)) },
        RadioCase { within: Some(halves), ..case("inside a crate, whole albums: what it holds of them", Ask { albums: true, ..similar() }) },
        RadioCase {
            within: Some(inside.clone()),
            ..case("all of it queued: a new round, the longest ago first", Ask { exclude: inside.clone(), last: inside.last().cloned(), count: Some(3), ..no_seed(similar()) })
        },
        case("at random, what the queue left of the seed first", drawing()),
        RadioCase { excluded: vec![seeds[4].clone()], ..case("at random, never what the listener excluded of the seed", drawing()) },
        case("at random, no favourite of the seed while they are ignored", Ask { skip_favourites: true, ..drawing() }),
        RadioCase { folders: Some(vec![1, 2]), ..case("at random, nothing of the seed from a library not chosen", drawing()) },
        RadioCase { within: Some(one_of_the_seed), ..case("at random inside a crate, nothing of the seed outside it", drawing()) },
    ]);
    cases
}

/// Why a case may not pick a track, if it may not: what no answer of it can hold, whatever `write` was handed.
fn barred(lib: &Shared, c: &RadioCase, id: &String) -> Option<&'static str> {
    let folder = lib.files.iter().find(|f| song(f.track) == *id).expect("a track of the library").folder;
    match &c.within {
        Some(inside) if !inside.contains(id) => Some("outside what it stays inside"),
        None if c.folders.as_ref().is_some_and(|f| !f.contains(&folder)) => Some("in a library not chosen"),
        _ if c.excluded.contains(id) => Some("excluded"),
        _ if c.ask.skip_favourites && c.favourites.contains(id) => Some("a favourite, ignored"),
        _ => None,
    }
}

/// One thing to a line, so a changed answer is a changed line: a list's items, or a map's values.
fn by_line(v: &serde_json::Value) -> String {
    use serde_json::Value::{Array, Object};
    let joined = |open: char, close: char, lines: Vec<String>| format!("{open}\n{}\n{close}", lines.join(",\n"));
    match v {
        Array(items) if !items.is_empty() => joined('[', ']', items.iter().map(ToString::to_string).collect()),
        Object(keys) if !keys.is_empty() => joined('{', '}', keys.iter().map(|(k, v)| format!("{}: {v}", serde_json::Value::from(k.as_str()))).collect()),
        other => other.to_string(),
    }
}

/// A map of lists, each by its lines: the shared library's shape.
fn by_lines(v: &serde_json::Value) -> String {
    let keys = v.as_object().expect("the library is a map").iter().map(|(k, v)| format!("{}: {}", serde_json::Value::from(k.as_str()), by_line(v)));
    format!("{{\n{}\n}}", keys.collect::<Vec<_>>().join(",\n"))
}

const RNG: &str = include_str!("vectors/rng.json");
const PICK: &str = include_str!("vectors/pick.json");
const SHUFFLE: &str = include_str!("vectors/shuffle_order.json");
const ADD: &str = include_str!("vectors/add_place.json");
const RESTORE: &str = include_str!("vectors/restore_order.json");
const LIBRARY: &str = include_str!("vectors/library.json");
const CRATES: &str = include_str!("vectors/crates.json");
const RADIO: &str = include_str!("vectors/radio.json");

#[test]
fn the_generator_gives_the_numbers_written() {
    let cases: Vec<RngCase> = serde_json::from_str(RNG).unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        let now = rng_case(c.seed);
        assert_eq!((now.next, now.below_10), (c.next, c.below_10), "seed {}", c.seed);
    }
}

#[test]
fn picking_answers_as_written() {
    let cases: Vec<PickCase> = serde_json::from_str(PICK).unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        assert_eq!(pick(c.n, c.how, c.at, c.most, &mut Rng::seeded(c.seed)), c.expected, "{}", c.name);
    }
}

#[test]
fn a_shuffle_once_answers_as_written() {
    let cases: Vec<ShuffleCase> = serde_json::from_str(SHUFFLE).unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        assert_eq!(shuffle_order(&c.lanes, c.current, &mut Rng::seeded(c.seed)), c.expected, "{}", c.name);
    }
}

#[test]
fn adding_by_hand_answers_as_written() {
    let cases: Vec<AddCase> = serde_json::from_str(ADD).unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        assert_eq!(add_place(&c.lanes, c.current, c.by), c.expected, "{}", c.name);
    }
}

#[test]
fn undo_answers_as_written() {
    let cases: Vec<RestoreCase> = serde_json::from_str(RESTORE).unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        assert_eq!(restore_order(&c.queue, c.current, &c.before), c.expected, "{}", c.name);
    }
}

#[test]
fn every_crate_answers_as_written() {
    let lib: Shared = serde_json::from_str(LIBRARY).unwrap();
    let cases: Vec<CrateCase> = serde_json::from_str(CRATES).unwrap();
    let (files, tracks) = (lib.facts(), lib.files.len());
    let alike = lib.alike(&files);
    assert!(cases.len() >= 60 && tracks >= 50);
    for c in &cases {
        assert_eq!(crate_answer(&lib, &files, &alike, c), c.expected, "{}", c.name);
        // All or nothing says little of a rule, so a case that does says it means to.
        assert!(!c.expected.is_empty() || c.name.contains("nothing"), "{}: holds nothing", c.name);
        assert!(c.expected.len() < tracks || c.name.contains("everything"), "{}: holds everything", c.name);
    }
}

#[test]
fn every_axis_is_asked_of_the_shared_library() {
    let cases: Vec<CrateCase> = serde_json::from_str(CRATES).unwrap();
    let asked = |axis: &str| cases.iter().any(|c| c.conditions.iter().any(|x| x.axis == axis));
    for axis in crates::list_axes(&[], &[], None, None, 0) {
        assert!(asked(axis.key), "{}", axis.key);
    }
    assert!(asked("track") && asked("credited"), "what a crate adds unseen");
}

#[test]
fn radio_picks_as_written() {
    let lib: Shared = serde_json::from_str(LIBRARY).unwrap();
    let cases: Vec<RadioCase> = serde_json::from_str(RADIO).unwrap();
    let index = lib.index();
    assert!(cases.len() >= 100);
    for c in &cases {
        assert_eq!(radio_answer(&index, c), c.expected, "{}", c.name);
        assert!(!c.expected.is_empty() || c.name.contains("nothing"), "{}: picks nothing", c.name);
    }
}

/// The written answers come of sounds each library made of unit length itself. A server's facts come so already and the
/// libraries share them: the answers stay the ones written.
#[test]
fn every_crate_and_pick_answers_as_written_from_sounds_of_unit_length_shared() {
    let lib: Shared = serde_json::from_str(LIBRARY).unwrap();
    let (crates, radio): (Vec<CrateCase>, Vec<RadioCase>) = (serde_json::from_str(CRATES).unwrap(), serde_json::from_str(RADIO).unwrap());
    let mut files = lib.facts();
    for f in &mut files {
        f.sound = f.sound.take().and_then(|s| mean_sound(&[&s])).map(Arc::from);
    }
    assert!(files.iter().any(|f| f.sound.is_some()));
    let alike = lib.alike(&files);
    for c in &crates {
        assert_eq!(crate_answer(&lib, &files, &alike, c), c.expected, "{}", c.name);
    }
    let index = lib.index_of(&files);
    for c in &radio {
        assert_eq!(radio_answer(&index, c), c.expected, "{}", c.name);
    }
}

#[test]
fn every_way_of_asking_radio_is_written_and_the_seed_decides() {
    let cases: Vec<RadioCase> = serde_json::from_str(RADIO).unwrap();
    for (mode, by) in ASKS {
        let on = |seed: u64| -> Vec<&Vec<String>> { cases.iter().filter(|c| c.ask.mode == mode && c.ask.by == by && c.seed == seed && c.name.ends_with(&format!("seed {seed}"))).map(|c| &c.expected).collect() };
        assert_eq!(on(7).len(), 4, "{mode:?} {by:?}");
        assert_eq!(on(1234).len(), 4, "{mode:?} {by:?}");
    }
    let draws = |mode: Mode| cases.iter().filter(|c| c.ask.mode == mode && c.name.ends_with("seed 7")).zip(cases.iter().filter(|c| c.ask.mode == mode && c.name.ends_with("seed 1234"))).any(|(a, b)| a.expected != b.expected);
    for mode in [Mode::Similar, Mode::Random, Mode::Radio, Mode::Taste] {
        assert!(draws(mode), "{mode:?} drew the same on both seeds");
    }
}

/// A written answer sets down what every build must give, so `write` must not be able to set down a leak. Reshuffle is the
/// seed again, whatever it holds, and no player asks for it now.
#[test]
fn no_written_answer_holds_what_may_not_be_picked() {
    let lib: Shared = serde_json::from_str(LIBRARY).unwrap();
    let cases: Vec<RadioCase> = serde_json::from_str(RADIO).unwrap();
    for c in cases.iter().filter(|c| c.ask.mode != Mode::Reshuffle) {
        for t in &c.expected {
            assert_eq!(barred(&lib, c, t), None, "{}: {t}", c.name);
        }
    }
}

/// A whole album is where the limits were lost: the record picked has to hold a track each of them leaves out. (No record of
/// the shared library spans two libraries, so that limit is `radio.rs`'s own test.)
#[test]
fn whole_albums_are_written_with_a_track_of_the_record_for_each_limit_to_leave_out() {
    let lib: Shared = serde_json::from_str(LIBRARY).unwrap();
    let cases: Vec<RadioCase> = serde_json::from_str(RADIO).unwrap();
    let mut kept_out: HashSet<&str> = HashSet::new();
    for c in cases.iter().filter(|c| (c.ask.albums || c.ask.mode == Mode::AlbumMix) && c.ask.mode != Mode::Reshuffle) {
        let records: HashSet<&String> = lib.files.iter().filter(|f| c.expected.contains(&song(f.track))).map(|f| &f.album).collect();
        kept_out.extend(lib.files.iter().filter(|f| records.contains(&f.album)).filter_map(|f| barred(&lib, c, &song(f.track))));
    }
    assert_eq!(kept_out, HashSet::from(["outside what it stays inside", "excluded", "a favourite, ignored"]));
}

/// Random takes the seed by a road of its own, past the pool, so a case has to reach it with something there to keep out.
#[test]
fn random_is_written_with_a_seed_left_to_draw_that_holds_each_thing_it_may_not_pick() {
    let lib: Shared = serde_json::from_str(LIBRARY).unwrap();
    let cases: Vec<RadioCase> = serde_json::from_str(RADIO).unwrap();
    let left = |c: &'_ RadioCase| -> Vec<String> { c.ask.seed.iter().filter(|t| !c.ask.exclude.contains(t) && c.ask.last.as_ref() != Some(t)).cloned().collect() };
    let mut kept_out: HashSet<&str> = HashSet::new();
    for c in cases.iter().filter(|c| c.ask.mode == Mode::Random && !c.ask.albums) {
        for t in left(c) {
            if let Some(why) = barred(&lib, c, &t) {
                assert!(!c.expected.contains(&t), "{}: {t}: {why}", c.name);
                kept_out.insert(why);
            }
        }
    }
    assert_eq!(kept_out.len(), 4, "{kept_out:?}");
    assert!(cases.iter().any(|c| c.ask.mode == Mode::Random && left(c).iter().any(|t| c.expected.contains(t))), "no case draws from the seed");
}

// --- The same answers through the index (0101's point 17): what a device holds is what the server read ---

/// The shared library as a device holds it: an index of its files, through its bytes. A record's and an artist's id are numbers
/// for life there, so what a case names by this library's own words moves to them first: what they became is the second part.
fn held(lib: &Shared) -> (index::Index, HashMap<String, String>) {
    let (mut records, mut artists) = (Numbers::default(), Numbers::default());
    let mut moved = HashMap::new();
    let mut rows = Rows::default();
    for (name, id) in &lib.artist_ids {
        let number = i64::from(artists.of(id)) + 1;
        moved.insert(id.clone(), format!("ar-{number}"));
        match rows.artists.iter_mut().find(|a| a.id == number) {
            Some(a) => a.spelled.push(name.clone()),
            None => rows.artists.push(Artist { id: number, name: name.clone(), sort: name.to_lowercase(), spelled: Vec::new() }),
        }
    }
    for f in &lib.files {
        let record = i64::from(records.of(&f.album)) + 1;
        moved.insert(f.album.clone(), format!("al-{record}"));
        if !rows.records.iter().any(|r| r.id == record) {
            rows.records.push(index::Record { id: record, title: f.album.clone(), sort: f.album.clone(), ..index::Record::default() });
        }
        if !rows.libraries.iter().any(|l| l.id == f.folder) {
            rows.libraries.push(Library { id: f.folder, key: format!("library-{}", f.folder), name: format!("Library {}", f.folder), ..Library::default() });
        }
        let artist_ids = f.artists.iter().map(|a| moved[&lib.artist_ids[a]][3..].parse().unwrap()).collect();
        rows.tracks.push(index::Track { id: f.track, record, number: Some(f.track as u32), title: f.rel.clone(), artist_ids, facts: f.facts(), ..index::Track::default() });
    }
    let saved = index::Index::build(1, &rows).unwrap().save();
    (index::Index::load(&saved).unwrap(), moved)
}

/// The index's facts, with the sounds as written: how near eight bits come is another question, and this one is what else differs.
fn facts_held(index: &index::Index, lib: &Shared) -> Vec<Facts> {
    let mut files = index.facts(None);
    for (f, written) in files.iter_mut().zip(lib.facts()) {
        assert_eq!(f.rel, written.rel, "the order they are handed in");
        f.sound = written.sound;
    }
    files
}

#[test]
fn every_crate_answers_as_written_through_the_index() {
    let lib: Shared = serde_json::from_str(LIBRARY).unwrap();
    let mut cases: Vec<CrateCase> = serde_json::from_str(CRATES).unwrap();
    let (index, moved) = held(&lib);
    let files = facts_held(&index, &lib);
    let alike = Alike::of(&files, &lib.membership(), &index.artist_ids());
    assert!(cases.len() >= 60);
    for c in &mut cases {
        crates::move_named(&mut c.conditions, &mut |id| moved.get(id).cloned());
        assert_eq!(crate_answer(&lib, &files, &alike, c), c.expected, "{}", c.name);
    }
}

/// Radio's index of the shared library as the server reads one (`app::radio`): songs by their album's id as text, then track,
/// albums and artists numbered as met, the facts on top.
fn read_as_the_server_does(lib: &Shared, moved: &HashMap<String, String>, member: &HashMap<String, Vec<u32>>) -> Index {
    let mut files: Vec<&File> = lib.files.iter().collect();
    files.sort_by_key(|f| (moved[&f.album].clone(), f.track));
    let (mut albums, mut artists) = (Numbers::default(), Numbers::default());
    let songs = files
        .into_iter()
        .map(|f| {
            let id = song(f.track);
            let like = Track { playlists: member.get(&id).cloned().unwrap_or_default(), artists: f.artists.iter().map(|a| artists.of(&lib.artist_ids[a])).collect(), ..Track::default() };
            Song { id, title: f.rel.clone(), album: f.album.clone(), album_id: moved[&f.album].clone(), album_n: albums.of(&f.album), like, folders: vec![f.folder], ..Song::default() }
        })
        .collect();
    Index::new(songs).with_facts(&lib.facts())
}

#[test]
fn radio_picks_through_the_index_as_from_the_servers_read() {
    let lib: Shared = serde_json::from_str(LIBRARY).unwrap();
    let cases: Vec<RadioCase> = serde_json::from_str(RADIO).unwrap();
    let (index, moved) = held(&lib);
    let mut lists = Numbers::default();
    let member: HashMap<String, Vec<u32>> =
        lib.files.iter().filter_map(|f| Some((song(f.track), lib.playlists.get(&f.rel)?.iter().map(|p| lists.of(p)).collect()))).collect();
    let (device, server) = (index.radio(&facts_held(&index, &lib), &member), read_as_the_server_does(&lib, &moved, &member));
    assert!(cases.len() >= 100);
    for c in &cases {
        assert_eq!(radio_answer(&device, c), radio_answer(&server, c), "{}", c.name);
    }
}

// --- The files a device is tested on (web/tests, through WebAssembly): a small library before and after the feed ---

/// What a device holds and is sent of one small library.
struct Device {
    before: index::Index,
    changes: index::Changes,
    after: index::Index,
    sounds: index::Sounds,
    sound_changes: index::SoundChanges,
    sounds_after: index::Sounds,
    own: index::Marks,
    /// A second account's on the same device (decision 0101 point 28): his own, none of hers.
    own_bob: index::Marks,
}

fn device_library() -> Device {
    let library = |id: i64, key: &str, name: &str, kind, colour: &str, owner: Option<&str>| Library {
        id,
        key: key.into(),
        name: name.into(),
        kind,
        order: id as u32,
        colour: Some(colour.into()),
        owner: owner.map(str::to_string),
    };
    let artist = |id: i64, name: &str, sort: &str| Artist { id, name: name.into(), sort: sort.into(), spelled: Vec::new() };
    let copy = |library: i64, dir: &str, format: &str, min_kbps: u32, bits: Option<u32>, size: u64, tracks: u32| index::RecordCopy {
        library,
        dir: dir.into(),
        format: Some(format.into()),
        min_kbps: Some(min_kbps),
        bits,
        rate: bits.map(|_| 44_100),
        size: Some(size),
        added: Some(1_000 + library),
        tracks,
    };
    let track = |id: i64, record: i64, number: u32, title: &str, artists: &[i64], rel: &str, folders: &[i64], length: Option<f64>| index::Track {
        id,
        record,
        disc: Some(1),
        number: Some(number),
        title: title.into(),
        artist: Some(if artists.len() > 1 { "Ana Bell & The Quiet".into() } else { "Ana Bell".into() }),
        artist_ids: artists.to_vec(),
        facts: Facts { rel: rel.into(), folders: folders.to_vec(), born: Some(1_000 + id), length, year: Some(2001.0), bpm: Some(120.0), ..Facts::default() },
    };
    let libraries = vec![
        library(1, "cratarr//music", "Music", index::Kind::Main, "#aa3322", None),
        library(2, "cratarr//in coming", "Incoming", index::Kind::Incoming, "#22aa33", None),
        library(3, "own/ann", "Ann's music", index::Kind::Own, "#3322aa", Some("ann")),
    ];
    let first_light = index::Record {
        id: 10,
        title: "First Light".into(),
        sort: "first light".into(),
        artist: "Ana Bell".into(),
        artist_ids: vec![1],
        year: Some(2001),
        original_year: Some(1999),
        added: Some(1_001),
        kind: Some("album".into()),
        secondary: vec!["live".into()],
        genres: vec!["Jazz".into(), "Soul".into()],
        copies: vec![copy(1, "Ana Bell/First Light", "FLAC", 900, Some(16), 5_000, 2), copy(2, "First Light", "MP3", 320, None, 2_000, 2)],
        ..index::Record::default()
    };
    let second_wind = index::Record {
        id: 11,
        title: "Second Wind".into(),
        sort: "second wind".into(),
        artist: "Ana Bell & The Quiet".into(),
        artist_ids: vec![1, 2],
        year: Some(2003),
        added: Some(1_002),
        kind: Some("ep".into()),
        whole_tracks: Some(3),
        copies: vec![copy(1, "Ana Bell/Second Wind", "FLAC", 700, Some(24), 3_000, 2)],
        ..index::Record::default()
    };
    let own = index::Record {
        id: 12,
        title: "Own Thing".into(),
        sort: "own thing".into(),
        artist: "Mo".into(),
        artist_ids: vec![3],
        copies: vec![copy(3, "Mo/Own Thing", "OPUS", 128, None, 900, 1)],
        ..index::Record::default()
    };
    let new_record = index::Record {
        id: 13,
        title: "New Day".into(),
        sort: "new day".into(),
        artist: "Vera".into(),
        artist_ids: vec![4],
        added: Some(2_000),
        copies: vec![copy(1, "Vera/New Day", "FLAC", 800, Some(16), 1_000, 1)],
        ..index::Record::default()
    };
    let (dawn, daybreak) = (
        track(100, 10, 1, "Dawn", &[1], "Ana Bell/First Light/01.flac", &[1, 2], Some(200.5)),
        track(100, 10, 1, "Daybreak", &[1], "Ana Bell/First Light/01.flac", &[1, 2], Some(200.5)),
    );
    let noon = track(101, 10, 2, "Noon", &[1], "Ana Bell/First Light/02.flac", &[1, 2], Some(180.0));
    let gust = track(102, 11, 1, "Gust", &[1, 2], "Ana Bell/Second Wind/01.flac", &[1], Some(90.25));
    let calm = track(103, 11, 2, "Calm", &[1, 2], "Ana Bell/Second Wind/02.flac", &[1], None);
    let mine = track(104, 12, 1, "Mine", &[3], "../own/Mo/Own Thing/01.opus", &[3], Some(60.0));
    let fresh = track(105, 13, 1, "Fresh", &[4], "Vera/New Day/01.flac", &[1], Some(30.0));
    let artists = vec![artist(1, "Ana Bell", "bell, ana"), artist(2, "The Quiet", "quiet, the"), artist(3, "Mo", "mo")];
    let vera = artist(4, "Vera", "vera");
    let before = Rows {
        libraries: libraries.clone(),
        artists: artists.clone(),
        records: vec![first_light.clone(), second_wind.clone(), own.clone()],
        tracks: vec![dawn, noon.clone(), gust.clone(), calm, mine.clone()],
    };
    let after = Rows {
        libraries: libraries.clone(),
        artists: artists.iter().cloned().chain([vera.clone()]).collect(),
        records: vec![first_light.clone(), second_wind, own, new_record.clone()],
        tracks: vec![daybreak.clone(), noon, gust, mine, fresh.clone()],
    };
    // A batch brings what its rows name, as the server's does.
    let set = Rows { libraries, artists: vec![artists[0].clone(), vera], records: vec![first_light, new_record], tracks: vec![daybreak, fresh] };
    let changes = index::Changes::new(5, 9, &set, index::Gone { libraries: Vec::new(), tracks: vec![103] });
    let sound = |x: f32| vec![x, 1.0 - x, 0.5, -x];
    let (sounds, sounds_after) =
        (index::Sounds::build(5, &[(100, sound(0.25)), (101, sound(0.5)), (103, sound(0.75))]), index::Sounds::build(9, &[(100, sound(0.25)), (101, sound(0.625)), (105, sound(0.125))]));
    let sound_changes = sounds_after.changes(5, &[101, 103, 105]);
    let hearted = Kept { id: "hearted".into(), name: "Hearted".into(), conditions: vec![Condition::new("favourite", false, Value::Yes)], listed: true, ..Kept::default() };
    let own = Own { favourites: vec![101, 102], crates: vec![hearted.clone()], lists: vec![List { id: "pl-1".into(), name: "Mix".into(), tracks: vec![100] }], ..Own::default() };
    let marks = [index::Mark { track: 102, fit: Some(0.9), ..index::Mark::default() }, index::Mark { track: 101, fit: Some(0.25), plays: 2, last: Some(1_700_000_000), ..index::Mark::default() }];
    let own = index::Marks::build(1_800_000_000, &marks).with_own(serde_json::to_string(&own).unwrap());
    // The same crate saved by another account, who hearted another track and played it.
    let bob = Own { favourites: vec![100], crates: vec![hearted], ..Own::default() };
    let marks = [index::Mark { track: 100, fit: Some(0.75), plays: 1, last: Some(1_750_000_000), ..index::Mark::default() }];
    let own_bob = index::Marks::build(1_800_000_000, &marks).with_own(serde_json::to_string(&bob).unwrap());
    Device { before: index::Index::build(5, &before).unwrap(), changes, after: index::Index::build(9, &after).unwrap(), sounds, sound_changes, sounds_after, own, own_bob }
}

/// Where those files lie.
fn device_files() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/index")
}

#[test]
fn the_files_a_device_is_tested_on_are_what_this_library_makes_them() {
    let (d, dir) = (device_library(), device_files());
    let read = |name: &str| std::fs::read(dir.join(name)).unwrap();
    assert_eq!(read("whole.bin"), d.before.save());
    assert_eq!(read("changes.bin"), d.changes.save());
    assert_eq!(read("after.bin"), d.after.save());
    assert_eq!(read("sounds.bin"), d.sounds.save());
    assert_eq!(read("sound-changes.bin"), d.sound_changes.save());
    assert_eq!(read("sounds-after.bin"), d.sounds_after.save());
    assert_eq!(read("own.bin"), d.own.save());
    assert_eq!(read("own-bob.bin"), d.own_bob.save());
    let text = |name: &str| String::from_utf8(read(name)).unwrap().replace('\r', "");
    assert_eq!(text("list.json"), d.before.list_json() + "\n");
    assert_eq!(text("record.json"), d.before.record_json(10).unwrap() + "\n");
}

#[test]
fn the_feed_of_those_files_applied_is_the_index_and_the_sounds_after_it_to_the_byte() {
    let mut d = device_library();
    d.before.apply(&d.changes).unwrap();
    assert_eq!(d.before.save(), d.after.save());
    d.sounds.apply(&d.sound_changes).unwrap();
    assert_eq!(d.sounds.save(), d.sounds_after.save());
}

#[test]
#[ignore = "writes the vectors: only when a rule changes on purpose"]
fn write() {
    if std::env::var("CRATARR_RULES_WRITE").as_deref() != Ok("1") {
        return;
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors");
    let save = |name: &str, v: serde_json::Value| std::fs::write(dir.join(name), serde_json::to_string_pretty(&v).unwrap() + "\n").unwrap();
    let (d, files) = (device_library(), device_files());
    std::fs::create_dir_all(&files).unwrap();
    for (name, bytes) in [
        ("whole.bin", d.before.save()),
        ("changes.bin", d.changes.save()),
        ("after.bin", d.after.save()),
        ("sounds.bin", d.sounds.save()),
        ("sound-changes.bin", d.sound_changes.save()),
        ("sounds-after.bin", d.sounds_after.save()),
        ("own.bin", d.own.save()),
        ("own-bob.bin", d.own_bob.save()),
        ("list.json", (d.before.list_json() + "\n").into_bytes()),
        ("record.json", (d.before.record_json(10).unwrap() + "\n").into_bytes()),
    ] {
        std::fs::write(files.join(name), bytes).unwrap();
    }
    // The answers come from the library as written, so they are what reading it back gives on the last bit.
    let written = by_lines(&serde_json::to_value(shared_library()).unwrap()) + "\n";
    std::fs::write(dir.join("library.json"), &written).unwrap();
    let lib: Shared = serde_json::from_str(&written).unwrap();
    let (files, index) = (lib.facts(), lib.index());
    let alike = lib.alike(&files);
    let mut asked = crate_cases(&lib);
    asked.iter_mut().for_each(|c| c.expected = crate_answer(&lib, &files, &alike, c));
    std::fs::write(dir.join("crates.json"), by_line(&serde_json::to_value(asked).unwrap()) + "\n").unwrap();
    let mut picks = radio_cases(&lib);
    picks.iter_mut().for_each(|c| c.expected = radio_answer(&index, c));
    std::fs::write(dir.join("radio.json"), by_line(&serde_json::to_value(picks).unwrap()) + "\n").unwrap();
    save("rng.json", serde_json::to_value([rng_case(0), rng_case(1), rng_case(7), rng_case(123_456_789)]).unwrap());
    save(
        "pick.json",
        serde_json::to_value([
            pick_case("play: the first ones, in order", 20, How::Play, None, 8, 1),
            pick_case("play from a track far from the end", 20, How::Play, Some(5), 8, 1),
            pick_case("play from a track near the end: those before fill up", 20, How::Play, Some(17), 8, 1),
            pick_case("play from a track not in it: from the start", 5, How::Play, Some(9), 8, 1),
            pick_case("play next takes them in order", 20, How::Next, None, 8, 1),
            pick_case("add to queue takes them in order", 20, How::Last, Some(3), 8, 1),
            pick_case("play over the cap: the first 500", 600, How::Play, Some(450), 500, 1),
            pick_case("shuffle draws from all of it", 20, How::Shuffle, None, 8, 7),
            pick_case("shuffle from a track: it first", 20, How::Shuffle, Some(13), 8, 7),
            pick_case("a small collection shuffles whole", 6, How::Shuffle, None, 500, 3),
            pick_case("nothing to pick", 0, How::Shuffle, None, 500, 3),
        ])
        .unwrap(),
    );
    save(
        "shuffle_order.json",
        serde_json::to_value([
            shuffle_case("own tracks only", "wwwwwwww", Some(0), 5),
            shuffle_case("hand-added, inserts and Autoplay's tail stay", "wwnnrrwwwxwwaa", Some(1), 11),
            shuffle_case("the playing track partway in", "wwwwwwww", Some(3), 2),
            shuffle_case("nothing plays", "wwww", None, 1),
            shuffle_case("one own track to come", "wwnaa", Some(0), 1),
        ])
        .unwrap(),
    );
    save(
        "add_place.json",
        serde_json::to_value([
            add_case("play next: right after the playing one", "wwnnwa", Some(1), ByHand::Next),
            add_case("add to queue: after those added by hand", "wwnnwa", Some(1), ByHand::Last),
            add_case("add to queue with none added", "wwwa", Some(0), ByHand::Last),
            add_case("nothing plays: at the end", "www", None, ByHand::Next),
        ])
        .unwrap(),
    );
    save(
        "restore_order.json",
        serde_json::to_value([
            restore_case("after a clear, Autoplay's in between: all back", &["a", "b", "x", "y"], Some(2), &["c", "d", "e"]),
            restore_case("after a shuffle, two played since: the rest back", &["a", "b", "d", "c", "e"], Some(3), &["c", "d", "e"]),
            restore_case("nothing played since", &["a", "b", "e", "c", "d"], Some(1), &["c", "d", "e"]),
        ])
        .unwrap(),
    );
}
