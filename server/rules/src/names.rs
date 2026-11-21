//! Names to match by: how two spellings of one name are told to be one. Pure text work, for the server's imports and
//! for the artists a crate leaves out alike (`excluded_artists`).

/// A name in lower case, the common accents taken off: what `natural_keys` compares.
pub fn name_key(s: &str) -> Vec<char> {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à'..='å' => 'a',
            'ç' => 'c',
            'è'..='ë' => 'e',
            'ì'..='ï' => 'i',
            'ñ' => 'n',
            'ò'..='ö' | 'ø' => 'o',
            'ù'..='ü' => 'u',
            'ý' | 'ÿ' => 'y',
            c => c,
        })
        .collect()
}

/// Two names as `name_key` keys them, in their natural order: numbers by what they count.
pub fn natural_keys(x: &[char], y: &[char]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut i, mut j) = (0, 0);
    while i < x.len() && j < y.len() {
        if x[i].is_ascii_digit() && y[j].is_ascii_digit() {
            let (si, sj) = (i, j);
            while i < x.len() && x[i].is_ascii_digit() {
                i += 1;
            }
            while j < y.len() && y[j].is_ascii_digit() {
                j += 1;
            }
            let digits = |d: &[char]| -> String { d.iter().skip_while(|c| **c == '0').collect() };
            let (n, m) = (digits(&x[si..i]), digits(&y[sj..j]));
            match n.len().cmp(&m.len()).then_with(|| n.cmp(&m)) {
                Ordering::Equal => continue,
                o => return o,
            }
        }
        match x[i].cmp(&y[j]) {
            Ordering::Equal => {
                i += 1;
                j += 1;
            }
            o => return o,
        }
    }
    (x.len() - i).cmp(&(y.len() - j))
}

/// A name to match by: lower case, letters and digits only.
pub fn key(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// A name to match by when its own key finds nothing: accents folded, "&" as "and", without what is in brackets, a
/// "feat." part or a leading "the" ("Beyoncé feat. Jay-Z" is "beyonce"; "The Wall (Remastered)" is "wall").
pub fn loose(s: &str) -> String {
    let lower: String = s.chars().map(fold).collect::<String>().to_lowercase().replace('&', " and ");
    let mut kept = String::new();
    let mut depth = 0u32;
    for c in lower.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => kept.push(c),
            _ => {}
        }
    }
    let words: Vec<&str> = kept.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let end = words.iter().position(|w| matches!(*w, "feat" | "ft" | "featuring")).unwrap_or(words.len());
    let start = usize::from(words.first() == Some(&"the") && end > 1);
    let out: String = words[start..end].concat();
    if out.is_empty() { key(s) } else { out }
}

/// A letter without its accent (the Latin ones; others as they are).
fn fold(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' => 'A',
        'ç' | 'ć' | 'č' => 'c',
        'Ç' | 'Ć' | 'Č' => 'C',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ė' | 'Ę' | 'Ě' => 'E',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' => 'i',
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ī' | 'Į' => 'I',
        'ñ' | 'ń' | 'ň' => 'n',
        'Ñ' | 'Ń' | 'Ň' => 'N',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' | 'Ő' => 'O',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ū' | 'Ů' | 'Ű' => 'U',
        'ý' | 'ÿ' => 'y',
        'Ý' | 'Ÿ' => 'Y',
        'š' | 'ś' => 's',
        'Š' | 'Ś' => 'S',
        'ž' | 'ź' | 'ż' => 'z',
        'Ž' | 'Ź' | 'Ż' => 'Z',
        'ł' => 'l',
        'Ł' => 'L',
        'ř' => 'r',
        'Ř' => 'R',
        'đ' => 'd',
        'Đ' => 'D',
        'ß' => 's',
        '’' | '‘' | '`' => '\'',
        c => c,
    }
}

/// A record's artists as one text, where only text fits (a card's line, the notification): the one formatter the server, the page
/// (index.wasm) and the device (rules-ffi) share (NAMES.24; the owner, 2026-11-20 ~18:35, "A & B", "A, B & C"). A source that
/// confirms the credit and gives its own phrase ("Commodo x Alfa Mist") is said as it gives it. An empty name is no artist.
pub fn credit_text(names: &[&str], phrase: Option<&str>) -> String {
    if let Some(phrase) = phrase.map(str::trim).filter(|p| !p.is_empty()) {
        return phrase.to_string();
    }
    let names: Vec<&str> = names.iter().map(|n| n.trim()).filter(|n| !n.is_empty()).collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} & {last}", rest.join(", ")),
    }
}

/// A track's title as every surface names it (OBJ5, OB2: one name for a track): its name, else its file's name without its
/// extension, never a "…" for an empty one. Each read through the one tag rule (WM.2): a site's stamp is no name, and one beside a
/// name is cut ("T2 www." reads "T2", "02 - www.electronicfresh.com.flac" reads "02").
pub fn track_title(name: Option<&str>, file: &str) -> String {
    use crate::hygiene::{Field, value};
    if let Some(name) = name.and_then(|n| value(Field::Title, n)).map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
        return name;
    }
    let base = file.rsplit(['/', '\\']).next().unwrap_or(file);
    let stem = base.rsplit_once('.').map_or(base, |(stem, _)| stem).trim();
    value(Field::Title, stem).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| stem.to_string())
}

