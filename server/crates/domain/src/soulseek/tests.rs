//! The Soulseek machine's tests (SCENARIOS-DC2 KM1–KM6, KM12; C.2's checks).

use super::*;

const T: Time = 1_800_000_000;

fn a() -> SourceKey {
    SourceKey::new("peer-a", r"music\Glass Harbour\Tide Tables [FLAC]")
}

fn b() -> SourceKey {
    SourceKey::new("peer-b", r"share\Tide Tables")
}

fn found(sources: &[(SourceKey, bool)]) -> Item {
    Item { sources: sources.iter().map(|(key, certain)| Source { key: key.clone(), certain: *certain }).collect(), state: State::Found, ..Item::new(11) }
}

/// Every event the walk tries.
fn events(every: bool) -> Vec<Event> {
    let answered = Event::Answered(vec![Source { key: a(), certain: true }, Source { key: b(), certain: false }]);
    let mut out = vec![
        Event::Due,
        answered,
        Event::Certain(a(), T),
        Event::Pick { source: b(), by: Some("root".into()), at: T },
        Event::Started,
        Event::Downloaded,
        Event::CheckTurn { of: 3 },
        Event::Passed,
        Event::Failed(Failure::User),
        Event::Failed(Failure::File),
        Event::Cancel,
        Event::Placed,
    ];
    if every {
        out.extend([
            Event::Missed { again: T + 3600 },
            Event::NoList { why: "No list to judge by".into(), again: T + 86_400 },
            Event::Certain(b(), T),
            Event::Pick { source: a(), by: None, at: T },
            Event::Progress(40),
            Event::Checked(2),
            Event::Failed(Failure::Paused { source: "acoustid".into(), until: T + 120 }),
            Event::Failed(Failure::Away("slskd could not be reached".into())),
            Event::Failed(Failure::NotFound),
            Event::Failed(Failure::Stuck("No list".into())),
            Event::PlaceFailed { until: T + 900, day: T + 86_400 },
            Event::Hold(Hold::Floor),
            Event::Unhold,
            Event::TakeItAnyway(a()),
            Event::TakeOff,
            Event::RecordHad,
            Event::CopyGone,
        ]);
    }
    out
}

/// Every state an item can be in, as the walk starts from it.
fn starts() -> Vec<Item> {
    let two = found(&[(a(), true), (b(), false)]);
    let by = Picker::Administrator(Some("root".into()));
    let with = |state: State| Item { state, ..two.clone() };
    vec![
        Item::new(11),
        with(State::Searching),
        two.clone(),
        with(State::Picked { source: a(), by: Picker::Cratarr, at: T }),
        with(State::Fetching { source: b(), by: by.clone(), percent: 40 }),
        with(State::Fetched { source: a(), by: Picker::Cratarr }),
        with(State::Checking { source: b(), by: by.clone(), done: 1, of: 3 }),
        Item { set_aside: vec![a()], ..with(State::Waiting { due: 0 }) },
        with(State::Placing),
        with(State::Placed),
        with(State::Stuck { why: "No list to judge by".into(), due: T }),
    ]
}

