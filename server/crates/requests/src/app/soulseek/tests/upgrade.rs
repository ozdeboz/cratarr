//! The upgrade from 0.16.3's queue (SCENARIOS-SK17, Part 4 and SK5.6), on its fixture (`tests/fixtures/0.16.3`, made by the plan's
//! generator): 57 items, 2 fetching (b-01 of peer-01, b-02 of peer-02), 20 approved held for slskd's 429, 18 found, 8 waiting, 8 stuck,
//! 1 placed; 27 folders set aside on 10 of them; the checking variant adds ss-58, b-03 of peer-03. The fixture's times are around its
//! t0, 1_800_000_000, ahead of the clock: tests pass their times.
//!
//! The list is every test's: an item a test puts on it is named `<tag>-<its id>`.

use super::*;

const QUEUE: &[u8] = include_bytes!("../../../../../../tests/fixtures/0.16.3/soulseek.json");
const CHECKING: &[u8] = include_bytes!("../../../../../../tests/fixtures/0.16.3/soulseek-checking.json");
const REQUESTS: &[u8] = include_bytes!("../../../../../../tests/fixtures/0.16.3/requests.json");
const REQUESTS_CHECKING: &[u8] = include_bytes!("../../../../../../tests/fixtures/0.16.3/requests-checking.json");
/// The fixture's t0: 0.16.3 wrote the queue ten minutes before it.
const T0: i64 = 1_800_000_000;

/// A data folder of its own (`tag`) holding 0.16.3's queue `bytes` as `soulseek.json`, and the queue as 0.17's first start reads it
/// (`load`): the folder, the file.
fn loaded(tag: &str, bytes: &[u8]) -> (PathBuf, PathBuf, Kept) {
    let d = data_folder(tag);
    let p = d.join("soulseek.json");
    std::fs::write(&p, bytes).unwrap();
    let k = load(&p);
    (d, p, k)
}

/// The queue as 0.16.3 kept it, read as 0.17 reads a queue's items, with nothing taken up.
fn as_kept(bytes: &[u8]) -> Kept {
    serde_json::from_slice(bytes).unwrap()
}

/// Item `id` of a queue.
fn of_queue<'a>(k: &'a Kept, id: &str) -> &'a Item {
    k.items.iter().find(|i| i.id == id).unwrap_or_else(|| panic!("no {id}"))
}

/// The fixture's ids of items in `state` as 0.16.3 kept them.
fn ids_in(state: State) -> Vec<String> {
    as_kept(QUEUE).items.into_iter().filter(|i| i.state == state).map(|i| i.id).collect()
}

/// The round as `read_requests` takes the list (`take_requests`), on the fixture's approved requests `requests` (its items' and the
/// dropped 900-911), each request's record 10_000 and its id: what it handed back to be stopped.
fn first_round(k: &mut Kept, requests: &[u8]) -> Vec<Item> {
    let rs: Vec<crate::userdata::Request> = serde_json::from_slice(requests).unwrap();
    let of: Vec<_> = rs.iter().map(|r| of(10_000 + r.id)).collect();
    take_requests(k, &rs, &of, &vec![0; rs.len()])
}

/// The queue's items on the list, each named `<tag>-<its id>`.
fn on_list(tag: &str, k: &Kept) {
    let mine: Vec<Item> = k.items.iter().map(|i| Item { id: format!("{tag}-{}", i.id), ..i.clone() }).collect();
    let prefix = format!("{tag}-");
    kept(|q| {
        q.items.retain(|i| !i.id.starts_with(&prefix));
        q.items.extend(mine);
    });
}

/// Item `ss-<n>` of a test's `tag` on the list.
fn the(tag: &str, n: u32) -> Item {
    item(&format!("{tag}-ss-{n}")).unwrap_or_else(|| panic!("no {tag}-ss-{n}"))
}

/// slskd holding 0.16.3's two downloads as the fixture has them (b-01 of peer-01 at 42%, b-02 of peer-02 at 7.5%), each batch kept
/// going till the test lets it end, its files written under `d` then.
fn slskd_of_0163(k: &Kept, d: &Path) -> Scripted {
    let files = |id: &str| of_queue(k, id).folders[0].files.iter().map(|f| f.filename.clone()).collect::<Vec<_>>();
    Scripted::new().hold_all(5.0).writes_to(d, None).knows("b-01", "peer-01", &files("ss-1"), 42.0).knows("b-02", "peer-02", &files("ss-2"), 7.5)
}

/// The batches asked for and taken of `tag`'s item `ss-<n>`, by its chosen folder's files.
fn taken_for(slskd: &Scripted, tag: &str, n: u32) -> Vec<String> {
    let i = the(tag, n);
    let files: Vec<String> = i.folders[i.chosen.unwrap()].files.iter().map(|f| f.filename.clone()).collect();
    slskd.posts().into_iter().filter(|p| p.status == 200 && p.files == files).map(|p| p.batch).collect()
}

