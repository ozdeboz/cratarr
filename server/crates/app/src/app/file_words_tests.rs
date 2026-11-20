//! `file_words`' test of a word written into every copy and undone, moved up out of base for the split's phase 2: it
//! writes through a whole App (`every_copy`'s) and the one path's runs, which sit above base. Its body is as it was, its
//! `super::super` paths made absolute.

use crate::app::file_words::proposal_of;
use crate::app::file_words::tests::w;
use crate::app::words::store::{Kind, Source};

/// A word is the record's or its track's: every copy's file gets it, and a copy on tracks of its own the record's.
#[tokio::test(flavor = "multi_thread")]
async fn a_word_reaches_every_copy_and_its_undo_takes_it_out() {
    use crate::app::every_copy::{app, tag, two_copies, undone, written};
    use crate::app::words::store::{KindWords, Words};
    for shared in [true, false] {
        let (c, app) = (two_copies(&format!("words-{shared}"), shared), app(&format!("words-{shared}")).await);
        let words = Words { kinds: vec![KindWords { kind: Kind::Mood, hand: false, words: vec![w("Calm", &[Source::Hand])] }], tracks: Vec::new() };
        let stored = crate::app::words::Stored { records: std::collections::HashMap::from([(c.album.clone(), words)]), ..Default::default() };
        let p = proposal_of(&c.serve, &stored, Kind::Mood, None).unwrap();
        assert_eq!(p.files.len(), 4, "every file of both copies");
        let token = written(&app, p).await;
        assert!(c.files().iter().all(|f| tag(f, "mood") == ["Calm"]));
        undone(&app, &token).await;
        assert!(c.files().iter().all(|f| tag(f, "mood").is_empty()));
    }
}
