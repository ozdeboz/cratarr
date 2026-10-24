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
/// How many suggestions are kept.
const MOST: usize = 60;
const LB_API: &str = "https://api.listenbrainz.org/1";
const FM_API: &str = "https://ws.audioscrobbler.com/2.0/";
/// Last.fm asks for no more than about five calls a second.
const FM_PAUSE: Duration = Duration::from_millis(250);
/// Of the user's most played artists, how many are looked from; of the artists like them, how many get an album.
const TOP_ARTISTS: &str = "25";
const SIMILAR_PER: &str = "10";
const NEW_ARTISTS: usize = 30;
/// ListenBrainz's recordings named at a time.
const META_BATCH: usize = 25;

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

/// The recordings of the newest Weekly Exploration made for `user`.
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
    let Some(newest) = list["playlists"].as_array().into_iter().flatten().find(|p| {
        p["playlist"]["extension"][ext]["additional_metadata"]["algorithm_metadata"]["source_patch"].as_str() == Some("weekly-exploration")
    }) else {
        return Ok(Vec::new());
    };
    let Some(id) = newest["playlist"]["identifier"].as_str().and_then(|u| u.rsplit('/').next()) else { return Ok(Vec::new()) };
    let pl: serde_json::Value = http.get(format!("{LB_API}/playlist/{id}")).send().await?.error_for_status()?.json().await?;
    Ok(pl["playlist"]["track"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| t["identifier"].as_array()?.iter().find_map(|i| i.as_str()?.rsplit('/').next().map(str::to_string)))
        .collect())
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
            [one] => format!("Like {one}, which you play"),
            [one, two, ..] => format!("Like {one} and {two}, which you play"),
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
    pub artists: HashSet<String>,
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
    Ok(Have { release_groups, matcher: Matcher::read(&data_dir.join("library.sqlite"))?, artists: artists.into_iter().map(|a| loose(&a)).collect() })
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
                    score: 0.0,
                },
                HashSet::new(),
            )
        });
        let (s, sources) = e;
        if !s.why.contains(&c.why) {
            s.why.push(c.why.clone());
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
/// the album's releases' and its own), else a search for it there, named so.
pub fn listen_links(urls: &[String], artist: &str, album: &str) -> Vec<Link> {
    let find = |hosts: &[&str]| urls.iter().find(|u| reqwest::Url::parse(u).ok().and_then(|x| x.host_str().map(str::to_string)).is_some_and(|h| hosts.iter().any(|w| h == *w || h.ends_with(&format!(".{w}"))))).cloned();
    let words = format!("{artist} {album}");
    let search = |base: &str, key: &str, extra: &[(&str, &str)]| {
        let mut u = reqwest::Url::parse(base).expect("a fixed address");
        u.query_pairs_mut().append_pair(key, &words).extend_pairs(extra);
        u.to_string()
    };
    vec![
        match find(&["bandcamp.com"]) {
            Some(url) => Link { name: "Bandcamp".into(), url },
            None => Link { name: "Search Bandcamp".into(), url: search("https://bandcamp.com/search", "q", &[("item_type", "a")]) },
        },
        match find(&["youtube.com", "youtu.be", "music.youtube.com"]) {
            Some(url) => Link { name: "YouTube".into(), url },
            None => Link { name: "Search YouTube".into(), url: search("https://www.youtube.com/results", "search_query", &[]) },
        },
    ]
}

/// The pages MusicBrainz keeps for a release group and its releases (their "url" relations).
async fn mb_urls(http: &reqwest::Client, release_group: &str) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Url {
        resource: String,
    }
    #[derive(Deserialize)]
    struct Relation {
        url: Option<Url>,
    }
    #[derive(Deserialize)]
    struct Release {
        #[serde(default)]
        relations: Vec<Relation>,
    }
    #[derive(Deserialize)]
    struct Browse {
        #[serde(default)]
        releases: Vec<Release>,
    }
    let url = format!("https://musicbrainz.org/ws/2/release?release-group={release_group}&inc=url-rels&fmt=json&limit=10");
    let b: Browse = http.get(url).send().await?.error_for_status()?.json().await?;
    Ok(b.releases.into_iter().flat_map(|r| r.relations).filter_map(|r| r.url.map(|u| u.resource)).collect())
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
        have(&d, &cat, artists)
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
    progress("Leaving out what you have", 0, None);
    let (ud, a) = (serve.userdata.clone(), account.to_string());
    let (answered, no_artists) = tokio::task::spawn_blocking(move || ud.discover_answered(&a)).await??;
    let mut albums = one_list(candidates, &|c: &Candidate| have.has(c), &have.artists, &answered, &no_artists);
    // Where to hear each: its Bandcamp and YouTube pages as MusicBrainz knows them, else a search there.
    let http = http()?;
    let total = albums.len() as u64;
    for (i, s) in albums.iter_mut().enumerate() {
        progress("Finding where to listen", i as u64, Some(total));
        let urls = match &s.release_group {
            Some(g) => {
                let got = mb_urls(&http, g).await.unwrap_or_default();
                tokio::time::sleep(MB_PAUSE).await;
                got
            }
            None => Vec::new(),
        };
        s.links.extend(listen_links(&urls, &s.artist, &s.album));
    }
    let found = Found { at: super::moment::now_secs(), albums, notes };
    let (ud, a, json) = (serve.userdata.clone(), account.to_string(), serde_json::to_string(&found)?);
    let at = found.at;
    tokio::task::spawn_blocking(move || ud.set_discover_found(&a, at, &json)).await??;
    Ok(found)
}