/// C.2's checks between an item before an event and after it.
fn holds(i: &Item, e: &Event, after: &Result<(Item, Does), Refused>) -> Result<(), String> {
    let Ok((n, does)) = after else {
        // 4. A placing is never cancelled: refused, as it is.
        return Ok(());
    };
    // 1. Nothing fetched without a pick, and Cratarr's own pick only where certain.
    if matches!(n.state, State::Fetching { .. }) && !matches!(i.state, State::Picked { .. } | State::Fetching { .. }) {
        return Err(format!("fetching without a pick: {:?} by {e:?}", i.state));
    }
    if let (State::Found, State::Picked { source, by: Picker::Cratarr, .. }) = (&i.state, &n.state)
        && !i.sources.iter().any(|s| s.key == *source && s.certain)
    {
        return Err(format!("Cratarr picked an uncertain source by {e:?}"));
    }
    // 2. A pick's source exists, by its key.
    if let Some(source) = n.state.source()
        && !n.sources.iter().any(|s| s.key == *source)
    {
        return Err(format!("{source:?} picked, not among its sources {:?}, by {e:?}", n.sources));
    }
    // 3. A cancel blames no one, and a hold uses no attempt.
    if matches!(e, Event::Cancel | Event::Hold(_) | Event::Unhold | Event::Failed(Failure::Paused { .. } | Failure::Away(_) | Failure::Other(_)))
        && (n.failed != i.failed || n.misses != i.misses || n.local_failures != i.local_failures || n.place_failures != i.place_failures || n.set_aside != i.set_aside)
    {
        return Err(format!("{e:?} blamed or counted: {i:?} → {n:?}"));
    }
    // 4. A failed placing is placed again from its checked files, never fetched again.
    if i.state == State::Placing && !matches!(n.state, State::Placing | State::Placed) {
        return Err(format!("a placing left for {:?} by {e:?}", n.state));
    }
    // 5. Take off keeps the record dropped.
    if *e == Event::TakeOff && *does != (Does::TakeOff { dropped: true }) {
        return Err(format!("taken off as {does:?}"));
    }
    // A person's Cancel of Cratarr's pick: Cratarr picks nothing for it until a new search (CA6).
    if n.cancelled && matches!(n.state, State::Picked { by: Picker::Cratarr, .. }) {
        return Err("Cratarr picked a cancelled item".into());
    }
    Ok(())
}

fn walk(i: &Item, depth: usize, every: bool, path: &mut Vec<Event>) -> Result<usize, String> {
    if depth == 0 {
        return Ok(1);
    }
    let mut walked = 0;
    for e in events(every) {
        let after = next(i, e.clone());
        path.push(e.clone());
        holds(i, &e, &after).map_err(|why| format!("{why}\n  after {path:?}"))?;
        walked += match after {
            Ok((n, _)) => walk(&n, depth - 1, every, path)?,
            Err(_) => 1,
        };
        path.pop();
    }
    Ok(walked)
}

/// KM1: the machine is pure and walked (C.2's checks): every sequence of six of its main events from a new item, and every sequence
/// of three of all its events from every state.
#[test]
fn the_soulseek_walk_holds_everywhere() {
    let mut walked = walk(&Item::new(11), 6, false, &mut Vec::new()).unwrap_or_else(|e| panic!("{e}"));
    for start in starts() {
        walked += walk(&start, 3, true, &mut Vec::new()).unwrap_or_else(|e| panic!("from {:?}: {e}", start.state));
    }
    assert!(walked > 1_000_000, "{walked}");
}

/// KM2: a source is its user and folder, never its place: root picks A by its key whatever the order a new search gives; after
/// `peer-a` fails, the next source by key (B) under the same pick; a pick naming a source no longer offered is refused.
#[test]
fn a_pick_names_its_source_by_key() {
    let ss1 = found(&[(b(), false), (a(), false)]);
    let (picked, _) = next(&ss1, Event::Pick { source: a(), by: Some("root".into()), at: T }).unwrap();
    assert_eq!(picked.state.source(), Some(&a()), "the folder it named, whatever the order");
    let (fetching, _) = next(&picked, Event::Started).unwrap();
    let (after, does) = next(&fetching, Event::Failed(Failure::User)).unwrap();
    assert_eq!(after.state, State::Picked { source: b(), by: Picker::Administrator(Some("root".into())), at: 0 }, "the next under root's pick");
    assert_eq!((after.failed.as_slice(), does), (&["peer-a".to_string()][..], Does::DropTransfers));
    let gone = SourceKey::new("peer-c", "elsewhere");
    assert_eq!(next(&ss1, Event::Pick { source: gone, by: None, at: T }), Err(Refused::SourceGone));
    assert_eq!(Refused::SourceGone.words(), "That source is gone; look again");
}

