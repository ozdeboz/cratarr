//! The fixture library (decision 0093): a dozen small albums of generated audio, shaped like the library's hard cases,
//! made the same way every time (`make`; `cratarr-server --make-fixtures <folder>`), for the end-to-end suite and the
//! development instance. No audio is kept in git. The artists and albums are made up.
//!
//! The cases: full MusicBrainz-style tags with fields lofty has no key for (STYLE, RELEASESTATUS,
//! MUSICBRAINZ_ALBUMTYPE, ORIGINALYEAR beside ORIGINALDATE) and a cover beside; two discs with the cover inside; a
//! soundtrack of various artists as ID3v2.4 MP3s; a classical work with movements; a record missing tracks; no year; an
//! EP by its name as ID3v2.3 MP3s with an ID3v1 tag; a single; an LP by its name; a compilation with no type; a junk
//! date; FLAC and MP3 in one album.
//!
//! The whole layout (`make_layout`; `cratarr-server --make-layout <folder>`): those albums as the first main library, and
//! a second one, an Incoming and two accounts' own music beside them, holding the cases of "the same record"
//! (docs/records-and-copies.md, `layout_albums`). `make` stays as it is: the suite and the development instance count on
//! its twelve albums in one folder.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lofty::config::{ParseOptions, WriteOptions};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::*;

/// How long a track is (s), unless its album says.
const SECONDS: u32 = 3;
const RATE: u32 = 44_100;

#[derive(Clone, Copy, PartialEq)]
enum Form {
    Flac,
    /// MP3 with an ID3v2.4 tag.
    Mp3,
    /// MP3 with an ID3v2.3 tag and an ID3v1 tag after the audio.
    Mp3Old,
}

#[derive(Clone, Copy, PartialEq)]
enum Cover {
    None,
    Beside,
    Inside,
}

struct Album {
    artist: &'static str,
    title: &'static str,
    /// The folder's year; none: "(no year)".
    year: Option<&'static str>,
    form: Form,
    cover: Cover,
    /// (disc, number, title, artist where it is not the album's).
    tracks: Vec<(u32, u32, &'static str, Option<&'static str>)>,
    /// How many tracks the release has, where more than are here.
    of: Option<u32>,
    /// Fields every track has, as (key, value), several values of a key in turn.
    fields: Vec<(&'static str, &'static str)>,
    /// A form per track where they differ (`Form` by track number).
    mixed: bool,
    /// Each track's length (s), in order, where it is not `SECONDS`: two copies are one record within two seconds a track.
    lengths: &'static [u32],
}

