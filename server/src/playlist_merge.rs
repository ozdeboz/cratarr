//! The two-way merge of one playlist (decision 0012): the `.m3u` copy and the user's Jellyfin playlist each may have
//! changed since the last list both agreed on. Pure.
//!
//! - Added on either side: added, after the entry it followed there.
//! - Removed on either side: removed.
//! - Reordered on one side only: that order. Reordered on both: the side that changed last.
//! - A missing track (an `.m3u` line with no track id) exists only in the `.m3u`; Jellyfin cannot see it, so it is
//!   never taken as removed. It stays after the entry it followed.
//!
//! - The result never holds a track twice (decision 0025, amended 2026-10-15): the first copy stays. A list read
//!   with repeats (a server or file that has them) is matched copy by copy, the n-th copy with the n-th.

use std::collections::{HashMap, HashSet};

/// An entry of the `.m3u` copy.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    Track(i64),
    /// A line whose track is not known: kept as written.
    Missing(String),
}

/// Which copy changed last, for when both reordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Newer {
    File,
    Server,
}

/// A track's n-th copy in a list: what the merge matches.
type Key = (i64, usize);

fn keyed(ids: impl IntoIterator<Item = i64>) -> Vec<Key> {
    let mut seen: HashMap<i64, usize> = HashMap::new();
    ids.into_iter()
        .map(|id| {
            let n = seen.entry(id).or_default();
            *n += 1;
            (id, *n)
        })
        .collect()
}

