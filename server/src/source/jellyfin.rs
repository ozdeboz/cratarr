//! Jellyfin as a source. What is playing comes from the activity log first (session records can be stale with some
//! clients, see `nowplaying.rs`), the position from the session playing that track. Library membership comes from
//! the track's ancestors, never from paths. No Jellyfin user is configured; the one call that needs a user context
//! (which library a track is in) uses an administrator, found once (decision 0005).

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use anyhow::{Context, Result, bail};

use super::{Choice, Field, FieldKind, Fut, Kind, Library, Playing, PlaylistChange, PlaylistEvent, Source, Status, Track, Upcoming, Values, text};
use crate::jellyfin::{AlbumTrackItem, Device, Jellyfin, PlayingItem, Session, pick_now_playing};
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
        Field {
            key: "account",
            label: "Plays as",
            kind: FieldKind::Choice,
            required: false,
            help: Some(
                "Whose favourites and play counts Cratarr uses when no player says, as when it plays in the browser. \
                 Empty: the followed player's account, else the server's only account or its administrator.",
            ),
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
        fallback: tokio::sync::OnceCell::new(),
        player: Player::parse(text(values, "player").unwrap_or("")),
        account: text(values, "account").map(str::to_string).filter(|a| !a.is_empty()),
        items: Mutex::new(HashMap::new()),
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

/// Whose favourite or play a track is: the account of the session playing it, else the account Cratarr plays as (its
/// setting; decision 0022: a track the browser plays has no session), else the account of the player the page follows.
fn whose(sessions: &[Session], player: &Player, account: Option<&str>, track: &str) -> Option<String> {
    let playing = sessions.iter().find(|s| s.now_playing_item.as_ref().is_some_and(|i| i.id == track) && s.user_id.is_some());
    playing
        .and_then(|s| s.user_id.clone())
        .or_else(|| account.map(str::to_string))
        .or_else(|| control_targets(sessions, player).into_iter().find(|s| s.user_id.is_some()).and_then(|s| s.user_id.clone()))
}

/// How to turn a playlist's tracks into the wanted list with the operations an API key has.
#[derive(Debug, PartialEq, Eq)]
enum TrackPlan {
    Nothing,
    /// The wanted list is the current one plus more at the end: add from this index of the wanted list.
    Append(usize),
    /// The wanted list is the current one without these entries (indexes into the current list), order kept.
    Remove(Vec<usize>),
    /// Anything else: empty it and add the wanted list.
    Refill,
}

fn plan_tracks(current: &[String], wanted: &[String]) -> TrackPlan {
    if current == wanted {
        return TrackPlan::Nothing;
    }
    if wanted.len() > current.len() && wanted.starts_with(current) {
        return TrackPlan::Append(current.len());
    }
    // A subsequence of the current list: remove the rest.
    let mut remove = Vec::new();
    let mut w = 0;
    for (i, c) in current.iter().enumerate() {
        if wanted.get(w) == Some(c) {
            w += 1;
        } else {
            remove.push(i);
        }
    }
    if w == wanted.len() {
        return TrackPlan::Remove(remove);
    }
    TrackPlan::Refill
}

/// A playlist imported from an `.m3u` file in a library (its path is that file), as opposed to one Jellyfin keeps
/// itself (in its data folder, as `playlist.xml`).
fn own_playlist_is_imported(path: Option<&str>) -> bool {
    path.is_some_and(|p| p.to_lowercase().ends_with(".m3u") || p.to_lowercase().ends_with(".m3u8"))
}

/// A session that has not been active for this long is idle: its record may still show a track that stopped long
/// ago, so it is not believed (an open Finamp is active every few seconds, playing or paused).
const IDLE_SECS: i64 = 300;

/// What the player plays now, from two sources that each lag at times: the activity log (a start per track, but a
/// track whose stop was never reported stays "started"), and the player's own session (current, but it can lag the
/// log by seconds after a skip). The newest evidence wins:
/// - a track both report: the log's entry (it has the start time);
/// - otherwise the player's newest report, when it is newer than the log's newest start: its track (start = report
///   time minus position), or nothing when it reports nothing. Right after Play, so, the new track, not the newest
///   unclosed start in the log (the "ghost");
/// - otherwise the log's newest start (right after Next, when the log moved first).
///
/// Idle sessions never count.
pub fn choose(playing: &[Started], sessions: &[Session], player: &Player, now: i64) -> Option<Started> {
    let epoch = |d: &Option<String>| d.as_deref().and_then(crate::nowplaying::parse_iso_epoch).filter(|t| *t > 0);
    fn audio(s: &Session) -> Option<&PlayingItem> {
        s.now_playing_item.as_ref().filter(|i| i.media_type.as_deref().is_none_or(|m| m.eq_ignore_ascii_case("Audio")))
    }
    let fresh: Vec<&Session> = sessions
        .iter()
        // No player chosen: every player has a say.
        .filter(|s| player.app.is_none() || player.matches(s.client.as_deref(), s.device_name.as_deref()))
        .filter(|s| epoch(&s.last_activity_date).is_some_and(|t| now - t <= IDLE_SECS))
        .collect();
    if let Some(both) = fresh.iter().filter_map(|s| audio(s)).find_map(|i| playing.iter().find(|p| p.item_id == i.id)) {
        return Some(both.clone());
    }
    let newest_log = playing.first();
    let report = fresh.iter().filter_map(|s| epoch(&s.last_playback_check_in).map(|t| (*s, t))).max_by_key(|(_, t)| *t);
    if let Some((s, at)) = report {
        let position = s.play_state.as_ref().and_then(|p| p.position_ticks).unwrap_or(0) / 10_000_000;
        let started = at - position;
        if newest_log.is_none_or(|l| started > l.at) {
            return audio(s).map(|i| Started { item_id: i.id.clone(), at: started });
        }
    }
    newest_log.cloned()
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
    /// The account used when nothing else tells (see `account_for`), asked for once.
    fallback: tokio::sync::OnceCell<String>,
    /// The account Cratarr plays as (its setting).
    account: Option<String>,
    /// Recently seen tracks with their library, so polling the playing track stays cheap.
    items: Mutex<HashMap<String, (PlayingItem, Option<String>)>>,
}


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

    /// The account a track's favourite and plays belong to (see `whose`); when nothing tells, the server's only account,
    /// or its administrator.
    async fn account_for(&self, sessions: &[Session], track: &str) -> Result<String> {
        if let Some(user) = whose(sessions, &self.player, self.account.as_deref(), track) {
            return Ok(user);
        }
        self.fallback_account().await
    }

    /// The server's only account, or its administrator (asked for once).
    async fn fallback_account(&self) -> Result<String> {
        Ok(self
            .fallback
            .get_or_try_init(|| async {
                let users = self.jf.users().await?;
                match users.as_slice() {
                    [(only, _)] => Ok::<_, anyhow::Error>(only.clone()),
                    _ => self.jf.admin_user().await,
                }
            })
            .await?
            .clone())
    }

    fn prefers(&self) -> impl Fn(&Session) -> bool + '_ {
        |s| self.player.matches(s.client.as_deref(), s.device_name.as_deref())
    }
}

