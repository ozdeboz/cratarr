//! When a device fetches what of its index (0101's points 16 to 18, audit/FINAMP-OFFLINE.md's A2 and A3): one rule for the page and
//! for the app's worker, so neither holds a decision of its own. A part of what a device keeps (the index, the sounds) is told how
//! it stands, and says what to do; the fetching and the keeping are the caller's, and the time is handed in.
//!
//! - **Whole once, on Wi-Fi, on mobile data after asking. Then changes on any connection:** they are small, and a device's copy is
//!   what it reads with Cratarr away.
//! - **A feed is asked when told, never on a timer:** rows' and sounds' each make a whole build on the server. `/api/changes` says when
//!   they moved. Covers' builds nothing, and asking it is what sets the maker going, so it is told at every sync.
//! - **Not in the first seconds** (A3): a device's own first reads go first, and hours in the background count as a start.
//! - **A lost connection is no failure** (A2): it waits and counts for nothing. Only a real answer can fail, and failing a few times
//!   rests until the next start. Lost a few times in a row, the device may say it is paused, once.
//! - **Another version in bytes just fetched stops:** the app and Cratarr differ, and fetching again will not mend it.
//! - **A start over what another version kept begins whole:** its stop was its own to say, and this one may not read its bytes, though a
//!   feed would have nothing to say of them.
//! - **An account's own has no feed** (`Part::step_stale`): it moves with every play and is worked out whole, each fetch costing the
//!   server the account's taste. Fetched whole when it has stood a while: on a free network every few hours; on mobile data once a
//!   day, and only for one who said yes to the library on mobile data. It asks no question of its own: a device holding an older
//!   one still answers, as of then.
//! - **What an answer and what bytes came to are said here too** (`ended`, `Outcome::of_whole`, `Outcome::of_changes`), or each device
//!   would decide them for itself.

use super::Refused;

/// The form of a bundle of small covers, and of their feed: raised by anything a reader must know (the server's `device_covers`).
pub const COVERS_VERSION: u32 = 1;
/// How much of them says what they are: `CRXV`, the version, the rev. Their frames are plain, and a device reads them as they come.
pub const COVERS_HEAD: usize = 16;

/// Where covers stand, from the head of their bytes, refused as an index would be.
pub fn covers_rev(head: &[u8]) -> Result<i64, Refused> {
    let word = |at: usize| head.get(at..at + 4).and_then(|b| <[u8; 4]>::try_from(b).ok());
    if head.len() < COVERS_HEAD || word(0) != Some(*b"CRXV") {
        return Err(Refused::Damaged);
    }
    let found = word(4).map_or(0, u32::from_le_bytes);
    if found != COVERS_VERSION {
        return Err(Refused::Version { found, wants: COVERS_VERSION });
    }
    head.get(8..16).and_then(|b| <[u8; 8]>::try_from(b).ok()).map(i64::from_le_bytes).ok_or(Refused::Damaged)
}

/// How long after a start the first fetch waits.
pub const FIRST_AFTER_MS: i64 = 10_000;
/// Hidden this long, a device coming back has started again.
pub const ANEW_AFTER_MS: i64 = 5 * 60 * 60 * 1000;
/// A lost connection looks again after this.
pub const LOST_AGAIN_MS: i64 = 30_000;
/// Lost this many times in a row, it is paused.
pub const PAUSED_AFTER: u32 = 3;
/// A failure tries again after this, doubled each time.
pub const FAILED_AGAIN_MS: i64 = 60_000;
/// Failed this many times, it rests until the next start.
pub const FAILED_MOST: u32 = 5;

/// How long an account's own stands before it is fetched again, on a free network and on mobile data.
pub const OWN_EVERY_MS: i64 = 6 * 60 * 60 * 1000;
pub const OWN_EVERY_METERED_MS: i64 = 24 * 60 * 60 * 1000;

/// What a fetch costs the person, as the device can tell. One that cannot tell (most browsers) counts as free.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Network {
    #[default]
    Free,
    Metered,
    Off,
}

/// What the person said of a whole fetch on mobile data.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mobile {
    #[default]
    Unasked,
    Yes,
    No,
}

