//! Playlist sync (decisions 0012, 0014): each playlist is one `.m3u` file in `Music/.playlists`, out of the media
//! server's reach (it skips hidden folders), and has an editable copy in accounts on the server, each kept in step both
//! ways. Nothing is set per playlist but whether it is shared:
//! - **your account** (per server: its setting, else the server's administrator) has every playlist, and every
//!   playlist made there (in Finamp) comes into Cratarr;
//! - **a shared playlist** is in every account; someone else taking it out of their account takes it out for them
//!   only. Other accounts' own playlists stay theirs.
//!
//! The engine below (`sync_server`) keeps one account's copies in step: a pass compares each copy with the list both
//! last agreed on and merges (`playlist_merge`). When both sides changed the order or the name between two passes, the
//! server's leads: it does not say when it saved its copy, so which change came last cannot be told.
//!
//! Every step is safe to repeat and to interrupt (the next pass finishes or undoes it, never guesses):
//! - a link is matched by the server copy's id, and remembers both agreed names, so a rename made on either side is
//!   told apart from a deletion even when the pass that carried it out was cut short;
//! - a rename made in Cratarr is recorded before the file is renamed;
//! - a listing on either side that suddenly lacks most linked playlists is not believed (no mass deletion);
//! - a file edited while a pass merged is left for the next pass;
//! - a server copy made but not yet remembered is found again by its name, not made twice;
//! - a file deleted because its server copy went can be restored in Cratarr;
//! - one playlist's problem is its own: the others go on, and the page shows it on that playlist's copy.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use anyhow::{Result, bail};

use super::ToggleError;
use super::library::{Library, epoch_now, label_of};
use crate::catalogue::Catalogue;
use crate::playlist_merge::{Item, Newer, merge, merge_once};
use crate::playlists::Store;
use crate::source::{PlaylistChange, Source};
use crate::sync_state::{Kind, Link, LinkStatus, LinkView, SyncState, Target};

/// Recorded in the sync state once the playlists in Music were adopted (decision 0014): never again, so the copies
/// there are not taken for playlists.
const ADOPTED: &str = "adopted";

/// At most this many new copies are made on a server per account and pass: a first start (or the move to editable
/// copies) does not flood the server.
pub(super) const COPIES_PER_PASS: usize = 8;

