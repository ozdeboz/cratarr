//! The library's index on a device (decisions 0085's step 4 and 0101's points 16 to 19; the owner, 2026-11-06: one packed
//! format, held in memory here, the store only bytes, a file in the app and IndexedDB in the page): the tracks, records,
//! artists and copies an account can see, every library it may switch on among them, so that a device browses, searches and
//! works out crates, Radio and Sounds like with no server.
//!
//! - **Rows go in, packed arrays stay.** The server reads [`Rows`] and makes an [`Index`] of them. A device holds columns from
//!   then on: a number a row in a plain array, what repeats as a number into one list of words. [`Index::save`] and
//!   [`Index::load`] move those arrays to bytes and back; a row is put together again only when asked for.
//! - **It is a copy, and it says what the server said.** A track's facts are the server's [`Facts`], field by field, and nothing
//!   is worked out again here, so a rule changed on the server needs no new app and a crate reads the same on both. Whatever
//!   cannot be read is refused ([`Refused`]) rather than read wrong, and the answer is always a fresh fetch, which loses nothing.
//! - **Rows name rows by their ids for life** (`tr-`, `al-`, `ar-`, a library's catalogue folder), so a change moves nothing else.
//! - **One library, one form.** Rows lie in the order of their ids, words in theirs, and what nothing names is not kept
//!   (`Packed::tidy`). So what was done to an index leaves no mark: changes applied save the very bytes a whole build of the
//!   result saves, and a device and the server can be told to agree by comparing them.
//! - **A record's copies travel with it**, in the order they would play for this account, and a track says which libraries
//!   hold it, so switching a library works offline. Which copy plays stays the server's to say.
//! - **What is an account's own stays out of it:** plays, favourites, playlists and fits are the caller's, as `World` and
//!   `radio::Song` take them. Rows are found by id or by their line ([`Facts::rel`]). What the server last worked out of them
//!   comes apart, an account's own ([`Marks`], and [`own::Own`] in them).
//! - **[`Sounds`] are kept apart** (0101's point 16): they change rarely and weigh more than all the rest.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::crates::Facts;
use crate::{ids, radio, similarity};

#[cfg(any(test, feature = "device"))]
pub mod device;
pub mod find;
pub mod own;
mod read;
pub mod sync;

/// What is written here, for the index and for changes to it. Raised by anything that changes a file's layout or what a row
/// means, a column added too. A device holding another version refuses it and fetches the index whole, so a new version is
/// this one line: there is nothing to migrate in a copy.
pub const VERSION: u32 = 1;

/// The same for the sounds, on their own: a column added to the index is no reason to fetch them again.
pub const SOUNDS_VERSION: u32 = 1;

/// The same for an account's marks.
pub const MARKS_VERSION: u32 = 2;

// --- Rows: what goes in, and what comes out when asked for ---

/// What a library is to a device. Its `id` is its catalogue folder, as a copy and `Facts::folders` name it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Library {
    pub id: i64,
    /// How the pages name it (a copy's `library`).
    pub key: String,
    pub name: String,
    pub kind: Kind,
    /// Its place in the libraries' order.
    pub order: u32,
    pub colour: Option<String>,
    /// Whose own music it is.
    pub owner: Option<String>,
}

/// What a library is for: as the server's own (`settings::LibraryKind`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Main,
    Own,
    Incoming,
}

impl Kind {
    fn of(n: i64) -> Self {
        match n {
            0 => Kind::Main,
            1 => Kind::Own,
            _ => Kind::Incoming,
        }
    }
}

/// An artist (`ar-<id>`), for life.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Artist {
    pub id: i64,
    pub name: String,
    /// How the name sorts.
    pub sort: String,
    /// Every other way the files write the name: an artist seed finds its tracks through what the files say, and what makes two
    /// spellings one artist is the server's (`Index::artist_ids`).
    pub spelled: Vec<String>,
}

/// One folder's files of a record (docs/records-and-copies.md).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecordCopy {
    /// Its catalogue folder: its library.
    pub library: i64,
    pub dir: String,
    /// One format for all its files, else none.
    pub format: Option<String>,
    /// Its worst file's.
    pub min_kbps: Option<u32>,
    pub bits: Option<u32>,
    pub rate: Option<u32>,
    pub size: Option<u64>,
    pub added: Option<i64>,
    pub tracks: u32,
}

/// A record (`al-<id>`), and its copies, in the order they would play for this account: the first in a library switched on is
/// what plays, as a list's `copies` are on the page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Record {
    pub id: i64,
    pub title: String,
    /// How the title sorts.
    pub sort: String,
    /// Its album artist as shown, and each of them.
    pub artist: String,
    pub artist_ids: Vec<i64>,
    pub year: Option<u32>,
    pub original_year: Option<u32>,
    pub added: Option<i64>,
    /// Its release type and MusicBrainz's secondary types, as the lists name them.
    pub kind: Option<String>,
    pub secondary: Vec<String>,
    pub genres: Vec<String>,
    pub compilation: bool,
    /// Where tracks were found missing, the whole release's count.
    pub whole_tracks: Option<u32>,
    pub copies: Vec<RecordCopy>,
}

/// A track (`tr-<id>`) of a record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub id: i64,
    pub record: i64,
    pub disc: Option<u32>,
    pub number: Option<u32>,
    pub title: String,
    /// Its artist as shown.
    pub artist: Option<String>,
    /// Radio's artists: the track's in their order, then the album artist unless it is a compilation's. A name may come twice,
    /// as it does there.
    pub artist_ids: Vec<i64>,
    /// What the crates read of it, as the server has it. Its `track` and `album` are this row's, its `sound` lies in [`Sounds`],
    /// and `file_words` are the server's own, gone once read.
    pub facts: Facts,
}

impl Track {
    fn head(&self) -> TrackHead<'_> {
        TrackHead { id: self.id, record: self.record, disc: self.disc, number: self.number, title: &self.title, artist: self.artist.as_deref(), artist_ids: &self.artist_ids }
    }
}

/// A track's own fields, as [`Builder::track`] takes them beside facts that stay where they are.
#[derive(Debug, Clone, Copy)]
pub struct TrackHead<'a> {
    pub id: i64,
    pub record: i64,
    pub disc: Option<u32>,
    pub number: Option<u32>,
    pub title: &'a str,
    pub artist: Option<&'a str>,
    pub artist_ids: &'a [i64],
}

/// Rows, in any order: of two with one id the later stands.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rows {
    pub libraries: Vec<Library>,
    pub artists: Vec<Artist>,
    pub records: Vec<Record>,
    pub tracks: Vec<Track>,
}

/// What leaves. A record and an artist need no word of their own: they go by themselves when nothing names them (`Packed::tidy`),
/// so the server does not have to work out, account by account, who last named what.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gone {
    pub libraries: Vec<i64>,
    pub tracks: Vec<i64>,
}

/// Why bytes or changes were not taken. Whatever it says the answer is one: fetch the index whole, and until then what was held
/// stays as it was. A `Version` on what was just fetched says the app and the server differ, and fetching again will not mend it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Written by another version of this format.
    Version { found: u32, wants: u32 },
    /// Not what was written: cut short, changed, or not this at all.
    Damaged,
    /// Changes that do not start where the index stands (`at`).
    Rev { at: i64, from: i64 },
    /// A row names what is neither held nor in the same changes.
    Unknown { what: &'static str, id: i64 },
}

// --- The bytes ---

/// Every file begins with these: what it is, its version, its body's length and what the body adds up to. A version is read
/// before anything else, and it stays in this place whatever else changes, or a device could not say why it refuses.
const HEADER: usize = 24;
const INDEX: [u8; 4] = *b"CRXI";
const CHANGES: [u8; 4] = *b"CRXC";
const SOUNDS: [u8; 4] = *b"CRXS";
const SOUND_CHANGES: [u8; 4] = *b"CRXD";
const MARKS: [u8; 4] = *b"CRXM";

/// What a body adds up to: a torn write or a changed byte is refused, not read as a library. Every step can be undone, so one
/// word changed always changes the sum.
fn checksum(bytes: &[u8]) -> u64 {
    let step = |sum: u64, word: u64| (sum ^ word).wrapping_mul(0x0000_0100_0000_01B3).rotate_left(23);
    let mut chunks = bytes.chunks_exact(8);
    let mut sum = 0xCBF2_9CE4_8422_2325;
    let mut word = [0u8; 8];
    for chunk in &mut chunks {
        word.copy_from_slice(chunk);
        sum = step(sum, u64::from_le_bytes(word));
    }
    let tail = chunks.remainder();
    word = [0; 8];
    word[..tail.len()].copy_from_slice(tail);
    step(sum, u64::from_le_bytes(word))
}

/// `out`, begun with `HEADER` bytes of room, with its header written.
fn sealed(mut out: Vec<u8>, kind: [u8; 4], version: u32) -> Vec<u8> {
    let (length, sum) = ((out.len() - HEADER) as u64, checksum(&out[HEADER..]));
    out[..4].copy_from_slice(&kind);
    out[4..8].copy_from_slice(&version.to_le_bytes());
    out[8..16].copy_from_slice(&length.to_le_bytes());
    out[16..24].copy_from_slice(&sum.to_le_bytes());
    out
}

/// A file's body, if it is what it says it is.
fn opened(bytes: &[u8], kind: [u8; 4], version: u32) -> Result<Reader<'_>, Refused> {
    let mut head = Reader { bytes };
    if head.take(4).ok() != Some(&kind[..]) {
        return Err(Refused::Damaged);
    }
    let found = u32::from_le_bytes(head.array()?);
    if found != version {
        return Err(Refused::Version { found, wants: version });
    }
    let (length, sum) = (u64::from_le_bytes(head.array()?), u64::from_le_bytes(head.array()?));
    if head.bytes.len() as u64 != length || checksum(head.bytes) != sum {
        return Err(Refused::Damaged);
    }
    Ok(head)
}

/// A number in as few bytes as it needs, seven bits a byte.
fn varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// A signed number with its sign in the lowest bit, so a small one stays small.
fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

fn unzigzag(v: u64) -> i64 {
    (v >> 1) as i64 ^ -((v & 1) as i64)
}

/// Bytes being read. Nothing read from a file is trusted to be in bounds: a panic here would end the app, not refuse a file.
struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Refused> {
        if n > self.bytes.len() {
            return Err(Refused::Damaged);
        }
        let (taken, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Ok(taken)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], Refused> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn varint(&mut self) -> Result<u64, Refused> {
        let (mut v, mut shift) = (0u64, 0);
        loop {
            let byte = self.take(1)?[0];
            if shift > 63 || (shift == 63 && byte > 1) {
                return Err(Refused::Damaged);
            }
            v |= u64::from(byte & 0x7f) << shift;
            if byte < 0x80 {
                return Ok(v);
            }
            shift += 7;
        }
    }

    fn int(&mut self) -> Result<i64, Refused> {
        Ok(unzigzag(self.varint()?))
    }

    fn size(&mut self) -> Result<usize, Refused> {
        usize::try_from(self.varint()?).map_err(|_| Refused::Damaged)
    }

    /// How many of something that takes a byte at least: never more than is left, so a wrong count asks for no great room.
    fn rows(&mut self) -> Result<usize, Refused> {
        let n = self.size()?;
        if n > self.bytes.len() { Err(Refused::Damaged) } else { Ok(n) }
    }

    fn done(&self) -> Result<(), Refused> {
        if self.bytes.is_empty() { Ok(()) } else { Err(Refused::Damaged) }
    }
}

// --- Columns ---

/// What a number column holds for "none": no id or date is this.
const NONE: i64 = i64::MIN;

fn from_small(v: Option<u32>) -> i64 {
    v.map_or(NONE, i64::from)
}

fn to_small(v: i64) -> Option<u32> {
    u32::try_from(v).ok()
}

fn to_int(v: i64) -> Option<i64> {
    (v != NONE).then_some(v)
}

fn from_float(v: Option<f64>) -> f64 {
    v.unwrap_or(f64::NAN)
}

fn to_float(v: f64) -> Option<f64> {
    (!v.is_nan()).then_some(v)
}

/// A word: its place in the index's one list of words. Its first word is empty, and stands for none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Word(u32);

/// One value of a column.
trait Cell: Copy + Default + PartialEq {
    fn put(cells: &[Self], out: &mut Vec<u8>);
    fn get(from: &mut Reader, n: usize, into: &mut Vec<Self>) -> Result<(), Refused>;
    /// It where words are numbered as `map` says.
    fn mapped(self, _map: &[u32]) -> Self {
        self
    }
    fn word(&mut self) -> Option<&mut u32> {
        None
    }
    /// What it says, whatever number its word has here: what a row's print is made of.
    fn said(self, words: &Text, out: &mut Vec<u8>);
}

/// Each as what it differs by from the last: ids rise slowly and a record's tracks say the same, so most take a byte.
impl Cell for i64 {
    fn put(cells: &[i64], out: &mut Vec<u8>) {
        let mut last = 0i64;
        for v in cells {
            varint(out, zigzag(v.wrapping_sub(last)));
            last = *v;
        }
    }

    fn get(from: &mut Reader, n: usize, into: &mut Vec<i64>) -> Result<(), Refused> {
        let mut last = 0i64;
        for _ in 0..n {
            last = last.wrapping_add(from.int()?);
            into.push(last);
        }
        Ok(())
    }

    fn said(self, _words: &Text, out: &mut Vec<u8>) {
        out.extend(self.to_le_bytes());
    }
}

impl Cell for Word {
    fn put(cells: &[Word], out: &mut Vec<u8>) {
        let mut last = 0i64;
        for w in cells {
            varint(out, zigzag(i64::from(w.0) - last));
            last = i64::from(w.0);
        }
    }

    fn get(from: &mut Reader, n: usize, into: &mut Vec<Word>) -> Result<(), Refused> {
        let mut last = 0i64;
        for _ in 0..n {
            last = last.wrapping_add(from.int()?);
            into.push(Word(u32::try_from(last).map_err(|_| Refused::Damaged)?));
        }
        Ok(())
    }

    fn mapped(self, map: &[u32]) -> Word {
        Word(map[self.0 as usize])
    }

    fn word(&mut self) -> Option<&mut u32> {
        Some(&mut self.0)
    }

    fn said(self, words: &Text, out: &mut Vec<u8>) {
        said_text(words.word(self), out);
    }
}

/// Past this a float has no fraction left to lose, and a whole number as a varint no gain.
const WHOLE: f64 = (1u64 << 50) as f64;

/// Nearly all of these are whole numbers (a year, a tempo) or thousandths (a length in seconds, from its milliseconds), which
/// take two or three bytes. Anything else keeps all its eight: what the server said comes back exact, as a crate's range is
/// tested against it. None takes a byte. The first byte says which: 0 none, 1 eight bytes follow, then the number and its scale.
impl Cell for f64 {
    fn put(cells: &[f64], out: &mut Vec<u8>) {
        for v in cells.iter().copied() {
            let thousandths = (v * 1000.0).round() as i64;
            if v.is_nan() {
                varint(out, 0);
            } else if v.abs() < WHOLE && v as i64 as f64 == v {
                varint(out, 2 + (zigzag(v as i64) << 1));
            } else if v.abs() < WHOLE && thousandths as f64 / 1000.0 == v {
                varint(out, 3 + (zigzag(thousandths) << 1));
            } else {
                varint(out, 1);
                out.extend(v.to_le_bytes());
            }
        }
    }

    fn get(from: &mut Reader, n: usize, into: &mut Vec<f64>) -> Result<(), Refused> {
        for _ in 0..n {
            into.push(match from.varint()? {
                0 => f64::NAN,
                1 => f64::from_le_bytes(from.array()?),
                said if said % 2 == 0 => unzigzag((said - 2) >> 1) as f64,
                said => unzigzag((said - 3) >> 1) as f64 / 1000.0,
            });
        }
        Ok(())
    }

    fn said(self, _words: &Text, out: &mut Vec<u8>) {
        out.extend(self.to_bits().to_le_bytes());
    }
}

