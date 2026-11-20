//! The app's records (SP2-MOVE-NIGHT.md): discovery, references, labels, classical, Discogs, artists, works and their
//! kin. The app's other parts are below it and above it.

pub use cratarr_listening::app::{albums_base, along_base, analysis_pass_base, audiomuse, backup_list, base_host, changes, collection, describe, device_index_base, error, excluded_artists, file_facts_base, file_runs_base, file_words, first_start, fixes_base, follow, health_base, health_findings, history, import, incoming_base, incomplete_base, intake_card_base, lastfm, library, listenbrainz, listening, listening_base, matching_base, moment, musicbrainz, playlists_base, playon_base, plays, queue_base, queues_listening, radio_base, reach, references_turn, renumber_base, replace_base, requests_base, requests_turn, resume, retention_base, retention_protected, runtime_base, scrobble, search_base, settings, settings_base, shelf, soulseek_base, switches, sync, taste, tempo, track_fetch_turn, track_intake_base, turn, words};
pub use cratarr_listening::app::{CollectionService, CollectionView, FollowTarget, FreshIds, Library, LidarrView, NamingView, PlayEvent, PlayOnCommand, Players, Plays, QueueTrack, SettingsService, SettingsView, SourceTab, SourceView, TargetChoice, ToggleError, VERSION, plain, players};

pub mod artists;
pub mod insights;
pub mod discover;
pub mod editions;
pub mod beyond;
pub mod labels;
pub mod around_records;
pub mod references;
pub mod discogs_facts;
pub mod related;
pub mod originals;
pub mod works;
pub mod classical;
pub mod discogs;
pub mod cover_cache;
pub mod track_fetch_records;
pub mod insights_records;
pub mod records_host;

pub use insights::InsightsService;