/// What to do now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Stopped for good on this version.
    Stopped,
    /// Nothing, until something changes: a start, the network, the person's answer.
    Rest,
    /// Nothing before this time.
    Wait(i64),
    /// Ask whether to fetch it whole on mobile data.
    Ask,
    /// Fetch it whole.
    Whole,
    /// Fetch what changed.
    Changes,
}

/// How a fetch ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Taken, or nothing had changed.
    Took,
    /// The connection went, or Cratarr was out of reach.
    Lost,
    /// A real answer, and no good one.
    Failed,
    /// "Start again", or what was held cannot go on: only a whole fetch will do.
    StartAgain,
    /// Bytes just fetched whole are of a version newer than this device reads: only the device's own update mends that.
    Version,
}

impl Outcome {
    /// What bytes just fetched whole came to. Only these can say the two differ for good, and only where Cratarr is the newer:
    /// a device newer than Cratarr is mended by Cratarr's own update, with nothing done here, so it fails as a bad answer does and
    /// is tried again, never stopped (audit/OFFLINE-REVIEW.md, finding 2). Anything else refused is a bad answer.
    pub fn of_whole<T>(taken: &Result<T, Refused>) -> Self {
        match taken {
            Ok(_) => Self::Took,
            Err(Refused::Version { found, wants }) if found > wants => Self::Version,
            Err(_) => Self::Failed,
        }
    }

    /// What a feed came to: whatever cannot go on, of whichever version, is fetched whole.
    pub fn of_changes<T>(taken: &Result<T, Refused>) -> Self {
        if taken.is_ok() { Self::Took } else { Self::StartAgain }
    }
}

/// How an answer ended a fetch, or `None` where it brought bytes to take. `status`: none where no answer came, `whole`: what was
/// asked for. Nothing changed and "start again" are a feed's answers, and a whole has nothing less to be asked for.
pub fn ended(status: Option<u16>, whole: bool) -> Option<Outcome> {
    match status {
        // No answer of Cratarr's, or none it can give this session: the connection's, and the sign-in's own to say.
        None | Some(401 | 403 | 502 | 503 | 504) => Some(Outcome::Lost),
        Some(204) => Some(if whole { Outcome::Failed } else { Outcome::Took }),
        Some(410) => Some(if whole { Outcome::Failed } else { Outcome::StartAgain }),
        Some(200..=299) => None,
        Some(_) => Some(Outcome::Failed),
    }
}

/// How one part stands. Kept by the caller, as it must outlast a start.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Part {
    /// Nothing is held, or the server said start again.
    pub whole: bool,
    pub stopped: bool,
    /// Lost connections in a row.
    pub lost: u32,
    /// Real failures in a row.
    pub failed: u32,
    /// When the last fetch ended.
    pub tried: Option<i64>,
    /// When this start began.
    pub started: i64,
}

impl Part {
    /// When it may look again after what went wrong.
    fn again(&self) -> Option<i64> {
        let tried = self.tried?;
        match (self.failed, self.lost) {
            (0, 0) => None,
            (0, _) => Some(tried + LOST_AGAIN_MS),
            (failed, _) => Some(tried + (FAILED_AGAIN_MS << (failed - 1).min(16))),
        }
    }

    /// What to do now. `told`: its feed may be asked, as the log said it moved, or as it builds nothing.
    pub fn step(&self, network: Network, mobile: Mobile, told: bool, now: i64) -> Step {
        let first = self.started + FIRST_AFTER_MS;
        if self.stopped {
            return Step::Stopped;
        }
        if network == Network::Off || self.failed >= FAILED_MOST {
            return Step::Rest;
        }
        if now < first {
            return Step::Wait(first);
        }
        if let Some(again) = self.again().filter(|again| now < *again) {
            return Step::Wait(again);
        }
        if !self.whole {
            return if told { Step::Changes } else { Step::Rest };
        }
        match (network, mobile) {
            (Network::Metered, Mobile::Unasked) => Step::Ask,
            (Network::Metered, Mobile::No) => Step::Rest,
            _ => Step::Whole,
        }
    }

