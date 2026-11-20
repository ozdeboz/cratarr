//! The runner's resources (RN item 3; due diligence §4, "ours, not found elsewhere"): what used to be each part's private limit.
//!
//! - **`DECODE`**: one decode at a time, whoever asks (was `heavy_base::HEAVY`): the analysis pass, a page's visuals, a play's
//!   waveform, the look ahead, Soulseek's and the intake's checks. One decode takes about half a gigabyte for a moment; two at once
//!   went over the NAS's 768 MB (2026-10-23/24).
//! - **`COVERS`**: three cover fetches at once (was `cover_cache::FETCHING`).
//! - **Memory** (`Memory`, OQ-RN-1, both readings built, the owner's to choose): (a) each kind declares a rough peak and a job starts
//!   only while the declared peaks of those running fit a budget; (b) `memwatch::room` alone, as 0.17. Under both, `memwatch::room`
//!   is a decode's last check.
//!
//! Someone waiting goes first (`Priority::Asked`: a page's waveform, the cover a page shows) before the background's next (the
//! pass, a look's covers ahead), and a waiter past `AGE` gets the next turn whatever it is, so a stream of asks never starves the
//! pass. Whoever waits is seen: a job's row says "Waits for its turn to decode", and an asker with no job of its own has a row of its
//! own while it waits and while it decodes (`rows`).
//!
//! The services' turns (TN's `Turn`, one per outside service: MusicBrainz, AcoustID, Discogs, Bandcamp, Last.fm, GitHub) are the
//! runner's resource for each service's calls (DOMAIN-CORE C.8); they live with the services' clients, below the runner, and a job
//! on one shows its pause (`Handle::wait_for`). The file gate's lock is GF's.

use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::time::Instant;

use crate::jobs::{Handle, Row};
use crate::state::State;

/// A waiter past this gets the next turn, whatever its priority (ageing: the background is never starved).
pub const AGE: Duration = Duration::from_secs(30);

/// Who waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// Someone waits for it now (a page): it goes before the background's next.
    Asked,
    /// The background (a pass, a look ahead).
    Background,
}

struct Waiter {
    seq: u64,
    prio: Priority,
    since: Instant,
    /// What it is, where no job of its own shows it.
    what: Option<String>,
    tx: oneshot::Sender<()>,
}

struct Inner {
    held: usize,
    seq: u64,
    waiting: Vec<Waiter>,
    /// Those holding a turn that no job of their own shows: their turn's number and what they are.
    holders: Vec<(u64, String)>,
}

/// A resource with a budget of turns.
pub struct Resource {
    name: &'static str,
    /// What its waiters wait for, as Activity says it ("decode": "Waits for its turn to decode").
    what: &'static str,
    budget: usize,
    inner: Mutex<Inner>,
}

/// One decode at a time.
pub static DECODE: Resource = Resource::new("decode", "decode", 1);
/// Three covers fetched at once.
pub static COVERS: Resource = Resource::new("covers", "fetch a cover", 3);

/// A turn on a resource, given back when dropped (into blocking work and ended with it).
pub struct Permit {
    res: &'static Resource,
    seq: u64,
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.res.release(self.seq);
    }
}

/// A waiter's place, taken out of the line when its wait is dropped (a timeout, a page that went).
struct Place {
    res: &'static Resource,
    seq: u64,
    granted: bool,
}

impl Drop for Place {
    fn drop(&mut self) {
        if self.granted {
            return;
        }
        let mut g = self.res.inner();
        let before = g.waiting.len();
        g.waiting.retain(|w| w.seq != self.seq);
        if g.waiting.len() == before {
            // Its turn came as it went: given on.
            drop(g);
            self.res.release(self.seq);
        }
    }
}

