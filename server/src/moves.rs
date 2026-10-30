//! Moving an album's files between Cratarr's folders (Keep and Replace, decision 0013): every file of a plan goes, or
//! none does. A rename where the two folders share a file system (instant); a copy then delete where they do not.
//! Folders left empty behind are removed, up to the folder's root. Blocking file operations.

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

/// One file to move: (root folder, path relative to it) → (root folder, path relative to it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub from: (PathBuf, String),
    pub to: (PathBuf, String),
}

/// `rel` must be a plain relative path: no `..`, no root, not into a hidden folder (backups), except a trash entry
/// (`.cratarr-trash/<stamp>/…`: Replace moves the old files there) and converted files waiting to take the originals'
/// place (`.cratarr-convert/…`, decision 0013).
fn checked(rel: &str) -> Result<PathBuf> {
    let p = Path::new(rel);
    let plain = |rest: &Path| rest.components().all(|c| matches!(c, Component::Normal(n) if !n.to_string_lossy().starts_with('.')));
    let in_trash = || -> Option<bool> {
        let rest = p.strip_prefix(crate::trash::TRASH_DIR).ok()?;
        let stamp = rest.components().next()?.as_os_str().to_str()?;
        Some(!stamp.is_empty() && stamp.bytes().all(|b| b.is_ascii_digit()) && rest.components().count() > 1 && plain(rest))
    };
    let staged = || p.strip_prefix(crate::convert::STAGING_DIR).is_ok_and(|rest| rest.components().count() > 1 && plain(rest));
    let ok = !rel.is_empty() && !rel.contains('\0') && (plain(p) || in_trash() == Some(true) || staged());
    if !ok {
        bail!("not a plain path inside a library: {rel:?}");
    }
    Ok(p.to_path_buf())
}

fn full((root, rel): &(PathBuf, String)) -> Result<PathBuf> {
    Ok(root.join(checked(rel)?))
}

/// Move every file of the plan, in order. Nothing is overwritten: when a target exists, nothing moves, unless an
/// earlier step moves that file away (Replace: an old file goes to the trash, a new one of the same name takes its
/// place). When a move fails midway, the files already moved go back and the error is returned. Safe to repeat: a
/// step already done (its file at the target, and gone from the source or put there by a later step) is skipped, so
/// a plan interrupted by a crash can be finished.
pub fn apply(plan: &[Step]) -> Result<()> {
    let all: Vec<(PathBuf, PathBuf)> = plan.iter().map(|s| Ok((full(&s.from)?, full(&s.to)?))).collect::<Result<_>>()?;
    let done = |i: usize| {
        let (from, to) = &all[i];
        to.is_file() && (!from.exists() || all[i + 1..].iter().any(|(_, later)| later == from))
    };
    let pairs: Vec<(PathBuf, PathBuf)> = (0..all.len()).filter(|&i| !done(i)).map(|i| all[i].clone()).collect();
    for (k, (from, to)) in pairs.iter().enumerate() {
        if !from.is_file() {
            bail!("{} is not there", from.display());
        }
        if to.exists() && !pairs[..k].iter().any(|(earlier, _)| earlier == to) {
            bail!("{} exists already; not overwriting it", to.display());
        }
    }
    let mut done: Vec<&(PathBuf, PathBuf)> = Vec::new();
    for pair in &pairs {
        if let Err(e) = move_file(&pair.0, &pair.1) {
            for (from, to) in done.iter().rev() {
                if let Err(back) = move_file(to, from) {
                    tracing::error!("could not move {} back to {}: {back:#}", to.display(), from.display());
                }
            }
            return Err(e);
        }
        done.push(pair);
    }
    for s in plan {
        prune_empty(&s.from.0, &s.from.1);
    }
    Ok(())
}

/// Undo `apply`: every file back where it came from.
pub fn revert(plan: &[Step]) -> Result<()> {
    let back: Vec<Step> = plan.iter().rev().map(|s| Step { from: s.to.clone(), to: s.from.clone() }).collect();
    apply(&back)
}

