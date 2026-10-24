//! Discover (decision 0069; the owner, 2026-10-24: "I wouldn't be interested in suggestions within my library, but
//! suggestions outside of"; "we need a discover page next to playlists"): albums the library does not have, each with
//! why it is suggested, from the signed-in account's own listening services (decision 0068).
//!
//! - **ListenBrainz** (its user: the one the account scrobbles as): its recommendations for the user (recordings,
//!   scored; marked experimental, 204 while none are made) and the newest Weekly Exploration made for them, each
//!   recording named through ListenBrainz's metadata (its album's release group, album artist, year, cover).
//! - **Last.fm** (the user it scrobbles as, else the one it imported from; asked as the import asks, `lastfm::Auth`):
//!   the user's most played artists, the artists like them (`artist.getSimilar`), and of each such artist the library
//!   has not got their best known album (`artist.getTopAlbums`).
//!
//! An album the library has (Music or Incoming) is left out: by its MusicBrainz release group where both know it, else
//! by its name and artist through the import's matcher. So is one the account answered (Not interested, or wanted), and
//! every album of an artist it said no to twice. The same album from more than one source is one suggestion with all
//! their reasons, ranked higher. Looked up once a day for an account that opens Discover, as a task (`app::tasks`), and
//! kept in between.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::import::{Matcher, key, loose};
use super::lastfm::{Auth, Progress};

/// The task's kind (`app::tasks`).
pub const KIND: &str = "discover";
/// Looked up again after this long, when the page is opened.
pub const FRESH_SECS: i64 = 86_400;
/// How many suggestions are kept: everything the sources bring, as far as is sensible (the owner, 2026-10-24: "are
/// you 100% sure that we have all the recommendations that come to us?"; it was 60).
const MOST: usize = 500;
/// How many of ListenBrainz's Weekly Explorations are read, the newest first (the owner: two, it was one).
const EXPLORATIONS: usize = 2;
const LB_API: &str = "https://api.listenbrainz.org/1";
const FM_API: &str = "https://ws.audioscrobbler.com/2.0/";
/// Last.fm asks for no more than about five calls a second.
const FM_PAUSE: Duration = Duration::from_millis(250);
/// Of the user's most played artists, how many are looked from; of the artists like them, how many get an album.
const TOP_ARTISTS: &str = "25";
const SIMILAR_PER: &str = "10";
const NEW_ARTISTS: usize = 30;
/// ListenBrainz's recordings (and release groups) named at a time.
const META_BATCH: usize = 25;
/// A card's genres, at most (0069 amended: "two or three").
const GENRES: usize = 3;

/// An album suggested.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    /// Its key: its release group, else its artist's and name's (what an answer is kept by).
    pub key: String,
    pub album: String,
    pub artist: String,
    /// The artist's key (an artist said no to twice is left out).
    pub artist_key: String,
    pub year: Option<i32>,
    pub release_group: Option<String>,
    /// A picture of it: the Cover Art Archive's, else Last.fm's.
    pub cover: Option<String>,
    /// Where to listen elsewhere.
    pub links: Vec<Link>,
    /// An artist the library has nothing of.
    pub new_artist: bool,
    /// Why, one reason a source.
    pub why: Vec<String>,
    /// Which services suggested it, by name (the owner, 2026-10-24: "where the suggestion comes from").
    #[serde(default)]
    pub sources: Vec<String>,
    /// Its album page on Bandcamp, where one was found: it plays there as a preview (decision 0071).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandcamp: Option<String>,
    /// What MusicBrainz calls it (Album, EP, Single…), as ListenBrainz gives it (0069 amended: the card shows as much
    /// as is known).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Its genres, `GENRES` at most: the album's own, else its artist's (`genres_of`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<String>,
    /// Its length and how many tracks, of one of its releases (an official one first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_count: Option<u32>,
    /// How many listen to it on Last.fm, where the account asks Last.fm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listeners: Option<u64>,
    #[serde(skip)]
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub name: String,
    pub url: String,
}

/// What a look found: the suggestions, and what the page should say of the sources (none yet from ListenBrainz, say).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Found {
    pub at: i64,
    pub albums: Vec<Suggestion>,
    pub notes: Vec<String>,
}

/// Whose services: the ListenBrainz user, the Last.fm user and how to ask Last.fm.
#[derive(Debug, Clone, Default)]
pub struct Who {
    pub listenbrainz: Option<String>,
    pub lastfm: Option<(String, Auth)>,
}

impl Who {
    pub fn any(&self) -> bool {
        self.listenbrainz.is_some() || self.lastfm.is_some()
    }
}

/// An album a source named, before the library and the answers are looked at.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Candidate {
    pub album: String,
    pub artist: String,
    pub year: Option<i32>,
    pub release_group: Option<String>,
    pub cover: Option<String>,
    pub lastfm_url: Option<String>,
    /// Which source, for the extra rank of more than one.
    pub source: &'static str,
    pub why: String,
    /// 0..=1 within its source.
    pub score: f64,
}

fn http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(Duration::from_secs(30)).user_agent(concat!("Cratarr/", env!("CARGO_PKG_VERSION"))).build()?)
}

// ---- ListenBrainz

#[derive(Deserialize)]
struct Recs {
    payload: RecsPayload,
}
#[derive(Deserialize)]
struct RecsPayload {
    mbids: Vec<Rec>,
}
#[derive(Deserialize)]
struct Rec {
    recording_mbid: String,
    score: f64,
}

/// The recordings ListenBrainz recommends for `user`, scored (none while it has made none).
async fn lb_recommended(http: &reqwest::Client, user: &str) -> Result<Vec<(String, f64)>> {
    let res = http.get(format!("{LB_API}/cf/recommendation/user/{user}/recording")).query(&[("count", "100")]).send().await.context("ListenBrainz did not answer")?;
    if res.status() == reqwest::StatusCode::NO_CONTENT || res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    let r: Recs = res.error_for_status()?.json().await.context("ListenBrainz's recommendations could not be read")?;
    Ok(r.payload.mbids.into_iter().map(|m| (m.recording_mbid, m.score)).collect())
}

