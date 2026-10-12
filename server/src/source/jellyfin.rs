//! Jellyfin as a source. What is playing comes from the activity log first (session records can be stale with some
//! clients, see `nowplaying.rs`), the position from the session playing that track. Library membership comes from
//! the track's ancestors, never from paths. No Jellyfin user is configured; the one call that needs a user context
//! (which library a track is in) uses an administrator, found once (decision 0005).

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use anyhow::{Context, Result, bail};

use super::{Choice, Field, FieldKind, Fut, Kind, Library, Playing, PlaylistChange, PlaylistEvent, Source, Status, Track, Values, text};
use crate::jellyfin::{AlbumSummary, Device, Jellyfin, PlayingItem, Session, pick_now_playing};
use crate::nowplaying::{Started, has_audio_history, playing_from_activity, progress};

pub const KIND: Kind = Kind { id: "jellyfin", label: "Jellyfin", fields, connect };

/// A track that started longer ago than this without a stop entry is assumed to be a stale log line.
const MAX_PLAYING_AGE_SECS: i64 = 3 * 3600;

fn fields() -> Vec<Field> {
    vec![
        Field { key: "url", label: "Server address", kind: FieldKind::Url, required: true, help: None, bound_to: &[] },
        Field {
            key: "api_key",
            label: "API key",
            kind: FieldKind::Secret,
            required: true,
            help: Some("Made in Jellyfin under Dashboard, API keys."),
            bound_to: &["url"],
        },
        Field {
            key: "player",
            label: "Follow player",
            kind: FieldKind::Choice,
            required: false,
            help: Some("When several players play music at once, the page follows this one."),
            bound_to: &[],
        },
    ]
}

fn connect(values: &Values, partial: bool) -> Result<Box<dyn Source>> {
    let url = text(values, "url").context("the server address is required")?.trim_end_matches('/');
    if !url.starts_with("http://") && !url.starts_with("https://") {
        bail!("the server address must start with http:// or https://");
    }
    let key = text(values, "api_key").context("the API key is required")?;
    let _ = partial; // nothing here waits on a choice
    Ok(Box::new(JellyfinSource {
        jf: Jellyfin::new(url, key)?,
        admin: tokio::sync::OnceCell::new(),
        player: Player::parse(text(values, "player").unwrap_or("")),
        items: Mutex::new(HashMap::new()),
        summaries: Mutex::new(HashMap::new()),
    }))
}

/// The player to follow: an app, optionally on one device. Empty = any player (no preference).
/// Stored as `App` or `App::Device`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Player {
    app: Option<String>,
    device: Option<String>,
}

const SEP: &str = "::";

impl Player {
    pub fn parse(v: &str) -> Self {
        let (app, device) = v.split_once(SEP).unwrap_or((v, ""));
        let clean = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
        let app = clean(app);
        Self { device: app.as_ref().and(clean(device)), app }
    }

    pub fn value(&self) -> String {
        match (&self.app, &self.device) {
            (Some(a), Some(d)) => format!("{a}{SEP}{d}"),
            (Some(a), None) => a.clone(),
            _ => String::new(),
        }
    }

    /// App names are compared loosely ("Jellyfin for Android" is "Jellyfin Android"); device names exactly, ignoring
    /// case. No preference matches nothing, so the newest track wins.
    pub fn matches(&self, client: Option<&str>, device: Option<&str>) -> bool {
        let Some(app) = &self.app else { return false };
        client.is_some_and(|c| loose(c) == loose(app))
            && self.device.as_ref().is_none_or(|d| device.is_some_and(|x| x.trim().eq_ignore_ascii_case(d)))
    }
}

fn loose(app: &str) -> String {
    app.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !w.eq_ignore_ascii_case("for"))
        .map(str::to_lowercase)
        .collect()
}

/// The chosen player's track when it is one of those playing, else the newest.
pub fn choose<'a>(playing: &'a [Started], sessions: &[Session], player: &Player) -> Option<&'a Started> {
    sessions
        .iter()
        .filter(|s| player.matches(s.client.as_deref(), s.device_name.as_deref()))
        .filter_map(|s| s.now_playing_item.as_ref())
        .find_map(|i| playing.iter().find(|p| p.item_id == i.id))
        .or_else(|| playing.first())
}

