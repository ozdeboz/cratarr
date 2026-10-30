//! Discover (decision 0069; the owner, 2026-10-24: "I wouldn't be interested in suggestions within my library, but
//! suggestions outside of"; "we need a discover page next to playlists"): albums the library does not have, each with
//! why it is suggested, from the signed-in account's own listening services (decision 0068).
//!
//! - **ListenBrainz** (its user: the one the account scrobbles as): its recommendations for the user (recordings,
//!   scored; marked experimental, 204 while none are made) and the newest Weekly Exploration made for them, each
//!   recording named through ListenBrainz's metadata (its album's release group, album artist, year, cover).
//! - **Last.fm** (the user it scrobbles as, else the one it imported from; asked as the import asks, `lastfm::Auth`;
//!   without one, with the server's own key, `server_lastfm_key`): the account's most played artists in Cratarr and on
//!   Last.fm, the artists like them (`artist.getSimilar`), and of each such artist the library has not got their best
//!   known album (`artist.getTopAlbums`).
//! - **The library's artists' discographies** (0.9, no service needed): each artist's albums and EPs on MusicBrainz the
//!   library has none of, the account's most played first (`from_discographies`, read a few artists a look and kept).
//! - **ListenBrainz's fresh releases** for its user: new albums and EPs by artists they listen to (`from_fresh`).
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
use super::musicbrainz::Asked as _;
use super::reach::Sent as _;

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
/// The discographies (0069 amended, 0.9): MusicBrainz is asked for this many artists a look (one request a second),
/// and an artist's is read again after this long.
const DISCOGRAPHIES_PER_LOOK: usize = 25;
const DISCOGRAPHY_FRESH_SECS: i64 = 30 * 86_400;
/// MusicBrainz's "Various Artists": no discography of its own.
const VARIOUS_ARTISTS: &str = "89ad4ac3-39f7-470e-963a-56509c546377";
/// Of the account's own most played artists (Cratarr's plays), how many seed Last.fm's artists like them.
const OWN_SEEDS: usize = 25;
/// The masonry's artists (0.9.2; the owner: more tiles should come from the related artists): of each seed
/// (`app::around`, 0.9.7), this many of its related artists new to the library, an album each; sixteen run a group past
/// the screen's edge.
const RELATED_PER_SEED: usize = 16;
/// ListenBrainz's fresh releases: of the last this many days (90, the owner 2026-10-25: the row "New from artists you
/// have" had 3 with 30).
const FRESH_DAYS: &str = "90";
/// Where the server's own Last.fm key is kept (an import choice), and what gives it until one is saved.
pub const SERVER_LASTFM: &str = "lastfm:server";
const SERVER_LASTFM_ENV: &str = "LASTFM_API_KEY";

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
    /// Its artist new to the library has a picture (and so may be featured, the owner 2026-10-25): read with the look,
    /// as the artist store reads the library's (TheAudioDB, picture and description, by name).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub artist_photo: bool,
    /// How many albums its artist new to the library has released (MusicBrainz's albums of their own, as the
    /// discographies count them), read with the picture (0.9.2, the owner: under each new artist's picture).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist_albums: Option<u32>,
    /// Its artist new to the library's own Bandcamp page, as MusicBrainz keeps it (read with the picture), and whether
    /// the artist plays on Bandcamp at all: that page, or any of their albums found there (the owner, 2026-10-27: New to
    /// you puts those first; the others lower, not left out, and without Listen).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist_bandcamp: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub artist_on_bandcamp: bool,
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
    /// How alike its artist is to each artist it was found from (Last.fm's `match`, 0 to 1), the most alike first: the
    /// masonry places it by these (0.9.2, the owner). Only Last.fm's suggestions have them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alike: Vec<Alike>,
    /// When it came out, as closely as its source says ("YYYY-MM-DD", "YYYY-MM" or "YYYY"): Fresh reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released: Option<String>,
    /// How well it fits each of Discover's reasons, 0 to 1, as its sources found it (`Fits`).
    #[serde(default, skip_serializing_if = "Fits::is_empty")]
    pub fits: Fits,
    /// The account's plays of its artist, where the library has the artist (Gaps' order).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist_plays: Option<u32>,
    /// Its mode, the reason that fits it best, worked out as it is read (`modes`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
    /// The look's own rank as it is kept; as it is read, its mode's fit (`modes`).
    #[serde(default)]
    pub score: f64,
}

/// Discover's modes (the owner, 2026-10-29): each suggestion shows once, in the mode whose reason
/// fits it best ("by how well it fits", not a fixed order). Fresh: anyone's records out in the last three months that
/// fit; Alike: records of any age that fit; Gaps: missing from the library's artists' albums; Artists: artists new to
/// the library, with their records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Fresh,
    Gaps,
    Artists,
    Alike,
}

impl Mode {
    /// The reason a source gives: the discographies a gap; Last.fm's and the related artists an artist like the
    /// account's; ListenBrainz's fresh releases a new record; its recommendations and Weekly Exploration, a record alike.
    fn of_source(source: &str) -> Self {
        match source {
            "discography" => Self::Gaps,
            "lastfm" | "related" => Self::Artists,
            "fresh" => Self::Fresh,
            _ => Self::Alike,
        }
    }
}

/// How well a suggestion fits each reason, 0 to 1 (a source's own score: how much the account plays the artist, how
/// alike the artist is, how sure ListenBrainz is). One scale for all, so the reasons can be weighed against each other.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Fits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fresh: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alike: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gaps: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artists: Option<f64>,
}

impl Fits {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn slot(&mut self, m: Mode) -> &mut Option<f64> {
        match m {
            Mode::Fresh => &mut self.fresh,
            Mode::Alike => &mut self.alike,
            Mode::Gaps => &mut self.gaps,
            Mode::Artists => &mut self.artists,
        }
    }

    /// A reason found again: kept at its best fit.
    fn raise(&mut self, m: Mode, fit: f64) {
        let f = fit.clamp(0.0, 1.0);
        let s = self.slot(m);
        *s = Some(s.map_or(f, |x| x.max(f)));
    }
}

/// An artist new to the library as the Artists mode shows them: who, why, and their records in that mode (their keys,
/// the best fit first).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewArtist {
    pub name: String,
    pub artist_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo: Option<String>,
    pub why: Vec<String>,
    pub score: f64,
    pub records: Vec<String>,
}

/// How alike a suggestion's artist is to one of the account's artists (`seed`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alike {
    pub seed: String,
    pub value: f32,
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
    /// The Artists mode's artists, worked out as it is read (`modes`); never kept.
    #[serde(default, skip_deserializing, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<NewArtist>,
}

/// Whose services: the ListenBrainz user, the Last.fm user and how to ask Last.fm; and the server's own Last.fm key,
/// asked for the artists like the account's own when it has no Last.fm.
#[derive(Debug, Clone, Default)]
pub struct Who {
    pub listenbrainz: Option<String>,
    pub lastfm: Option<(String, Auth)>,
    pub server_lastfm: Option<Auth>,
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
    /// What it is and its genres, where its source says (MusicBrainz's discographies, the fresh releases).
    pub kind: Option<String>,
    pub genres: Vec<String>,
    /// When it came out, where its source says.
    pub released: Option<String>,
    /// How alike its artist is to each artist it was found from, where its source says (Last.fm, the related artists).
    pub alike: Vec<Alike>,
    /// Its album page on Bandcamp, where its source found one (the related artists).
    pub bandcamp: Option<String>,
    /// Which source, for the extra rank of more than one.
    pub source: &'static str,
    pub why: String,
    /// 0..=1 within its source.
    pub score: f64,
}

pub(super) fn http() -> Result<reqwest::Client> {
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
    let res = http.get(format!("{LB_API}/cf/recommendation/user/{user}/recording")).query(&[("count", "100")]).sent().await.context("ListenBrainz did not answer")?;
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
        .sent()
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
        let pl: serde_json::Value = http.get(format!("{LB_API}/playlist/{id}")).sent().await?.error_for_status()?.json().await?;
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
            .sent()
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
                .map(|g| format!("/api/covers/release-group/{g}"))
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
            .sent()
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
pub(super) async fn fm(http: &reqwest::Client, auth: &Auth, method: &str, params: &[(&str, &str)]) -> Result<serde_json::Value> {
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
    let body: serde_json::Value = http.get(FM_API).query(&q).sent().await.context("Last.fm did not answer")?.json().await?;
    if let Some(why) = body["message"].as_str().filter(|_| body.get("error").is_some()) {
        anyhow::bail!("Last.fm: {why}");
    }
    Ok(body)
}

/// A number Last.fm may send as a string.
fn number(v: &serde_json::Value) -> f64 {
    v.as_f64().or_else(|| v.as_str()?.parse().ok()).unwrap_or(0.0)
}

/// The best known album of each artist like the account's most played that the library has none of: its most played
/// in Cratarr (`own`, the most first; 0069 amended, 0.9) and, where it has a Last.fm `user`, there.
async fn from_lastfm(user: Option<&str>, auth: &Auth, own: &[String], artists_had: &HashSet<String>, progress: &Progress) -> Result<Vec<Candidate>> {
    let http = http()?;
    progress("Asking Last.fm", 0, None);
    let mut top: Vec<String> = own.iter().take(OWN_SEEDS).cloned().collect();
    if let Some(user) = user {
        let theirs = fm(&http, auth, "user.gettopartists", &[("user", user), ("period", "6month"), ("limit", TOP_ARTISTS)]).await?;
        for name in theirs["topartists"]["artist"].as_array().into_iter().flatten().filter_map(|a| a["name"].as_str()) {
            if !top.iter().any(|t| loose(t) == loose(name)) {
                top.push(name.to_string());
            }
        }
    }
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
        because.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let alike: Vec<Alike> = because.iter().map(|(m, seed)| Alike { seed: seed.clone(), value: *m as f32 }).collect();
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
            alike,
            score: score / most,
            ..Candidate::default()
        });
    }
    Ok(out)
}

// ---- MusicBrainz's discographies

/// An album of an artist's as MusicBrainz lists their release groups (kept, `UserData::discography`).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Group {
    id: String,
    title: String,
    kind: Option<String>,
    year: Option<i32>,
    #[serde(default)]
    genres: Vec<String>,
    /// Its first release's date, as MusicBrainz writes it (kept from 0.9.17; older ones have none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    date: Option<String>,
}

/// An artist the library has, with how much the account plays them.
#[derive(Debug, Clone)]
pub(super) struct OwnArtist {
    pub(super) name: String,
    pub(super) mbid: Option<String>,
    pub(super) plays: u32,
}