/// The recordings of the newest Weekly Explorations made for `user` (`EXPLORATIONS` of them).
async fn lb_exploration(http: &reqwest::Client, user: &str) -> Result<Vec<String>> {
    let list: serde_json::Value = http
        .get(format!("{LB_API}/user/{user}/playlists/createdfor"))
        .query(&[("count", "25")])
        .send()
        .await
        .context("ListenBrainz did not answer")?
        .error_for_status()?
        .json()
        .await?;
    let ext = "https://musicbrainz.org/doc/jspf#playlist";
    let ids: Vec<String> = list["playlists"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["playlist"]["extension"][ext]["additional_metadata"]["algorithm_metadata"]["source_patch"].as_str() == Some("weekly-exploration"))
        .filter_map(|p| p["playlist"]["identifier"].as_str().and_then(|u| u.rsplit('/').next()).map(str::to_string))
        .take(EXPLORATIONS)
        .collect();
    let mut out = Vec::new();
    for id in ids {
        let pl: serde_json::Value = http.get(format!("{LB_API}/playlist/{id}")).send().await?.error_for_status()?.json().await?;
        for r in pl["playlist"]["track"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t["identifier"].as_array()?.iter().find_map(|i| i.as_str()?.rsplit('/').next().map(str::to_string)))
        {
            if !out.contains(&r) {
                out.push(r);
            }
        }
    }
    Ok(out)
}

/// Recordings' albums, as ListenBrainz names them: (recording, album).
async fn lb_albums(http: &reqwest::Client, recordings: &[String], progress: &Progress) -> Result<HashMap<String, Candidate>> {
    let mut out = HashMap::new();
    for (i, batch) in recordings.chunks(META_BATCH).enumerate() {
        progress("Naming ListenBrainz's suggestions", (i * META_BATCH) as u64, Some(recordings.len() as u64));
        let meta: HashMap<String, serde_json::Value> = http
            .get(format!("{LB_API}/metadata/recording/"))
            .query(&[("recording_mbids", batch.join(",").as_str()), ("inc", "artist release")])
            .send()
            .await
            .context("ListenBrainz did not answer")?
            .error_for_status()?
            .json()
            .await
            .context("ListenBrainz's names could not be read")?;
        for (rec, m) in meta {
            let r = &m["release"];
            let Some(album) = r["name"].as_str() else { continue };
            let artist = r["album_artist_name"].as_str().or(m["artist"]["name"].as_str()).unwrap_or_default();
            let rg = r["release_group_mbid"].as_str().filter(|s| !s.is_empty()).map(str::to_string);
            let cover = rg
                .as_ref()
                .map(|g| format!("https://coverartarchive.org/release-group/{g}/front-250"))
                .or_else(|| r["caa_release_mbid"].as_str().map(|m| format!("https://coverartarchive.org/release/{m}/front-250")));
            out.insert(
                rec,
                Candidate {
                    album: album.to_string(),
                    artist: artist.to_string(),
                    year: r["year"].as_i64().map(|y| y as i32),
                    release_group: rg,
                    cover,
                    ..Candidate::default()
                },
            );
        }
    }
    Ok(out)
}

/// What ListenBrainz knows of these release groups, `META_BATCH` at a time: MusicBrainz's type for each, and its
/// genres (the album's own, else its artist's).
async fn lb_group_facts(http: &reqwest::Client, groups: &[String]) -> Result<HashMap<String, (Option<String>, Vec<String>)>> {
    let mut out = HashMap::new();
    for batch in groups.chunks(META_BATCH) {
        let meta: HashMap<String, serde_json::Value> = http
            .get(format!("{LB_API}/metadata/release_group/"))
            .query(&[("release_group_mbids", batch.join(",").as_str()), ("inc", "tag")])
            .send()
            .await
            .context("ListenBrainz did not answer")?
            .error_for_status()?
            .json()
            .await
            .context("ListenBrainz's facts could not be read")?;
        for (g, m) in meta {
            let kind = m["release_group"]["type"].as_str().filter(|t| !t.is_empty()).map(str::to_string);
            let own = genres_of(&m["tag"]["release_group"]);
            out.insert(g, (kind, if own.is_empty() { genres_of(&m["tag"]["artist"]) } else { own }));
        }
    }
    Ok(out)
}

/// The tags that name one of MusicBrainz's genres (its curated list: those with a `genre_mbid`; no "seen live"), the
/// most given first, `GENRES` at most; one given far less than the most (under a quarter as often) is left out, so a
/// stray vote does not show.
fn genres_of(tags: &serde_json::Value) -> Vec<String> {
    let mut given: Vec<(i64, String)> = tags
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| t["genre_mbid"].as_str().is_some_and(|m| !m.is_empty()))
        .filter_map(|t| Some((t["count"].as_i64().unwrap_or(0), t["tag"].as_str()?.to_string())))
        .collect();
    given.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let least = given.first().map_or(0, |g| (g.0 / 4).max(1));
    let mut out: Vec<String> = Vec::new();
    for (n, tag) in given {
        if n < least || out.len() == GENRES {
            break;
        }
        if !out.contains(&tag) {
            out.push(tag);
        }
    }
    out
}

