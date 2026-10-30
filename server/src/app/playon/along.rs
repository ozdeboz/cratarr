//! Listening along (decision 0079; the owner, 2026-10-30): a listener who opened their listening to others ("Others may
//! follow me") is followed on another account's own player, from anywhere, in step: each change of the leader's (its
//! track, play or pause, a seek, its speed) puts every follower in line with a play of that one track at the leader's
//! place now, and a follower that drifts more than two seconds off is sought back. Only the leader edits the queue;
//! followers ask for a track (`queue_requests`). A shared device stays at home's: another's shared player is followed
//! from home only, as it is used (0.9.8's fence).
//!
//! Not sync play (0037): that is one room's players playing one sound, every member steering it. Here each follower
//! plays its own stream wherever it is, a second or two apart at most, and steers nothing but its own pause.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Command, Entry, OWN_MUSIC, PlayFrom, PlayOn, PlayerKind, PlayerView, QueueTrack, belongs_to, first_owner, key, view_in};
use crate::app::ToggleError;
use crate::app::queue::Repeat;
use crate::app::queue_requests::{Asker, QueueRequest, SETTLE_MS, State, line_of};
use crate::app::radio::AutoplayMode;

/// A leader (or a follower) gone this long ends the following.
const GONE_FOR: Duration = Duration::from_secs(120);
/// A follower further off its leader than this is sought back (the owner, 2026-10-30).
const OFF_BY_MS: u64 = 2_000;
/// After a play or a seek is sent, a follower's reports are not judged for this long: it is getting there.
const SETTLING: Duration = Duration::from_secs(3);
/// The follower's queue while it follows: one id, whoever it follows, so its own queue is kept once.
const FOLLOWING: &str = "following";
/// How many times of following a player keeps, and for how long after they ended (s): plays are told late at most a
/// few days after (a player gives up after 30).
const SPANS: usize = 50;
const SPANS_KEPT_SECS: i64 = 30 * 86_400;

/// A player followed, by its user and name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FollowTarget {
    pub user: String,
    pub player: String,
}

/// A player one may follow, as the leaders list shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LeaderPlayer {
    pub name: String,
    pub playing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<QueueTrack>,
    pub position_ms: u64,
    pub position_at: u64,
}

/// A user one may follow, with their players.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Leader {
    pub user: String,
    pub players: Vec<LeaderPlayer>,
}

/// A user who follows one's players (which of them), or whose requests go straight in.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Follower {
    pub user: String,
    pub players: Vec<String>,
}

/// New requests to tell each leader, with when the last came: told as one once they settle.
#[derive(Default)]
pub(super) struct Asked(Mutex<HashMap<String, (Vec<String>, Instant)>>);

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// A speed as compared: in twentieths, so a report's last digits are no change.
pub(super) fn rate(speed: Option<f64>) -> i64 {
    (speed.unwrap_or(1.0) * 20.0).round() as i64
}

/// The account a player is a device of.
fn account(e: &Entry, first: Option<&str>) -> Option<String> {
    e.owner.clone().or_else(|| first.map(str::to_string))
}

/// The users following player `name`, by name.
pub(super) fn followers_in(entries: &HashMap<String, Entry>, name: &str) -> Vec<String> {
    let following: Vec<&Entry> = entries.values().filter(|e| e.following.as_ref().is_some_and(|f| f.player == name)).collect();
    if following.is_empty() {
        return Vec::new();
    }
    let first = first_owner();
    let mut users: Vec<String> = following.into_iter().filter_map(|e| account(e, first.as_deref())).collect();
    users.sort();
    users.dedup();
    users
}

/// Where a player is heard now: its last report counted on at its speed (the tape's, 0075) while it plays.
fn heard_at(e: &Entry) -> u64 {
    let r = &e.report;
    let speed = r.speed_x.unwrap_or(1.0);
    if !r.playing || !e.online() || r.rewinding || rate(Some(speed)) == 20 {
        return e.position_now();
    }
    let p = (r.position_ms as f64 + e.at.elapsed().as_millis() as f64 * speed).max(0.0) as u64;
    r.duration_ms.map_or(p, |d| p.min(d))
}

