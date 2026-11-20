//! `listening`'s way in, in base: a track heard, written and then told on the one in-process channel (decision 0068),
//! as Cratarr's own source counts a play. `listening` re-exports it, and keeps who listens and what starts.

use std::sync::{Arc, OnceLock};

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

pub fn bus() -> &'static broadcast::Sender<Event> {
    static BUS: OnceLock<broadcast::Sender<Event>> = OnceLock::new();
    BUS.get_or_init(|| broadcast::channel(KEPT).0)
}

pub fn tell(e: Event) {
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
    // An old id plays on in a queue no take reached, and what is heard of it is the track's.
    let song = serve.song_now(song);
    let Some((pid, album, artists)) = serve.library.play_targets(&song)? else { return Ok(false) };
    let new = serve.userdata.record(user, (&song, &pid), &album, &artists, h)?;
    if new {
        tell(Event::Heard { user: user.to_string(), song, at: h.at, counted: h.counts });
    }
    Ok(new)
}

/// As [`heard_blocking`], from async code.
pub async fn heard(serve: &Arc<Serve>, user: &str, song: &str, h: &Heard) -> anyhow::Result<bool> {
    let (s, u, id, h) = (serve.clone(), user.to_string(), song.to_string(), h.clone());
    crate::run::spawn_blocking(move || heard_blocking(&s, &u, &id, &h)).await?
}