/// ListenBrainz's suggestions for `user`, and a note when it has none yet.
async fn from_listenbrainz(user: &str, progress: &Progress) -> Result<(Vec<Candidate>, Vec<String>)> {
    let http = http()?;
    progress("Asking ListenBrainz", 0, None);
    let recs = lb_recommended(&http, user).await?;
    let exploring = lb_exploration(&http, user).await.unwrap_or_default();
    let mut notes = Vec::new();
    if recs.is_empty() {
        notes.push("ListenBrainz has made no recommendations for you yet.".to_string());
    }
    let all: Vec<String> = recs.iter().map(|r| r.0.clone()).chain(exploring.iter().cloned()).collect::<HashSet<_>>().into_iter().collect();
    let named = lb_albums(&http, &all, progress).await?;
    let most = recs.iter().map(|r| r.1).fold(0.0f64, f64::max).max(f64::EPSILON);
    let mut out = Vec::new();
    for (rec, score) in &recs {
        if let Some(c) = named.get(rec) {
            out.push(Candidate { source: "listenbrainz", why: "Recommended by ListenBrainz".into(), score: score / most, ..c.clone() });
        }
    }
    for rec in &exploring {
        if let Some(c) = named.get(rec) {
            out.push(Candidate { source: "exploration", why: "In your Weekly Exploration".into(), score: 0.6, ..c.clone() });
        }
    }
    Ok((out, notes))
}

// ---- Last.fm

/// One call to Last.fm, signed with the account's sign-in where it has one.
async fn fm(http: &reqwest::Client, auth: &Auth, method: &str, params: &[(&str, &str)]) -> Result<serde_json::Value> {
    let mut q: std::collections::BTreeMap<&str, String> = std::collections::BTreeMap::new();
    q.insert("method", method.to_string());
    q.insert("api_key", auth.key.clone());
    for (k, v) in params {
        q.insert(*k, v.to_string());
    }
    if let Some((session, secret)) = &auth.signed {
        q.insert("sk", session.clone());
        let sig = super::scrobble::signature(&q, secret);
        q.insert("api_sig", sig);
    }
    q.insert("format", "json".into());
    tokio::time::sleep(FM_PAUSE).await;
    let body: serde_json::Value = http.get(FM_API).query(&q).send().await.context("Last.fm did not answer")?.json().await?;
    if let Some(why) = body["message"].as_str().filter(|_| body.get("error").is_some()) {
        anyhow::bail!("Last.fm: {why}");
    }
    Ok(body)
}

/// A number Last.fm may send as a string.
fn number(v: &serde_json::Value) -> f64 {
    v.as_f64().or_else(|| v.as_str()?.parse().ok()).unwrap_or(0.0)
}

/// The best known album of each artist like the user's most played that the library has none of.
async fn from_lastfm(user: &str, auth: &Auth, artists_had: &HashSet<String>, progress: &Progress) -> Result<Vec<Candidate>> {
    let http = http()?;
    progress("Asking Last.fm", 0, None);
    let top = fm(&http, auth, "user.gettopartists", &[("user", user), ("period", "6month"), ("limit", TOP_ARTISTS)]).await?;
    let top: Vec<String> = top["topartists"]["artist"].as_array().into_iter().flatten().filter_map(|a| a["name"].as_str().map(str::to_string)).collect();
    // Each artist like them: how alike in all, and which of the user's it is like (the most alike first).
    let mut like: HashMap<String, (String, f64, Vec<(f64, String)>)> = HashMap::new();
    for (i, t) in top.iter().enumerate() {
        progress("Finding artists like yours", i as u64, Some(top.len() as u64));
        let Ok(s) = fm(&http, auth, "artist.getsimilar", &[("artist", t), ("limit", SIMILAR_PER), ("autocorrect", "1")]).await else { continue };
        for a in s["similarartists"]["artist"].as_array().into_iter().flatten() {
            let Some(name) = a["name"].as_str() else { continue };
            let k = loose(name);
            if artists_had.contains(&k) || top.iter().any(|x| loose(x) == k) {
                continue;
            }
            let m = number(&a["match"]);
            let e = like.entry(k).or_insert_with(|| (name.to_string(), 0.0, Vec::new()));
            e.1 += m;
            e.2.push((m, t.clone()));
        }
    }
    let mut ranked: Vec<(String, f64, Vec<(f64, String)>)> = like.into_values().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked.truncate(NEW_ARTISTS);
    let most = ranked.first().map_or(1.0, |r| r.1).max(f64::EPSILON);
    let mut out = Vec::new();
    for (i, (name, score, because)) in ranked.into_iter().enumerate() {
        progress("Choosing their albums", i as u64, Some(NEW_ARTISTS as u64));
        let Ok(albums) = fm(&http, auth, "artist.gettopalbums", &[("artist", &name), ("limit", "3"), ("autocorrect", "1")]).await else { continue };
        let Some(a) = albums["topalbums"]["album"].as_array().into_iter().flatten().find(|a| a["name"].as_str().is_some_and(|n| !n.is_empty() && n != "(null)")) else {
            continue;
        };
        let mut because = because;
        because.sort_by(|a, b| b.0.total_cmp(&a.0));
        let names: Vec<String> = because.into_iter().map(|b| b.1).take(2).collect();
        let why = match names.as_slice() {
            // Not ", which you play" after each: every card would say it (the owner, 2026-10-24).
            [one] => format!("Like {one}"),
            [one, two, ..] => format!("Like {one} and {two}"),
            [] => "Like what you play".into(),
        };
        let cover = a["image"].as_array().and_then(|i| i.iter().rev().find_map(|x| x["#text"].as_str().filter(|s| !s.is_empty()))).map(str::to_string);
        out.push(Candidate {
            album: a["name"].as_str().unwrap_or_default().to_string(),
            artist: name,
            cover,
            lastfm_url: a["url"].as_str().map(str::to_string),
            source: "lastfm",
            why,
            score: score / most,
            ..Candidate::default()
        });
    }
    Ok(out)
}

// ---- The library, the answers, and one list

/// What the library has: the release groups of every folder's albums (Music and Incoming), and its albums by name
/// (the import's matcher), and its artists (loosely, as Last.fm's are compared).
pub struct Have {
    pub release_groups: HashSet<String>,
    pub matcher: Matcher,
    /// The artists the library has albums by: Last.fm's new artists are none of these.
    pub artists: HashSet<String>,
    /// Those and every artist a track credits (compilations, guests): "New to you" is for none of these (the owner,
    /// 2026-10-24: one on a compilation is not new).
    pub known: HashSet<String>,
}

