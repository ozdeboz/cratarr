//! The playlist sync (decision 0012): each playlist file and the chosen user's own playlist on the server are two
//! copies of one playlist, kept in step both ways. A pass compares each copy with the list both last agreed on and
//! merges (`playlist_merge`). Only servers with a playlist owner set take part.
//!
//! Every step is safe to repeat and to interrupt (the next pass finishes or undoes it, never guesses):
//! - a link is matched by the server copy's id, and remembers both agreed names, so a rename made on either side is
//!   told apart from a deletion even when the pass that carried it out was cut short;
//! - a rename made in Cratarr is recorded before the file is renamed;
//! - a listing on either side that suddenly lacks most linked playlists is not believed (no mass deletion);
//! - a file edited while a pass merged is left for the next pass;
//! - a server copy made but not yet remembered is found again by its name, not made twice;
//! - a file deleted because its server copy went can be restored in Cratarr.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, bail};

use super::library::Library;
use crate::catalogue::Catalogue;
use crate::playlist_merge::{Item, Newer, merge};
use crate::playlists::Store;
use crate::source::Source;
use crate::sync_state::{Link, SyncState};

/// The server field naming whose playlists these are.
pub(super) const PLAYLIST_USER: &str = "playlist_user";

pub struct SyncService {
    lib: Arc<Library>,
    state: SyncState,
    one_at_a_time: tokio::sync::Mutex<()>,
}

impl SyncService {
    pub(super) fn new(lib: Arc<Library>, state: SyncState) -> Self {
        Self { lib, state, one_at_a_time: tokio::sync::Mutex::new(()) }
    }

    /// One pass over every connected server with a playlist owner set. The first one also moves the playlist files
    /// out of Jellyfin's reach (and finishes that move if a restart cut it short): part of the program, so deploying
    /// a version is all a migration takes (decision 0012).
    pub async fn pass(&self) {
        let _one = self.one_at_a_time.lock().await;
        let owned = self.lib.connected().iter().any(|(a, _)| crate::source::text(&a.cfg.values, PLAYLIST_USER).is_some());
        if !owned {
            return;
        }
        if let Err(e) = self.hide_playlist_files().await {
            tracing::warn!("playlist sync: cannot move the playlist files out of Jellyfin's reach: {e:#}");
            return;
        }
        let Some(store) = self.lib.store_opt() else { return };
        for (a, s) in self.lib.connected() {
            let Some(user) = crate::source::text(&a.cfg.values, PLAYLIST_USER).map(str::to_string) else { continue };
            if let Err(e) = sync_server(&store, self.lib.catalogue(), &*s, &self.state, &a.cfg.id, &user).await {
                tracing::warn!("playlist sync with {}: {e:#}", a.cfg.id);
            }
        }
    }

    /// Rename a playlist made in Cratarr, keeping its server copies: the new name is recorded first, then `rename`
    /// (the file) runs, and the next pass gives the server copies the new name. Cut short after the record, the next
    /// pass finds the file under its old name and goes back to it; the playlist is never taken as deleted.
    pub async fn rename<F, E>(&self, from: &str, to: &str, rename: F) -> Result<(), E>
    where
        F: std::future::Future<Output = Result<(), E>>,
    {
        let _one = self.one_at_a_time.lock().await;
        let recorded = self.state.rename(from, to).unwrap_or_else(|e| {
            tracing::warn!("playlist sync: {e:#}");
            0
        });
        let done = rename.await;
        if done.is_err() && recorded > 0 {
            if let Err(e) = self.state.rename(to, from) {
                tracing::warn!("playlist sync: {e:#}");
            }
        }
        done
    }
}

