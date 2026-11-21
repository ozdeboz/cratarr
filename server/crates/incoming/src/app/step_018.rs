//! 0.18's step, RN's parts (DI06; SCENARIOS-RN DM1-DM5): what 0.17 kept in files of its own moved once onto the journal's rows, at
//! the first start of 0.18, before the turns take up their holds and before anything reads the queue.
//!
//! - **`queues`**: the bulk queue (`track-intake.json`) onto DI01's `bulk` items and their steps, by the table (DM1): each state as it
//!   was, but Searching (its search was in memory: Queued) and Fetching or Checking (back to their tick, fetched again under a new batch,
//!   R9); candidates and ticks kept, a held item's due and why kept, the next id kept. The file is then renamed
//!   `track-intake.json.017`, kept one version and read by nothing (DM3). One that cannot be read is set aside whole as
//!   `track-intake.json.unreadable-<time>`, said in the log as an error and on Health while it lies there, and the start goes on (DM4:
//!   0.17 read it as an empty queue and replaced it at its next write).
//! - **`pauses`**: the holds 0.17 kept in the user data's server choices (`<source>:held_until`) onto `source_pauses`, each cut to its
//!   source's cap, a past one dropped (DM5); the choices cleared after the rows' commit, at the next start where a stop came between.
//! - **`floor`** (PL4 (c)): an install upgrading from 0.17 with no floor saved keeps 0.17's 200 GB, saved, so the new default (a tenth
//!   of the smallest disk the floor reads) changes no floor at the upgrade.
//! - **Each part once, safe to repeat** (DM2): one transaction whose last write is its mark (`step_018:<part>` in the journal's
//!   `intake_marks`, holding the part's report), so a part cut off wrote nothing and runs whole at the next start; what comes after
//!   its commit (the rename, the choices cleared, the migration report) is done again at every start until it is. The counts go into
//!   the migration report (`/api/admin/migration`). Soulseek's `soulseek.json` is DC2's (resolution 6): neither read nor written here.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};

use super::track_intake::{Item, KIND, State};
use crate::intake::Pause;
use crate::journal::Journal;
use crate::slskd::{Candidate, Offered};
use crate::userdata::UserData;

/// The parts' marks, and their parts of the migration report.
pub const QUEUES: &str = "step_018:queues";
pub const PAUSES: &str = "step_018:pauses";
pub const FLOOR: &str = "step_018:floor";
/// The migration report's parts, as `/api/admin/migration` lists them.
pub const QUEUES_PART: &str = "bulk queue";
pub const PAUSES_PART: &str = "sources' holds";
pub const FLOOR_PART: &str = "space floor";

/// 0.17's bulk queue, its name once moved, and what one that cannot be read is set aside as (before its time).
pub const FILE: &str = "track-intake.json";
pub const KEPT: &str = "track-intake.json.017";
pub const UNREADABLE: &str = "track-intake.json.unreadable-";

/// The step at a start, before the turns take up their holds (`turn::keep_all`): its parts each done before, or now. `data`: the
/// data folder (none: nothing kept in files); `upgrading`: an install coming from 0.17 (its settings file from before 0.18). A part
/// that fails stops the start, naming itself.
pub fn run(journal: &Journal, data: Option<&Path>, ud: &UserData, now: i64, upgrading: bool) -> Result<()> {
    if let Some(d) = data {
        queues(journal, d, ud, now, &|| Ok(())).context("0.18's step, part queues")?;
    }
    pauses(journal, ud, now, &|| Ok(())).context("0.18's step, part pauses")?;
    floor(journal, ud, upgrading).context("0.18's step, part floor")?;
    Ok(())
}

/// A 0.17 item as the file kept it: its candidates whole (0.17 wrote them, and read them back never).
#[derive(serde::Deserialize)]
struct Kept017 {
    #[serde(flatten)]
    item: Item,
    #[serde(default)]
    candidates: Vec<Candidate>,
}

