//! Spotify's Web API as decision 0073 uses it: the sign-in (Authorization Code with PKCE: no client secret, the
//! account's own app), and reading the account's lists a page at a time (`/me/albums`, `/me/tracks`, `/me/playlists`
//! with each own or collaborative playlist's `/items`, `/me/following`). Spotify's 2026 shapes: a playlist's entries
//! under `items`, each one's track as `item` (`track` before February 2026, read too).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{Album, AlbumTrack, Artist, Lists, Playlist, Track};
use crate::app::reach::Sent;

const AUTHORIZE: &str = "https://accounts.spotify.com/authorize";
const TOKEN: &str = "https://accounts.spotify.com/api/token";
const API: &str = "https://api.spotify.com/v1";
/// Reading only: nothing plays on Spotify (0073, amended).
const SCOPES: &str = "user-library-read playlist-read-private playlist-read-collaborative user-follow-read";
/// A sign-in begun and not come back after this long is forgotten.
const PENDING_FOR: Duration = Duration::from_secs(15 * 60);
/// Spotify's "too many" without saying how long: waited this long.
const BACK_OFF: Duration = Duration::from_secs(5);
/// Waits longer than this (Spotify's `Retry-After`) end the read: it is tried again at the next turn.
const LONGEST_WAIT: Duration = Duration::from_secs(120);

pub fn http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(Duration::from_secs(30)).user_agent(concat!("Cratarr/", env!("CARGO_PKG_VERSION"))).build()?)
}

/// A sign-in on its way: for whom, its PKCE verifier, the address Spotify comes back to, and whether it began in the
/// Android app.
#[derive(Debug, Clone)]
pub struct Pending {
    pub account: String,
    pub client_id: String,
    pub verifier: String,
    pub redirect: String,
    pub app: bool,
    at: Instant,
}

static PENDING: Mutex<Option<HashMap<String, Pending>>> = Mutex::new(None);

/// A text of `n` random URL-safe characters (the verifier, the state).
fn random(n: usize) -> String {
    let mut bytes = vec![0u8; n];
    getrandom::fill(&mut bytes).expect("the system's randomness");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes).chars().take(n).collect()
}

/// Where to send the browser to sign in, coming back to `redirect`; the sign-in is kept for its way back.
pub fn sign_in(account: &str, client_id: &str, redirect: &str, app: bool) -> Result<String> {
    let verifier = random(64);
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random(32);
    let mut url = reqwest::Url::parse(AUTHORIZE)?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect)
        .append_pair("code_challenge_method", "S256")
        .append_pair("code_challenge", &challenge)
        .append_pair("state", &state)
        .append_pair("scope", SCOPES);
    let mut pending = PENDING.lock().unwrap_or_else(|p| p.into_inner());
    let all = pending.get_or_insert_with(HashMap::new);
    all.retain(|_, p| p.at.elapsed() < PENDING_FOR);
    all.insert(state, Pending { account: account.into(), client_id: client_id.into(), verifier, redirect: redirect.into(), app, at: Instant::now() });
    Ok(url.into())
}

/// The sign-in a way back belongs to, once.
pub fn take_pending(state: &str) -> Option<Pending> {
    let mut pending = PENDING.lock().unwrap_or_else(|p| p.into_inner());
    pending.as_mut()?.remove(state).filter(|p| p.at.elapsed() < PENDING_FOR)
}

/// An access token, and the refresh token to keep (Spotify may hand out a new one).
#[derive(Debug, Clone)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
}

fn tokens(v: &Value, old_refresh: Option<&str>) -> Result<Tokens> {
    let access = v["access_token"].as_str().context("Spotify gave no access token")?.to_string();
    let refresh = v["refresh_token"].as_str().or(old_refresh).context("Spotify gave no refresh token")?.to_string();
    Ok(Tokens { access, refresh })
}

/// Spotify's answer to a token request, its own words when it refused.
async fn token_request(http: &reqwest::Client, form: &[(&str, &str)]) -> Result<Value> {
    let r = http.post(TOKEN).form(form).sent().await?;
    let status = r.status();
    let v: Value = r.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let why = v["error_description"].as_str().or(v["error"].as_str()).unwrap_or("no reason given");
        bail!("Spotify refused the sign-in ({status}): {why}");
    }
    Ok(v)
}

