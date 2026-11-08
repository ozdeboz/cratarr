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
    /// The server's item of each track in `base`, by position ("" when it had none, or `base` is from before this was
    /// kept): what an agreed track is on the server while its file is away and the catalogue no longer links it.
    pub ids: Vec<String>,
}

impl Link {
    /// A link with nothing agreed yet.
    pub fn new(name: &str, remote: &str, remote_name: &str) -> Self {
        Link { name: name.into(), remote: remote.into(), remote_name: remote_name.into(), stamp: None, base: Vec::new(), ids: Vec::new() }
    }
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
    pub status: Option<LinkStatus>,
    /// The account's copy exists (false while it waits to be made, a few per pass). Always true for "shared".
    pub made: bool,
}

/// A sync link: a playlist and one target it is kept in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub playlist: String,
    pub kind: Kind,
    /// Shared: the name its copy was last written under (a rename leaves the old one to remove).
    pub copy: Option<String>,
}

/// The `direction` and `conflict` columns: every link is two-way, and when both sides changed the order or the name
/// in one pass the server's leads (decision 0014, amended 2026-10-15). What older files say there is not read; the
/// columns stay so they open unchanged.
const TWO_WAY: &str = "two_way";
const SERVER_LEADS: &str = "newest";

/// The meta key of the catalogue generation the agreed lists were made with (`follow_catalogue`).
const CATALOGUE: &str = "catalogue";