fn albums() -> Vec<Album> {
    let t = |n: u32, title: &'static str| (1, n, title, None);
    vec![
        Album {
            artist: "Glass Harbour",
            title: "Tide Tables",
            year: Some("2012"),
            form: Form::Flac,
            cover: Cover::Beside,
            tracks: vec![t(1, "Slack Water"), t(2, "Neap"), t(3, "Spring Tide"), t(4, "Ebb"), t(5, "Flood"), t(6, "Low Water")],
            of: None,
            fields: vec![
                ("DATE", "2012-03-05"),
                ("ORIGINALDATE", "2012-03-05"),
                ("ORIGINALYEAR", "2012"),
                ("GENRE", "Electronic"),
                ("STYLE", "Dub Techno"),
                ("STYLE", "Ambient"),
                ("RELEASETYPE", "album"),
                ("MUSICBRAINZ_ALBUMTYPE", "album"),
                ("RELEASESTATUS", "official"),
                ("MUSICBRAINZ_ALBUMSTATUS", "official"),
                ("MUSICBRAINZ_ALBUMID", "00000000-0000-4000-8000-000000000001"),
                ("MUSICBRAINZ_RELEASEGROUPID", "00000000-0000-4000-8000-000000000101"),
                ("LABEL", "Fixture Records"),
            ],
            mixed: false,
            lengths: &[],
        },
        Album {
            artist: "Noor Field",
            title: "Two Rooms",
            year: Some("2019"),
            form: Form::Flac,
            cover: Cover::Inside,
            tracks: vec![(1, 1, "Hall", None), (1, 2, "Stair", None), (1, 3, "Door", None), (2, 1, "Window", None), (2, 2, "Floor", None), (2, 3, "Roof", None)],
            of: None,
            fields: vec![("DATE", "2019"), ("GENRE", "Ambient"), ("TOTALDISCS", "2")],
            mixed: false,
            lengths: &[],
        },
        Album {
            artist: "Various Artists",
            title: "Late Train OST",
            year: Some("2005"),
            form: Form::Mp3,
            cover: Cover::Inside,
            tracks: vec![
                (1, 1, "Platform", Some("Kite Theory")),
                (1, 2, "Night Carriage", Some("Tape Club")),
                (1, 3, "Signal Box", Some("Orbit Lane")),
                (1, 4, "Last Stop", Some("Noor Field")),
                (1, 5, "Terminus", Some("Glass Harbour")),
            ],
            of: None,
            fields: vec![("DATE", "2005"), ("GENRE", "Soundtrack"), ("RELEASETYPE", "album"), ("RELEASETYPE", "soundtrack"), ("RELEASETYPE", "compilation")],
            mixed: false,
            lengths: &[],
        },
        Album {
            artist: "Lowstring Collective",
            title: "Cello Suite No. 1",
            year: Some("1995"),
            form: Form::Flac,
            cover: Cover::Beside,
            tracks: vec![t(1, "I. Prélude"), t(2, "II. Allemande"), t(3, "III. Courante"), t(4, "IV. Sarabande"), t(5, "V. Menuet I & II"), t(6, "VI. Gigue")],
            of: None,
            fields: vec![
                ("DATE", "1995"),
                ("GENRE", "Baroque"),
                ("COMPOSER", "Johann Sebastian Bach"),
                ("WORK", "Cello Suite No. 1 in G major, BWV 1007"),
                ("PERFORMER", "Lowstring Collective"),
            ],
            mixed: false,
            lengths: &[],
        },
        Album {
            artist: "Glass Harbour",
            title: "Partial Shore",
            year: Some("2011"),
            form: Form::Flac,
            cover: Cover::None,
            tracks: vec![t(1, "Sandbar"), t(2, "Groyne"), t(5, "Breakwater")],
            of: Some(7),
            fields: vec![("DATE", "2011"), ("GENRE", "Electronic")],
            mixed: false,
            lengths: &[],
        },
        Album {
            artist: "Kite Theory",
            title: "Undated",
            year: None,
            form: Form::Flac,
            cover: Cover::None,
            tracks: vec![t(1, "Morning"), t(2, "Noon"), t(3, "Evening"), t(4, "Night")],
            of: None,
            fields: vec![("GENRE", "Downtempo")],
            mixed: false,
            lengths: &[],
        },
        Album {
            artist: "Kite Theory",
            title: "Night Walks EP",
            year: Some("2020"),
            form: Form::Mp3Old,
            cover: Cover::None,
            tracks: vec![t(1, "Lamplight"), t(2, "Crossing"), t(3, "Home")],
            of: None,
            fields: vec![("DATE", "2020"), ("GENRE", "House")],
            mixed: false,
            lengths: &[],
        },
        Album {
            artist: "Noor Field",
            title: "Signal",
            year: Some("2021"),
            form: Form::Flac,
            cover: Cover::None,
            tracks: vec![t(1, "Signal")],
            of: None,
            fields: vec![("DATE", "2021"), ("GENRE", "Techno")],
            mixed: false,
            lengths: &[],
        },
        Album {
            artist: "Tape Club",
            title: "Long Player LP",
            year: Some("2015"),
            form: Form::Flac,
            cover: Cover::None,
            tracks: vec![t(1, "Side A"), t(2, "Side B"), t(3, "Run Out")],
            of: None,
            fields: vec![("DATE", "2015"), ("GENRE", "Funk")],
            mixed: false,
            lengths: &[],
        },
        Album {
            artist: "Various Artists",
            title: "Club Classics",
            year: Some("2001"),
            form: Form::Flac,
            cover: Cover::Beside,
            tracks: (1..=8).map(|n| (1, n, ["One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight"][n as usize - 1], Some(["Kite Theory", "Tape Club", "Orbit Lane"][n as usize % 3]))).collect(),
            of: None,
            fields: vec![("DATE", "2001"), ("GENRE", "House")],
            mixed: false,
            lengths: &[],
        },
        Album {
            artist: "Orbit Lane",
            title: "Junk Dates",
            year: Some("1"),
            form: Form::Flac,
            cover: Cover::None,
            tracks: vec![t(1, "Zero"), t(2, "One")],
            of: None,
            fields: vec![("DATE", "0001-01-01"), ("ORIGINALDATE", "0001-01-01"), ("ORIGINALYEAR", "1"), ("GENRE", "Electronic")],
            mixed: false,
            lengths: &[],
        },
        Album {
            artist: "Orbit Lane",
            title: "Mixed Formats",
            year: Some("2016"),
            form: Form::Flac,
            cover: Cover::Beside,
            tracks: vec![t(1, "Lossless"), t(2, "Lossy"), t(3, "Lossless Again"), t(4, "Lossy Again")],
            of: None,
            fields: vec![("DATE", "2016"), ("GENRE", "Electronic")],
            mixed: true,
            lengths: &[],
        },
    ]
}

