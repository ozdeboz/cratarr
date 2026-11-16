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
/// Closed by its arrival's deletion (W4, the owner's answer (c)): wanted again, it is a new request.
pub const REJECTED: &str = "rejected";
/// Why a request is rejected, as its reason says.
const REJECTED_WHY: &str = "Rejected";

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

/// A request as the pages show it: as kept, its state (W5, `request_state`: the one every view reads), and whether Hand over
/// again is open for it; and its album's state as the page has read it so far (decision 0070), from that one state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct View {
    #[serde(flatten)]
    pub request: Request,
    pub state: RequestState,
    pub album_state: Album,
    pub hand_over_again: bool,
}

/// A request's state, the one every view reads (W5, G1): its status as kept first (waiting for approval, declined, cancelled,
/// rejected), then how its record is held (W4: done, arrived, partly there), then what the fetchers say of one wanted: Lidarr's
/// progress, and Soulseek's item (searching, found and waiting for the tick, fetching and how far, or waiting and why). A
/// request's `fetched` stays the hand-over's words; its state is worked out at each read.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RequestState {
    WaitingForApproval,
    Declined,
    Cancelled,
    Rejected,
    Wanted {
        #[serde(skip_serializing_if = "Option::is_none")]
        lidarr: Option<Fetching>,
        #[serde(skip_serializing_if = "Option::is_none")]
        soulseek: Option<super::soulseek::Word>,
    },
    PartlyThere {
        has: u32,
        of: u32,
    },
    Arrived,
    Done,
}

/// What Lidarr says of a request it was handed (W5): looking for it, or fetching it and how far (0 to 1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Fetching {
    Looking,
    Fetching { done: f32 },
}

/// A request's state (W5): `RequestState`, from the request as kept, how its record is held (`holding`), Lidarr's progress for its
/// group, and Soulseek's word of its item. A request of an album the library has, or that arrived, reads so whatever its answer
/// was to be. Pure.
pub fn request_state(r: &Request, held: Holding, lidarr: Option<&Progress>, soulseek: Option<&super::soulseek::Word>) -> RequestState {
    match r.status.as_str() {
        DECLINED => return RequestState::Declined,
        CANCELLED => return RequestState::Cancelled,
        REJECTED => return RequestState::Rejected,
        _ => {}
    }
    match held {
        Holding::Done => RequestState::Done,
        Holding::Arrived => RequestState::Arrived,
        _ if r.status != APPROVED => RequestState::WaitingForApproval,
        Holding::Partly { has, of } => RequestState::PartlyThere { has, of },
        Holding::None => {
            let lidarr = lidarr.and_then(|p| match *p {
                Progress::Looking => Some(Fetching::Looking),
                Progress::Fetching { done } => Some(Fetching::Fetching { done }),
                Progress::Fetched { .. } => Some(Fetching::Fetching { done: 1.0 }),
                Progress::NotWanted | Progress::Unknown => None,
            });
            RequestState::Wanted { lidarr, soulseek: soulseek.cloned() }
        }
    }
}

/// The album state the page has read so far (decision 0070), from the one state (W5): run 9's page reads `state` itself.
fn album_state_of(s: &RequestState) -> Album {
    use super::soulseek::Word;
    match s {
        RequestState::Done => Album::InLibrary,
        RequestState::Arrived => Album::InIncoming,
        RequestState::PartlyThere { has, of } => Album::PartlyThere { tracks: *has, of: *of },
        RequestState::Wanted { lidarr: Some(Fetching::Fetching { done }), .. } => Album::Fetching { done: *done },
        RequestState::Wanted { soulseek: Some(Word::Fetching { percent }), .. } => Album::Fetching { done: (*percent / 100.0) as f32 },
        RequestState::Wanted { lidarr: Some(Fetching::Looking), .. } | RequestState::Wanted { soulseek: Some(Word::Searching | Word::Found), .. } => Album::Looking,
        RequestState::Wanted { lidarr: None, soulseek: None } => Album::Unknown,
        RequestState::Wanted { .. } | RequestState::WaitingForApproval | RequestState::Declined | RequestState::Cancelled | RequestState::Rejected => Album::Requested,
    }
}

/// Whether no fetcher has said a word of a request (W5, G2): none written, or only that it was not handed over. Hand over again is
/// open for an approved one still wanted (`decide`).
fn no_fetcher_spoke(r: &Request) -> bool {
    r.fetched.as_deref().is_none_or(|f| f.starts_with("Not handed over"))
}

/// The Requests page: the account's own requests, everyone's waiting ones for those who answer them, and what the
/// account may do.
#[derive(Debug, Clone, Serialize)]
pub struct Listing {
    pub mine: Vec<View>,
    pub waiting: Option<Vec<View>>,
    pub may: Permissions,
    /// The fetchers connected, set up and switched on (decision 0099's amendment, the owner 2026-11-03: Lidarr and
    /// Soulseek siblings, either or both): "lidarr", "soulseek". An approved request goes to each.
    pub fetchers: Vec<&'static str>,
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
    if let Some(r) = open_request(serve, account, &s.key)? {
        return Ok(r);
    }
    let record = record_of_album_in(serve, s.release_group.clone(), &s.artist, &s.album, s.year, s.cover.clone(), s.bandcamp.clone()).await?;
    asked_record(app, serve, account, record, Some(s.key), s.release_group).await
}

/// Any album asked for by its names (0.9.3, the owner: Want for any album an artist's page says is missing): keyed as
/// Discover keys albums (its release group, else its artist's and its name's), so its suggestion leaves Discover too; its
/// record found or made first (`record_of_album`, C49a), whose names the request shows.
#[allow(clippy::too_many_arguments)]
pub async fn ask_album(app: &super::App, serve: &Serve, account: &str, release_group: Option<String>, artist: &str, album: &str, year: Option<i32>, cover: Option<String>, page: Option<String>) -> Result<Request> {
    if !rights(serve).request {
        bail!("Requesting albums is not on for this account");
    }
    // An album with no title would be a record named nothing for life (W6).
    if album.trim().is_empty() {
        bail!("An album needs its title");
    }
    let release_group = release_group.filter(|g| !g.is_empty());
    let cover = cover.filter(|c| cover_kept(c));
    let key = release_group.clone().unwrap_or_else(|| format!("{}|{}", super::import::key(artist), super::import::key(album)));
    if let Some(r) = open_request(serve, account, &key)? {
        return Ok(r);
    }
    let record = record_of_album_in(serve, release_group.clone(), artist, album, year, cover, page).await?;
    asked_record(app, serve, account, record, Some(key), release_group).await
}

/// A record asked for by its id (C49a): an artist's or a label's missing record, any record not had. The account's open
/// request for it is the answer where there is one.
pub async fn ask_record(app: &super::App, serve: &Serve, account: &str, record: i64) -> Result<Request> {
    if !rights(serve).request {
        bail!("Requesting albums is not on for this account");
    }
    asked_record(app, serve, account, record, None, None).await
}

/// The request for `record` (`asked_of`), asked for and handed over.
async fn asked_record(app: &super::App, serve: &Serve, account: &str, record: i64, key: Option<String>, asked: Option<String>) -> Result<Request> {
    let cat = serve.catalogue().clone();
    let asked = tokio::task::spawn_blocking(move || asked_of(&cat, record, key, asked)).await??;
    ask_for(app, serve, account, asked).await
}

/// What a request for `record` is made of: its names, year and cover the record's; its release group the one it was asked with
/// (`group_asked`), and that group on the record too, as found by names (`search`), where the record holds none (R21, N1); its
/// key the one given (Discover's, so its suggestion leaves it), else its group, else its names'. Blocking.
fn asked_of(cat: &crate::catalogue::Catalogue, record: i64, key: Option<String>, asked: Option<String>) -> Result<Asked> {
    let row = cat.record(record)?.context("That record is gone")?;
    let held: Vec<String> = cat.references_of(&crate::serve::ids::record(row.id))?.into_iter().filter(|(r, _)| r.source == "musicbrainz" && r.kind == "release-group").map(|(r, _)| r.value).collect();
    let group = group_asked(asked, &held);
    // Never onto an owned record (W2, B2): one found by its names, where the group would make another album had.
    let owned = cat.requested(&[row.id])?.get(&row.id).is_some_and(|q| q.holder == Some(row.id));
    if held.is_empty()
        && !owned
        && let Some(g) = &group
    {
        cat.set_reference(&crate::serve::ids::record(row.id), &crate::catalogue::records::Ref::new("musicbrainz", "release-group", g), "search")?;
    }
    let key = key.or_else(|| group.clone()).unwrap_or_else(|| format!("{}|{}", super::import::key(&row.artist), super::import::key(&row.title)));
    Ok(Asked { record: row.id, key, artist_key: super::import::loose(&row.artist), release_group: group, album: row.title, artist: row.artist, year: row.year, cover: row.cover })
}

/// The release group a request names (F15, R21): the one it was asked with, whatever its record holds (a record found by names
/// holds no group against it, `request_record_in`), else the record's first. A record may hold two (a file's tag and a match that
/// disagree): the one asked for is the one wanted. Pure.
fn group_asked(asked: Option<String>, held: &[String]) -> Option<String> {
    asked.map(|g| g.to_ascii_lowercase()).filter(|g| super::discogs::is_mbid(g)).or_else(|| held.first().cloned())
}

