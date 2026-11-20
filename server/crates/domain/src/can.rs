//! Who may do what with a record, its copies and its arrivals (DOMAIN-CORE §5; SCENARIOS-DC1 CN1-CN13): one [`decide`] for
//! every view and every handler, its spec the policy table (CN1, `tests/policy.json`). A view offers what `decide` allows; a
//! handler asks it first and answers a refusal with its words: a right missing is a 403, a state a 409, an upload hidden a
//! 404 ([`Why::status`]).
//!
//! - **One viewer per request** ([`Viewer`]): its role decided once ([`Role::of`], CN8). No right here reads an account's
//!   stored flag, and the page decides none (CN13).
//! - **In order:** hidden first (an upload is its uploader's and the administrators' until it is added, UP03); then the
//!   state (what the record, the copy or the arrival is now); then the right (the key's scope, the role, the account's
//!   permissions, whose copy or request it is).
//! - **Facts, not lookups** ([`Record`]): what the store and the requests hand over. Nothing here reads them itself.

use cratarr_rules::holding::{Holding, Library};
use serde::{Deserialize, Serialize};

use crate::arrival::Arrival;

/// A viewer's role (CN8): an administrator signed in, an administrator's API key, or a user (signed in or by key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Administrator,
    AdministratorsKey,
    User,
}

impl Role {
    /// The one place a role is decided (CN8): from the account's own flag and whether the request came with an API key.
    pub fn of(admin: bool, by_key: bool) -> Role {
        match (admin, by_key) {
            (true, false) => Role::Administrator,
            (true, true) => Role::AdministratorsKey,
            (false, _) => Role::User,
        }
    }

    /// An administrator, or their key: its scope still bounds what it does.
    pub fn administers(self) -> bool {
        matches!(self, Role::Administrator | Role::AdministratorsKey)
    }
}

/// How far a request may go: a sign-in or a full key everywhere its account may; a key that controls the players; a key
/// that reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Full,
    Control,
    Read,
}

/// An account's permissions that a record's actions read (the Users panel, decision 0051): "Request albums", "Download in
/// apps", "Make playlists". An administrator has every one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct May {
    pub request: bool,
    pub offline: bool,
    pub playlists: bool,
}

/// Who asks, once per request (CN8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewer {
    /// The account signed in, or the one that made the key.
    pub account: String,
    pub role: Role,
    pub scope: Scope,
    pub may: May,
    /// Their own music's catalogue folder, while it is on: an arrival they asked for goes there (their default target).
    pub own: Option<i64>,
}

impl Viewer {
    /// A viewer, its role by [`Role::of`]: `key` is the scope of the API key the request came with, none for a sign-in.
    pub fn new(account: &str, admin: bool, key: Option<Scope>, may: May, own: Option<i64>) -> Self {
        Self { account: account.to_string(), role: Role::of(admin, key.is_some()), scope: key.unwrap_or(Scope::Full), may, own }
    }
}

/// A library as an action's target names it: its catalogue folder, and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub folder: i64,
    pub library: Library,
}

/// What an arrival is (the album review's facts): its state, whose requests it came for, and who answered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrivalFacts {
    pub state: Arrival,
    pub asked_by: Vec<String>,
    pub by: Option<String>,
}

/// One present copy of a record (the store's `copies_of`), as its actions read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordCopy {
    pub folder: i64,
    pub library: Library,
    /// When it was added: of a record only in people's own music, the first own copy's owner may Edit it (CN5).
    pub added: i64,
    pub converting: bool,
    /// An Incoming copy's arrival, or a copy kept from one (its Undo).
    pub arrival: Option<ArrivalFacts>,
}

/// What `decide` reads of a record, for one viewer in one place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Its present copies.
    pub copies: Vec<RecordCopy>,
    /// The copy that plays for this viewer here (`copy_for`'s answer), if one does.
    pub plays: Option<usize>,
    /// A page that previews it (Bandcamp's): the preview's Play row.
    pub preview: bool,
    /// The viewer's holding with every library counted, as the requests read it (HD7, HD15).
    pub holding: Holding,
    /// The accounts with an open request of it, or of its release group (want-path's rule, the requests' to work out).
    pub open_requests: Vec<String>,
}

