//! The one command path (SCENARIOS-GF §0; DOMAIN-CORE §11 and D.2): every Change goes through [`Commands::run`] (`decide()`,
//! intend, apply, applied: the answer `{ result, undo?, rev }`, its token at once) and every Undo through [`Commands::undo`], the
//! one route's (`POST /api/undo`). One per app, its kinds registered at the start. A kind joins by its [`Undoable`] alone (AP1):
//! no route, token, table, folder or rights rule of its own.
//!
//! - **The rev at once** (CP9): a command writes its own change-log rows in its apply's transaction and answers their rev; the
//!   catalogue's records it changed are logged here before the answer (two files: committed before the answer, not one
//!   transaction), so the served library's refresh never logs them twice.
//! - **Who may undo** is `change::may_undo`, judged at the Undo with the kind's right held now; every refusal is one of §0's four,
//!   answered before anything is changed.
//! - **An Undo before its Change applied** (CP8): queued and not started, it cancels the job; running, a 409.
//! - **A stop between the steps** settles at the next start by what the kind's store shows ([`Commands::settle`]).

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use cratarr_domain::can::{Decision, Viewer, Why};
use cratarr_domain::change::{Change, ChangeId, ChangeKind, Facts, Go, Refusal, UndoState, may_undo};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::ToggleError;
use crate::journal::{Journal, Kept};
use crate::serve::Serve;

/// A future the path awaits (inside the HTTP handlers: `Send`).
pub type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What a command answers (§0; D.2): its result, the token of its Undo at once, and the rev its own change-log rows took (CP9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answer<R> {
    pub result: R,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo: Option<String>,
    pub rev: i64,
}

/// What a command means to change, read before it does (write-ahead): what it is about, its name in the server's words, and
/// what undoing it needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    pub object: String,
    pub name: String,
    pub data: serde_json::Value,
}

/// What a command will do.
pub enum Plan<R> {
    /// A Change, kept before it applies.
    Change(Planned),
    /// Nothing would change: this answer, with no Change and no token.
    Nothing(R),
}

/// What it did: its result; the rev its own change-log rows took in its write's transaction (the user data's); and the
/// catalogue's records it changed, logged by the path before the answer.
pub struct Applied<R> {
    pub result: R,
    pub rev: Option<i64>,
    pub records: Vec<i64>,
}

/// A command on the path: a kind, `decide()` first, a plan read before it writes, and its write.
pub trait Command: Send + Sync {
    type Result: Serialize + DeserializeOwned + Send;

    fn kind(&self) -> ChangeKind;

    /// `decide()`'s answer for it, before anything is read or written: a right a 403, a state a 409 (`Why::status`).
    fn decide(&self, v: &Viewer) -> Decision;

    /// What it would change, read now.
    fn plan<'a>(&'a self, v: &'a Viewer) -> Boxed<'a, Result<Plan<Self::Result>, ToggleError>>;

    /// Its write as planned: its store's rows and its own change-log rows in one transaction, the Change's id kept with them
    /// where its kind settles by it.
    fn apply<'a>(&'a self, v: &'a Viewer, id: ChangeId, planned: &'a Planned) -> Boxed<'a, Result<Applied<Self::Result>, ToggleError>>;
}

/// What an Undo did: the rev its own change-log rows took, a Redo where its kind has one (a new Change on the path), and the
/// catalogue's records it changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Undone {
    pub rev: Option<i64>,
    pub redo: Option<ChangeId>,
    pub records: Vec<i64>,
}

/// An Undo that didn't happen: refused (§0's four), or failed on the way, its words said and the Change left not undone, to
/// be tried again (CP3 (e)).
#[derive(Debug)]
pub enum Failed {
    Refused(Refusal),
    Error(ToggleError),
}

impl From<Refusal> for Failed {
    fn from(r: Refusal) -> Self {
        Failed::Refused(r)
    }
}

/// What a kind's store shows of a Change, at a start after a stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shows {
    /// Its write is there (and no Undo of it).
    Applied,
    /// Its write isn't there: never made, or undone.
    NotApplied,
}

/// A kind's Undo (§0's `Undoable`): registered once, found by its kind for each of its Changes.
pub trait Undoable: Send + Sync {
    fn kind(&self) -> ChangeKind;

    /// The right to make this Change, judged now by its command's `decide()`: refused, nobody's Undo of it goes on (21:37).
    fn may(&self, v: &Viewer, c: &Change) -> Decision;

    /// A player's Change: whether `v` controls its player now.
    fn controls(&self, _v: &Viewer, _c: &Change) -> bool {
        false
    }

    /// What its store shows of `c` now (a stop between its intent and its end, or in its Undo).
    fn shows<'a>(&'a self, c: &'a Change) -> Boxed<'a, Result<Shows, ToggleError>>;

    /// Undo `c`, putting back what it changed, or refuse with why: a change on top changes nothing (CP5). `parts`: a run's or
    /// an album action's albums, a batch's items.
    fn undo<'a>(&'a self, path: &'a Commands, c: &'a Change, by: &'a Viewer, parts: Option<&'a [String]>) -> Boxed<'a, Result<Undone, Failed>>;
}

/// One Change undone, as the answer names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UndoneOne {
    pub token: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redo: Option<String>,
}

/// One token (or one part of it) refused, with its status and words (§0's shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RefusedOne {
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    pub status: u16,
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// What an Undo of one or several answers: each undone, each refused, and the rev it made.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UndoAnswer {
    pub undone: Vec<UndoneOne>,
    pub refused: Vec<RefusedOne>,
    pub rev: i64,
}

