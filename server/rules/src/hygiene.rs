//! Tag hygiene (SCENARIOS-WM layer 1; decision 0105 as amended 2026-11-20): the one rule where a file's tags become facts. A value
//! that is only a download site's stamp (a web address, a bare domain ending in one of `shared/stamps.json`'s endings, "ripped
//! by …") is no value; a stamp leading or trailing a real value is cut, with the separator or brackets around it. An address inside
//! a value stays as written, and so does a name with dots ("will.i.am", "St.Germain"). The raw tag stays recorded where it was read:
//! this reads a value, it never changes one (WM.4).
//!
//! One rule for the server, the page and the device (WM.7): the store's readers call [`clean`], and nothing else keeps a list of
//! domains. Also here, beside it, a file's titles from its name where it has no title of its own ([`file_titles`], WM.25), each
//! cleaned the same way, and the place a name gives ([`file_place`]).

use std::sync::OnceLock;

use crate::matcher::json;

/// `shared/stamps.json` as Cratarr ships it: the one table of the endings a stamped bare domain ends in (T-WM-8, closed).
pub const STAMPS: &str = include_str!("../../../shared/stamps.json");

/// The fields read as facts (WM.2): each reads through [`clean`]. A comment, a web address frame, a website tag and the
/// MusicBrainz ids are no facts of this kind: an address is their value, and nothing here reads them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Album,
    Artist,
    Artists,
    AlbumArtist,
    AlbumArtists,
    Label,
    CatalogNumber,
    Genre,
    Style,
    Grouping,
    Mood,
    Composer,
}

impl Field {
    /// The field a tag's lower-case name is, where it is one read as a fact (LABEL's other names, ORGANIZATION and PUBLISHER,
    /// read as a label).
    pub fn of(tag: &str) -> Option<Field> {
        Some(match tag {
            "title" => Field::Title,
            "album" => Field::Album,
            "artist" => Field::Artist,
            "artists" => Field::Artists,
            "albumartist" => Field::AlbumArtist,
            "albumartists" => Field::AlbumArtists,
            "label" | "organization" | "publisher" => Field::Label,
            "catalognumber" => Field::CatalogNumber,
            "genre" => Field::Genre,
            "style" => Field::Style,
            "grouping" => Field::Grouping,
            "mood" => Field::Mood,
            "composer" => Field::Composer,
            _ => return None,
        })
    }
}

/// What a value is, read as a fact: as written; cut (the value left, and the stamp found, for Health's listing: WM.19); or no
/// value (a stamp alone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Clean {
    Kept,
    Cut { value: String, stamp: String },
    NoValue { stamp: String },
}

impl Clean {
    /// The value as a fact reads it: the raw where kept, what is left where cut, none where only a stamp.
    pub fn value<'a>(&'a self, raw: &'a str) -> Option<&'a str> {
        match self {
            Clean::Kept => Some(raw),
            Clean::Cut { value, .. } => Some(value),
            Clean::NoValue { .. } => None,
        }
    }

    /// The stamp found, where one was.
    pub fn stamp(&self) -> Option<&str> {
        match self {
            Clean::Kept => None,
            Clean::Cut { stamp, .. } | Clean::NoValue { stamp } => Some(stamp),
        }
    }
}

/// The endings of `shared/stamps.json`, read once.
fn endings() -> &'static [String] {
    static ENDINGS: OnceLock<Vec<String>> = OnceLock::new();
    ENDINGS.get_or_init(|| {
        let v = json::parse(STAMPS.as_bytes()).unwrap_or_else(|| panic!("shared/stamps.json: not JSON"));
        let list = v.get("domains").and_then(json::Value::as_arr).unwrap_or_else(|| panic!("shared/stamps.json: no \"domains\""));
        list.iter().map(|d| d.as_str().map(|s| s.trim().to_lowercase()).unwrap_or_else(|| panic!("shared/stamps.json: a domain is a text"))).collect()
    })
}

