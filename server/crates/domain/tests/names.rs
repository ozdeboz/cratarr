//! SCENARIOS-DC3 MATCH.1: the matcher's inventory and its guard (MAP-DC3 §3's grep set, review M10). Every place outside
//! `identity.rs` that compares two names or keys of an artist or a label is on `server/tests/name_compares.txt`, one line per file
//! and pattern with its count and what becomes of it: moved into identity (`identity:`), deleted for an id (`goes:`), a named
//! exception with its reason (`exception:`), a fold that stays (`fold:`), or no name at all (`not a name:`). A new site fails, and a
//! site gone that the list still counts fails until the list loses it, so the count only falls (the OBJ guard's way). Search's own
//! matcher (`words::key`, rules/matcher.rs) is not counted in (DOMAIN-CORE §12).
//!
//! What is scanned: the server's and the rules crate's sources, but their tests (a `…_tests.rs` or `tests.rs` file, a `tests/`
//! folder, a module behind `#[cfg(test)]` or `#[cfg(any(test, …))]`, inline or in its own file) and comments. The named folds and
//! joins count wherever they are called; a case fold that is no name's own (`eq_ignore_ascii_case`, `COLLATE NOCASE`) counts where
//! its line or the function it is in speaks of an artist or a label (MAP §3: "on names").

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// MAP-DC3 §3's grep set: each fold or join of names called by its name (a whole word, so `x_artist_key(` is another; `loose(`
/// counts `names::loose(` and `import::loose(` alike).
const NAMED: [&str; 28] = [
    "artist_key(",
    "bare_key(",
    "names::key(",
    "import::key(",
    "excluded_artists::key(",
    "credit_names(",
    "credit_keys(",
    "credited(",
    "artists::key(",
    "words::key(",
    "title_key(",
    "credited_apart(",
    "clean_name(",
    "name_match(",
    "various(",
    "listless(",
    "artist_by_name(",
    "artist_of(",
    "artist_for(",
    "one_live_artist_in(",
    "one_of_name_in(",
    "library_artist_of(",
    "of_one_name(",
    "artist_by_key(",
    "main_artist(",
    "guests_in(",
    "loose(",
    "credit_parts(",
];

/// The case folds that compare names only where they speak of an artist or a label.
const FOLDS: [&str; 2] = ["eq_ignore_ascii_case", "COLLATE NOCASE"];

/// What becomes of a site, as the list names it.
const FATES: [&str; 5] = ["identity:", "goes:", "exception:", "fold:", "not a name:"];

fn server() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.is_dir() {
            if name != "tests" && name != "target" {
                sources(&path, out);
            }
        } else if name.ends_with(".rs") && !name.ends_with("_tests.rs") && name != "tests.rs" {
            out.push(path);
        }
    }
}

/// A `#[cfg(…test…)]` attribute.
fn test_only(line: &str) -> bool {
    let s = line.trim();
    s.starts_with("#[cfg(") && s.contains("test")
}

/// A module declared on the next line: its name, and whether its body follows inline (`mod x {`) or is its own file (`mod x;`).
fn module(line: &str) -> Option<(String, bool)> {
    let s = line.trim();
    let s = s.strip_prefix("pub ").or_else(|| s.strip_prefix("pub(crate) ")).or_else(|| s.strip_prefix("pub(super) ")).unwrap_or(s);
    let rest = s.strip_prefix("mod ")?;
    let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    let after = rest[name.len()..].trim_start();
    if after.starts_with('{') {
        Some((name, true))
    } else if after.starts_with(';') {
        Some((name, false))
    } else {
        None
    }
}

