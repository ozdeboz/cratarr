//! What changed since a device last looked (0.12, decision 0087): the change log as the asking account sees it, its
//! own changes and the playlists it sees (its own, those shared with it), each item named as the pages name it.

use std::sync::Arc;

use serde::Serialize;
use ts_rs::TS;

use super::{App, ToggleError};
use crate::userdata::offline::{ChangeKind, Since};

/// The most changes one answer holds; `more` says ask again.
pub const MOST: usize = 1_000;

/// One change: its rev, what changed (`playlist`, `favourite`, `rating`, `excluded`), the item's id as the pages name
/// it, and `changed` or `deleted`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
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

/// `GET /api/changes?since=<rev>&kinds=a,b`: `{rev, changes, more}`, at most 1,000 changes, the next asked from `rev`
/// while `more`; or `{rev, reset: true}`: not known from `since` (none sent, older than what is kept, ahead of this
/// server), so read everything again and go on from `rev`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Changes {
    pub rev: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub changes: Option<Vec<Change>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub more: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reset: Option<bool>,
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
pub async fn read(app: &Arc<App>, serve: Arc<crate::serve::Serve>, since: Option<i64>, kinds: Option<&str>) -> Result<Changes, ToggleError> {
    let user = serve.current_user().ok_or_else(|| ToggleError::Refused("No one is signed in.".into()))?;
    let folder = app.playlists.own_folder();
    let kinds = kinds_of(kinds);
    let found = tokio::task::spawn_blocking(move || serve.userdata.changes_since(since, &user, &folder, &kinds, MOST))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
    Ok(match found {
        Since::Reset { rev } => Changes { rev, changes: None, more: None, reset: Some(true) },
        Since::Changes { rev, changes, more } => Changes {
            rev,
            changes: Some(
                changes
                    .into_iter()
                    .map(|c| Change {
                        rev: c.rev,
                        kind: c.kind.as_str().to_string(),
                        // A playlist is logged by its key; the asking account names it its own way.
                        id: if c.kind == ChangeKind::Playlist { app.playlists.id_of_key(&c.item) } else { c.item },
                        op: c.op.as_str().to_string(),
                    })
                    .collect(),
            ),
            more: Some(more),
            reset: None,
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
    fn an_answer_is_changes_or_a_reset() {
        let reset = Changes { rev: 7, changes: None, more: None, reset: Some(true) };
        assert_eq!(serde_json::to_value(&reset).unwrap(), serde_json::json!({ "rev": 7, "reset": true }));
        let some = Changes {
            rev: 9,
            changes: Some(vec![Change { rev: 9, kind: "favourite".into(), id: "album:cratarr/al-1".into(), op: "deleted".into() }]),
            more: Some(false),
            reset: None,
        };
        assert_eq!(
            serde_json::to_value(&some).unwrap(),
            serde_json::json!({ "rev": 9, "changes": [{ "rev": 9, "kind": "favourite", "id": "album:cratarr/al-1", "op": "deleted" }], "more": false })
        );
    }
}