/// What a leader plays now: its track, where it is heard, whether it plays. None while it plays nothing of Cratarr's.
fn leader_now(entries: &HashMap<String, Entry>, player: &str) -> Option<(QueueTrack, u64, bool)> {
    let e = entries.get(player)?;
    // A player of a sync group shows its group's music: the group's leader's place.
    let plays = e.leader.as_deref().filter(|l| *l != player).and_then(|l| entries.get(l)).unwrap_or(e);
    if plays.device.is_some() {
        return None;
    }
    let track = view_in(entries, player, false)?.track?;
    Some((track, heard_at(plays), plays.report.playing))
}

/// Away this long, it is gone.
fn gone(e: &Entry) -> bool {
    !e.online() && SystemTime::now().duration_since(e.offline_since.unwrap_or(e.seen)).is_ok_and(|d| d >= GONE_FOR)
}

/// Its time of following ends now.
fn close_span(e: &mut Entry, now: i64) {
    if let Some(last) = e.follow_spans.last_mut()
        && last.1.is_none()
    {
        last.1 = Some(now);
    }
}

/// Whether `asker` may follow `to` (or ask a track of it): another user's player, of a user who opened their listening
/// (`followable`), connected and following no one itself; a shared device from home only.
fn may_follow_in(entries: &HashMap<String, Entry>, asker: &str, to: &FollowTarget, away: bool, followable: bool) -> Result<(), ToggleError> {
    let first = first_owner();
    let none = || ToggleError::Refused(format!("{} has no player called {} to follow.", to.user, to.player));
    let e = entries.get(&to.player).ok_or_else(none)?;
    if account(e, first.as_deref()).as_deref() != Some(to.user.as_str()) {
        return Err(none());
    }
    if to.user == asker {
        return Err(ToggleError::Refused("That is one of your own players.".into()));
    }
    if !followable {
        return Err(ToggleError::Refused(format!("{} is not open to followers.", to.user)));
    }
    if e.shared && away {
        return Err(ToggleError::Fenced);
    }
    if !e.online() {
        return Err(ToggleError::Refused(format!("{} is not connected.", to.player)));
    }
    if e.following.is_some() {
        return Err(ToggleError::Refused(format!("{} follows another player itself.", to.player)));
    }
    Ok(())
}

impl PlayOn {
    /// The player `name` follows, if it follows one.
    pub fn following_of(&self, name: &str) -> Option<FollowTarget> {
        self.entries().get(name).and_then(|e| e.following.clone())
    }

    /// The players following `name`, by their names.
    pub(super) fn followers_of(&self, name: &str) -> Vec<String> {
        self.entries().iter().filter(|(_, e)| e.following.as_ref().is_some_and(|f| f.player == name)).map(|(n, _)| n.clone()).collect()
    }

    /// The users `asker` may follow and their players (decision 0079): those whose listening is open (`followable`),
    /// never `asker`, their connected players that follow no one, a shared one from home only (`away`).
    pub fn leaders(&self, asker: &str, away: bool, followable: &dyn Fn(&str) -> bool) -> Vec<Leader> {
        let first = first_owner();
        let users: Vec<String> = {
            let entries = self.entries();
            let mut u: Vec<String> = entries.values().filter_map(|e| account(e, first.as_deref())).filter(|u| u != asker).collect();
            u.sort();
            u.dedup();
            u
        };
        // Asked outside the lock: each user's settings are read from the database.
        let open: Vec<String> = users.into_iter().filter(|u| followable(u)).collect();
        let entries = self.entries();
        let mut by_user: BTreeMap<String, Vec<LeaderPlayer>> = BTreeMap::new();
        for (name, e) in entries.iter() {
            let Some(user) = account(e, first.as_deref()).filter(|u| open.contains(u)) else { continue };
            let to = FollowTarget { user: user.clone(), player: name.clone() };
            if may_follow_in(&entries, asker, &to, away, true).is_err() {
                continue;
            }
            let Some(v) = view_in(&entries, name, false) else { continue };
            by_user.entry(user).or_default().push(LeaderPlayer { name: name.clone(), playing: v.playing, track: v.track, position_ms: v.position_ms, position_at: v.position_at });
        }
        by_user
            .into_iter()
            .map(|(user, mut players)| {
                players.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                Leader { user, players }
            })
            .collect()
    }

