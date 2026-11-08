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
    /// Its libraries in use, in order (0101 point 1; their roles before, decision 0009).
    #[serde(default)]
    pub libraries: Libraries,
    #[serde(default)]
    pub values: Values,
}

fn yes() -> bool {
    true
}

/// Cratarr's own source's value for the own-music folder (0101 point 9): one administrator's setting, a subfolder per
/// account in it.
pub const OWN_MUSIC: &str = "own_music";

/// The viewer's own music, as it is named (the owner, 2026-11-04); anyone else's is "<account>'s music".
pub const YOUR_MUSIC: &str = "Your music";

impl SourceConfig {
    /// The own-music folder: Cratarr's own source's, where one is set.
    pub fn own_music(&self) -> Option<std::path::PathBuf> {
        (self.kind == crate::source::cratarr::KIND.id).then(|| source::text(&self.values, OWN_MUSIC).map(std::path::PathBuf::from)).flatten()
    }
}

/// A library of a server as Cratarr uses it (0101 point 1: no roles), with its own settings (decision 0034). The list's
/// order is the default: the first main library takes every fallback. A library not in the list is not used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibrarySetting {
    /// The server's id of it (a folder's path for Cratarr's own).
    pub id: String,
    pub kind: LibraryKind,
    /// An Incoming library's fetcher, whose records come into it (decision 0096: one library per fetcher; [`FETCHERS`]):
    /// "soulseek", Cratarr fetching from Soulseek itself (0.15; the owner, 2026-11-03: its own Incoming library, not
    /// Lidarr's), or "lidarr", where Lidarr's imports land.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetcher: Option<String>,
    /// An Incoming library's Keep by default (0101 point 2: Incoming's setting), a main library of the list; none: the
    /// first main library. A user's request goes into the requester's own music whatever this says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_to: Option<String>,
    /// Its name as Cratarr shows it (the owner, 2026-11-02: a name of one's own, the folder's until then); none: the
    /// server's (a folder of Cratarr's own: the folder's name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Its albums' dot, "#rrggbb"; none: the palette's, in turn (`colour_among`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<String>,
    /// Its quality limit on the way in (decision 0096's amendment); none: its kind's (`LibraryKind::quality`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<Quality>,
}

impl LibrarySetting {
    /// A library of `kind` with nothing of its own set.
    pub fn new(id: impl Into<String>, kind: LibraryKind) -> Self {
        Self { id: id.into(), kind, fetcher: None, keep_to: None, name: None, colour: None, quality: None }
    }
}

/// What a library is (0101 point 1). The settings' list holds main and Incoming libraries; each account's own music
/// comes from the own-music folder (0101 point 9), never from the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryKind {
    /// The household's music: every main library is equal, the first taking every fallback.
    Main,
    /// A fetcher's new music, waiting to be heard and kept.
    Incoming,
    /// An account's own music (0084).
    Own,
}

impl LibraryKind {
    /// The quality limit of a library of this kind without one of its own: a main library's and own music's CD
    /// format (decision 0013's Keep), Incoming's none.
    pub fn quality(self) -> Quality {
        match self {
            LibraryKind::Main | LibraryKind::Own => Quality::Cd,
            LibraryKind::Incoming => Quality::None,
        }
    }

    /// As the page says it.
    pub fn as_str(self) -> &'static str {
        match self {
            LibraryKind::Main => "main",
            LibraryKind::Incoming => "incoming",
            LibraryKind::Own => "own",
        }
    }
}

/// A server's libraries in use, in order (0101 point 1). A file from before 0.16 holds their roles instead (decision
/// 0009: Music's ids, Incoming's, and each library's settings by id), read as the list (`Roles::listed`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Libraries(pub Vec<LibrarySetting>);

impl<'de> Deserialize<'de> for Libraries {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Kept {
            List(Vec<LibrarySetting>),
            Roles(Roles),
        }
        Ok(match Kept::deserialize(d)? {
            Kept::List(list) => Libraries(list),
            Kept::Roles(roles) => roles.listed(),
        })
    }
}

/// A server's libraries as files before 0.16 kept them: their roles, and each one's settings by id.
#[derive(Debug, Default, Deserialize)]
struct Roles {
    #[serde(default)]
    music: Vec<String>,
    /// Version 2 files hold one id or none here.
    #[serde(default, deserialize_with = "one_or_many")]
    incoming: Vec<String>,
    #[serde(default)]
    policies: std::collections::BTreeMap<String, LibraryPolicy>,
}

/// A library's settings as files before 0.16 kept them ("format" before the quality limit, 0.14).
#[derive(Debug, Clone, Default, Deserialize)]
struct LibraryPolicy {
    #[serde(default, alias = "format")]
    quality: Option<Quality>,
    #[serde(default)]
    colour: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    fetcher: Option<String>,
}

