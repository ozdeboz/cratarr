//! Sync links (decisions 0012, 0014): each playlist is one `.m3u` file in `Music/.playlists`, and each of its links
//! keeps a copy of it on a target in step.
//! - **Shared:** a copy in the Music folder, which the media server imports for everyone. Push only.
//! - **A user's own playlist on a server:** kept in step by the engine below, push or two-way. A two-way pass compares
//!   each copy with the list both last agreed on and merges (`playlist_merge`); the link's conflict rule picks the side
//!   that leads.
//!
//! Every step is safe to repeat and to interrupt (the next pass finishes or undoes it, never guesses):
//! - a link is matched by the server copy's id, and remembers both agreed names, so a rename made on either side is
//!   told apart from a deletion even when the pass that carried it out was cut short;
//! - a rename made in Cratarr is recorded before the file is renamed;
//! - a listing on either side that suddenly lacks most linked playlists is not believed (no mass deletion);
//! - a file edited while a pass merged is left for the next pass;
//! - a server copy made but not yet remembered is found again by its name, not made twice;
//! - a file deleted because its server copy went can be restored in Cratarr.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, bail};

use super::ToggleError;
use super::library::{Library, epoch_now, label_of};
use crate::catalogue::Catalogue;
use crate::playlist_merge::{Item, Newer, merge};
use crate::playlists::Store;
use crate::source::{PlaylistChange, Source};
use crate::sync_state::{Conflict, Direction, Kind, Link, LinkStatus, LinkView, SyncState, Target};

/// Recorded in the sync state once the playlists in Music were adopted (decision 0014): never again, so the shared
/// copies there are not taken for playlists.
const ADOPTED: &str = "adopted";

/// A target a playlist can be linked to.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TargetChoice {
    pub target: String,
    pub label: String,
    /// Push only (Shared: an imported playlist cannot be edited on the server).
    pub push_only: bool,
}

pub struct SyncService {
    lib: Arc<Library>,
    state: SyncState,
    /// Playlist lines follow album actions (the journal's "playlists" handler), one at a time with the passes.
    follow: super::follow::Follow,
    one_at_a_time: tokio::sync::Mutex<()>,
    /// The last pass per (playlist, target key).
    last: std::sync::Mutex<HashMap<(String, String), LinkStatus>>,
    /// Account names by (source, user id), for the labels.
    names: std::sync::Mutex<HashMap<(String, String), String>>,
}

impl SyncService {
    pub(super) fn new(lib: Arc<Library>, state: SyncState, journal: Arc<crate::journal::Journal>) -> Self {
        let follow = super::follow::Follow::new(lib.clone(), journal);
        Self {
            lib,
            state,
            follow,
            one_at_a_time: tokio::sync::Mutex::new(()),
            last: Default::default(),
            names: Default::default(),
        }
    }

