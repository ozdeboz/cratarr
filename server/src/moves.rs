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
/// place). When a move fails midway, the files already moved go back, what was made for them goes, and the error is
/// returned. Safe to repeat: a step already done (its file at the target, and gone from the source or put there by a
/// later step) is skipped, and the one step a stop left whole at both ends has only its original taken away, so a plan
/// interrupted by a crash can be finished.
pub fn apply(plan: &[Step]) -> Result<()> {
    let all: Vec<(PathBuf, PathBuf)> = plan.iter().map(|s| Ok((full(&s.from)?, full(&s.to)?))).collect::<Result<_>>()?;
    let done = |i: usize| {
        let (from, to) = &all[i];
        to.is_file() && (!from.exists() || all[i + 1..].iter().any(|(_, later)| later == from))
    };
    let mut pairs: Vec<(PathBuf, PathBuf)> = (0..all.len()).filter(|&i| !done(i)).map(|i| all[i].clone()).collect();
    // A copy stopped after its rename left the file whole at both ends: only taking the original away is left. A stop is
    // on one step, the first not done, and what follows it has not begun: a file alike anywhere else is a target like any
    // other. So is one another step moves on: those are two files, however alike.
    let stopped = match pairs.first() {
        Some((from, to)) if !all.iter().any(|(moved, _)| moved == to) && to.is_file() && from.is_file() && same_bytes(from, to)? => Some(pairs.remove(0)),
        _ => None,
    };
    for (k, (from, to)) in pairs.iter().enumerate() {
        if !from.is_file() {
            bail!("{} is not there", from.display());
        }
        if to.exists() && !pairs[..k].iter().any(|(earlier, _)| earlier == to) {
            bail!("{} exists already; not overwriting it", to.display());
        }
    }
    // Only now, the look behind it: a plan refused takes nothing away. Before the moves, so a stop in them leaves one step
    // at both ends, never two.
    if let Some((from, _)) = &stopped {
        std::fs::remove_file(from).with_context(|| format!("removing {} after copying it", from.display()))?;
    }
    let mut done: Vec<&(PathBuf, PathBuf)> = Vec::new();
    for pair in &pairs {
        if let Err(e) = move_file(&pair.0, &pair.1) {
            for (from, to) in done.iter().rev() {
                if let Err(back) = move_file(to, from) {
                    tracing::error!("could not move {} back to {}: {back:#}", to.display(), from.display());
                }
            }
            // Copied back, not moved: the file at its target may be one the plan found there.
            if let Some((from, to)) = &stopped
                && let Err(back) = copy_across(to, from, |_| Ok(()))
            {
                tracing::error!("could not put {} back: {back:#}", from.display());
            }
            // A target folder left behind would refuse every retry ("already has a folder named").
            for s in plan {
                prune_empty(&s.to.0, &s.to.1);
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

/// What a plan stopped before any file reached its target left there: a hidden part, the folders made for it. A plan
/// given up, so a Keep tried again finds the way free. Blocking.
pub fn clear_parts(plan: &[Step]) {
    for s in plan {
        if let Ok(to) = full(&s.to) {
            std::fs::remove_file(part_of(&to)).ok();
        }
        prune_empty(&s.to.0, &s.to.1);
    }
}

/// Whether two files hold the same bytes, read a piece at a time: a track can be larger than the server's memory allows.
fn same_bytes(a: &Path, b: &Path) -> Result<bool> {
    use std::io::Read;
    if std::fs::metadata(a)?.len() != std::fs::metadata(b)?.len() {
        return Ok(false);
    }
    let (mut a, mut b) = (std::io::BufReader::new(std::fs::File::open(a)?), std::io::BufReader::new(std::fs::File::open(b)?));
    let (mut x, mut y) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    loop {
        let n = a.read(&mut x)?;
        if n == 0 {
            return Ok(true);
        }
        b.read_exact(&mut y[..n])?;
        if x[..n] != y[..n] {
            return Ok(false);
        }
    }
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

/// Where a file is made before it takes its name: hidden, so no walk sees it half-written, and beside it, so the last step
/// is a rename.
pub fn part_of(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!(".{name}.cratarr-part"))
}

/// A file's new content, made by `fill` in `part_of(path)`, put in its place: a rename, so whatever stops this the file
/// is whole, the old one or the new. Its owner and rights stay: where this account may not give a file away, its group
/// and rights. What `fill` left goes on an error.
pub fn replace_through_part(path: &Path, fill: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    let part = part_of(path);
    let done = (|| -> Result<()> {
        fill(&part)?;
        let meta = std::fs::metadata(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, chown};
            chown(&part, Some(meta.uid()), Some(meta.gid())).or_else(|_| chown(&part, None, Some(meta.gid()))).ok();
        }
        std::fs::set_permissions(&part, meta.permissions())?;
        std::fs::File::open(&part)?.sync_all()?;
        std::fs::rename(&part, path).with_context(|| format!("putting {} in its place", path.display()))
    })();
    if done.is_err() {
        std::fs::remove_file(&part).ok();
    }
    done
}

fn move_file(from: &Path, to: &Path) -> Result<()> {
    create_dirs_like_parent(to.parent().context("target has no folder")?)?;
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => copy_across(from, to, |f| std::fs::remove_file(f)),
        Err(e) => Err(e).with_context(|| format!("moving {} to {}", from.display(), to.display())),
    }
}

/// Another file system: copied under a hidden name and made sure of on disk, then renamed, so a copy cut short is never a
/// track under its real name; then the original removed (`remove`). On any error what this made goes, and the original
/// stands whole.
fn copy_across(from: &Path, to: &Path, remove: impl FnOnce(&Path) -> std::io::Result<()>) -> Result<()> {
    let part = part_of(to);
    let placed = (|| -> Result<()> {
        std::fs::copy(from, &part).with_context(|| format!("copying {} to {}", from.display(), to.display()))?;
        std::fs::File::open(&part)?.sync_all()?;
        Ok(std::fs::rename(&part, to)?)
    })();
    if let Err(e) = placed {
        std::fs::remove_file(&part).ok();
        return Err(e);
    }
    remove(from).map_err(|e| {
        std::fs::remove_file(to).ok();
        anyhow::Error::new(e).context(format!("removing {} after copying it", from.display()))
    })
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

    fn names_in(d: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn a_copy_comes_whole_and_the_original_goes() {
        let (inc, music) = (tmp(), tmp());
        std::fs::write(inc.join("1.flac"), b"sound").unwrap();
        copy_across(&inc.join("1.flac"), &music.join("1.flac"), |f| {
            assert_eq!(names_in(&music), ["1.flac"], "named as a whole file, nothing else beside it");
            assert_eq!(std::fs::read(music.join("1.flac")).unwrap(), b"sound");
            std::fs::remove_file(f)
        })
        .unwrap();
        assert!(!inc.join("1.flac").exists());
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// The whole road, on two real file systems: the temporary folder and the shared memory beside it, where there is one.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_plan_across_file_systems_moves_every_file_and_its_undo_brings_them_back() {
        use std::os::unix::fs::MetadataExt;
        let music = tmp();
        let inc = Path::new("/dev/shm").join(format!("cratarr-moves-{}", std::process::id()));
        if std::fs::create_dir_all(&inc).is_err() || std::fs::metadata(&inc).unwrap().dev() == std::fs::metadata(&music).unwrap().dev() {
            return;
        }
        std::fs::create_dir_all(inc.join("A")).unwrap();
        std::fs::write(inc.join("A/1.flac"), b"1").unwrap();
        std::fs::write(inc.join("A/2.flac"), b"2").unwrap();
        let plan = vec![step(&inc, "A/1.flac", &music, "B/1.flac"), step(&inc, "A/2.flac", &music, "B/2.flac")];
        apply(&plan).unwrap();
        assert_eq!(names_in(&music.join("B")), ["1.flac", "2.flac"]);
        assert!(!inc.join("A").exists());
        revert(&plan).unwrap();
        assert_eq!(names_in(&inc.join("A")), ["1.flac", "2.flac"]);
        assert!(!music.join("B").exists());
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// `/proc/self/mem` opens and then fails its first read, as a disk failing part way does: the target is made by then.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_copy_that_fails_part_way_leaves_nothing_under_its_real_name() {
        let music = tmp();
        copy_across(Path::new("/proc/self/mem"), &music.join("1.flac"), |_| panic!("no copy, nothing to remove")).unwrap_err();
        assert!(names_in(&music).is_empty(), "{:?}", names_in(&music));
        std::fs::remove_dir_all(music).ok();
    }

    #[test]
    fn a_copy_whose_original_cannot_go_is_taken_back_and_the_original_stands() {
        let (inc, music) = (tmp(), tmp());
        std::fs::write(inc.join("1.flac"), b"sound").unwrap();
        let e = copy_across(&inc.join("1.flac"), &music.join("1.flac"), |_| Err(std::io::ErrorKind::PermissionDenied.into())).unwrap_err();
        assert!(format!("{e:#}").contains("after copying it"), "{e:#}");
        assert!(names_in(&music).is_empty());
        assert_eq!(std::fs::read(inc.join("1.flac")).unwrap(), b"sound");
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn a_copy_stopped_before_its_rename_leaves_no_track_and_is_made_again() {
        let (inc, music) = (tmp(), tmp());
        std::fs::create_dir_all(inc.join("A")).unwrap();
        std::fs::write(inc.join("A/1.flac"), b"sound").unwrap();
        std::fs::create_dir_all(music.join("B")).unwrap();
        std::fs::write(part_of(&music.join("B/1.flac")), b"so").unwrap();
        assert!(crate::catalogue::walk::walk(&music).unwrap().0.is_empty(), "no walk takes it for a track");
        copy_across(&inc.join("A/1.flac"), &music.join("B/1.flac"), |f| std::fs::remove_file(f)).unwrap();
        assert_eq!(names_in(&music.join("B")), ["1.flac"]);
        assert_eq!(std::fs::read(music.join("B/1.flac")).unwrap(), b"sound");
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn a_copy_stopped_after_its_rename_is_finished_by_a_repeat() {
        let (inc, music) = (tmp(), tmp());
        std::fs::create_dir_all(inc.join("A")).unwrap();
        std::fs::write(inc.join("A/1.flac"), b"1").unwrap();
        std::fs::write(inc.join("A/2.flac"), b"2").unwrap();
        std::fs::create_dir_all(music.join("B")).unwrap();
        std::fs::write(music.join("B/1.flac"), b"1").unwrap();
        let plan = vec![step(&inc, "A/1.flac", &music, "B/1.flac"), step(&inc, "A/2.flac", &music, "B/2.flac")];
        apply(&plan).unwrap();
        assert!(!inc.join("A").exists(), "the album left whole");
        assert_eq!(names_in(&music.join("B")), ["1.flac", "2.flac"]);
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// A file alike at a target is no reason to take anything away until the look has passed the whole plan.
    #[test]
    fn a_plan_refused_takes_no_original_away_however_alike_its_target() {
        let (inc, music) = (tmp(), tmp());
        std::fs::create_dir_all(inc.join("A")).unwrap();
        std::fs::write(inc.join("A/1.flac"), b"1").unwrap();
        std::fs::write(inc.join("A/2.flac"), b"2").unwrap();
        std::fs::create_dir_all(music.join("B")).unwrap();
        std::fs::write(music.join("B/1.flac"), b"1").unwrap();
        std::fs::write(music.join("B/2.flac"), b"another file").unwrap();
        let plan = vec![step(&inc, "A/1.flac", &music, "B/1.flac"), step(&inc, "A/2.flac", &music, "B/2.flac")];
        assert!(format!("{:#}", apply(&plan).unwrap_err()).contains("exists already"));
        assert_eq!(names_in(&inc.join("A")), ["1.flac", "2.flac"], "all or nothing");
        assert_eq!(std::fs::read(music.join("B/2.flac")).unwrap(), b"another file");
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// A stop is on one step: past the first step not done nothing has begun, so what stands there is no copy of the plan's.
    #[test]
    fn a_file_alike_past_the_first_step_not_done_is_a_target_like_any_other() {
        let (inc, music) = (tmp(), tmp());
        std::fs::create_dir_all(inc.join("A")).unwrap();
        std::fs::write(inc.join("A/1.flac"), b"1").unwrap();
        std::fs::write(inc.join("A/2.flac"), b"2").unwrap();
        std::fs::create_dir_all(music.join("B")).unwrap();
        std::fs::write(music.join("B/2.flac"), b"2").unwrap();
        let plan = vec![step(&inc, "A/1.flac", &music, "B/1.flac"), step(&inc, "A/2.flac", &music, "B/2.flac")];
        assert!(format!("{:#}", apply(&plan).unwrap_err()).contains("exists already"));
        assert_eq!(names_in(&inc.join("A")), ["1.flac", "2.flac"], "nothing moved");
        assert_eq!(names_in(&music.join("B")), ["2.flac"]);
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn a_plan_that_fails_after_a_stopped_copy_leaves_it_at_both_ends_as_found() {
        let (inc, music) = (tmp(), tmp());
        std::fs::create_dir_all(inc.join("A")).unwrap();
        std::fs::write(inc.join("A/1.flac"), b"1").unwrap();
        std::fs::write(inc.join("A/2.flac"), b"2").unwrap();
        std::fs::create_dir_all(music.join("B")).unwrap();
        std::fs::write(music.join("B/1.flac"), b"1").unwrap();
        // A file where the second one's folder should be: the look before moving does not see it, the move fails.
        std::fs::write(music.join("C"), b"a file").unwrap();
        let plan = vec![step(&inc, "A/1.flac", &music, "B/1.flac"), step(&inc, "A/2.flac", &music, "C/2.flac")];
        apply(&plan).unwrap_err();
        assert_eq!(names_in(&inc.join("A")), ["1.flac", "2.flac"], "no part left beside it");
        assert_eq!(std::fs::read(inc.join("A/1.flac")).unwrap(), b"1", "the original stands");
        assert_eq!(std::fs::read(music.join("B/1.flac")).unwrap(), b"1", "and so does what stood at its target");
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// A stop in the repeat of a stopped plan: still one step at both ends, or the next repeat would be refused.
    #[test]
    fn a_plan_stopped_twice_is_still_finished_by_a_repeat() {
        let (inc, music) = (tmp(), tmp());
        std::fs::create_dir_all(inc.join("A")).unwrap();
        for n in ["1", "2", "3"] {
            std::fs::write(inc.join(format!("A/{n}.flac")), n).unwrap();
        }
        std::fs::create_dir_all(music.join("B")).unwrap();
        std::fs::write(music.join("B/1.flac"), b"1").unwrap();
        let plan: Vec<Step> = ["1", "2", "3"].map(|n| step(&inc, &format!("A/{n}.flac"), &music, &format!("B/{n}.flac"))).to_vec();
        // The repeat's first step finished and a stop in its second: as `apply` leaves it, the first original already gone.
        apply(&plan[..1]).unwrap();
        std::fs::write(music.join("B/2.flac"), b"2").unwrap();
        assert_eq!(names_in(&inc.join("A")), ["2.flac", "3.flac"]);
        apply(&plan).unwrap();
        assert!(!inc.join("A").exists());
        assert_eq!(names_in(&music.join("B")), ["1.flac", "2.flac", "3.flac"]);
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// A new file alike to the old one it replaces: the old one must go to the trash, not count as this one arrived.
    #[test]
    fn a_target_an_earlier_step_moves_away_is_no_finished_copy_however_alike() {
        let (inc, music) = (tmp(), tmp());
        std::fs::create_dir_all(inc.join("A")).unwrap();
        std::fs::create_dir_all(music.join("X")).unwrap();
        std::fs::write(inc.join("A/1.flac"), b"same").unwrap();
        std::fs::write(music.join("X/01.flac"), b"same").unwrap();
        let plan = vec![step(&music, "X/01.flac", &inc, ".cratarr-trash/100/X/01.flac"), step(&inc, "A/1.flac", &music, "X/01.flac")];
        // Twice: the repeat finds the file at both ends of the first step, and the second step's target is its source.
        apply(&plan).unwrap();
        apply(&plan).unwrap();
        assert_eq!(std::fs::read(inc.join(".cratarr-trash/100/X/01.flac")).unwrap(), b"same");
        assert_eq!(std::fs::read(music.join("X/01.flac")).unwrap(), b"same");
        assert!(!inc.join("A").exists());
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn a_plan_that_fails_leaves_no_folder_at_its_target() {
        let (inc, music) = (tmp(), tmp());
        std::fs::create_dir_all(inc.join("A")).unwrap();
        std::fs::write(inc.join("A/1.flac"), b"1").unwrap();
        std::fs::write(inc.join("A/2.flac"), b"2").unwrap();
        // A file where the second one's folder should be: the look before moving does not see it, the move fails.
        std::fs::write(music.join("C"), b"a file").unwrap();
        let plan = vec![step(&inc, "A/1.flac", &music, "New/Album/1.flac"), step(&inc, "A/2.flac", &music, "C/2.flac")];
        apply(&plan).unwrap_err();
        assert!(!music.join("New").exists(), "a folder left would refuse every retry");
        assert_eq!(std::fs::read(inc.join("A/1.flac")).unwrap(), b"1", "the first went back");
        assert_eq!(std::fs::read(inc.join("A/2.flac")).unwrap(), b"2");
        for d in [inc, music] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn clearing_a_plan_given_up_takes_its_part_and_the_folders_made_for_it() {
        let (inc, music) = (tmp(), tmp());
        std::fs::create_dir_all(music.join("New/Album")).unwrap();
        std::fs::write(part_of(&music.join("New/Album/1.flac")), b"so").unwrap();
        std::fs::create_dir_all(music.join("Kept")).unwrap();
        std::fs::write(music.join("Kept/other.flac"), b"not this plan's").unwrap();
        clear_parts(&[step(&inc, "A/1.flac", &music, "New/Album/1.flac"), step(&inc, "A/2.flac", &music, "Kept/2.flac")]);
        assert!(!music.join("New").exists());
        assert_eq!(names_in(&music.join("Kept")), ["other.flac"]);
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
