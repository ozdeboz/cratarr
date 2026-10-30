//! Importing what an account did on a media server into Cratarr's own user data (decision 0030, amended 2026-10-15):
//! plays and last played, favourite tracks, albums and artists, ratings, and when albums were added. The source reads
//! them (`Source::track_data`, `favorite_groups`, `albums_added`); this finds them in the library and applies them to
//! one Cratarr account.
//!
//! Safe to run again and again: what each import brought in is kept per item (`userdata::Imported`), and the next one
//! adds only what changed on the server since:
//! - plays: the server's count minus the count imported last time, when more; last played, the later one;
//! - a favourite or a rating: the server's, only when it differs from the one imported last time (otherwise the user may
//!   have changed it in Cratarr since). Nothing imported yet reads as "no favourite, no rating", so a first import
//!   brings favourites and ratings in and takes none away;
//! - added dates: the earlier one.
//!
//! Tracks are found through the catalogue's links (server id to Cratarr track), albums through one of their tracks or
//! their folder, artists by name. What is not in the library is counted and left. One import at a time, in the
//! background, one page after the other with a pause between them, so the server goes on answering everyone else.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use anyhow::Result;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Serve, ids};
use crate::app::import::{Matcher, Track};
use crate::source::{AlbumRef, Fut, Source, TrackData};
use crate::userdata::{ImportChange, ImportKey, Imported, Kind};

/// The sources in use, by their id in the settings (`app::Library::connected_sources`).
pub type Sources = Arc<dyn Fn() -> Vec<(String, Arc<dyn Source>)> + Send + Sync>;

/// Taking away more stars at once than this share (percent) of those an import brought in, and more than
/// `UNSTAR_FLOOR`, looks like the server lost track rather than a person unstarring: they are left as they are.
const UNSTAR_SHARE: usize = 10;
const UNSTAR_FLOOR: usize = 20;

/// How an import paces itself.
#[derive(Debug, Clone)]
pub struct Pace {
    /// Between two pages, so everyone else gets their turn at the server.
    pub pause: Duration,
    /// A page that fails is asked again after each of these waits (a server busy scanning), then the import stops.
    pub retries: Vec<Duration>,
}

impl Default for Pace {
    fn default() -> Self {
        Self { pause: Duration::from_secs(2), retries: vec![Duration::from_secs(15), Duration::from_secs(60)] }
    }
}

/// What to import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct What {
    #[serde(default)]
    pub plays: bool,
    #[serde(default)]
    pub favorites: bool,
    #[serde(default)]
    pub ratings: bool,
    #[serde(default)]
    pub added: bool,
}

impl Default for What {
    fn default() -> Self {
        Self { plays: true, favorites: true, ratings: true, added: true }
    }
}

/// An import as the page asks for it: from which source and account on it, into which Cratarr account. A source id
/// `subsonic:<address>` is a Subsonic server imported from without being one of Cratarr's sources (decision 0067),
/// logged in to as `user` with `password` (given for each run, never kept or shown).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub source: String,
    /// The account on the server, by its id there.
    pub user: String,
    /// The Cratarr (Subsonic) account.
    pub account: String,
    pub what: What,
    /// Count what would change, change nothing.
    #[serde(default)]
    pub preview: bool,
    #[serde(default, skip_serializing)]
    pub password: Option<String>,
}

/// The id prefix of a Subsonic server imported from without being a source.
pub const SUBSONIC: &str = "subsonic:";

/// The library's songs and albums by name, for what a source names that the catalogue does not link (decision 0067).
fn matcher(serve: &Serve) -> Option<Arc<Matcher>> {
    let dir = serve.data_dir.as_ref()?;
    match Matcher::read(&std::path::Path::new(dir).join("library.sqlite")) {
        Ok(m) => Some(Arc::new(m)),
        Err(e) => {
            tracing::warn!(target: "imports", reason = %format!("{e:#}"), "the library could not be read to match by name");
            None
        }
    }
}

/// What an import did (or, as a preview, would do).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    /// Plays added, over this many songs.
    pub plays: u64,
    pub songs: u64,
    pub starred: u64,
    pub unstarred: u64,
    pub rated: u64,
    /// Albums whose added date was set or moved earlier.
    pub added: u64,
    /// Tracks, albums and artists the library does not have.
    pub skipped: u64,
}

impl Counts {
    fn add(&mut self, o: &Counts) {
        self.plays += o.plays;
        self.songs += o.songs;
        self.starred += o.starred;
        self.unstarred += o.unstarred;
        self.rated += o.rated;
        self.added += o.added;
        self.skipped += o.skipped;
    }

    fn count(&mut self, d: &Decision) {
        self.plays += u64::from(d.add_plays);
        self.songs += u64::from(d.add_plays > 0);
        match d.star {
            Some((true, _)) => self.starred += 1,
            Some((false, _)) => self.unstarred += 1,
            None => {}
        }
        self.rated += u64::from(d.rating.is_some());
    }
}

/// Where an import is: `tracks`, `groups` (favourite albums and artists) or `added`, and how far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Step {
    pub name: &'static str,
    pub read: usize,
    pub total: Option<usize>,
}

/// The import running now, or the last one.
#[derive(Debug, Clone, Serialize)]
pub struct Job {
    #[serde(flatten)]
    pub request: Request,
    pub started: i64,
    pub finished: Option<i64>,
    pub step: Option<Step>,
    pub counts: Counts,
    pub error: Option<String>,
    /// Something left undone on purpose (see `UNSTAR_SHARE`).
    pub note: Option<String>,
}

