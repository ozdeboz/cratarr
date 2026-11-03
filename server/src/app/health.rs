//! One health report (decision 0063): every domain's check says what it finds now, in one shape; the Health page
//! shows it all, Home's System status the most critical and the most recent. A check that fails is a finding itself, never a
//! missing report. When each finding was first found is kept while it stays, so "the most recent" means something.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use super::App;

/// How much it asks of the owner, most first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// The owner has to act (critical; called "needs you" until the owner renamed it, 2026-10-23).
    Critical,
    /// Something is wrong and may right itself.
    Warning,
    /// Worth knowing.
    Info,
}

/// Where a finding is dealt with: a page, and the switch to turn on there (the Library's `upgrades`).
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
    pub request: Option<super::requests::Mark>,
    /// The track it is about (0.9.14, `files:short-*`): its song id, for replacing it on its own (the album's single
    /// track replacement).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
}

/// At most this many files an item names.
const FILES: usize = 30;

/// At most this many items a finding lists. Its check finds them all, the report keeps the first: a record's own are
/// looked for among all (`record_health`).
const ITEMS: usize = 100;

/// A track's sound may end this much before its given length unnoticed (ms; a page's slack for MP3 headers that
/// disagree a little).
pub(crate) const SHORT_SLACK_MS: i64 = 1_500;

/// Why a file's sound ends `heard` ms in, before its given length: what it says of itself (`check_short`).
pub(crate) fn short_cause(path: &std::path::Path, heard: i64) -> &'static str {
    let mp3 = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mp3"));
    if !mp3 {
        return "files:sound-breaks";
    }
    let Some(info) = crate::stream::mp3_info(path) else { return "files:sound-breaks" };
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
pub(crate) fn clock(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

#[derive(Debug, Serialize)]
pub struct Report {
    /// Worst first, the most recent first among equals.
    pub findings: Vec<Finding>,
    /// When each domain was last checked (epoch s).
    pub checked: BTreeMap<&'static str, i64>,
}

/// A record's own problems (0.14): the findings that name it, as Health's rows say them, each with only its items.
#[derive(Debug, Serialize)]
pub struct RecordHealth {
    /// Worst first.
    pub findings: Vec<RecordFinding>,
}

/// A finding as one record has it: its kind (the report's key), its short name and row, how much it asks, and the
/// record's items in it (its tracks', where it is about files).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecordFinding {
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thing: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub level: Level,
    pub items: Vec<Item>,
}

/// What the findings say of one record (server, album id): each that names it, with only its items; and its files'
/// problems (kind, its track's item), a finding of each kind as `files` makes them. Worst first.
pub fn record(findings: &[Finding], album: &(String, String), wrong: &[(String, Item)]) -> RecordHealth {
    let of = |f: &Finding, items: Vec<Item>| RecordFinding { key: f.key.clone(), name: f.name.clone(), thing: f.thing.clone(), state: f.state.clone(), detail: f.detail.clone(), level: f.level, items };
    let mut out: Vec<RecordFinding> = findings
        .iter()
        .filter_map(|f| {
            let items: Vec<Item> = f.items.iter().filter(|i| i.album.as_ref() == Some(album)).cloned().collect();
            (!items.is_empty()).then(|| of(f, items))
        })
        .collect();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for (kind, _) in wrong {
        *counts.entry(kind.clone()).or_default() += 1;
    }
    for f in files(&counts) {
        let kind = f.key.trim_start_matches("files:");
        out.push(of(&f, wrong.iter().filter(|(k, _)| k == kind).map(|(_, i)| i.clone()).collect()));
    }
    out.sort_by(|a, b| a.level.cmp(&b.level).then_with(|| a.key.cmp(&b.key)));
    RecordHealth { findings: out }
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// When each finding (by key) was first found; those no longer found are forgotten.
static FIRST_SEEN: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);

/// A finding as a check makes it; its time is set by the report.
fn finding(domain: &'static str, key: impl Into<String>, level: Level, count: u64, title: impl Into<String>) -> Finding {
    let key = key.into();
    let (thing, state) = label_of(&key).map_or((None, None), |(t, s)| (Some(t.to_string()), Some(s.to_string())));
    Finding { domain, name: name_of(&key).into(), key, title: title.into(), thing, state, detail: None, count, of: None, level, since: 0, open: None, items: Vec::new() }
}

