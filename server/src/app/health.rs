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

/// At most this many items a finding lists.
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

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// When each finding (by key) was first found; those no longer found are forgotten.
static FIRST_SEEN: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);

/// A finding as a check makes it; its time is set by the report.
fn finding(domain: &'static str, key: impl Into<String>, level: Level, count: u64, title: impl Into<String>) -> Finding {
    let key = key.into();
    Finding { domain, name: name_of(&key).into(), key, title: title.into(), detail: None, count, of: None, level, since: 0, open: None, items: Vec::new() }
}

/// A finding's short name, by its key (the breadcrumb's; its title stays the sentence).
fn name_of(key: &str) -> &'static str {
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
        "library:musicbrainz" => "Missing MusicBrainz ids",
        "library:incomplete" => "Albums missing tracks",
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
pub fn library(rows: &[crate::serve::library::Lacks], numbering: &[(String, String, String, String)]) -> Vec<Finding> {
    type Pick = fn(&crate::serve::library::Lacks) -> bool;
    let kinds: [(&str, Level, &str, &str, Pick); 5] = [
        ("cover", Level::Critical, "album without a cover", "albums without a cover", |r| r.cover),
        ("year", Level::Warning, "album without a year", "albums without a year", |r| r.year),
        ("genre", Level::Warning, "album without a genre", "albums without a genre", |r| r.genre),
        ("album-artist", Level::Warning, "album without an album artist", "albums without an album artist", |r| r.album_artist),
        ("musicbrainz", Level::Info, "album without its MusicBrainz id", "albums without their MusicBrainz id", |r| r.musicbrainz),
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
            .take(ITEMS)
            .map(|r| Item { title: r.name.clone(), line: Some(r.artist.clone()), album: Some((crate::source::cratarr::KIND.id.to_string(), r.id.clone())), ..Item::default() })
            .collect();
        out.push(f);
    }
    if !numbering.is_empty() {
        let mut hit: Vec<&(String, String, String, String)> = numbering.iter().collect();
        hit.sort_by(|a, b| a.2.to_lowercase().cmp(&b.2.to_lowercase()).then_with(|| a.1.cmp(&b.1)));
        let count = hit.len() as u64;
        let mut f = finding("library", "library:track-numbers", Level::Warning, count, n(count, "album with track numbers off", "albums with track numbers off"));
        f.items = hit
            .iter()
            .take(ITEMS)
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

/// Incoming's albums as they stand to Music (decision 0034), and the conversions that failed. Dealt with on their items'
/// own pages (the comparison, the album's menu), not the Library, whose own filter could hide them (the owner,
/// 2026-10-24): no `open`.
pub fn incoming(upgrades: u64, duplicates: u64, failed: u64) -> Vec<Finding> {
    let mut out = Vec::new();
    if upgrades > 0 {
        // Said as what the library holds (the owner, 2026-10-25: duplicates on an artist's page went unnoticed under
        // "duplicates waiting").
        out.push(finding("incoming", "incoming:upgrades", Level::Critical, upgrades, n(upgrades, "better copy waits to replace Music's", "better copies wait to replace Music's")));
    }
    if duplicates > 0 {
        out.push(finding("incoming", "incoming:duplicates", Level::Critical, duplicates, n(duplicates, "album is in Music and Incoming at once", "albums are in Music and Incoming at once")));
    }
    if failed > 0 {
        out.push(finding("incoming", "incoming:failed", Level::Warning, failed, n(failed, "conversion failed", "conversions failed")));
    }
    out
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
        Report { findings: ordered(findings, at), checked }
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
            f.items = items.into_iter().take(ITEMS).map(|(i, _)| i).collect();
            out.push(f);
        }
        Ok(out)
    }

    /// Music's albums lacking what the library requires (the owner, 2026-10-24): Incoming's are marked there instead.
    async fn check_library(&self) -> anyhow::Result<Vec<Finding>> {
        let Some(served) = self.served.clone() else { return Ok(Vec::new()) };
        let music = self.library.folders().music;
        let folders = self.library.catalogue().folders();
        let music = music.and_then(|m| folders.iter().position(|f| f == std::path::Path::new(&m))).map(|i| i as i64);
        let (lib, ud) = (served.library.clone(), served.userdata.clone());
        let (rows, numbering, matched) =
            tokio::task::spawn_blocking(move || -> anyhow::Result<_> { Ok((lib.lacking()?, lib.numbering(music)?, ud.renumber_matched()?)) }).await??;
        let off: Vec<(String, String, String, String)> = numbering
            .into_iter()
            .filter_map(|(id, name, artist, numbers)| super::renumber::off(&numbers, !matched.contains(&id)).map(|line| (id, name, artist, line)))
            .collect();
        let mut out = library(&rows.into_iter().filter(|r| music.is_none_or(|m| r.folder == m)).collect::<Vec<_>>(), &off);
        out.extend(self.check_incomplete(&served, music).await?);
        // Albums whose files a rule names otherwise (0.9.7): their fix writes the rules' names into the files.
        let s = served.clone();
        let ruled = tokio::task::spawn_blocking(move || super::fixes::ruled_albums(&s)).await??;
        if !ruled.is_empty() {
            let count = ruled.len() as u64;
            let s = served.clone();
            let items = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<Item>> {
                let mut items = Vec::new();
                for a in ruled.iter().take(ITEMS) {
                    if let Some((_, _, _, artist, name)) = s.library.album_place(a)? {
                        items.push(Item { title: name, line: Some(artist), album: Some((crate::source::cratarr::KIND.id.to_string(), a.clone())), ..Item::default() });
                    }
                }
                Ok(items)
            })
            .await??;
            let mut f = finding("library", "library:rules", Level::Info, count, n(count, "album a rule names otherwise", "albums a rule names otherwise"));
            f.items = items;
            out.push(f);
        }
        Ok(out)
    }

    /// Albums missing tracks (0.9.7, `app::incomplete`): each with what its release has beyond it and whether it is
    /// asked for already; and the albums whose numbers have gaps that no release could be held against yet.
    async fn check_incomplete(&self, served: &std::sync::Arc<crate::serve::Serve>, music: Option<i64>) -> anyhow::Result<Vec<Finding>> {
        let s = served.clone();
        let (found, gaps) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let mut found: Vec<(String, String, String, super::incomplete::Incomplete)> = Vec::new();
            for (album, json) in s.userdata.albums_incomplete()? {
                let Ok(i) = serde_json::from_str::<super::incomplete::Incomplete>(&json) else { continue };
                let Some((folder, _, _, artist, name)) = s.library.album_place(&album)? else { continue };
                if music.is_some_and(|m| folder != m) {
                    continue;
                }
                found.push((album, name, artist, i));
            }
            // Gaps in the numbers where no release was held against the album (no MusicBrainz id, no match yet).
            let checked = s.userdata.album_complete_at()?;
            let mut gaps = Vec::new();
            for (id, name, artist, numbers) in s.library.numbering(music)? {
                if checked.contains_key(&id) {
                    continue;
                }
                let mut seen: Vec<u32> = numbers.iter().filter_map(|(_, t)| *t).filter(|t| *t > 0).collect();
                seen.sort_unstable();
                seen.dedup();
                let top = seen.last().copied().unwrap_or(0);
                let missing: Vec<String> = (1..top).filter(|n| !seen.contains(n)).map(|n| format!("{n:02}")).collect();
                if !missing.is_empty() && numbers.iter().all(|(d, _)| d.unwrap_or(1) <= 1) {
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
        let mut f = finding("library", "library:incomplete", Level::Warning, count, n(count, "album missing tracks", "albums missing tracks"));
        f.items = items.into_iter().take(ITEMS).collect();
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
            match v.kind {
                "upgrade" => upgrades += 1,
                "duplicate" => duplicates += 1,
                _ => {}
            }
        }
        let jobs: Vec<_> = self.incoming.jobs().into_iter().filter(|j| serde_json::to_value(&j.state).ok().and_then(|v| v.as_str().map(|s| s == "failed")) == Some(true)).collect();
        let mut out = incoming(upgrades, duplicates, jobs.len() as u64);
        // Each album waiting, with the Music album it upgrades or duplicates; each conversion failed, with why.
        for f in &mut out {
            let kind = match f.key.as_str() {
                "incoming:upgrades" => "upgrade",
                "incoming:duplicates" => "duplicate",
                "incoming:failed" => {
                    f.items = jobs
                        .iter()
                        .take(ITEMS)
                        .map(|j| Item { title: j.title.clone(), line: j.why.clone(), album: Some((j.source.clone(), j.album_id.clone())), ..Item::default() })
                        .collect();
                    continue;
                }
                _ => continue,
            };
            for s in &view.sources {
                for (id, v) in s.verdicts.iter().filter(|(_, v)| v.kind == kind) {
                    let a = s.albums.iter().find(|a| &a.id == id);
                    let title = a.map_or_else(|| id.clone(), |a| a.title.clone());
                    let by = a.and_then(|a| a.artist.clone());
                    // Both places: the copy in Music, as Incoming's verdict names it (this one opens Incoming's).
                    let line = [by, Some(format!("in Music as {}", v.library)), Some(v.why.clone()).filter(|w| !w.is_empty())]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · ");
                    f.items.push(Item { title, line: Some(line), album: Some((s.id.clone(), id.clone())), ..Item::default() });
                }
            }
            f.items.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
            f.items.truncate(ITEMS);
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
                f.detail = Some(why.clone());
                out.push(f);
            }
            let away = at - p.seen as i64;
            if !p.online && p.kind == "app" && (24 * 3600..30 * 24 * 3600).contains(&away) {
                let days = away / (24 * 3600);
                out.push(finding("players", format!("players:away:{}", p.name), Level::Info, 1, format!("{} not heard from for {}", p.name, n(days as u64, "day", "days"))));
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
        for key in ["files:broken", "library:cover", "library:track-numbers", "incoming:upgrades", "server:slow", "servers:jellyfin", "players:away:Tablet", "library:check"] {
            assert_ne!(name_of(key), "Finding", "{key}");
        }
        assert_eq!(finding("library", "library:genre", Level::Warning, 12, "12 albums without a genre").name, "Missing genres");
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
        assert_eq!(incoming(0, 17, 0)[0].title, "17 albums are in Music and Incoming at once");
        assert_eq!(incoming(1, 1, 0).iter().map(|f| f.title.as_str()).collect::<Vec<_>>(), ["1 better copy waits to replace Music's", "1 album is in Music and Incoming at once"]);
        assert!(incoming(0, 0, 0).is_empty());
    }
}
