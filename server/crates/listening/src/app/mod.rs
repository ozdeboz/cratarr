//! The app's listening (SP2-MOVE-NIGHT.md): plays, scrobbles, history imports, resume. The app's other parts are
//! below it and above it.

pub use cratarr_base::app::{albums_base, along_base, analysis_pass_base, audiomuse, backup_list, base_host, changes, commands, collection, describe, device_index_base, error, excluded_artists, file_facts_base, file_runs_base, file_words, first_start, fixes_base, follow, health_base, health_findings, history, incoming_base, incomplete_base, intake_card_base, library, listening_base, matching_base, moment, musicbrainz, playlists_base, playon_base, queue_base, radio_base, reach, references_turn, renumber_base, replace_base, requests_base, requests_turn, retention_base, retention_protected, runtime_base, search_base, settings, settings_base, shelf, soulseek_base, switches, sync, taste, tempo, track_fetch_turn, track_intake_base, turn, words};
pub use cratarr_base::app::{CollectionService, CollectionView, FollowTarget, FreshIds, Library, LidarrView, NamingView, PlayOnCommand, Players, QueueTrack, SettingsService, SettingsView, SourceTab, SourceView, TargetChoice, ToggleError, VERSION, plain, players};

pub mod plays;
pub mod import;
pub mod resume;
pub mod lastfm;
pub mod listening;
#[cfg(test)]
mod served_tests_listening;
pub mod queues_listening;
pub mod scrobble;
pub mod listenbrainz;

pub use plays::{PlayEvent, Plays};
