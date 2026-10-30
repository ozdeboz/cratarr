//! The one error type the services return; `api.rs` turns it into an HTTP status.

use crate::playlists::StoreError;
use crate::settings::ChangeError;

#[derive(Debug, thiserror::Error)]
pub enum ToggleError {
    #[error("nothing is playing")]
    NothingPlaying,
    #[error("the track is not in a library Cratarr can see, so it cannot be added to a playlist")]
    NotInLibrary,
    #[error("unknown playlist")]
    UnknownPlaylist,
    #[error("that repair is no longer offered")]
    NotOffered,
    #[error("there is no earlier version to go back to")]
    NothingToUndo,
    #[error("a playlist with that name already exists")]
    Exists,
    #[error("that is not a usable playlist name")]
    BadName,
    #[error("{0}")]
    CannotPlay(String),
    /// An action that cannot be done as asked (the album moved, the trash is gone…); shown as is.
    #[error("{0}")]
    Refused(String),
    /// Not this account's to do (decision 0051): an administrator's, or another user's.
    #[error("{0}")]
    Forbidden(String),
    /// Another user's shared device, from away from home (the owner, 2026-10-29: others use one's shared devices from
    /// home only).
    #[error("Works at home only.")]
    Fenced,
    /// A player's name another account's player has (decision 0051): the device takes another.
    #[error("{0} is another user's player.")]
    NameTaken(String),
    #[error("no Music library is set")]
    NoMusicFolder,
    #[error("settings cannot be saved: CRATARR_DATA_DIR is not set")]
    SettingsReadOnly,
    #[error("{0}")]
    BadSettings(String),
    #[error("{0:#}")]
    Upstream(#[from] anyhow::Error),
}

/// What a failed write says to whoever asked (0.9.8, the owner): a folder Cratarr may not write to is one short line,
/// never the system's own words ("Read-only file system (os error 30)"); anything else as it is.
pub fn plain(message: String) -> String {
    if message.contains("(os error 30)") { READ_ONLY.to_string() } else { message }
}

/// A write refused because the music is mounted read-only.
pub const READ_ONLY: &str = "The music folder is read-only.";

impl From<ChangeError> for ToggleError {
    fn from(e: ChangeError) -> Self {
        ToggleError::BadSettings(e.to_string())
    }
}

impl From<StoreError> for ToggleError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::Unknown => ToggleError::UnknownPlaylist,
            StoreError::NotOffered => ToggleError::NotOffered,
            StoreError::NothingToUndo => ToggleError::NothingToUndo,
            StoreError::Exists => ToggleError::Exists,
            StoreError::BadName => ToggleError::BadName,
            StoreError::Io(e) => ToggleError::Upstream(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_only_folder_is_one_short_line() {
        let raw = "writing /media/lidarr-music/A/B (1996)/cover.jpg: Read-only file system (os error 30)".to_string();
        assert_eq!(plain(raw), READ_ONLY);
        let other = "writing /media/A/cover.jpg: Permission denied (os error 13)".to_string();
        assert_eq!(plain(other.clone()), other, "any other failure as it is");
    }
}
