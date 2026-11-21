//! The app's crates (SP3-SCAN.md): the crate service, over the playlists. The app's other parts are below it and above it.

pub use cratarr_playlists::app::{albums_base, along_base, analysis_pass_base, around_records, artists, audiomuse, backup_list, base_host, beyond, changes, commands, classical, collection, cover_cache, describe, device_index_base, discogs, discogs_facts, discover, editions, error, excluded_artists, file_facts_base, file_runs_base, file_words, first_start, fixes_base, follow, health_base, health_findings, history, import, incoming_base, incomplete_base, insights, insights_records, intake_card_base, labels, lastfm, library, listenbrainz, listening, listening_base, matching_base, moment, musicbrainz, originals, playlists, playlists_base, playlists_settings, playon_base, plays, queue_base, queues_listening, radio_base, reach, records_host, references, references_turn, related, renumber_base, replace_base, requests_base, requests_turn, resume, retention_base, retention_protected, runtime_base, scrobble, search_base, settings, settings_base, shelf, soulseek_base, switches, sync, taste, tempo, track_fetch_records, track_fetch_turn, track_intake_base, turn, words, works};
pub use cratarr_playlists::app::{ByName, CollectionService, CollectionView, FollowTarget, FreshIds, InsightsService, Library, LidarrView, NamingView, OpsAnswer, PlayEvent, PlayOnCommand, PlayResult, Players, PlaylistOps, PlaylistRow, PlaylistService, PlaylistStats, Plays, QueueTrack, SettingsService, SettingsView, SourceTab, SourceView, TargetChoice, ToggleError, VERSION, plain, players};

pub mod crates;
pub mod crates_from_radio;
pub mod crates_insights;
pub mod crates_served;

pub use crates::{CrateLook, CrateOrder, CratePage, CrateService, CrateView, Digging, FitSummary, FitView, LikeCurve, MomentView, NotArtist};
