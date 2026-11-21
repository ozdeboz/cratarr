//! `references`' tests of a record not had's page, moved up out of records for the split's phase 2: they open the page
//! (`playing::page_at`, `AlbumView`), which sits above records. Their bodies are as they were; `super::super::moment` is
//! named from the crate.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

use anyhow::Result;

use crate::app::playing::{RecordPage, open_at};
use crate::app::references::tests::{list, reader, wanted};
use crate::app::references::{Read, ReadMore, Said, SwitchedOff, TestState, page_read_of, page_read_settled, source_name, tracklist};
use crate::app::switches::{BACKGROUND, BANDCAMP, DISCOGS, MUSICBRAINZ, set_for_tests};
use crate::catalogue::Catalogue;
use crate::catalogue::identity;
use crate::catalogue::records::Ref;
use crate::serve::ids;

// ---- F1 (preview 1's fix batch): a record not had says why it has no tracks

/// A Bandcamp page of the tests' artist.
const PAGE: &str = "https://kitetheory.bandcamp.com/album/answered";

/// A record not had's page as the server answers it at `now`, its list asked for through `read`.
async fn opened<R, F>(cat: &Arc<Catalogue>, record: i64, now: i64, read: R) -> crate::app::AlbumView
where
    R: Fn(Ref) -> F + Send + Sync + 'static,
    F: std::future::Future<Output = Result<Option<Read>>> + Send + 'static,
{
    match open_at(cat, record, now, read).await.unwrap() {
        Some(RecordPage::NotHad(v)) => *v,
        _ => panic!("al-{record} is no page of a record not had"),
    }
}

/// What the page says a source said.
fn saying(source: &'static str, said: &'static str) -> Said {
    Said { source, said, until: None }
}

/// What a reader was asked, in order.
fn asks() -> Arc<std::sync::Mutex<Vec<String>>> {
    Arc::new(std::sync::Mutex::new(Vec::new()))
}

/// `reader`, minding the MusicBrainz switch as `read_reference` does: switched off, it says so and is asked nothing.
fn minding(lists: &'static [(&'static str, &'static [&'static str])], asked: Arc<std::sync::Mutex<Vec<String>>>) -> impl Fn(Ref) -> std::future::Ready<Result<Option<Read>>> + Send + Sync + 'static {
    let read = reader(lists, asked, || Ok(None));
    move |r: Ref| {
        if r.source == "musicbrainz" && !crate::app::switches::is_on(MUSICBRAINZ) {
            return std::future::ready(Err(SwitchedOff("MusicBrainz").into()));
        }
        read(r)
    }
}

type Asking = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Read>>> + Send>>;

/// A reader held at a gate until the test opens it, then answering any reference with one track; it counts its questions.
fn gated(gate: &Arc<tokio::sync::Semaphore>, asked: &Arc<AtomicUsize>) -> impl Fn(Ref) -> Asking + Clone + Send + Sync + 'static {
    let (gate, asked) = (gate.clone(), asked.clone());
    move |_: Ref| -> Asking {
        asked.fetch_add(1, SeqCst);
        let gate = gate.clone();
        Box::pin(async move {
            let _open = gate.acquire().await;
            Ok::<Option<Read>, anyhow::Error>(Some((list(&[("Lamplight", Some(200_000))]), None, Default::default())))
        })
    }
}

/// Waits for a gated reader's first question (its read begun), then a beat for any other.
async fn first_asked(asked: &AtomicUsize) {
    for _ in 0..1_000 {
        if asked.load(SeqCst) > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("nothing was asked");
}

/// The titles of a page's rows.
fn titles(v: &crate::app::AlbumView) -> Vec<String> {
    v.rows.iter().map(|r| r.name.clone().unwrap_or_default()).collect()
}

/// `gated`, but MusicBrainz's release `r62` has none at once, so a read of it moves on; it keeps what it is asked (source:value).
fn following(gate: &Arc<tokio::sync::Semaphore>, asked: &Arc<std::sync::Mutex<Vec<String>>>) -> impl Fn(Ref) -> Asking + Clone + Send + Sync + 'static {
    let (gate, asked) = (gate.clone(), asked.clone());
    move |r: Ref| -> Asking {
        asked.lock().unwrap().push(format!("{}:{}", r.source, r.value));
        let gate = gate.clone();
        Box::pin(async move {
            if r.value == "r62" {
                return Ok::<Option<Read>, anyhow::Error>(None);
            }
            let _open = gate.acquire().await;
            Ok::<Option<Read>, anyhow::Error>(Some((list(&[("Lamplight", Some(200_000))]), None, Default::default())))
        })
    }
}

