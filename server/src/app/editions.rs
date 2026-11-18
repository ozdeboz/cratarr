//! F12.1 (the owner: "One card, 2 editions"): editions of one album shown as one card, marked with how many there are, on an
//! artist's page, search's Records and a label's page; an album's page lists them (`of_record`), each opening its record. Which
//! records are editions is the catalogue's (`Catalogue::edition_groups`). Display only: the records stay two (Q8).

use std::collections::HashMap;

use serde::Serialize;

use super::Library;
use crate::serve::ids;
use crate::source::{Album, AlbumCopy};

/// One edition of an album as its page lists it (F12.1): its record (`al-<n>`), which it opens, how many tracks it has, and its
/// copies, as a card's (`library` its library's path, and its format).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Edition {
    pub album: String,
    pub tracks: u32,
    pub copies: Vec<AlbumCopy>,
}

/// `cards` with the editions of one album as one card (F12.1): of each of `groups` (the editions of one album, the one shown first)
/// the card earliest in its order stays where it stands, marked (`mark`) with how many editions the group holds; the group's other
/// cards go. A card of no group stays as it is. How many went. Pure.
pub fn fold<T>(cards: &mut Vec<T>, record_of: impl Fn(&T) -> Option<i64>, groups: &[Vec<i64>], mut mark: impl FnMut(&mut T, u32)) -> usize {
    let place: HashMap<i64, (usize, usize)> = groups.iter().enumerate().flat_map(|(g, group)| group.iter().enumerate().map(move |(rank, r)| (*r, (g, rank)))).collect();
    // Each group's card that stays: (its rank in the group, its place among the cards).
    let mut stays: HashMap<usize, (usize, usize)> = HashMap::new();
    for (i, c) in cards.iter().enumerate() {
        if let Some(&(g, rank)) = record_of(c).and_then(|r| place.get(&r)) {
            let s = stays.entry(g).or_insert((rank, i));
            if rank < s.0 {
                *s = (rank, i);
            }
        }
    }
    let before = cards.len();
    let mut kept = Vec::with_capacity(before);
    for (i, mut c) in std::mem::take(cards).into_iter().enumerate() {
        match record_of(&c).and_then(|r| place.get(&r)).copied() {
            Some((g, _)) if stays.get(&g).is_some_and(|s| s.1 != i) => continue,
            Some((g, _)) => {
                mark(&mut c, u32::try_from(groups[g].len()).unwrap_or(u32::MAX));
                kept.push(c);
            }
            None => kept.push(c),
        }
    }
    *cards = kept;
    before - cards.len()
}

/// Which of `records` are editions of one album (`Catalogue::edition_groups`, a copy in a main library shown first). None where they
/// cannot be read: then every card stands alone.
async fn groups_of(lib: &Library, records: Vec<i64>) -> Vec<Vec<i64>> {
    if records.len() < 2 {
        return Vec::new();
    }
    let (cat, mains) = (lib.catalogue(), lib.main_folders());
    match tokio::task::spawn_blocking(move || cat.edition_groups(&records, &mains)).await {
        Ok(Ok(groups)) => groups,
        Ok(Err(e)) => {
            tracing::debug!("editions not read: {e:#}");
            Vec::new()
        }
        Err(e) => {
            tracing::debug!("editions not read: {e}");
            Vec::new()
        }
    }
}

/// An artist's page's cards of Cratarr's records (F12.1): the editions of one album, one card, marked with how many.
pub async fn fold_albums(lib: &Library, albums: &mut Vec<Album>) {
    let groups = groups_of(lib, albums.iter().filter_map(|a| ids::record_number(&a.id)).collect()).await;
    fold(albums, |a| ids::record_number(&a.id), &groups, |a, n| a.editions = Some(n));
}

