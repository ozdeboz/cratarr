//! `spotify.sqlite` (decision 0073): each account's own app and sign-in, its lists as each way brought them, and what
//! MusicBrainz answered about Spotify's ids (kept for everyone: an ISRC is the same recording whoever saved it).

use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::Lists;

const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS accounts (account TEXT PRIMARY KEY, client_id TEXT, refresh TEXT, user TEXT, name TEXT,
  since INTEGER, sync_at INTEGER, sync_error TEXT);
CREATE TABLE IF NOT EXISTS lists (account TEXT NOT NULL, way TEXT NOT NULL, at INTEGER NOT NULL, file TEXT,
  body TEXT NOT NULL, PRIMARY KEY (account, way));
CREATE TABLE IF NOT EXISTS mb (key TEXT PRIMARY KEY, value TEXT NOT NULL, at INTEGER NOT NULL);
";

/// The lists' two ways in.
pub const LIVE: &str = "live";
pub const IMPORT: &str = "import";

/// An account's connection as kept.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Account {
    pub client_id: Option<String>,
    pub refresh: Option<String>,
    pub user: Option<String>,
    pub name: Option<String>,
    pub since: Option<i64>,
    pub sync_at: Option<i64>,
    pub sync_error: Option<String>,
}

pub struct Store(Mutex<Connection>);

impl Store {
    pub fn open(path: Option<&Path>) -> Result<Self> {
        let conn = match path {
            Some(p) => Connection::open(p)?,
            None => Connection::open_in_memory()?,
        };
        conn.execute_batch(SCHEMA)?;
        Ok(Self(Mutex::new(conn)))
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn account(&self, account: &str) -> Result<Account> {
        Ok(self
            .db()
            .query_row("SELECT client_id, refresh, user, name, since, sync_at, sync_error FROM accounts WHERE account = ?1", [account], |r| {
                Ok(Account { client_id: r.get(0)?, refresh: r.get(1)?, user: r.get(2)?, name: r.get(3)?, since: r.get(4)?, sync_at: r.get(5)?, sync_error: r.get(6)? })
            })
            .optional()?
            .unwrap_or_default())
    }

    /// The accounts signed in to Spotify (with a sign-in kept), and when their lists were last read.
    pub fn connected(&self) -> Result<Vec<(String, Option<i64>)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT account, sync_at FROM accounts WHERE refresh IS NOT NULL AND client_id IS NOT NULL")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The account's own app: a new one forgets the sign-in and the live lists (they were another app's).
    pub fn set_client(&self, account: &str, client_id: Option<&str>) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        tx.execute(
            "INSERT INTO accounts (account, client_id) VALUES (?1, ?2)
             ON CONFLICT (account) DO UPDATE SET client_id = ?2, refresh = NULL, user = NULL, name = NULL, since = NULL,
               sync_at = NULL, sync_error = NULL",
            params![account, client_id],
        )?;
        tx.execute("DELETE FROM lists WHERE account = ?1 AND way = ?2", params![account, LIVE])?;
        tx.commit()?;
        Ok(())
    }

    pub fn signed_in(&self, account: &str, refresh: &str, user: &str, name: &str, at: i64) -> Result<()> {
        self.db().execute(
            "UPDATE accounts SET refresh = ?2, user = ?3, name = ?4, since = ?5, sync_error = NULL WHERE account = ?1",
            params![account, refresh, user, name, at],
        )?;
        Ok(())
    }

    /// Spotify handed out a new refresh token with a new access token: the old one may stop working.
    pub fn refreshed(&self, account: &str, refresh: &str) -> Result<()> {
        self.db().execute("UPDATE accounts SET refresh = ?2 WHERE account = ?1", params![account, refresh])?;
        Ok(())
    }

