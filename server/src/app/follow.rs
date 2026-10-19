//! Playlists follow album actions (decisions 0011 §6, 0012, 0013): the journal's "playlists" handler.
//!
//! - Keep and Replace move files into Music: every playlist line naming a moved file names its new place. Undo puts
//!   the lines back.
//! - Delete takes the album's tracks out of every playlist (the server copies follow through the sync). Where each
//!   line stood is recorded first, so a retry after a crash removes the same lines and Undo puts them back there.
//!
//! Safe to repeat: what is done (and undone) is recorded per event in the journal.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use super::library::{Library, epoch_now};
use crate::journal::{Entry, Event, Journal};
use crate::playlists::Store;

/// The handler name in the journal; its result is the playlists it changed.
pub(super) const PLAYLISTS: &str = "playlists";
/// Where a deleted album's lines stood, recorded before they are taken out.
const PLAYLISTS_PLAN: &str = "playlists-plan";
/// The lines are back after an Undo.
const PLAYLISTS_UNDONE: &str = "playlists-undone";

/// Per playlist: the lines taken out, each with its position.
type Removed = BTreeMap<String, Vec<(usize, String)>>;

/// A catalogue file as a playlist line: relative to the Music folder (folder 0), "../" for files outside it.
fn line_of(folders: &[PathBuf], (folder, rel): &(i64, String)) -> Option<String> {
    let folder = folders.get(usize::try_from(*folder).ok()?)?;
    super::sync::relative_to(folders.first()?, &folder.join(rel))
}

/// The lines an album action renames, `back` for its Undo. Empty for a Delete.
fn renames(event: &Event, folders: &[PathBuf], back: bool) -> HashMap<String, String> {
    let line = |at: &(i64, String)| line_of(folders, at);
    let mut out = HashMap::new();
    let mut add = |from: Option<String>, to: Option<String>| {
        if let (Some(f), Some(t)) = (from, to)
            && f != t
        {
            out.insert(f, t);
        }
    };
    match event {
        Event::AlbumRejected { .. } => {}
        Event::AlbumAdopted { moves, from_dir, converted, .. } => {
            for m in moves {
                // A converted file that was renamed (a WAV became FLAC): its lines may still name the original.
                let original = converted.iter().find(|c| c.converted == m.from.1).map(|c| (from_dir.0, c.original.clone()));
                let (f, t) = (line(original.as_ref().unwrap_or(&m.from)), line(&m.to));
                if back {
                    add(t, f)
                } else {
                    add(line(&m.from), t.clone());
                    add(f, t)
                }
            }
        }
        // The old files' lines go to the new file that inherited their track; back, a new file's line goes to the
        // old file it replaced (else to where it came from).
        Event::AlbumReplaced { moves, replaced, from_dir, converted, .. } => {
            for m in moves {
                let old = replaced.iter().find(|r| r.track.is_some() && r.track == m.track);
                let original = converted.iter().find(|c| c.converted == m.from.1).map(|c| (from_dir.0, c.original.clone()));
                let from = original.unwrap_or_else(|| m.from.clone());
                if back {
                    add(line(&m.to), old.map_or_else(|| line(&from), |r| line(&r.from)));
                } else {
                    add(line(&from), line(&m.to));
                    add(line(&m.from), line(&m.to));
                    if let Some(r) = old {
                        add(line(&r.from), line(&m.to));
                    }
                }
            }
        }
    }
    out
}

/// `entries` with the renamed lines renamed; `None` when none are there.
fn renamed(entries: &[String], map: &HashMap<String, String>) -> Option<Vec<String>> {
    entries.iter().any(|e| map.contains_key(e)).then(|| entries.iter().map(|e| map.get(e).unwrap_or(e).clone()).collect())
}

/// Every line in an album folder, per playlist, with its position.
fn lines_under(playlists: &[(String, Vec<String>, usize)], album: &str) -> Removed {
    let prefix = format!("{album}/");
    playlists
        .iter()
        .filter_map(|(id, lines, _)| {
            let found: Vec<(usize, String)> =
                lines.iter().enumerate().filter(|(_, l)| l.starts_with(&prefix)).map(|(i, l)| (i, l.clone())).collect();
            (!found.is_empty()).then(|| (id.clone(), found))
        })
        .collect()
}

