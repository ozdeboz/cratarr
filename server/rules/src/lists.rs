//! Lists by kind (DOMAIN-CORE §9 and C.6; SCENARIOS-DC2 LI1–LI5): per kind of object, its groups in their order, the states each
//! holds, how its items are ordered inside, and whether it folds. One declaration, read by the server's answers and the page alike
//! (`index.wasm`): no list draws a group of its own, and a held item stays in its state's group (LI2).
//!
//! - **Soulseek's list** (the owner, on production 0.17.0, 2026-11-20: "something that is being looked for is not on its way; it
//!   should be in a different section at the top"): Looking for it, Found, On its way (only what is really coming), Not found yet,
//!   Stuck, and Done folded at the end, newest first, for good (OQ-DC2-1 (a)): nothing is cleared by hand.
//! - **Requests' list** (F22, SB9): the open ones first, then Done folded, newest first. Its count is the open ones'.
//!
//! A state is named as the server's views name it (`is`, snake case).

use serde::Serialize;

/// A group of a list: its id, its title, the states it holds, whether it starts folded, and whether its newest come first (else its
/// oldest asked first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Group {
    pub id: &'static str,
    pub title: &'static str,
    pub states: &'static [&'static str],
    pub folded: bool,
    pub newest_first: bool,
}

/// A kind's list: its groups in order, its filters and its orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct List {
    pub kind: &'static str,
    pub groups: &'static [Group],
    pub filters: &'static [&'static str],
    pub orders: &'static [&'static str],
}

const fn group(id: &'static str, title: &'static str, states: &'static [&'static str]) -> Group {
    Group { id, title, states, folded: false, newest_first: false }
}

const fn done(states: &'static [&'static str]) -> Group {
    Group { id: "done", title: "Done", states, folded: true, newest_first: true }
}

/// Soulseek's list (LI1): Looking for it, Found, On its way, Not found yet, Stuck, Done.
pub const SOULSEEK: List = List {
    kind: "soulseek",
    groups: &[
        group("looking", "Looking for it", &["searching"]),
        group("found", "Found", &["found"]),
        group("on_its_way", "On its way", &["picked", "fetching", "fetched", "checking", "placing"]),
        group("not_found_yet", "Not found yet", &["waiting"]),
        group("stuck", "Stuck", &["stuck"]),
        done(&["placed"]),
    ],
    filters: &[],
    orders: &["asked"],
};

/// Requests' list (LI4): the open ones first (waiting, found, fetching, arrived for review), then Done folded, newest first.
pub const REQUESTS: List = List {
    kind: "request",
    groups: &[
        group("waiting", "Waiting", &["waits", "looking", "not_found_yet", "stuck", "switched_off", "held"]),
        group("found", "Found", &["waits_for_pick"]),
        group("fetching", "On its way", &["fetching", "fetched", "checking", "placing", "partly"]),
        group("arrived", "Arrived", &["arrived"]),
        done(&["delivered", "rejected", "taken_back", "asked_twice", "merged", "declined", "done"]),
    ],
    filters: &["standing", "fetcher", "mine"],
    orders: &["asked", "delivered"],
};

/// The group of `list` that holds `state`, where one does.
pub fn group_of(list: &List, state: &str) -> Option<&'static Group> {
    list.groups.iter().find(|g| g.states.contains(&state))
}

/// Items in their groups (LI1): each in the one group of its state, the groups in the list's order and none empty; inside a group by
/// when each was asked, oldest first, or newest first where the group says so (Done). `state` names an item's state, `at` its time.
pub fn grouped<'a, T>(list: &List, items: &'a [T], state: impl Fn(&T) -> &str, at: impl Fn(&T) -> i64) -> Vec<(&'static Group, Vec<&'a T>)> {
    let mut out: Vec<(&'static Group, Vec<&'a T>)> = Vec::new();
    for g in list.groups {
        let mut of: Vec<&T> = items.iter().filter(|i| g.states.contains(&state(i))).collect();
        if of.is_empty() {
            continue;
        }
        of.sort_by_key(|i| at(i));
        if g.newest_first {
            of.reverse();
        }
        out.push((g, of));
    }
    out
}

/// How many of a list's items count (SB9): those outside its folded Done, never a done or no-longer-wanted one.
pub fn counted<T>(list: &List, items: &[T], state: impl Fn(&T) -> &str) -> usize {
    items.iter().filter(|i| group_of(list, state(i)).is_some_and(|g| !g.folded)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LI1 (R5): Soulseek's groups as the owner decided, in order; each state in one group; Searching never under On its way; an empty
    /// group not shown; inside a group by when asked, Done newest first. LI2: a held item stays in its state's group.
    #[test]
    fn soulseeks_groups_are_the_owners() {
        let ids: Vec<&str> = SOULSEEK.groups.iter().map(|g| g.id).collect();
        assert_eq!(ids, ["looking", "found", "on_its_way", "not_found_yet", "stuck", "done"]);
        let states = ["waiting", "searching", "found", "picked", "fetching", "fetched", "checking", "placing", "placed", "stuck"];
        for s in states {
            assert_eq!(SOULSEEK.groups.iter().filter(|g| g.states.contains(&s)).count(), 1, "{s} in one group");
        }
        assert_eq!(group_of(&SOULSEEK, "searching").map(|g| g.title), Some("Looking for it"), "never under On its way");
        let items = [("placed", 10), ("searching", 5), ("placed", 30), ("fetching", 7), ("picked", 2)];
        let g = grouped(&SOULSEEK, &items, |i| i.0, |i| i.1);
        let shown: Vec<(&str, Vec<i64>)> = g.iter().map(|(g, is)| (g.id, is.iter().map(|i| i.1).collect())).collect();
        assert_eq!(shown, [("looking", vec![5]), ("on_its_way", vec![2, 7]), ("done", vec![30, 10])], "no empty group; Done newest first");
        let done = group_of(&SOULSEEK, "placed").unwrap();
        assert!(done.folded && done.title == "Done", "folded, at the end");
    }

    /// LI4: Requests' list, open first and a folded Done, newest first; its count the open ones (SB9), never a done one.
    #[test]
    fn requests_list_open_first_and_fold_done() {
        let ids: Vec<&str> = REQUESTS.groups.iter().map(|g| g.id).collect();
        assert_eq!(ids, ["waiting", "found", "fetching", "arrived", "done"]);
        let alices = [("waits", 1), ("waits", 2), ("waits_for_pick", 3), ("fetching", 4), ("arrived", 5), ("delivered", 6), ("delivered", 8), ("rejected", 7), ("taken_back", 9)];
        assert_eq!(counted(&REQUESTS, &alices, |r| r.0), 5, "the open ones");
        let g = grouped(&REQUESTS, &alices, |r| r.0, |r| r.1);
        let last = g.last().unwrap();
        assert_eq!((last.0.id, last.1.iter().map(|r| r.1).collect::<Vec<_>>()), ("done", vec![9, 8, 7, 6]), "Done last, newest first");
        for s in ["delivered", "rejected", "taken_back", "asked_twice", "merged", "declined"] {
            assert!(group_of(&REQUESTS, s).is_some_and(|g| g.folded), "{s} done");
        }
        assert_eq!(REQUESTS.orders, ["asked", "delivered"]);
    }
}