impl Have {
    fn has(&self, c: &Candidate) -> bool {
        c.release_group.as_deref().is_some_and(|g| self.release_groups.contains(g)) || self.matcher.find_album(&c.album, Some(&c.artist)).is_some()
    }
}

/// The library as Discover compares with it.
pub fn have(data_dir: &Path, cat: &crate::catalogue::Catalogue, artists: impl IntoIterator<Item = String>) -> Result<Have> {
    let mut release_groups = HashSet::new();
    for i in 0..cat.folders().len() {
        for f in cat.album_facts(i as i64)? {
            if let Some(g) = f.release_group.filter(|g| !g.is_empty()) {
                release_groups.insert(g);
            }
        }
    }
    {
        let artists: HashSet<String> = artists.into_iter().map(|a| loose(&a)).collect();
        Ok(Have { release_groups, matcher: Matcher::read(&data_dir.join("library.sqlite"))?, known: artists.clone(), artists })
    }
}

/// An album's key: its release group, else its artist's and its name's.
fn key_of(c: &Candidate) -> String {
    c.release_group.clone().unwrap_or_else(|| format!("{}|{}", key(&c.artist), key(&c.album)))
}

/// The candidates as one list: the library's albums and the ones answered left out (and every album of an artist said
/// no to twice), the same album once with every reason, ranked by its sources' scores (a second source counting
/// extra), the most `MOST`.
pub fn one_list(candidates: Vec<Candidate>, have: &dyn Fn(&Candidate) -> bool, artists_had: &HashSet<String>, answered: &HashSet<String>, no_artists: &HashSet<String>) -> Vec<Suggestion> {
    let mut by: HashMap<String, (Suggestion, HashSet<&'static str>)> = HashMap::new();
    let mut order = Vec::new();
    for c in candidates {
        let k = key_of(&c);
        let artist_key = loose(&c.artist);
        if answered.contains(&k) || no_artists.contains(&artist_key) || have(&c) {
            continue;
        }
        let e = by.entry(k.clone()).or_insert_with(|| {
            order.push(k.clone());
            let mut links = Vec::new();
            if let Some(g) = &c.release_group {
                links.push(Link { name: "MusicBrainz".into(), url: format!("https://musicbrainz.org/release-group/{g}") });
                links.push(Link { name: "ListenBrainz".into(), url: format!("https://listenbrainz.org/album/{g}") });
            }
            (
                Suggestion {
                    key: k.clone(),
                    album: c.album.clone(),
                    artist: c.artist.clone(),
                    artist_key: artist_key.clone(),
                    year: c.year,
                    release_group: c.release_group.clone(),
                    cover: c.cover.clone(),
                    links,
                    new_artist: !artists_had.contains(&artist_key),
                    why: Vec::new(),
                    sources: Vec::new(),
                    bandcamp: None,
                    kind: None,
                    genres: Vec::new(),
                    length_secs: None,
                    track_count: None,
                    listeners: None,
                    score: 0.0,
                },
                HashSet::new(),
            )
        });
        let (s, sources) = e;
        if !s.why.contains(&c.why) {
            s.why.push(c.why.clone());
        }
        let named = if c.source == "lastfm" { "Last.fm" } else { "ListenBrainz" };
        if !s.sources.iter().any(|x| x == named) {
            s.sources.push(named.to_string());
        }
        if let Some(u) = &c.lastfm_url
            && !s.links.iter().any(|l| l.name == "Last.fm")
        {
            s.links.push(Link { name: "Last.fm".into(), url: u.clone() });
        }
        if s.cover.is_none() {
            s.cover = c.cover.clone();
        }
        // A second source counts extra; more of one source's recordings on the album, a little.
        s.score += if sources.insert(c.source) { c.score + if sources.len() > 1 { 1.0 } else { 0.0 } } else { c.score * 0.25 };
    }
    let mut out: Vec<Suggestion> = order.into_iter().filter_map(|k| by.remove(&k).map(|e| e.0)).collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.artist.cmp(&b.artist)));
    out.truncate(MOST);
    out
}

/// Where an album can be heard elsewhere beyond the services that named it (the owner, 2026-10-24: "Bandcamp and
/// YouTube links next to the Last.fm links"): its own Bandcamp and YouTube pages where MusicBrainz keeps them (`urls`:
/// the album's releases' and its own, and the Bandcamp page found for it), else a search for it on YouTube, named so;
/// none on Bandcamp where no page was found (the owner, 2026-10-24, decision 0071).
pub fn listen_links(urls: &[String], artist: &str, album: &str) -> Vec<Link> {
    let find = |hosts: &[&str]| urls.iter().find(|u| reqwest::Url::parse(u).ok().and_then(|x| x.host_str().map(str::to_string)).is_some_and(|h| hosts.iter().any(|w| h == *w || h.ends_with(&format!(".{w}"))))).cloned();
    let words = format!("{artist} {album}");
    let search = |base: &str, key: &str, extra: &[(&str, &str)]| {
        let mut u = reqwest::Url::parse(base).expect("a fixed address");
        u.query_pairs_mut().append_pair(key, &words).extend_pairs(extra);
        u.to_string()
    };
    let mut links: Vec<Link> = find(&["bandcamp.com"]).map(|url| Link { name: "Bandcamp".into(), url }).into_iter().collect();
    links.push(match find(&["youtube.com", "youtu.be", "music.youtube.com"]) {
        Some(url) => Link { name: "YouTube".into(), url },
        None => Link { name: "Search YouTube".into(), url: search("https://www.youtube.com/results", "search_query", &[]) },
    });
    links
}

#[derive(Deserialize)]
struct MbUrl {
    resource: String,
}

#[derive(Deserialize)]
struct MbRelation {
    url: Option<MbUrl>,
}

