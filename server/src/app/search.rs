//! One search, answered by kind (decision 0072): `GET /api/search?q=&kinds=&limit=` → `{ q, groups, top }`. Every
//! result in one shape (`Item`), what the panel opens as its `subject`, why it is there in `matched`. Cratarr's own
//! library: albums (by their tracks too), artists (by their albums too), tracks, genres; the account's playlists and
//! crates by their names. `top` is every group merged by score, a weight per kind (Meilisearch's federated search).

use std::collections::BTreeMap;

use serde::Serialize;

use crate::search::{closeness, words};

/// What a search may find, and how much each kind weighs in `top`.
const KINDS: [(&str, f32); 6] = [("artist", 1.0), ("album", 0.95), ("playlist", 0.9), ("crate", 0.85), ("track", 0.8), ("genre", 0.7)];
/// Each group's most, unless asked for fewer (`limit`), and `top`'s.
pub const LIMIT: usize = 20;
const TOP: usize = 8;

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
    if wants("playlist") {
        let mut lists: Vec<Item> = app
            .playlists
            .view(None)
            .await?
            .into_iter()
            .filter(|p| crate::search::matches(&p.name, &w))
            .map(|p| Item {
                kind: "playlist",
                subject: format!("playlist:{}", p.id),
                sub: match p.count {
                    1 => "1 track".to_string(),
                    c => format!("{c} tracks"),
                },
                cover: None,
                track: None,
                score: score(&p.name, &w, 0, 1),
                matched: matched_name(&p.name),
                title: p.name,
            })
            .collect();
        lists.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
        groups.insert("playlist", lists);
    }
    if wants("crate") {
        let mut found: Vec<Item> = app
            .playlists
            .crate_names()
            .await?
            .into_iter()
            .filter(|(_, name)| crate::search::matches(name, &w))
            .map(|(id, name)| Item {
                kind: "crate",
                subject: format!("crate:{id}"),
                sub: String::new(),
                cover: None,
                track: None,
                score: score(&name, &w, 0, 1),
                matched: matched_name(&name),
                title: name,
            })
            .collect();
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
}
