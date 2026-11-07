//! Finding in what a device keeps, with no Cratarr to ask (decision 0101: the library's tier only): its artists, records and tracks by
//! the words, as the server's search reads them and in the order it puts them ([`matcher::typos_in`], [`matcher::rank_in`]). A
//! [`matcher::Index`] reads each name into words, letters and trigrams, far more than a device should hold for a library's tracks, so
//! here a row is one line of folded text, read at each search; what only trigrams find stays the server's. An artist's panel is here
//! too, as what a search finds opens it.

use std::collections::{HashMap, HashSet};

use super::read::{Made, Object, quoted};
use super::{Index, Packed, album_id, artist_id, to_small};
use crate::matcher::{self, json};

/// Between what a row is found by. No word holds it.
const APART: char = '\n';

/// Each row's names, folded: its own first, then what else it is found by.
#[derive(Default)]
struct Lines {
    all: String,
    ends: Vec<u32>,
}

impl Lines {
    fn add(&mut self, names: &[&str]) {
        for name in names {
            self.all.push_str(&matcher::fold(name));
            self.all.push(APART);
        }
        self.ends.push(self.all.len() as u32);
    }

    fn len(&self) -> usize {
        self.ends.len()
    }

    fn line(&self, row: usize) -> &str {
        let from = if row == 0 { 0 } else { self.ends[row - 1] as usize };
        &self.all[from..self.ends[row] as usize]
    }

    /// The row's own name.
    fn own(&self, row: usize) -> &str {
        self.line(row).split(APART).next().unwrap_or_default()
    }
}

fn words_of(line: &str) -> Vec<&str> {
    matcher::words_in(line).collect()
}

/// What a search reads. Made at the first, as a device may never search without Cratarr, and it weighs about what the names do.
#[derive(Default)]
pub struct Finder {
    artists: Lines,
    records: Lines,
    tracks: Lines,
}

/// What is asked. `kinds`: of `artist`, `album`, `track`, as the server names them, none for all of them.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Ask {
    pub q: String,
    pub kinds: Vec<String>,
    pub limit: usize,
    pub offset: usize,
    /// The libraries switched off, by key: a record in none other is left out, with its tracks.
    pub without: Vec<String>,
}

impl Ask {
    /// From JSON, as the page writes it; none where it says no `q`.
    pub fn read(bytes: &[u8]) -> Option<Self> {
        let v = json::parse(bytes)?;
        let texts = |key: &str| v.get(key).and_then(json::Value::as_arr).map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let number = |key: &str| v.get(key).and_then(json::Value::as_f64).map_or(0, |n| n as usize);
        Some(Self { q: v.get("q")?.as_str()?.to_string(), kinds: texts("kinds"), limit: number("limit"), offset: number("offset"), without: texts("without") })
    }

    fn wants(&self, kind: &str) -> bool {
        self.kinds.is_empty() || self.kinds.iter().any(|k| k == kind)
    }
}

/// The rows the words find, best first, each with its rank. Its order among equals is the rows', as the server's is the order its
/// tables gave.
fn found(lines: &Lines, words: &[String], off: impl Fn(usize) -> bool) -> Vec<(usize, f32)> {
    let rows: Vec<usize> = (0..lines.len()).filter(|&row| !off(row) && matcher::typos_in(&words_of(lines.line(row)), words).is_some()).collect();
    let of = rows.len();
    let mut out: Vec<(usize, f32)> = rows.into_iter().enumerate().map(|(at, row)| (row, matcher::rank_in(&words_of(lines.own(row)), words, at, of))).collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    out
}