impl Cell for f32 {
    fn put(cells: &[f32], out: &mut Vec<u8>) {
        for v in cells {
            out.extend(v.to_le_bytes());
        }
    }

    fn get(from: &mut Reader, n: usize, into: &mut Vec<f32>) -> Result<(), Refused> {
        for _ in 0..n {
            into.push(f32::from_le_bytes(from.array()?));
        }
        Ok(())
    }

    fn said(self, _words: &Text, out: &mut Vec<u8>) {
        out.extend(self.to_bits().to_le_bytes());
    }
}

impl Cell for i8 {
    fn put(cells: &[i8], out: &mut Vec<u8>) {
        out.extend(cells.iter().map(|v| *v as u8));
    }

    fn get(from: &mut Reader, n: usize, into: &mut Vec<i8>) -> Result<(), Refused> {
        into.extend(from.take(n)?.iter().map(|v| *v as i8));
        Ok(())
    }

    fn said(self, _words: &Text, out: &mut Vec<u8>) {
        out.push(self as u8);
    }
}

/// A column of a table: written and read whole, and taken from row by row when two tables become one.
trait Column: Default {
    fn write(&self, out: &mut Vec<u8>);
    fn read(from: &mut Reader, rows: usize) -> Result<Self, Refused>;
    /// Row `row` of `from` added at the end, its words numbered by `map`.
    fn take(&mut self, from: &Self, row: usize, map: &[u32]);
    /// Every word it holds.
    fn each_word(&mut self, _each: &mut dyn FnMut(&mut u32)) {}
    /// What row `row` says.
    fn said(&self, row: usize, words: &Text, out: &mut Vec<u8>);
}

impl<T: Cell> Column for Vec<T> {
    fn write(&self, out: &mut Vec<u8>) {
        T::put(self, out);
    }

    fn read(from: &mut Reader, rows: usize) -> Result<Self, Refused> {
        let mut out = Vec::with_capacity(rows);
        T::get(from, rows, &mut out)?;
        Ok(out)
    }

    fn take(&mut self, from: &Self, row: usize, map: &[u32]) {
        self.push(from[row].mapped(map));
    }

    fn each_word(&mut self, each: &mut dyn FnMut(&mut u32)) {
        self.iter_mut().filter_map(Cell::word).for_each(each);
    }

    fn said(&self, row: usize, words: &Text, out: &mut Vec<u8>) {
        self[row].said(words, out);
    }
}

/// A list a row, the lists end to end.
#[derive(Debug, Clone, Default)]
struct Lists<T> {
    ends: Vec<u32>,
    items: Vec<T>,
}

impl<T: Cell> Lists<T> {
    fn of(&self, row: usize) -> &[T] {
        let start = if row == 0 { 0 } else { self.ends[row - 1] as usize };
        &self.items[start..self.ends[row] as usize]
    }

    fn push(&mut self, items: impl IntoIterator<Item = T>) {
        self.items.extend(items);
        self.ends.push(self.items.len() as u32);
    }
}

/// A row says how many follow, one more than it holds; a row saying 0 holds what the row before holds, which is what most of a
/// record's tracks say of its words.
impl<T: Cell> Column for Lists<T> {
    fn write(&self, out: &mut Vec<u8>) {
        let mut last: &[T] = &[];
        for row in 0..self.ends.len() {
            let here = self.of(row);
            if here == last {
                varint(out, 0);
            } else {
                varint(out, here.len() as u64 + 1);
                T::put(here, out);
            }
            last = here;
        }
    }

    fn read(from: &mut Reader, rows: usize) -> Result<Self, Refused> {
        let mut out = Self { ends: Vec::with_capacity(rows), items: Vec::new() };
        let mut last = 0;
        for _ in 0..rows {
            let start = out.items.len();
            match from.size()? {
                0 => out.items.extend_from_within(last..start),
                said if said - 1 > from.bytes.len() => return Err(Refused::Damaged),
                said => T::get(from, said - 1, &mut out.items)?,
            }
            last = start;
            out.ends.push(u32::try_from(out.items.len()).map_err(|_| Refused::Damaged)?);
        }
        Ok(out)
    }

    fn take(&mut self, from: &Self, row: usize, map: &[u32]) {
        self.push(from.of(row).iter().map(|c| c.mapped(map)));
    }

    fn each_word(&mut self, each: &mut dyn FnMut(&mut u32)) {
        self.items.iter_mut().filter_map(Cell::word).for_each(each);
    }

    fn said(&self, row: usize, words: &Text, out: &mut Vec<u8>) {
        let cells = self.of(row);
        varint(out, cells.len() as u64);
        cells.iter().for_each(|c| c.said(words, out));
    }
}

/// Text a row, end to end. Written as what each shares with the one before and the rest, as a record's tracks share a folder
/// and sorted words their start: a track's line is the largest thing here and most of it repeats.
#[derive(Debug, Clone, Default)]
struct Text {
    ends: Vec<u32>,
    bytes: String,
}

impl Text {
    fn len(&self) -> usize {
        self.ends.len()
    }

    fn get(&self, row: usize) -> &str {
        let start = if row == 0 { 0 } else { self.ends[row - 1] as usize };
        &self.bytes[start..self.ends[row] as usize]
    }

    fn push(&mut self, text: &str) {
        self.bytes.push_str(text);
        self.ends.push(self.bytes.len() as u32);
    }

    fn word(&self, word: Word) -> &str {
        self.get(word.0 as usize)
    }

    fn maybe(&self, word: Word) -> Option<String> {
        (word.0 != 0).then(|| self.word(word).to_string())
    }

    fn all(&self, words: &[Word]) -> Vec<String> {
        words.iter().map(|w| self.word(*w).to_string()).collect()
    }
}

impl Column for Text {
    fn write(&self, out: &mut Vec<u8>) {
        let mut last: &[u8] = &[];
        for row in 0..self.len() {
            let here = self.get(row).as_bytes();
            let shared = here.iter().zip(last).take_while(|(a, b)| a == b).count();
            varint(out, shared as u64);
            varint(out, (here.len() - shared) as u64);
            out.extend_from_slice(&here[shared..]);
            last = here;
        }
    }

    fn read(from: &mut Reader, rows: usize) -> Result<Self, Refused> {
        let (mut bytes, mut ends): (Vec<u8>, Vec<u32>) = (Vec::new(), Vec::with_capacity(rows));
        let mut last = 0;
        for _ in 0..rows {
            let (shared, rest) = (from.size()?, from.size()?);
            let start = bytes.len();
            if shared > start - last {
                return Err(Refused::Damaged);
            }
            bytes.extend_from_within(last..last + shared);
            bytes.extend_from_slice(from.take(rest)?);
            last = start;
            ends.push(u32::try_from(bytes.len()).map_err(|_| Refused::Damaged)?);
        }
        // What `get` slices at must be places in the text, or it would panic later rather than refuse now.
        let bytes = String::from_utf8(bytes).map_err(|_| Refused::Damaged)?;
        if ends.iter().all(|end| bytes.is_char_boundary(*end as usize)) { Ok(Self { ends, bytes }) } else { Err(Refused::Damaged) }
    }

    fn take(&mut self, from: &Self, row: usize, _map: &[u32]) {
        self.push(from.get(row));
    }

    fn said(&self, row: usize, _words: &Text, out: &mut Vec<u8>) {
        said_text(self.get(row), out);
    }
}

/// Text said with its length, so that two cells cannot run into one another and read the same.
fn said_text(text: &str, out: &mut Vec<u8>) {
    varint(out, text.len() as u64);
    out.extend_from_slice(text.as_bytes());
}

/// A table: columns of one length, its rows in the order of its key.
trait Table: Default {
    fn keys(&self) -> &[i64];
    fn write(&self, out: &mut Vec<u8>);
    fn read(from: &mut Reader) -> Result<Self, Refused>;
    fn take(&mut self, from: &Self, row: usize, map: &[u32]);
    fn each_word(&mut self, each: &mut dyn FnMut(&mut u32));
    /// What row `row` says, column by column.
    fn said(&self, row: usize, words: &Text, out: &mut Vec<u8>);
}

/// A table's columns, said once: what is written, read and taken of a row follows from them.
macro_rules! table {
    ($(#[$about:meta])* $name:ident by $key:ident { $($col:ident: $kind:ty,)* }) => {
        $(#[$about])*
        #[derive(Debug, Clone, Default)]
        struct $name {
            $($col: $kind,)*
        }

        impl Table for $name {
            fn keys(&self) -> &[i64] {
                &self.$key
            }

            fn write(&self, out: &mut Vec<u8>) {
                varint(out, self.$key.len() as u64);
                $(self.$col.write(out);)*
            }

            fn read(from: &mut Reader) -> Result<Self, Refused> {
                let rows = from.rows()?;
                Ok(Self { $($col: Column::read(from, rows)?,)* })
            }

            fn take(&mut self, from: &Self, row: usize, map: &[u32]) {
                $(self.$col.take(&from.$col, row, map);)*
            }

            fn each_word(&mut self, each: &mut dyn FnMut(&mut u32)) {
                $(self.$col.each_word(each);)*
            }

            fn said(&self, row: usize, words: &Text, out: &mut Vec<u8>) {
                $(self.$col.said(row, words, out);)*
            }
        }
    };
}

table! {
    Libraries by id {
        id: Vec<i64>,
        key: Vec<Word>,
        name: Vec<Word>,
        kind: Vec<i64>,
        order: Vec<i64>,
        colour: Vec<Word>,
        owner: Vec<Word>,
    }
}

table! {
    Artists by id {
        id: Vec<i64>,
        name: Vec<Word>,
        sort: Text,
        spelled: Lists<Word>,
    }
}

table! {
    Records by id {
        id: Vec<i64>,
        title: Text,
        sort: Text,
        artist: Vec<Word>,
        artist_ids: Lists<i64>,
        year: Vec<i64>,
        original_year: Vec<i64>,
        added: Vec<i64>,
        kind: Vec<Word>,
        secondary: Lists<Word>,
        genres: Lists<Word>,
        compilation: Vec<i64>,
        whole_tracks: Vec<i64>,
    }
}

table! {
    /// By their record, a record's together in the order they would play.
    Copies by record {
        record: Vec<i64>,
        library: Vec<i64>,
        dir: Text,
        format: Vec<Word>,
        min_kbps: Vec<i64>,
        bits: Vec<i64>,
        rate: Vec<i64>,
        size: Vec<i64>,
        added: Vec<i64>,
        tracks: Vec<i64>,
    }
}

table! {
    Tracks by id {
        id: Vec<i64>,
        record: Vec<i64>,
        disc: Vec<i64>,
        number: Vec<i64>,
        title: Text,
        artist: Vec<Word>,
        artist_ids: Lists<i64>,
        line: Text,
        lines: Text,
        folders: Lists<i64>,
        born: Vec<i64>,
        length: Vec<f64>,
        year: Vec<f64>,
        bpm: Vec<f64>,
        energy: Vec<f64>,
        danceability: Vec<f64>,
        four: Vec<f32>,
        clap_moods: Lists<f32>,
        genres: Lists<Word>,
        styles: Lists<Word>,
        moods: Lists<Word>,
        tags: Lists<Word>,
        labels: Lists<Word>,
        release_types: Lists<Word>,
        artists: Lists<Word>,
        credits: Lists<Word>,
        credit_names: Lists<Word>,
        quality: Vec<Word>,
        key: Vec<Word>,
        meter: Vec<Word>,
    }
}

/// A track's other lines lie in one cell, apart by what no path holds. Most have none.
const LINES_APART: &str = "\0";

fn lines_in(cell: &str) -> Vec<String> {
    if cell.is_empty() { Vec::new() } else { cell.split(LINES_APART).map(str::to_string).collect() }
}

fn album_id(record: i64) -> String {
    format!("{}{record}", ids::ALBUM)
}

fn artist_id(artist: i64) -> String {
    format!("{}{artist}", ids::ARTIST)
}

/// Rows in the order of their ids, one an id.
fn by_id<T>(rows: &[T], id: impl Fn(&T) -> i64) -> Vec<&T> {
    let mut all: Vec<&T> = rows.iter().collect();
    all.sort_by_key(|r| id(r));
    let mut out: Vec<&T> = Vec::with_capacity(all.len());
    for row in all {
        if out.last().is_some_and(|last| id(last) == id(row)) {
            out.pop();
        }
        out.push(row);
    }
    out
}

/// `old` with `new`'s rows in place of those it holds of each key, and what is left of `old` but for the keys in `drop` (sorted):
/// the one way rows change, a whole key at a time, so a record's copies go and come together. `from_old`, `from_new`: their
/// words' numbers here.
fn merged<T: Table>(old: &T, new: &T, drop: &[i64], from_old: &[u32], from_new: &[u32]) -> T {
    let (a, b) = (old.keys(), new.keys());
    let mut out = T::default();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        let key = match (a.get(i), b.get(j)) {
            (Some(x), Some(y)) => *x.min(y),
            (Some(x), None) => *x,
            (_, Some(y)) => *y,
            (None, None) => break,
        };
        let (ours, theirs) = (a[i..].iter().take_while(|k| **k == key).count(), b[j..].iter().take_while(|k| **k == key).count());
        if theirs > 0 {
            (j..j + theirs).for_each(|row| out.take(new, row, from_new));
        } else if drop.binary_search(&key).is_err() {
            (i..i + ours).for_each(|row| out.take(old, row, from_old));
        }
        i += ours;
        j += theirs;
    }
    out
}

/// Each row of a table as a number, by its key: what it says, and what `also` adds of it.
fn printed<T: Table>(table: &T, words: &Text, also: impl Fn(i64, &mut Vec<u8>)) -> Vec<(i64, u64)> {
    let mut said = Vec::new();
    let print = |(row, key): (usize, &i64)| {
        said.clear();
        table.said(row, words, &mut said);
        also(*key, &mut said);
        (*key, checksum(&said))
    };
    table.keys().iter().enumerate().map(print).collect()
}

/// A table's rows whose key is in `keep`.
fn kept<T: Table>(table: &T, keep: &HashSet<i64>, same: &[u32]) -> T {
    let mut out = T::default();
    for (row, key) in table.keys().iter().enumerate() {
        if keep.contains(key) {
            out.take(table, row, same);
        }
    }
    out
}

/// Two sorted lists of words as one, and what each one's numbers are in it.
fn joined(a: &Text, b: &Text) -> (Text, Vec<u32>, Vec<u32>) {
    let mut out = Text::default();
    let (mut from_a, mut from_b) = (Vec::with_capacity(a.len()), Vec::with_capacity(b.len()));
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        let next = match (i < a.len(), j < b.len()) {
            (true, true) => a.get(i).min(b.get(j)),
            (true, false) => a.get(i),
            _ => b.get(j),
        };
        let n = out.len() as u32;
        if i < a.len() && a.get(i) == next {
            from_a.push(n);
            i += 1;
        }
        if j < b.len() && b.get(j) == next {
            from_b.push(n);
            j += 1;
        }
        out.push(next);
    }
    (out, from_a, from_b)
}

/// Words as they are met, numbered as met: a row is read once, and the words sorted after.
#[derive(Default)]
struct Names(HashMap<String, u32>);

impl Names {
    fn of(&mut self, word: &str) -> Word {
        if let Some(n) = self.0.get(word) {
            return Word(*n);
        }
        let n = self.0.len() as u32;
        self.0.insert(word.to_string(), n);
        Word(n)
    }

    fn maybe(&mut self, word: Option<&str>) -> Word {
        self.of(word.unwrap_or(""))
    }

    fn all(&mut self, words: &[String]) -> Vec<Word> {
        words.iter().map(|w| self.of(w)).collect()
    }
}

