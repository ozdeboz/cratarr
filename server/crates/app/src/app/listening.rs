//! One way in for listening (decision 0068): every live play comes in here, is written first (the history, the counts,
//! the send queues, in one transaction: `UserData::record`), and is then told on one in-process channel that anything
//! may subscribe to (the scrobbler first). A track starting on a player is told too, once per change of song.
//!
//! Imports are not live listening: they write the history themselves and tell nothing (decision 0067).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use tokio::sync::broadcast;

pub use super::listening_base::{Event, heard, heard_blocking};
use super::listening_base::{bus, tell};

/// What happens from now on.
pub fn subscribe() -> broadcast::Receiver<Event> {
    bus().subscribe()
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
