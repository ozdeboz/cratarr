//! What the playlist sync remembers (decision 0012): for each playlist, which server playlist is its copy, that
//! copy's change stamp when last seen, and the list both copies last agreed on (Cratarr track ids, in order). Not
//! derived: losing it means the next pass treats every pair as new. A small SQLite file in the data folder (in memory
//! without one).

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The playlist's name here (its `.m3u` file without the extension).
    pub name: String,
    /// The server's id of its copy.
    pub remote: String,
    /// The copy's name on the server when last agreed. Differs from `name` while a rename made here is not yet on
    /// the server; the server's current name differing from it means the server renamed it.
    pub remote_name: String,
    pub stamp: Option<String>,
    /// What both copies last agreed on.
    pub base: Vec<i64>,
}

// temp_store: the image has no /tmp.
const SCHEMA: &str = "
PRAGMA temp_store = MEMORY;
PRAGMA journal_mode = WAL;
CREATE TABLE IF NOT EXISTS links (
  source TEXT NOT NULL, user TEXT NOT NULL, name TEXT NOT NULL, remote TEXT NOT NULL, remote_name TEXT NOT NULL,
  stamp TEXT, base TEXT NOT NULL,
  PRIMARY KEY (source, user, name)) WITHOUT ROWID;
";

pub struct SyncState {
    db: Mutex<Connection>,
}

impl SyncState {
    pub fn open(file: Option<&Path>) -> Result<Self> {
        let conn = match file {
            Some(f) => Connection::open(f).with_context(|| format!("opening {}", f.display()))?,
            None => Connection::open_in_memory()?,
        };
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.execute_batch(SCHEMA).context("creating the playlist sync state")?;
        Ok(Self { db: Mutex::new(conn) })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Every link for one server and user.
    pub fn links(&self, source: &str, user: &str) -> Result<Vec<Link>> {
        let db = self.db();
        let mut st =
            db.prepare("SELECT name, remote, remote_name, stamp, base FROM links WHERE source = ?1 AND user = ?2 ORDER BY name")?;
        let rows = st.query_map(params![source, user], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, String>(4)?))
        })?;
        rows.map(|r| {
            let (name, remote, remote_name, stamp, base) = r?;
            Ok(Link { name, remote, remote_name, stamp, base: serde_json::from_str(&base)? })
        })
        .collect()
    }

    pub fn put(&self, source: &str, user: &str, link: &Link) -> Result<()> {
        self.db().execute(
            "INSERT OR REPLACE INTO links (source, user, name, remote, remote_name, stamp, base) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![source, user, link.name, link.remote, link.remote_name, link.stamp, serde_json::to_string(&link.base)?],
        )?;
        Ok(())
    }

    pub fn remove(&self, source: &str, user: &str, name: &str) -> Result<()> {
        self.db().execute("DELETE FROM links WHERE source = ?1 AND user = ?2 AND name = ?3", params![source, user, name])?;
        Ok(())
    }

    /// The name here changes (a rename made in Cratarr, recorded before the file is renamed; or one the server made,
    /// once the file followed). The server copy stays the same.
    pub fn rename(&self, from: &str, to: &str) -> Result<usize> {
        Ok(self.db().execute("UPDATE links SET name = ?2 WHERE name = ?1", params![from, to])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_kept_per_server_and_user() {
        let s = SyncState::open(None).unwrap();
        let l = Link { name: "Chill".into(), remote: "r1".into(), remote_name: "Chill".into(), stamp: Some("e1".into()), base: vec![3, 1, 2] };
        s.put("jellyfin", "u1", &l).unwrap();
        assert_eq!(s.links("jellyfin", "u1").unwrap(), vec![l.clone()]);
        assert!(s.links("jellyfin", "u2").unwrap().is_empty());
        assert_eq!(s.rename("Chill", "Calm").unwrap(), 1);
        assert_eq!(s.links("jellyfin", "u1").unwrap()[0].name, "Calm");
        s.remove("jellyfin", "u1", "Calm").unwrap();
        assert!(s.links("jellyfin", "u1").unwrap().is_empty());
    }
}
