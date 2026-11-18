//! The Library page's one list (decision 0034): every album of the followed server's used libraries, each with its
//! library, and what holds against the other libraries. Made of Collection's lists (Music) and Incoming's, which keep
//! and refresh themselves; this only puts them together.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use super::{App, JobView, KeepPreview, KeepResult, ToggleError};
use super::library::{Active, epoch_now};
use crate::serve::copies::{Copy, Viewer, against, copy_for, in_order, judged, replace_offers, replaces, replacing_copy, why};
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
/// the account's page asks (0101 point 7). Nothing is copied into Incoming (T13.3): what waits there is a fetcher's or a
/// request's, and no page offers it.
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
    (to == Some(LibraryKind::Incoming) && copies(from, to, viewers_own, left)).then_some("It can't be copied into Incoming")
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
    /// Per album id of a library other than Music: what it lacks of what the library requires (the owner,
    /// 2026-10-24: marked before Keep, as Health marks Music's). This and the lists below are the server's own: the page is
    /// sent each copy's (C36, `AlbumsView::as_sent`).
    #[serde(skip)]
    pub lacks: BTreeMap<String, Vec<&'static str>>,
    /// The albums Cratarr can delete.
    #[serde(skip)]
    pub deletable: Vec<String>,
    /// The Incoming albums the viewer may Keep (0101 point 2: an administrator every one; a user their own requests,
    /// once own music is in).
    #[serde(skip)]
    pub keepable: Vec<String>,
    /// Per Incoming album, where Keep puts it by default (0101 point 2: the requester's own music, else the first main
    /// library).
    #[serde(skip)]
    pub keep_to: BTreeMap<String, String>,
}

impl AlbumsView {
    /// The view as the page is sent it (C36): what the viewer may do with each copy, and what it lacks, on the copy alone.
    /// Another server's album gets its one copy, made of its source's lists by id; Cratarr's own records, their copies saying
    /// their libraries, say no `library` of their own.
    pub fn as_sent(mut self) -> Self {
        for s in &mut self.sources {
            for a in &mut s.albums {
                if !a.copies.is_empty() {
                    a.library = None;
                    continue;
                }
                let Some(library) = a.library.clone() else { continue };
                a.copies.push(source::AlbumCopy {
                    library,
                    format: a.format.clone(),
                    kbps: a.bitrate_kbps,
                    deletable: s.deletable.contains(&a.id),
                    keepable: s.keepable.contains(&a.id),
                    keep_to: s.keep_to.get(&a.id).cloned(),
                    lacks: s.lacks.get(&a.id).map(|l| l.iter().map(|w| w.to_string()).collect()).unwrap_or_default(),
                    plays: true,
                    tracks: a.tracks,
                    ..Default::default()
                });
            }
        }
        self
    }
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
                    albums,
                    error: c.and_then(|c| c.error.clone()).or_else(|| i.and_then(|i| i.error.clone())),
                    as_of: [c.map(|c| c.as_of), i.map(|i| i.as_of)].into_iter().flatten().min().unwrap_or_else(epoch_now),
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
            self.paired_stands(s).await;
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
/// of one library the one at `dir`, the folder a tap's sheet showed (RF10, the review of runs 11 and 12, RA9), else the first, as
/// the record's copies stand (Q2). A record of several copies must be told which: what is done to its files cannot be left to which
/// of them plays. Another server's album is its one library's.
fn named_copy(a: &source::Album, from: Option<&str>, dir: Option<&str>) -> Result<source::Album, ToggleError> {
    let elsewhere = || ToggleError::Refused("It has no copy in that library".into());
    if a.copies.is_empty() {
        return match from {
            Some(f) if a.library.as_deref() != Some(f) => Err(elsewhere()),
            _ => Ok(a.clone()),
        };
    }
    match from {
        Some(f) => a.copies.iter().find(|c| c.library == f && dir.is_none_or(|d| c.dir == d)).map(|c| a.as_copy(c)).ok_or_else(elsewhere),
        None => match a.copies.as_slice() {
            [only] => Ok(a.as_copy(only)),
            _ => Err(ToggleError::Refused("It is in several libraries: say which copy".into())),
        },
    }
}

/// What a Want of a record the library holds copies into its asker's own music (T5): nothing where the asker holds a copy of it in
/// their own music already; none where no copy is known whole; else the copy, by its catalogue folder and place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Chosen {
    Theirs,
    NoneWhole,
    From(i64, String),
}

/// The copy a Want copies (T5; the owner's "Yes, from anyone's"): of a record's copies, each judged for copying (S.1), those not
/// short, in a kept library, not switched off (`off`, catalogue folders): the one that plays for most (`copy_for` for no one: a
/// main library's, then, where only own music holds it, anyone's first added; a tie by the libraries' order and place). `asker`:
/// the asker's own music's folder, where it is in use. Pure.
pub(crate) fn want_copy(copies: &[(Copy, crate::catalogue::identity::Lacks)], asker: Option<i64>, off: &[i64]) -> Chosen {
    if asker.is_some_and(|a| copies.iter().any(|(c, _)| c.folder == a)) {
        return Chosen::Theirs;
    }
    let whole: Vec<Copy> = copies.iter().filter(|(c, l)| c.kind != LibraryKind::Incoming && !off.contains(&c.folder) && l.copies()).map(|(c, _)| c.clone()).collect();
    copy_for(&whole, None, &[]).map_or(Chosen::NoneWhole, |c| Chosen::From(c.folder, c.dir.clone()))
}

/// The catalogue folder a used library of a server lies in (its first folder), where it is in use.
fn folder_in(on: &Active, folders: &crate::catalogue::InUse, library: &str) -> Option<i64> {
    on.placement.libraries.iter().find(|l| l.id == library).and_then(|l| l.folders.first()).and_then(|p| folders.id_of(std::path::Path::new(p)))
}

/// The catalogue folders of own music switched off for its account (it owns none, `Active::libraries`): no Want copies from them
/// (T5), and no Replace takes a main library's place from them (RY01).
fn own_off(on: &Active, folders: &crate::catalogue::InUse) -> Vec<i64> {
    on.libraries().into_iter().filter(|u| u.kind == LibraryKind::Own && u.owner.is_none()).filter_map(|u| folder_in(on, folders, &u.id)).collect()
}