impl SyncService {
    /// Move the playlist files into `Music/.playlists` (until none are left in Music), then read them from there and
    /// tell the servers the imported playlists are gone. Safe to run at every pass: with nothing left to move it only
    /// lists the Music folder.
    async fn hide_playlist_files(&self) -> Result<()> {
        let Some(music) = self.lib.music() else { return Ok(()) };
        let m = PathBuf::from(&music);
        let moved = blocking(move || crate::playlists::move_into_hidden_folder(&m)).await??;
        if !moved.conflicts.is_empty() {
            bail!("in both Music and {}, with different content: {}", crate::playlists::HIDDEN_DIR, moved.conflicts.join(", "));
        }
        if self.lib.store_opt().is_some_and(|s| !s.is_hidden()) {
            self.lib.reopen_store();
        }
        let gone: Vec<String> = moved.moved.iter().chain(&moved.already).map(|n| n.trim_end_matches(".m3u").to_string()).collect();
        if gone.is_empty() {
            return Ok(());
        }
        tracing::info!("moved {} playlist file(s) into {}; the server copies take over", gone.len(), crate::playlists::HIDDEN_DIR);
        for (a, s) in self.lib.connected() {
            let events: Vec<crate::source::PlaylistEvent> = gone
                .iter()
                .map(|name| crate::source::PlaylistEvent {
                    name: name.clone(),
                    path: a.placement.playlist_path(&music, name),
                    change: crate::source::PlaylistChange::Deleted,
                })
                .collect();
            if let Err(e) = s.playlists_changed(events).await {
                tracing::warn!("playlist sync: telling {} the imported playlists are gone: {e:#}", a.cfg.id);
            }
        }
        Ok(())
    }
}

/// Lines, track ids and server ids, through the catalogue. Blocking.
struct Translate {
    cat: Arc<Catalogue>,
    source: String,
    /// The catalogue's folders; 0 is the Music folder, which playlist lines are relative to.
    folders: Vec<PathBuf>,
}

impl Translate {
    fn items(&self, lines: &[String]) -> Vec<Item> {
        lines.iter().map(|l| self.cat.track_of(0, l).map_or_else(|| Item::Missing(l.clone()), Item::Track)).collect()
    }

    /// A track's playlist line: its path relative to the Music folder.
    fn line(&self, track: i64) -> Option<String> {
        let (folder, rel) = self.cat.file_of(track)?;
        if folder == 0 {
            return Some(rel);
        }
        relative_to(self.folders.first()?, &self.folders.get(folder as usize)?.join(rel))
    }

    fn lines(&self, items: &[Item]) -> Vec<String> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::Track(t) => self.line(*t),
                Item::Missing(l) => Some(l.clone()),
            })
            .collect()
    }

    fn server_id(&self, track: i64) -> Option<String> {
        self.cat.source_id_of(&self.source, track)
    }

    /// The server's list as track ids, and the items Cratarr does not know, each with the position it had (kept on
    /// the server as they are; the file cannot hold them).
    fn from_server(&self, ids: &[String]) -> (Vec<i64>, Vec<(usize, String)>) {
        let mut tracks = Vec::new();
        let mut unknown = Vec::new();
        for (i, id) in ids.iter().enumerate() {
            match self.cat.track_of_source(&self.source, id) {
                Some(t) => tracks.push(t),
                None => unknown.push((i, id.clone())),
            }
        }
        (tracks, unknown)
    }

    /// The tracks of a merged list that the server knows, as (track, server id).
    fn on_server(&self, items: &[Item]) -> Vec<(i64, String)> {
        items
            .iter()
            .filter_map(|i| if let Item::Track(t) = i { self.server_id(*t).map(|s| (*t, s)) } else { None })
            .collect()
    }
}

