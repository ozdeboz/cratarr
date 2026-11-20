//! The app's playing (SP2-MOVE-NIGHT.md): Play On, the queue, devices, radio, home, playlists and crates, the visuals.
//! The app's other parts are below it and above it.

pub use cratarr_playlists::app::{albums_base, along_base, analysis_pass_base, around_records, artists, audiomuse, backup_list, base_host, beyond, changes, classical, collection, cover_cache, describe, device_index_base, discogs, discogs_facts, discover, editions, error, excluded_artists, file_facts_base, file_runs_base, file_words, first_start, fixes_base, follow, health_base, health_findings, heavy_base, history, import, incoming_base, incomplete_base, insights, insights_records, intake_card_base, labels, lastfm, library, listenbrainz, listening, listening_base, matching_base, moment, musicbrainz, originals, playlists, playlists_base, playlists_settings, playon_base, plays, queue_base, queues_listening, radio_base, reach, records_host, references, references_turn, related, renumber_base, replace_base, requests_base, requests_turn, resume, retention_base, retention_protected, runtime_base, scrobble, search_base, settings, settings_base, shelf, soulseek_base, switches, sync, tasks, taste, tempo, track_fetch_records, track_fetch_turn, track_intake_base, turn, words, works};
pub use cratarr_playlists::app::{ByName, CollectionService, CollectionView, FollowTarget, FreshIds, InsightsService, Library, LidarrView, NamingView, OpsAnswer, PlayEvent, PlayOnCommand, PlayResult, Players, PlaylistOps, PlaylistRow, PlaylistService, PlaylistStats, Plays, QueueTrack, SettingsService, SettingsView, SourceTab, SourceView, TargetChoice, ToggleError, VERSION, plain, players};

pub mod kodi;
pub mod playing;
pub mod playing_host;
pub mod play;
pub mod crates;
pub mod crates_served;
pub mod playon;
pub mod playon_served;
pub mod queue;
pub mod home;
pub mod around;
#[cfg(test)]
mod references_playing_tests;
#[cfg(test)]
mod discover_playing_tests;
#[cfg(test)]
mod labels_playing_tests;
pub mod queue_requests;
pub mod radio;
pub mod similar;
pub mod together;
pub mod device_covers;
pub mod device_index;
pub mod visuals;
pub mod playon_players;
pub mod crates_insights;
pub mod crates_from_radio;

pub use playing::{AlbumMore, AlbumRef, AlbumView, ArtistView, FromQueue, PlayingService, StateView, TrackStateView};
pub use crates::{CrateLook, CrateOrder, CratePage, CrateService, CrateView, Digging, FitSummary, FitView, LikeCurve, MomentView, NotArtist};
pub use playon::{PlayOn, PlayOnSettings, PlayOnSettingsView, QueueTracks, Report as PlayerReport, whose as whose_players};
