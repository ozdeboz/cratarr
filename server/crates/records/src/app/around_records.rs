//! An account's Around as kept, read and changed, and the suggestions of the group showing, beside records (moved from
//! `app::around`, which keeps their names): Discover's Want, Hide and Not interested read the suggestions too.

use std::sync::Mutex;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::discover::Suggestion;

/// An account's Around, as kept (`UserData::discover_around`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// The seeds shown this round (MusicBrainz ids), the group showing among them.
    #[serde(default)]
    pub shown: Vec<String>,
    /// The group showing: its seeds (name, MusicBrainz id), and their suggestions once made.
    #[serde(default)]
    pub seeds: Vec<(String, String)>,
    #[serde(default)]
    pub albums: Option<Vec<Suggestion>>,
    /// The group after it, looked up ahead.
    #[serde(default)]
    pub next: Vec<(String, String)>,
}

/// One change of an account's State at a time (read, changed, kept).
static CHANGING: Mutex<()> = Mutex::new(());

pub fn kept(serve: &crate::serve::Serve, account: &str) -> Result<State> {
    Ok(serve.userdata.discover_around(account)?.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default())
}

/// The account's State read, changed by `change` and kept, one change at a time.
pub fn change<T>(serve: &crate::serve::Serve, account: &str, change: impl FnOnce(&mut State) -> T) -> Result<(State, T)> {
    let _one = CHANGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut st = kept(serve, account)?;
    let out = change(&mut st);
    serve.userdata.set_discover_around(account, &serde_json::to_string(&st)?)?;
    Ok((st, out))
}

/// The group showing's suggestions as kept, for what is done with one of them (Want, Hide, Not interested).
pub fn suggestions(serve: &crate::serve::Serve, account: &str) -> Result<Vec<Suggestion>> {
    Ok(kept(serve, account)?.albums.unwrap_or_default())
}