/// Today as MusicBrainz writes dates (UTC).
fn today() -> String {
    let (y, m, d) = super::history::civil(super::moment::now_secs().div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// An artist's albums and EPs on MusicBrainz that are out (first released by `today`; MusicBrainz's dates may be only
/// a year, which sorts before any day of it) and their own (no secondary type: no compilation, live album, remix,
/// soundtrack…), each with its genres (MusicBrainz's own, the most given first, `GENRES` at most).
/// How long to wait before asking MusicBrainz again when it says it is busy (503).
const MB_BUSY_PAUSE: std::time::Duration = std::time::Duration::from_secs(3);

/// How long a related artist's albums are kept before MusicBrainz is asked again (0.9.4, the owner: "Finding artists
/// related to yours" took long, each look asking for every one of them, a second apart).
const RELATED_DISCOGRAPHY_SECS: i64 = 7 * 86_400;

/// A related artist's albums and EPs: as kept where read within a week (no call, no pause), else read from MusicBrainz
/// and kept (in the discographies, by their MusicBrainz id, as the library's artists' are); none where it cannot say.
async fn related_discography(serve: &Arc<crate::serve::Serve>, http: &reqwest::Client, mbid: &str, today: &str) -> Vec<Group> {
    let now = super::moment::now_secs();
    let (ud, key) = (serve.userdata.clone(), mbid.to_string());
    let kept = tokio::task::spawn_blocking(move || ud.discography(&key)).await.ok().and_then(Result::ok).flatten();
    if let Some(groups) = kept.filter(|(at, _)| now - at <= RELATED_DISCOGRAPHY_SECS).and_then(|(_, json)| serde_json::from_str::<Vec<Group>>(&json).ok()) {
        return groups;
    }
    let groups = match mb_discography(http, mbid, today).await {
        Ok(groups) => {
            if let Ok(json) = serde_json::to_string(&groups) {
                let (ud, key) = (serve.userdata.clone(), mbid.to_string());
                if let Ok(Err(e)) = tokio::task::spawn_blocking(move || ud.set_discography(&key, now, &json)).await {
                    tracing::warn!("discover: a related artist's albums not kept: {e:#}");
                }
            }
            groups
        }
        Err(e) => {
            tracing::info!("discover: a related artist's albums ({mbid}) could not be read from MusicBrainz: {e:#}");
            Vec::new()
        }
    };
    tokio::time::sleep(MB_PAUSE).await;
    groups
}

async fn mb_discography(http: &reqwest::Client, artist: &str, today: &str) -> Result<Vec<Group>> {
    #[derive(Deserialize)]
    struct Genre {
        name: String,
        #[serde(default)]
        count: i64,
    }
    #[derive(Deserialize)]
    struct Rg {
        id: String,
        title: String,
        #[serde(rename = "primary-type")]
        primary: Option<String>,
        #[serde(rename = "secondary-types", default)]
        secondary: Vec<String>,
        #[serde(rename = "first-release-date", default)]
        first: String,
        #[serde(default)]
        genres: Vec<Genre>,
    }
    #[derive(Deserialize)]
    struct Browse {
        #[serde(rename = "release-groups", default)]
        groups: Vec<Rg>,
    }
    let mut url = reqwest::Url::parse("https://musicbrainz.org/ws/2/release-group")?;
    url.query_pairs_mut().append_pair("artist", artist).append_pair("type", "album|ep").append_pair("inc", "genres").append_pair("fmt", "json").append_pair("limit", "100");
    // MusicBrainz busy (503, the owner's sync 2026-10-25): once more after a pause, not the artist lost.
    let mut again = true;
    let b: Browse = loop {
        let r = http.get(url.clone()).asked().await?;
        if r.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE && std::mem::take(&mut again) {
            tokio::time::sleep(MB_BUSY_PAUSE).await;
            continue;
        }
        break r.error_for_status()?.json().await?;
    };
    Ok(b.groups
        .into_iter()
        .filter(|g| g.secondary.is_empty() && !g.first.is_empty() && g.first.as_str() <= today)
        .map(|g| {
            let mut genres = g.genres;
            genres.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
            Group {
                year: g.first.get(..4).and_then(|y| y.parse().ok()),
                kind: g.primary,
                genres: genres.into_iter().take(GENRES).map(|x| x.name).collect(),
                id: g.id,
                title: g.title,
                date: Some(g.first),
            }
        })
        .collect())
}

/// Gaps in the library's artists' discographies (0069 amended, 0.9: "first"): each artist's albums and EPs, as
/// MusicBrainz lists them, ranked by how much the account plays the artist (`artists`, the most played first); the
/// library's own are left out later, as every source's are. MusicBrainz is asked for `DISCOGRAPHIES_PER_LOOK` artists a
/// look, those never read first (the most played first), then those read longest ago; the rest are kept from before,
/// so the list fills over a few days.
async fn from_discographies(serve: &Arc<crate::serve::Serve>, http: &reqwest::Client, artists: &[OwnArtist], progress: &Progress) -> Result<Vec<Candidate>> {
    let ids: Vec<String> = artists.iter().filter_map(|a| a.mbid.clone()).collect();
    let ud = serve.userdata.clone();
    let mut kept: HashMap<String, (i64, Vec<Group>)> = tokio::task::spawn_blocking(move || -> Result<_> {
        let mut out = HashMap::new();
        for id in ids {
            if let Some((at, json)) = ud.discography(&id)? {
                out.insert(id, (at, serde_json::from_str(&json).unwrap_or_default()));
            }
        }
        Ok(out)
    })
    .await??;
    let now = super::moment::now_secs();
    let mut due: Vec<&OwnArtist> =
        artists.iter().filter(|a| a.mbid.as_ref().is_some_and(|m| kept.get(m).is_none_or(|k| now - k.0 > DISCOGRAPHY_FRESH_SECS))).collect();
    // Never read before read long ago; the sort keeps the most played first among equals.
    due.sort_by_key(|a| a.mbid.as_ref().and_then(|m| kept.get(m)).map_or(i64::MIN, |k| k.0));
    due.truncate(DISCOGRAPHIES_PER_LOOK);
    let today = today();
    for (i, a) in due.iter().enumerate() {
        progress("Reading your artists' albums", i as u64, Some(due.len() as u64));
        let Some(id) = a.mbid.clone() else { continue };
        match mb_discography(http, &id, &today).await {
            Ok(groups) => {
                let (ud, json, key) = (serve.userdata.clone(), serde_json::to_string(&groups)?, id.clone());
                tokio::task::spawn_blocking(move || ud.set_discography(&key, now, &json)).await??;
                kept.insert(id, (now, groups));
            }
            Err(e) => tracing::info!("discover: {}'s albums could not be read from MusicBrainz: {e:#}", a.name),
        }
        tokio::time::sleep(MB_PAUSE).await;
    }
    let most = artists.iter().map(|a| a.plays).max().unwrap_or(0).max(1) as f64;
    let mut out = Vec::new();
    for a in artists {
        let Some((_, groups)) = a.mbid.as_ref().and_then(|m| kept.get(m)) else { continue };
        let score = played_fit(a.plays, most);
        for g in groups {
            out.push(Candidate {
                album: g.title.clone(),
                artist: a.name.clone(),
                year: g.year,
                release_group: Some(g.id.clone()),
                cover: Some(format!("/api/covers/release-group/{}", g.id)),
                kind: g.kind.clone(),
                genres: g.genres.clone(),
                released: g.date.clone(),
                source: "discography",
                why: format!("Missing from {}'s albums", a.name),
                score,
                ..Candidate::default()
            });
        }
    }
    Ok(out)
}

// ---- The masonry's artists: related artists as suggestions

/// For each seed (its name and MusicBrainz id), its related artists new to the library (app::related, kept a month for
/// every account, looked up now where none are), an album each: the one that plays on Bandcamp where they have one
/// (it plays at once), else their newest album on MusicBrainz with its cover. Each says "Like <seed>" and how related
/// (the most related 1), as Last.fm's suggestions do, so the page groups them with that seed's.
pub(super) async fn from_related(serve: &Arc<crate::serve::Serve>, http: &reqwest::Client, seeds: &[(String, String)], progress: &Progress) -> Vec<Candidate> {
    let today = today();
    let mut out = Vec::new();
    for (i, (seed, mbid)) in seeds.iter().enumerate() {
        progress("Finding artists related to yours", i as u64, Some(seeds.len() as u64));
        let found = match super::related::kept_or_look(serve, seed, mbid).await {
            Ok(f) => f,
            Err(e) => {
                tracing::info!("discover: {seed}'s related artists: {e:#}");
                continue;
            }
        };
        let new: Vec<&super::related::Related> = found.artists.iter().filter(|r| r.id.is_none()).take(RELATED_PER_SEED).collect();
        let most = new.iter().map(|r| r.score).fold(0.0, f64::max).max(f64::EPSILON);
        for r in new {
            let value = (r.score / most).clamp(0.0, 1.0);
            let base = Candidate {
                artist: r.name.clone(),
                source: "related",
                why: format!("Like {seed}"),
                alike: vec![Alike { seed: seed.clone(), value: value as f32 }],
                score: 0.8 * value,
                ..Candidate::default()
            };
            if let (Some(page), Some(title)) = (&r.bandcamp, &r.album) {
                out.push(Candidate {
                    album: title.clone(),
                    cover: Some(format!("/api/cover/{}/{}", crate::bandcamp::SOURCE, crate::bandcamp::track_id(page, 0))),
                    bandcamp: Some(page.clone()),
                    ..base
                });
            } else if let Some(m) = &r.mbid {
                let groups = related_discography(serve, http, m, &today).await;
                let newest = groups.iter().filter(|g| g.kind.as_deref() == Some("Album")).max_by_key(|g| g.year).or(groups.first());
                if let Some(g) = newest {
                    out.push(Candidate {
                        album: g.title.clone(),
                        year: g.year,
                        release_group: Some(g.id.clone()),
                        cover: Some(format!("/api/covers/release-group/{}", g.id)),
                        kind: g.kind.clone(),
                        genres: g.genres.clone(),
                        released: g.date.clone(),
                        ..base
                    });
                }
            }
        }
    }
    out
}

// ---- ListenBrainz's fresh releases

/// ListenBrainz's fresh releases for `user` (0069 amended, 0.9): new albums and EPs (no singles, no compilations or
/// remixes) by artists they listen to, of the last `FRESH_DAYS` days.
async fn from_fresh(http: &reqwest::Client, user: &str) -> Result<Vec<Candidate>> {
    let body: serde_json::Value = http
        .get(format!("{LB_API}/user/{user}/fresh_releases"))
        .query(&[("days", FRESH_DAYS), ("past", "true"), ("future", "false")])
        .sent()
        .await
        .context("ListenBrainz did not answer")?
        .error_for_status()?
        .json()
        .await
        .context("ListenBrainz's fresh releases could not be read")?;
    Ok(body["payload"]["releases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| matches!(r["release_group_primary_type"].as_str(), Some("Album" | "EP")) && r["release_group_secondary_type"].as_str().is_none_or(str::is_empty))
        .filter_map(|r| {
            let rg = r["release_group_mbid"].as_str().filter(|g| !g.is_empty())?.to_string();
            let artist = r["artist_credit_name"].as_str()?.to_string();
            Some(Candidate {
                album: r["release_name"].as_str()?.to_string(),
                year: r["release_date"].as_str().and_then(|d| d.get(..4)?.parse().ok()),
                released: r["release_date"].as_str().filter(|d| !d.is_empty()).map(str::to_string),
                cover: Some(format!("/api/covers/release-group/{rg}")),
                kind: r["release_group_primary_type"].as_str().map(str::to_string),
                source: "fresh",
                why: format!("New from {artist}"),
                score: 0.8,
                release_group: Some(rg),
                artist,
                ..Candidate::default()
            })
        })
        .collect())
}

// ---- The library, the answers, and one list

/// What the library has: the release groups of every folder's albums (Music and Incoming), and its albums by name
/// (the import's matcher), and its artists (loosely, as Last.fm's are compared).
pub struct Have {
    pub release_groups: HashSet<String>,
    /// The library's albums alone, shared (`Matcher::library_albums`; 0.11.1: /api/discover's memory, 2026-10-30:
    /// each look, Around, an artist's missing albums and a search read every song of the library here).
    pub matcher: Arc<Matcher>,
    /// The artists the library has albums by: Last.fm's new artists are none of these.
    pub artists: HashSet<String>,
    /// Those and every artist a track credits (compilations, guests): "New to you" is for none of these (the owner,
    /// 2026-10-24: one on a compilation is not new).
    pub known: HashSet<String>,
}

impl Have {
    pub(super) fn has(&self, c: &Candidate) -> bool {
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
        // The server's own library: kept while it is not built again; any other read now.
        let built = crate::serve::instance().filter(|s| s.data_dir.as_deref() == Some(data_dir)).map(|s| s.built_at());
        Ok(Have { release_groups, matcher: Matcher::library_albums(&data_dir.join("library.sqlite"), built)?, known: artists.clone(), artists })
    }
}

/// An album's key: its release group, else its artist's and its name's.
fn key_of(c: &Candidate) -> String {
    c.release_group.clone().unwrap_or_else(|| format!("{}|{}", key(&c.artist), key(&c.album)))
}

/// What Discover leaves out for an account: the albums it answered and the artists it said no to (Discover's own,
/// `UserData::discover_answered`), and the artists it excluded everywhere (decision 0083), their keys.
pub fn leaves_out(ud: &crate::userdata::UserData, account: &str) -> Result<(HashSet<String>, HashSet<String>, HashSet<String>)> {
    let (albums, no_artists) = ud.discover_answered(account)?;
    Ok((albums, no_artists, ud.excluded_artists(account)?))
}

/// Whether a suggestion credits an artist excluded everywhere (decision 0083): its artist, by any credit ("A feat. B",
/// "A & B").
pub fn excluded_everywhere(s: &Suggestion, out: &HashSet<String>) -> bool {
    super::excluded_artists::credits_excluded(&[s.artist.as_str()], out)
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
                    bandcamp: c.bandcamp.clone(),
                    artist_photo: false,
                    artist_albums: None,
                    artist_bandcamp: None,
                    artist_on_bandcamp: false,
                    kind: c.kind.clone(),
                    genres: c.genres.clone(),
                    length_secs: None,
                    track_count: None,
                    listeners: None,
                    alike: Vec::new(),
                    released: c.released.clone(),
                    fits: Fits::default(),
                    artist_plays: None,
                    mode: None,
                    score: 0.0,
                },
                HashSet::new(),
            )
        });
        let (s, sources) = e;
        if !s.why.contains(&c.why) {
            s.why.push(c.why.clone());
        }
        let named = match c.source {
            "lastfm" => "Last.fm",
            "discography" => "MusicBrainz",
            _ => "ListenBrainz",
        };
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
        if s.kind.is_none() {
            s.kind = c.kind.clone();
        }
        if s.genres.is_empty() {
            s.genres = c.genres.clone();
        }
        if s.bandcamp.is_none() {
            s.bandcamp = c.bandcamp.clone();
        }
        if s.released.is_none() {
            s.released = c.released.clone();
        }
        s.fits.raise(Mode::of_source(c.source), c.score);
        // Each seed once, at its strongest; the most alike first.
        for a in &c.alike {
            match s.alike.iter_mut().find(|x| x.seed == a.seed) {
                Some(x) => x.value = x.value.max(a.value),
                None => s.alike.push(a.clone()),
            }
        }
        s.alike.sort_by(|a, b| b.value.total_cmp(&a.value).then_with(|| a.seed.cmp(&b.seed)));
        // A second source counts extra; more of one source's recordings on the album, a little.
        s.score += if sources.insert(c.source) { c.score + if sources.len() > 1 { 1.0 } else { 0.0 } } else { c.score * 0.25 };
    }
    let mut out: Vec<Suggestion> = order.into_iter().filter_map(|k| by.remove(&k).map(|e| e.0)).collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.artist.cmp(&b.artist)));
    out.truncate(MOST);
    out
}

