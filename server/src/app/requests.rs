//! Requests (decision 0070): an album asked for from Discover is kept by Cratarr with its own status (pending,
//! approved, declined, cancelled); an approved one is handed to a fetcher (`ports::Fetcher`, Lidarr first), and the
//! album's own state (being fetched, in Incoming, in the library) read beside it. Seerr's model (MIT,
//! github.com/seerr-team/seerr), not its code. Who may ask, have their asking approved at once, and answer everyone's:
//! the account's permissions (`serve::users::Permissions`), an administrator's all.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use super::import::Matcher;
use crate::lidarr::Progress;
use crate::ports::{Fetcher, LidarrFetcher};
use crate::serve::Serve;
use crate::serve::users::Permissions;
use crate::userdata::Request;

pub const PENDING: &str = "pending";
pub const APPROVED: &str = "approved";
pub const DECLINED: &str = "declined";
pub const CANCELLED: &str = "cancelled";

/// The most requests a list shows.
const LIST: usize = 200;
/// The most albums of Lidarr's Wanted the page shows.
const WATCHED: usize = 50;

/// An album's own state beside its request's (decision 0070): asked for and not handed over yet, looked for, being
/// fetched (how far), in Incoming, partly there, in the library; or not known to the fetcher (it is not connected,
/// or did not answer).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Album {
    Requested,
    Looking,
    Fetching { done: f32 },
    InIncoming,
    PartlyThere { tracks: u32, of: u32 },
    InLibrary,
    Unknown,
}

/// A request as the pages show it: as kept, and its album's state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct View {
    #[serde(flatten)]
    pub request: Request,
    pub album_state: Album,
}

/// The Requests page: the account's own requests, everyone's waiting ones for those who answer them, and what the
/// account may do.
#[derive(Debug, Clone, Serialize)]
pub struct Listing {
    pub mine: Vec<View>,
    pub waiting: Option<Vec<View>>,
    pub may: Permissions,
    /// A fetcher is connected (Lidarr, switched on).
    pub fetcher: bool,
}

/// What the signed-in account may do with requests: its permissions, an administrator's all (signed in as one, not
/// through one of their keys: access.rs `administering`).
pub fn rights(serve: &Serve) -> Permissions {
    if crate::serve::access::administering() {
        return Permissions::all();
    }
    serve.current_user().and_then(|n| serve.users.get(&n)).map(|u| u.permissions).unwrap_or_default()
}

/// The fetcher approved requests are handed to: Lidarr, where it is connected and switched on, with the profiles and
/// root folder chosen on its page in Connections.
pub fn fetcher(app: &super::App) -> Option<Box<dyn Fetcher>> {
    let lidarr = super::discover::lidarr(app)?;
    Some(Box::new(LidarrFetcher { lidarr, with: super::discover::add_with(app) }))
}

/// A suggestion of the account's Discover asked for: kept pending, or approved and handed over at once where the account
/// may (an administrator's, or Auto-approve). Asked again, the request already open is the answer.
pub async fn ask(app: &super::App, serve: &Serve, account: &str, key: &str) -> Result<Request> {
    if !rights(serve).request {
        bail!("Requesting albums is not on for this account");
    }
    let s = super::discover::suggestion(serve, account, key)?.context("That suggestion is gone; look again")?;
    ask_for(app, serve, account, Asked { key: s.key, artist_key: s.artist_key, release_group: s.release_group, album: s.album, artist: s.artist, year: s.year, cover: s.cover }).await
}

/// Any album asked for by its names (0.9.3, the owner: Want for any album an artist's page says is missing): keyed as
/// Discover keys albums (its release group, else its artist's and its name's), so its suggestion leaves Discover too.
#[allow(clippy::too_many_arguments)]
pub async fn ask_album(app: &super::App, serve: &Serve, account: &str, release_group: Option<String>, artist: &str, album: &str, year: Option<i32>, cover: Option<String>) -> Result<Request> {
    if !rights(serve).request {
        bail!("Requesting albums is not on for this account");
    }
    let release_group = release_group.filter(|g| !g.is_empty());
    let key = release_group.clone().unwrap_or_else(|| format!("{}|{}", super::import::key(artist), super::import::key(album)));
    let asked = Asked { key, artist_key: super::import::loose(artist), release_group, album: album.to_string(), artist: artist.to_string(), year, cover };
    ask_for(app, serve, account, asked).await
}

/// What a request asks for: an album, by its key and its artist's, and what to show of it.
struct Asked {
    key: String,
    artist_key: String,
    release_group: Option<String>,
    album: String,
    artist: String,
    year: Option<i32>,
    cover: Option<String>,
}

