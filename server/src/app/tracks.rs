//! The Library page's Tracks (0.9.4): Cratarr's own library's tracks as one list, paged, in the order asked for,
//! filtered by the words as search reads them (title, artist, album), from the libraries switched on; each with the
//! asking account's plays and star. The Library's filter applies to them as to its albums (0.9.15-1, the owner: "Make
//! it whole and complete"): its switches and its conditions (decision 0061), before the rows are paged.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};

use crate::serve::read::TrackRow;

/// Most rows one page holds.
pub const MOST: usize = 500;

/// Two texts in order, case aside.
fn ci(a: &str, b: &str) -> Ordering {
    a.chars().flat_map(char::to_lowercase).cmp(b.chars().flat_map(char::to_lowercase))
}

/// The rows in `order` (`title`, `artist`, `album`, `added`, `year`, `plays`, `duration`, and 0.9.7's `tempo`,
/// `lastPlayed`, `albumArtist`; any other: `album`), the other
/// way round where `down`; equals in their album's order (album, disc, track), always the same way.
pub fn sort(rows: &mut [TrackRow], order: &str, down: bool) {
    rows.sort_by(|a, b| {
        let first = match order {
            "title" => ci(&a.order_title, &b.order_title),
            "artist" => ci(&a.artist, &b.artist),
            "added" => a.added.cmp(&b.added),
            "year" => a.year.cmp(&b.year),
            "plays" => a.plays.cmp(&b.plays),
            "duration" => a.duration.cmp(&b.duration),
            "tempo" => a.bpm.cmp(&b.bpm),
            "lastPlayed" => a.last_played.cmp(&b.last_played),
            "albumArtist" => ci(a.album_artist.as_deref().unwrap_or(&a.artist), b.album_artist.as_deref().unwrap_or(&b.artist)),
            _ => ci(&a.album, &b.album),
        };
        (if down { first.reverse() } else { first })
            .then_with(|| ci(&a.album, &b.album))
            .then_with(|| a.album_id.cmp(&b.album_id))
            .then_with(|| a.disc_no.cmp(&b.disc_no))
            .then_with(|| a.track_no.cmp(&b.track_no))
            .then_with(|| ci(&a.title, &b.title))
    });
}

/// The Library's filter on its tracks: its switches (Favourites only, Not played yet, 4 stars and up) and its
/// conditions, a crate's (decision 0061), each track an item of one.
#[derive(Debug, Default, Clone)]
pub struct Narrow {
    pub favourites: bool,
    pub unplayed: bool,
    pub top_rated: bool,
    pub conditions: Vec<crate::crates::Condition>,
}

/// 4 stars and up.
const TOP_RATED: u8 = 4;

/// A page of the list.
pub struct Listed {
    pub rows: Vec<TrackRow>,
    /// How many pass the filter; how many there are without it (the words and the libraries switched off still apply).
    pub total: usize,
    pub all: usize,
}

