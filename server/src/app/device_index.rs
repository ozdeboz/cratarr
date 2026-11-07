//! The library on a device (0101's points 16 to 18): the rules crate's `Index`, made here from what the server holds, sent whole,
//! and then as it changes. Nothing of it is kept between requests: 34,000 tracks pack in about a second, a device fetches them
//! once, and the server is held to 768 MB, so each asking builds its bytes and lets them go. One build at a time
//! (`Serve::device_turn`), as two would hold twice what one holds.
//!
//! - **Rows are the served library's, facts the crates'.** A track's own fields, its record and its artists come from the
//!   library (`Library::index_rows`), its `Facts` as `Catalogue::crate_facts` reads them, by reference: the reading the pages
//!   share, not a copy of it.
//! - **What an account can see** ([`can_see`]) is every library, off ones too: 0084's "every user can switch every library",
//!   0101's point 6, no private own music. Off is a switch of theirs, which the device reads from their preferences, so that
//!   switching one on works offline. A library that is not seen leaves every row that names it ([`within`]).
//! - **An own library goes by its owner**, not as "Your music": one index for everyone who sees the same libraries, and the
//!   device knows who is signed in.
//! - **The feed is a whole build, compared.** What a row says comes from a dozen places, some of them no write to anything here
//!   (`Catalogue::index_settle`), and what it says is worked out in only one way. So a device's rows come from the index a fresh
//!   fetch would be sent, and what it lacks is what the log says moved: applied, it is that index, to the byte. Asked with nothing
//!   moved, nothing is built.
//! - **Rows, not base64:** the changes have a route of their own, `/api/index/changes`, in the bytes `Changes::save` writes, gzip
//!   over them. As text in `/api/changes` they would be a third larger, and a retag of a library would pass through the
//!   page's heap as one string. `/api/changes` says where the log stands, and whether there is anything to fetch.
//! - **The sounds go apart** (0101's point 16): eight bits each, from what the crates hold, and only for what a device asks for
//!   them, as they weigh more than all the rest and change rarely. Their rev is the log's, and their feed its `sound` rows.
//! - **What is an account's own goes apart too** (0101's point 18), as `index::Marks`: its fits and its last counts, and all else a
//!   crate and a pick read of it (`index::own`), worked out on asking, with no feed. They move with every play and are a small part
//!   of what the index weighs.
//! - **Small covers** have a file of their own (`device_covers`): they are no rows, and they are made in the background.
//! - **"Start again":** a rev from before what is kept or ahead of it, or more changed than rows are worth (a library in front of
//!   the others moves every line): 410 with `{rev, reset: true}`, as `/api/changes` says it, and the device fetches whole.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Weak};

use anyhow::{Result, anyhow};
use axum::Json;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use cratarr_rules::index::{self, Artist, Builder, Gone, Index, Kind, Refused, Rows, Sounds, TrackHead};

use super::changes::IndexStands;
use super::{App, ToggleError};
use crate::crates::Facts;
use crate::serve::Serve;
use crate::serve::ids;
use crate::settings::LibraryKind;

/// A refusal of the rules crate's, as the server reports it: rows that do not add up are a fault here, not a device's.
fn refused(what: Refused) -> anyhow::Error {
    anyhow!("the index could not be made: {what:?}")
}

/// Whether `account` can see `library`. Every account every library, today: see the module's notes. What this says no to, the
/// rest of this file leaves out.
fn can_see(_library: &index::Library, _account: Option<&str>) -> bool {
    true
}

impl super::library::Library {
    /// Every catalogue folder as a device names it: its library where one holds it, else its path, so a server's library not
    /// answering for now does not take its rows off every device. By its id, a folder's, `order` saying the libraries' order. Blocking.
    pub(super) fn index_libraries(&self, serve: &Serve) -> Vec<index::Library> {
        let (kinds, live, folders) = (self.folder_kinds(), self.live(), self.catalogue().folders());
        let of = |path: &Path| {
            live.iter().find_map(|a| {
                let placed = a.placement.libraries.iter().find(|p| p.folders.iter().any(|f| Path::new(f) == path))?;
                Some((a, placed, a.libraries().into_iter().find(|u| u.id == placed.id)))
            })
        };
        let mut rows: Vec<index::Library> = folders
            .iter()
            .enumerate()
            .map(|(n, (folder, path))| {
                let at = kinds.iter().position(|(f, _)| *f == folder);
                let order = at.unwrap_or(kinds.len() + n) as u32;
                let kind = match at.map_or(LibraryKind::Incoming, |at| kinds[at].1) {
                    LibraryKind::Main => Kind::Main,
                    LibraryKind::Own => Kind::Own,
                    LibraryKind::Incoming => Kind::Incoming,
                };
                let (key, name) = (path.display().to_string(), path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned()));
                match of(path.as_path()) {
                    Some((a, placed, used)) => {
                        let owner = used.and_then(|u| u.owner).filter(|o| serve.users.get(o).is_some_and(|u| u.active));
                        index::Library { id: folder, key: placed.id.clone(), name: placed.name.clone(), kind, order, colour: a.colour_of(&placed.id), owner }
                    }
                    None => index::Library { id: folder, key, name, kind, order, colour: None, owner: None },
                }
            })
            .collect();
        rows.sort_by_key(|l| l.id);
        rows
    }
}

/// A track's facts as an account sees them: what lies in a library it cannot see goes from `folders` and `lines`, or the device
/// would refuse the row as naming what it does not hold, and would show where it lies. Its line is another's where its own lies
/// out of sight, that folder first, as `Facts::folders` has it. None where nothing is left. The usual track, wholly in sight,
/// costs no copy.
fn within<'f>(f: &'f Facts, seen: &HashSet<i64>, folder_of: &dyn Fn(&str) -> Option<i64>) -> Option<Cow<'f, Facts>> {
    if f.folders.iter().all(|x| seen.contains(x)) {
        return Some(Cow::Borrowed(f));
    }
    let mut folders: Vec<i64> = f.folders.iter().copied().filter(|x| seen.contains(x)).collect();
    let mut lines: Vec<String> = f.lines.iter().filter(|l| folder_of(l.as_str()).is_some_and(|x| seen.contains(&x))).cloned().collect();
    let rel = if f.folders.first().is_some_and(|x| seen.contains(x)) {
        f.rel.clone()
    } else {
        if lines.is_empty() {
            return None;
        }
        let rel = lines.remove(0);
        let first = folder_of(rel.as_str());
        folders.sort_by_key(|x| Some(*x) != first);
        rel
    };
    (!folders.is_empty()).then(|| Cow::Owned(Facts { rel, lines, folders, ..f.clone() }))
}

/// Every other way the files write each artist, as `crates::artist_ids` makes one artist of them: their names by id.
fn spellings(serve: &Serve, facts: &[Facts]) -> Result<HashMap<i64, Vec<String>>> {
    let mut out: HashMap<i64, Vec<String>> = HashMap::new();
    for (name, id) in crate::crates::artist_ids(facts, &serve.artist_ids()?) {
        if let Some(id) = id.strip_prefix(ids::ARTIST).and_then(|n| n.parse().ok()) {
            out.entry(id).or_default().push(name);
        }
    }
    out.values_mut().for_each(|names| names.sort_unstable());
    Ok(out)
}

/// The index of these `libraries`, as of `rev`: every track of them the catalogue's `facts` and the served library both hold,
/// each packed in its turn, so all that is held of a track is what the index keeps of it. A song not read yet, or read and not
/// built yet, waits for both. Blocking.
pub fn build(serve: &Serve, facts: &[Facts], libraries: &[index::Library], rev: i64) -> Result<Index> {
    let mut listed = serve.library.index_rows()?;
    let spelled = spellings(serve, facts)?;
    let seen: HashSet<i64> = libraries.iter().map(|l| l.id).collect();
    let mut to = Builder::new();
    libraries.iter().for_each(|l| to.library(l));
    listed.artists.sort_unstable_by_key(|a| a.id);
    for a in listed.artists {
        let spelled: Vec<String> = spelled.get(&a.id).map(|names| names.iter().filter(|n| **n != a.name).cloned().collect()).unwrap_or_default();
        to.artist(&Artist { id: a.id, name: a.name, sort: a.sort, spelled });
    }
    listed.records.sort_unstable_by_key(|r| r.id);
    for r in &mut listed.records {
        r.copies.retain(|c| seen.contains(&c.library));
        to.record(r);
    }
    let of_track: HashMap<i64, &Facts> = facts.iter().filter_map(|f| Some((f.track?, f))).collect();
    let cat = serve.catalogue();
    let folder_of = |line: &str| cat.place_of_line(line).map(|p| p.0);
    listed.songs.sort_unstable_by_key(|s| s.track);
    for s in &listed.songs {
        if let Some(f) = of_track.get(&s.track).and_then(|f| within(f, &seen, &folder_of)) {
            to.track(TrackHead { id: s.track, record: s.record, disc: s.disc, number: s.number, title: &s.title, artist: s.artist.as_deref(), artist_ids: &s.artists }, &f);
        }
    }
    to.build(rev).map_err(refused)
}

