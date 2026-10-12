//! What belongs to the container: the port and the playlist folder mount. Read from environment variables.
//! Sources (Jellyfin and any others) are configured separately, see `settings.rs`.

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    /// Address to listen on, e.g. `0.0.0.0:8080`.
    pub bind: String,
    /// Folder holding the `.m3u` playlists, as this process sees it. The music sits under it too.
    pub playlist_dir: String,
    /// Where `settings.json` is kept; `None`: the Settings page can look but not save.
    pub data_dir: Option<String>,
}

/// Variables from the first version that are no longer read (`placement.rs` finds paths on its own).
pub const RETIRED: [&str; 2] = ["CRATARR_MUSIC_PREFIXES", "CRATARR_JELLYFIN_PLAYLIST_DIR"];

impl Config {
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let get = |k: &str| lookup(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        Ok(Self {
            bind: get("CRATARR_BIND").unwrap_or_else(|| "0.0.0.0:8080".into()),
            playlist_dir: get("CRATARR_PLAYLIST_DIR").context("environment variable CRATARR_PLAYLIST_DIR is required")?,
            data_dir: get("CRATARR_DATA_DIR"),
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
        let c = cfg(&[("CRATARR_PLAYLIST_DIR", "/music")]).unwrap();
        assert_eq!(c.bind, "0.0.0.0:8080");
        assert_eq!(c.playlist_dir, "/music");
        assert_eq!(c.data_dir, None);
    }

    #[test]
    fn the_playlist_folder_is_required_and_named_when_missing() {
        let e = cfg(&[]).unwrap_err();
        assert!(e.to_string().contains("CRATARR_PLAYLIST_DIR"), "{e}");
        assert!(cfg(&[("CRATARR_PLAYLIST_DIR", "  ")]).is_err());
    }

    #[test]
    fn a_blank_data_dir_means_none() {
        let c = cfg(&[("CRATARR_PLAYLIST_DIR", "/music"), ("CRATARR_DATA_DIR", " ")]).unwrap();
        assert_eq!(c.data_dir, None);
    }
}