/// `"name":{"total":…,"rows":[…]}`, the rows of the page asked for. `say`: one row.
fn group(out: &mut String, name: &str, hits: &[(usize, f32)], ask: &Ask, mut say: impl FnMut(&mut Object, usize, f32)) {
    if out.len() > 1 {
        out.push(',');
    }
    quoted(out, name);
    out.push_str(":{\"total\":");
    out.push_str(&hits.len().to_string());
    out.push_str(",\"rows\":[");
    for (at, (row, rank)) in hits.iter().skip(ask.offset).take(ask.limit).enumerate() {
        if at > 0 {
            out.push(',');
        }
        let mut o = Object::of(out);
        say(&mut o, *row, *rank);
        o.end();
    }
    out.push_str("]}");
}

/// What a hit says it was found by: its own name, or what else it goes under.
fn found_on(own: &str, words: &[String]) -> &'static str {
    if matcher::typos_in(&words_of(own), words).is_some() { "name" } else { "artist" }
}

impl Packed {
    /// Whether each record lies only in libraries switched off, by its row.
    fn records_off(&self, without: &[String]) -> Vec<bool> {
        let (r, c, l) = (&self.records, &self.copies, &self.libraries);
        if without.is_empty() {
            return vec![false; r.id.len()];
        }
        let off: HashSet<i64> = (0..l.id.len()).filter(|&row| without.iter().any(|key| key == self.words.word(l.key[row]))).map(|row| l.id[row]).collect();
        r.id.iter()
            .map(|id| {
                let copies = c.record.partition_point(|at| at < id)..c.record.partition_point(|at| at <= id);
                !copies.is_empty() && c.library[copies].iter().all(|library| off.contains(library))
            })
            .collect()
    }

    /// Each record's row by a track's row.
    fn record_rows(&self) -> Vec<Option<usize>> {
        self.tracks.record.iter().map(|record| self.records.id.binary_search(record).ok()).collect()
    }

    /// Newest first, as an artist's panel is, on the server too: a record with no year last.
    fn newest_first(&self, a: usize, b: usize) -> std::cmp::Ordering {
        let r = &self.records;
        to_small(r.year[b]).cmp(&to_small(r.year[a])).then_with(|| r.sort.get(a).cmp(r.sort.get(b))).then(a.cmp(&b))
    }

    fn say_records(&self, out: &mut String, rows: &[usize], keys: &HashMap<i64, &str>, made: &HashMap<i64, Made>) {
        out.push('[');
        for (at, row) in rows.iter().enumerate() {
            if at > 0 {
                out.push(',');
            }
            self.say_record(out, *row, keys, made.get(&self.records.id[*row]).copied().unwrap_or(Made { tracks: 0, seconds: None }));
        }
        out.push(']');
    }
}

impl Index {
    /// What a search reads of this index.
    pub fn finder(&self) -> Finder {
        let p = &self.packed;
        let (a, r, t, w) = (&p.artists, &p.records, &p.tracks, &p.words);
        let mut out = Finder::default();
        for row in 0..a.id.len() {
            let names: Vec<&str> = std::iter::once(&a.name[row]).chain(a.spelled.of(row)).map(|name| w.word(*name)).collect();
            out.artists.add(&names);
        }
        for row in 0..r.id.len() {
            out.records.add(&[r.title.get(row), w.word(r.artist[row])]);
        }
        let records = p.record_rows();
        for row in 0..t.id.len() {
            out.tracks.add(&[t.title.get(row), w.word(t.artist[row]), records[row].map_or("", |at| r.title.get(at))]);
        }
        out
    }