/// A finding's row, by its key: what it is about (a plural noun, Records where it counts records) and how it is (the
/// owner's table, 2026-11-01). An unreachable server's thing is its own name, set where it is found.
fn label_of(key: &str) -> Option<(&'static str, &'static str)> {
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
        "playlists:missing" => ("Playlist tracks", "missing"),
        "analysis:audiomuse" => ("Analysis", "to do"),
        "sign-in:refused" => ("Sign-ins", "refused"),
        "sign-in:throttled" => ("Sign-ins", "throttled"),
        "server:runaway" => ("Clients", "running away"),
        "server:slow" => ("Responses", "slow"),
        "server:settings-file" => ("Settings file", "unreadable"),
        "servers:lidarr" => ("Lidarr", "unreachable"),
        "requests:waiting" => ("Requests", "waiting"),
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

/// A finding's short name, by its key (the breadcrumb's; its title stays the sentence): a fix plan's sheet's too.
pub(super) fn name_of(key: &str) -> &'static str {
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
        "library:rules" => "Named otherwise by a rule",
        "incoming:upgrades" => "Quality replacements",
        "incoming:duplicates" => "Duplicates",
        "incoming:failed" => "Failed conversions",
        "playlists:missing" => "Missing tracks",
        // No tool names in the names (the owner, 2026-10-26).
        "analysis:audiomuse" => "Analysis",
        "servers:lidarr" => "Lidarr",
        "sign-in:refused" => "Refused requests",
        "sign-in:throttled" => "Throttled sign-ins",
        "server:runaway" => "Runaway client",
        "server:slow" => "Slow responses",
        "server:settings-file" => "Settings",
        "requests:waiting" => "Requests",
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

fn n(count: u64, one: &str, many: &str) -> String {
    format!("{} {}", thousands(count), if count == 1 { one } else { many })
}

/// A count with its thousands apart, as the page shows numbers ("22,729").
fn thousands(count: u64) -> String {
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

/// A check's answer: its findings, or the one that says it could not look.
fn or_failed(domain: &'static str, r: anyhow::Result<Vec<Finding>>) -> Vec<Finding> {
    r.unwrap_or_else(|e| {
        let mut f = finding(domain, format!("{domain}:check"), Level::Warning, 1, format!("The {domain} could not be checked"));
        f.detail = Some(format!("{e:#}"));
        vec![f]
    })
}

/// The findings in the report's order, each with when it was first found (kept from earlier reports).
pub fn ordered(mut findings: Vec<Finding>, at: i64) -> Vec<Finding> {
    let mut seen = FIRST_SEEN.lock().unwrap_or_else(|p| p.into_inner());
    let seen = seen.get_or_insert_with(HashMap::new);
    seen.retain(|k, _| findings.iter().any(|f| &f.key == k));
    for f in &mut findings {
        f.since = *seen.entry(f.key.clone()).or_insert(at);
    }
    findings.sort_by(|a, b| a.level.cmp(&b.level).then(b.since.cmp(&a.since)).then_with(|| a.key.cmp(&b.key)));
    findings
}

/// The library's files the catalogue could not read properly (decision 0025): its own counts.
pub fn files(issues: &BTreeMap<String, u64>) -> Vec<Finding> {
    let health = || Some(Open { page: "health", switch: None, thing: None });
    let mut out = Vec::new();
    for (kind, &count) in issues {
        if count == 0 {
            continue;
        }
        let (level, title) = match kind.as_str() {
            "broken" => (Level::Critical, n(count, "file can't be read", "files can't be read")),
            "damaged_tag" => (Level::Warning, n(count, "file has a damaged tag", "files have a damaged tag")),
            "no_tags" => (Level::Warning, n(count, "file has no tags", "files have no tags")),
            "no_length" => (Level::Info, n(count, "file has no length", "files have no length")),
            "bad_seektable" => (Level::Warning, n(count, "file has a broken seek table", "files have a broken seek table")),
            other => (Level::Info, format!("{count} files: {other}")),
        };
        let mut f = finding("files", format!("files:{kind}"), level, count, title);
        f.open = health();
        out.push(f);
    }
    out
}

/// The library's albums lacking what it requires, a finding for each kind, each album opening its panel: the cover,
/// the year, the genre, the album artist and the track numbers are warnings; the MusicBrainz id, which many lack and
/// which only some things use (Lidarr, Discover, exact imports), is told. A cover missing is critical (the owner,
/// 2026-10-25). No share of all the albums (0.9.7, the owner: a bar is for something good, not for what is wrong).
/// Track numbers (0.9.6): `numbering`'s
/// albums whose numbers are off (twice, missing, none), each with what is off; Renumber fixes them.
/// Disc numbers (0.13): `discs`' albums that lost theirs (numbers starting again, or fewer discs than the release that
/// holds them), each with what is off; Renumber fixes them too.
pub fn library(rows: &[crate::serve::library::Lacks], numbering: &[(String, String, String, String)], discs: &[(String, String, String, String)]) -> Vec<Finding> {
    type Pick = fn(&crate::serve::library::Lacks) -> bool;
    let kinds: [(&str, Level, &str, &str, Pick); 5] = [
        ("cover", Level::Critical, "record without a cover", "records without a cover", |r| r.cover),
        ("year", Level::Warning, "record without a year", "records without a year", |r| r.year),
        ("genre", Level::Warning, "record without a genre", "records without a genre", |r| r.genre),
        ("album-artist", Level::Warning, "record without an album artist", "records without an album artist", |r| r.album_artist),
        ("musicbrainz", Level::Info, "record without its MusicBrainz id", "records without their MusicBrainz id", |r| r.musicbrainz),
    ];
    let mut out = Vec::new();
    for (key, level, one, many, pick) in kinds {
        let mut hit: Vec<&crate::serve::library::Lacks> = rows.iter().filter(|r| pick(r)).collect();
        if hit.is_empty() {
            continue;
        }
        hit.sort_by(|a, b| a.artist.to_lowercase().cmp(&b.artist.to_lowercase()).then_with(|| a.name.cmp(&b.name)));
        let mut f = finding("library", format!("library:{key}"), level, hit.len() as u64, n(hit.len() as u64, one, many));
        f.items = hit
            .iter()
            .map(|r| Item { title: r.name.clone(), line: Some(r.artist.clone()), album: Some((crate::source::cratarr::KIND.id.to_string(), r.id.clone())), ..Item::default() })
            .collect();
        out.push(f);
    }
    let found: [(&[(String, String, String, String)], &str, &str, &str); 2] = [
        (numbering, "library:track-numbers", "record with track numbers off", "records with track numbers off"),
        (discs, "library:discs", "record that lost its disc numbers", "records that lost their disc numbers"),
    ];
    for (albums, key, one, many) in found {
        if albums.is_empty() {
            continue;
        }
        let mut hit: Vec<&(String, String, String, String)> = albums.iter().collect();
        hit.sort_by(|a, b| a.2.to_lowercase().cmp(&b.2.to_lowercase()).then_with(|| a.1.cmp(&b.1)));
        let count = hit.len() as u64;
        let mut f = finding("library", key, Level::Warning, count, n(count, one, many));
        f.items = hit
            .iter()
            .map(|(id, name, artist, line)| Item {
                title: name.clone(),
                line: Some(format!("{artist} · {line}")),
                album: Some((crate::source::cratarr::KIND.id.to_string(), id.clone())),
                ..Item::default()
            })
            .collect();
        out.push(f);
    }
    out
}

/// Incoming's albums as they stand to the other libraries (decision 0034): better copies of Music's (Replace), the rest
/// in two libraries at once (an upgrade of another Incoming library's copy among them: Replace is Music's alone); and
/// the conversions that failed. Dealt with on their items' own pages (the comparison, the album's menu), not the
/// Library, whose own filter could hide them (the owner, 2026-10-24): no `open`.
pub fn incoming(upgrades: u64, duplicates: u64, failed: u64) -> Vec<Finding> {
    let mut out = Vec::new();
    if upgrades > 0 {
        // Said as what the library holds (the owner, 2026-10-25: duplicates on an artist's page went unnoticed under
        // "duplicates waiting").
        out.push(finding("incoming", "incoming:upgrades", Level::Critical, upgrades, n(upgrades, "better copy waits to replace Music's", "better copies wait to replace Music's")));
    }
    if duplicates > 0 {
        out.push(finding("incoming", "incoming:duplicates", Level::Critical, duplicates, n(duplicates, "record is in two libraries at once", "records are in two libraries at once")));
    }
    if failed > 0 {
        out.push(finding("incoming", "incoming:failed", Level::Warning, failed, n(failed, "conversion failed", "conversions failed")));
    }
    out
}

/// The track numbers a record of one disc lacks where no release was held against it: those below its highest number,
/// and those above it up to the total its tags agree on (0.14, the owner, 2026-11-02: "05/06" lacks 06 too). None for
/// a record of several discs.
fn gaps_in(numbers: &[(Option<u32>, Option<u32>)], totals: &[u32]) -> Vec<String> {
    if numbers.iter().any(|(d, _)| d.unwrap_or(1) > 1) {
        return Vec::new();
    }
    let mut seen: Vec<u32> = numbers.iter().filter_map(|(_, t)| *t).filter(|t| *t > 0).collect();
    seen.sort_unstable();
    seen.dedup();
    // A record numbered nowhere lacks nothing its numbers tell.
    let Some(&highest) = seen.last() else { return Vec::new() };
    let top = highest.max(agreed_total(totals));
    (1..=top).filter(|n| !seen.contains(n)).map(|n| format!("{n:02}")).collect()
}

/// The total a record's tags agree on: the largest said by two tracks or more, or the one every track that says one
/// says (a single track's "/12" among "/06"s is that track's slip, not six tracks missing); 0 for none.
fn agreed_total(totals: &[u32]) -> u32 {
    let mut by: BTreeMap<u32, usize> = BTreeMap::new();
    for &t in totals.iter().filter(|t| **t > 0) {
        *by.entry(t).or_default() += 1;
    }
    if by.len() == 1 {
        return by.keys().next().copied().unwrap_or(0);
    }
    by.iter().rev().find(|(_, n)| **n >= 2).map_or(0, |(t, _)| *t)
}

/// A better copy Replace can take: an upgrade of Music's album (`incoming`'s upgrades; any other verdict is an album in
/// two libraries at once).
fn replaces(v: &super::incoming::VerdictView) -> bool {
    v.kind == "upgrade" && v.replaceable
}

/// Where a verdict's other copy is, for an item's line: Music, else its library as named.
fn paired_in<'a>(v: &super::incoming::VerdictView, libraries: &'a [super::albums::LibraryView]) -> &'a str {
    if v.replaceable {
        return "Music";
    }
    v.library_id.as_ref().and_then(|id| libraries.iter().find(|l| &l.id == id)).map_or("another library", |l| l.name.as_str())
}