impl Resource {
    pub const fn new(name: &'static str, what: &'static str, budget: usize) -> Self {
        Resource { name, what, budget, inner: Mutex::new(Inner { held: 0, seq: 0, waiting: Vec::new(), holders: Vec::new() }) }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Its turns free now.
    pub fn available(&self) -> usize {
        let g = self.inner();
        self.budget - g.held.min(self.budget)
    }

    /// How many wait.
    pub fn waiting(&self) -> usize {
        self.inner().waiting.len()
    }

    /// A turn now, where one is free and nobody waits.
    pub fn try_take(&'static self) -> Option<Permit> {
        let mut g = self.inner();
        if g.held < self.budget && g.waiting.is_empty() {
            g.held += 1;
            g.seq += 1;
            return Some(Permit { res: self, seq: g.seq });
        }
        None
    }

    /// Waits for a turn, `prio`'s place in the line, and takes it.
    pub async fn take(&'static self, prio: Priority) -> Permit {
        self.taken(prio, None, None).await
    }

    /// As `take`, `job` saying it waits for its turn while it does; or, with no job, `what` seen in Activity while it waits and
    /// while it holds its turn.
    pub async fn take_for(&'static self, prio: Priority, job: Option<&Handle>, what: Option<String>) -> Permit {
        self.taken(prio, job, what).await
    }

    /// As `take`, waiting `within` at most.
    pub async fn take_within(&'static self, prio: Priority, within: Duration, what: Option<String>) -> Option<Permit> {
        tokio::time::timeout(within, self.taken(prio, None, what)).await.ok()
    }

    async fn taken(&'static self, prio: Priority, job: Option<&Handle>, what: Option<String>) -> Permit {
        let (seq, rx) = {
            let mut g = self.inner();
            g.seq += 1;
            let seq = g.seq;
            if g.held < self.budget && g.waiting.is_empty() {
                g.held += 1;
                if let Some(w) = what {
                    g.holders.push((seq, w));
                }
                return Permit { res: self, seq };
            }
            let (tx, rx) = oneshot::channel();
            g.waiting.push(Waiter { seq, prio, since: Instant::now(), what, tx });
            (seq, rx)
        };
        if let Some(j) = job {
            j.wait_for(&crate::words::say("waits_for_turn", &[("what", self.what)]).unwrap_or_default(), None);
        }
        let mut place = Place { res: self, seq, granted: false };
        // The sender is only dropped as it is sent, or with the resource: a turn either way.
        let _ = rx.await;
        place.granted = true;
        if let Some(j) = job {
            j.go();
        }
        Permit { res: self, seq }
    }

    /// A turn given back: the next waiter's (one past `AGE` first, then one asked, then the oldest), else free.
    fn release(&self, seq: u64) {
        let mut g = self.inner();
        g.holders.retain(|(s, _)| *s != seq);
        loop {
            let now = Instant::now();
            let next = g
                .waiting
                .iter()
                .enumerate()
                .min_by_key(|(_, w)| {
                    let aged = now.duration_since(w.since) >= AGE;
                    (!aged && w.prio != Priority::Asked, w.seq)
                })
                .map(|(i, _)| i);
            let Some(i) = next else {
                g.held = g.held.saturating_sub(1);
                return;
            };
            let w = g.waiting.remove(i);
            if let Some(what) = w.what.clone() {
                g.holders.push((w.seq, what));
            }
            if w.tx.send(()).is_ok() {
                return;
            }
            g.holders.retain(|(s, _)| *s != w.seq);
        }
    }

    /// Its askers with no job of their own, as Activity shows them: those holding a turn, then those waiting for one.
    pub fn rows(&self) -> Vec<Row> {
        let g = self.inner();
        let row = |id: u64, what: &str, waits: bool| Row {
            id: format!("{}:{id}", self.name),
            kind: self.name.into(),
            state: if waits { State::Waiting } else { State::Running },
            what: what.into(),
            detail: None,
            done: None,
            of: None,
            unit: None,
            waits: waits.then(|| crate::jobs::Wait { what: crate::words::say("waits_for_turn", &[("what", self.what)]).unwrap_or_default(), until: None }),
            at: None,
            tries: None,
            whose: None,
            started: 0,
            finished: None,
        };
        let mut rows: Vec<Row> = g.holders.iter().map(|(s, w)| row(*s, w, false)).collect();
        rows.extend(g.waiting.iter().filter_map(|w| Some(row(w.seq, w.what.as_deref()?, true))));
        rows
    }
}

/// OQ-RN-1: which reading of the memory budget holds. Unset (0.17's): `memwatch::room` alone. Set
/// (`CRATARR_MEMORY_BUDGET`, in MB): the declared peaks of the jobs running fit under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// (a) Each kind's declared peak (`kinds::Kind::peak_mb`); a job starts while those running fit `budget_mb`.
    Declared { budget_mb: u64 },
    /// (b) `memwatch::room` alone, for a decode: as 0.17.
    Room,
}