/// A Change told as it applies or is undone (CP7, CP8: the page offers Undo again when a Change it was refused for applies).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Told {
    pub token: String,
    pub kind: ChangeKind,
    pub who: Option<String>,
    pub undone: bool,
    pub rev: i64,
}

/// A queued Change (CP8): its answer, given at once with its token, and what its job applies when it starts.
pub struct Queued {
    pub answer: Answer<()>,
    pub id: ChangeId,
    planned: Planned,
}

/// Where a test stops a command, as a process stopping there would (AP1).
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// After the intent, before the store's write.
    Intended,
    /// After the store's write, before `applied`.
    Written,
}

/// The clock the path reads windows and times by (a test moves it).
type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The one path: the journal's table of Changes, the kinds' Undos, and where the change log stands.
pub struct Commands {
    journal: Arc<Journal>,
    served: Option<Arc<Serve>>,
    kinds: RwLock<HashMap<&'static str, Arc<dyn Undoable>>>,
    trash_days: Arc<dyn Fn() -> i64 + Send + Sync>,
    clock: RwLock<Clock>,
    told: tokio::sync::broadcast::Sender<Told>,
    #[cfg(any(test, feature = "test-support"))]
    stop: std::sync::Mutex<Option<Stop>>,
}

/// A refusal of `decide()`, as the routes answer it: a right a 403, a state a 409, a hidden upload as if not there.
fn denied(why: Why) -> ToggleError {
    match why.status() {
        403 => ToggleError::Forbidden(why.words().into()),
        404 => ToggleError::Gone(why.words().into()),
        _ => ToggleError::Refused(why.words().into()),
    }
}

/// Whether a token is the one path's (`j<n>`): the route hands it to [`Commands::undo`]. Nothing reads it but the path.
pub fn on_the_path(token: &str) -> bool {
    ChangeId::of(token).is_some()
}

/// An Undo's refusal, where it ends a command (a Redo's): its words.
pub fn refused(r: &Refusal) -> ToggleError {
    match r {
        Refusal::NotYours => ToggleError::Forbidden(r.words().into()),
        _ => ToggleError::Refused(r.words().into()),
    }
}

impl Commands {
    /// The path over `journal`, its revs from `served`'s change log (none: 0), the file kinds' window `trash_days` long.
    pub fn new(journal: Arc<Journal>, served: Option<Arc<Serve>>, trash_days: Arc<dyn Fn() -> i64 + Send + Sync>) -> Self {
        Self {
            journal,
            served,
            kinds: RwLock::default(),
            trash_days,
            clock: RwLock::new(Arc::new(super::library::epoch_now) as Clock),
            told: tokio::sync::broadcast::channel(64).0,
            #[cfg(any(test, feature = "test-support"))]
            stop: std::sync::Mutex::default(),
        }
    }

    /// A kind's Undo, registered at the start by its domain crate.
    pub fn register(&self, kind: Arc<dyn Undoable>) {
        self.kinds.write().unwrap_or_else(|p| p.into_inner()).insert(kind.kind().name(), kind);
    }

    fn kind(&self, kind: ChangeKind) -> Option<Arc<dyn Undoable>> {
        self.kinds.read().unwrap_or_else(|p| p.into_inner()).get(kind.name()).cloned()
    }

