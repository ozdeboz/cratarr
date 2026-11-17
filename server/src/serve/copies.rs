//! Which copy of a record plays (docs/records-and-copies.md): the one place that decides. Pure.
//!
//! A copy is one folder's files of a record. Whoever asks, a copy in a library switched off never plays. Then the copy this
//! one request names; then the best quality among the kept libraries, the viewer's own music as any of them, a whole copy before
//! one short of tracks. An Incoming copy of a kept record plays only when named. Nothing here is remembered. Whether a record or
//! a track shows is another question, and has its own one answer: `crates::on_for`.

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
    /// It lacks a track its record has: a folder of the very same release still arriving, or short for good, or a file gone a while.
    pub short: bool,
    /// When its first file came: of a record only in people's own music, the first added speaks for it.
    pub added: Option<i64>,
}

/// Someone asking, as far as the choice goes: the library this one request names, if it names one. That anyone asks at all is
/// what counts otherwise: see `copy_for`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Viewer {
    pub from: Option<i64>,
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

/// The best of these copies. A whole one comes first, whatever the other's quality: a card says the copy that plays, and a better
/// copy on its way must not turn a record into three tracks of FLAC and seven of something else, its cover and facts with it, until
/// it is all there. A tie goes by the libraries' order, then the folder's path, so it never turns on the order they came in.
fn best<'a>(copies: impl Iterator<Item = &'a Copy>) -> Option<&'a Copy> {
    copies.min_by(|a, b| a.short.cmp(&b.short).then(rank(b).cmp(&rank(a))).then(a.order.cmp(&b.order)).then(a.folder.cmp(&b.folder)).then(a.dir.cmp(&b.dir)))
}

/// Who keeps a record of two copies that differ, the higher first (CR18, the owner): a main library, then someone's own music,
/// then Incoming. The track count decides only between copies of one kind.
pub fn keeping(kind: Kind) -> u8 {
    match kind {
        Kind::Main => 2,
        Kind::Own => 1,
        Kind::Incoming => 0,
    }
}

/// The copy that plays, of a record's `copies`: the best the kept libraries on hold, whoever's own music it lies in. `viewer` is
/// none for no one in particular: then it is the row copy, the one that plays for most people and whose word the record goes by,
/// and own music counts as off unless nothing else holds the record (one user's FLAC is not what everyone else's Music shows).
/// There the first added stands, whatever its quality, as a second person's copy must not change what the first one's says. `off`:
/// the folders of the libraries switched off. None where nothing plays: every copy off, or only an Incoming copy on of a record
/// kept elsewhere.
pub fn copy_for<'a>(copies: &'a [Copy], viewer: Option<&Viewer>, off: &[i64]) -> Option<&'a Copy> {
    let only_own = copies.iter().all(|c| c.kind == Kind::Own);
    let held: Vec<&Copy> = copies.iter().filter(|c| viewer.is_some() || only_own || c.kind != Kind::Own).collect();
    let on = || held.iter().copied().filter(|c| !off.contains(&c.folder));
    if let Some(c) = viewer.and_then(|v| v.from).and_then(|from| best(on().filter(|c| c.folder == from))) {
        return Some(c);
    }
    if viewer.is_none() && only_own {
        return on().min_by(|a, b| (a.added.is_none(), a.added, a.order, a.folder).cmp(&(b.added.is_none(), b.added, b.order, b.folder)).then(a.dir.cmp(&b.dir)));
    }
    // A record only in Incoming has nothing else to play: its copy plays, as it always has.
    best(on().filter(|c| c.kind != Kind::Incoming)).or_else(|| best(on()).filter(|_| held.iter().all(|c| c.kind == Kind::Incoming)))
}

/// An Incoming copy of a record kept elsewhere, against the copy that plays the record for most among its kept ones (C37a; the
/// owner's OQ1: a record is one by its tracks, so a copy no better is the same record twice): whether it ranks above that copy in
/// the one order copies are chosen in (a whole copy before one short of tracks, then quality), and that copy. None: not an
/// Incoming copy, or the record is kept nowhere else.
pub fn against<'a>(copies: &'a [Copy], incoming: &Copy) -> Option<(bool, &'a Copy)> {
    if incoming.kind != Kind::Incoming {
        return None;
    }
    let kept: Vec<Copy> = copies.iter().filter(|c| c.kind != Kind::Incoming).cloned().collect();
    let row = copy_for(&kept, None, &[])?;
    let row = copies.iter().find(|c| *c == row)?;
    Some((above(incoming, row), row))
}