/// The sessions to send music to: every session of one device. The device is the chosen player (its most recently
/// active device when only an app is chosen); with no player chosen, the device playing music now. Empty rather
/// than a guess.
///
/// Every session of the device, not one: Finamp 1.0.x registers two per device (observed on 10.11). The one that
/// reports playback lists all its commands but holds no connection and ignores commands; a second one ("Play On")
/// lists no commands at all yet is the one that plays what it is sent. Both report `SupportsRemoteControl: false`,
/// and Jellyfin answers 204 to both, so neither the flags nor the answer tell them apart.
pub fn cast_targets<'a>(sessions: &'a [Session], player: &Player) -> Vec<&'a Session> {
    device_sessions(sessions, player, false)
}

/// The sessions the player controls act on (and the pill names): as `cast_targets`, except that with no player chosen
/// a PAUSED device counts too, or play/pause could never resume what it paused.
pub fn control_targets<'a>(sessions: &'a [Session], player: &Player) -> Vec<&'a Session> {
    device_sessions(sessions, player, true)
}

/// "Finamp on Lenovo Tab": the wording play and the pill share.
pub fn session_label(s: &Session) -> String {
    [s.client.as_deref(), s.device_name.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" on ")
}

fn device_sessions<'a>(sessions: &'a [Session], player: &Player, paused_counts: bool) -> Vec<&'a Session> {
    // ISO dates sort as text; the most recently active session decides the device.
    let newest = |a: &&Session, b: &&Session| a.last_activity_date.cmp(&b.last_activity_date);
    let with_id = || sessions.iter().filter(|s| s.id.is_some());
    let anchor = if player.app.is_some() {
        with_id().filter(|s| player.matches(s.client.as_deref(), s.device_name.as_deref())).max_by(newest)
    } else {
        // A session record can keep a finished track, so "not paused" is required as well.
        with_id()
            .filter(|s| s.now_playing_item.as_ref().is_some_and(|i| i.media_type.as_deref().is_some_and(|m| m.eq_ignore_ascii_case("Audio"))))
            .filter(|s| paused_counts || s.play_state.as_ref().and_then(|p| p.is_paused) == Some(false))
            .max_by(newest)
    };
    let Some(anchor) = anchor else { return Vec::new() };
    let same_app = |s: &&Session| matches!((&s.client, &anchor.client), (Some(a), Some(b)) if loose(a) == loose(b));
    let same_device =
        |s: &&Session| matches!((&s.device_name, &anchor.device_name), (Some(a), Some(b)) if a.trim().eq_ignore_ascii_case(b.trim()));
    with_id().filter(same_app).filter(same_device).collect()
}

/// "Any player", then per app (most recently seen first) the app on any device and on each device it was seen on.
/// Sessions count as seen now; remembered devices by their last activity.
pub fn player_choices(sessions: &[Session], devices: &[Device]) -> Vec<Choice> {
    let mut seen: Vec<(String, String, String)> = Vec::new(); // (app, device, last activity)
    let now = "9999".to_string();
    let rows = sessions
        .iter()
        .map(|s| (s.client.clone(), s.device_name.clone(), now.clone()))
        .chain(devices.iter().map(|d| (d.app_name.clone(), d.name.clone(), d.date_last_activity.clone().unwrap_or_default())));
    for (app, device, last) in rows {
        let (Some(app), Some(device)) = (app, device) else { continue };
        let (app, device) = (app.trim().to_string(), device.trim().to_string());
        if app.is_empty() || device.is_empty() {
            continue;
        }
        match seen.iter_mut().find(|(a, d, _)| loose(a) == loose(&app) && d.eq_ignore_ascii_case(&device)) {
            Some(row) if row.2 < last => row.2 = last,
            Some(_) => {}
            None => seen.push((app, device, last)),
        }
    }
    seen.sort_by(|a, b| b.2.cmp(&a.2));
    let mut apps: Vec<String> = Vec::new();
    for (a, _, _) in &seen {
        if !apps.iter().any(|x| loose(x) == loose(a)) {
            apps.push(a.clone());
        }
    }
    let mut out = vec![Choice { value: String::new(), label: "Any player".into() }];
    for app in apps {
        out.push(Choice { value: app.clone(), label: format!("{app}, any device") });
        for (a, d, _) in seen.iter().filter(|(a, _, _)| loose(a) == loose(&app)) {
            let p = Player { app: Some(a.clone()), device: Some(d.clone()) };
            out.push(Choice { value: p.value(), label: format!("{a} on {d}") });
        }
    }
    out
}

