//! One search, answered by kind (decision 0072): `GET /api/search?q=&kinds=&limit=` → `{ q, groups, top }`. Every
//! result in one shape (`Item`), what the panel opens as its `subject`, why it is there in `matched`. Cratarr's own
//! library: albums (by their tracks too), artists (by their albums too), tracks, genres; the account's playlists and
//! crates by their names and by the tracks in them (a track's title, artist or album); and what is not in the library
//! (point 7): the albums missing from its artists, and what Discover leaves out for the account. `top` is every group
//! merged by score, a weight per kind (Meilisearch's federated search).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::search::{closeness, words};

/// What a search may find, and how much each kind weighs in `top`.
const KINDS: [(&str, f32); 14] = [
    ("artist", 1.0),
    ("album", 0.95),
    ("playlist", 0.9),
    ("crate", 0.85),
    ("track", 0.8),
    ("genre", 0.7),
    // The library's words of the other kinds (0.9.7, `app::words`).
    ("style", 0.68),
    ("mood", 0.66),
    ("tag", 0.64),
    // What Cratarr knows of outside the library (0076, the indexed universe), surest first.
    ("missing", 0.6),
    ("requested", 0.58),
    ("spotify", 0.56),
    ("suggested", 0.54),
    ("excluded", 0.5),
];
/// The known-elsewhere kinds in the order an album shown in several is kept in the first (its reasons gathered there).
const KNOWN: [&str; 5] = ["missing", "requested", "spotify", "suggested", "excluded"];
/// Most a page of one kind (Show all) reaches into it.
const DEEPEST: usize = 2000;
/// Each group's most, unless asked for fewer (`limit`), and `top`'s.
pub const LIMIT: usize = 20;
const TOP: usize = 8;
/// Most tracks a search looks for in playlists and crates.
const DEEP: usize = 2000;
/// How long the missing albums are kept for search: reading them weighs the whole library against every discography.
const MISSING_KEEP: Duration = Duration::from_secs(60);
/// How long an account's crates, with their tracks, are kept for search: a query is asked again as it is typed, and
/// making every crate is the Crates page's work.
const CRATES_KEEP: Duration = Duration::from_secs(60);

type Members = Arc<Vec<(String, String, Vec<String>)>>;

/// Why a track found makes a list found: the field that holds the words (`track`, its title, best; then `artist`;
/// then `album`), how well, and its text.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    rank: (u8, u8),
    on: &'static str,
    text: String,
}

/// A found track's `Entry`: the first of its title, artist and album holding every word; its title where the words
/// are spread over them.
fn entry(title: String, artist: String, album: String, w: &[String]) -> Entry {
    for (rank, on, text) in [(3, "track", &title), (2, "artist", &artist), (1, "album", &album)] {
        if crate::search::matches(text, w) {
            return Entry { rank: (rank, closeness(text, w)), on, text: text.clone() };
        }
    }
    Entry { rank: (0, 0), on: "track", text: title }
}

/// Why a result is there: what it matched on (`name`, `artist`, `album`, `track`), the text, and how many.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Matched {
    pub on: &'static str,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

/// One result, the same shape for every kind.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub kind: &'static str,
    /// What the panel opens: `album:cratarr/<id>`, `artist:cratarr/<name>`, `playlist:<id>`, `crate:<id>`,
    /// `crate:genre:<name>`; a track its album's.
    pub subject: String,
    pub title: String,
    pub sub: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    /// A track's own id, to play it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    /// An album's artist where the panel needs it apart from `sub`: one not in the library, to look for on Bandcamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    /// A missing album's page on Bandcamp where an open of it found one lately (0.9.5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bandcamp: Option<String>,
    /// A missing album's open request (0.9.5): wanted, or coming and how far.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<super::requests::Mark>,
    /// Why Cratarr knows an album not in the library (0076, the indexed universe): "Missing from <artist>", "On your
    /// Spotify", "Suggested", "Requested"; one album once, its reasons together.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub known: Vec<String>,
    /// A classical record's, or its track's, composers (0.10); a record found by its composer says so in `matched`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub composers: Vec<crate::source::ComposerRef>,
    pub score: f32,
    pub matched: Vec<Matched>,
}

#[derive(Debug, Default, Serialize)]
pub struct Group {
    pub total: usize,
    pub items: Vec<Item>,
}

#[derive(Debug, Default, Serialize)]
pub struct Answer {
    pub q: String,
    pub groups: BTreeMap<&'static str, Group>,
    pub top: Vec<Item>,
}

/// A result's score (0.9.7, `search::rank`): a direct hit first, then how close its own name is (a whole name, whole
/// words, a start), then how much of its name the words make up, then its place in the order its source gave
/// (`at` of `of`). One found through what it holds (no word in its name) scores below 1, as `by_tracks` does.
fn score(title: &str, words: &[String], at: usize, of: usize) -> f32 {
    crate::search::rank(title, words, at, of)
}