/// What the log was last brought up to: all that can move a row without moving the log. The reading only weakly, so as not to
/// hold on to it, and so its address stays taken: a reading made in the same place would pass for it (`Catalogue::crate_facts`).
#[derive(Debug, Default)]
pub struct Settled {
    facts: Weak<Vec<Facts>>,
    built: i64,
    rev: i64,
    libraries: Vec<index::Library>,
}

impl Settled {
    /// Whether what was settled is what stands now: no row can differ, and nothing needs building.
    fn holds(&self, facts: &Arc<Vec<Facts>>, built: i64, rev: i64, libraries: &[index::Library]) -> bool {
        self.rev == rev && self.stands(facts, built, libraries)
    }

    /// `holds`, but for the log: the reading, the build and the libraries, which move rows with no word in it.
    fn stands(&self, facts: &Arc<Vec<Facts>>, built: i64, libraries: &[index::Library]) -> bool {
        std::ptr::eq(self.facts.as_ptr(), Arc::as_ptr(facts)) && self.built == built && self.libraries == libraries
    }
}

/// The sounds of `index`'s tracks, as of its rev: a track's taken from the reading in its turn, and held in eight bits.
pub(super) fn sounds_of(facts: &[Facts], index: &Index) -> Sounds {
    let mut heard: Vec<(i64, &[f32])> = facts.iter().filter_map(|f| Some((f.track?, f.sound.as_deref()?))).filter(|(track, _)| index.holds(*track)).collect();
    heard.sort_unstable_by_key(|h| h.0);
    Sounds::from_sorted(index.rev(), heard)
}

/// The index of every library, and the log brought up to it (`Catalogue::index_settle`): as of where the log was read (`read_at`),
/// before any row was, as what is written while this reads is logged past that and comes again by the feed, where the other way
/// round it would be lost. Blocking.
fn settle(serve: &Serve, turn: &mut Settled, facts: &Arc<Vec<Facts>>, read_at: i64, libraries: &[index::Library]) -> Result<Index> {
    let built = serve.built_at();
    let index = build(serve, facts, libraries, read_at)?;
    let (prints, sounds) = (index.prints(), sounds_of(facts, &index).prints());
    let rev = serve.catalogue().index_settle(read_at, &[("library", &prints.libraries[..]), ("artist", &prints.artists[..]), ("record", &prints.records[..]), ("track", &prints.tracks[..]), ("sound", &sounds[..])])?;
    *turn = Settled { facts: Arc::downgrade(facts), built, rev, libraries: libraries.to_vec() };
    Ok(index.at(rev))
}

/// `index`, of every library, as `account` sees it. The same one where it sees them all. Blocking.
fn seen_by(serve: &Serve, facts: &[Facts], index: Index, libraries: &[index::Library], account: Option<&str>) -> Result<Index> {
    let seen: Vec<index::Library> = libraries.iter().filter(|l| can_see(l, account)).cloned().collect();
    if seen.len() == libraries.len() {
        return Ok(index);
    }
    let rev = index.rev();
    drop(index);
    build(serve, facts, &seen, rev)
}

/// The index `account` can see, as it stands, and the reading it was made of. Blocking.
pub(super) fn whole_of(serve: &Serve, libraries: &[index::Library], account: Option<&str>) -> Result<(Index, Arc<Vec<Facts>>)> {
    let mut turn = serve.device_turn();
    let cat = serve.catalogue();
    let read_at = cat.index_rev()?;
    let facts = cat.crate_facts()?;
    let index = settle(serve, &mut turn, &facts, read_at, libraries)?;
    Ok((seen_by(serve, &facts, index, libraries, account)?, facts))
}

/// `whole_of`, the index alone. Blocking.
fn whole(serve: &Serve, libraries: Vec<index::Library>, account: Option<&str>) -> Result<Index> {
    Ok(whole_of(serve, &libraries, account)?.0)
}

/// A batch of what `logged` names, as `index` has it, whole: a row left out of it has left. Each row brings what it names, as a
/// device may not hold it. A record and an artist need no word when they leave: nothing names them on a device then either.
fn batch(index: &Index, logged: &BTreeMap<String, Vec<i64>>) -> (Rows, Gone) {
    let of = |kind: &str| logged.get(kind).map_or(&[][..], Vec::as_slice);
    let (mut rows, mut gone) = (Rows { libraries: index.libraries(), ..Rows::default() }, Gone::default());
    let (mut records, mut artists): (BTreeSet<i64>, BTreeSet<i64>) = (of("record").iter().copied().collect(), of("artist").iter().copied().collect());
    for id in of("track") {
        match index.track(*id) {
            Some(t) => {
                records.insert(t.record);
                artists.extend(&t.artist_ids);
                rows.tracks.push(t);
            }
            None => gone.tracks.push(*id),
        }
    }
    for r in records.iter().filter_map(|id| index.record(*id)) {
        artists.extend(&r.artist_ids);
        rows.records.push(r);
    }
    rows.artists.extend(artists.iter().filter_map(|id| index.artist(*id)));
    gone.libraries.extend(of("library").iter().filter(|id| !rows.libraries.iter().any(|l| l.id == **id)));
    (rows, gone)
}

/// What a device at a rev is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feed {
    /// What it lacks, as `Changes::save` writes it.
    Changes(Bytes),
    /// It stands where the library does.
    Same,
    /// Start again: it fetches whole, and goes on from what that says.
    Whole { rev: i64 },
}

impl IntoResponse for Feed {
    fn into_response(self) -> Response {
        match self {
            Feed::Changes(bytes) => bytes.into_response(),
            Feed::Same => StatusCode::NO_CONTENT.into_response(),
            Feed::Whole { rev } => (StatusCode::GONE, Json(serde_json::json!({ "rev": rev, "reset": true }))).into_response(),
        }
    }
}

/// Who asks the feed from where, on which reading.
struct Asked<'a> {
    facts: &'a Arc<Vec<Facts>>,
    read_at: i64,
    libraries: &'a [index::Library],
    account: Option<&'a str>,
    since: i64,
    takes_gzip: bool,
}

/// Past this, rows cost as much as the whole and say less: a library in front of the others moves every line.
fn more_than_rows_are_worth(moved: usize, of: usize) -> bool {
    moved * 2 > of
}

/// The log brought up to now, and what it holds for a device at `since`: `made` makes that of the index it can see, and what the
/// log names, into an answer. Blocking.
fn from_log(serve: &Serve, turn: &mut Settled, asked: &Asked, made: impl FnOnce(Index, &BTreeMap<String, Vec<i64>>) -> Result<Feed>) -> Result<Feed> {
    if asked.since == asked.read_at && turn.holds(asked.facts, serve.built_at(), asked.read_at, asked.libraries) {
        return Ok(Feed::Same);
    }
    let index = settle(serve, turn, asked.facts, asked.read_at, asked.libraries)?;
    let (rev, cat) = (index.rev(), serve.catalogue());
    if asked.since < cat.index_floor()? || asked.since > rev {
        return Ok(Feed::Whole { rev });
    }
    if asked.since == rev {
        return Ok(Feed::Same);
    }
    let logged = cat.index_logged(asked.since, rev)?;
    made(seen_by(serve, asked.facts, index, asked.libraries, asked.account)?, &logged)
}

/// `feed`, on this reading of the facts. Blocking.
fn feed_on(serve: &Serve, turn: &mut Settled, asked: &Asked) -> Result<Feed> {
    from_log(serve, turn, asked, |index, logged| {
        let (rows, gone) = batch(&index, logged);
        if more_than_rows_are_worth(rows.tracks.len() + gone.tracks.len(), index.tracks()) {
            return Ok(Feed::Whole { rev: index.rev() });
        }
        Ok(Feed::Changes(Bytes::of(index::Changes::new(asked.since, index.rev(), &rows, gone).save(), asked.takes_gzip)?))
    })
}

/// `sounds_feed`, on this reading of the facts. Blocking.
fn sounds_feed_on(serve: &Serve, turn: &mut Settled, asked: &Asked) -> Result<Feed> {
    from_log(serve, turn, asked, |index, logged| {
        let (sounds, moved) = (sounds_of(asked.facts, &index), logged.get("sound").map_or(&[][..], Vec::as_slice));
        if more_than_rows_are_worth(moved.len(), sounds.len()) {
            return Ok(Feed::Whole { rev: index.rev() });
        }
        Ok(Feed::Changes(Bytes::of(sounds.changes(asked.since, moved).save(), asked.takes_gzip)?))
    })
}