/// The way back's code traded for the tokens.
pub async fn trade(http: &reqwest::Client, p: &Pending, code: &str) -> Result<Tokens> {
    let v = token_request(
        http,
        &[("grant_type", "authorization_code"), ("code", code), ("redirect_uri", &p.redirect), ("client_id", &p.client_id), ("code_verifier", &p.verifier)],
    )
    .await?;
    tokens(&v, None)
}

/// A new access token from the kept refresh token.
pub async fn refresh(http: &reqwest::Client, client_id: &str, refresh: &str) -> Result<Tokens> {
    let v = token_request(http, &[("grant_type", "refresh_token"), ("refresh_token", refresh), ("client_id", client_id)]).await?;
    tokens(&v, Some(refresh))
}

/// One GET of the Web API: waited out and asked again on 429 as Spotify says.
pub async fn get(http: &reqwest::Client, access: &str, url: &str) -> Result<Value> {
    for _ in 0..5 {
        let r = http.get(url).bearer_auth(access).sent().await?;
        let status = r.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let wait = r.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).map(Duration::from_secs).unwrap_or(BACK_OFF);
            if wait > LONGEST_WAIT {
                bail!("Spotify asks to wait {} s: read again later", wait.as_secs());
            }
            tokio::time::sleep(wait).await;
            continue;
        }
        if !status.is_success() {
            let v: Value = r.json().await.unwrap_or(Value::Null);
            let why = v["error"]["message"].as_str().unwrap_or("no reason given");
            return Err(anyhow!("Spotify answered {status}: {why}")).context(Refusal(status.as_u16()));
        }
        return Ok(r.json().await?);
    }
    bail!("Spotify kept asking to wait: read again later")
}

/// The status Spotify refused a GET with: a playlist it will not show is passed over, the rest ends the read.
#[derive(Debug)]
pub struct Refusal(pub u16);

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "refused ({})", self.0)
    }
}

/// Every item of a paged list, from its first page on (`next` followed); `at` picks the page out of the answer.
async fn all(http: &reqwest::Client, access: &str, first: &str, at: impl Fn(&Value) -> &Value) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    let mut next = Some(first.to_string());
    while let Some(url) = next.take() {
        let v = get(http, access, &url).await?;
        let page = at(&v);
        out.extend(page["items"].as_array().cloned().unwrap_or_default());
        next = page["next"].as_str().map(str::to_string);
    }
    Ok(out)
}

/// The signed-in Spotify user: id and name.
pub async fn me(http: &reqwest::Client, access: &str) -> Result<(String, String)> {
    let v = get(http, access, &format!("{API}/me")).await?;
    let id = v["id"].as_str().context("Spotify gave no user")?.to_string();
    let name = v["display_name"].as_str().filter(|n| !n.is_empty()).unwrap_or(&id).to_string();
    Ok((id, name))
}

/// The account's four lists, as they are now.
pub async fn read(http: &reqwest::Client, access: &str, me: &str) -> Result<Lists> {
    let mut lists = Lists::default();
    for saved in all(http, access, &format!("{API}/me/albums?limit=50"), |v| v).await? {
        let mut album = album_of(&saved["album"], added(&saved));
        // An album of more than 50 tracks: the rest of its list.
        if let Some(more) = saved["album"]["tracks"]["next"].as_str() {
            for t in all(http, access, more, |v| v).await? {
                album.tracks.push(album_track(&t));
            }
        }
        lists.albums.push(album);
    }
    for saved in all(http, access, &format!("{API}/me/tracks?limit=50"), |v| v).await? {
        if let Some(t) = track_of(&saved["track"], added(&saved)) {
            lists.tracks.push(t);
        }
    }
    for p in all(http, access, &format!("{API}/me/playlists?limit=50"), |v| v).await? {
        let (Some(id), Some(name)) = (p["id"].as_str(), p["name"].as_str()) else { continue };
        let owner = p["owner"]["id"].as_str().unwrap_or_default();
        let collaborative = p["collaborative"].as_bool().unwrap_or(false);
        // Only one's own and collaborative playlists can be read (development mode); followed ones come by Exportify.
        if owner != me && !collaborative {
            continue;
        }
        let entries = match all(http, access, &format!("{API}/playlists/{id}/items?limit=50"), |v| v).await {
            Ok(e) => e,
            Err(e) if e.downcast_ref::<Refusal>().is_some_and(|r| matches!(r.0, 403 | 404)) => continue,
            Err(e) => return Err(e),
        };
        let tracks = entries.iter().filter_map(|e| track_of(e.get("item").filter(|i| !i.is_null()).unwrap_or(&e["track"]), added(e))).collect();
        lists.playlists.push(Playlist {
            id: id.into(),
            name: name.into(),
            owner: p["owner"]["display_name"].as_str().or(Some(owner)).map(str::to_string),
            collaborative,
            cover: image(&p["images"]),
            tracks,
        });
    }
    for a in all(http, access, &format!("{API}/me/following?type=artist&limit=50"), |v| &v["artists"]).await? {
        let (Some(id), Some(name)) = (a["id"].as_str(), a["name"].as_str()) else { continue };
        lists.artists.push(Artist { id: id.into(), name: name.into(), photo: image(&a["images"]) });
    }
    Ok(lists)
}

