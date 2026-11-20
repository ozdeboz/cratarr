//! The gate cases (SK6, SCENARIOS-SK17-GATE "Shapes"): Glass Harbour's two made-up records as Soulseek's users might share them,
//! each track a few seconds of generated sound that the sourcing gate hears as its case says. Made the same way every time
//! (`gate_cases`; `cratarr-server --make-gate-cases <folder>`), as decision 0093 makes the fixture library: no audio in git.
//!
//! - `clean`: full band, which the gate passes;
//! - `walled`: the same music stopping at 16 kHz, a lossy transcode posing as FLAC ("Not lossless: it stops at 16 kHz");
//! - `notes`: Tide Tables with a clean first track, a clipped second and a third with a click and a pop at the very end.
//!
//! Each file is FLAC, 16-bit 44.1 kHz stereo (`convert::to_cd_flac`, as the gate's own tests make theirs), its length exact to
//! the sample, and tagged as the fixture library's are. The sounds are the gate's unit tests' recipes (`track_fetch`'s tests:
//! `noise`, `sweep`, `tones`, and `analysis::tests::wav`'s samples), here for the generator; `the_gate_cases_are_heard_as_named`
//! pins what each file is heard as, so the cases cannot drift under the end-to-end suite.

use std::f64::consts::TAU;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lofty::config::{ParseOptions, WriteOptions};
use lofty::prelude::*;