/// Waits until `asked` holds `what`.
async fn until_asked(asked: &std::sync::Mutex<Vec<String>>, what: &str) {
    for _ in 0..1_000 {
        if asked.lock().unwrap().iter().any(|a| a == what) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("{what} was never asked");
}

/// F6 (F1.12's loader): while a record not had's list is read, its page is told the source the read asks now: before its first
/// question the first it will ask, then each it moves on to; with MusicBrainz switched off, Discogs from the start. None once the
/// read has ended.
#[tokio::test]
async fn a_read_says_which_source_it_reads() {
    let _state = TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let asked = asks();
    let t = crate::app::moment::now_secs();
    // MusicBrainz asked, waiting at its gate.
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let waits = wanted(&cat, "Answered", &[("musicbrainz", "release", "r61"), ("discogs", "release", "2000061")]);
    let v = opened(&cat, waits, t, following(&gate, &asked)).await;
    assert_eq!((v.more.looking, v.more.reading), (true, Some("musicbrainz")), "the first it will ask");
    until_asked(&asked, "musicbrainz:r61").await;
    assert_eq!(opened(&cat, waits, t, following(&gate, &asked)).await.more.reading, Some("musicbrainz"), "while MusicBrainz is asked");
    gate.add_permits(8);
    page_read_settled(&cat, waits).await;
    let v = opened(&cat, waits, t + 1, following(&gate, &asked)).await;
    assert_eq!((v.rows.len(), v.more.looking, v.more.reading), (1, false, None), "ended: no source named");
    assert!(serde_json::to_value(&v).unwrap().get("reading").is_none());
    // MusicBrainz has none: the read moves on to Discogs, and says so.
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let moves = wanted(&cat, "Break Synonyms", &[("musicbrainz", "release", "r62"), ("discogs", "release", "2000062")]);
    assert_eq!(opened(&cat, moves, t, following(&gate, &asked)).await.more.reading, Some("musicbrainz"));
    until_asked(&asked, "discogs:2000062").await;
    assert_eq!(opened(&cat, moves, t, following(&gate, &asked)).await.more.reading, Some("discogs"), "it follows the read");
    gate.add_permits(8);
    page_read_settled(&cat, moves).await;
    // MusicBrainz switched off: Discogs from the start.
    set_for_tests(MUSICBRAINZ, false);
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let off = wanted(&cat, "The Curved Line", &[("musicbrainz", "release", "r63"), ("discogs", "release", "2000063")]);
    assert_eq!(opened(&cat, off, t, following(&gate, &asked)).await.more.reading, Some("discogs"), "MusicBrainz off: Discogs first");
    until_asked(&asked, "discogs:2000063").await;
    assert_eq!(opened(&cat, off, t, following(&gate, &asked)).await.more.reading, Some("discogs"), "while Discogs is asked");
    gate.add_permits(8);
    page_read_settled(&cat, off).await;
    assert!(!asked.lock().unwrap().iter().any(|a| a == "musicbrainz:r63"), "MusicBrainz asked nothing while off");
}

/// F1.2: a source that does not answer is read again after a quarter hour, not before. Changed by design (F13.1): meanwhile the next
/// source's list is read, kept and shown, and counts nothing; no reason line beside its rows; never "none".
#[tokio::test]
async fn a_source_that_does_not_answer_is_said_and_read_again_after_a_quarter_hour() {
    let _state = TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let record = wanted(&cat, "Answered", &[("musicbrainz", "release", "r12"), ("bandcamp", "album", PAGE)]);
    let asked = asks();
    let read = || reader(&[("bandcamp", &["Lamplight", "Morning"])], asked.clone(), || Err(anyhow::anyhow!("MusicBrainz answered 503 Service Unavailable")));
    let t = crate::app::moment::now_secs();
    let first = opened(&cat, record, t, read()).await;
    assert!(first.more.looking && first.rows.is_empty() && first.more.no_list.is_none(), "at t: being read");
    page_read_settled(&cat, record).await;
    let later = opened(&cat, record, t + 60, read()).await;
    assert!(!later.more.looking && titles(&later) == ["Lamplight", "Morning"], "a minute on: Bandcamp's rows");
    assert_eq!(later.more.no_list, None, "no reason line beside rows");
    assert_eq!(*asked.lock().unwrap(), ["musicbrainz:release", "bandcamp:album"], "MusicBrainz first, then the next");
    assert_eq!(kept_sources(&cat, record), ["bandcamp"], "Bandcamp's list kept");
    assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "nothing counted");
    assert_eq!(opened(&cat, record, t + 300, read()).await.rows.len(), 2);
    assert_eq!(asked.lock().unwrap().len(), 2, "not read again inside the quarter hour");
    let again = opened(&cat, record, t + 960, read()).await;
    assert!(!again.more.looking && titles(&again) == ["Lamplight", "Morning"], "a quarter hour on: read again, the rows shown meanwhile");
    page_read_settled(&cat, record).await;
    assert_eq!(*asked.lock().unwrap(), ["musicbrainz:release", "bandcamp:album", "musicbrainz:release"], "MusicBrainz again; Bandcamp's kept list not asked again");
    assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "still busy: still nothing counted");
}

/// F1.3: none of a record's references gives a list: each named as having none, nothing counted, no list kept; never "off" or
/// "unanswered", nor the three lost to one word.
#[tokio::test]
async fn a_record_no_reference_lists_says_which_have_none() {
    let _state = TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let record = wanted(&cat, "Answered", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000513"), ("discogs", "master", "4002"), ("bandcamp", "album", PAGE)]);
    let asked = asks();
    let t = crate::app::moment::now_secs();
    assert!(opened(&cat, record, t, reader(&[], asked.clone(), || Ok(None))).await.more.looking);
    page_read_settled(&cat, record).await;
    let v = opened(&cat, record, t + 1, reader(&[], asked.clone(), || Ok(None))).await;
    assert!(!v.more.looking && v.rows.is_empty());
    assert_eq!(v.more.no_list, Some(vec![saying("musicbrainz", "none"), saying("discogs", "none"), saying("bandcamp", "none")]));
    assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "nothing counted");
    assert!(cat.reference_tracks(record).unwrap().is_empty(), "no list kept");
    assert_eq!(*asked.lock().unwrap(), ["musicbrainz:release-group", "discogs:master", "bandcamp:album"], "each read once");
}

/// F1.5: Bandcamp held back after its bot check says until when: (a) a record with only its page, at once, nothing read; (b) one
/// whose MusicBrainz release has none, once its read ends, Bandcamp asked nothing while held. After the hold, or Bandcamp switched
/// off and on (its hold lifted), (a) is read at once. Never "unanswered" of a hold Cratarr keeps itself.
#[tokio::test]
async fn bandcamp_held_back_says_until_when() {
    let _state = TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let t = crate::app::moment::now_secs();
    let until = t + 3_600;
    crate::bandcamp::hold_for_tests(until);
    let alone = wanted(&cat, "Answered", &[("bandcamp", "album", PAGE)]);
    let both = wanted(&cat, "Kyverdale Road", &[("musicbrainz", "release", "r15"), ("bandcamp", "album", "https://kitetheory.bandcamp.com/album/kyverdale-road")]);
    let lifted = wanted(&cat, "The Curved Line", &[("bandcamp", "album", "https://kitetheory.bandcamp.com/album/the-curved-line")]);
    let asked = asks();
    let read = || {
        let asked = asked.clone();
        move |r: Ref| {
            asked.lock().unwrap().push(format!("{}:{}", r.source, r.kind));
            std::future::ready(if r.source == "bandcamp" { Err(anyhow::Error::from(crate::bandcamp::NotRead)) } else { Ok::<Option<Read>, anyhow::Error>(None) })
        }
    };
    let held = Said { source: "bandcamp", said: "held", until: Some(until) };
    let a = opened(&cat, alone, t, read()).await;
    assert!(!a.more.looking, "(a) at once, nothing read");
    assert_eq!(a.more.no_list, Some(vec![held.clone()]));
    assert!(asked.lock().unwrap().is_empty());
    assert!(opened(&cat, both, t, read()).await.more.looking, "(b) MusicBrainz is to be read");
    page_read_settled(&cat, both).await;
    let b = opened(&cat, both, t + 1, read()).await;
    assert!(!b.more.looking);
    assert_eq!(b.more.no_list, Some(vec![saying("musicbrainz", "none"), held.clone()]));
    assert_eq!(*asked.lock().unwrap(), ["musicbrainz:release"], "Bandcamp asked nothing while held");
    assert!(opened(&cat, alone, until + 1, read()).await.more.looking, "(a) after its hold: read at once");
    page_read_settled(&cat, alone).await;
    assert_eq!(opened(&cat, lifted, t, read()).await.more.no_list, Some(vec![held.clone()]));
    set_for_tests(BANDCAMP, false);
    crate::bandcamp::switched(None, false);
    set_for_tests(BANDCAMP, true);
    crate::bandcamp::switched(None, true);
    assert!(opened(&cat, lifted, t, read()).await.more.looking, "switched off and on: its hold lifted, read at once");
    page_read_settled(&cat, lifted).await;
}

