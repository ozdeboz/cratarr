//! Wiring (decision 0011): builds the feature services, answers the navigation header, and runs the background pass.
//! The HTTP handlers call the services directly; nothing here decides anything.
//!
//! - `library`: the servers in use, the folders behind their libraries, the playlist store, the catalogue;
//! - `playing`: what plays, the controls, adding it to a playlist;
//! - `playlists`: playlist files, edits, repairs, Play;
//! - `incoming`: new music, verdicts, Delete and Undo, Lidarr;
//! - `collection`: the albums of the Music library;
//! - `shelf`: reading album lists, shared by Incoming and Collection;
//! - `insights`: figures about the library and the playlists;
//! - `sync`: the user's own server playlists, kept in step with the files;
//! - `settings`: the Settings page.

mod albums;
pub mod analysis_import;
mod collection;
mod error;
mod events;
mod follow;
mod incoming;
mod insights;
pub mod kept;
mod kodi;
mod library;
mod playing;
mod playlists;
mod playon;
mod plays;
mod queue;
pub mod radio;
mod settings;
mod shelf;
mod sync;
pub mod together;

use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;

pub use albums::AlbumsView;
pub use collection::{CollectionService, CollectionView};
pub use error::ToggleError;
pub use incoming::{AlbumFate, DeleteResult, IncomingService, IncomingView, JobView, KeepPreview, KeepResult, LidarrView, ReplacePreview};
pub use insights::InsightsService;
pub use library::{Library, SourceTab};
pub use playing::{AlbumRef, AlbumView, ArtistView, FromQueue, PlayingService, StateView, TrackStateView};
pub use playlists::{ByName, CrateView, PlayMode, PlayResult, PlaylistService};
pub use playon::{Command as PlayOnCommand, PlayOn, PlayOnSettings, PlayOnSettingsView, QueueTrack, Report as PlayerReport};
pub use plays::{PlayEvent, Plays};
pub use sync::TargetChoice;
pub use events::connect as events;
pub use settings::{NamingView, SettingsService, SettingsView, SourceView};

use crate::config::Config;
use crate::settings::{Origin, Saved};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How often the background pass runs (see `App::sync`).
pub const SYNC_SECS: u64 = 30;

#[derive(Debug, Serialize)]
pub struct StatusView {
    pub connected: bool,
    /// A Music library is set and its folder found: playlists can be read and made.
    pub music: bool,
    /// Some enabled server has an Incoming library: the page shows incoming-music features.
    pub incoming: bool,
    /// Connected sources, for the source picker (`playing` is not checked here and always false).
    pub sources: Vec<SourceTab>,
    /// The server's name with one source; "n of m sources" with several.
    pub server_name: Option<String>,
    pub version: &'static str,
    /// What is wrong with the servers, one line each (a server that stopped processing changes).
    pub problems: Vec<String>,
}

pub struct App {
    pub library: Arc<Library>,
    /// The user's own server playlists, in step with the files (decision 0012).
    pub sync: Arc<sync::SyncService>,
    pub playing: PlayingService,
    pub playlists: Arc<PlaylistService>,
    pub incoming: Arc<IncomingService>,
    pub collection: Arc<CollectionService>,
    pub insights: InsightsService,
    pub settings: SettingsService,
    /// What happened to albums (decision 0011); the live channel tells the pages.
    pub journal: Arc<crate::journal::Journal>,
    /// Notices when a player changes, for the live channel.
    pub events: events::PlayerWatch,
    /// The players Cratarr commands itself: the Android app (decision 0031).
    pub play_on: Arc<PlayOn>,
    /// Plays the players tell, late too, each counted once (decision 0033, step 4).
    pub plays: Plays,
}

