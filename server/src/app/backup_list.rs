//! The backup's list of shared copies (C41, design step 6): the copies in someone's own music whose blocks are their original's in
//! Music (Copy to on one btrfs, `files.shared`), so the NAS's backup, which keeps Music, need not keep their bytes twice. A copy
//! whose original left Music (deleted and its trash emptied, or moved out) is its own, and leaves the list; its Undo, or the
//! original's, puts it back, as the list is written anew from the catalogue each time: after a copy and its Undo, and at the end
//! of every library build, which reads what a Delete, a move or an emptied trash changed (no timer of its own). Music is the first main library (Q16), and
//! an original in its trash counts as there still. A plain list (Q16): one copy's path a line, as Cratarr sees it, in the data
//! folder (`LIST`). Nothing here reads it; the NAS's part comes with the release.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::catalogue::Catalogue;
use crate::serve::Serve;

/// The list's file, in the data folder.
pub const LIST: &str = "backup-shared-copies.txt";

/// The shared copies' paths (`Catalogue::shared_copies`) of an original in catalogue folder `main`, each as Cratarr sees it. Blocking.
pub fn paths(cat: &Catalogue, main: i64) -> Result<Vec<PathBuf>> {
    let folders = cat.folders();
    Ok(cat.shared_copies(main)?.into_iter().filter_map(|(folder, rel)| Some(folders.get(folder)?.join(rel))).collect())
}

/// The list written anew in `data`: through a part of this writer's own (`part_in`), made sure of on disk, then renamed, so two
/// writes at once (a copy's refresh and a build's) each leave a whole list, and a crash leaves the old one or the new (T12). On
/// any error the part goes and the old list stays. How many copies. Blocking.
pub fn write(cat: &Catalogue, main: i64, data: &Path) -> Result<usize> {
    use std::io::Write;
    let paths = paths(cat, main)?;
    let text: String = paths.iter().map(|p| format!("{}\n", p.display())).collect();
    let (to, part) = (data.join(LIST), part_in(data));
    let written = (|| -> Result<()> {
        let mut file = std::fs::File::create(&part)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        Ok(std::fs::rename(&part, &to)?)
    })();
    if written.is_err() && part.is_file() {
        std::fs::remove_file(&part).ok();
    }
    written.map(|()| paths.len())
}

/// A write's part in `data`: the process's and the writing thread's, so two writes at once never share one (T12).
fn part_in(data: &Path) -> PathBuf {
    let thread: String = format!("{:?}", std::thread::current().id()).chars().filter(char::is_ascii_digit).collect();
    data.join(format!("{LIST}.{}-{thread}.part", std::process::id()))
}

/// The list written anew for the server (`served`): Music the first main library. Nothing where there is no data folder or no
/// main library. Blocking.
pub fn write_for(served: &Serve) -> Result<usize> {
    let (Some(data), Some(main)) = (served.data_dir.as_deref(), served.library.kinds().iter().find(|(_, k)| *k == crate::settings::LibraryKind::Main).map(|(f, _)| *f)) else {
        return Ok(0);
    };
    write(served.catalogue(), main, data)
}

