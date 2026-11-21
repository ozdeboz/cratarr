//! Health › Activity (RN item 6; SCENARIOS-RN AC1, AC5, AC6): every job from the runner alone, with what it does now, how far and
//! whose; AudioMuse's row said to be AudioMuse's.

use std::sync::Arc;
use std::time::Duration;

use crate::runner::{Priority, Resource, Row, Runner, State};

fn fresh() -> &'static Runner {
    Box::leak(Box::new(Runner::new()))
}

fn words(r: &Row) -> String {
    r.words(&|_| "14:05".to_string())
}

/// AC1: running at once: the pass on a file's band waves (catching up), a Soulseek check of "Tide Tables" at 4 of 9, alice's
/// Last.fm import at page 120 of 400, Discogs' typing paused until 14:05, root's Renumber of `al-42` at 3 of 10 files, and a decode
/// waiting for its turn. One row per job, its words from the table, whose (alice's, or Cratarr's own), what a waiting job waits for
/// and until when; the rows come from the runner and follow the live channel, a step's count at most four times a second. Never a
/// running job missing, an ended job shown as running, nor the same work in two rows.
#[tokio::test(start_paused = true)]
async fn activity_lists_every_job_with_what_it_does_and_how_far() {
    static ONE: Resource = Resource::new("decode", "decode", 1);
    let r = fresh();
    let mut told = r.subscribe_activity();
    let pass = Arc::new(r.begin_keyed("analysis", "analysis", None).unwrap());
    pass.detail(Some("Glass Harbour/Night Ferry/03 Neap.flac · catching up".into()), None);
    pass.step("Band waves", 1_204, Some(3_580));
    let check = r.begin_keyed("soulseek:check", "ss-11", None).unwrap();
    check.detail(None, Some("files"));
    check.step("Checking Glass Harbour – Tide Tables", 4, Some(9));
    let import = r.begin("import:lastfm", "alice", false).unwrap();
    import.step("Reading scrobbles", 120, Some(400));
    let typing = r.begin_keyed("discogs:type", "discogs:type", None).unwrap();
    typing.step("Typing records", 30, Some(300));
    typing.wait_for("", Some(1_800_000_000));
    let renumber = r.begin("renumber:al-42", "root", false).unwrap();
    renumber.detail(None, Some("files"));
    renumber.step("Renumbering Night Ferry", 3, Some(10));
    let held = ONE.take(Priority::Background).await;
    let (p, waiter) = (pass.clone(), tokio::spawn(async move { ONE.take(Priority::Background).await }));
    let decode = tokio::spawn(async move { drop(ONE.take_for(Priority::Background, Some(p.as_ref()), None).await) });
    tokio::time::sleep(Duration::from_millis(10)).await;
    let rows = r.rows();
    let said: Vec<(String, Option<String>)> = rows.iter().map(|x| (words(x), x.whose.clone())).collect();
    assert_eq!(said, [
        ("Band waves · Glass Harbour/Night Ferry/03 Neap.flac · catching up · 1,204 of 3,580 · Waits for its turn to decode".to_string(), None),
        ("Checking Glass Harbour – Tide Tables · 4 of 9 files".to_string(), None),
        ("Reading scrobbles · 120 of 400".to_string(), Some("alice".to_string())),
        ("Typing records · 30 of 300 · Paused until 14:05".to_string(), None),
        ("Renumbering Night Ferry · 3 of 10 files".to_string(), Some("root".to_string())),
    ], "one row per job, the pass once though it waits again");
    assert_eq!(rows.iter().filter(|x| x.state == State::Waiting).count(), 2, "the decode waiting, Discogs paused");
    drop(held);
    drop(waiter.await.unwrap());
    decode.await.unwrap();
    // The live channel: a patch per change; a step's count at most four a second.
    while told.try_recv().is_ok() {}
    for n in 0..100 {
        import.step("Reading scrobbles", 121 + n, Some(400));
    }
    let mut patches = 0;
    while let Ok(row) = told.try_recv() {
        patches += usize::from(row.kind == "import:lastfm");
    }
    assert!(patches <= 2, "a hundred steps at once told {patches} times");
    // An ended job leaves, at its end event.
    renumber.finish(Ok::<_, anyhow::Error>(()));
    let mut ended = false;
    while let Ok(row) = told.try_recv() {
        ended |= row.kind.starts_with("renumber") && row.finished.is_some();
    }
    assert!(ended, "its end told");
    assert!(r.rows().iter().all(|x| !x.kind.starts_with("renumber")), "no ended job shown");
    drop((check, import, typing, pass));
}

/// AC6 (kind 3): Activity open on alice's import and root's Renumber. The Renumber ends: its row leaves. alice's account is deleted:
/// her import is asked to stop with it, ends, and its row leaves; the pruner later takes both rows. Never a row stuck running, nor a
/// job of a deleted account going on.
#[tokio::test]
async fn activity_follows_jobs_that_end_or_lose_their_account() {
    let r = fresh();
    let journal = Arc::new(crate::journal::Journal::open(None).unwrap());
    r.attach(journal.clone()).unwrap();
    let import = r.begin("import:lastfm", "alice", false).unwrap();
    let renumber = r.begin("renumber:al-42", "root", false).unwrap();
    renumber.finish(Ok::<_, anyhow::Error>(()));
    assert_eq!(r.rows().iter().map(|x| x.kind.as_str()).collect::<Vec<_>>(), ["import:lastfm"], "the Renumber's row left");
    // Her account goes: her work, which looks before each step, ends.
    assert_eq!(r.stop_account("alice"), 1);
    assert!(import.stopped(), "asked to stop");
    import.finish(Ok::<_, anyhow::Error>(()));
    assert!(r.rows().is_empty(), "no row stuck running");
    let now = crate::app::moment::now_secs();
    assert_eq!(journal.jobs().unwrap().len(), 2, "kept their hour");
    r.prune(now + 2 * 3600).unwrap();
    assert!(journal.jobs().unwrap().is_empty(), "the pruner takes both");
}

/// AC5: AudioMuse's row reads "31,639 of 34,790 tracks analysed", said to be AudioMuse's, imported from the PC: no job of Cratarr's,
/// nothing of its pass. Never that count shown as Cratarr's own analysis.
#[test]
fn audiomuses_row_is_audiomuses() {
    use crate::app::health_findings::label_of;
    assert_eq!(crate::app::health_base::name_of("analysis:audiomuse"), "AudioMuse");
    assert_eq!(label_of("analysis:audiomuse"), Some(("AudioMuse", "to do")));
    assert!(super::health::AUDIOMUSE.contains("AudioMuse's analysis, imported from the PC"));
    let r = fresh();
    let pass = r.begin_keyed("analysis", "analysis", None).unwrap();
    pass.step("Band waves", 1, Some(2));
    assert!(r.rows().iter().all(|x| !words(x).contains("tracks analysed")), "the pass's row is the pass's");
    drop(pass);
}
