//! `labels`' tests that read a record's page, moved up out of records for the split's phase 2: About's chips as the page
//! shows a record's labels (`playing::record_labels_shown_for_tests`), and two releases of one master opened through their
//! pages (`playing::page_at`). Both sit above records. Their bodies are as they were; `super::super::moment` is named from
//! the crate.

use std::sync::Arc;

use crate::app::labels::about;
use crate::app::labels::tests::{discogs_row, open_row};
use crate::catalogue::Catalogue;
use crate::catalogue::records::Ref;
use crate::serve::ids;

/// A2.5.1, A2.5.2: About shows a label once. Discogs names Traum Schallplatten (2880) twice, once per catalogue number: one chip,
/// "TRAUM CD21, TRAUM SCHALLPLATTEN CD21". Two labels of one name with two Discogs ids: two chips. An entry with no id beside one
/// with an id of the same bare key: one, the id's. A record tied to one label with two catalogue numbers: one, both numbers.
#[test]
fn about_shows_a_label_once_with_its_catalogue_numbers() {
    let cat = Catalogue::open(None, Vec::new()).unwrap();
    let l = |name: &str, catno: Option<&str>, id: Option<u64>| crate::app::discogs::Label { name: name.into(), catno: catno.map(str::to_string), id, namesake: false };
    let traum = [l("Traum Schallplatten", Some("TRAUM CD21"), Some(2880)), l("Traum Schallplatten", Some("TRAUM SCHALLPLATTEN CD21"), Some(2880))];
    let shown = about(&cat, &traum, &[]).unwrap();
    assert_eq!(shown.len(), 1, "one chip: {shown:?}");
    assert_eq!(shown[0].catno.as_deref(), Some("TRAUM CD21, TRAUM SCHALLPLATTEN CD21"));
    let two = about(&cat, &[l("Source", Some("A"), Some(1)), l("Source", Some("B"), Some(2))], &[]).unwrap();
    assert_eq!(two.len(), 2, "two ids: two labels");
    let one = about(&cat, &[l("Warp", Some("W1"), None), l("Warp Records", Some("W2"), Some(23528))], &[]).unwrap();
    assert_eq!(one.len(), 1, "no id beside an id of the same bare key: one");
    assert_eq!(one[0].reference.as_ref().map(|r| r.id.as_str()), Some("23528"), "the id's");
    assert_eq!(one[0].catno.as_deref(), Some("W1, W2"));
    let mb = [crate::app::discogs::MbLabel { name: "Warp Records Ltd.".into(), mbid: "00000000-0000-4000-8000-000000000909".into(), catno: None }];
    assert_eq!(about(&cat, &[l("Warp", None, Some(23528))], &mb).unwrap().len(), 1, "MusicBrainz's of the same bare key: not again");
    assert_eq!(crate::app::playing::record_labels_shown_for_tests(&cat), (1, Some("TRAUM CD21, TRAUM SCHALLPLATTEN CD21".to_string())), "a record's: one, both numbers");
}

/// F15.1: a label's kept Discogs rows "Two Rooms" by Glass Harbour, releases 3600001 and 3600002, no master on the rows; each
/// release's read names master 5000001. Each row opened, each page's read settled: one live record holding both releases and the
/// master (`link`), the second open's id answering as the first's, one Discogs list kept (the first's), and the memory holding
/// both releases' master.
#[tokio::test]
async fn two_releases_of_one_master_become_one_record_once_the_master_is_known() {
    let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
    let label = cat.label_for("Low Tide Records").unwrap();
    let rows = [discogs_row("3600001", "Two Rooms", "Glass Harbour", "LTR-21"), discogs_row("3600002", "Two Rooms", "Glass Harbour", "LTR-22")];
    cat.keep_label_rows(label, "discogs", &rows, None, true, 1).unwrap();
    let read = |r: Ref| {
        let track = |n: u32, t: &str| crate::catalogue::identity::RefTrack { disc: 1, position: n, title: t.into(), length_ms: Some(200_000), recording: None };
        let more = crate::app::references::ReadMore { also: vec![Ref::new("discogs", "master", "5000001")], artists: Vec::new() };
        std::future::ready(Ok(Some((vec![track(1, &format!("Lamplight {}", r.value)), track(2, "Morning")], None, more))))
    };
    let t = crate::app::moment::now_secs();
    let first = open_row(&cat, label, &rows[0]);
    crate::app::playing::page_at(&cat, first, t, read).await.unwrap();
    crate::app::references::page_read_settled(&cat, first).await;
    let second = open_row(&cat, label, &rows[1]);
    assert_ne!(second, first, "unknown at the open: a record of its own");
    crate::app::playing::page_at(&cat, second, t, read).await.unwrap();
    crate::app::references::page_read_settled(&cat, second).await;
    assert_eq!(cat.record(second).unwrap().unwrap().id, first, "the second answers as the first");
    let held: Vec<(String, String, String)> = cat.references_of(&ids::record(first)).unwrap().into_iter().filter(|(r, _)| r.source == "discogs").map(|(r, row)| (r.kind, r.value, row.how)).collect();
    for (kind, value) in [("release", "3600001"), ("release", "3600002"), ("master", "5000001")] {
        assert!(held.iter().any(|(k, v, _)| k == kind && v == value), "holds {kind} {value}: {held:?}");
    }
    assert!(held.iter().any(|(k, v, how)| k == "master" && v == "5000001" && how == "link"), "the master by `link`");
    let lists = cat.reference_tracks(first).unwrap();
    assert_eq!(lists.iter().filter(|(s, _)| s == "discogs").count(), 1, "one Discogs list kept");
    assert!(lists.iter().any(|(s, l)| s == "discogs" && l.first().is_some_and(|t| t.title == "Lamplight 3600001")), "the first's");
    let remembered: i64 = cat.db_for_tests().query_row("SELECT count(*) FROM discogs_masters WHERE master = 5000001 AND release IN (3600001, 3600002)", [], |r| r.get(0)).unwrap();
    assert_eq!(remembered, 2, "the memory holds both releases' master");
    let live: i64 = cat.db_for_tests().query_row("SELECT count(*) FROM records WHERE merged_into IS NULL AND id IN (?1, ?2)", rusqlite::params![first, second], |r| r.get(0)).unwrap();
    assert_eq!(live, 1, "never two live records of one master");
}