/// The connections failing (0.9.6, the owner: each one's signal): a service Cratarr calls out to that failed a few
/// times in a row, or for a quarter of an hour, with no answer since; a switched-off one says nothing. MusicBrainz is
/// critical (the background pass and Find more music stop), the rest a warning.
fn check_connections(at: i64) -> Vec<Finding> {
    let tz = super::moment::tz(super::moment::told(at).as_ref());
    let clock = |t: i64| {
        let s = (t + tz).rem_euclid(86_400);
        format!("{:02}:{:02}", s / 3600, (s % 3600) / 60)
    };
    let switched_off = |name: &str| match name {
        "musicbrainz" => !super::switches::is_on(super::switches::MUSICBRAINZ),
        "discogs" => !super::switches::is_on(super::switches::DISCOGS),
        "lastfm" => !super::switches::is_on(super::switches::LASTFM),
        _ => false,
    };
    super::reach::failing(at)
        .into_iter()
        .filter(|(name, _)| !switched_off(name))
        .map(|(name, s)| {
            let level = if name == "musicbrainz" { Level::Critical } else { Level::Warning };
            let mut f = finding("connections", format!("connections:{name}"), level, 1, format!("{} is failing since {}", super::reach::label(name), clock(s.failing_since)));
            f.name = super::reach::label(name).to_string();
            (f.thing, f.state) = (Some(f.name.clone()), Some("failing".into()));
            f.detail = s.last_error.map(|(_, why)| why);
            f.open = matches!(name, "musicbrainz" | "discogs" | "lastfm").then_some(Open { page: "settings", switch: None, thing: Some(name_static(name)) });
            f
        })
        .collect()
}

fn name_static(name: &str) -> &'static str {
    match name {
        "musicbrainz" => "musicbrainz",
        "discogs" => "discogs",
        _ => "lastfm",
    }
}

/// Requests waiting for an answer (decision 0070), each named, opening the Requests page.
fn check_requests(served: Option<&crate::serve::Serve>) -> anyhow::Result<Vec<Finding>> {
    let Some(serve) = served else { return Ok(Vec::new()) };
    let waiting = serve.userdata.requests(None, Some(super::requests::PENDING), ITEMS)?;
    if waiting.is_empty() {
        return Ok(Vec::new());
    }
    let count = super::requests::waiting(&serve)? as u64;
    let mut f = finding("requests", "requests:waiting", Level::Info, count, format!("{} waiting", n(count, "request", "requests")));
    f.open = Some(Open { page: "requests", switch: None, thing: None });
    f.items = waiting.into_iter().map(|r| Item { title: r.album, line: Some(format!("{} · asked by {}", r.artist, r.user)), ..Item::default() }).collect();
    Ok(vec![f])
}

impl App {
    /// Every domain's findings now (decision 0063), worst first, the most recent first among equals.
    pub async fn health_report(&self) -> Report {
        let at = now();
        let (files, playlists, incoming, analysis, servers, library) = tokio::join!(
            self.check_files(),
            self.check_playlists(),
            self.check_incoming(),
            self.check_analysis(),
            self.check_servers(),
            self.check_library()
        );
        let mut findings = Vec::new();
        let mut checked = BTreeMap::new();
        for (domain, found) in
            [("files", files), ("playlists", playlists), ("incoming", incoming), ("analysis", analysis), ("servers", servers), ("library", library)]
        {
            findings.extend(or_failed(domain, found));
            checked.insert(domain, at);
        }
        findings.extend(self.check_players());
        checked.insert("players", at);
        for (domain, found) in [("sign-in", check_sign_in(at)), ("server", check_server(at)), ("requests", check_requests(self.served.as_deref())), ("connections", Ok(check_connections(at)))] {
            findings.extend(or_failed(domain, found));
            checked.insert(domain, at);
        }
        for f in &mut findings {
            f.items.truncate(ITEMS);
        }
        Report { findings: ordered(findings, at), checked }
    }

