//! Requests (decision 0070): an album asked for from Discover is kept by Cratarr with its own status (pending,
//! approved, declined, cancelled); an approved one is handed to a fetcher (`ports::Fetcher`, Lidarr first), and the
//! album's own state (being fetched, in Incoming, in the library) read beside it. Seerr's model (MIT,
//! github.com/seerr-team/seerr), not its code. Who may ask, have their asking approved at once, and answer everyone's:
//! the account's permissions (`serve::users::Permissions`), an administrator's all.

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

/// What the signed-in account may do with requests: its permissions, an administrator's all.
pub fn rights(serve: &Serve) -> Permissions {
    if serve.current_is_admin() {
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
    let may = rights(serve);
    if !may.request {
        bail!("Requesting albums is not on for this account");
    }
    let s = super::discover::kept(serve, account)?.and_then(|f| f.albums.into_iter().find(|s| s.key == key)).context("That suggestion is gone; look again")?;
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
            Err(e) => format!("Not handed over: {e:#}"),
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
    let mine = views(f.as_deref(), lib.as_ref(), mine).await;
    let waiting = match waiting {
        Some(w) => Some(views(f.as_deref(), lib.as_ref(), w).await),
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
    let (f, lib) = (fetcher(app), library(serve).await);
    Ok(views(f.as_deref(), lib.as_ref(), newest).await.into_iter().map(|v| (v.request.album_key.clone(), v)).collect())
}

/// What Lidarr is looking for and fetching now (the page's "Lidarr is looking for these"); asked while the page is
/// open, not watched.
pub async fn watching(app: &super::App) -> Result<Option<crate::lidarr::Watching>> {
    match super::discover::lidarr(app) {
        Some(l) => Ok(Some(l.watching(WATCHED).await?)),
        None => Ok(None),
    }
}

/// How many requests wait for an answer (Health, for those who answer them).
pub fn waiting(serve: &Serve) -> Result<usize> {
    serve.userdata.requests_waiting()
}

/// Requests with their albums' states, one after the other (a few, each a call or two to Lidarr).
async fn views(f: Option<&dyn Fetcher>, lib: Option<&Matcher>, rs: Vec<Request>) -> Vec<View> {
    let mut out = Vec::with_capacity(rs.len());
    for r in rs {
        let album_state = state(f, lib, &r).await;
        out.push(View { request: r, album_state });
    }
    out
}

/// The library's albums by name (the import's matcher), where Cratarr's data folder holds it.
async fn library(serve: &Serve) -> Option<Matcher> {
    let dir = serve.data_dir.clone()?;
    tokio::task::spawn_blocking(move || Matcher::read(&dir.join("library.sqlite")).ok()).await.ok().flatten()
}

/// A request's album as it stands: the library first, then the fetcher (only for one handed to it).
async fn state(f: Option<&dyn Fetcher>, lib: Option<&Matcher>, r: &Request) -> Album {
    if lib.is_some_and(|m| m.find_album(&r.album, Some(&r.artist)).is_some()) {
        return Album::InLibrary;
    }
    if r.status != APPROVED {
        return Album::Requested;
    }
    let (Some(f), Some(rg)) = (f, r.release_group.as_deref()) else { return Album::Unknown };
    match f.status(rg).await {
        Ok(Progress::Fetched { tracks, of }) if of > 0 && tracks < of => Album::PartlyThere { tracks, of },
        Ok(Progress::Fetched { .. }) => Album::InIncoming,
        Ok(Progress::Fetching { done }) => Album::Fetching { done },
        Ok(Progress::Looking) => Album::Looking,
        Ok(Progress::NotWanted | Progress::Unknown) => Album::Requested,
        Err(_) => Album::Unknown,
    }
}
