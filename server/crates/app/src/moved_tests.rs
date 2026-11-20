//! Tests moved up into this crate from the crates below, by their module (SPLIT-PLAN 3.2): each needs this crate.

/// From `crate::catalogue`'s tests.
#[cfg(test)]
mod catalogue {
    use crate::catalogue::*;

    #[test]
    fn a_label_from_discogs_reads_the_crate_facts_again_as_a_word_does() {
        let s = crate::serve::Serve::open(None, Rules::default(), &|_| None).unwrap();
        let before = said_stamp(&s);
        assert_eq!(said_stamp(&s), before, "nothing said: nothing to read");
        s.userdata.set_discogs_facts("album:a", Some(r#"{"labels":[{"name":"Warp"}]}"#), 1).unwrap();
        assert_ne!(said_stamp(&s), before);
    }
}

/// From `crate::placement`'s tests.
#[cfg(test)]
mod placement {
    use crate::placement::*;
    use crate::source::Library;
    use crate::placement::testing::*;

    #[tokio::test]
    async fn an_empty_folder_of_cratarrs_own_is_where_it_is_and_another_servers_is_nowhere() {
        let empty = std::env::temp_dir().join(format!("cratarr-empty-{}", std::process::id()));
        std::fs::create_dir_all(&empty).unwrap();
        let path = empty.display().to_string();
        let gone = format!("{path}-gone");
        let values: crate::source::Values = [("folders".to_string(), serde_json::json!([path, gone]))].into();
        let own = (crate::source::cratarr::KIND.connect)(&values, false).unwrap();
        let p = place(&*own, std::slice::from_ref(&path), &Default::default()).await.unwrap();
        assert_eq!(p.libraries[0].folders, std::slice::from_ref(&path), "nothing in it yet: music can be added to it");
        assert_eq!(p.root(&path), Some(format!("{path}/").as_str()));
        assert!(p.libraries[1].folders.is_empty(), "a folder that is not there stays unseen");
        let library = Library { id: "l".into(), name: "L".into(), paths: vec![path.clone()] };
        let other = Fake(vec![(library, vec![])], vec![("l".into(), path.clone())]);
        assert!(place(&other, std::slice::from_ref(&path), &Default::default()).await.unwrap().libraries[0].folders.is_empty());
        std::fs::remove_dir_all(&empty).ok();
    }
}

/// From `crate::bandcamp`'s tests.
#[cfg(test)]
mod bandcamp {
    use crate::bandcamp::*;
    use crate::app::QueueTracks as _;

    /// O4: after its bot check Bandcamp is left alone for a day, said on Health as a connection failing; a search is not read then,
    /// never "nothing found". An album's page is read alone: a track with no stream on it is read from its own page only as it is
    /// played.
    #[tokio::test]
    async fn a_bot_check_holds_bandcamp_for_a_day() {
        let _one = crate::app::switches::TEST_STATE.lock().await;
        let was = TURN.as_held();
        let now = crate::app::moment::now_secs();
        hold();
        let until = held_until(now);
        let bc = Bandcamp::new().unwrap();
        let searched = bc.search("Glass Harbour", "Two Rooms").await;
        let page = "https://glassharbour.bandcamp.com/album/two-rooms";
        let track = |n: u32, stream: bool| Track { n, title: format!("Room {n}"), artist: None, duration_secs: None, stream: stream.then(|| format!("https://t4.bcbits.com/stream/{n}")), page: (!stream).then(|| format!("https://glassharbour.bandcamp.com/track/room-{n}")) };
        bc.keep(Album { url: page.into(), title: "Two Rooms".into(), artist: "Glass Harbour".into(), cover: None, tracks: vec![track(1, true), track(2, false)], released: None, label: None, account: None, about: None, credits: None, tags: Vec::new() });
        let listed = bc.album(page).await.unwrap().queue_tracks().len();
        let played = bc.stream(page, 2, None).await;
        let found: Vec<crate::app::health::Finding> = crate::app::health::check_connections(now).into_iter().filter(|f| f.key == "connections:bandcamp-check").collect();
        let said: Vec<String> = found.iter().map(|f| f.title.clone()).collect();
        TURN.put_back(was);
        let opens = found.iter().map(|f| f.open.as_ref().map(|o| (o.page, o.thing))).collect::<Vec<_>>();
        assert_eq!(opens, [Some(("settings", Some("bandcamp")))], "G9: opens Bandcamp's page in Settings");
        assert!(said.len() == 1 && said[0].starts_with("Bandcamp asked for a browser check; Cratarr stopped asking until "), "{said:?}");
        assert!(until.is_some_and(|u| u >= now + CHECK_HOLD_SECS), "{until:?}");
        assert!(searched.is_err_and(|e| not_read(&e)), "not read, not nothing found");
        assert!(crate::app::reach::state("bandcamp").failures > 0, "a failure for Health's connections");
        assert_eq!(listed, 2, "the track with its own page plays too");
        assert!(played.is_err_and(|e| not_read(&e)), "its own page asked for only now, as it is played");
    }
}

/// From `crate::serve::library`'s tests.
#[cfg(test)]
mod library {
    use std::collections::BTreeSet;

    use crate::catalogue::{Rules, ServedRow};
    use crate::serve::copies::Kind;
    use crate::serve::ids;
    use crate::serve::library::*;
    use crate::serve::library::testing::*;

    /// What plays stays what it was until the copy is all there: Music's, in another kept library as in Incoming, whatever its quality.
    /// Named by `from`, the copy is heard for what it holds, and a track it lacks is passed over, not asked of it.
    #[test]
    fn a_copy_short_of_tracks_changes_nothing_that_plays() {
        for kind in [Kind::Incoming, Kind::Main, Kind::Own] {
            let (roots, cat, lib, album) = a_copy_begun("plays", kind);
            let [music, other] = [0, 1].map(|folder| (folder, "A/03 Three.flac".to_string()));
            let song = |rel: &str| ids::song(cat.track_of(0, rel).unwrap());
            let (three, one) = (song("A/03 Three.flac"), song("A/01 One.flac"));
            let hears = |id: &str, from: Option<i64>| {
                let who = crate::serve::listener::Listener { account: Some("ann".into()), from, ..Default::default() };
                lib.file_for(id, &[], &who).unwrap()
            };
            assert_eq!(hears(&three, None), Some(music.clone()), "{kind:?}: Music's, as before it came");
            assert_eq!(lib.row_copy(&album).unwrap().map(|c| c.folder), Some(0), "{kind:?}: the record's facts and cover are Music's");
            assert_eq!(hears(&three, Some(1)), Some(other), "{kind:?}: by `from`");
            assert_eq!(hears(&one, Some(1)), Some((0, "A/01 One.flac".to_string())), "{kind:?}: a track it does not hold");
            roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
        }
    }

    /// The row copy lacks a file for now: the record still lists the track, from the copy that has it, and a stream reaches it.
    #[test]
    fn a_copy_lacking_a_file_still_lists_every_track() {
        let (roots, cat, lib) = two_kept("lacking");
        let album = taken(&roots, &cat, &lib);
        let (rel, track) = ("A/02 Two.flac", cat.track_of(0, "A/02 Two.flac").unwrap());
        let row = lib.row_copy(&album).unwrap().unwrap().folder;
        let (row_root, other) = (&roots[row as usize], 1 - row);
        std::fs::remove_file(row_root.join(rel)).unwrap();
        cat.walk_all();
        build_now(&lib, &cat);
        assert_eq!(lib.album(&album).unwrap().expect("one album").song_count, 2, "every track");
        assert_eq!(lib.file_of(&ids::song(track)).unwrap(), Some((other, rel.to_string())), "from the copy that has it");
        let who = crate::serve::listener::Listener::default();
        assert_eq!(lib.file_for(&ids::song(track), &cat.files_of(track), &who).unwrap(), Some((other, rel.to_string())));
        assert_eq!(one_number(&lib, "SELECT count(*) FROM albums"), 1, "for now: it splits nothing");
        assert_eq!(cat.track_of(row, "A/01 One.flac"), cat.track_of(other, "A/01 One.flac"), "and gives the copy no tracks of its own");
        // Back: the same again.
        std::fs::write(row_root.join(rel), rel).unwrap();
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        build_now(&lib, &cat);
        assert_eq!(cat.track_of(row, rel), Some(track));
        assert_eq!(lib.file_of(&ids::song(track)).unwrap(), Some((row, rel.to_string())), "the row copy's again");
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// CR1: copies A and B share a track, and B's file is moved into another album's folder outside Cratarr. The album's vote
    /// gives that file another record, so two records hold the track, and a write keyed by record writes it twice.
    #[test]
    fn one_track_in_two_records_is_written_once() {
        let kinds = [(0, Kind::Main)];
        let (a, mut b) = (copy_rows(1, "Z/First", 1, "FLAC", 900), copy_rows(3, "B/Second", 2, "FLAC", 900));
        // Tags of its own, or the two folders are one group, one album, and no track lies in two.
        b.iter_mut().for_each(|r| tag(r, "album", "Second"));
        let moved = || ServedRow { rel: "B/Second/09 One.flac".into(), ..a[0].clone() };
        let apart = || vec![in_folder(0, [a.clone(), b.clone()].concat())];
        let shared = || vec![in_folder(0, [a.clone(), b.clone(), vec![moved()]].concat())];
        let lib = built_with(&kinds, &shared());
        assert_eq!(one_number(&lib, "SELECT count(*) FROM songs WHERE id = 'tr-1'"), 1);
        assert_eq!(lib.albums_of(&["tr-1".to_string()]).unwrap()["tr-1"], "al-1", "in the record it holds");
        assert_eq!(lib.file_of("tr-1").unwrap(), Some((0, "Z/First/01 One.flac".to_string())));
        assert_eq!(one_number(&lib, "SELECT count(*) FROM albums"), 2, "two records, or this proves nothing");
        assert_eq!(lib.album("al-2").unwrap().unwrap().song_count, 2, "not a third song in the other");
        assert_eq!(one_number(&lib, "SELECT count(*) FROM copy_files WHERE song = 'tr-1'"), 2, "both files are built");
        let who = crate::serve::listener::Listener::default();
        let files = [(0, "B/Second/09 One.flac".to_string()), (0, "Z/First/01 One.flac".to_string())];
        assert_eq!(lib.file_for("tr-1", &files, &who).unwrap(), Some(files[1].clone()), "its own record's copy, though the other file sorts first");

        // Built where it changed: the other record lies outside what changed, and only the track reaches it.
        assert_eq!(same_as_whole_in(&kinds, &apart(), &shared()), 2);
        let retagged = |mut f: Vec<FolderRows>, at: usize| {
            tag(&mut f[0].rows[at], "genre", "Dub");
            f
        };
        assert_eq!(same_as_whole_in(&kinds, &shared(), &retagged(shared(), 3)), 2, "a file of the album it was moved into");
        assert_eq!(same_as_whole_in(&kinds, &shared(), &retagged(shared(), 1)), 2, "a file of its own record");
        assert_eq!(same_as_whole_in(&kinds, &shared(), &apart()), 2, "the moved file gone");
        // Its own record's file gone: the song goes over to the file left, in the album it lies in.
        let left = || {
            let mut f = shared();
            f[0].rows.remove(0);
            f
        };
        assert_eq!(same_as_whole_in(&kinds, &shared(), &left()), 2);
        assert_eq!(built_with(&kinds, &left()).albums_of(&["tr-1".to_string()]).unwrap()["tr-1"], "al-2");
    }

    /// The filters ask `copies` and `copy_files`, as `songs` and `albums` hold one place for what can lie in two. A library
    /// off takes a track away only when no file of it is left on.
    #[test]
    fn a_song_is_listed_by_any_folder_it_has_a_file_in() {
        let lib = Library::open(None).unwrap();
        lib.refresh(&sample(), &Rules::default()).unwrap();
        lib.db()
            .execute_batch(
                "INSERT INTO copy_files SELECT 1, 'Elsewhere/' || rel, 'Elsewhere', song, album, src, size, suffix, kbps, duration_ms, bpm,
                   rg_track_gain, rg_track_peak, rg_album_gain, rg_album_peak FROM copy_files WHERE dir = 'Björk/Homogenic (1997)';
                 INSERT INTO copies SELECT album, 1, 'Elsewhere', format, min_kbps, size, added, tracks, bits, rate, short FROM copies WHERE dir = 'Björk/Homogenic (1997)';",
            )
            .unwrap();
        let titles = |songs: &[crate::serve::api::Song]| {
            let mut t: Vec<String> = songs.iter().map(|s| s.title.clone()).collect();
            t.sort();
            t
        };
        let both = ["Hunter", "Jóga"];
        for folder in [Some(0), Some(1)] {
            let albums = lib.album_list(&crate::serve::read::AlbumOrder::ByName, 10, 0, folder).unwrap();
            assert!(albums.iter().any(|a| a.name == "Homogenic"), "{folder:?}");
            assert_eq!(titles(&lib.random_songs(10, Some("Art Pop"), (None, None), folder).unwrap()), both, "{folder:?}");
            assert_eq!(titles(&lib.songs_by_genre("Art Pop", 10, 0, folder).unwrap()), both, "{folder:?}");
            for query in ["", "Homogenic"] {
                let (_, albums, songs) = lib.search(query, (0, 0), (10, 0), (10, 0), folder).unwrap();
                assert!(albums.iter().any(|a| a.name == "Homogenic"), "{query:?} in {folder:?}");
                assert!(both.iter().all(|t| songs.iter().any(|s| s.title == *t)), "{query:?} in {folder:?}");
            }
        }
        assert_eq!(lib.album_list(&crate::serve::read::AlbumOrder::ByName, 10, 0, Some(1)).unwrap().len(), 1, "the other album has no copy there");
        assert!(lib.album_list(&crate::serve::read::AlbumOrder::ByName, 10, 0, Some(2)).unwrap().is_empty());
        assert!(lib.random_songs(10, None, (None, None), Some(2)).unwrap().is_empty());
        assert!(lib.songs_by_genre("Art Pop", 10, 0, Some(2)).unwrap().is_empty());
        let (_, albums, songs) = lib.search("Homogenic", (0, 0), (10, 0), (10, 0), Some(2)).unwrap();
        assert!(albums.is_empty() && songs.is_empty());

        let on = |off: &[i64]| {
            let mut t: Vec<String> = lib.track_rows(&[], off).unwrap().into_iter().map(|r| r.title).collect();
            t.sort();
            t
        };
        assert_eq!(on(&[0]), both, "Music off: the file in the other folder keeps it");
        assert_eq!(on(&[1]).len(), 3, "the other folder off: Music holds all three");
        assert!(on(&[0, 1]).is_empty(), "both off");

        let album: String = lib.db().query_row("SELECT id FROM albums WHERE name = 'Homogenic'", [], |r| r.get(0)).unwrap();
        let hunter = ids::song(1);
        assert_eq!(lib.song_folders(std::slice::from_ref(&hunter)).unwrap()[&hunter].iter().copied().collect::<BTreeSet<i64>>(), [0, 1].into());
        assert_eq!(lib.album_folders().unwrap()[&album].iter().copied().collect::<BTreeSet<i64>>(), [0, 1].into());
        let folders = lib.artist_albums().unwrap().into_iter().find(|r| r.3 == album).unwrap().4;
        assert_eq!(folders.into_iter().collect::<BTreeSet<i64>>(), [0, 1].into());
        assert_eq!(lib.albums_in(1, 10).unwrap().iter().map(|c| (c.album.id.clone(), c.dir.clone())).collect::<Vec<_>>(), [(album.clone(), "Elsewhere".to_string())], "the copy's own folder");
        let copies: Vec<(i64, String)> = lib.copies_of(&album).unwrap().into_iter().map(|c| (c.folder, c.dir)).collect();
        assert_eq!(copies, [(0, "Björk/Homogenic (1997)".to_string()), (1, "Elsewhere".to_string())]);
        assert_eq!(lib.album_in(1, "Elsewhere").unwrap(), Some(album));
    }

    #[test]
    fn an_artists_work_is_their_albums_newest_first_then_what_they_appear_on() {
        let lib = Library::open(None).unwrap();
        let t = |n: i64, rel: &str, artist: &str, album_artist: &str, album: &str, date: &str| {
            let title = format!("t{n}");
            row(n, rel, &[("title", &title), ("artist", artist), ("albumartist", album_artist), ("album", album), ("tracknumber", &n.to_string()), ("date", date)])
        };
        let folders = vec![FolderRows {
            folder: 0,
            root: "/music".into(),
            rows: vec![
                t(1, "A/Old (2001)/01 t.flac", "A", "A", "Old", "2001"),
                t(2, "A/New (2010)/01 t.flac", "A", "A", "New", "2010"),
                t(3, "A/New (2010)/02 t.flac", "C", "A", "New", "2010"),
                t(4, "B/Mix (2005)/01 t.flac", "B", "B", "Mix", "2005"),
                t(5, "B/Mix (2005)/02 t.flac", "A", "B", "Mix", "2005"),
            ],
        }];
        lib.refresh(&folders, &Rules::default()).unwrap();
        let (own, on, songs) = lib.artist_works(&ids::artist(artist_number_in_tests("a"))).unwrap();
        let names = |l: &[crate::serve::api::Album]| l.iter().map(|a| a.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&own), ["New", "Old"]);
        assert_eq!(names(&on), ["Mix"], "credited on a track only");
        let titles: Vec<&str> = songs.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["t2", "t3", "t5", "t1"], "every track of their albums, and theirs on others', album by album");
    }
}

/// From `crate::serve::listener`'s tests.
#[cfg(test)]
mod listener {
    use crate::serve::listener::tests::*;
    use crate::source::CRATARR;

