//! One command path, one Undo (DOMAIN-CORE §11; SCENARIOS-GF §0): the record every command on the path keeps of what it changed
//! (a [`Change`]), its one token kind, the four refusals an Undo answers, each kind's scope, window and Redo, and the one rule of
//! who may undo ([`may_undo`]).
//!
//! - **One token kind** ([`ChangeId::token`]): `j<n>`, opaque to every client; an id given once is never given again.
//! - **Each kind declares** its scope ([`Scope`]), its window ([`Window`]) and whether its Undo answers a Redo: a new kind adds
//!   its [`ChangeKind`] and nothing else here (AP1).
//! - **Who may undo, one rule** (OQ-GF-1 (b), the owner 2026-11-20 23:59): per account, the account's alone; shared, its maker
//!   or any administrator; a player's, whoever controls that player now. Always with the right still held at the Undo.
//! - **In order:** the right first (a 403, before anything else is read), then the Change's state and window (a 409).

use serde::{Deserialize, Serialize};

use crate::can::{Decision, Scope as KeyScope, Viewer, Why};

/// A Change's id. Its token is the one kind a client holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChangeId(pub i64);

impl ChangeId {
    /// The token every answer carries (CP1): `j<n>`.
    pub fn token(self) -> String {
        format!("j{}", self.0)
    }

    /// The Change a token names: `j<n>` exactly as [`ChangeId::token`] writes it; any other shape names none.
    pub fn of(token: &str) -> Option<Self> {
        let digits = token.strip_prefix('j')?;
        let n: i64 = digits.parse().ok()?;
        (n > 0 && n.to_string() == digits).then_some(Self(n))
    }
}

/// An album action on the gate (0092's one path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlbumAction {
    Keep,
    Move,
    CopyTo,
    Replace,
    ReplaceFromCopy,
    Delete,
    DeleteOwn,
}

/// A queue's Change (the owner, 2026-11-20 20:11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueueChange {
    Shuffle,
    Clear,
    NewQueue,
}

/// What a Change is (T-GF-1's reach). DC1a's cover pick, DC1b's membership tick (a playlist's), DC3's join and AP01's picture
/// pick are added as they land, each with its scope, window and Redo below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    Playlist,
    /// One request of a playlist's offline ops (T-GF-19).
    PlaylistOps,
    Crate,
    /// The tracks excluded everywhere.
    Excluded,
    /// An artist put on or taken off an account's everywhere list.
    ArtistOut,
    /// Discover's answers: Not interested, Hide, artist Not interested.
    Discover,
    Album(AlbumAction),
    /// A run of changes to the library's files (0092): Health's repairs, a fetched track's add.
    FileRun,
    /// An account's own import of its listening.
    Import,
    /// An administrators' import of another server's users' listening, into several accounts (T-GF-18).
    ImportUsers,
    /// 0.17's BPM change of another server's track, folded at the step only.
    Tempo,
    Queue(QueueChange),
    TakeBack,
    Pick,
    /// The path's own test kind (AP1): one object's picked picture, shared, an administrators' command.
    #[cfg(any(test, feature = "test-support"))]
    TestPicture,
}

/// Whose a Change is to undo, as its kind declares it (OQ-GF-1 (b)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Kept in the account's own store (playlists, crates, hearts, Out, Discover's answers, excluded tracks, its imports): the
    /// account's alone, administrators included.
    Account,
    /// The library, arrivals, requests, Soulseek, Health's runs: its maker, or any administrator.
    Shared,
    /// A player's queue: whoever controls that player now.
    Player,
}

/// How long a Change can be undone, read at the Undo (OQ-GF-2 (b), the owner).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    /// The kinds without files: 24 hours.
    Day,
    /// The file kinds: the trash's days.
    TrashDays,
    /// An import: while its rows exist (its kind says when they are gone).
    WhileItsRowsExist,
    /// A Pick: until its placing begins (its kind says when it began).
    UntilPlacingBegins,
}

/// A day, in seconds: the window of the kinds without files.
pub const DAY: i64 = 24 * 3600;