/// How well an artist's record fits by how much the account plays them: from 0.4 to 0.9 by how much (the most played
/// `most`); never played, below every played artist's.
fn played_fit(plays: u32, most: f64) -> f64 {
    if plays > 0 { 0.4 + 0.5 * f64::from(plays) / most.max(1.0) } else { 0.2 }
}

/// Each suggestion by an artist the library has: the account's plays of them (Gaps' order), and a new record of theirs
/// fits Fresh as much as they are played, as a gap in their albums would (`plays` by the artist's key).
fn weigh(albums: &mut [Suggestion], plays: &HashMap<String, u32>) {
    let most = f64::from(plays.values().copied().max().unwrap_or(0));
    for s in albums {
        let Some(&p) = plays.get(&s.artist_key) else { continue };
        s.artist_plays = Some(p);
        if s.fits.fresh.is_some() {
            s.fits.fresh = Some(played_fit(p, most));
        }
    }
}

/// How many days back a record counts as fresh (the owner, 2026-10-29: "the last 3 months").
const FRESH_MODE_DAYS: i64 = 90;
/// How many of an artist's records the Artists mode lists.
const ARTIST_RECORDS: usize = 6;

/// Whether a record came out on or after `since` ("YYYY-MM-DD"): a date of only a year never does, a month's when it is
/// the month after `since`'s or later.
fn out_since(released: Option<&str>, since: &str) -> bool {
    released.is_some_and(|r| r.len() >= 7 && r >= since)
}

/// What a suggestion kept before its reasons' fits were (0.9.16 and older) is taken to fit: by its sources and reasons.
fn fits_of_old(s: &Suggestion) -> Fits {
    let mut f = Fits::default();
    for w in &s.why {
        let m = if w.starts_with("New from ") {
            Mode::Fresh
        } else if w.starts_with("Missing from ") {
            Mode::Gaps
        } else if w.starts_with("Like ") {
            Mode::Artists
        } else {
            Mode::Alike
        };
        f.raise(m, 0.5);
    }
    if f.is_empty() {
        f.raise(if s.new_artist { Mode::Artists } else { Mode::Alike }, 0.5);
    }
    f
}

/// Each suggestion's mode and score (its mode's fit), and the Artists mode's artists, as of `since` (the first day
/// still fresh): every reason it has is weighed by its fit, and one out since then also fits Fresh as well as it fits
/// at all (anyone's new records that fit, the owner). The best fit wins; even, the narrower reason: Fresh, then Gaps,
/// then Artists, then Alike.
pub fn modes(found: &mut Found, since: &str) {
    for s in &mut found.albums {
        let mut f = if s.fits.is_empty() { fits_of_old(s) } else { s.fits };
        if out_since(s.released.as_deref(), since) {
            let best = [f.fresh, f.alike, f.gaps, f.artists].into_iter().flatten().fold(0.0, f64::max);
            f.raise(Mode::Fresh, best);
        } else {
            f.fresh = None;
        }
        let order = [Mode::Fresh, Mode::Gaps, Mode::Artists, Mode::Alike];
        let (mode, fit) = order.into_iter().filter_map(|m| (*f.slot(m)).map(|x| (m, x))).fold(None, |best: Option<(Mode, f64)>, (m, x)| match best {
            Some((_, b)) if b >= x => best,
            _ => Some((m, x)),
        }).unwrap_or((Mode::Alike, 0.0));
        s.mode = Some(mode);
        s.score = fit;
    }
    let mut by: HashMap<String, NewArtist> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for s in found.albums.iter().filter(|s| s.mode == Some(Mode::Artists)) {
        let a = by.entry(s.artist_key.clone()).or_insert_with(|| {
            order.push(s.artist_key.clone());
            NewArtist { name: s.artist.clone(), artist_key: s.artist_key.clone(), photo: None, why: Vec::new(), score: 0.0, records: Vec::new() }
        });
        if a.photo.is_none() && s.artist_photo {
            let mut u = reqwest::Url::parse("http://cratarr/api/discover/artist/photo").expect("a fixed URL");
            u.query_pairs_mut().append_pair("name", &s.artist);
            a.photo = Some(format!("{}?{}", u.path(), u.query().unwrap_or_default()));
        }
        for w in &s.why {
            if !a.why.contains(w) {
                a.why.push(w.clone());
            }
        }
        a.score = a.score.max(s.score);
        a.records.push(s.key.clone());
    }
    let score: HashMap<&str, f64> = found.albums.iter().map(|s| (s.key.as_str(), s.score)).collect();
    let mut artists: Vec<NewArtist> = order.into_iter().filter_map(|k| by.remove(&k)).collect();
    for a in &mut artists {
        a.records.sort_by(|x, y| score.get(y.as_str()).copied().unwrap_or(0.0).total_cmp(&score.get(x.as_str()).copied().unwrap_or(0.0)));
        a.records.truncate(ARTIST_RECORDS);
    }
    artists.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    found.artists = artists;
}