/// The request for `s`: the account's open one for that album where there is one, else a new one (approved at once
/// where the account may, and handed to the fetcher).
async fn ask_for(app: &super::App, serve: &Serve, account: &str, s: Asked) -> Result<Request> {
    let may = rights(serve);
    let open = serve.userdata.requests(Some(account), None, LIST)?;
    if let Some(r) = open.into_iter().find(|r| r.album_key == s.key && (r.status == PENDING || r.status == APPROVED)) {
        return Ok(r);
    }
    let now = super::moment::now_secs();
    let approved = may.auto_approve;
    let mut r = Request {
        id: 0,
        user: account.to_string(),
        album_key: s.key,
        artist_key: s.artist_key,
        release_group: s.release_group,
        album: s.album,
        artist: s.artist,
        year: s.year,
        cover: s.cover,
        at: now,
        status: if approved { APPROVED } else { PENDING }.to_string(),
        reason: None,
        decided_by: approved.then(|| account.to_string()),
        decided_at: approved.then_some(now),
        fetched: None,
    };
    r.id = serve.userdata.add_request(&r)?;
    if approved {
        r.fetched = Some(hand(app, serve, &r).await);
    }
    Ok(r)
}

/// An approved request handed to the fetcher; what it said (or why it could not), kept on the request.
async fn hand(app: &super::App, serve: &Serve, r: &Request) -> String {
    let said = match fetcher(app) {
        None => "Not handed over: Lidarr is not connected, or is switched off".to_string(),
        Some(f) => match f.want(r.release_group.as_deref(), &r.artist, &r.album).await {
            Ok(said) => said,
            Err(e) => {
                // In the log too (0.9.6: a Lidarr timeout the owner saw left no trace to follow).
                tracing::warn!("request {}: not handed to Lidarr: {e:#}", r.id);
                format!("Not handed over: {e:#}")
            }
        },
    };
    if let Err(e) = serve.userdata.set_request_fetched(r.id, &said) {
        tracing::warn!("request {}: what the fetcher said was not kept: {e:#}", r.id);
    }
    said
}

/// Requests answered, one or many (the select mode): approved ones handed over, declined ones with the reason, if one
/// was given. Only pending ones are answered; an approved one that was not handed over is handed over again. Each id's
/// outcome, in words.
pub async fn decide(app: &super::App, serve: &Serve, by: &str, ids: &[i64], approve: bool, reason: Option<&str>) -> Result<Vec<(i64, String)>> {
    if !rights(serve).manage_requests {
        bail!("Answering requests is not on for this account");
    }
    let reason = reason.map(str::trim).filter(|r| !r.is_empty());
    let mut out = Vec::new();
    for &id in ids {
        let Some(r) = serve.userdata.request(id)? else {
            out.push((id, "That request is gone".to_string()));
            continue;
        };
        let retry = approve && r.status == APPROVED && r.fetched.as_deref().is_none_or(|f| f.starts_with("Not handed over"));
        if r.status != PENDING && !retry {
            out.push((id, format!("Already {}", r.status)));
            continue;
        }
        if approve {
            if !retry {
                serve.userdata.set_request_status(id, APPROVED, None, Some(by))?;
            }
            out.push((id, hand(app, serve, &r).await));
        } else {
            serve.userdata.set_request_status(id, DECLINED, reason, Some(by))?;
            out.push((id, "Declined".to_string()));
        }
    }
    Ok(out)
}

/// A request taken back: the account's own while pending; an approved one by those who answer requests, the fetcher
/// told to stop looking for it (what is there already stays).
pub async fn cancel(app: &super::App, serve: &Serve, account: &str, id: i64) -> Result<()> {
    let r = serve.userdata.request(id)?.context("That request is gone")?;
    let manage = rights(serve).manage_requests;
    match r.status.as_str() {
        PENDING if r.user == account || manage => {}
        APPROVED if manage => {
            if let (Some(f), Some(rg)) = (fetcher(app), r.release_group.as_deref()) {
                f.cancel(rg).await?;
            }
        }
        PENDING | APPROVED => bail!("Only a request still waiting can be taken back"),
        s => bail!("That request is {s} already"),
    }
    serve.userdata.set_request_status(id, CANCELLED, None, Some(account))
}

