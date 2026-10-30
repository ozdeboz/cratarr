//! A directory (LDAP) asked first at sign-in (decision 0040): LLDAP, OpenLDAP, Active Directory. Cratarr's own accounts
//! stay the fallback (decision 0039) and the Subsonic API keeps them alone: its logins need the password itself, which
//! a directory never gives.
//!
//! A sign-in looks the name up as a read-only service account (its password read from a file at each use, never kept,
//! logged or shown), within the base and, when one is set, the group; then binds as the user found with the password
//! typed. A directory name signs in as the Cratarr account of the same name, or the one mapped to it; that account
//! must exist. Every exchange has a short timeout. While the directory cannot be reached (no connection, no answer in
//! time; not when it refuses Cratarr's service account, which fails closed), a sign-in it accepted in the last 30 days
//! still works through a slow hash of its password (Argon2id, owner-only file); a refusal by the directory
//! deletes that hash at once, so a password changed or a user removed there stops working here too. The name may be
//! the user's address (`mail`) too; the hash is kept by directory name only, so while the directory is away only that
//! name signs in.
//!
//! The name a user goes by is read with them, at sign-in and in the hourly check: `displayName` (LLDAP's), else `cn`,
//! else none. The page greets them with it.
//!
//! An empty password is refused before any exchange (a bind with one is an anonymous bind, which succeeds), and so is
//! a name with control characters. Names and group go into the search filter escaped (RFC 4515). Plain LDAP is
//! accepted only to a host on this machine's networks (a container name, localhost, a private address); anything
//! further needs `ldaps://` or StartTLS.
//!
//! The settings are the operator's: `ldap.json` in the data folder, read and checked at start. The page shows them and
//! never sets them, as a page that could set the address or the password file could send any file's contents to any
//! server, or sign anyone in as the owner through a directory of its own.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::Serve;

/// Each exchange with the directory (connecting, each bind, the search).
const TIMEOUT: Duration = Duration::from_secs(3);
/// How long a sign-in the directory accepted still works while it cannot be reached.
const KEEP_SECS: i64 = 30 * 24 * 3600;
/// Argon2id at OWASP's minimum: 19 MiB, two passes, one lane.
const ARGON_MEMORY_KIB: u32 = 19 * 1024;
const ARGON_PASSES: u32 = 2;

/// The directory settings (Settings, Authentication), kept in `ldap.json` in the data folder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    /// `ldap://host:port` or `ldaps://host:port`.
    pub url: String,
    /// StartTLS on a plain `ldap://` connection.
    pub starttls: bool,
    /// Where users are looked up, e.g. `ou=people,dc=example,dc=com`.
    pub base_dn: String,
    /// The attribute holding the name typed at sign-in (`uid`; `sAMAccountName` in Active Directory).
    pub user_attribute: String,
    /// A group's DN the user must be a member of (`memberOf`); empty: any user under the base.
    pub group: String,
    /// A group's DN whose members are Cratarr's administrators (decision 0051: `media_admin`); empty: the directory makes
    /// no one an administrator.
    pub admin_group: String,
    /// The read-only service account the lookup binds as.
    pub bind_dn: String,
    /// The file holding the service account's password.
    pub bind_password_file: String,
    /// Directory name to Cratarr account, where they differ.
    pub accounts: BTreeMap<String, String>,
}

impl Config {
    fn attribute(&self) -> &str {
        if self.user_attribute.trim().is_empty() { "uid" } else { self.user_attribute.trim() }
    }

    /// What is wrong with these settings, for saving them (switched off, anything may be kept).
    fn check(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let (scheme, rest) = self.url.split_once("://").context("The address needs ldap:// or ldaps://.")?;
        let host = rest.split(['/', '?']).next().unwrap_or("");
        let host = host.rsplit_once(':').map_or(host, |(h, p)| if p.chars().all(|c| c.is_ascii_digit()) { h } else { host });
        let host = host.trim_start_matches('[').trim_end_matches(']');
        match scheme {
            "ldaps" => {}
            "ldap" if self.starttls || nearby(host) => {}
            "ldap" => bail!("Plain LDAP only to this machine's networks; use ldaps:// or StartTLS for {host}."),
            _ => bail!("The address needs ldap:// or ldaps://."),
        }
        if host.is_empty() {
            bail!("The address has no host.");
        }
        if self.base_dn.trim().is_empty() || self.bind_dn.trim().is_empty() || self.bind_password_file.trim().is_empty() {
            bail!("The base, the service account and its password file are needed.");
        }
        if !self.attribute().chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            bail!("The user attribute is a plain attribute name.");
        }
        Ok(())
    }
}