/// Every look and every download's reads of `within`, each download let end in turn, till no picked record of it is left to start.
async fn every_one_taken(within: Within<'_>, slskd: &Scripted, d: &Path, look: &Look) {
    let client = slskd.client();
    for _ in 0..40 {
        start_downloads(within, &client, d, look).await;
        follow_round(within, &client, d).await;
        let going: Vec<String> = kept(|q| q.items.iter().filter(|i| within(i) && i.state == State::Fetching).filter_map(|i| i.batch.clone()).collect());
        let left = kept(|q| q.items.iter().any(|i| within(i) && i.state == State::Approved));
        if going.is_empty() && !left {
            return;
        }
        for b in going {
            slskd.release(&b);
        }
        follow_round(within, &client, d).await;
    }
    panic!("never all taken");
}

/// The 20 approved items' numbers in the fixture: ss-3 … ss-22.
const APPROVED: std::ops::RangeInclusive<u32> = 3..=22;

/// SK4.1: 0.16.3's queue is read whole: 57 items, the file kept aside as `.016` once, byte for byte; nothing changed in the first
/// write but the 429's words (SK4.2) and the shape 0.17 writes (no release group, no record yet, 0.17's fields at their defaults); each
/// item given its request's record at the first round; the dropped requests their dropped records, no item made of them.
#[test]
fn the_0163_queue_is_read_whole() {
    let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
    let (d, p, mut k) = loaded("u41", QUEUE);
    // A stop before the first round loses none of the requests taken off the list: written while they are left.
    let written = serde_json::to_value(&k).unwrap();
    let was: serde_json::Value = serde_json::from_slice(QUEUE).unwrap();
    assert_eq!(written["dropped"], was["dropped"], "the dropped requests in the first write");
    assert_eq!((k.items.len(), k.frozen), (57, false));
    assert_eq!(load_for_tests(&p), (57, false), "read again: every one");
    assert_eq!(std::fs::read(p.with_extension("json.016")).unwrap(), QUEUE, "kept aside byte for byte");
    assert!(!std::fs::read_dir(&d).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with(UNREADABLE)), "never set aside as unreadable");
    assert_eq!(written["next"], was["next"]);
    for (a, b) in was["items"].as_array().unwrap().iter().zip(written["items"].as_array().unwrap()) {
        let id = a["id"].as_str().unwrap();
        let held_for_429 = a["why"].as_str().is_some_and(|w| w.contains("429"));
        for (key, value) in a.as_object().unwrap() {
            match key.as_str() {
                "record" => {
                    let (mut r, mut n) = (value.clone(), b["record"].clone());
                    r.as_object_mut().unwrap().remove("release_group");
                    assert_eq!(n.as_object_mut().unwrap().remove("record"), Some(serde_json::json!(0)), "{id}: no record till the first round");
                    assert_eq!(n, r, "{id}: its names and request as 0.16.3 kept them");
                }
                "why" if held_for_429 => assert_eq!(b.get("why"), None, "{id}: the 429's words cleared"),
                "due" if held_for_429 => assert_eq!(b["due"], 0, "{id}: due now"),
                _ => assert_eq!(b.get(key), Some(value), "{id}: {key} as 0.16.3 kept it"),
            }
        }
        for key in ["untimed", "checked", "placing"] {
            assert_eq!(b.get(key), None, "{id}: {key} not there");
        }
        for key in ["local_failures", "placed_at", "place_failures", "ended_at", "forgotten", "found_at"] {
            assert_eq!(b[key], 0, "{id}: {key} at its default");
        }
        assert_eq!(b["auto"], false, "{id}: not Cratarr's pick");
    }
    // The first round: each item its request's record; the dropped requests' records dropped, no item made of them.
    let stopped = first_round(&mut k, REQUESTS);
    assert!(stopped.is_empty(), "nothing taken off");
    assert_eq!(k.items.len(), 57, "every one, none made");
    assert!(k.items.iter().all(|i| i.record.record == 10_000 + i.record.request), "each its request's record");
    assert_eq!(k.dropped_records, (900..=911).map(|r| 10_000 + r).collect::<Vec<i64>>());
    assert!(k.dropped.is_empty());
    // A later start: `.016` never written again.
    std::fs::write(&p, serde_json::to_vec(&k).unwrap()).unwrap();
    assert_eq!(load(&p).items.len(), 57);
    assert_eq!(std::fs::read(p.with_extension("json.016")).unwrap(), QUEUE, "once");
    std::fs::remove_dir_all(&d).ok();
}

