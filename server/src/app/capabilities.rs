//! What this install can do (0.11, decision 0085, step 1): one list the page asks once a load, instead of comparing
//! versions or trying a call for its 404. Two kinds of name: what this build's code can do (`CODE`, the same on every
//! install of it), and what is set up and switched on here (`Configured`; not whether it answers now). Names only,
//! present only when true: never a value, an address or a key. Behind the sign-in, not on the open `/api/health`, which
//! must not say how the install is set up.

use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;
use ts_rs::TS;

use super::{App, discover, switches, track_fetch};

/// What this build's code can do, whatever is set up: each one a feature the page asked a call's 404 or 405 about.
pub const CODE: &[&str] = &[
    "acoustid_settings",
    "bandcamp_artist",
    // Several playlists, albums or tracks in one call (0.9.4).
    "batches",
    "beyond_search",
    "classical",
    "cover_look",
    "cover_stamps",
    "cover_upload",
    "crate_fits",
    "describe",
    "discogs_settings",
    "exclude_artists",
    // Download…: one file as itself, several as one zip.
    "export_single",
    "follow",
    "intake_check",
    "musicbrainz_settings",
    "offline",
    "other_versions",
    "queue_requests",
    // Quick crates (0.11): Sounds like an artist, the closeness cutoff, the label and album axes.
    "quick_crates",
    "renumber",
    "server_restart",
    "server_runtime",
    "slskd_settings",
    "spectrum",
    "track_files",
];

/// What is set up and switched on here, as `read` finds it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Configured {
    /// Signing in is required (and there is an account).
    pub sign_in: bool,
    /// The directory (LDAP) is switched on, its settings checked.
    pub directory_sign_in: bool,
    /// The guest portal is on, whoever asks.
    pub guest: bool,
    pub lidarr: bool,
    /// slskd set up and switched on, and an AcoustID key: a missing track looked for.
    pub track_search: bool,
    /// slskd's folder saved and an AcoustID key: files already on the server taken in.
    pub track_intake: bool,
    pub musicbrainz: bool,
    pub discogs: bool,
    /// The server's own Last.fm key, switched on.
    pub lastfm: bool,
    /// The sync play server listens.
    pub sync_play: bool,
    pub jellyfin: bool,
    pub subsonic: bool,
    /// Some enabled server has an Incoming library.
    pub incoming: bool,
    pub opus_copies: bool,
    /// Spotify's store is open (it needs the data folder).
    pub spotify: bool,
}

/// The names, `CODE`'s and those configured that are on, sorted.
pub fn names(on: &Configured) -> Vec<&'static str> {
    let configured = [
        ("sign_in", on.sign_in),
        ("directory_sign_in", on.directory_sign_in),
        ("guest", on.guest),
        ("lidarr", on.lidarr),
        ("track_search", on.track_search),
        ("track_intake", on.track_intake),
        ("musicbrainz", on.musicbrainz),
        ("discogs", on.discogs),
        ("lastfm", on.lastfm),
        ("sync_play", on.sync_play),
        ("jellyfin", on.jellyfin),
        ("subsonic", on.subsonic),
        ("incoming", on.incoming),
        ("opus_copies", on.opus_copies),
        ("spotify", on.spotify),
    ];
    let mut out: Vec<&'static str> = CODE.iter().copied().chain(configured.into_iter().filter_map(|(name, yes)| yes.then_some(name))).collect();
    out.sort_unstable();
    out
}

/// `GET /api/capabilities`. `version` is for showing and the logs only: the page decides by the names.
/// `{ "version": "0.11.0", "capabilities": ["batches", "classical", "lidarr", …] }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Capabilities {
    pub version: String,
    pub capabilities: Vec<String>,
}