/// The rule chosen: `CRATARR_MEMORY_BUDGET`'s MB for (a), else (b).
pub fn rule() -> Rule {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(r) = TEST_RULE.with(|r| r.get()) {
        return r;
    }
    match std::env::var("CRATARR_MEMORY_BUDGET").ok().and_then(|v| v.trim().parse::<u64>().ok()) {
        Some(budget_mb) => Rule::Declared { budget_mb },
        None => Rule::Room,
    }
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static TEST_RULE: std::cell::Cell<Option<Rule>> = const { std::cell::Cell::new(None) };
}

/// Tests: the memory rule on this thread.
#[cfg(any(test, feature = "test-support"))]
pub fn rule_for_tests(r: Option<Rule>) {
    TEST_RULE.with(|t| t.set(r));
}

/// How often a wait for memory looks again (memwatch's own).
const ROOM_EVERY: Duration = Duration::from_secs(5);

/// The jobs running, by their declared peaks (reading (a)).
pub struct Memory {
    running: Mutex<Vec<(u64, u64)>>,
    freed: tokio::sync::Notify,
    next: std::sync::atomic::AtomicU64,
}

/// Cratarr's memory, as the runner counts it.
pub static MEMORY: Memory = Memory::new();

/// A job's place in the memory budget, given back when dropped.
pub struct Fits {
    mem: &'static Memory,
    id: u64,
}

impl Drop for Fits {
    fn drop(&mut self) {
        self.mem.running.lock().unwrap_or_else(|e| e.into_inner()).retain(|(id, _)| *id != self.id);
        self.mem.freed.notify_waiters();
    }
}

impl Memory {
    pub const fn new() -> Self {
        Memory { running: Mutex::new(Vec::new()), freed: tokio::sync::Notify::const_new(), next: std::sync::atomic::AtomicU64::new(0) }
    }

