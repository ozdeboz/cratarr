//! Requests (decision 0070): an album asked for from Discover is kept by Cratarr with its own status (pending,
//! approved, declined, cancelled); an approved one is handed to a fetcher (`ports::Fetcher`, Lidarr first), and the
//! album's own state (being fetched, in Incoming, in the library) read beside it. Seerr's model (MIT,
//! github.com/seerr-team/seerr), not its code. Who may ask and answer everyone's: the account's permissions
//! (`serve::users::Permissions`), an administrator's all. Since 0.17 (AL02, the owner's "No approval per request") "Request albums"
//! is the gate: a Want goes ahead with no one's yes, and its asker's room is a brake, not an approval: over it, a new Want waits,
//! saying so, and goes by itself at the requests' round once they are under, oldest first. A request done in a library is
//! delivered ((c)): kept in the history, it pulls no copy, room, right or search any more. Below the disk floor (AL03, `room::space`)
//! a user's new Want waits for space the same way, and one Lidarr only looks for is told to stop till there is space; an
//! administrator's goes on.

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
/// Closed once its album is done in a library for its asker (AL02.8, the owner's (c) "Closes when done"): it stays in the history,
/// and pulls no copy, room, right or Soulseek search any more; wanted again, it is a new request.
pub const DELIVERED: &str = "delivered";
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
    /// Its asker is over their room (AL02.2): handed to no fetcher and copied from nothing till they are under. Their room's
    /// numbers, in bytes, where a view read them (`views_of`).
    WaitingForRoom {
        used: u64,
        room: u64,
    },
    /// The server is short of space (AL03): a user's Want handed to no fetcher, or told to stop, till there is space.
    WaitingForSpace,
    Declined,
    Cancelled,
    Rejected,
    /// Done in a library for its asker, and closed ((c), AL02.8).
    Delivered,
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

