//! The Library page's one list (decision 0034): every album of the followed server's used libraries, each with its
//! library, and what holds against the other libraries. Made of Collection's lists (Music) and Incoming's, which keep
//! and refresh themselves; this only puts them together.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use super::incoming::VerdictView;
use super::{App, JobView, KeepPreview, KeepResult, ToggleError};
use super::library::{Active, epoch_now};
use crate::serve::copies::{Copy, Viewer, against, copy_for, in_order, replaces, why};
use crate::settings::{LibraryKind, Quality, YOUR_MUSIC};
use crate::source;

/// Whether the viewer may delete from a kept library from a menu. An administrator deletes in a main library; in someone
/// else's own music only from the Users panel, never a menu (the owner, 2026-11-04); own music is its owner's.
fn may_delete(kind: Option<LibraryKind>, mine: bool, admin: bool) -> bool {
    match kind {
        Some(LibraryKind::Main) => admin,
        Some(LibraryKind::Own) => mine,
        _ => false,
    }
}

/// Why a record may not move, if so. Out of Incoming it is added: to where it goes by default, or by an administrator to
/// a main library or their own music. Between other libraries only an administrator moves one; into or out of own music it
/// is a copy (the owner, 2026-11-04; C40: `copies`), and the music of someone who left (`left`) moves into a main library, as
/// the account's page asks (0101 point 7).
fn move_refused(from: Option<LibraryKind>, to: Option<LibraryKind>, admin: bool, keepable: bool, default: bool, viewers_own: bool, left: bool) -> Option<&'static str> {
    if from == Some(LibraryKind::Incoming) {
        if !keepable {
            return Some("That isn't yours to add");
        }
        return (!default && !(admin && (to == Some(LibraryKind::Main) || viewers_own))).then_some("It can't be added to that library");
    }
    if !admin {
        return Some("Moving between libraries is the administrators'");
    }
    let _ = (left, viewers_own);
    None
}

/// Whether a move between these libraries is a copy (C40, the owner 2026-11-04): into or out of someone's own music the album
/// stays where it is, and a copy of it goes. Out of Incoming it is Keep, a move; between main libraries a move; and the music of
/// someone who left moves into a main library (0101 point 7). Pure.
fn copies(from: Option<LibraryKind>, to: Option<LibraryKind>, viewers_own: bool, left: bool) -> bool {
    from != Some(LibraryKind::Incoming) && !(left && to == Some(LibraryKind::Main)) && (from == Some(LibraryKind::Own) || to == Some(LibraryKind::Own) || viewers_own)
}

/// A used library of a server, as the page shows it (0101 point 1: no roles; the list's order is the default, the
/// first main library taking every fallback).
#[derive(Debug, Clone, Serialize)]
pub struct LibraryView {
    pub id: String,
    /// As its server calls it, or its own name.
    pub name: String,
    /// "main", "incoming" or "own" (someone's own music, 0101 point 2).
    pub kind: &'static str,
    /// An Incoming library's fetcher: "lidarr", "soulseek" or "uploads".
    pub fetcher: Option<String>,
    /// Own music's account; none for another kind, or once its account was removed (an ownerless library).
    pub owner: Option<String>,
    /// The viewer's own music.
    pub mine: bool,
    /// The dot on its albums: every library has one (the page shows it while the viewer has two or more on).
    pub colour: Option<String>,
    /// Its quality limit on the way in (decision 0096's amendment).
    pub quality: Quality,
    /// Whether Cratarr sees its files, and why not.
    pub seen: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seen_why: Option<String>,
    /// A main library's naming template (Keep and Move rename by the target's); none: the Music naming.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub naming: Option<String>,
}

