//! Diagnostics (decision 0045): what happened, kept by Cratarr itself, so that "why did that happen" can be read days
//! later and after a deploy, across the server and its players. One small SQLite file in the data folder
//! (`diagnostics.sqlite`; in memory without one), apart from the undo journal (`journal`).
//!
//! It is fed by the server's own logging: an event whose `target` is one of [AREAS] is kept, with its message (constant
//! text: "connection ended", not "Phone disconnected") and its fields, and those of the spans it happened in (a
//! request's `request` and `conn`; its own win); `player` says where it happened (else the server), `client` holds an address (only there: Download masks it), and `conn`, `request`, `path`, `status` and
//! `reason` are the others the page knows. Events with another target (a module's path) are printed only. The layer
//! hands each event to a thread of its own, never writing on the caller's; when that thread falls behind, what does not
//! fit is counted and the count logged. Fields named like a secret (a key, password, token, session, cookie,
//! authorization) are left out.
//!
//! The same event again within a minute (same area, level, message, place, client, path, status and reason) adds to
//! the count on its row. Kept 14 days, at most 100,000 rows, the oldest going first.
//!
//! The players' own logs come in through [Diagnostics::add_lines], numbered so that sending again adds nothing. What is
//! logged by area changes while Cratarr runs ([set_levels]), for a while and then back by itself; the same filter governs
//! this log and what the server prints.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, ToSql, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// The areas an event can be of (its `target`).
// `playback`: a page's own player, what it did with each track (its device lines; the owner, 2026-10-26).
pub const AREAS: [&str; 8] = ["playon", "playback", "syncplay", "signin", "library", "subsonic", "imports", "server"];
/// How long a row is kept.
const KEEP_MS: i64 = 14 * 24 * 3600 * 1000;
/// At most this many rows.
const MAX_ROWS: i64 = 100_000;
/// The same event again within this long counts on its first row.
const SAME_MS: i64 = 60_000;
/// Events waiting for the writer; more are counted, not kept.
const QUEUE: usize = 10_000;
/// The old rows go this often.
const PRUNE_EVERY: Duration = Duration::from_secs(3600);
/// Field names holding these are never kept.
const SECRET: [&str; 7] = ["key", "password", "token", "secret", "session", "cookie", "authorization"];
/// The fields that, with the area, level, message and place, tell one event from another.
const SAME: [&str; 4] = ["client", "path", "status", "reason"];
/// Lines taken from one player at once.
const MAX_LINES: usize = 2_000;
/// Levels from the most to the least serious.
const LEVELS: [&str; 5] = ["error", "warn", "info", "debug", "trace"];

const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY,
  at INTEGER NOT NULL,
  last INTEGER NOT NULL,
  count INTEGER NOT NULL DEFAULT 1,
  level TEXT NOT NULL,
  area TEXT NOT NULL,
  place TEXT NOT NULL,
  what TEXT NOT NULL,
  fields TEXT NOT NULL DEFAULT '{}',
  origin TEXT NOT NULL DEFAULT 'server');
CREATE INDEX IF NOT EXISTS events_by_time ON events (at, id);
CREATE INDEX IF NOT EXISTS events_by_place ON events (place, at);
CREATE TABLE IF NOT EXISTS lines_held (player TEXT NOT NULL, log TEXT NOT NULL, last INTEGER NOT NULL, PRIMARY KEY (player, log));
";

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn secret(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    SECRET.iter().any(|s| n.contains(s))
}

fn level_name(l: Level) -> &'static str {
    match l {
        Level::ERROR => "error",
        Level::WARN => "warn",
        Level::INFO => "info",
        Level::DEBUG => "debug",
        Level::TRACE => "trace",
    }
}

/// One event, as the layer took it.
#[derive(Debug, Clone)]
struct Noted {
    at: i64,
    level: &'static str,
    area: &'static str,
    place: String,
    what: String,
    fields: Map<String, Value>,
}

enum Msg {
    Note(Noted),
    /// Answered once everything sent before it is written.
    Flush(mpsc::Sender<()>),
}

#[derive(Default)]
struct Fields {
    what: Option<String>,
    map: Map<String, Value>,
}

impl Fields {
    fn put(&mut self, f: &Field, v: Value) {
        if f.name() == "message" {
            self.what = Some(v.as_str().map_or_else(|| v.to_string(), str::to_string));
        } else if !secret(f.name()) {
            self.map.insert(f.name().to_string(), v);
        }
    }
}

