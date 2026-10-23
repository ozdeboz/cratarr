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

/// Genres and artists shown, most albums first.
// Insights shows a few on its overview and all of them on their own views (decision 0065).
const TOP_GENRES: usize = 200;
const TOP_ARTISTS: usize = 200;
/// A pair is sent when one of its signals reaches this; the page decides what to draw.
const MIN_SIGNAL: f64 = 0.05;
/// Genres named per playlist and per pair.
const NAMED_GENRES: usize = 3;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Insights {
    pub library: LibraryStats,
    pub playlists: PlaylistInsights,
    pub proximity: Proximity,
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
    /// Albums per genre, most first.
    pub genres: Vec<Count>,
    /// Albums per album artist, most first.
    pub top_artists: Vec<Count>,
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

/// Everything on the Insights page. `playlists`: (name, entry lines, missing tracks), as the playlist store has them.
pub fn build(rows: &[TrackRow], rules: &Rules, playlists: &[(String, Vec<String>, usize)]) -> Insights {
    let library = library_stats(rows, rules);

    let borrowed: Vec<(&str, &[String], usize)> = playlists.iter().map(|(n, l, d)| (n.as_str(), l.as_slice(), *d)).collect();
    let stats = playlist_stats(&borrowed);
    let in_any: HashSet<&str> = playlists.iter().flat_map(|(_, l, _)| l.iter().map(String::as_str)).collect();
    let by_rel: HashMap<&str, &TrackRow> = rows.iter().map(|r| (r.rel.as_str(), r)).collect();
    let albums_with: HashSet<&str> = rows.iter().filter(|r| in_any.contains(r.rel.as_str())).map(|r| album_dir(&r.rel)).collect();
    let most_shared = stats
        .most_shared
        .iter()
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
        library_tracks_in_playlists: rows.iter().filter(|r| in_any.contains(r.rel.as_str())).count(),
        albums_without_playlist: library.albums - albums_with.len(),
        most_shared,
    };

    Insights { library, playlists: playlists_view, proximity: proximity(rows, rules, playlists) }
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
    let genres = tf_idf(profile("genre"));
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

/// The library figures, counted per track and per album folder.
pub fn library_stats(rows: &[TrackRow], rules: &Rules) -> LibraryStats {
    #[derive(Default)]
    struct Album<'a> {
        formats: Vec<&'a str>,
        min_kbps: Option<u32>,
        year: Option<u32>,
        genres: HashSet<String>,
        album_artists: HashSet<String>,
        /// Artists every track has, for albums without an album artist tag.
        common_artists: Option<HashSet<String>>,
    }
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
        a.genres.extend(values("genre"));
        a.album_artists.extend(r.artists(rules, "albumartist"));
        let artists: HashSet<String> = r.artists(rules, "artist").into_iter().collect();
        a.common_artists = Some(match a.common_artists.take() {
            None => artists,
            Some(c) => c.intersection(&artists).cloned().collect(),
        });
    }
    out.albums = albums.len();
    let mut decades: BTreeMap<u32, usize> = BTreeMap::new();
    let mut genres: HashMap<String, usize> = HashMap::new();
    let mut artists: HashMap<String, usize> = HashMap::new();
    for a in albums.values() {
        match quality_of(a.formats.iter().copied(), a.min_kbps) {
            Quality::Lossless => out.lossless_albums += 1,
            Quality::Lossy { kbps } if kbps < 320 => out.below_320_albums += 1,
            _ => {}
        }
        if let Some(y) = a.year {
            *decades.entry(y / 10 * 10).or_default() += 1;
        }
        for g in &a.genres {
            *genres.entry(g.clone()).or_default() += 1;
        }
        let names = if a.album_artists.is_empty() { a.common_artists.clone().unwrap_or_default() } else { a.album_artists.clone() };
        // A compilation's "Various Artists" is no artist: as one, it dwarfed every real one's bar (the owner, 2026-10-23).
        for n in names.into_iter().filter(|n| !crate::crates::various(n)) {
            *artists.entry(n).or_default() += 1;
        }
    }
    out.artists = artists.len();
    out.formats = ranked(formats, usize::MAX);
    out.decades = decades.into_iter().map(|(d, n)| Count { name: format!("{d}s"), n }).collect();
    out.genres = ranked(genres, TOP_GENRES);
    out.top_artists = ranked(artists, TOP_ARTISTS);
    out
}

/// Most first, then by name; at most `top`.
fn ranked(counts: HashMap<String, usize>, top: usize) -> Vec<Count> {
    let mut v: Vec<Count> = counts.into_iter().map(|(name, n)| Count { name, n }).collect();
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
        let s = library_stats(&library(), &Rules::default());
        assert_eq!((s.tracks, s.albums, s.duration_ms, s.bytes), (5, 3, 1_000_000, 5_000));
        assert_eq!(s.formats, vec![Count { name: "MP3".into(), n: 3 }, Count { name: "FLAC".into(), n: 2 }]);
        assert_eq!((s.lossless_albums, s.below_320_albums), (1, 1), "AIR lossless; Bonobo has a 192 kbps track");
        assert_eq!(s.decades, vec![Count { name: "1990s".into(), n: 1 }, Count { name: "2010s".into(), n: 1 }], "original date wins");
        assert_eq!(s.genres, vec![Count { name: "Downtempo".into(), n: 2 }, Count { name: "Electronic".into(), n: 1 }]);
        // Bonobo has no album artist tag: the artist every track shares counts, not the guest.
        assert_eq!(s.top_artists, vec![Count { name: "AIR".into(), n: 1 }, Count { name: "Bonobo".into(), n: 1 }]);
        assert_eq!(s.artists, 2);
    }

    #[test]
    fn a_compilation_s_various_artists_is_no_artist() {
        let mut lib = library();
        lib.push(row("VA - Mix (2001)/01 a.mp3", "MP3", 320, &[("albumartist", "Various Artists"), ("artist", "AIR"), ("title", "a")]));
        let s = library_stats(&lib, &Rules::default());
        assert_eq!(s.top_artists, vec![Count { name: "AIR".into(), n: 1 }, Count { name: "Bonobo".into(), n: 1 }]);
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
        let i = build(&library(), &Rules::default(), &playlists);
        let p = &i.playlists;
        assert_eq!((p.playlists, p.distinct_tracks, p.missing), (3, 4, 1));
        assert_eq!((p.library_tracks_in_playlists, p.albums_without_playlist), (3, 1), "only the untagged album is in none");
        assert!(p.most_shared.contains(&SharedTrack { title: "Kiara".into(), artist: Some("Bonobo; Andreya Triana".into()), playlists: 2 }));
        assert_eq!(p.most_shared.len(), 3, "the two AIR tracks and Kiara are each in two playlists");
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
