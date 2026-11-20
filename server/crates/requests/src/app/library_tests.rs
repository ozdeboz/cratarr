//! `library`'s tests of a start that runs 0.17's step, moved up out of base for the split's phase 2: the step is the
//! requests' (`step_017`), and a start runs it as App::new hands it in. Their bodies are as they were; their start hands
//! the step in, as App::new does.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use crate::app::library::switched_off_tests::data_folder;
use crate::app::library::{CATALOGUE_KEY, Library};

/// A start as App::new makes it: 0.17's step handed in.
async fn started(d: &Path, db: &Arc<crate::userdata::UserData>) -> Result<Arc<Library>> {
    Library::new(Some(d.to_string_lossy().to_string()), None, Vec::new(), Default::default(), None, db.clone(), Some(crate::app::step_017::run)).await
}

fn lost(d: &Path) {
    for file in ["catalogue.sqlite", "catalogue.sqlite-wal", "catalogue.sqlite-shm"] {
        std::fs::remove_file(d.join(file)).ok();
    }
}

/// A catalogue lost after the first start and started afresh is the user data's, though it gave no record then: taken for one
/// from before the first start, it would stop every start from then on. P3.6 (W3): while its ids are lost the step waits, its
/// credits part too (no mark); started afresh, it runs.
#[tokio::test]
async fn a_lost_catalogue_started_afresh_after_the_first_start_still_starts() {
    let (d, db) = data_folder("afresh");
    drop(started(&d, &db).await.unwrap());
    assert!(db.first_start_done().unwrap());
    lost(&d);
    let lib = started(&d, &db).await.unwrap();
    assert!(lib.catalogue().ids_lost());
    assert!(lib.catalogue().meta("017:credits").is_none(), "the ids lost: the credits part waits");
    lib.fresh_track_ids().unwrap();
    drop(lib);
    let lib = started(&d, &db).await.expect("it starts");
    assert!(!lib.catalogue().ids_lost());
    assert!(lib.catalogue().meta("017:credits").is_some(), "started afresh: the step and its credits part ran");
    drop(lib);
    std::fs::remove_dir_all(&d).ok();
}

/// FX01's data folder (0.16.2's), and its user data.
fn data_0_16(name: &str) -> (PathBuf, Arc<crate::userdata::UserData>) {
    let d = std::env::temp_dir().join(format!("cratarr-library-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    crate::catalogue::records::data_0_16(&d).unwrap();
    let db = Arc::new(crate::userdata::UserData::open(Some(&d.join("userdata.sqlite"))).unwrap());
    (d, db)
}

/// The parts of 0.17's step marked done in a data folder's catalogue.
fn marks_017(d: &Path) -> Vec<String> {
    let db = rusqlite::Connection::open(d.join("catalogue.sqlite")).unwrap();
    let mut st = db.prepare("SELECT key FROM meta WHERE key GLOB '017:*' ORDER BY key").unwrap();
    st.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

/// R5: a start runs 0.17's step on a data folder 0.16.2 left: every part marked, the report there; a second start leaves
/// both as they were.
#[tokio::test]
async fn a_start_runs_the_step_and_leaves_its_marks() {
    let (d, db) = data_0_16("step-runs");
    drop(started(&d, &db).await.unwrap());
    let marks = marks_017(&d);
    assert_eq!(marks.len(), crate::catalogue::records::PARTS_017.len() + 3, "and the band waves', the credits' and the fold's: {marks:?}");
    let report = db.migration_report().unwrap()[crate::app::step_017::PART].clone();
    drop(started(&d, &db).await.unwrap());
    assert_eq!((marks_017(&d), db.migration_report().unwrap()[crate::app::step_017::PART].clone()), (marks, report));
    std::fs::remove_dir_all(&d).ok();
}

/// R5: with the catalogue's ids lost, the step waits, as the first start does: nothing marked, no report, no reference.
#[tokio::test]
async fn the_step_waits_while_the_ids_are_lost() {
    let (d, db) = data_0_16("step-waits");
    db.set_meta(CATALOGUE_KEY, "another catalogue").unwrap();
    let lib = started(&d, &db).await.unwrap();
    assert!(lib.catalogue().ids_lost());
    assert!(marks_017(&d).is_empty());
    assert!(!db.migration_report().unwrap().contains_key(crate::app::step_017::PART));
    assert_eq!(rusqlite::Connection::open(d.join("catalogue.sqlite")).unwrap().query_row("SELECT count(*) FROM \"references\"", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    drop(lib);
    std::fs::remove_dir_all(&d).ok();
}

/// R2: 0.17's step failing stops the start, as 0.16's first start does, the parts before the one that failed done; once
/// what stood in its way is gone, the next start finishes it.
#[tokio::test]
async fn a_step_that_fails_stops_the_start() {
    let (d, db) = data_0_16("step-fails");
    let catalogue = || rusqlite::Connection::open(d.join("catalogue.sqlite")).unwrap();
    catalogue().execute_batch("CREATE TRIGGER no_pages BEFORE INSERT ON meta WHEN NEW.key = '017:pages' BEGIN SELECT RAISE(ABORT, 'no room for the pages'); END;").unwrap();
    let marks = || marks_017(&d);
    let err = started(&d, &db).await.err().expect("the start stops");
    assert!(format!("{err:#}").contains("0.17's step did not finish"), "{err:#}");
    assert_eq!(marks(), ["017:certain", "017:discography", "017:discogs", "017:files", "017:hand", "017:likely", "017:links"], "the parts before the pages done");
    assert!(!db.migration_report().unwrap().contains_key(crate::app::step_017::PART));

    catalogue().execute_batch("DROP TRIGGER no_pages").unwrap();
    drop(started(&d, &db).await.expect("it starts"));
    assert_eq!(marks().len(), crate::catalogue::records::PARTS_017.len() + 3, "and the band waves', the credits' and the fold's");
    assert!(db.migration_report().unwrap().contains_key(crate::app::step_017::PART));
    std::fs::remove_dir_all(&d).ok();
}
