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
    let record = record_of_album_in(serve, s.release_group.clone(), &s.artist, &s.album, s.year, s.cover.clone()).await?;
    asked_record(app, serve, account, record, Some(s.key)).await
}

/// Any album asked for by its names (0.9.3, the owner: Want for any album an artist's page says is missing): keyed as
/// Discover keys albums (its release group, else its artist's and its name's), so its suggestion leaves Discover too; its
/// record found or made first (`record_of_album`, C49a), whose names the request shows.
#[allow(clippy::too_many_arguments)]
pub async fn ask_album(app: &super::App, serve: &Serve, account: &str, release_group: Option<String>, artist: &str, album: &str, year: Option<i32>, cover: Option<String>) -> Result<Request> {
    if !rights(serve).request {
        bail!("Requesting albums is not on for this account");
    }
    let release_group = release_group.filter(|g| !g.is_empty());
    let key = release_group.clone().unwrap_or_else(|| format!("{}|{}", super::import::key(artist), super::import::key(album)));
    if let Some(r) = open_request(serve, account, &key)? {
        return Ok(r);
    }
    let record = record_of_album_in(serve, release_group, artist, album, year, cover).await?;
    asked_record(app, serve, account, record, Some(key)).await
}

/// A record asked for by its id (C49a): an artist's or a label's missing record, any record not had. The account's open
/// request for it is the answer where there is one.
pub async fn ask_record(app: &super::App, serve: &Serve, account: &str, record: i64) -> Result<Request> {
    if !rights(serve).request {
        bail!("Requesting albums is not on for this account");
    }
    asked_record(app, serve, account, record, None).await
}

/// The request for `record`: its names, year, cover and release group the record's (the first group it holds), its key the one
/// given (Discover's, so its suggestion leaves it), else the record's group, else its names'.
async fn asked_record(app: &super::App, serve: &Serve, account: &str, record: i64, key: Option<String>) -> Result<Request> {
    let cat = serve.catalogue().clone();
    let (row, group) = tokio::task::spawn_blocking(move || -> Result<_> {
        let row = cat.record(record)?.context("That record is gone")?;
        let group = cat.references_of(&crate::serve::ids::record(row.id))?.into_iter().find(|(r, _)| r.source == "musicbrainz" && r.kind == "release-group").map(|(r, _)| r.value);
        Ok((row, group))
    })
    .await??;
    let key = key.or_else(|| group.clone()).unwrap_or_else(|| format!("{}|{}", super::import::key(&row.artist), super::import::key(&row.title)));
    let asked = Asked { record: row.id, key, artist_key: super::import::loose(&row.artist), release_group: group, album: row.title, artist: row.artist, year: row.year, cover: row.cover };
    ask_for(app, serve, account, asked).await
}

/// The account's open request (pending or approved) for an album by its key, where there is one.
fn open_request(serve: &Serve, account: &str, key: &str) -> Result<Option<Request>> {
    Ok(serve.userdata.requests(Some(account), None, LIST)?.into_iter().find(|r| r.album_key == key && (r.status == PENDING || r.status == APPROVED)))
}

/// The record an album asked for by its names is (C49a): its names as a label's own account gives them sorted out first (S12,
/// `bandcamp::label_release`: a Want from a label's Bandcamp page, "Artist - Title (Label)" by the label, is the artist's
/// "Title", the label credited), then the record of its release group, else a record had of its names, else a new record not
/// had (`Catalogue::record_for_request`). Blocking.
pub fn record_of_album(cat: &crate::catalogue::Catalogue, release_group: Option<&str>, artist: &str, album: &str, year: Option<i32>, cover: Option<String>) -> Result<i64> {
    let named = crate::bandcamp::label_release(artist, album, None);
    let wanted = crate::catalogue::records::Wanted { group: release_group.map(str::to_string), artist: named.artist, title: named.title, label: named.label, year, cover, page: None };
    cat.record_for_request(&wanted)
}