/// The account's open request (pending or approved) for an album by its key, where there is one, among all its requests (W6).
fn open_request(serve: &Serve, account: &str, key: &str) -> Result<Option<Request>> {
    serve.userdata.open_request(account, None, key)
}

/// Whether a Want's cover is one every account may be shown (W6): Cratarr's own (`/api/covers/…`), or the Cover Art Archive's,
/// Bandcamp's, Spotify's or Discogs' images over https. Any other address is left out.
fn cover_kept(cover: &str) -> bool {
    if cover.starts_with("/api/covers/") {
        return true;
    }
    let Ok(u) = reqwest::Url::parse(cover) else { return false };
    let host = u.host_str().unwrap_or_default().to_ascii_lowercase();
    u.scheme() == "https" && ["coverartarchive.org", "archive.org", "bcbits.com", "scdn.co", "discogs.com"].iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

/// Views as a viewer may read them (W6): an administrator the fetchers' own words too; any other account the states alone, never
/// Lidarr's text (its errors name its address).
fn shown_to(mut views: Vec<View>, administering: bool) -> Vec<View> {
    if !administering {
        views.iter_mut().for_each(|v| v.request.fetched = None);
    }
    views
}

/// The record an album asked for by its names is (C49a): its names as a label's own account gives them sorted out first (S12,
/// `bandcamp::label_release`: a Want from a label's Bandcamp page, "Artist - Title (Label)" by the label, is the artist's
/// "Title", the label credited), then the record of its release group, else a record had of its names, else a new record not
/// had (`Catalogue::record_for_request`). Blocking.
pub fn record_of_album(cat: &crate::catalogue::Catalogue, release_group: Option<&str>, artist: &str, album: &str, year: Option<i32>, cover: Option<String>) -> Result<i64> {
    record_of_album_paged(cat, release_group, crate::bandcamp::label_release(artist, album, None), Vec::new(), year, cover, None)
}

/// `record_of_album`, its names sorted out by the caller, its Bandcamp album page given (R22): kept on the record as a reference
/// (`search`), so a record only Bandcamp lists is judged by Bandcamp's list.
fn record_of_album_paged(cat: &crate::catalogue::Catalogue, release_group: Option<&str>, named: crate::bandcamp::Named, artists: Vec<String>, year: Option<i32>, cover: Option<String>, page: Option<String>) -> Result<i64> {
    let wanted = crate::catalogue::records::Wanted { group: release_group.map(str::to_string), artist: named.artist, artists, title: named.title, label: named.label, year, cover, page };
    cat.record_for_request(&wanted)
}

/// `record_of_album` for a Want (R22, N2): its Bandcamp album page the card's, else the one Discover found for it by its release
/// group or by its names (`bandcamp_found`, as a page is found when an album is opened), made plain; else, once its record is
/// known, the one found by a release group the record holds (the owner's GROUCH request: 0.16 found its page by the group its card
/// knew, and it was asked for by Bandcamp's names). Where neither its group nor its page names a record yet and it has no group, its
/// record is named as Bandcamp lists the album (W1, `bandcamp::named_by_page`, read as kept): Bandcamp's bot check leaves the names
/// sent.
#[allow(clippy::too_many_arguments)]
async fn record_of_album_in(serve: &Serve, release_group: Option<String>, artist: &str, album: &str, year: Option<i32>, cover: Option<String>, page: Option<String>) -> Result<i64> {
    let (cat, ud) = (serve.catalogue().clone(), serve.userdata.clone());
    let group = release_group.filter(|g| !g.is_empty());
    let (page, known) = {
        let (cat, ud, group, artist, album) = (cat.clone(), ud.clone(), group.clone(), artist.to_string(), album.to_string());
        tokio::task::spawn_blocking(move || -> Result<(Option<String>, bool)> {
            let found = |key: &str| -> Result<Option<String>> { Ok(ud.bandcamp_found(key)?.and_then(|(_, page)| page)) };
            let page = match page.filter(|p| !p.is_empty()) {
                Some(page) => Some(page),
                None => match group.as_deref().map(found).transpose()?.flatten() {
                    Some(page) => Some(page),
                    None => found(&requested(&artist, &album))?,
                },
            }
            .and_then(|p| crate::bandcamp::album_page(&p));
            let known = cat.record_named_by(group.as_deref(), page.as_deref())?.is_some();
            Ok((page, known))
        })
        .await??
    };
    let read = match &page {
        Some(p) if !known && group.is_none() => crate::bandcamp::Bandcamp::get().album(p).await.ok(),
        _ => None,
    };
    let named = crate::bandcamp::named_by_page(read.as_ref(), artist, album);
    // Its credit split as the record pass splits a file's (W6, the library's rules).
    let artists = serve.rules().values("albumartist", std::slice::from_ref(&named.artist));
    tokio::task::spawn_blocking(move || -> Result<i64> {
        let found = |key: &str| -> Result<Option<String>> { Ok(ud.bandcamp_found(key)?.and_then(|(_, page)| page)) };
        let record = record_of_album_paged(&cat, group.as_deref(), named, artists, year, cover, page.clone())?;
        if page.is_none() {
            let refs = cat.references_of(&crate::serve::ids::record(record))?;
            if !refs.iter().any(|(r, _)| r.source == "bandcamp" && r.kind == "album")
                && let Some(page) = refs.iter().filter(|(r, _)| r.source == "musicbrainz" && r.kind == "release-group").find_map(|(r, _)| found(&r.value).transpose()).transpose()?
            {
                cat.set_reference(&crate::serve::ids::record(record), &crate::catalogue::records::Ref::new("bandcamp", "album", &page), "search")?;
            }
        }
        Ok(record)
    })
    .await?
}

/// What a request asks for: a record, by its key and its artist's, and what to show of it.
struct Asked {
    record: i64,
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
    if let Some(r) = serve.userdata.open_request(account, Some(s.record), &s.key)? {
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
        record: Some(s.record),
    };
    r.id = match serve.userdata.add_request(&r) {
        Ok(id) => id,
        // Asked twice at once (W6): the account's one open request of the record is the answer.
        Err(e) => return serve.userdata.open_request(account, Some(s.record), &r.album_key)?.ok_or(e),
    };
    if approved {
        r.fetched = Some(hand(app, serve, &r).await);
    }
    Ok(r)
}

/// The fetchers connected, Lidarr's given (`lidarr`): in the order the page lists them.
async fn connected(lidarr: bool) -> Vec<&'static str> {
    let mut out = Vec::new();
    if lidarr {
        out.push("lidarr");
    }
    if super::soulseek::connected().await {
        out.push("soulseek");
    }
    out
}

/// What a request handed to Soulseek says (`hand`).
pub const SOULSEEK_SEARCHES: &str = "Soulseek searches for it";

/// The approved requests a copy arrived for (W4): those whose record it is or whose record it holds (R13), asked before it came,
/// as R32 has it for a requester's rights; each as the pages show it (its record's group). Read before the copy goes, as its going
/// ends the holding. A copy whose coming is not known arrived for none.
pub async fn arrived_for(serve: &Serve, record: i64, came: Option<i64>) -> Vec<Request> {
    let Some(came) = came else { return Vec::new() };
    let approved = serve.userdata.requests(None, Some(APPROVED), OPEN).unwrap_or_default();
    let of = named(serve, &approved).await;
    approved
        .into_iter()
        .zip(of)
        .filter(|(r, o)| r.at <= came && o.as_ref().is_some_and(|o| o.record == record || o.holder == Some(record)))
        .map(|(r, o)| shown(r, o.as_ref()))
        .collect()
}

/// Requests whose arrival was deleted from Incoming, closed (W4, the owner's answer (c)): each "rejected", its reason "Rejected";
/// Lidarr told to stop by its record's group, as far as it can be (a failure is said in the log); Soulseek woken, which drops its
/// item unless it is under way. Wanted again, it is a new request.
pub async fn reject(lidarr: Option<&dyn Fetcher>, serve: &Serve, rs: &[Request]) {
    for r in rs {
        match serve.userdata.set_request_status(r.id, APPROVED, REJECTED, Some(REJECTED_WHY), None) {
            Ok(true) => {}
            // Answered otherwise meanwhile (W6): as it is.
            Ok(false) => continue,
            Err(e) => {
                tracing::warn!("request {}: not closed as rejected: {e:#}", r.id);
                continue;
            }
        }
        if let (Some(f), Some(group)) = (lidarr, r.release_group.as_deref())
            && let Err(e) = f.cancel(group).await
        {
            tracing::warn!("request {}: Lidarr not told to stop: {e:#}", r.id);
        }
    }
    if !rs.is_empty() {
        super::soulseek::wake();
    }
}

/// What a request of a record the library has says, handed to no fetcher (W3).
pub const ALREADY_HAD: &str = "Already in the library";

/// An approved request handed to every fetcher connected (decision 0099's amendment), as its record is named and referenced
/// (`shown`, C51b): Lidarr told to want it; Soulseek woken to take it in (it reads the approved Requests itself, each judged
/// by its record's list; one with none says so on its row). What each said (or why it could not), kept on the request. A
/// release group Lidarr found by the record's names is kept on the record, as found by names (`search`): its state can be
/// read from then on.
async fn hand(app: &super::App, serve: &Serve, r: &Request) -> String {
    let lidarr = fetcher(app);
    let soulseek = super::soulseek::connected().await;
    hand_to(lidarr.as_deref(), soulseek, serve, r).await
}

