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

/// A credit as one text, where only text fits (a card's line, a track's line, the player, the notification): the one formatter the
/// server, the page (index.wasm) and the device (rules-ffi) share (NAMES.24; the owner, 2026-11-20 ~18:35, "A & B", "A, B & C";
/// SCENARIOS-CRED CRED.27, the owner's credit model, 2026-11-21 14:24). It takes the credit's ordered list: each entry's name and the
/// phrase printed after it ("Rowan Fell feat. Oda Brenn", "Commodo x Alfa Mist"); an entry with none, its own join, ", " within a
/// run and " & " before the run's last. A separator ("/", ";") is never printed. An empty name is no artist.
pub fn credit_text(entries: &[(&str, Option<&str>)]) -> String {
    let entries: Vec<(&str, Option<&str>)> =
        entries.iter().map(|(n, p)| (n.trim(), p.map(str::trim).filter(|p| !is_separator(p)))).filter(|(n, _)| !n.is_empty()).collect();
    let mut out = String::new();
    for (i, (name, phrase)) in entries.iter().enumerate() {
        out.push_str(name);
        if i + 1 == entries.len() {
            break;
        }
        match phrase {
            Some(p) => {
                out.push(' ');
                out.push_str(p);
                out.push(' ');
            }
            // Its own join: " & " before the last of a run of entries with no phrase, ", " within it.
            None => out.push_str(if i + 2 == entries.len() || entries[i + 1].1.is_some() { " & " } else { ", " }),
        }
    }
    out
}

/// Whether a joining text is a list separator, never a phrase (CRED.24): "/" and ";", and nothing at all.
pub fn is_separator(join: &str) -> bool {
    matches!(join.trim(), "" | "/" | ";")
}

/// The phrase printed after an entry of a worked-out credit (CRED.27, NAMES.24, amended): the hand's (MATCH.11's split keeps its
/// printed phrase); then the files', where they name the same artists as the worked-out credit and it is a real phrase, never a
/// separator; then the first confirming source's (in the order they answered: T-NAMES-13); else none (`credit_text`'s own join).
pub fn phrase_of<'a>(hand: Option<&'a str>, files: Option<&'a str>, files_name_the_same: bool, sources: &[&'a str]) -> Option<&'a str> {
    let real = |p: &&str| !is_separator(p);
    hand.filter(real).or(files.filter(|_| files_name_the_same).filter(real)).or_else(|| sources.iter().copied().find(real))
}

/// One artist of a credit as F54's rule names it (SCENARIOS-DC3 F54.1): its name, the id the files or a source pair it with, and
/// the phrase printed after it.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Part {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phrase: Option<String>,
}

/// A credit's artists as F54 works them out, and the text it prints where that is not its artists' own: the files' whole line, kept
/// whole by the placeholder rule over a part's id (T-DC3-31).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parts {
    pub parts: Vec<Part>,
    pub text: Option<String>,
}

/// F54: a credit splits only where the files or a source name each artist (SCENARIOS-DC3 F54.1; the one splitter, used everywhere a
/// credit string becomes artists). `parts`: the credit as the files name it apart (a list tag, else its line as the library's
/// separators split it), each with the id the files pair it with and the phrase after it; `whole`: the names the same record's files
/// name whole (T-DC3-9); `source`: a source's credit for the same record. Red first (3a-1): no rule yet, the parts as given.
pub fn credit_parts(parts: &[Part], whole: &[&str], source: &[Part]) -> Parts {
    let _ = (whole, source);
    Parts { parts: parts.to_vec(), text: None }
}

