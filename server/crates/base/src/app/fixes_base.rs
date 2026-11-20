//! Health's fixes as the page shows them, and a track's tag values as the tag reader reads them, below the areas that read them
//! (moved from `fixes`, which keeps their names).

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

/// A fix as the page shows it, its JSON kept stable (the page builds on it).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fix {
    /// `<field>:<album>`: the same fix keeps its id while it stands.
    pub id: String,
    pub album: String,
    /// What it sets: year, genre, albumartist, musicbrainz, cover (its `to` the picture's address), rule (a genre or
    /// an artist the rules name otherwise: `from` → `to`).
    pub field: &'static str,
    /// What the album has now (none: nothing).
    pub from: Option<String>,
    pub to: String,
    /// Where it comes from ("MusicBrainz", "Discogs and Last.fm"…).
    pub source: String,
    /// certain (Apply all certain applies it) or likely (applied one by one).
    pub certainty: &'static str,
    pub why: String,
    /// What each track's tags get (the page has no need of it).
    #[serde(skip)]
    pub writes: Vec<(PathBuf, Vec<(&'static str, Option<String>)>)>,
    /// A genre fix's choices (0.9.8, the owner: "the option to use multiple found genres"): every genre and style found
    /// for the album, each with its sources; the administrator ticks several (`choose`). `to` is the strongest.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Choice>,
}

/// One genre or style a genre fix offers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Choice {
    pub word: String,
    /// genre or style, as the sources have it.
    pub kind: &'static str,
    /// Its sources as said ("Discogs and Last.fm").
    pub source: String,
    /// certain where two sources or more say it.
    pub certainty: &'static str,
}

/// A track's tag values by name, as the tag reader reads them (the first of each).
pub fn values(track: &Path, names: &[&str]) -> Result<Vec<Option<String>>> {
    let (tags, _) = crate::catalogue::tags::read(track)?;
    Ok(names.iter().map(|n| tags.items.iter().find(|(_, name, _)| *name == Some(*n)).map(|(_, _, v)| v.clone())).collect())
}
