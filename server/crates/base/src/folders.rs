//! Folders, chosen and shown (decision 0044): a browser of what Cratarr can see, to add a folder in, and a folder's
//! state (whether it is there, free space, whether Cratarr can write there). Only what is inside the container's mounts
//! is listed or looked into, nothing above them, symlinks included; Cratarr's own data folder is left out.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Audio files counted in a folder at most (a library holds tens of thousands): enough to tell music from not.
const COUNT_UP_TO: u32 = 500;
/// How deep a folder is looked into for its audio.
const COUNT_DEPTH: usize = 4;
const AUDIO: [&str; 10] = ["flac", "mp3", "m4a", "ogg", "opus", "wav", "aiff", "aif", "alac", "wma"];

/// One folder in the browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub name: String,
    pub path: String,
    /// Audio files in it and below (at most [COUNT_UP_TO]; `more` when there are more).
    pub audio: u32,
    pub more: bool,
    pub writable: bool,
}

/// What the browser shows at a place: its folders, and where up leads (none at the top, the mounts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Listing {
    /// None: the top, the mounts themselves.
    pub path: Option<String>,
    pub parent: Option<String>,
    pub folders: Vec<Entry>,
}

/// A chosen folder's state, for its row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    pub exists: bool,
    pub writable: bool,
    /// Bytes free on its disk; none where that cannot be told.
    pub free: Option<u64>,
}

/// What the browser starts from: the container's mounts, Cratarr's own data folder left out.
pub fn roots() -> Vec<String> {
    let data = crate::serve::instance()
        .and_then(|s| s.data_dir.as_ref().map(|d| d.display().to_string()))
        .or_else(|| std::env::var("CRATARR_DATA_DIR").ok().filter(|d| !d.trim().is_empty()));
    let exclude: Vec<&str> = data.as_deref().into_iter().collect();
    crate::mounts::list(&exclude)
}

fn audio(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| AUDIO.iter().any(|a| a.eq_ignore_ascii_case(ext)))
}

/// Audio files in `dir` and below, up to [COUNT_UP_TO] (and whether there are more). Hidden folders are skipped.
fn count_audio(dir: &Path) -> (u32, bool) {
    let mut found = 0u32;
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() && depth < COUNT_DEPTH {
                stack.push((e.path(), depth + 1));
            } else if t.is_file() && audio(&name) {
                found += 1;
                if found >= COUNT_UP_TO {
                    return (found, true);
                }
            }
        }
    }
    (found, false)
}

/// `path` resolved, when it lies inside one of `roots` (each resolved too); none otherwise.
pub fn inside(path: &str, roots: &[String]) -> Option<PathBuf> {
    let p = std::fs::canonicalize(path).ok()?;
    roots.iter().filter_map(|r| std::fs::canonicalize(r).ok()).any(|r| p.starts_with(&r)).then_some(p)
}

fn entry(p: &Path) -> Entry {
    let (audio, more) = count_audio(p);
    Entry {
        name: p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned()),
        path: p.display().to_string(),
        audio,
        more,
        writable: crate::mounts::writable(p),
    }
}

/// What the browser shows at `path` (none: the top, `roots` themselves): its folders by name, hidden ones left out.
/// A path outside `roots` is refused. Blocking.
pub fn browse(path: Option<&str>, roots: &[String]) -> Result<Listing, String> {
    let Some(path) = path.filter(|p| !p.is_empty()) else {
        let mut folders: Vec<Entry> = roots.iter().map(|r| entry(Path::new(r))).collect();
        folders.sort_by(|a, b| a.path.cmp(&b.path));
        return Ok(Listing { path: None, parent: None, folders });
    };
    let here = inside(path, roots).ok_or_else(|| "Cratarr cannot see that folder.".to_string())?;
    let at_root = roots.iter().filter_map(|r| std::fs::canonicalize(r).ok()).any(|r| r == here);
    let parent = if at_root { None } else { here.parent().map(|p| p.display().to_string()) };
    let mut folders: Vec<Entry> = std::fs::read_dir(&here)
        .map_err(|_| "Cratarr cannot read that folder.".to_string())?
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| inside(&e.path().display().to_string(), roots))
        .map(|p| entry(&p))
        .collect();
    folders.sort_by_key(|e| e.name.to_lowercase());
    Ok(Listing { path: Some(here.display().to_string()), parent, folders })
}

/// Bytes free on the disk `dir` is on. Unix only.
#[cfg(unix)]
pub fn free(dir: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path and `s` a writable statvfs.
    (unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0).then(|| s.f_bavail as u64 * s.f_frsize as u64)
}

#[cfg(not(unix))]
pub fn free(_dir: &Path) -> Option<u64> {
    None
}

/// A chosen folder's state. Blocking.
pub fn status(dir: &Path) -> Status {
    let exists = dir.is_dir();
    Status { exists, writable: exists && crate::mounts::writable(dir), free: if exists { free(dir) } else { None } }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small library, its own per test (tests run side by side).
    fn tree(test: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("cratarr-folders-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (dir, file) in [("Music/A/X", "01.flac"), ("Music/A/X", "02.FLAC"), ("Music/B", "notes.txt"), ("Incoming", ""), (".hidden/Y", "01.mp3")] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            if !file.is_empty() {
                std::fs::write(root.join(dir).join(file), b"x").unwrap();
            }
        }
        root
    }

    #[test]
    fn the_browser_shows_folders_inside_the_mounts_only() {
        let root = tree("browser");
        let roots = [root.display().to_string()];
        let top = browse(None, &roots).unwrap();
        assert_eq!(top.folders.len(), 1);
        assert_eq!((top.folders[0].audio, top.folders[0].more), (2, false), "the hidden folder's audio is not counted");
        let at = browse(Some(&roots[0]), &roots).unwrap();
        assert_eq!(at.parent, None, "up from a mount is the top");
        assert_eq!(at.folders.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Incoming", "Music"], "hidden ones left out");
        assert!(at.folders.iter().all(|e| e.writable));
        let music = browse(Some(&root.join("Music").display().to_string()), &roots).unwrap();
        assert_eq!(music.parent.as_deref(), Some(std::fs::canonicalize(&root).unwrap().display().to_string().as_str()));
        assert_eq!(music.folders.iter().map(|e| (e.name.as_str(), e.audio)).collect::<Vec<_>>(), [("A", 2), ("B", 0)]);
        assert!(browse(Some("/"), &roots).is_err(), "nothing above the mounts");
        assert!(browse(Some(&root.join("Music/../..").display().to_string()), &roots).is_err());
        assert!(browse(Some(&root.join("missing").display().to_string()), &roots).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folders_status_says_whether_it_is_there_and_writable() {
        let root = tree("status");
        let s = status(&root.join("Incoming"));
        assert!(s.exists && s.writable);
        #[cfg(unix)]
        assert!(s.free.is_some_and(|f| f > 0));
        let gone = status(&root.join("missing"));
        assert_eq!(gone, Status { exists: false, writable: false, free: None });
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn counting_stops_at_its_limit() {
        let root = std::env::temp_dir().join(format!("cratarr-folders-many-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for i in 0..(COUNT_UP_TO + 3) {
            std::fs::write(root.join(format!("{i}.mp3")), b"x").unwrap();
        }
        assert_eq!(count_audio(&root), (COUNT_UP_TO, true));
        let _ = std::fs::remove_dir_all(&root);
    }
}
