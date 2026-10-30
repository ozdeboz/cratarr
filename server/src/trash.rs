//! Albums deleted from Cratarr go to a trash folder inside their own library's folder (each library keeps its own, the
//! owner, 2026-10-16; Music's too since decision 0034), so Undo can put them back and nothing is lost on one tap; the
//! trash is emptied after the days set in Settings (decision 0010). Inside the same folder because a rename across
//! mounts fails (and copying an album would be slow). Blocking file operations.

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Under each library's folder; hidden, so media servers, Lidarr and Cratarr's own scan leave it alone.
pub const TRASH_DIR: &str = ".cratarr-trash";
/// How long a deleted album can still be brought back, until Settings says otherwise.
pub const DEFAULT_KEEP_DAYS: u32 = 14;

/// `rel` must be a plain relative path inside the folder: no `..`, no root, not the trash itself.
fn checked(rel: &str) -> Result<PathBuf> {
    let p = Path::new(rel);
    let ok = !rel.is_empty()
        && !rel.contains('\0')
        && p.components().all(|c| matches!(c, Component::Normal(_)))
        && p.components().next().is_some_and(|c| c.as_os_str() != TRASH_DIR);
    if !ok {
        bail!("not an album folder inside Incoming: {rel:?}");
    }
    Ok(p.to_path_buf())
}

/// The token `trash` will return for `rel` at `now_epoch`: `<epoch>/<rel>`, relative to the trash. Known before
/// trashing, so the journal can record the intent first.
pub fn token(now_epoch: i64, rel: &str) -> String {
    format!("{now_epoch}/{rel}")
}

/// Whether the trash holds `token` (the intent of a deletion happened).
pub fn holds(folder: &Path, token: &str) -> bool {
    folder.join(TRASH_DIR).join(token).is_dir()
}

/// Move `folder/rel` into the trash. Returns the undo token (`token`). Safe to repeat: when it is already in the
/// trash under this token and gone from its place, that is success.
pub fn trash(folder: &Path, rel: &str, now_epoch: i64) -> Result<String> {
    let rel_path = checked(rel)?;
    let src = folder.join(&rel_path);
    let token = token(now_epoch, rel);
    let dest = folder.join(TRASH_DIR).join(&token);
    if !src.exists() && dest.is_dir() {
        return Ok(token);
    }
    if !src.is_dir() {
        bail!("{rel} is not there any more");
    }
    if dest.exists() {
        bail!("already in the trash: {token}");
    }
    crate::moves::create_dirs_like_parent(dest.parent().context("trash path has no parent")?)?;
    std::fs::rename(&src, &dest).with_context(|| format!("moving {rel} to the trash"))?;
    Ok(token)
}

/// Put a trashed album back where it was. `token` as returned by `trash`.
pub fn restore(folder: &Path, token: &str) -> Result<String> {
    let (stamp, rel) = token.split_once('/').context("not an undo token")?;
    if stamp.is_empty() || !stamp.bytes().all(|b| b.is_ascii_digit()) {
        bail!("not an undo token");
    }
    let rel_path = checked(rel)?;
    let src = folder.join(TRASH_DIR).join(stamp).join(&rel_path);
    let dest = folder.join(&rel_path);
    // Safe to repeat: already back (a crash after restoring, before the journal knew).
    if !src.exists() && dest.is_dir() {
        return Ok(rel.to_string());
    }
    if !src.is_dir() {
        bail!("{rel} is no longer in the trash");
    }
    if dest.exists() {
        bail!("{rel} exists again; not overwriting it");
    }
    crate::moves::create_dirs_like_parent(dest.parent().context("album path has no parent")?)?;
    std::fs::rename(&src, &dest).with_context(|| format!("restoring {rel}"))?;
    // Leave no empty stamp or artist folders behind in the trash.
    let mut d = src.parent().map(Path::to_path_buf);
    let stop = folder.join(TRASH_DIR);
    while let Some(dir) = d {
        if dir == stop || std::fs::remove_dir(&dir).is_err() {
            break;
        }
        d = dir.parent().map(Path::to_path_buf);
    }
    Ok(rel.to_string())
}

/// Move one track `folder/rel` into the trash (0.9.8: a track replaced by single-track replacement, or one it added and
/// Undo takes away); its token, as `trash`'s.
pub fn trash_file(folder: &Path, rel: &str, now_epoch: i64) -> Result<String> {
    let rel_path = checked(rel)?;
    let src = folder.join(&rel_path);
    let token = token(now_epoch, rel);
    let dest = folder.join(TRASH_DIR).join(&token);
    if !src.exists() && dest.is_file() {
        return Ok(token);
    }
    if !src.is_file() {
        bail!("{rel} is not there any more");
    }
    if dest.exists() {
        bail!("already in the trash: {token}");
    }
    crate::moves::create_dirs_like_parent(dest.parent().context("trash path has no parent")?)?;
    std::fs::rename(&src, &dest).with_context(|| format!("moving {rel} to the trash"))?;
    Ok(token)
}

