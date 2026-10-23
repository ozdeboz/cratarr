//! One health report (decision 0063): every domain's check says what it finds now, in one shape; the Health page
//! shows it all, Home's Needs you the most critical and the most recent. A check that fails is a finding itself, never a
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
    /// The owner has to act.
    NeedsYou,
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
}

/// What a check finds.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub domain: &'static str,
    /// Stable while the same thing is found (its first sight is kept by it).
    pub key: String,
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
    Finding { domain, key: key.into(), title: title.into(), detail: None, count, of: None, level, since: 0, open: None }
}

fn n(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
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
    let health = || Some(Open { page: "health", switch: None });
    let mut out = Vec::new();
    for (kind, &count) in issues {
        if count == 0 {
            continue;
        }
        let (level, title) = match kind.as_str() {
            "broken" => (Level::NeedsYou, n(count, "file can't be read", "files can't be read")),
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

/// Incoming's albums as they stand to Music (decision 0034), and the conversions that failed.
pub fn incoming(upgrades: u64, duplicates: u64, failed: u64) -> Vec<Finding> {
    let library = |switch| Some(Open { page: "library", switch: Some(switch) });
    let mut out = Vec::new();
    if upgrades > 0 {
        let mut f = finding("incoming", "incoming:upgrades", Level::NeedsYou, upgrades, n(upgrades, "upgrade waiting", "upgrades waiting"));
        f.open = library("upgrades");
        out.push(f);
    }
    if duplicates > 0 {
        let mut f = finding("incoming", "incoming:duplicates", Level::NeedsYou, duplicates, n(duplicates, "duplicate waiting", "duplicates waiting"));
        f.open = library("duplicates");
        out.push(f);
    }
    if failed > 0 {
        let mut f = finding("incoming", "incoming:failed", Level::Warning, failed, n(failed, "conversion failed", "conversions failed"));
        f.open = Some(Open { page: "library", switch: None });
        out.push(f);
    }
    out
}

impl App {
    /// Every domain's findings now (decision 0063), worst first, the most recent first among equals.
    pub async fn health_report(&self) -> Report {
        let at = now();
        let (files, playlists, incoming, analysis, servers) =
            tokio::join!(self.check_files(), self.check_playlists(), self.check_incoming(), self.check_analysis(), self.check_servers());
        let mut findings = Vec::new();
        let mut checked = BTreeMap::new();
        for (domain, found) in [("files", files), ("playlists", playlists), ("incoming", incoming), ("analysis", analysis), ("servers", servers)] {
            findings.extend(or_failed(domain, found));
            checked.insert(domain, at);
        }
        findings.extend(self.check_players());
        checked.insert("players", at);
        for (domain, found) in [("sign-in", check_sign_in(at)), ("server", check_server(at))] {
            findings.extend(or_failed(domain, found));
            checked.insert(domain, at);
        }
        Report { findings: ordered(findings, at), checked }
    }

    async fn check_files(&self) -> anyhow::Result<Vec<Finding>> {
        let c = self.library.catalogue();
        let status = tokio::task::spawn_blocking(move || c.status()).await??;
        Ok(files(&status.issues))
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
        f.detail = Some(broken.iter().take(5).map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "));
        f.open = Some(Open { page: "playlists", switch: None });
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
        let failed = self.incoming.jobs().iter().filter(|j| serde_json::to_value(&j.state).ok().and_then(|v| v.as_str().map(|s| s == "failed")) == Some(true)).count() as u64;
        Ok(incoming(upgrades, duplicates, failed))
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
        let mut f = finding("analysis", "analysis:audiomuse", Level::Info, heard, format!("AudioMuse has heard {heard} of {tracks}"));
        f.of = Some(tracks);
        Ok(vec![f])
    }

    /// Each server switched on that cannot be reached, and Lidarr.
    async fn check_servers(&self) -> anyhow::Result<Vec<Finding>> {
        let view = self.settings.view().await?;
        let settings = || Some(Open { page: "settings", switch: None });
        let mut out = Vec::new();
        for s in view.sources.iter().filter(|s| s.enabled && !s.status.connected) {
            let mut f = finding("servers", format!("servers:{}", s.id), Level::NeedsYou, 1, format!("{} can't be reached", s.label));
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

fn check_sign_in(at: i64) -> anyhow::Result<Vec<Finding>> {
    let refused = last_hour("signin", "request refused", at)?;
    let throttled = last_hour("signin", "sign-in throttled", at)?;
    let mut out = Vec::new();
    if refused > 0 {
        out.push(finding("sign-in", "sign-in:refused", Level::Warning, refused, format!("{} in the last hour", n(refused, "request refused", "requests refused"))));
    }
    if throttled > 0 {
        out.push(finding("sign-in", "sign-in:throttled", Level::Warning, throttled, format!("{} in the last hour", n(throttled, "sign-in throttled", "sign-ins throttled"))));
    }
    Ok(out)
}

/// The server's own: slow answers and runaway clients (decision 0045, amended 2026-10-23).
fn check_server(at: i64) -> anyhow::Result<Vec<Finding>> {
    let slow = last_hour("server", "slow response", at)?;
    let runaway = last_hour("server", "client requests too often", at)?;
    let mut out = Vec::new();
    if slow > 0 {
        out.push(finding("server", "server:slow", Level::Info, slow, format!("{} in the last hour", n(slow, "slow answer", "slow answers"))));
    }
    if runaway > 0 {
        out.push(finding("server", "server:runaway", Level::Warning, runaway, format!("A client asked far too often in the last hour ({runaway}×)")));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn findings_are_worst_first_then_newest_and_keep_their_first_sight() {
        let f = |key: &str, level| finding("files", key, level, 1, key);
        let first = ordered(vec![f("a", Level::Info), f("b", Level::NeedsYou)], 100);
        assert_eq!(first.iter().map(|x| x.key.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        // Found again later, with a new one of the same level: the older keeps its first sight, the newer comes first.
        let again = ordered(vec![f("a", Level::Info), f("c", Level::Info), f("b", Level::NeedsYou)], 200);
        assert_eq!(again.iter().map(|x| (x.key.as_str(), x.since)).collect::<Vec<_>>(), [("b", 100), ("c", 200), ("a", 100)]);
        // Gone, then back: found anew.
        ordered(vec![f("b", Level::NeedsYou)], 300);
        let back = ordered(vec![f("a", Level::Info)], 400);
        assert_eq!(back[0].since, 400);
    }

    #[test]
    fn the_files_and_incoming_say_what_needs_the_owner() {
        let issues: BTreeMap<String, u64> = [("broken".to_string(), 2), ("no_length".to_string(), 1), ("no_tags".to_string(), 0)].into();
        let found = files(&issues);
        assert_eq!(found.iter().map(|f| (f.title.as_str(), f.level)).collect::<Vec<_>>(), [("2 files can't be read", Level::NeedsYou), ("1 file has no length", Level::Info)]);
        let waiting = incoming(13, 0, 1);
        assert_eq!(waiting[0].title, "13 upgrades waiting");
        assert_eq!(waiting[0].open, Some(Open { page: "library", switch: Some("upgrades") }));
        assert_eq!(waiting[1].title, "1 conversion failed");
        assert!(incoming(0, 0, 0).is_empty());
    }
}
