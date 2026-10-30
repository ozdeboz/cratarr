//! The matcher's JSON for the page (0.12: one matcher, the owner 2026-10-30): the shapes the WebAssembly exports read
//! and write, by hand (no serde_json in rules.wasm). Reads any JSON, keeps what it knows: `{names:[{text, aliases?}]}`
//! (a name may be a plain string too) and `{q, limit}`; writes `[{i, score, how}]`.

use super::{Hit, Name};

/// A JSON value, as read.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }
}

/// Deeper than this is refused (the page sends two levels).
const DEPTH: usize = 32;

/// One JSON value (whitespace around it), none for anything else.
pub fn parse(bytes: &[u8]) -> Option<Value> {
    let mut r = Reader { b: bytes, at: 0 };
    let v = r.value(0)?;
    r.space();
    (r.at == bytes.len()).then_some(v)
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.at).copied()
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn word(&mut self, w: &[u8]) -> Option<()> {
        self.b.get(self.at..self.at + w.len()).filter(|s| *s == w)?;
        self.at += w.len();
        Some(())
    }

    fn value(&mut self, depth: usize) -> Option<Value> {
        if depth > DEPTH {
            return None;
        }
        self.space();
        match self.peek()? {
            b'n' => self.word(b"null").map(|_| Value::Null),
            b't' => self.word(b"true").map(|_| Value::Bool(true)),
            b'f' => self.word(b"false").map(|_| Value::Bool(false)),
            b'"' => self.string().map(Value::Str),
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                self.space();
                if self.peek()? == b']' {
                    self.at += 1;
                    return Some(Value::Arr(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.space();
                    match self.peek()? {
                        b',' => self.at += 1,
                        b']' => {
                            self.at += 1;
                            return Some(Value::Arr(items));
                        }
                        _ => return None,
                    }
                }
            }
            b'{' => {
                self.at += 1;
                let mut fields = Vec::new();
                self.space();
                if self.peek()? == b'}' {
                    self.at += 1;
                    return Some(Value::Obj(fields));
                }
                loop {
                    self.space();
                    if self.peek()? != b'"' {
                        return None;
                    }
                    let k = self.string()?;
                    self.space();
                    if self.peek()? != b':' {
                        return None;
                    }
                    self.at += 1;
                    fields.push((k, self.value(depth + 1)?));
                    self.space();
                    match self.peek()? {
                        b',' => self.at += 1,
                        b'}' => {
                            self.at += 1;
                            return Some(Value::Obj(fields));
                        }
                        _ => return None,
                    }
                }
            }
            b'-' | b'0'..=b'9' => self.number().map(Value::Num),
            _ => None,
        }
    }

    fn digits(&mut self) -> usize {
        let from = self.at;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        self.at - from
    }

    /// A number, by its own arithmetic (exact for integers, which is all the page sends: `limit`).
    fn number(&mut self) -> Option<f64> {
        let negative = self.peek() == Some(b'-');
        if negative {
            self.at += 1;
        }
        let from = self.at;
        if self.digits() == 0 {
            return None;
        }
        let mut n = self.b[from..self.at].iter().fold(0.0f64, |n, d| n * 10.0 + f64::from(d - b'0'));
        if self.peek() == Some(b'.') {
            self.at += 1;
            let from = self.at;
            if self.digits() == 0 {
                return None;
            }
            let mut unit = 1.0;
            for d in &self.b[from..self.at] {
                unit /= 10.0;
                n += f64::from(d - b'0') * unit;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            let down = match self.peek() {
                Some(b'-') => {
                    self.at += 1;
                    true
                }
                Some(b'+') => {
                    self.at += 1;
                    false
                }
                _ => false,
            };
            let from = self.at;
            if self.digits() == 0 {
                return None;
            }
            let e = self.b[from..self.at].iter().fold(0u32, |e, d| e.saturating_mul(10).saturating_add(u32::from(d - b'0')));
            for _ in 0..e.min(400) {
                n = if down { n / 10.0 } else { n * 10.0 };
            }
        }
        Some(if negative { -n } else { n })
    }

    fn hex4(&mut self) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..4 {
            let d = match self.peek()? {
                c @ b'0'..=b'9' => c - b'0',
                c @ b'a'..=b'f' => c - b'a' + 10,
                c @ b'A'..=b'F' => c - b'A' + 10,
                _ => return None,
            };
            v = v * 16 + u32::from(d);
            self.at += 1;
        }
        Some(v)
    }

    /// A string at `"`: its escapes read (a pair of `\u` surrogates as one character, a lone one as U+FFFD), its bytes
    /// UTF-8 or none.
    fn string(&mut self) -> Option<String> {
        self.at += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = self.peek()?;
            self.at += 1;
            match c {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let e = self.peek()?;
                    self.at += 1;
                    let ch = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            if (0xD800..0xDC00).contains(&hi) && self.b.get(self.at..self.at + 2) == Some(&b"\\u"[..]) {
                                let back = self.at;
                                self.at += 2;
                                let lo = self.hex4()?;
                                if (0xDC00..0xE000).contains(&lo) {
                                    char::from_u32(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)).unwrap_or('\u{FFFD}')
                                } else {
                                    self.at = back;
                                    '\u{FFFD}'
                                }
                            } else {
                                char::from_u32(hi).unwrap_or('\u{FFFD}')
                            }
                        }
                        _ => return None,
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                0..=0x1F => return None,
                _ => out.push(c),
            }
        }
    }
}