    /// Changes as they apply and are undone.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<Told> {
        self.told.subscribe()
    }

    /// The journal the path keeps its Changes in.
    pub fn journal(&self) -> &Arc<Journal> {
        &self.journal
    }

    fn now(&self) -> i64 {
        (self.clock.read().unwrap_or_else(|p| p.into_inner()))()
    }

    /// The clock moved (tests: a window passing).
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_clock(&self, clock: impl Fn() -> i64 + Send + Sync + 'static) {
        *self.clock.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(clock);
    }

    /// The next command stops there, as a process stopping would (tests).
    #[cfg(any(test, feature = "test-support"))]
    pub fn stop_at(&self, at: Option<Stop>) {
        *self.stop.lock().unwrap_or_else(|p| p.into_inner()) = at;
    }

    #[cfg(any(test, feature = "test-support"))]
    fn stops(&self, here: Stop) -> bool {
        let mut stop = self.stop.lock().unwrap_or_else(|p| p.into_inner());
        if *stop == Some(here) {
            *stop = None;
            return true;
        }
        false
    }

    /// Where the change log stands now.
    pub fn rev(&self) -> i64 {
        self.served.as_ref().and_then(|s| s.userdata.change_rev().ok()).unwrap_or(0)
    }

    /// The records a Change wrote, logged now at one rev, before its answer (CP9); none: where the change log stands.
    fn logged(&self, own: Option<i64>, records: &[i64]) -> Result<i64, ToggleError> {
        let records = match (&self.served, records.is_empty()) {
            (Some(s), false) => s.log_records_now(records)?,
            _ => None,
        };
        Ok(own.into_iter().chain(records).max().unwrap_or_else(|| self.rev()))
    }

    fn tell(&self, c: &Change, undone: bool, rev: i64) {
        let _ = self.told.send(Told { token: c.id.token(), kind: c.kind, who: c.who.clone(), undone, rev });
    }

    /// Whose a Change is: the viewer's account (none: Cratarr's own).
    fn whose(v: &Viewer) -> Option<String> {
        Some(v.account.clone()).filter(|a| !a.is_empty())
    }

    /// A resend of `key`: its Change's first answer (ID2), or none when the key brought no Change yet.
    fn resent<R: DeserializeOwned>(&self, v: &Viewer, key: Option<&str>) -> Result<Option<Answer<R>>, ToggleError> {
        let Some(key) = key else { return Ok(None) };
        let Some(id) = self.journal.change_by_key(Self::whose(v).as_deref(), key)? else { return Ok(None) };
        self.again(id).map(Some)
    }

    /// The answer a kept Change gave.
    fn again<R: DeserializeOwned>(&self, id: ChangeId) -> Result<Answer<R>, ToggleError> {
        let kept = self.journal.change_entry(id)?.ok_or_else(|| ToggleError::Refused(cratarr_domain::change::said::NO_LONGER.into()))?;
        match (&kept.change.state, kept.answer) {
            (UndoState::Waiting | UndoState::Running, _) => Err(refused(&Refusal::StillWorking)),
            (_, Some(answer)) => {
                let result = serde_json::from_str(&answer).map_err(|e| anyhow::anyhow!("its answer: {e}"))?;
                Ok(Answer { result, undo: Some(id.token()), rev: kept.change.rev.unwrap_or_default() })
            }
            (_, None) => Err(ToggleError::Refused(cratarr_domain::change::said::NO_LONGER.into())),
        }
    }

    /// A command, on the path: `decide()`, its key (a resend gets its first answer), its plan, the intent kept, its write, its
    /// records logged, `applied`; then its answer with its token and rev.
    pub async fn run<C: Command>(&self, v: &Viewer, key: Option<&str>, c: &C) -> Result<Answer<C::Result>, ToggleError> {
        if let Decision::Deny(why) = c.decide(v) {
            return Err(denied(why));
        }
        if let Some(again) = self.resent(v, key)? {
            return Ok(again);
        }
        let planned = match c.plan(v).await? {
            Plan::Nothing(result) => return Ok(Answer { result, undo: None, rev: self.rev() }),
            Plan::Change(planned) => planned,
        };
        let (id, new) = self.journal.intend_change(&intent(c.kind(), v, &planned, key, UndoState::Running, self.now()))?;
        if !new {
            return self.again(id);
        }
        self.apply(v, id, c, &planned).await
    }

    /// The intended Change applied: its write, its records logged, `applied`, told.
    async fn apply<C: Command>(&self, v: &Viewer, id: ChangeId, c: &C, planned: &Planned) -> Result<Answer<C::Result>, ToggleError> {
        #[cfg(any(test, feature = "test-support"))]
        if self.stops(Stop::Intended) {
            return Err(ToggleError::Refused("stopped".into()));
        }
        let applied = match c.apply(v, id, planned).await {
            Ok(a) => a,
            Err(e) => {
                self.journal.forget_change(id)?;
                return Err(e);
            }
        };
        #[cfg(any(test, feature = "test-support"))]
        if self.stops(Stop::Written) {
            return Err(ToggleError::Refused("stopped".into()));
        }
        let rev = self.logged(applied.rev, &applied.records)?;
        let answer = serde_json::to_string(&applied.result).map_err(|e| anyhow::anyhow!("its answer: {e}"))?;
        self.journal.change_applied(id, rev, &answer)?;
        if let Some(kept) = self.journal.change_entry(id)? {
            self.tell(&kept.change, false, rev);
        }
        Ok(Answer { result: applied.result, undo: Some(id.token()), rev })
    }

    /// A command its job applies later (CP8): `decide()` and its plan now, the intent kept `waiting`, its answer at once with its
    /// token. [`Commands::start`] applies it when its turn comes, unless an Undo cancelled it first.
    pub async fn queue<C: Command>(&self, v: &Viewer, key: Option<&str>, c: &C) -> Result<Option<Queued>, ToggleError> {
        if let Decision::Deny(why) = c.decide(v) {
            return Err(denied(why));
        }
        let Plan::Change(planned) = c.plan(v).await? else { return Ok(None) };
        let (id, _) = self.journal.intend_change(&intent(c.kind(), v, &planned, key, UndoState::Waiting, self.now()))?;
        Ok(Some(Queued { answer: Answer { result: (), undo: Some(id.token()), rev: self.rev() }, id, planned }))
    }

    /// A queued command's job starts: applied, as `run` applies; none when an Undo cancelled it before it started (it never runs).
    pub async fn start<C: Command>(&self, v: &Viewer, q: Queued, c: &C) -> Result<Option<Answer<C::Result>>, ToggleError> {
        if !self.journal.start_change(q.id)? {
            return Ok(None);
        }
        self.apply(v, q.id, c, &q.planned).await.map(Some)
    }

    /// The one Undo (`POST /api/undo`): each token newest first, each judged by `may_undo` with its kind's right held now and
    /// answered undone or refused in §0's shape; nothing refused changes anything. `parts`: by token, a run's albums or a batch's
    /// items.
    pub async fn undo(&self, v: &Viewer, tokens: &[String], parts: &HashMap<String, Vec<String>>) -> UndoAnswer {
        let mut answer = UndoAnswer::default();
        let mut revs = Vec::new();
        for token in tokens.iter().rev() {
            let refuse = |r: Refusal| RefusedOne { token: token.clone(), part: None, status: r.status(), error: r.words().into(), why: r.why().map(str::to_string) };
            match self.undo_one(v, token, parts.get(token).map(Vec::as_slice)).await {
                Ok((one, rev)) => {
                    answer.undone.push(one);
                    revs.push(rev);
                }
                Err(Failed::Refused(r)) => answer.refused.push(refuse(r)),
                Err(Failed::Error(e)) => answer.refused.push(RefusedOne {
                    token: token.clone(),
                    part: None,
                    status: 409,
                    error: super::plain(e.to_string()),
                    why: None,
                }),
            }
        }
        answer.rev = revs.into_iter().max().unwrap_or_else(|| self.rev());
        answer
    }

    async fn undo_one(&self, v: &Viewer, token: &str, parts: Option<&[String]>) -> Result<(UndoneOne, i64), Failed> {
        let no_longer = || Failed::Refused(Refusal::NoLonger);
        let id = ChangeId::of(token).ok_or_else(no_longer)?;
        let Some(Kept { change: c, .. }) = self.journal.change_entry(id).map_err(|e| Failed::Error(e.into()))? else { return Err(no_longer()) };
        let kind = self.kind(c.kind).ok_or_else(no_longer)?;
        let facts = Facts { now: self.now(), trash_days: (self.trash_days)(), right: kind.may(v, &c), controls: kind.controls(v, &c) };
        let by = Self::whose(v);
        match may_undo(v, &c, &facts)? {
            Go::Cancel => {
                if !self.journal.cancel_change(id, facts.now, by.as_deref()).map_err(|e| Failed::Error(e.into()))? {
                    // Its job started first: one outcome, never both (CP8).
                    return Err(Failed::Refused(Refusal::StillWorking));
                }
                let rev = self.rev();
                self.tell(&c, true, rev);
                Ok((UndoneOne { token: token.into(), name: c.name.clone(), redo: None }, rev))
            }
            Go::Undo => {
                if !self.journal.claim_undo(id, by.as_deref()).map_err(|e| Failed::Error(e.into()))? {
                    return Err(no_longer());
                }
                let done = match kind.undo(self, &c, v, parts).await {
                    Ok(done) => done,
                    Err(e) => {
                        self.journal.release_undo(id).map_err(|e| Failed::Error(e.into()))?;
                        return Err(e);
                    }
                };
                let rev = self.logged(done.rev, &done.records).map_err(Failed::Error)?;
                self.journal.change_undone(id, facts.now, rev).map_err(|e| Failed::Error(e.into()))?;
                self.tell(&c, true, rev);
                let redo = done.redo.filter(|_| c.kind.redo()).map(ChangeId::token);
                Ok((UndoneOne { token: token.into(), name: c.name.clone(), redo }, rev))
            }
        }
    }

    /// At a start: each Change a stop left between its intent and its end, or in its Undo, settled by what its kind's store
    /// shows (AP1, GJ2's rule): written, it applied (open for its Undo); not, its intent is forgotten; an Undo whose write is
    /// there is done, one whose isn't is handed back. A kind not registered is left as it is. How many were settled.
    pub async fn settle(&self) -> Result<usize, ToggleError> {
        let mut n = 0;
        for kept in self.journal.unsettled()? {
            let c = kept.change;
            let Some(kind) = self.kind(c.kind) else { continue };
            let shows = kind.shows(&c).await?;
            match (&c.state, shows) {
                (UndoState::Running, Shows::Applied) => {
                    let rev = self.rev();
                    self.journal.change_applied(c.id, rev, "null")?;
                }
                (UndoState::Running, Shows::NotApplied) => self.journal.forget_change(c.id)?,
                (UndoState::Undoing { .. }, Shows::Applied) => self.journal.release_undo(c.id)?,
                (UndoState::Undoing { .. }, Shows::NotApplied) => {
                    self.journal.change_undone(c.id, self.now(), self.rev())?;
                }
                _ => continue,
            }
            n += 1;
        }
        Ok(n)
    }
}