impl Roles {
    /// As 0.16's list: Music's libraries first, in their order, then Incoming's, each with its settings (a fetcher only
    /// an Incoming library's). One in both is Music's; one in neither is not used, and its settings go.
    fn listed(&self) -> Libraries {
        let mut out: Vec<LibrarySetting> = Vec::new();
        for (ids, kind) in [(&self.music, LibraryKind::Main), (&self.incoming, LibraryKind::Incoming)] {
            for id in ids.iter().filter(|id| !id.trim().is_empty()) {
                if out.iter().any(|l| &l.id == id) {
                    continue;
                }
                let p = self.policies.get(id).cloned().unwrap_or_default();
                let fetcher = p.fetcher.filter(|_| kind == LibraryKind::Incoming);
                out.push(LibrarySetting { fetcher, name: p.name, colour: p.colour, quality: p.quality, ..LibrarySetting::new(id.clone(), kind) });
            }
        }
        Libraries(out)
    }

    /// The libraries with settings of their own but in no role: not used, so left out of the list.
    fn unused(&self) -> impl Iterator<Item = &str> {
        self.policies.keys().filter(|l| !self.music.contains(l) && !self.incoming.contains(l)).map(String::as_str)
    }
}

/// The fetchers a library may be fetched into by (decision 0099's amendment, the owner 2026-11-03: siblings).
pub const FETCHERS: [&str; 2] = ["lidarr", "soulseek"];

/// A library's quality limit (decision 0096's amendment, the owner, 2026-11-02: in place of the format): lossless files
/// above it become FLAC at it as they move in or are kept into it; lossy files and those within it move as they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    /// No limit: the files move as they are ("as_is", the format before the limit).
    #[serde(alias = "as_is")]
    None,
    /// Up to 24-bit/96 kHz.
    Hr96,
    /// Up to 24-bit/48 kHz.
    Hr48,
    /// CD format, 16-bit/44.1 kHz (decision 0013's Keep).
    Cd,
}

impl Quality {
    /// The bits and rate files are converted down to; none for no limit.
    pub fn limit(self) -> Option<crate::convert::Limit> {
        use crate::convert::Limit;
        match self {
            Quality::None => None,
            Quality::Hr96 => Some(Limit { bits: 24, rate: 96_000 }),
            Quality::Hr48 => Some(Limit { bits: 24, rate: 48_000 }),
            Quality::Cd => Some(crate::convert::CD),
        }
    }
}

/// The colours a library other than the primary gets until one is chosen, in turn.
pub const PALETTE: [&str; 6] = ["#e0a23a", "#5b8def", "#d4567f", "#3fae9a", "#9a6fdc", "#c8743c"];

impl Libraries {
    /// One of the list.
    pub fn get(&self, id: &str) -> Option<&LibrarySetting> {
        self.0.iter().find(|l| l.id == id)
    }

    /// The ids of those of `kind`, in the list's order.
    pub fn of(&self, kind: LibraryKind) -> impl Iterator<Item = &str> {
        self.0.iter().filter(move |l| l.kind == kind).map(|l| l.id.as_str())
    }

    /// The quality limit a move into `library` follows: its own, else its kind's; none for a library not listed.
    pub fn quality_of(&self, library: &str) -> Quality {
        self.get(library).map_or(Quality::None, |l| l.quality.unwrap_or(l.kind.quality()))
    }

    /// The dot of `library`'s albums (`colour_among`).
    pub fn colour_of(&self, library: &str) -> Option<String> {
        colour_among(self.0.iter().map(|l| (l.id.as_str(), l.kind)), library, self.get(library).and_then(|l| l.colour.as_deref()))
    }

    /// `library`'s own name, if one was given (`placement::place` names it so wherever it shows).
    pub fn name_of(&self, library: &str) -> Option<String> {
        self.get(library).and_then(|l| l.name.clone())
    }

    /// The Incoming library `fetcher` fetches into.
    pub fn fetched_by(&self, fetcher: &str) -> Option<&str> {
        self.0.iter().find(|l| l.kind == LibraryKind::Incoming && l.fetcher.as_deref() == Some(fetcher)).map(|l| l.id.as_str())
    }
}

/// The dot of `library`'s albums among the libraries in use (`used`: in order, with their kinds; 0101 point 1: every
/// library has one, the page showing it while the viewer has two or more on): its own colour (`own`), else the
/// palette's in turn, Incoming's first (as before 0.16), then the main libraries', then own music's. None for a library
/// not in use.
pub fn colour_among<'a>(used: impl Iterator<Item = (&'a str, LibraryKind)>, library: &str, own: Option<&str>) -> Option<String> {
    let rank = |k: LibraryKind| match k {
        LibraryKind::Incoming => 0,
        LibraryKind::Main => 1,
        LibraryKind::Own => 2,
    };
    let mut order: Vec<(&str, LibraryKind)> = used.collect();
    order.sort_by_key(|(_, k)| rank(*k));
    let i = order.iter().position(|(id, _)| *id == library)?;
    Some(own.map_or_else(|| PALETTE[i % PALETTE.len()].to_string(), str::to_string))
}