/// A track's second line (OBJ5): its credit (by `credit_text`, NAMES.24), then " · " and its record's title line; either alone where
/// the other is none. Rows, cards, the player, the queue, the notification and OpenSubsonic say it by this one rule.
pub fn track_line(credit: &str, record: &str) -> String {
    match (credit.trim(), record.trim()) {
        ("", r) => r.to_string(),
        (c, "") => c.to_string(),
        (c, r) => format!("{c} · {r}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OBJ5 (DC1b's OB2, moved): a track's words are one rule, its title (its name, else its file's) and its second line (its
    /// credit by the one formatter, then its record's title line), written to shared/track-names.json for the page's, the app's and
    /// the device's tests. Red first: no rule (the title built five ways, "…" for an empty name on one surface, the file's name on
    /// another; "artist · album" joined in place; the file's ARTIST sent raw).
    #[test]
    fn a_tracks_words_are_one_rule() {
        #[derive(serde::Serialize)]
        struct Case {
            name: Option<&'static str>,
            file: &'static str,
            artists: Vec<&'static str>,
            record: &'static str,
            title: String,
            line: String,
        }
        let case = |name: Option<&'static str>, file: &'static str, artists: Vec<&'static str>, record: &'static str| {
            let title = track_title(name, file);
            let line = track_line(&credit_text(&artists, None), record);
            Case { name, file, artists, record, title, line }
        };
        let cases = vec![
            case(Some("Pier"), "Glass Harbour/Night Ferry/01 Pier.flac", vec!["Glass Harbour"], "Night Ferry"),
            case(Some("Hydra"), "Deep Harbour/01 Hydra.flac", vec!["Commodo", "Alfa Mist"], "Deep Harbour"),
            case(Some("Echo"), "Deep Harbour/03 Echo.flac", vec!["Anz", "Commodo", "Alfa Mist"], "Deep Harbour"),
            case(None, "Kite Theory/Answered/02 Lamplight.mp3", vec!["Kite Theory"], "Answered"),
            case(Some("  "), "Kite Theory/Answered/03 Morning.mp3", vec![], "Answered"),
            case(Some("Interlude"), "loose/Interlude.flac", vec!["Kite Theory"], ""),
        ];
        assert_eq!(cases[1].line, "Commodo & Alfa Mist · Deep Harbour", "never the file's raw \"Commodo / Alfa Mist\"");
        assert_eq!(cases[2].line, "Anz, Commodo & Alfa Mist · Deep Harbour");
        assert_eq!(cases[3].title, "02 Lamplight", "no name: the file's, never \"…\"");
        assert_eq!((cases[4].title.as_str(), cases[4].line.as_str()), ("03 Morning", "Answered"), "an empty name is none; no credit, the record alone");
        assert_eq!(cases[5].line, "Kite Theory", "no record: the credit alone");
        #[derive(serde::Serialize)]
        struct Table {
            about: &'static str,
            cases: Vec<Case>,
        }
        let table = Table {
            about: "A track's words (SCENARIOS-DC1 OBJ5), written by the rules crate's tests: its title (its name, else its file's) and its second line (its credit by credit_text, then ' · ' and its record's title line), one rule for the page, the app and the device.",
            cases,
        };
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shared/track-names.json");
        let json = serde_json::to_string_pretty(&table).unwrap() + "\n";
        let kept = std::fs::read_to_string(&path).unwrap_or_default().replace('\r', "");
        if kept != json {
            std::fs::write(&path, &json).unwrap();
            panic!("shared/track-names.json was not the table's written form: written now, commit it");
        }
    }

    /// NAMES.24's formatter (its rules half; the surfaces are DC3's SPLIT): every artist named, "A & B", "A, B & C", one alone, a
    /// confirming source's own phrase as it gives it. Red first: a card names the album artist alone (records.rs's artist line).
    #[test]
    fn one_formatter_names_a_records_artists() {
        assert_eq!(credit_text(&["Commodo", "Alfa Mist"], None), "Commodo & Alfa Mist");
        assert_eq!(credit_text(&["Commodo", "Alfa Mist", "Anz"], None), "Commodo, Alfa Mist & Anz");
        assert_eq!(credit_text(&["Secede", "Kettel"], None), "Secede & Kettel");
        assert_eq!(credit_text(&["Glass Harbour"], None), "Glass Harbour");
        assert_eq!(credit_text(&[], None), "");
        assert_eq!(credit_text(&["Commodo", " ", "Alfa Mist"], None), "Commodo & Alfa Mist", "an empty name is no artist");
        assert_eq!(credit_text(&["Commodo", "Alfa Mist"], Some("Commodo x Alfa Mist")), "Commodo x Alfa Mist");
        assert_eq!(credit_text(&["Commodo", "Alfa Mist"], Some("  ")), "Commodo & Alfa Mist", "an empty phrase is none");
    }

    #[test]
    fn loosely_accents_brackets_feat_and_the_count_for_nothing() {
        assert_eq!(loose("Beyoncé feat. Jay-Z"), "beyonce");
        assert_eq!(loose("The Wall (Remastered 2011)"), "wall");
        assert_eq!(loose("Simon & Garfunkel"), "simonandgarfunkel");
        assert_eq!(loose("The The"), "the", "a name that is only \"the\" keeps it");
        assert_eq!(loose("(Untitled)"), "untitled", "nothing but brackets: its own key");
    }
}