/// Search's Records (F12.1): Cratarr's records among `items` (`album:cratarr/al-<n>`) that are editions of one album, one item,
/// marked with how many. How many items went.
pub async fn fold_items(lib: &Library, items: &mut Vec<super::search::Item>) -> usize {
    let record_of = |i: &super::search::Item| i.subject.strip_prefix("album:cratarr/").and_then(ids::record_number);
    let groups = groups_of(lib, items.iter().filter_map(record_of).collect()).await;
    fold(items, record_of, &groups, |i, n| i.editions = Some(n))
}

/// A label's page's rows of records (F12.1): the editions of one album, one row, marked with how many.
pub async fn fold_rows(lib: &Library, rows: &mut Vec<super::labels::LabelEntry>) {
    let record_of = |e: &super::labels::LabelEntry| e.record.as_deref().and_then(ids::record_number);
    let groups = groups_of(lib, rows.iter().filter_map(record_of).collect()).await;
    fold(rows, record_of, &groups, |e, n| e.editions = Some(n));
}

/// The editions of Cratarr's record `album` (`al-<n>`) as its page lists them (F12.1): its own and the others of its album
/// (`Catalogue::editions_near`, `edition_groups`), in the order shown, each with its tracks and its copies. None where it has no
/// other. Blocking.
pub fn of_record(lib: &Library, served: &crate::serve::Serve, album: &str) -> anyhow::Result<Vec<Edition>> {
    let Some(record) = ids::record_number(album) else { return Ok(Vec::new()) };
    let cat = lib.catalogue();
    let near = cat.editions_near(record)?;
    if near.is_empty() {
        return Ok(Vec::new());
    }
    let asked: Vec<i64> = std::iter::once(record).chain(near).collect();
    let Some(group) = cat.edition_groups(&asked, &lib.main_folders())?.into_iter().find(|g| g.contains(&record)) else { return Ok(Vec::new()) };
    let roots: HashMap<i64, String> = served.library.roots().into_iter().collect();
    let albums: Vec<String> = group.iter().map(|r| ids::record(*r)).collect();
    let tracks: HashMap<String, u32> = served.library.albums_by_id(&albums)?.into_iter().map(|a| (a.id, a.song_count)).collect();
    albums
        .into_iter()
        .map(|album| -> anyhow::Result<Edition> {
            let n = tracks.get(&album).copied().unwrap_or(0);
            let copies = served
                .library
                .copies_of(&album)?
                .into_iter()
                .map(|c| AlbumCopy {
                    library: roots.get(&c.folder).cloned().unwrap_or_default(),
                    format: c.format,
                    kbps: c.min_kbps.and_then(|k| u32::try_from(k).ok()),
                    bits: c.bits.and_then(|b| u32::try_from(b).ok()),
                    rate: c.rate.and_then(|r| u32::try_from(r).ok()),
                    added: c.added,
                    tracks: n,
                    folder: c.folder,
                    dir: c.dir,
                    ..AlbumCopy::default()
                })
                .collect();
            Ok(Edition { album, tracks: n, copies })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// F12.1 (the owner: "One card, 2 editions"): cards folded by their editions: of a group, the card earliest in its order (the
    /// edition shown) stays where it stood, marked with how many editions the group holds, though one is not among the cards; the
    /// group's other cards go; a card of no group stays as it is.
    #[test]
    fn editions_of_one_album_show_as_one_card() {
        let card = |id: i64| Album { id: ids::record(id), title: format!("Album {id}"), ..Album::default() };
        let mut cards = vec![card(1), card(2), card(3), card(4), card(5)];
        let groups = [vec![2, 1], vec![5, 9, 3]];
        let went = fold(&mut cards, |a| ids::record_number(&a.id), &groups, |a, n| a.editions = Some(n));
        let shown: Vec<(String, Option<u32>)> = cards.iter().map(|a| (a.id.clone(), a.editions)).collect();
        assert_eq!(shown, [(ids::record(2), Some(2)), (ids::record(4), None), (ids::record(5), Some(3))]);
        assert_eq!(went, 2);
    }
}