    /// What the words find, of each kind asked: `{artists, records, tracks}`, each `{total, rows}`. As rows, for the page to say them in
    /// its own shapes (lib/deviceReads).
    pub fn found_json(&self, finder: &Finder, ask: &Ask) -> String {
        let p = &self.packed;
        let (a, r, t, w) = (&p.artists, &p.records, &p.tracks, &p.words);
        let words = matcher::words(&ask.q);
        let off = p.records_off(&ask.without);
        let mut out = String::from("{");
        if ask.wants("artist") {
            // Their records with a library on, as the server counts them with none switched off.
            let mut records: HashMap<i64, u32> = HashMap::new();
            for row in (0..r.id.len()).filter(|row| !off[*row]) {
                for artist in r.artist_ids.of(row) {
                    *records.entry(*artist).or_default() += 1;
                }
            }
            group(&mut out, "artists", &found(&finder.artists, &words, |_| false), ask, |o, row, rank| {
                o.text("id", &artist_id(a.id[row]));
                o.text("name", w.word(a.name[row]));
                o.number("records", Some(records.get(&a.id[row]).copied().unwrap_or(0)));
                o.number("score", Some(rank));
            });
        }
        if ask.wants("album") {
            group(&mut out, "records", &found(&finder.records, &words, |row| off[row]), ask, |o, row, rank| {
                o.text("id", &album_id(r.id[row]));
                o.text("title", r.title.get(row));
                o.text("artist", w.word(r.artist[row]));
                o.number("year", to_small(r.year[row]));
                o.number("score", Some(rank));
                o.text("on", found_on(finder.records.own(row), &words));
            });
        }
        if ask.wants("track") {
            let records = p.record_rows();
            let hits = found(&finder.tracks, &words, |row| records[row].is_some_and(|at| off[at]));
            group(&mut out, "tracks", &hits, ask, |o, row, rank| {
                p.track_fields(o, row);
                o.text("record", &album_id(t.record[row]));
                o.text("album", records[row].map_or("", |at| r.title.get(at)));
                o.number("score", Some(rank));
                o.text("on", found_on(finder.tracks.own(row), &words));
            });
        }
        out.push('}');
        out
    }

    /// An artist as their panel shows them, by any name the files give them: the records credited to them and those they only appear
    /// on, newest first, and their tracks record by record: `{albums, appears_on, tracks}`. None where no artist goes by it.
    pub fn artist_json(&self, name: &str, without: &[String]) -> Option<String> {
        let p = &self.packed;
        let (a, r, t) = (&p.artists, &p.records, &p.tracks);
        let at = (0..a.id.len()).find(|&row| std::iter::once(&a.name[row]).chain(a.spelled.of(row)).any(|n| p.words.word(*n) == name))?;
        let id = a.id[at];
        let off = p.records_off(without);
        let places = p.record_rows();
        let mut own: Vec<usize> = (0..r.id.len()).filter(|row| !off[*row] && r.artist_ids.of(*row).contains(&id)).collect();
        let theirs: HashSet<usize> = own.iter().copied().collect();
        let credited = |row: usize| t.artist_ids.of(row).contains(&id);
        let mut on: Vec<usize> = (0..t.id.len()).filter(|row| credited(*row)).filter_map(|row| places[row]).filter(|row| !off[*row] && !theirs.contains(row)).collect::<HashSet<_>>().into_iter().collect();
        own.sort_unstable_by(|a, b| p.newest_first(*a, *b));
        on.sort_unstable_by(|a, b| p.newest_first(*a, *b));
        let mut tracks: Vec<(usize, usize)> = (0..t.id.len()).filter_map(|row| places[row].filter(|at| !off[*at] && (theirs.contains(at) || credited(row))).map(|at| (at, row))).collect();
        tracks.sort_unstable_by(|&(ra, a), &(rb, b)| {
            p.newest_first(ra, rb).then(t.disc[a].cmp(&t.disc[b])).then(t.number[a].cmp(&t.number[b])).then_with(|| t.line.get(a).cmp(t.line.get(b)))
        });
        let (keys, made) = (p.library_keys(), p.made());
        let mut out = String::with_capacity((own.len() + on.len()) * 400 + tracks.len() * 200);
        out.push_str("{\"albums\":");
        p.say_records(&mut out, &own, &keys, &made);
        out.push_str(",\"appears_on\":");
        p.say_records(&mut out, &on, &keys, &made);
        out.push_str(",\"tracks\":[");
        for (at, (record, row)) in tracks.into_iter().enumerate() {
            if at > 0 {
                out.push(',');
            }
            let mut o = Object::of(&mut out);
            p.track_fields(&mut o, row);
            o.text("record", &album_id(t.record[row]));
            o.text("album", r.title.get(record));
            o.end();
        }
        out.push_str("]}");
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Artist, Index, Library, Record, RecordCopy, Rows, Track};
    use super::Ask;
    use crate::crates::Facts;

