//! Sources saved from the Settings page, in `CRATARR_DATA_DIR/settings.json`. Once the file holds sources it is the
//! whole truth; until then one Jellyfin source is made from the `JELLYFIN_*` environment variables (first run).
//! Secrets are never sent to the page, and a saved secret is only reused while the fields it is bound to stay the
//! same. See decisions 0004, 0005 and 0009.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::source::{self, FieldKind, Values};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceConfig {
    /// Stable within this install; short, lowercase.
    pub id: String,
    /// Which plugin (`source::KINDS`).
    pub kind: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// What its libraries are used for (decision 0009).
    #[serde(default)]
    pub libraries: Roles,
    #[serde(default)]
    pub values: Values,
}

fn yes() -> bool {
    true
}

/// What a server's libraries are used for, by the server's library ids. Libraries in neither list are not used.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roles {
    /// The music that can go in playlists. Their folder holds the playlists.
    #[serde(default)]
    pub music: Vec<String>,
    /// New music waiting to be heard. Version 2 files hold one id or none here.
    #[serde(default, deserialize_with = "one_or_many")]
    pub incoming: Vec<String>,
}

fn one_or_many<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match Option::<OneOrMany>::deserialize(d)? {
        None => Vec::new(),
        Some(OneOrMany::One(id)) => vec![id],
        Some(OneOrMany::Many(ids)) => ids,
    })
}

/// Version 2 files may still hold a `folders` list (before decision 0009); it is ignored.
#[derive(Serialize, Deserialize)]
struct FileV3 {
    version: u32,
    sources: Vec<SourceConfig>,
    #[serde(default)]
    music_folder: Option<String>,
    #[serde(default)]
    tag_rules: crate::catalogue::Rules,
}

/// Everything the Settings page saves.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Saved {
    pub sources: Vec<SourceConfig>,
    /// Where the Music libraries were last found, as Cratarr sees it: used until the servers answer.
    pub music_folder: Option<String>,
    /// How tag values are split when read (decision 0008).
    pub tag_rules: crate::catalogue::Rules,
}

/// Cratarr's folders, as found behind the libraries in use (decision 0009).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folders {
    /// Holds the playlists and the music that can go in them.
    pub music: Option<String>,
    /// Where the Incoming libraries' files are, when Cratarr can see them.
    pub incoming: Vec<String>,
}

impl Folders {
    /// Music first, then incoming: the catalogue's folders.
    pub fn all(&self) -> Vec<String> {
        self.music.iter().chain(&self.incoming).cloned().collect()
    }
}

/// Where the sources came from, shown on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    File,
    Env,
}

/// What was saved; `None` when nothing was saved yet. The first format (a flat map of environment-variable names,
/// decision 0004) is read as the Jellyfin source it described.
pub fn load(path: &Path, env: &dyn Fn(&str) -> Option<String>) -> Result<Option<Saved>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
    };
    let json: serde_json::Value = serde_json::from_str(&text).with_context(|| format!("{} is not JSON", path.display()))?;
    if json.get("version").is_some() {
        let f: FileV3 = serde_json::from_value(json).with_context(|| format!("{} has an unexpected shape", path.display()))?;
        return Ok(Some(Saved { sources: f.sources, music_folder: f.music_folder, tag_rules: f.tag_rules }));
    }
    let flat: std::collections::BTreeMap<String, String> =
        serde_json::from_value(json).with_context(|| format!("{} is neither a source list nor the older flat format", path.display()))?;
    Ok(Some(Saved { sources: from_env(&|k: &str| flat.get(k).cloned().or_else(|| env(k))), ..Saved::default() }))
}

/// Write atomically (temporary file, then rename), readable by the owner only: it holds secrets.
pub fn save(path: &Path, saved: &Saved) -> Result<()> {
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
    let body = FileV3 {
        version: 3,
        sources: saved.sources.clone(),
        music_folder: saved.music_folder.clone(),
        tag_rules: saved.tag_rules.clone(),
    };
    f.write_all(serde_json::to_string_pretty(&body)?.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))
}