/// KM3: a Pick carries who made it: root's tick, Cratarr's certain rule, an old one with no account; after a failure Cratarr's goes
/// on only where the next is certain, else Found and no longer Cratarr's; root's goes to the next under his pick.
#[test]
fn a_pick_says_who_made_it() {
    for by in [Picker::Administrator(Some("root".into())), Picker::Cratarr, Picker::Administrator(None)] {
        let json = serde_json::to_string(&by).unwrap();
        assert_eq!(serde_json::from_str::<Picker>(&json).unwrap(), by, "{json}");
    }
    let mixed = found(&[(a(), true), (b(), false)]);
    let (cratarrs, _) = next(&mixed, Event::Certain(a(), T)).unwrap();
    assert_eq!(cratarrs.state, State::Picked { source: a(), by: Picker::Cratarr, at: T });
    assert_eq!(next(&mixed, Event::Certain(b(), T)).unwrap().0.state, State::Found, "never an uncertain source");
    let (fetching, _) = next(&cratarrs, Event::Started).unwrap();
    assert_eq!(next(&fetching, Event::Failed(Failure::User)).unwrap().0.state, State::Found, "the next is uncertain: Found, no longer Cratarr's");
    let both = found(&[(a(), true), (b(), true)]);
    let (cratarrs, _) = next(&both, Event::Certain(a(), T)).unwrap();
    let (fetching, _) = next(&cratarrs, Event::Started).unwrap();
    assert_eq!(next(&fetching, Event::Failed(Failure::User)).unwrap().0.state, State::Picked { source: b(), by: Picker::Cratarr, at: 0 }, "certain too: goes on");
}

/// KM4: holds are typed and use no attempt: each hold keeps misses, failures and set-asides as they were; "held" is worked out once
/// (`held_at`), a deadline past holding no more; only a hold with a deadline says until when.
#[test]
fn holds_are_typed_and_count_no_try() {
    let fetching = Item { misses: 2, local_failures: 1, place_failures: 1, failed: vec!["peer-c".into()], ..Item { state: State::Fetching { source: a(), by: Picker::Cratarr, percent: 10 }, ..found(&[(a(), true)]) } };
    let holds = [
        Hold::Paused { source: "acoustid".into(), until: T + 120 },
        Hold::SlskdAway { said: "slskd could not be reached".into() },
        Hold::Floor,
        Hold::Room,
        Hold::AcoustidAway { said: "AcoustID answered 503".into() },
        Hold::LocalFailures { until: T + 86_400 },
        Hold::SlskdOff,
        Hold::BackgroundPaused,
        Hold::Other { said: "an error's own words".into() },
    ];
    for h in holds {
        let (n, _) = next(&fetching, Event::Hold(h.clone())).unwrap();
        assert_eq!((n.misses, n.local_failures, n.place_failures, &n.failed, &n.state), (2, 1, 1, &fetching.failed, &fetching.state), "{h:?}");
        assert_eq!(held_at(&n, T), Some(&h));
        assert_eq!(h.until().is_some(), matches!(h, Hold::Paused { .. } | Hold::LocalFailures { .. }), "{h:?}");
    }
    let (paused, _) = next(&fetching, Event::Hold(Hold::Paused { source: "acoustid".into(), until: T + 120 })).unwrap();
    assert_eq!(held_at(&paused, T + 121), None, "a deadline past holds no more");
    assert_eq!(next(&fetching, Event::Failed(Failure::Paused { source: "acoustid".into(), until: T + 120 })).unwrap().0.failed, fetching.failed, "a pause fails no user");
}

