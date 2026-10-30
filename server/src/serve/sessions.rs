//! Signed-in devices (decision 0039): each sign-in on Cratarr's sign-in page is a session, which the browser keeps as a
//! cookie. As OWASP's Session Management Cheat Sheet advises: the token is 256 bits from the operating system's random
//! source, and only its SHA-256 is stored, so a copy of the database signs nobody in; a session ends after [IDLE]
//! without use and [LIFETIME] after its sign-in, at sign-out, when it is signed out from Settings → Authentication, and
//! when its account's password changes. Kept apart from the user data (which goes into backups), in an owner-only file.

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

/// A session not used for this long has ended.
pub const IDLE: i64 = 90 * 24 * 3600;
/// Nor does one last longer than this after its sign-in (browsers keep a cookie at most 400 days).
pub const LIFETIME: i64 = 400 * 24 * 3600;
/// When a session was last used is written at most this often.
const TOUCH_EVERY: i64 = 3600;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Session {
    /// Names the session in Settings (never the token).
    pub id: String,
    pub user: String,
    /// What signed in, as its browser told ("Firefox on Windows", "Cratarr app on Android").
    pub device: String,
    pub created: i64,
    pub last_used: i64,
}

pub struct Sessions {
    db: Mutex<Connection>,
    /// The requirement (decision 0043) as kept, read at every request: kept here too.
    required: std::sync::RwLock<Option<bool>>,
}

/// `bytes` from the operating system's random source, in hex.
pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("the operating system's random source");
    hex(&buf)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

const COLUMNS: &str = "id, user, device, created, last_used";

fn session(r: &rusqlite::Row) -> rusqlite::Result<Session> {
    Ok(Session { id: r.get(0)?, user: r.get(1)?, device: r.get(2)?, created: r.get(3)?, last_used: r.get(4)? })
}

