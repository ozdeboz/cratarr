//! The request machine's tests (SCENARIOS-DC2 SM1–SM4): its one typed state, the walk of every `next()` under every condition
//! (C.1's checks, as `deck.rs` walks its machine), its waits worked out at each read, and Lidarr's want per record.

use super::*;

/// A record's world as a carrier keeps it: its requests (each with whether its asker administers), its holding for its askers, and
/// what holds now; what Lidarr was told, and the stop owed while it is away.
#[derive(Debug, Clone)]
struct World {
    requests: Vec<(Request, bool)>,
    held: Held,
    floor_ok: bool,
    room_ok: bool,
    released: bool,
    lidarr_reached: bool,
    /// Stops told, in order.
    told: u32,
    /// Hand-overs on the want's edge.
    handed: u32,
}

/// What the walk does to a world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Round,
    Holding(Held),
    ArrivalDeleted,
    TakeBack(usize),
    Undone(usize),
}

const T: Time = 1_800_000_000;

impl World {
    fn now(&self, admin: bool) -> Now {
        Now { room_ok: self.room_ok || admin, floor_ok: self.floor_ok, admin, released: self.released, may: true }
    }

    fn open(&self) -> Vec<Asking<'_>> {
        self.requests.iter().filter(|(r, _)| matches!(r, Request::Open { .. })).map(|(r, a)| (r, *a)).collect()
    }

    fn wanted(&self) -> bool {
        lidarr_wanted(&self.open(), self.held, self.floor_ok)
    }

    fn at_it(&self) -> bool {
        self.requests.iter().any(|(r, _)| matches!(r, Request::Open { handed } if handed.lidarr.at_it()))
    }

    /// The carrier: each request's `next()`, the hand-overs it asks for (Lidarr takes it where it answers, else it is away), the
    /// record's want's edge (a stop told once, or owed while Lidarr is away), and the owed stops told once Lidarr answers.
    fn apply(&mut self, step: Step, at: Time) {
        let before = self.wanted();
        let lidarr_was = self.at_it();
        let reached = self.lidarr_reached;
        let events: Vec<Option<Event>> = (0..self.requests.len())
            .map(|i| match step {
                Step::Round => Some(Event::Round(self.now(self.requests[i].1), self.held)),
                Step::Holding(Held::Had) => Some(Event::Whole(at)),
                Step::Holding(_) => None,
                Step::ArrivalDeleted if self.held == Held::Arrived => Some(Event::ArrivalDeleted { at, by: Some("root".into()) }),
                Step::ArrivalDeleted => None,
                Step::TakeBack(n) if n == i => Some(Event::TakeBack { at, by: format!("asker-{i}") }),
                Step::Undone(n) if n == i => Some(Event::Undone),
                _ => None,
            })
            .collect();
        if let Step::Holding(h) = step {
            self.held = h;
        }
        if step == Step::ArrivalDeleted && self.held == Held::Arrived {
            self.held = Held::NotHad;
        }
        for (i, e) in events.into_iter().enumerate() {
            let Some(e) = e else { continue };
            let (r, does) = next(&self.requests[i].0, e);
            self.requests[i].0 = r;
            match does {
                Does::Hand => {
                    let lidarr = if reached { Lidarr::Took } else { Lidarr::Away };
                    let (r, _) = next(&self.requests[i].0, Event::Handed(Handed { lidarr, soulseek: true, ..Handed::default() }));
                    self.requests[i].0 = r;
                }
                Does::CopyIntoOwn => {
                    let (r, _) = next(&self.requests[i].0, Event::Handed(Handed { house: House::Copied, ..Handed::default() }));
                    self.requests[i].0 = r;
                }
                Does::Nothing => {}
            }
        }
        let after = self.wanted();
        match edge(before, after, lidarr_was) {
            Some(Edge::Stop) => {
                let to = stopped(reached, self.held.arrived());
                if reached {
                    self.told += 1;
                }
                for (r, _) in &mut self.requests {
                    if let Request::Open { handed } = r
                        && handed.lidarr.at_it()
                    {
                        *r = next(r, Event::Lidarr(to)).0;
                    }
                }
            }
            Some(Edge::Hand) => self.handed += 1,
            None => {}
        }
        // The outbox: a stop owed is told once Lidarr answers, at the round.
        if step == Step::Round && reached {
            let arrived = self.held.arrived();
            for (r, _) in &mut self.requests {
                if let Request::Open { handed } = r
                    && handed.lidarr == Lidarr::StopOwed
                {
                    *r = next(r, Event::Lidarr(stopped(true, arrived))).0;
                }
            }
        }
    }
}