    /// Hear of album actions and their Undo, and make the playlist lines follow at once, for as long as Cratarr runs.
    /// What is missed (a lag, a failed write) the pass catches up.
    pub(super) fn listen(self: &Arc<Self>) {
        use tokio::sync::broadcast::error::RecvError;
        let journal = self.follow.journal();
        for (mut rx, undone) in [(journal.subscribe(), false), (journal.subscribe_undone(), true)] {
            let me = Arc::downgrade(self);
            tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(id) => {
                            let Some(me) = me.upgrade() else { return };
                            let Ok(Some(entry)) = me.follow.journal().get(id) else { continue };
                            let _one = me.one_at_a_time.lock().await;
                            if undone { me.follow.handle_undo(&entry).await } else { me.follow.handle(&entry).await }
                        }
                        Err(RecvError::Lagged(_)) => {}
                        Err(RecvError::Closed) => return,
                    }
                }
            });
        }
    }

    fn record(&self, playlist: &str, kind: &Kind, error: Option<String>) {
        let status = LinkStatus { at: epoch_now(), error };
        self.last.lock().unwrap_or_else(|p| p.into_inner()).insert((playlist.to_string(), kind.key()), status);
    }

    /// One pass: the playlists in Music adopted (once), then every link brought in step.
    pub async fn pass(&self) {
        let _one = self.one_at_a_time.lock().await;
        self.follow.catch_up().await;
        if let Err(e) = self.adopt().await {
            tracing::warn!("playlist sync: cannot adopt the playlists in Music: {e:#}");
            return;
        }
        let Some(store) = self.lib.store_opt() else { return };
        // Mid album action, a line can name a file that just moved: merged now, its track would leave the server copy.
        if self.follow.busy() {
            return;
        }
        let targets = match self.state.targets() {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("playlist sync: {e:#}");
                return;
            }
        };
        self.push_shared(&store, targets.iter().filter(|t| t.kind == Kind::Shared).cloned().collect()).await;
        let users: BTreeSet<(String, String)> = targets
            .iter()
            .filter_map(|t| match &t.kind {
                Kind::User { source, user } => Some((source.clone(), user.clone())),
                Kind::Shared => None,
            })
            .collect();
        for (a, s) in self.lib.connected() {
            for (_, user) in users.iter().filter(|(source, _)| *source == a.cfg.id) {
                let done = sync_server(&store, self.lib.catalogue(), &*s, &self.state, &a.cfg.id, user).await;
                if let Err(e) = &done {
                    tracing::warn!("playlist sync with {}: {e:#}", a.cfg.id);
                }
                let kind = Kind::User { source: a.cfg.id.clone(), user: user.clone() };
                let error = done.err().map(|e| format!("{e:#}"));
                for t in self.state.targets().unwrap_or_default().iter().filter(|t| t.kind == kind) {
                    self.record(&t.playlist, &kind, error.clone());
                }
            }
        }
    }

    /// Bring the shared copies of these playlists in step now, after an edit (the pass would in 30 s).
    pub(super) fn changed(self: &Arc<Self>, playlists: Vec<String>) {
        let me = self.clone();
        tokio::spawn(async move {
            let _one = me.one_at_a_time.lock().await;
            let Some(store) = me.lib.store_opt() else { return };
            let shared: Vec<Target> =
                me.state.targets().unwrap_or_default().into_iter().filter(|t| t.kind == Kind::Shared && playlists.contains(&t.playlist)).collect();
            me.push_shared(&store, shared).await;
        });
    }

    /// Rename a playlist made in Cratarr, keeping its links: the new name is recorded first, then `rename` (the file)
    /// runs, and the next pass gives the copies the new name. Cut short after the record, the next pass finds the file
    /// under its old name and goes back to it; the playlist is never taken as deleted.
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

    /// A playlist's links, for the page.
    pub(super) fn links_of(&self, playlist: &str) -> Vec<LinkView> {
        let last = self.last.lock().unwrap_or_else(|p| p.into_inner()).clone();
        self.state
            .targets()
            .unwrap_or_default()
            .into_iter()
            .filter(|t| t.playlist == playlist)
            .map(|t| LinkView {
                target: t.kind.key(),
                label: self.label(&t.kind),
                direction: t.direction,
                conflict: t.conflict,
                status: last.get(&(t.playlist.clone(), t.kind.key())).cloned(),
            })
            .collect()
    }

    /// Every playlist's links, by playlist.
    pub(super) fn links(&self) -> HashMap<String, Vec<LinkView>> {
        let mut out: HashMap<String, Vec<LinkView>> = HashMap::new();
        for t in self.state.targets().unwrap_or_default() {
            out.entry(t.playlist.clone()).or_default();
        }
        out.into_keys().map(|p| (p.clone(), self.links_of(&p))).collect()
    }

    fn label(&self, kind: &Kind) -> String {
        match kind {
            Kind::Shared => "Shared".into(),
            Kind::User { source, user } => {
                let name = self.names.lock().unwrap_or_else(|p| p.into_inner()).get(&(source.clone(), user.clone())).cloned();
                let name = name.unwrap_or_else(|| user.clone());
                match self.lib.connected().as_slice() {
                    [_] => name,
                    many => match many.iter().find(|(a, _)| &a.cfg.id == source) {
                        Some((a, _)) => format!("{name} on {}", label_of(&a.cfg)),
                        None => name,
                    },
                }
            }
        }
    }

    /// The other names each playlist is spoken as, by playlist.
    pub(super) fn spoken(&self) -> std::collections::HashMap<String, Vec<String>> {
        self.state.spoken().unwrap_or_default()
    }

    pub(super) fn set_spoken(&self, playlist: &str, names: &[String]) -> Result<(), ToggleError> {
        Ok(self.state.set_spoken(playlist, names)?)
    }

    /// What a playlist can be linked to: Shared, and each account on each connected server that keeps playlists.
    pub(super) async fn choices(&self) -> Vec<TargetChoice> {
        let mut out = vec![TargetChoice { target: Kind::Shared.key(), label: "Shared".into(), push_only: true }];
        for (a, s) in self.lib.connected() {
            match s.users().await {
                Ok(users) => {
                    for (id, name) in users {
                        self.names.lock().unwrap_or_else(|p| p.into_inner()).insert((a.cfg.id.clone(), id.clone()), name);
                        let kind = Kind::User { source: a.cfg.id.clone(), user: id };
                        out.push(TargetChoice { target: kind.key(), label: self.label(&kind), push_only: false });
                    }
                }
                Err(e) => tracing::debug!("{}: no accounts to sync to: {e:#}", a.cfg.id),
            }
        }
        out
    }

    /// Link a playlist to a target, or change how it is kept. The copy is made now (Shared) or by the next pass.
    pub(super) async fn link(
        self: &Arc<Self>,
        playlist: &str,
        kind: Kind,
        direction: Direction,
        conflict: Conflict,
    ) -> Result<(), ToggleError> {
        let store = self.lib.store()?;
        if store.entries(playlist).await.is_err() {
            return Err(ToggleError::UnknownPlaylist);
        }
        let direction = if kind == Kind::Shared { Direction::Push } else { direction };
        if let Kind::User { source, .. } = &kind
            && !self.lib.connected().iter().any(|(a, _)| &a.cfg.id == source)
        {
            return Err(ToggleError::Refused("that server is not connected".into()));
        }
        {
            let _one = self.one_at_a_time.lock().await;
            let copy = self.state.targets()?.into_iter().find(|t| t.playlist == playlist && t.kind == kind).and_then(|t| t.copy);
            self.state.put_target(&Target { playlist: playlist.to_string(), kind, direction, conflict, copy })?;
        }
        let me = self.clone();
        tokio::spawn(async move { me.pass().await });
        Ok(())
    }

    /// A restored playlist (its deletion undone) gets back the links it had, else it is Shared, as a new one.
    pub(super) async fn restore_links(self: &Arc<Self>, playlist: &str) -> Result<(), ToggleError> {
        let revived = {
            let _one = self.one_at_a_time.lock().await;
            self.state.revive_targets(playlist)?
        };
        if revived == 0 {
            return self.link(playlist, Kind::Shared, Direction::Push, Conflict::Newest).await;
        }
        let me = self.clone();
        tokio::spawn(async move { me.pass().await });
        Ok(())
    }

    /// A new playlist: Shared, as playlists were before sync links; links a deleted one of the same name had are not
    /// its.
    pub(super) async fn new_playlist(self: &Arc<Self>, playlist: &str) -> Result<(), ToggleError> {
        self.state.forget_retired(playlist)?;
        self.link(playlist, Kind::Shared, Direction::Push, Conflict::Newest).await
    }

    /// Take a link off: its copy on the target is deleted.
    pub(super) async fn unlink(&self, playlist: &str, kind: &Kind) -> Result<(), ToggleError> {
        let _one = self.one_at_a_time.lock().await;
        let Some(t) = self.state.targets()?.into_iter().find(|t| t.playlist == playlist && &t.kind == kind) else {
            return Ok(());
        };
        match kind {
            Kind::Shared => self.remove_copy(t.copy.as_deref().unwrap_or(playlist)).await?,
            Kind::User { source, user } => {
                if let Some(link) = self.state.links(source, user)?.into_iter().find(|l| l.name == playlist) {
                    let s = self.lib.connected().into_iter().find(|(a, _)| &a.cfg.id == source).map(|(_, s)| s);
                    let s = s.ok_or_else(|| ToggleError::Refused("that server is not connected".into()))?;
                    s.delete_playlist(&link.remote).await.map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
                    self.state.remove(source, user, playlist)?;
                }
            }
        }
        self.state.remove_target(playlist, kind)?;
        self.last.lock().unwrap_or_else(|p| p.into_inner()).remove(&(playlist.to_string(), kind.key()));
        Ok(())
    }

    /// Adopt the playlists in Music (decision 0014), once: each is copied into `.playlists` (unless already there) and
    /// gets a Shared link whose copy is the file it came from, so nothing visible changes.
    async fn adopt(&self) -> Result<()> {
        if self.state.meta(ADOPTED).is_some() {
            return Ok(());
        }
        let Some(music) = self.lib.music() else { return Ok(()) };
        let m = PathBuf::from(&music);
        let names = blocking(move || crate::playlists::adopt_into_playlist_dir(&m)).await??;
        for name in &names {
            let shared = Target {
                playlist: name.clone(),
                kind: Kind::Shared,
                direction: Direction::Push,
                conflict: Conflict::Newest,
                copy: Some(name.clone()),
            };
            self.state.put_target(&shared)?;
        }
        self.state.set_meta(ADOPTED, &epoch_now().to_string())?;
        self.lib.reopen_store();
        tracing::info!("adopted {} playlist(s) from Music into {}, each linked Shared", names.len(), crate::playlists::PLAYLIST_DIR);
        Ok(())
    }

    /// Write the shared copies that differ from their playlist, remove those of renamed or deleted playlists, and tell
    /// the servers which changed.
    async fn push_shared(&self, store: &Store, shared: Vec<Target>) {
        let Some(music) = self.lib.music() else { return };
        for mut t in shared {
            let text = store.text(&t.playlist).await;
            let done: Result<()> = async {
                let Some(text) = text else {
                    // The playlist is gone: its copy goes too.
                    self.remove_copy(t.copy.as_deref().unwrap_or(&t.playlist)).await?;
                    self.state.retire_target(&t.playlist, &Kind::Shared)?;
                    return Ok(());
                };
                if let Some(old) = t.copy.clone().filter(|old| old != &t.playlist) {
                    self.remove_copy(&old).await?;
                    t.copy = None;
                    self.state.put_target(&t)?;
                }
                let path = PathBuf::from(&music).join(format!("{}.m3u", t.playlist));
                let want = crate::m3u::without_prefix(&text, "../");
                let (p, w) = (path.clone(), want.clone());
                let had = blocking(move || std::fs::read(&p).ok()).await?;
                if had.as_deref() != Some(w.as_bytes()) {
                    if had.is_some() && t.copy.as_deref() != Some(t.playlist.as_str()) {
                        bail!("Music already has a file named {}.m3u", t.playlist);
                    }
                    blocking(move || write_whole(&path, &want)).await??;
                    let change = if had.is_some() { PlaylistChange::Edited } else { PlaylistChange::Created };
                    super::playlists::tell_sources(&self.lib, vec![(t.playlist.clone(), change)]);
                }
                if t.copy.as_deref() != Some(t.playlist.as_str()) {
                    t.copy = Some(t.playlist.clone());
                    self.state.put_target(&t)?;
                }
                Ok(())
            }
            .await;
            if let Err(e) = &done {
                tracing::warn!("playlist sync: the shared copy of {}: {e:#}", t.playlist);
            }
            self.record(&t.playlist, &Kind::Shared, done.err().map(|e| format!("{e:#}")));
        }
    }

    /// Remove a shared copy from Music, and tell the servers it is gone.
    async fn remove_copy(&self, name: &str) -> Result<()> {
        let Some(music) = self.lib.music() else { return Ok(()) };
        let path = PathBuf::from(&music).join(format!("{name}.m3u"));
        let removed = blocking(move || match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        })
        .await??;
        if removed {
            super::playlists::tell_sources(&self.lib, vec![(name.to_string(), PlaylistChange::Deleted)]);
        }
        Ok(())
    }
}

