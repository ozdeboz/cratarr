//! A Soulseek item as one machine (DOMAIN-CORE §4.2 and C.2; SCENARIOS-DC2 SKM): what one record's fetch from Soulseek is, every
//! state typed, its source named by its user and folder (never its place in a list), who picked it, and its holds typed. `next()`
//! alone gives an item's new state and what its carrier does; the scheduler's rules across items (one download per Soulseek user,
//! one check at a time) are the carrier's.
//!
//! - **A Pick** is a person's or the certain rule's (`Picker`): nothing is fetched without one, and Cratarr's own only where certain
//!   (C.2 check 1). After a user fails, the next source by key goes on under the same pick, Cratarr's only where that one is
//!   certain too (SK3.7).
//! - **Holds** are typed and use no attempt (KM4): a source paused until when, slskd away, the floor, AcoustID away, a placing that
//!   failed, local failures, a switch off, background reading paused, or an error's own words (`Hold::Other`).
//! - **Cancel** (SC01) goes back to Found with the sources kept and blames no one; a placing is never cancelled (Y10, KM6); Take it
//!   anyway (SC03) places a set-aside folder; Take off keeps the record dropped (X2).
//! - **TN's failure class** is the machine's failure event (KM12): `Failure`.
//!
//! No I/O, no clock: every time is handed in.

use serde::{Deserialize, Serialize};

/// Epoch seconds.
pub type Time = i64;

/// A source: a user's folder, by its key (R2; never a place in a list).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceKey {
    pub user: String,
    pub folder: String,
}

impl SourceKey {
    pub fn new(user: &str, folder: &str) -> SourceKey {
        SourceKey { user: user.into(), folder: folder.into() }
    }
}

/// A source as a search found it: its key, and whether the certain rule takes it (C.2's Certain row, judged by the carrier).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub key: SourceKey,
    #[serde(default)]
    pub certain: bool,
}

/// Who picked (R3): an administrator (none where 0.17 kept no account for a tick), or Cratarr by the certain rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", content = "account", rename_all = "snake_case")]
pub enum Picker {
    Administrator(Option<String>),
    Cratarr,
}

/// An item's state (C.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "is", rename_all = "snake_case")]
pub enum State {
    /// Searched again at `due` (0: now).
    Waiting { due: Time },
    Searching,
    /// Sources found: waits for a pick where none is certain (F20).
    Found,
    Picked { source: SourceKey, by: Picker, at: Time },
    Fetching { source: SourceKey, by: Picker, percent: u8 },
    /// Downloaded, waiting its turn to be checked (KM5).
    Fetched { source: SourceKey, by: Picker },
    /// Checked file by file (SC02): `done` of `of`.
    Checking { source: SourceKey, by: Picker, done: u16, of: u16 },
    /// Past its rename's note: never cancelled (Y10); a failed one is placed again from its checked files (X4).
    Placing,
    Placed,
    Stuck { why: String, due: Time },
}

impl State {
    /// The source it fetches from, where it has one.
    pub fn source(&self) -> Option<&SourceKey> {
        match self {
            State::Picked { source, .. } | State::Fetching { source, .. } | State::Fetched { source, .. } | State::Checking { source, .. } => Some(source),
            _ => None,
        }
    }

    /// Something of it is under way that a Cancel stops (SC01): picked, fetching, fetched or being checked.
    pub fn cancellable(&self) -> bool {
        matches!(self, State::Picked { .. } | State::Fetching { .. } | State::Fetched { .. } | State::Checking { .. })
    }
}