impl Window {
    /// Whether a Change made `at` is still in this window `now` (epoch seconds). The two windows a kind keeps itself are open
    /// here: its Undo says "That can no longer be undone." when they have closed.
    pub fn open(self, at: i64, now: i64, trash_days: i64) -> bool {
        match self {
            Window::Day => now - at < DAY,
            Window::TrashDays => now - at < trash_days * DAY,
            Window::WhileItsRowsExist | Window::UntilPlacingBegins => true,
        }
    }
}

impl ChangeKind {
    /// Every kind, for the tables that read them all.
    pub fn all() -> Vec<ChangeKind> {
        use AlbumAction as A;
        use QueueChange as Q;
        #[allow(unused_mut)]
        let mut all = vec![
            ChangeKind::Playlist,
            ChangeKind::PlaylistOps,
            ChangeKind::Crate,
            ChangeKind::Excluded,
            ChangeKind::ArtistOut,
            ChangeKind::Discover,
            ChangeKind::Album(A::Keep),
            ChangeKind::Album(A::Move),
            ChangeKind::Album(A::CopyTo),
            ChangeKind::Album(A::Replace),
            ChangeKind::Album(A::ReplaceFromCopy),
            ChangeKind::Album(A::Delete),
            ChangeKind::Album(A::DeleteOwn),
            ChangeKind::FileRun,
            ChangeKind::Import,
            ChangeKind::ImportUsers,
            ChangeKind::Tempo,
            ChangeKind::Queue(Q::Shuffle),
            ChangeKind::Queue(Q::Clear),
            ChangeKind::Queue(Q::NewQueue),
            ChangeKind::TakeBack,
            ChangeKind::Pick,
        ];
        #[cfg(any(test, feature = "test-support"))]
        all.push(ChangeKind::TestPicture);
        all
    }

    /// Its name, as the journal keeps it and the registry finds its `Undoable` by.
    pub fn name(self) -> &'static str {
        use AlbumAction as A;
        use QueueChange as Q;
        match self {
            ChangeKind::Playlist => "playlist",
            ChangeKind::PlaylistOps => "playlist_ops",
            ChangeKind::Crate => "crate",
            ChangeKind::Excluded => "excluded",
            ChangeKind::ArtistOut => "artist_out",
            ChangeKind::Discover => "discover",
            ChangeKind::Album(A::Keep) => "album:keep",
            ChangeKind::Album(A::Move) => "album:move",
            ChangeKind::Album(A::CopyTo) => "album:copy_to",
            ChangeKind::Album(A::Replace) => "album:replace",
            ChangeKind::Album(A::ReplaceFromCopy) => "album:replace_from_copy",
            ChangeKind::Album(A::Delete) => "album:delete",
            ChangeKind::Album(A::DeleteOwn) => "album:delete_own",
            ChangeKind::FileRun => "file_run",
            ChangeKind::Import => "import",
            ChangeKind::ImportUsers => "import_users",
            ChangeKind::Tempo => "tempo",
            ChangeKind::Queue(Q::Shuffle) => "queue:shuffle",
            ChangeKind::Queue(Q::Clear) => "queue:clear",
            ChangeKind::Queue(Q::NewQueue) => "queue:new_queue",
            ChangeKind::TakeBack => "take_back",
            ChangeKind::Pick => "pick",
            #[cfg(any(test, feature = "test-support"))]
            ChangeKind::TestPicture => "test_picture",
        }
    }

    /// The kind a name names.
    pub fn of(name: &str) -> Option<Self> {
        Self::all().into_iter().find(|k| k.name() == name)
    }

    /// Whose it is to undo (each kind declares one: a new kind follows the rule without a list).
    pub fn scope(self) -> Scope {
        match self {
            ChangeKind::Playlist
            | ChangeKind::PlaylistOps
            | ChangeKind::Crate
            | ChangeKind::Excluded
            | ChangeKind::ArtistOut
            | ChangeKind::Discover
            | ChangeKind::Import => Scope::Account,
            ChangeKind::Album(_) | ChangeKind::FileRun | ChangeKind::ImportUsers | ChangeKind::Tempo | ChangeKind::TakeBack | ChangeKind::Pick => {
                Scope::Shared
            }
            ChangeKind::Queue(_) => Scope::Player,
            #[cfg(any(test, feature = "test-support"))]
            ChangeKind::TestPicture => Scope::Shared,
        }
    }

    /// How long it can be undone (OQ-GF-2 (b)).
    pub fn window(self) -> Window {
        match self {
            ChangeKind::Album(_) | ChangeKind::FileRun => Window::TrashDays,
            ChangeKind::Import | ChangeKind::ImportUsers => Window::WhileItsRowsExist,
            ChangeKind::Pick => Window::UntilPlacingBegins,
            ChangeKind::Playlist
            | ChangeKind::PlaylistOps
            | ChangeKind::Crate
            | ChangeKind::Excluded
            | ChangeKind::ArtistOut
            | ChangeKind::Discover
            | ChangeKind::Tempo
            | ChangeKind::Queue(_)
            | ChangeKind::TakeBack => Window::Day,
            #[cfg(any(test, feature = "test-support"))]
            ChangeKind::TestPicture => Window::Day,
        }
    }

    /// Whether its Undo answers a Redo, a new Change on the same path (OQ-GF-3 (a): where today's kind has one).
    pub fn redo(self) -> bool {
        match self {
            ChangeKind::Playlist | ChangeKind::PlaylistOps | ChangeKind::Crate | ChangeKind::Excluded | ChangeKind::ArtistOut | ChangeKind::Discover => true,
            ChangeKind::Album(_)
            | ChangeKind::FileRun
            | ChangeKind::Import
            | ChangeKind::ImportUsers
            | ChangeKind::Tempo
            | ChangeKind::Queue(_)
            | ChangeKind::TakeBack
            | ChangeKind::Pick => false,
            #[cfg(any(test, feature = "test-support"))]
            ChangeKind::TestPicture => false,
        }
    }
}