/// What a viewer may ask of a record (SCENARIOS-DC1's shapes; the arrival's last four). An action on a copy names it, by its
/// place in [`Record::copies`] (CN6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Play,
    Shuffle,
    PlayNext,
    AddToQueue,
    StartRadio,
    Resume,
    Want,
    Requested,
    Favourite,
    Rating,
    Share,
    WebSearch,
    GeniusSearch,
    GoTo,
    SoundsLike,
    View,
    Edit,
    ChangeCover,
    MissingTracks,
    AddToPlaylist,
    SaveTo { copy: usize, to: Target },
    MoveTo { copy: usize, to: Target },
    /// Replace a library's copy (the better one, or a file, the plan's to pick: RY01, C37a).
    ReplaceIn { into: Target },
    PlayFrom { copy: usize },
    CancelConversion { copy: usize },
    Delete { copy: usize },
    Download,
    Sync,
    RemoveFromDevice,
    AddTo { copy: usize, to: Target },
    Replace { copy: usize, into: Target },
    DeleteArrival { copy: usize },
    Undo { copy: usize },
}

impl Action {
    /// The copy an action names, where it names one.
    pub fn copy(&self) -> Option<usize> {
        match self {
            Action::SaveTo { copy, .. }
            | Action::MoveTo { copy, .. }
            | Action::PlayFrom { copy }
            | Action::CancelConversion { copy }
            | Action::Delete { copy }
            | Action::AddTo { copy, .. }
            | Action::Replace { copy, .. }
            | Action::DeleteArrival { copy }
            | Action::Undo { copy } => Some(*copy),
            Action::Play
            | Action::Shuffle
            | Action::PlayNext
            | Action::AddToQueue
            | Action::StartRadio
            | Action::Resume
            | Action::Want
            | Action::Requested
            | Action::Favourite
            | Action::Rating
            | Action::Share
            | Action::WebSearch
            | Action::GeniusSearch
            | Action::GoTo
            | Action::SoundsLike
            | Action::View
            | Action::Edit
            | Action::ChangeCover
            | Action::MissingTracks
            | Action::AddToPlaylist
            | Action::ReplaceIn { .. }
            | Action::Download
            | Action::Sync
            | Action::RemoveFromDevice => None,
        }
    }
}

/// What a yes is asked for before a command runs (§11: the plan's own words come with the command path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Plan {
    Delete { copy: usize },
}

/// Why an action is refused, with its words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    /// The viewer may not: a 403.
    Right(&'static str),
    /// The record, the copy or the arrival is not so now: a 409.
    State(&'static str),
    /// An upload the viewer may not see: as if it were not there, a 404.
    Hidden,
}

impl Why {
    pub fn status(self) -> u16 {
        match self {
            Why::Right(_) => 403,
            Why::State(_) => 409,
            Why::Hidden => 404,
        }
    }

    pub fn words(self) -> &'static str {
        match self {
            Why::Right(words) | Why::State(words) => words,
            Why::Hidden => said::HIDDEN,
        }
    }
}

/// `decide`'s answer: go ahead; go ahead once the viewer says yes; or no, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    AskFirst(Plan),
    Deny(Why),
}

/// The refusals' words, in one place (CN1's Never: no reason without words).
pub mod said {
    pub const ADMINISTRATORS: &str = "This is for administrators.";
    pub const REQUEST: &str = "Requesting albums is not on for this account";
    pub const OFFLINE: &str = "Downloading in apps is not on for this account";
    pub const PLAYLISTS: &str = "Making playlists is not on for this account";
    pub const KEY: &str = "This key can't change anything.";
    pub const OWNER: &str = "This is for the owner of this music.";
    pub const ASKERS: &str = "This is for administrators and whoever asked for it.";
    pub const UNDOER: &str = "Only whoever did it can undo it.";
    pub const HIDDEN: &str = "Not found.";
    pub const NOTHING_PLAYS: &str = "Nothing of it plays here.";
    pub const NOT_HAD: &str = "Not in your library";
    pub const HAD: &str = "In your library";
    pub const REQUESTED: &str = "Requested";
    pub const NOT_REQUESTED: &str = "No request of yours is open.";
    pub const NO_COPY: &str = "That copy isn't there.";
    pub const CONVERTING: &str = "Its copy is being converted.";
    pub const NOT_CONVERTING: &str = "Its copy isn't being converted.";
    pub const NOT_A_TARGET: &str = "It can't go there.";
    pub const NOTHING_TO_REPLACE: &str = "No copy there to replace.";
    pub const UPLOAD_REPLACES_NOTHING: &str = "An upload replaces nothing.";
    pub const NOT_WAITING: &str = "It isn't waiting to be added.";
    pub const NOTHING_TO_UNDO: &str = "Nothing to undo.";
}