/// A space of any width, a zero-width one too: what lies between words, and what goes with a stamp at a cut's edge (T-WM-8).
fn space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}')
}

/// What separates a stamp from the value beside it.
fn separator(word: &str) -> bool {
    matches!(word, "-" | "–" | "—" | "|" | ":" | "~" | "/" | "·")
}

/// A value's edges trimmed of spaces of any width and of separators.
fn trimmed(s: &str) -> &str {
    s.trim_matches(|c: char| space(c) || matches!(c, '-' | '–' | '—' | '|' | ':' | '~' | '/' | '·' | ','))
}

/// A web address: "http://…", "https://…", or "www." with or without the rest (a field cuts one off: "T2 www.", T-WM-1).
fn address(word: &str) -> bool {
    let lower = word.to_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www.")
}

/// A bare domain: one word of letters, digits, dots and dashes, two parts at least, none empty, ending in one of the table's
/// endings ("electronicfresh.com"; never "will.i.am", "M.I.A." or "St.Germain").
fn domain(word: &str) -> bool {
    let word = word.strip_suffix('/').unwrap_or(word);
    if !word.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-') {
        return false;
    }
    let parts: Vec<&str> = word.split('.').collect();
    parts.len() >= 2 && parts.iter().all(|p| !p.is_empty()) && parts.last().is_some_and(|end| endings().iter().any(|e| e.eq_ignore_ascii_case(end)))
}

/// One word that is a site's: an address or a bare domain.
fn web(word: &str) -> bool {
    address(word) || domain(word)
}

/// "ripped by" at a text's start (any case, a colon or a space after, or nothing).
fn ripped(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.strip_prefix("ripped by").is_some_and(|rest| rest.is_empty() || rest.starts_with(|c: char| space(c) || c == ':'))
}

/// A text that is a stamp whole: "ripped by …", or one word that is a site's.
fn stamp(text: &str) -> bool {
    let text = trimmed(text);
    !text.is_empty() && (ripped(text) || (!text.chars().any(space) && web(text)))
}

/// A text's words, by their byte ranges in it.
fn words(s: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (at, c) in s.char_indices() {
        match (space(c), start) {
            (true, Some(from)) => {
                out.push((from, at));
                start = None;
            }
            (false, None) => start = Some(at),
            _ => {}
        }
    }
    if let Some(from) = start {
        out.push((from, s.len()));
    }
    out
}

/// The bracket that closes `open`.
fn closer(open: char) -> Option<char> {
    match open {
        '[' => Some(']'),
        '(' => Some(')'),
        '{' => Some('}'),
        _ => None,
    }
}

/// A stamp trailing a value: (where the value ends, the stamp). In brackets at the end; after the last separator; or the last word,
/// a site's.
fn trailing(s: &str) -> Option<(usize, String)> {
    let w = words(s);
    let last = *w.last()?;
    if w.len() < 2 {
        return None;
    }
    // In brackets at the end: "Mumia [www.electronicfresh.com]", "Mumia (Ripped by Team X)".
    let tail = &s[last.0..last.1];
    if let Some(close) = tail.chars().last().filter(|c| matches!(c, ']' | ')' | '}')) {
        for &(from, _) in w.iter().rev() {
            let open = s[from..].chars().next();
            if open.and_then(closer) == Some(close) {
                let inner = &s[from + 1..last.1 - close.len_utf8()];
                if stamp(inner) && from > 0 {
                    return Some((from, trimmed(inner).to_string()));
                }
                break;
            }
        }
    }
    // After the last separator: "Mumia - www.electronicfresh.com", "Mumia | electronicfresh.com", "Mumia - ripped by Team X".
    if let Some(k) = (1..w.len()).rev().find(|&k| separator(&s[w[k].0..w[k].1]))
        && k + 1 < w.len()
    {
        let after = &s[w[k + 1].0..];
        if stamp(after) {
            return Some((w[k].0, trimmed(after).to_string()));
        }
    }
    // The last word, a site's: "Mumia www.electronicfresh.com", "T2 www." (cut off by its field).
    web(tail).then(|| (last.0, tail.to_string()))
}