/// Whether copy `a` ranks above copy `b` in the one order copies are chosen in: a whole copy before one short of tracks, then
/// quality. As good is not above.
fn above(a: &Copy, b: &Copy) -> bool {
    a.short.cmp(&b.short).then(rank(b).cmp(&rank(a))) == std::cmp::Ordering::Less
}

/// A record's copies as copying judges them (S.1, `identity::lacks`): `short` where it says Short, whatever `copies.short` said; a
/// copy it cannot tell of (Unknown) counts as whole (OQ-Q). What RY01's offer reads, never what plays.
pub fn judged(copies: &[(Copy, crate::catalogue::identity::Lacks)]) -> Vec<Copy> {
    copies.iter().map(|(c, l)| Copy { short: *l == crate::catalogue::identity::Lacks::Short, ..c.clone() }).collect()
}

/// The own copy Replace in a main library takes the place of that library's copy `main` from (RY01; OQ2, the owner: "Copy in,
/// admins"), of a record's `copies` as copying judges them (`judged`): the best of the copies in someone's own music, not switched
/// off or out of use (`off`, catalogue folders), where it ranks above `main` (a whole copy before one short of tracks, then
/// quality; a tie is no better). Never a lossy copy over one lossless by its format, whatever is not read of it (OQ-C); never an
/// Incoming copy (that is Replace from Incoming) nor another main library's (that is Move). None where `main` is no main
/// library's. Pure.
pub fn replacing_copy<'a>(copies: &'a [Copy], main: &Copy, off: &[i64]) -> Option<&'a Copy> {
    if main.kind != Kind::Main {
        return None;
    }
    let own = best(copies.iter().filter(|c| c.kind == Kind::Own && !off.contains(&c.folder)))?;
    (above(own, main) && !(lossless(main) && !lossless(own))).then_some(own)
}

/// Every Replace from someone's own music a record's `copies` offer (RY01, OQ-N): one per main library whose copy the own copy
/// beats (`replacing_copy`), that library's first copy as listed (`copies_of` lists them by place); none for one it does not
/// beat. Each as (the own copy, the main library's). Pure.
pub fn replace_offers<'a>(copies: &'a [Copy], off: &[i64]) -> Vec<(&'a Copy, &'a Copy)> {
    let mut seen: Vec<i64> = Vec::new();
    let mut out = Vec::new();
    for main in copies.iter().filter(|c| c.kind == Kind::Main) {
        if seen.contains(&main.folder) {
            continue;
        }
        seen.push(main.folder);
        if let Some(own) = replacing_copy(copies, main, off) {
            out.push((own, main));
        }
    }
    out
}

/// Whether a better copy (`against`) is one Replace takes: Replace puts it in a main library's copy's place, so the copy it is
/// better than must lie in one (Health's better copies; the rest are records in two libraries at once).
pub fn replaces(better: bool, kept: Kind) -> bool {
    better && kept == Kind::Main
}

/// Why an Incoming copy stands as it does against the kept one (`against`), as the intake card, Health and the Library say it:
/// its quality and that copy's, and which of them lacks tracks.
pub fn why(new: &Copy, kept: &Copy) -> String {
    let quality = |c: &Copy| match (lossless(c), c.min_kbps) {
        (true, _) => "lossless".to_string(),
        (false, Some(kbps)) => format!("{kbps} kbps"),
        (false, None) => "unknown quality".to_string(),
    };
    let (n, o) = (quality(new), quality(kept));
    let said = if n == o { format!("{n}, same as yours") } else { format!("{n}, yours is {o}") };
    match (new.short, kept.short) {
        (false, true) => format!("{said}, yours lacks tracks"),
        (true, false) => format!("{said}, lacking tracks"),
        _ => said,
    }
}

/// The library whose owner a record's facts are theirs to change, a catalogue folder: where it lies only in people's own music,
/// that of the copy that speaks for it. Anywhere else the record is the administrators', and none (the owner, 2026-11-07: "If it's
/// owned by administrator, then only administrator can edit").
pub fn owned_by(copies: &[Copy]) -> Option<i64> {
    copies.iter().all(|c| c.kind == Kind::Own).then(|| copy_for(copies, None, &[]).map(|c| c.folder)).flatten()
}