/// An album that has an album of another library matched to it (the inverse of an Incoming album's verdict).
#[derive(Debug, Clone, Serialize)]
pub struct Waiting {
    /// "upgrade" or "duplicate": what the other album is to this one.
    pub kind: &'static str,
    /// The other album's id.
    pub album: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlbumsSourceView {
    pub id: String,
    pub label: String,
    /// False when the server cannot list albums at all.
    pub supported: bool,
    pub albums: Vec<source::Album>,
    pub error: Option<String>,
    /// When the oldest of its lists was read from the server, epoch seconds.
    pub as_of: i64,
    /// The library ids listed (the filter's library switches).
    pub libraries: Vec<String>,
    /// Those libraries, with their names, uses, colours and formats.
    pub library_info: Vec<LibraryView>,
    /// Per album id of a library other than Music: the album of another used library it upgrades or duplicates
    /// (decision 0034), Music's first.
    pub verdicts: BTreeMap<String, VerdictView>,
    /// Per album id of a library other than Music: what it lacks of what the library requires (the owner,
    /// 2026-10-24: marked before Keep, as Health marks Music's).
    pub lacks: BTreeMap<String, Vec<&'static str>>,
    /// Per album id a verdict pairs with (Music's, or another library's): an album of another library that upgrades
    /// or duplicates it.
    pub waiting: BTreeMap<String, Waiting>,
    /// The albums Cratarr can delete.
    pub deletable: Vec<String>,
    /// The Incoming albums the viewer may Keep (0101 point 2: an administrator every one; a user their own requests,
    /// once own music is in).
    pub keepable: Vec<String>,
    /// Per Incoming album, where Keep puts it by default (0101 point 2: the requester's own music, else the first main
    /// library).
    pub keep_to: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct AlbumsView {
    pub sources: Vec<AlbumsSourceView>,
    /// How long a deleted album stays in the trash.
    pub trash_days: u32,
    /// The Music library's name, as its server calls it.
    pub music: Option<String>,
}

/// Someone's own music before its first use, as the list shows it: there to add to, its folder made then.
fn own_to_be(a: &Active, id: String, name: String, owner: String, mine: bool) -> LibraryView {
    let used = a.libraries();
    let colour = crate::settings::colour_among(used.iter().map(|u| (u.id.as_str(), u.kind)).chain([(id.as_str(), LibraryKind::Own)]), &id, None);
    LibraryView { id, name, kind: "own", fetcher: None, owner: Some(owner), mine, colour, quality: LibraryKind::Own.quality(), seen: true, seen_why: None, naming: None }
}

impl App {
    /// Whether `account` is one here, not removed (0101 point 7: its own music stays, ownerless and no target).
    fn account_on(&self, account: &str) -> bool {
        self.served.as_ref().is_none_or(|s| s.users.get(account).is_some_and(|u| u.active))
    }

    /// Every server with used libraries (or only `want`): all their albums in one list, with their facts.
    pub async fn albums(&self, want: Option<&str>) -> AlbumsView {
        let (collection, incoming) = tokio::join!(self.collection.list(want), self.incoming.list(want));
        let mut ids: Vec<String> = collection.sources.iter().map(|s| s.id.clone()).collect();
        for s in &incoming.sources {
            if !ids.contains(&s.id) {
                ids.push(s.id.clone());
            }
        }
        let live = self.library.connected();
        let kept_deletable = self.incoming.kept_deletable().await;
        let jobs = self.incoming.jobs();
        // Who is looking (0101 point 2): the names, the rights and where Add to goes are theirs.
        let viewer = self.served.as_ref().and_then(|s| s.current_user());
        let admin = crate::serve::access::administering();
        // Read beside the async workers (R16, F5): one catalogue reader, never a build's lock.
        let asked = match self.served.clone() {
            Some(s) => tokio::task::spawn_blocking(move || super::requests::requesters(&s)).await.unwrap_or_default(),
            None => Default::default(),
        };
        let requester = |al: &source::Album| requester_of(&asked, al);
        let mut sources: Vec<AlbumsSourceView> = ids
            .into_iter()
            .map(|id| {
                let c = collection.sources.iter().find(|s| s.id == id);
                let i = incoming.sources.iter().find(|s| s.id == id);
                let active = live.iter().find(|(a, _)| a.cfg.id == id).map(|(a, _)| a.clone());
                let libraries: Vec<String> = c.iter().flat_map(|c| c.libraries.clone()).chain(i.iter().flat_map(|i| i.libraries.clone())).collect();
                let mut library_info: Vec<LibraryView> = active
                    .as_ref()
                    .map(|a| {
                        let used = a.libraries();
                        let mut info: Vec<LibraryView> = libraries
                            .iter()
                            .map(|l| {
                                let placed = a.placement.libraries.iter().find(|p| &p.id == l);
                                let seen = placed.is_some_and(|p| !p.folders.is_empty());
                                let owner = used.iter().find(|u| &u.id == l).and_then(|u| u.owner.clone()).filter(|o| self.account_on(o));
                                let mine = owner.is_some() && owner == viewer;
                                LibraryView {
                                    id: l.clone(),
                                    name: if mine { YOUR_MUSIC.to_string() } else { placed.map_or_else(|| l.clone(), |p| p.name.clone()) },
                                    kind: a.kind_of(l).unwrap_or(LibraryKind::Incoming).as_str(),
                                    fetcher: a.cfg.libraries.get(l).and_then(|s| s.fetcher.clone()),
                                    owner,
                                    mine,
                                    colour: a.colour_of(l),
                                    quality: a.quality_of(l),
                                    seen,
                                    seen_why: (!seen).then(|| "Cratarr can't see its files".to_string()),
                                    naming: None,
                                }
                            })
                            .collect();
                        // The viewer's own music before its first use: there to add to, its folder made then.
                        if let Some((v, id)) = viewer.as_ref().and_then(|v| Some((v, a.own_of(v)?))).filter(|(_, id)| !info.iter().any(|l| &l.id == id)) {
                            info.push(own_to_be(a, id, YOUR_MUSIC.to_string(), v.clone(), true));
                        }
                        info
                    })
                    .unwrap_or_default();
                let music_albums: &[source::Album] = c.map_or(&[], |c| c.albums.as_slice());
                // The Incoming albums the viewer may add (an administrator every one, a user their own requests, into their
                // own music), and where each goes by default: its requester's own music, else its library's setting, else
                // the first main library.
                let incoming_albums: &[source::Album] = i.map_or(&[], |i| i.albums.as_slice());
                let theirs = |al: &source::Album| viewer.is_some() && requester(al) == viewer.as_ref();
                // A user's own request's only while their own music is on (X5): theirs goes nowhere else.
                let may_keep = |al: &source::Album| active.as_ref().map_or(admin, |a| may_add(a, admin, viewer.as_deref(), requester(al).map(String::as_str)));
                let keepable: Vec<String> = incoming_albums.iter().filter(|al| may_keep(al)).map(|al| al.id.clone()).collect();
                let keeps_to = |al: &source::Album| default_target(active.as_ref()?, al.library.as_deref(), requester(al).filter(|r| self.account_on(r)).map(String::as_str));
                let keep_to: BTreeMap<String, String> = incoming_albums.iter().filter(|al| may_keep(al)).filter_map(|al| Some((al.id.clone(), keeps_to(al)?))).collect();
                // A requester's own music before its first use: there to add to.
                if let Some(a) = active.as_ref() {
                    for to in keep_to.values() {
                        if !library_info.iter().any(|l| &l.id == to) && !a.used(to) {
                            let account = std::path::Path::new(to).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                            library_info.push(own_to_be(a, to.clone(), format!("{account}'s music"), account, false));
                        }
                    }
                }
                let may_delete_in = |l: &str| {
                    let Some(a) = active.as_ref() else { return false };
                    let mine = viewer.is_some() && a.libraries().iter().any(|u| u.id == l && u.owner == viewer);
                    may_delete(a.kind_of(l), mine, admin)
                };
                let kept_deletable_here = |a: &source::Album| a.path.is_some() && a.library.as_ref().is_some_and(|l| kept_deletable.iter().any(|(s, m)| s == &id && m == l) && may_delete_in(l));
                let incoming_deletable = |a: &source::Album| i.is_some_and(|i| i.deletable.contains(&a.id)) && (admin || theirs(a));
                let verdicts = i.map(|i| i.verdicts.clone()).unwrap_or_default();
                // Each listed copy says what the viewer may do with it, as the album's lists below say it by id. Those lists
                // cannot tell a record's copies apart, so the two are worked out here, before the copies are folded.
                let told = |a: &source::Album, deletable: bool, incoming: bool| {
                    let mut a = a.clone();
                    let (format, kbps) = (a.format.clone(), a.bitrate_kbps);
                    let job = job_on(&jobs, &id, &a);
                    let lacks: Vec<String> = i.filter(|_| incoming).and_then(|i| i.lacks.get(&a.id)).map(|l| l.iter().map(|w| w.to_string()).collect()).unwrap_or_default();
                    let keepable = incoming && may_keep(&a);
                    let keep_to = if keepable { keeps_to(&a) } else { None };
                    for c in &mut a.copies {
                        (c.format, c.kbps, c.deletable, c.keepable, c.keep_to, c.lacks, c.job) = (format.clone(), kbps, deletable, keepable, keep_to.clone(), lacks.clone(), job.map(|j| j.id));
                        c.plays = true;
                    }
                    a
                };
                let albums: Vec<source::Album> = music_albums
                    .iter()
                    .map(|a| told(a, kept_deletable_here(a), false))
                    .chain(incoming_albums.iter().map(|a| told(a, incoming_deletable(a), true)))
                    .collect();
                AlbumsSourceView {
                    label: c.map(|c| c.label.clone()).or_else(|| i.map(|i| i.label.clone())).unwrap_or_else(|| id.clone()),
                    supported: c.is_some_and(|c| c.supported) || i.is_some_and(|i| i.supported),
                    waiting: waiting(&verdicts, &albums),
                    albums,
                    error: c.and_then(|c| c.error.clone()).or_else(|| i.and_then(|i| i.error.clone())),
                    as_of: [c.map(|c| c.as_of), i.map(|i| i.as_of)].into_iter().flatten().min().unwrap_or_else(epoch_now),
                    verdicts,
                    lacks: i.map(|i| i.lacks.clone()).unwrap_or_default(),
                    deletable: i
                        .map(|i| i.deletable.clone())
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|d| admin || incoming_albums.iter().any(|al| &al.id == d && theirs(al)))
                        .chain(music_albums.iter().filter(|a| kept_deletable_here(a)).map(|a| a.id.clone()))
                        .collect(),
                    keepable,
                    keep_to,
                    libraries,
                    library_info,
                    id,
                }
            })
            .collect();
        // A record is listed once (a keyed list on the page throws on a repeat), with every copy in it. Cratarr's own alone:
        // another server's albums are entries of their libraries, and their switches go by that.
        for s in sources.iter_mut().filter(|s| s.id == crate::source::cratarr::KIND.id) {
            let orders = self.play_orders(&s.albums, viewer.as_deref()).await;
            s.albums = fold(std::mem::take(&mut s.albums), &orders);
        }
        // Classical (0.10): Cratarr's own albums say whether they are, their composers and performers. Another server's
        // are typed by the owner's rule (0.13: a type is always known).
        for s in &mut sources {
            super::classical::tell(&s.id, &mut s.albums).await;
            super::file_facts::tell(&s.id, &mut s.albums);
            if s.id != crate::source::cratarr::KIND.id {
                s.albums.iter_mut().for_each(crate::serve::library::type_by_rule);
            }
        }
        AlbumsView { sources, trash_days: incoming.trash_days, music: incoming.music }
    }
}

/// The copy of a listed record that an action names, as its library alone would list it. `from` is a library: of two folders
/// of one library the first, as the record's copies stand (Q2). A record of several copies must be told which: what is
/// done to its files cannot be left to which of them plays. Another server's album is its one library's.
fn named_copy(a: &source::Album, from: Option<&str>) -> Result<source::Album, ToggleError> {
    let elsewhere = || ToggleError::Refused("It has no copy in that library".into());
    if a.copies.is_empty() {
        return match from {
            Some(f) if a.library.as_deref() != Some(f) => Err(elsewhere()),
            _ => Ok(a.clone()),
        };
    }
    match from {
        Some(f) => a.copies.iter().find(|c| c.library == f).map(|c| a.as_copy(c)).ok_or_else(elsewhere),
        None => match a.copies.as_slice() {
            [only] => Ok(a.as_copy(only)),
            _ => Err(ToggleError::Refused("It is in several libraries: say which copy".into())),
        },
    }
}

/// The conversion at work on this library's copy of a record: a record's other copies have nothing to do with it, and two
/// folders of one library are told apart by place.
fn job_on<'a>(jobs: &'a [JobView], source: &str, a: &source::Album) -> Option<&'a JobView> {
    let here = |j: &JobView| match a.copies.first() {
        Some(c) => j.place.0 == c.folder && j.place.1 == c.dir,
        None => j.from == a.library,
    };
    jobs.iter().find(|j| j.source == source && j.album_id == a.id && j.at_work() && here(j))
}