#[derive(Default)]
pub struct Importer {
    sources: RwLock<Option<Sources>>,
    job: Mutex<Option<Job>>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Importer {
    /// The sources to import from; until this is called there are none.
    pub fn connect(&self, sources: Sources) {
        *self.sources.write().unwrap_or_else(|p| p.into_inner()) = Some(sources);
    }

    /// The sources in use that can import.
    fn sources(&self) -> Vec<(String, Arc<dyn Source>)> {
        let f = self.sources.read().unwrap_or_else(|p| p.into_inner()).clone();
        f.map(|f| f()).unwrap_or_default().into_iter().filter(|(_, s)| s.imports()).collect()
    }

    pub fn job(&self) -> Option<Job> {
        self.job.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn update(&self, f: impl FnOnce(&mut Job)) {
        if let Some(j) = self.job.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
            f(j);
        }
    }

    /// Marks an import as running; refused while another runs.
    fn begin(&self, request: Request) -> Result<Job, &'static str> {
        let mut job = self.job.lock().unwrap_or_else(|p| p.into_inner());
        if job.as_ref().is_some_and(|j| j.finished.is_none()) {
            return Err("An import is running.");
        }
        let j = Job { request, started: now(), finished: None, step: None, counts: Counts::default(), error: None, note: None };
        *job = Some(j.clone());
        Ok(j)
    }
}

/// What the server says of one item now; a part not read is `None`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Seen {
    pub server_item: String,
    /// Plays and last played.
    pub plays: Option<(u32, Option<i64>)>,
    /// Favourite, and since when if the server keeps that.
    pub star: Option<(bool, Option<i64>)>,
    pub rating: Option<u8>,
}

/// What to change for one item, and what to keep for the next import.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Decision {
    pub add_plays: u32,
    pub played: Option<i64>,
    pub star: Option<(bool, Option<i64>)>,
    pub rating: Option<u8>,
    pub now: Imported,
}

impl Decision {
    /// Nothing to write: no change, and the record would stay as it is.
    fn is_nothing(&self, last: Option<&Imported>) -> bool {
        self.add_plays == 0 && self.played.is_none() && self.star.is_none() && self.rating.is_none() && last == Some(&self.now)
    }
}

/// The rules of an import again (see the module): the server's values against what the last import brought in.
pub fn decide(last: Option<&Imported>, seen: &Seen) -> Decision {
    let before = last.cloned().unwrap_or_default();
    let mut d = Decision { now: Imported { server_item: Some(seen.server_item.clone()), ..before.clone() }, ..Decision::default() };
    if let Some((plays, played)) = seen.plays {
        // Fewer than last time (the count was reset on the server): nothing to add, and later plays count from there.
        d.add_plays = plays.saturating_sub(before.plays);
        if played.is_some() && played != before.played {
            d.played = played;
        }
        d.now.plays = plays;
        d.now.played = played;
    }
    if let Some((on, at)) = seen.star {
        if on != before.starred {
            d.star = Some((on, at));
        }
        d.now.starred = on;
    }
    if let Some(r) = seen.rating {
        if r != before.rating {
            d.rating = Some(r);
        }
        d.now.rating = r;
    }
    d
}

/// Albums by where their folders are, to find one by the folder a server names (its own path for it).
pub struct AlbumPlaces {
    /// By the folder's own name: (album id, the folder's path in parts, how many of those parts are the album's in its
    /// catalogue folder).
    by_leaf: HashMap<String, Vec<(String, Vec<String>, usize)>>,
}

fn parts(path: &str) -> Vec<String> {
    path.split(['/', '\\']).filter(|p| !p.is_empty()).map(str::to_string).collect()
}

impl AlbumPlaces {
    /// `places`: (album id, catalogue folder, path in it); `roots`: the catalogue's folders as Cratarr sees them.
    pub fn new(places: Vec<(String, i64, String)>, roots: &[PathBuf]) -> Self {
        let mut by_leaf: HashMap<String, Vec<(String, Vec<String>, usize)>> = HashMap::new();
        for (id, folder, dir) in places {
            let own = parts(&dir);
            // An album lying loose in a catalogue folder has no folder of its own to be found by.
            let (Some(leaf), Some(root)) = (own.last().cloned(), roots.get(folder as usize)) else { continue };
            let mut all = parts(&root.to_string_lossy());
            let n = own.len();
            all.extend(own);
            by_leaf.entry(leaf).or_default().push((id, all, n));
        }
        Self { by_leaf }
    }

    /// The album whose folder `path` is (as a server sees it, under its own root): the one sharing the most trailing
    /// folders with it, at least all of its own; none when two are as near. A disc folder ("CD1") is tried as its album
    /// too.
    pub fn find(&self, path: &str) -> Option<&str> {
        let p = parts(path);
        self.find_parts(&p).or_else(|| p.split_last().and_then(|(_, up)| self.find_parts(up)))
    }

    fn find_parts(&self, p: &[String]) -> Option<&str> {
        let candidates = self.by_leaf.get(p.last()?)?;
        let mut best: Option<(usize, &str)> = None;
        let mut tie = false;
        for (id, all, own) in candidates {
            let shared = all.iter().rev().zip(p.iter().rev()).take_while(|(a, b)| a == b).count();
            if shared < *own {
                continue;
            }
            match best {
                Some((b, _)) if shared < b => {}
                Some((b, _)) if shared == b => tie = true,
                _ => {
                    best = Some((shared, id.as_str()));
                    tie = false;
                }
            }
        }
        if tie { None } else { best.map(|(_, id)| id) }
    }
}

/// An album of the library from a source's naming of it: through its track, else its folder.
fn album_of(serve: &Serve, source: &str, places: &AlbumPlaces, matcher: Option<&Matcher>, a: &AlbumRef) -> Result<Option<String>> {
    if let Some(track) = a.track.as_deref().and_then(|t| serve.catalogue().track_of_source(source, t))
        && let Some((_, album, _)) = serve.library.play_targets(&ids::song(track))?
    {
        return Ok(Some(album));
    }
    if let Some(album) = a.path.as_deref().and_then(|p| places.find(p)) {
        return Ok(Some(album.to_string()));
    }
    // By its name and artist (decision 0067).
    Ok(a.name.as_deref().zip(matcher).and_then(|(name, m)| m.find_album(name, a.artist.as_deref())).map(str::to_string))
}