impl Visit for Fields {
    fn record_str(&mut self, f: &Field, v: &str) {
        self.put(f, v.into());
    }
    fn record_i64(&mut self, f: &Field, v: i64) {
        self.put(f, v.into());
    }
    fn record_u64(&mut self, f: &Field, v: u64) {
        self.put(f, v.into());
    }
    fn record_bool(&mut self, f: &Field, v: bool) {
        self.put(f, v.into());
    }
    fn record_f64(&mut self, f: &Field, v: f64) {
        self.put(f, v.into());
    }
    fn record_error(&mut self, f: &Field, v: &(dyn std::error::Error + 'static)) {
        self.put(f, v.to_string().into());
    }
    fn record_debug(&mut self, f: &Field, v: &dyn std::fmt::Debug) {
        self.put(f, format!("{v:?}").into());
    }
}

/// The layer that takes the areas' events (decision 0045); [layer] makes it.
pub struct DiagnosticsLayer {
    tx: mpsc::SyncSender<Msg>,
    dropped: Arc<AtomicU64>,
}

/// What the layer took before the log was opened ([Diagnostics::start] writes it).
pub struct Pending {
    rx: mpsc::Receiver<Msg>,
    tx: mpsc::SyncSender<Msg>,
    dropped: Arc<AtomicU64>,
}

/// The layer for the server's logging, and what it takes until the log is opened (the data folder is known later).
pub fn layer() -> (DiagnosticsLayer, Pending) {
    let (tx, rx) = mpsc::sync_channel(QUEUE);
    let dropped = Arc::new(AtomicU64::new(0));
    (DiagnosticsLayer { tx: tx.clone(), dropped: dropped.clone() }, Pending { rx, tx, dropped })
}

/// A span's fields, kept for the events in it.
struct SpanFields(Map<String, Value>);

impl<S> Layer<S> for DiagnosticsLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut f = Fields::default();
        attrs.record(&mut f);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(SpanFields(f.map));
        }
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let mut f = Fields::default();
        values.record(&mut f);
        let mut ext = span.extensions_mut();
        match ext.get_mut::<SpanFields>() {
            Some(SpanFields(kept)) => kept.extend(f.map),
            None => ext.insert(SpanFields(f.map)),
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let meta = event.metadata();
        let Some(area) = AREAS.iter().copied().find(|a| *a == meta.target()) else { return };
        // The spans' fields from the outermost in, then the event's own over them.
        let mut f = Fields::default();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                if let Some(SpanFields(kept)) = span.extensions().get::<SpanFields>() {
                    f.map.extend(kept.clone());
                }
            }
        }
        event.record(&mut f);
        let place = match f.map.remove("player") {
            Some(Value::String(p)) if !p.is_empty() => p,
            Some(Value::Null | Value::String(_)) | None => "server".into(),
            Some(other) => other.to_string(),
        };
        let noted = Noted { at: now_ms(), level: level_name(*meta.level()), area, place, what: f.what.unwrap_or_default(), fields: f.map };
        if self.tx.try_send(Msg::Note(noted)).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// A row of the log.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub id: i64,
    /// When it happened first (ms since the epoch, on Cratarr's clock).
    pub at: i64,
    /// When it happened last (the same as `at` unless it came again).
    pub last: i64,
    /// How many times it happened.
    pub count: i64,
    pub level: String,
    pub area: String,
    /// Where it happened: a player's name, or "server".
    pub place: String,
    pub what: String,
    pub fields: Value,
    /// "server", or "device": a line from the player's own log.
    pub origin: String,
}

/// Which rows: newest first.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Query {
    /// A player's name, or "server".
    pub place: Option<String>,
    pub area: Option<String>,
    /// The least serious level shown: "warn" shows warnings and errors.
    pub level: Option<String>,
    /// From this time on (ms since the epoch).
    pub since: Option<i64>,
    /// The rows after this one, going back (its id): the next page.
    pub before: Option<i64>,
    /// At most this many (200 when not said, never more than 1,000).
    pub limit: Option<usize>,
}

/// A line of a player's own log (decision 0045 item 2), its time already on Cratarr's clock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceLine {
    /// Its number in the player's log: counts up, never repeats within one log.
    pub seq: u64,
    /// When (ms since the epoch, on Cratarr's clock).
    pub at: i64,
    #[serde(default = "info")]
    pub level: String,
    /// One of [AREAS].
    pub area: String,
    pub what: String,
    #[serde(default)]
    pub fields: Map<String, Value>,
}

fn info() -> String {
    "info".into()
}

/// The log.
pub struct Diagnostics {
    conn: Arc<Mutex<Connection>>,
    tx: mpsc::SyncSender<Msg>,
}

static LOG: OnceLock<Arc<Diagnostics>> = OnceLock::new();

/// A panic stops Cratarr (`panic = "abort"`): first it is written down, where and why and on which thread, and the
/// log written out, so the cause outlives the restart (the audit, 2026-10-29: a panic left nothing in the log).
pub fn keep_panics() {
    let shown = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let payload = info.payload();
        let why = payload.downcast_ref::<&str>().map(|s| (*s).to_string()).or_else(|| payload.downcast_ref::<String>().cloned()).unwrap_or_default();
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        tracing::error!(target: "server", at, why, thread, "panic: Cratarr stops");
        if let Some(d) = get() {
            d.flush();
        }
        shown(info);
    }));
}