/// F1.6 (kind 3): a source said to be switched off that is on since is read at once: a record said off at once, and one whose read
/// came to "MusicBrainz off, Discogs unanswered" a minute before; MusicBrainz is asked, and its list shown at the next answer.
/// Never "switched off" said while it is on, nor a quarter hour waited.
#[tokio::test]
async fn a_source_switched_on_since_is_read_at_once() {
    let _state = TestState::set().await;
    set_for_tests(MUSICBRAINZ, false);
    set_for_tests(DISCOGS, false);
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let off = wanted(&cat, "Answered", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000516"), ("discogs", "master", "4001")]);
    let away = wanted(&cat, "Break Synonyms", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000517"), ("discogs", "master", "4006")]);
    let asked = asks();
    let read = || reader(&[("musicbrainz", &["Lamplight"])], asked.clone(), || Err(anyhow::anyhow!("Discogs answered 503 Service Unavailable")));
    let t = crate::app::moment::now_secs();
    assert_eq!(opened(&cat, off, t, read()).await.more.no_list, Some(vec![saying("musicbrainz", "off"), saying("discogs", "off")]));
    set_for_tests(DISCOGS, true);
    assert!(opened(&cat, away, t, read()).await.more.looking);
    page_read_settled(&cat, away).await;
    assert_eq!(opened(&cat, away, t + 1, read()).await.more.no_list, Some(vec![saying("musicbrainz", "off"), saying("discogs", "unanswered")]));
    assert_eq!(*asked.lock().unwrap(), ["discogs:master"], "nothing asked of MusicBrainz while it was off");
    set_for_tests(MUSICBRAINZ, true);
    for record in [off, away] {
        let v = opened(&cat, record, t + 60, read()).await;
        assert!(v.more.looking && v.more.no_list.is_none(), "{record}: read at once, nothing said off");
        page_read_settled(&cat, record).await;
        let v = opened(&cat, record, t + 61, read()).await;
        assert_eq!((titles(&v), v.more.looking, v.more.no_list.clone()), (vec!["Lamplight".to_string()], false, None), "{record}: MusicBrainz's list shown");
    }
    assert_eq!(asked.lock().unwrap().iter().filter(|a| *a == "musicbrainz:release-group").count(), 2, "MusicBrainz asked for each");
}

/// F1.8 (kind 4): two opens at once read once; both say it is being read, and once it ends one list is kept and each next answer
/// has its rows. Never two reads of one record at once, nor a read ended left `looking`.
#[tokio::test]
async fn two_opens_read_once() {
    let _state = TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let record = wanted(&cat, "Fourth: The Golden Eagle", &[("musicbrainz", "release", "r18")]);
    let (gate, asked) = (Arc::new(tokio::sync::Semaphore::new(0)), Arc::new(AtomicUsize::new(0)));
    let read = gated(&gate, &asked);
    let t = crate::app::moment::now_secs();
    let (a, b) = tokio::join!(opened(&cat, record, t, read.clone()), opened(&cat, record, t, read.clone()));
    assert!(a.more.looking && b.more.looking, "both see it read");
    first_asked(&asked).await;
    assert_eq!(asked.load(SeqCst), 1, "one read");
    gate.add_permits(8);
    page_read_settled(&cat, record).await;
    assert_eq!(cat.reference_tracks(record).unwrap().len(), 1, "one list kept");
    let (a, b) = tokio::join!(opened(&cat, record, t + 1, read.clone()), opened(&cat, record, t + 1, read.clone()));
    for v in [a, b] {
        assert_eq!((v.rows.len(), v.more.looking, v.more.no_list.clone()), (1, false, None), "each has the rows");
    }
    assert_eq!(asked.load(SeqCst), 1, "nothing read again");
}

/// F1.9 (kind 4): a read that never ends is cut at `PAGE_READ_AT_MOST` (a second in tests) and said unanswered: never `looking` for
/// good, never left "being read".
#[tokio::test]
async fn a_page_read_that_never_ends_is_cut_and_said_unanswered() {
    let _state = TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let record = wanted(&cat, "Answered", &[("musicbrainz", "release", "r19")]);
    let silent = |_: Ref| std::future::pending::<Result<Option<Read>>>();
    let t = crate::app::moment::now_secs();
    assert!(opened(&cat, record, t, silent).await.more.looking);
    tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
    page_read_settled(&cat, record).await;
    let v = opened(&cat, record, t + 2, silent).await;
    assert!(!v.more.looking, "cut: no longer looking");
    assert_eq!(v.more.no_list, Some(vec![saying("musicbrainz", "unanswered")]));
    assert_eq!(page_read_of(&cat, record), Some(false), "ended, not left being read");
}

/// F1.10 (kind 4): a record merged into another while its page's read waits on Bandcamp: the list lands on the record it is now,
/// whose page (opened by the old id) shows its rows. Never a list kept on a merged-away record.
#[tokio::test]
async fn a_list_read_during_a_merge_lands_on_the_record_it_is_now() {
    let _state = TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let a = wanted(&cat, "Kyverdale Road", &[("bandcamp", "album", "https://kitetheory.bandcamp.com/album/kyverdale-road")]);
    let b = wanted(&cat, "Kyverdale Road", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000510")]);
    let (gate, asked) = (Arc::new(tokio::sync::Semaphore::new(0)), Arc::new(AtomicUsize::new(0)));
    let read = gated(&gate, &asked);
    let t = crate::app::moment::now_secs();
    assert!(opened(&cat, a, t, read.clone()).await.more.looking);
    first_asked(&asked).await;
    cat.merge_records(a, b).unwrap();
    gate.add_permits(8);
    page_read_settled(&cat, a).await;
    let kept = cat.reference_tracks(b).unwrap();
    assert!(kept.iter().any(|(s, l)| s == "bandcamp" && l.len() == 1), "on the record it is now: {kept:?}");
    let on_a: i64 = cat.db_for_tests().query_row("SELECT COUNT(*) FROM reference_tracks WHERE record = ?1", [a], |r| r.get(0)).unwrap();
    assert_eq!(on_a, 0, "none on the record merged away");
    let v = opened(&cat, a, t + 1, read.clone()).await;
    assert_eq!((v.id.clone(), titles(&v)), (ids::record(b), vec!["Lamplight".to_string()]), "its old id opens the page it is now, with its rows");
}

/// F1.11, the pause (a pin): a page's read is the page's, not background reading's: it goes while background reading is paused,
/// its list kept and shown; nothing says "paused".
#[tokio::test]
async fn a_page_read_goes_while_background_reading_is_paused() {
    let _state = TestState::set().await;
    set_for_tests(BACKGROUND, false);
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let record = wanted(&cat, "Answered", &[("musicbrainz", "release", "r111")]);
    let asked = asks();
    let read = || reader(&[("musicbrainz", &["Lamplight", "Morning"])], asked.clone(), || Ok(None));
    let t = crate::app::moment::now_secs();
    let v = opened(&cat, record, t, read()).await;
    assert!(v.more.looking && v.more.no_list.is_none(), "read, no word of a pause");
    page_read_settled(&cat, record).await;
    let v = opened(&cat, record, t + 1, read()).await;
    assert_eq!((v.rows.len(), v.more.looking, v.more.no_list.clone()), (2, false, None));
    assert_eq!(cat.reference_tracks(record).unwrap().len(), 1, "its list kept");
    assert_eq!(*asked.lock().unwrap(), ["musicbrainz:release"]);
}

/// F1.11, Y12 (a pin): MusicBrainz switched off, Bandcamp's list answers: its rows show and Bandcamp counts; MusicBrainz switched
/// on later reads nothing new for it, its count kept. As before.
#[tokio::test]
async fn with_musicbrainz_off_a_pages_read_counts_bandcamp_and_keeps_it() {
    let _state = TestState::set().await;
    set_for_tests(MUSICBRAINZ, false);
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let record = wanted(&cat, "Answered", &[("musicbrainz", "release", "r112"), ("bandcamp", "album", PAGE)]);
    let asked = asks();
    let read = || minding(&[("musicbrainz", &["Other"]), ("bandcamp", &["Lamplight", "Morning"])], asked.clone());
    let t = crate::app::moment::now_secs();
    assert!(opened(&cat, record, t, read()).await.more.looking);
    page_read_settled(&cat, record).await;
    let shown = vec!["Lamplight".to_string(), "Morning".to_string()];
    let v = opened(&cat, record, t + 1, read()).await;
    assert_eq!((titles(&v), v.more.looking), (shown.clone(), false), "Bandcamp's rows");
    assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "VR10, changed by design: Bandcamp's list kept, nothing counted");
    set_for_tests(MUSICBRAINZ, true);
    let v = opened(&cat, record, t + 2, read()).await;
    assert_eq!((titles(&v), v.more.looking), (shown, false), "nothing new read");
    assert_eq!(*asked.lock().unwrap(), ["bandcamp:album"]);
    assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "VR10, changed by design: still nothing counted");
}

/// F1.11, RT01: a kept list let go of after thirty days unopened is read again at the next open, and shown.
#[tokio::test]
async fn a_list_let_go_of_after_thirty_days_is_read_again_when_opened() {
    let _state = TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let record = wanted(&cat, "Answered", &[("musicbrainz", "release", "r113")]);
    let asked = asks();
    let read = || reader(&[("musicbrainz", &["Lamplight"])], asked.clone(), || Ok(None));
    let t = crate::app::moment::now_secs();
    assert!(opened(&cat, record, t, read()).await.more.looking);
    page_read_settled(&cat, record).await;
    assert_eq!(opened(&cat, record, t + 1, read()).await.rows.len(), 1);
    cat.db_for_tests().execute("UPDATE reference_tracks SET at = ?1", [t - 45 * 86_400]).unwrap();
    cat.db_for_tests().execute("UPDATE records SET opened_at = ?1", [t - 45 * 86_400]).unwrap();
    assert_eq!(cat.forget_quiet_lists(t, &Default::default()).unwrap(), 1, "let go of");
    let v = opened(&cat, record, t + 2, read()).await;
    assert!(v.more.looking && v.rows.is_empty() && v.more.no_list.is_none(), "read again");
    page_read_settled(&cat, record).await;
    assert_eq!(opened(&cat, record, t + 3, read()).await.rows.len(), 1);
    assert_eq!(asked.lock().unwrap().len(), 2);
}

/// F1.11, Soulseek's Stuck: for a record whose sources all have none, `tracklist`'s why names the same sources as the page's
/// `no_list`: one set of outcomes, two wordings. Never two different source lists for one record.
#[tokio::test]
async fn the_page_and_soulseek_name_the_same_sources() {
    let _state = TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let record = wanted(&cat, "Answered", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000511"), ("discogs", "master", "4011"), ("bandcamp", "album", PAGE)]);
    let asked = asks();
    let none = || reader(&[], asked.clone(), || Ok(None));
    let t = crate::app::moment::now_secs();
    opened(&cat, record, t, none()).await;
    page_read_settled(&cat, record).await;
    let listed = opened(&cat, record, t + 1, none()).await.more.no_list.expect("the page says why");
    let why = tracklist(&cat, record, none()).await.unwrap().expect_err("no list to judge by");
    let named: Vec<&str> = identity::COUNTING_ORDER.iter().copied().filter(|s| why.contains(source_name(s))).collect();
    assert_eq!(listed.iter().map(|s| s.source).collect::<Vec<_>>(), named, "{why}");
    assert_eq!(named.len(), 3, "{why}");
    assert!(listed.iter().all(|s| s.said == "none"), "{listed:?}");
}

/// F18.2: records credited to nobody (opened from a label's rows by a name no artist has, two have, or Discogs marks as shared);
/// each release's read names its artists by Discogs id, one release a collaboration of two. Each artist found by the id, else the
/// one live artist of the name holding no Discogs id (credited, given no id: M1, changed by design), else one made; credited in
/// the release's order. A record credited before
/// keeps its credit (one the release contradicts is said in the log, nothing moved); never a second Discogs id on one artist.
#[tokio::test]
async fn a_release_read_credits_the_artists_it_names() {
    let _state = crate::app::references::TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let field = cat.artist_for("Noor Field").unwrap();
    let held = cat.artist_for("Orbit Lane").unwrap();
    assert!(cat.set_reference(&ids::artist(held), &Ref::new("discogs", "artist", "9300"), "link").unwrap());
    let (signal, together, lamplight) = (wanted(&cat, "Signal", &[("discogs", "release", "3800001")]), wanted(&cat, "Together", &[("discogs", "release", "3800002")]), wanted(&cat, "Lamplight", &[("discogs", "release", "3800003")]));
    let credited_before = wanted(&cat, "Undertow", &[("discogs", "release", "3800004")]);
    cat.db_for_tests().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'listing')", rusqlite::params![credited_before, field]).unwrap();
    let read = |r: Ref| {
        let artists: Vec<(Ref, String)> = match r.value.as_str() {
            "3800001" => vec![(Ref::new("discogs", "artist", "9101"), "Noor Field".into())],
            "3800002" => vec![(Ref::new("discogs", "artist", "9102"), "Kite Theory".into()), (Ref::new("discogs", "artist", "9300"), "Orbit Lane (2)".into())],
            "3800003" => vec![(Ref::new("discogs", "artist", "9103"), "Glass Harbour (2)".into())],
            _ => vec![(Ref::new("discogs", "artist", "9104"), "Tape Club".into())],
        };
        std::future::ready(Ok(Some((list(&[("One", Some(200_000))]), None, ReadMore { also: Vec::new(), artists }))))
    };
    let t = crate::app::moment::now_secs();
    for r in [signal, together, lamplight, credited_before] {
        opened(&cat, r, t, read).await;
        page_read_settled(&cat, r).await;
    }
    let artist_holding = |id: &str| cat.subject_of(&Ref::new("discogs", "artist", id)).unwrap().iter().find_map(|s| ids::artist_number(s));
    assert_eq!(cat.record_artists(signal).unwrap(), [field], "the one Noor Field holding no Discogs id");
    assert_eq!(artist_holding("9101"), None, "M1: credited by the name alone, given no id");
    let kite = cat.artist_of("Kite Theory").unwrap().expect("made by the release's id");
    assert_eq!(cat.record_artists(together).unwrap(), [kite, held], "a collaboration, in the release's order, each by their id");
    let made = cat.record_artists(lamplight).unwrap();
    assert_eq!(made.len(), 1);
    assert_ne!(cat.artist_named(made[0]).unwrap().unwrap().name, "", "a name Discogs marks as shared: an artist of their own");
    assert_eq!(artist_holding("9103"), Some(made[0]));
    assert_eq!(cat.record_artists(credited_before).unwrap(), [field], "credited before: kept");
    assert_eq!(artist_holding("9104"), None, "nothing moved, nothing made for it");
    let discogs_ids: i64 = cat.db_for_tests().query_row("SELECT max(n) FROM (SELECT count(*) AS n FROM \"references\" WHERE subject GLOB 'ar-*' AND source = 'discogs' AND kind = 'artist' GROUP BY subject)", [], |r| r.get(0)).unwrap();
    assert_eq!(discogs_ids, 1, "never a second Discogs id on one artist");
}

/// M1 (the review of 1.2; A1.8's strict rule, by another door): a record opened from a label's row by "Coma", credited to nobody;
/// its Discogs release names artist "Coma", Discogs id 12. The library's one Coma holds no Discogs id, so only the name says they
/// are that Coma: the record credited to them, and no Discogs id put on them (12 held by nobody). A Discogs id goes to an artist by
/// an id path alone: MusicBrainz's link, or the artist who holds it.
#[tokio::test]
async fn a_release_read_never_gives_an_artist_an_id_by_their_name_alone() {
    let _state = crate::app::references::TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let coma = cat.artist_for("Coma").unwrap();
    let record = wanted(&cat, "Northern Lights", &[("discogs", "release", "3800021")]);
    let read = |_: Ref| std::future::ready(Ok(Some((list(&[("One", Some(200_000))]), None, ReadMore { also: Vec::new(), artists: vec![(Ref::new("discogs", "artist", "12"), "Coma".into())] }))));
    opened(&cat, record, crate::app::moment::now_secs(), read).await;
    page_read_settled(&cat, record).await;
    assert_eq!(cat.record_artists(record).unwrap(), [coma], "credited to the library's one Coma");
    assert!(cat.subject_of(&Ref::new("discogs", "artist", "12")).unwrap().is_empty(), "no Discogs id by the name alone");
    assert!(cat.references_of(&ids::artist(coma)).unwrap().is_empty(), "nothing put on them");
}

/// F14.5: a record not had "Night Walks" with MusicBrainz release R, whose read (asked with `url-rels`) has two tracks and its Bandcamp
/// page P: P kept on it (`link`, its page then plays its preview). A second record not had holding P alone is the same album: the newer
/// into the older. A had record holding P keeps it: nothing joined, P not moved. Never P on two records.
#[tokio::test]
async fn a_releases_bandcamp_link_is_kept_with_its_tracklist() {
    let _state = crate::app::references::TestState::set().await;
    const P: &str = "https://kitetheory.bandcamp.com/album/night-walks";
    let read = |_: Ref| std::future::ready(Ok(Some((list(&[("Lamplight", Some(200_000)), ("Morning", Some(180_000))]), None, ReadMore { also: vec![Ref::new("bandcamp", "album", P)], artists: Vec::new() }))));
    let t = crate::app::moment::now_secs();
    // Kept.
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let night = wanted(&cat, "Night Walks", &[("musicbrainz", "release", "00000000-0000-4000-8000-000000000b51")]);
    opened(&cat, night, t, read).await;
    page_read_settled(&cat, night).await;
    assert_eq!(cat.subject_of(&Ref::new("bandcamp", "album", P)).unwrap(), [ids::record(night)], "P kept on it");
    assert!(opened(&cat, night, t, read).await.more.bandcamp.is_some(), "its preview");
    // A record not had holding P alone: joined.
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let older = wanted(&cat, "Night Walks", &[("bandcamp", "album", P)]);
    let night = wanted(&cat, "Night Walks", &[("musicbrainz", "release", "00000000-0000-4000-8000-000000000b52")]);
    opened(&cat, night, t, read).await;
    page_read_settled(&cat, night).await;
    assert_eq!(cat.record(night).unwrap().unwrap().id, older, "the newer into the older");
    // A had record holding P: nothing joined.
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let had = cat.record_for_tests("Night Walks", 2);
    assert!(cat.set_reference(&ids::record(had), &Ref::new("bandcamp", "album", P), "link").unwrap());
    let night = wanted(&cat, "Night Walks", &[("musicbrainz", "release", "00000000-0000-4000-8000-000000000b53")]);
    opened(&cat, night, t, read).await;
    page_read_settled(&cat, night).await;
    assert_eq!(cat.record(night).unwrap().unwrap().id, night, "a had record's page: no merge");
    assert_eq!(cat.subject_of(&Ref::new("bandcamp", "album", P)).unwrap(), [ids::record(had)], "P not moved");
}

// ---- F13: a busy source is passed over, and its list counts once it answers

/// How the tests' MusicBrainz answers (F13): busy (a 503), its list of two at once, none ("not found"), its list once the gate
/// opens, or busy once the gate opens.
const BUSY: u8 = 0;
const ANSWERS: u8 = 1;
const NOT_FOUND: u8 = 2;
const GATED: u8 = 3;
const BUSY_AT_GATE: u8 = 4;

/// A reader whose MusicBrainz answers as `mode` says; Discogs and Bandcamp answer two tracks with lengths (Lamplight, Morning) at
/// once. Each question kept in `asked` (source:kind).
fn switching(mode: &Arc<std::sync::atomic::AtomicU8>, gate: &Arc<tokio::sync::Semaphore>, asked: &Arc<std::sync::Mutex<Vec<String>>>) -> impl Fn(Ref) -> Asking + Clone + Send + Sync + 'static {
    let (mode, gate, asked) = (mode.clone(), gate.clone(), asked.clone());
    move |r: Ref| -> Asking {
        asked.lock().unwrap().push(format!("{}:{}", r.source, r.kind));
        let (mode, gate) = (mode.load(SeqCst), gate.clone());
        Box::pin(async move {
            let two = || -> Result<Option<Read>> { Ok(Some((list(&[("Lamplight", Some(200_000)), ("Morning", Some(180_000))]), None, ReadMore::default()))) };
            let busy = || -> Result<Option<Read>> { Err(anyhow::anyhow!("MusicBrainz answered 503 Service Unavailable")) };
            if r.source != "musicbrainz" {
                return two();
            }
            match mode {
                BUSY => busy(),
                NOT_FOUND => Ok(None),
                GATED | BUSY_AT_GATE => {
                    let _open = gate.acquire().await;
                    if mode == GATED { two() } else { busy() }
                }
                _ => two(),
            }
        })
    }
}