    /// Whether `kind` may start now, by the rule.
    fn fit(&self, kind: &crate::kinds::Kind) -> bool {
        let room = !kind.decodes || cratarr_store::memwatch::room();
        match rule() {
            Rule::Room => room,
            Rule::Declared { budget_mb } => {
                let held: u64 = self.running.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(_, p)| p).sum();
                room && held + kind.peak_mb <= budget_mb
            }
        }
    }

    /// Waits until `kind` fits (by the rule), `job` saying "Waits for memory" meanwhile, and takes its place.
    pub async fn fits(&'static self, kind: &str, job: Option<&Handle>) -> Fits {
        let k = crate::kinds::of(kind);
        let mut said = false;
        loop {
            let freed = self.freed.notified();
            if self.fit(&k) {
                let id = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                self.running.lock().unwrap_or_else(|e| e.into_inner()).push((id, k.peak_mb));
                if said && let Some(j) = job {
                    j.go();
                }
                return Fits { mem: self, id };
            }
            if !said {
                if let Some(j) = job {
                    j.wait_for(crate::words::word("waits_for_memory").unwrap_or_default(), None);
                }
                said = true;
            }
            let _ = tokio::time::timeout(ROOM_EVERY, freed).await;
        }
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::jobs::Runner;

    fn fresh() -> &'static Runner {
        Box::leak(Box::new(Runner::new()))
    }

    /// RL1: four decoders ask the one decode at once: the pass (catching up, a job), a page's waveform (asked for, no job of its
    /// own), a Soulseek check (a job) and the intake card's hearing (asked for, a job). Exactly one decodes at any moment; the
    /// page's ask goes before the pass's next file and the pass goes on after; Activity shows the one decoding and the others
    /// "Waits for its turn to decode". A background waiter past `AGE` gets the next turn, so a stream of asks never starves it.
    #[tokio::test(start_paused = true)]
    async fn one_decode_at_a_time_and_the_waiting_are_seen() {
        static ONE: Resource = Resource::new("decode", "decode", 1);
        let r = fresh();
        let pass = Arc::new(r.begin_keyed("analysis", "analysis", None).unwrap());
        let check = Arc::new(r.begin_keyed("soulseek:check", "ss-11", None).unwrap());
        let hearing = Arc::new(r.begin("intake", "root", false).unwrap());
        let (at_once, most, order) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)), Arc::new(Mutex::new(Vec::new())));
        let counts = (at_once.clone(), most.clone(), order.clone());
        let decode = move |who: &'static str, held: Permit| {
            let (at_once, most, order) = (counts.0.clone(), counts.1.clone(), counts.2.clone());
            async move {
                let n = at_once.fetch_add(1, Ordering::SeqCst) + 1;
                most.fetch_max(n, Ordering::SeqCst);
                order.lock().unwrap().push(who);
                tokio::time::sleep(Duration::from_secs(2)).await;
                at_once.fetch_sub(1, Ordering::SeqCst);
                drop(held);
            }
        };
        // The pass decodes file A; the others ask meanwhile: the pass's next and the check (the background), the page and the hearing.
        let first = ONE.take_for(Priority::Background, Some(&pass), None).await;
        let a = tokio::spawn(decode("pass: A", first));
        let (p2, c2, h2) = (pass.clone(), check.clone(), hearing.clone());
        let (d1, d2, d3, d4) = (decode.clone(), decode.clone(), decode.clone(), decode.clone());
        let next_pass = tokio::spawn(async move {
            let t = ONE.take_for(Priority::Background, Some(&p2), None).await;
            d1("pass: E", t).await;
        });
        let check_c = tokio::spawn(async move {
            let t = ONE.take_for(Priority::Background, Some(&c2), None).await;
            d2("check: C", t).await;
        });
        tokio::time::sleep(Duration::from_millis(1)).await;
        let page = tokio::spawn(async move {
            let t = ONE.take_for(Priority::Asked, None, Some("Waveform · Glass Harbour/Night Ferry/03 Neap.flac · asked for".into())).await;
            d3("page: B", t).await;
        });
        let hear = tokio::spawn(async move {
            let t = ONE.take_for(Priority::Asked, Some(&h2), None).await;
            d4("hearing: D", t).await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        // Seen while they wait: the pass decoding, the others waiting for their turn.
        let mut rows = r.rows();
        rows.extend(ONE.rows());
        let turn = "Waits for its turn to decode";
        let waiting = |what: &str| rows.iter().filter(|x| x.waits.as_ref().is_some_and(|w| w.what == turn) && (x.kind == what || x.what.starts_with(what))).count();
        assert_eq!((waiting("soulseek:check"), waiting("intake"), waiting("Waveform")), (1, 1, 1), "{rows:#?}");
        assert_eq!(rows.iter().filter(|x| x.kind == "analysis").count(), 1, "the pass once, though it waits again for its next");
        for t in [a, next_pass, check_c, page, hear] {
            t.await.unwrap();
        }
        assert_eq!(most.load(Ordering::SeqCst), 1, "never two decodes at once");
        let order = order.lock().unwrap().clone();
        assert_eq!(order[0], "pass: A");
        assert_eq!(&order[1..3], ["page: B", "hearing: D"], "those asked for go before the background's next: {order:?}");
        assert!(order.contains(&"pass: E") && order.contains(&"check: C"), "the pass goes on after");
        assert!(ONE.rows().is_empty() && r.rows().iter().all(|x| x.waits.is_none()), "nothing left waiting");

        // Ageing: the background waiting past AGE goes before an ask that came after it.
        let held = ONE.take(Priority::Background).await;
        let order = Arc::new(Mutex::new(Vec::new()));
        let o = order.clone();
        let background = tokio::spawn(async move {
            let _t = ONE.take(Priority::Background).await;
            o.lock().unwrap().push("background");
        });
        tokio::time::sleep(AGE + Duration::from_secs(1)).await;
        let o = order.clone();
        let asked = tokio::spawn(async move {
            let _t = ONE.take(Priority::Asked).await;
            o.lock().unwrap().push("asked");
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        drop(held);
        background.await.unwrap();
        asked.await.unwrap();
        assert_eq!(*order.lock().unwrap(), ["background", "asked"], "a waiter past its bound gets the next turn");
        drop((pass, check, hearing));
    }

    /// RL3's turn: three at a time, the one asked for first. Twelve of a look's covers fetched ahead (the background) and a page's
    /// cover asked meanwhile, each fetch two seconds: never four at once, and the page's is the next to start.
    #[tokio::test(start_paused = true)]
    async fn covers_go_three_at_a_time_the_asked_first() {
        static THREE: Resource = Resource::new("covers", "fetch a cover", 3);
        let (at_once, most, started) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)), Arc::new(Mutex::new(Vec::<String>::new())));
        let fetch = |name: String, prio: Priority| {
            let (at_once, most, started) = (at_once.clone(), most.clone(), started.clone());
            tokio::spawn(async move {
                let _t = THREE.take(prio).await;
                let n = at_once.fetch_add(1, Ordering::SeqCst) + 1;
                most.fetch_max(n, Ordering::SeqCst);
                started.lock().unwrap().push(name);
                tokio::time::sleep(Duration::from_secs(2)).await;
                at_once.fetch_sub(1, Ordering::SeqCst);
            })
        };
        let look: Vec<_> = (1..=12).map(|n| fetch(format!("look {n}"), Priority::Background)).collect();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let page = fetch("al-42".into(), Priority::Asked);
        for t in look {
            t.await.unwrap();
        }
        page.await.unwrap();
        assert_eq!(most.load(Ordering::SeqCst), 3, "three at a time, never four");
        let started = started.lock().unwrap().clone();
        assert_eq!(started.iter().position(|s| s == "al-42"), Some(3), "the page's cover the next to start: {started:?}");
    }

    /// RL5 (OQ-RN-1, both readings): an import (declared about 120 MB), a decode (about 500 MB) and a covers look are due together.
    /// (a) With declared peaks under a budget of 600 MB, the decode and the import do not start together: the one waiting says
    /// "Waits for memory", and starts as the other ends. (b) With `memwatch::room` alone (no room: its stand-in), the decode waits
    /// for room and says so, and the import starts. Never a decode started while `room` says no (under (a) too), nor a wait for
    /// memory Activity doesn't show.
    #[tokio::test(start_paused = true)]
    async fn a_job_starts_only_while_it_fits() {
        let r = fresh();
        let mem: &'static Memory = Box::leak(Box::new(Memory::new()));
        let shown = |r: &Runner, kind: &str| r.rows().iter().find(|x| x.kind == kind).and_then(|x| x.waits.clone()).map(|w| w.what);

        // (a) Declared peaks under 600 MB: the decode in first, the import waits for memory until it ends.
        rule_for_tests(Some(Rule::Declared { budget_mb: 600 }));
        let decode = r.begin_keyed("analysis", "analysis", None).unwrap();
        let import = r.begin("import:lastfm", "alice", false).unwrap();
        let looks = r.begin_keyed("covers", "al-42", Some("root")).unwrap();
        let d = mem.fits("analysis", Some(&decode)).await;
        let c = mem.fits("covers", Some(&looks)).await;
        {
            let waiting = mem.fits("import:lastfm", Some(&import));
            tokio::pin!(waiting);
            assert!(tokio::time::timeout(Duration::from_secs(1), &mut waiting).await.is_err(), "the import does not start beside the decode");
            assert_eq!(shown(r, "import:lastfm").as_deref(), Some("Waits for memory"));
            drop(d);
            let i = tokio::time::timeout(Duration::from_secs(1), &mut waiting).await.expect("it starts as the decode ends");
            assert_eq!(shown(r, "import:lastfm"), None, "it runs");
            drop((i, c));
        }

        // (b) `memwatch::room` alone: no room for a decode; the decode waits for it and says so, and the import starts.
        rule_for_tests(Some(Rule::Room));
        cratarr_store::memwatch::FULL.set(true);
        let i = tokio::time::timeout(Duration::from_secs(1), mem.fits("import:lastfm", Some(&import))).await.expect("the import starts");
        {
            let waiting = mem.fits("analysis", Some(&decode));
            tokio::pin!(waiting);
            assert!(tokio::time::timeout(Duration::from_secs(12), &mut waiting).await.is_err(), "no decode while room says no");
            assert_eq!(shown(r, "analysis").as_deref(), Some("Waits for memory"));
            cratarr_store::memwatch::FULL.set(false);
            let d = tokio::time::timeout(Duration::from_secs(10), &mut waiting).await.expect("it starts once there is room");
            assert_eq!(shown(r, "analysis"), None);
            drop(d);
        }
        // Under (a) too, room is a decode's last check.
        rule_for_tests(Some(Rule::Declared { budget_mb: 10_000 }));
        cratarr_store::memwatch::FULL.set(true);
        assert!(tokio::time::timeout(Duration::from_secs(6), mem.fits("decode", None)).await.is_err(), "(a): room still refuses a decode");
        cratarr_store::memwatch::FULL.set(false);
        rule_for_tests(None);
        drop((i, decode, import, looks));
    }
}