/// [`hand`] to Lidarr where it is connected (the tests' stands in for it), and to Soulseek where it is.
async fn hand_to(lidarr: Option<&dyn Fetcher>, soulseek: bool, serve: &Serve, r: &Request) -> String {
    let of = named(serve, std::slice::from_ref(r)).await.pop().flatten();
    let r = shown(r.clone(), of.as_ref());
    // Its record had (W3, F4): no fetcher's to fetch, so neither is asked (Lidarr would fetch it again where its own folder no
    // longer holds it, and Soulseek takes no item of it, R26). It reads as had; C52 makes the asker's copy of it (OQ3).
    if of.as_ref().is_some_and(RecordOf::had) {
        if let Err(e) = serve.userdata.set_request_fetched(r.id, ALREADY_HAD) {
            tracing::warn!("request {}: what the fetchers were not told was not kept: {e:#}", r.id);
        }
        return ALREADY_HAD.to_string();
    }
    let mut said = Vec::new();
    if let Some(f) = lidarr {
        said.push(match f.want(r.release_group.as_deref(), &r.artist, &r.album).await {
            Ok(handed) => {
                if r.release_group.is_none()
                    && let Some(record) = r.record
                {
                    let (cat, group) = (serve.catalogue().clone(), handed.release_group.to_ascii_lowercase());
                    let kept = tokio::task::spawn_blocking(move || cat.set_reference(&crate::serve::ids::record(record), &crate::catalogue::records::Ref::new("musicbrainz", "release-group", &group), "search")).await;
                    if let Err(e) = kept.map_err(anyhow::Error::from).and_then(|k| k) {
                        tracing::warn!("request {}: its release group was not kept: {e:#}", r.id);
                    }
                }
                handed.said
            }
            Err(e) => {
                // In the log too (0.9.6: a Lidarr timeout the owner saw left no trace to follow).
                tracing::warn!("request {}: not handed to Lidarr: {e:#}", r.id);
                format!("Not handed over to Lidarr: {e:#}")
            }
        });
    }
    if soulseek {
        super::soulseek::wake();
        said.push(SOULSEEK_SEARCHES.to_string());
    }
    let said = if said.is_empty() { "Not handed over: no fetcher is connected, or they are switched off".to_string() } else { said.join("; ") };
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
        let retry = approve && r.status == APPROVED && no_fetcher_spoke(&r);
        if r.status != PENDING && !retry {
            out.push((id, format!("Already {}", r.status)));
            continue;
        }
        // Written where it is still pending (W6): another's answer meanwhile wins.
        if approve {
            if !retry && !serve.userdata.set_request_status(id, PENDING, APPROVED, None, Some(by))? {
                out.push((id, "Answered meanwhile".to_string()));
                continue;
            }
            out.push((id, hand(app, serve, &r).await));
        } else if serve.userdata.set_request_status(id, PENDING, DECLINED, reason, Some(by))? {
            out.push((id, "Declined".to_string()));
        } else {
            out.push((id, "Answered meanwhile".to_string()));
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
            // The release group its record holds (`shown`): the one Lidarr was told to want.
            let group = shown(r.clone(), named(serve, std::slice::from_ref(&r)).await.pop().flatten().as_ref()).release_group;
            if let (Some(f), Some(rg)) = (fetcher(app), group.as_deref()) {
                f.cancel(rg).await?;
            }
        }
        PENDING | APPROVED => bail!("Only a request still waiting can be taken back"),
        s => bail!("That request is {s} already"),
    }
    if !serve.userdata.set_request_status(id, &r.status, CANCELLED, None, Some(account))? {
        bail!("That request was answered meanwhile");
    }
    Ok(())
}

/// The Requests page's lists, each request with its album's state.
pub async fn list(app: &super::App, serve: &Serve, account: &str) -> Result<Listing> {
    let may = rights(serve);
    let mine = serve.userdata.requests(Some(account), None, LIST)?;
    let waiting = if may.manage_requests { Some(serve.userdata.requests(None, Some(PENDING), LIST)?) } else { None };
    let f = fetcher(app);
    let mine = views(serve, f.as_deref(), mine).await;
    let waiting = match waiting {
        Some(w) => Some(views(serve, f.as_deref(), w).await),
        None => None,
    };
    let fetchers = connected(f.is_some()).await;
    let administering = crate::serve::access::administering();
    Ok(Listing { mine: shown_to(mine, administering), waiting: waiting.map(|w| shown_to(w, administering)), may, fetchers })
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
    let f = fetcher(app);
    Ok(shown_to(views(serve, f.as_deref(), newest).await, crate::serve::access::administering()).into_iter().map(|v| (v.request.album_key.clone(), v)).collect())
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

/// Every open request's mark (pending or approved, any account's: an album someone asked for comes for the whole server), by
/// its record and by what names its record (C49b, `marks_of`); one coming wins over one only wanted. Kept `MARKS_KEEP`.
pub async fn marks(app: &super::App, serve: &Serve) -> Result<Arc<Marks>> {
    type Kept = Option<(Instant, Arc<Marks>)>;
    static KEPT: OnceLock<Mutex<Kept>> = OnceLock::new();
    let kept = KEPT.get_or_init(Default::default);
    if let Some((at, m)) = kept.lock().unwrap_or_else(|p| p.into_inner()).clone()
        && at.elapsed() < MARKS_KEEP
    {
        return Ok(m);
    }
    let open: Vec<Request> = serve.userdata.requests(None, None, OPEN)?.into_iter().filter(|r| r.status == PENDING || r.status == APPROVED).collect();
    let of = named(serve, &open).await;
    let views = views_of(fetcher(app).as_deref(), open, &of).await;
    let m = Arc::new(marks_of(&views, &of));
    *kept.lock().unwrap_or_else(|p| p.into_inner()) = Some((Instant::now(), m.clone()));
    Ok(m)
}

/// The open requests' marks (C49b): by the record each asks for (merges followed), and by what names that record for lists
/// that have no record id (Discover's cards, Spotify's albums, search's "beyond"): its release groups, its names (`requested`)
/// and the key it was asked by (Discover's). One coming wins over one only wanted.
#[derive(Debug, Clone, Default)]
pub struct Marks {
    by_record: HashMap<i64, Mark>,
    by_key: HashMap<String, Mark>,
}

impl Marks {
    /// A record's mark, by the id it answers by now (`Catalogue::record` follows a merge).
    #[allow(dead_code)] // the pages of records not had (C53a, C56)
    pub fn of_record(&self, record: i64) -> Option<Mark> {
        self.by_record.get(&record).copied()
    }

    /// Tests elsewhere: marks by key alone, as lists with no record id find them.
    #[cfg(test)]
    pub fn by_key(by_key: HashMap<String, Mark>) -> Self {
        Self { by_key, by_record: HashMap::new() }
    }
}

fn put<K: std::hash::Hash + Eq>(map: &mut HashMap<K, Mark>, key: K, m: Mark) {
    let e = map.entry(key).or_insert(m);
    if e.state == "wanted" && m.state == "downloading" {
        *e = m;
    }
}

/// `views`' marks, each request's record as `records_of` read it (`of`, in the views' order). Pure.
fn marks_of(views: &[View], of: &[Option<RecordOf>]) -> Marks {
    let mut marks = Marks::default();
    for (v, o) in views.iter().zip(of) {
        let Some(m) = mark(&v.album_state) else { continue };
        put(&mut marks.by_key, v.request.album_key.clone(), m);
        if let Some(o) = o {
            put(&mut marks.by_record, o.record, m);
            for g in &o.groups {
                put(&mut marks.by_key, g.clone(), m);
            }
            put(&mut marks.by_key, requested(&o.artist, &o.title), m);
        }
    }
    marks
}

/// An album's key by its names: its artist's and its title's.
pub fn requested(artist: &str, album: &str) -> String {
    format!("{}|{}", super::import::key(artist), super::import::key(album))
}

/// Who asked for each approved request, by its record (C49b, merges followed) and by the record that holds its files where that
/// is another (R13: an album that came as a record of its own): an Incoming record goes into its requester's own music by
/// default, and is theirs to add or delete (0101 point 2), where it came after they asked (R32, `Requester::of_copy`). The newest
/// asker of a record. Kept `MARKS_KEEP`, as the marks are (R30, N15): the Albums page asks at every open, and a read works the had
/// rule out for every approved request's record.
pub fn requesters(serve: &Serve) -> Arc<HashMap<i64, Requester>> {
    let kept = serve.requesters_kept().0.clone();
    if let Some((at, who)) = kept
        && at.elapsed() < MARKS_KEEP
    {
        return who;
    }
    let who = Arc::new(requesters_now(serve));
    serve.requesters_kept().0 = Some((Instant::now(), who.clone()));
    who
}

/// Who asked for what, as last read, and when (`requesters`): on the server, so each server of the tests has its own.
#[derive(Debug, Default)]
pub struct KeptRequesters(Option<(Instant, Arc<HashMap<i64, Requester>>)>);

/// Who asked for a record, and when (`requesters`).
#[derive(Debug, Clone, PartialEq)]
pub struct Requester {
    pub user: String,
    /// When they asked, epoch seconds.
    pub at: i64,
}

impl Requester {
    /// Whether a copy that came at `came` is theirs to keep into their own music or to delete: it came after they asked (R32,
    /// N14, the owner's). One the library held already (an upgrade the administrator's Lidarr fetched, an album that was there
    /// when they asked), or whose coming is not known, stays an administrator's.
    pub fn of_copy(&self, came: Option<i64>) -> bool {
        came.is_some_and(|came| came >= self.at)
    }
}

/// `requesters`, read now.
fn requesters_now(serve: &Serve) -> HashMap<i64, Requester> {
    let approved = serve.userdata.requests(None, Some(APPROVED), OPEN).unwrap_or_default();
    let of = records_held(serve.catalogue(), &approved, &serve.library.kinds()).unwrap_or_else(|e| {
        tracing::debug!("the requests' records not read: {e:#}");
        vec![None; approved.len()]
    });
    approved.into_iter().zip(of).rev().filter_map(|(r, o)| Some((o?, Requester { user: r.user, at: r.at }))).flat_map(|(o, who)| o.holder.into_iter().chain([o.record]).map(move |id| (id, who.clone()))).collect()
}

/// An album's mark where a list has no record id for it: by its release group, else by its artist's and its name's key (a
/// Bandcamp album's Want), as `marks_of` keys them.
pub fn mark_of(marks: &Marks, release_group: Option<&str>, artist: &str, album: &str) -> Option<Mark> {
    release_group.filter(|g| !g.is_empty()).and_then(|g| marks.by_key.get(g)).or_else(|| marks.by_key.get(&requested(artist, album))).copied()
}

/// What a request's record says now (C49b): the id it answers by (a merge followed), its release groups, its names, year and
/// cover, and the record that holds its files where it is had (R13: the one rule for "had", `Catalogue::holders`): itself, or an
/// album that came as a record of its own.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecordOf {
    pub record: i64,
    pub groups: Vec<String>,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub cover: Option<String>,
    pub holder: Option<i64>,
    /// How it is held (W4): partly, arrived (in Incoming alone), or done.
    pub holding: Holding,
}

