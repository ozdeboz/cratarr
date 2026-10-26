//! Insights: figures about the Music library and its playlists, and how close playlists are (the old app's Statistics
//! and Proximity). Pure over plain rows: the catalogue's files (`Catalogue::track_rows` on the Music folder) and the
//! playlists' lines. Tags are split per the saved rules, as everywhere (decision 0008).
//!
//! Closeness has three signals, each 0..1, which the page turns on and off: shared tracks (shared over all, so a big
//! playlist is not close to everything), and genre and artist profiles compared by cosine, where a genre or artist
//! found in many playlists counts less (tf-idf: "Electronic" says little, "Downtempo" more).

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use crate::catalogue::{Rules, TrackRow};
use crate::incoming::{Quality, quality_of};
use crate::playlists::playlist_stats;

/// Genres and artists: every one, so that each way the page ranks them (by tracks, by those in a playlist, A–Z) is
/// the top of the whole set, not of another ranking's top (the owner, 2026-10-23).
const TOP_GENRES: usize = usize::MAX;
const TOP_ARTISTS: usize = usize::MAX;
/// A pair is sent when one of its signals reaches this; the page decides what to draw.
const MIN_SIGNAL: f64 = 0.05;
/// Genres named per playlist and per pair.
const NAMED_GENRES: usize = 3;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Insights {
    pub library: LibraryStats,
    pub playlists: PlaylistInsights,
    pub proximity: Proximity,
    /// None where Cratarr's own library is not open.
    pub overview: Option<Overview>,
}

/// The playlists as points, and the pairs that are close in at least one way.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Proximity {
    pub playlists: Vec<Point>,
    /// Indexes into `playlists`.
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Point {
    pub name: String,
    /// Different tracks.
    pub tracks: usize,
    /// Its most characteristic genres.
    pub genres: Vec<String>,
    /// How concentrated its genres are, 0..1: high for a well defined playlist (one style), low for a grab-bag.
    /// `None` without genre tags.
    pub focus: Option<f64>,
    /// Each genre's share of its tracks' genres, summing to 1 (0.9.8: the Map's genre territories), the rules' aliases
    /// and merges applied; those under [`LEAST_SHARE`] left out. Plain shares, not the characteristic weights `genres`
    /// is chosen by: a genre every playlist has still covers its ground.
    pub shares: BTreeMap<String, f64>,
}

/// A genre's least share of a playlist that is told (the page draws a territory from 0.25).
pub const LEAST_SHARE: f64 = 0.05;

/// A profile's shares of its total, those under [`LEAST_SHARE`] left out, to three places.
fn shares_of(profile: &HashMap<String, f64>) -> BTreeMap<String, f64> {
    let total: f64 = profile.values().sum();
    if total <= 0.0 {
        return BTreeMap::new();
    }
    profile.iter().map(|(g, n)| (g.clone(), n / total)).filter(|(_, s)| *s >= LEAST_SHARE).map(|(g, s)| (g, (s * 1000.0).round() / 1000.0)).collect()
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Link {
    pub a: usize,
    pub b: usize,
    /// Shared tracks over all tracks of the two, 0..1.
    pub tracks: f64,
    /// Genre profiles alike, 0..1.
    pub genre: f64,
    /// Artist profiles alike, 0..1.
    pub artists: f64,
    pub shared_tracks: usize,
    pub shared_artists: usize,
    /// The genres that bring the two closest, most first.
    pub genres: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct LibraryStats {
    pub tracks: usize,
    /// Album folders.
    pub albums: usize,
    /// Different album artists.
    pub artists: usize,
    pub duration_ms: u64,
    pub bytes: u64,
    /// Tracks per format, most first.
    pub formats: Vec<Count>,
    /// Albums whose every track is lossless.
    pub lossless_albums: usize,
    /// Lossy albums with a track under 320 kbps: the upgrade candidates.
    pub below_320_albums: usize,
    /// Albums per decade of their (original) release, oldest first: "1970s".
    pub decades: Vec<Count>,
    /// Albums per year of their (original) release, oldest first: "1997" (a decade's columns, opened from its own).
    pub years: Vec<Count>,
    /// Tracks per genre (a track's own genres), and how many of them are in a playlist; most first.
    pub genres: Vec<Layered>,
    /// Tracks per album artist (else the artist every track of the album shares), and how many of them are in a
    /// playlist; most first.
    pub top_artists: Vec<Layered>,
}

/// Collection's overview beyond the files (the owner, 2026-10-23): what waits in Incoming, how the libraries switched
/// on grew, and how much of them the asking account has heard. From Cratarr's own library and the account's plays and
/// stars.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Overview {
    /// Albums in an Incoming folder, and when the one there longest came (epoch s).
    pub incoming_albums: usize,
    pub incoming_since: Option<i64>,
    /// Albums of the libraries switched on that came in each step of the period (the owner, 2026-10-23: bound by it):
    /// a week's days ("2026-10-23"), a month's weeks (their Mondays), a year's months ("2026-10"), all's years
    /// ("2026", since the first that came); oldest first, empty steps too. An album with no known arrival is not
    /// counted (nothing is made up).
    pub added: Vec<Count>,
    /// "day", "week", "month" or "year".
    pub added_step: &'static str,
    /// Tracks of the libraries switched on, those played at least once, albums none of whose tracks were, and the
    /// tracks starred.
    pub tracks: usize,
    pub tracks_heard: usize,
    pub albums: usize,
    pub albums_never: usize,
    pub favourites: usize,
    /// Tracks by how often they were played (the account's play counts): "0", "1", "2–4", "5–9", "10–19", "20+"; and
    /// the most played (song id and plays; the service names them), most first (Tracks, beside the playlists'; the
    /// owner, 2026-10-23).
    pub plays: Vec<Count>,
    pub most_played: Vec<PlayedTrack>,
}

/// A track played often: what it is, and how often.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct PlayedTrack {
    #[serde(skip)]
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub plays: u32,
}

