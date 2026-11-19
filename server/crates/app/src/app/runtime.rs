//! Settings that were the environment's only (0.9.8, the owner: what blocks others from running Cratarr): the proxies
//! whose forwarded headers are believed, the analysis pass, Beat this!, and the host players reach sync play on. The
//! environment seeds them; a value saved in Settings wins from then on (kept as server choices, `runtime:*`). Each one
//! applies at once: the proxies' screen, the analysis pass (woken), sync play's address (Play On's). The home networks
//! a shared device works from (`serve::proxy::at_home`) are Settings' only, none by default; so is the guest portal
//! (decision 0079), off by default.

use std::sync::{OnceLock, RwLock};

use anyhow::Result;
use ipnet::IpNet;
use serde::{Deserialize, Serialize};

use super::analysis_pass::Mode;

const PROXIES: &str = "runtime:proxies";
const ANALYSIS: &str = "runtime:analysis";
const BEAT_THIS: &str = "runtime:beat_this";
const SNAPCAST_HOST: &str = "runtime:snapcast_host";
const HOME_NETWORKS: &str = "runtime:home_networks";
const GUEST: &str = "runtime:guest";

/// The guest portal (decision 0079; the owner, 2026-10-30): `/guest`, at home, without an account, on or off, and whose
/// queue its requests go to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guest {
    #[serde(default)]
    pub on: bool,
    #[serde(default)]
    pub host: Option<GuestHost>,
}

/// A guest's host: a user (the player they listen on then), or one player (a shared device: its owner answers).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuestHost {
    User(String),
    Player(String),
}

/// The settings in force.
#[derive(Debug, Clone, PartialEq)]
pub struct Runtime {
    /// None: local peers only (`serve::proxy`).
    pub proxies: Option<Vec<IpNet>>,
    pub analysis: Mode,
    pub beat_this: bool,
    pub snapcast_host: Option<String>,
    /// Home besides the local addresses.
    pub home_networks: Vec<IpNet>,
    pub guest: Guest,
}

/// What the environment said at the start (none: it said nothing).
#[derive(Debug, Clone, Default)]
pub struct Seed {
    pub proxies: Option<Vec<IpNet>>,
    pub analysis: Option<Mode>,
    pub beat_this: Option<bool>,
    pub snapcast_host: Option<String>,
}

impl Seed {
    pub fn from_env(env: &impl Fn(&str) -> Option<String>) -> Self {
        let set = |k: &str| env(k).filter(|v| !v.trim().is_empty());
        Seed {
            proxies: crate::serve::proxy::parse(set("CRATARR_PROXIES").as_deref()),
            analysis: set("CRATARR_ANALYSIS").map(|v| Mode::from_env(Some(&v))),
            beat_this: set("CRATARR_BEAT_THIS").map(|v| super::analysis_pass::hears_beats(Some(&v))),
            snapcast_host: set("CRATARR_SNAPCAST_HOST"),
        }
    }
}

/// What Settings saved (none: not saved, the environment's stands).
#[derive(Debug, Clone, Default, PartialEq)]
struct Saved {
    proxies: Option<Vec<IpNet>>,
    proxies_set: bool,
    analysis: Option<Mode>,
    beat_this: Option<bool>,
    snapcast_host: Option<Option<String>>,
    home_networks: Vec<IpNet>,
    guest: Guest,
}

static SEED: OnceLock<Seed> = OnceLock::new();
/// Cratarr's own port, for the address players reach it on beside sync play.
static PORT: OnceLock<String> = OnceLock::new();
static NOW: RwLock<Option<Runtime>> = RwLock::new(None);
/// Wakes the analysis pass when its settings change.
pub static CHANGED: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// The settings in force (the environment's until `start`).
pub fn now() -> Runtime {
    if let Some(r) = NOW.read().unwrap_or_else(|p| p.into_inner()).clone() {
        return r;
    }
    combine(SEED.get().cloned().unwrap_or_default(), &Saved::default())
}

fn combine(seed: Seed, saved: &Saved) -> Runtime {
    Runtime {
        proxies: if saved.proxies_set { saved.proxies.clone() } else { seed.proxies },
        analysis: saved.analysis.or(seed.analysis).unwrap_or(Mode::Keep),
        beat_this: saved.beat_this.or(seed.beat_this).unwrap_or(false),
        snapcast_host: saved.snapcast_host.clone().unwrap_or(seed.snapcast_host),
        home_networks: saved.home_networks.clone(),
        guest: saved.guest.clone(),
    }
}

fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::Keep => "keep",
        Mode::All => "all",
        Mode::Off => "off",
    }
}