/// C.1's checks between a world before a step and after it.
fn holds(before: &World, step: Step, after: &World, at: Time) -> Result<(), String> {
    for (i, ((b, _), (a, admin))) in before.requests.iter().zip(&after.requests).enumerate() {
        // 1. A closed request changes only by Undone, which opens it; Declined, Asked twice and Merged never open.
        if let Request::Closed { how, .. } = b
            && a != b
        {
            if step != Step::Undone(i) || matches!(how, How::Declined | How::AskedTwice | How::Merged { .. }) {
                return Err(format!("request {i} closed {how:?} changed by {step:?}"));
            }
            if !matches!(a, Request::Open { .. }) {
                return Err(format!("request {i}'s Undo did not open it"));
            }
        }
        // 7. No event of the walk closes as Declined, Asked twice or Merged: only the step and a merge.
        if let (Request::Open { .. }, Request::Closed { how: How::Declined | How::AskedTwice | How::Merged { .. }, .. }) = (b, a) {
            return Err(format!("request {i} closed as a system closure by {step:?}"));
        }
        // 6. Delivered's time is the copy's whole time.
        if let (Request::Open { .. }, Request::Closed { how: How::Delivered, at: t, .. }) = (b, a)
            && (*t != at || step != Step::Holding(Held::Had))
        {
            return Err(format!("request {i} delivered at {t} by {step:?}"));
        }
        // 5. A copy into an asker's own music only within room and floor.
        if let (Request::Open { handed: hb }, Request::Open { handed: ha }) = (b, a)
            && hb.house != House::Copied
            && ha.house == House::Copied
            && !((after.room_ok || *admin) && (after.floor_ok || *admin))
        {
            return Err(format!("request {i} copied outside its room or the floor"));
        }
        // 3. A stop owed is never dropped: told, or still owed.
        if let (Request::Open { handed: hb }, Request::Open { handed: ha }) = (b, a)
            && hb.lidarr == Lidarr::StopOwed
            && !matches!(ha.lidarr, Lidarr::StopOwed | Lidarr::StoppedForSpace | Lidarr::StoppedOnArrival)
        {
            return Err(format!("request {i}'s owed stop dropped: {:?}", ha.lidarr));
        }
    }
    // 2. Lidarr is told to stop only on the record's want edge, never while another open request of it wants it; below the floor an
    // administrator's keeps it.
    if after.told > before.told && after.wanted() {
        return Err(format!("a stop told while the record is wanted, by {step:?}"));
    }
    if after.told > before.told + 1 {
        return Err("a stop told twice".into());
    }
    let admins_want = after.open().iter().any(|(_, admin)| *admin) && !after.held.arrived();
    if admins_want && !after.wanted() {
        return Err("an administrator's open request does not keep the record wanted".into());
    }
    // 4. The first arrival ends Lidarr's want.
    if after.held.arrived() && after.wanted() {
        return Err("wanted after it arrived".into());
    }
    Ok(())
}

/// Every world: each record shape, each condition.
fn worlds(shapes: &[&[bool]]) -> Vec<World> {
    let mut out = Vec::new();
    for shape in shapes {
        for bits in 0..16u8 {
            let requests = shape.iter().map(|admin| (Request::Open { handed: Handed::default() }, *admin)).collect();
            out.push(World {
                requests,
                held: Held::NotHad,
                floor_ok: bits & 1 != 0,
                room_ok: bits & 2 != 0,
                released: bits & 4 != 0,
                lidarr_reached: bits & 8 != 0,
                told: 0,
                handed: 0,
            });
        }
    }
    out
}