/// A record of the cases: its names and its tracks, each (title, seconds).
#[derive(Debug, Clone, Copy)]
pub struct GateRecord {
    pub artist: &'static str,
    pub album: &'static str,
    pub year: u32,
    pub tracks: [(&'static str, u32); 3],
}

impl GateRecord {
    /// The folder its user shares it as, and the case's folder on disk: "Glass Harbour - Tide Tables (2012)".
    pub fn folder(&self) -> String {
        format!("{} - {} ({})", self.artist, self.album, self.year)
    }

    /// Track `n`'s file name (from 1): "01 Slack Water.flac".
    pub fn file(&self, n: usize) -> String {
        format!("{n:02} {}.flac", self.tracks[n - 1].0)
    }
}

/// Glass Harbour, "Tide Tables" (2012): Slack Water 5 s, Neap 4 s, Spring Tide 7 s.
pub const TIDE_TABLES: GateRecord = GateRecord { artist: "Glass Harbour", album: "Tide Tables", year: 2012, tracks: [("Slack Water", 5), ("Neap", 4), ("Spring Tide", 7)] };
/// Glass Harbour, "Harbour Lights" (2014): Pier 4 s, Breakwater 5 s, Lantern 6 s.
pub const HARBOUR_LIGHTS: GateRecord = GateRecord { artist: "Glass Harbour", album: "Harbour Lights", year: 2014, tracks: [("Pier", 4), ("Breakwater", 5), ("Lantern", 6)] };

/// The case folders: a case's folder holds its records' folders.
pub const CLEAN: &str = "clean";
pub const WALLED: &str = "walled";
pub const NOTES: &str = "notes";

/// What a track sounds like.
#[derive(Debug, Clone, Copy)]
enum Sound {
    /// Full band: a sweep from 100 Hz to 21 kHz and noise.
    Clean,
    /// The same music stopping at 16 kHz: the sweep to 16 kHz and tones every 100 Hz below it.
    Walled,
    /// A tone driven past full scale from 1.5 s to 2.5 s, cut flat there.
    Clipped,
    /// A tone, silence with one sample's spike in it at 1.5 s, the tone again, near-silence, and a burst in the last 50 ms.
    ClickAndPop,
}

/// Every case: its folder, its record, and each track's sound in turn.
const CASES: [(&str, GateRecord, [Sound; 3]); 5] = [
    (CLEAN, TIDE_TABLES, [Sound::Clean; 3]),
    (WALLED, TIDE_TABLES, [Sound::Walled; 3]),
    (NOTES, TIDE_TABLES, [Sound::Clean, Sound::Clipped, Sound::ClickAndPop]),
    (CLEAN, HARBOUR_LIGHTS, [Sound::Clean; 3]),
    (WALLED, HARBOUR_LIGHTS, [Sound::Walled; 3]),
];

const RATE: u32 = 44_100;

// ---- The recipes: the gate's unit tests' own (`track_fetch`'s tests)

/// Noise, -1..1, the same for the same sample (a hash of its index, so a recipe's `Fn` can make it).
pub(crate) fn noise(t: f64) -> f64 {
    let mut z = ((t * 44_100.0).round() as u64).wrapping_add(1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
}

/// A sine sweeping from 100 Hz up to `top` over `secs`.
pub(crate) fn sweep(t: f64, secs: f64, top: f64) -> f64 {
    (TAU * (100.0 * t + (top - 100.0) * t * t / (2.0 * secs))).sin()
}

/// Tones every 100 Hz from 100 Hz to `top`, each at `level(hz)` and in a phase of its own: noise of that band.
pub(crate) fn tones(top: f64, level: impl Fn(f64) -> f64) -> impl Fn(f64) -> f64 {
    let tones: Vec<(f64, f64, f64)> =
        (1u32..).map(|i| f64::from(i) * 100.0).take_while(|&hz| hz <= top).map(|hz| (hz, level(hz), std::f64::consts::PI * noise(hz / 44_100.0))).collect();
    move |t: f64| tones.iter().map(|&(hz, a, phase)| a * (TAU * hz * t + phase).sin()).sum::<f64>()
}

/// Whether `t` is the sample at `when` (s).
fn at(t: f64, when: f64) -> bool {
    (t * 44_100.0).round() == (when * 44_100.0).round()
}

/// A sound's samples over `secs`, -1..1 before it is cut at full scale.
fn recipe(sound: Sound, secs: f64) -> Box<dyn Fn(f64) -> f64> {
    match sound {
        // `a_full_band_file_passes_and_the_same_walled_at_16_khz_in_a_flac_is_not_lossless`'s passing half.
        Sound::Clean => Box::new(move |t: f64| 0.3 * sweep(t, secs, 21_000.0) + 0.05 * noise(t)),
        // Its other half.
        Sound::Walled => {
            let band = tones(15_900.0, |_| 0.004);
            Box::new(move |t: f64| 0.3 * sweep(t, secs, 16_000.0) + band(t))
        }
        // `a_clipped_stretch_is_refused_with_when_it_starts`.
        Sound::Clipped => Box::new(|t: f64| {
            let gain = if (1.5..2.5).contains(&t) { 1.6 } else { 0.5 };
            gain * (TAU * 220.0 * t).sin()
        }),
        // `a_click_out_of_silence_is_refused_with_its_time_but_not_in_the_first_second`'s click, then
        // `a_burst_in_the_last_moment_after_near_silence_is_a_pop_at_the_very_end`'s ending, in one file.
        Sound::ClickAndPop => Box::new(move |t: f64| {
            let tone = 0.3 * (TAU * 440.0 * t).sin();
            if at(t, 1.5) {
                0.8
            } else if t < 1.0 || (2.5..4.0).contains(&t) {
                tone
            } else if t < 2.5 {
                0.0
            } else if t >= secs - 0.05 {
                0.9 * noise(t)
            } else {
                0.000_04 * noise(t)
            }
        }),
    }
}

/// `f` over `secs` as a 16-bit stereo WAV at `path`, as the gate's tests write theirs (`analysis::tests::wav`): the same sample in
/// both channels, cut at full scale.
fn wav(path: &Path, secs: u32, f: &dyn Fn(f64) -> f64) -> Result<()> {
    let frames = (RATE * secs) as usize;
    let mut data = Vec::with_capacity(frames * 4);
    for i in 0..frames {
        let s = (f(i as f64 / f64::from(RATE)).clamp(-1.0, 1.0) * 32767.0) as i16;
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

/// Track `n` (from 1) of `r` in `folder`, sounding as `sound`: FLAC, tagged as the fixture library's tracks are.
fn write_track(folder: &Path, r: &GateRecord, n: usize, sound: Sound) -> Result<PathBuf> {
    let (title, secs) = r.tracks[n - 1];
    let flac = folder.join(r.file(n));
    let tone = folder.join(format!(".{n:02} {title}.wav"));
    wav(&tone, secs, &*recipe(sound, f64::from(secs)))?;
    crate::convert::to_cd_flac(&tone, &flac).with_context(|| format!("encoding {}", flac.display()))?;
    std::fs::remove_file(&tone)?;
    let mut file = lofty::flac::FlacFile::read_from(&mut std::fs::File::open(&flac)?, ParseOptions::new())?;
    let mut vc = file.remove_vorbis_comments().unwrap_or_default();
    vc.push("TITLE".into(), title.into());
    vc.push("ARTIST".into(), r.artist.into());
    vc.push("ALBUM".into(), r.album.into());
    vc.push("ALBUMARTIST".into(), r.artist.into());
    vc.push("TRACKNUMBER".into(), n.to_string());
    vc.push("TRACKTOTAL".into(), r.tracks.len().to_string());
    vc.push("DISCNUMBER".into(), "1".into());
    vc.push("DATE".into(), r.year.to_string());
    file.set_vorbis_comments(vc);
    file.save_to_path(&flac, WriteOptions::default())?;
    Ok(flac)
}

/// Makes the gate cases in `dir` (made anew: what was there goes): `<case>/<artist> - <album> (<year>)/<nn> <title>.flac` for each
/// case. Returns the case folders.
pub fn gate_cases(dir: &Path) -> Result<Vec<PathBuf>> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).with_context(|| format!("clearing {}", dir.display()))?;
    }
    let mut made = Vec::new();
    for (case, r, sounds) in CASES {
        let folder = dir.join(case).join(r.folder());
        std::fs::create_dir_all(&folder)?;
        for (n, sound) in (1..).zip(sounds) {
            write_track(&folder, &r, n, sound)?;
        }
        made.push(folder);
    }
    Ok(made)
}

/// The gate cases made once a run, for the tests that hear them (SK6's unit twins, the pin below): where they are.
#[cfg(test)]
pub fn gate_cases_once() -> &'static Path {
    static MADE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    MADE.get_or_init(|| {
        let d = std::env::temp_dir().join(format!("cratarr-gate-cases-{}", std::process::id()));
        gate_cases(&d).unwrap();
        d
    })
}

/// slskd sharing folders, the end-to-end suite's fake (`tests/sk6_harness`), compiled here too so its answers are read by the real
/// client (`the_sharing_slskd_answers_the_client_as_slskd_does`).
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../../tests/sk6_harness/slskd_sharing.rs"]
mod slskd_sharing;

#[cfg(test)]
mod tests {
    use super::*;

