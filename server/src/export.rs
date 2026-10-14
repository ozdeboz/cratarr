//! Export (decision 0007; the panel's Export tab, decision 0025): playlists, optionally with their tracks, as one
//! download. `plan` resolves the playlists' lines to files in the Music folder (each file once, with its size and the
//! tags a flat name needs; missing ones kept apart for the report), `layout` decides the names inside the bundle and
//! the playlists' lines, and `stream` writes the ZIP as it goes (stored, sizes and CRCs in data descriptors, ZIP64
//! where a size or an offset needs it), so memory does not depend on the export's size. Its exact length is known
//! before the first byte, so the browser shows real progress.

use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

/// A file in the export: its path relative to the Music folder, its size, and its first artist and title tags.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub rel: String,
    pub size: u64,
    pub artist: Option<String>,
    pub title: Option<String>,
}

/// The playlists (by name, each its lines as indexes into `tracks`), every file once, and the lines whose file is
/// missing (playlist, line), which are left out.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub root: PathBuf,
    pub lists: Vec<(String, Vec<usize>)>,
    pub tracks: Vec<Track>,
    pub missing: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Bundle the tracks.
    pub tracks: bool,
    /// The tracks in one folder as `Artist - Title.ext`, instead of the library's folders.
    pub flat: bool,
    /// One playlist of this name instead of each (every track once, at its first place).
    pub combine: Option<String>,
}

/// What the export will hold, before starting.
#[derive(Debug, Serialize, PartialEq)]
pub struct Preview {
    pub tracks: usize,
    pub missing: usize,
    /// Only with tracks.
    pub bytes: Option<u64>,
}

/// Lines whose path stays inside the folder (no `..`, not absolute).
fn inside(rel: &str) -> bool {
    let p = Path::new(rel);
    !rel.is_empty() && !p.is_absolute() && p.components().all(|c| matches!(c, Component::Normal(_)))
}

/// Resolves the playlists' lines (relative to `music`) to files. `tags` gives each file's (artist, title).
pub fn plan(music: &Path, lists: Vec<(String, Vec<String>)>, tags: impl FnOnce(&[String]) -> Result<Vec<(Option<String>, Option<String>)>>) -> Result<Plan> {
    let mut index = std::collections::HashMap::<String, usize>::new();
    let mut rels: Vec<(String, u64)> = Vec::new();
    let mut missing = Vec::new();
    let mut out = Vec::new();
    for (name, lines) in lists {
        let mut at = Vec::new();
        for line in lines {
            if let Some(&i) = index.get(&line) {
                at.push(i);
                continue;
            }
            let size = inside(&line).then(|| std::fs::metadata(music.join(&line)).ok()).flatten().filter(|m| m.is_file()).map(|m| m.len());
            match size {
                Some(size) => {
                    index.insert(line.clone(), rels.len());
                    at.push(rels.len());
                    rels.push((line, size));
                }
                None => missing.push((name.clone(), line)),
            }
        }
        out.push((name, at));
    }
    let names: Vec<String> = rels.iter().map(|(r, _)| r.clone()).collect();
    let mut facts = tags(&names)?.into_iter();
    let tracks = rels
        .into_iter()
        .map(|(rel, size)| {
            let (artist, title) = facts.next().unwrap_or_default();
            Track { rel, size, artist, title }
        })
        .collect();
    Ok(Plan { root: music.to_path_buf(), lists: out, tracks, missing })
}

/// The playlists as written: combined into one (each track once, at its first place), or as they are.
fn lists(plan: &Plan, o: &Options) -> Vec<(String, Vec<usize>)> {
    match &o.combine {
        Some(name) => {
            let mut seen = std::collections::HashSet::new();
            let all = plan.lists.iter().flat_map(|(_, at)| at.iter().copied()).filter(|i| seen.insert(*i)).collect();
            vec![(name.clone(), all)]
        }
        None => plan.lists.clone(),
    }
}