    /// Disconnected: the sign-in and the live lists forgotten, the app's id kept.
    pub fn disconnect(&self, account: &str) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        tx.execute(
            "UPDATE accounts SET refresh = NULL, user = NULL, name = NULL, since = NULL, sync_at = NULL, sync_error = NULL WHERE account = ?1",
            [account],
        )?;
        tx.execute("DELETE FROM lists WHERE account = ?1 AND way = ?2", params![account, LIVE])?;
        tx.commit()?;
        Ok(())
    }

    /// A read of the live lists ended: when, and what went wrong (none: it went well).
    pub fn synced(&self, account: &str, at: i64, error: Option<&str>) -> Result<()> {
        self.db().execute("UPDATE accounts SET sync_at = ?2, sync_error = ?3 WHERE account = ?1", params![account, at, error])?;
        Ok(())
    }

    /// The lists one way brought, replacing what it brought before.
    pub fn put_lists(&self, account: &str, way: &str, at: i64, file: Option<&str>, lists: &Lists) -> Result<()> {
        self.db().execute(
            "INSERT OR REPLACE INTO lists (account, way, at, file, body) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![account, way, at, file, serde_json::to_string(lists)?],
        )?;
        Ok(())
    }

    pub fn forget_lists(&self, account: &str, way: &str) -> Result<bool> {
        Ok(self.db().execute("DELETE FROM lists WHERE account = ?1 AND way = ?2", params![account, way])? > 0)
    }

    /// When a way's lists came, and from what file.
    pub fn lists_at(&self, account: &str, way: &str) -> Result<Option<(i64, Option<String>)>> {
        Ok(self.db().query_row("SELECT at, file FROM lists WHERE account = ?1 AND way = ?2", params![account, way], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
    }

    pub fn lists(&self, account: &str, way: &str) -> Result<Option<Lists>> {
        let body: Option<String> = self.db().query_row("SELECT body FROM lists WHERE account = ?1 AND way = ?2", params![account, way], |r| r.get(0)).optional()?;
        Ok(body.map(|b| serde_json::from_str(&b)).transpose()?)
    }

    /// Every account's lists, either way: what MusicBrainz is asked about.
    pub fn all_lists(&self) -> Result<Vec<Lists>> {
        let db = self.db();
        let mut st = db.prepare("SELECT body FROM lists")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|b| b.ok().and_then(|b| serde_json::from_str(&b).ok())).collect())
    }

    /// A MusicBrainz answer kept (`isrc:…`, `upc:…`, `name:…`), as JSON; "null" for none found.
    pub fn put_mb(&self, key: &str, value: &str, at: i64) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO mb (key, value, at) VALUES (?1, ?2, ?3)", params![key, value, at])?;
        Ok(())
    }

    /// Every kept answer, for matching many items at once.
    pub fn all_mb(&self) -> Result<std::collections::HashMap<String, String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT key, value FROM mb")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// An account removed (0051): its Spotify forgotten.
    #[allow(dead_code)]
    pub fn forget_account(&self, account: &str) -> Result<()> {
        let db = self.db();
        db.execute("DELETE FROM accounts WHERE account = ?1", [account])?;
        db.execute("DELETE FROM lists WHERE account = ?1", [account])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_app_forgets_the_sign_in_and_the_live_lists_but_not_the_export() {
        let s = Store::open(None).unwrap();
        s.set_client("me", Some("abc")).unwrap();
        s.signed_in("me", "r1", "u", "U", 10).unwrap();
        s.put_lists("me", LIVE, 11, None, &Lists::default()).unwrap();
        s.put_lists("me", IMPORT, 12, Some("my_spotify_data.zip"), &Lists::default()).unwrap();
        assert_eq!(s.connected().unwrap(), vec![("me".to_string(), None)]);
        s.set_client("me", Some("def")).unwrap();
        assert_eq!(s.account("me").unwrap().refresh, None);
        assert!(s.lists("me", LIVE).unwrap().is_none());
        assert_eq!(s.lists_at("me", IMPORT).unwrap(), Some((12, Some("my_spotify_data.zip".into()))));
        s.signed_in("me", "r2", "u", "U", 13).unwrap();
        s.disconnect("me").unwrap();
        assert_eq!(s.account("me").unwrap().client_id.as_deref(), Some("def"));
        assert!(s.connected().unwrap().is_empty());
    }
}
