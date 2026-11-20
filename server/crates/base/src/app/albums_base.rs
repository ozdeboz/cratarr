//! What a Want's copy of a record the library holds came to, below the requests that read it (moved from `albums`, which keeps
//! its name).

/// What a Want's copy of a record the library holds came to (T5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WantCopy {
    /// Made in the asker's own music.
    Made,
    /// The asker has no own music, or a copy of it is in theirs already: nothing to say.
    Nothing,
    /// No copy of it is known whole (S.1): nothing copied; the Want goes to the fetchers.
    NoneWhole,
    /// Below the floor (AL03.1), asked for a copy that shares its blocks alone, it would not: nothing copied; the Want waits for space.
    NoSpace,
    /// Not made, and why.
    Failed(String),
}