/// The server's log, once opened. Process-wide as the tracing layer that writes it is: it opens before anything else
/// and outlives every request.
pub fn get() -> Option<Arc<Diagnostics>> {
    LOG.get().cloned()
}

/// Opens the server's log and starts writing what the layer took (from before too).
pub fn start_global(file: Option<&Path>, pending: Pending) -> Result<Arc<Diagnostics>> {
    let d = Diagnostics::start(file, pending)?;
    let _ = LOG.set(d.clone());
    Ok(d)
}

impl Diagnostics {
    /// Opens the log at `file` (`None`, or a file that cannot be opened: in memory, so Cratarr runs on) and starts its
    /// writer.
    pub fn start(file: Option<&Path>, pending: Pending) -> Result<Arc<Self>> {
        let opened = file.map(|f| crate::db::open(Some(f)).and_then(|c| c.execute_batch(SCHEMA).map(|()| c).map_err(Into::into)));
        let conn = match opened {
            Some(Ok(c)) => c,
            Some(Err(e)) => {
                tracing::warn!("diagnostics: kept in memory only: {e:#}");
                let c = crate::db::open(None)?;
                c.execute_batch(SCHEMA)?;
                c
            }
            None => {
                let c = crate::db::open(None)?;
                c.execute_batch(SCHEMA)?;
                c
            }
        };
        let conn = Arc::new(Mutex::new(conn));
        let Pending { rx, tx, dropped } = pending;
        let writer = Writer { conn: conn.clone(), recent: HashMap::new(), pruned: None, dropped };
        std::thread::Builder::new().name("cratarr-diagnostics".into()).spawn(move || writer.run(rx))?;
        Ok(Arc::new(Self { conn, tx }))
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Waits (at most 5 s) until what was logged before is written: before the server stops, and in tests.
    pub fn flush(&self) {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Msg::Flush(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(5));
        }
    }

