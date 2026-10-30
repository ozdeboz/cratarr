//! A playlist's edits as operations (0.12, decision 0087): what a page made offline (add a track after an entry,
//! remove an entry, move one after another), replayed and merged on the playlist as it is now (`crate::playlist_ops`),
//! written as one edit of the `.m3u` like every other.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{PlaylistService, ToggleError};
use crate::playlist_ops::{Do, plain_id};
use crate::source::PlaylistChange;

/// The most operations one call carries.
const MOST: usize = 1_000;

/// A track on a server: what an add brings, and what each entry plays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrackRef {
    pub source: String,
    pub id: String,
}

/// One operation, as a page made it: its own id (`op_id`, made once: sent again, not applied again), when it was made
/// on the device (`at`, epoch ms), and the entries it names (`after`: null for the start).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "op", rename_all = "snake_case")]
#[ts(export)]
pub enum PlaylistOp {
    /// A track added as a new entry (its id the page's own) after `after`.
    Add { op_id: String, at: i64, entry: String, track: TrackRef, #[serde(default)] after: Option<String> },
    Remove { op_id: String, at: i64, entry: String },
    Move { op_id: String, at: i64, entry: String, #[serde(default)] after: Option<String> },
}

impl PlaylistOp {
    fn op_id(&self) -> &str {
        match self {
            PlaylistOp::Add { op_id, .. } | PlaylistOp::Remove { op_id, .. } | PlaylistOp::Move { op_id, .. } => op_id,
        }
    }

    fn at(&self) -> i64 {
        match self {
            PlaylistOp::Add { at, .. } | PlaylistOp::Remove { at, .. } | PlaylistOp::Move { at, .. } => *at,
        }
    }

    /// Its ids are short plain tokens.
    fn plain(&self) -> bool {
        let named = match self {
            PlaylistOp::Add { entry, after, .. } | PlaylistOp::Move { entry, after, .. } => {
                plain_id(entry) && after.as_deref().is_none_or(plain_id)
            }
            PlaylistOp::Remove { entry, .. } => plain_id(entry),
        };
        named && plain_id(self.op_id())
    }
}

/// `POST /api/playlists/{id}/ops`: `{ops: [...]}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export)]
pub struct PlaylistOps {
    pub ops: Vec<PlaylistOp>,
}

/// One entry of the playlist as it now is: its id and its track on the server asked about (`?source=`, as the rows
/// take it); `track` null where the line is not a track there (its file missing, or not matched).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct OpsEntry {
    pub entry: String,
    pub track: Option<TrackRef>,
}

/// An operation not applied, and why in a short phrase ("not found": an add whose track Cratarr cannot find). Kept
/// with its `op_id`: sent again, it is refused again; the page drops it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RefusedOp {
    pub op_id: String,
    pub why: String,
}

/// The answer: the change log's rev after the operations, the playlist as it now is, and the operations refused
/// (empty when none).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct OpsAnswer {
    pub rev: i64,
    pub entries: Vec<OpsEntry>,
    pub refused: Vec<RefusedOp>,
}

/// Why an add is refused: its track not found on its server, or not in a library Cratarr sees.
const NOT_FOUND: &str = "not found";
/// A Bandcamp preview (decision 0071).
const NEVER_IN_A_PLAYLIST: &str = "never in a playlist";

