//! Genres (decision 0097, amended 2026-11-20): a hand-kept set of 26, each with the words that mean it, the exceptions
//! read before the split, and the links that give a style its genres. One list for the server, the page and a device:
//! `shared/genres.json`, compiled in ([`SHIPPED`]) and read once ([`shipped`]).
//!
//! - **A word's genre** ([`Genres::genre_of`]): by its [`key`], as search reads words ("Hip-Hop", "HipHop" and "Rap" are
//!   Hip Hop; "R'n'B" is R&B). Any other word a caller names a genre or a style is a style, the rest are tags: that sort
//!   is the words store's.
//! - **The one split** ([`Genres::split`]): every words field (GENRE, STYLE, MOOD, Grouping) and every service's stored
//!   word, on every path.
//! - **The links** ([`Genres::links`]): the genres a style counts toward, never shown.
//!
//! Read by hand, as the matcher's JSON is ([`crate::matcher::json`]): no serde_json in the page's WebAssembly.

use std::sync::OnceLock;

use rustc_hash::FxHashMap;

use crate::matcher::json::{self, Value};

/// The words fields' separators (the owner, 2026-11-20: "split everywhere"). Never "&".
pub const SEPARATORS: [char; 3] = ['/', ';', ','];

/// `shared/genres.json` as Cratarr ships it.
pub const SHIPPED: &str = include_str!("../../../shared/genres.json");

/// One of the genres, and the words that mean it (its name is one too).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Genre {
    pub name: String,
    pub words: Vec<String>,
}

/// A written form read as the words it is, before the split ("RnB/Swing": R&B and New Jack Swing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exception {
    pub value: String,
    pub words: Vec<String>,
}

/// The genres a style counts toward (none: it gives none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub style: String,
    pub genres: Vec<String>,
}

/// A list of genres: the genres with their words, the exceptions and the links, each word's genre found by its key.
#[derive(Debug, Clone)]
pub struct Genres {
    genres: Vec<Genre>,
    exceptions: Vec<Exception>,
    links: Vec<Link>,
    /// Each word's key, each name's too, to its genre's place in `genres`.
    by_key: FxHashMap<String, usize>,
}

/// The list Cratarr ships ([`SHIPPED`]), read once.
pub fn shipped() -> &'static Genres {
    static LIST: OnceLock<Genres> = OnceLock::new();
    LIST.get_or_init(|| Genres::from_json(SHIPPED).unwrap_or_else(|e| panic!("shared/genres.json: {e}")))
}

/// [`Genres::split`] by the list Cratarr ships.
pub fn split(value: &str, keep: &[String]) -> Vec<String> {
    shipped().split(value, keep)
}

/// A word's key: its words as search reads them ([`crate::matcher::words`]), so "Hip-Hop", "hip hop" and "HIP HOP" are
/// one, and "R&B" is "r b". The words store's key too.
pub fn key(word: &str) -> String {
    crate::matcher::words(word).join(" ")
}

impl Genres {
    /// A list from its JSON, in `shared/genres.json`'s shape: `{genres: [{name, words}], exceptions: [{value, words}],
    /// links: [{style, genres}]}`, an `about` beside them. Refused: any other field (no family above the genres), no
    /// genres, a word of two genres or of no letters, an exception of no words, a link from a genre's word or to anything
    /// but one of the genres' names, a style linked twice.
    pub fn from_json(source: &str) -> Result<Self, String> {
        let v = json::parse(source.as_bytes()).ok_or("not JSON")?;
        only(&v, &["about", "genres", "exceptions", "links"], "the list")?;
        if v.get("genres").is_none() {
            return Err("no genres".to_string());
        }
        let mut genres: Vec<Genre> = Vec::new();
        let mut by_key: FxHashMap<String, usize> = FxHashMap::default();
        for g in items(&v, "genres")? {
            only(g, &["name", "words"], "a genre")?;
            let genre = Genre { name: one(g, "name", "a genre")?, words: many(g, "words", "a genre")? };
            let at = genres.len();
            for w in std::iter::once(&genre.name).chain(&genre.words) {
                let k = key(w);
                if k.is_empty() {
                    return Err(format!("{}: \"{w}\" has no letters", genre.name));
                }
                if let Some(other) = by_key.insert(k, at) {
                    if other != at {
                        return Err(format!("\"{w}\" is a word of {} and of {}", genres[other].name, genre.name));
                    }
                }
            }
            genres.push(genre);
        }
        let mut exceptions: Vec<Exception> = Vec::new();
        for e in items(&v, "exceptions")? {
            only(e, &["value", "words"], "an exception")?;
            let exception = Exception { value: one(e, "value", "an exception")?, words: many(e, "words", "an exception")? };
            if exception.words.is_empty() {
                return Err(format!("the exception \"{}\" is no words", exception.value));
            }
            exceptions.push(exception);
        }
        let mut links: Vec<Link> = Vec::new();
        for l in items(&v, "links")? {
            only(l, &["style", "genres"], "a link")?;
            let link = Link { style: one(l, "style", "a link")?, genres: many(l, "genres", "a link")? };
            let k = key(&link.style);
            if k.is_empty() || by_key.contains_key(&k) {
                return Err(format!("a link from \"{}\": a link goes from a style", link.style));
            }
            if links.iter().any(|o| key(&o.style) == k) {
                return Err(format!("\"{}\" is linked twice", link.style));
            }
            if let Some(g) = link.genres.iter().find(|g| !genres.iter().any(|x| &x.name == *g)) {
                return Err(format!("a link from \"{}\" to \"{g}\": not one of the genres", link.style));
            }
            links.push(link);
        }
        Ok(Self { genres, exceptions, links, by_key })
    }

