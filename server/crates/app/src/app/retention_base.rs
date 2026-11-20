//! What a record or an artist forgotten answers (RT01, RT01b), below requests (moved from `app::retention`, which keeps
//! their names and forgets): playing's views answer it too.

use super::ToggleError;
use crate::catalogue::Catalogue;

/// What an album asked for by its id answers once its record was forgotten (RT01).
pub const NO_LONGER_KNOWN: &str = "This album is no longer known: nobody had opened it or seen it in a list for six months.";
/// What an artist asked for by their id answers once forgotten (RT01b).
pub const ARTIST_NO_LONGER_KNOWN: &str = "This artist is no longer known.";

/// Whether `subject` was forgotten (RT01): its tombstone. One that cannot be read is not, and is answered as before.
fn forgotten(cat: &Catalogue, subject: &str) -> bool {
    match cat.forgotten_at(subject) {
        Ok(at) => at.is_some(),
        Err(e) => {
            tracing::debug!(target: "library", subject, "whether it was forgotten could not be read: {e:#}");
            false
        }
    }
}

/// What Cratarr's album `id` answers where it cannot be read: a record forgotten is no longer known (410), by its tombstone;
/// any other, None, answered as before. Blocking.
pub fn unknown_album(cat: &Catalogue, id: &str) -> Option<ToggleError> {
    forgotten(cat, id).then(|| ToggleError::Gone(NO_LONGER_KNOWN.into()))
}

/// What Cratarr's artist `id` (`ar-<n>`) answers where nothing is known of them: one forgotten is no longer known (410), by
/// their tombstone (RT01b); any other, None, answered as before. Blocking.
pub fn unknown_artist(cat: &Catalogue, id: &str) -> Option<ToggleError> {
    forgotten(cat, id).then(|| ToggleError::Gone(ARTIST_NO_LONGER_KNOWN.into()))
}