    /// A record's own problems (0.14, the owner, 2026-11-02: on its page and in its menu): the checks that name records,
    /// run as the report runs them, and its tracks' files as the catalogue read them. Not the report's other domains, and
    /// not through `ordered`: what the report found first stays its own.
    pub async fn record_health(&self, source: &str, album: &str) -> anyhow::Result<RecordHealth> {
        let (short, incoming, library) = tokio::join!(self.check_short(), self.check_incoming(), self.check_library());
        let found = [short?, incoming?, library?].concat();
        let mut wrong = Vec::new();
        // Its files' problems: only the own library's records have files Cratarr read.
        if let Some(served) = self.served.clone().filter(|_| source == crate::source::cratarr::KIND.id) {
            let (c, id) = (self.library.catalogue(), album.to_string());
            wrong = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<(String, Item)>> {
                let Some((folder, ..)) = served.library.album_place(&id)? else { return Ok(Vec::new()) };
                let tracks = served.library.album_tracks(&id)?;
                let rels: Vec<String> = tracks.iter().map(|t| t.1.clone()).collect();
                let mut out = Vec::new();
                for p in c.problems_in(folder, &rels)? {
                    let Some((song, _, title, ..)) = tracks.iter().find(|t| t.1 == p.rel) else { continue };
                    let file = p.rel.rsplit('/').next().unwrap_or(&p.rel).to_string();
                    let item = Item {
                        title: title.clone(),
                        line: p.detail,
                        album: Some((crate::source::cratarr::KIND.id.to_string(), id.clone())),
                        files: vec![file],
                        track: Some(song.clone()),
                        ..Item::default()
                    };
                    out.push((p.kind, item));
                }
                Ok(out)
            })
            .await??;
        }
        Ok(record(&found, &(source.to_string(), album.to_string()), &wrong))
    }

    async fn check_files(&self) -> anyhow::Result<Vec<Finding>> {
        let c = self.library.catalogue();
        let status = tokio::task::spawn_blocking(move || c.status()).await??;
        let mut out = files(&status.issues);
        out.extend(self.check_short().await?);
        Ok(out)
    }

    /// Tracks whose sound ends before the length they give (0.9.14, the owner: different causes, different fixes, offer
    /// all): a player asked past their sound's end fails. By cause, from what each file says of itself:
    /// - **read wrong**: its MP3 info frame gives its true length; Cratarr read another (read again);
    /// - **guessed**: an MP3 without an info frame, its length guessed from its first frame's bitrate (its sound whole);
    /// - **cut off**: its info frame tells of more frames or bytes than the file holds (its sound missing: replace it,
    ///   or play it to its end);
    /// - **breaking off**: any other (a FLAC's damaged frames, decoding stopped): replace it, or play it to where it
    ///   breaks off.
    async fn check_short(&self) -> anyhow::Result<Vec<Finding>> {
        let c = self.library.catalogue();
        let served = self.served.clone();
        let found = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<(&'static str, Item, i64)>> {
            let mut out = Vec::new();
            for (folder, rel, path, given, heard) in c.shorter_than_heard(SHORT_SLACK_MS)? {
                let cause = short_cause(&path, heard);
                let song = served.as_ref().and_then(|s| s.library.song_at(folder, &rel).ok().flatten());
                let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel.as_str()));
                let item = Item {
                    title: song.as_ref().map_or_else(|| file.to_string(), |s| s.2.clone()),
                    line: Some(format!("Sounds {} of {} · {dir}", clock(heard), clock(given))),
                    album: song.as_ref().map(|s| (crate::source::cratarr::KIND.id.to_string(), s.1.clone())),
                    track: song.map(|s| s.0),
                    files: vec![file.to_string()],
                    ..Item::default()
                };
                out.push((cause, item, given - heard));
            }
            Ok(out)
        })
        .await??;
        let mut out = Vec::new();
        for (key, level, one, many, detail) in [
            ("files:length-misread", Level::Warning, "track's length is read wrong", "tracks' lengths are read wrong", "Their files give their true length; Cratarr read another. Read them again."),
            ("files:length-guessed", Level::Warning, "track's length is guessed", "tracks' lengths are guessed", "Their files do not give their length, so it was guessed; their sound is whole. Write their true length into them."),
            ("files:cut-short", Level::Critical, "file is cut off", "files are cut off", "Their files hold less than they say: the end of their sound is missing. Replace them, or play them to where they end."),
            ("files:sound-breaks", Level::Critical, "track's sound breaks off", "tracks' sound breaks off", "Their sound cannot be decoded to its end (damaged). Replace them, or play them to where they break off."),
        ] {
            let mut items: Vec<(Item, i64)> = found.iter().filter(|(c, ..)| *c == key).map(|(_, i, short)| (i.clone(), *short)).collect();
            if items.is_empty() {
                continue;
            }
            items.sort_by(|a, b| b.1.cmp(&a.1));
            let count = items.len() as u64;
            let mut f = finding("files", key, level, count, n(count, one, many));
            f.detail = Some(detail.to_string());
            f.open = Some(Open { page: "health", switch: None, thing: None });
            f.items = items.into_iter().map(|(i, _)| i).collect();
            out.push(f);
        }
        Ok(out)
    }

    /// The albums of every library in use lacking what the library requires (the owner, 2026-11-02: Music's and
    /// Incoming's alike; Incoming's are marked there too).
    async fn check_library(&self) -> anyhow::Result<Vec<Finding>> {
        let Some(served) = self.served.clone() else { return Ok(Vec::new()) };
        let (lib, ud) = (served.library.clone(), served.userdata.clone());
        let (rows, numbering, matched, released) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            Ok((lib.lacking()?, lib.numbering(None)?, ud.renumber_matched()?, ud.album_discs()?))
        })
        .await??;
        // Disc numbers lost (0.13) first: such an album's numbers twice are that, not Track numbers.
        let mut discs: Vec<(String, String, String, String)> = Vec::new();
        let mut off: Vec<(String, String, String, String)> = Vec::new();
        for (id, name, artist, numbers) in numbering {
            if let Some(line) = super::renumber::discs_off(&numbers, released.get(&id).copied()) {
                discs.push((id, name, artist, line));
            } else if let Some(line) = super::renumber::off(&numbers, !matched.contains(&id)) {
                off.push((id, name, artist, line));
            }
        }
        let mut out = library(&rows, &off, &discs);
        out.extend(self.check_incomplete(&served).await?);
        out.extend(self.check_references(&served).await?);
        // Each kind of word the Catalogue sheet writes (0.15, decision 0097): the records whose files differ.
        for (kind, records) in super::file_words::findings(&served).await? {
            let count = records.len() as u64;
            let word = kind.name();
            let mut f = finding("library", format!("words:{word}"), Level::Info, count, n(count, &format!("record whose files' {word} differs from Cratarr's"), &format!("records whose files' {word} differs from Cratarr's")));
            f.items = records
                .into_iter()
                .take(ITEMS)
                .map(|r| Item { title: r.title, line: Some(r.artist), album: Some((crate::source::cratarr::KIND.id.to_string(), r.album)), ..Item::default() })
                .collect();
            out.push(f);
        }
        // Albums whose files a rule names otherwise (0.9.7): their fix writes the rules' names into the files.
        let s = served.clone();
        let ruled = tokio::task::spawn_blocking(move || super::fixes::ruled_albums(&s)).await??;
        if !ruled.is_empty() {
            let count = ruled.len() as u64;
            let s = served.clone();
            let items = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<Item>> {
                let mut items = Vec::new();
                for a in &ruled {
                    if let Some((_, _, _, artist, name)) = s.library.album_place(a)? {
                        items.push(Item { title: name, line: Some(artist), album: Some((crate::source::cratarr::KIND.id.to_string(), a.clone())), ..Item::default() });
                    }
                }
                Ok(items)
            })
            .await??;
            let mut f = finding("library", "library:rules", Level::Info, count, n(count, "record a rule names otherwise", "records a rule names otherwise"));
            f.items = items;
            out.push(f);
        }
        Ok(out)
    }

    /// Albums missing tracks (0.9.7, `app::incomplete`): each with what its release has beyond it and whether it is
    /// asked for already; and the albums whose numbers have gaps that no release could be held against yet. Every
    /// library in use's (the owner, 2026-11-02).
    async fn check_incomplete(&self, served: &std::sync::Arc<crate::serve::Serve>) -> anyhow::Result<Vec<Finding>> {
        let s = served.clone();
        let (found, gaps) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let mut found: Vec<(String, String, String, super::incomplete::Incomplete)> = Vec::new();
            for (album, json) in s.userdata.albums_incomplete()? {
                let Ok(i) = serde_json::from_str::<super::incomplete::Incomplete>(&json) else { continue };
                // One the library no longer has is left out.
                let Some((_, _, _, artist, name)) = s.library.album_place(&album)? else { continue };
                found.push((album, name, artist, i));
            }
            // Gaps in the numbers where no release was held against the album (no MusicBrainz id, no match yet).
            let checked = s.userdata.album_complete_at()?;
            let totals = s.library.track_totals()?;
            let mut gaps = Vec::new();
            for (id, name, artist, numbers) in s.library.numbering(None)? {
                if checked.contains_key(&id) {
                    continue;
                }
                let missing = gaps_in(&numbers, totals.get(&id).map_or(&[][..], Vec::as_slice));
                if !missing.is_empty() {
                    gaps.push((id, name, artist, missing));
                }
            }
            Ok((found, gaps))
        })
        .await??;
        if found.is_empty() && gaps.is_empty() {
            return Ok(Vec::new());
        }
        let marks = super::requests::marks(self, served).await.ok();
        let mut items: Vec<Item> = found
            .into_iter()
            .map(|(album, name, artist, i)| {
                let discs = i.missing.iter().any(|m| m.0 > 1);
                Item {
                    title: name.clone(),
                    line: Some(format!("{artist} · {} of {}", i.has, i.of)),
                    album: Some((crate::source::cratarr::KIND.id.to_string(), album)),
                    missing: i.missing.iter().map(|(d, t, title)| if discs { format!("Disc {d} · {t:02} · {title}") } else { format!("{t:02} · {title}") }).collect(),
                    request: marks.as_deref().and_then(|m| super::requests::mark_of(m, i.release_group.as_deref(), &artist, &name)),
                    release_group: i.release_group,
                    ..Item::default()
                }
            })
            .collect();
        items.extend(gaps.into_iter().map(|(album, name, artist, missing)| Item {
            title: name,
            line: Some(format!("{artist} · no MusicBrainz match yet")),
            album: Some((crate::source::cratarr::KIND.id.to_string(), album)),
            missing,
            ..Item::default()
        }));
        items.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.title.cmp(&b.title)));
        let count = items.len() as u64;
        let mut f = finding("library", "library:incomplete", Level::Warning, count, n(count, "record missing tracks", "records missing tracks"));
        f.items = items;
        Ok(vec![f])
    }

    /// Records no reference speaks for (0.14, the owner, 2026-11-02: one Health item for references): each opens to its
    /// sources, one to choose, a link to paste, or checked by hand (`app::references`).
    async fn check_references(&self, served: &std::sync::Arc<crate::serve::Serve>) -> anyhow::Result<Vec<Finding>> {
        let s = served.clone();
        let found = tokio::task::spawn_blocking(move || super::references::unreferenced(&s)).await??;
        if found.is_empty() {
            return Ok(Vec::new());
        }
        let count = found.len() as u64;
        let mut f = finding("library", "library:references", Level::Info, count, n(count, "record without a reference", "records without a reference"));
        let mut items: Vec<Item> = found
            .into_iter()
            .map(|(album, name, artist)| Item { title: name, line: Some(artist), album: Some((crate::source::cratarr::KIND.id.to_string(), album)), ..Item::default() })
            .collect();
        items.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.title.cmp(&b.title)));
        items.truncate(ITEMS);
        f.items = items;
        Ok(vec![f])
    }

    /// One's own playlists' tracks whose file is gone (the Repair view, one playlist at a time).
    async fn check_playlists(&self) -> anyhow::Result<Vec<Finding>> {
        let lists = self.playlists.view(None).await?;
        let broken: Vec<_> = lists.iter().filter(|p| p.mine && p.dead > 0).collect();
        let missing: u64 = broken.iter().map(|p| p.dead as u64).sum();
        if missing == 0 {
            return Ok(Vec::new());
        }
        let title = format!("{} in {}", n(missing, "missing track", "missing tracks"), n(broken.len() as u64, "playlist", "playlists"));
        let mut f = finding("playlists", "playlists:missing", Level::Warning, missing, title);
        let mut most: Vec<_> = broken.clone();
        most.sort_by(|a, b| b.dead.cmp(&a.dead).then_with(|| a.name.cmp(&b.name)));
        f.items = most
            .iter()
            .take(ITEMS)
            .map(|p| Item { title: p.name.clone(), line: Some(n(p.dead as u64, "missing track", "missing tracks")), playlist: Some(p.name.clone()), ..Item::default() })
            .collect();
        f.open = Some(Open { page: "playlists", switch: None, thing: None });
        Ok(vec![f])
    }

    async fn check_incoming(&self) -> anyhow::Result<Vec<Finding>> {
        let view = self.albums(None).await;
        let verdicts = view.sources.iter().flat_map(|s| s.verdicts.values());
        let (mut upgrades, mut duplicates) = (0, 0);
        for v in verdicts {
            if replaces(v) {
                upgrades += 1;
            } else {
                duplicates += 1;
            }
        }
        let jobs: Vec<_> = self.incoming.jobs().into_iter().filter(|j| serde_json::to_value(&j.state).ok().and_then(|v| v.as_str().map(|s| s == "failed")) == Some(true)).collect();
        let mut out = incoming(upgrades, duplicates, jobs.len() as u64);
        // Each album waiting, with the album it upgrades or duplicates; each conversion failed, with why.
        for f in &mut out {
            let better = match f.key.as_str() {
                "incoming:upgrades" => true,
                "incoming:duplicates" => false,
                "incoming:failed" => {
                    f.items = jobs
                        .iter()
                        .map(|j| Item { title: j.title.clone(), line: j.why.clone(), album: Some((j.source.clone(), j.album_id.clone())), ..Item::default() })
                        .collect();
                    continue;
                }
                _ => continue,
            };
            for s in &view.sources {
                for (id, v) in s.verdicts.iter().filter(|(_, v)| replaces(v) == better) {
                    let a = s.albums.iter().find(|a| &a.id == id);
                    let title = a.map_or_else(|| id.clone(), |a| a.title.clone());
                    let by = a.and_then(|a| a.artist.clone());
                    // Both places: the other copy, in its library as the verdict names it (this one opens Incoming's).
                    let line = [by, Some(format!("in {} as {}", paired_in(v, &s.library_info), v.library)), Some(v.why.clone()).filter(|w| !w.is_empty())]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · ");
                    f.items.push(Item { title, line: Some(line), album: Some((s.id.clone(), id.clone())), ..Item::default() });
                }
            }
            f.items.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        }
        Ok(out)
    }

    /// How far AudioMuse has heard the library (decision 0054): while it has not heard all of it.
    async fn check_analysis(&self) -> anyhow::Result<Vec<Finding>> {
        let cat = self.library.catalogue();
        let facts = tokio::task::spawn_blocking(move || cat.crate_facts()).await??;
        let tracks = facts.iter().filter(|f| f.track.is_some()).count() as u64;
        let heard = facts.iter().filter(|f| f.track.is_some() && f.sound.is_some()).count() as u64;
        if tracks == 0 || heard >= tracks {
            return Ok(Vec::new());
        }
        let mut f = finding("analysis", "analysis:audiomuse", Level::Info, heard, format!("{} of {} tracks analysed", thousands(heard), thousands(tracks)));
        f.of = Some(tracks);
        // The album folders with the most tracks not heard yet.
        let mut by: HashMap<&str, (u64, u64)> = HashMap::new();
        let mut unheard: HashMap<&str, Vec<String>> = HashMap::new();
        for x in facts.iter().filter(|x| x.track.is_some()) {
            let (dir, file) = x.rel.rsplit_once('/').unwrap_or(("", x.rel.as_str()));
            let e = by.entry(dir).or_default();
            e.1 += 1;
            if x.sound.is_none() {
                e.0 += 1;
                let names = unheard.entry(dir).or_default();
                if names.len() < FILES {
                    names.push(file.to_string());
                }
            }
        }
        let mut left: Vec<_> = by.into_iter().filter(|(_, (not, _))| *not > 0).collect();
        left.sort_by(|a, b| b.1.0.cmp(&a.1.0).then_with(|| a.0.cmp(b.0)));
        f.detail = Some(format!("{} not heard yet, in {}", n(tracks - heard, "track", "tracks"), n(left.len() as u64, "folder", "folders")));
        f.items = left
            .into_iter()
            .take(ITEMS)
            .map(|(dir, (not, all))| {
                let mut files = unheard.remove(dir).unwrap_or_default();
                files.sort();
                Item { title: dir.rsplit('/').next().unwrap_or(dir).trim_start_matches("../").to_string(), line: Some(format!("{not} of {all} not heard · {dir}")), files, ..Item::default() }
            })
            .collect();
        Ok(vec![f])
    }

    /// Each server switched on that cannot be reached, and Lidarr; and settings that cannot be saved (no data folder:
    /// About showed it until the owner, 2026-10-24).
    async fn check_servers(&self) -> anyhow::Result<Vec<Finding>> {
        let view = self.settings.view().await?;
        let settings = || Some(Open { page: "settings", switch: None, thing: None });
        let mut out = Vec::new();
        if view.settings_file.is_none() {
            let mut f = finding("server", "server:settings-file", Level::Critical, 1, "Settings can't be saved");
            f.detail = Some("Cratarr has no data folder: set CRATARR_DATA_DIR.".into());
            out.push(f);
        }
        for s in view.sources.iter().filter(|s| s.enabled && !s.status.connected) {
            let mut f = finding("servers", format!("servers:{}", s.id), Level::Critical, 1, format!("{} can't be reached", s.label));
            f.thing = Some(s.label.to_string());
            f.detail = s.status.error.clone().or_else(|| s.error.clone());
            f.open = settings();
            out.push(f);
        }
        if let Some(l) = view.lidarr.as_ref().filter(|l| l.error.is_some()) {
            let mut f = finding("servers", "servers:lidarr", Level::Warning, 1, "Lidarr can't be reached");
            f.detail = l.error.clone();
            f.open = settings();
            out.push(f);
        }
        Ok(out)
    }

    /// Cratarr's own players: a sound held back, and one of the app's not heard from for a day (but within a month:
    /// one long gone is no news).
    fn check_players(&self) -> Vec<Finding> {
        let at = now();
        let mut out = Vec::new();
        for p in self.play_on.list() {
            if let Some(why) = &p.held {
                let mut f = finding("players", format!("players:held:{}", p.name), Level::Warning, 1, format!("{} held back a play", p.name));
                f.thing = Some(p.name.clone());
                f.detail = Some(why.clone());
                out.push(f);
            }
            let away = at - p.seen as i64;
            if !p.online && p.kind == "app" && (24 * 3600..30 * 24 * 3600).contains(&away) {
                let days = away / (24 * 3600);
                // No count: a player is silent or not ("Salon · silent"; the page leaves a 0 out).
                let mut f = finding("players", format!("players:away:{}", p.name), Level::Info, 0, format!("{} not heard from for {}", p.name, n(days as u64, "day", "days")));
                f.thing = Some(p.name.clone());
                out.push(f);
            }
        }
        out
    }
}