/// The play counts' columns: each the least plays it holds and its name.
const PLAY_BUCKETS: [(u32, &str); 6] = [(0, "0"), (1, "1"), (2, "2–4"), (5, "5–9"), (10, "10–19"), (20, "20+")];
/// At most this many in Most played.
const MOST_PLAYED: usize = 10;

/// Growth's steps: of a day, a week, a month or a year; how many (None: years since the first album came).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    Day,
    Week,
    Month,
    Year,
}

/// The overview from the library's albums (id, folder, arrival) and songs (id, album), the folders switched on, the
/// Incoming folders, and the account's play counts and starred songs; `now` and `tz` (seconds east of UTC) for the months.
#[allow(clippy::too_many_arguments)]
pub fn overview(
    albums: &[(String, i64, Option<i64>)],
    songs: &[(String, String, i64, String)],
    shown: &HashSet<i64>,
    incoming: &HashSet<i64>,
    plays: &HashMap<String, u32>,
    starred: &HashSet<String>,
    since: Option<i64>,
    growth: (Step, Option<i64>),
    now: i64,
    tz: i64,
) -> Overview {
    let mut out = Overview::default();
    let inc: Vec<Option<i64>> = albums.iter().filter(|(_, f, _)| incoming.contains(f)).map(|(_, _, c)| *c).collect();
    out.incoming_albums = inc.len();
    out.incoming_since = inc.into_iter().flatten().min();
    // Each step as a number, local time: days since 1970, Mondays (as days), year * 12 + month0, years.
    let (step, count) = growth;
    let key = |t: i64| {
        let day = (t + tz).div_euclid(86_400);
        let (y, m, _) = civil_of(day);
        match step {
            Step::Day => day,
            Step::Week => day - (day + 3).rem_euclid(7),
            Step::Month => y * 12 + i64::from(m) - 1,
            Step::Year => y,
        }
    };
    let back = |k: i64, n: i64| if step == Step::Week { k - 7 * n } else { k - n };
    let shown_albums: HashSet<&str> = albums.iter().filter(|(_, f, _)| shown.contains(f)).map(|(id, _, _)| id.as_str()).collect();
    let this = key(now);
    let first = match count {
        Some(n) => back(this, n - 1),
        None => albums.iter().filter(|(id, _, _)| shown_albums.contains(id.as_str())).filter_map(|(_, _, c)| c.map(key)).min().unwrap_or(this).min(this),
    };
    let mut added: BTreeMap<i64, usize> = BTreeMap::new();
    let mut k = this;
    while k >= first {
        added.insert(k, 0);
        k = back(k, 1);
    }
    // Growth and Heard both of the period (`since`: the albums that came then).
    for (_, _, created) in albums.iter().filter(|(id, _, _)| shown_albums.contains(id.as_str())) {
        if let Some(n) = created.and_then(|c| added.get_mut(&key(c))) {
            *n += 1;
        }
    }
    let name = |k: i64| match step {
        Step::Day | Step::Week => {
            let (y, m, d) = civil_of(k);
            format!("{y}-{m:02}-{d:02}")
        }
        Step::Month => format!("{}-{:02}", k.div_euclid(12), k.rem_euclid(12) + 1),
        Step::Year => k.to_string(),
    };
    out.added = added.into_iter().map(|(k, n)| Count { name: name(k), n }).collect();
    out.added_step = match step {
        Step::Day => "day",
        Step::Week => "week",
        Step::Month => "month",
        Step::Year => "year",
    };
    let period_albums: HashSet<&str> = albums
        .iter()
        .filter(|(id, _, c)| shown_albums.contains(id.as_str()) && since.is_none_or(|s| c.is_some_and(|c| c >= s)))
        .map(|(id, _, _)| id.as_str())
        .collect();
    let mut heard_albums: HashSet<&str> = HashSet::new();
    let mut buckets = [0usize; PLAY_BUCKETS.len()];
    let mut most: Vec<(&str, u32)> = Vec::new();
    for (id, album, _, _) in songs.iter().filter(|(_, a, _, _)| period_albums.contains(a.as_str())) {
        out.tracks += 1;
        let n = plays.get(id).copied().unwrap_or(0);
        if n > 0 {
            out.tracks_heard += 1;
            heard_albums.insert(album);
            most.push((id, n));
        }
        buckets[PLAY_BUCKETS.iter().rposition(|(least, _)| n >= *least).unwrap_or(0)] += 1;
        out.favourites += usize::from(starred.contains(id));
    }
    out.plays = PLAY_BUCKETS.iter().zip(buckets).map(|((_, name), n)| Count { name: (*name).to_string(), n }).collect();
    most.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    out.most_played = most.into_iter().take(MOST_PLAYED).map(|(id, plays)| PlayedTrack { id: id.to_string(), plays, ..PlayedTrack::default() }).collect();
    out.albums = period_albums.len();
    out.albums_never = out.albums - heard_albums.len();
    out
}