/// Why an item is held (KM4), typed: never found by comparing words. A hold uses no attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "hold", rename_all = "snake_case")]
pub enum Hold {
    /// A source asked Cratarr to wait (AcoustID, MusicBrainz, Discogs): until then.
    Paused { source: String, until: Time },
    /// slskd could not be reached, in its own words.
    SlskdAway { said: String },
    /// The server is short of space: no user's transfer begins (AL03.2).
    Floor,
    /// The asker is over their room (SK3.5).
    Room,
    /// AcoustID could not be asked at the check, in its words.
    AcoustidAway { said: String },
    /// A placing failed (Y12): tried again at `until`; a day after three.
    PlaceFailed { tries: u8, until: Time },
    /// Three local failures in a row (Y9): a day.
    LocalFailures { until: Time },
    /// slskd switched off.
    SlskdOff,
    /// Background reading paused (Y7).
    BackgroundPaused,
    /// An error's own words no kind names (R8: 0.17's, carried as they were).
    Other { said: String },
}

impl Hold {
    /// Until when it holds, where it has a deadline (TN.12: "Paused until" only then).
    pub fn until(&self) -> Option<Time> {
        match self {
            Hold::Paused { until, .. } | Hold::PlaceFailed { until, .. } | Hold::LocalFailures { until } => Some(*until),
            _ => None,
        }
    }
}

/// An item of Soulseek's list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub record: i64,
    pub sources: Vec<Source>,
    /// Users who failed this record: never asked for it again.
    #[serde(default)]
    pub failed: Vec<String>,
    /// Folders set aside for a real fault, by their key.
    #[serde(default)]
    pub set_aside: Vec<SourceKey>,
    /// Searches in a row that found nothing.
    #[serde(default)]
    pub misses: u8,
    #[serde(default)]
    pub local_failures: u8,
    #[serde(default)]
    pub place_failures: u8,
    pub state: State,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held: Option<Hold>,
    /// A person cancelled Cratarr's own pick (OQ-DC2-2 (a), CA6): Cratarr picks nothing for it until it is searched again.
    #[serde(default)]
    pub cancelled: bool,
}

impl Item {
    /// A new item of `record`, due now.
    pub fn new(record: i64) -> Item {
        Item { record, sources: Vec::new(), failed: Vec::new(), set_aside: Vec::new(), misses: 0, local_failures: 0, place_failures: 0, state: State::Waiting { due: 0 }, held: None, cancelled: false }
    }
}

/// TN.1's failure class (KM12), as the machine takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The user refused or failed: the next source, else Waiting.
    User,
    /// The file: a transcode, a wrong recording, broken or missing files: set aside, its user failed.
    File,
    /// Nothing there: a miss.
    NotFound,
    /// Away, no deadline: held.
    Away(String),
    /// Paused until then: held, no attempt used.
    Paused { source: String, until: Time },
    Stuck(String),
    /// None of these: the reader's own handling (held with its words).
    Other(String),
}

/// What happens to an item (C.2's events).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Its time came (Waiting, Stuck): searched, its list read first.
    Due,
    /// The search found these sources; nothing; no list to judge by.
    Answered(Vec<Source>),
    Missed { again: Time },
    NoList { why: String, again: Time },
    /// The certain rule's pick (G2, G3, G6, G9, H2, SK3.4: judged by the carrier).
    Certain(SourceKey, Time),
    /// An administrator's pick.
    Pick { source: SourceKey, by: Option<String>, at: Time },
    /// Its batch asked of slskd.
    Started,
    Progress(u8),
    /// Its download done.
    Downloaded,
    /// Its check's turn: `of` files.
    CheckTurn { of: u16 },
    Checked(u16),
    /// Every file passed: placed next.
    Passed,
    Failed(Failure),
    PlaceFailed { until: Time, day: Time },
    Placed,
    /// Held, using no attempt (a pause, slskd away, the floor …).
    Hold(Hold),
    Unhold,
    Cancel,
    TakeItAnyway(SourceKey),
    TakeOff,
    /// Its record had for the house, or no open request names it: off the list.
    RecordHad,
    NoOpenRequest,
    /// Its placed copy gone, after a completed pass (U7).
    CopyGone,
}

