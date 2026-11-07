//! What a device reads of its index, as JSON: how rows reach the page across WebAssembly, and across the bridge in the app. A row is
//! put together only when it is asked for, and the fields go by the names the page's lists give them. What a list says and an index
//! does not hold (what the viewer may do, plays, a record's own tags) is not here: the page's side says what it leaves out
//! (lib/deviceReads). Written by hand, as the rules take no JSON crate.

use std::collections::HashMap;
use std::fmt::{Display, Write};

use super::{Index, Kind, Packed, album_id, to_float, to_int, to_small};
use crate::ids;

/// `text` as JSON says it.
pub(super) fn quoted(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// An object's fields, each after the last.
pub(super) struct Object<'a> {
    out: &'a mut String,
    first: bool,
}

impl<'a> Object<'a> {
    pub(super) fn of(out: &'a mut String) -> Self {
        out.push('{');
        Self { out, first: true }
    }

    fn key(&mut self, name: &str) -> &mut String {
        if !self.first {
            self.out.push(',');
        }
        self.first = false;
        quoted(self.out, name);
        self.out.push(':');
        &mut *self.out
    }

    pub(super) fn text(&mut self, name: &str, value: &str) {
        quoted(self.key(name), value);
    }

    pub(super) fn maybe_text(&mut self, name: &str, value: Option<&str>) {
        match value {
            Some(v) => self.text(name, v),
            None => self.key(name).push_str("null"),
        }
    }

    /// A number, or null. A length here is never anything JSON cannot say: `to_float` lets no NaN through, and nothing writes infinity.
    pub(super) fn number<T: Display>(&mut self, name: &str, value: Option<T>) {
        let out = self.key(name);
        match value {
            Some(v) => {
                let _ = write!(out, "{v}");
            }
            None => out.push_str("null"),
        }
    }

    fn texts<'t>(&mut self, name: &str, values: impl IntoIterator<Item = &'t str>) {
        let out = self.key(name);
        out.push('[');
        for (at, value) in values.into_iter().enumerate() {
            if at > 0 {
                out.push(',');
            }
            quoted(out, value);
        }
        out.push(']');
    }

    pub(super) fn end(self) {
        self.out.push('}');
    }
}

/// What a record's tracks come to: how many, and their length, none where one of them has none.
#[derive(Clone, Copy)]
pub(super) struct Made {
    pub(super) tracks: u32,
    pub(super) seconds: Option<f64>,
}

impl Packed {
    /// Each library's key by its id, as a copy names it.
    pub(super) fn library_keys(&self) -> HashMap<i64, &str> {
        let c = &self.libraries;
        c.id.iter().enumerate().map(|(row, id)| (*id, self.words.word(c.key[row]))).collect()
    }

    /// What each record's tracks come to, by its id.
    pub(super) fn made(&self) -> HashMap<i64, Made> {
        let mut out: HashMap<i64, Made> = HashMap::with_capacity(self.records.id.len());
        for row in 0..self.tracks.id.len() {
            let length = to_float(self.tracks.length[row]);
            let made = out.entry(self.tracks.record[row]).or_insert(Made { tracks: 0, seconds: Some(0.0) });
            made.tracks += 1;
            made.seconds = made.seconds.zip(length).map(|(all, one)| all + one);
        }
        out
    }

    fn say_library(&self, out: &mut String, row: usize) {
        let l = self.library_at(row);
        let mut o = Object::of(out);
        o.number("id", Some(l.id));
        o.text("key", &l.key);
        o.text("name", &l.name);
        o.text(
            "kind",
            match l.kind {
                Kind::Main => "main",
                Kind::Own => "own",
                Kind::Incoming => "incoming",
            },
        );
        o.number("order", Some(l.order));
        o.maybe_text("colour", l.colour.as_deref());
        o.maybe_text("owner", l.owner.as_deref());
        o.end();
    }

