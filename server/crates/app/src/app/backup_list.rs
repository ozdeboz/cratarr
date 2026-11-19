//! The backup's list of shared copies (C41, design step 6): the copies outside Music whose blocks are an original's kept in a main
//! library or anyone's own music (Copy to on one btrfs, `files.shared`; T2b), so the NAS's backup, which keeps the original, need
//! not keep their bytes twice. A copy whose original left (deleted, moved out, or into Incoming), or was replaced or rewritten,
//! is its own, and leaves the list; its Undo, or the original's, puts it back, as the list is written anew from the catalogue
//! each time: after a copy and its Undo, and at the end of every library build, which reads what a Delete, a move or an emptied
//! trash changed (no timer of its own). Music is the first main library (Q16). An original in a trash no longer covers its copy
//! (T2b: the backup errs on the safe side). A plain list (Q16): one copy's path a line, as Cratarr sees it, in the data folder
//! (`LIST`). Nothing here reads it; the NAS's part comes with the release.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::catalogue::Catalogue;
use crate::serve::Serve;

/// The list's file, in the data folder.
pub const LIST: &str = "backup-shared-copies.txt";

/// The paths of the shared copies outside catalogue folder `main` (`Catalogue::shared_copies`) of an original kept in one of the
/// folders `kept`, each as Cratarr sees it. Blocking.
pub fn paths(cat: &Catalogue, main: i64, kept: &[i64]) -> Result<Vec<PathBuf>> {
    let folders = cat.folders();
    Ok(cat.shared_copies(main, kept)?.into_iter().filter_map(|(folder, rel)| Some(folders.get(folder)?.join(rel))).collect())
}