/// F13's reader with its own mode, gate and log.
#[allow(clippy::type_complexity)]
fn switched(mode: u8) -> (Arc<std::sync::atomic::AtomicU8>, Arc<tokio::sync::Semaphore>, Arc<std::sync::Mutex<Vec<String>>>) {
    (Arc::new(std::sync::atomic::AtomicU8::new(mode)), Arc::new(tokio::sync::Semaphore::new(0)), asks())
}

/// Waits until `asked` holds `what` `n` times (a read at its question).
async fn asked_times(asked: &std::sync::Mutex<Vec<String>>, what: &str, n: usize) {
    for _ in 0..1_000 {
        if asked.lock().unwrap().iter().filter(|a| *a == what).count() >= n {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("{what} never asked {n} times");
}

/// The sources a page marks as giving its tracks.
fn marked(v: &crate::app::AlbumView) -> Vec<&'static str> {
    v.more.sources.iter().filter(|s| s.tracks).map(|s| s.source).collect()
}

/// The sources whose lists a record keeps.
fn kept_sources(cat: &Catalogue, record: i64) -> Vec<String> {
    cat.reference_tracks(record).unwrap().into_iter().map(|(s, _)| s).collect()
}

/// F13's MusicBrainz release `r13` (an id as MusicBrainz's are, so About can link it).
const R13: &str = "00000000-0000-4000-8000-000000000f13";

/// F13's record: "Two Rooms" with MusicBrainz release `r13` and Discogs release `2000013`, nothing kept.
fn two_rooms(cat: &Catalogue) -> i64 {
    wanted(cat, "Two Rooms", &[("musicbrainz", "release", R13), ("discogs", "release", "2000013")])
}

/// F13.1: MusicBrainz busy, Discogs answering two tracks. At t: being read, MusicBrainz first; a minute on: Discogs' rows, Discogs
/// marked as giving them, no reason line; nothing counted; Discogs' list kept; asked MusicBrainz, then Discogs. Never counted
/// "discogs"; never a reason beside rows; never Discogs before MusicBrainz.
#[tokio::test]
async fn a_busy_source_is_passed_over_and_the_next_list_shows_uncounted() {
    let _state = TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let record = two_rooms(&cat);
    let (mode, gate, asked) = switched(BUSY);
    let read = switching(&mode, &gate, &asked);
    let t = crate::app::moment::now_secs();
    let first = opened(&cat, record, t, read.clone()).await;
    assert!(first.more.looking, "at t: being read");
    assert_eq!(first.more.reading, Some("musicbrainz"));
    page_read_settled(&cat, record).await;
    let later = opened(&cat, record, t + 60, read).await;
    assert_eq!(titles(&later), ["Lamplight", "Morning"], "a minute on: Discogs' rows");
    assert_eq!(marked(&later), ["discogs"]);
    assert!(!later.more.looking && later.more.no_list.is_none(), "no reason line beside rows");
    assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "never counted");
    assert_eq!(kept_sources(&cat, record), ["discogs"]);
    assert_eq!(*asked.lock().unwrap(), ["musicbrainz:release", "discogs:release"], "MusicBrainz first, then Discogs");
}

