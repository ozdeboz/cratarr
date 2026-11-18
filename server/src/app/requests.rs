//! Requests (decision 0070): an album asked for from Discover is kept by Cratarr with its own status (pending,
//! approved, declined, cancelled); an approved one is handed to a fetcher (`ports::Fetcher`, Lidarr first), and the
//! album's own state (being fetched, in Incoming, in the library) read beside it. Seerr's model (MIT,
//! github.com/seerr-team/seerr), not its code. Who may ask, have their asking approved at once, and answer everyone's:
//! the account's permissions (`serve::users::Permissions`), an administrator's all.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

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
    /// Its grab failed or stalls (X1): Soulseek's turn.
    Stuck,
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
                Progress::Stuck => Some(Fetching::Stuck),
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
    // The row keeps only a cover every account may be shown (Y5): this is the one way a request row is written.
    let cover = row.cover.filter(|c| cover_kept(c));
    Ok(Asked { record: row.id, key, artist_key: super::import::loose(&row.artist), release_group: group, album: row.title, artist: row.artist, year: row.year, cover })
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

/// Whether a Want's cover is one every account may be shown (W6): Cratarr's own, in the exact shapes it makes (O3: a release
/// group's `/api/covers/release-group/<id>`, a Bandcamp album's `/api/cover/bandcamp/<id>`, never a path that climbs out with `..`),
/// or the Cover Art Archive's, Bandcamp's, Spotify's or Discogs' images over https. Any other address is left out. Checked on the
/// way to every record (`record_of_album_paged`) and by 0.17's step for 0.16's Wants, on the records and on the request rows; on
/// the way onto a request row (`asked_of`) and on every view of one (`shown`, Y5).
pub(crate) fn cover_kept(cover: &str) -> bool {
    if cover.starts_with('/') {
        return super::cover_cache::Of::of_address(cover).is_some();
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
    // Every way to a record checks its cover (O3).
    let cover = cover.filter(|c| cover_kept(c));
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

/// What a request of an album not out yet says until it is (X1): both fetchers wait for its release, Lidarr asked then (`start`).
pub const WAITS_FOR_RELEASE: &str = "Waits for its release:";

/// What a request says once its album arrived and Lidarr was told to stop looking for it (X1: the first arrival stops the other).
pub const STOPPED_ON_ARRIVAL: &str = "Lidarr told to stop: it arrived";

/// The day a release date names ("2026", "2026-12", "2026-12-04"), at its start, epoch seconds (UTC). Pure.
pub fn release_day(s: &str) -> Option<i64> {
    let mut parts = s.trim().split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = match parts.next() {
        Some(p) => p.parse().ok()?,
        None => 1,
    };
    let d: i64 = match parts.next() {
        Some(p) => p.parse().ok()?,
        None => 1,
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Days from 1970-01-01 (days_from_civil).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400)
}

/// When each fetcher was last found out of reach and has stayed so (X1: a dead fetcher raises a Health item), by its name.
static DOWN: std::sync::Mutex<std::collections::BTreeMap<&'static str, i64>> = std::sync::Mutex::new(std::collections::BTreeMap::new());

/// How long a fetcher may be out of reach before Health says so (X1).
pub const DOWN_FOR: i64 = 3600;

/// What a fetcher's last word was, now: reached, or out of reach (since the first time it was).
pub fn fetcher_said(fetcher: &'static str, reached: bool) {
    fetcher_said_at(fetcher, reached, super::moment::now_secs());
}

/// `fetcher_said`, at `at`.
pub(crate) fn fetcher_said_at(fetcher: &'static str, reached: bool, at: i64) {
    let mut down = DOWN.lock().unwrap_or_else(|p| p.into_inner());
    if reached {
        down.remove(fetcher);
    } else {
        down.entry(fetcher).or_insert(at);
    }
}

/// The fetchers out of reach for over `DOWN_FOR` at `now`, each with since when (Health's item).
pub fn fetchers_down(now: i64) -> Vec<(&'static str, i64)> {
    DOWN.lock().unwrap_or_else(|p| p.into_inner()).iter().filter(|(_, since)| now - **since > DOWN_FOR).map(|(f, s)| (*f, *s)).collect()
}

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

/// The open requests of a copy's record (V2): pending or approved, whose record it is or whose record it holds, each as the pages
/// show it. Read before the copy goes, as `arrived_for`: one a Delete leaves keeps a fetcher (`deleted`).
pub async fn open_of(serve: &Serve, record: i64) -> Vec<Request> {
    let open: Vec<Request> = serve.userdata.requests(None, None, OPEN).unwrap_or_default().into_iter().filter(|r| r.status == PENDING || r.status == APPROVED).collect();
    let of = named(serve, &open).await;
    open.into_iter().zip(of).filter(|(_, o)| o.as_ref().is_some_and(|o| o.record == record || o.holder == Some(record))).map(|(r, o)| shown(r, o.as_ref())).collect()
}

/// The requests a Delete of their arrival closes (Y4): an administrator's, every one the copy arrived for; anyone else's, their
/// own alone, the other askers' staying as they were. Pure.
pub fn closed_by(arrived: Vec<Request>, administering: bool, viewer: Option<&str>) -> Vec<Request> {
    arrived.into_iter().filter(|r| administering || viewer == Some(r.user.as_str())).collect()
}

/// What a Delete's journal entry keeps the requests it closed under (Y4), for its Undo (`reopen`).
pub const CLOSED_BY_DELETE: &str = "requests-closed";

/// What a Delete's journal entry says where it left an open request of its record (V2, `Incoming::keep_fetching`): Lidarr goes on,
/// so what Incoming's handler stopped for the copy is put back.
pub const KEPT_FOR_REQUESTS: &str = "lidarr-kept";

/// A request a Delete closed, as it was before (Y4): what its Undo puts back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Closed {
    pub id: i64,
    pub status: String,
    pub reason: Option<String>,
    pub decided_by: Option<String>,
    pub decided_at: Option<i64>,
}

/// Requests whose arrival was deleted from Incoming, closed (W4, the owner's answer (c)): each "rejected", its reason "Rejected".
/// What it closed, each as it was, is kept on the Delete's journal entry (`undo`) before anyone is told, so its Undo opens them
/// again (`reopen`, Y4). Then Lidarr, where it is given (none where another request of the record is left: `deleted`, V2), told
/// to stop by each one's record's group, as far as it can be (a failure is said in the log); Soulseek woken, which drops its item
/// unless it is under way. Wanted again, it is a new request. What it closed.
pub async fn reject(lidarr: Option<&dyn Fetcher>, ud: &crate::userdata::UserData, rs: &[Request], undo: Option<(&crate::journal::Journal, crate::journal::EventId)>) -> Vec<Closed> {
    let mut closed = Vec::new();
    for r in rs {
        match ud.set_request_status(r.id, APPROVED, REJECTED, Some(REJECTED_WHY), None) {
            Ok(true) => closed.push(Closed { id: r.id, status: r.status.clone(), reason: r.reason.clone(), decided_by: r.decided_by.clone(), decided_at: r.decided_at }),
            // Answered otherwise meanwhile (W6): as it is.
            Ok(false) => {}
            Err(e) => tracing::warn!("request {}: not closed as rejected: {e:#}", r.id),
        }
    }
    if let Some((journal, id)) = undo
        && !closed.is_empty()
        && let Err(e) = journal.done(id, CLOSED_BY_DELETE, super::moment::now_secs(), &serde_json::json!(closed))
    {
        tracing::warn!("the requests a Delete closed were not kept for its Undo: {e:#}");
    }
    for r in rs.iter().filter(|r| closed.iter().any(|c| c.id == r.id)) {
        if let (Some(f), Some(group)) = (lidarr, r.release_group.as_deref())
            && let Err(e) = f.cancel(group).await
        {
            tracing::warn!("request {}: Lidarr not told to stop: {e:#}", r.id);
        }
    }
    if !rs.is_empty() {
        super::soulseek::wake();
    }
    closed
}

/// A Delete of a copy in Incoming, for its requests (W4, Y4, V2, U5): those it closes (`closed_by`) closed as rejected (`reject`).
/// Where it leaves an approved request of the record (`open`, read before the copy went: `open_of`) whose record it no longer holds,
/// a fetcher goes on for it. Each one's status is read again now the copy has gone (U5): a pending one alone is no reason to fetch,
/// and one cancelled meanwhile is not handed again.
/// - A copy Lidarr brought first has its own grab marked failed in Lidarr's history (U5, T9: `whose`), and only that grab, whose
///   import is the deleted copy: Lidarr blocklists that release and looks for another, so the next asker is not handed the same bad
///   rip. Where no such grab is found, the copy is no fetcher's known (dropped by hand), or Lidarr can't be told, Lidarr stays
///   stopped: the closed ones tell it so (but where it is away: it was told when the copy arrived, and the Delete's own entry tells
///   it again), and Soulseek alone looks for those left. A copy another fetcher brought (`copy`'s library names it) leaves Lidarr
///   to go on.
/// - Where Lidarr goes on, as when one of two requests is taken back (`tell_stopped`), it is not told to stop, and each one closed
///   says so (`LIDARR_KEPT`).
///
/// Each one left is handed to the fetchers again (`hand_to`), its word that Lidarr was told to stop on the arrival gone. Whether
/// Lidarr goes on: Incoming's handler then puts back what it stopped for the copy (`Incoming::keep_fetching`).
#[allow(clippy::too_many_arguments)]
pub async fn deleted(lidarr: Option<&dyn Fetcher>, soulseek: bool, serve: &Serve, closing: &[Request], open: Vec<Request>, copy: Option<&DeletedCopy>, undo: Option<(&crate::journal::Journal, crate::journal::EventId)>) -> bool {
    // Approved now, as the copy has gone (U5).
    let left: Vec<Request> = open
        .into_iter()
        .filter(|r| !closing.iter().any(|c| c.id == r.id))
        .filter(|r| serve.userdata.request(r.id).ok().flatten().is_some_and(|now| now.status == APPROVED))
        .collect();
    // Had still, by another whole copy: none left wants it.
    let of = named(serve, &left).await;
    let left: Vec<Request> = left.into_iter().zip(of).filter(|(_, o)| !o.as_ref().is_some_and(RecordOf::had)).map(|(r, _)| r).collect();
    if left.is_empty() {
        reject(lidarr, &serve.userdata, closing, undo).await;
        return false;
    }
    // Whose the copy was (U5, T9): Lidarr's own grab of it failed first, by the group Lidarr was asked by; else Lidarr stays stopped
    // and Soulseek looks. One another fetcher brought leaves Lidarr to go on.
    let was = match copy {
        Some(copy) => whose(lidarr, copy, left.iter().chain(closing).find_map(|r| r.release_group.as_deref())).await,
        None => Whose::Another,
    };
    let goes_on = matches!(was, Whose::Another | Whose::Lidarr { failed: true });
    // Lidarr away: not told again to stop, as it was when the copy arrived (one wait, and one warning).
    let closed = reject(lidarr.filter(|_| !goes_on && was != Whose::Unknown), &serve.userdata, closing, undo).await;
    if goes_on {
        for r in closing.iter().filter(|r| closed.iter().any(|c| c.id == r.id)) {
            if let Some(said) = r.fetched.as_deref().filter(|f| f.contains("Lidarr") && !f.starts_with("Not handed over")) {
                let _ = serve.userdata.set_request_fetched(r.id, &format!("{said}; {LIDARR_KEPT}"));
            }
        }
    }
    for r in &left {
        hand_to(lidarr.filter(|_| goes_on), soulseek, serve, r).await;
    }
    goes_on
}

/// A deleted Incoming copy, as its Delete tells its requests (U5, T9): the fetcher its library names (none: a library from before
/// 0.15's fetchers, as staging's `lidarr-music`), and its folder there (`Artist/Album (Year)`), which Lidarr's history is matched by.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeletedCopy {
    pub fetcher: Option<String>,
    pub folder: String,
}

