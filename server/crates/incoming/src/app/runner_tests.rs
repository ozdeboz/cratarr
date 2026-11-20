//! RN's tests of Incoming's work on the runner: a copy's count (TU2) and the analysis pass's row (AC2).

use std::time::Duration;

use crate::runner::{Priority, Runner};

fn fresh() -> &'static Runner {
    Box::leak(Box::new(Runner::new()))
}

/// The words a row says, its times as the page's `when()` would (the tests' clock: "14:05").
fn words(r: &crate::runner::Row) -> String {
    r.words(&|_| "14:05".to_string())
}

/// TU2 (RA8): Copy to of a 24-file album, a byte copy: its step says "1 of 24" … "24 of 24" as each file lands, the last told
/// before the job ends. Never "0 of 24" until the end, a count past its total, nor one going back.
#[test]
fn a_copy_counts_its_files_as_they_land() {
    let r = fresh();
    let d = std::env::temp_dir().join(format!("cratarr-tu2-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    let (from, to) = (d.join("from"), d.join("to"));
    std::fs::create_dir_all(&from).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    let plan: Vec<crate::moves::Step> = (1..=24)
        .map(|n| {
            let name = format!("{n:02} Track.flac");
            std::fs::write(from.join(&name), vec![n as u8; 4096]).unwrap();
            crate::moves::Step { from: (from.clone(), name.clone()), to: (to.clone(), format!("Album/{name}")) }
        })
        .collect();
    let job = r.begin("copy", "alice", false).unwrap();
    job.step("Copying Night Ferry", 0, Some(24));
    let stepper = job.stepper();
    let seen = std::sync::Mutex::new(Vec::new());
    crate::moves::copy_told(&plan, &|n| {
        stepper.step("Copying Night Ferry", n as u64, Some(24));
        let row = r.rows().into_iter().find(|x| x.kind == "copy").unwrap();
        seen.lock().unwrap().push((row.done, row.of));
    })
    .unwrap();
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen, (1..=24).map(|n| (Some(n), Some(24))).collect::<Vec<_>>(), "each file counted as it lands, the last before the end");
    assert_eq!(words(&r.rows().into_iter().find(|x| x.kind == "copy").unwrap()), "Copying Night Ferry · 24 of 24");
    job.finish(Ok::<_, anyhow::Error>(()));
    assert!(r.rows().is_empty(), "ended: no row");
    std::fs::remove_dir_all(&d).ok();
}

/// AC2: the pass's row says its work, its file and why: (a) "Band waves · Glass Harbour/Night Ferry/03 Neap.flac · catching up ·
/// 1,204 of 3,580"; (b) a page's waveform "Waveform · …/03 Neap.flac · asked for" while it waits for its turn; (c) "Beats · <file>";
/// (d) resting, the row staying; (e) "Waits for memory"; (f) switched off, no row. Never AudioMuse's count in the pass's row.
#[tokio::test(start_paused = true)]
async fn the_pass_says_its_file_its_work_and_why() {
    use super::analysis_pass::{KIND, asked_for, resting, working};
    let r = fresh();
    let rel = "Glass Harbour/Night Ferry/03 Neap.flac";
    let job = r.begin_keyed(KIND, KIND, None).unwrap();
    let row = || r.rows().into_iter().find(|x| x.kind == KIND).map(|x| words(&x));
    // (a)
    working(Some(&job), "Band waves", rel, true, Some((1_204, 3_580)));
    assert_eq!(row().as_deref(), Some("Band waves · Glass Harbour/Night Ferry/03 Neap.flac · catching up · 1,204 of 3,580"));
    // (b) A page's waveform, asked for while a decode holds the turn: seen waiting, with no job of its own.
    static ONE: crate::runner::Resource = crate::runner::Resource::new("decode", "decode", 1);
    let held = ONE.take(Priority::Background).await;
    let page = tokio::spawn(ONE.take_within(Priority::Asked, Duration::from_secs(20), Some(asked_for("Waveform", rel))));
    tokio::time::sleep(Duration::from_millis(10)).await;
    let waiting: Vec<String> = ONE.rows().iter().map(words).collect();
    assert_eq!(waiting, ["Waveform · Glass Harbour/Night Ferry/03 Neap.flac · asked for · Waits for its turn to decode"]);
    drop(held);
    let turn = page.await.unwrap().expect("its turn");
    assert_eq!(ONE.rows().iter().map(words).collect::<Vec<_>>(), ["Waveform · Glass Harbour/Night Ferry/03 Neap.flac · asked for"], "decoding");
    drop(turn);
    // (c)
    working(Some(&job), "Beats", rel, false, None);
    assert_eq!(row().as_deref(), Some("Beats · Glass Harbour/Night Ferry/03 Neap.flac"));
    // (d)
    resting(Some(&job));
    assert_eq!(row().as_deref(), Some("Resting"), "it runs, and goes on");
    // (e)
    {
    crate::memwatch::FULL.set(true);
    let mem: &'static crate::runner::resource::Memory = Box::leak(Box::new(crate::runner::resource::Memory::new()));
    crate::runner::resource::rule_for_tests(Some(crate::runner::resource::Rule::Room));
    let waits = mem.fits(KIND, Some(&job));
    tokio::pin!(waits);
    assert!(tokio::time::timeout(Duration::from_secs(1), &mut waits).await.is_err());
    assert!(row().is_some_and(|w| w.ends_with("Waits for memory")), "{:?}", row());
    crate::memwatch::FULL.set(false);
    drop(tokio::time::timeout(Duration::from_secs(10), &mut waits).await.expect("room again"));
    crate::runner::resource::rule_for_tests(None);
    }
    assert!(!row().unwrap_or_default().contains("tracks analysed"), "never AudioMuse's count");
    // (f) Switched off: the pass ends its job, and no row is left.
    job.finish(Ok::<_, anyhow::Error>(()));
    assert_eq!(row(), None);
}
