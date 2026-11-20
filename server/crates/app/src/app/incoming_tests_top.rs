//! Tests moved up from incoming's files (the app split's phase 2): each runs the one path on the whole app (`App::new`,
//! or every_copy's records on it), so they go up to the top. By origin, their bodies as they were but for paths.

/// From `file_runs`' tests.
#[cfg(test)]
mod file_runs {
    use std::path::Path;
    use std::sync::Arc;

    use crate::app::file_runs::testing::*;
    use crate::app::file_runs::{CUT, MOVE, Proposal, RUN, RunEntry, apply_one, apply_whole, busy, fields_of, holds, reached, restore_whole, run, runs, start, turn, undo};
    use crate::retag::fixtures::*;

    /// An `App` with no server, for what asks it: its journal, the list of runs.
    async fn app(tag: &str) -> Arc<crate::app::App> {
        let d = dir(tag);
        std::fs::create_dir_all(d.join(crate::playlists::PLAYLIST_DIR)).unwrap();
        let cfg = crate::config::Config::from_lookup(|_| None).unwrap();
        let saved = crate::settings::Saved { music_folder: Some(d.to_string_lossy().into_owned()), ..Default::default() };
        crate::app::App::new(cfg, saved, crate::settings::Origin::Env, None).await.unwrap()
    }

    #[tokio::test]
    async fn a_run_under_way_is_on_the_list_once_and_a_run_cut_off_is_on_it_with_its_undo() {
        let app = app("run-entry-list").await;
        let p = proposal(vec![tag_write("a", Path::new("/m/a.flac"), &["ep"])]);
        let e = RunEntry::begin(app.journal.clone(), &p, super::super::moment::now_secs(), None, 1000);
        let id = e.id.unwrap();
        assert!(runs(&app, 0).unwrap().iter().all(|s| s.id != format!("{RUN}{id}")), "told as it goes: its entry waits for its end");
        // A restart: nothing is writing, and the entry is all that is left of it.
        app.journal.run_ended(id);
        let cut = runs(&app, 0).unwrap().into_iter().find(|s| s.id == format!("{RUN}{id}")).unwrap();
        assert_eq!(cut.undo, Some(format!("{RUN}{id}")));
        assert_eq!(cut.failed.iter().map(|f| f.why.as_str()).collect::<Vec<_>>(), [CUT]);
    }

    /// Two journals number their runs alike, each from 1: an Undo going in one is no run of the other's (the gate of
    /// 2026-11-16 saw another test's Undo on this list above). The one lock holds the Undo waiting while both lists are read.
    #[tokio::test]
    async fn an_undo_under_way_is_on_its_own_journals_list_alone() {
        let (one, two) = (app("undo-own-one").await, app("undo-own-two").await);
        let p = proposal(vec![tag_write("a", Path::new("/m/a.flac"), &["ep"])]);
        let now = super::super::moment::now_secs();
        let id = RunEntry::begin(one.journal.clone(), &p, now, None, 1000).id.unwrap();
        one.journal.run_ended(id);
        let held = turn(&one.journal).await;
        assert_eq!(undo(&one, &format!("{RUN}{id}"), None).await.unwrap().state, "waiting");
        assert_eq!(RunEntry::begin(two.journal.clone(), &p, now, None, 1000).id, Some(id), "numbered alike");
        assert!(runs(&two, 0).unwrap().iter().all(|s| s.id != format!("{RUN}{id}")), "the other journal's Undo is none of this one's runs");
        assert!(runs(&one, 0).unwrap().iter().any(|s| s.id == format!("{RUN}{id}") && s.state == "waiting"), "its own, waiting its turn");
        assert!(busy(&one.journal, "types") && !busy(&two.journal, "types"), "busy, as its own list says");
        drop(held);
    }