impl RecordOf {
    /// Whether its album is delivered (W4): a whole copy arrived in Incoming, or in a library. A copy short of its list is not: it
    /// is searched and fetched still.
    pub fn had(&self) -> bool {
        matches!(self.holding, Holding::Arrived | Holding::Done)
    }
}

/// How a request's record is held (W4, the owner's answers (b) and (d): one "had" for requests and fetchers): no copy there (none,
/// or only in the trash); a copy short of its list ("partly there": searched and fetched still, its request open); a whole copy in
/// Incoming alone ("arrived": delivered, until it is added); a whole copy in a main library or someone's own music ("done").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Holding {
    #[default]
    None,
    Partly {
        has: u32,
        of: u32,
    },
    Arrived,
    Done,
}

/// How a record read for its requests is held (`Holding`), each catalogue folder's library as `kind` says: a copy is whole where it
/// holds every track of the list it must hold (`Requested::list`; with none, any copy is). Tracks no file holds, as the tests make
/// them, are one copy kept. Pure.
pub fn holding_of(q: &crate::catalogue::records::Requested, kind: impl Fn(i64) -> crate::settings::LibraryKind) -> Holding {
    if q.holder.is_none() || q.present == 0 {
        return Holding::None;
    }
    let whole = |n: usize| q.list.is_none_or(|l| n >= l);
    let partly = Holding::Partly { has: q.present as u32, of: q.list.unwrap_or(q.present) as u32 };
    if q.copies.is_empty() {
        return if whole(q.present) { Holding::Done } else { partly };
    }
    let whole_in: Vec<crate::settings::LibraryKind> = q.copies.iter().filter(|(_, n)| whole(*n)).map(|(folder, _)| kind(*folder)).collect();
    if whole_in.is_empty() {
        partly
    } else if whole_in.iter().any(|k| *k != crate::settings::LibraryKind::Incoming) {
        Holding::Done
    } else {
        Holding::Arrived
    }
}

/// What each catalogue folder's library is, as the running server holds them; none where there is no server (a test), and
/// `kind_in`'s own rule holds: the first folder Music, the others Incoming.
fn library_kinds() -> Vec<(i64, crate::settings::LibraryKind)> {
    crate::serve::instance().map(|s| s.library.kinds()).unwrap_or_default()
}

/// Each request's record as the catalogue holds it now, in the requests' order: none where it names no record yet (before
/// 0.17's step) or one that is gone. All read at once on one reader, never under the writer's lock (R16,
/// `Catalogue::requested`): the Albums page and the marks ask for every open request while builds run. Blocking.
pub fn records_of(cat: &crate::catalogue::Catalogue, rs: &[Request]) -> Result<Vec<Option<RecordOf>>> {
    records_held(cat, rs, &library_kinds())
}

/// `records_of`, each catalogue folder's library as `kinds` has it (`Serve`'s library), for how each record is held (W4).
pub fn records_held(cat: &crate::catalogue::Catalogue, rs: &[Request], kinds: &[(i64, crate::settings::LibraryKind)]) -> Result<Vec<Option<RecordOf>>> {
    let read = cat.requested(&rs.iter().filter_map(|r| r.record).collect::<Vec<_>>())?;
    let kind = |folder: i64| crate::serve::library::kind_in(kinds, folder).0;
    Ok(rs
        .iter()
        .map(|r| {
            let q = read.get(&r.record?)?;
            let holding = holding_of(q, kind);
            Some(RecordOf { record: q.row.id, groups: q.groups.clone(), title: q.row.title.clone(), artist: q.row.artist.clone(), year: q.row.year, cover: q.row.cover.clone(), holder: q.holder, holding })
        })
        .collect())
}

/// `records_of` from an async caller; none for each where the catalogue cannot be read (logged).
async fn named(serve: &Serve, rs: &[Request]) -> Vec<Option<RecordOf>> {
    let (cat, rs2, kinds) = (serve.catalogue().clone(), rs.to_vec(), serve.library.kinds());
    match tokio::task::spawn_blocking(move || records_held(&cat, &rs2, &kinds)).await {
        Ok(Ok(of)) => of,
        Ok(Err(e)) => {
            tracing::debug!("the requests' records not read: {e:#}");
            vec![None; rs.len()]
        }
        Err(e) => {
            tracing::debug!("the requests' records not read: {e}");
            vec![None; rs.len()]
        }
    }
}

/// A request as the pages show it (C49b): its record's names, year, cover and release group in place of those it was asked
/// with (of the record's groups, the one the request names, F15), and the id its record answers by, where it has a record.
fn shown(mut r: Request, of: Option<&RecordOf>) -> Request {
    if let Some(o) = of {
        r.album = o.title.clone();
        r.artist = o.artist.clone();
        r.year = o.year.or(r.year);
        r.cover = o.cover.clone().or(r.cover);
        let own = r.release_group.take();
        r.release_group = own.clone().filter(|g| o.groups.iter().any(|x| x.eq_ignore_ascii_case(g))).or_else(|| o.groups.first().cloned()).or(own);
        r.record = Some(o.record);
    }
    r
}

/// The requests waiting for an answer (Health, for those who answer them, W5): pending ones whose state says so, each as the pages
/// show it; one whose album the library has meanwhile waits for no one.
pub fn waiting(serve: &Serve) -> Result<Vec<Request>> {
    let pending = serve.userdata.requests(None, Some(PENDING), OPEN)?;
    let of = records_held(serve.catalogue(), &pending, &serve.library.kinds())?;
    Ok(pending
        .into_iter()
        .zip(of)
        .filter(|(r, o)| request_state(r, o.as_ref().map_or(Holding::None, |o| o.holding), None, None) == RequestState::WaitingForApproval)
        .map(|(r, o)| shown(r, o.as_ref()))
        .collect())
}

/// Requests as the pages show them, with their albums' states (C49b): each by its record (`shown`), in the library where its
/// record is had; the fetcher asked once, for every approved one not had.
async fn views(serve: &Serve, f: Option<&dyn Fetcher>, rs: Vec<Request>) -> Vec<View> {
    let of = named(serve, &rs).await;
    views_of(f, rs, &of).await
}

/// `views`, each request's record read (`of`, in the requests' order).
async fn views_of(f: Option<&dyn Fetcher>, rs: Vec<Request>, of: &[Option<RecordOf>]) -> Vec<View> {
    let rs: Vec<(Request, Holding)> = rs.into_iter().zip(of).map(|(r, o)| (shown(r, o.as_ref()), o.as_ref().map_or(Holding::None, |o| o.holding))).collect();
    let asked: Vec<String> = rs.iter().filter(|(r, held)| !matches!(held, Holding::Arrived | Holding::Done) && r.status == APPROVED).filter_map(|(r, _)| r.release_group.clone()).collect();
    let progress = match f {
        Some(f) if !asked.is_empty() => f.statuses(&asked).await.ok(),
        _ => None,
    };
    let soulseek = super::soulseek::item_words(&rs.iter().map(|(r, _)| r.id).collect::<Vec<_>>());
    rs.into_iter()
        .map(|(r, held)| {
            let lidarr = r.release_group.as_deref().and_then(|g| progress.as_ref()?.get(g));
            let state = request_state(&r, held, lidarr, soulseek.get(&r.id));
            View { album_state: album_state_of(&state), hand_over_again: matches!(state, RequestState::Wanted { .. }) && no_fetcher_spoke(&r), state, request: r }
        })
        .collect()
}