    /// Rows, newest first.
    /// How many times an event (`area`, `what`) happened since `since_ms` (epoch ms), the minute's repeats counted
    /// (decision 0063: a domain's check reads its last hour here).
    pub fn count_since(&self, area: &str, what: &str, since_ms: i64) -> Result<u64> {
        let n: i64 = self.conn().query_row(
            "SELECT coalesce(sum(count), 0) FROM events WHERE area = ?1 AND what = ?2 AND last >= ?3",
            params![area, what, since_ms],
            |r| r.get(0),
        )?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    pub fn query(&self, q: &Query) -> Result<Vec<Row>> {
        let mut sql = String::from("SELECT id, at, last, count, level, area, place, what, fields, origin FROM events WHERE 1 = 1");
        let mut p: Vec<Box<dyn ToSql>> = Vec::new();
        if let Some(place) = &q.place {
            p.push(Box::new(place.clone()));
            sql.push_str(&format!(" AND place = ?{}", p.len()));
        }
        if let Some(area) = &q.area {
            p.push(Box::new(area.clone()));
            sql.push_str(&format!(" AND area = ?{}", p.len()));
        }
        if let Some(level) = &q.level {
            let Some(i) = LEVELS.iter().position(|l| l.eq_ignore_ascii_case(level)) else { bail!("no level is called {level}") };
            let shown: Vec<String> = LEVELS[..=i].iter().map(|l| format!("'{l}'")).collect();
            sql.push_str(&format!(" AND level IN ({})", shown.join(", ")));
        }
        if let Some(since) = q.since {
            p.push(Box::new(since));
            sql.push_str(&format!(" AND last >= ?{}", p.len()));
        }
        if let Some(before) = q.before {
            p.push(Box::new(before));
            sql.push_str(&format!(" AND (at, id) < (SELECT at, id FROM events WHERE id = ?{})", p.len()));
        }
        p.push(Box::new(i64::try_from(q.limit.unwrap_or(200).clamp(1, 1_000)).unwrap_or(200)));
        sql.push_str(&format!(" ORDER BY at DESC, id DESC LIMIT ?{}", p.len()));
        rows(&self.conn(), &sql, &p)
    }

    /// Everything kept, oldest first, for a bug report: client addresses masked (decision 0045 item 5), each address
    /// one name throughout ("home-1", "outside-2"), so what one client did can still be followed.
    pub fn download(&self) -> Result<Vec<Row>> {
        let mut all = rows(&self.conn(), "SELECT id, at, last, count, level, area, place, what, fields, origin FROM events ORDER BY at, id", &[])?;
        let mut names = HashMap::new();
        for r in &mut all {
            if let Some(Value::String(c)) = r.fields.get_mut("client") {
                *c = masked(c, &mut names);
            }
        }
        Ok(all)
    }

    /// The last line held of `player`'s log `log` (0: none yet).
    pub fn last_line(&self, player: &str, log: &str) -> Result<u64> {
        let last: Option<i64> =
            self.conn().query_row("SELECT last FROM lines_held WHERE player = ?1 AND log = ?2", params![player, log], |r| r.get(0)).optional()?;
        Ok(u64::try_from(last.unwrap_or(0)).unwrap_or(0))
    }

    /// Lines from `player`'s own log `log` (a new name when the app's log starts over): those after the last one held
    /// are added under the player, at most 2,000 at a time, and the last one held is the answer. Sending the same
    /// lines again adds nothing. Blocking: called off the async threads.
    pub fn add_lines(&self, player: &str, log: &str, lines: &[DeviceLine]) -> Result<u64> {
        if player.is_empty() || log.is_empty() {
            bail!("a player's lines need its name and its log's");
        }
        let mut conn = self.conn();
        let t = conn.transaction()?;
        let held: i64 = t.query_row("SELECT last FROM lines_held WHERE player = ?1 AND log = ?2", params![player, log], |r| r.get(0)).optional()?.unwrap_or(0);
        let mut last = held;
        let mut new: Vec<&DeviceLine> = lines.iter().filter(|l| i64::try_from(l.seq).is_ok_and(|s| s > held)).collect();
        new.sort_by_key(|l| l.seq);
        for l in new.into_iter().take(MAX_LINES) {
            let seq = i64::try_from(l.seq).unwrap_or(i64::MAX);
            if seq <= last {
                continue;
            }
            let level = LEVELS.iter().find(|v| v.eq_ignore_ascii_case(&l.level)).copied().unwrap_or("info");
            let fields: Map<String, Value> = l.fields.iter().filter(|(k, _)| !secret(k) && k.as_str() != "player").map(|(k, v)| (k.clone(), v.clone())).collect();
            let area: String = l.area.chars().take(32).collect();
            t.execute(
                "INSERT INTO events (at, last, level, area, place, what, fields, origin) VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, 'device')",
                params![l.at, level, area, player, l.what, Value::Object(fields).to_string()],
            )?;
            last = seq;
        }
        t.execute(
            "INSERT INTO lines_held (player, log, last) VALUES (?1, ?2, ?3) ON CONFLICT (player, log) DO UPDATE SET last = excluded.last",
            params![player, log, last],
        )?;
        t.commit()?;
        Ok(u64::try_from(last).unwrap_or(0))
    }
}

fn rows(conn: &Connection, sql: &str, p: &[Box<dyn ToSql>]) -> Result<Vec<Row>> {
    let mut stmt = conn.prepare(sql)?;
    let found = stmt.query_map(rusqlite::params_from_iter(p.iter()), |r| {
        let fields: String = r.get(8)?;
        Ok(Row {
            id: r.get(0)?,
            at: r.get(1)?,
            last: r.get(2)?,
            count: r.get(3)?,
            level: r.get(4)?,
            area: r.get(5)?,
            place: r.get(6)?,
            what: r.get(7)?,
            fields: serde_json::from_str(&fields).unwrap_or_else(|_| Value::Object(Map::new())),
            origin: r.get(9)?,
        })
    })?;
    Ok(found.collect::<rusqlite::Result<_>>()?)
}

/// `client` (an address, or several: a proxy's list) by the names given so far: the home network's addresses (private,
/// loopback, link-local) as "home-N", others as "outside-N", what is no address as "other-N"; "unknown" stays.
fn masked(client: &str, names: &mut HashMap<String, String>) -> String {
    client
        .split(',')
        .map(str::trim)
        .map(|c| {
            if c.is_empty() || c == "unknown" {
                return c.to_string();
            }
            let n = names.len() + 1;
            names
                .entry(c.to_string())
                .or_insert_with(|| {
                    let kind = match address(c) {
                        Some(ip) if home(ip) => "home",
                        Some(_) => "outside",
                        None => "other",
                    };
                    format!("{kind}-{n}")
                })
                .clone()
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn address(c: &str) -> Option<IpAddr> {
    c.parse::<IpAddr>().ok().or_else(|| c.parse::<SocketAddr>().ok().map(|s| s.ip()))
}

fn home(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => home(IpAddr::V4(v4)),
            None => v6.is_loopback() || v6.is_unique_local() || v6.is_unicast_link_local(),
        },
    }
}

struct Writer {
    conn: Arc<Mutex<Connection>>,
    /// The rows written within the last minute, by what tells them apart: a repeat counts on its row.
    recent: HashMap<String, (i64, i64)>,
    pruned: Option<Instant>,
    dropped: Arc<AtomicU64>,
}

impl Writer {
    fn run(mut self, rx: mpsc::Receiver<Msg>) {
        while let Ok(first) = rx.recv() {
            // What came meanwhile, written together.
            let mut batch = vec![first];
            while batch.len() < 500 {
                match rx.try_recv() {
                    Ok(m) => batch.push(m),
                    Err(_) => break,
                }
            }
            let mut waiting = Vec::new();
            let mut notes = Vec::new();
            for m in batch {
                match m {
                    Msg::Note(n) => notes.push(n),
                    Msg::Flush(done) => waiting.push(done),
                }
            }
            let lost = self.dropped.swap(0, Ordering::Relaxed);
            if lost > 0 {
                let mut fields = Map::new();
                fields.insert("lost".into(), lost.into());
                notes.push(Noted { at: now_ms(), level: "warn", area: "server", place: "server".into(), what: "diagnostics fell behind".into(), fields });
            }
            if let Err(e) = self.write(&notes) {
                tracing::warn!("diagnostics: {} events not kept: {e:#}", notes.len());
            }
            if self.pruned.is_none_or(|p| p.elapsed() >= PRUNE_EVERY) {
                let conn = self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Err(e) = prune(&conn, now_ms(), KEEP_MS, MAX_ROWS) {
                    tracing::warn!("diagnostics: old rows not cleared: {e:#}");
                }
                self.pruned = Some(Instant::now());
            }
            for done in waiting {
                let _ = done.send(());
            }
        }
    }

    fn write(&mut self, notes: &[Noted]) -> Result<()> {
        if notes.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let t = conn.transaction()?;
        for n in notes {
            note(&t, &mut self.recent, n)?;
        }
        t.commit()?;
        Ok(())
    }
}

/// What tells an event from another: a repeat of the same counts on its row.
fn same_key(n: &Noted) -> String {
    let mut key = format!("{}\u{1f}{}\u{1f}{}\u{1f}{}", n.area, n.level, n.what, n.place);
    for f in SAME {
        key.push('\u{1f}');
        if let Some(v) = n.fields.get(f) {
            key.push_str(&v.to_string());
        }
    }
    key
}

fn note(conn: &Connection, recent: &mut HashMap<String, (i64, i64)>, n: &Noted) -> Result<()> {
    let key = same_key(n);
    if let Some(&(id, first)) = recent.get(&key)
        && n.at - first < SAME_MS
        && conn.execute("UPDATE events SET count = count + 1, last = MAX(last, ?2) WHERE id = ?1", params![id, n.at])? == 1
    {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO events (at, last, level, area, place, what, fields) VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6)",
        params![n.at, n.level, n.area, n.place, n.what, Value::Object(n.fields.clone()).to_string()],
    )?;
    recent.insert(key, (conn.last_insert_rowid(), n.at));
    if recent.len() > 4_096 {
        recent.retain(|_, (_, first)| n.at - *first < SAME_MS);
    }
    Ok(())
}

/// Rows last seen more than `keep_ms` ago go, and beyond the newest `max_rows` the oldest.
fn prune(conn: &Connection, now: i64, keep_ms: i64, max_rows: i64) -> Result<()> {
    conn.execute("DELETE FROM events WHERE last < ?1", [now - keep_ms])?;
    conn.execute("DELETE FROM events WHERE id IN (SELECT id FROM events ORDER BY at DESC, id DESC LIMIT -1 OFFSET ?1)", [max_rows])?;
    Ok(())
}

// --- what is logged, by area, while Cratarr runs ---

/// The filter for `directives` (as `RUST_LOG`'s): the tag reader's warnings about every MP3 whose length it estimates
/// kept out (they are normal, and would flood the output).
pub fn filter(directives: &str) -> Result<EnvFilter> {
    Ok(EnvFilter::try_new(directives)?.add_directive("lofty=error".parse()?))
}

/// What is logged when nothing else is set: `RUST_LOG`, else "info".
pub fn base_directives() -> String {
    std::env::var("RUST_LOG").ok().filter(|v| filter(v).is_ok()).unwrap_or_else(|| "info".into())
}

/// What is logged now.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LevelsNow {
    /// As `RUST_LOG`'s: the base, and what was added to it.
    pub directives: String,
    /// What is logged when nothing is added.
    pub base: String,
    /// Until when the addition holds (ms since the epoch); none: the base, for good.
    pub until: Option<i64>,
}