/// Put a trashed track back where it was; its path relative to the folder.
pub fn restore_file(folder: &Path, token: &str) -> Result<String> {
    let (stamp, rel) = token.split_once('/').context("not an undo token")?;
    if stamp.is_empty() || !stamp.bytes().all(|b| b.is_ascii_digit()) {
        bail!("not an undo token");
    }
    let rel_path = checked(rel)?;
    let src = folder.join(TRASH_DIR).join(stamp).join(&rel_path);
    let dest = folder.join(&rel_path);
    if !src.exists() && dest.is_file() {
        return Ok(rel.to_string());
    }
    if !src.is_file() {
        bail!("{rel} is no longer in the trash");
    }
    if dest.exists() {
        bail!("{rel} exists again; not overwriting it");
    }
    crate::moves::create_dirs_like_parent(dest.parent().context("track path has no parent")?)?;
    std::fs::rename(&src, &dest).with_context(|| format!("restoring {rel}"))?;
    let mut d = src.parent().map(Path::to_path_buf);
    let stop = folder.join(TRASH_DIR);
    while let Some(dir) = d {
        if dir == stop || std::fs::remove_dir(&dir).is_err() {
            break;
        }
        d = dir.parent().map(Path::to_path_buf);
    }
    Ok(rel.to_string())
}

/// Empty trash older than `keep_days`. Returns how many deletions were removed for good.
pub fn purge(folder: &Path, now_epoch: i64, keep_days: u32) -> Result<usize> {
    let trash = folder.join(TRASH_DIR);
    let Ok(entries) = std::fs::read_dir(&trash) else { return Ok(0) };
    let mut gone = 0;
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(stamp) = name.to_str().and_then(|s| s.parse::<i64>().ok()) else { continue };
        if now_epoch - stamp > i64::from(keep_days) * 86_400 && e.path().is_dir() {
            std::fs::remove_dir_all(e.path()).with_context(|| format!("emptying {}", e.path().display()))?;
            gone += 1;
        }
    }
    Ok(gone)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let p = std::env::temp_dir().join(format!("cratarr-trash-{}-{}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    }

    #[test]
    fn an_album_goes_to_the_trash_and_comes_back() {
        let f = tmp();
        std::fs::create_dir_all(f.join("Air/Moon Safari (1998)")).unwrap();
        std::fs::write(f.join("Air/Moon Safari (1998)/01.flac"), b"x").unwrap();
        let token = trash(&f, "Air/Moon Safari (1998)", 1000).unwrap();
        assert_eq!(token, "1000/Air/Moon Safari (1998)");
        assert!(!f.join("Air/Moon Safari (1998)").exists());
        assert!(f.join(".cratarr-trash/1000/Air/Moon Safari (1998)/01.flac").exists());
        assert_eq!(restore(&f, &token).unwrap(), "Air/Moon Safari (1998)");
        assert!(f.join("Air/Moon Safari (1998)/01.flac").exists());
        assert!(!f.join(".cratarr-trash/1000").exists(), "no empty folders left in the trash");
    }

    #[test]
    fn nothing_outside_the_folder_or_in_the_trash_can_be_named() {
        let f = tmp();
        for bad in ["", "../etc", "/etc", "a/../../b", ".cratarr-trash/1/x"] {
            assert!(trash(&f, bad, 1).is_err(), "{bad:?}");
        }
        assert!(restore(&f, "abc/x").is_err());
        assert!(restore(&f, "1/../x").is_err());
    }

    #[test]
    fn restore_never_overwrites() {
        let f = tmp();
        std::fs::create_dir_all(f.join("A/B")).unwrap();
        let token = trash(&f, "A/B", 5).unwrap();
        std::fs::create_dir_all(f.join("A/B")).unwrap();
        assert!(restore(&f, &token).is_err());
    }

    #[test]
    fn only_old_trash_is_emptied() {
        let f = tmp();
        std::fs::create_dir_all(f.join("Old/X")).unwrap();
        std::fs::create_dir_all(f.join("New/Y")).unwrap();
        trash(&f, "Old/X", 0).unwrap();
        let now = 20 * 86_400;
        trash(&f, "New/Y", now - 86_400).unwrap();
        assert_eq!(purge(&f, now, 14).unwrap(), 1);
        assert!(!f.join(".cratarr-trash/0").exists());
        assert!(f.join(format!(".cratarr-trash/{}/New/Y", now - 86_400)).exists());
    }
}
