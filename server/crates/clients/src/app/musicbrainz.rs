//! MusicBrainz as Settings shows it (0.9.6, the owner: Connections › Discovery and metadata, no key): when Cratarr
//! last reached it, the last time it could not, and how far the background pass (original years and release types) has
//! come. Every question to MusicBrainz notes its answer (`reach`).

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;

use super::turn::{MUSICBRAINZ, Turn};

/// Whether the background pass is asking now, and when it asks next (epoch s; 0: not known).
pub static PASS_RUNNING: AtomicBool = AtomicBool::new(false);
pub static PASS_NEXT: AtomicI64 = AtomicI64::new(0);
/// The background pass' pace: one question every this many seconds.
pub const PACE_SECS: u64 = 2;
/// How long a question waits out a hold before it is told until when instead (OQ-T1 (a), TN.13): a page's, so it never hangs for
/// minutes, and the background asker's alike, so its loop takes other work.
pub const PATIENCE: Duration = Duration::from_secs(10);

/// An answer heard (O5, TN.3): its limit (a 503, a 429) holds the turn's questions, 5 s doubling in a row to a minute, or as long as
/// its Retry-After says (a quarter of an hour at most); any other answer ends the run. The wait, after its limit.
fn answered(turn: &Turn, status: reqwest::StatusCode, retry_after: Option<u64>) -> Option<Duration> {
    turn.heard(MUSICBRAINZ.limits.contains(&status.as_u16()), retry_after.map(Duration::from_secs))
}

/// The server's one turn (S10, TN).
pub(crate) static TURN: Turn = Turn::new(MUSICBRAINZ);

tokio::task_local! {
    /// Set for the background asker's questions (`behind`).
    static BEHIND: bool;
}

/// `f`'s questions to MusicBrainz go behind any other's (`Turn`): the background asker's passes, which no one waits for.
pub async fn behind<F: std::future::Future>(f: F) -> F::Output {
    BEHIND.scope(true, f).await
}

/// A question to MusicBrainz: refused while it is switched off (0.9.6), asked in the server's one turn (`Turn`), its answer
/// noted. Every question to MusicBrainz goes through this, so no caller paces itself. The background asker's (`behind`) is refused
/// while background reading is paused (Y7), so a running pass stops at its next question; a page's still goes. Its limit answer
/// (a 503, a 429) is the pause it asks for (TN.1), and a hold past `PATIENCE` is told at once, not waited out (OQ-T1 (a)).
pub trait Asked {
    fn asked(self) -> impl std::future::Future<Output = Result<reqwest::Response>> + Send;
}

impl Asked for reqwest::RequestBuilder {
    fn asked(self) -> impl std::future::Future<Output = Result<reqwest::Response>> + Send {
        asked_in(&TURN, self)
    }
}