/// One page of the list (`offset`, at most `limit` up to `MOST`), how many pass the filter and how many there are.
#[allow(clippy::too_many_arguments)]
pub async fn list(app: &super::App, q: &str, order: &str, down: bool, without: &[String], narrow: Narrow, offset: usize, limit: usize) -> Result<Listed, super::ToggleError> {
    let refused = |e: &dyn std::fmt::Display| super::ToggleError::Refused(format!("{e:#}"));
    let served = app.served.clone().context("Cratarr's own library is not open").map_err(|e| refused(&e))?;
    if let Some(c) = narrow.conditions.iter().find(|c| !crate::crates::is_axis(&c.axis)) {
        return Err(super::ToggleError::Refused(format!("Unknown axis: {}.", c.axis)));
    }
    // The tracks the conditions let through, by their catalogue track, worked out as a filter's are (app::filter).
    let passing: Option<HashSet<i64>> = if narrow.conditions.iter().any(|c| c.enabled) {
        let lib = app.playlists.crate_library_for(&narrow.conditions, Some(crate::source::cratarr::KIND.id), without).await?;
        let (resolved, w) = (lib.resolved(&narrow.conditions), lib.world());
        Some(lib.files().filter(|f| crate::crates::passes(&[f], &resolved, &w)).filter_map(|f| f.track).collect())
    } else {
        None
    };
    let off = app.library.folders_off(without);
    let (words, order, user) = (crate::search::words(q), order.to_string(), served.current_user());
    tokio::task::spawn_blocking(move || -> Result<Listed> {
        let mut rows = served.library.track_rows(&words, &off)?;
        let all = rows.len();
        let ratings = match &user {
            Some(user) => {
                of_account(&served, user, &mut rows)?;
                narrow.top_rated.then(|| ratings_of(&served, user)).transpose()?
            }
            None => None,
        };
        rows.retain(|r| keeps(r, &narrow, ratings.as_ref(), passing.as_ref()));
        sort(&mut rows, &order, down);
        let total = rows.len();
        Ok(Listed { rows: rows.into_iter().skip(offset).take(limit.clamp(1, MOST)).collect(), total, all })
    })
    .await
    .map_err(|e| refused(&e))?
    .map_err(|e| refused(&e))
}

/// The account's stars: each track's own, and each album's (a track without its own has its album's).
pub struct Ratings {
    tracks: HashMap<String, u8>,
    albums: HashMap<String, u8>,
}

impl Ratings {
    fn of(&self, r: &TrackRow) -> u8 {
        match self.tracks.get(&r.id) {
            Some(&own) if own > 0 => own,
            _ => self.albums.get(&r.album_id).copied().unwrap_or(0),
        }
    }
}

/// The account's stars on tracks and albums. Blocking.
fn ratings_of(served: &crate::serve::Serve, user: &str) -> Result<Ratings> {
    let stars = |kind| -> Result<HashMap<String, u8>> { Ok(served.userdata.used(user, kind)?.into_iter().map(|(id, a)| (id, a.rating)).collect()) };
    Ok(Ratings { tracks: stars(crate::userdata::Kind::Song)?, albums: stars(crate::userdata::Kind::Album)? })
}

/// Whether a row (its account's plays and star on it) passes the filter; with no one's account, nothing of it is known
/// (no favourites, never played, no stars).
fn keeps(r: &TrackRow, narrow: &Narrow, ratings: Option<&Ratings>, passing: Option<&HashSet<i64>>) -> bool {
    (!narrow.favourites || r.favorite)
        && (!narrow.unplayed || (r.plays == 0 && r.last_played.is_none()))
        && (!narrow.top_rated || ratings.is_some_and(|s| s.of(r) >= TOP_RATED))
        && passing.is_none_or(|p| crate::serve::ids::track_of(&r.id).is_some_and(|t| p.contains(&t)))
}