/// The first day still fresh today (`FRESH_MODE_DAYS` back, UTC).
fn fresh_since() -> String {
    let (y, m, d) = super::history::civil(super::moment::now_secs().div_euclid(86_400) - FRESH_MODE_DAYS);
    format!("{y:04}-{m:02}-{d:02}")
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
    let b: Browse = http.get(url).asked().await?.error_for_status()?.json().await?;
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
pub(super) async fn mb_artist_urls(http: &reqwest::Client, artist: &str) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Artist {
        #[serde(default)]
        relations: Vec<MbRelation>,
    }
    let url = format!("https://musicbrainz.org/ws/2/artist/{artist}?inc=url-rels&fmt=json");
    let a: Artist = http.get(url).asked().await?.error_for_status()?.json().await?;
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

/// An artist's own page on Bandcamp among the addresses MusicBrainz keeps for them (not Bandcamp's own site).
fn own_bandcamp(urls: &[String]) -> Option<String> {
    urls.iter()
        .filter_map(|u| crate::bandcamp::page(u))
        .find(|p| reqwest::Url::parse(p).ok().and_then(|u| u.host_str().map(|h| h != "bandcamp.com")).unwrap_or(false))
}

/// An artist's own Bandcamp page and their releases there, newest first (New to you's Listen, the owner 2026-10-27):
/// the page given (Discover's `artist_bandcamp`), else MusicBrainz's for them, else Bandcamp's search box where the name
/// is the same; none found, none.
pub async fn bandcamp_artist(name: &str, mbid: Option<&str>, given: Option<&str>) -> Result<(Option<String>, Vec<(String, String)>)> {
    let http = http()?;
    let bc = crate::bandcamp::Bandcamp::get();
    let mut page = given.and_then(|g| own_bandcamp(&[g.to_string()]));
    if page.is_none()
        && let Some(id) = mbid.filter(|m| !m.is_empty())
    {
        page = own_bandcamp(&mb_artist_urls(&http, id).await.unwrap_or_default());
        tokio::time::sleep(MB_PAUSE).await;
    }
    if page.is_none() {
        page = bc.search_artist(name).await.unwrap_or_default().into_iter().find(|(n, _)| same(n, name)).map(|(_, p)| p);
    }
    let Some(page) = page else { return Ok((None, Vec::new())) };
    let releases = bc.releases(&page).await.unwrap_or_default();
    Ok((Some(page), releases))
}

/// The artist MusicBrainz is sure a name is (its search's score 95 or more, the name the same as the matcher reads
/// names): their id.
async fn mb_artist_by_name(http: &reqwest::Client, name: &str) -> Result<Option<String>> {
    #[derive(Deserialize)]
    struct Artist {
        id: String,
        #[serde(default)]
        score: u32,
        #[serde(default)]
        name: String,
    }
    #[derive(Deserialize)]
    struct Search {
        #[serde(default)]
        artists: Vec<Artist>,
    }
    let mut url = reqwest::Url::parse("https://musicbrainz.org/ws/2/artist")?;
    url.query_pairs_mut().append_pair("query", &format!("artist:{}", quoted(name))).append_pair("fmt", "json").append_pair("limit", "3");
    let s: Search = http.get(url).asked().await?.error_for_status()?.json().await?;
    Ok(s.artists.into_iter().find(|a| a.score >= 95 && loose(&a.name) == loose(name)).map(|a| a.id))
}

/// A release group's first credited artist on MusicBrainz (their id).
async fn artist_of_group(http: &reqwest::Client, release_group: &str) -> Result<Option<String>> {
    let v: serde_json::Value = http
        .get(format!("https://musicbrainz.org/ws/2/release-group/{release_group}?inc=artist-credits&fmt=json"))
        .asked()
        .await?
        .error_for_status()?
        .json()
        .await?;
    tokio::time::sleep(MB_PAUSE).await;
    Ok(v["artist-credit"].as_array().and_then(|c| c.first()).and_then(|c| c["artist"]["id"].as_str()).map(str::to_string))
}

/// How many full-length studio albums an artist (their MusicBrainz id) has released (0.9.3; the owner: "how many
/// full-length studio albums. That's the golden standard."): their release groups of the primary type Album with no
/// secondary type (no compilation, live album, remix, soundtrack…), out by today, as the discographies count them.
async fn studio_albums(http: &reqwest::Client, artist: &str) -> Result<u32> {
    Ok(mb_discography(http, artist, &today()).await?.iter().filter(|g| g.kind.as_deref() == Some("Album")).count() as u32)
}

/// How many studio albums an artist new to the library has released, by name (a Discover artist's page, 0.9.3):
/// MusicBrainz's sure match, counted, and kept a day. None where MusicBrainz is not sure who they are.
pub async fn artist_albums(name: &str) -> Option<u32> {
    use std::sync::{Mutex, OnceLock};
    static KEPT: OnceLock<Mutex<HashMap<String, (i64, Option<u32>)>>> = OnceLock::new();
    let kept = KEPT.get_or_init(Default::default);
    let key = loose(name);
    let now = super::moment::now_secs();
    if let Some((at, n)) = kept.lock().unwrap_or_else(|p| p.into_inner()).get(&key).copied()
        && now - at < 86_400
    {
        return n;
    }
    let http = http().ok()?;
    let n = match mb_artist_by_name(&http, name).await.ok().flatten() {
        Some(mbid) => {
            tokio::time::sleep(MB_PAUSE).await;
            studio_albums(&http, &mbid).await.ok()
        }
        None => None,
    };
    kept.lock().unwrap_or_else(|p| p.into_inner()).insert(key, (now, n));
    n
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
    let s: Search = http.get(url).asked().await?.error_for_status()?.json().await?;
    Ok(s.groups.into_iter().find(|g| g.score >= 95 && loose(&g.title) == loose(album)).map(|g| (g.id, g.first.get(..4).and_then(|y| y.parse().ok()))))
}

/// MusicBrainz asks for at most one request a second.
pub(super) const MB_PAUSE: Duration = Duration::from_millis(1100);

/// Looks up the account's suggestions now (blocking parts apart) and keeps them.
pub async fn look(
    serve: &Arc<crate::serve::Serve>,
    cat: Arc<crate::catalogue::Catalogue>,
    account: &str,
    who: &Who,
    store: Option<Arc<super::artists::ArtistInfo>>,
    progress: &Progress,
) -> Result<Found> {
    // Its play history off (0.9.5, the owner): what Discover can find without its listening, nothing personalised: no
    // ListenBrainz or Last.fm picks from the account's listening (the server's Last.fm key still asks for nothing).
    let (ud, a) = (serve.userdata.clone(), account.to_string());
    let general;
    let who = if tokio::task::spawn_blocking(move || ud.tracking(&a)).await? {
        who
    } else {
        general = Who { listenbrainz: None, lastfm: None, server_lastfm: who.server_lastfm.clone() };
        &general
    };
    progress("Reading your library", 0, None);
    let (have, artists) = own(serve, cat, account).await?;
    let http = http()?;
    let mut candidates = Vec::new();
    let mut notes = Vec::new();
    // The library's artists' discographies first: they need no service (0069 amended, 0.9).
    match from_discographies(serve, &http, &artists, progress).await {
        Ok(c) => candidates.extend(c),
        Err(e) => notes.push(format!("MusicBrainz could not be asked for your artists' albums: {e:#}")),
    }
    if let Some(user) = &who.listenbrainz {
        match from_listenbrainz(user, progress).await {
            Ok((c, n)) => {
                candidates.extend(c);
                notes.extend(n);
            }
            Err(e) => notes.push(format!("ListenBrainz could not be asked: {e:#}")),
        }
        match from_fresh(&http, user).await {
            Ok(c) => candidates.extend(c),
            Err(e) => notes.push(format!("ListenBrainz's new releases could not be read: {e:#}")),
        }
    }
    // Last.fm with the account's own, else the server's key: the artists like the account's most played.
    let fm_auth = who.lastfm.as_ref().map(|l| l.1.clone()).or_else(|| who.server_lastfm.clone());
    if let Some(auth) = &fm_auth {
        let own: Vec<String> = artists.iter().filter(|a| a.plays > 0).map(|a| a.name.clone()).collect();
        if who.lastfm.is_some() || !own.is_empty() {
            match from_lastfm(who.lastfm.as_ref().map(|l| l.0.as_str()), auth, &own, &have.artists, progress).await {
                Ok(c) => candidates.extend(c),
                Err(e) => notes.push(format!("Last.fm could not be asked: {e:#}")),
            }
        }
    }
    // The masonry's artists are Around's own (0.9.7, `app::around`): the look waits on none of their seeds.
    // Albums named without their MusicBrainz id (Last.fm's mostly): looked up there.
    let unknown = candidates.iter().filter(|c| c.release_group.is_none()).count() as u64;
    let mut n = 0;
    for c in candidates.iter_mut().filter(|c| c.release_group.is_none()) {
        progress("Finding albums on MusicBrainz", n, Some(unknown));
        n += 1;
        if let Ok(Some((g, year))) = mb_release_group(&http, &c.artist, &c.album).await {
            if c.cover.is_none() {
                c.cover = Some(format!("/api/covers/release-group/{g}"));
            }
            c.year = c.year.or(year);
            c.release_group = Some(g);
        }
        tokio::time::sleep(MB_PAUSE).await;
    }
    progress("Leaving out what you have", 0, None);
    let (ud, a) = (serve.userdata.clone(), account.to_string());
    let (answered, no_artists, everywhere) = tokio::task::spawn_blocking(move || leaves_out(&ud, &a)).await??;
    let mut albums = one_list(candidates, &|c: &Candidate| have.has(c), &have.known, &answered, &no_artists);
    albums.retain(|s| !excluded_everywhere(s, &everywhere));
    weigh(&mut albums, &artists.iter().map(|a| (loose(&a.name), a.plays)).collect());
    // What each is and its genres, from ListenBrainz in a few calls, so the cards show them from the first.
    progress("Reading what each album is", 0, None);
    let groups: Vec<String> = albums.iter().filter(|s| s.kind.is_none() || s.genres.is_empty()).filter_map(|s| s.release_group.clone()).collect();
    match lb_group_facts(&http, &groups).await {
        Ok(facts) => {
            for s in albums.iter_mut() {
                if let Some((kind, genres)) = s.release_group.as_ref().and_then(|g| facts.get(g)) {
                    if s.kind.is_none() {
                        s.kind = kind.clone();
                    }
                    if s.genres.is_empty() {
                        s.genres = genres.clone();
                    }
                }
            }
        }
        Err(e) => tracing::info!("discover: ListenBrainz's facts of the albums could not be read: {e:#}"),
    }
    // Discogs' genres and styles where ListenBrainz gave fewer than a card shows (0.9.8), kept a while per release group.
    if let Some(served) = crate::serve::instance() {
        for s in albums.iter_mut().filter(|s| s.genres.len() < GENRES) {
            let Some(group) = s.release_group.clone() else { continue };
            match super::discogs_facts::release_group(&served, &group).await {
                Ok(Some(f)) => {
                    for g in f.genres.into_iter().chain(f.styles) {
                        if s.genres.len() < GENRES && !s.genres.iter().any(|x| x.eq_ignore_ascii_case(&g)) {
                            s.genres.push(g);
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => tracing::debug!("discover: Discogs on {group}: {e:#}"),
            }
        }
    }
    // Where the look before found each on Bandcamp, kept from the start and not looked up again (the owner, 2026-10-25:
    // the pick of the day, which plays, went while a sync ran, until the pages were found again; and Bandcamp is asked
    // less).
    let (sv, acc) = (serve.clone(), account.to_string());
    let before: HashMap<String, String> = tokio::task::spawn_blocking(move || kept(&sv, &acc))
        .await
        .ok()
        .and_then(|r| r.ok())
        .flatten()
        .map(|f| f.albums.into_iter().filter_map(|s| Some((s.key, s.bandcamp?))).collect())
        .unwrap_or_default();
    for s in albums.iter_mut().filter(|s| s.bandcamp.is_none()) {
        s.bandcamp = before.get(&s.key).cloned();
    }
    // Shown the moment they are known (the owner, 2026-10-24, 0.9): kept now, so the page lists them while the rest is
    // looked up behind them, and kept again as it fills in (the page reads them again as this step moves on).
    let at = super::moment::now_secs();
    keep_found(serve, account, &Found { at, albums: albums.clone(), notes: notes.clone(), ..Found::default() }).await?;
    // Where to hear each: its Bandcamp and YouTube pages as MusicBrainz knows them, else a search there; and its album
    // page on Bandcamp, found further where MusicBrainz has none (decision 0071), to play it from.
    let total = albums.len() as u64;
    for i in 0..albums.len() {
        if i > 0 && i % KEEP_EVERY == 0 {
            keep_found(serve, account, &Found { at, albums: albums.clone(), notes: notes.clone(), ..Found::default() }).await?;
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
        if s.bandcamp.is_none() {
            s.bandcamp = bandcamp_page(&http, &mb.urls, &mb.artists, &s.artist, &s.album).await;
        }
        if let Some(p) = &s.bandcamp {
            mb.urls.insert(0, p.clone());
        }
        s.links.extend(listen_links(&mb.urls, &s.artist, &s.album));
        // How many listen to it, where the account asks Last.fm (its tracks too, where MusicBrainz had none).
        if let Some(auth) = &fm_auth
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
    // The new artists' pictures and descriptions, with the sync (the owner, 2026-10-25: "It should be in the sync",
    // "as well as description if we can get it"): the first few new artists, as the artist store reads the library's,
    // kept there; a suggestion says whether its artist has a picture, the page only features one that has.
    if let Some(store) = store {
        let mut new: Vec<String> = Vec::new();
        for s in albums.iter().filter(|s| s.new_artist) {
            if !new.contains(&s.artist) && new.len() < NEW_ARTIST_PICTURES {
                new.push(s.artist.clone());
            }
        }
        let total = new.len() as u64;
        for (i, name) in new.iter().enumerate() {
            progress("Finding new artists' pictures", i as u64, Some(total));
            // TheAudioDB's, not MusicBrainz's; asked at the same gentle pace all the same.
            if i > 0 {
                tokio::time::sleep(MB_PAUSE).await;
            }
            let photo = store.get_or_read(name, None).await.is_some_and(|info| info.photo.is_some());
            // How many albums they have released: their MusicBrainz id by name (a sure match only), their albums; and
            // their own Bandcamp page, as MusicBrainz keeps it.
            let (mut released, mut own) = (None, None);
            if let Ok(Some(mbid)) = mb_artist_by_name(&http, name).await {
                tokio::time::sleep(MB_PAUSE).await;
                released = studio_albums(&http, &mbid).await.ok();
                tokio::time::sleep(MB_PAUSE).await;
                own = own_bandcamp(&mb_artist_urls(&http, &mbid).await.unwrap_or_default());
            }
            tokio::time::sleep(MB_PAUSE).await;
            for s in albums.iter_mut().filter(|s| &s.artist == name) {
                (s.artist_photo, s.artist_albums, s.artist_bandcamp) = (photo, released, own.clone());
            }
        }
    }
    // Whether each new artist plays on Bandcamp at all: their own page, or any of their albums found there.
    let playing: HashSet<String> = albums.iter().filter(|s| s.new_artist && (s.bandcamp.is_some() || s.artist_bandcamp.is_some())).map(|s| s.artist_key.clone()).collect();
    for s in albums.iter_mut().filter(|s| s.new_artist) {
        s.artist_on_bandcamp = playing.contains(&s.artist_key);
    }
    let found = Found { at, albums, notes, ..Found::default() };
    keep_found(serve, account, &found).await?;
    // Its covers fetched ahead and kept (0.9.5), so the page shows them at once.
    super::cover_cache::warm(found.albums.iter().filter_map(|s| s.cover.clone()).collect());
    Ok(found)
}

/// Each artist's plays as Discover weighs them: the account's own, none heard while following another (the owner,
/// 2026-10-30).
fn artist_plays(ud: &crate::userdata::UserData, account: &str) -> Result<HashMap<String, u32>> {
    Ok(ud.used(account, crate::userdata::Kind::Artist)?.into_iter().map(|(id, x)| (id, x.own_count)).collect())
}

/// The library's artists, the account's most played first (its own plays, 0064, `artist_plays`), and what the library
/// has.
pub(super) async fn own(serve: &Arc<crate::serve::Serve>, cat: Arc<crate::catalogue::Catalogue>, account: &str) -> Result<(Have, Vec<OwnArtist>)> {
    let dir = serve.data_dir.clone().context("Discover needs Cratarr's data folder")?;
    let (lib, ud, a) = (serve.library.clone(), serve.userdata.clone(), account.to_string());
    tokio::task::spawn_blocking(move || -> Result<(Have, Vec<OwnArtist>)> {
        let rows = lib.artist_albums()?;
        let plays = artist_plays(&ud, &a)?;
        let mut by_id: HashMap<&str, OwnArtist> = HashMap::new();
        for (id, name, mbid, _, _, _) in &rows {
            by_id.entry(id.as_str()).or_insert_with(|| OwnArtist {
                name: name.clone(),
                mbid: mbid.clone().filter(|m| !m.is_empty() && m != VARIOUS_ARTISTS),
                plays: plays.get(id).copied().unwrap_or(0),
            });
        }
        let mut artists: Vec<OwnArtist> = by_id.into_values().collect();
        artists.sort_by(|x, y| y.plays.cmp(&x.plays).then_with(|| x.name.cmp(&y.name)));
        let mut have = have(&dir, &cat, rows.iter().map(|r| r.1.clone()))?;
        have.known = have.artists.iter().cloned().chain(lib.track_artist_names()?.iter().map(|a| loose(a))).collect();
        Ok((have, artists))
    })
    .await?
}

/// How many new artists' pictures a look reads (the page features one; the rest are its chips).
const NEW_ARTIST_PICTURES: usize = 12;

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

/// The account's kept suggestion of an album on Bandcamp (by its page), where there is one: what Discover knows of it,
/// for the album's panel (0.9.2).
pub fn suggestion_on_bandcamp(serve: &crate::serve::Serve, account: &str, page: &str) -> Result<Option<Suggestion>> {
    let bare = |u: &str| u.to_ascii_lowercase().trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_string();
    let page = bare(page);
    Ok(kept(serve, account)?.and_then(|f| f.albums.into_iter().find(|s| s.bandcamp.as_deref().is_some_and(|b| bare(b) == page))))
}

/// An album of an artist's the library has none of (0.9.3, the owner: an artist's page shows their missing albums).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Missing {
    pub release_group: String,
    pub title: String,
    pub year: Option<i32>,
    pub kind: Option<String>,
    pub cover: String,
    /// Its page on Bandcamp where an open of it found one lately (0.9.5: its cover's play button, as Discover's).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bandcamp: Option<String>,
    /// An open request's mark (0.9.5): wanted, or coming and how far; none without one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<super::requests::Mark>,
}

/// An artist's albums and EPs the library has none of, newest first: their discography as Discover keeps it (read now
/// and kept where it is not, or old), by the library's MusicBrainz id for them, else MusicBrainz's sure match by name;
/// what the library has left out as every source's is (by release group, else by name and artist).
/// `mbid`, `release_group` (0.9.6): what tells the artist where the name may not (an artist not in the library, from
/// a Bandcamp album or a suggestion): their MusicBrainz id, else a release group of theirs; else the library's id for
/// the name, else MusicBrainz's sure match for it.
pub async fn missing_albums(
    serve: &Arc<crate::serve::Serve>,
    cat: Arc<crate::catalogue::Catalogue>,
    name: &str,
    mbid: Option<&str>,
    release_group: Option<&str>,
) -> Result<Vec<Missing>> {
    let http = http()?;
    let hinted = match (mbid.filter(|m| super::discogs::is_mbid(m)), release_group.filter(|g| super::discogs::is_mbid(g))) {
        (Some(m), _) => Some(m.to_string()),
        (None, Some(g)) => artist_of_group(&http, g).await.unwrap_or_default(),
        (None, None) => None,
    };
    let (lib, n) = (serve.library.clone(), name.to_string());
    let known = match hinted {
        Some(m) => Some(m),
        None => tokio::task::spawn_blocking(move || lib.artist_by_name(&n)).await??.and_then(|(_, m)| m).filter(|m| !m.is_empty()),
    };
    let Some(mbid) = (match known {
        Some(m) => Some(m),
        None => mb_artist_by_name(&http, name).await?,
    }) else {
        return Ok(Vec::new());
    };
    let (ud, key) = (serve.userdata.clone(), mbid.clone());
    let kept = tokio::task::spawn_blocking(move || ud.discography(&key)).await??;
    let now = super::moment::now_secs();
    let groups: Vec<Group> = match kept.filter(|(at, _)| now - at <= DISCOGRAPHY_FRESH_SECS).and_then(|(_, json)| serde_json::from_str(&json).ok()) {
        Some(g) => g,
        None => {
            let g = mb_discography(&http, &mbid, &today()).await?;
            let (ud, key, json) = (serve.userdata.clone(), mbid.clone(), serde_json::to_string(&g)?);
            tokio::task::spawn_blocking(move || ud.set_discography(&key, now, &json)).await??;
            g
        }
    };
    let dir = serve.data_dir.clone().context("Cratarr's data folder is needed")?;
    let (lib, ud, artist) = (serve.library.clone(), serve.userdata.clone(), name.to_string());
    let mut out = tokio::task::spawn_blocking(move || -> Result<Vec<Missing>> {
        let names = lib.artist_albums()?.into_iter().map(|r| r.1).collect::<Vec<_>>();
        let have = have(Path::new(&dir), &cat, names)?;
        let pages = ud.bandcamp_pages(now - BANDCAMP_FOUND_SECS)?;
        Ok(groups
            .into_iter()
            .filter(|g| !have.has(&Candidate { album: g.title.clone(), artist: artist.clone(), release_group: Some(g.id.clone()), ..Candidate::default() }))
            .map(|g| Missing {
                cover: format!("/api/covers/release-group/{}", g.id),
                bandcamp: pages.get(&g.id).cloned(),
                request: None,
                release_group: g.id,
                title: g.title,
                year: g.year,
                kind: g.kind,
            })
            .collect())
    })
    .await??;
    out.sort_by(|a, b| b.year.cmp(&a.year).then_with(|| a.title.cmp(&b.title)));
    Ok(out)
}

/// How long an album's page found on Bandcamp (or not found) is kept before it is looked for again.
const BANDCAMP_FOUND_SECS: i64 = 30 * 86_400;

/// An album's page on Bandcamp, looked up when it is opened (0.9.3: an artist's missing album; never for a whole list):
/// as a look finds them (`bandcamp_page`), from its release group's MusicBrainz pages where it is known. Kept a month
/// for every account, a miss too (the owner: "Yes, please"), so a second open is at once and Bandcamp asked less.
pub async fn find_on_bandcamp(serve: &Arc<crate::serve::Serve>, artist: &str, album: &str, release_group: Option<&str>) -> Result<Option<String>> {
    let album_key = release_group.filter(|g| !g.is_empty()).map_or_else(|| format!("{}|{}", key(artist), key(album)), str::to_string);
    let now = super::moment::now_secs();
    let (ud, k) = (serve.userdata.clone(), album_key.clone());
    if let Some((at, page)) = tokio::task::spawn_blocking(move || ud.bandcamp_found(&k)).await??
        && now - at <= BANDCAMP_FOUND_SECS
    {
        return Ok(page);
    }
    let page = look_on_bandcamp(artist, album, release_group).await?;
    let (ud, p) = (serve.userdata.clone(), page.clone());
    tokio::task::spawn_blocking(move || ud.set_bandcamp_found(&album_key, now, p.as_deref())).await??;
    Ok(page)
}

/// The look itself, `find_on_bandcamp`'s.
async fn look_on_bandcamp(artist: &str, album: &str, release_group: Option<&str>) -> Result<Option<String>> {
    let http = http()?;
    let (urls, artists) = match release_group.filter(|g| !g.is_empty()) {
        Some(g) => {
            let got = mb_urls(&http, g).await.unwrap_or_default();
            tokio::time::sleep(MB_PAUSE).await;
            (got.urls, got.artists)
        }
        None => (Vec::new(), Vec::new()),
    };
    Ok(bandcamp_page(&http, &urls, &artists, artist, album).await)
}

/// A missing album of one of the library's artists (Insights › Discovery › Missing albums, 0.9.3).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MissingOf {
    pub release_group: String,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub kind: Option<String>,
    pub cover: String,
    /// Its page on Bandcamp where an open of it found one lately (0.9.5: its cover's play button, as Discover's).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bandcamp: Option<String>,
    /// An open request's mark (0.9.5): wanted, or coming and how far; none without one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<super::requests::Mark>,
}

/// Every missing album of the library's artists whose discography Discover keeps (shared by every account), newest
/// first, as an artist's page counts them; and how many artists that is.
pub async fn all_missing(serve: &Arc<crate::serve::Serve>, cat: Arc<crate::catalogue::Catalogue>) -> Result<(Vec<MissingOf>, usize)> {
    let dir = serve.data_dir.clone().context("Cratarr's data folder is needed")?;
    let (lib, ud) = (serve.library.clone(), serve.userdata.clone());
    tokio::task::spawn_blocking(move || -> Result<(Vec<MissingOf>, usize)> {
        let rows = lib.artist_albums()?;
        let names: HashMap<String, String> = rows.iter().filter_map(|(_, name, mbid, _, _, _)| Some((mbid.clone().filter(|m| !m.is_empty())?, name.clone()))).collect();
        let have = have(Path::new(&dir), &cat, rows.into_iter().map(|r| r.1))?;
        let pages = ud.bandcamp_pages(super::moment::now_secs() - BANDCAMP_FOUND_SECS)?;
        let mut out = Vec::new();
        let mut artists = 0;
        for (mbid, json) in ud.discographies()? {
            let Some(artist) = names.get(&mbid) else { continue };
            let Ok(groups) = serde_json::from_str::<Vec<Group>>(&json) else { continue };
            artists += 1;
            for g in groups {
                if have.has(&Candidate { album: g.title.clone(), artist: artist.clone(), release_group: Some(g.id.clone()), ..Candidate::default() }) {
                    continue;
                }
                out.push(MissingOf {
                    cover: format!("/api/covers/release-group/{}", g.id),
                    bandcamp: pages.get(&g.id).cloned(),
                    request: None,
                    release_group: g.id,
                    title: g.title,
                    artist: artist.clone(),
                    year: g.year,
                    kind: g.kind,
                });
            }
        }
        out.sort_by(|a, b| b.year.cmp(&a.year).then_with(|| a.artist.cmp(&b.artist)).then_with(|| a.title.cmp(&b.title)));
        Ok((out, artists))
    })
    .await?
}

/// An album left out of Discover (Insights › Discovery › Discovery exclusions, 0.9.3): Not interested (`no`) or
/// hidden, and when.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Excluded {
    pub key: String,
    pub album: String,
    pub artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    pub how: String,
    pub at: i64,
}

/// An artist left out of Discover: twice Not interested.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LeftOutArtist {
    pub artist: String,
    pub key: String,
    pub at: i64,
}

/// The account's Discover exclusions, the newest first: each album with its names (kept with the answer since 0.9.3;
/// an older one's from the last look where it is still there, else its key's own words), and the artists left out.
pub fn exclusions(serve: &crate::serve::Serve, account: &str) -> Result<(Vec<Excluded>, Vec<LeftOutArtist>)> {
    let rows = serve.userdata.discover_exclusions(account)?;
    let last: HashMap<String, Suggestion> = serve
        .userdata
        .discover_found(account)?
        .and_then(|(_, json)| serde_json::from_str::<Found>(&json).ok())
        .map(|f| f.albums.into_iter().map(|s| (s.key.clone(), s)).collect())
        .unwrap_or_default();
    let (artist_rows, rows): (Vec<_>, Vec<_>) = rows.into_iter().partition(|r| r.answer == ARTIST_ANSWER);
    let albums: Vec<Excluded> = rows
        .iter()
        .map(|r| {
            let s = last.get(&r.key);
            let (from_key_artist, from_key_album) = r.key.split_once('|').map_or(("", "Unknown album"), |(a, b)| (a, b));
            Excluded {
                key: r.key.clone(),
                album: r.name.clone().or_else(|| s.map(|s| s.album.clone())).unwrap_or_else(|| from_key_album.to_string()),
                artist: r.artist.clone().or_else(|| s.map(|s| s.artist.clone())).unwrap_or_else(|| from_key_artist.to_string()),
                year: r.year.or_else(|| s.and_then(|s| s.year)),
                cover: r.cover.clone().or_else(|| s.and_then(|s| s.cover.clone())),
                how: r.answer.clone(),
                at: r.at,
            }
        })
        .collect();
    // Twice Not interested: the artist is left out (as `discover_answered` decides).
    let mut by_artist: HashMap<&str, (usize, i64, String)> = HashMap::new();
    for (r, a) in rows.iter().zip(&albums).filter(|(r, _)| r.answer == "no") {
        let e = by_artist.entry(r.artist_key.as_str()).or_insert((0, 0, a.artist.clone()));
        e.0 += 1;
        if r.at > e.1 {
            (e.1, e.2) = (r.at, a.artist.clone());
        }
    }
    let mut artists: Vec<LeftOutArtist> =
        by_artist.into_iter().filter(|(_, (n, _, _))| *n >= 2).map(|(k, (_, at, name))| LeftOutArtist { artist: name, key: k.to_string(), at }).collect();
    // Said no to as an artist: once, with the name it was said to.
    for r in artist_rows {
        if !artists.iter().any(|a| a.key == r.artist_key) {
            artists.push(LeftOutArtist { artist: r.artist.clone().unwrap_or_else(|| r.artist_key.clone()), key: r.artist_key.clone(), at: r.at });
        }
    }
    artists.sort_by(|a, b| b.at.cmp(&a.at));
    Ok((albums, artists))
}

/// An exclusion taken back, any time (as its Undo): an album (`key`) or an artist (`artist`, their key).
pub fn take_back(serve: &crate::serve::Serve, account: &str, key: Option<&str>, artist: Option<&str>) -> Result<()> {
    if let Some(k) = key.filter(|k| !k.is_empty()) {
        serve.userdata.unanswer_discover(account, k)?;
    }
    if let Some(a) = artist.filter(|a| !a.is_empty()) {
        serve.userdata.unanswer_discover_artist(account, a)?;
    }
    Ok(())
}

/// An album MusicBrainz finds for "Find more music" (0.9.3, the owner: global search's last resort), and whether the
/// library has it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FoundAlbum {
    pub release_group: String,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub kind: Option<String>,
    pub cover: String,
    pub have: bool,
}

/// An artist MusicBrainz finds, and whether the library has them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FoundArtist {
    pub mbid: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disambiguation: Option<String>,
    pub have: bool,
}

