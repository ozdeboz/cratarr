//! The app's requests (SP2-MOVE-NIGHT.md): requests, Soulseek, the turn, track fetches, intake cards, rooms, retention
//! and Spotify. The app's other parts are below it and above it.

pub use cratarr_records::app::{albums_base, along_base, analysis_pass_base, around_records, artists, audiomuse, backup_list, base_host, beyond, changes, commands, classical, collection, cover_cache, describe, device_index_base, discogs, discogs_facts, discover, editions, error, excluded_artists, file_facts_base, file_runs_base, file_words, first_start, fixes_base, follow, health_base, health_findings, history, import, incoming_base, incomplete_base, insights, insights_records, intake_card_base, labels, lastfm, library, listenbrainz, listening, listening_base, matching_base, moment, musicbrainz, originals, playlists_base, playon_base, plays, queue_base, queues_listening, radio_base, reach, records_host, references, references_turn, related, renumber_base, replace_base, requests_base, requests_turn, resume, retention_base, retention_protected, runtime_base, scrobble, search_base, settings, settings_base, shelf, soulseek_base, switches, sync, taste, tempo, track_fetch_records, track_fetch_turn, track_intake_base, turn, words, works};
pub use cratarr_records::app::{CollectionService, CollectionView, FollowTarget, FreshIds, InsightsService, Library, LidarrView, NamingView, PlayEvent, PlayOnCommand, Players, Plays, QueueTrack, SettingsService, SettingsView, SourceTab, SourceView, TargetChoice, ToggleError, VERSION, plain, players};

pub mod step_017;
#[cfg(test)]
mod peaks;
pub mod health_requests;
#[cfg(test)]
mod library_tests;
pub mod intake_card;
pub mod matching;
pub mod track_fetch;
pub mod soulseek;
pub mod refind;
pub mod requests;
pub mod requests_host;
#[cfg(test)]
mod served_tests_requests;
#[cfg(test)]
mod had_vectors;
// What counts against each user's room (AL01).
pub mod room;
// Records not had that nobody sees, forgotten (RT01).
pub mod retention;
pub mod spotify;
#[cfg(test)]
mod turn_tests;