/// A whole word's count: `pattern` where the character before it is no identifier's (a path before it, `ids::artist_key(`, counts).
fn count(line: &str, pattern: &str) -> usize {
    line.match_indices(pattern).filter(|(i, _)| !line[..*i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_')).count()
}

/// The line after the block that opens at byte `open` (a `{`) closes: its braces counted outside strings, raw strings, character
/// literals and comments.
fn block_end(text: &str, open: usize) -> usize {
    let b = text.as_bytes();
    let (mut i, mut depth) = (open, 0i64);
    while i < b.len() {
        match b[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return text[..i].matches('\n').count() + 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 1;
            }
            b'r' if (b.get(i + 1) == Some(&b'"') || b.get(i + 1) == Some(&b'#')) && !b[i.saturating_sub(1)].is_ascii_alphanumeric() && b[i.saturating_sub(1)] != b'_' => {
                let mut hashes = 0;
                let mut j = i + 1;
                while b.get(j) == Some(&b'#') {
                    hashes += 1;
                    j += 1;
                }
                if b.get(j) == Some(&b'"') {
                    let close: Vec<u8> = std::iter::once(b'"').chain(std::iter::repeat_n(b'#', hashes)).collect();
                    j += 1;
                    while j < b.len() && !b[j..].starts_with(&close) {
                        j += 1;
                    }
                    i = j + close.len() - 1;
                }
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'\'' => {
                // A character literal ('{', '\'', '\u{7b}'); a lifetime ('a) is none.
                if b.get(i + 1) == Some(&b'\\') {
                    let mut j = i + 2;
                    while j < b.len() && b[j] != b'\'' {
                        j += 1;
                    }
                    i = j;
                } else if b.get(i + 2) == Some(&b'\'') {
                    i += 2;
                } else if let Some(c) = text[i + 1..].chars().next()
                    && text[i + 1 + c.len_utf8()..].starts_with('\'')
                {
                    i += c.len_utf8() + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    text.matches('\n').count() + 1
}

/// Every site of each pattern in one file's code (its tests and comments out), and the files its `#[cfg(test)] mod x;` lines name.
fn scan(text: &str, sites: &mut BTreeMap<String, usize>, test_files: &mut Vec<String>) {
    let lines: Vec<&str> = text.lines().collect();
    let starts: Vec<usize> = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect();
    let mut function = String::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let s = line.trim();
        if test_only(line)
            && let Some((name, inline)) = lines.get(i + 1).and_then(|l| module(l))
        {
            if inline {
                // The module's body, to its closing brace.
                let open = starts[i + 1] + lines[i + 1].find('{').unwrap();
                i = block_end(text, open);
            } else {
                test_files.push(name);
                i += 2;
            }
            continue;
        }
        if let Some(at) = s.find("fn ") {
            let name: String = s[at + 3..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if !name.is_empty() && (at == 0 || s[..at].ends_with(' ')) {
                function = name;
            }
        }
        if !s.starts_with("//") {
            for p in NAMED {
                let n = count(line, p);
                if n > 0 {
                    *sites.entry(p.to_string()).or_default() += n;
                }
            }
            let speaks = {
                let context = format!("{line} {function}").to_lowercase();
                context.contains("artist") || context.contains("label")
            };
            for p in FOLDS {
                let n = line.matches(p).count();
                if n > 0 && speaks {
                    *sites.entry(p.to_string()).or_default() += n;
                }
            }
        }
        i += 1;
    }
}

/// Today's sites: (file, pattern) → count, the file relative to server/.
fn sites() -> BTreeMap<(String, String), usize> {
    let root = server();
    let mut files = Vec::new();
    for dir in ["crates", "rules/src", "src"] {
        sources(&root.join(dir), &mut files);
    }
    files.sort();
    let mut texts: Vec<(PathBuf, String)> = files.into_iter().map(|f| (f.clone(), std::fs::read_to_string(&f).unwrap())).collect();
    // A module behind `#[cfg(test)]` in its own file (`mod x;`): that file is a test's, whatever its name.
    let mut tests: BTreeSet<PathBuf> = BTreeSet::new();
    let mut out = BTreeMap::new();
    let mut found: Vec<(PathBuf, BTreeMap<String, usize>)> = Vec::new();
    for (path, text) in &texts {
        let (mut sites, mut named) = (BTreeMap::new(), Vec::new());
        scan(text, &mut sites, &mut named);
        let dir = path.parent().unwrap();
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        for name in named {
            // `mod x;` in main.rs, lib.rs or mod.rs is beside it; in `y.rs` it is in `y/`.
            let base = if ["main", "lib", "mod"].contains(&stem.as_str()) { dir.to_path_buf() } else { dir.join(&stem) };
            tests.insert(base.join(format!("{name}.rs")));
            tests.insert(base.join(&name).join("mod.rs"));
        }
        found.push((path.clone(), sites));
    }
    texts.clear();
    for (path, sites) in found {
        if tests.contains(&path) {
            continue;
        }
        let rel = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        for (pattern, n) in sites {
            out.insert((rel.clone(), pattern), n);
        }
    }
    out
}

/// The list: (file, pattern) → (count, fate).
fn listed() -> BTreeMap<(String, String), (usize, String)> {
    let text = std::fs::read_to_string(server().join("tests/name_compares.txt")).unwrap();
    let mut out = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(" | ").collect();
        assert_eq!(parts.len(), 4, "name_compares.txt:{}: FILE | PATTERN | COUNT | FATE", n + 1);
        let fate = parts[3].trim().to_string();
        assert!(FATES.iter().any(|f| fate.starts_with(f)), "name_compares.txt:{}: a fate is one of {FATES:?}: {fate}", n + 1);
        let count: usize = parts[2].trim().parse().unwrap_or_else(|_| panic!("name_compares.txt:{}: a count", n + 1));
        assert!(out.insert((parts[0].trim().to_string(), parts[1].trim().to_string()), (count, fate)).is_none(), "name_compares.txt:{}: listed twice", n + 1);
    }
    out
}

/// MATCH.1: every name compare outside identity.rs is listed with what becomes of it; a new one fails; one gone fails until the list
/// loses it (the count only falls). Not red first: it starts at the inventory's count, pinned, and each DC3 step lowers it in its own
/// commit.
#[test]
fn no_name_compare_outside_identity() {
    let (now, list) = (sites(), listed());
    let mut wrong: Vec<String> = Vec::new();
    for ((file, pattern), n) in &now {
        if file.ends_with("domain/src/identity.rs") {
            continue;
        }
        match list.get(&(file.clone(), pattern.clone())) {
            None => wrong.push(format!("new: {file} | {pattern} | {n} (a name compare outside identity.rs: list it with its fate, or move it there)")),
            Some((listed, _)) if listed < n => wrong.push(format!("more: {file} | {pattern} | {n}, listed {listed}")),
            Some((listed, _)) if listed > n => wrong.push(format!("fewer: {file} | {pattern} | {n}, listed {listed}: lower the list")),
            _ => {}
        }
    }
    for ((file, pattern), (listed, _)) in &list {
        if !now.contains_key(&(file.clone(), pattern.clone())) {
            wrong.push(format!("gone: {file} | {pattern}, listed {listed}: take it off the list"));
        }
    }
    let total: usize = list.values().map(|(n, _)| n).sum();
    assert!(wrong.is_empty(), "the name compares' list (server/tests/name_compares.txt, {total} sites) and the sources disagree:\n{}", wrong.join("\n"));
}