/// Rows packed one at a time, each table's in the order of its ids: how the server makes an index, as `Rows` would hold every
/// track's facts a second time beside the crates', and a whole library of small strings does not go back to the system whole.
pub struct Builder {
    names: Names,
    packed: Packed,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        let mut names = Names::default();
        names.of("");
        Self { names, packed: Packed::default() }
    }

    pub fn library(&mut self, row: &Library) {
        let (to, names) = (&mut self.packed.libraries, &mut self.names);
        to.id.push(row.id);
        to.key.push(names.of(&row.key));
        to.name.push(names.of(&row.name));
        to.kind.push(row.kind as i64);
        to.order.push(i64::from(row.order));
        to.colour.push(names.maybe(row.colour.as_deref()));
        to.owner.push(names.maybe(row.owner.as_deref()));
    }

    pub fn artist(&mut self, row: &Artist) {
        let (to, names) = (&mut self.packed.artists, &mut self.names);
        to.id.push(row.id);
        to.name.push(names.of(&row.name));
        to.sort.push(&row.sort);
        to.spelled.push(names.all(&row.spelled));
    }

    /// A record and its copies.
    pub fn record(&mut self, row: &Record) {
        let (to, names) = (&mut self.packed.records, &mut self.names);
        to.id.push(row.id);
        to.title.push(&row.title);
        to.sort.push(&row.sort);
        to.artist.push(names.of(&row.artist));
        to.artist_ids.push(row.artist_ids.iter().copied());
        to.year.push(from_small(row.year));
        to.original_year.push(from_small(row.original_year));
        to.added.push(row.added.unwrap_or(NONE));
        to.kind.push(names.maybe(row.kind.as_deref()));
        to.secondary.push(names.all(&row.secondary));
        to.genres.push(names.all(&row.genres));
        to.compilation.push(i64::from(row.compilation));
        to.whole_tracks.push(from_small(row.whole_tracks));
        for copy in &row.copies {
            let to = &mut self.packed.copies;
            to.record.push(row.id);
            to.library.push(copy.library);
            to.dir.push(&copy.dir);
            to.format.push(names.maybe(copy.format.as_deref()));
            to.min_kbps.push(from_small(copy.min_kbps));
            to.bits.push(from_small(copy.bits));
            to.rate.push(from_small(copy.rate));
            to.size.push(copy.size.map_or(NONE, |s| s as i64));
            to.added.push(copy.added.unwrap_or(NONE));
            to.tracks.push(i64::from(copy.tracks));
        }
    }

    /// A track: `row`'s own fields, and what the crates read of it.
    pub fn track(&mut self, row: TrackHead, f: &Facts) {
        let (to, names) = (&mut self.packed.tracks, &mut self.names);
        to.id.push(row.id);
        to.record.push(row.record);
        to.disc.push(from_small(row.disc));
        to.number.push(from_small(row.number));
        to.title.push(row.title);
        to.artist.push(names.maybe(row.artist));
        to.artist_ids.push(row.artist_ids.iter().copied());
        to.line.push(&f.rel);
        to.lines.push(&f.lines.join(LINES_APART));
        to.folders.push(f.folders.iter().copied());
        to.born.push(f.born.unwrap_or(NONE));
        to.length.push(from_float(f.length));
        to.year.push(from_float(f.year));
        to.bpm.push(from_float(f.bpm));
        to.energy.push(from_float(f.energy));
        to.danceability.push(from_float(f.danceability));
        to.four.push(f.four.unwrap_or(f32::NAN));
        to.clap_moods.push(f.clap_moods.iter().flatten().copied());
        to.genres.push(names.all(&f.genres));
        to.styles.push(names.all(&f.styles));
        to.moods.push(names.all(&f.moods));
        to.tags.push(names.all(&f.tags));
        to.labels.push(names.all(&f.labels));
        to.release_types.push(names.all(&f.release_types));
        to.artists.push(names.all(&f.artists));
        to.credits.push(names.all(&f.credits));
        to.credit_names.push(names.all(&f.credit_names));
        to.quality.push(names.maybe(f.quality.as_deref()));
        to.key.push(names.maybe(f.key.as_deref()));
        to.meter.push(names.maybe(f.meter.as_deref()));
    }

    /// What was packed, its words sorted: what is left is `tidy`'s.
    fn packed(self) -> Packed {
        let Self { names, mut packed } = self;
        let mut sorted: Vec<(String, u32)> = names.0.into_iter().collect();
        sorted.sort_unstable();
        let mut place = vec![0; sorted.len()];
        for (at, (word, n)) in sorted.iter().enumerate() {
            place[*n as usize] = at as u32;
            packed.words.push(word);
        }
        packed.each_word(&mut |w| *w = place[*w as usize]);
        packed
    }

    /// The index of what was packed, as of `rev`. Refused where a row names what is not here, or came out of the order of its
    /// ids: a file of that form would be refused by whoever loads it.
    pub fn build(self, rev: i64) -> Result<Index, Refused> {
        let mut packed = self.packed();
        if !packed.in_order() {
            return Err(Refused::Damaged);
        }
        packed.tidy();
        packed.check()?;
        Ok(Index { rev, packed })
    }
}

/// Every table, and the words they say.
#[derive(Debug, Clone, Default)]
struct Packed {
    words: Text,
    libraries: Libraries,
    artists: Artists,
    records: Records,
    copies: Copies,
    tracks: Tracks,
}

impl Packed {
    /// These rows, in order, their words sorted: what is left is `tidy`'s.
    fn of(rows: &Rows) -> Self {
        let mut to = Builder::new();
        by_id(&rows.libraries, |l| l.id).into_iter().for_each(|row| to.library(row));
        by_id(&rows.artists, |a| a.id).into_iter().for_each(|row| to.artist(row));
        by_id(&rows.records, |r| r.id).into_iter().for_each(|row| to.record(row));
        by_id(&rows.tracks, |t| t.id).into_iter().for_each(|row| to.track(row.head(), &row.facts));
        to.packed()
    }

    fn each_word(&mut self, each: &mut dyn FnMut(&mut u32)) {
        self.libraries.each_word(each);
        self.artists.each_word(each);
        self.records.each_word(each);
        self.copies.each_word(each);
        self.tracks.each_word(each);
    }

    /// This with changes in, unchecked: what is left of it may name nothing yet.
    fn with(&self, new: &Packed, gone: &Gone) -> Packed {
        let (words, ours, theirs) = joined(&self.words, &new.words);
        Packed {
            libraries: merged(&self.libraries, &new.libraries, &gone.libraries, &ours, &theirs),
            artists: merged(&self.artists, &new.artists, &[], &ours, &theirs),
            records: merged(&self.records, &new.records, &[], &ours, &theirs),
            // A record sent with no copies has none left.
            copies: merged(&self.copies, &new.copies, &new.records.id, &ours, &theirs),
            tracks: merged(&self.tracks, &new.tracks, &gone.tracks, &ours, &theirs),
            words,
        }
    }

    /// Lets go of what nothing names. A record stands while a track names it, and its copies with it; an artist while a track or
    /// a record does; a word while something says it. So a fresh build and a long history come to the same bytes, and nothing
    /// lingers that a list would show.
    fn tidy(&mut self) {
        let same: Vec<u32> = (0..self.words.len() as u32).collect();
        let held: HashSet<i64> = self.tracks.record.iter().copied().collect();
        if self.records.id.iter().any(|r| !held.contains(r)) {
            self.records = kept(&self.records, &held, &same);
            self.copies = kept(&self.copies, &held, &same);
        }
        let named: HashSet<i64> = self.tracks.artist_ids.items.iter().chain(&self.records.artist_ids.items).copied().collect();
        if self.artists.id.iter().any(|a| !named.contains(a)) {
            self.artists = kept(&self.artists, &named, &same);
        }
        let mut said = vec![false; self.words.len()];
        said[0] = true;
        self.each_word(&mut |w| said[*w as usize] = true);
        if said.contains(&false) {
            let (mut words, mut place) = (Text::default(), vec![0; said.len()]);
            for (at, _) in said.iter().enumerate().filter(|(_, said)| **said) {
                place[at] = words.len() as u32;
                words.push(self.words.get(at));
            }
            self.each_word(&mut |w| *w = place[*w as usize]);
            self.words = words;
        }
    }

    /// Whether everything named is here.
    fn check(&self) -> Result<(), Refused> {
        let within = |what: &'static str, named: &[i64], held: &[i64]| match named.iter().find(|id| held.binary_search(id).is_err()) {
            Some(id) => Err(Refused::Unknown { what, id: *id }),
            None => Ok(()),
        };
        within("record", &self.tracks.record, &self.records.id)?;
        within("record", &self.copies.record, &self.records.id)?;
        within("artist", &self.tracks.artist_ids.items, &self.artists.id)?;
        within("artist", &self.records.artist_ids.items, &self.artists.id)?;
        within("library", &self.tracks.folders.items, &self.libraries.id)?;
        within("library", &self.copies.library, &self.libraries.id)
    }

    fn write(&self, out: &mut Vec<u8>) {
        varint(out, self.words.len() as u64);
        self.words.write(out);
        self.libraries.write(out);
        self.artists.write(out);
        self.records.write(out);
        self.copies.write(out);
        self.tracks.write(out);
    }

    /// As written, or refused: everything later indexes by what is read here without asking again.
    fn read(from: &mut Reader) -> Result<Self, Refused> {
        let rows = from.rows()?;
        let mut packed = Packed {
            words: Text::read(from, rows)?,
            libraries: Libraries::read(from)?,
            artists: Artists::read(from)?,
            records: Records::read(from)?,
            copies: Copies::read(from)?,
            tracks: Tracks::read(from)?,
        };
        let sorted = rows > 0 && packed.words.get(0).is_empty() && (1..rows).all(|i| packed.words.get(i - 1) < packed.words.get(i));
        let mut known = true;
        packed.each_word(&mut |w| known &= (*w as usize) < rows);
        if sorted && packed.in_order() && known { Ok(packed) } else { Err(Refused::Damaged) }
    }

    /// Whether each table's rows lie in the order of their ids, a record's copies together.
    fn in_order(&self) -> bool {
        let rising = |ids: &[i64]| ids.windows(2).all(|w| w[0] < w[1]);
        rising(self.libraries.keys()) && rising(self.artists.keys()) && rising(self.records.keys()) && rising(self.tracks.keys()) && self.copies.record.windows(2).all(|w| w[0] <= w[1])
    }

    fn library_at(&self, row: usize) -> Library {
        let (c, words) = (&self.libraries, &self.words);
        Library {
            id: c.id[row],
            key: words.word(c.key[row]).to_string(),
            name: words.word(c.name[row]).to_string(),
            kind: Kind::of(c.kind[row]),
            order: to_small(c.order[row]).unwrap_or(0),
            colour: words.maybe(c.colour[row]),
            owner: words.maybe(c.owner[row]),
        }
    }

    fn artist_at(&self, row: usize) -> Artist {
        let (c, words) = (&self.artists, &self.words);
        Artist { id: c.id[row], name: words.word(c.name[row]).to_string(), sort: c.sort.get(row).to_string(), spelled: words.all(c.spelled.of(row)) }
    }

    fn copy_at(&self, row: usize) -> RecordCopy {
        let (c, words) = (&self.copies, &self.words);
        RecordCopy {
            library: c.library[row],
            dir: c.dir.get(row).to_string(),
            format: words.maybe(c.format[row]),
            min_kbps: to_small(c.min_kbps[row]),
            bits: to_small(c.bits[row]),
            rate: to_small(c.rate[row]),
            size: u64::try_from(c.size[row]).ok(),
            added: to_int(c.added[row]),
            tracks: to_small(c.tracks[row]).unwrap_or(0),
        }
    }

    fn record_at(&self, row: usize) -> Record {
        let (c, words) = (&self.records, &self.words);
        let id = c.id[row];
        let copies = self.copies.record.partition_point(|r| *r < id)..self.copies.record.partition_point(|r| *r <= id);
        Record {
            id,
            title: c.title.get(row).to_string(),
            sort: c.sort.get(row).to_string(),
            artist: words.word(c.artist[row]).to_string(),
            artist_ids: c.artist_ids.of(row).to_vec(),
            year: to_small(c.year[row]),
            original_year: to_small(c.original_year[row]),
            added: to_int(c.added[row]),
            kind: words.maybe(c.kind[row]),
            secondary: words.all(c.secondary.of(row)),
            genres: words.all(c.genres.of(row)),
            compilation: c.compilation[row] != 0,
            whole_tracks: to_small(c.whole_tracks[row]),
            copies: copies.map(|at| self.copy_at(at)).collect(),
        }
    }

    /// Every field by name: a fact added to `Facts` stops the build here, and asks whether a device needs it.
    fn facts_at(&self, row: usize, album: Option<Arc<str>>) -> Facts {
        let (c, words) = (&self.tracks, &self.words);
        Facts {
            rel: c.line.get(row).to_string(),
            lines: lines_in(c.lines.get(row)),
            folders: c.folders.of(row).to_vec(),
            born: to_int(c.born[row]),
            genres: words.all(c.genres.of(row)),
            year: to_float(c.year[row]),
            bpm: to_float(c.bpm[row]),
            four: (!c.four[row].is_nan()).then_some(c.four[row]),
            meter: words.maybe(c.meter[row]),
            energy: to_float(c.energy[row]),
            danceability: to_float(c.danceability[row]),
            moods: words.all(c.moods.of(row)),
            styles: words.all(c.styles.of(row)),
            tags: words.all(c.tags.of(row)),
            track: Some(c.id[row]),
            sound: None,
            artists: words.all(c.artists.of(row)),
            credits: words.all(c.credits.of(row)),
            credit_names: words.all(c.credit_names.of(row)),
            clap_moods: <[f32; similarity::MOODS.len()]>::try_from(c.clap_moods.of(row)).ok(),
            length: to_float(c.length[row]),
            quality: words.maybe(c.quality[row]),
            key: words.maybe(c.key[row]),
            release_types: words.all(c.release_types.of(row)),
            album,
            labels: words.all(c.labels.of(row)),
            file_words: None,
        }
    }

    fn track_at(&self, row: usize, album: Option<Arc<str>>) -> Track {
        let (c, words) = (&self.tracks, &self.words);
        Track {
            id: c.id[row],
            record: c.record[row],
            disc: to_small(c.disc[row]),
            number: to_small(c.number[row]),
            title: c.title.get(row).to_string(),
            artist: words.maybe(c.artist[row]),
            artist_ids: c.artist_ids.of(row).to_vec(),
            facts: self.facts_at(row, album),
        }
    }

    /// Each record's id as it is served, one copy for all its tracks, by the records' rows.
    fn albums(&self) -> Vec<Arc<str>> {
        self.records.id.iter().map(|r| Arc::from(album_id(*r))).collect()
    }

    /// A track's album among those.
    fn album_of(&self, row: usize, albums: &[Arc<str>]) -> Option<Arc<str>> {
        self.records.id.binary_search(&self.tracks.record[row]).ok().map(|at| albums[at].clone())
    }
}

// --- The index ---

/// The library on a device. Always in its one form: rows in order, nothing nothing names, so two that hold the same rows are the
/// same to the byte, and that is what equal means here.
#[derive(Debug, Clone)]
pub struct Index {
    rev: i64,
    packed: Packed,
}

impl PartialEq for Index {
    fn eq(&self, other: &Self) -> bool {
        self.save() == other.save()
    }
}

impl Index {
    /// These rows as of `rev`, as the server makes them. Refused where a row names what is not among them.
    pub fn build(rev: i64, rows: &Rows) -> Result<Self, Refused> {
        let mut packed = Packed::of(rows);
        packed.tidy();
        packed.check()?;
        Ok(Self { rev, packed })
    }

    /// Where in the server's change log this stands.
    pub fn rev(&self) -> i64 {
        self.rev
    }

    /// This as of `rev`: the server knows where it stands only once it has compared what this holds with what was told.
    pub fn at(mut self, rev: i64) -> Self {
        self.rev = rev;
        self
    }

    pub fn tracks(&self) -> usize {
        self.packed.tracks.id.len()
    }

    /// Whether a track is held.
    pub fn holds(&self, track: i64) -> bool {
        self.packed.tracks.id.binary_search(&track).is_ok()
    }

