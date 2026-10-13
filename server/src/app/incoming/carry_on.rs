//! Carry on playing (decision 0013): Keep and Replace move files, which the server then knows by new items. When the
//! album being moved is the one playing, the player goes on with the old item, which the server no longer has: it
//! cannot say what plays, so the header, voice and favourites lose track. Once the server has the new items, the same
//! track plays again from where it was, with the rest of the album after it.

use std::sync::Arc;

use super::IncomingService;
use crate::app::library::Library;

/// How long to wait for the server to have the moved files.
const WAIT_SECS: u64 = 15 * 60;
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
}

fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

impl IncomingService {
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
            });
        }
        None
    }
}

/// A playing file's track id after the move.
fn before_ids(before: &[(i64, i64)], track: Option<i64>) -> Option<i64> {
    let t = track?;
    before.iter().find(|(b, _)| *b == t).map(|(_, a)| *a)
}

/// After the move: wait in the background until the server has the new items, then play the track again from where
/// it is now, with the rest of the album after it. Gives up when the player moved on to something else, or after
/// `WAIT_SECS`.
pub(super) fn carry_on(lib: Arc<Library>, p: Playing) {
    tokio::spawn(async move {
        let started = now();
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(EVERY_SECS)).await;
            if now() - started > WAIT_SECS as f64 {
                tracing::warn!("{}: the moved track never appeared; playback left as it is", p.source);
                return;
            }
            let Some((_, s)) = lib.connected_sources().into_iter().find(|(id, _)| *id == p.source) else { return };
            let c = lib.catalogue();
            let new = c.source_id_of(&p.source, p.track).filter(|n| *n != p.old_item);
            if let Ok(Some(now_playing)) = s.now_playing().await {
                let id = now_playing.track.id;
                if Some(&id) == new.as_ref() {
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
            let ids: Vec<String> = rest.iter().filter_map(|t| c.source_id_of(&p.source, *t)).collect();
            if ids.is_empty() {
                return;
            }
            match s.play_from(&ids, position).await {
                Ok(player) => tracing::info!("carried on on {player} with the moved album, {position:.0} s in"),
                Err(e) => tracing::warn!("{}: could not carry on with the moved album: {e:#}", p.source),
            }
            return;
        }
    });
}
