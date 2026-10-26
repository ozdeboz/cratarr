//! Carry on playing (decision 0013): Keep and Replace move files, which the server then knows by new items. When the
//! album being moved is the one playing, the player goes on with the old item. Once the server drops it (Jellyfin's
//! real-time monitor does so about a minute after the files leave), the player's reports name an item that is gone:
//! the server cannot say what plays (header, voice and favourites lose track), and Jellyfin's database refuses those
//! reports. On 2026-10-13 its library work hung after them; why is not known, the refused reports were the only
//! anomaly (decision 0017).
//!
//! So the player stops right before the move (a stopped player reports nothing), the server hears of the new folders
//! at once and of the old ones only once the same track plays again, on its new item, from where it stopped, with the
//! rest of the album after it.

use std::sync::Arc;

use super::{IncomingService, Told, tell_moved};
use crate::app::library::Library;
use crate::journal::Event;

/// How long to wait for the server to have the moved files: the player is stopped meanwhile. A server that takes
/// longer has stopped processing changes (Jellyfin did, 2026-10-13).
const WAIT_SECS: u64 = 3 * 60;
/// How often to look.
const EVERY_SECS: u64 = 5;

/// What was playing from an album about to move.
pub(super) struct Playing {
    source: String,
    /// The catalogue track playing, as it will be known after the move.
    track: i64,
    /// The server's item for it before the move (gone after it).
    old_item: String,
    at_secs: f64,
    paused: bool,
    /// When the position was read, epoch seconds.
    read_at: f64,
    duration_secs: Option<f64>,
    /// The album's tracks from this one on, in album order, as known after the move.
    rest: Vec<i64>,
    /// The player it plays on, as the source named it: stopped, looked at and carried on on, whichever is followed by
    /// then (0.9.8: stopped on the Salon tablet, it went on on Bedroom).
    player: Option<String>,
}

fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

impl IncomingService {
    /// Stop the player before its album moves, keeping where it was. Best effort: when it cannot be stopped, carry-on
    /// still follows (and the position keeps running).
    pub(super) async fn hold(&self, p: &mut Playing) {
        let Some((_, s)) = self.lib.connected_sources().into_iter().find(|(id, _)| *id == p.source) else { return };
        if p.paused {
            return;
        }
        match s.transport_on(crate::source::Transport::Stop, p.player.as_deref()).await {
            Ok(player) => {
                p.at_secs += now() - p.read_at;
                p.read_at = now();
                p.paused = true;
                tracing::info!("stopped {player} while its album moves, {:.0} s into the track", p.at_secs);
            }
            Err(e) => tracing::warn!("{}: could not stop the player before moving its album: {e:#}", p.source),
        }
    }

    /// What plays from an album about to move: `order` is its tracks in album order as known after the move,
    /// `before` maps each id a playing file may have now to its id after. `None` when none of them plays.
    pub(super) async fn playing_among(&self, order: &[i64], before: &[(i64, i64)]) -> Option<Playing> {
        let c = self.lib.catalogue();
        for (source, s) in self.lib.connected_sources() {
            let Ok(Some(p)) = s.now_playing().await else { continue };
            let Some(after) = before_ids(before, c.track_of_source(&source, &p.track.id)) else { continue };
            let Some(at) = order.iter().position(|t| *t == after) else { continue };
            let progress = p.progress.as_ref();
            return Some(Playing {
                source,
                track: after,
                old_item: p.track.id.clone(),
                at_secs: progress.map_or(0.0, |g| g.position_secs),
                paused: progress.is_some_and(|g| g.paused),
                read_at: now(),
                duration_secs: progress.and_then(|g| g.duration_secs),
                rest: order[at..].to_vec(),
                player: p.player.clone(),
            });
        }
        None
    }
}

