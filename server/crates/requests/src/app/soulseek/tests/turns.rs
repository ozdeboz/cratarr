//! Whose turn it is, as round 1.3's review of parts 1 and 2 found it: a Found record Cratarr's pick holds for its asker's room or the
//! server's space reads waiting in those words (the room and space arm beside M3's waiting line, which `no_two_records_from_one_user_at_once`
//! asserts); a search slskd keeps busy past its wait (L3); a batch let go that slskd could not be asked for (L4).

use super::*;

/// Found, and held for its asker's room or the server's space (SK3.5: Cratarr's pick sets those words on a Found item): it reads
/// waiting, in those words exactly, as the item alone and as its request reads it from the list. Any other Found item reads Found.
#[test]
fn a_found_record_held_for_room_or_space_reads_waiting_in_those_words() {
    for words in [crate::app::requests::WAITS_FOR_ROOM, crate::app::requests::WAITS_FOR_SPACE] {
        let i = Item { why: Some(words.into()), ..found_twice("ss-turns-room") };
        assert_eq!(word_of(&i), Word::Waiting { why: Some(words.to_string()) });
        assert_eq!(word_in(std::slice::from_ref(&i), &i), Word::Waiting { why: Some(words.to_string()) }, "as its request reads it");
    }
    assert_eq!(word_of(&found_twice("ss-turns-room")), Word::Found);
    assert_eq!(word_of(&Item { why: Some("peer-a is offline".into()), ..found_twice("ss-turns-room") }), Word::Found, "any other why");
}

/// The review of part 1, L3: slskd answering a search's answers 429 past the two minutes they are read again (a proxy before it):
/// the record waits a round, as with slskd away. No miss, no five-minute or longer wait of a miss, no 429 in its why.
#[tokio::test]
async fn a_search_slskd_keeps_busy_past_its_wait_is_no_miss() {
    let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
    let slskd = Scripted::new().answers(Vec::new()).responses_busy(1_000);
    waiting_with("ss-turns-l3", vec![track(1, "One", 100), track(2, "Two", 200)], Vec::new());
    with_item("ss-turns-l3", |i| i.misses = 2);
    let t = now();
    search_through("ss-turns-l3", &slskd.client()).await;
    let i = item("ss-turns-l3").unwrap();
    assert_eq!((i.state, i.misses), (State::Waiting, 2), "no miss: {i:?}");
    assert!((i.due - t - ROUND_AT_LEAST).abs() <= 5, "a round later, as with slskd away, not a miss's day: {}", i.due - t);
    let why = i.why.clone().unwrap_or_default();
    assert!(!why.contains("429") && !why.contains("Too Many Requests"), "{why}");
    assert!(slskd.violations().is_empty(), "{:?}", slskd.violations());
}

/// The review of part 1, L4: a download given up (queued at its user past its time) whose batch slskd could not be read as it was let
/// go is kept to drop, and its user's turn stays taken: the user's next record waits, never a second live batch of one user. The next
/// look slskd answers drops it, and that record goes.
#[tokio::test]
async fn a_batch_let_go_unread_keeps_its_users_turn_till_it_is_dropped() {
    let _one = crate::app::switches::TEST_STATE.read().await; // reads now(), the turns' clock (`moment::PINNED`), which TN's tests move
    let d = data_folder("turns-l4");
    let slskd = Scripted::new().hold_all(0.0);
    picked("ss-turns-l4-1", &["peer-a", "peer-b"]);
    picked("ss-turns-l4-3", &["peer-a"]);
    let (ours, client, look) = (ours("ss-turns-l4-"), slskd.client(), Look::roomy());
    start_downloads(&ours, &client, &d, &look).await;
    assert_eq!(slskd.post_users(), ["peer-a"]);
    let b1 = batch_of("ss-turns-l4-1");
    // Queued at peer-a past its time: given up. Its read answered, the read as it is let go answered 429, and the next look's too.
    slskd.then_reads_busy(1, 2);
    // Followed past its time, however long the machine has been up (its time given, not read off the clock).
    assert!(matches!(follow_step("ss-turns-l4-1", &client, &d, QUEUED_FOR + Duration::from_secs(60)).await, Followed::Over));
    assert_eq!(item("ss-turns-l4-1").unwrap().failed, ["peer-a"], "given up on peer-a");
    assert!(!slskd.deleted(&b1), "not dropped: slskd could not be read");
    start_downloads(&ours, &client, &d, &look).await;
    assert_eq!(slskd.post_users(), ["peer-a", "peer-b"], "ss-1 to peer-b; ss-3 waits till peer-a's batch is dropped");
    assert_eq!(item("ss-turns-l4-3").unwrap().state, State::Approved);
    // slskd answers: the batch dropped, then peer-a's turn is free.
    start_downloads(&ours, &client, &d, &look).await;
    assert!(slskd.deleted(&b1), "dropped at the next look");
    assert_eq!(slskd.post_users(), ["peer-a", "peer-b", "peer-a"]);
    assert_eq!(item("ss-turns-l4-3").unwrap().state, State::Fetching);
    assert!(item("ss-turns-l4-3").unwrap().failed.is_empty(), "nobody failed for the wait");
    assert!(slskd.violations().is_empty(), "{:?}", slskd.violations());
    std::fs::remove_dir_all(&d).ok();
}
