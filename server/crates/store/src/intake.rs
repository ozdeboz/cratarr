//! DI01 (RN, SCENARIOS-RN R1): the saved queues and the runner's jobs, in the journal's file (`journal.sqlite`, the owner
//! 2026-11-20 13:00: "jobs live in the journal's file, written in one transaction with their journal entries"), on its one
//! connection beside `events`, `handled`, `changes` and `file_runs`. No other SQLite file.
//!
//! - **`intake_items`**: one row per item of a saved queue: the bulk queue's tracks (`bulk`), a page's single searches and
//!   fetches (`search`, `fetch`), later Soulseek's (DC2). Its state and who asked as columns; the rest of its kind's own
//!   fields as one document.
//! - **`intake_steps`**: each step of an item (`search`, `fetch`, `check`, a check's file `check:<n>`): its state, tries,
//!   failure class, why, next time, batch, and a checked file's size, time, verdict and notes. A step's next time and its
//!   tries are kept here and nowhere else (R5).
//! - **`source_pauses`**: a source's hold, until when and why (R4: where TN kept `<source>:held_until` in the user data).
//! - **`jobs`**: the runner's rows: a job's kind, key, whose, state, when it started and ended, its error and result. Its
//!   progress stays in memory (R7).
//! - **`intake_marks`**: a queue's next id, and the 0.18 step's parts' marks.
//!
//! A write is one transaction (`Journal::write`): a step that journals commits its row and its entry's last write together
//! (R2), or neither.

use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, params};

use crate::journal::Journal;

/// The tables, made when missing: safe to run at every open.
#[allow(dead_code)] // red: not made yet
pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS intake_items (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL, seq INTEGER NOT NULL, state TEXT NOT NULL, asked_by TEXT, item TEXT NOT NULL) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS intake_steps (
  item TEXT NOT NULL, step TEXT NOT NULL, state TEXT NOT NULL, tries INTEGER NOT NULL DEFAULT 0, failure TEXT, why TEXT,
  next_at INTEGER NOT NULL DEFAULT 0, batch TEXT, file TEXT, size INTEGER, modified INTEGER, verdict TEXT, notes TEXT,
  PRIMARY KEY (item, step)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS source_pauses (
  source TEXT PRIMARY KEY, until INTEGER NOT NULL, why TEXT, own INTEGER NOT NULL DEFAULT 0) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS jobs (
  id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, key TEXT, account TEXT, preview INTEGER NOT NULL DEFAULT 0,
  state TEXT NOT NULL, started INTEGER NOT NULL, finished INTEGER, error TEXT, result TEXT);
CREATE TABLE IF NOT EXISTS intake_marks (name TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
";

/// At an open (a start): a check's file being heard when the server stopped reads back cut off (OQ-RN-3 (a): heard whole again);
/// every other step as it was kept (a fetch is slskd's, followed again by its batch).
#[allow(dead_code)] // red: not made yet
pub(crate) const AT_OPEN: &str = "UPDATE intake_steps SET state = 'cut_off' WHERE state = 'running' AND step LIKE 'check:%';";

/// An item of a saved queue.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemRow {
    pub id: String,
    /// Which queue: `bulk`, `search`, `fetch` (and DC2's).
    pub kind: String,
    /// Its place in its queue (the order it came in).
    pub seq: i64,
    pub state: String,
    /// Whose (decision 0051); none for Cratarr's own, or where 0.17 kept none.
    pub asked_by: Option<String>,
    /// Its kind's own fields.
    pub fields: serde_json::Value,
}

/// A step of an item.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StepRow {
    /// `search`, `fetch`, `check`, `check:<n>` (a check's file).
    pub step: String,
    /// `to_do`, `running`, `held`, `done`, `failed`, `cut_off`.
    pub state: String,
    pub tries: u32,
    /// The failure class (TN.1) it met last: `paused`, `away`, `user`, `file`, `not_found`, `stuck`, `other`.
    pub failure: Option<String>,
    pub why: Option<String>,
    /// When it is taken up (epoch s; 0: now).
    pub next_at: i64,
    /// slskd's batch it is fetched under.
    pub batch: Option<String>,
    /// A check's file: its name, its size and time (ms) as heard, its verdict and notes.
    pub file: Option<String>,
    pub size: Option<u64>,
    pub modified: Option<i64>,
    pub verdict: Option<String>,
    pub notes: Option<String>,
}