    pub fn prints(&self) -> Prints {
        let (p, none) = (&self.packed, |_: i64, _: &mut Vec<u8>| {});
        let copies = |record: i64, out: &mut Vec<u8>| {
            let at = p.copies.record.partition_point(|r| *r < record)..p.copies.record.partition_point(|r| *r <= record);
            at.for_each(|row| p.copies.said(row, &p.words, out));
        };
        Prints { libraries: printed(&p.libraries, &p.words, none), artists: printed(&p.artists, &p.words, none), records: printed(&p.records, &p.words, copies), tracks: printed(&p.tracks, &p.words, none) }
    }

    /// Every library held.
    pub fn libraries(&self) -> Vec<Library> {
        (0..self.packed.libraries.id.len()).map(|row| self.packed.library_at(row)).collect()
    }

    pub fn save(&self) -> Vec<u8> {
        let mut out = vec![0; HEADER];
        varint(&mut out, zigzag(self.rev));
        self.packed.write(&mut out);
        sealed(out, INDEX, VERSION)
    }

    pub fn load(bytes: &[u8]) -> Result<Self, Refused> {
        let mut from = opened(bytes, INDEX, VERSION)?;
        let rev = from.int()?;
        let packed = Packed::read(&mut from)?;
        from.done()?;
        packed.check()?;
        Ok(Self { rev, packed })
    }

    /// The feed's next changes. Refused, the index left as it was, if they do not start here or leave a row naming nothing.
    pub fn apply(&mut self, changes: &Changes) -> Result<(), Refused> {
        if changes.from != self.rev || changes.to < changes.from {
            return Err(Refused::Rev { at: self.rev, from: changes.from });
        }
        let mut next = self.packed.with(&changes.set, &changes.gone);
        next.tidy();
        next.check()?;
        self.packed = next;
        self.rev = changes.to;
        Ok(())
    }

    /// Every row.
    pub fn rows(&self) -> Rows {
        let p = &self.packed;
        let albums = p.albums();
        Rows {
            libraries: (0..p.libraries.id.len()).map(|row| p.library_at(row)).collect(),
            artists: (0..p.artists.id.len()).map(|row| p.artist_at(row)).collect(),
            records: (0..p.records.id.len()).map(|row| p.record_at(row)).collect(),
            tracks: (0..p.tracks.id.len()).map(|row| p.track_at(row, p.album_of(row, &albums))).collect(),
        }
    }

    pub fn artist(&self, id: i64) -> Option<Artist> {
        self.packed.artists.id.binary_search(&id).ok().map(|row| self.packed.artist_at(row))
    }

    pub fn record(&self, id: i64) -> Option<Record> {
        self.packed.records.id.binary_search(&id).ok().map(|row| self.packed.record_at(row))
    }

    pub fn track(&self, id: i64) -> Option<Track> {
        let row = self.packed.tracks.id.binary_search(&id).ok()?;
        Some(self.packed.track_at(row, Some(Arc::from(album_id(self.packed.tracks.record[row])))))
    }

    /// What `crates::View` works over, in the order the server hands it in: newest first, then by the row file's folder and path,
    /// so a crate's order and a tie among its closest fall as they do there. `sounds`: none, and no Sounds like finds anything by
    /// sound. Worth keeping while the index stands: it is the one thing here that costs room.
    pub fn facts(&self, sounds: Option<&Sounds>) -> Vec<Facts> {
        let (p, t) = (&self.packed, &self.packed.tracks);
        let albums = p.albums();
        let mut order: Vec<usize> = (0..t.id.len()).collect();
        // None is the lowest here as a NULL is there, so it comes last.
        order.sort_unstable_by(|&a, &b| t.born[b].cmp(&t.born[a]).then_with(|| t.folders.of(a).first().cmp(&t.folders.of(b).first())).then_with(|| t.line.get(a).cmp(t.line.get(b))));
        order
            .into_iter()
            .map(|row| Facts { sound: sounds.and_then(|s| s.of(t.id[row])), ..p.facts_at(row, p.album_of(row, &albums)) })
            .collect()
    }

    /// Each artist's id by every name the files give them: what `crates::Alike::of` asks for.
    pub fn artist_ids(&self) -> HashMap<String, String> {
        let (a, words) = (&self.packed.artists, &self.packed.words);
        let mut out = HashMap::new();
        for row in 0..a.id.len() {
            for name in std::iter::once(&a.name[row]).chain(a.spelled.of(row)) {
                out.insert(words.word(*name).to_string(), artist_id(a.id[row]));
            }
        }
        out
    }

    /// Radio's index of it, as the server reads one (`app::radio`): the songs by their album's id as text, disc, track and
    /// path, albums and artists numbered as met, then `facts` over it. A draw goes along that order, so one seed picks alike
    /// only on the same order and the same numbers. `playlists`: the numbers of the playlists holding each song, by its id. What
    /// a listener marks on a song is left to the caller, as it is there.
    pub fn radio(&self, facts: &[Facts], playlists: &HashMap<String, Vec<u32>>) -> radio::Index {
        let (t, records) = (&self.packed.tracks, &self.packed.records);
        let albums: Vec<String> = records.id.iter().map(|r| album_id(*r)).collect();
        let place: Vec<usize> = t.record.iter().map(|r| records.id.binary_search(r).unwrap_or(0)).collect();
        let mut order: Vec<usize> = (0..t.id.len()).collect();
        order.sort_unstable_by(|&a, &b| {
            albums[place[a]].cmp(&albums[place[b]]).then(t.disc[a].cmp(&t.disc[b])).then(t.number[a].cmp(&t.number[b])).then_with(|| t.line.get(a).cmp(t.line.get(b)))
        });
        let (mut album_numbers, mut artist_numbers, mut artist_ids) = (HashMap::new(), HashMap::new(), Vec::new());
        let songs = order
            .into_iter()
            .map(|row| {
                let id = ids::song(t.id[row]);
                let next = album_numbers.len() as u32;
                let album_n = *album_numbers.entry(t.record[row]).or_insert(next);
                let mut artist_n = |artist: i64| {
                    let next = artist_numbers.len() as u32;
                    *artist_numbers.entry(artist).or_insert_with(|| {
                        artist_ids.push(artist_id(artist));
                        next
                    })
                };
                let artists = t.artist_ids.of(row).iter().map(|a| artist_n(*a)).collect();
                let like = similarity::Track { artists, playlists: playlists.get(&id).cloned().unwrap_or_default(), ..similarity::Track::default() };
                radio::Song {
                    title: t.title.get(row).to_string(),
                    artist: self.packed.words.maybe(t.artist[row]),
                    album: records.title.get(place[row]).to_string(),
                    album_id: albums[place[row]].clone(),
                    album_n,
                    like,
                    duration_secs: to_float(t.length[row]),
                    folders: t.folders.of(row).to_vec(),
                    id,
                    ..radio::Song::default()
                }
            })
            .collect();
        let mut index = radio::Index::new(songs);
        index.artist_ids = artist_ids;
        index.with_facts(facts)
    }
}

/// What each row of an index says, as a number: its id, and one that moves with anything the row says and with nothing else, not
/// the rev, not where a word lies in the list. What the server has told devices is kept as these, so that a row moved with no write
/// to it is seen for what it is: a mood's bar raised, a word of the household's, a copy's order. A record's says its copies too.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prints {
    pub libraries: Vec<(i64, u64)>,
    pub artists: Vec<(i64, u64)>,
    pub records: Vec<(i64, u64)>,
    pub tracks: Vec<(i64, u64)>,
}

// --- Changes ---

/// What the feed brings from one rev to the next: rows as they stand now, whole, and what left. A row sent is a row replaced,
/// its record's copies with it, and one that leaves and was not held is no fault, so the server need not know what a device
/// holds, only what the log says changed. Rows name what they need: anything new to this account among what they name comes in the
/// same changes, or they are refused.
#[derive(Debug, Clone)]
pub struct Changes {
    from: i64,
    to: i64,
    set: Packed,
    gone: Gone,
}

impl Changes {
    pub fn new(from: i64, to: i64, set: &Rows, mut gone: Gone) -> Self {
        gone.libraries.sort_unstable();
        gone.tracks.sort_unstable();
        Self { from, to, set: Packed::of(set), gone }
    }

    /// Where they leave an index.
    pub fn to(&self) -> i64 {
        self.to
    }

    /// As they travel: no JSON to read in the page's WebAssembly, and the columns' own bytes.
    pub fn save(&self) -> Vec<u8> {
        let mut out = vec![0; HEADER];
        varint(&mut out, zigzag(self.from));
        varint(&mut out, zigzag(self.to));
        self.set.write(&mut out);
        for ids in [&self.gone.libraries, &self.gone.tracks] {
            varint(&mut out, ids.len() as u64);
            ids.write(&mut out);
        }
        sealed(out, CHANGES, VERSION)
    }

    pub fn load(bytes: &[u8]) -> Result<Self, Refused> {
        let mut from = opened(bytes, CHANGES, VERSION)?;
        let (start, to) = (from.int()?, from.int()?);
        let set = Packed::read(&mut from)?;
        let mut ids = || -> Result<Vec<i64>, Refused> {
            let rows = from.rows()?;
            let mut read: Vec<i64> = Column::read(&mut from, rows)?;
            read.sort_unstable();
            Ok(read)
        };
        let gone = Gone { libraries: ids()?, tracks: ids()? };
        from.done()?;
        Ok(Self { from: start, to, set, gone })
    }
}

// --- Sounds ---

table! {
    /// Each track AudioMuse heard, its sound in eight bits.
    Heard by track {
        track: Vec<i64>,
        sound: Lists<i8>,
    }
}

/// What AudioMuse heard of each track, apart from the index: they change rarely and weigh more than all the rest, so they are
/// sent and kept on their own, with a rev of their own (0101's point 16). Eight bits a number, the largest of a vector at 127: its
/// direction is all a sound says, so it needs no scale kept, and comes back of unit length.
#[derive(Debug, Clone, Default)]
pub struct Sounds {
    rev: i64,
    heard: Heard,
}

impl PartialEq for Sounds {
    fn eq(&self, other: &Self) -> bool {
        self.save() == other.save()
    }
}

/// None where it says nothing: all zero, as a vector not heard is.
fn eight_bits(sound: &[f32]) -> Option<Vec<i8>> {
    let most = sound.iter().fold(0.0f32, |most, x| most.max(x.abs()));
    (most > 0.0 && most.is_finite()).then(|| sound.iter().map(|x| (x / most * 127.0).round() as i8).collect())
}

impl Sounds {
    /// These tracks' sounds as of `rev`.
    pub fn build(rev: i64, heard: &[(i64, Vec<f32>)]) -> Self {
        Self::from_sorted(rev, by_id(heard, |h| h.0).into_iter().map(|(track, sound)| (*track, sound.as_slice())))
    }

    /// The same a sound at a time, in the order of the tracks' ids: the server's stay where they are, and only eight bits of each
    /// are held here.
    pub fn from_sorted<'a>(rev: i64, heard: impl IntoIterator<Item = (i64, &'a [f32])>) -> Self {
        let mut out = Heard::default();
        for (track, sound) in heard.into_iter().filter_map(|(track, sound)| Some((track, eight_bits(sound)?))) {
            out.track.push(track);
            out.sound.push(sound);
        }
        Self { rev, heard: out }
    }

    pub fn rev(&self) -> i64 {
        self.rev
    }

    /// These as of `rev`, as [`Index::at`].
    pub fn at(mut self, rev: i64) -> Self {
        self.rev = rev;
        self
    }

    /// How many tracks are heard.
    pub fn len(&self) -> usize {
        self.heard.track.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heard.track.is_empty()
    }

    /// What each track's sound says, as [`Index::prints`] has it of a row.
    pub fn prints(&self) -> Vec<(i64, u64)> {
        printed(&self.heard, &Text::default(), |_, _| {})
    }

    /// `tracks` as they stand here, whole, from `from`: a device's feed. One not heard has left.
    pub fn changes(&self, from: i64, tracks: &[i64]) -> SoundChanges {
        let (mut set, mut gone) = (Heard::default(), Vec::new());
        let mut tracks = tracks.to_vec();
        tracks.sort_unstable();
        tracks.dedup();
        for track in tracks {
            match self.heard.track.binary_search(&track) {
                Ok(row) => set.take(&self.heard, row, &[]),
                Err(_) => gone.push(track),
            }
        }
        SoundChanges { from, to: self.rev, set, gone }
    }

    /// A track's sound, of unit length; none where it was not heard.
    pub fn of(&self, track: i64) -> Option<Arc<[f32]>> {
        let row = self.heard.track.binary_search(&track).ok()?;
        let sound: Vec<f32> = self.heard.sound.of(row).iter().map(|x| f32::from(*x)).collect();
        similarity::mean_sound(&[&sound]).map(Arc::from)
    }

    pub fn save(&self) -> Vec<u8> {
        let mut out = vec![0; HEADER];
        varint(&mut out, zigzag(self.rev));
        self.heard.write(&mut out);
        sealed(out, SOUNDS, SOUNDS_VERSION)
    }

    pub fn load(bytes: &[u8]) -> Result<Self, Refused> {
        let mut from = opened(bytes, SOUNDS, SOUNDS_VERSION)?;
        let rev = from.int()?;
        let heard = Heard::read(&mut from)?;
        from.done()?;
        if heard.track.windows(2).all(|w| w[0] < w[1]) { Ok(Self { rev, heard }) } else { Err(Refused::Damaged) }
    }

    /// The feed's next changes. Refused, what is held left as it was, if they do not start here.
    pub fn apply(&mut self, changes: &SoundChanges) -> Result<(), Refused> {
        if changes.from != self.rev || changes.to < changes.from {
            return Err(Refused::Rev { at: self.rev, from: changes.from });
        }
        self.heard = merged(&self.heard, &changes.set, &changes.gone, &[], &[]);
        self.rev = changes.to;
        Ok(())
    }
}

/// What the feed brings of the sounds from one rev to the next: as [`Changes`] of rows, on their own, as a device on a mobile
/// connection may leave them for later.
#[derive(Debug, Clone)]
pub struct SoundChanges {
    from: i64,
    to: i64,
    set: Heard,
    gone: Vec<i64>,
}

impl SoundChanges {
    /// Where they leave what is held.
    pub fn to(&self) -> i64 {
        self.to
    }

    /// How many sounds they bring, and how many they take away.
    pub fn counts(&self) -> (usize, usize) {
        (self.set.track.len(), self.gone.len())
    }

    pub fn save(&self) -> Vec<u8> {
        let mut out = vec![0; HEADER];
        varint(&mut out, zigzag(self.from));
        varint(&mut out, zigzag(self.to));
        self.set.write(&mut out);
        varint(&mut out, self.gone.len() as u64);
        self.gone.write(&mut out);
        sealed(out, SOUND_CHANGES, SOUNDS_VERSION)
    }

    pub fn load(bytes: &[u8]) -> Result<Self, Refused> {
        let mut from = opened(bytes, SOUND_CHANGES, SOUNDS_VERSION)?;
        let (start, to) = (from.int()?, from.int()?);
        let set = Heard::read(&mut from)?;
        let rows = from.rows()?;
        let mut gone: Vec<i64> = Column::read(&mut from, rows)?;
        gone.sort_unstable();
        from.done()?;
        if set.track.windows(2).all(|w| w[0] < w[1]) { Ok(Self { from: start, to, set, gone }) } else { Err(Refused::Damaged) }
    }
}

// --- An account's marks ---

table! {
    /// By track: only those with anything to say.
    Marked by track {
        track: Vec<i64>,
        fit: Vec<f64>,
        skipped: Vec<f64>,
        plays: Vec<i64>,
        last: Vec<i64>,
        rating: Vec<i64>,
        skips: Vec<i64>,
    }
}

/// What the server last worked out for one account of one track: how well it fits their taste and how like it is to what they
/// skip (decision 0094: Best match, Fits your taste, Radio's Your taste), and what `crates::Use` holds of their listening. A
/// device adds its own unsent plays to the counts (0101's point 18).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mark {
    pub track: i64,
    pub fit: Option<f64>,
    pub skipped: Option<f64>,
    pub plays: u32,
    /// When last played (epoch s).
    pub last: Option<i64>,
    pub rating: Option<u8>,
    pub skips: u32,
}