    /// Each generated file through today's gate analysis (`acoustid::print`, its `Findings::why()`): heard as its case says
    /// (SCENARIOS-SK17-GATE's table), whole, of its length to the sample, sold as lossless; each folder its three tracks alone,
    /// tagged.
    #[test]
    #[ignore = "slow (encodes and hears every case): the Dockerfile's e2e stage"]
    fn the_gate_cases_are_heard_as_named() {
        let d = gate_cases_once();
        let walled = Some("Not lossless: it stops at 16 kHz");
        let heard_as: [(&str, GateRecord, [Option<&str>; 3]); 5] = [
            (CLEAN, TIDE_TABLES, [None, None, None]),
            (WALLED, TIDE_TABLES, [walled; 3]),
            (NOTES, TIDE_TABLES, [None, Some("Clipped at 0:01"), Some("A click at 0:01; a pop at the very end")]),
            (CLEAN, HARBOUR_LIGHTS, [None, None, None]),
            (WALLED, HARBOUR_LIGHTS, [walled; 3]),
        ];
        for (case, r, whys) in heard_as {
            let folder = d.join(case).join(r.folder());
            let mut names: Vec<String> = std::fs::read_dir(&folder).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            assert_eq!(names, [r.file(1), r.file(2), r.file(3)], "{case}/{}: its three tracks and nothing else", r.folder());
            for (n, why) in (1..).zip(whys) {
                let path = folder.join(r.file(n));
                let at = format!("{case}/{}/{}", r.folder(), r.file(n));
                let print = crate::acoustid::print(&path).unwrap();
                assert_eq!(print.flaws.why().as_deref(), why, "{at}");
                assert_eq!(print.damaged, 0, "{at}: whole");
                assert_eq!(print.duration_secs, f64::from(r.tracks[n - 1].1), "{at}: its length to the sample");
                assert_eq!(print.flaws.sold, crate::analysis::flaws::Sold::Lossless, "{at}");
                let tags = crate::catalogue::tags::read(&path).unwrap().0.items;
                let tag = |name: &str| tags.iter().find(|(_, k, _)| *k == Some(name)).map(|(_, _, v)| v.clone());
                assert_eq!((tag("title"), tag("album"), tag("albumartist")), (Some(r.tracks[n - 1].0.into()), Some(r.album.into()), Some(r.artist.into())), "{at}");
                assert_eq!((tag("tracknumber"), tag("date")), (Some(n.to_string()), Some(r.year.to_string())), "{at}");
            }
        }
        // A note's file and a clean one: the same first track, byte for byte, so only what a case says differs.
        let first = |case: &str| std::fs::read(d.join(case).join(TIDE_TABLES.folder()).join(TIDE_TABLES.file(1))).unwrap();
        assert!(first(NOTES) == first(CLEAN), "made the same way every time");
    }