/// A host on this machine's networks: a container's name (no dots), localhost, or a private or loopback address.
fn nearby(host: &str) -> bool {
    if host == "localhost" || (!host.is_empty() && !host.contains('.') && !host.contains(':')) {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(a)) => a.is_private() || a.is_loopback(),
        Ok(std::net::IpAddr::V6(a)) => a.is_loopback() || (a.segments()[0] & 0xfe00) == 0xfc00,
        Err(_) => false,
    }
}

/// A value inside a search filter, escaped as RFC 4515 says.
pub fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\5c"),
            '*' => out.push_str("\\2a"),
            '(' => out.push_str("\\28"),
            ')' => out.push_str("\\29"),
            '\0' => out.push_str("\\00"),
            c => out.push(c),
        }
    }
    out
}

/// The search for one user: the name in its attribute or as their address (`mail`; 0.9.8, the owner: "Both should be
/// accepted", as Jellyfin's LDAP plugin takes either), and the group when one is set.
fn filter(attribute: &str, name: &str, group: &str) -> String {
    let name = escape(name);
    let user = if attribute.eq_ignore_ascii_case(MAIL) { format!("({attribute}={name})") } else { format!("(|({attribute}={name})({MAIL}={name}))") };
    if group.trim().is_empty() { user } else { format!("(&{user}(memberOf={}))", escape(group.trim())) }
}

/// The address attribute a sign-in name is also looked up in.
const MAIL: &str = "mail";

/// Where the name a user goes by is, the first found: the standard attribute (LLDAP's), then the common name.
const DISPLAY_NAME: [&str; 2] = ["displayName", "cn"];

/// The attributes a search for users asks for: the name and the display name's.
fn wanted(attribute: &str) -> Vec<&str> {
    vec![attribute, DISPLAY_NAME[0], DISPLAY_NAME[1]]
}

/// An attribute's first value, its name matched regardless of case (as directories match them).
fn first<'a>(attrs: &'a HashMap<String, Vec<String>>, name: &str) -> Option<&'a String> {
    attrs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).and_then(|(_, v)| v.first())
}

/// The name a user goes by, from their entry's attributes: `displayName`, else `cn`, else none; trimmed, an empty one
/// being none.
fn display_name(attrs: &HashMap<String, Vec<String>>) -> Option<String> {
    DISPLAY_NAME.iter().find_map(|a| first(attrs, a).map(|v| v.trim()).filter(|v| !v.is_empty()).map(str::to_string))
}

/// A user the directory has: their name as it spells it, and the name they go by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub display_name: Option<String>,
}

/// What the directory says of a user it signed in: whether an administrator, and the name they go by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    pub admin: bool,
    pub display_name: Option<String>,
}

/// The one form a name's kept hash is filed under: directories match names regardless of case, so a refusal for
/// "ROBIN" must delete what an earlier sign-in kept for "robin".
fn key(name: &str) -> String {
    name.to_lowercase()
}

/// Whether a name and password may be put to the directory at all.
fn acceptable(name: &str, password: &str) -> bool {
    !password.is_empty() && !name.is_empty() && name.len() <= 256 && !name.chars().any(char::is_control)
}

/// The directory's answer to a name and password.
#[derive(Debug, PartialEq, Eq)]
enum Answer {
    /// Signed in as this directory name (as the directory spells it), with whether it is in the administrators' group and
    /// its display name.
    Yes(String, Said),
    /// Refused: no such user (in the group), or the wrong password.
    No,
    /// Not reachable: no connection, or no answer in time. A recent sign-in's kept hash may stand in.
    Unreachable,
    /// Reachable but refusing Cratarr itself: the service account refused, its password file unreadable, the search
    /// refused. A misconfiguration, or the service account disabled on purpose: no kept hash stands in (fail closed),
    /// though none is deleted either; Cratarr's own accounts still sign in.
    Misconfigured,
}