#[derive(serde::Deserialize)]
struct Queue017 {
    #[serde(default)]
    next: u64,
    #[serde(default)]
    items: Vec<serde_json::Value>,
}

/// A candidate's file as offered, made again from what 0.17 kept of it (its file, size, format and length).
fn offered_of(c: &Candidate) -> Offered {
    Offered {
        filename: c.file.clone(),
        size: c.size,
        extension: Some(c.format.clone()),
        bit_rate: c.bitrate,
        bit_depth: c.bit_depth,
        sample_rate: c.sample_rate,
        length: c.length_ms.map(|ms| ms / 1000),
        is_locked: false,
    }
}

/// The table (DM1): a 0.17 item as 0.18 keeps it, and the line of the report it counts on.
fn moved(raw: &serde_json::Value) -> Result<(Item, &'static str)> {
    let k: Kept017 = serde_json::from_value(raw.clone()).context("an item of the bulk queue")?;
    let mut i = k.item;
    i.candidates = k.candidates.into_iter().map(|c| Candidate { offered: offered_of(&c), ..c }).collect();
    let line = match i.state {
        State::Queued => "queued",
        State::Searching => {
            i.state = State::Queued;
            "searched again"
        }
        State::Found => "found",
        State::None => "not found",
        State::Failed => "failed",
        State::Approved => "to fetch",
        State::Fetching | State::Checking => {
            // Back to its tick, fetched again under a new batch (R9); a hold's due and why kept with it.
            i.state = State::Approved;
            "back to its tick"
        }
        State::Added => "added",
        State::Refused => "refused",
    };
    // Held for its source: its class as its words say it (0.17 kept the words alone).
    if let Some(why) = i.why.as_deref().filter(|_| i.due > 0 && matches!(line, "back to its tick" | "queued" | "to fetch" | "searched again")) {
        i.failure = Some(if why.contains("asked Cratarr to wait") { "paused" } else { "away" }.into());
    }
    Ok((i, line))
}

/// The report's words, as `/api/admin/migration` shows them: "Bulk queue: 12 moved (2 queued, …); 0 dropped".
pub fn said(counts: &BTreeMap<String, u64>, dropped: u64) -> String {
    const ORDER: [&str; 9] = ["queued", "searched again", "found", "not found", "failed", "to fetch", "back to its tick", "added", "refused"];
    let moved: u64 = counts.values().sum();
    let parts: Vec<String> = ORDER.iter().filter_map(|k| counts.get(*k).filter(|n| **n > 0).map(|n| format!("{n} {k}"))).collect();
    format!("Bulk queue: {moved} moved ({}); {dropped} dropped", parts.join(", "))
}

/// The part `queues`. `after`: heard once its rows and mark are committed, before the file is renamed (what the tests cut it by).
/// Blocking.
pub fn queues(journal: &Journal, data: &Path, ud: &UserData, now: i64, after: &dyn Fn() -> Result<()>) -> Result<()> {
    let file = data.join(FILE);
    if let Some(report) = journal.mark(QUEUES)? {
        // Done before: the rename and the report again, where a stop came first.
        return finish(&file, &data.join(KEPT), ud, &serde_json::from_str(&report)?);
    }
    let Ok(bytes) = std::fs::read(&file) else { return Ok(()) };
    tracing::info!(target: "library", "0.18's step: the bulk queue onto the journal's rows");
    let queue: Queue017 = match serde_json::from_slice(&bytes) {
        Ok(q) => q,
        Err(e) => return set_aside(&file, data, ud, journal, now, &format!("{e}")),
    };
    let mut items = Vec::new();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut states_017: BTreeMap<String, u64> = BTreeMap::new();
    let mut dropped = Vec::new();
    for raw in &queue.items {
        *states_017.entry(raw["state"].as_str().unwrap_or("?").to_string()).or_default() += 1;
        match moved(raw) {
            Ok((i, line)) => {
                *counts.entry(line.to_string()).or_default() += 1;
                items.push(i);
            }
            Err(e) => dropped.push(serde_json::json!({ "item": raw.get("id"), "why": format!("{e:#}") })),
        }
    }
    let mut states_018: BTreeMap<String, u64> = BTreeMap::new();
    for i in &items {
        *states_018.entry(serde_json::to_value(i.state)?.as_str().unwrap_or_default().to_string()).or_default() += 1;
    }
    let next = queue.next.max(items.iter().map(|i| super::track_intake::seq_of(&i.id).max(0) as u64).max().unwrap_or(0));
    let report = serde_json::json!({
        "said": said(&counts, dropped.len() as u64),
        "moved": items.len(),
        "by_line": counts,
        "before": states_017,
        "after": states_018,
        "next": next,
        "dropped": dropped,
        "file": KEPT,
    });
    let rows: Vec<_> = items.iter().map(|i| super::track_intake::rows_of(i, super::track_intake::seq_of(&i.id))).collect();
    journal.write(|tx| {
        tx.keep_items(KIND, Some(next), &rows, &[])?;
        // Its last write: its mark, holding its report.
        tx.set_mark(QUEUES, &report.to_string())
    })?;
    after()?;
    finish(&file, &data.join(KEPT), ud, &report)
}