fn matched_name(title: &str) -> Vec<Matched> {
    vec![Matched { on: "name", text: title.to_string(), count: None }]
}

fn matched(h: &crate::serve::read::Hit) -> Vec<Matched> {
    vec![Matched { on: h.on, text: h.text.clone(), count: h.count }]
}

/// The lines of the tracks the words name (title, artist, album), with why; none without Cratarr's own library.
async fn found_lines(served: Option<Arc<crate::serve::Serve>>, w: &[String]) -> anyhow::Result<HashMap<String, Entry>> {
    let Some(served) = served else { return Ok(HashMap::new()) };
    let words = w.to_vec();
    tokio::task::spawn_blocking(move || -> anyhow::Result<HashMap<String, Entry>> {
        let cat = served.catalogue();
        Ok(served
            .library
            .songs_naming(&words, DEEP)?
            .into_iter()
            .filter_map(|(id, title, artist, album)| Some((crate::serve::playlists::line_of_song(cat, &id)?, entry(title, artist, album, &words))))
            .collect())
    })
    .await?
}

/// How many of a list's lines are tracks found, and the best of them (`Entry::rank`, the first among equals); none when
/// it has none.
fn holding<'a>(lines: impl IntoIterator<Item = &'a String>, deep: &HashMap<String, Entry>) -> Option<(u32, Entry)> {
    let mut best: Option<&Entry> = None;
    let mut n = 0;
    for e in lines.into_iter().filter_map(|l| deep.get(l)) {
        n += 1;
        if best.is_none_or(|b| e.rank > b.rank) {
            best = Some(e);
        }
    }
    best.map(|e| (n, e.clone()))
}

/// A list found by its tracks only: below every name that matches (those score 2 and up), more tracks higher.
fn by_tracks(count: u32, e: Entry) -> (f32, Vec<Matched>) {
    (1.0 - 1.0 / (1.0 + count as f32), vec![Matched { on: e.on, text: e.text, count: Some(count) }])
}

/// Why an album not in the library is there: its name holding the words, else its artist; its name where the words are
/// spread over both. Its closeness, the better of the two.
fn album_match(title: &str, artist: &str, w: &[String]) -> Option<(u8, Vec<Matched>)> {
    if !crate::search::matches(&format!("{title} {artist}"), w) {
        return None;
    }
    let close = closeness(title, w).max(closeness(artist, w));
    let on = if !crate::search::matches(title, w) && crate::search::matches(artist, w) { ("artist", artist) } else { ("name", title) };
    Some((close, vec![Matched { on: on.0, text: on.1.to_string(), count: None }]))
}

/// An album outside the library ranked as `search::rank` ranks a name: its name's or its artist's closeness, the typos
/// the one matched needs, then how much of it the words make up.
fn matched_rank(close: u8, matched: &[Matched], w: &[String], at: usize, of: usize) -> f32 {
    let text = matched.first().map_or("", |m| m.text.as_str());
    crate::search::ranked(close, crate::search::typos(text, w), crate::search::coverage(text, w), at, of)
}

/// The missing albums the words name (`subject` `missing:<release group>`), the best first, the newest among equals.
fn missing_items(list: &[crate::app::discover::MissingOf], w: &[String], marks: Option<&HashMap<String, super::requests::Mark>>) -> Vec<Item> {
    let n = list.len();
    let mut out: Vec<Item> = list
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let (close, matched) = album_match(&m.title, &m.artist, w)?;
            Some(Item {
                kind: "missing",
                subject: format!("missing:{}", m.release_group),
                known: vec![format!("Missing from {}", m.artist)],
                composers: Vec::new(),
                title: m.title.clone(),
                sub: [Some(m.artist.clone()), m.year.map(|y| y.to_string())].into_iter().flatten().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),
                cover: Some(m.cover.clone()),
                track: None,
                artist: Some(m.artist.clone()),
                bandcamp: m.bandcamp.clone(),
                request: marks.and_then(|k| super::requests::mark_of(k, Some(&m.release_group), &m.artist, &m.title)),
                score: matched_rank(close, &matched, w, i, n),
                matched,
            })
        })
        .collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

