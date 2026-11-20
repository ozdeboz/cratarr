//! Whether two names are one (DOMAIN-CORE §12, the one identity matcher's first part): the one title rule, X3's, for a file
//! against a list's track. Soulseek's folders and the holding's copies pair by it (PAIR01, OQ-PAIR01-1 (a), the owner
//! 2026-11-20); `assign` owns the lengths, the tiers and the ties, and is handed this rule's verdict.

use cratarr_rules::names::loose;

/// Words that make a recording another version of its title (X3): a file's title saying one its list's title does not is not
/// that track.
pub const VERSIONS: [&str; 12] = ["instrumental", "live", "remix", "mix", "acoustic", "demo", "karaoke", "acapella", "cappella", "unplugged", "reprise", "edit"];

/// A title as compared: loose, its "(Remastered)" or "- Live" left off too.
fn titled(t: &str) -> [String; 2] {
    let base = t.split(" (").next().unwrap_or(t).split(" [").next().unwrap_or(t).split(" - ").next().unwrap_or(t);
    [loose(t), loose(base)]
}

/// Two titles of one track, loosely: the same loose name, or the same before a bracket or " - ".
pub fn same_title(a: &str, b: &str) -> bool {
    let (x, y) = (titled(a), titled(b));
    !x[0].is_empty() && (x[0] == y[0] || (!x[1].is_empty() && x[1] == y[1]))
}

/// Whether a file's title is a list track's (X3, the one title rule): as [`same_title`] has it, but an "Artist - Title" list
/// compares its title part, never its artist alone; and a version word in brackets or after " - " that the list's title lacks
/// (Instrumental, Live, Remix…) makes the file another version. A word on the list's side only still pairs (OQ-PAIR01-2 (a)),
/// and a bracket that names no version ("(Bonus Track)", "(Remastered)") is the same title (OQ-PAIR01-1 (a)).
pub fn titled_as(file: &str, list: &str) -> bool {
    let words = |s: &str| -> Vec<String> { s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect() };
    let listed = words(list);
    let lower = file.to_lowercase();
    let marked: Vec<&str> = lower.split(['(', '[']).skip(1).chain(lower.split(" - ").skip(1)).collect();
    if marked.iter().any(|part| words(part).iter().any(|w| VERSIONS.contains(&w.as_str()) && !listed.contains(w))) {
        return false;
    }
    match list.split_once(" - ") {
        Some((_, title)) => loose(file) == loose(list) || same_title(file, title.trim()),
        None => same_title(file, list),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// X3 and OQ-PAIR01-1/-2 (a), the owner 2026-11-20: the rule Soulseek's `titled_as` applied before it moved here, its
    /// verdicts kept (requests' `a_bracketed_version_word_is_another_version` pins the caller's side).
    #[test]
    fn one_title_rule_for_files_and_lists() {
        assert!(!titled_as("01 Pier (Instrumental)", "Pier") && !titled_as("Pier - Live", "Pier") && !titled_as("Pier [Remix]", "Pier"));
        assert!(titled_as("Pier (Live)", "Pier (Live)") && titled_as("Pier (Remastered)", "Pier"));
        assert!(titled_as("Pier (Bonus Track)", "Pier"), "a bracket that names no version: the same title");
        assert!(titled_as("Pier", "Pier (Live)"), "a version word on the list's side only still pairs");
        assert!(titled_as("Pier", "Glass Harbour - Pier") && !titled_as("Glass Harbour - Landing", "Glass Harbour - Pier"));
        assert!(!titled_as("Glass Harbour", "Glass Harbour - Pier"), "never by the artist alone");
        assert!(titled_as("Embrace (feat. Phoebe Killdeer)", "Embrace"), "a guest folds");
        assert!(!titled_as("Breakwater", "Pier"));
        assert!(same_title("The Wall (Remastered)", "Wall") && !same_title("", ""));
    }
}