/// One planned step of a Change and how it came out (GA4: a side-write is a step, undone or said).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub what: String,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Done,
    Undone,
    /// Not undone, and why (a side-write that can't be undone by nature is named, T-GF-14).
    NotUndone(String),
}

/// Where a Change stands for its Undo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoState {
    /// Its job hasn't started: an Undo cancels it, and nothing is applied (CP8).
    Waiting,
    /// Being applied (its job runs, or its command applies): an Undo is a 409, "Still working; undo once it's done." (CP8).
    Running,
    /// Applied: undoable while its kind's window holds.
    Open,
    /// An Undo of it under way: another is a 409 (CP3).
    Undoing { by: Option<String> },
    Undone { at: i64, by: Option<String> },
    /// Its undo data gone: it can't be undone any more.
    NoLonger,
}

/// What a command on the path changed: kept for good, its undo data pruned by its kind's window (T-GF-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub id: ChangeId,
    pub kind: ChangeKind,
    /// Who made it; none for Cratarr itself, or a row older than accounts (administrators' to undo).
    pub who: Option<String>,
    /// When, epoch seconds.
    pub at: i64,
    /// What it is about, as the pages name it (`artist:<key>`, `al-42`, a player).
    pub object: String,
    /// How the toast and the lists name it, in the server's words.
    pub name: String,
    /// What undoing it needs (its before and after), as its kind keeps it: the path never reads it.
    pub data: String,
    pub steps: Vec<Step>,
    pub state: UndoState,
    /// The `Idempotency-Key` it came with, its account's: a resend finds it.
    pub key: Option<String>,
    /// The change log's rev its own rows were written at, in its apply (CP9); none before it applied.
    pub rev: Option<i64>,
}

/// The four refusals of an Undo (T-GF-3), from the one wording table ([`said`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// 403: the rights (the scope's rule, or the right no longer held).
    NotYours,
    /// 409: past its window, already undone or being undone, its undo data gone.
    NoLonger,
    /// 409: a change on top; why, where the kind knows it ("Its files moved").
    ChangedSince(Option<String>),
    /// 409: its Change's job is running.
    StillWorking,
}

/// The words of an Undo's refusals.
pub mod said {
    pub const NOT_YOURS: &str = "That isn't yours to undo.";
    pub const NO_LONGER: &str = "That can no longer be undone.";
    pub const CHANGED_SINCE: &str = "It changed since; nothing was undone.";
    pub const STILL_WORKING: &str = "Still working; undo once it's done.";
}

