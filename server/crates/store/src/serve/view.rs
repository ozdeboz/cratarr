//! The server as the stores below it see it (SPLIT-PLAN P4, P5): `Serve` sits above the catalogue, the switches, Bandcamp,
//! MusicBrainz and the file runs, which still reach it where no handle does (`serve::instance`'s reasons). `serve::set_instance`
//! sets it here too; unset, as `instance()` is, where nothing set one (the tests).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use anyhow::Result;

use crate::catalogue::Catalogue;
use crate::serve::library::Library;
use crate::userdata::UserData;

/// What of the server the stores below it read, and what the crates' facts take from above (`Catalogue::crate_facts`).
pub trait ServeView: Send + Sync {
    fn userdata(&self) -> &Arc<UserData>;
    fn library(&self) -> &Arc<Library>;
    /// Cratarr's data folder.
    fn data_dir(&self) -> Option<PathBuf>;
    /// When the library was last built, epoch seconds.
    fn built_at(&self) -> i64;
    fn has_catalogue(&self) -> bool;
    fn catalogue(&self) -> &Arc<Catalogue>;
    /// Written files read again at once (`Serve::reread`). Blocking.
    fn reread(&self, paths: &[PathBuf]);
    /// What the library's words are made from, cheaply (`app::words::stamp`).
    fn words_stamp(&self) -> i64;
    /// What Classical is made of, cheaply (`app::classical::stamp`).
    fn classical_stamp(&self) -> i64;
    /// Classical where its album is one (`app::classical::add_kind`).
    fn add_classical(&self, facts: &mut [crate::crates::Facts]) -> Result<()>;
    /// Each file with its record's words and its own (`app::words::build`).
    fn add_words(&self, album_of: &HashMap<String, String>, facts: &mut [crate::crates::Facts]) -> Result<()>;
    /// Every album's labels as kept from Discogs, by its id (`app::discogs_facts::kept_labels`).
    fn kept_labels(&self) -> Result<HashMap<String, Vec<String>>>;
}

static VIEW: OnceLock<Arc<dyn ServeView>> = OnceLock::new();

/// Once, by `serve::set_instance`.
pub fn set(view: Arc<dyn ServeView>) {
    let _ = VIEW.set(view);
}

/// The server, for the stores below it that have no handle to it.
pub fn get() -> Option<Arc<dyn ServeView>> {
    VIEW.get().cloned()
}
