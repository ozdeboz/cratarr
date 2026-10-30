//! Opening Cratarr's SQLite files. Every connection keeps its temporary storage in memory: the image has no /tmp (FROM
//! scratch), so a sort, an index being built or a statement journal that spills to a temporary file fails with "disk I/O
//! error" (6410, no temporary directory). Crates did, 2026-10-17, on a connection that had lost the setting.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};

/// A file to read and write (created when missing); `None`: in memory.
pub fn open(file: Option<&Path>) -> Result<Connection> {
    let conn = match file {
        Some(f) => Connection::open(f).with_context(|| format!("opening {}", f.display()))?,
        None => Connection::open_in_memory()?,
    };
    in_memory_temp(conn)
}

/// A file to read beside its writer (WAL), not to change.
pub fn open_read_only(file: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .with_context(|| format!("opening {} to read", file.display()))?;
    in_memory_temp(conn)
}

fn in_memory_temp(conn: Connection) -> Result<Connection> {
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(c: &Connection) -> i64 {
        c.query_row("PRAGMA temp_store", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn every_connection_keeps_its_temporary_storage_in_memory() {
        let dir = std::env::temp_dir().join(format!("cratarr-db-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.sqlite");
        let rw = open(Some(&file)).unwrap();
        rw.execute_batch("PRAGMA journal_mode = WAL; CREATE TABLE t (v TEXT);").unwrap();
        let ro = open_read_only(&file).unwrap();
        assert_eq!((temp_store(&rw), temp_store(&ro), temp_store(&open(None).unwrap())), (2, 2, 2));
        drop((rw, ro));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