/// Make `dir` and any missing folders above it, each with the permission bits of the folder it is made in, so new
/// folders look like their neighbours whatever the process's umask: a NAS's shared media folders are 2775 for a
/// group, and Cratarr's default umask (022) made 2755 folders the group could not write in (seen 2026-10-13). Safe to
/// repeat; setting the bits is best effort.
pub fn create_dirs_like_parent(dir: &Path) -> std::io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    let parent = dir.parent().ok_or_else(|| std::io::Error::other("no folder above"))?;
    create_dirs_like_parent(parent)?;
    match std::fs::create_dir(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(e) => return Err(e),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(parent) {
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(meta.permissions().mode() & 0o7777));
        }
    }
    Ok(())
}

fn move_file(from: &Path, to: &Path) -> Result<()> {
    create_dirs_like_parent(to.parent().context("target has no folder")?)?;
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        // Another file system: copy, make sure it is on disk, then remove the original.
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            std::fs::copy(from, to).with_context(|| format!("copying {} to {}", from.display(), to.display()))?;
            std::fs::File::open(to)?.sync_all()?;
            std::fs::remove_file(from).with_context(|| format!("removing {} after copying it", from.display()))
        }
        Err(e) => Err(e).with_context(|| format!("moving {} to {}", from.display(), to.display())),
    }
}