    /// Who follows `leader`'s players (which of them), and those in their `auto_accept` beside them, following now or
    /// not: the page lists them all, each with its switch.
    pub fn followers(&self, leader: &str, auto_accept: &[String]) -> Vec<Follower> {
        let first = first_owner();
        let mut by_user: BTreeMap<String, Vec<String>> = auto_accept.iter().map(|u| (u.clone(), Vec::new())).collect();
        for e in self.entries().values() {
            let Some(to) = e.following.as_ref().filter(|to| to.user == leader) else { continue };
            let Some(user) = account(e, first.as_deref()) else { continue };
            by_user.entry(user).or_default().push(to.player.clone());
        }
        by_user
            .into_iter()
            .map(|(user, mut players)| {
                players.sort();
                players.dedup();
                Follower { user, players }
            })
            .collect()
    }

    /// Whether `asker` may follow `to` or ask a track of it now (`followable`: whether its user opened their listening).
    pub fn may_follow(&self, asker: &str, to: &FollowTarget, away: bool, followable: bool) -> Result<(), ToggleError> {
        may_follow_in(&self.entries(), asker, to, away, followable)
    }

    /// `user`'s player `mine` follows `to` (decision 0079) and is put in line at once: its own queue is kept as it
    /// stood, to switch back to. Not one in a sync group, one others follow, or one playing the device's own music.
    pub fn follow(&self, user: &str, mine: &str, to: FollowTarget, away: bool, followable: bool) -> Result<(), ToggleError> {
        {
            let first = first_owner();
            let mut entries = self.entries();
            match entries.get(mine) {
                Some(e) if belongs_to(e, user, first.as_deref()) => {
                    if !e.online() {
                        return Err(ToggleError::Refused(format!("{mine} is not connected.")));
                    }
                    if e.leader.is_some() {
                        return Err(ToggleError::Refused(format!("{mine} plays in a group: take it out first.")));
                    }
                    if e.device.is_some() {
                        return Err(ToggleError::Refused(OWN_MUSIC.into()));
                    }
                }
                _ => return Err(ToggleError::Refused(format!("No player is called {mine}."))),
            }
            if mine == to.player {
                return Err(ToggleError::Refused("A player cannot follow itself.".into()));
            }
            if entries.values().any(|x| x.following.as_ref().is_some_and(|f| f.player == mine)) {
                return Err(ToggleError::Refused(format!("Others follow {mine}: it cannot follow too.")));
            }
            may_follow_in(&entries, user, &to, away, followable)?;
            let Some(e) = entries.get_mut(mine) else { return Err(ToggleError::Refused(format!("No player is called {mine}."))) };
            let now = now_secs();
            close_span(e, now);
            e.follow_spans.push((now, None));
            e.follow_spans.retain(|(_, until)| until.is_none_or(|u| now - u < SPANS_KEPT_SECS));
            let extra = e.follow_spans.len().saturating_sub(SPANS);
            e.follow_spans.drain(..extra).for_each(drop);
            (e.following, e.held, e.lined, e.follow_away) = (Some(to.clone()), false, None, away);
        }
        tracing::info!(target: "playon", player = mine, leader = to.player.as_str(), user = to.user.as_str(), "following");
        self.line_up(mine);
        self.changed();
        Ok(())
    }

    /// `user`'s player `mine` stops following; false when it followed no one.
    pub fn unfollow(&self, user: &str, mine: &str) -> Result<bool, ToggleError> {
        let first = first_owner();
        if !self.entries().get(mine).is_some_and(|e| belongs_to(e, user, first.as_deref())) {
            return Err(ToggleError::Refused(format!("No player is called {mine}.")));
        }
        Ok(self.end_follow(mine, "stopped"))
    }

    /// Player `name` follows no one now; false when it followed no one. What it plays plays on.
    pub(super) fn end_follow(&self, name: &str, why: &str) -> bool {
        let ended = {
            let mut entries = self.entries();
            let Some(e) = entries.get_mut(name) else { return false };
            let Some(to) = e.following.take() else { return false };
            close_span(e, now_secs());
            (e.held, e.lined) = (false, None);
            to
        };
        tracing::info!(target: "playon", player = name, leader = ended.player.as_str(), why, "following ended");
        self.changed();
        true
    }