impl Refusal {
    pub fn status(&self) -> u16 {
        match self {
            Refusal::NotYours => 403,
            Refusal::NoLonger | Refusal::ChangedSince(_) | Refusal::StillWorking => 409,
        }
    }

    pub fn words(&self) -> &'static str {
        match self {
            Refusal::NotYours => said::NOT_YOURS,
            Refusal::NoLonger => said::NO_LONGER,
            Refusal::ChangedSince(_) => said::CHANGED_SINCE,
            Refusal::StillWorking => said::STILL_WORKING,
        }
    }

    /// Why it changed, where the kind knows it.
    pub fn why(&self) -> Option<&str> {
        match self {
            Refusal::ChangedSince(why) => why.as_deref(),
            Refusal::NotYours | Refusal::NoLonger | Refusal::StillWorking => None,
        }
    }

    /// As `decide()` answers a refusal: a right a 403, a state a 409.
    pub fn as_why(&self) -> Why {
        match self {
            Refusal::NotYours => Why::Right(said::NOT_YOURS),
            other => Why::State(other.words()),
        }
    }
}

/// Who may undo, as one rule of the viewer, the Change's maker and its kind's scope (DC1a's rule shape; OQ-GF-1 (b)): per
/// account, its account alone; shared, its maker or any administrator; a player's, whoever `controls` it now. A key that reads
/// undoes nothing; a key that controls undoes a player's Change, and, an administrator's, shared data (Home Assistant's review
/// and its Undo); an account's own data only by a sign-in or a full key. `maker`: none for Cratarr itself or a row older than
/// accounts, the administrators'.
pub fn undoer(v: &Viewer, maker: Option<&str>, scope: Scope, controls: bool) -> Result<(), &'static str> {
    let made = maker.is_some_and(|m| m == v.account);
    let unmade = maker.is_none() && v.role.administers();
    let may = match (scope, v.scope) {
        (_, KeyScope::Read) => false,
        (Scope::Player, _) => controls,
        (Scope::Shared, KeyScope::Full) => made || v.role.administers(),
        (Scope::Shared, KeyScope::Control) => v.role.administers(),
        (Scope::Account, KeyScope::Full) => made || unmade,
        (Scope::Account, KeyScope::Control) => false,
    };
    if may { Ok(()) } else { Err(said::NOT_YOURS) }
}

/// What an Undo goes on to do once it may.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Go {
    /// Undo the applied Change.
    Undo,
    /// Cancel its job before it starts: nothing was applied (CP8).
    Cancel,
}

/// What the path reads at the Undo, besides the viewer and the Change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    /// Now, epoch seconds.
    pub now: i64,
    pub trash_days: i64,
    /// The right to make this Change, judged now by its kind's `decide()`: refused, nobody's Undo goes on (21:37).
    pub right: Decision,
    /// A player's Change: whether the viewer controls that player now.
    pub controls: bool,
}