/// What `viewer` may do: `action` on `record`, now (CN1).
pub fn decide(viewer: &Viewer, action: &Action, record: &Record) -> Decision {
    judge(viewer, action, record).unwrap_or_else(Decision::Deny)
}

/// Whether `decide` lets it go ahead, with or without a yes first.
pub fn can(viewer: &Viewer, action: &Action, record: &Record) -> bool {
    !matches!(decide(viewer, action, record), Decision::Deny(_))
}

fn judge(v: &Viewer, a: &Action, r: &Record) -> Result<Decision, Why> {
    if let Some(copy) = a.copy()
        && let Some(c) = r.copies.get(copy)
        && hidden(c, v)
    {
        return Err(Why::Hidden);
    }
    state(v, a, r).map_err(Why::State)?;
    right(v, a, r).map_err(Why::Right)
}

/// Whose upload a copy is, while it waits in Incoming.
fn uploaded(c: &RecordCopy) -> Option<&str> {
    match &c.library {
        Library::Incoming { uploader: Some(uploader), .. } => Some(uploader.as_str()),
        _ => None,
    }
}

/// An upload is hidden from all but its uploader and the administrators (UP03).
fn hidden(c: &RecordCopy, v: &Viewer) -> bool {
    uploaded(c).is_some_and(|uploader| uploader != v.account.as_str() && !v.role.administers())
}

fn copy_at(r: &Record, copy: usize) -> Result<&RecordCopy, &'static str> {
    r.copies.get(copy).ok_or(said::NO_COPY)
}

fn asked(v: &Viewer, r: &Record) -> bool {
    r.open_requests.iter().any(|asker| asker == &v.account)
}

/// Whether the viewer asked for this arrival, or uploaded it.
fn asked_for(c: &RecordCopy, v: &Viewer) -> bool {
    uploaded(c) == Some(v.account.as_str()) || c.arrival.as_ref().is_some_and(|x| x.asked_by.iter().any(|asker| asker == &v.account))
}

fn plays(r: &Record) -> Result<(), &'static str> {
    if r.plays.is_some() { Ok(()) } else { Err(said::NOTHING_PLAYS) }
}

fn has_copy(v: &Viewer, r: &Record) -> Result<(), &'static str> {
    if r.copies.iter().any(|c| !hidden(c, v)) { Ok(()) } else { Err(said::NOT_HAD) }
}

fn unconverted(c: &RecordCopy) -> Result<(), &'static str> {
    if c.converting { Err(said::CONVERTING) } else { Ok(()) }
}

/// A copy may go into a main library or someone's own music, never into Incoming (F34) nor where it is.
fn target(c: &RecordCopy, to: &Target) -> Result<(), &'static str> {
    if to.folder == c.folder || matches!(to.library, Library::Incoming { .. }) { Err(said::NOT_A_TARGET) } else { Ok(()) }
}

/// A main library's copy to replace: present there, and not being converted.
fn replaceable(r: &Record, into: &Target) -> Result<(), &'static str> {
    if !matches!(into.library, Library::Main) {
        return Err(said::NOT_A_TARGET);
    }
    let there: Vec<&RecordCopy> = r.copies.iter().filter(|c| c.folder == into.folder).collect();
    if there.is_empty() {
        Err(said::NOTHING_TO_REPLACE)
    } else if there.iter().any(|c| c.converting) {
        Err(said::CONVERTING)
    } else {
        Ok(())
    }
}

/// An arrival waiting for its review: an Incoming copy not yet answered.
fn waiting(c: &RecordCopy) -> Result<&RecordCopy, &'static str> {
    let unanswered = c.arrival.as_ref().is_none_or(|x| x.state == Arrival::Waiting);
    if matches!(c.library, Library::Incoming { .. }) && unanswered { Ok(c) } else { Err(said::NOT_WAITING) }
}