/// How long a query's answer from MusicBrainz is kept (in memory): the page asks again as it is typed and opened.
const MB_SEARCH_KEEP: i64 = 600;

/// "Find more music" (0.9.3): MusicBrainz's release groups and artists for the words, its surest first, each marked
/// where the library has it; kept a few minutes per query. Two MusicBrainz calls, a second apart.
pub async fn musicbrainz_search(serve: &Arc<crate::serve::Serve>, cat: Arc<crate::catalogue::Catalogue>, q: &str, limit: usize) -> Result<(Vec<FoundAlbum>, Vec<FoundArtist>)> {
    use std::sync::{Mutex, OnceLock};
    type Kept = HashMap<String, (i64, Vec<FoundAlbum>, Vec<FoundArtist>)>;
    static KEPT: OnceLock<Mutex<Kept>> = OnceLock::new();
    let words = crate::search::words(q);
    if words.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let limit = limit.clamp(1, 50);
    let query = format!("{}@{limit}", words.join(" "));
    let now = super::moment::now_secs();
    let kept = KEPT.get_or_init(Default::default);
    if let Some((at, albums, artists)) = kept.lock().unwrap_or_else(|p| p.into_inner()).get(&query).cloned()
        && now - at < MB_SEARCH_KEEP
    {
        return Ok((albums, artists));
    }
    let http = http()?;
    let text = words.join(" ");
    let mut url = reqwest::Url::parse("https://musicbrainz.org/ws/2/release-group")?;
    url.query_pairs_mut().append_pair("query", &text).append_pair("fmt", "json").append_pair("limit", &limit.to_string());
    let rg: serde_json::Value = http.get(url).asked().await?.error_for_status()?.json().await?;
    tokio::time::sleep(MB_PAUSE).await;
    let mut url = reqwest::Url::parse("https://musicbrainz.org/ws/2/artist")?;
    url.query_pairs_mut().append_pair("query", &text).append_pair("fmt", "json").append_pair("limit", &limit.to_string());
    let ar: serde_json::Value = http.get(url).asked().await?.error_for_status()?.json().await?;
    let credit = |g: &serde_json::Value| {
        g["artist-credit"].as_array().map(|c| c.iter().map(|x| format!("{}{}", x["name"].as_str().unwrap_or_default(), x["joinphrase"].as_str().unwrap_or_default())).collect::<String>()).unwrap_or_default()
    };
    let mut albums: Vec<(u64, FoundAlbum)> = rg["release-groups"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| {
            let id = g["id"].as_str()?.to_string();
            Some((
                g["score"].as_u64().unwrap_or(0),
                FoundAlbum {
                    cover: format!("/api/covers/release-group/{id}"),
                    title: g["title"].as_str()?.to_string(),
                    artist: credit(g),
                    year: g["first-release-date"].as_str().and_then(|d| d.get(..4)?.parse().ok()),
                    kind: g["primary-type"].as_str().map(str::to_string),
                    release_group: id,
                    have: false,
                },
            ))
        })
        .collect();
    albums.sort_by(|a, b| b.0.cmp(&a.0));
    let mut artists: Vec<(u64, FoundArtist)> = ar["artists"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            Some((
                a["score"].as_u64().unwrap_or(0),
                FoundArtist {
                    mbid: a["id"].as_str()?.to_string(),
                    name: a["name"].as_str()?.to_string(),
                    disambiguation: a["disambiguation"].as_str().filter(|d| !d.is_empty()).map(str::to_string),
                    have: false,
                },
            ))
        })
        .collect();
    artists.sort_by(|a, b| b.0.cmp(&a.0));
    let (mut albums, mut artists): (Vec<FoundAlbum>, Vec<FoundArtist>) = (albums.into_iter().map(|x| x.1).collect(), artists.into_iter().map(|x| x.1).collect());
    // What the library has, as Discover leaves it out: albums by release group, else by name and artist; artists by
    // their MusicBrainz id, else their name.
    let dir = serve.data_dir.clone().context("Cratarr's data folder is needed")?;
    let lib = serve.library.clone();
    let (albums_in, artists_in) = (albums.clone(), artists.clone());
    let (have_albums, have_artists) = tokio::task::spawn_blocking(move || -> Result<(Vec<bool>, Vec<bool>)> {
        let rows = lib.artist_albums()?;
        let mbids: HashSet<String> = rows.iter().filter_map(|r| r.2.clone()).collect();
        let names: HashSet<String> = rows.iter().map(|r| loose(&r.1)).collect();
        let have = have(Path::new(&dir), &cat, rows.into_iter().map(|r| r.1))?;
        let a = albums_in.iter().map(|x| have.has(&Candidate { album: x.title.clone(), artist: x.artist.clone(), release_group: Some(x.release_group.clone()), ..Candidate::default() })).collect();
        let b = artists_in.iter().map(|x| mbids.contains(&x.mbid) || names.contains(&loose(&x.name))).collect();
        Ok((a, b))
    })
    .await??;
    for (x, h) in albums.iter_mut().zip(have_albums) {
        x.have = h;
    }
    for (x, h) in artists.iter_mut().zip(have_artists) {
        x.have = h;
    }
    kept.lock().unwrap_or_else(|p| p.into_inner()).insert(query, (now, albums.clone(), artists.clone()));
    Ok((albums, artists))
}

