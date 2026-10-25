//! One search, answered by kind (decision 0072): `GET /api/search?q=&kinds=&limit=` → `{ q, groups, top }`. Every
//! result in one shape (`Item`), what the panel opens as its `subject`, why it is there in `matched`. Cratarr's own
//! library: albums (by their tracks too), artists (by their albums too), tracks, genres; the account's playlists and
//! crates by their names and by the tracks in them (a track's title, artist or album). `top` is every group merged by score, a weight per kind
//! (Meilisearch's federated search).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::search::{closeness, words};

/// What a search may find, and how much each kind weighs in `top`.
const KINDS: [(&str, f32); 6] = [("artist", 1.0), ("album", 0.95), ("playlist", 0.9), ("crate", 0.85), ("track", 0.8), ("genre", 0.7)];
/// Each group's most, unless asked for fewer (`limit`), and `top`'s.
pub const LIMIT: usize = 20;
const TOP: usize = 8;
/// Most tracks a search looks for in playlists and crates.
const DEEP: usize = 2000;
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

/// A result's score: how its own name matches (a whole name, 3; whole words, 2; a start, 1; not itself, 0), then its
/// place in the order the library's search gave (`at` of `of`), so the best first within each closeness.
fn score(title: &str, words: &[String], at: usize, of: usize) -> f32 {
    f32::from(closeness(title, words)) + 1.0 - at as f32 / of.max(1) as f32
}

fn matched_name(title: &str) -> Vec<Matched> {
    vec![Matched { on: "name", text: title.to_string(), count: None }]
}

fn matched(h: &crate::serve::read::Hit) -> Vec<Matched> {
    vec![Matched { on: h.on, text: h.text.clone(), count: h.count }]
}

/// The lines of the tracks the words name (title, artist, album), with why; none without Cratarr's own library.
async fn found_lines(w: &[String]) -> anyhow::Result<HashMap<String, Entry>> {
    let Some(served) = crate::serve::instance() else { return Ok(HashMap::new()) };
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

/// The asking account's crates with their tracks, kept a minute (`CRATES_KEEP`).
async fn crate_members(app: &super::App) -> anyhow::Result<Members> {
    static KEPT: OnceLock<Mutex<HashMap<Option<String>, (Instant, Members)>>> = OnceLock::new();
    let account = super::playlists::asking_account();
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
pub async fn search(app: &super::App, q: &str, kinds: &[String], limit: usize) -> anyhow::Result<Answer> {
    let w = words(q);
    let mut answer = Answer { q: q.to_string(), ..Answer::default() };
    if w.is_empty() {
        return Ok(answer);
    }
    let wants = |k: &str| kinds.is_empty() || kinds.iter().any(|x| x == k);
    let limit = limit.clamp(1, 100);
    let mut groups: BTreeMap<&'static str, Vec<Item>> = BTreeMap::new();
    if ["album", "artist", "track", "genre"].iter().any(|k| wants(k))
        && let Some(served) = crate::serve::instance()
    {
        let (lib, words_) = (served.library.clone(), w.clone());
        let found = tokio::task::spawn_blocking(move || lib.find(&words_, limit)).await??;
        let n = found.albums.len();
        groups.insert(
            "album",
            found
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
                    score: score(&a.name, &w, i, n),
                    matched: matched(h),
                })
                .collect(),
        );
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
                    score: score(&t.title, &w, i, n),
                    matched: if closeness(&t.title, &w) > 0 { matched_name(&t.title) } else { vec![Matched { on: "artist", text: t.artist.clone(), count: None }] },
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
                    score: score(g, &w, i, n),
                    matched: matched_name(g),
                })
                .collect(),
        );
    }
    // Playlists and crates by their names, and by the tracks in them (a track's title, artist or album).
    let deep = if wants("playlist") || wants("crate") { found_lines(&w).await? } else { HashMap::new() };
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
            found.push(Item { kind: "crate", subject: format!("crate:{id}"), sub: String::new(), cover: None, track: None, score, matched, title: name });
        }
        found.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
        groups.insert("crate", found);
    }
    // Each group as asked for; `top` the best of all, weighed by kind.
    let mut top: Vec<Item> = Vec::new();
    for (kind, weight) in KINDS {
        let Some(mut items) = groups.remove(kind) else { continue };
        let total = items.len();
        items.truncate(limit);
        top.extend(items.iter().take(TOP).cloned().map(|mut i| {
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
    fn a_whole_name_scores_above_a_start_and_the_library_s_order_within() {
        let w = words("air");
        assert!(score("Air", &w, 3, 10) > score("Air Supply", &w, 0, 10));
        assert!(score("Air Supply", &w, 0, 10) > score("Airbag", &w, 0, 10));
        assert!(score("Airbag", &w, 0, 10) > score("Airbag", &w, 5, 10));
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