/// The tracks the written playlists hold, each once, in the order they first appear.
fn used(lists: &[(String, Vec<usize>)]) -> Vec<usize> {
    let mut seen = std::collections::HashSet::new();
    lists.iter().flat_map(|(_, at)| at.iter().copied()).filter(|i| seen.insert(*i)).collect()
}

pub fn preview(plan: &Plan, o: &Options) -> Preview {
    let written = lists(plan, o);
    let used = used(&written);
    let missing = if o.combine.is_some() {
        plan.missing.iter().map(|(_, l)| l).collect::<std::collections::HashSet<_>>().len()
    } else {
        plan.missing.len()
    };
    Preview { tracks: used.len(), missing, bytes: o.tracks.then(|| used.iter().map(|&i| plan.tracks[i].size).sum()) }
}

/// A name that is safe as one file name on any system: no separators or reserved characters, no control characters,
/// no trailing dots or spaces.
pub fn safe_name(s: &str) -> String {
    let cleaned: String = s.chars().map(|c| if c.is_control() || r#"/\:*?"<>|"#.contains(c) { '_' } else { c }).collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']).to_string();
    if trimmed.is_empty() { "Untitled".into() } else { trimmed }
}

/// `name` if it is free (ignoring case), else `stem (2).ext`, `stem (3).ext`…
fn unique(name: String, taken: &mut std::collections::HashSet<String>) -> String {
    if taken.insert(name.to_lowercase()) {
        return name;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name.as_str(), ""),
    };
    (2..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|c| taken.insert(c.to_lowercase()))
        .expect("an unused name")
}

/// What goes in the bundle: the tracks (name inside, track), the playlist files (name, contents) and the report of
/// missing tracks.
#[derive(Debug, PartialEq)]
pub struct Layout {
    pub files: Vec<(String, usize)>,
    pub lists: Vec<(String, String)>,
    pub report: Option<String>,
}

pub fn layout(plan: &Plan, o: &Options) -> Layout {
    let written = lists(plan, o);
    let mut taken = std::collections::HashSet::new();
    // Each track's name inside the bundle (and so its line): its path in the library, or Artist - Title in one folder.
    let mut inside = std::collections::HashMap::new();
    let mut files = Vec::new();
    for i in used(&written) {
        let t = &plan.tracks[i];
        let name = if o.tracks && o.flat {
            let ext = Path::new(&t.rel).extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
            let stem = match (&t.artist, &t.title) {
                (Some(a), Some(ti)) => format!("{a} - {ti}"),
                (None, Some(ti)) => ti.clone(),
                _ => Path::new(&t.rel).file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string(),
            };
            unique(format!("{}{ext}", safe_name(&stem)), &mut taken)
        } else {
            t.rel.clone()
        };
        if o.tracks {
            files.push((name.clone(), i));
        }
        inside.insert(i, name);
    }
    let mut list_names = std::collections::HashSet::new();
    let lists = written
        .iter()
        .map(|(name, at)| {
            let body: String = at.iter().map(|i| format!("{}\r\n", inside[i])).collect();
            (unique(format!("{}.m3u", safe_name(name)), &mut list_names), body)
        })
        .collect();
    let report = (o.tracks && !plan.missing.is_empty()).then(|| {
        let mut r = String::from("These tracks were left out: their files are missing from the music folder.\r\n\r\n");
        for (p, line) in &plan.missing {
            r.push_str(&format!("{p}: {line}\r\n"));
        }
        r
    });
    Layout { files, lists, report }
}

pub const REPORT: &str = "Missing tracks.txt";

/// One playlist without tracks downloads as its `.m3u`; anything else as one `.zip`.
pub fn is_zip(plan: &Plan, o: &Options) -> bool {
    o.tracks || lists(plan, o).len() > 1
}