/// Who asked for the record an album is (its id names it, C49b), where this copy of it is theirs: it came after they asked (R32,
/// N14). An Incoming copy that was there before, an upgrade another fetched, is no requester's.
pub(super) fn requester_of<'a>(asked: &'a HashMap<i64, super::requests::Requester>, al: &source::Album) -> Option<&'a String> {
    crate::serve::ids::record_number(&al.id).and_then(|record| asked.get(&record)).filter(|r| r.of_copy(al.added)).map(|r| &r.user)
}

/// Whether the viewer may add an Incoming album (0101 point 2): an administrator any; a user their own request's, and only while
/// their own music is on (X5): a user's album goes nowhere else. Pure.
fn may_add(a: &Active, admin: bool, viewer: Option<&str>, requester: Option<&str>) -> bool {
    admin || viewer.is_some_and(|v| requester == Some(v) && a.cfg.own_music_on(v))
}

/// Where an Incoming album goes by default (0101 point 2), the page's and Home Assistant's promote's alike (X5): its requester's own
/// music, else its library's setting, else the first main library. `requester`: the first to ask for it before this copy came
/// (`requests::requesters`), an account still on. Pure.
pub(super) fn default_target(a: &Active, library: Option<&str>, requester: Option<&str>) -> Option<String> {
    let set = library.and_then(|l| a.cfg.libraries.get(l)).and_then(|l| l.keep_to.clone());
    requester.and_then(|r| a.own_of(r)).or(set).or_else(|| a.main().into_iter().next())
}

/// Whether the viewer may delete this copy: the view's `deletable` goes by id, so it holds while any copy of the record may
/// go, and a requester's right to their Incoming copy must not reach the kept one beside it.
fn may_delete_copy(deletable: &[String], a: &source::Album) -> bool {
    a.copies.first().map_or_else(|| deletable.contains(&a.id), |c| c.deletable)
}

/// How a record's copies stand for one viewer: what `fold` needs of the library, so `fold` stays pure.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PlayOrder {
    /// Each copy's (catalogue folder, folder in it), in the order they would play, whatever is off.
    places: Vec<(i64, String)>,
    /// The copy that plays them: as their libraries are switched, else as if the one it would be were on.
    plays: Option<(i64, String)>,
    /// Each Incoming copy of a record kept elsewhere, as it stands against the copy that plays the record for most (C37a).
    against: Vec<Stand>,
}

/// An Incoming copy's stand against the copy that plays its record for most (C37a, `copies::against`): its place, whether it is
/// better and Replace's to take, that copy's place, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stand {
    at: (i64, String),
    better: bool,
    replaces: bool,
    kept: (i64, String),
    why: String,
}

impl PlayOrder {
    /// `copy_for` alone decides, here as at a stream. With nothing on for them a copy is still named, as an Incoming record's
    /// switch thrown on the page must find its copy says it plays.
    fn of(copies: &[Copy], viewer: Option<&Viewer>, off: &[i64]) -> Self {
        let place = |c: &Copy| (c.folder, c.dir.clone());
        Self {
            places: in_order(copies, viewer).iter().map(place).collect(),
            plays: copy_for(copies, viewer, off).or_else(|| copy_for(copies, viewer, &[])).map(place),
            against: copies
                .iter()
                .filter_map(|c| {
                    let (better, kept) = against(copies, c)?;
                    Some(Stand { at: place(c), better, replaces: replaces(better, kept.kind), kept: place(kept), why: why(c, kept) })
                })
                .collect(),
        }
    }
}

/// One entry per record, from a list of each library's copies. A record listed once is as it was; of one listed from
/// several, the entry of the copy that plays stands, so the card's format and bitrate are that copy's, with every copy in it
/// in `orders`' order. The order the records came in stays.
fn fold(listed: Vec<source::Album>, orders: &HashMap<String, PlayOrder>) -> Vec<source::Album> {
    let mut records: Vec<Vec<source::Album>> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for a in listed {
        match seen.get(&a.id) {
            Some(i) => records[*i].push(a),
            None => {
                seen.insert(a.id.clone(), records.len());
                records.push(vec![a]);
            }
        }
    }
    records
        .into_iter()
        .filter_map(|mut entries| {
            if entries.len() == 1 {
                return entries.pop();
            }
            let order = entries.first().and_then(|a| orders.get(&a.id)).cloned().unwrap_or_default();
            let place = |a: &source::Album| a.copies.first().map(|c| (c.folder, c.dir.clone()));
            let rank = |a: &source::Album| place(a).and_then(|p| order.places.iter().position(|o| *o == p)).unwrap_or(usize::MAX);
            entries.sort_by_key(rank);
            let lead = entries.iter().position(|a| place(a).is_some() && place(a) == order.plays).unwrap_or(0);
            let mut copies: Vec<source::AlbumCopy> = entries
                .iter()
                .enumerate()
                .flat_map(|(i, a)| a.copies.iter().cloned().map(move |c| source::AlbumCopy { plays: i == lead, ..c }))
                .collect();
            // An Incoming copy's stand, its kept copy named by its library as listed.
            let libraries: Vec<((i64, String), String)> = copies.iter().map(|c| ((c.folder, c.dir.clone()), c.library.clone())).collect();
            for c in &mut copies {
                let Some(s) = order.against.iter().find(|s| s.at == (c.folder, c.dir.clone())) else { continue };
                let library = libraries.iter().find(|(at, _)| *at == s.kept).map(|(_, l)| l.clone());
                c.against = library.map(|library| source::Against { better: s.better, replaces: s.replaces, library, dir: s.kept.1.clone(), why: s.why.clone() });
            }
            let mut record = entries.swap_remove(lead);
            record.copies = copies;
            Some(record)
        })
        .collect()
}

