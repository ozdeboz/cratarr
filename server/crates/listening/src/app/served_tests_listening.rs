//! Tests moved up from served's files (the app split's phase 2): each counts a play through listening's one way in
//! (`app::listening`), so they go up to listening. By origin, their bodies as they were but for paths.

/// From `serve`'s tests.
#[cfg(test)]
mod serve {
    use crate::serve::ids;
    use crate::serve::testing::*;
    use crate::userdata::Heard;

    /// An old id plays on, in a queue no take reached, and what is heard of it counts on the track that took it.
    #[test]
    fn a_play_on_a_taken_id_counts_on_the_track_that_took_it() {
        let (roots, cat, serve) = on_two_folders("heard-taken");
        let one = [("A/01 One.flac", "One")];
        add(&cat, &roots[0], 0, &one);
        add(&cat, &roots[1], 1, &one);
        serve.refresh().unwrap();
        let stays = ids::song(cat.track_of(0, "A/01 One.flac").unwrap());
        let old = ids::song(*cat.moved_tracks().unwrap().keys().next().expect("the second copy's id was taken"));
        assert!(crate::app::listening::heard_blocking(&serve, "me", &old, &Heard::counted(100)).unwrap(), "heard");
        assert_eq!((plays_of(&serve, &stays), plays_of(&serve, &old)), (1, 0));
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }

    /// Two records tagged alike, a track more making the second one, are told apart by their ids alone. One's library taken away, a
    /// persistent id on one song reads as a rebuilt catalogue would, and a play went over to the other record and never came back.
    #[test]
    fn what_lies_on_a_track_away_with_its_library_stays_on_it() {
        let (roots, cat, serve) = on_two_folders("away");
        add(&cat, &roots[0], 0, &[("A/01 One.flac", "One")]);
        add(&cat, &roots[1], 1, &[("A/01 One.flac", "One"), ("A/02 Two.flac", "Two")]);
        checked_again(&serve);
        let song_in = |folder: i64| ids::song(cat.track_of(folder, "A/01 One.flac").unwrap());
        let (here, away) = (song_in(0), song_in(1));
        assert_ne!(here, away, "two records");
        crate::app::listening::heard_blocking(&serve, "me", &away, &Heard::counted(100)).unwrap();
        cat.set_folders(vec![roots[0].clone()]).unwrap();
        checked_again(&serve);
        assert_eq!((plays_of(&serve, &away), plays_of(&serve, &here)), (1, 0), "on its own track, and nothing on the other");
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }
}
