//! OQ-RN-1's measurements (RN, RL5): each decoding job kind's peak of resident memory with the tests' stand-ins, on a five-minute
//! CD-quality FLAC (the library's common case; a decode holds the whole file's samples). Linux only (`/proc/self`): the peak reset
//! (`clear_refs` 5) before each kind, read (`VmHWM`) after. Run alone, by name, in the e2e stage's container:
//! `rt e2e measure_each_kinds_peak`. Its numbers go to RN-REPORT.md; the owner chooses the budget's reading from them.

/// Resident memory now and its peak since the last reset, MB.
fn resident() -> (u64, u64) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let kb = |key: &str| status.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok()).unwrap_or(0);
    (kb("VmRSS:") / 1024, kb("VmHWM:") / 1024)
}

/// Each kind's work, its peak over the resident memory before it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "OQ-RN-1's measurement: run by name, alone"]
async fn measure_each_kinds_peak() {
    let d = std::env::temp_dir().join(format!("cratarr-peaks-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let wav = crate::analysis::tests::wav("peaks", 300.0, |t| 0.2 * (t * (440.0 + 200.0 * (t * 0.7).sin()) * std::f64::consts::TAU).sin() as f32);
    let flac = d.join("five-minutes.flac");
    crate::convert::to_cd_flac(&wav, &flac).unwrap();
    std::fs::remove_file(&wav).ok();
    let track = super::track_fetch::track_for(1, 1, "Five Minutes", "Glass Harbour", Some(300_000), "rec-peaks");
    let mut said = Vec::new();
    let mut measure = |kind: &str, run: &mut dyn FnMut()| {
        let _ = std::fs::write("/proc/self/clear_refs", "5");
        let (before, _) = resident();
        run();
        let (_, peak) = resident();
        said.push(format!("{kind}: peak {peak} MB, {before} MB before, +{} MB", peak.saturating_sub(before)));
    };
    measure("analysis (the pass: analyse)", &mut || drop(crate::analysis::analyse(&flac, &[]).unwrap()));
    measure("analysis (signals, a page's visuals)", &mut || drop(crate::analysis::analyse_with_signals(&flac, &[], None).unwrap()));
    measure("analysis (band waves)", &mut || drop(crate::analysis::band_wave::work_out(&flac).unwrap()));
    let _ = std::fs::write("/proc/self/clear_refs", "5");
    let (before, _) = resident();
    let lookup = |_| async { Ok::<Vec<crate::acoustid::Found>, anyhow::Error>(Vec::new()) };
    let _ = super::track_fetch::gate_track_with(&flac, &track, lookup).await;
    let (_, peak) = resident();
    said.push(format!("check (Soulseek's and the intake's gate: decode and print): peak {peak} MB, {before} MB before, +{} MB", peak.saturating_sub(before)));
    for line in &said {
        println!("OQ-RN-1 {line}");
    }
    std::fs::remove_dir_all(&d).ok();
    assert!(said.len() == 4);
}
