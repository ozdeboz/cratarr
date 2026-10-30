//! Related artists to explore, on an artist's page (0.9.2; the owner, 2026-10-25: "recommended artists, for example.
//! related artists to explore. That's the best."; "prioritize Bandcamp playable artists"): one ranked row from the
//! artists ListenBrainz finds listened to alongside (its similar artists, by MusicBrainz id) and MusicBrainz's members
//! and collaborators. One the library has opens its own page; a new one its album on Bandcamp, where one plays. Those
//! that play (the library's, and the new ones on Bandcamp) come first. Looked up in the background the first time the
//! page asks, kept 30 days for every account (userdata `related_artists`): MusicBrainz and Bandcamp at one request a
//! second.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::import::loose;
use super::musicbrainz::Asked as _;
use super::reach::Sent as _;

/// ListenBrainz's similar artists, as its own artist pages ask for them (its labs API, with the algorithm they use).
const LB_LABS: &str = "https://labs.api.listenbrainz.org/similar-artists/json";
const LB_ALGORITHM: &str = "session_based_days_7500_session_300_contribution_5_threshold_10_limit_100_filter_True_skip_30";
/// Of ListenBrainz's similar artists, how many are taken; of all, how many the row shows.
const SIMILAR: usize = 30;
const MOST: usize = 20;
/// Of the new artists, the most related this many are looked for on Bandcamp (MusicBrainz, then Bandcamp).
const BANDCAMP_LOOKS: usize = 12;
/// Looked up again after this long.
const FRESH_SECS: i64 = 30 * 86_400;
/// What MusicBrainz's relations rank at, against ListenBrainz's most alike (1).
const RELATED_ON_MB: f64 = 0.8;

/// An artist related to another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Related {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbid: Option<String>,
    /// Why, in a few words each: "Listened alongside", "Band member", "Worked together".
    pub why: Vec<String>,
    /// Their id in the library, whose page they open; none: new to the library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// An album of theirs that plays on Bandcamp (a new artist's way in), and its title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandcamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    /// How related, as ranked (kept, so Discover can weigh them: the most related 1, roughly).
    #[serde(default)]
    pub score: f64,
}

/// What a look found for an artist (by MusicBrainz id), and when.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Found {
    pub at: i64,
    pub artists: Vec<Related>,
}

/// The artists being looked up now (by MusicBrainz id): one look each at a time.
static LOOKING: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn looking() -> std::sync::MutexGuard<'static, Option<HashSet<String>>> {
    LOOKING.lock().unwrap_or_else(|p| p.into_inner())
}

/// Whether the artist is being looked up now.
pub fn is_looking(mbid: &str) -> bool {
    looking().as_ref().is_some_and(|l| l.contains(mbid))
}

/// What is kept for the artist, and whether it is due to be looked up (never, or longer ago than `FRESH_SECS`).
pub fn kept(serve: &crate::serve::Serve, mbid: &str) -> Result<(Option<Found>, bool)> {
    let found = serve.userdata.related_artists(mbid)?.and_then(|(_, json)| serde_json::from_str::<Found>(&json).ok());
    let due = found.as_ref().is_none_or(|f| super::moment::now_secs() - f.at > FRESH_SECS);
    Ok((found, due))
}

/// The artist's related artists as kept, looked up now (and kept) when none are or they are old: for Discover, which
/// waits for them in its own background look (0.9.2: the masonry's artists get their tiles from here). While another
/// look of the same artist runs, what is kept (possibly nothing).
pub async fn kept_or_look(serve: &Arc<crate::serve::Serve>, name: &str, mbid: &str) -> Result<Found> {
    let (s, key) = (serve.clone(), mbid.to_string());
    let (found, due) = tokio::task::spawn_blocking(move || kept(&s, &key)).await??;
    if !due || !looking().get_or_insert_with(HashSet::new).insert(mbid.to_string()) {
        return Ok(found.unwrap_or_default());
    }
    let looked = look(serve, name, mbid).await;
    if let Some(l) = looking().as_mut() {
        l.remove(mbid);
    }
    let found = looked?;
    let (ud, key, json) = (serve.userdata.clone(), mbid.to_string(), serde_json::to_string(&found)?);
    let at = found.at;
    tokio::task::spawn_blocking(move || ud.set_related_artists(&key, at, &json)).await??;
    Ok(found)
}

