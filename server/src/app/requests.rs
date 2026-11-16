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
    let release_group = release_group.filter(|g| !g.is_empty());
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
    if held.is_empty()
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

/// The account's open request (pending or approved) for an album by its key, where there is one.
fn open_request(serve: &Serve, account: &str, key: &str) -> Result<Option<Request>> {
    Ok(serve.userdata.requests(Some(account), None, LIST)?.into_iter().find(|r| r.album_key == key && (r.status == PENDING || r.status == APPROVED)))
}

/// The record an album asked for by its names is (C49a): its names as a label's own account gives them sorted out first (S12,
/// `bandcamp::label_release`: a Want from a label's Bandcamp page, "Artist - Title (Label)" by the label, is the artist's
/// "Title", the label credited), then the record of its release group, else a record had of its names, else a new record not
/// had (`Catalogue::record_for_request`). Blocking.
pub fn record_of_album(cat: &crate::catalogue::Catalogue, release_group: Option<&str>, artist: &str, album: &str, year: Option<i32>, cover: Option<String>) -> Result<i64> {
    record_of_album_paged(cat, release_group, crate::bandcamp::label_release(artist, album, None), year, cover, None)
}

/// `record_of_album`, its names sorted out by the caller, its Bandcamp album page given (R22): kept on the record as a reference
/// (`search`), so a record only Bandcamp lists is judged by Bandcamp's list.
fn record_of_album_paged(cat: &crate::catalogue::Catalogue, release_group: Option<&str>, named: crate::bandcamp::Named, year: Option<i32>, cover: Option<String>, page: Option<String>) -> Result<i64> {
    let wanted = crate::catalogue::records::Wanted { group: release_group.map(str::to_string), artist: named.artist, title: named.title, label: named.label, year, cover, page };
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
    tokio::task::spawn_blocking(move || -> Result<i64> {
        let found = |key: &str| -> Result<Option<String>> { Ok(ud.bandcamp_found(key)?.and_then(|(_, page)| page)) };
        let record = record_of_album_paged(&cat, group.as_deref(), named, year, cover, page.clone())?;
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
    let of = records_of(serve.catalogue(), &approved).unwrap_or_else(|e| {
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
}

impl RecordOf {
    /// Whether its album is in the library.
    pub fn had(&self) -> bool {
        self.holder.is_some()
    }
}

/// Each request's record as the catalogue holds it now, in the requests' order: none where it names no record yet (before
/// 0.17's step) or one that is gone. All read at once on one reader, never under the writer's lock (R16,
/// `Catalogue::requested`): the Albums page and the marks ask for every open request while builds run. Blocking.
pub fn records_of(cat: &crate::catalogue::Catalogue, rs: &[Request]) -> Result<Vec<Option<RecordOf>>> {
    let read = cat.requested(&rs.iter().filter_map(|r| r.record).collect::<Vec<_>>())?;
    Ok(rs
        .iter()
        .map(|r| {
            let q = read.get(&r.record?)?;
            Some(RecordOf { record: q.row.id, groups: q.groups.clone(), title: q.row.title.clone(), artist: q.row.artist.clone(), year: q.row.year, cover: q.row.cover.clone(), holder: q.holder })
        })
        .collect())
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
    let rs: Vec<(Request, bool)> = rs.into_iter().zip(of).map(|(r, o)| (shown(r, o.as_ref()), o.as_ref().is_some_and(RecordOf::had))).collect();
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
    rs.into_iter().zip(of).filter(|(_, o)| !o.as_ref().is_some_and(RecordOf::had)).map(|(r, o)| shown(r, o.as_ref())).collect()
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