impl App {
    pub async fn new(cfg: Config, saved: Saved, origin: Origin) -> Result<Arc<Self>> {
        let last_music = saved.music_folder.or_else(|| cfg.playlist_dir.clone());
        let library = Library::new(cfg.data_dir.clone(), last_music, saved.sources, saved.tag_rules, saved.naming).await?;
        let sync_file = cfg.data_dir.as_ref().map(|d| std::path::PathBuf::from(d).join("playlist-sync.sqlite"));
        let journal_file = cfg.data_dir.as_ref().map(|d| std::path::PathBuf::from(d).join("journal.sqlite"));
        let journal = Arc::new(crate::journal::Journal::open(journal_file.as_deref())?);
        let sync_state = crate::sync_state::SyncState::open(sync_file.as_deref())?;
        let sync = Arc::new(sync::SyncService::new(library.clone(), sync_state, journal.clone()));
        sync.listen();
        let playlists = Arc::new(PlaylistService::new(library.clone(), sync.clone(), journal.clone()));
        let files = crate::ports::disk();
        let shelves = Arc::new(shelf::Shelves::new(library.clone(), cfg.data_dir.as_deref()));
        let collection = Arc::new(CollectionService::new(library.clone(), shelves.clone()));
        let incoming = Arc::new(IncomingService::new(
            library.clone(),
            files.clone(),
            journal.clone(),
            saved.lidarr,
            saved.trash_days,
            cfg.data_dir.as_deref(),
            shelves,
        ));
        incoming.listen();
        incoming.work();
        let playing = PlayingService::new(library.clone(), playlists.clone());
        let settings = SettingsService::new(
            library.clone(),
            files,
            playlists.clone(),
            incoming.clone(),
            cfg.bind,
            cfg.data_dir.as_deref(),
            origin,
        );
        let insights = InsightsService::new(library.clone());
        let play_on = PlayOn::new(library.clone(), playlists.clone(), cfg.data_dir.as_deref());
        let plays = Plays::new(cfg.data_dir.as_deref());
        // Cratarr's own library (0032) built anew (a first start, a new layout): what was read from it while it was
        // empty is read again.
        if let Some(served) = crate::serve::instance() {
            let mut built = served.subscribe_built();
            let (collection, incoming) = (collection.clone(), incoming.clone());
            tokio::spawn(async move {
                while built.changed().await.is_ok() {
                    incoming.forget_lists().await;
                    collection.refresh().await;
                }
            });
        }
        Ok(Arc::new(Self { library, sync, playing, playlists, incoming, collection, insights, settings, journal, events: Default::default(), play_on, plays }))
    }

    /// For the navigation header.
    pub async fn status(&self) -> StatusView {
        let sources = self.library.connected();
        let mut statuses = Vec::new();
        for (_, s) in &sources {
            statuses.push(s.status().await.unwrap_or_default());
        }
        let up = statuses.iter().filter(|s| s.connected).count();
        let server_name = match statuses.as_slice() {
            [] => None,
            [one] => one.server_name.clone(),
            many => Some(format!("{up} of {} sources", many.len())),
        };
        let sources =
            sources.iter().map(|(a, _)| SourceTab { id: a.cfg.id.clone(), label: library::label_of(&a.cfg), playing: false }).collect();
        StatusView {
            connected: up > 0,
            music: self.library.music().is_some(),
            incoming: self.library.live().iter().any(|a| a.cfg.enabled && !a.cfg.libraries.incoming.is_empty()),
            sources,
            server_name,
            version: VERSION,
            problems: self.library.problems(),
        }
    }

    /// The one background pass that keeps Cratarr in step with its sources and the library, run every
    /// `SYNC_SECS`. Sources offer no reliable change notifications (webhooks are optional plugins), so it looks:
    /// incoming lists every pass; dead playlist entries and sources still missing their libraries every other pass;
    /// the Music library's albums every quarter of an hour (reading them takes the server half a minute); the Incoming
    /// trash every hour.
    pub async fn sync(&self, pass: u64) {
        self.incoming.refresh().await;
        self.sync.pass().await;
        if pass % 2 == 0 {
            self.incoming.catch_up().await;
            self.playlists.refresh_dead().await;
            self.library.retry_placement().await;
            self.settings.infer_naming().await;
            self.library.watch_servers().await;
        }
        if pass % 30 == 0 {
            // On its own: half a minute of reading must not hold up the pass.
            let collection = self.collection.clone();
            tokio::spawn(async move { collection.refresh().await });
        }
        if pass % 120 == 0 {
            self.incoming.purge_trash().await;
        }
    }
}