/// What the carrier does after `next()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Does {
    Nothing,
    /// Read its list (where it has none), then search.
    Search,
    /// Ask slskd for its source's batch (one per user at a time: the carrier's).
    Enqueue,
    /// Place its checked files.
    Place,
    /// Set its source's folder aside, its user failed (a real fault).
    SetAside(SourceKey),
    /// Drop its transfers and remove what was fetched of them (a Cancel, Take off, its record had).
    DropTransfers,
    /// Place a set-aside folder (Take it anyway).
    PlaceSetAside(SourceKey),
    /// Off the list; `dropped`: its record stays dropped while a request names it (X2).
    TakeOff { dropped: bool },
}

/// Why an event is refused (the answer's 409 or 404), in the words the answer gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// A placing is never cancelled or taken off (Y10, KM6).
    BeingPlaced,
    /// A Cancel of nothing under way (CA7 (d)).
    NothingUnderWay,
    /// A pick or a take naming a source the item doesn't offer (`SOURCE_GONE`).
    SourceGone,
    /// A take of a folder not set aside (SC03.8).
    NotSetAside,
    /// A take of an item placed already (TA3 (a)).
    AlreadyPlaced,
}

impl Refused {
    pub fn words(self) -> &'static str {
        match self {
            Refused::BeingPlaced => "It is being placed",
            Refused::NothingUnderWay => "Nothing under way",
            Refused::SourceGone => "That source is gone; look again",
            Refused::NotSetAside => "No such folder set aside",
            Refused::AlreadyPlaced => "Already in Incoming",
        }
    }
}

/// How many placings in a row may fail before a day's hold (Y12).
pub const PLACE_AT_MOST: u8 = 3;

