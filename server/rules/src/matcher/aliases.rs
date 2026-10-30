//! Other names a genre goes by (0.12: one matcher, the owner 2026-10-30): data the matcher takes ([`super::Name`]'s
//! aliases), nothing fetched. A name's aliases are the other names of its group ([`of`]), groups found by their letters
//! (separators don't count), so a library's "Drum & Bass" and "drum and bass" both find "DnB".
//!
//! [`GENRES`] is a small set of well-known spellings, not MusicBrainz's list: the full one (MusicBrainz's genres with
//! their aliases, CC0) is for the server to fetch at its own pace and pass in as groups of the same shape ([`of`]
//! takes any; [`Groups`] reads a long list once, 0.12: one matcher, genre aliases from MusicBrainz). Spellings that
//! differ only in separators, case or marks ("Hip-Hop", "hiphop", "Lo-Fi") need no alias.

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
    if key.is_empty() {
        return Vec::new();
    }
    others(&key, groups.iter().map(slice::<G, S>).filter(|g| g.iter().any(|n| super::letters(n.as_ref()) == key)))
}

fn slice<G: AsRef<[S]>, S>(g: &G) -> &[S] {
    g.as_ref()
}

/// The names of `groups` but those with `key`'s letters, each once (by letters), in order.
fn others<'a, S: AsRef<str> + 'a>(key: &str, groups: impl Iterator<Item = &'a [S]>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen = vec![key.to_string()];
    for g in groups {
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

/// Groups read once, asked by a name's letters (0.12: one matcher, genre aliases from MusicBrainz): MusicBrainz's
/// whole list (some 2,200 genres) with [`GENRES`] before it, where [`of`] would read every name of every group for
/// each name asked. The same answers as [`of`] over the same groups.
#[derive(Debug, Clone, Default)]
pub struct Groups {
    groups: Vec<Vec<String>>,
    /// The groups a name's letters are in, by those letters, each group once and in order.
    by: std::collections::HashMap<String, Vec<usize>>,
}

impl Groups {
    pub fn new<G: AsRef<[S]>, S: AsRef<str>>(groups: &[G]) -> Self {
        let mut out = Self::default();
        for g in groups {
            let names: Vec<String> = g.as_ref().iter().map(|n| n.as_ref().to_string()).filter(|n| !super::letters(n).is_empty()).collect();
            if names.len() < 2 {
                continue;
            }
            let at = out.groups.len();
            for n in &names {
                let held = out.by.entry(super::letters(n)).or_default();
                if held.last() != Some(&at) {
                    held.push(at);
                }
            }
            out.groups.push(names);
        }
        out
    }

    /// [`GENRES`] alone: what a genre goes by before anything is fetched, and offline.
    pub fn seed() -> Self {
        Self::new(GENRES)
    }

    /// [`GENRES`] first, then `fetched` (MusicBrainz's: each its name first, then its aliases).
    pub fn with_seed<G: AsRef<[S]>, S: AsRef<str>>(fetched: &[G]) -> Self {
        let mut all: Vec<Vec<String>> = GENRES.iter().map(|g| g.iter().map(|n| n.to_string()).collect()).collect();
        all.extend(fetched.iter().map(|g| g.as_ref().iter().map(|n| n.as_ref().to_string()).collect()));
        Self::new(&all)
    }

    /// How many groups (of two names or more).
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// The other names of the groups `name` is in (by its letters), each once, in the groups' order; none for a name
    /// in no group.
    pub fn of(&self, name: &str) -> Vec<String> {
        let key = super::letters(name);
        match self.by.get(&key) {
            Some(at) if !key.is_empty() => others(&key, at.iter().map(|&i| self.groups[i].as_slice())),
            _ => Vec::new(),
        }
    }
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

    #[test]
    fn groups_read_once_answer_as_of_does() {
        // 0.12: MusicBrainz's list after the seed, asked by letters; a group of one name says nothing.
        let fetched: Vec<Vec<String>> = vec![
            vec!["drum and bass".into(), "d'n'b".into(), "dnb".into(), "drum n bass".into()],
            vec!["hip hop".into(), "hip-hop".into(), "rap".into()],
            vec!["2-step".into()],
        ];
        let g = Groups::with_seed(&fetched);
        assert_eq!(g.len(), GENRES.len() + 2);
        assert_eq!(g.of("Drum & Bass"), ["Drum and Bass", "Drum 'n' Bass", "DnB", "D&B"]);
        assert_eq!(g.of("DnB"), ["Drum and Bass", "Drum & Bass", "Drum 'n' Bass", "D&B"], "an alias finds its groups too, each name once");
        assert_eq!(g.of("Hip-Hop"), ["rap"], "only the fetched list knows it");
        assert_eq!(g.of("rap"), ["hip hop"]);
        assert!(g.of("2-step").is_empty() && g.of("").is_empty() && g.of("Shoegaze").is_empty());
        let all: Vec<Vec<String>> = GENRES.iter().map(|x| x.iter().map(|n| n.to_string()).collect()).chain(fetched.clone()).collect();
        for name in ["drum & bass", "R&B", "rap", "UKG", "Rock"] {
            assert_eq!(g.of(name), of(name, &all), "{name:?}");
        }
        assert_eq!(Groups::seed().of("rnb"), ["Rhythm and Blues", "R&B"]);
    }
}
