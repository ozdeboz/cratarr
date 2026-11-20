//! A track's work as a playlist's rows carry it (`PlaylistRow::work`), below the app (moved from `app::classical`, which keeps
//! their names).

use serde::Serialize;

/// A work as Other versions names it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct WorkRef {
    pub mbid: String,
    pub title: String,
}

/// The work an album's track performs (0.12, decision 0086; an album's rows' `work`): the work that holds it and its
/// number there, where MusicBrainz says. A classical record's tracks only (0.13: MusicBrainz has works for almost every
/// song, so a soundtrack listed its songs as works).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct RowWork {
    pub mbid: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub parent: Option<WorkRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub part_number: Option<u32>,
}
