//! Scrobbling (the owner, 2026-10-24: "support last.fm and ListenBrainz's connections for sending listening behavior to
//! external sources using our connections"): every play Cratarr counts, from any of its players or a Subsonic app, sent
//! to the listening services the account connected. Imported listens never count, so none goes back.
//!
//! A counted play is queued in the same transaction that logs it (`userdata::UserData::record`, decision 0068's one way
//! in), one row per connected service; this sends the queue in batches, oldest first, again later with a growing pause
//! when a service is down, and stops for a service whose login fails until it is connected again (`broken`, shown in
//! Connections). It listens on the listening channel (`app::listening`): a counted play wakes it, and a track starting
//! is sent as "now playing" (best effort, never queued).
//!
//! - **ListenBrainz:** the user token from ListenBrainz's settings page, checked when given (`validate-token`).
//! - **Last.fm:** the API key and its shared secret the person made at last.fm/api, then Last.fm's web sign-in: Cratarr
//!   sends the browser to Last.fm, which comes back to `/api/scrobble/lastfm/callback` with a token Cratarr trades for a
//!   session key (`auth.getSession`); calls are signed with the secret.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use crate::serve::Serve;
use crate::userdata::Queued;
use super::reach::Sent as _;

pub const LISTENBRAINZ: &str = "listenbrainz";
pub const LASTFM: &str = "lastfm";
const LB_API: &str = "https://api.listenbrainz.org/1";
const FM_API: &str = "https://ws.audioscrobbler.com/2.0/";
/// Last.fm takes at most 50 scrobbles a call; ListenBrainz many more, sent alike.
const BATCH: usize = 50;
/// How often the queue is looked at when nothing nudged it.
const EVERY: Duration = Duration::from_secs(60);

/// What a connected service keeps, as JSON in `scrobble_target.secret`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Secret {
    /// ListenBrainz's user token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Last.fm's API key, its shared secret, and the session key once signed in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

fn http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(Duration::from_secs(30)).user_agent(concat!("Cratarr/", env!("CARGO_PKG_VERSION"))).build()?)
}

/// The ListenBrainz user a token belongs to; refused when it is no token.
pub async fn listenbrainz_user(token: &str) -> Result<String> {
    #[derive(Deserialize)]
    struct Valid {
        valid: bool,
        user_name: Option<String>,
    }
    let v: Valid = http()?
        .get(format!("{LB_API}/validate-token"))
        .header("Authorization", format!("Token {token}"))
        .sent()
        .await
        .context("ListenBrainz did not answer")?
        .json()
        .await?;
    match (v.valid, v.user_name) {
        (true, Some(u)) => Ok(u),
        _ => bail!("ListenBrainz does not know that token"),
    }
}

/// Last.fm's signature over a call's parameters: each name and value in name order, then the secret, md5 in hex.
pub fn signature(params: &BTreeMap<&str, String>, secret: &str) -> String {
    let mut s = String::new();
    for (k, v) in params {
        if *k == "format" || *k == "callback" {
            continue;
        }
        s.push_str(k);
        s.push_str(v);
    }
    s.push_str(secret);
    Md5::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// A sign-in at Last.fm under way: whose account it is for, and whether it began in the Android app (then the way back
/// is the app's own link, `cratarr://connected/lastfm`). Found by a one-time code in the address Last.fm comes back to,
/// so the way back needs no sign-in to Cratarr: it may end in a browser that has none (the app opens Last.fm there).
#[derive(Debug, Clone)]
pub struct Pending {
    pub account: String,
    pub app: bool,
}

/// How long a sign-in at Last.fm may take.
const PENDING_FOR: Duration = Duration::from_secs(600);

fn pending_list() -> &'static std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, Pending)>> {
    static PENDING: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, Pending)>>> = std::sync::OnceLock::new();
    PENDING.get_or_init(Default::default)
}

/// A new sign-in's one-time code.
pub fn pending(p: Pending) -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("the operating system's random source");
    let code: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let mut list = pending_list().lock().unwrap_or_else(|e| e.into_inner());
    list.retain(|_, (at, _)| at.elapsed() < PENDING_FOR);
    list.insert(code.clone(), (std::time::Instant::now(), p));
    code
}

/// The sign-in a code was made for, once (and only within its ten minutes).
pub fn take_pending(code: &str) -> Option<Pending> {
    let mut list = pending_list().lock().unwrap_or_else(|e| e.into_inner());
    list.remove(code).filter(|(at, _)| at.elapsed() < PENDING_FOR).map(|(_, p)| p)
}

