//! MusicBrainz as Settings shows it (0.9.6, the owner: Connections › Discovery and metadata, no key): when Cratarr
//! last reached it, the last time it could not, and how far the background pass (original years and release types) has
//! come. Every question to MusicBrainz notes its answer (`reach`).

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use anyhow::Result;
use serde::Serialize;

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
