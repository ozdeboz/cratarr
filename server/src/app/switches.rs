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

static KNOWN: Mutex<Option<HashMap<&'static str, bool>>> = Mutex::new(None);

fn key(name: &str) -> String {
    format!("off:{name}")
}

/// Whether a connection is on (on unless switched off; on where nothing can be read).
pub fn is_on(name: &'static str) -> bool {
    if let Some(on) = KNOWN.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|k| k.get(name)) {
        return *on;
    }
    let on = crate::serve::instance().is_none_or(|s| s.userdata.import_choice(&key(name)).ok().flatten().is_none_or(|c| c != "1"));
    KNOWN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(name, on);
    on
}

/// A connection switched on or off. Blocking.
pub fn set_on(name: &'static str, on: bool) -> Result<()> {
    let serve = crate::serve::instance().ok_or_else(|| anyhow::anyhow!("Cratarr's own library is not open"))?;
    serve.userdata.set_import_choice(&key(name), if on { "" } else { "1" })?;
    KNOWN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(name, on);
    Ok(())
}