/// The list now (asked once a page load, so read each time, not kept): what is in memory read here, the switches and
/// keys kept in SQLite off the async threads, together.
pub async fn read(app: &Arc<App>, serve: Option<Arc<crate::serve::Serve>>) -> Result<Capabilities> {
    let on = Configured {
        sign_in: serve.as_ref().is_some_and(|s| s.sign_in_required()),
        directory_sign_in: serve.as_ref().is_some_and(|s| s.directory.config().enabled),
        guest: super::runtime::now().guest.on,
        lidarr: discover::lidarr(app).is_some(),
        sync_play: app.snapcast.get().is_some(),
        jellyfin: app.library.kind_on(crate::source::jellyfin::KIND.id),
        subsonic: app.library.kind_on(crate::source::subsonic::KIND.id),
        incoming: app.library.incoming_on(),
        opus_copies: app.opus_copies.is_some(),
        spotify: super::spotify::store().is_ok(),
        ..Configured::default()
    };
    let ud = serve.map(|s| s.userdata.clone());
    let on = tokio::task::spawn_blocking(move || -> Result<Configured> {
        let mut on = on;
        on.musicbrainz = switches::is_on(switches::MUSICBRAINZ);
        on.discogs = switches::is_on(switches::DISCOGS);
        if let Some(ud) = ud.as_deref() {
            let acoustid = track_fetch::acoustid_key(ud)?.is_some();
            on.track_search = acoustid && switches::is_on(switches::SLSKD) && track_fetch::slskd_set(ud)?.is_some();
            on.track_intake = acoustid && ud.import_choice(track_fetch::SLSKD_FOLDER)?.is_some_and(|f| !f.trim().is_empty());
            on.lastfm = discover::server_lastfm_key(ud)?.is_some();
        }
        Ok(on)
    })
    .await??;
    Ok(Capabilities { version: super::VERSION.to_string(), capabilities: names(&on).into_iter().map(str::to_string).collect() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Configured = Configured {
        sign_in: true,
        directory_sign_in: true,
        guest: true,
        lidarr: true,
        track_search: true,
        track_intake: true,
        musicbrainz: true,
        discogs: true,
        lastfm: true,
        sync_play: true,
        jellyfin: true,
        subsonic: true,
        incoming: true,
        opus_copies: true,
        spotify: true,
    };

    #[test]
    fn nothing_set_up_is_the_code_alone_sorted() {
        let mut code = CODE.to_vec();
        code.sort_unstable();
        assert_eq!(names(&Configured::default()), code);
    }

    #[test]
    fn everything_set_up_is_named_once_in_order_in_snake_case() {
        let all = names(&ALL);
        assert_eq!(all.len(), CODE.len() + 15, "every configured name, none twice: {all:?}");
        assert!(all.windows(2).all(|w| w[0] < w[1]), "sorted, no duplicates: {all:?}");
        assert!(all.iter().all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')), "{all:?}");
        for name in ["sign_in", "directory_sign_in", "guest", "lidarr", "track_search", "track_intake", "musicbrainz", "discogs", "lastfm", "sync_play", "jellyfin", "subsonic", "incoming", "opus_copies", "spotify"] {
            assert!(all.contains(&name), "{name}");
        }
    }

    #[test]
    fn the_code_list_has_no_duplicates() {
        let mut code = CODE.to_vec();
        code.sort_unstable();
        code.dedup();
        assert_eq!(code.len(), CODE.len());
    }

    #[test]
    fn only_what_is_on_is_named() {
        let some = names(&Configured { lidarr: true, jellyfin: true, ..Configured::default() });
        assert!(some.contains(&"lidarr") && some.contains(&"jellyfin"));
        assert!(!some.contains(&"subsonic") && !some.contains(&"sign_in") && !some.contains(&"spotify"));
        assert_eq!(some.len(), CODE.len() + 2);
    }

    #[test]
    fn the_answer_is_a_version_and_a_list() {
        let c = Capabilities { version: "0.11.0".into(), capabilities: vec!["classical".into(), "lidarr".into()] };
        assert_eq!(serde_json::to_value(&c).unwrap(), serde_json::json!({ "version": "0.11.0", "capabilities": ["classical", "lidarr"] }));
    }
}