/// What can be set for a playlist's sync: whether it is shared with everyone.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TargetChoice {
    pub target: String,
    pub label: String,
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
    /// "Your account" per source, as the last pass found it.
    owners: std::sync::Mutex<HashMap<String, String>>,
    /// Passes so far: each reads a tenth of the copies whatever their stamp says (`Verify::Slice`).
    passes: AtomicU64,
    /// A pass is asked for (after an edit) and not started yet.
    wanted: AtomicBool,
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
            owners: Default::default(),
            passes: Default::default(),
            wanted: Default::default(),
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

    /// One pass: the playlists in Music adopted (once), the copies in Music retired (once made native), then per
    /// server: which account has which playlist, and every account's copies brought in step.
    pub async fn pass(&self) {
        self.wanted.store(false, Ordering::Relaxed);
        let _one = self.one_at_a_time.lock().await;
        self.follow.catch_up().await;
        if let Err(e) = self.adopt().await {
            tracing::warn!("playlist sync: cannot adopt the playlists in Music: {e:#}");
            return;
        }
        let Some(store) = self.lib.store_opt() else { return };
        // The links' agreed lists hold track ids: a catalogue made afresh (a restructure, a lost data folder) numbers
        // them anew, so the lists are forgotten and the next merge adds what either side has.
        match self.state.follow_catalogue(&self.lib.catalogue().generation()) {
            Ok(Some(n)) => tracing::info!("playlist sync: the catalogue is new; {n} link(s) merge afresh"),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("playlist sync: {e:#}");
                return;
            }
        }
        match self.state.make_native() {
            Ok(Some(n)) => tracing::info!("playlist sync: {n} shared copies in Music are replaced by copies in your account"),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("playlist sync: {e:#}");
                return;
            }
        }
        // Mid album action, a line can name a file that just moved: merged now, its track would leave the server copy.
        if self.follow.busy() {
            return;
        }
        let pass = self.passes.fetch_add(1, Ordering::Relaxed);
        for (a, s) in self.lib.connected() {
            let source = a.cfg.id.clone();
            let owner = match s.sync_account().await {
                Ok(o) => o,
                Err(e) => {
                    tracing::debug!("playlist sync: {source} keeps no playlists: {e:#}");
                    continue;
                }
            };
            self.owners.lock().unwrap_or_else(|p| p.into_inner()).insert(source.clone(), owner.clone());
            if let Err(e) = self.reconcile(&store, &*s, &source, &owner).await {
                tracing::warn!("playlist sync with {source}: {e:#}");
                continue;
            }
            let mut accounts: Vec<String> = self
                .state
                .targets()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|t| match t.kind {
                    Kind::User { source: s2, user } if s2 == source => Some(user),
                    _ => None,
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            // Yours first: its new playlists come in before the others' copies are made.
            accounts.sort_by_key(|u| u != &owner);
            if !accounts.contains(&owner) {
                accounts.insert(0, owner.clone());
            }
            for user in &accounts {
                let mine = user == &owner;
                let done = sync_server(&store, self.lib.catalogue(), &*s, &self.state, &source, user, mine, Verify::Slice(pass)).await;
                if let Err(e) = &done {
                    tracing::warn!("playlist sync with {source} ({user}): {e:#}");
                }
                // The account's error on every copy; else each playlist's own.
                let kind = Kind::User { source: source.clone(), user: user.clone() };
                for t in self.state.targets().unwrap_or_default().iter().filter(|t| t.kind == kind) {
                    let error = match &done {
                        Err(e) => Some(format!("{e:#}")),
                        Ok(undone) => undone.get(&t.playlist).cloned(),
                    };
                    self.record(&t.playlist, &kind, error);
                }
            }
            self.retire_copies(&store, &*s, &source, &owner).await;
        }
    }

    /// Which account has which playlist on one server: yours every playlist, every account the shared ones (but for
    /// those who took one out). A copy no longer wanted (a playlist no longer shared) is deleted from that account.
    async fn reconcile(&self, store: &Store, s: &dyn Source, source: &str, owner: &str) -> Result<()> {
        let files: Vec<String> = store.snapshot().await?.into_iter().map(|(n, _, _)| n).collect();
        let targets = self.state.targets()?;
        let shared: BTreeSet<&str> = targets.iter().filter(|t| t.kind == Kind::Shared).map(|t| t.playlist.as_str()).collect();
        let users: Vec<String> = if shared.is_empty() && self.name_known(source, owner) {
            Vec::new()
        } else {
            let all = s.users().await?;
            let mut names = self.names.lock().unwrap_or_else(|p| p.into_inner());
            for (id, name) in &all {
                names.insert((source.to_string(), id.clone()), name.clone());
            }
            all.into_iter().map(|(id, _)| id).collect()
        };
        let mut left = HashMap::new();
        for p in &shared {
            left.insert(p.to_string(), self.state.left_by(p)?);
        }
        let want = wanted(&files, &shared, source, owner, &users, &left);
        for (playlist, user) in &want {
            let kind = Kind::User { source: source.to_string(), user: user.clone() };
            if !targets.iter().any(|t| &t.playlist == playlist && t.kind == kind) {
                self.state.put_target(&Target { playlist: playlist.clone(), kind, copy: None })?;
            }
        }
        // A copy no longer wanted, of a playlist still here (a deleted one's copies go through the account's pass).
        for t in targets.iter().filter(|t| files.contains(&t.playlist)) {
            let Kind::User { source: s2, user } = &t.kind else { continue };
            if s2 != source || want.contains(&(t.playlist.clone(), user.clone())) {
                continue;
            }
            if let Some(link) = self.state.links(source, user)?.into_iter().find(|l| l.name == t.playlist) {
                s.delete_playlist(&link.remote).await?;
                self.state.remove(source, user, &t.playlist)?;
            }
            self.state.remove_target(&t.playlist, &t.kind)?;
            self.last.lock().unwrap_or_else(|p| p.into_inner()).remove(&(t.playlist.clone(), t.kind.key()));
        }
        Ok(())
    }

    fn name_known(&self, source: &str, user: &str) -> bool {
        self.names.lock().unwrap_or_else(|p| p.into_inner()).contains_key(&(source.to_string(), user.to_string()))
    }

    /// The copies in Music the server imported (Shared before, or adopted at the first start), each removed once the
    /// playlist's copy in your account exists, with your heart on it carried over. A deleted playlist's copy goes too.
    async fn retire_copies(&self, store: &Store, s: &dyn Source, source: &str, owner: &str) {
        let Ok(list) = self.state.to_retire() else { return };
        if list.is_empty() {
            return;
        }
        let names: Vec<String> = match store.snapshot().await {
            Ok(f) => f.into_iter().map(|(n, _, _)| n).collect(),
            Err(_) => return,
        };
        let linked: Vec<String> = self.state.links(source, owner).unwrap_or_default().into_iter().map(|l| l.name).collect();
        let mut hearts: Option<std::collections::BTreeMap<String, bool>> = None;
        for (playlist, copy) in list {
            let gone = !names.contains(&playlist);
            if !gone && !linked.contains(&playlist) {
                continue;
            }
            let done: Result<()> = async {
                if !gone {
                    if hearts.is_none() {
                        hearts = Some(s.playlist_favorites().await.unwrap_or_default());
                    }
                    // Hearting by name hearts every copy of that name: the new one too.
                    if hearts.as_ref().is_some_and(|h| h.get(&playlist).copied().unwrap_or(false)) {
                        s.set_playlist_favorite(&playlist, true).await?;
                    }
                }
                self.remove_copy(&copy).await?;
                self.state.retired_copy(&playlist)?;
                Ok(())
            }
            .await;
            if let Err(e) = done {
                tracing::warn!("playlist sync: the copy of {playlist} in Music: {e:#}");
            }
        }
    }

    /// Bring the copies of these playlists in step soon after an edit (the pass would in 30 s).
    pub(super) fn changed(self: &Arc<Self>, _playlists: Vec<String>) {
        if self.wanted.swap(true, Ordering::Relaxed) {
            return;
        }
        let me = self.clone();
        tokio::spawn(async move {
            // A burst of edits makes one pass.
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            me.pass().await;
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

    /// Every playlist's sync as the page shows it, by playlist: shared or not, and its copy in each account (made yet
    /// or not) with how its last pass went (yours first).
    pub(super) fn links(&self) -> HashMap<String, Vec<LinkView>> {
        let last = self.last.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let owners = self.owners.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let targets = self.state.targets().unwrap_or_default();
        // Which copies exist: every account's links, read once.
        let mut made: std::collections::HashSet<(String, String, String)> = Default::default();
        let accounts: BTreeSet<(String, String)> = targets
            .iter()
            .filter_map(|t| if let Kind::User { source, user } = &t.kind { Some((source.clone(), user.clone())) } else { None })
            .collect();
        for (source, user) in accounts {
            for l in self.state.links(&source, &user).unwrap_or_default() {
                made.insert((source.clone(), user.clone(), l.name));
            }
        }
        let mut out: HashMap<String, Vec<(bool, LinkView)>> = HashMap::new();
        for t in targets {
            let (yours, made) = match &t.kind {
                Kind::User { source, user } => {
                    (owners.get(source) == Some(user), made.contains(&(source.clone(), user.clone(), t.playlist.clone())))
                }
                Kind::Shared => (false, true),
            };
            let view = LinkView {
                target: t.kind.key(),
                label: self.label(&t.kind),
                status: last.get(&(t.playlist.clone(), t.kind.key())).cloned(),
                made,
            };
            out.entry(t.playlist.clone()).or_default().push((!yours, view));
        }
        out.into_iter()
            .map(|(p, mut v)| {
                v.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.label.cmp(&b.1.label)));
                (p, v.into_iter().map(|(_, v)| v).collect())
            })
            .collect()
    }

    fn label(&self, kind: &Kind) -> String {
        match kind {
            Kind::Shared => "Everyone".into(),
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

    /// What can be set for a playlist's sync: sharing it with everyone.
    pub(super) async fn choices(&self) -> Vec<TargetChoice> {
        vec![TargetChoice { target: Kind::Shared.key(), label: "Shared with everyone".into() }]
    }

    /// Share a playlist with everyone: the next pass gives every account a copy (also those that took it out before).
    /// Accounts follow by themselves; only sharing is set.
    pub(super) async fn link(self: &Arc<Self>, playlist: &str, kind: Kind) -> Result<(), ToggleError> {
        if kind != Kind::Shared {
            return Err(ToggleError::Refused("accounts follow by themselves; only sharing is set".into()));
        }
        let store = self.lib.store()?;
        if store.entries(playlist).await.is_err() {
            return Err(ToggleError::UnknownPlaylist);
        }
        {
            let _one = self.one_at_a_time.lock().await;
            self.state.put_target(&Target { playlist: playlist.to_string(), kind, copy: None })?;
            self.state.clear_left(playlist)?;
        }
        self.changed(vec![playlist.to_string()]);
        Ok(())
    }

    /// A restored playlist (its deletion undone) gets back what it had: shared, and the copies (made again by the next
    /// pass).
    pub(super) async fn restore_links(self: &Arc<Self>, playlist: &str) -> Result<(), ToggleError> {
        {
            let _one = self.one_at_a_time.lock().await;
            self.state.revive_targets(playlist)?;
        }
        self.changed(vec![playlist.to_string()]);
        Ok(())
    }

    /// A new playlist: yours only (the next pass makes its copy); what a deleted one of the same name had is not its.
    pub(super) async fn new_playlist(self: &Arc<Self>, playlist: &str) -> Result<(), ToggleError> {
        self.state.forget_retired(playlist)?;
        self.state.clear_left(playlist)?;
        self.changed(vec![playlist.to_string()]);
        Ok(())
    }

    /// Stop sharing a playlist: the next pass deletes the other accounts' copies; yours stays.
    pub(super) async fn unlink(self: &Arc<Self>, playlist: &str, kind: &Kind) -> Result<(), ToggleError> {
        if kind != &Kind::Shared {
            return Err(ToggleError::Refused("accounts follow by themselves; only sharing is set".into()));
        }
        {
            let _one = self.one_at_a_time.lock().await;
            self.state.remove_target(playlist, kind)?;
            self.state.clear_left(playlist)?;
        }
        self.last.lock().unwrap_or_else(|p| p.into_inner()).remove(&(playlist.to_string(), kind.key()));
        self.changed(vec![playlist.to_string()]);
        Ok(())
    }

    /// Adopt the playlists in Music (decision 0014), once: each is copied into `.playlists` (unless already there);
    /// the file in Music, which the server imported, is retired once the playlist's copy in your account exists.
    async fn adopt(&self) -> Result<()> {
        if self.state.meta(ADOPTED).is_some() {
            return Ok(());
        }
        let Some(music) = self.lib.music() else { return Ok(()) };
        let m = PathBuf::from(&music);
        let names = blocking(move || crate::playlists::adopt_into_playlist_dir(&m)).await??;
        for name in &names {
            self.state.put_retire(name, name)?;
        }
        self.state.set_meta(ADOPTED, &epoch_now().to_string())?;
        self.lib.reopen_store();
        tracing::info!("adopted {} playlist(s) from Music into {}", names.len(), crate::playlists::PLAYLIST_DIR);
        Ok(())
    }

    /// Remove a copy from Music, and tell the servers it is gone.
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

/// Which account should have which playlist on one server, as (playlist, user): yours every playlist; every account
/// the shared ones, but for those who took one out (`left`, by playlist: (source, user)).
fn wanted(
    files: &[String],
    shared: &BTreeSet<&str>,
    source: &str,
    owner: &str,
    users: &[String],
    left: &HashMap<String, Vec<(String, String)>>,
) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for p in files {
        out.insert((p.clone(), owner.to_string()));
        if shared.contains(p.as_str()) {
            let gone = left.get(p).cloned().unwrap_or_default();
            for u in users.iter().filter(|u| !gone.iter().any(|(s, g)| s == source && g == *u)) {
                out.insert((p.clone(), u.clone()));
            }
        }
    }
    out
}

/// Which linked copies a pass reads whatever their change stamp says.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(test), allow(dead_code))]
pub(super) enum Verify {
    /// Only those whose stamp moved.
    None,
    /// Every one.
    All,
    /// About a tenth: those whose turn it is in this pass (by their id), so each is read at least every
    /// `VERIFY_EVERY` passes without a burst.
    Slice(u64),
}

impl Verify {
    fn of(self, remote: &str) -> bool {
        match self {
            Verify::None => false,
            Verify::All => true,
            Verify::Slice(pass) => {
                let h = remote.bytes().fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b)));
                (h.wrapping_add(pass)) % VERIFY_EVERY == 0
            }
        }
    }
}

