//! What an id says by its own shape (decision 0030): a type prefix, so a bare id says what it is, and a song's track.
//! How the library makes its albums' and artists' ids is the server's (`serve::ids`): they need md5, and a tag row.

pub const SONG: &str = "tr-";
pub const ALBUM: &str = "al-";
pub const ARTIST: &str = "ar-";

/// A song's id from its Cratarr track id.
pub fn song(track: i64) -> String {
    format!("{SONG}{track}")
}

/// The Cratarr track id in a song id.
pub fn track_of(id: &str) -> Option<i64> {
    id.strip_prefix(SONG)?.parse().ok()
}