/// An account's marks as of when they were worked out. They go with no feed: they move with every play, weigh little beside the
/// index, and are worked out whole.
#[derive(Debug, Clone, Default)]
pub struct Marks {
    at: i64,
    marked: Marked,
    /// [`own::Own`] as JSON: the server writes it, and only what answers on a device reads it.
    own: String,
}

impl PartialEq for Marks {
    fn eq(&self, other: &Self) -> bool {
        self.save() == other.save()
    }
}

impl Marks {
    /// `rows`, in any order, as of `at` (epoch s).
    pub fn build(at: i64, rows: &[Mark]) -> Self {
        let mut to = Marked::default();
        for row in by_id(rows, |m| m.track) {
            to.track.push(row.track);
            to.fit.push(from_float(row.fit));
            to.skipped.push(from_float(row.skipped));
            to.plays.push(i64::from(row.plays));
            to.last.push(row.last.unwrap_or(NONE));
            to.rating.push(row.rating.map_or(NONE, i64::from));
            to.skips.push(i64::from(row.skips));
        }
        Self { at, marked: to, own: String::new() }
    }

    /// These with the rest of what is the account's own, as JSON.
    pub fn with_own(self, own: String) -> Self {
        Self { own, ..self }
    }

    /// The rest of what is the account's own, as JSON.
    pub fn own(&self) -> &str {
        &self.own
    }

    /// When they were worked out: what a device's own plays since are added to.
    pub fn at(&self) -> i64 {
        self.at
    }

    fn at_row(&self, row: usize) -> Mark {
        let m = &self.marked;
        Mark {
            track: m.track[row],
            fit: to_float(m.fit[row]),
            skipped: to_float(m.skipped[row]),
            plays: to_small(m.plays[row]).unwrap_or(0),
            last: to_int(m.last[row]),
            rating: u8::try_from(m.rating[row]).ok(),
            skips: to_small(m.skips[row]).unwrap_or(0),
        }
    }

    /// A track's, where it has any.
    pub fn of(&self, track: i64) -> Option<Mark> {
        self.marked.track.binary_search(&track).ok().map(|row| self.at_row(row))
    }

    pub fn rows(&self) -> Vec<Mark> {
        (0..self.marked.track.len()).map(|row| self.at_row(row)).collect()
    }

    pub fn save(&self) -> Vec<u8> {
        let mut out = vec![0; HEADER];
        varint(&mut out, zigzag(self.at));
        self.marked.write(&mut out);
        varint(&mut out, self.own.len() as u64);
        out.extend_from_slice(self.own.as_bytes());
        sealed(out, MARKS, MARKS_VERSION)
    }

    pub fn load(bytes: &[u8]) -> Result<Self, Refused> {
        let mut from = opened(bytes, MARKS, MARKS_VERSION)?;
        let at = from.int()?;
        let marked = Marked::read(&mut from)?;
        let length = from.rows()?;
        let own = String::from_utf8(from.take(length)?.to_vec()).map_err(|_| Refused::Damaged)?;
        from.done()?;
        if marked.track.windows(2).all(|w| w[0] < w[1]) { Ok(Self { at, marked, own }) } else { Err(Refused::Damaged) }
    }
}

// --- A library made up ---

/// A library made up in a real one's proportions (about eleven tracks a record, a few more artists than records, three
/// libraries), with the sounds AudioMuse would give it: what the index is measured on, here and on a device. One seed, one
/// library. Outside this crate's tests the feature `made-up` turns it on, and nothing that plays builds that.
#[cfg(any(test, feature = "made-up"))]
pub fn made_up(tracks: usize, seed: u64) -> (Rows, Vec<(i64, Vec<f32>)>) {
    use crate::Rng;

    const BITS: [&str; 40] = [
        "ka", "ro", "mi", "vel", "dor", "sha", "lin", "tor", "ne", "bra", "sol", "qui", "ma", "len", "fa", "gru", "hel", "ix", "jo", "ku", "pla", "re", "sto", "tha", "um", "vi", "wen", "xa",
        "yo", "zen", "ae", "ber", "cor", "dra", "el", "fri", "gal", "har", "in", "jan",
    ];
    /// How long AudioMuse's vectors are.
    const DIMS: usize = 200;

    fn word(r: &mut Rng) -> String {
        let mut word: String = (0..2 + r.below(3)).map(|_| BITS[r.below(BITS.len())]).collect();
        if r.below(30) == 0 {
            word.push('é');
        }
        word
    }
    fn named(r: &mut Rng, words: usize) -> String {
        let capital = |w: String| {
            let mut letters = w.chars();
            letters.next().map_or(String::new(), |first| first.to_uppercase().chain(letters).collect())
        };
        (0..words).map(|_| capital(word(r))).collect::<Vec<_>>().join(" ")
    }
    fn names(r: &mut Rng, n: usize) -> Vec<String> {
        (0..n.max(2))
            .map(|_| {
                let words = 1 + r.below(2);
                named(r, words)
            })
            .collect()
    }
    /// A place below `n`, the low ones likelier: a library says a few words often.
    fn common(r: &mut Rng, n: usize) -> usize {
        let top = 1 + r.below(n);
        r.below(top)
    }
    fn some(r: &mut Rng, pool: &[String], most: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for _ in 0..1 + r.below(most) {
            let word = &pool[common(r, pool.len())];
            if !out.contains(word) {
                out.push(word.clone());
            }
        }
        out
    }

    let mut r = Rng::seeded(seed);
    let (records, people) = ((tracks * 10 / 108).max(1), (tracks * 10 / 86).max(2));
    let (genres, styles, tags, labels) = (names(&mut r, 300), names(&mut r, 150), names(&mut r, tracks / 12), names(&mut r, records / 3));
    let moods = similarity::MOODS.map(str::to_string);
    let mut artists: Vec<Artist> = names(&mut r, people).into_iter().enumerate().map(|(n, name)| Artist { id: n as i64 * 2 + 1, sort: name.to_lowercase(), spelled: Vec::new(), name }).collect();
    for artist in artists.iter_mut().step_by(50) {
        artist.spelled.push(artist.name.to_uppercase());
    }
    artists.push(Artist { id: people as i64 * 2 + 1, name: "Various Artists".into(), sort: "various artists".into(), spelled: Vec::new() });
    let libraries = vec![
        Library { id: 0, key: "music".into(), name: "Music".into(), kind: Kind::Main, order: 0, colour: None, owner: None },
        Library { id: 1, key: "own-music".into(), name: "Own music".into(), kind: Kind::Own, order: 1, colour: Some("#a040c0".into()), owner: Some("listener".into()) },
        Library { id: 2, key: "lidarr-music".into(), name: "Lidarr".into(), kind: Kind::Incoming, order: 2, colour: Some("#4080c0".into()), owner: None },
    ];
    let mut out = Rows { libraries, ..Rows::default() };
    let mut heard = Vec::with_capacity(tracks);
    for n in 0..records {
        let record = n as i64 * 3 + 1;
        let compilation = n % 40 == 39;
        // Some artists hold many records, most one or two.
        let lead = if n % 4 == 0 { common(&mut r, people) } else { n * people / records };
        let album_artist = &artists[if compilation { people } else { lead }];
        let words = 1 + r.below(3);
        let title = named(&mut r, words);
        let year = 1950 + r.below(76) as u32;
        // Music's mostly, some Incoming, a few someone's own; every sixteenth a second copy.
        let folder: i64 = match n % 25 {
            0 | 1 => 2,
            2 => 1,
            _ => 0,
        };
        let second = (n % 16 == 5).then_some(if folder == 0 { 2 } else { 0 });
        let dir = format!("{}/{title} ({year})", album_artist.name);
        let line = |folder: i64, file: &str| match folder {
            0 => format!("{dir}/{file}"),
            1 => format!("../own/{dir}/{file}"),
            _ => format!("../lidarr-music/{dir}/{file}"),
        };
        let (first, end) = (n * tracks / records, (n + 1) * tracks / records);
        let (kept_genres, kept_tags, kept_labels) = (some(&mut r, &genres, 3), some(&mut r, &tags, 4), some(&mut r, &labels, 1));
        let types: Vec<String> = if compilation { vec!["Album".into(), "Compilation".into()] } else { vec![["Album", "Album", "Album", "EP", "Single"][r.below(5)].into()] };
        let lossless = n % 3 != 0;
        let quality = if lossless { "Lossless" } else { ["320 kbps", "256 kbps", "192 kbps"][r.below(3)] };
        let added = 1_500_000_000 + n as i64 * 40_000;
        let copy = |library: i64, r: &mut Rng| RecordCopy {
            library,
            dir: dir.clone(),
            format: Some(if lossless { "FLAC" } else { "MP3" }.into()),
            min_kbps: Some(if lossless { 700 + r.below(400) as u32 } else { 192 }),
            bits: lossless.then_some(16),
            rate: Some(44_100),
            size: Some(30_000_000 + r.below(300_000_000) as u64),
            added: Some(added),
            tracks: (end - first) as u32,
        };
        let copies: Vec<RecordCopy> = [Some(folder), second].into_iter().flatten().map(|library| copy(library, &mut r)).collect();
        let centre: Vec<f32> = (0..DIMS).map(|_| r.unit() as f32 - 0.5).collect();
        for t in first..end {
            let id = t as i64 * 2 + 1;
            let number = (t - first + 1) as u32;
            let words = 1 + r.below(4);
            let title = named(&mut r, words);
            let by = if compilation || t % 8 == 1 { r.below(people) } else { lead };
            let artist = &artists[by];
            let credit = match artist.spelled.first() {
                Some(spelled) if t % 2 == 0 => spelled.clone(),
                _ if t % 10 == 3 => format!("{} feat. {}", artist.name, artists[common(&mut r, people)].name),
                _ => artist.name.clone(),
            };
            let written = if compilation || credit == album_artist.name { vec![credit.clone()] } else { vec![credit.clone(), album_artist.name.clone()] };
            let (credits, credit_names) = crate::excluded_artists::credited(&written);
            let is_heard = t % 10 != 7;
            let scores: [f32; 5] = [(); 5].map(|_| ((0.4 + 0.3 * r.unit() as f32) * 10_000.0).round() / 10_000.0);
            let file = format!("{number:02} - {title}.flac");
            let facts = Facts {
                rel: line(folder, &file),
                lines: second.iter().map(|other| line(*other, &file)).collect(),
                folders: [Some(folder), second].into_iter().flatten().collect(),
                born: Some(added + t as i64 % 60),
                genres: kept_genres.clone(),
                year: Some(f64::from(year)),
                bpm: Some(if t % 9 == 0 { 120.0 + r.below(4_000) as f64 / 100.0 } else { 70.0 + r.below(100) as f64 }),
                four: is_heard.then(|| r.unit() as f32),
                meter: is_heard.then(|| ["4/4", "4/4", "3/4", "6/8"][r.below(4)].to_string()),
                energy: is_heard.then(|| (r.unit() * 100.0).round()),
                danceability: is_heard.then(|| (r.unit() * 100.0).round()),
                moods: if is_heard { some(&mut r, &moods, 2) } else { Vec::new() },
                styles: if is_heard { some(&mut r, &styles, 3) } else { Vec::new() },
                tags: kept_tags.clone(),
                track: Some(id),
                sound: None,
                artists: written,
                credits,
                credit_names,
                clap_moods: is_heard.then_some(scores),
                length: Some((60_000 + r.below(400_000)) as f64 / 1000.0),
                quality: Some(quality.to_string()),
                key: (t % 3 != 0).then(|| format!("{} {}", ["A", "B", "C", "D", "E", "F", "G", "F#"][r.below(8)], ["major", "minor"][r.below(2)])),
                release_types: types.clone(),
                album: Some(Arc::from(album_id(record))),
                labels: kept_labels.clone(),
                file_words: None,
            };
            let artist_ids = if compilation { vec![artist.id] } else { vec![artist.id, album_artist.id] };
            out.tracks.push(Track { id, record, disc: (t % 7 == 0).then_some(1), number: Some(number), title, artist: Some(credit), artist_ids, facts });
            if is_heard {
                heard.push((id, centre.iter().map(|x| x + 0.3 * (r.unit() as f32 - 0.5)).collect()));
            }
        }
        out.records.push(Record {
            id: record,
            sort: title.to_lowercase(),
            title,
            artist: album_artist.name.clone(),
            artist_ids: vec![album_artist.id],
            year: Some(year),
            original_year: (n % 11 == 0).then_some(year - 1),
            added: Some(added),
            kind: Some(types[0].to_lowercase()),
            secondary: types[1..].iter().map(|t| t.to_lowercase()).collect(),
            genres: kept_genres,
            compilation,
            whole_tracks: (n % 50 == 0).then_some((end - first) as u32 + 1),
            copies,
        });
    }
    let named_here: HashSet<i64> = out.tracks.iter().flat_map(|t| &t.artist_ids).chain(out.records.iter().flat_map(|r| &r.artist_ids)).copied().collect();
    out.artists = artists.into_iter().filter(|a| named_here.contains(&a.id)).collect();
    (out, heard)
}

#[cfg(test)]
mod tests {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    use super::*;
    use crate::Rng;
    use crate::crates::{Alike, Condition, Value, View, World};
    use crate::radio::{Ask, Mode, choose};
    use crate::similarity::{Signal, Weights};

    fn library(id: i64) -> Library {
        Library { id, key: format!("library-{id}"), name: format!("Library {id}"), order: id as u32, ..Library::default() }
    }

    fn artist(id: i64) -> Artist {
        Artist { id, name: format!("Artist {id}"), sort: format!("artist {id}"), spelled: Vec::new() }
    }

    fn record(id: i64, artist: i64, libraries: &[i64]) -> Record {
        let copies = libraries.iter().map(|l| RecordCopy { library: *l, dir: format!("r{id}"), tracks: 1, ..RecordCopy::default() }).collect();
        Record { id, title: format!("Record {id}"), sort: format!("record {id}"), artist: format!("Artist {artist}"), artist_ids: vec![artist], copies, ..Record::default() }
    }

    fn track(id: i64, record: i64, artist: i64, folder: i64) -> Track {
        let facts = Facts { rel: format!("r{record}/{id}.flac"), folders: vec![folder], track: Some(id), album: Some(Arc::from(album_id(record))), ..Facts::default() };
        Track { id, record, title: format!("Track {id}"), artist_ids: vec![artist], facts, ..Track::default() }
    }

    /// Two libraries, two artists, two records, three tracks.
    fn small() -> Rows {
        Rows {
            libraries: vec![library(0), library(1)],
            artists: vec![artist(1), artist(2)],
            records: vec![record(10, 1, &[0]), record(20, 2, &[0, 1])],
            tracks: vec![track(100, 10, 1, 0), track(101, 10, 1, 0), track(200, 20, 2, 1)],
        }
    }

    fn built(rows: &Rows) -> Index {
        Index::build(1, rows).unwrap()
    }

    /// A word only the track at `at` says.
    fn word_of(rows: &mut Rows, at: usize, word: &str) {
        rows.tracks[at].facts.genres.push(word.to_string());
    }

    #[test]
    fn a_saved_index_loads_as_the_same_index() {
        let (rows, _) = made_up(600, 3);
        let index = Index::build(42, &rows).unwrap();
        let bytes = index.save();
        let loaded = Index::load(&bytes).unwrap();
        assert_eq!(loaded.rev(), 42);
        assert!(loaded == index, "the same rows, to the byte");
        assert_eq!(loaded.save(), bytes, "saved again: the same bytes");
        assert_eq!(loaded.rows(), rows, "every row as it went in");
    }