    fn track(id: i64, record: i64, number: u32, title: &str, artist: &str, artist_ids: &[i64]) -> Track {
        Track {
            id,
            record,
            number: Some(number),
            title: title.into(),
            artist: Some(artist.into()),
            artist_ids: artist_ids.to_vec(),
            facts: Facts { rel: format!("r{record}/{number}.flac"), folders: vec![1], length: Some(60.0), ..Facts::default() },
            ..Track::default()
        }
    }

    fn record(id: i64, title: &str, artist: &str, artist_ids: &[i64], year: u32, library: i64) -> Record {
        Record {
            id,
            title: title.into(),
            sort: title.to_lowercase(),
            artist: artist.into(),
            artist_ids: artist_ids.to_vec(),
            year: Some(year),
            copies: vec![RecordCopy { library, dir: format!("r{id}"), tracks: 1, ..RecordCopy::default() }],
            ..Record::default()
        }
    }

    fn rows() -> Rows {
        Rows {
            libraries: vec![
                Library { id: 1, key: "lib/one".into(), name: "One".into(), ..Library::default() },
                Library { id: 2, key: "lib/two".into(), name: "Two".into(), ..Library::default() },
            ],
            artists: vec![
                Artist { id: 7, name: "Miles Davis".into(), sort: "davis".into(), spelled: vec!["Davis Quintet".into()] },
                Artist { id: 8, name: "Björk".into(), sort: "bjork".into(), spelled: Vec::new() },
            ],
            records: vec![
                record(3, "Kind of Blue", "Miles Davis", &[7], 1959, 1),
                record(4, "Debut", "Björk", &[8], 1993, 2),
                record(5, "Blue Notes", "Various", &[], 2001, 1),
            ],
            tracks: vec![
                track(1, 3, 1, "So What", "Miles Davis", &[7]),
                track(2, 3, 2, "Blue in Green", "Miles Davis", &[7]),
                track(3, 4, 1, "Human Behaviour", "Björk", &[8]),
                track(4, 5, 1, "Blue Train", "Miles Davis", &[7]),
                track(5, 5, 2, "Other", "Someone", &[]),
            ],
        }
    }

    fn ask(q: &str) -> Ask {
        Ask { q: q.into(), limit: 5, ..Ask::default() }
    }

    fn find(a: &Ask) -> serde_json::Value {
        let index = Index::build(4, &rows()).unwrap();
        serde_json::from_str(&index.found_json(&index.finder(), a)).unwrap()
    }

