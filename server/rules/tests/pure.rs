//! The crate's own rule (src/lib.rs): no I/O, no clock, no randomness of its own, and it builds for wasm32. Only
//! scripts/rules-wasm.sh builds that, by hand, so a break would show in the next player's build: every test run reads
//! here what would cause one.

use std::path::{Path, PathBuf};

/// What a server has and these rules may not: a file, the network, a clock, a thread, the process around them.
const NOT_HERE: [&str; 11] = ["std::fs", "std::net", "std::time", "std::thread", "std::env", "std::process", "SystemTime", "Instant", "tokio", "anyhow", "getrandom"];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_rules_read_no_file_no_clock_and_no_network() {
    let mut files = Vec::new();
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    assert!(files.len() >= 8, "{files:?}");
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        // A test of its own may read the shared cases.
        let rules = text.split("mod tests {").next().unwrap();
        for word in NOT_HERE {
            assert!(!rules.contains(word), "{word} in {}", file.display());
        }
    }
}

#[test]
fn the_rules_take_no_dependency_a_player_could_not_build() {
    let manifest = include_str!("../Cargo.toml").replace('\r', "");
    let (_, rest) = manifest.split_once("[dependencies]\n").unwrap();
    let (dependencies, _) = rest.split_once("\n[").unwrap();
    let names: Vec<&str> = dependencies.lines().filter_map(|l| l.split_once(" = ")).map(|(name, _)| name).collect();
    assert_eq!(names, ["serde", "unicode-normalization"], "a new one must build for wasm32-unknown-unknown first: scripts/rules-wasm.sh");
}
