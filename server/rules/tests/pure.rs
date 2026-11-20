//! The crate's own rule (src/lib.rs): no I/O, no clock, no randomness of its own, and it builds for wasm32. The build
//! itself is `cargo check-wasm`, in the Dockerfile's test stage. It cannot see the rest: a file, a clock or a thread
//! compile for wasm32-unknown-unknown and only fail when a page runs them, so every test run reads here that none is
//! used. Which leaves making sure that compile reaches every module the page loads.

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
        // A module's tests in a file of their own (`mod tests;`) are tests: they may read the shared cases and keep time.
        if file.file_name().is_some_and(|name| name == "tests.rs") {
            continue;
        }
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
    // ts-rs only writes the page's types, in the server's tests: no build of the rules that plays turns `ts` on.
    let names: Vec<&str> = dependencies.lines().filter_map(|l| l.split_once(" = ")).map(|(name, _)| name).filter(|name| *name != "ts-rs").collect();
    // pathfinding (PAIR01's `assign`): pure, and check-wasm builds it with every module the page loads (SCENARIOS-PAIR01 C6).
    assert_eq!(
        names,
        ["pathfinding", "rustc-hash", "serde", "serde_json", "unicode-normalization"],
        "a new one must build for wasm32-unknown-unknown first: cargo check-wasm"
    );
    assert!(manifest.contains("\ndefault = [\"nfkd\"]\n"), "the rules' default build stays without ts");
    let check = include_str!("../../.cargo/config.toml").lines().find(|l| l.starts_with("check-wasm = ")).unwrap();
    assert!(!check.split([' ', ',', '"']).any(|word| word == "ts"), "the page's build stays without ts");
}

#[test]
fn every_module_the_page_loads_is_in_the_wasm32_check() {
    let config = include_str!("../../.cargo/config.toml").replace('\r', "");
    let (_, rest) = config.split_once("check-wasm = ").unwrap();
    let check = rest.lines().next().unwrap();
    let manifest = include_str!("../Cargo.toml").replace('\r', "");
    let modules: Vec<&str> = manifest.lines().filter_map(|l| l.split_once(" = [")).map(|(name, _)| name).filter(|name| name.starts_with("wasm-")).collect();
    assert!(!modules.is_empty(), "the crate's wasm features are read from its manifest");
    for module in modules {
        assert!(check.contains(module), "{module} is left out of cargo check-wasm, so its exports compile for nobody until the page's build");
    }
}

/// PAIR01.8 (SCENARIOS-PAIR01): `assign` reads its two lists and the caller's verdict, nothing else. It imports only the
/// solver and serde (no store, app, database or runtime); it is the one place the solver is called (no second solver beside
/// it); and its slack is the only one the rules define.
#[test]
fn assign_reads_only_its_inputs() {
    let text = include_str!("../src/assign.rs").replace('\r', "");
    let own = text.split("mod tests {").next().unwrap();
    let uses: Vec<&str> = own.lines().map(str::trim).filter(|l| l.starts_with("use ")).collect();
    assert_eq!(uses, ["use pathfinding::prelude::{Matrix, kuhn_munkres_min};", "use serde::{Deserialize, Serialize};"], "assign.rs imports the solver and serde alone");
    for word in ["crate::", "super::", "static ", "thread_local", "rusqlite", "cratarr_"] {
        assert!(!own.contains(word), "{word} in assign.rs: it reads only its inputs");
    }
    let mut files = Vec::new();
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    for file in files.iter().filter(|f| f.file_name().is_some_and(|name| name != "assign.rs")) {
        let text = std::fs::read_to_string(file).unwrap();
        assert!(!text.contains("kuhn_munkres"), "a second solver in {}", file.display());
        assert!(!text.contains("SLACK_MS: "), "a slack of its own in {}", file.display());
    }
}
