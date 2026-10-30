//! The shared cases (tests/vectors/*.json): what each rule answers, for the server and for every other build of these
//! rules (the page's, through WebAssembly) to check against. Each case: its input, its seed where it draws, and the
//! answer expected.
//!
//! Written again from the cases below with `CRATARR_RULES_WRITE=1 cargo test -p cratarr-rules -- --ignored write`: only
//! when a rule changes on purpose (then every other build must change with it).

use cratarr_rules::{ByHand, How, Lane, Picked, Rng, add_place, pick, restore_order, shuffle_order};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct RngCase {
    seed: u64,
    /// The first numbers, as strings (more than JavaScript's safe integers).
    next: Vec<String>,
    /// `below(10)` after those.
    below_10: Vec<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PickCase {
    name: String,
    n: usize,
    how: How,
    at: Option<usize>,
    most: usize,
    seed: u64,
    expected: Picked,
}

#[derive(Debug, Serialize, Deserialize)]
struct ShuffleCase {
    name: String,
    lanes: Vec<Lane>,
    current: Option<usize>,
    seed: u64,
    expected: Option<Vec<usize>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AddCase {
    name: String,
    lanes: Vec<Lane>,
    current: Option<usize>,
    by: ByHand,
    expected: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct RestoreCase {
    name: String,
    queue: Vec<String>,
    current: Option<usize>,
    before: Vec<String>,
    expected: Vec<usize>,
}

fn rng_case(seed: u64) -> RngCase {
    let mut r = Rng::seeded(seed);
    let next = (0..5).map(|_| r.next_u64().to_string()).collect();
    let below_10 = (0..10).map(|_| r.below(10)).collect();
    RngCase { seed, next, below_10 }
}

fn pick_case(name: &str, n: usize, how: How, at: Option<usize>, most: usize, seed: u64) -> PickCase {
    let expected = pick(n, how, at, most, &mut Rng::seeded(seed));
    PickCase { name: name.into(), n, how, at, most, seed, expected }
}

fn lanes(s: &str) -> Vec<Lane> {
    s.chars()
        .map(|c| match c {
            'w' => Lane::Own,
            'n' => Lane::Next,
            'r' => Lane::Radio,
            'a' => Lane::Autoplay,
            _ => Lane::Other,
        })
        .collect()
}

fn shuffle_case(name: &str, l: &str, current: Option<usize>, seed: u64) -> ShuffleCase {
    let lanes = lanes(l);
    let expected = shuffle_order(&lanes, current, &mut Rng::seeded(seed));
    ShuffleCase { name: name.into(), lanes, current, seed, expected }
}

fn add_case(name: &str, l: &str, current: Option<usize>, by: ByHand) -> AddCase {
    let lanes = lanes(l);
    let expected = add_place(&lanes, current, by);
    AddCase { name: name.into(), lanes, current, by, expected }
}

fn restore_case(name: &str, queue: &[&str], current: Option<usize>, before: &[&str]) -> RestoreCase {
    let (queue, before): (Vec<String>, Vec<String>) = (queue.iter().map(|s| s.to_string()).collect(), before.iter().map(|s| s.to_string()).collect());
    let expected = restore_order(&queue, current, &before);
    RestoreCase { name: name.into(), queue, current, before, expected }
}

const RNG: &str = include_str!("vectors/rng.json");
const PICK: &str = include_str!("vectors/pick.json");
const SHUFFLE: &str = include_str!("vectors/shuffle_order.json");
const ADD: &str = include_str!("vectors/add_place.json");
const RESTORE: &str = include_str!("vectors/restore_order.json");

#[test]
fn the_generator_gives_the_numbers_written() {
    let cases: Vec<RngCase> = serde_json::from_str(RNG).unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        let now = rng_case(c.seed);
        assert_eq!((now.next, now.below_10), (c.next, c.below_10), "seed {}", c.seed);
    }
}

#[test]
fn picking_answers_as_written() {
    let cases: Vec<PickCase> = serde_json::from_str(PICK).unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        assert_eq!(pick(c.n, c.how, c.at, c.most, &mut Rng::seeded(c.seed)), c.expected, "{}", c.name);
    }
}

#[test]
fn a_shuffle_once_answers_as_written() {
    let cases: Vec<ShuffleCase> = serde_json::from_str(SHUFFLE).unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        assert_eq!(shuffle_order(&c.lanes, c.current, &mut Rng::seeded(c.seed)), c.expected, "{}", c.name);
    }
}

