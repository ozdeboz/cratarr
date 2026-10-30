//! Excluding an artist everywhere (decision 0083; the owner, 2026-10-30): one list per account, by each artist's loose
//! key (`import::loose`: case, accents, a leading "the" and a "feat." part count for nothing), read by every automatic
//! pick: every crate (the fixed ones too), Radio and Autoplay, the shuffles, Discover and Home's shelves. Any credit
//! counts: a track crediting the artist anywhere (main, featured, a collaboration) is left out, and so is an album whose
//! album artist is them. Played by hand, they play, as an excluded track does.

use std::collections::HashSet;

/// An artist's key: their name as the list keeps it.
pub fn key(name: &str) -> String {
    super::import::loose(name.trim())
}

/// Words that join two artists in one credit ("A feat. B", "A x B", "A vs. B", "A with B").
const JOINING: [&str; 6] = ["feat", "ft", "featuring", "x", "vs", "with"];

/// The keys one credit names: the whole credit's, and each artist's in it, split where a credit list joins them (at
/// "&", ",", ";", " / ", brackets and the joining words). The library's tags keep "&", "," and "feat." inside a value
/// (decision 0008: they are part of credits), so a featured artist is found here, not there.
pub fn credit_keys(credit: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut push = |k: String| {
        if !k.is_empty() && !keys.contains(&k) {
            keys.push(k);
        }
    };
    push(key(credit));
    let marked: String = credit
        .to_lowercase()
        .replace(" / ", "\u{1}")
        .chars()
        .map(|c| if matches!(c, '&' | ',' | ';' | '(' | ')' | '[' | ']') { '\u{1}' } else { c })
        .collect();
    for chunk in marked.split('\u{1}') {
        let mut part: Vec<&str> = Vec::new();
        for w in chunk.split_whitespace().chain(std::iter::once("ft")) {
            if JOINING.contains(&w.trim_end_matches('.')) {
                if !part.is_empty() {
                    push(key(&part.join(" ")));
                    part.clear();
                }
            } else {
                part.push(w);
            }
        }
    }
    keys
}

/// The keys of several credits (a track's artists and album artist), each once.
pub fn credits_keys<S: AsRef<str>>(credits: &[S]) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for k in credits.iter().flat_map(|c| credit_keys(c.as_ref())) {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    keys
}

/// Whether any of these credits names an artist excluded (`out`, their keys).
pub fn credits_excluded<S: AsRef<str>>(credits: &[S], out: &HashSet<String>) -> bool {
    !out.is_empty() && credits.iter().any(|c| credit_keys(c.as_ref()).iter().any(|k| out.contains(k)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| key(n)).collect()
    }

    #[test]
    fn a_featured_artist_and_a_collaborator_are_credited() {
        assert_eq!(credit_keys("Beyoncé feat. Jay-Z"), ["beyonce", "jayz"]);
        assert_eq!(credit_keys("Daft Punk (ft. Pharrell Williams)"), ["daftpunk", "pharrellwilliams"]);
        assert_eq!(credit_keys("Bonobo & Totally Enormous Extinct Dinosaurs"), ["bonoboandtotallyenormousextinctdinosaurs", "bonobo", "totallyenormousextinctdinosaurs"]);
        assert_eq!(credit_keys("Skrillex x Four Tet, Flowdan"), ["skrillexxfourtetflowdan", "skrillex", "fourtet", "flowdan"]);
        assert!(credits_excluded(&["Air feat. Beck"], &out(&["Beck"])));
        assert!(credits_excluded(&["Hall & Oates"], &out(&["Oates"])));
        assert!(credits_excluded(&["The Avalanches"], &out(&["avalanches"])), "loosely: a leading the counts for nothing");
        assert!(!credits_excluded(&["Becky Hill"], &out(&["Beck"])), "a whole name, not a part of one");
        assert!(!credits_excluded(&["AC/DC"], &out(&["DC"])), "a slash inside a name is the name");
        assert!(!credits_excluded(&["Air"], &HashSet::new()));
    }
}