/// The most a library's name holds.
const NAME_MOST: usize = 80;

/// "#rrggbb", lower case.
pub fn valid_colour(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
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

/// The file as written since 0.16 (version 4). Version 2 files may still hold a `folders` list (before decision 0009),
/// which is ignored; before version 4 a source's libraries are their roles (`Libraries`).
#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    sources: Vec<SourceConfig>,
    #[serde(default)]
    music_folder: Option<String>,
    #[serde(default)]
    tag_rules: crate::catalogue::Rules,
    #[serde(default)]
    lidarr: Option<LidarrConfig>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    lidarr_removed: bool,
    #[serde(default = "default_trash_days")]
    trash_days: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    naming: Option<String>,
}

fn default_trash_days() -> u32 {
    crate::trash::DEFAULT_KEEP_DAYS
}

/// Lidarr, which Incoming asks to stop fetching an album it deleted. Optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LidarrConfig {
    pub url: String,
    /// Secret: never sent to the page; reused only while the address stays the same.
    #[serde(default)]
    pub api_key: String,
    /// Switched on (the owner, 2026-10-24: every connection has its switch); off, Cratarr does not use it.
    #[serde(default = "switched_on")]
    pub enabled: bool,
    /// What Discover's Want adds an album with, as Seerr asks of each server (the owner, 2026-10-24): Lidarr's
    /// quality and metadata profiles and its root folder (which decides where it lands: Incoming's). None: as the
    /// artists Lidarr has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_profile: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_profile: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_folder: Option<String>,
}

fn switched_on() -> bool {
    true
}

/// Everything the Settings page saves.
#[derive(Debug, Clone, PartialEq)]
pub struct Saved {
    pub sources: Vec<SourceConfig>,
    /// Where the Music libraries were last found, as Cratarr sees it: used until the servers answer.
    pub music_folder: Option<String>,
    /// How tag values are split when read (decision 0008).
    pub tag_rules: crate::catalogue::Rules,
    pub lidarr: Option<LidarrConfig>,
    /// The environment holds a Lidarr, and Settings said none: it stays out at the next start (`lidarr_at_start`).
    pub lidarr_removed: bool,
    /// How long albums deleted from Incoming stay in the trash.
    pub trash_days: u32,
    /// How Keep and Replace name albums in Music (decision 0013), as a template string. `None`: the one the library
    /// already follows.
    pub naming: Option<String>,
}

impl Default for Saved {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            music_folder: None,
            tag_rules: Default::default(),
            lidarr: None,
            lidarr_removed: false,
            trash_days: default_trash_days(),
            naming: None,
        }
    }
}

/// Cratarr's folders, as found behind the libraries in use (decision 0009).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folders {
    /// The first main library's folder (0101 point 1: the list's first takes every fallback): playlists' lines are made
    /// from it (the catalogue's folder 0).
    pub music: Option<String>,
    /// The other main libraries' folders, in the list's order (0101 point 1: main libraries are equal).
    pub more_main: Vec<String>,
    /// Each account's own music (0101 point 9).
    pub own: Vec<String>,
    /// Where the Incoming libraries' files are, when Cratarr can see them.
    pub incoming: Vec<String>,
}

impl Folders {
    /// The main libraries' first, in order, then own music's and Incoming's: the catalogue's folders.
    pub fn all(&self) -> Vec<String> {
        self.music.iter().chain(&self.more_main).chain(&self.own).chain(&self.incoming).cloned().collect()
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
        let f: File = serde_json::from_value(json).with_context(|| format!("{} has an unexpected shape", path.display()))?;
        return Ok(Some(Saved {
            sources: f.sources,
            music_folder: f.music_folder,
            tag_rules: f.tag_rules,
            lidarr: f.lidarr,
            lidarr_removed: f.lidarr_removed,
            trash_days: f.trash_days,
            naming: f.naming,
        }));
    }
    let flat: std::collections::BTreeMap<String, String> =
        serde_json::from_value(json).with_context(|| format!("{} is neither a source list nor the older flat format", path.display()))?;
    Ok(Some(Saved { sources: from_env(&|k: &str| flat.get(k).cloned().or_else(|| env(k))), ..Saved::default() }))
}