/// Where an album of the layout lies.
#[derive(Clone, Copy, PartialEq)]
enum Place {
    Main,
    Second,
    Incoming,
    /// An account's own music.
    Own(&'static str),
}

/// An album by the artist of the layout's cases, no cover, every track its number in the list.
fn case(title: &'static str, form: Form, tracks: &[&'static str], lengths: &'static [u32], release: Option<&'static str>) -> Album {
    Album {
        artist: "Salt Meridian",
        title,
        year: Some("2018"),
        form,
        cover: Cover::None,
        tracks: tracks.iter().zip(1..).map(|(title, n)| (1, n, *title, None)).collect(),
        of: None,
        fields: [("DATE", "2018"), ("GENRE", "Electronic")].into_iter().chain(release.map(|r| ("MUSICBRAINZ_ALBUMID", r))).collect(),
        mixed: false,
        lengths,
    }
}

/// The albums the layout adds to `make`'s, each case of "the same record" spelt out by what must come of it.
fn layout_albums() -> Vec<(Place, Album)> {
    use Form::{Flac, Mp3};
    use Place::{Incoming, Main, Own, Second};
    const SHORES: Option<&str> = Some("00000000-0000-4000-8000-000000000201");
    const MEASURE: Option<&str> = Some("00000000-0000-4000-8000-000000000204");
    let shores = ["Landing", "Crossing", "Return"];
    let bonus = ["Opening", "Middle", "Closing"];
    let pressings = ["Side One", "Side Two"];
    let measure = ["Bar One", "Bar Two", "Bar Three", "Bar Four"];
    let papers = ["Unsigned", "Undated"];
    let cut = ["Short Take", "Long Take"];
    vec![
        // One record in every kind of library, FLAC and MP3, one release: a track a second longer is still that track.
        (Main, case("Both Shores", Flac, &shores, &[2, 3, 2], SHORES)),
        (Second, case("Both Shores", Flac, &shores, &[3, 3, 2], SHORES)),
        (Incoming, case("Both Shores", Mp3, &shores, &[2, 3, 2], SHORES)),
        (Own("ann"), case("Both Shores", Mp3, &shores, &[2, 3, 2], SHORES)),
        // A deluxe edition: a bonus track makes another record, whatever the name says.
        (Main, case("Bonus Round", Flac, &bonus, &[2, 2, 2], None)),
        (Second, case("Bonus Round", Flac, &["Opening", "Middle", "Closing", "Encore"], &[2, 2, 2, 2], None)),
        // The same tracks on two MusicBrainz releases: two records.
        (Main, case("Two Pressings", Flac, &pressings, &[2, 2], Some("00000000-0000-4000-8000-000000000202"))),
        (Second, case("Two Pressings", Flac, &pressings, &[2, 2], Some("00000000-0000-4000-8000-000000000203"))),
        // A folder short of a track of its very release is a copy of it, as one still arriving is (the owner, 2026-11-07).
        (Main, case("Short Measure", Flac, &measure, &[2, 2, 2, 2], MEASURE)),
        (Incoming, case("Short Measure", Mp3, &measure[..3], &[2, 2, 2], MEASURE)),
        // No release on either side: the album artist and title, and the tracks, make it one record.
        (Main, case("No Papers", Flac, &papers, &[3, 2], None)),
        (Own("bob"), case("No Papers", Mp3, &papers, &[2, 2], None)),
        // A track more than two seconds apart: two records, all else the same.
        (Main, case("Long Cut", Flac, &cut, &[2, 2], None)),
        (Second, case("Long Cut", Flac, &cut, &[2, 5], None)),
        // Each library's own, with no copy anywhere else.
        (Second, case("Far Bank", Flac, &["Far Bank"], &[2], None)),
        (Incoming, case("Just Arrived", Mp3, &["Just Arrived"], &[2], None)),
        (Own("ann"), case("Own Room", Mp3, &["Own Room"], &[2], None)),
        (Own("bob"), case("Other Room", Mp3, &["Other Room"], &[2], None)),
    ]
}

