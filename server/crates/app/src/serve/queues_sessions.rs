//! What the apps report playing, as the server holds it (`Serve::now_playing`), below the app (moved from `serve::queues`,
//! which keeps their names and answers `getNowPlaying` and `reportPlayback`).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A paused app's session lasts this long after its last report (Navidrome's).
const PAUSED_FOR: Duration = Duration::from_secs(30 * 60);
/// A playing one this long after its track's end.
const AFTER_END: Duration = Duration::from_secs(5);

/// What the apps report playing, by account and app.
#[derive(Default)]
pub struct Sessions(Mutex<HashMap<(String, String), Session>>);

#[derive(Debug, Clone)]
pub struct Session {
    pub song: String,
    pub state: &'static str,
    pub position_ms: u64,
    pub rate: f64,
    pub duration_ms: u64,
    /// When the report came.
    pub at: Instant,
    /// The same, epoch seconds.
    pub at_epoch: i64,
}

impl Session {
    pub fn moving(&self) -> bool {
        self.state == "playing"
    }

    /// Where it is now: counted on from the report while it plays, never past the end.
    pub fn position_now(&self) -> u64 {
        if !self.moving() {
            return self.position_ms;
        }
        let gone = self.at.elapsed().as_millis() as f64 * self.rate;
        (self.position_ms + gone as u64).min(self.duration_ms.max(self.position_ms))
    }

    pub fn over(&self) -> bool {
        let lasts = if self.moving() && self.rate > 0.0 {
            let left = self.duration_ms.saturating_sub(self.position_ms) as f64 / self.rate;
            Duration::from_millis(left as u64) + AFTER_END
        } else {
            PAUSED_FOR
        };
        self.at.elapsed() > lasts
    }
}

impl Sessions {
    pub fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), Session>> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn live(&self) -> Vec<((String, String), Session)> {
        let mut all = self.lock();
        all.retain(|_, s| !s.over());
        all.iter().map(|(k, s)| (k.clone(), s.clone())).collect()
    }
}