/// A record's copies in the order they would play for this viewer, whatever is off: `copy_for` asked again of what is left,
/// so there is no second rule. The first of these a library switched on holds is what plays, so a switch thrown on the page
/// moves it with no new list; a copy off keeps its place, as what is off must not change the order.
pub fn in_order(copies: &[Copy], viewer: Option<&Viewer>) -> Vec<Copy> {
    let mut left = copies.to_vec();
    let mut order = Vec::with_capacity(left.len());
    while let Some(next) = copy_for(&left, viewer, &[]).cloned() {
        left.retain(|c| *c != next);
        order.push(next);
    }
    order.append(&mut left);
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy(folder: i64, kind: Kind, format: &str, kbps: i64, bits: Option<i64>, rate: Option<i64>) -> Copy {
        Copy { folder, dir: "A/B".into(), kind, order: folder as usize, format: Some(format.into()), min_kbps: Some(kbps), bits, rate, short: false, added: None }
    }

    fn flac(folder: i64, kind: Kind) -> Copy {
        copy(folder, kind, "FLAC", 900, Some(16), Some(44_100))
    }

    fn mp3(folder: i64, kind: Kind, kbps: i64) -> Copy {
        copy(folder, kind, "MP3", kbps, None, Some(44_100))
    }

    const ANYONE: &Viewer = &Viewer { from: None };

    /// C37a: an Incoming copy of a kept record stands against the copy that plays it for most, by the order copies are chosen in:
    /// better where it ranks above it, the same record twice where not; a record kept nowhere else has nothing to stand against.
    #[test]
    fn an_incoming_copy_stands_against_the_copy_that_plays_for_most() {
        let waiting = flac(2, Kind::Incoming);
        let copies = [mp3(0, Kind::Main, 320), flac(1, Kind::Own), waiting.clone()];
        let (better, kept) = against(&copies, &waiting).unwrap();
        assert!(better, "lossless over 320 kb/s");
        assert_eq!(kept.folder, 0, "Music's copy plays for most, not someone's own");
        assert_eq!(why(&waiting, kept), "lossless, yours is 320 kbps");
        let same = mp3(2, Kind::Incoming, 320);
        let twice = [mp3(0, Kind::Main, 320), same.clone()];
        assert_eq!(against(&twice, &same).map(|(b, k)| (b, k.folder)), Some((false, 0)), "the same quality: not better");
        assert_eq!(why(&same, &twice[0]), "320 kbps, same as yours");
        let short = Copy { short: true, ..flac(2, Kind::Incoming) };
        let whole = mp3(0, Kind::Main, 128);
        assert_eq!(against(&[whole.clone(), short.clone()], &short).map(|(b, _)| b), Some(false), "short of tracks: never better than a whole copy");
        assert_eq!(why(&short, &whole), "lossless, yours is 128 kbps, lacking tracks");
        let own = [flac(1, Kind::Own), mp3(2, Kind::Incoming, 320)];
        assert_eq!(against(&own, &own[1]).map(|(b, k)| (b, k.folder)), Some((false, 1)), "kept in own music alone: against that");
        assert!(against(&[waiting.clone()], &waiting).is_none(), "kept nowhere else");
        assert!(against(&copies, &copies[0]).is_none(), "a kept copy stands against nothing");
        assert!(replaces(true, Kind::Main) && !replaces(true, Kind::Own) && !replaces(false, Kind::Main), "Replace takes a main library's place");
    }

    /// RY01.2: the own copy offered against Music's is the best one above it, as copying judges copies (S.1): never a short one
    /// beside Music's whole one, a tie, one switched off or parked, an Incoming copy or another main library's, nor a lossy one over
    /// Music's lossless (OQ-C); of two better, the best. One line per main library it beats (OQ-N). S.2 cases 3 and 4, through
    /// `lacks`: both short alike by their clean tags, Bob's is offered by its quality; Bob's short beside Music's whole, never.
    #[test]
    fn the_own_copy_offered_is_the_best_whole_one_above_musics() {
        use crate::catalogue::identity::{FileNumbers, Judged, lacks};
        let music = mp3(0, Kind::Main, 320);
        let bob = flac(3, Kind::Own);
        assert_eq!(folder(replacing_copy(&[music.clone(), bob.clone()], &music, &[])), Some(3));
        assert_eq!(replacing_copy(&[music.clone(), Copy { short: true, ..bob.clone() }], &music, &[]), None, "short: never");
        assert_eq!(replacing_copy(&[music.clone(), mp3(2, Kind::Own, 320)], &music, &[]), None, "a tie is no better");
        assert_eq!(folder(replacing_copy(&[music.clone(), mp3(2, Kind::Own, 320), bob.clone()], &music, &[])), Some(3), "Bob's, beside Ann's tie");
        let hires = copy(2, Kind::Own, "FLAC", 3000, Some(24), Some(96_000));
        assert_eq!(folder(replacing_copy(&[music.clone(), bob.clone(), hires], &music, &[])), Some(2), "the best of two better");
        assert_eq!(replacing_copy(&[music.clone(), bob.clone()], &music, &[3]), None, "switched off or parked");
        assert_eq!(replacing_copy(&[music.clone(), flac(1, Kind::Incoming)], &music, &[]), None, "Incoming is today's Replace");
        assert_eq!(replacing_copy(&[music.clone(), flac(4, Kind::Main)], &music, &[]), None, "another main library's is Move's");
        let short_music = Copy { short: true, ..mp3(0, Kind::Main, 320) };
        assert_eq!(folder(replacing_copy(&[short_music.clone(), mp3(3, Kind::Own, 320)], &short_music, &[])), Some(3), "whole before short");
        assert_eq!(replacing_copy(std::slice::from_ref(&music), &music, &[]), None, "another record's FLAC is no copy of this one (OQ1)");
        let unread = copy(0, Kind::Main, "FLAC", 900, None, None);
        assert_eq!(replacing_copy(&[unread.clone(), mp3(3, Kind::Own, 320)], &unread, &[]), None, "OQ-C: lossy never over lossless");
        let waiting = flac(1, Kind::Incoming);
        assert_eq!(replacing_copy(&[waiting.clone(), bob.clone()], &waiting, &[]), None, "never against what is not a main library's");
        // OQ-N: one line per main library the own copy beats, none for one it does not; never two for one library.
        let second = Copy { order: 4, ..flac(4, Kind::Main) };
        let record = [music.clone(), Copy { dir: "A/C".into(), ..mp3(0, Kind::Main, 128) }, second, bob.clone()];
        let offers = replace_offers(&record, &[]);
        assert_eq!(offers.iter().map(|(o, m)| (o.folder, m.folder, m.dir.as_str())).collect::<Vec<_>>(), [(3, 0, "A/B")]);
        // Shortness as S.1 judges it, not `copies.short` alone.
        let tagged = |n: u32, total: u32| (1..=n).map(|t| FileNumbers { disc: Some(1), number: Some(t), total: Some(total) }).collect::<Vec<_>>();
        let said = |fewer: bool, files: Vec<FileNumbers>| lacks(&Judged { fewer_than_its_copies: fewer, files, ..Judged::default() });
        let lone = judged(&[(music.clone(), said(false, tagged(10, 10))), (bob.clone(), said(false, tagged(8, 10)))]);
        assert_eq!(replacing_copy(&lone, &lone[0], &[]), None, "Bob's short by his clean tags beside Music's whole, though copies.short says whole");
        let both = judged(&[(music.clone(), said(false, tagged(8, 10))), (bob.clone(), said(false, tagged(8, 10)))]);
        assert_eq!(folder(replacing_copy(&both, &both[0], &[])), Some(3), "S.2 case 3: both short alike, Bob's offered by quality");
        let fewer = judged(&[(music.clone(), said(false, tagged(10, 10))), (bob.clone(), said(true, tagged(8, 10)))]);
        assert_eq!(replacing_copy(&fewer, &fewer[0], &[]), None, "S.2 case 4: Bob's 8 of Music's 10, never");
        let untidy = judged(&[(music.clone(), said(false, tagged(10, 10))), (bob.clone(), said(false, vec![FileNumbers { total: None, ..tagged(1, 8)[0] }]))]);
        assert_eq!(folder(replacing_copy(&untidy, &untidy[0], &[])), Some(3), "tags that cannot say count as whole (OQ-Q)");
    }

    fn naming(from: i64) -> Viewer {
        Viewer { from: Some(from) }
    }

    /// The owner, 2026-11-07: only in people's own music, the first copy added decides what the record says for most people, whatever
    /// a later one's quality. Whoever listens hears the best they have on, as anywhere.
    #[test]
    fn of_copies_only_in_peoples_own_music_the_first_added_speaks_for_the_record() {
        let first = Copy { added: Some(100), ..mp3(1, Kind::Own, 128) };
        let later = Copy { added: Some(200), ..flac(0, Kind::Own) };
        let copies = [later.clone(), first.clone()];
        assert_eq!(folder(copy_for(&copies, None, &[])), Some(1), "though it is the worse, in the later library");
        assert_eq!(in_order(&copies, None).iter().map(|c| c.folder).collect::<Vec<_>>(), [1, 0]);
        assert_eq!(folder(copy_for(&copies, Some(ANYONE), &[])), Some(0), "a listener hears the better");
        assert_eq!(folder(copy_for(&copies, None, &[1])), Some(0), "with it off, what is left");
        let unknown = Copy { added: None, ..mp3(2, Kind::Own, 128) };
        assert_eq!(folder(copy_for(&[unknown, first], None, &[])), Some(1), "a date goes before none");
    }

    /// Beside a kept copy or an Incoming one nothing changes: own music does not count for most people, first added or not.
    #[test]
    fn being_added_first_says_nothing_beside_a_library_not_someones_own() {
        let own = Copy { added: Some(100), ..flac(1, Kind::Own) };
        assert_eq!(folder(copy_for(&[Copy { added: Some(200), ..mp3(0, Kind::Main, 128) }, own.clone()], None, &[])), Some(0));
        assert_eq!(folder(copy_for(&[Copy { added: Some(200), ..mp3(2, Kind::Incoming, 128) }, own], None, &[])), Some(2));
    }

    /// "If it's owned by administrator, then only administrator can edit": a main library or Incoming beside it makes it theirs.
    #[test]
    fn a_record_is_a_persons_to_change_only_where_it_lies_only_in_their_own_music_first() {
        let (mine, yours) = (Copy { added: Some(100), ..mp3(1, Kind::Own, 128) }, Copy { added: Some(200), ..flac(2, Kind::Own) });
        assert_eq!(owned_by(std::slice::from_ref(&mine)), Some(1));
        assert_eq!(owned_by(&[yours.clone(), mine.clone()]), Some(1), "of two people's, the first added's");
        assert_eq!(owned_by(&[mine.clone(), flac(0, Kind::Main)]), None, "in a main library");
        assert_eq!(owned_by(&[mine, flac(3, Kind::Incoming)]), None, "beside Incoming, the administrators'");
        assert_eq!(owned_by(&[]), None, "not had");
    }

    /// A better copy on its way changes nothing until it is all there: what plays, what a card says, and what a list says of it.
    #[test]
    fn a_whole_copy_plays_before_a_better_one_short_of_tracks() {
        let arriving = Copy { short: true, ..flac(1, Kind::Main) };
        let copies = [mp3(0, Kind::Main, 320), arriving.clone()];
        assert_eq!(folder(copy_for(&copies, Some(ANYONE), &[])), Some(0), "though it is the better");
        assert_eq!(folder(copy_for(&copies, None, &[])), Some(0), "and for most people");
        assert_eq!(in_order(&copies, Some(ANYONE)).iter().map(|c| c.folder).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(folder(copy_for(&copies, Some(&naming(1)), &[])), Some(1), "a request can name it");
        assert_eq!(folder(copy_for(&copies, Some(ANYONE), &[0])), Some(1), "with the whole one off it is what there is");
        assert_eq!(folder(copy_for(&[flac(0, Kind::Main), Copy { short: false, ..arriving }], Some(ANYONE), &[])), Some(0), "whole, it counts as any copy");
    }

    /// An Incoming copy short of tracks is no nearer playing by itself than a whole one, and a kept copy short of tracks is still kept.
    #[test]
    fn being_short_of_tracks_says_nothing_of_incoming() {
        let short_in_music = Copy { short: true, ..mp3(0, Kind::Main, 128) };
        assert_eq!(folder(copy_for(&[short_in_music, flac(1, Kind::Incoming)], Some(ANYONE), &[])), Some(0), "whole in Incoming plays only by name");
        let waiting = Copy { short: true, ..flac(1, Kind::Incoming) };
        assert_eq!(folder(copy_for(&[mp3(0, Kind::Main, 128), waiting], Some(ANYONE), &[])), Some(0));
    }

    /// Of copies all short of something, quality says, as among whole ones.
    #[test]
    fn of_copies_all_short_of_something_the_best_plays() {
        let copies = [Copy { short: true, ..mp3(0, Kind::Main, 320) }, Copy { short: true, ..flac(1, Kind::Main) }];
        assert_eq!(folder(copy_for(&copies, Some(ANYONE), &[])), Some(1));
    }

    fn folder(c: Option<&Copy>) -> Option<i64> {
        c.map(|c| c.folder)
    }

    #[test]
    fn a_copy_a_request_names_comes_before_all_else() {
        let copies = [flac(0, Kind::Main), mp3(1, Kind::Own, 128), mp3(2, Kind::Incoming, 192)];
        assert_eq!(folder(copy_for(&copies, Some(&naming(2)), &[])), Some(2), "even an Incoming copy, over a better one");
        assert_eq!(folder(copy_for(&copies, Some(&naming(1)), &[])), Some(1), "at a lower quality");
    }

    #[test]
    fn a_copy_named_in_a_library_off_loses() {
        let copies = [flac(0, Kind::Main), mp3(2, Kind::Incoming, 192)];
        assert_eq!(folder(copy_for(&copies, Some(&naming(2)), &[2])), Some(0));
    }

    #[test]
    fn a_library_without_a_copy_names_nothing() {
        assert_eq!(folder(copy_for(&[flac(0, Kind::Main)], Some(&naming(7)), &[])), Some(0));
    }

    /// CR17, the owner: their own music comes first no more. The best quality plays, whoever's library holds it.
    #[test]
    fn the_viewers_own_music_counts_as_any_kept_library() {
        let lossy_own = [flac(0, Kind::Main), mp3(1, Kind::Own, 320)];
        assert_eq!(folder(copy_for(&lossy_own, Some(ANYONE), &[])), Some(0), "a FLAC in Music beats their own MP3");
        assert_eq!(folder(copy_for(&lossy_own, Some(ANYONE), &[0])), Some(1), "with Music off");
        let lossless_own = [mp3(0, Kind::Main, 320), flac(1, Kind::Own)];
        assert_eq!(folder(copy_for(&lossless_own, Some(ANYONE), &[])), Some(1), "their own FLAC beats Music's MP3");
        assert_eq!(folder(copy_for(&lossless_own, Some(ANYONE), &[1])), Some(0), "not with their own music off");
    }

    /// As it was before CR17: of two alike the libraries' order decides, own music or not.
    #[test]
    fn a_tie_goes_by_the_libraries_order_whoever_owns_either() {
        let tie = [mp3(1, Kind::Own, 320), mp3(0, Kind::Main, 320)];
        assert_eq!(folder(copy_for(&tie, Some(ANYONE), &[])), Some(0));
        let ahead = [Copy { order: 0, ..mp3(1, Kind::Own, 320) }, Copy { order: 1, ..mp3(0, Kind::Main, 320) }];
        assert_eq!(folder(copy_for(&ahead, Some(ANYONE), &[])), Some(1));
    }

    #[test]
    fn the_best_kept_copy_on_plays_and_someone_elses_own_music_is_kept_while_it_is_on() {
        let copies = [mp3(0, Kind::Main, 320), flac(1, Kind::Own)];
        assert_eq!(folder(copy_for(&copies, Some(ANYONE), &[])), Some(1));
        assert_eq!(folder(copy_for(&copies, Some(ANYONE), &[1])), Some(0));
    }

    #[test]
    fn an_incoming_copy_of_a_kept_record_plays_only_when_named() {
        let copies = [mp3(0, Kind::Main, 128), flac(2, Kind::Incoming)];
        assert_eq!(folder(copy_for(&copies, Some(ANYONE), &[])), Some(0), "though it is the better");
        assert_eq!(folder(copy_for(&copies, None, &[])), Some(0));
        assert_eq!(copy_for(&copies, Some(ANYONE), &[0]), None, "with the kept one off, nothing plays by itself");
        assert_eq!(folder(copy_for(&copies, Some(&naming(2)), &[0])), Some(2), "a request can name it");
    }

    #[test]
    fn a_record_only_in_incoming_plays_its_one_copy() {
        let copies = [mp3(2, Kind::Incoming, 192)];
        assert_eq!(folder(copy_for(&copies, Some(ANYONE), &[])), Some(2));
        assert_eq!(folder(copy_for(&copies, None, &[])), Some(2));
        assert_eq!(copy_for(&copies, Some(ANYONE), &[2]), None, "not with Incoming off");
    }

    #[test]
    fn a_copy_in_a_library_off_never_plays() {
        let copies = [flac(0, Kind::Main)];
        assert_eq!(copy_for(&copies, Some(ANYONE), &[0]), None);
    }

    #[test]
    fn the_row_copy_is_not_someones_own_music() {
        let copies = [mp3(0, Kind::Main, 128), flac(1, Kind::Own)];
        assert_eq!(folder(copy_for(&copies, None, &[])), Some(0), "one user's FLAC does not beat Music's MP3");
        let incoming = [mp3(2, Kind::Incoming, 128), flac(1, Kind::Own)];
        assert_eq!(folder(copy_for(&incoming, None, &[])), Some(2), "Incoming holds it too: own music counts as off");
        assert_eq!(folder(copy_for(&[flac(1, Kind::Own)], None, &[])), Some(1), "nothing else holds it");
    }

    /// The page reads `in_order` by itself (`copyPlaying`: the first copy on, an Incoming one only when named or the list
    /// said it plays, `PlayOrder::of`, read here with nothing off), so that reading must come to `copy_for`'s answer for every
    /// viewer and every set of libraries off, or the card and the stream name two copies. One case cannot: of copies only in
    /// Incoming, with the one that plays switched off since the list was read, the page knows of no other until it reads again.
    #[test]
    fn the_first_copy_on_in_order_is_what_copy_for_plays_whatever_is_off() {
        let records = [
            vec![flac(0, Kind::Main), mp3(1, Kind::Own, 128), mp3(2, Kind::Incoming, 192), flac(3, Kind::Own)],
            vec![mp3(0, Kind::Main, 128), mp3(1, Kind::Main, 320), flac(2, Kind::Incoming)],
            vec![mp3(2, Kind::Incoming, 128), flac(3, Kind::Incoming)],
            vec![flac(1, Kind::Own), mp3(3, Kind::Own, 128)],
        ];
        for copies in &records {
            let folders: Vec<i64> = copies.iter().map(|c| c.folder).collect();
            let mut viewers = vec![None, Some(Viewer::default())];
            viewers.extend(folders.iter().map(|f| Some(naming(*f))));
            for who in &viewers {
                let order = in_order(copies, who.as_ref());
                assert_eq!(order.len(), copies.len(), "every copy, off or not");
                for set in 0..1 << folders.len() {
                    let off: Vec<i64> = folders.iter().enumerate().filter(|(i, _)| set >> i & 1 == 1).map(|(_, f)| *f).collect();
                    let only_incoming = copies.iter().all(|c| c.kind == Kind::Incoming);
                    let named = |c: &Copy| who.as_ref().is_some_and(|v| v.from == Some(c.folder));
                    let plays = copy_for(copies, who.as_ref(), &[]);
                    let read = order.iter().find(|c| !off.contains(&c.folder) && (c.kind != Kind::Incoming || named(c) || plays == Some(*c)));
                    if only_incoming && read.is_none() && plays.is_some_and(|p| off.contains(&p.folder)) {
                        continue;
                    }
                    // With no one, own music counts as off unless nothing else holds the record, and the order puts it last.
                    let shunned = |c: &&Copy| who.is_none() && c.kind == Kind::Own && copies.iter().any(|o| o.kind != Kind::Own);
                    assert_eq!(read.filter(|c| !shunned(c)), copy_for(copies, who.as_ref(), &off), "{who:?}, {off:?} off, in {order:?}");
                }
            }
        }
    }

    #[test]
    fn a_copy_off_keeps_its_place_in_the_order() {
        let copies = [mp3(1, Kind::Main, 128), flac(0, Kind::Main), mp3(2, Kind::Incoming, 320)];
        let order: Vec<i64> = in_order(&copies, Some(ANYONE)).iter().map(|c| c.folder).collect();
        assert_eq!(order, [0, 1, 2], "best first, Incoming last though it beats the MP3");
        let named: Vec<i64> = in_order(&copies, Some(&naming(2))).iter().map(|c| c.folder).collect();
        assert_eq!(named, [2, 0, 1], "the copy named, then the rest as they would play");
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