/// The last hour of an event, as the diagnostics log counted it.
fn last_hour(area: &str, what: &str, at: i64) -> anyhow::Result<u64> {
    let Some(d) = crate::diagnostics::get() else { return Ok(0) };
    d.count_since(area, what, (at - 3600) * 1000)
}

/// The last hour's events of a kind, grouped by `by` (a field, as the log keeps it): per group how often, the largest
/// `ms`, and the clients; the most first.
fn grouped(area: &str, what: &str, by: &str, at: i64) -> anyhow::Result<Vec<Item>> {
    let Some(d) = crate::diagnostics::get() else { return Ok(Vec::new()) };
    let rows = d.query(&crate::diagnostics::Query { area: Some(area.into()), since: Some((at - 3600) * 1000), limit: Some(1000), ..Default::default() })?;
    let mut groups: HashMap<String, (i64, u64, std::collections::BTreeSet<String>)> = HashMap::new();
    for r in rows.iter().filter(|r| r.what == what) {
        let key = r.fields.get(by).and_then(|v| v.as_str().map(str::to_string).or_else(|| Some(v.to_string()))).unwrap_or_else(|| "—".into());
        let g = groups.entry(key).or_default();
        g.0 += r.count;
        g.1 = g.1.max(r.fields.get("ms").and_then(serde_json::Value::as_u64).unwrap_or(0));
        if let Some(c) = r.fields.get("client").and_then(|v| v.as_str()) {
            g.2.insert(c.to_string());
        }
    }
    let mut list: Vec<_> = groups.into_iter().collect();
    list.sort_by(|a, b| b.1.0.cmp(&a.1.0).then_with(|| a.0.cmp(&b.0)));
    Ok(list
        .into_iter()
        .take(ITEMS)
        .map(|(key, (count, ms, clients))| {
            let mut line = vec![format!("{count}×")];
            if ms > 0 {
                line.push(format!("up to {ms} ms"));
            }
            if !clients.is_empty() {
                line.push(format!("from {}", clients.into_iter().collect::<Vec<_>>().join(", ")));
            }
            Item { title: key, line: Some(line.join(" · ")), ..Item::default() }
        })
        .collect())
}