type Reload = Box<dyn Fn(EnvFilter) -> std::result::Result<(), String> + Send + Sync>;

struct Levels {
    reload: Reload,
    base: String,
    now: Mutex<LevelsNow>,
    /// Counts up at every change: a return to the base that a newer change overtook does nothing.
    turn: AtomicU64,
}

impl Levels {
    fn new(base: String, reload: Reload) -> Arc<Self> {
        let now = LevelsNow { directives: base.clone(), base: base.clone(), until: None };
        Arc::new(Self { reload, base, now: Mutex::new(now), turn: AtomicU64::new(0) })
    }

    fn current(&self) -> LevelsNow {
        self.now.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    fn set(self: &Arc<Self>, added: &str, holds: Duration) -> Result<LevelsNow> {
        let added = added.trim();
        if added.is_empty() {
            return self.reset();
        }
        let directives = format!("{},{added}", self.base);
        (self.reload)(filter(&directives)?).map_err(anyhow::Error::msg)?;
        let turn = self.turn.fetch_add(1, Ordering::SeqCst) + 1;
        let until = now_ms().saturating_add(i64::try_from(holds.as_millis()).unwrap_or(i64::MAX));
        let now = LevelsNow { directives, base: self.base.clone(), until: Some(until) };
        *self.now.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = now.clone();
        let back = Arc::clone(self);
        std::thread::Builder::new().name("cratarr-levels".into()).spawn(move || {
            std::thread::sleep(holds);
            if back.turn.load(Ordering::SeqCst) == turn {
                let _ = back.reset();
            }
        })?;
        Ok(now)
    }

    fn reset(&self) -> Result<LevelsNow> {
        (self.reload)(filter(&self.base)?).map_err(anyhow::Error::msg)?;
        self.turn.fetch_add(1, Ordering::SeqCst);
        let now = LevelsNow { directives: self.base.clone(), base: self.base.clone(), until: None };
        *self.now.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = now.clone();
        Ok(now)
    }
}

static SET_LEVELS: OnceLock<Arc<Levels>> = OnceLock::new();

/// Keeps the way to change what is logged (the reloadable filter `main` set up with `base`).
pub fn keep_levels(base: String, reload: impl Fn(EnvFilter) -> std::result::Result<(), String> + Send + Sync + 'static) {
    let _ = SET_LEVELS.set(Levels::new(base, Box::new(reload)));
}

/// What is logged now.
pub fn levels() -> Option<LevelsNow> {
    SET_LEVELS.get().map(|l| l.current())
}

/// The longest an addition holds.
pub const LONGEST: Duration = Duration::from_secs(7 * 24 * 3600);

/// More (or less) logged for `holds` (at most a week), then the base again by itself: `added` as `RUST_LOG`'s, on top of
/// the base ("playon=debug"); empty: the base now.
pub fn set_levels(added: &str, holds: Duration) -> Result<LevelsNow> {
    let Some(l) = SET_LEVELS.get() else { bail!("what is logged cannot be changed here") };
    l.set(added, holds.min(LONGEST))
}

/// A test's own logging: `subscriber` as this thread's default until it is dropped, with a second dispatcher registered
/// beside it. With only one dispatcher registered, tracing-core (0.1.36, `has_just_one`) takes a callsite's interest
/// from the default of whichever thread reaches the callsite first, and keeps it for every thread: a test running at
/// the same moment without a subscriber switched "request refused" off for the test that logged it (lost in 2 of 4
/// full runs, never alone). With two, the interest comes from every registered dispatcher.
#[cfg(test)]
pub(crate) fn log_here<S: Subscriber + Send + Sync + 'static>(subscriber: S) -> TestLogging {
    let beside = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    TestLogging { _default: tracing::subscriber::set_default(subscriber), _beside: beside }
}

