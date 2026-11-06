//! Which copy of a record plays (docs/records-and-copies.md): the one place that decides. Pure.
//!
//! A copy is one folder's files of a record. Whoever asks, a copy in a library switched off never plays. Then a copy
//! picked by hand; then the viewer's own music, even at a lower quality; then the best quality among the kept libraries.
//! An Incoming copy of a kept record plays only when picked.

pub use crate::settings::LibraryKind as Kind;

/// What a copy is, as far as choosing between copies goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copy {
    /// Its catalogue folder: its library.
    pub folder: i64,
    pub dir: String,
    pub kind: Kind,
    /// Its library's place in the libraries' order: what settles a tie.
    pub order: usize,
    /// One format for all its files, else none.
    pub format: Option<String>,
    /// Its worst file's bitrate.
    pub min_kbps: Option<i64>,
    /// Its worst file's bit depth and sample rate: none on a lossy copy, and on a lossless one with a file not read since
    /// they were kept.
    pub bits: Option<i64>,
    pub rate: Option<i64>,
}

/// Who asks, as far as the choice goes: their own music's folder, and the library they picked for this record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Viewer {
    pub own: Option<i64>,
    pub pick: Option<i64>,
}

/// Whether a copy's files are lossless, by its format: an MP4 at 500 kb/s or more is ALAC (`export::kind_guessed`; AAC
/// stays far below).
fn lossless(c: &Copy) -> bool {
    match c.format.as_deref() {
        Some("FLAC" | "WAV" | "AIFF" | "APE" | "WAVPACK") => true,
        Some("MP4") => c.min_kbps.is_some_and(|k| k >= 500),
        _ => false,
    }
}

/// A copy's quality, the higher the better: lossless before lossy, then bit depth and rate, then bitrate. What is not
/// known ranks last: a lossless copy not read since bit depth was kept, and a copy of mixed formats, which is as good as
/// its worst file and that file is not told.
pub fn rank(c: &Copy) -> (u8, Option<i64>, Option<i64>, Option<i64>) {
    let tier = match (c.format.is_some(), lossless(c), c.bits.is_some() && c.rate.is_some()) {
        (true, true, true) => 2,
        (true, false, _) => 1,
        _ => 0,
    };
    (tier, c.bits, c.rate, c.min_kbps)
}

/// The best of these copies; a tie goes by the libraries' order, then the folder's path, so it never turns on the order
/// they came in.
fn best<'a>(copies: impl Iterator<Item = &'a Copy>) -> Option<&'a Copy> {
    copies.min_by(|a, b| rank(b).cmp(&rank(a)).then(a.order.cmp(&b.order)).then(a.folder.cmp(&b.folder)).then(a.dir.cmp(&b.dir)))
}

/// The copy that plays, of a record's `copies`. `viewer` is none for no one in particular: then it is the row copy, the
/// one that plays for most people, and own music counts as off unless nothing else holds the record (one user's FLAC is
/// not what everyone else's Music shows). `off`: the folders of the libraries switched off. None where nothing plays:
/// every copy off, or only an Incoming copy on of a record kept elsewhere.
pub fn copy_for<'a>(copies: &'a [Copy], viewer: Option<&Viewer>, off: &[i64]) -> Option<&'a Copy> {
    let only_own = copies.iter().all(|c| c.kind == Kind::Own);
    let held: Vec<&Copy> = copies.iter().filter(|c| viewer.is_some() || only_own || c.kind != Kind::Own).collect();
    let on = || held.iter().copied().filter(|c| !off.contains(&c.folder));
    if let Some(v) = viewer {
        for folder in [v.pick, v.own].into_iter().flatten() {
            if let Some(c) = best(on().filter(|c| c.folder == folder)) {
                return Some(c);
            }
        }
    }
    // A record only in Incoming has nothing else to play: its copy plays, as it always has.
    best(on().filter(|c| c.kind != Kind::Incoming)).or_else(|| best(on()).filter(|_| held.iter().all(|c| c.kind == Kind::Incoming)))
}