/// First run: one Jellyfin source from `JELLYFIN_URL`, `JELLYFIN_API_KEY` and the optional `CRATARR_PLAYER_CLIENT`;
/// nothing when `JELLYFIN_URL` is unset.
pub fn from_env(env: &dyn Fn(&str) -> Option<String>) -> Vec<SourceConfig> {
    let get = |k: &str| env(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let Some(url) = get("JELLYFIN_URL") else { return Vec::new() };
    let mut values = Values::new();
    values.insert("url".into(), url.into());
    for (key, var) in [("api_key", "JELLYFIN_API_KEY"), ("player", "CRATARR_PLAYER_CLIENT")] {
        if let Some(v) = get(var) {
            values.insert(key.into(), v.into());
        }
    }
    vec![SourceConfig { id: "jellyfin".into(), kind: "jellyfin".into(), enabled: true, libraries: Roles::default(), values }]
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChangeError {
    /// Otherwise anyone who can open the page could point a source somewhere new and receive the stored secret.
    #[error("{name}: a new {field} needs the {secret} entered again")]
    SecretNeeded { name: String, field: String, secret: String },
    #[error("{0}")]
    Invalid(String),
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Check a list from the page, keep only fields the plugin declares (with the right shape), and fill blank secrets
/// from the saved source with the same id, unless a field the secret is bound to has changed.
pub fn merge(saved: &[SourceConfig], draft: Vec<SourceConfig>) -> Result<Vec<SourceConfig>, ChangeError> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(draft.len());
    for mut d in draft {
        if !valid_id(&d.id) {
            return Err(ChangeError::Invalid(format!("{:?} is not a usable source id (a–z, 0–9 and -, at most 32)", d.id)));
        }
        if !seen.insert(d.id.clone()) {
            return Err(ChangeError::Invalid(format!("two sources are called {:?}", d.id)));
        }
        let Some(kind) = source::kind(&d.kind) else {
            return Err(ChangeError::Invalid(format!("{}: unknown kind of source {:?}", d.id, d.kind)));
        };
        let fields = (kind.fields)();
        d.values.retain(|k, v| {
            fields.iter().any(|f| {
                f.key == k
                    && match f.kind {
                        FieldKind::MultiChoice => v.is_array() || v.is_string(),
                        _ => v.is_string(),
                    }
            })
        });
        let clean = |ids: &mut Vec<String>| {
            ids.retain(|l| !l.trim().is_empty());
            let mut seen = std::collections::HashSet::new();
            ids.retain(|l| seen.insert(l.clone()));
        };
        clean(&mut d.libraries.music);
        clean(&mut d.libraries.incoming);
        if d.libraries.music.iter().any(|l| d.libraries.incoming.contains(l)) {
            return Err(ChangeError::Invalid(format!("{}: one library cannot be both Music and Incoming", d.id)));
        }
        let prev = saved.iter().find(|s| s.id == d.id && s.kind == d.kind);
        for secret in fields.iter().filter(|f| f.kind == FieldKind::Secret) {
            if source::text(&d.values, secret.key).is_some() {
                continue;
            }
            d.values.remove(secret.key);
            let Some(prev) = prev else { continue };
            let Some(old) = source::text(&prev.values, secret.key) else { continue };
            if let Some(changed) = secret.bound_to.iter().find(|b| source::text(&prev.values, b) != source::text(&d.values, b)) {
                let field = fields.iter().find(|f| f.key == *changed).map_or(*changed, |f| f.label);
                return Err(ChangeError::SecretNeeded {
                    name: d.id.clone(),
                    field: field.to_lowercase(),
                    secret: secret.label.to_lowercase(),
                });
            }
            d.values.insert(secret.key.into(), old.into());
        }
        out.push(d);
    }
    Ok(out)
}

/// The values the page may see: secrets left out, plus which secrets are set.
pub fn redact(cfg: &SourceConfig) -> (Values, Vec<String>) {
    let secrets: Vec<&str> = source::kind(&cfg.kind)
        .map(|k| (k.fields)().into_iter().filter(|f| f.kind == FieldKind::Secret).map(|f| f.key).collect())
        .unwrap_or_default();
    let mut values = cfg.values.clone();
    let mut set = Vec::new();
    for s in secrets {
        if source::text(&values, s).is_some() {
            set.push(s.to_string());
        }
        values.remove(s);
    }
    (values, set)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| m.get(k).cloned()
    }

    fn jf(id: &str, url: &str, key: Option<&str>) -> SourceConfig {
        let mut values = Values::new();
        values.insert("url".into(), url.into());
        values.insert("player".into(), "Finamp".into());
        if let Some(k) = key {
            values.insert("api_key".into(), k.into());
        }
        SourceConfig { id: id.into(), kind: "jellyfin".into(), enabled: true, libraries: Roles::default(), values }
    }

    #[test]
    fn the_environment_gives_one_jellyfin_source() {
        let env = env_of(&[("JELLYFIN_URL", "http://jf:8096"), ("JELLYFIN_API_KEY", "k"), ("JELLYFIN_USER_ID", "u"), ("CRATARR_PLAYER_CLIENT", "Finamp")]);
        let s = from_env(&env);
        assert_eq!(s.len(), 1);
        assert_eq!(source::text(&s[0].values, "api_key"), Some("k"));
        assert_eq!(source::text(&s[0].values, "player"), Some("Finamp"));
        assert_eq!(source::text(&s[0].values, "user"), None, "JELLYFIN_USER_ID is no longer read");
        assert!(from_env(&env_of(&[])).is_empty());
    }

    #[test]
    fn a_blank_secret_keeps_the_saved_one_while_the_address_stays() {
        let saved = vec![jf("jellyfin", "http://jf:8096", Some("secret"))];
        let out = merge(&saved, vec![jf("jellyfin", "http://jf:8096", None)]).unwrap();
        assert_eq!(source::text(&out[0].values, "api_key"), Some("secret"));
        let out = merge(&saved, vec![jf("jellyfin", "http://jf:8096", Some("  "))]).unwrap();
        assert_eq!(source::text(&out[0].values, "api_key"), Some("secret"));
    }

    #[test]
    fn a_new_address_needs_the_secret_again() {
        let saved = vec![jf("jellyfin", "http://jf:8096", Some("secret"))];
        let e = merge(&saved, vec![jf("jellyfin", "http://evil:8096", None)]).unwrap_err();
        assert!(matches!(e, ChangeError::SecretNeeded { .. }), "{e}");
        assert!(e.to_string().contains("address"), "{e}");
        let out = merge(&saved, vec![jf("jellyfin", "http://new:8096", Some("k2"))]).unwrap();
        assert_eq!(source::text(&out[0].values, "api_key"), Some("k2"));
    }

    #[test]
    fn a_secret_never_moves_to_another_source() {
        let saved = vec![jf("jellyfin", "http://jf:8096", Some("secret"))];
        let out = merge(&saved, vec![jf("other", "http://jf:8096", None)]).unwrap();
        assert_eq!(source::text(&out[0].values, "api_key"), None);
    }

    #[test]
    fn ids_and_kinds_are_checked_and_unknown_fields_dropped() {
        assert!(merge(&[], vec![jf("Bad Id", "http://x", Some("k"))]).is_err());
        assert!(merge(&[], vec![jf("a", "http://x", Some("k")), jf("a", "http://y", Some("k"))]).is_err());
        let mut odd = jf("a", "http://x", Some("k"));
        odd.kind = "winamp".into();
        assert!(merge(&[], vec![odd]).is_err());
        let mut extra = jf("a", "http://x", Some("k"));
        extra.values.insert("smuggled".into(), "x".into());
        extra.values.insert("player".into(), serde_json::json!(["not", "a", "string"]));
        let out = merge(&[], vec![extra]).unwrap();
        assert!(!out[0].values.contains_key("smuggled"));
        assert!(!out[0].values.contains_key("player"), "wrong shape is dropped");
    }

    #[test]
    fn a_library_has_one_use() {
        let mut s = jf("a", "http://x", Some("k"));
        s.libraries = Roles { music: vec!["l1".into(), "l1".into(), " ".into()], incoming: vec!["l2".into()] };
        let out = merge(&[], vec![s.clone()]).unwrap();
        assert_eq!(out[0].libraries, Roles { music: vec!["l1".into()], incoming: vec!["l2".into()] });
        s.libraries.incoming.push("l1".into());
        assert!(merge(&[], vec![s]).is_err());
    }

    #[test]
    fn the_page_never_sees_secrets() {
        let (values, set) = redact(&jf("jellyfin", "http://jf:8096", Some("secret")));
        assert!(!values.contains_key("api_key"));
        assert_eq!(set, vec!["api_key"]);
        let (_, set) = redact(&jf("jellyfin", "http://jf:8096", None));
        assert!(set.is_empty());
    }

    fn dir() -> std::path::PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("cratarr-settings-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_file_round_trips_owner_only() {
        let d = dir();
        let path = d.join("settings.json");
        let none = env_of(&[]);
        assert_eq!(load(&path, &none).unwrap(), None);
        let s = vec![jf("jellyfin", "http://jf:8096", Some("k"))];
        let mut s = s;
        s[0].libraries = Roles { music: vec!["m".into()], incoming: vec!["i1".into(), "i2".into()] };
        let saved = Saved { sources: s, music_folder: Some("/music".into()), tag_rules: Default::default() };
        save(&path, &saved).unwrap();
        assert_eq!(load(&path, &none).unwrap(), Some(saved));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert!(!d.join("settings.json.tmp").exists());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_version_2_file_keeps_its_library_roles() {
        let d = dir();
        let path = d.join("settings.json");
        std::fs::write(
            &path,
            r#"{"version":2,"sources":[{"id":"jellyfin","kind":"jellyfin","libraries":{"music":["a"],"incoming":"b"},"values":{"url":"http://jf"}},{"id":"other","kind":"jellyfin","libraries":{"music":[],"incoming":null}}],"folders":["/x"]}"#,
        )
        .unwrap();
        let s = load(&path, &env_of(&[])).unwrap().unwrap();
        assert_eq!(s.sources[0].libraries, Roles { music: vec!["a".into()], incoming: vec!["b".into()] });
        assert_eq!(s.sources[1].libraries, Roles::default());
        assert_eq!(s.music_folder, None);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_older_flat_file_still_loads() {
        let d = dir();
        let path = d.join("settings.json");
        std::fs::write(&path, r#"{"CRATARR_PLAYER_CLIENT":"Kodi","CRATARR_MUSIC_PREFIXES":"/x"}"#).unwrap();
        let env = env_of(&[("JELLYFIN_URL", "http://jf:8096"), ("JELLYFIN_API_KEY", "k")]);
        let s = load(&path, &env).unwrap().unwrap().sources;
        assert_eq!(s.len(), 1);
        assert_eq!(source::text(&s[0].values, "player"), Some("Kodi"));
        assert_eq!(source::text(&s[0].values, "api_key"), Some("k"));
        std::fs::remove_dir_all(&d).ok();
    }
}