fn album_places(serve: &Serve) -> Result<AlbumPlaces> {
    Ok(AlbumPlaces::new(serve.library.album_places()?, &serve.catalogue().folders()))
}

/// What one run has seen so far, across its pages.
#[derive(Default)]
struct Run {
    /// Cratarr items handled (a track listed twice, played and a favourite, counts once).
    done: HashSet<String>,
    /// Server ids listed, found or not.
    listed: HashSet<String>,
    /// Server ids the library does not have.
    missed: HashSet<String>,
    /// The library by name, for tracks the catalogue does not link.
    matcher: Option<Arc<Matcher>>,
}

/// A page's tracks: found, decided, applied (unless a preview). Blocking.
fn tracks_page(serve: &Serve, req: &Request, items: Vec<TrackData>, run: &mut Run) -> Result<Counts> {
    let key = ImportKey { account: &req.account, source: &req.source, server_user: &req.user };
    let w = req.what;
    let mut found = Vec::new();
    for t in items {
        run.listed.insert(t.id.clone());
        // By the catalogue's link to the source's id, else by the names the source gives (decision 0067).
        let by_name = || {
            let (m, x) = (t.meta.as_ref()?, run.matcher.as_deref()?);
            let track = Track { recording: m.recording.as_deref(), artist: &m.artist, title: &m.title, album: m.album.as_deref(), length_secs: m.length_secs };
            x.find(&track).map(str::to_string)
        };
        let Some(song) = serve.catalogue().track_of_source(&req.source, &t.id).map(ids::song).or_else(by_name) else {
            run.missed.insert(t.id.clone());
            continue;
        };
        if !run.done.insert(song.clone()) {
            continue;
        }
        match serve.library.play_targets(&song)? {
            Some((pid, album, artists)) => found.push((song, pid, album, artists, t)),
            None => {
                run.missed.insert(t.id.clone());
            }
        }
    }
    let songs: Vec<String> = found.iter().map(|f| f.0.clone()).collect();
    let last = serve.userdata.imported(key, Kind::Song, &songs)?;
    let mut counts = Counts::default();
    let mut changes = Vec::new();
    for (song, pid, album, artists, t) in found {
        let seen = Seen {
            plays: w.plays.then_some((t.plays, t.last_played)),
            star: w.favorites.then_some((t.favorite, t.favorite_at)),
            rating: w.ratings.then_some(t.rating),
            server_item: t.id,
        };
        let before = last.get(&song);
        let d = decide(before, &seen);
        if d.is_nothing(before) {
            continue;
        }
        counts.count(&d);
        changes.push(ImportChange {
            kind: Kind::Song,
            item: song,
            pid: Some(pid),
            add_plays: d.add_plays,
            played: d.played,
            album: Some(album),
            artists,
            star: d.star,
            rating: d.rating,
            now: d.now,
        });
    }
    if !req.preview {
        serve.userdata.apply_import(key, &changes)?;
    }
    Ok(counts)
}

/// Stars an earlier import brought in whose server item the complete listing no longer has (with `listed`, every
/// server id it had): no longer a favourite there, so taken away here too, unless far too many (see `UNSTAR_SHARE`).
/// Returns the counts and a note when they were left. Blocking.
fn unstar_gone(serve: &Serve, req: &Request, kind: Kind, listed: &HashSet<String>) -> Result<(Counts, Option<String>)> {
    let key = ImportKey { account: &req.account, source: &req.source, server_user: &req.user };
    let starred = serve.userdata.imported_starred(key, kind)?;
    let gone: Vec<&(String, Imported)> = starred.iter().filter(|(_, i)| i.server_item.as_ref().is_none_or(|s| !listed.contains(s))).collect();
    let mut counts = Counts::default();
    if gone.is_empty() {
        return Ok((counts, None));
    }
    if gone.len() > UNSTAR_FLOOR.max(starred.len() * UNSTAR_SHARE / 100) {
        return Ok((counts, Some(format!("{} favourites are gone from the server; left as they were.", gone.len()))));
    }
    let changes: Vec<ImportChange> = gone
        .into_iter()
        .map(|(item, last)| {
            let seen = Seen { server_item: last.server_item.clone().unwrap_or_default(), star: Some((false, None)), ..Seen::default() };
            let d = decide(Some(last), &seen);
            counts.count(&d);
            ImportChange { kind, item: item.clone(), star: d.star, now: d.now, ..ImportChange::default() }
        })
        .collect();
    if !req.preview {
        serve.userdata.apply_import(key, &changes)?;
    }
    Ok((counts, None))
}

