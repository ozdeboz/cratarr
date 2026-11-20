//! `radio`'s Autoplay modes and what Radio stays inside, in base: what a command and a player's view hold of them.
//! `radio` re-exports them.

use serde::{Deserialize, Serialize};

use cratarr_rules::radio::Mode;

/// Autoplay's modes; off is its absence (`stop` in commands).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AutoMode {
    #[default]
    Similar,
    Artist,
    Era,
    Random,
    Taste,
}

impl AutoMode {
    /// What Autoplay in this mode asks for.
    pub fn asks(self) -> Mode {
        match self {
            AutoMode::Similar => Mode::Similar,
            AutoMode::Artist => Mode::Artist,
            AutoMode::Era => Mode::Era,
            AutoMode::Random => Mode::Random,
            AutoMode::Taste => Mode::Taste,
        }
    }
}

/// Autoplay's mode in a command: `stop` switches it off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AutoplayMode {
    Stop,
    Similar,
    Artist,
    Era,
    Random,
    Taste,
}

impl AutoplayMode {
    pub fn on(self) -> Option<AutoMode> {
        match self {
            AutoplayMode::Stop => None,
            AutoplayMode::Similar => Some(AutoMode::Similar),
            AutoplayMode::Artist => Some(AutoMode::Artist),
            AutoplayMode::Era => Some(AutoMode::Era),
            AutoplayMode::Random => Some(AutoMode::Random),
            AutoplayMode::Taste => Some(AutoMode::Taste),
        }
    }
    pub fn of(mode: Option<AutoMode>) -> Self {
        match mode {
            None => AutoplayMode::Stop,
            Some(AutoMode::Similar) => AutoplayMode::Similar,
            Some(AutoMode::Artist) => AutoplayMode::Artist,
            Some(AutoMode::Era) => AutoplayMode::Era,
            Some(AutoMode::Random) => AutoplayMode::Random,
            Some(AutoMode::Taste) => AutoplayMode::Taste,
        }
    }
}

/// What Radio and Autoplay stay inside (decision 0094: the page's collection), as Cratarr's own library has it: a
/// crate as saved (a genre too, `genre:<name>`), a playlist, a record, an artist, or tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Within {
    /// `without`: the libraries switched off, as the crate showed (Home's crates are of every library).
    Crate {
        id: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(test, ts(as = "Option<_>", optional))]
        without: Vec<String>,
    },
    Playlist { id: String },
    Album { source: String, id: String },
    /// `id`: theirs (`ar-<n>`, G6) where the page knows it; `without`: the libraries switched off, as the artist showed.
    Artist {
        source: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(test, ts(as = "Option<_>", optional))]
        without: Vec<String>,
    },
    Tracks { source: String, ids: Vec<String> },
}