/// What follows a part's commit: the file renamed (kept one version, byte for byte, read by nothing), the report kept.
fn finish(file: &Path, kept: &Path, ud: &UserData, report: &serde_json::Value) -> Result<()> {
    if file.exists() && !kept.exists() {
        std::fs::rename(file, kept).with_context(|| format!("renaming {} to {}", file.display(), kept.display()))?;
    }
    if ud.migration_report()?.get(QUEUES_PART).is_none_or(|(_, r)| r != report) {
        ud.set_migration_part(QUEUES_PART, report)?;
    }
    Ok(())
}

/// A queue that cannot be read (DM4): set aside whole under its time, said in the log as an error and on Health while it lies there
/// (`queues_set_aside`), its part of the report naming it; no rows, no mark: put back as `track-intake.json`, it is tried again at the
/// next start; the start goes on.
fn set_aside(file: &Path, data: &Path, ud: &UserData, journal: &Journal, now: i64, why: &str) -> Result<()> {
    let aside = data.join(format!("{UNREADABLE}{now}"));
    std::fs::rename(file, &aside).with_context(|| format!("setting {} aside", file.display()))?;
    tracing::error!(target: "library", file = %file.display(), aside = %aside.display(), reason = why, "the bulk queue could not be read: set aside whole, nothing moved");
    let report = serde_json::json!({ "said": format!("Bulk queue: could not be read; set aside as {}", aside.file_name().unwrap_or_default().to_string_lossy()), "moved": 0, "set_aside": aside.file_name().map(|n| n.to_string_lossy().into_owned()), "why": why });
    let _ = journal;
    ud.set_migration_part(QUEUES_PART, &report)
}

/// The bulk queues set aside in the data folder `dir` (DM4), as it holds them now: each file's name and when it was set aside, the
/// latest first. Health's "The bulk queue was set aside" while any is there.
pub fn queues_set_aside(dir: &Path) -> Vec<(String, Option<i64>)> {
    let mut out: Vec<(String, Option<i64>)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let at = name.strip_prefix(UNREADABLE)?.parse().ok();
            Some((name, at))
        })
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