/// The Requests page's lists, each request with its album's state.
pub async fn list(app: &super::App, serve: &Serve, account: &str) -> Result<Listing> {
    let may = rights(serve);
    let mine = serve.userdata.requests(Some(account), None, LIST)?;
    let waiting = if may.manage_requests { Some(serve.userdata.requests(None, Some(PENDING), LIST)?) } else { None };
    let (f, lib) = (fetcher(app), library(serve).await);
    let mine = views(f.as_deref(), lib.as_deref(), mine).await;
    let waiting = match waiting {
        Some(w) => Some(views(f.as_deref(), lib.as_deref(), w).await),
        None => None,
    };
    Ok(Listing { mine, waiting, may, fetcher: f.is_some() })
}

/// The account's requests by their suggestion's key, for Discover's cards: the newest of each album, with its state.
pub async fn of_suggestions(app: &super::App, serve: &Serve, account: &str, keys: &[String]) -> Result<Vec<(String, View)>> {
    let mut newest: Vec<Request> = Vec::new();
    for r in serve.userdata.requests(Some(account), None, LIST)? {
        if keys.contains(&r.album_key) && !newest.iter().any(|n| n.album_key == r.album_key) {
            newest.push(r);
        }
    }
    // None of these asked for: the library is not read (0.11.1: /api/discover's memory, 2026-10-30).
    if newest.is_empty() {
        return Ok(Vec::new());
    }
    let (f, lib) = (fetcher(app), library(serve).await);
    Ok(views(f.as_deref(), lib.as_deref(), newest).await.into_iter().map(|v| (v.request.album_key.clone(), v)).collect())
}

/// What Lidarr is looking for and fetching now (the page's "Lidarr is looking for these"); asked while the page is
/// open, not watched.
pub async fn watching(app: &super::App) -> Result<Option<crate::lidarr::Watching>> {
    match super::discover::lidarr(app) {
        Some(l) => Ok(Some(l.watching(WATCHED).await?)),
        None => Ok(None),
    }
}

/// An open request's mark on an album not in the library (0.9.5, the owner: a clock while it is wanted, an arrow in
/// the progress ring while it comes; none once it is in): `wanted`, or `downloading` and how far (0 to 1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Mark {
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<f32>,
}

/// How long the marks are kept: lists ask for them as they open and as search is typed, the fetcher asked once.
const MARKS_KEEP: Duration = Duration::from_secs(30);
/// The most requests looked at for them.
const OPEN: usize = 1000;

/// A request's album state as its mark; none once it is in the library.
fn mark(state: &Album) -> Option<Mark> {
    match *state {
        Album::InLibrary => None,
        Album::Fetching { done } => Some(Mark { state: "downloading", done: Some(done.clamp(0.0, 1.0)) }),
        // Arrived, not in the library yet (Keep or Replace puts it there): still coming, all of it fetched.
        Album::InIncoming | Album::PartlyThere { .. } => Some(Mark { state: "downloading", done: Some(1.0) }),
        Album::Requested | Album::Looking | Album::Unknown => Some(Mark { state: "wanted", done: None }),
    }
}

/// Every open request's mark (pending or approved, any account's: an album someone asked for comes for the whole
/// server), by its album's key as Discover keys albums (its release group, else its artist's and its name's); one
/// coming wins over one only wanted. Kept `MARKS_KEEP`.
pub async fn marks(app: &super::App, serve: &Serve) -> Result<Arc<HashMap<String, Mark>>> {
    type Kept = Option<(Instant, Arc<HashMap<String, Mark>>)>;
    static KEPT: OnceLock<Mutex<Kept>> = OnceLock::new();
    let kept = KEPT.get_or_init(Default::default);
    if let Some((at, m)) = kept.lock().unwrap_or_else(|p| p.into_inner()).clone()
        && at.elapsed() < MARKS_KEEP
    {
        return Ok(m);
    }
    let open: Vec<Request> = serve.userdata.requests(None, None, OPEN)?.into_iter().filter(|r| r.status == PENDING || r.status == APPROVED).collect();
    let (f, lib) = (fetcher(app), library(serve).await);
    let mut out: HashMap<String, Mark> = HashMap::new();
    for v in views(f.as_deref(), lib.as_deref(), open).await {
        let Some(m) = mark(&v.album_state) else { continue };
        let e = out.entry(v.request.album_key.clone()).or_insert(m);
        if e.state == "wanted" && m.state == "downloading" {
            *e = m;
        }
    }
    let m = Arc::new(out);
    *kept.lock().unwrap_or_else(|p| p.into_inner()) = Some((Instant::now(), m.clone()));
    Ok(m)
}

/// An album's mark, by its release group, else by its artist's and its name's key (a Bandcamp album's Want).
pub fn mark_of(marks: &HashMap<String, Mark>, release_group: Option<&str>, artist: &str, album: &str) -> Option<Mark> {
    release_group
        .filter(|g| !g.is_empty())
        .and_then(|g| marks.get(g))
        .or_else(|| marks.get(&format!("{}|{}", super::import::key(artist), super::import::key(album))))
        .copied()
}

