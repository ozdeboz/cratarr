//! Health's words that a fix and a length's file read too, below Incoming (moved from `app::health`, which keeps their names):
//! a short file's slack and cause, a length as a clock, a finding's short name.

/// A track's sound may end this much before its given length unnoticed (ms; a page's slack for MP3 headers that
/// disagree a little).
pub const SHORT_SLACK_MS: i64 = 1_500;

/// Why a file's sound ends `heard` ms in, before its given length: what it says of itself (`check_short`).
pub fn short_cause(path: &std::path::Path, heard: i64) -> &'static str {
    let mp3 = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mp3"));
    if !mp3 {
        return "files:sound-breaks";
    }
    let Some(info) = crate::catalogue::tags::mp3_info(path) else { return "files:sound-breaks" };
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    // Its info frame tells of more bytes than the file holds after its tag (a few KB of slack for a trailing tag).
    if info.bytes.is_some_and(|b| b > size.saturating_sub(info.start) + 16 * 1024) {
        return "files:cut-short";
    }
    match info.length_ms() {
        // Its own length is what was heard: Cratarr read another.
        Some(ms) if (ms as i64 - heard).abs() <= SHORT_SLACK_MS => "files:length-misread",
        // It tells of more than there is: frames missing.
        Some(_) => "files:cut-short",
        None => "files:length-guessed",
    }
}

/// A length as a clock: m:ss, or h:mm:ss.
pub fn clock(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

/// A finding's short name, by its key (the breadcrumb's; its title stays the sentence): a fix plan's sheet's too.
pub fn name_of(key: &str) -> &'static str {
    match key {
        "files:broken" => "Unreadable files",
        "files:damaged_tag" => "Damaged tags",
        "files:no_tags" => "Untagged files",
        "files:no_length" => "Files without a length",
        "files:length-misread" => "Lengths read wrong",
        "files:length-guessed" => "Lengths guessed",
        "files:cut-short" => "Cut-off files",
        "files:sound-breaks" => "Sound breaking off",
        "library:cover" => "Missing covers",
        "library:year" => "Missing years",
        "library:genre" => "Missing genres",
        "library:album-artist" => "Missing album artists",
        "library:track-numbers" => "Track numbers",
        "library:discs" => "Disc numbers",
        "library:musicbrainz" => "Missing MusicBrainz ids",
        "library:incomplete" => "Records missing tracks",
        "library:references" => "References",
        "library:missing" => "Missing music",
        "library:rules" => "Named otherwise by a rule",
        "incoming:upgrades" => "Quality replacements",
        "incoming:duplicates" => "Duplicates",
        "incoming:failed" => "Failed conversions",
        "incoming:arrived" => "Arrived",
        "requests:fetcher-down" => "Fetchers out of reach",
        "playlists:missing" => "Missing tracks",
        // No tool names in the names (the owner, 2026-10-26).
        "analysis:audiomuse" => "AudioMuse",
        "servers:lidarr" => "Lidarr",
        "sign-in:refused" => "Refused requests",
        "sign-in:throttled" => "Throttled sign-ins",
        "server:runaway" => "Runaway client",
        "server:slow" => "Slow responses",
        "server:settings-file" => "Settings",
        "server:short-of-space" => "Short of space",
        "requests:waiting" => "Requests",
        "requests:soulseek-set-aside" | "requests:soulseek-frozen" => "Soulseek queue",
        "requests:bulk-set-aside" => "Bulk queue",
        "words:genre" => "Genres in the files",
        "words:style" => "Styles in the files",
        "words:mood" => "Moods in the files",
        "words:tag" => "Tags in the files",
        k if k.starts_with("files:") => "File problems",
        k if k.starts_with("servers:") => "Unreachable server",
        k if k.starts_with("players:held:") => "Held plays",
        k if k.starts_with("players:away:") => "Silent player",
        k if k.ends_with(":check") => "Check failed",
        _ => "Finding",
    }
}