    fn names(found: &serde_json::Value, group: &str, field: &str) -> Vec<String> {
        found[group]["rows"].as_array().unwrap().iter().map(|r| r[field].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn an_artist_is_found_by_their_name_with_their_records_counted() {
        let found = find(&ask("miles"));
        assert_eq!(names(&found, "artists", "name"), ["Miles Davis"]);
        assert_eq!(found["artists"]["rows"][0]["id"], "ar-7");
        assert_eq!(found["artists"]["rows"][0]["records"], 1);
    }

    #[test]
    fn an_artist_is_found_by_another_spelling_the_files_give() {
        assert_eq!(names(&find(&ask("quintet")), "artists", "name"), ["Miles Davis"]);
    }

    #[test]
    fn marks_in_a_name_do_not_count_for_the_one_searching_without_them() {
        assert_eq!(names(&find(&ask("bjork")), "artists", "name"), ["Björk"]);
    }

    #[test]
    fn a_word_with_a_typo_within_its_budget_is_found() {
        assert_eq!(names(&find(&ask("behavour")), "tracks", "title"), ["Human Behaviour"]);
    }

    #[test]
    fn a_word_too_far_from_any_name_finds_nothing() {
        let found = find(&ask("zzzz"));
        assert_eq!(found["tracks"]["total"], 0);
        assert_eq!(found["records"]["total"], 0);
        assert_eq!(found["artists"]["total"], 0);
    }

    #[test]
    fn no_words_find_nothing() {
        assert_eq!(find(&ask(" - "))["tracks"]["total"], 0);
    }

    #[test]
    fn a_record_is_found_by_its_title_or_its_artist_and_says_which() {
        let found = find(&ask("kind"));
        assert_eq!(names(&found, "records", "title"), ["Kind of Blue"]);
        assert_eq!(found["records"]["rows"][0]["on"], "name");
        assert_eq!(found["records"]["rows"][0]["year"], 1959);
        let by_artist = find(&ask("bjork"));
        assert_eq!(names(&by_artist, "records", "title"), ["Debut"]);
        assert_eq!(by_artist["records"]["rows"][0]["on"], "artist");
    }

    #[test]
    fn a_track_is_found_across_its_title_its_artist_and_its_record() {
        assert_eq!(names(&find(&ask("so what davis")), "tracks", "title"), ["So What"]);
        assert_eq!(names(&find(&ask("human debut")), "tracks", "title"), ["Human Behaviour"]);
    }

    #[test]
    fn a_track_says_its_record_and_what_it_was_found_by() {
        let found = find(&ask("so what"));
        let row = &found["tracks"]["rows"][0];
        assert_eq!(row["id"], "tr-1");
        assert_eq!(row["record"], "al-3");
        assert_eq!(row["album"], "Kind of Blue");
        assert_eq!(row["seconds"], 60.0);
        assert_eq!(row["on"], "name");
        assert_eq!(find(&ask("miles"))["tracks"]["rows"][0]["on"], "artist");
    }

    #[test]
    fn what_the_name_holds_comes_before_what_it_is_found_through() {
        // Blue Notes' Other has the word in its record's title alone.
        assert_eq!(names(&find(&ask("blue")), "tracks", "title"), ["Blue Train", "Blue in Green", "So What", "Other"]);
    }

    #[test]
    fn a_whole_name_comes_before_one_it_only_begins() {
        let mut more = rows();
        more.records.push(record(2, "Blue Notes Again", "Various", &[], 2002, 1));
        more.tracks.push(track(6, 2, 1, "Again", "Various", &[]));
        let index = Index::build(4, &more).unwrap();
        let found: serde_json::Value = serde_json::from_str(&index.found_json(&index.finder(), &ask("blue notes"))).unwrap();
        assert_eq!(names(&found, "records", "title"), ["Blue Notes", "Blue Notes Again"], "its row comes after, and still it comes first");
    }

    #[test]
    fn only_the_kinds_asked_are_read() {
        let found = find(&Ask { kinds: vec!["album".into()], ..ask("blue") });
        assert!(found.get("records").is_some());
        assert!(found.get("tracks").is_none());
        assert!(found.get("artists").is_none());
    }

    #[test]
    fn a_page_of_results_says_how_many_there_are_in_all() {
        let found = find(&Ask { limit: 1, offset: 1, ..ask("blue") });
        assert_eq!(found["tracks"]["total"], 4);
        assert_eq!(names(&found, "tracks", "title"), ["Blue in Green"]);
    }

    #[test]
    fn a_library_switched_off_takes_its_records_and_their_tracks_out() {
        let found = find(&Ask { without: vec!["lib/two".into()], ..ask("bjork") });
        assert_eq!(found["records"]["total"], 0);
        assert_eq!(found["tracks"]["total"], 0);
        assert_eq!(found["artists"]["rows"][0]["records"], 0, "the artist stays, with nothing to their name here");
    }

    #[test]
    fn a_record_with_a_copy_in_a_library_on_stays() {
        let mut both = rows();
        both.records[1].copies.push(RecordCopy { library: 1, dir: "again".into(), tracks: 1, ..RecordCopy::default() });
        let index = Index::build(4, &both).unwrap();
        let found: serde_json::Value = serde_json::from_str(&index.found_json(&index.finder(), &Ask { without: vec!["lib/two".into()], ..ask("debut") })).unwrap();
        assert_eq!(found["records"]["total"], 1);
    }

    #[test]
    fn an_ask_is_read_from_what_the_page_writes() {
        let read = Ask::read(br#"{"q":"blue","kinds":["album"],"limit":5,"offset":2,"without":["lib/two"]}"#).unwrap();
        assert_eq!(read, Ask { q: "blue".into(), kinds: vec!["album".into()], limit: 5, offset: 2, without: vec!["lib/two".into()] });
        assert_eq!(Ask::read(b"{}"), None);
        assert_eq!(Ask::read(b"not json"), None);
    }

    fn artist(name: &str, without: &[String]) -> Option<serde_json::Value> {
        Index::build(4, &rows()).unwrap().artist_json(name, without).map(|json| serde_json::from_str(&json).unwrap())
    }

    fn ids(found: &serde_json::Value, group: &str) -> Vec<String> {
        found[group].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn an_artists_panel_has_their_records_those_they_appear_on_and_their_tracks() {
        let them = artist("Miles Davis", &[]).unwrap();
        assert_eq!(ids(&them, "albums"), ["al-3"]);
        assert_eq!(ids(&them, "appears_on"), ["al-5"]);
        assert_eq!(ids(&them, "tracks"), ["tr-4", "tr-1", "tr-2"], "newest record first, each in the order it plays, and not a track of a record they only appear on that is not theirs");
    }

    #[test]
    fn an_artists_track_says_its_record() {
        let them = artist("Miles Davis", &[]).unwrap();
        assert_eq!(them["tracks"][1]["record"], "al-3");
        assert_eq!(them["tracks"][1]["album"], "Kind of Blue");
    }

    #[test]
    fn an_artist_is_found_by_any_name_the_files_give_them() {
        assert_eq!(ids(&artist("Davis Quintet", &[]).unwrap(), "albums"), ["al-3"]);
    }

    #[test]
    fn a_name_nobody_goes_by_has_no_panel() {
        assert_eq!(artist("Nobody", &[]), None);
    }

    #[test]
    fn a_library_switched_off_leaves_out_what_lies_only_there() {
        let them = artist("Björk", &["lib/two".into()]).unwrap();
        assert!(them["albums"].as_array().unwrap().is_empty());
        assert!(them["tracks"].as_array().unwrap().is_empty());
        let guest = artist("Miles Davis", &["lib/one".into()]).unwrap();
        assert!(guest["appears_on"].as_array().unwrap().is_empty(), "a record they appear on, there too");
    }

    #[test]
    fn a_record_without_a_year_comes_after_those_with_one() {
        let mut more = rows();
        more.records[2].year = None;
        more.records.push(record(6, "Later", "Miles Davis", &[7], 1970, 1));
        more.tracks.push(track(6, 6, 1, "Late", "Miles Davis", &[7]));
        let index = Index::build(4, &more).unwrap();
        let them: serde_json::Value = serde_json::from_str(&index.artist_json("Miles Davis", &[]).unwrap()).unwrap();
        assert_eq!(ids(&them, "albums"), ["al-6", "al-3"]);
        more.records[0].artist_ids.clear();
        more.records[2].artist_ids.push(7);
        let index = Index::build(4, &more).unwrap();
        let them: serde_json::Value = serde_json::from_str(&index.artist_json("Miles Davis", &[]).unwrap()).unwrap();
        assert_eq!(ids(&them, "albums"), ["al-6", "al-5"]);
    }
}