/// Why an exchange failed, for the sign-in (which fails closed on a misconfiguration) and Settings' Test.
#[derive(Debug)]
enum Failure {
    Unreachable(String),
    Misconfigured(String),
}

impl Failure {
    fn answer(&self) -> Answer {
        match self {
            Failure::Unreachable(_) => Answer::Unreachable,
            Failure::Misconfigured(_) => Answer::Misconfigured,
        }
    }

    fn why(&self) -> &str {
        match self {
            Failure::Unreachable(r) | Failure::Misconfigured(r) => r,
        }
    }

}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Verifier {
    /// The name as the directory spells it (what `accounts` maps); kept under `key(name)`.
    name: String,
    /// Argon2id, as a PHC string (its salt and parameters in it).
    hash: String,
    /// When the directory last accepted this name's sign-in (seconds).
    at: i64,
}

pub struct Directory {
    /// Where `set_config` keeps new settings; only tests set them (decision 0040).
    #[cfg(test)]
    config_file: Option<PathBuf>,
    verifier_file: Option<PathBuf>,
    config: RwLock<Config>,
    verifiers: Mutex<HashMap<String, Verifier>>,
    /// One Argon2 hash at a time (19 MiB each).
    hashing: tokio::sync::Semaphore,
}

impl Directory {
    /// The settings and kept hashes from the data folder (`None`: in memory).
    pub fn open(data_dir: Option<&Path>) -> Result<Self> {
        let config_file = data_dir.map(|d| d.join("ldap.json"));
        let verifier_file = data_dir.map(|d| d.join("ldap-verifiers.json"));
        let mut config: Config = match &config_file {
            Some(f) if f.exists() => serde_json::from_str(&std::fs::read_to_string(f)?).with_context(|| format!("reading {}", f.display()))?,
            _ => Config::default(),
        };
        if let Err(e) = config.check() {
            tracing::warn!("ldap: ldap.json is not used: {e:#}");
            config.enabled = false;
        }
        let verifiers = match &verifier_file {
            Some(f) if f.exists() => serde_json::from_str(&std::fs::read_to_string(f)?).unwrap_or_default(),
            _ => HashMap::new(),
        };
        Ok(Self {
            #[cfg(test)]
            config_file,
            verifier_file,
            config: RwLock::new(config),
            verifiers: Mutex::new(verifiers),
            hashing: tokio::sync::Semaphore::new(1),
        })
    }

