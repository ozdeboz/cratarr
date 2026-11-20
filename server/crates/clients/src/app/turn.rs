//! One turn per outside service (TN: CU14, DI02 and DI03 as one; the owner's T1 and T2, 2026-11-17). Every question to a service
//! waits its turn, `apart` after the one before whoever asked it, and none goes while the service asked to be left alone. Its limit
//! answer (a 429, MusicBrainz's 503, AcoustID's code 14, GitHub's spent limit) holds every asker, those already waiting too (Y8),
//! for its Retry-After where it says (a quarter of an hour at most), else for the source's own default, doubling in a row up to its
//! cap; any other answer ends a run, and a hold only grows. Whoever asks can read until when it holds (`paused_until`), and an
//! asker with patience (a page's, MusicBrainz's background) is told so at once rather than held past it.
//!
//! And the one failure class (TN.1): what an error of any service comes to for whoever asked. A user's (another may serve), the
//! file's own, nothing there, the service away (no deadline), paused until when, or stuck until someone acts.
//!
//! slskd has no turn here: its turn is its lane (`slskd`, 0.17's round 1.3), where its "one operation at a time" 429 is slskd
//! busy, asked again, and never a limit answer.
//!
//! A server's turn keeps its hold in the journal's file, its source's row in `source_pauses` (RN's DI01; SCENARIOS-RN R4: one place
//! says a source's hold, where 0.17 kept `<source>:held_until` in the user data), and takes it up again at the start (`keep`), cut
//! to its cap: a restart asks nothing early.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use crate::intake::Pause;
use crate::journal::Journal;

use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use tokio::time::Instant;

/// How a source is asked, and waited for when it asks to be left alone (OQ-T2 (a), TN.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// Its connection's name, as `reach` knows it.
    pub name: &'static str,
    /// Between two of its questions, whoever asks.
    pub apart: Duration,
    /// Its limit answer without a Retry-After holds it this long, doubling in a row up to `most` (as long as `first`: the same
    /// each time).
    pub first: Duration,
    pub most: Duration,
    /// The longest a deadline of its own may hold: GitHub's reset, Bandcamp's bot check. A Retry-After holds `RETRY_AFTER_MOST` at
    /// most everywhere.
    pub longest: Duration,
    /// The statuses that are its limit answer (beside GitHub's 403 with none left, and AcoustID's code 14 under any status).
    pub limits: &'static [u16],
}

/// A Retry-After is held to this at most (TN.2), whatever it says.
pub const RETRY_AFTER_MOST: Duration = Duration::from_secs(15 * 60);
const MINUTE: Duration = Duration::from_secs(60);

/// MusicBrainz (S10, O5): one question a second from one address, a little more apart so two never fall in one second; its 503 (a
/// 429 alike) 5 s doubling to a minute, as built.
pub const MUSICBRAINZ: Policy =
    Policy { name: "musicbrainz", apart: Duration::from_millis(1100), first: Duration::from_secs(5), most: MINUTE, longest: RETRY_AFTER_MOST, limits: &[429, 503] };
/// AcoustID (O6): three questions a second; its limit (a 429, or its code 14 under any status) a minute doubling to a quarter hour.
pub const ACOUSTID: Policy = Policy { name: "acoustid", apart: Duration::from_millis(350), first: MINUTE, most: RETRY_AFTER_MOST, longest: RETRY_AFTER_MOST, limits: &[429] };
/// Discogs: its anonymous pace until a reply says its limit (`discogs::gap`); its 429 a minute doubling to a quarter hour.
pub const DISCOGS: Policy = Policy { name: "discogs", apart: Duration::from_millis(2450), first: MINUTE, most: RETRY_AFTER_MOST, longest: RETRY_AFTER_MOST, limits: &[429] };
/// Bandcamp (O4): a page a second; its 429 and 503 a minute each time; its bot check a day.
pub const BANDCAMP: Policy =
    Policy { name: "bandcamp", apart: Duration::from_secs(1), first: MINUTE, most: MINUTE, longest: Duration::from_secs(86_400), limits: &[429, 503] };
/// GitHub's releases (UC01): its 429 a minute doubling to a quarter hour; its spent limit (a 403 with none left) until its reset,
/// an hour at most.
pub const GITHUB: Policy = Policy { name: "github", apart: Duration::ZERO, first: MINUTE, most: RETRY_AFTER_MOST, longest: Duration::from_secs(3600), limits: &[429] };
/// Last.fm (O6): four questions a second; its rate limit (its error 29, in a body) a minute each time.
pub const LASTFM: Policy = Policy { name: "lastfm", apart: Duration::from_millis(250), first: MINUTE, most: MINUTE, longest: RETRY_AFTER_MOST, limits: &[] };

/// Every source's policy.
const POLICIES: [&Policy; 6] = [&MUSICBRAINZ, &ACOUSTID, &DISCOGS, &BANDCAMP, &GITHUB, &LASTFM];

/// A source's policy, by its connection's name.
pub fn policy(name: &str) -> Option<&'static Policy> {
    POLICIES.into_iter().find(|p| p.name == name)
}

/// Where 0.17 kept a source's hold across a restart (OQ-T3): the user data's server choice `<source>:held_until`, epoch seconds
/// ("": none). Read once, by 0.18's step (its `pauses` part, SCENARIOS-RN DM5), which moves each into `source_pauses`.
pub fn held_choice(source: &str) -> String {
    format!("{source}:held_until")
}

/// Every source with a turn of the server's own, by its connection's name: those whose kept holds 0.18's step moves (DM5).
pub const KEPT: [&str; 6] = ["musicbrainz", "acoustid", "bandcamp", "discogs", "lastfm", "github"];

/// Every turn of the server's own kept in the journal's file (R4), each hold kept before a restart taken up first (`keep`): at the
/// start, before anything is asked. `theirs`: the app's own turns (Discogs', Last.fm's), which are above this crate. slskd has no turn
/// here (its lane, `slskd`): its 429 is asked again, never held, and it has no row.
pub fn keep_all(journal: &Arc<Journal>, theirs: &[&Turn]) {
    for turn in [&super::musicbrainz::TURN, &crate::acoustid::TURN, &crate::bandcamp::TURN].into_iter().chain(theirs.iter().copied()) {
        turn.keep(journal.clone());
    }
}