/// A stamp leading a value: (where the value starts, the stamp). In brackets at the start; before the first separator; or the first
/// word, a site's (OQ-WM-4 (a): cut, as a trailing one).
fn leading(s: &str) -> Option<(usize, String)> {
    let w = words(s);
    if w.len() < 2 {
        return None;
    }
    let first = &s[w[0].0..w[0].1];
    if let Some(close) = first.chars().next().and_then(closer) {
        for &(_, to) in &w {
            if s[..to].ends_with(close) {
                let inner = &s[w[0].0 + 1..to - close.len_utf8()];
                if stamp(inner) && to < s.len() {
                    return Some((to, trimmed(inner).to_string()));
                }
                break;
            }
        }
    }
    if let Some(k) = (1..w.len() - 1).find(|&k| separator(&s[w[k].0..w[k].1])) {
        let before = &s[..w[k].0];
        if trimmed(before).chars().all(|c| !space(c)) && trimmed(before).chars().next().is_some() && web(trimmed(before)) {
            return Some((w[k].1, trimmed(before).to_string()));
        }
    }
    web(first).then(|| (w[1].0, first.to_string()))
}

/// A value read as a fact (WM.1): a stamp alone is no value; a stamp leading or trailing a real value is cut, spaces of any width
/// and the separator or brackets going with it; anything else is as written. The same for every fact field (WM.2).
pub fn clean(field: Field, raw: &str) -> Clean {
    let _ = field;
    let s = raw.trim_matches(space);
    if s.is_empty() {
        return Clean::Kept;
    }
    let whole = |s: &str| {
        let inner = s.strip_prefix(['[', '(']).and_then(|i| i.strip_suffix([']', ')'])).unwrap_or(s);
        stamp(inner).then(|| trimmed(inner).to_string())
    };
    if let Some(stamp) = whole(s) {
        return Clean::NoValue { stamp };
    }
    let mut value = s;
    let mut found: Vec<String> = Vec::new();
    if let Some((end, stamp)) = trailing(value) {
        value = &value[..end];
        found.push(stamp);
    }
    if let Some((start, stamp)) = leading(value) {
        value = &value[start..];
        found.insert(0, stamp);
    }
    if found.is_empty() {
        return Clean::Kept;
    }
    let value = trimmed(value);
    let stamp = found.join(" ");
    // A cut that leaves nothing but spaces or a separator was a stamp alone.
    if !value.chars().any(char::is_alphanumeric) {
        return Clean::NoValue { stamp };
    }
    Clean::Cut { value: value.to_string(), stamp }
}

/// [`clean`]'s value: the raw where kept, what is left where cut, none where only a stamp.
pub fn value(field: Field, raw: &str) -> Option<String> {
    match clean(field, raw) {
        Clean::Kept => Some(raw.to_string()),
        Clean::Cut { value, .. } => Some(value),
        Clean::NoValue { .. } => None,
    }
}

/// A file's name without its folders and its extension.
fn stem(file: &str) -> &str {
    let base = file.rsplit(['/', '\\']).next().unwrap_or(file);
    base.rsplit_once('.').map_or(base, |(stem, _)| stem)
}

/// A file's titles (WM.25, T-WM-7), the one reader of a name for them: its TITLE, cleaned, where it has a value (its name unread);
/// else its name's, as Soulseek reads a name (its extension left off; the name with and without a leading track number and its
/// separator; the part after " - "), each cleaned, those with a value kept, in that order, none twice.
pub fn file_titles(title: Option<&str>, file: &str) -> Vec<String> {
    if let Some(t) = title.and_then(|t| value(Field::Title, t)).map(|t| t.trim_matches(space).to_string()).filter(|t| !t.is_empty()) {
        return vec![t];
    }
    let base = stem(file).trim_matches(space);
    let no_number = base.trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, '-' | '.' | '_' | ')') || space(c));
    let mut out: Vec<String> = Vec::new();
    let mut add = |t: &str| {
        if let Some(t) = value(Field::Title, t).map(|t| t.trim_matches(space).to_string()).filter(|t| !t.is_empty())
            && !out.contains(&t)
        {
            out.push(t);
        }
    };
    add(base);
    add(no_number);
    for s in [base, no_number] {
        if let Some((_, after)) = s.split_once(" - ") {
            add(after);
        }
        if let Some((_, after)) = s.rsplit_once(" - ") {
            add(after);
        }
    }
    out
}