/// Write a file whole or not at all: a temporary file next to it, then renamed into place. Blocking.
fn write_whole(path: &Path, text: &str) -> Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.cratarr-tmp"));
    std::fs::write(&tmp, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o664));
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
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

pub(super) use crate::playlists::relative_to;

/// The server's list with the agreed tracks it no longer lists put back where they were, for those `linked` says
/// have no server item now.
fn with_unlinked_kept(base: &[i64], mut listed: Vec<i64>, linked: impl Fn(i64) -> bool) -> Vec<i64> {
    for (at, track) in base.iter().enumerate() {
        if !listed.contains(track) && !linked(*track) {
            listed.insert(at.min(listed.len()), *track);
        }
    }
    listed
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

/// One pass for one server and user: the playlists linked to this account (decision 0014).
pub(super) async fn sync_server(
    store: &Store,
    cat: Arc<Catalogue>,
    s: &dyn Source,
    state: &SyncState,
    source_id: &str,
    user: &str,
) -> Result<()> {
    let kind = Kind::User { source: source_id.to_string(), user: user.to_string() };
    let mine: Vec<Target> = state.targets()?.into_iter().filter(|t| t.kind == kind).collect();
    let target_of = |name: &str| mine.iter().find(|t| t.playlist == name).cloned();
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
        let Some(target) = target_of(&link.name) else { continue };
        let mut link = link;
        if target.direction == Direction::Push && remote.name != link.name {
            // Push: the playlist's name wins, also over a rename made on the server.
            link.remote = s.rename_playlist(user, &remote.id, &link.name).await?;
            link.remote_name = link.name.clone();
            link.stamp = None;
            state.put(source_id, user, &link)?;
            continue;
        }
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
            // Renamed here: the server copy follows (a server may give it a new id doing so).
            link.remote = s.rename_playlist(user, &remote.id, &link.name).await?;
            link.remote_name = link.name.clone();
            link.stamp = None;
            state.put(source_id, user, &link)?;
            // Its content is looked at next pass, under the server's new listing.
            continue;
        }
        sync_content(store, s, state, &tr, source_id, user, &link, remote.stamp.clone(), lines, &target).await?;
    }

    // 3. Gone on one side: gone on the other (two-way), or made again (push).
    for l in &links {
        let on_server = own.iter().any(|o| o.id == l.remote);
        let push = target_of(&l.name).is_some_and(|t| t.direction == Direction::Push);
        match (has_file(&l.name), on_server) {
            (true, false) if push => {
                // Push: the copy is made again (step 4).
                state.remove(source_id, user, &l.name)?;
            }
            (true, false) => {
                tracing::info!("playlist sync: {} was deleted on {source_id}; deleting it here too", l.name);
                match store.delete(&l.name).await {
                    Ok(()) | Err(crate::playlists::StoreError::Unknown) => {
                        state.remove(source_id, user, &l.name)?;
                        state.retire_target(&l.name, &kind)?;
                    }
                    Err(e) => tracing::warn!("playlist sync: cannot delete {}: {e:?}", l.name),
                }
            }
            (false, true) => {
                tracing::info!("playlist sync: {} was deleted here; deleting it on {source_id}", l.name);
                match s.delete_playlist(&l.remote).await {
                    Ok(()) => {
                        state.remove(source_id, user, &l.name)?;
                        state.retire_target(&l.name, &kind)?;
                    }
                    // Kept: the next pass tries again (or finds it gone).
                    Err(e) => tracing::warn!("playlist sync: deleting {} on {source_id}: {e:#}", l.name),
                }
            }
            (false, false) => {
                state.remove(source_id, user, &l.name)?;
                state.retire_target(&l.name, &kind)?;
            }
            (true, true) => {}
        }
    }
    // A link to a playlist deleted before it ever had a copy there.
    for t in mine.iter().filter(|t| !has_file(&t.playlist) && !links.iter().any(|l| l.name == t.playlist)) {
        state.retire_target(&t.playlist, &kind)?;
    }
    // Read again: this pass renamed and deleted (the names at the start would make copies of what is gone).
    let links = state.links(source_id, user)?;
    let mine: Vec<Target> = state.targets()?.into_iter().filter(|t| t.kind == kind).collect();
    let target_of = |name: &str| mine.iter().find(|t| t.playlist == name).cloned();

    // 4. New on one side: a copy on the other.
    let linked_remotes: Vec<String> = links.iter().map(|l| l.remote.clone()).collect();
    let linked_names: Vec<String> = links.iter().map(|l| l.name.clone()).collect();
    for (name, lines, _) in files.iter().filter(|(n, _, _)| !linked_names.contains(n)) {
        let Some(target) = target_of(name) else { continue };
        // A server copy of that name nobody holds: made before this pass could remember it, or on both sides. Linked
        // with no agreed list, so the first merge keeps everything from both.
        if let Some(o) = own.iter().find(|o| &o.name == name && !linked_remotes.contains(&o.id)) {
            let link = Link { name: name.clone(), remote: o.id.clone(), remote_name: o.name.clone(), stamp: None, base: Vec::new() };
            state.put(source_id, user, &link)?;
            sync_content(store, s, state, &tr, source_id, user, &link, o.stamp.clone(), lines, &target).await?;
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
    // Made on the server (in Finamp, say): a new playlist here, linked two-way. Not for an account whose links all
    // push (it only receives).
    let takes_new = mine.is_empty() || mine.iter().any(|t| t.direction == Direction::TwoWay);
    for o in own.iter().filter(|o| takes_new && !linked_remotes.contains(&o.id) && !has_file(&o.name)) {
        // Remembered before its tracks are written, so a cut-short pass finds file and link again instead of making a
        // second file.
        match store.create(&o.name).await {
            Ok(()) | Err(crate::playlists::StoreError::Exists) => {}
            Err(e) => {
                tracing::warn!("playlist sync: cannot make a file for {:?}: {e:?}", o.name);
                continue;
            }
        }
        let target =
            Target { playlist: o.name.clone(), kind: kind.clone(), direction: Direction::TwoWay, conflict: Conflict::Newest, copy: None };
        state.put_target(&target)?;
        let link = Link { name: o.name.clone(), remote: o.id.clone(), remote_name: o.name.clone(), stamp: None, base: Vec::new() };
        state.put(source_id, user, &link)?;
        sync_content(store, s, state, &tr, source_id, user, &link, o.stamp.clone(), &[], &target).await?;
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
    target: &Target,
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
    // A track the server stopped listing while Cratarr has no server item for it was not taken out by anyone: its
    // file moved (Keep, Replace, a restructure) and the server's new item is not linked yet. Read as a removal it
    // would leave the playlist here; it stays, and goes back to the server once linked.
    let server_tracks = if server_changed {
        let (t, base, listed) = (tr.clone(), link.base.clone(), server_tracks);
        blocking(move || with_unlinked_kept(&base, listed, |track| t.server_id(track).is_some())).await?
    } else {
        server_tracks
    };
    let (t, its) = (tr.clone(), items.clone());
    let file_on_server: Vec<i64> = blocking(move || t.on_server(&its).into_iter().map(|(t, _)| t).collect()).await?;
    if !server_changed && file_on_server == link.base {
        return Ok(());
    }
    // Push: the playlist as it is. Two-way: merged, the conflict rule picking the side that leads.
    let merged = match (target.direction, target.conflict) {
        (Direction::Push, _) => items.clone(),
        (Direction::TwoWay, Conflict::Playlist) => merge(&link.base, &items, &server_tracks, Newer::File),
        (Direction::TwoWay, Conflict::Target) => merge(&link.base, &items, &server_tracks, Newer::Server),
        (Direction::TwoWay, Conflict::Newest) => {
            merge(&link.base, &items, &server_tracks, if server_changed { Newer::Server } else { Newer::File })
        }
    };
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
        s.set_playlist_tracks(user, &link.remote, &server_now).await?;
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
        /// A rename made in Finamp (signed in): same id.
        fn rename_in_finamp(&self, id: &str, name: &str) {
            let mut l = self.lists.lock().unwrap();
            let p = l.iter_mut().find(|p| p.0 == id).unwrap();
            p.1 = name.to_string();
            p.3 += 1;
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
        fn set_playlist_tracks<'a>(&'a self, _user: &'a str, id: &'a str, ids: &'a [String]) -> Fut<'a, ()> {
            Box::pin(async move {
                let mut l = self.lists.lock().unwrap();
                let p = l.iter_mut().find(|p| p.0 == id).unwrap();
                p.2 = ids.to_vec();
                p.3 += 1;
                Ok(())
            })
        }
        /// As Jellyfin with an API key: a new playlist under the new name, the old one deleted.
        fn rename_playlist<'a>(&'a self, _user: &'a str, id: &'a str, name: &'a str) -> Fut<'a, String> {
            Box::pin(async move {
                let mut l = self.lists.lock().unwrap();
                let i = l.iter().position(|p| p.0 == id).unwrap();
                let old = l.remove(i);
                let new_id = format!("{}r", old.0);
                l.push((new_id.clone(), name.to_string(), old.2, 1));
                Ok(new_id)
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

    fn me() -> Kind {
        Kind::User { source: "srv".into(), user: "me".into() }
    }

    /// Every playlist here linked two-way to "me" on "srv" (what 0012 did for all of them), then one pass.
    async fn run(store: &Store, cat: &Arc<Catalogue>, srv: &Fake, state: &SyncState) -> Result<()> {
        let linked: Vec<String> = state.targets().unwrap().into_iter().map(|t| t.playlist).collect();
        for p in store.view(None).await.unwrap().into_iter().filter(|p| !linked.contains(&p.id)) {
            let t = Target { playlist: p.id, kind: me(), direction: Direction::TwoWay, conflict: Conflict::Newest, copy: None };
            state.put_target(&t).unwrap();
        }
        sync_server(store, cat.clone(), srv, state, "srv", "me").await
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
        let sync = || run(&store, &cat, &srv, &state);

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
        let sync = || run(&store, &cat, &srv, &state);
        // Made in Finamp: a file appears.
        srv.create_playlist("me", "Road", &lines(&["s4", "s1"])).await.unwrap();
        sync().await.unwrap();
        assert_eq!(store.entries("Road").await.unwrap(), lines(&["A/04.mp3", "A/01.mp3"]));
        // Renamed in Finamp: the file follows, keeping its content.
        srv.rename_in_finamp("p1", "Drive");
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
        let sync = || run(&store, &cat, &srv, &state);
        store.create("Chill").await.unwrap();
        sync().await.unwrap();
        // Renamed on the server, and the pass that renamed the file was cut short before it remembered.
        srv.rename_in_finamp("p1", "Calm");
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
        run(&store, &cat, &srv, &state).await.unwrap();
        run(&store, &cat, &srv, &state).await.unwrap();
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
        run(&store, &cat, &srv, &state).await.unwrap();
        for n in ["A1", "A2", "A3"] {
            std::fs::remove_file(d.join(format!("{n}.m3u"))).unwrap();
        }
        assert!(run(&store, &cat, &srv, &state).await.is_err());
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
        run(&store, &cat, &srv, &state).await.unwrap();
        srv.lists.lock().unwrap().truncate(1);
        assert!(run(&store, &cat, &srv, &state).await.is_err());
        for n in ["A1", "A2", "A3", "A4"] {
            assert!(store.entries(n).await.is_ok(), "{n} must survive a server that lost its playlists");
        }
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn a_push_link_overwrites_what_changed_on_the_server_and_takes_nothing_back() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        store.create("Chill").await.unwrap();
        store.set_entries("Chill", &lines(&["A/01.mp3", "A/02.mp3"])).await.unwrap();
        let push = Target { playlist: "Chill".into(), kind: me(), direction: Direction::Push, conflict: Conflict::Newest, copy: None };
        state.put_target(&push).unwrap();
        let sync = || sync_server(&store, cat.clone(), &srv, &state, "srv", "me");
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s1", "s2"]));
        // Changed in Finamp: put back as the playlist has it; the file is untouched.
        srv.edit("Chill", |v| v.push("s3".into()));
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s1", "s2"]));
        assert_eq!(store.entries("Chill").await.unwrap(), lines(&["A/01.mp3", "A/02.mp3"]));
        // Renamed in Finamp: named back.
        srv.rename_in_finamp("p1", "Calm");
        sync().await.unwrap();
        assert_eq!(srv.lists.lock().unwrap()[0].1, "Chill");
        assert!(store.entries("Calm").await.is_err());
        // Deleted in Finamp: made again.
        let id = srv.lists.lock().unwrap()[0].0.clone();
        srv.delete_playlist(&id).await.unwrap();
        sync().await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s1", "s2"]));
        assert!(store.entries("Chill").await.is_ok(), "the playlist stays");
        // Made in Finamp: not taken in, for an account that only receives.
        srv.create_playlist("me", "Road", &lines(&["s4"])).await.unwrap();
        sync().await.unwrap();
        assert!(store.entries("Road").await.is_err());
        // Only linked playlists take part.
        store.create("Unlinked").await.unwrap();
        sync().await.unwrap();
        assert!(!srv.lists.lock().unwrap().iter().any(|p| p.1 == "Unlinked"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn a_conflict_rule_picks_the_side_that_leads() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        store.create("Chill").await.unwrap();
        store.set_entries("Chill", &lines(&["A/01.mp3", "A/02.mp3"])).await.unwrap();
        let mut t = Target { playlist: "Chill".into(), kind: me(), direction: Direction::TwoWay, conflict: Conflict::Playlist, copy: None };
        state.put_target(&t).unwrap();
        let sync = || sync_server(&store, cat.clone(), &srv, &state, "srv", "me");
        sync().await.unwrap();
        // Both reordered: the playlist's order leads.
        srv.edit("Chill", |v| v.reverse());
        store.set_entries("Chill", &lines(&["A/02.mp3", "A/01.mp3", "A/03.mp3"])).await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s2", "s1", "s3"]));
        // The target leads instead.
        t.conflict = Conflict::Target;
        state.put_target(&t).unwrap();
        srv.edit("Chill", |v| *v = vec!["s3".into(), "s1".into(), "s2".into()]);
        store.set_entries("Chill", &lines(&["A/01.mp3", "A/02.mp3", "A/03.mp3"])).await.unwrap();
        sync().await.unwrap();
        assert_eq!(store.entries("Chill").await.unwrap(), lines(&["A/03.mp3", "A/01.mp3", "A/02.mp3"]));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn unlinked_tracks_the_server_dropped_go_back_where_they_were() {
        let linked = |t: i64| t != 2;
        // 2 has no server item now: put back. 3 has one and was taken out: a real removal.
        assert_eq!(with_unlinked_kept(&[1, 2, 3, 4], vec![1, 4], linked), vec![1, 2, 4]);
        assert_eq!(with_unlinked_kept(&[2, 1], vec![1], linked), vec![2, 1]);
        assert_eq!(with_unlinked_kept(&[1, 2], vec![1, 2], linked), vec![1, 2]);
    }

    #[tokio::test]
    async fn a_moved_file_the_server_has_not_linked_yet_stays_in_the_playlist() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        store.create("Chill").await.unwrap();
        store.set_entries("Chill", &lines(&["A/01.mp3", "A/02.mp3", "A/03.mp3"])).await.unwrap();
        let sync = || run(&store, &cat, &srv, &state);
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s1", "s2", "s3"]));

        // A/02's server item is gone (its file moved) and the server's new item is not linked yet.
        let keep: Vec<(String, i64, String)> = [1, 3, 4].iter().map(|i| (format!("s{i}"), 0, format!("A/0{i}.mp3"))).collect();
        cat.record_links("srv", "lib", &keep, 2).unwrap();
        cat.forget_unseen("srv", "lib", 2).unwrap();
        srv.edit("Chill", |v| v.retain(|s| s != "s2"));
        sync().await.unwrap();
        assert_eq!(store.entries("Chill").await.unwrap(), lines(&["A/01.mp3", "A/02.mp3", "A/03.mp3"]));

        // Linked again under its new server item: the server gets it back.
        cat.record_links("srv", "lib", &[("s2b".to_string(), 0, "A/02.mp3".to_string())], 3).unwrap();
        sync().await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s1", "s2b", "s3"]));
        assert_eq!(store.entries("Chill").await.unwrap(), lines(&["A/01.mp3", "A/02.mp3", "A/03.mp3"]));

        // Taken out in Finamp while linked: a real removal, followed here.
        srv.edit("Chill", |v| v.retain(|s| s != "s3"));
        sync().await.unwrap();
        assert_eq!(store.entries("Chill").await.unwrap(), lines(&["A/01.mp3", "A/02.mp3"]));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn paths_relative_to_the_music_folder() {
        assert_eq!(relative_to(Path::new("/media/Music"), Path::new("/media/lidarr-music/X/01.flac")).as_deref(), Some("../lidarr-music/X/01.flac"));
        assert_eq!(relative_to(Path::new("/media/Music"), Path::new("/media/Music/A/01.mp3")).as_deref(), Some("A/01.mp3"));
    }
}
