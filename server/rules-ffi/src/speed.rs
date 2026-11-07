//! The speed spike (preview 2's D03): the library's index on the device it will run on, over a library made up to the real
//! one's size, each number set against its budget: a crate within 0.3 s, a Radio pick within 0.05 s.
//!
//! The loop lies here and not in the app, as a call across UniFFI costs about 100 µs and a round must not carry that. Not in
//! the rules crate either, which has no clock (`tests/pure.rs`). Only the debug screen `-Pspike` builds asks for any of it, and
//! the counting allocator below is for it alone: D04 takes this file out, and the feature `made-up` with it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

use cratarr_rules::crates::{Alike, Condition, Value, View, World, like_crate};
use cratarr_rules::index::{Index, Refused, Sounds, made_up};
use cratarr_rules::radio::{Ask, Mode, choose};
use cratarr_rules::similarity::Weights;
use cratarr_rules::{QUEUE_MOST, Rng};
use serde_json::{Value as Json, json};

/// The budgets on the slowest device.
const CRATE_MS: f64 = 300.0;
const PICK_MS: f64 = 50.0;

/// How many times each thing is done: one round says little, a device being what it is.
const ROUNDS: usize = 20;

/// The seed the rules crate's own measure uses, so the two can be set side by side.
const SEED: u64 = 1;

/// Below this a library has too few tracks to draw seeds from, and a panic here ends the app (`panic = "abort"`).
const FEWEST: usize = 100;

/// How many a Sounds like holds, as a crate of one does.
const LIKE_COUNT: usize = 200;

/// Any now: what is counted back from it is made up too.
const NOW: i64 = 1_800_000_000;

/// Counts what Rust holds, as the rules crate's measure does, so the two read alike: the system's figure below is the whole
/// process, the app's runtime and all.
struct Counted;

static HELD: AtomicUsize = AtomicUsize::new(0);
static MOST: AtomicUsize = AtomicUsize::new(0);

fn more_held(bytes: usize) {
    MOST.fetch_max(HELD.fetch_add(bytes, Relaxed) + bytes, Relaxed);
}

unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        more_held(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        more_held(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, at: *mut u8, layout: Layout) {
        HELD.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(at, layout) }
    }

    // The system's own: left to the default every Vec that grows is copied whole, and that is a cost no app would have.
    unsafe fn realloc(&self, at: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        HELD.fetch_sub(layout.size(), Relaxed);
        more_held(size);
        unsafe { System.realloc(at, layout, size) }
    }
}

#[global_allocator]
static COUNTED: Counted = Counted;

fn thousandths(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn ms_since(start: Instant) -> f64 {
    thousandths(start.elapsed().as_secs_f64() * 1000.0)
}

fn mb(bytes: usize) -> f64 {
    thousandths(bytes as f64 / 1_000_000.0)
}

/// What the system counts against this process, in MB: all that is resident, the allocator's slack too, which is what it
/// reclaims by. None where there is no such file.
fn resident_mb() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb: f64 = status.lines().find_map(|l| l.strip_prefix("VmRSS:"))?.trim().strip_suffix(" kB")?.parse().ok()?;
    Some(thousandths(kb * 1024.0 / 1_000_000.0))
}

fn memory() -> Json {
    json!({ "rust_mb": mb(HELD.load(Relaxed)), "resident_mb": resident_mb() })
}