/// Of these requests, those whose record is not had yet (the owner, 2026-10-30: "Once something is actually in the library,
/// there's no reason to show it in any of the requested sections except for in the requests control panel"), each as the
/// pages show it (`shown`, C49b).
pub async fn not_in_library(serve: &Serve, rs: Vec<Request>) -> Vec<Request> {
    let of = named(serve, &rs).await;
    rs.into_iter()
        .zip(of)
        .filter(|(r, o)| request_state(r, o.as_ref().map_or(Holding::None, |o| o.holding), None, None) != RequestState::Done)
        .map(|(r, o)| shown(r, o.as_ref()))
        .collect()
}

/// The library's albums by name (the import's matcher), where Cratarr's data folder holds it: its albums alone, one
/// reading shared by those asking at once and kept until the library is built again (`Matcher::library_albums`;
/// 0.11.1: /api/discover's memory, 2026-10-30: each Discover request read every song of the library here, twice, some
/// 70 MB each time on the NAS).
pub(crate) async fn library(serve: &Serve) -> Option<Arc<Matcher>> {
    let (dir, built) = (serve.data_dir.clone()?, serve.built_at());
    tokio::task::spawn_blocking(move || Matcher::library_albums(&dir.join("library.sqlite"), Some(built)).ok()).await.ok().flatten()
}



#[cfg(test)]
mod tests {
    use super::*;

    /// C49b: an open request's mark is found by the record it asks for (the one a merge made it, too), by what names that
    /// record (its release group, its names), and by the names it was asked with; and the request shows its record's names.
    #[test]
    fn a_mark_finds_its_record() {
        let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
        let group = "00000000-0000-4000-8000-000000000130";
        let record = record_of_album(&cat, Some(group), "Paper Lantern", "Tape Club - After Hours (Paper Lantern)", Some(2020), None).unwrap();
        let asked = cat.record_for_tests("After Hours (a copy asked for)", 0);
        cat.merge_records(asked, record).unwrap();
        let r = Request {
            id: 1,
            user: "ann".into(),
            album_key: requested("Paper Lantern", "Tape Club - After Hours (Paper Lantern)"),
            artist_key: "paper lantern".into(),
            release_group: None,
            album: "Tape Club - After Hours (Paper Lantern)".into(),
            artist: "Paper Lantern".into(),
            year: None,
            cover: None,
            at: 1,
            status: APPROVED.into(),
            reason: None,
            decided_by: None,
            decided_at: None,
            fetched: None,
            record: Some(asked),
        };
        let of = records_of(&cat, std::slice::from_ref(&r)).unwrap();
        assert_eq!(of[0].as_ref().map(|o| o.record), Some(record), "merged: the record it went into");
        let views = vec![View { request: shown(r, of[0].as_ref()), state: RequestState::WaitingForApproval, album_state: Album::Requested, hand_over_again: false }];
        assert_eq!((views[0].request.artist.as_str(), views[0].request.album.as_str(), views[0].request.release_group.as_deref()), ("Tape Club", "After Hours", Some(group)));
        let marks = marks_of(&views, &of);
        let wanted = Some(Mark { state: "wanted", done: None });
        assert_eq!(marks.of_record(record), wanted);
        assert_eq!(mark_of(&marks, Some(group), "", ""), wanted, "by its record's group");
        assert_eq!(mark_of(&marks, None, "Tape Club", "After Hours"), wanted, "by its record's names");
        assert_eq!(mark_of(&marks, None, "Paper Lantern", "Tape Club - After Hours (Paper Lantern)"), wanted, "by the names it was asked with");
        assert_eq!(mark_of(&marks, None, "Tape Club", "Before Hours"), None);
    }

    /// C49a, S12: a Want from a label's Bandcamp album page, which names the label's own account as the artist and the album
    /// "Artist - Title (Label)", makes the artist's record of that title, the label credited; another Want by its release
    /// group finds the record of the group.
    #[test]
    fn a_want_from_a_labels_album_page_names_the_artist() {
        let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
        let record = record_of_album(&cat, None, "Paper Lantern", "Tape Club - After Hours (Paper Lantern)", Some(2020), None).unwrap();
        let row = cat.record(record).unwrap().unwrap();
        assert_eq!((row.artist.as_str(), row.title.as_str(), row.year, row.had), ("Tape Club", "After Hours", Some(2020), false));
        assert_eq!(cat.record_artists(record).unwrap(), [cat.artist_of("Tape Club").unwrap().unwrap()]);
        assert_eq!(cat.label_records(cat.label_for("Paper Lantern").unwrap()).unwrap(), [(record, String::new())]);
        assert_eq!(cat.artist_of("Paper Lantern").unwrap(), None, "the account is no artist");
        let mbid = "00000000-0000-4000-8000-000000000130";
        let by_group = record_of_album(&cat, Some(mbid), "Orbit Lane", "Undertow", None, None).unwrap();
        assert_eq!(record_of_album(&cat, Some(mbid), "Orbit Lane", "Undertow (Deluxe)", None, None).unwrap(), by_group, "its group finds it");
    }

    /// A request of bob's, approved, for `record`.
    fn asked_for(record: i64) -> Request {
        Request {
            id: 0,
            user: "bob".into(),
            album_key: "k".into(),
            artist_key: "a".into(),
            release_group: None,
            album: "Asked".into(),
            artist: "Asker".into(),
            year: None,
            cover: None,
            at: 1,
            status: APPROVED.into(),
            reason: None,
            decided_by: None,
            decided_at: None,
            fetched: None,
            record: Some(record),
        }
    }

    /// R13 (F1): an album that arrived untagged is a record of its own, and its request's record stays not had, until its group
    /// is known on the arrival (its tags since, a match, Lidarr): from then on the request's record is had, by the arrival, and
    /// its album is in the library.
    #[tokio::test]
    async fn an_untagged_arrival_completes_its_request_once_its_group_is_known() {
        let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
        let group = "00000000-0000-4000-8000-000000000140";
        let asked = record_of_album(&cat, Some(group), "Orbit Lane", "Undertow", Some(2018), None).unwrap();
        let arrived = cat.record_for_tests("Untitled", 2);
        let rs = vec![asked_for(asked)];
        let holder = |cat: &crate::catalogue::Catalogue| records_of(cat, &rs).unwrap()[0].as_ref().map(|o| (o.record, o.holder));
        assert_eq!(holder(&cat), Some((asked, None)), "not had");
        assert!(cat.set_reference(&crate::serve::ids::record(arrived), &crate::catalogue::records::Ref::new("musicbrainz", "release-group", group), "likely").unwrap());
        assert_eq!(holder(&cat), Some((asked, Some(arrived))), "had, by the arrival");
        let of = records_of(&cat, &rs).unwrap();
        assert_eq!(views_of(None, rs.clone(), &of).await[0].album_state, Album::InLibrary);
    }

    /// R13 (F1): a requested album that came as a record of its own, by its artist and title (an untagged copy kept from Incoming
    /// into Music), is in the library and leaves the requested lists; where the two hold one source's same kind with no value in
    /// common they are two records, and it stays requested.
    #[tokio::test]
    async fn a_requested_album_kept_into_music_leaves_the_requested_lists() {
        let serve = served();
        let cat = serve.catalogue();
        let asked = record_of_album(cat, None, "Glass Harbour", "Tide Tables", Some(2012), None).unwrap();
        let rs = vec![asked_for(asked)];
        assert_eq!(not_in_library(&serve, rs.clone()).await.len(), 1, "nothing of it had yet");
        let kept = cat.record_for_tests("Tide Tables", 2);
        assert!(not_in_library(&serve, rs.clone()).await.is_empty(), "in the library");
        let of = records_of(cat, &rs).unwrap();
        let views = views_of(None, rs.clone(), &of).await;
        assert_eq!((of[0].as_ref().and_then(|o| o.holder), &views[0].album_state), (Some(kept), &Album::InLibrary));
        assert_eq!(marks_of(&views, &of).of_record(asked), None, "no mark");
        let page = |record: i64, url: &str| assert!(cat.set_reference(&crate::serve::ids::record(record), &crate::catalogue::records::Ref::new("bandcamp", "album", url), "link").unwrap());
        page(asked, "https://glassharbour.bandcamp.com/album/tide-tables");
        page(kept, "https://glassharbour.bandcamp.com/album/tide-tables-live");
        assert_eq!(not_in_library(&serve, rs).await.len(), 1, "two records");
    }

    /// R13 (F1): the requester of an album that came as a record of its own (C50 did not join it to the request's record) is still
    /// its requester, by the record that holds the request's record's files: its Incoming copy goes into the requester's own music.
    #[test]
    fn the_requester_of_an_arrival_not_joined_is_still_its_requester() {
        let serve = served();
        let asked = record_of_album(serve.catalogue(), None, "Glass Harbour", "Two Rooms", Some(2019), None).unwrap();
        let arrived = serve.catalogue().record_for_tests("Two Rooms", 2);
        let mut r = asked_for(asked);
        r.id = serve.userdata.add_request(&r).unwrap();
        let who = requesters(&serve);
        assert_eq!((who.get(&asked).map(|r| r.user.as_str()), who.get(&arrived).map(|r| r.user.as_str())), (Some("bob"), Some("bob")));
    }

