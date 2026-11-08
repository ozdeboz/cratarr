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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loosely_accents_brackets_feat_and_the_count_for_nothing() {
        assert_eq!(loose("Beyoncé feat. Jay-Z"), "beyonce");
        assert_eq!(loose("The Wall (Remastered 2011)"), "wall");
        assert_eq!(loose("Simon & Garfunkel"), "simonandgarfunkel");
        assert_eq!(loose("The The"), "the", "a name that is only \"the\" keeps it");
        assert_eq!(loose("(Untitled)"), "untitled", "nothing but brackets: its own key");
    }
}