#[cfg(test)]
pub(crate) struct TestLogging {
    _default: tracing::subscriber::DefaultGuard,
    _beside: tracing::Dispatch,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;

    /// A log in memory, and the events of `f` logged into it.
    fn logged(f: impl FnOnce()) -> Arc<Diagnostics> {
        let (layer, pending) = layer();
        let d = Diagnostics::start(None, pending).unwrap();
        {
            let _logging = log_here(tracing_subscriber::registry().with(layer));
            f();
        }
        d.flush();
        d
    }

    fn all(d: &Diagnostics) -> Vec<Row> {
        d.query(&Query { limit: Some(1_000), ..Default::default() }).unwrap()
    }

    #[test]
    fn the_areas_events_are_kept_with_their_fields_and_nothing_secret() {
        let d = logged(|| {
            tracing::info!(target: "playon", player = "Phone", conn = "c1", reason = "replaced", "connection ended");
            tracing::warn!(target: "signin", client = "84.1.2.3", path = "/api/players", status = 401u16, token = "abc", api_key = "k", "request refused");
            tracing::info!("a module's line, printed only");
            tracing::info!(target: "cratarr_server::app", "not an area");
        });
        let rows = all(&d);
        assert_eq!(rows.len(), 2);
        let refused = &rows[0];
        assert_eq!((refused.area.as_str(), refused.level.as_str(), refused.place.as_str(), refused.what.as_str()), ("signin", "warn", "server", "request refused"));
        assert_eq!(refused.fields, serde_json::json!({ "client": "84.1.2.3", "path": "/api/players", "status": 401 }), "no token, no key");
        let ended = &rows[1];
        assert_eq!((ended.place.as_str(), ended.origin.as_str(), ended.count), ("Phone", "server", 1));
        assert_eq!(ended.fields, serde_json::json!({ "conn": "c1", "reason": "replaced" }));
    }