/// What the record, the copy or the arrival must be now (a 409 where it isn't).
fn state(v: &Viewer, a: &Action, r: &Record) -> Result<(), &'static str> {
    match a {
        Action::Share | Action::WebSearch | Action::GeniusSearch | Action::GoTo | Action::View | Action::RemoveFromDevice => Ok(()),
        Action::Play => {
            if r.plays.is_some() || r.preview {
                Ok(())
            } else {
                Err(said::NOTHING_PLAYS)
            }
        }
        Action::Shuffle
        | Action::PlayNext
        | Action::AddToQueue
        | Action::StartRadio
        | Action::Resume
        | Action::AddToPlaylist
        | Action::Download
        | Action::Sync => plays(r),
        Action::SoundsLike | Action::Favourite | Action::Rating | Action::Edit | Action::ChangeCover | Action::MissingTracks => {
            has_copy(v, r)
        }
        Action::Want => {
            if r.holding == Holding::Had {
                Err(said::HAD)
            } else if asked(v, r) {
                Err(said::REQUESTED)
            } else {
                Ok(())
            }
        }
        Action::Requested => {
            if asked(v, r) {
                Ok(())
            } else {
                Err(said::NOT_REQUESTED)
            }
        }
        Action::PlayFrom { copy } => copy_at(r, *copy).map(|_| ()),
        Action::SaveTo { copy, to } => target(copy_at(r, *copy)?, to),
        Action::MoveTo { copy, to } => {
            let c = copy_at(r, *copy)?;
            unconverted(c)?;
            // Between main libraries only (§5): into own music is a Save, into Incoming never (F34).
            if matches!(c.library, Library::Main) && matches!(to.library, Library::Main) && to.folder != c.folder {
                Ok(())
            } else {
                Err(said::NOT_A_TARGET)
            }
        }
        Action::ReplaceIn { into } => replaceable(r, into),
        Action::CancelConversion { copy } => {
            if copy_at(r, *copy)?.converting {
                Ok(())
            } else {
                Err(said::NOT_CONVERTING)
            }
        }
        Action::Delete { copy } => unconverted(copy_at(r, *copy)?),
        Action::AddTo { copy, to } => target(waiting(copy_at(r, *copy)?)?, to),
        Action::Replace { copy, into } => {
            let c = waiting(copy_at(r, *copy)?)?;
            // UP03.4: an upload is added, never put in another's place.
            if uploaded(c).is_some() {
                return Err(said::UPLOAD_REPLACES_NOTHING);
            }
            replaceable(r, into)
        }
        Action::DeleteArrival { copy } => waiting(copy_at(r, *copy)?).map(|_| ()),
        Action::Undo { copy } => match copy_at(r, *copy)?.arrival.as_ref().map(|x| x.state) {
            Some(Arrival::Kept { .. } | Arrival::Replaced { .. } | Arrival::Rejected) => Ok(()),
            Some(Arrival::Waiting | Arrival::Gone) | None => Err(said::NOTHING_TO_UNDO),
        },
    }
}

/// The key's scope: what changes anything takes a sign-in or a full key.
fn full(v: &Viewer) -> Result<(), &'static str> {
    if v.scope == Scope::Full { Ok(()) } else { Err(said::KEY) }
}

fn administers(v: &Viewer) -> Result<(), &'static str> {
    if v.role.administers() { Ok(()) } else { Err(said::ADMINISTRATORS) }
}

/// A permission: an administrator has every one.
fn granted(v: &Viewer, may: bool, refused: &'static str) -> Result<(), &'static str> {
    if v.role.administers() || may { Ok(()) } else { Err(refused) }
}

/// Of a record only in people's own music, the owner of the copy added first (CN5).
fn first_owner(r: &Record) -> Option<&str> {
    let mut owners = Vec::new();
    for c in &r.copies {
        match &c.library {
            Library::Own(owner) => owners.push((c.added, owner.as_str())),
            Library::Main | Library::Incoming { .. } => return None,
        }
    }
    owners.into_iter().min_by_key(|&(added, _)| added).map(|(_, owner)| owner)
}

/// Who may delete a copy (§5): a main library's, administrators; own music's, its owner alone; Incoming's, administrators or
/// whoever asked for it (or uploaded it).
fn may_delete(v: &Viewer, c: &RecordCopy) -> Result<(), &'static str> {
    match &c.library {
        Library::Main => administers(v),
        Library::Own(owner) => {
            if owner == &v.account {
                Ok(())
            } else {
                Err(said::OWNER)
            }
        }
        Library::Incoming { .. } => {
            if v.role.administers() || asked_for(c, v) {
                Ok(())
            } else {
                Err(said::ASKERS)
            }
        }
    }
}

/// Whether a target is the viewer's own music, while it is on (their default target).
fn into_own(v: &Viewer, to: &Target) -> bool {
    matches!(&to.library, Library::Own(owner) if owner == &v.account) && v.own == Some(to.folder)
}