/// `entries` without these lines (every copy); `None` when none are there.
fn without(entries: &[String], lines: &[(usize, String)]) -> Option<Vec<String>> {
    let gone: HashSet<&str> = lines.iter().map(|(_, l)| l.as_str()).collect();
    entries.iter().any(|e| gone.contains(e.as_str())).then(|| entries.iter().filter(|e| !gone.contains(e.as_str())).cloned().collect())
}

/// `entries` with the taken-out lines back where they stood (at the end when the playlist got shorter). A line
/// already there again is not added twice; `None` when nothing is put back.
fn put_back(entries: &[String], lines: &[(usize, String)]) -> Option<Vec<String>> {
    let there: HashSet<&str> = entries.iter().map(String::as_str).collect();
    let mut back: Vec<&(usize, String)> = lines.iter().filter(|(_, l)| !there.contains(l.as_str())).collect();
    if back.is_empty() {
        return None;
    }
    back.sort_by_key(|(at, _)| *at);
    let mut out = entries.to_vec();
    for (at, line) in back {
        out.insert((*at).min(out.len()), line.clone());
    }
    Some(out)
}

fn read<T: serde::de::DeserializeOwned>(journal: &Journal, id: i64, handler: &str) -> Option<T> {
    journal.result(id, handler).ok().flatten().and_then(|v| serde_json::from_value(v).ok())
}

/// Every account's playlists (decision 0051), as one list by their keys (`Store::key`).
async fn snapshot_of(stores: &[&Store]) -> Result<Vec<(String, Vec<String>, usize)>> {
    let mut out = Vec::new();
    for store in stores {
        out.extend(store.snapshot().await?.into_iter().map(|(n, l, d)| (store.key(&n), l, d)));
    }
    Ok(out)
}

/// `edit` on every account's playlists (by key); the keys of those it changed.
async fn edit_all(stores: &[&Store], edit: impl Fn(&str, &[String]) -> Option<Vec<String>>) -> Result<Vec<String>> {
    let mut changed = Vec::new();
    for store in stores {
        let done = store.edit_all(|id, entries| edit(&store.key(id), entries)).await?;
        changed.extend(done.iter().map(|id| store.key(id)));
    }
    Ok(changed)
}

/// React to an album action in every account's playlists. Returns the playlists that changed, by key (none when it
/// was already handled).
async fn apply(stores: &[&Store], folders: &[PathBuf], journal: &Journal, entry: &Entry) -> Result<Vec<String>> {
    if journal.result(entry.id, PLAYLISTS)?.is_some() {
        return Ok(Vec::new());
    }
    let changed = match &entry.event {
        Event::AlbumRejected { folder, dir, .. } => {
            let plan: Removed = match read(journal, entry.id, PLAYLISTS_PLAN) {
                Some(p) => p,
                None => {
                    let album = line_of(folders, &(*folder, dir.clone()));
                    let plan = match album {
                        Some(a) => lines_under(&snapshot_of(stores).await?, &a),
                        None => Removed::new(), // the folder is no longer in use
                    };
                    journal.done(entry.id, PLAYLISTS_PLAN, epoch_now(), &serde_json::to_value(&plan)?)?;
                    plan
                }
            };
            edit_all(stores, |id, entries| without(entries, plan.get(id)?)).await?
        }
        event => {
            let map = renames(event, folders, false);
            edit_all(stores, |_, entries| renamed(entries, &map)).await?
        }
    };
    journal.done(entry.id, PLAYLISTS, epoch_now(), &serde_json::json!(changed))?;
    Ok(changed)
}