/// What is done once, in order: how long it took and what it left held.
#[derive(Default)]
struct Once(Vec<(&'static str, f64, f64)>);

impl Once {
    fn timed<T>(&mut self, what: &'static str, work: impl FnOnce() -> T) -> T {
        let (start, before) = (Instant::now(), HELD.load(Relaxed));
        let out = work();
        self.0.push((what, ms_since(start), mb(HELD.load(Relaxed).saturating_sub(before))));
        out
    }

    fn ms(&self, stages: &[&str]) -> f64 {
        thousandths(self.0.iter().filter(|(what, ..)| stages.contains(what)).map(|(_, ms, _)| ms).sum())
    }
}

/// `work` `ROUNDS` times, told which round it is and saying how much it found: nothing found is nothing measured.
fn rounds(what: &str, budget_ms: f64, mut work: impl FnMut(usize) -> usize) -> Json {
    let (mut took, mut found) = (Vec::with_capacity(ROUNDS), 0);
    for round in 0..ROUNDS {
        let start = Instant::now();
        found = found.max(work(round));
        took.push(ms_since(start));
    }
    let first = took[0];
    took.sort_by(f64::total_cmp);
    let median = thousandths((took[ROUNDS / 2 - 1] + took[ROUNDS / 2]) / 2.0);
    json!({ "what": what, "budget_ms": budget_ms, "first_ms": first, "best_ms": took[0], "median_ms": median, "worst_ms": took[ROUNDS - 1], "found": found })
}

/// Everything, in the order a device comes to it, the same calls the page and the player will make.
fn measured(tracks: usize) -> Result<Json, Refused> {
    let mut once = Once::default();
    let (rows, heard) = made_up(tracks, SEED);
    let library = json!({ "tracks": rows.tracks.len(), "records": rows.records.len(), "artists": rows.artists.len(), "libraries": rows.libraries.len(), "heard": heard.len() });

    // The server's part, as there is no server to ask.
    let built = once.timed("build", || Index::build(1, &rows))?;
    let index_bytes = once.timed("save", || built.save());
    let sounds = once.timed("sounds, build", || Sounds::build(1, &heard));
    let sound_bytes = once.timed("sounds, save", || sounds.save());

    // A device's part: it never holds rows, only what it reads from its two files, so the memory counts from here.
    drop((rows, heard, built, sounds));
    MOST.store(HELD.load(Relaxed), Relaxed);
    let index = once.timed("load", || Index::load(&index_bytes))?;
    let sounds = once.timed("sounds, load", || Sounds::load(&sound_bytes))?;
    let bytes = json!({ "index": index_bytes.len(), "sounds": sound_bytes.len() });
    drop((index_bytes, sound_bytes));
    let facts = once.timed("facts, with sounds", || index.facts(Some(&sounds)));
    let (membership, off, by) = (HashMap::new(), HashSet::new(), Weights::EQUAL);
    let alike = once.timed("Alike", || Alike::of(&facts, &membership, &index.artist_ids()));
    let mut radio = once.timed("radio's index", || index.radio(&facts, &HashMap::new()));

    // Your taste goes by what the caller marks, and with no marks at all it is Similar: this row would say nothing of its own.
    let mut r = Rng::seeded(SEED);
    radio.songs.iter_mut().for_each(|song| song.taste = Some(r.unit()));
    let n = radio.songs.len();
    // A week's listening. An empty one would cost nothing, where one with anything in it is asked about every song.
    let recent: HashMap<String, i64> = radio.songs.iter().step_by((n / 200).max(1)).map(|song| (song.id.clone(), NOW - 3_600)).collect();
    // Seeds AudioMuse heard, as nearly all of a real library's are. One not heard is scored by nothing but its words, and it counts:
    // from twelve heard seeds up a Sounds like falls into groups, each of them a pass over the library, and eleven take one pass.
    let heard: Vec<&str> = radio.songs.iter().enumerate().filter(|(at, _)| radio.like.sound(*at).is_some()).map(|(_, song)| song.id.as_str()).collect();
    // A round's seeds, apart across the library as a playlist's are, and another round's from elsewhere.
    let seeds = |round: usize, count: usize| -> Vec<String> { (0..count).map(|k| heard[(round * heard.len() / ROUNDS + k * (heard.len() / count)) % heard.len()].to_string()).collect() };

    let world = World { membership: &membership, favourites: None, favourite_albums: None, uses: None, now: NOW, fits: None };
    let view = View { files: &facts, switched_off: &off, world, by: &by, alike: || &alike };
    let two_conditions = |round: usize| {
        let genre = facts[round * n / ROUNDS].genres.first().cloned().unwrap_or_default();
        let conditions = [Condition::new("genre", false, Value::AnyOf { values: vec![genre] }), Condition::new("bpm", false, Value::Range { min: Some(100.0), max: Some(140.0) })];
        view.select(&conditions, false).len()
    };
    let like = |round: usize, count: usize| view.select(&like_crate("like:x", "Like", seeds(round, count), LIKE_COUNT).conditions, false).len();
    let crates = [
        rounds("Crate, 2 conditions", CRATE_MS, two_conditions),
        rounds("Sounds like, 1 seed", CRATE_MS, |round| like(round, 1)),
        rounds("Sounds like, 12 seeds", CRATE_MS, |round| like(round, 12)),
    ];

    // The first of the seed is the playing track, and all of it is in the queue, as when a pick is asked for in play.
    let pick = |mode: Mode, seed: Vec<String>, round: usize| {
        let ask = Ask { mode, last: seed.first().cloned(), exclude: seed.clone(), seed, ..Ask::default() };
        choose(&radio, &ask, &recent, None, &mut Rng::seeded(round as u64)).len()
    };
    let modes = [("Similar", Mode::Similar), ("Radio", Mode::Radio), ("Artist", Mode::Artist), ("Era", Mode::Era), ("Random", Mode::Random), ("Taste", Mode::Taste)];
    let mut picks: Vec<Json> = modes.into_iter().map(|(name, mode)| rounds(&format!("Pick: {name}"), PICK_MS, |round| pick(mode, seeds(round, 1), round))).collect();
    // Autoplay's seed may be the queue, and the budget is for that pick too.
    picks.push(rounds("Pick: Similar, full queue", PICK_MS, |round| pick(Mode::Similar, seeds(round, QUEUE_MOST), round)));

    let to = |stages: &[&str], first: &Json| thousandths(once.ms(stages) + first["first_ms"].as_f64().unwrap_or(0.0));
    Ok(json!({
        "build": if cfg!(debug_assertions) { "unoptimised" } else { "release" },
        "library": library,
        "bytes": bytes,
        "once": once.0.iter().map(|(what, ms, held)| json!({ "what": what, "ms": ms, "held_mb": held })).collect::<Vec<_>>(),
        "rounds": ROUNDS,
        "each": crates.iter().chain(&picks).collect::<Vec<_>>(),
        // From nothing held to the first answer: what a listener waits for at a cold start, the file's reading apart.
        "cold": {
            "to_a_crate_ms": to(&["load", "sounds, load", "facts, with sounds"], &crates[0]),
            "to_a_sounds_like_ms": to(&["load", "sounds, load", "facts, with sounds", "Alike"], &crates[2]),
            "to_a_pick_ms": to(&["load", "sounds, load", "facts, with sounds", "radio's index"], &picks[0]),
        },
        "memory": { "held": memory(), "most_rust_mb": mb(MOST.load(Relaxed)) },
    }))
}

/// Every number, as JSON, over about `tracks` made-up tracks.
#[uniffi::export]
pub fn index_speed(tracks: u32) -> String {
    let before = memory();
    // Said, not unwrapped: `panic = "abort"` would end the app with no word of why.
    let mut said = measured((tracks as usize).max(FEWEST)).unwrap_or_else(|refused| json!({ "refused": format!("{refused:?}") }));
    said["memory"]["before"] = before;
    said["memory"]["after"] = memory();
    said.to_string()
}

/// A made-up library's index as bytes: what the app will keep in a file and hand across, which until now was only ever numbers.
#[uniffi::export]
pub fn index_bytes(tracks: u32) -> Vec<u8> {
    let (rows, _) = made_up((tracks as usize).max(FEWEST), SEED);
    Index::build(1, &rows).map_or_else(|_| Vec::new(), |index| index.save())
}

/// `bytes` straight back: what a handover costs there and back, with nothing else in it.
#[uniffi::export]
pub fn index_bytes_back(bytes: Vec<u8>) -> Vec<u8> {
    bytes
}

/// Whether `bytes` read as an index: its sum says every byte arrived.
#[uniffi::export]
pub fn index_bytes_loads(bytes: Vec<u8>) -> bool {
    Index::load(&bytes).is_ok()
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard, PoisonError};

    use super::*;

    /// The counter is the whole process's, and a test beside this one would be counted in.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    /// One test failing is that one's: the others still run.
    fn alone() -> MutexGuard<'static, ()> {
        ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn number(row: &Json, field: &str) -> f64 {
        row[field].as_f64().unwrap_or_else(|| panic!("{field} in {row}"))
    }

    fn answer(tracks: u32) -> Json {
        serde_json::from_str(&index_speed(tracks)).expect("well-formed JSON")
    }

    #[test]
    fn the_answer_is_json_and_every_number_in_it_was_measured() {
        let _alone = alone();
        let said = answer(600);
        assert_eq!(said["library"]["tracks"], 600);
        assert!(number(&said["library"], "heard") > 0.0, "{said}");
        assert_eq!(said["build"], "unoptimised", "a test build says so, as its times are not a device's");
        assert_eq!(said["rounds"], ROUNDS);
        assert!(number(&said["bytes"], "index") > 1_000.0 && number(&said["bytes"], "sounds") > 1_000.0, "{said}");

        let once = said["once"].as_array().unwrap();
        let stages: Vec<&str> = once.iter().map(|stage| stage["what"].as_str().unwrap()).collect();
        assert_eq!(stages, ["build", "save", "sounds, build", "sounds, save", "load", "sounds, load", "facts, with sounds", "Alike", "radio's index"]);
        for stage in once {
            assert!(number(stage, "ms") > 0.0, "{stage}");
        }
        for held in ["load", "facts, with sounds", "Alike", "radio's index"] {
            let stage = once.iter().find(|stage| stage["what"] == held).unwrap();
            assert!(number(stage, "held_mb") > 0.0, "what a device keeps of {stage}");
        }

        let each = said["each"].as_array().unwrap();
        let rows: Vec<&str> = each.iter().map(|row| row["what"].as_str().unwrap()).collect();
        let picks = ["Pick: Similar", "Pick: Radio", "Pick: Artist", "Pick: Era", "Pick: Random", "Pick: Taste", "Pick: Similar, full queue"];
        assert_eq!(rows[..3], ["Crate, 2 conditions", "Sounds like, 1 seed", "Sounds like, 12 seeds"]);
        assert_eq!(rows[3..], picks);
        for row in each {
            let budget = if picks.contains(&row["what"].as_str().unwrap()) { PICK_MS } else { CRATE_MS };
            assert_eq!(number(row, "budget_ms"), budget, "{row}");
            assert!(number(row, "median_ms") > 0.0 && number(row, "worst_ms") >= number(row, "median_ms"), "{row}");
            assert!(number(row, "first_ms") <= number(row, "worst_ms"), "{row}");
            assert!(number(row, "found") > 0.0, "nothing found is nothing measured: {row}");
        }
        // Marked, Your taste scores nothing. Unmarked it is Similar, whole pass and all, and the row would say nothing of its own.
        let median = |what: &str| number(each.iter().find(|row| row["what"] == what).unwrap(), "median_ms");
        assert!(median("Pick: Taste") * 2.0 < median("Pick: Similar"), "{each:?}");
        // Twelve heard seeds fall into groups, a pass over the library each, in every round: with a seed not heard among them a round
        // takes one pass, and says nothing of what a playlist costs.
        let best = number(each.iter().find(|row| row["what"] == "Sounds like, 12 seeds").unwrap(), "best_ms");
        assert!(median("Sounds like, 1 seed") * 1.5 < best, "{each:?}");

        let (cold, memory) = (&said["cold"], &said["memory"]);
        let ms = |stages: &[&str]| -> f64 { stages.iter().map(|what| number(once.iter().find(|stage| stage["what"] == *what).unwrap(), "ms")).sum() };
        let first = |what: &str| number(each.iter().find(|row| row["what"] == what).unwrap(), "first_ms");
        let adds_up = |field: &str, sum: f64| assert!((number(cold, field) - sum).abs() < 0.01, "{field}, not {sum}: {cold}");
        let read = ms(&["load", "sounds, load", "facts, with sounds"]);
        adds_up("to_a_crate_ms", read + first("Crate, 2 conditions"));
        adds_up("to_a_sounds_like_ms", read + ms(&["Alike"]) + first("Sounds like, 12 seeds"));
        adds_up("to_a_pick_ms", read + ms(&["radio's index"]) + first("Pick: Similar"));
        let held = number(&memory["held"], "rust_mb");
        assert!(held > number(&memory["before"], "rust_mb") && held > number(&memory["after"], "rust_mb"), "{memory}");
        assert!(number(memory, "most_rust_mb") >= held, "{memory}");
    }

    #[test]
    fn too_few_tracks_still_answer() {
        let _alone = alone();
        for tracks in [0, 1, 99] {
            assert_eq!(answer(tracks)["library"]["tracks"], FEWEST);
        }
    }

    #[test]
    fn the_index_goes_out_and_back_whole() {
        let _alone = alone();
        let bytes = index_bytes(600);
        assert!(bytes.len() > 1_000);
        assert_eq!(index_bytes_back(bytes.clone()), bytes);
        assert!(index_bytes_loads(bytes.clone()));
        let mut changed = bytes.clone();
        changed[bytes.len() / 2] ^= 0x40;
        assert!(!index_bytes_loads(changed), "a byte changed on the way is seen");
        assert!(!index_bytes_loads(bytes[..bytes.len() - 1].to_vec()), "so is one lost");
    }
}
