//! Working out what is playing. Jellyfin's session record can be stale with some clients (observed: it kept an
//! old track as "paused at 0 s" while the activity log correctly said a new track had started), so the
//! activity log is the primary source: the newest audio playback that has not been stopped.

use serde::Serialize;

use crate::jellyfin::{ActivityEntry, Session};

const START: &str = "AudioPlayback";
const STOP: &str = "AudioPlaybackStopped";

/// A track the activity log says is playing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    pub item_id: String,
    /// When playback started, seconds since the Unix epoch.
    pub at: i64,
}

/// The track that is currently playing according to the activity log, if any.
/// `None` also means "the log says everything has stopped"; callers decide whether to fall back. (The source reads
/// every one playing, `playing_from_activity`; this is the tests'.)
#[cfg(test)]
pub fn current_from_activity(entries: &[ActivityEntry], now_epoch: i64, max_age_secs: i64) -> Option<Started> {
    playing_from_activity(entries, now_epoch, max_age_secs).into_iter().next()
}

/// Every track the log says is playing (started, not stopped, not too old), newest first. Several players can play
/// at once; the caller picks one.
pub fn playing_from_activity(entries: &[ActivityEntry], now_epoch: i64, max_age_secs: i64) -> Vec<Started> {
    let mut starts: Vec<&ActivityEntry> = entries.iter().filter(|e| e.kind == START && e.item_id.is_some()).collect();
    starts.sort_by(|a, b| b.date.cmp(&a.date)); // ISO-8601 UTC strings sort chronologically
    let mut out: Vec<Started> = Vec::new();
    for s in starts {
        let stopped = entries.iter().any(|e| e.kind == STOP && e.item_id == s.item_id && e.date >= s.date);
        let (Some(item_id), Some(at)) = (s.item_id.clone(), parse_iso_epoch(&s.date)) else { continue };
        if stopped || now_epoch - at > max_age_secs || out.iter().any(|o| o.item_id == item_id) {
            continue;
        }
        out.push(Started { item_id, at });
    }
    out
}

/// How far into the playing track we are.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Progress {
    pub position_secs: f64,
    pub duration_secs: Option<f64>,
    pub paused: bool,
    /// True when no player session reports this track, so the position is "time since it started" from the
    /// activity log: right unless it was paused or seeked.
    pub estimated: bool,
    /// Tape rewind under way on the player (0.9.10): the position goes back to the start as reported, not run on.
    pub rewinding: bool,
    /// Asked to play from rest and not sounding yet (0.9.14): the page's waveform stays grey until it sounds.
    pub starting: bool,
}

const TICKS_PER_SEC: f64 = 10_000_000.0;

/// Position of `item` (with its run time in ticks): from a session playing that exact item (the preferred player
/// first). Estimated from the activity log's start time only when no session reports any music at all (a player
/// that does not report itself). When a session reports a DIFFERENT track, the sessions and the log disagree (a
/// session lagging after a track change, or a stale record): no position rather than a guess that jumps when they
/// catch up (2026-10-12: Finamp's reporting session lagged the log by seconds and the bar jumped back and forth).
/// An estimate past the end of the track is no position either: the track has ended, whatever the log says.
pub fn progress(
    item_id: &str,
    run_time_ticks: Option<i64>,
    sessions: &[Session],
    prefers: impl Fn(&Session) -> bool,
    started_at: Option<i64>,
    now_epoch: i64,
) -> Option<Progress> {
    let duration_secs = run_time_ticks.filter(|t| *t > 0).map(|t| t as f64 / TICKS_PER_SEC);
    let clamp = |p: f64| duration_secs.map_or(p, |d| p.min(d)).max(0.0);
    let reporting = |s: &&Session| {
        s.now_playing_item.as_ref().is_some_and(|i| i.id == item_id)
            && s.play_state.as_ref().is_some_and(|p| p.position_ticks.is_some())
    };
    let session = sessions
        .iter()
        .filter(reporting)
        .find(|s| prefers(s))
        .or_else(|| sessions.iter().find(reporting));
    if let Some(ps) = session.and_then(|s| s.play_state.as_ref()) {
        return Some(Progress {
            position_secs: clamp(ps.position_ticks.unwrap_or(0) as f64 / TICKS_PER_SEC),
            duration_secs,
            paused: ps.is_paused.unwrap_or(false),
            estimated: false,
            rewinding: false,
            starting: false,
        });
    }
    let other_music = sessions.iter().any(|s| {
        s.now_playing_item.as_ref().is_some_and(|i| i.id != item_id && i.media_type.as_deref().is_none_or(|m| m.eq_ignore_ascii_case("Audio")))
    });
    if other_music {
        return None;
    }
    let at = started_at?;
    let elapsed = (now_epoch - at) as f64;
    if duration_secs.is_some_and(|d| elapsed > d) {
        return None;
    }
    Some(Progress { position_secs: clamp(elapsed), duration_secs, paused: false, estimated: true, rewinding: false, starting: false })
}

