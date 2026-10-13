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

/// Where a playlist is kept in step (decision 0014).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A copy in the Music folder, imported by the media server for everyone.
    Shared,
    /// A user's own playlist on a server.
    User { source: String, user: String },
}

impl Kind {
    /// "shared", or "user/<source>/<user>": how the page names it.
    pub fn key(&self) -> String {
        match self {
            Kind::Shared => "shared".into(),
            Kind::User { source, user } => format!("user/{source}/{user}"),
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        if key == "shared" {
            return Some(Kind::Shared);
        }
        let (source, user) = key.strip_prefix("user/")?.split_once('/')?;
        (!source.is_empty() && !user.is_empty() && !user.contains('/'))
            .then(|| Kind::User { source: source.into(), user: user.into() })
    }

    fn columns(&self) -> (&str, &str, &str) {
        match self {
            Kind::Shared => ("shared", "", ""),
            Kind::User { source, user } => ("user", source, user),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// The playlist wins: changes on the target are overwritten.
    Push,
    TwoWay,
}

/// Two-way: which side leads when both changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Conflict {
    Newest,
    Playlist,
    Target,
}

/// How a link's last pass went.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct LinkStatus {
    /// Epoch seconds.
    pub at: i64,
    /// Why it stopped, if it did.
    pub error: Option<String>,
}

/// A link as the page sees it.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct LinkView {
    /// "shared", or "user/<source>/<user>".
    pub target: String,
    pub label: String,
    pub direction: Direction,
    pub conflict: Conflict,
    pub status: Option<LinkStatus>,
}

/// A sync link: a playlist and one target it is kept in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub playlist: String,
    pub kind: Kind,
    pub direction: Direction,
    pub conflict: Conflict,
    /// Shared: the name its copy was last written under (a rename leaves the old one to remove).
    pub copy: Option<String>,
}

// temp_store: the image has no /tmp.
const SCHEMA: &str = "
PRAGMA temp_store = MEMORY;
PRAGMA journal_mode = WAL;
CREATE TABLE IF NOT EXISTS links (
  source TEXT NOT NULL, user TEXT NOT NULL, name TEXT NOT NULL, remote TEXT NOT NULL, remote_name TEXT NOT NULL,
  stamp TEXT, base TEXT NOT NULL,
  PRIMARY KEY (source, user, name)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS targets (
  playlist TEXT NOT NULL, kind TEXT NOT NULL, source TEXT NOT NULL, user TEXT NOT NULL,
  direction TEXT NOT NULL, conflict TEXT NOT NULL, copy TEXT,
  PRIMARY KEY (playlist, kind, source, user)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
";

fn enum_text<T: serde::Serialize>(v: T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

fn enum_of<T: serde::de::DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(text.into())).ok()
}

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
        let db = self.db();
        db.execute("UPDATE OR REPLACE targets SET playlist = ?2 WHERE playlist = ?1", params![from, to])?;
        Ok(db.execute("UPDATE links SET name = ?2 WHERE name = ?1", params![from, to])?)
    }

    /// Every sync link (decision 0014), by playlist.
    pub fn targets(&self) -> Result<Vec<Target>> {
        let db = self.db();
        let mut st = db.prepare("SELECT playlist, kind, source, user, direction, conflict, copy FROM targets ORDER BY playlist, kind, source, user")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (playlist, kind, source, user, direction, conflict, copy) = row?;
            let kind = match kind.as_str() {
                "shared" => Kind::Shared,
                "user" => Kind::User { source, user },
                _ => continue,
            };
            let (Some(direction), Some(conflict)) = (enum_of(&direction), enum_of(&conflict)) else { continue };
            out.push(Target { playlist, kind, direction, conflict, copy });
        }
        Ok(out)
    }

    pub fn put_target(&self, t: &Target) -> Result<()> {
        let (kind, source, user) = t.kind.columns();
        self.db().execute(
            "INSERT OR REPLACE INTO targets (playlist, kind, source, user, direction, conflict, copy) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![t.playlist, kind, source, user, enum_text(t.direction), enum_text(t.conflict), t.copy],
        )?;
        Ok(())
    }

    pub fn remove_target(&self, playlist: &str, kind: &Kind) -> Result<()> {
        let (k, source, user) = kind.columns();
        self.db().execute(
            "DELETE FROM targets WHERE playlist = ?1 AND kind = ?2 AND source = ?3 AND user = ?4",
            params![playlist, k, source, user],
        )?;
        Ok(())
    }

    pub fn meta(&self, key: &str) -> Option<String> {
        use rusqlite::OptionalExtension;
        self.db().query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional().ok().flatten()
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)", params![key, value])?;
        Ok(())
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

    #[test]
    fn targets_are_kept_per_playlist_and_follow_a_rename() {
        let s = SyncState::open(None).unwrap();
        let user = Kind::User { source: "jellyfin".into(), user: "u1".into() };
        let t = |kind: Kind, direction| Target { playlist: "Chill".into(), kind, direction, conflict: Conflict::Newest, copy: None };
        s.put_target(&t(Kind::Shared, Direction::Push)).unwrap();
        s.put_target(&t(user.clone(), Direction::TwoWay)).unwrap();
        s.put_target(&t(user.clone(), Direction::Push)).unwrap();
        assert_eq!(s.targets().unwrap(), vec![t(Kind::Shared, Direction::Push), t(user.clone(), Direction::Push)], "one link per target");
        s.rename("Chill", "Calm").unwrap();
        assert!(s.targets().unwrap().iter().all(|t| t.playlist == "Calm"));
        s.remove_target("Calm", &user).unwrap();
        assert_eq!(s.targets().unwrap().len(), 1);
        assert_eq!(Kind::from_key(&user.key()), Some(user));
        assert_eq!(Kind::from_key("shared"), Some(Kind::Shared));
        assert_eq!(Kind::from_key("user/x"), None);
        assert_eq!(s.meta("migrated"), None);
        s.set_meta("migrated", "1").unwrap();
        assert_eq!(s.meta("migrated").as_deref(), Some("1"));
    }
}