/// The wait a limit answer's Retry-After asks (TN.2): its seconds, or the HTTP date it names, from `now` (epoch s). None where it is
/// absent, 0, negative, past or neither (the source's default then); `RETRY_AFTER_MOST` at most. Pure.
pub fn retry_after(headers: &HeaderMap, now: i64) -> Option<Duration> {
    let said = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?.trim();
    let secs = match said.parse::<i64>() {
        Ok(secs) => secs,
        Err(_) => http_date(said)? - now,
    };
    (secs > 0).then(|| Duration::from_secs(secs as u64).min(RETRY_AFTER_MOST))
}

/// An HTTP date as senders write it ("Wed, 21 Oct 2026 07:28:00 GMT", RFC 9110's IMF-fixdate), epoch seconds. Pure.
fn http_date(s: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let (_, rest) = s.split_once(", ")?;
    let parts: Vec<&str> = rest.split(' ').collect();
    let [day, month, year, time, "GMT"] = parts[..] else { return None };
    let month = MONTHS.iter().position(|m| *m == month)? + 1;
    if day.len() != 2 || year.len() != 4 {
        return None;
    }
    let start = super::requests_turn::release_day(&format!("{year}-{month:02}-{day}"))?;
    let hms: Vec<i64> = time.split(':').map(|n| n.parse().ok()).collect::<Option<_>>()?;
    let [h, m, s] = hms[..] else { return None };
    ((0..24).contains(&h) && (0..60).contains(&m) && (0..61).contains(&s)).then_some(start + h * 3600 + m * 60 + s)
}

/// Whether an answer is `p`'s source asking to be left alone (TN.1), and for how long it said: nothing said (its default then),
/// its Retry-After, or GitHub's reset (to its `longest`). None: no limit answer. Pure.
pub fn limit_said(p: &Policy, status: StatusCode, headers: &HeaderMap, now: i64) -> Option<Option<Duration>> {
    if p.name == GITHUB.name && status == StatusCode::FORBIDDEN {
        let number = |name: &str| headers.get(name)?.to_str().ok()?.trim().parse::<i64>().ok();
        if number("x-ratelimit-remaining") != Some(0) {
            return None;
        }
        return Some(number("x-ratelimit-reset").map(|at| at - now).filter(|w| *w > 0).map(|w| Duration::from_secs(w as u64).min(p.longest)));
    }
    p.limits.contains(&status.as_u16()).then(|| retry_after(headers, now))
}

/// A source's questions in turn, whoever asks (S10, O6, TN), each `apart` after the one before: a page, a pass and a fetch asking
/// at once share the source between them, which a sleep of each caller's own could not. They go in the order they came, but for the
/// background asker's (`behind`, MusicBrainz's): one of those goes only when no other question waits, so a page asking meanwhile goes
/// first, however long the pass. The wait is before a question, not after it, so one asked after a quiet while goes at once. None
/// goes while the source is held (`heard`), those already waiting too (Y8).
pub struct Turn {
    policy: Policy,
    /// Between two of its questions now (ms): its policy's, or what the source last said its limit is (Discogs', TN.4).
    apart: AtomicU64,
    /// When the last question went; none before the first. The next goes `apart` after it.
    last: tokio::sync::Mutex<Option<Instant>>,
    /// Questions waiting that are not the background's.
    waiting: AtomicUsize,
    /// Told when none waits any more.
    quiet: tokio::sync::Notify,
    /// The run of limit answers' last wait (TN.3), none after any other answer.
    run: std::sync::Mutex<Option<Duration>>,
    held: std::sync::Mutex<Option<Hold>>,
    /// Where its hold is kept across a restart (OQ-T3, R4): the journal's file, for the server's own turns (`keep`).
    kept: std::sync::Mutex<Option<Arc<Journal>>>,
}

/// A turn's hold.
#[derive(Debug, Clone, Copy)]
struct Hold {
    /// The instant no question goes before.
    at: Instant,
    /// The same as epoch seconds: what an asker is told.
    until: i64,
    /// A deadline of the source's own (`hold_until`: Bandcamp's bot check's day), not a limit answer's.
    own: bool,
}

/// A question waiting that is not the background's, counted while it waits, a question dropped meanwhile too.
struct Waiting<'t>(&'t Turn);

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        if self.0.waiting.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.quiet.notify_waiters();
        }
    }
}

impl Turn {
    pub const fn new(policy: Policy) -> Self {
        Self {
            policy,
            apart: AtomicU64::new(policy.apart.as_millis() as u64),
            last: tokio::sync::Mutex::const_new(None),
            waiting: AtomicUsize::new(0),
            quiet: tokio::sync::Notify::const_new(),
            run: std::sync::Mutex::new(None),
            held: std::sync::Mutex::new(None),
            kept: std::sync::Mutex::new(None),
        }
    }

    /// Its questions `apart` apart from the next on (Discogs' limit as its answers say it, TN.4).
    pub fn set_apart(&self, apart: Duration) {
        self.apart.store(apart.as_millis() as u64, Ordering::Relaxed);
    }

    fn apart(&self) -> Duration {
        Duration::from_millis(self.apart.load(Ordering::Relaxed))
    }

    /// Waits for a question's turn and takes it, a hold waited out however long.
    pub async fn turn(&self) {
        self.take(false).await;
    }

    /// Waits for a question's turn (the background's `behind` any other) and takes it, a hold waited out however long.
    pub async fn take(&self, behind: bool) {
        let _ = self.taken(behind, None).await;
    }

    /// As `take`, but a hold out past `patience` from now is not waited for (OQ-T1 (a)): the asker is told until when at once, and
    /// so too when one comes while it waits.
    pub async fn take_within(&self, behind: bool, patience: Duration) -> Result<(), Paused> {
        self.taken(behind, Some(Instant::now() + patience)).await
    }