fn epoch_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

struct JellyfinSource {
    jf: Jellyfin,
    player: Player,
    /// See `Jellyfin::admin_user`.
    admin: tokio::sync::OnceCell<String>,
    /// Recently seen tracks with their library, so polling the playing track stays cheap.
    items: Mutex<HashMap<String, (PlayingItem, Option<String>)>>,
    /// Per album: its last-saved stamp and what its tracks add up to; read again only when the stamp changes.
    summaries: Mutex<HashMap<String, (Option<String>, AlbumSummary)>>,
}

/// How many albums' tracks are read at once when many albums are new (first look at a library).
const SUMMARY_READERS: usize = 4;

impl JellyfinSource {
    async fn item(&self, id: &str) -> Result<(PlayingItem, Option<String>)> {
        if let Some(hit) = self.items.lock().unwrap_or_else(|p| p.into_inner()).get(id) {
            return Ok(hit.clone());
        }
        let library = async {
            let admin = self.admin.get_or_try_init(|| self.jf.admin_user()).await?;
            self.jf.library_of(id, admin).await
        };
        let (item, library) = tokio::join!(self.jf.item(id), library);
        let library = library.unwrap_or_else(|e| {
            tracing::warn!("cannot tell which library {id} is in: {e:#}");
            None
        });
        let got = (item?, library);
        let mut cache = self.items.lock().unwrap_or_else(|p| p.into_inner());
        if cache.len() > 256 {
            cache.clear();
        }
        cache.insert(id.to_string(), got.clone());
        Ok(got)
    }

    fn prefers(&self) -> impl Fn(&Session) -> bool + '_ {
        |s| self.player.matches(s.client.as_deref(), s.device_name.as_deref())
    }
}

fn to_track((i, library_id): (PlayingItem, Option<String>)) -> Track {
    Track { id: i.id, title: i.name, artist: i.album_artist, album: i.album, album_id: i.album_id, path: i.path, library_id }
}