/// An item's next state and what its carrier does (C.2's table), or why the event is refused. Pure.
pub fn next(i: &Item, e: Event) -> Result<(Item, Does), Refused> {
    let mut n = i.clone();
    let done = |n: Item, does: Does| Ok((n, does));
    match (&i.state, e) {
        // Placed: only its copy gone sends it back (U7); nothing else moves it, Take off included (kept as history, OQ-DC2-1 (a)).
        (State::Placed, Event::CopyGone) => {
            n.state = State::Waiting { due: 0 };
            done(n, Does::Nothing)
        }
        (State::Placed, Event::Cancel) => Err(Refused::NothingUnderWay),
        (State::Placed, Event::TakeItAnyway(_)) => Err(Refused::AlreadyPlaced),
        // Out of the list by a person (CA9: on every item but Placing): its record stays dropped (X2).
        (State::Placed, Event::TakeOff) => done(n, Does::TakeOff { dropped: true }),
        (State::Placed, _) => done(n, Does::Nothing),
        // Placing: never cancelled, never taken off; it ends placed, or held and placed again from its checked files.
        (State::Placing, Event::Cancel | Event::TakeOff | Event::TakeItAnyway(_)) => Err(Refused::BeingPlaced),
        (State::Placing, Event::Placed) => {
            (n.state, n.held, n.place_failures, n.local_failures) = (State::Placed, None, 0, 0);
            done(n, Does::Nothing)
        }
        (State::Placing, Event::PlaceFailed { until, day }) => {
            n.place_failures = n.place_failures.saturating_add(1);
            let until = if n.place_failures >= PLACE_AT_MOST { day } else { until };
            n.held = Some(Hold::PlaceFailed { tries: n.place_failures, until });
            done(n, Does::Nothing)
        }
        (State::Placing, Event::Unhold) => {
            n.held = None;
            done(n, Does::Place)
        }
        (State::Placing, Event::Hold(h)) => {
            n.held = Some(h);
            done(n, Does::Nothing)
        }
        (State::Placing, _) => done(n, Does::Nothing),
        // Any other state.
        (_, Event::TakeOff) => done(n, Does::TakeOff { dropped: true }),
        (_, Event::RecordHad | Event::NoOpenRequest) => done(n, Does::TakeOff { dropped: false }),
        (_, Event::Hold(h)) => {
            n.held = Some(h);
            done(n, Does::Nothing)
        }
        (_, Event::Unhold) => {
            n.held = None;
            done(n, Does::Nothing)
        }
        (_, Event::TakeItAnyway(key)) => {
            if !i.set_aside.contains(&key) {
                return Err(Refused::NotSetAside);
            }
            // Its fetch, where one is under way, is cancelled first (TA3 (b)); its user is failed no more.
            n.failed.retain(|u| *u != key.user);
            (n.state, n.held) = (State::Placing, None);
            done(n, Does::PlaceSetAside(key))
        }
        (state, Event::Cancel) => {
            if !state.cancellable() {
                return Err(Refused::NothingUnderWay);
            }
            // Back to Found with its sources kept, nobody blamed, no attempt used (SC01). Cratarr's own pick cancelled is not
            // picked again by Cratarr until a new search (CA6).
            n.cancelled = matches!(state, State::Picked { by: Picker::Cratarr, .. } | State::Fetching { by: Picker::Cratarr, .. } | State::Fetched { by: Picker::Cratarr, .. } | State::Checking { by: Picker::Cratarr, .. })
                || i.cancelled;
            (n.state, n.held) = (State::Found, None);
            done(n, if matches!(state, State::Picked { .. }) { Does::Nothing } else { Does::DropTransfers })
        }
        (State::Waiting { .. } | State::Stuck { .. }, Event::Due) => {
            n.state = State::Searching;
            done(n, Does::Search)
        }
        (State::Searching, Event::Answered(sources)) => {
            // A new search: the cancel's mark goes (CA6), the failed users' folders never come back.
            n.sources = sources.into_iter().filter(|s| !i.failed.contains(&s.key.user)).collect();
            (n.misses, n.cancelled) = (0, false);
            n.state = if n.sources.is_empty() { State::Waiting { due: 0 } } else { State::Found };
            done(n, Does::Nothing)
        }
        (State::Searching, Event::Missed { again }) => {
            n.misses = n.misses.saturating_add(1);
            n.state = State::Waiting { due: again };
            done(n, Does::Nothing)
        }
        // A search's own failure: nothing there is a miss; a pause or a source away waits, using no attempt (KM12, KM13).
        (State::Searching, Event::Failed(Failure::NotFound)) => {
            n.misses = n.misses.saturating_add(1);
            n.state = State::Waiting { due: 0 };
            done(n, Does::Nothing)
        }
        (State::Searching, Event::Failed(Failure::Paused { source, until })) => {
            (n.state, n.held) = (State::Waiting { due: until }, Some(Hold::Paused { source, until }));
            done(n, Does::Nothing)
        }
        (State::Searching, Event::Failed(Failure::Stuck(why))) => {
            n.state = State::Stuck { why, due: 0 };
            done(n, Does::Nothing)
        }
        (State::Searching, Event::Failed(Failure::Away(said) | Failure::Other(said))) => {
            (n.state, n.held) = (State::Waiting { due: 0 }, Some(Hold::Other { said }));
            done(n, Does::Nothing)
        }
        (State::Searching, Event::NoList { why, again }) => {
            n.state = State::Stuck { why, due: again };
            done(n, Does::Nothing)
        }
        (State::Found, Event::Certain(source, at)) => {
            // Cratarr's own pick: only a source it offers and the rule judged certain, never after a person's Cancel (CA6).
            if i.cancelled || !i.sources.iter().any(|s| s.key == source && s.certain) {
                return done(n, Does::Nothing);
            }
            n.state = State::Picked { source, by: Picker::Cratarr, at };
            done(n, Does::Nothing)
        }
        (State::Found, Event::Pick { source, by, at }) => {
            if !i.sources.iter().any(|s| s.key == source) {
                return Err(Refused::SourceGone);
            }
            n.state = State::Picked { source, by: Picker::Administrator(by), at };
            done(n, Does::Nothing)
        }
        (State::Picked { source, by, .. }, Event::Started) => {
            (n.state, n.held) = (State::Fetching { source: source.clone(), by: by.clone(), percent: 0 }, None);
            done(n, Does::Enqueue)
        }
        (State::Fetching { source, by, .. }, Event::Progress(p)) => {
            n.state = State::Fetching { source: source.clone(), by: by.clone(), percent: p.min(100) };
            done(n, Does::Nothing)
        }
        (State::Fetching { source, by, .. }, Event::Downloaded) => {
            (n.state, n.local_failures) = (State::Fetched { source: source.clone(), by: by.clone() }, 0);
            done(n, Does::Nothing)
        }
        (State::Fetched { source, by }, Event::CheckTurn { of }) => {
            n.state = State::Checking { source: source.clone(), by: by.clone(), done: 0, of };
            done(n, Does::Nothing)
        }
        (State::Checking { source, by, done: was, of }, Event::Checked(k)) => {
            // A count never goes back nor past its total while one check runs (CK1).
            let k = k.clamp(*was, *of);
            n.state = State::Checking { source: source.clone(), by: by.clone(), done: k, of: *of };
            done(n, Does::Nothing)
        }
        (State::Checking { .. }, Event::Passed) => {
            n.state = State::Placing;
            done(n, Does::Place)
        }
        (state, Event::Failed(f)) if state.source().is_some() => {
            let source = state.source().cloned().expect("a source");
            match f {
                Failure::User => done(failed_user(i, &source, None), Does::DropTransfers),
                Failure::File => {
                    let mut n = failed_user(i, &source, None);
                    n.set_aside.push(source.clone());
                    done(n, Does::SetAside(source))
                }
                // A pause or a source away holds it as it is: no attempt used, nobody blamed (KM4, KM12).
                Failure::Paused { source, until } => {
                    n.held = Some(Hold::Paused { source, until });
                    done(n, Does::Nothing)
                }
                Failure::Away(said) => {
                    n.held = Some(Hold::SlskdAway { said });
                    done(n, Does::Nothing)
                }
                Failure::Other(said) => {
                    n.held = Some(Hold::Other { said });
                    done(n, Does::Nothing)
                }
                // slskd no longer knows its batch (a restart): fetched again from the same source, nobody blamed; the carrier's count of
                // batches forgotten in a row fails its user the second time (OQ-C3 (a)).
                Failure::NotFound => {
                    let by = match state {
                        State::Picked { by, .. } | State::Fetching { by, .. } | State::Fetched { by, .. } | State::Checking { by, .. } => by.clone(),
                        _ => Picker::Cratarr,
                    };
                    (n.state, n.held) = (State::Picked { source, by, at: 0 }, None);
                    done(n, Does::Nothing)
                }
                Failure::Stuck(why) => {
                    n.state = State::Stuck { why, due: 0 };
                    done(n, Does::DropTransfers)
                }
            }
        }
        _ => done(n, Does::Nothing),
    }
}

