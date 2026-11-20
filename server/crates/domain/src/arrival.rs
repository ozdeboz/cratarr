//! The arrival's machine, its first form (DOMAIN-CORE §4.4, C.4; SCENARIOS-DC1 AR1): what a copy that came into Incoming is
//! to its review. Its state is worked out from the album actions' journal, never kept apart; its events are those actions'
//! commands. Who a Delete closes is today's `closed_by`: an administrator's closes the requests the copy arrived for, anyone
//! else's only their own.

use serde::{Deserialize, Serialize};

/// An arrival's state: waiting for its review, kept (added) into a library, put in place of a library's copy, rejected
/// (deleted), or gone outside Cratarr.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arrival {
    Waiting,
    Kept { into: i64 },
    Replaced { into: i64 },
    Rejected,
    Gone,
}

/// What happens to an arrival: the album actions (Add to, Replace, Delete, and the Undo of each), or its files going.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    AddTo { into: i64 },
    Replace { into: i64 },
    /// `administrator`: whether `by` administers (`can::Role::administers`), which decides whose requests it closes.
    Delete { by: String, administrator: bool },
    Undo,
    WentOutside,
}

/// What a step asks of the requests: the accounts whose open request of the record it closes, and whether the ones it
/// closed or delivered open again (an Undo: `undeliver`, as today).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Does {
    pub closes: Vec<String>,
    pub reopens: bool,
}

/// One step: the arrival after `event`, and what it asks of the requests of `asked_by` (whose requests it came for). None
/// where the event does not apply: nothing changes, and the command is refused by its state (`can::decide`).
pub fn next(at: Arrival, event: &Event, asked_by: &[String]) -> Option<(Arrival, Does)> {
    match (at, event) {
        (Arrival::Waiting, Event::AddTo { into }) => Some((Arrival::Kept { into: *into }, Does::default())),
        (Arrival::Waiting, Event::Replace { into }) => Some((Arrival::Replaced { into: *into }, Does::default())),
        (Arrival::Waiting, Event::Delete { by, administrator }) => {
            let closes = if *administrator { asked_by.to_vec() } else { asked_by.iter().filter(|asker| *asker == by).cloned().collect() };
            Some((Arrival::Rejected, Does { closes, reopens: false }))
        }
        (Arrival::Kept { .. } | Arrival::Replaced { .. } | Arrival::Rejected, Event::Undo) => {
            Some((Arrival::Waiting, Does { closes: Vec::new(), reopens: true }))
        }
        (Arrival::Gone, Event::WentOutside) => None,
        (_, Event::WentOutside) => Some((Arrival::Gone, Does::default())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MUSIC: i64 = 0;
    const STATES: [Arrival; 6] =
        [Arrival::Waiting, Arrival::Kept { into: MUSIC }, Arrival::Replaced { into: MUSIC }, Arrival::Rejected, Arrival::Gone, Arrival::Kept { into: 4 }];

    fn askers() -> Vec<String> {
        vec!["alice".to_string(), "bob".to_string()]
    }

    fn events() -> Vec<Event> {
        let delete = |by: &str, administrator: bool| Event::Delete { by: by.to_string(), administrator };
        vec![
            Event::AddTo { into: MUSIC },
            Event::Replace { into: MUSIC },
            delete("alice", false),
            delete("carol", false),
            delete("root", true),
            Event::Undo,
            Event::WentOutside,
        ]
    }

    /// AR1's invariants, for one step.
    fn holds(from: Arrival, event: &Event, asked_by: &[String]) {
        let Some((to, does)) = next(from, event, asked_by) else {
            return;
        };
        if matches!(to, Arrival::Kept { .. } | Arrival::Replaced { .. } | Arrival::Rejected) {
            assert_eq!(from, Arrival::Waiting, "kept, replaced and rejected come only from waiting: {from:?} by {event:?}");
        }
        if *event == Event::Undo {
            assert_eq!(to, Arrival::Waiting, "an Undo leads only back to waiting: {from:?}");
        }
        if *event == Event::WentOutside {
            assert_eq!(to, Arrival::Gone);
        }
        if let Event::Delete { by, administrator } = event {
            if *administrator {
                assert_eq!(does.closes, asked_by, "an administrator's Delete closes the requests it arrived for");
            } else {
                assert!(does.closes.iter().all(|asker| asker == by), "never another asker's request: {does:?}");
            }
        }
        assert!(!(matches!(from, Arrival::Kept { .. }) && matches!(event, Event::AddTo { .. })), "never a Keep from Kept");
    }

    fn walk(at: Arrival, left: usize, events: &[Event], asked_by: &[String]) {
        if left == 0 {
            return;
        }
        for event in events {
            holds(at, event, asked_by);
            let after = next(at, event, asked_by).map_or(at, |(to, _)| to);
            walk(after, left - 1, events, asked_by);
        }
    }

    /// AR1, as `deck.rs`'s walk: every state takes every event, then every sequence of six from a new arrival. (Red first:
    /// no machine today; the intake card and `closed_by` decide each step apart.)
    #[test]
    fn the_arrival_walk_holds_everywhere() {
        let (asked_by, events) = (askers(), events());
        for at in STATES {
            for event in &events {
                holds(at, event, &asked_by);
            }
        }
        walk(Arrival::Waiting, 6, &events, &asked_by);
        // The steps themselves.
        let step = |at: Arrival, event: Event| next(at, &event, &asked_by);
        assert_eq!(step(Arrival::Waiting, Event::AddTo { into: MUSIC }).map(|(to, _)| to), Some(Arrival::Kept { into: MUSIC }));
        assert_eq!(step(Arrival::Kept { into: MUSIC }, Event::AddTo { into: MUSIC }), None, "never a Keep from Kept");
        let alices = step(Arrival::Waiting, Event::Delete { by: "alice".to_string(), administrator: false }).unwrap();
        assert_eq!(alices, (Arrival::Rejected, Does { closes: vec!["alice".to_string()], reopens: false }), "her own request only");
        let carols = step(Arrival::Waiting, Event::Delete { by: "carol".to_string(), administrator: false }).unwrap();
        assert!(carols.1.closes.is_empty(), "one who asked nothing closes nothing");
        let roots = step(Arrival::Waiting, Event::Delete { by: "root".to_string(), administrator: true }).unwrap();
        assert_eq!(roots.1.closes, askers(), "an administrator's: every request it arrived for");
        let undone = step(Arrival::Kept { into: MUSIC }, Event::Undo).unwrap();
        assert_eq!(undone, (Arrival::Waiting, Does { closes: Vec::new(), reopens: true }), "AR4: the request reopens");
        assert_eq!(step(Arrival::Waiting, Event::Undo), None, "nothing to undo");
        assert_eq!(step(Arrival::Kept { into: MUSIC }, Event::WentOutside).map(|(to, _)| to), Some(Arrival::Gone));
    }
}