    /// Each field its own value, so one read into another's place shows.
    #[test]
    fn every_field_comes_back_as_it_went_in() {
        let words = |s: &[&str]| s.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        let library = Library { id: 7, key: "seven".into(), name: "Seven".into(), kind: Kind::Own, order: 3, colour: Some("#123456".into()), owner: Some("someone".into()) };
        let other = Library { id: 9, kind: Kind::Incoming, ..library.clone() };
        let copy = RecordCopy { library: 7, dir: "A/B".into(), format: Some("FLAC".into()), min_kbps: Some(900), bits: Some(24), rate: Some(96_000), size: Some(1 << 40), added: Some(1_600_000_000), tracks: 12 };
        let artist = Artist { id: 5, name: "Name".into(), sort: "sort of name".into(), spelled: words(&["NAME", "name!"]) };
        let record = Record {
            id: 6,
            title: "Títle".into(),
            sort: "title sorted".into(),
            artist: "Album Artist".into(),
            artist_ids: vec![5],
            year: Some(1999),
            original_year: Some(1998),
            added: Some(1_500_000_000),
            kind: Some("ep".into()),
            secondary: words(&["live", "remix"]),
            genres: words(&["Dub", "Ambient"]),
            compilation: true,
            whole_tracks: Some(13),
            copies: vec![copy.clone(), RecordCopy { library: 9, format: None, ..copy }],
        };
        let facts = Facts {
            rel: "A/B/01.flac".into(),
            lines: words(&["../other/A/B/01.flac", "../third/01.flac"]),
            folders: vec![7, 9],
            born: Some(1_700_000_123),
            genres: words(&["Trip Hop"]),
            year: Some(1997.0),
            bpm: Some(127.53),
            four: Some(0.625),
            meter: Some("7/8".into()),
            energy: Some(61.0),
            danceability: Some(47.0),
            moods: words(&["sad"]),
            styles: words(&["dub techno", "drone"]),
            tags: words(&["night"]),
            track: Some(3),
            sound: None,
            artists: words(&["Name feat. Other", "Album Artist"]),
            credits: words(&["name", "other", "albumartist"]),
            credit_names: words(&["Name", "Other", "Album Artist"]),
            clap_moods: Some([0.51, 0.52, 0.53, 0.54, 0.55]),
            length: Some(212.345),
            quality: Some("Lossless".into()),
            key: Some("F# major".into()),
            release_types: words(&["EP", "Live"]),
            album: Some(Arc::from("al-6")),
            labels: words(&["Warp"]),
            file_words: None,
        };
        let track = Track { id: 3, record: 6, disc: Some(2), number: Some(11), title: "Tïtle".into(), artist: Some("Name feat. Other".into()), artist_ids: vec![5, 5], facts };
        let rows = Rows { libraries: vec![library, other], artists: vec![artist], records: vec![record], tracks: vec![track] };
        let index = Index::load(&built(&rows).save()).unwrap();
        assert_eq!(index.rows(), rows);
        assert_eq!(index.track(3).as_ref(), Some(&rows.tracks[0]));
        assert_eq!(index.record(6).as_ref(), Some(&rows.records[0]));
        assert_eq!(index.artist(5).as_ref(), Some(&rows.artists[0]));
        assert!(index.track(4).is_none() && index.record(5).is_none() && index.artist(6).is_none());
    }

    #[test]
    fn a_number_no_short_form_can_hold_comes_back_exact() {
        let cells = [0.0, 1.0, -1.0, 1997.0, 127.53, -127.5, 0.001, 212.345, 1.0 / 3.0, 1e300, -1e-300, f64::INFINITY, 2f64.powi(60)];
        let mut bytes = Vec::new();
        f64::put(&cells, &mut bytes);
        let mut read = Vec::new();
        f64::get(&mut Reader { bytes: &bytes }, cells.len(), &mut read).unwrap();
        assert_eq!(read, cells);
        assert!(bytes.len() < cells.len() * 5, "the ordinary ones are short: {}", bytes.len());
    }

    #[test]
    fn rows_in_another_order_save_the_same_bytes() {
        let (mut rows, _) = made_up(300, 5);
        let first = built(&rows).save();
        rows.tracks.reverse();
        rows.records.reverse();
        rows.artists.reverse();
        rows.libraries.reverse();
        assert_eq!(built(&rows).save(), first);
    }

    /// The rows of `a` and `b` that print differently, each table's ids.
    fn moved(a: &Prints, b: &Prints) -> [Vec<i64>; 4] {
        let differ = |x: &[(i64, u64)], y: &[(i64, u64)]| -> Vec<i64> {
            let (x, y): (HashMap<i64, u64>, HashMap<i64, u64>) = (x.iter().copied().collect(), y.iter().copied().collect());
            let mut ids: Vec<i64> = x.keys().chain(y.keys()).copied().filter(|id| x.get(id) != y.get(id)).collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        };
        [differ(&a.libraries, &b.libraries), differ(&a.artists, &b.artists), differ(&a.records, &b.records), differ(&a.tracks, &b.tracks)]
    }

    #[test]
    fn a_print_is_what_a_row_says_and_no_more() {
        let rows = small();
        let was = built(&rows).prints();
        assert_eq!(Index::build(9, &rows).unwrap().prints(), was, "the rev says nothing of a row");
        // A word that sorts first moves every other word's number, and says nothing of their rows.
        let mut other = rows.clone();
        other.tracks[2].facts.genres.push("AAA first of all".into());
        assert_eq!(moved(&was, &built(&other).prints()), [vec![], vec![], vec![], vec![200]]);
        let mut other = rows.clone();
        other.tracks[0].title = "Another".into();
        assert_eq!(moved(&was, &built(&other).prints()), [vec![], vec![], vec![], vec![100]]);
        let mut other = rows.clone();
        other.records[1].copies.reverse();
        assert_eq!(moved(&was, &built(&other).prints()), [vec![], vec![], vec![20], vec![]], "a copy's order is its record's");
        let mut other = rows.clone();
        other.records[1].copies[1].tracks = 3;
        assert_eq!(moved(&was, &built(&other).prints()), [vec![], vec![], vec![20], vec![]]);
        let mut other = rows.clone();
        other.artists[0].spelled.push("ARTIST".into());
        assert_eq!(moved(&was, &built(&other).prints()), [vec![], vec![1], vec![], vec![]]);
        let mut other = rows.clone();
        other.libraries[1].name = "Renamed".into();
        assert_eq!(moved(&was, &built(&other).prints()), [vec![1], vec![], vec![], vec![]]);
    }

    /// A column added is printed by the same line that writes it. Each of these went by another path once, and a row's cells must
    /// not be able to run into each other: two words of one row read as one, or one list's end as another's start.
    #[test]
    fn cells_cannot_run_into_each_other_in_a_print() {
        let mut one = small();
        one.tracks[0].facts.genres = vec!["ab".into(), "c".into()];
        let mut two = small();
        two.tracks[0].facts.genres = vec!["a".into(), "bc".into()];
        assert_eq!(moved(&built(&one).prints(), &built(&two).prints())[3], [100]);
        let mut three = small();
        three.tracks[0].facts.styles = vec!["ab".into()];
        three.tracks[0].facts.genres = vec!["c".into()];
        assert_eq!(moved(&built(&one).prints(), &built(&three).prints())[3], [100]);
    }

    /// The server packs a track at a time, its facts staying where they are, and must come to the very bytes `Rows` come to.
    #[test]
    fn rows_packed_one_at_a_time_save_what_a_whole_build_saves() {
        let (mut rows, _) = made_up(300, 8);
        rows.tracks.sort_by_key(|t| t.id);
        rows.records.sort_by_key(|r| r.id);
        rows.artists.sort_by_key(|a| a.id);
        // Named by nothing: gone by either way.
        rows.artists.push(artist(1_000_000));
        let mut to = Builder::new();
        rows.libraries.iter().for_each(|l| to.library(l));
        rows.artists.iter().for_each(|a| to.artist(a));
        rows.records.iter().for_each(|r| to.record(r));
        rows.tracks.iter().for_each(|t| to.track(t.head(), &t.facts));
        assert_eq!(to.build(7).unwrap().save(), Index::build(7, &rows).unwrap().save());
    }

    #[test]
    fn rows_packed_out_of_their_order_are_refused_as_any_reader_would_refuse_them() {
        let packing = |rows: &Rows, ids: [usize; 3]| {
            let mut to = Builder::new();
            rows.libraries.iter().for_each(|l| to.library(l));
            rows.artists.iter().for_each(|a| to.artist(a));
            rows.records.iter().for_each(|r| to.record(r));
            ids.iter().for_each(|at| to.track(rows.tracks[*at].head(), &rows.tracks[*at].facts));
            to.build(1)
        };
        assert!(packing(&small(), [0, 1, 2]).is_ok());
        assert_eq!(packing(&small(), [1, 0, 2]).err(), Some(Refused::Damaged));
        let mut unknown = small();
        unknown.records.remove(1);
        assert_eq!(packing(&unknown, [0, 1, 2]).err(), Some(Refused::Unknown { what: "record", id: 20 }));
    }

    /// What a pass of the library might do to `rows`, and the changes the feed brings for it, read as the server reads them: the
    /// log names ids, and each is sent as it stands now, with what it names.
    fn a_batch(rows: &mut Rows, r: &mut Rng, next: &mut i64, fresh: &[Track]) -> (Rows, Gone) {
        let (mut tracks, mut records, mut artists, mut libraries) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        for _ in 0..1 + r.below(6) {
            *next += 1;
            let new = *next;
            let some_track = if rows.tracks.is_empty() { None } else { Some(r.below(rows.tracks.len())) };
            let some_record = r.below(rows.records.len());
            let some_artist = r.below(rows.artists.len());
            match (r.below(10), some_track) {
                // A retag: a new title, a word no one said, the old words gone.
                (0 | 1, Some(at)) => {
                    let t = &mut rows.tracks[at];
                    t.title = format!("Retagged {new}");
                    t.facts.genres = vec![format!("Genre {new}")];
                    t.facts.bpm = None;
                    tracks.insert(t.id);
                }
                // A file renamed.
                (2, Some(at)) => {
                    let t = &mut rows.tracks[at];
                    t.facts.rel = format!("moved/{new}.flac");
                    t.facts.born = Some(new);
                    tracks.insert(t.id);
                }
                // A file deleted.
                (3, Some(at)) => {
                    tracks.insert(rows.tracks.swap_remove(at).id);
                }
                // A file added to a record there is.
                (4, _) => {
                    let id = rows.records[some_record].id;
                    let mut t = fresh[r.below(fresh.len())].clone();
                    (t.id, t.record, t.facts.track, t.facts.album) = (new, id, Some(new), Some(Arc::from(album_id(id))));
                    t.facts.folders = vec![rows.libraries[r.below(rows.libraries.len())].id];
                    t.artist_ids = vec![rows.artists[some_artist].id];
                    tracks.insert(new);
                    rows.tracks.push(t);
                }
                // A record come in, by an artist never seen.
                (5, _) => {
                    let (who, folder) = (Artist { id: new, name: format!("New artist {new}"), sort: format!("new artist {new}"), spelled: Vec::new() }, rows.libraries[0].id);
                    let mut held = record(new, new, &[folder]);
                    held.artist = who.name.clone();
                    let mut t = fresh[r.below(fresh.len())].clone();
                    (t.id, t.record, t.facts.track, t.facts.album) = (new, new, Some(new), Some(Arc::from(album_id(new))));
                    t.facts.folders = vec![folder];
                    t.artist_ids = vec![new];
                    rows.artists.push(who);
                    rows.records.push(held);
                    tracks.insert(new);
                    rows.tracks.push(t);
                }
                // A copy comes or goes.
                (6, _) => {
                    let held = &mut rows.records[some_record];
                    if held.copies.len() > 1 {
                        held.copies.pop();
                    } else if let Some(mut copy) = held.copies.first().cloned() {
                        copy.dir = format!("copy-{new}");
                        held.copies.insert(0, copy);
                    }
                    records.insert(held.id);
                }
                // A record's facts.
                (7, _) => {
                    let held = &mut rows.records[some_record];
                    held.title = format!("Renamed {new}");
                    held.kind = Some(format!("Kind {new}"));
                    records.insert(held.id);
                }
                // An artist renamed, and a track taken by another record.
                (8, Some(at)) => {
                    rows.artists[some_artist].name = format!("Renamed {new}");
                    artists.insert(rows.artists[some_artist].id);
                    let (to, t) = (rows.records[some_record].id, &mut rows.tracks[at]);
                    (t.record, t.facts.album) = (to, Some(Arc::from(album_id(to))));
                    tracks.insert(t.id);
                }
                // A library comes, and one goes, taking what lies in it alone.
                (9, _) => {
                    let id = 1_000 + new;
                    rows.libraries.push(library(id));
                    libraries.insert(id);
                    if rows.libraries.len() > 4 {
                        let out = rows.libraries.remove(1).id;
                        libraries.insert(out);
                        for held in rows.records.iter_mut().filter(|h| h.copies.iter().any(|c| c.library == out)) {
                            held.copies.retain(|c| c.library != out);
                            records.insert(held.id);
                        }
                        for t in rows.tracks.iter_mut().filter(|t| t.facts.folders.contains(&out)) {
                            t.facts.folders.retain(|f| *f != out);
                            tracks.insert(t.id);
                        }
                        let (left, emptied): (Vec<Track>, Vec<Track>) = std::mem::take(&mut rows.tracks).into_iter().partition(|t| !t.facts.folders.is_empty());
                        rows.tracks = left;
                        tracks.extend(emptied.iter().map(|t| t.id));
                    }
                }
                _ => {}
            }
        }
        let (mut set, mut gone) = (Rows::default(), Gone::default());
        for id in tracks {
            match rows.tracks.iter().find(|t| t.id == id) {
                Some(t) => {
                    set.tracks.push(t.clone());
                    records.insert(t.record);
                    artists.extend(t.artist_ids.iter().copied());
                }
                None => gone.tracks.push(id),
            }
        }
        for held in records.iter().filter_map(|id| rows.records.iter().find(|r| r.id == *id)) {
            set.records.push(held.clone());
            artists.extend(held.artist_ids.iter().copied());
        }
        set.artists = artists.iter().filter_map(|id| rows.artists.iter().find(|a| a.id == *id)).cloned().collect();
        for id in libraries {
            match rows.libraries.iter().find(|l| l.id == id) {
                Some(l) => set.libraries.push(l.clone()),
                None => gone.libraries.push(id),
            }
        }
        (set, gone)
    }

    #[test]
    fn changes_applied_equal_a_whole_build_of_the_result() {
        let (mut rows, _) = made_up(400, 7);
        let (fresh, _) = made_up(40, 8);
        let mut r = Rng::seeded(11);
        let mut index = Index::build(0, &rows).unwrap();
        let (mut next, mut rev) = (1_000_000, 0);
        let (mut emptied, mut words) = (0, BTreeSet::new());
        for round in 0..400 {
            let (set, gone) = a_batch(&mut rows, &mut r, &mut next, &fresh.tracks);
            let changes = Changes::new(rev, rev + 1, &set, gone);
            rev += 1;
            // As it reaches a device: as bytes.
            index.apply(&Changes::load(&changes.save()).unwrap()).unwrap();
            let whole = Index::build(rev, &rows).unwrap();
            assert_eq!(index.rows(), whole.rows(), "round {round}");
            assert!(index == whole, "round {round}: the same bytes");
            emptied += rows.records.len() - whole.rows().records.len();
            words.insert(whole.packed.words.len());
        }
        assert!(emptied > 0, "a record has gone with its last track");
        assert!(words.len() > 50, "the words changed");
        assert!(Index::load(&index.save()).unwrap() == index);
    }

    #[test]
    fn a_change_replaces_a_row_whole() {
        let mut rows = small();
        word_of(&mut rows, 0, "Dub");
        let mut index = built(&rows);
        // Its words and its copies as they stand now, not added to what was held.
        rows.tracks[0].facts.genres = vec!["Jazz".into()];
        rows.records[1].copies.remove(0);
        let set = Rows { tracks: vec![rows.tracks[0].clone()], records: vec![rows.records[1].clone()], ..Rows::default() };
        index.apply(&Changes::new(1, 2, &set, Gone::default())).unwrap();
        assert_eq!(index.track(100).unwrap().facts.genres, ["Jazz"]);
        assert_eq!(index.record(20).unwrap().copies, rows.records[1].copies);
        assert!(!index.packed.words.bytes.contains("Dub"), "a word nothing says is not kept");
        assert!(index == Index::build(2, &rows).unwrap());
    }