/// Whether a record plays for this viewer: what every list's "is it switched on" comes to.
#[allow(dead_code)] // the lists, once a track lies in several libraries
pub fn on_for(copies: &[Copy], viewer: Option<&Viewer>, off: &[i64]) -> bool {
    copy_for(copies, viewer, off).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy(folder: i64, kind: Kind, format: &str, kbps: i64, bits: Option<i64>, rate: Option<i64>) -> Copy {
        Copy { folder, dir: "A/B".into(), kind, order: folder as usize, format: Some(format.into()), min_kbps: Some(kbps), bits, rate }
    }

    fn flac(folder: i64, kind: Kind) -> Copy {
        copy(folder, kind, "FLAC", 900, Some(16), Some(44_100))
    }

    fn mp3(folder: i64, kind: Kind, kbps: i64) -> Copy {
        copy(folder, kind, "MP3", kbps, None, Some(44_100))
    }

    fn viewer(own: Option<i64>, pick: Option<i64>) -> Viewer {
        Viewer { own, pick }
    }

    fn folder(c: Option<&Copy>) -> Option<i64> {
        c.map(|c| c.folder)
    }

    #[test]
    fn a_pick_comes_before_all_else() {
        let copies = [flac(0, Kind::Main), mp3(1, Kind::Own, 128), mp3(2, Kind::Incoming, 192)];
        assert_eq!(folder(copy_for(&copies, Some(&viewer(Some(1), Some(2))), &[])), Some(2), "even an Incoming copy, over the viewer's own");
    }

    #[test]
    fn a_pick_in_a_library_off_loses() {
        let copies = [flac(0, Kind::Main), mp3(2, Kind::Incoming, 192)];
        assert_eq!(folder(copy_for(&copies, Some(&viewer(None, Some(2))), &[2])), Some(0));
    }

    #[test]
    fn a_pick_of_a_library_without_a_copy_is_no_pick() {
        assert_eq!(folder(copy_for(&[flac(0, Kind::Main)], Some(&viewer(None, Some(7))), &[])), Some(0));
    }

    #[test]
    fn the_viewers_own_music_comes_before_better_quality() {
        let copies = [flac(0, Kind::Main), mp3(1, Kind::Own, 128)];
        assert_eq!(folder(copy_for(&copies, Some(&viewer(Some(1), None)), &[])), Some(1));
        assert_eq!(folder(copy_for(&copies, Some(&viewer(Some(1), None)), &[1])), Some(0), "not with their own music off");
    }

    #[test]
    fn the_best_kept_copy_on_plays_and_someone_elses_own_music_is_kept_while_it_is_on() {
        let copies = [mp3(0, Kind::Main, 320), flac(1, Kind::Own)];
        assert_eq!(folder(copy_for(&copies, Some(&viewer(None, None)), &[])), Some(1));
        assert_eq!(folder(copy_for(&copies, Some(&viewer(None, None)), &[1])), Some(0));
    }

    #[test]
    fn an_incoming_copy_of_a_kept_record_plays_only_when_picked() {
        let copies = [mp3(0, Kind::Main, 128), flac(2, Kind::Incoming)];
        assert_eq!(folder(copy_for(&copies, Some(&viewer(None, None)), &[])), Some(0), "though it is the better");
        assert_eq!(folder(copy_for(&copies, None, &[])), Some(0));
        assert_eq!(copy_for(&copies, Some(&viewer(None, None)), &[0]), None, "with the kept one off, nothing plays by itself");
    }

    #[test]
    fn a_record_only_in_incoming_plays_its_one_copy() {
        let copies = [mp3(2, Kind::Incoming, 192)];
        assert_eq!(folder(copy_for(&copies, Some(&viewer(None, None)), &[])), Some(2));
        assert_eq!(folder(copy_for(&copies, None, &[])), Some(2));
        assert_eq!(copy_for(&copies, Some(&viewer(None, None)), &[2]), None, "not with Incoming off");
    }

    #[test]
    fn a_copy_in_a_library_off_never_plays() {
        let copies = [flac(0, Kind::Main)];
        assert_eq!(copy_for(&copies, Some(&viewer(None, None)), &[0]), None);
        assert!(!on_for(&copies, Some(&viewer(None, None)), &[0]));
        assert!(on_for(&copies, Some(&viewer(None, None)), &[1]));
    }

    #[test]
    fn the_row_copy_is_not_someones_own_music() {
        let copies = [mp3(0, Kind::Main, 128), flac(1, Kind::Own)];
        assert_eq!(folder(copy_for(&copies, None, &[])), Some(0), "one user's FLAC does not beat Music's MP3");
        let incoming = [mp3(2, Kind::Incoming, 128), flac(1, Kind::Own)];
        assert_eq!(folder(copy_for(&incoming, None, &[])), Some(2), "Incoming holds it too: own music counts as off");
        assert_eq!(folder(copy_for(&[flac(1, Kind::Own)], None, &[])), Some(1), "nothing else holds it");
    }

    #[test]
    fn lossless_comes_before_lossy_and_then_depth_rate_and_bitrate() {
        let order = [
            copy(0, Kind::Main, "FLAC", 3000, Some(24), Some(96_000)),
            copy(1, Kind::Main, "FLAC", 2000, Some(24), Some(48_000)),
            copy(2, Kind::Main, "FLAC", 1400, Some(16), Some(96_000)),
            copy(3, Kind::Main, "FLAC", 900, Some(16), Some(44_100)),
            copy(4, Kind::Main, "FLAC", 800, Some(16), Some(44_100)),
            mp3(5, Kind::Main, 320),
            mp3(6, Kind::Main, 128),
        ];
        for pair in order.windows(2) {
            assert_eq!(folder(copy_for(&[pair[1].clone(), pair[0].clone()], None, &[])), Some(pair[0].folder), "{pair:?}");
        }
    }

    #[test]
    fn an_mp4_at_a_lossless_rate_is_lossless() {
        let alac = copy(0, Kind::Main, "MP4", 800, Some(16), Some(44_100));
        let aac = copy(1, Kind::Main, "MP4", 256, None, Some(44_100));
        assert_eq!(folder(copy_for(&[aac, alac], None, &[])), Some(0));
    }

    #[test]
    fn a_copy_not_fully_read_ranks_last() {
        let unread = copy(0, Kind::Main, "FLAC", 900, None, None);
        assert_eq!(folder(copy_for(&[unread.clone(), mp3(1, Kind::Main, 128)], None, &[])), Some(1));
        let mixed = Copy { format: None, ..flac(0, Kind::Main) };
        assert_eq!(folder(copy_for(&[mixed, mp3(1, Kind::Main, 128)], None, &[])), Some(1));
        assert_eq!(folder(copy_for(&[unread], None, &[])), Some(0), "alone it plays");
    }

    #[test]
    fn a_tie_goes_the_same_way_whatever_order_the_copies_came_in() {
        let (a, b) = (Copy { order: 0, ..flac(5, Kind::Main) }, Copy { order: 1, ..flac(2, Kind::Main) });
        assert_eq!(folder(copy_for(&[a.clone(), b.clone()], None, &[])), Some(5), "by the libraries' order, not the folder's id");
        assert_eq!(folder(copy_for(&[b, a], None, &[])), Some(5));
        let (a, b) = (Copy { dir: "A".into(), ..flac(0, Kind::Main) }, Copy { dir: "B".into(), ..flac(0, Kind::Main) });
        assert_eq!(copy_for(&[b.clone(), a.clone()], None, &[]).map(|c| c.dir.as_str()), Some("A"), "two folders of one library");
        assert_eq!(copy_for(&[a, b], None, &[]).map(|c| c.dir.as_str()), Some("A"));
    }
}
