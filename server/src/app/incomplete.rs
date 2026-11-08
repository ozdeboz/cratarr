//! Albums missing tracks (0.9.7, the owner: "golden work"; the Last.fm preview's loved tracks showed owned albums
//! lacking some, none lost by Cratarr: they arrived so). An album with a MusicBrainz release (its files', else its
//! match's, `matching`) is held against its release group's releases: of those holding every one of its tracks (by
//! title, the closest length within `SLACK_SECS`), the smallest, so a deluxe edition does not flag a standard one. What
//! that release has beyond the album's tracks is missing, by disc and number. Looked at in the harvest's rounds
//! (`harvest`), a batch at a time, each album's result kept (`UserData::album_complete`) and looked at again after
//! `AGAIN`.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A track's length within this of a release's (s).
const SLACK_SECS: i64 = 5;
/// Albums looked at a round.
const BATCH: usize = 200;
/// An album is looked at again after this long.
const AGAIN: i64 = 60 * 86_400;
/// An album whose look failed (its files not read, MusicBrainz not answering) waits this long (s) before it is tried
/// again, the others going first: one such album stopped every pass after twelve albums, from 2026-10-27 until 0.13.
const FAILED_WAIT: i64 = 86_400;

/// The albums whose look failed, and when (kept while the server runs).
static FAILED: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);

fn failed<T>(f: impl FnOnce(&mut HashMap<String, i64>) -> T) -> T {
    f(FAILED.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new))
}

/// What an album lacks of its smallest release that holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Incomplete {
    /// How many tracks the release has, and the album.
    pub of: usize,
    pub has: usize,
    /// The release, as Renumber shows one ("Delight, 2023, Digital Media").
    pub release: String,
    /// Its release group: what "Ask for the full album" asks Lidarr for.
    #[serde(default)]
    pub release_group: Option<String>,
    /// The release's MusicBrainz id (0.9.8: where single-track replacement finds the missing tracks).
    #[serde(default)]
    pub release_id: Option<String>,
    /// Each missing track: its disc and number on the release (from 1), and its title.
    pub missing: Vec<(u32, u32, String)>,
    /// The release's length and its longest track (ms), where every track's length is known (0.13: the type rule reads
    /// an incomplete album as its release).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longest_ms: Option<u64>,
}

/// The album's tracks (title, length s) against the release group's releases: the smallest holding every track, and
/// what it has beyond them; none where the album is whole or no release holds it.
pub(super) fn check(files: &[(String, i64)], releases: &[super::renumber::Release]) -> Option<Incomplete> {
    held(files, releases).and_then(|(r, used)| beyond(files, r, &used))
}

/// The smallest release holding every one of the album's tracks, and where each is on it (medium, track).
fn held<'a>(files: &[(String, i64)], releases: &'a [super::renumber::Release]) -> Option<(&'a super::renumber::Release, HashSet<(usize, usize)>)> {
    let holds = |r: &super::renumber::Release| -> Option<HashSet<(usize, usize)>> {
        let mut used = HashSet::new();
        for (title, secs) in files {
            let best = r
                .media
                .iter()
                .enumerate()
                .flat_map(|(d, m)| m.tracks.iter().enumerate().map(move |(t, x)| ((d, t), x)))
                .filter(|(at, x)| !used.contains(at) && super::renumber::same_title(title, &x.0))
                .filter(|(_, x)| x.1.is_none_or(|l| (i64::from(l) - secs).abs() <= SLACK_SECS))
                .min_by_key(|(_, x)| x.1.map_or(i64::MAX, |l| (i64::from(l) - secs).abs()))?;
            used.insert(best.0);
        }
        Some(used)
    };
    releases.iter().filter_map(|r| Some((r, holds(r)?))).min_by_key(|(r, _)| size(r))
}

fn size(r: &super::renumber::Release) -> usize {
    r.media.iter().map(|m| m.tracks.len()).sum()
}

/// What a release that holds the album has beyond its tracks; none where nothing.
fn beyond(files: &[(String, i64)], r: &super::renumber::Release, used: &HashSet<(usize, usize)>) -> Option<Incomplete> {
    let of = size(r);
    if of <= files.len() {
        return None;
    }
    let missing = r
        .media
        .iter()
        .enumerate()
        .flat_map(|(d, m)| m.tracks.iter().enumerate().map(move |(t, x)| ((d, t), x.0.clone())))
        .filter(|(at, _)| !used.contains(at))
        .map(|((d, t), title)| (d as u32 + 1, t as u32 + 1, title))
        .collect();
    let f = &r.facts;
    let release = [Some(f.title.clone()), f.year.map(|y| y.to_string()), Some(f.format.clone()).filter(|x| !x.is_empty())].into_iter().flatten().collect::<Vec<_>>().join(", ");
    let lengths: Option<Vec<u64>> = r.media.iter().flat_map(|m| m.tracks.iter().map(|t| t.1.map(|s| u64::from(s) * 1000))).collect();
    let (length_ms, longest_ms) = match lengths {
        Some(l) if !l.is_empty() => (Some(l.iter().sum()), l.iter().max().copied()),
        _ => (None, None),
    };
    Some(Incomplete { of, has: files.len(), release, release_group: None, release_id: Some(r.id.clone()).filter(|i| !i.is_empty()), missing, length_ms, longest_ms })
}