    /// `take`, refusing a hold out past `by`. One `behind` waits while another waits, and holds no place in the line while it sleeps:
    /// woken, it looks again, so a question that came meanwhile goes before it.
    async fn taken(&self, behind: bool, by: Option<Instant>) -> Result<(), Paused> {
        let past = |held: Option<Instant>| matches!((by, held), (Some(by), Some(held)) if held > by);
        if !behind {
            self.waiting.fetch_add(1, Ordering::SeqCst);
            let _waiting = Waiting(self);
            let mut last = self.last.lock().await;
            // The turn's own instant and the hold's, looked at again on waking: a hold set meanwhile holds on (Y8), a pace said
            // meanwhile paces (TN.4).
            loop {
                let held = self.held_at();
                if past(held) {
                    return Err(self.paused());
                }
                match last.map(|l| l + self.apart()).max(held).filter(|at| *at > Instant::now()) {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => break,
                }
            }
            *last = Some(Instant::now());
            return Ok(());
        }
        loop {
            let quiet = self.quiet.notified();
            if self.waiting.load(Ordering::SeqCst) > 0 {
                quiet.await;
                continue;
            }
            let mut last = self.last.lock().await;
            let held = self.held_at();
            if past(held) {
                return Err(self.paused());
            }
            match last.map(|l| l + self.apart()).max(held) {
                Some(at) if at > Instant::now() => {
                    drop(last);
                    tokio::time::sleep_until(at).await;
                }
                _ if self.waiting.load(Ordering::SeqCst) > 0 => {}
                _ => {
                    *last = Some(Instant::now());
                    return Ok(());
                }
            }
        }
    }

    /// An answer heard (TN.3, O5): its limit (`limit`) holds every asker, those already waiting too, for `said` (its Retry-After or
    /// reset, capped) where it said, else for the source's default, `first` doubling in a row to `most`; any other answer ends the
    /// run. Held at once, not behind the questions waiting for the turn's lock. The wait, after a limit answer.
    pub fn heard(&self, limit: bool, said: Option<Duration>) -> Option<Duration> {
        let wait = {
            let mut run = self.run.lock().unwrap_or_else(|p| p.into_inner());
            if !limit {
                *run = None;
                return None;
            }
            let base = run.map_or(self.policy.first, |d| (d * 2).min(self.policy.most));
            *run = Some(base);
            said.unwrap_or(base)
        };
        self.hold_for(wait);
        tracing::warn!(target: "connections", source = self.policy.name, secs = wait.as_secs(), "a source asked Cratarr to wait: its questions wait");
        Some(wait)
    }

    /// Its limit answer heard (`heard`), for `said`: the pause, to tell whoever asked.
    pub fn limited(&self, said: Option<Duration>) -> Paused {
        self.heard(true, said);
        self.paused()
    }

    /// An answer's status and headers heard (`limit_said`, `heard`): its limit answer the pause, to tell whoever asked.
    pub fn heard_answer(&self, status: StatusCode, headers: &HeaderMap) -> Result<(), Paused> {
        match limit_said(&self.policy, status, headers, super::moment::now_secs()) {
            Some(said) => Err(self.limited(said)),
            None => {
                self.heard(false, None);
                Ok(())
            }
        }
    }

    /// No question for `wait`, whoever asks (a limit answer's, or Discogs' moving minute spent): a later hold set already stays (a
    /// hold only grows). Kept where the turn is (OQ-T3).
    pub fn hold_for(&self, wait: Duration) {
        let at = Instant::now() + wait;
        let until = super::moment::now_secs() + wait.as_secs() as i64 + i64::from(wait.subsec_nanos() > 0);
        if self.grow(Hold { at, until, own: false }) {
            self.keep_until(None, Some(until));
        }
    }

    /// No question until `until` (epoch s), whoever asks: a deadline of the source's own (Bandcamp's bot check's day), not waited
    /// out by an asker's patience. A later hold set already stays. Kept in `ud` where given, else where the turn is (OQ-T3).
    pub fn hold_until(&self, until: i64, ud: Option<&Journal>) {
        let now = super::moment::now_secs();
        if until <= now {
            return;
        }
        self.grow(Hold { at: Instant::now() + Duration::from_secs((until - now) as u64), until, own: true });
        let kept = self.held.lock().unwrap_or_else(|p| p.into_inner()).map(|h| h.until);
        self.keep_until(ud, kept);
    }

    /// The hold `h` set where it ends later than the one there (a hold only grows): whether it was.
    fn grow(&self, h: Hold) -> bool {
        let mut held = self.held.lock().unwrap_or_else(|p| p.into_inner());
        let later = held.is_none_or(|was| was.at < h.at);
        if later {
            *held = Some(h);
        }
        later
    }

    /// Its hold lifted, the run of limit answers over (Bandcamp switched on again, V8): asked again at once, in memory and in the
    /// journal `ud` where given, else where the turn is kept, so a restart takes up nothing.
    pub fn lift(&self, ud: Option<&Journal>) {
        *self.held.lock().unwrap_or_else(|p| p.into_inner()) = None;
        *self.run.lock().unwrap_or_else(|p| p.into_inner()) = None;
        self.keep_until(ud, None);
    }

    /// Until when (epoch s) its own hold (`hold_until`) holds at `now`, where it does. Never longer than its `longest` from `now`
    /// (U11): one set while the clock ran ahead is cut to that as it is read, and held so.
    pub fn own_until(&self, now: i64) -> Option<i64> {
        let mut held = self.held.lock().unwrap_or_else(|p| p.into_inner());
        let h = held.as_mut().filter(|h| h.own)?;
        let most = now + self.policy.longest.as_secs() as i64;
        if h.until > most {
            h.at = h.at.checked_sub(Duration::from_secs((h.until - most) as u64)).unwrap_or(h.at);
            h.until = most;
        }
        (h.until > now).then_some(h.until)
    }

    /// Its holds kept in the journal's file from now on (OQ-T3, R4), the one kept before a restart taken up first (`take_up`): at
    /// the start.
    pub fn keep(&self, journal: Arc<Journal>) {
        self.take_up(&journal);
        *self.kept.lock().unwrap_or_else(|p| p.into_inner()) = Some(journal);
    }