/// Favourite albums and artists: found, decided, applied, and the ones no longer favourites taken away. Blocking.
fn groups(serve: &Serve, req: &Request, g: crate::source::FavoriteGroups) -> Result<(Counts, Option<String>)> {
    let key = ImportKey { account: &req.account, source: &req.source, server_user: &req.user };
    let places = album_places(serve)?;
    let names = matcher(serve);
    let mut counts = Counts::default();
    let mut notes = Vec::new();
    // (kind, Cratarr id, server id, since when)
    let mut found: Vec<(Kind, String, String, Option<i64>)> = Vec::new();
    let mut listed: HashMap<Kind, HashSet<String>> = HashMap::new();
    for (a, at) in g.albums {
        listed.entry(Kind::Album).or_default().insert(a.id.clone());
        match album_of(serve, &req.source, &places, names.as_deref(), &a)? {
            Some(album) => found.push((Kind::Album, album, a.id, at)),
            None => counts.skipped += 1,
        }
    }
    for (a, at) in g.artists {
        listed.entry(Kind::Artist).or_default().insert(a.id.clone());
        let id = ids::artist(&a.name);
        if serve.library.has_artist(&id)? {
            found.push((Kind::Artist, id, a.id, at));
        } else {
            counts.skipped += 1;
        }
    }
    for kind in [Kind::Album, Kind::Artist] {
        let mine: Vec<&(Kind, String, String, Option<i64>)> = found.iter().filter(|f| f.0 == kind).collect();
        let items: Vec<String> = mine.iter().map(|f| f.1.clone()).collect();
        let last = serve.userdata.imported(key, kind, &items)?;
        let mut done = HashSet::new();
        let mut changes = Vec::new();
        for (_, item, server_item, at) in mine {
            if !done.insert(item.clone()) {
                continue;
            }
            let before = last.get(item);
            let d = decide(before, &Seen { server_item: server_item.clone(), star: Some((true, *at)), ..Seen::default() });
            if d.is_nothing(before) {
                continue;
            }
            counts.count(&d);
            changes.push(ImportChange { kind, item: item.clone(), star: d.star, now: d.now, ..ImportChange::default() });
        }
        if !req.preview {
            serve.userdata.apply_import(key, &changes)?;
        }
        let (gone, note) = unstar_gone(serve, req, kind, &listed.remove(&kind).unwrap_or_default())?;
        counts.add(&gone);
        notes.extend(note);
    }
    Ok((counts, (!notes.is_empty()).then(|| notes.join(" "))))
}

/// A page of albums' added dates: found, and set where earlier than the one known (`known` follows along). Blocking.
fn added_page(
    serve: &Serve,
    req: &Request,
    places: &AlbumPlaces,
    names: Option<&Matcher>,
    items: Vec<(AlbumRef, i64)>,
    known: &mut HashMap<String, i64>,
) -> Result<Counts> {
    let mut counts = Counts::default();
    let mut earlier = Vec::new();
    for (a, at) in items {
        let Some(album) = album_of(serve, &req.source, places, names, &a)? else {
            counts.skipped += 1;
            continue;
        };
        if known.get(&album).is_none_or(|k| at < *k) {
            known.insert(album.clone(), at);
            earlier.push((album, at));
        }
    }
    counts.added = earlier.len() as u64;
    if !req.preview {
        serve.userdata.set_added_many(&earlier, &req.source)?;
    }
    Ok(counts)
}

/// Runs blocking work with what it needs moved in and back out.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| anyhow::anyhow!("the import stopped: {e}"))?
}

/// A page from the source, asked again after each of `pace.retries` while it fails.
async fn ask<'a, T>(pace: &Pace, mut page: impl FnMut() -> Fut<'a, T>) -> Result<T> {
    let mut waits = pace.retries.iter();
    loop {
        match page().await {
            Ok(v) => return Ok(v),
            Err(e) => match waits.next() {
                Some(w) => {
                    tracing::warn!(target: "imports", reason = %format!("{e:#}"), again_in_s = w.as_secs(), "import page failed");
                    tokio::time::sleep(*w).await;
                }
                None => return Err(e),
            },
        }
    }
}

fn step(serve: &Serve, name: &'static str, read: usize, total: Option<usize>) {
    serve.import.update(|j| j.step = Some(Step { name, read, total }));
}

fn counted(serve: &Serve, c: &Counts, note: Option<String>) {
    serve.import.update(|j| {
        j.counts.add(c);
        if note.is_some() {
            j.note = note;
        }
    });
}