/// `on`, as `account` asks it at `since` with the log and the facts as they stand. Blocking.
fn asking(serve: &Serve, libraries: &[index::Library], account: Option<&str>, since: i64, takes_gzip: bool, on: fn(&Serve, &mut Settled, &Asked) -> Result<Feed>) -> Result<Feed> {
    let mut turn = serve.device_turn();
    let cat = serve.catalogue();
    let read_at = cat.index_rev()?;
    let facts = cat.crate_facts()?;
    on(serve, &mut turn, &Asked { facts: &facts, read_at, libraries, account, since, takes_gzip })
}

/// What `account` lacks at `since`. Blocking.
fn feed(serve: &Serve, libraries: &[index::Library], account: Option<&str>, since: i64, takes_gzip: bool) -> Result<Feed> {
    asking(serve, libraries, account, since, takes_gzip, feed_on)
}

/// What of the sounds `account` lacks at `since`. Blocking.
fn sounds_feed(serve: &Serve, libraries: &[index::Library], account: Option<&str>, since: i64, takes_gzip: bool) -> Result<Feed> {
    asking(serve, libraries, account, since, takes_gzip, sounds_feed_on)
}

/// Where the log stands for a device at `since`, nothing built: whether it has anything to ask for. Blocking.
fn stands_for(serve: &Serve, libraries: &[index::Library], since: Option<i64>) -> Result<IndexStands> {
    let turn = serve.device_turn();
    let cat = serve.catalogue();
    let (rev, floor) = (cat.index_rev()?, cat.index_floor()?);
    let Some(since) = since.filter(|s| *s >= floor && *s <= rev) else { return Ok(IndexStands { rev, changed: true, reset: Some(true) }) };
    let facts = cat.crate_facts()?;
    Ok(IndexStands { rev, changed: cat.index_moved(since, rev)? || !turn.stands(&facts, serve.built_at(), libraries), reset: None })
}

/// Bytes as gzip: a device fetches these over whatever connection it has.
fn gzipped(bytes: &[u8]) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut out = flate2::write::GzEncoder::new(Vec::with_capacity(bytes.len() / 2), flate2::Compression::default());
    out.write_all(bytes)?;
    Ok(out.finish()?)
}

/// What a device is sent: bytes as the rules crate wrote them, and whether they are gzip's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bytes {
    pub bytes: Vec<u8>,
    pub gzip: bool,
}

impl Bytes {
    /// `bytes`, packed where the asker `takes_gzip`.
    fn of(bytes: Vec<u8>, takes_gzip: bool) -> Result<Self> {
        Ok(if takes_gzip { Self { bytes: gzipped(&bytes)?, gzip: true } } else { Self { bytes, gzip: false } })
    }
}

impl IntoResponse for Bytes {
    fn into_response(self) -> Response {
        let mut sent = ([(header::CONTENT_TYPE, "application/octet-stream"), (header::CACHE_CONTROL, "no-store"), (header::VARY, "accept-encoding")], self.bytes).into_response();
        if self.gzip {
            sent.headers_mut().insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        }
        sent
    }
}

/// Whether a request takes gzip: a browser's always does, and it hands the page plain bytes.
pub fn takes_gzip(headers: &HeaderMap) -> bool {
    let named = |item: &str| {
        let mut parts = item.split(';').map(str::trim);
        parts.next().is_some_and(|name| name.eq_ignore_ascii_case("gzip")) && !parts.any(|p| p.replace(' ', "").strip_prefix("q=").and_then(|q| q.parse::<f32>().ok()) == Some(0.0))
    };
    headers.get(header::ACCEPT_ENCODING).and_then(|v| v.to_str().ok()).is_some_and(|v| v.split(',').any(named))
}

/// `GET /api/index`: the whole index `account` can see, every library it can see in it, as `Index::save` writes it.
pub async fn read(app: &Arc<App>, serve: Arc<Serve>, account: Option<String>, takes_gzip: bool) -> Result<Bytes, ToggleError> {
    let libraries = app.library.index_libraries(&serve);
    let bytes = tokio::task::spawn_blocking(move || Bytes::of(whole(&serve, libraries, account.as_deref())?.save(), takes_gzip));
    Ok(bytes.await.map_err(|e| anyhow!("{e}"))??)
}

/// `GET /api/index/changes?since=<rev>`: what `account` lacks at `since`.
pub async fn changes(app: &Arc<App>, serve: Arc<Serve>, account: Option<String>, since: i64, takes_gzip: bool) -> Result<Feed, ToggleError> {
    let libraries = app.library.index_libraries(&serve);
    let found = tokio::task::spawn_blocking(move || feed(&serve, &libraries, account.as_deref(), since, takes_gzip));
    Ok(found.await.map_err(|e| anyhow!("{e}"))??)
}

/// `GET /api/index/sounds`: the sounds of every track `account` can see, as `Sounds::save` writes them.
pub async fn sounds(app: &Arc<App>, serve: Arc<Serve>, account: Option<String>, takes_gzip: bool) -> Result<Bytes, ToggleError> {
    let libraries = app.library.index_libraries(&serve);
    let bytes = tokio::task::spawn_blocking(move || {
        let (index, facts) = whole_of(&serve, &libraries, account.as_deref())?;
        Bytes::of(sounds_of(&facts, &index).save(), takes_gzip)
    });
    Ok(bytes.await.map_err(|e| anyhow!("{e}"))??)
}

/// `GET /api/index/sounds/changes?since=<rev>`: what of the sounds `account` lacks at `since`.
pub async fn sound_changes(app: &Arc<App>, serve: Arc<Serve>, account: Option<String>, since: i64, takes_gzip: bool) -> Result<Feed, ToggleError> {
    let libraries = app.library.index_libraries(&serve);
    let found = tokio::task::spawn_blocking(move || sounds_feed(&serve, &libraries, account.as_deref(), since, takes_gzip));
    Ok(found.await.map_err(|e| anyhow!("{e}"))??)
}

/// One account's fits and counts as a device holds them: a row for every track with anything to say.
fn marks_of(fits: &HashMap<String, f64>, skipped: &HashMap<String, f64>, uses: &HashMap<i64, crate::crates::Use>) -> Vec<index::Mark> {
    fn row(rows: &mut HashMap<i64, index::Mark>, track: i64) -> &mut index::Mark {
        rows.entry(track).or_insert_with(|| index::Mark { track, ..index::Mark::default() })
    }
    let mut rows: HashMap<i64, index::Mark> = HashMap::new();
    for (song, fit) in fits {
        if let Some(track) = ids::track_of(song) {
            row(&mut rows, track).fit = Some(*fit);
        }
    }
    for (song, like) in skipped {
        if let Some(track) = ids::track_of(song) {
            row(&mut rows, track).skipped = Some(*like);
        }
    }
    for (track, u) in uses {
        let m = row(&mut rows, *track);
        (m.plays, m.last, m.rating, m.skips) = (u.plays, u.last, u.rating, u.skips);
    }
    rows.into_values().collect()
}

/// `GET /api/index/own`: what the server last worked out for the account asking, and no one else: its fits and its counts, as
/// `Marks::save` writes them.
pub async fn own(app: &Arc<App>, account: Option<String>, takes_gzip: bool) -> Result<Bytes, ToggleError> {
    let user = account.ok_or_else(|| ToggleError::Refused("No one is signed in.".into()))?;
    let (fits, skipped, uses) = (app.playlists.taste_fits(&user, &[]).await?, app.playlists.skipped_fits(&user, &[]).await?, app.playlists.uses_by_track(&user).await?);
    let rest = serde_json::to_string(&app.playlists.device_own(&user).await?).map_err(|e| anyhow!("{e}"))?;
    let bytes = tokio::task::spawn_blocking(move || Bytes::of(index::Marks::build(super::moment::now_secs(), &marks_of(&fits, &skipped, &uses)).with_own(rest).save(), takes_gzip));
    Ok(bytes.await.map_err(|e| anyhow!("{e}"))??)
}

/// `/api/changes`' word on the index, for a device at `since`.
pub async fn stands(app: &Arc<App>, serve: Arc<Serve>, since: Option<i64>) -> Result<IndexStands, ToggleError> {
    let libraries = app.library.index_libraries(&serve);
    let found = tokio::task::spawn_blocking(move || stands_for(&serve, &libraries, since));
    Ok(found.await.map_err(|e| anyhow!("{e}"))??)
}