/// SK4.2: the 20 approved records 0.16.3 held for slskd's 429 go ahead: their why cleared and due 0 at the load; their POSTs one after
/// another through the lane, never two of one user: 15 at once, the three second ones after their peer's first ends, peer-01's and
/// peer-02's after b-01's and b-02's downloads end; each then fetching with its pick as 0.16.3 kept it, not Cratarr's; nobody failed.
#[tokio::test]
async fn the_approved_items_0163_held_for_slskds_429_go_ahead() {
    let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
    let (d, _, k) = loaded("u42", QUEUE);
    let was = as_kept(QUEUE);
    assert!(k.items.iter().filter(|i| i.state == State::Approved).count() == 20);
    assert!(k.items.iter().filter(|i| i.state == State::Approved).all(|i| i.why.is_none() && i.due == 0), "cleared at the load");
    assert!(k.items.iter().all(|i| !i.why.as_deref().unwrap_or_default().contains("429")), "a 429's words on no item");
    on_list("u42", &k);
    let slskd = slskd_of_0163(&k, &d);
    let (prefix, client, look) = ("u42-".to_string(), slskd.client(), Look::roomy());
    let ours = ours(&prefix);
    start_downloads(&ours, &client, &d, &look).await;
    assert_eq!(slskd.posts().len(), 15, "20, less peer-01's two and peer-02's, less the second ones of peer-03 and peer-05");
    for n in [3, 20, 4, 21, 22] {
        let i = the("u42", n);
        assert!(i.state == State::Approved && i.batch.is_none() && i.due == 0, "ss-{n} waits for its user's other download: {i:?}");
    }
    // Each user's next once their download before ends.
    for (ends, next) in [(5, 21), (7, 22)] {
        slskd.release(&the("u42", ends).batch.unwrap());
        follow_round(&ours, &client, &d).await;
        start_downloads(&ours, &client, &d, &look).await;
        assert_eq!(slskd.posts().last().map(|p| p.batch.clone()), the("u42", next).batch, "ss-{next} after ss-{ends}");
    }
    for (ends, next, after) in [("b-01", 3, 20), ("b-02", 4, 0)] {
        slskd.release(ends);
        follow_round(&ours, &client, &d).await;
        start_downloads(&ours, &client, &d, &look).await;
        assert_eq!(slskd.posts().last().map(|p| p.batch.clone()), the("u42", next).batch, "ss-{next} after {ends}");
        if after > 0 {
            assert_eq!(the("u42", after).state, State::Approved, "ss-{after} still waits for ss-{next}");
            slskd.release(&the("u42", next).batch.unwrap());
            follow_round(&ours, &client, &d).await;
            start_downloads(&ours, &client, &d, &look).await;
            assert_eq!(slskd.posts().last().map(|p| p.batch.clone()), the("u42", after).batch, "ss-{after} after ss-{next}");
        }
    }
    assert_eq!(slskd.posts().len(), 20);
    for n in APPROVED {
        let (i, w) = (the("u42", n), of_queue(&was, &format!("ss-{n}")));
        assert!(matches!(i.state, State::Fetching | State::Checking), "ss-{n}: {i:?}");
        assert_eq!((i.chosen, &i.folders, i.auto, &i.failed), (w.chosen, &w.folders, false, &w.failed), "ss-{n}: its pick as 0.16.3 kept it");
        assert_eq!(taken_for(&slskd, "u42", n).len(), 1, "ss-{n}: one batch");
    }
    assert!(slskd.never_overlapped() && slskd.violations().is_empty(), "{:?}", slskd.violations());
    std::fs::remove_dir_all(&d).ok();
}

/// SK4.3 (SC02.U): 0.16.3's two downloads and its one check (the variant's b-03) are followed: both downloads read at once, their
/// batches kept; b-03 checked again, every file, first among the checks (it has no end time); peer-01's approved record waits for
/// b-01. Never a batch cancelled, a user blamed, b-03's files lost or two checks at once.
#[tokio::test]
async fn several_fetches_and_a_check_from_0163_are_followed() {
    let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
    let (d, _, mut k) = loaded("u43", CHECKING);
    let was = as_kept(CHECKING);
    assert!(first_round(&mut k, REQUESTS_CHECKING).is_empty() && k.items.len() == 58, "none taken off for more than one under way");
    on_list("u43", &k);
    let slskd = slskd_of_0163(&k, &d);
    // b-03's files where 0.16.3 left them, at its check.
    let b3 = d.join(crate::slskd::FOLDER).join("b-03");
    std::fs::create_dir_all(&b3).unwrap();
    let checked = &of_queue(&k, "ss-58").folders[0].files;
    for f in checked {
        std::fs::write(b3.join(crate::slskd::base_name(&f.filename)), b"x").unwrap();
    }
    let (prefix, client, look) = ("u43-".to_string(), slskd.client(), Look::roomy());
    let ours = ours(&prefix);
    start_downloads(&ours, &client, &d, &look).await;
    follow_round(&ours, &client, &d).await;
    assert!(slskd.reads_of("b-01") > 0 && slskd.reads_of("b-02") > 0, "both followed");
    for (n, b, percent) in [(1, "b-01", 42.0), (2, "b-02", 7.5)] {
        let i = the("u43", n);
        assert_eq!((i.state, i.batch.as_deref(), i.percent), (State::Fetching, Some(b), percent), "ss-{n}");
    }
    assert_eq!(the("u43", 3).state, State::Approved, "peer-01's record waits for b-01");
    // b-01 ends: peer-01's next goes, and b-01's check comes after b-03's, which has no end time.
    slskd.release("b-01");
    follow_round(&ours, &client, &d).await;
    assert!(the("u43", 1).state == State::Checking && the("u43", 1).ended_at > 0);
    start_downloads(&ours, &client, &d, &look).await;
    assert_eq!(the("u43", 3).state, State::Fetching, "peer-01's record goes once b-01 is done");
    let ids: Vec<String> = [58, 1].iter().map(|n| format!("u43-ss-{n}")).collect();
    let gate = HeardInTurn::new().watching(&ids.iter().map(String::as_str).collect::<Vec<_>>());
    let g = gate.gate();
    check_until_idle(&ours, &client, &d, &places_in(&d), &g).await;
    assert_eq!(gate.batches_in_order(), ["b-03", "b-01"], "b-03 first, then b-01");
    assert_eq!(gate.calls().iter().filter(|c| c.0 == "b-03").count(), checked.len(), "every file of b-03");
    assert!(gate.violations.lock().unwrap().is_empty(), "{:?}", gate.violations.lock().unwrap());
    let placed = the("u43", 58);
    assert_eq!(placed.state, State::Placed, "{placed:?}");
    assert_eq!(files_under(Path::new(placed.placed.as_deref().unwrap())).len(), checked.len(), "b-03's files placed, none lost");
    assert!(slskd.going().contains(&"b-02".to_string()) && !slskd.deleted("b-02"), "b-02 goes on, never cancelled");
    for n in [1, 2, 58] {
        assert_eq!(the("u43", n).failed, of_queue(&was, &format!("ss-{n}")).failed, "ss-{n}: nobody blamed");
    }
    assert!(slskd.violations().is_empty(), "{:?}", slskd.violations());
    std::fs::remove_dir_all(&d).ok();
}