    #[test]
    fn a_record_sent_with_no_copies_has_none_left() {
        let mut rows = small();
        let mut index = built(&rows);
        rows.records[0].copies.clear();
        index.apply(&Changes::new(1, 2, &Rows { records: vec![rows.records[0].clone()], ..Rows::default() }, Gone::default())).unwrap();
        assert!(index.record(10).unwrap().copies.is_empty());
        assert_eq!(index.record(20).unwrap().copies.len(), 2);
    }

    #[test]
    fn what_nothing_names_leaves_and_comes_back_with_its_row() {
        let rows = small();
        let mut index = built(&rows);
        // Track 200 was all of record 20 and all artist 2 had.
        index.apply(&Changes::new(1, 2, &Rows::default(), Gone { tracks: vec![200], ..Gone::default() })).unwrap();
        assert!(index.record(20).is_none() && index.artist(2).is_none());
        assert_eq!(index.record(10).unwrap().copies.len(), 1);
        // Left out of the rows it is refused, as nothing here says what it was.
        let back = Rows { tracks: vec![rows.tracks[2].clone()], ..Rows::default() };
        assert_eq!(index.apply(&Changes::new(2, 3, &back, Gone::default())), Err(Refused::Unknown { what: "record", id: 20 }));
        let back = Rows { records: vec![rows.records[1].clone()], artists: vec![rows.artists[1].clone()], ..back };
        index.apply(&Changes::new(2, 3, &back, Gone::default())).unwrap();
        assert!(index == Index::build(3, &rows).unwrap());
    }

    #[test]
    fn a_build_keeps_nothing_nothing_names() {
        let mut rows = small();
        rows.artists.push(artist(3));
        rows.records.push(record(30, 3, &[0]));
        let held = built(&rows).rows();
        assert_eq!(held.artists.len(), 2);
        assert_eq!(held.records.len(), 2);
        assert!(built(&rows) == built(&small()));
    }

    #[test]
    fn what_leaves_and_was_not_held_is_no_fault() {
        let mut index = built(&small());
        let before = index.rows();
        index.apply(&Changes::new(1, 2, &Rows::default(), Gone { libraries: vec![55], tracks: vec![999] })).unwrap();
        assert_eq!(index.rows(), before);
        assert_eq!(index.rev(), 2);
    }

    #[test]
    fn changes_that_do_not_start_where_the_index_stands_refuse_and_leave_it_as_it_was() {
        let mut index = built(&small());
        let before = index.save();
        let rows = Rows { tracks: vec![track(300, 10, 1, 0)], ..Rows::default() };
        assert_eq!(index.apply(&Changes::new(0, 2, &rows, Gone::default())), Err(Refused::Rev { at: 1, from: 0 }));
        assert_eq!(index.apply(&Changes::new(2, 3, &rows, Gone::default())), Err(Refused::Rev { at: 1, from: 2 }));
        assert_eq!(index.apply(&Changes::new(1, 0, &rows, Gone::default())), Err(Refused::Rev { at: 1, from: 1 }), "running back");
        assert_eq!(index.save(), before);
        index.apply(&Changes::new(1, 2, &rows, Gone::default())).unwrap();
        assert_eq!(index.rev(), 2);
    }

    #[test]
    fn a_row_naming_what_is_not_there_refuses_and_leaves_the_index_as_it_was() {
        let mut index = built(&small());
        let before = index.save();
        let refused = |index: &mut Index, set: Rows, gone: Gone| index.apply(&Changes::new(1, 2, &set, gone)).unwrap_err();
        let tracks = |t: Track| Rows { tracks: vec![t], ..Rows::default() };
        assert_eq!(refused(&mut index, tracks(track(300, 77, 1, 0)), Gone::default()), Refused::Unknown { what: "record", id: 77 });
        assert_eq!(refused(&mut index, tracks(track(300, 10, 77, 0)), Gone::default()), Refused::Unknown { what: "artist", id: 77 });
        assert_eq!(refused(&mut index, tracks(track(300, 10, 1, 77)), Gone::default()), Refused::Unknown { what: "library", id: 77 });
        assert_eq!(refused(&mut index, Rows::default(), Gone { libraries: vec![1], ..Gone::default() }), Refused::Unknown { what: "library", id: 1 }, "a library gone with copies in it");
        let mut rows = small();
        rows.records[0].copies[0].library = 77;
        assert_eq!(refused(&mut index, Rows { records: vec![rows.records[0].clone()], ..Rows::default() }, Gone::default()), Refused::Unknown { what: "library", id: 77 });
        assert_eq!(Index::build(1, &rows).unwrap_err(), Refused::Unknown { what: "library", id: 77 });
        assert_eq!(index.save(), before);
    }

    /// `bytes` with the version in its header changed.
    fn another_version(mut bytes: Vec<u8>, version: u32) -> Vec<u8> {
        bytes[4..8].copy_from_slice(&version.to_le_bytes());
        bytes
    }

    #[test]
    fn a_version_mismatch_refuses() {
        let index = built(&small());
        for found in [VERSION + 1, VERSION - 1, 0, u32::MAX] {
            let refused = Refused::Version { found, wants: VERSION };
            assert_eq!(Index::load(&another_version(index.save(), found)), Err(refused));
            let changes = Changes::new(1, 2, &Rows::default(), Gone::default());
            assert_eq!(Changes::load(&another_version(changes.save(), found)).unwrap_err(), refused);
        }
        let sounds = Sounds::build(1, &[(100, vec![1.0, 0.0])]);
        let refused = Refused::Version { found: SOUNDS_VERSION + 1, wants: SOUNDS_VERSION };
        assert_eq!(Sounds::load(&another_version(sounds.save(), SOUNDS_VERSION + 1)), Err(refused));
    }

    #[test]
    fn a_file_of_another_kind_refuses() {
        let (index, changes) = (built(&small()), Changes::new(1, 2, &small(), Gone::default()));
        assert_eq!(Index::load(&changes.save()), Err(Refused::Damaged));
        assert_eq!(Changes::load(&index.save()).unwrap_err(), Refused::Damaged);
        assert_eq!(Sounds::load(&index.save()), Err(Refused::Damaged));
        assert_eq!(Index::load(b""), Err(Refused::Damaged));
        // Whole and right in all but what it says it is: only the name at its head tells.
        let mut named = index.save();
        named[..4].copy_from_slice(&CHANGES);
        assert_eq!(Index::load(&named), Err(Refused::Damaged));
    }

    #[test]
    fn a_file_cut_short_refuses_at_any_length() {
        let bytes = built(&small()).save();
        for length in 0..bytes.len() {
            assert_eq!(Index::load(&bytes[..length]), Err(Refused::Damaged), "at {length}");
        }
    }

    #[test]
    fn a_byte_changed_anywhere_past_the_version_refuses() {
        let bytes = built(&small()).save();
        for at in (0..4).chain(8..bytes.len()) {
            let mut changed = bytes.clone();
            changed[at] ^= 0x40;
            assert_eq!(Index::load(&changed), Err(Refused::Damaged), "at {at}");
        }
    }

    /// `bytes`' body changed, and sealed again as if it had been written so: what a writer's own fault looks like. Nothing it holds
    /// may panic, as `panic = "abort"` in the app's bridge would end the app at every start.
    #[test]
    fn nothing_read_from_a_file_can_panic() {
        let (mut rows, _) = made_up(40, 13);
        rows.tracks.truncate(20);
        let bytes = built(&rows).save();
        let mut r = Rng::seeded(17);
        let body = |bytes: &[u8]| bytes[HEADER..].to_vec();
        for _ in 0..3_000 {
            let mut changed = body(&bytes);
            for _ in 0..1 + r.below(3) {
                let at = r.below(changed.len());
                changed[at] = r.next_u64() as u8;
            }
            let mut out = vec![0; HEADER];
            out.extend(changed);
            let sealed = sealed(out, INDEX, VERSION);
            // Read or refused, and what is read can be gone through.
            if let Ok(index) = Index::load(&sealed) {
                index.rows();
                index.facts(None);
                index.radio(&[], &HashMap::new());
            }
        }
    }

    #[test]
    fn the_crates_files_come_as_the_server_hands_them_in() {
        let at = |id: i64, born: Option<i64>, folder: i64, line: &str| {
            let mut t = track(id, 10, 1, folder);
            (t.facts.born, t.facts.rel) = (born, line.to_string());
            t
        };
        let rows = Rows {
            tracks: vec![at(1, Some(5), 1, "a.flac"), at(2, Some(5), 0, "z.flac"), at(3, None, 0, "a.flac"), at(4, Some(9), 1, "b.flac"), at(5, Some(5), 0, "b.flac")],
            ..small()
        };
        let order: Vec<i64> = built(&rows).facts(None).iter().filter_map(|f| f.track).collect();
        assert_eq!(order, [4, 5, 2, 1, 3], "newest first, none last, then the folder, then the path");
    }

    #[test]
    fn a_tracks_facts_are_what_the_crates_read_of_its_record() {
        let rows = small();
        let facts = built(&rows).facts(None);
        let of = |id: i64| facts.iter().find(|f| f.track == Some(id)).unwrap();
        assert_eq!(of(100).album.as_deref(), Some("al-10"));
        assert!(Arc::ptr_eq(of(100).album.as_ref().unwrap(), of(101).album.as_ref().unwrap()), "one copy shared by its tracks");
        assert_eq!(of(200).folders, [1]);
        assert_eq!(of(200).song().as_deref(), Some("tr-200"));
    }

    #[test]
    fn an_artist_goes_by_every_way_the_files_write_them() {
        let mut rows = small();
        rows.artists[0].spelled = vec!["ARTIST ONE".into(), "Artist one".into()];
        let ids = built(&rows).artist_ids();
        assert_eq!(ids.get("Artist 1").map(String::as_str), Some("ar-1"));
        assert_eq!(ids.get("ARTIST ONE").map(String::as_str), Some("ar-1"));
        assert_eq!(ids.get("Artist one").map(String::as_str), Some("ar-1"));
        assert_eq!(ids.get("Artist 2").map(String::as_str), Some("ar-2"));
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn an_artist_seed_finds_their_tracks_through_the_index() {
        let (mut rows, heard) = made_up(600, 19);
        // The first is written one way by the files and named another by the library.
        let first = &mut rows.artists[0];
        first.spelled = vec![std::mem::replace(&mut first.name, "A name no file holds".into())];
        let index = built(&rows);
        let sounds = Sounds::build(1, &heard);
        let facts = index.facts(Some(&sounds));
        let alike = Alike::of(&facts, &HashMap::new(), &index.artist_ids());
        for a in rows.artists.iter().take(40) {
            let id = artist_id(a.id);
            let theirs = rows.tracks.iter().filter(|t| t.artist_ids.contains(&a.id)).count();
            assert!(theirs > 0 || rows.records.iter().any(|r| r.artist_ids.contains(&a.id)), "{id}: named");
            assert!(alike.seed_places(&id).len() >= theirs, "{id}: their tracks, any credit");
        }
    }

    /// Nothing but its sound says what a track is like here: a record's lie close, the two records far apart.
    #[test]
    fn a_sounds_like_hears_the_sounds_kept_apart() {
        let rows = Rows {
            libraries: vec![library(0)],
            artists: vec![artist(1)],
            records: vec![record(10, 1, &[0]), record(20, 1, &[0])],
            tracks: (1..=8).map(|id| track(id, if id <= 4 { 10 } else { 20 }, 1, 0)).collect(),
        };
        let near = |id: i64| 0.05 * id as f32;
        let heard: Vec<(i64, Vec<f32>)> = (1..=8).map(|id| (id, if id <= 4 { vec![1.0, near(id), 0.0] } else { vec![0.0, near(id), 1.0] })).collect();
        let index = built(&rows);
        let facts = index.facts(Some(&Sounds::build(1, &heard)));
        let (membership, off, by) = (HashMap::new(), HashSet::new(), Weights::EQUAL);
        let alike = Alike::of(&facts, &membership, &index.artist_ids());
        let world = World { membership: &membership, favourites: None, favourite_albums: None, uses: None, now: 0, fits: None };
        let view = View { files: &facts, switched_off: &off, world, by: &by, alike: || &alike };
        let like = crate::crates::like_crate("like:tr-6", "Like", vec![ids::song(6)], 4);
        let mut lines = view.select(&like.conditions, false);
        lines.sort();
        assert_eq!(lines, ["r20/5.flac", "r20/6.flac", "r20/7.flac", "r20/8.flac"]);
    }

    #[test]
    fn radio_reads_the_index_in_the_servers_order_and_numbers() {
        let at = |id: i64, record: i64, disc: Option<u32>, number: Option<u32>, line: &str, artists: &[i64]| {
            let mut t = track(id, record, artists[0], 0);
            (t.disc, t.number, t.facts.rel, t.artist_ids) = (disc, number, line.to_string(), artists.to_vec());
            t
        };
        let rows = Rows {
            libraries: vec![library(0)],
            artists: vec![artist(1), artist(2)],
            // The album's id as text: al-10 comes before al-9.
            records: vec![record(9, 1, &[0]), record(10, 2, &[0])],
            tracks: vec![
                at(1, 9, Some(1), Some(1), "b/1.flac", &[1]),
                at(2, 10, Some(2), Some(1), "a/4.flac", &[2, 2]),
                at(3, 10, Some(1), Some(2), "a/3.flac", &[2, 1]),
                at(4, 10, None, None, "a/2.flac", &[2]),
                at(5, 10, Some(1), Some(2), "a/1.flac", &[1]),
            ],
        };
        let index = built(&rows);
        let radio = index.radio(&index.facts(None), &HashMap::from([("tr-3".to_string(), vec![4, 7])]));
        let got: Vec<&str> = radio.songs.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(got, ["tr-4", "tr-5", "tr-3", "tr-2", "tr-1"], "the album as text, then disc and track, none first, then the path");
        let numbers: Vec<u32> = radio.songs.iter().map(|s| s.album_n).collect();
        assert_eq!(numbers, [0, 0, 0, 0, 1], "as met");
        assert_eq!(radio.artist_ids, ["ar-2", "ar-1"], "as met");
        let of = |id: &str| &radio.songs[radio.by_id[id]];
        assert_eq!(of("tr-2").like.artists, [0, 0], "a name twice stays twice");
        assert_eq!(of("tr-3").like.artists, [0, 1]);
        assert_eq!(of("tr-3").like.playlists, [4, 7]);
        assert_eq!((of("tr-1").album.as_str(), of("tr-1").album_id.as_str()), ("Record 9", "al-9"));
        assert_eq!(of("tr-1").folders, [0]);
    }

    #[test]
    fn a_sound_in_eight_bits_points_the_same_way() {
        let (_, heard) = made_up(200, 23);
        let sounds = Sounds::build(9, &heard);
        for (track, sound) in &heard {
            let unit = similarity::mean_sound(&[sound]).unwrap();
            let kept = sounds.of(*track).unwrap();
            let length: f32 = kept.iter().map(|x| x * x).sum();
            assert!((length - 1.0).abs() < 1e-4, "of unit length: {length}");
            let cosine: f32 = kept.iter().zip(&unit).map(|(a, b)| a * b).sum();
            assert!(cosine > 0.999, "{cosine}");
        }
        assert!(sounds.of(2).is_none(), "not heard");
    }

    #[test]
    fn a_saved_sounds_loads_as_the_same_sounds() {
        let (_, heard) = made_up(200, 23);
        let sounds = Sounds::build(9, &heard);
        let loaded = Sounds::load(&sounds.save()).unwrap();
        assert!(loaded == sounds);
        assert_eq!(loaded.rev(), 9);
        assert_eq!(loaded.of(heard[0].0), sounds.of(heard[0].0));
        assert_eq!(Sounds::build(1, &[(1, vec![0.0, 0.0])]).of(1), None, "all zero: not heard");
    }

    #[test]
    fn sounds_changed_equal_a_whole_build_of_the_result() {
        let (_, mut heard) = made_up(200, 29);
        let mut sounds = Sounds::build(1, &heard);
        let gone: Vec<i64> = heard.drain(..10).map(|(t, _)| t).collect();
        heard[0].1[0] += 1.0;
        let first = heard[0].0;
        heard.push((9_999, vec![0.5; 200]));
        let fresh = Sounds::build(2, &heard);
        let moved: Vec<i64> = gone.iter().copied().chain([first, 9_999]).collect();
        let changes = SoundChanges::load(&fresh.changes(1, &moved).save()).unwrap();
        assert_eq!((changes.counts(), changes.to()), ((2, 10), 2));
        assert_eq!(sounds.apply(&fresh.changes(0, &moved)), Err(Refused::Rev { at: 1, from: 0 }));
        assert!(sounds.rev() == 1, "held as it was");
        sounds.apply(&changes).unwrap();
        assert_eq!(sounds.save(), fresh.save());
    }

    #[test]
    fn sound_changes_of_another_kind_or_version_or_cut_short_are_refused() {
        let sounds = Sounds::build(2, &[(1, vec![1.0, 0.0])]);
        let bytes = sounds.changes(1, &[1, 2]).save();
        assert!(SoundChanges::load(&bytes).is_ok());
        assert_eq!(SoundChanges::load(&bytes[..bytes.len() - 1]).err(), Some(Refused::Damaged));
        assert_eq!(SoundChanges::load(&sounds.save()).err(), Some(Refused::Damaged), "whole sounds are no changes");
        assert_eq!(SoundChanges::load(&another_version(bytes, SOUNDS_VERSION + 1)).err(), Some(Refused::Version { found: SOUNDS_VERSION + 1, wants: SOUNDS_VERSION }));
    }

    #[test]
    fn sounds_print_what_they_say_as_a_device_holds_it() {
        let two = |x: f32| Sounds::build(1, &[(1, vec![1.0, 0.5]), (2, vec![x, 0.25])]);
        let (a, b) = (two(1.0).prints(), two(0.5).prints());
        assert_eq!((a[0], a.len()), (b[0], 2), "the other's did not move");
        assert_ne!(a[1], b[1]);
        let kept = two(1.0 + 1e-6).prints();
        assert_eq!(a, kept, "what eight bits cannot hold moves nothing a device has");
    }

    /// The sounds as a server holds them: of unit length, made so twice as `read_crate_facts` makes them.
    fn as_read(heard: &[(i64, Vec<f32>)]) -> HashMap<i64, Arc<[f32]>> {
        heard.iter().filter_map(|(t, s)| Some((*t, Arc::from(similarity::mean_sound(&[&similarity::mean_sound(&[s])?])?)))).collect()
    }

    /// How close a device's Sounds like comes to the server's for one seed: of the server's list, how many are on the device's; of
    /// its first 20 how many are among the device's first 20; and the worst place on the device's list, from 1, of its first 10.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Closeness {
        of_list: usize,
        of_first: usize,
        worst_of_ten: usize,
    }

