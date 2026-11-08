//! A device's one fresh start at 0.16 (decision 0101 point 23; the owner, 2026-11-08: "Build it into the app"). Every id moved at
//! the server's first start, so what a device keeps under the old ones goes once, and what it had downloaded comes again under the
//! new. In steps, each safe to stop in: what to fetch again is written down first (and never again once it is), then the stores of
//! before go, then the device says it is done. Each account's downloads come again only at its own sign-in (point 28), on the
//! network its downloads may use, and never while the kept lists cannot be read: they would be added to, not to what they were.
//! The app hands in facts and does what it is told.

/// The app's version that brings it (`versionCode`: 0.16.0).
pub const FRESH_AT: u32 = 1600;

/// What the device knows as it looks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Facts {
    /// The version whose fresh start was done here, kept once it was; none before.
    pub marked: Option<u32>,
    /// What to fetch again is written down.
    pub collected: bool,
    /// The stores of before are gone, and it is written down that they are.
    pub cleared: bool,
    /// Both kept lists read.
    pub kept_readable: bool,
    /// Something waits to be fetched again for the account signed in.
    pub waiting_here: bool,
    /// Cratarr answers.
    pub reached: bool,
    /// The downloads' own network setting lets a download go now.
    pub may_download: bool,
}

/// What to do now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Nothing,
    /// Write down what each account had downloaded and kept.
    Collect,
    /// Let the stores of before go: the downloads, the kept lists, the library kept on the device.
    Clear,
    /// Say it is done: the last step, so a stop before it does it all again.
    Mark,
    /// Fetch again what the account signed in had, under the ids it has now.
    Fetch,
}

/// What to do now, by an app of `version`.
pub fn step(f: &Facts, version: u32) -> Step {
    let due = version >= FRESH_AT && f.marked.is_none_or(|done| done < FRESH_AT);
    if due {
        // Whose each download is lies in the kept lists: unread, nothing can be written down for the right account.
        if !f.collected && !f.kept_readable {
            return Step::Nothing;
        }
        return if !f.collected {
            Step::Collect
        } else if !f.cleared {
            Step::Clear
        } else {
            Step::Mark
        };
    }
    if f.waiting_here && f.reached && f.may_download && f.kept_readable { Step::Fetch } else { Step::Nothing }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: u32 = 1503;
    const NEW: u32 = FRESH_AT;

    fn before() -> Facts {
        Facts { kept_readable: true, reached: true, may_download: true, ..Facts::default() }
    }

    #[test]
    fn the_app_that_brings_it_writes_down_then_clears_then_says_it_is_done() {
        let f = before();
        assert_eq!(step(&f, NEW), Step::Collect);
        assert_eq!(step(&Facts { collected: true, ..f }, NEW), Step::Clear);
        assert_eq!(step(&Facts { collected: true, cleared: true, ..f }, NEW), Step::Mark);
        assert_eq!(step(&Facts { marked: Some(OLD), ..f }, NEW), Step::Collect, "a mark of before is no mark");
    }

    #[test]
    fn a_stop_anywhere_goes_on_from_where_it_stood_and_never_writes_down_again() {
        let written = Facts { collected: true, ..before() };
        assert_eq!(step(&written, NEW), Step::Clear, "stopped while clearing: it clears again, with the list it has");
        assert_eq!(step(&Facts { cleared: true, ..written }, NEW), Step::Mark, "stopped before the mark: only the mark");
    }

    #[test]
    fn once_done_never_again_and_an_app_from_before_never() {
        let done = Facts { marked: Some(NEW), ..before() };
        assert_eq!(step(&done, NEW), Step::Nothing);
        assert_eq!(step(&Facts { marked: Some(NEW), ..done }, NEW + 1), Step::Nothing, "a later version does not do it again");
        assert_eq!(step(&before(), OLD), Step::Nothing, "0.15 knows nothing of it");
    }

    #[test]
    fn kept_lists_that_cannot_be_read_hold_it_back_before_anything_is_written_down() {
        let unread = Facts { kept_readable: false, ..before() };
        assert_eq!(step(&unread, NEW), Step::Nothing);
        assert_eq!(step(&Facts { collected: true, ..unread }, NEW), Step::Clear, "written down already: the list is what counts");
    }

    #[test]
    fn what_was_had_comes_again_for_the_account_signed_in_when_it_may() {
        let done = Facts { marked: Some(NEW), waiting_here: true, ..before() };
        assert_eq!(step(&done, NEW), Step::Fetch);
        assert_eq!(step(&Facts { waiting_here: false, ..done }, NEW), Step::Nothing, "another's waits for its own sign-in");
        assert_eq!(step(&Facts { reached: false, ..done }, NEW), Step::Nothing);
        assert_eq!(step(&Facts { may_download: false, ..done }, NEW), Step::Nothing, "Wi-Fi only stays Wi-Fi only");
        assert_eq!(step(&Facts { kept_readable: false, ..done }, NEW), Step::Nothing, "not added to a list that cannot be read");
    }

    #[test]
    fn nothing_is_fetched_before_the_device_is_cleared_and_marked() {
        let waiting = Facts { waiting_here: true, collected: true, ..before() };
        assert_eq!(step(&waiting, NEW), Step::Clear);
    }
}