    /// The hold kept in the journal's file before a restart taken up (OQ-T3, R4), so nothing is asked of the source before it is out:
    /// never longer than its `longest` from now (V8), one kept while the clock ran ahead cut to that, in its row too, and never
    /// lengthened. One further off than any limit answer holds (`RETRY_AFTER_MOST`) is the source's own (Bandcamp's day). Until
    /// when it holds.
    pub fn take_up(&self, journal: &Journal) -> Option<i64> {
        let _ = journal;
        None
    }

    /// Its hold's deadline (none: lifted) kept for a restart in its row (R4), in the journal `ud` where given, else where the turn is
    /// kept; nowhere otherwise. A hold only grows: a shorter one leaves the row as it is.
    fn keep_until(&self, ud: Option<&Journal>, until: Option<i64>) {
        let _ = (ud, until, Pause { source: String::new(), until: 0, why: None, own: false });
    }

    /// Its source's connection name.
    pub fn source(&self) -> &'static str {
        self.policy.name
    }

    /// The instant its hold ends, where one was set.
    pub fn held_at(&self) -> Option<Instant> {
        self.held.lock().unwrap_or_else(|p| p.into_inner()).map(|h| h.at)
    }

    /// Until when it holds (epoch s), while it does.
    pub fn paused_until(&self) -> Option<i64> {
        self.held.lock().unwrap_or_else(|p| p.into_inner()).filter(|h| h.at > Instant::now()).map(|h| h.until)
    }

    /// The pause it holds for, as the error an asker is told: its source, and until when.
    pub fn paused(&self) -> Paused {
        let until = self.held.lock().unwrap_or_else(|p| p.into_inner()).map_or_else(super::moment::now_secs, |h| h.until);
        Paused { source: self.policy.name, until }
    }
}

/// Tests: a turn's hold as it was, to put back (`switches::TEST_STATE`'s rule for the state the process shares).
#[cfg(any(test, feature = "test-support"))]
pub struct Held(Option<Hold>);

#[cfg(any(test, feature = "test-support"))]
impl Turn {
    pub fn as_held(&self) -> Held {
        Held(*self.held.lock().unwrap_or_else(|p| p.into_inner()))
    }

    pub fn put_back(&self, was: Held) {
        *self.held.lock().unwrap_or_else(|p| p.into_inner()) = was.0;
    }
}

/// A source asked to be left alone, until when (epoch s) (TN.1): raised where its limit answer is heard, and to whoever its turn
/// refuses meanwhile. It says nothing of what was asked for: no user, file or record is held against for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paused {
    pub source: &'static str,
    pub until: i64,
}

impl std::fmt::Display for Paused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} asked Cratarr to wait", super::reach::label(self.source))
    }
}

impl std::error::Error for Paused {}

/// A source's refusal by its status, in its own words (Bandcamp's "Bandcamp answered 404 Not Found", AcoustID's or GitHub's): read
/// by its status (TN.1).
#[derive(Debug)]
pub struct Refused {
    pub source: &'static str,
    pub status: StatusCode,
    words: String,
}

impl Refused {
    pub fn new(source: &'static str, status: StatusCode, words: String) -> Self {
        Self { source, status, words }
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.words)
    }
}

impl std::error::Error for Refused {}

/// What cannot go on until someone acts (TN.1): no list to judge a record by, no library to put it in. Reads as why.
#[derive(Debug)]
pub struct Stuck(pub String);

impl std::fmt::Display for Stuck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Stuck {}

/// What a failure comes to for whoever asked (TN.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The one asked (a user of Soulseek) refused it or failed: another may not.
    User,
    /// The file itself: frames that would not decode, a length not the recording's, a flaw, another recording.
    File,
    /// The source has no such thing.
    NotFound,
    /// The source cannot be asked now, with no deadline: down, not answering, its key refused, switched off, background reading
    /// paused. In its own words.
    Away(String),
    /// The source asked to be left alone until then (epoch s).
    Paused { until: i64 },
    /// Nothing goes on until someone acts.
    Stuck,
    /// None of these (an answer not understood): each reader keeps what it did with such an error.
    Other,
}

/// What an error comes to (TN.1), now.
pub fn class(e: &anyhow::Error) -> Failure {
    class_at(e, super::moment::now_secs())
}

/// What an error comes to at `now` (epoch s), by the first of its causes one of the services' kinds names. Pure.
pub fn class_at(e: &anyhow::Error, now: i64) -> Failure {
    e.chain().find_map(|cause| class_of(cause, now)).unwrap_or(Failure::Other)
}

