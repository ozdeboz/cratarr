//! Excluding an artist everywhere (decision 0083; the owner, 2026-10-30): one list per account, by each artist's loose
//! key (`import::loose`: case, accents, a leading "the" and a "feat." part count for nothing), read by every automatic
//! pick: every crate (the fixed ones too), Radio and Autoplay, the shuffles, Discover and Home's shelves. Any credit
//! counts: a track crediting the artist anywhere (main, featured, a collaboration) is left out, and so is an album whose
//! album artist is them. Played by hand, they play, as an excluded track does.

use std::collections::HashSet;

/// An artist's key: their name as the list keeps it, and as the Artist filter matches it. A name of no letters or
/// digits ("!!!") is its own key, lowercased, so it is still an artist.
pub fn key(name: &str) -> String {
    let name = name.trim();
    match super::import::loose(name) {
        k if k.is_empty() => name.to_lowercase(),
        k => k,
    }
}

/// Words that join two artists in one credit ("A feat. B", "A x B", "A vs. B", "A with B").
const JOINING: [&str; 6] = ["feat", "ft", "featuring", "x", "vs", "with"];

/// The artists one credit names, each by their key and as written: the whole credit, and each artist in it, split where
/// a credit list joins them (at "&", ",", ";", " / ", brackets and the joining words). The library's tags keep "&", ","
/// and "feat." inside a value (decision 0008: they are part of credits), so a featured artist is found here, not there.
/// The whole credit is named as its first artist where they are one ("Air feat. Beck" is Air's).
pub fn credit_names(credit: &str) -> Vec<(String, String)> {
    let credit = credit.trim();
    let mut parts: Vec<(String, String)> = Vec::new();
    let marked: String = credit
        .replace(" / ", "\u{1}")
        .chars()
        .map(|c| if matches!(c, '&' | ',' | ';' | '(' | ')' | '[' | ']') { '\u{1}' } else { c })
        .collect();
    for chunk in marked.split('\u{1}') {
        let mut part: Vec<&str> = Vec::new();
        for w in chunk.split_whitespace().chain(std::iter::once("ft")) {
            let joining = JOINING.contains(&w.to_lowercase().trim_end_matches('.'));
            if !joining {
                part.push(w);
            } else if !part.is_empty() {
                let name = part.join(" ");
                parts.push((key(&name), name));
                part.clear();
            }
        }
    }
    let whole = key(credit);
    let named = parts.iter().find(|(k, _)| *k == whole).map_or_else(|| credit.to_string(), |(_, n)| n.clone());
    let mut out: Vec<(String, String)> = Vec::new();
    for (k, n) in std::iter::once((whole, named)).chain(parts) {
        if !k.is_empty() && !out.iter().any(|(x, _)| *x == k) {
            out.push((k, n));
        }
    }
    out
}

/// The keys one credit names (`credit_names`).
pub fn credit_keys(credit: &str) -> Vec<String> {
    credit_names(credit).into_iter().map(|(k, _)| k).collect()
}

/// The artists several credits name (a track's artists and album artist), each once, as first written: their keys and
/// their names, in one order (`Facts::credits`, `Facts::credit_names`).
pub fn credited<S: AsRef<str>>(credits: &[S]) -> (Vec<String>, Vec<String>) {
    let (mut keys, mut names): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for (k, n) in credits.iter().flat_map(|c| credit_names(c.as_ref())) {
        if !keys.contains(&k) {
            keys.push(k);
            names.push(n);
        }
    }
    (keys, names)
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

    #[test]
    fn each_credited_artist_is_named_as_written() {
        let names = |c: &str| credit_names(c).into_iter().map(|(_, n)| n).collect::<Vec<_>>();
        assert_eq!(names("Air feat. Beck"), ["Air", "Beck"], "the whole credit is its first artist's");
        assert_eq!(names("Beck & Air"), ["Beck & Air", "Beck", "Air"]);
        assert_eq!(names("Daft Punk (FT. Pharrell Williams)"), ["Daft Punk", "Pharrell Williams"], "a joining word in any case");
        assert_eq!(names("AC/DC"), ["AC/DC"]);
        let (keys, named) = credited(&["Air feat. Beck", "AIR", "Beck, Air"]);
        assert_eq!((keys, named), (vec!["air".to_string(), "beck".into(), "beckair".into()], vec!["Air".to_string(), "Beck".into(), "Beck, Air".into()]), "each once, as first written");
        assert_eq!((key("!!!"), key("  ")), ("!!!".to_string(), String::new()), "a name of no letters is its own");
    }
}