    /// Inside the second before the next save, so the entry still says the first album was not finished with.
    #[tokio::test]
    async fn a_run_cut_off_says_on_its_page_what_was_written_whatever_its_entry_got_to() {
        let app = app("run-entry-page").await;
        let d = dir("run-entry-page-files");
        let (one, two) = (flac(&d, "one"), flac(&d, "two"));
        let p = proposal(vec![tag_write("a", &one, &["ep"]), tag_write("b", &two, &["ep"])]);
        let mut e = RunEntry::begin(app.journal.clone(), &p, super::super::moment::now_secs(), None, 1000);
        let first = p.files[0].clone();
        apply_one(&first.path, &first.before, &first.after, "moved").unwrap();
        e.album_done("a", vec![first], &[]);

        let (_, sheet) = run(&app, &format!("{RUN}{}", e.id.unwrap())).await.unwrap();
        let written: Vec<(&str, Option<usize>)> = sheet.records.iter().map(|r| (r.album.as_str(), r.written)).collect();
        assert_eq!(written, [("a", Some(1)), ("b", Some(0))]);
        std::fs::remove_dir_all(&d).ok();
    }

    fn ep(path: &Path) -> bool {
        holds(&fields_of(path, &["releasetype"]).unwrap(), &[("releasetype".to_string(), s(&["ep"]))])
    }