    #[test]
    fn the_spans_an_event_happened_in_give_their_fields_and_its_own_win() {
        let d = logged(|| {
            let request = tracing::info_span!("request", request = "r1", conn = tracing::field::Empty, reason = "outer", password = "pw");
            let _in = request.enter();
            request.record("conn", "c9");
            let inner = tracing::info_span!("player", player = "Phone");
            inner.in_scope(|| tracing::warn!(target: "signin", reason = "inner", "request refused"));
            drop(_in);
            tracing::info!(target: "library", "library built");
        });
        let rows = all(&d);
        assert_eq!(rows[1].fields, serde_json::json!({ "request": "r1", "conn": "c9", "reason": "inner" }), "no password from a span either");
        assert_eq!(rows[1].place, "Phone");
        assert_eq!((rows[0].what.as_str(), &rows[0].fields), ("library built", &serde_json::json!({})), "outside the span: none of its fields");
    }

    #[test]
    fn the_same_event_within_a_minute_counts_on_one_row() {
        let d = logged(|| {
            for _ in 0..3 {
                tracing::warn!(target: "signin", client = "84.1.2.3", path = "/api/players/Phone/connect", status = 401u16, "request refused");
            }
            tracing::warn!(target: "signin", client = "84.1.2.4", path = "/api/players/Phone/connect", status = 401u16, "request refused");
            tracing::warn!(target: "signin", client = "84.1.2.3", path = "/api/players/Phone/connect", status = 403u16, "request refused");
        });
        let mut counts: Vec<(String, i64)> = all(&d).iter().map(|r| (format!("{}/{}", r.fields["client"], r.fields["status"]), r.count)).collect();
        counts.sort();
        assert_eq!(counts, [("\"84.1.2.3\"/401".to_string(), 3), ("\"84.1.2.3\"/403".to_string(), 1), ("\"84.1.2.4\"/401".to_string(), 1)]);

        // A minute after the first, a new row.
        let conn = crate::db::open(None).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        let mut recent = HashMap::new();
        let n = |at| Noted { at, level: "warn", area: "signin", place: "server".into(), what: "request refused".into(), fields: Map::new() };
        for at in [0, 30_000, 59_999, 60_000, 61_000] {
            note(&conn, &mut recent, &n(at)).unwrap();
        }
        let rows = rows(&conn, "SELECT id, at, last, count, level, area, place, what, fields, origin FROM events ORDER BY id", &[]).unwrap();
        assert_eq!(rows.iter().map(|r| (r.at, r.last, r.count)).collect::<Vec<_>>(), [(0, 59_999, 3), (60_000, 61_000, 2)]);
    }

    #[test]
    fn old_rows_go_and_the_newest_are_kept_within_the_cap() {
        let conn = crate::db::open(None).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        let mut recent = HashMap::new();
        let day = 24 * 3600 * 1000;
        for (i, at) in [0, day, 15 * day, 16 * day, 17 * day, 18 * day].into_iter().enumerate() {
            let n = Noted { at, level: "info", area: "server", place: "server".into(), what: format!("e{i}"), fields: Map::new() };
            note(&conn, &mut recent, &n).unwrap();
        }
        prune(&conn, 18 * day, KEEP_MS, 3).unwrap();
        let kept = rows(&conn, "SELECT id, at, last, count, level, area, place, what, fields, origin FROM events ORDER BY at", &[]).unwrap();
        assert_eq!(kept.iter().map(|r| r.what.as_str()).collect::<Vec<_>>(), ["e3", "e4", "e5"], "older than 14 days, then beyond 3 rows");
    }