/// The Change a command intends, `state` waiting or running.
fn intent(kind: ChangeKind, v: &Viewer, planned: &Planned, key: Option<&str>, state: UndoState, at: i64) -> Change {
    Change {
        id: ChangeId(0),
        kind,
        who: Commands::whose(v),
        at,
        object: planned.object.clone(),
        name: planned.name.clone(),
        data: planned.data.to_string(),
        steps: Vec::new(),
        state,
        key: key.map(str::to_string),
        rev: None,
    }
}

/// The path's own test kind (AP1): one record's picked picture, kept as a fact of the record's in the catalogue with the
/// Change that picked it, an administrators' command behind `decide()`, shared to undo. It needs only its kind, its command and
/// its `Undoable`, as AP01's pick will.
#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use cratarr_domain::can::{Decision, Scope, Viewer, Why, said};
    use cratarr_domain::change::{Change, ChangeId, ChangeKind, Refusal};
    use serde::{Deserialize, Serialize};

    use super::{Applied, Boxed, Commands, Failed, Plan, Planned, Shows, Undoable, Undone};
    use crate::app::ToggleError;
    use crate::serve::Serve;

    /// Its fact's name in the catalogue: one row a record, its value the picture, its time the Change that picked it.
    const FACT: &str = "test-picture";
    const HAND: &str = "hand";

    /// An administrators' command, as AP01's pick is (`Action::ChangeCover`'s right).
    fn administers(v: &Viewer) -> Decision {
        if v.scope != Scope::Full {
            Decision::Deny(Why::Right(said::KEY))
        } else if v.role.administers() {
            Decision::Allow
        } else {
            Decision::Deny(Why::Right(said::ADMINISTRATORS))
        }
    }

    /// What a record's picture is now, and the Change that picked it.
    fn picked(serve: &Serve, record: i64) -> Result<Option<(String, i64)>, ToggleError> {
        let facts = serve.catalogue().facts_said(record)?;
        Ok(facts.into_iter().find(|f| f.fact == FACT && f.source == HAND).and_then(|f| Some((f.value?, f.at))))
    }

    /// Its picture set to `to` by Change `by`, or none.
    fn set(serve: &Serve, record: i64, to: Option<&(String, i64)>) -> Result<(), ToggleError> {
        let cat = serve.catalogue();
        match to {
            Some((picture, by)) => cat.said(record, FACT, HAND, "", Some(picture), *by)?,
            None => cat.execute_for_tests(&format!("DELETE FROM facts WHERE record = {record} AND fact = '{FACT}' AND source = '{HAND}'")),
        }
        Ok(())
    }

    /// What the Change keeps: the record, its picture before (and the Change that picked it), and the one picked.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Data {
        record: i64,
        before: Option<(String, i64)>,
        after: String,
    }

    /// Pick `picture` for `record`.
    pub struct Pick {
        pub serve: Arc<Serve>,
        pub record: i64,
        pub picture: String,
    }

    impl super::Command for Pick {
        type Result = String;

        fn kind(&self) -> ChangeKind {
            ChangeKind::TestPicture
        }

        fn decide(&self, v: &Viewer) -> Decision {
            administers(v)
        }

        fn plan<'a>(&'a self, _v: &'a Viewer) -> Boxed<'a, Result<Plan<String>, ToggleError>> {
            Box::pin(async move {
                let before = picked(&self.serve, self.record)?;
                if before.as_ref().is_some_and(|(p, _)| *p == self.picture) {
                    return Ok(Plan::Nothing(self.picture.clone()));
                }
                let data = Data { record: self.record, before, after: self.picture.clone() };
                Ok(Plan::Change(Planned {
                    object: crate::serve::ids::record(self.record),
                    name: "Picture picked".into(),
                    data: serde_json::to_value(data).map_err(|e| anyhow::anyhow!("{e}"))?,
                }))
            })
        }

        fn apply<'a>(&'a self, _v: &'a Viewer, id: ChangeId, _planned: &'a Planned) -> Boxed<'a, Result<Applied<String>, ToggleError>> {
            Box::pin(async move {
                set(&self.serve, self.record, Some(&(self.picture.clone(), id.0)))?;
                Ok(Applied { result: self.picture.clone(), rev: None, records: vec![self.record] })
            })
        }
    }

    /// Its Undo: the picture before back, where the one it picked is still there.
    pub struct Picture {
        pub serve: Arc<Serve>,
        /// The next Undo fails on the way (a file held open), as CP3 (e)'s.
        pub fail_next: AtomicBool,
    }

    impl Picture {
        pub fn new(serve: Arc<Serve>) -> Arc<Self> {
            Arc::new(Self { serve, fail_next: AtomicBool::new(false) })
        }

        pub fn fail_next(&self) {
            self.fail_next.store(true, Ordering::SeqCst);
        }
    }

    fn data(c: &Change) -> Result<Data, ToggleError> {
        Ok(serde_json::from_str(&c.data).map_err(|e| anyhow::anyhow!("{e}"))?)
    }

    impl Undoable for Picture {
        fn kind(&self) -> ChangeKind {
            ChangeKind::TestPicture
        }

        fn may(&self, v: &Viewer, _c: &Change) -> Decision {
            administers(v)
        }

        fn shows<'a>(&'a self, c: &'a Change) -> Boxed<'a, Result<Shows, ToggleError>> {
            Box::pin(async move {
                let d = data(c)?;
                let now = picked(&self.serve, d.record)?;
                Ok(if now.is_some_and(|(_, by)| by == c.id.0) { Shows::Applied } else { Shows::NotApplied })
            })
        }

        fn undo<'a>(&'a self, _path: &'a Commands, c: &'a Change, _by: &'a Viewer, _parts: Option<&'a [String]>) -> Boxed<'a, Result<Undone, Failed>> {
            Box::pin(async move {
                let d = data(c).map_err(Failed::Error)?;
                let now = picked(&self.serve, d.record).map_err(Failed::Error)?;
                if !now.is_some_and(|(_, by)| by == c.id.0) {
                    return Err(Failed::Refused(Refusal::ChangedSince(None)));
                }
                // Another task may come in here (two Undos at once, CP3).
                tokio::task::yield_now().await;
                if self.fail_next.swap(false, Ordering::SeqCst) {
                    return Err(Failed::Error(ToggleError::Refused("The picture is held open.".into())));
                }
                set(&self.serve, d.record, d.before.as_ref()).map_err(Failed::Error)?;
                Ok(Undone { rev: None, redo: None, records: vec![d.record] })
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use cratarr_domain::can::{May, Viewer};
    use cratarr_domain::change::{ChangeId, ChangeKind, UndoState};

    use super::testing::{Pick, Picture};
    use super::*;

    const T0: i64 = 1_800_000_000;

    fn root() -> Viewer {
        Viewer::new("root", true, None, May::default(), None)
    }

    fn dave() -> Viewer {
        Viewer::new("dave", true, None, May::default(), None)
    }

    fn alice() -> Viewer {
        Viewer::new("alice", false, None, May { request: true, offline: true, playlists: true }, None)
    }

    struct World {
        serve: Arc<Serve>,
        journal: Arc<Journal>,
        path: Commands,
        kind: Arc<Picture>,
        record: i64,
    }

    /// The path with the test kind registered, over a served library with one record (Night Ferry), its clock at t0.
    fn world() -> World {
        let serve = Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap()));
        let record = serve.catalogue().record_for_tests("Night Ferry", 2);
        // What making it owed is in the change log already: the tests' rows are their own.
        let owed = serve.catalogue().owed_records(100).unwrap();
        serve.catalogue().owed_moved(&owed).unwrap();
        let journal = Arc::new(Journal::open(None).unwrap());
        let (path, kind) = restart(&serve, &journal);
        World { serve, journal, path, kind, record }
    }

    /// The path as a start makes it, over the same journal and stores (a process started again).
    fn restart(serve: &Arc<Serve>, journal: &Arc<Journal>) -> (Commands, Arc<Picture>) {
        let path = Commands::new(journal.clone(), Some(serve.clone()), Arc::new(|| 14));
        path.set_clock(|| T0);
        let kind = Picture::new(serve.clone());
        path.register(kind.clone());
        (path, kind)
    }

    impl World {
        fn pick(&self, picture: &str) -> Pick {
            Pick { serve: self.serve.clone(), record: self.record, picture: picture.into() }
        }

        fn picture(&self) -> Option<String> {
            self.serve.catalogue().facts_said(self.record).unwrap().into_iter().find(|f| f.fact == "test-picture").and_then(|f| f.value)
        }

        async fn undo(&self, v: &Viewer, token: &str) -> UndoAnswer {
            self.path.undo(v, &[token.to_string()], &HashMap::new()).await
        }

        fn state(&self, token: &str) -> UndoState {
            self.journal.change_entry(ChangeId::of(token).unwrap()).unwrap().unwrap().change.state
        }
    }

    fn refusal(a: &UndoAnswer) -> (u16, &str) {
        assert!(a.undone.is_empty(), "{a:?}");
        let r = &a.refused[0];
        (r.status, r.error.as_str())
    }

    /// AP1: a new kind joins the path by its `Undoable` alone (its kind with its scope, its command, its Undo): no route,
    /// token, table, folder or rights rule of its own. Red first: new (no path on the base: five token shapes on two routes).
    #[tokio::test]
    async fn a_new_kind_joins_the_path_by_its_undoable_alone() {
        let w = world();
        // root picks with key K: `{ result, undo, rev }`, the token at once and the rev its own rows took.
        let first = w.path.run(&root(), Some("K"), &w.pick("sleeve.jpg")).await.unwrap();
        assert_eq!(first.result, "sleeve.jpg");
        let token = first.undo.clone().expect("a token at once");
        assert!(ChangeId::of(&token).is_some(), "one token kind: {token}");
        assert!(first.rev > 0 && first.rev == w.serve.userdata.change_rev().unwrap(), "its rev: {first:?}");
        // K sent again: the first answer, nothing made twice (ID2).
        assert_eq!(w.path.run(&root(), Some("K"), &w.pick("sleeve.jpg")).await.unwrap(), first);
        assert_eq!(w.journal.change_entry(ChangeId(ChangeId::of(&token).unwrap().0 + 1)).unwrap(), None, "one Change");
        // alice tries: refused by decide(), before anything is read.
        assert!(matches!(w.path.run(&alice(), None, &w.pick("other.jpg")).await, Err(ToggleError::Forbidden(_))));
        assert!(matches!(w.undo(&alice(), &token).await.refused[0].status, 403));
        assert_eq!(w.picture().as_deref(), Some("sleeve.jpg"), "refused: nothing changed");
        // root undoes: none again.
        let a = w.undo(&root(), &token).await;
        assert_eq!((a.undone.len(), a.undone[0].name.as_str(), a.undone[0].redo.clone()), (1, "Picture picked", None), "{a:?}");
        assert_eq!(w.picture(), None);
        assert!(matches!(w.state(&token), UndoState::Undone { by: Some(ref by), .. } if by == "root"));
        // root picks again, dave undoes it: shared, any administrator.
        let again = w.path.run(&root(), None, &w.pick("sleeve.jpg")).await.unwrap().undo.unwrap();
        assert_eq!(w.undo(&dave(), &again).await.undone.len(), 1);
        assert_eq!(w.picture(), None);
        // root picks, dave picks over it: root's Undo meets a change on top and changes nothing.
        let roots = w.path.run(&root(), None, &w.pick("sleeve.jpg")).await.unwrap().undo.unwrap();
        w.path.run(&dave(), None, &w.pick("inlay.jpg")).await.unwrap();
        assert_eq!(refusal(&w.undo(&root(), &roots).await), (409, "It changed since; nothing was undone."));
        assert_eq!(w.picture().as_deref(), Some("inlay.jpg"));
        assert_eq!(w.state(&roots), UndoState::Open, "refused: not undone, its claim handed back");
        // The window passes: no longer, its record kept (JS3).
        let daves = w.path.run(&dave(), None, &w.pick("back.jpg")).await.unwrap().undo.unwrap();
        w.path.set_clock(|| T0 + 25 * 3600);
        assert_eq!(refusal(&w.undo(&dave(), &daves).await), (409, "That can no longer be undone."));
        assert_eq!(w.journal.change_entry(ChangeId::of(&daves).unwrap()).unwrap().unwrap().change.name, "Picture picked", "kept for good");
        w.path.set_clock(|| T0);
        // A stop between the intent and the store's write: the start forgets the intent; nothing written.
        w.path.stop_at(Some(Stop::Intended));
        assert!(w.path.run(&root(), None, &w.pick("lost.jpg")).await.is_err());
        let (path, _) = restart(&w.serve, &w.journal);
        assert_eq!(path.settle().await.unwrap(), 1);
        assert_eq!(w.picture().as_deref(), Some("back.jpg"));
        // A stop between the store's write and `applied`: the start finds it written, applied with its Undo.
        path.stop_at(Some(Stop::Written));
        assert!(path.run(&root(), None, &w.pick("kept.jpg")).await.is_err());
        let written = w.journal.unsettled().unwrap()[0].change.id.token();
        let (path, _) = restart(&w.serve, &w.journal);
        assert_eq!(path.settle().await.unwrap(), 1);
        assert_eq!(w.journal.change_entry(ChangeId::of(&written).unwrap()).unwrap().unwrap().change.state, UndoState::Open);
        assert_eq!(path.undo(&root(), &[written], &HashMap::new()).await.undone.len(), 1);
        assert_eq!(w.picture().as_deref(), Some("back.jpg"));
        // A second page reads the rev: what its rows say at or after the answer's rev names the record.
        let rev = path.run(&root(), None, &w.pick("last.jpg")).await.unwrap().rev;
        let id = crate::serve::ids::record(w.record);
        assert!(change_log_names(&w.serve, rev - 1, &id), "the record at the answer's rev");
    }

    /// Whether the change log since `since` names record `id`.
    fn change_log_names(serve: &Serve, since: i64, id: &str) -> bool {
        use crate::userdata::offline::{ChangeKind as Logged, Since};
        match serve.userdata.changes_since(Some(since), "root", "", &[Logged::Record], 100).unwrap() {
            Since::Changes { changes, .. } => changes.iter().any(|c| c.item == id),
            Since::Reset { .. } => false,
        }
    }

    /// CP3 (on the test kind): an Undo is done once. Two at once, one applies and the other is a 409 having changed nothing; a
    /// second after the first likewise; one that fails on the way leaves the Change not undone, its claim handed back, and can be
    /// tried again. Red first: new (today two Undos of one album event at once both pass, correction 1).
    #[tokio::test]
    async fn an_undo_is_done_once() {
        let w = world();
        w.path.run(&dave(), None, &w.pick("before.jpg")).await.unwrap();
        let t = w.path.run(&root(), None, &w.pick("sleeve.jpg")).await.unwrap().undo.unwrap();
        let (by_root, by_dave) = (root(), dave());
        let (a, b) = tokio::join!(w.undo(&by_root, &t), w.undo(&by_dave, &t));
        assert_eq!(a.undone.len() + b.undone.len(), 1, "{a:?} {b:?}");
        let loser = if a.undone.is_empty() { &a } else { &b };
        assert_eq!(refusal(loser), (409, "That can no longer be undone."));
        assert_eq!(w.picture().as_deref(), Some("before.jpg"), "put back once");
        assert_eq!(refusal(&w.undo(&root(), &t).await), (409, "That can no longer be undone."), "a second, after");
        // (e): it fails on the way: not undone, the claim handed back, its why said; tried again, it goes.
        let t = w.path.run(&root(), None, &w.pick("sleeve.jpg")).await.unwrap().undo.unwrap();
        w.kind.fail_next();
        let failed = w.undo(&root(), &t).await;
        assert_eq!(refusal(&failed), (409, "The picture is held open."));
        assert_eq!((w.state(&t), w.picture().as_deref()), (UndoState::Open, Some("sleeve.jpg")));
        assert_eq!(w.undo(&root(), &t).await.undone.len(), 1);
        assert_eq!(w.picture().as_deref(), Some("before.jpg"));
    }

    /// CP4 (on the test kind): an Undo of several says what it did: a 200 naming each undone and each refused with its status and
    /// words; nothing undone lost from the answer; the refused stays not undone. Red first: new (today the first failure ends the
    /// answer, the earlier undone and their redos lost, api.rs:4500-4515).
    #[tokio::test]
    async fn an_undo_of_several_says_what_it_did() {
        let w = world();
        let first = w.path.run(&root(), None, &w.pick("one.jpg")).await.unwrap().undo.unwrap();
        let second = w.path.run(&root(), None, &w.pick("two.jpg")).await.unwrap().undo.unwrap();
        // Newest first: the second goes, then the first finds its picture back where it was, and goes too; an unknown token and a
        // 0.17 shape are refused.
        let a = w.path.undo(&root(), &["c12".to_string(), first.clone(), second.clone(), "j999".to_string()], &HashMap::new()).await;
        assert_eq!(a.undone.iter().map(|u| u.token.as_str()).collect::<Vec<_>>(), [second.as_str(), first.as_str()], "{a:?}");
        assert_eq!(a.refused.iter().map(|r| (r.token.as_str(), r.status, r.error.as_str())).collect::<Vec<_>>(), [
            ("j999", 409, "That can no longer be undone."),
            ("c12", 409, "That can no longer be undone.")
        ]);
        assert!(a.rev > 0);
    }

    /// CP8 (on the test kind): an Undo before its Change applied. Queued, not started: the Undo cancels it, 200 undone, the job
    /// never runs and nothing is written. Running: 409 "Still working; undo once it's done.", the job goes on; once applied, it is
    /// told, and its Undo works as any. Its start and an Undo at once: one outcome. Red first: new (a queued Keep answers
    /// `undo: None` today, incoming/src/app/incoming/queue.rs:386).
    #[tokio::test]
    async fn an_undo_before_its_change_applies_cancels_or_waits() {
        let w = world();
        let mut told = w.path.subscribe();
        // (a)–(c): queued, its token at once; undone before it starts.
        let q = w.path.queue(&root(), None, &w.pick("queued.jpg")).await.unwrap().unwrap();
        let token = q.answer.undo.clone().unwrap();
        assert_eq!(w.state(&token), UndoState::Waiting);
        let a = w.undo(&root(), &token).await;
        assert_eq!(a.undone.len(), 1, "{a:?}");
        assert!(w.path.start(&root(), q, &w.pick("queued.jpg")).await.unwrap().is_none(), "cancelled: it never runs");
        assert_eq!(w.picture(), None, "nothing applied");
        assert!(matches!(w.state(&token), UndoState::Undone { .. }));
        // (d)–(g): running; the route doesn't wait for it.
        let q = w.path.queue(&root(), None, &w.pick("running.jpg")).await.unwrap().unwrap();
        let token = q.answer.undo.clone().unwrap();
        assert!(w.journal.start_change(q.id).unwrap(), "its job starts");
        assert_eq!(refusal(&w.undo(&root(), &token).await), (409, "Still working; undo once it's done."));
        // The job goes on and applies (as `start` would, its turn taken): told, and its Undo works.
        let planned = q.planned.clone();
        let applied = w.path.apply(&root(), q.id, &w.pick("running.jpg"), &planned).await.unwrap();
        assert_eq!(applied.undo.as_deref(), Some(token.as_str()));
        assert_eq!(w.picture().as_deref(), Some("running.jpg"));
        loop {
            let t = told.recv().await.unwrap();
            if t.token == token {
                assert!(!t.undone && t.kind == ChangeKind::TestPicture && t.who.as_deref() == Some("root"));
                break;
            }
        }
        assert_eq!(w.undo(&root(), &token).await.undone.len(), 1);
        assert_eq!(w.picture(), None);
        // Its start and an Undo at once: one outcome, never both. The Undo first: cancelled before its first step, and the job,
        // coming after, never runs; the job first: it applies, and the Undo after it undoes what it applied, never a job
        // finishing after its Undo said "undone".
        let q = w.path.queue(&root(), None, &w.pick("raced.jpg")).await.unwrap().unwrap();
        let token = q.answer.undo.clone().unwrap();
        let (root, raced) = (root(), w.pick("raced.jpg"));
        let (undone, started) = tokio::join!(w.undo(&root, &token), w.path.start(&root, q, &raced));
        assert_eq!((undone.undone.len(), started.unwrap().is_some(), w.picture()), (1, false, None));
        let q = w.path.queue(&root, None, &raced).await.unwrap().unwrap();
        let token = q.answer.undo.clone().unwrap();
        let (started, undone) = tokio::join!(w.path.start(&root, q, &raced), w.undo(&root, &token));
        assert_eq!((started.unwrap().is_some(), undone.undone.len(), w.picture()), (true, 1, None));
        assert!(matches!(w.state(&token), UndoState::Undone { .. }));
    }

    /// CP9 (on the test kind's catalogue write): the answer's rev already holds its own Change. Its record's row is in the change
    /// log at once, before any refresh, at a rev no higher than the answer's; the refresh's owed-move then logs it no second time;
    /// a change made outside any command is still owed and moved at the refresh. Red first: new (today a record's rows are moved
    /// only at `Serve::refresh`, after the answer; album actions answer no rev).
    #[tokio::test]
    async fn an_answer_carries_the_rev_of_its_own_change() {
        let w = world();
        let before = w.serve.userdata.change_rev().unwrap();
        let a = w.path.run(&root(), None, &w.pick("sleeve.jpg")).await.unwrap();
        let id = crate::serve::ids::record(w.record);
        assert!(a.rev > before && change_log_names(&w.serve, before, &id), "at once, before any refresh: {a:?}");
        assert!(w.serve.catalogue().owed_records(100).unwrap().is_empty(), "nothing left owed for the refresh");
        w.serve.refresh().unwrap();
        assert_eq!(w.serve.userdata.change_rev().unwrap(), a.rev, "the refresh logged it no second time");
        // Outside any command: owed, and moved at the refresh, as LIVE has it.
        w.serve.catalogue().said(w.record, "year", "musicbrainz", "", Some("2015"), 9).unwrap();
        assert!(!change_log_names(&w.serve, a.rev, &id));
        w.serve.refresh().unwrap();
        assert!(change_log_names(&w.serve, a.rev, &id), "moved at the refresh");
        // An Undo's rev likewise, and told on the records' stream at that rev (CP7: the pages read the record again).
        let mut told = w.serve.subscribe_records();
        let u = w.undo(&root(), a.undo.as_ref().unwrap()).await;
        assert!(u.rev > a.rev && change_log_names(&w.serve, u.rev - 1, &id), "{u:?}");
        let seen = told.try_recv().expect("told at once");
        assert_eq!((seen.rev, seen.ids.iter().any(|c| c.id == id)), (u.rev, true), "{seen:?}");
    }
}
