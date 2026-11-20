//! What a Want's copy of a record the library holds came to, and the album actions' rules the requests read (the copy a Want
//! copies, who asked for an album, who may add it and where it goes), below the requests that read them (moved from
//! `albums`, which keeps their names).

use std::collections::HashMap;

use super::library::Active;
use crate::serve::copies::{Copy, copy_for};
use crate::settings::LibraryKind;
use crate::source;

/// What a Want's copy of a record the library holds came to (T5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WantCopy {
    /// Made in the asker's own music.
    Made,
    /// The asker has no own music, or a copy of it is in theirs already: nothing to say.
    Nothing,
    /// No copy of it is known whole (S.1): nothing copied; the Want goes to the fetchers.
    NoneWhole,
    /// Below the floor (AL03.1), asked for a copy that shares its blocks alone, it would not: nothing copied; the Want waits for space.
    NoSpace,
    /// Not made, and why.
    Failed(String),
}

/// What a Want of a record the library holds copies into its asker's own music (T5): nothing where the asker holds a copy of it in
/// their own music already; none where no copy is known whole; else the copy, by its catalogue folder and place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chosen {
    Theirs,
    NoneWhole,
    From(i64, String),
}

/// The copy a Want copies (T5; the owner's "Yes, from anyone's"): of a record's copies, each judged for copying (S.1), those not
/// short, in a kept library, not switched off (`off`, catalogue folders): the one that plays for most (`copy_for` for no one: a
/// main library's, then, where only own music holds it, anyone's first added; a tie by the libraries' order and place). `asker`:
/// the asker's own music's folder, where it is in use. Pure.
pub fn want_copy(copies: &[(Copy, crate::catalogue::identity::Lacks)], asker: Option<i64>, off: &[i64]) -> Chosen {
    if asker.is_some_and(|a| copies.iter().any(|(c, _)| c.folder == a)) {
        return Chosen::Theirs;
    }
    let whole: Vec<Copy> = copies.iter().filter(|(c, l)| c.kind != LibraryKind::Incoming && !off.contains(&c.folder) && l.copies()).map(|(c, _)| c.clone()).collect();
    copy_for(&whole, None, &[]).map_or(Chosen::NoneWhole, |c| Chosen::From(c.folder, c.dir.clone()))
}

/// Who asked for the record an album is (its id names it, C49b), where this copy of it is theirs: it came after they asked (R32,
/// N14). An Incoming copy that was there before, an upgrade another fetched, is no requester's.
pub fn requester_of<'a>(asked: &'a HashMap<i64, crate::serve::requests_kept::Requester>, al: &source::Album) -> Option<&'a String> {
    crate::serve::ids::record_number(&al.id).and_then(|record| asked.get(&record)).filter(|r| r.of_copy(al.added)).map(|r| &r.user)
}

/// Whether the viewer may add an Incoming album (0101 point 2): an administrator any; a user their own request's, and only while
/// their own music is on (X5): a user's album goes nowhere else. Pure.
pub fn may_add(a: &Active, admin: bool, viewer: Option<&str>, requester: Option<&str>) -> bool {
    admin || viewer.is_some_and(|v| requester == Some(v) && a.cfg.own_music_on(v))
}

/// Where an Incoming album goes by default (0101 point 2), the page's and Home Assistant's promote's alike (X5): its requester's own
/// music, else its library's setting, else the first main library. `requester`: the first to ask for it before this copy came
/// (`requests::requesters`), an account still on. Pure.
pub fn default_target(a: &Active, library: Option<&str>, requester: Option<&str>) -> Option<String> {
    let set = library.and_then(|l| a.cfg.libraries.get(l)).and_then(|l| l.keep_to.clone());
    requester.and_then(|r| a.own_of(r)).or(set).or_else(|| a.main().into_iter().next())
}