/// SK4.4, what Part 4 decides alone (OQ-SK3 (a)): the 18 found items 0.16 left are searched again once the round gives them their
/// record, with 0.17's counting list read anew, before anything may start from 0.16's folders. Their automatic start, where certain,
/// is Part 3's (`the_found_items_of_0163_are_searched_again_and_start_where_certain`).
#[tokio::test]
async fn the_found_items_of_0163_are_searched_again_once_given_their_record() {
    let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
    let (d, _, mut k) = loaded("u44", QUEUE);
    let found = ids_in(State::Found);
    assert_eq!(found.len(), 18);
    // Before the round gives them their record (none at the load): as 0.16 left them.
    for id in &found {
        let i = of_queue(&k, id);
        assert!(i.state == State::Found && i.record.record == 0 && !i.folders.is_empty(), "{id}: {i:?}");
    }
    first_round(&mut k, REQUESTS);
    for id in &found {
        let i = of_queue(&k, id);
        assert_eq!((i.state, i.due, i.why.as_deref(), i.chosen), (State::Waiting, 0, None, None), "{id}: searched again now");
        assert!(i.folders.is_empty() && i.tracks.is_empty() && i.untimed.is_empty(), "{id}: 0.16's folders and list gone: its list read anew");
        assert!(i.record.record != 0 && i.found_at == 0 && !i.auto, "{id}: {i:?}");
    }
    // Nothing asked of slskd for them: none is picked.
    on_list("u44", &k);
    let slskd = Scripted::new();
    let mine: Vec<String> = found.iter().map(|id| format!("u44-{id}")).collect();
    let theirs = |i: &Item| mine.contains(&i.id);
    start_downloads(&theirs, &slskd.client(), &d, &Look::roomy()).await;
    assert!(slskd.posts().is_empty(), "none started from 0.16's folders");
    // A second round changes nothing more: the record given once.
    let again: Vec<Item> = k.items.clone();
    first_round(&mut k, REQUESTS);
    assert_eq!(k.items, again);
    std::fs::remove_dir_all(&d).ok();
}

