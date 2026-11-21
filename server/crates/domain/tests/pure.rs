//! The crate's own rule (src/lib.rs, DOMAIN-CORE §14): pure. No file, clock, network, thread or runtime in its modules, and
//! no dependency but the rules crate and serde, so it stays below the store and is tested in seconds.

use std::path::{Path, PathBuf};

/// What a server has and the domain may not: a file, the network, a clock, a thread, the process around them, a runtime.
const NOT_HERE: [&str; 13] =
    ["std::fs", "std::net", "std::time", "std::thread", "std::env", "std::process", "SystemTime", "Instant", "tokio", "anyhow", "rusqlite", "async ", ".await"];

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
fn the_domain_reads_no_file_no_clock_and_no_network() {
    let mut files = Vec::new();
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    assert!(!files.is_empty());
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        // A module's own tests may keep time and read the shared cases.
        let rules = text.split("mod tests {").next().unwrap();
        for word in NOT_HERE {
            assert!(!rules.contains(word), "{word} in {}", file.display());
        }
    }
}

#[test]
fn the_domain_depends_on_the_rules_and_serde_alone() {
    let manifest = include_str!("../Cargo.toml").replace('\r', "");
    let (_, rest) = manifest.split_once("[dependencies]\n").unwrap();
    let dependencies = rest.split_once("\n[").map_or(rest, |(dependencies, _)| dependencies);
    // `name.workspace = true` or `name = "1"`: the name is what comes before the first dot, space or equals sign.
    let names: Vec<&str> = dependencies
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split([' ', '.', '=']).next().unwrap_or(l))
        .collect();
    assert_eq!(names, ["cratarr-rules", "serde"], "the domain stays pure: no app, store or runtime crate");
}
