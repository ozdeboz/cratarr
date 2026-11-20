//! What a device's index was last settled at, below the app (moved from `app::device_index`, which keeps its name): the
//! server holds it, one index made at a time (`Serve::device_turn`).

use std::sync::{Arc, Weak};

use cratarr_rules::index;

use crate::crates::Facts;

/// What the log was last brought up to: all that can move a row without moving the log. The reading only weakly, so as not to
/// hold on to it, and so its address stays taken: a reading made in the same place would pass for it (`Catalogue::crate_facts`).
#[derive(Debug, Default)]
pub struct Settled {
    pub facts: Weak<Vec<Facts>>,
    pub built: i64,
    pub rev: i64,
    pub libraries: Vec<index::Library>,
}

impl Settled {
    /// Whether what was settled is what stands now: no row can differ, and nothing needs building.
    pub fn holds(&self, facts: &Arc<Vec<Facts>>, built: i64, rev: i64, libraries: &[index::Library]) -> bool {
        self.rev == rev && self.stands(facts, built, libraries)
    }

    /// `holds`, but for the log: the reading, the build and the libraries, which move rows with no word in it.
    pub fn stands(&self, facts: &Arc<Vec<Facts>>, built: i64, libraries: &[index::Library]) -> bool {
        std::ptr::eq(self.facts.as_ptr(), Arc::as_ptr(facts)) && self.built == built && self.libraries == libraries
    }
}
