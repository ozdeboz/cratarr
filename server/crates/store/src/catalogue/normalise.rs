//! Normalising tag values on read (decision 0008): the files keep their tags exactly as they are, and Cratarr reads
//! multi-value fields through per-library rules. Artist fields split on credit-list separators only ("&", ","
//! and "feat." are part of credits, so they stay); genres split on more; whitelisted names are never split, also
//! inside a longer value ("Jazz/Rock/Folk, World, & Country" gives Jazz, Rock and "Folk, World, & Country").
//! Old ID3v1 numeric genres become names; repeats are dropped case-insensitively. Aliases (0.9.7, the owner) name one
//! thing one way: "Hip-Hop" read as "Hip Hop", several names merged into one ("Electronica", "Electronic Music" as
//! "Electronic"); applied to the library, to the harvested words (`app::words`), and offered as fixes for the files
//! (`app::fixes`, written only when an administrator applies them).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    #[serde(default = "artist_separators")]
    pub artist_separators: Vec<String>,
    #[serde(default = "genre_separators")]
    pub genre_separators: Vec<String>,
    /// Artist names that contain a separator but are one name (AC/DC).
    #[serde(default)]
    pub artist_keep: Vec<String>,
    /// Genres that contain a separator but are one genre ("Funk / Soul").
    #[serde(default)]
    pub genre_keep: Vec<String>,
    /// Names read as another (an alias: one `from`; a merge: several), matched case-insensitively after splitting.
    #[serde(default)]
    pub genre_aliases: Vec<Alias>,
    #[serde(default)]
    pub artist_aliases: Vec<Alias>,
}

/// One name for others (0.9.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alias {
    pub from: Vec<String>,
    pub to: String,
}

/// A value as the aliases name it: the `to` of the first alias one of whose `from` it is, else itself.
pub fn aliased(value: &str, aliases: &[Alias]) -> String {
    let v = value.trim();
    aliases.iter().find(|a| a.from.iter().any(|f| f.trim().eq_ignore_ascii_case(v))).map_or_else(|| v.to_string(), |a| a.to.trim().to_string())
}

fn artist_separators() -> Vec<String> {
    vec!["/".into(), ";".into()]
}

fn genre_separators() -> Vec<String> {
    vec!["/".into(), ";".into(), ",".into()]
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            artist_separators: artist_separators(),
            genre_separators: genre_separators(),
            artist_keep: vec![],
            genre_keep: vec![],
            genre_aliases: vec![],
            artist_aliases: vec![],
        }
    }
}

/// Fields read as artist lists.
pub const ARTIST_FIELDS: [&str; 5] = ["artist", "albumartist", "artists", "albumartists", "performer"];

impl Rules {
    /// The values of one field, split and cleaned per these rules. Other fields only lose surrounding space and
    /// empty values.
    pub fn values(&self, field: &str, raw: &[String]) -> Vec<String> {
        let parts: Vec<String> = if ARTIST_FIELDS.contains(&field) {
            let seps = artist_separators_as_used(&self.artist_separators);
            raw.iter().flat_map(|v| split(v, &seps, &self.artist_keep)).map(|a| aliased(&a, &self.artist_aliases)).collect()
        } else if field == "genre" {
            raw.iter()
                .flat_map(|v| split(v, &self.genre_separators, &self.genre_keep))
                .map(|g| id3v1_genre(&g).map(str::to_string).unwrap_or(g))
                .map(|g| aliased(&g, &self.genre_aliases))
                .collect()
        } else {
            raw.iter().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect()
        };
        dedupe(parts)
    }
}

/// The artist separators as they are used (0.9.7, the owner: 30/70 was read as two artists, "30" and "70"): a slash
/// splits artists only with a space each side (" / "), as a credit list writes it; a slash inside a name (30/70, AC/DC,
/// Cos/Mes) is the name. The tags' own lists (ARTISTS, ALBUMARTISTS) are several values already.
pub fn artist_separators_as_used(separators: &[String]) -> Vec<String> {
    separators.iter().map(|s| if s.trim() == "/" { " / ".to_string() } else { s.clone() }).collect()
}