/// The own copy Replace from someone's own music takes in catalogue folder `own` (RY01; RF10, the review's RA9), of a record's
/// `copies` as copying judges them: the one at `dir`, the folder its tap names, as its preview showed it; else, asked by the library
/// alone (a preview), the one Replace offers there against one of `mains` (`copies::replacing_copy`), else the first there. None: no
/// copy there, or none at `dir`. Pure.
fn own_named<'a>(copies: &'a [Copy], own: i64, dir: Option<&str>, mains: &[&Copy], off: &[i64]) -> Option<&'a Copy> {
    let first = || copies.iter().find(|c| c.folder == own);
    match dir {
        Some(d) => copies.iter().find(|c| c.folder == own && c.dir == d),
        None => mains.iter().find_map(|m| replacing_copy(copies, m, off)).filter(|o| o.folder == own).or_else(first),
    }
}

/// Replace from someone's own music (RY01) as `App::own_offer` finds it: the record's title, the own copy listed alone, the main
/// library's copy it would take the place of, and why not, where it is not offered.
struct OwnOffer {
    title: String,
    copy: source::Album,
    main: (i64, String),
    refused: Option<String>,
}

/// A Replace's tap (`App::replace_album`): the copies, the year where the tags lack it, the cover (`keep`, `new`), carry over, and
/// (RY01) what its sheet showed, the own copy named by its folder (`from_dir`, RF10).
pub struct ReplaceAsk<'a> {
    pub from: Option<&'a str>,
    pub to: Option<&'a str>,
    pub year: Option<&'a str>,
    pub cover: Option<&'a str>,
    pub carry: bool,
    pub sheet: Option<super::ReplaceSheet>,
    pub from_dir: Option<&'a str>,
}

/// The one record's listing of the copy a Want copies (T5): its card with that copy alone, by its folder and place, in the library
/// `root` names. Never the whole Albums view. Pure.
fn copy_listed(card: source::Album, root: &str, (folder, dir): (i64, String)) -> source::Album {
    let copy = source::AlbumCopy { library: root.to_string(), folder, dir: dir.clone(), ..Default::default() };
    source::Album { library: Some(root.to_string()), path: Some(format!("{}/{dir}", root.trim_end_matches('/'))), copies: vec![copy], ..card }
}

/// A record's copies (`album`, the holder `record`), each judged for copying (S.1, `identity::lacks`): whether it holds fewer of its
/// tracks than its copies together, a track of the counting list, what MusicBrainz's check found, and its files' tags. Blocking.
fn judged_copies(served: &crate::serve::Serve, album: &str, record: i64) -> anyhow::Result<Vec<(Copy, crate::catalogue::identity::Lacks)>> {
    use crate::catalogue::identity::{Judged, Missing, lacks};
    let cat = served.catalogue();
    let list = cat.requested(&[record])?.get(&record).and_then(|q| q.list);
    let whole = served.library.whole_of(album);
    served
        .library
        .copies_of(album)?
        .into_iter()
        .map(|c| {
            let files = cat.numbers_under(c.folder, &c.dir)?;
            let tracks = files.iter().filter_map(|(t, _)| *t).collect::<std::collections::BTreeSet<i64>>().len();
            let judged = Judged {
                fewer_than_its_copies: c.short,
                short_of_list: list.map(|n| tracks < n),
                checked: whole.map(|w| Missing { has: tracks as u32, of: w.tracks as u32 }),
                files: files.into_iter().map(|(_, n)| n).collect(),
            };
            let said = lacks(&judged);
            Ok((c, said))
        })
        .collect()
}