/// What MusicBrainz keeps of a release group's releases (`mb_urls`).
#[derive(Debug, Default)]
struct MbAlbum {
    /// Their pages ("url" relations).
    urls: Vec<String>,
    /// Their artists' ids, for their own pages.
    artists: Vec<String>,
    /// How many tracks one of them has (an official one first), and how long it is when every track's length is known.
    tracks: Option<u32>,
    length_secs: Option<u32>,
}

/// The pages MusicBrainz keeps for a release group and its releases (their "url" relations), its releases' artists,
/// and one release's tracks and length, in the one call.
async fn mb_urls(http: &reqwest::Client, release_group: &str) -> Result<MbAlbum> {
    #[derive(Deserialize)]
    struct Artist {
        id: String,
    }
    #[derive(Deserialize)]
    struct Credit {
        artist: Artist,
    }
    #[derive(Deserialize)]
    struct Track {
        length: Option<u64>,
    }
    #[derive(Deserialize)]
    struct Medium {
        #[serde(rename = "track-count", default)]
        count: u32,
        #[serde(default)]
        tracks: Vec<Track>,
    }
    #[derive(Deserialize)]
    struct Release {
        #[serde(default)]
        relations: Vec<MbRelation>,
        #[serde(rename = "artist-credit", default)]
        credits: Vec<Credit>,
        status: Option<String>,
        #[serde(default)]
        media: Vec<Medium>,
    }
    #[derive(Deserialize)]
    struct Browse {
        #[serde(default)]
        releases: Vec<Release>,
    }
    let url = format!("https://musicbrainz.org/ws/2/release?release-group={release_group}&inc=url-rels+artist-credits+media+recordings&fmt=json&limit=10");
    let b: Browse = http.get(url).send().await?.error_for_status()?.json().await?;
    let mut artists: Vec<String> = Vec::new();
    for id in b.releases.iter().flat_map(|r| r.credits.iter().map(|c| c.artist.id.clone())) {
        if !artists.contains(&id) {
            artists.push(id);
        }
    }
    let with_tracks = |r: &&Release| r.media.iter().any(|m| m.count > 0);
    let one = b.releases.iter().filter(with_tracks).find(|r| r.status.as_deref() == Some("Official")).or_else(|| b.releases.iter().find(with_tracks));
    let tracks = one.map(|r| r.media.iter().map(|m| m.count).sum::<u32>());
    let length_secs = one.and_then(|r| {
        let all: Vec<&Track> = r.media.iter().flat_map(|m| &m.tracks).collect();
        let ms: Option<u64> = all.iter().map(|t| t.length).sum();
        ms.filter(|_| !all.is_empty()).map(|ms| (ms / 1000) as u32)
    });
    Ok(MbAlbum { urls: b.releases.into_iter().flat_map(|r| r.relations).filter_map(|r| r.url.map(|u| u.resource)).collect(), artists, tracks, length_secs })
}

/// The pages MusicBrainz keeps for an artist (their "url" relations).
async fn mb_artist_urls(http: &reqwest::Client, artist: &str) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Artist {
        #[serde(default)]
        relations: Vec<MbRelation>,
    }
    let url = format!("https://musicbrainz.org/ws/2/artist/{artist}?inc=url-rels&fmt=json");
    let a: Artist = http.get(url).send().await?.error_for_status()?.json().await?;
    Ok(a.relations.into_iter().filter_map(|r| r.url.map(|u| u.resource)).collect())
}

/// Two names the same as the import's matcher compares them (exactly, else loosely).
fn same(a: &str, b: &str) -> bool {
    key(a) == key(b) || loose(a) == loose(b)
}

/// An album's page on Bandcamp (decision 0071): its own on MusicBrainz, else found on its artist's own Bandcamp page
/// (MusicBrainz's for the artist) by its title, else Bandcamp's search box where artist and title both match; none
/// otherwise. Bandcamp is asked at most once a second, as MusicBrainz is.
async fn bandcamp_page(http: &reqwest::Client, urls: &[String], artists: &[String], artist: &str, album: &str) -> Option<String> {
    if let Some(p) = urls.iter().find_map(|u| crate::bandcamp::album_page(u)) {
        return Some(p);
    }
    let bc = crate::bandcamp::Bandcamp::get();
    if let Some(id) = artists.first() {
        let own = mb_artist_urls(http, id).await.unwrap_or_default();
        tokio::time::sleep(MB_PAUSE).await;
        let page = own.iter().filter_map(|u| crate::bandcamp::page(u)).find(|p| reqwest::Url::parse(p).ok().and_then(|u| u.host_str().map(|h| h != "bandcamp.com")).unwrap_or(false));
        if let Some(page) = page {
            let releases = bc.releases(&page).await.unwrap_or_default();
            tokio::time::sleep(MB_PAUSE).await;
            if let Some((_, url)) = releases.into_iter().find(|(t, _)| same(t, album)) {
                return Some(url);
            }
        }
    }
    let found = bc.search(artist, album).await.unwrap_or_default();
    tokio::time::sleep(MB_PAUSE).await;
    found.into_iter().find(|(t, a, _)| same(t, album) && same(a, artist)).map(|(_, _, url)| url)
}