/// Split `value` on any separator, except inside whitelisted names (matched case-insensitively).
pub fn split(value: &str, separators: &[String], keep: &[String]) -> Vec<String> {
    let seps: Vec<&str> = separators.iter().map(String::as_str).filter(|s| !s.is_empty()).collect();
    let lower = value.to_lowercase();
    // Byte ranges covered by a whitelisted name; separators inside them do not count. Longest names first, so a
    // name containing another one wins.
    let mut keeps: Vec<&String> = keep.iter().filter(|k| !k.trim().is_empty()).collect();
    keeps.sort_by_key(|k| std::cmp::Reverse(k.len()));
    let mut protected: Vec<(usize, usize)> = Vec::new();
    for k in keeps {
        let k = k.to_lowercase();
        let mut from = 0;
        while let Some(i) = lower[from..].find(&k) {
            let (a, b) = (from + i, from + i + k.len());
            if !protected.iter().any(|&(x, y)| a < y && x < b) {
                protected.push((a, b));
            }
            from = b;
        }
    }
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    // `lower` has the same byte offsets as `value` only for ASCII; split on `value` itself, checking the protected
    // ranges by its offsets (lowercasing ASCII separators and names keeps lengths for the cases that matter).
    let same_offsets = lower.len() == value.len();
    while i < value.len() {
        if !value.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let inside = same_offsets && protected.iter().any(|&(a, b)| i >= a && i < b);
        match (!inside).then(|| seps.iter().find(|s| value[i..].starts_with(**s))).flatten() {
            Some(sep) => {
                out.push(value[start..i].trim().to_string());
                i += sep.len();
                start = i;
            }
            None => i += 1,
        }
    }
    out.push(value[start..].trim().to_string());
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

/// Drop repeats, ignoring case, keeping the first spelling and the order.
pub fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    values.into_iter().filter(|v| seen.insert(v.to_lowercase())).collect()
}

/// "17", "(17)" or "(17)Rock" → the ID3v1 name; anything else `None`.
pub fn id3v1_genre(value: &str) -> Option<&'static str> {
    let v = value.trim();
    let digits = v.strip_prefix('(').and_then(|r| r.split_once(')')).map(|(n, _)| n).unwrap_or(v);
    let n: usize = digits.parse().ok()?;
    ID3V1_GENRES.get(n).copied()
}

