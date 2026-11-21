//! Wiring (decision 0011): builds the feature services, answers the navigation header, and runs the background pass.
//! The HTTP handlers call the services directly; nothing here decides anything.
//!
//! - `library`: the servers in use, the folders behind their libraries, the playlist store, the catalogue;
//! - `playing`: what plays, the controls, adding it to a playlist;
//! - `playlists`: playlist files, edits, repairs, Play;
//! - `crates`: crates, Sounds like, the fits and the taste, Home's crates, what is excluded everywhere;
//! - `incoming`: new music, verdicts, Delete and Undo, Lidarr;
//! - `collection`: the albums of the Music library;
//! - `shelf`: reading album lists, shared by Incoming and Collection;
//! - `insights`: figures about the library and the playlists;
//! - `sync`: the user's own server playlists, kept in step with the files;
//! - `settings`: the Settings page.

pub use cratarr_health::app::{arrival_job, albums, albums_base, along_base, analysis_import, analysis_pass, analysis_pass_base, around, around_records, artists, audiomuse, backup_list, base_host, beat_this, beyond, changes, commands, classical, collection, cover_cache, covers, crates, crates_from_radio, crates_insights, crates_served, describe, device_covers, device_index, device_index_base, discogs, discogs_facts, discover, editions, error, excluded_artists, file_facts, file_facts_base, file_runs, file_runs_base, file_words, first_start, fix_plans, fixes, fixes_base, follow, harvest, health, health_base, health_findings, health_host, health_requests, history, home, import, incoming, incoming_base, incoming_host, incoming_served, incoming_settings, incomplete, incomplete_base, insights, insights_records, intake_card, intake_card_base, kodi, labels, lastfm, length_fixes, library, listenbrainz, listening, listening_base, matching, matching_base, moment, musicbrainz, originals, play, playing, playing_host, playlists, playlists_base, playlists_settings, playon, playon_base, playon_players, playon_served, plays, queue, queue_base, queue_requests, queues_listening, radio, radio_base, reach, records_host, references, references_door, references_turn, refind, related, renumber, renumber_base, replace_base, requests, requests_base, requests_host, requests_turn, resume, retention, retention_base, retention_protected, room, runtime_base, scrobble, search_base, settings, settings_base, shelf, similar, soulseek, soulseek_base, spotify, step_017, step_018, switches, sync, taste, tempo, together, track_fetch, track_fetch_records, track_fetch_turn, track_intake, track_intake_base, turn, visuals, words, works};
pub use cratarr_health::app::{AlbumFate, AlbumMore, AlbumRef, AlbumView, AlbumsView, ArtistView, ByName, CollectionService, CollectionView, CopySheet, CrateLook, CrateOrder, CratePage, CrateService, CrateView, DeleteResult, Digging, FitSummary, FitView, FollowTarget, FreshIds, FromQueue, IncomingService, IncomingView, InsightsService, JobView, KeepPreview, KeepResult, Library, LidarrView, LikeCurve, MomentView, NamingView, NotArtist, OpsAnswer, PlayEvent, PlayOn, PlayOnCommand, PlayOnSettings, PlayOnSettingsView, PlayResult, PlayerReport, Players, PlayingService, PlaylistOps, PlaylistRow, PlaylistService, PlaylistStats, Plays, QueueTrack, QueueTracks, ReplaceAsk, ReplacePreview, ReplaceSheet, SettingsService, SettingsView, SourceTab, SourceView, StateView, TargetChoice, ToggleError, TrackStateView, VERSION, incoming_jobs_shown, plain, players, whose_players};
pub use cratarr_search::app::{search, search_host};

mod app_albums;
pub mod capabilities;
// The record as one object, read for its views (DC1a: OBJECT-MAP-2 §2.1).
pub mod core;
mod app_served;
mod app_base;
mod app_protected;
mod app_records;
mod events;
pub mod filter;
mod app_health;
mod app_incoming;
pub mod kept;
mod app_playing;
mod app_playlists;
mod app_home;
pub mod import_files;
#[cfg(test)]
mod every_copy;
#[cfg(test)]
mod file_words_tests;
// GEN's tests on the whole app (lane C's run 2): Bandcamp's tags from a record's own page.
#[cfg(test)]
mod gen_words_tests;
pub mod runtime;
mod app_search;
mod app_requests;
// Incoming's tests that run the one path on the whole app, moved up (the app split's phase 2).
#[cfg(test)]
mod incoming_tests_top;
// Playing's test that runs on the whole app, moved up (the app split's phase 2).
#[cfg(test)]
mod playing_tests_top;
// The proof that moving the rules changes no answer (decision 0101 point 27), above every area it reads.
#[cfg(test)]
mod radio_proof;
#[cfg(test)]
mod tag_rules_tests;
pub mod tracks;

use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;

pub use events::connect as events;