/// MusicBrainz busy (a 503); every other source has no list of it. Each question kept.
fn busy_else_none(asked: &Arc<std::sync::Mutex<Vec<String>>>) -> impl Fn(Ref) -> std::future::Ready<Result<Option<Read>>> + Send + Sync + 'static {
    let none = reader(&[], asked.clone(), || Ok(None));
    let asked = asked.clone();
    move |r: Ref| {
        if r.source == "musicbrainz" {
            asked.lock().unwrap().push(format!("{}:{}", r.source, r.kind));
            return std::future::ready(Err(anyhow::anyhow!("MusicBrainz answered 503 Service Unavailable")));
        }
        none(r)
    }
}

/// F13.2: no source gives a list (MusicBrainz busy, Discogs none, Bandcamp held back after its check): each named once, in counting
/// order, Bandcamp with its hold; nothing kept or counted. Never cut after the first unanswered; never "none" said of MusicBrainz;
/// Bandcamp never asked while held.
#[tokio::test]
async fn every_source_is_named_where_none_gives_a_list() {
    let _state = TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let record = wanted(&cat, "Two Rooms", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000e13"), ("discogs", "master", "4013"), ("bandcamp", "album", PAGE)]);
    let t = crate::app::moment::now_secs();
    crate::bandcamp::hold_for_tests(t + 3600);
    let asked = asks();
    assert!(opened(&cat, record, t, busy_else_none(&asked)).await.more.looking);
    page_read_settled(&cat, record).await;
    let v = opened(&cat, record, t + 60, busy_else_none(&asked)).await;
    let held = crate::bandcamp::held_until(t + 60);
    assert!(held.is_some());
    assert_eq!(v.more.no_list, Some(vec![saying("musicbrainz", "unanswered"), saying("discogs", "none"), Said { source: "bandcamp", said: "held", until: held }]));
    assert!(!v.more.looking && v.rows.is_empty());
    assert!(!asked.lock().unwrap().iter().any(|a| a.starts_with("bandcamp")), "Bandcamp not asked while held");
    assert!(cat.reference_tracks(record).unwrap().is_empty() && cat.record(record).unwrap().unwrap().counting.is_none(), "nothing kept or counted");
}

/// F13.3: F13.1 a minute on, then MusicBrainz answers: (a) at t + 300 Discogs' rows, MusicBrainz not asked again; (b) at t + 960 it is
/// read again while Discogs' rows show (never empty or loading), MusicBrainz once more and Discogs not again; after it, counted
/// "musicbrainz", its rows marked as MusicBrainz's, Discogs' list still kept. (c) A record whose MusicBrainz release then answers
/// "not found": counted "discogs" (the next list), no further MusicBrainz reads.
#[tokio::test]
async fn the_busy_source_counts_once_it_answers() {
    let _state = TestState::set().await;
    let mut catalogues = Vec::new();
    for (case, then) in [("(b)", GATED), ("(c)", NOT_FOUND)] {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        catalogues.push(cat.clone());
        let record = two_rooms(&cat);
        let (mode, gate, asked) = switched(BUSY);
        let read = switching(&mode, &gate, &asked);
        let t = crate::app::moment::now_secs();
        opened(&cat, record, t, read.clone()).await;
        page_read_settled(&cat, record).await;
        assert_eq!(titles(&opened(&cat, record, t + 60, read.clone()).await), ["Lamplight", "Morning"], "{case}: F13.1 a minute on");
        mode.store(then, SeqCst);
        let soon = opened(&cat, record, t + 300, read.clone()).await;
        assert_eq!(titles(&soon), ["Lamplight", "Morning"], "{case} (a): Discogs' rows");
        assert_eq!(asked.lock().unwrap().len(), 2, "{case} (a): MusicBrainz not asked inside the quarter hour");
        let again = opened(&cat, record, t + 960, read.clone()).await;
        assert!(!again.more.looking && titles(&again) == ["Lamplight", "Morning"], "{case}: the rows while MusicBrainz is read again");
        if then == GATED {
            asked_times(&asked, "musicbrainz:release", 2).await;
            let during = opened(&cat, record, t + 961, read.clone()).await;
            assert!(!during.more.looking && titles(&during) == ["Lamplight", "Morning"], "(b): never empty or loading while it is read");
            gate.add_permits(8);
        }
        page_read_settled(&cat, record).await;
        assert_eq!(asked.lock().unwrap()[2..], ["musicbrainz:release"], "{case}: MusicBrainz once more, Discogs not again");
        let after = opened(&cat, record, t + 1000, read.clone()).await;
        if then == GATED {
            assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "(b) VR1, changed by design: its list kept, nothing counted");
            assert_eq!((titles(&after), marked(&after)), (vec!["Lamplight".to_string(), "Morning".to_string()], vec!["musicbrainz"]), "(b): MusicBrainz's rows");
            assert!(kept_sources(&cat, record).iter().any(|s| s == "discogs"), "(b): Discogs' list still kept");
        } else {
            assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "(c) VR1, changed by design: the lists kept, nothing counted");
            opened(&cat, record, t + 2000, read.clone()).await;
            page_read_settled(&cat, record).await;
            assert_eq!(asked.lock().unwrap().iter().filter(|a| a.starts_with("musicbrainz")).count(), 2, "(c): no further MusicBrainz reads");
        }
    }
}