/// The steps of a walk over a record of `n` requests: every holding where `every`, else its arrival and its delivery alone.
fn steps(n: usize, every: bool) -> Vec<Step> {
    let mut out = vec![Step::Round, Step::Holding(Held::Arrived), Step::Holding(Held::Had), Step::ArrivalDeleted];
    if every {
        out.extend([Step::Holding(Held::NotHad), Step::Holding(Held::Partly { has: 3, of: 8 }), Step::Holding(Held::OthersOwn), Step::Holding(Held::Retired)]);
    }
    for i in 0..n {
        out.push(Step::TakeBack(i));
        out.push(Step::Undone(i));
    }
    out
}

/// Walks every sequence of `depth` steps from `w`, checking C.1's checks at each.
fn walk(w: &World, depth: usize, every: bool, at: Time, path: &mut Vec<Step>) -> Result<usize, String> {
    if depth == 0 {
        return Ok(1);
    }
    let mut n = 0;
    for step in steps(w.requests.len(), every) {
        let mut after = w.clone();
        after.apply(step, at);
        path.push(step);
        holds(w, step, &after, at).map_err(|e| format!("{e}\n  after {path:?} from {w:?}"))?;
        n += walk(&after, depth - 1, every, at + 60, path)?;
        path.pop();
    }
    Ok(n)
}

/// SM1: a request's state is one typed value, stored once: it reads back as written, its JSON naming what it is.
#[test]
fn a_request_is_one_typed_state() {
    let states = [
        Request::Open { handed: Handed::default() },
        Request::Open { handed: Handed { lidarr: Lidarr::StopOwed, soulseek: true, house: House::NotCopied("No room".into()), said: Some("Lidarr is looking for it".into()) } },
        Request::Closed { how: How::Delivered, at: T, by: None, reason: None },
        Request::Closed { how: How::Merged { into: 20 }, at: T, by: None, reason: None },
        Request::Closed { how: How::TakenBack, at: T, by: Some("alice".into()), reason: Some("changed my mind".into()) },
    ];
    for s in &states {
        let json = serde_json::to_string(s).unwrap();
        assert_eq!(&serde_json::from_str::<Request>(&json).unwrap(), s, "{json}");
    }
    assert_eq!(serde_json::to_value(&states[3]).unwrap(), serde_json::json!({ "is": "closed", "how": { "how": "merged", "into": 20 }, "at": T }));
}

/// SM2: every `next()` under every condition (C.1's checks): records of one, two and three requests (an administrator's and a
/// user's), every holding, Lidarr reached or away, above or below the floor, room within or over, released or not; every sequence
/// of six steps (Round, a holding, ArrivalDeleted, TakeBack, Undone) for the smaller records, five for the largest.
#[test]
fn the_request_walk_holds_everywhere() {
    let mut walked = 0;
    for w in worlds(&[&[false], &[true], &[true, false]]) {
        walked += walk(&w, 6, false, T, &mut Vec::new()).unwrap_or_else(|e| panic!("{e}"));
    }
    for w in worlds(&[&[false, false, true]]) {
        walked += walk(&w, 5, false, T, &mut Vec::new()).unwrap_or_else(|e| panic!("{e}"));
    }
    // Every holding, partly there, in another's own music and retired among them, four deep.
    for w in worlds(&[&[false], &[true, false], &[false, false, true]]) {
        walked += walk(&w, 4, true, T, &mut Vec::new()).unwrap_or_else(|e| panic!("{e}"));
    }
    assert!(walked > 1_000_000, "{walked} sequences walked");
}

/// SM1/SM2's check 1 and 7, alone: Declined, Asked twice and Merged never open again, whatever comes; Delivered, Rejected and Taken
/// back open by their Undo alone.
#[test]
fn a_closed_request_opens_only_by_its_undo() {
    let closed = |how: How| Request::Closed { how, at: T, by: None, reason: None };
    let now = Now { room_ok: true, floor_ok: true, admin: false, released: true, may: true };
    let every = [Event::Round(now, Held::NotHad), Event::Whole(T), Event::ArrivalDeleted { at: T, by: None }, Event::TakeBack { at: T, by: "alice".into() }, Event::Handed(Handed::default()), Event::Lidarr(Lidarr::Took)];
    for how in [How::Declined, How::AskedTwice, How::Merged { into: 3 }, How::Delivered, How::Rejected, How::TakenBack] {
        for e in every.clone() {
            assert_eq!(next(&closed(how.clone()), e.clone()), (closed(how.clone()), Does::Nothing), "{how:?} by {e:?}");
        }
        let undone = next(&closed(how.clone()), Event::Undone);
        let opens = matches!(how, How::Delivered | How::Rejected | How::TakenBack);
        assert_eq!(matches!(undone.0, Request::Open { .. }), opens, "{how:?}");
    }
}

