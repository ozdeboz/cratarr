//! The job kinds the runner knows (C.8's carriers): what each is called where it has no step yet, whether a restart lets it go on
//! from its own checkpoints, and the rough peak of memory it declares (OQ-RN-1 (a), measured with the tests' stand-ins:
//! RN-REPORT.md). A kind not listed is a person's long work with no checkpoints, nothing declared.

/// A kind's facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    /// Its name, or the first part of it (`covers:<album>`, `renumber:<album>`).
    pub kind: &'static str,
    /// What Activity calls it before its first step.
    pub named: &'static str,
    /// A restart lets it go on from its own checkpoints (its rows, its own reading of what it did): the old job ends cancelled
    /// and a new one begins, rather than failing with "It stopped before it was done.".
    pub resumes: bool,
    /// Its rough peak of memory, MB, as measured (OQ-RN-1): what reading (a) counts against the budget.
    pub peak_mb: u64,
    /// It decodes: under either reading `memwatch::room` is its last check.
    pub decodes: bool,
}

const fn k(kind: &'static str, named: &'static str, resumes: bool, peak_mb: u64, decodes: bool) -> Kind {
    Kind { kind, named, resumes, peak_mb, decodes }
}

/// Every kind (the peaks: RN-REPORT.md's measurements, rounded up).
pub const KINDS: [Kind; 16] = [
    k("import:lastfm", "Reading scrobbles", false, 120, false),
    k("import:listenbrainz", "Reading listens", false, 120, false),
    k("import:file", "Reading the file", false, 120, false),
    k("covers", "Looking for covers", false, 60, false),
    k("discover", "Looking for music", false, 60, false),
    k("copy", "Copying", false, 40, false),
    k("replace", "Replacing", false, 40, false),
    k("intake", "Hearing", false, 500, true),
    k("matching", "Matching", false, 60, false),
    k("renumber", "Renumbering", false, 40, false),
    k("analysis", "Analysis", true, 500, true),
    k("soulseek:check", "Checking", true, 500, true),
    k("soulseek:place", "Placing", true, 40, false),
    k("discogs:type", "Typing records", true, 40, false),
    k("references:lists", "Reading MusicBrainz lists", true, 40, false),
    k("decode", "Decoding", false, 500, true),
];

/// A kind's facts, by its name or its first part; a person's work with nothing declared where it is none of these.
pub fn of(kind: &str) -> Kind {
    let head = kind.split(':').next().unwrap_or(kind);
    KINDS
        .iter()
        .find(|k| k.kind == kind)
        .or_else(|| KINDS.iter().find(|k| k.kind == head))
        .copied()
        .unwrap_or(Kind { kind: "", named: "Working", resumes: false, peak_mb: 0, decodes: false })
}

/// What Activity calls a job before its first step.
pub fn named(kind: &str) -> String {
    of(kind).named.to_string()
}
