//! The app's shared services and what the areas above it share (SP2-MOVE-NIGHT.md): library, settings, sync, shelf,
//! collection, follow, words and the parts moved down from the areas (`<origin>_base`). The app's other parts
//! are below it (cratarr-served and the lower crates) and above it.

pub use cratarr_served::app::{audiomuse, error, excluded_artists, import, moment, musicbrainz, reach, references_turn, requests_turn, switches, track_fetch_turn, turn};

pub mod changes;
pub mod analysis_pass_base;
pub mod collection;
pub mod first_start;
pub mod follow;
pub mod health_base;
pub mod health_findings;
pub mod backup_list;
pub mod library;
pub mod history;
pub mod words;
pub mod file_words;
pub mod track_intake_base;
pub mod runtime_base;
pub mod retention_base;
pub mod taste;
pub mod tempo;
pub mod settings;
pub mod shelf;
pub mod sync;
pub mod describe;
// Base's parts of the modules above it, moved down for the split's phase 2 (each named for its origin).
pub mod base_host;
pub mod device_index_base;
pub mod file_facts_base;
pub mod file_runs_base;
pub mod incoming_base;
pub mod intake_card_base;
pub mod matching_base;
pub mod listening_base;
pub mod playlists_base;
pub mod settings_base;
pub mod playon_base;
pub mod queue_base;
pub mod radio_base;
pub mod along_base;
pub mod renumber_base;
pub mod albums_base;
pub mod fixes_base;
pub mod incomplete_base;
pub mod replace_base;
pub mod requests_base;
pub mod soulseek_base;
pub mod search_base;
pub mod retention_protected;

pub use collection::{CollectionService, CollectionView};
pub use error::{ToggleError, plain};
pub use incoming_base::LidarrView;
pub use library::{FreshIds, Library, SourceTab};
pub use playon_base::{Command as PlayOnCommand, Players, QueueTrack, players};
pub use along_base::FollowTarget;
pub use sync::TargetChoice;
pub use settings::{NamingView, SettingsService, SettingsView, SourceView};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
