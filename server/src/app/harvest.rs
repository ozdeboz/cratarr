//! The harvest of the library's words (0.9.7, the owner: one structured harvest; `app::words` merges what it keeps):
//! MusicBrainz's genres and tags of each album's release group and of each artist, Discogs' genres and styles of each
//! album's master, Last.fm's top tags of each album and artist. In the background and gently (a request a second or
//! so, as MusicBrainz asks), a batch a round: the subjects never asked first, then those asked longest ago, each asked
//! again after its source's `again`. A source that does not answer keeps what it said before.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;

use super::musicbrainz::Asked as _;
use crate::userdata::words::RawWord;

/// The first round waits for the server to settle; the rounds follow each other at this distance.
const START_AFTER: Duration = Duration::from_secs(15 * 60);
const ROUND: Duration = Duration::from_secs(30 * 60);
/// Subjects asked of each source a round.
const BATCH: usize = 200;
/// How long what a source said stands before it is asked again.
const MB_AGAIN: i64 = 90 * 86_400;
const DISCOGS_AGAIN: i64 = 180 * 86_400;
const LASTFM_AGAIN: i64 = 90 * 86_400;
/// MusicBrainz's tags kept from this many votes (its genres, a curated list, always).
const MB_TAG_VOTES: i64 = 2;
/// Last.fm's tags kept from this count (its top tag 100), at most this many.
const LASTFM_LEAST: f64 = 10.0;
const LASTFM_MOST: usize = 10;
/// Last.fm's tags that say nothing of the music.
const JUNK: [&str; 24] = [
    "seen live",
    "favorites",
    "favourites",
    "favorite",
    "favourite",
    "favorite albums",
    "albums i own",
    "owned",
    "my albums",
    "love",
    "loved",
    "awesome",
    "beautiful",
    "best",
    "good",
    "great",
    "cool",
    "amazing",
    "check out",
    "to listen",
    "vinyl",
    "cd",
    "spotify",
    "under 2000 listeners",
];

/// The harvest's rounds, in the background.
pub fn start() {
    tokio::spawn(async {
        tokio::time::sleep(START_AFTER).await;
        loop {
            if let Err(e) = round().await {
                tracing::info!("harvest: a round stopped: {e:#}");
            }
            tokio::time::sleep(ROUND).await;
        }
    });
}

/// One round: each source asked about its batch.
async fn round() -> Result<()> {
    let Some(served) = crate::serve::instance() else { return Ok(()) };
    let lib = served.library.clone();
    let (albums, artists) = tokio::task::spawn_blocking(move || -> Result<_> { Ok((lib.album_subjects()?, lib.artist_subjects()?)) }).await??;
    let http = super::discover::http()?;
    // MusicBrainz: albums with a release, artists with an id.
    for (subject, a) in due(&served, "musicbrainz", albums.iter().filter(|a| super::discogs::is_mbid(&a.1)).map(|a| (format!("album:{}", a.0), a)), MB_AGAIN).await? {
        let said = match super::discogs::release_group_of(&http, &a.1).await {
            Ok(Some(rg)) => mb_words(&http, &format!("release-group/{rg}")).await,
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(e),
        };
        keep(&served, &subject, "musicbrainz", said).await?;
    }
    for (subject, a) in due(&served, "musicbrainz", artists.iter().filter(|a| super::discogs::is_mbid(&a.1)).map(|a| (format!("artist:{}", a.0), a)), MB_AGAIN).await? {
        let said = mb_words(&http, &format!("artist/{}", a.1)).await;
        keep(&served, &subject, "musicbrainz", said).await?;
    }
    // Albums missing tracks (`incomplete`): at MusicBrainz's pace too.
    if let Err(e) = super::incomplete::pass(&served, &http).await {
        tracing::info!("harvest: albums missing tracks not looked at: {e:#}");
    }
    // Discogs: every album's master (not only those without a genre, as the genres' pass asks).
    for (subject, a) in due(&served, "discogs", albums.iter().map(|a| (format!("album:{}", a.0), a)), DISCOGS_AGAIN).await? {
        let rg = if super::discogs::is_mbid(&a.1) { super::discogs::release_group_of(&http, &a.1).await.unwrap_or_default() } else { None };
        let said = match super::discogs::master_of(rg.as_deref(), &a.2, &a.3).await {
            Ok(Some(id)) => super::discogs::master(id).await.map(|m| {
                m.map(|m| {
                    let genres = m.genres.into_iter().map(|g| RawWord { word: g, kind: "genre".into(), weight: 1.0 });
                    genres.chain(m.styles.into_iter().map(|s| RawWord { word: s, kind: "style".into(), weight: 1.0 })).collect::<Vec<RawWord>>()
                })
                .unwrap_or_default()
            }),
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(e),
        };
        keep(&served, &subject, "discogs", said).await?;
    }
    // Last.fm, with the server's own key: albums by their names, artists by theirs.
    let ud = served.userdata.clone();
    let Some(key) = tokio::task::spawn_blocking(move || super::discover::server_lastfm_key(&ud)).await?? else { return Ok(()) };
    let auth = super::lastfm::Auth { key, signed: None };
    for (subject, a) in due(&served, "lastfm", albums.iter().map(|a| (format!("album:{}", a.0), a)), LASTFM_AGAIN).await? {
        let said = super::discover::fm(&http, &auth, "album.gettoptags", &[("artist", &a.2), ("album", &a.3), ("autocorrect", "1")]).await.map(|v| lastfm_words(&v, &a.2));
        keep(&served, &subject, "lastfm", said).await?;
    }
    for (subject, a) in due(&served, "lastfm", artists.iter().map(|a| (format!("artist:{}", a.0), a)), LASTFM_AGAIN).await? {
        let said = super::discover::fm(&http, &auth, "artist.gettoptags", &[("artist", &a.2), ("autocorrect", "1")]).await.map(|v| lastfm_words(&v, &a.2));
        keep(&served, &subject, "lastfm", said).await?;
    }
    Ok(())
}

