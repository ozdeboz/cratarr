//! What plays, and the saved queue, as Subsonic apps see them (decision 0033, step 5): `getNowPlaying`,
//! `reportPlayback` (OpenSubsonic `playbackReport`), and `savePlayQueue` / `getPlayQueue` with their by-index forms
//! (`indexBasedQueue`).
//!
//! How others do it, read in their source on 2026-10-16:
//! - Navidrome keeps one saved queue per account, the last save winning (`persistence/playqueue_repository.go`);
//!   Ampache keeps one per account and app. Apps save often (every few seconds, on every change) and several take the
//!   server's queue in place of their own when they start, or while paused (Psysonic).
//! - Navidrome's now playing is a session per app, fed by `reportPlayback` and `scrobble?submission=false`, ending
//!   5 s after the track's end while it plays and 30 min after a pause; a report that stops past half the track or
//!   four minutes counts a play (`core/scrobbler/play_tracker.go`).
//!
//! Cratarr's own players' queues are Play On's: apps see them and never write them. What an app saves is kept as its
//! own, one per app (Ampache's way), so it gets back what it saved. The queue an app is given is the one started or
//! saved last: an app's own save, or the queue something new was last started in on one of Cratarr's players. A
//! player playing on, Radio's inserts or an edit leave its time alone, so an app that takes the server's queue does
//! not take a player's again and again.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::Serve;
use super::api::{self, Song};
use super::routes::{Answer, Failed, Params, annotate_songs, blocking, missing, not_found};
use super::users::User;
use crate::userdata::AppQueue;

/// A queue given to an app is cut to this many songs around the one that plays (Psysonic sends at most 1000; long
/// GET URLs fail).
const MOST: usize = 1000;
/// Of which this many come before the one that plays.
const BEFORE: usize = 100;
/// A paused app's session lasts this long after its last report (Navidrome's).
const PAUSED_FOR: Duration = Duration::from_secs(30 * 60);
/// A playing one this long after its track's end.
const AFTER_END: Duration = Duration::from_secs(5);
/// A play counts when it stops past half the track or this far in (Navidrome's, as Last.fm's).
const COUNTS_AFTER_MS: u64 = 240_000;

/// What the apps report playing, by account and app.
#[derive(Default)]
pub struct Sessions(Mutex<HashMap<(String, String), Session>>);

#[derive(Debug, Clone)]
struct Session {
    song: String,
    state: &'static str,
    position_ms: u64,
    rate: f64,
    duration_ms: u64,
    /// When the report came.
    at: Instant,
    /// The same, epoch seconds.
    at_epoch: i64,
}

impl Session {
    fn moving(&self) -> bool {
        self.state == "playing"
    }

    /// Where it is now: counted on from the report while it plays, never past the end.
    fn position_now(&self) -> u64 {
        if !self.moving() {
            return self.position_ms;
        }
        let gone = self.at.elapsed().as_millis() as f64 * self.rate;
        (self.position_ms + gone as u64).min(self.duration_ms.max(self.position_ms))
    }

    fn over(&self) -> bool {
        let lasts = if self.moving() && self.rate > 0.0 {
            let left = self.duration_ms.saturating_sub(self.position_ms) as f64 / self.rate;
            Duration::from_millis(left as u64) + AFTER_END
        } else {
            PAUSED_FOR
        };
        self.at.elapsed() > lasts
    }
}