/// `target` as a path relative to the folder `base` ("../lidarr-music/A/01.flac").
fn relative_to(base: &Path, target: &Path) -> Option<String> {
    let (b, t): (Vec<_>, Vec<_>) = (base.components().collect(), target.components().collect());
    let common = b.iter().zip(&t).take_while(|(x, y)| x == y).count();
    if common == 0 {
        return None;
    }
    let mut parts: Vec<String> = std::iter::repeat_n("..".to_string(), b.len() - common).collect();
    parts.extend(t[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    Some(parts.join("/"))
}

/// Put the server items Cratarr does not know back where they were.
fn with_unknown(mut ids: Vec<String>, unknown: &[(usize, String)]) -> Vec<String> {
    for (at, id) in unknown {
        ids.insert((*at).min(ids.len()), id.clone());
    }
    ids
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| anyhow::anyhow!("{e}"))
}

/// Whether a listing that lost most of what is linked can be believed: not when four or more are linked and more
/// than half are gone at once (an unmounted folder, a server answering for the wrong user).
fn believable(linked: usize, lost: usize) -> bool {
    linked < 4 || lost * 2 <= linked
}

/// One pass for one server and user.
pub(super) async fn sync_server(
    store: &Store,
    cat: Arc<Catalogue>,
    s: &dyn Source,
    state: &SyncState,
    source_id: &str,
    user: &str,
) -> Result<()> {
    let own = s.own_playlists(user).await?;
    let files = store.snapshot().await?;
    let has_file = |n: &str| files.iter().any(|(f, _, _)| f == n);
    let mut links = state.links(source_id, user)?;

    // 1. Names first. A link whose file is not under the name it has here: if the server copy's current name is a
    //    file no other link holds, that is the file (a rename made on either side, carried out or not).
    let linked_names: Vec<String> = links.iter().map(|l| l.name.clone()).collect();
    for l in links.iter_mut().filter(|l| !has_file(&l.name)) {
        if let Some(r) = own.iter().find(|o| o.id == l.remote)
            && has_file(&r.name)
            && !linked_names.contains(&r.name)
        {
            state.rename(&l.name, &r.name)?;
            l.name = r.name.clone();
        }
    }
    let lost_on_server = links.iter().filter(|l| !own.iter().any(|o| o.id == l.remote)).count();
    let lost_here = links.iter().filter(|l| !has_file(&l.name)).count();
    if !believable(links.len(), lost_on_server) {
        bail!("the server lists only {} of {} linked playlists; not believed, nothing changed", links.len() - lost_on_server, links.len());
    }
    if !believable(links.len(), lost_here) {
        bail!("only {} of {} linked playlist files are here; not believed, nothing changed", links.len() - lost_here, links.len());
    }
    let tr = Arc::new(Translate { folders: cat.folders(), cat, source: source_id.to_string() });

    // 2. Each linked playlist present on both sides: names, then contents.
    for link in links.clone() {
        let Some(remote) = own.iter().find(|o| o.id == link.remote) else { continue };
        let Some((_, lines, _)) = files.iter().find(|(n, _, _)| n == &link.name) else { continue };
        let mut link = link;
        if remote.name != link.remote_name {
            // Renamed on the server (it wins over a rename here not yet carried over): the file follows.
            if remote.name != link.name {
                match store.rename(&link.name, &remote.name).await {
                    Ok(()) => {}
                    Err(e) => {
                        tracing::warn!("playlist sync: cannot rename {} to {:?}: {e:?}", link.name, remote.name);
                        continue;
                    }
                }
                state.rename(&link.name, &remote.name)?;
            }
            link.name = remote.name.clone();
            link.remote_name = remote.name.clone();
            state.put(source_id, user, &link)?;
            // Its content is looked at next pass, under the new name.
            continue;
        }
        if link.name != link.remote_name {
            // Renamed here: the server copy follows.
            s.set_playlist(&remote.id, Some(&link.name), None).await?;
            link.remote_name = link.name.clone();
            link.stamp = None;
            state.put(source_id, user, &link)?;
        }
        sync_content(store, s, state, &tr, source_id, user, &link, remote.stamp.clone(), lines).await?;
    }

    // 3. Gone on one side: gone on the other.
    for l in &links {
        let on_server = own.iter().any(|o| o.id == l.remote);
        match (has_file(&l.name), on_server) {
            (true, false) => {
                tracing::info!("playlist sync: {} was deleted on {source_id}; deleting it here too", l.name);
                match store.delete(&l.name).await {
                    Ok(()) | Err(crate::playlists::StoreError::Unknown) => state.remove(source_id, user, &l.name)?,
                    Err(e) => tracing::warn!("playlist sync: cannot delete {}: {e:?}", l.name),
                }
            }
            (false, true) => {
                tracing::info!("playlist sync: {} was deleted here; deleting it on {source_id}", l.name);
                match s.delete_playlist(&l.remote).await {
                    Ok(()) => state.remove(source_id, user, &l.name)?,
                    // Kept: the next pass tries again (or finds it gone).
                    Err(e) => tracing::warn!("playlist sync: deleting {} on {source_id}: {e:#}", l.name),
                }
            }
            (false, false) => state.remove(source_id, user, &l.name)?,
            (true, true) => {}
        }
    }

    // 4. New on one side: a copy on the other.
    let linked_remotes: Vec<String> = links.iter().map(|l| l.remote.clone()).collect();
    let linked_names: Vec<String> = links.iter().map(|l| l.name.clone()).collect();
    for (name, lines, _) in files.iter().filter(|(n, _, _)| !linked_names.contains(n)) {
        // A server copy of that name nobody holds: made before this pass could remember it, or on both sides. Linked
        // with no agreed list, so the first merge keeps everything from both.
        if let Some(o) = own.iter().find(|o| &o.name == name && !linked_remotes.contains(&o.id)) {
            let link = Link { name: name.clone(), remote: o.id.clone(), remote_name: o.name.clone(), stamp: None, base: Vec::new() };
            state.put(source_id, user, &link)?;
            sync_content(store, s, state, &tr, source_id, user, &link, o.stamp.clone(), lines).await?;
            continue;
        }
        let (t, l) = (tr.clone(), lines.clone());
        let on_server = blocking(move || {
            let items = t.items(&l);
            t.on_server(&items)
        })
        .await?;
        let ids: Vec<String> = on_server.iter().map(|(_, s)| s.clone()).collect();
        let remote = s.create_playlist(user, name, &ids).await?;
        let base = on_server.into_iter().map(|(t, _)| t).collect();
        state.put(source_id, user, &Link { name: name.clone(), remote, remote_name: name.clone(), stamp: None, base })?;
    }
    for o in own.iter().filter(|o| !linked_remotes.contains(&o.id) && !has_file(&o.name)) {
        // Made on the server (in Finamp, say): a file. Remembered before its tracks are written, so a cut-short pass
        // finds file and link again instead of making a second file.
        match store.create(&o.name).await {
            Ok(()) | Err(crate::playlists::StoreError::Exists) => {}
            Err(e) => {
                tracing::warn!("playlist sync: cannot make a file for {:?}: {e:?}", o.name);
                continue;
            }
        }
        let link = Link { name: o.name.clone(), remote: o.id.clone(), remote_name: o.name.clone(), stamp: None, base: Vec::new() };
        state.put(source_id, user, &link)?;
        sync_content(store, s, state, &tr, source_id, user, &link, o.stamp.clone(), &[]).await?;
    }
    Ok(())
}

/// Bring one linked pair's contents in step (names already agree).
#[allow(clippy::too_many_arguments)]
async fn sync_content(
    store: &Store,
    s: &dyn Source,
    state: &SyncState,
    tr: &Arc<Translate>,
    source_id: &str,
    user: &str,
    link: &Link,
    stamp: Option<String>,
    lines: &[String],
) -> Result<()> {
    let server_changed = stamp != link.stamp || link.stamp.is_none();
    let (t, l) = (tr.clone(), lines.to_vec());
    let items = blocking(move || t.items(&l)).await?;
    let (server_tracks, unknown) = if server_changed {
        let ids = s.playlist_tracks(user, &link.remote).await?;
        let t = tr.clone();
        blocking(move || t.from_server(&ids)).await?
    } else {
        (link.base.clone(), Vec::new())
    };
    let (t, its) = (tr.clone(), items.clone());
    let file_on_server: Vec<i64> = blocking(move || t.on_server(&its).into_iter().map(|(t, _)| t).collect()).await?;
    if !server_changed && file_on_server == link.base {
        return Ok(());
    }
    let newer = if server_changed { Newer::Server } else { Newer::File };
    let merged = merge(&link.base, &items, &server_tracks, newer);
    let (t, m) = (tr.clone(), merged.clone());
    let (new_lines, on_server) = blocking(move || (t.lines(&m), t.on_server(&m))).await?;
    if new_lines != lines && !store.set_entries_if(&link.name, lines, &new_lines).await? {
        // Edited while this pass merged: the next pass merges that too.
        return Ok(());
    }
    let server_now: Vec<String> = with_unknown(on_server.iter().map(|(_, s)| s.clone()).collect(), &unknown);
    let server_before: Vec<String> = {
        let (t, before) = (tr.clone(), server_tracks.clone());
        let ids = blocking(move || before.iter().filter_map(|x| t.server_id(*x)).collect::<Vec<_>>()).await?;
        with_unknown(ids, &unknown)
    };
    let wrote_server = server_now != server_before;
    if wrote_server {
        s.set_playlist(&link.remote, None, Some(&server_now)).await?;
    }
    let base = on_server.into_iter().map(|(t, _)| t).collect();
    // After a write the stamp changed: unknown until the next listing, which then finds nothing to do.
    let stamp = if wrote_server { None } else { stamp };
    state.put(source_id, user, &Link { stamp, base, ..link.clone() })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Fut, OwnPlaylist, Playing, PlaylistEvent, Status, Track};
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    /// A server keeping playlists in memory; every change bumps the playlist's stamp.
    #[derive(Default)]
    struct Fake {
        lists: Mutex<Vec<(String, String, Vec<String>, u32)>>, // id, name, items, version
    }

    impl Fake {
        fn edit(&self, name: &str, f: impl FnOnce(&mut Vec<String>)) {
            let mut l = self.lists.lock().unwrap();
            let p = l.iter_mut().find(|p| p.1 == name).unwrap();
            f(&mut p.2);
            p.3 += 1;
        }
        fn items(&self, name: &str) -> Vec<String> {
            self.lists.lock().unwrap().iter().find(|p| p.1 == name).map(|p| p.2.clone()).unwrap_or_default()
        }
    }

    impl Source for Fake {
        fn status(&self) -> Fut<'_, Status> {
            Box::pin(async { Ok(Status::default()) })
        }
        fn options(&self) -> Fut<'_, BTreeMap<String, Vec<crate::source::Choice>>> {
            Box::pin(async { Ok(BTreeMap::new()) })
        }
        fn libraries(&self) -> Fut<'_, Vec<crate::source::Library>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn sample_paths<'a>(&'a self, _id: &'a str, _n: usize) -> Fut<'a, Vec<String>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn now_playing(&self) -> Fut<'_, Option<Playing>> {
            Box::pin(async { Ok(None) })
        }
        fn track<'a>(&'a self, _id: &'a str) -> Fut<'a, Track> {
            Box::pin(async { anyhow::bail!("no") })
        }
        fn playlists_changed(&self, _e: Vec<PlaylistEvent>) -> Fut<'_, ()> {
            Box::pin(async { Ok(()) })
        }
        fn own_playlists<'a>(&'a self, _user: &'a str) -> Fut<'a, Vec<OwnPlaylist>> {
            Box::pin(async move {
                Ok(self
                    .lists
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(id, name, _, v)| OwnPlaylist { id: id.clone(), name: name.clone(), stamp: Some(format!("v{v}")) })
                    .collect())
            })
        }
        fn playlist_tracks<'a>(&'a self, _user: &'a str, id: &'a str) -> Fut<'a, Vec<String>> {
            Box::pin(async move { Ok(self.lists.lock().unwrap().iter().find(|p| p.0 == id).map(|p| p.2.clone()).unwrap_or_default()) })
        }
        fn create_playlist<'a>(&'a self, _user: &'a str, name: &'a str, ids: &'a [String]) -> Fut<'a, String> {
            Box::pin(async move {
                let mut l = self.lists.lock().unwrap();
                let id = format!("p{}", l.len() + 1);
                l.push((id.clone(), name.to_string(), ids.to_vec(), 1));
                Ok(id)
            })
        }
        fn set_playlist<'a>(&'a self, id: &'a str, name: Option<&'a str>, ids: Option<&'a [String]>) -> Fut<'a, ()> {
            Box::pin(async move {
                let mut l = self.lists.lock().unwrap();
                let p = l.iter_mut().find(|p| p.0 == id).unwrap();
                if let Some(n) = name {
                    p.1 = n.to_string();
                }
                if let Some(ids) = ids {
                    p.2 = ids.to_vec();
                }
                p.3 += 1;
                Ok(())
            })
        }
        fn delete_playlist<'a>(&'a self, id: &'a str) -> Fut<'a, ()> {
            Box::pin(async move {
                self.lists.lock().unwrap().retain(|p| p.0 != id);
                Ok(())
            })
        }
    }

    /// A Music folder with four tracks, catalogued, identified and linked to server ids "s1".."s4".
    fn library() -> (PathBuf, Arc<Catalogue>, Store) {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("cratarr-sync-{}-{n}", std::process::id()));
        std::fs::create_dir_all(d.join("A")).unwrap();
        for i in 1..=4 {
            std::fs::write(d.join(format!("A/0{i}.mp3")), format!("track {i}")).unwrap();
        }
        let cat = Arc::new(Catalogue::open(None, vec![d.clone()]).unwrap());
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        let links: Vec<(String, i64, String)> = (1..=4).map(|i| (format!("s{i}"), 0, format!("A/0{i}.mp3"))).collect();
        cat.record_links("srv", "lib", &links, 1).unwrap();
        let store = Store::new(&d);
        (d, cat, store)
    }

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn files_and_server_playlists_stay_in_step_both_ways() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        store.create("Chill").await.unwrap();
        store.set_entries("Chill", &lines(&["A/01.mp3", "gone/x.mp3", "A/02.mp3"])).await.unwrap();
        let sync = || sync_server(&store, cat.clone(), &srv, &state, "srv", "me");

        // First pass: the server copy is made, with what the server knows (not the missing track).
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s1", "s2"]));

        // Added in Finamp: the file gets it, the missing line stays where it was.
        srv.edit("Chill", |v| v.push("s3".into()));
        sync().await.unwrap();
        assert_eq!(store.entries("Chill").await.unwrap(), lines(&["A/01.mp3", "gone/x.mp3", "A/02.mp3", "A/03.mp3"]));

        // Removed in Cratarr: gone from the server too.
        store.set_entries("Chill", &lines(&["A/01.mp3", "gone/x.mp3", "A/03.mp3"])).await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s1", "s3"]));

        // Nothing changed: nothing is written.
        let before = srv.lists.lock().unwrap()[0].3;
        sync().await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.lists.lock().unwrap()[0].3, before);

        // Reordered in Finamp: the file follows.
        srv.edit("Chill", |v| v.reverse());
        sync().await.unwrap();
        assert_eq!(store.entries("Chill").await.unwrap(), lines(&["A/03.mp3", "A/01.mp3", "gone/x.mp3"]));
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn made_renamed_and_deleted_on_either_side() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        let sync = || sync_server(&store, cat.clone(), &srv, &state, "srv", "me");
        // Made in Finamp: a file appears.
        srv.create_playlist("me", "Road", &lines(&["s4", "s1"])).await.unwrap();
        sync().await.unwrap();
        assert_eq!(store.entries("Road").await.unwrap(), lines(&["A/04.mp3", "A/01.mp3"]));
        // Renamed in Finamp: the file follows, keeping its content.
        srv.set_playlist("p1", Some("Drive"), None).await.unwrap();
        sync().await.unwrap();
        assert_eq!(store.entries("Drive").await.unwrap(), lines(&["A/04.mp3", "A/01.mp3"]));
        assert!(store.entries("Road").await.is_err());
        // Deleted in Cratarr: deleted on the server.
        store.delete("Drive").await.unwrap();
        sync().await.unwrap();
        assert!(srv.lists.lock().unwrap().is_empty());
        // Deleted in Finamp: the file goes (and can be restored in Cratarr).
        store.create("Gym").await.unwrap();
        sync().await.unwrap();
        let id = srv.lists.lock().unwrap()[0].0.clone();
        srv.delete_playlist(&id).await.unwrap();
        sync().await.unwrap();
        assert!(store.entries("Gym").await.is_err());
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn an_interrupted_rename_is_finished_or_undone_never_taken_for_a_deletion() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        let sync = || sync_server(&store, cat.clone(), &srv, &state, "srv", "me");
        store.create("Chill").await.unwrap();
        sync().await.unwrap();
        // Renamed on the server, and the pass that renamed the file was cut short before it remembered.
        srv.set_playlist("p1", Some("Calm"), None).await.unwrap();
        store.rename("Chill", "Calm").await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.lists.lock().unwrap().len(), 1, "not deleted, not made twice");
        assert!(store.entries("Calm").await.is_ok());
        // A rename here recorded, but the file rename never happened: back to the file's name, nothing lost.
        state.rename("Calm", "Quiet").unwrap();
        sync().await.unwrap();
        assert!(store.entries("Calm").await.is_ok());
        assert_eq!(srv.lists.lock().unwrap()[0].1, "Calm");
        // A rename here carried out: the server copy follows.
        state.rename("Calm", "Quiet").unwrap();
        store.rename("Calm", "Quiet").await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.lists.lock().unwrap()[0].1, "Quiet");
        assert_eq!(srv.lists.lock().unwrap().len(), 1);
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn a_server_copy_made_but_not_remembered_is_not_made_twice() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        store.create("Chill").await.unwrap();
        store.set_entries("Chill", &lines(&["A/01.mp3"])).await.unwrap();
        // The server copy exists (made by a pass cut short before remembering it).
        srv.create_playlist("me", "Chill", &lines(&["s1"])).await.unwrap();
        sync_server(&store, cat.clone(), &srv, &state, "srv", "me").await.unwrap();
        sync_server(&store, cat.clone(), &srv, &state, "srv", "me").await.unwrap();
        assert_eq!(srv.lists.lock().unwrap().len(), 1);
        assert_eq!(srv.items("Chill"), lines(&["s1"]));
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn files_that_suddenly_vanished_do_not_delete_the_server_copies() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        for n in ["A1", "A2", "A3", "A4"] {
            store.create(n).await.unwrap();
        }
        sync_server(&store, cat.clone(), &srv, &state, "srv", "me").await.unwrap();
        for n in ["A1", "A2", "A3"] {
            std::fs::remove_file(d.join(format!("{n}.m3u"))).unwrap();
        }
        assert!(sync_server(&store, cat.clone(), &srv, &state, "srv", "me").await.is_err());
        assert_eq!(srv.lists.lock().unwrap().len(), 4);
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn a_listing_that_lost_most_playlists_is_not_believed() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        for n in ["A1", "A2", "A3", "A4"] {
            store.create(n).await.unwrap();
        }
        sync_server(&store, cat.clone(), &srv, &state, "srv", "me").await.unwrap();
        srv.lists.lock().unwrap().truncate(1);
        assert!(sync_server(&store, cat.clone(), &srv, &state, "srv", "me").await.is_err());
        for n in ["A1", "A2", "A3", "A4"] {
            assert!(store.entries(n).await.is_ok(), "{n} must survive a server that lost its playlists");
        }
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn paths_relative_to_the_music_folder() {
        assert_eq!(relative_to(Path::new("/media/Music"), Path::new("/media/lidarr-music/X/01.flac")).as_deref(), Some("../lidarr-music/X/01.flac"));
        assert_eq!(relative_to(Path::new("/media/Music"), Path::new("/media/Music/A/01.mp3")).as_deref(), Some("A/01.mp3"));
    }
}