fn read_saved(ud: &crate::userdata::UserData) -> Result<Saved> {
    let proxies = ud.import_choice(PROXIES)?;
    Ok(Saved {
        proxies_set: proxies.is_some(),
        proxies: proxies.as_deref().and_then(|p| crate::serve::proxy::parse(Some(p))).filter(|p| !p.is_empty()),
        analysis: ud.import_choice(ANALYSIS)?.map(|v| Mode::from_env(Some(&v))),
        beat_this: ud.import_choice(BEAT_THIS)?.map(|v| v == "1"),
        snapcast_host: ud.import_choice(SNAPCAST_HOST)?.map(|h| Some(h.trim().to_string()).filter(|h| !h.is_empty())),
        home_networks: ud.import_choice(HOME_NETWORKS)?.as_deref().and_then(|p| crate::serve::proxy::parse(Some(p))).unwrap_or_default(),
        guest: ud.import_choice(GUEST)?.and_then(|g| serde_json::from_str(&g).ok()).unwrap_or_default(),
    })
}

/// At the start: the environment's seed, then what Settings saved over it; the proxies' screen set. Blocking.
pub fn start(seed: Seed, ud: Option<&crate::userdata::UserData>, bind: &str) -> Runtime {
    let _ = SEED.set(seed.clone());
    let _ = PORT.set(bind.rsplit(':').next().unwrap_or("8080").to_string());
    let saved = ud.and_then(|u| read_saved(u).map_err(|e| tracing::warn!("settings: the saved server settings could not be read: {e:#}")).ok()).unwrap_or_default();
    let r = combine(seed, &saved);
    crate::serve::proxy::set(r.proxies.clone());
    crate::serve::proxy::set_home(r.home_networks.clone());
    *NOW.write().unwrap_or_else(|p| p.into_inner()) = Some(r.clone());
    r
}

/// Cratarr on the local network, as a Kodi streams from it: the sync-play host and Cratarr's port.
pub fn lan_base(r: &Runtime) -> Option<String> {
    r.snapcast_host.as_ref().map(|h| format!("http://{h}:{}", PORT.get().map_or("8080", String::as_str)))
}

/// The settings as Settings shows them.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct View {
    pub proxies: Vec<String>,
    pub analysis: &'static str,
    pub beat_this: bool,
    pub snapcast_host: Option<String>,
    pub home_networks: Vec<String>,
    pub guest: Guest,
    /// What the environment set and Settings has not saved over.
    pub from_env: serde_json::Map<String, serde_json::Value>,
    /// Those that wait for a restart (none: each applies at once).
    pub restart: Vec<&'static str>,
}

pub fn view(ud: &crate::userdata::UserData) -> Result<View> {
    let saved = read_saved(ud)?;
    let seed = SEED.get().cloned().unwrap_or_default();
    let r = now();
    let mut from_env = serde_json::Map::new();
    let mut say = |k: &str, by_env: bool| {
        if by_env {
            from_env.insert(k.to_string(), true.into());
        }
    };
    say("proxies", seed.proxies.is_some() && !saved.proxies_set);
    say("analysis", seed.analysis.is_some() && saved.analysis.is_none());
    say("beat_this", seed.beat_this.is_some() && saved.beat_this.is_none());
    say("snapcast_host", seed.snapcast_host.is_some() && saved.snapcast_host.is_none());
    Ok(View {
        proxies: r.proxies.unwrap_or_default().iter().map(ToString::to_string).collect(),
        analysis: mode_name(r.analysis),
        beat_this: r.beat_this,
        snapcast_host: r.snapcast_host,
        home_networks: r.home_networks.iter().map(ToString::to_string).collect(),
        guest: r.guest,
        from_env,
        restart: Vec::new(),
    })
}

/// A change from Settings: each field given is saved; absent ones stay.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Change {
    #[serde(default)]
    pub proxies: Option<Vec<String>>,
    #[serde(default)]
    pub analysis: Option<String>,
    #[serde(default)]
    pub beat_this: Option<bool>,
    /// Some(None) or Some("") clears it.
    #[serde(default, deserialize_with = "some_or_null")]
    pub snapcast_host: Option<Option<String>>,
    #[serde(default)]
    pub home_networks: Option<Vec<String>>,
    #[serde(default)]
    pub guest: Option<Guest>,
}

fn some_or_null<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Option<Option<String>>, D::Error> {
    Ok(Some(Option::<String>::deserialize(d)?))
}

/// Each entry an address or a range; the first that is not, named.
pub fn proxies_of(list: &[String]) -> std::result::Result<Vec<IpNet>, String> {
    list.iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<IpNet>().or_else(|_| p.parse::<std::net::IpAddr>().map(IpNet::from)).map_err(|_| format!("{p} is not an address or a range.")))
        .collect()
}