    /// What "close" is, said before anything was measured (PREVIEW2's D11). A device shows a Sounds like of 200 tracks: at most one
    /// in twenty of them another track than the server's, at most two of its first twenty, and nothing the server puts in the first
    /// ten past the twentieth. A listener can tell a list with another first track from the server's, and not one that swaps
    /// the 150th for the 210th. The order within is not held: two tracks about as close swap places on rounding alone.
    const CLOSE: Closeness = Closeness { of_list: 190, of_first: 18, worst_of_ten: 20 };

    impl Closeness {
        fn of(server: &[String], device: &[String]) -> Self {
            let on = |list: &[String], line: &String| list.iter().position(|l| l == line);
            let first = &server[..20.min(server.len())];
            Closeness {
                of_list: server.iter().filter(|l| on(device, l).is_some()).count(),
                of_first: first.iter().filter(|l| on(&device[..20.min(device.len())], l).is_some()).count(),
                worst_of_ten: server.iter().take(10).map(|l| on(device, l).map_or(usize::MAX, |at| at + 1)).max().unwrap_or(0),
            }
        }

        fn holds(self) -> bool {
            self.of_list >= CLOSE.of_list && self.of_first >= CLOSE.of_first && self.worst_of_ten <= CLOSE.worst_of_ten
        }
    }

    /// A Sounds like of each of `seeds` from the sounds as the server holds them and as a device does, by a listener's Similar by.
    fn compared(index: &Index, heard: &[(i64, Vec<f32>)], seeds: &[i64], by: &Weights) -> Vec<Closeness> {
        let exact = as_read(heard);
        let mut server = index.facts(None);
        server.iter_mut().for_each(|f| f.sound = f.track.and_then(|t| exact.get(&t).cloned()));
        let device = index.facts(Some(&Sounds::load(&Sounds::build(1, heard).save()).unwrap()));
        let (membership, off, names) = (HashMap::new(), HashSet::new(), index.artist_ids());
        let (on_server, on_device) = (Alike::of(&server, &membership, &names), Alike::of(&device, &membership, &names));
        let world = || World { membership: &membership, favourites: None, favourite_albums: None, uses: None, now: 1_800_000_000, fits: None };
        let (a, b) = (
            View { files: &server, switched_off: &off, world: world(), by, alike: || &on_server },
            View { files: &device, switched_off: &off, world: world(), by, alike: || &on_device },
        );
        let list = |seed: i64| crate::crates::like_crate("like:x", "Like", vec![ids::song(seed)], crate::crates::LIKE_COUNT).conditions;
        seeds.iter().map(|seed| Closeness::of(&a.select(&list(*seed), false), &b.select(&list(*seed), false))).collect()
    }

    /// Sounds far closer together than `made_up`'s, as a library of few styles has them: a few centres, a record close to its centre
    /// and a track close to its record, so that the 200 closest of a seed lie within a hair of each other.
    fn crowded(rows: &Rows, heard: &[(i64, Vec<f32>)], centres: usize, r: &mut Rng) -> Vec<(i64, Vec<f32>)> {
        let dims = heard[0].1.len();
        let noise = |r: &mut Rng, by: f32| (0..dims).map(|_| by * (r.unit() as f32 - 0.5)).collect::<Vec<f32>>();
        let centres: Vec<Vec<f32>> = (0..centres).map(|_| noise(r, 1.0)).collect();
        let records: HashMap<i64, Vec<f32>> = rows
            .records
            .iter()
            .map(|record| {
                let (centre, by) = (&centres[r.below(centres.len())], noise(r, 0.3));
                (record.id, centre.iter().zip(by).map(|(a, b)| a + b).collect())
            })
            .collect();
        let record_of: HashMap<i64, i64> = rows.tracks.iter().map(|t| (t.id, t.record)).collect();
        heard.iter().map(|(track, _)| (*track, records[&record_of[track]].iter().zip(noise(r, 0.2)).map(|(a, b)| a + b).collect())).collect()
    }

    fn every_nth(heard: &[(i64, Vec<f32>)], seeds: usize) -> Vec<i64> {
        heard.iter().step_by((heard.len() / seeds).max(1)).take(seeds).map(|h| h.0).collect()
    }

    #[test]
    fn a_sounds_like_from_eight_bits_stays_close_to_the_servers() {
        let (rows, heard) = made_up(3_000, 31);
        let index = built(&rows);
        for (what, sounds) in [("far apart", heard.clone()), ("crowded", crowded(&rows, &heard, 12, &mut Rng::seeded(5)))] {
            for by in [Weights::EQUAL, Weights::ranked(&[Signal::Sound])] {
                for close in compared(&index, &sounds, &every_nth(&sounds, 12), &by) {
                    assert!(close.holds(), "{what}, by {by:?}: {close:?}");
                }
            }
        }
    }

    /// `seeds` compared, as the worst of each number and their mean.
    fn reported(what: &str, closeness: &[Closeness]) {
        let n = closeness.len() as f64;
        let mean = |f: fn(&Closeness) -> usize| closeness.iter().map(|c| f(c) as f64).sum::<f64>() / n;
        let held = closeness.iter().filter(|c| c.holds()).count();
        println!(
            "{what}: close for {held} of {} seeds; of 200 mean {:.1} worst {}; of the first 20 mean {:.1} worst {}; the first 10's last place mean {:.1} worst {}",
            closeness.len(),
            mean(|c| c.of_list),
            closeness.iter().map(|c| c.of_list).min().unwrap_or(0),
            mean(|c| c.of_first),
            closeness.iter().map(|c| c.of_first).min().unwrap_or(0),
            mean(|c| c.worst_of_ten),
            closeness.iter().map(|c| c.worst_of_ten).max().unwrap_or(0),
        );
    }

    /// D11's measure, on made-up vectors. Their centres are random, so what this says of a real library's closest tracks is how
    /// much eight bits move a score, not whether a real seed's neighbours lie as near: D22 reads that off real ones.
    #[test]
    #[ignore = "measures: --ignored --nocapture"]
    fn measured_sounds_like_from_eight_bits_at_34_000_tracks() {
        let (rows, heard) = made_up(34_000, 37);
        let index = built(&rows);
        let (every_signal, sound_alone) = (Weights::EQUAL, Weights::ranked(&[Signal::Sound]));
        reported("far apart, every signal", &compared(&index, &heard, &every_nth(&heard, 40), &every_signal));
        reported("far apart, sound alone", &compared(&index, &heard, &every_nth(&heard, 40), &sound_alone));
        for centres in [40, 12, 3] {
            let sounds = crowded(&rows, &heard, centres, &mut Rng::seeded(centres as u64));
            reported(&format!("crowded, {centres} centres, every signal"), &compared(&index, &sounds, &every_nth(&sounds, 40), &every_signal));
            reported(&format!("crowded, {centres} centres, sound alone"), &compared(&index, &sounds, &every_nth(&sounds, 40), &sound_alone));
        }
    }

    #[test]
    fn an_accounts_marks_come_back_as_they_went_in() {
        let rows = vec![
            Mark { track: 9, fit: Some(0.123_456_789_012), skipped: Some(0.5), plays: 3, last: Some(1_700_000_000), rating: Some(4), skips: 1 },
            Mark { track: 2, plays: 1, ..Mark::default() },
            Mark { track: 5, fit: Some(0.25), ..Mark::default() },
        ];
        let marks = Marks::load(&Marks::build(1_800_000_000, &rows).save()).unwrap();
        assert_eq!(marks.at(), 1_800_000_000);
        assert_eq!(marks.of(9).as_ref(), Some(&rows[0]), "a fit exact: a share of the library is cut by rank");
        assert_eq!(marks.of(2).as_ref(), Some(&rows[1]));
        assert_eq!(marks.of(3), None);
        assert_eq!(marks.rows().iter().map(|m| m.track).collect::<Vec<_>>(), [2, 5, 9], "by track");
        assert!(marks == Marks::build(1_800_000_000, &[rows[2].clone(), rows[0].clone(), rows[1].clone()]));
    }

    #[test]
    fn marks_of_another_kind_or_version_or_cut_short_are_refused() {
        let bytes = Marks::build(1, &[Mark { track: 1, ..Mark::default() }]).save();
        assert_eq!(Marks::load(&bytes[..bytes.len() - 1]).err(), Some(Refused::Damaged));
        assert_eq!(Marks::load(&built(&small()).save()).err(), Some(Refused::Damaged));
        assert_eq!(Marks::load(&another_version(bytes, MARKS_VERSION + 1)).err(), Some(Refused::Version { found: MARKS_VERSION + 1, wants: MARKS_VERSION }));
    }

    /// Counts what is held, so a measure can say what a thing weighs in memory.
    struct Counted;

    static HELD: AtomicUsize = AtomicUsize::new(0);

    unsafe impl GlobalAlloc for Counted {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            HELD.fetch_add(layout.size(), Ordering::Relaxed);
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, at: *mut u8, layout: Layout) {
            HELD.fetch_sub(layout.size(), Ordering::Relaxed);
            unsafe { System.dealloc(at, layout) }
        }
    }

    #[global_allocator]
    static COUNTED: Counted = Counted;

    fn timed<T>(what: &str, work: impl FnOnce() -> T) -> T {
        let (start, before) = (Instant::now(), HELD.load(Ordering::Relaxed));
        let out = work();
        let (took, held) = (start.elapsed(), HELD.load(Ordering::Relaxed).saturating_sub(before));
        println!("{:>7} ms {:>7.1} MB  {what}", took.as_millis(), held as f64 / 1_000_000.0);
        out
    }

    /// What the index costs at `n` tracks, printed. In a test build, unoptimised: the bytes are what they are, the times only say
    /// how much there is to do, many times what a release build takes.
    fn measure(n: usize) {
        println!("--- {n} made-up tracks; a test build: {}", if cfg!(debug_assertions) { "unoptimised" } else { "optimised" });
        println!("     ms      held");
        let (rows, heard) = timed("made up", || made_up(n, 1));
        println!("{} records, {} artists, {} libraries, {} heard", rows.records.len(), rows.artists.len(), rows.libraries.len(), heard.len());
        let index = timed("build", || Index::build(1, &rows).unwrap());
        let bytes = timed("save", || index.save());
        let loaded = timed("load", || Index::load(&bytes).unwrap());
        assert!(loaded == index);
        println!("index: {} bytes, {} a track; {} words", bytes.len(), bytes.len() / n, index.packed.words.len());
        let changes = Changes::new(1, 2, &Rows { tracks: rows.tracks[..100].to_vec(), ..Rows::default() }, Gone::default());
        println!("100 tracks' changes: {} bytes", changes.save().len());
        let mut applied = index.clone();
        timed("apply 100 changed tracks", || applied.apply(&changes).unwrap());
        let sounds = timed("sounds, build", || Sounds::build(1, &heard));
        let sound_bytes = timed("sounds, save", || sounds.save());
        timed("sounds, load", || Sounds::load(&sound_bytes).unwrap());
        println!("sounds: {} bytes, {} a track heard", sound_bytes.len(), sound_bytes.len() / heard.len());
        let facts = timed("facts, with sounds", || index.facts(Some(&sounds)));
        let (membership, off, by) = (HashMap::new(), HashSet::new(), Weights::EQUAL);
        let alike = timed("alike", || Alike::of(&facts, &membership, &index.artist_ids()));
        let view = View {
            files: &facts,
            switched_off: &off,
            world: World { membership: &membership, favourites: None, favourite_albums: None, uses: None, now: 1_800_000_000, fits: None },
            by: &by,
            alike: || &alike,
        };
        let genre = facts.iter().flat_map(|f| &f.genres).next().unwrap().clone();
        let words = [Condition::new("genre", false, Value::AnyOf { values: vec![genre] }), Condition::new("bpm", false, Value::Range { min: Some(100.0), max: Some(140.0) })];
        let held = timed("a crate of two conditions (budget on the slowest device: 300 ms)", || view.select(&words, false));
        println!("it holds {}", held.len());
        let seeds: Vec<String> = rows.tracks[..12].iter().map(|t| ids::song(t.id)).collect();
        let like = crate::crates::like_crate("like:x", "Like", seeds.clone(), 200);
        let held = timed("a Sounds like of 12 seeds (budget on the slowest device: 300 ms)", || view.select(&like.conditions, false));
        println!("it holds {}", held.len());
        let radio = timed("radio's index", || index.radio(&facts, &HashMap::new()));
        let ask = Ask { mode: Mode::Similar, seed: seeds[..1].to_vec(), exclude: seeds[..1].to_vec(), ..Ask::default() };
        let picks = timed("a Radio pick (budget on the slowest device: 50 ms)", || choose(&radio, &ask, &HashMap::new(), None, &mut Rng::seeded(1)));
        println!("it picks {}", picks.len());
    }

    #[test]
    #[ignore = "measures: --ignored --nocapture"]
    fn measured_at_34_000_tracks() {
        measure(34_000);
    }

    #[test]
    #[ignore = "measures: --ignored --nocapture"]
    fn measured_at_100_000_tracks() {
        measure(100_000);
    }
}
