//! What belongs to the container: the port and the data folder. Read from environment variables. Servers and what
//! each folder is for are configured on the Settings page, see `settings.rs`.

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Config {
    /// Address to listen on, e.g. `0.0.0.0:8080`.
    pub bind: String,
    /// The Music folder until one is chosen in Settings (decision 0009).
    pub playlist_dir: Option<String>,
    /// Where `settings.json` is kept; `None`: the Settings page can look but not save.
    pub data_dir: Option<String>,
    /// Where Snapcast's clients connect for sync play (decision 0037), e.g. `0.0.0.0:1704`; empty: not served.
    pub snapcast_bind: Option<String>,
    /// The proxies whose `X-Forwarded-*` headers are believed (`CRATARR_PROXIES`); `None`: local peers' (serve::proxy).
    /// `CRATARR_SNAPCAST_HOST`, the analysis and Beat this! are read as Settings' seeds (`app::runtime`).
    pub proxies: Option<Vec<ipnet::IpNet>>,
}

/// Variables that are no longer read: `placement.rs` finds paths on its own, and no Jellyfin user is involved.
pub const RETIRED: [&str; 3] = ["CRATARR_MUSIC_PREFIXES", "CRATARR_JELLYFIN_PLAYLIST_DIR", "JELLYFIN_USER_ID"];

impl Config {
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let get = |k: &str| lookup(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        Ok(Self {
            bind: get("CRATARR_BIND").unwrap_or_else(|| "0.0.0.0:8080".into()),
            playlist_dir: get("CRATARR_PLAYLIST_DIR"),
            data_dir: get("CRATARR_DATA_DIR"),
            snapcast_bind: match lookup("CRATARR_SNAPCAST_BIND") {
                Some(b) => Some(b.trim().to_string()).filter(|b| !b.is_empty()),
                None => Some("0.0.0.0:1704".into()),
            },
            proxies: crate::serve::proxy::parse(get("CRATARR_PROXIES").as_deref()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cfg(pairs: &[(&str, &str)]) -> Result<Config> {
        let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Config::from_lookup(|k| m.get(k).cloned())
    }

    #[test]
    fn minimal_config_gets_defaults() {
        let c = cfg(&[]).unwrap();
        assert_eq!(c.bind, "0.0.0.0:8080");
        assert_eq!(c.playlist_dir, None);
        assert_eq!(c.data_dir, None);
        assert_eq!(c.snapcast_bind.as_deref(), Some("0.0.0.0:1704"));
        assert_eq!(cfg(&[("CRATARR_SNAPCAST_BIND", "")]).unwrap().snapcast_bind, None, "set empty: not served");
        assert_eq!(cfg(&[("CRATARR_PLAYLIST_DIR", "/music")]).unwrap().playlist_dir.as_deref(), Some("/music"));
        assert_eq!(cfg(&[("CRATARR_PLAYLIST_DIR", "  ")]).unwrap().playlist_dir, None);
    }

    #[test]
    fn a_blank_data_dir_means_none() {
        let c = cfg(&[("CRATARR_DATA_DIR", " ")]).unwrap();
        assert_eq!(c.data_dir, None);
    }
}
