//! A job's states (DOMAIN-CORE §4.7; Oban's): waiting, scheduled, running, retrying, done, failed, cancelled, and the one pure
//! function that walks them (SCENARIOS-RN JR1). Done, failed and cancelled are final.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// For a resource (its turn, memory, a source's pause), or for room.
    Waiting,
    /// At a time (`Job::at`).
    Scheduled,
    Running,
    /// Failed in a way its kind tries again, at a time (`Job::at`).
    Retrying,
    Done,
    Failed,
    Cancelled,
}

impl State {
    pub fn is_final(self) -> bool {
        matches!(self, State::Done | State::Failed | State::Cancelled)
    }

    /// How the journal's row keeps it.
    pub fn as_str(self) -> &'static str {
        match self {
            State::Waiting => "waiting",
            State::Scheduled => "scheduled",
            State::Running => "running",
            State::Retrying => "retrying",
            State::Done => "done",
            State::Failed => "failed",
            State::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<State> {
        [State::Waiting, State::Scheduled, State::Running, State::Retrying, State::Done, State::Failed, State::Cancelled].into_iter().find(|x| x.as_str() == s)
    }
}

/// What happens to a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Its turn came (a resource let it in): it starts, where it may.
    Start,
    /// It said where it is.
    Step,
    /// It failed in a way its kind tries again (a retry at `again`).
    FailAgain,
    /// It failed for good.
    FailFinal,
    /// Its time came.
    Due,
    /// Someone asked it to stop: its work stops before its next step (`Handle::stopped`).
    CancelAsked,
    /// It stopped.
    Cancel,
    /// It ended well.
    Done,
}

/// A job as the walk sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Job {
    pub state: State,
    /// Scheduled or retrying: when (epoch s).
    pub at: i64,
    /// Asked to stop while it runs.
    pub cancel_asked: bool,
}

impl Job {
    pub fn new(state: State, at: i64) -> Self {
        Job { state, at, cancel_asked: false }
    }
}

/// What `e` at `now` makes of `job`. `again`: when a failure its kind tries again is tried again (none: its kind tries nothing
/// again, so it fails). Pure.
pub fn walk(job: Job, e: Event, now: i64, again: Option<i64>) -> Job {
    use State::{Cancelled, Failed, Retrying, Running, Scheduled, Waiting};
    if job.state.is_final() || job.state != State::Done {
        let _ = (e, now, again);
        return job;
    }
    if job.state.is_final() {
        return job;
    }
    let to = |state: State, at: i64| Job { state, at, cancel_asked: false };
    match (job.state, e) {
        // A cancel of a job not running ends it at once; of one running, it is asked and its work stops before its next step.
        (Running, Event::CancelAsked) => Job { cancel_asked: true, ..job },
        (Running, Event::Step) if job.cancel_asked => to(Cancelled, 0),
        (_, Event::Cancel) | (Waiting | Scheduled | Retrying, Event::CancelAsked) => to(Cancelled, 0),
        (Waiting, Event::Start) => to(Running, 0),
        // A scheduled job, or one retrying, runs only at or after its time.
        (Scheduled | Retrying, Event::Due | Event::Start) if now >= job.at => to(Running, 0),
        (Running, Event::Step) => job,
        // Ended while a cancel was asked: one outcome, the first that came.
        (Running, Event::Done) => to(State::Done, 0),
        (Running, Event::FailAgain) => match again {
            Some(at) if !job.cancel_asked => to(Retrying, at),
            _ => to(Failed, 0),
        },
        (Running, Event::FailFinal) => to(Failed, 0),
        _ => job,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVENTS: [Event; 8] = [Event::Start, Event::Step, Event::FailAgain, Event::FailFinal, Event::Due, Event::CancelAsked, Event::Cancel, Event::Done];
    const STATES: [State; 7] = [State::Waiting, State::Scheduled, State::Running, State::Retrying, State::Done, State::Failed, State::Cancelled];

    /// JR1: every sequence of six events walked from every state, each event a second after the one before, a scheduled or
    /// retrying job's time at 3: done, failed and cancelled stay final; retrying comes only from running on a failure its kind
    /// tries again; a scheduled job runs only at or after its time; a running job asked to stop is cancelled before its next step;
    /// a cancel and an end at once give one outcome. Never a final state left, done and cancelled both, a scheduled job run early.
    #[test]
    fn the_job_walk_holds_everywhere() {
        let mut walked = 0u64;
        for start in STATES {
            for tries_again in [false, true] {
                let mut seq = [0usize; 6];
                loop {
                    let mut job = Job::new(start, 3);
                    let mut ended: Option<State> = None;
                    for (now, &e) in seq.iter().enumerate() {
                        let now = now as i64;
                        let again = tries_again.then_some(now + 10);
                        let next = walk(job, EVENTS[e], now, again);
                        if let Some(end) = ended {
                            assert_eq!(next.state, end, "a final state left: {start:?} {seq:?}");
                        }
                        if next.state == State::Retrying && job.state != State::Retrying {
                            assert!(job.state == State::Running && EVENTS[e] == Event::FailAgain && tries_again, "retrying from {:?} on {:?}", job.state, EVENTS[e]);
                        }
                        if matches!(job.state, State::Scheduled | State::Retrying) && next.state == State::Running {
                            assert!(now >= job.at, "run early: {start:?} {seq:?} at {now}");
                        }
                        if job.state == State::Running && job.cancel_asked && EVENTS[e] == Event::Step {
                            assert_eq!(next.state, State::Cancelled, "asked to stop: cancelled before its next step");
                        }
                        if next.state.is_final() && ended.is_none() {
                            ended = Some(next.state);
                        }
                        job = next;
                        walked += 1;
                    }
                    // The next sequence.
                    let mut i = 0;
                    while i < 6 {
                        seq[i] += 1;
                        if seq[i] < EVENTS.len() {
                            break;
                        }
                        seq[i] = 0;
                        i += 1;
                    }
                    if i == 6 {
                        break;
                    }
                }
            }
        }
        assert_eq!(walked, 7 * 2 * 8u64.pow(6) * 6);
        // A cancel and an end at once: whichever came first is the one outcome.
        let running = Job::new(State::Running, 0);
        let asked = walk(running, Event::CancelAsked, 0, None);
        assert_eq!(walk(asked, Event::Done, 1, None).state, State::Done);
        assert_eq!(walk(walk(asked, Event::Done, 1, None), Event::Cancel, 2, None).state, State::Done);
        assert_eq!(walk(walk(asked, Event::Cancel, 1, None), Event::Done, 2, None).state, State::Cancelled);
        assert_eq!(walk(Job::new(State::Scheduled, 3), Event::Due, 2, None).state, State::Scheduled, "not before its time");
        assert_eq!(walk(Job::new(State::Scheduled, 3), Event::Due, 3, None).state, State::Running);
        for s in STATES {
            assert_eq!(State::parse(s.as_str()), Some(s));
        }
    }
}
