//! MusicBrainz as Settings shows it (0.9.6, the owner: Connections › Discovery and metadata, no key): when Cratarr
//! last reached it, the last time it could not, and how far the background pass (original years and release types) has
//! come. Every question to MusicBrainz notes its answer (`reach`).

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
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
/// After a 503 the turn's next question waits this long, doubling on each in a row up to `BACK_OFF_MOST` (O5); as long as
/// Retry-After says where MusicBrainz says.
const BACK_OFF_FIRST: Duration = Duration::from_secs(5);
const BACK_OFF_MOST: Duration = Duration::from_secs(60);

/// The questions to MusicBrainz in turn, each `APART` after the one before, whoever asked it (S10, 0100's map 1.11). A page, a
/// pass and a Soulseek fetch asking at once share the one second between them, which a sleep of each caller's own after its
/// question could not: it saw only its own. They go in the order they came, but for the background asker's (`behind`): one of
/// those goes only when no other question waits, so a page asking meanwhile goes first, however long the pass. The wait is
/// before a question, not after it, so one asked after a quiet while goes at once.
struct Turn {
    /// When the next question may go; none before the first.
    next: tokio::sync::Mutex<Option<tokio::time::Instant>>,
    /// Questions waiting that are not the background's.
    waiting: AtomicUsize,
    /// Told when none waits any more.
    quiet: tokio::sync::Notify,
    /// The 503s' wait in a row (O5), none after any other answer.
    backing: std::sync::Mutex<Option<Duration>>,
}

/// A question waiting that is not the background's, counted while it waits, a question dropped meanwhile too.
struct Waiting<'t>(&'t Turn);

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        if self.0.waiting.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.quiet.notify_waiters();
        }
    }
}

impl Turn {
    const fn new() -> Self {
        Self { next: tokio::sync::Mutex::const_new(None), waiting: AtomicUsize::new(0), quiet: tokio::sync::Notify::const_new(), backing: std::sync::Mutex::new(None) }
    }

    /// An answer heard (O5): a 503 pushes the turn's next question out, `BACK_OFF_FIRST` doubling in a row to `BACK_OFF_MOST`, or as
    /// long as its Retry-After says (at most a quarter of an hour); any other answer ends the run. The wait, after a 503.
    async fn answered(&self, status: reqwest::StatusCode, retry_after: Option<u64>) -> Option<Duration> {
        let wait = {
            let mut backing = self.backing.lock().unwrap_or_else(|p| p.into_inner());
            if status != reqwest::StatusCode::SERVICE_UNAVAILABLE {
                *backing = None;
                return None;
            }
            let base = backing.map_or(BACK_OFF_FIRST, |d| (d * 2).min(BACK_OFF_MOST));
            *backing = Some(base);
            retry_after.map_or(base, |secs| Duration::from_secs(secs.min(15 * 60)))
        };
        let at = tokio::time::Instant::now() + wait;
        let mut next = self.next.lock().await;
        if next.is_none_or(|n| n < at) {
            *next = Some(at);
        }
        tracing::warn!(target: "musicbrainz", secs = wait.as_secs(), "MusicBrainz answered 503: its next question waits");
        Some(wait)
    }

    /// Waits for a question's turn, and takes it. One `behind` waits while another waits, and holds no place in the line while
    /// it sleeps: woken, it looks again, so a question that came meanwhile goes before it.
    async fn take(&self, behind: bool) {
        if !behind {
            self.waiting.fetch_add(1, Ordering::SeqCst);
            let _waiting = Waiting(self);
            let mut next = self.next.lock().await;
            if let Some(at) = *next {
                tokio::time::sleep_until(at).await;
            }
            *next = Some(tokio::time::Instant::now() + APART);
            return;
        }
        loop {
            let quiet = self.quiet.notified();
            if self.waiting.load(Ordering::SeqCst) > 0 {
                quiet.await;
                continue;
            }
            let mut next = self.next.lock().await;
            match *next {
                Some(at) if at > tokio::time::Instant::now() => {
                    drop(next);
                    tokio::time::sleep_until(at).await;
                }
                _ if self.waiting.load(Ordering::SeqCst) > 0 => {}
                _ => {
                    *next = Some(tokio::time::Instant::now() + APART);
                    return;
                }
            }
        }
    }
}

/// The server's one turn.
static TURN: Turn = Turn::new();

tokio::task_local! {
    /// Set for the background asker's questions (`behind`).
    static BEHIND: bool;
}

/// `f`'s questions to MusicBrainz go behind any other's (`Turn`): the background asker's passes, which no one waits for.
pub async fn behind<F: std::future::Future>(f: F) -> F::Output {
    BEHIND.scope(true, f).await
}

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
            TURN.take(BEHIND.try_with(|b| *b).unwrap_or(false)).await;
            let r = super::reach::Sent::sent(self).await?;
            let retry_after = r.headers().get(reqwest::header::RETRY_AFTER).and_then(|v| v.to_str().ok()?.trim().parse::<u64>().ok());
            TURN.answered(r.status(), retry_after).await;
            Ok(r)
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

    /// O5: a 503 pushes the turn's next question out, 5 s doubling to a minute in a row, as long as Retry-After says where it says;
    /// any other answer ends the run.
    #[tokio::test]
    async fn a_503_pushes_musicbrainz_back() {
        let t = Turn::new();
        let busy = reqwest::StatusCode::SERVICE_UNAVAILABLE;
        let secs = |d: Option<Duration>| d.map(|d| d.as_secs());
        assert_eq!(secs(t.answered(busy, None).await), Some(5));
        assert_eq!(secs(t.answered(busy, None).await), Some(10));
        assert_eq!(secs(t.answered(busy, None).await), Some(20));
        assert_eq!(secs(t.answered(busy, Some(7)).await), Some(7), "Retry-After honoured");
        for _ in 0..4 {
            t.answered(busy, None).await;
        }
        assert_eq!(secs(t.answered(busy, None).await), Some(60), "at most a minute");
        let next = t.next.lock().await.unwrap();
        assert!(next > Instant::now() + Duration::from_secs(50), "the next question waits");
        assert_eq!(t.answered(reqwest::StatusCode::OK, None).await, None);
        assert_eq!(secs(t.answered(busy, None).await), Some(5), "a run ends at any other answer");
    }

    /// When a question asked at `start` went.
    async fn went(turn: &Turn, start: Instant) -> Duration {
        turn.take(false).await;
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
        turn.take(false).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(went(&turn, Instant::now()).await, Duration::ZERO);
    }

    /// R3: the background asker's question waits while a page's waits: the page goes first, though it came after it, and the
    /// background's a turn later; with none waiting it goes at its turn as any other.
    #[tokio::test(start_paused = true)]
    async fn a_page_asks_before_the_background() {
        let (turn, start) = (Turn::new(), Instant::now());
        turn.take(false).await;
        let background = async {
            turn.take(true).await;
            Instant::now() - start
        };
        let page = async {
            tokio::task::yield_now().await;
            went(&turn, start).await
        };
        let (background, page) = tokio::join!(background, page);
        assert_eq!((page, background), (APART, APART * 2));
        tokio::time::sleep(Duration::from_secs(5)).await;
        let quiet = Instant::now();
        turn.take(true).await;
        assert_eq!(Instant::now() - quiet, Duration::ZERO);
    }
}
