//! One way in for listening (decision 0068): every live play comes in here, is written first (the history, the counts,
//! the send queues, in one transaction: `UserData::record`), and is then told on one in-process channel that anything
//! may subscribe to (the scrobbler first). A track starting on a player is told too, once per change of song.
//!
//! Imports are not live listening: they write the history themselves and tell nothing (decision 0067).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::broadcast;

use crate::serve::Serve;
use crate::userdata::Heard;

/// What happened.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A track began on a player (`player`: its name as Play On or the Subsonic app knows it).
    Started { user: String, player: String, song: String, at: i64 },
    /// A track heard, written already; `counted` when it counted as a play.
    Heard { user: String, song: String, at: i64, counted: bool },
}

/// Events kept for a subscriber that is a moment behind; one further behind skips what it missed.
const KEPT: usize = 256;

fn bus() -> &'static broadcast::Sender<Event> {
    static BUS: OnceLock<broadcast::Sender<Event>> = OnceLock::new();
    BUS.get_or_init(|| broadcast::channel(KEPT).0)
}

/// What happens from now on.
pub fn subscribe() -> broadcast::Receiver<Event> {
    bus().subscribe()
}

fn tell(e: Event) {
    // No subscriber is no error.
    let _ = bus().send(e);
}

/// A track heard (decision 0064) by `user`: written (logged, counted when it counts, queued for the connected
/// services), then told. False when the play was there already (a player telling it again). Blocking.
pub fn heard_blocking(serve: &Serve, user: &str, song: &str, h: &Heard) -> anyhow::Result<bool> {
    // A preview is not listening (decision 0071).
    if crate::bandcamp::is_preview(song) {
        return Ok(false);
    }
    // Its play history off (0.9.5): nothing kept, counted or sent.
    if !serve.userdata.tracking(user) {
        return Ok(false);
    }
    let Some((pid, album, artists)) = serve.library.play_targets(song)? else { return Ok(false) };
    let new = serve.userdata.record(user, (song, &pid), &album, &artists, h)?;
    if new {
        tell(Event::Heard { user: user.to_string(), song: song.to_string(), at: h.at, counted: h.counts });
    }
    Ok(new)
}

/// As [`heard_blocking`], from async code.
pub async fn heard(serve: &Arc<Serve>, user: &str, song: &str, h: &Heard) -> anyhow::Result<bool> {
    let (s, u, id, h) = (serve.clone(), user.to_string(), song.to_string(), h.clone());
    tokio::task::spawn_blocking(move || heard_blocking(&s, &u, &id, &h)).await?
}

/// A track began on `player` for `user`: told once per change of song on that player (players report often).
pub fn started(user: &str, player: &str, song: &str) {
    if crate::bandcamp::is_preview(song) {
        return;
    }
    static PLAYING: OnceLock<Mutex<HashMap<(String, String), String>>> = OnceLock::new();
    let mut playing = PLAYING.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner());
    let key = (user.to_string(), player.to_string());
    if playing.get(&key).is_some_and(|s| s == song) {
        return;
    }
    playing.insert(key, song.to_string());
    drop(playing);
    // Its play history off (0.9.5): no "now playing" either.
    if crate::serve::instance().is_some_and(|s| !s.userdata.tracking(user)) {
        return;
    }
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    tell(Event::Started { user: user.to_string(), player: player.to_string(), song: song.to_string(), at });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_is_told_once_per_change_of_song_on_a_player() {
        let mut rx = subscribe();
        started("me", "Salon tablet", "tr-1");
        started("me", "Salon tablet", "tr-1");
        started("me", "Phone", "tr-1");
        started("me", "Salon tablet", "tr-2");
        let mut told = Vec::new();
        while let Ok(e) = rx.try_recv() {
            if let Event::Started { player, song, .. } = e {
                told.push((player, song));
            }
        }
        // Other tests may tell too: only ours count.
        told.retain(|(p, _)| p == "Salon tablet" || p == "Phone");
        assert_eq!(told, [("Salon tablet".into(), "tr-1".into()), ("Phone".into(), "tr-1".into()), ("Salon tablet".into(), "tr-2".into())]);
    }
}
