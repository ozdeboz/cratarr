//! The two-way merge of one playlist (decision 0012): the `.m3u` copy and the user's Jellyfin playlist each may have
//! changed since the last list both agreed on. Pure.
//!
//! - Added on either side: added, after the entry it followed there.
//! - Removed on either side: removed.
//! - Reordered on one side only: that order. Reordered on both: the side that changed last.
//! - A missing track (an `.m3u` line with no track id) exists only in the `.m3u`; Jellyfin cannot see it, so it is
//!   never taken as removed. It stays after the entry it followed.
//!
//! - Repeats are kept (decision 0025, amended 2026-10-16): lists are matched copy by copy, the n-th copy with the
//!   n-th, so a copy added or taken out on either side is added or taken out.
//! - A server that holds each track once (Jellyfin) merges with `merge_once`: on the first copies, the file's other
//!   copies put back after what they followed there, so the copies the server cannot hold never read as removed.

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
    // The missing lines: each after the nearest earlier track of the file that is still there (or at the start),
    // several after the same track in their file order.
    let mut out: Vec<Item> = tracks.iter().map(|(id, _)| Item::Track(*id)).collect();
    let mut placed_after: HashMap<Option<Key>, usize> = HashMap::new();
    let mut earlier: Vec<Key> = Vec::new();
    let mut ki = file_k.iter();
    for item in file {
        match item {
            Item::Track(_) => earlier.push(*ki.next().expect("one key per track")),
            Item::Missing(line) => {
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

/// As `merge`, with a server that holds each track once (Jellyfin): the copies after a track's first are not the
/// server's to keep or take out. The merge runs on the first copies of `base` and `file`; then each other copy of a
/// track the result keeps goes back after what it followed in the file (or at the start). A track gone from either
/// side goes with all its copies; a copy taken out in the file stays out.
pub fn merge_once(base: &[i64], file: &[Item], server: &[i64], newer: Newer) -> Vec<Item> {
    let first = |ids: &mut HashSet<i64>, item: &Item| match item {
        Item::Track(t) => ids.insert(*t),
        Item::Missing(_) => true,
    };
    let mut seen = HashSet::new();
    let base_firsts: Vec<i64> = base.iter().copied().filter(|t| seen.insert(*t)).collect();
    let mut seen = HashSet::new();
    let file_firsts: Vec<Item> = file.iter().filter(|i| first(&mut seen, i)).cloned().collect();
    let mut seen = HashSet::new();
    let server_firsts: Vec<i64> = server.iter().copied().filter(|t| seen.insert(*t)).collect();
    let mut out = merge(&base_firsts, &file_firsts, &server_firsts, newer);
    let kept: HashSet<i64> = out.iter().filter_map(|i| if let Item::Track(t) = i { Some(*t) } else { None }).collect();
    // The n-th time `item` occurs in `list`, as an index.
    let nth = |list: &[Item], item: &Item, n: usize| list.iter().enumerate().filter(|(_, x)| *x == item).nth(n).map(|(i, _)| i);
    let mut counts: HashMap<&Item, usize> = HashMap::new();
    for (i, item) in file.iter().enumerate() {
        let n = *counts.entry(item).and_modify(|c| *c += 1).or_insert(0);
        let Item::Track(t) = item else { continue };
        if n == 0 || !kept.contains(t) {
            continue;
        }
        // After the nearest earlier item of the file that the result has (by its own occurrence), else at the start.
        let at = file[..i].iter().enumerate().rev().find_map(|(j, prev)| {
            let m = file[..j].iter().filter(|x| *x == prev).count();
            nth(&out, prev, m).map(|p| p + 1)
        });
        out.insert(at.unwrap_or(0), item.clone());
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
    fn repeats_are_merged_copy_by_copy() {
        // Unchanged repeats stay.
        assert_eq!(ids(&merge(&[1, 2, 1], &t(&[1, 2, 1]), &[1, 2, 1], Newer::File)), vec![1, 2, 1]);
        // A repeat added on one side is added.
        assert_eq!(ids(&merge(&[1, 2], &t(&[1, 2, 1]), &[1, 2], Newer::File)), vec![1, 2, 1]);
        assert_eq!(ids(&merge(&[2], &t(&[2]), &[2, 2], Newer::File)), vec![2, 2]);
        // A copy taken out on one side is taken out.
        assert_eq!(ids(&merge(&[1, 2, 1], &t(&[1, 2]), &[1, 2, 1], Newer::File)), vec![1, 2]);
        // The same track added on both sides, in different places: one copy each side agrees on.
        assert_eq!(ids(&merge(&[1, 2], &t(&[3, 1, 2]), &[1, 2, 3], Newer::File)), vec![3, 1, 2]);
        // A missing line twice stays twice.
        let file = vec![Item::Missing("gone/a.mp3".into()), Item::Track(1), Item::Missing("gone/a.mp3".into())];
        assert_eq!(merge(&[1], &file, &[1], Newer::File), file);
    }

    #[test]
    fn a_server_that_holds_each_track_once_never_takes_out_the_files_repeats() {
        // The file holds 1 twice; the server once: nothing changed.
        assert_eq!(ids(&merge_once(&[1, 2], &t(&[1, 2, 1]), &[1, 2], Newer::File)), vec![1, 2, 1]);
        // A base kept with repeats reads the same.
        assert_eq!(ids(&merge_once(&[1, 2, 1], &t(&[1, 2, 1]), &[1, 2], Newer::File)), vec![1, 2, 1]);
        // Added on the server: added, the repeats stay where they were.
        assert_eq!(ids(&merge_once(&[1, 2], &t(&[1, 2, 1]), &[1, 2, 3], Newer::File)), vec![1, 2, 1, 3]);
        // Taken out on the server: every copy goes.
        assert_eq!(ids(&merge_once(&[1, 2], &t(&[1, 2, 1]), &[2], Newer::File)), vec![2]);
        // A copy taken out in the file stays out.
        assert_eq!(ids(&merge_once(&[1, 2], &t(&[1, 2]), &[1, 2], Newer::File)), vec![1, 2]);
        // Reordered on the server: its order for the first copies, a repeat after what it followed in the file.
        assert_eq!(ids(&merge_once(&[1, 2, 3], &t(&[1, 2, 3, 2]), &[3, 2, 1], Newer::Server)), vec![3, 2, 2, 1]);
        // Missing lines keep their places.
        let file = vec![Item::Track(1), Item::Missing("gone/a.mp3".into()), Item::Track(1)];
        assert_eq!(merge_once(&[1], &file, &[1], Newer::File), file);
    }

    #[test]
    fn a_first_sync_without_a_base_keeps_everything_from_both() {
        // No agreed list yet (a new link): the file leads, the server's extra tracks join at the end.
        assert_eq!(merge(&[], &t(&[1, 2]), &[1, 2, 3], Newer::File), t(&[1, 2, 3]));
    }
}