    /// The music of player `from` was handed over to `to` (the owner, 2026-10-30: "if another user or guest is
    /// following, then these also move when the music moves"): whoever followed `from` follows `to` now, put in line at
    /// once with what plays there (`now`, as handed over). One who may not follow `to` (as following is judged: `to`'s
    /// user open to followers, a shared device from home only) follows no one, and their apps are told.
    pub(super) fn follow_moved(&self, from: &str, to: &str, now: Option<(QueueTrack, u64, bool)>) {
        let first = first_owner();
        for f in self.followers_of(from) {
            let (asker, was, away, target) = {
                let entries = self.entries();
                let Some(e) = entries.get(&f) else { continue };
                let Some(was) = e.following.clone() else { continue };
                let Some(user) = entries.get(to).and_then(|y| account(y, first.as_deref())) else { continue };
                (account(e, first.as_deref()).unwrap_or_default(), was, e.follow_away, FollowTarget { user, player: to.to_string() })
            };
            // The same user: open to followers, as they were to be followed at all (closing it ends every following).
            let open = target.user == was.user || self.followable(&target.user);
            let may = may_follow_in(&self.entries(), &asker, &target, away, open);
            match may {
                Ok(()) => {
                    if let Some(e) = self.entries().get_mut(&f) {
                        (e.following, e.lined) = (Some(target.clone()), None);
                    }
                    tracing::info!(target: "playon", player = f.as_str(), from, leader = to, "following moved with the music");
                    self.line_up_at(&f, now.clone());
                }
                Err(why) => {
                    tracing::info!(target: "playon", player = f.as_str(), from, to, "following ended where the music moved: {why}");
                    self.end_follow(&f, "the music moved where it may not follow");
                    self.notice(&asker, &format!("{}'s music moved to {to}", was.user));
                }
            }
        }
        self.changed();
    }

    /// Whether `user` opened their listening to others ("Others may follow me"), as their settings say; not where no
    /// settings are kept.
    fn followable(&self, user: &str) -> bool {
        let Some(served) = self.playlists.served() else { return false };
        served.userdata.settings_of(user).is_ok_and(|(_, s)| s["followable"].as_bool() == Some(true))
    }

    /// `leader` closed their listening to others (Others may follow me off): whoever followed them follows no one.
    pub fn end_following_of(&self, leader: &str) {
        let names: Vec<String> = self.entries().iter().filter(|(_, e)| e.following.as_ref().is_some_and(|f| f.user == leader)).map(|(n, _)| n.clone()).collect();
        for n in names {
            self.end_follow(&n, "no longer open to followers");
        }
    }

    /// Following ends where the player followed, or the follower, has been gone a while.
    pub(super) fn drop_lost_follows(&self) {
        let lost: Vec<String> = {
            let entries = self.entries();
            entries
                .iter()
                .filter(|(_, e)| e.following.as_ref().is_some_and(|to| entries.get(&to.player).is_none_or(gone) || gone(e)))
                .map(|(n, _)| n.clone())
                .collect()
        };
        for n in lost {
            self.end_follow(&n, "gone");
        }
    }

    /// Whether player `name` followed another at `at` (epoch seconds): a play it tells, late too, is history then and
    /// not taste (the owner, 2026-10-30).
    pub fn was_following(&self, name: &str, at: i64) -> bool {
        self.entries().get(name).is_some_and(|e| e.follow_spans.iter().any(|(from, until)| at >= from - 5 && until.is_none_or(|u| at < u)))
    }