/// F13.4: (a) two opens at once while MusicBrainz is busy: one read, MusicBrainz and Discogs asked once each, both opens looking;
/// (b) Soulseek's judging while the page's read waits at the gate stops at MusicBrainz (R23); the page's read keeps Discogs' list
/// uncounted; neither counts anything. Twenty rounds.
#[tokio::test]
async fn two_opens_while_musicbrainz_is_busy_read_once() {
    let _state = TestState::set().await;
    let mut catalogues = Vec::new();
    for round in 0..20 {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        catalogues.push(cat.clone());
        let record = two_rooms(&cat);
        let (mode, gate, asked) = switched(BUSY_AT_GATE);
        let page = switching(&mode, &gate, &asked);
        let t = crate::app::moment::now_secs();
        let (a, b) = tokio::join!(opened(&cat, record, t, page.clone()), opened(&cat, record, t, page.clone()));
        assert!(a.more.looking && b.more.looking, "round {round}: both say it is being read");
        asked_times(&asked, "musicbrainz:release", 1).await;
        let (busy, other_gate, soulseek_asked) = switched(BUSY);
        assert!(tracklist(&cat, record, switching(&busy, &other_gate, &soulseek_asked)).await.is_err(), "round {round}: Soulseek stops at MusicBrainz");
        assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "round {round}: Soulseek counted nothing");
        gate.add_permits(8);
        page_read_settled(&cat, record).await;
        assert_eq!(*asked.lock().unwrap(), ["musicbrainz:release", "discogs:release"], "round {round}: one page read, each asked once");
        assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "round {round}: nothing counted");
        assert_eq!(kept_sources(&cat, record), ["discogs"], "round {round}: Discogs' list kept, uncounted");
    }
}