    /// What to do now with what has no feed and goes stale: an account's own. `took`: when it was last taken.
    pub fn step_stale(&self, network: Network, mobile: Mobile, took: Option<i64>, now: i64) -> Step {
        let first = self.started + FIRST_AFTER_MS;
        if self.stopped {
            return Step::Stopped;
        }
        if network == Network::Off || self.failed >= FAILED_MOST {
            return Step::Rest;
        }
        if now < first {
            return Step::Wait(first);
        }
        if let Some(again) = self.again().filter(|again| now < *again) {
            return Step::Wait(again);
        }
        let every = if network == Network::Metered { OWN_EVERY_METERED_MS } else { OWN_EVERY_MS };
        let due = self.whole || took.is_none_or(|took| now - took >= every);
        if due && (network == Network::Free || mobile == Mobile::Yes) { Step::Whole } else { Step::Rest }
    }

    /// How it stands after a fetch.
    pub fn after(self, outcome: Outcome, now: i64) -> Self {
        let tried = Some(now);
        match outcome {
            Outcome::Took => Self { whole: false, lost: 0, failed: 0, tried, ..self },
            Outcome::Lost => Self { lost: self.lost + 1, tried, ..self },
            Outcome::Failed => Self { lost: 0, failed: self.failed + 1, tried, ..self },
            Outcome::StartAgain => Self { whole: true, lost: 0, tried, ..self },
            Outcome::Version => Self { stopped: true, whole: true, lost: 0, tried, ..self },
        }
    }

    /// A start: what went wrong is forgotten, and a stop stands.
    pub fn anew(self, now: i64) -> Self {
        Self { lost: 0, failed: 0, tried: None, started: now, ..self }
    }

    /// A start over what a reader of another version kept. The caller knows, by keeping the version with each write.
    pub fn anew_by_another(self, now: i64) -> Self {
        Self { whole: true, stopped: false, ..self.anew(now) }
    }