    /// A command to a player that follows (decision 0079): Next, Previous and what edits the queue are refused (the
    /// leader's queue it is: ask for a track); its pause is its own, and its resume catches up with the leader; a play of
    /// something else, or a stop, ends the following first. A handover takes the following along (the owner,
    /// 2026-10-30). None: done here.
    pub(super) fn along(&self, name: &str, command: Command) -> Result<Option<Command>, ToggleError> {
        let Some(to) = self.following_of(name) else { return Ok(Some(command)) };
        self.reached(name)?;
        let ask = || ToggleError::Refused(format!("Following {}: ask for a track instead.", to.user));
        let playing = self.entries().get(name).is_some_and(|e| e.report.playing);
        let resume = |me: &Self| -> Result<Option<Command>, ToggleError> {
            if !me.entries().get(name).is_some_and(Entry::online) {
                return Err(ToggleError::Refused(format!("{name} is not connected.")));
            }
            me.hold(name, false);
            me.line_up(name);
            Ok(None)
        };
        match command {
            Command::Next
            | Command::Previous
            | Command::Add { .. }
            | Command::Move { .. }
            | Command::Remove { .. }
            | Command::Upcoming { .. }
            | Command::Shuffle { .. }
            | Command::Clear
            | Command::Restore { .. }
            | Command::Jump { .. }
            | Command::Queue { .. }
            | Command::Join { .. }
            | Command::Wind { .. }
            | Command::Direction { .. } => Err(ask()),
            Command::Pause => {
                self.hold(name, true);
                Ok(Some(Command::Pause))
            }
            Command::Toggle if playing => {
                self.hold(name, true);
                Ok(Some(Command::Pause))
            }
            Command::Resume | Command::Toggle => resume(self),
            c @ (Command::Play { .. } | Command::Stop { .. }) => {
                self.end_follow(name, "played something else");
                Ok(Some(c))
            }
            other => Ok(Some(other)),
        }
    }

    fn hold(&self, name: &str, held: bool) {
        if let Some(e) = self.entries().get_mut(name) {
            e.held = held;
        }
    }

    /// Puts a follower in line with its leader: a play of the leader's one track at the leader's place now, paused as
    /// the leader is or as its own listener left it. The first time, its own queue is kept as it stood, and its Repeat,
    /// Radio and Autoplay are off while it follows (they come back with that queue).
    pub(super) fn line_up(&self, follower: &str) {
        self.line_up_at(follower, None);
    }

    /// As [`PlayOn::line_up`], with what the leader plays given (`at`: its track, place and whether it plays) where it
    /// has not reported it yet (music just handed over to it).
    fn line_up_at(&self, follower: &str, at: Option<(QueueTrack, u64, bool)>) {
        let mut entries = self.entries();
        let Some(to) = entries.get(follower).and_then(|e| e.following.clone()) else { return };
        let now = at.or_else(|| leader_now(&entries, &to.player));
        let Some(e) = entries.get_mut(follower) else { return };
        if !e.online() || e.device.is_some() {
            return;
        }
        e.lined = Some(Instant::now());
        let Some((track, position_ms, playing)) = now else {
            // The leader plays nothing of Cratarr's: the follower waits.
            if e.report.playing {
                let v = e.queue.version;
                let _ = PlayOn::tell(e, follower, &Command::Pause, v);
            }
            return;
        };
        let current = usize::try_from(e.index()).ok();
        let mut queue = e.queue.clone();
        let mut told = Vec::new();
        if queue.id != FOLLOWING {
            let standing = e.standing();
            e.keep(standing);
            let off = [
                Command::Repeat { mode: Repeat::Off },
                Command::Radio { on: false, by: None, count: None, mode: None, seed: None, within: None },
                Command::Autoplay { mode: AutoplayMode::Stop, albums: None, restart: None, seed: None, within: None },
            ];
            for c in off {
                if let Ok(t) = queue.edit_at(c, current) {
                    told.push(t);
                }
            }
        }
        let track = QueueTrack { entry: String::new(), lane: None, n: None, ..track };
        let play = Command::Play {
            from: PlayFrom { whole: true, ..PlayFrom::default() },
            tracks: Some(vec![track]),
            index: 0,
            position_ms,
            shuffle: false,
            paused: !playing || e.held,
            queue_id: Some(FOLLOWING.into()),
            queue_name: Some(format!("Following {}", to.user)),
        };
        match queue.edit_at(play, current) {
            Ok(t) => told.push(t),
            Err(err) => {
                tracing::warn!(target: "playon", player = follower, "not put in line: {err}");
                return;
            }
        }
        for t in &told {
            if PlayOn::tell(e, follower, t, queue.version).is_err() {
                break;
            }
        }
        e.told = queue.version;
        e.queue = queue;
        let kept = e.kept.len();
        e.kept.retain(|k| k.queue.id != FOLLOWING);
        if e.kept.len() != kept {
            e.kept_version += 1;
        }
        drop(entries);
        self.changed();
    }