/// True when the log has any audio playback entries at all (so its verdict, even "stopped", is meaningful).
pub fn has_audio_history(entries: &[ActivityEntry]) -> bool {
    entries.iter().any(|e| e.kind == START || e.kind == STOP)
}

/// Seconds since the Unix epoch for `YYYY-MM-DDTHH:MM:SS[.fraction]Z`; `None` if it does not parse.
pub fn parse_iso_epoch(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || (b[10] != b'T' && b[10] != b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (hh, mm, ss) = (n(11..13)?, n(14..16)?, n(17..19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    // days from civil (Howard Hinnant's algorithm)
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3_600 + mm * 60 + ss)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = include_str!("../tests/fixtures/activity.json");

    fn entries() -> Vec<ActivityEntry> {
        let page: crate::jellyfin::ItemsPage<ActivityEntry> = serde_json::from_str(LOG).unwrap();
        page.items
    }

    fn id(s: Option<Started>) -> Option<String> {
        s.map(|s| s.item_id)
    }

    fn e(date: &str, kind: &str, item: &str) -> ActivityEntry {
        ActivityEntry { date: date.into(), kind: kind.into(), item_id: Some(item.into()) }
    }

    #[test]
    fn iso_dates_parse_to_epoch_seconds() {
        assert_eq!(parse_iso_epoch("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso_epoch("2000-03-01T00:00:00.1234567Z"), Some(951_868_800));
        assert_eq!(parse_iso_epoch("2026-10-12T13:28:00.5Z"), Some(1_791_811_680));
        assert_eq!(parse_iso_epoch("not a date"), None);
        assert_eq!(parse_iso_epoch("2026-13-12T13:28:00Z"), None);
    }

    #[test]
    fn newest_unstopped_start_is_the_current_track() {
        let now = parse_iso_epoch("2026-10-12T13:29:00Z").unwrap();
        assert_eq!(id(current_from_activity(&entries(), now, 3 * 3600)).as_deref(), Some("item-e"));
    }

    #[test]
    fn a_start_and_a_stop_of_other_items_in_the_same_second_do_not_confuse_it() {
        let log = vec![
            e("2026-10-12T13:21:00.9Z", "AudioPlayback", "new"),
            e("2026-10-12T13:21:00.9Z", "AudioPlaybackStopped", "old"),
            e("2026-10-12T13:12:58.1Z", "AudioPlayback", "old"),
        ];
        let now = parse_iso_epoch("2026-10-12T13:22:00Z").unwrap();
        assert_eq!(id(current_from_activity(&log, now, 3600)).as_deref(), Some("new"));
    }

    #[test]
    fn a_stopped_track_is_not_current() {
        let log = vec![e("2026-10-12T13:22:16Z", "AudioPlaybackStopped", "a"), e("2026-10-12T13:21:00Z", "AudioPlayback", "a")];
        let now = parse_iso_epoch("2026-10-12T13:23:00Z").unwrap();
        assert_eq!(current_from_activity(&log, now, 3600), None);
    }

    #[test]
    fn very_old_starts_are_ignored() {
        let log = vec![e("2026-10-12T08:00:00Z", "AudioPlayback", "a")];
        let now = parse_iso_epoch("2026-10-12T13:00:00Z").unwrap();
        assert_eq!(current_from_activity(&log, now, 3 * 3600), None);
    }

    #[test]
    fn other_activity_types_are_ignored() {
        let log = vec![e("2026-10-12T13:28:00Z", "VideoPlayback", "v"), e("2026-10-12T13:00:00Z", "AudioPlayback", "a")];
        let now = parse_iso_epoch("2026-10-12T13:29:00Z").unwrap();
        assert_eq!(id(current_from_activity(&log, now, 3600)).as_deref(), Some("a"));
        assert!(has_audio_history(&log));
        assert!(!has_audio_history(&[e("2026-10-12T13:28:00Z", "SessionStarted", "x")]));
    }

    #[test]
    fn the_start_time_comes_with_the_current_track() {
        let log = vec![e("2026-10-12T13:21:00.9Z", "AudioPlayback", "a")];
        let now = parse_iso_epoch("2026-10-12T13:22:00Z").unwrap();
        let at = parse_iso_epoch("2026-10-12T13:21:00Z").unwrap();
        assert_eq!(current_from_activity(&log, now, 3600), Some(Started { item_id: "a".into(), at }));
    }

    fn sess(client: &str, item: &str, ticks: i64, paused: bool) -> Session {
        let json = format!(
            r#"{{"Client":"{client}","NowPlayingItem":{{"Id":"{item}","Name":"x","MediaType":"Audio"}},
               "PlayState":{{"PositionTicks":{ticks},"IsPaused":{paused}}}}}"#
        );
        serde_json::from_str(&json).unwrap()
    }

    fn finamp(s: &Session) -> bool {
        s.client.as_deref().is_some_and(|c| c.eq_ignore_ascii_case("Finamp"))
    }

    #[test]
    fn every_playing_track_is_listed_newest_first() {
        let log = vec![
            e("2026-10-12T13:20:00Z", "AudioPlayback", "tablet"),
            e("2026-10-12T13:21:00Z", "AudioPlayback", "phone"),
            e("2026-10-12T13:19:00Z", "AudioPlayback", "stopped"),
            e("2026-10-12T13:19:30Z", "AudioPlaybackStopped", "stopped"),
        ];
        let now = parse_iso_epoch("2026-10-12T13:22:00Z").unwrap();
        let ids: Vec<String> = playing_from_activity(&log, now, 3600).into_iter().map(|s| s.item_id).collect();
        assert_eq!(ids, vec!["phone", "tablet"]);
    }

    #[test]
    fn the_matching_session_gives_the_position() {
        let s = vec![sess("Web", "other", 50_000_000, false), sess("Finamp", "a", 1_234_000_000, true)];
        let p = progress("a", Some(3_240_000_000), &s, finamp, Some(0), 1_000).unwrap();
        assert_eq!(p, Progress { position_secs: 123.4, duration_secs: Some(324.0), paused: true, estimated: false, rewinding: false, starting: false });
    }

    #[test]
    fn the_preferred_client_wins_when_two_sessions_play_the_same_track() {
        let s = vec![sess("Web", "a", 10_000_000, false), sess("finamp", "a", 20_000_000, false)];
        assert_eq!(progress("a", None, &s, finamp, None, 0).unwrap().position_secs, 2.0);
    }

    #[test]
    fn a_session_on_another_track_means_no_position_not_a_guess() {
        let s = vec![sess("Finamp", "old", 0, true)];
        assert_eq!(progress("new", Some(3_000_000_000), &s, finamp, Some(1_000), 1_090), None, "lagging or stale: they disagree");
    }

    #[test]
    fn with_no_session_reporting_music_the_start_time_gives_an_estimate() {
        let p = progress("a", Some(3_000_000_000), &[], finamp, Some(1_000), 1_090).unwrap();
        assert_eq!(p, Progress { position_secs: 90.0, duration_secs: Some(300.0), paused: false, estimated: true, rewinding: false, starting: false });
    }

    #[test]
    fn an_estimate_past_the_end_is_no_position() {
        assert_eq!(progress("a", Some(1_000_000_000), &[], finamp, Some(0), 500), None, "a 100 s track 500 s ago has ended");
    }

    #[test]
    fn positions_are_kept_within_the_track() {
        let p = progress("a", Some(1_000_000_000), &[], finamp, Some(0), 100).unwrap();
        assert_eq!(p.position_secs, 100.0);
        let s = vec![sess("Finamp", "a", -5, false)];
        assert_eq!(progress("a", None, &s, finamp, None, 0).unwrap().position_secs, 0.0);
    }

    #[test]
    fn no_session_and_no_start_time_means_no_progress() {
        assert_eq!(progress("a", Some(1), &[], finamp, None, 0), None);
    }
}