/// After `source`'s user failed: the next source by key under the same pick (KM2), Cratarr's only where that one is certain too,
/// else Found, no longer Cratarr's (SK3.7); with no source left, Waiting, due now.
fn failed_user(i: &Item, source: &SourceKey, _why: Option<&str>) -> Item {
    let mut n = i.clone();
    if !n.failed.contains(&source.user) {
        n.failed.push(source.user.clone());
    }
    n.sources.retain(|s| !n.failed.contains(&s.key.user));
    let by = match &i.state {
        State::Picked { by, .. } | State::Fetching { by, .. } | State::Fetched { by, .. } | State::Checking { by, .. } => by.clone(),
        _ => Picker::Cratarr,
    };
    n.held = None;
    n.state = match n.sources.first() {
        None => State::Waiting { due: 0 },
        Some(next) if by == Picker::Cratarr && !next.certain => State::Found,
        Some(next) => State::Picked { source: next.key.clone(), by, at: 0 },
    };
    n
}

/// Where an item is held at `now`: its hold while it lasts (a deadline past holds no more). Worked out once, here (KM4).
pub fn held_at(i: &Item, now: Time) -> Option<&Hold> {
    i.held.as_ref().filter(|h| h.until().is_none_or(|until| until > now))
}

#[cfg(test)]
mod tests;
