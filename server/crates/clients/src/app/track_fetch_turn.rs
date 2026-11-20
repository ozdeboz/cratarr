//! What the turn reads of track_fetch.rs, moved down from the app (its failure class).

/// The gate stopped short of a verdict: AcoustID could not be asked (away, over its limit, its key gone or never set).
/// That says nothing of the file, so it is no refusal: what fetched the file on its own must not treat it as one
/// (`soulseek::check`, `track_intake::fetch`). Reads as the failure it carries.
#[derive(Debug)]
pub struct NotAsked(pub String);

impl std::fmt::Display for NotAsked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NotAsked {}

/// What the gate found wrong with a file itself (TN.1): frames that would not decode, a length not the recording's, a flaw heard, or
/// AcoustID knowing it as another recording. Reads as why.
#[derive(Debug)]
pub struct Unfit(pub String);

impl std::fmt::Display for Unfit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Unfit {}
