//! Which folders a server's music lies in (decision 0009). Servers report track paths as they see them (Jellyfin,
//! for one, reports `/data/media/Music/...` while Cratarr sees `/music/...`), so no configured prefix is trusted.
//! Instead the core strips leading folders off a few sample tracks of each library until what is left is a file in
//! one of Cratarr's folders. What was stripped is that folder as the server sees it (its "root").
//!
//! The uses (Music, Incoming) are set on libraries; the folders behind them are found here. A track can go in
//! playlists when its path lies under the Music folder's root.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::source::{Library, Source};

/// Samples per library: enough to survive one odd file, few enough to stay cheap.
const SAMPLES: usize = 3;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Placement {
    /// Each folder the server's tracks were found in, with the server's own path for it.
    pub roots: Vec<FolderRoot>,
    pub libraries: Vec<PlacedLibrary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FolderRoot {
    /// As Cratarr sees it: "/music".
    pub folder: String,
    /// As the server sees it, ending in `/`: "/data/media/Music/".
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlacedLibrary {
    pub id: String,
    pub name: String,
    /// Cratarr's folders its sample tracks were found in; empty when Cratarr cannot see its files.
    pub folders: Vec<String>,
    /// Its folders as the server describes them (to say what to mount).
    pub paths: Vec<String>,
    /// Cratarr sees its files but cannot write there (filled in for the Settings page, which says so).
    pub read_only: bool,
}

impl Placement {
    /// The server's path for one of Cratarr's folders, if its tracks were found there.
    pub fn root(&self, folder: &str) -> Option<&str> {
        self.roots.iter().find(|r| r.folder == folder).map(|r| r.root.as_str())
    }

    /// The libraries found in a folder.
    pub fn libraries_in<'a>(&'a self, folder: &'a str) -> impl Iterator<Item = &'a PlacedLibrary> + 'a {
        self.libraries.iter().filter(move |l| l.folders.iter().any(|f| f == folder))
    }

    /// The playlist line for a track: its path relative to the Music folder, if it lies there.
    pub fn entry(&self, music: &str, path: Option<&str>) -> Option<String> {
        let root = self.root(music)?;
        crate::m3u::library_entry(path?, &[root.to_string()]).filter(|rel| safe_relative(rel))
    }

    /// A playlist file in the Music folder, as the server sees it.
    pub fn playlist_path(&self, music: &str, name: &str) -> Option<String> {
        self.root(music).map(|r| format!("{r}{name}.m3u"))
    }
}

/// Look at every music library of the server against `folders` (the mounted ones). Errors from the server fail the
/// whole placement; a library whose samples cannot be read is simply found nowhere.
///
/// A mount may be a library's own folder (`/music`) or a parent of several (`/media`): the library's own folder
/// inside it (`/media/Music`) comes from the folders the server reports for the library (`library_roots`).
pub async fn place(source: &dyn Source, folders: &[String]) -> Result<Placement> {
    let libraries: Vec<Library> = source.libraries().await?;
    // Each mount the server's tracks were found in, with the server's path for it.
    let mut mounts: Vec<FolderRoot> = Vec::new();
    let mut roots: Vec<FolderRoot> = Vec::new();
    let mut placed = Vec::with_capacity(libraries.len());
    for lib in libraries {
        let samples = source.sample_paths(&lib.id, SAMPLES).await.unwrap_or_else(|e| {
            tracing::warn!("cannot sample library {:?}: {e:#}", lib.name);
            Vec::new()
        });
        let own: Vec<String> = source
            .library_roots(&lib.id)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("cannot read the folders of library {:?}: {e:#}", lib.name);
                Vec::new()
            })
            .into_iter()
            .map(|r| format!("{}/", r.replace('\\', "/").trim_end_matches('/')))
            .collect();
        let mut found: Vec<String> = Vec::new();
        for path in &samples {
            let path = path.replace('\\', "/");
            for folder in folders {
                let dir = Path::new(folder);
                let prefix = match mounts.iter().find(|r| &r.folder == folder) {
                    Some(r) => under_root(dir, &r.root, &path).then(|| r.root.clone()),
                    None => find_root(dir, &path).inspect(|root| mounts.push(FolderRoot { folder: folder.clone(), root: root.clone() })),
                };
                let Some(prefix) = prefix else { continue };
                let here = own_folder(folder, &prefix, &own, &path).unwrap_or(FolderRoot { folder: folder.clone(), root: prefix });
                if !found.contains(&here.folder) {
                    found.push(here.folder.clone());
                }
                if !roots.contains(&here) {
                    roots.push(here);
                }
            }
        }
        placed.push(PlacedLibrary { id: lib.id, name: lib.name, folders: found, paths: lib.paths, read_only: false });
    }
    Ok(Placement { roots, libraries: placed })
}