async fn import(serve: &Arc<Serve>, source: &dyn Source, req: &Request, pace: &Pace) -> Result<()> {
    let w = req.what;
    let req = Arc::new(req.clone());
    if w.plays || w.favorites || w.ratings {
        step(serve, "tracks", 0, None);
        let s = serve.clone();
        let mut run = Run { matcher: blocking(move || Ok(matcher(&s))).await?, ..Run::default() };
        let (mut cursor, mut read, mut total) = (None::<String>, 0, None);
        loop {
            let page = ask(pace, || source.track_data(&req.user, cursor.as_deref())).await?;
            total = page.total.or(total);
            read += page.items.len();
            let (s, r, mut taken) = (serve.clone(), req.clone(), std::mem::take(&mut run));
            let (counts, back) = blocking(move || Ok((tracks_page(&s, &r, page.items, &mut taken)?, taken))).await?;
            run = back;
            counted(serve, &counts, None);
            step(serve, "tracks", total.map_or(read, |t| read.min(t)), total);
            match page.next {
                Some(next) => {
                    cursor = Some(next);
                    tokio::time::sleep(pace.pause).await;
                }
                None => break,
            }
        }
        serve.import.update(|j| j.counts.skipped += run.missed.len() as u64);
        if w.favorites {
            let (s, r) = (serve.clone(), req.clone());
            let (counts, note) = blocking(move || unstar_gone(&s, &r, Kind::Song, &run.listed)).await?;
            counted(serve, &counts, note);
        }
    }
    if w.favorites {
        step(serve, "groups", 0, None);
        let g = ask(pace, || source.favorite_groups(&req.user)).await?;
        let n = g.albums.len() + g.artists.len();
        let (s, r) = (serve.clone(), req.clone());
        let (counts, note) = blocking(move || groups(&s, &r, g)).await?;
        counted(serve, &counts, note);
        step(serve, "groups", n, Some(n));
    }
    if w.added {
        step(serve, "added", 0, None);
        let s = serve.clone();
        let (places, names, mut known) = blocking(move || Ok((Arc::new(album_places(&s)?), matcher(&s), s.userdata.added()?))).await?;
        let (mut cursor, mut read, mut total) = (None::<String>, 0, None);
        loop {
            if cursor.is_some() {
                tokio::time::sleep(pace.pause).await;
            }
            let page = ask(pace, || source.albums_added(cursor.as_deref())).await?;
            total = page.total.or(total);
            read += page.items.len();
            let (s, r, p, m) = (serve.clone(), req.clone(), places.clone(), names.clone());
            let (counts, back) = blocking(move || Ok((added_page(&s, &r, &p, m.as_deref(), page.items, &mut known)?, known))).await?;
            known = back;
            counted(serve, &counts, None);
            step(serve, "added", total.map_or(read, |t| read.min(t)), total);
            match page.next {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
    }
    Ok(())
}

/// One import from start to end, its outcome kept in the job.
pub async fn run(serve: Arc<Serve>, source: Arc<dyn Source>, req: Request, pace: Pace) {
    let result = import(&serve, &*source, &req, &pace).await;
    if let Err(e) = &result {
        tracing::warn!(target: "imports", source = %req.source, account = %req.account, reason = %format!("{e:#}"), "import failed");
    }
    serve.import.update(|j| {
        j.finished = Some(now());
        j.step = None;
        j.error = result.err().map(|e| format!("{e:#}"));
        if !j.request.preview {
            tracing::info!(target: "imports", source = %j.request.source, account = %j.request.account, counts = ?j.counts, "import done");
        }
    });
}

/// `GET /api/subsonic/import/sources`: the sources that can import, each with its accounts and the choices made last
/// time (else its own account, and everything), and the Cratarr accounts.
pub async fn sources(State(serve): State<Arc<Serve>>) -> Response {
    let accounts = serve.users.names();
    let mut out = Vec::new();
    for (id, s) in serve.import.sources() {
        let (users, mine) = tokio::join!(s.users(), s.sync_account());
        let (ud, key) = (serve.userdata.clone(), id.clone());
        let last: Option<Request> =
            tokio::task::spawn_blocking(move || ud.import_choice(&key)).await.ok().and_then(Result::ok).flatten().and_then(|c| serde_json::from_str(&c).ok());
        match users {
            Ok(users) => {
                let user = last.as_ref().map(|l| l.user.clone()).filter(|u| users.iter().any(|(i, _)| i == u)).or(mine.ok());
                let account = last.as_ref().map(|l| l.account.clone()).filter(|a| accounts.contains(a));
                let users: Vec<_> = users.into_iter().map(|(id, name)| json!({ "id": id, "name": name })).collect();
                out.push(json!({ "id": id, "users": users, "user": user, "account": account, "what": last.map(|l| l.what).unwrap_or_default() }));
            }
            Err(e) => out.push(json!({ "id": id, "users": [], "error": format!("{e:#}") })),
        }
    }
    // The Subsonic servers imported from before (decision 0067): their address, login and choices; the password is
    // asked for again each run.
    let ud = serve.userdata.clone();
    let kept = tokio::task::spawn_blocking(move || ud.import_choices(SUBSONIC)).await.ok().and_then(Result::ok).unwrap_or_default();
    for (id, choice) in kept {
        let Ok(last) = serde_json::from_str::<Request>(&choice) else { continue };
        let account = Some(last.account.clone()).filter(|a| accounts.contains(a));
        out.push(json!({
            "id": id,
            "users": [{ "id": last.user, "name": last.user }],
            "user": last.user,
            "account": account,
            "what": last.what,
            "password": true,
        }));
    }
    axum::Json(json!({ "accounts": accounts, "sources": out })).into_response()
}

/// `GET /api/subsonic/import`: the import running now, or the last one.
pub async fn status(State(serve): State<Arc<Serve>>) -> Response {
    axum::Json(json!({ "job": serve.import.job() })).into_response()
}

/// `POST /api/subsonic/import`: starts an import (or a preview) in the background.
pub async fn start(State(serve): State<Arc<Serve>>, axum::Json(req): axum::Json<Request>) -> Response {
    let refuse = |status: StatusCode, why: &str| (status, axum::Json(json!({ "error": why }))).into_response();
    let source = match serve.import.sources().into_iter().find(|(id, _)| *id == req.source) {
        Some((_, s)) => s,
        // A Subsonic server, for this run only, with the password given now (decision 0067).
        None if req.source.starts_with(SUBSONIC) => {
            let Some(password) = req.password.as_deref().filter(|p| !p.is_empty()) else {
                return refuse(StatusCode::BAD_REQUEST, "The server's password is needed.");
            };
            let mut values = crate::source::Values::new();
            values.insert("url".into(), json!(&req.source[SUBSONIC.len()..]));
            values.insert("username".into(), json!(req.user));
            values.insert("password".into(), json!(password));
            match (crate::source::subsonic::KIND.connect)(&values, false) {
                Ok(s) => Arc::from(s),
                Err(e) => return refuse(StatusCode::BAD_REQUEST, &format!("{e:#}")),
            }
        }
        None => return refuse(StatusCode::BAD_REQUEST, "That server is not connected."),
    };
    if !serve.users.names().contains(&req.account) {
        return refuse(StatusCode::BAD_REQUEST, "There is no such account.");
    }
    let w = req.what;
    if !(w.plays || w.favorites || w.ratings || w.added) || req.user.trim().is_empty() {
        return refuse(StatusCode::BAD_REQUEST, "Nothing to import.");
    }
    let job = match serve.import.begin(req.clone()) {
        Ok(j) => j,
        Err(why) => return refuse(StatusCode::CONFLICT, why),
    };
    // The choices are offered again next time.
    let (ud, choice) = (serve.userdata.clone(), Request { preview: false, ..req.clone() });
    let kept = tokio::task::spawn_blocking(move || -> Result<()> { ud.set_import_choice(&choice.source, &serde_json::to_string(&choice)?) }).await;
    if let Ok(Err(e)) = kept {
        tracing::warn!(target: "imports", reason = %format!("{e:#}"), "import choices not kept");
    }
    tokio::spawn(run(serve.clone(), source, req, Pace::default()));
    (StatusCode::ACCEPTED, axum::Json(json!({ "job": job }))).into_response()
}

/// Whose import an undo takes back (decision 0067).
#[derive(Debug, Deserialize)]
pub struct Whose {
    pub source: String,
    pub user: String,
    pub account: String,
}

/// `DELETE /api/subsonic/import`: an import from a server undone as a whole (decision 0067): the plays it added, the
/// stars and ratings it set that are still as it set them; not while an import runs.
pub async fn undo(State(serve): State<Arc<Serve>>, axum::Json(w): axum::Json<Whose>) -> Response {
    if serve.import.job().is_some_and(|j| j.finished.is_none()) {
        return (StatusCode::CONFLICT, axum::Json(json!({ "error": "An import is running." }))).into_response();
    }
    let s = serve.clone();
    let done = tokio::task::spawn_blocking(move || {
        let key = ImportKey { account: &w.account, source: &w.source, server_user: &w.user };
        s.userdata.undo_import(key, |song| s.library.play_targets(song).ok().flatten().map(|(_, album, artists)| (album, artists)))
    })
    .await;
    match done {
        Ok(Ok(u)) => {
            tracing::info!(target: "imports", items = u.items, plays = u.plays, "import undone");
            axum::Json(json!({ "undone": u })).into_response()
        }
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({ "error": format!("{e:#}") }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({ "error": format!("{e:#}") }))).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::{Catalogue, Rules, ServedRow};
    use crate::serve::library::FolderRows;
    use crate::source::{ArtistRef, Choice, FavoriteGroups, Library, Playing, PlaylistEvent, Status, Track, TrackDataPage};
    use crate::userdata::Annotation;
    use std::collections::BTreeMap;

    fn seen(plays: Option<(u32, Option<i64>)>, star: Option<bool>, rating: Option<u8>) -> Seen {
        Seen { server_item: "s1".into(), plays, star: star.map(|on| (on, None)), rating }
    }

    #[test]
    fn a_first_import_brings_everything_in_and_takes_nothing_away() {
        let d = decide(None, &seen(Some((12, Some(100))), Some(true), Some(4)));
        assert_eq!((d.add_plays, d.played, d.star, d.rating), (12, Some(100), Some((true, None)), Some(4)));
        assert_eq!(d.now, Imported { server_item: Some("s1".into()), plays: 12, played: Some(100), starred: true, rating: 4 });
        let d = decide(None, &seen(Some((0, None)), Some(false), Some(0)));
        assert_eq!((d.add_plays, d.played, d.star, d.rating), (0, None, None, None), "no favourite and no rating on the server: nothing to take away");
    }

    #[test]
    fn again_only_what_changed_on_the_server_since() {
        let last = decide(None, &seen(Some((12, Some(100))), Some(true), Some(4))).now;
        let same = decide(Some(&last), &seen(Some((12, Some(100))), Some(true), Some(4)));
        assert!(same.is_nothing(Some(&last)), "{same:?}");
        let more = decide(Some(&last), &seen(Some((15, Some(200))), Some(true), Some(4)));
        assert_eq!((more.add_plays, more.played, more.star, more.rating), (3, Some(200), None, None));
        let unstarred = decide(Some(&last), &seen(None, Some(false), Some(2)));
        assert_eq!((unstarred.star, unstarred.rating), (Some((false, None)), Some(2)), "changed there: changed here");
        assert_eq!(unstarred.now.plays, 12, "plays not read: what was imported stays");
        let reset = decide(Some(&last), &seen(Some((2, Some(300))), None, None));
        assert_eq!((reset.add_plays, reset.now.plays), (0, 2), "a reset count adds nothing, and later plays count from it");
        assert_eq!(decide(Some(&reset.now), &seen(Some((5, Some(400))), None, None)).add_plays, 3);
        let moved = decide(Some(&last), &Seen { server_item: "s2".into(), ..seen(Some((12, Some(100))), Some(true), Some(4)) });
        assert!(!moved.is_nothing(Some(&last)) && moved.add_plays == 0, "a new server item for the same song: recorded, nothing added");
    }

    #[test]
    fn albums_are_found_by_the_folders_they_share_with_the_servers_path() {
        let roots = vec![PathBuf::from("/media/Music"), PathBuf::from("/media/lidarr-music")];
        let places = AlbumPlaces::new(
            vec![
                ("al-music".into(), 0, "Björk/Homogenic (1997)".into()),
                ("al-incoming".into(), 1, "Björk/Homogenic (1997)".into()),
                ("al-other".into(), 0, "Other/Homogenic (1997)".into()),
                ("al-loose".into(), 0, "".into()),
            ],
            &roots,
        );
        assert_eq!(places.find("/data/media/Music/Björk/Homogenic (1997)"), Some("al-music"));
        assert_eq!(places.find("/data/media/lidarr-music/Björk/Homogenic (1997)"), Some("al-incoming"));
        assert_eq!(places.find("D:\\Music\\Björk\\Homogenic (1997)\\CD1"), Some("al-music"), "a disc folder; the root's own name decides");
        assert_eq!(places.find("/srv/Björk/Homogenic (1997)"), None, "as near to both: neither");
        assert_eq!(places.find("/srv/Homogenic (1997)"), None, "the album's own folders must all match");
        assert_eq!(places.find("/data/media/Music/Nobody/Nothing"), None);
    }

    /// A media server whose data the tests change between imports. Tracks come two to a page.
    #[derive(Default)]
    struct Server {
        tracks: Vec<TrackData>,
        groups: FavoriteGroups,
        added: Vec<(AlbumRef, i64)>,
    }

    struct Fake(Mutex<Server>);

    impl Source for Fake {
        fn status(&self) -> Fut<'_, Status> {
            Box::pin(async { Ok(Status::default()) })
        }
        fn options(&self) -> Fut<'_, BTreeMap<String, Vec<Choice>>> {
            Box::pin(async { Ok(BTreeMap::new()) })
        }
        fn libraries(&self) -> Fut<'_, Vec<Library>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn sample_paths<'a>(&'a self, _id: &'a str, _n: usize) -> Fut<'a, Vec<String>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn now_playing(&self) -> Fut<'_, Option<Playing>> {
            Box::pin(async { Ok(None) })
        }
        fn track<'a>(&'a self, _id: &'a str) -> Fut<'a, Track> {
            Box::pin(async { anyhow::bail!("no tracks") })
        }
        fn playlists_changed(&self, _e: Vec<PlaylistEvent>) -> Fut<'_, ()> {
            Box::pin(async { Ok(()) })
        }
        fn imports(&self) -> bool {
            true
        }
        fn track_data<'a>(&'a self, user: &'a str, cursor: Option<&'a str>) -> Fut<'a, TrackDataPage> {
            Box::pin(async move {
                anyhow::ensure!(user == "u1", "not this server's account");
                let all = self.0.lock().unwrap().tracks.clone();
                let start: usize = cursor.map_or(0, |c| c.parse().unwrap());
                let items: Vec<TrackData> = all.iter().skip(start).take(2).cloned().collect();
                let next = (start + 2 < all.len()).then(|| (start + 2).to_string());
                Ok(TrackDataPage { items, next, total: cursor.is_none().then_some(all.len()) })
            })
        }
        fn favorite_groups<'a>(&'a self, _user: &'a str) -> Fut<'a, FavoriteGroups> {
            Box::pin(async move { Ok(self.0.lock().unwrap().groups.clone()) })
        }
        fn albums_added<'a>(&'a self, _cursor: Option<&'a str>) -> Fut<'a, crate::source::AddedPage> {
            Box::pin(async move {
                let items = self.0.lock().unwrap().added.clone();
                Ok(crate::source::AddedPage { total: Some(items.len()), items, next: None })
            })
        }
    }

    fn played(id: &str, plays: u32, at: i64) -> TrackData {
        TrackData { id: id.into(), plays, last_played: Some(at), ..TrackData::default() }
    }

    fn favourite(mut t: TrackData) -> TrackData {
        t.favorite = true;
        t
    }

    /// Three files in two album folders, catalogued, linked to the server's "s1" to "s3", and served as songs.
    fn library() -> (PathBuf, Arc<Serve>, [String; 3]) {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("cratarr-import-{}-{n}", std::process::id()));
        let files = ["Björk/Homogenic (1997)/01 Hunter.mp3", "Björk/Homogenic (1997)/02 Jóga.mp3", "The Orb/Adventures (1991)/01 Clouds.mp3"];
        for f in files {
            std::fs::create_dir_all(d.join(f).parent().unwrap()).unwrap();
            std::fs::write(d.join(f), f).unwrap();
        }
        let cat = Arc::new(Catalogue::open(None, vec![d.clone()]).unwrap());
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        let links: Vec<(String, i64, String)> = files.iter().enumerate().map(|(i, f)| (format!("s{}", i + 1), 0, f.to_string())).collect();
        cat.record_links("jf", "lib", &links, 1).unwrap();
        let tracks = files.map(|f| cat.track_of(0, f).unwrap());
        let serve = Serve::open(None, Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(cat);
        let tagged = |i: usize, artist: &str, album: &str, date: &str| ServedRow {
            rel: files[i].into(),
            track: Some(tracks[i]),
            size: 1,
            mtime: 1,
            tags: [("title", "t"), ("artist", artist), ("albumartist", artist), ("album", album), ("date", date), ("tracknumber", &(i + 1).to_string())]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..ServedRow::default()
        };
        let rows = vec![tagged(0, "Björk", "Homogenic", "1997"), tagged(1, "Björk", "Homogenic", "1997"), tagged(2, "The Orb", "Adventures", "1991")];
        serve.library.refresh(&[FolderRows { folder: 0, root: d.display().to_string(), rows }], &Rules::default()).unwrap();
        serve.users.set("me", "pw").unwrap();
        (d, serve, tracks.map(ids::song))
    }

    fn request(preview: bool) -> Request {
        Request { source: "jf".into(), user: "u1".into(), account: "me".into(), what: What::default(), preview, password: None }
    }

    fn quick() -> Pace {
        Pace { pause: Duration::ZERO, retries: Vec::new() }
    }

    async fn import_once(serve: &Arc<Serve>, fake: &Arc<Fake>, preview: bool) -> Job {
        serve.import.begin(request(preview)).unwrap();
        run(serve.clone(), fake.clone(), request(preview), quick()).await;
        let job = serve.import.job().unwrap();
        assert_eq!(job.error, None);
        job
    }

    fn song(serve: &Serve, id: &str) -> Annotation {
        serve.userdata.get("me", Kind::Song, &[id.to_string()]).unwrap().remove(id).unwrap_or_default()
    }

    fn album(serve: &Serve, name: &str) -> (String, Annotation) {
        let id = serve.library.album_places().unwrap().into_iter().find(|(_, _, dir)| dir.contains(name)).unwrap().0;
        let a = serve.userdata.get("me", Kind::Album, &[id.clone()]).unwrap().remove(&id).unwrap_or_default();
        (id, a)
    }

    #[tokio::test]
    async fn an_import_brings_in_plays_favourites_ratings_and_dates_and_again_only_what_changed() {
        let (dir, serve, [hunter, joga, clouds]) = library();
        let mut rated = played("s2", 2, 300);
        rated.rating = 4;
        let homogenic_path = Some("/data/media/Music/Björk/Homogenic (1997)".to_string());
        let fake = Arc::new(Fake(Mutex::new(Server {
            // Played first, then the favourites (a played favourite twice), and one the library does not have.
            tracks: vec![
                favourite(played("s1", 5, 200)),
                rated,
                played("s9", 3, 100),
                favourite(played("s1", 5, 200)),
                favourite(TrackData { id: "s3".into(), ..TrackData::default() }),
            ],
            groups: FavoriteGroups {
                albums: vec![
                    (AlbumRef { id: "a1".into(), track: None, path: homogenic_path.clone(), ..AlbumRef::default() }, Some(50)),
                    (AlbumRef { id: "a2".into(), track: None, path: Some("/data/media/Music/Nobody/Nothing".into()), ..AlbumRef::default() }, None),
                ],
                artists: vec![(ArtistRef { id: "r1".into(), name: "björk".into() }, None)],
            },
            added: vec![
                (AlbumRef { id: "a1".into(), track: None, path: homogenic_path, ..AlbumRef::default() }, 1_000),
                (AlbumRef { id: "a3".into(), track: Some("s3".into()), path: None, ..AlbumRef::default() }, 2_000),
            ],
        })));

        let preview = import_once(&serve, &fake, true).await;
        assert_eq!(preview.counts, Counts { plays: 7, songs: 2, starred: 4, unstarred: 0, rated: 1, added: 2, skipped: 2 });
        assert_eq!(song(&serve, &hunter), Annotation::default(), "a preview changes nothing");
        assert!(serve.userdata.added().unwrap().is_empty());

        let first = import_once(&serve, &fake, false).await;
        assert_eq!(first.counts, preview.counts, "the import does what the preview said");
        let h = song(&serve, &hunter);
        assert_eq!((h.play_count, h.play_date, h.starred_at.is_some()), (5, Some(200), true));
        assert_eq!((song(&serve, &joga).rating, song(&serve, &joga).play_count), (4, 2));
        assert!(song(&serve, &clouds).starred_at.is_some());
        let (homogenic, a) = album(&serve, "Homogenic");
        assert_eq!((a.play_count, a.play_date, a.starred_at), (7, Some(300), Some(50)), "the songs' plays count on their album");
        let bjork = ids::artist("Björk");
        assert!(serve.userdata.get("me", Kind::Artist, &[bjork.clone()]).unwrap()[&bjork].starred_at.is_some());
        let (adventures, _) = album(&serve, "Adventures");
        assert_eq!(serve.userdata.added().unwrap(), [(homogenic, 1_000), (adventures, 2_000)].into());

        let again = import_once(&serve, &fake, false).await;
        assert_eq!(again.counts, Counts { skipped: 2, ..Counts::default() }, "nothing changed on the server: nothing to do");
        assert_eq!(song(&serve, &hunter).play_count, 5, "no plays counted twice");

        // On the server: Hunter played three more times, Jóga rated again, Clouds and the album no longer favourites (so
        // no longer listed). In Cratarr: Hunter unstarred, which the server's unchanged favourite leaves alone.
        serve.userdata.set_starred("me", Kind::Song, &[hunter.clone()], false).unwrap();
        {
            let mut s = fake.0.lock().unwrap();
            s.tracks = vec![favourite(played("s1", 8, 900)), TrackData { rating: 2, ..played("s2", 2, 300) }];
            s.groups.albums.clear();
        }
        let later = import_once(&serve, &fake, false).await;
        assert_eq!(later.counts, Counts { plays: 3, songs: 1, unstarred: 2, rated: 1, ..Counts::default() });
        let h = song(&serve, &hunter);
        assert_eq!((h.play_count, h.play_date, h.starred_at), (8, Some(900), None));
        assert_eq!(song(&serve, &joga).rating, 2);
        assert_eq!(song(&serve, &clouds).starred_at, None, "gone from the server's favourites: gone here");
        let (_, a) = album(&serve, "Homogenic");
        assert_eq!((a.play_count, a.starred_at), (10, None));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn only_what_is_ticked_and_one_import_at_a_time() {
        let (dir, serve, [hunter, ..]) = library();
        let fake = Arc::new(Fake(Mutex::new(Server { tracks: vec![favourite(played("s1", 5, 200))], ..Server::default() })));
        let mut req = request(false);
        req.what = What { plays: false, favorites: true, ratings: false, added: false };
        serve.import.begin(req.clone()).unwrap();
        assert_eq!(serve.import.begin(req.clone()).unwrap_err(), "An import is running.");
        run(serve.clone(), fake.clone(), req, quick()).await;
        let h = song(&serve, &hunter);
        assert_eq!((h.play_count, h.starred_at.is_some()), (0, true));
        // Plays later: all of them, as none were imported before.
        let job = import_once(&serve, &fake, false).await;
        assert_eq!((job.counts.plays, song(&serve, &hunter).play_count), (5, 5));
        let mut wrong = request(false);
        wrong.user = "someone".into();
        serve.import.begin(wrong.clone()).unwrap();
        run(serve.clone(), fake.clone(), wrong, Pace { pause: Duration::ZERO, retries: vec![Duration::ZERO] }).await;
        assert!(serve.import.job().unwrap().error.is_some_and(|e| e.contains("not this server's account")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn far_too_many_favourites_gone_at_once_are_left() {
        let (dir, serve, _) = library();
        let req = request(false);
        let key = ImportKey { account: "me", source: "jf", server_user: "u1" };
        let star = |i: usize| ImportChange {
            kind: Kind::Song,
            item: format!("tr-{}", 100 + i),
            star: Some((true, None)),
            now: Imported { server_item: Some(format!("x{i}")), starred: true, ..Imported::default() },
            ..ImportChange::default()
        };
        serve.userdata.apply_import(key, &(0..30).map(star).collect::<Vec<_>>()).unwrap();
        let (counts, note) = unstar_gone(&serve, &req, Kind::Song, &HashSet::new()).unwrap();
        assert_eq!((counts.unstarred, note.as_deref()), (0, Some("30 favourites are gone from the server; left as they were.")));
        let listed: HashSet<String> = (0..12).map(|i| format!("x{i}")).collect();
        let (counts, note) = unstar_gone(&serve, &req, Kind::Song, &listed).unwrap();
        assert_eq!((counts.unstarred, note), (18, None), "up to 20 at once");
        assert_eq!(serve.userdata.starred("me", Kind::Song).unwrap().len(), 12);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
