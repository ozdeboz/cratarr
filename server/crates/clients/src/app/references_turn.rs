//! What the turn reads of references.rs, moved down from the app (its failure class).

/// A source switched off: no answer about any record, and none against it (R23).
#[derive(Debug)]
pub struct SwitchedOff(pub &'static str);

impl std::fmt::Display for SwitchedOff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is switched off", self.0)
    }
}

impl std::error::Error for SwitchedOff {}