/// A name as MusicBrainz's search reads it: in quotes, its own quotes and backslashes escaped.
fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The release group MusicBrainz is sure an album is (its search's score 95 or more, the name the same as the matcher
/// reads names): its id and year.
async fn mb_release_group(http: &reqwest::Client, artist: &str, album: &str) -> Result<Option<(String, Option<i32>)>> {
    #[derive(Deserialize)]
    struct Group {
        id: String,
        #[serde(default)]
        score: u32,
        #[serde(default)]
        title: String,
        #[serde(rename = "first-release-date", default)]
        first: String,
    }
    #[derive(Deserialize)]
    struct Search {
        #[serde(rename = "release-groups", default)]
        groups: Vec<Group>,
    }
    let mut url = reqwest::Url::parse("https://musicbrainz.org/ws/2/release-group")?;
    url.query_pairs_mut()
        .append_pair("query", &format!("releasegroup:{} AND artist:{}", quoted(album), quoted(artist)))
        .append_pair("fmt", "json")
        .append_pair("limit", "3");
    let s: Search = http.get(url).send().await?.error_for_status()?.json().await?;
    Ok(s.groups.into_iter().find(|g| g.score >= 95 && loose(&g.title) == loose(album)).map(|g| (g.id, g.first.get(..4).and_then(|y| y.parse().ok()))))
}

/// MusicBrainz asks for at most one request a second.
const MB_PAUSE: Duration = Duration::from_millis(1100);

/// Looks up the account's suggestions now (blocking parts apart) and keeps them.
pub async fn look(serve: &Arc<crate::serve::Serve>, cat: Arc<crate::catalogue::Catalogue>, account: &str, who: &Who, progress: &Progress) -> Result<Found> {
    let dir = serve.data_dir.clone().context("Discover needs Cratarr's data folder")?;
    progress("Reading your library", 0, None);
    let (lib, d) = (serve.library.clone(), dir.clone());
    let have = tokio::task::spawn_blocking(move || -> Result<Have> {
        let artists = lib.artist_albums()?.into_iter().map(|r| r.1).collect::<Vec<_>>();
        let mut have = have(&d, &cat, artists)?;
        have.known = have.artists.iter().cloned().chain(lib.track_artist_names()?.iter().map(|a| loose(a))).collect();
        Ok(have)
    })
    .await??;
    let mut candidates = Vec::new();
    let mut notes = Vec::new();
    if let Some(user) = &who.listenbrainz {
        match from_listenbrainz(user, progress).await {
            Ok((c, n)) => {
                candidates.extend(c);
                notes.extend(n);
            }
            Err(e) => notes.push(format!("ListenBrainz could not be asked: {e:#}")),
        }
    }
    if let Some((user, auth)) = &who.lastfm {
        match from_lastfm(user, auth, &have.artists, progress).await {
            Ok(c) => candidates.extend(c),
            Err(e) => notes.push(format!("Last.fm could not be asked: {e:#}")),
        }
    }
    // Albums named without their MusicBrainz id (Last.fm's mostly): looked up there.
    let http = http()?;
    let unknown = candidates.iter().filter(|c| c.release_group.is_none()).count() as u64;
    let mut n = 0;
    for c in candidates.iter_mut().filter(|c| c.release_group.is_none()) {
        progress("Finding albums on MusicBrainz", n, Some(unknown));
        n += 1;
        if let Ok(Some((g, year))) = mb_release_group(&http, &c.artist, &c.album).await {
            if c.cover.is_none() {
                c.cover = Some(format!("https://coverartarchive.org/release-group/{g}/front-250"));
            }
            c.year = c.year.or(year);
            c.release_group = Some(g);
        }
        tokio::time::sleep(MB_PAUSE).await;
    }
    progress("Leaving out what you have", 0, None);
    let (ud, a) = (serve.userdata.clone(), account.to_string());
    let (answered, no_artists) = tokio::task::spawn_blocking(move || ud.discover_answered(&a)).await??;
    let mut albums = one_list(candidates, &|c: &Candidate| have.has(c), &have.known, &answered, &no_artists);
    // What each is and its genres, from ListenBrainz in a few calls, so the cards show them from the first.
    progress("Reading what each album is", 0, None);
    let groups: Vec<String> = albums.iter().filter_map(|s| s.release_group.clone()).collect();
    match lb_group_facts(&http, &groups).await {
        Ok(facts) => {
            for s in albums.iter_mut() {
                if let Some((kind, genres)) = s.release_group.as_ref().and_then(|g| facts.get(g)) {
                    (s.kind, s.genres) = (kind.clone(), genres.clone());
                }
            }
        }
        Err(e) => tracing::info!("discover: ListenBrainz's facts of the albums could not be read: {e:#}"),
    }
    // Shown the moment they are known (the owner, 2026-10-24, 0.9): kept now, so the page lists them while the rest is
    // looked up behind them, and kept again as it fills in (the page reads them again as this step moves on).
    let at = super::moment::now_secs();
    keep_found(serve, account, &Found { at, albums: albums.clone(), notes: notes.clone() }).await?;
    // Where to hear each: its Bandcamp and YouTube pages as MusicBrainz knows them, else a search there; and its album
    // page on Bandcamp, found further where MusicBrainz has none (decision 0071), to play it from.
    let total = albums.len() as u64;
    for i in 0..albums.len() {
        if i > 0 && i % KEEP_EVERY == 0 {
            keep_found(serve, account, &Found { at, albums: albums.clone(), notes: notes.clone() }).await?;
        }
        let s = &mut albums[i];
        progress(LISTEN_STEP, i as u64, Some(total));
        let mut mb = match &s.release_group {
            Some(g) => {
                let got = mb_urls(&http, g).await.unwrap_or_default();
                tokio::time::sleep(MB_PAUSE).await;
                got
            }
            None => MbAlbum::default(),
        };
        (s.track_count, s.length_secs) = (mb.tracks, mb.length_secs);
        s.bandcamp = bandcamp_page(&http, &mb.urls, &mb.artists, &s.artist, &s.album).await;
        if let Some(p) = &s.bandcamp {
            mb.urls.insert(0, p.clone());
        }
        s.links.extend(listen_links(&mb.urls, &s.artist, &s.album));
        // How many listen to it, where the account asks Last.fm (its tracks too, where MusicBrainz had none).
        if let Some((_, auth)) = &who.lastfm
            && let Ok(info) = fm(&http, auth, "album.getinfo", &[("artist", &s.artist), ("album", &s.album), ("autocorrect", "1")]).await
        {
            let info = &info["album"];
            s.listeners = Some(number(&info["listeners"]) as u64).filter(|n| *n > 0);
            if s.track_count.is_none()
                && let Some(tracks) = info["tracks"]["track"].as_array().filter(|t| !t.is_empty())
            {
                s.track_count = Some(tracks.len() as u32);
                let secs: Vec<f64> = tracks.iter().map(|t| number(&t["duration"])).collect();
                s.length_secs = secs.iter().all(|d| *d > 0.0).then(|| secs.iter().sum::<f64>() as u32);
            }
        }
    }
    let found = Found { at, albums, notes };
    keep_found(serve, account, &found).await?;
    Ok(found)
}

/// The step after which the list is kept and can be shown (lib's Discover reads it again from this step on).
pub const LISTEN_STEP: &str = "Finding where to listen";
/// Kept again after this many albums' links, so the page fills in as it goes.
const KEEP_EVERY: usize = 10;

/// What a look found, kept for the account (`kept` reads it).
async fn keep_found(serve: &Arc<crate::serve::Serve>, account: &str, found: &Found) -> Result<()> {
    let (ud, a, json, at) = (serve.userdata.clone(), account.to_string(), serde_json::to_string(found)?, found.at);
    tokio::task::spawn_blocking(move || ud.set_discover_found(&a, at, &json)).await??;
    Ok(())
}

/// What was found last for the account, the albums answered since left out.
pub fn kept(serve: &crate::serve::Serve, account: &str) -> Result<Option<Found>> {
    let Some((_, json)) = serve.userdata.discover_found(account)? else { return Ok(None) };
    let mut found: Found = serde_json::from_str(&json).unwrap_or_default();
    let (answered, no_artists) = serve.userdata.discover_answered(account)?;
    found.albums.retain(|s| !answered.contains(&s.key) && !no_artists.contains(&s.artist_key));
    // A look kept before 0.9 said ", which you play" after each reason: read without it.
    for w in found.albums.iter_mut().flat_map(|s| s.why.iter_mut()) {
        if let Some(short) = w.strip_suffix(", which you play") {
            *w = short.to_string();
        }
    }
    Ok(Some(found))
}

/// Whose services the account uses: ListenBrainz as it scrobbles (on, not stopped); Last.fm as it scrobbles, signed in,
/// else the user and key its import kept (decision 0067).
pub fn who(serve: &crate::serve::Serve, account: &str) -> Result<Who> {
    let ud = &serve.userdata;
    let targets = ud.scrobble_targets(account)?;
    let on = |t: &str| targets.iter().find(|x| x.target == t && x.on && x.broken.is_none()).map(|x| x.name.clone()).filter(|n| !n.is_empty());
    // As it scrobbles, else the user its import keeps (the owner, 2026-10-24).
    let listenbrainz = on(super::scrobble::LISTENBRAINZ).or(ud.import_choice(&format!("listenbrainz:{account}"))?.filter(|u| !u.is_empty()));
    let signed = ud
        .scrobble_secret(account, super::scrobble::LASTFM)?
        .and_then(|s| serde_json::from_str::<super::scrobble::Secret>(&s).ok())
        .and_then(|s| Some(Auth { key: s.api_key?, signed: Some((s.session?, s.api_secret?)) }));
    let kept = ud.import_choice(&format!("lastfm:{account}"))?.and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok());
    let kept_user = kept.as_ref().and_then(|k| k["user"].as_str()).filter(|u| !u.is_empty()).map(str::to_string);
    let lastfm = match (on(super::scrobble::LASTFM), signed) {
        (Some(user), Some(auth)) => Some((user, auth)),
        _ => kept.as_ref().and_then(|k| Some((kept_user.clone()?, Auth { key: k["key"].as_str()?.to_string(), signed: None }))),
    };
    Ok(Who { listenbrainz, lastfm })
}