/// Whether `v` may undo `c` now, and how (§0's `may_undo`): the rule ([`undoer`]) and the right held now first, a 403 before
/// anything else is read; then its state and its kind's window, a 409.
pub fn may_undo(v: &Viewer, c: &Change, f: &Facts) -> Result<Go, Refusal> {
    undoer(v, c.who.as_deref(), c.kind.scope(), f.controls).map_err(|_| Refusal::NotYours)?;
    if let Decision::Deny(_) = f.right {
        return Err(Refusal::NotYours);
    }
    match c.state {
        UndoState::Waiting => Ok(Go::Cancel),
        UndoState::Running => Err(Refusal::StillWorking),
        UndoState::Open if c.kind.window().open(c.at, f.now, f.trash_days) => Ok(Go::Undo),
        UndoState::Open | UndoState::Undoing { .. } | UndoState::Undone { .. } | UndoState::NoLonger => Err(Refusal::NoLonger),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::can::{May, Scope as Key};

    const T0: i64 = 1_800_000_000;
    const ALL: May = May { request: true, offline: true, playlists: true };

    fn root() -> Viewer {
        Viewer::new("root", true, None, May::default(), None)
    }

    fn dave() -> Viewer {
        Viewer::new("dave", true, None, May::default(), None)
    }

    fn alice() -> Viewer {
        Viewer::new("alice", false, None, ALL, None)
    }

    fn bob() -> Viewer {
        Viewer::new("bob", false, None, ALL, None)
    }

    /// A user with "Manage requests", which makes no administrator.
    fn carol() -> Viewer {
        Viewer::new("carol", false, None, ALL, None)
    }

    fn reading_key() -> Viewer {
        Viewer::new("root", true, Some(Key::Read), May::default(), None)
    }

    /// Home Assistant's key: root's, controlling the players.
    fn control_key() -> Viewer {
        Viewer::new("root", true, Some(Key::Control), May::default(), None)
    }

    fn change(kind: ChangeKind, who: &str) -> Change {
        Change {
            id: ChangeId(7),
            kind,
            who: Some(who.to_string()),
            at: T0,
            object: "x".into(),
            name: "x".into(),
            data: String::new(),
            steps: Vec::new(),
            state: UndoState::Open,
            key: None,
            rev: Some(1),
        }
    }

    fn facts(controls: bool) -> Facts {
        Facts { now: T0 + 60, trash_days: 14, right: Decision::Allow, controls }
    }

    /// CP1's pure half: one token kind, `j<n>`, read back only as written; none of today's five shapes is one.
    #[test]
    fn one_token_kind() {
        assert_eq!(ChangeId(42).token(), "j42");
        assert_eq!(ChangeId::of("j42"), Some(ChangeId(42)));
        for other in ["c42", "m42", "r42", "l42", "du:no:al-1", "dr:hide:al-1", "42", "j", "j0", "j-1", "j01", "j4a", "J42", " j42"] {
            assert_eq!(ChangeId::of(other), None, "{other}");
        }
    }

    /// CP6's rule, by every scope (OQ-GF-1 (b) and the owner's 23:59): one rule, `undoer`, read through `may_undo`; every 403
    /// in one shape. Red first: four rules today (MAP-GF §8.1), none of them here.
    #[test]
    fn who_may_undo_is_one_rule() {
        let playlist = change(ChangeKind::Playlist, "alice");
        let discover = change(ChangeKind::Discover, "alice");
        let excluded = change(ChangeKind::Excluded, "alice");
        let import = change(ChangeKind::Import, "alice");
        let out = change(ChangeKind::ArtistOut, "alice");
        let keep = change(ChangeKind::Album(AlbumAction::Keep), "root");
        let repair = change(ChangeKind::FileRun, "root");
        let users_import = change(ChangeKind::ImportUsers, "root");
        let alices_delete = change(ChangeKind::Album(AlbumAction::Delete), "alice");
        let roots_delete = change(ChangeKind::Album(AlbumAction::Delete), "root");
        let take_back = change(ChangeKind::TakeBack, "alice");
        let clear = change(ChangeKind::Queue(QueueChange::Clear), "alice");
        let undone = |v: &Viewer, c: &Change, controls: bool| may_undo(v, c, &facts(controls));
        let refused = |v: &Viewer, c: &Change, controls: bool| undone(v, c, controls) == Err(Refusal::NotYours);
        // Per account: the account's alone, administrators included.
        assert_eq!(undone(&alice(), &playlist, false), Ok(Go::Undo));
        for c in [&playlist, &discover, &excluded, &import, &out] {
            assert!(refused(&root(), c, false), "root, alice's {:?}", c.kind);
            assert!(refused(&bob(), c, false), "bob, alice's {:?}", c.kind);
        }
        // Shared: its maker, or any administrator.
        assert!(refused(&alice(), &keep, false), "alice, root's Keep (AR4)");
        assert_eq!(undone(&dave(), &keep, false), Ok(Go::Undo), "dave, root's Keep");
        assert_eq!(undone(&dave(), &repair, false), Ok(Go::Undo), "dave, root's repair");
        assert_eq!(undone(&dave(), &users_import, false), Ok(Go::Undo), "dave, root's import of another server's users (T-GF-18)");
        assert!(refused(&alice(), &users_import, false));
        assert!(refused(&alice(), &roots_delete, false), "alice, root's Delete of her arrival");
        assert_eq!(undone(&root(), &alices_delete, false), Ok(Go::Undo), "root, alice's own Delete (X5)");
        assert_eq!(undone(&alice(), &alices_delete, false), Ok(Go::Undo), "alice, her own Delete");
        assert_eq!(undone(&root(), &take_back, false), Ok(Go::Undo), "root, alice's Take back: requests are shared");
        assert!(refused(&bob(), &take_back, false));
        assert!(refused(&carol(), &take_back, false), "Manage requests makes no administrator");
        // The reading key: none.
        for c in [&playlist, &keep, &repair, &take_back, &clear, &roots_delete] {
            assert!(refused(&reading_key(), c, true), "the reading key, {:?}", c.kind);
        }
        // A player's: control of it now, whoever made it; an administrator who doesn't control it, none.
        assert_eq!(undone(&bob(), &clear, true), Ok(Go::Undo), "bob at home, alice's clear of Salon");
        assert!(refused(&bob(), &clear, false), "bob away");
        assert!(refused(&root(), &clear, false), "root, alice's clear of her Phone");
        assert_eq!(undone(&control_key(), &clear, true), Ok(Go::Undo), "Home Assistant's key, controlling it");
        // A control key: an administrator's undoes shared data (the review's Undo); no account's own data.
        assert_eq!(undone(&control_key(), &keep, false), Ok(Go::Undo));
        assert!(refused(&control_key(), &change(ChangeKind::Playlist, "root"), false));
        // The right judged now (21:37): alice's own playlist edit, her "Change playlists" taken away.
        let gone = Facts { right: Decision::Deny(Why::Right("Making playlists is not on for this account")), ..facts(false) };
        assert_eq!(may_undo(&alice(), &playlist, &gone), Err(Refusal::NotYours));
        // Cratarr's own, or a row older than accounts: the administrators'.
        let nobodys = Change { who: None, ..keep.clone() };
        assert_eq!(undone(&dave(), &nobodys, false), Ok(Go::Undo));
        assert!(refused(&alice(), &nobodys, false));
        // Every 403 is one shape.
        assert_eq!((Refusal::NotYours.status(), Refusal::NotYours.words()), (403, "That isn't yours to undo."));
    }

    /// The order (T-GF-3): the right before the state, so a refusal by right reads nothing of where the Change stands; then the
    /// state (CP3, CP8) and the window (JS2).
    #[test]
    fn an_undo_reads_its_right_first_then_its_state() {
        let mut c = change(ChangeKind::Album(AlbumAction::Keep), "root");
        for state in [UndoState::Waiting, UndoState::Running, UndoState::Undone { at: T0, by: None }] {
            c.state = state;
            assert_eq!(may_undo(&alice(), &c, &facts(false)), Err(Refusal::NotYours), "{:?}", c.state);
        }
        c.state = UndoState::Waiting;
        assert_eq!(may_undo(&root(), &c, &facts(false)), Ok(Go::Cancel), "queued, not started: cancelled");
        c.state = UndoState::Running;
        assert_eq!(may_undo(&root(), &c, &facts(false)), Err(Refusal::StillWorking));
        for state in [UndoState::Undoing { by: Some("dave".into()) }, UndoState::Undone { at: T0, by: Some("dave".into()) }, UndoState::NoLonger] {
            c.state = state;
            assert_eq!(may_undo(&root(), &c, &facts(false)), Err(Refusal::NoLonger), "{:?}", c.state);
        }
        // The window, read at the Undo: the trash's days for the file kinds, a day for the others.
        c.state = UndoState::Open;
        let at = |days: i64| Facts { now: T0 + days * DAY - 1, ..facts(false) };
        assert_eq!(may_undo(&root(), &c, &at(14)), Ok(Go::Undo));
        assert_eq!(may_undo(&root(), &c, &Facts { now: T0 + 14 * DAY, ..facts(false) }), Err(Refusal::NoLonger));
        let out = change(ChangeKind::ArtistOut, "alice");
        assert_eq!(may_undo(&alice(), &out, &at(1)), Ok(Go::Undo));
        assert_eq!(may_undo(&alice(), &out, &Facts { now: T0 + DAY, ..facts(false) }), Err(Refusal::NoLonger));
    }

    /// CP2's and JS2's table: every kind declares its scope, its window and whether it has a Redo, as the scenarios say; and
    /// its name reads back. Never a kind with no scope (the match is the declaration: a kind left out does not build).
    #[test]
    fn each_kind_declares_its_scope_window_and_redo() {
        use AlbumAction as A;
        let rows: [(ChangeKind, Scope, Window, bool); 23] = [
            (ChangeKind::Playlist, Scope::Account, Window::Day, true),
            (ChangeKind::PlaylistOps, Scope::Account, Window::Day, true),
            (ChangeKind::Crate, Scope::Account, Window::Day, true),
            (ChangeKind::Excluded, Scope::Account, Window::Day, true),
            (ChangeKind::ArtistOut, Scope::Account, Window::Day, true),
            (ChangeKind::Discover, Scope::Account, Window::Day, true),
            (ChangeKind::Album(A::Keep), Scope::Shared, Window::TrashDays, false),
            (ChangeKind::Album(A::Move), Scope::Shared, Window::TrashDays, false),
            (ChangeKind::Album(A::CopyTo), Scope::Shared, Window::TrashDays, false),
            (ChangeKind::Album(A::Replace), Scope::Shared, Window::TrashDays, false),
            (ChangeKind::Album(A::ReplaceFromCopy), Scope::Shared, Window::TrashDays, false),
            (ChangeKind::Album(A::Delete), Scope::Shared, Window::TrashDays, false),
            (ChangeKind::Album(A::DeleteOwn), Scope::Shared, Window::TrashDays, false),
            (ChangeKind::FileRun, Scope::Shared, Window::TrashDays, false),
            (ChangeKind::Import, Scope::Account, Window::WhileItsRowsExist, false),
            (ChangeKind::ImportUsers, Scope::Shared, Window::WhileItsRowsExist, false),
            (ChangeKind::Tempo, Scope::Shared, Window::Day, false),
            (ChangeKind::Queue(QueueChange::Shuffle), Scope::Player, Window::Day, false),
            (ChangeKind::Queue(QueueChange::Clear), Scope::Player, Window::Day, false),
            (ChangeKind::Queue(QueueChange::NewQueue), Scope::Player, Window::Day, false),
            (ChangeKind::TakeBack, Scope::Shared, Window::Day, false),
            (ChangeKind::Pick, Scope::Shared, Window::UntilPlacingBegins, false),
            (ChangeKind::TestPicture, Scope::Shared, Window::Day, false),
        ];
        assert_eq!(rows.len(), ChangeKind::all().len(), "every kind in the table");
        for (kind, scope, window, redo) in rows {
            assert_eq!((kind.scope(), kind.window(), kind.redo()), (scope, window, redo), "{kind:?}");
            assert_eq!(ChangeKind::of(kind.name()), Some(kind), "{kind:?} reads back");
        }
        let names: std::collections::HashSet<&str> = ChangeKind::all().into_iter().map(ChangeKind::name).collect();
        assert_eq!(names.len(), ChangeKind::all().len(), "one name a kind");
        assert_eq!(ChangeKind::of("c"), None);
    }

    /// T-GF-3: one refusal shape, four words, two statuses; as `decide()` says a refusal, a right a 403 and a state a 409.
    #[test]
    fn the_refusals_are_one_shape() {
        let all = [Refusal::NotYours, Refusal::NoLonger, Refusal::ChangedSince(Some("Its files moved".into())), Refusal::StillWorking];
        let shown: Vec<(u16, &str)> = all.iter().map(|r| (r.status(), r.words())).collect();
        assert_eq!(
            shown,
            [
                (403, "That isn't yours to undo."),
                (409, "That can no longer be undone."),
                (409, "It changed since; nothing was undone."),
                (409, "Still working; undo once it's done.")
            ]
        );
        assert_eq!(all[2].why(), Some("Its files moved"));
        for r in &all {
            assert_eq!(r.as_why().status(), r.status());
            assert_eq!(r.as_why().words(), r.words());
        }
    }
}