/// The info chips from Jellyfin's description of the file: its audio stream, and the play count for the account.
fn track_info(d: &crate::jellyfin::MediaDetails) -> super::TrackInfo {
    let audio = d.media_streams.iter().find(|s| s.kind.as_deref() == Some("Audio"));
    super::TrackInfo {
        play_count: d.user_data.as_ref().and_then(|u| u.play_count),
        codec: audio.and_then(|a| a.codec.clone()),
        bitrate_kbps: audio.and_then(|a| a.bit_rate).map(|b| ((b + 500) / 1000) as u32),
        bit_depth: audio.and_then(|a| a.bit_depth),
        sample_rate_hz: audio.and_then(|a| a.sample_rate),
        // Jellyfin keeps no explicit flag for music.
        explicit: None,
        bpm: None,
    }
}

/// A jump in the queue: the rest of it from that track, as one play command (at most 240 ids fit, decision 0006).
fn from_track(queue: &[String], track: &str) -> Vec<String> {
    let from = queue.iter().position(|i| i == track).unwrap_or(0);
    queue[from..].iter().take(240).cloned().collect()
}

/// A track's artists as its tags credit them, else the album's.
fn album_track(i: AlbumTrackItem) -> super::AlbumTrack {
    let artist = if i.artists.is_empty() { i.album_artist.clone() } else { Some(i.artists.join(", ")) };
    super::AlbumTrack {
        id: i.id,
        title: i.name,
        artist,
        album: i.album,
        album_artist: i.album_artist,
        year: i.production_year,
        path: i.path,
        duration_secs: i.run_time_ticks.map(|t| t as f64 / 10_000_000.0),
    }
}

fn to_track((i, library_id): (PlayingItem, Option<String>)) -> Track {
    Track { id: i.id, title: i.name, artist: i.album_artist, album: i.album, album_id: i.album_id, path: i.path, library_id }
}

