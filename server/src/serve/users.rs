//! Who may use the OpenSubsonic API, and checking a request's login (decision 0030). The classic Subsonic login sends
//! either the password or `md5(password + salt)` with the salt, so the server must know each password itself: logins
//! are kept as Cratarr keeps its other secrets, in an owner-only file in the data folder (`subsonic-users.json`, written
//! atomically, never sent to a page), not in the user data database that goes into backups.
//!
//! The login check follows nghe's `auth` module (https://github.com/vnghia/nghe, AGPL-3.0, Copyright vnghia):
//! token (`t`, `s`), password (`p`, plain or `enc:` hex), each with the user `u`. Changed: Subsonic's error codes are
//! answered in the envelope with HTTP 200 (see `api`), and API keys are not offered yet.
//!
//! Cratarr's own accounts, made and kept in the Users panel beside the directory's (decision 0051, amended
//! 2026-10-19), are translated from nghe's user routes (same project and licence: `nghe/src/route/user/{setup,create,
//! update_role,update_password,delete}.rs`), with Navidrome's rules as the checklist. Changed from nghe: a name is taken
//! whatever its case and by either kind of account; a removed account is kept, inactive (its data stays), not deleted;
//! no one changes their own role or removes themselves; the last active administrator stays one.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::{Context, Result};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub name: String,
    /// The OpenSubsonic password; empty: none yet (an account from the directory, before its user sets one).
    pub password: String,
    #[serde(default)]
    pub admin: bool,
    /// Whether it may sign in at all (decision 0051): one who left the directory's group is kept, not deleted.
    #[serde(default = "yes")]
    pub active: bool,
    /// Made from the directory (decision 0051), which says whether it is an administrator and whether it stays.
    #[serde(default)]
    pub directory: bool,
    #[serde(default)]
    pub permissions: Permissions,
    /// The name the person goes by, from the directory (its `displayName`, else `cn`), which the page greets them with;
    /// `None` for an account the directory never signed in, or one it gives no such name.
    #[serde(default)]
    pub display_name: Option<String>,
}

fn yes() -> bool {
    true
}

/// What a user may do beyond playing (decision 0051, the Users panel): keep tracks to play offline, make playlists,
/// share them with the others, and make public share links (OpenSubsonic's `createShare`); ask for albums, have that
/// asking approved at once, and answer everyone's requests (decision 0070).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Permissions {
    pub offline: bool,
    pub playlists: bool,
    pub share: bool,
    pub links: bool,
    pub request: bool,
    pub auto_approve: bool,
    pub manage_requests: bool,
}

impl Default for Permissions {
    fn default() -> Self {
        Self { offline: true, playlists: true, share: true, links: false, request: true, auto_approve: false, manage_requests: false }
    }
}

impl Permissions {
    /// An administrator's: everything.
    pub fn all() -> Self {
        Self { offline: true, playlists: true, share: true, links: true, request: true, auto_approve: true, manage_requests: true }
    }
}

/// Why a login failed, as Subsonic's error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// 10: no user, or neither password nor token.
    Missing,
    /// 40: wrong user or password.
    Wrong,
}

impl Refused {
    pub fn code(self) -> u32 {
        match self {
            Refused::Missing => 10,
            Refused::Wrong => 40,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Refused::Missing => "Required parameter is missing: u, and p or t and s",
            Refused::Wrong => "Wrong username or password",
        }
    }
}

impl User {
    /// A new, active account of this name: no password yet, not an administrator, the usual permissions.
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_string(),
            password: String::new(),
            admin: false,
            active: true,
            directory: false,
            permissions: Permissions::default(),
            display_name: None,
        }
    }
}

/// A display name as it is kept: trimmed, and none when that leaves nothing.
fn tidy(display_name: Option<&str>) -> Option<String> {
    display_name.map(str::trim).filter(|d| !d.is_empty()).map(str::to_string)
}

pub struct Users {
    file: Option<PathBuf>,
    users: RwLock<Vec<User>>,
}

