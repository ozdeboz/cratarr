//! Other names a genre goes by (0.12: one matcher, the owner 2026-10-30): data the matcher takes ([`super::Name`]'s
//! aliases), nothing fetched. A name's aliases are the other names of its group ([`of`]), groups found by their letters
//! (separators don't count), so a library's "Drum & Bass" and "drum and bass" both find "DnB".
//!
//! [`GENRES`] is kept by hand (the owner, 2026-10-30: no reading of MusicBrainz's website, whose web service gives
//! genres no aliases): well-known spellings, extended when a gap is found. Spellings that differ only in separators,
//! case or marks ("Hip-Hop", "hiphop", "Lo-Fi") need no alias.

/// Groups of names for one genre each, its usual name first.
pub const GENRES: &[&[&str]] = &[
    &["Drum and Bass", "Drum & Bass", "Drum 'n' Bass", "DnB", "D&B"],
    &["Rhythm and Blues", "R&B", "RnB"],
    &["Rock and Roll", "Rock & Roll", "Rock 'n' Roll"],
    &["Electronic Dance Music", "EDM"],
    &["Intelligent Dance Music", "IDM"],
    &["UK Garage", "UKG"],
];

/// The other names of the groups `name` is in (by its letters), each once, in the groups' order; none for a name in
/// no group.
pub fn of<G: AsRef<[S]>, S: AsRef<str>>(name: &str, groups: &[G]) -> Vec<String> {
    let key = super::letters(name);
    let mut out: Vec<String> = Vec::new();
    if key.is_empty() {
        return out;
    }
    let mut seen = vec![key.clone()];
    for g in groups {
        let g = g.as_ref();
        if !g.iter().any(|n| super::letters(n.as_ref()) == key) {
            continue;
        }
        for n in g {
            let l = super::letters(n.as_ref());
            if !l.is_empty() && !seen.contains(&l) {
                seen.push(l);
                out.push(n.as_ref().to_string());
            }
        }
    }
    out
}

/// A genre's aliases from [`GENRES`].
pub fn genre(name: &str) -> Vec<String> {
    of(name, GENRES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_genre_has_its_groups_other_names() {
        assert_eq!(genre("drum & bass"), ["Drum and Bass", "Drum 'n' Bass", "DnB", "D&B"]);
        assert_eq!(genre("R&B"), ["Rhythm and Blues", "RnB"]);
        assert!(genre("Hip Hop").is_empty() && genre("").is_empty());
        let loaded: Vec<Vec<String>> = vec![vec!["Hip Hop".into(), "Hip-Hop".into(), "Rap".into()]];
        assert_eq!(of("hiphop", &loaded), ["Rap"], "a loaded list: same letters are the name itself");
    }
}