/// A source's hold (TN): until when (epoch s), why, and whether it is a deadline of the source's own (Bandcamp's day).
#[derive(Debug, Clone, PartialEq)]
pub struct Pause {
    pub source: String,
    pub until: i64,
    pub why: Option<String>,
    pub own: bool,
}

/// A runner's job as kept.
#[derive(Debug, Clone, PartialEq)]
pub struct JobRow {
    pub id: i64,
    pub kind: String,
    /// Identical jobs are one: the same kind and key.
    pub key: Option<String>,
    pub account: Option<String>,
    pub preview: bool,
    /// `running`, `done`, `failed`, `cancelled`.
    pub state: String,
    pub started: i64,
    pub finished: Option<i64>,
    pub error: Option<String>,
    pub result: Option<String>,
}

/// The journal's file inside one transaction (`Journal::write`).
pub struct Tx<'a>(&'a rusqlite::Transaction<'a>);

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    // A test's cut (SCENARIOS-RN DS5, DM2): the next write on this thread stops before its commit, as a stop would.
    static CUT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Tests: the next write on this thread is cut off before its commit (a stop inside its transaction): nothing of it is kept.
#[cfg(any(test, feature = "test-support"))]
pub fn cut_next_write() {
    CUT.set(true);
}

impl Journal {
    /// `f`'s writes, as one transaction on the journal's file: all kept, or none (R2).
    pub fn write<T>(&self, f: impl FnOnce(&Tx) -> Result<T>) -> Result<T> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let out = f(&Tx(&tx))?;
        #[cfg(any(test, feature = "test-support"))]
        if CUT.replace(false) {
            anyhow::bail!("cut off before its commit");
        }
        tx.commit()?;
        Ok(out)
    }

    /// A queue's items, in its order, each with its steps; and its next id.
    pub fn intake(&self, kind: &str) -> Result<(u64, Vec<(ItemRow, Vec<StepRow>)>)> {
        self.write(|tx| tx.intake(kind))
    }

    /// One item and its steps.
    pub fn intake_item(&self, id: &str) -> Result<Option<(ItemRow, Vec<StepRow>)>> {
        self.write(|tx| tx.intake_item(id))
    }

    /// A source's kept hold.
    pub fn pause(&self, source: &str) -> Result<Option<Pause>> {
        self.write(|tx| tx.pause(source))
    }

    pub fn pauses(&self) -> Result<Vec<Pause>> {
        self.write(|tx| tx.pauses())
    }

    /// A hold kept where it ends later than the one kept (a hold only grows): whether it was.
    pub fn keep_pause(&self, p: &Pause) -> Result<bool> {
        self.write(|tx| tx.keep_pause(p))
    }

    /// A hold kept as given (one cut to its cap as it is taken up).
    pub fn set_pause(&self, p: &Pause) -> Result<()> {
        self.write(|tx| tx.set_pause(p))
    }

    /// A source's hold lifted.
    pub fn lift_pause(&self, source: &str) -> Result<()> {
        self.write(|tx| tx.lift_pause(source))
    }

    /// A job begun, kept: its id.
    pub fn begin_job(&self, row: &JobRow) -> Result<i64> {
        self.write(|tx| tx.begin_job(row))
    }

    /// A job ended, kept.
    pub fn end_job(&self, row: &JobRow) -> Result<()> {
        self.write(|tx| tx.end_job(row))
    }

    pub fn jobs(&self) -> Result<Vec<JobRow>> {
        self.write(|tx| tx.jobs())
    }

    /// The jobs that ended before `before` (epoch s) forgotten; those running never. How many.
    pub fn prune_jobs(&self, before: i64) -> Result<usize> {
        self.write(|tx| Ok(tx.0.execute("DELETE FROM jobs WHERE finished IS NOT NULL AND finished < ?1", [before])?))
    }

    /// A mark (a queue's next id, a step part's report).
    pub fn mark(&self, name: &str) -> Result<Option<String>> {
        self.write(|tx| tx.mark(name))
    }

    /// A check's count from its rows (DQ1): its files checked, of its files.
    pub fn check_count(&self, item: &str) -> Result<(u32, u32)> {
        self.write(|tx| tx.check_count(item))
    }
}

