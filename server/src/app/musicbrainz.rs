//! MusicBrainz as Settings shows it (0.9.6, the owner: Connections › Discovery and metadata, no key): when Cratarr
//! last reached it, the last time it could not, and how far the background pass (original years and release types) has
//! come. Every question to MusicBrainz notes its answer (`reach`).

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;

/// Whether the background pass is asking now, and when it asks next (epoch s; 0: not known).
pub static PASS_RUNNING: AtomicBool = AtomicBool::new(false);
pub static PASS_NEXT: AtomicI64 = AtomicI64::new(0);
/// The background pass' pace: one question every this many seconds.
pub const PACE_SECS: u64 = 2;
/// MusicBrainz's rule is one question a second from one address: a little more apart, so two never fall in one second.
const APART: Duration = Duration::from_millis(1100);

/// The questions to MusicBrainz in turn, each `APART` after the one before, whoever asked it (S10, 0100's map 1.11). A page, a
/// pass and a Soulseek fetch asking at once share the one second between them, which a sleep of each caller's own after its
/// question could not: it saw only its own. They go in the order they came. The wait is before a question, not after it, so
/// one asked after a quiet while goes at once.
struct Turn {
    /// When the next question may go; none before the first.
    next: tokio::sync::Mutex<Option<tokio::time::Instant>>,
}

impl Turn {
    const fn new() -> Self {
        Self { next: tokio::sync::Mutex::const_new(None) }
    }

    /// Waits for a question's turn, and takes it.
    async fn take(&self) {
        let mut next = self.next.lock().await;
        if let Some(at) = *next {
            tokio::time::sleep_until(at).await;
        }
        *next = Some(tokio::time::Instant::now() + APART);
    }
}

/// The server's one turn.
static TURN: Turn = Turn::new();

/// A question to MusicBrainz: refused while it is switched off (0.9.6), asked in the server's one turn (`Turn`), its answer
/// noted. Every question to MusicBrainz goes through this, so no caller paces itself.
pub trait Asked {
    fn asked(self) -> impl std::future::Future<Output = Result<reqwest::Response>> + Send;
}

impl Asked for reqwest::RequestBuilder {
    fn asked(self) -> impl std::future::Future<Output = Result<reqwest::Response>> + Send {
        async move {
            if !super::switches::is_on(super::switches::MUSICBRAINZ) {
                anyhow::bail!("MusicBrainz is switched off");
            }
            TURN.take().await;
            Ok(super::reach::Sent::sent(self).await?)
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Failed {
    pub at: i64,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct Pass {
    pub name: &'static str,
    pub done: usize,
    pub total: usize,
    pub running: bool,
    pub next_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct Status {
    /// Switched on (0.9.6): off, Cratarr asks it nothing.
    pub enabled: bool,
    pub last_ok: Option<i64>,
    pub last_error: Option<Failed>,
    pub pace_secs: u64,
    pub passes: Vec<Pass>,
    /// The library's albums with a MusicBrainz release asked about, of all of them.
    pub asked: usize,
    pub of: usize,
}

/// How MusicBrainz stands now.
pub async fn status() -> Result<Status> {
    let (asked, works, of) = match crate::serve::instance() {
        Some(s) => {
            let (lib, ud) = (s.library.clone(), s.userdata.clone());
            tokio::task::spawn_blocking(move || -> Result<(usize, usize, usize)> {
                let releases = lib.album_releases()?;
                let typed = ud.release_types_asked("mb:", 0)?;
                let read = ud.mb_releases_asked(0, 0)?;
                Ok((releases.iter().filter(|r| typed.contains(*r)).count(), releases.iter().filter(|r| read.contains(*r)).count(), releases.len()))
            })
            .await??
        }
        None => (0, 0, 0),
    };
    let next = PASS_NEXT.load(Ordering::Relaxed);
    let reach = super::reach::state("musicbrainz");
    Ok(Status {
        enabled: super::switches::is_on(super::switches::MUSICBRAINZ),
        last_ok: Some(reach.last_ok).filter(|t| *t > 0),
        last_error: reach.last_error.map(|(at, message)| Failed { at, message }),
        pace_secs: PACE_SECS,
        passes: vec![
            Pass { name: "Release types and years", done: asked, total: of, running: PASS_RUNNING.load(Ordering::Relaxed), next_at: Some(next).filter(|t| *t > 0) },
            // The works and their composers (0.10, `works`): in turn with the release types, in the same asker.
            Pass { name: "Works and composers", done: works, total: of, running: PASS_RUNNING.load(Ordering::Relaxed), next_at: Some(next).filter(|t| *t > 0) },
        ],
        asked,
        of,
    })
}

#[cfg(test)]
mod tests {
    use tokio::time::Instant;

    use super::*;

    /// When a question asked at `start` went.
    async fn went(turn: &Turn, start: Instant) -> Duration {
        turn.take().await;
        Instant::now() - start
    }

    /// A page, a pass and a fetch asking at once: one goes, the next a turn after it, the third a turn after that (S10).
    #[tokio::test(start_paused = true)]
    async fn two_asks_at_once_are_a_turn_apart() {
        let (turn, start) = (Turn::new(), Instant::now());
        let (a, b, c) = tokio::join!(went(&turn, start), went(&turn, start), went(&turn, start));
        let mut at = [a, b, c];
        at.sort();
        assert_eq!(at, [Duration::ZERO, APART, APART * 2]);
    }

    /// The wait is for the question before: one asked after a quiet while goes at once.
    #[tokio::test(start_paused = true)]
    async fn an_ask_after_a_quiet_while_goes_at_once() {
        let turn = Turn::new();
        turn.take().await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(went(&turn, Instant::now()).await, Duration::ZERO);
    }
}