/// The merged list for both copies. `file` is the `.m3u` copy, `server` the Jellyfin one, `base` what both last agreed
/// on (track ids only).
pub fn merge(base: &[i64], file: &[Item], server: &[i64], newer: Newer) -> Vec<Item> {
    let base_k = keyed(base.iter().copied());
    let file_ids: Vec<i64> = file.iter().filter_map(|i| if let Item::Track(t) = i { Some(*t) } else { None }).collect();
    let file_k = keyed(file_ids.iter().copied());
    let server_k = keyed(server.iter().copied());
    let in_base: HashSet<Key> = base_k.iter().copied().collect();
    let in_file: HashSet<Key> = file_k.iter().copied().collect();
    let in_server: HashSet<Key> = server_k.iter().copied().collect();

    // What stays: kept by both sides (from base), or added by either.
    let keep = |k: &Key| if in_base.contains(k) { in_file.contains(k) && in_server.contains(k) } else { true };

    // The order: base order filtered to what both still have; a side reordered when its own order of those differs.
    let common: Vec<Key> = base_k.iter().copied().filter(|k| in_file.contains(k) && in_server.contains(k)).collect();
    let order_of = |side: &[Key]| -> Vec<Key> { side.iter().copied().filter(|k| common.contains(k)).collect() };
    let (file_order, server_order) = (order_of(&file_k), order_of(&server_k));
    let file_moved = file_order != common;
    let server_moved = server_order != common;
    let lead_is_server = match (file_moved, server_moved) {
        (false, true) => true,
        (true, false) => false,
        (true, true) => newer == Newer::Server,
        (false, false) => false,
    };

    // The tracks: the leading side's kept entries, then the other side's additions after the entry they followed
    // there (or at the start).
    let lead: &[Key] = if lead_is_server { &server_k } else { &file_k };
    let other: &[Key] = if lead_is_server { &file_k } else { &server_k };
    let mut tracks: Vec<Key> = lead.iter().copied().filter(|k| keep(k)).collect();
    for (i, k) in other.iter().enumerate() {
        if tracks.contains(k) || in_base.contains(k) {
            continue;
        }
        let at = other[..i].iter().rev().find_map(|p| tracks.iter().position(|t| t == p)).map_or(0, |p| p + 1);
        tracks.insert(at, *k);
    }
    // The first copy of each track only.
    let mut once = HashSet::new();
    tracks.retain(|(id, _)| once.insert(*id));
    // The missing lines: each after the nearest earlier track of the file that is still there (or at the start),
    // several after the same track in their file order; a line once.
    let mut out: Vec<Item> = tracks.iter().map(|(id, _)| Item::Track(*id)).collect();
    let mut placed_after: HashMap<Option<Key>, usize> = HashMap::new();
    let mut earlier: Vec<Key> = Vec::new();
    let mut ki = file_k.iter();
    let mut lines_once = HashSet::new();
    for item in file {
        match item {
            Item::Track(_) => earlier.push(*ki.next().expect("one key per track")),
            Item::Missing(line) => {
                if !lines_once.insert(line) {
                    continue;
                }
                let anchor = earlier.iter().rev().find(|k| tracks.contains(k)).copied();
                let n = placed_after.entry(anchor).or_default();
                // Where the anchor is in `out` now (missing lines before it shift it).
                let base_at = match anchor {
                    Some(a) => {
                        let mut seen = 0;
                        out.iter().position(|o| {
                            if let Item::Track(_) = o {
                                seen += 1;
                                tracks.get(seen - 1) == Some(&a)
                            } else {
                                false
                            }
                        })
                        .map_or(0, |p| p + 1)
                    }
                    None => 0,
                };
                out.insert(base_at + *n, Item::Missing(line.clone()));
                *n += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(ids: &[i64]) -> Vec<Item> {
        ids.iter().map(|&i| Item::Track(i)).collect()
    }

    fn ids(items: &[Item]) -> Vec<i64> {
        items.iter().filter_map(|i| if let Item::Track(t) = i { Some(*t) } else { None }).collect()
    }

    #[test]
    fn nothing_changed_is_nothing() {
        assert_eq!(merge(&[1, 2, 3], &t(&[1, 2, 3]), &[1, 2, 3], Newer::File), t(&[1, 2, 3]));
    }

    #[test]
    fn additions_from_both_sides_land_after_what_they_followed() {
        // Cratarr added 9 after 1; Finamp added 7 at the end.
        let m = merge(&[1, 2, 3], &t(&[1, 9, 2, 3]), &[1, 2, 3, 7], Newer::File);
        assert_eq!(m, t(&[1, 9, 2, 3, 7]));
        // Finamp added at the very start.
        assert_eq!(merge(&[1, 2], &t(&[1, 2]), &[5, 1, 2], Newer::File), t(&[5, 1, 2]));
    }

    #[test]
    fn removals_from_either_side_win() {
        assert_eq!(merge(&[1, 2, 3], &t(&[1, 3]), &[1, 2, 3], Newer::Server), t(&[1, 3]));
        assert_eq!(merge(&[1, 2, 3], &t(&[1, 2, 3]), &[2, 3], Newer::File), t(&[2, 3]));
        assert_eq!(merge(&[1, 2, 3], &t(&[1, 3, 4]), &[1, 2], Newer::File), t(&[1, 4]), "both removed different ones, one added");
    }

    #[test]
    fn a_reorder_on_one_side_wins_and_on_both_the_newer_one() {
        assert_eq!(merge(&[1, 2, 3], &t(&[1, 2, 3]), &[3, 1, 2], Newer::File), t(&[3, 1, 2]), "only Finamp reordered");
        assert_eq!(merge(&[1, 2, 3], &t(&[2, 1, 3]), &[1, 2, 3], Newer::Server), t(&[2, 1, 3]), "only Cratarr reordered");
        assert_eq!(merge(&[1, 2, 3], &t(&[2, 1, 3]), &[3, 2, 1], Newer::Server), t(&[3, 2, 1]));
        assert_eq!(merge(&[1, 2, 3], &t(&[2, 1, 3]), &[3, 2, 1], Newer::File), t(&[2, 1, 3]));
    }

    #[test]
    fn missing_tracks_stay_where_they_were() {
        let file = vec![Item::Track(1), Item::Missing("gone/a.mp3".into()), Item::Track(2), Item::Track(3)];
        // Finamp removed 3 and added 4: the missing line stays after 1.
        let m = merge(&[1, 2, 3], &file, &[1, 2, 4], Newer::File);
        assert_eq!(m, vec![Item::Track(1), Item::Missing("gone/a.mp3".into()), Item::Track(2), Item::Track(4)]);
        // Finamp reordered: the missing line still follows 1.
        let m = merge(&[1, 2, 3], &file, &[3, 1, 2], Newer::File);
        assert_eq!(m, vec![Item::Track(3), Item::Track(1), Item::Missing("gone/a.mp3".into()), Item::Track(2)]);
    }

    #[test]
    fn the_result_never_holds_a_track_twice() {
        // Track 1 twice; the server holds it once.
        assert_eq!(ids(&merge(&[1, 2, 1], &t(&[1, 2, 1]), &[1, 2], Newer::File)), vec![1, 2]);
        // The same track added on both sides, in different places: the first copy stays.
        assert_eq!(ids(&merge(&[1, 2], &t(&[3, 1, 2]), &[1, 2, 3], Newer::File)), vec![3, 1, 2]);
        // A server that lists a repeat: once.
        assert_eq!(ids(&merge(&[2], &t(&[2]), &[2, 2], Newer::File)), vec![2]);
        let file = vec![Item::Missing("gone/a.mp3".into()), Item::Track(1), Item::Missing("gone/a.mp3".into())];
        assert_eq!(merge(&[1], &file, &[1], Newer::File), vec![Item::Missing("gone/a.mp3".into()), Item::Track(1)]);
    }

    #[test]
    fn a_first_sync_without_a_base_keeps_everything_from_both() {
        // No agreed list yet (a new link): the file leads, the server's extra tracks join at the end.
        assert_eq!(merge(&[], &t(&[1, 2]), &[1, 2, 3], Newer::File), t(&[1, 2, 3]));
    }
}
