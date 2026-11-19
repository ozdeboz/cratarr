//! What changed since a device last looked (0.12, decision 0087): the change log as the asking account sees it, its
//! own changes and the playlists it sees (its own, those shared with it), each item named as the pages name it. Where a device
//! asks, a word on the library's own log too (`app::device_index`): that log has its own counter, as the catalogue keeps it.

use std::sync::Arc;

use serde::Serialize;

use super::{App, ToggleError};
use crate::userdata::offline::{ChangeKind, Since};

/// The most changes one answer holds; `more` says ask again.
pub const MOST: usize = 1_000;

/// One change: its rev, what changed (`playlist`, `favourite`, `rating`, `excluded`), the item's id as the pages name
/// it, and `changed` or `deleted`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct Change {
    pub rev: i64,
    pub kind: String,
    /// `playlist`: its id (one's own by name, another's shared one `owner/name`); `favourite`: `track:<source>/<id>`,
    /// `album:<source>/<id>`, `artist:<source>/<name>` or `playlist:<id>`; `rating`: `album:<source>/<id>`;
    /// `excluded`: `tracks` or `artists`.
    pub id: String,
    /// `changed` (there now: made, edited, a favourite, rated) or `deleted` (no longer: deleted, not a favourite, no
    /// stars).
    pub op: String,
}

/// `kinds=index`, from `index=<rev>`: where the library's change log stands, and what a device at that rev has to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct IndexStands {
    pub rev: i64,
    /// Rows moved after that rev: `GET /api/index/changes?since=<it>` brings them.
    pub changed: bool,
    /// Not known from that rev (none sent, older than what is kept, ahead of this server): start again, with `GET /api/index`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub reset: Option<bool>,
}

/// `GET /api/changes?since=<rev>&kinds=a,b`: `{rev, changes, more}`, at most 1,000 changes, the next asked from `rev`
/// while `more`; or `{rev, reset: true}`: not known from `since` (none sent, older than what is kept, ahead of this
/// server), so read everything again and go on from `rev`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct Changes {
    pub rev: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub changes: Option<Vec<Change>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub more: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub reset: Option<bool>,
    /// `index` among `kinds`: the library's log.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub index: Option<IndexStands>,
}

/// Whether `index` is among the kinds asked for: named, as it has a rev of its own to go with it.
fn asks_index(asked: Option<&str>) -> bool {
    asked.is_some_and(|list| list.split(',').any(|k| k.trim() == "index"))
}

/// The kinds asked for (`a,b`; none: all). Names not known are left out.
pub fn kinds_of(asked: Option<&str>) -> Vec<ChangeKind> {
    match asked {
        None => ChangeKind::ALL.to_vec(),
        Some(list) => {
            let mut kinds: Vec<ChangeKind> = list.split(',').filter_map(|k| ChangeKind::of(k.trim())).collect();
            kinds.dedup();
            kinds
        }
    }
}

/// The changes the asking account may see since `since`.
pub async fn read(app: &Arc<App>, serve: Arc<crate::serve::Serve>, since: Option<i64>, kinds: Option<&str>, index_since: Option<i64>) -> Result<Changes, ToggleError> {
    let user = serve.current_user().ok_or_else(|| ToggleError::Refused("No one is signed in.".into()))?;
    let folder = app.playlists.own_folder();
    let index = if asks_index(kinds) { Some(super::device_index::stands(app, serve.clone(), index_since).await?) } else { None };
    let kinds = kinds_of(kinds);
    let found = tokio::task::spawn_blocking(move || serve.userdata.changes_since(since, &user, &folder, &kinds, MOST))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
    Ok(match found {
        Since::Reset { rev } => Changes { rev, changes: None, more: None, reset: Some(true), index },
        Since::Changes { rev, changes, more } => Changes {
            rev,
            changes: Some(
                changes
                    .into_iter()
                    .map(|c| Change {
                        rev: c.rev,
                        kind: c.kind.as_str().to_string(),
                        // A playlist is logged by its key; the asking account names it its own way.
                        id: c.item,
                        op: c.op.as_str().to_string(),
                    })
                    .collect(),
            ),
            more: Some(more),
            reset: None,
            index,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kinds_asked_are_read_and_unknown_ones_left_out() {
        assert_eq!(kinds_of(None), ChangeKind::ALL.to_vec());
        assert_eq!(kinds_of(Some("playlist, rating,crate")), vec![ChangeKind::Playlist, ChangeKind::Rating]);
        assert!(kinds_of(Some("")).is_empty());
    }

    #[test]
    fn index_is_asked_for_by_name_and_no_kind_of_the_user_datas() {
        assert!(asks_index(Some("index")) && asks_index(Some("playlist, index")));
        assert!(!asks_index(None) && !asks_index(Some("playlist")));
        assert_eq!(kinds_of(Some("index,rating")), vec![ChangeKind::Rating]);
    }

    #[test]
    fn the_library_has_its_say_beside_the_changes() {
        let some = Changes { rev: 9, changes: Some(Vec::new()), more: Some(false), reset: None, index: Some(IndexStands { rev: 40, changed: true, reset: None }) };
        assert_eq!(serde_json::to_value(&some).unwrap(), serde_json::json!({ "rev": 9, "changes": [], "more": false, "index": { "rev": 40, "changed": true } }));
        let again = IndexStands { rev: 40, changed: true, reset: Some(true) };
        assert_eq!(serde_json::to_value(&again).unwrap(), serde_json::json!({ "rev": 40, "changed": true, "reset": true }));
    }

    #[test]
    fn an_answer_is_changes_or_a_reset() {
        let reset = Changes { rev: 7, changes: None, more: None, reset: Some(true), index: None };
        assert_eq!(serde_json::to_value(&reset).unwrap(), serde_json::json!({ "rev": 7, "reset": true }));
        let some = Changes {
            rev: 9,
            changes: Some(vec![Change { rev: 9, kind: "favourite".into(), id: "album:cratarr/al-1".into(), op: "deleted".into() }]),
            more: Some(false),
            reset: None,
            index: None,
        };
        assert_eq!(
            serde_json::to_value(&some).unwrap(),
            serde_json::json!({ "rev": 9, "changes": [{ "rev": 9, "kind": "favourite", "id": "album:cratarr/al-1", "op": "deleted" }], "more": false })
        );
    }
}
