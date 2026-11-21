//! The app's incoming (SP2-MOVE-NIGHT.md): Incoming and its runs, the album actions, fixes, covers and the
//! analysis pass. The app's other parts are below it and above it.

pub use cratarr_requests::app::{albums_base, along_base, analysis_pass_base, around_records, artists, audiomuse, backup_list, base_host, beyond, changes, commands, classical, collection, cover_cache, describe, device_index_base, discogs, discogs_facts, discover, editions, error, excluded_artists, file_facts_base, file_runs_base, file_words, first_start, fixes_base, follow, health_base, health_findings, health_requests, history, import, incoming_base, incomplete_base, insights, insights_records, intake_card, intake_card_base, labels, lastfm, library, listenbrainz, listening, listening_base, matching, matching_base, moment, musicbrainz, originals, playlists_base, playon_base, plays, queue_base, queues_listening, radio_base, reach, records_host, references, references_door, references_turn, refind, related, renumber_base, replace_base, requests, requests_base, requests_host, requests_turn, resume, retention, retention_base, retention_protected, room, runtime_base, scrobble, search_base, settings, settings_base, shelf, soulseek, soulseek_base, spotify, step_017, switches, sync, taste, tempo, track_fetch, track_fetch_records, track_fetch_turn, track_intake_base, turn, words, works};
pub use cratarr_requests::app::{CollectionService, CollectionView, FollowTarget, FreshIds, InsightsService, Library, LidarrView, NamingView, PlayEvent, PlayOnCommand, Players, Plays, QueueTrack, SettingsService, SettingsView, SourceTab, SourceView, TargetChoice, ToggleError, VERSION, plain, players};
pub use cratarr_playing::app::{around, crates, crates_from_radio, crates_insights, crates_served, device_covers, device_index, home, kodi, play, playing, playing_host, playlists, playlists_settings, playon, playon_players, playon_served, queue, queue_requests, radio, similar, together, visuals};
pub use cratarr_playing::app::{AlbumMore, AlbumRef, AlbumView, ArtistView, ByName, CrateLook, CrateOrder, CratePage, CrateService, CrateView, Digging, FitSummary, FitView, FromQueue, LikeCurve, MomentView, NotArtist, OpsAnswer, PlayOn, PlayOnSettings, PlayOnSettingsView, PlayResult, PlayerReport, PlayingService, PlaylistOps, PlaylistRow, PlaylistService, PlaylistStats, QueueTracks, StateView, TrackStateView, whose_players};

pub mod albums;
pub mod beat_this;
pub mod analysis_pass;
pub mod analysis_import;
pub mod length_fixes;
pub mod incoming;
pub mod incoming_served;
pub mod incoming_host;
pub mod harvest;
pub mod fixes;
pub mod fix_plans;
#[cfg(test)]
mod backup_list_tests;
pub mod file_facts;
pub mod file_runs;
pub mod incomplete;
// Bulk intake of missing tracks (0.13, decision 0080 amended).
pub mod step_018;
#[cfg(test)]
mod runner_tests;
pub mod track_intake;
#[cfg(test)]
mod u1_tests_incoming;
pub mod covers;
pub mod renumber;
pub mod incoming_settings;

pub use albums::{AlbumsView, ReplaceAsk};
pub use incoming::{AlbumFate, CopySheet, DeleteResult, IncomingService, IncomingView, JobView, KeepPreview, KeepResult, ReplacePreview, ReplaceSheet, shown_jobs as incoming_jobs_shown};