/// Agreed tracks whose file is away for now, and their items on the server as the link remembers them.
type Away = HashMap<i64, String>;

/// Lines, track ids and server ids, through the catalogue. Blocking.
struct Translate {
    cat: Arc<Catalogue>,
    source: String,
    /// The catalogue's folders; 0 is the Music folder, which playlist lines are relative to.
    folders: Vec<PathBuf>,
}

impl Translate {
    fn items(&self, lines: &[String]) -> Vec<Item> {
        lines
            .iter()
            .map(|l| {
                let track = self.cat.place_of_line(l).and_then(|(folder, rel)| self.cat.track_of(folder, &rel));
                track.map_or_else(|| Item::Missing(l.clone()), Item::Track)
            })
            .collect()
    }

    /// A track's playlist line: its path relative to the Music folder.
    fn line(&self, track: i64) -> Option<String> {
        let (folder, rel) = self.cat.file_of(track)?;
        if folder == 0 {
            return Some(rel);
        }
        relative_to(self.folders.first()?, &self.folders.get(folder as usize)?.join(rel))
    }

    /// A track's playlist line also while its file is away (where it was last seen).
    fn line_any(&self, track: i64) -> Option<String> {
        let (folder, rel) = self.cat.file_of_any(track)?;
        if folder == 0 {
            return Some(rel);
        }
        relative_to(self.folders.first()?, &self.folders.get(folder as usize)?.join(rel))
    }