    pub fn config(&self) -> Config {
        self.config.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// New settings, checked and kept. Kept hashes go when the directory or its base changes. Not reachable from the
    /// page (decision 0040): the operator writes `ldap.json`.
    #[cfg(test)]
    pub fn set_config(&self, new: Config) -> Result<()> {
        new.check()?;
        let old = self.config();
        if let Some(f) = &self.config_file {
            write_private(f, &serde_json::to_string_pretty(&new)?)?;
        }
        if (old.url.as_str(), old.base_dn.as_str(), old.group.as_str()) != (new.url.as_str(), new.base_dn.as_str(), new.group.as_str()) {
            self.verifiers.lock().unwrap_or_else(|p| p.into_inner()).clear();
            self.save_verifiers();
        }
        *self.config.write().unwrap_or_else(|p| p.into_inner()) = new;
        Ok(())
    }

    /// The Cratarr account a name and password sign in as through the directory; `None` when it is off, refuses them,
    /// or cannot be reached and has no recent sign-in of theirs. `account`: the Cratarr account for a name (the one
    /// `accounts` maps it to, else the same), given what the directory says of it (whether an administrator, its display
    /// name; `None`: not asked, it was unreachable); made or brought back where the directory signed it in (decision
    /// 0051), only found otherwise.
    pub async fn sign_in(&self, name: &str, password: &str, account: &(dyn Fn(&str, Option<Said>) -> Option<String> + Sync), now: i64) -> Option<String> {
        let config = self.config();
        if !config.enabled || !acceptable(name, password) {
            return None;
        }
        let mapped = |directory_name: &str| config.accounts.get(directory_name).cloned().unwrap_or_else(|| directory_name.to_string());
        match ask(&config, name, password).await {
            Answer::Yes(directory_name, said) => {
                let account = account(&mapped(&directory_name), Some(said))?;
                self.remember(&directory_name, password, now).await;
                Some(account)
            }
            Answer::No => {
                self.forget(name);
                None
            }
            Answer::Unreachable => {
                tracing::warn!(target: "signin", user = %name, reason = "directory unreachable", "directory sign-in: a recent one tried");
                self.recalls(name, password, now).await.and_then(|spelled| account(&mapped(&spelled), None))
            }
            Answer::Misconfigured => None,
        }
    }

    /// The directory's members now (decision 0051's hourly check): the Cratarr accounts of the sign-in group's members
    /// (with their display names), and of the administrators' group's; `None` when it is off, unreachable or refusing
    /// (then no one is let go).
    pub async fn census(&self) -> Option<(Vec<Member>, Vec<String>)> {
        let config = self.config();
        if !config.enabled {
            return None;
        }
        let exchange = async {
            let (mut ldap, _) = connect(&config).await.ok()?;
            let members = members(&mut ldap, &config, &config.group).await;
            let admins = if config.admin_group.trim().is_empty() { Ok(Vec::new()) } else { members_of_admins(&mut ldap, &config).await };
            let _ = ldap.unbind().await;
            let account = |n: String| config.accounts.get(&n).cloned().unwrap_or(n);
            match (members, admins) {
                (Ok(m), Ok(a)) => Some((
                    m.into_iter().map(|m| Member { name: account(m.name), ..m }).collect(),
                    a.into_iter().map(|m| account(m.name)).collect(),
                )),
                (m, a) => {
                    let why = m.err().or(a.err()).map(|f| f.why().to_string()).unwrap_or_default();
                    tracing::warn!(target: "signin", reason = %why, "directory census not taken");
                    None
                }
            }
        };
        tokio::time::timeout(TIMEOUT * 4, exchange).await.ok().flatten()
    }

    /// Forgets a name's kept sign-in (one who left: the directory down must not let them in).
    pub fn forget_account(&self, name: &str) {
        self.forget(name);
    }

    async fn remember(&self, name: &str, password: &str, now: i64) {
        let Ok(_one) = self.hashing.acquire().await else { return };
        let password = password.to_string();
        let Ok(Ok(hash)) = tokio::task::spawn_blocking(move || hash(&password)).await else { return };
        self.verifiers.lock().unwrap_or_else(|p| p.into_inner()).insert(key(name), Verifier { name: name.to_string(), hash, at: now });
        self.save_verifiers();
    }

    fn forget(&self, name: &str) {
        let gone = self.verifiers.lock().unwrap_or_else(|p| p.into_inner()).remove(&key(name)).is_some();
        if gone {
            self.save_verifiers();
        }
    }

    /// The directory's spelling of the name, when a recent sign-in kept for it matches this password.
    async fn recalls(&self, name: &str, password: &str, now: i64) -> Option<String> {
        let v = self.verifiers.lock().unwrap_or_else(|p| p.into_inner()).get(&key(name)).cloned()?;
        if now - v.at > KEEP_SECS {
            self.forget(name);
            return None;
        }
        let _one = self.hashing.acquire().await.ok()?;
        let (hash, password) = (v.hash, password.to_string());
        tokio::task::spawn_blocking(move || verify(&hash, &password)).await.unwrap_or(false).then_some(v.name)
    }

    fn save_verifiers(&self) {
        let Some(f) = &self.verifier_file else { return };
        let text = serde_json::to_string(&*self.verifiers.lock().unwrap_or_else(|p| p.into_inner())).unwrap_or_default();
        if let Err(e) = write_private(f, &text) {
            tracing::warn!("ldap: could not keep the sign-ins' hashes: {e:#}");
        }
    }
}

/// Connected and bound as the service account; the error is the reason, for Settings.
async fn connect(config: &Config) -> std::result::Result<(ldap3::Ldap, ()), Failure> {
    let password = std::fs::read_to_string(&config.bind_password_file)
        .map_err(|_| Failure::Misconfigured("The service account's password file cannot be read.".into()))?;
    let password = password.trim_end_matches(['\n', '\r']);
    let settings = ldap3::LdapConnSettings::new().set_conn_timeout(TIMEOUT).set_starttls(config.starttls);
    let (conn, mut ldap) = ldap3::LdapConnAsync::with_settings(settings, &config.url).await.map_err(|e| {
        tracing::debug!("ldap: connecting: {e}");
        Failure::Unreachable("The directory cannot be reached.".into())
    })?;
    ldap3::drive!(conn);
    match ldap.with_timeout(TIMEOUT).simple_bind(&config.bind_dn, password).await {
        Ok(r) if r.rc == 0 => Ok((ldap, ())),
        Ok(r) => {
            let _ = ldap.unbind().await;
            Err(Failure::Misconfigured(format!("The directory refused the service account ({}).", r.rc)))
        }
        Err(_) => Err(Failure::Unreachable("The directory did not answer.".into())),
    }
}

/// The DN of the user with this name (in the group), and the user as the directory has them; `None`: not found.
async fn find(ldap: &mut ldap3::Ldap, config: &Config, name: &str) -> std::result::Result<Option<(String, Member)>, Failure> {
    let attribute = config.attribute();
    let found = ldap
        .with_timeout(TIMEOUT)
        .search(&config.base_dn, ldap3::Scope::Subtree, &filter(attribute, name, &config.group), wanted(attribute))
        .await
        .map_err(|_| Failure::Unreachable("The directory did not answer the search.".into()))?
        .success()
        .map_err(|e| Failure::Misconfigured(format!("The search was refused: {e}")))?
        .0;
    if found.len() != 1 {
        return Ok(None);
    }
    let entry = ldap3::SearchEntry::construct(found.into_iter().next().expect("one"));
    let spelled = first(&entry.attrs, attribute).cloned().unwrap_or_else(|| name.to_string());
    let display_name = display_name(&entry.attrs);
    Ok(Some((entry.dn, Member { name: spelled, display_name })))
}

/// Whether `name` is a member of `group` (none set: no one is), asked on a connection bound as the service account.
async fn member(ldap: &mut ldap3::Ldap, config: &Config, name: &str, group: &str) -> std::result::Result<bool, Failure> {
    if group.trim().is_empty() {
        return Ok(false);
    }
    let attribute = config.attribute();
    let found = ldap
        .with_timeout(TIMEOUT)
        .search(&config.base_dn, ldap3::Scope::Subtree, &filter(attribute, name, group), vec![attribute])
        .await
        .map_err(|_| Failure::Unreachable("The directory did not answer the search.".into()))?
        .success()
        .map_err(|e| Failure::Misconfigured(format!("The search was refused: {e}")))?
        .0;
    Ok(found.len() == 1)
}

/// `group`'s members (any user under the base when none is set): their names as the directory spells them and their
/// display names, asked on a connection bound as the service account.
async fn members(ldap: &mut ldap3::Ldap, config: &Config, group: &str) -> std::result::Result<Vec<Member>, Failure> {
    let attribute = config.attribute();
    let filter = if group.trim().is_empty() { format!("({attribute}=*)") } else { format!("(&({attribute}=*)(memberOf={}))", escape(group.trim())) };
    let found = ldap
        .with_timeout(TIMEOUT)
        .search(&config.base_dn, ldap3::Scope::Subtree, &filter, wanted(attribute))
        .await
        .map_err(|_| Failure::Unreachable("The directory did not answer the search.".into()))?
        .success()
        .map_err(|e| Failure::Misconfigured(format!("The search was refused: {e}")))?
        .0;
    Ok(found
        .into_iter()
        .filter_map(|e| {
            let entry = ldap3::SearchEntry::construct(e);
            let name = first(&entry.attrs, attribute).cloned()?;
            Some(Member { name, display_name: display_name(&entry.attrs) })
        })
        .collect())
}

async fn members_of_admins(ldap: &mut ldap3::Ldap, config: &Config) -> std::result::Result<Vec<Member>, Failure> {
    members(ldap, config, &config.admin_group).await
}

async fn ask(config: &Config, name: &str, password: &str) -> Answer {
    let exchange = async {
        let (mut ldap, _) = match connect(config).await {
            Ok(c) => c,
            Err(why) => {
                tracing::warn!(target: "signin", reason = %why.why(), "directory not usable");
                return why.answer();
            }
        };
        let answer = match find(&mut ldap, config, name).await {
            Err(why) => {
                tracing::warn!(target: "signin", reason = %why.why(), "directory not usable");
                why.answer()
            }
            Ok(None) => Answer::No,
            Ok(Some((dn, found))) => {
                // Whether an administrator, asked as the service account, before binding as the user (who may not
                // search).
                let admin = match member(&mut ldap, config, &found.name, &config.admin_group).await {
                    Ok(admin) => admin,
                    Err(why) => {
                        tracing::warn!(target: "signin", reason = %why.why(), "directory not usable");
                        let _ = ldap.unbind().await;
                        return why.answer();
                    }
                };
                match ldap.with_timeout(TIMEOUT).simple_bind(&dn, password).await {
                    Ok(r) if r.rc == 0 => Answer::Yes(found.name, Said { admin, display_name: found.display_name }),
                    // 49: invalidCredentials.
                    Ok(r) if r.rc == 49 => Answer::No,
                    _ => Answer::Unreachable,
                }
            }
        };
        let _ = ldap.unbind().await;
        answer
    };
    // Three exchanges at most, each with its own timeout; this bounds the whole.
    tokio::time::timeout(TIMEOUT * 4, exchange).await.unwrap_or(Answer::Unreachable)
}

fn hash(password: &str) -> Result<String> {
    use argon2::password_hash::{PasswordHasher, SaltString};
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| anyhow::anyhow!("random: {e}"))?;
    let salt = SaltString::encode_b64(&salt).map_err(|e| anyhow::anyhow!("salt: {e}"))?;
    let params = argon2::Params::new(ARGON_MEMORY_KIB, ARGON_PASSES, 1, None).map_err(|e| anyhow::anyhow!("argon2: {e}"))?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    Ok(argon.hash_password(password.as_bytes(), &salt).map_err(|e| anyhow::anyhow!("argon2: {e}"))?.to_string())
}

