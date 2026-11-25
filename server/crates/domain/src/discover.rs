//! Discover's list (decision 0069's amendment of 2026-11-25, the owner): no one artist's missing albums take the list's places.

/// How many of one artist's missing albums Discover keeps (the owner, 2026-11-25: "5–10"; Lee 'Scratch' Perry alone had some
/// 95 cards in Gaps of the 500 a look keeps).
pub const GAPS_PER_ARTIST: usize = 8;

/// A suggestion as these rules read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Card<'a> {
    /// The artist's key, as the list compares artists (loosely).
    pub artist: &'a str,
    /// Its only reason is a gap in the library's albums of that artist (no other source named it).
    pub gap_only: bool,
    /// When it came out, as closely as known ("YYYY-MM-DD", "YYYY-MM" or "YYYY").
    pub released: Option<&'a str>,
}

/// Which of `cards` stay: every card another source named, and of each artist's gap-only cards the `cap` newest (the
/// list's order among cards out the same day, or with no date: an undated one comes last). The answer is in `cards`'
/// order.
pub fn gaps_kept(cards: &[Card], cap: usize) -> Vec<bool> {
    let mut keep = vec![true; cards.len()];
    let mut by_artist: Vec<(&str, Vec<usize>)> = Vec::new();
    for (i, c) in cards.iter().enumerate().filter(|(_, c)| c.gap_only) {
        match by_artist.iter_mut().find(|(a, _)| *a == c.artist) {
            Some((_, at)) => at.push(i),
            None => by_artist.push((c.artist, vec![i])),
        }
    }
    for (_, mut at) in by_artist {
        // Newest first; the sort is stable, so the list's own order breaks ties.
        at.sort_by(|&x, &y| cards[y].released.unwrap_or("").cmp(cards[x].released.unwrap_or("")));
        for &i in at.iter().skip(cap) {
            keep[i] = false;
        }
    }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gap<'a>(artist: &'a str, released: Option<&'a str>) -> Card<'a> {
        Card { artist, gap_only: true, released }
    }

    #[test]
    fn one_artists_gaps_keep_only_the_newest_few() {
        let cards = [
            gap("perry", Some("1976")),
            gap("perry", Some("2019-05-03")),
            gap("perry", Some("1980-01")),
            gap("perry", None),
            gap("perry", Some("2008")),
        ];
        assert_eq!(gaps_kept(&cards, 3), [false, true, true, false, true]);
    }

    #[test]
    fn a_card_another_source_named_is_never_capped() {
        let other = Card { artist: "perry", gap_only: false, released: Some("1970") };
        let cards = [gap("perry", Some("2020")), other, gap("perry", Some("2021")), other];
        assert_eq!(gaps_kept(&cards, 1), [false, true, true, true]);
    }

    #[test]
    fn each_artist_counts_alone_and_ties_keep_the_lists_order() {
        let cards = [gap("air", Some("2001")), gap("perry", Some("1999")), gap("air", Some("2001")), gap("air", Some("2001"))];
        assert_eq!(gaps_kept(&cards, 2), [true, true, true, false]);
    }

    #[test]
    fn under_the_cap_nothing_goes() {
        let cards = [gap("air", None), gap("air", Some("1998"))];
        assert_eq!(gaps_kept(&cards, GAPS_PER_ARTIST), [true, true]);
        assert!(gaps_kept(&[], GAPS_PER_ARTIST).is_empty());
    }
}