/// A track's title as a row shows it beside its credit (CRED.28; the owner, ~14:38, "Trim the title on screen"): its "(feat. X)"
/// ("ft.", "featuring", in round or square brackets) taken off where the credit names that guest X (by name, case and accents
/// aside), and that part alone; every other part kept ("[Live]", "(Remix)"); a guest the credit doesn't name, kept. The stored
/// title is never touched: OpenSubsonic, search and the files keep it whole.
pub fn title_shown(title: &str, featured: &[&str]) -> String {
    let lower = title.to_lowercase();
    let mut cut: Vec<(usize, usize)> = Vec::new();
    for (open, close) in [('(', ')'), ('[', ']')] {
        let mut from = 0;
        while let Some(i) = lower[from..].find(open).map(|i| i + from) {
            let Some(j) = lower[i..].find(close).map(|j| j + i) else { break };
            let inner = lower[i + 1..j].trim_start();
            let guest = ["featuring ", "feat. ", "ft. "].iter().find_map(|m| inner.strip_prefix(m)).map(str::trim);
            if guest.is_some_and(|g| featured.iter().any(|f| name_key(f.trim()) == name_key(g))) {
                cut.push((i, j + close.len_utf8()));
            }
            from = j + close.len_utf8();
        }
    }
    cut.sort();
    let mut out = String::new();
    let mut at = 0;
    for (i, j) in cut {
        out.push_str(&title[at..i]);
        at = j;
    }
    out.push_str(&title[at..]);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
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
            let line = track_line(&credit_text(&artists.iter().map(|a| (*a, None)).collect::<Vec<_>>()), record);
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
        struct Shown {
            title: &'static str,
            featured: Vec<&'static str>,
            shown: String,
        }
        #[derive(serde::Serialize)]
        struct Table {
            about: &'static str,
            cases: Vec<Case>,
            /// CRED.28's rows: a title as shown beside a credit featuring these names (`title_shown`).
            titles: Vec<Shown>,
        }
        let titles = TITLE_ROWS.iter().map(|(title, featured, _)| Shown { title, featured: featured.to_vec(), shown: title_shown(title, featured) }).collect();
        let table = Table {
            about: "A track's words (SCENARIOS-DC1 OBJ5), written by the rules crate's tests: its title (its name, else its file's) and its second line (its credit by credit_text, then ' · ' and its record's title line), one rule for the page, the app and the device. `titles`: a title as shown beside a credit naming its featured guests (SCENARIOS-CRED CRED.28, title_shown): only the bracket naming a guest the credit features goes.",
            cases,
            titles,
        };
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shared/track-names.json");
        let json = serde_json::to_string_pretty(&table).unwrap() + "\n";
        let kept = std::fs::read_to_string(&path).unwrap_or_default().replace('\r', "");
        if kept != json {
            std::fs::write(&path, &json).unwrap();
            panic!("shared/track-names.json was not the table's written form: written now, commit it");
        }
    }

    /// The names alone, as a credit with no phrase.
    fn plain<'a>(names: &[&'a str]) -> Vec<(&'a str, Option<&'a str>)> {
        names.iter().map(|n| (*n, None)).collect()
    }

    /// NAMES.24's formatter (its rules half; the surfaces are DC3's SPLIT): every artist named, "A & B", "A, B & C", one alone, a
    /// confirming source's own phrase as it gives it. Red first: a card names the album artist alone (records.rs's artist line).
    /// Its phrase rows rewritten for CRED.27 (a pin changed by design, Found item 24): the phrase is each entry's, printed after it,
    /// where it was one text for the whole credit; DC1a's rows stand as the no-phrase case.
    #[test]
    fn one_formatter_names_a_records_artists() {
        assert_eq!(credit_text(&plain(&["Commodo", "Alfa Mist"])), "Commodo & Alfa Mist");
        assert_eq!(credit_text(&plain(&["Commodo", "Alfa Mist", "Anz"])), "Commodo, Alfa Mist & Anz");
        assert_eq!(credit_text(&plain(&["Secede", "Kettel"])), "Secede & Kettel");
        assert_eq!(credit_text(&plain(&["Glass Harbour"])), "Glass Harbour");
        assert_eq!(credit_text(&[]), "");
        assert_eq!(credit_text(&plain(&["Commodo", " ", "Alfa Mist"])), "Commodo & Alfa Mist", "an empty name is no artist");
        assert_eq!(credit_text(&[("Commodo", Some("x")), ("Alfa Mist", None)]), "Commodo x Alfa Mist");
        assert_eq!(credit_text(&[("Commodo", Some("  ")), ("Alfa Mist", None)]), "Commodo & Alfa Mist", "an empty phrase is none");
    }

    /// CRED.27: the printed phrase is the hand's, then the files', then a source's (rows (q1)-(q8), and the no-phrase case), and
    /// `credit_text` prints each entry's: the files' real phrase between the same artists first; a separator never printed (MusicBrainz's
    /// then); the files naming other artists than the credit give none; MATCH.11's hand split keeps its printed phrase. The rows are
    /// written to shared/credit-texts.json for the page's and the device's tests. Never a separator printed; never a files' phrase where
    /// the files name other artists; never the hand split's phrase replaced. Red first: `credit_text` took one phrase for the whole
    /// credit (rules/names.rs:117).
    #[test]
    fn the_printed_phrase_is_the_hands_then_the_files_then_a_sources() {
        #[derive(serde::Serialize)]
        struct Row {
            row: &'static str,
            says: &'static str,
            names: Vec<&'static str>,
            hand: Option<&'static str>,
            files: Option<&'static str>,
            files_name_the_same: bool,
            sources: Vec<&'static str>,
            roles: Vec<&'static str>,
            text: String,
        }
        // Each row: the credit's two names, their roles, the phrase each place offers between them, the text.
        let rows: Vec<(&'static str, &'static str, [&'static str; 2], [&'static str; 2], Option<&'static str>, Option<&'static str>, bool, Vec<&'static str>, &'static str)> = vec![
            ("q1", "Night Signals 5: the files' \"&\" (a real phrase, the same artists), MusicBrainz's \"x\", Discogs' \"&\"", ["Rowan Fell", "Oda Brenn"], ["main", "main"], None, Some("&"), true, vec!["x", "&"], "Rowan Fell & Oda Brenn"),
            ("q2", "Night Signals 3: the files' \"/\" (a separator: none), MusicBrainz's \"&\"", ["Rowan Fell", "Oda Brenn"], ["main", "main"], None, Some("/"), true, vec!["&"], "Rowan Fell & Oda Brenn"),
            ("q3", "Night Signals 6: the files' \";\" (none), MusicBrainz's \"&\"", ["Rowan Fell", "Oda Brenn"], ["main", "main"], None, Some(";"), true, vec!["&"], "Rowan Fell & Oda Brenn"),
            ("q4", "Night Signals 4: the files' \"ft.\", MusicBrainz's \"feat.\"", ["Rowan Fell", "Oda Brenn"], ["main", "featuring"], None, Some("ft."), true, vec!["feat."], "Rowan Fell ft. Oda Brenn"),
            ("q5", "Deep Harbour 1: the files' \"/\" only", ["Commodo", "Alfa Mist"], ["main", "main"], None, Some("/"), true, vec![], "Commodo & Alfa Mist"),
            ("q6", "the files name fewer artists than the credit (\"Rowan Fell\" for two): no files' phrase; a confirming source's", ["Rowan Fell", "Oda Brenn"], ["main", "main"], None, Some("x"), false, vec!["with"], "Rowan Fell with Oda Brenn"),
            ("q6b", "as (q6) with no source's phrase: credit_text's own", ["Rowan Fell", "Oda Brenn"], ["main", "main"], None, Some("x"), false, vec![], "Rowan Fell & Oda Brenn"),
            ("q7", "MATCH.11's split of \"Commodo & Alfa Mist\" by hand: the printed \"&\" it keeps", ["Commodo", "Alfa Mist"], ["main", "main"], Some("&"), Some("/"), true, vec!["x"], "Commodo & Alfa Mist"),
            ("q8", "Harbour's record: the files' album line's \"feat.\", MusicBrainz's \" feat. \"", ["Rowan Fell", "Tamsin Kell"], ["main", "featuring"], None, Some("feat."), true, vec!["feat."], "Rowan Fell feat. Tamsin Kell"),
        ];
        let mut written = Vec::new();
        for (row, says, names, roles, hand, files, same, sources, text) in rows {
            let phrase = phrase_of(hand, files, same, &sources);
            let got = credit_text(&[(names[0], phrase), (names[1], None)]);
            assert_eq!(got, text, "({row}) {says}");
            written.push(Row { row, says, names: names.to_vec(), hand, files, files_name_the_same: same, sources, roles: roles.to_vec(), text: got });
        }
        assert_eq!(credit_text(&[("Anz", None), ("Commodo", None), ("Alfa Mist", None)]), "Anz, Commodo & Alfa Mist", "no phrase: its own join");
        assert_eq!(credit_text(&[("Rowan Fell", Some("/")), ("Oda Brenn", None)]), "Rowan Fell & Oda Brenn", "a separator never printed");
        #[derive(serde::Serialize)]
        struct Table {
            about: &'static str,
            rows: Vec<Row>,
        }
        let table = Table {
            about: "A credit's printed phrases (SCENARIOS-CRED CRED.27), written by the rules crate's tests: the phrase between two entries is the hand's, then the files' (the same artists, a real phrase, never a separator), then the first confirming source's, else credit_text's own join; the text credit_text prints.",
            rows: written,
        };
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shared/credit-texts.json");
        let json = serde_json::to_string_pretty(&table).unwrap() + "\n";
        let kept = std::fs::read_to_string(&path).unwrap_or_default().replace('\r', "");
        if kept != json {
            std::fs::write(&path, &json).unwrap();
            panic!("shared/credit-texts.json was not the table's written form: written now, commit it");
        }
    }

    /// CRED.28: a title shows trimmed where its credit names its guest, and stays whole otherwise: "Harbour (feat. Tamsin Kell)" by
    /// "Rowan Fell feat. Tamsin Kell" shows "Harbour"; "Sloth (feat. Oda Brenn) [Live]" shows "Sloth [Live]"; "Song (feat. Friends)"
    /// (a placeholder: no featured credit) and "Tide (feat. Oda Brenn)" (a hand's split naming Rowan Fell alone) stay whole; no other
    /// bracket trimmed. Never a stored title cut (the formatter is display only); never a title trimmed where the credit doesn't name
    /// that guest. Red first: titles show whole everywhere.
    #[test]
    fn a_title_shows_trimmed_where_its_credit_names_the_guest() {
        for (title, featured, shown) in TITLE_ROWS {
            assert_eq!(title_shown(title, featured), *shown, "{title:?} beside {featured:?}");
        }
    }

    /// CRED.28's rows (title, the credit's featured names, as shown), written to shared/track-names.json's `titles` by OBJ5's test.
    const TITLE_ROWS: &[(&str, &[&str], &str)] = &[
        ("Harbour (feat. Tamsin Kell)", &["Tamsin Kell"], "Harbour"),
        ("Sloth (feat. Oda Brenn) [Live]", &["Oda Brenn"], "Sloth [Live]"),
        // "Friends" is F54's placeholder: no featured credit, so the title stays whole.
        ("Song (feat. Friends)", &[], "Song (feat. Friends)"),
        ("Tide (feat. Oda Brenn)", &[], "Tide (feat. Oda Brenn)"),
        // A guest the credit doesn't name.
        ("Tide (feat. Oda Brenn)", &["Tamsin Kell"], "Tide (feat. Oda Brenn)"),
        // Square brackets too; every other part kept.
        ("Night (Remix) [feat. Oda Brenn]", &["oda brenn"], "Night (Remix)"),
        ("Harbour (Featuring Tamsin Kell)", &["Tamsin Kell"], "Harbour"),
        // A featured part one name, whole.
        ("Embrace (feat. Kerys Lae & Loïc Brenn)", &["Kerys Lae & Loïc Brenn"], "Embrace"),
    ];

    #[test]
    fn loosely_accents_brackets_feat_and_the_count_for_nothing() {
        assert_eq!(loose("Beyoncé feat. Jay-Z"), "beyonce");
        assert_eq!(loose("The Wall (Remastered 2011)"), "wall");
        assert_eq!(loose("Simon & Garfunkel"), "simonandgarfunkel");
        assert_eq!(loose("The The"), "the", "a name that is only \"the\" keeps it");
        assert_eq!(loose("(Untitled)"), "untitled", "nothing but brackets: its own key");
    }
}