/// A track's id on its source after the move: a server's new one, once it has the file; Cratarr's own the same, once
/// its library has the file where it now is.
fn id_after(c: &crate::catalogue::Catalogue, source: &str, track: i64) -> Option<String> {
    if source != crate::source::cratarr::KIND.id {
        return c.source_id_of(source, track);
    }
    let song = crate::serve::ids::song(track);
    let s = crate::serve::instance()?;
    let (folder, rel) = s.library.file_of(&song).ok()??;
    let root = s.library.roots().into_iter().find(|(f, _)| *f == folder)?.1;
    std::path::Path::new(&root).join(rel).exists().then_some(song)
}

/// A playing file's track id after the move.
fn before_ids(before: &[(i64, i64)], track: Option<i64>) -> Option<i64> {
    let t = track?;
    before.iter().find(|(b, _)| *b == t).map(|(_, a)| *a)
}

/// After the move: wait in the background until the server has the new items, then play the track again from where
/// it is now, with the rest of the album after it; then tell the server the old folders are gone. Gives up (and tells
/// it) when the player moved on to something else, or after `WAIT_SECS`.
pub(super) fn carry_on(lib: Arc<Library>, p: Playing, event: &Event) {
    let (title, moved) = match event {
        Event::AlbumAdopted { title, moves, .. } => (title.clone(), moves.iter().map(|m| (m.from.clone(), m.to.clone())).collect()),
        Event::AlbumReplaced { title, moves, replaced, .. } => {
            (title.clone(), replaced.iter().chain(moves).map(|m| (m.from.clone(), m.to.clone())).collect())
        }
        Event::AlbumRejected { .. } => (String::new(), Vec::new()),
    };
    tokio::spawn(async move {
        follow(&lib, &p).await;
        tell_moved(&lib, &title, &moved, Told::Gone).await;
    });
}

async fn follow(lib: &Arc<Library>, p: &Playing) {
    {
        let started = now();
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(EVERY_SECS)).await;
            if now() - started > WAIT_SECS as f64 {
                // Where "the server stopped processing changes" is told (decision 0013).
                tracing::warn!("{}: the moved track never appeared within {WAIT_SECS} s; the player stays stopped", p.source);
                lib.server_stuck(&p.source, &format!("moved files did not show up within {} min", WAIT_SECS / 60));
                return;
            }
            let Some((_, s)) = lib.connected_sources().into_iter().find(|(id, _)| *id == p.source) else { return };
            let c = lib.catalogue();
            // Cratarr's own tracks keep their ids (0.9.6: waiting for a new one flagged it as stuck, its player stopped).
            let own = p.source == crate::source::cratarr::KIND.id;
            let new = id_after(&c, &p.source, p.track).filter(|n| own || *n != p.old_item);
            if let Ok(Some(now_playing)) = s.now_playing_on(p.player.as_deref()).await {
                let id = now_playing.track.id;
                if !own && Some(&id) == new.as_ref() {
                    return; // the server knows it again
                }
                if id != p.old_item && c.track_of_source(&p.source, &id).is_some_and(|t| t != p.track) {
                    return; // something else plays now
                }
            }
            let Some(_) = new else { continue };
            let mut position = p.at_secs + if p.paused { 0.0 } else { now() - p.read_at };
            let mut rest = p.rest.clone();
            if p.duration_secs.is_some_and(|d| position >= d - 1.0) {
                // It ended meanwhile: the next one, from its start.
                rest.remove(0);
                position = 0.0;
            }
            let ids: Vec<String> = rest.iter().filter_map(|t| id_after(&c, &p.source, *t)).collect();
            if ids.is_empty() {
                return;
            }
            // A track that was paused comes back playing: it was stopped for the move, or paused by hand.
            match s.play_from_on(&ids, position, p.player.as_deref()).await {
                Ok(player) => tracing::info!("carried on on {player} with the moved album, {position:.0} s in"),
                Err(e) => tracing::warn!("{}: could not carry on with the moved album: {e:#}", p.source),
            }
            return;
        }
    }
}