#[test]
fn adding_by_hand_answers_as_written() {
    let cases: Vec<AddCase> = serde_json::from_str(ADD).unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        assert_eq!(add_place(&c.lanes, c.current, c.by), c.expected, "{}", c.name);
    }
}

#[test]
fn undo_answers_as_written() {
    let cases: Vec<RestoreCase> = serde_json::from_str(RESTORE).unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        assert_eq!(restore_order(&c.queue, c.current, &c.before), c.expected, "{}", c.name);
    }
}

#[test]
#[ignore = "writes the vectors: only when a rule changes on purpose"]
fn write() {
    if std::env::var("CRATARR_RULES_WRITE").as_deref() != Ok("1") {
        return;
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors");
    let save = |name: &str, v: serde_json::Value| std::fs::write(dir.join(name), serde_json::to_string_pretty(&v).unwrap() + "\n").unwrap();
    save("rng.json", serde_json::to_value([rng_case(0), rng_case(1), rng_case(7), rng_case(123_456_789)]).unwrap());
    save(
        "pick.json",
        serde_json::to_value([
            pick_case("play: the first ones, in order", 20, How::Play, None, 8, 1),
            pick_case("play from a track far from the end", 20, How::Play, Some(5), 8, 1),
            pick_case("play from a track near the end: those before fill up", 20, How::Play, Some(17), 8, 1),
            pick_case("play from a track not in it: from the start", 5, How::Play, Some(9), 8, 1),
            pick_case("play next takes them in order", 20, How::Next, None, 8, 1),
            pick_case("add to queue takes them in order", 20, How::Last, Some(3), 8, 1),
            pick_case("play over the cap: the first 500", 600, How::Play, Some(450), 500, 1),
            pick_case("shuffle draws from all of it", 20, How::Shuffle, None, 8, 7),
            pick_case("shuffle from a track: it first", 20, How::Shuffle, Some(13), 8, 7),
            pick_case("a small collection shuffles whole", 6, How::Shuffle, None, 500, 3),
            pick_case("nothing to pick", 0, How::Shuffle, None, 500, 3),
        ])
        .unwrap(),
    );
    save(
        "shuffle_order.json",
        serde_json::to_value([
            shuffle_case("own tracks only", "wwwwwwww", Some(0), 5),
            shuffle_case("hand-added, inserts and Autoplay's tail stay", "wwnnrrwwwxwwaa", Some(1), 11),
            shuffle_case("the playing track partway in", "wwwwwwww", Some(3), 2),
            shuffle_case("nothing plays", "wwww", None, 1),
            shuffle_case("one own track to come", "wwnaa", Some(0), 1),
        ])
        .unwrap(),
    );
    save(
        "add_place.json",
        serde_json::to_value([
            add_case("play next: right after the playing one", "wwnnwa", Some(1), ByHand::Next),
            add_case("add to queue: after those added by hand", "wwnnwa", Some(1), ByHand::Last),
            add_case("add to queue with none added", "wwwa", Some(0), ByHand::Last),
            add_case("nothing plays: at the end", "www", None, ByHand::Next),
        ])
        .unwrap(),
    );
    save(
        "restore_order.json",
        serde_json::to_value([
            restore_case("after a clear, Autoplay's in between: all back", &["a", "b", "x", "y"], Some(2), &["c", "d", "e"]),
            restore_case("after a shuffle, two played since: the rest back", &["a", "b", "d", "c", "e"], Some(3), &["c", "d", "e"]),
            restore_case("nothing played since", &["a", "b", "e", "c", "d"], Some(1), &["c", "d", "e"]),
        ])
        .unwrap(),
    );
}