/// The account's plays, when it last played and its favourites, on the rows. Blocking.
pub fn of_account(served: &crate::serve::Serve, user: &str, rows: &mut [TrackRow]) -> Result<()> {
    let used = served.userdata.used(user, crate::userdata::Kind::Song)?;
    let starred: HashSet<String> = served.userdata.starred(user, crate::userdata::Kind::Song)?.into_iter().collect();
    for r in rows {
        let u = used.get(&r.id);
        r.plays = u.map_or(0, |a| a.play_count);
        r.last_played = u.and_then(|a| a.play_date);
        r.favorite = starred.contains(&r.id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(title: &str, artist: &str, album: &str, track: i64, plays: u32) -> TrackRow {
        TrackRow {
            id: format!("tr-{title}"),
            title: title.into(),
            order_title: title.trim_start_matches("The ").into(),
            artist: artist.into(),
            album: album.into(),
            album_id: format!("al-{album}"),
            track_no: Some(track),
            plays,
            ..TrackRow::default()
        }
    }

    #[test]
    fn the_filters_switches_keep_favourites_the_unplayed_and_four_stars_and_up_a_tracks_own_else_its_albums() {
        let mut a = row("A", "x", "One", 1, 0);
        a.favorite = true;
        let mut b = row("B", "x", "One", 2, 3);
        b.last_played = Some(1);
        let c = row("C", "x", "Two", 1, 0);
        let mut played_once = row("D", "x", "Two", 2, 0);
        played_once.last_played = Some(5);
        let ratings = Ratings {
            tracks: [("tr-A".to_string(), 2), ("tr-B".to_string(), 5)].into(),
            albums: [("al-One".to_string(), 4), ("al-Two".to_string(), 4)].into(),
        };
        let kept = |n: &Narrow| [&a, &b, &c, &played_once].into_iter().filter(|r| keeps(r, n, Some(&ratings), None)).map(|r| r.title.as_str()).collect::<Vec<_>>();
        assert_eq!(kept(&Narrow::default()), ["A", "B", "C", "D"]);
        assert_eq!(kept(&Narrow { favourites: true, ..Narrow::default() }), ["A"]);
        assert_eq!(kept(&Narrow { unplayed: true, ..Narrow::default() }), ["A", "C"], "played: by count or by date");
        assert_eq!(kept(&Narrow { top_rated: true, ..Narrow::default() }), ["B", "C", "D"], "A's own 2 stars over its album's 4");
        assert_eq!(kept(&Narrow { favourites: true, top_rated: true, ..Narrow::default() }), Vec::<&str>::new());
        assert!(!keeps(&c, &Narrow { top_rated: true, ..Narrow::default() }, None, None), "no one's stars: none");
    }

    #[test]
    fn the_filters_conditions_keep_the_tracks_they_let_through() {
        let mut a = row("A", "x", "One", 1, 0);
        a.id = "tr-7".into();
        let mut b = row("B", "x", "One", 2, 0);
        b.id = "tr-8".into();
        let through: HashSet<i64> = [8].into();
        let n = Narrow::default();
        assert!(!keeps(&a, &n, None, Some(&through)));
        assert!(keeps(&b, &n, None, Some(&through)));
    }

    fn titles(rows: &[TrackRow]) -> Vec<&str> {
        rows.iter().map(|r| r.title.as_str()).collect()
    }

    #[test]
    fn tracks_go_in_the_order_asked_for_their_album_s_order_among_equals() {
        let mut rows = vec![
            row("Hunter", "Björk", "Homogenic", 1, 0),
            row("The Bachelorette", "Björk", "Homogenic", 9, 4),
            row("Jóga", "Björk", "Homogenic", 2, 4),
            row("Little Fluffy Clouds", "the Orb", "Adventures", 1, 1),
        ];
        sort(&mut rows, "album", false);
        assert_eq!(titles(&rows), ["Little Fluffy Clouds", "Hunter", "Jóga", "The Bachelorette"]);
        sort(&mut rows, "title", false);
        assert_eq!(titles(&rows), ["The Bachelorette", "Hunter", "Jóga", "Little Fluffy Clouds"], "articles aside");
        sort(&mut rows, "artist", true);
        assert_eq!(titles(&rows), ["Little Fluffy Clouds", "Hunter", "Jóga", "The Bachelorette"], "case aside; down, the albums still in order");
        sort(&mut rows, "plays", true);
        assert_eq!(titles(&rows), ["Jóga", "The Bachelorette", "Little Fluffy Clouds", "Hunter"]);
        for r in rows.iter_mut() {
            r.bpm = match r.title.as_str() {
                "Hunter" => Some(120),
                "Jóga" => Some(90),
                _ => None,
            };
        }
        sort(&mut rows, "tempo", true);
        assert_eq!(titles(&rows)[..2], ["Hunter", "Jóga"], "the fastest first, those without a tempo last");
        sort(&mut rows, "nonsense", false);
        assert_eq!(titles(&rows)[0], "Little Fluffy Clouds", "the album order");
    }
}
