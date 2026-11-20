//! The server's settings in force, below Incoming (moved from `app::runtime`, which keeps their names and reads and saves them):
//! the analysis pass reads them each round and wakes when they change.

use std::sync::{OnceLock, RwLock};

use ipnet::IpNet;
use serde::{Deserialize, Serialize};

use super::analysis_pass_base::Mode;

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
            beat_this: set("CRATARR_BEAT_THIS").map(|v| super::analysis_pass_base::hears_beats(Some(&v))),
            snapcast_host: set("CRATARR_SNAPCAST_HOST"),
        }
    }
}

/// What Settings saved (none: not saved, the environment's stands).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Saved {
    pub proxies: Option<Vec<IpNet>>,
    pub proxies_set: bool,
    pub analysis: Option<Mode>,
    pub beat_this: Option<bool>,
    pub snapcast_host: Option<Option<String>>,
    pub home_networks: Vec<IpNet>,
    pub guest: Guest,
}

pub static SEED: OnceLock<Seed> = OnceLock::new();
pub static NOW: RwLock<Option<Runtime>> = RwLock::new(None);
/// Wakes the analysis pass when its settings change.
pub static CHANGED: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// The settings in force (the environment's until `start`).
pub fn now() -> Runtime {
    if let Some(r) = NOW.read().unwrap_or_else(|p| p.into_inner()).clone() {
        return r;
    }
    combine(SEED.get().cloned().unwrap_or_default(), &Saved::default())
}

pub fn combine(seed: Seed, saved: &Saved) -> Runtime {
    Runtime {
        proxies: if saved.proxies_set { saved.proxies.clone() } else { seed.proxies },
        analysis: saved.analysis.or(seed.analysis).unwrap_or(Mode::Keep),
        beat_this: saved.beat_this.or(seed.beat_this).unwrap_or(false),
        snapcast_host: saved.snapcast_host.clone().unwrap_or(seed.snapcast_host),
        home_networks: saved.home_networks.clone(),
        guest: saved.guest.clone(),
    }
}