/// Why an account could not be changed, in one short sentence: a bad request, a conflict with what is, no such
/// account, or the change could not be saved.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    Bad(&'static str),
    Conflict(&'static str),
    Missing,
    Unsaved,
}

impl Refusal {
    pub fn message(&self) -> &'static str {
        match self {
            Refusal::Bad(m) | Refusal::Conflict(m) => *m,
            Refusal::Missing => "There is no such account.",
            Refusal::Unsaved => "The accounts could not be saved.",
        }
    }
}

/// A name an account may have: not empty, at most 64 characters, no `/` nor control characters, no spaces around it,
/// and not `.` or `..` (an account's playlists are a folder of its name).
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= 64
        && name.trim() == name
        && !name.chars().any(|c| c == '/' || c == '\\' || c.is_control())
        && name != "."
        && name != ".."
}

/// Cratarr's own account `name` (not the directory's, whose role and password the directory keeps).
fn own_account(users: &[User], name: &str) -> Result<User, Refusal> {
    let u = users.iter().find(|u| u.name == name).ok_or(Refusal::Missing)?;
    if u.directory {
        return Err(Refusal::Conflict("Set in the directory."));
    }
    Ok(u.clone())
}

/// Whether an active administrator other than `name` is left.
fn other_admin(users: &[User], name: &str) -> bool {
    users.iter().any(|u| u.admin && u.active && u.name != name)
}