/// What was found last for the account, the albums answered since left out.
pub fn kept(serve: &crate::serve::Serve, account: &str) -> Result<Option<Found>> {
    let Some((_, json)) = serve.userdata.discover_found(account)? else { return Ok(None) };
    let mut found: Found = serde_json::from_str(&json).unwrap_or_default();
    let (answered, no_artists) = serve.userdata.discover_answered(account)?;
    found.albums.retain(|s| !answered.contains(&s.key) && !no_artists.contains(&s.artist_key));
    Ok(Some(found))
}

/// Whose services the account uses: ListenBrainz as it scrobbles (on, not stopped); Last.fm as it scrobbles, signed in,
/// else the user and key its import kept (decision 0067).
pub fn who(serve: &crate::serve::Serve, account: &str) -> Result<Who> {
    let ud = &serve.userdata;
    let targets = ud.scrobble_targets(account)?;
    let on = |t: &str| targets.iter().find(|x| x.target == t && x.on && x.broken.is_none()).map(|x| x.name.clone()).filter(|n| !n.is_empty());
    let listenbrainz = on(super::scrobble::LISTENBRAINZ);
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

/// Want (decision 0069): Lidarr fetches the suggestion into Incoming, and it leaves the list (`wanted`). What Lidarr
/// did, in words.
pub async fn want(app: &super::App, serve: &crate::serve::Serve, account: &str, key: &str) -> Result<String> {
    let l = lidarr(app).context("Lidarr is not connected, or is switched off")?;
    let s = kept(serve, account)?.and_then(|f| f.albums.into_iter().find(|s| s.key == key)).context("That suggestion is gone; look again")?;
    let said = l.want(s.release_group.as_deref(), &s.artist, &s.album).await?;
    serve.userdata.answer_discover(account, &s.key, &s.artist_key, "wanted")?;
    Ok(said)
}

/// Not interested (decision 0069): it never comes back, and an artist said no to twice is left out.
pub fn not_interested(serve: &crate::serve::Serve, account: &str, key: &str) -> Result<()> {
    let s = kept(serve, account)?.and_then(|f| f.albums.into_iter().find(|s| s.key == key)).context("That suggestion is gone; look again")?;
    serve.userdata.answer_discover(account, &s.key, &s.artist_key, "no")
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
        assert_eq!(none[0].name, "Search Bandcamp");
        assert!(none[0].url.starts_with("https://bandcamp.com/search?q=Bonobo+Migration"));
        assert_eq!(none[1].name, "Search YouTube");
        assert!(none[1].url.contains("search_query=Bonobo+Migration"));
    }

    fn c(album: &str, artist: &str, rg: Option<&str>, source: &'static str, why: &str, score: f64) -> Candidate {
        Candidate { album: album.into(), artist: artist.into(), release_group: rg.map(Into::into), source, why: why.into(), score, ..Candidate::default() }
    }

    #[test]
    fn one_album_from_two_sources_is_one_suggestion_with_both_reasons_and_first() {
        let list = one_list(
            vec![
                c("Black Sands", "Bonobo", Some("rg-bs"), "listenbrainz", "Recommended by ListenBrainz", 0.9),
                c("Moon Safari", "Air", None, "lastfm", "Like Bonobo, which you play", 1.0),
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
    fn the_library_is_matched_by_release_group_or_by_name_and_artist() {
        let mut m = Matcher::default();
        m.add_album("al-1", "Moon Safari", Some("Air"));
        let have = Have { release_groups: HashSet::from(["rg-1".to_string()]), matcher: m, artists: HashSet::new() };
        assert!(have.has(&c("Anything", "Anyone", Some("rg-1"), "lastfm", "", 0.0)));
        assert!(have.has(&c("Moon Safari (Remastered)", "AIR", None, "lastfm", "", 0.0)), "loosely, as imports match");
        assert!(!have.has(&c("Talkie Walkie", "Air", Some("rg-2"), "lastfm", "", 0.0)));
    }
}
