//! Queue requests (decision 0079; the owner, 2026-10-30): a track asked of another listener's player, by someone
//! following it or by a guest at home, play next or at the end, waiting for its leader's Accept or Decline. Named apart
//! from `requests` (decision 0070: an album asked of the library). Kept in memory, each for [`KEEP_SECS`] after it was
//! asked: what waits that long has expired.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;

use super::playon::QueueTrack;

/// How long a request is kept, waiting or answered (s).
pub const KEEP_SECS: i64 = 2 * 3600;

/// Who asked: an account, or a guest by the name they gave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Asker {
    User(String),
    Guest(String),
}

impl Asker {
    /// As a line names them: the account's name, "Guest" and the guest's.
    pub fn named(&self) -> String {
        match self {
            Asker::User(u) => u.clone(),
            Asker::Guest(g) => format!("Guest {g}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Pending,
    Accepted,
    Declined,
}

/// One request.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueueRequest {
    pub id: String,
    pub from: Asker,
    pub track: QueueTrack,
    /// `next` or `last`.
    pub how: String,
    /// When it was asked (epoch seconds).
    pub at: i64,
    /// The player it is for.
    pub player: String,
    pub state: State,
    /// The account that answers it: the player's own (a shared device's owner).
    #[serde(skip)]
    pub leader: String,
    /// A guest's token, which their own list goes by.
    #[serde(skip)]
    pub guest: Option<String>,
}

#[derive(Default)]
struct Inner {
    list: Vec<QueueRequest>,
    /// Goes up for an account whenever a request of theirs, or to them, changes: their pages' live channel carries it.
    versions: HashMap<String, u64>,
}

#[derive(Default)]
pub struct Requests {
    inner: Mutex<Inner>,
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// A request's id: 12 random bytes, as hex.
fn new_id() -> String {
    let mut b = [0u8; 12];
    getrandom::fill(&mut b).expect("the operating system's random source");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl Inner {
    fn bump(&mut self, r: &QueueRequest) {
        for user in [Some(r.leader.as_str()), if let Asker::User(u) = &r.from { Some(u.as_str()) } else { None }].into_iter().flatten() {
            *self.versions.entry(user.to_string()).or_default() += 1;
        }
    }

    /// Those asked more than [`KEEP_SECS`] ago, out.
    fn prune(&mut self, now: i64) {
        self.list.retain(|r| now - r.at < KEEP_SECS);
    }
}

impl Requests {
    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// A new request, waiting: its id given here.
    pub fn ask(&self, from: Asker, track: QueueTrack, how: &str, player: &str, leader: &str, guest: Option<String>) -> QueueRequest {
        let r = QueueRequest {
            id: new_id(),
            from,
            track,
            how: if how == "next" { "next".into() } else { "last".into() },
            at: now_secs(),
            player: player.to_string(),
            state: State::Pending,
            leader: leader.to_string(),
            guest,
        };
        let mut inner = self.inner();
        inner.prune(now_secs());
        inner.bump(&r);
        inner.list.push(r.clone());
        r
    }

    pub fn prune(&self) {
        self.inner().prune(now_secs());
    }

    /// Request `id`, unexpired.
    pub fn get(&self, id: &str) -> Option<QueueRequest> {
        let mut inner = self.inner();
        inner.prune(now_secs());
        inner.list.iter().find(|r| r.id == id).cloned()
    }

    /// Answers a waiting request; none when there is none, or it was answered.
    pub fn answer(&self, id: &str, state: State) -> Option<QueueRequest> {
        let mut inner = self.inner();
        inner.prune(now_secs());
        let r = inner.list.iter_mut().find(|r| r.id == id && r.state == State::Pending)?;
        r.state = state;
        let r = r.clone();
        inner.bump(&r);
        Some(r)
    }

    /// Those waiting for `leader`'s answer (on `player` only, when named), the oldest first.
    pub fn waiting_for(&self, leader: &str, player: Option<&str>) -> Vec<QueueRequest> {
        let mut inner = self.inner();
        inner.prune(now_secs());
        let mut out: Vec<QueueRequest> =
            inner.list.iter().filter(|r| r.state == State::Pending && r.leader == leader && player.is_none_or(|p| r.player == p)).cloned().collect();
        out.sort_by_key(|r| r.at);
        out
    }

    /// Those waiting on player `from` wait on `to` now, answered by `leader` (its owner): the music they were asked
    /// for was handed over there (the owner, 2026-10-30), so an Accept adds to the queue that plays. How many moved.
    pub fn moved(&self, from: &str, to: &str, leader: &str) -> usize {
        let mut inner = self.inner();
        inner.prune(now_secs());
        let mut moved = Vec::new();
        for r in inner.list.iter_mut().filter(|r| r.state == State::Pending && r.player == from) {
            let before = std::mem::replace(&mut r.leader, leader.to_string());
            r.player = to.to_string();
            moved.push((before, r.clone()));
        }
        // The pages of everyone it concerns hear of it: the askers, and the answering account after and before.
        for (before, r) in &moved {
            inner.bump(r);
            if *before != r.leader {
                *inner.versions.entry(before.clone()).or_default() += 1;
            }
        }
        moved.len()
    }

    /// How many wait on `player`.
    pub fn pending_for(&self, player: &str) -> usize {
        let now = now_secs();
        self.inner().list.iter().filter(|r| r.state == State::Pending && r.player == player && now - r.at < KEEP_SECS).count()
    }

    /// `user`'s own, the newest first.
    pub fn of_user(&self, user: &str) -> Vec<QueueRequest> {
        self.of(|r| matches!(&r.from, Asker::User(u) if u == user))
    }

    /// A guest's own, by their token, the newest first.
    pub fn of_guest(&self, token: &str) -> Vec<QueueRequest> {
        self.of(|r| r.guest.as_deref() == Some(token))
    }

    fn of(&self, mine: impl Fn(&QueueRequest) -> bool) -> Vec<QueueRequest> {
        let mut inner = self.inner();
        inner.prune(now_secs());
        let mut out: Vec<QueueRequest> = inner.list.iter().filter(|r| mine(r)).cloned().collect();
        out.sort_by_key(|r| std::cmp::Reverse(r.at));
        out
    }

    /// Goes up whenever a request of `user`'s, or to them, changes (none: anyone's, for one user without accounts).
    pub fn version_of(&self, user: Option<&str>) -> u64 {
        let inner = self.inner();
        match user {
            Some(u) => inner.versions.get(u).copied().unwrap_or(0),
            None => inner.versions.values().sum(),
        }
    }

    /// As if request `id` had been asked `secs` earlier (the tests' clock).
    #[cfg(test)]
    pub fn age(&self, id: &str, secs: i64) {
        if let Some(r) = self.inner().list.iter_mut().find(|r| r.id == id) {
            r.at -= secs;
        }
    }
}

/// How long new requests to one leader wait for more before they are told as one notice (ms).
pub const SETTLE_MS: i64 = 5_000;

/// The notices new requests make (decision 0079): those within [`SETTLE_MS`] of the one before go as one, sent once
/// none follows for that long: its own line when it is one ("Ann asks for So What"), else how many. Each request as
/// (when, ms; its line); the notices as (when sent, ms; the line). Pure, for the tests: the leader's timer
/// (`PlayOn::tell_leader`) runs the same rule as it goes.
#[cfg(test)]
pub fn notices(asked: &[(i64, String)]) -> Vec<(i64, String)> {
    let mut sorted: Vec<&(i64, String)> = asked.iter().collect();
    sorted.sort_by_key(|(at, _)| *at);
    let mut out = Vec::new();
    let mut burst: Vec<&(i64, String)> = Vec::new();
    for a in sorted {
        if burst.last().is_some_and(|(last, _)| a.0 - last >= SETTLE_MS) {
            out.push(told(&burst));
            burst.clear();
        }
        burst.push(a);
    }
    if !burst.is_empty() {
        out.push(told(&burst));
    }
    out
}

#[cfg(test)]
fn told(burst: &[&(i64, String)]) -> (i64, String) {
    let last = burst.last().map_or(0, |(at, _)| *at);
    (last + SETTLE_MS, line_of(&burst.iter().map(|(_, l)| l.clone()).collect::<Vec<_>>()))
}

/// One notice for these requests' lines: the line itself for one, else how many.
pub fn line_of(lines: &[String]) -> String {
    match lines {
        [one] => one.clone(),
        many => format!("{} new requests", many.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(name: &str) -> QueueTrack {
        QueueTrack { source: "cratarr".into(), id: format!("tr-{name}"), name: name.into(), ..QueueTrack::default() }
    }

    #[test]
    fn requests_wait_for_their_leader_are_answered_once_and_expire() {
        let r = Requests::default();
        let a = r.ask(Asker::User("bob".into()), track("So What"), "next", "Salon", "ann", None);
        let g = r.ask(Asker::Guest("Sam".into()), track("Blue"), "whatever", "Salon", "ann", Some("tok".into()));
        assert_eq!((a.state, g.how.as_str()), (State::Pending, "last"));
        assert_eq!(r.waiting_for("ann", None).iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), [a.id.as_str(), g.id.as_str()]);
        assert_eq!(r.waiting_for("ann", Some("Phone")).len(), 0, "another player's");
        assert_eq!(r.pending_for("Salon"), 2);
        assert!(r.version_of(Some("bob")) > 0 && r.version_of(Some("ann")) >= 2);
        let before = r.version_of(Some("bob"));
        assert_eq!(r.answer(&a.id, State::Declined).map(|x| x.state), Some(State::Declined));
        assert!(r.version_of(Some("bob")) > before, "the asker's pages are told");
        assert_eq!(r.answer(&a.id, State::Accepted), None, "answered once");
        assert_eq!(r.of_user("bob")[0].state, State::Declined, "the asker sees it declined");
        assert_eq!(r.of_guest("tok").len(), 1);
        r.age(&g.id, KEEP_SECS);
        assert_eq!(r.get(&g.id), None, "expired after two hours");
        assert_eq!(r.pending_for("Salon"), 0);
        let json = serde_json::to_value(&r.of_user("bob")[0]).unwrap();
        assert_eq!((json["from"]["user"].as_str(), json["state"].as_str()), (Some("bob"), Some("declined")));
        assert!(json.get("leader").is_none() && json.get("guest").is_none(), "nor whose token");
    }

    #[test]
    fn requests_waiting_on_a_player_move_with_its_music() {
        let r = Requests::default();
        let a = r.ask(Asker::User("bob".into()), track("So What"), "next", "Ann phone", "ann", None);
        let done = r.ask(Asker::User("bob".into()), track("Blue"), "last", "Ann phone", "ann", None);
        r.answer(&done.id, State::Declined);
        let other = r.ask(Asker::User("bob".into()), track("Green"), "last", "Kitchen", "ann", None);
        let before = (r.version_of(Some("ann")), r.version_of(Some("bob")));
        assert_eq!(r.moved("Ann phone", "Salon", "ann"), 1, "the waiting one only");
        assert_eq!(r.get(&a.id).map(|x| x.player), Some("Salon".to_string()));
        assert_eq!(r.get(&done.id).map(|x| x.player), Some("Ann phone".to_string()), "an answered one stays");
        assert_eq!(r.get(&other.id).map(|x| x.player), Some("Kitchen".to_string()));
        assert_eq!((r.pending_for("Ann phone"), r.pending_for("Salon")), (0, 1));
        assert!(r.version_of(Some("ann")) > before.0 && r.version_of(Some("bob")) > before.1, "their pages are told");
        // To another's shared device: its owner answers now.
        assert_eq!(r.moved("Salon", "Carl tablet", "carl"), 1);
        assert_eq!(r.waiting_for("carl", None).len(), 1);
        assert_eq!(r.waiting_for("ann", None).iter().map(|x| x.player.as_str()).collect::<Vec<_>>(), ["Kitchen"], "hers on her own player still wait");
    }

    #[test]
    fn requests_close_together_are_told_as_one_notice() {
        let line = |s: &str| s.to_string();
        // One alone: its own line, five seconds after it.
        assert_eq!(notices(&[(1_000, line("Ann asks for So What"))]), [(6_000, line("Ann asks for So What"))]);
        // Three within seconds of each other, then one a minute later.
        let asked = [
            (10_000, line("Ann asks for A")),
            (12_000, line("Guest Sam asks for B")),
            (16_500, line("Ann asks for C")),
            (80_000, line("Bob asks for D")),
        ];
        assert_eq!(notices(&asked), [(21_500, line("3 new requests")), (85_000, line("Bob asks for D"))]);
        assert!(notices(&[]).is_empty());
    }
}
