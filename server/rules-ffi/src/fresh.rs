//! A device's one fresh start at 0.16 (`cratarr_rules::fresh`, decision 0101 point 23), in UniFFI's types: the app hands in facts
//! and does what it is told.

use cratarr_rules::fresh::{self, Facts, Step};

/// What the device knows as it looks (`fresh::Facts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct FreshFacts {
    pub marked: Option<u32>,
    pub collected: bool,
    pub cleared: bool,
    pub kept_readable: bool,
    pub waiting_here: bool,
    pub reached: bool,
    pub may_download: bool,
}

/// What to do now (`fresh::Step`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FreshStep {
    Nothing,
    Collect,
    Clear,
    Mark,
    Fetch,
}

/// What to do now, by an app of `version` (its `versionCode`).
#[uniffi::export]
pub fn fresh_step(facts: FreshFacts, version: u32) -> FreshStep {
    let f = Facts {
        marked: facts.marked,
        collected: facts.collected,
        cleared: facts.cleared,
        kept_readable: facts.kept_readable,
        waiting_here: facts.waiting_here,
        reached: facts.reached,
        may_download: facts.may_download,
    };
    match fresh::step(&f, version) {
        Step::Nothing => FreshStep::Nothing,
        Step::Collect => FreshStep::Collect,
        Step::Clear => FreshStep::Clear,
        Step::Mark => FreshStep::Mark,
        Step::Fetch => FreshStep::Fetch,
    }
}

/// The app's version that brings it.
#[uniffi::export]
pub fn fresh_at() -> u32 {
    fresh::FRESH_AT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_goes_through_as_it_is() {
        let facts = FreshFacts { marked: None, collected: false, cleared: false, kept_readable: true, waiting_here: false, reached: true, may_download: true };
        assert_eq!(fresh_step(facts, fresh_at()), FreshStep::Collect);
        assert_eq!(fresh_step(FreshFacts { marked: Some(fresh_at()), waiting_here: true, ..facts }, fresh_at()), FreshStep::Fetch);
        assert_eq!(fresh_step(facts, fresh_at() - 1), FreshStep::Nothing);
    }
}