    /// A follower's report, against its leader: another track is put in line again; paused by its own listener while
    /// the leader plays, it is left so (held); playing more than two seconds off, it is sought back.
    pub(super) fn check_in_line(&self, follower: &str) {
        let mut entries = self.entries();
        let Some(to) = entries.get(follower).and_then(|e| e.following.clone()) else { return };
        let leader = leader_now(&entries, &to.player);
        let Some(e) = entries.get_mut(follower) else { return };
        if e.lined.is_some_and(|t| t.elapsed() < SETTLING) || e.report.starting || e.device.is_some() || !e.online() {
            return;
        }
        let Some((track, at, playing)) = leader else { return };
        let mine = usize::try_from(e.index()).ok().and_then(|i| e.queue.tracks.get(i)).map(|t| (t.source.clone(), t.id.clone()));
        if mine != key(Some(&track)) {
            drop(entries);
            self.line_up(follower);
            return;
        }
        match (e.report.playing, playing) {
            (false, true) => e.held = true,
            (true, _) if e.held => e.held = false,
            _ => {}
        }
        if e.report.playing && playing {
            let off = e.position_now().abs_diff(at);
            if off > OFF_BY_MS {
                e.lined = Some(Instant::now());
                let v = e.queue.version;
                let seek = Command::Seek { position_ms: Some(at), percent: None, by_ms: None, wind: false };
                if PlayOn::tell(e, follower, &seek, v).is_ok() {
                    tracing::debug!(target: "playon", player = follower, off, "sought back in line");
                }
            }
        }
    }

    /// A track asked of player `player`, whose account `leader` answers (decision 0079): kept waiting and told to the
    /// leader's apps, or, `auto` (the asker is in the leader's auto_accept), added at once as if accepted (back to
    /// waiting if it cannot be added now).
    #[allow(clippy::too_many_arguments)]
    pub async fn ask_for(
        self: &Arc<Self>,
        from: Asker,
        player: &str,
        leader: &str,
        track: QueueTrack,
        how: &str,
        guest: Option<String>,
        auto: bool,
    ) -> Result<QueueRequest, ToggleError> {
        if track.source.trim().is_empty() || track.id.trim().is_empty() {
            return Err(ToggleError::BadSettings("Ask for which track? Give its source and id.".into()));
        }
        // Who asked is the server's to say, as it accepts.
        let mut track = self.named(vec![QueueTrack { entry: String::new(), lane: None, n: None, asked_by: None, ..track }]).await.pop().unwrap_or_default();
        if track.name.is_empty() {
            track.name = track.id.clone();
        }
        let r = self.requests.ask(from, track, how, player, leader, guest);
        tracing::info!(target: "playon", player, from = %r.from.named(), auto, "queue request");
        if auto {
            match self.accept(&r.id, leader, false).await {
                Ok(done) => return Ok(done),
                Err(e) => tracing::info!(target: "playon", player, "queue request not added at once: {e}"),
            }
        }
        self.tell_leader(leader, format!("{} asks for {}", r.from.named(), r.track.name));
        self.changed();
        Ok(r)
    }

    /// Request `id` accepted by `by` (its leader, or an administrator): its track added to the player's queue, next or
    /// at the end as asked, through Play On's add as the leader, its entry saying who asked; the asker told.
    pub async fn accept(&self, id: &str, by: &str, admin: bool) -> Result<QueueRequest, ToggleError> {
        let r = self.waiting(id, by, admin)?;
        let track = QueueTrack { asked_by: Some(r.from.named()), ..r.track.clone() };
        let add = Command::Add { from: PlayFrom::default(), tracks: vec![track], at: None, after: None, how: Some(r.how.clone()) };
        let leader = crate::serve::who::Who { user: r.leader.clone(), admin: false, scope: crate::serve::who::Scope::Full, home: true };
        crate::serve::who::acting_as(leader, self.command(&r.player, add)).await?;
        let done = self.requests.answer(id, State::Accepted).ok_or_else(|| ToggleError::Refused("That request was answered meanwhile.".into()))?;
        tracing::info!(target: "playon", player = done.player.as_str(), from = %done.from.named(), "queue request accepted");
        self.tell_asker(&done);
        self.changed();
        Ok(done)
    }