/// One call's operations at a time: each reads the playlist's entries, applies, writes and keeps them.
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl PlaylistService {
    /// Applies a page's operations to one of the asking account's own playlists (as every edit: `own`), merged on
    /// the playlist as it is now, and answers it as it then is. Operations applied before (by `op_id`) are left out;
    /// an add whose track is not in a library Cratarr sees is refused, and kept so: sent again, refused again.
    pub async fn apply_ops(&self, id: &str, want: Option<&str>, ops: Vec<PlaylistOp>) -> Result<OpsAnswer, ToggleError> {
        if ops.len() > MOST {
            return Err(ToggleError::Refused(format!("At most {MOST} operations at once.")));
        }
        if !ops.iter().all(PlaylistOp::plain) {
            return Err(ToggleError::Refused("An operation's ids are not usable.".into()));
        }
        let (store, name) = self.own(id)?;
        let serve = self.served.clone().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let me = serve.current_user().unwrap_or_default();
        let key = store.key(&name);
        let lines = self.lines_added(&ops).await;
        let _one = ONE_AT_A_TIME.lock().await;
        let ids: Vec<String> = ops.iter().map(|o| o.op_id().to_string()).collect();
        let (ud, who) = (serve.userdata.clone(), me.clone());
        let applied = tokio::task::spawn_blocking(move || ud.ops_applied(&who, &ids)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        // Read, merged and written while the file is as read; changed meanwhile (another edit), read again.
        let mut tries = 0;
        let entries = loop {
            tries += 1;
            let before = store.entries(&name).await?;
            let (ud, k, b) = (serve.userdata.clone(), key.clone(), before.clone());
            let mut entries = tokio::task::spawn_blocking(move || ud.entries_of(&k, &b)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
            let now = now_ms();
            let mut done: Vec<(String, Option<String>)> = Vec::new();
            for (i, op) in ops.iter().enumerate() {
                if applied.contains_key(op.op_id()) {
                    continue;
                }
                let one = match op {
                    PlaylistOp::Add { entry, after, .. } => match lines.get(i).cloned().flatten() {
                        Some(Ok(line)) => Do::Add { entry: entry.clone(), line, after: after.clone() },
                        Some(Err(why)) => {
                            done.push((op.op_id().to_string(), Some(why.to_string())));
                            continue;
                        }
                        None => {
                            done.push((op.op_id().to_string(), Some(NOT_FOUND.to_string())));
                            continue;
                        }
                    },
                    PlaylistOp::Remove { entry, .. } => Do::Remove { entry: entry.clone() },
                    PlaylistOp::Move { entry, after, .. } => Do::Move { entry: entry.clone(), after: after.clone() },
                };
                entries.apply(&one, op.at(), now);
                done.push((op.op_id().to_string(), None));
            }
            let after = entries.lines();
            if after != before {
                match store.replace_entries_if(&name, &before, &after).await? {
                    Some(edit) => {
                        self.tell_sources(&store, vec![(name.clone(), PlaylistChange::Edited)]);
                        // Kept for its undo as every edit (the page's Undo is not offered for these).
                        let _ = self.keep_texts(vec![edit]);
                    }
                    None if tries < 3 => continue,
                    None => return Err(ToggleError::Refused("The playlist kept changing meanwhile: send it again.".into())),
                }
            }
            // Refused before (sent again) and now, in the order sent.
            let refused: Vec<RefusedOp> = ops
                .iter()
                .filter_map(|op| {
                    let why = match applied.get(op.op_id()) {
                        Some(before) => before.clone(),
                        None => done.iter().find(|(id, _)| id == op.op_id()).and_then(|(_, why)| why.clone()),
                    }?;
                    Some(RefusedOp { op_id: op.op_id().to_string(), why })
                })
                .collect();
            let (ud, k, e, who) = (serve.userdata.clone(), key.clone(), entries.clone(), me.clone());
            tokio::task::spawn_blocking(move || ud.keep_entries(&k, &e, &who, &done)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
            break (entries, refused);
        };
        let (entries, refused) = entries;
        let ud = serve.userdata.clone();
        let rev = tokio::task::spawn_blocking(move || ud.change_rev()).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        let source = self.rows_source(want);
        let rows = self.rows_of(entries.lines(), &source).await?;
        let entries = entries
            .live
            .into_iter()
            .zip(rows)
            .map(|(e, row)| OpsEntry { entry: e.id, track: row.id.map(|id| TrackRef { source: source.clone(), id }) })
            .collect();
        Ok(OpsAnswer { rev, entries, refused })
    }

    /// Each add's line, from its track as a connected server and the catalogue give it (never a path a page sent), or
    /// why there is none; `None` for every other operation.
    async fn lines_added(&self, ops: &[PlaylistOp]) -> Vec<Option<Result<String, &'static str>>> {
        let folders = self.lib.folders();
        let sources = self.lib.connected();
        let mut out = Vec::with_capacity(ops.len());
        for op in ops {
            let PlaylistOp::Add { track, .. } = op else {
                out.push(None);
                continue;
            };
            // A Bandcamp preview is never in a playlist (decision 0071).
            if track.source == crate::bandcamp::SOURCE {
                out.push(Some(Err(NEVER_IN_A_PLAYLIST)));
                continue;
            }
            if !crate::covers::valid_item_id(&track.id) {
                out.push(Some(Err(NOT_FOUND)));
                continue;
            }
            let found = match sources.iter().find(|(a, _)| a.cfg.id == track.source) {
                Some((a, s)) => match s.track(&track.id).await {
                    Ok(t) => a.line(&folders, &t),
                    Err(_) => None,
                },
                None => None,
            };
            out.push(Some(found.ok_or(NOT_FOUND)));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operations_are_read_as_a_page_sends_them() {
        let body = serde_json::json!({ "ops": [
            { "op_id": "o1", "at": 5, "op": "add", "entry": "p1", "track": { "source": "cratarr", "id": "tr-1" }, "after": null },
            { "op_id": "o2", "at": 6, "op": "remove", "entry": "s1" },
            { "op_id": "o3", "at": 7, "op": "move", "entry": "p1", "after": "s2" },
        ] });
        let ops: PlaylistOps = serde_json::from_value(body).unwrap();
        assert_eq!(
            ops.ops,
            vec![
                PlaylistOp::Add { op_id: "o1".into(), at: 5, entry: "p1".into(), track: TrackRef { source: "cratarr".into(), id: "tr-1".into() }, after: None },
                PlaylistOp::Remove { op_id: "o2".into(), at: 6, entry: "s1".into() },
                PlaylistOp::Move { op_id: "o3".into(), at: 7, entry: "p1".into(), after: Some("s2".into()) },
            ]
        );
        assert!(ops.ops.iter().all(PlaylistOp::plain));
        let bad = PlaylistOp::Remove { op_id: "o4".into(), at: 8, entry: "../x".into() };
        assert!(!bad.plain());
    }

    #[test]
    fn the_answer_is_the_rev_and_each_entry_with_its_track() {
        let a = OpsAnswer {
            refused: vec![RefusedOp { op_id: "o9".into(), why: "not found".into() }],
            rev: 12,
            entries: vec![
                OpsEntry { entry: "s1".into(), track: Some(TrackRef { source: "cratarr".into(), id: "tr-1".into() }) },
                OpsEntry { entry: "p1".into(), track: None },
            ],
        };
        assert_eq!(
            serde_json::to_value(&a).unwrap(),
            serde_json::json!({
                "rev": 12,
                "entries": [{ "entry": "s1", "track": { "source": "cratarr", "id": "tr-1" } }, { "entry": "p1", "track": null }],
                "refused": [{ "op_id": "o9", "why": "not found" }],
            })
        );
    }
}