impl App {
    /// The records listed more than once, each with how its listed copies stand for `viewer` (`PlayOrder`).
    async fn play_orders(&self, albums: &[source::Album], viewer: Option<&str>) -> HashMap<String, PlayOrder> {
        let Some(served) = self.served.clone() else { return HashMap::new() };
        let mut listed: HashMap<String, Vec<(i64, String)>> = HashMap::new();
        for a in albums {
            listed.entry(a.id.clone()).or_default().extend(a.copies.iter().map(|c| (c.folder, c.dir.clone())));
        }
        listed.retain(|_, places| places.len() > 1);
        if listed.is_empty() {
            return HashMap::new();
        }
        let who = served.listener(viewer);
        let read = tokio::task::spawn_blocking(move || -> anyhow::Result<HashMap<String, PlayOrder>> {
            let mut out = HashMap::new();
            for (id, places) in listed {
                // Only what is listed can stand for the record: a copy read since the list was has no entry here yet.
                let copies: Vec<Copy> = served.library.copies_of(&id)?.into_iter().filter(|c| places.iter().any(|(f, d)| *f == c.folder && *d == c.dir)).collect();
                let order = PlayOrder::of(&copies, who.viewer().as_ref(), &who.off);
                out.insert(id, order);
            }
            Ok(out)
        });
        match read.await {
            Ok(Ok(orders)) => orders,
            Ok(Err(e)) => {
                tracing::warn!(target: "library", reason = %format!("{e:#}"), "which copy plays not read: copies as listed");
                HashMap::new()
            }
            Err(e) => {
                tracing::warn!(target: "library", reason = %e, "which copy plays not read: copies as listed");
                HashMap::new()
            }
        }
    }

    /// A listed album of a server (Music's or another library's), by id, as the viewer sees that server: the copy `from`
    /// names (`named_copy`).
    async fn listed_album(&self, source: &str, album: &str, from: Option<&str>) -> Result<(AlbumsSourceView, source::Album), ToggleError> {
        let view = self.albums(Some(source)).await;
        let s = view.sources.into_iter().find(|s| s.id == source).ok_or_else(|| ToggleError::Refused("that server is not connected".into()))?;
        let a = s.albums.iter().find(|a| a.id == album).ok_or_else(|| ToggleError::Refused("that album is no longer listed".into()))?;
        let a = named_copy(a, from)?;
        Ok((s, a))
    }

    /// Delete a copy the viewer may, into its library's trash (decision 0034). The record stays while another copy does.
    pub async fn delete_album(&self, source: &str, album: &str, from: Option<&str>) -> Result<super::DeleteResult, ToggleError> {
        let (s, a) = self.listed_album(source, album, from).await?;
        if !may_delete_copy(&s.deletable, &a) {
            return Err(ToggleError::Refused("That isn't yours to delete".into()));
        }
        // A copy in Incoming that arrived for requests (W4, the owner's answer (c)): read before it goes, they close as rejected once
        // it has.
        let live = self.library.live();
        let incoming = a.library.as_deref().and_then(|l| live.iter().find(|x| x.cfg.id == source)?.kind_of(l)) == Some(LibraryKind::Incoming);
        // Whose it was (U5): an Incoming copy outside Soulseek's library is Lidarr's, whose grab is marked failed where another asker
        // is left.
        let lidarrs = incoming && a.library.as_deref().and_then(|l| live.iter().find(|x| x.cfg.id == source)?.cfg.libraries.get(l)?.fetcher.clone()).as_deref() != Some("soulseek");
        // Every open request of its record too (V2): one the Delete leaves keeps a fetcher.
        let (arrived, open) = match (self.served.as_ref(), crate::serve::ids::record_number(&a.id)) {
            (Some(served), Some(record)) if incoming => (super::requests::arrived_for(served, record, a.added).await, super::requests::open_of(served, record).await),
            _ => (Vec::new(), Vec::new()),
        };
        // Soulseek's Placed items of its record are forgotten by the Delete itself (V1, U3: `delete_listed`).
        let done = self.incoming.delete_listed(source, a).await?;
        if let Some(served) = self.served.as_ref() {
            // An administrator's Delete closes every asker's request, a requester's their own alone (Y4); what it closed is kept on
            // its journal entry, for its Undo. An open request of the record it leaves has Lidarr go on, and what Incoming's handler
            // stopped for the copy put back (V2).
            let closing = super::requests::closed_by(arrived, crate::serve::access::administering(), served.current_user().as_deref());
            let id = done.undo.parse::<crate::journal::EventId>().ok();
            let soulseek = super::soulseek::connected().await;
            let goes_on = super::requests::deleted(super::requests::fetcher(self).as_deref(), soulseek, served, &closing, open, lidarrs, id.map(|id| (self.journal.as_ref(), id))).await;
            if goes_on && let Some(id) = id {
                self.incoming.keep_fetching(id, &done.title).await;
            }
        }
        Ok(done)
    }

    /// `account`'s copy of a record the library has, in their own music (C52, OQ3: "Add to Your music" through Want), made as Copy to
    /// makes one (C40): from the copy that plays the record for most (`copies::copy_for` for no one: never someone else's own music
    /// where a library holds it), named, as a record of several copies must be (C52b); into their own music, made now where it is
    /// not in use yet. None where they have no own music, or a copy of it is in theirs already, or the record is no album listed here.
    pub(super) async fn copy_for(&self, account: &str, record: i64) -> Option<Result<KeepResult, ToggleError>> {
        let (source, id) = (crate::source::cratarr::KIND.id, crate::serve::ids::record(record));
        let served = self.served.clone()?;
        let id2 = id.clone();
        let copies = tokio::task::spawn_blocking(move || served.library.copies_of(&id2)).await.ok()?.ok()?;
        let row = copy_for(&copies, None, &[]).map(|c| (c.folder, c.dir.clone()))?;
        let view = self.albums(Some(source)).await;
        let listed = view.sources.into_iter().find(|s| s.id == source)?.albums.into_iter().find(|a| a.id == id)?;
        let from = listed.copies.iter().find(|c| (c.folder, c.dir.clone()) == row).map(|c| c.library.clone())?;
        let a = named_copy(&listed, Some(&from)).ok()?;
        let own = {
            let live = self.library.live();
            live.iter().find(|x| x.cfg.id == source)?.own_of(account)?
        };
        if listed.copies.iter().any(|c| c.library == own) {
            return None;
        }
        let to = match self.library.live().iter().find(|x| x.cfg.id == source).is_some_and(|x| x.used(&own)) {
            true => own,
            false => self.library.own_library(account).await.ok()??,
        };
        Some(self.incoming.copy_album(source, a, &to, None).await)
    }