    #[test]
    fn rows_are_found_by_place_area_and_level_newest_first_page_by_page() {
        let d = logged(|| {
            tracing::info!(target: "playon", player = "Phone", "connected");
            tracing::warn!(target: "playon", player = "Phone", reason = "not connected", "command not delivered");
            tracing::info!(target: "library", why = "new songs", "library built");
            tracing::error!(target: "playon", player = "Tablet", "connection failed");
        });
        let q = |place: Option<&str>, area: Option<&str>, level: Option<&str>| {
            let q = Query { place: place.map(Into::into), area: area.map(Into::into), level: level.map(Into::into), ..Default::default() };
            d.query(&q).unwrap().into_iter().map(|r| r.what).collect::<Vec<_>>()
        };
        assert_eq!(q(Some("Phone"), None, None), ["command not delivered", "connected"]);
        assert_eq!(q(None, Some("library"), None), ["library built"]);
        assert_eq!(q(None, None, Some("warn")), ["connection failed", "command not delivered"]);
        assert!(d.query(&Query { level: Some("loud".into()), ..Default::default() }).is_err());
        let first = d.query(&Query { limit: Some(2), ..Default::default() }).unwrap();
        let next = d.query(&Query { limit: Some(2), before: Some(first[1].id), ..Default::default() }).unwrap();
        assert_eq!(first.iter().chain(&next).map(|r| r.what.as_str()).collect::<Vec<_>>(), ["connection failed", "library built", "command not delivered", "connected"]);
    }

    #[test]
    fn a_players_lines_are_added_once_and_their_secrets_left_out() {
        let (_, pending) = layer();
        let d = Diagnostics::start(None, pending).unwrap();
        let line = |seq, what: &str| DeviceLine {
            seq,
            at: 1_000 + i64::try_from(seq).unwrap(),
            level: "WARN".into(),
            area: "playon".into(),
            what: what.into(),
            fields: serde_json::from_value(serde_json::json!({ "address": "public", "session": "s", "player": "x" })).unwrap(),
        };
        assert_eq!(d.add_lines("Phone", "log-1", &[line(1, "connecting"), line(2, "refused: waiting 10 min")]).unwrap(), 2);
        // Sent again with one more: only the new one.
        assert_eq!(d.add_lines("Phone", "log-1", &[line(2, "refused: waiting 10 min"), line(3, "connected"), line(1, "connecting")]).unwrap(), 3);
        assert_eq!(d.last_line("Phone", "log-1").unwrap(), 3);
        // The app's log started over: its own numbers.
        assert_eq!(d.add_lines("Phone", "log-2", &[line(1, "started")]).unwrap(), 1);
        assert_eq!(d.last_line("Tablet", "log-1").unwrap(), 0);
        let rows = d.query(&Query { place: Some("Phone".into()), ..Default::default() }).unwrap();
        assert_eq!(rows.iter().map(|r| r.what.as_str()).collect::<Vec<_>>(), ["connected", "refused: waiting 10 min", "started", "connecting"], "newest first, then the later row");
        assert!(rows.iter().all(|r| r.origin == "device" && r.level == "warn" && r.fields == serde_json::json!({ "address": "public" })));
        assert!(d.add_lines("", "log-1", &[]).is_err());
    }

    #[test]
    fn a_download_masks_each_client_address_with_one_name_throughout() {
        let d = logged(|| {
            for (i, client) in ["192.168.1.100", "84.1.2.3", "192.168.1.100", "84.1.2.3, 10.0.0.2", "unknown", "[fd00::1]:443", "::1"].into_iter().enumerate() {
                tracing::warn!(target: "signin", client, path = %format!("/x/{i}"), "request refused");
            }
            tracing::info!(target: "playon", player = "Phone", "connected");
        });
        let clients: Vec<Value> = d.download().unwrap().into_iter().map(|r| r.fields.get("client").cloned().unwrap_or(Value::Null)).collect();
        assert_eq!(
            clients,
            ["home-1", "outside-2", "home-1", "outside-2, home-3", "unknown", "home-4", "home-5"]
                .into_iter()
                .map(Value::from)
                .chain([Value::Null])
                .collect::<Vec<_>>(),
            "oldest first"
        );
        assert_eq!(all(&d)[1].fields["client"], "::1", "kept as it was in the log itself");
    }

    #[test]
    fn what_is_logged_changes_for_a_while_and_then_is_the_base_again() {
        let set: Arc<Mutex<Vec<String>>> = Arc::default();
        let seen = set.clone();
        let l = Levels::new("info".into(), Box::new(move |f: EnvFilter| {
            seen.lock().unwrap().push(f.to_string());
            Ok(())
        }));
        let now = l.set("playon=debug", Duration::from_millis(100)).unwrap();
        assert_eq!((now.directives.as_str(), now.until.is_some()), ("info,playon=debug", true));
        assert!(l.set("playon=loud", Duration::from_secs(1)).is_err(), "not a filter");
        assert_eq!(l.current().directives, "info,playon=debug", "unchanged by the one that failed");
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(l.current(), LevelsNow { directives: "info".into(), base: "info".into(), until: None });
        // A newer change is not undone by the older one's end.
        l.set("syncplay=debug", Duration::from_millis(100)).unwrap();
        l.set("signin=debug", Duration::from_secs(60)).unwrap();
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(l.current().directives, "info,signin=debug");
        let filters = set.lock().unwrap().clone();
        assert!(filters.iter().all(|f| f.contains("lofty=error")), "{filters:?}");
    }
}
