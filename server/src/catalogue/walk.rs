//! Walking a music folder: every audio file with its size and modification time. Hidden entries (".cratarr-backups"
//! and the like) are skipped; symlinks are not followed, so the walk never leaves the folder.

use std::path::Path;

/// File extensions the tag reader handles, lower case.
pub const AUDIO: [&str; 14] = ["mp3", "flac", "m4a", "mp4", "aac", "ogg", "oga", "opus", "wav", "aif", "aiff", "wv", "ape", "mpc"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Relative to the folder, with "/".
    pub rel: String,
    pub size: u64,
    /// Seconds since the epoch.
    pub mtime: i64,
}

pub fn is_audio(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| AUDIO.contains(&ext.to_ascii_lowercase().as_str()))
}

/// Every audio file under `root`. Unreadable sub-folders are skipped (and counted), not fatal.
pub fn walk(root: &Path) -> std::io::Result<(Vec<Found>, usize)> {
    walk_with_covers(root).map(|(found, _, skipped)| (found, skipped))
}

/// Every audio file under `root`, and every cover file beside them (0.9.7: Cratarr's cover names, `covers::CANDIDATES`,
/// not empty), each with its size and time: a cover changed on disk changes its album's stamp.
pub fn walk_with_covers(root: &Path) -> std::io::Result<(Vec<Found>, Vec<Found>, usize)> {
    let mut out = Vec::new();
    let mut covers = Vec::new();
    let mut skipped = 0;
    let mut stack = vec![(root.to_path_buf(), String::new())];
    // The root itself must be readable; below it, problems only skip that part.
    let mut first = true;
    while let Some((dir, prefix)) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if first => return Err(e),
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        first = false;
        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                skipped += 1;
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
            if kind.is_dir() {
                stack.push((entry.path(), rel));
            } else if kind.is_file() && (is_audio(&name) || crate::covers::candidate_rank(&name).is_some()) {
                let Ok(meta) = entry.metadata() else {
                    skipped += 1;
                    continue;
                };
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                if is_audio(&name) {
                    out.push(Found { rel, size: meta.len(), mtime });
                } else if meta.len() > 0 {
                    // An empty file is no cover (`serve::art::cover_file`).
                    covers.push(Found { rel, size: meta.len(), mtime });
                }
            }
        }
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    covers.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok((out, covers, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_files_are_found_and_hidden_folders_skipped() {
        let d = std::env::temp_dir().join(format!("cratarr-walk-{}", std::process::id()));
        std::fs::create_dir_all(d.join("A - B (2020)")).unwrap();
        std::fs::create_dir_all(d.join(".cratarr-backups")).unwrap();
        std::fs::write(d.join("A - B (2020)/01 x.FLAC"), b"12345").unwrap();
        std::fs::write(d.join("A - B (2020)/cover.jpg"), b"x").unwrap();
        std::fs::write(d.join(".cratarr-backups/old.mp3"), b"x").unwrap();
        std::fs::write(d.join("Chill.m3u"), b"x").unwrap();
        let (found, skipped) = walk(&d).unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rel, "A - B (2020)/01 x.FLAC");
        assert_eq!(found[0].size, 5);
        assert!(found[0].mtime > 0);
        assert!(walk(&d.join("missing")).is_err(), "an unreadable root is an error");
        std::fs::write(d.join("A - B (2020)/folder.jpg"), b"").unwrap();
        let (_, covers, _) = walk_with_covers(&d).unwrap();
        assert_eq!(covers.iter().map(|c| c.rel.as_str()).collect::<Vec<_>>(), ["A - B (2020)/cover.jpg"], "its cover beside it; an empty one is none");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn extensions_are_matched_without_case() {
        assert!(is_audio("x.Mp3") && is_audio("a.b.opus"));
        assert!(!is_audio("x.m3u") && !is_audio("flac") && !is_audio("cover.jpg"));
    }
}
