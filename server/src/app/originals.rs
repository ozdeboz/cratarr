//! Albums' original years from MusicBrainz (0.9.5, the owner: Ott's Skylon showed 2012, its reissue's year, not 2008).
//! Every album whose files carry no original date but a MusicBrainz release gets its release group's first release
//! year, asked in the background one album at a time and kept (user data `original_years`); the library is built with
//! it (`Library::set_originals`), no file changed.

use std::time::Duration;

use anyhow::Result;

/// Releases asked about in one pass, kept together at its end: each keeping builds the library again.
const BATCH: usize = 500;
/// Between two questions: half Discover's pace, so the two together stay within MusicBrainz's one a second.
const PAUSE: Duration = Duration::from_secs(2);
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
            let asked = pass().await.unwrap_or_else(|e| {
                tracing::info!("original years: MusicBrainz could not be asked now: {e:#}");
                0
            });
            if asked == 0 {
                tokio::time::sleep(IDLE).await;
            }
        }
    });
}

/// One pass: up to `BATCH` releases not asked about yet, their years kept at the end. How many were asked.
async fn pass() -> Result<usize> {
    let Some(served) = crate::serve::instance() else { return Ok(0) };
    let now = super::moment::now_secs();
    let (lib, ud) = (served.library.clone(), served.userdata.clone());
    let due: Vec<String> = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
        let asked = ud.original_years_asked(now - AGAIN_SECS)?;
        Ok(lib.releases_without_original()?.into_iter().filter(|r| !asked.contains(r)).take(BATCH).collect())
    })
    .await??;
    if due.is_empty() {
        return Ok(0);
    }
    let http = super::discover::http()?;
    let mut found = Vec::with_capacity(due.len());
    for release in &due {
        match first_release_year(&http, release).await {
            Ok(year) => found.push((release.clone(), year)),
            // MusicBrainz out of reach: what was found is kept, the rest asked next time.
            Err(e) => {
                tracing::info!("original years: stopped after {} of {}: {e:#}", found.len(), due.len());
                break;
            }
        }
        tokio::time::sleep(PAUSE).await;
    }
    let n = found.len();
    let ud = served.userdata.clone();
    tokio::task::spawn_blocking(move || ud.set_original_years(&found, super::moment::now_secs())).await??;
    tracing::info!("original years: {n} albums asked of MusicBrainz");
    Ok(n)
}

/// A release's release group's first release year; none where MusicBrainz has no date, or no such release.
async fn first_release_year(http: &reqwest::Client, release: &str) -> Result<Option<u32>> {
    if release.len() != 36 || !release.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Ok(None);
    }
    let url = format!("https://musicbrainz.org/ws/2/release/{release}?inc=release-groups&fmt=json");
    let mut again = true;
    loop {
        let r = http.get(&url).send().await?;
        match r.status() {
            reqwest::StatusCode::SERVICE_UNAVAILABLE if std::mem::take(&mut again) => {
                tokio::time::sleep(BUSY).await;
                continue;
            }
            reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::BAD_REQUEST => return Ok(None),
            _ => {}
        }
        let v: serde_json::Value = r.error_for_status()?.json().await?;
        return Ok(year_of(&v));
    }
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