    /// A record's row as a list holds it: its artists by name, each copy's library by its key, and what its tracks come to.
    pub(super) fn say_record(&self, out: &mut String, row: usize, keys: &HashMap<i64, &str>, made: Made) {
        let r = self.record_at(row);
        let mut o = Object::of(out);
        o.text("id", &album_id(r.id));
        o.text("title", &r.title);
        o.text("sort", &r.sort);
        o.text("artist", &r.artist);
        o.texts("artists", r.artist_ids.iter().filter_map(|id| self.artists.id.binary_search(id).ok()).map(|at| self.words.word(self.artists.name[at])));
        o.number("year", r.year);
        o.number("original_year", r.original_year);
        o.number("added", r.added);
        o.maybe_text("kind", r.kind.as_deref());
        o.texts("secondary", r.secondary.iter().map(String::as_str));
        o.texts("genres", r.genres.iter().map(String::as_str));
        o.number("whole_tracks", r.whole_tracks);
        o.number("tracks", Some(made.tracks));
        o.number("seconds", made.seconds);
        let copies = o.key("copies");
        copies.push('[');
        for (at, c) in r.copies.iter().enumerate() {
            if at > 0 {
                copies.push(',');
            }
            let mut o = Object::of(copies);
            o.text("library", keys.get(&c.library).copied().unwrap_or_default());
            o.text("dir", &c.dir);
            o.maybe_text("format", c.format.as_deref());
            o.number("kbps", c.min_kbps);
            o.number("bits", c.bits);
            o.number("rate", c.rate);
            o.number("size", c.size);
            o.number("added", c.added);
            o.number("tracks", Some(c.tracks));
            o.end();
        }
        copies.push(']');
        o.end();
    }

    fn say_track(&self, out: &mut String, row: usize) {
        let mut o = Object::of(out);
        self.track_fields(&mut o, row);
        o.end();
    }

    pub(super) fn track_fields(&self, o: &mut Object, row: usize) {
        let c = &self.tracks;
        o.text("id", &ids::song(c.id[row]));
        o.number("disc", to_small(c.disc[row]));
        o.number("number", to_small(c.number[row]));
        o.text("title", c.title.get(row));
        o.maybe_text("artist", self.words.maybe(c.artist[row]).as_deref());
        o.text("line", c.line.get(row));
        o.number("seconds", to_float(c.length[row]));
        o.number("year", to_float(c.year[row]));
        o.number("bpm", to_float(c.bpm[row]));
        o.number("born", to_int(c.born[row]));
    }
}

impl Index {
    /// Every library and record, as the Library's list takes them: `{rev, libraries, records}`.
    pub fn list_json(&self) -> String {
        let p = &self.packed;
        let (keys, made) = (p.library_keys(), p.made());
        let mut out = String::with_capacity(p.records.id.len() * 400);
        let _ = write!(out, "{{\"rev\":{},\"libraries\":[", self.rev);
        for row in 0..p.libraries.id.len() {
            if row > 0 {
                out.push(',');
            }
            p.say_library(&mut out, row);
        }
        out.push_str("],\"records\":[");
        for row in 0..p.records.id.len() {
            if row > 0 {
                out.push(',');
            }
            p.say_record(&mut out, row, &keys, made.get(&p.records.id[row]).copied().unwrap_or(Made { tracks: 0, seconds: None }));
        }
        out.push_str("]}");
        out
    }

    /// A record and its tracks in the order they play, as its panel takes them: `{record, tracks}`. None where it is not held.
    pub fn record_json(&self, id: i64) -> Option<String> {
        let p = &self.packed;
        let row = p.records.id.binary_search(&id).ok()?;
        let t = &p.tracks;
        let mut rows: Vec<usize> = (0..t.id.len()).filter(|row| t.record[*row] == id).collect();
        // None is the lowest here, so a track with no number comes first, as it does on the server.
        rows.sort_unstable_by(|&a, &b| t.disc[a].cmp(&t.disc[b]).then(t.number[a].cmp(&t.number[b])).then_with(|| t.line.get(a).cmp(t.line.get(b))));
        let made = Made { tracks: rows.len() as u32, seconds: rows.iter().map(|row| to_float(t.length[*row])).sum() };
        let mut out = String::with_capacity(rows.len() * 200 + 400);
        out.push_str("{\"record\":");
        p.say_record(&mut out, row, &p.library_keys(), made);
        out.push_str(",\"tracks\":[");
        for (at, row) in rows.into_iter().enumerate() {
            if at > 0 {
                out.push(',');
            }
            p.say_track(&mut out, row);
        }
        out.push_str("]}");
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Artist, Index, Library, Record, RecordCopy, Rows, Sounds, Track};
    use crate::crates::{Alike, Facts};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn rows() -> Rows {
        Rows {
            libraries: vec![Library { id: 1, key: "lib/one".into(), name: "One".into(), order: 0, colour: Some("#123".into()), ..Library::default() }],
            artists: vec![Artist { id: 7, name: "A \"B\"".into(), sort: "b".into(), spelled: Vec::new() }],
            records: vec![Record {
                id: 3,
                title: "T\\U".into(),
                sort: "t".into(),
                artist: "A \"B\"".into(),
                artist_ids: vec![7],
                year: Some(2000),
                genres: vec!["Jazz".into()],
                copies: vec![RecordCopy { library: 1, dir: "d".into(), format: Some("FLAC".into()), min_kbps: Some(900), tracks: 2, ..RecordCopy::default() }],
                ..Record::default()
            }],
            tracks: vec![
                Track { id: 2, record: 3, number: Some(2), title: "Two".into(), artist_ids: vec![7], facts: Facts { rel: "d/2.flac".into(), folders: vec![1], length: Some(10.5), ..Facts::default() }, ..Track::default() },
                Track { id: 1, record: 3, number: Some(1), title: "One\n".into(), artist_ids: vec![7], facts: Facts { rel: "d/1.flac".into(), folders: vec![1], length: Some(20.0), ..Facts::default() }, ..Track::default() },
            ],
        }
    }