    /// Delete an album of someone's own music from the Users panel: an administrator's, only there (the owner, 2026-11-04:
    /// "not anywhere else than in a control panel").
    pub async fn delete_own_album(&self, source: &str, album: &str, from: Option<&str>) -> Result<super::DeleteResult, ToggleError> {
        let (_, a) = self.listed_album(source, album, from).await?;
        let live = self.library.live();
        let own = a.library.as_deref().and_then(|l| live.iter().find(|x| x.cfg.id == source)?.kind_of(l)) == Some(LibraryKind::Own);
        if !own {
            return Err(ToggleError::Refused("That is no one's own music".into()));
        }
        self.incoming.delete_listed(source, a).await
    }

    /// Where a move of `a` goes, if the viewer may: `to`, someone's own music made now if this is its first use.
    /// Where a move of `a` goes, if the viewer may, and whether it is a copy (`copies`).
    async fn move_target(&self, source: &str, s: &AlbumsSourceView, a: &source::Album, to: &str) -> Result<(String, bool), ToggleError> {
        let live = self.library.live();
        let on = live.iter().find(|x| x.cfg.id == source).ok_or_else(|| ToggleError::Refused("that server is not connected".into()))?;
        let viewer = self.served.as_ref().and_then(|s| s.current_user());
        let viewers_own = viewer.as_ref().and_then(|v| on.own_of(v)).as_deref() == Some(to);
        // A record's copies differ in both: only another server's album goes by its id.
        let (keepable, default) = match a.copies.first() {
            Some(c) => (c.keepable, c.keep_to.as_deref() == Some(to)),
            None => (s.keepable.contains(&a.id), s.keep_to.get(&a.id).map(String::as_str) == Some(to)),
        };
        let from = a.library.as_deref().and_then(|l| on.kind_of(l));
        let owner = on.libraries().into_iter().find(|u| a.library.as_deref() == Some(u.id.as_str())).and_then(|u| u.owner);
        let left = owner.is_some_and(|o| !self.account_on(&o));
        if let Some(why) = move_refused(from, on.kind_of(to), crate::serve::access::administering(), keepable, default, viewers_own, left) {
            return Err(ToggleError::Refused(why.into()));
        }
        // Into someone's own music not in use yet: their own library, made now; a copy, as into any own music.
        let copy = copies(from, on.kind_of(to).or_else(|| (!on.used(to)).then_some(LibraryKind::Own)), viewers_own, left);
        if on.used(to) {
            return Ok((to.to_string(), copy));
        }
        let account = std::path::Path::new(to).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let made = self.library.own_library(&account).await.map_err(ToggleError::Upstream)?;
        made.map(|m| (m, copy)).ok_or_else(|| ToggleError::Refused("no own-music folder is set".into()))
    }

    /// Move a copy into another used library of its server (decision 0034).
    pub async fn move_album(&self, source: &str, album: &str, from: Option<&str>, to: &str, year: Option<&str>) -> Result<KeepResult, ToggleError> {
        let (s, a) = self.listed_album(source, album, from).await?;
        match self.move_target(source, &s, &a, to).await? {
            (to, true) => self.incoming.copy_album(source, a, &to, year).await,
            (to, false) => self.incoming.move_album(source, a, &to, year).await,
        }
    }

    /// What that move would do.
    pub async fn move_preview(&self, source: &str, album: &str, from: Option<&str>, to: &str, year: Option<&str>) -> Result<KeepPreview, ToggleError> {
        let (s, a) = self.listed_album(source, album, from).await?;
        match self.move_target(source, &s, &a, to).await? {
            (to, true) => self.incoming.copy_preview(source, a, &to, year).await,
            (to, false) => self.incoming.move_preview(source, a, &to, year).await,
        }
    }

    /// `source`'s own-music libraries as the viewer sees them: (id, name, theirs).
    pub fn own_music_seen(&self, source: &str) -> Vec<(String, String, bool)> {
        let viewer = self.served.as_ref().and_then(|s| s.current_user());
        let live = self.library.live();
        let Some(a) = live.iter().find(|x| x.cfg.id == source) else { return Vec::new() };
        a.libraries()
            .into_iter()
            .filter(|u| u.kind == LibraryKind::Own)
            .map(|u| {
                let mine = u.owner.is_some() && u.owner == viewer;
                let name = if mine { YOUR_MUSIC.to_string() } else { a.placement.libraries.iter().find(|p| p.id == u.id).map_or_else(|| u.id.clone(), |p| p.name.clone()) };
                (u.id, name, mine)
            })
            .collect()
    }
}

