//! Settings saved from the Settings page. The file holds the same keys as the environment variables and wins over
//! them, so `Config::from_lookup` validates and defaults both alike. Only `EDITABLE` keys are ever read from or
//! written to it; the API key is never sent back to the page. See decision 0004.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::Config;

pub const URL: &str = "JELLYFIN_URL";
pub const KEY: &str = "JELLYFIN_API_KEY";
pub const USER: &str = "JELLYFIN_USER_ID";
pub const CLIENT: &str = "CRATARR_PLAYER_CLIENT";
pub const PREFIXES: &str = "CRATARR_MUSIC_PREFIXES";
pub const JF_PLAYLIST_DIR: &str = "CRATARR_JELLYFIN_PLAYLIST_DIR";

/// What the page may change. The bind address and playlist folder are tied to the container (port, mount), so
/// they stay in the environment.
pub const EDITABLE: [&str; 6] = [URL, KEY, USER, CLIENT, PREFIXES, JF_PLAYLIST_DIR];

/// The saved file: env-var name to value, editable keys only.
pub type Overrides = BTreeMap<String, String>;

/// Read the file; a missing file means nothing is overridden. Unknown keys are dropped (and logged), so a
/// hand-edited file can never smuggle in, say, a different playlist folder.
pub fn load(path: &Path) -> Result<Overrides> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Overrides::new()),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
    };
    let all: Overrides = serde_json::from_str(&text).with_context(|| format!("{} is not a JSON object of strings", path.display()))?;
    Ok(all
        .into_iter()
        .filter(|(k, _)| {
            let ok = EDITABLE.contains(&k.as_str());
            if !ok {
                tracing::warn!("ignoring {k} in {}: it cannot be set from the settings file", path.display());
            }
            ok
        })
        .collect())
}

/// Write the file atomically (temporary file, then rename), readable by the owner only: it can hold the API key.
pub fn save(path: &Path, overrides: &Overrides) -> Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("json.tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp).with_context(|| format!("cannot write {}", tmp.display()))?;
    f.write_all(serde_json::to_string_pretty(overrides)?.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))
}

/// The file first, then the environment.
pub fn lookup<'a>(file: &'a Overrides, env: &'a dyn Fn(&str) -> Option<String>) -> impl Fn(&str) -> Option<String> + 'a {
    move |k| file.get(k).cloned().or_else(|| env(k))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    File,
    Env,
    Default,
}

pub fn source(key: &str, file: &Overrides, env: &dyn Fn(&str) -> Option<String>) -> Source {
    if file.contains_key(key) {
        Source::File
    } else if env(key).is_some_and(|v| !v.trim().is_empty()) {
        Source::Env
    } else {
        Source::Default
    }
}