/// A request's state (W5): `RequestState`, from the request as kept, how its record is held for its asker (`holding`), Lidarr's
/// progress for its group, and Soulseek's word of its item. A request of an album the asker has, or that arrived, reads so whatever
/// its answer was to be; one held only in another's own music is still wanted, for its asker's copy (I05). One waiting for room
/// reads so (AL02.2), its numbers 0 till a view gives them (`views_of`); one waiting for space too (AL03); either of them before the
/// album being done for its asker (RF17, the review of run 13, RM1: a Want the house meets waits for its asker's copy, though
/// Music's is in everyone's view). Pure.
pub fn request_state(r: &Request, held: Holding, lidarr: Option<&Progress>, soulseek: Option<&super::soulseek::Word>) -> RequestState {
    match r.status.as_str() {
        DECLINED => return RequestState::Declined,
        CANCELLED => return RequestState::Cancelled,
        REJECTED => return RequestState::Rejected,
        DELIVERED => return RequestState::Delivered,
        _ => {}
    }
    match held {
        Holding::Done if !(waits_for_room(r) || waits_for_space(r)) => RequestState::Done,
        Holding::Arrived => RequestState::Arrived,
        _ if r.status != APPROVED => RequestState::WaitingForApproval,
        _ if waits_for_room(r) => RequestState::WaitingForRoom { used: 0, room: 0 },
        _ if waits_for_space(r) => RequestState::WaitingForSpace,
        Holding::Done => RequestState::Done,
        Holding::Partly { has, of } => RequestState::PartlyThere { has, of },
        Holding::None | Holding::Elsewhere => {
            let lidarr = lidarr.and_then(|p| match *p {
                Progress::Looking => Some(Fetching::Looking),
                // Importing reads as fetching, as it did before Lidarr's step was read (RQ01.3: the story's part tells it).
                Progress::Fetching { done } | Progress::Importing { done } => Some(Fetching::Fetching { done }),
                Progress::Fetched { .. } => Some(Fetching::Fetching { done: 1.0 }),
                Progress::Stuck { .. } => Some(Fetching::Stuck),
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
        RequestState::Done | RequestState::Delivered => Album::InLibrary,
        RequestState::Arrived => Album::InIncoming,
        RequestState::PartlyThere { has, of } => Album::PartlyThere { tracks: *has, of: *of },
        RequestState::Wanted { lidarr: Some(Fetching::Fetching { done }), .. } => Album::Fetching { done: *done },
        RequestState::Wanted { soulseek: Some(Word::Fetching { percent }), .. } => Album::Fetching { done: (*percent / 100.0) as f32 },
        RequestState::Wanted { lidarr: Some(Fetching::Looking), .. } | RequestState::Wanted { soulseek: Some(Word::Searching | Word::Found), .. } => Album::Looking,
        RequestState::Wanted { lidarr: None, soulseek: None } => Album::Unknown,
        RequestState::Wanted { .. }
        | RequestState::WaitingForApproval
        | RequestState::WaitingForRoom { .. }
        | RequestState::WaitingForSpace
        | RequestState::Declined
        | RequestState::Cancelled
        | RequestState::Rejected => Album::Requested,
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

/// What a Want is handed with (AL02): Lidarr where it is connected, whether Soulseek is, the house's copy of a record held whole
/// into its asker's own music (C52, T5: `App::copy_for`), and the disks the floor reads (AL03, `room::Disks`). The tests' stand in
/// for each.
pub(crate) struct Hands<'a> {
    pub lidarr: Option<&'a dyn Fetcher>,
    pub soulseek: bool,
    pub house: &'a dyn House,
    pub disks: &'a dyn super::room::Disks,
}

/// The house's copy of a record into an account's own music (C52, T5): what it came to. `sharing`: made only where it shares its
/// blocks with what it is made of, taking no new space (below the floor, AL03.1); else not made (`WantCopy::NoSpace`).
pub(crate) trait House: Send + Sync {
    fn copy_into_own<'a>(&'a self, account: &'a str, record: i64, sharing: bool) -> std::pin::Pin<Box<dyn std::future::Future<Output = WantCopy> + Send + 'a>>;
}

impl House for super::App {
    fn copy_into_own<'a>(&'a self, account: &'a str, record: i64, sharing: bool) -> std::pin::Pin<Box<dyn std::future::Future<Output = WantCopy> + Send + 'a>> {
        Box::pin(self.copy_for(account, record, sharing))
    }
}

/// The fetchers as they are now, for a `Hands`: Lidarr where connected and switched on, and whether Soulseek is.
async fn fetchers(app: &super::App) -> (Option<Box<dyn Fetcher>>, bool) {
    (fetcher(app), super::soulseek::connected().await)
}

/// A suggestion of the account's Discover asked for: a Want, gone ahead (`ask_for`). Asked again, the request already open is the
/// answer.
pub async fn ask(app: &super::App, serve: &Serve, account: &str, key: &str) -> Result<Request> {
    if !rights(serve).request {
        bail!("Requesting albums is not on for this account");
    }
    let s = super::discover::suggestion(serve, account, key)?.context("That suggestion is gone; look again")?;
    if let Some(r) = open_request(serve, account, &s.key)? {
        return Ok(r);
    }
    let record = record_of_album_in(serve, s.release_group.clone(), &s.artist, &s.album, s.year, s.cover.clone(), s.bandcamp.clone()).await?;
    let (lidarr, soulseek) = fetchers(app).await;
    asked_record(&Hands { lidarr: lidarr.as_deref(), soulseek, house: app, disks: &super::room::TheDisks }, serve, account, record, Some(s.key), s.release_group).await
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
    let (lidarr, soulseek) = fetchers(app).await;
    asked_record(&Hands { lidarr: lidarr.as_deref(), soulseek, house: app, disks: &super::room::TheDisks }, serve, account, record, Some(key), release_group).await
}

/// A record asked for by its id (C49a): an artist's or a label's missing record, any record not had. The account's open
/// request for it is the answer where there is one.
pub async fn ask_record(app: &super::App, serve: &Serve, account: &str, record: i64) -> Result<Request> {
    let (lidarr, soulseek) = fetchers(app).await;
    ask_record_with(&Hands { lidarr: lidarr.as_deref(), soulseek, house: app, disks: &super::room::TheDisks }, serve, account, record).await
}

/// [`ask_record`], handed with `h`.
pub(crate) async fn ask_record_with(h: &Hands<'_>, serve: &Serve, account: &str, record: i64) -> Result<Request> {
    if !rights(serve).request {
        bail!("Requesting albums is not on for this account");
    }
    asked_record(h, serve, account, record, None, None).await
}

/// The request for `record` (`asked_of`), asked for and gone ahead (`ask_for`).
async fn asked_record(h: &Hands<'_>, serve: &Serve, account: &str, record: i64, key: Option<String>, asked: Option<String>) -> Result<Request> {
    let cat = serve.catalogue().clone();
    let asked = crate::run::spawn_blocking(move || asked_of(&cat, record, key, asked)).await??;
    ask_for(h, serve, account, asked).await
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

/// Whether a Want's cover is one every account may be shown: in `catalogue::records`, below the requests (SPLIT-PLAN P7).
pub(crate) use crate::catalogue::records::cover_kept;

/// Views as a viewer may read them (W6): an administrator the fetchers' own words too; any other account the states alone, never
/// Lidarr's text (its errors name its address), Soulseek's whys in Cratarr's own words (OQ-RQ1 (a), `own_words`).
fn shown_to(mut views: Vec<View>, administering: bool) -> Vec<View> {
    if !administering {
        for v in &mut views {
            v.request.fetched = None;
            if let RequestState::Wanted { soulseek: Some(super::soulseek::Word::Waiting { why: Some(why) } | super::soulseek::Word::Stuck { why: Some(why) }), .. } = &mut v.state {
                *why = own_words(why);
            }
        }
    }
    views
}

/// A why as a user reads it (OQ-RQ1 (a)): Cratarr's own words, up to its first ": ", where an error's own text (an address, a path)
/// begins. Pure.
fn own_words(why: &str) -> String {
    why.split(": ").next().unwrap_or_default().to_string()
}

/// The record an album asked for by its names is (C49a): in `catalogue::records`, below the requests (SPLIT-PLAN P7).
pub use crate::catalogue::records::record_of_album;
use crate::catalogue::records::record_of_album_paged;

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
        crate::run::spawn_blocking(move || -> Result<(Option<String>, bool)> {
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
    crate::run::spawn_blocking(move || -> Result<i64> {
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

/// The request for `s`: the account's open one for that album where there is one, else a new one, approved at once with no one's
/// yes ("Request albums" is the gate, checked by the caller: AL02.1) and gone ahead (`go_ahead`): handed over, or waiting for room.
async fn ask_for(h: &Hands<'_>, serve: &Serve, account: &str, s: Asked) -> Result<Request> {
    if let Some(r) = serve.userdata.open_request(account, Some(s.record), &s.key)? {
        return Ok(r);
    }
    let now = super::moment::now_secs();
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
        status: APPROVED.to_string(),
        reason: None,
        decided_by: Some(account.to_string()),
        decided_at: Some(now),
        fetched: None,
        record: Some(s.record),
    };
    r.id = match serve.userdata.add_request(&r) {
        Ok(id) => id,
        // Asked twice at once (W6): the account's one open request of the record is the answer.
        Err(e) => return serve.userdata.open_request(account, Some(s.record), &r.album_key)?.ok_or(e),
    };
    r.fetched = Some(go_ahead(h, serve, &r).await);
    Ok(r)
}

/// What a Want of an account over its room says, on its line, until its asker is under (AL02.2): its state then reads
/// `waiting_for_room`, and the requests' round hands it by itself once they are under, oldest first (`hand_waiting`).
pub const WAITS_FOR_ROOM: &str = "Waits for room";

/// Whether an approved request waits for its asker's room (AL02.2): its line the room's word.
pub fn waits_for_room(r: &Request) -> bool {
    r.status == APPROVED && r.fetched.as_deref() == Some(WAITS_FOR_ROOM)
}

/// What a user's Want says, on its line, while the server is short of space (AL03): below the disk floor it is handed to no fetcher,
/// or told to stop where Lidarr only looks for it; its state then reads `waiting_for_space`, and the requests' round hands it by
/// itself once there is space, oldest first (`hand_waiting`).
pub const WAITS_FOR_SPACE: &str = "Waits for space";

/// Whether an approved request waits for space (AL03): its line the floor's word.
pub fn waits_for_space(r: &Request) -> bool {
    r.status == APPROVED && r.fetched.as_deref() == Some(WAITS_FOR_SPACE)
}

/// A Want goes ahead (AL02, the owner's "No approval per request": room is a brake, not an approval): its asker over their room,
/// it waits, saying so (`WAITS_FOR_ROOM`), handed to no fetcher and copied from nothing (AL02.5: a Want the house would meet waits
/// too, and a copy's cost is read from its rows once made, never guessed); a user's below the disk floor goes only as far as it
/// takes no new space (`below_the_floor`, AL03.1); else it is handed (`hand_with`). An administrator has no room, and the floor
/// holds nothing of theirs, so theirs always goes. What it says.
async fn go_ahead(h: &Hands<'_>, serve: &Serve, r: &Request) -> String {
    if full(serve, &r.user).await {
        return wait_for_room(serve, r);
    }
    if held_by_floor(h, serve, &r.user) {
        return below_the_floor(h, serve, r).await;
    }
    hand_with(h, serve, r).await
}

/// `r` set to wait for its asker's room (AL02.2): the word on its line, which a restart keeps. The word.
fn wait_for_room(serve: &Serve, r: &Request) -> String {
    if let Err(e) = serve.userdata.set_request_fetched(r.id, WAITS_FOR_ROOM) {
        tracing::warn!("request {}: that it waits for room was not kept: {e:#}", r.id);
    }
    tracing::info!(target: "requests", request = r.id, user = %r.user, "a Want waits for its asker's room");
    WAITS_FOR_ROOM.to_string()
}

/// `r` set to wait for space (AL03): the floor's word on its line, which a restart keeps. The word.
fn wait_for_space(serve: &Serve, r: &Request) -> String {
    if let Err(e) = serve.userdata.set_request_fetched(r.id, WAITS_FOR_SPACE) {
        tracing::warn!("request {}: that it waits for space was not kept: {e:#}", r.id);
    }
    tracing::info!(target: "requests", request = r.id, user = %r.user, "a Want waits for space: the server is short of space");
    WAITS_FOR_SPACE.to_string()
}

/// Whether `account` is a user (AL01, AL03): an account that is not an administrator. One not known is neither held by the floor nor
/// given a room, as room reads it.
fn is_user(serve: &Serve, account: &str) -> bool {
    serve.users.get(account).is_some_and(|u| !u.admin)
}

/// Whether the disk floor holds `account`'s new Wants now (AL03.1): a user's, while the server is short of space (`room::short_on`,
/// the disks as `h` reads them). Never an administrator's.
fn held_by_floor(h: &Hands<'_>, serve: &Serve, account: &str) -> bool {
    is_user(serve, account) && super::room::short_on(serve, h.disks)
}

/// A user's Want below the floor (AL03.1): the house's copy of a record it holds whole is made only where it shares its blocks, which
/// takes no new space; a record Incoming holds whole is had, and no fetcher's (W3); any other waits for space, handed to no fetcher,
/// and goes by itself at the round once there is space (`hand_waiting`). What it says.
async fn below_the_floor(h: &Hands<'_>, serve: &Serve, r: &Request) -> String {
    let of = named(serve, std::slice::from_ref(r)).await.pop().flatten();
    match of.map(|o| (o.holding, o.holder)) {
        Some((Holding::Done | Holding::Elsewhere, Some(holder))) => match h.house.copy_into_own(&r.user, holder, true).await {
            WantCopy::NoneWhole => wait_for_space(serve, r),
            copied => hand_after(h.lidarr, h.soulseek, serve, r, copied).await,
        },
        Some((Holding::Arrived, _)) => hand_to(h.lidarr, h.soulseek, serve, r).await,
        _ => wait_for_space(serve, r),
    }
}

/// Whether `account` is a user over their room now (AL01's `room::rooms`, one read): an administrator, an account not known, or a
/// room that cannot be read (said in the log) is not.
async fn full(serve: &Serve, account: &str) -> bool {
    rooms_now(serve, std::slice::from_ref(&account.to_string())).await.get(account).is_some_and(|r| r.full)
}

/// The rooms of `accounts` now, by name (`room::rooms_in`, one read, who asked first read now and not as kept: a room just freed is
/// seen at once): none for an administrator or an account not known, nor where they cannot be read (said in the log).
async fn rooms_now(serve: &Serve, accounts: &[String]) -> HashMap<String, super::room::Room> {
    let users: Vec<crate::serve::users::User> = accounts.iter().filter_map(|a| serve.users.get(a)).filter(|u| !u.admin).collect();
    if users.is_empty() || !serve.has_catalogue() {
        return HashMap::new();
    }
    let (cat, ud, kinds) = (serve.catalogue().clone(), serve.userdata.clone(), serve.library.kinds());
    let read = crate::run::spawn_blocking(move || super::room::rooms_in(&cat, &kinds, &requesters_in(&cat, &ud, &kinds), &users)).await;
    match read.map_err(anyhow::Error::from).and_then(|r| r) {
        Ok(rooms) => rooms,
        Err(e) => {
            tracing::warn!(target: "requests", reason = %format!("{e:#}"), "the rooms were not read: Wants go ahead");
            HashMap::new()
        }
    }
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

/// A Delete of a copy in Incoming, for its requests (W4, Y4, V2, U5, RF3): those it closes (`closed_by`) closed as rejected
/// (`reject`). Where it leaves an approved request of the record (`open`, read before the copy went: `open_of`) whose record it no
/// longer holds, the fetchers go on for it, Lidarr as Soulseek, whoever's copy it was (V2, the owner's default: "Lidarr keeps looking
/// for them, and so does Soulseek"). Each one's status is read again now the copy has gone (U5): a pending one alone is no reason to
/// fetch, and one cancelled meanwhile is not handed again.
/// - A copy Lidarr brought first has its own grab marked failed in Lidarr's history (U5, T9: `whose`), and only that grab, whose
///   import is the deleted copy: Lidarr blocklists that release and looks for another, so the next asker is not handed the same bad
///   rip. A copy no fetcher's known (dropped by hand), one whose grab is not found (a manual import, a history cleaned), and another
///   fetcher's mark none, and Lidarr goes on all the same (RF3).
/// - Lidarr is not told to stop, as when one of two requests is taken back (`tell_stopped`), and each one closed says so
///   (`LIDARR_KEPT`).
/// - Lidarr away (`Whose::Unknown`): it is not asked again now. Each one left is Soulseek's meanwhile, saying Lidarr is asked again
///   once it answers (`LIDARR_AWAY`), and the requests' round hands it to Lidarr then (`ask_lidarr_again`).
///
/// Each one left is handed to the fetchers again (`hand_to`), its word that Lidarr was told to stop on the arrival gone. Whether
/// Lidarr goes on (one is left): Incoming's handler then puts back what it stopped for the copy (`Incoming::keep_fetching`), where
/// Lidarr is away once it answers (the handler is run again then: `Incoming::catch_up`).
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
    // Whose the copy was (U5, T9): Lidarr's own grab of it marked failed first, by the group Lidarr was asked by. Whoever's it was,
    // Lidarr goes on for those left (V2, RF3); away, it is asked again once it answers (one wait, and one warning).
    let was = match copy {
        Some(copy) => whose(lidarr, copy, left.iter().chain(closing).find_map(|r| r.release_group.as_deref())).await,
        None => Whose::Another,
    };
    let away = was == Whose::Unknown;
    // Never told to stop while another asks for it.
    let closed = reject(None, &serve.userdata, closing, undo).await;
    for r in closing.iter().filter(|r| closed.iter().any(|c| c.id == r.id)) {
        if let Some(said) = r.fetched.as_deref().filter(|f| f.contains("Lidarr") && !f.starts_with("Not handed over")) {
            let _ = serve.userdata.set_request_fetched(r.id, &format!("{said}; {LIDARR_KEPT}"));
        }
    }
    for r in &left {
        if away {
            hand_while_away(soulseek, serve, r);
        } else {
            hand_to(lidarr, soulseek, serve, r).await;
        }
    }
    true
}

/// What a request a Delete left says while Lidarr is away (RF3): not handed to it now, and handed to it once it answers (the requests'
/// round: `ask_lidarr_again`). It begins as a hand-over Lidarr did not take, so every reader takes it as one Lidarr is not at
/// (`stop_arrived`, `tell_stopped`, below the floor, Soulseek's first search).
pub const LIDARR_AWAY: &str = "Not handed over to Lidarr: away, asked again once it answers";

/// A request a Delete left while Lidarr is away (RF3): Soulseek's meanwhile, where it is connected (woken), its line saying Lidarr
/// is asked again once it answers (`LIDARR_AWAY`).
fn hand_while_away(soulseek: bool, serve: &Serve, r: &Request) {
    let said = if soulseek {
        super::soulseek::wake();
        format!("{LIDARR_AWAY}; {SOULSEEK_SEARCHES}")
    } else {
        LIDARR_AWAY.to_string()
    };
    if let Err(e) = serve.userdata.set_request_fetched(r.id, &said) {
        tracing::warn!("request {}: what the fetchers were told was not kept: {e:#}", r.id);
    }
}

/// The approved requests a Delete left while Lidarr was away (RF3: `LIDARR_AWAY`), handed to Lidarr now it answers (`hand_to`, Soulseek
/// with it), oldest first, each once. Where it is still away, the first one asked keeps its line for the next round, and the others are
/// not asked this round (one wait a round). A request taken back meanwhile is not handed. Their ids handed.
pub(crate) async fn ask_lidarr_again(lidarr: Option<&dyn Fetcher>, soulseek: bool, serve: &Serve) -> Result<Vec<i64>> {
    let Some(f) = lidarr else { return Ok(Vec::new()) };
    let mut waiting: Vec<Request> = serve.userdata.requests(None, Some(APPROVED), OPEN)?.into_iter().filter(|r| r.fetched.as_deref().is_some_and(|f| f.starts_with(LIDARR_AWAY))).collect();
    waiting.sort_by_key(|r| (r.at, r.id));
    let mut handed = Vec::new();
    for r in waiting {
        let said = hand_to(Some(f), soulseek, serve, &r).await;
        if said.starts_with("Not handed over to Lidarr") {
            // Still away: its line as it was, asked again at the next round.
            if let Some(was) = r.fetched.as_deref() {
                serve.userdata.set_request_fetched(r.id, was)?;
            }
            break;
        }
        handed.push(r.id);
    }
    Ok(handed)
}

/// A deleted Incoming copy, as its Delete tells its requests (U5, T9): the fetcher its library names (none: a library from before
/// 0.15's fetchers, as staging's `lidarr-music`), and its folder there (`Artist/Album (Year)`), which Lidarr's history is matched by.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeletedCopy {
    pub fetcher: Option<String>,
    pub folder: String,
}

/// Whose a deleted Incoming copy is, for the requests it leaves (T9). Lidarr goes on for those left whichever it is (RF3), only later
/// where it is away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whose {
    /// Another fetcher's, its library naming it (Soulseek's): Lidarr is not asked.
    Another,
    /// No fetcher's known: its library names none and Lidarr's history imported nothing into its folder (dropped there by hand).
    Nobody,
    /// Lidarr's, its library naming Lidarr or its history importing into the folder; `failed`: its own grab marked failed.
    Lidarr { failed: bool },
    /// Not asked: Lidarr not connected, or no release group to ask by.
    NotAsked,
    /// Not known: Lidarr away or answering wrongly. It is asked again for those left once it answers (`ask_lidarr_again`).
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
    let (Some(f), Some(group)) = (lidarr, group) else { return Whose::NotAsked };
    match f.fail_grab(group, &copy.folder).await {
        Ok(Imported::Failed) => Whose::Lidarr { failed: true },
        Ok(Imported::Unmarked) => {
            tracing::info!(target: "requests", group = %group, folder = %copy.folder, "a deleted copy's grab not marked: Lidarr's newest import of the album went elsewhere, or its grab is not in the history; Lidarr goes on for those left");
            Whose::Lidarr { failed: false }
        }
        Ok(Imported::Not) if named.is_some() => {
            tracing::info!(target: "requests", group = %group, folder = %copy.folder, "a deleted copy's grab is not in Lidarr's history: Lidarr goes on for those left");
            Whose::Lidarr { failed: false }
        }
        Ok(Imported::Not) => {
            tracing::info!(target: "requests", group = %group, folder = %copy.folder, "a deleted copy Lidarr never imported (dropped by hand): no grab marked, Lidarr goes on for those left");
            Whose::Nobody
        }
        Err(e) => {
            tracing::warn!(target: "requests", group = %group, reason = %format!("{e:#}"), "a deleted copy's grab not marked failed: Lidarr away; Soulseek looks for those left, and Lidarr is asked again once it answers");
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
async fn hand_with(h: &Hands<'_>, serve: &Serve, r: &Request) -> String {
    // A record the library has already (C52, OQ3): the asker's own copy of it, in their own music, as Copy to makes one (T5: from
    // anyone's whole copy).
    let copied = copied_for(h.house, serve, r).await;
    hand_after(h.lidarr, h.soulseek, serve, r, copied).await
}

/// What a Want says once its asker's copy was looked at (T5): made, "Copied into your music" (whose music it came from is not said:
/// the owner's OQ-I); not made, why, in the log (`requests`) and on its line, never "Already in the library"; not made below the floor
/// as it would not share its blocks, waiting for space (AL03.1); no copy known whole, handed to the fetchers past a had record's
/// shortcut, said in the log; nothing to do, as [`hand_to`].
pub(super) async fn hand_after(lidarr: Option<&dyn Fetcher>, soulseek: bool, serve: &Serve, r: &Request, copied: WantCopy) -> String {
    let record = r.record.map_or_else(|| "none".to_string(), |n| n.to_string());
    let said = match copied {
        WantCopy::Made => COPIED_INTO_OWN.to_string(),
        WantCopy::NoSpace => return wait_for_space(serve, r),
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

/// A request whose record the library holds whole (C52, OQ3: "Add to Your music" any stored record, through Want), in its asker's
/// view or only in another's own music (T5, I05): its asker's copy of it in their own music (`App::copy_for`). Nothing to do where
/// it is not held whole by its list.
async fn copied_for(house: &dyn House, serve: &Serve, r: &Request) -> WantCopy {
    let Some(of) = named(serve, std::slice::from_ref(r)).await.pop().flatten() else { return WantCopy::Nothing };
    match (of.holding, of.holder) {
        (Holding::Done | Holding::Elsewhere, Some(holder)) => house.copy_into_own(&r.user, holder, false).await,
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
                    let kept = crate::run::spawn_blocking(move || -> Result<bool> {
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
    crate::run::spawn(async move {
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
    let (lidarr, soulseek) = fetchers(app).await;
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
    round_with(&Hands { lidarr: lidarr.as_deref(), soulseek, house: app, disks: &super::room::TheDisks }, serve, now).await
}

/// The round's own work (X1, AL02, AL03), handed with `h`, at `now`, the disk floor read once for it (`room::short_on`), in this order:
/// - 0.16's pending requests go ahead as any Want does (the owner's answer to OQ-R4: no request is pending from 0.17 on);
/// - the Wants waiting for room whose asker is under now, and those waiting for space once there is space, go by themselves, oldest
///   first (`hand_waiting`);
/// - below the floor, a user's request Lidarr only looks for is told to stop, and waits for space (`pause_for_space`, AL03.2);
/// - a request held whole only in another's own music has its asker's copy made, room and space permitting (`copy_for_askers`, I05:
///   the second asker's copy once the first adds);
/// - an album now out handed to Lidarr, Soulseek's turn coming with it;
/// - the requests a Delete left while Lidarr was away handed to it now it answers (`ask_lidarr_again`, RF3);
/// - the first arrival stops the other fetcher, and the stops not told yet are told;
/// - then each approved request done for its asker is delivered ((c)), the count in the log.
pub(crate) async fn round_with(h: &Hands<'_>, serve: &Serve, now: i64) -> Result<()> {
    pending_go_ahead(h, serve).await?;
    let short = super::room::short_on(serve, h.disks);
    hand_waiting(h, serve, short).await?;
    if short {
        pause_for_space(h, serve).await?;
    }
    copy_for_askers(h, serve, short).await?;
    // Out now: handed to Lidarr, Soulseek's turn coming with it.
    let approved = serve.userdata.requests(None, Some(APPROVED), OPEN)?;
    let of = named(serve, &approved).await;
    for (r, o) in approved.iter().zip(&of) {
        let out = o.as_ref().and_then(|o| o.released.as_deref()).and_then(release_day).is_none_or(|day| day <= now);
        if out && r.fetched.as_deref().is_some_and(|f| f.starts_with(WAITS_FOR_RELEASE)) {
            hand_with(h, serve, r).await;
        }
    }
    ask_lidarr_again(h.lidarr, h.soulseek, serve).await?;
    stop_arrived(h.lidarr, serve).await;
    retry_stops(h.lidarr, serve).await;
    let delivered = deliver(serve).await;
    if !delivered.is_empty() {
        tracing::info!(target: "requests", count = delivered.len(), "requests delivered: their albums are done in a library for their askers");
    }
    Ok(())
}

/// 0.16's pending requests, gone ahead as any Want of an account with "Request albums" (the owner's answer to OQ-R4, 2026-11-17:
/// "go ahead" at the upgrade), oldest first: approved by no one's yes but their asker's, as a Want is, then handed or waiting for
/// room (`go_ahead`). No request is made pending since 0.17 (AL02.1), so this is 0.16's alone, at the first round. Their ids.
async fn pending_go_ahead(h: &Hands<'_>, serve: &Serve) -> Result<Vec<i64>> {
    let mut pending = serve.userdata.requests(None, Some(PENDING), OPEN)?;
    pending.reverse();
    let mut went = Vec::new();
    for r in pending {
        // Answered meanwhile (W6): as it is.
        if !serve.userdata.set_request_status(r.id, PENDING, APPROVED, None, Some(&r.user))? {
            continue;
        }
        go_ahead(h, serve, &Request { status: APPROVED.into(), ..r.clone() }).await;
        went.push(r.id);
    }
    if !went.is_empty() {
        tracing::info!(target: "requests", count = went.len(), "0.16's pending requests went ahead as Wants");
    }
    Ok(went)
}

/// The Wants waiting for room or for space, handed by themselves once they may go (AL02.3, AL03.2), oldest first: their asker under
/// their room, and the server not short of space (`short`, as the round read it) or their asker no user. One whose asker is under but
/// the server short waits for space now, and one waiting for space whose asker went over their room waits for room, each saying so. A
/// copy one of them makes from the house counts at once (its rows read again), so the next of that asker's waits if it filled the room
/// (AL02.5). No yes is asked, and none lets one through. Their ids.
pub(crate) async fn hand_waiting(h: &Hands<'_>, serve: &Serve, short: bool) -> Result<Vec<i64>> {
    let waits = |r: &Request| waits_for_room(r) || waits_for_space(r);
    let mut waiting: Vec<Request> = serve.userdata.requests(None, Some(APPROVED), OPEN)?.into_iter().filter(waits).collect();
    if waiting.is_empty() {
        return Ok(Vec::new());
    }
    waiting.sort_by_key(|r| (r.at, r.id));
    let askers: Vec<String> = waiting.iter().map(|r| r.user.clone()).collect();
    let mut rooms = rooms_now(serve, &askers).await;
    let mut handed = Vec::new();
    for r in waiting {
        // Read again now (W6): one cancelled meanwhile is not handed.
        let Some(now) = serve.userdata.request(r.id)?.filter(waits) else { continue };
        if rooms.get(&r.user).is_some_and(|room| room.full) {
            if waits_for_space(&now) {
                wait_for_room(serve, &now);
            }
            continue;
        }
        if short && is_user(serve, &r.user) {
            if waits_for_room(&now) {
                wait_for_space(serve, &now);
            }
            continue;
        }
        let said = hand_with(h, serve, &r).await;
        handed.push(r.id);
        if said == COPIED_INTO_OWN {
            rooms = rooms_now(serve, &askers).await;
        }
    }
    Ok(handed)
}

/// Below the floor (AL03.2; OQ-F1 (a)): a user's request Lidarr only looks for, monitored with nothing grabbed, is told to stop and
/// waits for space, handed again once there is (`hand_waiting`); one in Lidarr's queue is left to finish, and an administrator's goes
/// on, as does a user's of an album an administrator asked for too. Lidarr asked once for all of them; one it could not be told of is
/// looked at again at the next round. Their ids.
async fn pause_for_space(h: &Hands<'_>, serve: &Serve) -> Result<Vec<i64>> {
    let Some(f) = h.lidarr else { return Ok(Vec::new()) };
    let approved = serve.userdata.requests(None, Some(APPROVED), OPEN)?;
    let lidarrs = |r: &Request| r.fetched.as_deref().is_some_and(|f| f.contains("Lidarr") && !f.starts_with("Not handed over") && !f.contains(STOPPED_ON_ARRIVAL));
    let handed: Vec<Request> = approved.iter().filter(|r| is_user(serve, &r.user) && r.release_group.is_some() && lidarrs(r)).cloned().collect();
    if handed.is_empty() {
        return Ok(Vec::new());
    }
    let of = named(serve, &handed).await;
    let handed: Vec<Request> = handed.into_iter().zip(of).filter(|(_, o)| !o.as_ref().is_some_and(RecordOf::had)).map(|(r, _)| r).collect();
    let mut groups: Vec<String> = handed.iter().filter_map(|r| r.release_group.clone()).collect();
    groups.sort();
    groups.dedup();
    // An administrator's request of the album keeps Lidarr at it.
    groups.retain(|g| !approved.iter().any(|o| o.release_group.as_deref() == Some(g.as_str()) && !is_user(serve, &o.user)));
    if groups.is_empty() {
        return Ok(Vec::new());
    }
    let progress = match f.statuses(&groups).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(target: "requests", reason = %format!("{e:#}"), "below the floor, Lidarr not asked how far it is: asked again at the next round");
            return Ok(Vec::new());
        }
    };
    let mut paused = Vec::new();
    for g in groups.iter().filter(|g| matches!(progress.get(*g), Some(Progress::Looking))) {
        match f.cancel(g).await {
            Ok(()) => {
                for r in handed.iter().filter(|r| r.release_group.as_deref() == Some(g.as_str())) {
                    wait_for_space(serve, r);
                    paused.push(r.id);
                }
            }
            Err(e) => tracing::warn!(target: "requests", group = %g, reason = %format!("{e:#}"), "below the floor, Lidarr not told to stop a user's request: told at the next round"),
        }
    }
    Ok(paused)
}

/// Whether a request's line says the house's copy for its asker was looked at already (C52, T5, AL02): copied, not copied (why),
/// nothing to copy into, or waiting for room or for space. One that says none of these, held whole only in another's own music, is
/// still to have its asker's copy (`copy_for_askers`).
fn house_said(r: &Request) -> bool {
    r.fetched.as_deref().is_some_and(|f| f == COPIED_INTO_OWN || f == ALREADY_HAD || f == WAITS_FOR_ROOM || f == WAITS_FOR_SPACE || f.starts_with(NOT_COPIED))
}

/// The second asker's copy (AL02.9, I05, the Want path's taken default: "a copy goes into each other asker's, free when shared"):
/// each approved request whose record is held whole only in another's own music (the first asker added it there), not looked at for
/// its asker's copy yet (`house_said`): its asker over room, it waits, saying so; else their copy is made through C52's path
/// (`App::copy_for`, named by folder: T5), and its request reads done once it is there; a user's while the server is short of space
/// (`short`) only where it shares its blocks, else it waits for space (AL03.1). Never handed to a fetcher from here: a record held whole
/// is had. Their ids.
pub(crate) async fn copy_for_askers(h: &Hands<'_>, serve: &Serve, short: bool) -> Result<Vec<i64>> {
    // One with no line yet is being asked for right now: its asking hands it (`ask_for`).
    let approved: Vec<Request> = serve.userdata.requests(None, Some(APPROVED), OPEN)?.into_iter().filter(|r| r.fetched.is_some() && !house_said(r)).collect();
    if approved.is_empty() {
        return Ok(Vec::new());
    }
    let of = named(serve, &approved).await;
    let mut asking: Vec<(Request, i64)> =
        approved.into_iter().zip(of).filter_map(|(r, o)| o.filter(|o| o.holding == Holding::Elsewhere).and_then(|o| o.holder).map(|holder| (r, holder))).collect();
    asking.sort_by_key(|(r, _)| (r.at, r.id));
    let mut done = Vec::new();
    for (r, holder) in asking {
        if full(serve, &r.user).await {
            wait_for_room(serve, &r);
        } else {
            let copied = match h.house.copy_into_own(&r.user, holder, short && is_user(serve, &r.user)).await {
                WantCopy::NoneWhole => WantCopy::Failed("no copy of it is known whole".into()),
                c => c,
            };
            hand_after(h.lidarr, h.soulseek, serve, &r, copied).await;
        }
        done.push(r.id);
    }
    Ok(done)
}

/// What a Keep's journal entry keeps the requests it delivered under (OQ-D1), for its Undo (`undeliver`).
pub const DELIVERED_BY_KEEP: &str = "requests-delivered";

/// The approved requests done for their askers now delivered ((c), AL02.8): their album whole in a main library or the asker's own
/// music (`Holding::Done`, I05). Each as it was (`deliver_in`). Who approved it stays; when is the delivery's.
pub(crate) async fn deliver(serve: &Serve) -> Vec<Closed> {
    if !serve.has_catalogue() {
        return Vec::new();
    }
    let (cat, ud, kinds) = (serve.catalogue().clone(), serve.userdata.clone(), serve.library.kinds());
    crate::run::spawn_blocking(move || deliver_in(&ud, &cat, &kinds, None)).await.unwrap_or_else(|e| {
        tracing::warn!(target: "requests", "the requests done were not delivered: {e}");
        Vec::new()
    })
}

/// [`deliver`] in the user data `ud` and the catalogue `cat`, each catalogue folder's library as `kinds` has it; with `record`, only
/// the requests whose record it is or holds (a Keep's: OQ-D1). Never one that waits for room or for space (RF17, the review of run
/// 13, RM1): a Want the house meets waits for its asker's copy, though Music's copy makes the album done for everyone; it is
/// delivered once its copy is made. Each as it was, for an Undo (`undeliver`). Blocking.
pub(crate) fn deliver_in(ud: &crate::userdata::UserData, cat: &crate::catalogue::Catalogue, kinds: &[(i64, crate::settings::LibraryKind)], record: Option<i64>) -> Vec<Closed> {
    let approved = ud.requests(None, Some(APPROVED), usize::MAX).unwrap_or_default();
    let of = match records_held(cat, &approved, kinds) {
        Ok(of) => of,
        Err(e) => {
            tracing::warn!(target: "requests", "the requests done were not read: {e:#}");
            return Vec::new();
        }
    };
    let mut delivered = Vec::new();
    for (r, o) in approved.iter().zip(of) {
        let Some(o) = o.filter(|o| o.holding == Holding::Done) else { continue };
        if waits_for_room(r) || waits_for_space(r) {
            continue;
        }
        if record.is_some_and(|n| o.record != n && o.holder != Some(n)) {
            continue;
        }
        match ud.set_request_status(r.id, APPROVED, DELIVERED, None, r.decided_by.as_deref()) {
            Ok(true) => delivered.push(Closed { id: r.id, status: r.status.clone(), reason: r.reason.clone(), decided_by: r.decided_by.clone(), decided_at: r.decided_at }),
            Ok(false) => {}
            Err(e) => tracing::warn!("request {}: not delivered: {e:#}", r.id),
        }
    }
    delivered
}

/// The requests a Keep delivered, opened again by its Undo (OQ-D1), each as it was, where it still reads delivered: a request of the
/// record its account made since stands (one open request a record, W6). The ids opened. Blocking.
pub fn undeliver(ud: &crate::userdata::UserData, delivered: &[Closed]) -> Vec<i64> {
    delivered
        .iter()
        .filter(|c| match ud.restore_request_status(c.id, DELIVERED, &c.status, c.reason.as_deref(), c.decided_by.as_deref(), c.decided_at) {
            Ok(opened) => opened,
            Err(e) => {
                tracing::warn!("request {}: not opened again: {e:#}", c.id);
                false
            }
        })
        .map(|c| c.id)
        .collect()
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

/// Requests answered, one or many (the select mode): approved ones gone ahead as any Want (`go_ahead`), declined ones with the
/// reason, if one was given. Only pending ones are answered (0.16's, till the first round: AL02.7); an approved one that was not
/// handed over is handed over again, room permitting. One waiting for room is let through by no yes (AL02.2): the answer says so.
/// Each id's outcome, in words.
pub async fn decide(app: &super::App, serve: &Serve, by: &str, ids: &[i64], approve: bool, reason: Option<&str>) -> Result<Vec<(i64, String)>> {
    let (lidarr, soulseek) = fetchers(app).await;
    decide_with(&Hands { lidarr: lidarr.as_deref(), soulseek, house: app, disks: &super::room::TheDisks }, serve, by, ids, approve, reason).await
}

/// [`decide`], handed with `h`.
pub(crate) async fn decide_with(h: &Hands<'_>, serve: &Serve, by: &str, ids: &[i64], approve: bool, reason: Option<&str>) -> Result<Vec<(i64, String)>> {
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
        if approve && waits_for_room(&r) {
            out.push((id, format!("It waits for room: raise {}'s room", r.user)));
            continue;
        }
        if approve && waits_for_space(&r) {
            out.push((id, "It waits for space: the server is short of space".to_string()));
            continue;
        }
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
            out.push((id, go_ahead(h, serve, &Request { status: APPROVED.into(), ..r }).await));
        } else if serve.userdata.set_request_status(id, PENDING, DECLINED, reason, Some(by))? {
            out.push((id, "Declined".to_string()));
        } else {
            out.push((id, "Answered meanwhile".to_string()));
        }
    }
    Ok(out)
}

/// A request taken back: the account's own, pending or approved (since 0.17 a Want goes ahead with no one's yes, AL02, so it is its
/// asker's to take back, waiting for room or handed); anyone's by those who answer requests. The fetcher told to stop looking for it
/// (what is there already stays).
pub async fn cancel(app: &super::App, serve: &Serve, account: &str, id: i64) -> Result<()> {
    cancel_with(fetcher(app).as_deref(), serve, account, id).await
}

/// [`cancel`], Lidarr as `lidarr`.
pub(crate) async fn cancel_with(lidarr: Option<&dyn Fetcher>, serve: &Serve, account: &str, id: i64) -> Result<()> {
    let r = serve.userdata.request(id)?.context("That request is gone")?;
    let manage = rights(serve).manage_requests;
    match r.status.as_str() {
        PENDING | APPROVED if r.user == account || manage => {}
        PENDING | APPROVED => bail!("That request is another account's"),
        s => bail!("That request is {s} already"),
    }
    // Written first (X2): the fetchers are told after, as far as they can be, and again at the requests' round where they could
    // not be (`retry_stops`). Soulseek, woken, gives its item to another request of the record, or drops it unless it is under way.
    if !serve.userdata.set_request_status(id, &r.status, CANCELLED, None, Some(account))? {
        bail!("That request was answered meanwhile");
    }
    if r.status == APPROVED {
        tell_stopped(lidarr, serve, &r).await;
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
    Ok(with_rooms(serve, views_of(f, vec![r], &[o]).await).await.pop())
}

/// Each view of a Want waiting for room given its asker's room's numbers (AL02.2), read once for them all.
async fn with_rooms(serve: &Serve, mut views: Vec<View>) -> Vec<View> {
    let askers: Vec<String> = views.iter().filter(|v| matches!(v.state, RequestState::WaitingForRoom { .. })).map(|v| v.request.user.clone()).collect();
    if askers.is_empty() {
        return views;
    }
    let rooms = rooms_now(serve, &askers).await;
    for v in &mut views {
        if let (RequestState::WaitingForRoom { used, room }, Some(r)) = (&mut v.state, rooms.get(&v.request.user)) {
            (*used, *room) = (r.used, r.size);
        }
    }
    views
}

/// A request's story (RQ01, the owner's "a request's sheet tells its whole story"): its row as the Requests page gives it (W5's state,
/// the words `shown_to` lets its viewer read), the record's page it opens (merges followed), when it was asked, when and where its
/// asker's copy came, and where each fetcher is with it. Its asker's and the administrators' alone (`story`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Story {
    #[serde(flatten)]
    pub view: View,
    /// The record's page, `al-<n>`, a merge followed; none where it names no record, or one gone.
    pub opens: Option<String>,
    /// When it was asked, epoch seconds.
    pub asked_at: i64,
    /// When its asker's copy came whole where it is (its last file's coming), epoch seconds; for one delivered with no copy left, when
    /// it was delivered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivered_at: Option<i64>,
    /// The library holding its asker's whole copy: Music, an Incoming library, their own music. None where there is none.
    #[serde(rename = "where", skip_serializing_if = "Option::is_none")]
    pub held_in: Option<Where>,
    /// Where Lidarr is with it, or what it waits for before Lidarr has it: none where Lidarr is not connected or is switched off, the
    /// request is closed, or Lidarr was never its fetcher (Soulseek's alone, had, not answered yet).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lidarr: Option<LidarrPart>,
    /// Soulseek's item of its record, whole (RQ01.4): none where the record has none, or the request is closed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soulseek: Option<super::soulseek::Part>,
}

/// The library holding a request's asker's copy (RQ01.6): its catalogue folder, and its name as the viewer reads it ("Your music"
/// for their own).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Where {
    pub library: i64,
    pub name: String,
}

/// Lidarr's part of a story (RQ01.3): where Lidarr is with it, how far (0 to 1) while grabbed, and Lidarr's own words (its queue's
/// warning or error, else what the hand-over said of Lidarr), which name its address and paths: administrators' alone (W6, OQ-RQ1).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LidarrPart {
    pub step: Step,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
    /// The day its album comes out, as its release date reads ("2026-12-04", "2026-12", "2026"), while it waits for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day: Option<String>,
}

/// Where Lidarr is with a request (RQ01.3): looking for it, grabbed and downloading it, importing it (its queue's state says so),
/// imported, stuck (its grab failed or stalls), or not handed over (or no longer known or wanted by it). Not handed to it yet, what it
/// waits for, and told to stop on its album's arrival (PB4, the review of preview 1's fixes: what is true).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Looking,
    Grabbed,
    Importing,
    Imported,
    Stuck,
    NotHanded,
    /// Its asker over their room (AL02.2): handed to no fetcher yet.
    WaitsForRoom,
    /// The server short of space (AL03): handed to no fetcher yet, or told to stop till there is.
    WaitsForSpace,
    /// Its album not out yet (X1): Lidarr asked once it is; the day in the part's `day`.
    WaitsForRelease,
    /// Told to stop: its album arrived (X1: the first arrival stops the other).
    Stopped,
}

/// A request's story for `account` (RQ01): its asker's and the administrators' (OQ-RQ2 (a): "Manage requests" reads one's own alone);
/// none for anyone else, as for a request that does not exist. Lidarr as it is connected; libraries named as the Library names them.
pub async fn story(app: &super::App, serve: &Serve, account: &str, id: i64) -> Result<Option<Story>> {
    let f = fetcher(app);
    let named = |folder: i64| app.incoming.library_name(crate::source::cratarr::KIND.id, folder);
    story_with(serve, f.as_deref(), account, id, &named).await
}

/// [`story`], Lidarr asked as `f`, a catalogue folder's library named by `named`.
pub(crate) async fn story_with(serve: &Serve, f: Option<&dyn Fetcher>, account: &str, id: i64, named: &(dyn Fn(i64) -> String + Sync)) -> Result<Option<Story>> {
    let administering = crate::serve::access::administering();
    let Some(r) = serve.userdata.request(id)? else { return Ok(None) };
    // Anyone but its asker and the administrators is answered as for a request that does not exist (RQ01.1).
    if !administering && r.user != account {
        return Ok(None);
    }
    let (cat, kinds, asked) = (serve.catalogue().clone(), serve.library.kinds(), r.clone());
    let (of, copy) = crate::run::spawn_blocking(move || held_for(&cat, &asked, &kinds)).await??;
    let held = of.as_ref().map_or(Holding::None, |o| o.holding);
    let shown = shown(r, of.as_ref());
    let open = shown.status == PENDING || shown.status == APPROVED;
    // Lidarr asked once, for its part and for the state (W5: what the Requests page asks it, read as the page reads it).
    let (for_state, for_part) = (asks_lidarr(&shown, held), open && handed_to_lidarr(&shown));
    let progress = match (f, shown.release_group.clone()) {
        (Some(f), Some(group)) if for_state || for_part => f.statuses(std::slice::from_ref(&group)).await.ok().and_then(|mut p| {
            let p = p.remove(&group)?;
            Some((group, p))
        }),
        _ => None,
    };
    let lidarr = f.filter(|_| open).and_then(|_| lidarr_part(&shown, progress.as_ref().filter(|_| for_part).map(|(_, p)| p)));
    // Its record's item (RQ01.4, RQ01.5: a second asker of the record reads the first's), for an open request alone (RQ01.7).
    let soulseek = if open { super::soulseek::part_of(shown.id, shown.record, super::moment::now_secs()) } else { None };
    let read: HashMap<String, Progress> = progress.filter(|_| for_state).into_iter().collect();
    let Some(view) = with_rooms(serve, views_read(vec![(shown, held)], Some(&read))).await.pop() else { return Ok(None) };
    let held_in = copy.map(|c| Where { library: c.folder, name: if c.own && view.request.user == account { crate::settings::YOUR_MUSIC.to_string() } else { named(c.folder) } });
    let delivered = (view.request.status == DELIVERED).then_some(view.request.decided_at).flatten();
    let story = Story {
        opens: of.as_ref().map(|o| crate::serve::ids::record(o.record)),
        asked_at: view.request.at,
        delivered_at: copy.and_then(|c| c.came).or(delivered),
        held_in,
        lidarr,
        soulseek,
        view,
    };
    Ok(Some(story_shown_to(story, administering)))
}

/// A story as its viewer may read it (W6, OQ-RQ1 (a)): an administrator all of it; anyone else Cratarr's own words, never Lidarr's,
/// and every why of Soulseek's (held, stuck, set aside) up to where an error's own text begins (`own_words`).
fn story_shown_to(mut s: Story, administering: bool) -> Story {
    if !administering {
        s.view = shown_to(vec![s.view], false).remove(0);
        if let Some(l) = s.lidarr.as_mut() {
            l.said = None;
        }
        if let Some(p) = s.soulseek.as_mut() {
            if let Some(h) = p.held.as_mut() {
                h.why = own_words(&h.why);
            }
            p.stuck = p.stuck.as_deref().map(own_words);
            for f in p.set_aside.iter_mut().flat_map(|a| a.files.iter_mut()) {
                f.why = own_words(&f.why);
            }
        }
    }
    s
}

/// A request's asker's whole copy, as a story tells it (RQ01.6): the catalogue folder holding it, whether that is the asker's own
/// music, and when it came whole there (its last file's coming; none where not known).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AskersCopy {
    folder: i64,
    own: bool,
    came: Option<i64>,
}

/// What a story reads of a request's record (RQ01.6): the record as the views read it (`records_held`), and its asker's whole copy
/// (`copy_folder`), with when it came there (`Catalogue::went_of`: its present files there). Blocking.
fn held_for(cat: &crate::catalogue::Catalogue, r: &Request, kinds: &[(i64, crate::settings::LibraryKind)]) -> Result<(Option<RecordOf>, Option<AskersCopy>)> {
    let of = records_held(cat, std::slice::from_ref(r), kinds)?.pop().flatten();
    let Some((record, holder, holding)) = of.as_ref().and_then(|o| Some((o.record, o.holder?, o.holding))) else { return Ok((of, None)) };
    let Some(q) = cat.requested(&[record])?.remove(&record) else { return Ok((of, None)) };
    let places = super::room::Places::of(kinds, &cat.folders());
    let own: Vec<i64> = places.own.iter().filter(|(_, account)| *account == r.user).map(|(folder, _)| *folder).collect();
    let Some(folder) = copy_folder(&q, holding, |folder| crate::serve::library::kind_in(kinds, folder).0, &own) else { return Ok((of, None)) };
    let went = cat.went_of(&[holder])?.remove(&holder).unwrap_or_default();
    let came = went.iter().filter(|w| w.folder == folder && w.gone.is_none() && w.trashed.is_none()).filter_map(|w| w.came).max();
    Ok((of, Some(AskersCopy { folder, own: own.contains(&folder), came })))
}

/// The catalogue folder holding a request's asker's copy whole (RQ01.6), as its holding reads it (W4, I05): their own music first,
/// else a main library, where it is done for them; an Incoming library where it arrived; none else (partly there, not had, or held
/// only in another's own music), nor for tracks no file holds. Pure.
fn copy_folder(q: &crate::catalogue::records::Requested, holding: Holding, kind: impl Fn(i64) -> crate::settings::LibraryKind, own: &[i64]) -> Option<i64> {
    use crate::settings::LibraryKind;
    let whole = |n: usize| q.list.is_none_or(|l| n >= l);
    let first = |pick: &dyn Fn(i64) -> bool| q.copies.iter().find(|(folder, n)| whole(*n) && pick(*folder)).map(|(folder, _)| *folder);
    match holding {
        Holding::Done => first(&|folder| own.contains(&folder)).or_else(|| first(&|folder| kind(folder) == LibraryKind::Main)),
        Holding::Arrived => first(&|folder| kind(folder) == LibraryKind::Incoming),
        _ => None,
    }
}

/// Whether a request was handed to Lidarr (X1): its line names Lidarr, and not as one Lidarr did not take ("Not handed over").
fn handed_to_lidarr(r: &Request) -> bool {
    r.fetched.as_deref().is_some_and(|f| f.contains("Lidarr") && !f.starts_with("Not handed over"))
}

/// Lidarr's part of a request (RQ01.3; PB4, the review of preview 1's fixes: what is true), from its line and what Lidarr says of
/// its group (`progress`; none: not asked, or no answer). One that waits for its asker's room, for space or for its album's release
/// says that wait: no fetcher was to have it yet. One Lidarr should have had and did not (its line "Not handed over") is not handed
/// over. One handed: where Lidarr is with it; told to stop, its album come, unless Lidarr imported it; not handed over where Lidarr
/// knows or wants it no longer though never told to stop; none where Lidarr said nothing of it. None where Lidarr was never its
/// fetcher: Soulseek's alone, had, not answered yet. Its words: the queue's, else what the request's line says of Lidarr. Pure.
fn lidarr_part(r: &Request, progress: Option<&Progress>) -> Option<LidarrPart> {
    let fetched = r.fetched.as_deref().unwrap_or_default();
    let waits = |step: Step, day: Option<String>| Some(LidarrPart { step, done: None, said: None, day });
    if waits_for_room(r) {
        return waits(Step::WaitsForRoom, None);
    }
    if waits_for_space(r) {
        return waits(Step::WaitsForSpace, None);
    }
    if let Some(day) = fetched.strip_prefix(WAITS_FOR_RELEASE) {
        return waits(Step::WaitsForRelease, Some(day.trim().to_string()).filter(|d| !d.is_empty()));
    }
    let line = Some(fetched.split("; ").filter(|s| s.contains("Lidarr")).collect::<Vec<_>>().join("; ")).filter(|s| !s.is_empty());
    if fetched.starts_with("Not handed over") {
        return Some(LidarrPart { step: Step::NotHanded, done: None, said: line, day: None });
    }
    if !handed_to_lidarr(r) {
        return None;
    }
    let stopped = fetched.contains(STOPPED_ON_ARRIVAL);
    let (step, done, said) = match progress {
        Some(Progress::Fetched { .. }) => (Step::Imported, None, None),
        // Told to stop, its album come (X1: the first arrival stops the other): not at it, as told.
        _ if stopped => (Step::Stopped, None, None),
        None => return None,
        Some(Progress::Looking) => (Step::Looking, None, None),
        Some(Progress::Fetching { done }) => (Step::Grabbed, Some(*done), None),
        Some(Progress::Importing { .. }) => (Step::Importing, None, None),
        Some(Progress::Stuck { said }) => (Step::Stuck, None, said.clone()),
        // It knows the album no longer, or no longer wants it, and was never told to stop: it is not at it.
        Some(Progress::Unknown | Progress::NotWanted) => (Step::NotHanded, None, None),
    };
    Some(LidarrPart { step, done, said: said.or(line), day: None })
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

pub use crate::serve::requests_kept::{KeptRequesters, Requester};

/// `requesters`, read now.
fn requesters_now(serve: &Serve) -> HashMap<i64, Requester> {
    requesters_in(serve.catalogue(), &serve.userdata, &serve.library.kinds())
}

/// Who asked first for each approved request's record (`requesters`), read now from the user data `ud` and the catalogue `cat`,
/// each catalogue folder's library as `kinds` says: what room counts a copy in Incoming for (AL01) as well. Blocking.
pub(crate) fn requesters_in(cat: &crate::catalogue::Catalogue, ud: &crate::userdata::UserData, kinds: &[(i64, crate::settings::LibraryKind)]) -> HashMap<i64, Requester> {
    let approved = ud.requests(None, Some(APPROVED), OPEN).unwrap_or_default();
    let of = records_held(cat, &approved, kinds).unwrap_or_else(|e| {
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
    /// Whether its album is delivered (W4): a whole copy arrived in Incoming, or in a library, anyone's own music too (the house has
    /// it: no fetcher's to fetch). A copy short of its list is not: it is searched and fetched still.
    pub fn had(&self) -> bool {
        matches!(self.holding, Holding::Arrived | Holding::Done | Holding::Elsewhere)
    }
}

/// How a request's record is held (W4, the owner's answers (b) and (d): one "had" for requests and fetchers): no copy there (none,
/// or only in the trash); a copy short of its list ("partly there": searched and fetched still, its request open); a whole copy in
/// Incoming alone ("arrived": delivered, until it is added); a whole copy in a main library or someone's own music ("done"). For a
/// request (`records_held`), "done" is in its asker's view alone (I05, AL02.9): a whole copy in a main library or their own music;
/// one only in other accounts' own music is had by the house but not theirs ("elsewhere"): their copy is made (C52, T5), room
/// permitting, and it reads done once it is there.
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
    Elsewhere,
}

/// Whether a record held whole is in `own`'s view (I05): a whole copy in a main library, or in the catalogue folders `own` names (the
/// asker's own music). Tracks no file holds, as the tests make them, are one copy kept, in view. Pure.
fn in_view(q: &crate::catalogue::records::Requested, kind: impl Fn(i64) -> crate::settings::LibraryKind, own: &[i64]) -> bool {
    let whole = |n: usize| q.list.is_none_or(|l| n >= l);
    q.copies.is_empty() || q.copies.iter().any(|(folder, n)| whole(*n) && (kind(*folder) == crate::settings::LibraryKind::Main || own.contains(folder)))
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
pub(crate) fn library_kinds() -> Vec<(i64, crate::settings::LibraryKind)> {
    crate::serve::instance().map(|s| s.library.kinds()).unwrap_or_default()
}

/// Each request's record as the catalogue holds it now, in the requests' order: none where it names no record yet (before
/// 0.17's step) or one that is gone. All read at once on one reader, never under the writer's lock (R16,
/// `Catalogue::requested`): the Albums page and the marks ask for every open request while builds run. Blocking.
pub fn records_of(cat: &crate::catalogue::Catalogue, rs: &[Request]) -> Result<Vec<Option<RecordOf>>> {
    records_held(cat, rs, &library_kinds())
}

/// `records_of`, each catalogue folder's library as `kinds` has it (`Serve`'s library), for how each record is held (W4), done in
/// its asker's view alone (I05): a record held whole only in another's own music is `Elsewhere` for them. An own-music folder is
/// the account's its name is (as room reads it, `room::Places`).
pub fn records_held(cat: &crate::catalogue::Catalogue, rs: &[Request], kinds: &[(i64, crate::settings::LibraryKind)]) -> Result<Vec<Option<RecordOf>>> {
    let read = cat.requested(&rs.iter().filter_map(|r| r.record).collect::<Vec<_>>())?;
    let kind = |folder: i64| crate::serve::library::kind_in(kinds, folder).0;
    let places = super::room::Places::of(kinds, &cat.folders());
    let own_of = |user: &str| -> Vec<i64> { places.own.iter().filter(|(_, account)| account == user).map(|(folder, _)| *folder).collect() };
    Ok(rs
        .iter()
        .map(|r| {
            let q = read.get(&r.record?)?;
            let holding = match holding_of(q, kind) {
                Holding::Done if !in_view(q, kind, &own_of(&r.user)) => Holding::Elsewhere,
                held => held,
            };
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
    match crate::run::spawn_blocking(move || records_held(&cat, &rs2, &kinds)).await {
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
        // F16.3 (F3): its record's own address, which tries each of the record's pictures; the stored row as it was.
        r.cover = Some(super::cover_cache::record_address(o.record));
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
    with_rooms(serve, views_of(f, rs, &of).await).await
}

/// `views`, each request's record read (`of`, in the requests' order); a Want waiting for room with no numbers yet (`with_rooms`).
async fn views_of(f: Option<&dyn Fetcher>, rs: Vec<Request>, of: &[Option<RecordOf>]) -> Vec<View> {
    let rs: Vec<(Request, Holding)> = rs.into_iter().zip(of).map(|(r, o)| (shown(r, o.as_ref()), o.as_ref().map_or(Holding::None, |o| o.holding))).collect();
    let asked: Vec<String> = rs.iter().filter(|(r, held)| asks_lidarr(r, *held)).filter_map(|(r, _)| r.release_group.clone()).collect();
    let progress = match f {
        Some(f) if !asked.is_empty() => f.statuses(&asked).await.ok(),
        _ => None,
    };
    views_read(rs, progress.as_ref())
}

/// Whether a view asks Lidarr how far it is with a request (W5): approved, not had, and waiting neither for room nor for space.
fn asks_lidarr(r: &Request, held: Holding) -> bool {
    !matches!(held, Holding::Arrived | Holding::Done | Holding::Elsewhere) && r.status == APPROVED && !waits_for_room(r) && !waits_for_space(r)
}

/// `views_of`'s views, each request as shown with how its record is held, Lidarr's word of each group as it was read (`progress`),
/// Soulseek's read now.
fn views_read(rs: Vec<(Request, Holding)>, progress: Option<&HashMap<String, Progress>>) -> Vec<View> {
    let soulseek = super::soulseek::item_words(&rs.iter().map(|(r, _)| (r.id, r.record)).collect::<Vec<_>>());
    rs.into_iter()
        .map(|(r, held)| {
            let lidarr = r.release_group.as_deref().and_then(|g| progress?.get(g));
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
        .filter(|(r, o)| !matches!(request_state(r, o.as_ref().map_or(Holding::None, |o| o.holding), None, None), RequestState::Done | RequestState::Delivered))
        .map(|(r, o)| shown(r, o.as_ref()))
        .collect()
}


#[cfg(any(test, feature = "test-support"))]
pub(crate) mod tests {
    // Shared with the tests of the crates above (test-support): the helpers only its own tests call are idle there.
    #![cfg_attr(not(test), allow(dead_code, unused_imports))]
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
        assert_eq!(views(&serve, None, vec![row.clone()]).await[0].request.cover, Some(super::super::cover_cache::record_address(record)), "F16.3: the record's own address; neither its outside picture nor the row's");
        let kept = "/api/covers/release-group/00000000-0000-4000-8000-000000000116";
        cat.db_for_tests().execute("UPDATE records SET cover = ?2 WHERE id = ?1", rusqlite::params![record, kept]).unwrap();
        assert_eq!(views(&serve, None, vec![row]).await[0].request.cover, Some(super::super::cover_cache::record_address(record)), "F16.3: the record's own address");
    }

    /// F16.3 (F3): a request's row shows its record's cover: one whose row kept no cover and one whose row kept a Cover Art Archive
    /// address both show `/api/albums/cratarr/al-<n>/cover`; one with no record shows its row's as before; `cover_kept` takes exactly
    /// that shape (`al-` and digits); the stored rows as they were. Never O3's refused shapes (`/api/albums/cratarr/1/cover`, `..`).
    #[tokio::test]
    async fn a_requests_cover_is_its_records() {
        let serve = served();
        let cat = serve.catalogue();
        let (bare, with) = (cat.record_for_tests("Two Rooms", 0), cat.record_for_tests("Undertow", 0));
        let caa = "https://coverartarchive.org/release-group/00000000-0000-4000-8000-000000000f21/front";
        let mut stored = Request { cover: Some(caa.into()), ..asked_for(with) };
        stored.id = serve.userdata.add_request(&stored).unwrap();
        let rows = vec![Request { cover: None, ..asked_for(bare) }, stored.clone(), Request { cover: Some(caa.into()), record: None, ..asked_for(0) }];
        let covers: Vec<Option<String>> = views(&serve, None, rows).await.into_iter().map(|v| v.request.cover).collect();
        let address = super::super::cover_cache::record_address;
        assert_eq!(covers, [Some(address(bare)), Some(address(with)), Some(caa.to_string())]);
        assert_eq!(serve.userdata.request(stored.id).unwrap().unwrap().cover.as_deref(), Some(caa), "the stored row as it was");
        assert!(cover_kept(&address(bare)));
        for left in ["/api/albums/cratarr/al-/cover", "/api/albums/cratarr/al-12x/cover", "/api/albums/cratarr/al-1/cover/../x", "/api/albums/jellyfin/al-1/cover", "/api/albums/cratarr/1/cover", "/api/albums/cratarr/al-1/../cover"] {
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

    /// U5 (the review of 8c/8d, finding 5), changed by RF3 (the review of runs 11 and 12, RB3; V2): Ann's Delete of a copy Lidarr
    /// brought, Bob's request left, and Lidarr's history holds no grab of it: no grab is marked, and Lidarr goes on for Bob's (never
    /// told to stop, asked to want it again), Soulseek too. A copy Soulseek brought leaves Lidarr to go on, no grab asked after.
    #[tokio::test]
    async fn a_lidarr_copy_whose_grab_is_not_found_leaves_lidarr_fetching() {
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
        assert!(deleted(Some(&lidarr), true, &serve, &closed_by(arrived, false, Some("ann")), open, Some(&lidarrs_copy()), None).await, "Lidarr goes on");
        assert_eq!(*lidarr.3.lock().unwrap(), [format!("failed {group}"), "want".to_string()], "asked for its grab (none found), then to want it for Bob's");
        assert!(lidarr.1.lock().unwrap().is_empty(), "never told to stop while Bob asks");
        let now = |id: i64| serve.userdata.request(id).unwrap().unwrap();
        assert_eq!((now(ann.id).status, now(bob.id).status), (REJECTED.to_string(), APPROVED.to_string()));
        assert!(now(ann.id).fetched.unwrap().ends_with(LIDARR_KEPT), "hers says Lidarr goes on");
        assert_eq!(now(bob.id).fetched.unwrap(), format!("Lidarr added it and is looking for it; {SOULSEEK_SEARCHES}"), "Lidarr and Soulseek look for Bob's");
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
    /// marked, Lidarr not asked again now, Soulseek looks for Bob's meanwhile, and the warning is logged once; the Delete says Lidarr
    /// goes on, as it does once it answers (RF3: `lidarr_away_at_a_delete_is_asked_again_for_the_askers_left_once_it_answers`).
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
            assert!(deleted(Some(&lidarr), true, &serve, &closed_by(arrived, false, Some("ann")), open, Some(&copy), None).await, "Lidarr goes on, once it answers ({mode:?})");
            assert!(seen.writes().is_empty(), "no grab marked, nothing wanted ({mode:?}): {:?}", seen.writes());
            assert_eq!((now(&serve, ann.id).status, now(&serve, bob.id).status), (REJECTED.to_string(), APPROVED.to_string()));
            assert_eq!(now(&serve, bob.id).fetched.unwrap(), format!("{LIDARR_AWAY}; {SOULSEEK_SEARCHES}"), "Soulseek looks for Bob's meanwhile ({mode:?})");
            let text = log.text();
            let warned: Vec<&str> = text.lines().filter(|l| l.contains("WARN")).collect();
            assert_eq!(warned.len(), 1, "the warning once ({mode:?}): {text}");
            assert!(warned[0].contains("Lidarr is asked again once it answers"), "{text}");
        }
    }

    /// T9.2: files dropped by hand into Lidarr's library at `Salt Meridian/Both Shores (2018)`, Lidarr's history holding imports of the
    /// album elsewhere only (a folder whose name begins alike, another artist's folder of that name): no grab marked, and, changed by
    /// design (RF3, the review of runs 11 and 12, RB3; V2), Lidarr goes on for Bob's: never told to stop, his handed to it again, and
    /// Soulseek looks too; Ann's says Lidarr goes on. Alike where the library names Lidarr (`Lidarr { failed: false }`) and where it
    /// names no fetcher (staging's `lidarr-music`: `Nobody`).
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
            let copy = DeletedCopy { fetcher: fetcher.map(str::to_string), folder: "Salt Meridian/Both Shores (2018)".into() };
            let is = whose(Some(&faked(&fake::lidarr(fake::Mode::Answers, elsewhere.clone()).await.0, None)), &copy, Some(fake::GROUP)).await;
            assert_eq!(is, if fetcher.is_some() { Whose::Lidarr { failed: false } } else { Whose::Nobody }, "{fetcher:?}");
            let (url, seen) = fake::lidarr(fake::Mode::Answers, elsewhere).await;
            assert!(deleted(Some(&faked(&url, None)), true, &serve, &closed_by(arrived, false, Some("ann")), open, Some(&copy), None).await, "Lidarr goes on ({fetcher:?})");
            assert_eq!(seen.writes(), ["monitor [7] true", "search [7]"], "no POST, never told to stop: Bob's handed to Lidarr again ({fetcher:?})");
            assert_eq!(seen.reads(), ["history 3"], "its imports read, no grab ({fetcher:?})");
            assert!(seen.refused().is_empty(), "{:?}", seen.refused());
            let now = |id: i64| serve.userdata.request(id).unwrap().unwrap();
            assert_eq!((now(ann.id).status, now(bob.id).status), (REJECTED.to_string(), APPROVED.to_string()));
            assert!(now(ann.id).fetched.unwrap().ends_with(LIDARR_KEPT), "hers says Lidarr goes on ({fetcher:?})");
            assert_eq!(now(bob.id).fetched.unwrap(), format!("Lidarr is looking for it; {SOULSEEK_SEARCHES}"), "Lidarr and Soulseek look for Bob's ({fetcher:?})");
        }
    }

    /// RF3 (the review of runs 11 and 12, RB3; V2: "Lidarr keeps looking for them, and so does Soulseek"): Ann deletes her arrival,
    /// Bob still asking, and Lidarr's history marks no grab of it (`Lidarr { failed: false }`): a manual import into the copy's folder
    /// (its imports carry no `downloadId`), a download whose grab is not in the history, or, the library naming Lidarr, a history
    /// cleaned of the album's imports. Each time Lidarr keeps fetching for Bob's: never told to stop, his handed to it again, Soulseek
    /// too, and Ann's says Lidarr goes on. Never a grab marked.
    #[tokio::test]
    async fn a_delete_whose_grab_is_not_found_leaves_lidarr_fetching_for_the_askers_left() {
        use crate::lidarr::fake;
        let into = "/data/music/Salt Meridian/Both Shores (2018)";
        let manual: Vec<serde_json::Value> = fake::imports("D1", into, "2026-11-10")
            .into_iter()
            .map(|mut i| {
                i.as_object_mut().unwrap().remove("downloadId");
                i
            })
            .collect();
        let cases = [
            ("a manual import", None, manual, vec!["history 3"]),
            ("a grab not in the history", None, fake::imports("D9", into, "2026-11-10"), vec!["history 3", "history 1"]),
            ("a history cleaned", Some("lidarr"), Vec::new(), vec!["history 3"]),
        ];
        for (case, fetcher, imports, reads) in cases {
            let copy = DeletedCopy { fetcher: fetcher.map(str::to_string), folder: "Salt Meridian/Both Shores (2018)".into() };
            let is = whose(Some(&faked(&fake::lidarr(fake::Mode::Answers, imports.clone()).await.0, None)), &copy, Some(fake::GROUP)).await;
            assert_eq!(is, Whose::Lidarr { failed: false }, "{case}");
            let serve = served();
            let (ann, bob, arrived, open) = both_shores_asked(&serve).await;
            let (url, seen) = fake::lidarr(fake::Mode::Answers, imports).await;
            assert!(deleted(Some(&faked(&url, None)), true, &serve, &closed_by(arrived, false, Some("ann")), open, Some(&copy), None).await, "Lidarr goes on ({case})");
            assert_eq!(seen.reads(), reads, "{case}");
            assert_eq!(seen.writes(), ["monitor [7] true", "search [7]"], "no grab marked, never told to stop: Bob's handed to Lidarr again ({case})");
            assert!(seen.refused().is_empty(), "{case}: {:?}", seen.refused());
            let now = |id: i64| serve.userdata.request(id).unwrap().unwrap();
            assert_eq!((now(ann.id).status, now(bob.id).status), (REJECTED.to_string(), APPROVED.to_string()), "{case}");
            assert!(now(ann.id).fetched.unwrap().ends_with(LIDARR_KEPT), "hers says Lidarr goes on ({case})");
            assert_eq!(now(bob.id).fetched.unwrap(), format!("Lidarr is looking for it; {SOULSEEK_SEARCHES}"), "Lidarr and Soulseek look for Bob's ({case})");
        }
    }

    /// RF3 (RB3's last case: "Nothing puts Lidarr back when Lidarr returns"): Lidarr away at Ann's Delete (503 to everything), Bob
    /// and Carol still asking. No grab is marked and Lidarr is not asked again now: Bob's and Carol's are Soulseek's meanwhile, each
    /// line saying Lidarr is asked again once it answers, and the Delete says Lidarr goes on (its entry keeps Lidarr fetching, which
    /// Incoming's handler puts back once Lidarr answers: `incoming`'s `a_delete_that_left_a_request_has_lidarr_go_on`). Carol takes
    /// hers back. A round while Lidarr is still away hands nothing and keeps Bob's line; once it answers, the round hands Bob's to
    /// Lidarr (it monitors the album again and searches), his line Lidarr's and Soulseek's; the next round asks nothing more. Never
    /// Carol's, taken back, or Ann's, closed, handed; never a grab marked.
    #[tokio::test]
    async fn lidarr_away_at_a_delete_is_asked_again_for_the_askers_left_once_it_answers() {
        use crate::lidarr::fake;
        let serve = served();
        let (ann, bob, _, _) = both_shores_asked(&serve).await;
        let record = bob.record.unwrap();
        let carol = {
            let r = Request { id: 0, user: "carol".into(), album_key: "k-carol".into(), ..bob.clone() };
            Request { id: serve.userdata.add_request(&r).unwrap(), ..r }
        };
        let (arrived, open) = (arrived_for(&serve, record, Some(200)).await, open_of(&serve, record).await);
        assert_eq!((arrived.len(), open.len()), (3, 3), "it arrived for all three");
        let copy = DeletedCopy { fetcher: None, folder: "Salt Meridian/Both Shores (2018)".into() };
        let (url, seen) = fake::lidarr(fake::Mode::Away, fake::imports("D1", "/data/music/Salt Meridian/Both Shores (2018)", "2026-11-10")).await;
        let lidarr = faked(&url, None);
        assert!(deleted(Some(&lidarr), true, &serve, &closed_by(arrived, false, Some("ann")), open, Some(&copy), None).await, "Lidarr goes on, once it answers");
        let line = |id: i64| now_of(&serve, id).fetched.unwrap();
        let meanwhile = format!("{LIDARR_AWAY}; {SOULSEEK_SEARCHES}");
        assert_eq!((line(bob.id), line(carol.id)), (meanwhile.clone(), meanwhile.clone()), "Soulseek's meanwhile, Lidarr asked again once it answers");
        assert!(seen.writes().is_empty(), "no grab marked, nothing wanted while it is away: {:?}", seen.writes());
        cancel_with(None, &serve, "carol", carol.id).await.unwrap();
        let house = Copies::none();
        let h = Hands { lidarr: Some(&lidarr), soulseek: true, house: &house, disks: &Roomy };
        let at = super::super::moment::now_secs();
        round_with(&h, &serve, at).await.unwrap();
        assert!(seen.writes().is_empty(), "still away: nothing handed: {:?}", seen.writes());
        assert_eq!(line(bob.id), meanwhile, "Bob's line kept for the next round");
        seen.answers(fake::Mode::Answers);
        round_with(&h, &serve, at).await.unwrap();
        assert_eq!(seen.writes(), ["monitor [7] true", "search [7]"], "Bob's handed to Lidarr once it answers, Carol's never");
        assert_eq!(line(bob.id), format!("Lidarr is looking for it; {SOULSEEK_SEARCHES}"), "his line Lidarr's and Soulseek's");
        round_with(&h, &serve, at).await.unwrap();
        assert_eq!(seen.writes().len(), 2, "the next round asks nothing more: {:?}", seen.writes());
        assert!(seen.refused().is_empty(), "{:?}", seen.refused());
        let status = |id: i64| now_of(&serve, id).status;
        assert_eq!((status(ann.id), status(bob.id), status(carol.id)), (REJECTED.to_string(), APPROVED.to_string(), CANCELLED.to_string()));
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

    /// A3.1.1: a Want of a record whose one copy went (deleted outside Cratarr, its tracks within their day) or sits in a parked
    /// library: handed to the fetchers, never "Already in the library"; the requests' round delivers neither.
    #[tokio::test]
    async fn a_want_of_a_copy_gone_or_parked_is_fetched_and_never_delivered() {
        let serve = served();
        let cat = serve.catalogue();
        let gone = cat.record_for_tests("Gone Copy", 2);
        cat.copy_for_tests(gone, 0);
        cat.execute_for_tests(&format!("UPDATE files SET gone_at = {} WHERE track IN (SELECT id FROM tracks WHERE record = {gone})", super::super::moment::now_secs()));
        let parked = cat.record_for_tests("Parked Copy", 2);
        cat.copy_for_tests(parked, -1);
        for record in [gone, parked] {
            let mut r = asked_for(record);
            r.id = serve.userdata.add_request(&r).unwrap();
            let lidarr = Wanting::default();
            let said = hand_to(Some(&lidarr), false, &serve, &r).await;
            assert_ne!(said, ALREADY_HAD, "{record}");
            assert_eq!(lidarr.0.lock().unwrap().len(), 1, "{record}: to the fetchers");
        }
        assert!(deliver_in(&serve.userdata, cat, &[(0, crate::settings::LibraryKind::Main)], None).is_empty(), "the round delivers neither");
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

    // AL02 (run 13): Wants and room, no approval per request (SCENARIOS-RUNS-13-14's AL02.1-AL02.10, the owner's "No approval per
    // request", Q1-Q3, (c), OQ-R4 answered "go ahead").

    use crate::app::room::tests::{ANN, BOB, FOLDERS, LIDARR, MUSIC, Roomy, gb, put_file, room_shape, room_shape_in, rows_of, total, used};

    /// Lidarr as the scenarios' `Strict` fake: made with what it expects, it records what it is asked, and a call it does not expect
    /// is a violation, answered "not expected" (each test ends asserting there are none). `want` takes only the release groups
    /// listed, in their order, each once, and never one with no group; `cancel` only the groups listed; `fail_grab` never (T9 decides
    /// grabs, and nothing here should); `statuses` answers from its map (AL03.2: `Looking`, monitored with nothing grabbed, or
    /// `Fetching`, in its queue), knowing of nothing else.
    #[derive(Default)]
    pub(crate) struct Strict {
        wants: Vec<String>,
        cancels: Vec<String>,
        statuses: HashMap<String, Progress>,
        wanted: Mutex<Vec<String>>,
        cancelled: Mutex<Vec<String>>,
        violations: Mutex<Vec<String>>,
    }

    impl Strict {
        pub(crate) fn expecting_nothing() -> Self {
            Self::default()
        }

        pub(crate) fn expecting_wants(groups: &[&str]) -> Self {
            Strict { wants: groups.iter().map(|g| g.to_string()).collect(), ..Self::default() }
        }

        /// Expecting to be told to stop by these groups too, each once.
        pub(crate) fn and_cancels(self, groups: &[String]) -> Self {
            Strict { cancels: groups.to_vec(), ..self }
        }

        /// Saying how far it is with these groups.
        pub(crate) fn with_statuses(self, statuses: &[(&str, Progress)]) -> Self {
            Strict { statuses: statuses.iter().map(|(g, p)| (g.to_string(), p.clone())).collect(), ..self }
        }

        pub(crate) fn wanted(&self) -> Vec<String> {
            self.wanted.lock().unwrap().clone()
        }

        pub(crate) fn cancelled(&self) -> Vec<String> {
            self.cancelled.lock().unwrap().clone()
        }

        pub(crate) fn violations(&self) -> Vec<String> {
            self.violations.lock().unwrap().clone()
        }

        fn refused<'a, T: Send + 'a>(&'a self, what: String) -> crate::source::Fut<'a, T> {
            self.violations.lock().unwrap().push(what);
            Box::pin(async { Err(anyhow::anyhow!("not expected")) })
        }
    }

    impl Fetcher for Strict {
        fn want<'a>(&'a self, release_group: Option<&'a str>, artist: &'a str, album: &'a str) -> crate::source::Fut<'a, crate::lidarr::Handed> {
            let Some(group) = release_group else { return self.refused(format!("want with no group: {artist} - {album}")) };
            let mut wanted = self.wanted.lock().unwrap();
            if self.wants.get(wanted.len()).map(String::as_str) != Some(group) {
                drop(wanted);
                return self.refused(format!("want {group}"));
            }
            wanted.push(group.to_string());
            let group = group.to_string();
            Box::pin(async move { Ok(crate::lidarr::Handed { said: "Lidarr is looking for it".into(), release_group: group }) })
        }

        fn statuses<'a>(&'a self, groups: &'a [String]) -> crate::source::Fut<'a, HashMap<String, Progress>> {
            let known: HashMap<String, Progress> = groups.iter().filter_map(|g| Some((g.clone(), self.statuses.get(g)?.clone()))).collect();
            Box::pin(async move { Ok(known) })
        }

        fn cancel<'a>(&'a self, release_group: &'a str) -> crate::source::Fut<'a, ()> {
            let mut cancelled = self.cancelled.lock().unwrap();
            if !self.cancels.iter().any(|g| g == release_group) || cancelled.iter().any(|g| g == release_group) {
                drop(cancelled);
                return self.refused(format!("cancel {release_group}"));
            }
            cancelled.push(release_group.to_string());
            Box::pin(async { Ok(()) })
        }

        fn fail_grab<'a>(&'a self, release_group: &'a str, folder: &'a str) -> crate::source::Fut<'a, crate::lidarr::Imported> {
            self.refused(format!("fail_grab {release_group} {folder}"))
        }
    }

    /// The house's copy as the tests make it (C52, T5's `App::copy_for`): each ask recorded; for an account given its own music's
    /// catalogue folder, the copy's rows written as Copy to writes them (`copy_rows`), each file of `from` (folder and place) into
    /// theirs at the same place, sharing blocks as the sharing stand-in says (`shares`: btrfs; ext4 where not); asked for a copy that
    /// shares alone (below the floor, AL03.1) where it would not, none made. Asked for one who holds the copy already, or has no own
    /// music: nothing to copy.
    pub(crate) struct Copies {
        cat: Option<Arc<crate::catalogue::Catalogue>>,
        from: Vec<(i64, String)>,
        own: HashMap<String, i64>,
        shares: bool,
        asked: Mutex<Vec<(String, i64)>>,
    }

    impl Copies {
        pub(crate) fn none() -> Self {
            Copies { cat: None, from: Vec::new(), own: HashMap::new(), shares: false, asked: Mutex::default() }
        }

        pub(crate) fn of(cat: &Arc<crate::catalogue::Catalogue>, from: &[(i64, String)], own: &[(&str, i64)], shares: bool) -> Self {
            Copies { cat: Some(cat.clone()), from: from.to_vec(), own: own.iter().map(|(a, f)| (a.to_string(), *f)).collect(), shares, asked: Mutex::default() }
        }

        pub(crate) fn asked(&self) -> Vec<(String, i64)> {
            self.asked.lock().unwrap().clone()
        }
    }

    impl House for Copies {
        fn copy_into_own<'a>(&'a self, account: &'a str, record: i64, sharing: bool) -> std::pin::Pin<Box<dyn std::future::Future<Output = WantCopy> + Send + 'a>> {
            self.asked.lock().unwrap().push((account.to_string(), record));
            let made = match (&self.cat, self.own.get(account)) {
                (Some(cat), Some(&to)) if !self.from.iter().any(|(_, rel)| cat.has_row(to, rel)) && sharing && !self.shares => WantCopy::NoSpace,
                (Some(cat), Some(&to)) if !self.from.iter().any(|(_, rel)| cat.has_row(to, rel)) => {
                    let pairs: Vec<((i64, String), (i64, String), bool)> = self.from.iter().map(|(f, rel)| ((*f, rel.clone()), (to, rel.clone()), self.shares)).collect();
                    match cat.copy_rows(&pairs) {
                        Ok(_) => WantCopy::Made,
                        Err(e) => WantCopy::Failed(format!("{e:#}")),
                    }
                }
                _ => WantCopy::Nothing,
            };
            Box::pin(async move { made })
        }
    }

    /// AL02's accounts on `serve`, production's shape (every user with Request albums, none with a yes of their own): root, an
    /// administrator; ann, bob and eve, users; dee, a user without Request albums.
    pub(crate) fn with_accounts(serve: &Serve) {
        for (name, admin) in [("root", true), ("ann", false), ("bob", false), ("eve", false), ("dee", false)] {
            serve.users.create(name, "pw", admin).unwrap();
        }
        assert!(serve.users.set_permissions("dee", Permissions { request: false, ..Permissions::default() }).unwrap());
    }

    /// `f` as `user` asks it, signed in (`admin`: as an administrator).
    pub(crate) async fn acting<F: std::future::Future>(user: &str, admin: bool, f: F) -> F::Output {
        let who = crate::serve::who::Who { user: user.into(), admin, scope: crate::serve::who::Scope::Full, home: true };
        crate::serve::who::acting_as(who, f).await
    }

    /// A record not had, of the release group `group`: a Want's.
    pub(crate) fn not_had(serve: &Serve, title: &str, group: &str) -> i64 {
        record_of_album(serve.catalogue(), Some(group), "Glass Harbour", title, None, None).unwrap()
    }

    /// `user`'s approved request of `record`, asked at `at`, by `group`, its line `fetched`: as kept.
    fn kept_request(serve: &Serve, user: &str, record: i64, group: &str, at: i64, fetched: &str) -> Request {
        let r = Request {
            id: 0,
            user: user.into(),
            album_key: format!("{group}-{user}"),
            artist_key: "glass harbour".into(),
            release_group: Some(group.into()),
            album: format!("Record {record}"),
            artist: "Glass Harbour".into(),
            year: None,
            cover: None,
            at,
            status: APPROVED.into(),
            reason: None,
            decided_by: Some(user.into()),
            decided_at: Some(at),
            fetched: Some(fetched.into()),
            record: Some(record),
        };
        Request { id: serve.userdata.add_request(&r).unwrap(), ..r }
    }

    /// A record of `tracks` tracks held by a copy in catalogue folder `folder`, each file `size` bytes, come at `came`, of the
    /// release group `group`: its files' places.
    fn held_in(serve: &Serve, title: &str, tracks: usize, group: &str, folder: i64, size: u64, came: i64) -> (i64, Vec<(i64, String)>) {
        let cat = serve.catalogue();
        let record = cat.record_for_tests(title, tracks);
        assert!(cat.set_reference(&crate::serve::ids::record(record), &crate::catalogue::records::Ref::new("musicbrainz", "release-group", group), "files").unwrap());
        cat.copy_for_tests(record, folder);
        cat.execute_for_tests(&format!("UPDATE files SET size = {size}, came = {came} WHERE folder = {folder} AND rel LIKE 'copy-{record}/%'"));
        (record, places_of(serve, folder, record))
    }

    /// The places of `record`'s copy in catalogue folder `folder` (`copy_for_tests`' files).
    fn places_of(serve: &Serve, folder: i64, record: i64) -> Vec<(i64, String)> {
        let db = serve.catalogue().writer_for_tests();
        let mut st = db.prepare("SELECT rel FROM files WHERE folder = ?1 AND rel LIKE 'copy-' || ?2 || '/%' ORDER BY rel").unwrap();
        let rels: Vec<String> = st.query_map(rusqlite::params![folder, record], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        rels.into_iter().map(|rel| (folder, rel)).collect()
    }

    /// A request as it is kept now.
    fn now_of(serve: &Serve, id: i64) -> Request {
        serve.userdata.request(id).unwrap().unwrap()
    }

    /// A request's state as the Requests page reads it.
    async fn state_of(serve: &Serve, id: i64) -> RequestState {
        views(serve, None, vec![now_of(serve, id)]).await.pop().unwrap().state
    }

    // RQ01 (preview 1's fix batch): a request's story, its route's half (SCENARIOS-P1-FIX's RQ01.3, RQ01.6). The scenarios' alice is
    // room_shape's ann here, whose own music it holds.

    /// A catalogue folder's library named by the folder's last part, as Cratarr's own server names one given no name of its own.
    fn last_part(serve: &Serve) -> impl Fn(i64) -> String + Sync + '_ {
        move |folder| serve.catalogue().folders().get(folder).and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// Request `id`'s story as `who` reads it, signed in (`admin`: as an administrator), Lidarr as `f`.
    async fn told(serve: &Serve, f: Option<&dyn Fetcher>, who: &str, admin: bool, id: i64) -> Story {
        let named = last_part(serve);
        acting(who, admin, story_with(serve, f, who, id, &named)).await.unwrap().unwrap_or_else(|| panic!("{who} reads no story of request {id}"))
    }

    /// RQ01.3 (OQ-RQ1 (a), W6): ann's R1-R6 handed to Lidarr, which says of their groups: G1 monitored, in no queue row; G2 grabbed,
    /// 42% downloaded; G3 waiting to import; G4 importing; G5 stuck, its warning naming a path and Lidarr's address; G6 imported, 11 of
    /// 11. R7 not handed over (Lidarr out of reach then, its error naming its address). Each story says where Lidarr is with it, for ann
    /// as for root; the request's one state (W5) is the row the Requests page gives, as before (fetching 0.42 for G2, stuck for G5).
    /// Ann reads no word of Lidarr's own, nor its address or a path; root reads them. And RQ01.1's rule: bob, with Manage requests
    /// too (OQ-RQ2 (a)), reads no story of hers, as none of a request that does not exist.
    #[tokio::test]
    async fn a_story_says_where_lidarr_is_and_its_words_are_the_administrators() {
        const G: [&str; 7] = [
            "00000000-0000-4000-8000-000000002811",
            "00000000-0000-4000-8000-000000002812",
            "00000000-0000-4000-8000-000000002813",
            "00000000-0000-4000-8000-000000002814",
            "00000000-0000-4000-8000-000000002815",
            "00000000-0000-4000-8000-000000002816",
            "00000000-0000-4000-8000-000000002817",
        ];
        const WARNING: &str = "warning: Import failed: /media/music not found on http://192.168.1.20:8686";
        const AWAY: &str = "Not handed over to Lidarr: error sending request for url (http://192.168.1.20:8686/api/v1/album/lookup)";
        const HANDED: &str = "Lidarr is looking for it; Soulseek searches for it";
        let serve = room_shape();
        with_accounts(&serve);
        let titles = ["Night Ferry", "Quiet Lanes", "Signal", "Late Shift", "Two Rooms", "Far Room", "Low Tide"];
        let ids: Vec<i64> = (0..7).map(|n| kept_request(&serve, "ann", not_had(&serve, titles[n], G[n]), G[n], 100 + n as i64, if n == 6 { AWAY } else { HANDED }).id).collect();
        let strict = Strict::expecting_nothing().with_statuses(&[
            (G[0], Progress::Looking),
            (G[1], Progress::Fetching { done: 0.42 }),
            (G[2], Progress::Importing { done: 1.0 }),
            (G[3], Progress::Importing { done: 1.0 }),
            (G[4], Progress::Stuck { said: Some(WARNING.into()) }),
            (G[5], Progress::Fetched { tracks: 11, of: 11 }),
        ]);
        let f: Option<&dyn Fetcher> = Some(&strict);
        let (mut anns, mut roots) = (Vec::new(), Vec::new());
        for &id in &ids {
            anns.push(told(&serve, f, "ann", false, id).await);
            roots.push(told(&serve, f, "root", true, id).await);
        }
        let steps = |stories: &[Story]| stories.iter().map(|s| s.lidarr.as_ref().map(|l| l.step)).collect::<Vec<_>>();
        let want = [Step::Looking, Step::Grabbed, Step::Importing, Step::Importing, Step::Stuck, Step::Imported, Step::NotHanded].map(Some).to_vec();
        assert_eq!(steps(&anns), want, "ann's");
        assert_eq!(steps(&roots), want, "root's");
        assert!(anns[1].lidarr.as_ref().and_then(|l| l.done).is_some_and(|d| (d - 0.42).abs() < 1e-6), "grabbed, 42%: {:?}", anns[1].lidarr);
        // W6: Lidarr's own words are the administrators'.
        let said = |s: &Story| s.lidarr.as_ref().and_then(|l| l.said.clone());
        assert_eq!((said(&anns[4]), said(&anns[6])), (None, None), "ann reads none of Lidarr's own words");
        assert_eq!(said(&roots[4]).as_deref(), Some(WARNING), "root reads its queue's words");
        assert_eq!(said(&roots[6]).as_deref(), Some(AWAY), "and what the hand-over said of it");
        for s in &anns {
            let json = serde_json::to_string(s).unwrap();
            assert!(!json.contains("192.168.1.20") && !json.contains("/media/music") && !json.contains("Import failed"), "nothing of Lidarr's own in hers: {json}");
        }
        // W5: the request's one state, the row the Requests page gives, as before.
        for (n, s) in anns.iter().enumerate() {
            let row = shown_to(views(&serve, f, vec![now_of(&serve, ids[n])]).await, false).pop().unwrap();
            assert_eq!(s.view, row, "R{}: its row as the Requests page gives it", n + 1);
        }
        assert!(matches!(anns[1].view.state, RequestState::Wanted { lidarr: Some(Fetching::Fetching { done }), .. } if (done - 0.42).abs() < 1e-6), "{:?}", anns[1].view.state);
        assert!(matches!(anns[2].view.state, RequestState::Wanted { lidarr: Some(Fetching::Fetching { .. }), .. }), "importing reads fetching, as before: {:?}", anns[2].view.state);
        assert!(matches!(anns[4].view.state, RequestState::Wanted { lidarr: Some(Fetching::Stuck), .. }), "{:?}", anns[4].view.state);
        // RQ01.1's rule: bob, Manage requests or not, reads none of hers, as none of a request that does not exist.
        let named = last_part(&serve);
        for manages in [false, true] {
            assert!(serve.users.set_permissions("bob", Permissions { manage_requests: manages, ..Permissions::default() }).unwrap());
            for id in [ids[0], 999_999] {
                assert!(acting("bob", false, story_with(&serve, f, "bob", id, &named)).await.unwrap().is_none(), "bob (manages: {manages}) reads no story of {id}");
            }
        }
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
    }

    /// RQ01.6: why ann's requests wait, and when and where each came. W waits for room, 130 of her 100 GB: the numbers /api/me's room
    /// gives (`room::rooms`). S waits for space. D, asked at t1, came whole into Music at t2, and was delivered. I arrived in Soulseek's
    /// Incoming. O is done in her own music. Each story says so, and where her copy is, by its library's id and name ("Your music" to
    /// her, its own name to root). None for W, S, nor E, held only in bob's own music. Never room numbers but /api/me's, nor a "where"
    /// with no copy of hers.
    #[tokio::test]
    async fn a_story_says_why_it_waits_and_when_and_where_it_came() {
        use crate::app::room::tests::SOULSEEK;
        const G: [&str; 6] = [
            "00000000-0000-4000-8000-000000002821",
            "00000000-0000-4000-8000-000000002822",
            "00000000-0000-4000-8000-000000002823",
            "00000000-0000-4000-8000-000000002824",
            "00000000-0000-4000-8000-000000002825",
            "00000000-0000-4000-8000-000000002826",
        ];
        const T1: i64 = 1_790_000_000;
        const T2: i64 = T1 + 86_400;
        let serve = room_shape();
        with_accounts(&serve);
        let cat = serve.catalogue().clone();
        put_file(&cat, ANN, "Mine/1.flac", None, gb(130.0), 10, false, 100);
        let w = kept_request(&serve, "ann", not_had(&serve, "Night Ferry", G[0]), G[0], 100, WAITS_FOR_ROOM);
        let s = kept_request(&serve, "ann", not_had(&serve, "Signal", G[1]), G[1], 110, WAITS_FOR_SPACE);
        let (d_record, _) = held_in(&serve, "Quiet Lanes", 2, G[2], MUSIC, 0, T2);
        let d = kept_request(&serve, "ann", d_record, G[2], T1, SOULSEEK_SEARCHES);
        let (i_record, _) = held_in(&serve, "Two Rooms", 2, G[3], SOULSEEK, 0, T1 + 600);
        let i = kept_request(&serve, "ann", i_record, G[3], T1 + 100, SOULSEEK_SEARCHES);
        let (o_record, _) = held_in(&serve, "Far Room", 2, G[4], ANN, 0, T1 + 700);
        let o = kept_request(&serve, "ann", o_record, G[4], T1 + 200, SOULSEEK_SEARCHES);
        let (e_record, _) = held_in(&serve, "Other Room", 2, G[5], BOB, 0, T1 + 800);
        let e = kept_request(&serve, "ann", e_record, G[5], T1 + 300, SOULSEEK_SEARCHES);
        let delivered = deliver_in(&serve.userdata, &cat, &serve.library.kinds(), Some(d_record));
        assert_eq!(delivered.iter().map(|c| c.id).collect::<Vec<_>>(), [d.id], "D delivered");
        let room = crate::app::room::rooms(&serve, &[serve.users.get("ann").unwrap()]).unwrap()["ann"];
        assert_eq!((room.used, room.size), (gb(130.0), gb(100.0)), "her room as /api/me gives it");
        // W and S: why each waits, with /api/me's numbers; no copy of hers, no where.
        let ws = told(&serve, None, "ann", false, w.id).await;
        assert_eq!(ws.view.state, RequestState::WaitingForRoom { used: room.used, room: room.size }, "130 of 100 GB");
        assert_eq!((&ws.held_in, ws.delivered_at), (&None, None));
        let ss = told(&serve, None, "ann", false, s.id).await;
        assert_eq!(ss.view.state, RequestState::WaitingForSpace);
        assert_eq!((&ss.held_in, ss.delivered_at), (&None, None));
        // D: asked at t1, came whole into Music at t2, delivered.
        let ds = told(&serve, None, "ann", false, d.id).await;
        assert_eq!(ds.view.state, RequestState::Delivered);
        assert_eq!((ds.asked_at, ds.delivered_at), (T1, Some(T2)), "asked at t1, came at t2: when its copy came, not when the round saw it");
        assert_eq!(ds.held_in, Some(Where { library: MUSIC, name: "music".into() }), "Music, by its id and its name");
        assert_eq!(ds.opens, Some(crate::serve::ids::record(d_record)), "it opens its record");
        // I: in Soulseek's Incoming.
        let is = told(&serve, None, "ann", false, i.id).await;
        assert_eq!(is.view.state, RequestState::Arrived);
        assert_eq!((is.held_in.clone(), is.delivered_at), (Some(Where { library: SOULSEEK, name: "soulseek".into() }), Some(T1 + 600)));
        // O: in her own music, "Your music" to her, by its own name to root.
        let os = told(&serve, None, "ann", false, o.id).await;
        assert_eq!(os.view.state, RequestState::Done);
        assert_eq!((os.held_in.clone(), os.delivered_at), (Some(Where { library: ANN, name: crate::settings::YOUR_MUSIC.into() }), Some(T1 + 700)));
        let roots = told(&serve, None, "root", true, o.id).await;
        assert_eq!(roots.held_in, Some(Where { library: ANN, name: "ann".into() }), "root reads her library by its own name");
        // E: held only in bob's own music: still wanted for her, and no where.
        let es = told(&serve, None, "ann", false, e.id).await;
        assert!(matches!(es.view.state, RequestState::Wanted { .. }), "{:?}", es.view.state);
        assert_eq!((&es.held_in, es.delivered_at), (&None, None), "no copy of hers, no where");
    }

    /// RG10 (PB4, the review of preview 1's fixes): with Lidarr connected, a story's Lidarr part says what is true. Ann's Wants: W1
    /// waits for her room, W2 for space, W3 for its release (no fetcher was to have them yet): each says that wait, never "not handed
    /// over". S, Soulseek's alone (handed while Lidarr was not connected): no Lidarr part. A, handed
    /// to Lidarr, came into Soulseek's Incoming, Lidarr told to stop: says so, Lidarr's own words beside it for root alone. H, had
    /// (handed to no fetcher), and P, 0.16's waiting one: none. Not handed over only where Lidarr should have had it and didn't (pins):
    /// R7's hand-over that failed (RQ01.3), and D, handed, which Lidarr no longer looks for, never told to stop.
    #[tokio::test]
    async fn a_storys_lidarr_part_says_what_is_true() {
        use crate::app::room::tests::SOULSEEK;
        const G: [&str; 9] = [
            "00000000-0000-4000-8000-000000002961",
            "00000000-0000-4000-8000-000000002962",
            "00000000-0000-4000-8000-000000002963",
            "00000000-0000-4000-8000-000000002964",
            "00000000-0000-4000-8000-000000002965",
            "00000000-0000-4000-8000-000000002966",
            "00000000-0000-4000-8000-000000002967",
            "00000000-0000-4000-8000-000000002968",
            "00000000-0000-4000-8000-000000002969",
        ];
        const AWAY: &str = "Not handed over to Lidarr: error sending request for url (http://192.168.1.20:8686/api/v1/album/lookup)";
        const HANDED: &str = "Lidarr is looking for it; Soulseek searches for it";
        let serve = room_shape();
        with_accounts(&serve);
        let w1 = kept_request(&serve, "ann", not_had(&serve, "Night Ferry", G[0]), G[0], 100, WAITS_FOR_ROOM);
        let w2 = kept_request(&serve, "ann", not_had(&serve, "Signal", G[1]), G[1], 110, WAITS_FOR_SPACE);
        let w3 = kept_request(&serve, "ann", not_had(&serve, "Late Shift", G[2]), G[2], 120, &format!("{WAITS_FOR_RELEASE} 2026-12-04"));
        let s = kept_request(&serve, "ann", not_had(&serve, "Quiet Lanes", G[3]), G[3], 130, SOULSEEK_SEARCHES);
        let (a_record, _) = held_in(&serve, "Two Rooms", 2, G[4], SOULSEEK, 0, 1_790_000_600);
        let a = kept_request(&serve, "ann", a_record, G[4], 140, &format!("{HANDED}; {STOPPED_ON_ARRIVAL}"));
        let (h_record, _) = held_in(&serve, "Far Room", 2, G[5], MUSIC, 0, 1_790_000_700);
        let h = kept_request(&serve, "ann", h_record, G[5], 150, ALREADY_HAD);
        let p = serve
            .userdata
            .add_request(&Request {
                id: 0,
                user: "ann".into(),
                album_key: format!("{}-ann", G[6]),
                artist_key: "glass harbour".into(),
                release_group: Some(G[6].into()),
                album: "Low Tide".into(),
                artist: "Glass Harbour".into(),
                year: None,
                cover: None,
                at: 160,
                status: PENDING.into(),
                reason: None,
                decided_by: None,
                decided_at: None,
                fetched: None,
                record: Some(not_had(&serve, "Low Tide", G[6])),
            })
            .unwrap();
        let r7 = kept_request(&serve, "ann", not_had(&serve, "Other Room", G[7]), G[7], 170, AWAY);
        let d = kept_request(&serve, "ann", not_had(&serve, "Long Way", G[8]), G[8], 180, HANDED);
        let strict = Strict::expecting_nothing().with_statuses(&[(G[4], Progress::NotWanted), (G[8], Progress::NotWanted)]);
        let f: Option<&dyn Fetcher> = Some(&strict);
        let ids = [w1.id, w2.id, w3.id, s.id, a.id, h.id, p, r7.id, d.id];
        let (mut anns, mut roots) = (Vec::new(), Vec::new());
        for &id in &ids {
            anns.push(told(&serve, f, "ann", false, id).await);
            roots.push(told(&serve, f, "root", true, id).await);
        }
        let steps = |stories: &[Story]| stories.iter().map(|s| s.lidarr.as_ref().map(|l| l.step)).collect::<Vec<_>>();
        let want = vec![
            Some(Step::WaitsForRoom),
            Some(Step::WaitsForSpace),
            Some(Step::WaitsForRelease),
            None,
            Some(Step::Stopped),
            None,
            None,
            Some(Step::NotHanded),
            Some(Step::NotHanded),
        ];
        assert_eq!(steps(&anns), want, "ann's");
        assert_eq!(steps(&roots), want, "root's");
        // The waits as sent: their own step, the release's day, no words of Lidarr's.
        let sent = |s: &Story| serde_json::to_value(s).unwrap()["lidarr"].clone();
        assert_eq!(sent(&anns[0]), serde_json::json!({ "step": "waits_for_room" }));
        assert_eq!(sent(&roots[1]), serde_json::json!({ "step": "waits_for_space" }));
        assert_eq!(sent(&anns[2]), serde_json::json!({ "step": "waits_for_release", "day": "2026-12-04" }));
        assert_eq!(sent(&roots[2]), serde_json::json!({ "step": "waits_for_release", "day": "2026-12-04" }));
        // Told to stop: Lidarr's own words beside it, root's alone (W6).
        assert_eq!(sent(&anns[4]), serde_json::json!({ "step": "stopped" }));
        assert_eq!(roots[4].lidarr.as_ref().and_then(|l| l.said.as_deref()), Some("Lidarr is looking for it; Lidarr told to stop: it arrived"));
        assert_eq!(roots[7].lidarr.as_ref().and_then(|l| l.said.as_deref()), Some(AWAY), "R7: what the hand-over said of Lidarr");
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
    }

    /// A Soulseek folder of `user`'s as a search finds one (RQ01.4).
    #[allow(clippy::too_many_arguments)]
    fn found_folder(user: &str, dir: &str, format: &str, lossless: bool, free_slot: bool, queue: i64, speed: i64, extra: usize) -> crate::app::soulseek::Folder {
        crate::app::soulseek::Folder { user: user.into(), dir: dir.into(), format: format.into(), lossless, free_slot, queue, speed, extra, files: Vec::new(), unchecked: None }
    }

    /// `room_shape` on a data folder of its own whose requests are numbered above `floor`: Soulseek's list is one per test binary, so
    /// a story test's items name requests no other test's do. Its folder, to remove after.
    fn room_shape_above(tag: &str, floor: i64) -> (Arc<Serve>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("cratarr-requests-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join("data")).unwrap();
        let serve = room_shape_in(Some(&dir.join("data")), None);
        let db = rusqlite::Connection::open(dir.join("data").join("userdata.sqlite")).unwrap();
        db.execute("INSERT INTO sqlite_sequence (name, seq) VALUES ('request', ?1)", [floor]).unwrap();
        (serve, dir)
    }

    /// Request `id`'s Soulseek part as `who` reads it in its story (`admin`: as an administrator), and the story as sent.
    async fn part_told(serve: &Serve, who: &str, admin: bool, id: i64) -> (crate::app::soulseek::Part, String) {
        let s = told(serve, None, who, admin, id).await;
        let json = serde_json::to_string(&s).unwrap();
        (s.soulseek.unwrap_or_else(|| panic!("no Soulseek part in {json}")), json)
    }

    /// RQ01.4 (OQ-RQ1 (a)): ann's request R of record X; X's item through each of its states. Found, with peer-a's folder (FLAC, a free
    /// slot, 2.1 MB/s) and peer-b's (MP3, 4 queued, an extra file); fetching from peer-a, 37%; ticked and held, slskd out of reach, for
    /// five minutes; held at its check, AcoustID not asked, for ten; waiting, three misses, searched again tomorrow; stuck, no list of
    /// it. Throughout, peer-c's folder set aside with why per file, and peer-d failed. Ann's story holds the item whole, each as given,
    /// her whys Cratarr's own words (up to the first ": "), root's whole. Never the set-aside's path on the server, nor an address in
    /// hers.
    #[tokio::test]
    async fn a_story_tells_soulseeks_whole_part_to_its_asker() {
        use crate::app::soulseek::{Aside, AsideFile, Folder, Held, Part, SetAside, Source, State, change_for_tests, list_for_tests};
        const G: &str = "00000000-0000-4000-8000-000000002831";
        const ID: &str = "ss-rq01-4";
        const SLSKD_AWAY: &str = "slskd could not be reached: error sending request for url (http://192.168.1.20:5030/api/v0/transfers/downloads)";
        const ACOUSTID: &str = "AcoustID could not be asked: error sending request for url (http://192.168.1.20:8080/v2/lookup)";
        const NO_LIST: &str = "MusicBrainz has no list of it: 503 from http://192.168.1.30:5000/ws/2/release";
        let _state = crate::app::switches::TEST_STATE.lock().await;
        let (serve, dir) = room_shape_above("rq01-4", 96_400);
        with_accounts(&serve);
        serve.catalogue().number_above(9_610).unwrap();
        let x = not_had(&serve, "Night Ferry", G);
        let r = kept_request(&serve, "ann", x, G, 100, SOULSEEK_SEARCHES);
        let a = found_folder("peer-a", "@@a\\Music\\Glass Harbour\\Night Ferry", "flac", true, true, 0, 2_100_000, 0);
        let b = found_folder("peer-b", "@@b\\shared\\Night Ferry (2019)", "mp3", false, false, 4, 600_000, 1);
        let aside = SetAside {
            user: "peer-c".into(),
            path: "/data/soulseek-set-aside/ss-rq01-4 peer-c".into(),
            why: [("01 Slack Water.flac".to_string(), "Clipping at 1:23".to_string()), ("02 Neap.flac".to_string(), "Too short".to_string())].into(),
        };
        list_for_tests(ID, x);
        change_for_tests(ID, |i| (i.record.request, i.folders, i.failed, i.set_aside) = (r.id, vec![a.clone(), b.clone()], vec!["peer-d".into()], vec![aside]));
        let source = |f: &Folder, folder: &str| Source {
            user: f.user.clone(),
            folder: folder.into(),
            format: f.format.clone(),
            lossless: f.lossless,
            free_slot: f.free_slot,
            queue: f.queue,
            speed: f.speed,
            extra: f.extra,
            unchecked: None,
        };
        let files = vec![AsideFile { file: "01 Slack Water.flac".into(), why: "Clipping at 1:23".into() }, AsideFile { file: "02 Neap.flac".into(), why: "Too short".into() }];
        let whole = |state: State| Part {
            id: ID.into(),
            state,
            percent: None,
            sources: vec![source(&a, "Night Ferry"), source(&b, "Night Ferry (2019)")],
            chosen: None,
            failed: vec!["peer-d".into()],
            held: None,
            stuck: None,
            misses: 0,
            next_search: None,
            set_aside: vec![Aside { user: "peer-c".into(), files: files.clone() }],
            off: false,
        };
        let clean = |json: &str| assert!(!json.contains("soulseek-set-aside") && !json.contains("192.168.1."), "nothing of the server's paths nor an address in hers: {json}");
        // Found: two sources, waiting for a tick.
        let (found, json) = part_told(&serve, "ann", false, r.id).await;
        assert_eq!(found, whole(State::Found), "{json}");
        clean(&json);
        // Fetching from peer-a, 37%.
        change_for_tests(ID, |i| (i.state, i.chosen, i.percent, i.batch) = (State::Fetching, Some(0), 37.0, Some("b-rq01-4".into())));
        let (fetching, _) = part_told(&serve, "ann", false, r.id).await;
        assert_eq!(fetching, Part { percent: Some(37.0), chosen: Some(0), ..whole(State::Fetching) });
        assert_eq!(fetching.chosen.map(|c| fetching.sources[c].user.as_str()), Some("peer-a"), "peer-a's, the one fetching");
        // Ticked and held: slskd out of reach, for five minutes.
        let now = super::super::moment::now_secs();
        change_for_tests(ID, |i| (i.state, i.percent, i.batch, i.why, i.due) = (State::Approved, 0.0, None, Some(SLSKD_AWAY.into()), now + 300));
        let (held, json) = part_told(&serve, "ann", false, r.id).await;
        assert_eq!(held, Part { chosen: Some(0), held: Some(Held { why: "slskd could not be reached".into(), until: now + 300 }), ..whole(State::Approved) }, "{json}");
        clean(&json);
        let (theirs, _) = part_told(&serve, "root", true, r.id).await;
        assert_eq!(theirs.held, Some(Held { why: SLSKD_AWAY.into(), until: now + 300 }), "root reads it whole");
        // Held at its check: AcoustID not asked, for ten minutes.
        change_for_tests(ID, |i| (i.state, i.why, i.due) = (State::Checking, Some(ACOUSTID.into()), now + 600));
        let (checking, json) = part_told(&serve, "ann", false, r.id).await;
        assert_eq!(checking, Part { chosen: Some(0), held: Some(Held { why: "AcoustID could not be asked".into(), until: now + 600 }), ..whole(State::Checking) });
        clean(&json);
        // Waiting: three misses, searched again tomorrow.
        change_for_tests(ID, |i| (i.state, i.chosen, i.why, i.misses, i.due) = (State::Waiting, None, None, 3, now + 86_400));
        let (waiting, _) = part_told(&serve, "ann", false, r.id).await;
        assert_eq!(waiting, Part { misses: 3, next_search: Some(now + 86_400), ..whole(State::Waiting) });
        // Stuck: no list of it.
        change_for_tests(ID, |i| (i.state, i.why, i.due) = (State::Stuck, Some(NO_LIST.into()), 0));
        let (stuck, json) = part_told(&serve, "ann", false, r.id).await;
        assert_eq!(stuck, Part { misses: 3, stuck: Some("MusicBrainz has no list of it".into()), ..whole(State::Stuck) }, "{json}");
        clean(&json);
        let (theirs, _) = part_told(&serve, "root", true, r.id).await;
        assert_eq!(theirs.stuck.as_deref(), Some(NO_LIST), "root reads it whole");
        drop(serve);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// RQ01.5 (kind 2): ann's R_a and bob's R_b of record X; X's one item names R_a (one item a record), fetching from peer-a at 60%.
    /// Bob's story of R_b holds X's item, its source and 60%; his request reads Soulseek's word, fetching 60%, in his list as in his
    /// story, as hers does. Nothing in his answers names ann or her request. Never his Wanted with no Soulseek word while X's item
    /// fetches.
    #[tokio::test]
    async fn two_askers_of_one_record_each_see_its_soulseek_part() {
        use crate::app::soulseek::{State, Word, change_for_tests, list_for_tests};
        const G: &str = "00000000-0000-4000-8000-000000002841";
        const ID: &str = "ss-rq01-5";
        let (serve, dir) = room_shape_above("rq01-5", 96_500);
        with_accounts(&serve);
        serve.catalogue().number_above(9_630).unwrap();
        let x = not_had(&serve, "Quiet Lanes", G);
        let ra = kept_request(&serve, "ann", x, G, 100, SOULSEEK_SEARCHES);
        let rb = kept_request(&serve, "bob", x, G, 200, SOULSEEK_SEARCHES);
        list_for_tests(ID, x);
        let a = found_folder("peer-a", "@@a\\Music\\Glass Harbour\\Quiet Lanes", "flac", true, true, 0, 2_100_000, 0);
        change_for_tests(ID, |i| (i.record.request, i.state, i.folders, i.chosen, i.percent, i.batch) = (ra.id, State::Fetching, vec![a], Some(0), 60.0, Some("b-rq01-5".into())));
        let fetching = RequestState::Wanted { lidarr: None, soulseek: Some(Word::Fetching { percent: 60.0 }) };
        assert_eq!(state_of(&serve, rb.id).await, fetching, "his row reads Soulseek's word");
        assert_eq!(state_of(&serve, ra.id).await, fetching, "as hers does");
        let his = told(&serve, None, "bob", false, rb.id).await;
        assert_eq!(his.view.state, fetching, "in his story too");
        let part = his.soulseek.clone().expect("X's item in his story");
        assert_eq!((part.id.as_str(), part.state, part.percent, part.chosen), (ID, State::Fetching, Some(60.0), Some(0)));
        assert_eq!(part.sources.iter().map(|s| s.user.as_str()).collect::<Vec<_>>(), ["peer-a"]);
        let hers = told(&serve, None, "ann", false, ra.id).await;
        assert_eq!(hers.soulseek, Some(part), "the one item, for both");
        let listed = shown_to(views(&serve, None, vec![now_of(&serve, rb.id)]).await, false);
        for text in [serde_json::to_string(&his).unwrap(), serde_json::to_string(&listed).unwrap()] {
            assert!(!text.contains("\"ann\"") && !text.contains("-ann") && !text.contains("\"request\""), "nothing of hers in his: {text}");
        }
        drop(serve);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// RQ01.7 (kind 3): what changed since a sheet was read. ann's R of X, handed to Lidarr and Soulseek, X's item fetching from peer-a.
    /// (b) peer-a goes offline: the next story has peer-b chosen and peer-a failed. (c) Lidarr disconnected or switched off: no Lidarr
    /// part, no Lidarr word in the state. (d) slskd switched off: the item as kept, `off`. (e) X merged into Y: the story names Y and
    /// opens it, its item still its own. (a) ann takes R back while her sheet is open: closed, cancelled, with when, and neither part.
    /// (f) One declined, one rejected: the same. Never a stale item for a closed request.
    #[tokio::test]
    async fn a_story_follows_what_changed() {
        use crate::app::soulseek::{Part, State, change_for_tests, failed_for_tests, list_for_tests};
        const G: [&str; 3] = ["00000000-0000-4000-8000-000000002851", "00000000-0000-4000-8000-000000002852", "00000000-0000-4000-8000-000000002853"];
        const ID: &str = "ss-rq01-7";
        const HANDED: &str = "Lidarr is looking for it; Soulseek searches for it";
        let _state = crate::app::switches::TEST_STATE.lock().await;
        let (serve, dir) = room_shape_above("rq01-7", 96_700);
        with_accounts(&serve);
        let cat = serve.catalogue().clone();
        cat.number_above(9_650).unwrap();
        let x = not_had(&serve, "Night Ferry", G[0]);
        let r = kept_request(&serve, "ann", x, G[0], 100, HANDED);
        list_for_tests(ID, x);
        let folders = vec![found_folder("peer-a", "@@a\\Night Ferry", "flac", true, true, 0, 1_000_000, 0), found_folder("peer-b", "@@b\\Night Ferry", "flac", true, true, 0, 900_000, 0)];
        change_for_tests(ID, |i| (i.record.request, i.state, i.folders, i.chosen, i.percent, i.batch) = (r.id, State::Fetching, folders, Some(0), 10.0, Some("b-rq01-7".into())));
        let strict = Strict::expecting_nothing().with_statuses(&[(G[0], Progress::Looking)]);
        let lidarr: Option<&dyn Fetcher> = Some(&strict);
        let before = told(&serve, lidarr, "ann", false, r.id).await;
        assert!(before.lidarr.is_some() && before.soulseek.as_ref().is_some_and(|p| p.chosen == Some(0) && p.sources[0].user == "peer-a"), "{before:?}");
        // (b) The chosen peer goes offline.
        failed_for_tests(ID, "peer-a");
        let next = told(&serve, lidarr, "ann", false, r.id).await.soulseek.expect("its item");
        assert_eq!((next.state, next.chosen), (State::Approved, Some(0)));
        assert_eq!((next.sources.iter().map(|s| s.user.as_str()).collect::<Vec<_>>(), next.failed.clone()), (vec!["peer-b"], vec!["peer-a".to_string()]), "the next source chosen, the peer failed");
        // (c) Lidarr disconnected or switched off.
        let away = told(&serve, None, "ann", false, r.id).await;
        assert_eq!(away.lidarr, None, "no Lidarr part");
        assert!(matches!(away.view.state, RequestState::Wanted { lidarr: None, .. }), "no Lidarr word: {:?}", away.view.state);
        // (d) slskd switched off: the item as kept, nothing moving.
        let was = crate::app::switches::set_for_tests(crate::app::switches::SLSKD, false);
        let off = told(&serve, lidarr, "ann", false, r.id).await.soulseek;
        crate::app::switches::set_for_tests(crate::app::switches::SLSKD, was);
        assert_eq!(off, Some(Part { off: true, ..next.clone() }), "as kept, slskd off");
        // (e) X merged into Y.
        let y = cat.record_for_tests("Night Ferry (Remastered)", 0);
        cat.merge_records(x, y).unwrap();
        let merged = told(&serve, None, "ann", false, r.id).await;
        assert_eq!((merged.view.request.record, merged.opens.clone()), (Some(y), Some(crate::serve::ids::record(y))), "the merge's end, and it opens it");
        assert_eq!(merged.soulseek.map(|p| p.id), Some(ID.to_string()), "its item still its own");
        // (a) Taken back while the sheet is open.
        acting("ann", false, cancel_with(None, &serve, "ann", r.id)).await.unwrap();
        let back = told(&serve, lidarr, "ann", false, r.id).await;
        assert_eq!((back.view.request.status.as_str(), &back.view.state), (CANCELLED, &RequestState::Cancelled));
        assert!(back.view.request.decided_at.is_some(), "with when");
        assert_eq!((&back.lidarr, &back.soulseek), (&None, &None), "neither part: no stale item for a closed request");
        // (f) Declined, rejected: as taken back.
        for (n, (to, why, state)) in [(DECLINED, Some("No"), RequestState::Declined), (REJECTED, Some(REJECTED_WHY), RequestState::Rejected)].into_iter().enumerate() {
            let record = not_had(&serve, &format!("Signal {n}"), G[n + 1]);
            let closed = kept_request(&serve, "ann", record, G[n + 1], 300 + n as i64, HANDED);
            let id = format!("ss-rq01-7{n}");
            list_for_tests(&id, record);
            change_for_tests(&id, |i| i.record.request = closed.id);
            assert!(serve.userdata.set_request_status(closed.id, APPROVED, to, why, Some("root")).unwrap());
            let s = told(&serve, lidarr, "ann", false, closed.id).await;
            assert_eq!(s.view.state, state);
            assert_eq!((&s.lidarr, &s.soulseek), (&None, &None), "{to}: neither part");
        }
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
        drop(serve);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// AL02.1 (the owner's "No approval per request"; production's shape: every user `request: true`, none with a yes of their own):
    /// ann's Want of R goes ahead at once with no administrator's answer: approved, handed (the strict Lidarr wants its group once),
    /// Soulseek woken (its line says Soulseek searches); Health has no "requests waiting" for it. Asked again: the same request,
    /// Lidarr not asked twice. dee, without Request albums, is refused, and no row is made. Never a pending row from an account with
    /// Request albums.
    #[tokio::test]
    async fn a_want_goes_ahead_on_request_albums_alone() {
        const G: &str = "00000000-0000-4000-8000-000000000401";
        let serve = room_shape();
        with_accounts(&serve);
        let r = not_had(&serve, "Night Ferry", G);
        let (strict, house) = (Strict::expecting_wants(&[G]), Copies::none());
        let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy };
        let asked = acting("ann", false, ask_record_with(&h, &serve, "ann", r)).await.unwrap();
        let row = now_of(&serve, asked.id);
        assert_eq!((row.status.as_str(), row.decided_by.as_deref()), (APPROVED, Some("ann")), "approved at once, no administrator's answer");
        assert_eq!(strict.wanted(), [G], "handed: Lidarr asked once");
        assert!(row.fetched.as_deref().is_some_and(|f| f.contains("Lidarr is looking for it") && f.contains(SOULSEEK_SEARCHES)), "and Soulseek woken: {row:?}");
        assert!(waiting(&serve).unwrap().is_empty(), "none waits for an answer");
        let health = crate::app::health::check_requests(Some(serve.as_ref())).unwrap();
        assert!(!health.iter().any(|f| f.key == "requests:waiting"), "Health: no requests waiting");
        let again = acting("ann", false, ask_record_with(&h, &serve, "ann", r)).await.unwrap();
        assert_eq!((again.id, strict.wanted().len()), (asked.id, 1), "asked again: the same request, Lidarr not asked twice");
        let rows = serve.userdata.requests(None, None, LIST).unwrap().len();
        let refused = acting("dee", false, ask_record_with(&h, &serve, "dee", r)).await.unwrap_err();
        assert!(refused.to_string().contains("Requesting albums is not on for this account"), "{refused:#}");
        assert_eq!(serve.userdata.requests(None, None, LIST).unwrap().len(), rows, "no row made for dee");
        assert!(serve.userdata.requests(None, Some(PENDING), LIST).unwrap().is_empty(), "never a pending row");
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
    }

    /// AL02.2: ann at 101 of her 100 GB. Her Want of R is approved, its line the room's word, its state `waiting_for_room` with her
    /// room's numbers; Lidarr is not asked, and Soulseek takes no item of it. Root's yes on it is refused, saying so, and hands
    /// nothing; root's own Want of S is handed at once. Never a pending row for ann; never an administrator's yes handing a Want over
    /// room.
    #[tokio::test]
    async fn a_want_over_room_waits_and_no_yes_lets_it_through() {
        const R: &str = "00000000-0000-4000-8000-000000000402";
        const S: &str = "00000000-0000-4000-8000-000000000403";
        let serve = room_shape();
        with_accounts(&serve);
        put_file(serve.catalogue(), ANN, "Mine/1.flac", None, gb(101.0), 10, false, 100);
        let (r, s) = (not_had(&serve, "Night Ferry", R), not_had(&serve, "Signal", S));
        let (strict, house) = (Strict::expecting_wants(&[S]), Copies::none());
        let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy };
        let asked = acting("ann", false, ask_record_with(&h, &serve, "ann", r)).await.unwrap();
        let row = now_of(&serve, asked.id);
        assert_eq!((row.status.as_str(), row.fetched.as_deref()), (APPROVED, Some(WAITS_FOR_ROOM)), "approved, its line the room's word");
        assert_eq!(state_of(&serve, row.id).await, RequestState::WaitingForRoom { used: gb(101.0), room: gb(100.0) });
        assert!(strict.wanted().is_empty(), "Lidarr not asked for R");
        let (approved, of) = crate::app::soulseek::requests_taken(&serve.userdata, serve.catalogue(), &serve.library.kinds()).unwrap();
        assert!(!approved.iter().any(|a| a.id == row.id), "not among the requests Soulseek reads");
        assert!(!crate::app::soulseek::items_of(&approved, &of).contains(&r), "Soulseek takes no item of R");
        let said = acting("root", true, decide_with(&h, &serve, "root", &[row.id], true, None)).await.unwrap();
        assert_eq!(said, [(row.id, "It waits for room: raise ann's room".to_string())], "no yes lets it through");
        assert_eq!(now_of(&serve, row.id).fetched.as_deref(), Some(WAITS_FOR_ROOM), "nothing handed");
        assert!(strict.wanted().is_empty());
        let roots = acting("root", true, ask_record_with(&h, &serve, "root", s)).await.unwrap();
        assert_eq!(strict.wanted(), [S], "root's handed at once");
        assert!(roots.fetched.as_deref().is_some_and(|f| f.contains("Lidarr is looking for it")), "{roots:?}");
        assert!(serve.userdata.requests(None, Some(PENDING), LIST).unwrap().is_empty(), "never a pending row");
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
    }

    /// AL02.3 (Q2, Q3): ann over room with three Wants waiting, asked at t1 < t2 < t3 (kept in another order). Once she is under, by
    /// (i) root raising her room, (ii) her deleting an album of hers into the trash, or (iii) an arrival of hers deleted (rejected),
    /// the round hands all three, oldest first, each once: their lines the hand-over's, each read wanted; the second round asks
    /// nothing. Never one handed while she is full, twice, or out of order.
    #[tokio::test]
    async fn waiting_wants_go_by_themselves_oldest_first() {
        const G: [&str; 3] = ["00000000-0000-4000-8000-000000000404", "00000000-0000-4000-8000-000000000405", "00000000-0000-4000-8000-000000000406"];
        const A: &str = "00000000-0000-4000-8000-000000000407";
        for case in ["her room raised", "her album deleted", "her arrival rejected"] {
            let serve = room_shape();
            with_accounts(&serve);
            let cat = serve.catalogue().clone();
            put_file(&cat, ANN, "Mine/1.flac", None, gb(98.0), 10, false, 100);
            // Her arrival: a record she asked for, its copy of 5 GB come into Lidarr's Incoming after (98 + 5: over).
            let (arrival, _) = held_in(&serve, "Arrival", 1, A, LIDARR, gb(5.0), 600);
            let arrived = kept_request(&serve, "ann", arrival, A, 500, SOULSEEK_SEARCHES);
            let records: Vec<i64> = (0..3).map(|n| not_had(&serve, &format!("Want {n}"), G[n])).collect();
            let waiting: Vec<Request> = [2, 0, 1].into_iter().map(|n| kept_request(&serve, "ann", records[n], G[n], 1000 * (n as i64 + 1), WAITS_FOR_ROOM)).collect();
            let (strict, house) = (Strict::expecting_wants(&G), Copies::none());
            let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy };
            let now = super::super::moment::now_secs();
            round_with(&h, &serve, now).await.unwrap();
            assert!(strict.wanted().is_empty(), "{case}: none handed while she is full");
            match case {
                "her room raised" => serve.users.set_room("ann", gb(200.0)).unwrap(),
                "her album deleted" => {
                    cat.mark_trashed(ANN, "Mine", "100/Mine").unwrap();
                }
                _ => {
                    cat.mark_trashed(LIDARR, &format!("copy-{arrival}"), "100/Arrival").unwrap();
                    assert_eq!(reject(None, &serve.userdata, std::slice::from_ref(&arrived), None).await.len(), 1);
                }
            }
            round_with(&h, &serve, now).await.unwrap();
            assert_eq!(strict.wanted(), G, "{case}: all three within the round, oldest first");
            for r in &waiting {
                assert_eq!(now_of(&serve, r.id).fetched.as_deref(), Some(format!("Lidarr is looking for it; {SOULSEEK_SEARCHES}").as_str()), "{case}: the hand-over's line");
                assert!(matches!(state_of(&serve, r.id).await, RequestState::Wanted { lidarr: None, .. }), "{case}: wanted");
            }
            round_with(&h, &serve, now).await.unwrap();
            assert_eq!(strict.wanted(), G, "{case}: the second round asks nothing");
            assert!(strict.violations().is_empty(), "{case}: {:?}", strict.violations());
        }
    }

    /// AL02.5 (INTERACTIONS top 11 (c), I21): Music holds R whole (2 GB); ann at 99 of 100 GB, bob at 101. Ann's Want is met from the
    /// house: her copy made (C52), and her room read from its rows after: 101 GB where the disk could not share (ext4), 99 where it
    /// shares (btrfs). Bob's Want waits, and no copy is made for him; once he is under, the round hands it and his copy is made.
    /// Never a copy made while its asker is full, nor its cost assumed before it is made.
    #[tokio::test]
    async fn a_want_met_from_the_house_waits_for_room_like_any_want() {
        const R: &str = "00000000-0000-4000-8000-000000000408";
        for shares in [false, true] {
            let serve = room_shape();
            with_accounts(&serve);
            let cat = serve.catalogue().clone();
            let (r, files) = held_in(&serve, "Two Rooms", 2, R, MUSIC, gb(1.0), 50);
            put_file(&cat, ANN, "Mine/1.flac", None, gb(99.0), 10, false, 100);
            put_file(&cat, BOB, "His/1.flac", None, gb(101.0), 10, false, 100);
            let (strict, house) = (Strict::expecting_nothing(), Copies::of(&cat, &files, &[("ann", ANN), ("bob", BOB)], shares));
            let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy };
            let anns = acting("ann", false, ask_record_with(&h, &serve, "ann", r)).await.unwrap();
            assert_eq!(anns.fetched.as_deref(), Some(COPIED_INTO_OWN), "shares {shares}: her copy made");
            assert_eq!(used(&serve, "ann"), if shares { gb(99.0) } else { gb(101.0) }, "shares {shares}: read from the rows after the copy");
            let his = rows_of(&cat, BOB);
            let bobs = acting("bob", false, ask_record_with(&h, &serve, "bob", r)).await.unwrap();
            assert_eq!(now_of(&serve, bobs.id).fetched.as_deref(), Some(WAITS_FOR_ROOM), "shares {shares}: his waits");
            assert_eq!(house.asked(), [("ann".to_string(), r)], "shares {shares}: no copy asked for while he is full");
            assert_eq!(rows_of(&cat, BOB), his, "shares {shares}: nothing copied into his music");
            serve.users.set_room("bob", gb(200.0)).unwrap();
            round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
            assert_eq!(house.asked(), [("ann".to_string(), r), ("bob".to_string(), r)], "shares {shares}: under, his copy made at the round");
            assert_eq!(rows_of(&cat, BOB).len(), his.len() + files.len(), "shares {shares}: his copy's rows");
            assert!(strict.violations().is_empty(), "shares {shares}: {:?}", strict.violations());
        }
    }

    /// RF17 (the review of run 13, RM1): a Want the house meets that waits stays waiting through the requests' round. Music holds R
    /// whole; bob is at 101 of 100 GB, so his Want of R waits for room, and it reads that it waits, not "In the library" (Music's copy
    /// is in everyone's view). A round runs while he is still over: not delivered, its line and its state still the room's, no copy
    /// made. Once he is under, the round makes his copy, and only then is it delivered. Never delivered before its copy is made.
    #[tokio::test]
    async fn a_want_met_from_the_house_waits_through_a_round_while_its_asker_is_over() {
        const R: &str = "00000000-0000-4000-8000-000000000491";
        let serve = room_shape();
        with_accounts(&serve);
        let cat = serve.catalogue().clone();
        let (r, files) = held_in(&serve, "Two Rooms", 2, R, MUSIC, gb(1.0), 50);
        put_file(&cat, BOB, "His/1.flac", None, gb(101.0), 10, false, 100);
        let (strict, house) = (Strict::expecting_nothing(), Copies::of(&cat, &files, &[("bob", BOB)], false));
        let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy };
        let bobs = acting("bob", false, ask_record_with(&h, &serve, "bob", r)).await.unwrap();
        assert_eq!(now_of(&serve, bobs.id).fetched.as_deref(), Some(WAITS_FOR_ROOM), "his waits");
        assert!(matches!(state_of(&serve, bobs.id).await, RequestState::WaitingForRoom { .. }), "it reads that it waits, not done");
        let his = rows_of(&cat, BOB);
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        let row = now_of(&serve, bobs.id);
        assert_eq!((row.status.as_str(), row.fetched.as_deref()), (APPROVED, Some(WAITS_FOR_ROOM)), "a round on, still waiting: not delivered");
        assert!(matches!(state_of(&serve, bobs.id).await, RequestState::WaitingForRoom { .. }), "still reads that it waits");
        assert!(house.asked().is_empty() && rows_of(&cat, BOB) == his, "no copy made while he is over");
        serve.users.set_room("bob", gb(200.0)).unwrap();
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        assert_eq!(house.asked(), [("bob".to_string(), r)], "under: his copy made at the round");
        assert_eq!(rows_of(&cat, BOB).len(), his.len() + files.len(), "its rows in his music");
        assert_eq!(now_of(&serve, bobs.id).status, DELIVERED, "delivered once his copy is made");
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
    }

    /// RF17 (RM1), below the floor (AL03.1): ann, under her room, Wants a record Music holds whole where her copy would not share its
    /// blocks: it waits for space, and reads so. A round below the floor leaves it waiting, not delivered, no copy made; once there is
    /// space, the round makes her copy, then delivers it.
    #[tokio::test]
    async fn a_want_met_from_the_house_waits_through_a_round_below_the_floor() {
        use crate::app::room::tests::Stand;
        const M: &str = "00000000-0000-4000-8000-000000000492";
        let serve = room_shape();
        with_accounts(&serve);
        let cat = serve.catalogue().clone();
        let stand = Stand::of(&[(FOLDERS[1], 150.0), (FOLDERS[2], 150.0)]);
        let (m, apart) = held_in(&serve, "Far Room", 2, M, MUSIC, gb(1.0), 50);
        let (strict, house) = (Strict::expecting_nothing(), Copies::of(&cat, &apart, &[("ann", ANN)], false));
        let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &stand };
        let anns = acting("ann", false, ask_record_with(&h, &serve, "ann", m)).await.unwrap();
        assert_eq!(now_of(&serve, anns.id).fetched.as_deref(), Some(WAITS_FOR_SPACE), "hers waits for space");
        assert_eq!(state_of(&serve, anns.id).await, RequestState::WaitingForSpace, "it reads that it waits, not done");
        let hers = rows_of(&cat, ANN);
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        let row = now_of(&serve, anns.id);
        assert_eq!((row.status.as_str(), row.fetched.as_deref()), (APPROVED, Some(WAITS_FOR_SPACE)), "a round below the floor: still waiting, not delivered");
        assert_eq!(state_of(&serve, anns.id).await, RequestState::WaitingForSpace, "still reads that it waits");
        assert_eq!(rows_of(&cat, ANN), hers, "no copy made");
        let h = Hands { disks: &Roomy, ..h };
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        assert_eq!(rows_of(&cat, ANN).len(), hers.len() + apart.len(), "space again: her copy made at the round");
        assert_eq!(now_of(&serve, anns.id).status, DELIVERED, "then delivered");
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
    }

    /// A server on a copy of `fixtures::data_0_16` with `more` run on its user data first (0.16's rows), 0.17's step run, and the
    /// accounts root, ann and bob; its folder. The libraries' own rule: `/music` the Music folder.
    fn upgraded(tag: &str, more: &str) -> (Arc<Serve>, std::path::PathBuf) {
        let d = std::env::temp_dir().join(format!("cratarr-requests-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        crate::fixtures::data_0_16(&d).unwrap();
        rusqlite::Connection::open(d.join("userdata.sqlite")).unwrap().execute_batch(more).unwrap();
        let serve = Serve::open(Some(&d), crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(Arc::new(crate::catalogue::Catalogue::open(Some(&d.join("catalogue.sqlite")), vec![std::path::PathBuf::from("/music")]).unwrap()));
        super::super::step_017::run(serve.catalogue(), &serve.userdata, &[], &crate::catalogue::Rules::default()).unwrap();
        for (name, admin) in [("root", true), ("ann", false), ("bob", false)] {
            serve.users.create(name, "pw", admin).unwrap();
        }
        (serve, d)
    }

    /// The groups the first arrival's stop tells Lidarr of (`stop_arrived`) on these rows as they stand: what a strict Lidarr must
    /// expect of a round over 0.16's data.
    async fn stops_due(serve: &Serve) -> Vec<String> {
        let approved = serve.userdata.requests(None, Some(APPROVED), OPEN).unwrap();
        let of = named(serve, &approved).await;
        approved
            .iter()
            .zip(&of)
            .filter(|(r, o)| o.as_ref().is_some_and(RecordOf::had) && r.fetched.as_deref().is_some_and(|f| f.contains("Lidarr") && !f.starts_with("Not handed over") && !f.contains(STOPPED_ON_ARRIVAL)))
            .filter_map(|(r, o)| r.release_group.clone().or_else(|| o.as_ref()?.groups.first().cloned()))
            .collect()
    }

    /// AL02.7 (OQ-R4, the owner's answer, 2026-11-17: 0.16's pending requests "go ahead" at the upgrade, like any Want of an account
    /// with Request albums): 0.16's user data with three pending requests of bob's (production's account with three). 0.17 starts:
    /// at its first round they go ahead as Wants, oldest first, each handed once (a strict Lidarr expecting their groups, and the
    /// first arrival's stops 0.16's rows call for), approved by no yes but bob's own; none is left pending, Health lists none waiting
    /// for an answer, and the second round asks nothing more. Never a request pending since 0.17.
    #[tokio::test]
    async fn pending_requests_from_0_16_go_ahead_at_the_upgrade() {
        const G: [&str; 3] = ["00000000-0000-4000-8000-000000000431", "00000000-0000-4000-8000-000000000432", "00000000-0000-4000-8000-000000000433"];
        let rows: Vec<String> = G
            .iter()
            .enumerate()
            .map(|(n, g)| format!("('bob', '{g}', 'orbit lane', '{g}', 'Pending {n}', 'Orbit Lane', 2020, NULL, {}, 'pending', NULL, NULL, NULL, NULL)", 1_793_951_000 + 100 * n as i64))
            .collect();
        let more = format!("INSERT INTO request (user, album_key, artist_key, release_group, album, artist, year, cover, at, status, reason, decided_by, decided_at, fetched) VALUES {};", rows.join(", "));
        let (serve, d) = upgraded("al02-pending", &more);
        let pending = serve.userdata.requests(Some("bob"), Some(PENDING), LIST).unwrap();
        assert_eq!(pending.len(), 3, "0.16's three pending, through the step");
        assert_eq!(waiting(&serve).unwrap().len(), 3, "listed as waiting till the first round");
        let strict = Strict::expecting_wants(&G).and_cancels(&stops_due(&serve).await);
        let house = Copies::none();
        let h = Hands { lidarr: Some(&strict), soulseek: false, house: &house, disks: &Roomy };
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        assert_eq!(strict.wanted(), G, "they went ahead at the first round, oldest first");
        for r in &pending {
            let now = now_of(&serve, r.id);
            assert_eq!((now.status.as_str(), now.decided_by.as_deref(), now.fetched.as_deref()), (APPROVED, Some("bob"), Some("Lidarr is looking for it")), "as a Want: {now:?}");
        }
        assert!(serve.userdata.requests(None, Some(PENDING), LIST).unwrap().is_empty(), "none left pending");
        assert!(waiting(&serve).unwrap().is_empty(), "Health: none waiting for an answer");
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        assert_eq!(strict.wanted(), G, "the second round asks nothing more");
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
        drop(serve);
        std::fs::remove_dir_all(&d).ok();
    }

    /// AL02.8 ((c) "Closes when done"; INTERACTIONS top 3, top 12; AL01.7; OQ-D1). Ann's request of R (asked in January) arrived and
    /// was added to her own music whole; her R2 was delivered into Music. Within one round both are delivered, read so, and stay in
    /// her history. In May root's Lidarr brings a FLAC edition of R into Incoming: it is no one's request's (the delivered one has no
    /// say): its default target is Music, not ann's own music, and Home Assistant's promote goes there too; ann may neither add nor
    /// delete it; her room counts nothing of it, and no one's does (AL01.7). R2's Music copy deleted and the trash emptied: Soulseek
    /// makes no item of it, and it never reads wanted. Ann's new Want of R is a new request, the delivered one staying. A Keep's
    /// delivery, named on its entry, is opened again by its Undo as it was (OQ-D1). And 0.16's approved requests done in a library
    /// are delivered at the first round, their count in the log.
    #[tokio::test]
    async fn a_delivered_request_has_no_say_over_a_later_copy() {
        use super::super::albums::{default_target, may_add, requester_of};
        use crate::settings::{Libraries, LibraryKind, LibrarySetting, SourceConfig};
        const R: &str = "00000000-0000-4000-8000-000000000441";
        const R2: &str = "00000000-0000-4000-8000-000000000442";
        const R3: &str = "00000000-0000-4000-8000-000000000443";
        let serve = room_shape();
        with_accounts(&serve);
        let cat = serve.catalogue().clone();
        let (r, _) = held_in(&serve, "Two Rooms", 2, R, ANN, gb(1.0), 200);
        let (r2, _) = held_in(&serve, "Quiet Lanes", 2, R2, MUSIC, gb(1.0), 250);
        let rq = kept_request(&serve, "ann", r, R, 100, SOULSEEK_SEARCHES);
        let rq2 = kept_request(&serve, "ann", r2, R2, 150, SOULSEEK_SEARCHES);
        let held = used(&serve, "ann");
        assert_eq!(held, gb(2.0), "her own copy of R");
        let (strict, house) = (Strict::expecting_nothing(), Copies::none());
        let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy };
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        for id in [rq.id, rq2.id] {
            assert_eq!(now_of(&serve, id).status, DELIVERED, "within one round");
            assert_eq!(state_of(&serve, id).await, RequestState::Delivered, "read so");
        }
        let history: Vec<i64> = serve.userdata.requests(Some("ann"), None, LIST).unwrap().iter().map(|r| r.id).collect();
        assert!(history.contains(&rq.id) && history.contains(&rq2.id), "in her history still");
        // May: root's Lidarr brings a FLAC edition of R into Incoming, after she asked.
        cat.copy_for_tests(r, LIDARR);
        cat.execute_for_tests(&format!("UPDATE files SET size = {}, came = 5000 WHERE folder = {LIDARR}", gb(1.5)));
        let asked = requesters(&serve);
        assert!(!asked.contains_key(&r), "a delivered request is no one's say over a copy");
        let flac = crate::source::Album { id: crate::serve::ids::record(r), added: Some(5000), library: Some(FOLDERS[1].into()), ..Default::default() };
        let placed = |id: &str| crate::placement::PlacedLibrary { id: id.into(), name: id.into(), folders: vec![id.into()], paths: Vec::new(), read_only: false };
        let values = [(crate::settings::OWN_MUSIC.to_string(), serde_json::json!("/own")), (crate::settings::OWN_MUSIC_OFF.to_string(), serde_json::json!([]))].into_iter().collect();
        let libraries = Libraries(vec![LibrarySetting::new(FOLDERS[0], LibraryKind::Main), LibrarySetting::new(FOLDERS[1], LibraryKind::Incoming), LibrarySetting::new(FOLDERS[2], LibraryKind::Incoming)]);
        let cfg = SourceConfig { id: "cratarr".into(), kind: crate::source::cratarr::KIND.id.into(), enabled: true, libraries, values };
        let placement = crate::placement::Placement { roots: Vec::new(), libraries: FOLDERS.iter().map(|f| placed(f)).collect() };
        let a = crate::app::library::Active { cfg, source: None, placement, error: None };
        let requester = requester_of(&asked, &flac).map(String::as_str);
        assert_eq!(requester, None, "no requester: neither hers to add nor to delete");
        assert_eq!(default_target(&a, flac.library.as_deref(), requester).as_deref(), Some(FOLDERS[0]), "its default target the library's: Music");
        assert_eq!(default_target(&a, flac.library.as_deref(), Some("ann")).as_deref(), Some("/own/ann"), "as her open request's would have gone");
        assert!(!may_add(&a, false, Some("ann"), requester), "ann may not add it");
        // Home Assistant's promote is a Keep with no library named: where the page's default goes.
        assert_eq!(default_target(&a, flac.library.as_deref(), requester).as_deref(), Some(FOLDERS[0]), "Home Assistant's promote: into Music");
        assert_eq!((used(&serve, "ann"), total(&serve)), (held, held), "AL01.7: her room counts nothing of it, and no one's does");
        // R2's Music copy deleted, the trash emptied: never searched again, never wanted.
        cat.mark_trashed(MUSIC, &format!("copy-{r2}"), "300/Quiet Lanes").unwrap();
        cat.execute_for_tests(&format!("DELETE FROM files WHERE folder = {MUSIC}"));
        let (approved, of) = crate::app::soulseek::requests_taken(&serve.userdata, &cat, &serve.library.kinds()).unwrap();
        assert!(!approved.iter().any(|a| a.id == rq2.id) && !crate::app::soulseek::items_of(&approved, &of).contains(&r2), "Soulseek makes no item for R2");
        assert_eq!(state_of(&serve, rq2.id).await, RequestState::Delivered, "R2 never reads wanted");
        // Ann's new Want of R: a new request; the delivered one stays.
        let again = acting("ann", false, ask_record_with(&h, &serve, "ann", r)).await.unwrap();
        assert_ne!(again.id, rq.id, "a new request");
        assert_eq!((now_of(&serve, rq.id).status.as_str(), now_of(&serve, again.id).status.as_str()), (DELIVERED, APPROVED));
        // OQ-D1: R3 approved, then a Keep into her own music delivers it, named on its entry; the Keep's Undo opens it again as it was.
        let r3 = not_had(&serve, "Signal", R3);
        let rq3 = kept_request(&serve, "ann", r3, R3, 300, SOULSEEK_SEARCHES);
        assert!(deliver_in(&serve.userdata, &cat, &serve.library.kinds(), Some(r3)).is_empty(), "not had: nothing to deliver");
        cat.execute_for_tests(&format!("INSERT INTO tracks (born, record, disc, number) VALUES (1, {r3}, 1, 1)"));
        cat.copy_for_tests(r3, ANN);
        let delivered = deliver_in(&serve.userdata, &cat, &serve.library.kinds(), Some(r3));
        assert_eq!(delivered.iter().map(|c| c.id).collect::<Vec<_>>(), [rq3.id], "the Keep's: R3's alone");
        assert_eq!(now_of(&serve, rq3.id).status, DELIVERED);
        assert_eq!(undeliver(&serve.userdata, &delivered), [rq3.id]);
        let back = now_of(&serve, rq3.id);
        assert_eq!((back.status.as_str(), back.decided_by.as_deref(), back.decided_at), (APPROVED, Some("ann"), Some(300)), "as it was");
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
        // 0.16's approved requests done in a library: delivered at the first round, their count in the log.
        let (old, d) = upgraded("al02-delivered", "");
        let approved = old.userdata.requests(None, Some(APPROVED), LIST).unwrap();
        let done: Vec<i64> = approved.iter().zip(named(&old, &approved).await).filter(|(_, o)| o.as_ref().is_some_and(|o| o.holding == Holding::Done)).map(|(r, _)| r.id).collect();
        assert!(!done.is_empty(), "0.16's data holds requests done");
        let strict = Strict::expecting_nothing().and_cancels(&stops_due(&old).await);
        let h = Hands { lidarr: Some(&strict), soulseek: false, house: &house, disks: &Roomy };
        let log = Logged::default();
        let writer = log.clone();
        // Beside a second dispatcher (`log_here`): alone, a test reaching the callsite first on another thread keeps it off here.
        let _logging = crate::diagnostics::log_here(tracing_subscriber::fmt().with_writer(move || writer.clone()).with_ansi(false).with_max_level(tracing::Level::INFO).finish());
        round_with(&h, &old, super::super::moment::now_secs()).await.unwrap();
        let delivered: Vec<i64> = old.userdata.requests(None, Some(DELIVERED), LIST).unwrap().iter().map(|r| r.id).collect();
        assert_eq!(delivered.iter().copied().collect::<std::collections::BTreeSet<_>>(), done.iter().copied().collect(), "delivered at the first round");
        let text = log.text();
        assert!(text.lines().any(|l| l.contains("requests delivered") && l.contains(&format!("count={}", done.len()))), "their count in the log: {text}");
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
        drop(old);
        std::fs::remove_dir_all(&d).ok();
    }

    /// AL02.8 ((c); INTERACTIONS top 3): ann's R delivered, then root's Lidarr's FLAC of it in Incoming, came after she asked. Root's
    /// Delete of the FLAC closes nothing: no request is its arrival's, its journal entry keeps no closed list, ann's R stays
    /// delivered, and Lidarr is never told to stop by her group (the strict Lidarr refuses any cancel).
    #[tokio::test]
    async fn deleting_a_later_upgrade_closes_no_delivered_request() {
        const R: &str = "00000000-0000-4000-8000-000000000444";
        let serve = room_shape();
        with_accounts(&serve);
        let (r, _) = held_in(&serve, "Two Rooms", 2, R, ANN, gb(1.0), 200);
        let rq = kept_request(&serve, "ann", r, R, 100, &format!("Lidarr is looking for it; {STOPPED_ON_ARRIVAL}"));
        let (strict, house) = (Strict::expecting_nothing(), Copies::none());
        let h = Hands { lidarr: Some(&strict), soulseek: false, house: &house, disks: &Roomy };
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        assert_eq!(now_of(&serve, rq.id).status, DELIVERED);
        serve.catalogue().copy_for_tests(r, LIDARR);
        serve.catalogue().came_for_tests(LIDARR, 5000);
        let journal = crate::journal::Journal::open(None).unwrap();
        let event = crate::journal::Event::AlbumRejected {
            source: "cratarr".into(),
            album_id: crate::serve::ids::record(r),
            title: "Two Rooms".into(),
            folder: LIDARR,
            dir: format!("copy-{r}"),
            token: "100/Two Rooms".into(),
            release_group: Some(R.into()),
        };
        let id = journal.intend(5100, &event).unwrap();
        journal.applied(id).unwrap();
        let (arrived, open) = (arrived_for(&serve, r, Some(5000)).await, open_of(&serve, r).await);
        assert!(arrived.is_empty() && open.is_empty(), "no request is its arrival's, none open of it");
        let closing = closed_by(arrived, true, Some("root"));
        let copy = DeletedCopy { fetcher: None, folder: "Glass Harbour/Two Rooms".into() };
        assert!(!deleted(Some(&strict), false, &serve, &closing, open, Some(&copy), Some((&journal, id))).await, "no fetcher goes on");
        assert_eq!(journal.result(id, CLOSED_BY_DELETE).unwrap(), None, "its entry's closed list empty");
        assert_eq!(now_of(&serve, rq.id).status, DELIVERED, "ann's R stays delivered");
        assert!(strict.violations().is_empty(), "Lidarr never told to stop by her group: {:?}", strict.violations());
    }

    /// AL02.9's rule at the round (I05; OQ-S; T5), unit: bob added a record ann asked for too to his own music. For ann it is had by
    /// the house but not in her view: it reads wanted, never "In the library", and Lidarr is not asked for it; bob's own request of
    /// it reads done. At the round her copy is made through C52's path (the house's copy, named by folder), and hers reads done once
    /// it is there. A second record he added while she is over her room: hers waits for room, nothing copied, through rounds; once
    /// she is under, the round makes her copy. Never a copy into a full account's music.
    #[tokio::test]
    async fn a_want_held_only_in_anothers_music_is_copied_for_its_asker_room_permitting() {
        const G: &str = "00000000-0000-4000-8000-000000000451";
        const G2: &str = "00000000-0000-4000-8000-000000000452";
        let serve = room_shape();
        with_accounts(&serve);
        let cat = serve.catalogue().clone();
        let (r, files) = held_in(&serve, "Other Room", 1, G, BOB, gb(0.5), 50);
        let hers = kept_request(&serve, "ann", r, G, 100, SOULSEEK_SEARCHES);
        let his = kept_request(&serve, "bob", r, G, 90, SOULSEEK_SEARCHES);
        let holding = named(&serve, std::slice::from_ref(&now_of(&serve, hers.id))).await.pop().flatten().map(|o| o.holding);
        assert_eq!(holding, Some(Holding::Elsewhere), "had by the house, not in her view");
        assert!(matches!(state_of(&serve, hers.id).await, RequestState::Wanted { lidarr: None, .. }), "never In the library for her yet");
        assert_eq!(state_of(&serve, his.id).await, RequestState::Done, "in his own music: his is done");
        let (strict, house) = (Strict::expecting_nothing(), Copies::of(&cat, &files, &[("ann", ANN)], true));
        let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy };
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        assert_eq!(house.asked(), [("ann".to_string(), r)], "her copy, made at the round");
        assert_eq!(now_of(&serve, hers.id).fetched.as_deref(), Some(COPIED_INTO_OWN));
        assert!(matches!(state_of(&serve, hers.id).await, RequestState::Done | RequestState::Delivered), "done once her copy is there");
        // A second record bob added, ann over her room meanwhile.
        let (r2, files2) = held_in(&serve, "Far Room", 1, G2, BOB, gb(0.5), 60);
        let hers2 = kept_request(&serve, "ann", r2, G2, 200, SOULSEEK_SEARCHES);
        put_file(&cat, ANN, "Mine/1.flac", None, gb(101.0), 10, false, 100);
        let house = Copies::of(&cat, &files2, &[("ann", ANN)], true);
        let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy };
        let hers_now = rows_of(&cat, ANN);
        for _ in 0..2 {
            round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
            assert_eq!(now_of(&serve, hers2.id).fetched.as_deref(), Some(WAITS_FOR_ROOM), "over room: hers waits");
            assert!(matches!(state_of(&serve, hers2.id).await, RequestState::WaitingForRoom { .. }));
            assert!(house.asked().is_empty() && rows_of(&cat, ANN) == hers_now, "nothing copied into her full music");
        }
        serve.users.set_room("ann", gb(200.0)).unwrap();
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        assert_eq!(house.asked(), [("ann".to_string(), r2)], "under: her copy made");
        assert!(matches!(state_of(&serve, hers2.id).await, RequestState::Done | RequestState::Delivered));
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
    }

    /// AL02.10: ann's Want waiting for room. (i) She takes it back, then drops under: cancelled, never handed. (ii) The server
    /// restarts while it waits: it still waits (the word on its row), and is handed at the first round once she is under. (iii)
    /// Her two tabs Want R at once while she is full: one row, waiting. Never a cancelled Want handed, nor a waiting one lost.
    #[tokio::test]
    async fn a_waiting_want_survives_a_restart_and_a_cancel_ends_it() {
        const G: &str = "00000000-0000-4000-8000-000000000461";
        let house = Copies::none();
        // (i)
        let serve = room_shape();
        with_accounts(&serve);
        put_file(serve.catalogue(), ANN, "Mine/1.flac", None, gb(101.0), 10, false, 100);
        let r = not_had(&serve, "Night Ferry", G);
        let strict = Strict::expecting_nothing();
        let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy };
        let asked = acting("ann", false, ask_record_with(&h, &serve, "ann", r)).await.unwrap();
        assert_eq!(now_of(&serve, asked.id).fetched.as_deref(), Some(WAITS_FOR_ROOM));
        acting("ann", false, cancel_with(Some(&strict), &serve, "ann", asked.id)).await.unwrap();
        serve.users.set_room("ann", gb(200.0)).unwrap();
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        assert_eq!((now_of(&serve, asked.id).status.as_str(), state_of(&serve, asked.id).await), (CANCELLED, RequestState::Cancelled), "(i) cancelled");
        assert!(strict.wanted().is_empty() && strict.violations().is_empty(), "(i) never handed: {:?}", strict.violations());
        // (ii)
        let dir = std::env::temp_dir().join(format!("cratarr-requests-al02-restart-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join("data")).unwrap();
        let (data, file) = (dir.join("data"), dir.join("catalogue.sqlite"));
        let (id, r) = {
            let serve = room_shape_in(Some(&data), Some(&file));
            with_accounts(&serve);
            put_file(serve.catalogue(), ANN, "Mine/1.flac", None, gb(101.0), 10, false, 100);
            let r = not_had(&serve, "Night Ferry", G);
            let strict = Strict::expecting_nothing();
            let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy };
            let asked = acting("ann", false, ask_record_with(&h, &serve, "ann", r)).await.unwrap();
            assert!(strict.wanted().is_empty() && strict.violations().is_empty());
            (asked.id, r)
        };
        let serve = room_shape_in(Some(&data), Some(&file));
        assert_eq!(now_of(&serve, id).fetched.as_deref(), Some(WAITS_FOR_ROOM), "(ii) the word on its row, after the restart");
        assert_eq!(state_of(&serve, id).await, RequestState::WaitingForRoom { used: gb(101.0), room: gb(100.0) }, "(ii) still waiting");
        serve.users.set_room("ann", gb(200.0)).unwrap();
        let strict = Strict::expecting_wants(&[G]);
        let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy };
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        assert_eq!(strict.wanted(), [G], "(ii) handed at the first round once she is under");
        assert_eq!(now_of(&serve, id).record, Some(r));
        assert!(strict.violations().is_empty(), "(ii) {:?}", strict.violations());
        drop(serve);
        std::fs::remove_dir_all(&dir).ok();
        // (iii)
        let serve = room_shape();
        with_accounts(&serve);
        put_file(serve.catalogue(), ANN, "Mine/1.flac", None, gb(101.0), 10, false, 100);
        let r = not_had(&serve, "Night Ferry", G);
        let strict = Strict::expecting_nothing();
        let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy };
        let (one, two) = tokio::join!(acting("ann", false, ask_record_with(&h, &serve, "ann", r)), acting("ann", false, ask_record_with(&h, &serve, "ann", r)));
        let (one, two) = (one.unwrap(), two.unwrap());
        assert_eq!(one.id, two.id, "(iii) one request");
        assert_eq!(serve.userdata.requests(Some("ann"), None, LIST).unwrap().len(), 1, "(iii) one row");
        assert_eq!(now_of(&serve, one.id).fetched.as_deref(), Some(WAITS_FOR_ROOM), "(iii) waiting");
        assert!(strict.violations().is_empty(), "(iii) {:?}", strict.violations());
    }

    // AL03 (run 13): the disk floor (SCENARIOS-RUNS-13-14's AL03.1, AL03.2; the owner's "No approval per request"; OQ-F1).

    /// AL03.1: the disks users' music lands on 150 GB free, the floor 200 GB; ann under her room. Ann's Want of R: approved, its line
    /// the floor's word, `waiting_for_space`, Lidarr not asked, and Soulseek takes no item of it. Root's Want of S: handed at once (the
    /// strict Lidarr expects his alone). Ann's Want of a record Music holds whole, where the sharing stand-in says her copy shares its
    /// blocks: made, as it takes no new space; of another, where it would not: none made, it waits for space. Never a user's Want
    /// handed below the floor; never an administrator's held by it.
    #[tokio::test]
    async fn below_the_floor_users_wants_wait_and_administrators_go() {
        use crate::app::room::tests::Stand;
        const R: &str = "00000000-0000-4000-8000-000000000481";
        const S: &str = "00000000-0000-4000-8000-000000000482";
        const M1: &str = "00000000-0000-4000-8000-000000000483";
        const M2: &str = "00000000-0000-4000-8000-000000000484";
        let serve = room_shape();
        with_accounts(&serve);
        let cat = serve.catalogue().clone();
        let stand = Stand::of(&[(FOLDERS[1], 150.0), (FOLDERS[2], 150.0)]);
        let (r, s) = (not_had(&serve, "Night Ferry", R), not_had(&serve, "Signal", S));
        let (m1, shares) = held_in(&serve, "Two Rooms", 2, M1, MUSIC, gb(1.0), 50);
        let (m2, apart) = held_in(&serve, "Far Room", 2, M2, MUSIC, gb(1.0), 50);
        let strict = Strict::expecting_wants(&[S]);
        let (sharing, not_sharing) = (Copies::of(&cat, &shares, &[("ann", ANN)], true), Copies::of(&cat, &apart, &[("ann", ANN)], false));
        let h = Hands { lidarr: Some(&strict), soulseek: true, house: &sharing, disks: &stand };
        let anns = acting("ann", false, ask_record_with(&h, &serve, "ann", r)).await.unwrap();
        let row = now_of(&serve, anns.id);
        assert_eq!((row.status.as_str(), row.fetched.as_deref()), (APPROVED, Some(WAITS_FOR_SPACE)), "approved, its line the floor's word");
        assert_eq!(state_of(&serve, row.id).await, RequestState::WaitingForSpace);
        assert!(strict.wanted().is_empty(), "Lidarr not asked for R");
        let (approved, of) = crate::app::soulseek::requests_taken(&serve.userdata, &cat, &serve.library.kinds()).unwrap();
        assert!(!crate::app::soulseek::items_of(&approved, &of).contains(&r), "Soulseek takes no item of R");
        let roots = acting("root", true, ask_record_with(&h, &serve, "root", s)).await.unwrap();
        assert_eq!(strict.wanted(), [S], "root's handed at once, below the floor");
        assert!(roots.fetched.as_deref().is_some_and(|f| f.contains("Lidarr is looking for it")), "{roots:?}");
        let hers = rows_of(&cat, ANN);
        let copied = acting("ann", false, ask_record_with(&h, &serve, "ann", m1)).await.unwrap();
        assert_eq!(copied.fetched.as_deref(), Some(COPIED_INTO_OWN), "her copy that shares its blocks made: no new space");
        assert_eq!(rows_of(&cat, ANN).len(), hers.len() + shares.len(), "its rows in her own music");
        let h = Hands { house: &not_sharing, ..h };
        let hers = rows_of(&cat, ANN);
        let waits = acting("ann", false, ask_record_with(&h, &serve, "ann", m2)).await.unwrap();
        assert_eq!(now_of(&serve, waits.id).fetched.as_deref(), Some(WAITS_FOR_SPACE), "where it would not share: none made, it waits for space");
        assert_eq!(not_sharing.asked(), [("ann".to_string(), m2)], "asked for a copy that shares alone");
        assert_eq!(rows_of(&cat, ANN), hers, "nothing copied into her music");
        assert_eq!(strict.wanted(), [S], "never a user's Want handed below the floor");
        assert!(serve.userdata.requests(None, Some(PENDING), LIST).unwrap().is_empty(), "never a pending row");
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
    }

    /// AL03.2 (INTERACTIONS top 13, I14; OQ-F1 (a)), Lidarr's side: ann's R handed and only looked for (monitored, nothing grabbed),
    /// her R2 in Lidarr's queue (0.4 of it), her R3 Soulseek's alone, root's S looked for. The disk drops below the floor: at the round
    /// Lidarr is told to stop R, once, and R waits for space, never reading fetching; R2 is left to finish, R3 and root's S untouched.
    /// The server restarts below the floor: its round tells Lidarr nothing more, the states as before. The disk rises to the floor and
    /// its margin: at the round R is handed again, once. R2 then arrives: placed in Incoming, it counts in ann's room. Never a second
    /// stop, an administrator's fetch stopped, nor one in Lidarr's queue.
    #[tokio::test]
    async fn below_the_floor_handed_requests_pause_where_they_are() {
        use crate::app::room::tests::Stand;
        const G_R: &str = "00000000-0000-4000-8000-000000000485";
        const G_R2: &str = "00000000-0000-4000-8000-000000000486";
        const G_R3: &str = "00000000-0000-4000-8000-000000000487";
        const G_S: &str = "00000000-0000-4000-8000-000000000488";
        const HANDED: &str = "Lidarr is looking for it; Soulseek searches for it";
        let dir = std::env::temp_dir().join(format!("cratarr-requests-al03-pause-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join("data")).unwrap();
        let (data, file) = (dir.join("data"), dir.join("catalogue.sqlite"));
        let serve = room_shape_in(Some(&data), Some(&file));
        with_accounts(&serve);
        let r2_record = not_had(&serve, "Quiet Lanes", G_R2);
        let r = kept_request(&serve, "ann", not_had(&serve, "Night Ferry", G_R), G_R, 1000, HANDED);
        let r2 = kept_request(&serve, "ann", r2_record, G_R2, 1100, HANDED);
        let r3 = kept_request(&serve, "ann", not_had(&serve, "Signal", G_R3), G_R3, 1200, SOULSEEK_SEARCHES);
        let s = kept_request(&serve, "root", not_had(&serve, "Late Shift", G_S), G_S, 1300, HANDED);
        let statuses = [(G_R, Progress::Looking), (G_R2, Progress::Fetching { done: 0.4 }), (G_S, Progress::Looking)];
        let strict = Strict::expecting_wants(&[G_R]).and_cancels(&[G_R.to_string()]).with_statuses(&statuses);
        let house = Copies::none();
        let stand = Stand::of(&[(FOLDERS[1], 150.0), (FOLDERS[2], 150.0)]);
        let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &stand };
        let lines = |serve: &Serve| [&r, &r2, &r3, &s].map(|q| now_of(serve, q.id).fetched);
        let below = [Some(WAITS_FOR_SPACE.to_string()), Some(HANDED.to_string()), Some(SOULSEEK_SEARCHES.to_string()), Some(HANDED.to_string())];

        // Below the floor: a round.
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        assert_eq!(strict.cancelled(), [G_R], "Lidarr told to stop R alone");
        assert_eq!(lines(&serve), below, "R waits for space; R2, R3 and root's S as they were");
        let viewed = views(&serve, Some(&strict as &dyn Fetcher), vec![now_of(&serve, r.id), now_of(&serve, r2.id), now_of(&serve, s.id)]).await;
        assert_eq!((&viewed[0].state, &viewed[0].album_state), (&RequestState::WaitingForSpace, &Album::Requested), "R waits for space, never fetching");
        assert!(matches!(viewed[1].state, RequestState::Wanted { lidarr: Some(Fetching::Fetching { done }), .. } if (done - 0.4).abs() < 1e-6), "R2 left to finish: {:?}", viewed[1].state);
        assert!(matches!(viewed[2].state, RequestState::Wanted { lidarr: Some(Fetching::Looking), .. }), "root's S looked for still: {:?}", viewed[2].state);

        // The server restarts below the floor: its round tells Lidarr nothing more.
        drop(serve);
        let serve = room_shape_in(Some(&data), Some(&file));
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        assert_eq!(strict.cancelled(), [G_R], "no second stop after the restart");
        assert_eq!(lines(&serve), below, "the states as before");
        assert!(strict.wanted().is_empty(), "nothing handed below the floor");

        // The disk rises to the floor and its margin: R handed again, once.
        for d in [FOLDERS[1], FOLDERS[2]] {
            stand.set(d, Some(210.0));
        }
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        assert_eq!(strict.wanted(), [G_R], "R handed again, once");
        assert_eq!(now_of(&serve, r.id).fetched.as_deref(), Some(HANDED));
        round_with(&h, &serve, super::super::moment::now_secs()).await.unwrap();
        assert_eq!((strict.wanted(), strict.cancelled()), (vec![G_R.to_string()], vec![G_R.to_string()]), "a round on asks nothing more");

        // R2, left to finish, arrives: placed in Lidarr's Incoming, counted in ann's room.
        let cat = serve.catalogue().clone();
        cat.execute_for_tests(&format!("INSERT INTO tracks (born, record, disc, number) VALUES (1, {r2_record}, 1, 1), (1, {r2_record}, 1, 2)"));
        cat.copy_for_tests(r2_record, LIDARR);
        cat.execute_for_tests(&format!("UPDATE files SET size = {}, came = 2000 WHERE folder = {LIDARR}", gb(0.5)));
        assert_eq!(state_of(&serve, r2.id).await, RequestState::Arrived, "R2 placed");
        assert_eq!(used(&serve, "ann"), gb(1.0), "and counted in her room");
        assert!(strict.violations().is_empty(), "{:?}", strict.violations());
        drop(serve);
        std::fs::remove_dir_all(&dir).ok();
    }

    // ---- F2: an artist's own list and a Want meet on one record

    const ANSWERED_GROUP: &str = "00000000-0000-4000-8000-000000002701";
    const ANSWERED_PAGE: &str = "https://kitetheory.bandcamp.com/album/answered-ep";

    /// The scenarios' accounts on `serve`: root, an administrator (the first made); alice and bob, users with Request albums.
    fn askers(serve: &Serve) {
        for (name, admin) in [("root", true), ("alice", false), ("bob", false)] {
            serve.users.create(name, "pw", admin).unwrap();
        }
    }

    /// A row of Kite Theory's own list kept (`keep_listing(Some(artist), …)`): its record.
    fn their_row(serve: &Serve, title: &str, kind: Option<&str>, refs: &[(&str, &str, &str)]) -> i64 {
        let cat = serve.catalogue();
        let artist = cat.artist_for("Kite Theory").unwrap();
        let refs = refs.iter().map(|(s, k, v)| crate::catalogue::records::Ref::new(s, k, v)).collect();
        let row = crate::catalogue::records::Listing { title: title.into(), artist: "Kite Theory".into(), kind: kind.map(str::to_string), refs, ..Default::default() };
        cat.keep_listing(Some(artist), &[row], "link").unwrap()[0]
    }

    /// An untagged had record of Kite Theory's, `title`, its files crediting them.
    fn had_of_theirs(serve: &Serve, title: &str) -> i64 {
        let cat = serve.catalogue();
        let artist = cat.artist_for("Kite Theory").unwrap();
        let record = cat.record_for_tests(title, 3);
        cat.execute_for_tests(&format!("UPDATE records SET artist = 'Kite Theory' WHERE id = {record}"));
        cat.execute_for_tests(&format!("INSERT INTO record_artists (record, artist, position, how) VALUES ({record}, {artist}, 0, 'files')"));
        record
    }

    fn records_now(serve: &Serve) -> i64 {
        serve.catalogue().writer_for_tests().query_row("SELECT count(*) FROM records", [], |r| r.get(0)).unwrap()
    }

    /// F2.7 (kind 4, kind 6: a Want's record). (a) alice Wants Bandcamp's "Answered EP" (its page, no group): record R holds the page.
    /// MusicBrainz's list then names "Answered" (its group): the group joins R; her request still names R, and shows R's names and
    /// the group; Soulseek's item of R keeps its record. (b) A joined record ("Answered", its group and its page): bob's Want from
    /// Bandcamp's card and alice's from MusicBrainz's both name it, and no record is made. (c) OQ-F2b (a): bob's Want of "Answered
    /// EP" by its names alone (no page, no group, as a Spotify Want) names the record not had "Answered", found by its title's key
    /// (X8 permitting); a had record by its title's key only where its type agrees (RG1), this one's the rule cannot place. Never a
    /// Want making a record beside a
    /// joined one that holds its page or group.
    #[tokio::test]
    async fn a_list_after_a_want_joins_its_record_and_a_want_after_finds_it() {
        let house = Copies::none();
        let h = Hands { lidarr: None, soulseek: true, house: &house, disks: &Roomy };
        // (a)
        let serve = served();
        askers(&serve);
        crate::bandcamp::Bandcamp::get().keep(crate::bandcamp::Album {
            url: ANSWERED_PAGE.into(),
            title: "Answered EP".into(),
            artist: "Kite Theory".into(),
            cover: None,
            tracks: Vec::new(),
            released: None,
            label: None,
            account: None,
            about: None,
            credits: None,
            tags: Vec::new(),
        });
        let r = record_of_album_in(&serve, None, "Kite Theory", "Answered EP", None, None, Some(ANSWERED_PAGE.into())).await.unwrap();
        let asked = acting("alice", false, ask_record_with(&h, &serve, "alice", r)).await.unwrap();
        assert_eq!(now_of(&serve, asked.id).record, Some(r), "(a) her request names R");
        assert_eq!(their_row(&serve, "Answered", Some("ep"), &[("musicbrainz", "release-group", ANSWERED_GROUP)]), r, "(a) the group joins R");
        assert_eq!(now_of(&serve, asked.id).record, Some(r), "(a) her request still names R");
        let view = views(&serve, None, vec![now_of(&serve, asked.id)]).await.pop().unwrap();
        assert_eq!(
            (view.request.album.as_str(), view.request.artist.as_str(), view.request.release_group.as_deref()),
            ("Answered EP", "Kite Theory", Some(ANSWERED_GROUP)),
            "(a) R's names, and its group"
        );
        let (approved, of) = crate::app::soulseek::requests_taken(&serve.userdata, serve.catalogue(), &serve.library.kinds()).unwrap();
        assert!(crate::app::soulseek::items_of(&approved, &of).contains(&r), "(a) Soulseek's item of R keeps its record");
        // (b)
        let serve = served();
        askers(&serve);
        let joined = their_row(&serve, "Answered", Some("ep"), &[("musicbrainz", "release-group", ANSWERED_GROUP)]);
        assert_eq!(their_row(&serve, "Answered EP", None, &[("bandcamp", "album", ANSWERED_PAGE)]), joined, "(b) joined");
        let before = records_now(&serve);
        let by_page = record_of_album_in(&serve, None, "Kite Theory", "Answered EP", None, None, Some(ANSWERED_PAGE.into())).await.unwrap();
        let by_group = record_of_album_in(&serve, Some(ANSWERED_GROUP.into()), "Kite Theory", "Answered", None, None, None).await.unwrap();
        let bobs = acting("bob", false, ask_record_with(&h, &serve, "bob", by_page)).await.unwrap();
        let alices = acting("alice", false, ask_record_with(&h, &serve, "alice", by_group)).await.unwrap();
        assert_eq!((now_of(&serve, bobs.id).record, now_of(&serve, alices.id).record), (Some(joined), Some(joined)), "(b) by its page, by its group");
        assert_eq!(records_now(&serve), before, "(b) no record made");
        // (c)
        let serve = served();
        askers(&serve);
        let answered = their_row(&serve, "Answered", Some("ep"), &[("musicbrainz", "release-group", ANSWERED_GROUP)]);
        let by_names = record_of_album_in(&serve, None, "Kite Theory", "Answered EP", None, None, None).await.unwrap();
        let bobs = acting("bob", false, ask_record_with(&h, &serve, "bob", by_names)).await.unwrap();
        assert_eq!(now_of(&serve, bobs.id).record, Some(answered), "(c) by its title's key: the record not had \"Answered\"");
        let owned = had_of_theirs(&serve, "Kyverdale Road");
        let loose = record_of_album_in(&serve, None, "Kite Theory", "Kyverdale Road EP", None, None, None).await.unwrap();
        assert_ne!(loose, owned, "(c) a had record by its title's key only where its type agrees (RG1): the rule cannot place this one");
        assert_eq!(record_of_album_in(&serve, None, "Kite Theory", "Kyverdale Road", None, None, None).await.unwrap(), owned, "(c) by its exact names");
    }

    /// F2.9's Want: MusicBrainz's row of an untagged had record's names joins it (its group on it); alice's Want by that group then
    /// finds the had record: "Already in the library", no fetcher asked (the strict Lidarr expects nothing).
    #[tokio::test]
    async fn a_list_row_joins_a_had_record_of_its_names() {
        const G: &str = "00000000-0000-4000-8000-000000002901";
        let serve = served();
        askers(&serve);
        let owned = had_of_theirs(&serve, "Kyverdale Road");
        let joined = their_row(&serve, "Kyverdale Road", Some("album"), &[("musicbrainz", "release-group", G)]);
        assert_eq!(serve.catalogue().record(joined).unwrap().unwrap().id, owned, "MusicBrainz's row joins the had record");
        let (strict, house) = (Strict::expecting_nothing(), Copies::none());
        let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy };
        let wanted = record_of_album_in(&serve, Some(G.into()), "Kite Theory", "Kyverdale Road", None, None, None).await.unwrap();
        assert_eq!(wanted, owned, "her Want finds the had record by its group");
        let asked = acting("alice", false, ask_record_with(&h, &serve, "alice", wanted)).await.unwrap();
        assert_eq!(now_of(&serve, asked.id).fetched.as_deref(), Some(ALREADY_HAD), "in the library");
        assert!(strict.violations().is_empty() && strict.wanted().is_empty(), "no fetcher asked: {:?}", strict.violations());
    }

    /// RG1 (the owner, 2026-11-18: "When the type agrees"): a Want by its names alone (no group, no page: a Spotify Want) whose title
    /// differs from a had record's by "EP" or "LP" finds it where the had record's class agrees: "Kite EP" a had EP "Kite" (four
    /// tracks), "Kite LP" a had album "Kite" (ten); asked, it is in the library at once, no fetcher asked. Beside a had album "Kite" a
    /// Want "Kite EP", or beside a had EP a Want "Kite LP", takes a record of its own, not had, and stays open.
    #[tokio::test]
    async fn a_want_by_names_finds_a_had_record_only_where_its_type_agrees() {
        for (case, tracks, title, joins) in [("EP, a had EP", 4, "Kite EP", true), ("EP, a had album", 10, "Kite EP", false), ("LP, a had album", 10, "Kite LP", true), ("LP, a had EP", 4, "Kite LP", false)] {
            let serve = served();
            askers(&serve);
            let cat = serve.catalogue();
            let artist = cat.artist_for("Kite Theory").unwrap();
            let kite = cat.record_for_tests("Kite", tracks);
            cat.execute_for_tests(&format!("UPDATE records SET artist = 'Kite Theory' WHERE id = {kite}"));
            cat.execute_for_tests(&format!("INSERT INTO record_artists (record, artist, position, how) VALUES ({kite}, {artist}, 0, 'files')"));
            let wanted = record_of_album_in(&serve, None, "Kite Theory", title, None, None, None).await.unwrap();
            assert_eq!(wanted == kite, joins, "{case}: found the had record");
            let (strict, house) = (Strict::expecting_nothing(), Copies::none());
            if joins {
                let h = Hands { lidarr: Some(&strict), soulseek: true, house: &house, disks: &Roomy };
                let asked = acting("alice", false, ask_record_with(&h, &serve, "alice", wanted)).await.unwrap();
                assert_eq!(now_of(&serve, asked.id).fetched.as_deref(), Some(ALREADY_HAD), "{case}: in the library");
                assert!(strict.violations().is_empty() && strict.wanted().is_empty(), "{case}: no fetcher asked: {:?}", strict.violations());
            } else {
                let h = Hands { lidarr: None, soulseek: true, house: &house, disks: &Roomy };
                let asked = acting("alice", false, ask_record_with(&h, &serve, "alice", wanted)).await.unwrap();
                assert!(!cat.record(wanted).unwrap().unwrap().had, "{case}: a record of its own, not had");
                let now = now_of(&serve, asked.id);
                assert!(now.status == super::APPROVED && now.fetched.as_deref() != Some(ALREADY_HAD), "{case}: open: {now:?}");
            }
        }
    }
}