/// SK4.4, Part 3's half: the 18 found items 0.16 left start by themselves where the folders their search again finds are certain;
/// the others wait for a pick, with those fresh folders. Never one with record 0 (before the round gives it), never from 0.16's
/// folders as they are, nobody failed by the search again.
#[tokio::test]
async fn the_found_items_of_0163_are_searched_again_and_start_where_certain() {
    let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
    let (d, _, mut k) = loaded("u44a", QUEUE);
    let found = ids_in(State::Found);
    let slskd = Scripted::new().hold_all(5.0);
    on_list("u44z", &k);
    let unrecorded: Vec<String> = found.iter().map(|id| format!("u44z-{id}")).collect();
    start_downloads(&|i: &Item| unrecorded.contains(&i.id), &slskd.client(), &d, &Look::roomy()).await;
    assert!(slskd.posts().is_empty(), "never an automatic start of an item with record 0, nor from 0.16's folders");
    first_round(&mut k, REQUESTS);
    on_list("u44a", &k);
    let mine: Vec<String> = found.iter().map(|id| format!("u44a-{id}")).collect();
    let failed_before: Vec<Vec<String>> = mine.iter().map(|id| item(id).unwrap().failed).collect();
    // Their searches come back: every other one an exact folder with a free slot, the rest only a fuller folder.
    for (n, id) in mine.iter().enumerate() {
        with_item(id, |i| {
            let mut f = two_tracks(&format!("peer-u44a-{n}"));
            if n % 2 == 1 {
                (f.extra, f.unchecked) = (1, Some("Takes 2 of 3 files".into()));
            }
            (i.tracks, i.state, i.folders, i.found_at, i.chosen) = (vec![track(1, "One", 100), track(2, "Two", 200)], State::Found, vec![f], now(), None);
        });
    }
    start_downloads(&|i: &Item| mine.contains(&i.id), &slskd.client(), &d, &Look::roomy()).await;
    for (n, id) in mine.iter().enumerate() {
        let i = item(id).unwrap();
        if n % 2 == 0 {
            assert!(i.state == State::Fetching && i.auto, "{id}: certain, it starts by itself: {i:?}");
        } else {
            assert!(i.state == State::Found && !i.auto && i.folders.len() == 1, "{id}: it waits for a pick, its fresh folders kept: {i:?}");
        }
        assert_eq!(i.failed, failed_before[n], "{id}: nobody failed by searching again");
    }
    assert_eq!(slskd.posts().len(), 9);
    assert!(slskd.violations().is_empty(), "{:?}", slskd.violations());
    std::fs::remove_dir_all(&d).ok();
}

/// SK4.5: 0.16.3's 8 waiting and 8 stuck go on as today: the stuck ones (due 0) looked at in the first round (A1.4), the waiting ones
/// when due; a search that finds nothing makes each waiting with a miss, none stuck; the placed one stays.
#[tokio::test]
async fn the_waiting_and_stuck_items_of_0163_go_on_as_today() {
    let _state = crate::app::references::TestState::set().await;
    let (d, _, mut k) = loaded("u45", QUEUE);
    notes_pass(&mut k);
    first_round(&mut k, REQUESTS);
    let before: Vec<Item> = k.items.clone();
    let (waiting, stuck) = (ids_in(State::Waiting), ids_in(State::Stuck));
    let sixteen: Vec<String> = waiting.iter().chain(&stuck).cloned().collect();
    // Each record's counting list, with lengths, as MusicBrainz gives it: a record of the catalogue's, its reference.
    let cat = Arc::new(crate::catalogue::Catalogue::open(None, vec![PathBuf::from("/music")]).unwrap());
    for i in k.items.iter_mut().filter(|i| sixteen.contains(&i.id)) {
        i.record.record = cat.record_for_tests(&i.record.album, 0);
        cat.set_reference(&crate::serve::ids::record(i.record.record), &crate::catalogue::records::Ref::new("musicbrainz", "release", &format!("r163-{}", i.id)), "link").unwrap();
    }
    let read = |_: crate::catalogue::records::Ref| {
        let list: Vec<Track> = [("Lamplight", 200_000), ("Morning", 180_000)].into_iter().zip(1..).map(|((title, ms), n)| Track { disc: 1, position: n, title: title.into(), length_ms: Some(ms), recording: None }).collect();
        std::future::ready(Ok::<_, anyhow::Error>(Some((list, None::<crate::catalogue::records::Ref>, crate::app::references::ReadMore::default()))))
    };
    let nothing = |_: String| std::future::ready(Ok::<Vec<Answer>, anyhow::Error>(Vec::new()));
    let mut order: Vec<String> = Vec::new();
    // The first round, now; then later, when the waiting ones are due (around the fixture's t0).
    for t in [now(), T0 + 9 * 3600] {
        loop {
            let pending = Kept { items: k.items.iter().filter(|i| sixteen.contains(&i.id) && !order.contains(&i.id)).cloned().collect(), ..Kept::default() };
            let Some(id) = next_to_search(&pending, t) else { break };
            let at = k.items.iter().position(|i| i.id == id).unwrap();
            let it = Item { id: format!("u45-{id}"), ..k.items[at].clone() };
            kept(|q| q.items.push(it.clone()));
            if read_its_list(&it.id, &it, &cat, read).await {
                search_with(&it.id, nothing).await;
            }
            let done = kept(|q| take_off(&mut q.items, |i| i.id == it.id)).pop().unwrap();
            k.items[at] = Item { id, ..done };
            order.push(k.items[at].id.clone());
        }
        if t != T0 + 9 * 3600 {
            assert!(stuck.iter().all(|id| order.contains(id)), "the stuck ones in the first round: {order:?}");
        }
    }
    assert_eq!(order.len(), 16, "every one searched once");
    for id in &sixteen {
        let (i, b) = (of_queue(&k, id), before.iter().find(|i| &i.id == id).unwrap());
        assert_eq!(i.state, State::Waiting, "{id}: {i:?}");
        assert_eq!(i.misses, b.misses + 1, "{id}: a miss");
        assert_eq!(i.why.as_deref(), Some("No folder holds every track of its release"), "{id}");
    }
    assert!(!k.items.iter().any(|i| i.state == State::Stuck), "none stuck");
    let placed = of_queue(&k, "ss-57");
    assert_eq!(placed, before.iter().find(|i| i.id == "ss-57").unwrap(), "the placed one stays");
    assert_eq!(placed.state, State::Placed);
    std::fs::remove_dir_all(&d).ok();
}