/// The batch of `subjects` due at a source: those never asked, in the library's order, then those asked longest ago.
async fn due<'a, T: 'a>(served: &std::sync::Arc<crate::serve::Serve>, source: &'static str, subjects: impl Iterator<Item = (String, &'a T)>, again: i64) -> Result<Vec<(String, &'a T)>> {
    let ud = served.userdata.clone();
    let asked: HashMap<String, i64> = tokio::task::spawn_blocking(move || ud.words_asked_at(source)).await??;
    let now = super::moment::now_secs();
    let mut due: Vec<(i64, String, &T)> = subjects.filter_map(|(s, t)| {
        let at = asked.get(&s).copied();
        at.is_none_or(|at| now - at >= again).then(|| (at.unwrap_or(i64::MIN), s, t))
    }).collect();
    due.sort_by_key(|d| d.0);
    Ok(due.into_iter().take(BATCH).map(|(_, s, t)| (s, t)).collect())
}

/// What a source said of a subject kept (an error: noted as asked, what it said before kept).
async fn keep(served: &std::sync::Arc<crate::serve::Serve>, subject: &str, source: &'static str, said: Result<Vec<RawWord>>) -> Result<()> {
    let ok = said.is_ok();
    if let Err(e) = &said {
        tracing::debug!("harvest: {source} on {subject}: {e:#}");
    }
    let (ud, subject, words) = (served.userdata.clone(), subject.to_string(), said.unwrap_or_default());
    tokio::task::spawn_blocking(move || ud.set_words(&subject, source, &words, ok, super::moment::now_secs())).await??;
    Ok(())
}

/// MusicBrainz's genres (all, a curated list) and tags (from `MB_TAG_VOTES` votes, those not also a genre) of a release
/// group or an artist (`what`: `release-group/<id>`, `artist/<id>`), each weighed by its votes against the most.
async fn mb_words(http: &reqwest::Client, what: &str) -> Result<Vec<RawWord>> {
    let v: Value = http.get(format!("https://musicbrainz.org/ws/2/{what}?inc=genres+tags&fmt=json")).asked().await?.error_for_status()?.json().await?;
    tokio::time::sleep(super::discover::MB_PAUSE).await;
    Ok(mb_parse(&v))
}

fn mb_parse(v: &Value) -> Vec<RawWord> {
    let list = |k: &str| -> Vec<(String, i64)> {
        v[k].as_array().into_iter().flatten().filter_map(|g| Some((g["name"].as_str()?.to_string(), g["count"].as_i64().unwrap_or(1)))).collect()
    };
    let (genres, tags) = (list("genres"), list("tags"));
    let most = genres.iter().chain(&tags).map(|g| g.1).max().unwrap_or(1).max(1) as f64;
    let mut out: Vec<RawWord> = genres.iter().map(|(w, n)| RawWord { word: w.clone(), kind: "genre".into(), weight: (*n as f64 / most).max(0.2) }).collect();
    for (w, n) in tags.into_iter().filter(|(_, n)| *n >= MB_TAG_VOTES) {
        if !genres.iter().any(|(g, _)| g.eq_ignore_ascii_case(&w)) {
            out.push(RawWord { word: w, kind: "tag".into(), weight: n as f64 / most });
        }
    }
    out
}

/// Last.fm's top tags worth keeping: from `LASTFM_LEAST`, none that says nothing of the music (`JUNK`), names its
/// artist, or is a year or a decade; `LASTFM_MOST` at most, weighed by their count.
fn lastfm_words(v: &Value, artist: &str) -> Vec<RawWord> {
    let artist = super::words::key(artist);
    v["toptags"]["tag"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| {
            let name = t["name"].as_str()?.trim();
            let count = t["count"].as_f64().or_else(|| t["count"].as_str()?.parse().ok()).unwrap_or(0.0);
            let k = super::words::key(name);
            let dated = k.trim_end_matches('s').chars().all(|c| c.is_ascii_digit());
            (count >= LASTFM_LEAST && !k.is_empty() && !dated && k != artist && !JUNK.contains(&k.as_str())).then(|| RawWord { word: name.to_string(), kind: "tag".into(), weight: (count / 100.0).min(1.0) })
        })
        .take(LASTFM_MOST)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn musicbrainz_genres_are_all_kept_its_tags_from_two_votes() {
        let v = serde_json::json!({
            "genres": [{"name": "trip hop", "count": 4}, {"name": "downtempo", "count": 1}],
            "tags": [{"name": "trip hop", "count": 4}, {"name": "bristol", "count": 2}, {"name": "my faves", "count": 1}]
        });
        let got: Vec<(String, String)> = mb_parse(&v).into_iter().map(|w| (w.word, w.kind)).collect();
        assert_eq!(got, [("trip hop".to_string(), "genre".to_string()), ("downtempo".into(), "genre".into()), ("bristol".into(), "tag".into())]);
    }

    #[test]
    fn lastfm_keeps_what_describes_the_music() {
        let v = serde_json::json!({"toptags": {"tag": [
            {"name": "trip-hop", "count": 100}, {"name": "seen live", "count": 60}, {"name": "Portishead", "count": 40},
            {"name": "90s", "count": 30}, {"name": "1994", "count": 20}, {"name": "melancholic", "count": "25"}, {"name": "bristol", "count": 5}
        ]}});
        let got: Vec<String> = lastfm_words(&v, "Portishead").into_iter().map(|w| w.word).collect();
        assert_eq!(got, ["trip-hop", "melancholic"]);
    }
}
