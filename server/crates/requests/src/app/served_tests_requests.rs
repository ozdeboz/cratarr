//! Tests moved up from served's files (the app split's phase 2): each reads a record's holding as requests do
//! (`app::requests`), so they go up to requests. By origin, their bodies as they were but for paths.

/// From `serve`'s tests.
#[cfg(test)]
mod serve {
    use crate::serve::ids;
    use crate::serve::testing::*;

    /// Found missing tracks (`app::incomplete`: its whole release eight), the build types it as that release, an album, and the
    /// catalogue keeps that type: the album's record of its name, MusicBrainz's, is held by it, partly there (three of eight), and off
    /// the artist's missing list. Never: the album read missing beside its own partial copy once it is found short.
    #[test]
    fn a_partial_copy_holds_its_album_record_partly_there() {
        use crate::app::requests::{Holding, held};
        let (roots, cat, serve) = on_two_folders("rf1-partly");
        add(&cat, &roots[0], 0, &[("A/01 One.flac", "One"), ("A/02 Two.flac", "Two"), ("A/03 Three.flac", "Three")]);
        serve.refresh().unwrap();
        let owned = record_in(&serve, 0, "A");
        let album = listed(&cat, "album", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000006201")]);
        // The release's list names the three files' tracks (HD15: a copy's tracks pair with the list by the one title rule).
        let title = |n: u32| ["One", "Two", "Three"].get(n as usize - 1).map_or(format!("Track {n}"), |t| t.to_string());
        let list: Vec<crate::catalogue::identity::RefTrack> =
            (1..=8).map(|n| crate::catalogue::identity::RefTrack { disc: 1, position: n, title: title(n), length_ms: Some(200_000), recording: None }).collect();
        cat.set_reference_tracks(album, "musicbrainz", &list).unwrap();
        cat.set_counting(album, Some("musicbrainz"), false).unwrap();
        assert_eq!(cat.typed_for_tests(owned).as_deref(), Some("single"), "three short tracks: a single, until found short");
        assert_eq!(cat.requested(&[album]).unwrap()[&album].holder, None);
        serve.userdata.set_album_complete(&ids::record(owned), Some(r#"{"of":8,"has":3,"release":"Album","missing":[]}"#), Some(1), 100).unwrap();
        serve.refresh().unwrap();
        assert_eq!(cat.typed_for_tests(owned).as_deref(), Some("album"), "typed as its whole release");
        let q = cat.requested(&[album]).unwrap().remove(&album).unwrap();
        assert_eq!(q.holder, Some(owned), "the album had by its partial copy");
        let copies = cat.copies_held(&[(album, owned)], &|_| cratarr_domain::holding::Library::Main).unwrap().remove(&album).unwrap();
        assert_eq!(held(&copies, "me"), Holding::Partly { has: 3, of: 8 }, "partly there");
        let artist = cat.artist_for("Artist").unwrap();
        assert!(cat.missing_of(artist).unwrap().iter().all(|m| m.row.id != album), "off the missing list");
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }
}