    /// The end-to-end suite's fake slskd (`tests/sk6_harness/slskd_sharing.rs`) as Cratarr's own client reads it: a search taken,
    /// complete, answered with the shares switched on, which `folders_of` judges as it judges slskd's; a batch slskd is busy for asked
    /// again in the lane and taken, its user's files landing under `cratarr/<batch>` byte for byte, read back done; its transfers in
    /// slskd's downloads until dropped; a batch never taken not known; the search forgotten; every operation recorded and nothing a
    /// violation. Then one: a batch naming a file its user does not share.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_sharing_slskd_answers_the_client_as_slskd_does() {
        use super::slskd_sharing::{Kind, Share, slskd_sharing};
        use crate::slskd::{NotKnown, Offered, Slskd, new_batch};
        let d = std::env::temp_dir().join(format!("cratarr-sharing-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        let r = TIDE_TABLES;
        // Two users' copies of the record, a second a track: real FLACs, so each answer carries its length.
        let copy = |at: &Path, tone: u32| {
            std::fs::create_dir_all(at).unwrap();
            for n in 1..=3 {
                let w = at.join(format!(".{n}.wav"));
                super::super::wav(&w, tone + n as u32, 1).unwrap();
                crate::convert::to_cd_flac(&w, &at.join(r.file(n))).unwrap();
                std::fs::remove_file(&w).unwrap();
            }
        };
        let (a, b) = (d.join("a").join(r.folder()), d.join("b").join(r.folder()));
        copy(&a, 0);
        copy(&b, 8);
        let peer_a = Share::new("peer-a", &a, &r.folder()).speed(100);
        let peer_b = Share::new("peer-b", &b, &r.folder()).speed(200);
        peer_b.switch(false);
        let completed = d.join("slskd");
        let fake = slskd_sharing(&completed, &[&peer_a, &peer_b]).await;
        fake.busy_first(1);
        let client = Slskd::new(&fake.url, "k").unwrap();
        assert!(client.version().await.is_ok(), "an idle read answered");

        let search = client.search("Glass Harbour Tide Tables 2012").await.unwrap();
        assert!(client.searched(&search).await.unwrap(), "complete at once");
        let answers = client.answers(&search).await.unwrap();
        client.forget(&search).await;
        assert_eq!(answers.iter().map(|a| a.username.as_str()).collect::<Vec<_>>(), ["peer-a"], "a share switched off is not offered");
        let list: Vec<crate::catalogue::identity::RefTrack> = r
            .tracks
            .iter()
            .zip(1..)
            .map(|((title, _), n)| crate::catalogue::identity::RefTrack { disc: 1, position: n, title: title.to_string(), length_ms: Some(1_000), recording: None })
            .collect();
        let found = crate::app::soulseek::folders_of(&answers, &list, &[]);
        assert_eq!(found.len(), 1, "{found:?}");
        let f = &found[0];
        assert_eq!((f.user.as_str(), f.format.as_str(), f.lossless, f.free_slot, f.queue, f.speed, f.extra), ("peer-a", "flac", true, true, 0, 100, 0));
        assert_eq!(f.files.iter().map(|o| o.filename.clone()).collect::<Vec<_>>(), peer_a.offered(), "each track's file, in the list's order");
        assert!(f.files.iter().all(|o| o.length == Some(1) && o.bit_depth == Some(16) && o.sample_rate == Some(44_100) && o.size > 0), "{:?}", f.files);

        // slskd busy with its own work first: a 429, nothing taken, asked again in the lane a second on, and taken.
        let batch = new_batch();
        let asked = std::time::Instant::now();
        client.fetch_many(&batch, "peer-a", &f.files, || true).await.unwrap();
        assert!(asked.elapsed() >= std::time::Duration::from_millis(900), "asked again after slskd's busy answer");
        let posts = fake.posts();
        assert_eq!(posts.iter().map(|p| (p.user.as_str(), p.batch.as_str(), p.status)).collect::<Vec<_>>(), [("peer-a", batch.as_str(), 429), ("peer-a", batch.as_str(), 200)]);
        assert_eq!(posts[1].files, peer_a.offered());
        let landed = completed.join(crate::slskd::FOLDER).join(&batch);
        for n in 1..=3 {
            assert!(std::fs::read(landed.join(r.file(n))).unwrap() == std::fs::read(a.join(r.file(n))).unwrap(), "{} as peer-a shares it", r.file(n));
        }
        let transfers = client.transfers(&batch).await.unwrap();
        assert!(transfers.iter().all(|t| t.done() && t.succeeded() && t.percent_complete == 100.0), "{transfers:?}");
        assert_eq!(transfers.iter().map(|t| t.id.clone()).collect::<Vec<_>>(), fake.transfers_of(&batch), "one transfer a file");
        assert_eq!(client.downloads().await.unwrap(), [("peer-a".to_string(), peer_a.offered())], "in slskd's downloads");
        for t in &transfers {
            client.drop_transfer("peer-a", &t.id).await;
        }
        assert_eq!(fake.dropped(), fake.transfers_of(&batch), "each dropped once");
        assert!(client.downloads().await.unwrap().is_empty(), "dropped with remove=true: gone from its downloads");
        let never = client.transfers(&new_batch()).await.unwrap_err();
        assert!(never.is::<NotKnown>(), "a batch never taken: {never:#}");
        let kinds: Vec<Kind> = fake.operations().into_iter().map(|o| o.kind).collect();
        assert_eq!(kinds, [Kind::Search, Kind::ForgetSearch, Kind::Batch, Kind::Batch, Kind::DropTransfer, Kind::DropTransfer, Kind::DropTransfer]);
        assert!(fake.every_search_forgotten() && fake.violations().is_empty(), "{:?}", fake.violations());

        // From the mark on: what it was asked since. A batch naming a file peer-a does not share is a violation, refused.
        fake.mark();
        assert!(fake.operations().is_empty() && fake.posts().is_empty() && fake.dropped().is_empty());
        let other = Offered { filename: format!("Music\\{}\\04 Ebb.flac", r.folder()), size: 1, extension: None, bit_rate: None, bit_depth: None, sample_rate: None, length: None, is_locked: false };
        assert!(client.fetch_many(&new_batch(), "peer-a", &[other], || true).await.is_err());
        assert_eq!(fake.violations().len(), 1, "{:?}", fake.violations());
        assert_eq!(fake.post_users_of(&r.folder()), ["peer-a", "peer-a"], "each try: slskd busy again from the mark, then the refusal");
        std::fs::remove_dir_all(&d).ok();
    }
}
