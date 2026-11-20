//! What the crates read of an account's Radio settings and plays, moved down from radio.rs (and the panel's genre count from
//! playing.rs) so the collections name nothing of playing (SP3-U): an account's Similar by and More music choice, what Radio
//! leaves for later. Radio and playing re-export them where they used them.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use rusqlite::{Connection, params};

use crate::similarity::Signal;

/// The most genres the panel shows on top of an album or a crate.
pub const GENRES_SHOWN: usize = 5;

/// An account's own "Similar by" (its settings' `similar_by`: signal names, the first counting most), when it set one;
/// names not known are left out. A player Cratarr drives (Play On, with nobody asking) takes its account's.
pub fn similar_by_of(user: &str) -> Option<Vec<Signal>> {
    let served = crate::serve::instance()?;
    let (_, settings) = served.userdata.settings_of(user).ok()?;
    similar_by_in(&settings)
}

/// An account's More music choice (0.9.5, its settings' `more_music`): the libraries Radio and Autoplay choose from
/// (by Cratarr's library ids, its folders; none: all) and whether Discover's Bandcamp suggestions join them.
pub fn more_music_of(user: &str) -> (Option<Vec<String>>, bool) {
    match crate::serve::instance().and_then(|s| s.userdata.settings_of(user).ok()) {
        Some((_, settings)) => more_music_in(&settings),
        None => (None, false),
    }
}

pub(super) fn more_music_in(settings: &serde_json::Value) -> (Option<Vec<String>>, bool) {
    let m = &settings["more_music"];
    (m["libraries"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()), m["bandcamp"].as_bool().unwrap_or(false))
}

/// "Similar by" as an account's settings hold it; none set, or none known: none.
pub(super) fn similar_by_in(settings: &serde_json::Value) -> Option<Vec<Signal>> {
    let names = settings.get("similar_by")?.as_array()?.clone();
    Some(names.into_iter().filter_map(|n| serde_json::from_value(n).ok()).collect()).filter(|v: &Vec<Signal>| !v.is_empty())
}

/// Played within this long: left out while there is enough else (the owner, 2026-10-16).
const RECENT: Duration = Duration::from_secs(7 * 24 * 3600);

pub(super) fn read_only(file: &Path) -> Result<Connection> {
    let conn = crate::db::open_read_only(file)?;
    conn.busy_timeout(Duration::from_secs(10))?;
    Ok(conn)
}

/// Songs played within seven days as one's own, with when (epoch seconds): what was heard while following another
/// is not kept from the radio (the owner, 2026-10-30).
pub(super) fn recent_plays(userdata: &Path, owner: &str) -> Result<HashMap<String, i64>> {
    let conn = read_only(userdata)?;
    let since = super::moment::now_secs() - RECENT.as_secs() as i64;
    let mut st = conn.prepare("SELECT item, own_date FROM annotation WHERE user = ?1 AND kind = 'song' AND own_date >= ?2")?;
    let rows = st.query_map(params![owner, since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// What Radio leaves for later of `user`'s, as `ready` reads it: nothing while its play history is off.
pub(super) fn recent_of(serve: &crate::serve::Serve, user: &str) -> Result<HashMap<String, i64>> {
    let Some(dir) = &serve.data_dir else { return Ok(HashMap::new()) };
    if serve.userdata.tracking(user) { recent_plays(&dir.join("userdata.sqlite"), user) } else { Ok(HashMap::new()) }
}

/// The folders of the libraries `user` lets Radio and Autoplay choose from; none: every one.
pub(super) fn folders_of(serve: &crate::serve::Serve, user: &str) -> Option<Vec<i64>> {
    let libraries = more_music_of(user).0?;
    Some(serve.library.roots().into_iter().filter(|(_, r)| libraries.iter().any(|l| Path::new(l) == Path::new(r))).map(|(f, _)| f).collect())
}