/// The part `pauses` (DM5): each source's hold 0.17 kept in the user data moved into `source_pauses`, cut to its cap from `now`, a
/// past or empty one dropped; then the choices cleared (after the rows' commit, so a stop between clears them at the next start).
/// `after`: heard once the rows and mark are committed, before the choices are cleared (what the tests cut it by). Blocking.
/// PL4 (c): an install upgrading from 0.17 (`upgrading`: its settings file from before 0.18) with no floor saved keeps 0.17's 200 GB,
/// saved, so the new default (a tenth of the smallest disk the floor reads) changes no floor at the upgrade; a floor saved before
/// stays. Once (its mark), safe to repeat: the floor is saved before the mark, and a floor found saved at a repeat is kept. The
/// report says what was saved and the margin's new rule.
pub fn floor(journal: &Journal, ud: &UserData, upgrading: bool) -> Result<()> {
    use super::room::{FLOOR as KEPT_017, GB, saved_floor, set_floor};
    if journal.mark(FLOOR)?.is_some() {
        return Ok(());
    }
    let saved = saved_floor(ud);
    let keeps = upgrading && saved.is_none();
    if keeps {
        set_floor(ud, KEPT_017)?;
    }
    let report = serde_json::json!({
        "saved_gb": keeps.then_some(KEPT_017 / GB),
        "kept_gb": saved.map(|s| s as f64 / GB as f64),
        "said": "0.17's floor of 200 GB kept, saved; a saved floor under 100 GB now clears 10% above it, not 10 GB",
    });
    journal.write(|tx| tx.set_mark(FLOOR, &report.to_string()))?;
    if upgrading {
        ud.set_migration_part(FLOOR_PART, &report)?;
    }
    Ok(())
}