/// A file from before 0.16 (version 4), its sources' library roles as 0.16's list: the migration report's "libraries"
/// part (decision 0101, rehearsed on staging, compared with production's copy), what the roles held and the list they
/// become, and the settings of libraries in no role (left out). None when the file holds the list already, or none of
/// the roles (the flat format).
pub fn libraries_migration(path: &Path) -> Result<Option<serde_json::Value>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
    };
    let json: serde_json::Value = serde_json::from_str(&text).with_context(|| format!("{} is not JSON", path.display()))?;
    if json.get("version").and_then(serde_json::Value::as_u64).is_none_or(|v| v >= 4) {
        return Ok(None);
    }
    let (mut roles, mut settings, mut listed) = (0usize, 0usize, 0usize);
    let mut left_out: Vec<String> = Vec::new();
    let mut sources = serde_json::Map::new();
    for s in json.get("sources").and_then(serde_json::Value::as_array).into_iter().flatten() {
        let id = s.get("id").and_then(serde_json::Value::as_str).unwrap_or_default();
        let r: Roles = match s.get("libraries") {
            Some(l) if !l.is_null() => serde_json::from_value(l.clone()).with_context(|| format!("{id}: its libraries have an unexpected shape"))?,
            _ => Roles::default(),
        };
        roles += r.music.len() + r.incoming.len();
        settings += r.policies.len();
        left_out.extend(r.unused().map(|l| format!("{id}/{l}")));
        let list = r.listed();
        listed += list.0.len();
        sources.insert(id.to_string(), serde_json::json!({ "main": list.of(LibraryKind::Main).count(), "incoming": list.of(LibraryKind::Incoming).count() }));
    }
    Ok(Some(serde_json::json!({
        "tables": [
            { "name": "settings: library roles (Music and Incoming ids)", "before": roles, "after": 0 },
            { "name": "settings: library settings by id", "before": settings, "after": 0 },
            { "name": "settings: libraries (the list)", "before": 0, "after": listed },
        ],
        "unmatched": [{ "what": "settings of libraries in no role (left out)", "count": left_out.len(), "sample": left_out.iter().take(10).collect::<Vec<_>>() }],
        "sources": sources,
    })))
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
    let body = File {
        version: 4,
        sources: saved.sources.clone(),
        music_folder: saved.music_folder.clone(),
        tag_rules: saved.tag_rules.clone(),
        lidarr: saved.lidarr.clone(),
        lidarr_removed: saved.lidarr_removed,
        trash_days: saved.trash_days,
        naming: saved.naming.clone(),
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
    vec![SourceConfig { id: "jellyfin".into(), kind: "jellyfin".into(), enabled: true, libraries: Libraries::default(), values }]
}