#[cfg(test)]
pub(super) mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::catalogue::{Catalogue, Rules};
    use crate::serve::copies::Kind as CopyKind;

    /// A server over folders on the disk, for a test named `name`: what `add` puts there is read, identified and built.
    pub(in crate::app) struct Served {
        pub roots: Vec<PathBuf>,
        pub cat: Arc<Catalogue>,
        pub serve: Arc<Serve>,
    }

    impl Drop for Served {
        fn drop(&mut self) {
            for root in &self.roots {
                std::fs::remove_dir_all(root).ok();
            }
        }
    }

    pub(in crate::app) fn served(name: &str, kinds: &[CopyKind]) -> Served {
        served_in(name, kinds, None)
    }

    /// `served`, its data folder `data`, as covers need one.
    pub(in crate::app) fn served_in(name: &str, kinds: &[CopyKind], data: Option<&Path>) -> Served {
        let roots: Vec<PathBuf> = (0..kinds.len()).map(|n| std::env::temp_dir().join(format!("cratarr-device-{name}-{n}-{}", std::process::id()))).collect();
        for root in &roots {
            std::fs::remove_dir_all(root).ok();
            std::fs::create_dir_all(root).unwrap();
        }
        let cat = Arc::new(Catalogue::open(None, roots.clone()).unwrap());
        let serve = Serve::open(data, Rules::default(), &|_| None).unwrap();
        serve.library.set_kinds(kinds.iter().enumerate().map(|(n, k)| (n as i64, *k)).collect());
        serve.attach_catalogue(cat.clone());
        Served { roots, cat, serve }
    }

    impl Served {
        /// These files in folder `folder`: (path, tags as a read would leave them).
        pub fn add(&self, folder: i64, files: &[(&str, &[(&str, &str)])]) {
            for (rel, _) in files {
                let at = self.roots[folder as usize].join(rel);
                std::fs::create_dir_all(at.parent().unwrap()).unwrap();
                std::fs::write(at, rel).unwrap();
            }
            self.cat.walk_all();
            self.cat.pretend_read_for_tests();
            for (rel, tags) in files {
                self.cat.tag_for_tests(folder, rel, tags);
            }
            self.cat.identify().unwrap();
        }

        /// The library built from what the catalogue holds now, the facts kept as read.
        pub fn built_and_not_read(&self) {
            let (_, print) = self.serve.library.change(&self.cat, &Rules::default()).unwrap();
            self.serve.library.build_from(&self.cat, &Rules::default(), &print).unwrap();
            self.serve.build_passed_for_tests();
        }

        /// The library built from what the catalogue holds now, and the facts read again at the next asking.
        pub fn built(&self) {
            self.built_and_not_read();
            self.cat.facts_forgotten_for_tests();
        }

        /// The libraries, one to a folder, as `Library::index_libraries` would give them.
        pub fn libraries(&self) -> Vec<index::Library> {
            (0..self.roots.len() as i64).map(|id| index::Library { id, key: format!("library-{id}"), name: format!("Library {id}"), order: id as u32, ..index::Library::default() }).collect()
        }

        pub fn index(&self) -> Index {
            whole(&self.serve, self.libraries(), Some("ann")).unwrap()
        }
    }

    /// A number of /proc/self/status, in MB: what the system says this process holds.
    fn status_mb(field: &str) -> f64 {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let line = status.lines().find(|l| l.starts_with(field)).unwrap_or("");
        line.split_whitespace().nth(1).and_then(|kb| kb.parse::<f64>().ok()).unwrap_or(0.0) / 1024.0
    }

    /// `rows` in the served library's tables, as a build leaves them, for what reads them.
    fn filled(s: &Served, rows: &index::Rows) {
        let db = s.serve.library.db();
        let tx = db.unchecked_transaction().unwrap();
        for a in &rows.artists {
            tx.execute("INSERT INTO artists (id, name, album_count, song_count, order_name) VALUES (?1, ?2, 1, 1, ?3)", rusqlite::params![ids::artist(a.id), a.name, a.sort]).unwrap();
        }
        for r in &rows.records {
            let id = ids::record(r.id);
            tx.execute(
                "INSERT INTO albums (id, name, artist, artist_id, year, original_year, genres, song_count, duration_ms, size, discs, created, compilation, order_name, record, release_type, secondary, whole_tracks)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 1, 1, 1, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                rusqlite::params![id, r.title, r.artist, ids::artist(r.artist_ids[0]), r.year, r.original_year, r.genres.join("\u{1f}"), r.added, r.compilation, r.sort, r.id, r.kind, r.secondary.join("\u{1f}"), r.whole_tracks],
            )
            .unwrap();
            for (n, a) in r.artist_ids.iter().enumerate() {
                tx.execute("INSERT INTO album_artists (album_id, artist_id, name, position) VALUES (?1, ?2, '', ?3)", rusqlite::params![id, ids::artist(*a), n as i64]).unwrap();
            }
            for c in &r.copies {
                tx.execute(
                    "INSERT INTO copies (album, folder, dir, format, min_kbps, size, added, tracks, bits, rate) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![id, c.library, c.dir, c.format, c.min_kbps, c.size.map(|x| x as i64), c.added, c.tracks, c.bits, c.rate],
                )
                .unwrap();
            }
        }
        for t in &rows.tracks {
            let id = ids::song(t.id);
            tx.execute(
                "INSERT INTO songs (id, pid, folder, rel, path, album_id, title, artist, album, track_no, disc_no, size, order_title) VALUES (?1, ?1, 0, ?2, ?2, ?3, ?4, ?5, '', ?6, ?7, 1, ?4)",
                rusqlite::params![id, t.facts.rel, ids::record(t.record), t.title, t.artist, t.number, t.disc],
            )
            .unwrap();
            // The album artist comes from the record, as the library has it: only the song's own here.
            for (n, a) in t.artist_ids.iter().take(1).enumerate() {
                tx.execute("INSERT INTO song_artists (song_id, artist_id, name, position) VALUES (?1, ?2, '', ?3)", rusqlite::params![id, ids::artist(*a), n as i64]).unwrap();
            }
        }
        tx.commit().unwrap();
    }

    /// What a whole build costs at the real library's size, in the server's own code, with the crates' facts held as they are on a
    /// server. The bytes and the memory are what they are; the times are an unoptimised test build's, and this test's allocator
    /// is not the image's musl, so read them for how much there is to do. The size on the wire is made-up names', which repeat
    /// less than real ones.
    #[test]
    #[ignore = "a measure: rt-side.sh test measured_a_build --ignored --nocapture --test-threads=1"]
    fn measured_a_build_at_34_000_tracks() {
        let s = served("measured", &[CopyKind::Main, CopyKind::Own, CopyKind::Incoming]);
        let (rows, heard) = index::made_up(34_000, 1);
        filled(&s, &rows);
        let libraries = rows.libraries.clone();
        let sounds: HashMap<i64, Arc<[f32]>> = heard.iter().filter_map(|(t, v)| Some((*t, Arc::from(cratarr_rules::similarity::mean_sound(&[v.as_slice()])?)))).collect();
        drop(heard);
        let facts: Vec<Facts> = rows.tracks.iter().map(|t| Facts { sound: sounds.get(&t.id).cloned(), ..t.facts.clone() }).collect();
        let tracks = rows.tracks.len();
        drop(rows);
        let (before, hwm_before) = (status_mb("VmRSS"), status_mb("VmHWM"));
        let started = std::time::Instant::now();
        let index = build(&s.serve, &facts, &libraries, 1).unwrap();
        let built = started.elapsed();
        let saved_at = std::time::Instant::now();
        let bytes = index.save();
        let saved = saved_at.elapsed();
        let zipped_at = std::time::Instant::now();
        let packed = Bytes::of(bytes.clone(), true).unwrap();
        let zipped = zipped_at.elapsed();
        let (held, top) = (status_mb("VmRSS"), status_mb("VmHWM"));
        let sounds_at = std::time::Instant::now();
        let sound_bytes = sounds_of(&facts, &index).save();
        let sounds_took = sounds_at.elapsed();
        let zipped_at = std::time::Instant::now();
        let sounds_packed = Bytes::of(sound_bytes.clone(), true).unwrap();
        let sounds_zipped = zipped_at.elapsed();
        println!("sounds: made and saved {sounds_took:?}, gzip {sounds_zipped:?}; bytes {}, gzip {} ({:.0}%); resident top now {:.1}", sound_bytes.len(), sounds_packed.bytes.len(), sounds_packed.bytes.len() as f64 * 100.0 / sound_bytes.len() as f64, status_mb("VmHWM"));
        // Every track with a fit and one heard skip's likeness, and every fifth played: an account that listens a lot.
        let marks: Vec<index::Mark> = facts.iter().enumerate().filter_map(|(n, f)| Some(index::Mark { track: f.track?, fit: Some(n as f64 / 34_567.0), skipped: Some(n as f64 / 76_543.0), plays: if n % 5 == 0 { n as u32 % 40 } else { 0 }, last: (n % 5 == 0).then_some(1_700_000_000 + n as i64 * 977), rating: None, skips: 0 })).collect();
        let own = index::Marks::build(1, &marks).save();
        println!("an account's marks: bytes {}, gzip {}", own.len(), Bytes::of(own.clone(), true).unwrap().bytes.len());
        println!("{tracks} tracks: build {built:?}, save {saved:?}, gzip {zipped:?}");
        println!("bytes {}, gzip {} ({:.0}%)", bytes.len(), packed.bytes.len(), packed.bytes.len() as f64 * 100.0 / bytes.len() as f64);
        println!("resident {before:.1} MB before, {held:.1} with the index and its bytes held, top {top:.1} (top before {hwm_before:.1}): a build costs {:.1} MB at its top", top - before);
    }

    fn album(title: &str, artist: &str, track: &str) -> Vec<(&'static str, String)> {
        vec![("album", title.to_string()), ("albumartist", artist.to_string()), ("artist", artist.to_string()), ("title", track.to_string())]
    }

    /// One album in a folder.
    fn with_album(s: &Served, folder: i64, dir: &str, title: &str, artist: &str, tracks: &[&str]) {
        let tags: Vec<(String, Vec<(&'static str, String)>)> = tracks.iter().enumerate().map(|(n, t)| (format!("{dir}/{n:02} {t}.flac"), album(title, artist, t))).collect();
        let tags: Vec<(&str, Vec<(&str, &str)>)> = tags.iter().map(|(rel, t)| (rel.as_str(), t.iter().map(|(k, v)| (*k, v.as_str())).collect())).collect();
        let files: Vec<(&str, &[(&str, &str)])> = tags.iter().map(|(rel, t)| (*rel, t.as_slice())).collect();
        s.add(folder, &files);
    }

    #[test]
    fn a_tracks_row_is_what_the_library_serves_and_the_crates_read() {
        let s = served("row", &[CopyKind::Main]);
        with_album(&s, 0, "A/B", "Record", "Someone", &["First", "Second"]);
        s.built();
        let index = s.index();
        let rows = index.rows();
        assert_eq!((rows.tracks.len(), rows.records.len(), rows.libraries.len()), (2, 1, 1));
        let first = rows.tracks.iter().find(|t| t.title == "First").unwrap();
        assert_eq!(first.artist.as_deref(), Some("Someone"));
        assert_eq!(first.facts.rel, "A/B/00 First.flac");
        assert_eq!(first.facts.folders, [0]);
        assert_eq!(first.facts.track, Some(first.id));
        let record = index.record(first.record).unwrap();
        assert_eq!((record.title.as_str(), record.artist.as_str()), ("Record", "Someone"));
        assert_eq!(record.copies.iter().map(|c| (c.library, c.dir.as_str(), c.tracks)).collect::<Vec<_>>(), [(0, "A/B", 2)]);
        assert_eq!(first.artist_ids.len(), 2, "the track's artist, then the album artist");
        let artist = index.artist(first.artist_ids[0]).unwrap();
        assert_eq!((artist.name.as_str(), record.artist_ids.as_slice()), ("Someone", &first.artist_ids[..1]));
    }

    /// The feed applied, to where it says nothing more. Refused, or told to start again: the test's failure.
    fn fed(s: &Served, device: &mut Index) {
        match feed(&s.serve, &s.libraries(), Some("ann"), device.rev(), false).unwrap() {
            Feed::Changes(sent) => device.apply(&index::Changes::load(&sent.bytes).unwrap()).unwrap(),
            Feed::Same => {}
            Feed::Whole { rev } => panic!("told to start again at {rev}, from {}", device.rev()),
        }
    }

    /// The owner's proof for D10: what a device comes to by the feed is what a fresh fetch says, to the byte.
    fn in_step(s: &Served, device: &mut Index) {
        fed(s, device);
        assert!(device.save() == s.index().save(), "{:?}\nfresh {:?}", device.rows(), s.index().rows());
    }

    /// A device that holds the library as `s` has it.
    fn device_of(s: &Served) -> Index {
        Index::load(&s.index().save()).unwrap()
    }

    fn title_of(index: &Index, track: i64) -> String {
        index.track(track).unwrap().title
    }

    #[test]
    fn a_retag_comes_by_the_feed() {
        let s = served("retag", &[CopyKind::Main]);
        with_album(&s, 0, "A", "Record", "Someone", &["First", "Second"]);
        s.built();
        let mut device = device_of(&s);
        s.cat.tag_for_tests(0, "A/00 First.flac", &album("Record", "Someone", "Renamed").iter().map(|(k, v)| (*k, v.as_str())).collect::<Vec<_>>());
        s.built();
        in_step(&s, &mut device);
        let track = s.cat.track_of(0, "A/00 First.flac").unwrap();
        assert_eq!(title_of(&device, track), "Renamed");
    }

    /// The write is logged before the library is built from it, and a device asking in between is sent the row as it stands then:
    /// the build moves it again with no write, and only the row's print can say so.
    #[test]
    fn a_device_asking_between_a_write_and_its_build_is_told_again_after_the_build() {
        let s = served("between", &[CopyKind::Main]);
        with_album(&s, 0, "A", "Record", "Someone", &["First", "Second"]);
        s.built();
        let mut device = device_of(&s);
        s.cat.tag_for_tests(0, "A/00 First.flac", &album("Record", "Someone", "Renamed").iter().map(|(k, v)| (*k, v.as_str())).collect::<Vec<_>>());
        in_step(&s, &mut device);
        let track = s.cat.track_of(0, "A/00 First.flac").unwrap();
        assert_eq!(title_of(&device, track), "First", "not built yet");
        s.built();
        in_step(&s, &mut device);
        assert_eq!(title_of(&device, track), "Renamed");
    }

    #[test]
    fn a_keep_comes_by_the_feed() {
        let s = served("keep", &[CopyKind::Main, CopyKind::Incoming]);
        with_album(&s, 0, "A", "Stays", "Someone", &["One"]);
        with_album(&s, 1, "B", "Comes", "Someone", &["Two"]);
        s.built();
        let mut device = device_of(&s);
        let track = s.cat.track_of(1, "B/00 Two.flac").unwrap();
        assert_eq!(device.track(track).unwrap().facts.folders, [1]);
        std::fs::create_dir_all(s.roots[0].join("B")).unwrap();
        std::fs::rename(s.roots[1].join("B/00 Two.flac"), s.roots[0].join("B/00 Two.flac")).unwrap();
        s.cat.move_files(1, "B", 0, "B").unwrap();
        s.built();
        in_step(&s, &mut device);
        assert_eq!(device.track(track).unwrap().facts.folders, [0], "the same track, in the library it was added to");
        assert_eq!(device.record(device.track(track).unwrap().record).unwrap().copies[0].library, 0);
    }

    #[test]
    fn a_delete_takes_its_track_its_record_and_who_only_it_named() {
        let s = served("delete", &[CopyKind::Main]);
        with_album(&s, 0, "A", "Stays", "Someone", &["One", "Three", "Four"]);
        with_album(&s, 0, "B", "Goes", "Another", &["Two"]);
        s.built();
        let mut device = device_of(&s);
        let track = s.cat.track_of(0, "B/00 Two.flac").unwrap();
        let (record, artist) = (device.track(track).unwrap().record, device.track(track).unwrap().artist_ids[0]);
        s.cat.mark_trashed(0, "B/00 Two.flac", "token").unwrap();
        s.built();
        in_step(&s, &mut device);
        assert!(device.track(track).is_none() && device.record(record).is_none() && device.artist(artist).is_none());
    }

    /// The other copy stays: its record is what changed, and no row of the track's own says so but its folders.
    #[test]
    fn one_of_two_copies_going_changes_its_track_and_its_record() {
        let s = served("copy", &[CopyKind::Main, CopyKind::Incoming]);
        with_album(&s, 0, "A", "Both", "Someone", &["One"]);
        with_album(&s, 1, "A", "Both", "Someone", &["One"]);
        with_album(&s, 0, "B", "Beside", "Someone", &["Two", "Three", "Four"]);
        s.built();
        let mut device = device_of(&s);
        let track = s.cat.track_of(0, "A/00 One.flac").unwrap();
        let record = device.track(track).unwrap().record;
        assert_eq!(device.record(record).unwrap().copies.len(), 2);
        s.cat.mark_trashed(1, "A/00 One.flac", "token").unwrap();
        s.built();
        in_step(&s, &mut device);
        let row = device.track(track).unwrap();
        assert_eq!((device.record(record).unwrap().copies.len(), row.facts.folders.as_slice(), row.facts.lines.len()), (1, &[0][..], 0));
    }

    /// What the library's build makes of what is no file: a year set by hand, a type, a whole release's count. Nothing is written
    /// to the catalogue, and no trigger can see it.
    #[test]
    fn a_records_row_moved_by_the_library_alone_comes_by_the_feed() {
        let s = served("library", &[CopyKind::Main]);
        with_album(&s, 0, "A", "Record", "Someone", &["One"]);
        s.built();
        let mut device = device_of(&s);
        let head = s.cat.index_rev().unwrap();
        s.serve.library.db().execute("UPDATE albums SET year = 1999", []).unwrap();
        s.serve.build_passed_for_tests();
        assert_eq!(s.cat.index_rev().unwrap(), head, "no write to anything a trigger sees");
        in_step(&s, &mut device);
        assert_eq!(device.rows().records[0].year, Some(1999));
    }

    /// `tags`, `labels`, `release_types`, `moods`: read from elsewhere, or worked out over the whole library, so a track's row moves
    /// with no write to it. A reading is what says so.
    #[test]
    fn what_a_new_reading_says_of_a_row_comes_by_the_feed_with_no_write_to_it() {
        let s = served("reading", &[CopyKind::Main]);
        with_album(&s, 0, "A", "Record", "Someone", &["One", "Two"]);
        s.built();
        let facts = s.cat.crate_facts().unwrap();
        let mut device = device_of(&s);
        let libraries = s.libraries();
        let changed: Vec<(&str, fn(&mut Facts))> = vec![
            ("tags", |f| f.tags = vec!["night".into()]),
            ("labels", |f| f.labels = vec!["Warp".into()]),
            ("types", |f| f.release_types = vec!["EP".into()]),
            ("moods", |f| f.moods = vec!["sad".into()]),
        ];
        let at = s.cat.index_rev().unwrap();
        for (what, change) in changed {
            let mut read: Vec<Facts> = facts.to_vec();
            read.iter_mut().filter(|f| f.rel.contains("One")).for_each(change);
            let read = Arc::new(read);
            let mut turn = s.serve.device_turn();
            let head = s.cat.index_rev().unwrap();
            let asked = Asked { facts: &read, read_at: head, libraries: &libraries, account: Some("ann"), since: device.rev(), takes_gzip: false };
            let Feed::Changes(sent) = feed_on(&s.serve, &mut turn, &asked).unwrap() else { panic!("{what}: no changes") };
            device.apply(&index::Changes::load(&sent.bytes).unwrap()).unwrap();
            let fresh = build(&s.serve, &read, &libraries, device.rev()).unwrap();
            assert!(device.save() == fresh.save(), "{what}");
        }
        assert!(device.rev() > at);
    }

    #[test]
    fn a_tracks_mark_is_its_fit_what_it_is_like_of_the_skipped_and_what_was_done_with_it() {
        use crate::crates::Use;
        let fits = HashMap::from([(ids::song(1), 0.75), (ids::song(2), 0.5), ("al-3".to_string(), 0.1)]);
        let skipped = HashMap::from([(ids::song(2), 0.25)]);
        let uses = HashMap::from([(1, Use { plays: 3, last: Some(100), rating: Some(4), skips: 2 }), (5, Use { plays: 1, ..Use::default() })]);
        let marks = index::Marks::build(1, &marks_of(&fits, &skipped, &uses));
        assert_eq!(marks.of(1).unwrap(), index::Mark { track: 1, fit: Some(0.75), skipped: None, plays: 3, last: Some(100), rating: Some(4), skips: 2 });
        assert_eq!(marks.of(2).unwrap(), index::Mark { track: 2, fit: Some(0.5), skipped: Some(0.25), ..index::Mark::default() });
        assert_eq!(marks.of(5).unwrap().plays, 1, "played, and no fit known");
        assert_eq!(marks.rows().len(), 3, "and no row of what is no track");
    }

    /// A reading of `s`'s facts with these tracks heard, for the sounds' feed.
    fn with_sounds(s: &Served, heard: &[(i64, &[f32])]) -> Arc<Vec<Facts>> {
        let mut read: Vec<Facts> = s.cat.crate_facts().unwrap().to_vec();
        for f in &mut read {
            f.sound = heard.iter().find(|h| Some(h.0) == f.track).map(|h| Arc::from(h.1));
        }
        Arc::new(read)
    }

    fn sounds_at(s: &Served, read: &Arc<Vec<Facts>>, since: i64) -> Feed {
        let (libraries, mut turn) = (s.libraries(), s.serve.device_turn());
        let read_at = s.cat.index_rev().unwrap();
        sounds_feed_on(&s.serve, &mut turn, &Asked { facts: read, read_at, libraries: &libraries, account: Some("ann"), since, takes_gzip: false }).unwrap()
    }

    /// The sounds' own proof: what comes by their feed is what a fresh fetch says, to the byte.
    #[test]
    fn a_sound_comes_by_its_own_feed_as_a_fresh_fetch_says_it() {
        let s = served("sounds", &[CopyKind::Main]);
        let names = ["One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine"];
        with_album(&s, 0, "A", "Record", "Someone", &names);
        s.built();
        let tracks: Vec<i64> = names.iter().enumerate().map(|(n, t)| s.cat.track_of(0, &format!("A/{n:02} {t}.flac")).unwrap()).collect();
        let (near, far) = (vec![1.0f32, 0.0, 0.0], vec![0.0f32, 1.0, 0.0]);
        let heard = |first: &[f32], last: usize| -> Vec<(i64, Vec<f32>)> { (0..8).map(|n| (tracks[if n == 7 { last } else { n }], if n == 0 { first.to_vec() } else { near.clone() })).collect() };
        let reading = |rows: &[(i64, Vec<f32>)]| with_sounds(&s, &rows.iter().map(|(t, v)| (*t, v.as_slice())).collect::<Vec<_>>());
        let read = reading(&heard(&near, 7));
        // The first prints, as a device's first fetch makes them.
        let libraries = s.libraries();
        let index = {
            let mut turn = s.serve.device_turn();
            let read_at = s.cat.index_rev().unwrap();
            settle(&s.serve, &mut turn, &read, read_at, &libraries).unwrap()
        };
        let mut held = Sounds::load(&sounds_of(&read, &index).save()).unwrap();
        assert_eq!((held.len(), held.rev()), (8, index.rev()));
        assert_eq!(sounds_at(&s, &read, held.rev()), Feed::Same);

        let read = reading(&heard(&far, 8));
        let Feed::Changes(sent) = sounds_at(&s, &read, held.rev()) else { panic!("no changes") };
        let changes = index::SoundChanges::load(&sent.bytes).unwrap();
        assert_eq!(changes.counts(), (2, 1), "one changed, one new, one gone");
        held.apply(&changes).unwrap();
        let fresh = build(&s.serve, &read, &libraries, changes.to()).unwrap();
        assert!(held.save() == sounds_of(&read, &fresh).save(), "as a fresh fetch says");
        assert_eq!(sounds_at(&s, &read, held.rev()), Feed::Same);
    }

    #[test]
    fn with_nothing_moved_a_device_is_told_so_and_nothing_is_built() {
        let s = served("same", &[CopyKind::Main]);
        with_album(&s, 0, "A", "Record", "Someone", &["One"]);
        s.built();
        let device = device_of(&s);
        assert_eq!(feed(&s.serve, &s.libraries(), Some("ann"), device.rev(), false).unwrap(), Feed::Same);
        assert!(s.serve.device_turn().holds(&s.cat.crate_facts().unwrap(), s.serve.built_at(), device.rev(), &s.libraries()));
        // Nothing can be built from this: a build would fail on it, and the answer shows none was tried.
        s.serve.library.db().execute("ALTER TABLE songs RENAME TO songs_away", []).unwrap();
        assert_eq!(feed(&s.serve, &s.libraries(), Some("ann"), device.rev(), false).unwrap(), Feed::Same);
        s.serve.library.db().execute("ALTER TABLE songs_away RENAME TO songs", []).unwrap();
    }

    /// A library's covers take hours on a NAS, and each is logged: a device told "changed" at each would ask for a whole build.
    #[test]
    fn a_cover_made_moves_the_log_and_no_row() {
        let s = served("cover", &[CopyKind::Main]);
        with_album(&s, 0, "A", "Record", "Someone", &["One"]);
        s.built();
        let rev = s.index().rev();
        s.cat.index_note("cover", 1).unwrap();
        let at = || stands_for(&s.serve, &s.libraries(), Some(rev)).unwrap();
        assert_eq!(at(), IndexStands { rev: rev + 1, changed: false, reset: None }, "the rev says a cover's feed has something");
        s.cat.tag_for_tests(0, "A/00 One.flac", &[("title", "Renamed")]);
        assert!(at().changed, "a row past it");
    }

    #[test]
    fn a_rev_no_longer_kept_or_ahead_of_the_log_starts_again() {
        let s = served("reset", &[CopyKind::Main]);
        with_album(&s, 0, "A", "Record", "Someone", &["One"]);
        s.built();
        let rev = s.index().rev();
        assert_eq!(feed(&s.serve, &s.libraries(), None, rev + 1, false).unwrap(), Feed::Whole { rev }, "ahead of the log");
        assert_eq!(feed(&s.serve, &s.libraries(), None, rev - 1, false).unwrap(), Feed::Whole { rev }, "from before what was first told");
    }

    /// Every line moves, so every row is sent: the whole costs less, and says it in one.
    #[test]
    fn the_first_folder_going_moves_every_line_and_starts_again() {
        let s = served("front", &[CopyKind::Main, CopyKind::Main]);
        with_album(&s, 0, "A", "First", "Someone", &["One"]);
        with_album(&s, 1, "B", "Second", "Someone", &["Two"]);
        with_album(&s, 1, "C", "Third", "Someone", &["Three"]);
        s.built();
        let device = device_of(&s);
        assert!(device.rows().tracks.iter().any(|t| t.facts.rel.starts_with("../")), "lines climb out of the first folder");
        s.cat.set_folders(vec![s.roots[1].clone()]).unwrap();
        s.serve.library.set_kinds(vec![(1, CopyKind::Main)]);
        s.built();
        let libraries = vec![s.libraries().remove(1)];
        let Feed::Whole { rev } = feed(&s.serve, &libraries, Some("ann"), device.rev(), false).unwrap() else { panic!("rows for every line") };
        assert!(rev > device.rev());
    }

    /// A batch names what a device may not hold, or it would refuse the row: a new track's record, its artists, and the record's.
    #[test]
    fn a_new_record_brings_its_artists_and_a_device_takes_it() {
        let s = served("names", &[CopyKind::Main]);
        with_album(&s, 0, "A", "Old", "Someone", &["One"]);
        with_album(&s, 0, "B", "Older", "Someone", &["Two"]);
        with_album(&s, 0, "D", "Oldest", "Someone", &["Four"]);
        s.built();
        let mut device = device_of(&s);
        with_album(&s, 0, "C", "New", "Another", &["Three"]);
        s.built();
        in_step(&s, &mut device);
        assert!(device.rows().records.iter().any(|r| r.title == "New"));
        assert!(device.rows().artists.iter().any(|a| a.name == "Another"));
    }

    /// A compilation's track logged by itself: its record's album artist is no artist of the track, as Radio has it.
    fn one_track_logged(name: &str) -> (Served, Rows, Index) {
        let s = served(name, &[CopyKind::Main]);
        s.add(0, &[("V/01.flac", &[("album", "Hits"), ("albumartist", "Various Artists"), ("artist", "Someone"), ("title", "One")])]);
        s.built();
        let index = s.index();
        let track = index.rows().tracks[0].id;
        let (rows, gone) = batch(&index, &BTreeMap::from([("track".to_string(), vec![track])]));
        assert_eq!((rows.tracks.len(), gone.tracks.len()), (1, 0));
        (s, rows, index)
    }

    /// The brief's "each changed row's record and artists": a device may hold none of what a row names, as a row can come into its
    /// sight with no change to what it names, and it would refuse the row.
    #[test]
    fn a_batch_brings_each_tracks_record() {
        let (_s, rows, _) = one_track_logged("batch-record");
        assert_eq!(rows.records.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["Hits"]);
    }

    #[test]
    fn a_batch_brings_each_tracks_artists() {
        let (_s, rows, _) = one_track_logged("batch-artists");
        assert!(rows.artists.iter().any(|a| a.name == "Someone"), "{:?}", rows.artists);
    }

    #[test]
    fn a_batch_brings_each_records_artists_who_are_no_artists_of_its_tracks() {
        let (_s, rows, _) = one_track_logged("batch-various");
        assert!(rows.artists.iter().any(|a| a.name == "Various Artists"), "{:?}", rows.artists);
    }

    /// All that a batch brings of a row makes a device of nothing whole.
    #[test]
    fn a_device_holding_nothing_of_a_batch_takes_it() {
        let (_s, rows, index) = one_track_logged("batch-taken");
        let (mut empty, set) = (Index::build(1, &Rows { libraries: rows.libraries.clone(), ..Rows::default() }).unwrap(), rows);
        empty.apply(&index::Changes::new(1, index.rev(), &set, Gone::default())).unwrap();
        assert_eq!(empty.save(), index.save());
    }

    #[test]
    fn a_feed_for_an_account_that_sees_less_leaves_out_what_it_cannot_see() {
        let s = served("narrow", &[CopyKind::Main, CopyKind::Own]);
        with_album(&s, 0, "A", "Theirs", "Someone", &["One"]);
        with_album(&s, 1, "B", "Hidden", "Another", &["Two"]);
        s.built();
        let facts = s.cat.crate_facts().unwrap();
        let every = s.libraries();
        let index = s.index();
        let narrow = build(&s.serve, &facts, &every[..1], index.rev()).unwrap();
        let (rows, gone) = batch(&narrow, &BTreeMap::from([("track".to_string(), index.rows().tracks.iter().map(|t| t.id).collect())]));
        assert_eq!(rows.tracks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["One"]);
        assert_eq!(gone.tracks.len(), 1, "what it cannot see, gone if it held it");
    }

    #[test]
    fn a_device_hears_where_the_log_stands_and_whether_to_ask() {
        let s = served("stands", &[CopyKind::Main]);
        with_album(&s, 0, "A", "Record", "Someone", &["One"]);
        s.built();
        let rev = s.index().rev();
        let at = |since: Option<i64>| stands_for(&s.serve, &s.libraries(), since).unwrap();
        assert_eq!(at(Some(rev)), IndexStands { rev, changed: false, reset: None });
        assert_eq!(at(None), IndexStands { rev, changed: true, reset: Some(true) }, "no rev, nothing to go on from");
        assert_eq!(at(Some(rev + 1)), IndexStands { rev, changed: true, reset: Some(true) });
        assert_eq!(at(Some(rev - 1)), IndexStands { rev, changed: true, reset: Some(true) }, "from before what is kept");
        s.cat.tag_for_tests(0, "A/00 One.flac", &[("title", "Renamed")]);
        assert_eq!(at(Some(rev)), IndexStands { rev: s.cat.index_rev().unwrap(), changed: true, reset: None }, "a write");
        s.index();
        let rev = s.cat.index_rev().unwrap();
        assert!(!at(Some(rev)).changed);
        let mut renamed = s.libraries();
        renamed[0].name = "Renamed".into();
        assert!(stands_for(&s.serve, &renamed, Some(rev)).unwrap().changed, "what has no rev of its own moves it too");
    }

    #[test]
    fn the_rev_is_read_before_the_rows_so_what_is_written_meanwhile_comes_again() {
        let s = served("rev", &[CopyKind::Main]);
        with_album(&s, 0, "A", "Record", "Someone", &["One", "Two", "Three"]);
        s.built();
        let mut device = device_of(&s);
        let (read_at, facts, libraries) = (s.cat.index_rev().unwrap(), s.cat.crate_facts().unwrap(), s.libraries());
        // Written after the log was read and before the rows are: they hold it or they do not.
        s.cat.tag_for_tests(0, "A/00 One.flac", &album("Record", "Someone", "Meanwhile").iter().map(|(k, v)| (*k, v.as_str())).collect::<Vec<_>>());
        let mut turn = Settled::default();
        let index = settle(&s.serve, &mut turn, &facts, read_at, &libraries).unwrap();
        drop(turn);
        assert_eq!(index.rev(), read_at, "not past what the write logged");
        s.built();
        in_step(&s, &mut device);
    }

    #[test]
    fn a_song_not_built_or_not_read_waits_for_both() {
        let s = served("waits", &[CopyKind::Main]);
        with_album(&s, 0, "A/B", "Record", "Someone", &["First"]);
        s.built();
        with_album(&s, 0, "C/D", "Other", "Someone", &["Second"]);
        assert_eq!(s.index().rows().tracks.len(), 1, "read, and no song yet");
        with_album(&s, 0, "E/F", "Third", "Someone", &["Third"]);
        s.built_and_not_read();
        assert_eq!(s.index().rows().tracks.len(), 2, "a song, and no facts read of it yet");
        s.cat.facts_forgotten_for_tests();
        assert_eq!(s.index().rows().tracks.len(), 3);
    }

    #[test]
    fn a_compilations_album_artist_is_nobodys_artist_as_radio_has_it() {
        let s = served("compilation", &[CopyKind::Main]);
        let tags = [("album", "Hits"), ("albumartist", "Various Artists"), ("artist", "Someone"), ("title", "One")];
        s.add(0, &[("Various/Hits/01.flac", &tags)]);
        s.built();
        let t = s.index().rows().tracks.remove(0);
        assert_eq!(t.artist_ids.len(), 1, "{:?}", t.artist_ids);
    }

    #[test]
    fn every_way_the_files_write_an_artist_is_kept_on_that_artist() {
        let s = served("spelled", &[CopyKind::Main]);
        let (plain, curly) = ("Sinead O'Connor", "Sinead O\u{2019}Connor");
        s.add(0, &[("A/01.flac", &[("album", "One"), ("albumartist", plain), ("artist", plain), ("title", "T")]), ("A/02.flac", &[("album", "One"), ("albumartist", plain), ("artist", curly), ("title", "U")])]);
        s.built();
        let index = s.index();
        let id = index.rows().tracks[0].artist_ids[0];
        let artist = index.artist(id).unwrap();
        assert_eq!(artist.spelled.len(), 1, "{artist:?}");
        assert_ne!(artist.spelled[0], artist.name, "the other way, not the name again");
        let names = index.artist_ids();
        assert_eq!((&names[plain], &names[curly]), (&ids::artist(id), &ids::artist(id)), "an artist seed finds them by either");
    }

    /// The index refuses what names what it does not hold, and one odd row must not cost every device the whole of it.
    #[test]
    fn a_row_naming_what_is_not_served_loses_that_name_and_no_one_their_index() {
        let s = served("odd", &[CopyKind::Main]);
        with_album(&s, 0, "A", "Record", "Someone", &["First"]);
        with_album(&s, 0, "B", "Other", "Another", &["Second"]);
        s.built();
        s.serve.library.db().execute("DELETE FROM artists WHERE name = 'Another'", []).unwrap();
        s.serve.library.db().execute("DELETE FROM albums WHERE name = 'Record'", []).unwrap();
        let rows = s.index().rows();
        assert_eq!(rows.tracks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["Second"], "a song of no record waits for one");
        assert!(rows.tracks[0].artist_ids.is_empty(), "{:?}", rows.tracks[0]);
    }

    #[test]
    fn a_records_copies_lie_in_the_order_they_would_play() {
        let s = served("order", &[CopyKind::Incoming, CopyKind::Main]);
        with_album(&s, 0, "A", "Record", "Someone", &["First"]);
        with_album(&s, 1, "A", "Record", "Someone", &["First"]);
        s.built();
        let t = s.index().rows();
        assert_eq!(t.records.len(), 1, "one record");
        assert_eq!(t.records[0].copies.iter().map(|c| c.library).collect::<Vec<_>>(), [1, 0], "Incoming last, whatever its folder's id");
        assert_eq!(t.tracks[0].facts.folders.len(), 2);
    }

    /// What an account cannot see goes from every list of a row, and a device would refuse a row naming it.
    #[test]
    fn a_library_out_of_sight_leaves_a_tracks_folders_and_lines() {
        let f = Facts { rel: "A/1.flac".into(), lines: vec!["../b/A/1.flac".into(), "../c/A/1.flac".into()], folders: vec![0, 1, 2], track: Some(1), ..Facts::default() };
        let of = |line: &str| Some(if line.contains("../b/") { 1 } else if line.contains("../c/") { 2 } else { 0 });
        let seen = |ids: &[i64]| ids.iter().copied().collect::<HashSet<i64>>();
        assert!(matches!(within(&f, &seen(&[0, 1, 2]), &of), Some(Cow::Borrowed(_))), "wholly in sight: no copy");
        let without_b = within(&f, &seen(&[0, 2]), &of).unwrap();
        assert_eq!((without_b.rel.as_str(), without_b.lines.as_slice(), without_b.folders.as_slice()), ("A/1.flac", &["../c/A/1.flac".to_string()][..], &[0, 2][..]));
        let without_own_line = within(&f, &seen(&[1, 2]), &of).unwrap();
        assert_eq!(without_own_line.rel, "../b/A/1.flac", "its line is one it can see");
        assert_eq!((without_own_line.lines.as_slice(), without_own_line.folders.as_slice()), (&["../c/A/1.flac".to_string()][..], &[1, 2][..]));
        let turned = Facts { lines: vec!["../c/A/1.flac".into(), "../b/A/1.flac".into()], ..f.clone() };
        let turned = within(&turned, &seen(&[1, 2]), &of).unwrap();
        assert_eq!((turned.rel.as_str(), turned.folders.as_slice()), ("../c/A/1.flac", &[2, 1][..]), "the folder of its line first");
        let far = within(&f, &seen(&[2]), &of).unwrap();
        assert_eq!((far.rel.as_str(), far.folders.as_slice(), far.lines.len()), ("../c/A/1.flac", &[2][..], 0));
        assert!(within(&f, &seen(&[5]), &of).is_none(), "nothing left: no row");
    }

    #[test]
    fn a_library_left_out_takes_its_copies_and_what_lies_only_there() {
        let s = served("sight", &[CopyKind::Main, CopyKind::Own]);
        with_album(&s, 0, "A", "Both", "Someone", &["First"]);
        with_album(&s, 1, "A", "Both", "Someone", &["First"]);
        with_album(&s, 1, "B", "Only Theirs", "Another", &["Second"]);
        s.built();
        let everything = s.index();
        assert_eq!(everything.rows().tracks.len(), 2);
        let facts = s.cat.crate_facts().unwrap();
        let one = vec![s.libraries().remove(0)];
        let rows = build(&s.serve, &facts, &one, 1).unwrap().rows();
        assert_eq!(rows.libraries.iter().map(|l| l.id).collect::<Vec<_>>(), [0]);
        assert_eq!(rows.tracks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["First"]);
        assert_eq!(rows.tracks[0].facts.folders, [0]);
        assert!(rows.tracks[0].facts.lines.is_empty(), "{:?}", rows.tracks[0].facts);
        assert_eq!(rows.records.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["Both"], "a record goes with its last track");
        assert_eq!(rows.records[0].copies.iter().map(|c| c.library).collect::<Vec<_>>(), [0]);
        assert_eq!(rows.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Someone"]);
    }

    /// 0084, 0101's point 6: off is the account's own switch, and it rides along so that switching on works offline.
    #[test]
    fn every_account_sees_every_library_whatever_it_has_off() {
        let library = index::Library { id: 3, owner: Some("bob".into()), kind: Kind::Own, ..index::Library::default() };
        assert!(can_see(&library, Some("ann")) && can_see(&library, Some("bob")) && can_see(&library, None));
    }

    #[test]
    fn bytes_are_gzip_only_for_who_takes_it_and_come_back_the_same() {
        let plain = vec![7u8; 10_000];
        let sent = Bytes::of(plain.clone(), true).unwrap();
        assert!(sent.gzip && sent.bytes.len() < 100, "{}", sent.bytes.len());
        let mut back = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&sent.bytes[..]), &mut back).unwrap();
        assert_eq!(back, plain);
        assert_eq!(Bytes::of(plain.clone(), false).unwrap(), Bytes { bytes: plain, gzip: false });
    }

    #[test]
    fn gzip_is_taken_where_it_is_named_and_not_refused() {
        let asking = |said: &str| takes_gzip(&HeaderMap::from_iter([(header::ACCEPT_ENCODING, HeaderValue::from_str(said).unwrap())]));
        assert!(asking("gzip") && asking("gzip, deflate, br") && asking("br, GZIP;q=0.5"));
        assert!(!asking("identity") && !asking("gzip;q=0") && !asking("deflate, gzip; q=0.0"));
        assert!(!takes_gzip(&HeaderMap::new()));
    }

    #[test]
    fn what_a_device_loads_is_what_was_built() {
        let s = served("load", &[CopyKind::Main]);
        with_album(&s, 0, "A/B", "Record", "Someone", &["First", "Second"]);
        s.built();
        let index = s.index();
        assert!(Index::load(&index.save()).unwrap() == index);
    }
}