    /// The genres, in the list's order.
    pub fn genres(&self) -> &[Genre] {
        &self.genres
    }

    /// The exceptions, in the list's order.
    pub fn exceptions(&self) -> &[Exception] {
        &self.exceptions
    }

    /// The links, in the list's order.
    pub fn links(&self) -> &[Link] {
        &self.links
    }

    /// The genre a word is (one of the genres' names), by its [`key`]; none for any other word.
    pub fn genre_of(&self, word: &str) -> Option<&str> {
        self.genre_of_key(&key(word))
    }

    /// [`Genres::genre_of`] for a word whose key is made already.
    pub fn genre_of_key(&self, k: &str) -> Option<&str> {
        self.by_key.get(k).map(|&at| self.genres[at].name.as_str())
    }

    /// A words field's value as its words: the one split, on every path. A library's keep list (`keep`: its own forms
    /// kept whole) is read first, ignoring case and inside a longer value, longest first; then the rest splits on "/",
    /// ";" and ",". Each word trimmed, none empty; a repeat (ignoring case) dropped, its first spelling kept.
    pub fn split(&self, value: &str, keep: &[String]) -> Vec<String> {
        let mut taken: Vec<Taken> = Vec::new();
        let mut kept: Vec<&str> = keep.iter().map(|k| k.trim()).filter(|k| !k.is_empty()).collect();
        kept.sort_by_key(|k| std::cmp::Reverse(k.len()));
        for k in kept {
            take(value, k, None, &mut taken);
        }
        taken.sort_by_key(|t| t.from);
        let mut words: Vec<String> = Vec::new();
        let mut part = String::new();
        let mut next = taken.iter().peekable();
        let mut at = 0;
        while let Some(c) = value[at..].chars().next() {
            if let Some(t) = next.next_if(|t| t.from == at) {
                match t.exception {
                    Some(e) => {
                        push_word(&mut words, &part);
                        part.clear();
                        words.extend(self.exceptions[e].words.iter().cloned());
                    }
                    None => part.push_str(&value[t.from..t.to]),
                }
                at = t.to;
            } else {
                if SEPARATORS.contains(&c) {
                    push_word(&mut words, &part);
                    part.clear();
                } else {
                    part.push(c);
                }
                at += c.len_utf8();
            }
        }
        push_word(&mut words, &part);
        dedupe(words)
    }
}

/// A stretch of a value that one form takes: its bytes, and the exception it is (none: a kept form).
struct Taken {
    from: usize,
    to: usize,
    exception: Option<usize>,
}

/// Every stretch of `value` that `form` matches, ignoring case, where no form before it took one.
fn take(value: &str, form: &str, exception: Option<usize>, taken: &mut Vec<Taken>) {
    if form.is_empty() {
        return;
    }
    let mut at = 0;
    while let Some(c) = value[at..].chars().next() {
        match matched(value, at, form) {
            Some(len) if !taken.iter().any(|t| at < t.to && t.from < at + len) => {
                taken.push(Taken { from: at, to: at + len, exception });
                at += len;
            }
            _ => at += c.len_utf8(),
        }
    }
}

/// How many bytes of `value`, from `at`, `form` matches ignoring case; none where it doesn't.
fn matched(value: &str, at: usize, form: &str) -> Option<usize> {
    let mut chars = value[at..].chars();
    let mut len = 0;
    for f in form.chars() {
        let c = chars.next()?;
        if c != f && !c.to_lowercase().eq(f.to_lowercase()) {
            return None;
        }
        len += c.len_utf8();
    }
    Some(len)
}

/// A part as a word, trimmed, if anything is left of it.
fn push_word(words: &mut Vec<String>, part: &str) {
    let word = part.trim();
    if !word.is_empty() {
        words.push(word.to_string());
    }
}

/// Each word once, ignoring case, its first spelling kept, in order.
fn dedupe(words: Vec<String>) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    words
        .into_iter()
        .filter(|w| {
            let k = w.to_lowercase();
            let new = !seen.contains(&k);
            if new {
                seen.push(k);
            }
            new
        })
        .collect()
}

/// An object's fields: these and none other.
fn only(v: &Value, fields: &[&str], what: &str) -> Result<(), String> {
    match v {
        Value::Obj(have) => match have.iter().find(|(k, _)| !fields.contains(&k.as_str())) {
            Some((k, _)) => Err(format!("{what}: no field \"{k}\"")),
            None => Ok(()),
        },
        _ => Err(format!("{what}: an object")),
    }
}

/// A field's items: none where there is no such field.
fn items<'a>(v: &'a Value, field: &str) -> Result<&'a [Value], String> {
    match v.get(field) {
        None => Ok(&[]),
        Some(list) => list.as_arr().ok_or_else(|| format!("\"{field}\": a list")),
    }
}

/// A field's text, trimmed, not empty.
fn one(v: &Value, field: &str, what: &str) -> Result<String, String> {
    v.get(field).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(str::to_string).ok_or_else(|| format!("{what}: \"{field}\", a text"))
}

/// A field's texts, each trimmed, none empty.
fn many(v: &Value, field: &str, what: &str) -> Result<Vec<String>, String> {
    let list = v.get(field).and_then(Value::as_arr).ok_or_else(|| format!("{what}: \"{field}\", a list of texts"))?;
    list.iter()
        .map(|t| t.as_str().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string).ok_or_else(|| format!("{what}: \"{field}\", a list of texts")))
        .collect()
}