/// SK4.6 (kinds 3, 4, 6): the upgrade meets (a) a stop after five of the 20 POSTs, (b) slskd silent at first, (c) the floor, (d) its
/// asker's room, (e) a rollback an hour on. Never a record fetched twice, nor a user blamed for anything but a real failure.
#[tokio::test]
async fn the_0163_upgrade_meets_restarts_slskd_room_and_the_floor() {
    let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
    a_stop_after_five_posts().await;
    slskd_silent_at_first().await;
    below_the_floor().await;
    over_the_askers_room().await;
    a_rollback_an_hour_on();
}

/// The fixture's failed users of the approved 20 (and the two downloads), as 0.16.3 kept them: nobody blamed at the upgrade.
fn nobody_blamed(tag: &str) {
    let was = as_kept(QUEUE);
    for n in 1..=22 {
        assert_eq!(the(tag, n).failed, of_queue(&was, &format!("ss-{n}")).failed, "{tag}: ss-{n} blamed");
    }
}

/// SK4.6 (a): the server stops after five of the 20 POSTs, the sixth's id kept and not sent (SK2.3 (b)), and starts again: the five
/// followed by their batches, the sixth read back by its id and sent under it, the rest started as their users' turns come; one batch
/// each, none twice.
async fn a_stop_after_five_posts() {
    let (d, p, k) = loaded("u46a", QUEUE);
    on_list("u46a", &k);
    let slskd = slskd_of_0163(&k, &d);
    let client = slskd.client();
    let five: Vec<String> = (5..=9).map(|n| format!("u46a-ss-{n}")).collect();
    let first_five = |i: &Item| five.contains(&i.id);
    start_downloads(&first_five, &client, &d, &Look::roomy()).await;
    assert_eq!(slskd.posts().len(), 5);
    let batches: Vec<Option<String>> = (5..=9).map(|n| the("u46a", n).batch).collect();
    let sixth = crate::slskd::new_batch();
    with_item("u46a-ss-10", |i| i.batch = Some(sixth.clone()));
    // The stop: the list as last written, read by the next start.
    let prefix = "u46a-".to_string();
    let ours = ours(&prefix);
    let written = Kept { items: kept(|q| q.items.iter().filter(|i| ours(i)).cloned().collect()), notes_passed: true, ..Kept::default() };
    std::fs::write(&p, serde_json::to_vec(&written).unwrap()).unwrap();
    let again = load(&p);
    kept(|q| {
        q.items.retain(|i| !ours(i));
        q.items.extend(again.items);
    });
    follow_round(&ours, &client, &d).await;
    assert_eq!((5..=9).map(|n| the("u46a", n).batch).collect::<Vec<_>>(), batches, "the five followed by their batches");
    assert!((5..=9).all(|n| the("u46a", n).state == State::Fetching));
    every_one_taken(&ours, &slskd, &d, &Look::roomy()).await;
    assert_eq!(the("u46a", 10).batch.as_deref(), Some(sixth.as_str()), "the sixth sent under its kept id");
    for n in APPROVED {
        assert_eq!(taken_for(&slskd, "u46a", n).len(), 1, "ss-{n}: one batch, none twice");
    }
    assert_eq!(slskd.posts().iter().filter(|p| p.status == 200).count(), 20);
    nobody_blamed("u46a");
    assert!(slskd.never_overlapped() && slskd.violations().is_empty(), "{:?}", slskd.violations());
    std::fs::remove_dir_all(&d).ok();
}

/// SK4.6 (b): slskd answers nothing at first: every start and both downloads held as today (`SLSKD_AGAIN`), their batches kept,
/// nobody failed; at their time with slskd answering, all go.
async fn slskd_silent_at_first() {
    let (d, _, k) = loaded("u46b", QUEUE);
    on_list("u46b", &k);
    let prefix = "u46b-".to_string();
    let ours = ours(&prefix);
    let away = slskd_gone().await;
    start_downloads(&ours, &away, &d, &Look::roomy()).await;
    for n in APPROVED {
        let i = the("u46b", n);
        assert!(i.state == State::Approved && i.batch.is_none(), "ss-{n}: {i:?}");
        if [3, 20, 4].contains(&n) {
            // peer-01's and peer-02's: waiting for b-01 and b-02, never asked.
            assert_eq!((i.due, i.why.as_deref()), (0, None), "ss-{n}");
        } else {
            assert!(i.due > now() && i.why.as_deref().is_some_and(|w| w.starts_with("slskd could not be reached")), "ss-{n} held: {i:?}");
        }
    }
    for (n, b) in [(1, "b-01"), (2, "b-02")] {
        follow_one(&format!("u46b-ss-{n}"), &away, &d).await;
        let i = the("u46b", n);
        assert!(i.state == State::Fetching && i.batch.as_deref() == Some(b) && i.due > now(), "ss-{n} held, its batch kept: {i:?}");
    }
    nobody_blamed("u46b");
    let slskd = slskd_of_0163(&k, &d);
    every_one_taken(&ours, &slskd, &d, &Look { now: now() + SLSKD_AGAIN + 1, ..Look::roomy() }).await;
    for n in APPROVED {
        assert_eq!(taken_for(&slskd, "u46b", n).len(), 1, "ss-{n}: gone, once");
    }
    nobody_blamed("u46b");
    assert!(slskd.violations().is_empty(), "{:?}", slskd.violations());
    std::fs::remove_dir_all(&d).ok();
}