/// What Discover leaves out for the account that the words name: albums (`excluded:<key>`, Not interested or Hidden)
/// and artists (`excluded-artist:<their key>`, what Take back takes); and the artists it excluded everywhere, a kind of
/// their own (`excluded-everywhere:<their key>`, decision 0083: what DELETE /api/me/excluded-artists/{key} takes back).
/// The best first, the newest among equals.
fn excluded_items(
    albums: &[crate::app::discover::Excluded],
    artists: &[crate::app::discover::LeftOutArtist],
    everywhere: &[crate::userdata::ExcludedArtist],
    w: &[String],
) -> Vec<Item> {
    let n = albums.len() + artists.len() + everywhere.len();
    let mut out: Vec<Item> = albums
        .iter()
        .enumerate()
        .filter_map(|(i, x)| {
            let (close, matched) = album_match(&x.album, &x.artist, w)?;
            Some(Item {
                kind: "excluded",
                subject: format!("excluded:{}", x.key),
                title: x.album.clone(),
                sub: if x.how == "hidden" { "Hidden" } else { "Not interested" }.to_string(),
                cover: x.cover.clone(),
                track: None,
                artist: Some(x.artist.clone()),
                bandcamp: None,
                known: Vec::new(),
                composers: Vec::new(),
                request: None,
                score: matched_rank(close, &matched, w, i, n),
                matched,
            })
        })
        .chain(artists.iter().enumerate().filter(|(_, a)| crate::search::matches(&a.artist, w)).map(|(i, a)| Item {
            kind: "excluded",
            subject: format!("excluded-artist:{}", a.key),
            title: a.artist.clone(),
            sub: "Artist left out".to_string(),
            cover: None,
            track: None,
            artist: Some(a.artist.clone()),
            bandcamp: None,
            known: Vec::new(),
            composers: Vec::new(),
            request: None,
            score: score(&a.artist, w, i, n),
            matched: matched_name(&a.artist),
        }))
        .chain(everywhere.iter().enumerate().filter(|(_, a)| crate::search::matches(&a.name, w)).map(|(i, a)| Item {
            kind: "excluded",
            subject: format!("excluded-everywhere:{}", a.key),
            title: a.name.clone(),
            sub: "Excluded everywhere".to_string(),
            cover: None,
            track: None,
            artist: Some(a.name.clone()),
            bandcamp: None,
            known: Vec::new(),
            composers: Vec::new(),
            request: None,
            score: score(&a.name, w, i, n),
            matched: matched_name(&a.name),
        }))
        .collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

/// An album outside the library from one of the known-elsewhere lists, as search shows it.
fn known_item(kind: &'static str, id: String, title: &str, artist: &str, year: Option<i32>, cover: Option<String>, why: &str, w: &[String], at: usize, of: usize) -> Option<Item> {
    let (close, matched) = album_match(title, artist, w)?;
    Some(Item {
        kind,
        subject: format!("{kind}:{id}"),
        title: title.to_string(),
        sub: [Some(artist.to_string()), year.map(|y| y.to_string())].into_iter().flatten().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),
        cover,
        track: None,
        artist: Some(artist.to_string()),
        bandcamp: None,
        request: None,
        known: vec![why.to_string()],
        composers: Vec::new(),
        score: matched_rank(close, &matched, w, at, of),
        matched,
    })
}

/// What one album is, across the known-elsewhere lists: its release group where one is known, else its names.
fn album_key(i: &Item) -> String {
    let id = i.subject.split_once(':').map_or("", |(_, id)| id);
    let uuid = id.len() == 36 && id.chars().filter(|c| *c == '-').count() == 4;
    if uuid { id.to_string() } else { format!("{}|{}", crate::app::import::key(i.artist.as_deref().unwrap_or_default()), crate::app::import::key(&i.title)) }
}

/// The classical records the words find by their composers (0.10, the owner: "Brahms" finds the records where Brahms
/// is not the album artist, Arrau's, Schiff's), those found already (`have`, by id) left out: under the composer's
/// name, the composers' best first, each composer's records in the library's order.
fn by_composer(c: &super::classical::Index, w: &[String], have: &std::collections::HashSet<String>) -> Vec<Item> {
    let named = c.composers_named(w);
    let mut seen = have.clone();
    let mut out: Vec<Item> = Vec::new();
    for (i, composer) in named.iter().enumerate() {
        let n = named.len();
        for id in &composer.albums {
            let Some(r) = c.records.get(id).filter(|r| r.classical) else { continue };
            if !seen.insert(id.clone()) {
                continue;
            }
            out.push(Item {
                kind: "album",
                subject: format!("album:cratarr/{id}"),
                title: r.title.clone(),
                sub: [Some(r.artist.clone()), r.year.map(|y| y.to_string())].into_iter().flatten().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),
                cover: Some(format!("/api/albums/cratarr/{id}/cover")),
                track: None,
                artist: None,
                bandcamp: None,
                known: Vec::new(),
                composers: r.composers.clone(),
                request: None,
                // Below the records named by the words themselves, the composer's closeness among them.
                score: score(&composer.name, w, i, n) * 0.9,
                matched: vec![Matched { on: "composer", text: composer.name.clone(), count: None }],
            });
        }
    }
    out
}