/// Where to send the browser to sign in to Last.fm, coming back to `callback`.
pub fn lastfm_sign_in(api_key: &str, callback: &str) -> Result<String> {
    let mut url = reqwest::Url::parse("https://www.last.fm/api/auth/")?;
    url.query_pairs_mut().append_pair("api_key", api_key).append_pair("cb", callback);
    Ok(url.to_string())
}

/// Trades the token Last.fm's sign-in came back with for a session key: (session key, Last.fm user).
pub async fn lastfm_session(api_key: &str, api_secret: &str, token: &str) -> Result<(String, String)> {
    #[derive(Deserialize)]
    struct Session {
        key: String,
        name: String,
    }
    #[derive(Deserialize)]
    struct Answer {
        session: Option<Session>,
        message: Option<String>,
    }
    let mut p: BTreeMap<&str, String> = BTreeMap::new();
    p.insert("method", "auth.getSession".into());
    p.insert("api_key", api_key.into());
    p.insert("token", token.into());
    let sig = signature(&p, api_secret);
    p.insert("api_sig", sig);
    p.insert("format", "json".into());
    let a: Answer = http()?.get(FM_API).query(&p).sent().await.context("Last.fm did not answer")?.json().await?;
    match a.session {
        Some(s) => Ok((s.key, s.name)),
        None => bail!("Last.fm refused the sign-in: {}", a.message.unwrap_or_default()),
    }
}

/// A queued play with its song's names, ready to send.
struct Play {
    at: i64,
    artist: String,
    title: String,
    album: String,
    length_secs: u64,
}

/// Why a send failed: try again later, or the login is no good (stop until connected again).
enum Failed {
    Later(String),
    Login(String),
}

async fn send_listenbrainz(token: &str, plays: &[Play]) -> Result<(), Failed> {
    let payload: Vec<serde_json::Value> = plays
        .iter()
        .map(|p| {
            serde_json::json!({
                "listened_at": p.at,
                "track_metadata": {
                    "artist_name": p.artist,
                    "track_name": p.title,
                    "release_name": p.album,
                    "additional_info": {
                        "duration_ms": p.length_secs * 1000,
                        "media_player": "Cratarr",
                        "submission_client": "Cratarr",
                        "submission_client_version": env!("CARGO_PKG_VERSION"),
                    }
                }
            })
        })
        .collect();
    let kind = if plays.len() == 1 { "single" } else { "import" };
    let res = http()
        .map_err(|e| Failed::Later(format!("{e:#}")))?
        .post(format!("{LB_API}/submit-listens"))
        .header("Authorization", format!("Token {token}"))
        .json(&serde_json::json!({ "listen_type": kind, "payload": payload }))
        .sent()
        .await
        .map_err(|e| Failed::Later(format!("{e:#}")))?;
    match res.status().as_u16() {
        200..=299 => Ok(()),
        401 | 403 => Err(Failed::Login("ListenBrainz no longer takes the token".into())),
        // A listen it will never take (400): sending it again would block the queue, so it goes.
        400 => {
            let body = res.text().await.unwrap_or_default();
            tracing::warn!(target: "scrobble", body = %body, "ListenBrainz refused listens; dropped");
            Ok(())
        }
        s => Err(Failed::Later(format!("ListenBrainz answered {s}"))),
    }
}

async fn send_lastfm(secret: &Secret, plays: &[Play]) -> Result<(), Failed> {
    let (Some(key), Some(sec), Some(sk)) = (&secret.api_key, &secret.api_secret, &secret.session) else {
        return Err(Failed::Login("Last.fm is not signed in".into()));
    };
    let mut p: BTreeMap<String, String> = BTreeMap::new();
    p.insert("method".into(), "track.scrobble".into());
    p.insert("api_key".into(), key.clone());
    p.insert("sk".into(), sk.clone());
    for (i, x) in plays.iter().enumerate() {
        p.insert(format!("artist[{i}]"), x.artist.clone());
        p.insert(format!("track[{i}]"), x.title.clone());
        p.insert(format!("timestamp[{i}]"), x.at.to_string());
        p.insert(format!("album[{i}]"), x.album.clone());
        p.insert(format!("duration[{i}]"), x.length_secs.to_string());
    }
    let by: BTreeMap<&str, String> = p.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    let sig = signature(&by, sec);
    p.insert("api_sig".into(), sig);
    p.insert("format".into(), "json".into());
    let res = http().map_err(|e| Failed::Later(format!("{e:#}")))?.post(FM_API).form(&p).sent().await.map_err(|e| Failed::Later(format!("{e:#}")))?;
    let body: serde_json::Value = res.json().await.map_err(|e| Failed::Later(format!("{e:#}")))?;
    match body.get("error").and_then(|e| e.as_i64()) {
        None => Ok(()),
        // Invalid session, key or signature: the login.
        Some(4 | 9 | 10 | 13 | 14 | 26) => Err(Failed::Login(format!("Last.fm: {}", body["message"].as_str().unwrap_or("the sign-in is no longer valid")))),
        Some(_) => Err(Failed::Later(format!("Last.fm: {}", body["message"].as_str().unwrap_or("try again later")))),
    }
}