/// Six of the 20 asked for by a user (alice's, in the scenario; the fixture's request ids stand in): ss-6, ss-8 … ss-12, of peers
/// free at the first look.
const ALICES: [u32; 6] = [6, 8, 9, 10, 11, 12];

/// SK4.6 (c): below the disk floor, alice's six are held as today (AL03.2), saying so, blaming no one; the others go; at the floor's
/// next look with space again, hers go.
async fn below_the_floor() {
    let (d, _, k) = loaded("u46c", QUEUE);
    on_list("u46c", &k);
    let slskd = slskd_of_0163(&k, &d);
    let prefix = "u46c-".to_string();
    let ours = ours(&prefix);
    let hers: Vec<i64> = ALICES.iter().map(|n| the("u46c", *n).record.request).collect();
    start_downloads(&ours, &slskd.client(), &d, &Look::roomy().below_floor(&hers)).await;
    assert_eq!(slskd.posts().len(), 9, "the 15, less her six");
    for n in ALICES {
        let i = the("u46c", n);
        assert!(!slskd.post_users().contains(&i.folders[0].user), "ss-{n}: no transfer of a user's below the floor");
        assert!(i.state == State::Approved && i.batch.is_none() && i.why.as_deref() == Some(SHORT_OF_SPACE), "ss-{n}: {i:?}");
    }
    nobody_blamed("u46c");
    start_downloads(&ours, &slskd.client(), &d, &Look { now: now() + FLOOR_AGAIN + 1, ..Look::roomy() }).await;
    assert!(ALICES.iter().all(|n| the("u46c", *n).state == State::Fetching), "space again: hers go");
    assert_eq!(slskd.posts().len(), 15);
    assert!(slskd.violations().is_empty(), "{:?}", slskd.violations());
    std::fs::remove_dir_all(&d).ok();
}

/// SK4.6 (d): alice over her room: her six go, as they were ticked and open Wants go through (Q2). Room is read when a Want is handed
/// (AL02.2, `waits_for_room`) and by Cratarr's own pick (Part 3), never for a request 0.16.3 handed: the round keeps her items, and
/// they start.
async fn over_the_askers_room() {
    let (d, _, mut k) = loaded("u46d", QUEUE);
    let rs: Vec<crate::userdata::Request> = serde_json::from_slice(REQUESTS).unwrap();
    let hers: Vec<i64> = ALICES.iter().map(|n| of_queue(&k, &format!("ss-{n}")).record.request).collect();
    assert!(rs.iter().filter(|r| hers.contains(&r.id)).all(|r| !crate::app::requests::waits_for_room(r)), "handed by 0.16.3: no room's word");
    first_round(&mut k, REQUESTS);
    on_list("u46d", &k);
    let slskd = slskd_of_0163(&k, &d);
    let prefix = "u46d-".to_string();
    let ours = ours(&prefix);
    // Part 3's look carries her room: over it, her six still go (ticked), none made Cratarr's own pick.
    start_downloads(&ours, &slskd.client(), &d, &Look::roomy().full(&hers)).await;
    for n in ALICES {
        let i = the("u46d", n);
        assert!(i.state == State::Fetching && !i.auto, "ss-{n}: hers go: {i:?}");
    }
    nobody_blamed("u46d");
    std::fs::remove_dir_all(&d).ok();
}

/// SK4.6 (e): an hour on, the owner rolls back to 0.16 (`soulseek.json.016` put back, as the release notes say): 0.16.3 reads its own
/// queue as it was, byte for byte, whatever 0.17 wrote and read meanwhile.
fn a_rollback_an_hour_on() {
    let (d, p, mut k) = loaded("u46e", QUEUE);
    notes_pass(&mut k);
    first_round(&mut k, REQUESTS);
    std::fs::write(&p, serde_json::to_vec(&k).unwrap()).unwrap();
    load(&p);
    std::fs::write(&p, serde_json::to_vec(&k).unwrap()).unwrap();
    std::fs::copy(p.with_extension("json.016"), &p).unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), QUEUE, "0.16.3's queue as it left it");
    std::fs::remove_dir_all(&d).ok();
}