/// Every missing album of the library's artists, kept a minute (`MISSING_KEEP`): the same for every account.
async fn all_missing(app: &super::App, served: &Arc<crate::serve::Serve>) -> anyhow::Result<Arc<Vec<crate::app::discover::MissingOf>>> {
    type Kept = Option<(Instant, Arc<Vec<crate::app::discover::MissingOf>>)>;
    static KEPT: OnceLock<Mutex<Kept>> = OnceLock::new();
    let kept = KEPT.get_or_init(Default::default);
    if let Some((at, m)) = kept.lock().unwrap_or_else(|p| p.into_inner()).clone()
        && at.elapsed() < MISSING_KEEP
    {
        return Ok(m);
    }
    let m = Arc::new(crate::app::discover::all_missing(served, app.library.catalogue()).await?.0);
    *kept.lock().unwrap_or_else(|p| p.into_inner()) = Some((Instant::now(), m.clone()));
    Ok(m)
}

/// The asking account's crates with their tracks, kept a minute (`CRATES_KEEP`).
async fn crate_members(app: &super::App) -> anyhow::Result<Members> {
    static KEPT: OnceLock<Mutex<HashMap<Option<String>, (Instant, Members)>>> = OnceLock::new();
    let account = app.playlists.asking_account();
    let kept = KEPT.get_or_init(Default::default);
    if let Some((at, m)) = kept.lock().unwrap_or_else(|p| p.into_inner()).get(&account).cloned()
        && at.elapsed() < CRATES_KEEP
    {
        return Ok(m);
    }
    let m: Members = Arc::new(app.playlists.crate_members().await?);
    kept.lock().unwrap_or_else(|p| p.into_inner()).insert(account, (Instant::now(), m.clone()));
    Ok(m)
}