    /// `items` with the agreed tracks whose file is away for now put back (`with_vanished_kept`).
    fn with_away_kept(&self, base: &[i64], items: Vec<Item>) -> Vec<Item> {
        with_vanished_kept(base, items, |t| if self.line(t).is_some() { None } else { Some(self.line_any(t)) })
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

    /// A track's item on the server: the one the catalogue links its file to, else (its file away) the one `away`
    /// remembers.
    fn sid(&self, track: i64, away: &Away) -> Option<String> {
        self.server_id(track).or_else(|| away.get(&track).cloned())
    }

    /// The server's list as track ids, and the items Cratarr does not know, each with the position it had (kept on
    /// the server as they are; the file cannot hold them). An item `away` remembers is its track.
    fn from_server(&self, ids: &[String], away: &Away) -> (Vec<i64>, Vec<(usize, String)>) {
        let mut tracks = Vec::new();
        let mut unknown = Vec::new();
        for (i, id) in ids.iter().enumerate() {
            let remembered = || away.iter().find(|(_, a)| *a == id).map(|(t, _)| *t);
            match self.cat.track_of_source(&self.source, id).or_else(remembered) {
                Some(t) => tracks.push(t),
                None => unknown.push((i, id.clone())),
            }
        }
        (tracks, unknown)
    }

    /// The tracks of a list that are on the server, as (track, server id): what both sides agree on.
    fn on_server(&self, items: &[Item], away: &Away) -> Vec<(i64, String)> {
        items
            .iter()
            .filter_map(|i| if let Item::Track(t) = i { self.sid(*t, away).map(|s| (*t, s)) } else { None })
            .collect()
    }

    /// What a link remembers of its agreed tracks whose file is away now: their items on the server.
    fn away(&self, link: &Link) -> Away {
        link.base
            .iter()
            .zip(&link.ids)
            .filter(|(t, id)| !id.is_empty() && self.line(**t).is_none())
            .map(|(t, id)| (*t, id.clone()))
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

/// Each linked copy is read on the server at least every this many passes whatever its change stamp says (about
/// every five minutes at one pass in 30 s): a change a server did not stamp is found all the same.
pub(super) const VERIFY_EVERY: u64 = 10;

/// What one account's pass left undone: by playlist, why. A problem with one playlist is that playlist's alone (the
/// rest go on, and it is tried again next pass); a problem with the whole account is the pass's error.
pub(super) type Undone = HashMap<String, String>;

/// One pass for one server and user: the playlists linked to this account (decision 0014). Two-way, whatever happened
/// on either side since the last pass:
/// - renamed on one side: the other follows; on both: the server's name;
/// - edited: merged (`playlist_merge`); reordered on both: the server's order;
/// - deleted on one side: deleted on the other, unless the other changed since: an edit wins over a deletion, and the
///   playlist comes back where it was deleted, with the edits;
/// - new on one side: made on the other; a name that cannot be a file name here, or is taken, gets the nearest free
///   one ("Chill 2"), and the server copy is renamed to match;
/// - a track whose file is away for now (an unmounted folder, a rescan under way) is no removal on either side.
///
/// `yours`: the account is your account. Another account only holds the playlists shared with it: none of its own
/// come in, and its taking a copy out takes the playlist out for it only.
#[allow(clippy::too_many_arguments)]
pub(super) async fn sync_server(
    store: &Store,
    cat: Arc<Catalogue>,
    s: &dyn Source,
    state: &SyncState,
    source_id: &str,
    user: &str,
    yours: bool,
    verify: Verify,
) -> Result<Undone> {
    let kind = Kind::User { source: source_id.to_string(), user: user.to_string() };
    let mine: Vec<Target> = state.targets()?.into_iter().filter(|t| t.kind == kind).collect();
    let target_of = |name: &str| mine.iter().find(|t| t.playlist == name).cloned();
    let own = s.own_playlists(user).await?;
    let files = store.snapshot().await?;
    let has_file = |n: &str| files.iter().any(|(f, _, _)| f == n);
    let mut links = state.links(source_id, user)?;
    let mut undone = Undone::new();

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
    let linked_remotes: Vec<String> = links.iter().map(|l| l.remote.clone()).collect();

    // 2. Each linked playlist present on both sides: names, then contents.
    for link in links.clone() {
        let Some(remote) = own.iter().find(|o| o.id == link.remote) else { continue };
        let Some((_, lines, _)) = files.iter().find(|(n, _, _)| n == &link.name) else { continue };
        if target_of(&link.name).is_none() {
            continue;
        }
        let name = link.name.clone();
        let pair = Pair { store, s, state, tr: &tr, source_id, user, own: &own, linked_remotes: &linked_remotes };
        let verify = verify.of(&link.remote);
        if let Err(e) = pair.sync(link, remote, lines, &has_file, verify).await {
            tracing::warn!("playlist sync: {name} with {source_id}: {e:#}");
            undone.insert(name, format!("{e:#}"));
        }
    }

    // 3. Gone on one side: gone on the other too, unless the other changed since they last agreed (then it comes
    //    back where it was deleted, with those changes).
    for l in &links {
        let remote = own.iter().find(|o| o.id == l.remote);
        let file = files.iter().find(|(n, _, _)| n == &l.name).map(|(_, lines, _)| lines);
        let done: Result<()> = async {
            match (file, remote) {
                (Some(_), None) if !yours => {
                    // Someone else took a shared playlist out of their account: out for them only, and it stays out.
                    tracing::info!("playlist sync: {user} on {source_id} took {} out of their account", l.name);
                    state.set_left(&l.name, source_id, user)?;
                    state.remove(source_id, user, &l.name)?;
                    state.remove_target(&l.name, &kind)?;
                }
                (Some(lines), None) => {
                    if changed_here(&tr, l, lines).await? {
                        // Edited here since: a new server copy is made (step 4); the playlist stays.
                        tracing::info!("playlist sync: {} was deleted on {source_id} but changed here since; made again there", l.name);
                        state.remove(source_id, user, &l.name)?;
                        return Ok(());
                    }
                    tracing::info!("playlist sync: {} was deleted on {source_id}; deleting it here too", l.name);
                    match store.delete(&l.name).await {
                        Ok(()) | Err(crate::playlists::StoreError::Unknown) => {
                            state.remove(source_id, user, &l.name)?;
                            state.retire_target(&l.name, &kind)?;
                        }
                        Err(e) => bail!("cannot delete it: {e:?}"),
                    }
                }
                (None, Some(r)) if yours => {
                    let ids = s.playlist_tracks(user, &r.id).await?;
                    let (t, link) = (tr.clone(), l.clone());
                    let (tracks, changed) = blocking(move || {
                        let away = t.away(&link);
                        let (listed, _) = t.from_server(&ids, &away);
                        let changed = with_unlinked_kept(&link.base, listed.clone(), |x| t.sid(x, &away).is_some()) != link.base;
                        (listed, changed)
                    })
                    .await?;
                    if changed {
                        // Edited on the server since: the playlist comes back here, as the server has it, with every
                        // link it had (its Shared copy too).
                        tracing::info!("playlist sync: {} was deleted here but changed on {source_id} since; restored", l.name);
                        match store.create(&l.name).await {
                            Ok(()) | Err(crate::playlists::StoreError::Exists) => {}
                            Err(e) => bail!("cannot restore it: {e:?}"),
                        }
                        let (t, back) = (tr.clone(), tracks.clone());
                        let new_lines = blocking(move || t.lines(&back.into_iter().map(Item::Track).collect::<Vec<_>>())).await?;
                        // Into the empty file just made (a file made meanwhile is left as it is, for the next pass).
                        store.set_entries_if(&l.name, &[], &new_lines).await.map_err(|e| anyhow::anyhow!("cannot restore it: {e:?}"))?;
                        state.revive_targets(&l.name)?;
                        let (t, back) = (tr.clone(), tracks);
                        let agreed = blocking(move || t.on_server(&back.into_iter().map(Item::Track).collect::<Vec<_>>(), &Away::new())).await?;
                        let (base, ids) = agreed.into_iter().unzip();
                        state.put(source_id, user, &Link { stamp: r.stamp.clone(), base, ids, ..l.clone() })?;
                        return Ok(());
                    }
                    tracing::info!("playlist sync: {} was deleted here; deleting it on {source_id}", l.name);
                    // Kept on failure: the next pass tries again (or finds it gone).
                    s.delete_playlist(&l.remote).await?;
                    state.remove(source_id, user, &l.name)?;
                    state.retire_target(&l.name, &kind)?;
                }
                (None, Some(_)) => {
                    // Deleted in Cratarr (only you delete a playlist for everyone): their copy goes too.
                    s.delete_playlist(&l.remote).await?;
                    state.remove(source_id, user, &l.name)?;
                    state.retire_target(&l.name, &kind)?;
                }
                (None, None) => {
                    state.remove(source_id, user, &l.name)?;
                    state.retire_target(&l.name, &kind)?;
                }
                (Some(_), Some(_)) => {}
            }
            Ok(())
        }
        .await;
        if let Err(e) = done {
            tracing::warn!("playlist sync: {} with {source_id}: {e:#}", l.name);
            undone.insert(l.name.clone(), format!("{e:#}"));
        }
    }
    // Read again: this pass renamed, deleted and restored (the names at the start would make copies of what is gone).
    let files = store.snapshot().await?;
    let has_file = |n: &str| files.iter().any(|(f, _, _)| f == n);
    let links = state.links(source_id, user)?;
    // A link to a playlist deleted before it ever had a copy there.
    for t in mine.iter().filter(|t| !has_file(&t.playlist) && !links.iter().any(|l| l.name == t.playlist)) {
        state.retire_target(&t.playlist, &kind)?;
    }
    let mine: Vec<Target> = state.targets()?.into_iter().filter(|t| t.kind == kind).collect();
    let target_of = |name: &str| mine.iter().find(|t| t.playlist == name).cloned();

    // 4. New on one side: a copy on the other.
    // Every server copy linked at the start too: a copy renamed or deleted this pass is still in the listing read
    // then, and is no new playlist.
    let mut claimed: BTreeSet<String> = links.iter().map(|l| l.remote.clone()).chain(linked_remotes.iter().cloned()).collect();
    let linked_names: Vec<String> = links.iter().map(|l| l.name.clone()).collect();
    let mut made = 0;
    for (name, lines, _) in files.iter().filter(|(n, _, _)| !linked_names.contains(n)) {
        if target_of(name).is_none() {
            continue;
        }
        // A server copy of that name nobody holds, in your account: made before this pass could remember it, or on
        // both sides. In another account it is theirs, never merged with a playlist shared with them.
        let same_name = own.iter().find(|o| yours && &o.name == name && !claimed.contains(&o.id));
        if same_name.is_none() && made >= COPIES_PER_PASS {
            // The rest next pass: the server is not flooded.
            continue;
        }
        let done: Result<()> = async {
            // Linked with no agreed list, so the first merge keeps everything from both.
            if let Some(o) = same_name {
                claimed.insert(o.id.clone());
                let link = Link::new(name, &o.id, &o.name);
                state.put(source_id, user, &link)?;
                return sync_content(store, s, state, &tr, source_id, user, &link, o.stamp.clone(), lines, true).await;
            }
            let (t, l) = (tr.clone(), lines.clone());
            let on_server = blocking(move || {
                let items = t.items(&l);
                t.on_server(&items, &Away::new())
            })
            .await?;
            let ids: Vec<String> = on_server.iter().map(|(_, s)| s.clone()).collect();
            made += 1;
            let remote = s.create_playlist(user, name, &ids).await?;
            claimed.insert(remote.clone());
            let (base, ids) = on_server.into_iter().unzip();
            state.put(source_id, user, &Link { base, ids, ..Link::new(name, &remote, name) })?;
            Ok(())
        }
        .await;
        if let Err(e) = done {
            tracing::warn!("playlist sync: {name} with {source_id}: {e:#}");
            undone.insert(name.clone(), format!("{e:#}"));
        }
    }
    // Made on the server (in Finamp, say): a new playlist here, linked. Its name as the server has it when that is a
    // usable file name no playlist here has; else the nearest free one, and the server copy is renamed to match on
    // the next pass. Two server playlists of one name are two playlists here.
    // Only from your account: another account's own playlists stay theirs.
    let mut names_here: Vec<String> = files.iter().map(|(n, _, _)| n.clone()).collect();
    for o in own.iter().filter(|o| yours && !claimed.contains(&o.id)) {
        let name = crate::playlists::free_name(&crate::playlists::usable_name(&o.name), |n| names_here.iter().any(|h| h.eq_ignore_ascii_case(n)));
        names_here.push(name.clone());
        let done: Result<()> = async {
            // Remembered before its tracks are written, so a cut-short pass finds file and link again instead of
            // making a second file.
            store.create(&name).await.map_err(|e| anyhow::anyhow!("cannot make a playlist for {:?} from {source_id}: {e:?}", o.name))?;
            state.put_target(&Target { playlist: name.clone(), kind: kind.clone(), copy: None })?;
            let link = Link::new(&name, &o.id, &o.name);
            state.put(source_id, user, &link)?;
            sync_content(store, s, state, &tr, source_id, user, &link, o.stamp.clone(), &[], true).await
        }
        .await;
        if let Err(e) = done {
            tracing::warn!("playlist sync: {e:#}");
            undone.insert(name, format!("{e:#}"));
        }
    }
    Ok(undone)
}

/// Whether the playlist here changed since both sides last agreed (in its tracks the server knows, or that are away
/// for now).
async fn changed_here(tr: &Arc<Translate>, link: &Link, lines: &[String]) -> Result<bool> {
    let (t, l, base, link) = (tr.clone(), lines.to_vec(), link.base.clone(), link.clone());
    blocking(move || {
        let items = t.with_away_kept(&base, t.items(&l));
        let away = t.away(&link);
        t.on_server(&items, &away).into_iter().map(|(x, _)| x).collect::<Vec<_>>() != base
    })
    .await
}

/// One linked pair present on both sides, in one pass.
struct Pair<'a> {
    store: &'a Store,
    s: &'a dyn Source,
    state: &'a SyncState,
    tr: &'a Arc<Translate>,
    source_id: &'a str,
    user: &'a str,
    own: &'a [crate::source::OwnPlaylist],
    linked_remotes: &'a [String],
}

impl Pair<'_> {
    /// Names, then contents. A rename is carried over in one pass and the contents looked at in the next, under the
    /// names both sides then agree on.
    async fn sync(
        &self,
        mut link: Link,
        remote: &crate::source::OwnPlaylist,
        lines: &[String],
        has_file: &(dyn Fn(&str) -> bool + Sync),
        verify: bool,
    ) -> Result<()> {
        let (state, source_id, user) = (self.state, self.source_id, self.user);
        let renamed_there = remote.name != link.remote_name;
        let renamed_here = link.name != link.remote_name;
        // Renamed on both sides: the server's name.
        if renamed_there {
            // The file follows, under the nearest usable free name (the server copy then follows that, next pass).
            if remote.name != link.name {
                let to = crate::playlists::free_name(&crate::playlists::usable_name(&remote.name), |n| n != link.name && has_file(n));
                if to != link.name {
                    self.store.rename(&link.name, &to).await.map_err(|e| anyhow::anyhow!("cannot rename it to {to:?}: {e:?}"))?;
                    state.rename(&link.name, &to)?;
                    link.name = to;
                }
            }
            link.remote_name = remote.name.clone();
            state.put(source_id, user, &link)?;
            return Ok(());
        }
        if renamed_here || renamed_there {
            // The server copy follows the name here. Another linked playlist of that name there is not taken over.
            if self.own.iter().any(|o| o.name == link.name && o.id != remote.id && self.linked_remotes.contains(&o.id)) {
                bail!("{source_id} has another linked playlist named {:?}", link.name);
            }
            link.remote = self.s.rename_playlist(user, &remote.id, &link.name).await?;
            link.remote_name = link.name.clone();
            link.stamp = None;
            state.put(source_id, user, &link)?;
            return Ok(());
        }
        sync_content(self.store, self.s, state, self.tr, source_id, user, &link, remote.stamp.clone(), lines, verify).await
    }
}

/// Bring one linked pair's contents in step (names already agree). The server's list is read when its stamp moved,
/// when `verify`, or when the playlist here changed (so a write keeps what Cratarr does not know there).
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
    verify: bool,
) -> Result<()> {
    // A track whose file is away for now was not taken out here: it counts as still in the playlist (its line stays
    // as it is, a missing one), and stays in the server copy for as long as the server lists it.
    let (t, l, lk) = (tr.clone(), lines.to_vec(), link.clone());
    let (items, file_tracks, away) = blocking(move || {
        let items = t.with_away_kept(&lk.base, t.items(&l));
        let away = t.away(&lk);
        let tracks: Vec<i64> = t.on_server(&items, &away).into_iter().map(|(x, _)| x).collect();
        (items, tracks, away)
    })
    .await?;
    let away = Arc::new(away);
    // A server that holds each track once (Jellyfin) is compared and written on the first copies only: the file's
    // other copies are the file's, and never read as taken out there (decision 0025, amended 2026-10-16).
    let once = !s.playlists_keep_repeats();
    let file_tracks = if once { first_copies(&file_tracks) } else { file_tracks };
    let stamp_moved = stamp != link.stamp || link.stamp.is_none();
    if !stamp_moved && !verify && file_tracks == link.base {
        return Ok(());
    }
    let ids = s.playlist_tracks(user, &link.remote).await?;
    // A track the server stopped listing while Cratarr has no server item for it was not taken out by anyone: its
    // file moved (Keep, Replace, a restructure) or is away, and the server's item for it is not linked. Read as a
    // removal it would leave the playlist here; it stays, and goes back to the server once linked.
    let (t, base, aw) = (tr.clone(), link.base.clone(), away.clone());
    let (server_tracks, unknown) = blocking(move || {
        let (listed, unknown) = t.from_server(&ids, &aw);
        (with_unlinked_kept(&base, listed, |x| t.sid(x, &aw).is_some()), unknown)
    })
    .await?;
    let server_changed = server_tracks != link.base;
    if !server_changed && file_tracks == link.base {
        if stamp != link.stamp {
            state.put(source_id, user, &Link { stamp, ..link.clone() })?;
        }
        return Ok(());
    }
    // Reordered on both sides: the server's order.
    let newer = if server_changed { Newer::Server } else { Newer::File };
    let merged = if once { merge_once(&link.base, &items, &server_tracks, newer) } else { merge(&link.base, &items, &server_tracks, newer) };
    let (t, m, aw) = (tr.clone(), merged.clone(), away.clone());
    let (new_lines, on_server) = blocking(move || (t.lines(&m), t.on_server(&m, &aw))).await?;
    let on_server = if once {
        let mut seen = std::collections::HashSet::new();
        on_server.into_iter().filter(|(x, _)| seen.insert(*x)).collect()
    } else {
        on_server
    };
    if new_lines != lines && !store.set_entries_if(&link.name, lines, &new_lines).await? {
        // Edited while this pass merged: the next pass merges that too.
        return Ok(());
    }
    let server_now: Vec<String> = with_unknown(on_server.iter().map(|(_, s)| s.clone()).collect(), &unknown);
    let server_before: Vec<String> = {
        let (t, before, aw) = (tr.clone(), server_tracks.clone(), away.clone());
        let ids = blocking(move || before.iter().filter_map(|x| t.sid(*x, &aw)).collect::<Vec<_>>()).await?;
        with_unknown(ids, &unknown)
    };
    let wrote_server = server_now != server_before;
    if wrote_server {
        s.set_playlist_tracks(user, &link.remote, &server_now).await?;
    }
    // After a write the stamp changed: unknown until the next listing, which then finds nothing to do.
    let stamp = if wrote_server { None } else { stamp };
    let (base, ids) = on_server.into_iter().unzip();
    state.put(source_id, user, &Link { stamp, base, ids, ..link.clone() })?;
    Ok(())
}

