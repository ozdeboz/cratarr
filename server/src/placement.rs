//! Where a source's music lies in the playlist folder. Sources report track paths as they see them (Jellyfin, for
//! one, reports `/data/media/Music/...` while its library settings say `/Volume1/media/Music`), so no configured
//! prefix is trusted. Instead the core strips leading folders off a sample track's path until what is left is a file
//! in the playlist folder. What was stripped is the playlist folder as that source sees it (its "root").
//!
//! A library is usable when its sample tracks lie under that root. A library elsewhere (an incoming folder, say) can
//! never be added to playlists: its tracks have no path relative to the playlist files.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::source::{Library, Source};

/// What a source's libraries are used for. Library ids are the source's own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Roles {
    /// The curated music: tracks here can go in playlists. Empty = every library inside the playlist folder.
    #[serde(default)]
    pub music: Vec<String>,
    /// Optional: where new music waits for review (e.g. Lidarr's sandbox). Its tracks never go in playlists; its
    /// presence turns on incoming-music features.
    #[serde(default)]
    pub incoming: Option<String>,
}

/// Samples per library: enough to survive one odd file, few enough to stay cheap.
const SAMPLES: usize = 3;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Placement {
    /// The playlist folder as this source sees it, ending in `/`; `None` until a sample track was found in it.
    pub root: Option<String>,
    pub libraries: Vec<PlacedLibrary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlacedLibrary {
    pub id: String,
    pub name: String,
    /// Inside the playlist folder, so its tracks can go in playlists.
    pub inside: bool,
}

impl Placement {
    /// The music libraries in use: the chosen ones that are inside, or every inside one when none were chosen.
    /// The incoming library never counts, wherever it lies.
    pub fn usable<'a>(&'a self, roles: &'a Roles) -> impl Iterator<Item = &'a PlacedLibrary> + 'a {
        self.libraries.iter().filter(move |l| {
            l.inside && roles.incoming.as_deref() != Some(l.id.as_str()) && (roles.music.is_empty() || roles.music.contains(&l.id))
        })
    }

    /// The playlist line for a track, if its library is in use and its path lies under the root.
    pub fn entry(&self, roles: &Roles, library_id: Option<&str>, path: Option<&str>) -> Option<String> {
        let lib = library_id?;
        if !self.usable(roles).any(|l| l.id == lib) {
            return None;
        }
        let root = self.root.as_deref()?;
        crate::m3u::library_entry(path?, &[root.to_string()]).filter(|rel| safe_relative(rel))
    }

    /// A playlist file as the source sees it.
    pub fn playlist_path(&self, name: &str) -> Option<String> {
        self.root.as_ref().map(|r| format!("{r}{name}.m3u"))
    }
}

/// Look at every music library of the source. Errors from the source fail the whole placement; a library whose
/// samples cannot be read is simply not inside.
pub async fn place(source: &dyn Source, playlist_dir: &Path) -> Result<Placement> {
    let libraries: Vec<Library> = source.libraries().await?;
    let mut root: Option<String> = None;
    let mut placed = Vec::with_capacity(libraries.len());
    for lib in libraries {
        let samples = source.sample_paths(&lib.id, SAMPLES).await.unwrap_or_else(|e| {
            tracing::warn!("cannot sample library {:?}: {e:#}", lib.name);
            Vec::new()
        });
        let mut inside = false;
        for path in &samples {
            let found = match &root {
                Some(r) => under_root(playlist_dir, r, path),
                None => match find_root(playlist_dir, path) {
                    Some(r) => {
                        root = Some(r);
                        true
                    }
                    None => false,
                },
            };
            if found {
                inside = true;
                break;
            }
        }
        placed.push(PlacedLibrary { id: lib.id, name: lib.name, inside });
    }
    Ok(Placement { root, libraries: placed })
}

fn segments(path: &str) -> Vec<&str> {
    path.split(['/', '\\']).filter(|s| !s.is_empty()).collect()
}

/// No `..`, `.`, NUL or drive letters: the relative path can only point inside the playlist folder.
pub fn safe_relative(rel: &str) -> bool {
    !rel.is_empty()
        && !rel.starts_with('/')
        && !rel.contains('\0')
        && rel.split('/').all(|s| !s.is_empty() && s != "." && s != ".." && !s.contains(':'))
}

/// The root (with a trailing `/`) that makes `path` a file in `playlist_dir`, trying the longest remainder first.
pub fn find_root(playlist_dir: &Path, path: &str) -> Option<String> {
    let segs = segments(path);
    let absolute = path.starts_with('/') || path.starts_with('\\');
    for i in 0..segs.len().saturating_sub(1) {
        let rel = segs[i..].join("/");
        if safe_relative(&rel) && playlist_dir.join(&rel).is_file() {
            let lead = if absolute { "/" } else { "" };
            let mut root = format!("{lead}{}", segs[..i].join("/"));
            if !root.ends_with('/') {
                root.push('/');
            }
            return Some(root);
        }
    }
    None
}