/// A pause before trying again: a minute, doubling, at most six hours.
fn later(tries: i64) -> i64 {
    60i64.saturating_mul(1 << tries.clamp(0, 9)).min(6 * 3600)
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// One pass over the queue: each account's due plays to each of its services, a batch at a time.
async fn pass(serve: &Arc<Serve>) -> Result<()> {
    let ud = serve.userdata.clone();
    let due: Vec<Queued> = tokio::task::spawn_blocking(move || ud.due_scrobbles(now(), 500)).await??;
    if due.is_empty() {
        return Ok(());
    }
    let mut groups: BTreeMap<(String, String), Vec<Queued>> = BTreeMap::new();
    for q in due {
        groups.entry((q.user.clone(), q.target.clone())).or_default().push(q);
    }
    for ((user, target), rows) in groups {
        // Queued before the account switched its play history off (0.9.5): never sent.
        let (ud, u) = (serve.userdata.clone(), user.clone());
        if tokio::task::spawn_blocking(move || -> Result<bool> {
            let off = !ud.tracking(&u);
            if off {
                ud.forget_scrobbles(&u)?;
            }
            Ok(off)
        })
        .await??
        {
            continue;
        }
        let (ud, u, t) = (serve.userdata.clone(), user.clone(), target.clone());
        let Some(secret) = tokio::task::spawn_blocking(move || ud.scrobble_secret(&u, &t)).await?? else { continue };
        let secret: Secret = serde_json::from_str(&secret).unwrap_or_default();
        for chunk in rows.chunks(BATCH) {
            let (lib, ids) = (serve.library.clone(), chunk.iter().map(|q| q.song.clone()).collect::<Vec<_>>());
            let songs = tokio::task::spawn_blocking(move || lib.songs_by_id(&ids)).await??;
            let by: std::collections::HashMap<&str, &crate::serve::api::Song> = songs.iter().map(|s| (s.id.as_str(), s)).collect();
            // A song gone from the library since: nothing to name it by, so it goes.
            let (named, gone): (Vec<&Queued>, Vec<&Queued>) = chunk.iter().partition(|q| by.contains_key(q.song.as_str()));
            let plays: Vec<Play> = named
                .iter()
                .map(|q| {
                    let s = by[q.song.as_str()];
                    Play { at: q.at, artist: s.artist.clone(), title: s.title.clone(), album: s.album.clone(), length_secs: s.duration }
                })
                .collect();
            let sent = if plays.is_empty() {
                Ok(())
            } else if target == LISTENBRAINZ {
                match &secret.token {
                    Some(tok) => send_listenbrainz(tok, &plays).await,
                    None => Err(Failed::Login("ListenBrainz has no token".into())),
                }
            } else {
                send_lastfm(&secret, &plays).await
            };
            let ud = serve.userdata.clone();
            let ids: Vec<i64> = chunk.iter().map(|q| q.id).collect();
            let gone: Vec<i64> = gone.iter().map(|q| q.id).collect();
            let tries = chunk.iter().map(|q| q.tries).max().unwrap_or(0);
            let (u, t) = (user.clone(), target.clone());
            match sent {
                Ok(()) => {
                    tracing::info!(target: "scrobble", user = %user, to = %target, plays = plays.len(), "scrobbled");
                    tokio::task::spawn_blocking(move || ud.scrobbled(&ids)).await??;
                }
                Err(Failed::Later(why)) => {
                    tracing::warn!(target: "scrobble", user = %user, to = %target, reason = %why, "scrobbles wait");
                    tokio::task::spawn_blocking(move || {
                        ud.scrobbled(&gone)?;
                        ud.scrobbles_wait(&ids, now() + later(tries), &why)
                    })
                    .await??;
                    break;
                }
                Err(Failed::Login(why)) => {
                    tracing::warn!(target: "scrobble", user = %user, to = %target, reason = %why, "scrobbling stopped until connected again");
                    tokio::task::spawn_blocking(move || ud.scrobble_broken(&u, &t, &why)).await??;
                    break;
                }
            }
        }
    }
    Ok(())
}

/// "Now playing" to `user`'s connected services: best effort, never queued (it is stale a minute later).
async fn now_playing(serve: Arc<Serve>, user: String, song: String) -> Result<()> {
    let (ud, u) = (serve.userdata.clone(), user.clone());
    let secrets = tokio::task::spawn_blocking(move || -> Result<Vec<(String, String)>> {
        let mut out = Vec::new();
        for t in [LISTENBRAINZ, LASTFM] {
            if let Some(s) = ud.scrobble_secret(&u, t)? {
                out.push((t.to_string(), s));
            }
        }
        Ok(out)
    })
    .await??;
    if secrets.is_empty() {
        return Ok(());
    }
    let (lib, ids) = (serve.library.clone(), vec![song]);
    let Some(s) = tokio::task::spawn_blocking(move || lib.songs_by_id(&ids)).await??.into_iter().next() else { return Ok(()) };
    for (target, secret) in secrets {
        let secret: Secret = serde_json::from_str(&secret).unwrap_or_default();
        let sent = if target == LISTENBRAINZ {
            let Some(token) = &secret.token else { continue };
            let body = serde_json::json!({ "listen_type": "playing_now", "payload": [{ "track_metadata": {
                "artist_name": s.artist, "track_name": s.title, "release_name": s.album,
                "additional_info": { "duration_ms": s.duration * 1000, "media_player": "Cratarr", "submission_client": "Cratarr" } } }] });
            http()?.post(format!("{LB_API}/submit-listens")).header("Authorization", format!("Token {token}")).json(&body).sent().await.map(|_| ())
        } else {
            let (Some(key), Some(sec), Some(sk)) = (&secret.api_key, &secret.api_secret, &secret.session) else { continue };
            let mut p: BTreeMap<&str, String> = BTreeMap::new();
            p.insert("method", "track.updateNowPlaying".into());
            p.insert("api_key", key.clone());
            p.insert("sk", sk.clone());
            p.insert("artist", s.artist.clone());
            p.insert("track", s.title.clone());
            p.insert("album", s.album.clone());
            p.insert("duration", s.duration.to_string());
            let sig = signature(&p, sec);
            p.insert("api_sig", sig);
            p.insert("format", "json".into());
            http()?.post(FM_API).form(&p).sent().await.map(|_| ())
        };
        if let Err(e) = sent {
            tracing::debug!(target: "scrobble", user = %user, to = %target, reason = %format!("{e:#}"), "now playing not sent");
        }
    }
    Ok(())
}

/// Sends the queue: now, every minute, and whenever a counted play comes in; and "now playing" as tracks start
/// (decision 0068's channel).
pub fn run(serve: Arc<Serve>) {
    let wake = Arc::new(tokio::sync::Notify::new());
    let (w, s) = (wake.clone(), serve.clone());
    let mut events = super::listening::subscribe();
    tokio::spawn(async move {
        use super::listening::Event;
        use tokio::sync::broadcast::error::RecvError;
        loop {
            match events.recv().await {
                Ok(Event::Heard { counted: true, .. }) => w.notify_one(),
                Ok(Event::Started { user, song, .. }) => {
                    let s = s.clone();
                    tokio::spawn(async move {
                        if let Err(e) = now_playing(s, user, song).await {
                            tracing::debug!(target: "scrobble", reason = %format!("{e:#}"), "now playing failed");
                        }
                    });
                }
                Ok(Event::Heard { .. }) => {}
                // Behind: what was missed is in the queue already; look at it.
                Err(RecvError::Lagged(_)) => w.notify_one(),
                Err(RecvError::Closed) => return,
            }
        }
    });
    tokio::spawn(async move {
        loop {
            if let Err(e) = pass(&serve).await {
                tracing::warn!(target: "scrobble", reason = %format!("{e:#}"), "scrobble pass failed");
            }
            let _ = tokio::time::timeout(EVERY, wake.notified()).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_fms_signature_is_its_names_and_values_in_order_then_the_secret() {
        // Last.fm's own example: api_key, method, token, in that order, then the secret, md5.
        let mut p: BTreeMap<&str, String> = BTreeMap::new();
        p.insert("token", "t".into());
        p.insert("method", "auth.getSession".into());
        p.insert("api_key", "k".into());
        p.insert("format", "json".into());
        let want: String = Md5::digest(b"api_keykmethodauth.getSessiontokents").iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(signature(&p, "s"), want, "format left out");
    }

    #[test]
    fn a_sign_ins_code_is_good_once() {
        let code = pending(Pending { account: "me".into(), app: true });
        assert_eq!(code.len(), 32);
        assert!(take_pending(&code).is_some_and(|p| p.account == "me" && p.app));
        assert!(take_pending(&code).is_none(), "used");
        assert!(take_pending("not a code").is_none());
    }

    #[test]
    fn a_failed_send_waits_longer_each_time_and_at_most_six_hours() {
        assert_eq!(later(0), 60);
        assert_eq!(later(3), 480);
        assert_eq!(later(30), 6 * 3600);
    }
}