/// `Asked`, in `turn` (the tests' own).
pub async fn asked_in(turn: &Turn, req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
    if !super::switches::is_on(super::switches::MUSICBRAINZ) {
        return Err(super::references_turn::SwitchedOff("MusicBrainz").into());
    }
    let behind = BEHIND.try_with(|b| *b).unwrap_or(false);
    // Looked at before its turn and again once it comes (Y7): a pass's question may wait long behind a page's.
    let paused = || behind && super::switches::background_paused();
    if paused() {
        anyhow::bail!(super::switches::PAUSED);
    }
    turn.take_within(behind, PATIENCE).await?;
    if paused() {
        anyhow::bail!(super::switches::PAUSED);
    }
    let r = super::reach::Sent::sent(req).await?;
    let retry_after = super::turn::retry_after(r.headers(), super::moment::now_secs());
    if answered(turn, r.status(), retry_after.map(|d| d.as_secs())).is_some() {
        return Err(turn.paused().into());
    }
    Ok(r)
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
    let (asked, works, of) = match crate::serve::view::get() {
        Some(s) => {
            let (lib, ud) = (s.library().clone(), s.userdata().clone());
            crate::run::spawn_blocking(move || -> Result<(usize, usize, usize)> {
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

    /// Y7: while background reading is paused the background asker's question is refused, saying so, so a running pass stops at its
    /// next question; a page's still goes. Unpaused, the pass's goes again.
    #[tokio::test]
    async fn a_running_pass_stops_at_the_pause() {
        use super::super::switches::{BACKGROUND, PAUSED, TEST_STATE, set_for_tests};
        let _one = TEST_STATE.lock().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/ws/2/release", listener.local_addr().unwrap());
        crate::run::spawn(async move { axum::serve(listener, axum::Router::new().route("/ws/2/release", axum::routing::get(|| async { "{}" }))).await.unwrap() });
        let ask = || reqwest::Client::new().get(&url).asked();
        let was = set_for_tests(BACKGROUND, false);
        let pass = behind(ask()).await;
        let page = ask().await;
        set_for_tests(BACKGROUND, true);
        let again = behind(ask()).await;
        set_for_tests(BACKGROUND, was);
        assert!(pass.is_err_and(|e| e.to_string() == PAUSED), "a pass's question refused");
        assert!(page.is_ok_and(|r| r.status().is_success()), "a page's question still goes");
        assert!(again.is_ok(), "unpaused, the pass's goes again");
    }

    /// O5: a 503 pushes the turn's next question out, 5 s doubling to a minute in a row, as long as Retry-After says where it says;
    /// any other answer ends the run.
    #[tokio::test]
    async fn a_503_pushes_musicbrainz_back() {
        let t = Turn::new(MUSICBRAINZ);
        let busy = reqwest::StatusCode::SERVICE_UNAVAILABLE;
        let secs = |d: Option<Duration>| d.map(|d| d.as_secs());
        assert_eq!(secs(answered(&t, busy, None)), Some(5));
        assert_eq!(secs(answered(&t, busy, None)), Some(10));
        assert_eq!(secs(answered(&t, busy, None)), Some(20));
        assert_eq!(secs(answered(&t, busy, Some(7))), Some(7), "Retry-After honoured");
        for _ in 0..4 {
            answered(&t, busy, None);
        }
        assert_eq!(secs(answered(&t, busy, None)), Some(60), "at most a minute");
        let held = t.held_at().unwrap();
        assert!(held > Instant::now() + Duration::from_secs(50), "the next question waits");
        assert_eq!(answered(&t, reqwest::StatusCode::OK, None), None);
        assert_eq!(secs(answered(&t, busy, None)), Some(5), "a run ends at any other answer");
    }

    /// Y8: a 503 holds the questions already waiting for the turn, not only the next: three wait while the first's answer is a 503,
    /// and none goes before its back-off is out, each a turn after the one before.
    #[tokio::test(start_paused = true)]
    async fn a_503_holds_the_questions_already_waiting() {
        let (turn, start) = (Turn::new(MUSICBRAINZ), Instant::now());
        turn.take(false).await;
        let busy = async {
            tokio::task::yield_now().await;
            answered(&turn, reqwest::StatusCode::SERVICE_UNAVAILABLE, None)
        };
        let (a, b, c, wait) = tokio::join!(went(&turn, start), went(&turn, start), went(&turn, start), busy);
        assert_eq!(wait, Some(MUSICBRAINZ.first));
        let mut at = [a, b, c];
        at.sort();
        assert_eq!(at, [MUSICBRAINZ.first, MUSICBRAINZ.first + MUSICBRAINZ.apart, MUSICBRAINZ.first + MUSICBRAINZ.apart * 2]);
    }

    /// When a question asked at `start` went.
    async fn went(turn: &Turn, start: Instant) -> Duration {
        turn.take(false).await;
        Instant::now() - start
    }

    /// A page, a pass and a fetch asking at once: one goes, the next a turn after it, the third a turn after that (S10).
    #[tokio::test(start_paused = true)]
    async fn two_asks_at_once_are_a_turn_apart() {
        let (turn, start) = (Turn::new(MUSICBRAINZ), Instant::now());
        let (a, b, c) = tokio::join!(went(&turn, start), went(&turn, start), went(&turn, start));
        let mut at = [a, b, c];
        at.sort();
        assert_eq!(at, [Duration::ZERO, MUSICBRAINZ.apart, MUSICBRAINZ.apart * 2]);
    }

    /// The wait is for the question before: one asked after a quiet while goes at once.
    #[tokio::test(start_paused = true)]
    async fn an_ask_after_a_quiet_while_goes_at_once() {
        let turn = Turn::new(MUSICBRAINZ);
        turn.take(false).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(went(&turn, Instant::now()).await, Duration::ZERO);
    }

    /// R3: the background asker's question waits while a page's waits: the page goes first, though it came after it, and the
    /// background's a turn later; with none waiting it goes at its turn as any other.
    #[tokio::test(start_paused = true)]
    async fn a_page_asks_before_the_background() {
        let (turn, start) = (Turn::new(MUSICBRAINZ), Instant::now());
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
        assert_eq!((page, background), (MUSICBRAINZ.apart, MUSICBRAINZ.apart * 2));
        tokio::time::sleep(Duration::from_secs(5)).await;
        let quiet = Instant::now();
        turn.take(true).await;
        assert_eq!(Instant::now() - quiet, Duration::ZERO);
    }

    /// TN.13 (OQ-T1 (a)): MusicBrainz held a minute (its global limiter's 503 as captured, saying `Retry-After: 60`). A moment later
    /// a page's two questions (a record not had opened, an administrator's track search) and the background asker's are told until
    /// when at once, none held, and nothing is asked meanwhile. A hold shorter than a page's patience (`Retry-After: 4`) is waited out,
    /// and the page has its answer. The fake sees only Cratarr's User-Agent, its web service's paths, and nothing before a deadline.
    #[tokio::test]
    async fn a_page_is_told_until_when_not_held() {
        use std::sync::{Arc, Mutex};
        use super::super::turn::{Failure, Pinned, class, fakes};
        let _one = super::super::switches::TEST_STATE.lock().await;
        let pinned = Pinned::now();
        let t0 = pinned.0;
        let asked = Arc::new(Mutex::new(Vec::<(std::time::Instant, String)>::new()));
        let violations = Arc::new(Mutex::new(Vec::<String>::new()));
        let (seen, wrong) = (asked.clone(), violations.clone());
        let answer = move |uri: axum::http::Uri, headers: axum::http::HeaderMap| {
            let path = uri.path().to_string();
            if !path.starts_with("/ws/2/") || headers.get(axum::http::header::USER_AGENT).is_none_or(|a| a.as_bytes() != super::super::reach::USER_AGENT.as_bytes()) {
                wrong.lock().unwrap().push(format!("asked {path} as {headers:?}"));
            }
            seen.lock().unwrap().push((std::time::Instant::now(), path.clone()));
            std::future::ready(match path.as_str() {
                "/ws/2/release/held" => fakes::musicbrainz_busy(Some(Some("60"))),
                "/ws/2/release/short" => fakes::musicbrainz_busy(Some(Some("4"))),
                _ => axum::response::IntoResponse::into_response(axum::Json(serde_json::json!({ "id": "r1", "title": "Tide Tables" }))),
            })
        };
        let at = fakes::serve(axum::Router::new().fallback(answer)).await;
        let http = reqwest::Client::builder().user_agent(super::super::reach::USER_AGENT).build().unwrap();
        let release = |id: &str| http.get(format!("{at}/ws/2/release/{id}"));

        let turn = Turn::new(MUSICBRAINZ);
        let held = asked_in(&turn, release("held")).await.unwrap_err();
        assert_eq!(class(&held), Failure::Paused { until: t0 + 60 });
        tokio::time::sleep(Duration::from_secs(1)).await;
        let before = Instant::now();
        let record_page = asked_in(&turn, release("r1")).await;
        let track_search = asked_in(&turn, release("r2")).await;
        let pass = behind(asked_in(&turn, release("r3"))).await;
        let took = Instant::now() - before;
        for (who, r) in [("a record's page", record_page), ("a track search", track_search), ("the background", pass)] {
            assert_eq!(class(&r.unwrap_err()), Failure::Paused { until: t0 + 60 }, "{who}: told until when");
        }
        assert!(took < Duration::from_secs(1) && took < PATIENCE, "never held: {took:?}");
        assert_eq!(turn.paused_until(), Some(t0 + 60), "every asker reads until when");

        // Shorter than a page's patience: waited out.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let short = Turn::new(MUSICBRAINZ);
        let held = asked_in(&short, release("short")).await.unwrap_err();
        assert_eq!(class(&held), Failure::Paused { until: t0 + 4 });
        tokio::time::sleep(Duration::from_secs(1)).await;
        let before = Instant::now();
        let page = asked_in(&short, release("r1")).await.expect("waited out, and answered");
        let waited = Instant::now() - before;
        assert!(page.status().is_success(), "{}", page.status());
        assert_eq!(page.json::<serde_json::Value>().await.unwrap()["title"], "Tide Tables", "the page has its answer");
        assert!(waited >= Duration::from_millis(2900) && waited < PATIENCE, "{waited:?}");

        let asked = asked.lock().unwrap().clone();
        let paths: Vec<&str> = asked.iter().map(|(_, p)| p.as_str()).collect();
        assert_eq!(paths, ["/ws/2/release/held", "/ws/2/release/short", "/ws/2/release/r1"], "nothing asked while held");
        assert!(asked[2].0 - asked[1].0 >= Duration::from_secs(4) && asked[1].0 - asked[0].0 >= Duration::from_millis(1100), "none before a deadline");
        assert!(violations.lock().unwrap().is_empty(), "{:?}", violations.lock().unwrap());
    }
}