/// The download's file name.
pub fn download_name(plan: &Plan, o: &Options) -> String {
    let written = lists(plan, o);
    let base = match (&o.combine, written.as_slice()) {
        (Some(n), _) => safe_name(n),
        (None, [(n, _)]) => safe_name(n),
        _ => "Playlists".into(),
    };
    format!("{base}.{}", if is_zip(plan, o) { "zip" } else { "m3u" })
}

// The ZIP format (APPNOTE 6.3): stored entries, each a local header, the data and a data descriptor (the CRC is known
// only after reading); the central directory and its end, with ZIP64 records where 32 bits do not hold a value.

const MAX32: u64 = 0xFFFF_FFFF;

const CRC_TABLE: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

/// CRC-32 (as ZIP uses it), continued over chunks: start with 0.
pub fn crc32(crc: u32, data: &[u8]) -> u32 {
    let mut c = !crc;
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

/// An entry being written: where its local header starts, and whether its sizes need 64 bits.
pub struct Entry {
    name: String,
    offset: u64,
    zip64: bool,
}

/// The writer's bookkeeping; every method returns the bytes to send next.
pub struct Zip {
    offset: u64,
    central: Vec<u8>,
    count: u64,
    time: u16,
    date: u16,
}

impl Zip {
    pub fn new(time: u16, date: u16) -> Self {
        Self { offset: 0, central: Vec::new(), count: 0, time, date }
    }

    /// The local header of an entry of about `size` bytes (its CRC and exact size follow in its descriptor).
    pub fn begin(&mut self, name: &str, size: u64) -> (Vec<u8>, Entry) {
        let zip64 = size >= MAX32;
        let mut h = Vec::with_capacity(30 + name.len() + 20);
        h.extend(0x0403_4b50u32.to_le_bytes());
        h.extend((if zip64 { 45u16 } else { 20u16 }).to_le_bytes());
        h.extend(0x0808u16.to_le_bytes()); // sizes in the descriptor; UTF-8 names
        h.extend(0u16.to_le_bytes()); // stored
        h.extend(self.time.to_le_bytes());
        h.extend(self.date.to_le_bytes());
        h.extend(0u32.to_le_bytes());
        let size32 = if zip64 { MAX32 as u32 } else { 0 };
        h.extend(size32.to_le_bytes());
        h.extend(size32.to_le_bytes());
        h.extend((name.len() as u16).to_le_bytes());
        h.extend((if zip64 { 20u16 } else { 0 }).to_le_bytes());
        h.extend(name.as_bytes());
        if zip64 {
            h.extend(1u16.to_le_bytes());
            h.extend(16u16.to_le_bytes());
            h.extend(0u64.to_le_bytes());
            h.extend(0u64.to_le_bytes());
        }
        let e = Entry { name: name.to_string(), offset: self.offset, zip64 };
        self.offset += h.len() as u64;
        (h, e)
    }

    /// After `size` bytes of data with this CRC: the entry's descriptor (and its record for the central directory).
    pub fn finish(&mut self, e: Entry, crc: u32, size: u64) -> Vec<u8> {
        self.offset += size;
        let mut d = Vec::with_capacity(24);
        d.extend(0x0807_4b50u32.to_le_bytes());
        d.extend(crc.to_le_bytes());
        if e.zip64 {
            d.extend(size.to_le_bytes());
            d.extend(size.to_le_bytes());
        } else {
            d.extend((size as u32).to_le_bytes());
            d.extend((size as u32).to_le_bytes());
        }
        self.offset += d.len() as u64;

        let big_size = size >= MAX32;
        let big_offset = e.offset >= MAX32;
        let mut extra = Vec::new();
        if big_size || big_offset {
            let mut fields = Vec::new();
            if big_size {
                fields.extend(size.to_le_bytes());
                fields.extend(size.to_le_bytes());
            }
            if big_offset {
                fields.extend(e.offset.to_le_bytes());
            }
            extra.extend(1u16.to_le_bytes());
            extra.extend((fields.len() as u16).to_le_bytes());
            extra.extend(fields);
        }
        let c = &mut self.central;
        c.extend(0x0201_4b50u32.to_le_bytes());
        c.extend((0x0300u16 | 45).to_le_bytes()); // made by: Unix, 4.5
        c.extend((if e.zip64 || big_size || big_offset { 45u16 } else { 20u16 }).to_le_bytes());
        c.extend(0x0808u16.to_le_bytes());
        c.extend(0u16.to_le_bytes());
        c.extend(self.time.to_le_bytes());
        c.extend(self.date.to_le_bytes());
        c.extend(crc.to_le_bytes());
        let s32 = if big_size { MAX32 as u32 } else { size as u32 };
        c.extend(s32.to_le_bytes());
        c.extend(s32.to_le_bytes());
        c.extend((e.name.len() as u16).to_le_bytes());
        c.extend((extra.len() as u16).to_le_bytes());
        c.extend(0u16.to_le_bytes()); // comment
        c.extend(0u16.to_le_bytes()); // disk
        c.extend(0u16.to_le_bytes()); // internal attributes
        c.extend((0o100644u32 << 16).to_le_bytes()); // a regular file, rw-r--r--
        c.extend((if big_offset { MAX32 as u32 } else { e.offset as u32 }).to_le_bytes());
        c.extend(e.name.as_bytes());
        c.extend(extra);
        self.count += 1;
        d
    }

    /// A small entry whose bytes are at hand (a playlist, the report): header, data and descriptor.
    pub fn whole(&mut self, name: &str, data: &[u8]) -> Vec<u8> {
        let (mut out, e) = self.begin(name, data.len() as u64);
        out.extend(data);
        let d = self.finish(e, crc32(0, data), data.len() as u64);
        out.extend(d);
        out
    }

    /// The central directory and its end.
    pub fn end(self) -> Vec<u8> {
        let (cd_offset, cd_size, n) = (self.offset, self.central.len() as u64, self.count);
        let mut out = self.central;
        let zip64 = n >= 0xFFFF || cd_offset >= MAX32 || cd_size >= MAX32;
        if zip64 {
            let at = cd_offset + cd_size;
            out.extend(0x0606_4b50u32.to_le_bytes());
            out.extend(44u64.to_le_bytes());
            out.extend((0x0300u16 | 45).to_le_bytes());
            out.extend(45u16.to_le_bytes());
            out.extend(0u32.to_le_bytes());
            out.extend(0u32.to_le_bytes());
            out.extend(n.to_le_bytes());
            out.extend(n.to_le_bytes());
            out.extend(cd_size.to_le_bytes());
            out.extend(cd_offset.to_le_bytes());
            out.extend(0x0706_4b50u32.to_le_bytes());
            out.extend(0u32.to_le_bytes());
            out.extend(at.to_le_bytes());
            out.extend(1u32.to_le_bytes());
        }
        let n16 = if n >= 0xFFFF { 0xFFFF } else { n as u16 };
        out.extend(0x0605_4b50u32.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(n16.to_le_bytes());
        out.extend(n16.to_le_bytes());
        out.extend((cd_size.min(MAX32) as u32).to_le_bytes());
        out.extend((cd_offset.min(MAX32) as u32).to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }
}

/// DOS time and date (ZIP's), from seconds since 1970 (UTC).
pub fn dos_time(unix: u64) -> (u16, u16) {
    let days = (unix / 86_400) as i64;
    let secs = unix % 86_400;
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let year = year.clamp(1980, 2107);
    let time = ((secs / 3600) << 11 | (secs % 3600 / 60) << 5 | (secs % 60) / 2) as u16;
    let date = (((year - 1980) << 9) | (month << 5) | day) as u16;
    (time, date)
}

/// Each entry in order: its name and the bytes it will have (the playlists' and the report's at hand).
enum Part {
    File(String, usize),
    Text(String, Vec<u8>),
}

fn parts(plan: &Plan, layout: Layout) -> Vec<Part> {
    let mut parts: Vec<Part> = layout.lists.into_iter().map(|(n, body)| Part::Text(n, body.into_bytes())).collect();
    if let Some(r) = layout.report {
        parts.push(Part::Text(REPORT.into(), r.into_bytes()));
    }
    parts.extend(layout.files.into_iter().map(|(n, i)| Part::File(n, i)));
    let _ = plan;
    parts
}

/// The ZIP's exact length, before writing it.
fn length(plan: &Plan, parts: &[Part]) -> u64 {
    let mut z = Zip::new(0, 0);
    let mut n = 0u64;
    for p in parts {
        let (name, size) = match p {
            Part::File(name, i) => (name, plan.tracks[*i].size),
            Part::Text(name, b) => (name, b.len() as u64),
        };
        let (h, e) = z.begin(name, size);
        n += h.len() as u64 + size;
        n += z.finish(e, 0, size).len() as u64;
    }
    n + z.end().len() as u64
}

pub type Chunk = std::result::Result<axum::body::Bytes, std::io::Error>;

/// Writes the ZIP into a channel as it goes (one file's chunk at a time); returns its exact length and the receiving
/// end. A track is read for exactly the size measured in the plan: a file that changed meanwhile ends the download with
/// an error rather than a broken bundle. A closed channel (the download was cancelled) stops the reading.
pub fn stream(plan: Plan, layout: Layout, now_unix: u64) -> (u64, tokio::sync::mpsc::Receiver<Chunk>) {
    let parts = parts(&plan, layout);
    let len = length(&plan, &parts);
    let (tx, rx) = tokio::sync::mpsc::channel::<Chunk>(4);
    tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let (time, date) = dos_time(now_unix);
        let mut z = Zip::new(time, date);
        let send = |b: Vec<u8>| {
            let tx = tx.clone();
            async move { tx.send(Ok(axum::body::Bytes::from(b))).await.is_ok() }
        };
        for p in parts {
            match p {
                Part::Text(name, body) => {
                    if !send(z.whole(&name, &body)).await {
                        return;
                    }
                }
                Part::File(name, i) => {
                    let t = &plan.tracks[i];
                    let (h, e) = z.begin(&name, t.size);
                    if !send(h).await {
                        return;
                    }
                    let mut file = match tokio::fs::File::open(plan.root.join(&t.rel)).await {
                        Ok(f) => f,
                        Err(err) => {
                            let _ = tx.send(Err(err)).await;
                            return;
                        }
                    };
                    let (mut crc, mut left) = (0u32, t.size);
                    let mut buf = vec![0u8; 256 * 1024];
                    while left > 0 {
                        let want = left.min(buf.len() as u64) as usize;
                        match file.read(&mut buf[..want]).await {
                            Ok(0) => {
                                let _ = tx.send(Err(std::io::Error::other(format!("{} changed during the export", t.rel)))).await;
                                return;
                            }
                            Ok(n) => {
                                crc = crc32(crc, &buf[..n]);
                                left -= n as u64;
                                if !send(buf[..n].to_vec()).await {
                                    return;
                                }
                            }
                            Err(err) => {
                                let _ = tx.send(Err(err)).await;
                                return;
                            }
                        }
                    }
                    if !send(z.finish(e, crc, t.size)).await {
                        return;
                    }
                }
            }
        }
        let _ = send(z.end()).await;
    });
    (len, rx)
}

/// The single `.m3u` of a playlist exported without tracks.
pub fn single_m3u(plan: &Plan, o: &Options) -> Vec<u8> {
    layout(plan, o).lists.into_iter().next().map(|(_, b)| b.into_bytes()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn music() -> tempfile_dir::Dir {
        tempfile_dir::Dir::new()
    }

    /// A throwaway folder under the system's temp folder.
    mod tempfile_dir {
        pub struct Dir(pub std::path::PathBuf);
        impl Dir {
            pub fn new() -> Self {
                let p = std::env::temp_dir().join(format!("cratarr-export-{}-{}", std::process::id(), rand()));
                std::fs::create_dir_all(&p).unwrap();
                Dir(p)
            }
            pub fn put(&self, rel: &str, bytes: &[u8]) {
                let p = self.0.join(rel);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, bytes).unwrap();
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        fn rand() -> u128 {
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        }
    }

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn tags(rels: &[String]) -> Result<Vec<(Option<String>, Option<String>)>> {
        Ok(rels
            .iter()
            .map(|r| match r.as_str() {
                "A - Alb/01 One.flac" => (Some("A".into()), Some("One".into())),
                "B - Alb/01 One.mp3" => (Some("A".into()), Some("One".into())),
                _ => (None, None),
            })
            .collect())
    }

    fn sample() -> (tempfile_dir::Dir, Plan) {
        let d = music();
        d.put("A - Alb/01 One.flac", b"one-flac");
        d.put("A - Alb/02 Two.flac", b"two");
        d.put("B - Alb/01 One.mp3", b"one-mp3!");
        let plan = plan(
            &d.0,
            vec![
                ("Chill".into(), lines(&["A - Alb/01 One.flac", "Gone/x.mp3", "A - Alb/02 Two.flac"])),
                ("Jazz/Soul".into(), lines(&["A - Alb/02 Two.flac", "B - Alb/01 One.mp3", "../outside.mp3"])),
            ],
            tags,
        )
        .unwrap();
        (d, plan)
    }

    #[test]
    fn a_plan_has_each_file_once_and_keeps_missing_or_outside_lines_apart() {
        let (_d, p) = sample();
        assert_eq!(p.tracks.iter().map(|t| t.rel.as_str()).collect::<Vec<_>>(), ["A - Alb/01 One.flac", "A - Alb/02 Two.flac", "B - Alb/01 One.mp3"]);
        assert_eq!(p.lists, vec![("Chill".to_string(), vec![0, 1]), ("Jazz/Soul".to_string(), vec![1, 2])]);
        assert_eq!(p.missing, vec![("Chill".into(), "Gone/x.mp3".into()), ("Jazz/Soul".into(), "../outside.mp3".into())]);
        assert_eq!(p.tracks[0].size, 8);
    }

    #[test]
    fn without_tracks_the_lines_are_relative_to_the_music_folder_and_one_playlist_is_a_plain_m3u() {
        let (_d, p) = sample();
        let o = Options::default();
        let l = layout(&p, &o);
        assert!(l.files.is_empty() && l.report.is_none());
        assert_eq!(l.lists[0], ("Chill.m3u".into(), "A - Alb/01 One.flac\r\nA - Alb/02 Two.flac\r\n".into()));
        assert_eq!(l.lists[1].0, "Jazz_Soul.m3u", "a name that is not a safe file name");
        assert!(is_zip(&p, &o), "two playlists: a zip");
        let one = Plan { lists: p.lists[..1].to_vec(), ..p.clone() };
        assert!(!is_zip(&one, &o));
        assert_eq!(download_name(&one, &o), "Chill.m3u");
        assert_eq!(download_name(&p, &o), "Playlists.zip");
    }

    #[test]
    fn combining_lists_each_track_once_at_its_first_place() {
        let (_d, p) = sample();
        let o = Options { combine: Some("Both".into()), ..Default::default() };
        let l = layout(&p, &o);
        assert_eq!(l.lists, vec![("Both.m3u".into(), "A - Alb/01 One.flac\r\nA - Alb/02 Two.flac\r\nB - Alb/01 One.mp3\r\n".into())]);
        assert_eq!(download_name(&p, &o), "Both.m3u");
        assert_eq!(preview(&p, &o), Preview { tracks: 3, missing: 2, bytes: None });
    }

    #[test]
    fn in_one_folder_tracks_are_named_artist_title_and_a_clash_gets_a_number() {
        let (_d, p) = sample();
        let o = Options { tracks: true, flat: true, combine: None };
        let l = layout(&p, &o);
        assert_eq!(l.files.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["A - One.flac", "02 Two.flac", "A - One.mp3"]);
        let o = Options { tracks: true, flat: true, combine: None };
        let d = music();
        d.put("x/1.mp3", b"1");
        d.put("y/1.mp3", b"2");
        let same = plan(&d.0, vec![("p".into(), lines(&["x/1.mp3", "y/1.mp3"]))], |r| Ok(r.iter().map(|_| (Some("A".into()), Some("T".into()))).collect())).unwrap();
        let names: Vec<_> = layout(&same, &o).files.into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["A - T.mp3", "A - T (2).mp3"]);
        assert_eq!(layout(&same, &o).lists[0].1, "A - T.mp3\r\nA - T (2).mp3\r\n", "the lines name the bundled files");
    }

    #[test]
    fn the_zip_holds_the_playlists_the_report_and_the_tracks_and_its_length_is_known_before() {
        let (_d, p) = sample();
        let o = Options { tracks: true, flat: false, combine: None };
        let l = layout(&p, &o);
        assert!(l.report.as_deref().unwrap().contains("Chill: Gone/x.mp3"));
        assert_eq!(preview(&p, &o), Preview { tracks: 3, missing: 2, bytes: Some(19) });
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let (len, bytes) = rt.block_on(async {
            let (len, mut rx) = stream(p, l, 1_791_000_000);
            let mut all = Vec::new();
            while let Some(c) = rx.recv().await {
                all.extend_from_slice(&c.unwrap());
            }
            (len, all)
        });
        assert_eq!(len, bytes.len() as u64, "the announced length is the length sent");
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let names: Vec<String> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().to_string()).collect();
        assert_eq!(names, ["Chill.m3u", "Jazz_Soul.m3u", "Missing tracks.txt", "A - Alb/01 One.flac", "A - Alb/02 Two.flac", "B - Alb/01 One.mp3"]);
        let mut s = String::new();
        zip.by_name("B - Alb/01 One.mp3").unwrap().read_to_string(&mut s).unwrap();
        assert_eq!(s, "one-mp3!", "the data is read back with its CRC checked");
    }

    #[test]
    fn sizes_and_offsets_past_4_gb_get_zip64_fields() {
        let mut z = Zip::new(0, 0);
        z.offset = 5_000_000_000;
        let (h, e) = z.begin("big.flac", 6_000_000_000);
        assert_eq!(u16::from_le_bytes([h[28], h[29]]), 20, "a zip64 extra in the local header");
        let d = z.finish(e, 7, 6_000_000_000);
        assert_eq!(d.len(), 24, "64-bit sizes in the descriptor");
        let end = z.end();
        assert_eq!(&end[end.len() - 22..end.len() - 18], &0x0605_4b50u32.to_le_bytes());
        assert!(end.windows(4).any(|w| w == 0x0606_4b50u32.to_le_bytes()), "a zip64 end record");
        assert_eq!(u32::from_le_bytes(end[end.len() - 6..end.len() - 2].try_into().unwrap()), u32::MAX, "the offset says: see zip64");
    }

    #[test]
    fn dos_dates_and_safe_names() {
        // 2026-10-14 12:34:56 UTC.
        let (t, d) = dos_time(1_791_981_296);
        assert_eq!((d >> 9) + 1980, 2026);
        assert_eq!((d >> 5) & 0xF, 10);
        assert_eq!(d & 0x1F, 14);
        assert_eq!((t >> 11, (t >> 5) & 0x3F, (t & 0x1F) * 2), (12, 34, 56));
        assert_eq!(safe_name("AC/DC: Live?"), "AC_DC_ Live_");
        assert_eq!(safe_name(" ... "), "Untitled");
        assert_eq!(crc32(0, b"123456789"), 0xCBF4_3926);
    }
}