    /// Request `id` declined by `by` (its leader, or an administrator): out of the leader's list at once, the asker
    /// told.
    pub fn decline(&self, id: &str, by: &str, admin: bool) -> Result<QueueRequest, ToggleError> {
        self.waiting(id, by, admin)?;
        let done = self.requests.answer(id, State::Declined).ok_or_else(|| ToggleError::Refused("That request was answered meanwhile.".into()))?;
        tracing::info!(target: "playon", player = done.player.as_str(), from = %done.from.named(), "queue request declined");
        self.tell_asker(&done);
        self.changed();
        Ok(done)
    }

    /// A waiting request `by` may answer.
    fn waiting(&self, id: &str, by: &str, admin: bool) -> Result<QueueRequest, ToggleError> {
        let r = self.requests.get(id).filter(|r| r.state == State::Pending).ok_or_else(|| ToggleError::Refused("That request is not waiting.".into()))?;
        if r.leader != by && !admin {
            return Err(ToggleError::Forbidden("Only its player's owner answers it.".into()));
        }
        Ok(r)
    }

    /// The asker's apps are told how their request was answered; their pages learn it from the change.
    fn tell_asker(&self, r: &QueueRequest) {
        let Asker::User(user) = &r.from else { return };
        let text = match r.state {
            State::Accepted => format!("{} added your request: {}", r.leader, r.track.name),
            State::Declined => format!("{} declined: {}", r.leader, r.track.name),
            State::Pending => return,
        };
        self.notice(user, &text);
    }

    /// One line to each of `user`'s apps that is connected: shown as a notification while the app is not in front.
    pub(super) fn notice(&self, user: &str, text: &str) {
        let first = first_owner();
        let apps: Vec<String> =
            self.entries().iter().filter(|(_, e)| e.kind == PlayerKind::App && e.online() && belongs_to(e, user, first.as_deref())).map(|(n, _)| n.clone()).collect();
        for n in apps {
            let _ = self.send(&n, &Command::Notice { text: text.to_string() });
        }
    }

    /// A new request told to its leader's apps: those coming within seconds of each other go as one notice, once they
    /// settle (`queue_requests::notices`, the same rule).
    fn tell_leader(self: &Arc<Self>, leader: &str, line: String) {
        let first = {
            let mut asked = self.asked.0.lock().unwrap_or_else(|p| p.into_inner());
            let first = !asked.contains_key(leader);
            let slot = asked.entry(leader.to_string()).or_insert_with(|| (Vec::new(), Instant::now()));
            slot.0.push(line);
            slot.1 = Instant::now();
            first
        };
        if !first {
            return;
        }
        let (me, leader) = (Arc::downgrade(self), leader.to_string());
        let settle = Duration::from_millis(SETTLE_MS as u64);
        tokio::spawn(async move {
            let mut wait = settle;
            loop {
                tokio::time::sleep(wait).await;
                let Some(me) = me.upgrade() else { return };
                let lines = {
                    let mut asked = me.asked.0.lock().unwrap_or_else(|p| p.into_inner());
                    let left = asked.get(&leader).map_or(Duration::ZERO, |(_, last)| settle.saturating_sub(last.elapsed()));
                    if left.is_zero() {
                        Some(asked.remove(&leader).map(|(l, _)| l).unwrap_or_default())
                    } else {
                        wait = left;
                        None
                    }
                };
                if let Some(lines) = lines {
                    if !lines.is_empty() {
                        me.notice(&leader, &line_of(&lines));
                    }
                    return;
                }
            }
        });
    }

    /// The player `user` listens on now (a guest's host, decision 0079): of their connected ones with something
    /// loaded, the one playing, else the one heard from last; one whose music was just handed over (and has not said it
    /// stopped yet): where it went.
    pub fn playing_of(&self, user: &str) -> Option<String> {
        let first = first_owner();
        let found = self
            .entries()
            .iter()
            .filter(|(_, e)| e.online() && e.index() >= 0 && belongs_to(e, user, first.as_deref()))
            .max_by_key(|(_, e)| (e.report.playing, std::cmp::Reverse(e.at.elapsed())))
            .map(|(n, _)| n.clone())?;
        Some(self.moved_on(&found))
    }

    /// Player `name` as no account sees it (a guest's view of what plays): none when there is none.
    pub fn view_of(&self, name: &str) -> Option<PlayerView> {
        let entries = self.entries();
        view_in(&entries, name, false)
    }
}
