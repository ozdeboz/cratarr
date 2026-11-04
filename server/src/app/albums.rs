//! The Library page's one list (decision 0034): every album of the followed server's used libraries, each with its
//! library, and what holds against the other libraries. Made of Collection's lists (Music) and Incoming's, which keep
//! and refresh themselves; this only puts them together.

use std::collections::BTreeMap;

use serde::Serialize;

use super::incoming::VerdictView;
use super::{App, KeepPreview, KeepResult, ToggleError};
use super::library::{Active, epoch_now};
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
/// a main library or their own music. Between other libraries only an administrator moves one, and into or out of own
/// music it would be a copy (the owner, 2026-11-04), which comes with records and copies; but the music of someone who
/// left (`left`) moves into a main library, as the account's page asks (0101 point 7).
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
    if left && to == Some(LibraryKind::Main) {
        return None;
    }
    (from == Some(LibraryKind::Own) || to == Some(LibraryKind::Own) || viewers_own).then_some("A copy into or out of own music isn't here yet")
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
        // Who is looking (0101 point 2): the names, the rights and where Add to goes are theirs.
        let viewer = self.served.as_ref().and_then(|s| s.current_user());
        let admin = crate::serve::access::administering();
        let asked = self.served.as_ref().map(|s| super::requests::requesters(s)).unwrap_or_default();
        let requester = |al: &source::Album| al.artist.as_deref().and_then(|ar| asked.get(&super::requests::requested(ar, &al.title)));
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
                let own_music = active.as_ref().is_some_and(|a| a.cfg.own_music().is_some());
                let keepable: Vec<String> = incoming_albums.iter().filter(|al| admin || (own_music && theirs(al))).map(|al| al.id.clone()).collect();
                let first_main = active.as_ref().and_then(|a| a.main().into_iter().next());
                let keep_to: BTreeMap<String, String> = incoming_albums
                    .iter()
                    .filter(|al| keepable.contains(&al.id))
                    .filter_map(|al| {
                        let a = active.as_ref()?;
                        let set = al.library.as_deref().and_then(|l| a.cfg.libraries.get(l)).and_then(|l| l.keep_to.clone());
                        let to = requester(al).filter(|r| self.account_on(r)).and_then(|r| a.own_of(r)).or(set).or_else(|| first_main.clone())?;
                        Some((al.id.clone(), to))
                    })
                    .collect();
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
                let verdicts = i.map(|i| i.verdicts.clone()).unwrap_or_default();
                let albums: Vec<source::Album> = music_albums.iter().cloned().chain(i.iter().flat_map(|i| i.albums.iter().cloned())).collect();
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
                        .chain(
                            music_albums
                                .iter()
                                .filter(|a| a.library.as_ref().is_some_and(|l| kept_deletable.iter().any(|(s, m)| s == &id && m == l) && may_delete_in(l)))
                                .filter(|a| a.path.is_some())
                                .map(|a| a.id.clone()),
                        )
                        .collect(),
                    keepable,
                    keep_to,
                    libraries,
                    library_info,
                    id,
                }
            })
            .collect();
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

impl App {
    /// A listed album of a server (Music's or another library's), by id, as the viewer sees that server.
    async fn listed_album(&self, source: &str, album: &str) -> Result<(AlbumsSourceView, source::Album), ToggleError> {
        let view = self.albums(Some(source)).await;
        let s = view.sources.into_iter().find(|s| s.id == source).ok_or_else(|| ToggleError::Refused("that server is not connected".into()))?;
        let a = s.albums.iter().find(|a| a.id == album).cloned().ok_or_else(|| ToggleError::Refused("that album is no longer listed".into()))?;
        Ok((s, a))
    }

    /// Delete an album the viewer may (the view's `deletable`), into its library's trash (decision 0034).
    pub async fn delete_album(&self, source: &str, album: &str) -> Result<super::DeleteResult, ToggleError> {
        let (s, a) = self.listed_album(source, album).await?;
        if !s.deletable.contains(&a.id) {
            return Err(ToggleError::Refused("That isn't yours to delete".into()));
        }
        self.incoming.delete_listed(source, a).await
    }

    /// Delete an album of someone's own music from the Users panel: an administrator's, only there (the owner, 2026-11-04:
    /// "not anywhere else than in a control panel").
    pub async fn delete_own_album(&self, source: &str, album: &str) -> Result<super::DeleteResult, ToggleError> {
        let (_, a) = self.listed_album(source, album).await?;
        let live = self.library.live();
        let own = a.library.as_deref().and_then(|l| live.iter().find(|x| x.cfg.id == source)?.kind_of(l)) == Some(LibraryKind::Own);
        if !own {
            return Err(ToggleError::Refused("That is no one's own music".into()));
        }
        self.incoming.delete_listed(source, a).await
    }

    /// Where a move of `a` goes, if the viewer may: `to`, someone's own music made now if this is its first use.
    async fn move_target(&self, source: &str, s: &AlbumsSourceView, a: &source::Album, to: &str) -> Result<String, ToggleError> {
        let live = self.library.live();
        let on = live.iter().find(|x| x.cfg.id == source).ok_or_else(|| ToggleError::Refused("that server is not connected".into()))?;
        let viewer = self.served.as_ref().and_then(|s| s.current_user());
        let viewers_own = viewer.as_ref().and_then(|v| on.own_of(v)).as_deref() == Some(to);
        let default = s.keep_to.get(&a.id).map(String::as_str) == Some(to);
        let from = a.library.as_deref().and_then(|l| on.kind_of(l));
        let owner = on.libraries().into_iter().find(|u| a.library.as_deref() == Some(u.id.as_str())).and_then(|u| u.owner);
        let left = owner.is_some_and(|o| !self.account_on(&o));
        if let Some(why) = move_refused(from, on.kind_of(to), crate::serve::access::administering(), s.keepable.contains(&a.id), default, viewers_own, left) {
            return Err(ToggleError::Refused(why.into()));
        }
        if on.used(to) {
            return Ok(to.to_string());
        }
        let account = std::path::Path::new(to).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let made = self.library.own_library(&account).await.map_err(ToggleError::Upstream)?;
        made.ok_or_else(|| ToggleError::Refused("no own-music folder is set".into()))
    }

    /// Move an album into another used library of its server (decision 0034).
    pub async fn move_album(&self, source: &str, album: &str, to: &str, year: Option<&str>) -> Result<KeepResult, ToggleError> {
        let (s, a) = self.listed_album(source, album).await?;
        let to = self.move_target(source, &s, &a, to).await?;
        self.incoming.move_album(source, a, &to, year).await
    }

    /// What that move would do.
    pub async fn move_preview(&self, source: &str, album: &str, to: &str, year: Option<&str>) -> Result<KeepPreview, ToggleError> {
        let (s, a) = self.listed_album(source, album).await?;
        let to = self.move_target(source, &s, &a, to).await?;
        self.incoming.move_preview(source, a, &to, year).await
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
        // Into or out of own music it would be a copy.
        assert!(move_refused(Some(Main), Some(Own), true, false, false, false, false).is_some());
        assert!(move_refused(Some(Main), None, true, false, false, true, false).is_some());
        assert!(move_refused(Some(Own), Some(Main), true, false, false, false, false).is_some());
        // The music of someone who left moves into a main library, by an administrator.
        assert_eq!(move_refused(Some(Own), Some(Main), true, false, false, false, true), None);
        assert!(move_refused(Some(Own), Some(Main), false, false, false, false, true).is_some());
        assert!(move_refused(Some(Own), Some(Own), true, false, false, false, true).is_some());
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