/// Saves a change and puts it in force; the new settings. `Err(message)` for a value refused (nothing saved then).
/// Blocking.
pub fn save(ud: &crate::userdata::UserData, c: &Change) -> std::result::Result<Runtime, String> {
    let proxies = c.proxies.as_deref().map(proxies_of).transpose()?;
    let home = c.home_networks.as_deref().map(proxies_of).transpose()?;
    let analysis = match c.analysis.as_deref() {
        None => None,
        Some(a @ ("keep" | "all" | "off")) => Some(a),
        Some(other) => return Err(format!("{other} is not keep, all or off.")),
    };
    let host = c.snapcast_host.clone().map(|h| h.map(|h| h.trim().to_string()).filter(|h| !h.is_empty()));
    if let Some(Some(h)) = &host
        && (h.contains(['/', ' ', '@']) || h.contains("://"))
    {
        return Err(format!("{h} is not a host name or an address."));
    }
    let guest = c.guest.clone().map(guest_of).transpose()?;
    let fail = |e: anyhow::Error| format!("{e:#}");
    if let Some(p) = &proxies {
        ud.set_import_choice(PROXIES, &p.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")).map_err(fail)?;
    }
    if let Some(a) = analysis {
        ud.set_import_choice(ANALYSIS, a).map_err(fail)?;
    }
    if let Some(b) = c.beat_this {
        ud.set_import_choice(BEAT_THIS, if b { "1" } else { "0" }).map_err(fail)?;
    }
    if let Some(h) = &host {
        ud.set_import_choice(SNAPCAST_HOST, h.as_deref().unwrap_or("")).map_err(fail)?;
    }
    if let Some(n) = &home {
        ud.set_import_choice(HOME_NETWORKS, &n.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")).map_err(fail)?;
    }
    if let Some(g) = &guest {
        ud.set_import_choice(GUEST, &serde_json::to_string(g).map_err(|e| e.to_string())?).map_err(fail)?;
    }
    let r = combine(SEED.get().cloned().unwrap_or_default(), &read_saved(ud).map_err(fail)?);
    crate::serve::proxy::set(r.proxies.clone());
    crate::serve::proxy::set_home(r.home_networks.clone());
    *NOW.write().unwrap_or_else(|p| p.into_inner()) = Some(r.clone());
    CHANGED.notify_waiters();
    Ok(r)
}

/// The guest setting as saved: its host's name trimmed; on needs a host.
fn guest_of(g: Guest) -> std::result::Result<Guest, String> {
    let named = |n: String| Some(n.trim().to_string()).filter(|n| !n.is_empty() && n.chars().count() <= 64 && !n.chars().any(char::is_control));
    let host = match g.host {
        None => None,
        Some(GuestHost::User(u)) => Some(GuestHost::User(named(u).ok_or("Name the host.")?)),
        Some(GuestHost::Player(p)) => Some(GuestHost::Player(named(p).ok_or("Name the host's player.")?)),
    };
    if g.on && host.is_none() {
        return Err("The guest page needs a host: a user or a player.".into());
    }
    Ok(Guest { on: g.on, host })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guest_page_is_off_by_default_and_on_needs_a_host() {
        assert_eq!(combine(Seed::default(), &Saved::default()).guest, Guest::default());
        let g: Guest = serde_json::from_str(r#"{"on":true,"host":{"player":" Salon tablet "}}"#).unwrap();
        assert_eq!(guest_of(g).unwrap().host, Some(GuestHost::Player("Salon tablet".into())));
        assert!(guest_of(Guest { on: true, host: None }).is_err());
        assert_eq!(serde_json::to_value(Guest { on: false, host: Some(GuestHost::User("ann".into())) }).unwrap(), serde_json::json!({ "on": false, "host": { "user": "ann" } }));
    }

    #[test]
    fn a_bad_proxy_is_named_and_good_ones_are_ranges() {
        assert_eq!(proxies_of(&["172.19.0.0/16".into(), " 192.168.1.5 ".into(), "".into()]).unwrap().len(), 2);
        assert_eq!(proxies_of(&["10.0.0.1".into(), "proxy.lan".into()]).unwrap_err(), "proxy.lan is not an address or a range.");
    }

    #[test]
    fn saved_wins_over_the_environment() {
        let seed = Seed { proxies: crate::serve::proxy::parse(Some("10.0.0.1")), analysis: Some(Mode::All), beat_this: Some(true), snapcast_host: Some("nas".into()) };
        let r = combine(seed.clone(), &Saved::default());
        assert_eq!((r.analysis, r.beat_this, r.snapcast_host.as_deref()), (Mode::All, true, Some("nas")));
        assert_eq!(r.proxies.unwrap().len(), 1);
        let saved = Saved { proxies_set: true, proxies: None, analysis: Some(Mode::Off), beat_this: Some(false), snapcast_host: Some(None), home_networks: Vec::new(), guest: Guest::default() };
        let r = combine(seed, &saved);
        assert_eq!((r.proxies, r.analysis, r.beat_this, r.snapcast_host), (None, Mode::Off, false, None), "saved [] is local peers only, over the environment's");
    }
}