impl JellyfinSource {
    /// Send tracks to the player the page follows (every session of its device). `start_ticks`: where in the first
    /// track to start (PlayNow only).
    async fn cast(&self, ids: &[String], how: super::PlayHow, start_ticks: Option<i64>) -> Result<String> {
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
                if let Err(e) = self.jf.play_command(session, cmd, batch, start_ticks).await {
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
    }
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
            let users = self.jf.users().await.unwrap_or_default();
            // Empty: decided per track (see `account_for`).
            let automatic = Choice { value: String::new(), label: "Automatic".into() };
            out.insert("account".into(), std::iter::once(automatic).chain(users.into_iter().map(|(value, label)| Choice { value, label })).collect());
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
                .map(|f| Library { id: f.item_id, name: f.name, paths: f.locations })
                .collect())
        })
    }

    fn sample_paths<'a>(&'a self, library_id: &'a str, n: usize) -> Fut<'a, Vec<String>> {
        Box::pin(self.jf.sample_paths(library_id, n))
    }

    fn users(&self) -> Fut<'_, Vec<(String, String)>> {
        Box::pin(self.jf.users())
    }

    fn sync_account(&self) -> Fut<'_, String> {
        Box::pin(async move {
            match &self.account {
                Some(a) => Ok(a.clone()),
                None => self.fallback_account().await,
            }
        })
    }

    fn own_playlists<'a>(&'a self, user: &'a str) -> Fut<'a, Vec<super::OwnPlaylist>> {
        Box::pin(async move {
            Ok(self
                .jf
                .user_playlists(user)
                .await?
                .into_iter()
                .filter(|p| !own_playlist_is_imported(p.path.as_deref()))
                .map(|p| super::OwnPlaylist { id: p.id, name: p.name, stamp: p.etag })
                .collect())
        })
    }

    fn playlist_tracks<'a>(&'a self, user: &'a str, playlist: &'a str) -> Fut<'a, Vec<String>> {
        Box::pin(self.jf.playlist_entries(user, playlist))
    }

    fn create_playlist<'a>(&'a self, user: &'a str, name: &'a str, ids: &'a [String]) -> Fut<'a, String> {
        Box::pin(self.jf.create_playlist(user, name, ids))
    }

    /// Jellyfin refuses replacing a playlist's list (and moving an entry) to an API key; adding and removing entries
    /// it allows (observed on 10.11). So: the smallest of append, remove, or empty and refill.
    /// A Jellyfin playlist holds a track once, whatever it is sent.
    fn playlists_keep_repeats(&self) -> bool {
        false
    }

    fn set_playlist_tracks<'a>(&'a self, user: &'a str, playlist: &'a str, ids: &'a [String]) -> Fut<'a, ()> {
        Box::pin(async move {
            let now = self.jf.playlist_entry_ids(user, playlist).await?;
            let current: Vec<String> = now.iter().map(|(id, _)| id.clone()).collect();
            match plan_tracks(&current, ids) {
                TrackPlan::Nothing => Ok(()),
                TrackPlan::Append(from) => self.jf.add_to_playlist(user, playlist, &ids[from..]).await,
                TrackPlan::Remove(at) => {
                    let entries: Vec<String> = at.iter().map(|&i| now[i].1.clone()).collect();
                    self.jf.remove_from_playlist(user, playlist, &entries).await
                }
                TrackPlan::Refill => {
                    let entries: Vec<String> = now.iter().map(|(_, e)| e.clone()).collect();
                    self.jf.remove_from_playlist(user, playlist, &entries).await?;
                    self.jf.add_to_playlist(user, playlist, ids).await
                }
            }
        })
    }

    /// An API key cannot rename either: a copy under the new name takes the tracks, then the old one goes. A copy
    /// left by a rename cut short is reused, never made twice.
    fn rename_playlist<'a>(&'a self, user: &'a str, playlist: &'a str, name: &'a str) -> Fut<'a, String> {
        Box::pin(async move {
            let own = self.jf.user_playlists(user).await?;
            if !own.iter().any(|p| p.id == playlist) {
                // Already renamed and the old one gone: the copy is the playlist.
                if let Some(p) = own.iter().find(|p| p.name == name) {
                    return Ok(p.id.clone());
                }
                anyhow::bail!("the playlist to rename is gone");
            }
            let ids: Vec<String> = self.jf.playlist_entry_ids(user, playlist).await?.into_iter().map(|(id, _)| id).collect();
            let target = match own.iter().find(|p| p.name == name && p.id != playlist) {
                Some(p) => {
                    self.set_playlist_tracks(user, &p.id, &ids).await?;
                    p.id.clone()
                }
                None => self.jf.create_playlist(user, name, &ids).await?,
            };
            // The new copy keeps the old one's heart (best effort: a rename is not held up by it).
            let hearted = self.jf.favorites(&[playlist.to_string()], user).await.is_ok_and(|f| f.iter().any(|(_, on)| *on));
            if hearted && let Err(e) = self.jf.set_favorite(user, &target, true).await {
                tracing::warn!("the renamed playlist {name:?} lost its heart: {e:#}");
            }
            self.jf.delete_item(playlist).await?;
            Ok(target)
        })
    }

    fn delete_playlist<'a>(&'a self, playlist: &'a str) -> Fut<'a, ()> {
        Box::pin(self.jf.delete_item(playlist))
    }

    fn user_data<'a>(&'a self, item: &'a str) -> Fut<'a, Vec<(String, serde_json::Value)>> {
        Box::pin(async move {
            let mut out = Vec::new();
            for (user, name) in self.jf.users().await? {
                // Each account on its own: one that cannot see the item (no access to its library: 404) has no data
                // for it, and must not cost the others theirs (a Keep lost every account's play data that way while
                // one account could not see Incoming, 2026-10-15).
                let data = match self.jf.user_item_data(item, &user).await {
                    Ok(d) => d,
                    Err(e) if format!("{e:#}").contains("404") => continue,
                    Err(e) => {
                        tracing::warn!("could not read {name}'s play data of {item}: {e:#}");
                        continue;
                    }
                };
                if let Some(kept) = crate::jellyfin::user_data_to_keep(&data) {
                    out.push((user, kept));
                }
            }
            Ok(out)
        })
    }

    fn combine_user_data(&self, a: &serde_json::Value, b: &serde_json::Value) -> serde_json::Value {
        crate::jellyfin::combine_user_data(a, b)
    }

    fn set_user_data<'a>(&'a self, item: &'a str, user: &'a str, data: &'a serde_json::Value) -> Fut<'a, ()> {
        Box::pin(self.jf.set_user_item_data(item, user, data))
    }

    fn files_changed<'a>(&'a self, changes: &'a [(String, bool)]) -> Fut<'a, ()> {
        Box::pin(async move {
            // The paths as Jellyfin has them, not as its API shows them: a notice or a library location compared
            // with a substituted path matches nothing (Keep into Music went unnoticed, 2026-10-15).
            let substitutions = self.jf.path_substitutions().await?;
            let changes: Vec<(String, bool)> =
                changes.iter().map(|(p, on)| (crate::jellyfin::unsubstitute(p, &substitutions), *on)).collect();
            let kinds: Vec<(String, &str)> = changes.iter().map(|(p, on)| (p.clone(), if *on { "Created" } else { "Deleted" })).collect();
            self.jf.library_changed(&kinds).await?;
            // A notice alone finds neither a new nor a gone file in a library Jellyfin does not watch (a new .m3u in
            // Music was still not listed 110 s after its notice, 2026-10-15): have each library that got one look
            // now (see `Jellyfin::refresh_library`).
            let inside = |path: &str, location: &str| {
                let l = location.trim_end_matches('/');
                path.strip_prefix(l).is_some_and(|rest| rest.starts_with('/'))
            };
            for f in self.jf.virtual_folders().await? {
                if changes.iter().any(|(p, _)| f.locations.iter().any(|l| inside(p, l))) {
                    self.jf.refresh_library(&f.item_id).await?;
                }
            }
            Ok(())
        })
    }

    fn library_scan(&self) -> Fut<'_, super::LibraryScan> {
        Box::pin(self.jf.library_scan())
    }

    fn library_roots<'a>(&'a self, library_id: &'a str) -> Fut<'a, Vec<String>> {
        Box::pin(self.jf.library_roots(library_id, 50))
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
                    match choose(&playing, &sessions, &self.player, now) {
                        Some(s) => (s.item_id, Some(s.at)),
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
            let favorite = match self.account_for(&sessions, &id).await {
                Ok(user) => self.jf.is_favorite(&user, &id).await.ok(),
                Err(_) => None,
            };
            // The player's queue, from the session that plays this track (the preferred player's first).
            let mut playing_it: Vec<&Session> =
                sessions.iter().filter(|s| s.now_playing_item.as_ref().is_some_and(|i| i.id == id)).collect();
            playing_it.sort_by_key(|s| !self.prefers()(s));
            let upcoming = playing_it
                .first()
                .map(|s| {
                    s.now_playing_queue_full_items
                        .iter()
                        .map(|i| Upcoming {
                            duration_secs: i.run_time_ticks.map(|t| t as f64 / 10_000_000.0),
                            track: to_track((i.clone(), found.1.clone())),
                            asked_by: None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(Some(Playing { track: to_track(found), progress, started_at, player, favorite, upcoming }))
        })
    }

    fn players(&self) -> Fut<'_, Option<String>> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await?;
            let mut lines: Vec<String> = sessions
                .iter()
                .filter_map(|s| {
                    let item = s.now_playing_item.as_ref()?;
                    let paused = s.play_state.as_ref().and_then(|p| p.is_paused).unwrap_or(false);
                    let queue: Vec<&str> = s.now_playing_queue_full_items.iter().map(|i| i.id.as_str()).collect();
                    Some(format!("{} {} {paused} {}", s.id.as_deref().unwrap_or(""), item.id, queue.join(",")))
                })
                .collect();
            lines.sort();
            Ok(Some(lines.join("\n")))
        })
    }

    fn track_info<'a>(&'a self, track: &'a str) -> Fut<'a, Option<super::TrackInfo>> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await.unwrap_or_default();
            let user = self.account_for(&sessions, track).await.ok();
            let d = self.jf.media_details(track, user.as_deref()).await?;
            Ok(Some(track_info(&d)))
        })
    }

    fn jump_to<'a>(&'a self, track: &'a str) -> Fut<'a, String> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await?;
            // What comes next as the player reports it (see now_playing), from the session playing for the page.
            let mut reporting: Vec<&Session> = sessions.iter().filter(|s| s.now_playing_item.is_some()).collect();
            reporting.sort_by_key(|s| !self.prefers()(s));
            let queue: Vec<String> = reporting
                .iter()
                .map(|s| s.now_playing_queue_full_items.iter().map(|i| i.id.clone()).collect::<Vec<_>>())
                .find(|q| q.iter().any(|i| i == track))
                .context("that track is no longer in the player's queue")?;
            self.cast(&from_track(&queue, track), super::PlayHow::Now, None).await
        })
    }

    fn count_play<'a>(&'a self, track: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await?;
            let user = self.account_for(&sessions, track).await?;
            self.jf.count_play(track, &user).await
        })
    }

    fn favorite_tracks(&self) -> Fut<'_, Vec<String>> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await.unwrap_or_default();
            let user = self.account_for(&sessions, "").await?;
            self.jf.favorite_audio(&user).await
        })
    }

    fn favorite_albums(&self) -> Fut<'_, Vec<String>> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await.unwrap_or_default();
            let user = self.account_for(&sessions, "").await?;
            self.jf.favorite_albums(&user).await
        })
    }

    fn favorite_artists(&self) -> Fut<'_, Vec<String>> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await.unwrap_or_default();
            let user = self.account_for(&sessions, "").await?;
            self.jf.favorite_artists(&user).await
        })
    }

    fn playlist_favorites(&self) -> Fut<'_, BTreeMap<String, bool>> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await.unwrap_or_default();
            // No track: the account Cratarr plays as, else the followed player's (see `whose`).
            let user = self.account_for(&sessions, "").await?;
            let mut out: BTreeMap<String, bool> = BTreeMap::new();
            for (_, name, on) in self.jf.playlist_favorites(&user).await? {
                *out.entry(name).or_default() |= on;
            }
            Ok(out)
        })
    }

    fn set_playlist_favorite<'a>(&'a self, name: &'a str, on: bool) -> Fut<'a, ()> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await.unwrap_or_default();
            let user = self.account_for(&sessions, "").await?;
            let copies: Vec<String> = self.jf.playlist_favorites(&user).await?.into_iter().filter(|(_, n, _)| n == name).map(|(id, _, _)| id).collect();
            anyhow::ensure!(!copies.is_empty(), "Jellyfin has no playlist called {name} yet");
            for id in &copies {
                self.jf.set_favorite(&user, id, on).await?;
            }
            Ok(())
        })
    }

    fn favorites<'a>(&'a self, tracks: &'a [String]) -> Fut<'a, BTreeMap<String, bool>> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await?;
            // The account of the player the page follows (no track of these need be playing).
            let Some(first) = tracks.first() else { return Ok(BTreeMap::new()) };
            let user = self.account_for(&sessions, first).await?;
            let mut out = BTreeMap::new();
            // Long lists in pieces: the ids go in the address.
            for chunk in tracks.chunks(100) {
                out.extend(self.jf.favorites(chunk, &user).await?);
            }
            Ok(out)
        })
    }

    fn favorite<'a>(&'a self, track: &'a str) -> Fut<'a, Option<bool>> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await?;
            match self.account_for(&sessions, track).await {
                Ok(user) => Ok(Some(self.jf.is_favorite(&user, track).await?)),
                Err(_) => Ok(None),
            }
        })
    }

    fn set_favorite<'a>(&'a self, track: &'a str, on: bool) -> Fut<'a, bool> {
        Box::pin(async move {
            let sessions = self.jf.sessions().await?;
            let user = self.account_for(&sessions, track).await?;
            self.jf.set_favorite(&user, track, on).await?;
            Ok(on)
        })
    }

    fn track<'a>(&'a self, id: &'a str) -> Fut<'a, Track> {
        Box::pin(async move { Ok(to_track(self.item(id).await?)) })
    }

    fn playlists_changed(&self, events: Vec<PlaylistEvent>) -> Fut<'_, ()> {
        Box::pin(async move {
            // New and removed files, as any file: Jellyfin's Music library does not watch the folder, so tell it. The
            // paths are as its API shows them; told so, Jellyfin ignored them (the 66 retired files stayed listed).
            let files: Vec<(String, bool)> = events
                .iter()
                .filter_map(|e| {
                    let on = match e.change {
                        PlaylistChange::Created => true,
                        PlaylistChange::Deleted => false,
                        PlaylistChange::Edited => return None,
                    };
                    e.path.clone().map(|p| (p, on))
                })
                .collect();
            if !files.is_empty() {
                if let Err(e) = self.files_changed(&files).await {
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

    fn albums<'a>(&'a self, library_id: &'a str, limit: usize) -> Fut<'a, Vec<super::Listed>> {
        Box::pin(async move {
            let heads = self.jf.album_heads(library_id, limit).await?;
            Ok(heads.into_iter().map(|(album, stamp)| super::Listed { album, stamp }).collect())
        })
    }

    fn album_summary<'a>(&'a self, album: &'a str) -> Fut<'a, Option<super::AlbumDetails>> {
        Box::pin(async move {
            let s = self.jf.album_summary(album).await?;
            Ok(Some(super::AlbumDetails { tracks: s.tracks, format: s.format, bitrate_kbps: s.bitrate_kbps, duration_secs: s.duration_secs }))
        })
    }

    fn tracks_of_album<'a>(&'a self, album_id: &'a str) -> Fut<'a, Vec<super::AlbumTrack>> {
        Box::pin(async move { Ok(self.jf.tracks_of_album(album_id).await?.into_iter().map(album_track).collect()) })
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
            let tracks: Vec<String> = self.jf.tracks_of_album(album_id).await?.into_iter().map(|t| t.id).collect();
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
                super::Transport::Stop => "Stop",
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
        Box::pin(self.cast(ids, how, None))
    }

    fn play_from<'a>(&'a self, ids: &'a [String], start_secs: f64) -> Fut<'a, String> {
        Box::pin(self.cast(ids, super::PlayHow::Now, Some((start_secs * 10_000_000.0) as i64)))
    }

    fn album_cover<'a>(&'a self, album_id: &'a str) -> Fut<'a, Option<(String, Vec<u8>)>> {
        Box::pin(self.jf.primary_image(album_id, crate::covers::MAX_BYTES))
    }

    fn imports(&self) -> bool {
        true
    }

    /// The played tracks, then the favourites (Jellyfin cannot list either at once). The first page also counts the
    /// favourites, so the whole listing's size is known from the start.
    fn track_data<'a>(&'a self, user: &'a str, cursor: Option<&'a str>) -> Fut<'a, super::TrackDataPage> {
        Box::pin(async move {
            let (filter, start) = import_cursor(cursor)?;
            let (items, listed) = self.jf.user_track_data(user, filter, start, IMPORT_PAGE).await?;
            let total = match cursor {
                None => Some(listed + self.jf.count_user_tracks(user, "IsFavorite").await?),
                Some(_) => None,
            };
            let next = next_import_cursor(filter, start, items.len());
            Ok(super::TrackDataPage { items, next, total })
        })
    }

    fn favorite_groups<'a>(&'a self, user: &'a str) -> Fut<'a, super::FavoriteGroups> {
        Box::pin(self.jf.favorite_groups(user))
    }

    fn albums_added<'a>(&'a self, cursor: Option<&'a str>) -> Fut<'a, super::AddedPage> {
        Box::pin(async move {
            const PAGE: usize = 250;
            let start: usize = match cursor {
                Some(c) => c.parse().context("not a cursor")?,
                None => 0,
            };
            let (items, total) = self.jf.albums_added(start, PAGE).await?;
            // Counted before the undated ones were left out.
            let next = (start + PAGE < total).then(|| (start + PAGE).to_string());
            Ok(super::AddedPage { items, next, total: Some(total) })
        })
    }
}

/// Tracks per page of an import's listing: 200 took Jellyfin 4.4 to 5.8 s on 2026-10-15 (250 had taken 9 to 12 s an hour
/// before), inside the 20 s a request may take, and a page holds one of Cratarr's turns at Jellyfin (`MAX_IN_FLIGHT`) no
/// longer than that.
const IMPORT_PAGE: usize = 200;

/// Each page starts this many tracks before the last one ended: a favourite taken away meanwhile moves the rest of the
/// listing up, and the track at the page's edge would be missed. Read twice instead, which is harmless.
const IMPORT_OVERLAP: usize = 4;

/// An import's cursor: the listing (`IsPlayed`, then `IsFavorite`) and where in it; `None` is the first page.
fn import_cursor(cursor: Option<&str>) -> Result<(&'static str, usize)> {
    let Some(c) = cursor else { return Ok(("IsPlayed", 0)) };
    let (filter, start) = c.split_once(':').context("not a cursor")?;
    let filter = match filter {
        "IsPlayed" => "IsPlayed",
        "IsFavorite" => "IsFavorite",
        _ => bail!("not a cursor"),
    };
    Ok((filter, start.parse().context("not a cursor")?))
}

/// The page after one of `n` tracks from `start`: further on in the same listing while pages are full, then the
/// favourites from their start, then nothing.
fn next_import_cursor(filter: &str, start: usize, n: usize) -> Option<String> {
    if n == IMPORT_PAGE {
        Some(format!("{filter}:{}", start + n - IMPORT_OVERLAP))
    } else if filter == "IsPlayed" {
        Some("IsFavorite:0".into())
    } else {
        None
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

    /// 2026-10-12T17:00:00Z.
    const NOW: i64 = 1_791_824_400;

    /// A session active a minute ago that last reported playback `checked_in` seconds before NOW, `position` seconds in.
    fn fresh(client: &str, device: &str, playing: Option<&str>, checked_in: i64, position: i64) -> Session {
        let mut s = session(client, device, playing);
        s.last_activity_date = Some("2026-10-12T16:59:00Z".into());
        let t = 17 * 3600 - checked_in;
        s.last_playback_check_in = Some(format!("2026-10-12T{:02}:{:02}:{:02}Z", t / 3600, t / 60 % 60, t % 60));
        s.play_state = Some(crate::jellyfin::PlayState { position_ticks: Some(position * 10_000_000), is_paused: Some(false) });
        s
    }

    fn chosen(playing: &[Started], sessions: &[Session], p: &str) -> Option<String> {
        choose(playing, sessions, &Player::parse(p), NOW).map(|s| s.item_id)
    }

    #[test]
    fn a_jump_plays_the_rest_of_the_queue_from_that_track() {
        let q: Vec<String> = ["a", "b", "c", "d"].map(String::from).to_vec();
        assert_eq!(from_track(&q, "c"), ["c", "d"]);
        assert_eq!(from_track(&q, "a"), q);
        let long: Vec<String> = (0..300).map(|i| i.to_string()).collect();
        let sent = from_track(&long, "10");
        assert_eq!((sent.len(), sent[0].as_str()), (240, "10"), "one play command holds at most 240 ids");
    }

    #[test]
    fn info_chips_come_from_the_audio_stream_and_the_accounts_plays() {
        let d: crate::jellyfin::MediaDetails = serde_json::from_value(serde_json::json!({
            "MediaStreams": [
                { "Type": "EmbeddedImage", "Codec": "mjpeg", "BitDepth": 8 },
                { "Type": "Audio", "Codec": "flac", "BitRate": 753657, "BitDepth": 16, "SampleRate": 44100 }
            ],
            "UserData": { "PlayCount": 16, "Played": true }
        }))
        .unwrap();
        let i = track_info(&d);
        assert_eq!(
            (i.codec.as_deref(), i.bitrate_kbps, i.bit_depth, i.sample_rate_hz, i.play_count, i.explicit),
            (Some("flac"), Some(754), Some(16), Some(44100), Some(16), None)
        );
        let bare: crate::jellyfin::MediaDetails = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(track_info(&bare), super::super::TrackInfo::default());
    }

    #[test]
    fn a_favourite_belongs_to_the_account_playing_it() {
        let mut playing = remote("Finamp", "Lenovo Tab", "report", true, Some("t1"), "2026-10-12T16:49:00Z");
        playing.user_id = Some("me".into());
        let mut other = remote("Finamp", "motorola", "phone", true, Some("t2"), "2026-10-12T16:50:00Z");
        other.user_id = Some("guest".into());
        let sessions = vec![playing, other];
        assert_eq!(whose(&sessions, &Player::default(), None, "t2").as_deref(), Some("guest"), "the session playing that track");
        assert_eq!(whose(&sessions, &Player::default(), Some("set"), "t2").as_deref(), Some("guest"), "before the setting");
        assert_eq!(
            whose(&sessions, &Player::parse("Finamp::Lenovo Tab"), Some("set"), "t9").as_deref(),
            Some("set"),
            "a track no session plays (the browser's): the account Cratarr plays as"
        );
        assert_eq!(whose(&sessions, &Player::parse("Finamp::Lenovo Tab"), None, "t9").as_deref(), Some("me"), "else the followed player");
        assert_eq!(whose(&[], &Player::default(), None, "t1"), None);
    }

    #[test]
    fn the_smallest_change_with_what_an_api_key_may_do() {
        let v = |x: &[&str]| x.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(plan_tracks(&v(&["a", "b"]), &v(&["a", "b"])), TrackPlan::Nothing);
        assert_eq!(plan_tracks(&v(&["a", "b"]), &v(&["a", "b", "c", "d"])), TrackPlan::Append(2));
        assert_eq!(plan_tracks(&v(&["a", "b", "c", "b"]), &v(&["a", "b"])), TrackPlan::Remove(vec![2, 3]));
        assert_eq!(plan_tracks(&v(&["a", "b"]), &v(&["b", "a"])), TrackPlan::Refill);
        assert_eq!(plan_tracks(&v(&[]), &v(&["a"])), TrackPlan::Append(0));
        assert_eq!(plan_tracks(&v(&["a"]), &v(&[])), TrackPlan::Remove(vec![0]));
    }

    #[test]
    fn imported_m3u_playlists_are_not_the_users_own() {
        assert!(own_playlist_is_imported(Some("/data/media/Music/Chill.m3u")));
        assert!(!own_playlist_is_imported(Some("/config/data/playlists/Chill/playlist.xml")));
        assert!(!own_playlist_is_imported(None));
        let listed: Vec<crate::jellyfin::UserPlaylist> = serde_json::from_str(
            r#"[{"Name":"Chill","Id":"a","Etag":"e1","Path":"/data/media/Music/Chill.m3u"},{"Name":"Mine","Id":"b","Etag":"e2","Path":"/config/data/playlists/Mine/playlist.xml"}]"#,
        )
        .unwrap();
        let own: Vec<&str> = listed.iter().filter(|p| !own_playlist_is_imported(p.path.as_deref())).map(|p| p.name.as_str()).collect();
        assert_eq!(own, ["Mine"]);
    }

    #[test]
    fn the_chosen_player_wins_when_it_is_playing() {
        let playing = vec![Started { item_id: "phone".into(), at: NOW - 10 }, Started { item_id: "tab".into(), at: NOW - 20 }];
        let sessions = vec![fresh("Finamp", "motorola", Some("phone"), 5, 5), fresh("Finamp", "Lenovo Tab", Some("tab"), 5, 15)];
        assert_eq!(chosen(&playing, &sessions, "Finamp::Lenovo Tab").as_deref(), Some("tab"));
        assert_eq!(chosen(&playing, &sessions, "").as_deref(), Some("phone"), "no preference: newest");
        assert_eq!(chosen(&playing, &sessions, "Kodi").as_deref(), Some("phone"), "chosen player not there: newest");
        assert!(choose(&[], &[], &Player::default(), NOW).is_none());
    }

    #[test]
    fn right_after_play_the_new_track_shows_not_an_old_unclosed_start() {
        // The log still holds a start from half an hour ago whose stop never came, and no start for the new track yet.
        let playing = vec![Started { item_id: "ghost".into(), at: NOW - 1800 }];
        let sessions = vec![fresh("Finamp", "Lenovo Tab", Some("new"), 1, 0)];
        assert_eq!(chosen(&playing, &sessions, "Finamp::Lenovo Tab").as_deref(), Some("new"));
        assert_eq!(chosen(&playing, &sessions, "").as_deref(), Some("new"));
    }

    #[test]
    fn right_after_next_the_log_wins_over_a_session_that_lags() {
        // Measured 2026-10-12: the log has the next track at once; the session still shows the previous one, as of
        // its last report, 2 s into it, well before the log's start.
        let playing = vec![Started { item_id: "next".into(), at: NOW - 3 }];
        let sessions = vec![fresh("Finamp", "Lenovo Tab", Some("previous"), 200, 2)];
        assert_eq!(chosen(&playing, &sessions, "Finamp::Lenovo Tab").as_deref(), Some("next"));
    }

    #[test]
    fn idle_sessions_are_not_believed() {
        let playing = vec![Started { item_id: "logged".into(), at: NOW - 600 }];
        let mut stale = fresh("Finamp", "Lenovo Tab", Some("old"), 1, 0);
        stale.last_activity_date = Some("2026-10-12T15:00:00Z".into());
        assert_eq!(chosen(&playing, &[stale.clone()], "Finamp::Lenovo Tab").as_deref(), Some("logged"));
        // A session without dates (never active) counts as idle too.
        assert_eq!(chosen(&playing, &[session("Finamp", "Lenovo Tab", Some("old"))], "Finamp::Lenovo Tab").as_deref(), Some("logged"));
    }

    #[test]
    fn a_player_that_reported_stopping_shows_nothing_rather_than_a_ghost() {
        let playing = vec![Started { item_id: "ghost".into(), at: NOW - 1800 }];
        let stopped = fresh("Finamp", "Lenovo Tab", None, 30, 0);
        assert_eq!(chosen(&playing, &[stopped.clone()], "Finamp::Lenovo Tab"), None);
        // One that never reported playback has no say: the log stands (players that do not report themselves).
        let mut silent = stopped;
        silent.last_playback_check_in = Some("0001-01-01T00:00:00.0000000Z".into());
        assert_eq!(chosen(&playing, &[silent], "Finamp::Lenovo Tab").as_deref(), Some("ghost"));
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
    fn an_import_lists_the_played_tracks_then_the_favourites_with_pages_that_overlap() {
        assert_eq!(import_cursor(None).unwrap(), ("IsPlayed", 0));
        let next = next_import_cursor("IsPlayed", 0, IMPORT_PAGE).unwrap();
        assert_eq!(import_cursor(Some(&next)).unwrap(), ("IsPlayed", IMPORT_PAGE - IMPORT_OVERLAP));
        assert_eq!(next_import_cursor("IsPlayed", 400, 17).as_deref(), Some("IsFavorite:0"), "the played ones are done");
        assert_eq!(next_import_cursor("IsFavorite", 400, 17), None, "and then the favourites");
        assert!(import_cursor(Some("IsPlayed")).is_err());
        assert!(import_cursor(Some("Likes:0")).is_err(), "only the two listings");
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