/// The ID3v1 genre list with the Winamp extensions (0–191).
const ID3V1_GENRES: [&str; 192] = [
    "Blues", "Classic Rock", "Country", "Dance", "Disco", "Funk", "Grunge", "Hip-Hop", "Jazz", "Metal", "New Age", "Oldies",
    "Other", "Pop", "R&B", "Rap", "Reggae", "Rock", "Techno", "Industrial", "Alternative", "Ska", "Death Metal", "Pranks",
    "Soundtrack", "Euro-Techno", "Ambient", "Trip-Hop", "Vocal", "Jazz+Funk", "Fusion", "Trance", "Classical", "Instrumental",
    "Acid", "House", "Game", "Sound Clip", "Gospel", "Noise", "AlternRock", "Bass", "Soul", "Punk", "Space", "Meditative",
    "Instrumental Pop", "Instrumental Rock", "Ethnic", "Gothic", "Darkwave", "Techno-Industrial", "Electronic", "Pop-Folk",
    "Eurodance", "Dream", "Southern Rock", "Comedy", "Cult", "Gangsta", "Top 40", "Christian Rap", "Pop/Funk", "Jungle",
    "Native American", "Cabaret", "New Wave", "Psychedelic", "Rave", "Showtunes", "Trailer", "Lo-Fi", "Tribal", "Acid Punk",
    "Acid Jazz", "Polka", "Retro", "Musical", "Rock & Roll", "Hard Rock", "Folk", "Folk-Rock", "National Folk", "Swing",
    "Fast Fusion", "Bebop", "Latin", "Revival", "Celtic", "Bluegrass", "Avantgarde", "Gothic Rock", "Progressive Rock",
    "Psychedelic Rock", "Symphonic Rock", "Slow Rock", "Big Band", "Chorus", "Easy Listening", "Acoustic", "Humour", "Speech",
    "Chanson", "Opera", "Chamber Music", "Sonata", "Symphony", "Booty Bass", "Primus", "Porn Groove", "Satire", "Slow Jam",
    "Club", "Tango", "Samba", "Folklore", "Ballad", "Power Ballad", "Rhythmic Soul", "Freestyle", "Duet", "Punk Rock",
    "Drum Solo", "A Cappella", "Euro-House", "Dance Hall", "Goa", "Drum & Bass", "Club-House", "Hardcore", "Terror", "Indie",
    "BritPop", "Negerpunk", "Polsk Punk", "Beat", "Christian Gangsta Rap", "Heavy Metal", "Black Metal", "Crossover",
    "Contemporary Christian", "Christian Rock", "Merengue", "Salsa", "Thrash Metal", "Anime", "JPop", "Synthpop", "Abstract",
    "Art Rock", "Baroque", "Bhangra", "Big Beat", "Breakbeat", "Chillout", "Downtempo", "Dub", "EBM", "Eclectic", "Electro",
    "Electroclash", "Emo", "Experimental", "Garage", "Global", "IDM", "Illbient", "Industro-Goth", "Jam Band", "Krautrock",
    "Leftfield", "Lounge", "Math Rock", "New Romantic", "Nu-Breakz", "Post-Punk", "Post-Rock", "Psytrance", "Shoegaze",
    "Space Rock", "Trop Rock", "World Music", "Neoclassical", "Audiobook", "Audio Theatre", "Neue Deutsche Welle", "Podcast",
    "Indie Rock", "G-Funk", "Dubstep", "Garage Rock", "Psybient",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    /// The rules this install uses (canon-tags.py, nas-infra 526c441).
    fn reference() -> Rules {
        Rules {
            artist_keep: s(&["Cos/Mes", "30/70", "The/Das", "AC/DC", "FC/Kahuna"]),
            genre_keep: s(&["Folk, World, & Country", "Funk / Soul"]),
            ..Rules::default()
        }
    }

    #[test]
    fn aliases_name_one_thing_one_way_and_merges_many() {
        let r = Rules {
            genre_aliases: vec![
                Alias { from: s(&["Hip-Hop"]), to: "Hip Hop".into() },
                Alias { from: s(&["Electronica", "electronic music"]), to: "Electronic".into() },
            ],
            artist_aliases: vec![Alias { from: s(&["Beyonce"]), to: "Beyoncé".into() }],
            ..reference()
        };
        assert_eq!(r.values("genre", &s(&["hip-hop; Electronica", "Electronic Music"])), s(&["Hip Hop", "Electronic"]), "merged, then once");
        assert_eq!(r.values("artist", &s(&["Beyonce / Jay-Z"])), s(&["Beyoncé", "Jay-Z"]));
        assert_eq!(aliased(" Jazz ", &r.genre_aliases), "Jazz");
    }

    #[test]
    fn artists_split_on_list_separators_only() {
        let r = reference();
        assert_eq!(r.values("artist", &s(&["Moodymann / Andrés; Theo Parrish"])), s(&["Moodymann", "Andrés", "Theo Parrish"]));
        let none_kept = Rules::default();
        assert_eq!(none_kept.values("artist", &s(&["30/70"])), s(&["30/70"]), "a slash inside a name is the name, kept or not");
        assert_eq!(none_kept.values("artist", &s(&["AC/DC / Lamb"])), s(&["AC/DC", "Lamb"]), "a spaced slash splits");
        assert_eq!(none_kept.values("genre", &s(&["Jazz/Rock"])), s(&["Jazz", "Rock"]), "genres split on a slash as before");
        assert_eq!(r.values("artist", &s(&["Simon & Garfunkel"])), s(&["Simon & Garfunkel"]));
        assert_eq!(r.values("artist", &s(&["Kruder, Dorfmeister"])), s(&["Kruder, Dorfmeister"]));
        assert_eq!(r.values("artist", &s(&["Wax feat. Cuffed"])), s(&["Wax feat. Cuffed"]));
        assert_eq!(r.values("albumartist", &s(&["AC/DC"])), s(&["AC/DC"]));
        assert_eq!(r.values("artist", &s(&["Cos/Mes / Moodymann"])), s(&["Cos/Mes", "Moodymann"]));
        assert_eq!(r.values("artist", &s(&["ac/dc"])), s(&["ac/dc"]), "whitelist ignores case");
    }

    #[test]
    fn genres_split_more_but_keep_whitelisted_names_even_inside_compounds() {
        let r = reference();
        assert_eq!(r.values("genre", &s(&["Jazz/Rock/Folk, World, & Country"])), s(&["Jazz", "Rock", "Folk, World, & Country"]));
        assert_eq!(r.values("genre", &s(&["Funk / Soul; Electronic, Downtempo"])), s(&["Funk / Soul", "Electronic", "Downtempo"]));
        assert_eq!(r.values("genre", &s(&["(17)", "13"])), s(&["Rock", "Pop"]));
        assert_eq!(r.values("genre", &s(&["House", "house", "Techno/House"])), s(&["House", "Techno"]));
    }

    #[test]
    fn without_a_whitelist_names_with_separators_are_split_but_an_artist_s_slash_only_spaced() {
        let r = Rules::default();
        assert_eq!(r.values("artist", &s(&["AC/DC"])), s(&["AC/DC"]), "0.9.7: a slash inside a name is the name");
        assert_eq!(r.values("artist", &s(&["AC / DC"])), s(&["AC", "DC"]));
        assert_eq!(r.values("genre", &s(&["Funk / Soul"])), s(&["Funk", "Soul"]));
    }

    #[test]
    fn other_fields_are_only_trimmed() {
        let r = reference();
        assert_eq!(r.values("title", &s(&[" Love, Love/Love ", "  "])), s(&["Love, Love/Love"]));
    }

    #[test]
    fn id3v1_codes() {
        assert_eq!(id3v1_genre("(31)"), Some("Trance"));
        assert_eq!(id3v1_genre("31"), Some("Trance"));
        assert_eq!(id3v1_genre("(4)Disco"), Some("Disco"));
        assert_eq!(id3v1_genre("999"), None);
        assert_eq!(id3v1_genre("Rock"), None);
        assert_eq!(ID3V1_GENRES[191], "Psybient");
    }

    #[test]
    fn non_ascii_values_split_safely() {
        let r = Rules::default();
        assert_eq!(r.values("artist", &s(&["Ålborg / Ødegård"])), s(&["Ålborg", "Ødegård"]));
        assert_eq!(r.values("genre", &s(&["Ålborg/Ødegård"])), s(&["Ålborg", "Ødegård"]), "unspaced, at a character boundary");
    }
}
