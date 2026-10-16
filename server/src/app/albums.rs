//! The Library page's one list (decision 0034): every album of the followed server's used libraries, each with its
//! library, and what holds against the other libraries. Made of Collection's lists (Music) and Incoming's, which keep
//! and refresh themselves; this only puts them together.

use std::collections::BTreeMap;

use serde::Serialize;

use super::App;
use super::incoming::VerdictView;
use super::library::epoch_now;
use crate::settings::Format;
use crate::source;

/// A used library of a server, as the page shows it.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryView {
    pub id: String,
    /// As its server calls it.
    pub name: String,
    /// "music" or "incoming" (decision 0009).
    #[serde(rename = "use")]
    pub use_: &'static str,
    /// The dot on its albums; none for the primary (Music).
    pub colour: Option<String>,
    /// What a move into it does to the files.
    pub format: Format,
}

/// A Music album that has an album of another library matched to it (the inverse of an Incoming album's verdict).
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
    /// Per album id of a library other than Music: the Music album it upgrades or duplicates.
    pub verdicts: BTreeMap<String, VerdictView>,
    /// Per Music album id: an album of another library that upgrades or duplicates it.
    pub waiting: BTreeMap<String, Waiting>,
    /// The albums Cratarr can delete.
    pub deletable: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AlbumsView {
    pub sources: Vec<AlbumsSourceView>,
    /// How long a deleted album stays in the trash.
    pub trash_days: u32,
    /// The Music library's name, as its server calls it.
    pub music: Option<String>,
}

impl App {
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
        let sources = ids
            .into_iter()
            .map(|id| {
                let c = collection.sources.iter().find(|s| s.id == id);
                let i = incoming.sources.iter().find(|s| s.id == id);
                let active = live.iter().find(|(a, _)| a.cfg.id == id).map(|(a, _)| a.clone());
                let libraries: Vec<String> = c.iter().flat_map(|c| c.libraries.clone()).chain(i.iter().flat_map(|i| i.libraries.clone())).collect();
                let library_info = active
                    .as_ref()
                    .map(|a| {
                        let roles = &a.cfg.libraries;
                        libraries
                            .iter()
                            .map(|l| LibraryView {
                                id: l.clone(),
                                name: a.placement.libraries.iter().find(|p| &p.id == l).map_or_else(|| l.clone(), |p| p.name.clone()),
                                use_: if roles.music.contains(l) { "music" } else { "incoming" },
                                colour: roles.colour_of(l),
                                format: roles.format_of(l),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let music_albums: &[source::Album] = c.map_or(&[], |c| c.albums.as_slice());
                let verdicts = i.map(|i| i.verdicts.clone()).unwrap_or_default();
                AlbumsSourceView {
                    label: c.map(|c| c.label.clone()).or_else(|| i.map(|i| i.label.clone())).unwrap_or_else(|| id.clone()),
                    supported: c.is_some_and(|c| c.supported) || i.is_some_and(|i| i.supported),
                    albums: music_albums.iter().cloned().chain(i.iter().flat_map(|i| i.albums.iter().cloned())).collect(),
                    error: c.and_then(|c| c.error.clone()).or_else(|| i.and_then(|i| i.error.clone())),
                    as_of: [c.map(|c| c.as_of), i.map(|i| i.as_of)].into_iter().flatten().min().unwrap_or_else(epoch_now),
                    waiting: waiting(&verdicts, music_albums),
                    verdicts,
                    deletable: i.map(|i| i.deletable.clone()).unwrap_or_default(),
                    libraries,
                    library_info,
                    id,
                }
            })
            .collect();
        AlbumsView { sources, trash_days: incoming.trash_days, music: incoming.music }
    }
}

/// The verdicts the other way round: per Music album (found by its folder, the end of its path), the album that
/// upgrades or duplicates it; an upgrade wins over a duplicate.
fn waiting(verdicts: &BTreeMap<String, VerdictView>, music: &[source::Album]) -> BTreeMap<String, Waiting> {
    let mut out: BTreeMap<String, Waiting> = BTreeMap::new();
    for (other, v) in verdicts {
        let dir = format!("/{}", v.library.trim_matches('/'));
        let Some(m) = music.iter().filter(|a| a.path.as_deref().is_some_and(|p| p.trim_end_matches('/').ends_with(&dir))).max_by_key(|a| a.path.as_ref().map_or(0, String::len)) else {
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

    fn album(id: &str, path: &str) -> source::Album {
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
            tags: Vec::new(),
            library: None,
        }
    }

    #[test]
    fn a_music_album_knows_what_waits_for_it() {
        let music = [album("m1", "/data/media/Music/Arp/Zebra"), album("m2", "/data/media/Music/Ott/Heads (2022)")];
        let mut verdicts = BTreeMap::new();
        verdicts.insert("i1".to_string(), VerdictView { kind: "duplicate", library: "Arp/Zebra".into(), why: String::new() });
        verdicts.insert("i2".to_string(), VerdictView { kind: "upgrade", library: "Arp/Zebra".into(), why: String::new() });
        verdicts.insert("i3".to_string(), VerdictView { kind: "upgrade", library: "Nobody/Here".into(), why: String::new() });
        let w = waiting(&verdicts, &music);
        assert_eq!(w.len(), 1);
        assert_eq!((w["m1"].kind, w["m1"].album.as_str()), ("upgrade", "i2"));
    }
}
