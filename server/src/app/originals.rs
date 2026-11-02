//! Albums' original years from MusicBrainz (0.9.5, the owner: Ott's Skylon showed 2012, its reissue's year, not 2008).
//! Every album whose files carry no original date but a MusicBrainz release gets its release group's first release
//! year, asked in the background one album at a time and kept (user data `original_years`); the library is built with
//! it (`Library::set_originals`), no file changed. The same question gives the release group's types (0.9.6, the owner:
//! hide EPs and singles), asked of every album with a release and kept in `release_types` (`mb:<release>`) as said: the
//! owner's rule reads it as the library is built (0.13, `serve::library`; it replaced 0.10's edition rule).

use std::time::Duration;

use anyhow::Result;
use super::musicbrainz::Asked as _;

/// Releases asked about in one pass, kept together at its end: each keeping builds the library again.
const BATCH: usize = 500;
/// Between two questions: half Discover's pace, so the two together stay within MusicBrainz's one a second.
const PAUSE: Duration = Duration::from_secs(super::musicbrainz::PACE_SECS);
/// A release MusicBrainz knew no date for is asked about again after this.
const AGAIN_SECS: i64 = 90 * 86_400;
/// Nothing left to ask: looked at again after this (new albums).
const IDLE: Duration = Duration::from_secs(3600);
/// MusicBrainz busy (503): once more after this.
const BUSY: Duration = Duration::from_secs(3);

/// The background asker, from the server's start (a few minutes in, after its own work) until it stops.
pub fn start() {
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_secs(300)).await;
        loop {
            // Switched off (0.9.6): looked at again later.
            if !super::switches::is_on(super::switches::MUSICBRAINZ) {
                super::musicbrainz::PASS_NEXT.store(0, std::sync::atomic::Ordering::Relaxed);
                tokio::time::sleep(IDLE).await;
                continue;
            }
            super::musicbrainz::PASS_RUNNING.store(true, std::sync::atomic::Ordering::Relaxed);
            let asked = pass().await.unwrap_or_else(|e| {
                tracing::info!("original years: MusicBrainz could not be asked now: {e:#}");
                0
            });
            // Nothing left of these: the works (0.10, `works`), in turn with them, so the two keep one pace.
            let asked = match asked {
                0 => super::works::pass().await.unwrap_or_else(|e| {
                    tracing::info!("works: MusicBrainz could not be asked now: {e:#}");
                    0
                }),
                n => n,
            };
            super::musicbrainz::PASS_RUNNING.store(false, std::sync::atomic::Ordering::Relaxed);
            // Settings shows when it asks next (0.9.6).
            let wait = if asked == 0 { IDLE } else { Duration::ZERO };
            super::musicbrainz::PASS_NEXT.store(super::moment::now_secs() + wait.as_secs() as i64, std::sync::atomic::Ordering::Relaxed);
            tokio::time::sleep(wait).await;
        }
    });
}

/// One pass: up to `BATCH` releases not asked about yet, their years kept at the end. How many were asked.
async fn pass() -> Result<usize> {
    let Some(served) = crate::serve::instance() else { return Ok(0) };
    let now = super::moment::now_secs();
    let (lib, ud) = (served.library.clone(), served.userdata.clone());
    // (release, its year wanted): those without a year not asked yet, then those whose types are not asked yet.
    let due: Vec<(String, bool)> = tokio::task::spawn_blocking(move || -> Result<Vec<(String, bool)>> {
        let asked = ud.original_years_asked(now - AGAIN_SECS)?;
        let typed = ud.release_types_asked("mb:", now - AGAIN_SECS)?;
        let years: std::collections::HashSet<String> = lib.releases_without_original()?.into_iter().filter(|r| !asked.contains(r)).collect();
        let mut due: Vec<(String, bool)> = years.iter().map(|r| (r.clone(), true)).collect();
        due.extend(lib.album_releases()?.into_iter().filter(|r| !typed.contains(r) && !years.contains(r)).map(|r| (r, false)));
        due.truncate(BATCH);
        Ok(due)
    })
    .await??;
    if due.is_empty() {
        return Ok(0);
    }
    let http = super::discover::http()?;
    let (mut found, mut types) = (Vec::with_capacity(due.len()), Vec::with_capacity(due.len()));
    for (release, year_wanted) in &due {
        match release_group(&http, release).await {
            Ok(group) => {
                let (year, primary, secondary) = group.unwrap_or_default();
                if *year_wanted {
                    found.push((release.clone(), year));
                }
                types.push((format!("mb:{release}"), primary, secondary));
            }
            // MusicBrainz out of reach: what was found is kept, the rest asked next time.
            Err(e) => {
                tracing::info!("original years: stopped after {} of {}: {e:#}", found.len(), due.len());
                break;
            }
        }
        tokio::time::sleep(PAUSE).await;
    }
    let n = types.len();
    let ud = served.userdata.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let at = super::moment::now_secs();
        ud.set_original_years(&found, at)?;
        ud.set_release_types(&types, at)
    })
    .await??;
    tracing::info!("original years and release types: {n} albums asked of MusicBrainz");
    Ok(n)
}

/// What a release's release group says: (its first release year, its primary type, its secondary types); none where
/// there is no such release.
type Group = (Option<u32>, Option<String>, Vec<String>);

/// One question to MusicBrainz, once more when it is busy; none where it has no such thing.
async fn ask(http: &reqwest::Client, url: &str) -> Result<Option<serde_json::Value>> {
    let mut again = true;
    loop {
        let r = http.get(url).asked().await?;
        match r.status() {
            reqwest::StatusCode::SERVICE_UNAVAILABLE if std::mem::take(&mut again) => {
                tokio::time::sleep(BUSY).await;
                continue;
            }
            reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::BAD_REQUEST => return Ok(None),
            _ => return Ok(Some(r.error_for_status()?.json().await?)),
        }
    }
}

async fn release_group(http: &reqwest::Client, release: &str) -> Result<Option<Group>> {
    if !super::discogs::is_mbid(release) {
        return Ok(None);
    }
    let Some(v) = ask(http, &format!("https://musicbrainz.org/ws/2/release/{release}?inc=release-groups&fmt=json")).await? else { return Ok(None) };
    let g = &v["release-group"];
    let words = g["primary-type"].as_str().into_iter().chain(g["secondary-types"].as_array().into_iter().flatten().filter_map(|t| t.as_str()));
    let (primary, secondary) = crate::serve::library::release_words(words);
    Ok(Some((year_of(&v), primary, secondary)))
}

/// The year of a release's `release-group.first-release-date` ("2008-01-25", "2008").
fn year_of(release: &serde_json::Value) -> Option<u32> {
    release["release-group"]["first-release-date"].as_str()?.get(..4)?.parse().ok().filter(|y| *y > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_year_is_the_release_group_s_first() {
        let skylon = serde_json::json!({ "date": "2012-09-24", "release-group": { "first-release-date": "2008-01-25" } });
        assert_eq!(year_of(&skylon), Some(2008));
        assert_eq!(year_of(&serde_json::json!({ "release-group": { "first-release-date": "1997" } })), Some(1997));
        assert_eq!(year_of(&serde_json::json!({ "release-group": { "first-release-date": "" } })), None);
        assert_eq!(year_of(&serde_json::json!({ "date": "2012" })), None);
    }
}
