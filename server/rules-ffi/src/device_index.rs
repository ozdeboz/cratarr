//! The library on this device, as the app's sync asks for it (PREVIEW2's D15). Kotlin decides nothing: what to do, and what each
//! answer and each set of bytes came to, are the rules crate's (`index::sync`, the rule the page asks too). This is that rule in
//! UniFFI's types, and the two calls that take bytes, so an index is held in memory only while one of them runs.

use cratarr_rules::index::sync::{self, COVERS_VERSION, Mobile, Network, Outcome, Part, Step, covers_rev};
use cratarr_rules::index::{Changes, Index, MARKS_VERSION, Marks, Refused, SOUNDS_VERSION, SoundChanges, Sounds, VERSION};

/// What a device keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SyncKind {
    Index,
    Sounds,
    Covers,
    /// An account's own (`index::Marks`): no feed, fetched whole when it has stood a while.
    Own,
}

/// How one part stands (`index::sync::Part`). The app keeps it, as it must outlast a start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct SyncPart {
    pub whole: bool,
    pub stopped: bool,
    pub lost: u32,
    pub failed: u32,
    pub tried: Option<i64>,
    pub started: i64,
}

impl From<SyncPart> for Part {
    fn from(p: SyncPart) -> Self {
        Self { whole: p.whole, stopped: p.stopped, lost: p.lost, failed: p.failed, tried: p.tried, started: p.started }
    }
}