/// Whether the viewer may (a 403 where not), once the state allows it.
fn right(v: &Viewer, a: &Action, r: &Record) -> Result<Decision, &'static str> {
    match a {
        // Reading and playing: anyone, a key of any scope too (§5: "a reading key too").
        Action::Play
        | Action::Shuffle
        | Action::PlayNext
        | Action::AddToQueue
        | Action::StartRadio
        | Action::Resume
        | Action::SoundsLike
        | Action::PlayFrom { .. }
        | Action::Share
        | Action::WebSearch
        | Action::GeniusSearch
        | Action::GoTo
        | Action::View
        | Action::Requested
        | Action::RemoveFromDevice => Ok(Decision::Allow),
        Action::Favourite | Action::Rating => full(v).map(|()| Decision::Allow),
        Action::Want => {
            full(v)?;
            granted(v, v.may.request, said::REQUEST).map(|()| Decision::Allow)
        }
        Action::Download | Action::Sync => {
            full(v)?;
            granted(v, v.may.offline, said::OFFLINE).map(|()| Decision::Allow)
        }
        Action::AddToPlaylist => {
            full(v)?;
            granted(v, v.may.playlists, said::PLAYLISTS).map(|()| Decision::Allow)
        }
        Action::Edit => {
            full(v)?;
            if v.role.administers() || first_owner(r) == Some(v.account.as_str()) {
                Ok(Decision::Allow)
            } else {
                Err(said::ADMINISTRATORS)
            }
        }
        Action::ChangeCover
        | Action::MissingTracks
        | Action::SaveTo { .. }
        | Action::MoveTo { .. }
        | Action::ReplaceIn { .. }
        | Action::CancelConversion { .. }
        | Action::Replace { .. } => {
            full(v)?;
            administers(v).map(|()| Decision::Allow)
        }
        Action::Delete { copy } | Action::DeleteArrival { copy } => {
            full(v)?;
            may_delete(v, copy_at(r, *copy)?)?;
            Ok(Decision::AskFirst(Plan::Delete { copy: *copy }))
        }
        Action::AddTo { copy, to } => {
            full(v)?;
            if v.role.administers() {
                return Ok(Decision::Allow);
            }
            if !asked_for(copy_at(r, *copy)?, v) {
                return Err(said::ASKERS);
            }
            // Its asker adds it to their own music only, its default target.
            if into_own(v, to) { Ok(Decision::Allow) } else { Err(said::ADMINISTRATORS) }
        }
        Action::Undo { copy } => {
            full(v)?;
            let by = copy_at(r, *copy)?.arrival.as_ref().and_then(|x| x.by.as_deref());
            if by == Some(v.account.as_str()) { Ok(Decision::Allow) } else { Err(said::UNDOER) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cratarr_rules::holding::{CopyFacts, On, holding};

    // SCENARIOS-DC1's libraries, by their catalogue folders; root's own music for CN6's "Save to Your music".
    const MUSIC: i64 = 0;
    const IN: i64 = 1;
    const OWN_A: i64 = 2;
    const OWN_B: i64 = 3;
    const MORE: i64 = 4;
    const IN2: i64 = 5;
    const ROOTS: i64 = 7;
    const ALL: May = May { request: true, offline: true, playlists: true };

    fn root() -> Viewer {
        Viewer::new("root", true, None, May::default(), Some(ROOTS))
    }

    /// The reading key: an API key root made, scope Read.
    fn key() -> Viewer {
        Viewer::new("root", true, Some(Scope::Read), May::default(), Some(ROOTS))
    }

    fn alice() -> Viewer {
        Viewer::new("alice", false, None, ALL, Some(OWN_A))
    }

    fn bob() -> Viewer {
        Viewer::new("bob", false, None, ALL, Some(OWN_B))
    }

    /// A user with none of "Request albums", "Download in apps" and "Upload music".
    fn carol() -> Viewer {
        Viewer::new("carol", false, None, May { request: false, offline: false, playlists: true }, None)
    }

    fn main_lib(folder: i64) -> Target {
        Target { folder, library: Library::Main }
    }

    fn own_lib(folder: i64, owner: &str) -> Target {
        Target { folder, library: Library::Own(owner.to_string()) }
    }

    fn incoming_lib(folder: i64) -> Target {
        Target { folder, library: Library::Incoming { fetcher: cratarr_rules::holding::Fetcher::Lidarr, uploader: None } }
    }

    fn copy(target: Target, added: i64) -> RecordCopy {
        RecordCopy { folder: target.folder, library: target.library, added, converting: false, arrival: None }
    }

    fn arrival(target: Target, asked_by: &[&str]) -> RecordCopy {
        let asked_by = asked_by.iter().map(|a| a.to_string()).collect();
        RecordCopy { arrival: Some(ArrivalFacts { state: Arrival::Waiting, asked_by, by: None }), ..copy(target, 300) }
    }

    /// A record with these whole copies, for this viewer: its holding by the rules, every library counted.
    fn record(copies: Vec<RecordCopy>, plays: Option<usize>, v: &Viewer) -> Record {
        let facts: Vec<CopyFacts> =
            copies.iter().map(|c| CopyFacts { folder: c.folder, library: c.library.clone(), present: 10, paired: 10, went: None }).collect();
        let holding = holding(&facts, Some(10), &v.account, &On::all());
        Record { copies, plays, preview: false, holding, open_requests: Vec::new() }
    }

    fn not_had() -> Record {
        Record { copies: Vec::new(), plays: None, preview: false, holding: Holding::NotHad, open_requests: Vec::new() }
    }

    fn refused_right(d: Decision) -> bool {
        matches!(d, Decision::Deny(Why::Right(_)))
    }

    fn refused_state(d: Decision) -> bool {
        matches!(d, Decision::Deny(Why::State(_)))
    }

    /// CN2 (SB8). Red first: the page offers Want to everyone (recordNotHad.ts:34), Discover alone reads `may_request`
    /// (Discover.svelte:72), and no answer carries a 403 for it (app/requests.rs:279 refuses with its words alone).
    #[test]
    fn want_is_offered_only_with_request_albums() {
        // "Answered", not had. `decide` takes no place: from search, an artist's missing list, a label page, Beyond and
        // Discover alike, one answer; Discover has no `may_request` of its own.
        let answered = not_had();
        assert_eq!(decide(&alice(), &Action::Want, &answered), Decision::Allow);
        let refused = decide(&carol(), &Action::Want, &answered);
        assert_eq!(refused, Decision::Deny(Why::Right("Requesting albums is not on for this account")));
        assert!(!can(&carol(), &Action::Want, &answered), "never shown to carol");
        let Decision::Deny(why) = refused else { panic!("{refused:?}") };
        assert_eq!(why.status(), 403, "a right is a 403, never a 409");
        assert_eq!(why.words(), said::REQUEST);
    }

    /// CN3 (OQ-DC1-3 (a): Requested shows its words with no tap before DC2). Red first: the sheet has no Requested of the
    /// server's (want-path.test.ts decides it on the page).
    #[test]
    fn requested_replaces_want_for_its_asker() {
        let mut answered = not_had();
        answered.open_requests = vec!["alice".to_string()];
        assert!(refused_state(decide(&alice(), &Action::Want, &answered)), "alice: no Want while hers is open");
        assert_eq!(decide(&alice(), &Action::Requested, &answered), Decision::Allow);
        assert_eq!(decide(&bob(), &Action::Want, &answered), Decision::Allow, "bob: Want");
        assert!(!can(&bob(), &Action::Requested, &answered), "never Requested by another account's request");
        for v in [root(), key(), alice(), bob(), carol()] {
            assert!(!(can(&v, &Action::Want, &answered) && can(&v, &Action::Requested, &answered)), "never both: {}", v.account);
        }
        // After the take-back: Want again for alice.
        answered.open_requests.clear();
        assert_eq!(decide(&alice(), &Action::Want, &answered), Decision::Allow);
        assert!(!can(&alice(), &Action::Requested, &answered));
    }

    /// CN5. Red first: the page decides it (`mayEdit`, copiesRule.ts:316).
    #[test]
    fn edit_is_the_administrators_and_the_first_own_copys_owner() {
        let edit = |v: &Viewer, copies: Vec<RecordCopy>| decide(v, &Action::Edit, &record(copies, Some(0), v));
        // Only in OWN-A (alice's first) and OWN-B (bob's, later), listed bob's first: the first added decides.
        let both = || vec![copy(own_lib(OWN_B, "bob"), 200), copy(own_lib(OWN_A, "alice"), 100)];
        let only_a = || vec![copy(own_lib(OWN_A, "alice"), 100)];
        let with_music = || vec![copy(main_lib(MUSIC), 50), copy(own_lib(OWN_A, "alice"), 100)];
        assert_eq!(edit(&alice(), both()), Decision::Allow);
        assert_eq!(edit(&alice(), only_a()), Decision::Allow);
        assert!(refused_right(edit(&alice(), with_music())), "only root may Edit the third");
        assert!(refused_right(edit(&bob(), both())), "bob's copy came later");
        assert!(refused_right(edit(&bob(), only_a())));
        assert!(refused_right(edit(&bob(), with_music())));
        for copies in [both(), only_a(), with_music()] {
            assert_eq!(edit(&root(), copies), Decision::Allow);
        }
    }

    /// The Two Rooms arrival of CN7: `al-9` in IN, asked by alice (her own music on, OWN-A her default); MUSIC holds an
    /// older MP3.
    fn two_rooms(v: &Viewer) -> Record {
        record(vec![copy(main_lib(MUSIC), 10), arrival(incoming_lib(IN), &["alice"])], Some(0), v)
    }

    /// CN7. Red first: three paths decide apart today (api.rs:287-289, app/albums.rs:30-48, serve/review.rs).
    #[test]
    fn an_arrival_is_the_administrators_and_its_askers() {
        let add_own = Action::AddTo { copy: 1, to: own_lib(OWN_A, "alice") };
        let add_music = Action::AddTo { copy: 1, to: main_lib(MUSIC) };
        let replace = Action::Replace { copy: 1, into: main_lib(MUSIC) };
        let delete = Action::DeleteArrival { copy: 1 };
        let ask = Decision::AskFirst(Plan::Delete { copy: 1 });
        // Add to Your music (OWN-A, her default).
        assert_eq!(decide(&alice(), &add_own, &two_rooms(&alice())), Decision::Allow);
        for v in [bob(), carol()] {
            assert!(refused_right(decide(&v, &add_own, &two_rooms(&v))), "{}", v.account);
        }
        assert!(!can(&key(), &add_own, &two_rooms(&key())));
        // Add to MUSIC; Replace in MUSIC (better): administrators.
        for action in [&add_music, &replace] {
            assert_eq!(decide(&root(), action, &two_rooms(&root())), Decision::Allow, "{action:?}");
            for v in [alice(), bob(), carol()] {
                assert!(refused_right(decide(&v, action, &two_rooms(&v))), "{action:?} for {}", v.account);
            }
            assert!(!can(&key(), action, &two_rooms(&key())));
        }
        // Delete: administrators or its asker, a yes first.
        assert_eq!(decide(&root(), &delete, &two_rooms(&root())), ask);
        assert_eq!(decide(&alice(), &delete, &two_rooms(&alice())), ask);
        for v in [bob(), carol()] {
            assert!(refused_right(decide(&v, &delete, &two_rooms(&v))), "{}", v.account);
        }
        // Undo, after root's Add to MUSIC: whoever did it.
        let kept = |v: &Viewer| {
            let mut r = two_rooms(v);
            r.copies[1] = RecordCopy {
                arrival: Some(ArrivalFacts { state: Arrival::Kept { into: MUSIC }, asked_by: vec!["alice".to_string()], by: Some("root".to_string()) }),
                ..copy(main_lib(MUSIC), 300)
            };
            r
        };
        let undo = Action::Undo { copy: 1 };
        assert_eq!(decide(&root(), &undo, &kept(&root())), Decision::Allow);
        for v in [alice(), bob(), carol()] {
            assert!(refused_right(decide(&v, &undo, &kept(&v))), "{}", v.account);
        }
        assert!(!can(&key(), &undo, &kept(&key())));
        // A kept arrival is no longer waiting: a second Keep is refused by its state (AR3).
        assert!(refused_state(decide(&root(), &Action::AddTo { copy: 1, to: main_lib(MORE) }, &kept(&root()))));
    }

    /// CN8. Red first: five ways decide a role today (`administering()`, `admin_or_key`, the stored flag, `rights()`, the
    /// page's `me.admin`).
    #[test]
    fn the_role_is_decided_once() {
        assert_eq!(root().role, Role::Administrator);
        for scope in [Scope::Full, Scope::Control, Scope::Read] {
            let v = Viewer::new("root", true, Some(scope), May::default(), None);
            assert_eq!((v.role, v.scope), (Role::AdministratorsKey, scope), "an administrator's key");
        }
        assert_eq!(alice().role, Role::User);
        assert_eq!(Viewer::new("alice", false, Some(Scope::Full), ALL, Some(OWN_A)).role, Role::User, "a user's key");
        // A record right reads the role and the key's scope: an administrator's full key edits, a reading key doesn't.
        let had = record(vec![copy(main_lib(MUSIC), 10)], Some(0), &root());
        let full_key = Viewer::new("root", true, Some(Scope::Full), May::default(), None);
        assert_eq!(decide(&full_key, &Action::Edit, &had), Decision::Allow);
        assert_eq!(decide(&key(), &Action::Edit, &had), Decision::Deny(Why::Right(said::KEY)));
        // Never a record right reading the admin flag itself: `decide`'s module reads `role` alone.
        let source = include_str!("can.rs").replace('\r', "");
        let rules = source.split("mod tests {").next().unwrap();
        assert!(!rules.contains(".admin "), "a right reads the role, never the flag");
        assert!(!rules.contains(".admin)") && !rules.contains(".admin,") && !rules.contains(".admin;"));
    }

    /// CN4's `decide` half (its test, `the_play_row_is_offered_where_a_copy_plays_or_a_preview`, is the records crate's: it
    /// reads `copy_for`, which gives `plays` here).
    #[test]
    fn the_play_row_follows_the_copy_that_plays_or_a_preview() {
        let night_ferry = |plays: Option<usize>| record(vec![copy(main_lib(MUSIC), 10), arrival(incoming_lib(IN), &[])], plays, &alice());
        assert_eq!(decide(&alice(), &Action::Play, &night_ferry(Some(0))), Decision::Allow, "every library on: MUSIC's plays");
        // MUSIC off: no copy plays (an Incoming copy does not play by itself); Play from IN in the Library section.
        assert!(refused_state(decide(&alice(), &Action::Play, &night_ferry(None))));
        assert_eq!(decide(&alice(), &Action::PlayFrom { copy: 1 }, &night_ferry(None)), Decision::Allow);
        // Not had, with a preview; without.
        let preview = Record { preview: true, ..not_had() };
        assert_eq!(decide(&alice(), &Action::Play, &preview), Decision::Allow);
        assert!(refused_state(decide(&alice(), &Action::Play, &not_had())));
        assert!(refused_state(decide(&alice(), &Action::Shuffle, &preview)), "the preview's Play row is Play alone");
        // The reading key: the Play row.
        for action in [Action::Play, Action::Shuffle, Action::PlayNext, Action::AddToQueue, Action::StartRadio] {
            assert_eq!(decide(&key(), &action, &night_ferry(Some(0))), Decision::Allow, "{action:?}");
        }
    }

    /// CN6's `decide` half (its test, `library_actions_name_their_copy_and_follow_the_server`, is the records crate's: the
    /// view lists them). Night Ferry in MUSIC (MP3) and OWN-A (FLAC, better); MORE, IN and IN2 exist.
    #[test]
    fn each_library_action_names_its_copy_and_decide_holds_it() {
        let night_ferry = |v: &Viewer| record(vec![copy(main_lib(MUSIC), 10), copy(own_lib(OWN_A, "alice"), 20)], Some(0), v);
        let (r, a) = (night_ferry(&root()), night_ferry(&alice()));
        // root.
        assert_eq!(decide(&root(), &Action::MoveTo { copy: 0, to: main_lib(MORE) }, &r), Decision::Allow, "Move to MORE");
        assert_eq!(decide(&root(), &Action::SaveTo { copy: 0, to: own_lib(ROOTS, "root") }, &r), Decision::Allow, "Save to Your music");
        assert_eq!(decide(&root(), &Action::SaveTo { copy: 0, to: own_lib(OWN_B, "bob") }, &r), Decision::Allow, "Save to bob's music");
        assert_eq!(decide(&root(), &Action::ReplaceIn { into: main_lib(MUSIC) }, &r), Decision::Allow, "Replace in MUSIC");
        assert_eq!(decide(&root(), &Action::Delete { copy: 0 }, &r), Decision::AskFirst(Plan::Delete { copy: 0 }), "MUSIC's copy");
        assert!(refused_state(decide(&root(), &Action::MoveTo { copy: 0, to: incoming_lib(IN) }, &r)), "no Move to IN");
        assert!(refused_state(decide(&root(), &Action::MoveTo { copy: 0, to: incoming_lib(IN2) }, &r)), "no Move to IN2");
        assert!(refused_right(decide(&root(), &Action::Delete { copy: 1 }, &r)), "no Delete of OWN-A's copy");
        // Never: root denied Save to MUSIC from OWN-A because the copy is not his to delete (A.4's 7th).
        assert_eq!(decide(&root(), &Action::SaveTo { copy: 1, to: main_lib(MUSIC) }, &r), Decision::Allow);
        // alice.
        assert_eq!(decide(&alice(), &Action::Delete { copy: 1 }, &a), Decision::AskFirst(Plan::Delete { copy: 1 }), "OWN-A's copy");
        assert!(refused_right(decide(&alice(), &Action::MoveTo { copy: 0, to: main_lib(MORE) }, &a)), "no Move");
        assert!(refused_right(decide(&alice(), &Action::SaveTo { copy: 1, to: main_lib(MUSIC) }, &a)), "no Save to MUSIC");
        // A copy that isn't there is a state, never a panic.
        assert!(refused_state(decide(&root(), &Action::Delete { copy: 9 }, &r)));
    }
}
