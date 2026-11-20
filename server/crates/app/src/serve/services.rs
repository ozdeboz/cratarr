//! What `Serve` holds of the app above it, once the app is up: the playlist service, Incoming and the players
//! (`Serve::set_playlists`, `set_incoming`, `set_play_on`), and what the crates' facts take from the app
//! (`Serve::set_facts_from`). Each is a narrow trait of what Serve itself asks of it, implemented where the service is.
//! Code above that knows a service takes it back as its own type (`as_any`: `PlaylistService::held_by` and the like).

use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;

use super::Serve;

/// Cratarr's playlist service, as Serve asks of it: the Subsonic playlists are its files.
pub trait Playlists: Send + Sync {
    /// What every account keeps in its crates follows ids a take moved (`PlaylistService::follow_tracks`).
    fn follow_tracks<'a>(&'a self, moved: &'a crate::moved::Moved) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>>;
    /// The service as its own type, for code above that knows it.
    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

/// Cratarr's Incoming, as Serve asks of it: promote and reject are its Keep and Delete.
pub trait Incoming: Send + Sync {
    /// Each catalogue folder's library, as Incoming knows them (`IncomingService::folder_kinds`).
    fn folder_kinds(&self) -> Vec<(i64, crate::settings::LibraryKind)>;
    /// The catalogue folders of the libraries switched off, all but `without` (`IncomingService::folders_off`).
    fn folders_off(&self, without: &[String]) -> Vec<i64>;
    /// The service as its own type, for code above that knows it.
    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

/// Cratarr's players (Play On): an account's devices, and what plays for getNowPlaying. Serve only holds them.
pub trait Players: Send + Sync {
    /// The players as their own type, for code above that knows them.
    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

/// What the crates' facts take from the app (`view::ServeView`'s): Classical, the words and Discogs' labels.
pub trait FactsFrom: Send + Sync {
    /// Classical where its album is one (`app::classical::add_kind`).
    fn add_classical(&self, serve: &Serve, facts: &mut [crate::crates::Facts]) -> Result<()>;
    /// Each file with its record's words and its own (`app::words::build`).
    fn add_words(&self, serve: &Serve, album_of: &HashMap<String, String>, facts: &mut [crate::crates::Facts]) -> Result<()>;
    /// Every album's labels as kept from Discogs, by its id (`app::discogs_facts::kept_labels`).
    fn kept_labels(&self, serve: &Serve) -> Result<HashMap<String, Vec<String>>>;
}