/// KM5: a download done is Fetched, waiting its turn to be checked, never Checking before its check begins; the check counts from 0
/// of its files, never back nor past its total.
#[test]
fn a_download_done_waits_its_check_as_fetched() {
    let fetching = Item { state: State::Fetching { source: a(), by: Picker::Cratarr, percent: 99 }, ..found(&[(a(), true)]) };
    let (fetched, _) = next(&fetching, Event::Downloaded).unwrap();
    assert_eq!(fetched.state, State::Fetched { source: a(), by: Picker::Cratarr });
    let (checking, _) = next(&fetched, Event::CheckTurn { of: 9 }).unwrap();
    assert_eq!(checking.state, State::Checking { source: a(), by: Picker::Cratarr, done: 0, of: 9 });
    let (four, _) = next(&checking, Event::Checked(4)).unwrap();
    let count = |i: &Item| match i.state {
        State::Checking { done, of, .. } => (done, of),
        _ => panic!("{:?}", i.state),
    };
    assert_eq!(count(&four), (4, 9));
    assert_eq!(count(&next(&four, Event::Checked(2)).unwrap().0), (4, 9), "never back");
    assert_eq!(count(&next(&four, Event::Checked(12)).unwrap().0), (9, 9), "never past its total");
}

/// KM6's machine half: a placing is never cancelled nor taken off (409, "It is being placed"); a failed one stays Placing, held and
/// placed again from its checked files, a day after three.
#[test]
fn a_placing_is_never_cancelled_and_is_placed_again() {
    let placing = Item { state: State::Placing, ..found(&[(a(), true)]) };
    assert_eq!(next(&placing, Event::Cancel), Err(Refused::BeingPlaced));
    assert_eq!(next(&placing, Event::TakeOff), Err(Refused::BeingPlaced));
    assert_eq!(Refused::BeingPlaced.words(), "It is being placed");
    let mut i = placing.clone();
    for (n, until) in [(1u8, T + 900), (2, T + 900), (3, T + 86_400)] {
        i = next(&i, Event::PlaceFailed { until: T + 900, day: T + 86_400 }).unwrap().0;
        assert_eq!((i.state.clone(), i.held.clone()), (State::Placing, Some(Hold::PlaceFailed { tries: n, until })));
    }
    assert_eq!(next(&i, Event::Unhold).unwrap().1, Does::Place, "placed again from its checked files");
    assert_eq!(next(&i, Event::Placed).unwrap().0.state, State::Placed);
}

/// KM12: TN's failure class is the machine's one failure event: a user's fails that user (the next source, or Waiting); the file's
/// sets the folder aside and fails its user; nothing there is a miss in a search, fetched again in flight; away and paused hold,
/// using no attempt; Stuck is Stuck; anything else holds with its words.
#[test]
fn each_failure_class_is_one_event() {
    let alone = Item { state: State::Fetching { source: a(), by: Picker::Cratarr, percent: 10 }, ..found(&[(a(), true)]) };
    let (user, _) = next(&alone, Event::Failed(Failure::User)).unwrap();
    assert_eq!((user.state, user.failed), (State::Waiting { due: 0 }, vec!["peer-a".to_string()]));
    let (file, does) = next(&alone, Event::Failed(Failure::File)).unwrap();
    assert_eq!((file.set_aside, file.failed, does), (vec![a()], vec!["peer-a".to_string()], Does::SetAside(a())));
    let (forgotten, _) = next(&alone, Event::Failed(Failure::NotFound)).unwrap();
    assert_eq!((forgotten.state, forgotten.failed.len()), (State::Picked { source: a(), by: Picker::Cratarr, at: 0 }, 0), "fetched again, nobody blamed");
    let (away, _) = next(&alone, Event::Failed(Failure::Away("slskd could not be reached".into()))).unwrap();
    assert_eq!((away.held, away.failed.len()), (Some(Hold::SlskdAway { said: "slskd could not be reached".into() }), 0));
    let (paused, _) = next(&alone, Event::Failed(Failure::Paused { source: "acoustid".into(), until: T + 120 })).unwrap();
    assert_eq!((paused.held, paused.state), (Some(Hold::Paused { source: "acoustid".into(), until: T + 120 }), alone.state.clone()));
    let (stuck, _) = next(&alone, Event::Failed(Failure::Stuck("No list".into()))).unwrap();
    assert!(matches!(stuck.state, State::Stuck { .. }));
    let searching = Item { state: State::Searching, ..Item::new(11) };
    let (missed, _) = next(&searching, Event::Failed(Failure::NotFound)).unwrap();
    assert_eq!((missed.state, missed.misses), (State::Waiting { due: 0 }, 1), "a miss");
    let (held, _) = next(&searching, Event::Failed(Failure::Paused { source: "musicbrainz".into(), until: T + 60 })).unwrap();
    assert_eq!((held.state, held.misses), (State::Waiting { due: T + 60 }, 0), "waits until then, never Stuck, no miss");
}