pub fn pauses(journal: &Journal, ud: &UserData, now: i64, after: &dyn Fn() -> Result<()>) -> Result<()> {
    let choices: Vec<(&str, String)> = super::turn::KEPT.iter().map(|s| (*s, super::turn::held_choice(s))).collect();
    if journal.mark(PAUSES)?.is_none() {
        let mut moved = Vec::new();
        let mut rows = Vec::new();
        for (source, key) in &choices {
            let Some(until) = ud.import_choice(key)?.and_then(|v| v.trim().parse::<i64>().ok()) else { continue };
            let Some(policy) = super::turn::policy(source) else { continue };
            let cut = until.min(now + policy.longest.as_secs() as i64);
            if cut <= now {
                moved.push(serde_json::json!({ "source": source, "until": until, "moved": false, "why": "past" }));
                continue;
            }
            let own = cut - now > super::turn::RETRY_AFTER_MOST.as_secs() as i64;
            rows.push(Pause { source: source.to_string(), until: cut, why: None, own });
            moved.push(serde_json::json!({ "source": source, "until": until, "moved": true, "kept_until": cut }));
        }
        let report = serde_json::json!({ "moved": rows.len(), "holds": moved });
        journal.write(|tx| {
            for p in &rows {
                tx.keep_pause(p)?;
            }
            tx.set_mark(PAUSES, &report.to_string())
        })?;
        ud.set_migration_part(PAUSES_PART, &report)?;
    }
    after()?;
    // The choices cleared once the rows are in (the mark says so), at this start or the next: one place for each hold (R4).
    for (_, key) in &choices {
        if ud.import_choice(key)?.is_some_and(|v| !v.is_empty()) {
            ud.set_import_choice(key, "")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::app::track_intake::item_of;

    const T0: i64 = 1_800_000_000;
    const FIXTURE: &str = include_str!("../../../../tests/fixtures/0.17/track-intake.json");

    /// A 0.17 data folder of the test's own: the fixture's queue, and Soulseek's `soulseek.json` (DC2's, never touched here).
    fn data_017(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-step018-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(FILE), FIXTURE).unwrap();
        std::fs::write(d.join("soulseek.json"), br#"{"next":3,"items":[]}"#).unwrap();
        d
    }

    fn opened(d: &Path) -> Arc<Journal> {
        Arc::new(Journal::open(Some(&d.join("journal.sqlite"))).unwrap())
    }

    fn items(j: &Journal) -> Vec<Item> {
        j.intake(KIND).unwrap().1.iter().map(|(r, s)| item_of(r, s).unwrap()).collect()
    }

    fn state(i: &Item) -> String {
        serde_json::to_value(i.state).unwrap().as_str().unwrap().to_string()
    }

    /// PL4 (c): an install upgrading from 0.17 with no floor saved keeps 0.17's 200 GB, saved by the step, its report saying so (and that a
    /// saved floor under 100 GB now clears 10% above it); once, safe to repeat. A new install saves none (its floor is a tenth of its
    /// disk), and a floor saved before stays. Never an upgrading install's floor changed by the new default.
    #[test]
    fn the_upgrade_keeps_0_17s_floor() {
        use crate::app::room::{FLOOR as DEFAULT_017, GB, saved_floor, set_floor};
        let d = data_017("pl4");
        let (j, ud) = (opened(&d), UserData::open(None).unwrap());
        floor(&j, &ud, true).unwrap();
        assert_eq!(saved_floor(&ud), Some(200 * GB), "0.17's floor saved");
        assert_eq!(DEFAULT_017, 200 * GB);
        let report = ud.migration_report().unwrap().remove(FLOOR_PART).map(|(_, r)| r).expect("its report");
        assert_eq!(report["saved_gb"], 200, "{report}");
        assert!(report["said"].as_str().is_some_and(|s| s.contains("10%")), "{report}");
        set_floor(&ud, 80 * GB).unwrap();
        floor(&j, &ud, true).unwrap();
        assert_eq!(saved_floor(&ud), Some(80 * GB), "once: an administrator's floor since stays");
        let (j2, fresh) = (Arc::new(Journal::open(None).unwrap()), UserData::open(None).unwrap());
        floor(&j2, &fresh, false).unwrap();
        assert_eq!(saved_floor(&fresh), None, "a new install saves none");
        let (j3, kept) = (Arc::new(Journal::open(None).unwrap()), UserData::open(None).unwrap());
        set_floor(&kept, 30 * GB).unwrap();
        floor(&j3, &kept, true).unwrap();
        assert_eq!(saved_floor(&kept), Some(30 * GB), "a floor saved before stays");
        std::fs::remove_dir_all(&d).ok();
    }

    /// DM1: every bulk item of a 0.17 queue becomes rows, by the table: each with its id, album, disc, position, title, recording,
    /// failed users, misses, due, why, Undo and set-aside files; Found and Approved with their candidates and choices; Searching back
    /// to Queued; Fetching and Checking back to their tick (a held one's due and why kept); the next id 41. The report counts each, and
    /// per state the counts before equal those after by the table. Never an item lost, a field dropped, a held item's deadline
    /// shortened, nor Soulseek's queue read or written.
    #[test]
    fn every_bulk_item_moves_and_none_is_lost() {
        let d = data_017("dm1");
        let (j, ud) = (opened(&d), UserData::open(None).unwrap());
        let soulseek = std::fs::read(d.join("soulseek.json")).unwrap();
        queues(&j, &d, &ud, T0, &|| Ok(())).unwrap();
        let fixture: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
        let had = fixture["items"].as_array().unwrap();
        let moved = items(&j);
        assert_eq!(moved.len(), 12, "none lost");
        for (i, f) in moved.iter().zip(had) {
            let was = |k: &str| f[k].clone();
            assert_eq!(
                serde_json::json!([i.id, i.album, i.disc, i.position, i.title, i.recording, i.failed, i.misses, i.due, i.why, i.undo, i.set_aside]),
                serde_json::json!([was("id"), was("album"), was("disc"), was("position"), was("title"), was("recording"), was("failed"), was("misses"), was("due"), was("why"), was("undo"), was("set_aside")]),
                "{}: every field",
                i.id
            );
            assert_eq!(i.candidates.len(), f["candidates"].as_array().unwrap().len(), "{}: its candidates", i.id);
            assert_eq!(i.chosen.map(|c| c as u64), f["chosen"].as_u64(), "{}: its choice", i.id);
            assert!(i.candidates.iter().all(|c| !c.offered.filename.is_empty()), "{}: each candidate's file to ask for", i.id);
            let table = match f["state"].as_str().unwrap() {
                "searching" => "queued",
                "fetching" | "checking" => "approved",
                s => s,
            };
            assert_eq!(state(i), table, "{}: by the table", i.id);
        }
        let held = moved.iter().find(|i| i.id == "ti-37").unwrap();
        assert_eq!((held.due, held.why.as_deref(), held.chosen), (T0 + 900, Some("AcoustID asked Cratarr to wait"), Some(1)), "a held one's deadline and why kept, at its tick");
        assert_eq!(j.intake(KIND).unwrap().0, 40, "the next id given is 41");
        let report = &ud.migration_report().unwrap()[QUEUES_PART].1;
        assert_eq!(report["said"], "Bulk queue: 12 moved (2 queued, 1 searched again, 2 found, 1 not found, 1 failed, 1 to fetch, 2 back to its tick, 1 added, 1 refused); 0 dropped");
        let (before, after) = (&report["before"], &report["after"]);
        let n = |v: &serde_json::Value, k: &str| v[k].as_u64().unwrap_or(0);
        assert_eq!(n(before, "queued") + n(before, "searching"), n(after, "queued"));
        assert_eq!(n(before, "approved") + n(before, "fetching") + n(before, "checking"), n(after, "approved"));
        for s in ["found", "none", "failed", "added", "refused"] {
            assert_eq!(before[s], after[s], "{s}: as many after");
        }
        assert_eq!(std::fs::read(d.join("soulseek.json")).unwrap(), soulseek, "Soulseek's queue untouched (DC2's)");
        drop(j);
        std::fs::remove_dir_all(&d).ok();
    }

    /// DM2 (kind 3): the part cut (a) inside its transaction: no rows, the file where it was, the part whole at the next start; (b) after
    /// its commit, before the rename: the rows and mark there, the next start renames the file and writes nothing more; (c) run twice:
    /// the second writes nothing, the report the same. Never rows twice, the file renamed with no rows, nor the mark without the rows.
    #[test]
    fn the_queues_step_is_safe_to_repeat() {
        let d = data_017("dm2");
        let (j, ud) = (opened(&d), UserData::open(None).unwrap());
        // (a)
        crate::intake::cut_next_write();
        assert!(queues(&j, &d, &ud, T0, &|| Ok(())).is_err(), "cut off inside");
        assert!(items(&j).is_empty() && j.mark(QUEUES).unwrap().is_none(), "no rows, no mark");
        assert!(d.join(FILE).exists() && !d.join(KEPT).exists(), "the file where it was");
        // (b)
        assert!(queues(&j, &d, &ud, T0, &|| anyhow::bail!("the stop")).is_err());
        assert_eq!(items(&j).len(), 12, "rows and mark committed");
        assert!(j.mark(QUEUES).unwrap().is_some() && d.join(FILE).exists(), "the rename not yet");
        let rows = j.intake(KIND).unwrap();
        queues(&j, &d, &ud, T0, &|| Ok(())).unwrap();
        assert!(!d.join(FILE).exists() && d.join(KEPT).exists(), "the next start renames it");
        assert_eq!(j.intake(KIND).unwrap(), rows, "and writes nothing more");
        // (c)
        let report = ud.migration_report().unwrap()[QUEUES_PART].1.clone();
        queues(&j, &d, &ud, T0 + 60, &|| Ok(())).unwrap();
        assert_eq!((j.intake(KIND).unwrap(), ud.migration_report().unwrap()[QUEUES_PART].1.clone()), (rows, report), "the second run writes nothing");
        drop(j);
        std::fs::remove_dir_all(&d).ok();
    }

    /// DM3: once moved, `track-intake.json` is `track-intake.json.017` byte for byte, kept (never deleted by 0.18), and read by
    /// nothing: an edit of it changes no row at a later start, and no code but this step (by its constant) and the first start's move
    /// of a 0.15 data folder's ids (before it: `id_stores`, `Moved::stores`) names the file.
    #[test]
    fn the_old_queue_file_is_kept_and_never_read() {
        fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        let d = data_017("dm3");
        let (j, ud) = (opened(&d), UserData::open(None).unwrap());
        queues(&j, &d, &ud, T0, &|| Ok(())).unwrap();
        assert_eq!(std::fs::read_to_string(d.join(KEPT)).unwrap(), FIXTURE, "byte for byte");
        let rows = j.intake(KIND).unwrap();
        std::fs::write(d.join(KEPT), r#"{"next":99,"items":[]}"#).unwrap();
        queues(&j, &d, &ud, T0 + 3600, &|| Ok(())).unwrap();
        assert_eq!(j.intake(KIND).unwrap(), rows, "never read again");
        assert!(d.join(KEPT).exists(), "kept, not deleted");
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut files = Vec::new();
        walk(crates, &mut files);
        let mut named = Vec::new();
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            let cut = ["#[cfg(test)]\nmod tests", "#[cfg(test)]\npub(crate) mod tests"].iter().filter_map(|m| text.find(m)).min().unwrap_or(text.len());
            let code = &text[..cut];
            if code.lines().any(|l| !l.trim_start().starts_with("//") && l.contains(&["track-intake", ".json\""].concat())) {
                named.push(f.strip_prefix(crates).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
        named.sort();
        assert_eq!(named, ["base/src/app/library.rs", "incoming/src/app/step_018.rs", "store/src/moved.rs"], "only the step and the 0.16 move of a 0.15 folder's ids name it");
        drop(j);
        std::fs::remove_dir_all(&d).ok();
    }

    /// DM4 (kind 3): a `track-intake.json` cut off mid-write is set aside whole as `track-intake.json.unreadable-<time>`, said in the
    /// log as an error and on Health while it lies there (`queues_set_aside`, Health's "The bulk queue was set aside"); no rows; the
    /// start goes on; the report names it. Never the file replaced by an empty queue (0.17's `queue()` did), deleted, nor the start
    /// stopped for it.
    #[test]
    fn an_unreadable_bulk_queue_is_set_aside_not_replaced() {
        let d = data_017("dm4");
        let cut = br#"{"next": 40, "items": [{"id": "ti-28", "album": {"source": "cra"#;
        std::fs::write(d.join(FILE), cut).unwrap();
        let (j, ud) = (opened(&d), UserData::open(None).unwrap());
        queues(&j, &d, &ud, T0, &|| Ok(())).expect("the start goes on");
        let aside = queues_set_aside(&d);
        assert_eq!(aside, [(format!("{UNREADABLE}{T0}"), Some(T0))], "set aside under its time");
        assert_eq!(std::fs::read(d.join(&aside[0].0)).unwrap(), cut, "whole");
        assert!(!d.join(FILE).exists() && !d.join(KEPT).exists(), "not replaced by an empty queue");
        assert!(items(&j).is_empty(), "no rows");
        let report = &ud.migration_report().unwrap()[QUEUES_PART].1;
        assert!(report["said"].as_str().unwrap().contains(&aside[0].0), "the report names it: {report}");
        queues(&j, &d, &ud, T0 + 60, &|| Ok(())).unwrap();
        assert_eq!(queues_set_aside(&d).len(), 1, "a later start sets nothing more aside");
        drop(j);
        std::fs::remove_dir_all(&d).ok();
    }

    /// DM5 (R4): the user data's kept holds at t0: MusicBrainz t0+40, Bandcamp t0+80,000 (its check's day), AcoustID none, Discogs
    /// past, Last.fm t0+200,000 (beyond its quarter hour). The part moves MusicBrainz, Bandcamp, and Last.fm cut to its cap into
    /// `source_pauses`; none for AcoustID or Discogs; the choices cleared after the rows' commit (a stop between: at the next start);
    /// the turns take them up: nothing asked of MusicBrainz before t0+40. Never a hold lost, lengthened, or kept in two places.
    #[tokio::test]
    async fn the_kept_holds_move_into_their_rows_once() {
        use crate::app::turn::{MUSICBRAINZ, Pinned, Turn, held_choice};
        let _one = crate::app::switches::TEST_STATE.write().await;
        let _pinned = Pinned(T0);
        crate::app::moment::PINNED.store(T0, std::sync::atomic::Ordering::Relaxed);
        let j = Arc::new(Journal::open(None).unwrap());
        let ud = UserData::open(None).unwrap();
        for (source, until) in [("musicbrainz", (T0 + 40).to_string()), ("bandcamp", (T0 + 80_000).to_string()), ("acoustid", String::new()), ("discogs", (T0 - 10).to_string()), ("lastfm", (T0 + 200_000).to_string())] {
            ud.set_import_choice(&held_choice(source), &until).unwrap();
        }
        // A stop between the rows' commit and the choices cleared.
        assert!(pauses(&j, &ud, T0, &|| anyhow::bail!("the stop")).is_err());
        let rows = j.pauses().unwrap();
        assert_eq!(rows.iter().map(|p| (p.source.as_str(), p.until)).collect::<Vec<_>>(), [("bandcamp", T0 + 80_000), ("lastfm", T0 + 900), ("musicbrainz", T0 + 40)]);
        assert_eq!(ud.import_choice(&held_choice("musicbrainz")).unwrap(), Some((T0 + 40).to_string()), "not cleared yet");
        // The next start: nothing moved again, the choices cleared.
        pauses(&j, &ud, T0 + 5, &|| Ok(())).unwrap();
        assert_eq!(j.pauses().unwrap(), rows, "nothing more, nothing lengthened");
        for s in crate::app::turn::KEPT {
            assert!(ud.import_choice(&held_choice(s)).unwrap().is_none_or(|v| v.is_empty()), "{s}: in one place");
        }
        let report = &ud.migration_report().unwrap()[PAUSES_PART].1;
        assert_eq!(report["moved"], 3);
        assert_eq!(report["holds"].as_array().unwrap().len(), 4, "each kept hold listed: {report}");
        let mb = Turn::new(MUSICBRAINZ);
        mb.keep(j.clone());
        assert_eq!(mb.paused_until(), Some(T0 + 40), "nothing asked of MusicBrainz before t0+40");
    }
}

/// 0.18's part `covers` (SCENARIOS-COUPLING COU.20, T-COU-21), what the journal and the data folder hold of it: each 0.17 Change
/// cover pick still kept and not undone, its picture read from what its run kept of the file it wrote (`cover-undo/<…>/new`), at its
/// run's time; and the kept pictures no run names any more (0.17 pruned their run): listed, never made picks (C-COU-6). The
/// catalogue's part (`Catalogue::step_018_covers`) keeps them. Reads only. Blocking.
pub fn cover_picks(journal: &Journal, data: Option<&Path>) -> Result<(Vec<crate::catalogue::step_018::CoverPick>, Vec<String>)> {
    let mut picks = Vec::new();
    if true {
        let _ = (journal, data);
        return Ok((Vec::new(), Vec::new()));
    }
    let mut named: std::collections::BTreeSet<std::path::PathBuf> = std::collections::BTreeSet::new();
    for (id, at, run, undone) in journal.file_runs_since(0)? {
        for f in &run.files {
            let Some(w) = f.whole.as_ref().filter(|w| w.op == "cover") else { continue };
            let Some(kept) = w.kept.as_ref() else { continue };
            named.insert(kept.clone());
            if undone || f.undone {
                continue;
            }
            let Some(record) = crate::serve::ids::record_number(&f.album) else { continue };
            let Ok(bytes) = std::fs::read(kept.join("new")) else { continue };
            picks.push(crate::catalogue::step_018::CoverPick { record, bytes, who: None, at, run: id });
        }
    }
    let mut unknown = Vec::new();
    if let Some(data) = data
        && let Ok(dirs) = std::fs::read_dir(data.join("cover-undo"))
    {
        for d in dirs.flatten() {
            let path = d.path();
            // Replace's own set-aside folders (`replace-<at>`) were never picks.
            let name = d.file_name().to_string_lossy().into_owned();
            if path.join("new").is_file() && !name.starts_with("replace-") && !named.contains(&path) {
                unknown.push(name);
            }
        }
    }
    unknown.sort();
    Ok((picks, unknown))
}