fn under_root(playlist_dir: &Path, root: &str, path: &str) -> bool {
    crate::m3u::library_entry(path, &[root.to_string()])
        .is_some_and(|rel| safe_relative(&rel) && playlist_dir.join(rel).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Fut, Playing, PlaylistEvent, Status, Track};
    use std::collections::BTreeMap;

    fn music() -> std::path::PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("cratarr-place-{}-{n}", std::process::id()));
        std::fs::create_dir_all(d.join("Artist - Album (2020)")).unwrap();
        std::fs::write(d.join("Artist - Album (2020)/01 Song.mp3"), b"x").unwrap();
        std::fs::create_dir_all(d.join("Music/Nested")).unwrap();
        std::fs::write(d.join("Music/Nested/02 Other.mp3"), b"x").unwrap();
        d
    }

    #[test]
    fn the_root_is_what_must_be_stripped_to_reach_a_file() {
        let d = music();
        assert_eq!(find_root(&d, "/data/media/Music/Artist - Album (2020)/01 Song.mp3").as_deref(), Some("/data/media/Music/"));
        assert_eq!(find_root(&d, r"D:\Music\Artist - Album (2020)\01 Song.mp3").as_deref(), Some("D:/Music/"));
        // A folder inside the library named like the library: the longest remainder that exists wins.
        assert_eq!(find_root(&d, "/srv/Music/Music/Nested/02 Other.mp3").as_deref(), Some("/srv/Music/"));
        assert_eq!(find_root(&d, "/data/media/lidarr-music/X/01 Song.mp3"), None);
        assert_eq!(find_root(&d, "/nothing/here.mp3"), None);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn unsafe_relative_paths_are_refused() {
        for bad in ["", "/a", "a/../b", "./a", "a//b", "C:/x", "a\0b", ".."] {
            assert!(!safe_relative(bad), "{bad:?}");
        }
        assert!(safe_relative("Artist - Album (2020)/01 Song.mp3"));
    }

    fn placement() -> Placement {
        Placement {
            root: Some("/data/media/Music/".into()),
            libraries: vec![
                PlacedLibrary { id: "music".into(), name: "Music".into(), inside: true },
                PlacedLibrary { id: "more".into(), name: "More".into(), inside: true },
                PlacedLibrary { id: "incoming".into(), name: "Incoming".into(), inside: false },
            ],
        }
    }

    fn roles(music: &[&str], incoming: Option<&str>) -> Roles {
        Roles { music: music.iter().map(|s| s.to_string()).collect(), incoming: incoming.map(str::to_string) }
    }

    #[test]
    fn entries_need_a_usable_library_and_a_path_under_the_root() {
        let p = placement();
        let auto = Roles::default();
        let path = Some("/data/media/Music/A/01.mp3");
        assert_eq!(p.entry(&auto, Some("music"), path).as_deref(), Some("A/01.mp3"));
        assert_eq!(p.entry(&auto, Some("incoming"), path), None, "outside libraries never count");
        assert_eq!(p.entry(&roles(&["more"], None), Some("music"), path), None, "only the chosen libraries");
        assert_eq!(p.entry(&roles(&["incoming"], None), Some("incoming"), path), None, "choosing an outside library does not help");
        assert_eq!(p.entry(&roles(&[], Some("more")), Some("more"), path), None, "the incoming role is never addable, even inside");
        assert_eq!(p.entry(&roles(&[], Some("more")), Some("music"), path).as_deref(), Some("A/01.mp3"));
        assert_eq!(p.entry(&auto, None, path), None);
        assert_eq!(p.entry(&auto, Some("music"), Some("/elsewhere/A/01.mp3")), None);
        assert_eq!(p.entry(&auto, Some("music"), Some("/data/media/Music/A/../../x.mp3")), None);
        assert_eq!(Placement::default().entry(&auto, Some("music"), path), None, "unknown root");
        assert_eq!(p.playlist_path("Chill").as_deref(), Some("/data/media/Music/Chill.m3u"));
    }

    /// A source with fixed libraries and samples.
    struct Fake(Vec<(Library, Vec<String>)>);

    impl Source for Fake {
        fn status(&self) -> Fut<'_, Status> {
            Box::pin(async { Ok(Status::default()) })
        }
        fn options(&self) -> Fut<'_, BTreeMap<String, Vec<crate::source::Choice>>> {
            Box::pin(async { Ok(BTreeMap::new()) })
        }
        fn libraries(&self) -> Fut<'_, Vec<Library>> {
            Box::pin(async { Ok(self.0.iter().map(|(l, _)| l.clone()).collect()) })
        }
        fn sample_paths<'a>(&'a self, id: &'a str, n: usize) -> Fut<'a, Vec<String>> {
            Box::pin(async move { Ok(self.0.iter().find(|(l, _)| l.id == id).map(|(_, s)| s.iter().take(n).cloned().collect()).unwrap_or_default()) })
        }
        fn now_playing(&self) -> Fut<'_, Option<Playing>> {
            Box::pin(async { Ok(None) })
        }
        fn track<'a>(&'a self, _id: &'a str) -> Fut<'a, Track> {
            Box::pin(async { anyhow::bail!("no tracks") })
        }
        fn playlists_changed(&self, _e: Vec<PlaylistEvent>) -> Fut<'_, ()> {
            Box::pin(async { Ok(()) })
        }
    }

    fn lib(id: &str) -> Library {
        Library { id: id.into(), name: id.to_uppercase() }
    }

    #[tokio::test]
    async fn libraries_inside_the_playlist_folder_are_found() {
        let d = music();
        let src = Fake(vec![
            (lib("incoming"), vec!["/data/media/lidarr-music/A/B/01.flac".into()]),
            (lib("music"), vec!["/data/media/Music/missing.mp3".into(), "/data/media/Music/Artist - Album (2020)/01 Song.mp3".into()]),
            (lib("empty"), vec![]),
        ]);
        let p = place(&src, &d).await.unwrap();
        assert_eq!(p.root.as_deref(), Some("/data/media/Music/"));
        let inside: Vec<_> = p.libraries.iter().map(|l| (l.id.as_str(), l.inside)).collect();
        assert_eq!(inside, vec![("incoming", false), ("music", true), ("empty", false)]);
        std::fs::remove_dir_all(&d).ok();
    }
}