/// Whose a deleted Incoming copy is, for the requests it leaves (T9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whose {
    /// Another fetcher's, its library naming it (Soulseek's): Lidarr is not asked, and goes on for those left.
    Another,
    /// No fetcher's known: its library names none and Lidarr's history imported nothing into its folder (dropped there by hand).
    Nobody,
    /// Lidarr's, its library naming Lidarr or its history importing into the folder; `failed`: its own grab marked failed.
    Lidarr { failed: bool },
    /// Not known: Lidarr not connected, no release group to ask by, or Lidarr away or answering wrongly.
    Unknown,
}

/// Whose a deleted Incoming copy is (T9, the review of run 10, finding 9), Lidarr asked by `group`, the release group it was asked
/// by. A library naming another fetcher says so alone. Else Lidarr's history says what it imported into the copy's folder, and the
/// grab of the album's newest import is marked failed only where that import went there (`Lidarr::fail_grab`): a good release
/// another asker waits for, a download under way, or a copy dropped by hand is never blocklisted. A library naming Lidarr is
/// Lidarr's whatever the history says; one naming none, only by that history. Lidarr away: the warning logged once, here.
pub async fn whose(lidarr: Option<&dyn Fetcher>, copy: &DeletedCopy, group: Option<&str>) -> Whose {
    use crate::lidarr::Imported;
    let named = copy.fetcher.as_deref();
    if named.is_some_and(|f| f != "lidarr") {
        return Whose::Another;
    }
    let (Some(f), Some(group)) = (lidarr, group) else { return Whose::Unknown };
    match f.fail_grab(group, &copy.folder).await {
        Ok(Imported::Failed) => Whose::Lidarr { failed: true },
        Ok(Imported::Unmarked) => {
            tracing::info!(target: "requests", group = %group, folder = %copy.folder, "a deleted copy's grab not marked: Lidarr's newest import of the album went elsewhere, or its grab is not in the history; Lidarr stays stopped, Soulseek looks");
            Whose::Lidarr { failed: false }
        }
        Ok(Imported::Not) if named.is_some() => {
            tracing::info!(target: "requests", group = %group, folder = %copy.folder, "a deleted copy's grab is not in Lidarr's history: Lidarr stays stopped, Soulseek looks");
            Whose::Lidarr { failed: false }
        }
        Ok(Imported::Not) => {
            tracing::info!(target: "requests", group = %group, folder = %copy.folder, "a deleted copy Lidarr never imported (dropped by hand): no grab marked, Lidarr stays stopped, Soulseek looks");
            Whose::Nobody
        }
        Err(e) => {
            tracing::warn!(target: "requests", group = %group, reason = %format!("{e:#}"), "a deleted copy's grab not marked failed: Lidarr stays stopped, Soulseek looks");
            Whose::Unknown
        }
    }
}

/// The requests a Delete closed, opened again by its Undo (Y4), each as it was (`Closed`), where it still reads as the Delete
/// left it: an answer since stands, and so does a request of the record its account made since (one open request a record,
/// W6). The ids opened. Blocking.
pub fn reopen(ud: &crate::userdata::UserData, closed: &[Closed]) -> Vec<i64> {
    let mut opened = Vec::new();
    for c in closed {
        match ud.restore_request_status(c.id, REJECTED, &c.status, c.reason.as_deref(), c.decided_by.as_deref(), c.decided_at) {
            Ok(true) => opened.push(c.id),
            Ok(false) => {}
            Err(e) => tracing::warn!("request {}: not opened again: {e:#}", c.id),
        }
    }
    opened
}

/// What a request of a record the library has says, handed to no fetcher (W3).
pub const ALREADY_HAD: &str = "Already in the library";

/// An approved request handed to every fetcher connected (decision 0099's amendment), as its record is named and referenced
/// (`shown`, C51b): Lidarr told to want it; Soulseek woken to take it in (it reads the approved Requests itself, each judged
/// by its record's list; one with none says so on its row). What each said (or why it could not), kept on the request. A
/// release group Lidarr found by the record's names is kept on the record, as found by names (`search`): its state can be
/// read from then on.
async fn hand(app: &super::App, serve: &Serve, r: &Request) -> String {
    // A record the library has already (C52, OQ3): the asker's own copy of it, in their own music, as Copy to makes one (T5: from
    // anyone's whole copy).
    let copied = copied_for(app, serve, r).await;
    let lidarr = fetcher(app);
    let soulseek = super::soulseek::connected().await;
    hand_after(lidarr.as_deref(), soulseek, serve, r, copied).await
}

/// What a Want says once its asker's copy was looked at (T5): made, "Copied into your music" (whose music it came from is not said:
/// the owner's OQ-I); not made, why, in the log (`requests`) and on its line, never "Already in the library"; no copy known whole,
/// handed to the fetchers past a had record's shortcut, said in the log; nothing to do, as [`hand_to`].
pub(super) async fn hand_after(lidarr: Option<&dyn Fetcher>, soulseek: bool, serve: &Serve, r: &Request, copied: WantCopy) -> String {
    let record = r.record.map_or_else(|| "none".to_string(), |n| n.to_string());
    let said = match copied {
        WantCopy::Made => COPIED_INTO_OWN.to_string(),
        WantCopy::Failed(why) => {
            tracing::warn!(target: "requests", "request {}, record {record}: its asker's copy was not made: {why}", r.id);
            format!("{NOT_COPIED}: {why}")
        }
        WantCopy::NoneWhole => {
            tracing::warn!(target: "requests", "request {}, record {record}: no copy of it is known whole: handed to the fetchers", r.id);
            return hand_to_as(lidarr, soulseek, serve, r, false).await;
        }
        WantCopy::Nothing => return hand_to(lidarr, soulseek, serve, r).await,
    };
    if let Err(e) = serve.userdata.set_request_fetched(r.id, &said) {
        tracing::warn!("request {}: what was done was not kept: {e:#}", r.id);
    }
    said
}

use super::albums::WantCopy;

/// What a request of a record the library has says once its asker's copy is made in their own music (C52).
pub const COPIED_INTO_OWN: &str = "Copied into your music";

/// What a request of a record the library has says where its asker's copy could not be made (T5), before why.
pub const NOT_COPIED: &str = "Not copied into your music";

/// A request whose record the library holds whole (C52, OQ3: "Add to Your music" any stored record, through Want): its asker's
/// copy of it in their own music (`App::copy_for`, T5). Nothing to do where it is not held whole by its list.
async fn copied_for(app: &super::App, serve: &Serve, r: &Request) -> WantCopy {
    let Some(of) = named(serve, std::slice::from_ref(r)).await.pop().flatten() else { return WantCopy::Nothing };
    match (of.holding, of.holder) {
        (Holding::Done, Some(holder)) => app.copy_for(&r.user, holder).await,
        _ => WantCopy::Nothing,
    }
}

/// [`hand`] to Lidarr where it is connected (the tests' stands in for it), and to Soulseek where it is.
async fn hand_to(lidarr: Option<&dyn Fetcher>, soulseek: bool, serve: &Serve, r: &Request) -> String {
    hand_to_as(lidarr, soulseek, serve, r, true).await
}