/// F13.5: (a) merged into another while its read waits at the gate: Discogs' list lands on the record it is now, uncounted (as
/// F1.10); (b) MusicBrainz switched off at the first read (Discogs busy), on now: read at once at the next open, and counted (as
/// F1.6); (c) had meanwhile (its files arrived while its read ran): nothing counted by the read, and no page read after.
#[tokio::test]
async fn a_busy_source_passed_over_meets_merges_switches_and_arrivals() {
    let _state = TestState::set().await;
    let t = crate::app::moment::now_secs();
    // (a)
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let a = two_rooms(&cat);
    let b = wanted(&cat, "Two Rooms", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000e15")]);
    let (mode, gate, asked) = switched(BUSY_AT_GATE);
    let read = switching(&mode, &gate, &asked);
    assert!(opened(&cat, a, t, read.clone()).await.more.looking);
    asked_times(&asked, "musicbrainz:release", 1).await;
    cat.merge_records(a, b).unwrap();
    gate.add_permits(8);
    page_read_settled(&cat, a).await;
    assert_eq!(kept_sources(&cat, b), ["discogs"], "(a) on the record it is now");
    let on_a: i64 = cat.db_for_tests().query_row("SELECT COUNT(*) FROM reference_tracks WHERE record = ?1", [a], |r| r.get(0)).unwrap();
    assert_eq!(on_a, 0, "(a) none on the record merged away");
    assert_eq!(cat.record(b).unwrap().unwrap().counting, None, "(a) uncounted");
    // (b)
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let c = two_rooms(&cat);
    let asked = asks();
    let read = || reader(&[("musicbrainz", &["Lamplight", "Morning"])], asked.clone(), || Err(anyhow::anyhow!("Discogs answered 503 Service Unavailable")));
    set_for_tests(MUSICBRAINZ, false);
    assert!(opened(&cat, c, t, read()).await.more.looking);
    page_read_settled(&cat, c).await;
    assert_eq!(opened(&cat, c, t + 1, read()).await.more.no_list, Some(vec![saying("musicbrainz", "off"), saying("discogs", "unanswered")]));
    set_for_tests(MUSICBRAINZ, true);
    assert!(opened(&cat, c, t + 60, read()).await.more.looking, "(b) read at once");
    page_read_settled(&cat, c).await;
    assert_eq!(cat.record(c).unwrap().unwrap().counting, None, "(b) VR1, changed by design: read, not counted");
    // (c)
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let d = wanted(&cat, "Two Rooms", &[("musicbrainz", "release", "r15")]);
    let (mode, gate, asked) = switched(GATED);
    let read = switching(&mode, &gate, &asked);
    assert!(opened(&cat, d, t, read.clone()).await.more.looking);
    asked_times(&asked, "musicbrainz:release", 1).await;
    cat.db_for_tests().execute("INSERT INTO tracks (born, record, disc, number) VALUES (1, ?1, 1, 1)", [d]).unwrap();
    gate.add_permits(8);
    page_read_settled(&cat, d).await;
    assert_eq!(cat.record(d).unwrap().unwrap().counting, None, "(c) a had record counted nothing by a page's read");
    assert!(!matches!(open_at(&cat, d, t + 1, read.clone()).await.unwrap(), Some(RecordPage::NotHad(_))), "(c) had: no page of a record not had");
    assert_eq!(asked.lock().unwrap().len(), 1, "(c) no page read after");
}

/// F13.6: Discogs' list kept uncounted, MusicBrainz still busy: Soulseek's judging waits (R23: `Later`, its words MusicBrainz's),
/// never judged by Discogs' uncounted list; once MusicBrainz answers with lengths, judged by its list, counted "musicbrainz".
#[tokio::test]
async fn soulseek_waits_for_a_busy_source_though_the_page_shows_the_next() {
    let _state = TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let record = two_rooms(&cat);
    let (mode, gate, asked) = switched(BUSY);
    let read = switching(&mode, &gate, &asked);
    let t = crate::app::moment::now_secs();
    opened(&cat, record, t, read.clone()).await;
    page_read_settled(&cat, record).await;
    assert_eq!(titles(&opened(&cat, record, t + 60, read.clone()).await), ["Lamplight", "Morning"], "the page shows Discogs' list");
    let why = format!("{:#}", tracklist(&cat, record, read.clone()).await.err().expect("later, never judged by Discogs' uncounted list"));
    assert!(why.contains("MusicBrainz"), "{why}");
    assert_eq!(cat.record(record).unwrap().unwrap().counting, None);
    mode.store(ANSWERS, SeqCst);
    let judged = tracklist(&cat, record, read).await.unwrap().unwrap();
    assert_eq!(judged.timed.map(|(s, _)| s).as_deref(), Some("musicbrainz"));
    assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "VR10, changed by design: nothing counted");
}