/// Lidarr, where it is connected and switched on: Want fetches through it.
/// What Want adds an album with: Lidarr's choices in Connections.
pub fn add_with(app: &super::App) -> crate::lidarr::AddWith {
    let cfg = app.incoming.lidarr_state().0;
    crate::lidarr::AddWith {
        quality: cfg.as_ref().and_then(|c| c.quality_profile),
        metadata: cfg.as_ref().and_then(|c| c.metadata_profile),
        root: cfg.and_then(|c| c.root_folder),
    }
}

pub fn lidarr(app: &super::App) -> Option<crate::lidarr::Lidarr> {
    app.incoming.lidarr_state().1
}

/// Looks up the account's suggestions as a task (`app::tasks`), its result the `Found`.
pub fn start(app: &Arc<super::App>, serve: Arc<crate::serve::Serve>, account: &str, who: Who) -> Result<super::tasks::Task, String> {
    let (cat, a) = (app.library.catalogue(), account.to_string());
    super::tasks::spawn(KIND, account, false, move |h| async move {
        let progress = move |name: &str, done: u64, total: Option<u64>| h.step(name, done, total);
        look(&serve, cat, &a, &who, &progress).await
    })
}

/// Hide (the owner, 2026-10-24: "hide suggestions without disliking"): it leaves the list and never comes back, and
/// says nothing of its artist (only Not interested counts towards leaving an artist out). Its undo token.
pub fn hide(serve: &crate::serve::Serve, account: &str, key: &str) -> Result<String> {
    answer(serve, account, key, "hidden")
}

/// Not interested (decision 0069): it never comes back, and an artist said no to twice is left out. Its undo token.
pub fn not_interested(serve: &crate::serve::Serve, account: &str, key: &str) -> Result<String> {
    answer(serve, account, key, "no")
}

fn answer(serve: &crate::serve::Serve, account: &str, key: &str, answer: &str) -> Result<String> {
    let s = kept(serve, account)?.and_then(|f| f.albums.into_iter().find(|s| s.key == key)).context("That suggestion is gone; look again")?;
    serve.userdata.answer_discover(account, &s.key, &s.artist_key, answer)?;
    Ok(format!("{UNDO}{answer}:{key}"))
}

