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
