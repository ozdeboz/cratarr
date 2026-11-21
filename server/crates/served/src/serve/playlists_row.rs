//! A playlist's line as its view shows it, below the app (moved from `app::playlists`, which keeps its name): the served
//! library gives rows their records' types (`Serve::with_types`).

use serde::Serialize;

/// An artist a track credits (NAMES.23): their id for life and one name.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct TrackArtist {
    pub id: String,
    pub name: String,
}

/// One line of a playlist as its view shows it (decision 0023). `entry` is the line as the `.m3u` has it (library-
/// relative), `file` its file name; the rest is what the catalogue knows, `None` where it does not.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct PlaylistRow {
    /// Its record's type, as the Library list serves it (GOTO.S1): a track's Go to names it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub album_secondary: Vec<String>,
    pub entry: String,
    /// Its entry id (0.12, decision 0087): what the playlist's operations name it by (`entry` stays the line). Absent
    /// (a crate's row, or ids not kept): no operations for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<String>,
    pub file: String,
    /// The server's track id, when the file is matched to one.
    pub id: Option<String>,
    pub name: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_secs: Option<f64>,
    /// The panel's orders (decision 0025's 2026-10-16 amendment): from the file's tags,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    /// its tempo (the BPM tag, else the analysis', decision 0035),
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bpm: Option<u32>,
    /// the user's plays of the track and when it was last played,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_played: Option<i64>,
    /// and when the entry came into the playlist (none for one held before Cratarr kept that; a crate's row: when its
    /// track came into the library).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<i64>,
    /// Its album's id where it is known: an artist's rows, and a playlist's or a crate's on Cratarr's own source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_id: Option<String>,
    /// Its picture (OBJ5): its record's, by the one resolver (`ids::record_picture`), where its record is Cratarr's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    /// Its artists, each by their id for life and one name (NAMES.23: each its own link in the one track row), where the library
    /// credits them by id; `artist` is their credit by the one formatter.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<TrackArtist>,
    /// How many tracks that album has (the Records view's "3 of 10"), where its id is known on Cratarr's own source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_tracks: Option<u32>,
    /// No such file in the Music folder.
    pub missing: bool,
    /// The library holds another recording of its work (0.10, Other versions): an album's rows on Cratarr's own
    /// library; absent where not, or not said.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_versions: Option<bool>,
    /// The work it performs, with the work that holds it and its number there (0.12, decision 0086: an album's
    /// movements under their work's heading): an album's rows on Cratarr's own library, where MusicBrainz says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<super::classical_works::RowWork>,
}