impl Sessions {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), Session>> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn live(&self) -> Vec<((String, String), Session)> {
        let mut all = self.lock();
        all.retain(|_, s| !s.over());
        all.iter().map(|(k, s)| (k.clone(), s.clone())).collect()
    }
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The app a request comes from (Subsonic's `c`): every install of an app shares it.
fn client(p: &Params) -> String {
    p.get("c").unwrap_or_default().trim().to_string()
}

/// A player's number in `getNowPlaying`, the same each time (Navidrome's counts up per answer).
fn player_id(kind: &str, name: &str) -> i64 {
    // FNV-1a, 31 bits: a positive int in any client.
    let mut h: u32 = 0x811c_9dc5;
    for b in kind.bytes().chain([0]).chain(name.bytes()) {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    i64::from(h & 0x7fff_ffff)
}

/// `reportPlayback`: where an app is in a song (OpenSubsonic `playbackReport`). A stop past half the song or four
/// minutes counts a play, unless `ignoreScrobble`.
pub async fn report_playback(serve: &Arc<Serve>, user: &User, p: &Params) -> Result<Answer, Failed> {
    let song = p.get("mediaId").ok_or_else(|| missing("mediaId"))?.to_string();
    let kind = p.get("mediaType").ok_or_else(|| missing("mediaType"))?;
    if kind != "song" {
        return Err(not_found("Media"));
    }
    let position_ms = p.get("positionMs").ok_or_else(|| missing("positionMs"))?.trim().parse::<f64>().map_err(|_| missing("positionMs"))?.max(0.0) as u64;
    let state = match p.get("state").ok_or_else(|| missing("state"))? {
        "starting" => "starting",
        "playing" => "playing",
        "paused" => "paused",
        "stopped" => "stopped",
        other => return Err(Failed(0, format!("Unknown playback state: {other}"))),
    };
    let rate = p.get("playbackRate").and_then(|r| r.trim().parse::<f64>().ok()).filter(|r| r.is_finite() && *r >= 0.0).unwrap_or(1.0);
    let ignore = p.get("ignoreScrobble").is_some_and(|v| v.eq_ignore_ascii_case("true"));
    let duration_ms = song_duration_ms(serve, &song).await?.ok_or_else(|| not_found("Song"))?;
    let key = (user.name.clone(), client(p));
    if state == "stopped" {
        if !ignore && position_ms >= (duration_ms / 2).min(COUNTS_AFTER_MS) {
            count_play(serve, &user.name, &song, now()).await?;
        }
        // A late stop of the song before does not end the one that plays now.
        let mut all = serve.now_playing.lock();
        if all.get(&key).is_none_or(|s| s.song == song) {
            all.remove(&key);
        }
        return Ok(Answer::Json(json!({})));
    }
    let mut all = serve.now_playing.lock();
    // Reports can come out of order: a late `starting` does not stop the clock of the same song playing.
    if state == "starting" && all.get(&key).is_some_and(|s| s.song == song && s.moving()) {
        return Ok(Answer::Json(json!({})));
    }
    if state == "playing" || state == "starting" {
        crate::app::listening::started(&key.0, &key.1, &song);
    }
    all.insert(key, Session { song, state, position_ms, rate, duration_ms, at: Instant::now(), at_epoch: now() });
    Ok(Answer::Json(json!({})))
}

/// `scrobble` with `submission=false`, Subsonic's "now playing": the app starts the song, unless it already reports
/// that song (a `reportPlayback` session knows better where it is).
pub async fn now_playing_from_scrobble(serve: &Arc<Serve>, user: &User, p: &Params) -> Result<(), Failed> {
    let Some(song) = p.all("id").into_iter().next() else { return Ok(()) };
    let Some(duration_ms) = song_duration_ms(serve, &song).await? else { return Ok(()) };
    let key = (user.name.clone(), client(p));
    let mut all = serve.now_playing.lock();
    if all.get(&key).is_none_or(|s| s.song != song) {
        crate::app::listening::started(&key.0, &key.1, &song);
        all.insert(key, Session { song, state: "playing", position_ms: 0, rate: 1.0, duration_ms, at: Instant::now(), at_epoch: now() });
    }
    Ok(())
}

async fn song_duration_ms(serve: &Arc<Serve>, song: &str) -> Result<Option<u64>, Failed> {
    let (lib, ids) = (serve.library.clone(), vec![song.to_string()]);
    let found = blocking(move || lib.songs_by_id(&ids)).await?;
    Ok(found.first().map(|s| s.duration * 1000))
}

/// A play of a song, counted as `scrobble` counts it.
pub async fn count_play(serve: &Arc<Serve>, user: &str, song: &str, at: i64) -> Result<(), Failed> {
    Ok(record(serve, user, song, &crate::userdata::Heard::counted(at)).await?)
}

/// A song heard (decision 0064): through the one way in for listening (decision 0068).
pub async fn record(serve: &Arc<Serve>, user: &str, song: &str, heard: &crate::userdata::Heard) -> anyhow::Result<()> {
    crate::app::listening::heard(serve, user, song, heard).await.map(|_| ())
}

/// One of Cratarr's players as an app may see it.
struct Player {
    name: String,
    state: &'static str,
    playing: bool,
    seen: u64,
    position_ms: u64,
    song: Option<String>,
}

/// The asking account's players (decision 0051).
fn players(serve: &Serve) -> Vec<Player> {
    let Some(on) = serve.play_on() else { return Vec::new() };
    on.list()
        .into_iter()
        .map(|v| Player {
            state: v.state,
            playing: v.playing,
            seen: v.seen,
            position_ms: v.position_ms,
            song: v.ids.track_id.clone(),
            name: v.name,
        })
        .collect()
}

/// `getNowPlaying`: each of the account's own players that plays or is paused in a song, and each of its apps that
/// reports one (`reportPlayback`, `scrobble` with `submission=false`), the latest first. One's listening is one's own
/// (decision 0051).
pub async fn now_playing(serve: &Arc<Serve>, user: &User) -> Result<Answer, Failed> {
    struct Row {
        song: String,
        username: String,
        player_id: i64,
        player_name: String,
        state: &'static str,
        position_ms: u64,
        rate: f64,
        at: i64,
    }
    let at = now();
    let mut rows: Vec<Row> = players(serve)
        .into_iter()
        .filter(|p| p.state != "stopped")
        .filter_map(|p| {
            Some(Row {
                song: p.song?,
                username: user.name.clone(),
                player_id: player_id("player", &p.name),
                state: if p.playing { "playing" } else { "paused" },
                position_ms: p.position_ms,
                rate: 1.0,
                at: p.seen as i64,
                player_name: p.name,
            })
        })
        .collect();
    rows.extend(serve.now_playing.live().into_iter().filter(|((username, _), _)| *username == user.name).map(|((username, app), s)| Row {
        player_id: player_id("app", &format!("{username}\n{app}")),
        position_ms: s.position_now(),
        song: s.song,
        player_name: app,
        state: s.state,
        rate: s.rate,
        at: s.at_epoch,
        username,
    }));
    rows.sort_by(|a, b| b.at.cmp(&a.at));
    let songs = songs_by_id(serve, &user.name, rows.iter().map(|r| r.song.clone()).collect()).await?;
    let entries: Vec<Value> = rows
        .into_iter()
        .filter_map(|r| {
            let mut e = serde_json::to_value(songs.get(&r.song)?).ok()?;
            let o = e.as_object_mut()?;
            o.insert("username".into(), r.username.into());
            o.insert("minutesAgo".into(), ((at - r.at).max(0) / 60).into());
            o.insert("playerId".into(), r.player_id.into());
            o.insert("playerName".into(), r.player_name.into());
            o.insert("state".into(), r.state.into());
            o.insert("positionMs".into(), r.position_ms.into());
            o.insert("playbackRate".into(), r.rate.into());
            Some(e)
        })
        .collect();
    Ok(Answer::Json(json!({ "nowPlaying": { "entry": entries } })))
}

/// Songs by id with the account's stars and plays, each once.
async fn songs_by_id(serve: &Arc<Serve>, user: &str, mut ids: Vec<String>) -> Result<HashMap<String, Song>, Failed> {
    ids.sort();
    ids.dedup();
    let lib = serve.library.clone();
    let mut songs = blocking(move || lib.songs_by_id(&ids)).await?;
    annotate_songs(serve, user, &mut songs)?;
    Ok(songs.into_iter().map(|s| (s.id.clone(), s)).collect())
}

/// `savePlayQueue` and `savePlayQueueByIndex`: the app's own queue, in place of the one it saved before. Without songs
/// it is cleared. Cratarr's players' queues are never touched.
pub fn save(serve: &Serve, user: &User, p: &Params, by_index: bool) -> Result<Answer, Failed> {
    // One `id` per song; some apps send them in one, separated by commas (Castafiore).
    let songs: Vec<String> =
        p.all("id").iter().flat_map(|v| v.split(',')).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
    let current = if songs.is_empty() {
        None
    } else if by_index {
        let i = p.get("currentIndex").ok_or_else(|| missing("currentIndex"))?.trim().parse::<usize>().ok();
        Some(i.filter(|i| *i < songs.len()).ok_or_else(|| Failed(10, "currentIndex is not a place in the queue".into()))?)
    } else {
        // The first copy of the song: all an id can say. Without one, the first song.
        Some(p.get("current").and_then(|c| songs.iter().position(|s| s == c)).unwrap_or(0))
    };
    // Milliseconds; some apps send a fraction.
    let position_ms = p.get("position").and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).map_or(0, |v| v.max(0.0) as u64);
    serve.userdata.save_app_queue(&user.name, &AppQueue { client: client(p), songs, current, position_ms, changed: now() })?;
    Ok(Answer::Json(json!({})))
}

/// The queue an app is given: the one started or saved last.
struct Given {
    songs: Vec<String>,
    current: Option<usize>,
    position_ms: u64,
    changed: i64,
    by: String,
}

/// The queue something new was last started in on one of `whose` players (`whose_players`), with where it is now.
fn latest_player_queue(on: Option<Arc<crate::app::PlayOn>>, catalogue: &crate::catalogue::Catalogue, whose: Option<&str>) -> Option<Given> {
    let on = on?;
    let newest = on.list_for(whose).into_iter().filter(|v| v.queue_started > 0 && v.queue_length > 0).max_by_key(|v| v.queue_started)?;
    let v = on.get(&newest.name)?;
    let tracks = v.queue.unwrap_or_default();
    player_given(v.name, v.queue_started, v.index, v.position_ms, &tracks, |t| {
        catalogue.track_of_source(&t.source, &t.id).map(super::ids::song)
    })
}

/// A player's queue as an app is given it: the tracks the library serves (`song_of`), the place of the one that plays
/// counted among them. When that one is not served, the queue starts at its first song.
fn player_given(
    name: String,
    started: u64,
    index: i64,
    position_ms: u64,
    tracks: &[crate::app::QueueTrack],
    song_of: impl Fn(&crate::app::QueueTrack) -> Option<String>,
) -> Option<Given> {
    let (mut songs, mut current) = (Vec::new(), None);
    for (i, t) in tracks.iter().enumerate() {
        let Some(song) = song_of(t) else { continue };
        if i as i64 == index {
            current = Some(songs.len());
        }
        songs.push(song);
    }
    if songs.is_empty() {
        return None;
    }
    Some(Given {
        position_ms: if current.is_some() { position_ms } else { 0 },
        current: current.or(Some(0)),
        songs,
        changed: started as i64,
        by: name,
    })
}

/// The part of a long queue given (start, end): `MOST` songs, `BEFORE` of them ahead of the one that plays.
fn window(len: usize, current: usize) -> (usize, usize) {
    if len <= MOST {
        return (0, len);
    }
    let from = current.saturating_sub(BEFORE).min(len - MOST);
    (from, from + MOST)
}

/// `getPlayQueue` and `getPlayQueueByIndex`.
pub async fn get(serve: &Arc<Serve>, user: &User, by_index: bool) -> Result<Answer, Failed> {
    let saved = serve.userdata.app_queues(&user.name)?.into_iter().map(|q| Given {
        songs: q.songs,
        current: q.current,
        position_ms: q.position_ms,
        changed: q.changed,
        by: q.client,
    });
    let (catalogue, whose, on) = (serve.catalogue().clone(), crate::app::whose_players(), serve.play_on().cloned());
    let player = tokio::task::spawn_blocking(move || latest_player_queue(on, &catalogue, whose.as_deref())).await.map_err(|e| Failed(0, e.to_string()))?;
    let Some(mut q) = saved.chain(player).max_by_key(|q| q.changed) else {
        return Ok(Answer::Json(json!({})));
    };
    // A long queue (a radio, a Kodi's) is cut around the song that plays.
    let at = q.current.unwrap_or(0).min(q.songs.len().saturating_sub(1));
    let (from, to) = window(q.songs.len(), at);
    if (from, to) != (0, q.songs.len()) {
        q.songs = q.songs[from..to].to_vec();
        q.current = Some(at - from);
    }
    let found = songs_by_id(serve, &user.name, q.songs.clone()).await?;
    // Songs gone from the library since are left out, and the place of the one that plays moves with them.
    let (mut entries, mut current) = (Vec::new(), None);
    for (i, id) in q.songs.iter().enumerate() {
        let Some(song) = found.get(id) else { continue };
        if Some(i) == q.current {
            current = Some(entries.len());
        }
        entries.push(song.clone());
    }
    if entries.is_empty() {
        return Ok(Answer::Json(json!({})));
    }
    let (current, position) = match current {
        Some(c) => (c, q.position_ms),
        None => (0, 0),
    };
    let mut body = json!({ "username": user.name, "changed": api::iso(q.changed), "changedBy": q.by, "position": position });
    if by_index {
        body["currentIndex"] = current.into();
        body["entry"] = serde_json::to_value(&entries)?;
        Ok(Answer::Json(json!({ "playQueueByIndex": body })))
    } else {
        body["current"] = entries[current].id.clone().into();
        body["entry"] = serde_json::to_value(&entries)?;
        Ok(Answer::Json(json!({ "playQueue": body })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_players_number_stays_the_same() {
        assert_eq!(player_id("player", "Bedroom"), player_id("player", "Bedroom"));
        assert_ne!(player_id("player", "Bedroom"), player_id("app", "Bedroom"));
        assert!(player_id("app", "me\nFeishin") >= 0);
    }

    #[test]
    fn a_players_queue_leaves_out_what_the_library_does_not_serve() {
        let t = |id: &str| crate::app::QueueTrack { source: "cratarr".into(), id: id.into(), ..Default::default() };
        let tracks = [t("tr-1"), t("gone"), t("tr-2"), t("tr-1")];
        let served = |x: &crate::app::QueueTrack| (x.id != "gone").then(|| x.id.clone());
        let q = player_given("Bedroom".into(), 7, 3, 1500, &tracks, served).unwrap();
        assert_eq!((q.songs.join(" "), q.current, q.position_ms), ("tr-1 tr-2 tr-1".into(), Some(2), 1500), "the second copy plays");
        assert_eq!((q.changed, q.by.as_str()), (7, "Bedroom"));
        let q = player_given("Bedroom".into(), 7, 1, 1500, &tracks, served).unwrap();
        assert_eq!((q.current, q.position_ms), (Some(0), 0), "the one that plays is gone: from the start");
        assert!(player_given("Bedroom".into(), 7, 0, 0, &[t("gone")], served).is_none());
    }

    #[test]
    fn a_long_queue_is_cut_around_the_song_that_plays() {
        assert_eq!(window(30, 20), (0, 30));
        assert_eq!(window(5000, 20), (0, MOST), "near the start");
        assert_eq!(window(5000, 2000), (2000 - BEFORE, 2000 - BEFORE + MOST));
        assert_eq!(window(5000, 4990), (5000 - MOST, 5000), "near the end");
    }

    #[test]
    fn a_session_moves_while_it_plays_and_ends_after_its_song() {
        let s = |state, at: Instant| Session { song: "tr-1".into(), state, position_ms: 1_000, rate: 2.0, duration_ms: 10_000, at, at_epoch: 0 };
        let then = Instant::now() - Duration::from_millis(500);
        assert!((1_900..2_200).contains(&s("playing", then).position_now()), "twice as fast");
        assert_eq!(s("paused", then).position_now(), 1_000);
        assert!(!s("playing", then).over());
        let long_ago = Instant::now() - Duration::from_secs(20);
        assert_eq!(s("playing", long_ago).position_now(), 10_000, "never past the end");
        assert!(s("playing", long_ago).over(), "4.5 s left at twice the speed, and 5 s after");
        assert!(!s("paused", long_ago).over(), "a pause lasts half an hour");
    }
}