/// Lidarr from `LIDARR_URL` and `LIDARR_API_KEY` (until one is saved); nothing unless both are set.
pub fn lidarr_from_env(env: &dyn Fn(&str) -> Option<String>) -> Option<LidarrConfig> {
    let get = |k: &str| env(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    Some(LidarrConfig { url: get("LIDARR_URL")?, api_key: get("LIDARR_API_KEY")?, enabled: true, ..Default::default() })
}

/// The Lidarr a start goes by: the one saved, else the environment's, unless Settings took that out. A blank address saves as none,
/// so without this word the environment would undo it at every start.
pub fn lidarr_at_start(saved: &Saved, env: &dyn Fn(&str) -> Option<String>) -> Option<LidarrConfig> {
    saved.lidarr.clone().or_else(|| if saved.lidarr_removed { None } else { lidarr_from_env(env) })
}

/// Whether what a save leaves of Lidarr is the environment's taken out: none, where the environment holds one.
pub fn lidarr_removed(saved: Option<&LidarrConfig>, env: &dyn Fn(&str) -> Option<String>) -> bool {
    saved.is_none() && lidarr_from_env(env).is_some()
}

/// Check the Lidarr from the page against the saved one: a blank address removes it; a blank key keeps the saved
/// key while the address stays the same (the rule of `merge`).
pub fn merge_lidarr(saved: Option<&LidarrConfig>, draft: LidarrConfig) -> Result<Option<LidarrConfig>, ChangeError> {
    let url = draft.url.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        return Ok(None);
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(ChangeError::Invalid("Lidarr: the address must start with http:// or https://".into()));
    }
    let key = draft.api_key.trim().to_string();
    let with = LidarrConfig { enabled: draft.enabled, quality_profile: draft.quality_profile, metadata_profile: draft.metadata_profile, root_folder: draft.root_folder.clone(), ..Default::default() };
    if !key.is_empty() {
        return Ok(Some(LidarrConfig { url, api_key: key, ..with }));
    }
    match saved.filter(|s| !s.api_key.is_empty()) {
        Some(s) if s.url == url => Ok(Some(LidarrConfig { url, api_key: s.api_key.clone(), ..with })),
        Some(_) => Err(ChangeError::SecretNeeded { name: "Lidarr".into(), field: "address".into(), secret: "API key".into() }),
        None => Err(ChangeError::Invalid("Lidarr: the API key is required".into())),
    }
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
        // Cratarr's own folders (decision 0044) are its values though no field of its form shows them (they are chosen in
        // the folder browser): kept, or every save dropped them and the source then had no library (2026-11-02, found
        // on the development instance: "a Music library is no longer on the server" for any change).
        let own_folders = d.kind == crate::source::cratarr::KIND.id;
        d.values.retain(|k, v| {
            (own_folders && k == "folders" && v.is_array())
                || (own_folders && k == OWN_MUSIC && v.is_string())
                || fields.iter().any(|f| {
                    f.key == k
                        && match f.kind {
                            FieldKind::MultiChoice => v.is_array() || v.is_string(),
                            _ => v.is_string(),
                        }
                })
        });
        // The own-music folder holds a subfolder per account: never inside a library's folder, nor around one (two libraries
        // over the same files).
        if let Some(own) = d.own_music() {
            if !own.is_absolute() {
                return Err(ChangeError::Invalid(format!("{}: the own-music folder must be a whole path", d.id)));
            }
            if source::list(&d.values, "folders").iter().any(|c| own.starts_with(c) || Path::new(c).starts_with(&own)) {
                return Err(ChangeError::Invalid(format!("{}: the own-music folder can't lie in a library's folder or hold one", d.id)));
            }
        }
        let prev = saved.iter().find(|s| s.id == d.id && s.kind == d.kind);
        // The libraries in use, in order (0101 point 1): each once, one kind each; own music is no setting (its folder's,
        // 0101 point 9).
        d.libraries.0.retain(|l| !l.id.trim().is_empty());
        let mut listed = std::collections::HashSet::new();
        for l in &d.libraries.0 {
            if !listed.insert(l.id.as_str()) {
                return Err(ChangeError::Invalid(format!("{}: {} is listed twice: a library has one kind", d.id, l.id)));
            }
            if l.kind == LibraryKind::Own {
                return Err(ChangeError::Invalid(format!("{}: {} is own music, which comes from the own-music folder, not the list", d.id, l.id)));
            }
        }
        let mains: Vec<String> = d.libraries.of(LibraryKind::Main).map(str::to_string).collect();
        let mut fetched_by: Vec<String> = Vec::new();
        for l in d.libraries.0.iter_mut() {
            let lib = l.id.clone();
            if let Some(c) = &l.colour {
                if !valid_colour(c) {
                    return Err(ChangeError::Invalid(format!("{}: {lib}'s colour {c:?} is not #rrggbb", d.id)));
                }
                l.colour = Some(c.to_ascii_lowercase());
            }
            // A name kept trimmed; blank is none (the folder's name again).
            l.name = l.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
            if let Some(n) = &l.name
                && (n.chars().count() > NAME_MOST || n.chars().any(char::is_control))
            {
                return Err(ChangeError::Invalid(format!("{}: {lib}'s name is longer than {NAME_MOST} characters or holds control characters", d.id)));
            }
            // Its fetcher (decision 0099's amendment: Lidarr and Soulseek siblings): one of them, an Incoming library's, one
            // library each.
            l.fetcher = l.fetcher.as_deref().map(str::trim).filter(|f| !f.is_empty()).map(str::to_string);
            if let Some(f) = l.fetcher.as_deref() {
                if !FETCHERS.contains(&f) {
                    return Err(ChangeError::Invalid(format!("{}: {lib}'s fetcher {f:?} is not one of {}", d.id, FETCHERS.join(", "))));
                }
                if l.kind != LibraryKind::Incoming {
                    return Err(ChangeError::Invalid(format!("{}: {lib} is not an Incoming library: only an Incoming library is fetched into", d.id)));
                }
                if fetched_by.iter().any(|x| x == f) {
                    return Err(ChangeError::Invalid(format!("{}: {f} fetches into one Incoming library only", d.id)));
                }
                fetched_by.push(f.to_string());
            }
            // Where its records are kept by default (0101 point 2): one of the server's main libraries, an Incoming
            // library's only.
            l.keep_to = l.keep_to.as_deref().map(str::trim).filter(|k| !k.is_empty()).map(str::to_string);
            if let Some(k) = l.keep_to.as_deref() {
                if l.kind != LibraryKind::Incoming {
                    return Err(ChangeError::Invalid(format!("{}: {lib} is not an Incoming library: only an Incoming library's records are kept", d.id)));
                }
                if !mains.iter().any(|m| m == k) {
                    return Err(ChangeError::Invalid(format!("{}: {lib} keeps into {k}, which is not one of its main libraries", d.id)));
                }
            }
        }
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

    fn list(entries: &[(&str, LibraryKind)]) -> Libraries {
        Libraries(entries.iter().map(|(id, k)| LibrarySetting::new(*id, *k)).collect())
    }

    #[test]
    fn a_library_follows_its_kind_until_it_has_its_own_settings() {
        let mut r = list(&[("m", LibraryKind::Main), ("i1", LibraryKind::Incoming), ("i2", LibraryKind::Incoming)]);
        assert_eq!(r.quality_of("m"), Quality::Cd);
        assert_eq!(r.quality_of("i1"), Quality::None);
        assert_eq!(r.quality_of("unused"), Quality::None);
        assert_eq!(r.colour_of("m").as_deref(), Some(PALETTE[2]), "a main library has one too, after Incoming's");
        assert_eq!(r.colour_of("i1").as_deref(), Some(PALETTE[0]));
        assert_eq!(r.colour_of("i2").as_deref(), Some(PALETTE[1]));
        assert_eq!(r.colour_of("unused"), None);
        r.0[1].quality = Some(Quality::Hr48);
        r.0[1].colour = Some("#112233".into());
        assert_eq!(r.quality_of("i1"), Quality::Hr48);
        assert_eq!(r.colour_of("i1").as_deref(), Some("#112233"));
        // As saved: only what was set, in the list's order.
        assert_eq!(
            serde_json::to_value(&r).unwrap(),
            serde_json::json!([{ "id": "m", "kind": "main" }, { "id": "i1", "kind": "incoming", "colour": "#112233", "quality": "hr48" }, { "id": "i2", "kind": "incoming" }])
        );
        // Own music after the main libraries, in the palette's turn.
        let used = [("m", LibraryKind::Main), ("o", LibraryKind::Own), ("i", LibraryKind::Incoming)];
        assert_eq!(colour_among(used.into_iter(), "o", None).as_deref(), Some(PALETTE[2]));
        assert_eq!(LibraryKind::Own.quality(), Quality::Cd);
    }

    #[test]
    fn a_format_saved_before_the_quality_limit_reads_as_its_limit() {
        let read = |v: serde_json::Value| serde_json::from_value::<LibraryPolicy>(v).unwrap().quality;
        assert_eq!(read(serde_json::json!({ "format": "cd" })), Some(Quality::Cd));
        assert_eq!(read(serde_json::json!({ "format": "as_is" })), Some(Quality::None));
        assert_eq!(read(serde_json::json!({ "quality": "hr96" })), Some(Quality::Hr96));
        assert_eq!(read(serde_json::json!({ "quality": "none" })), Some(Quality::None));
        // Written only as the limit.
        let l = LibrarySetting { quality: Some(Quality::None), ..LibrarySetting::new("m", LibraryKind::Main) };
        assert_eq!(serde_json::to_value(&l).unwrap(), serde_json::json!({ "id": "m", "kind": "main", "quality": "none" }));
        assert_eq!(Quality::None.limit(), None);
        assert_eq!(Quality::Hr96.limit().map(|l| (l.bits, l.rate)), Some((24, 96_000)));
        assert_eq!(Quality::Cd.limit(), Some(crate::convert::CD));
    }

    #[test]
    fn cratarrs_own_folders_are_kept_through_a_save() {
        let own = |folders: serde_json::Value| SourceConfig {
            id: "cratarr".into(),
            kind: crate::source::cratarr::KIND.id.into(),
            enabled: true,
            libraries: list(&[("/music", LibraryKind::Main), ("/incoming", LibraryKind::Incoming)]),
            values: [("folders".to_string(), folders)].into_iter().collect(),
        };
        let out = merge(&[], vec![own(serde_json::json!(["/music", "/incoming"]))]).unwrap();
        assert_eq!(out[0].values.get("folders"), Some(&serde_json::json!(["/music", "/incoming"])));
        // Only as a list, and only Cratarr's own: another kind's undeclared values still go.
        let out = merge(&[], vec![own(serde_json::json!("/music"))]).unwrap();
        assert!(out[0].values.get("folders").is_none());
    }

    #[test]
    fn the_own_music_folder_is_kept_and_checked() {
        let own = |folder: &str| SourceConfig {
            id: "cratarr".into(),
            kind: crate::source::cratarr::KIND.id.into(),
            enabled: true,
            libraries: Libraries::default(),
            values: [("folders".to_string(), serde_json::json!(["/music", "/media/x"])), (OWN_MUSIC.to_string(), serde_json::json!(folder))].into_iter().collect(),
        };
        let out = merge(&[], vec![own("/own")]).unwrap();
        assert_eq!(out[0].own_music(), Some(std::path::PathBuf::from("/own")));
        assert_eq!(merge(&[], vec![own(" ")]).unwrap()[0].own_music(), None, "blank: none");
        assert!(merge(&[], vec![own("own")]).is_err(), "a whole path");
        assert!(merge(&[], vec![own("/music/own")]).is_err(), "not inside a library");
        assert!(merge(&[], vec![own("/media")]).is_err(), "not around one");
        let mut other = jf("jf", "http://a", Some("k"));
        other.values.insert(OWN_MUSIC.into(), "/own".into());
        assert_eq!(merge(&[], vec![other]).unwrap()[0].own_music(), None, "Cratarr's own source's only");
    }

    #[test]
    fn library_settings_are_checked() {
        let with = |entries: Vec<LibrarySetting>| {
            let mut s = jf("jf", "http://a", Some("k"));
            s.libraries = Libraries(entries);
            merge(&[], vec![s])
        };
        let main = |id: &str| LibrarySetting::new(id, LibraryKind::Main);
        let incoming = |id: &str| LibrarySetting::new(id, LibraryKind::Incoming);
        // A colour as #rrggbb, kept in lower case.
        assert!(with(vec![LibrarySetting { colour: Some("red".into()), ..main("m") }]).is_err());
        let out = with(vec![LibrarySetting { colour: Some("#AABBCC".into()), ..main("m") }]).unwrap();
        assert_eq!(out[0].libraries.0[0].colour.as_deref(), Some("#aabbcc"));
        // A name of its own, kept trimmed; blank is none again.
        let out = with(vec![LibrarySetting { name: Some("  Vinyl rips ".into()), ..incoming("i") }, LibrarySetting { name: Some("   ".into()), ..incoming("j") }]).unwrap();
        assert_eq!(out[0].libraries.name_of("i").as_deref(), Some("Vinyl rips"));
        assert_eq!(out[0].libraries.name_of("j"), None);
        assert!(with(vec![LibrarySetting { name: Some("x".repeat(81)), ..main("m") }]).is_err());
        // A fetcher: only one of the siblings, only an Incoming library's, one library each.
        let out = with(vec![main("m"), LibrarySetting { fetcher: Some("soulseek".into()), ..incoming("i") }, incoming("k")]).unwrap();
        assert_eq!(out[0].libraries.fetched_by("soulseek"), Some("i"));
        assert!(with(vec![LibrarySetting { fetcher: Some("soulseek".into()), ..incoming("i") }, LibrarySetting { fetcher: Some("soulseek".into()), ..incoming("k") }]).is_err());
        assert!(with(vec![LibrarySetting { fetcher: Some("usenet".into()), ..incoming("i") }]).is_err());
        assert!(with(vec![LibrarySetting { fetcher: Some("lidarr".into()), ..main("m") }]).is_err());
        // Keep's default: one of the server's main libraries, an Incoming library's only.
        let out = with(vec![main("m"), main("n"), LibrarySetting { keep_to: Some("n".into()), ..incoming("i") }]).unwrap();
        assert_eq!(out[0].libraries.get("i").and_then(|l| l.keep_to.as_deref()), Some("n"));
        assert!(with(vec![main("m"), LibrarySetting { keep_to: Some("elsewhere".into()), ..incoming("i") }]).is_err());
        assert!(with(vec![main("m"), LibrarySetting { keep_to: Some("m".into()), ..main("n") }]).is_err());
    }

    fn jf(id: &str, url: &str, key: Option<&str>) -> SourceConfig {
        let mut values = Values::new();
        values.insert("url".into(), url.into());
        values.insert("player".into(), "Finamp".into());
        if let Some(k) = key {
            values.insert("api_key".into(), k.into());
        }
        SourceConfig { id: id.into(), kind: "jellyfin".into(), enabled: true, libraries: Libraries::default(), values }
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
    fn a_library_is_listed_once_with_one_kind() {
        let mut s = jf("a", "http://x", Some("k"));
        s.libraries = list(&[("l1", LibraryKind::Main), (" ", LibraryKind::Main), ("l2", LibraryKind::Incoming)]);
        let out = merge(&[], vec![s.clone()]).unwrap();
        assert_eq!(out[0].libraries, list(&[("l1", LibraryKind::Main), ("l2", LibraryKind::Incoming)]), "a blank one goes");
        s.libraries.0.push(LibrarySetting::new("l1", LibraryKind::Incoming));
        assert!(merge(&[], vec![s.clone()]).is_err(), "one library, one kind");
        s.libraries = list(&[("/own/ann", LibraryKind::Own)]);
        assert!(merge(&[], vec![s]).is_err(), "own music comes from its folder");
    }

    /// Settings' word stands: a Lidarr removed there stays removed at every start, however the environment reads, and one saved
    /// afterwards counts as any other. The environment holding none says nothing of a later one.
    #[test]
    fn a_lidarr_removed_in_settings_stays_removed_whatever_the_environment_says() {
        let env = env_of(&[("LIDARR_URL", "http://lidarr:8686"), ("LIDARR_API_KEY", "k")]);
        let none = env_of(&[]);
        let d = std::env::temp_dir().join(format!("cratarr-lidarr-removed-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let file = d.join("settings.json");
        assert!(lidarr_at_start(&Saved::default(), &env).is_some(), "the environment's, until Settings says");
        let removed = Saved { lidarr_removed: lidarr_removed(None, &env), ..Saved::default() };
        save(&file, &removed).unwrap();
        let read = load(&file, &env).unwrap().unwrap();
        assert!(read.lidarr_removed && lidarr_at_start(&read, &env).is_none(), "taken out, across a restart");
        let saved = LidarrConfig { url: "http://other:8686".into(), api_key: "k2".into(), enabled: true, ..Default::default() };
        assert!(!lidarr_removed(Some(&saved), &env), "one saved takes the mark away");
        assert!(!lidarr_removed(None, &none), "a save with nothing to take out marks nothing");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn lidarr_keeps_its_key_only_for_the_same_address() {
        let saved = LidarrConfig { url: "http://lidarr:8686".into(), api_key: "k".into(), enabled: true, ..Default::default() };
        let draft = |url: &str, key: &str| LidarrConfig { url: url.into(), api_key: key.into(), enabled: true, ..Default::default() };
        assert_eq!(merge_lidarr(Some(&saved), draft("http://lidarr:8686/", "")).unwrap(), Some(saved.clone()));
        assert!(matches!(merge_lidarr(Some(&saved), draft("http://evil:8686", "")), Err(ChangeError::SecretNeeded { .. })));
        assert_eq!(merge_lidarr(Some(&saved), draft("http://new:8686", "k2")).unwrap().unwrap().api_key, "k2");
        assert_eq!(merge_lidarr(Some(&saved), draft(" ", "")).unwrap(), None, "a blank address removes it");
        assert!(merge_lidarr(None, draft("http://lidarr:8686", "")).is_err(), "a new one needs its key");
        assert!(merge_lidarr(None, draft("lidarr:8686", "k")).is_err());
        let env = env_of(&[("LIDARR_URL", "http://lidarr:8686"), ("LIDARR_API_KEY", "k")]);
        assert_eq!(lidarr_from_env(&env), Some(saved));
        assert_eq!(lidarr_from_env(&env_of(&[("LIDARR_URL", "http://lidarr:8686")])), None);
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
        s[0].libraries = list(&[("m", LibraryKind::Main), ("i1", LibraryKind::Incoming), ("i2", LibraryKind::Incoming)]);
        let lidarr = Some(LidarrConfig { url: "http://lidarr:8686".into(), api_key: "k".into(), enabled: true, ..Default::default() });
        let saved = Saved {
            sources: s,
            music_folder: Some("/music".into()),
            tag_rules: Default::default(),
            lidarr,
            lidarr_removed: false,
            trash_days: 30,
            naming: Some(crate::naming::DEFAULT.into()),
        };
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
    fn a_file_before_0_16_reads_its_roles_as_the_list_and_says_so() {
        let d = dir();
        let path = d.join("settings.json");
        std::fs::write(
            &path,
            r##"{"version":3,"sources":[{"id":"cratarr","kind":"cratarr","libraries":{"music":["/b","/a"],"incoming":["/in","/a"],"policies":{"/in":{"format":"cd","fetcher":"lidarr","colour":"#112233"},"/a":{"name":"Vinyl","fetcher":"soulseek"},"/gone":{"name":"Old"}}},"values":{"folders":["/a","/b","/in"]}}]}"##,
        )
        .unwrap();
        let report = libraries_migration(&path).unwrap().expect("a file before 0.16");
        let s = load(&path, &env_of(&[])).unwrap().unwrap();
        let main = |id: &str| LibrarySetting::new(id, LibraryKind::Main);
        assert_eq!(
            s.sources[0].libraries.0,
            [
                main("/b"),
                // In both: Music's, and a fetcher only an Incoming library's.
                LibrarySetting { name: Some("Vinyl".into()), ..main("/a") },
                LibrarySetting { fetcher: Some("lidarr".into()), colour: Some("#112233".into()), quality: Some(Quality::Cd), ..LibrarySetting::new("/in", LibraryKind::Incoming) },
            ],
            "Music's first in their order, then Incoming's"
        );
        assert_eq!((report["tables"][0]["before"].as_u64(), report["tables"][1]["before"].as_u64(), report["tables"][2]["after"].as_u64()), (Some(4), Some(3), Some(3)));
        assert_eq!(report["unmatched"][0]["sample"], serde_json::json!(["cratarr//gone"]), "the settings of a library in no role");
        assert_eq!(report["sources"]["cratarr"], serde_json::json!({ "main": 2, "incoming": 1 }));
        // Saved again it is 0.16's: nothing more to report, and it reads back the same.
        save(&path, &s).unwrap();
        assert_eq!(libraries_migration(&path).unwrap(), None);
        assert_eq!(load(&path, &env_of(&[])).unwrap().unwrap(), s);
        // A version 2 file: one Incoming id or none.
        std::fs::write(
            &path,
            r#"{"version":2,"sources":[{"id":"jellyfin","kind":"jellyfin","libraries":{"music":["a"],"incoming":"b"},"values":{"url":"http://jf"}},{"id":"other","kind":"jellyfin","libraries":{"music":[],"incoming":null}}],"folders":["/x"]}"#,
        )
        .unwrap();
        let s = load(&path, &env_of(&[])).unwrap().unwrap();
        assert_eq!(s.sources[0].libraries, list(&[("a", LibraryKind::Main), ("b", LibraryKind::Incoming)]));
        assert_eq!(s.sources[1].libraries, Libraries::default());
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