/// One cause's class, where it is one of the kinds the services raise. Pure.
fn class_of(cause: &(dyn std::error::Error + 'static), now: i64) -> Option<Failure> {
    let away = || Some(Failure::Away(cause.to_string()));
    if let Some(p) = cause.downcast_ref::<Paused>() {
        return Some(Failure::Paused { until: p.until });
    }
    // Bandcamp's bot check: left alone a day (O4).
    if cause.is::<crate::bandcamp::NotRead>() {
        return Some(Failure::Paused { until: now + BANDCAMP.longest.as_secs() as i64 });
    }
    if cause.is::<crate::slskd::Declined>() {
        return Some(Failure::User);
    }
    // slskd busy (a read's 429, its lane's operations ask again) or an answer lost (read back by its id): nothing of anyone's.
    if cause.is::<crate::slskd::Busy>() || cause.is::<crate::slskd::Lost>() {
        return away();
    }
    if cause.is::<crate::slskd::NotKnown>() {
        return Some(Failure::NotFound);
    }
    // The gate's AcoustID not asked: its pause where AcoustID asked to be left alone, else away.
    if let Some(n) = cause.downcast_ref::<super::track_fetch_turn::NotAsked>() {
        return Some(n.1.map_or_else(|| Failure::Away(cause.to_string()), |until| Failure::Paused { until }));
    }
    if cause.is::<crate::slskd::Away>() || cause.is::<super::references_turn::SwitchedOff>() {
        return away();
    }
    if cause.is::<super::track_fetch_turn::Unfit>() {
        return Some(Failure::File);
    }
    if cause.is::<Stuck>() {
        return Some(Failure::Stuck);
    }
    if let Some(p) = cause.downcast_ref::<crate::acoustid::Problem>() {
        return Some(match p.code {
            // Its fingerprint or its length refused: the file's.
            crate::acoustid::BAD_PRINT | crate::acoustid::BAD_DURATION => Failure::File,
            crate::acoustid::LIMIT => Failure::Paused { until: now + ACOUSTID.first.as_secs() as i64 },
            // Its key refused, its service unavailable, or Cratarr's own question refused: nothing of the file.
            _ => Failure::Away(cause.to_string()),
        });
    }
    if let Some(r) = cause.downcast_ref::<Refused>() {
        return Some(of_status(Some(r.source), r.status, now));
    }
    if let Some(r) = cause.downcast_ref::<reqwest::Error>() {
        let source = r.url().and_then(super::reach::name_of);
        let who = super::reach::label(source.unwrap_or_default());
        if r.is_timeout() {
            return Some(Failure::Away(format!("{who} did not answer in time")));
        }
        if r.is_connect() {
            return Some(Failure::Away(format!("{who} could not be reached")));
        }
        return r.status().map(|s| of_status(source, s, now));
    }
    if cause.to_string() == super::switches::PAUSED {
        return away();
    }
    None
}

/// A source's answer of `status` as a failure (TN.1): 404 and 400 nothing there; every 429, and MusicBrainz's 503, a pause for the
/// source's default (the error carries no Retry-After); 401, 403 and any other refusal or server error away. Pure.
fn of_status(source: Option<&'static str>, status: StatusCode, now: i64) -> Failure {
    let default = |p: Option<&Policy>| now + p.map_or(MINUTE, |p| p.first).as_secs() as i64;
    match status.as_u16() {
        400 | 404 | 410 => Failure::NotFound,
        429 => Failure::Paused { until: default(source.and_then(policy)) },
        503 if source == Some(MUSICBRAINZ.name) => Failure::Paused { until: default(Some(&MUSICBRAINZ)) },
        400..=599 => Failure::Away(format!("{} answered {status}", super::reach::label(source.unwrap_or_default()))),
        _ => Failure::Other,
    }
}

/// Tests: the epoch clock (`moment::PINNED`) held at its time now until dropped, so deadlines are exact. Under `TEST_STATE`.
#[cfg(any(test, feature = "test-support"))]
pub struct Pinned(pub i64);

#[cfg(any(test, feature = "test-support"))]
impl Pinned {
    pub fn now() -> Self {
        let now = super::moment::now_secs();
        super::moment::PINNED.store(now, Ordering::Relaxed);
        Pinned(now)
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for Pinned {
    fn drop(&mut self) {
        super::moment::PINNED.store(0, Ordering::Relaxed);
    }
}

/// Tests: the fakes the turns' tests share.
#[cfg(any(test, feature = "test-support"))]
pub mod fakes {
    /// A server of the test's own on this machine, answering as `app` does: its address.
    pub async fn serve(app: axum::Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let at = format!("http://{}", listener.local_addr().unwrap());
        crate::run::spawn(async move { axum::serve(listener, app).await.unwrap() });
        at
    }

    /// MusicBrainz's global limiter's 503 as captured on 2026-11-17 (its body and headers, `tests/fixtures/listings`), its
    /// Retry-After as it was (1), as `retry_after` says, or none.
    pub fn musicbrainz_busy(retry_after: Option<Option<&str>>) -> axum::response::Response {
        use axum::http::{HeaderName, HeaderValue, StatusCode};
        use axum::response::IntoResponse;
        let head = include_str!("../../../../tests/fixtures/listings/mb-503-global.headers");
        let body = include_str!("../../../../tests/fixtures/listings/mb-503-global.json").trim().to_string();
        let mut r = (StatusCode::SERVICE_UNAVAILABLE, body).into_response();
        for line in head.lines().skip(1) {
            let Some((name, value)) = line.split_once(':') else { continue };
            let name = name.trim().to_ascii_lowercase();
            // What the connection was, not the answer: hyper sets its own.
            if ["transfer-encoding", "connection", "keep-alive", "date"].contains(&name.as_str()) {
                continue;
            }
            r.headers_mut().insert(HeaderName::from_bytes(name.as_bytes()).unwrap(), HeaderValue::from_str(value.trim()).unwrap());
        }
        if let Some(said) = retry_after {
            r.headers_mut().remove(axum::http::header::RETRY_AFTER);
            if let Some(said) = said {
                r.headers_mut().insert(axum::http::header::RETRY_AFTER, HeaderValue::from_str(said).unwrap());
            }
        }
        r
    }

    /// Headers holding these.
    pub fn headers(pairs: &[(&'static str, String)]) -> reqwest::header::HeaderMap {
        let mut h = reqwest::header::HeaderMap::new();
        for (name, value) in pairs {
            h.insert(*name, value.parse().unwrap());
        }
        h
    }

    /// A failure as compared here: Away's words left out.
    pub fn kind(f: super::Failure) -> super::Failure {
        match f {
            super::Failure::Away(_) => super::Failure::Away(String::new()),
            f => f,
        }
    }

    /// A print as the tests' AcoustID lookups take it.
    pub fn print() -> crate::acoustid::Print {
        crate::acoustid::Print { fingerprint: "AQAAT0mUaEkSRZEGAA".into(), duration_secs: 241.0, damaged: 0, flaws: Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use super::*;

    use super::fakes::{headers, print};

    /// O6 (moved from `reach.rs`, its `Pacer` folded into the turn): every asker's questions `apart` apart, and none while it is held,
    /// whoever asks.
    #[tokio::test(start_paused = true)]
    async fn a_pacer_spaces_every_askers_questions_and_holds_them_all() {
        let gap = Duration::from_millis(250);
        let p = Arc::new(Turn::new(Policy { apart: gap, ..LASTFM }));
        let askers: Vec<_> = (0..5)
            .map(|_| {
                let p = p.clone();
                crate::run::spawn(async move {
                    p.turn().await;
                    Instant::now()
                })
            })
            .collect();
        let mut at = Vec::new();
        for a in askers {
            at.push(a.await.unwrap());
        }
        at.sort();
        assert!(at.windows(2).all(|w| w[1] - w[0] >= gap), "{at:?}");
        p.hold_for(Duration::from_secs(60));
        let before = Instant::now();
        p.turn().await;
        assert!(Instant::now() - before >= Duration::from_secs(59), "held for everyone");
    }

    /// Y8 (moved from `reach.rs`): a hold holds the askers already waiting for their turn too, not only the next to come.
    #[tokio::test(start_paused = true)]
    async fn a_pacer_hold_holds_the_askers_already_waiting() {
        async fn went(p: &Turn, start: Instant) -> Duration {
            p.turn().await;
            Instant::now() - start
        }
        let (p, start) = (Turn::new(Policy { apart: Duration::from_millis(250), ..LASTFM }), Instant::now());
        p.turn().await;
        let held = async {
            tokio::task::yield_now().await;
            p.hold_for(Duration::from_secs(60));
        };
        let (a, b, ()) = tokio::join!(went(&p, start), went(&p, start), held);
        assert!(a.min(b) >= Duration::from_secs(60), "{a:?} {b:?}");
    }

    /// AcoustID's answers in turn, as scripted (TN.2's fake, standing in for its address): each lookup's instant kept. A lookup under
    /// 333 ms after the one before, one while a 429's Retry-After holds, one whose `meta` is not `recordingids`, or one without the key
    /// in its form, is a violation.
    #[derive(Default)]
    struct AcoustidFake {
        script: Mutex<VecDeque<(u16, Option<&'static str>)>>,
        asked: Mutex<Vec<Instant>>,
        held: Mutex<Option<Instant>>,
        violations: Mutex<Vec<String>>,
    }

    impl AcoustidFake {
        fn script(answers: &[(u16, Option<&'static str>)]) -> Self {
            Self { script: Mutex::new(answers.iter().copied().collect()), ..Default::default() }
        }

        fn answer(&self, form: Vec<(&'static str, String)>) -> std::future::Ready<anyhow::Result<crate::acoustid::Heard>> {
            let now = Instant::now();
            let mut violations = self.violations.lock().unwrap();
            let field = |k: &str| form.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
            if field("meta") != Some("recordingids") || field("client") != Some("key") {
                violations.push(format!("asked {form:?}"));
            }
            let mut asked = self.asked.lock().unwrap();
            if asked.last().is_some_and(|last| now - *last < Duration::from_millis(333)) {
                violations.push(format!("two lookups under 333 ms apart at {now:?}"));
            }
            if self.held.lock().unwrap().is_some_and(|until| now < until) {
                violations.push(format!("a lookup while held at {now:?}"));
            }
            asked.push(now);
            let (status, said) = self.script.lock().unwrap().pop_front().unwrap_or((200, None));
            let mut h = HeaderMap::new();
            if let Some(said) = said {
                h.insert(reqwest::header::RETRY_AFTER, said.parse().unwrap());
                if let Ok(secs) = said.parse::<u64>() {
                    *self.held.lock().unwrap() = Some(now + Duration::from_secs(secs));
                }
            }
            let body = if status == 429 { r#"{"status":"error","error":{"code":14,"message":"rate limit exceeded"}}"# } else { r#"{"status":"ok","results":[]}"# };
            std::future::ready(Ok(crate::acoustid::Heard { status: StatusCode::from_u16(status).unwrap(), headers: h, body: body.to_string() }))
        }
    }

    /// TN.2: AcoustID's 429 with its Retry-After holds its turn: the lookup that met it is told until when; the two asked with it wait,
    /// none asked before then, and go after it, as does the first asked again, each a turn apart. Every asker reads until when while it
    /// holds. Retry-After as a date is that instant; 0, -5 or words are the source's default; a day is cut to a quarter hour; a hold
    /// only grows.
    #[tokio::test(start_paused = true)]
    async fn a_429_holds_the_turn_for_its_retry_after() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let t0 = Pinned::now();
        let t0 = t0.0;
        let turn = Turn::new(ACOUSTID);
        let fake = AcoustidFake::script(&[(429, Some("30")), (200, None), (200, None), (200, None)]);
        let print = print();
        let ask = || crate::acoustid::lookup_in(&turn, |form| fake.answer(form), "key", &print);
        let start = Instant::now();
        let read_meanwhile = async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            turn.paused_until()
        };
        let (a, b, c, meanwhile) = tokio::join!(ask(), ask(), ask(), read_meanwhile);
        assert!(matches!(class(&a.unwrap_err()), Failure::Paused { until } if until == t0 + 30));
        assert!(b.is_ok() && c.is_ok(), "the two that waited are answered");
        assert_eq!(meanwhile, Some(t0 + 30), "every caller reads until when while it holds");
        ask().await.unwrap();
        let sent = fake.asked.lock().unwrap().clone();
        assert_eq!(sent.len(), 4);
        assert!(sent[1] - sent[0] >= Duration::from_secs(30) && sent.windows(2).skip(1).all(|w| w[1] - w[0] >= Duration::from_millis(350)), "{sent:?}");
        assert!(sent[1] - start >= Duration::from_secs(30), "nothing asked before t0+30");
        assert!(fake.violations.lock().unwrap().is_empty(), "{:?}", fake.violations.lock().unwrap());
        assert_eq!(turn.paused_until(), None, "its hold is out");

        // Retry-After's forms.
        let date = 1_792_567_680; // Wed, 21 Oct 2026 07:28:00 GMT
        let said = |v: &str, now: i64| retry_after(&headers(&[("retry-after", v.to_string())]), now);
        assert_eq!(said("Wed, 21 Oct 2026 07:28:00 GMT", date - 120), Some(Duration::from_secs(120)), "a date is that instant");
        for v in ["0", "-5", "soon", "Wed, 21 Oct 2026 07:28:00 GMT"] {
            assert_eq!(said(v, date), None, "{v}: the source's default");
        }
        assert_eq!(said("86400", t0), Some(RETRY_AFTER_MOST), "a day cut to a quarter hour");
        let held = Turn::new(ACOUSTID);
        let limit = |v: &str| held.limited(retry_after(&headers(&[("retry-after", v.to_string())]), t0));
        assert_eq!(limit("0").until, t0 + 60, "0: the source's default");
        assert_eq!(limit("86400").until, t0 + 900, "cut to a quarter hour");
        assert_eq!(limit("5").until, t0 + 900, "a later answer never shortens the hold");
        assert_eq!(held.paused_until(), Some(t0 + 900));
    }

    /// TN.3 (OQ-T2 (a)): each source's limit answered five times in a row without Retry-After, then once ok, then again: MusicBrainz
    /// 5 s doubling to a minute; AcoustID, Discogs and GitHub a minute doubling to a quarter hour; Bandcamp a minute each time,
    /// its bot check a day. Never under its first, never doubled past its cap, no run carried over an ok.
    #[test]
    fn a_pause_without_retry_after_doubles_to_its_cap() {
        let s = |n: &[u64]| n.iter().map(|n| Duration::from_secs(*n)).collect::<Vec<_>>();
        let cases = [
            (MUSICBRAINZ, s(&[5, 10, 20, 40, 60, 60])),
            (ACOUSTID, s(&[60, 120, 240, 480, 900, 900])),
            (DISCOGS, s(&[60, 120, 240, 480, 900, 900])),
            (GITHUB, s(&[60, 120, 240, 480, 900, 900])),
            (BANDCAMP, s(&[60, 60, 60, 60, 60, 60])),
        ];
        for (p, want) in cases {
            let t = Turn::new(p);
            let waits: Vec<Duration> = (0..6).map(|_| t.heard(true, None).unwrap()).collect();
            assert_eq!(waits, want, "{}", p.name);
            assert!(waits.iter().all(|w| *w >= p.first && *w <= p.most), "{}", p.name);
            assert_eq!(t.heard(false, None), None, "{}: an ok", p.name);
            assert_eq!(t.heard(true, None), Some(p.first), "{}: no run carried over an ok", p.name);
        }
        assert_eq!(policy("bandcamp").map(|p| p.longest), Some(Duration::from_secs(86_400)), "Bandcamp's bot check holds a day");
        assert_eq!(class_at(&crate::bandcamp::NotRead.into(), 1_800_000_000), Failure::Paused { until: 1_800_000_000 + 86_400 });
        assert!(POLICIES.iter().all(|p| policy(p.name) == Some(*p) && p.first <= p.most && p.most <= RETRY_AFTER_MOST.max(p.longest)), "every source declared once");
    }

    /// TN.9 (OQ-T3), the turn's half: a restart mid-pause asks nothing early. AcoustID held until t0+600 (its 429 saying 600), Discogs
    /// until t0+300 (its 429 saying 300), Bandcamp for its bot check's day, each kept in its `source_pauses` row as it is set (RN's R4;
    /// SCENARIOS-RN DS3's pin: read from the rows, its assertions unchanged); the server stops at t0+60 and starts at t0+120: each hold
    /// is taken up again, never lengthened, and nothing is asked before it is out. A kept deadline beyond its cap (kept while the clock
    /// ran ahead) is cut to it as it is taken up, in its row too.
    #[tokio::test(start_paused = true)]
    async fn a_restart_mid_pause_asks_nothing_early() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let t0 = 1_800_000_000;
        let _pinned = Pinned(t0);
        let clock = |secs: i64| super::super::moment::PINNED.store(t0 + secs, Ordering::Relaxed);
        clock(0);
        let journal = Arc::new(Journal::open(None).unwrap());
        let kept = |source: &str| journal.pause(source).unwrap().map(|p| p.until);
        let made = || {
            let turns = (Turn::new(ACOUSTID), Turn::new(DISCOGS), Turn::new(BANDCAMP));
            for turn in [&turns.0, &turns.1, &turns.2] {
                turn.keep(journal.clone());
            }
            turns
        };
        let fake = AcoustidFake::script(&[(429, Some("600"))]);
        let print = print();
        let start = Instant::now();

        // Before the stop: each source's limit (Bandcamp's check) holds its turn, kept as it is set.
        let (acoustid, discogs, bandcamp) = made();
        let limited = crate::acoustid::lookup_in(&acoustid, |form| fake.answer(form), "key", &print).await;
        assert_eq!(class(&limited.unwrap_err()), Failure::Paused { until: t0 + 600 });
        let rate = discogs.heard_answer(StatusCode::TOO_MANY_REQUESTS, &headers(&[("retry-after", "300".into())]));
        assert_eq!(rate, Err(Paused { source: "discogs", until: t0 + 300 }));
        bandcamp.hold_until(t0 + 86_400, None);
        assert_eq!([kept("acoustid"), kept("discogs"), kept("bandcamp")], [Some(t0 + 600), Some(t0 + 300), Some(t0 + 86_400)], "kept as each is set");
        assert!(journal.pause("bandcamp").unwrap().is_some_and(|p| p.own), "Bandcamp's its own deadline, in its row");

        // The server stops at t0+60 and starts at t0+120: each turn made anew, its hold taken up from its row.
        tokio::time::advance(Duration::from_secs(60)).await;
        clock(60);
        drop((acoustid, discogs, bandcamp));
        tokio::time::advance(Duration::from_secs(60)).await;
        clock(120);
        let (acoustid, discogs, bandcamp) = made();
        let held = [acoustid.paused_until(), discogs.paused_until(), bandcamp.paused_until()];
        assert_eq!(held, [Some(t0 + 600), Some(t0 + 300), Some(t0 + 86_400)], "taken up, never lengthened");

        // Nothing asked early: AcoustID not before t0+600, Discogs not before t0+300, Bandcamp not before its day is out.
        let asked_discogs = async {
            discogs.turn().await;
            Instant::now()
        };
        let looked = crate::acoustid::lookup_in(&acoustid, |form| fake.answer(form), "key", &print);
        let (discogs_at, looked) = tokio::join!(asked_discogs, looked);
        assert!(looked.is_ok());
        let sent = fake.asked.lock().unwrap().clone();
        assert_eq!(sent.len(), 2);
        assert!(sent[1] - start >= Duration::from_secs(600), "AcoustID asked at {:?}", sent[1] - start);
        assert!(discogs_at - start >= Duration::from_secs(300), "Discogs asked at {:?}", discogs_at - start);
        assert_eq!(bandcamp.take_within(false, Duration::from_secs(10)).await, Err(Paused { source: "bandcamp", until: t0 + 86_400 }));
        assert!(fake.violations.lock().unwrap().is_empty(), "{:?}", fake.violations.lock().unwrap());

        // Kept beyond its cap (the clock ran ahead): cut to it as it is taken up, in the user data too.
        clock(600);
        let now = t0 + 600;
        for (source, far) in [("acoustid", t0 + 5_000), ("discogs", t0 + 5_000), ("bandcamp", t0 + 30 * 86_400)] {
            journal.set_pause(&Pause { source: source.into(), until: far, why: None, own: source == "bandcamp" }).unwrap();
        }
        let (acoustid, discogs, bandcamp) = made();
        let held = [acoustid.paused_until(), discogs.paused_until(), bandcamp.paused_until()];
        assert_eq!(held, [Some(now + 900), Some(now + 900), Some(now + 86_400)], "each cut to its cap");
        assert_eq!([kept("acoustid"), kept("discogs"), kept("bandcamp")], held, "cut in its row too");
    }

    /// DS3 (DI01's `a_pause_reads_back_after_reopening`): AcoustID held until t0+600, Bandcamp for its bot check's day (its own
    /// deadline), MusicBrainz until t0+30, each in its `source_pauses` row as the turn sets it, a shorter hold later leaving the longer.
    /// The server stops and starts at t0+60 (the journal's file opened again): AcoustID holds to t0+600, MusicBrainz to nothing,
    /// Bandcamp to its day, and nothing is asked before its kept deadline. Bandcamp switched on again: its row is gone, in memory and in
    /// the file. One kept while the clock ran ahead is cut to its `longest`, never lengthened. Never a row for slskd, nor a hold in the
    /// user data (R4: one place says a source's hold).
    #[tokio::test(start_paused = true)]
    async fn a_pause_reads_back_after_reopening() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let t0 = 1_800_000_000;
        let _pinned = Pinned(t0);
        let clock = |secs: i64| super::super::moment::PINNED.store(t0 + secs, Ordering::Relaxed);
        clock(0);
        let d = std::env::temp_dir().join(format!("cratarr-turn-ds3-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        let file = d.join("journal.sqlite");
        let made = |journal: &Arc<Journal>| {
            let turns = (Turn::new(ACOUSTID), Turn::new(BANDCAMP), Turn::new(MUSICBRAINZ));
            for turn in [&turns.0, &turns.1, &turns.2] {
                turn.keep(journal.clone());
            }
            turns
        };
        let start = Instant::now();
        {
            let journal = Arc::new(Journal::open(Some(&file)).unwrap());
            let (acoustid, bandcamp, musicbrainz) = made(&journal);
            acoustid.heard(true, Some(Duration::from_secs(600)));
            bandcamp.hold_until(t0 + 86_400, None);
            musicbrainz.heard(true, Some(Duration::from_secs(30)));
            acoustid.heard(true, Some(Duration::from_secs(100)));
            let rows = journal.pauses().unwrap();
            let read: Vec<(&str, i64, bool)> = rows.iter().map(|p| (p.source.as_str(), p.until, p.own)).collect();
            assert_eq!(read, [("acoustid", t0 + 600, false), ("bandcamp", t0 + 86_400, true), ("musicbrainz", t0 + 30, false)], "a shorter hold later leaves the longer");
            assert_eq!(rows[0].why.as_deref(), Some("AcoustID asked Cratarr to wait"));
        }

        // The server stops and starts at t0+60: the journal's file opened again, each turn made anew.
        tokio::time::advance(Duration::from_secs(60)).await;
        clock(60);
        let journal = Arc::new(Journal::open(Some(&file)).unwrap());
        let (acoustid, bandcamp, musicbrainz) = made(&journal);
        assert_eq!([acoustid.paused_until(), bandcamp.paused_until(), musicbrainz.paused_until()], [Some(t0 + 600), Some(t0 + 86_400), None]);
        assert_eq!(acoustid.take_within(false, Duration::from_secs(10)).await, Err(Paused { source: "acoustid", until: t0 + 600 }), "not asked before its deadline");
        musicbrainz.turn().await;
        assert!(Instant::now() - start < Duration::from_secs(61), "MusicBrainz's hold is past: asked at once");
        acoustid.turn().await;
        assert!(Instant::now() - start >= Duration::from_secs(600), "AcoustID asked at {:?}", Instant::now() - start);
        clock(600);

        // Bandcamp switched off and on again (V8): its hold lifted in memory and in the file; a restart takes up nothing.
        bandcamp.lift(None);
        assert_eq!((bandcamp.paused_until(), journal.pause("bandcamp").unwrap()), (None, None), "its row gone");
        assert_eq!(Turn::new(BANDCAMP).take_up(&journal), None);

        // Kept while the clock ran ahead: cut to its longest as it is taken up, in its row too, never lengthened.
        journal.set_pause(&Pause { source: "bandcamp".into(), until: t0 + 600 + 30 * 86_400, why: None, own: true }).unwrap();
        let again = Turn::new(BANDCAMP);
        assert_eq!(again.take_up(&journal), Some(t0 + 600 + 86_400));
        assert_eq!(journal.pause("bandcamp").unwrap().map(|p| p.until), Some(t0 + 600 + 86_400), "cut in its row");

        assert!(journal.pauses().unwrap().iter().all(|p| p.source != "slskd"), "no row for slskd: its 429 is busy, never a pause");
        let here = include_str!("turn.rs");
        let code = here.split("#[cfg(test)]").next().unwrap();
        assert!(!code.contains(&["set_import", "_choice"].concat()) && !code.contains(&["import", "_choice("].concat()), "no hold kept in the user data");
        drop(journal);
        std::fs::remove_dir_all(&d).ok();
    }

}