/// How many requests wait for an answer (Health, for those who answer them).
pub fn waiting(serve: &Serve) -> Result<usize> {
    serve.userdata.requests_waiting()
}

/// Requests with their albums' states: the fetcher asked once, for every approved one not in the library.
async fn views(f: Option<&dyn Fetcher>, lib: Option<&Matcher>, rs: Vec<Request>) -> Vec<View> {
    let found: Vec<bool> = rs.iter().map(|r| lib.is_some_and(|m| m.find_album(&r.album, Some(&r.artist)).is_some())).collect();
    let asked: Vec<String> =
        rs.iter().zip(&found).filter(|(r, there)| !**there && r.status == APPROVED).filter_map(|(r, _)| r.release_group.clone()).collect();
    let progress = match f {
        Some(f) if !asked.is_empty() => f.statuses(&asked).await.ok(),
        _ => None,
    };
    rs.into_iter().zip(found).map(|(r, there)| View { album_state: state(there, progress.as_ref(), &r), request: r }).collect()
}

/// Of these requests, those whose album is not in the library yet (the owner, 2026-10-30: "Once something is actually
/// in the library, there's no reason to show it in any of the requested sections except for in the requests control
/// panel").
pub async fn not_in_library(serve: &Serve, rs: Vec<Request>) -> Vec<Request> {
    let lib = library(serve).await;
    rs.into_iter().filter(|r| !lib.as_ref().is_some_and(|m| m.find_album(&r.album, Some(&r.artist)).is_some())).collect()
}

/// The library's albums by name (the import's matcher), where Cratarr's data folder holds it: its albums alone, one
/// reading shared by those asking at once and kept until the library is built again (`Matcher::library_albums`;
/// 0.11.1: /api/discover's memory, 2026-10-30: each Discover request read every song of the library here, twice, some
/// 70 MB each time on the NAS).
async fn library(serve: &Serve) -> Option<Arc<Matcher>> {
    let (dir, built) = (serve.data_dir.clone()?, serve.built_at());
    tokio::task::spawn_blocking(move || Matcher::library_albums(&dir.join("library.sqlite"), Some(built)).ok()).await.ok().flatten()
}

/// A request's album as it stands: the library first, then the fetcher's word (only for one handed to it; none when
/// there is no fetcher or it could not be asked).
fn state(in_library: bool, progress: Option<&HashMap<String, Progress>>, r: &Request) -> Album {
    if in_library {
        return Album::InLibrary;
    }
    if r.status != APPROVED {
        return Album::Requested;
    }
    let Some(p) = r.release_group.as_deref().and_then(|rg| progress?.get(rg)) else { return Album::Unknown };
    match *p {
        Progress::Fetched { tracks, of } if of > 0 && tracks < of => Album::PartlyThere { tracks, of },
        Progress::Fetched { .. } => Album::InIncoming,
        Progress::Fetching { done } => Album::Fetching { done },
        Progress::Looking => Album::Looking,
        Progress::NotWanted | Progress::Unknown => Album::Requested,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_marks_its_album_wanted_or_coming_and_none_once_in() {
        assert_eq!(mark(&Album::InLibrary), None);
        assert_eq!(mark(&Album::Looking), Some(Mark { state: "wanted", done: None }));
        assert_eq!(mark(&Album::Unknown), Some(Mark { state: "wanted", done: None }), "no fetcher, or not handed over");
        assert_eq!(mark(&Album::Fetching { done: 0.4 }), Some(Mark { state: "downloading", done: Some(0.4) }));
        assert_eq!(mark(&Album::PartlyThere { tracks: 3, of: 9 }), Some(Mark { state: "downloading", done: Some(1.0) }));
        let marks: HashMap<String, Mark> = [
            ("rg-1".to_string(), Mark { state: "wanted", done: None }),
            (format!("{}|{}", crate::app::import::key("Ott"), crate::app::import::key("Skylon")), Mark { state: "downloading", done: Some(0.5) }),
        ]
        .into();
        assert_eq!(mark_of(&marks, Some("rg-1"), "X", "Y").map(|m| m.state), Some("wanted"));
        assert_eq!(mark_of(&marks, Some("rg-2"), "OTT", "Skylon").map(|m| m.state), Some("downloading"), "by its names where its release group has none");
        assert_eq!(mark_of(&marks, None, "Ott", "Mir"), None);
    }
}