// journal_size_limit: as the catalogue's. (Temporary storage in memory: crate::db.)
const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA journal_size_limit = 8388608;
CREATE TABLE IF NOT EXISTS links (
  source TEXT NOT NULL, user TEXT NOT NULL, name TEXT NOT NULL, remote TEXT NOT NULL, remote_name TEXT NOT NULL,
  stamp TEXT, base TEXT NOT NULL,
  PRIMARY KEY (source, user, name)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS targets (
  playlist TEXT NOT NULL, kind TEXT NOT NULL, source TEXT NOT NULL, user TEXT NOT NULL,
  direction TEXT NOT NULL, conflict TEXT NOT NULL, copy TEXT,
  PRIMARY KEY (playlist, kind, source, user)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS retired (
  playlist TEXT NOT NULL, kind TEXT NOT NULL, source TEXT NOT NULL, user TEXT NOT NULL,
  direction TEXT NOT NULL, conflict TEXT NOT NULL,
  PRIMARY KEY (playlist, kind, source, user)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS spoken (playlist TEXT NOT NULL, name TEXT NOT NULL, PRIMARY KEY (playlist, name)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS left_by (playlist TEXT NOT NULL, source TEXT NOT NULL, user TEXT NOT NULL,
  PRIMARY KEY (playlist, source, user)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS retire (playlist TEXT PRIMARY KEY, copy TEXT NOT NULL) WITHOUT ROWID;
";

/// Recorded once the playlists were made native (decision 0014, amended 2026-10-15): Shared copies in Music are
/// retired, and every playlist is its owner's only.
const NATIVE: &str = "native";

/// Columns added after the first schema; added to an existing file when missing.
const ADDED_COLUMNS: [(&str, &str); 1] = [("links", "base_ids TEXT NOT NULL DEFAULT '[]'")];

pub struct SyncState {
    db: Mutex<Connection>,
}

impl SyncState {
    pub fn open(file: Option<&Path>) -> Result<Self> {
        let conn = crate::db::open(file)?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.execute_batch(SCHEMA).context("creating the playlist sync state")?;
        for (table, column) in ADDED_COLUMNS {
            let name = column.split_whitespace().next().unwrap_or_default();
            let has: bool = conn.query_row(&format!("SELECT COUNT(*) > 0 FROM pragma_table_info('{table}') WHERE name = ?1"), [name], |r| r.get(0))?;
            if !has {
                conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column}"))?;
            }
        }
        Ok(Self { db: Mutex::new(conn) })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Every link for one server and user.
    pub fn links(&self, source: &str, user: &str) -> Result<Vec<Link>> {
        let db = self.db();
        let mut st =
            db.prepare("SELECT name, remote, remote_name, stamp, base, base_ids FROM links WHERE source = ?1 AND user = ?2 ORDER BY name")?;
        let rows = st.query_map(params![source, user], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        rows.map(|r| {
            let (name, remote, remote_name, stamp, base, ids) = r?;
            Ok(Link { name, remote, remote_name, stamp, base: serde_json::from_str(&base)?, ids: serde_json::from_str(&ids)? })
        })
        .collect()
    }

    pub fn put(&self, source: &str, user: &str, link: &Link) -> Result<()> {
        self.db().execute(
            "INSERT OR REPLACE INTO links (source, user, name, remote, remote_name, stamp, base, base_ids) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                source,
                user,
                link.name,
                link.remote,
                link.remote_name,
                link.stamp,
                serde_json::to_string(&link.base)?,
                serde_json::to_string(&link.ids)?
            ],
        )?;
        Ok(())
    }

    /// An agreed list names tracks, and a record's track can take one (`Catalogue::moved_tracks`): left as it was the track
    /// would read as removed and another added, and the server's copy would be edited for nothing. Place by place, as `base_ids`
    /// goes by place: a list that held both copies holds that track twice, as it can hold any. Their stamps stay, so the next pass
    /// reads no change. Safe to repeat. What it did, for the first start's report. Blocking.
    pub fn follow_tracks(&self, moved: &std::collections::HashMap<i64, i64>) -> Result<crate::moved::Report> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let rows: Vec<(String, String, String, String)> =
            tx.prepare("SELECT source, user, name, base FROM links")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
        let (n, mut walk) = (rows.len() as u64, crate::moved::Walk::default());
        for (source, user, name, base) in rows {
            let mut list: Vec<i64> = serde_json::from_str(&base)?;
            let mut changed = false;
            for track in &mut list {
                if let Some(new) = moved.get(track) {
                    *track = *new;
                    changed = true;
                    walk.moved += 1;
                }
            }
            if changed {
                tx.execute("UPDATE links SET base = ?4 WHERE source = ?1 AND user = ?2 AND name = ?3", params![source, user, name, serde_json::to_string(&list)?])?;
            }
        }
        tx.commit()?;
        let mut report = crate::moved::Report::default();
        report.walked("playlist-sync: links.base", n, walk);
        Ok(report)
    }

    /// Forget every link's agreed list and change stamp (its track ids belong to a catalogue that is gone): the next
    /// pass merges both sides as they are, adding what either has. Returns how many links.
    pub fn forget_baselines(&self) -> Result<usize> {
        Ok(self.db().execute("UPDATE links SET base = '[]', base_ids = '[]', stamp = NULL", [])?)
    }

    /// The agreed lists hold the track ids of one catalogue (`Catalogue::generation`). Given the catalogue now: when
    /// it is another than the one recorded, every list is forgotten (`forget_baselines`) and how many is returned;
    /// `None` when it is the same, or the first one seen (recorded then).
    pub fn follow_catalogue(&self, generation: &str) -> Result<Option<usize>> {
        let forgot = match self.meta(CATALOGUE) {
            Some(was) if was == generation => return Ok(None),
            Some(_) => Some(self.forget_baselines()?),
            None => None,
        };
        self.set_meta(CATALOGUE, generation)?;
        Ok(forgot)
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
        db.execute("UPDATE OR REPLACE left_by SET playlist = ?2 WHERE playlist = ?1", params![from, to])?;
        db.execute("UPDATE OR REPLACE retire SET playlist = ?2 WHERE playlist = ?1", params![from, to])?;
        db.execute("UPDATE OR REPLACE spoken SET playlist = ?2 WHERE playlist = ?1", params![from, to])?;
        Ok(db.execute("UPDATE links SET name = ?2 WHERE name = ?1", params![from, to])?)
    }

    /// Every sync link (decision 0014), by playlist.
    pub fn targets(&self) -> Result<Vec<Target>> {
        let db = self.db();
        let mut st = db.prepare("SELECT playlist, kind, source, user, copy FROM targets ORDER BY playlist, kind, source, user")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (playlist, kind, source, user, copy) = row?;
            let kind = match kind.as_str() {
                "shared" => Kind::Shared,
                "user" => Kind::User { source, user },
                _ => continue,
            };
            out.push(Target { playlist, kind, copy });
        }
        Ok(out)
    }

    pub fn put_target(&self, t: &Target) -> Result<()> {
        let (kind, source, user) = t.kind.columns();
        self.db().execute(
            "INSERT OR REPLACE INTO targets (playlist, kind, source, user, direction, conflict, copy) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![t.playlist, kind, source, user, TWO_WAY, SERVER_LEADS, t.copy],
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

    /// A link of a playlist that was deleted: put aside, so restoring the playlist brings it back.
    pub fn retire_target(&self, playlist: &str, kind: &Kind) -> Result<()> {
        let (k, source, user) = kind.columns();
        let mut db = self.db();
        let tx = db.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO retired SELECT playlist, kind, source, user, direction, conflict FROM targets
             WHERE playlist = ?1 AND kind = ?2 AND source = ?3 AND user = ?4",
            params![playlist, k, source, user],
        )?;
        tx.execute("DELETE FROM targets WHERE playlist = ?1 AND kind = ?2 AND source = ?3 AND user = ?4", params![playlist, k, source, user])?;
        tx.commit()?;
        Ok(())
    }

    /// Bring back the links put aside when the playlist was deleted. Returns how many.
    pub fn revive_targets(&self, playlist: &str) -> Result<usize> {
        let mut db = self.db();
        let tx = db.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO targets (playlist, kind, source, user, direction, conflict, copy)
             SELECT playlist, kind, source, user, direction, conflict, NULL FROM retired WHERE playlist = ?1",
            [playlist],
        )?;
        let n = tx.execute("DELETE FROM retired WHERE playlist = ?1", [playlist])?;
        tx.commit()?;
        Ok(n)
    }

    /// A new playlist of a deleted one's name: what was put aside is not its.
    pub fn forget_retired(&self, playlist: &str) -> Result<()> {
        self.db().execute("DELETE FROM retired WHERE playlist = ?1", [playlist])?;
        Ok(())
    }

    /// The accounts that took a shared playlist out of their account (it stays out for them), as (source, user).
    pub fn left_by(&self, playlist: &str) -> Result<Vec<(String, String)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT source, user FROM left_by WHERE playlist = ?1 ORDER BY source, user")?;
        let rows = st.query_map([playlist], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_left(&self, playlist: &str, source: &str, user: &str) -> Result<()> {
        self.db().execute("INSERT OR IGNORE INTO left_by (playlist, source, user) VALUES (?1, ?2, ?3)", params![playlist, source, user])?;
        Ok(())
    }

    /// Shared again: every account gets it again.
    pub fn clear_left(&self, playlist: &str) -> Result<()> {
        self.db().execute("DELETE FROM left_by WHERE playlist = ?1", [playlist])?;
        Ok(())
    }

    /// The copies in Music (imported by the server) still to remove, as (playlist, copy file name): each goes once the
    /// playlist's copy in its owner's account exists.
    pub fn to_retire(&self) -> Result<Vec<(String, String)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT playlist, copy FROM retire ORDER BY playlist")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn put_retire(&self, playlist: &str, copy: &str) -> Result<()> {
        self.db().execute("INSERT OR REPLACE INTO retire (playlist, copy) VALUES (?1, ?2)", params![playlist, copy])?;
        Ok(())
    }

    pub fn retired_copy(&self, playlist: &str) -> Result<()> {
        self.db().execute("DELETE FROM retire WHERE playlist = ?1", [playlist])?;
        Ok(())
    }

    /// Once (decision 0014, amended 2026-10-15): every Shared copy in Music is to be retired, and no playlist is
    /// shared any more (they were the owner's own; sharing is chosen anew per playlist). Returns how many, or None
    /// when done before.
    pub fn make_native(&self) -> Result<Option<usize>> {
        if self.meta(NATIVE).is_some() {
            return Ok(None);
        }
        let n = {
            let mut db = self.db();
            let tx = db.transaction()?;
            tx.execute("INSERT OR REPLACE INTO retire (playlist, copy) SELECT playlist, COALESCE(copy, playlist) FROM targets WHERE kind = 'shared'", [])?;
            let n = tx.execute("DELETE FROM targets WHERE kind = 'shared'", [])?;
            tx.execute("DELETE FROM retired WHERE kind = 'shared'", [])?;
            tx.commit()?;
            n
        };
        self.set_meta(NATIVE, "1")?;
        Ok(Some(n))
    }

    /// The other names each playlist is spoken as (decision 0015), by playlist.
    pub fn spoken(&self) -> Result<std::collections::HashMap<String, Vec<String>>> {
        let db = self.db();
        let mut st = db.prepare("SELECT playlist, name FROM spoken ORDER BY playlist, name")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for row in rows {
            let (p, n) = row?;
            out.entry(p).or_default().push(n);
        }
        Ok(out)
    }

    /// A playlist's spoken names, exactly these.
    pub fn set_spoken(&self, playlist: &str, names: &[String]) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        tx.execute("DELETE FROM spoken WHERE playlist = ?1", [playlist])?;
        for n in names {
            tx.execute("INSERT OR IGNORE INTO spoken (playlist, name) VALUES (?1, ?2)", params![playlist, n])?;
        }
        tx.commit()?;
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

    /// A track taken must read as itself in an agreed list, or the sync takes it for one removed and another added, and edits
    /// the server's copy for nothing.
    #[test]
    fn an_agreed_list_follows_a_track_that_was_taken() {
        let s = SyncState::open(None).unwrap();
        let link = Link { stamp: Some("e".into()), base: vec![1, 2, 3], ids: vec!["a".into(), "b".into(), "c".into()], ..Link::new("A", "r", "A") };
        s.put("jf", "u", &link).unwrap();
        s.put("jf", "other", &Link { base: vec![7], ids: vec!["x".into()], ..Link::new("B", "r", "B") }).unwrap();
        let moved = |s: &SyncState| s.follow_tracks(&std::collections::HashMap::from([(2, 9)])).unwrap().table_named("playlist-sync: links.base").unwrap().moved;
        assert_eq!(moved(&s), 1, "the one list that names it");
        let now = s.links("jf", "u").unwrap().remove(0);
        assert_eq!((now.base.as_slice(), now.stamp.as_deref()), ([1, 9, 3].as_slice(), Some("e")), "in its place, nothing else moved");
        assert_eq!(now.ids, ["a", "b", "c"], "the server's item is the same");
        assert_eq!(moved(&s), 0, "safe to repeat");
    }

    /// Two copies' tracks became one: the list both sides agreed on says so, or the next pass reads the track as taken out on
    /// one side and put in on the other. Place by place, as `base_ids` is.
    #[test]
    fn an_agreed_list_follows_its_tracks_place_by_place() {
        let s = SyncState::open(None).unwrap();
        let link = Link { stamp: Some("e1".into()), base: vec![3, 9, 2, 3], ids: vec!["c".into(), "i".into(), "".into(), "c".into()], ..Link::new("Chill", "r1", "Chill") };
        s.put("jf", "u1", &link).unwrap();
        s.put("jf", "u1", &Link { base: vec![5], ids: vec!["e".into()], ..Link::new("Other", "r2", "Other") }).unwrap();
        let moved = std::collections::HashMap::from([(9, 3)]);
        let report = s.follow_tracks(&moved).unwrap();
        let links = s.links("jf", "u1").unwrap();
        assert_eq!(links[0], Link { base: vec![3, 3, 2, 3], ..link }, "its stamp and server items as they were");
        assert_eq!(links[1].base, [5]);
        let t = report.table_named("playlist-sync: links.base").unwrap();
        assert_eq!((t.before, t.moved), (2, 1));
        assert_eq!(s.follow_tracks(&moved).unwrap().table_named("playlist-sync: links.base").unwrap().moved, 0, "safe to repeat");
    }

    #[test]
    fn a_new_catalogue_forgets_every_agreed_list_once() {
        let s = SyncState::open(None).unwrap();
        let link = |n: &str| Link { stamp: Some("e".into()), base: vec![1, 2], ids: vec!["a".into(), "b".into()], ..Link::new(n, &format!("r-{n}"), n) };
        s.put("jf", "u1", &link("A")).unwrap();
        s.put("jf", "u2", &link("B")).unwrap();
        assert_eq!(s.follow_catalogue("g1").unwrap(), None, "the first seen is recorded, nothing forgotten");
        assert_eq!(s.links("jf", "u1").unwrap()[0].base, vec![1, 2]);
        assert_eq!(s.follow_catalogue("g1").unwrap(), None, "the same catalogue");
        assert_eq!(s.links("jf", "u2").unwrap()[0].stamp.as_deref(), Some("e"));
        assert_eq!(s.follow_catalogue("g2").unwrap(), Some(2), "another catalogue: both lists forgotten");
        for u in ["u1", "u2"] {
            let l = &s.links("jf", u).unwrap()[0];
            assert!(l.base.is_empty() && l.ids.is_empty());
            assert_eq!(l.stamp, None);
        }
        s.put("jf", "u1", &link("A")).unwrap();
        assert_eq!(s.follow_catalogue("g2").unwrap(), None, "once: lists agreed since are kept");
        assert_eq!(s.links("jf", "u1").unwrap()[0].base, vec![1, 2]);
    }

    #[test]
    fn going_native_retires_the_shared_copies_once_and_shares_nothing() {
        let s = SyncState::open(None).unwrap();
        let user = Kind::User { source: "jf".into(), user: "u1".into() };
        s.put_target(&Target { playlist: "Chill".into(), kind: Kind::Shared, copy: Some("Chill".into()) }).unwrap();
        s.put_target(&Target { playlist: "Old".into(), kind: Kind::Shared, copy: None }).unwrap();
        s.put_target(&Target { playlist: "Chill".into(), kind: user.clone(), copy: None }).unwrap();
        assert_eq!(s.make_native().unwrap(), Some(2));
        assert_eq!(s.to_retire().unwrap(), vec![("Chill".into(), "Chill".into()), ("Old".into(), "Old".into())]);
        assert_eq!(s.targets().unwrap(), vec![Target { playlist: "Chill".into(), kind: user, copy: None }]);
        assert_eq!(s.make_native().unwrap(), None, "once");
        s.rename("Chill", "Calm").unwrap();
        s.set_left("Calm", "jf", "u2").unwrap();
        assert_eq!(s.left_by("Calm").unwrap(), vec![("jf".into(), "u2".into())]);
        s.rename("Calm", "Hush").unwrap();
        assert_eq!(s.left_by("Hush").unwrap(), vec![("jf".into(), "u2".into())]);
        s.clear_left("Hush").unwrap();
        assert!(s.left_by("Hush").unwrap().is_empty());
        assert!(s.to_retire().unwrap().iter().any(|(p, c)| p == "Hush" && c == "Chill"), "the copy keeps its file name");
        s.retired_copy("Hush").unwrap();
        assert_eq!(s.to_retire().unwrap().len(), 1);
    }

    #[test]
    fn links_saved_as_push_read_as_two_way() {
        let s = SyncState::open(None).unwrap();
        s.db()
            .execute("INSERT INTO targets (playlist, kind, source, user, direction, conflict) VALUES ('Chill', 'user', 'jf', 'u1', 'push', 'newest')", [])
            .unwrap();
        let user = Kind::User { source: "jf".into(), user: "u1".into() };
        assert_eq!(s.targets().unwrap(), vec![Target { playlist: "Chill".into(), kind: user, copy: None }]);
    }

    #[test]
    fn links_are_kept_per_server_and_user() {
        let s = SyncState::open(None).unwrap();
        let l = Link { stamp: Some("e1".into()), base: vec![3, 1, 2], ids: vec!["c".into(), "".into(), "b".into()], ..Link::new("Chill", "r1", "Chill") };
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
        let t = |kind: Kind, copy: Option<&str>| Target { playlist: "Chill".into(), kind, copy: copy.map(str::to_string) };
        s.put_target(&t(Kind::Shared, None)).unwrap();
        s.put_target(&t(user.clone(), None)).unwrap();
        s.put_target(&t(user.clone(), Some("x"))).unwrap();
        assert_eq!(s.targets().unwrap(), vec![t(Kind::Shared, None), t(user.clone(), Some("x"))], "one link per target");
        s.rename("Chill", "Calm").unwrap();
        assert!(s.targets().unwrap().iter().all(|t| t.playlist == "Calm"));
        s.remove_target("Calm", &user).unwrap();
        assert_eq!(s.targets().unwrap().len(), 1);
        assert_eq!(Kind::from_key(&user.key()), Some(user));
        assert_eq!(Kind::from_key("shared"), Some(Kind::Shared));
        assert_eq!(Kind::from_key("user/x"), None);
        let mut shared = t(Kind::Shared, None);
        shared.playlist = "Calm".into();
        s.retire_target("Calm", &Kind::Shared).unwrap();
        assert!(s.targets().unwrap().iter().all(|t| t.playlist != "Calm"), "put aside");
        assert_eq!(s.revive_targets("Calm").unwrap(), 1);
        assert_eq!(s.targets().unwrap(), vec![shared], "back as it was");
        assert_eq!(s.revive_targets("Calm").unwrap(), 0, "once");
        s.set_spoken("Calm", &["quiet".to_string(), "still".to_string()]).unwrap();
        s.rename("Calm", "Hush").unwrap();
        assert_eq!(s.spoken().unwrap().get("Hush"), Some(&vec!["quiet".to_string(), "still".to_string()]));
        s.set_spoken("Hush", &[]).unwrap();
        assert!(s.spoken().unwrap().is_empty());
        assert_eq!(s.meta("migrated"), None);
        s.set_meta("migrated", "1").unwrap();
        assert_eq!(s.meta("migrated").as_deref(), Some("1"));
    }
}