fn token(password: &str, salt: &str) -> String {
    Md5::digest(format!("{password}{salt}").as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// A `p` value as the password: plain, or `enc:` and its UTF-8 bytes in hex.
fn plain_password(p: &str) -> Option<String> {
    let Some(hex) = p.strip_prefix("enc:") else { return Some(p.to_string()) };
    if hex.len() % 2 != 0 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect();
    String::from_utf8(bytes?).ok()
}

/// Constant-time comparison, so a guess learns nothing from how long the answer took.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl Users {
    /// Reads the logins from the data folder (`None`: in memory only).
    pub fn open(data_dir: Option<&Path>) -> Result<Self> {
        let file = data_dir.map(|d| d.join("subsonic-users.json"));
        let users = match &file {
            Some(f) => match std::fs::read_to_string(f) {
                Ok(t) => serde_json::from_str(&t).with_context(|| format!("{} is not a list of users", f.display()))?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                Err(e) => return Err(e).with_context(|| format!("cannot read {}", f.display())),
            },
            None => Vec::new(),
        };
        Ok(Self { file, users: RwLock::new(users) })
    }

    /// Adds a user or sets their password. The first user is an administrator.
    pub fn set(&self, name: &str, password: &str) -> Result<()> {
        let mut users = self.users.write().unwrap_or_else(|p| p.into_inner());
        match users.iter_mut().find(|u| u.name == name) {
            Some(u) => u.password = password.to_string(),
            None => {
                let admin = users.is_empty();
                users.push(User { password: password.to_string(), admin, ..User::named(name) });
            }
        }
        self.save(&users)
    }

    /// The first administrator's name: whose data Cratarr's own calls act on.
    pub fn owner(&self) -> Option<String> {
        let users = self.users.read().unwrap_or_else(|p| p.into_inner());
        users.iter().find(|u| u.admin).or_else(|| users.first()).map(|u| u.name.clone())
    }

    /// The accounts' names, the administrator first.
    pub fn names(&self) -> Vec<String> {
        let users = self.users.read().unwrap_or_else(|p| p.into_inner());
        let mut names: Vec<(bool, String)> = users.iter().map(|u| (!u.admin, u.name.clone())).collect();
        names.sort();
        names.into_iter().map(|(_, n)| n).collect()
    }

    /// An account by its name.
    pub fn get(&self, name: &str) -> Option<User> {
        self.users.read().unwrap_or_else(|p| p.into_inner()).iter().find(|u| u.name == name).cloned()
    }

    /// Every account, the administrators first.
    pub fn all(&self) -> Vec<User> {
        let mut users = self.users.read().unwrap_or_else(|p| p.into_inner()).clone();
        users.sort_by(|a, b| (!a.admin, &a.name).cmp(&(!b.admin, &b.name)));
        users
    }

    /// The account the directory signs `name` in as (decision 0051): made at its first sign-in, active again if it had
    /// left, an administrator as the directory says, with the display name it gives. Cratarr's own accounts (not from
    /// the directory) are left as they are but for that name: one of the same name is that account. Written only when
    /// something changed.
    pub fn from_directory(&self, name: &str, admin: bool, display_name: Option<&str>) -> Result<User> {
        let display_name = tidy(display_name);
        let mut users = self.users.write().unwrap_or_else(|p| p.into_inner());
        let before = users.clone();
        match users.iter_mut().find(|u| u.name == name) {
            Some(u) if !u.directory => {
                if u.display_name == display_name {
                    return Ok(u.clone());
                }
                u.display_name = display_name;
            }
            Some(u) => {
                if (u.admin, u.active, &u.display_name) == (admin, true, &display_name) {
                    return Ok(u.clone());
                }
                u.admin = admin;
                u.active = true;
                u.display_name = display_name;
            }
            None => users.push(User { admin, directory: true, display_name, ..User::named(name) }),
        }
        if let Err(e) = self.save(&users) {
            *users = before;
            return Err(e);
        }
        Ok(users.iter().find(|u| u.name == name).cloned().expect("just kept"))
    }

    /// The display name the directory gives an account now (the hourly check): kept, and the file written, only when it
    /// changed. Returns whether it did.
    pub fn set_display_name(&self, name: &str, display_name: Option<&str>) -> Result<bool> {
        let display_name = tidy(display_name);
        let mut users = self.users.write().unwrap_or_else(|p| p.into_inner());
        let Some(u) = users.iter_mut().find(|u| u.name == name) else { return Ok(false) };
        if u.display_name == display_name {
            return Ok(false);
        }
        let old = std::mem::replace(&mut u.display_name, display_name);
        let saved = self.save(&users);
        if saved.is_err() {
            if let Some(u) = users.iter_mut().find(|u| u.name == name) {
                u.display_name = old;
            }
        }
        saved.map(|()| true)
    }

    /// An account from the directory that left its group (decision 0051): it may no longer sign in, its data kept.
    /// Returns whether it was active.
    pub fn deactivate(&self, name: &str) -> Result<bool> {
        let mut users = self.users.write().unwrap_or_else(|p| p.into_inner());
        let Some(u) = users.iter_mut().find(|u| u.name == name && u.directory && u.active) else { return Ok(false) };
        u.active = false;
        let saved = self.save(&users);
        if saved.is_err() {
            if let Some(u) = users.iter_mut().find(|u| u.name == name) {
                u.active = true;
            }
        }
        saved.map(|()| true)
    }

    /// A user's permissions, set in the Users panel.
    pub fn set_permissions(&self, name: &str, permissions: Permissions) -> Result<bool> {
        let mut users = self.users.write().unwrap_or_else(|p| p.into_inner());
        let Some(u) = users.iter_mut().find(|u| u.name == name) else { return Ok(false) };
        let old = std::mem::replace(&mut u.permissions, permissions);
        let saved = self.save(&users);
        if saved.is_err() {
            if let Some(u) = users.iter_mut().find(|u| u.name == name) {
                u.permissions = old;
            }
        }
        saved.map(|()| true)
    }

    /// A user's own OpenSubsonic password, set by themselves (an account from the directory has none until then).
    pub fn set_app_password(&self, name: &str, password: &str) -> Result<(), &'static str> {
        if password.is_empty() {
            return Err("Choose a password.");
        }
        let mut users = self.users.write().unwrap_or_else(|p| p.into_inner());
        let Some(u) = users.iter_mut().find(|u| u.name == name) else { return Err("There is no such account.") };
        let old = std::mem::replace(&mut u.password, password.to_string());
        if let Err(e) = self.save(&users) {
            tracing::warn!("subsonic: could not save the app password: {e:#}");
            if let Some(u) = users.iter_mut().find(|u| u.name == name) {
                u.password = old;
            }
            return Err("The password could not be saved.");
        }
        Ok(())
    }

    /// The first account, from the Settings page (as Navidrome's first-run "create admin"): refused once any exists.
    pub fn create_first(&self, name: &str, password: &str) -> Result<(), &'static str> {
        let name = name.trim();
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            return Err("Choose a username without spaces.");
        }
        if password.is_empty() {
            return Err("Choose a password.");
        }
        let mut users = self.users.write().unwrap_or_else(|p| p.into_inner());
        if !users.is_empty() {
            return Err("There is an account already.");
        }
        users.push(User { password: password.to_string(), admin: true, ..User::named(name) });
        self.save(&users).map_err(|e| {
            tracing::warn!("subsonic: could not save the account: {e:#}");
            users.clear();
            "The account could not be saved."
        })
    }

    /// A new password for an account, given its current one.
    pub fn change_password(&self, name: &str, current: &str, new: &str) -> Result<(), &'static str> {
        if new.is_empty() {
            return Err("Choose a password.");
        }
        let mut users = self.users.write().unwrap_or_else(|p| p.into_inner());
        let Some(u) = users.iter_mut().find(|u| u.name == name) else { return Err("There is no such account.") };
        if !same(&u.password, current) {
            return Err("The current password is wrong.");
        }
        let old = std::mem::replace(&mut u.password, new.to_string());
        let saved = self.save(&users);
        if let Err(e) = saved {
            tracing::warn!("subsonic: could not save the new password: {e:#}");
            if let Some(u) = users.iter_mut().find(|u| u.name == name) {
                u.password = old;
            }
            return Err("The password could not be saved.");
        }
        Ok(())
    }

    /// A change to the accounts, kept: undone when it cannot be saved.
    fn change<T>(&self, f: impl FnOnce(&mut Vec<User>) -> Result<T, Refusal>) -> Result<T, Refusal> {
        let mut users = self.users.write().unwrap_or_else(|p| p.into_inner());
        let before = users.clone();
        let out = f(&mut users)?;
        if let Err(e) = self.save(&users) {
            tracing::warn!("subsonic: could not save the accounts: {e:#}");
            *users = before;
            return Err(Refusal::Unsaved);
        }
        Ok(out)
    }

    /// A new account of Cratarr's own (nghe's `create`; its `setup` while there is none: then an administrator, whatever
    /// is asked). Its name is free of either kind of account, whatever its case.
    pub fn create(&self, name: &str, password: &str, admin: bool) -> Result<User, Refusal> {
        if !valid_name(name) {
            return Err(Refusal::Bad("Choose a name of at most 64 characters, without / and without spaces around it."));
        }
        if password.is_empty() {
            return Err(Refusal::Bad("Choose a password."));
        }
        self.change(|users| {
            if users.iter().any(|u| u.name.to_lowercase() == name.to_lowercase()) {
                return Err(Refusal::Conflict("That name is taken."));
            }
            let user = User { password: password.to_string(), admin: admin || users.is_empty(), ..User::named(name) };
            users.push(user.clone());
            Ok(user)
        })
    }

    /// An administrator or not (nghe's `update_role`): Cratarr's own accounts only (the directory says it for its own),
    /// never one's own (`by`), and the last active administrator stays one.
    pub fn set_admin(&self, name: &str, admin: bool, by: Option<&str>) -> Result<(), Refusal> {
        self.change(|users| {
            let u = own_account(users, name)?;
            if by == Some(name) {
                return Err(Refusal::Conflict("You cannot change your own role."));
            }
            if u.admin && !admin && u.active && !other_admin(users, name) {
                return Err(Refusal::Conflict("Cratarr needs an administrator: make another one first."));
            }
            if let Some(u) = users.iter_mut().find(|u| u.name == name) {
                u.admin = admin;
            }
            Ok(())
        })
    }

    /// A Cratarr account's password, set by an administrator (nghe's `update_password` with an id): no current one asked.
    /// It is also the account's OpenSubsonic password.
    pub fn set_password(&self, name: &str, password: &str) -> Result<(), Refusal> {
        if password.is_empty() {
            return Err(Refusal::Bad("Choose a password."));
        }
        self.change(|users| {
            own_account(users, name)?;
            if let Some(u) = users.iter_mut().find(|u| u.name == name) {
                u.password = password.to_string();
            }
            Ok(())
        })
    }

    /// A Cratarr account removed (`false`; nghe's `delete`, but kept, inactive: its data stays, as when a directory
    /// account leaves its group) or restored. Never one's own (`by`), nor the last active administrator. Returns whether
    /// it changed.
    pub fn set_active(&self, name: &str, active: bool, by: Option<&str>) -> Result<bool, Refusal> {
        self.change(|users| {
            let u = own_account(users, name)?;
            if u.active == active {
                return Ok(false);
            }
            if by == Some(name) {
                return Err(Refusal::Conflict("You cannot remove yourself."));
            }
            if !active && u.admin && !other_admin(users, name) {
                return Err(Refusal::Conflict("Cratarr needs an administrator: make another one first."));
            }
            if let Some(u) = users.iter_mut().find(|u| u.name == name) {
                u.active = active;
            }
            Ok(true)
        })
    }

    fn save(&self, users: &[User]) -> Result<()> {
        let Some(path) = &self.file else { return Ok(()) };
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
        f.write_all(serde_json::to_string_pretty(users)?.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))?;
        Ok(())
    }

    /// The account a user name and password sign in as on Cratarr's sign-in page (decision 0039): the password exactly
    /// as typed. The same work whether or not the account exists, and the passwords compared as digests, so how long it
    /// takes tells neither.
    pub fn sign_in(&self, name: &str, password: &str) -> Option<User> {
        use sha2::Sha256;
        let users = self.users.read().unwrap_or_else(|p| p.into_inner());
        let known = users.iter().find(|x| x.name == name);
        let kept = Sha256::digest(known.map_or("", |x| x.password.as_str()).as_bytes());
        let typed = Sha256::digest(password.as_bytes());
        let ok = kept.iter().zip(typed.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0;
        known.filter(|u| ok && !u.password.is_empty() && u.active).cloned()
    }

    /// The user a request logs in as: `u` with `t` and `s`, or with `p`.
    pub fn check(&self, u: Option<&str>, t: Option<&str>, s: Option<&str>, p: Option<&str>) -> Result<User, Refused> {
        let u = u.filter(|u| !u.is_empty()).ok_or(Refused::Missing)?;
        let users = self.users.read().unwrap_or_else(|p| p.into_inner());
        // The same work whether or not the user exists.
        let known = users.iter().find(|x| x.name == u);
        let password = known.map_or("", |x| x.password.as_str());
        let ok = match (t, s, p) {
            (Some(t), Some(s), _) if !t.is_empty() && !s.is_empty() => same(&token(password, s), &t.to_ascii_lowercase()),
            (_, _, Some(p)) => plain_password(p).is_some_and(|p| same(&p, password)),
            _ => return Err(Refused::Missing),
        };
        match known {
            Some(user) if ok && !password.is_empty() && user.active => Ok(user.clone()),
            _ => Err(Refused::Wrong),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_and_password_logins() {
        let users = Users::open(None).unwrap();
        users.set("sesame-user", "sesame").unwrap();
        // The Subsonic API documentation's own example: password "sesame", salt "c19b2d".
        let t = "26719a1196d2a940705a59634eb18eab";
        assert_eq!(users.check(Some("sesame-user"), Some(t), Some("c19b2d"), None).unwrap().name, "sesame-user");
        assert!(users.check(Some("sesame-user"), None, None, None).is_err_and(|e| e == Refused::Missing));
        assert_eq!(users.check(Some("sesame-user"), Some(t), Some("other"), None), Err(Refused::Wrong));
        assert!(users.check(Some("sesame-user"), None, None, Some("sesame")).is_ok());
        assert!(users.check(Some("sesame-user"), None, None, Some("enc:736573616d65")).is_ok(), "hex-encoded");
        assert_eq!(users.check(Some("nobody"), None, None, Some("sesame")), Err(Refused::Wrong));
        assert!(users.check(Some("sesame-user"), None, None, None).is_err());
        assert!(users.check(None, Some(t), Some("c19b2d"), None).is_err_and(|e| e == Refused::Missing));
    }

    #[test]
    fn the_sign_in_page_takes_the_password_exactly_as_typed() {
        let users = Users::open(None).unwrap();
        assert!(users.sign_in("me", "").is_none(), "no account yet");
        users.set("me", "enc:41").unwrap();
        assert_eq!(users.sign_in("me", "enc:41").unwrap().name, "me");
        assert!(users.sign_in("me", "A").is_none(), "not read as a Subsonic `enc:` password");
        assert!(users.sign_in("me", "enc:4").is_none());
        assert!(users.sign_in("Me", "enc:41").is_none());
        assert!(users.sign_in("nobody", "").is_none());
    }

    #[test]
    fn the_first_account_from_the_page_and_only_the_first() {
        let users = Users::open(None).unwrap();
        assert_eq!(users.create_first("two words", "pw"), Err("Choose a username without spaces."));
        assert_eq!(users.create_first("me", ""), Err("Choose a password."));
        assert!(users.create_first(" me ", "pw").is_ok());
        assert_eq!(users.names(), ["me"]);
        assert_eq!(users.create_first("intruder", "pw"), Err("There is an account already."));
        assert_eq!(users.change_password("me", "wrong", "new"), Err("The current password is wrong."));
        assert!(users.change_password("me", "pw", "new").is_ok());
        assert!(users.check(Some("me"), None, None, Some("new")).unwrap().admin);
    }

    #[test]
    fn the_first_user_is_the_administrator_and_logins_persist() {
        let dir = std::env::temp_dir().join(format!("cratarr-users-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let users = Users::open(Some(&dir)).unwrap();
        users.set("me", "one").unwrap();
        users.set("you", "two").unwrap();
        users.set("me", "three").unwrap();
        let again = Users::open(Some(&dir)).unwrap();
        assert!(again.check(Some("me"), None, None, Some("three")).unwrap().admin);
        assert!(!again.check(Some("you"), None, None, Some("two")).unwrap().admin);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_users_file_from_before_display_names_still_reads() {
        let dir = std::env::temp_dir().join(format!("cratarr-users-old-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("subsonic-users.json"), r#"[{ "name": "me", "password": "pw", "admin": true }, { "name": "dee", "password": "", "directory": true }]"#).unwrap();
        let users = Users::open(Some(&dir)).unwrap();
        assert!(users.all().iter().all(|u| u.display_name.is_none() && u.active));
        assert!(users.check(Some("me"), None, None, Some("pw")).unwrap().admin);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_directorys_display_name_is_kept_and_written_only_when_it_changed() {
        let dir = std::env::temp_dir().join(format!("cratarr-users-dname-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("subsonic-users.json");
        let users = Users::open(Some(&dir)).unwrap();
        users.set("me", "pw").unwrap();
        assert_eq!(users.get("me").unwrap().display_name, None, "Cratarr's own account: none");
        assert_eq!(users.from_directory("dee", false, Some("  Dee Example ")).unwrap().display_name.as_deref(), Some("Dee Example"));
        assert!(std::fs::read_to_string(&file).unwrap().contains("\"display_name\": \"Dee Example\""));

        // The same again (trimmed alike) writes nothing: the file, taken away, stays away.
        std::fs::remove_file(&file).unwrap();
        users.from_directory("dee", false, Some("Dee Example")).unwrap();
        assert!(!users.set_display_name("dee", Some(" Dee Example")).unwrap());
        assert!(!file.exists(), "unchanged: not written");

        // A new one is kept and written; an empty one is none.
        assert!(users.set_display_name("dee", Some("Dee E.")).unwrap());
        assert!(file.exists());
        assert_eq!(Users::open(Some(&dir)).unwrap().get("dee").unwrap().display_name.as_deref(), Some("Dee E."));
        assert!(users.set_display_name("dee", Some("  ")).unwrap());
        assert_eq!(users.get("dee").unwrap().display_name, None);
        assert!(!users.set_display_name("nobody", Some("x")).unwrap());

        // Cratarr's own account the directory signs in as takes its display name, nothing else.
        assert_eq!(users.from_directory("me", false, Some("Me Myself")).unwrap().display_name.as_deref(), Some("Me Myself"));
        let me = users.get("me").unwrap();
        assert!(me.admin && !me.directory);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cratarr_accounts_are_made_and_kept_beside_the_directorys() {
        let users = Users::open(None).unwrap();
        // The first is an administrator whatever is asked (nghe's setup); the name rules; a name taken whatever its case.
        assert!(users.create("ann", "pw", false).unwrap().admin);
        for bad in ["", " bob", "bob ", "a/b", "..", &"x".repeat(65)] {
            assert!(matches!(users.create(bad, "pw", false), Err(Refusal::Bad(_))), "{bad:?}");
        }
        assert!(matches!(users.create("bob", "", false), Err(Refusal::Bad(_))));
        users.from_directory("dee", false, None).unwrap();
        assert_eq!(users.create("ANN", "pw", false), Err(Refusal::Conflict("That name is taken.")));
        assert_eq!(users.create("Dee", "pw", false), Err(Refusal::Conflict("That name is taken.")), "a directory account's too");
        let bob = users.create("Bob Smith", "pw", false).unwrap();
        assert!(!bob.admin && !bob.directory && bob.active);
        assert!(users.sign_in("Bob Smith", "pw").is_some());

        // Roles: not the directory's, not one's own, and the last active administrator stays one.
        assert_eq!(users.set_admin("dee", true, Some("ann")), Err(Refusal::Conflict("Set in the directory.")));
        assert_eq!(users.set_admin("ann", false, Some("ann")), Err(Refusal::Conflict("You cannot change your own role.")));
        assert!(matches!(users.set_admin("ann", false, Some("Bob Smith")), Err(Refusal::Conflict(_))), "the last administrator");
        users.set_admin("Bob Smith", true, Some("ann")).unwrap();
        users.set_admin("ann", false, Some("Bob Smith")).unwrap();
        assert!(!users.get("ann").unwrap().admin);
        assert_eq!(users.set_admin("nobody", true, None), Err(Refusal::Missing));

        // A password set by an administrator, no current one asked; not a directory account's.
        users.set_password("ann", "new").unwrap();
        assert!(users.sign_in("ann", "new").is_some() && users.sign_in("ann", "pw").is_none());
        assert_eq!(users.set_password("dee", "x"), Err(Refusal::Conflict("Set in the directory.")));

        // Removed: kept, inactive, no sign-in; not oneself, nor the last active administrator; restored.
        assert_eq!(users.set_active("ann", false, Some("ann")), Err(Refusal::Conflict("You cannot remove yourself.")));
        assert!(matches!(users.set_active("Bob Smith", false, Some("ann")), Err(Refusal::Conflict(_))), "the last administrator");
        assert_eq!(users.set_active("ann", false, Some("Bob Smith")), Ok(true));
        assert!(users.sign_in("ann", "new").is_none() && users.get("ann").is_some_and(|u| !u.active));
        assert_eq!(users.set_active("ann", false, Some("Bob Smith")), Ok(false), "already removed");
        assert_eq!(users.set_active("ann", true, Some("Bob Smith")), Ok(true));
        assert!(users.sign_in("ann", "new").is_some());
        assert_eq!(users.set_active("dee", false, Some("ann")), Err(Refusal::Conflict("Set in the directory.")));
    }
}