fn check_sign_in(at: i64) -> anyhow::Result<Vec<Finding>> {
    let refused = last_hour("signin", "request refused", at)?;
    let throttled = last_hour("signin", "sign-in throttled", at)?;
    let mut out = Vec::new();
    if refused > 0 {
        let mut f = finding("sign-in", "sign-in:refused", Level::Warning, refused, format!("{} in the last hour", n(refused, "request refused", "requests refused")));
        f.items = grouped("signin", "request refused", "path", at)?;
        out.push(f);
    }
    if throttled > 0 {
        let mut f = finding("sign-in", "sign-in:throttled", Level::Warning, throttled, format!("{} in the last hour", n(throttled, "sign-in throttled", "sign-ins throttled")));
        f.items = grouped("signin", "sign-in throttled", "client", at)?;
        out.push(f);
    }
    Ok(out)
}

/// The server's own: slow answers and runaway clients (decision 0045, amended 2026-10-23).
fn check_server(at: i64) -> anyhow::Result<Vec<Finding>> {
    let slow = last_hour("server", "slow response", at)?;
    let runaway = last_hour("server", "client requests too often", at)?;
    let mut out = Vec::new();
    if slow > 0 {
        let mut f = finding("server", "server:slow", Level::Info, slow, format!("{} in the last hour", n(slow, "slow answer", "slow answers")));
        f.detail = Some("Answers that took longer than a second, by page: how often, the slowest, and who asked.".into());
        f.items = grouped("server", "slow response", "path", at)?;
        out.push(f);
    }
    if runaway > 0 {
        let mut f = finding("server", "server:runaway", Level::Warning, runaway, format!("A client asked far too often in the last hour ({runaway}×)"));
        f.items = grouped("server", "client requests too often", "client", at)?;
        out.push(f);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_grouped_by_thousands() {
        assert_eq!((thousands(7), thousands(2264), thousands(31263), thousands(1_000_000)), ("7".into(), "2,264".into(), "31,263".into(), "1,000,000".into()));
    }

    #[test]
    fn every_finding_has_a_short_name() {
        for key in ["files:broken", "library:cover", "library:track-numbers", "library:discs", "incoming:upgrades", "server:slow", "servers:jellyfin", "players:away:Tablet", "library:check"] {
            assert_ne!(name_of(key), "Finding", "{key}");
        }
        assert_eq!(finding("library", "library:genre", Level::Warning, 12, "12 records without a genre").name, "Missing genres");
    }

    #[test]
    fn every_finding_has_a_row_of_thing_and_state() {
        for key in ["files:broken", "files:no_tags", "library:incomplete", "incoming:failed", "server:slow", "servers:jellyfin", "players:held:Tablet", "players:away:Tablet", "library:check"] {
            assert!(label_of(key).is_some(), "{key}");
        }
        let f = finding("library", "library:cover", Level::Warning, 3, "3 records without a cover");
        assert_eq!((f.thing.as_deref(), f.state.as_deref()), (Some("Covers"), Some("missing")));
        assert_eq!(label_of("files:something-new"), None, "the page falls back on the name");
    }

    #[test]
    fn findings_are_worst_first_then_newest_and_keep_their_first_sight() {
        let f = |key: &str, level| finding("files", key, level, 1, key);
        let first = ordered(vec![f("a", Level::Info), f("b", Level::Critical)], 100);
        assert_eq!(first.iter().map(|x| x.key.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        // Found again later, with a new one of the same level: the older keeps its first sight, the newer comes first.
        let again = ordered(vec![f("a", Level::Info), f("c", Level::Info), f("b", Level::Critical)], 200);
        assert_eq!(again.iter().map(|x| (x.key.as_str(), x.since)).collect::<Vec<_>>(), [("b", 100), ("c", 200), ("a", 100)]);
        // Gone, then back: found anew.
        ordered(vec![f("b", Level::Critical)], 300);
        let back = ordered(vec![f("a", Level::Info)], 400);
        assert_eq!(back[0].since, 400);
    }

    #[test]
    fn the_files_and_incoming_say_what_needs_the_owner() {
        let issues: BTreeMap<String, u64> = [("broken".to_string(), 2), ("no_length".to_string(), 1), ("no_tags".to_string(), 0)].into();
        let found = files(&issues);
        assert_eq!(found.iter().map(|f| (f.title.as_str(), f.level)).collect::<Vec<_>>(), [("2 files can't be read", Level::Critical), ("1 file has no length", Level::Info)]);
        let waiting = incoming(13, 0, 1);
        assert_eq!(waiting[0].title, "13 better copies wait to replace Music's");
        assert_eq!(waiting[0].open, None);
        assert_eq!(waiting[1].title, "1 conversion failed");
        assert_eq!(incoming(0, 17, 0)[0].title, "17 records are in two libraries at once");
        assert_eq!(incoming(1, 1, 0).iter().map(|f| f.title.as_str()).collect::<Vec<_>>(), ["1 better copy waits to replace Music's", "1 record is in two libraries at once"]);
        assert!(incoming(0, 0, 0).is_empty());
    }

    #[test]
    fn every_librarys_albums_are_found_each_opening_its_own_record() {
        // An Incoming album (folder 1, its id its own: `serve::ids::album`) beside Music's of the same name.
        let lacks = |id: &str, folder: i64| crate::serve::library::Lacks {
            id: id.into(),
            name: "Zebra".into(),
            artist: "Arp".into(),
            folder,
            cover: true,
            year: false,
            genre: false,
            album_artist: false,
            track_no: false,
            musicbrainz: false,
        };
        let numbers = [("al-in".to_string(), "Heads".to_string(), "Ott".to_string(), "01 twice".to_string())];
        let found = library(&[lacks("al-music", 0), lacks("al-in", 1)], &numbers, &[]);
        assert_eq!(found.iter().map(|f| (f.key.as_str(), f.title.as_str())).collect::<Vec<_>>(), [("library:cover", "2 records without a cover"), ("library:track-numbers", "1 record with track numbers off")]);
        let opens = |f: &Finding| f.items.iter().map(|i| i.album.clone().unwrap().1).collect::<Vec<_>>();
        assert_eq!(opens(&found[0]), ["al-music", "al-in"]);
        assert_eq!(opens(&found[1]), ["al-in"]);
    }

    #[test]
    fn a_records_health_is_its_own_findings_and_files_and_no_other_records() {
        let own = ("cratarr".to_string(), "al-own".to_string());
        let item = |album: &str, title: &str| Item { title: title.into(), album: Some(("cratarr".into(), album.into())), ..Item::default() };
        let mut cover = finding("library", "library:cover", Level::Critical, 2, "2 records without a cover");
        cover.items = vec![item("al-other", "Other"), item("al-own", "Own")];
        let mut genre = finding("library", "library:genre", Level::Warning, 1, "1 record without a genre");
        genre.items = vec![item("al-other", "Other")];
        let mut cut = finding("files", "files:cut-short", Level::Critical, 2, "2 files are cut off");
        cut.detail = Some("Their files hold less than they say.".into());
        cut.items = vec![Item { track: Some("tr-3".into()), ..item("al-own", "Three") }, Item { track: Some("tr-9".into()), ..item("al-other", "Nine") }];
        // The same id on another server is another record.
        let mut failed = finding("incoming", "incoming:failed", Level::Warning, 1, "1 conversion failed");
        failed.items = vec![Item { album: Some(("jellyfin".into(), "al-own".into())), ..Item::default() }];
        let wrong = [("damaged_tag".to_string(), Item { track: Some("tr-1".into()), ..item("al-own", "One") }), ("no_length".to_string(), Item { track: Some("tr-2".into()), ..item("al-own", "Two") })];
        let health = record(&[cover, genre, cut, failed], &own, &wrong);
        let rows: Vec<(&str, Option<&str>, Option<&str>, Vec<&str>)> =
            health.findings.iter().map(|f| (f.key.as_str(), f.thing.as_deref(), f.state.as_deref(), f.items.iter().map(|i| i.title.as_str()).collect())).collect();
        assert_eq!(
            rows,
            [
                ("files:cut-short", Some("Files"), Some("cut off"), vec!["Three"]),
                ("library:cover", Some("Covers"), Some("missing"), vec!["Own"]),
                ("files:damaged_tag", Some("Tags"), Some("damaged"), vec!["One"]),
                ("files:no_length", Some("Lengths"), Some("missing"), vec!["Two"]),
            ]
        );
        assert_eq!(health.findings[0].detail.as_deref(), Some("Their files hold less than they say."));
        assert!(record(&[], &own, &[]).findings.is_empty());
    }

    #[test]
    fn a_records_gaps_reach_up_to_the_total_its_tags_agree_on() {
        let one_disc = |tracks: &[u32]| tracks.iter().map(|t| (Some(1), Some(*t))).collect::<Vec<_>>();
        let numbers = one_disc(&[1, 2, 3, 5]);
        assert_eq!(gaps_in(&numbers, &[6, 6, 6, 6]), ["04", "06"], "05/06: 06 is missing too");
        assert_eq!(gaps_in(&numbers, &[]), ["04"], "without a total, as before");
        assert_eq!(gaps_in(&numbers, &[5, 5]), ["04"], "a total no higher than the numbers adds nothing");
        assert_eq!(gaps_in(&numbers, &[6, 6, 6, 12]), ["04", "06"], "one track's slip is not the record's total");
        assert_eq!(gaps_in(&one_disc(&[1, 2, 3]), &[4]), ["04"], "one track saying it, the others none");
        assert!(gaps_in(&one_disc(&[1, 2, 3, 4, 5, 6]), &[6; 6]).is_empty(), "whole");
        let two_discs = [(Some(1), Some(1)), (Some(1), Some(2)), (Some(2), Some(1))];
        assert!(gaps_in(&two_discs, &[3, 3, 3]).is_empty(), "several discs: not this check's");
        assert!(gaps_in(&[(None, None), (None, None)], &[6, 6]).is_empty(), "numbered nowhere");
    }

    #[test]
    fn an_incoming_album_says_which_library_its_other_copy_is_in() {
        use crate::app::incoming::VerdictView;
        let v = |kind: &'static str, library_id: Option<&str>, replaceable: bool| VerdictView {
            kind,
            library: "Arp/Zebra".into(),
            why: String::new(),
            library_id: library_id.map(Into::into),
            replaceable,
        };
        let libraries = [crate::app::albums::LibraryView { id: "new".into(), name: "New music".into(), use_: "incoming", colour: None, quality: crate::settings::Quality::None }];
        assert!(replaces(&v("upgrade", Some("music"), true)));
        assert!(!replaces(&v("upgrade", Some("new"), false)), "a better copy of another Incoming library's: in two libraries at once");
        assert!(!replaces(&v("duplicate", Some("music"), true)));
        assert_eq!(paired_in(&v("duplicate", Some("music"), true), &libraries), "Music");
        assert_eq!(paired_in(&v("upgrade", Some("new"), false), &libraries), "New music");
        assert_eq!(paired_in(&v("upgrade", None, false), &libraries), "another library");
    }
}