    async fn until(what: &str, mut done: impl FnMut() -> bool) {
        for _ in 0..1000 {
            if done() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("waited for {what}");
    }

    /// The finding's case, by the real write and the real Undo: a run stopped, the write tried again for what is left.
    #[tokio::test]
    async fn a_run_cut_off_claims_nothing_a_later_run_wrote_and_each_undoes_its_own() {
        let app = app("settle-later").await;
        let d = dir("settle-later-files");
        let (one, two) = (flac(&d, "one"), flac(&d, "two"));
        let p = proposal(vec![tag_write("a", &one, &["ep"]), tag_write("b", &two, &["ep"])]);
        let cut = cut_after_the_first(&app.journal, &p);

        start(&app, Proposal { kind: "settle-later".into(), ..proposal(vec![tag_write("b", &two, &["ep"])]) });
        until("the second run", || !busy(&app.journal, "settle-later")).await;
        assert!(ep(&one) && ep(&two));
        let later = app.journal.file_runs_since(0).unwrap().into_iter().map(|(id, ..)| id).find(|id| *id != cut).unwrap();

        let (_, sheet) = run(&app, &format!("{RUN}{cut}")).await.unwrap();
        let written: Vec<(&str, Option<usize>)> = sheet.records.iter().map(|r| (r.album.as_str(), r.written)).collect();
        assert_eq!(written, [("a", Some(1)), ("b", Some(0))], "the second run's file is not said to be the first's");

        undo(&app, &format!("{RUN}{cut}"), None).await.unwrap();
        until("the first's Undo", || app.journal.file_run(cut).unwrap().unwrap().2).await;
        assert!(!ep(&one), "what it wrote goes back");
        assert!(ep(&two), "and what it did not write stays");

        undo(&app, &format!("{RUN}{later}"), None).await.unwrap();
        until("the second's Undo", || app.journal.file_run(later).unwrap().unwrap().2).await;
        assert!(!ep(&two), "the second run still undoes its own");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Read under the lock, after the Undo was asked for: its entry is shorter than the one the Undo was started from.
    #[tokio::test]
    async fn an_undo_asked_of_a_run_not_read_yet_puts_back_what_it_wrote() {
        let app = app("settle-undo").await;
        let d = dir("settle-undo-files");
        let (one, two) = (flac(&d, "one"), flac(&d, "two"));
        // The file it did not reach first: the one it wrote is the second of the entry until the entry is read.
        let p = proposal(vec![tag_write("a", &two, &["ep"]), tag_write("a", &one, &["ep"])]);
        let e = RunEntry::begin(app.journal.clone(), &p, super::super::moment::now_secs(), None, 1000);
        let id = e.id.unwrap();
        apply_one(&one, &p.files[1].before, &p.files[1].after, "moved").unwrap();
        app.journal.run_ended(id);

        undo(&app, &format!("{RUN}{id}"), None).await.unwrap();
        until("its Undo", || app.journal.file_run(id).unwrap().unwrap().2).await;
        assert!(!ep(&one));
        std::fs::remove_dir_all(&d).ok();
    }

    /// A stop between two copies: the first is in the entry with its Undo, and the second, not begun, is in neither.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_cover_stopped_between_its_copies_undoes_the_copy_it_reached() {
        use super::super::every_copy::{picture, two_copies};
        let c = two_copies("cover-cut", true);
        let p = super::super::covers::proposal(&c.serve, &c.album, &picture(200), "uploaded").unwrap();
        let (_j, mut e) = entry(&p);
        assert!(e.run.files.is_empty(), "a cover is in it on its way, not before");
        let mut first = p.files[0].clone();
        apply_whole(&mut first, Path::new("unused"), 0, |f| e.whole_file(f)).unwrap();
        assert_eq!(e.run.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>(), [first.path.clone()]);
        assert!(reached(&e.run.files[0]).unwrap());
        assert!(first.path.join("cover.jpg").exists());
        let second = &p.files[1].path;
        assert!(!second.join("cover.jpg").exists(), "the second not begun");
        restore_whole(&first.path, first.whole.as_ref().unwrap()).unwrap();
        assert!(!first.path.join("cover.jpg").exists(), "the copy it reached put back");
    }

    /// An album action's page (decision 0092) holds its record with every file it moved, as the page shows a record and
    /// its Undo only for what was written. An add to someone's own music opened empty in 0.16.0: its record said none.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_album_actions_page_holds_its_record_with_every_file_it_moved() {
        use crate::journal::{Event, TrackMove};
        let app = super::super::every_copy::app("album-action-page").await;
        let root = app.library.catalogue().folders().get(0).cloned().expect("the music folder in use");
        let moved = |n: usize| (0..n).map(|i| TrackMove { track: Some(i as i64 + 1), from: (0, format!("in/Drift/{i}.flac")), to: (0, format!("A/Drift/{i}.flac")), was: None }).collect::<Vec<_>>();
        let adopted = Event::AlbumAdopted {
            source: "cratarr".into(),
            album_id: "al-1".into(),
            title: "Drift".into(),
            from_dir: (0, "in/Drift".into()),
            to_dir: (0, "A/Drift".into()),
            moves: moved(3),
            release_group: None,
            converted: Vec::new(),
        };
        let replaced = Event::AlbumReplaced {
            source: "cratarr".into(),
            album_id: "al-2".into(),
            title: "Drift".into(),
            from_dir: (0, "in/Drift".into()),
            to_dir: (0, "A/Drift".into()),
            moves: moved(2),
            replaced: moved(2),
            token: "1/A/Drift".into(),
            release_group: None,
            converted: Vec::new(),
        };
        let gone = root.join("in/Gone");
        std::fs::create_dir_all(gone.join("CD 2")).unwrap();
        for f in ["1.flac", "2.flac", "cover.jpg", "CD 2/1.flac"] {
            std::fs::write(gone.join(f), b"x").unwrap();
        }
        let token = crate::trash::trash(&root, "in/Gone", 1).unwrap();
        let rejected = Event::AlbumRejected { source: "cratarr".into(), album_id: "al-3".into(), title: "Gone".into(), folder: 0, dir: "in/Gone".into(), token, release_group: None };
        for (event, files) in [(adopted, 3), (replaced, 2), (rejected, 4)] {
            let id = app.journal.intend(1, &event).unwrap();
            app.journal.applied(id).unwrap();
            let (status, sheet) = run(&app, &format!("{MOVE}{id}")).await.unwrap();
            assert_eq!(status.undo, Some(format!("{MOVE}{id}")), "{}", status.title);
            let records: Vec<(usize, Option<usize>)> = sheet.records.iter().map(|r| (r.files, r.written)).collect();
            assert_eq!(records, [(files, Some(files))], "{}", status.title);
        }
    }
}
