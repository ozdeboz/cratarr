//! What an id says by its own shape (decision 0030): a type prefix, so a bare id says what it is, and a song's track.
//! How the library makes its albums' and artists' ids is the server's (`serve::ids`): they need md5, and a tag row.

pub const SONG: &str = "tr-";
pub const ALBUM: &str = "al-";
pub const ARTIST: &str = "ar-";

/// A record's picture as every answer sends it (OBJ5's one resolver for a record and its tracks): the address a source's kept
/// picture has where one is, else the server's own cover of the record (`/api/cover/cratarr/al-<n>`), which resolves the chosen
/// cover (the files', or a source's; COU.11's store from DC1a part 3). Never worked out on the page.
pub fn record_picture(album_id: &str, kept: Option<&str>) -> String {
    match kept.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => k.to_string(),
        None => format!("/api/cover/cratarr/{album_id}"),
    }
}

/// A song's id from its Cratarr track id.
pub fn song(track: i64) -> String {
    format!("{SONG}{track}")
}

/// The Cratarr track id in a song id.
pub fn track_of(id: &str) -> Option<i64> {
    id.strip_prefix(SONG)?.parse().ok()
}
