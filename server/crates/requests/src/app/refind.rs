//! A missing playlist line found again in the library (Health's missing tracks; the owner, 2026-10-24: the search was
//! too strict, "not all of those are truly missing"). What the line still says (its artist, title and album, from its
//! file's name and folders) against the library's tracks: the same title at heart (punctuation, "(feat. …)" and an
//! edit's or a mix's name aside) by the same artist, wherever it now lies (another album, a compilation, a file renamed).

/// What a line still says: its artist (none when its file's name has none), title and album, as written.
#[derive(Debug, Clone, PartialEq)]
pub struct Said {
    pub artist: String,
    pub title: String,
    pub album: String,
}

/// From a playlist line ("Artist/Album (2011)/03 Artist - Title.mp3"): the file's name without its number and
/// extension split at its first " - " (artist, title), else the artist's folder; the album's folder without its year.
pub fn said(line: &str) -> Said {
    let parts: Vec<&str> = line.split('/').collect();
    let file = parts.last().copied().unwrap_or("");
    let stem = file.rsplit_once('.').map_or(file, |(s, ext)| if ext.len() <= 5 { s } else { file });
    let stem = stem.trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, ' ' | '.' | '-' | '_'));
    let album = parts.len().checked_sub(2).and_then(|i| parts.get(i)).map_or(String::new(), |a| strip_year(a));
    let folder_artist = parts.len().checked_sub(3).and_then(|i| parts.get(i)).map_or("", |a| a).to_string();
    match stem.split_once(" - ") {
        Some((artist, title)) => Said { artist: artist.trim().into(), title: title.trim().into(), album },
        None => Said { artist: folder_artist, title: stem.trim().into(), album },
    }
}

fn strip_year(folder: &str) -> String {
    let f = folder.trim();
    match f.rfind(" (") {
        Some(i) if f.ends_with(')') && f[i + 2..f.len() - 1].chars().take(4).all(|c| c.is_ascii_digit()) => f[..i].trim().into(),
        _ => f.into(),
    }
}

/// Words that name a version, not the track: a bracket holding one is set aside (with "feat." and "ft.").
const VERSION: [&str; 11] = ["edit", "mix", "version", "remaster", "remastered", "mono", "stereo", "feat", "ft", "featuring", "radio"];

/// A title at heart: lower case, its brackets naming a version or a guest left out, its punctuation as spaces.
pub fn core(title: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    let mut bracket = String::new();
    for c in title.chars() {
        match c {
            '(' | '[' => {
                depth += 1;
                bracket.clear();
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                let words = normal(&bracket);
                if !words.split(' ').any(|w| VERSION.contains(&w)) {
                    out.push(' ');
                    out.push_str(&words);
                }
            }
            _ if depth > 0 => bracket.push(c),
            _ => out.push(c),
        }
    }
    // A guest outside brackets ("Title feat. Name"): from there on left out.
    let n = normal(&out);
    let words: Vec<&str> = n.split(' ').collect();
    let end = words.iter().position(|w| matches!(*w, "feat" | "ft" | "featuring")).unwrap_or(words.len());
    words[..end].join(" ")
}

/// Lower case, letters and digits only, one space between words.
pub fn normal(s: &str) -> String {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

/// How well a library track answers what a line says: none when the title does not match at heart (the same, or one
/// the start of the other); else more for the same title, the same artist (either within the other: "Arp" in "Arp &
/// Friends"), the same album.
pub fn score(said: &Said, title: &str, artist: &str, album: &str) -> Option<u32> {
    let (want, got) = (core(&said.title), core(title));
    if want.is_empty() || got.is_empty() {
        return None;
    }
    let title_points = if want == got {
        4
    } else if got.starts_with(&want) || want.starts_with(&got) {
        2
    } else {
        return None;
    };
    let (sa, ta) = (normal(&said.artist), normal(artist));
    let artist_points = if !sa.is_empty() && !ta.is_empty() && (ta.contains(&sa) || sa.contains(&ta)) { 3 } else { 0 };
    let album_points = u32::from(!said.album.is_empty() && normal(&said.album) == normal(album));
    // A title alone, by someone else, is no answer.
    (artist_points > 0 || sa.is_empty()).then_some(title_points + artist_points + album_points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_says_its_artist_title_and_album() {
        let s = said("Agoria/Impermanence (2011)/03 Agoria - Panta Rei.mp3");
        assert_eq!(s, Said { artist: "Agoria".into(), title: "Panta Rei".into(), album: "Impermanence".into() });
        assert_eq!(said("14 Actress - N.E.W..mp3").title, "N.E.W.");
        assert_eq!(said("Air/Moon Safari (1998)/01 La femme d'argent.flac"), Said { artist: "Air".into(), title: "La femme d'argent".into(), album: "Moon Safari".into() });
        assert_eq!(said("Against All Logic/2012 – 2017 (2018)/11 Against All Logic - Rave On U.mp3").album, "2012 – 2017");
    }

    #[test]
    fn titles_match_at_heart() {
        assert_eq!(core("Pastoral Symphony- I. Dominoes II. Infinity Room"), core("Pastoral Symphony_ I. Dominoes - II. Infinity Room"));
        assert_eq!(core("True To Hip Hop (feat. AG)"), "true to hip hop");
        assert_eq!(core("Grande Torino (Album Edit)"), core("Grande Torino (Original Mix)"));
        assert_eq!(core("Strings of Eden (Garden Mix)  (feat. Miguel Atwood-Ferguson)"), "strings of eden");
        assert_eq!(core("Song (Live at Paradiso)"), "song live at paradiso", "a bracket naming no version stays");
    }

    #[test]
    fn the_same_track_scores_and_another_does_not() {
        let s = said("Aphex Twin/Selected Ambient Works 85-92 (1992)/02 Aphex Twin - Tha.mp3");
        assert_eq!(score(&s, "Tha", "Aphex Twin", "The Art Of Chill 2"), Some(7), "on a compilation: title and artist");
        assert_eq!(score(&s, "Tha", "Aphex Twin", "Selected Ambient Works 85-92"), Some(8), "and the album");
        assert_eq!(score(&s, "Tha", "Someone Else", "X"), None, "a title alone, by someone else");
        assert_eq!(score(&s, "Thaw", "Aphex Twin", "X"), Some(5), "one title the start of the other");
        assert_eq!(score(&s, "Xtal", "Aphex Twin", "X"), None);
    }
}
