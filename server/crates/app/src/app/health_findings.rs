//! Health's finding as every check makes it (decision 0063), below the areas whose checks make them (moved from `app::health`,
//! which keeps their names): how much it asks, where it is dealt with, what it is about, one by one; a finding's row, by its key.

use serde::Serialize;

use super::health_base::name_of;

/// How much it asks of the owner, most first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Broken or blind (the owner, 2026-11-04, for 0.16): the music can't play right, or Cratarr can't see (called
    /// "needs you" until the owner renamed it, 2026-10-23; "the owner has to act" until 0.16).
    Critical,
    /// Something is wrong but the music plays: data that is incomplete, what retries by itself, a decision waiting on
    /// the owner.
    Warning,
    /// Worth knowing.
    Info,
}

/// Where a finding is dealt with: a page, and the switch to turn on there (the Library's `upgrades`). `connections`: Settings'
/// Connections list itself, where the switches of every service are (G9: background reading).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Open {
    pub page: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch: Option<&'static str>,
    /// The thing on that page it opens (a connection in Settings: `musicbrainz`, `discogs`, `lastfm`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thing: Option<&'static str>,
}

/// What a check finds.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub domain: &'static str,
    /// Stable while the same thing is found (its first sight is kept by it).
    pub key: String,
    /// A short name for it, a noun phrase in title case (0.9.6, the owner: the panel's breadcrumb): "Missing covers".
    pub name: String,
    pub title: String,
    /// Health's row, in two parts (0.13, the owner): what it is about and how it is, read "{thing} · {count} {state}"
    /// ("Covers · 3 missing"). `None` where no such pair fits; the page falls back on the name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thing: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub count: u64,
    /// Of how many, where it is a share (AudioMuse's tracks heard).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub of: Option<u64>,
    pub level: Level,
    /// When it was first found (epoch s).
    pub since: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open: Option<Open>,
    /// What it is about, one by one (the owner, 2026-10-23: "more deeper information"): the albums waiting, the
    /// playlists and their missing tracks, the pages that answered slowly; each item opens where it is, when it is a
    /// thing of its own.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Item>,
}

/// One thing a finding is about.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Item {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// An album it opens: (server, album id); its cover shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<(String, String)>,
    /// That record's type, as the Library list serves it (GOTO.S1): its Go to names it (F31.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_type: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub album_secondary: Vec<String>,
    /// A playlist it opens (on its Repair tab).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist: Option<String>,
    /// The files it is about, by name (a folder's tracks AudioMuse has not heard), at most `FILES`: its own page lists
    /// them (the owner, 2026-10-24: the third level).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// An album's missing tracks (0.9.7, `library:incomplete`): "06 · Title", "Disc 2 · 03 · Title".
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    /// Its release group, for "Ask for the full album" (a request, as Discover's Want), and the request already out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<super::requests_base::Mark>,
    /// The track it is about (0.9.14, `files:short-*`): its song id, for replacing it on its own (the album's single
    /// track replacement).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    /// Since when it stands so (epoch s, `library:missing`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
}

/// At most this many items a finding lists. Its check finds them all, the report keeps the first: a record's own are
/// looked for among all (`record_health`).
pub const ITEMS: usize = 100;

/// A finding as a check makes it; its time is set by the report.
pub fn finding(domain: &'static str, key: impl Into<String>, level: Level, count: u64, title: impl Into<String>) -> Finding {
    let key = key.into();
    let (thing, state) = label_of(&key).map_or((None, None), |(t, s)| (Some(t.to_string()), Some(s.to_string())));
    Finding { domain, name: name_of(&key).into(), key, title: title.into(), thing, state, detail: None, count, of: None, level, since: 0, open: None, items: Vec::new() }
}

/// A finding's row, by its key: what it is about (a plural noun, Records where it counts records) and how it is (the
/// owner's table, 2026-11-01). An unreachable server's thing is its own name, set where it is found.
pub fn label_of(key: &str) -> Option<(&'static str, &'static str)> {
    Some(match key {
        "library:cover" => ("Covers", "missing"),
        "library:year" => ("Years", "missing"),
        "library:genre" => ("Genres", "missing"),
        "library:album-artist" => ("Album artists", "missing"),
        "library:musicbrainz" => ("MusicBrainz ids", "missing"),
        "library:track-numbers" => ("Track numbers", "off"),
        "library:discs" => ("Disc numbers", "off"),
        "library:incomplete" => ("Tracks", "missing"),
        "library:references" => ("References", "missing"),
        "library:missing" => ("Music", "missing"),
        "library:rules" => ("Names", "set by a rule"),
        "files:broken" => ("Files", "unreadable"),
        "files:cut-short" => ("Files", "cut off"),
        "files:damaged_tag" => ("Tags", "damaged"),
        "files:no_tags" => ("Tags", "missing"),
        "files:no_length" => ("Lengths", "missing"),
        "files:length-misread" => ("Lengths", "read wrong"),
        "files:length-guessed" => ("Lengths", "guessed"),
        "files:sound-breaks" => ("Sound", "breaking off"),
        "incoming:upgrades" => ("Replacements", "better"),
        "incoming:duplicates" => ("Duplicates", "found"),
        "incoming:failed" => ("Conversions", "failed"),
        "incoming:arrived" => ("Records", "arrived"),
        "requests:fetcher-down" => ("Fetchers", "down"),
        "playlists:missing" => ("Playlist tracks", "missing"),
        "analysis:audiomuse" => ("Analysis", "to do"),
        "sign-in:refused" => ("Sign-ins", "refused"),
        "sign-in:throttled" => ("Sign-ins", "throttled"),
        "server:runaway" => ("Clients", "running away"),
        "server:slow" => ("Responses", "slow"),
        "server:settings-file" => ("Settings file", "unreadable"),
        "server:short-of-space" => ("Disk space", "short"),
        "servers:lidarr" => ("Lidarr", "unreachable"),
        "library:ids-lost" => ("Track ids", "lost"),
        "requests:waiting" => ("Requests", "waiting"),
        "requests:soulseek-set-aside" => ("Soulseek queue", "set aside"),
        "requests:soulseek-frozen" => ("Soulseek queue", "not kept"),
        "words:genre" => ("Genres", "differ in the files"),
        "words:style" => ("Styles", "differ in the files"),
        "words:mood" => ("Moods", "differ in the files"),
        "words:tag" => ("Tags", "differ in the files"),
        k if k.starts_with("servers:") => ("Server", "unreachable"),
        // A player's own (the owner, 2026-11-01: "The player as the thing"): named where it is found.
        k if k.starts_with("players:held:") => ("Player", "play held"),
        k if k.starts_with("players:away:") => ("Player", "silent"),
        k if k.ends_with(":check") => ("Checks", "failed"),
        _ => return None,
    })
}

pub fn n(count: u64, one: &str, many: &str) -> String {
    format!("{} {}", thousands(count), if count == 1 { one } else { many })
}

/// A count with its thousands apart, as the page shows numbers ("22,729").
pub fn thousands(count: u64) -> String {
    let digits = count.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