/// A round's batch: the albums with a release, of every library in use (the owner, 2026-11-02: Incoming's too), never
/// looked at first, then those looked at longest ago.
pub async fn pass(served: &std::sync::Arc<crate::serve::Serve>, http: &reqwest::Client) -> Result<()> {
    let (s, now) = (served.clone(), super::moment::now_secs());
    let due: Vec<(String, String)> = tokio::task::spawn_blocking(move || -> Result<Vec<(String, String)>> {
        // Those looked at by an older rule count as never looked at (0.13: their discs are kept now).
        let checked = s.userdata.album_complete_now()?;
        let mut albums: Vec<(i64, String, String)> = Vec::new();
        for l in s.library.lacking()? {
            let at = checked.get(&l.id).copied();
            if at.is_some_and(|at| now - at < AGAIN) || failed(|f| f.get(&l.id).is_some_and(|t| now - t < FAILED_WAIT)) {
                continue;
            }
            let own = s.library.album_names(&l.id)?.and_then(|p| p.0).filter(|m| super::discogs::is_mbid(m));
            let release = match own {
                Some(r) => Some(r),
                None => s.userdata.album_matches(std::slice::from_ref(&l.id))?.remove(&l.id).and_then(|(_, j)| j).and_then(|j| serde_json::from_str::<super::matching::Match>(&j).ok()).map(|m| m.release),
            };
            if let Some(r) = release {
                albums.push((at.unwrap_or(i64::MIN), l.id, r));
            }
        }
        albums.sort_by_key(|a| a.0);
        Ok(albums.into_iter().take(BATCH).map(|(_, a, r)| (a, r)).collect())
    })
    .await??;
    for (album, release) in due {
        // One album failing is set aside (`FAILED_WAIT`), never the end of the pass.
        if let Err(e) = look(served, http, &album, &release).await {
            tracing::info!("incomplete: {album} not looked at, tried again in a day: {e:#}");
            failed(|f| f.insert(album, super::moment::now_secs()));
        }
    }
    Ok(())
}

/// One album held against its release group's releases, what it misses and the discs its tracks are on kept.
async fn look(served: &std::sync::Arc<crate::serve::Serve>, http: &reqwest::Client, album: &str, release: &str) -> Result<()> {
    let (releases, rg) = match super::discogs::release_group_of(http, release).await? {
        Some(rg) => (super::renumber::group_releases(http, &rg).await?, rg),
        None => (Vec::new(), String::new()),
    };
    let a = album.to_string();
    let tracks = tokio::task::spawn_blocking(move || super::renumber::tracks_of(&a)).await??.0;
    let files: Vec<(String, i64)> = tracks.iter().map(|t| (t.title.clone(), i64::from(t.secs))).collect();
    let found = check(&files, &releases).map(|i| serde_json::to_string(&Incomplete { release_group: Some(rg.clone()).filter(|g| !g.is_empty()), ..i })).transpose()?;
    // The discs Renumber would put its tracks on (0.13, Health's Disc numbers).
    let on = super::renumber::discs_from(&tracks, &releases);
    let (ud, album) = (served.userdata.clone(), album.to_string());
    tokio::task::spawn_blocking(move || ud.set_album_complete(&album, found.as_deref(), on, super::moment::now_secs())).await??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::renumber::{Medium, Release, ReleaseFacts};
    use super::*;

    fn release(format: &str, tracks: &[(&str, u32)]) -> Release {
        Release {
            id: String::new(),
            facts: ReleaseFacts { title: "Delight".into(), format: format.into(), country: None, year: Some(2023) },
            media: vec![Medium { format: format.into(), tracks: tracks.iter().map(|(t, s)| (t.to_string(), Some(*s))).collect() }],
        }
    }

    #[test]
    fn the_smallest_release_holding_every_track_says_what_is_missing() {
        let files = vec![("Euphoria".to_string(), 300), ("Delight".to_string(), 280)];
        let standard = release("Digital Media", &[("Euphoria", 301), ("Delight", 279), ("Bloom", 250)]);
        let deluxe = release("Digital Media", &[("Euphoria", 301), ("Delight", 279), ("Bloom", 250), ("Bonus", 200), ("Remix", 310)]);
        let got = check(&files, &[deluxe, standard.clone()]).unwrap();
        assert_eq!((got.of, got.has), (3, 2), "the standard edition, not the deluxe");
        assert_eq!(got.missing, [(1, 3, "Bloom".to_string())]);
        let whole = vec![("Euphoria".to_string(), 300), ("Delight".to_string(), 280), ("Bloom".to_string(), 251)];
        assert!(check(&whole, &[standard.clone()]).is_none(), "whole");
        let other = vec![("Euphoria".to_string(), 300), ("Elsewhere".to_string(), 200)];
        assert!(check(&other, &[standard]).is_none(), "no release holds a track it does not have");
    }
}