/// [`hand_to`]; `had`: whether a record held is said to be had and handed to no fetcher (W3), as it is unless no copy of it is known
/// whole (T5).
async fn hand_to_as(lidarr: Option<&dyn Fetcher>, soulseek: bool, serve: &Serve, r: &Request, had: bool) -> String {
    let of = named(serve, std::slice::from_ref(r)).await.pop().flatten();
    let r = shown(r.clone(), of.as_ref());
    // Its record had (W3, F4): no fetcher's to fetch, so neither is asked (Lidarr would fetch it again where its own folder no
    // longer holds it, and Soulseek takes no item of it, R26). It reads as had; C52 makes the asker's copy of it (OQ3).
    if had && of.as_ref().is_some_and(RecordOf::had) {
        if let Err(e) = serve.userdata.set_request_fetched(r.id, ALREADY_HAD) {
            tracing::warn!("request {}: what the fetchers were not told was not kept: {e:#}", r.id);
        }
        return ALREADY_HAD.to_string();
    }
    // An album not out yet waits for its release, for both fetchers (X1): Lidarr is asked once it is out (`start`'s round), and
    // Soulseek's turn comes then too (`soulseek::first_search`).
    if let Some(day) = of.as_ref().and_then(|o| o.released.clone()).filter(|d| release_day(d).is_some_and(|at| at > super::moment::now_secs())) {
        let said = format!("{WAITS_FOR_RELEASE} {day}");
        if let Err(e) = serve.userdata.set_request_fetched(r.id, &said) {
            tracing::warn!("request {}: what the fetchers were not told was not kept: {e:#}", r.id);
        }
        return said;
    }
    let mut said = Vec::new();
    if let Some(f) = lidarr {
        said.push(match f.want(r.release_group.as_deref(), &r.artist, &r.album).await {
            Ok(handed) => {
                fetcher_said("lidarr", true);
                // The group it was asked with kept on the request (X1): what it is told to stop by.
                let asked = r.release_group.clone().unwrap_or_else(|| handed.release_group.to_ascii_lowercase());
                if let Err(e) = serve.userdata.keep_request_group(r.id, &asked) {
                    tracing::warn!("request {}: the group Lidarr was asked by was not kept: {e:#}", r.id);
                }
                if r.release_group.is_none()
                    && let Some(record) = r.record
                {
                    let (cat, group) = (serve.catalogue().clone(), handed.release_group.to_ascii_lowercase());
                    // Lidarr's guess, by names, goes only onto a record no reference speaks for at all (X3): over a Bandcamp page or a
                    // Discogs release it would become the list Soulseek judges by.
                    let kept = tokio::task::spawn_blocking(move || -> Result<bool> {
                        let subject = crate::serve::ids::record(record);
                        if !cat.references_of(&subject)?.is_empty() {
                            return Ok(false);
                        }
                        cat.set_reference(&subject, &crate::catalogue::records::Ref::new("musicbrainz", "release-group", &group), "search")
                    })
                    .await;
                    if let Err(e) = kept.map_err(anyhow::Error::from).and_then(|k| k) {
                        tracing::warn!("request {}: its release group was not kept: {e:#}", r.id);
                    }
                }
                handed.said
            }
            Err(e) => {
                // Out of reach, not an answer about the album: Health hears of it after an hour (X1).
                if e.downcast_ref::<reqwest::Error>().is_some() {
                    fetcher_said("lidarr", false);
                }
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

/// The requests' round (X1), once a minute: an album now out handed to Lidarr; the first arrival stopping the other fetcher; Lidarr
/// asked whether it can be reached; and, at the first 0.17 start, the approved requests no fetcher spoke of handed to Lidarr once.
pub fn start(app: &std::sync::Arc<super::App>) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    let Some(serve) = crate::serve::instance() else { return };
    let app = app.clone();
    tokio::spawn(async move {
        let mut asked_at = 0;
        loop {
            if let Err(e) = round(&app, &serve, &mut asked_at).await {
                tracing::warn!(target: "requests", reason = %format!("{e:#}"), "the requests' round");
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

/// How often Lidarr is asked whether it can be reached (X1), at most.
const ASK_LIDARR_EVERY: i64 = 300;

/// The migration part that says the first start's hand-over to Lidarr was made (X1).
const HANDED_AT_START: &str = "requests: handed to Lidarr at 0.17's first start";

/// One round of `start`.
async fn round(app: &super::App, serve: &Serve, asked_at: &mut i64) -> Result<()> {
    let lidarr = fetcher(app);
    let now = super::moment::now_secs();
    if let Some(f) = lidarr.as_deref()
        && now - *asked_at >= ASK_LIDARR_EVERY
    {
        *asked_at = now;
        let reached = match super::discover::lidarr(app) {
            Some(l) => l.version().await.is_ok(),
            None => true,
        };
        fetcher_said("lidarr", reached);
        if reached && !serve.userdata.migration_report()?.contains_key(HANDED_AT_START) {
            let handed = hand_over_at_start(f, serve).await;
            serve.userdata.set_migration_part(HANDED_AT_START, &serde_json::json!({ "handed": handed }))?;
        }
    }
    // Out now: handed to Lidarr, Soulseek's turn coming with it.
    let approved = serve.userdata.requests(None, Some(APPROVED), OPEN)?;
    let of = named(serve, &approved).await;
    for (r, o) in approved.iter().zip(&of) {
        let out = o.as_ref().and_then(|o| o.released.as_deref()).and_then(release_day).is_none_or(|day| day <= now);
        if out && r.fetched.as_deref().is_some_and(|f| f.starts_with(WAITS_FOR_RELEASE)) {
            hand(app, serve, r).await;
        }
    }
    stop_arrived(lidarr.as_deref(), serve).await;
    retry_stops(lidarr.as_deref(), serve).await;
    Ok(())
}

/// The approved requests not had that no fetcher has spoken of (none, or "Not handed over"), handed to Lidarr once at the first
/// 0.17 start, one at a time (X1; the owner's GROUCH request is then asked for by its record's group). Their ids.
async fn hand_over_at_start(lidarr: &dyn Fetcher, serve: &Serve) -> Vec<i64> {
    let approved = serve.userdata.requests(None, Some(APPROVED), OPEN).unwrap_or_default();
    let of = named(serve, &approved).await;
    let mut handed = Vec::new();
    for (r, o) in approved.iter().zip(&of) {
        if o.as_ref().is_some_and(RecordOf::had) || !no_fetcher_spoke(r) {
            continue;
        }
        hand_to(Some(lidarr), false, serve, r).await;
        handed.push(r.id);
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    handed
}

/// The first arrival stops the other (X1, the owner's 6 November order): each approved request whose album arrived or is in the
/// library, handed to Lidarr, has Lidarr told to stop by the group it was asked with; said on the request, once
/// (`STOPPED_ON_ARRIVAL`). Every open request of a record reads it alike, so none still wants one that is had (Y12: no check for
/// one). Soulseek drops its own item (`soulseek::take_requests`). The requests told.
pub async fn stop_arrived(lidarr: Option<&dyn Fetcher>, serve: &Serve) -> Vec<i64> {
    let Some(f) = lidarr else { return Vec::new() };
    let open: Vec<Request> = serve.userdata.requests(None, None, OPEN).unwrap_or_default().into_iter().filter(|r| r.status == PENDING || r.status == APPROVED).collect();
    let of = named(serve, &open).await;
    let mut told = Vec::new();
    for (r, o) in open.iter().zip(&of) {
        let Some(o) = o.as_ref().filter(|o| o.had()) else { continue };
        let Some(said) = r.fetched.as_deref().filter(|f| f.contains("Lidarr") && !f.starts_with("Not handed over") && !f.contains(STOPPED_ON_ARRIVAL)) else { continue };
        if r.status != APPROVED {
            continue;
        }
        let Some(group) = r.release_group.clone().or_else(|| o.groups.first().cloned()) else { continue };
        match f.cancel(&group).await {
            Ok(()) => {
                let _ = serve.userdata.set_request_fetched(r.id, &format!("{said}; {STOPPED_ON_ARRIVAL}"));
                told.push(r.id);
            }
            Err(e) => tracing::warn!("request {}: Lidarr not told to stop: {e:#}", r.id),
        }
    }
    told
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
        APPROVED if manage => {}
        PENDING | APPROVED => bail!("Only a request still waiting can be taken back"),
        s => bail!("That request is {s} already"),
    }
    // Written first (X2): the fetchers are told after, as far as they can be, and again at the requests' round where they could
    // not be (`retry_stops`). Soulseek, woken, gives its item to another request of the record, or drops it unless it is under way.
    if !serve.userdata.set_request_status(id, &r.status, CANCELLED, None, Some(account))? {
        bail!("That request was answered meanwhile");
    }
    if r.status == APPROVED {
        tell_stopped(fetcher(app).as_deref(), serve, &r).await;
        super::soulseek::wake();
    }
    Ok(())
}

/// What a request taken back says once Lidarr was told to stop looking for its album (X2), and while it is still to be told.
const LIDARR_TOLD: &str = "Lidarr told to stop";
const LIDARR_TO_TELL: &str = "Lidarr to be told to stop";
/// What one says where another open request still wants its record, so Lidarr goes on (X2).
const LIDARR_KEPT: &str = "Lidarr goes on for another request";

/// Lidarr told to stop looking for a request's album, taken back (X2): by the group it was asked with (kept on the request, X1),
/// else its record's; only where no other open request names the record. Best effort: one not told says so, and the requests' round
/// tells it again (`retry_stops`). A request Lidarr never took is left as it is.
async fn tell_stopped(lidarr: Option<&dyn Fetcher>, serve: &Serve, r: &Request) {
    let Some(said) = r.fetched.clone().filter(|f| f.contains("Lidarr") && !f.starts_with("Not handed over")) else { return };
    let said = said.trim_end_matches(&format!("; {LIDARR_TO_TELL}")).to_string();
    let of = named(serve, std::slice::from_ref(r)).await.pop().flatten();
    let open: Vec<Request> = serve.userdata.requests(None, None, OPEN).unwrap_or_default().into_iter().filter(|o| o.id != r.id && (o.status == PENDING || o.status == APPROVED)).collect();
    let others = named(serve, &open).await;
    let record = of.as_ref().map(|o| o.record).or(r.record);
    let words = if others.iter().flatten().any(|o| Some(o.record) == record) {
        LIDARR_KEPT
    } else {
        match (lidarr, r.release_group.clone().or_else(|| of.as_ref()?.groups.first().cloned())) {
            (Some(f), Some(group)) => match f.cancel(&group).await {
                Ok(()) => LIDARR_TOLD,
                Err(e) => {
                    tracing::warn!("request {}: Lidarr not told to stop yet: {e:#}", r.id);
                    LIDARR_TO_TELL
                }
            },
            (None, Some(_)) => LIDARR_TO_TELL,
            (_, None) => return,
        }
    };
    let _ = serve.userdata.set_request_fetched(r.id, &format!("{said}; {words}"));
}

/// The requests taken back whose Lidarr is still to be told to stop, told again (X2, at the requests' round). Their ids.
async fn retry_stops(lidarr: Option<&dyn Fetcher>, serve: &Serve) -> Vec<i64> {
    let waiting: Vec<Request> = serve.userdata.requests(None, Some(CANCELLED), OPEN).unwrap_or_default().into_iter().filter(|r| r.fetched.as_deref().is_some_and(|f| f.ends_with(LIDARR_TO_TELL))).collect();
    for r in &waiting {
        tell_stopped(lidarr, serve, r).await;
    }
    waiting.iter().map(|r| r.id).collect()
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

/// The account's newest request of a record (C54: the page of a record not had), whatever its status, as the Requests page shows
/// it. None: it asked for none.
pub async fn request_of_record(app: &super::App, serve: &Serve, account: &str, record: i64) -> Result<Option<View>> {
    let f = fetcher(app);
    let newest = newest_of_record(serve, f.as_deref(), account, record).await?;
    Ok(shown_to(newest.into_iter().collect(), crate::serve::access::administering()).pop())
}

/// The account's newest request of an album a view names by its record, else by its release group (G3: a Bandcamp album Discover
/// found by its group, which no record holds by its page): the record that group names. As the Requests page shows it; none where
/// it asked for neither.
pub async fn request_of(app: &super::App, serve: &Serve, account: &str, record: Option<i64>, group: Option<&str>) -> Result<Option<View>> {
    let f = fetcher(app);
    let newest = newest_of(serve, f.as_deref(), account, record, group).await?;
    Ok(shown_to(newest.into_iter().collect(), crate::serve::access::administering()).pop())
}

/// A request just asked for as every view shows it (G11): with where it stands (`state`), as the Requests page reads it, so Want's
/// answer needs no second read. As it was asked where it has no record, which no request since 0.17 lacks.
pub async fn as_viewed(app: &super::App, serve: &Serve, account: &str, r: Request) -> serde_json::Value {
    let f = fetcher(app);
    match viewed(serve, f.as_deref(), account, &r).await {
        Some(v) => serde_json::json!(shown_to(vec![v], crate::serve::access::administering()).pop()),
        None => serde_json::json!(r),
    }
}

/// `as_viewed`'s view, Lidarr asked as `f`: the account's newest request of its record, where that is it.
async fn viewed(serve: &Serve, f: Option<&dyn Fetcher>, account: &str, r: &Request) -> Option<View> {
    let record = r.record?;
    newest_of_record(serve, f, account, record).await.ok().flatten().filter(|v| v.request.id == r.id)
}

/// `request_of`, Lidarr asked as `f`.
async fn newest_of(serve: &Serve, f: Option<&dyn Fetcher>, account: &str, record: Option<i64>, group: Option<&str>) -> Result<Option<View>> {
    let record = match (record, group) {
        (Some(record), _) => Some(record),
        (None, Some(group)) => serve.catalogue().record_named_by(Some(group), None)?,
        (None, None) => None,
    };
    match record {
        Some(record) => newest_of_record(serve, f, account, record).await,
        None => Ok(None),
    }
}

/// `request_of_record`, Lidarr asked as `f`: the account's requests by the record each asked for, as merged now (`named`).
async fn newest_of_record(serve: &Serve, f: Option<&dyn Fetcher>, account: &str, record: i64) -> Result<Option<View>> {
    let asked: Vec<Request> = serve.userdata.requests(Some(account), None, LIST)?.into_iter().filter(|r| r.record.is_some()).collect();
    // None of a record: the catalogue is not read.
    if asked.is_empty() {
        return Ok(None);
    }
    let of = named(serve, &asked).await;
    let Some((r, o)) = asked.into_iter().zip(of).find(|(_, o)| o.as_ref().is_some_and(|o| o.record == record)) else { return Ok(None) };
    Ok(views_of(f, vec![r], &[o]).await.pop())
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
    pub fn of_record(&self, record: i64) -> Option<Mark> {
        self.by_record.get(&record).copied()
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
/// default, and is theirs to add or delete (0101 point 2), where it came after they asked (R32, `Requester::of_copy`). The first
/// asker of a record (X5): with two, the default is the first's own music; copies for the others come with C40 and C52. Kept
/// `MARKS_KEEP`, as the marks are (R30, N15): the Albums page asks at every open, and a read works the had rule out for every
/// approved request's record.
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
    // Newest first: the first to ask is written last, and stands (X5).
    approved.into_iter().zip(of).filter_map(|(r, o)| Some((o?, Requester { user: r.user, at: r.at }))).flat_map(|(o, who)| o.holder.into_iter().chain([o.record]).map(move |id| (id, who.clone()))).collect()
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
    /// When it is first out ("2026-12-04", or a year), as MusicBrainz says: one not out yet waits for it (X1).
    pub released: Option<String>,
    /// Holding no copy, whether its last one went from an Incoming library (U7, T8: `went_from_incoming`): deleted from Incoming, or
    /// removed there outside Cratarr, which lets Soulseek look for it again. One kept into a main library and deleted there months
    /// later is not, nor one that went otherwise.
    pub gone_from_incoming: bool,
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
            Some(RecordOf {
                record: q.row.id,
                groups: q.groups.clone(),
                title: q.row.title.clone(),
                artist: q.row.artist.clone(),
                year: q.row.year,
                cover: q.row.cover.clone(),
                holder: q.holder,
                holding,
                released: q.row.released.clone(),
                gone_from_incoming: false,
            })
        })
        .collect())
}

/// Where the last copy of each record holding none went (U7, T8, for Soulseek's list): `gone_from_incoming` set on those whose files
/// say it went from an Incoming library (`went_from_incoming`), read once for them all (`Catalogue::went_of`). Blocking.
pub fn mark_gone_from_incoming(cat: &crate::catalogue::Catalogue, of: &mut [Option<RecordOf>], kinds: &[(i64, crate::settings::LibraryKind)]) -> Result<()> {
    // An Incoming library's catalogue folder; a parked one (a negative id) is no library's.
    let incoming = |folder: i64| folder >= 0 && crate::serve::library::kind_in(kinds, folder).0 == crate::settings::LibraryKind::Incoming;
    let none: Vec<i64> = of.iter().flatten().filter(|o| o.holding == Holding::None).map(|o| o.record).collect();
    if none.is_empty() {
        return Ok(());
    }
    let went = cat.went_of(&none)?;
    for o in of.iter_mut().flatten().filter(|o| o.holding == Holding::None) {
        o.gone_from_incoming = went.get(&o.record).is_some_and(|files| went_from_incoming(files, &incoming));
    }
    Ok(())
}

/// Whether a record holding no copy saw its last one go from an Incoming library (U7, T8), its files as `Catalogue::went_of` reads
/// them, `incoming` saying which catalogue folders are Incoming libraries': its newest file in a trash lies in one (a Delete from
/// Incoming, U7); or its newest file gone does (removed outside Cratarr, or read as another record since: T8, the review of run 10,
/// finding 8), and no file of it came, went, or went into a trash in a main library or anyone's own music at that time or after.
/// One kept into a main library and deleted there is not. Pure.
pub fn went_from_incoming(files: &[crate::catalogue::records::Went], incoming: impl Fn(i64) -> bool) -> bool {
    use crate::catalogue::records::Went;
    let newest = |at: &dyn Fn(&Went) -> Option<i64>| files.iter().filter_map(|f| Some((at(f)?, f.folder))).max();
    if newest(&|f| f.trashed).is_some_and(|(_, folder)| incoming(folder)) {
        return true;
    }
    let Some((at, folder)) = newest(&|f| f.gone) else { return false };
    incoming(folder) && !files.iter().filter(|f| !incoming(f.folder)).any(|f| [f.came, f.gone, f.trashed].into_iter().flatten().any(|t| t >= at))
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
        r.cover = o.cover.clone().filter(|c| cover_kept(c)).or(r.cover);
        let own = r.release_group.take();
        r.release_group = own.clone().filter(|g| o.groups.iter().any(|x| x.eq_ignore_ascii_case(g))).or_else(|| o.groups.first().cloned()).or(own);
        r.record = Some(o.record);
    }
    // Only a cover every account may be shown (W6, Y5), the record's or the row's, whenever the row was written.
    r.cover = r.cover.filter(|c| cover_kept(c));
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

    /// X5: with two askers of a record, its requester (where its Incoming copy goes by default, who may add it) is the first to ask.
    #[test]
    fn with_two_askers_the_first_is_the_requester() {
        let serve = served();
        let record = record_of_album(serve.catalogue(), None, "Glass Harbour", "Two Rooms", Some(2019), None).unwrap();
        let first = Request { user: "ann".into(), at: 10, ..asked_for(record) };
        let second = Request { user: "bob".into(), at: 20, album_key: "k2".into(), ..asked_for(record) };
        for r in [&second, &first] {
            serve.userdata.add_request(r).unwrap();
        }
        assert_eq!(requesters(&serve).get(&record).map(|r| r.user.as_str()), Some("ann"));
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

    /// X1: an album not out yet waits for its release: Lidarr is not asked, the request says so, and Soulseek's turn is its release.
    #[tokio::test]
    async fn an_unreleased_album_waits_for_its_release() {
        let serve = served();
        let cat = serve.catalogue();
        let record = record_of_album(cat, None, "Glass Harbour", "Next Year", None, None).unwrap();
        cat.db_for_tests().execute("UPDATE records SET released = '2999-04-01' WHERE id = ?1", [record]).unwrap();
        let mut r = asked_for(record);
        r.id = serve.userdata.add_request(&r).unwrap();
        let lidarr = Wanting::default();
        assert_eq!(hand_to(Some(&lidarr), true, &serve, &r).await, "Waits for its release: 2999-04-01");
        assert!(lidarr.0.lock().unwrap().is_empty(), "Lidarr not asked yet");
        let r = serve.userdata.request(r.id).unwrap().unwrap();
        let day = release_day("2999-04-01").unwrap();
        assert_eq!(crate::app::soulseek::first_search(&r, Some(day), true), day, "Soulseek waits for it too");
        assert_eq!(release_day("1970-01-02"), Some(86_400));
        assert_eq!(release_day("2000"), release_day("2000-01-01"));
    }

    /// X1: the first arrival stops the other: Lidarr is told to stop by the group it was asked with, once, where no other open
    /// request still wants the record.
    #[tokio::test]
    async fn the_first_arrival_stops_the_other_fetcher() {
        let serve = served();
        let group = "00000000-0000-4000-8000-000000000305";
        let arrived = serve.catalogue().record_for_tests("Harbour Lights", 2);
        let mut r = Request { release_group: Some(group.into()), fetched: Some("Lidarr added it and is looking for it".into()), ..asked_for(arrived) };
        r.id = serve.userdata.add_request(&r).unwrap();
        let lidarr = Wanting::default();
        assert_eq!(stop_arrived(Some(&lidarr), &serve).await, [r.id]);
        assert_eq!(*lidarr.1.lock().unwrap(), [group.to_string()]);
        assert!(serve.userdata.request(r.id).unwrap().unwrap().fetched.unwrap().ends_with(STOPPED_ON_ARRIVAL));
        assert!(stop_arrived(Some(&lidarr), &serve).await.is_empty(), "once");
    }

    /// X2: a request taken back is cancelled first; Lidarr, out of reach then, is told to stop at a later round, by the group it was
    /// asked with.
    #[tokio::test]
    async fn cancelling_writes_first_and_tells_lidarr_after() {
        let serve = served();
        let group = "00000000-0000-4000-8000-000000000306";
        let record = record_of_album(serve.catalogue(), Some(group), "Glass Harbour", "Two Rooms", None, None).unwrap();
        let mut r = Request { release_group: Some(group.into()), fetched: Some("Lidarr added it and is looking for it".into()), ..asked_for(record) };
        r.id = serve.userdata.add_request(&r).unwrap();
        assert!(serve.userdata.set_request_status(r.id, APPROVED, CANCELLED, None, Some("ann")).unwrap());
        let lidarr = Wanting::default();
        lidarr.2.store(true, std::sync::atomic::Ordering::SeqCst);
        tell_stopped(Some(&lidarr), &serve, &r).await;
        let now = serve.userdata.request(r.id).unwrap().unwrap();
        assert_eq!(now.status, CANCELLED, "written first");
        assert!(now.fetched.as_deref().unwrap().ends_with(LIDARR_TO_TELL));
        lidarr.2.store(false, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(retry_stops(Some(&lidarr), &serve).await, [r.id]);
        assert_eq!(*lidarr.1.lock().unwrap(), [group.to_string()]);
        assert!(serve.userdata.request(r.id).unwrap().unwrap().fetched.as_deref().unwrap().ends_with(LIDARR_TOLD));
        assert!(retry_stops(Some(&lidarr), &serve).await.is_empty(), "told once");
    }

    /// X2: taking back one of two requests of a record leaves Lidarr looking for it, for the other.
    #[tokio::test]
    async fn cancelling_one_of_two_requests_keeps_lidarr_looking() {
        let serve = served();
        let group = "00000000-0000-4000-8000-000000000307";
        let record = record_of_album(serve.catalogue(), Some(group), "Glass Harbour", "Two Rooms", None, None).unwrap();
        let mut bob = Request { release_group: Some(group.into()), fetched: Some("Lidarr added it and is looking for it".into()), ..asked_for(record) };
        bob.id = serve.userdata.add_request(&bob).unwrap();
        serve.userdata.add_request(&Request { user: "ann".into(), ..bob.clone() }).unwrap();
        assert!(serve.userdata.set_request_status(bob.id, APPROVED, CANCELLED, None, Some("bob")).unwrap());
        let lidarr = Wanting::default();
        tell_stopped(Some(&lidarr), &serve, &bob).await;
        assert!(lidarr.1.lock().unwrap().is_empty(), "ann's still wants it");
        assert!(serve.userdata.request(bob.id).unwrap().unwrap().fetched.as_deref().unwrap().ends_with(LIDARR_KEPT));
    }

    /// X3: Lidarr's guess of a release group by names goes only onto a record no reference speaks for: one holding a Bandcamp page
    /// keeps it as what Soulseek judges by.
    #[tokio::test]
    async fn lidarrs_guessed_group_goes_only_onto_a_record_with_no_reference() {
        let serve = served();
        let cat = serve.catalogue();
        let paged = record_of_album(cat, None, "Glass Harbour", "Two Rooms", None, None).unwrap();
        assert!(cat.set_reference(&crate::serve::ids::record(paged), &crate::catalogue::records::Ref::new("bandcamp", "album", "https://glassharbour.bandcamp.com/album/two-rooms"), "link").unwrap());
        let bare = record_of_album(cat, None, "Glass Harbour", "Quiet Lanes", None, None).unwrap();
        let lidarr = Wanting::default();
        for record in [paged, bare] {
            let mut r = asked_for(record);
            r.id = serve.userdata.add_request(&r).unwrap();
            hand_to(Some(&lidarr), false, &serve, &r).await;
        }
        let groups = |record: i64| cat.references_of(&crate::serve::ids::record(record)).unwrap().into_iter().filter(|(r, _)| r.kind == "release-group").count();
        assert_eq!((groups(paged), groups(bare)), (0, 1));
    }

    /// X1: a fetcher out of reach for over an hour shows on Health. Y12: by a fetcher's name of the test's own, which the tests handing
    /// requests to their stand-in Lidarr never touch, under the one test lock, put back as it was.
    #[test]
    fn a_dead_fetcher_shows_on_health() {
        const FETCHER: &str = "the test's fetcher";
        let _one = crate::app::switches::TEST_STATE.blocking_lock();
        let was = DOWN.lock().unwrap_or_else(|p| p.into_inner()).get(FETCHER).copied();
        fetcher_said_at(FETCHER, false, 1_000);
        fetcher_said_at(FETCHER, false, 2_000);
        let not_yet = fetchers_down(1_000 + DOWN_FOR);
        let down = fetchers_down(1_001 + DOWN_FOR);
        let f = crate::app::health::fetchers_down(&down, 1_001 + DOWN_FOR).unwrap();
        fetcher_said_at(FETCHER, true, 9_000);
        let reached = fetchers_down(99_999);
        if let Some(at) = was {
            fetcher_said_at(FETCHER, false, at);
        }
        assert!(not_yet.is_empty(), "not an hour yet");
        assert_eq!(down, [(FETCHER, 1_000)], "since the first time");
        assert_eq!((f.key.as_str(), f.count), ("requests:fetcher-down", 1));
        assert!(reached.is_empty(), "reached again");
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

    /// O3: Cratarr's own cover addresses are kept only in the shapes it makes, a Bandcamp album's included; one that climbs out of
    /// them with `..` is not.
    #[test]
    fn a_want_cover_takes_only_the_shapes_cratarr_makes() {
        let bandcamp = format!("/api/cover/bandcamp/{}", crate::bandcamp::track_id("https://glassharbour.bandcamp.com/album/two-rooms", 0));
        assert!(cover_kept(&bandcamp), "{bandcamp}");
        assert!(cover_kept("/api/covers/release-group/00000000-0000-4000-8000-000000000116"));
        for left in ["/api/covers/../stream/bandcamp/bc.x", "/api/covers/release-group/../../players/kitchen/connect", "/api/cover/bandcamp/../x", "/api/covers/", "/api/albums/cratarr/1/cover"] {
            assert!(!cover_kept(left), "{left}");
        }
        let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
        let record = record_of_album(&cat, None, "Glass Harbour", "Two Rooms", None, Some("/api/covers/../stream/bandcamp/bc.x".into())).unwrap();
        assert_eq!(cat.requested(&[record]).unwrap()[&record].row.cover, None, "checked on the way to the record");
    }

    /// Y5 (the review's finding 5): a request is never written with a cover that fails W6's check, whatever its record holds, and no
    /// view of one shows such a cover, the record's or a row's written before; a kept one shows.
    #[tokio::test]
    async fn a_request_keeps_and_shows_only_a_kept_cover() {
        let serve = served();
        let cat = serve.catalogue();
        let record = cat.record_for_tests("Two Rooms", 0);
        cat.db_for_tests().execute("UPDATE records SET cover = 'https://example.org/two-rooms.jpg' WHERE id = ?1", [record]).unwrap();
        assert_eq!(asked_of(cat, record, None, None).unwrap().cover, None, "never onto the row");
        let row = Request { cover: Some("https://example.org/old.jpg".into()), ..asked_for(record) };
        assert_eq!(views(&serve, None, vec![row.clone()]).await[0].request.cover, None, "neither the record's nor the row's");
        let kept = "/api/covers/release-group/00000000-0000-4000-8000-000000000116";
        cat.db_for_tests().execute("UPDATE records SET cover = ?2 WHERE id = ?1", rusqlite::params![record, kept]).unwrap();
        assert_eq!(views(&serve, None, vec![row]).await[0].request.cover.as_deref(), Some(kept), "a kept one shows");
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

    /// C54: a record not had's page shows the account's newest request of it, whatever its status, as the Requests page shows it, by
    /// the record each asked for as merged now (G3); another account's, and its own of another record, are not its.
    #[tokio::test]
    async fn a_record_not_had_shows_its_request() {
        let serve = served();
        let cat = serve.catalogue();
        let (record, merged, other) = (cat.record_for_tests("Quiet Lanes", 0), cat.record_for_tests("Quiet Lanes (Deluxe)", 0), cat.record_for_tests("Two Rooms", 0));
        let add = |r: Request| Request { id: serve.userdata.add_request(&r).unwrap(), ..r };
        add(Request { status: CANCELLED.into(), at: 1, album_key: "k1".into(), ..asked_for(record) });
        let newest = add(Request { at: 2, album_key: "k2".into(), ..asked_for(merged) });
        add(Request { user: "ann".into(), at: 3, album_key: "k3".into(), ..asked_for(record) });
        let elsewhere = add(Request { at: 4, album_key: "k4".into(), ..asked_for(other) });
        cat.merge_records(merged, record).unwrap();
        let v = newest_of_record(&serve, None, "bob", record).await.unwrap().expect("bob asked for it");
        assert_eq!((v.request.id, v.request.record), (newest.id, Some(record)), "the newest, by its record as merged now");
        assert!(matches!(v.state, RequestState::Wanted { .. }) && v.hand_over_again, "as the Requests page shows it");
        assert_eq!(newest_of_record(&serve, None, "bob", other).await.unwrap().map(|v| v.request.id), Some(elsewhere.id));
        assert!(newest_of_record(&serve, None, "carol", record).await.unwrap().is_none(), "carol asked for nothing");
    }

    /// G11: a request just asked for is answered as the Requests page shows it, where it stands with it.
    #[tokio::test]
    async fn a_request_just_asked_for_says_where_it_stands() {
        let serve = served();
        let record = serve.catalogue().record_for_tests("Quiet Lanes", 0);
        let asked = Request { status: PENDING.into(), ..asked_for(record) };
        let asked = Request { id: serve.userdata.add_request(&asked).unwrap(), ..asked };
        let v = viewed(&serve, None, "bob", &asked).await.expect("viewed");
        assert_eq!((v.request.id, v.state), (asked.id, RequestState::WaitingForApproval));
        assert!(viewed(&serve, None, "bob", &Request { record: None, ..asked }).await.is_none(), "no record: as asked");
    }

    /// G3: a view names its album by its record, else by its release group: the request of the record that group names is found
    /// either way; nothing by neither, or by a group no record holds.
    #[tokio::test]
    async fn a_request_is_found_by_its_record_or_its_group() {
        let serve = served();
        let cat = serve.catalogue();
        let group = "00000000-0000-4000-8000-000000000392";
        let record = cat.record_for_tests("Quiet Lanes", 0);
        assert!(cat.set_reference(&crate::serve::ids::record(record), &crate::catalogue::records::Ref::new("musicbrainz", "release-group", group), "link").unwrap());
        let asked = serve.userdata.add_request(&Request { album_key: group.into(), release_group: Some(group.into()), ..asked_for(record) }).unwrap();
        let found = |record: Option<i64>, group: Option<&str>| {
            let serve = serve.clone();
            let group = group.map(str::to_string);
            async move { newest_of(&serve, None, "bob", record, group.as_deref()).await.unwrap().map(|v| v.request.id) }
        };
        assert_eq!(found(Some(record), None).await, Some(asked), "by its record");
        assert_eq!(found(None, Some(group)).await, Some(asked), "by its group's record");
        assert_eq!(found(None, Some("00000000-0000-4000-8000-000000000393")).await, None, "a group no record holds");
        assert_eq!(found(None, None).await, None);
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
        reject(Some(&lidarr), &serve.userdata, &arrived, None).await;
        let closed = serve.userdata.request(r.id).unwrap().unwrap();
        assert_eq!((closed.status.as_str(), closed.reason.as_deref()), (REJECTED, Some("Rejected")));
        assert_eq!(*lidarr.1.lock().unwrap(), [group.to_string()], "Lidarr told to stop, by the record's group");
        assert!(open_request(&serve, "bob", &r.album_key).unwrap().is_none(), "wanted again, a new request");
    }

    /// Y4 (the review's finding 4): a requester's Delete of the copy that arrived closes their own request alone, as rejected; the
    /// other asker's stays open as it was, and keeps a fetcher (V2: `a_requesters_delete_leaves_the_other_asker_a_fetcher`). An
    /// administrator's Delete closes every asker's. Both asked by the album's one release group, as in life (V2).
    #[tokio::test]
    async fn a_requesters_delete_closes_only_their_request() {
        let serve = served();
        let group = "00000000-0000-4000-8000-000000000308";
        let record = record_of_album(serve.catalogue(), Some(group), "Glass Harbour", "Two Rooms", None, None).unwrap();
        let ask = |user: &str| {
            let r = Request { user: user.into(), album_key: format!("k-{user}"), release_group: Some(group.into()), at: 100, ..asked_for(record) };
            Request { id: serve.userdata.add_request(&r).unwrap(), ..r }
        };
        let (ann, bob) = (ask("ann"), ask("bob"));
        let arrived = arrived_for(&serve, record, Some(200)).await;
        assert_eq!(arrived.len(), 2, "it arrived for both");
        let closed = reject(None, &serve.userdata, &closed_by(arrived.clone(), false, Some("ann")), None).await;
        assert_eq!(closed.iter().map(|c| (c.id, c.status.as_str())).collect::<Vec<_>>(), [(ann.id, APPROVED)], "Ann's, as it was");
        let status = |id: i64| serve.userdata.request(id).unwrap().unwrap().status;
        assert_eq!((status(ann.id), status(bob.id)), (REJECTED.to_string(), APPROVED.to_string()), "Bob's stays open");
        assert!(closed_by(arrived.clone(), false, None).is_empty(), "an account not known closes none");
        assert_eq!(closed_by(arrived, true, Some("root")).len(), 2, "an administrator's: every asker's");
    }

    /// V2 (the review of run 8b, finding 2): Ann and Bob asked for one album, by its one release group, and Lidarr was told to stop
    /// when it arrived. Ann's Delete of the arrival closes hers alone and leaves Bob a fetcher: Lidarr is not told to stop, hers says it
    /// goes on, and his is handed to Lidarr again, its "told to stop: it arrived" gone. An administrator's Delete of the next arrival,
    /// no request left, tells Lidarr to stop.
    #[tokio::test]
    async fn a_requesters_delete_leaves_the_other_asker_a_fetcher() {
        let serve = served();
        let group = "00000000-0000-4000-8000-000000000310";
        let record = record_of_album(serve.catalogue(), Some(group), "Glass Harbour", "Two Rooms", None, None).unwrap();
        let ask = |user: &str| {
            let fetched = Some(format!("Lidarr added it and is looking for it; {STOPPED_ON_ARRIVAL}"));
            let r = Request { user: user.into(), album_key: format!("k-{user}"), release_group: Some(group.into()), at: 100, fetched, ..asked_for(record) };
            Request { id: serve.userdata.add_request(&r).unwrap(), ..r }
        };
        let (ann, bob) = (ask("ann"), ask("bob"));
        let (arrived, open) = (arrived_for(&serve, record, Some(200)).await, open_of(&serve, record).await);
        assert_eq!((arrived.len(), open.len()), (2, 2), "both asked for it, by one group");
        let lidarr = Wanting::default();
        assert!(deleted(Some(&lidarr), true, &serve, &closed_by(arrived, false, Some("ann")), open, Some(&lidarrs_copy()), None).await, "Lidarr goes on");
        assert_eq!(*lidarr.3.lock().unwrap(), [format!("failed {group}"), "want".to_string()], "U5: its grab marked failed first");
        assert!(lidarr.1.lock().unwrap().is_empty(), "Lidarr not told to stop: Bob's still wants it");
        assert_eq!(*lidarr.0.lock().unwrap(), [(Some(group.to_string()), "Glass Harbour".to_string(), "Two Rooms".to_string())], "Bob's handed to it again");
        let now = |id: i64| serve.userdata.request(id).unwrap().unwrap();
        assert_eq!((now(ann.id).status, now(bob.id).status), (REJECTED.to_string(), APPROVED.to_string()));
        assert!(now(ann.id).fetched.unwrap().ends_with(LIDARR_KEPT), "hers says Lidarr goes on");
        assert_eq!(now(bob.id).fetched.unwrap(), format!("Lidarr added it and is looking for it; {SOULSEEK_SEARCHES}"), "his no longer told to stop");
        let (arrived, open) = (arrived_for(&serve, record, Some(200)).await, open_of(&serve, record).await);
        assert!(!deleted(Some(&lidarr), false, &serve, &closed_by(arrived, true, Some("root")), open, Some(&lidarrs_copy()), None).await, "none left");
        assert_eq!(*lidarr.1.lock().unwrap(), [group.to_string()], "Lidarr told to stop");
        assert_eq!(now(bob.id).status, REJECTED);
    }

    /// U5 (the review of 8c/8d, finding 5): Ann's Delete of a copy Lidarr brought, Bob's request left, and Lidarr's history holds no
    /// grab of it: Lidarr stays stopped (told so for Ann's, never asked to want it again), and Soulseek alone looks for Bob's. A copy
    /// Soulseek brought leaves Lidarr to go on, no grab asked after.
    #[tokio::test]
    async fn a_lidarr_copy_whose_grab_is_not_found_leaves_lidarr_stopped() {
        let serve = served();
        let group = "00000000-0000-4000-8000-000000000312";
        let record = record_of_album(serve.catalogue(), Some(group), "Glass Harbour", "Two Rooms", None, None).unwrap();
        let ask = |user: &str| {
            let fetched = Some(format!("Lidarr added it and is looking for it; {STOPPED_ON_ARRIVAL}"));
            let r = Request { user: user.into(), album_key: format!("k-{user}"), release_group: Some(group.into()), at: 100, fetched, ..asked_for(record) };
            Request { id: serve.userdata.add_request(&r).unwrap(), ..r }
        };
        let (ann, bob) = (ask("ann"), ask("bob"));
        let (arrived, open) = (arrived_for(&serve, record, Some(200)).await, open_of(&serve, record).await);
        let lidarr = Wanting::default();
        lidarr.4.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(!deleted(Some(&lidarr), true, &serve, &closed_by(arrived, false, Some("ann")), open, Some(&lidarrs_copy()), None).await, "Lidarr stays stopped");
        assert_eq!(*lidarr.3.lock().unwrap(), [format!("failed {group}")], "asked for its grab, never to want it");
        assert_eq!(*lidarr.1.lock().unwrap(), [group.to_string()], "told to stop for Ann's");
        let now = |id: i64| serve.userdata.request(id).unwrap().unwrap();
        assert_eq!((now(ann.id).status, now(bob.id).status), (REJECTED.to_string(), APPROVED.to_string()));
        assert_eq!(now(bob.id).fetched.unwrap(), SOULSEEK_SEARCHES, "Soulseek alone looks for Bob's");
        // A copy Soulseek brought: Lidarr goes on for the one left, no grab asked after.
        let carol = ask("carol");
        let (arrived, open) = (arrived_for(&serve, record, Some(200)).await, open_of(&serve, record).await);
        let lidarr = Wanting::default();
        assert!(deleted(Some(&lidarr), true, &serve, &closed_by(arrived, false, Some("bob")), open, Some(&soulseeks_copy()), None).await);
        assert_eq!(*lidarr.3.lock().unwrap(), ["want".to_string()], "no grab asked after: Carol's handed to it");
        assert_eq!(now(carol.id).status, APPROVED);
    }

    /// U5: only an approved request left keeps a fetcher. A pending one alone is no reason to fetch: Ann's Delete of her arrival tells
    /// Lidarr to stop, and Bob's pending request stays as it is, handed to no one.
    #[tokio::test]
    async fn a_delete_leaves_no_fetcher_for_a_pending_request_alone() {
        let serve = served();
        let group = "00000000-0000-4000-8000-000000000313";
        let record = record_of_album(serve.catalogue(), Some(group), "Glass Harbour", "Two Rooms", None, None).unwrap();
        let ask = |user: &str, status: &str| {
            let r = Request { user: user.into(), album_key: format!("k-{user}"), release_group: Some(group.into()), at: 100, status: status.into(), ..asked_for(record) };
            Request { id: serve.userdata.add_request(&r).unwrap(), ..r }
        };
        let (_ann, bob) = (ask("ann", APPROVED), ask("bob", PENDING));
        let (arrived, open) = (arrived_for(&serve, record, Some(200)).await, open_of(&serve, record).await);
        assert_eq!((arrived.len(), open.len()), (1, 2), "arrived for Ann's; both open");
        let lidarr = Wanting::default();
        assert!(!deleted(Some(&lidarr), false, &serve, &closed_by(arrived, false, Some("ann")), open, Some(&lidarrs_copy()), None).await, "no fetcher goes on");
        assert_eq!(*lidarr.1.lock().unwrap(), [group.to_string()], "Lidarr told to stop");
        assert!(lidarr.3.lock().unwrap().is_empty(), "no grab marked, nothing wanted");
        let now = serve.userdata.request(bob.id).unwrap().unwrap();
        assert_eq!((now.status.as_str(), now.fetched), (PENDING, None), "Bob's pending, as it was");
    }

    /// U5: the requests left are read again once the copy has gone: Bob's, cancelled while the Delete ran, is not handed to Lidarr
    /// again, and Lidarr is told to stop.
    #[tokio::test]
    async fn a_request_cancelled_during_a_delete_is_not_handed_again() {
        let serve = served();
        let group = "00000000-0000-4000-8000-000000000314";
        let record = record_of_album(serve.catalogue(), Some(group), "Glass Harbour", "Two Rooms", None, None).unwrap();
        let ask = |user: &str| {
            let r = Request { user: user.into(), album_key: format!("k-{user}"), release_group: Some(group.into()), at: 100, ..asked_for(record) };
            Request { id: serve.userdata.add_request(&r).unwrap(), ..r }
        };
        let (_ann, bob) = (ask("ann"), ask("bob"));
        let (arrived, open) = (arrived_for(&serve, record, Some(200)).await, open_of(&serve, record).await);
        assert_eq!(open.len(), 2);
        assert!(serve.userdata.set_request_status(bob.id, APPROVED, CANCELLED, None, None).unwrap(), "Bob takes his back meanwhile");
        let lidarr = Wanting::default();
        assert!(!deleted(Some(&lidarr), false, &serve, &closed_by(arrived, false, Some("ann")), open, Some(&lidarrs_copy()), None).await);
        assert!(lidarr.0.lock().unwrap().is_empty() && lidarr.3.lock().unwrap().is_empty(), "nothing handed again");
        assert_eq!(*lidarr.1.lock().unwrap(), [group.to_string()], "Lidarr told to stop");
        assert_eq!(serve.userdata.request(bob.id).unwrap().unwrap().status, CANCELLED);
    }

    /// U7 (the review of 8c/8d, finding 7): a record holding no copy says whether its last one went to an Incoming library's trash:
    /// deleted from Incoming, yes; kept into Music and deleted there later, no; never in a trash, no.
    #[test]
    fn a_record_says_whether_its_last_copy_went_from_incoming() {
        use crate::settings::LibraryKind;
        let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
        let (incoming, kept, never) = (cat.record_for_tests("Two Rooms", 2), cat.record_for_tests("Signal", 2), cat.record_for_tests("Lamplight", 0));
        for r in [incoming, kept] {
            cat.copy_for_tests(r, 1);
        }
        cat.mark_trashed(1, &format!("copy-{incoming}"), &format!("100/copy-{incoming}")).unwrap();
        cat.move_files(1, &format!("copy-{kept}"), 0, &format!("copy-{kept}")).unwrap();
        cat.mark_trashed(0, &format!("copy-{kept}"), &format!("200/copy-{kept}")).unwrap();
        let rs: Vec<Request> = [incoming, kept, never].into_iter().map(asked_for).collect();
        let kinds = [(0, LibraryKind::Main), (1, LibraryKind::Incoming)];
        let mut of = records_held(&cat, &rs, &kinds).unwrap();
        mark_gone_from_incoming(&cat, &mut of, &kinds).unwrap();
        let read: Vec<(Holding, bool)> = of.iter().map(|o| o.as_ref().map(|o| (o.holding, o.gone_from_incoming)).unwrap()).collect();
        assert_eq!(read, [(Holding::None, true), (Holding::None, false), (Holding::None, false)]);
    }

    /// A deleted copy in a library naming Lidarr (U5's tests): Lidarr's whatever its history says.
    fn lidarrs_copy() -> DeletedCopy {
        DeletedCopy { fetcher: Some("lidarr".into()), folder: "Glass Harbour/Two Rooms".into() }
    }

    /// A deleted copy in Soulseek's library: Lidarr is not asked of it.
    fn soulseeks_copy() -> DeletedCopy {
        DeletedCopy { fetcher: Some(super::super::soulseek::FETCHER.into()), folder: "Glass Harbour/Two Rooms".into() }
    }

    /// T9's strict Lidarr (`lidarr::fake`) as the fetcher a Delete tells, each call given up after `wait` where given.
    fn faked(url: &str, wait: Option<std::time::Duration>) -> crate::ports::LidarrFetcher {
        let lidarr = match wait {
            Some(wait) => crate::lidarr::Lidarr::waiting(url, "key", wait),
            None => crate::lidarr::Lidarr::new(url, "key"),
        };
        crate::ports::LidarrFetcher { lidarr: lidarr.unwrap(), with: Default::default() }
    }

    /// T9's shape: Ann and Bob ask for Salt Meridian's Both Shores by the release group Lidarr knows (`fake::GROUP`), Lidarr told to
    /// stop when it arrived for both. Ann's and Bob's requests, those it arrived for, and those open.
    async fn both_shores_asked(serve: &Serve) -> (Request, Request, Vec<Request>, Vec<Request>) {
        let group = crate::lidarr::fake::GROUP;
        let record = record_of_album(serve.catalogue(), Some(group), "Salt Meridian", "Both Shores", Some(2018), None).unwrap();
        let ask = |user: &str| {
            let fetched = Some(format!("Lidarr added it and is looking for it; {STOPPED_ON_ARRIVAL}"));
            let r = Request { user: user.into(), album_key: format!("k-{user}"), release_group: Some(group.into()), at: 100, fetched, ..asked_for(record) };
            Request { id: serve.userdata.add_request(&r).unwrap(), ..r }
        };
        let (ann, bob) = (ask("ann"), ask("bob"));
        let (arrived, open) = (arrived_for(serve, record, Some(200)).await, open_of(serve, record).await);
        assert_eq!((arrived.len(), open.len()), (2, 2), "it arrived for both, asked by one group");
        (ann, bob, arrived, open)
    }

    /// T9.1, T9.4 (the review of run 10, finding 9): Ann deletes her arrival, Bob still asking. Lidarr's history holds grab 50, D1's,
    /// whose tracks it imported into the copy's folder as its own container names it (`/data/music/...`; Cratarr's copy at
    /// `Salt Meridian/Both Shores (2018)` in a library naming no fetcher, OQ-J), and grab 52, D2's, newer and still downloading for
    /// Bob: exactly one grab is marked failed, 50, and Lidarr goes on for Bob. Lidarr away, answering 503 or not in time: no grab
    /// marked, Lidarr stays stopped, Soulseek alone looks for Bob's, and the warning is logged once.
    #[tokio::test]
    async fn only_the_grab_whose_import_is_the_deleted_copy_is_marked_failed() {
        use crate::lidarr::fake;
        let copy = DeletedCopy { fetcher: None, folder: "Salt Meridian/Both Shores (2018)".into() };
        let imported = || fake::imports("D1", "/data/music/Salt Meridian/Both Shores (2018)", "2026-11-10");
        let now = |serve: &Serve, id: i64| serve.userdata.request(id).unwrap().unwrap();
        let serve = served();
        let (ann, bob, arrived, open) = both_shores_asked(&serve).await;
        let (url, seen) = fake::lidarr(fake::Mode::Answers, imported()).await;
        assert!(deleted(Some(&faked(&url, None)), true, &serve, &closed_by(arrived, false, Some("ann")), open, Some(&copy), None).await, "Lidarr goes on for Bob");
        assert_eq!(seen.writes(), ["failed 50", "monitor [7] true", "search [7]"], "D1's grab alone marked, never 52; Bob's handed to Lidarr again");
        assert_eq!(seen.reads(), ["history 3", "history 1"]);
        assert!(seen.refused().is_empty(), "{:?}", seen.refused());
        assert_eq!((now(&serve, ann.id).status, now(&serve, bob.id).status), (REJECTED.to_string(), APPROVED.to_string()));
        assert_eq!(now(&serve, bob.id).fetched.unwrap(), format!("Lidarr is looking for it; {SOULSEEK_SEARCHES}"));
        for mode in [fake::Mode::Away, fake::Mode::Slow] {
            let serve = served();
            let (ann, bob, arrived, open) = both_shores_asked(&serve).await;
            let (url, seen) = fake::lidarr(mode, imported()).await;
            let log = Logged::default();
            let writer = log.clone();
            let _logging = crate::diagnostics::log_here(tracing_subscriber::fmt().with_writer(move || writer.clone()).with_ansi(false).with_max_level(tracing::Level::INFO).finish());
            let lidarr = faked(&url, Some(std::time::Duration::from_millis(300)));
            assert!(!deleted(Some(&lidarr), true, &serve, &closed_by(arrived, false, Some("ann")), open, Some(&copy), None).await, "Lidarr stays stopped ({mode:?})");
            assert!(seen.writes().is_empty(), "no grab marked, nothing wanted ({mode:?}): {:?}", seen.writes());
            assert_eq!((now(&serve, ann.id).status, now(&serve, bob.id).status), (REJECTED.to_string(), APPROVED.to_string()));
            assert_eq!(now(&serve, bob.id).fetched.unwrap(), SOULSEEK_SEARCHES, "Soulseek alone looks for Bob's ({mode:?})");
            let text = log.text();
            let warned: Vec<&str> = text.lines().filter(|l| l.contains("WARN")).collect();
            assert_eq!(warned.len(), 1, "the warning once ({mode:?}): {text}");
            assert!(warned[0].contains("Lidarr stays stopped"), "{text}");
        }
    }

    /// T9.2: files dropped by hand into Lidarr's library at `Salt Meridian/Both Shores (2018)`, Lidarr's history holding imports of the
    /// album elsewhere only (a folder whose name begins alike, another artist's folder of that name): no grab marked, Lidarr stays
    /// stopped, Soulseek looks for Bob's. Alike where the library names Lidarr and where it names no fetcher (staging's
    /// `lidarr-music`).
    #[tokio::test]
    async fn a_hand_dropped_copy_marks_no_grab() {
        use crate::lidarr::fake;
        for fetcher in [Some("lidarr"), None] {
            let serve = served();
            let (ann, bob, arrived, open) = both_shores_asked(&serve).await;
            let elsewhere = [
                fake::imports("D1", "/data/music/Salt Meridian/Both Shores (2018) (Deluxe)", "2026-11-10"),
                fake::imports("D3", "/data/music/Other Artist/Both Shores (2018)", "2026-11-11"),
            ]
            .concat();
            let (url, seen) = fake::lidarr(fake::Mode::Answers, elsewhere).await;
            let copy = DeletedCopy { fetcher: fetcher.map(str::to_string), folder: "Salt Meridian/Both Shores (2018)".into() };
            assert!(!deleted(Some(&faked(&url, None)), true, &serve, &closed_by(arrived, false, Some("ann")), open, Some(&copy), None).await, "Lidarr stays stopped ({fetcher:?})");
            assert!(seen.writes().is_empty(), "no POST, nothing wanted ({fetcher:?}): {:?}", seen.writes());
            assert_eq!(seen.reads(), ["history 3"], "its imports read, no grab ({fetcher:?})");
            assert!(seen.refused().is_empty(), "{:?}", seen.refused());
            let now = |id: i64| serve.userdata.request(id).unwrap().unwrap();
            assert_eq!((now(ann.id).status, now(bob.id).status), (REJECTED.to_string(), APPROVED.to_string()));
            assert_eq!(now(bob.id).fetched.unwrap(), SOULSEEK_SEARCHES, "Soulseek looks for Bob's ({fetcher:?})");
        }
    }

    /// T9.3: whose a deleted Incoming copy is. (a) Its library names Lidarr: Lidarr's whatever the history says (here it imported
    /// elsewhere), its imports read for its grab. (b) Its library names no fetcher (staging's `lidarr-music`) and Lidarr imported into
    /// its folder: Lidarr's, its grab marked. (c) Names none, not in the history: no fetcher's, the one read saying so and nothing
    /// more. (d) Soulseek's library: Soulseek's, Lidarr not asked.
    #[tokio::test]
    async fn a_copy_is_lidarrs_by_its_library_or_its_import() {
        use crate::lidarr::fake;
        let here = || fake::imports("D1", "/data/music/Salt Meridian/Both Shores (2018)", "2026-11-10");
        let elsewhere = || fake::imports("D1", "/data/music/Salt Meridian/Other Shores (2019)", "2026-11-10");
        let cases = [
            (Some("lidarr"), elsewhere(), Whose::Lidarr { failed: false }, vec!["history 3"], vec![]),
            (None, here(), Whose::Lidarr { failed: true }, vec!["history 3", "history 1"], vec!["failed 50"]),
            (None, elsewhere(), Whose::Nobody, vec!["history 3"], vec![]),
            (Some("soulseek"), here(), Whose::Another, vec![], vec![]),
        ];
        for (fetcher, imports, is, reads, writes) in cases {
            let (url, seen) = fake::lidarr(fake::Mode::Answers, imports).await;
            let copy = DeletedCopy { fetcher: fetcher.map(str::to_string), folder: "Salt Meridian/Both Shores (2018)".into() };
            assert_eq!(whose(Some(&faked(&url, None)), &copy, Some(fake::GROUP)).await, is, "{fetcher:?}");
            assert_eq!(seen.reads(), reads, "{fetcher:?}");
            assert_eq!(seen.writes(), writes, "{fetcher:?}");
            assert!(seen.refused().is_empty(), "{fetcher:?}: {:?}", seen.refused());
        }
    }

    /// V9 (the review of run 8b's nits): an Undo whose request meets a newer open one of the account's of its record leaves it
    /// closed, quietly: no write the one-open-request rule refuses (W6), and the newer one as it is.
    #[test]
    fn an_undo_meeting_a_newer_open_request_leaves_it_closed_quietly() {
        let ud = crate::userdata::UserData::open(None).unwrap();
        let ask = || {
            let r = Request { user: "ann".into(), ..asked_for(1) };
            Request { id: ud.add_request(&r).unwrap(), ..r }
        };
        let old = ask();
        assert!(ud.set_request_status(old.id, APPROVED, REJECTED, Some(REJECTED_WHY), None).unwrap());
        let newer = ask();
        let restored = ud.restore_request_status(old.id, REJECTED, APPROVED, None, None, None);
        assert!(restored.is_ok_and(|written| !written), "not written, and no error");
        let closed = Closed { id: old.id, status: APPROVED.into(), reason: None, decided_by: None, decided_at: None };
        assert!(reopen(&ud, &[closed]).is_empty());
        let status = |id: i64| ud.request(id).unwrap().unwrap().status;
        assert_eq!((status(old.id), status(newer.id)), (REJECTED.to_string(), APPROVED.to_string()));
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

    /// A log the test reads: what a `fmt` subscriber writes.
    #[derive(Clone, Default)]
    struct Logged(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Logged {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Logged {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    /// T5.5: a Want whose copy cannot be made (its own music not made, its copies not read, its copy not listed) says why in the
    /// log, as a warning of `requests` naming its request and record, and on its line, never "Already in the library", and asks no
    /// fetcher of a record held; no own music, or a copy there already, says nothing new.
    #[tokio::test]
    async fn a_want_that_cannot_copy_says_why_in_the_log() {
        let serve = served();
        let owned = serve.catalogue().record_for_tests("Harbour Lights", 2);
        let mut r = asked_for(owned);
        r.id = serve.userdata.add_request(&r).unwrap();
        let log = Logged::default();
        let writer = log.clone();
        let subscriber = tracing_subscriber::fmt().with_writer(move || writer.clone()).with_ansi(false).with_max_level(tracing::Level::INFO).finish();
        let _logging = tracing::subscriber::set_default(subscriber);
        let lidarr = Wanting::default();
        for why in ["its own music could not be made: no room", "its copies could not be read: the catalogue is busy", "its copies could not be read: its record is not listed"] {
            let said = hand_after(Some(&lidarr), false, &serve, &r, WantCopy::Failed(why.into())).await;
            assert_eq!(said, format!("{NOT_COPIED}: {why}"));
            assert_ne!(said, ALREADY_HAD);
            let text = log.text();
            assert!(text.lines().any(|l| l.contains("WARN") && l.contains("requests") && l.contains(why) && l.contains(&format!("record {owned}"))), "{text}");
        }
        assert!(lidarr.0.lock().unwrap().is_empty(), "held: no fetcher asked");
        log.0.lock().unwrap().clear();
        assert_eq!(hand_after(Some(&lidarr), false, &serve, &r, WantCopy::Nothing).await, ALREADY_HAD, "no own music, or one there already: as before");
        assert!(!log.text().contains("WARN"), "and nothing to warn of: {}", log.text());
    }

    /// S.2 (cases 1 and 2): Music's lone copy whose three files each say eight tracks is short by its clean tags (read from the
    /// catalogue as the choice reads them), though `copies.short` says whole: no copy for Ann, and her Want goes to the fetchers
    /// as a record not held whole, never "Copied into your music". Ten files with no totals cannot say: copied, as today.
    #[tokio::test]
    async fn a_want_copies_only_a_copy_not_known_short() {
        use crate::catalogue::identity::{Judged, lacks};
        use crate::serve::copies::{Copy, Kind};
        use super::super::albums::{Chosen, want_copy};
        let serve = served();
        let owned = serve.catalogue().record_for_tests("Other Room", 8);
        // Music's three files saying eight (folder 0), and five more saying no total (folder 1).
        for (folder, dir, n, number) in [(0, "A", 1, "1/8"), (0, "A", 2, "2/8"), (0, "A", 3, "3/8"), (1, "B", 4, "1"), (1, "B", 5, "2")] {
            let rel = format!("{dir}/Other Room/{n}.mp3");
            serve.catalogue().execute_for_tests(&format!("INSERT INTO files (folder, rel, size, mtime, track) VALUES ({folder}, '{rel}', 1, 1, {n})"));
            serve.catalogue().execute_for_tests(&format!(
                "INSERT INTO tags (folder, rel, n, raw, name, value) VALUES ({folder}, '{rel}', 0, 'tracknumber', 'tracknumber', '{number}'), ({folder}, '{rel}', 1, 'discnumber', 'discnumber', '1')"
            ));
        }
        let lone = |folder: i64, dir: &str| Copy { folder, dir: dir.into(), kind: Kind::Main, order: 0, format: Some("MP3".into()), min_kbps: Some(320), bits: None, rate: None, short: false, added: Some(1) };
        let judged = |folder: i64, dir: &str| lacks(&Judged { files: serve.catalogue().numbers_under(folder, dir).unwrap().into_iter().map(|(_, n)| n).collect(), ..Judged::default() });
        // 1. Three files, each saying eight: short.
        assert_eq!(serve.catalogue().numbers_under(0, "A/Other Room").unwrap().len(), 3);
        assert_eq!(want_copy(&[(lone(0, "A/Other Room"), judged(0, "A/Other Room"))], Some(2), &[]), Chosen::NoneWhole);
        let mut r = asked_for(owned);
        r.id = serve.userdata.add_request(&r).unwrap();
        let lidarr = Wanting::default();
        let said = hand_after(Some(&lidarr), false, &serve, &r, WantCopy::NoneWhole).await;
        assert!(said != COPIED_INTO_OWN && said != ALREADY_HAD, "{said}");
        assert_eq!(lidarr.0.lock().unwrap().len(), 1, "to the fetchers, as a record not held whole");
        // 2. Files with no totals: the tags cannot say, which copies.
        assert_eq!(want_copy(&[(lone(1, "B/Other Room"), judged(1, "B/Other Room"))], Some(2), &[]), Chosen::From(1, "B/Other Room".into()));
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
        super::super::step_017::run(serve.catalogue(), &serve.userdata, &[], &crate::catalogue::Rules::default()).unwrap();
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

    /// The Lidarr a test hands requests to: what it was asked to want, and to stop wanting; the release group it finds; whether it
    /// is out of reach (the third); its grabs marked failed and its wants, in order (the fourth, U5); and whether it finds no grab to
    /// mark (the fifth).
    #[derive(Default)]
    struct Wanting(Mutex<Vec<(Option<String>, String, String)>>, Mutex<Vec<String>>, std::sync::atomic::AtomicBool, Mutex<Vec<String>>, std::sync::atomic::AtomicBool);

    const FOUND: &str = "00000000-0000-4000-8000-000000000131";

    impl Fetcher for Wanting {
        fn want<'a>(&'a self, release_group: Option<&'a str>, artist: &'a str, album: &'a str) -> crate::source::Fut<'a, crate::lidarr::Handed> {
            self.0.lock().unwrap().push((release_group.map(str::to_string), artist.to_string(), album.to_string()));
            self.3.lock().unwrap().push("want".into());
            Box::pin(async { Ok(crate::lidarr::Handed { said: "Lidarr added it and is looking for it".into(), release_group: FOUND.into() }) })
        }

        fn statuses<'a>(&'a self, _: &'a [String]) -> crate::source::Fut<'a, HashMap<String, Progress>> {
            Box::pin(async { Ok(HashMap::new()) })
        }

        fn cancel<'a>(&'a self, release_group: &'a str) -> crate::source::Fut<'a, ()> {
            if self.2.load(std::sync::atomic::Ordering::SeqCst) {
                return Box::pin(async { Err(anyhow::anyhow!("Lidarr could not be reached")) });
            }
            self.1.lock().unwrap().push(release_group.to_string());
            Box::pin(async { Ok(()) })
        }

        fn fail_grab<'a>(&'a self, release_group: &'a str, _: &'a str) -> crate::source::Fut<'a, crate::lidarr::Imported> {
            self.3.lock().unwrap().push(format!("failed {release_group}"));
            let found = if self.4.load(std::sync::atomic::Ordering::SeqCst) { crate::lidarr::Imported::Not } else { crate::lidarr::Imported::Failed };
            Box::pin(async move { Ok(found) })
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
        assert_eq!((kept.artist.as_str(), kept.album.as_str(), kept.release_group.as_deref()), ("Paper Lantern", "Tape Club - After Hours (Paper Lantern)", Some(FOUND)), "as asked, the group Lidarr took it by kept (X1)");
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
