//! MusicBrainz as Settings shows it (0.9.6, the owner: Connections › Discovery and metadata, no key): when Cratarr
//! last reached it, the last time it could not, and how far the background pass (original years and release types) has
//! come. Every question to MusicBrainz notes its answer (`Noted`).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use anyhow::Result;
use serde::Serialize;

/// When MusicBrainz last answered (epoch s; 0: not since the server started).
static LAST_OK: AtomicI64 = AtomicI64::new(0);
/// The last time it could not be asked, and why.
static LAST_ERROR: Mutex<Option<(i64, String)>> = Mutex::new(None);
/// Whether the background pass is asking now, and when it asks next (epoch s; 0: not known).
pub static PASS_RUNNING: AtomicBool = AtomicBool::new(false);
pub static PASS_NEXT: AtomicI64 = AtomicI64::new(0);
/// The background pass' pace: one question every this many seconds.
pub const PACE_SECS: u64 = 2;

/// A question to MusicBrainz: refused while it is switched off (0.9.6), its answer noted.
pub trait Asked {
    fn asked(self) -> impl std::future::Future<Output = Result<reqwest::Response>> + Send;
}

impl Asked for reqwest::RequestBuilder {
    fn asked(self) -> impl std::future::Future<Output = Result<reqwest::Response>> + Send {
        async move {
            if !super::switches::is_on(super::switches::MUSICBRAINZ) {
                anyhow::bail!("MusicBrainz is switched off");
            }
            Ok(self.send().await.noted()?)
        }
    }
}

/// A question's answer noted: reached (any answer but MusicBrainz busy or refusing), or not and why.
pub trait Noted {
    fn noted(self) -> Self;
}

impl Noted for reqwest::Result<reqwest::Response> {
    fn noted(self) -> Self {
        let now = super::moment::now_secs();
        match &self {
            Ok(r) if !(r.status().is_server_error() || r.status() == reqwest::StatusCode::TOO_MANY_REQUESTS || r.status() == reqwest::StatusCode::FORBIDDEN) => {
                LAST_OK.store(now, Ordering::Relaxed);
            }
            Ok(r) => *LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = Some((now, format!("MusicBrainz answered {}", r.status()))),
            Err(e) => *LAST_ERROR.lock().unwrap_or_else(|p| p.into_inner()) = Some((now, plain(e))),
        }
        self
    }
}

/// A request's error without its address (an artist's name in a query is no one's business in Settings).
fn plain(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "MusicBrainz did not answer in time".into()
    } else if e.is_connect() {
        "MusicBrainz could not be reached".into()
    } else {
        let said = e.to_string();
        format!("MusicBrainz: {}", said.split(" for url").next().unwrap_or(&said))
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
    let (asked, of) = match crate::serve::instance() {
        Some(s) => {
            let (lib, ud) = (s.library.clone(), s.userdata.clone());
            tokio::task::spawn_blocking(move || -> Result<(usize, usize)> {
                let releases = lib.album_releases()?;
                let typed = ud.release_types_asked("mb:", 0)?;
                Ok((releases.iter().filter(|r| typed.contains(*r)).count(), releases.len()))
            })
            .await??
        }
        None => (0, 0),
    };
    let next = PASS_NEXT.load(Ordering::Relaxed);
    Ok(Status {
        enabled: super::switches::is_on(super::switches::MUSICBRAINZ),
        last_ok: Some(LAST_OK.load(Ordering::Relaxed)).filter(|t| *t > 0),
        last_error: LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()).clone().map(|(at, message)| Failed { at, message }),
        pace_secs: PACE_SECS,
        passes: vec![Pass { name: "Release types and years", done: asked, total: of, running: PASS_RUNNING.load(Ordering::Relaxed), next_at: Some(next).filter(|t| *t > 0) }],
        asked,
        of,
    })
}
