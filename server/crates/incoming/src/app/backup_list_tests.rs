//! `backup_list`'s test of a Replace from a copy, moved up out of base for the split's phase 2: it reaches Incoming's rows
//! (`rows_of_copy`, `rows_back_of_copy`), which sit above base. Its body is as it was.

use std::path::PathBuf;

use crate::app::backup_list::paths;
use crate::catalogue::Catalogue;

/// RY01.11, on the catalogue alone with the rows a Replace from Bob's copy makes and its Undo puts back (`rows_of_copy`,
/// `rows_back_of_copy`). Case 1: Ann's MP3 is a Copy to of Music's MP3, listed; after the Replace (its copies sharing blocks, as
/// the disk said) Music holds no original of its size and time on its track, and it leaves the list; after the Undo it is back.
/// Bob's rows keep `shared = 0` and are never listed (OQ-E); Music's are never listed (they are Music's). Case 2, as T2b has it
/// (an original in anyone's own music covers its copy, and a copy never holds another): Ann's FLAC, a Want's copy of Bob's, is
/// covered by Bob's original before, after and after the Undo, Music's new rows being copies that hold nothing. Case 3
/// (T2b's own rule): as case 2, then Bob's original trashed after the Replace: it covers neither Ann's copy nor
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