/// Whether a folder's name is one disc's of a release: "CD1", "CD 2", "Disc 1", "Disk 2 - …".
pub fn disc_folder(name: &str) -> Option<u32> {
    let n = name.trim().to_ascii_lowercase();
    let rest = ["cd", "disc", "disk"].iter().find_map(|p| n.strip_prefix(p))?.trim_start();
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|d| *d > 0)
}

/// The place a file's name gives where its tags give none (WM.25): its name's leading number ("07 x.flac": 7; "1-03 Pier.flac":
/// disc 1, track 3), a number only where a separator and more of the name follow it ("1979.flac" is a title), and its disc folder's
/// disc ("CD2/01 Intro.flac"). (disc, number).
pub fn file_place(file: &str) -> (Option<u32>, Option<u32>) {
    let base = stem(file).trim_matches(space);
    let folder_disc = file.rsplit(['/', '\\']).nth(1).and_then(disc_folder);
    let digits = |s: &str| -> (Option<u32>, usize) {
        let d: String = s.chars().take_while(char::is_ascii_digit).collect();
        if d.is_empty() || d.len() > 3 { (None, d.len()) } else { (d.parse().ok().filter(|n| *n > 0), d.len()) }
    };
    let (first, len) = digits(base);
    let rest = &base[len..];
    let more = |s: &str| s.chars().next().is_some_and(|c| space(c) || matches!(c, '-' | '.' | '_' | ')')) && s.chars().any(char::is_alphanumeric);
    let (disc, number) = match first {
        Some(d) if rest.starts_with('-') => match digits(&rest[1..]) {
            (Some(n), l) if more(&rest[1 + l..]) => (Some(d), Some(n)),
            _ if more(rest) => (None, Some(d)),
            _ => (None, None),
        },
        Some(n) if more(rest) => (None, Some(n)),
        _ => (None, None),
    };
    (disc.or(folder_disc), number)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WM.1's table (`tests/vectors/hygiene.json`): each value as a fact reads it, and the stamp found.
    fn vectors() -> Vec<(String, Option<String>)> {
        let v = json::parse(include_str!("../tests/vectors/hygiene.json").as_bytes()).expect("hygiene.json");
        v.get("cases")
            .and_then(json::Value::as_arr)
            .expect("cases")
            .iter()
            .map(|c| (c.get("value").and_then(json::Value::as_str).expect("value").to_string(), c.get("reads").and_then(json::Value::as_str).map(str::to_string)))
            .collect()
    }

    /// WM.1: a value that is only a site's stamp is no value, whatever its case, spaces or kind (an address, a bare domain from the
    /// table, "ripped by"), every field alike. Red first: no rule.
    #[test]
    fn a_sites_stamp_is_no_value() {
        let cases = vectors();
        let none: Vec<&String> = cases.iter().filter(|(_, reads)| reads.is_none()).map(|(v, _)| v).collect();
        assert!(none.len() >= 13, "the table's no-value rows");
        for value in none {
            for field in [Field::Album, Field::Title, Field::AlbumArtist, Field::Label] {
                let read = clean(field, value);
                assert!(matches!(read, Clean::NoValue { .. }), "{value:?} as {field:?}: {read:?}");
                assert!(read.stamp().is_some_and(|s| !s.is_empty() && !s.starts_with(space) && !s.ends_with(space)));
            }
        }
        assert_eq!(clean(Field::Album, "www.electronicfresh.com").stamp(), Some("www.electronicfresh.com"), "the stamp, for Health's listing");
    }

    /// WM.1: a stamp leading or trailing a real value is cut, with its separator or brackets and any space at the cut's edge; the
    /// value left never empty, never a separator, never ending or starting in an invisible space; an address inside a value stays.
    #[test]
    fn a_trailing_stamp_is_cut() {
        for (value, reads) in vectors().into_iter().filter(|(v, reads)| reads.as_deref().is_some_and(|r| r != v)) {
            let read = clean(Field::Album, &value);
            let reads = reads.expect("a value");
            assert_eq!(read.value(&value), Some(reads.as_str()), "{value:?}");
            assert!(matches!(read, Clean::Cut { .. }), "{value:?}: {read:?}");
            assert!(!reads.starts_with(space) && !reads.ends_with(space) && reads.chars().any(char::is_alphanumeric));
        }
        assert_eq!(clean(Field::Title, "Mumia - www.electronicfresh.com"), Clean::Cut { value: "Mumia".into(), stamp: "www.electronicfresh.com".into() });
        assert_eq!(clean(Field::Title, "Live at www.club.com 2003"), Clean::Kept, "an address inside a value stays");
        assert!(matches!(clean(Field::Title, "Mumia -"), Clean::Kept), "no stamp: nothing cut");
    }

    /// WM.1: names with dots and numbers with dots are as written; a domain whose ending isn't on the table is no stamp (T-WM-8).
    #[test]
    fn a_name_with_dots_is_kept() {
        for (raw, _) in vectors().into_iter().filter(|(v, reads)| reads.as_deref() == Some(v.as_str())) {
            assert_eq!(clean(Field::Artist, &raw), Clean::Kept, "{raw:?}");
            assert_eq!(value(Field::Artist, &raw).as_deref(), Some(raw.as_str()));
        }
        for kept in ["will.i.am", "M.I.A.", "St.Germain", "flacsite.io", "Vol. 2", "4.44"] {
            assert_eq!(clean(Field::Album, kept), Clean::Kept, "{kept}");
        }
        assert_eq!(endings().len(), 14, "T-WM-8: the table ships the fourteen endings");
    }

    /// WM.25 (a) (`tests/vectors/file_titles.json`): a file's titles from its name where it has no TITLE, Soulseek's reading, each
    /// cleaned; with a TITLE, that alone. Red first: no shared reader (Soulseek's `titles_of` alone, for its folders).
    #[test]
    fn file_titles_from_names() {
        let v = json::parse(include_str!("../tests/vectors/file_titles.json").as_bytes()).expect("file_titles.json");
        let cases = v.get("cases").and_then(json::Value::as_arr).expect("cases");
        assert!(cases.len() >= 10);
        for c in cases {
            let file = c.get("file").and_then(json::Value::as_str).expect("file");
            let title = c.get("title").and_then(json::Value::as_str);
            let titles: Vec<&str> = c.get("titles").and_then(json::Value::as_arr).expect("titles").iter().filter_map(json::Value::as_str).collect();
            assert_eq!(file_titles(title, file), titles, "{file} (TITLE {title:?})");
        }
    }

    /// WM.25's place from a name: its leading number where a separator follows, a disc's with "1-03", its disc folder's.
    #[test]
    fn a_names_place() {
        assert_eq!(file_place("07 x.flac"), (None, Some(7)));
        assert_eq!(file_place("Long Water/CD2/01 Intro.flac"), (Some(2), Some(1)));
        assert_eq!(file_place("1-03 Pier.flac"), (Some(1), Some(3)));
        assert_eq!(file_place("03 - Pier.flac"), (None, Some(3)));
        assert_eq!(file_place("1979.flac"), (None, None), "a title, no place");
        assert_eq!(file_place("x.flac"), (None, None));
        assert_eq!(file_place("a1b2c3.flac"), (None, None));
    }
}