/// Put back what `apply` did, once the action is undone. Returns the playlists that changed.
async fn undo(stores: &[&Store], folders: &[PathBuf], journal: &Journal, entry: &Entry) -> Result<Vec<String>> {
    if journal.result(entry.id, PLAYLISTS_UNDONE)?.is_some() {
        return Ok(Vec::new());
    }
    let changed = match &entry.event {
        // The plan, not the result: a crash may have cut `apply` short after some lines went.
        Event::AlbumRejected { .. } => match read::<Removed>(journal, entry.id, PLAYLISTS_PLAN) {
            Some(plan) => edit_all(stores, |id, entries| put_back(entries, plan.get(id)?)).await?,
            None => return Ok(Vec::new()),
        },
        event => {
            if journal.result(entry.id, PLAYLISTS)?.is_none() {
                return Ok(Vec::new()); // nothing was renamed
            }
            let map = renames(event, folders, true);
            edit_all(stores, |_, entries| renamed(entries, &map)).await?
        }
    };
    journal.done(entry.id, PLAYLISTS_UNDONE, epoch_now(), &serde_json::json!(changed))?;
    Ok(changed)
}

pub(super) struct Follow {
    lib: Arc<Library>,
    journal: Arc<Journal>,
}

impl Follow {
    pub(super) fn new(lib: Arc<Library>, journal: Arc<Journal>) -> Self {
        Self { lib, journal }
    }

    pub(super) fn journal(&self) -> &Arc<Journal> {
        &self.journal
    }

    /// React to one event, or put it back when it was undone. Without a Music folder it waits (stays pending).
    pub(super) async fn handle(&self, entry: &Entry) {
        let stores = self.lib.all_stores();
        if stores.is_empty() {
            return;
        }
        let stores: Vec<&Store> = stores.iter().map(|s| s.as_ref()).collect();
        let folders = self.lib.catalogue().folders();
        let mut done = apply(&stores, &folders, &self.journal, entry).await;
        // Undone while it was being handled, or before: put it back at once.
        if done.is_ok() && self.journal.get(entry.id).ok().flatten().is_some_and(|e| e.undone) {
            done = undo(&stores, &folders, &self.journal, entry).await;
        }
        self.after(entry, done);
    }

    pub(super) async fn handle_undo(&self, entry: &Entry) {
        let stores = self.lib.all_stores();
        if stores.is_empty() {
            return;
        }
        let stores: Vec<&Store> = stores.iter().map(|s| s.as_ref()).collect();
        let done = undo(&stores, &self.lib.catalogue().folders(), &self.journal, entry).await;
        self.after(entry, done);
    }