/// SM3: waits are worked out at each read, never stored. Alice over her room reads Room until the room is raised; bob's Want of an
/// album out on 4 Dec waits for its release until that day; below the floor a user's waits for space and an administrator's goes.
/// No request holds a wait: the same stored request reads each.
#[test]
fn waits_are_worked_out_never_stored() {
    let open = Request::Open { handed: Handed::default() };
    let now = |room_ok, floor_ok, admin, released| Now { room_ok, floor_ok, admin, released, may: true };
    assert_eq!(waits(&open, Held::NotHad, &now(false, true, false, true)), Some(Wait::Room), "alice over her room");
    assert_eq!(waits(&open, Held::NotHad, &now(true, true, false, true)), None, "the room raised: no wait, the same request");
    assert_eq!(next(&open, Event::Round(now(true, true, false, true), Held::NotHad)).1, Does::Hand, "and the round hands it");
    assert_eq!(next(&open, Event::Round(now(false, true, false, true), Held::NotHad)).1, Does::Nothing, "never while it waits");
    assert_eq!(waits(&open, Held::NotHad, &now(true, true, false, false)), Some(Wait::Release), "bob's: before 4 Dec");
    assert_eq!(waits(&open, Held::NotHad, &now(true, false, false, true)), Some(Wait::Space), "carol's, below the floor");
    assert_eq!(waits(&open, Held::NotHad, &now(true, false, true, true)), None, "root's goes");
    assert_eq!(waits(&open, Held::Had, &now(false, false, false, false)), None, "had: nothing to wait for");
    assert_eq!(waits(&Request::Closed { how: How::TakenBack, at: T, by: None, reason: None }, Held::NotHad, &now(false, false, false, false)), None);
    // RF20: a Want of an asker who may not ask now stays where it is, never handed.
    assert_eq!(next(&open, Event::Round(Now { may: false, ..now(true, true, false, true) }, Held::NotHad)).1, Does::Nothing);
}

/// SM4: Lidarr's want is the record's. Record 11 wanted by alice and root, below the floor: root's keeps it; alice taking hers back
/// changes nothing; root's taking his back is the edge, one stop; the first arrival ends the want; a new Want is a hand-over, once.
#[test]
fn lidarr_is_told_on_the_records_edge_alone() {
    let open = Request::Open { handed: Handed { lidarr: Lidarr::Took, ..Handed::default() } };
    let alice_gone = Request::Closed { how: How::TakenBack, at: T, by: Some("alice".into()), reason: None };
    assert!(lidarr_wanted(&[(&open, false), (&open, true)], Held::NotHad, false), "below the floor, root's keeps it");
    assert!(!lidarr_wanted(&[(&open, false)], Held::NotHad, false), "a user's alone does not");
    let before = lidarr_wanted(&[(&open, false), (&open, true)], Held::NotHad, false);
    let after = lidarr_wanted(&[(&alice_gone, false), (&open, true)], Held::NotHad, false);
    assert_eq!(edge(before, after, true), None, "alice's take back tells Lidarr nothing");
    let gone = lidarr_wanted(&[(&alice_gone, false), (&alice_gone, true)], Held::NotHad, false);
    assert_eq!(edge(after, gone, true), Some(Edge::Stop), "root's is the want's edge");
    assert_eq!(edge(gone, gone, true), None, "once");
    assert!(!lidarr_wanted(&[(&open, true)], Held::Arrived, true), "the first arrival ends the want (X1)");
    assert_eq!(edge(true, lidarr_wanted(&[(&open, true)], Held::Arrived, true), true), Some(Edge::Stop));
    let new = Request::Open { handed: Handed::default() };
    assert_eq!(edge(false, lidarr_wanted(&[(&new, false)], Held::NotHad, true), false), Some(Edge::Hand), "a new Want: Hand, once");
    assert_eq!(stopped(false, false), Lidarr::StopOwed, "Lidarr away: owed");
    assert_eq!(stopped(true, true), Lidarr::StoppedOnArrival);
}