/// The playlist's items with the tracks of `base` whose file is away for now put back where they were: while away a
/// track's line reads as a missing one, so it would look taken out. `away` says, for a track whose file is away,
/// the line it had (`Some(None)` when not even that is known); `None` for a track whose file is there. Only a track
/// whose line is still in the playlist is put back: a line taken out is a real removal.
fn with_vanished_kept(base: &[i64], mut items: Vec<Item>, away: impl Fn(i64) -> Option<Option<String>>) -> Vec<Item> {
    for (at, track) in base.iter().enumerate() {
        if items.contains(&Item::Track(*track)) {
            continue;
        }
        if let Some(Some(line)) = away(*track)
            && items.contains(&Item::Missing(line))
        {
            items.insert(at.min(items.len()), Item::Track(*track));
        }
    }
    items
}

/// Each track's first copy, in order: a list as a server that holds each track once has it.
fn first_copies(tracks: &[i64]) -> Vec<i64> {
    let mut seen = std::collections::HashSet::new();
    tracks.iter().copied().filter(|t| seen.insert(*t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::source::{Fut, OwnPlaylist, Playing, PlaylistEvent, Status, Track};
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    /// A server keeping playlists in memory; every change bumps the playlist's stamp.
    #[derive(Default)]
    struct Fake {
        lists: Mutex<Vec<(String, String, Vec<String>, u32)>>, // id, name, items, version
        /// Writes to these playlists fail (a server refusing one playlist).
        refuse: Mutex<Vec<String>>,
    }

    impl Fake {
        fn edit(&self, name: &str, f: impl FnOnce(&mut Vec<String>)) {
            let mut l = self.lists.lock().unwrap();
            let p = l.iter_mut().find(|p| p.1 == name).unwrap();
            f(&mut p.2);
            p.3 += 1;
        }
        /// A change the server does not stamp.
        fn edit_quietly(&self, name: &str, f: impl FnOnce(&mut Vec<String>)) {
            let mut l = self.lists.lock().unwrap();
            f(&mut l.iter_mut().find(|p| p.1 == name).unwrap().2);
        }
        fn names(&self) -> Vec<String> {
            let mut n: Vec<String> = self.lists.lock().unwrap().iter().map(|p| p.1.clone()).collect();
            n.sort();
            n
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
                if self.refuse.lock().unwrap().iter().any(|r| r == id) {
                    anyhow::bail!("refused");
                }
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

    /// Every playlist here linked to "me" on "srv" (what 0012 did for all of them), then one pass; what it left undone
    /// is its error.
    async fn run(store: &Store, cat: &Arc<Catalogue>, srv: &Fake, state: &SyncState) -> Result<()> {
        let linked: Vec<String> = state.targets().unwrap().into_iter().map(|t| t.playlist).collect();
        for p in store.view(None).await.unwrap().into_iter().filter(|p| !linked.contains(&p.id)) {
            let t = Target { playlist: p.id, kind: me(), copy: None };
            state.put_target(&t).unwrap();
        }
        let undone = sync_server(store, cat.clone(), srv, state, "srv", "me", true, Verify::None).await?;
        if !undone.is_empty() {
            bail!("undone: {undone:?}");
        }
        Ok(())
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

    #[test]
    fn yours_has_every_playlist_and_everyone_the_shared_ones() {
        let files = lines(&["Chill", "Gym", "Road"]);
        let shared: BTreeSet<&str> = ["Gym", "Road"].into_iter().collect();
        let users = lines(&["me", "elva", "mike"]);
        let left: HashMap<String, Vec<(String, String)>> = [("Road".to_string(), vec![("srv".to_string(), "mike".to_string())])].into();
        let got: Vec<(String, String)> = wanted(&files, &shared, "srv", "me", &users, &left).into_iter().collect();
        let want: Vec<(String, String)> = [("Chill", "me"), ("Gym", "elva"), ("Gym", "me"), ("Gym", "mike"), ("Road", "elva"), ("Road", "me")]
            .iter()
            .map(|(p, u)| (p.to_string(), u.to_string()))
            .collect();
        assert_eq!(got, want);
    }

    #[tokio::test]
    async fn copies_are_made_a_few_per_pass() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        for i in 0..COPIES_PER_PASS + 3 {
            store.create(&format!("P{i:02}")).await.unwrap();
        }
        run(&store, &cat, &srv, &state).await.unwrap();
        assert_eq!(srv.lists.lock().unwrap().len(), COPIES_PER_PASS);
        run(&store, &cat, &srv, &state).await.unwrap();
        assert_eq!(srv.lists.lock().unwrap().len(), COPIES_PER_PASS + 3);
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn another_account_holds_what_is_shared_with_it_and_nothing_else() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        let elva = Kind::User { source: "srv".into(), user: "elva".into() };
        store.create("Gym").await.unwrap();
        store.set_entries("Gym", &lines(&["A/01.mp3"])).await.unwrap();
        state.put_target(&Target { playlist: "Gym".into(), kind: elva.clone(), copy: None }).unwrap();
        // Her own playlists: one of the same name, one other. Neither comes in, nor is merged.
        srv.create_playlist("elva", "Gym", &lines(&["s4"])).await.unwrap();
        srv.create_playlist("elva", "Hers", &lines(&["s3"])).await.unwrap();
        let sync = || sync_server(&store, cat.clone(), &srv, &state, "srv", "elva", false, Verify::None);
        sync().await.unwrap();
        sync().await.unwrap();
        assert!(store.entries("Hers").await.is_err(), "her own playlist stays hers");
        assert_eq!(store.entries("Gym").await.unwrap(), lines(&["A/01.mp3"]), "not merged with hers");
        let copies: Vec<Vec<String>> = srv.lists.lock().unwrap().iter().filter(|p| p.1 == "Gym").map(|p| p.2.clone()).collect();
        assert_eq!(copies.len(), 2, "hers and the shared copy");
        // Her edit to the shared copy comes back.
        let shared_id = state.links("srv", "elva").unwrap()[0].remote.clone();
        srv.lists.lock().unwrap().iter_mut().find(|p| p.0 == shared_id).unwrap().2.push("s2".into());
        srv.lists.lock().unwrap().iter_mut().find(|p| p.0 == shared_id).unwrap().3 += 1;
        sync().await.unwrap();
        assert_eq!(store.entries("Gym").await.unwrap(), lines(&["A/01.mp3", "A/02.mp3"]));
        // She takes it out of her account: out for her only; the playlist stays, and it stays out for her.
        srv.delete_playlist(&shared_id).await.unwrap();
        sync().await.unwrap();
        assert!(store.entries("Gym").await.is_ok());
        assert_eq!(state.left_by("Gym").unwrap(), vec![("srv".to_string(), "elva".to_string())]);
        assert!(!state.targets().unwrap().iter().any(|t| t.kind == elva));
        // Deleted in Cratarr: her copy goes too.
        state.clear_left("Gym").unwrap();
        state.put_target(&Target { playlist: "Gym".into(), kind: elva.clone(), copy: None }).unwrap();
        sync().await.unwrap();
        assert_eq!(srv.lists.lock().unwrap().iter().filter(|p| p.1 == "Gym").count(), 2);
        store.delete("Gym").await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.names(), lines(&["Gym", "Hers"]), "only her own are left");
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn an_edit_wins_over_a_deletion_on_the_other_side() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        let sync = || run(&store, &cat, &srv, &state);
        store.create("Chill").await.unwrap();
        store.set_entries("Chill", &lines(&["A/01.mp3"])).await.unwrap();
        sync().await.unwrap();
        // Deleted in Finamp, edited here: made again there, with the edit.
        let id = srv.lists.lock().unwrap()[0].0.clone();
        srv.delete_playlist(&id).await.unwrap();
        store.set_entries("Chill", &lines(&["A/01.mp3", "A/02.mp3"])).await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s1", "s2"]));
        assert_eq!(store.entries("Chill").await.unwrap(), lines(&["A/01.mp3", "A/02.mp3"]));
        // Deleted here, edited in Finamp: back here as Finamp has it, with its links.
        let shared = Target { playlist: "Chill".into(), kind: Kind::Shared, copy: None };
        state.put_target(&shared).unwrap();
        store.delete("Chill").await.unwrap();
        state.retire_target("Chill", &Kind::Shared).unwrap();
        srv.edit("Chill", |v| v.push("s3".into()));
        sync().await.unwrap();
        assert_eq!(store.entries("Chill").await.unwrap(), lines(&["A/01.mp3", "A/02.mp3", "A/03.mp3"]));
        assert!(state.targets().unwrap().contains(&shared), "its Shared link is back too");
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s1", "s2", "s3"]));
        // Deleted on one side with nothing changed on the other: deleted on both.
        store.delete("Chill").await.unwrap();
        sync().await.unwrap();
        assert!(srv.lists.lock().unwrap().is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn a_name_that_cannot_be_used_here_is_made_free_and_both_sides_agree() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        let sync = || run(&store, &cat, &srv, &state);
        // A playlist here not linked to this account, and Finamp playlists of its name, twice, and a name no file
        // can have.
        store.create("Chill").await.unwrap();
        let other = Target { playlist: "Chill".into(), kind: Kind::Shared, copy: None };
        state.put_target(&other).unwrap();
        srv.create_playlist("me", "Chill", &lines(&["s1"])).await.unwrap();
        srv.create_playlist("me", "Chill", &lines(&["s2"])).await.unwrap();
        srv.create_playlist("me", "AC/DC?", &lines(&["s3"])).await.unwrap();
        sync_server(&store, cat.clone(), &srv, &state, "srv", "me", true, Verify::None).await.unwrap();
        sync_server(&store, cat.clone(), &srv, &state, "srv", "me", true, Verify::None).await.unwrap();
        assert_eq!(store.entries("Chill").await.unwrap(), Vec::<String>::new(), "the one here is untouched");
        assert_eq!(store.entries("Chill 2").await.unwrap(), lines(&["A/01.mp3"]));
        assert_eq!(store.entries("Chill 3").await.unwrap(), lines(&["A/02.mp3"]));
        assert_eq!(store.entries("AC-DC-").await.unwrap(), lines(&["A/03.mp3"]));
        // The server copies took the names here.
        assert_eq!(srv.names(), lines(&["AC-DC-", "Chill 2", "Chill 3"]));
        sync().await.unwrap();
        assert_eq!(srv.names(), lines(&["AC-DC-", "Chill 2", "Chill 3"]), "settled");
        // Renamed in Finamp to a name taken here: the file gets the nearest free one, the server copy follows.
        let id = srv.lists.lock().unwrap().iter().find(|p| p.1 == "AC-DC-").unwrap().0.clone();
        srv.rename_in_finamp(&id, "Chill 2");
        sync().await.unwrap();
        sync().await.unwrap();
        assert_eq!(store.entries("Chill 4").await.unwrap(), lines(&["A/03.mp3"]));
        assert_eq!(srv.names(), lines(&["Chill 2", "Chill 3", "Chill 4"]));
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn renamed_on_both_sides_the_servers_name_leads() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        store.create("Chill").await.unwrap();
        let sync = || run(&store, &cat, &srv, &state);
        sync().await.unwrap();
        srv.rename_in_finamp("p1", "Calm");
        state.rename("Chill", "Quiet").unwrap();
        store.rename("Chill", "Quiet").await.unwrap();
        sync().await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.names(), lines(&["Calm"]));
        assert!(store.entries("Calm").await.is_ok());
        assert!(store.entries("Quiet").await.is_err());
        assert_eq!(store.view(None).await.unwrap().len(), 1, "no stray playlist from a renamed copy's old id");
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn a_track_whose_file_is_away_is_no_removal() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        store.create("Chill").await.unwrap();
        store.set_entries("Chill", &lines(&["A/01.mp3", "A/02.mp3", "A/03.mp3"])).await.unwrap();
        let sync = || run(&store, &cat, &srv, &state);
        sync().await.unwrap();
        // A/02's file is away (an unmounted folder), and meanwhile a track is added in Finamp.
        std::fs::remove_file(d.join("A/02.mp3")).unwrap();
        cat.walk_all();
        srv.edit("Chill", |v| v.push("s4".into()));
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s1", "s2", "s3", "s4"]), "the server keeps it");
        assert_eq!(store.entries("Chill").await.unwrap(), lines(&["A/01.mp3", "A/02.mp3", "A/03.mp3", "A/04.mp3"]), "its line stays");
        // Its missing line taken out here (Repair, say): a real removal, also on the server.
        std::fs::remove_file(d.join("A/02.mp3")).ok();
        store.set_entries("Chill", &lines(&["A/01.mp3", "A/03.mp3", "A/04.mp3"])).await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s1", "s3", "s4"]));
        // Its file back and added again here: once the server's item is linked again, it goes back there too.
        std::fs::write(d.join("A/02.mp3"), "track 2").unwrap();
        cat.walk_all();
        store.set_entries("Chill", &lines(&["A/01.mp3", "A/02.mp3", "A/03.mp3", "A/04.mp3"])).await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s1", "s3", "s4"]), "not linked yet: nothing to send");
        cat.record_links("srv", "lib", &[("s2".to_string(), 0, "A/02.mp3".to_string())], 3).unwrap();
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s1", "s2", "s3", "s4"]));
        let before = srv.lists.lock().unwrap()[0].3;
        sync().await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.lists.lock().unwrap()[0].3, before, "settled");
        assert_eq!(store.entries("Chill").await.unwrap(), lines(&["A/01.mp3", "A/02.mp3", "A/03.mp3", "A/04.mp3"]));
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn an_away_file_that_comes_back_is_no_change() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        store.create("Chill").await.unwrap();
        store.set_entries("Chill", &lines(&["A/01.mp3", "A/02.mp3"])).await.unwrap();
        let sync = || run(&store, &cat, &srv, &state);
        sync().await.unwrap();
        std::fs::remove_file(d.join("A/02.mp3")).unwrap();
        cat.walk_all();
        sync().await.unwrap();
        // Back, and linked again on the next listing.
        std::fs::write(d.join("A/02.mp3"), "track 2").unwrap();
        cat.walk_all();
        sync().await.unwrap();
        cat.record_links("srv", "lib", &[("s2".to_string(), 0, "A/02.mp3".to_string())], 3).unwrap();
        sync().await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s1", "s2"]));
        assert_eq!(store.entries("Chill").await.unwrap(), lines(&["A/01.mp3", "A/02.mp3"]));
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn one_playlists_problem_does_not_hold_up_the_others() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        for n in ["A1", "A2"] {
            store.create(n).await.unwrap();
        }
        run(&store, &cat, &srv, &state).await.unwrap();
        let a1 = srv.lists.lock().unwrap().iter().find(|p| p.1 == "A1").unwrap().0.clone();
        srv.refuse.lock().unwrap().push(a1);
        store.set_entries("A1", &lines(&["A/01.mp3"])).await.unwrap();
        store.set_entries("A2", &lines(&["A/02.mp3"])).await.unwrap();
        let undone = sync_server(&store, cat.clone(), &srv, &state, "srv", "me", true, Verify::None).await.unwrap();
        assert_eq!(undone.keys().collect::<Vec<_>>(), vec!["A1"]);
        assert_eq!(srv.items("A2"), lines(&["s2"]));
        // Tried again next pass.
        srv.refuse.lock().unwrap().clear();
        run(&store, &cat, &srv, &state).await.unwrap();
        assert_eq!(srv.items("A1"), lines(&["s1"]));
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn a_change_the_server_did_not_stamp_is_found_when_verifying() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        store.create("Chill").await.unwrap();
        store.set_entries("Chill", &lines(&["A/01.mp3", "A/02.mp3"])).await.unwrap();
        run(&store, &cat, &srv, &state).await.unwrap();
        run(&store, &cat, &srv, &state).await.unwrap();
        srv.edit_quietly("Chill", |v| v.reverse());
        run(&store, &cat, &srv, &state).await.unwrap();
        assert_eq!(store.entries("Chill").await.unwrap(), lines(&["A/01.mp3", "A/02.mp3"]), "not seen without a stamp");
        sync_server(&store, cat.clone(), &srv, &state, "srv", "me", true, Verify::All).await.unwrap();
        assert_eq!(store.entries("Chill").await.unwrap(), lines(&["A/02.mp3", "A/01.mp3"]));
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn reordered_on_both_sides_the_servers_order_leads_and_both_additions_stay() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        store.create("Chill").await.unwrap();
        store.set_entries("Chill", &lines(&["A/01.mp3", "A/02.mp3", "A/03.mp3"])).await.unwrap();
        let sync = || run(&store, &cat, &srv, &state);
        sync().await.unwrap();
        // Both reordered in one pass, and each added a track.
        srv.edit("Chill", |v| *v = vec!["s3".into(), "s2".into(), "s1".into(), "s4".into()]);
        store.set_entries("Chill", &lines(&["A/02.mp3", "A/01.mp3", "A/03.mp3", "gone/x.mp3"])).await.unwrap();
        sync().await.unwrap();
        sync().await.unwrap();
        assert_eq!(srv.items("Chill"), lines(&["s3", "s2", "s1", "s4"]));
        let file = store.entries("Chill").await.unwrap();
        assert_eq!(file.iter().filter(|l| l.starts_with("A/")).cloned().collect::<Vec<_>>(), lines(&["A/03.mp3", "A/02.mp3", "A/01.mp3", "A/04.mp3"]));
        assert!(file.contains(&"gone/x.mp3".to_string()), "the addition here stays");
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn an_incoming_track_stays_in_a_synced_playlist() {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!("cratarr-sync-inc-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
        let (m, inc) = (root.join("Music"), root.join("lidarr-music"));
        std::fs::create_dir_all(m.join("A")).unwrap();
        std::fs::create_dir_all(inc.join("X")).unwrap();
        std::fs::write(m.join("A/01.mp3"), "a").unwrap();
        std::fs::write(inc.join("X/01.flac"), "x").unwrap();
        let cat = Arc::new(Catalogue::open(None, vec![m.clone(), inc.clone()]).unwrap());
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        cat.record_links("srv", "lib", &[("s1".to_string(), 0, "A/01.mp3".to_string()), ("i1".to_string(), 1, "X/01.flac".to_string())], 1)
            .unwrap();
        let store = Store::new(&m);
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        // Made in Finamp with an Incoming track: it comes in as a line beside Music, and stays on both sides.
        srv.create_playlist("me", "Mix", &lines(&["s1", "i1"])).await.unwrap();
        state.put_target(&Target { playlist: "Other".into(), kind: me(), copy: None }).unwrap();
        for _ in 0..3 {
            sync_server(&store, cat.clone(), &srv, &state, "srv", "me", true, Verify::All).await.unwrap();
        }
        assert_eq!(store.entries("Mix").await.unwrap(), lines(&["A/01.mp3", "../lidarr-music/X/01.flac"]));
        assert_eq!(srv.items("Mix"), lines(&["s1", "i1"]));
        std::fs::remove_dir_all(&root).ok();
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

    #[tokio::test]
    async fn after_a_new_catalogue_both_sides_are_merged_and_nothing_is_taken_out() {
        let (d, cat, store) = library();
        let (srv, state) = (Fake::default(), SyncState::open(None).unwrap());
        store.create("Chill").await.unwrap();
        store.set_entries("Chill", &lines(&["A/01.mp3", "A/02.mp3"])).await.unwrap();
        let sync = || run(&store, &cat, &srv, &state);
        sync().await.unwrap();
        // The agreed list is forgotten (its ids were another catalogue's); meanwhile each side changed.
        assert_eq!(state.forget_baselines().unwrap(), 1);
        srv.edit("Chill", |v| v.push("s3".into()));
        store.set_entries("Chill", &lines(&["A/01.mp3", "A/02.mp3", "A/04.mp3"])).await.unwrap();
        sync().await.unwrap();
        sync().await.unwrap();
        let mut file = store.entries("Chill").await.unwrap();
        file.sort();
        assert_eq!(file, lines(&["A/01.mp3", "A/02.mp3", "A/03.mp3", "A/04.mp3"]));
        let mut server = srv.items("Chill");
        server.sort();
        assert_eq!(server, lines(&["s1", "s2", "s3", "s4"]));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn paths_relative_to_the_music_folder() {
        assert_eq!(relative_to(Path::new("/media/Music"), Path::new("/media/lidarr-music/X/01.flac")).as_deref(), Some("../lidarr-music/X/01.flac"));
        assert_eq!(relative_to(Path::new("/media/Music"), Path::new("/media/Music/A/01.mp3")).as_deref(), Some("A/01.mp3"));
    }
}