    fn after(&self, entry: &Entry, done: Result<Vec<String>>) {
        match done {
            Ok(changed) if !changed.is_empty() => {
                let (title, _) = entry.event.album();
                tracing::info!("playlists: {} changed with {title}", changed.len());
                // The first administrator's are kept in step with other servers (their keys are their names).
                let theirs = changed.into_iter().filter(|k| !k.contains('/'));
                super::playlists::tell_sources(&self.lib, theirs.map(|n| (n, crate::source::PlaylistChange::Edited)).collect());
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("playlists: could not follow event {}: {e:#}", entry.id),
        }
    }

    /// Handle what the journal shows as not done, or not put back (a restart, a failed write).
    pub(super) async fn catch_up(&self) {
        if self.lib.store_opt().is_none() {
            return;
        }
        match self.journal.pending(PLAYLISTS, 0) {
            Ok(list) => {
                for e in &list {
                    self.handle(e).await;
                }
            }
            Err(e) => tracing::warn!("journal: {e:#}"),
        }
        match self.journal.undone_since(PLAYLISTS, PLAYLISTS_UNDONE, 0) {
            Ok(list) => {
                for e in &list {
                    self.handle_undo(e).await;
                }
            }
            Err(e) => tracing::warn!("journal: {e:#}"),
        }
    }

    /// Whether an album action is under way or its lines not yet followed: the playlist files may name files that
    /// moved, which the sync must not take for removed tracks.
    pub(super) fn busy(&self) -> bool {
        let intents = self.journal.intents().map(|l| !l.is_empty()).unwrap_or(true);
        intents || self.journal.pending(PLAYLISTS, 0).map(|l| !l.is_empty()).unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::TrackMove;

    struct Dir(PathBuf);

    impl Dir {
        fn new(tag: &str) -> Self {
            let d = std::env::temp_dir().join(format!("cratarr-follow-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(d.join("Music")).unwrap();
            Self(d)
        }
        fn folders(&self) -> Vec<PathBuf> {
            vec![self.0.join("Music"), self.0.join("incoming")]
        }
        fn write(&self, name: &str, lines: &[&str]) {
            std::fs::write(self.0.join("Music").join(format!("{name}.m3u")), lines.join("\n") + "\n").unwrap();
        }
        fn read(&self, name: &str) -> Vec<String> {
            std::fs::read_to_string(self.0.join("Music").join(format!("{name}.m3u"))).unwrap().lines().map(str::to_string).collect()
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn mv(track: i64, from: (i64, &str), to: (i64, &str)) -> TrackMove {
        TrackMove { track: Some(track), from: (from.0, from.1.into()), to: (to.0, to.1.into()), was: None }
    }

    fn adopted() -> Event {
        Event::AlbumAdopted {
            source: "jellyfin".into(),
            album_id: "a".into(),
            title: "New".into(),
            from_dir: (1, "X/New".into()),
            to_dir: (0, "X - New (2020)".into()),
            moves: vec![mv(7, (1, "X/New/01.flac"), (0, "X - New (2020)/01.flac")), mv(8, (1, "X/New/02.flac"), (0, "X - New (2020)/02.flac"))],
            release_group: None,
            converted: Vec::new(),
        }
    }

    fn rejected() -> Event {
        Event::AlbumRejected {
            source: "jellyfin".into(),
            album_id: "a".into(),
            title: "New".into(),
            folder: 1,
            dir: "X/New".into(),
            token: "t".into(),
            release_group: None,
        }
    }

    #[tokio::test]
    async fn kept_tracks_keep_their_place_in_playlists_and_undo_puts_the_lines_back() {
        let d = Dir::new("keep");
        d.write("Mix", &["A/01.mp3", "../incoming/X/New/02.flac", "B/01.mp3", "../incoming/X/New/01.flac"]);
        d.write("Other", &["A/01.mp3"]);
        let (store, j) = (Store::new(d.0.join("Music")), Journal::open(None).unwrap());
        let id = j.record(100, &adopted()).unwrap();
        let entry = j.get(id).unwrap().unwrap();
        assert_eq!(apply(&[&store], &d.folders(), &j, &entry).await.unwrap(), vec!["Mix".to_string()]);
        assert_eq!(d.read("Mix"), ["A/01.mp3", "X - New (2020)/02.flac", "B/01.mp3", "X - New (2020)/01.flac"]);
        assert!(apply(&[&store], &d.folders(), &j, &entry).await.unwrap().is_empty(), "once");
        assert!(j.pending(PLAYLISTS, 0).unwrap().is_empty());
        j.mark_undone(id, 200).unwrap();
        let entry = j.get(id).unwrap().unwrap();
        assert_eq!(undo(&[&store], &d.folders(), &j, &entry).await.unwrap(), vec!["Mix".to_string()]);
        assert_eq!(d.read("Mix"), ["A/01.mp3", "../incoming/X/New/02.flac", "B/01.mp3", "../incoming/X/New/01.flac"]);
        assert!(undo(&[&store], &d.folders(), &j, &entry).await.unwrap().is_empty(), "once");
    }

    #[tokio::test]
    async fn a_deleted_albums_tracks_leave_every_playlist_and_come_back_where_they_were() {
        let d = Dir::new("delete");
        d.write("Mix", &["A/01.mp3", "../incoming/X/New/02.flac", "B/01.mp3", "../incoming/X/New/01.flac", "../incoming/X/Newer/01.flac"]);
        let (store, j) = (Store::new(d.0.join("Music")), Journal::open(None).unwrap());
        let id = j.record(100, &rejected()).unwrap();
        let entry = j.get(id).unwrap().unwrap();
        apply(&[&store], &d.folders(), &j, &entry).await.unwrap();
        assert_eq!(d.read("Mix"), ["A/01.mp3", "B/01.mp3", "../incoming/X/Newer/01.flac"], "only that album's folder");
        // Edited meanwhile: the lines come back at their old positions, as far as the playlist reaches.
        store.set_member("Mix", "C/01.mp3", true).await.unwrap();
        j.mark_undone(id, 200).unwrap();
        undo(&[&store], &d.folders(), &j, &j.get(id).unwrap().unwrap()).await.unwrap();
        assert_eq!(
            d.read("Mix"),
            ["A/01.mp3", "../incoming/X/New/02.flac", "B/01.mp3", "../incoming/X/New/01.flac", "../incoming/X/Newer/01.flac", "C/01.mp3"]
        );
    }

    #[tokio::test]
    async fn a_delete_cut_short_removes_the_planned_lines_and_undo_still_works() {
        let d = Dir::new("crash");
        // As if a crash came after the plan was recorded and the line taken out, before the done row: the retry reads
        // the plan, not the file (which no longer shows where the line stood).
        d.write("Mix", &["A/01.mp3"]);
        let (store, j) = (Store::new(d.0.join("Music")), Journal::open(None).unwrap());
        let id = j.record(100, &rejected()).unwrap();
        let plan: Removed = [("Mix".to_string(), vec![(0, "../incoming/X/New/01.flac".to_string())])].into();
        j.done(id, PLAYLISTS_PLAN, 100, &serde_json::to_value(&plan).unwrap()).unwrap();
        apply(&[&store], &d.folders(), &j, &j.get(id).unwrap().unwrap()).await.unwrap();
        assert_eq!(d.read("Mix"), ["A/01.mp3"]);
        assert!(j.pending(PLAYLISTS, 0).unwrap().is_empty());
        j.mark_undone(id, 200).unwrap();
        undo(&[&store], &d.folders(), &j, &j.get(id).unwrap().unwrap()).await.unwrap();
        assert_eq!(d.read("Mix"), ["../incoming/X/New/01.flac", "A/01.mp3"]);
    }

    #[test]
    fn a_replacement_takes_over_the_old_files_lines_and_undo_gives_them_back() {
        let folders = vec![PathBuf::from("/m/Music"), PathBuf::from("/m/incoming")];
        let event = Event::AlbumReplaced {
            source: "jellyfin".into(),
            album_id: "a".into(),
            title: "Up".into(),
            from_dir: (1, "X/Up".into()),
            to_dir: (0, "X - Up".into()),
            moves: vec![mv(7, (1, "X/Up/01.flac"), (0, "X - Up/01.flac")), mv(9, (1, "X/Up/99.flac"), (0, "X - Up/99.flac"))],
            replaced: vec![mv(7, (0, "X - Up/01.mp3"), (1, ".trash/X - Up/01.mp3"))],
            token: "t".into(),
            release_group: None,
            converted: Vec::new(),
        };
        let forward = renames(&event, &folders, false);
        assert_eq!(forward.get("X - Up/01.mp3").map(String::as_str), Some("X - Up/01.flac"));
        assert_eq!(forward.get("../incoming/X/Up/01.flac").map(String::as_str), Some("X - Up/01.flac"));
        let back = renames(&event, &folders, true);
        assert_eq!(back.get("X - Up/01.flac").map(String::as_str), Some("X - Up/01.mp3"), "to the old file");
        assert_eq!(back.get("X - Up/99.flac").map(String::as_str), Some("../incoming/X/Up/99.flac"), "a new track goes back to Incoming");
    }

    #[test]
    fn lines_go_back_in_order_and_are_never_doubled() {
        let lines = vec![(0, "a".to_string()), (2, "c".to_string()), (9, "z".to_string())];
        assert_eq!(put_back(&["b".into(), "d".into()], &lines).unwrap(), ["a", "b", "c", "d", "z"]);
        assert_eq!(put_back(&["a".into(), "c".into(), "z".into()], &lines), None);
        assert_eq!(without(&["b".into()], &lines), None);
    }
}
