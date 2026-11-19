//! Tests moved up into this crate from the crates below, by their module (SPLIT-PLAN 3.2): each needs this crate.

/// From `crate::catalogue`'s tests.
#[cfg(test)]
mod catalogue {
    use crate::catalogue::*;
    use crate::catalogue::testing::*;

    #[test]
    fn trashed_files_keep_their_ids_until_the_trash_is_emptied() {
        let d = music();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        let one = cat.track_of(0, "A - B (2020)/01 One.mp3").unwrap();
        let token = crate::trash::trash(&d, "A - B (2020)", 1_000).unwrap();
        assert_eq!(cat.mark_trashed(0, "A - B (2020)", &token).unwrap(), 2);
        cat.walk_all();
        cat.identify().unwrap();
        assert_eq!(cat.status().unwrap().files, 0, "trashed files are not counted");
        assert_eq!(cat.file_of(one), None);
        assert!(!retired(&cat, one), "still in the trash: not retired");
        // Undo: back, same ids.
        crate::trash::restore(&d, &token).unwrap();
        assert_eq!(cat.unmark_trashed(0, "A - B (2020)", &token).unwrap(), 2);
        cat.walk_all();
        assert_eq!(cat.file_of(one), Some((0, "A - B (2020)/01 One.mp3".to_string())));
        // Trashed again and the trash emptied: retired.
        let token = crate::trash::trash(&d, "A - B (2020)", 2_000).unwrap();
        cat.mark_trashed(0, "A - B (2020)", &token).unwrap();
        crate::trash::purge(&d, 2_000 + 30 * 86_400, 1).unwrap();
        cat.identify().unwrap();
        assert!(retired(&cat, one));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_library_keep_and_delete_emptied_is_no_problem() {
        let (a, b, c) = (music(), music(), music());
        let cat = Catalogue::open(None, vec![a.clone(), b.clone(), c.clone()]).unwrap();
        cat.walk_all();
        read_as_audio(&cat);
        cat.identify().unwrap();
        // Keep of a's last album: the files and the rows go together.
        std::fs::rename(a.join("A - B (2020)"), b.join("C - D (2021)")).unwrap();
        cat.move_files(0, "A - B (2020)", 1, "C - D (2021)").unwrap();
        // Delete of c's: into its trash, the rows marked.
        let token = crate::trash::trash(&c, "A - B (2020)", 100).unwrap();
        cat.mark_trashed(2, "A - B (2020)", &token).unwrap();
        assert_eq!(cat.walk_all(), Vec::<String>::new());
        for d in [a, b, c] {
            std::fs::remove_dir_all(d).ok();
        }
    }
}