    /// Whether it should say it is paused: the connection keeps going.
    pub fn paused(&self) -> bool {
        self.lost >= PAUSED_AFTER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: i64 = 1_000_000;
    const LATER: i64 = START + FIRST_AFTER_MS;

    fn whole() -> Part {
        Part { whole: true, started: START, ..Part::default() }
    }

    fn held() -> Part {
        Part { started: START, ..Part::default() }
    }

    #[test]
    fn an_accounts_own_goes_whole_on_a_free_network_when_it_has_stood_a_while() {
        let took = LATER - OWN_EVERY_MS;
        assert_eq!(held().step_stale(Network::Free, Mobile::Unasked, None, LATER), Step::Whole, "never taken");
        assert_eq!(held().step_stale(Network::Free, Mobile::No, Some(took), LATER), Step::Whole);
        assert_eq!(held().step_stale(Network::Free, Mobile::Unasked, Some(took + 1), LATER), Step::Rest, "not yet");
        assert_eq!(whole().step_stale(Network::Free, Mobile::Unasked, Some(took + 1), LATER), Step::Whole, "nothing held");
    }

    #[test]
    fn on_mobile_data_an_accounts_own_goes_once_a_day_for_one_who_said_yes_and_asks_nothing() {
        let took = LATER - OWN_EVERY_METERED_MS;
        assert_eq!(held().step_stale(Network::Metered, Mobile::Yes, Some(took), LATER), Step::Whole);
        assert_eq!(held().step_stale(Network::Metered, Mobile::Yes, Some(took + 1), LATER), Step::Rest);
        assert_eq!(held().step_stale(Network::Metered, Mobile::Unasked, None, LATER), Step::Rest, "no question of its own");
        assert_eq!(held().step_stale(Network::Metered, Mobile::No, None, LATER), Step::Rest);
    }

    #[test]
    fn an_accounts_own_waits_and_stops_as_any_part_does() {
        assert_eq!(held().step_stale(Network::Free, Mobile::Yes, None, START), Step::Wait(LATER), "its first seconds");
        assert_eq!(held().step_stale(Network::Off, Mobile::Yes, None, LATER), Step::Rest);
        assert_eq!(Part { stopped: true, ..held() }.step_stale(Network::Free, Mobile::Yes, None, LATER), Step::Stopped);
        assert_eq!(Part { failed: FAILED_MOST, ..held() }.step_stale(Network::Free, Mobile::Yes, None, LATER), Step::Rest);
        let lost = held().after(Outcome::Lost, LATER);
        assert_eq!(lost.step_stale(Network::Free, Mobile::Yes, None, LATER), Step::Wait(LATER + LOST_AGAIN_MS));
    }

    #[test]
    fn nothing_is_fetched_in_the_first_seconds_of_a_start() {
        assert_eq!(whole().step(Network::Free, Mobile::Unasked, true, START), Step::Wait(LATER));
        assert_eq!(held().step(Network::Free, Mobile::Unasked, true, LATER - 1), Step::Wait(LATER));
        assert_eq!(held().step(Network::Free, Mobile::Unasked, true, LATER), Step::Changes);
    }

    #[test]
    fn whole_goes_on_wifi_and_asks_first_on_mobile() {
        assert_eq!(whole().step(Network::Free, Mobile::Unasked, true, LATER), Step::Whole);
        assert_eq!(whole().step(Network::Metered, Mobile::Unasked, true, LATER), Step::Ask);
        assert_eq!(whole().step(Network::Metered, Mobile::Yes, true, LATER), Step::Whole);
    }

    #[test]
    fn a_no_to_mobile_data_rests_until_wifi_and_no_more_asking() {
        assert_eq!(whole().step(Network::Metered, Mobile::No, true, LATER), Step::Rest);
        assert_eq!(whole().step(Network::Free, Mobile::No, true, LATER), Step::Whole);
    }

    #[test]
    fn changes_go_on_any_connection_with_nothing_asked() {
        for mobile in [Mobile::Unasked, Mobile::Yes, Mobile::No] {
            assert_eq!(held().step(Network::Metered, mobile, true, LATER), Step::Changes);
            assert_eq!(held().step(Network::Free, mobile, true, LATER), Step::Changes);
        }
    }

    #[test]
    fn a_feed_is_asked_only_when_told_as_it_is_a_whole_build() {
        assert_eq!(held().step(Network::Free, Mobile::Yes, false, LATER), Step::Rest);
        assert_eq!(held().step(Network::Free, Mobile::Yes, true, LATER), Step::Changes);
    }

    #[test]
    fn a_whole_owed_needs_no_telling() {
        assert_eq!(whole().step(Network::Free, Mobile::Unasked, false, LATER), Step::Whole);
        assert_eq!(whole().step(Network::Metered, Mobile::Unasked, false, LATER), Step::Ask);
    }

    #[test]
    fn what_stands_in_the_way_of_a_feed_stands_told_or_not() {
        assert_eq!(held().step(Network::Free, Mobile::Yes, false, START), Step::Wait(LATER));
        assert_eq!(held().after(Outcome::Lost, LATER).step(Network::Free, Mobile::Yes, false, LATER), Step::Wait(LATER + LOST_AGAIN_MS));
        assert_eq!(held().step(Network::Off, Mobile::Yes, true, LATER), Step::Rest);
    }

    fn head(version: u32, rev: i64) -> Vec<u8> {
        [&b"CRXV"[..], &version.to_le_bytes(), &rev.to_le_bytes()].concat()
    }

    #[test]
    fn covers_say_where_they_stand_in_their_head() {
        assert_eq!(covers_rev(&head(COVERS_VERSION, 41)), Ok(41));
        assert_eq!(head(COVERS_VERSION, 41).len(), COVERS_HEAD);
    }

    #[test]
    fn covers_of_another_version_are_refused_by_name() {
        assert_eq!(covers_rev(&head(9, 41)), Err(Refused::Version { found: 9, wants: COVERS_VERSION }));
    }

    #[test]
    fn what_is_not_covers_or_is_cut_short_is_damaged() {
        assert_eq!(covers_rev(&[]), Err(Refused::Damaged));
        assert_eq!(covers_rev(&head(COVERS_VERSION, 41)[..15]), Err(Refused::Damaged));
        assert_eq!(covers_rev(&[&b"CRXI"[..], &head(COVERS_VERSION, 41)[4..]].concat()), Err(Refused::Damaged));
    }

    #[test]
    fn no_network_rests_until_there_is_one() {
        assert_eq!(whole().step(Network::Off, Mobile::Yes, true, LATER), Step::Rest);
        assert_eq!(held().step(Network::Off, Mobile::Yes, true, LATER), Step::Rest);
    }

    #[test]
    fn a_lost_connection_is_no_failure_however_often() {
        let mut s = held();
        for n in 1..=(FAILED_MOST + 3) {
            s = s.after(Outcome::Lost, LATER);
            assert_eq!(s.failed, 0);
            assert_eq!(s.lost, n);
        }
        assert_eq!(s.step(Network::Free, Mobile::Unasked, true, LATER), Step::Wait(LATER + LOST_AGAIN_MS));
        assert_eq!(s.step(Network::Free, Mobile::Unasked, true, LATER + LOST_AGAIN_MS), Step::Changes, "it goes on looking");
    }

    #[test]
    fn paused_is_said_after_a_few_lost_in_a_row_and_a_real_answer_ends_it() {
        let once = held().after(Outcome::Lost, LATER);
        assert!(!once.paused());
        let thrice = once.after(Outcome::Lost, LATER).after(Outcome::Lost, LATER);
        assert!(thrice.paused());
        assert!(!thrice.after(Outcome::Took, LATER).paused());
        assert!(!thrice.after(Outcome::Failed, LATER).paused());
    }

    #[test]
    fn a_real_failure_counts_waits_longer_each_time_and_rests_at_the_most() {
        let once = held().after(Outcome::Failed, LATER);
        assert_eq!(once.step(Network::Free, Mobile::Unasked, true, LATER), Step::Wait(LATER + FAILED_AGAIN_MS));
        let twice = once.after(Outcome::Failed, LATER);
        assert_eq!(twice.step(Network::Free, Mobile::Unasked, true, LATER), Step::Wait(LATER + 2 * FAILED_AGAIN_MS));
        let mut s = held();
        for _ in 0..FAILED_MOST {
            s = s.after(Outcome::Failed, LATER);
        }
        assert_eq!(s.step(Network::Free, Mobile::Unasked, true, LATER + 10 * ANEW_AFTER_MS), Step::Rest);
    }

    #[test]
    fn a_lost_connection_leaves_the_failures_counted_as_they_were() {
        let s = held().after(Outcome::Failed, LATER).after(Outcome::Lost, LATER);
        assert_eq!(s.failed, 1);
    }

    #[test]
    fn a_fetch_taken_ends_the_whole_and_what_went_wrong() {
        let s = whole().after(Outcome::Failed, LATER).after(Outcome::Took, LATER);
        assert_eq!(s, Part { tried: Some(LATER), started: START, ..Part::default() });
    }

    #[test]
    fn start_again_asks_for_it_whole_at_once_and_as_any_whole_is_asked() {
        let s = held().after(Outcome::StartAgain, LATER);
        assert!(s.whole);
        assert_eq!(s.step(Network::Free, Mobile::Unasked, true, LATER), Step::Whole);
        assert_eq!(s.step(Network::Metered, Mobile::Unasked, true, LATER), Step::Ask);
    }

    #[test]
    fn another_version_in_what_was_just_fetched_stops_and_a_start_does_not_undo_it() {
        let s = whole().after(Outcome::Version, LATER);
        assert_eq!(s.step(Network::Free, Mobile::Yes, true, LATER + 1), Step::Stopped);
        assert_eq!(s.anew(LATER + ANEW_AFTER_MS).step(Network::Free, Mobile::Yes, true, LATER + 2 * ANEW_AFTER_MS), Step::Stopped);
    }

    #[test]
    fn an_answer_with_nothing_of_cratarrs_in_it_is_a_lost_connection_whatever_was_asked() {
        for status in [None, Some(401), Some(403), Some(502), Some(503), Some(504)] {
            assert_eq!(ended(status, true), Some(Outcome::Lost), "{status:?}");
            assert_eq!(ended(status, false), Some(Outcome::Lost), "{status:?}");
        }
    }

    #[test]
    fn nothing_changed_and_start_again_are_a_feeds_answers_and_a_whole_fails_on_them() {
        assert_eq!(ended(Some(204), false), Some(Outcome::Took));
        assert_eq!(ended(Some(410), false), Some(Outcome::StartAgain));
        assert_eq!(ended(Some(204), true), Some(Outcome::Failed));
        assert_eq!(ended(Some(410), true), Some(Outcome::Failed));
    }

    #[test]
    fn a_real_answer_brings_bytes_or_fails() {
        assert_eq!(ended(Some(200), true), None);
        assert_eq!(ended(Some(200), false), None);
        assert_eq!(ended(Some(206), true), None);
        for status in [300, 400, 404, 500, 501] {
            assert_eq!(ended(Some(status), false), Some(Outcome::Failed), "{status}");
        }
    }

    #[test]
    fn only_another_version_in_bytes_fetched_whole_stops() {
        assert_eq!(Outcome::of_whole(&Ok(())), Outcome::Took);
        assert_eq!(Outcome::of_whole::<()>(&Err(Refused::Version { found: 2, wants: 1 })), Outcome::Version);
        assert_eq!(Outcome::of_whole::<()>(&Err(Refused::Damaged)), Outcome::Failed);
        assert_eq!(Outcome::of_whole::<()>(&Err(Refused::Rev { at: 1, from: 2 })), Outcome::Failed);
        assert_eq!(Outcome::of_whole::<()>(&Err(Refused::Unknown { what: "track", id: 1 })), Outcome::Failed);
    }

    #[test]
    fn a_device_newer_than_cratarr_tries_again_and_one_older_waits_for_its_update() {
        // Cratarr older than this device (it sends a version below the one read here): no stop, as Cratarr's own update mends
        // it with nothing done on the device. Tried again as a failure is, and at every start.
        let newer = Outcome::of_whole::<()>(&Err(Refused::Version { found: 1, wants: 2 }));
        assert_eq!(newer, Outcome::Failed);
        let s = whole().after(newer, LATER);
        assert!(!s.stopped);
        assert_eq!(s.step(Network::Free, Mobile::Yes, false, LATER + FAILED_AGAIN_MS), Step::Whole);
        assert_eq!(s.anew(LATER + 1).step(Network::Free, Mobile::Yes, false, LATER + 1 + FIRST_AFTER_MS), Step::Whole);
        // This device older than Cratarr: only its own update mends it, so it stops until then.
        assert_eq!(Outcome::of_whole::<()>(&Err(Refused::Version { found: 2, wants: 1 })), Outcome::Version);
    }

    #[test]
    fn a_feed_refused_any_way_starts_again() {
        assert_eq!(Outcome::of_changes(&Ok(())), Outcome::Took);
        assert_eq!(Outcome::of_changes::<()>(&Err(Refused::Version { found: 2, wants: 1 })), Outcome::StartAgain);
        assert_eq!(Outcome::of_changes::<()>(&Err(Refused::Damaged)), Outcome::StartAgain);
        assert_eq!(Outcome::of_changes::<()>(&Err(Refused::Rev { at: 1, from: 2 })), Outcome::StartAgain);
        assert_eq!(Outcome::of_changes::<()>(&Err(Refused::Unknown { what: "track", id: 1 })), Outcome::StartAgain);
    }

    #[test]
    fn a_start_over_another_versions_stop_tries_again() {
        let s = whole().after(Outcome::Version, LATER).anew_by_another(LATER + 1);
        assert_eq!(s.step(Network::Free, Mobile::Yes, true, LATER + 1 + FIRST_AFTER_MS), Step::Whole);
    }

    #[test]
    fn a_start_over_another_versions_copy_comes_whole_as_any_whole_is_asked() {
        let s = held().anew_by_another(LATER + 1);
        assert_eq!(s.step(Network::Free, Mobile::Unasked, true, LATER + 1 + FIRST_AFTER_MS), Step::Whole, "not by the feed");
        assert_eq!(s.step(Network::Metered, Mobile::Unasked, true, LATER + 1 + FIRST_AFTER_MS), Step::Ask);
    }

    #[test]
    fn a_start_over_another_versions_forgets_what_went_wrong_and_keeps_its_first_seconds() {
        let s = held().after(Outcome::Failed, LATER).after(Outcome::Lost, LATER).anew_by_another(LATER + 1);
        assert_eq!((s.failed, s.lost, s.tried), (0, 0, None));
        assert_eq!(s.step(Network::Free, Mobile::Unasked, true, LATER + 1), Step::Wait(LATER + 1 + FIRST_AFTER_MS));
    }

    #[test]
    fn a_start_forgets_what_went_wrong_and_waits_its_first_seconds_again() {
        let mut s = held();
        for _ in 0..FAILED_MOST {
            s = s.after(Outcome::Failed, LATER);
        }
        let again = s.anew(LATER + 1);
        assert_eq!(again.step(Network::Free, Mobile::Unasked, true, LATER + 1), Step::Wait(LATER + 1 + FIRST_AFTER_MS));
        assert_eq!(again.step(Network::Free, Mobile::Unasked, true, LATER + 1 + FIRST_AFTER_MS), Step::Changes);
    }
}