use crate::config::Config;
use crate::settings::{Origin, Saved};

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
    /// Crates over the playlists (decision 0024).
    pub crates: Arc<CrateService>,
    pub incoming: Arc<IncomingService>,
    pub collection: Arc<CollectionService>,
    pub insights: InsightsService,
    /// Artists' descriptions and photos from TheAudioDB, built slowly (Home's Artists to explore); none without a data
    /// folder.
    pub artists: Option<Arc<artists::ArtistInfo>>,
    pub settings: SettingsService,
    /// What happened to albums (decision 0011); the live channel tells the pages.
    pub journal: Arc<crate::journal::Journal>,
    /// The one command path and its one Undo (GF): its kinds registered here, its Changes in the journal's file.
    pub commands: Arc<commands::Commands>,
    /// Notices when a player changes, for the live channel.
    pub events: events::PlayerWatch,
    /// The players Cratarr commands itself: the Android app (decision 0031).
    pub play_on: Arc<PlayOn>,
    /// Plays the players tell, late too, each counted once (decision 0033, step 4).
    pub plays: Plays,
    /// The visuals each sync group shares (decision 0046, amended).
    pub visuals: Arc<visuals::GroupVisuals>,
    /// The Opus copies kept for the app's streams on mobile data and smaller downloads; none without a data folder.
    pub opus_copies: Option<Arc<crate::opus_copies::OpusCopies>>,
    /// Cratarr's own library (decision 0030), opened before the app: `main` always hands it; a test may not.
    pub served: Option<Arc<crate::serve::Serve>>,
    /// The sync play server, once it listens (`main`, decision 0037).
    pub snapcast: std::sync::OnceLock<Arc<crate::snapcast::Snapcast>>,
}