/// A tone (`seconds` long, its pitch by `n`) as a 16-bit stereo WAV.
fn wav(path: &Path, n: u32, seconds: u32) -> Result<()> {
    let frames = (RATE * seconds) as usize;
    let hz = 220.0 * (1.0 + f64::from(n) * 0.125);
    let mut data = Vec::with_capacity(frames * 4);
    for i in 0..frames {
        let s = ((i as f64 * hz * std::f64::consts::TAU / f64::from(RATE)).sin() * 9000.0) as i16;
        data.extend_from_slice(&s.to_le_bytes());
        data.extend_from_slice(&s.to_le_bytes());
    }
    let mut b = Vec::with_capacity(44 + data.len());
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(data.len() as u32).to_le_bytes());
    b.extend_from_slice(&data);
    std::fs::write(path, b)?;
    Ok(())
}

/// A cover: a square of one colour (by `n`) as a JPEG.
fn cover(n: u32) -> Result<Vec<u8>> {
    let c = [(n * 53 % 200 + 40) as u8, (n * 97 % 200 + 40) as u8, (n * 31 % 200 + 40) as u8];
    let img = image::RgbImage::from_pixel(300, 300, image::Rgb(c));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Jpeg)?;
    Ok(out.into_inner())
}

/// Makes the fixture library in `dir` (made anew: what was there goes). Returns the album folders.
pub fn make(dir: &Path) -> Result<Vec<PathBuf>> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).with_context(|| format!("clearing {}", dir.display()))?;
    }
    std::fs::create_dir_all(dir)?;
    let mut made = Vec::new();
    for (a_n, a) in albums().into_iter().enumerate() {
        made.push(write_album(dir, a_n as u32, &a)?);
    }
    Ok(made)
}

/// The folders of the whole layout under one folder, for what makes it and what reads it.
pub struct Layout {
    /// The first main library: `make`'s albums, and the cases' copies there.
    pub main: PathBuf,
    /// A second main library, with albums of its own.
    pub second: PathBuf,
    pub incoming: PathBuf,
    /// The own-music folder, a folder per account in it: `ann` and `bob`.
    pub own: PathBuf,
}

impl Layout {
    pub fn of(dir: &Path) -> Self {
        Layout { main: dir.join("music"), second: dir.join("second"), incoming: dir.join("incoming"), own: dir.join("own") }
    }

    fn folder(&self, place: Place) -> PathBuf {
        match place {
            Place::Main => self.main.clone(),
            Place::Second => self.second.clone(),
            Place::Incoming => self.incoming.clone(),
            Place::Own(account) => self.own.join(account),
        }
    }
}

/// Makes the whole layout in `dir` (made anew: what was there goes). Returns the album folders.
pub fn make_layout(dir: &Path) -> Result<Vec<PathBuf>> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).with_context(|| format!("clearing {}", dir.display()))?;
    }
    let layout = Layout::of(dir);
    let mut made = make(&layout.main)?;
    let first = made.len() as u32;
    // Each with a tone of its own, so two copies of a record differ byte for byte: which one played can be told.
    for (a_n, (place, a)) in (first..).zip(layout_albums()) {
        made.push(write_album(&layout.folder(place), a_n, &a)?);
    }
    Ok(made)
}