/// The names in `{names:[{text, aliases?}]}` (or plain strings), none where `names` is not a list.
pub fn names_of(v: &Value) -> Option<Vec<Name>> {
    let names = v.get("names")?.as_arr()?;
    Some(
        names
            .iter()
            .map(|n| match n {
                Value::Str(s) => Name::new(s.clone()),
                _ => Name {
                    text: n.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
                    aliases: n.get("aliases").and_then(Value::as_arr).unwrap_or_default().iter().filter_map(Value::as_str).map(str::to_string).collect(),
                },
            })
            .collect(),
    )
}

/// A query's `{q, limit}`: its text (none, empty) and its limit (none: all; below 1: nothing).
pub fn ask_of(v: &Value) -> (String, usize) {
    let q = v.get("q").and_then(Value::as_str).unwrap_or_default().to_string();
    let limit = match v.get("limit").and_then(Value::as_f64) {
        None => usize::MAX,
        Some(n) if n >= 1.0 => n.min(1e9) as usize,
        Some(_) => 0,
    };
    (q, limit)
}

fn put_int(out: &mut String, mut n: u64) {
    let mut d = [0u8; 20];
    let mut k = d.len();
    loop {
        k -= 1;
        d[k] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    for &c in &d[k..] {
        out.push(char::from(c));
    }
}

/// Hits as `[{"i":0,"score":0.8123,"how":"exact"}]`: scores to four decimals.
pub fn hits(hits: &[Hit]) -> String {
    let mut out = String::from("[");
    for (k, h) in hits.iter().enumerate() {
        if k > 0 {
            out.push(',');
        }
        out.push_str("{\"i\":");
        put_int(&mut out, h.i as u64);
        out.push_str(",\"score\":");
        let s = (h.score.clamp(0.0, 1.0) * 10000.0 + 0.5) as u64;
        put_int(&mut out, s / 10000);
        out.push('.');
        let frac = s % 10000;
        for unit in [1000, 100, 10, 1] {
            out.push(char::from(b'0' + (frac / unit % 10) as u8));
        }
        out.push_str(",\"how\":\"");
        out.push_str(h.how.as_str());
        out.push_str("\"}");
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::{How, Index};

    #[test]
    fn the_pages_shapes_are_read() {
        let v = parse(br#" {"names": [{"text": "Drum and Bass", "aliases": ["DnB", "D\u0026B"]}, "Hip Hop", {"text": "Bj\u00f6rk \ud83c\udfb5"}, {}], "x": [1.5e2, -3, true, null]} "#).unwrap();
        let names = names_of(&v).unwrap();
        assert_eq!(names[0], Name { text: "Drum and Bass".into(), aliases: vec!["DnB".into(), "D&B".into()] });
        assert_eq!(names[1], Name::new("Hip Hop"));
        assert_eq!(names[2].text, "Björk \u{1F3B5}");
        assert_eq!(names[3], Name::new(""));
        assert_eq!(v.get("x").and_then(Value::as_arr).map(|x| x[0].clone()), Some(Value::Num(150.0)));
        assert_eq!(ask_of(&parse(br#"{"q":"dnb","limit":20}"#).unwrap()), ("dnb".to_string(), 20));
        assert_eq!(ask_of(&parse(br#"{"q":"dnb"}"#).unwrap()).1, usize::MAX);
        assert_eq!(ask_of(&parse(br#"{"q":"dnb","limit":0}"#).unwrap()).1, 0);
        let bad: [&[u8]; 8] = [b"", b"{", b"[1,]", b"{\"a\" 1}", b"\"\x01\"", b"nul", b"{} x", b"\"\\x\""];
        for bad in bad {
            assert_eq!(parse(bad), None, "{:?}", String::from_utf8_lossy(bad));
        }
        assert!(parse(&[b'['; 100]).is_none(), "too deep");
    }

    #[test]
    fn hits_are_written_for_the_page() {
        let out = hits(&[Hit { i: 3, score: 1.0, how: How::Exact }, Hit { i: 12, score: 0.81234, how: How::Fuzzy }, Hit { i: 0, score: 0.05, how: How::Prefix }]);
        assert_eq!(out, r#"[{"i":3,"score":1.0000,"how":"exact"},{"i":12,"score":0.8123,"how":"fuzzy"},{"i":0,"score":0.0500,"how":"prefix"}]"#);
        assert_eq!(hits(&[]), "[]");
        // The way the page goes: names in, a query in, hits out.
        let ix = Index::new(names_of(&parse(br#"{"names":[{"text":"Trip Hop"},{"text":"Hip Hop"}]}"#).unwrap()).unwrap());
        let (q, limit) = ask_of(&parse(br#"{"q":"hiphop","limit":5}"#).unwrap());
        assert_eq!(hits(&ix.query(&q, limit)), r#"[{"i":1,"score":1.0000,"how":"exact"}]"#);
    }
}