/// RV12 (the owner's "built once"; ENT.14's record half, its test name unchanged): the record's read runs at its open, as a job on
/// the runner, never on its GET. "Answered", a row not had by MusicBrainz group `mb:rg-ans` and Discogs master `9364`; its release
/// credits Kite Theory (`mb:kite`, held by nobody) and Glass Harbour (`mb:gh`, held by `ar-363`); `al-364` holds the master by
/// another path. (c) GETs before the open ask nothing, make, credit and merge nothing, and say the read is under way; (a) the open
/// begins the read as a job (`record:read`); (b) the job does what F18.2's read did: Kite Theory made and credited, Glass Harbour
/// credited, the records merged by the shared master; a GET after makes nothing more. Red first: the GET began the read
/// (`page_at` → `read_soon_with`), which made, credited and merged.
#[tokio::test]
async fn the_record_read_runs_at_its_open_not_its_get() {
    let _state = crate::app::references::TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let glass = cat.artist_for("Glass Harbour").unwrap();
    assert!(cat.set_reference(&ids::artist(glass), &Ref::new("musicbrainz", "artist", "mb:gh"), "link").unwrap());
    let answered = wanted(&cat, "Answered", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000365")]);
    let other = wanted(&cat, "Answered", &[("discogs", "master", "9364")]);
    let asked = Arc::new(AtomicUsize::new(0));
    // The read waits at a gate, so the job is seen running before it answers.
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let read = {
        let (asked, gate) = (asked.clone(), gate.clone());
        move |_: Ref| {
            let (asked, gate) = (asked.clone(), gate.clone());
            async move {
                let _open = gate.acquire().await.unwrap();
                asked.fetch_add(1, SeqCst);
                let artists = vec![(Ref::new("musicbrainz", "artist", "mb:kite"), "Kite Theory".into()), (Ref::new("musicbrainz", "artist", "mb:gh"), "Glass Harbour".into())];
                Ok::<_, anyhow::Error>(Some((list(&[("Lamplight", Some(200_000))]), None, ReadMore { also: vec![Ref::new("discogs", "master", "9364")], artists })))
            }
        }
    };
    let t = crate::app::moment::now_secs();
    let gets = |cat: &Arc<Catalogue>, before: bool| {
        let cat = cat.clone();
        async move {
            for _ in 0..2 {
                let Some(RecordPage::NotHad(v)) = crate::app::playing::page_at(&cat, answered, t).await.unwrap() else { panic!("a page") };
                assert!(!before || v.more.looking, "before the job, the page says its read is under way, as the loader does");
            }
        }
    };
    gets(&cat, true).await;
    assert_eq!(asked.load(SeqCst), 0, "(c) a GET asks nothing");
    assert!(cat.artist_of("Kite Theory").unwrap().is_none() && cat.record_artists(answered).unwrap().is_empty(), "(c) no artist made or credited by a GET");
    assert_eq!(cat.record(other).unwrap().unwrap().id, other, "(c) no record merged by a GET");
    // (a) the open begins the read as a job on the runner, keyed by the record; (b) it reads, credits and merges as F18.2's did.
    opened(&cat, answered, t, read).await;
    let running = crate::runner::runner().begin_keyed(crate::app::references::READ_KIND, &ids::record(answered), None);
    assert!(matches!(running, Err(Ok(_))), "(a) the record's read runs as a job");
    gate.add_permits(16);
    page_read_settled(&cat, answered).await;
    assert!(asked.load(SeqCst) >= 1, "(b) read");
    let kite = cat.artist_of("Kite Theory").unwrap().expect("(b) made by mb:kite");
    let into = cat.record(answered).unwrap().unwrap().id;
    let credited = cat.record_artists(into).unwrap();
    assert!(credited.contains(&kite) && credited.contains(&glass), "(b) Kite Theory and Glass Harbour credited: {credited:?}");
    assert_eq!(cat.record(other).unwrap().unwrap().id, cat.record(answered).unwrap().unwrap().id, "(b) merged by the shared master");
    let before = asked.load(SeqCst);
    gets(&cat, false).await;
    assert_eq!(asked.load(SeqCst), before, "a GET after asks nothing either");
}

/// RV12 (d) (DC3 RLB.3 (d)): a record made in 0.17 and never read is due its read on the server's schedule, one a tick, never in
/// bulk: the due job credits and merges as the open's read does; a second record waits for the next tick; one read now is not due
/// again until its lists are stale. The schedule is the runner's (`Runner::every`, kind `record:read`). Red first: no schedule
/// (a record never opened was never read).
#[tokio::test]
async fn a_record_never_read_is_read_on_the_schedule() {
    let _state = crate::app::references::TestState::set().await;
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let glass = cat.artist_for("Glass Harbour").unwrap();
    assert!(cat.set_reference(&ids::artist(glass), &Ref::new("musicbrainz", "artist", "mb:gh"), "link").unwrap());
    // A Bandcamp page alone: never due (no job reads Bandcamp, decision 0109); read at its next open.
    let bandcamp_only = wanted(&cat, "Further", &[("bandcamp", "album", "https://grouch.bandcamp.com/album/further")]);
    let first = wanted(&cat, "Answered", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000375"), ("bandcamp", "album", "https://kitetheory.bandcamp.com/album/answered")]);
    let second = wanted(&cat, "Two Rooms", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000376")]);
    let asked = Arc::new(AtomicUsize::new(0));
    let bandcamp = Arc::new(AtomicUsize::new(0));
    let read = {
        let (asked, bandcamp) = (asked.clone(), bandcamp.clone());
        move |r: Ref| {
            if r.source == "bandcamp" {
                bandcamp.fetch_add(1, SeqCst);
            }
            asked.fetch_add(1, SeqCst);
            let artists = vec![(Ref::new("musicbrainz", "artist", "mb:gh"), "Glass Harbour".into())];
            std::future::ready(Ok(Some((list(&[("Lamplight", Some(200_000))]), None, ReadMore { also: Vec::new(), artists }))))
        }
    };
    let t = crate::app::moment::now_secs();
    let tick = crate::app::references::due_read_tick(&cat, t, read.clone()).await.unwrap();
    assert_eq!(tick, Some(first), "the oldest due, alone");
    assert_eq!(asked.load(SeqCst), 1, "one record a tick, never in bulk");
    assert_eq!(cat.record_artists(first).unwrap(), [glass], "credited as the open's read credits");
    assert!(cat.record_artists(second).unwrap().is_empty(), "the second waits");
    assert_eq!(crate::app::references::due_read_tick(&cat, t, read.clone()).await.unwrap(), Some(second), "the next tick, the next");
    assert_eq!(crate::app::references::due_read_tick(&cat, t + 60, read).await.unwrap(), None, "nothing due until its lists are stale");
    assert_eq!(bandcamp.load(SeqCst), 0, "the due read asks Bandcamp nothing");
    assert!(cat.reference_tracks(bandcamp_only).unwrap().is_empty(), "a Bandcamp page alone is never due");
    assert!(crate::app::references::start_due_reads().is_ok() || crate::runner::runner().schedules().iter().any(|s| s.kind == "record:read"));
    let scheduled = crate::runner::runner().schedules();
    assert!(scheduled.iter().any(|s| s.kind == "record:read" && s.period_secs == crate::app::references::DUE_EVERY.as_secs()), "{scheduled:?}");
    assert!(crate::app::references::start_due_reads().is_err(), "started once");
}