/// Looks the artist up in the background, unless that runs already; kept when done.
pub fn start(serve: Arc<crate::serve::Serve>, name: String, mbid: String) {
    if !looking().get_or_insert_with(HashSet::new).insert(mbid.clone()) {
        return;
    }
    tokio::spawn(async move {
        match look(&serve, &name, &mbid).await {
            Ok(found) => {
                let (ud, key, json) = (serve.userdata.clone(), mbid.clone(), serde_json::to_string(&found).unwrap_or_default());
                if let Err(e) = tokio::task::spawn_blocking(move || ud.set_related_artists(&key, found.at, &json)).await.map_err(anyhow::Error::from).and_then(|r| r) {
                    tracing::warn!("related artists of {name}: could not be kept: {e:#}");
                }
            }
            Err(e) => tracing::info!("related artists of {name}: {e:#}"),
        }
        if let Some(l) = looking().as_mut() {
            l.remove(&mbid);
        }
    });
}

/// ListenBrainz's similar artists: (MusicBrainz id, name, its score).
async fn lb_similar(http: &reqwest::Client, mbid: &str) -> Result<Vec<(String, String, f64)>> {
    #[derive(Deserialize)]
    struct Similar {
        artist_mbid: String,
        name: String,
        #[serde(default)]
        score: f64,
    }
    let list: Vec<Similar> = http
        .get(LB_LABS)
        .query(&[("artist_mbids", mbid), ("algorithm", LB_ALGORITHM)])
        .sent()
        .await
        .context("ListenBrainz did not answer")?
        .error_for_status()?
        .json()
        .await
        .context("ListenBrainz's similar artists could not be read")?;
    Ok(list.into_iter().filter(|s| s.artist_mbid != mbid).take(SIMILAR).map(|s| (s.artist_mbid, s.name, s.score)).collect())
}