impl App {
    pub async fn new(cfg: Config, saved: Saved, origin: Origin, served: Option<Arc<crate::serve::Serve>>) -> Result<Arc<Self>> {
        // What the crates' facts take from the app, before anything can read them (`serve::services::FactsFrom`).
        if let Some(s) = &served {
            s.set_facts_from(Arc::new(app_served::AppFacts));
            s.set_protected(Arc::new(app_protected::AppProtected));
        }
        let last_music = saved.music_folder.or_else(|| cfg.playlist_dir.clone());
        // The playlists are kept in Cratarr's own library's user data (decision 0101); a test without one keeps them in
        // memory.
        let playlists_db = match &served {
            Some(s) => s.userdata.clone(),
            None => Arc::new(crate::userdata::UserData::open(None)?),
        };
        let library = Library::new(cfg.data_dir.clone(), last_music, saved.sources, saved.tag_rules, saved.naming, playlists_db, Some(step_017::run)).await?;
        let sync_file = cfg.data_dir.as_ref().map(|d| std::path::PathBuf::from(d).join("playlist-sync.sqlite"));
        let journal_file = cfg.data_dir.as_ref().map(|d| std::path::PathBuf::from(d).join("journal.sqlite"));
        // After the first start (`Library::new`), which moves 0.15's journal: the server's one journal (`main` makes it the instance).
        let journal = Arc::new(crate::journal::Journal::open(journal_file.as_deref())?);
        let sync_state = crate::sync_state::SyncState::open(sync_file.as_deref())?;
        let sync = Arc::new(sync::SyncService::new(library.clone(), sync_state, journal.clone()));
        sync.listen();
        let playlists = Arc::new(PlaylistService::new(library.clone(), sync.clone(), journal.clone(), served.clone()));
        let crates = Arc::new(CrateService::new(playlists.clone()));
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
        // The one path, the file kinds' window the trash's days as Settings has them now; each kind's Undo registered.
        let trash_days = {
            let incoming = incoming.clone();
            Arc::new(move || i64::from(settings_base::SettingsIncoming::trash_days(&*incoming)))
        };
        let commands = Arc::new(commands::Commands::new(journal.clone(), served.clone(), trash_days));
        if let Some(s) = &served {
            commands.register(Arc::new(crates::out::OutKind { serve: s.clone() }));
        }
        // What a stop left between a Change's intent and its end, settled by what its store shows.
        if let Err(e) = commands.settle().await {
            tracing::warn!(reason = %format!("{e:#}"), "the changes a stop left were not settled");
        }
        let playing = PlayingService::new(library.clone(), playlists.clone());
        playing.set_file_facts(Arc::new(app_playing::AppFileFacts));
        let settings = SettingsService::new(
            library.clone(),
            files,
            playlists.clone(),
            incoming.clone(),
            cfg.bind,
            cfg.data_dir.as_deref(),
            origin,
        );
        let insights = InsightsService::new(library.clone(), crates.clone());
        let play_on = PlayOn::new(library.clone(), playlists.clone(), cfg.data_dir.as_deref());
        let plays = Plays::new(cfg.data_dir.as_deref(), served.as_ref().map(|s| s.userdata.clone()));
        // What offline writes keep (decision 0087) forgotten after its 30 days, hourly.
        if let Some(served) = &served {
            crate::userdata::offline::prune_in_background(served.userdata.clone());
        }
        let artists = artists::ArtistInfo::open(cfg.data_dir.as_deref());
        if let Some(a) = &artists {
            a.build_slowly();
        }
        originals::start();
        discogs::start();
        // The library's words (0.9.7): MusicBrainz, Discogs and Last.fm asked in the background.
        harvest::start();
        // Spotify's lists (0073): read again every 6 hours, MusicBrainz asked about them.
        spotify::open(cfg.data_dir.as_deref());
        spotify::start();
        // Cratarr's own library (0032) built anew (a first start, a new layout): what was read from it while it was
        // empty is read again.
        if let Some(served) = &served {
            let mut built = served.subscribe_built();
            let (collection, incoming) = (collection.clone(), incoming.clone());
            crate::run::spawn(async move {
                while built.changed().await.is_ok() {
                    incoming.forget_lists().await;
                    collection.refresh().await;
                }
            });
        }
        // Shared visuals go with what they are shared by: a group's while a player is in it, a player's while it is
        // known.
        let visuals = Arc::new(visuals::GroupVisuals::new());
        {
            let (visuals, play_on) = (visuals.clone(), play_on.clone());
            let mut changed = play_on.subscribe();
            crate::run::spawn(async move {
                loop {
                    let groups = play_on.list().into_iter().flat_map(|p| [Some(p.name), p.group]).flatten().collect();
                    visuals.keep_only(&groups);
                    if let Err(tokio::sync::broadcast::error::RecvError::Closed) = changed.recv().await {
                        return;
                    }
                }
            });
        }
        let opus_copies = crate::opus_copies::OpusCopies::open(cfg.data_dir.as_deref());
        let app = Arc::new(Self {
            library,
            sync,
            playing,
            playlists,
            crates,
            incoming,
            collection,
            insights,
            artists,
            settings,
            journal,
            commands,
            events: Default::default(),
            play_on,
            plays,
            visuals,
            opus_copies,
            served,
            snapcast: Default::default(),
        });
        // What a Play On play names is chosen through app::play, as every play.
        app.play_on.set_app(&app);
        Ok(app)
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
            incoming: self.library.incoming_on(),
            sources,
            server_name,
            version: VERSION,
            problems: self.library.problems(),
        }
    }

    /// AudioMuse's file taken in when it changed (decision 0054); a note in the log of what it brought.
    async fn audiomuse(&self) {
        let Some(dir) = self.library.data_dir() else { return };
        let (cat, path) = (self.library.catalogue(), std::path::Path::new(&dir).join(audiomuse::FILE));
        match crate::run::spawn_blocking(move || audiomuse::import_if_changed(&cat, &path)).await {
            Ok(Ok(Some(c))) => tracing::info!("audiomuse: {} tracks taken in, {} not in the library, {} unreadable lines", c.kept, c.unknown, c.bad),
            Ok(Ok(None)) => {}
            Ok(Err(e)) => tracing::warn!("audiomuse: {e:#}"),
            Err(e) => tracing::warn!("audiomuse: {e}"),
        }
    }

    /// Beat this!'s beats from the PC taken in, and the list of files still without them written for it, when it
    /// changed (decision 0049, amended); a note in the log of what came.
    async fn beat_this(&self) {
        let Some(dir) = self.library.data_dir() else { return };
        let (cat, dir) = (self.library.catalogue(), std::path::Path::new(&dir).to_path_buf());
        let done = crate::run::spawn_blocking(move || {
            let counts = beat_this::import_waiting(&cat, &dir)?;
            let wanted = beat_this::write_wanted(&cat, &dir)?;
            anyhow::Ok((counts, wanted))
        })
        .await;
        match done {
            Ok(Ok((counts, wanted))) => {
                if let Some(c) = counts {
                    tracing::info!("beats: {} files from the PC: {} tracks taken in, {} changed since, {} not in the library, {} unreadable lines", c.files, c.stored, c.stale, c.unknown, c.bad);
                }
                if let Some(n) = wanted {
                    tracing::info!("beats: {n} files listed for the PC");
                }
            }
            Ok(Err(e)) => tracing::warn!("beats: {e:#}"),
            Err(e) => tracing::warn!("beats: {e}"),
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
            crate::run::spawn(async move { collection.refresh().await });
        }
        if pass % 120 == 0 {
            self.incoming.purge_trash().await;
        }
        // AudioMuse's analysis from the PC (decision 0054), when its file changed: every five minutes.
        if pass % 10 == 0 {
            self.audiomuse().await;
        }
        // Beat this!'s, from the PC and for it (decision 0049, amended): as often, a pass apart.
        if pass % 10 == 5 {
            self.beat_this().await;
        }
    }
}