    #[test]
    fn a_list_says_each_record_with_what_its_tracks_come_to_and_its_copies_by_their_librarys_key() {
        let list: serde_json::Value = serde_json::from_str(&Index::build(4, &rows()).unwrap().list_json()).unwrap();
        assert_eq!(list["rev"], 4);
        assert_eq!(list["libraries"][0]["key"], "lib/one");
        assert_eq!(list["libraries"][0]["kind"], "main");
        let record = &list["records"][0];
        assert_eq!(record["id"], "al-3");
        assert_eq!(record["title"], "T\\U");
        assert_eq!(record["artists"], serde_json::json!(["A \"B\""]));
        assert_eq!(record["tracks"], 2);
        assert_eq!(record["seconds"], 30.5);
        assert_eq!(record["copies"][0]["library"], "lib/one");
        assert_eq!(record["copies"][0]["kbps"], 900);
    }

    #[test]
    fn a_record_with_a_track_of_no_length_has_none_itself() {
        let mut none = rows();
        none.tracks[0].facts.length = None;
        let list: serde_json::Value = serde_json::from_str(&Index::build(4, &none).unwrap().list_json()).unwrap();
        assert_eq!(list["records"][0]["seconds"], serde_json::Value::Null);
        assert_eq!(list["records"][0]["tracks"], 2);
    }

    #[test]
    fn a_record_says_its_tracks_in_the_order_they_play_and_escapes_what_json_cannot_hold() {
        let index = Index::build(4, &rows()).unwrap();
        let read: serde_json::Value = serde_json::from_str(&index.record_json(3).unwrap()).unwrap();
        assert_eq!(read["record"]["id"], "al-3");
        let titles: Vec<&str> = read["tracks"].as_array().unwrap().iter().map(|t| t["title"].as_str().unwrap()).collect();
        assert_eq!(titles, ["One\n", "Two"]);
        assert_eq!(read["tracks"][0]["id"], "tr-1");
        assert_eq!(read["tracks"][0]["line"], "d/1.flac");
        assert_eq!(read["tracks"][1]["seconds"], 10.5);
    }

    /// What a device must keep to (the tablet held 173 MB in rules): `facts` made once, `Alike` and Radio's index made of them, and a
    /// sound is one vector among the three.
    #[test]
    fn facts_alike_and_radios_index_made_of_one_reading_hold_one_copy_of_each_sound() {
        let index = Index::build(4, &rows()).unwrap();
        let sounds = Sounds::build(4, &[(1, vec![0.6, 0.8]), (2, vec![1.0, 0.0])]);
        let facts = index.facts(Some(&sounds));
        let alike = Alike::of(&facts, &HashMap::new(), &index.artist_ids());
        let radio = index.radio(&facts, &HashMap::new());
        for f in &facts {
            assert_eq!(Arc::strong_count(f.sound.as_ref().unwrap()), 3, "the facts', the crates' library's and Radio's: one vector");
        }
        drop((alike, radio));
    }

    #[test]
    fn a_record_not_held_has_no_answer() {
        assert_eq!(Index::build(4, &rows()).unwrap().record_json(9), None);
    }
}
