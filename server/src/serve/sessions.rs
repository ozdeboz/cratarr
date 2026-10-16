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
        let db = match path {
            Some(p) => Connection::open(p).with_context(|| format!("cannot open {}", p.display()))?,
            None => Connection::open_in_memory()?,
        };
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
            )",
        )?;
        Ok(Self { db: Mutex::new(db) })
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