/// SK5.6 (OQ-SK6 (a)): at the first start, before the round's first sweep, each user set aside for notes alone is taken out of the
/// item's failed, once. The found, waiting and stuck items that had one searched again now (misses 0); the approved one only
/// unblamed; the placed one untouched; transcodes and wrong recordings stay failed; the set-asides still listed till their 14 days,
/// the sweep as before. A later start does nothing again.
#[test]
fn users_refused_for_notes_alone_are_asked_again_after_the_upgrade() {
    let _one = crate::app::switches::TEST_STATE.blocking_read(); // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
    let (d, p, mut k) = loaded("u56", QUEUE);
    // The set-asides where the data folder keeps them; some past their 14 days.
    let aside = d.join(SET_ASIDE);
    for s in k.items.iter_mut().flat_map(|i| i.set_aside.iter_mut()) {
        let name = Path::new(&s.path).file_name().unwrap().to_owned();
        s.path = aside.join(&name).display().to_string();
        std::fs::create_dir_all(aside.join(&name)).unwrap();
        std::fs::write(aside.join(&name).join("why.json"), serde_json::to_vec(&s.why).unwrap()).unwrap();
    }
    let old: Vec<PathBuf> = ["ss-23 peer-a", "ss-23 peer-b", "ss-23 peer-c", "ss-43 peer-d", "ss-57 peer-c"].iter().map(|n| aside.join(n)).collect();
    let fifteen_days_ago = std::time::SystemTime::now() - Duration::from_secs(15 * 86_400);
    for o in &old {
        std::fs::File::options().write(true).open(o.join("why.json")).unwrap().set_modified(fifteen_days_ago).unwrap();
    }
    let before: Vec<Item> = k.items.clone();
    let was = |id: &str| before.iter().find(|i| i.id == id).unwrap().clone();
    assert!(notes_pass(&mut k), "at the first start");
    assert!(k.notes_passed, "marked in the queue");
    // The five whose every set-aside is notes-only, and the three mixed: searched again now; only the transcodes and the wrong
    // recordings stay failed.
    for (id, stay) in [
        ("ss-23", vec![]),
        ("ss-24", vec![]),
        ("ss-41", vec![]),
        ("ss-42", vec![]),
        ("ss-49", vec![]),
        ("ss-25", vec!["peer-h", "peer-a"]),
        ("ss-43", vec!["peer-d", "peer-e"]),
        ("ss-50", vec!["peer-g", "peer-h", "peer-a"]),
    ] {
        let i = of_queue(&k, id);
        assert_eq!(i.failed, stay, "{id}");
        assert_eq!((i.state, i.due, i.misses, i.why.as_deref(), i.chosen), (State::Waiting, 0, 0, None, None), "{id}: searched again now");
        assert!(i.folders.is_empty() && i.tracks.is_empty() && i.untimed.is_empty(), "{id}: as Search now: {i:?}");
        assert_eq!(i.set_aside, was(id).set_aside, "{id}: its set-asides still listed");
    }
    // The approved one: only unblamed, its download as it is.
    let (a, w) = (of_queue(&k, "ss-3"), was("ss-3"));
    assert!(a.failed.is_empty(), "{a:?}");
    assert_eq!(Item { failed: Vec::new(), ..w.clone() }, *a, "nothing else of it changed");
    assert_eq!((a.state, a.chosen), (State::Approved, Some(0)));
    // The placed one untouched, and every other item as it was.
    let touched = ["ss-23", "ss-24", "ss-41", "ss-42", "ss-49", "ss-25", "ss-43", "ss-50", "ss-3"];
    for b in before.iter().filter(|b| !touched.contains(&b.id.as_str())) {
        assert_eq!(of_queue(&k, &b.id), b, "{}: untouched", b.id);
    }
    assert_eq!(of_queue(&k, "ss-57").failed, ["peer-c"], "the placed one untouched");
    // The sweep as before, after the pass: the folders past their 14 days deleted and taken off their items; the users it unblamed
    // first stay unblamed.
    on_list("u56", &k);
    sweep_set_asides(&d, now());
    assert!(old.iter().all(|o| !o.exists()), "past their 14 days: deleted");
    assert!(the("u56", 23).set_aside.is_empty() && the("u56", 23).failed.is_empty());
    assert_eq!(the("u56", 24).set_aside, was("ss-24").set_aside, "the others kept");
    assert_eq!(the("u56", 57).set_aside.len(), 0, "the placed one's swept as ever");
    // Once: a later start does nothing again. peer-d failed ss-24 since, offline, beside its notes-only set-aside: it stays failed.
    assert!(!notes_pass(&mut k), "once");
    let i = k.items.iter_mut().find(|i| i.id == "ss-24").unwrap();
    on_failed(i, "peer-d", "peer-d is offline".into());
    std::fs::write(&p, serde_json::to_vec(&k).unwrap()).unwrap();
    let mut later = load(&p);
    assert!(!notes_pass(&mut later), "marked in soulseek.json");
    assert_eq!(of_queue(&later, "ss-24").failed, ["peer-d"], "a user failed since stays failed");
    std::fs::remove_dir_all(&d).ok();
}