/// The list written anew in `data`: through a part of this writer's own (`part_in`), made sure of on disk, then renamed, so two
/// writes at once (a copy's refresh and a build's) each leave a whole list, and a crash leaves the old one or the new (T12). On
/// any error the part goes and the old list stays. How many copies. Blocking.
pub fn write(cat: &Catalogue, main: i64, kept: &[i64], data: &Path) -> Result<usize> {
    use std::io::Write;
    let paths = paths(cat, main, kept)?;
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

/// The list written anew for the server (`served`): Music the first main library; the main libraries and anyone's own music
/// kept. Nothing where there is no data folder or no main library. Blocking.
pub fn write_for(served: &Serve) -> Result<usize> {
    use crate::settings::LibraryKind;
    let kinds = served.library.kinds();
    let (Some(data), Some(main)) = (served.data_dir.as_deref(), kinds.iter().find(|(_, k)| *k == LibraryKind::Main).map(|(f, _)| *f)) else {
        return Ok(0);
    };
    let kept: Vec<i64> = kinds.iter().filter(|(_, k)| *k != LibraryKind::Incoming).map(|(f, _)| *f).collect();
    write(served.catalogue(), main, &kept, data)
}

/// The list written anew in the background, after a copy is made or undone; a failure goes to the log.
pub fn refresh() {
    let Some(served) = crate::serve::instance() else { return };
    crate::run::spawn_blocking(move || {
        if let Err(e) = write_for(&served) {
            tracing::warn!(target: "library", "the backup's list of shared copies was not written: {e:#}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C41: a shared copy of an original in Music is listed; one not shared (written since, or copied byte by byte) is not; an
    /// original in Music's trash no longer covers it (T2b, which changed T2.3's "there still"), nor once it left Music: the copy
    /// is its own, and leaves the list.
    #[test]
    fn a_copy_whose_original_left_is_its_own() {
        // Folder 2 is Incoming: out of the kept libraries (T2b: an original moved into anyone's own music still covers its copy).
        let cat = Catalogue::open(None, vec![PathBuf::from("/music"), PathBuf::from("/own"), PathBuf::from("/incoming")]).unwrap();
        let record = cat.record_for_tests("Tide Tables", 2);
        cat.copy_for_tests(record, 0);
        let rows: Vec<((i64, String), (i64, String), bool)> =
            [(1, true), (2, false)].iter().map(|(n, shared)| ((0, format!("copy-{record}/{n}.flac")), (1, format!("Own/{n}.flac")), *shared)).collect();
        cat.copy_rows(&rows).unwrap();
        let listed = || paths(&cat, 0, &[0, 1]).unwrap();
        assert_eq!(listed(), [PathBuf::from("/own/Own/1.flac")], "the shared copy alone");
        cat.mark_trashed(0, &format!("copy-{record}"), &format!("100/copy-{record}")).unwrap();
        assert!(listed().is_empty(), "an original in Music's trash covers its copy no longer (T2b)");
        cat.move_files(0, &format!(".cratarr-trash/100/copy-{record}"), 2, "Elsewhere").unwrap();
        assert!(listed().is_empty(), "its original left the kept libraries: the copy is its own");
        let data = std::env::temp_dir().join(format!("cratarr-backup-list-{}", std::process::id()));
        std::fs::create_dir_all(&data).unwrap();
        assert_eq!(write(&cat, 0, &[0, 1], &data).unwrap(), 0);
        assert_eq!(std::fs::read_to_string(data.join(LIST)).unwrap(), "");
        std::fs::remove_dir_all(&data).ok();
    }

    /// RY01.11, on the catalogue alone with the rows a Replace from Bob's copy makes and its Undo puts back (`rows_of_copy`,
    /// `rows_back_of_copy`). Case 1: Ann's MP3 is a Copy to of Music's MP3, listed; after the Replace (its copies sharing blocks, as
    /// the disk said) Music holds no original of its size and time on its track, and it leaves the list; after the Undo it is back.
    /// Bob's rows keep `shared = 0` and are never listed (OQ-E); Music's are never listed (they are Music's). Case 2, as T2b has it
    /// (an original in anyone's own music covers its copy, and a copy never holds another): Ann's FLAC, a Want's copy of Bob's, is
    /// covered by Bob's original before, after and after the Undo, Music's new rows being copies that hold nothing. Case 3 (the
    /// coordinator, T2b's own rule): as case 2, then Bob's original trashed after the Replace: it covers neither Ann's copy nor
    /// Music's new rows, and Ann's leaves the list, backed up.
    #[test]
    fn the_backup_list_follows_a_replace_from_a_copy() {
        use crate::journal::{Event, TrackMove};
        let roots = ["/music", "/inc", "/ann", "/bob"].map(PathBuf::from);
        let kept = [0, 2, 3];
        for case in [1, 2, 3] {
            let cat = Catalogue::open(None, roots.to_vec()).unwrap();
            let record = cat.record_for_tests("Drift", 2);
            cat.copy_for_tests(record, 0);
            let tracks: Vec<(String, i64)> = {
                let db = cat.writer_for_tests();
                let mut st = db.prepare("SELECT rel, track FROM files WHERE folder = 0 ORDER BY rel").unwrap();
                st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
            };
            let bobs = |n: usize| format!("Bob/{n}.flac");
            cat.copy_rows(&tracks.iter().enumerate().map(|(n, (rel, _))| ((0, rel.clone()), (3, bobs(n)), false)).collect::<Vec<_>>()).unwrap();
            cat.execute_for_tests("UPDATE files SET size = 300, mtime = 30 WHERE folder = 3");
            let anns: Vec<((i64, String), (i64, String), bool)> = match case {
                1 => tracks.iter().enumerate().map(|(n, (rel, _))| ((0, rel.clone()), (2, format!("Ann/{n}.mp3")), true)).collect(),
                _ => (0..tracks.len()).map(|n| ((3, bobs(n)), (2, format!("Ann/{n}.flac")), true)).collect(),
            };
            cat.copy_rows(&anns).unwrap();
            let listed = || paths(&cat, 0, &kept).unwrap();
            let ann_listed = || listed().iter().filter(|p| p.starts_with("/ann")).count();
            assert_eq!(ann_listed(), 2, "case {case}: Ann's two copies listed before");
            let event = Event::AlbumReplacedFromCopy {
                source: "cratarr".into(),
                album_id: format!("al-{record}"),
                title: "Drift".into(),
                from_dir: (3, "Bob".into()),
                to_dir: (0, "Drift".into()),
                copies: tracks.iter().enumerate().map(|(n, (_, t))| TrackMove { track: Some(*t), from: (3, bobs(n)), to: (0, format!("Drift/{n}.flac")), was: None }).collect(),
                stamps: Vec::new(),
                replaced: tracks.iter().map(|(rel, t)| TrackMove { track: Some(*t), from: (0, rel.clone()), to: (0, format!(".cratarr-trash/100/{rel}")), was: None }).collect(),
                token: format!("100/copy-{record}"),
            };
            crate::app::incoming::rows_of_copy(&cat, &event, &[true, true]).unwrap();
            let shared = |folder: i64| -> Vec<i64> {
                let db = cat.writer_for_tests();
                let mut st = db.prepare("SELECT shared FROM files WHERE folder = ?1 AND trashed IS NULL ORDER BY rel").unwrap();
                st.query_map([folder], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
            };
            assert_eq!(shared(3), [0, 0], "case {case}: Bob's rows keep shared = 0 (OQ-E)");
            assert_eq!(shared(0), [1, 1], "case {case}: Music's new rows are copies, sharing blocks as the disk said");
            assert!(listed().iter().all(|p| !p.starts_with("/bob") && !p.starts_with("/music")), "case {case}: never Bob's nor Music's rows");
            match case {
                1 => assert_eq!(ann_listed(), 0, "Ann's MP3 is its own now: backed up"),
                _ => assert_eq!(ann_listed(), 2, "Ann's FLAC covered by Bob's original, as before (T2b)"),
            }
            if case == 3 {
                cat.mark_trashed(3, "Bob", "300/Bob").unwrap();
                let covered = cat.shared_copies(-1, &kept).unwrap();
                assert!(covered.iter().all(|(folder, _)| *folder != 2 && *folder != 0), "Bob's original in his trash covers neither Ann's copy nor Music's new rows: {covered:?}");
                assert_eq!(ann_listed(), 0, "Ann's copy leaves the list: backed up");
                continue;
            }
            let taken: Vec<(String, String)> = (0..tracks.len()).map(|n| (format!("Drift/{n}.flac"), format!("200/Drift/{n}.flac"))).collect();
            crate::app::incoming::rows_back_of_copy(&cat, &event, &taken).unwrap();
            assert_eq!(ann_listed(), 2, "case {case}: after the Undo, as before");
            assert_eq!(shared(3), [0, 0], "case {case}: Bob's untouched by the Undo");
        }
    }

    /// Music's record of one track (`copy-<record>/1.flac`, size 1, time 1) and its copy in own music sharing its blocks: listed.
    fn one_shared_copy() -> (Catalogue, i64) {
        let cat = Catalogue::open(None, vec![PathBuf::from("/music"), PathBuf::from("/own")]).unwrap();
        let record = cat.record_for_tests("Tide Tables", 1);
        cat.copy_for_tests(record, 0);
        cat.copy_rows(&[((0, format!("copy-{record}/1.flac")), (1, "Own/1.flac".into()), true)]).unwrap();
        assert_eq!(paths(&cat, 0, &[0, 1]).unwrap(), [PathBuf::from("/own/Own/1.flac")], "listed");
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
        assert!(paths(&cat, 0, &[0, 1]).unwrap().is_empty(), "its own: backed up");
    }

    /// T2.2: Music's original rewritten in place (a tag write: another size and time on its row): the copy is its own, its own
    /// flag as it was, as the copy itself did not change.
    #[test]
    fn a_copy_whose_original_was_rewritten_is_its_own() {
        let (cat, record) = one_shared_copy();
        cat.execute_for_tests(&format!("UPDATE files SET size = 104, mtime = 30 WHERE folder = 0 AND rel = 'copy-{record}/1.flac'"));
        assert!(paths(&cat, 0, &[0, 1]).unwrap().is_empty(), "its own: backed up");
        cat.execute_for_tests(&format!("UPDATE files SET size = 1, mtime = 1 WHERE folder = 0 AND rel = 'copy-{record}/1.flac'"));
        assert_eq!(paths(&cat, 0, &[0, 1]).unwrap().len(), 1, "the copy's own flag untouched: as before, listed again");
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
        let text = |cat: &Catalogue| paths(cat, 0, &[0, 1]).unwrap().iter().map(|p| format!("{}\n", p.display())).collect::<String>();
        std::thread::scope(|s| {
            for i in 0..20 {
                let (cat, data) = (if i % 2 == 0 { &fifty } else { &two }, &data);
                s.spawn(move || {
                    for _ in 0..20 {
                        write(cat, 0, &[0, 1], data).unwrap();
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
        assert_eq!(write(&shared(2), 0, &[0, 1], &data).unwrap(), 2);
        let old = std::fs::read_to_string(data.join(LIST)).unwrap();
        std::fs::create_dir_all(part_in(&data)).unwrap();
        assert!(write(&shared(3), 0, &[0, 1], &data).is_err(), "the part could not be made");
        assert_eq!(std::fs::read_to_string(data.join(LIST)).unwrap(), old, "the old list as it was");
        std::fs::remove_dir(part_in(&data)).unwrap();
        assert!(parts(&data).is_empty(), "no part left");
        std::fs::remove_dir_all(&data).ok();
    }

    /// T2b's shape: catalogue folders 0 Music, 1 a second main library, 2 Incoming, 3 Ann's own music, 4 Bob's own music; the kept
    /// ones, as `write_for` gives them.
    fn five() -> Catalogue {
        Catalogue::open(None, ["/music", "/more", "/incoming", "/own/ann", "/own/bob"].map(PathBuf::from).to_vec()).unwrap()
    }
    const KEPT: [i64; 4] = [0, 1, 3, 4];

    /// The copies the backup's rule covers (T2b): every copy whose original covers it, wherever it lies, Music's too.
    fn covered(cat: &Catalogue) -> Vec<(i64, String)> {
        cat.shared_copies(-1, &KEPT).unwrap()
    }

    /// One record of one track whose original lies in `folder` (its file `copy-<record>/<track>.flac`), copied, sharing its blocks,
    /// to each of `to`. The track's id: records made in turn in a new catalogue, one track each.
    fn original_in(cat: &Catalogue, title: &str, folder: i64, to: &[(i64, &str)]) -> String {
        let record = cat.record_for_tests(title, 1);
        cat.copy_for_tests(record, folder);
        let from = format!("copy-{record}/{record}.flac");
        cat.copy_rows(&to.iter().map(|(f, rel)| ((folder, from.clone()), (*f, rel.to_string()), true)).collect::<Vec<_>>()).unwrap();
        from
    }

    /// T2b ("Free anywhere", the owner's (d)): an original kept in anyone's own music or any main library covers its copies: Bob's
    /// FLAC in his own music (RY01's shape) covers Music's copy and Ann's; one in a second main library covers Ann's. The backup's
    /// list leaves out Music's copy (Music is backed up as it is) and holds Ann's.
    #[test]
    fn a_copy_of_an_original_kept_anywhere_shares() {
        let cat = five();
        original_in(&cat, "Drift", 4, &[(0, "Agoria/Drift/1.flac"), (3, "Agoria/Drift/1.flac")]);
        original_in(&cat, "Tide Tables", 1, &[(3, "Glass Harbour/Tide Tables/1.flac")]);
        assert_eq!(
            covered(&cat),
            [(0, "Agoria/Drift/1.flac".to_string()), (3, "Agoria/Drift/1.flac".to_string()), (3, "Glass Harbour/Tide Tables/1.flac".to_string())],
            "Music's copy and Ann's of Bob's original, Ann's of the second main library's"
        );
        assert_eq!(paths(&cat, 0, &KEPT).unwrap(), [PathBuf::from("/own/ann/Agoria/Drift/1.flac"), PathBuf::from("/own/ann/Glass Harbour/Tide Tables/1.flac")]);
    }

    /// T2b: an original in Incoming covers nothing (Incoming is not kept), nor one in a trash, nor one deleted and its trash
    /// emptied: the copy is its own and is backed up.
    #[test]
    fn a_copy_of_an_original_in_incoming_or_a_trash_is_its_own() {
        let cat = five();
        original_in(&cat, "Night Ferry", 2, &[(3, "Night Ferry/1.flac")]);
        assert!(covered(&cat).is_empty(), "an original waiting in Incoming covers nothing");
        let record = cat.record_for_tests("Drift", 1);
        cat.copy_for_tests(record, 4);
        cat.copy_rows(&[((4, format!("copy-{record}/{record}.flac")), (3, "Drift/1.flac".into()), true)]).unwrap();
        assert_eq!(covered(&cat), [(3, "Drift/1.flac".to_string())], "covered while Bob's original is there");
        cat.mark_trashed(4, &format!("copy-{record}"), "300/x").unwrap();
        assert!(covered(&cat).is_empty() && paths(&cat, 0, &KEPT).unwrap().is_empty(), "Bob deleted his: Ann's copy is her own, backed up");
        cat.unmark_trashed(4, &format!("copy-{record}"), "300/x").unwrap();
        assert_eq!(covered(&cat).len(), 1, "his Undo covers it again");
    }

    /// T2b (the coordinator: two copies never cover each other on the list): Music's original M, Ann's copy A and Bob's copy B. M
    /// deleted and its trash emptied: A and B hold the same bytes, yet neither is an original, so neither covers the other and both
    /// are backed up.
    #[test]
    fn two_copies_never_cover_each_other() {
        let cat = five();
        let m = original_in(&cat, "Two Rooms", 0, &[(3, "Two Rooms/1.flac"), (4, "Two Rooms/1.flac")]);
        assert_eq!(paths(&cat, 0, &KEPT).unwrap().len(), 2, "both covered by M");
        cat.mark_trashed(0, m.rsplit_once('/').unwrap().0, "400/x").unwrap();
        cat.execute_for_tests("DELETE FROM files WHERE trashed IS NOT NULL");
        assert!(covered(&cat).is_empty(), "neither A nor B covered");
        assert!(paths(&cat, 0, &KEPT).unwrap().is_empty(), "A and B both backed up");
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
        assert_eq!(write(&cat, 0, &[0, 1], &data).unwrap(), 1);
        assert_eq!(std::fs::read_to_string(data.join(LIST)).unwrap(), "/own/Own/1.flac
");
        cat.mark_trashed(1, "Own", "200/Own").unwrap();
        assert_eq!(write(&cat, 0, &[0, 1], &data).unwrap(), 0, "undone: in its library's trash");
        assert_eq!(std::fs::read_to_string(data.join(LIST)).unwrap(), "");
        assert!(!data.join(format!("{LIST}.part")).exists(), "no part left");
        std::fs::remove_dir_all(&data).ok();
    }
}
