//! `queue`'s repeat and where a queue was heard, in base: what a player's view holds of its queue. `queue` re-exports
//! them.

use serde::{Deserialize, Serialize};

/// Where a queue was heard: on which player, through what (its app's audio output: `bluetooth`, `speaker`, `wired`,
/// `usb`, `hdmi`, `remote`, `other`; none where the player does not say) and the device's name where it has one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PlayedOn {
    pub player: String,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

impl PlayedOn {
    /// Whether `text` names it: in its device's name or its player's, whatever the case.
    pub fn named(&self, text: &str) -> bool {
        let t = text.to_lowercase();
        self.name.as_deref().is_some_and(|n| n.to_lowercase().contains(&t)) || self.player.to_lowercase().contains(&t)
    }
}

/// Off: the queue ends. One: the playing track starts again. All: after the last track the album or playlist starts
/// again (what was added by hand plays once).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test-support"), derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum Repeat {
    #[default]
    Off,
    One,
    All,
}