/// Remove the folders of `rel` that are now empty, deepest first, never `root` itself.
fn prune_empty(root: &Path, rel: &str) {
    let mut dir = root.join(rel).parent().map(Path::to_path_buf);
    while let Some(d) = dir {
        if d == root || !d.starts_with(root) || std::fs::remove_dir(&d).is_err() {
            break;
        }
        dir = d.parent().map(Path::to_path_buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!("cratarr-moves-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[cfg(unix)]
    #[test]
    fn new_folders_take_the_permissions_of_the_folder_they_are_made_in() {
        use std::os::unix::fs::PermissionsExt;
        let root = tmp();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o2775)).unwrap();
        create_dirs_like_parent(&root.join("Artist - Album (2000)/CD1")).unwrap();
        for d in ["Artist - Album (2000)", "Artist - Album (2000)/CD1"] {
            let mode = std::fs::metadata(root.join(d)).unwrap().permissions().mode() & 0o7777;
            assert_eq!(mode, 0o2775, "{d}");
        }
        create_dirs_like_parent(&root.join("Artist - Album (2000)")).unwrap();
    }

    fn step(from: &Path, a: &str, to: &Path, b: &str) -> Step {
        Step { from: (from.to_path_buf(), a.into()), to: (to.to_path_buf(), b.into()) }
    }

    #[test]
    fn an_album_moves_renamed_and_its_empty_folders_go() {
        let (inc, music) = (tmp(), tmp());
        std::fs::create_dir_all(inc.join("Air/Moon Safari (1998)")).unwrap();
        std::fs::write(inc.join("Air/Moon Safari (1998)/01 - Sexy Boy.flac"), b"a").unwrap();
        std::fs::write(inc.join("Air/Moon Safari (1998)/cover.jpg"), b"c").unwrap();
        let plan = vec![
            step(&inc, "Air/Moon Safari (1998)/01 - Sexy Boy.flac", &music, "Air - Moon Safari (1998)/01 Air - Sexy Boy.flac"),
            step(&inc, "Air/Moon Safari (1998)/cover.jpg", &music, "Air - Moon Safari (1998)/cover.jpg"),
        ];
        apply(&plan).unwrap();
        assert_eq!(std::fs::read(music.join("Air - Moon Safari (1998)/01 Air - Sexy Boy.flac")).unwrap(), b"a");
        assert!(!inc.join("Air").exists(), "album and artist folders left empty are removed");
        assert!(inc.exists(), "never the root");
        revert(&plan).unwrap();
        assert_eq!(std::fs::read(inc.join("Air/Moon Safari (1998)/cover.jpg")).unwrap(), b"c");
        assert!(!music.join("Air - Moon Safari (1998)").exists());
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn nothing_moves_when_a_target_exists() {
        let (inc, music) = (tmp(), tmp());
        std::fs::create_dir_all(inc.join("A")).unwrap();
        std::fs::write(inc.join("A/1.flac"), b"1").unwrap();
        std::fs::write(inc.join("A/2.flac"), b"2").unwrap();
        std::fs::create_dir_all(music.join("A")).unwrap();
        std::fs::write(music.join("A/2.flac"), b"mine").unwrap();
        let plan = vec![step(&inc, "A/1.flac", &music, "A/1.flac"), step(&inc, "A/2.flac", &music, "A/2.flac")];
        assert!(apply(&plan).is_err());
        assert!(inc.join("A/1.flac").exists() && !music.join("A/1.flac").exists(), "all or nothing");
        assert_eq!(std::fs::read(music.join("A/2.flac")).unwrap(), b"mine");
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn an_interrupted_plan_can_be_finished_and_undone_twice() {
        let (inc, music) = (tmp(), tmp());
        std::fs::create_dir_all(inc.join("A")).unwrap();
        std::fs::write(inc.join("A/1.flac"), b"1").unwrap();
        std::fs::write(inc.join("A/2.flac"), b"2").unwrap();
        let plan = vec![step(&inc, "A/1.flac", &music, "B/1.flac"), step(&inc, "A/2.flac", &music, "B/2.flac")];
        // A crash after the first file: finishing it moves only the rest.
        std::fs::create_dir_all(music.join("B")).unwrap();
        std::fs::rename(inc.join("A/1.flac"), music.join("B/1.flac")).unwrap();
        apply(&plan).unwrap();
        apply(&plan).unwrap();
        assert!(music.join("B/2.flac").exists() && !inc.join("A").exists());
        revert(&plan).unwrap();
        revert(&plan).unwrap();
        assert_eq!(std::fs::read(inc.join("A/2.flac")).unwrap(), b"2");
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn paths_must_stay_inside_and_out_of_hidden_folders() {
        let r = tmp();
        for bad in ["../x", "/etc/passwd", ".cratarr-trash/x/A", ".cratarr-trash/1", ".cratarr-trash/1/.h/A", ".cratarr-convert/x", ".backups/A", "A/../../x", ""] {
            assert!(apply(&[step(&r, bad, &r, "ok")]).is_err(), "{bad}");
            assert!(apply(&[step(&r, "ok", &r, bad)]).is_err(), "{bad}");
        }
        std::fs::remove_dir_all(r).ok();
    }

    #[test]
    fn a_replacement_takes_the_old_files_name_and_the_old_file_goes_to_the_trash() {
        let (inc, music) = (tmp(), tmp());
        std::fs::create_dir_all(inc.join("A")).unwrap();
        std::fs::create_dir_all(music.join("X")).unwrap();
        std::fs::write(inc.join("A/1.flac"), b"new").unwrap();
        std::fs::write(music.join("X/01.flac"), b"old").unwrap();
        let plan = vec![step(&music, "X/01.flac", &inc, ".cratarr-trash/100/X/01.flac"), step(&inc, "A/1.flac", &music, "X/01.flac")];
        apply(&plan).unwrap();
        apply(&plan).unwrap();
        assert_eq!(std::fs::read(music.join("X/01.flac")).unwrap(), b"new");
        assert_eq!(std::fs::read(inc.join(".cratarr-trash/100/X/01.flac")).unwrap(), b"old");
        revert(&plan).unwrap();
        revert(&plan).unwrap();
        assert_eq!(std::fs::read(music.join("X/01.flac")).unwrap(), b"old");
        assert_eq!(std::fs::read(inc.join("A/1.flac")).unwrap(), b"new");
        // Without the earlier step the same target is refused.
        assert!(apply(&plan[1..]).is_err());
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }
}