/// Answers a query for the asking account (its playlists and crates): every kind asked for (`kinds`, all when empty).
pub async fn search(app: &super::App, q: &str, kinds: &[String], limit: usize, offset: usize) -> anyhow::Result<Answer> {
    let w = words(q);
    let mut answer = Answer { q: q.to_string(), ..Answer::default() };
    if w.is_empty() {
        return Ok(answer);
    }
    let wants = |k: &str| kinds.is_empty() || kinds.iter().any(|x| x == k);
    let limit = limit.clamp(1, 100);
    // Show all (0076): a page of one kind from `offset`; the library asked for as far as that page reaches.
    let offset = offset.min(DEEPEST);
    let reach = (offset + limit).min(DEEPEST);
    let mut groups: BTreeMap<&'static str, Vec<Item>> = BTreeMap::new();
    // How many each kind matches in all, where the library shows fewer (`limit`); the rest count what they hold.
    let mut totals: HashMap<&'static str, usize> = HashMap::new();
    if ["album", "artist", "track", "genre"].iter().any(|k| wants(k))
        && let Some(served) = app.served.clone()
    {
        let (lib, words_) = (served.library.clone(), w.clone());
        let found = tokio::task::spawn_blocking(move || lib.find(&words_, reach)).await??;
        // Classical (0.10): the records' composers, and the records found by them ("Brahms" finds Arrau's).
        let s = served.clone();
        let classical = tokio::task::spawn_blocking(move || super::classical::index(&s)).await?.inspect_err(|e| tracing::debug!("search: Classical not read: {e:#}")).ok();
        let t = found.totals;
        totals.extend([("album", t.albums), ("artist", t.artists), ("track", t.tracks), ("genre", t.genres)]);
        let n = found.albums.len();
        let composers_of = |album: &str| classical.as_ref().and_then(|c| c.records.get(album)).filter(|r| r.classical).map(|r| r.composers.clone()).unwrap_or_default();
        let mut albums: Vec<Item> = found
            .albums
            .iter()
            .enumerate()
            .map(|(i, (a, h))| Item {
                kind: "album",
                subject: format!("album:cratarr/{}", a.id),
                title: a.name.clone(),
                sub: [Some(a.artist.clone()), a.year.map(|y| y.to_string())].into_iter().flatten().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),
                cover: Some(format!("/api/albums/cratarr/{}/cover", a.id)),
                track: None,
                artist: None,
                bandcamp: None,
                known: Vec::new(),
                composers: composers_of(a.id.as_str()),
                request: None,
                score: score(&a.name, &w, i, n),
                matched: matched(h),
            })
            .collect();
        if let Some(c) = &classical {
            let by = by_composer(c, &w, &albums.iter().filter_map(|a| a.subject.strip_prefix("album:cratarr/").map(str::to_string)).collect());
            if let Some(t) = totals.get_mut("album") {
                *t += by.len();
            }
            albums.extend(by);
        }
        groups.insert("album", albums);
        let n = found.artists.len();
        groups.insert(
            "artist",
            found
                .artists
                .iter()
                .enumerate()
                .map(|(i, (a, h))| Item {
                    kind: "artist",
                    subject: format!("artist:cratarr/{}", a.name),
                    title: a.name.clone(),
                    sub: match a.album_count {
                        1 => "1 album".to_string(),
                        c => format!("{c} albums"),
                    },
                    cover: Some(format!("/api/artists/{}/photo", a.id)),
                    track: None,
                    artist: None,
                    bandcamp: None,
                    known: Vec::new(),
                    composers: Vec::new(),
                    request: None,
                    score: score(&a.name, &w, i, n),
                    matched: matched(h),
                })
                .collect(),
        );
        let n = found.tracks.len();
        groups.insert(
            "track",
            found
                .tracks
                .iter()
                .enumerate()
                .map(|(i, t)| Item {
                    kind: "track",
                    subject: format!("album:cratarr/{}", t.album_id),
                    title: t.title.clone(),
                    sub: [t.artist.clone(), t.album.clone()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),
                    cover: Some(format!("/api/albums/cratarr/{}/cover", t.album_id)),
                    track: Some(t.id.clone()),
                    artist: None,
                    bandcamp: None,
                    known: Vec::new(),
                    composers: classical.as_ref().map(|c| c.song_composers(&t.id)).unwrap_or_default(),
                    request: None,
                    score: score(&t.title, &w, i, n),
                    matched: if crate::search::matches(&t.title, &w) { matched_name(&t.title) } else { vec![Matched { on: "artist", text: t.artist.clone(), count: None }] },
                })
                .collect(),
        );
        let n = found.genres.len();
        groups.insert(
            "genre",
            found
                .genres
                .iter()
                .enumerate()
                .map(|(i, (g, count))| Item {
                    kind: "genre",
                    subject: format!("crate:{}{g}", crate::crates::GENRE),
                    title: g.clone(),
                    sub: match count {
                        1 => "1 track".to_string(),
                        c => format!("{c} tracks"),
                    },
                    cover: None,
                    track: None,
                    artist: None,
                    bandcamp: None,
                    known: Vec::new(),
                    composers: Vec::new(),
                    request: None,
                    score: score(g, &w, i, n),
                    matched: matched_name(g),
                })
                .collect(),
        );
    }
    // The library's words (0.9.7, `app::words`): a group per kind, each word opening its crate (`crate:<kind>:<word>`),
    // by how many albums carry it among equals. In place of the library's genres where they can be read.
    if super::words::KINDS.iter().any(|k| wants(k))
        && let Some(served) = app.served.clone()
    {
        let (s, cat) = (served.clone(), app.library.catalogue());
        match tokio::task::spawn_blocking(move || super::words::search_index(&s, &cat)).await? {
            Ok(index) => {
                for kind in super::words::KINDS.into_iter().filter(|k| wants(k)) {
                    let hits: Vec<&(&'static str, String, usize)> = index.iter().filter(|(k, word, _)| *k == kind && crate::search::matches(word, &w)).collect();
                    let n = hits.len();
                    totals.insert(kind, n);
                    groups.insert(
                        kind,
                        hits.iter()
                            .enumerate()
                            .map(|(i, (_, word, albums))| Item {
                                kind,
                                subject: format!("crate:{kind}:{word}"),
                                title: word.clone(),
                                sub: match albums {
                                    1 => "1 album".to_string(),
                                    c => format!("{c} albums"),
                                },
                                cover: None,
                                track: None,
                                artist: None,
                                bandcamp: None,
                                known: Vec::new(),
                                composers: Vec::new(),
                                request: None,
                                score: score(word, &w, i, n),
                                matched: vec![Matched { on: kind, text: word.clone(), count: None }],
                            })
                            .collect(),
                    );
                }
            }
            Err(e) => tracing::debug!("search: the library's words not read: {e:#}"),
        }
    }
    // Playlists and crates by their names, and by the tracks in them (a track's title, artist or album).
    let deep = if wants("playlist") || wants("crate") { found_lines(app.served.clone(), &w).await? } else { HashMap::new() };
    if wants("playlist") {
        let lines: HashMap<String, Vec<String>> =
            if deep.is_empty() { HashMap::new() } else { app.playlists.visible_lines().await?.into_iter().map(|(id, _, l)| (id, l)).collect() };
        let mut lists: Vec<Item> = Vec::new();
        for p in app.playlists.view(None).await? {
            let (score, matched) = if crate::search::matches(&p.name, &w) {
                (score(&p.name, &w, 0, 1), matched_name(&p.name))
            } else if let Some((count, e)) = lines.get(&p.id).and_then(|l| holding(l, &deep)) {
                by_tracks(count, e)
            } else {
                continue;
            };
            lists.push(Item {
                kind: "playlist",
                subject: format!("playlist:{}", p.id),
                sub: match p.count {
                    1 => "1 track".to_string(),
                    c => format!("{c} tracks"),
                },
                cover: None,
                track: None,
                artist: None,
                bandcamp: None,
                known: Vec::new(),
                composers: Vec::new(),
                request: None,
                score,
                matched,
                title: p.name,
            });
        }
        lists.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
        groups.insert("playlist", lists);
    }
    if wants("crate") {
        // By name from the names alone; by tracks only where a track was found (making every crate is not cheap).
        let members = if deep.is_empty() { None } else { Some(crate_members(app).await?) };
        let names = match &members {
            Some(m) => m.iter().map(|(id, name, _)| (id.clone(), name.clone())).collect(),
            None => app.playlists.crate_names().await?,
        };
        let mut found: Vec<Item> = Vec::new();
        for (id, name) in names {
            let (score, matched) = if crate::search::matches(&name, &w) {
                (score(&name, &w, 0, 1), matched_name(&name))
            } else if let Some((count, e)) = members.as_ref().and_then(|m| m.iter().find(|x| x.0 == id)).and_then(|x| holding(&x.2, &deep)) {
                by_tracks(count, e)
            } else {
                continue;
            };
            found.push(Item { kind: "crate", subject: format!("crate:{id}"), sub: String::new(), cover: None, track: None, artist: None, bandcamp: None, request: None, known: Vec::new(), composers: Vec::new(), score, matched, title: name });
        }
        found.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
        groups.insert("crate", found);
    }
    // What is not in the library (point 7): only from Cratarr's own library; the exclusions an account's own.
    if let Some(served) = app.served.clone() {
        if wants("missing") {
            let marks = super::requests::marks(app, &served).await.inspect_err(|e| tracing::debug!("search: the requests' marks not read: {e:#}")).ok();
            groups.insert("missing", missing_items(&all_missing(app, &served).await?, &w, marks.as_deref()));
        }
        if wants("excluded")
            && let Some(account) = served.current_user()
        {
            let s = served.clone();
            let (albums, artists, everywhere) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
                let (albums, artists) = crate::app::discover::exclusions(&s, &account)?;
                Ok((albums, artists, s.userdata.excluded_artist_list(&account)?))
            })
            .await??;
            groups.insert("excluded", excluded_items(&albums, &artists, &everywhere, &w));
        }
        // The rest of the indexed universe (0076): the account's requests, its Spotify lists, Discover's suggestions.
        if let Some(account) = served.current_user()
            && (wants("requested") || wants("spotify") || wants("suggested"))
        {
            let marks = super::requests::marks(app, &served).await.ok();
            if wants("requested") {
                let (s, a) = (served.clone(), account.clone());
                let open: Vec<crate::userdata::Request> = tokio::task::spawn_blocking(move || s.userdata.requests(Some(&a), None, 1000))
                    .await??
                    .into_iter()
                    .filter(|r| r.status == super::requests::PENDING || r.status == super::requests::APPROVED)
                    .collect();
                // One in the library is the library's now: requested only on the Requests page.
                let open = super::requests::not_in_library(&served, open).await;
                let n = open.len();
                let items = open
                    .iter()
                    .enumerate()
                    .filter_map(|(i, r)| known_item("requested", r.release_group.clone().unwrap_or_else(|| r.id.to_string()), &r.album, &r.artist, r.year, r.cover.clone(), "Requested", &w, i, n))
                    .collect();
                groups.insert("requested", items);
            }
            if wants("spotify") {
                let a = account.clone();
                let albums = tokio::task::spawn_blocking(move || super::spotify::albums(&a, None, None)).await?.unwrap_or_default();
                let albums: Vec<_> = albums.into_iter().filter(|x| x.status != super::spotify::marks::IN).collect();
                let n = albums.len();
                let items = albums
                    .iter()
                    .enumerate()
                    .filter_map(|(i, x)| {
                        let mut item = known_item("spotify", x.release_group.clone().unwrap_or_else(|| x.id.clone()), &x.title, &x.artist, x.year, x.cover.clone(), "On your Spotify", &w, i, n)?;
                        item.request = marks.as_deref().and_then(|k| super::requests::mark_of(k, x.release_group.as_deref(), &x.artist, &x.title));
                        Some(item)
                    })
                    .collect();
                groups.insert("spotify", items);
            }
            if wants("suggested") {
                let (s, a) = (served.clone(), account.clone());
                let found = tokio::task::spawn_blocking(move || crate::app::discover::kept(&s, &a)).await?.ok().flatten();
                let albums = found.map(|f| f.albums).unwrap_or_default();
                let n = albums.len();
                let items = albums
                    .iter()
                    .enumerate()
                    .filter_map(|(i, x)| {
                        let mut item = known_item("suggested", x.release_group.clone().unwrap_or_else(|| x.key.clone()), &x.album, &x.artist, x.year, x.cover.clone(), "Suggested", &w, i, n)?;
                        item.bandcamp = x.bandcamp.clone();
                        item.request = marks.as_deref().and_then(|k| super::requests::mark_of(k, x.release_group.as_deref(), &x.artist, &x.album));
                        Some(item)
                    })
                    .collect();
                groups.insert("suggested", items);
            }
        }
    }
    // One album once outside the library, in the first known-elsewhere kind that has it, its reasons gathered there.
    let mut first: HashMap<String, (&'static str, usize)> = HashMap::new();
    for kind in KNOWN {
        let Some(items) = groups.remove(kind) else { continue };
        let mut kept: Vec<Item> = Vec::new();
        for item in items {
            let key = album_key(&item);
            if let Some((k, at)) = first.get(&key).copied() {
                let into = if k == kind { kept.get_mut(at) } else { groups.get_mut(k).and_then(|g| g.get_mut(at)) };
                if let Some(into) = into {
                    for why in item.known {
                        if !into.known.contains(&why) {
                            into.known.push(why);
                        }
                    }
                }
                continue;
            }
            first.insert(key, (kind, kept.len()));
            kept.push(item);
        }
        groups.insert(kind, kept);
    }
    // Each group as asked for; `top` the best of all, weighed by kind.
    let mut top: Vec<Item> = Vec::new();
    for (kind, weight) in KINDS {
        let Some(mut items) = groups.remove(kind) else { continue };
        let total = totals.get(kind).copied().unwrap_or(0).max(items.len());
        items.sort_by(|a, b| b.score.total_cmp(&a.score));
        // A page from `offset` (Show all); `top` only on the first.
        items.drain(..offset.min(items.len()));
        items.truncate(limit);
        top.extend(items.iter().take(if offset == 0 { TOP } else { 0 }).cloned().map(|mut i| {
            i.score *= weight;
            i
        }));
        answer.groups.insert(kind, Group { total, items });
    }
    top.sort_by(|a, b| b.score.total_cmp(&a.score));
    top.truncate(TOP);
    answer.top = top;
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_classical_record_is_found_by_its_composer_whoever_plays() {
        use crate::app::classical::{Inputs, build};
        use crate::app::works::{Performer, Person, Role, Stored, TrackWorks};
        use crate::serve::read::{WorkAlbum, WorkArtist};
        let mut i = Inputs::default();
        let album = |id: &str, artist: &str, release: &str| WorkAlbum {
            id: id.into(),
            base: id.into(),
            name: format!("{artist} plays"),
            artist: artist.into(),
            year: Some(1990),
            release: Some(release.into()),
            genres: vec!["Classical".into()],
            artists: vec![WorkArtist { id: format!("ar-{id}"), name: artist.into(), mbid: None }],
        };
        i.albums = vec![album("al-arrau", "Claudio Arrau", "r-1"), album("al-schiff", "András Schiff", "r-2"), album("al-brahms", "Brahms", "r-3")];
        for (r, player) in [("r-1", "Claudio Arrau"), ("r-2", "András Schiff"), ("r-3", "Someone")] {
            let stored = Stored {
                performed: true,
                performers: vec![Performer { name: player.into(), mbid: None, role: Role::Soloist }],
                tracks: vec![TrackWorks { recording: Some(format!("rec-{r}")), disc: 1, position: 1, works: vec!["w-1".into()] }],
                ..Stored::default()
            };
            i.releases.insert(r.into(), stored);
        }
        i.works.insert("w-1".into(), ("Piano Sonata no. 3".into(), vec![Person { mbid: "a-brahms".into(), name: "Johannes Brahms".into(), sort_name: Some("Brahms, Johannes".into()) }]));
        let ix = build(&i);
        // The library's own search found the record named Brahms already; the composer finds the other two.
        let have: std::collections::HashSet<String> = ["al-brahms".to_string()].into();
        let found = by_composer(&ix, &words("brahms"), &have);
        let subjects: Vec<&str> = found.iter().map(|x| x.subject.as_str()).collect();
        assert_eq!(subjects, ["album:cratarr/al-arrau", "album:cratarr/al-schiff"]);
        assert_eq!(found[0].matched, vec![Matched { on: "composer", text: "Johannes Brahms".into(), count: None }]);
        assert_eq!(found[0].composers[0].key, "a-brahms");
        assert!(by_composer(&ix, &words("arrau"), &std::collections::HashSet::new()).is_empty(), "a performer is no composer");
    }

    #[test]
    fn a_whole_name_scores_above_a_start_and_the_library_s_order_within() {
        let w = words("air");
        assert!(score("Air", &w, 3, 10) > score("Air Supply", &w, 0, 10));
        assert!(score("Air Supply", &w, 0, 10) > score("Airbag", &w, 0, 10));
        assert!(score("Airbag", &w, 0, 10) > score("Airbag", &w, 5, 10));
    }

    #[test]
    fn albums_not_in_the_library_are_found_by_their_name_or_artist_with_the_artist_apart() {
        use crate::app::discover::{Excluded, LeftOutArtist, MissingOf};
        let m = |t: &str, a: &str, y| MissingOf { release_group: format!("rg-{t}"), title: t.into(), artist: a.into(), year: y, kind: None, cover: "c".into(), bandcamp: None, request: None };
        let list = [m("Somersault", "Jacco Gardner", Some(2023)), m("Hypnophobia", "Jacco Gardner", Some(2015)), m("Moon Safari", "Air", Some(1998))];
        let found = missing_items(&list, &words("gardner"), None);
        assert_eq!(found.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["Somersault", "Hypnophobia"]);
        assert_eq!((found[0].subject.as_str(), found[0].sub.as_str(), found[0].artist.as_deref()), ("missing:rg-Somersault", "Jacco Gardner · 2023", Some("Jacco Gardner")));
        assert_eq!(found[0].matched, vec![Matched { on: "artist", text: "Jacco Gardner".into(), count: None }]);
        let found = missing_items(&list, &words("hypno gardner"), None);
        assert_eq!((found.len(), found[0].matched[0].on), (1, "name"));
        let x = |k: &str, al: &str, ar: &str, how: &str| Excluded { key: k.into(), album: al.into(), artist: ar.into(), year: None, cover: None, how: how.into(), at: 0 };
        let found = excluded_items(
            &[x("k1", "Talkie Walkie", "Air", "no"), x("k2", "Pocket Symphony", "Air", "hidden")],
            &[LeftOutArtist { artist: "Airbag".into(), key: "airbag".into(), at: 0 }],
            &[],
            &words("air"),
        );
        let got: Vec<_> = found.iter().map(|i| (i.subject.as_str(), i.sub.as_str())).collect();
        assert_eq!(got, [("excluded:k1", "Not interested"), ("excluded:k2", "Hidden"), ("excluded-artist:airbag", "Artist left out")]);
        // The artists excluded everywhere (decision 0083): a kind of their own beside Discover's.
        let everywhere = [crate::userdata::ExcludedArtist { key: "airsupply".into(), name: "Air Supply".into(), at: 0 }];
        let found = excluded_items(&[], &[LeftOutArtist { artist: "Airbag".into(), key: "airbag".into(), at: 0 }], &everywhere, &words("air supply"));
        let got: Vec<_> = found.iter().map(|i| (i.subject.as_str(), i.sub.as_str())).collect();
        assert_eq!(got, [("excluded-everywhere:airsupply", "Excluded everywhere")]);
    }

    #[test]
    fn a_track_is_found_by_its_title_first_then_its_artist_then_its_album() {
        let w = words("gardner");
        let e = entry("Clear the Air".into(), "Jacco Gardner".into(), "Cabinet of Curiosities".into(), &w);
        assert_eq!((e.on, e.text.as_str()), ("artist", "Jacco Gardner"));
        let e = entry("Gardner's Song".into(), "Jacco Gardner".into(), "X".into(), &w);
        assert_eq!((e.on, e.text.as_str()), ("track", "Gardner's Song"));
        let e = entry("Help".into(), "Someone".into(), "The Gardner Tapes".into(), &w);
        assert_eq!(e.on, "album");
        // The words spread over its fields: its title.
        let e = entry("Clear the Air".into(), "Jacco Gardner".into(), "X".into(), &words("gardner clear"));
        assert_eq!((e.on, e.text.as_str(), e.rank.0), ("track", "Clear the Air", 0));
    }

    #[test]
    fn a_list_found_by_its_tracks_counts_them_names_the_best_and_stays_below_a_name() {
        let w = words("gardner");
        let deep: HashMap<String, Entry> = [
            ("a/1.flac", entry("Clear the Air".into(), "Jacco Gardner".into(), "Cabinet".into(), &w)),
            ("a/2.flac", entry("Gardner's Song".into(), "Jacco Gardner".into(), "Cabinet".into(), &w)),
        ]
        .into_iter()
        .map(|(l, e)| (l.to_string(), e))
        .collect();
        let lines: Vec<String> = ["b/9.flac", "a/1.flac", "a/2.flac"].into_iter().map(str::to_string).collect();
        let (count, best) = holding(&lines, &deep).unwrap();
        assert_eq!((count, best.on, best.text.as_str()), (2, "track", "Gardner's Song"));
        assert_eq!(holding(&lines[..1], &deep), None);
        let (s, m) = by_tracks(count, best);
        assert_eq!(m, vec![Matched { on: "track", text: "Gardner's Song".into(), count: Some(2) }]);
        assert!(by_tracks(500, deep["a/1.flac"].clone()).0 < score("Gardner", &w, 9, 10).min(score("Gardners", &w, 9, 10)));
        assert!(s > by_tracks(1, deep["a/1.flac"].clone()).0);
    }
}
