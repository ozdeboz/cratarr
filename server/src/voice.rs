//! Finding a playlist by what was said (decision 0015): voice assistants hand over free text ("chill", "jean michel",
//! "a for africa"), so the name is matched loosely, surest first, and never guessed: a tie is no answer.
//!
//! 1. The name, or one of the playlist's spoken names, folded (case, accents and punctuation gone): "band aid" is
//!    "Band-aid".
//! 2. The one name that begins with what was said ("kokoro"), else the one name what was said begins with ("chill
//!    out" for "Chill").
//! 3. The closest name by letter pairs, when it is close and clearly closer than the next ("chil" for "Chill").

use crate::albums::fold;

/// How alike a name must be to count, and by how much it must beat the next one.
const CLOSE: f64 = 0.75;
const MARGIN: f64 = 0.15;
/// What was said must be this long before a name that merely begins with it counts.
const PREFIX_MIN: usize = 3;

/// A playlist as the matching sees it: its name and the other names it is spoken as.
pub struct Named<'a> {
    pub name: &'a str,
    pub spoken: &'a [String],
}

/// What was said without the words around a name ("chill music" → "chill"): a voice sentence may pass them on.
pub fn spoken_name(said: &str) -> &str {
    let said = said.trim();
    let lower = said.to_lowercase();
    for tail in [" music", " playlist", " songs", " tracks"] {
        if lower.ends_with(tail) && said.len() > tail.len() {
            return said[..said.len() - tail.len()].trim_end();
        }
    }
    said
}

/// The playlist meant by `said`, or `None` when nothing (or more than one) fits.
pub fn find<'a>(said: &str, playlists: &[Named<'a>]) -> Option<&'a str> {
    let said = fold(said);
    if said.is_empty() {
        return None;
    }
    let keys: Vec<(usize, String)> = playlists
        .iter()
        .enumerate()
        .flat_map(|(i, p)| std::iter::once(fold(p.name)).chain(p.spoken.iter().map(|s| fold(s))).map(move |k| (i, k)))
        .filter(|(_, k)| !k.is_empty())
        .collect();
    let unique = |hits: Vec<usize>| -> Option<usize> {
        let mut hits = hits;
        hits.sort_unstable();
        hits.dedup();
        match hits[..] {
            [one] => Some(one),
            _ => None,
        }
    };
    let pick = |test: &dyn Fn(&str) -> bool| -> Option<Option<usize>> {
        let hits: Vec<usize> = keys.iter().filter(|(_, k)| test(k)).map(|(i, _)| *i).collect();
        (!hits.is_empty()).then(|| unique(hits))
    };
    // The same name for two playlists is no answer.
    if let Some(found) = pick(&|k| k == said) {
        return found.map(|i| playlists[i].name);
    }
    // A beginning decides only when one name has it; a tie goes on to closeness.
    if said.chars().count() >= PREFIX_MIN {
        if let Some(Some(i)) = pick(&|k| k.starts_with(&said)) {
            return Some(playlists[i].name);
        }
        if let Some(Some(i)) = pick(&|k| said.starts_with(k) && k.chars().count() >= PREFIX_MIN) {
            return Some(playlists[i].name);
        }
    }
    let mut scored: Vec<(f64, usize)> = keys.iter().map(|(i, k)| (crate::playlists::similarity(k, &said), *i)).collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (best, i) = *scored.first()?;
    let second = scored.iter().find(|(_, j)| *j != i).map_or(0.0, |(s, _)| *s);
    (best >= CLOSE && best - second >= MARGIN).then(|| playlists[i].name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lists() -> Vec<(String, Vec<String>)> {
        [
            ("Chill", vec![]),
            ("Band-aid", vec![]),
            ("Jean-Michel", vec![]),
            ("A4AF", vec!["a for africa".to_string()]),
            ("Kokoro de Kiku", vec!["japanese".to_string()]),
            ("High 90s", vec!["high nineties".to_string()]),
            ("Dub", vec![]),
            ("Dubstep", vec![]),
            ("Chillwave", vec![]),
        ]
        .into_iter()
        .map(|(n, s)| (n.to_string(), s))
        .collect()
    }

    fn named(l: &[(String, Vec<String>)]) -> Vec<Named<'_>> {
        l.iter().map(|(n, s)| Named { name: n, spoken: s }).collect()
    }

    #[test]
    fn names_and_spoken_names_are_found_whatever_the_case_or_punctuation() {
        let l = lists();
        let p = named(&l);
        assert_eq!(find("chill", &p), Some("Chill"), "exact beats Chillwave, which merely begins with it");
        assert_eq!(find("band aid", &p), Some("Band-aid"));
        assert_eq!(find("Jean Michel", &p), Some("Jean-Michel"));
        assert_eq!(find("a for africa", &p), Some("A4AF"));
        assert_eq!(find("Japanese", &p), Some("Kokoro de Kiku"));
        assert_eq!(find("high nineties", &p), Some("High 90s"));
    }

    #[test]
    fn a_beginning_counts_only_when_one_name_has_it() {
        let l = lists();
        let p = named(&l);
        assert_eq!(find("kokoro", &p), Some("Kokoro de Kiku"));
        assert_eq!(find("dubs", &p), Some("Dubstep"));
        assert_eq!(find("chillw", &p), Some("Chillwave"));
        assert_eq!(find("du", &p), None, "too short to stand for a name");
    }

    #[test]
    fn the_words_around_a_name_are_left_out() {
        assert_eq!(spoken_name("chill music"), "chill");
        assert_eq!(spoken_name(" Jean Michel playlist "), "Jean Michel");
        assert_eq!(spoken_name("music"), "music", "a name on its own stays");
        assert_eq!(spoken_name("chill"), "chill");
    }

    #[test]
    fn a_close_name_counts_only_when_clearly_closest() {
        let l = lists();
        let p = named(&l);
        assert_eq!(find("chil", &p), Some("Chill"), "begins Chill and Chillwave, but Chill is closer by a margin");
        assert_eq!(find("jean mitchell", &p), Some("Jean-Michel"));
        assert_eq!(find("techno", &p), None);
        assert_eq!(find("", &p), None);
    }
}