    #[test]
    fn a_stream_plays_the_copy_copy_for_chose() {
        let t = two("chose");
        assert_eq!(t.plays(&t.who(None, &[])), at(0), "no one: the row copy");
        assert_eq!(t.plays(&t.who(Some("ann"), &[])), at(0), "the best kept copy");
        assert_eq!(t.plays(&t.who(Some("ann"), &[0])), at(1), "the other copy is off");

        let (path, kind, ms) = crate::stream::locate(&t.cat, &t.who(Some("ann"), &[0]), CRATARR, &t.song).unwrap();
        assert_eq!((path, matches!(kind, Some(crate::stream::Kind::Mp3)), ms), (t.roots[1].join(MP3), true, Some(200_000)));
        let (path, kind, _) = crate::stream::locate(&t.cat, &t.who(None, &[]), CRATARR, &t.song).unwrap();
        assert_eq!((path, matches!(kind, Some(crate::stream::Kind::Flac))), (t.roots[0].join(FLAC), true));
        let through = [(CRATARR.to_string(), t.song.clone()), (CRATARR.to_string(), t.song.clone())];
        let joins = crate::stream::locate_through(&t.cat, &t.who(Some("ann"), &[0]), &through);
        assert_eq!(joins, [(t.roots[1].join(MP3), Some(200_000)), (t.roots[1].join(MP3), Some(200_000))], "a wind goes through the same file");
    }
}
