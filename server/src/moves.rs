//! Moving an album's files between Cratarr's folders (Keep and Replace, decision 0013): every file of a plan goes, or
//! none does. A rename where the two folders share a file system (instant); a copy then delete where they do not.
//! Folders left empty behind are removed, up to the folder's root. Copying one (Copy to, C38): every file of a plan is
//! copied, or none is, each by the kernel's copy (`copy_file_range`, which btrfs makes a copy sharing the original's blocks,
//! across bind mounts of one file system too, where FICLONE answers EXDEV), else byte by byte. Blocking file operations.

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
    #[cfg(test)]
    AFTER_CHECK.with(|h| {
        if let Some(f) = h.take() {
            f()
        }
    });
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

#[cfg(test)]
thread_local! {
    /// What runs between `apply`'s look at its targets and its first move (T11.3's test): a file coming meanwhile.
    static AFTER_CHECK: std::cell::Cell<Option<Box<dyn FnOnce()>>> = const { std::cell::Cell::new(None) };
}

/// A rename that never replaces what is at `to` (T11): where a file came there since it was looked for, it fails
/// (`AlreadyExists`) and both stay. Linux's `renameat2(RENAME_NOREPLACE)`; where the file system does not know that flag,
/// `link_into`. Across file systems it fails as a rename does (`CrossesDevices`). Blocking.
pub fn rename_new(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = |p: &Path| std::ffi::CString::new(p.as_os_str().as_bytes()).map_err(std::io::Error::other);
        let (f, t) = (c(from)?, c(to)?);
        const RENAME_NOREPLACE: libc::c_uint = 1;
        // SAFETY: two NUL-ended paths that live through the call, and the flag renameat2 takes.
        let done = unsafe { libc::syscall(libc::SYS_renameat2, libc::AT_FDCWD, f.as_ptr(), libc::AT_FDCWD, t.as_ptr(), RENAME_NOREPLACE) };
        if done == 0 {
            return Ok(());
        }
        let e = std::io::Error::last_os_error();
        if !matches!(e.raw_os_error(), Some(libc::EINVAL | libc::ENOSYS | libc::EOPNOTSUPP)) {
            return Err(e);
        }
    }
    link_into(from, to)
}

/// `rename_new` where the file system knows no `RENAME_NOREPLACE`: a hard link, which refuses an existing name, then the old
/// name removed. Where links fail too, the step fails (OQ-L): never a rename that could replace. Blocking.
fn link_into(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::hard_link(from, to)?;
    std::fs::remove_file(from)
}

