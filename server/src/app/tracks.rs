//! The Library page's Tracks (0.9.4): Cratarr's own library's tracks as one list, paged, in the order asked for,
//! filtered by the words as search reads them (title, artist, album), from the libraries switched on; each with the
//! asking account's plays and star.

use std::cmp::Ordering;
use std::collections::HashSet;

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

/// One page of the list (`offset`, at most `limit` up to `MOST`) and how many there are in all.
#[allow(clippy::too_many_arguments)]
pub async fn list(app: &super::App, q: &str, order: &str, down: bool, without: &[String], offset: usize, limit: usize) -> Result<(Vec<TrackRow>, usize)> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let off = app.library.folders_off(without);
    let (words, order, user) = (crate::search::words(q), order.to_string(), served.current_user());
    tokio::task::spawn_blocking(move || -> Result<(Vec<TrackRow>, usize)> {
        let mut rows = served.library.track_rows(&words, &off)?;
        if let Some(user) = user {
            of_account(&served, &user, &mut rows)?;
        }
        sort(&mut rows, &order, down);
        let total = rows.len();
        Ok((rows.into_iter().skip(offset).take(limit.clamp(1, MOST)).collect(), total))
    })
    .await?
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