/// A change sent by the page. An empty or missing `api_key` keeps the current key.
#[derive(Debug, Clone, Deserialize)]
pub struct Change {
    pub jellyfin_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    pub user_id: String,
    pub player_client: String,
    pub music_prefixes: Vec<String>,
    pub jellyfin_playlist_dir: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChangeError {
    /// Otherwise anyone who can open the page could point Cratarr at their own server and receive the stored key.
    #[error("a new Jellyfin address needs the API key to be entered again")]
    KeyNeeded,
    #[error("{0}")]
    Invalid(String),
}

/// The new overrides and the config they produce. A value equal to what the environment (or the default) gives is
/// left out of the file, so the file holds only real overrides and typing the old value back undoes one.
pub fn apply(
    current: &Config,
    saved: &Overrides,
    change: &Change,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<(Overrides, Config), ChangeError> {
    let url = change.jellyfin_url.trim().trim_end_matches('/').to_string();
    let key = change.api_key.as_deref().map(str::trim).filter(|k| !k.is_empty());
    if key.is_none() && url != current.jellyfin_url {
        return Err(ChangeError::KeyNeeded);
    }
    let prefixes: Vec<&str> = change.music_prefixes.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
    let wanted = [
        (URL, Some(url.clone())),
        (KEY, key.map(str::to_string)),
        (USER, Some(change.user_id.trim().to_string())),
        (CLIENT, Some(change.player_client.trim().to_string())),
        (PREFIXES, Some(prefixes.join(","))),
        (JF_PLAYLIST_DIR, Some(change.jellyfin_playlist_dir.trim().to_string())),
    ];
    // Without the file. It can fail (the file may hold a required value); then nothing counts as "the same".
    let base = Config::from_lookup(env).ok();
    let base_value = |k: &str| {
        let b = base.as_ref()?;
        Some(match k {
            URL => b.jellyfin_url.clone(),
            KEY => b.api_key.clone(),
            USER => b.user_id.clone(),
            CLIENT => b.player_client.clone(),
            PREFIXES => b.music_prefixes.join(","),
            _ => b.jellyfin_playlist_dir.clone(),
        })
    };
    let mut next = saved.clone();
    for (k, v) in wanted {
        let Some(v) = v else { continue }; // keep whatever the key is now
        if base_value(k).as_deref() == Some(v.as_str()) {
            next.remove(k);
        } else {
            next.insert(k.to_string(), v);
        }
    }
    if next.get(USER).is_some_and(|v| v.is_empty()) {
        return Err(ChangeError::Invalid("choose a Jellyfin user".into()));
    }
    if next.get(CLIENT).is_some_and(|v| v.is_empty()) {
        next.remove(CLIENT); // empty means "the default"
    }
    let cfg = Config::from_lookup(lookup(&next, env)).map_err(|e| ChangeError::Invalid(format!("{e:#}")))?;
    Ok((next, cfg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| m.get(k).cloned()
    }

    const ENV: [(&str, &str); 4] =
        [(URL, "http://jf:8096"), (KEY, "envkey"), (USER, "u1"), ("CRATARR_PLAYLIST_DIR", "/music")];

    fn change(url: &str, key: Option<&str>) -> Change {
        Change {
            jellyfin_url: url.into(),
            api_key: key.map(str::to_string),
            user_id: "u1".into(),
            player_client: "Finamp".into(),
            music_prefixes: vec!["/data/media/Music".into(), "/Volume1/media/Music".into()],
            jellyfin_playlist_dir: "/Volume1/media/Music".into(),
        }
    }

    fn current(env: &dyn Fn(&str) -> Option<String>) -> Config {
        Config::from_lookup(env).unwrap()
    }

    #[test]
    fn the_file_wins_over_the_environment() {
        let env = env_of(&ENV);
        let file: Overrides = [(USER.to_string(), "u2".to_string())].into();
        let cfg = Config::from_lookup(lookup(&file, &env)).unwrap();
        assert_eq!(cfg.user_id, "u2");
        assert_eq!(cfg.api_key, "envkey");
        assert_eq!(source(USER, &file, &env), Source::File);
        assert_eq!(source(KEY, &file, &env), Source::Env);
        assert_eq!(source(CLIENT, &file, &env), Source::Default);
    }

    #[test]
    fn a_new_address_without_a_new_key_is_refused() {
        let env = env_of(&ENV);
        let r = apply(&current(&env), &Overrides::new(), &change("http://evil:8096", None), &env);
        assert_eq!(r.unwrap_err(), ChangeError::KeyNeeded);
        let r = apply(&current(&env), &Overrides::new(), &change("http://evil:8096", Some("  ")), &env);
        assert_eq!(r.unwrap_err(), ChangeError::KeyNeeded);
        let (file, cfg) = apply(&current(&env), &Overrides::new(), &change("http://other:8096/", Some("k2")), &env).unwrap();
        assert_eq!(cfg.jellyfin_url, "http://other:8096");
        assert_eq!(file.get(KEY).map(String::as_str), Some("k2"));
    }

    #[test]
    fn values_equal_to_the_environment_are_not_stored() {
        let env = env_of(&ENV);
        let saved: Overrides = [(USER.to_string(), "u2".to_string())].into();
        let (file, cfg) = apply(&current(&env), &saved, &change("http://jf:8096/", None), &env).unwrap();
        assert!(!file.contains_key(URL) && !file.contains_key(KEY) && !file.contains_key(USER), "{file:?}");
        assert_eq!(cfg.user_id, "u1");
        assert_eq!(cfg.api_key, "envkey");
    }

    #[test]
    fn defaults_are_not_pinned_into_the_file() {
        let env = env_of(&ENV);
        let (file, _) = apply(&current(&env), &Overrides::new(), &change("http://jf:8096", None), &env).unwrap();
        assert!(file.is_empty(), "{file:?}");
        let mut c = change("http://jf:8096", None);
        c.player_client = "Web".into();
        let (file, cfg) = apply(&current(&env), &Overrides::new(), &c, &env).unwrap();
        assert_eq!(file.keys().collect::<Vec<_>>(), vec![CLIENT]);
        assert_eq!(cfg.player_client, "Web");
    }

    #[test]
    fn an_empty_key_keeps_the_saved_one() {
        let env = env_of(&ENV);
        let saved: Overrides = [(KEY.to_string(), "filekey".to_string())].into();
        let cur = Config::from_lookup(lookup(&saved, &env)).unwrap();
        let (file, cfg) = apply(&cur, &saved, &change("http://jf:8096", Some("")), &env).unwrap();
        assert_eq!(file.get(KEY).map(String::as_str), Some("filekey"));
        assert_eq!(cfg.api_key, "filekey");
    }

    #[test]
    fn bad_values_are_refused_with_the_reason() {
        let env = env_of(&ENV);
        let mut c = change("jf:8096", Some("k"));
        assert!(matches!(apply(&current(&env), &Overrides::new(), &c, &env), Err(ChangeError::Invalid(m)) if m.contains("http")));
        c = change("http://jf:8096", None);
        c.music_prefixes = vec![" ".into()];
        assert!(matches!(apply(&current(&env), &Overrides::new(), &c, &env), Err(ChangeError::Invalid(_))));
        c = change("http://jf:8096", None);
        c.user_id = " ".into();
        assert!(matches!(apply(&current(&env), &Overrides::new(), &c, &env), Err(ChangeError::Invalid(_))));
    }

    #[test]
    fn prefixes_are_trimmed_and_joined() {
        let env = env_of(&ENV);
        let mut c = change("http://jf:8096", None);
        c.music_prefixes = vec![" /a/Music ".into(), "".into(), "/b/Music".into()];
        let (file, cfg) = apply(&current(&env), &Overrides::new(), &c, &env).unwrap();
        assert_eq!(file.get(PREFIXES).map(String::as_str), Some("/a/Music,/b/Music"));
        assert_eq!(cfg.music_prefixes, vec!["/a/Music", "/b/Music"]);
    }

    #[test]
    fn the_file_round_trips_owner_only_and_drops_unknown_keys() {
        let dir = std::env::temp_dir().join(format!("cratarr-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        assert!(load(&path).unwrap().is_empty());
        let o: Overrides = [(USER.to_string(), "u2".to_string())].into();
        save(&path, &o).unwrap();
        assert_eq!(load(&path).unwrap(), o);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::write(&path, r#"{"CRATARR_PLAYLIST_DIR":"/etc","JELLYFIN_USER_ID":"u3"}"#).unwrap();
        let back = load(&path).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back.get(USER).map(String::as_str), Some("u3"));
        assert!(!dir.join("settings.json.tmp").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