impl Tx<'_> {
    /// Items kept with their steps (each item's steps as given, those it no longer has gone), and `gone` forgotten with theirs; a
    /// queue's next id with them where given.
    pub fn keep_items(&self, kind: &str, next: Option<u64>, items: &[(ItemRow, Vec<StepRow>)], gone: &[String]) -> Result<()> {
        let c = self.0;
        for id in gone {
            c.execute("DELETE FROM intake_steps WHERE item = ?1", [id])?;
            c.execute("DELETE FROM intake_items WHERE id = ?1", [id])?;
        }
        for (item, steps) in items {
            anyhow::ensure!(item.kind == kind, "an item of another queue");
            c.execute(
                "INSERT INTO intake_items (id, kind, seq, state, asked_by, item) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET kind = ?2, seq = ?3, state = ?4, asked_by = ?5, item = ?6",
                params![item.id, item.kind, item.seq, item.state, item.asked_by, item.fields.to_string()],
            )?;
            let names: Vec<&str> = steps.iter().map(|s| s.step.as_str()).collect();
            let kept: Vec<String> =
                c.prepare("SELECT step FROM intake_steps WHERE item = ?1")?.query_map([&item.id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            for step in kept.iter().filter(|s| !names.contains(&s.as_str())) {
                c.execute("DELETE FROM intake_steps WHERE item = ?1 AND step = ?2", params![item.id, step])?;
            }
            for s in steps {
                c.execute(
                    "INSERT INTO intake_steps (item, step, state, tries, failure, why, next_at, batch, file, size, modified, verdict, notes)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                     ON CONFLICT(item, step) DO UPDATE SET state = ?3, tries = ?4, failure = ?5, why = ?6, next_at = ?7, batch = ?8,
                       file = ?9, size = ?10, modified = ?11, verdict = ?12, notes = ?13",
                    params![item.id, s.step, s.state, s.tries, s.failure, s.why, s.next_at, s.batch, s.file, s.size.map(|n| n as i64), s.modified, s.verdict, s.notes],
                )?;
            }
        }
        if let Some(next) = next {
            self.set_mark(&format!("next:{kind}"), &next.to_string())?;
        }
        Ok(())
    }

    /// A queue's items, in its order, each with its steps; and its next id (one past the highest kept where none was).
    pub fn intake(&self, kind: &str) -> Result<(u64, Vec<(ItemRow, Vec<StepRow>)>)> {
        let ids: Vec<String> =
            self.0.prepare("SELECT id FROM intake_items WHERE kind = ?1 ORDER BY seq, id")?.query_map([kind], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        let mut items = Vec::with_capacity(ids.len());
        for id in ids {
            items.extend(self.intake_item(&id)?);
        }
        let next = match self.mark(&format!("next:{kind}"))? {
            Some(n) => n.parse().context("a queue's next id")?,
            None => items.iter().map(|(i, _)| i.seq.max(0) as u64).max().unwrap_or(0),
        };
        Ok((next, items))
    }

    pub fn intake_item(&self, id: &str) -> Result<Option<(ItemRow, Vec<StepRow>)>> {
        let item = self
            .0
            .query_row("SELECT id, kind, seq, state, asked_by, item FROM intake_items WHERE id = ?1", [id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, String>(5)?))
            })
            .optional()?;
        let Some((id, kind, seq, state, asked_by, fields)) = item else { return Ok(None) };
        // The steps in the order they come: search, fetch, check, then a check's files by number.
        let mut st = self.0.prepare(
            "SELECT step, state, tries, failure, why, next_at, batch, file, size, modified, verdict, notes FROM intake_steps WHERE item = ?1
             ORDER BY CASE WHEN step LIKE 'check:%' THEN 1000000 + CAST(substr(step, 7) AS INTEGER)
               WHEN step = 'search' THEN 0 WHEN step = 'fetch' THEN 1 WHEN step = 'check' THEN 2 ELSE 3 END, step",
        )?;
        let steps = st
            .query_map([&id], |r| {
                Ok(StepRow {
                    step: r.get(0)?,
                    state: r.get(1)?,
                    tries: r.get(2)?,
                    failure: r.get(3)?,
                    why: r.get(4)?,
                    next_at: r.get(5)?,
                    batch: r.get(6)?,
                    file: r.get(7)?,
                    size: r.get::<_, Option<i64>>(8)?.map(|n| n as u64),
                    modified: r.get(9)?,
                    verdict: r.get(10)?,
                    notes: r.get(11)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Some((ItemRow { id, kind, seq, state, asked_by, fields: serde_json::from_str(&fields).context("an intake item")? }, steps)))
    }

    pub fn pause(&self, source: &str) -> Result<Option<Pause>> {
        Ok(self
            .0
            .query_row("SELECT source, until, why, own FROM source_pauses WHERE source = ?1", [source], |r| {
                Ok(Pause { source: r.get(0)?, until: r.get(1)?, why: r.get(2)?, own: r.get::<_, i64>(3)? != 0 })
            })
            .optional()?)
    }

    pub fn pauses(&self) -> Result<Vec<Pause>> {
        let mut st = self.0.prepare("SELECT source, until, why, own FROM source_pauses ORDER BY source")?;
        let rows = st.query_map([], |r| Ok(Pause { source: r.get(0)?, until: r.get(1)?, why: r.get(2)?, own: r.get::<_, i64>(3)? != 0 }))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A hold kept where it ends later than the one kept (a hold only grows): whether it was.
    pub fn keep_pause(&self, p: &Pause) -> Result<bool> {
        if self.pause(&p.source)?.is_some_and(|was| was.until >= p.until) {
            return Ok(false);
        }
        self.set_pause(p)?;
        Ok(true)
    }

    pub fn set_pause(&self, p: &Pause) -> Result<()> {
        self.0.execute(
            "INSERT INTO source_pauses (source, until, why, own) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(source) DO UPDATE SET until = ?2, why = ?3, own = ?4",
            params![p.source, p.until, p.why, i64::from(p.own)],
        )?;
        Ok(())
    }

    pub fn lift_pause(&self, source: &str) -> Result<()> {
        self.0.execute("DELETE FROM source_pauses WHERE source = ?1", [source])?;
        Ok(())
    }

    /// A job kept under the runner's id for it (its ids continue past those kept: `Runner::attach`), or a new one where it is 0.
    pub fn begin_job(&self, row: &JobRow) -> Result<i64> {
        self.0.execute(
            "INSERT INTO jobs (id, kind, key, account, preview, state, started, finished, error, result) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![(row.id > 0).then_some(row.id), row.kind, row.key, row.account, i64::from(row.preview), row.state, row.started, row.finished, row.error, row.result],
        )?;
        Ok(self.0.last_insert_rowid())
    }

    pub fn end_job(&self, row: &JobRow) -> Result<()> {
        self.0.execute(
            "UPDATE jobs SET state = ?2, finished = ?3, error = ?4, result = ?5 WHERE id = ?1",
            params![row.id, row.state, row.finished, row.error, row.result],
        )?;
        Ok(())
    }

    pub fn jobs(&self) -> Result<Vec<JobRow>> {
        let mut st = self.0.prepare("SELECT id, kind, key, account, preview, state, started, finished, error, result FROM jobs ORDER BY id")?;
        let rows = st.query_map([], |r| {
            Ok(JobRow {
                id: r.get(0)?,
                kind: r.get(1)?,
                key: r.get(2)?,
                account: r.get(3)?,
                preview: r.get::<_, i64>(4)? != 0,
                state: r.get(5)?,
                started: r.get(6)?,
                finished: r.get(7)?,
                error: r.get(8)?,
                result: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn mark(&self, name: &str) -> Result<Option<String>> {
        Ok(self.0.query_row("SELECT value FROM intake_marks WHERE name = ?1", [name], |r| r.get(0)).optional()?)
    }

    pub fn set_mark(&self, name: &str, value: &str) -> Result<()> {
        self.0.execute("INSERT INTO intake_marks (name, value) VALUES (?1, ?2) ON CONFLICT(name) DO UPDATE SET value = ?2", params![name, value])?;
        Ok(())
    }

    /// A check's count from its rows: its files checked (done), of its files.
    pub fn check_count(&self, item: &str) -> Result<(u32, u32)> {
        Ok(self.0.query_row(
            "SELECT count(CASE WHEN state = 'done' THEN 1 END), count(*) FROM intake_steps WHERE item = ?1 AND step LIKE 'check:%'",
            [item],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    }

    /// A file run kept inside this transaction (decision 0092; R2: with the rows of the step that made it): its id.
    pub fn record_run(&self, at: i64, run: &crate::journal::FileRun, account: Option<&str>, keep: i64) -> Result<crate::journal::EventId> {
        Journal::keep_run(self.0, at, run, account, keep)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_800_000_000;

    fn folder(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-intake-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A 0.17 journal as 0.17's schema made it: 40 events (5 not applied), 12 handled, 30 changes, 8 file runs (one still planned).
    fn journal_017(file: &std::path::Path) {
        let c = rusqlite::Connection::open(file).unwrap();
        c.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE events (id INTEGER PRIMARY KEY AUTOINCREMENT, at INTEGER NOT NULL, event TEXT NOT NULL, undone_at INTEGER,
               applied INTEGER NOT NULL DEFAULT 1, account TEXT, undone_by TEXT);
             CREATE TABLE handled (event INTEGER NOT NULL, handler TEXT NOT NULL, at INTEGER NOT NULL, result TEXT NOT NULL,
               PRIMARY KEY (event, handler)) WITHOUT ROWID;
             CREATE TABLE changes (id INTEGER PRIMARY KEY AUTOINCREMENT, at INTEGER NOT NULL, change TEXT NOT NULL, undone_at INTEGER, account TEXT);
             CREATE TABLE file_runs (id INTEGER PRIMARY KEY AUTOINCREMENT, at INTEGER NOT NULL, run TEXT NOT NULL, account TEXT, undone_at INTEGER);",
        )
        .unwrap();
        let event = |n: i64| {
            serde_json::json!({ "kind": "album_rejected", "source": "cratarr", "album_id": format!("al-{n}"), "title": format!("Record {n}"),
                "folder": 1, "dir": format!("A/{n}"), "token": format!("t{n}"), "release_group": null })
            .to_string()
        };
        for n in 1..=40 {
            c.execute("INSERT INTO events (at, event, applied, account) VALUES (?1, ?2, ?3, 'root')", params![T0 + n, event(n), i64::from(n > 5)]).unwrap();
        }
        for n in 6..=17 {
            c.execute("INSERT INTO handled (event, handler, at, result) VALUES (?1, 'playlists', ?2, '[]')", params![n, T0 + n]).unwrap();
        }
        for n in 1..=30 {
            let change = serde_json::json!({ "kind": "renamed", "from": format!("a{n}"), "to": format!("b{n}") }).to_string();
            c.execute("INSERT INTO changes (at, change, account) VALUES (?1, ?2, 'alice')", params![T0 + n, change]).unwrap();
        }
        for n in 1..=8 {
            let run = serde_json::json!({ "what": "types", "title": format!("Run {n}"), "records": [], "files": [], "failed": [],
                "planned": if n == 8 { vec!["al-8"] } else { Vec::new() } })
            .to_string();
            c.execute("INSERT INTO file_runs (at, run, account) VALUES (?1, ?2, 'root')", params![T0 + n, run]).unwrap();
        }
    }

    /// Every row of the journal's own tables, as text, by table.
    fn held(file: &std::path::Path) -> Vec<(String, Vec<String>)> {
        let c = rusqlite::Connection::open(file).unwrap();
        ["events", "handled", "changes", "file_runs"]
            .iter()
            .map(|t| {
                let cols: Vec<String> = c.prepare(&format!("SELECT name FROM pragma_table_info('{t}') ORDER BY cid")).unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
                let all = cols.iter().map(|c| format!("quote({c})")).collect::<Vec<_>>().join(" || '|' || ");
                let rows = c.prepare(&format!("SELECT {all} FROM {t} ORDER BY 1")).unwrap().query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).collect();
                (t.to_string(), rows)
            })
            .collect()
    }

    fn schema(file: &std::path::Path) -> Vec<String> {
        let c = rusqlite::Connection::open(file).unwrap();
        c.prepare("SELECT sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY name").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }

    /// DS1: a 0.17 `journal.sqlite` gains the queues' tables and the jobs' in the same file, keeps every row it held column for
    /// column, reads as before, and a second open changes nothing. Never a second SQLite file, nor an earlier row rewritten.
    #[test]
    fn the_journal_gains_the_queues_and_keeps_what_it_held() {
        let d = folder("ds1");
        let file = d.join("journal.sqlite");
        journal_017(&file);
        let before = held(&file);
        let j = Journal::open(Some(&file)).unwrap();
        let tables: Vec<String> = schema(&file);
        for t in ["intake_items", "intake_steps", "source_pauses", "jobs"] {
            assert!(tables.iter().any(|s| s.starts_with(&format!("CREATE TABLE {t} ("))), "{t} in the journal's file");
        }
        assert_eq!(held(&file), before, "every earlier row as it was, column for column");
        assert_eq!(j.intents().unwrap().len(), 5, "the five not applied read as before");
        assert_eq!(j.file_runs_since(0).unwrap().len(), 8);
        assert_eq!(j.file_runs_since(0).unwrap().iter().filter(|r| !r.2.planned.is_empty()).count(), 1, "the planned one still planned");
        assert_eq!(j.change(30).unwrap().map(|c| c.2), Some(Some("alice".into())));
        drop(j);
        let (first, rows) = (schema(&file), held(&file));
        let j = Journal::open(Some(&file)).unwrap();
        assert_eq!((schema(&file), held(&file)), (first, rows), "a second open changes nothing");
        drop(j);
        let mut files: Vec<String> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        files.sort();
        assert!(files.iter().all(|f| f.starts_with("journal.sqlite")), "no second SQLite file: {files:?}");
        std::fs::remove_dir_all(&d).ok();
    }

    /// ti-1 of `al-42`, asked by root, with its three steps.
    fn ti_1() -> (ItemRow, Vec<StepRow>) {
        let item = ItemRow {
            id: "ti-1".into(),
            kind: "bulk".into(),
            seq: 1,
            state: "fetching".into(),
            asked_by: Some("root".into()),
            fields: serde_json::json!({ "album": { "source": "cratarr", "id": "al-42", "title": "Night Ferry", "artist": "Glass Harbour" },
                "disc": 1, "position": 3, "title": "Neap", "recording": "rec-neap", "chosen": 0 }),
        };
        let steps = vec![
            StepRow { step: "search".into(), state: "done".into(), tries: 1, ..Default::default() },
            StepRow { step: "fetch".into(), state: "running".into(), batch: Some("b-1".into()), failure: Some("away".into()), why: Some("slskd could not be reached".into()), next_at: T0 + 300, ..Default::default() },
            StepRow { step: "check".into(), state: "to_do".into(), ..Default::default() },
        ];
        (item, steps)
    }

    /// DS2 (DI01's `a_step_reads_back_after_reopening`): an item and its three steps read back field for field after the journal is
    /// closed and opened again; the next id continues. Never a percent kept, a step without its item, or a field read as its default.
    #[test]
    fn a_step_reads_back_after_reopening() {
        let d = folder("ds2");
        let file = d.join("journal.sqlite");
        let given = ti_1();
        {
            let j = Journal::open(Some(&file)).unwrap();
            j.write(|tx| tx.keep_items("bulk", Some(1), std::slice::from_ref(&given), &[])).unwrap();
        }
        let j = Journal::open(Some(&file)).unwrap();
        let (next, items) = j.intake("bulk").unwrap();
        assert_eq!(items, vec![given.clone()], "field for field");
        assert_eq!(next, 1);
        assert_eq!(items[0].1[1].next_at, T0 + 300, "its next time, not the default");
        let (second, mut steps) = ti_1();
        let second = ItemRow { id: "ti-2".into(), seq: 2, ..second };
        steps.truncate(1);
        j.write(|tx| tx.keep_items("bulk", Some(2), &[(second, steps)], &["ti-1".to_string()])).unwrap();
        let (next, items) = j.intake("bulk").unwrap();
        assert_eq!((next, items.len(), items[0].0.id.as_str()), (2, 1, "ti-2"), "the next id continues past one gone");
        let orphans: i64 = j.db().query_row("SELECT count(*) FROM intake_steps WHERE item NOT IN (SELECT id FROM intake_items)", [], |r| r.get(0)).unwrap();
        assert_eq!(orphans, 0, "no step without its item");
        let columns: Vec<String> = j.db().prepare("SELECT name FROM pragma_table_info('intake_steps')").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert!(!columns.iter().any(|c| c.contains("percent")), "no percent kept (R7): {columns:?}");
        drop(j);
        std::fs::remove_dir_all(&d).ok();
    }

    /// DS3, the store's half: a hold only grows, is lifted by its row, and reads back after a reopen.
    #[test]
    fn a_hold_only_grows_and_reads_back() {
        let d = folder("ds3");
        let file = d.join("journal.sqlite");
        let p = |source: &str, until: i64, own: bool| Pause { source: source.into(), until, why: Some(format!("{source} asked Cratarr to wait")), own };
        {
            let j = Journal::open(Some(&file)).unwrap();
            assert!(j.keep_pause(&p("acoustid", T0 + 600, false)).unwrap());
            assert!(!j.keep_pause(&p("acoustid", T0 + 100, false)).unwrap(), "a shorter one later leaves the longer");
            assert!(j.keep_pause(&p("bandcamp", T0 + 86_400, true)).unwrap());
            j.lift_pause("bandcamp").unwrap();
            assert!(j.keep_pause(&p("musicbrainz", T0 + 30, false)).unwrap());
        }
        let j = Journal::open(Some(&file)).unwrap();
        assert_eq!(j.pauses().unwrap(), vec![p("acoustid", T0 + 600, false), p("musicbrainz", T0 + 30, false)]);
        std::fs::remove_dir_all(&d).ok();
    }

    /// DQ1: a check kept per file: four passed (the third with its notes), the fifth being heard, four to do; after a reopen nine
    /// steps each naming its file, the fifth cut off, "4 of 9" from the rows, each verdict with its file's size and time.
    #[test]
    fn a_checks_steps_read_back_per_file() {
        let d = folder("dq1");
        let file = d.join("journal.sqlite");
        let item = ItemRow { id: "ss-11".into(), kind: "soulseek".into(), seq: 11, state: "checking".into(), asked_by: None, fields: serde_json::json!({ "record": 11 }) };
        let step = |n: u32, state: &str| StepRow {
            step: format!("check:{n}"),
            state: state.into(),
            file: Some(format!("Glass Harbour - Tide Tables/0{n} Track.flac")),
            size: (state == "done").then_some(30_000_000 + u64::from(n)),
            modified: (state == "done").then_some(T0 * 1000 + i64::from(n)),
            verdict: (state == "done").then(|| "passed".to_string()),
            notes: (n == 3).then(|| "Clipped at 1:46".to_string()),
            ..Default::default()
        };
        let steps: Vec<StepRow> = (1..=9).map(|n| step(n, if n <= 4 { "done" } else if n == 5 { "running" } else { "to_do" })).collect();
        {
            let j = Journal::open(Some(&file)).unwrap();
            j.write(|tx| tx.keep_items("soulseek", None, &[(item.clone(), steps.clone())], &[])).unwrap();
        }
        let j = Journal::open(Some(&file)).unwrap();
        let (_, back) = j.intake_item("ss-11").unwrap().unwrap();
        assert_eq!(back.len(), 9, "nine steps, one a file");
        assert!(back.iter().enumerate().all(|(n, s)| s.file.as_deref() == Some(&*format!("Glass Harbour - Tide Tables/0{} Track.flac", n + 1))), "each naming its file, in order");
        assert_eq!(back[4].state, "cut_off", "the file being heard reads back cut off");
        assert_eq!(back.iter().filter(|s| s.state == "to_do").count(), 4);
        assert_eq!(back[2].notes.as_deref(), Some("Clipped at 1:46"), "notes with their file");
        assert!(back[..4].iter().all(|s| s.verdict.is_some() && s.size.is_some() && s.modified.is_some()), "each verdict with its file's size and time");
        assert_eq!(j.check_count("ss-11").unwrap(), (4, 9), "4 of 9 from the rows");
        std::fs::remove_dir_all(&d).ok();
    }
}
