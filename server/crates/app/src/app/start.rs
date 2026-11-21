//! The server's start (SCENARIOS-LOOKUPS LK.S1; the coordinator's R-N1/N2, R-N3): the one function that begins every loop a moved
//! lookup hangs on, called once by `main` after the boot's setups (Spotify's store, Soulseek's state, Discogs' `sources` provider stay
//! in the boot). A test's boot calls none of it, so no loop runs there (LK.0's harness 3).
//!
//! The schedules on the runner (`Runner::every`, `schedules()`): the records' due read (RV12 (d)), Soulseek's round (LK.B4), Spotify's
//! look (LK.E1). The loops still bare, each moving onto the shape in its own run (LK.S1's table): the artist builder, originals,
//! Discogs' round, harvest and retention's paced re-read. The arrival's job has no period: the record pass's watcher begins it when it
//! finds an arrival (F53.7).

use std::sync::Arc;

use super::App;

/// Every loop that asks an outside source, begun (LK.S1). Once a server: a schedule refuses a second of its kind.
pub fn loops(app: &Arc<App>) {
    // The records' due read: one record a tick, no Bandcamp (RV12 (d), lane A's).
    if let Err(e) = super::references::start_due_reads() {
        tracing::warn!(target: "library", "the records' due read: {e}");
    }
    // Soulseek in Cratarr (0.15): its round a schedule on the runner (LK.B4).
    super::soulseek::start(app);
    // Spotify's lists read again every 6 hours, MusicBrainz asked about them (0073): its look a schedule (LK.E1).
    super::spotify::start();
    // The arrivals the record pass finds: each one's matching and Discogs' search, its job (F53.7, LK.B1).
    if let Some(served) = &app.served {
        super::arrival_job::watch(served.clone());
        // Records not had that nothing keeps, forgotten once a day (RT01), and its paced re-read of the library artists' lists:
        // after Soulseek's list is read, whose items keep theirs.
        super::retention::start(served.clone());
    }
    // The artist builder: library artists' facts and photos from TheAudioDB (DC3 moves it onto the runner).
    if let Some(a) = &app.artists {
        a.build_slowly();
    }
    // MusicBrainz' years and types, the records' lists, the works (DC3's).
    super::originals::start();
    // Discogs' round, 6-hourly (DC3's).
    super::discogs::start();
    // The library's words: MusicBrainz, Discogs and Last.fm asked in the background (lane C's GEN).
    super::harvest::start();
}

/// What `loops` begins, each by the call that begins it, in its order: LK.S1's every-schedule test reads this list against the
/// function's own text, so a call taken out of it is named.
pub const BEGINS: [&str; 9] = [
    "references::start_due_reads()",
    "soulseek::start(app)",
    "spotify::start()",
    "arrival_job::watch(",
    "retention::start(",
    "a.build_slowly()",
    "originals::start()",
    "discogs::start()",
    "harvest::start()",
];