/// The library's own folder that holds `path`, inside `mount` (whose path on the server is `prefix`): the server's
/// folder for the library with `prefix` swapped for the mount. `None` when the server named none that fits.
fn own_folder(mount: &str, prefix: &str, own: &[String], path: &str) -> Option<FolderRoot> {
    own.iter().filter(|r| path.starts_with(r.as_str())).find_map(|r| {
        let sub = r.strip_prefix(prefix)?.trim_end_matches('/');
        let folder = if sub.is_empty() {
            mount.to_string()
        } else {
            if !safe_relative(sub) {
                return None;
            }
            format!("{}/{sub}", mount.trim_end_matches('/'))
        };
        Path::new(&folder).is_dir().then(|| FolderRoot { folder, root: r.clone() })
    })
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
            roots: vec![FolderRoot { folder: "/music".into(), root: "/data/media/Music/".into() }],
            libraries: vec![PlacedLibrary { id: "music".into(), name: "Music".into(), folders: vec!["/music".into()], paths: vec![], read_only: false }],
        }
    }

    #[test]
    fn entries_need_a_path_under_the_music_root() {
        let p = placement();
        let path = Some("/data/media/Music/A/01.mp3");
        assert_eq!(p.entry("/music", path).as_deref(), Some("A/01.mp3"));
        assert_eq!(p.entry("/incoming", path), None, "a folder the server was not found in");
        assert_eq!(p.entry("/music", None), None);
        assert_eq!(p.entry("/music", Some("/data/media/lidarr-music/A/01.mp3")), None);
        assert_eq!(p.entry("/music", Some("/data/media/Music/A/../../x.mp3")), None);
        assert_eq!(Placement::default().entry("/music", path), None, "unknown root");
        assert_eq!(p.playlist_path("/music", "Chill").as_deref(), Some("/data/media/Music/Chill.m3u"));
        assert_eq!(p.libraries_in("/music").map(|l| l.name.as_str()).collect::<Vec<_>>(), vec!["Music"]);
    }

    /// A source with fixed libraries and samples, and (optionally) each library's own folder.
    struct Fake(Vec<(Library, Vec<String>)>, Vec<(String, String)>);

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
        fn library_roots<'a>(&'a self, id: &'a str) -> Fut<'a, Vec<String>> {
            Box::pin(async move { Ok(self.1.iter().filter(|(l, _)| l == id).map(|(_, r)| r.clone()).collect()) })
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
        Library { id: id.into(), name: id.to_uppercase(), paths: vec![] }
    }

    #[tokio::test]
    async fn libraries_are_found_in_their_folders() {
        let m = music();
        let inc = m.with_extension("incoming");
        std::fs::create_dir_all(inc.join("X")).unwrap();
        std::fs::write(inc.join("X/01 New.flac"), b"x").unwrap();
        let folders = vec![m.display().to_string(), inc.display().to_string()];
        let src = Fake(
            vec![
                (lib("incoming"), vec!["/data/media/lidarr-music/X/01 New.flac".into()]),
                (lib("music"), vec!["/data/media/Music/missing.mp3".into(), "/data/media/Music/Artist - Album (2020)/01 Song.mp3".into()]),
                (lib("empty"), vec![]),
            ],
            vec![],
        );
        let p = place(&src, &folders).await.unwrap();
        assert_eq!(p.root(&folders[0]), Some("/data/media/Music/"));
        assert_eq!(p.root(&folders[1]), Some("/data/media/lidarr-music/"));
        let found: Vec<_> = p.libraries.iter().map(|l| (l.id.as_str(), l.folders.clone())).collect();
        assert_eq!(found, vec![("incoming", vec![folders[1].clone()]), ("music", vec![folders[0].clone()]), ("empty", vec![])]);
        std::fs::remove_dir_all(&m).ok();
        std::fs::remove_dir_all(&inc).ok();
    }

    #[tokio::test]
    async fn one_broad_mount_holds_each_library_in_its_own_folder() {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let media = std::env::temp_dir().join(format!("cratarr-media-{}-{n}", std::process::id()));
        for f in ["Music/Artist - Album (2020)/01 Song.mp3", "lidarr-music/X/Y/01 New.flac"] {
            std::fs::create_dir_all(media.join(f).parent().unwrap()).unwrap();
            std::fs::write(media.join(f), b"x").unwrap();
        }
        let m = media.display().to_string();
        let src = Fake(
            vec![
                (lib("music"), vec!["/data/media/Music/Artist - Album (2020)/01 Song.mp3".into()]),
                (lib("incoming"), vec!["/data/media/lidarr-music/X/Y/01 New.flac".into()]),
            ],
            vec![("music".into(), "/data/media/Music".into()), ("incoming".into(), "/data/media/lidarr-music/".into())],
        );
        let p = place(&src, &[m.clone()]).await.unwrap();
        let (music, incoming) = (format!("{m}/Music"), format!("{m}/lidarr-music"));
        assert_eq!(p.libraries[0].folders, vec![music.clone()]);
        assert_eq!(p.libraries[1].folders, vec![incoming.clone()]);
        assert_eq!(p.root(&music), Some("/data/media/Music/"));
        assert_eq!(p.entry(&music, Some("/data/media/Music/Artist - Album (2020)/01 Song.mp3")).as_deref(), Some("Artist - Album (2020)/01 Song.mp3"));
        assert_eq!(p.playlist_path(&music, "Chill").as_deref(), Some("/data/media/Music/Chill.m3u"));
        // Without the server's folders, the mount itself is the library's folder, as before.
        let bare = Fake(src.0.clone(), vec![]);
        assert_eq!(place(&bare, &[m.clone()]).await.unwrap().libraries[0].folders, vec![m.clone()]);
        std::fs::remove_dir_all(&media).ok();
    }
}