impl Sessions {
    /// The sessions kept in `path` (`None`: in memory only).
    pub fn open(path: Option<&Path>) -> Result<Self> {
        let db = crate::db::open(path)?;
        #[cfg(unix)]
        if let Some(p) = path {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600)).with_context(|| format!("cannot make {} the owner's only", p.display()))?;
        }
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                token_hash TEXT NOT NULL UNIQUE,
                user TEXT NOT NULL,
                device TEXT NOT NULL,
                created INTEGER NOT NULL,
                last_used INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS api_keys (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                key_hash TEXT NOT NULL UNIQUE,
                created INTEGER NOT NULL,
                last_used INTEGER
            );
            CREATE TABLE IF NOT EXISTS settings (
                name TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
        )?;
        // Keys made before they had an owner and a scope (decision 0051): the owner's, controlling, as they were.
        for (column, ddl) in [
            ("owner", "ALTER TABLE api_keys ADD COLUMN owner TEXT"),
            ("scope", "ALTER TABLE api_keys ADD COLUMN scope TEXT NOT NULL DEFAULT 'control'"),
            // What a key is for (the owner, 2026-10-24: keys live with what uses them, not in Authentication); the key
            // named Home Assistant before it had one is Home Assistant's.
            ("purpose", "ALTER TABLE api_keys ADD COLUMN purpose TEXT; UPDATE api_keys SET purpose = 'home_assistant' WHERE name = 'Home Assistant'"),
        ] {
            let has: bool = db.prepare("SELECT 1 FROM pragma_table_info('api_keys') WHERE name = ?1")?.exists(params![column])?;
            if !has {
                db.execute_batch(ddl)?;
            }
        }
        let required = db
            .query_row("SELECT value FROM settings WHERE name = 'sign_in_required'", [], |r| r.get::<_, String>(0))
            .optional()?
            .map(|v| v == "1");
        Ok(Self { db: Mutex::new(db), required: std::sync::RwLock::new(required) })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn prune(db: &Connection, now: i64) -> Result<()> {
        db.execute("DELETE FROM sessions WHERE last_used < ?1 OR created < ?2", params![now - IDLE, now - LIFETIME])?;
        Ok(())
    }

    /// A new session for `user`: its token (for the cookie, never kept) and the session.
    pub fn create(&self, user: &str, device: &str, now: i64) -> Result<(String, Session)> {
        let token = random_hex(32);
        let s = Session { id: random_hex(16), user: user.to_string(), device: device.to_string(), created: now, last_used: now };
        let db = self.db();
        Self::prune(&db, now)?;
        db.execute(
            "INSERT INTO sessions (id, token_hash, user, device, created, last_used) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![s.id, hash(&token), s.user, s.device, now],
        )?;
        Ok((token, s))
    }

    /// The session a token belongs to, while it lasts; its use is noted.
    pub fn check(&self, token: &str, now: i64) -> Option<Session> {
        let db = self.db();
        let found = db
            .query_row(&format!("SELECT {COLUMNS} FROM sessions WHERE token_hash = ?1"), params![hash(token)], session)
            .optional()
            .unwrap_or_else(|e| {
                tracing::warn!("sessions: cannot read: {e}");
                None
            })?;
        if now - found.last_used > IDLE || now - found.created > LIFETIME {
            let _ = db.execute("DELETE FROM sessions WHERE id = ?1", params![found.id]);
            return None;
        }
        if now - found.last_used >= TOUCH_EVERY {
            let _ = db.execute("UPDATE sessions SET last_used = ?2 WHERE id = ?1", params![found.id, now]);
        }
        Some(found)
    }

    /// The sessions that last, the last used first.
    pub fn list(&self, now: i64) -> Result<Vec<Session>> {
        let db = self.db();
        Self::prune(&db, now)?;
        let mut q = db.prepare(&format!("SELECT {COLUMNS} FROM sessions ORDER BY last_used DESC, created DESC"))?;
        Ok(q.query_map([], session)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Ends the session with this id (Settings → Authentication); whether there was one.
    pub fn revoke(&self, id: &str) -> Result<bool> {
        Ok(self.db().execute("DELETE FROM sessions WHERE id = ?1", params![id])? > 0)
    }

    /// Ends the session a token belongs to (signing out).
    pub fn end(&self, token: &str) -> Result<()> {
        self.db().execute("DELETE FROM sessions WHERE token_hash = ?1", params![hash(token)])?;
        Ok(())
    }

    /// Ends every session of `user` but `keep` (its password changed, from that session).
    pub fn revoke_user(&self, user: &str, keep: Option<&str>) -> Result<usize> {
        Ok(self.db().execute("DELETE FROM sessions WHERE user = ?1 AND id IS NOT ?2", params![user, keep])?)
    }

    /// A new API key named `name` (decision 0043) for `owner`, reading or controlling (`scope`, decision 0051), for
    /// what uses it (`purpose`: `home_assistant`): the key (shown once, never kept) and its entry.
    pub fn new_key(&self, name: &str, owner: Option<&str>, scope: &str, purpose: Option<&str>, now: i64) -> Result<(String, ApiKey)> {
        let key = format!("{KEY_PREFIX}{}", random_hex(32));
        let scope = if scope == "read" { "read" } else { "control" };
        let k = ApiKey {
            id: random_hex(8),
            name: name.trim().to_string(),
            created: now,
            last_used: None,
            owner: owner.map(str::to_string),
            scope: scope.into(),
            purpose: purpose.map(str::to_string),
        };
        self.db().execute(
            "INSERT INTO api_keys (id, name, key_hash, created, owner, scope, purpose) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![k.id, k.name, hash(&key), now, k.owner, k.scope, k.purpose],
        )?;
        Ok((key, k))
    }

    /// Whether the keys for `purpose` are switched off (Connections, the owner 2026-10-24): kept, but refused.
    pub fn purpose_off(&self, purpose: &str) -> bool {
        self.db()
            .query_row("SELECT 1 FROM settings WHERE name = ?1", params![format!("off:{purpose}")], |_| Ok(()))
            .optional()
            .ok()
            .flatten()
            .is_some()
    }

    /// The keys for `purpose` switched on or off.
    pub fn set_purpose_on(&self, purpose: &str, on: bool) -> Result<()> {
        let name = format!("off:{purpose}");
        if on {
            self.db().execute("DELETE FROM settings WHERE name = ?1", params![name])?;
        } else {
            self.db().execute("INSERT OR REPLACE INTO settings (name, value) VALUES (?1, '1')", params![name])?;
        }
        Ok(())
    }

    /// The API key `key` is, while it is not revoked and what it is for is not switched off; its use is noted.
    pub fn check_key(&self, key: &str, now: i64) -> Option<ApiKey> {
        if !key.starts_with(KEY_PREFIX) {
            return None;
        }
        let db = self.db();
        let found = db
            .query_row(&format!("SELECT {KEY_COLUMNS} FROM api_keys WHERE key_hash = ?1"), params![hash(key)], api_key)
            .optional()
            .unwrap_or_else(|e| {
                tracing::warn!("sessions: cannot read the keys: {e}");
                None
            })?;
        drop(db);
        if found.purpose.as_deref().is_some_and(|p| self.purpose_off(p)) {
            return None;
        }
        let db = self.db();
        if found.last_used.is_none_or(|t| now - t >= TOUCH_EVERY) {
            let _ = db.execute("UPDATE api_keys SET last_used = ?2 WHERE id = ?1", params![found.id, now]);
        }
        Some(found)
    }

    /// The API keys, the newest first.
    pub fn keys(&self) -> Result<Vec<ApiKey>> {
        let db = self.db();
        let mut q = db.prepare(&format!("SELECT {KEY_COLUMNS} FROM api_keys ORDER BY created DESC"))?;
        Ok(q.query_map([], api_key)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Revokes the API key with this id; whether there was one.
    pub fn revoke_key(&self, id: &str) -> Result<bool> {
        Ok(self.db().execute("DELETE FROM api_keys WHERE id = ?1", params![id])? > 0)
    }

    fn setting(&self, name: &str) -> Option<String> {
        self.db().query_row("SELECT value FROM settings WHERE name = ?1", params![name], |r| r.get(0)).optional().ok().flatten()
    }

    fn set_setting(&self, name: &str, value: &str) -> Result<()> {
        self.db().execute("INSERT INTO settings (name, value) VALUES (?1, ?2) ON CONFLICT(name) DO UPDATE SET value = ?2", params![name, value])?;
        Ok(())
    }

    /// Whether a sign-in is required on every request (decision 0043); `None` before it was ever set.
    pub fn required(&self) -> Option<bool> {
        *self.required.read().unwrap_or_else(|p| p.into_inner())
    }

    pub fn set_required(&self, on: bool) -> Result<()> {
        self.set_setting("sign_in_required", if on { "1" } else { "0" })?;
        *self.required.write().unwrap_or_else(|p| p.into_inner()) = Some(on);
        Ok(())
    }

    /// The secret signed links are signed with (decision 0043): made once, from the operating system's random source.
    pub fn link_secret(&self) -> Result<Vec<u8>> {
        if let Some(s) = self.setting("link_secret") {
            return Ok(s.into_bytes());
        }
        let s = random_hex(32);
        self.set_setting("link_secret", &s)?;
        Ok(s.into_bytes())
    }

    /// This install's id, made once (no secret: the health check shows it to anyone): the app learns it at the public
    /// address and binds its bridge to the local one only when that answers with it too.
    pub fn instance_id(&self) -> Result<String> {
        if let Some(s) = self.setting("instance_id") {
            return Ok(s);
        }
        let s = random_hex(16);
        self.set_setting("instance_id", &s)?;
        Ok(s)
    }
}

/// Every API key starts with this, so one is known for what it is wherever it turns up.
pub const KEY_PREFIX: &str = "cratarr_";

/// An API key's entry (never the key).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ApiKey {
    pub id: String,
    /// What it is for ("Home Assistant").
    pub name: String,
    pub created: i64,
    pub last_used: Option<i64>,
    /// The account it acts for (decision 0051); `None`: the owner (a key made before keys had one, as Home
    /// Assistant's).
    pub owner: Option<String>,
    /// `read`, or `control` (reading and the players).
    pub scope: String,
    /// What uses it: `home_assistant`, shown with its connection; none for a key made before keys had one.
    pub purpose: Option<String>,
}

const KEY_COLUMNS: &str = "id, name, created, last_used, owner, scope, purpose";

fn api_key(r: &rusqlite::Row) -> rusqlite::Result<ApiKey> {
    Ok(ApiKey { id: r.get(0)?, name: r.get(1)?, created: r.get(2)?, last_used: r.get(3)?, owner: r.get(4)?, scope: r.get(5)?, purpose: r.get(6)? })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn a_session_is_its_token_and_only_the_tokens_hash_is_kept() {
        let s = Sessions::open(None).unwrap();
        let (token, made) = s.create("me", "Firefox on Windows", NOW).unwrap();
        assert_eq!(token.len(), 64, "256 bits");
        assert_eq!(s.check(&token, NOW + 5), Some(made.clone()));
        assert_eq!(s.check("0".repeat(64).as_str(), NOW), None);
        let kept: String = s.db().query_row("SELECT token_hash FROM sessions", [], |r| r.get(0)).unwrap();
        assert_ne!(kept, token);
        assert_eq!(kept, hash(&token));
        let (other, _) = s.create("me", "Cratarr app on Android", NOW).unwrap();
        assert_ne!(other, token, "a new token each sign-in");
    }

    #[test]
    fn a_session_ends_unused_after_idle_and_at_most_after_its_lifetime() {
        let s = Sessions::open(None).unwrap();
        let (token, _) = s.create("me", "d", NOW).unwrap();
        assert!(s.check(&token, NOW + IDLE).is_some(), "used just in time");
        assert!(s.check(&token, NOW + 2 * IDLE - 1).is_some(), "each use counts");
        assert!(s.check(&token, NOW + 3 * IDLE + 1).is_none(), "then unused too long");
        assert!(s.check(&token, NOW + 2 * IDLE - 1).is_none(), "and gone");
        let (token, _) = s.create("me", "d", NOW).unwrap();
        let mut t = NOW;
        while t + IDLE / 2 <= NOW + LIFETIME {
            t += IDLE / 2;
            assert!(s.check(&token, t).is_some());
        }
        assert!(s.check(&token, NOW + LIFETIME + 1).is_none(), "used all along, still not past its lifetime");
    }

    #[test]
    fn sessions_end_one_by_one_by_sign_out_and_by_account() {
        let s = Sessions::open(None).unwrap();
        let (a, sa) = s.create("me", "a", NOW).unwrap();
        let (b, sb) = s.create("me", "b", NOW + 1).unwrap();
        let (c, _) = s.create("you", "c", NOW + 2).unwrap();
        assert_eq!(s.list(NOW + 3).unwrap().iter().map(|x| x.device.as_str()).collect::<Vec<_>>(), ["c", "b", "a"]);
        assert!(s.revoke(&sb.id).unwrap());
        assert!(!s.revoke(&sb.id).unwrap());
        assert!(s.check(&b, NOW + 4).is_none());
        s.end(&c).unwrap();
        assert!(s.check(&c, NOW + 4).is_none());
        let (d, _) = s.create("me", "d", NOW + 5).unwrap();
        assert_eq!(s.revoke_user("me", Some(&sa.id)).unwrap(), 1, "all of the account's but the one kept");
        assert!(s.check(&a, NOW + 6).is_some());
        assert!(s.check(&d, NOW + 6).is_none());
        assert_eq!(s.revoke_user("me", None).unwrap(), 1);
        assert!(s.list(NOW + 7).unwrap().is_empty());
    }

    #[test]
    fn an_api_key_is_kept_as_its_hash_used_and_revoked_alone() {
        let s = Sessions::open(None).unwrap();
        let (key, made) = s.new_key(" Home Assistant ", None, "control", Some("home_assistant"), NOW).unwrap();
        assert!(key.starts_with(KEY_PREFIX) && key.len() == KEY_PREFIX.len() + 64);
        assert_eq!(made.name, "Home Assistant");
        let (other, _) = s.new_key("scripts", None, "control", None, NOW).unwrap();
        assert_eq!(s.check_key(&key, NOW + 5).map(|k| k.id), Some(made.id.clone()));
        assert_eq!(s.keys().unwrap().iter().find(|k| k.id == made.id).unwrap().last_used, Some(NOW + 5));
        assert!(s.check_key("cratarr_nothing", NOW).is_none());
        assert!(s.check_key(&key[KEY_PREFIX.len()..], NOW).is_none(), "the bare hex is no key");
        let kept: Vec<String> = s.db().prepare("SELECT key_hash FROM api_keys").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert!(!kept.contains(&key) && kept.contains(&hash(&key)));
        assert!(s.revoke_key(&made.id).unwrap());
        assert!(s.check_key(&key, NOW + 6).is_none());
        assert!(s.check_key(&other, NOW + 6).is_some(), "the other key still works");
    }

    #[test]
    fn the_requirement_and_the_link_secret_are_kept() {
        let s = Sessions::open(None).unwrap();
        assert_eq!(s.required(), None);
        s.set_required(true).unwrap();
        assert_eq!(s.required(), Some(true));
        s.set_required(false).unwrap();
        assert_eq!(s.required(), Some(false));
        let secret = s.link_secret().unwrap();
        assert_eq!(secret.len(), 64);
        assert_eq!(s.link_secret().unwrap(), secret, "made once");
    }

    #[test]
    fn the_instance_id_is_made_once_and_kept() {
        let s = Sessions::open(None).unwrap();
        let id = s.instance_id().unwrap();
        assert_eq!(id.len(), 32);
        assert_eq!(s.instance_id().unwrap(), id);
    }

    #[test]
    fn sessions_last_across_a_restart() {
        let dir = std::env::temp_dir().join(format!("cratarr-sessions-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sessions.sqlite");
        let (token, made) = Sessions::open(Some(&path)).unwrap().create("me", "d", NOW).unwrap();
        assert_eq!(Sessions::open(Some(&path)).unwrap().check(&token, NOW + 1), Some(made));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
