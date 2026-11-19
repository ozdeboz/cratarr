//! A long import's read, kept as it goes (0.9.7, the owner: a failed import resumes where it stopped next time; the
//! plays already in stay, and it carries on from the last one in without asking again). Each page read is added to a
//! spool in the data folder, by source, account and user; the next import starts after its oldest; a finished read
//! takes it away. The history's own ids for each play keep an import that reads some plays twice from counting them
//! twice.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// What a read kept so far.
pub struct Spool<T> {
    path: PathBuf,
    pub items: Vec<T>,
}

impl<T: Serialize + DeserializeOwned> Spool<T> {
    /// The spool of `source`'s read of `user` for `account`; empty where there is none (or it cannot be read).
    pub fn open(dir: &Path, source: &str, account: &str, user: &str) -> Self {
        let name: String = format!("{source}-{account}-{}", user.to_lowercase()).chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
        let path = dir.join("import-resume").join(format!("{name}.json"));
        let items = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path, items }
    }

    /// A page read, added and kept at once.
    pub fn add(&mut self, page: impl IntoIterator<Item = T>) -> Result<()> {
        self.items.extend(page);
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, serde_json::to_vec(&self.items)?)?;
        Ok(())
    }

    /// The read finished: nothing to resume.
    pub fn finish(self) {
        std::fs::remove_file(&self.path).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_is_kept_page_by_page_until_it_finishes() {
        let dir = std::env::temp_dir().join(format!("cratarr-resume-{}", std::process::id()));
        let mut s: Spool<i64> = Spool::open(&dir, "lastfm", "me", "Someone");
        assert!(s.items.is_empty());
        s.add([30, 20]).unwrap();
        s.add([10]).unwrap();
        let again: Spool<i64> = Spool::open(&dir, "lastfm", "me", "someone");
        assert_eq!(again.items, [30, 20, 10], "found again after a stop, the user's name in any case");
        again.finish();
        assert!(Spool::<i64>::open(&dir, "lastfm", "me", "someone").items.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