    /// R16 (F5): who asked for what is read on one reader, never under the writer's lock: while a build holds it, the Albums
    /// page's requesters still answer, the catalogue opened once for all the requests.
    #[test]
    fn requesters_read_the_catalogue_once() {
        let dir = std::env::temp_dir().join(format!("cratarr-requesters-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let serve = Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(Arc::new(crate::catalogue::Catalogue::open(Some(&dir.join("catalogue.sqlite")), Vec::new()).unwrap()));
        for title in ["Tide Tables", "Two Rooms", "Quiet Lanes"] {
            let record = record_of_album(serve.catalogue(), None, "Glass Harbour", title, None, None).unwrap();
            serve.userdata.add_request(&asked_for(record)).unwrap();
        }
        let opened = serve.catalogue().readers_for_tests();
        let build = serve.catalogue().writer_for_tests();
        let (tx, rx) = std::sync::mpsc::channel();
        let s = serve.clone();
        std::thread::spawn(move || tx.send(requesters(&s)).ok());
        let who = rx.recv_timeout(Duration::from_secs(5)).expect("answered while a build holds the writer");
        drop(build);
        assert_eq!(who.len(), 3);
        assert_eq!(serve.catalogue().readers_for_tests() - opened, 1, "one reader for them all");
        drop(serve);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// F15: a request names the release group it was asked with where its record holds several, not the first in value order;
    /// one the record does not hold gives way to the record's.
    #[test]
    fn a_request_keeps_the_group_it_was_asked_with() {
        let (first, second) = ("00000000-0000-4000-8000-000000000150".to_string(), "00000000-0000-4000-8000-000000000151".to_string());
        let both = [first.clone(), second.clone()];
        assert_eq!(group_asked(Some(second.to_uppercase()), &both), Some(second.clone()));
        assert_eq!(group_asked(None, &both), Some(first.clone()), "none asked: the record's first");
        assert_eq!(group_asked(Some("00000000-0000-4000-8000-000000000159".into()), &both), Some("00000000-0000-4000-8000-000000000159".into()), "the one asked, always (R21)");
        assert_eq!(group_asked(Some(second.clone()), &[]), Some(second), "a record holding none: the one asked");
    }

    /// R21 (N1): a request keeps the release group it was asked for, and its record, holding none, takes it as found by names
    /// (`search`); a record holding another keeps its own, and the request still names the one asked.
    #[test]
    fn a_want_keeps_the_release_group_it_asked_for() {
        let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
        let (asked, other) = ("00000000-0000-4000-8000-000000000204", "00000000-0000-4000-8000-000000000205");
        let bare = cat.record_for_tests("Undertow", 0);
        let request = asked_of(&cat, bare, None, Some(asked.to_uppercase())).unwrap();
        assert_eq!((request.release_group.as_deref(), request.key.as_str()), (Some(asked), asked));
        let refs = cat.references_of(&crate::serve::ids::record(bare)).unwrap();
        assert!(refs.iter().any(|(r, row)| (r.value.as_str(), row.how.as_str()) == (asked, "search")), "{refs:?}");
        let holding = cat.record_for_tests("Quiet Lanes", 0);
        assert!(cat.set_reference(&crate::serve::ids::record(holding), &crate::catalogue::records::Ref::new("musicbrainz", "release-group", other), "link").unwrap());
        assert_eq!(asked_of(&cat, holding, None, Some(asked.into())).unwrap().release_group.as_deref(), Some(asked));
    }

    /// F15: Lidarr is asked with the release group the request names, where its record holds several.
    #[tokio::test]
    async fn lidarr_is_asked_with_the_group_the_request_names() {
        let (first, second) = ("00000000-0000-4000-8000-000000000150", "00000000-0000-4000-8000-000000000151");
        let serve = served();
        let record = record_of_album(serve.catalogue(), Some(second), "Glass Harbour", "Two Rooms", None, None).unwrap();
        assert!(serve.catalogue().set_reference(&crate::serve::ids::record(record), &crate::catalogue::records::Ref::new("musicbrainz", "release-group", first), "files").unwrap());
        let r = Request { release_group: Some(second.to_string()), ..asked_for(record) };
        let lidarr = Wanting::default();
        hand_to(Some(&lidarr), false, &serve, &r).await;
        assert_eq!(lidarr.0.lock().unwrap()[0].0.as_deref(), Some(second));
    }

    /// R22 (N2): a Want of a record Bandcamp alone lists, made since 0.17, keeps its page: the one the card sends, else the one
    /// Discover found for it by its names, else by the release group its record holds; so Soulseek judges it by Bandcamp's list.
    #[tokio::test]
    async fn a_bandcamp_want_made_now_is_judged_by_its_page() {
        let serve = served();
        let page_of = |record: i64| serve.catalogue().references_of(&crate::serve::ids::record(record)).unwrap().into_iter().find(|(r, _)| r.source == "bandcamp").map(|(r, _)| r.value);
        let sent = record_of_album_in(&serve, None, "Paper Lantern", "Tape Club - Neon (Paper Lantern)", None, None, Some("https://paperlantern.bandcamp.com/album/neon".into())).await.unwrap();
        assert_eq!(page_of(sent).as_deref(), Some("https://paperlantern.bandcamp.com/album/neon"), "the card's");
        serve.userdata.set_bandcamp_found(&requested("Orbit Lane", "Undertow"), 1, Some("https://orbitlane.bandcamp.com/album/undertow")).unwrap();
        let by_names = record_of_album_in(&serve, None, "Orbit Lane", "Undertow", None, None, None).await.unwrap();
        assert_eq!(page_of(by_names).as_deref(), Some("https://orbitlane.bandcamp.com/album/undertow"), "found by its names");
        let group = "00000000-0000-4000-8000-000000000210";
        let listed = record_of_album(serve.catalogue(), Some(group), "Kite Theory", "Night Walks", None, None).unwrap();
        serve.userdata.set_bandcamp_found(group, 1, Some("https://kitetheory.bandcamp.com/album/night-walks")).unwrap();
        let asked = record_of_album_in(&serve, None, "Kite Theory", "Night Walks", None, None, None).await.unwrap();
        assert_eq!((asked, page_of(asked).as_deref()), (listed, Some("https://kitetheory.bandcamp.com/album/night-walks")), "by the group its record holds");
    }

    /// W1: a Want with a Bandcamp page that names no record yet is named as Bandcamp lists the album, as kept from its read: its own
    /// artist, not a track's sent, and a label's release sorted out with the page's account as its label.
    #[tokio::test]
    async fn a_want_with_a_page_is_named_as_bandcamp_lists_the_album() {
        let serve = served();
        let page = "https://paperlantern.bandcamp.com/album/tape-club-harbour-songs";
        crate::bandcamp::Bandcamp::get().keep(crate::bandcamp::Album {
            url: page.into(),
            title: "Tape Club - Harbour Songs".into(),
            artist: "Paper Lantern".into(),
            cover: None,
            tracks: Vec::new(),
            released: None,
            label: None,
            account: Some("Paper Lantern".into()),
            about: None,
            credits: None,
            tags: Vec::new(),
        });
        let record = record_of_album_in(&serve, None, "A Track's Singer", "Tape Club - Harbour Songs", None, None, Some(page.into())).await.unwrap();
        let row = serve.catalogue().record(record).unwrap().unwrap();
        assert_eq!((row.artist.as_str(), row.title.as_str()), ("Tape Club", "Harbour Songs"));
        let label = serve.catalogue().label_for("Paper Lantern").unwrap();
        assert!(serve.catalogue().label_records(label).unwrap().iter().any(|(r, _)| *r == record), "the account its label");
    }

    /// W6 (H): an account has one open request of a record: found by its record wherever it is among the account's requests (past
    /// its newest 200), and a second open one is not kept; a status write checks the status it replaces.
    #[tokio::test]
    async fn an_account_has_one_open_request_of_a_record() {
        let serve = served();
        let record = record_of_album(serve.catalogue(), None, "Glass Harbour", "Two Rooms", None, None).unwrap();
        let first = serve.userdata.add_request(&asked_for(record)).unwrap();
        for n in 0..210 {
            serve.userdata.add_request(&Request { status: DECLINED.into(), album_key: format!("k{n}"), record: None, ..asked_for(record) }).unwrap();
        }
        assert_eq!(serve.userdata.open_request("bob", Some(record), "another key").unwrap().map(|r| r.id), Some(first), "past the newest 200");
        assert!(serve.userdata.add_request(&asked_for(record)).is_err(), "no second open one");
        assert!(!serve.userdata.set_request_status(first, PENDING, DECLINED, None, Some("ann")).unwrap(), "it is approved, not pending");
        assert_eq!(serve.userdata.request(first).unwrap().unwrap().status, APPROVED);
    }

    /// W6 (H): a Want's cover is kept only from Cratarr's own covers and the known image hosts.
    #[test]
    fn a_wants_cover_is_kept_only_from_known_hosts() {
        for kept in ["/api/covers/release-group/00000000-0000-4000-8000-000000000116", "https://coverartarchive.org/release-group/x/front", "https://f4.bcbits.com/img/a1_16.jpg", "https://i.scdn.co/image/ab67", "https://i.discogs.com/x.jpg"] {
            assert!(cover_kept(kept), "{kept}");
        }
        for left in ["https://example.org/a.jpg", "http://coverartarchive.org/x", "javascript:alert(1)", "https://notbcbits.com/a.jpg"] {
            assert!(!cover_kept(left), "{left}");
        }
    }

    /// W6 (H): an account other than an administrator reads each request's state, never the fetchers' own words (Lidarr's errors
    /// name its address).
    #[test]
    fn others_see_the_state_words_not_lidarrs() {
        let r = Request { fetched: Some("Not handed over to Lidarr: error sending request for url (http://192.168.1.20:8686/api/v1/album)".into()), ..asked_for(1) };
        let v = View { request: r, state: RequestState::Wanted { lidarr: None, soulseek: None }, album_state: Album::Unknown, hand_over_again: true };
        assert_eq!(shown_to(vec![v.clone()], false)[0].request.fetched, None);
        assert_eq!(shown_to(vec![v.clone()], false)[0].state, v.state, "the state stays");
        assert!(shown_to(vec![v], true)[0].request.fetched.is_some(), "an administrator's");
    }

    /// W5 (G1): every view reads one state of a request (`request_state`): the Requests page (and Hand over again on it, open while
    /// no fetcher has spoken), the marks the lists show, the requested lists (search's, Discover's), and Health's count of those
    /// waiting for an answer, which leaves one the library has out.
    #[tokio::test]
    async fn every_view_reads_one_request_state() {
        let serve = served();
        let cat = serve.catalogue();
        let owned = cat.record_for_tests("Harbour Lights", 2);
        let mut done = Request { status: PENDING.into(), ..asked_for(owned) };
        done.id = serve.userdata.add_request(&done).unwrap();
        let wanted = record_of_album(cat, None, "Glass Harbour", "Quiet Lanes", None, None).unwrap();
        let mut asking = Request { status: PENDING.into(), album_key: "k2".into(), ..asked_for(wanted) };
        asking.id = serve.userdata.add_request(&asking).unwrap();
        let other = record_of_album(cat, None, "Glass Harbour", "Two Rooms", None, None).unwrap();
        let mut asked = Request { album_key: "k3".into(), ..asked_for(other) };
        asked.id = serve.userdata.add_request(&asked).unwrap();
        let rs = vec![done.clone(), asking.clone(), asked.clone()];
        let shown = views(&serve, None, rs.clone()).await;
        assert_eq!(shown.iter().map(|v| v.state.clone()).collect::<Vec<_>>(), [RequestState::Done, RequestState::WaitingForApproval, RequestState::Wanted { lidarr: None, soulseek: None }]);
        assert_eq!(shown.iter().map(|v| v.album_state.clone()).collect::<Vec<_>>(), [Album::InLibrary, Album::Requested, Album::Unknown], "the page's own, from the one state");
        assert_eq!(shown.iter().map(|v| v.hand_over_again).collect::<Vec<_>>(), [false, false, true], "no fetcher has spoken of the one wanted");
        assert_eq!(waiting(&serve).unwrap().iter().map(|r| r.id).collect::<Vec<_>>(), [asking.id], "Health: the one waiting for an answer");
        assert_eq!(not_in_library(&serve, rs.clone()).await.iter().map(|r| r.id).collect::<Vec<_>>(), [asking.id, asked.id], "the requested lists");
        let marks = marks_of(&shown, &named(&serve, &rs).await);
        assert!(!marks.by_record.contains_key(&owned) && marks.by_record.contains_key(&wanted) && marks.by_record.contains_key(&other), "the marks");
    }

    /// W4 (D3): Health's "Ask for the full album" fetches it again: its group finds the owned record, short of the list its files
    /// name, which reads "partly there" and stays open; Lidarr is asked for it, and Soulseek searches it by that full list.
    #[tokio::test]
    async fn asking_for_the_full_album_fetches_it() {
        let serve = served();
        let cat = serve.catalogue();
        let group = "00000000-0000-4000-8000-000000000303";
        let owned = cat.record_for_tests("Tide Tables", 2);
        assert!(cat.set_reference(&crate::serve::ids::record(owned), &crate::catalogue::records::Ref::new("musicbrainz", "release-group", group), "files").unwrap());
        let list: Vec<crate::catalogue::identity::RefTrack> =
            (1..=3).map(|n| crate::catalogue::identity::RefTrack { disc: 1, position: n, title: format!("Track {n}"), length_ms: Some(200_000), recording: None }).collect();
        cat.set_reference_tracks(owned, "musicbrainz", &list).unwrap();
        assert_eq!(record_of_album(cat, Some(group), "Glass Harbour", "Tide Tables", None, None).unwrap(), owned, "by its group, the owned album");
        let mut r = Request { release_group: Some(group.into()), ..asked_for(owned) };
        r.id = serve.userdata.add_request(&r).unwrap();
        let of = records_of(cat, std::slice::from_ref(&r)).unwrap().pop().flatten().unwrap();
        assert_eq!(of.holding, Holding::Partly { has: 2, of: 3 });
        assert_eq!(views(&serve, None, vec![r.clone()]).await[0].album_state, Album::PartlyThere { tracks: 2, of: 3 });
        let lidarr = Wanting::default();
        hand_to(Some(&lidarr), false, &serve, &r).await;
        assert_eq!(lidarr.0.lock().unwrap()[0].0.as_deref(), Some(group), "Lidarr asked for it");
        let none = |_: crate::catalogue::records::Ref| std::future::ready(Ok::<Option<crate::app::references::Read>, anyhow::Error>(None));
        let judged = crate::app::references::tracklist(cat, owned, none).await.unwrap().unwrap();
        assert_eq!(judged.timed.map(|(source, l)| (source, l.len())), Some(("musicbrainz".to_string(), 3)), "searched by the full list");
    }

    /// W4 (D2, the owner's answer (d)): a requested album in Incoming reads "Arrived" (Incoming's state) until it is added; a whole
    /// copy in a main library or someone's own music makes it done.
    #[tokio::test]
    async fn a_requested_album_in_incoming_reads_arrived() {
        let serve = served();
        let cat = serve.catalogue();
        let record = cat.record_for_tests("Two Rooms", 2);
        cat.copy_for_tests(record, 1);
        let mut r = asked_for(record);
        r.id = serve.userdata.add_request(&r).unwrap();
        assert_eq!(views(&serve, None, vec![r.clone()]).await[0].album_state, Album::InIncoming, "in Incoming alone (its second folder)");
        assert!(!not_in_library(&serve, vec![r.clone()]).await.is_empty(), "still among those coming");
        cat.copy_for_tests(record, 0);
        assert_eq!(views(&serve, None, vec![r]).await[0].album_state, Album::InLibrary, "and in Music (its first)");
    }

    /// W4 (the owner's answer (c)): deleting a copy that arrived for a request (after it was asked) closes the request as rejected,
    /// Lidarr told to stop by its record's group; wanted again, it is a new request.
    #[tokio::test]
    async fn rejecting_the_arrival_closes_its_request() {
        let serve = served();
        let group = "00000000-0000-4000-8000-000000000304";
        let record = record_of_album(serve.catalogue(), Some(group), "Glass Harbour", "Two Rooms", None, None).unwrap();
        let mut r = Request { release_group: Some(group.into()), at: 100, ..asked_for(record) };
        r.id = serve.userdata.add_request(&r).unwrap();
        let arrived = arrived_for(&serve, record, Some(200)).await;
        assert_eq!(arrived.iter().map(|r| r.id).collect::<Vec<_>>(), [r.id]);
        let lidarr = Wanting::default();
        reject(Some(&lidarr), &serve, &arrived).await;
        let closed = serve.userdata.request(r.id).unwrap().unwrap();
        assert_eq!((closed.status.as_str(), closed.reason.as_deref()), (REJECTED, Some("Rejected")));
        assert_eq!(*lidarr.1.lock().unwrap(), [group.to_string()], "Lidarr told to stop, by the record's group");
        assert!(open_request(&serve, "bob", &r.album_key).unwrap().is_none(), "wanted again, a new request");
    }

    /// W4: deleting a copy that was there before the request was asked changes nothing of it.
    #[tokio::test]
    async fn deleting_an_older_copy_leaves_the_request_alone() {
        let serve = served();
        let record = record_of_album(serve.catalogue(), None, "Glass Harbour", "Quiet Lanes", None, None).unwrap();
        let mut r = Request { at: 300, ..asked_for(record) };
        r.id = serve.userdata.add_request(&r).unwrap();
        assert!(arrived_for(&serve, record, Some(200)).await.is_empty());
        assert!(arrived_for(&serve, record, None).await.is_empty(), "nor one whose coming is not known");
        assert_eq!(serve.userdata.request(r.id).unwrap().unwrap().status, APPROVED);
    }

    /// W3 (F4): a Want of an album the library has is handed to no fetcher: Lidarr is not asked (it would fetch it again where its
    /// folder no longer holds it), and nothing says Soulseek searches for it. It reads as had.
    #[tokio::test]
    async fn a_want_of_a_had_album_is_not_handed_to_lidarr() {
        let serve = served();
        let owned = serve.catalogue().record_for_tests("Harbour Lights", 2);
        let mut r = asked_for(owned);
        r.id = serve.userdata.add_request(&r).unwrap();
        let lidarr = Wanting::default();
        assert_eq!(hand_to(Some(&lidarr), true, &serve, &r).await, ALREADY_HAD);
        assert!(lidarr.0.lock().unwrap().is_empty(), "Lidarr not asked");
        let wanted = record_of_album(serve.catalogue(), None, "Glass Harbour", "Quiet Lanes", None, None).unwrap();
        let mut other = asked_for(wanted);
        other.id = serve.userdata.add_request(&other).unwrap();
        hand_to(Some(&lidarr), false, &serve, &other).await;
        assert_eq!(lidarr.0.lock().unwrap().len(), 1, "one not had is handed over");
    }

    /// W2 (B2): a Want's release group and Bandcamp page are never written onto an owned record found by its names: the had rule
    /// would count another album had. The request keeps the group it asked with.
    #[tokio::test]
    async fn an_asked_group_is_never_written_onto_an_owned_record_found_by_names() {
        let serve = served();
        let cat = serve.catalogue();
        let owned = cat.record_for_tests("Two Rooms", 2);
        let group = "00000000-0000-4000-8000-000000000302";
        let record = record_of_album_in(&serve, Some(group.into()), "Glass Harbour", "Two Rooms", None, None, Some("https://glassharbour.bandcamp.com/album/two-rooms".into())).await.unwrap();
        assert_eq!(record, owned, "its own names, nothing it holds against them");
        let asked = asked_of(cat, record, None, Some(group.into())).unwrap();
        assert_eq!(asked.release_group.as_deref(), Some(group), "the request keeps what it asked");
        assert!(cat.references_of(&crate::serve::ids::record(owned)).unwrap().is_empty(), "nothing written onto the owned record");
    }

    /// W1: a page Bandcamp did not let be read (its bot check) leaves the names sent, sorted out as names alone are.
    #[test]
    fn a_page_not_read_leaves_the_names_sent() {
        let named = crate::bandcamp::named_by_page(None, "Paper Lantern", "Tape Club - Harbour Songs");
        assert_eq!((named.artist.as_str(), named.title.as_str(), named.label), ("Paper Lantern", "Tape Club - Harbour Songs", None));
    }

    /// R30 (N15): who asked for what is kept with the marks (`MARKS_KEEP`): the Albums page, asking at every open, reads the
    /// requests and their records once in that while; a request made meanwhile shows once it has passed.
    #[test]
    fn requesters_are_kept_with_the_marks() {
        let serve = served();
        let first = record_of_album(serve.catalogue(), None, "Glass Harbour", "Tide Tables", None, None).unwrap();
        serve.userdata.add_request(&asked_for(first)).unwrap();
        assert_eq!(requesters(&serve).len(), 1);
        let second = record_of_album(serve.catalogue(), None, "Glass Harbour", "Two Rooms", None, None).unwrap();
        serve.userdata.add_request(&asked_for(second)).unwrap();
        assert_eq!(requesters(&serve).len(), 1, "kept, not read again");
    }

    /// R22 (the owner's GROUCH request, as production's copy holds it, made up: FX01's 12): asked by the label's account with no
    /// release group, it takes the record the artist's discography made of its split names, and Lidarr is asked with that record's
    /// group, by its names.
    #[tokio::test]
    async fn a_label_named_request_asks_lidarr_with_its_discography_records_group() {
        let d = std::env::temp_dir().join(format!("cratarr-requests-label-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        crate::fixtures::data_0_16(&d).unwrap();
        let serve = Serve::open(Some(&d), crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(Arc::new(crate::catalogue::Catalogue::open(Some(&d.join("catalogue.sqlite")), vec![std::path::PathBuf::from("/music")]).unwrap()));
        super::super::step_017::run(serve.catalogue(), &serve.userdata).unwrap();
        let r = serve.userdata.requests(None, Some(APPROVED), LIST).unwrap().into_iter().find(|r| r.id == 12).unwrap();
        assert_eq!(r.release_group, None, "asked with none");
        let lidarr = Wanting::default();
        hand_to(Some(&lidarr), false, &serve, &r).await;
        assert_eq!(lidarr.0.lock().unwrap()[0], (Some("00000000-0000-4000-8000-000000000116".to_string()), "Tape Club".to_string(), "Long Player".to_string()));
        drop(serve);
        std::fs::remove_dir_all(&d).ok();
    }

    /// A server of the tests' own, in memory, its catalogue attached.
    fn served() -> Arc<Serve> {
        let serve = Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap()));
        serve
    }

    /// The Lidarr a test hands requests to: what it was asked to want, and to stop wanting; the release group it finds.
    #[derive(Default)]
    struct Wanting(Mutex<Vec<(Option<String>, String, String)>>, Mutex<Vec<String>>);

    const FOUND: &str = "00000000-0000-4000-8000-000000000131";

    impl Fetcher for Wanting {
        fn want<'a>(&'a self, release_group: Option<&'a str>, artist: &'a str, album: &'a str) -> crate::source::Fut<'a, crate::lidarr::Handed> {
            self.0.lock().unwrap().push((release_group.map(str::to_string), artist.to_string(), album.to_string()));
            Box::pin(async { Ok(crate::lidarr::Handed { said: "Lidarr added it and is looking for it".into(), release_group: FOUND.into() }) })
        }

        fn statuses<'a>(&'a self, _: &'a [String]) -> crate::source::Fut<'a, HashMap<String, Progress>> {
            Box::pin(async { Ok(HashMap::new()) })
        }

        fn cancel<'a>(&'a self, release_group: &'a str) -> crate::source::Fut<'a, ()> {
            self.1.lock().unwrap().push(release_group.to_string());
            Box::pin(async { Ok(()) })
        }
    }

    /// C51b: Lidarr is asked for a request as its record is named, a label's release by the artist and title split from what
    /// the label's page said (S12, the owner's GROUCH case's shape); the request keeps the names it was asked with, and the
    /// release group Lidarr found goes on the record, found by names (`search`).
    #[tokio::test]
    async fn lidarr_is_asked_with_the_records_names() {
        let serve = served();
        let record = record_of_album(serve.catalogue(), None, "Paper Lantern", "Tape Club - After Hours (Paper Lantern)", Some(2020), None).unwrap();
        let mut r = Request {
            id: 0,
            user: "bob".into(),
            album_key: requested("Paper Lantern", "Tape Club - After Hours (Paper Lantern)"),
            artist_key: "paper lantern".into(),
            release_group: None,
            album: "Tape Club - After Hours (Paper Lantern)".into(),
            artist: "Paper Lantern".into(),
            year: Some(2020),
            cover: None,
            at: 1,
            status: APPROVED.into(),
            reason: None,
            decided_by: Some("ann".into()),
            decided_at: Some(1),
            fetched: None,
            record: Some(record),
        };
        r.id = serve.userdata.add_request(&r).unwrap();
        let lidarr = Wanting::default();
        assert_eq!(hand_to(Some(&lidarr), false, &serve, &r).await, "Lidarr added it and is looking for it");
        assert_eq!(*lidarr.0.lock().unwrap(), [(None, "Tape Club".to_string(), "After Hours".to_string())]);
        let kept = serve.userdata.request(r.id).unwrap().unwrap();
        assert_eq!((kept.artist.as_str(), kept.album.as_str(), kept.release_group), ("Paper Lantern", "Tape Club - After Hours (Paper Lantern)", None), "as asked");
        let refs = serve.catalogue().references_of(&crate::serve::ids::record(record)).unwrap();
        assert!(refs.iter().any(|(r, row)| (r.source.as_str(), r.kind.as_str(), r.value.as_str(), row.how.as_str()) == ("musicbrainz", "release-group", FOUND, "search")), "{refs:?}");
        // Asked again, the record names its group.
        hand_to(Some(&lidarr), false, &serve, &r).await;
        assert_eq!(lidarr.0.lock().unwrap()[1].0.as_deref(), Some(FOUND));
    }

    #[test]
    fn a_request_marks_its_album_wanted_or_coming_and_none_once_in() {
        assert_eq!(mark(&Album::InLibrary), None);
        assert_eq!(mark(&Album::Looking), Some(Mark { state: "wanted", done: None }));
        assert_eq!(mark(&Album::Unknown), Some(Mark { state: "wanted", done: None }), "no fetcher, or not handed over");
        assert_eq!(mark(&Album::Fetching { done: 0.4 }), Some(Mark { state: "downloading", done: Some(0.4) }));
        assert_eq!(mark(&Album::PartlyThere { tracks: 3, of: 9 }), Some(Mark { state: "downloading", done: Some(1.0) }));
        let marks = Marks {
            by_key: [
                ("rg-1".to_string(), Mark { state: "wanted", done: None }),
                (format!("{}|{}", crate::app::import::key("Ott"), crate::app::import::key("Skylon")), Mark { state: "downloading", done: Some(0.5) }),
            ]
            .into(),
            by_record: HashMap::new(),
        };
        assert_eq!(mark_of(&marks, Some("rg-1"), "X", "Y").map(|m| m.state), Some("wanted"));
        assert_eq!(mark_of(&marks, Some("rg-2"), "OTT", "Skylon").map(|m| m.state), Some("downloading"), "by its names where its release group has none");
        assert_eq!(mark_of(&marks, None, "Ott", "Mir"), None);
    }
}