/// What was found last for the account, the albums answered since left out.
pub fn kept(serve: &crate::serve::Serve, account: &str) -> Result<Option<Found>> {
    let Some((_, json)) = serve.userdata.discover_found(account)? else { return Ok(None) };
    let mut found: Found = serde_json::from_str(&json).unwrap_or_default();
    let (answered, no_artists, everywhere) = leaves_out(&serve.userdata, account)?;
    found.albums.retain(|s| !answered.contains(&s.key) && !no_artists.contains(&s.artist_key) && !excluded_everywhere(s, &everywhere));
    // A look kept before 0.9 said ", which you play" after each reason: read without it.
    for w in found.albums.iter_mut().flat_map(|s| s.why.iter_mut()) {
        if let Some(short) = w.strip_suffix(", which you play") {
            *w = short.to_string();
        }
    }
    modes(&mut found, &fresh_since());
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
    let server_lastfm = server_lastfm_key(ud)?.map(|key| Auth { key, signed: None });
    Ok(Who { listenbrainz, lastfm, server_lastfm })
}

/// The server's own Last.fm key (0069 amended, 0.9: "one key for the whole server", so every account gets the artists
/// like its own most played, a Last.fm account or not): the one saved under Connections, else `LASTFM_API_KEY`.
pub fn server_lastfm_key(ud: &crate::userdata::UserData) -> Result<Option<String>> {
    // Switched off (0.9.6): the key is kept, not used.
    if !super::switches::is_on(super::switches::LASTFM) {
        return Ok(None);
    }
    let saved = ud.import_choice(SERVER_LASTFM)?.filter(|k| !k.trim().is_empty());
    Ok(saved.or_else(|| std::env::var(SERVER_LASTFM_ENV).ok().filter(|k| !k.trim().is_empty())))
}