/// The verdicts the other way round: per album paired (found in the verdict's library by its folder, the end of its
/// path), the album that upgrades or duplicates it; an upgrade wins over a duplicate.
fn waiting(verdicts: &BTreeMap<String, VerdictView>, albums: &[source::Album]) -> BTreeMap<String, Waiting> {
    let mut out: BTreeMap<String, Waiting> = BTreeMap::new();
    for (other, v) in verdicts {
        let dir = format!("/{}", v.library.trim_matches('/'));
        let paired = |a: &&source::Album| a.library.is_some() && a.library == v.library_id && a.path.as_deref().is_some_and(|p| p.trim_end_matches('/').ends_with(&dir));
        let Some(m) = albums.iter().filter(paired).max_by_key(|a| a.path.as_ref().map_or(0, String::len)) else {
            continue;
        };
        if out.get(&m.id).is_some_and(|w| w.kind == "upgrade") && v.kind != "upgrade" {
            continue;
        }
        out.insert(m.id.clone(), Waiting { kind: v.kind, album: other.clone() });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Own music is its owner's to delete from a menu; a main library the administrators'; an administrator deletes in
    /// someone else's own music only from the Users panel (the owner, 2026-11-04).
    #[test]
    fn who_may_delete_from_a_menu() {
        use LibraryKind::{Incoming, Main, Own};
        assert!(may_delete(Some(Main), false, true) && !may_delete(Some(Main), false, false));
        assert!(may_delete(Some(Own), true, false) && may_delete(Some(Own), true, true), "one's own");
        assert!(!may_delete(Some(Own), false, true), "another's: not even an administrator, not from a menu");
        assert!(!may_delete(Some(Incoming), false, true) && !may_delete(None, true, true), "Incoming's rights are the requests'");
    }

    #[test]
    fn who_may_add_and_move_what_where() {
        use LibraryKind::{Incoming, Main, Own};
        let inc = Some(Incoming);
        // A user: their own requests, where they go by default, and nothing else.
        assert_eq!(move_refused(inc, Some(Own), false, true, true, true, false), None);
        assert!(move_refused(inc, Some(Main), false, true, false, false, false).is_some(), "not into a main library");
        assert!(move_refused(inc, Some(Own), false, false, false, true, false).is_some(), "not another's request");
        assert!(move_refused(Some(Main), Some(Main), false, false, false, false, false).is_some());
        // An administrator: a main library, their own music, or the default (a requester's own music).
        assert_eq!(move_refused(inc, Some(Main), true, true, false, false, false), None);
        assert_eq!(move_refused(inc, None, true, true, false, true, false), None, "their own music before its first use");
        assert_eq!(move_refused(inc, None, true, true, true, false, false), None, "a requester's before its first use");
        assert!(move_refused(inc, Some(Own), true, true, false, false, false).is_some(), "not just anyone's own music");
        assert_eq!(move_refused(Some(Main), Some(Main), true, false, false, false, false), None);
        // Into or out of own music it is a copy (C40), an administrator's.
        assert_eq!(move_refused(Some(Main), Some(Own), true, false, false, false, false), None);
        assert!(move_refused(Some(Main), Some(Own), false, false, false, false, false).is_some(), "not a user's");
        assert!(copies(Some(Main), Some(Own), false, false) && copies(Some(Own), Some(Main), false, false) && copies(Some(Main), None, true, false));
        assert!(!copies(Some(Main), Some(Main), false, false) && !copies(Some(Incoming), Some(Own), true, false), "moved; added");
        // The music of someone who left moves into a main library, by an administrator.
        assert_eq!(move_refused(Some(Own), Some(Main), true, false, false, false, true), None);
        assert!(move_refused(Some(Own), Some(Main), false, false, false, false, true).is_some());
        assert!(!copies(Some(Own), Some(Main), false, true) && copies(Some(Own), Some(Own), false, true), "a leaver's into another's own music: a copy");
    }

    /// X5: with own music switched off for the requester, only an administrator adds their album; on, a user adds their own request's
    /// and no one else's. Where it goes by default, the page's and Home Assistant's promote's alike: the requester's own music, else
    /// the library's setting, else the first main library.
    #[test]
    fn with_own_music_off_only_an_administrator_adds_a_requesters_album() {
        use crate::settings::{Libraries, LibrarySetting, SourceConfig};
        let values = [(crate::settings::OWN_MUSIC.to_string(), serde_json::json!("/own")), (crate::settings::OWN_MUSIC_OFF.to_string(), serde_json::json!(["bo"]))].into_iter().collect();
        let libraries = Libraries(vec![
            LibrarySetting::new("/music", LibraryKind::Main),
            LibrarySetting { keep_to: Some("/second".into()), ..LibrarySetting::new("/in", LibraryKind::Incoming) },
            LibrarySetting::new("/second", LibraryKind::Main),
            LibrarySetting::new("/new", LibraryKind::Incoming),
        ]);
        let cfg = SourceConfig { id: "cratarr".into(), kind: crate::source::cratarr::KIND.id.into(), enabled: true, libraries, values };
        let a = Active { cfg, source: None, placement: Default::default(), error: None };
        assert!(may_add(&a, false, Some("ann"), Some("ann")), "their own request's, into their own music");
        assert!(!may_add(&a, false, Some("bo"), Some("bo")), "own music off: theirs is the administrators' to add");
        assert!(!may_add(&a, false, Some("ann"), Some("bo")) && !may_add(&a, false, Some("ann"), None), "not another's, nor one no one asked for");
        assert!(may_add(&a, true, Some("me"), Some("bo")), "an administrator, any");
        assert_eq!(default_target(&a, Some("/in"), Some("ann")).as_deref(), Some("/own/ann"), "the requester's own music");
        assert_eq!(default_target(&a, Some("/in"), Some("bo")).as_deref(), Some("/second"), "off for them: the library's setting");
        assert_eq!(default_target(&a, Some("/new"), None).as_deref(), Some("/music"), "else the first main library");
    }

    fn album(id: &str, library: &str, path: &str) -> source::Album {
        source::Album {
            id: id.into(),
            title: id.into(),
            artist: None,
            year: None,
            added: None,
            path: Some(path.into()),
            tracks: 1,
            format: None,
            bitrate_kbps: None,
            duration_secs: None,
            tags: Vec::new(),
            library: Some(library.into()),
            copies: Vec::new(),
            artists: Vec::new(),
            release: Default::default(),
        }
    }

    fn verdict(kind: &'static str, library: &str, dir: &str) -> VerdictView {
        let main_folder = (library == "music").then_some(0);
        VerdictView {
            kind,
            library: dir.into(),
            why: String::new(),
            library_id: Some(library.into()),
            main_folder,
            replaceable_into: if main_folder.is_some() { vec![library.into()] } else { Vec::new() },
        }
    }

    /// A library's entry for its copy of record `id`, as `App::albums` has told it.
    fn listed(id: &str, folder: i64, library: &str, format: &str) -> source::Album {
        let path = format!("/{library}/A/Album");
        let copy = source::AlbumCopy { library: library.into(), format: Some(format.into()), plays: true, folder, dir: "A/Album".into(), ..Default::default() };
        source::Album { format: Some(format.into()), copies: vec![copy], ..album(id, library, &path) }
    }

    fn order(places: &[i64], plays: i64) -> HashMap<String, PlayOrder> {
        let at = |f: i64| (f, "A/Album".to_string());
        [("al-1".to_string(), PlayOrder { places: places.iter().copied().map(at).collect(), plays: Some(at(plays)), ..Default::default() })].into()
    }

    /// C37a: an Incoming copy of a record kept elsewhere says how it stands against the copy that plays the record for most, that
    /// copy named by its library; the kept copy says nothing.
    #[test]
    fn an_incoming_copy_says_how_it_stands_against_the_kept_one() {
        let at = |f: i64| (f, "A/Album".to_string());
        let stand = Stand { at: at(2), better: true, replaces: true, kept: at(0), why: "lossless, yours is 320 kbps".into() };
        let order = HashMap::from([("al-1".to_string(), PlayOrder { places: vec![at(0), at(2)], plays: Some(at(0)), against: vec![stand] })]);
        let folded = fold(vec![listed("al-1", 0, "music", "MP3"), listed("al-1", 2, "incoming", "FLAC")], &order);
        let against: Vec<Option<source::Against>> = folded[0].copies.iter().map(|c| c.against.clone()).collect();
        let said = source::Against { better: true, replaces: true, library: "music".into(), dir: "A/Album".into(), why: "lossless, yours is 320 kbps".into() };
        assert_eq!(against, [None, Some(said)]);
        let copies = [
            Copy { folder: 0, dir: "A/Album".into(), kind: LibraryKind::Main, order: 0, format: Some("MP3".into()), min_kbps: Some(320), bits: None, rate: None, short: false, added: None },
            Copy { folder: 2, dir: "A/Album".into(), kind: LibraryKind::Incoming, order: 1, format: Some("FLAC".into()), min_kbps: Some(900), bits: Some(16), rate: Some(44_100), short: false, added: None },
        ];
        assert_eq!(PlayOrder::of(&copies, None, &[]).against, order["al-1"].against, "worked out of the record's copies");
    }

    /// Two entries with one id, and a keyed list on the page throws: so a record is one entry whichever libraries hold it,
    /// and the card reads as the copy that plays.
    #[test]
    fn a_record_in_two_libraries_is_listed_once() {
        let albums = vec![listed("al-1", 0, "music", "MP3"), listed("al-2", 0, "music", "FLAC"), listed("al-1", 2, "incoming", "FLAC")];
        let folded = fold(albums, &order(&[0, 2], 0));
        assert_eq!(folded.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["al-1", "al-2"], "once, in the order they came");
        let record = &folded[0];
        assert_eq!((record.library.as_deref(), record.format.as_deref()), (Some("music"), Some("MP3")), "the card is the copy that plays");
        assert_eq!(record.copies.iter().map(|c| (c.library.as_str(), c.plays)).collect::<Vec<_>>(), [("music", true), ("incoming", false)]);
        assert_eq!(folded[1].copies.len(), 1, "a record with one copy as it was");
        assert!(folded[1].copies[0].plays);
    }

    /// `copies` is `copy_for`'s order for this viewer, and the first in it need not be what plays: that one can be off.
    #[test]
    fn a_records_copies_stand_in_the_viewers_order_and_the_card_is_what_plays() {
        let albums = vec![listed("al-1", 0, "music", "MP3"), listed("al-1", 1, "second", "FLAC"), listed("al-1", 2, "own", "OPUS")];
        let folded = fold(albums, &order(&[2, 1, 0], 1));
        assert_eq!(folded.len(), 1);
        assert_eq!(folded[0].copies.iter().map(|c| c.library.as_str()).collect::<Vec<_>>(), ["own", "second", "music"], "off or not, the same order");
        assert_eq!(folded[0].copies.iter().filter(|c| c.plays).map(|c| c.library.as_str()).collect::<Vec<_>>(), ["second"], "their own music is off");
        assert_eq!((folded[0].library.as_deref(), folded[0].format.as_deref()), (Some("second"), Some("FLAC")));
    }

    /// Two folders of one library are two copies (Q2), and nothing that names a record may lose one.
    #[test]
    fn two_folders_of_one_library_are_two_copies_of_one_entry() {
        let mut other = listed("al-1", 0, "music", "FLAC");
        other.copies[0].dir = "B/Album".into();
        let order = HashMap::from([("al-1".to_string(), PlayOrder { places: vec![(0, "B/Album".to_string()), (0, "A/Album".to_string())], plays: Some((0, "B/Album".to_string())), ..Default::default() })]);
        let folded = fold(vec![listed("al-1", 0, "music", "MP3"), other], &order);
        assert_eq!(folded.len(), 1);
        assert_eq!(folded[0].copies.iter().map(|c| (c.dir.as_str(), c.plays)).collect::<Vec<_>>(), [("B/Album", true), ("A/Album", false)]);
    }

    /// With no word from the library the record is still one entry with all its copies: a list must not lose one.
    #[test]
    fn a_record_with_no_order_read_keeps_every_copy() {
        let folded = fold(vec![listed("al-1", 0, "music", "MP3"), listed("al-1", 2, "incoming", "FLAC")], &HashMap::new());
        assert_eq!(folded.len(), 1);
        assert_eq!(folded[0].copies.len(), 2);
        assert_eq!(folded[0].copies.iter().filter(|c| c.plays).count(), 1, "one plays, as listed");
    }

    /// What the viewer may do goes by the copy: an Incoming copy can be added and lack a cover, while the kept one beside it
    /// can do neither, and a fold must carry each as it is.
    #[test]
    fn rights_are_per_copy() {
        let mut music = listed("al-1", 0, "music", "MP3");
        music.copies[0].deletable = true;
        let mut incoming = listed("al-1", 2, "incoming", "FLAC");
        let own = &mut incoming.copies[0];
        (own.keepable, own.keep_to, own.lacks, own.job, own.bits, own.added) = (true, Some("music".into()), vec!["cover".into()], Some(7), Some(24), Some(50));
        let folded = fold(vec![music, incoming], &order(&[0, 2], 0));
        let [kept, waiting] = &folded[0].copies[..] else { panic!("two copies") };
        assert!(kept.deletable && !kept.keepable && kept.keep_to.is_none() && kept.lacks.is_empty() && kept.job.is_none());
        assert!(!waiting.deletable && waiting.keepable);
        assert_eq!((waiting.keep_to.as_deref(), waiting.lacks.as_slice(), waiting.job, waiting.bits, waiting.added), (Some("music"), &["cover".to_string()][..], Some(7), Some(24), Some(50)));
    }

    /// R32 (N14): a requester has a say (Keep into their own music, Delete) over an Incoming copy of their request's record that
    /// came after they asked; one that was there before (an upgrade the administrator's Lidarr fetched) stays an administrator's.
    #[test]
    fn a_requester_has_no_say_over_an_incoming_copy_from_before_the_request() {
        let asked = HashMap::from([(1, super::super::requests::Requester { user: "bob".into(), at: 1_000 })]);
        let came = |added: Option<i64>| source::Album { added, ..listed("al-1", 2, "incoming", "FLAC") };
        assert_eq!(requester_of(&asked, &came(Some(2_000))).map(String::as_str), Some("bob"), "came after he asked");
        assert_eq!(requester_of(&asked, &came(Some(500))), None, "there before he asked");
        assert_eq!(requester_of(&asked, &came(None)), None, "when it came not known");
        assert_eq!(requester_of(&asked, &source::Album { added: Some(2_000), ..listed("al-2", 2, "incoming", "FLAC") }), None, "another record");
    }

    /// A requester may delete their Incoming copy, so the record's id is in the view's `deletable`: the right that counts is
    /// the copy's.
    #[test]
    fn a_right_to_one_copy_does_not_delete_another() {
        let mut waiting = listed("al-1", 2, "incoming", "FLAC");
        waiting.copies[0].deletable = true;
        let folded = fold(vec![listed("al-1", 0, "music", "MP3"), waiting], &order(&[0, 2], 0));
        let deletable = ["al-1".to_string()];
        let copy = |from: &str| named_copy(&folded[0], Some(from)).unwrap();
        assert!(!may_delete_copy(&deletable, &copy("music")), "the kept copy is not theirs");
        assert!(may_delete_copy(&deletable, &copy("incoming")), "their request is");
        let other = album("x", "jellyfin-music", "/m/x");
        assert!(may_delete_copy(&["x".to_string()], &other) && !may_delete_copy(&[], &other), "another server's goes by id");
    }

    /// The entry stands for the copy that plays, and what is named must come out of it as that copy alone: its library, its
    /// place, its numbers. This is what keeps a Delete off the copy that plays.
    #[test]
    fn a_named_copy_is_that_copy_and_not_the_one_that_plays() {
        let folded = fold(vec![listed("al-1", 0, "music", "MP3"), listed("al-1", 2, "incoming", "FLAC")], &order(&[0, 2], 0));
        assert_eq!(folded[0].library.as_deref(), Some("music"), "the card is the copy that plays");
        let named = named_copy(&folded[0], Some("incoming")).unwrap();
        assert_eq!((named.library.as_deref(), named.path.as_deref()), (Some("incoming"), Some("incoming/A/Album")));
        assert_eq!(named.copies.iter().map(|c| (c.folder, c.library.as_str())).collect::<Vec<_>>(), [(2, "incoming")], "and nothing of the other");
        assert!(matches!(named_copy(&folded[0], Some("second")), Err(ToggleError::Refused(_))), "no copy there");
    }

    /// Left to the copy that plays, an administrator's Delete would land on whatever plays for them today.
    #[test]
    fn a_record_of_several_copies_must_be_told_which() {
        let folded = fold(vec![listed("al-1", 0, "music", "MP3"), listed("al-1", 2, "incoming", "FLAC")], &order(&[0, 2], 0));
        assert!(matches!(named_copy(&folded[0], None), Err(ToggleError::Refused(_))));
        assert_eq!(named_copy(&listed("al-2", 0, "music", "FLAC"), None).unwrap().copies[0].folder, 0, "with one, there is nothing to tell");
        let other = album("x", "jellyfin-music", "/m/x");
        assert!(named_copy(&other, None).is_ok() && named_copy(&other, Some("jellyfin-music")).is_ok());
        assert!(named_copy(&other, Some("music")).is_err(), "another server's album is its one library's");
    }

    /// Q2: two folders of one library are two copies, and naming the library names the first as the record's copies stand.
    #[test]
    fn of_two_folders_of_one_library_the_first_is_named() {
        let mut other = listed("al-1", 0, "music", "FLAC");
        other.copies[0].dir = "B/Album".into();
        let order = HashMap::from([("al-1".to_string(), PlayOrder { places: vec![(0, "B/Album".to_string()), (0, "A/Album".to_string())], plays: Some((0, "B/Album".to_string())), ..Default::default() })]);
        let folded = fold(vec![listed("al-1", 0, "music", "MP3"), other], &order);
        assert_eq!(named_copy(&folded[0], Some("music")).unwrap().copies[0].dir, "B/Album");
    }

    /// A conversion of the Incoming copy is not the kept copy's: by id alone the kept one's menu would show it, and two folders
    /// of one library would share it.
    #[test]
    fn a_job_shows_on_its_copy() {
        let job = JobView::converting(7, "cratarr", "al-1", "incoming", 2, "A/Album");
        let jobs = [job.clone()];
        assert_eq!(job_on(&jobs, "cratarr", &listed("al-1", 2, "incoming", "FLAC")).map(|j| j.id), Some(7));
        assert!(job_on(&jobs, "cratarr", &listed("al-1", 0, "music", "MP3")).is_none(), "the record's other copy");
        let mut beside = listed("al-1", 2, "incoming", "FLAC");
        beside.copies[0].dir = "B/Album".into();
        assert!(job_on(&jobs, "cratarr", &beside).is_none(), "another folder of its library");
        assert!(job_on(&[job.done()], "cratarr", &listed("al-1", 2, "incoming", "FLAC")).is_none(), "over");
        let elsewhere = [JobView::converting(8, "jf", "x", "jellyfin-music", 0, "")];
        assert!(job_on(&elsewhere, "jf", &album("x", "jellyfin-music", "/m/x")).is_some(), "another server's goes by its library");
    }

    /// The Users panel names the account's library: a record also in Music is found there, not refused as Music's.
    #[test]
    fn delete_own_finds_the_owners_copy_of_a_record_also_in_music() {
        let folded = fold(vec![listed("al-1", 0, "music", "FLAC"), listed("al-1", 3, "/own/ann", "MP3")], &order(&[0, 3], 0));
        assert_eq!(folded[0].library.as_deref(), Some("music"));
        let named = named_copy(&folded[0], Some("/own/ann")).unwrap();
        assert_eq!((named.library.as_deref(), named.copies[0].folder), (Some("/own/ann"), 3));
    }

    /// The page's contract, `AlbumCopy` in `lib/api.ts`, and what stays out of it: a record's place is no business of a page.
    #[test]
    fn a_copy_goes_to_the_page_as_lib_api_says() {
        let copy = source::AlbumCopy { library: "music".into(), plays: true, folder: 3, dir: "A/Album".into(), ..Default::default() };
        let sent = serde_json::to_value(&copy).unwrap();
        let mut keys: Vec<&str> = sent.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["added", "bits", "deletable", "format", "job", "kbps", "keep_to", "keepable", "lacks", "library", "plays", "rate", "size", "tracks"]);
        assert_eq!(sent["lacks"], serde_json::json!([]), "a list");
        assert_eq!((&sent["job"], &sent["bits"], &sent["keep_to"]), (&serde_json::Value::Null, &serde_json::Value::Null, &serde_json::Value::Null), "null, not left out");
        let other = serde_json::to_value(album("x", "jellyfin-music", "/m/x")).unwrap();
        assert!(other.get("copies").is_none() && other.get("library").is_some(), "another server's album has its library alone");
    }

    /// `plays` is `copy_for`'s, as a stream hears it: with the library switched off nothing plays, and a record only in
    /// Incoming must still say so of its copy, or a switch thrown on the page finds none.
    #[test]
    fn what_plays_is_named_even_with_it_switched_off() {
        let copy = |folder: i64, kind: LibraryKind, format: &str, bits: Option<i64>| Copy { folder, dir: "A/Album".into(), kind, order: folder as usize, format: Some(format.into()), min_kbps: Some(900), bits, rate: Some(44_100), short: false, added: None };
        let place = |f: i64| Some((f, "A/Album".to_string()));
        let only = [copy(2, LibraryKind::Incoming, "MP3", None)];
        let nobody = Viewer::default();
        assert_eq!(PlayOrder::of(&only, Some(&nobody), &[]).plays, place(2));
        assert_eq!(PlayOrder::of(&only, Some(&nobody), &[2]).plays, place(2), "with Incoming off");
        let two = [copy(0, LibraryKind::Main, "MP3", None), copy(1, LibraryKind::Main, "FLAC", Some(16))];
        let order = PlayOrder::of(&two, Some(&nobody), &[1]);
        assert_eq!(order.plays, place(0), "the better copy is off");
        assert_eq!(order.places, PlayOrder::of(&two, Some(&nobody), &[]).places, "and the order does not care");
        assert_eq!(order.places[0], (1, "A/Album".to_string()));
    }

    #[test]
    fn a_music_album_knows_what_waits_for_it() {
        let music = [album("m1", "music", "/data/media/Music/Arp/Zebra"), album("m2", "music", "/data/media/Music/Ott/Heads (2022)")];
        let mut verdicts = BTreeMap::new();
        verdicts.insert("i1".to_string(), verdict("duplicate", "music", "Arp/Zebra"));
        verdicts.insert("i2".to_string(), verdict("upgrade", "music", "Arp/Zebra"));
        verdicts.insert("i3".to_string(), verdict("upgrade", "music", "Nobody/Here"));
        let w = waiting(&verdicts, &music);
        assert_eq!(w.len(), 1);
        assert_eq!((w["m1"].kind, w["m1"].album.as_str()), ("upgrade", "i2"));
    }

    #[test]
    fn an_album_of_another_library_knows_what_waits_for_it_in_its_own_library() {
        // The same folder in Music and in two Incoming libraries: each verdict finds its pair in the library it names.
        let albums = [
            album("m1", "music", "/data/media/Music/Arp/Zebra"),
            album("a1", "lidarr", "/data/media/lidarr/Arp/Zebra"),
            album("b1", "new", "/data/media/new/Arp/Zebra"),
        ];
        let mut verdicts = BTreeMap::new();
        verdicts.insert("a1".to_string(), verdict("duplicate", "new", "Arp/Zebra"));
        verdicts.insert("b1".to_string(), verdict("upgrade", "lidarr", "Arp/Zebra"));
        let w = waiting(&verdicts, &albums);
        assert_eq!(w.len(), 2, "not Music's, which no verdict names");
        assert_eq!((w["a1"].kind, w["a1"].album.as_str()), ("upgrade", "b1"));
        assert_eq!((w["b1"].kind, w["b1"].album.as_str()), ("duplicate", "a1"));
    }
}
