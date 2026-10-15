//! Who may use the OpenSubsonic API, and checking a request's login (decision 0030). The classic Subsonic login sends
//! either the password or `md5(password + salt)` with the salt, so the server must know each password itself: logins
//! are kept as Cratarr keeps its other secrets, in an owner-only file in the data folder (`subsonic-users.json`, written
//! atomically, never sent to a page), not in the user data database that goes into backups.
//!
//! The login check follows nghe's `auth` module (https://github.com/vnghia/nghe, AGPL-3.0, Copyright vnghia):
//! token (`t`, `s`), password (`p`, plain or `enc:` hex), each with the user `u`. Changed: Subsonic's error codes are
//! answered in the envelope with HTTP 200 (see `api`), and API keys are not offered yet.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::{Context, Result};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub name: String,
    pub password: String,
    #[serde(default)]
    pub admin: bool,
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

pub struct Users {
    file: Option<PathBuf>,
    users: RwLock<Vec<User>>,
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
                users.push(User { name: name.to_string(), password: password.to_string(), admin });
            }
        }
        self.save(&users)
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
            Some(user) if ok && !password.is_empty() => Ok(user.clone()),
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
}