async fn record_of_album_in(serve: &Serve, release_group: Option<String>, artist: &str, album: &str, year: Option<i32>, cover: Option<String>) -> Result<i64> {
    let (cat, artist, album) = (serve.catalogue().clone(), artist.to_string(), album.to_string());
    tokio::task::spawn_blocking(move || record_of_album(&cat, release_group.as_deref(), &artist, &album, year, cover)).await?
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
    let open = serve.userdata.requests(Some(account), None, LIST)?;
    if let Some(r) = open.into_iter().find(|r| (r.record == Some(s.record) || r.album_key == s.key) && (r.status == PENDING || r.status == APPROVED)) {
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
    r.id = serve.userdata.add_request(&r)?;
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
    let r = shown(r.clone(), named(serve, std::slice::from_ref(r)).await.pop().flatten().as_ref());
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
            // The release group its record holds (`shown`): the one Lidarr was told to want.
            let group = shown(r.clone(), named(serve, std::slice::from_ref(&r)).await.pop().flatten().as_ref()).release_group;
            if let (Some(f), Some(rg)) = (fetcher(app), group.as_deref()) {
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
    let f = fetcher(app);
    let mine = views(serve, f.as_deref(), mine).await;
    let waiting = match waiting {
        Some(w) => Some(views(serve, f.as_deref(), w).await),
        None => None,
    };
    let fetchers = connected(f.is_some()).await;
    Ok(Listing { mine, waiting, may, fetchers })
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
    Ok(views(serve, f.as_deref(), newest).await.into_iter().map(|v| (v.request.album_key.clone(), v)).collect())
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

/// Who asked for each approved request, by its record (C49b, merges followed): an Incoming record goes into its requester's
/// own music by default, and is theirs to add or delete (0101 point 2). The newest asker of a record.
pub fn requesters(serve: &Serve) -> HashMap<i64, String> {
    let approved = serve.userdata.requests(None, Some(APPROVED), OPEN).unwrap_or_default();
    let of = records_of(serve.catalogue(), &approved).unwrap_or_else(|e| {
        tracing::debug!("the requests' records not read: {e:#}");
        vec![None; approved.len()]
    });
    approved.into_iter().zip(of).rev().filter_map(|(r, o)| Some((o?.record, r.user))).collect()
}

/// An album's mark where a list has no record id for it: by its release group, else by its artist's and its name's key (a
/// Bandcamp album's Want), as `marks_of` keys them.
pub fn mark_of(marks: &Marks, release_group: Option<&str>, artist: &str, album: &str) -> Option<Mark> {
    release_group.filter(|g| !g.is_empty()).and_then(|g| marks.by_key.get(g)).or_else(|| marks.by_key.get(&requested(artist, album))).copied()
}

/// What a request's record says now (C49b): the id it answers by (a merge followed), its release groups, its names, year and
/// cover, and whether it is had.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecordOf {
    pub record: i64,
    pub groups: Vec<String>,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub cover: Option<String>,
    pub had: bool,
}

/// Each request's record as the catalogue holds it now, in the requests' order: none where it names no record yet (before
/// 0.17's step) or one that is gone. Blocking.
pub fn records_of(cat: &crate::catalogue::Catalogue, rs: &[Request]) -> Result<Vec<Option<RecordOf>>> {
    rs.iter()
        .map(|r| {
            let Some(row) = r.record.map(|id| cat.record(id)).transpose()?.flatten() else { return Ok(None) };
            let groups = cat.references_of(&crate::serve::ids::record(row.id))?.into_iter().filter(|(x, _)| x.source == "musicbrainz" && x.kind == "release-group").map(|(x, _)| x.value).collect();
            Ok(Some(RecordOf { record: row.id, groups, title: row.title, artist: row.artist, year: row.year, cover: row.cover, had: row.had }))
        })
        .collect()
}

/// `records_of` from an async caller; none for each where the catalogue cannot be read (logged).
async fn named(serve: &Serve, rs: &[Request]) -> Vec<Option<RecordOf>> {
    let (cat, rs2) = (serve.catalogue().clone(), rs.to_vec());
    match tokio::task::spawn_blocking(move || records_of(&cat, &rs2)).await {
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
/// with, and the id its record answers by, where it has a record.
fn shown(mut r: Request, of: Option<&RecordOf>) -> Request {
    if let Some(o) = of {
        r.album = o.title.clone();
        r.artist = o.artist.clone();
        r.year = o.year.or(r.year);
        r.cover = o.cover.clone().or(r.cover);
        r.release_group = o.groups.first().cloned().or(r.release_group);
        r.record = Some(o.record);
    }
    r
}

/// Requests as the pages show them (`shown`), their records read here (Health's list). Blocking.
pub fn shown_requests(serve: &Serve, rs: Vec<Request>) -> Vec<Request> {
    let of = records_of(serve.catalogue(), &rs).unwrap_or_else(|_| vec![None; rs.len()]);
    rs.into_iter().zip(of).map(|(r, o)| shown(r, o.as_ref())).collect()
}

/// How many requests wait for an answer (Health, for those who answer them).
pub fn waiting(serve: &Serve) -> Result<usize> {
    serve.userdata.requests_waiting()
}

/// Requests as the pages show them, with their albums' states (C49b): each by its record (`shown`), in the library where its
/// record is had; the fetcher asked once, for every approved one not had.
async fn views(serve: &Serve, f: Option<&dyn Fetcher>, rs: Vec<Request>) -> Vec<View> {
    let of = named(serve, &rs).await;
    views_of(f, rs, &of).await
}

/// `views`, each request's record read (`of`, in the requests' order).
async fn views_of(f: Option<&dyn Fetcher>, rs: Vec<Request>, of: &[Option<RecordOf>]) -> Vec<View> {
    let rs: Vec<(Request, bool)> = rs.into_iter().zip(of).map(|(r, o)| (shown(r, o.as_ref()), o.as_ref().is_some_and(|o| o.had))).collect();
    let asked: Vec<String> = rs.iter().filter(|(r, there)| !*there && r.status == APPROVED).filter_map(|(r, _)| r.release_group.clone()).collect();
    let progress = match f {
        Some(f) if !asked.is_empty() => f.statuses(&asked).await.ok(),
        _ => None,
    };
    rs.into_iter().map(|(r, there)| View { album_state: state(there, progress.as_ref(), &r), request: r }).collect()
}

/// Of these requests, those whose record is not had yet (the owner, 2026-10-30: "Once something is actually in the library,
/// there's no reason to show it in any of the requested sections except for in the requests control panel"), each as the
/// pages show it (`shown`, C49b).
pub async fn not_in_library(serve: &Serve, rs: Vec<Request>) -> Vec<Request> {
    let of = named(serve, &rs).await;
    rs.into_iter().zip(of).filter(|(_, o)| !o.as_ref().is_some_and(|o| o.had)).map(|(r, o)| shown(r, o.as_ref())).collect()
}

/// The library's albums by name (the import's matcher), where Cratarr's data folder holds it: its albums alone, one
/// reading shared by those asking at once and kept until the library is built again (`Matcher::library_albums`;
/// 0.11.1: /api/discover's memory, 2026-10-30: each Discover request read every song of the library here, twice, some
/// 70 MB each time on the NAS).
pub(crate) async fn library(serve: &Serve) -> Option<Arc<Matcher>> {
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
        let views = vec![View { request: shown(r, of[0].as_ref()), album_state: Album::Requested }];
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

    /// A server of the tests' own, in memory, its catalogue attached.
    fn served() -> Arc<Serve> {
        let serve = Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap()));
        serve
    }

    /// The Lidarr a test hands requests to: what it was asked to want; the release group it finds.
    #[derive(Default)]
    struct Wanting(Mutex<Vec<(Option<String>, String, String)>>);

    const FOUND: &str = "00000000-0000-4000-8000-000000000131";

    impl Fetcher for Wanting {
        fn want<'a>(&'a self, release_group: Option<&'a str>, artist: &'a str, album: &'a str) -> crate::source::Fut<'a, crate::lidarr::Handed> {
            self.0.lock().unwrap().push((release_group.map(str::to_string), artist.to_string(), album.to_string()));
            Box::pin(async { Ok(crate::lidarr::Handed { said: "Lidarr added it and is looking for it".into(), release_group: FOUND.into() }) })
        }

        fn statuses<'a>(&'a self, _: &'a [String]) -> crate::source::Fut<'a, HashMap<String, Progress>> {
            Box::pin(async { Ok(HashMap::new()) })
        }

        fn cancel<'a>(&'a self, _: &'a str) -> crate::source::Fut<'a, ()> {
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