/// What a Want's copy of a record the library holds came to (T5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WantCopy {
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
pub(super) fn may_add(a: &Active, admin: bool, viewer: Option<&str>, requester: Option<&str>) -> bool {
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
    /// Each Replace in a main library from someone's own copy (RY01, `copies::replace_offers`): the own copy's place, and the main
    /// library's copy it beats. One per main library (OQ-N).
    replace_into: Vec<Stand>,
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
            replace_into: Vec::new(),
        }
    }

    /// The Replaces from someone's own music a record's copies, as copying judges them (`copies::judged`), offer (RY01), own music
    /// switched off (`off`) left out.
    fn offering(mut self, judged: &[Copy], off: &[i64]) -> Self {
        let place = |c: &Copy| (c.folder, c.dir.clone());
        self.replace_into =
            replace_offers(judged, off).into_iter().map(|(own, main)| Stand { at: place(own), better: true, replaces: true, kept: place(main), why: why(own, main) }).collect();
        self
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
            // Someone's own copy says which main libraries' copies Replace from it would take the place of (RY01), each named by its
            // library as listed.
            for s in &order.replace_into {
                let Some(library) = libraries.iter().find(|(at, _)| *at == s.kept).map(|(_, l)| l.clone()) else { continue };
                if let Some(c) = copies.iter_mut().find(|c| (c.folder, c.dir.clone()) == s.at) {
                    c.replace_into.push(source::Against { better: s.better, replaces: s.replaces, library, dir: s.kept.1.clone(), why: s.why.clone() });
                }
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
        let off = self.own_music_off();
        let read = tokio::task::spawn_blocking(move || -> anyhow::Result<HashMap<String, PlayOrder>> {
            let mut out = HashMap::new();
            for (id, places) in listed {
                // Only what is listed can stand for the record: a copy read since the list was has no entry here yet.
                let here = |c: &Copy| places.iter().any(|(f, d)| *f == c.folder && *d == c.dir);
                let copies: Vec<Copy> = served.library.copies_of(&id)?.into_iter().filter(here).collect();
                let mut order = PlayOrder::of(&copies, who.viewer().as_ref(), &who.off);
                // Replace from someone's own music (RY01), where the record has a copy there and one in a main library: its copies
                // as copying judges them (S.1).
                let kinds = |k: LibraryKind| copies.iter().any(|c| c.kind == k);
                if let (true, true, Some(record)) = (kinds(LibraryKind::Own), kinds(LibraryKind::Main), crate::serve::ids::record_number(&id)) {
                    match judged_copies(&served, &id, record) {
                        Ok(all) => {
                            let listed: Vec<Copy> = judged(&all).into_iter().filter(here).collect();
                            order = order.offering(&listed, &off);
                        }
                        Err(e) => tracing::warn!(target: "library", album = %id, reason = %format!("{e:#}"), "its copies not judged: no Replace from own music offered"),
                    }
                }
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

    /// RF2: each Incoming copy of a record kept nowhere else that another record's copy in a main library pairs with track for track
    /// stands against that copy (`Serve::stands_of`), as a copy of one record does (`fold`): the badge, Health's better copies and
    /// records in two libraries, and Replace read it, that copy named by its library as listed.
    async fn paired_stands(&self, s: &mut AlbumsSourceView) {
        let Some(served) = self.served.clone() else { return };
        let incoming: Vec<String> = s.library_info.iter().filter(|l| l.kind == LibraryKind::Incoming.as_str()).map(|l| l.id.clone()).collect();
        let waiting: Vec<(String, (i64, String))> = s
            .albums
            .iter()
            .filter(|a| !a.copies.is_empty() && a.copies.iter().all(|c| c.against.is_none() && incoming.contains(&c.library)))
            .flat_map(|a| a.copies.iter().map(|c| (a.id.clone(), (c.folder, c.dir.clone()))))
            .collect();
        if waiting.is_empty() {
            return;
        }
        let mains = self.library.main_folders();
        type Stood = Vec<(String, (i64, String), Option<(bool, Copy, Copy)>)>;
        let read = tokio::task::spawn_blocking(move || -> anyhow::Result<Stood> {
            let mut asked: Vec<(String, Copy)> = Vec::new();
            for (id, (folder, dir)) in &waiting {
                if let Some(c) = served.library.copies_of(id)?.into_iter().find(|c| c.folder == *folder && &c.dir == dir) {
                    asked.push((id.clone(), c));
                }
            }
            let stands = served.stands_of(&asked, &mains)?;
            Ok(asked.into_iter().zip(stands).map(|((id, c), stand)| (id, (c.folder, c.dir.clone()), stand.map(|(better, kept)| (better, c, kept)))).collect())
        });
        let stood = match read.await {
            Ok(Ok(stood)) => stood,
            Ok(Err(e)) => return tracing::warn!(target: "library", reason = %format!("{e:#}"), "how copies waiting stand not read"),
            Err(e) => return tracing::warn!(target: "library", reason = %e, "how copies waiting stand not read"),
        };
        let libraries: HashMap<(i64, String), String> = s.albums.iter().flat_map(|a| a.copies.iter().map(|c| ((c.folder, c.dir.clone()), c.library.clone()))).collect();
        for (id, at, stand) in stood {
            let Some((better, c, kept)) = stand else { continue };
            let Some(library) = libraries.get(&(kept.folder, kept.dir.clone())).cloned() else { continue };
            if let Some(copy) = s.albums.iter_mut().filter(|a| a.id == id).flat_map(|a| a.copies.iter_mut()).find(|x| (x.folder, x.dir.clone()) == at) {
                copy.against = Some(source::Against { better, replaces: replaces(better, kept.kind), library, dir: kept.dir.clone(), why: why(&c, &kept) });
            }
        }
    }

    /// A listed album of a server (Music's or another library's), by id, as the viewer sees that server: the copy `from` names, at
    /// `dir` where a tap's sheet says which (`named_copy`).
    async fn listed_album(&self, source: &str, album: &str, from: Option<&str>, dir: Option<&str>) -> Result<(AlbumsSourceView, source::Album), ToggleError> {
        let view = self.albums(Some(source)).await;
        let s = view.sources.into_iter().find(|s| s.id == source).ok_or_else(|| ToggleError::Refused("that server is not connected".into()))?;
        let a = s.albums.iter().find(|a| a.id == album).ok_or_else(|| ToggleError::Refused("that album is no longer listed".into()))?;
        let a = named_copy(a, from, dir)?;
        Ok((s, a))
    }

    /// Delete a copy the viewer may, into its library's trash (decision 0034). The record stays while another copy does.
    pub async fn delete_album(&self, source: &str, album: &str, from: Option<&str>) -> Result<super::DeleteResult, ToggleError> {
        let (s, a) = self.listed_album(source, album, from, None).await?;
        if !may_delete_copy(&s.deletable, &a) {
            return Err(ToggleError::Refused("That isn't yours to delete".into()));
        }
        // A copy in Incoming that arrived for requests (W4, the owner's answer (c)): read before it goes, they close as rejected once
        // it has.
        let live = self.library.live();
        let incoming = a.library.as_deref().and_then(|l| live.iter().find(|x| x.cfg.id == source)?.kind_of(l)) == Some(LibraryKind::Incoming);
        // Whose it was (U5, T9): its library's fetcher, and its folder there, which Lidarr's history is matched by: a grab is marked
        // failed only for the copy's own import, where another asker is left.
        let copy = incoming.then(|| super::requests::DeletedCopy {
            fetcher: a.library.as_deref().and_then(|l| live.iter().find(|x| x.cfg.id == source)?.cfg.libraries.get(l)?.fetcher.clone()),
            folder: a.copies.first().map(|c| c.dir.clone()).unwrap_or_default(),
        });
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
            let goes_on = super::requests::deleted(super::requests::fetcher(self).as_deref(), soulseek, served, &closing, open, copy.as_ref(), id.map(|id| (self.journal.as_ref(), id))).await;
            if goes_on && let Some(id) = id {
                self.incoming.keep_fetching(id, &done.title).await;
            }
        }
        Ok(done)
    }

    /// `account`'s copy of a record the library holds, in their own music (C52, OQ3: "Add to Your music" through Want), made as Copy
    /// to makes one (C40), from anyone's whole copy (T5, the owner's "Yes, from anyone's"): the copy `want_copy` chooses among those
    /// judged whole enough (`judged_copies`), named by its folder and place, read from the one record's listing (never the whole
    /// Albums view); into their own music, made now where it is not in use yet. The copy it is made of is never moved or changed.
    /// `sharing` (below the floor, AL03.1): made only where a copy from that copy's library into their own music shares its blocks
    /// (`Files::shares`, C38's probe), taking no new space; else none is made, nor their own music (`WantCopy::NoSpace`).
    pub(super) async fn copy_for(&self, account: &str, record: i64, sharing: bool) -> WantCopy {
        let source = crate::source::cratarr::KIND.id;
        let Some(served) = self.served.clone() else { return WantCopy::Failed("Cratarr's own library is not open".into()) };
        let (own, used, asker, off) = {
            let live = self.library.live();
            let Some(on) = live.iter().find(|x| x.cfg.id == source) else { return WantCopy::Failed("Cratarr's own source is not connected".into()) };
            let Some(own) = on.own_of(account) else { return WantCopy::Nothing };
            let folders = self.library.catalogue().folders();
            // Own music switched off for its account (it owns none, `Active::libraries`) is copied from by no one.
            (own.clone(), on.used(&own), folder_in(on, &folders, &own), own_off(on, &folders))
        };
        let id = crate::serve::ids::record(record);
        let s = served.clone();
        let read = tokio::task::spawn_blocking(move || -> anyhow::Result<Result<(source::Album, Option<std::path::PathBuf>), Chosen>> {
            let chosen = want_copy(&judged_copies(&s, &id, record)?, asker, &off);
            let Chosen::From(folder, dir) = chosen else { return Ok(Err(chosen)) };
            let card = s.library.albums_by_id(std::slice::from_ref(&id))?.pop().ok_or_else(|| anyhow::anyhow!("its record is not listed"))?;
            let root = s.library.roots().into_iter().find(|(f, _)| *f == folder).map(|(_, r)| r).ok_or_else(|| anyhow::anyhow!("its copy's library is not listed"))?;
            let from = s.catalogue().folders().get(folder).cloned();
            Ok(Ok((copy_listed(crate::source::cratarr::card(card), &root, (folder, dir)), from)))
        })
        .await;
        let (album, from) = match read {
            Ok(Ok(Ok(read))) => read,
            Ok(Ok(Err(Chosen::Theirs))) => return WantCopy::Nothing,
            Ok(Ok(Err(_))) => return WantCopy::NoneWhole,
            Ok(Err(e)) => return WantCopy::Failed(format!("its copies could not be read: {e:#}")),
            Err(e) => return WantCopy::Failed(format!("its copies could not be read: {e}")),
        };
        // Below the floor: only a copy that shares its blocks, probed into their own music, or the own-music folder it is made in.
        if sharing {
            let target = std::path::PathBuf::from(&own);
            let to = if target.is_dir() { Some(target) } else { target.parent().map(std::path::Path::to_path_buf) };
            let incoming = self.incoming.clone();
            let shares = match (from, to) {
                (Some(from), Some(to)) => tokio::task::spawn_blocking(move || incoming.shares(&from, &to)).await.unwrap_or(false),
                _ => false,
            };
            if !shares {
                return WantCopy::NoSpace;
            }
        }
        let to = if used {
            own
        } else {
            match self.library.own_library(account).await {
                Ok(Some(made)) => made,
                Ok(None) => return WantCopy::Nothing,
                Err(e) => return WantCopy::Failed(format!("its own music could not be made: {e:#}")),
            }
        };
        match self.incoming.copy_album(source, album, &to, None, None).await {
            Ok(_) => WantCopy::Made,
            Err(e) => WantCopy::Failed(e.to_string()),
        }
    }

    /// The catalogue folders of own music switched off for its account, Cratarr's own (`own_off`).
    fn own_music_off(&self) -> Vec<i64> {
        let live = self.library.live();
        live.iter().find(|x| x.cfg.id == crate::source::cratarr::KIND.id).map(|on| own_off(on, &self.library.catalogue().folders())).unwrap_or_default()
    }

    /// The own library a Replace's `from` names (RY01): someone's own music of Cratarr's own; none for anything else, which is
    /// Replace from Incoming.
    fn own_source(&self, source: &str, from: Option<&str>) -> Option<String> {
        let from = from?;
        let live = self.library.live();
        let own = source == crate::source::cratarr::KIND.id && live.iter().find(|x| x.cfg.id == source)?.kind_of(from) == Some(LibraryKind::Own);
        own.then(|| from.to_string())
    }

    /// Replace in a main library from someone's own music (RY01) as the record stands now: the own copy `from` names, listed alone
    /// by its folder and place (`copy_listed`, as T5's Want reads it: never the whole Albums view), and the main library's copy it
    /// would take the place of, in `to`, else the first main library it beats. Offered only where that own copy is the one
    /// `copies::replacing_copy` takes, of the record's copies as copying judges them (S.1), own music switched off left out; else
    /// `refused` says why.
    async fn own_offer(&self, source: &str, album: &str, from: &str, to: Option<&str>, dir: Option<&str>) -> Result<OwnOffer, ToggleError> {
        let served = self.served.clone().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open".into()))?;
        let record = crate::serve::ids::record_number(album).ok_or_else(|| ToggleError::Refused("that album is no longer listed".into()))?;
        let (own, target, off) = {
            let live = self.library.live();
            let on = live.iter().find(|x| x.cfg.id == source).ok_or_else(|| ToggleError::Refused("that server is not connected".into()))?;
            let folders = self.library.catalogue().folders();
            let unseen = || ToggleError::Refused("that library's folder is not in use yet".into());
            let own = folder_in(on, &folders, from).ok_or_else(unseen)?;
            let target = match to {
                Some(to) if on.kind_of(to) == Some(LibraryKind::Main) => Some(folder_in(on, &folders, to).ok_or_else(unseen)?),
                Some(_) => return Err(ToggleError::Refused("Replace takes the place of a copy in a main library".into())),
                None => None,
            };
            (own, target, own_off(on, &folders))
        };
        let (id, s, dir) = (album.to_string(), served.clone(), dir.map(str::to_string));
        let read = tokio::task::spawn_blocking(move || -> anyhow::Result<Result<OwnOffer, String>> {
            let copies = judged(&judged_copies(&s, &id, record)?);
            let mains: Vec<&Copy> = copies.iter().filter(|c| c.kind == LibraryKind::Main && target.is_none_or(|t| t == c.folder)).collect();
            let Some(mine) = own_named(&copies, own, dir.as_deref(), &mains, &off) else { return Ok(Err("It has no copy in that library".into())) };
            let offered = mains.iter().copied().find(|m| replacing_copy(&copies, m, &off).is_some_and(|o| (o.folder, &o.dir) == (mine.folder, &mine.dir)));
            let Some(main) = offered.or_else(|| mains.first().copied()) else { return Ok(Err("It has no copy in a main library to replace".into())) };
            let refused = match offered {
                Some(_) => None,
                None if off.contains(&mine.folder) => Some("That music is switched off".to_string()),
                None if mine.short && !main.short => Some("That copy lacks tracks".to_string()),
                None => Some("That copy is not the best one better than the main library's".to_string()),
            };
            let card = s.library.albums_by_id(std::slice::from_ref(&id))?.pop().ok_or_else(|| anyhow::anyhow!("its record is not listed"))?;
            let root = s.library.roots().into_iter().find(|(f, _)| *f == own).map(|(_, r)| r).ok_or_else(|| anyhow::anyhow!("its copy's library is not listed"))?;
            let card = crate::source::cratarr::card(card);
            Ok(Ok(OwnOffer { title: card.title.clone(), copy: copy_listed(card, &root, (mine.folder, mine.dir.clone())), main: (main.folder, main.dir.clone()), refused }))
        })
        .await;
        match read {
            Ok(Ok(Ok(offer))) => Ok(offer),
            Ok(Ok(Err(why))) => Err(ToggleError::Refused(why)),
            Ok(Err(e)) => Err(ToggleError::Refused(format!("its copies could not be read: {e:#}"))),
            Err(e) => Err(ToggleError::Refused(format!("its copies could not be read: {e}"))),
        }
    }

    /// What Replace would do (decision 0013), for the page to show before asking: of an Incoming copy, or (RY01) of someone's own
    /// copy, whose sheet offers no cover to choose and carries nothing over (OQ-A), and is refused, saying why, where that copy is not
    /// the one offered. Changes nothing.
    pub async fn replace_preview(&self, source: &str, album: &str, from: Option<&str>, to: Option<&str>, year: Option<&str>) -> Result<super::ReplacePreview, ToggleError> {
        let Some(own) = self.own_source(source, from) else { return self.incoming.replace_preview(source, album, from, to, year).await };
        let offer = self.own_offer(source, album, &own, to, None).await?;
        match offer.refused {
            Some(why) => Ok(super::ReplacePreview::refused_from_copy(offer.title, offer.main.1, own, why)),
            None => self.incoming.replace_from_copy_preview(source, offer.copy, offer.main, year).await,
        }
    }

    /// Replace (decision 0013) of an Incoming copy, or (RY01) from someone's own copy, the administrators' as its route is. From own
    /// music no cover is chosen and nothing is carried over (OQ-A: `cover: keep` and `carry` are refused), and the tap carries what
    /// its sheet showed (OQ-G): refused where the copy stopped being the one offered, or the plan changed, since.
    pub async fn replace_album(&self, source: &str, album: &str, ask: ReplaceAsk<'_>) -> Result<KeepResult, ToggleError> {
        let Some(own) = self.own_source(source, ask.from) else {
            let keep = ask.cover != Some("new");
            return self.incoming.replace(source, album, ask.from, ask.to, ask.year, keep, ask.carry).await;
        };
        if ask.cover == Some("keep") || ask.carry {
            return Err(ToggleError::Refused("From someone's own music the copy goes in as it is: no cover is kept and nothing is carried over".into()));
        }
        let sheet = ask.sheet.ok_or_else(|| ToggleError::Refused("Say what Replace's sheet showed: its folder, tracks, added and files".into()))?;
        // Which copy its sheet showed, by its folder (RF10): of two in one library, that one, and none other.
        let dir = ask.from_dir.ok_or_else(|| ToggleError::Refused("Say which copy Replace's sheet showed: its folder".into()))?;
        let offer = self.own_offer(source, album, &own, ask.to, Some(dir)).await?;
        if let Some(why) = offer.refused {
            return Err(ToggleError::Refused(format!("Changed since you looked. {why}.")));
        }
        self.incoming.replace_from_copy(source, offer.copy, offer.main, ask.year, &sheet).await
    }

    /// Delete an album of someone's own music from the Users panel: an administrator's, only there (the owner, 2026-11-04:
    /// "not anywhere else than in a control panel").
    pub async fn delete_own_album(&self, source: &str, album: &str, from: Option<&str>) -> Result<super::DeleteResult, ToggleError> {
        let (_, a) = self.listed_album(source, album, from, None).await?;
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

    /// Move a copy into another used library of its server (decision 0034), the copy its sheet showed where the tap says (`sheet`'s
    /// `from_dir`). A copy (C40) carries its sheet (RF10, the owner's "writes nothing the sheet doesn't show"): refused without one,
    /// and where the copy would now be anything else.
    pub async fn move_album(&self, source: &str, album: &str, from: Option<&str>, to: &str, year: Option<&str>, sheet: Option<super::CopySheet>) -> Result<KeepResult, ToggleError> {
        let (s, a) = self.listed_album(source, album, from, sheet.as_ref().map(|s| s.from_dir.as_str())).await?;
        match self.move_target(source, &s, &a, to).await? {
            (to, true) => {
                let sheet = sheet.ok_or_else(|| ToggleError::Refused("Say what Copy to's sheet showed: its folder, its files and the copy's folder".into()))?;
                self.incoming.copy_album(source, a, &to, year, Some(&sheet)).await
            }
            (to, false) => self.incoming.move_album(source, a, &to, year).await,
        }
    }

    /// What that move would do.
    pub async fn move_preview(&self, source: &str, album: &str, from: Option<&str>, to: &str, year: Option<&str>) -> Result<KeepPreview, ToggleError> {
        let (s, a) = self.listed_album(source, album, from, None).await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::identity::Lacks;

    /// A whole copy of a record: `folder`, its place, its library's kind, its format, when it came.
    fn copy(folder: i64, dir: &str, kind: LibraryKind, format: &str, added: i64) -> Copy {
        let lossless = format == "FLAC";
        Copy {
            folder,
            dir: dir.into(),
            kind,
            order: folder as usize,
            format: Some(format.into()),
            min_kbps: Some(if lossless { 900 } else { 320 }),
            bits: lossless.then_some(16),
            rate: lossless.then_some(44_100),
            short: false,
            added: Some(added),
        }
    }

    /// Catalogue folders: 0 Music, 1 Incoming, 2 Ann's own music, 3 Bob's, 4 Carl's.
    const MUSIC: i64 = 0;
    const ANN: i64 = 2;
    const BOB: i64 = 3;
    const CARL: i64 = 4;

    /// T5.2: a short copy is never copied, by either way of knowing it: Music's 2 of 3 (short by its copies) gives way to Bob's
    /// whole one; a record whose one copy is short by its clean tags (3 files each saying 8) is copied from nowhere.
    #[test]
    fn a_want_never_copies_a_short_copy() {
        use crate::catalogue::identity::{FileNumbers, Judged, lacks};
        let music = Copy { short: true, ..copy(MUSIC, "Salt Meridian/Other Room (2018)", LibraryKind::Main, "MP3", 1) };
        let bob = copy(BOB, "Salt Meridian/Other Room (2018)", LibraryKind::Own, "MP3", 2);
        let judged = |c: &Copy| lacks(&Judged { fewer_than_its_copies: c.short, ..Judged::default() });
        assert_eq!(want_copy(&[(music.clone(), judged(&music)), (bob.clone(), judged(&bob))], Some(ANN), &[]), Chosen::From(BOB, bob.dir.clone()), "Bob's, whole");
        let three_of_eight: Vec<FileNumbers> = (1..=3).map(|t| FileNumbers { disc: Some(1), number: Some(t), total: Some(8) }).collect();
        let lone = copy(MUSIC, "Salt Meridian/Other Room (2018)", LibraryKind::Main, "MP3", 1);
        let short = lacks(&Judged { files: three_of_eight, ..Judged::default() });
        assert_eq!(want_copy(&[(lone, short)], Some(ANN), &[]), Chosen::NoneWhole, "short by its tags, though `copies.short` says whole");
    }

    /// T5.3: a main library's whole copy first, then the asker's own (none made: they have it), then anyone's, between Bob's and
    /// Carl's as `copy_for` orders them (the first added), whatever order they are listed in.
    #[test]
    fn a_want_copies_from_a_main_library_then_the_askers_then_anyones() {
        let at = "Salt Meridian/Other Room (2018)";
        let whole = |c: Copy| (c, Lacks::Whole);
        let (music, bob) = (copy(MUSIC, at, LibraryKind::Main, "MP3", 5), copy(BOB, at, LibraryKind::Own, "FLAC", 1));
        assert_eq!(want_copy(&[whole(bob.clone()), whole(music.clone())], Some(ANN), &[]), Chosen::From(MUSIC, at.into()), "Music's, though Bob's is FLAC");
        let ann = copy(ANN, at, LibraryKind::Own, "MP3", 3);
        assert_eq!(want_copy(&[whole(ann.clone()), whole(bob.clone())], Some(ANN), &[]), Chosen::Theirs, "Ann has it: none made");
        let carl = copy(CARL, at, LibraryKind::Own, "MP3", 0);
        assert_eq!(want_copy(&[whole(bob.clone()), whole(carl.clone())], Some(ANN), &[]), Chosen::From(CARL, at.into()), "Carl's, added first");
        assert_eq!(want_copy(&[whole(carl), whole(bob)], Some(ANN), &[]), Chosen::From(CARL, at.into()), "whatever the order listed");
    }

    /// T5.4: of two folders in one library, the folder chosen (the FLAC, as `copy_for` chooses) is the one copied, named by its folder
    /// and place, never the MP3 because it is listed first; the one record's listing names that copy alone.
    #[test]
    fn a_want_copies_the_folder_it_chose() {
        let (mp3, flac) = (copy(MUSIC, "A/Album (MP3)", LibraryKind::Main, "MP3", 1), copy(MUSIC, "A/Album (FLAC)", LibraryKind::Main, "FLAC", 2));
        let chosen = want_copy(&[(mp3, Lacks::Whole), (flac, Lacks::Whole)], Some(ANN), &[]);
        assert_eq!(chosen, Chosen::From(MUSIC, "A/Album (FLAC)".into()));
        let Chosen::From(folder, dir) = chosen else { unreachable!() };
        let listed = copy_listed(album("record-7", "/music", "/music/A/Album (MP3)"), "/music", (folder, dir));
        assert_eq!(listed.copies.len(), 1);
        assert_eq!((listed.copies[0].folder, listed.copies[0].dir.as_str(), listed.copies[0].library.as_str()), (MUSIC, "A/Album (FLAC)", "/music"));
        assert_eq!(listed.path.as_deref(), Some("/music/A/Album (FLAC)"), "the FLAC's folder, not the card's");
        assert_eq!(listed.id, "record-7");
    }

    /// T5.6: Bob's own music switched off (or parked, where it has no copy listed at all) is copied from by no one: the Want goes
    /// to the fetchers; with Music's copy too, Music's.
    #[test]
    fn a_want_never_copies_from_music_switched_off() {
        let at = "Salt Meridian/Other Room (2018)";
        let bob = (copy(BOB, at, LibraryKind::Own, "FLAC", 1), Lacks::Whole);
        assert_eq!(want_copy(std::slice::from_ref(&bob), Some(ANN), &[BOB]), Chosen::NoneWhole);
        assert_eq!(want_copy(&[], Some(ANN), &[]), Chosen::NoneWhole, "parked: nothing listed");
        let music = (copy(MUSIC, at, LibraryKind::Main, "MP3", 2), Lacks::Unknown);
        assert_eq!(want_copy(&[bob, music], Some(ANN), &[BOB]), Chosen::From(MUSIC, at.into()), "Unknown copies as whole");
    }

    /// T5.5: a Want's copy reads the one record's listing (`albums_by_id`, `copies_of` through `judged_copies`), never the whole
    /// Albums view, so a bulk approve of many Wants builds that view for none of them; and it is named by place, not by library.
    #[test]
    fn a_bulk_approve_reads_one_listing_per_want() {
        let source = include_str!("albums.rs").replace("\r\n", "\n");
        let at = source.find("pub(super) async fn copy_for").unwrap();
        let body = &source[at..at + source[at..].find("\n    }\n").unwrap()];
        assert!(!body.contains("self.albums(") && !body.contains("listed_album(") && !body.contains("named_copy("), "{body}");
        assert!(body.contains("albums_by_id") && body.contains("judged_copies") && body.contains("copy_listed"), "{body}");
    }

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

    /// T13.3 (run 10's review, the nits): an administrator's Copy from someone's own music into an Incoming library is refused in
    /// `move_refused`, whoever's music it is (a departed account's too); a copy into a main library or own music is not. Both doors
    /// ask `move_target`, which refuses, before anything is copied: nothing is written (the end-to-end test looks on disk).
    #[test]
    fn a_copy_into_incoming_is_refused() {
        use LibraryKind::{Incoming, Main, Own};
        for left in [false, true] {
            assert!(copies(Some(Own), Some(Incoming), false, left), "it would be a copy (left {left})");
            assert!(move_refused(Some(Own), Some(Incoming), true, false, false, false, left).is_some(), "refused (left {left})");
        }
        assert_eq!(move_refused(Some(Own), Some(Main), true, false, false, false, false), None, "into a main library: a copy still");
        assert_eq!(move_refused(Some(Own), Some(Own), true, false, false, false, false), None, "into someone's own music: a copy still");
        let source = include_str!("albums.rs").replace("\r\n", "\n");
        for door in ["pub async fn move_album(", "pub async fn move_preview("] {
            let at = source.find(door).unwrap();
            let body = &source[at..at + source[at..].find("\n    }\n").unwrap()];
            let (asked, copied) = (body.find("self.move_target(source, &s, &a, to).await?").unwrap(), body.find("self.incoming.copy_").unwrap());
            assert!(asked < copied, "refused before anything is copied: {body}");
        }
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

    /// C36: the page reads what each copy allows on the copy alone: another server's album is sent its one copy, made of the
    /// lists by id, which stay the server's; Cratarr's own records are sent no library of their own, their copies saying it.
    #[test]
    fn the_page_reads_each_copy_alone() {
        let source = |id: &str, albums: Vec<source::Album>| AlbumsSourceView {
            id: id.into(),
            label: id.into(),
            supported: true,
            albums,
            error: None,
            as_of: 0,
            libraries: Vec::new(),
            library_info: Vec::new(),
            lacks: [("j1".to_string(), vec!["cover"])].into(),
            deletable: vec!["j1".into()],
            keepable: vec!["j1".into()],
            keep_to: [("j1".to_string(), "music".to_string())].into(),
        };
        let view = AlbumsView { sources: vec![source("cratarr", vec![listed("al-1", 2, "incoming", "FLAC")]), source("jf", vec![album("j1", "new", "/data/new/A/Album")])], trash_days: 30, music: None };
        let view = view.as_sent();
        assert_eq!(view.sources[0].albums[0].library, None, "Cratarr's own: its copies say where");
        let c = &view.sources[1].albums[0].copies;
        assert_eq!(c.len(), 1, "another server's album: its one copy");
        assert_eq!((c[0].library.as_str(), c[0].deletable, c[0].keepable, c[0].keep_to.as_deref(), c[0].lacks.clone(), c[0].plays), ("new", true, true, Some("music"), vec!["cover".to_string()], true));
        let sent = serde_json::to_value(&view).unwrap();
        for field in ["deletable", "keepable", "keep_to", "lacks"] {
            assert!(sent["sources"][1].get(field).is_none(), "{field} is each copy's to say");
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
    /// RY01, the Library's list: of a record in Music and in Bob's and Ann's own music, Bob's FLAC, the best copy better than
    /// Music's MP3, says Replace in Music may take its place, naming Music's copy and why; Ann's MP3, as good as Music's, says
    /// nothing, and neither does Music's own copy. Bob's switched off: no copy says it.
    #[test]
    fn the_best_own_copy_says_which_main_copy_it_would_replace() {
        let at = |f: i64| (f, "A/Album".to_string());
        let copy = |folder: i64, kind: LibraryKind, format: &str, kbps: i64, bits: Option<i64>| Copy {
            folder,
            dir: "A/Album".into(),
            kind,
            order: folder as usize,
            format: Some(format.into()),
            min_kbps: Some(kbps),
            bits,
            rate: bits.map(|_| 44_100),
            short: false,
            added: None,
        };
        let copies = [copy(0, LibraryKind::Main, "MP3", 320, None), copy(2, LibraryKind::Own, "MP3", 320, None), copy(3, LibraryKind::Own, "FLAC", 900, Some(16))];
        let order = PlayOrder::of(&copies, None, &[]).offering(&copies, &[]);
        assert_eq!(order.replace_into.iter().map(|s| (s.at.clone(), s.kept.clone())).collect::<Vec<_>>(), [(at(3), at(0))]);
        let folded = fold(vec![listed("al-1", 0, "music", "MP3"), listed("al-1", 2, "ann", "MP3"), listed("al-1", 3, "bob", "FLAC")], &HashMap::from([("al-1".to_string(), order)]));
        let said: Vec<Vec<(String, String)>> = folded[0].copies.iter().map(|c| c.replace_into.iter().map(|a| (a.library.clone(), a.why.clone())).collect()).collect();
        let bobs = said[folded[0].copies.iter().position(|c| c.library == "bob").unwrap()].clone();
        assert_eq!(bobs, [("music".to_string(), "lossless, yours is 320 kbps".to_string())], "{said:?}");
        assert_eq!(said.iter().filter(|s| !s.is_empty()).count(), 1, "one copy says it, once: {said:?}");
        assert!(PlayOrder::of(&copies, None, &[]).offering(&copies, &[3]).replace_into.is_empty(), "Bob's music switched off");
    }

    #[test]
    fn an_incoming_copy_says_how_it_stands_against_the_kept_one() {
        let at = |f: i64| (f, "A/Album".to_string());
        let stand = Stand { at: at(2), better: true, replaces: true, kept: at(0), why: "lossless, yours is 320 kbps".into() };
        let order = HashMap::from([("al-1".to_string(), PlayOrder { places: vec![at(0), at(2)], plays: Some(at(0)), against: vec![stand], replace_into: Vec::new() })]);
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
        let copy = |from: &str| named_copy(&folded[0], Some(from), None).unwrap();
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
        let named = named_copy(&folded[0], Some("incoming"), None).unwrap();
        assert_eq!((named.library.as_deref(), named.path.as_deref()), (Some("incoming"), Some("incoming/A/Album")));
        assert_eq!(named.copies.iter().map(|c| (c.folder, c.library.as_str())).collect::<Vec<_>>(), [(2, "incoming")], "and nothing of the other");
        assert!(matches!(named_copy(&folded[0], Some("second"), None), Err(ToggleError::Refused(_))), "no copy there");
    }

    /// Left to the copy that plays, an administrator's Delete would land on whatever plays for them today.
    #[test]
    fn a_record_of_several_copies_must_be_told_which() {
        let folded = fold(vec![listed("al-1", 0, "music", "MP3"), listed("al-1", 2, "incoming", "FLAC")], &order(&[0, 2], 0));
        assert!(matches!(named_copy(&folded[0], None, None), Err(ToggleError::Refused(_))));
        assert_eq!(named_copy(&listed("al-2", 0, "music", "FLAC"), None, None).unwrap().copies[0].folder, 0, "with one, there is nothing to tell");
        let other = album("x", "jellyfin-music", "/m/x");
        assert!(named_copy(&other, None, None).is_ok() && named_copy(&other, Some("jellyfin-music"), None).is_ok());
        assert!(named_copy(&other, Some("music"), None).is_err(), "another server's album is its one library's");
    }

    /// Q2: two folders of one library are two copies, and naming the library names the first as the record's copies stand.
    #[test]
    fn of_two_folders_of_one_library_the_first_is_named() {
        let mut other = listed("al-1", 0, "music", "FLAC");
        other.copies[0].dir = "B/Album".into();
        let order = HashMap::from([("al-1".to_string(), PlayOrder { places: vec![(0, "B/Album".to_string()), (0, "A/Album".to_string())], plays: Some((0, "B/Album".to_string())), ..Default::default() })]);
        let folded = fold(vec![listed("al-1", 0, "music", "MP3"), other], &order);
        assert_eq!(named_copy(&folded[0], Some("music"), None).unwrap().copies[0].dir, "B/Album");
    }

    /// RF10 (the review of runs 11 and 12, RA9): a tap names its copy by the folder its preview showed, so of two folders of one
    /// library it takes the one named, and refuses a folder no copy is at now; with none named, the first, as before (Q2).
    #[test]
    fn a_tap_names_its_copy_by_the_folder_its_sheet_showed() {
        let mut other = listed("al-1", 0, "music", "FLAC");
        other.copies[0].dir = "B/Album".into();
        let order = HashMap::from([("al-1".to_string(), PlayOrder { places: vec![(0, "B/Album".to_string()), (0, "A/Album".to_string())], plays: Some((0, "B/Album".to_string())), ..Default::default() })]);
        let folded = fold(vec![listed("al-1", 0, "music", "MP3"), other], &order);
        assert_eq!(named_copy(&folded[0], Some("music"), Some("A/Album")).unwrap().copies[0].dir, "A/Album", "the one its sheet showed");
        assert_eq!(named_copy(&folded[0], Some("music"), Some("B/Album")).unwrap().copies[0].dir, "B/Album");
        assert!(named_copy(&folded[0], Some("music"), Some("C/Album")).is_err(), "no copy there now: refused");
        assert_eq!(named_copy(&folded[0], Some("music"), None).unwrap().copies[0].dir, "B/Album", "none named: the first");
    }

    /// RF10 (RA9): Bob's own music holds the record twice, an MP3 listed first and a FLAC that beats Music's MP3. Asked by his library
    /// alone, Replace takes the FLAC, the copy it offers (not the first there, which every preview and tap then refused); a tap takes
    /// the copy at the folder it names, and one naming a folder no copy is at takes none.
    #[test]
    fn replace_from_own_music_takes_the_copy_offered_or_the_one_named() {
        let music = copy(MUSIC, "A/Album", LibraryKind::Main, "MP3", 1);
        let mp3 = copy(BOB, "A/Album", LibraryKind::Own, "MP3", 2);
        let flac = copy(BOB, "A/Album (FLAC)", LibraryKind::Own, "FLAC", 3);
        let copies = [music.clone(), mp3.clone(), flac.clone()];
        let mains = [&copies[0]];
        assert_eq!(own_named(&copies, BOB, None, &mains, &[]), Some(&flac), "the copy offered");
        assert_eq!(own_named(&copies, BOB, Some("A/Album (FLAC)"), &mains, &[]), Some(&flac));
        assert_eq!(own_named(&copies, BOB, Some("A/Album"), &mains, &[]), Some(&mp3), "the one its tap names");
        assert_eq!(own_named(&copies, BOB, Some("A/Gone"), &mains, &[]), None, "none at that folder");
        assert_eq!(own_named(&copies, ANN, None, &mains, &[]), None, "none in that library");
        assert_eq!(own_named(&[music.clone(), mp3.clone()], BOB, None, &mains, &[]), Some(&mp3), "none offered: the first there");
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
        let named = named_copy(&folded[0], Some("/own/ann"), None).unwrap();
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
}