fn verify(hash: &str, password: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    PasswordHash::new(hash).is_ok_and(|h| argon2::Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
}

/// Written whole (a temporary file renamed over it), readable by its owner only.
fn write_private(path: &Path, text: &str) -> Result<()> {
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
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))?;
    Ok(())
}

// --- Settings, Authentication ---

/// Read-only: the page shows the directory settings, never sets them (see `Directory::open`).
pub fn router(serve: Arc<Serve>) -> Router {
    Router::new().route("/api/auth/ldap", get(settings)).layer(axum::middleware::from_fn(super::access::admin)).with_state(serve)
}

/// The settings, and whether the service account's password file can be read (never its contents).
fn view(config: &Config) -> serde_json::Value {
    let readable = !config.bind_password_file.is_empty()
        && std::fs::read_to_string(&config.bind_password_file).is_ok_and(|p| !p.trim_end_matches(['\n', '\r']).is_empty());
    json!({ "config": config, "bind_password": if config.bind_password_file.is_empty() { "not set" } else if readable { "set" } else { "unreadable" } })
}

async fn settings(State(serve): State<Arc<Serve>>) -> Response {
    axum::Json(view(&serve.directory.config())).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_escape_what_rfc_4515_says() {
        assert_eq!(escape(r"a*b(c)d\e"), r"a\2ab\28c\29d\5ce");
        assert_eq!(escape("nul\0"), r"nul\00");
        assert_eq!(escape("José"), "José");
        // A name that tries to widen the search finds only itself.
        assert_eq!(filter("uid", "*)(uid=*", ""), r"(|(uid=\2a\29\28uid=\2a)(mail=\2a\29\28uid=\2a))");
        assert_eq!(
            filter("uid", "robin", "cn=music,ou=groups,dc=example,dc=com"),
            "(&(|(uid=robin)(mail=robin))(memberOf=cn=music,ou=groups,dc=example,dc=com))"
        );
        // The address signs in as well; an attribute that is the address already is asked once.
        assert_eq!(filter("uid", "elva@example.com", ""), "(|(uid=elva@example.com)(mail=elva@example.com))");
        assert_eq!(filter("mail", "elva@example.com", ""), "(mail=elva@example.com)");
    }

    #[test]
    fn an_empty_password_or_a_name_with_control_characters_is_never_put_to_the_directory() {
        assert!(!acceptable("robin", ""), "an empty password binds anonymously");
        assert!(!acceptable("", "secret"));
        assert!(!acceptable("osk\nar", "secret"));
        assert!(!acceptable("robin\0", "secret"));
        assert!(!acceptable(&"x".repeat(300), "secret"));
        assert!(acceptable("robin", "secret"));
    }

    #[test]
    fn the_display_name_is_display_name_else_cn_else_none() {
        let attrs = |pairs: &[(&str, &str)]| -> HashMap<String, Vec<String>> { pairs.iter().map(|(k, v)| (k.to_string(), vec![v.to_string()])).collect() };
        assert_eq!(display_name(&attrs(&[("uid", "elva"), ("displayName", " Elva Smith "), ("cn", "elva")])).as_deref(), Some("Elva Smith"));
        assert_eq!(display_name(&attrs(&[("uid", "elva"), ("displayname", "Elva")])).as_deref(), Some("Elva"), "whatever the attribute's case");
        assert_eq!(display_name(&attrs(&[("uid", "elva"), ("displayName", "  "), ("CN", "Elva S.")])).as_deref(), Some("Elva S."), "an empty one: the common name");
        assert_eq!(display_name(&attrs(&[("uid", "elva"), ("cn", "")])), None);
        assert_eq!(display_name(&attrs(&[("uid", "elva")])), None);
        assert_eq!(display_name(&HashMap::from([("displayName".to_string(), Vec::new())])), None, "an attribute without a value");
        assert_eq!(wanted("uid"), ["uid", "displayName", "cn"]);
    }

    fn config() -> Config {
        Config {
            enabled: true,
            url: "ldap://lldap:3890".into(),
            base_dn: "ou=people,dc=example,dc=com".into(),
            bind_dn: "uid=svc,ou=people,dc=example,dc=com".into(),
            bind_password_file: "/data/ldap-bind".into(),
            ..Config::default()
        }
    }

    #[test]
    fn plain_ldap_only_to_this_machines_networks() {
        assert!(config().check().is_ok(), "a container's name");
        for url in ["ldap://localhost:389", "ldap://192.168.1.5:389", "ldap://10.0.0.2", "ldap://[::1]:389"] {
            assert!(Config { url: url.into(), ..config() }.check().is_ok(), "{url}");
        }
        assert!(Config { url: "ldap://ldap.example.com:389".into(), ..config() }.check().is_err());
        assert!(Config { url: "ldap://8.8.8.8".into(), ..config() }.check().is_err());
        assert!(Config { url: "ldap://ldap.example.com:389".into(), starttls: true, ..config() }.check().is_ok());
        assert!(Config { url: "ldaps://ldap.example.com:636".into(), ..config() }.check().is_ok());
        assert!(Config { url: "http://x".into(), ..config() }.check().is_err());
        assert!(Config { bind_password_file: String::new(), ..config() }.check().is_err());
        assert!(Config { user_attribute: "uid)(x".into(), ..config() }.check().is_err());
        assert!(Config { url: "nonsense".into(), enabled: false, ..config() }.check().is_ok(), "switched off, anything is kept");
    }

    #[test]
    fn a_kept_hash_checks_the_password_and_nothing_else() {
        let h = hash("correct horse").unwrap();
        assert!(h.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"), "{h}");
        assert!(verify(&h, "correct horse"));
        assert!(!verify(&h, "correct hors"));
        assert!(!verify("not a hash", "correct horse"));
    }

    #[tokio::test]
    async fn while_the_directory_cannot_be_reached_a_recent_sign_in_still_works_for_30_days() {
        let d = std::env::temp_dir().join(format!("cratarr-ldap-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let pw = d.join("bind");
        std::fs::write(&pw, "x").unwrap();
        let dir = Directory::open(Some(&d)).unwrap();
        // Nothing listens on port 1: the directory cannot be reached.
        dir.set_config(Config { url: "ldap://127.0.0.1:1".into(), bind_password_file: pw.display().to_string(), ..config() }).unwrap();
        let exists = |a: &str, _: Option<Said>| (a == "robin").then(|| a.to_string());
        assert_eq!(dir.sign_in("robin", "secret", &exists, 1_000).await, None, "nothing kept yet");
        // As if the directory had accepted it at time 1000.
        dir.remember("robin", "secret", 1_000).await;
        assert_eq!(dir.sign_in("robin", "secret", &exists, 2_000).await.as_deref(), Some("robin"));
        assert_eq!(dir.sign_in("robin", "wrong", &exists, 2_000).await, None);
        assert_eq!(dir.sign_in("robin", "", &exists, 2_000).await, None, "an empty password is refused first");
        // The hash is kept in its file, owner-only, never the password.
        let kept = std::fs::read_to_string(d.join("ldap-verifiers.json")).unwrap();
        assert!(kept.contains("$argon2id$") && !kept.contains("secret"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(d.join("ldap-verifiers.json")).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // Read back after a restart.
        let again = Directory::open(Some(&d)).unwrap();
        assert_eq!(again.sign_in("robin", "secret", &exists, 3_000).await.as_deref(), Some("robin"));
        // A mapped name signs in as its account; an account that does not exist is no sign-in.
        let mut mapped = again.config();
        mapped.accounts.insert("robin".into(), "owner".into());
        again.set_config(mapped).unwrap();
        assert_eq!(again.sign_in("robin", "secret", &|a: &str, _: Option<Said>| (a == "owner").then(|| a.to_string()), 3_000).await.as_deref(), Some("owner"));
        assert_eq!(again.sign_in("robin", "secret", &|_: &str, _: Option<Said>| None, 3_000).await, None);
        // A refusal for the name in another case deletes it (directories match names regardless of case), and the
        // name typed in another case finds the directory's spelling for the mapping.
        assert_eq!(again.sign_in("ROBIN", "secret", &|a: &str, _: Option<Said>| (a == "owner").then(|| a.to_string()), 3_000).await.as_deref(), Some("owner"));
        again.forget("ROBIN");
        assert_eq!(again.sign_in("robin", "secret", &|a: &str, _: Option<Said>| (a == "owner").then(|| a.to_string()), 3_000).await, None, "gone after a refusal");
        again.remember("robin", "secret", 1_000).await;
        // A misconfiguration (the service account's password file unreadable) fails closed: no kept hash stands in,
        // and none is deleted.
        let mut broken = again.config();
        broken.bind_password_file = d.join("missing").display().to_string();
        again.set_config(broken.clone()).unwrap();
        assert_eq!(again.sign_in("robin", "secret", &|a: &str, _: Option<Said>| (a == "owner").then(|| a.to_string()), 3_000).await, None, "fails closed");
        broken.bind_password_file = pw.display().to_string();
        again.set_config(broken).unwrap();
        assert_eq!(again.sign_in("robin", "secret", &|a: &str, _: Option<Said>| (a == "owner").then(|| a.to_string()), 3_000).await.as_deref(), Some("owner"), "kept, not deleted");
        // After 30 days it is gone.
        assert_eq!(again.sign_in("robin", "secret", &|a: &str, _: Option<Said>| (a == "owner").then(|| a.to_string()), 1_000 + KEEP_SECS + 1).await, None);
        assert_eq!(again.sign_in("robin", "secret", &|a: &str, _: Option<Said>| (a == "owner").then(|| a.to_string()), 3_000).await, None, "and stays gone");
        // Switched off, the directory is not asked at all.
        let mut off = again.config();
        off.enabled = false;
        again.set_config(off).unwrap();
        assert_eq!(again.sign_in("robin", "secret", &|a: &str, _: Option<Said>| Some(a.to_string()), 3_000).await, None);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_service_accounts_password_is_never_shown() {
        let d = std::env::temp_dir().join(format!("cratarr-ldap-view-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let pw = d.join("bind");
        std::fs::write(&pw, "s3cret-bind\n").unwrap();
        let v = view(&Config { bind_password_file: pw.display().to_string(), ..config() }).to_string();
        assert!(!v.contains("s3cret-bind") && v.contains("\"bind_password\":\"set\""));
        assert!(view(&Config { bind_password_file: "/nowhere".into(), ..config() }).to_string().contains("unreadable"));
        std::fs::remove_dir_all(&d).ok();
    }
}