/// The civil date (year, month 1–12, day) of a day since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_of(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// A genre's or an artist's tracks, and those of them in a playlist: Collection's bars in two layers (the owner,
/// 2026-10-23: "tracks and playlists").
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Layered {
    pub name: String,
    /// Tracks.
    pub n: usize,
    /// Of them, in one of the playlists or more.
    pub listed: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Count {
    pub name: String,
    pub n: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct PlaylistInsights {
    pub playlists: usize,
    /// Different tracks across all playlists.
    pub distinct_tracks: usize,
    /// Tracks in exactly one playlist.
    pub in_one_playlist: usize,
    /// Missing tracks, summed over playlists.
    pub missing: usize,
    /// Music tracks in at least one playlist.
    pub library_tracks_in_playlists: usize,
    /// The same tracks' in one of the account's own crates (not Unlisted); set by the service, which asks the crates.
    pub library_tracks_in_crates: usize,
    /// Albums with no track in any playlist.
    pub albums_without_playlist: usize,
    /// The tracks in the most playlists.
    pub most_shared: Vec<SharedTrack>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct SharedTrack {
    pub title: String,
    pub artist: Option<String>,
    pub playlists: usize,
}

/// Everything on the Insights page. `rows`: the Music folder's tracks, which the playlists are counted against;
/// `shown`: the tracks of the libraries switched on, which Collection's figures count (the owner, 2026-10-23: "one
/// toggle per library"), its playlists' among them: how many of those tracks and albums are in one, and which of them
/// are in the most (Incoming's are in none: a playlist's lines are the Music folder's). `playlists`: (name, entry lines, missing tracks), as the playlist store has them.
pub fn build(rows: &[TrackRow], shown: &[TrackRow], on: &[TrackRow], rules: &Rules, playlists: &[(String, Vec<String>, usize)]) -> Insights {
    let in_any: HashSet<&str> = playlists.iter().flat_map(|(_, l, _)| l.iter().map(String::as_str)).collect();
    let library = library_stats(shown, rules, &in_any);

    let borrowed: Vec<(&str, &[String], usize)> = playlists.iter().map(|(n, l, d)| (n.as_str(), l.as_slice(), *d)).collect();
    let stats = playlist_stats(&borrowed);
    let by_rel: HashMap<&str, &TrackRow> = rows.iter().map(|r| (r.rel.as_str(), r)).collect();
    let shown_rels: HashSet<&str> = shown.iter().map(|r| r.rel.as_str()).collect();
    let albums_with: HashSet<&str> = shown.iter().filter(|r| in_any.contains(r.rel.as_str())).map(|r| album_dir(&r.rel)).collect();
    let most_shared = stats
        .most_shared
        .iter()
        .filter(|(rel, _)| shown_rels.contains(rel.as_str()))
        .map(|(rel, n)| {
            let row = by_rel.get(rel.as_str());
            let title = row.and_then(|r| r.tag("title").next()).map(str::to_string).unwrap_or_else(|| file_name(rel));
            let artist = row.and_then(|r| r.tag("artist").next()).map(str::to_string);
            SharedTrack { title, artist, playlists: *n }
        })
        .collect();
    let playlists_view = PlaylistInsights {
        playlists: stats.playlists,
        distinct_tracks: stats.distinct_tracks,
        in_one_playlist: stats.in_one_playlist,
        missing: stats.missing,
        library_tracks_in_playlists: shown.iter().filter(|r| in_any.contains(r.rel.as_str())).count(),
        library_tracks_in_crates: 0,
        albums_without_playlist: library.albums - albums_with.len(),
        most_shared,
    };

    // The map by the libraries switched on too (the switches are all of Insights', the owner, 2026-10-23): each playlist
    // by its tracks there (a missing track still counts, as before); a playlist with none there is not on it. Not by
    // the period (`on`: the libraries' tracks, whenever they came).
    let music_rels: HashSet<&str> = rows.iter().map(|r| r.rel.as_str()).collect();
    let on_rels: HashSet<&str> = on.iter().map(|r| r.rel.as_str()).collect();
    let mapped: Vec<(String, Vec<String>, usize)> = playlists
        .iter()
        .filter(|(_, l, _)| l.iter().any(|x| on_rels.contains(x.as_str())))
        .map(|(n, l, d)| (n.clone(), l.iter().filter(|x| on_rels.contains(x.as_str()) || !music_rels.contains(x.as_str())).cloned().collect(), *d))
        .collect();
    Insights { library, playlists: playlists_view, proximity: proximity(rows, rules, &mapped), overview: None }
}

/// How close every two playlists are, by tracks, genre and artists.
pub fn proximity(rows: &[TrackRow], rules: &Rules, playlists: &[(String, Vec<String>, usize)]) -> Proximity {
    let by_rel: HashMap<&str, &TrackRow> = rows.iter().map(|r| (r.rel.as_str(), r)).collect();
    let tracks: Vec<HashSet<&str>> = playlists.iter().map(|(_, l, _)| l.iter().map(String::as_str).collect()).collect();
    // Per playlist: how many of its tracks carry each genre / artist.
    let profile = |field: &str| -> Vec<HashMap<String, f64>> {
        tracks
            .iter()
            .map(|set| {
                let mut m: HashMap<String, f64> = HashMap::new();
                for rel in set {
                    let Some(r) = by_rel.get(rel) else { continue };
                    let values = if field == "artist" {
                        r.artists(rules, field)
                    } else {
                        rules.values(field, &r.tag(field).map(str::to_string).collect::<Vec<_>>())
                    };
                    for v in values.into_iter().collect::<HashSet<_>>() {
                        *m.entry(v).or_default() += 1.0;
                    }
                }
                m
            })
            .collect()
    };
    let counts = profile("genre");
    let shares: Vec<BTreeMap<String, f64>> = counts.iter().map(shares_of).collect();
    let genres = tf_idf(counts);
    let artists = tf_idf(profile("artist"));
    // Compared without what only one playlist has: it can match nothing and would only lower that playlist's
    // closeness to every other (a reggae playlist alone with Ska and Rocksteady looked unlike everything).
    let genres_cmp = shared(&genres);
    let artists_cmp = shared(&artists);
    let points = playlists
        .iter()
        .enumerate()
        .map(|(i, (name, _, _))| Point {
            name: name.clone(),
            tracks: tracks[i].len(),
            genres: strongest(&genres[i], NAMED_GENRES),
            focus: focus(&genres[i]),
            shares: shares[i].clone(),
        })
        .collect();
    let mut links = Vec::new();
    for a in 0..playlists.len() {
        for b in a + 1..playlists.len() {
            let shared_tracks = tracks[a].intersection(&tracks[b]).count();
            let union = tracks[a].len() + tracks[b].len() - shared_tracks;
            let t = if union == 0 { 0.0 } else { shared_tracks as f64 / union as f64 };
            let g = cosine(&genres_cmp[a], &genres_cmp[b]);
            let r = cosine(&artists_cmp[a], &artists_cmp[b]);
            if t.max(g).max(r) < MIN_SIGNAL {
                continue;
            }
            let mut both: Vec<(String, f64)> =
                genres[a].iter().filter_map(|(k, x)| genres[b].get(k).map(|y| (k.clone(), x * y))).collect();
            both.sort_by(|x, y| y.1.total_cmp(&x.1).then_with(|| x.0.cmp(&y.0)));
            links.push(Link {
                a,
                b,
                tracks: t,
                genre: g,
                artists: r,
                shared_tracks,
                shared_artists: artists[a].keys().filter(|k| artists[b].contains_key(*k)).count(),
                genres: both.into_iter().take(NAMED_GENRES).map(|(k, _)| k).collect(),
            });
        }
    }
    Proximity { playlists: points, links }
}

/// The profiles without the keys found in one profile only.
fn shared(profiles: &[HashMap<String, f64>]) -> Vec<HashMap<String, f64>> {
    let mut df: HashMap<&str, usize> = HashMap::new();
    for p in profiles {
        for k in p.keys() {
            *df.entry(k.as_str()).or_default() += 1;
        }
    }
    profiles.iter().map(|p| p.iter().filter(|(k, _)| df[k.as_str()] > 1).map(|(k, v)| (k.clone(), *v)).collect()).collect()
}

/// Counts weighted by rarity across the playlists: idf = ln(N / playlists having it), so what every playlist has
/// weighs nothing.
fn tf_idf(profiles: Vec<HashMap<String, f64>>) -> Vec<HashMap<String, f64>> {
    let n = profiles.len() as f64;
    let mut df: HashMap<String, f64> = HashMap::new();
    for p in &profiles {
        for k in p.keys() {
            *df.entry(k.clone()).or_default() += 1.0;
        }
    }
    profiles
        .into_iter()
        .map(|p| {
            p.into_iter()
                .map(|(k, tf)| {
                    let w = tf * (n / df[&k]).ln();
                    (k, w)
                })
                .filter(|(_, w)| *w > 0.0)
                .collect()
        })
        .collect()
}

fn cosine(a: &HashMap<String, f64>, b: &HashMap<String, f64>) -> f64 {
    let dot: f64 = a.iter().filter_map(|(k, x)| b.get(k).map(|y| x * y)).sum();
    let norm = |m: &HashMap<String, f64>| m.values().map(|v| v * v).sum::<f64>().sqrt();
    let d = norm(a) * norm(b);
    if d == 0.0 { 0.0 } else { (dot / d).clamp(0.0, 1.0) }
}

/// Concentration of a weighted profile (Simpson's index over its shares): 1 when one genre carries all the weight,
/// near 0 when it is spread over many. Over the tf-idf weights, so a genre every playlist has (\"Electronic\") does not
/// make a grab-bag look focused.
fn focus(profile: &HashMap<String, f64>) -> Option<f64> {
    let total: f64 = profile.values().sum();
    (total > 0.0).then(|| profile.values().map(|w| (w / total).powi(2)).sum())
}

/// The `k` heaviest keys, most first.
fn strongest(m: &HashMap<String, f64>, k: usize) -> Vec<String> {
    let mut v: Vec<(&String, &f64)> = m.iter().collect();
    v.sort_by(|x, y| y.1.total_cmp(x.1).then_with(|| x.0.cmp(y.0)));
    v.into_iter().take(k).map(|(s, _)| s.clone()).collect()
}

/// The library figures, counted per track and per album folder; `listed`: the tracks in a playlist (their lines).
pub fn library_stats(rows: &[TrackRow], rules: &Rules, listed: &HashSet<&str>) -> LibraryStats {
    #[derive(Default)]
    struct Album<'a> {
        formats: Vec<&'a str>,
        min_kbps: Option<u32>,
        year: Option<u32>,
        genres: HashSet<String>,
        album_artists: HashSet<String>,
        /// Artists every track has, for albums without an album artist tag.
        common_artists: Option<HashSet<String>>,
        /// Its tracks, and those of them in a playlist.
        tracks: usize,
        listed: usize,
    }
    let mut genre_tracks: HashMap<String, (usize, usize)> = HashMap::new();
    let mut albums: BTreeMap<&str, Album> = BTreeMap::new();
    let mut formats: HashMap<String, usize> = HashMap::new();
    let mut out = LibraryStats { tracks: rows.len(), ..LibraryStats::default() };
    for r in rows {
        out.duration_ms += r.duration_ms.unwrap_or(0);
        out.bytes += r.size;
        *formats.entry(r.format.clone().unwrap_or_else(|| "Other".into())).or_default() += 1;
        let a = albums.entry(album_dir(&r.rel)).or_default();
        a.formats.extend(r.format.as_deref());
        if let Some(k) = r.kbps {
            a.min_kbps = Some(a.min_kbps.map_or(k, |m| m.min(k)));
        }
        let year = r.tag("originaldate").chain(r.tag("date")).find_map(year_of);
        if let Some(y) = year {
            a.year = Some(a.year.map_or(y, |m| m.min(y)));
        }
        let values = |field: &str| rules.values(field, &r.tag(field).map(str::to_string).collect::<Vec<_>>());
        let here = listed.contains(r.rel.as_str());
        a.tracks += 1;
        a.listed += usize::from(here);
        let genres: HashSet<String> = values("genre").into_iter().collect();
        for g in &genres {
            let e = genre_tracks.entry(g.clone()).or_default();
            e.0 += 1;
            e.1 += usize::from(here);
        }
        a.genres.extend(genres);
        a.album_artists.extend(r.artists(rules, "albumartist"));
        let artists: HashSet<String> = r.artists(rules, "artist").into_iter().collect();
        a.common_artists = Some(match a.common_artists.take() {
            None => artists,
            Some(c) => c.intersection(&artists).cloned().collect(),
        });
    }
    out.albums = albums.len();
    let mut decades: BTreeMap<u32, usize> = BTreeMap::new();
    let mut years: BTreeMap<u32, usize> = BTreeMap::new();
    let mut artists: HashMap<String, usize> = HashMap::new();
    let mut artist_tracks: HashMap<String, (usize, usize)> = HashMap::new();
    for a in albums.values() {
        match quality_of(a.formats.iter().copied(), a.min_kbps) {
            Quality::Lossless => out.lossless_albums += 1,
            Quality::Lossy { kbps } if kbps < 320 => out.below_320_albums += 1,
            _ => {}
        }
        if let Some(y) = a.year {
            *decades.entry(y / 10 * 10).or_default() += 1;
            *years.entry(y).or_default() += 1;
        }
        let names = if a.album_artists.is_empty() { a.common_artists.clone().unwrap_or_default() } else { a.album_artists.clone() };
        // A compilation's "Various Artists" is no artist: as one, it dwarfed every real one's bar (the owner, 2026-10-23).
        for n in names.into_iter().filter(|n| !crate::crates::various(n)) {
            let e = artist_tracks.entry(n.clone()).or_default();
            e.0 += a.tracks;
            e.1 += a.listed;
            *artists.entry(n).or_default() += 1;
        }
    }
    out.artists = artists.len();
    out.formats = ranked(formats, usize::MAX);
    out.decades = decades.into_iter().map(|(d, n)| Count { name: format!("{d}s"), n }).collect();
    out.years = years.into_iter().map(|(y, n)| Count { name: y.to_string(), n }).collect();
    out.genres = layered(genre_tracks, TOP_GENRES);
    out.top_artists = layered(artist_tracks, TOP_ARTISTS);
    out
}

/// Most first, then by name; at most `top`.
fn ranked(counts: HashMap<String, usize>, top: usize) -> Vec<Count> {
    let mut v: Vec<Count> = counts.into_iter().map(|(name, n)| Count { name, n }).collect();
    v.sort_by(|a, b| b.n.cmp(&a.n).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    v.truncate(top);
    v
}

fn layered(counts: HashMap<String, (usize, usize)>, top: usize) -> Vec<Layered> {
    let mut v: Vec<Layered> = counts.into_iter().map(|(name, (n, listed))| Layered { name, n, listed }).collect();
    v.sort_by(|a, b| b.n.cmp(&a.n).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    v.truncate(top);
    v
}

fn album_dir(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(d, _)| d)
}

fn file_name(rel: &str) -> String {
    rel.rsplit_once('/').map_or(rel, |(_, f)| f).to_string()
}

/// The year at the start of a date tag ("1998", "1998-03-16"), when it is a plausible release year.
fn year_of(date: &str) -> Option<u32> {
    let y: u32 = date.trim().get(..4)?.parse().ok()?;
    (1900..=2100).contains(&y).then_some(y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(rel: &str, format: &str, kbps: u32, tags: &[(&str, &str)]) -> TrackRow {
        TrackRow {
            rel: rel.into(),
            format: Some(format.into()),
            kbps: Some(kbps),
            duration_ms: Some(200_000),
            size: 1_000,
            tags: tags.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect(),
        }
    }

    fn library() -> Vec<TrackRow> {
        vec![
            row("AIR - Moon Safari (1998)/01 Sexy Boy.flac", "FLAC", 900, &[("albumartist", "AIR"), ("artist", "AIR"), ("title", "Sexy Boy"), ("genre", "Electronic; Downtempo"), ("date", "1998-01-16")]),
            row("AIR - Moon Safari (1998)/02 Talisman.flac", "FLAC", 880, &[("albumartist", "AIR"), ("artist", "AIR"), ("title", "Talisman"), ("genre", "Electronic"), ("date", "1998")]),
            row("Bonobo - Black Sands (2010)/01 Prelude.mp3", "MP3", 192, &[("artist", "Bonobo"), ("title", "Prelude"), ("genre", "Downtempo"), ("originaldate", "2010"), ("date", "2012")]),
            row("Bonobo - Black Sands (2010)/02 Kiara.mp3", "MP3", 320, &[("artist", "Bonobo; Andreya Triana"), ("title", "Kiara"), ("genre", "Downtempo")]),
            row("Untagged/01 x.mp3", "MP3", 320, &[]),
        ]
    }

    #[test]
    fn library_figures_count_tracks_albums_and_split_tags() {
        let air1 = "AIR - Moon Safari (1998)/01 Sexy Boy.flac";
        let s = library_stats(&library(), &Rules::default(), &HashSet::from([air1]));
        assert_eq!((s.tracks, s.albums, s.duration_ms, s.bytes), (5, 3, 1_000_000, 5_000));
        assert_eq!(s.formats, vec![Count { name: "MP3".into(), n: 3 }, Count { name: "FLAC".into(), n: 2 }]);
        assert_eq!((s.lossless_albums, s.below_320_albums), (1, 1), "AIR lossless; Bonobo has a 192 kbps track");
        assert_eq!(s.decades, vec![Count { name: "1990s".into(), n: 1 }, Count { name: "2010s".into(), n: 1 }], "original date wins");
        assert_eq!(s.years.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["1998", "2010"], "each album once, by its year");
        // Tracks, by their own genres; Sexy Boy (in a playlist) is both.
        let l = |name: &str, n, listed| Layered { name: name.into(), n, listed };
        assert_eq!(s.genres, vec![l("Downtempo", 3, 1), l("Electronic", 2, 1)]);
        // Bonobo has no album artist tag: the artist every track shares counts, not the guest.
        assert_eq!(s.top_artists, vec![l("AIR", 2, 1), l("Bonobo", 2, 0)]);
        assert_eq!(s.artists, 2);
    }

    #[test]
    fn the_overview_counts_incoming_growth_and_what_was_heard() {
        let day = 86_400;
        let now = 1_792_800_000; // 2026-10-23
        let albums = vec![
            ("a1".to_string(), 0, Some(now - 10 * day)),
            ("a2".to_string(), 0, Some(now - 400 * day)),
            ("a3".to_string(), 0, None),
            ("i1".to_string(), 1, Some(now - 41 * day)),
            ("i2".to_string(), 1, Some(now - 3 * day)),
        ];
        let songs: Vec<(String, String, i64, String)> =
            [("s1", "a1"), ("s2", "a1"), ("s3", "a2"), ("s4", "a3"), ("s5", "i1")].iter().map(|(s, a)| (s.to_string(), a.to_string(), 0, format!("{a}/{s}"))).collect();
        let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<HashSet<String>>();
        let plays: HashMap<String, u32> = [("s1".to_string(), 3), ("s5".to_string(), 1)].into();
        let o = overview(&albums, &songs, &HashSet::from([0]), &HashSet::from([1]), &plays, &set(&["s3"]), None, (Step::Month, Some(24)), now, 0);
        assert_eq!((o.incoming_albums, o.incoming_since), (2, Some(now - 41 * day)));
        assert_eq!(o.added.len(), 24);
        assert_eq!((o.added[23].name.as_str(), o.added[23].n, o.added.iter().map(|c| c.n).sum::<usize>()), ("2026-10", 1, 2), "a3 has no arrival");
        // All: years since the first album came (a2, 2025).
        let all = overview(&albums, &songs, &HashSet::from([0]), &HashSet::from([1]), &plays, &set(&["s3"]), None, (Step::Year, None), now, 0);
        assert_eq!(all.added, vec![Count { name: "2025".into(), n: 1 }, Count { name: "2026".into(), n: 1 }]);
        // A week: its days, a1 not among them (ten days back).
        let week = overview(&albums, &songs, &HashSet::from([0]), &HashSet::from([1]), &plays, &set(&["s3"]), Some(now - 7 * day), (Step::Day, Some(7)), now, 0);
        assert_eq!((week.added.len(), week.added.iter().map(|c| c.n).sum::<usize>(), week.added_step), (7, 0, "day"));
        assert_eq!((o.tracks, o.tracks_heard, o.albums, o.albums_never, o.favourites), (4, 1, 3, 2, 1), "Incoming switched off: s5 not counted");
        // The past month: only a1 came then (a3's arrival is not known); Growth stays its two years.
        assert_eq!(o.plays.iter().map(|c| c.n).collect::<Vec<_>>(), [3, 0, 1, 0, 0, 0], "s2, s3, s4 never; s1 three times");
        assert_eq!(o.most_played.iter().map(|t| (t.id.as_str(), t.plays)).collect::<Vec<_>>(), [("s1", 3)]);
        let m = overview(&albums, &songs, &HashSet::from([0]), &HashSet::from([1]), &plays, &set(&["s3"]), Some(now - 30 * day), (Step::Week, Some(5)), now, 0);
        assert_eq!((m.tracks, m.tracks_heard, m.albums, m.albums_never, m.added.iter().map(|c| c.n).sum::<usize>()), (2, 1, 1, 0, 1));
    }

    #[test]
    fn a_compilation_s_various_artists_is_no_artist() {
        let mut lib = library();
        lib.push(row("VA - Mix (2001)/01 a.mp3", "MP3", 320, &[("albumartist", "Various Artists"), ("artist", "AIR"), ("title", "a")]));
        let s = library_stats(&lib, &Rules::default(), &HashSet::new());
        assert_eq!(s.top_artists.iter().map(|a| (a.name.as_str(), a.n)).collect::<Vec<_>>(), [("AIR", 2), ("Bonobo", 2)]);
        assert_eq!(s.artists, 2);
    }

    #[test]
    fn playlists_are_counted_against_the_library_and_related() {
        let lines = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let air1 = "AIR - Moon Safari (1998)/01 Sexy Boy.flac";
        let air2 = "AIR - Moon Safari (1998)/02 Talisman.flac";
        let kiara = "Bonobo - Black Sands (2010)/02 Kiara.mp3";
        let playlists = vec![
            ("Chill".to_string(), lines(&[air1, air2, kiara]), 0),
            ("Evening".to_string(), lines(&[air1, air2, "Gone/01.mp3"]), 1),
            ("Party".to_string(), lines(&[kiara]), 0),
        ];
        let i = build(&library(), &library(), &library(), &Rules::default(), &playlists);
        let p = &i.playlists;
        assert_eq!((p.playlists, p.distinct_tracks, p.missing), (3, 4, 1));
        assert_eq!((p.library_tracks_in_playlists, p.albums_without_playlist), (3, 1), "only the untagged album is in none");
        assert!(p.most_shared.contains(&SharedTrack { title: "Kiara".into(), artist: Some("Bonobo; Andreya Triana".into()), playlists: 2 }));
        assert_eq!(p.most_shared.len(), 3, "the two AIR tracks and Kiara are each in two playlists");
        // Music switched off, another folder's tracks only (prefixed, as the service reads them): none in a playlist,
        // the playlists themselves and how close they are as before.
        let other: Vec<TrackRow> = library().into_iter().map(|mut r| (r.rel = format!("1:{}", r.rel), r).1).collect();
        let o = build(&library(), &other, &other, &Rules::default(), &playlists);
        assert_eq!((o.playlists.library_tracks_in_playlists, o.playlists.albums_without_playlist, o.playlists.most_shared.len()), (0, o.library.albums, 0));
        assert_eq!((o.playlists.playlists, o.proximity.playlists.len()), (3, 0), "the playlists are still counted; the map has none");
        let x = &i.proximity;
        assert_eq!(x.playlists.iter().map(|p| p.tracks).collect::<Vec<_>>(), vec![3, 3, 1]);
        let chill_evening = x.links.iter().find(|l| (l.a, l.b) == (0, 1)).expect("Chill and Evening are close");
        assert_eq!(chill_evening.shared_tracks, 2);
        assert!((chill_evening.tracks - 0.5).abs() < 1e-9, "2 shared of 4: {}", chill_evening.tracks);
    }

    #[test]
    fn genre_links_playlists_without_shared_tracks_and_common_genres_weigh_nothing() {
        let t = |rel: &str, genre: &str, artist: &str| row(rel, "MP3", 320, &[("genre", genre), ("artist", artist), ("title", rel)]);
        let rows = vec![
            t("a/1", "Electronic; Dub Techno", "Deepchord"),
            t("a/2", "Electronic; Dub Techno", "Echospace"),
            t("b/1", "Electronic; Dub Techno", "Deepchord"),
            t("c/1", "Electronic; Polka", "Polka Band"),
        ];
        let pl = |name: &str, lines: &[&str]| (name.to_string(), lines.iter().map(|s| s.to_string()).collect::<Vec<_>>(), 0);
        let p = proximity(&rows, &Rules::default(), &[pl("Deep", &["a/1", "a/2"]), pl("Night", &["b/1"]), pl("Odd", &["c/1"])]);
        let link = |a, b| p.links.iter().find(|l| (l.a, l.b) == (a, b));
        let deep_night = link(0, 1).expect("same rare genre and artist");
        assert_eq!(deep_night.shared_tracks, 0);
        assert!(deep_night.genre > 0.99, "{}", deep_night.genre);
        // Deepchord in both; Deep's Echospace is in no other playlist, so it is left out of the comparison.
        assert!(deep_night.artists > 0.99, "{}", deep_night.artists);
        assert_eq!(deep_night.genres, vec!["Dub Techno".to_string()], "Electronic is everywhere: it brings nothing");
        assert!(link(0, 2).is_none() && link(1, 2).is_none(), "only Electronic in common: not close");
        assert_eq!(p.playlists[0].genres, vec!["Dub Techno".to_string()]);
        assert_eq!(p.playlists[0].focus, Some(1.0), "one distinctive genre");
        // Shares are plain: Electronic, in every playlist, still covers half of Deep's ground.
        assert_eq!(p.playlists[0].shares, BTreeMap::from([("Dub Techno".to_string(), 0.5), ("Electronic".to_string(), 0.5)]));
    }

    #[test]
    fn a_small_share_is_not_told() {
        let profile = HashMap::from([("House".to_string(), 30.0), ("Techno".to_string(), 9.0), ("Polka".to_string(), 1.0)]);
        assert_eq!(shares_of(&profile), BTreeMap::from([("House".to_string(), 0.75), ("Techno".to_string(), 0.225)]), "Polka's 0.025 left out");
        assert!(shares_of(&HashMap::new()).is_empty());
    }

    #[test]
    fn a_genre_only_one_playlist_has_does_not_push_it_away() {
        let t = |rel: &str, genre: &str| row(rel, "MP3", 320, &[("genre", genre), ("artist", rel), ("title", rel)]);
        let pl = |name: &str, lines: &[&str]| (name.to_string(), lines.iter().map(|s| s.to_string()).collect::<Vec<_>>(), 0);
        let lists = [pl("Roots", &["a/1", "a/2"]), pl("Mixed", &["b/1"]), pl("Other", &["c/1"])];
        let with = |extra: &str| {
            let rows = vec![t("a/1", "Roots Reggae"), t("a/2", extra), t("b/1", "Roots Reggae"), t("c/1", "Polka")];
            proximity(&rows, &Rules::default(), &lists).links.iter().find(|l| (l.a, l.b) == (0, 1)).map(|l| l.genre).unwrap()
        };
        assert!((with("Ska") - with("Roots Reggae")).abs() < 1e-9, "Ska, in Roots only, changes nothing");
    }

    #[test]
    fn focus_is_high_for_one_style_and_low_for_a_spread() {
        let m = |v: &[(&str, f64)]| v.iter().map(|(k, w)| (k.to_string(), *w)).collect::<HashMap<_, _>>();
        assert_eq!(focus(&m(&[("Roots Reggae", 4.0)])), Some(1.0));
        let spread = focus(&m(&[("A", 1.0), ("B", 1.0), ("C", 1.0), ("D", 1.0)])).unwrap();
        assert!((spread - 0.25).abs() < 1e-9);
        assert_eq!(focus(&HashMap::new()), None);
    }

    #[test]
    fn years_must_be_plausible() {
        assert_eq!(year_of("1998-03-16"), Some(1998));
        assert_eq!(year_of("0000"), None);
        assert_eq!(year_of("98"), None);
    }
}