/// The list written anew in the background, after a copy is made or undone; a failure goes to the log.
pub fn refresh() {
    let Some(served) = crate::serve::instance() else { return };
    tokio::task::spawn_blocking(move || {
        if let Err(e) = write_for(&served) {
            tracing::warn!(target: "library", "the backup's list of shared copies was not written: {e:#}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C41: a shared copy of an original in Music is listed; one not shared (written since, or copied byte by byte) is not; an
    /// original in Music's trash counts as there still; a copy whose original left Music is its own, and leaves the list.
    #[test]
    fn a_copy_whose_original_left_is_its_own() {
        let cat = Catalogue::open(None, vec![PathBuf::from("/music"), PathBuf::from("/own")]).unwrap();
        let record = cat.record_for_tests("Tide Tables", 2);
        cat.copy_for_tests(record, 0);
        let rows: Vec<((i64, String), (i64, String), bool)> =
            [(1, true), (2, false)].iter().map(|(n, shared)| ((0, format!("copy-{record}/{n}.flac")), (1, format!("Own/{n}.flac")), *shared)).collect();
        cat.copy_rows(&rows).unwrap();
        let listed = || paths(&cat, 0).unwrap();
        assert_eq!(listed(), [PathBuf::from("/own/Own/1.flac")], "the shared copy alone");
        cat.mark_trashed(0, &format!("copy-{record}"), &format!("100/copy-{record}")).unwrap();
        assert_eq!(listed().len(), 1, "an original in Music's trash is there still");
        cat.move_files(0, &format!(".cratarr-trash/100/copy-{record}"), 1, "Elsewhere").unwrap();
        assert!(listed().is_empty(), "its original left Music: the copy is its own");
        let data = std::env::temp_dir().join(format!("cratarr-backup-list-{}", std::process::id()));
        std::fs::create_dir_all(&data).unwrap();
        assert_eq!(write(&cat, 0, &data).unwrap(), 0);
        assert_eq!(std::fs::read_to_string(data.join(LIST)).unwrap(), "");
        std::fs::remove_dir_all(&data).ok();
    }

    /// Music's record of one track (`copy-<record>/1.flac`, size 1, time 1) and its copy in own music sharing its blocks: listed.
    fn one_shared_copy() -> (Catalogue, i64) {
        let cat = Catalogue::open(None, vec![PathBuf::from("/music"), PathBuf::from("/own")]).unwrap();
        let record = cat.record_for_tests("Tide Tables", 1);
        cat.copy_for_tests(record, 0);
        cat.copy_rows(&[((0, format!("copy-{record}/1.flac")), (1, "Own/1.flac".into()), true)]).unwrap();
        assert_eq!(paths(&cat, 0).unwrap(), [PathBuf::from("/own/Own/1.flac")], "listed");
        (cat, record)
    }

    /// T2.1: Replace gives Music's track to a new file of another size and time and lets the old one go: the copy in own music no
    /// longer shares its blocks with anything in Music, and is its own.
    #[test]
    fn a_copy_whose_original_was_replaced_is_its_own() {
        let (cat, record) = one_shared_copy();
        cat.copy_rows(&[((0, format!("copy-{record}/1.flac")), (0, "New/1.flac".into()), false)]).unwrap();
        cat.execute_for_tests("UPDATE files SET size = 200, mtime = 20 WHERE folder = 0 AND rel = 'New/1.flac'");
        cat.set_tracks(&[((0, format!("copy-{record}/1.flac")), None), ((0, "New/1.flac".into()), Some(1))]).unwrap();
        assert!(paths(&cat, 0).unwrap().is_empty(), "its own: backed up");
    }

    /// T2.2: Music's original rewritten in place (a tag write: another size and time on its row): the copy is its own, its own
    /// flag as it was, as the copy itself did not change.
    #[test]
    fn a_copy_whose_original_was_rewritten_is_its_own() {
        let (cat, record) = one_shared_copy();
        cat.execute_for_tests(&format!("UPDATE files SET size = 104, mtime = 30 WHERE folder = 0 AND rel = 'copy-{record}/1.flac'"));
        assert!(paths(&cat, 0).unwrap().is_empty(), "its own: backed up");
        cat.execute_for_tests(&format!("UPDATE files SET size = 1, mtime = 1 WHERE folder = 0 AND rel = 'copy-{record}/1.flac'"));
        assert_eq!(paths(&cat, 0).unwrap().len(), 1, "the copy's own flag untouched: as before, listed again");
    }

    /// A catalogue whose Music record of `n` tracks has a copy in own music sharing its blocks, each track's: `n` lines listed.
    fn shared(n: usize) -> Catalogue {
        let cat = Catalogue::open(None, vec![PathBuf::from("/music"), PathBuf::from("/own")]).unwrap();
        let record = cat.record_for_tests("Tide Tables", n);
        cat.copy_for_tests(record, 0);
        let rows: Vec<((i64, String), (i64, String), bool)> = (1..=n).map(|t| ((0, format!("copy-{record}/{t}.flac")), (1, format!("Own/{t}.flac")), true)).collect();
        cat.copy_rows(&rows).unwrap();
        cat
    }

    /// The parts left in a data folder.
    fn parts(data: &Path) -> Vec<String> {
        std::fs::read_dir(data).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".part")).collect()
    }

    /// T12.1: twenty writers at once, of a list of 50 copies and one of 2, into one data folder: the list is one of the two whole,
    /// each line a full path ending in a newline, and no part is left.
    #[test]
    fn two_writes_at_once_leave_a_whole_list() {
        let (fifty, two) = (shared(50), shared(2));
        let data = std::env::temp_dir().join(format!("cratarr-backup-at-once-{}", std::process::id()));
        std::fs::create_dir_all(&data).unwrap();
        // Each list whole, as it alone would be written: its copies' paths in the catalogue's order.
        let text = |cat: &Catalogue| paths(cat, 0).unwrap().iter().map(|p| format!("{}\n", p.display())).collect::<String>();
        std::thread::scope(|s| {
            for i in 0..20 {
                let (cat, data) = (if i % 2 == 0 { &fifty } else { &two }, &data);
                s.spawn(move || {
                    for _ in 0..20 {
                        write(cat, 0, data).unwrap();
                    }
                });
            }
        });
        let list = std::fs::read_to_string(data.join(LIST)).unwrap();
        assert!(list == text(&fifty) || list == text(&two), "one whole list: {} lines", list.lines().count());
        assert!(parts(&data).is_empty(), "no part left: {:?}", parts(&data));
        std::fs::remove_dir_all(&data).ok();
    }

    /// T12.2: a write that fails (its part cannot be made) leaves the old list as it was, and no part.
    #[test]
    fn a_failed_write_keeps_the_old_list() {
        let data = std::env::temp_dir().join(format!("cratarr-backup-fails-{}", std::process::id()));
        std::fs::create_dir_all(&data).unwrap();
        assert_eq!(write(&shared(2), 0, &data).unwrap(), 2);
        let old = std::fs::read_to_string(data.join(LIST)).unwrap();
        std::fs::create_dir_all(part_in(&data)).unwrap();
        assert!(write(&shared(3), 0, &data).is_err(), "the part could not be made");
        assert_eq!(std::fs::read_to_string(data.join(LIST)).unwrap(), old, "the old list as it was");
        std::fs::remove_dir(part_in(&data)).unwrap();
        assert!(parts(&data).is_empty(), "no part left");
        std::fs::remove_dir_all(&data).ok();
    }

    /// C41: the list is written anew after a copy's Undo: the copy, its rows into its library's trash, leaves the list; written
    /// through a part, renamed, none left behind.
    #[test]
    fn the_list_is_written_anew_after_an_undo() {
        let cat = Catalogue::open(None, vec![PathBuf::from("/music"), PathBuf::from("/own")]).unwrap();
        let record = cat.record_for_tests("Tide Tables", 1);
        cat.copy_for_tests(record, 0);
        let track = 1; // The catalogue's first track: the record's one.
        cat.copy_rows(&[((0, format!("copy-{record}/{track}.flac")), (1, "Own/1.flac".into()), true)]).unwrap();
        let data = std::env::temp_dir().join(format!("cratarr-backup-undo-{}", std::process::id()));
        std::fs::create_dir_all(&data).unwrap();
        assert_eq!(write(&cat, 0, &data).unwrap(), 1);
        assert_eq!(std::fs::read_to_string(data.join(LIST)).unwrap(), "/own/Own/1.flac
");
        cat.mark_trashed(1, "Own", "200/Own").unwrap();
        assert_eq!(write(&cat, 0, &data).unwrap(), 0, "undone: in its library's trash");
        assert_eq!(std::fs::read_to_string(data.join(LIST)).unwrap(), "");
        assert!(!data.join(format!("{LIST}.part")).exists(), "no part left");
        std::fs::remove_dir_all(&data).ok();
    }
}