/// A related artist as MusicBrainz names them: (their MusicBrainz id, name, why).
type MbRelated = Vec<(String, String, &'static str)>;

/// MusicBrainz's artists related to an artist, each with why: the bands they play in and their members, those they
/// worked with, those who played with them, the person a performance name is and the names a person performs as (0.10:
/// kept, `works::persons_of`, so an alias is the real person); and those "is person" relations as found.
async fn mb_related(http: &reqwest::Client, mbid: &str) -> Result<(MbRelated, Vec<(String, String, String)>)> {
    #[derive(Deserialize)]
    struct Artist {
        id: String,
        name: String,
    }
    #[derive(Deserialize)]
    struct Relation {
        #[serde(rename = "type", default)]
        kind: String,
        #[serde(default)]
        direction: String,
        artist: Option<Artist>,
    }
    #[derive(Deserialize)]
    struct Lookup {
        #[serde(default)]
        relations: Vec<Relation>,
    }
    let url = format!("https://musicbrainz.org/ws/2/artist/{mbid}?inc=artist-rels&fmt=json");
    let raw: serde_json::Value = http.get(url).asked().await?.error_for_status()?.json().await?;
    let persons = super::works::persons_of(mbid, &raw);
    let found: Lookup = serde_json::from_value(raw)?;
    let related = found
        .relations
        .into_iter()
        .filter_map(|r| {
            let why = match (r.kind.as_str(), r.direction.as_str()) {
                ("member of band", "backward") => "Band member",
                ("member of band", _) => "Their band",
                ("collaboration", _) => "Worked together",
                ("supporting musician" | "instrumental supporting musician" | "vocal supporting musician", _) => "Played together",
                ("subgroup", _) => "Their group",
                ("is person", "backward") => "Same person",
                ("is person", _) => "Performs as",
                _ => return None,
            };
            let a = r.artist?;
            (a.id != mbid).then_some((a.id, a.name, why))
        })
        .collect();
    Ok((related, persons))
}

/// An album of the artist that plays on Bandcamp, and its title: their own Bandcamp page as MusicBrainz keeps it, its
/// newest release.
async fn on_bandcamp(http: &reqwest::Client, mbid: &str) -> Option<(String, String)> {
    let urls = super::discover::mb_artist_urls(http, mbid).await.ok()?;
    tokio::time::sleep(super::discover::MB_PAUSE).await;
    let own = urls.iter().filter_map(|u| crate::bandcamp::page(u)).find(|p| reqwest::Url::parse(p).ok().and_then(|u| u.host_str().map(|h| h != "bandcamp.com")).unwrap_or(false))?;
    let releases = crate::bandcamp::Bandcamp::get().releases(&own).await.ok()?;
    releases.into_iter().find_map(|(title, url)| Some((title, crate::bandcamp::album_page(&url)?)))
}

/// Looks up the artists related to one (`name`, `mbid`): ListenBrainz's and MusicBrainz's, the library's marked, the new
/// ones' Bandcamp albums found, ranked: those that play first, then the most related.
async fn look(serve: &Arc<crate::serve::Serve>, name: &str, mbid: &str) -> Result<Found> {
    let http = super::discover::http()?;
    let mut by: HashMap<String, Related> = HashMap::new();
    let mut add = |id: String, name: String, why: &str, score: f64| {
        let r = by.entry(id.clone()).or_insert_with(|| Related { name, mbid: Some(id), why: Vec::new(), id: None, bandcamp: None, album: None, score: 0.0 });
        if !r.why.iter().any(|w| w == why) {
            r.why.push(why.to_string());
        }
        // A second source counts extra.
        r.score = if r.score > 0.0 { r.score + score + 0.5 } else { score };
    };
    match lb_similar(&http, mbid).await {
        Ok(similar) => {
            let most = similar.iter().map(|s| s.2).fold(0.0, f64::max).max(f64::EPSILON);
            for (id, n, s) in similar {
                add(id, n, "Listened alongside", s / most);
            }
        }
        Err(e) => tracing::info!("related artists of {name}: ListenBrainz: {e:#}"),
    }
    match mb_related(&http, mbid).await {
        Ok((related, persons)) => {
            for (id, n, why) in related {
                add(id, n, why, RELATED_ON_MB);
            }
            if !persons.is_empty() {
                let ud = serve.userdata.clone();
                if let Err(e) = tokio::task::spawn_blocking(move || ud.add_persons(&persons)).await.map_err(anyhow::Error::from).and_then(|r| r) {
                    tracing::info!("related artists of {name}: who is who not kept: {e:#}");
                }
            }
        }
        Err(e) => tracing::info!("related artists of {name}: MusicBrainz: {e:#}"),
    }
    tokio::time::sleep(super::discover::MB_PAUSE).await;
    let mut artists: Vec<Related> = by.into_values().filter(|r| loose(&r.name) != loose(name)).collect();
    // The library's: by name, as the artist page names them.
    let (lib, names) = (serve.library.clone(), artists.iter().map(|r| r.name.clone()).collect::<Vec<_>>());
    let ids: Vec<Option<String>> =
        tokio::task::spawn_blocking(move || names.iter().map(|n| lib.artist_by_name(n).ok().flatten().map(|(id, _)| id)).collect()).await?;
    for (r, id) in artists.iter_mut().zip(ids) {
        r.id = id;
    }
    // The new ones on Bandcamp, the most related first.
    artists.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    let mut looked = 0;
    for r in artists.iter_mut().filter(|r| r.id.is_none()) {
        if looked == BANDCAMP_LOOKS {
            break;
        }
        looked += 1;
        if let Some(id) = r.mbid.clone()
            && let Some((title, page)) = on_bandcamp(&http, &id).await
        {
            (r.album, r.bandcamp) = (Some(title), Some(page));
        }
    }
    // Those that play first (the owner: "prioritize Bandcamp playable artists"), then the most related.
    artists.sort_by(|a, b| {
        let plays = |r: &Related| r.id.is_some() || r.bandcamp.is_some();
        plays(b).cmp(&plays(a)).then_with(|| b.score.total_cmp(&a.score)).then_with(|| a.name.cmp(&b.name))
    });
    artists.truncate(MOST);
    Ok(Found { at: super::moment::now_secs(), artists })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_look_reads_back_and_says_when_it_is_due() {
        let found = Found {
            at: super::super::moment::now_secs(),
            artists: vec![Related {
                name: "Extrawelt".into(),
                mbid: Some("m1".into()),
                why: vec!["Listened alongside".into()],
                id: None,
                bandcamp: Some("https://extrawelt.bandcamp.com/album/x".into()),
                album: Some("Fernweh".into()),
                score: 0.75,
            }],
        };
        let json = serde_json::to_string(&found).unwrap();
        let back: Found = serde_json::from_str(&json).unwrap();
        assert_eq!(back.artists[0].bandcamp.as_deref(), Some("https://extrawelt.bandcamp.com/album/x"));
        assert_eq!((back.artists[0].album.as_deref(), back.artists[0].score), (Some("Fernweh"), 0.75), "kept with its rank, for Discover");
        assert!(super::super::moment::now_secs() - back.at <= FRESH_SECS, "fresh");
    }
}