/// The folder `path` is in, made sure of on disk once a name is placed in it (T13.4, run 10's review: a power cut after the
/// rename could lose a copy the journal calls applied). Best effort: a file system that cannot sync a folder leaves the copy as
/// it is. Blocking.
fn sync_folder_of(path: &Path) {
    #[cfg(unix)]
    {
        if let Some(dir) = path.parent() {
            std::fs::File::open(dir).and_then(|d| d.sync_all()).ok();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn move_file(from: &Path, to: &Path) -> Result<()> {
    create_dirs_like_parent(to.parent().context("target has no folder")?)?;
    match rename_new(from, to) {
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
        rename_new(&part, to)?;
        sync_folder_of(to);
        Ok(())
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

/// Copy every file of the plan (C38, Copy to): all or none. Each comes through a hidden name (`part_of`) and is renamed into
/// place, its modification time the original's, so the walker reads nothing new. Nothing is overwritten: a target there already
/// holding the original's bytes is a step done (a plan cut short, tried again), any other refuses the plan before a file is
/// copied. A copy that fails takes away what this made, folders too. Which copies share the original's blocks (`shared`), in the
/// plan's order. Blocking.
pub fn copy(plan: &[Step]) -> Result<Vec<bool>> {
    let all: Vec<(PathBuf, PathBuf)> = plan.iter().map(|s| Ok((full(&s.from)?, full(&s.to)?))).collect::<Result<_>>()?;
    let mut todo = Vec::new();
    for (from, to) in &all {
        if !from.is_file() {
            bail!("{} is not there", from.display());
        }
        if to.exists() {
            if to.is_file() && same_bytes(from, to)? {
                continue;
            }
            bail!("{} exists already; not overwriting it", to.display());
        }
        todo.push((from, to));
    }
    let mut made: Vec<&PathBuf> = Vec::new();
    for (from, to) in todo {
        if let Err(e) = clone_file(from, to) {
            for to in made {
                std::fs::remove_file(to).ok();
            }
            for s in plan {
                prune_empty(&s.to.0, &s.to.1);
            }
            return Err(e);
        }
        made.push(to);
    }
    Ok(all.iter().map(|(_, to)| shared(to)).collect())
}

/// One file copied (C38): by the kernel's copy where it can (`std::fs::copy` asks for `copy_file_range` first: btrfs shares the
/// blocks), else byte by byte, into a hidden name beside its target, made sure of on disk, its modification time the original's,
/// then renamed into place, its folder made sure of too. A read-only source copies (T13.1): the part, which takes the source's
/// mode, is never opened to write. Its folders made like their neighbours. On any error what this made goes. Whether it shares
/// the original's blocks, as the file system says (`shared`). Blocking.
pub fn clone_file(from: &Path, to: &Path) -> Result<bool> {
    create_dirs_like_parent(to.parent().context("target has no folder")?)?;
    let part = part_of(to);
    let placed = (|| -> Result<()> {
        std::fs::copy(from, &part).with_context(|| format!("copying {} to {}", from.display(), to.display()))?;
        let at = std::fs::metadata(from)?.modified()?;
        // To read alone where that sets its time (its owner may, on any handle): a part of mode 0444 refuses an open to write.
        let file = std::fs::OpenOptions::new().read(true).write(cfg!(not(unix))).open(&part)?;
        file.set_modified(at)?;
        file.sync_all()?;
        rename_new(&part, to)?;
        sync_folder_of(to);
        Ok(())
    })();
    if let Err(e) = placed {
        std::fs::remove_file(&part).ok();
        return Err(e);
    }
    Ok(shared(to))
}

/// Whether every block of a file is shared with another file (C38), as the file system says (Linux's FIEMAP, its extents' shared
/// flag): a copy btrfs cloned. False where it cannot say: an empty file, another system, a file system that keeps no such flag.
pub fn shared(path: &Path) -> bool {
    #[cfg(target_os = "linux")]
    {
        fiemap::all_shared(path).unwrap_or(false)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        false
    }
}

/// Linux's FIEMAP read for its extents' shared flag (`linux/fiemap.h`), the structures as the kernel lays them out.
#[cfg(target_os = "linux")]
mod fiemap {
    use std::os::fd::AsRawFd;

    const EXTENTS: usize = 64;
    /// `_IOWR('f', 11, struct fiemap)`.
    const FS_IOC_FIEMAP: u32 = 0xC020_660B;
    const FIEMAP_FLAG_SYNC: u32 = 0x1;
    const FIEMAP_EXTENT_LAST: u32 = 0x1;
    const FIEMAP_EXTENT_SHARED: u32 = 0x2000;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Extent {
        logical: u64,
        physical: u64,
        length: u64,
        reserved64: [u64; 2],
        flags: u32,
        reserved: [u32; 3],
    }

    #[repr(C)]
    struct Map {
        start: u64,
        length: u64,
        flags: u32,
        mapped: u32,
        count: u32,
        reserved: u32,
        extents: [Extent; EXTENTS],
    }

    /// Whether the file has extents and every one is shared; None where the file system will not say.
    pub fn all_shared(path: &std::path::Path) -> Option<bool> {
        let file = std::fs::File::open(path).ok()?;
        let (mut start, mut any) = (0u64, false);
        loop {
            let mut map = Map { start, length: u64::MAX - start, flags: FIEMAP_FLAG_SYNC, mapped: 0, count: EXTENTS as u32, reserved: 0, extents: [Extent::default(); EXTENTS] };
            // SAFETY: `map` is a `struct fiemap` with room for `EXTENTS` extents, as `count` says, and lives through the call. The
            // request is passed as its bits: an int on musl, an unsigned long on glibc.
            let done = unsafe { libc::ioctl(file.as_raw_fd(), FS_IOC_FIEMAP as _, &mut map as *mut Map) };
            if done != 0 {
                return None;
            }
            let found = &map.extents[..(map.mapped as usize).min(EXTENTS)];
            if found.is_empty() {
                return Some(any);
            }
            for e in found {
                if e.flags & FIEMAP_EXTENT_SHARED == 0 {
                    return Some(false);
                }
                any = true;
                if e.flags & FIEMAP_EXTENT_LAST != 0 {
                    return Some(true);
                }
            }
            let last = found[found.len() - 1];
            start = last.logical + last.length;
        }
    }
}

/// The hidden folder the share probe works in, inside each of the two folders it tries (T13.2): out of every walk's sight, and
/// taken away whole after each probe and before it, so what a probe a stop cut short left goes at the next.
const PROBE_DIR: &str = ".cratarr-probe";

#[cfg(test)]
thread_local! {
    /// What runs while the share probe's files are there (T13.2's test): a look at the folders.
    static MID_PROBE: std::cell::Cell<Option<Box<dyn FnOnce()>>> = const { std::cell::Cell::new(None) };
}

/// Whether a copy from folder `from` into folder `to` shares its blocks (C38), tried once with a small file of its own each way
/// and kept for the server's life, as `mounts::writable` tries a write: a copy made between two libraries on one btrfs shares, and
/// costs no room (CP5). It works in a hidden folder of its own in each (`PROBE_DIR`) and leaves nothing behind. A folder not there
/// is never made for it (T13.2): false, tried again once it is there. False where it cannot be tried. Blocking.
pub fn shares(from: &Path, to: &Path) -> bool {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static KEPT: OnceLock<Mutex<HashMap<(PathBuf, PathBuf), bool>>> = OnceLock::new();
    // One probe at a time: each clears what an earlier one left in the folders it tries, which another pair may share.
    static PROBING: Mutex<()> = Mutex::new(());
    let key = (from.to_path_buf(), to.to_path_buf());
    if let Some(known) = KEPT.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner()).get(&key) {
        return *known;
    }
    if !from.is_dir() || !to.is_dir() {
        return false;
    }
    let _one = PROBING.lock().unwrap_or_else(|p| p.into_inner());
    let (source, copy) = (from.join(PROBE_DIR), to.join(PROBE_DIR));
    let clear = || {
        std::fs::remove_dir_all(&source).ok();
        std::fs::remove_dir_all(&copy).ok();
    };
    // What a probe a stop cut short left, this process's or an earlier one's.
    clear();
    let name = format!("probe-{}", std::process::id());
    // Each hidden folder made in its folder alone, never a folder above it. A few blocks' worth: a file of a few bytes may live
    // inside the file system's own records, never in a block of its own.
    let tried = std::fs::create_dir(&source).is_ok()
        && std::fs::create_dir(&copy).is_ok()
        && std::fs::write(source.join(&name), vec![7u8; 64 << 10]).is_ok()
        && clone_file(&source.join(&name), &copy.join(&name)).unwrap_or(false);
    #[cfg(test)]
    MID_PROBE.with(|h| {
        if let Some(f) = h.take() {
            f()
        }
    });
    clear();
    KEPT.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner()).insert(key, tried);
    tried
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

    /// C38: a copy holds the original's bytes and its modification time, comes through a hidden name and leaves none, and the
    /// original stays. Tried again, a target holding the same bytes is a step done; one holding others refuses the plan before a
    /// file is copied.
    #[test]
    fn a_copy_has_the_sources_time_and_bytes() {
        let (music, own) = (tmp(), tmp());
        std::fs::create_dir_all(music.join("A/Album")).unwrap();
        for (n, body) in [("1.flac", b"one".as_slice()), ("2.flac", b"two".as_slice())] {
            std::fs::write(music.join("A/Album").join(n), body).unwrap();
        }
        let at = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
        std::fs::OpenOptions::new().write(true).open(music.join("A/Album/1.flac")).unwrap().set_modified(at).unwrap();
        let plan = [step(&music, "A/Album/1.flac", &own, "A/Album/1.flac"), step(&music, "A/Album/2.flac", &own, "A/Album/2.flac")];
        assert_eq!(copy(&plan).unwrap().len(), 2);
        assert_eq!(std::fs::read(own.join("A/Album/1.flac")).unwrap(), b"one");
        assert_eq!(std::fs::read(own.join("A/Album/2.flac")).unwrap(), b"two");
        assert_eq!(std::fs::metadata(own.join("A/Album/1.flac")).unwrap().modified().unwrap(), at, "the original's time");
        assert!(music.join("A/Album/1.flac").is_file(), "the original stays");
        assert_eq!(names_in(&own.join("A/Album")), ["1.flac", "2.flac"], "no hidden part left");
        assert!(copy(&plan).is_ok(), "tried again: done already");
        std::fs::write(own.join("A/Album/2.flac"), b"other").unwrap();
        std::fs::remove_file(own.join("A/Album/1.flac")).unwrap();
        assert!(copy(&plan).is_err(), "another file there: refused");
        assert!(!own.join("A/Album/1.flac").exists(), "refused before a file was copied");
    }

    /// The folder's names, a part's hidden one too.
    fn all_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    /// T11.1: a copy whose target came to be between its check and its placing never replaces it: refused, the file that came
    /// kept byte for byte, no part left.
    #[test]
    fn a_copy_never_replaces_a_file_that_came_meanwhile() {
        let (music, own) = (tmp(), tmp());
        std::fs::create_dir_all(music.join("A")).unwrap();
        std::fs::create_dir_all(own.join("A")).unwrap();
        std::fs::write(music.join("A/1.flac"), b"copied").unwrap();
        std::fs::write(own.join("A/1.flac"), b"came meanwhile").unwrap();
        let e = clone_file(&music.join("A/1.flac"), &own.join("A/1.flac")).unwrap_err();
        assert!(format!("{e:#}").contains("exists"), "{e:#}");
        assert_eq!(std::fs::read(own.join("A/1.flac")).unwrap(), b"came meanwhile");
        assert_eq!(all_in(&own.join("A")), ["1.flac"], "no part left");
        assert_eq!(std::fs::read(music.join("A/1.flac")).unwrap(), b"copied");
    }

    /// T11.2: a move on one file system, and a copy across, alike: refused, the target's file and the source kept.
    #[test]
    fn a_move_never_replaces_a_file_that_came_meanwhile() {
        let (music, own) = (tmp(), tmp());
        std::fs::create_dir_all(music.join("A")).unwrap();
        std::fs::create_dir_all(own.join("A")).unwrap();
        std::fs::write(music.join("A/1.flac"), b"moving").unwrap();
        std::fs::write(own.join("A/1.flac"), b"came meanwhile").unwrap();
        assert!(move_file(&music.join("A/1.flac"), &own.join("A/1.flac")).is_err(), "a move refused");
        assert!(copy_across(&music.join("A/1.flac"), &own.join("A/1.flac"), |f| std::fs::remove_file(f)).is_err(), "a copy across refused");
        assert_eq!(std::fs::read(own.join("A/1.flac")).unwrap(), b"came meanwhile");
        assert_eq!(std::fs::read(music.join("A/1.flac")).unwrap(), b"moving", "the source kept");
        assert_eq!(all_in(&own.join("A")), ["1.flac"], "no part left");
    }

    /// T11.3: a Keep's plan of two files meeting a file that came to its second target after the look: refused, the first file
    /// back where it was, the file that came untouched, the folder made for the plan pruned where empty.
    #[test]
    fn a_plan_that_meets_a_file_that_came_meanwhile_puts_back_what_it_moved() {
        let (inc, music) = (tmp(), tmp());
        std::fs::create_dir_all(inc.join("X")).unwrap();
        std::fs::write(inc.join("X/1.flac"), b"one").unwrap();
        std::fs::write(inc.join("X/2.flac"), b"two").unwrap();
        let plan = [step(&inc, "X/1.flac", &music, "A/B/1.flac"), step(&inc, "X/2.flac", &music, "A/C/2.flac")];
        let came = music.join("A/C/2.flac");
        AFTER_CHECK.with(|h| {
            h.set(Some(Box::new(move || {
                std::fs::create_dir_all(came.parent().unwrap()).unwrap();
                std::fs::write(&came, b"came meanwhile").unwrap();
            })))
        });
        assert!(apply(&plan).is_err(), "refused");
        assert_eq!(std::fs::read(inc.join("X/1.flac")).unwrap(), b"one", "the first file back");
        assert_eq!(std::fs::read(inc.join("X/2.flac")).unwrap(), b"two");
        assert_eq!(std::fs::read(music.join("A/C/2.flac")).unwrap(), b"came meanwhile", "the file that came untouched");
        assert!(!music.join("A/B").exists(), "the folder made for the plan pruned");
    }

    /// T11.4: where the file system will not refuse a name by its rename, the fallback places by a hard link, which refuses an
    /// existing name, never by a plain rename.
    #[test]
    fn placing_falls_back_to_a_link_never_to_a_rename() {
        let d = tmp();
        std::fs::write(d.join("part"), b"new").unwrap();
        std::fs::write(d.join("taken"), b"there").unwrap();
        assert_eq!(link_into(&d.join("part"), &d.join("taken")).unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(d.join("taken")).unwrap(), b"there", "never replaced");
        assert!(d.join("part").is_file(), "the part kept for its caller to take away");
        link_into(&d.join("part"), &d.join("free")).unwrap();
        assert_eq!((std::fs::read(d.join("free")).unwrap(), d.join("part").exists()), (b"new".to_vec(), false), "a free name: placed");
    }

    /// T13.1 (run 10's review, the nits): a source of mode 0444 is copied, its bytes, its time and its mode kept; the part
    /// `std::fs::copy` gives the source's mode is never opened to write. The tests run as root in the container, where a mode refuses
    /// no open: the copy runs on a thread of its own as another account (`setresuid` by the system call, which changes that thread
    /// alone, not libc's wrapper, which changes them all), so a write's open is refused there as it is for any account but root.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_read_only_source_is_copied() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let d = tmp();
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o777)).unwrap();
        let (from, to) = (d.join("01 Slack Water.flac"), d.join("Copy/01 Slack Water.flac"));
        std::fs::write(&from, b"read only").unwrap();
        let at = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
        std::fs::OpenOptions::new().write(true).open(&from).unwrap().set_modified(at).unwrap();
        std::fs::set_permissions(&from, std::fs::Permissions::from_mode(0o444)).unwrap();
        let (f, t) = (from.clone(), to.clone());
        let copied = std::thread::spawn(move || {
            // SAFETY: plain numbers to a system call that changes the calling thread's account alone; the thread ends with the copy.
            if unsafe { libc::geteuid() } == 0 {
                let nobody: libc::uid_t = 65_534;
                let done = unsafe { libc::syscall(libc::SYS_setresuid, nobody, nobody, nobody) };
                assert_eq!(done, 0, "another account: {}", std::io::Error::last_os_error());
            }
            clone_file(&f, &t).map_err(|e| format!("{e:#}"))
        })
        .join()
        .unwrap();
        assert!(copied.is_ok(), "copied: {copied:?}");
        let meta = std::fs::metadata(&to).unwrap();
        assert_eq!((std::fs::read(&to).unwrap(), meta.modified().unwrap(), meta.mode() & 0o777), (b"read only".to_vec(), at, 0o444), "its bytes, its time and its mode");
        assert_eq!(all_in(&d.join("Copy")), ["01 Slack Water.flac"], "no part left");
        std::fs::remove_dir_all(&d).ok();
    }

    /// T13.2 (run 10's review, the nits): the share probe works in a hidden folder of its own in each folder it tries and takes
    /// both away; a target folder not there is not made (nothing tried: false); and what a probe a stop cut short left, another
    /// process's, goes at the next probe.
    #[test]
    fn the_share_probe_leaves_nothing_and_makes_no_root() {
        let (music, own) = (tmp(), tmp());
        std::fs::create_dir_all(music.join("A/Album")).unwrap();
        std::fs::write(music.join("A/Album/1.flac"), b"one").unwrap();
        let missing = own.join("ann");
        assert!(!shares(&music, &missing), "a target not there: nothing tried");
        assert!(!missing.exists(), "its root not made");
        assert_eq!(all_in(&music), ["A"], "nothing written into the source");
        for d in [&music, &own] {
            std::fs::create_dir_all(d.join(PROBE_DIR)).unwrap();
            std::fs::write(d.join(PROBE_DIR).join("probe-0"), b"left by a stop").unwrap();
        }
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let (m, o, s) = (music.clone(), own.clone(), seen.clone());
        MID_PROBE.with(|h| h.set(Some(Box::new(move || s.borrow_mut().extend([all_in(&m), all_in(&m.join(PROBE_DIR)), all_in(&o), all_in(&o.join(PROBE_DIR))])))));
        shares(&music, &own);
        let probe = format!("probe-{}", std::process::id());
        assert_eq!(*seen.borrow(), [vec![PROBE_DIR.to_string(), "A".into()], vec![probe.clone()], vec![PROBE_DIR.to_string()], vec![probe]], "in its hidden folders alone, what a stop left gone");
        assert_eq!((all_in(&music), all_in(&own)), (vec!["A".to_string()], Vec::<String>::new()), "nothing left");
        for d in [music, own] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// C38: where the file system shares no blocks (the tests' own, ext4 on staging), the copy says so: a copy of its own, which
    /// costs room; and so does the kept probe between two such folders.
    #[test]
    fn a_copy_that_cannot_share_says_so() {
        let (a, b) = (tmp(), tmp());
        std::fs::write(a.join("1.flac"), vec![1u8; 256 << 10]).unwrap();
        assert!(!clone_file(&a.join("1.flac"), &b.join("1.flac")).unwrap(), "not shared");
        assert!(!shared(&a.join("1.flac")), "nor the original");
        assert!(!shares(&a, &b), "the probe neither");
        assert!(names_in(&a) == ["1.flac"] && names_in(&b) == ["1.flac"], "the probe leaves nothing");
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