impl From<Part> for SyncPart {
    fn from(p: Part) -> Self {
        Self { whole: p.whole, stopped: p.stopped, lost: p.lost, failed: p.failed, tried: p.tried, started: p.started }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SyncNetwork {
    Free,
    Metered,
    Off,
}

impl From<SyncNetwork> for Network {
    fn from(n: SyncNetwork) -> Self {
        match n {
            SyncNetwork::Free => Self::Free,
            SyncNetwork::Metered => Self::Metered,
            SyncNetwork::Off => Self::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SyncMobile {
    Unasked,
    Yes,
    No,
}

impl From<SyncMobile> for Mobile {
    fn from(m: SyncMobile) -> Self {
        match m {
            SyncMobile::Unasked => Self::Unasked,
            SyncMobile::Yes => Self::Yes,
            SyncMobile::No => Self::No,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SyncStep {
    Stopped,
    Rest,
    Wait { until: i64 },
    Ask,
    Whole,
    Changes,
}

impl From<Step> for SyncStep {
    fn from(s: Step) -> Self {
        match s {
            Step::Stopped => Self::Stopped,
            Step::Rest => Self::Rest,
            Step::Wait(until) => Self::Wait { until },
            Step::Ask => Self::Ask,
            Step::Whole => Self::Whole,
            Step::Changes => Self::Changes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SyncOutcome {
    Took,
    Lost,
    Failed,
    StartAgain,
    Version,
}

impl From<SyncOutcome> for Outcome {
    fn from(o: SyncOutcome) -> Self {
        match o {
            SyncOutcome::Took => Self::Took,
            SyncOutcome::Lost => Self::Lost,
            SyncOutcome::Failed => Self::Failed,
            SyncOutcome::StartAgain => Self::StartAgain,
            SyncOutcome::Version => Self::Version,
        }
    }
}

impl From<Outcome> for SyncOutcome {
    fn from(o: Outcome) -> Self {
        match o {
            Outcome::Took => Self::Took,
            Outcome::Lost => Self::Lost,
            Outcome::Failed => Self::Failed,
            Outcome::StartAgain => Self::StartAgain,
            Outcome::Version => Self::Version,
        }
    }
}

/// What to do now with a part. `told`: its feed may be asked.
#[uniffi::export]
pub fn sync_step(part: SyncPart, network: SyncNetwork, mobile: SyncMobile, told: bool, now: i64) -> SyncStep {
    Part::from(part).step(network.into(), mobile.into(), told, now).into()
}

/// What to do now with an account's own. `took`: when it was last taken.
#[uniffi::export]
pub fn sync_step_stale(part: SyncPart, network: SyncNetwork, mobile: SyncMobile, took: Option<i64>, now: i64) -> SyncStep {
    Part::from(part).step_stale(network.into(), mobile.into(), took, now).into()
}

/// How a part stands after a fetch.
#[uniffi::export]
pub fn sync_after(part: SyncPart, outcome: SyncOutcome, now: i64) -> SyncPart {
    Part::from(part).after(outcome.into(), now).into()
}

/// A start over what this version kept.
#[uniffi::export]
pub fn sync_anew(part: SyncPart, now: i64) -> SyncPart {
    Part::from(part).anew(now).into()
}

/// A start over what another version kept.
#[uniffi::export]
pub fn sync_anew_by_another(part: SyncPart, now: i64) -> SyncPart {
    Part::from(part).anew_by_another(now).into()
}

/// Whether a part should say it is paused.
#[uniffi::export]
pub fn sync_paused(part: SyncPart) -> bool {
    Part::from(part).paused()
}

/// How long after a start the first fetch waits (ms), so a worker can wait them out.
#[uniffi::export]
pub fn sync_first_after_ms() -> i64 {
    sync::FIRST_AFTER_MS
}

/// Hidden this long, a device coming back has started again (ms).
#[uniffi::export]
pub fn sync_anew_after_ms() -> i64 {
    sync::ANEW_AFTER_MS
}

/// How an answer to a fetch ended it, or nothing where it brought bytes to take. `status`: none where no answer came.
#[uniffi::export]
pub fn sync_answer(status: Option<u16>, whole: bool) -> Option<SyncOutcome> {
    sync::ended(status, whole).map(Into::into)
}

/// The version a part's bytes must be for this app to read them.
#[uniffi::export]
pub fn sync_version(kind: SyncKind) -> u32 {
    match kind {
        SyncKind::Index => VERSION,
        SyncKind::Sounds => SOUNDS_VERSION,
        SyncKind::Covers => COVERS_VERSION,
        SyncKind::Own => MARKS_VERSION,
    }
}

/// What bytes came to, and where they leave the part.
#[derive(Debug, PartialEq, Eq, uniffi::Record)]
pub struct SyncTaken {
    pub outcome: SyncOutcome,
    pub rev: Option<i64>,
    /// What to keep now, after a feed. Bytes fetched whole are kept as they came.
    pub bytes: Option<Vec<u8>>,
}

fn whole<T>(loaded: Result<T, Refused>, rev: fn(&T) -> i64) -> SyncTaken {
    SyncTaken { outcome: Outcome::of_whole(&loaded).into(), rev: loaded.as_ref().ok().map(rev), bytes: None }
}

fn changes<T>(applied: Result<T, Refused>, rev: fn(&T) -> i64, save: fn(&T) -> Vec<u8>) -> SyncTaken {
    let outcome = Outcome::of_changes(&applied).into();
    let (rev, bytes) = applied.map_or((None, None), |held| (Some(rev(&held)), Some(save(&held))));
    SyncTaken { outcome, rev, bytes }
}

/// Bytes just fetched whole, read through. Of the covers only their head: their frames are the app's to read as they come.
#[uniffi::export]
pub fn sync_take_whole(kind: SyncKind, bytes: Vec<u8>) -> SyncTaken {
    match kind {
        SyncKind::Index => whole(Index::load(&bytes), Index::rev),
        SyncKind::Sounds => whole(Sounds::load(&bytes), Sounds::rev),
        SyncKind::Covers => whole(covers_rev(&bytes), |rev| *rev),
        SyncKind::Own => whole(Marks::load(&bytes), Marks::at),
    }
}

/// A feed applied to what is kept. Of the covers `feed` is the feed's head, and nothing comes back to keep.
#[uniffi::export]
pub fn sync_take_changes(kind: SyncKind, kept: Vec<u8>, feed: Vec<u8>) -> SyncTaken {
    match kind {
        SyncKind::Covers => {
            let head = covers_rev(&feed);
            SyncTaken { outcome: Outcome::of_changes(&head).into(), rev: head.ok(), bytes: None }
        }
        SyncKind::Index => {
            let applied = Index::load(&kept).and_then(|mut index| Changes::load(&feed).and_then(|c| index.apply(&c)).map(|()| index));
            changes(applied, Index::rev, Index::save)
        }
        SyncKind::Sounds => {
            let applied = Sounds::load(&kept).and_then(|mut sounds| SoundChanges::load(&feed).and_then(|c| sounds.apply(&c)).map(|()| sounds));
            changes(applied, Sounds::rev, Sounds::save)
        }
        // It has no feed: nothing can go on from what is kept.
        SyncKind::Own => SyncTaken { outcome: SyncOutcome::StartAgain, rev: None, bytes: None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHOLE: &[u8] = include_bytes!("../../rules/tests/vectors/index/whole.bin");
    const CHANGES: &[u8] = include_bytes!("../../rules/tests/vectors/index/changes.bin");
    const AFTER: &[u8] = include_bytes!("../../rules/tests/vectors/index/after.bin");
    const SOUNDS: &[u8] = include_bytes!("../../rules/tests/vectors/index/sounds.bin");
    const SOUND_CHANGES: &[u8] = include_bytes!("../../rules/tests/vectors/index/sound-changes.bin");
    const SOUNDS_AFTER: &[u8] = include_bytes!("../../rules/tests/vectors/index/sounds-after.bin");

    fn nothing() -> SyncPart {
        SyncPart { whole: true, stopped: false, lost: 0, failed: 0, tried: None, started: 0 }
    }

    /// The version sits in the header, read before anything else.
    fn another_version(bytes: &[u8]) -> Vec<u8> {
        let mut out = bytes.to_vec();
        out[4] = 9;
        out
    }

    #[test]
    fn each_way_of_a_part_reads_as_the_crate_has_it() {
        let held = SyncPart { whole: false, ..nothing() };
        let later = sync_first_after_ms();
        assert_eq!(sync_step(nothing(), SyncNetwork::Free, SyncMobile::Unasked, false, 0), SyncStep::Wait { until: later });
        assert_eq!(sync_step(nothing(), SyncNetwork::Free, SyncMobile::Unasked, false, later), SyncStep::Whole);
        assert_eq!(sync_step(nothing(), SyncNetwork::Metered, SyncMobile::Unasked, false, later), SyncStep::Ask);
        assert_eq!(sync_step(nothing(), SyncNetwork::Metered, SyncMobile::Yes, false, later), SyncStep::Whole);
        assert_eq!(sync_step(nothing(), SyncNetwork::Metered, SyncMobile::No, false, later), SyncStep::Rest);
        assert_eq!(sync_step(nothing(), SyncNetwork::Off, SyncMobile::Yes, false, later), SyncStep::Rest);
        assert_eq!(sync_step(held, SyncNetwork::Metered, SyncMobile::No, true, later), SyncStep::Changes);
        assert_eq!(sync_step(held, SyncNetwork::Metered, SyncMobile::No, false, later), SyncStep::Rest, "not told");
        assert_eq!(sync_step(SyncPart { stopped: true, ..held }, SyncNetwork::Free, SyncMobile::Yes, true, later), SyncStep::Stopped);
    }

    #[test]
    fn a_part_goes_in_with_all_it_knows() {
        let held = SyncPart { whole: false, ..nothing() };
        let at = 100_000;
        let step = |part| sync_step(part, SyncNetwork::Free, SyncMobile::Unasked, true, at);
        assert_eq!(step(SyncPart { lost: 1, tried: Some(at), ..held }), SyncStep::Wait { until: at + sync::LOST_AGAIN_MS });
        assert_eq!(step(SyncPart { failed: 1, tried: Some(at), ..held }), SyncStep::Wait { until: at + sync::FAILED_AGAIN_MS });
        assert_eq!(step(SyncPart { started: at, ..held }), SyncStep::Wait { until: at + sync::FIRST_AFTER_MS });
    }

    #[test]
    fn each_way_a_fetch_ends_reads_as_the_crate_has_it() {
        let held = SyncPart { whole: false, ..nothing() };
        assert_eq!(sync_after(nothing(), SyncOutcome::Took, 7), SyncPart { tried: Some(7), ..held });
        assert_eq!(sync_after(held, SyncOutcome::Lost, 7), SyncPart { lost: 1, tried: Some(7), ..held });
        assert_eq!(sync_after(held, SyncOutcome::Failed, 7), SyncPart { failed: 1, tried: Some(7), ..held });
        assert_eq!(sync_after(held, SyncOutcome::StartAgain, 7), SyncPart { tried: Some(7), ..nothing() });
        assert_eq!(sync_after(nothing(), SyncOutcome::Version, 7), SyncPart { stopped: true, tried: Some(7), ..nothing() });
    }

    #[test]
    fn a_start_keeps_a_stop_and_one_over_another_versions_lifts_it_and_comes_whole() {
        let stopped = SyncPart { whole: false, stopped: true, lost: 2, failed: 1, tried: Some(5), started: 1 };
        assert_eq!(sync_anew(stopped, 9), SyncPart { whole: false, stopped: true, lost: 0, failed: 0, tried: None, started: 9 });
        assert_eq!(sync_anew_by_another(stopped, 9), SyncPart { started: 9, ..nothing() });
    }

    #[test]
    fn paused_is_the_crates_to_say() {
        assert!(!sync_paused(SyncPart { lost: 2, ..nothing() }));
        assert!(sync_paused(SyncPart { lost: 3, ..nothing() }));
    }

    #[test]
    fn an_answer_says_how_it_ended_or_that_it_brought_bytes() {
        assert_eq!(sync_answer(None, false), Some(SyncOutcome::Lost));
        assert_eq!(sync_answer(Some(200), true), None);
        assert_eq!(sync_answer(Some(204), false), Some(SyncOutcome::Took));
        assert_eq!(sync_answer(Some(410), false), Some(SyncOutcome::StartAgain));
        assert_eq!(sync_answer(Some(500), false), Some(SyncOutcome::Failed));
    }

    #[test]
    fn each_part_says_its_own_version() {
        assert_eq!(sync_version(SyncKind::Index), VERSION);
        assert_eq!(sync_version(SyncKind::Sounds), SOUNDS_VERSION);
        assert_eq!(sync_version(SyncKind::Covers), COVERS_VERSION);
    }

    #[test]
    fn bytes_fetched_whole_are_read_through_and_say_where_they_stand() {
        assert_eq!(sync_take_whole(SyncKind::Index, WHOLE.to_vec()), SyncTaken { outcome: SyncOutcome::Took, rev: Some(5), bytes: None });
        assert_eq!(sync_take_whole(SyncKind::Sounds, SOUNDS.to_vec()), SyncTaken { outcome: SyncOutcome::Took, rev: Some(5), bytes: None });
    }

    #[test]
    fn another_version_in_bytes_fetched_whole_stops_and_anything_else_refused_fails() {
        assert_eq!(sync_take_whole(SyncKind::Index, another_version(WHOLE)).outcome, SyncOutcome::Version);
        assert_eq!(sync_take_whole(SyncKind::Sounds, another_version(SOUNDS)).outcome, SyncOutcome::Version);
        assert_eq!(sync_take_whole(SyncKind::Index, WHOLE[..100].to_vec()).outcome, SyncOutcome::Failed);
        assert_eq!(sync_take_whole(SyncKind::Index, SOUNDS.to_vec()).outcome, SyncOutcome::Failed, "the sounds are not an index");
    }

    #[test]
    fn bytes_of_a_cratarr_older_than_this_app_fail_and_are_tried_again_never_stopping() {
        let older = |bytes: &[u8]| {
            let mut out = bytes.to_vec();
            out[4] = 0;
            out
        };
        assert_eq!(sync_take_whole(SyncKind::Index, older(WHOLE)).outcome, SyncOutcome::Failed);
        assert_eq!(sync_take_whole(SyncKind::Sounds, older(SOUNDS)).outcome, SyncOutcome::Failed);
    }

    fn covers_head(version: u32, rev: i64) -> Vec<u8> {
        [&b"CRXV"[..], &version.to_le_bytes(), &rev.to_le_bytes()].concat()
    }

    #[test]
    fn covers_are_taken_by_their_head_and_only_a_whole_can_stop() {
        let at = |rev| SyncTaken { outcome: SyncOutcome::Took, rev: Some(rev), bytes: None };
        assert_eq!(sync_take_whole(SyncKind::Covers, covers_head(COVERS_VERSION, 41)), at(41));
        assert_eq!(sync_take_changes(SyncKind::Covers, Vec::new(), covers_head(COVERS_VERSION, 42)), at(42));
        assert_eq!(sync_take_whole(SyncKind::Covers, covers_head(9, 41)).outcome, SyncOutcome::Version);
        assert_eq!(sync_take_changes(SyncKind::Covers, Vec::new(), covers_head(9, 41)).outcome, SyncOutcome::StartAgain);
        assert_eq!(sync_take_whole(SyncKind::Covers, Vec::new()).outcome, SyncOutcome::Failed, "cut short");
        assert_eq!(sync_take_changes(SyncKind::Covers, Vec::new(), Vec::new()).outcome, SyncOutcome::StartAgain);
    }

    #[test]
    fn a_feed_applied_leaves_what_a_fresh_fetch_would_to_the_byte() {
        let index = sync_take_changes(SyncKind::Index, WHOLE.to_vec(), CHANGES.to_vec());
        assert_eq!(index, SyncTaken { outcome: SyncOutcome::Took, rev: Some(9), bytes: Some(AFTER.to_vec()) });
        let sounds = sync_take_changes(SyncKind::Sounds, SOUNDS.to_vec(), SOUND_CHANGES.to_vec());
        assert_eq!(sounds, SyncTaken { outcome: SyncOutcome::Took, rev: Some(9), bytes: Some(SOUNDS_AFTER.to_vec()) });
    }

    #[test]
    fn whatever_cannot_go_on_starts_again_and_only_a_whole_can_stop_it() {
        let gone = SyncTaken { outcome: SyncOutcome::StartAgain, rev: None, bytes: None };
        assert_eq!(sync_take_changes(SyncKind::Index, another_version(WHOLE), CHANGES.to_vec()), gone, "what is kept, of another version");
        assert_eq!(sync_take_changes(SyncKind::Index, WHOLE.to_vec(), another_version(CHANGES)), gone, "a feed of another version");
        assert_eq!(sync_take_changes(SyncKind::Index, WHOLE.to_vec(), CHANGES[..50].to_vec()), gone, "a feed cut short");
        assert_eq!(sync_take_changes(SyncKind::Index, AFTER.to_vec(), CHANGES.to_vec()), gone, "a feed from where it does not stand");
        assert_eq!(sync_take_changes(SyncKind::Sounds, another_version(SOUNDS), SOUND_CHANGES.to_vec()), gone);
    }
}