/// Discover's undo tokens (the one undo, decision 0026; the owner, 2026-10-24: "like all our actions, there should be
/// an undo"): `du:<answer>:<key>` takes the answer back, `dr:<answer>:<key>` gives it again. The signed-in account's own.
pub const UNDO: &str = "du:";
const REDO: &str = "dr:";

/// A Discover token undone or redone for `account`: the token that reverses it.
pub fn undo(serve: &crate::serve::Serve, account: &str, token: &str) -> Result<String> {
    let (back, rest) = match (token.strip_prefix(UNDO), token.strip_prefix(REDO)) {
        (Some(r), _) => (true, r),
        (_, Some(r)) => (false, r),
        _ => anyhow::bail!("Not Discover's undo"),
    };
    let (answer, key) = rest.split_once(':').context("Not Discover's undo")?;
    anyhow::ensure!(answer == "no" || answer == "hidden", "Not Discover's undo");
    if back {
        serve.userdata.unanswer_discover(account, key)?;
        Ok(format!("{REDO}{answer}:{key}"))
    } else {
        self::answer(serve, account, key, answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_albums_own_pages_else_a_search_there() {
        let own = listen_links(
            &["https://www.discogs.com/master/1".into(), "https://artist.bandcamp.com/album/x".into(), "https://www.youtube.com/playlist?list=OLAK5uy".into()],
            "Bonobo",
            "Migration",
        );
        assert_eq!(own[0], Link { name: "Bandcamp".into(), url: "https://artist.bandcamp.com/album/x".into() });
        assert_eq!(own[1].name, "YouTube");
        let none = listen_links(&[], "Bonobo", "Migration");
        assert_eq!(none.len(), 1, "no Bandcamp link without its page (decision 0071)");
        assert_eq!(none[0].name, "Search YouTube");
        assert!(none[0].url.contains("search_query=Bonobo+Migration"));
    }

    fn c(album: &str, artist: &str, rg: Option<&str>, source: &'static str, why: &str, score: f64) -> Candidate {
        Candidate { album: album.into(), artist: artist.into(), release_group: rg.map(Into::into), source, why: why.into(), score, ..Candidate::default() }
    }

    #[test]
    fn one_album_from_two_sources_is_one_suggestion_with_both_reasons_and_first() {
        let list = one_list(
            vec![
                c("Black Sands", "Bonobo", Some("rg-bs"), "listenbrainz", "Recommended by ListenBrainz", 0.9),
                c("Moon Safari", "Air", None, "lastfm", "Like Bonobo", 1.0),
                c("Black Sands", "Bonobo", Some("rg-bs"), "exploration", "In your Weekly Exploration", 0.6),
                c("Black Sands", "Bonobo", Some("rg-bs"), "listenbrainz", "Recommended by ListenBrainz", 0.5),
            ],
            &|_: &Candidate| false,
            &HashSet::from([loose("Bonobo")]),
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].album, "Black Sands", "two sources rank higher");
        assert_eq!(list[0].why, ["Recommended by ListenBrainz", "In your Weekly Exploration"], "each reason once");
        assert!(!list[0].new_artist);
        assert!(list[1].new_artist, "an artist the library has nothing of");
        assert_eq!(list[1].key, "air|moonsafari", "without a release group, by artist and name");
    }

    #[test]
    fn what_the_library_has_and_what_was_answered_is_left_out() {
        let list = one_list(
            vec![
                c("Had", "X", Some("rg-had"), "lastfm", "w", 1.0),
                c("Said no", "Y", Some("rg-no"), "lastfm", "w", 1.0),
                c("Other", "Twice", Some("rg-tw"), "lastfm", "w", 1.0),
                c("Kept", "Z", Some("rg-ok"), "lastfm", "w", 1.0),
            ],
            &|c: &Candidate| c.release_group.as_deref() == Some("rg-had"),
            &HashSet::new(),
            &HashSet::from(["rg-no".to_string()]),
            &HashSet::from([loose("Twice")]),
        );
        assert_eq!(list.iter().map(|s| s.album.as_str()).collect::<Vec<_>>(), ["Kept"]);
    }

    #[test]
    fn genres_are_tags_that_name_a_genre_the_most_given_first() {
        let tags = serde_json::json!([
            {"tag": "rock", "count": 8, "genre_mbid": "g1"},
            {"tag": "alternative rock", "count": 12, "genre_mbid": "g2"},
            {"tag": "jazz", "count": 1, "genre_mbid": "g3"},
            {"tag": "alternative", "count": 30},
            {"tag": "pop rock", "count": 5, "genre_mbid": "g4"},
            {"tag": "indie rock", "count": 4, "genre_mbid": "g5"}
        ]);
        assert_eq!(genres_of(&tags), ["alternative rock", "rock", "pop rock"], "no tag without a genre, three at most, a stray vote left out");
        assert!(genres_of(&serde_json::json!([])).is_empty());
        assert_eq!(genres_of(&serde_json::json!([{"tag": "ambient", "count": 1, "genre_mbid": "g"}])), ["ambient"], "one vote is enough alone");
    }

    #[test]
    fn the_library_is_matched_by_release_group_or_by_name_and_artist() {
        let mut m = Matcher::default();
        m.add_album("al-1", "Moon Safari", Some("Air"));
        let have = Have { release_groups: HashSet::from(["rg-1".to_string()]), matcher: m, artists: HashSet::new(), known: HashSet::new() };
        assert!(have.has(&c("Anything", "Anyone", Some("rg-1"), "lastfm", "", 0.0)));
        assert!(have.has(&c("Moon Safari (Remastered)", "AIR", None, "lastfm", "", 0.0)), "loosely, as imports match");
        assert!(!have.has(&c("Talkie Walkie", "Air", Some("rg-2"), "lastfm", "", 0.0)));
    }
}