/// When an item was saved (epoch seconds).
fn added(v: &Value) -> Option<i64> {
    v["added_at"].as_str().and_then(crate::nowplaying::parse_iso_epoch)
}

/// The picture nearest 300 pixels wide.
fn image(images: &Value) -> Option<String> {
    let all = images.as_array()?;
    all.iter()
        .min_by_key(|i| (i["width"].as_i64().unwrap_or(300) - 300).abs())
        .and_then(|i| i["url"].as_str())
        .map(str::to_string)
}

fn artists(v: &Value) -> String {
    v.as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default()
}

fn album_track(t: &Value) -> AlbumTrack {
    AlbumTrack { title: t["name"].as_str().unwrap_or_default().into(), length_ms: t["duration_ms"].as_u64() }
}

fn album_of(a: &Value, added_at: Option<i64>) -> Album {
    Album {
        id: a["id"].as_str().unwrap_or_default().into(),
        title: a["name"].as_str().unwrap_or_default().into(),
        artist: artists(&a["artists"]),
        year: a["release_date"].as_str().and_then(|d| d.get(..4)).and_then(|y| y.parse().ok()),
        cover: image(&a["images"]),
        added_at,
        upc: a["external_ids"]["upc"].as_str().map(str::to_string),
        tracks: a["tracks"]["items"].as_array().map(|t| t.iter().map(album_track).collect()).unwrap_or_default(),
    }
}

/// A track, not an episode nor a file of the user's own.
fn track_of(t: &Value, added_at: Option<i64>) -> Option<Track> {
    if t.is_null() || t["is_local"].as_bool() == Some(true) || t["type"].as_str().is_some_and(|k| k != "track") {
        return None;
    }
    Some(Track {
        id: t["id"].as_str()?.into(),
        title: t["name"].as_str().unwrap_or_default().into(),
        artist: artists(&t["artists"]),
        album: t["album"]["name"].as_str().unwrap_or_default().into(),
        album_artist: Some(artists(&t["album"]["artists"])).filter(|a| !a.is_empty()),
        album_id: t["album"]["id"].as_str().map(str::to_string),
        length_ms: t["duration_ms"].as_u64(),
        added_at,
        isrc: t["external_ids"]["isrc"].as_str().map(str::to_string),
        cover: image(&t["album"]["images"]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_playlist_entry_is_read_in_2026s_shape_and_the_one_before() {
        let new = serde_json::json!({ "added_at": "2026-03-01T10:00:00Z", "item": { "type": "track", "id": "t1", "name": "Zombie",
            "duration_ms": 306000, "artists": [{ "name": "The Cranberries" }], "external_ids": { "isrc": "IEACJ9400015" },
            "album": { "id": "a1", "name": "No Need To Argue", "artists": [{ "name": "The Cranberries" }], "images": [] } } });
        let t = track_of(new.get("item").unwrap(), added(&new)).unwrap();
        assert_eq!((t.id.as_str(), t.isrc.as_deref(), t.added_at), ("t1", Some("IEACJ9400015"), Some(1_772_359_200)));
        let old = serde_json::json!({ "track": { "type": "episode", "id": "e1", "name": "A podcast" } });
        assert!(track_of(&old["track"], None).is_none());
        assert!(track_of(&serde_json::json!({ "id": "x", "is_local": true }), None).is_none());
    }

    #[test]
    fn a_sign_in_comes_back_once() {
        let url = sign_in("me", "client", "https://cratarr.example/api/spotify/callback", false).unwrap();
        let state = reqwest::Url::parse(&url).unwrap().query_pairs().find(|(k, _)| k == "state").unwrap().1.into_owned();
        assert!(url.contains("code_challenge_method=S256") && url.contains("user-library-read"));
        assert_eq!(take_pending(&state).unwrap().account, "me");
        assert!(take_pending(&state).is_none());
    }
}
