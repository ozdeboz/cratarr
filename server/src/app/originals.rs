//! Albums' original years from MusicBrainz (0.9.5, the owner: Ott's Skylon showed 2012, its reissue's year, not 2008).
//! Every album whose files carry no original date but a MusicBrainz release gets its release group's first release
//! year, asked in the background one album at a time and kept (user data `original_years`); the library is built with
//! it (`Library::set_originals`), no file changed. The same question gives the release group's types (0.9.6, the owner:
//! hide EPs and singles), asked of every album with a release and kept in `release_types` (`mb:<release>`); an EP's or
//! single's edition far longer than its group's first release is kept an album (0.10, `edition_kind`).

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
    // With each album's size by its release: an EP's or single's edition is compared with its group's first release.
    type Due = (Vec<(String, bool)>, std::collections::HashMap<String, (usize, u64)>);
    let (due, sizes): Due = tokio::task::spawn_blocking(move || -> Result<Due> {
        let asked = ud.original_years_asked(now - AGAIN_SECS)?;
        let typed = ud.release_types_asked("mb:", now - AGAIN_SECS)?;
        let years: std::collections::HashSet<String> = lib.releases_without_original()?.into_iter().filter(|r| !asked.contains(r)).collect();
        let mut due: Vec<(String, bool)> = years.iter().map(|r| (r.clone(), true)).collect();
        due.extend(lib.album_releases()?.into_iter().filter(|r| !typed.contains(r) && !years.contains(r)).map(|r| (r, false)));
        due.truncate(BATCH);
        Ok((due, lib.release_sizes()?))
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
                let (year, mut primary, secondary, id) = group.unwrap_or_default();
                if matches!(primary.as_deref(), Some("ep" | "single"))
                    && let (Some(id), Some(album)) = (id, sizes.get(release))
                {
                    tokio::time::sleep(PAUSE).await;
                    match group_first(&http, &id).await {
                        Ok(first) => primary = edition_kind(primary, *album, first),
                        Err(e) => {
                            tracing::info!("release types: stopped after {} of {}: {e:#}", types.len(), due.len());
                            break;
                        }
                    }
                }
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

/// What a release's release group says: (its first release year, its primary type, its secondary types, its id); none
/// where there is no such release.
type Group = (Option<u32>, Option<String>, Vec<String>, Option<String>);

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
    Ok(Some((year_of(&v), primary, secondary, g["id"].as_str().map(str::to_string))))
}

/// A release's size as compared: its tracks, and their length (ms) where every one has one.
type Size = (usize, Option<u64>);

/// A release group's first release, asked of MusicBrainz (`first_of`).
async fn group_first(http: &reqwest::Client, group: &str) -> Result<Option<Size>> {
    let url = format!("https://musicbrainz.org/ws/2/release?release-group={group}&inc=media+recordings&limit=100&fmt=json");
    Ok(ask(http, &url).await?.as_ref().and_then(first_of))
}

/// The first of a release group's releases as MusicBrainz lists them: an official one where there is one, the earliest
/// dated (the undated last); its size.
fn first_of(releases: &serde_json::Value) -> Option<Size> {
    let first = releases["releases"].as_array()?.iter().min_by_key(|r| {
        let date = r["date"].as_str().unwrap_or_default().to_string();
        (r["status"].as_str() != Some("Official"), date.is_empty(), date)
    })?;
    let media = first["media"].as_array()?;
    let tracks: usize = media.iter().map(|m| m["track-count"].as_u64().map_or_else(|| m["tracks"].as_array().map_or(0, Vec::len), |n| n as usize)).sum();
    let lengths: Vec<Option<u64>> = media.iter().flat_map(|m| m["tracks"].as_array().into_iter().flatten()).map(|t| t["length"].as_u64()).collect();
    let length = (lengths.len() == tracks).then(|| lengths.into_iter().sum::<Option<u64>>()).flatten();
    (tracks > 0).then_some((tracks, length))
}

/// The group's primary type as this edition's (0.10, the owner: an expanded or deluxe edition of an EP stayed an EP): an
/// EP or single more than twice the tracks, or more than twice the length, of its group's first release is an album.
/// A long EP as long as its group's own release stays an EP.
fn edition_kind(primary: Option<String>, album: (usize, u64), first: Option<Size>) -> Option<String> {
    let far_longer = |(tracks, length): Size| album.0 > 2 * tracks || length.is_some_and(|ms| ms > 0 && album.1 > 2 * ms);
    if matches!(primary.as_deref(), Some("ep" | "single")) && first.is_some_and(far_longer) {
        Some("album".into())
    } else {
        primary
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

    /// A release as MusicBrainz lists a group's: its status, date and tracks of these lengths (s).
    fn release(status: &str, date: &str, secs: &[u64]) -> serde_json::Value {
        let tracks: Vec<serde_json::Value> = secs.iter().map(|s| serde_json::json!({ "length": s * 1000 })).collect();
        serde_json::json!({ "status": status, "date": date, "media": [{ "track-count": secs.len(), "tracks": tracks }] })
    }

    #[test]
    fn a_group_s_first_release_is_its_earliest_official_one() {
        let group = serde_json::json!({ "releases": [
            release("Official", "2012-06-04", &[300; 14]),
            release("Promotion", "1991", &[400]),
            release("Official", "", &[300; 2]),
            release("Official", "1992-03", &[360; 4]),
        ] });
        assert_eq!(first_of(&group), Some((4, Some(4 * 360 * 1000))));
        let unknown = serde_json::json!({ "releases": [{ "status": "Official", "date": "1992", "media": [{ "track-count": 3, "tracks": [{ "length": 1000 }, {}, {}] }] }] });
        assert_eq!(first_of(&unknown), Some((3, None)), "a track without a length: the length not known");
        assert_eq!(first_of(&serde_json::json!({ "releases": [] })), None);
    }

    #[test]
    fn an_edition_far_longer_than_its_group_s_first_release_is_no_ep_or_single() {
        let ep = || Some("ep".to_string());
        let min = |m: u64| m * 60_000;
        // Digeridoo, expanded edition: 68.6 minutes to the EP's four tracks of 24.
        assert_eq!(edition_kind(ep(), (14, 4_116_000), Some((4, Some(min(24))))).as_deref(), Some("album"));
        // A deluxe edition twice and a bit its group's release, by length alone (as many tracks).
        assert_eq!(edition_kind(Some("single".into()), (6, min(63)), Some((6, Some(min(30))))).as_deref(), Some("album"));
        // Or by its tracks alone, where MusicBrainz gives no lengths.
        assert_eq!(edition_kind(ep(), (13, min(63)), Some((5, None))).as_deref(), Some("album"));
        // Die Walküre's 220 minutes are no EP of four tracks.
        assert_eq!(edition_kind(ep(), (40, min(220)), Some((4, Some(min(25))))).as_deref(), Some("album"));
        // A 40-minute dance EP as long as its group's own release stays an EP; so does one no release to compare with.
        assert_eq!(edition_kind(ep(), (6, min(40)), Some((6, Some(min(40))))).as_deref(), Some("ep"));
        assert_eq!(edition_kind(ep(), (6, min(40)), None).as_deref(), Some("ep"));
        // Twice its release, not more: still the EP. An album is never touched.
        assert_eq!(edition_kind(ep(), (8, min(48)), Some((4, Some(min(24))))).as_deref(), Some("ep"));
        assert_eq!(edition_kind(Some("album".into()), (40, min(220)), Some((4, Some(min(25))))).as_deref(), Some("album"));
    }
}