impl Source for JellyfinSource {
    fn status(&self) -> Fut<'_, Status> {
        Box::pin(async move {
            // The public call says whether the server answers at all; the authenticated one whether the key works.
            if let Err(e) = self.jf.public_info().await {
                return Ok(Status { error: Some(format!("cannot reach Jellyfin: {e:#}")), ..Status::default() });
            }
            Ok(match self.jf.system_info().await {
                Ok(i) => Status { connected: true, server_name: i.server_name, version: Some(i.version), ..Status::default() },
                Err(e) => Status { error: Some(format!("Jellyfin refused the API key: {e:#}")), ..Status::default() },
            })
        })
    }

    fn options(&self) -> Fut<'_, BTreeMap<String, Vec<Choice>>> {
        Box::pin(async move {
            let (devices, sessions) = tokio::join!(self.jf.devices(), self.jf.sessions());
            let devices = devices.context("Jellyfin refused the API key")?;
            let mut out = BTreeMap::new();
            out.insert("player".into(), player_choices(&sessions.unwrap_or_default(), &devices));
            Ok(out)
        })
    }

    fn libraries(&self) -> Fut<'_, Vec<Library>> {
        Box::pin(async move {
            Ok(self
                .jf
                .virtual_folders()
                .await?
                .into_iter()
                .filter(|f| f.collection_type.as_deref() == Some("music"))
                .map(|f| Library { id: f.item_id, name: f.name })
                .collect())
        })
    }

    fn sample_paths<'a>(&'a self, library_id: &'a str, n: usize) -> Fut<'a, Vec<String>> {
        Box::pin(self.jf.sample_paths(library_id, n))
    }

    fn now_playing(&self) -> Fut<'_, Option<Playing>> {
        Box::pin(async move {
            let (activity, sessions) = tokio::join!(self.jf.activity(100), self.jf.sessions());
            let sessions = sessions.unwrap_or_else(|e| {
                tracing::warn!("sessions unavailable, estimating the position: {e:#}");
                Vec::new()
            });
            let now = epoch_now();
            let chosen = match activity {
                Ok(entries) if has_audio_history(&entries) => {
                    let playing = playing_from_activity(&entries, now, MAX_PLAYING_AGE_SECS);
                    match choose(&playing, &sessions, &self.player) {
                        Some(s) => (s.item_id.clone(), Some(s.at)),
                        None => return Ok(None),
                    }
                }
                other => {
                    if let Err(e) = other {
                        tracing::warn!("activity log unavailable, falling back to sessions: {e:#}");
                    }
                    match pick_now_playing(&sessions, self.prefers()) {
                        Some(i) => (i.id, None),
                        None => return Ok(None),
                    }
                }
            };
            let (id, started_at) = chosen;
            let found = self.item(&id).await?;
            let progress = progress(&found.0.id, found.0.run_time_ticks, &sessions, self.prefers(), started_at, now);
            let player = control_targets(&sessions, &self.player).first().map(|s| session_label(s));
            Ok(Some(Playing { track: to_track(found), progress, started_at, player }))
        })
    }

    fn track<'a>(&'a self, id: &'a str) -> Fut<'a, Track> {
        Box::pin(async move { Ok(to_track(self.item(id).await?)) })
    }

    fn playlists_changed(&self, events: Vec<PlaylistEvent>) -> Fut<'_, ()> {
        Box::pin(async move {
            // New and removed files: Jellyfin's Music library does not watch the folder, so tell it.
            let files: Vec<(String, &str)> = events
                .iter()
                .filter_map(|e| {
                    let kind = match e.change {
                        PlaylistChange::Created => "Created",
                        PlaylistChange::Deleted => "Deleted",
                        PlaylistChange::Edited => return None,
                    };
                    e.path.clone().map(|p| (p, kind))
                })
                .collect();
            if !files.is_empty() {
                if let Err(e) = self.jf.library_changed(&files).await {
                    tracing::warn!("could not tell Jellyfin about the playlist file change: {e:#}");
                }
            }
            // Edited files: ask Jellyfin to re-read each one (see `Jellyfin::refresh_playlist`).
            for e in events.iter().filter(|e| e.change == PlaylistChange::Edited) {
                if let Err(err) = self.jf.refresh_playlist(&e.name).await {
                    tracing::warn!("could not ask Jellyfin to re-read playlist {:?}: {err:#}", e.name);
                }
            }
            Ok(())
        })
    }

    fn album_details<'a>(&'a self, folder: &'a str) -> Fut<'a, BTreeMap<String, crate::albums::Known>> {
        Box::pin(async move {
            let term = crate::albums::search_term(folder);
            self.jf.album_tracks(&term, folder).await
        })
    }

    fn lists_tracks(&self) -> bool {
        true
    }

    fn track_paths<'a>(&'a self, library_id: &'a str, since: Option<i64>, start: usize, limit: usize) -> Fut<'a, super::TrackPage> {
        Box::pin(async move {
            let (items, total) = self.jf.track_page(library_id, since, start, limit).await?;
            Ok(super::TrackPage { items, total })
        })
    }

    fn lists_albums(&self) -> bool {
        true
    }

    fn albums<'a>(&'a self, library_id: &'a str, limit: usize) -> Fut<'a, Vec<super::Album>> {
        Box::pin(async move {
            let heads = self.jf.album_heads(library_id, limit).await?;
            // Only albums that are new, or that Jellyfin saved again since we last looked, have their tracks read.
            let stale: Vec<(String, Option<String>)> = {
                let known = self.summaries.lock().unwrap_or_else(|p| p.into_inner());
                heads
                    .iter()
                    .filter(|(a, saved)| known.get(&a.id).is_none_or(|(s, _)| s != saved))
                    .map(|(a, saved)| (a.id.clone(), saved.clone()))
                    .collect()
            };
            for chunk in stale.chunks(SUMMARY_READERS) {
                let mut readers = tokio::task::JoinSet::new();
                for (id, saved) in chunk.iter().cloned() {
                    let jf = self.jf.clone();
                    readers.spawn(async move { (id.clone(), saved, jf.album_summary(&id).await) });
                }
                while let Some(done) = readers.join_next().await {
                    let Ok((id, saved, summary)) = done else { continue };
                    match summary {
                        Ok(s) => {
                            self.summaries.lock().unwrap_or_else(|p| p.into_inner()).insert(id, (saved, s));
                        }
                        Err(e) => tracing::warn!("tracks of album {id}: {e:#}"),
                    }
                }
            }
            let mut known = self.summaries.lock().unwrap_or_else(|p| p.into_inner());
            known.retain(|id, _| heads.iter().any(|(a, _)| &a.id == id));
            Ok(heads
                .into_iter()
                .map(|(mut a, _)| {
                    if let Some((_, s)) = known.get(&a.id) {
                        a.tracks = s.tracks;
                        a.format = s.format.clone();
                        a.bitrate_kbps = s.bitrate_kbps;
                    }
                    a
                })
                .collect())
        })
    }

    fn play_album<'a>(&'a self, album_id: &'a str) -> Fut<'a, String> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await?;
            let targets = cast_targets(&sessions, &self.player);
            let Some(first) = targets.first() else {
                if self.player.app.is_some() {
                    bail!("{} is not open", self.player.value().replace(SEP, " on "));
                }
                bail!("no player is playing music; choose a player in Settings");
            };
            let label = [first.client.as_deref(), first.device_name.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" on ");
            let tracks = self.jf.album_track_ids(album_id).await?;
            if tracks.is_empty() {
                bail!("that album has no tracks");
            }
            let mut last_error = None;
            let mut sent = 0;
            for t in &targets {
                match self.jf.play_now(t.id.as_deref().unwrap_or_default(), &tracks).await {
                    Ok(()) => sent += 1,
                    Err(e) => last_error = Some(e),
                }
            }
            match (sent, last_error) {
                (0, Some(e)) => Err(e),
                _ => Ok(label),
            }
        })
    }

    fn transport<'a>(&'a self, command: super::Transport) -> Fut<'a, String> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await?;
            let targets = control_targets(&sessions, &self.player);
            let Some(first) = targets.first() else {
                bail!("nothing is playing");
            };
            let label = session_label(first);
            // Play/pause is sent as an explicit Pause or Unpause, decided from what the device reports: a toggle
            // that arrives late, twice, or at the wrong session could do the opposite of what the page shows.
            let paused = targets.iter().find(|t| t.now_playing_item.is_some()).and_then(|t| t.play_state.as_ref()).and_then(|p| p.is_paused);
            let name = match command {
                super::Transport::Previous => "PreviousTrack",
                super::Transport::PlayPause if paused == Some(true) => "Unpause",
                super::Transport::PlayPause => "Pause",
                super::Transport::Next => "NextTrack",
            };
            // Every session of the device (see cast_targets). Verified on the tablet 2026-10-12, one command and one
            // session at a time: the reporting session ignores Pause, Unpause and NextTrack; the command-less
            // "Play On" session executes them, and the reporting session then shows the result. NextTrack stops at
            // the end of the queue; PreviousTrack restarts the track first and goes back on a second press.
            let mut sent = 0;
            let mut last_error = None;
            for t in &targets {
                match self.jf.playstate(t.id.as_deref().unwrap_or_default(), name).await {
                    Ok(()) => sent += 1,
                    Err(e) => last_error = Some(e),
                }
            }
            match (sent, last_error) {
                (0, Some(e)) => Err(e),
                _ => Ok(label),
            }
        })
    }

    fn play_tracks<'a>(&'a self, ids: &'a [String], how: super::PlayHow) -> Fut<'a, String> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await?;
            let targets = cast_targets(&sessions, &self.player);
            let Some(first) = targets.first() else {
                if self.player.app.is_some() {
                    bail!("{} is not open", self.player.value().replace(SEP, " on "));
                }
                bail!("no player is playing music; start one, or choose a player in Settings");
            };
            let label = [first.client.as_deref(), first.device_name.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" on ");
            // Finamp re-sorts a multi-track PlayNext/PlayLast batch by track number, so those go one track per
            // command (decision 0006).
            let commands: Vec<(&str, Vec<String>)> = match how {
                super::PlayHow::Now => vec![("PlayNow", ids.to_vec())],
                // In file order, not reversed: Finamp puts each "play next" track after the ones already queued
                // as next (verified on the tablet 2026-10-12: reversed sending played them backwards).
                super::PlayHow::Next => ids.iter().map(|i| ("PlayNext", vec![i.clone()])).collect(),
                super::PlayHow::Last => ids.iter().map(|i| ("PlayLast", vec![i.clone()])).collect(),
            };
            let mut last_error = None;
            let mut sent = 0;
            for t in &targets {
                let session = t.id.as_deref().unwrap_or_default();
                let mut ok = true;
                for (cmd, batch) in &commands {
                    if let Err(e) = self.jf.play_command(session, cmd, batch).await {
                        last_error = Some(e);
                        ok = false;
                        break;
                    }
                }
                if ok {
                    sent += 1;
                }
            }
            match (sent, last_error) {
                (0, Some(e)) => Err(e),
                _ => Ok(label),
            }
        })
    }

    fn album_cover<'a>(&'a self, album_id: &'a str) -> Fut<'a, Option<(String, Vec<u8>)>> {
        Box::pin(self.jf.primary_image(album_id, crate::covers::MAX_BYTES))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn players_parse_and_print() {
        assert_eq!(Player::parse(""), Player::default());
        assert_eq!(Player::parse("Finamp").value(), "Finamp");
        assert_eq!(Player::parse(" Finamp :: Lenovo Tab ").value(), "Finamp::Lenovo Tab");
        assert_eq!(Player::parse("::Lenovo Tab"), Player::default(), "a device without an app means nothing");
    }

    #[test]
    fn players_match_by_loose_app_and_exact_device() {
        let tab = Player::parse("Finamp::Lenovo Tab");
        assert!(tab.matches(Some("Finamp"), Some("lenovo tab")));
        assert!(!tab.matches(Some("Finamp"), Some("motorola edge 50 neo")));
        assert!(!tab.matches(Some("Finamp"), None));
        let any_finamp = Player::parse("Finamp");
        assert!(any_finamp.matches(Some("finamp"), Some("motorola edge 50 neo")));
        assert!(Player::parse("Jellyfin Android").matches(Some("Jellyfin for Android"), Some("x")));
        assert!(!Player::default().matches(Some("Finamp"), Some("Lenovo Tab")), "no preference matches nothing");
    }

    fn session(client: &str, device: &str, playing: Option<&str>) -> Session {
        let item = playing.map_or("null".to_string(), |id| format!(r#"{{"Id":"{id}","Name":"x","MediaType":"Audio"}}"#));
        serde_json::from_str(&format!(r#"{{"Client":"{client}","DeviceName":"{device}","NowPlayingItem":{item}}}"#)).unwrap()
    }

    fn remote(client: &str, device: &str, id: &str, commands: bool, playing: Option<&str>, last: &str) -> Session {
        let mut s = session(client, device, playing);
        s.id = Some(id.into());
        s.supports_remote_control = Some(false);
        s.supported_commands = Some(if commands { vec!["PlayState".into()] } else { vec![] });
        s.play_state = playing.map(|_| crate::jellyfin::PlayState { position_ticks: Some(0), is_paused: Some(false) });
        s.last_activity_date = Some(last.into());
        s
    }

    fn ids(v: Vec<&Session>) -> Vec<String> {
        let mut out: Vec<String> = v.into_iter().filter_map(|s| s.id.clone()).collect();
        out.sort();
        out
    }

    /// Finamp 1.0.x on the tablet, as observed: a reporting session with commands and a "Play On" one without.
    fn tablet() -> Vec<Session> {
        vec![
            remote("Finamp", "Lenovo Tab", "report", true, Some("x"), "2026-10-12T16:49:00Z"),
            remote("Finamp", "Lenovo Tab", "playon", false, None, "2026-10-12T15:25:00Z"),
        ]
    }

    #[test]
    fn music_goes_to_every_session_of_the_chosen_device() {
        let mut sessions = tablet();
        sessions.push(remote("Finamp", "motorola", "phone", true, Some("y"), "2026-10-12T16:50:00Z"));
        sessions.push(remote("Jellyfin Web", "Edge", "web", true, None, "2026-10-12T16:51:00Z"));
        assert_eq!(ids(cast_targets(&sessions, &Player::parse("Finamp::Lenovo Tab"))), ["playon", "report"]);
        assert_eq!(ids(cast_targets(&sessions, &Player::parse("Finamp"))), ["phone"], "app only: its most recent device, never two devices");
        assert!(cast_targets(&sessions, &Player::parse("Kodi")).is_empty(), "not open: no guess");
    }

    #[test]
    fn with_no_player_chosen_music_goes_to_the_device_playing_now() {
        let mut sessions = tablet();
        sessions.push(remote("Finamp", "motorola", "phone", true, None, "2026-10-12T16:55:00Z"));
        assert_eq!(ids(cast_targets(&sessions, &Player::default())), ["playon", "report"]);
        let idle = vec![remote("Finamp", "Lenovo Tab", "playon", false, None, "2026-10-12T15:25:00Z")];
        assert!(cast_targets(&idle, &Player::default()).is_empty(), "nothing playing and nothing chosen: no guess");
    }

    #[test]
    fn controls_reach_a_paused_device_that_play_would_skip() {
        let mut paused = remote("Finamp", "Lenovo Tab", "report", true, Some("x"), "2026-10-12T16:00:00Z");
        paused.play_state = Some(crate::jellyfin::PlayState { position_ticks: Some(0), is_paused: Some(true) });
        let sessions = vec![paused, remote("Finamp", "Lenovo Tab", "playon", false, None, "2026-10-12T15:00:00Z")];
        assert!(cast_targets(&sessions, &Player::default()).is_empty(), "starting music needs a device that plays");
        assert_eq!(ids(control_targets(&sessions, &Player::default())), ["playon", "report"], "resume must reach it");
        assert_eq!(session_label(control_targets(&sessions, &Player::default())[0]), "Finamp on Lenovo Tab");
    }

    #[test]
    fn a_paused_or_finished_track_does_not_count_as_playing() {
        let mut paused = remote("Finamp", "motorola", "phone", true, Some("x"), "2026-10-12T16:00:00Z");
        paused.play_state = Some(crate::jellyfin::PlayState { position_ticks: Some(0), is_paused: Some(true) });
        assert!(cast_targets(&[paused], &Player::default()).is_empty());
    }

    #[test]
    fn the_chosen_player_wins_when_it_is_playing() {
        let playing = vec![Started { item_id: "phone".into(), at: 2 }, Started { item_id: "tab".into(), at: 1 }];
        let sessions = vec![session("Finamp", "motorola", Some("phone")), session("Finamp", "Lenovo Tab", Some("tab"))];
        let id = |p: &Player| choose(&playing, &sessions, p).map(|s| s.item_id.clone());
        assert_eq!(id(&Player::parse("Finamp::Lenovo Tab")).as_deref(), Some("tab"));
        assert_eq!(id(&Player::default()).as_deref(), Some("phone"), "no preference: newest");
        assert_eq!(id(&Player::parse("Kodi")).as_deref(), Some("phone"), "chosen player idle: newest");
        // A stale session showing a track the log says stopped does not count.
        let stale = vec![session("Finamp", "Lenovo Tab", Some("old"))];
        assert_eq!(choose(&playing, &stale, &Player::parse("Finamp::Lenovo Tab")).map(|s| s.item_id.as_str()), Some("phone"));
        assert!(choose(&[], &sessions, &Player::default()).is_none());
    }

    #[test]
    fn player_choices_group_by_app_newest_first_without_duplicates() {
        let sessions = vec![session("Finamp", "Lenovo Tab", None), session("Jellyfin for Android", "Lenovo Tab", None)];
        let devices: Vec<Device> = serde_json::from_str(
            r#"[{"AppName":"Finamp","Name":"Lenovo Tab","DateLastActivity":"2026-10-12"},
                {"AppName":"Finamp","Name":"motorola","DateLastActivity":"2026-10-11"},
                {"AppName":"Jellyfin Android","Name":"Lenovo Tab","DateLastActivity":"2026-10-12"},
                {"AppName":"Kodi","Name":"LibreELEC","DateLastActivity":"2026-09-01"},
                {"AppName":null,"Name":"ghost"}]"#,
        )
        .unwrap();
        let labels: Vec<String> = player_choices(&sessions, &devices).into_iter().map(|c| c.label).collect();
        assert_eq!(
            labels,
            vec![
                "Any player",
                "Finamp, any device",
                "Finamp on Lenovo Tab",
                "Finamp on motorola",
                "Jellyfin for Android, any device",
                "Jellyfin for Android on Lenovo Tab",
                "Kodi, any device",
                "Kodi on LibreELEC",
            ]
        );
    }

    #[test]
    fn connecting_needs_an_address_and_a_key() {
        let v = |pairs: &[(&str, &str)]| -> Values { pairs.iter().map(|(k, v)| (k.to_string(), (*v).into())).collect() };
        assert!(connect(&v(&[("url", "jf:8096"), ("api_key", "k")]), true).is_err());
        assert!(connect(&v(&[("url", "http://jf:8096")]), true).is_err());
        assert!(connect(&v(&[("url", "http://jf:8096"), ("api_key", "k")]), true).is_ok());
        assert!(connect(&v(&[("url", "http://jf:8096"), ("api_key", "k")]), false).is_ok());
        // A user saved by an earlier version is ignored, not an error.
        assert!(connect(&v(&[("url", "http://jf:8096"), ("api_key", "k"), ("user", "u")]), false).is_ok());
    }
}