/// CA's machine half (SC01, OQ-C1 (a), OQ-DC2-2 (a)): a Cancel takes a pick, a fetch, a fetched download or a check back to Found
/// with its sources, nobody blamed; one not started asks nothing of slskd; nothing under way refuses; a cancelled pick of Cratarr's
/// is picked by Cratarr no more until a new search, while a person may pick it.
#[test]
fn a_cancel_goes_back_to_found_and_blames_no_one() {
    let two = found(&[(a(), true), (b(), true)]);
    let (cratarrs, _) = next(&two, Event::Certain(a(), T)).unwrap();
    assert_eq!(next(&cratarrs, Event::Cancel).unwrap().1, Does::Nothing, "a pick not started: nothing asked of slskd");
    let (fetching, _) = next(&cratarrs, Event::Started).unwrap();
    let (cancelled, does) = next(&fetching, Event::Cancel).unwrap();
    assert_eq!((cancelled.state.clone(), does, cancelled.failed.len(), cancelled.sources.len()), (State::Found, Does::DropTransfers, 0, 2));
    assert_eq!(next(&cancelled, Event::Certain(a(), T)).unwrap().0.state, State::Found, "not picked again by Cratarr");
    assert_eq!(next(&cancelled, Event::Certain(b(), T)).unwrap().0.state, State::Found, "no source of it");
    assert!(matches!(next(&cancelled, Event::Pick { source: a(), by: Some("root".into()), at: T }).unwrap().0.state, State::Picked { .. }), "a person may");
    let (searching, _) = next(&Item { state: State::Waiting { due: 0 }, ..cancelled.clone() }, Event::Due).unwrap();
    let (again, _) = next(&searching, Event::Answered(vec![Source { key: a(), certain: true }])).unwrap();
    assert!(matches!(next(&again, Event::Certain(a(), T)).unwrap().0.state, State::Picked { by: Picker::Cratarr, .. }), "after a new search, Cratarr may");
    assert_eq!(next(&two, Event::Cancel), Err(Refused::NothingUnderWay));
}

/// TA's machine half (SC03): an administrator takes a set-aside folder in: Placing from any state but Placing and Placed, its user
/// failed no more; a folder not set aside is refused; two takes: the second answers "Being placed", or "Already in Incoming".
#[test]
fn a_set_aside_folder_is_taken_in() {
    let waiting = Item { state: State::Waiting { due: 0 }, set_aside: vec![a(), b()], failed: vec!["peer-a".into(), "peer-b".into()], ..Item::new(11) };
    let (taking, does) = next(&waiting, Event::TakeItAnyway(b())).unwrap();
    assert_eq!((taking.state.clone(), does, taking.failed.clone()), (State::Placing, Does::PlaceSetAside(b()), vec!["peer-a".to_string()]));
    assert_eq!(next(&waiting, Event::TakeItAnyway(SourceKey::new("peer-c", "x"))), Err(Refused::NotSetAside));
    assert_eq!(next(&taking, Event::TakeItAnyway(a())), Err(Refused::BeingPlaced));
    let (placed, _) = next(&taking, Event::Placed).unwrap();
    assert_eq!(next(&placed, Event::TakeItAnyway(a())), Err(Refused::AlreadyPlaced));
    assert_eq!(Refused::AlreadyPlaced.words(), "Already in Incoming");
}