/// A Last.fm API key as Last.fm gives them: 32 hexadecimal digits.
pub fn lastfm_key_ok(key: &str) -> bool {
    key.len() == 32 && key.bytes().all(|b| b.is_ascii_hexdigit())
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
    let (cat, a, artists) = (app.library.catalogue(), account.to_string(), app.artists.clone());
    super::tasks::spawn(KIND, account, false, move |h| async move {
        let progress = move |name: &str, done: u64, total: Option<u64>| h.step(name, done, total);
        look(&serve, cat, &a, &who, artists, &progress).await
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

/// What an artist's Not interested is kept as: an answer of its own, keyed by the artist.
const ARTIST_ANSWER: &str = "artist";
const ARTIST_KEY: &str = "artist:";

/// Not interested in an artist (the owner, 2026-10-29): they and every record of theirs leave Discover for good, in
/// every mode, as two albums' Not interested does. `name` as the page shows it. Its undo token.
pub fn artist_not_interested(serve: &crate::serve::Serve, account: &str, name: &str) -> Result<String> {
    let artist = loose(name.trim());
    anyhow::ensure!(!artist.is_empty(), "Which artist?");
    let key = format!("{ARTIST_KEY}{artist}");
    serve.userdata.answer_discover(account, &key, &artist, ARTIST_ANSWER)?;
    serve.userdata.name_discover_answer(account, &key, name.trim(), name.trim(), None, None)?;
    Ok(format!("{UNDO}{ARTIST_ANSWER}:{key}"))
}

/// A suggestion of the account's by its key: the look's, else Around's (0.9.7).
pub fn suggestion(serve: &crate::serve::Serve, account: &str, key: &str) -> Result<Option<Suggestion>> {
    if let Some(s) = kept(serve, account)?.and_then(|f| f.albums.into_iter().find(|s| s.key == key)) {
        return Ok(Some(s));
    }
    Ok(super::around::suggestions(serve, account)?.into_iter().find(|s| s.key == key))
}

fn answer(serve: &crate::serve::Serve, account: &str, key: &str, answer: &str) -> Result<String> {
    let s = suggestion(serve, account, key)?.context("That suggestion is gone; look again")?;
    serve.userdata.answer_discover(account, &s.key, &s.artist_key, answer)?;
    // Its names kept with it, for Insights' Discovery exclusions (0.9.3).
    serve.userdata.name_discover_answer(account, &s.key, &s.album, &s.artist, s.year, s.cover.as_deref())?;
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
    anyhow::ensure!(answer == "no" || answer == "hidden" || answer == ARTIST_ANSWER, "Not Discover's undo");
    if back {
        serve.userdata.unanswer_discover(account, key)?;
        Ok(format!("{REDO}{answer}:{key}"))
    } else if answer == ARTIST_ANSWER {
        let artist = key.strip_prefix(ARTIST_KEY).context("Not Discover's undo")?;
        serve.userdata.answer_discover(account, key, artist, ARTIST_ANSWER)?;
        Ok(format!("{UNDO}{answer}:{key}"))
    } else {
        self::answer(serve, account, key, answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_artists_plays_are_ones_own_none_heard_while_following() {
        let u = crate::userdata::UserData::open(None).unwrap();
        u.count_play("me", ("tr-1", "p1"), "al-1", &["ar-a".to_string()], 100).unwrap();
        let along = crate::userdata::Heard { event: Some("f1".into()), followed: true, ..crate::userdata::Heard::counted(200) };
        u.record("me", ("tr-2", "p2"), "al-2", &["ar-a".to_string(), "ar-b".to_string()], &along).unwrap();
        let plays = artist_plays(&u, "me").unwrap();
        assert_eq!(plays.get("ar-a"), Some(&1), "the followed play left out");
        assert_eq!(plays.get("ar-b").copied().unwrap_or(0), 0, "only heard while following: not played");
    }

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

    /// One list of `candidates`, nothing had or answered.
    fn listed(candidates: Vec<Candidate>) -> Vec<Suggestion> {
        one_list(candidates, &|_: &Candidate| false, &HashSet::new(), &HashSet::new(), &HashSet::new())
    }

    fn mode_of<'a>(found: &'a Found, album: &str) -> (Option<Mode>, f64) {
        let s = found.albums.iter().find(|s| s.album == album).unwrap();
        (s.mode, s.score)
    }

    #[test]
    fn each_source_fits_its_own_reason() {
        let list = listed(vec![
            c("Gap", "Mine", Some("rg-g"), "discography", "Missing from Mine's albums", 0.7),
            c("Gap", "Mine", Some("rg-g"), "listenbrainz", "Recommended by ListenBrainz", 0.4),
            c("Like", "Theirs", Some("rg-l"), "lastfm", "Like Mine", 0.9),
            c("Like", "Theirs", Some("rg-l"), "related", "Like Mine", 0.95),
            c("New", "Mine", Some("rg-n"), "fresh", "New from Mine", 0.8),
        ]);
        let fits = |a: &str| list.iter().find(|s| s.album == a).unwrap().fits;
        assert_eq!(fits("Gap"), Fits { gaps: Some(0.7), alike: Some(0.4), ..Fits::default() });
        assert_eq!(fits("Like"), Fits { artists: Some(0.95), ..Fits::default() }, "the best of one reason kept");
        assert_eq!(fits("New"), Fits { fresh: Some(0.8), ..Fits::default() });
    }

    #[test]
    fn the_reason_that_fits_best_is_the_mode() {
        let fresh = Candidate { released: Some("2026-10-01".into()), ..c("Out now", "Theirs", Some("rg-o"), "listenbrainz", "Recommended by ListenBrainz", 0.3) };
        let old = Candidate { released: Some("2026-01-01".into()), ..c("Out long ago", "Theirs", Some("rg-x"), "listenbrainz", "Recommended by ListenBrainz", 0.3) };
        let mut found = Found {
            albums: listed(vec![
                c("Gap wins", "Mine", Some("rg-g"), "discography", "Missing from Mine's albums", 0.7),
                c("Gap wins", "Mine", Some("rg-g"), "listenbrainz", "Recommended by ListenBrainz", 0.5),
                c("Alike wins", "Mine", Some("rg-a"), "discography", "Missing from Mine's albums", 0.3),
                c("Alike wins", "Mine", Some("rg-a"), "listenbrainz", "Recommended by ListenBrainz", 0.9),
                c("Even", "New one", Some("rg-e"), "related", "Like Mine", 0.6),
                c("Even", "New one", Some("rg-e"), "listenbrainz", "Recommended by ListenBrainz", 0.6),
                fresh,
                old,
            ]),
            ..Found::default()
        };
        modes(&mut found, "2026-08-01");
        assert_eq!(mode_of(&found, "Gap wins"), (Some(Mode::Gaps), 0.7));
        assert_eq!(mode_of(&found, "Alike wins"), (Some(Mode::Alike), 0.9));
        assert_eq!(mode_of(&found, "Even"), (Some(Mode::Artists), 0.6), "even: the narrower reason");
        assert_eq!(mode_of(&found, "Out now"), (Some(Mode::Fresh), 0.3), "out in the last three months: fresh, as well as it fits");
        assert_eq!(mode_of(&found, "Out long ago"), (Some(Mode::Alike), 0.3));
    }

    #[test]
    fn a_fresh_source_out_of_date_is_not_fresh() {
        let mut found = Found {
            albums: listed(vec![Candidate { released: Some("2025-12-01".into()), ..c("Was new", "Mine", Some("rg-w"), "fresh", "New from Mine", 0.8) }]),
            ..Found::default()
        };
        modes(&mut found, "2026-08-01");
        assert_eq!(mode_of(&found, "Was new").0, Some(Mode::Alike), "a reason it no longer has is no mode");
    }

    #[test]
    fn a_date_is_fresh_only_as_far_as_it_says() {
        assert!(out_since(Some("2026-08-01"), "2026-08-01"));
        assert!(out_since(Some("2026-09"), "2026-08-15"));
        assert!(!out_since(Some("2026-08"), "2026-08-15"), "a month that began before");
        assert!(!out_since(Some("2026"), "2026-01-01"), "a year alone never");
        assert!(!out_since(None, "2026-01-01"));
    }

    #[test]
    fn the_artists_mode_lists_each_artist_once_with_their_records_best_first() {
        let mut found = Found {
            albums: listed(vec![
                c("First", "New one", Some("rg-1"), "related", "Like Mine", 0.5),
                c("Second", "New one", Some("rg-2"), "lastfm", "Like Yours", 0.9),
                c("Other", "Another", Some("rg-3"), "related", "Like Mine", 0.7),
                c("Gap", "Mine", Some("rg-4"), "discography", "Missing from Mine's albums", 0.6),
            ]),
            ..Found::default()
        };
        found.albums.iter_mut().find(|s| s.album == "First").unwrap().artist_photo = true;
        modes(&mut found, "2026-08-01");
        let names: Vec<&str> = found.artists.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["New one", "Another"], "the best fit first");
        let one = &found.artists[0];
        assert_eq!(one.records, ["rg-2", "rg-1"]);
        assert_eq!(one.why, ["Like Yours", "Like Mine"], "the best record's reasons first");
        assert_eq!(one.score, 0.9);
        assert_eq!(one.photo.as_deref(), Some("/api/discover/artist/photo?name=New+one"));
        assert!(found.artists[1].photo.is_none());
    }

    #[test]
    fn a_look_kept_before_the_fits_gets_its_mode_from_its_reasons() {
        let s = |why: &str, new_artist: bool| Suggestion { why: vec![why.into()], new_artist, ..listed(vec![c("A", "B", Some("rg"), "listenbrainz", "x", 0.5)]).remove(0) };
        let mut found = Found { albums: vec![], ..Found::default() };
        for (album, why, new) in [("g", "Missing from B's albums", false), ("f", "New from B", false), ("l", "Like C", true), ("r", "Recommended by ListenBrainz", true)] {
            let mut x = s(why, new);
            x.album = album.into();
            x.key = album.into();
            x.fits = Fits::default();
            found.albums.push(x);
        }
        modes(&mut found, "2026-08-01");
        let m: Vec<Option<Mode>> = found.albums.iter().map(|s| s.mode).collect();
        assert_eq!(m, [Some(Mode::Gaps), Some(Mode::Alike), Some(Mode::Artists), Some(Mode::Alike)], "New from without a date is not fresh");
    }

    #[test]
    fn plays_weigh_the_gaps_and_the_fresh() {
        let mut list = listed(vec![
            c("Gap", "Mine", Some("rg-g"), "discography", "Missing from Mine's albums", 0.9),
            c("New", "Mine", Some("rg-n"), "fresh", "New from Mine", 0.8),
            c("Theirs", "Theirs", Some("rg-t"), "fresh", "New from Theirs", 0.8),
        ]);
        weigh(&mut list, &HashMap::from([("mine".to_string(), 10), ("most".to_string(), 20)]));
        let of = |a: &str| list.iter().find(|s| s.album == a).unwrap().clone();
        assert_eq!(of("Gap").artist_plays, Some(10));
        assert!((of("New").fits.fresh.unwrap() - 0.65).abs() < 1e-9, "a new record fits as much as its artist is played");
        assert_eq!((of("Theirs").artist_plays, of("Theirs").fits.fresh), (None, Some(0.8)), "an artist the library has not: as found");
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
    fn an_artist_excluded_everywhere_is_suggested_by_no_credit_and_not_interested_stays_discovers() {
        // Decision 0083: the everywhere list reaches Discover by any credit; Discover's own Not interested in an artist
        // stays Discover's alone, and is no exclusion everywhere.
        let ud = crate::userdata::UserData::open(None).unwrap();
        ud.exclude_artist("me", &crate::app::excluded_artists::key("Beck"), "Beck").unwrap();
        ud.answer_discover("me", "artist:bonobo", "bonobo", ARTIST_ANSWER).unwrap();
        let (answered, no_artists, everywhere) = leaves_out(&ud, "me").unwrap();
        assert_eq!(everywhere, HashSet::from(["beck".to_string()]), "Not interested is not everywhere");
        assert!(no_artists.contains("bonobo") && !no_artists.contains("beck"), "nor is everywhere Discover's Not interested");
        let mut list = one_list(
            vec![
                c("Odelay", "Beck", Some("rg-1"), "lastfm", "w", 1.0),
                c("Together", "Air feat. Beck", Some("rg-2"), "lastfm", "w", 1.0),
                c("Black Sands", "Bonobo", Some("rg-3"), "lastfm", "w", 1.0),
                c("Moon Safari", "Air", Some("rg-4"), "lastfm", "w", 1.0),
            ],
            &|_: &Candidate| false,
            &HashSet::new(),
            &answered,
            &no_artists,
        );
        list.retain(|s| !excluded_everywhere(s, &everywhere));
        assert_eq!(list.iter().map(|s| s.album.as_str()).collect::<Vec<_>>(), ["Moon Safari"]);
        assert!(leaves_out(&ud, "you").unwrap().2.is_empty(), "each account its own");
    }

    #[test]
    fn a_source_that_knows_what_an_album_is_says_so_and_is_named() {
        let mut fresh = c("New One", "Air", Some("rg-new"), "fresh", "New from Air", 0.8);
        (fresh.kind, fresh.genres) = (Some("EP".into()), vec!["electronic".into()]);
        let gap = Candidate { kind: Some("Album".into()), ..c("Talkie Walkie", "Air", Some("rg-tw"), "discography", "Missing from Air's albums", 0.9) };
        let list = one_list(vec![fresh, gap, c("New One", "Air", Some("rg-new"), "listenbrainz", "Recommended by ListenBrainz", 0.5)], &|_: &Candidate| false, &HashSet::new(), &HashSet::new(), &HashSet::new());
        let new_one = list.iter().find(|s| s.album == "New One").unwrap();
        assert_eq!((new_one.kind.as_deref(), new_one.genres.as_slice()), (Some("EP"), &["electronic".to_string()][..]));
        assert_eq!(new_one.sources, ["ListenBrainz"], "fresh releases are ListenBrainz's");
        let tw = list.iter().find(|s| s.album == "Talkie Walkie").unwrap();
        assert_eq!(tw.sources, ["MusicBrainz"], "a discography is MusicBrainz's");
        assert_eq!(tw.why, ["Missing from Air's albums"]);
    }

    #[test]
    fn how_alike_to_each_seed_is_kept_at_its_strongest_the_most_alike_first() {
        let a = |seed: &str, value: f32| Alike { seed: seed.into(), value };
        let first = Candidate { alike: vec![a("Air", 0.4), a("Bonobo", 0.9)], ..c("Moon", "Zero 7", Some("rg-m"), "lastfm", "Like Bonobo and Air", 0.8) };
        let again = Candidate { alike: vec![a("Air", 0.7)], ..c("Moon", "Zero 7", Some("rg-m"), "lastfm", "Like Air", 0.5) };
        let list = one_list(vec![first, again], &|_: &Candidate| false, &HashSet::new(), &HashSet::new(), &HashSet::new());
        assert_eq!(list[0].alike, vec![a("Bonobo", 0.9), a("Air", 0.7)]);
        let lb = one_list(vec![c("X", "Y", Some("rg-x"), "listenbrainz", "Recommended", 1.0)], &|_: &Candidate| false, &HashSet::new(), &HashSet::new(), &HashSet::new());
        assert!(lb[0].alike.is_empty(), "only Last.fm says how alike");
        assert!(!serde_json::to_string(&lb[0]).unwrap().contains("\"alike\":["), "left out when empty");
    }

    #[test]
    fn a_last_fm_key_is_32_hexadecimal_digits() {
        assert!(lastfm_key_ok("0123456789abcdef0123456789ABCDEF"));
        assert!(!lastfm_key_ok("0123456789abcdef"));
        assert!(!lastfm_key_ok("0123456789abcdef0123456789abcdeg"));
    }

    #[test]
    fn today_is_a_date_musicbrainz_compares() {
        let t = today();
        assert_eq!(t.len(), 10);
        assert!("2001" < t.as_str() && "2001-05" < t.as_str(), "a year-only date sorts before any day after it");
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
        let have = Have { release_groups: HashSet::from(["rg-1".to_string()]), matcher: Arc::new(m), artists: HashSet::new(), known: HashSet::new() };
        assert!(have.has(&c("Anything", "Anyone", Some("rg-1"), "lastfm", "", 0.0)));
        assert!(have.has(&c("Moon Safari (Remastered)", "AIR", None, "lastfm", "", 0.0)), "loosely, as imports match");
        assert!(!have.has(&c("Talkie Walkie", "Air", Some("rg-2"), "lastfm", "", 0.0)));
    }
}