/// Writes an album under `root`, `n` choosing its tones and cover. Its folder.
fn write_album(root: &Path, n: u32, a: &Album) -> Result<PathBuf> {
    let folder = root.join(a.artist).join(format!("{} ({})", a.title, a.year.unwrap_or("no year")));
    std::fs::create_dir_all(&folder)?;
    let picture = cover(n)?;
    if a.cover == Cover::Beside {
        std::fs::write(folder.join("cover.jpg"), &picture)?;
    }
    let discs = a.tracks.iter().map(|t| t.0).max().unwrap_or(1);
    for (i, &(disc, number, title, artist)) in a.tracks.iter().enumerate() {
        let form = if a.mixed && number % 2 == 0 { Form::Mp3 } else { a.form };
        let stem = if discs > 1 { format!("{disc}-{number:02} {title}") } else { format!("{number:02} {title}") };
        let tone = folder.join(format!(".{stem}.wav"));
        wav(&tone, n * 16 + i as u32, a.lengths.get(i).copied().unwrap_or(SECONDS))?;
        let flac = folder.join(format!("{stem}.flac"));
        crate::convert::to_flac_told(&tone, &flac, crate::convert::CD, |_| {})?;
        std::fs::remove_file(&tone)?;
        // The tags, each as the field is spelt in a FLAC.
        let mut file = lofty::flac::FlacFile::read_from(&mut std::fs::File::open(&flac)?, ParseOptions::new())?;
        let mut vc = file.remove_vorbis_comments().unwrap_or_default();
        vc.push("TITLE".into(), title.into());
        vc.push("ARTIST".into(), artist.unwrap_or(a.artist).into());
        vc.push("ALBUM".into(), a.title.into());
        vc.push("ALBUMARTIST".into(), a.artist.into());
        vc.push("TRACKNUMBER".into(), number.to_string());
        vc.push("TRACKTOTAL".into(), a.of.unwrap_or(a.tracks.iter().filter(|t| t.0 == disc).count() as u32).to_string());
        vc.push("DISCNUMBER".into(), disc.to_string());
        for (k, v) in &a.fields {
            vc.push((*k).into(), (*v).into());
        }
        if a.cover == Cover::Inside {
            let pic = Picture::unchecked(picture.clone()).pic_type(PictureType::CoverFront).mime_type(MimeType::Jpeg).build();
            lofty::ogg::OggPictureStorage::insert_picture(&mut vc, pic, None)?;
        }
        file.set_vorbis_comments(vc);
        file.save_to_path(&flac, WriteOptions::default())?;
        if form != Form::Flac {
            let mp3 = folder.join(format!("{stem}.mp3"));
            crate::stream_forms::mp3_file(&flac, &mp3, crate::stream_forms::Mp3Kind::Cbr320)?;
            std::fs::remove_file(&flac)?;
            if form == Form::Mp3Old {
                let mut file = lofty::mpeg::MpegFile::read_from(&mut std::fs::File::open(&mp3)?, ParseOptions::new())?;
                let mut v1 = lofty::id3::v1::Id3v1Tag::default();
                v1.set_title(title.into());
                v1.set_artist(artist.unwrap_or(a.artist).into());
                v1.set_album(a.title.into());
                file.set_id3v1(v1);
                file.save_to_path(&mp3, WriteOptions::default().use_id3v23(true))?;
            }
        }
    }
    Ok(folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "slow (encodes every track): the Dockerfile's e2e stage"]
    fn the_fixture_library_is_made_with_its_hard_cases() {
        let d = std::env::temp_dir().join(format!("cratarr-fixtures-{}", std::process::id()));
        let albums = make(&d).unwrap();
        assert_eq!(albums.len(), 12);
        let files: Vec<PathBuf> = albums
            .iter()
            .flat_map(|a| std::fs::read_dir(a).unwrap().map(|e| e.unwrap().path()))
            .filter(|p| p.extension().is_some_and(|x| x == "flac" || x == "mp3"))
            .collect();
        assert_eq!(files.len(), 6 + 6 + 5 + 6 + 3 + 4 + 3 + 1 + 3 + 8 + 2 + 4);
        let named = |p: &Path, name: &str| -> Vec<String> {
            crate::catalogue::tags::read(p).unwrap().0.items.into_iter().filter(|(_, n, _)| *n == Some(name)).map(|(_, _, v)| v).collect()
        };
        let tide = d.join("Glass Harbour/Tide Tables (2012)/01 Slack Water.flac");
        assert_eq!(named(&tide, "style"), ["Dub Techno", "Ambient"], "a field lofty has no key for, read");
        assert_eq!(named(&tide, "releasetype"), ["album"]);
        assert!(d.join("Glass Harbour/Tide Tables (2012)/cover.jpg").exists());
        let ost = d.join("Various Artists/Late Train OST (2005)/03 Signal Box.mp3");
        assert_eq!(named(&ost, "artist"), ["Orbit Lane"]);
        assert_eq!(named(&ost, "albumartist"), ["Various Artists"]);
        assert_eq!(named(&ost, "releasetype"), ["album", "soundtrack", "compilation"]);
        assert!(named(&d.join("Kite Theory/Undated (no year)/01 Morning.flac"), "date").is_empty());
        assert_eq!(named(&d.join("Noor Field/Two Rooms (2019)/2-03 Roof.flac"), "discnumber"), ["2"]);
        let ep = d.join("Kite Theory/Night Walks EP (2020)/01 Lamplight.mp3");
        let bytes = std::fs::read(&ep).unwrap();
        assert_eq!(&bytes[..4], b"ID3\x03", "ID3v2.3");
        assert_eq!(&bytes[bytes.len() - 128..bytes.len() - 125], b"TAG", "an ID3v1 tag after the audio");
        assert!(d.join("Orbit Lane/Mixed Formats (2016)/02 Lossy.mp3").exists() && d.join("Orbit Lane/Mixed Formats (2016)/01 Lossless.flac").exists());
        // Made the same way again.
        let again = make(&d).unwrap();
        assert_eq!(again, albums);
        assert_eq!(std::fs::read(&tide).unwrap().len(), std::fs::read(&tide).unwrap().len());
        std::fs::remove_dir_all(&d).ok();
    }

    /// "The same record" on real files, read and grouped as the library does: what each case of the layout is there for.
    #[test]
    #[ignore = "slow (encodes every track): the Dockerfile's e2e stage"]
    fn the_whole_layout_holds_each_case_of_the_same_record() {
        use crate::catalogue::{Catalogue, Rules};
        use crate::serve::library::{FolderRows, album_copies};
        let d = std::env::temp_dir().join(format!("cratarr-layout-{}", std::process::id()));
        let albums = make_layout(&d).unwrap();
        assert_eq!(albums.len(), 12 + layout_albums().len());
        let layout = Layout::of(&d);
        assert_eq!(std::fs::read_dir(layout.own.join("ann").join("Salt Meridian")).unwrap().count(), 2, "an account's own music");
        // Two copies of one track are told apart by their bytes, whatever the record says.
        let landing = "Salt Meridian/Both Shores (2018)/01 Landing.flac";
        assert_ne!(std::fs::read(layout.main.join(landing)).unwrap(), std::fs::read(layout.second.join(landing)).unwrap());

        let cat = Catalogue::open(None, vec![layout.main.clone(), layout.second.clone(), layout.incoming.clone(), layout.own.join("ann"), layout.own.join("bob")]).unwrap();
        cat.walk_all();
        while cat.read_tags_batch(100).unwrap() > 0 {}
        cat.identify().unwrap();
        let read = || -> Vec<FolderRows> {
            cat.folders().iter().map(|(folder, root)| FolderRows { folder, root: root.display().to_string(), rows: cat.served_rows(folder).unwrap() }).collect()
        };
        cat.assign_records(&album_copies(&read(), &Rules::default())).unwrap();
        let folders = read();
        let records = |album: &str| -> (usize, std::collections::BTreeSet<i64>) {
            let of: Vec<&FolderRows> = folders.iter().filter(|f| f.rows.iter().any(|r| r.first("album") == Some(album))).collect();
            let records = of.iter().flat_map(|f| &f.rows).filter(|r| r.first("album") == Some(album)).map(|r| r.record.expect("a record")).collect();
            (of.len(), records)
        };
        let (libraries, one) = records("Both Shores");
        assert_eq!((libraries, one.len()), (4, 1), "one record in four libraries, a second apart, FLAC and MP3");
        let (libraries, one) = records("No Papers");
        assert_eq!((libraries, one.len()), (2, 1), "no release on either side: artist, title and tracks");
        let (libraries, one) = records("Short Measure");
        assert_eq!((libraries, one.len()), (2, 1), "a folder short of a track of its very release");
        for two in ["Bonus Round", "Two Pressings", "Long Cut"] {
            assert_eq!(records(two).1.len(), 2, "{two}: not one record");
        }
        let long_take = |root: &Path| folders.iter().find(|f| f.root == root.display().to_string()).unwrap().rows.iter().find(|r| r.rel.ends_with("Long Take.flac")).unwrap().duration_ms.unwrap();
        assert!(long_take(&layout.second) - long_take(&layout.main) > 2_000, "more than the rule's two seconds apart");
        std::fs::remove_dir_all(&d).ok();
    }
}
