//! `search`'s result, in base: one result, the same shape for every kind, as records' editions fold them. `search`
//! re-exports it.

use serde::Serialize;

/// Why a result is there: what it matched on (`name`, `artist`, `album`, `track`), the text, and how many.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Matched {
    pub on: &'static str,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

/// One result, the same shape for every kind.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub kind: &'static str,
    /// What the panel opens: `album:cratarr/<id>`, `artist:cratarr/<name>`, `playlist:<id>`, `crate:<id>`,
    /// `crate:genre:<name>`; a track its album's.
    pub subject: String,
    pub title: String,
    pub sub: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    /// A track's own id, to play it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    /// An album's artist where the panel needs it apart from `sub`: one not in the library, to look for on Bandcamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    /// A track's album and its id, and its length (0.12: the page opens a track's own menu from a search hit, and
    /// no longer reads them out of `sub`, which a name holding " · " broke).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_id: Option<String>,
    /// Its record's type and secondary types as the Library list serves them (GOTO.S1): a track's Go to names it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub album_secondary: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
    /// A missing album's page on Bandcamp where an open of it found one lately (0.9.5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bandcamp: Option<String>,
    /// A missing album's open request (0.9.5): wanted, or coming and how far.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<super::requests_base::Mark>,
    /// Why Cratarr knows an album not in the library (0076, the indexed universe): "Missing from <artist>", "On your
    /// Spotify", "Suggested", "Requested"; one album once, its reasons together.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub known: Vec<String>,
    /// A classical record's, or its track's, composers (0.10); a record found by its composer says so in `matched`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub composers: Vec<crate::source::ComposerRef>,
    /// An artist's library id (`ar-…`; 0.11, quick crates: what a Sounds like is seeded with from a search hit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// An artist's number for their name (C55, G5): the page shows "Name (n)" where a list holds two of a name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<i64>,
    /// How many editions of one album a record's item stands for, its own among them (F12.1, `app::editions`); none: one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editions: Option<u32>,
    pub score: f32,
    pub matched: Vec<Matched>,
}
