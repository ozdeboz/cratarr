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

pub use super::incomplete_base::{Incomplete, still_short};

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
    let due: Vec<(String, String)> = crate::run::spawn_blocking(move || due_albums(&s, now)).await??;
    for (album, release) in due {
        // One album failing is set aside (`FAILED_WAIT`), never the end of the pass.
        if let Err(e) = look(served, http, &album, &release).await {
            tracing::info!("incomplete: {album} not looked at, tried again in a day: {e:#}");
            failed(|f| f.insert(album, super::moment::now_secs()));
        }
    }
    Ok(())
}

/// A round's batch at `now` (`pass`): never looked at first, then those looked at longest ago; one whose row no longer fits it (A3.4:
/// its tracks changed since its look) at once, as one never looked at, before its 60 days. Blocking.
fn due_albums(s: &crate::serve::Serve, now: i64) -> Result<Vec<(String, String)>> {
    // Those looked at by an older rule count as never looked at (0.13: their discs are kept now).
    let checked = s.userdata.album_complete_now()?;
    let counts = s.library.song_counts()?;
    let stale: HashSet<String> = s
        .userdata
        .albums_incomplete()?
        .into_iter()
        .filter(|(album, json)| serde_json::from_str::<Incomplete>(json).is_ok_and(|i| !i.fits(counts.get(album).copied().unwrap_or(0))))
        .map(|(album, _)| album)
        .collect();
    let mut albums: Vec<(i64, String, String)> = Vec::new();
    for l in s.library.lacking()? {
        let at = checked.get(&l.id).copied().filter(|_| !stale.contains(&l.id));
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
}

/// One album held against its release group's releases, what it misses and the discs its tracks are on kept.
async fn look(served: &std::sync::Arc<crate::serve::Serve>, http: &reqwest::Client, album: &str, release: &str) -> Result<()> {
    let (releases, rg) = match super::discogs::release_group_of(http, release).await? {
        Some(rg) => (super::renumber::group_releases(http, &rg).await?, rg),
        None => (Vec::new(), String::new()),
    };
    let a = album.to_string();
    let tracks = crate::run::spawn_blocking(move || super::renumber::tracks_of(&a)).await??.0;
    let files: Vec<(String, i64)> = tracks.iter().map(|t| (t.title.clone(), i64::from(t.secs))).collect();
    let found = check(&files, &releases).map(|i| serde_json::to_string(&Incomplete { release_group: Some(rg.clone()).filter(|g| !g.is_empty()), ..i })).transpose()?;
    // The discs Renumber would put its tracks on (0.13, Health's Disc numbers).
    let on = super::renumber::discs_from(&tracks, &releases);
    let (ud, album) = (served.userdata.clone(), album.to_string());
    crate::run::spawn_blocking(move || ud.set_album_complete(&album, found.as_deref(), on, super::moment::now_secs())).await??;
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

    /// A3.4.1, A3.4.2: a row "of 12, has 10" beside its album now holding 12 tracks (a Replace landed since its look) is read by nothing
    /// (`still_short`: Health's records missing tracks, a record's own health rows and their "Ask for the full album", the missing tracks'
    /// release), and the pass takes that album at once, before its 60 days; an album whose row still fits (10 tracks) is read as today
    /// and waits its 60 days. Never "2 tracks missing" beside a whole album.
    #[test]
    fn a_missing_tracks_row_that_no_longer_fits_is_not_read() {
        use crate::serve::copies::Kind;
        let root = std::env::temp_dir().join(format!("cratarr-incomplete-fits-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();
        let cat = std::sync::Arc::new(crate::catalogue::Catalogue::open(None, vec![root.clone()]).unwrap());
        let serve = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.library.set_kinds(vec![(0, Kind::Main)]);
        serve.attach_catalogue(cat.clone());
        let albums = [("Whole Now", 12, "00000000-0000-4000-8000-0000000034a1"), ("Still Short", 10, "00000000-0000-4000-8000-0000000034a2")];
        let mut files = Vec::new();
        for (dir, n, release) in albums {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            for t in 1..=n {
                let rel = format!("{dir}/{t:02} Track {t}.flac");
                std::fs::write(root.join(&rel), &rel).unwrap();
                files.push((rel, dir, t, release));
            }
        }
        cat.walk_all();
        cat.pretend_read_for_tests();
        for (rel, dir, t, release) in &files {
            let (title, number) = (format!("Track {t}"), t.to_string());
            cat.tag_for_tests(0, rel, &[("album", *dir), ("albumartist", "Kite Theory"), ("title", title.as_str()), ("tracknumber", number.as_str()), ("musicbrainz_albumid", *release)]);
        }
        cat.identify().unwrap();
        serve.refresh().unwrap();
        let in_library = |dir: &str| serve.library.album_in(0, dir).unwrap().expect("built");
        let (whole, short) = (in_library("Whole Now"), in_library("Still Short"));
        let row = Incomplete {
            of: 12,
            has: 10,
            release: "Night Walks, 2020, CD".into(),
            release_group: None,
            release_id: Some("00000000-0000-4000-8000-0000000034b1".into()),
            missing: vec![(1, 11, "Track 11".into()), (1, 12, "Track 12".into())],
            length_ms: None,
            longest_ms: None,
        };
        let t = super::super::moment::now_secs();
        for album in [&whole, &short] {
            serve.userdata.set_album_complete(album, Some(&serde_json::to_string(&row).unwrap()), Some(1), t).unwrap();
        }
        let read: Vec<String> = still_short(&serve).unwrap().into_iter().map(|(a, _)| a).collect();
        assert_eq!(read, [short.clone()], "the row that no longer fits is read by nothing; the one that fits as today");
        let due: Vec<String> = due_albums(&serve, t + 60).unwrap().into_iter().map(|(a, _)| a).collect();
        assert_eq!(due, [whole.clone()], "the album whose row no longer fits taken at once; the other waits its 60 days");
        for (file, src) in [("health.rs", include_str!("health.rs")), ("track_fetch.rs", include_str!("track_fetch.rs"))] {
            assert!(src.contains("super::incomplete_base::still_short(") && !src.contains("albums_incomplete()"), "{file} reads the rows that fit alone");
        }
        std::fs::remove_dir_all(&root).ok();
    }
}
