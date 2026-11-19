//! Connections switched off (0.9.6, the owner: every connection can be, as servers and Lidarr already can): the
//! server's Last.fm key, Discogs and MusicBrainz. Off, Cratarr does not use it at all; its key is kept, and what it
//! already brought stays. Kept as server choices (`off:<name>`), and in memory once read: they are asked at every call.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;

pub const LASTFM: &str = "lastfm";
pub const DISCOGS: &str = "discogs";
pub const MUSICBRAINZ: &str = "musicbrainz";
pub const SLSKD: &str = "slskd";
/// Bandcamp (O4): its pages, its search box, its streams.
pub const BANDCAMP: &str = "bandcamp";
/// Cratarr's reading of outside services on its own, in the background (O5, "Pause background reading"): MusicBrainz's lists, years
/// and works, harvest, Discogs' passes, artist info, Spotify's looks, Soulseek's list reads and its gate's AcoustID (Y7). Off, none
/// asks anything until it is on again, a pass stopping at its next question or album; what a page asks for still goes.
pub const BACKGROUND: &str = "background";

/// Whether an administrator paused the background reading (`BACKGROUND` off).
pub fn background_paused() -> bool {
    !is_on(BACKGROUND)
}

/// What a background read refused while background reading is paused says (Y7).
pub const PAUSED: &str = "Background reading is paused";

static KNOWN: Mutex<Option<HashMap<&'static str, bool>>> = Mutex::new(None);

fn key(name: &str) -> String {
    format!("off:{name}")
}

/// Whether a connection is on (on unless switched off; on where nothing can be read).
pub fn is_on(name: &'static str) -> bool {
    if let Some(on) = KNOWN.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|k| k.get(name)) {
        return *on;
    }
    let on = crate::serve::view::get().is_none_or(|s| s.userdata().import_choice(&key(name)).ok().flatten().is_none_or(|c| c != "1"));
    KNOWN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(name, on);
    on
}

/// Tests: a connection taken as switched on or off, nothing written; what it was, for the test to put back (Y12). Under
/// `TEST_STATE`.
#[cfg(test)]
pub fn set_for_tests(name: &'static str, on: bool) -> bool {
    let was = is_on(name);
    KNOWN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(name, on);
    was
}

/// Tests: the one lock for the state the whole process shares that some tests change (Y12): the switches, Bandcamp's and Discogs'
/// holds, a fetcher said down. A test that changes one, or reads one another test changes, takes it, and puts back what was there.
#[cfg(test)]
pub static TEST_STATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A connection switched on or off. Blocking.
pub fn set_on(name: &'static str, on: bool) -> Result<()> {
    let serve = crate::serve::view::get().ok_or_else(|| anyhow::anyhow!("Cratarr's own library is not open"))?;
    serve.userdata().set_import_choice(&key(name), if on { "" } else { "1" })?;
    KNOWN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(name, on);
    Ok(())
}
