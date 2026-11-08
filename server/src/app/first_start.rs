//! The first start of 0.16 (decision 0101 point 21, docs/records-and-copies.md, "The migration"): the ids that change, changed
//! once, before anything has read what names them, before the first build and before the first request. It runs in
//! `Library::new`, where the catalogue is open with its folders and no store has loaded: a store holds what it read and writes it
//! back, and a build first would serve new ids over old user data.
//!
//! No commit spans the databases, so every step is safe to go over again, and what says it is over is the `records` part of the
//! report, written last in the user data's one transaction. Until that part is there a start begins here, and a step that is done
//! says so by what it found. A failure stops the start: Cratarr does not run on half-moved data, and the way back is the copy
//! from before (decision 0101 point 24).

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::catalogue::{Catalogue, Rules};
use crate::journal::Journal;
use crate::moved::Report;
use crate::serve::copies::Kind;
use crate::serve::library::first_albums;
use crate::sync_state::SyncState;
use crate::userdata::{Moves, UserData};

pub(super) struct FirstStart {
    pub catalogue: Arc<Catalogue>,
    pub userdata: Arc<UserData>,
    /// The data folder. None: no journal, no sync and no stores to move.
    pub data: Option<PathBuf>,
    /// Each catalogue folder's library, in the libraries' order.
    pub kinds: Vec<(i64, Kind)>,
    pub rules: Rules,
}

impl FirstStart {
    /// Off the runtime: at a real library's size it takes a while.
    pub async fn run(self) -> Result<()> {
        tokio::task::spawn_blocking(move || self.steps(&|_| Ok(()))).await.context("it stopped")?
    }

    /// What a step found: kept from an earlier start, or found now and kept. A stop inside a step goes over it again, and what it
    /// had moved by then counts as none moved: the ids are right, and the count is short.
    fn step(&self, name: &str, work: impl FnOnce() -> Result<Report>) -> Result<Report> {
        if let Some(found) = self.userdata.step_found(name)? {
            return Ok(found);
        }
        let found = work().with_context(|| format!("moving {name}"))?;
        self.userdata.keep_step_found(name, &found)?;
        Ok(found)
    }

    /// `after` hears each step done: what a test stops it by.
    fn steps(&self, after: &dyn Fn(&str) -> Result<()>) -> Result<()> {
        if self.userdata.first_start_done()? {
            // The catalogue must be past it too: one from before it numbers its albums anew, and every row would name another. A
            // generation cannot say so, as a backup has the one it always had.
            anyhow::ensure!(
                self.catalogue.ids_lost() || self.catalogue.first_records_given()?,
                "the catalogue is from before the first start of 0.16, the user data from after it: restore a later catalogue, or all of the copy from before"
            );
            return Ok(());
        }
        // No ids to map: moved now, every row would be written off as unmatched, and nothing could move it later. What names
        // them waits, as all else does, for the catalogue restored or its ids started afresh (`Library::fresh_track_ids`).
        if self.catalogue.ids_lost() {
            tracing::error!("the first start of 0.16 waits: the catalogue's track ids were lost");
            return Ok(());
        }
        // Q26: the safe side of "never dropped": a library not mounted is no library that lost its music.
        let away = self.catalogue.folders_not_there()?;
        anyhow::ensure!(away.is_empty(), "every library must be there, and {} shows nothing: mount it and start again", away.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "));
        tracing::info!(target: "library", "the first start of 0.16: every album gets its record");
        let first = match self.catalogue.first_records_done()? {
            Some(first) => first,
            None => self.catalogue.first_records(first_albums(&self.catalogue, &self.rules)?, &self.kinds)?,
        };
        after("catalogue")?;
        let moved = self.catalogue.moved()?;
        let tracks = self.catalogue.moved_tracks()?;
        let mut report = Report::default();
        if let Some(data) = &self.data {
            report.extend(self.step("journal", || Journal::open(Some(&data.join("journal.sqlite")))?.move_changes(&moved))?);
            after("journal")?;
            report.extend(self.step("sync", || SyncState::open(Some(&data.join("playlist-sync.sqlite")))?.follow_tracks(&tracks))?);
            after("sync")?;
            report.extend(self.step("stores", || moved.stores(data))?);
            after("stores")?;
        }
        let (albums, artists) = (self.catalogue.albums_moved()?, self.catalogue.artists_moved()?);
        let pairs: Vec<(i64, i64)> = tracks.into_iter().collect();
        self.userdata.finish_move(&Moves { albums: &albums, artists: &artists, tracks: &pairs, stored: &moved }, report, &first)?;
        after("user data")?;
        // Left undone, the next build goes over them and finds nothing to move.
        self.catalogue.tracks_followed(&pairs)?;
        tracing::info!(target: "library", records = first.records, tracks_merged = first.tracks_merged, "the first start of 0.16 is done");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use rusqlite::Connection;
    use rusqlite::types::Value;
    use serde_json::{Value as Json, json};

    use super::*;

    const STEPS: [&str; 5] = ["catalogue", "journal", "sync", "stores", "user data"];
    const CATALOGUE: &str = include_str!("../../tests/fixtures/0.15/catalogue.sql");

    /// One row of the report's `tables`.
    fn table<'a>(part: &'a Json, name: &str) -> &'a Json {
        part["tables"].as_array().unwrap().iter().find(|t| t["name"] == name).unwrap_or_else(|| panic!("no {name} in {part}"))
    }

    /// A data folder as 0.15 left it (`tests/fixtures/0.15`), over three libraries that show a file each: nothing here reads
    /// them but to see they are there.
    struct Left {
        data: PathBuf,
        libraries: [PathBuf; 3],
    }

    impl Left {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("cratarr-first-start-{name}-{}", std::process::id()));
            std::fs::remove_dir_all(&root).ok();
            let (data, libraries) = (root.join("data"), ["music", "incoming", "second"].map(|l| root.join(l)));
            std::fs::create_dir_all(data.join("users/bob")).unwrap();
            for library in &libraries {
                std::fs::create_dir_all(library).unwrap();
                std::fs::write(library.join("a file"), b"x").unwrap();
            }
            let left = Self { data, libraries };
            left.write("catalogue.sqlite", CATALOGUE);
            left.write("userdata.sqlite", include_str!("../../tests/fixtures/0.15/userdata.sql"));
            left.write("journal.sqlite", include_str!("../../tests/fixtures/0.15/journal.sql"));
            left.write("playlist-sync.sqlite", include_str!("../../tests/fixtures/0.15/playlist-sync.sql"));
            for (file, text) in [
                ("crates.json", include_str!("../../tests/fixtures/0.15/crates.json")),
                ("users/bob/crates.json", include_str!("../../tests/fixtures/0.15/users/bob/crates.json")),
                ("queues.json", include_str!("../../tests/fixtures/0.15/queues.json")),
                ("convert-queue.json", include_str!("../../tests/fixtures/0.15/convert-queue.json")),
                ("track-intake.json", include_str!("../../tests/fixtures/0.15/track-intake.json")),
            ] {
                std::fs::write(left.data.join(file), text).unwrap();
            }
            left
        }

        /// A database as 0.15 left it, over these libraries.
        fn write(&self, file: &str, sql: &str) {
            let places = ["@music@", "@incoming@", "@second@"].into_iter().zip(&self.libraries);
            let sql = places.fold(sql.to_string(), |sql, (name, path)| sql.replace(name, &path.to_string_lossy()));
            Connection::open(self.data.join(file)).unwrap().execute_batch(&format!("BEGIN;\n{sql}\nCOMMIT;")).unwrap();
        }

        /// What `Library::new` hands it: every database opened anew, as a start does.
        fn start(&self) -> FirstStart {
            let catalogue = Catalogue::open(Some(&self.data.join("catalogue.sqlite")), self.libraries.to_vec()).unwrap();
            FirstStart {
                catalogue: Arc::new(catalogue),
                userdata: Arc::new(UserData::open(Some(&self.data.join("userdata.sqlite"))).unwrap()),
                data: Some(self.data.clone()),
                kinds: vec![(0, Kind::Main), (1, Kind::Incoming), (2, Kind::Incoming)],
                rules: Rules::default(),
            }
        }

        fn read(&self, file: &str) -> Json {
            serde_json::from_slice(&std::fs::read(self.data.join(file)).unwrap()).unwrap()
        }

        fn sql<T: rusqlite::types::FromSql>(&self, file: &str, sql: &str) -> T {
            Connection::open(self.data.join(file)).unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
        }

        /// Everything a first start leaves, row by row and store by store, but for when it ran.
        fn left(&self) -> BTreeMap<String, Vec<String>> {
            const TIMES: [&str; 6] = ["at", "born", "retired_at", "came", "changed", "followed"];
            let mut out = BTreeMap::new();
            for file in ["catalogue.sqlite", "userdata.sqlite", "journal.sqlite", "playlist-sync.sqlite"] {
                let db = Connection::open(self.data.join(file)).unwrap();
                let tables: Vec<String> = db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
                for table in tables {
                    let columns: Vec<String> = db.prepare(&format!("SELECT name FROM pragma_table_info('{table}')")).unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
                    let kept: Vec<usize> = (0..columns.len()).filter(|i| !(TIMES.contains(&columns[*i].as_str()) && ["records", "tracks", "artists", "migration_report", "files", "preference", "track_moved", "change_log"].contains(&table.as_str()))).collect();
                    let mut st = db.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
                    let root = self.data.parent().unwrap().to_string_lossy().to_string();
                    let mut rows: Vec<String> = st.query_map([], |r| Ok(kept.iter().map(|i| format!("{:?}", r.get::<_, Value>(*i).unwrap()).replace(&root, "")).collect::<Vec<_>>().join("|"))).unwrap().map(Result::unwrap).collect();
                    rows.sort();
                    out.insert(format!("{file}: {table}"), rows);
                }
            }
            for store in ["crates.json", "users/bob/crates.json", "queues.json", "convert-queue.json", "track-intake.json"] {
                out.insert(store.to_string(), vec![self.read(store).to_string()]);
            }
            out
        }

        fn gone(self) {
            if let Some(root) = self.data.parent() {
                std::fs::remove_dir_all(root).ok();
            }
        }
    }

    fn part(left: &Left) -> Json {
        serde_json::from_str(&left.sql::<String>("userdata.sqlite", "SELECT report FROM migration_report WHERE part = 'records'")).unwrap()
    }

    /// 0.15's data after a first start, a test's own. A preference is new in 0.16, so 0.15 left none, but a copy that ran a preview
    /// can hold one.
    fn moved(name: &str) -> Left {
        let left = Left::new(name);
        let start = left.start();
        let filter = json!({ "albums": { "conditions": [{ "axis": "album", "not": false, "value": { "kind": "any_of", "values": ["al-4FwVmWLNdYpZZSEtkan6Uy"] } }] } });
        start.userdata.set_preference("ann", "filters", &filter, 1).unwrap();
        start.steps(&|_| Ok(())).unwrap();
        drop(start);
        left
    }

    /// The record Both Shores' tracks hold, as it is served, and Tide Tables'.
    fn shores(left: &Left) -> String {
        format!("al-{}", left.sql::<i64>("catalogue.sqlite", "SELECT record FROM tracks WHERE id = 48"))
    }

    fn tide(left: &Left) -> String {
        on(left, 83)
    }

    /// The record a track holds, as it is served.
    fn on(left: &Left, track: i64) -> String {
        format!("al-{}", left.sql::<i64>("catalogue.sqlite", &format!("SELECT record FROM tracks WHERE id = {track}")))
    }

    /// One of an account's numbers on a record.
    fn held(left: &Left, user: &str, what: &str, record: &str) -> i64 {
        left.sql("userdata.sqlite", &format!("SELECT {what} FROM annotation WHERE user = '{user}' AND item = '{record}'"))
    }

    /// N4 on 0.15's data: the main library's tracks stand, whichever album a hash met first, and the report's numbers say so.
    #[test]
    fn the_main_librarys_tracks_stand_and_the_report_says_what_it_found() {
        let left = moved("report");
        let track_of = |folder: i64, rel: &str| left.sql::<i64>("catalogue.sqlite", &format!("SELECT track FROM files WHERE folder = {folder} AND rel = 'Salt Meridian/Both Shores (2018)/{rel}'"));
        assert_eq!((track_of(0, "01 Landing.flac"), track_of(1, "01 Landing.mp3"), track_of(2, "01 Landing.flac")), (48, 48, 48));
        assert_eq!((track_of(1, "03 Return.mp3"), track_of(2, "03 Return.flac")), (46, 46));
        let part = part(&left);
        assert_eq!((part["tracks_merged"].as_u64(), part["bases_on_two_records"].as_u64()), (Some(9), Some(2)), "{part}");
        assert_eq!(part["copies"].as_u64().unwrap(), part["records"].as_u64().unwrap() + 3, "Both Shores' three copies are one record, and Short Measure's two");
        assert_eq!(part["unmatched"].as_array().unwrap().iter().map(|u| u["count"].as_u64().unwrap()).sum::<u64>(), 0, "{part}");
        assert_eq!(left.sql::<i64>("userdata.sqlite", "SELECT count(*) FROM meta WHERE key LIKE 'first_start:%'"), 0, "what the steps found has gone into the report");
        left.gone();
    }

    #[test]
    fn a_crate_keeps_its_seeds_and_a_line_is_its_track() {
        let left = moved("crates");
        let (crates, record) = (left.read("crates.json"), shores(&left));
        assert_eq!(crates["crates"][0]["conditions"][0]["value"]["of"], json!([record]), "two ids of one record are one seed");
        assert_eq!(crates["left_out"]["c-shores"], json!(["tr-82"]));
        assert_eq!(crates["excluded"], json!(["tr-48"]));
        assert!(crates["fits"].get(format!("like:{record}")).is_some(), "{crates}");
        let part = part(&left);
        assert_eq!(table(&part, "crates.json")["before"], 3);
        left.gone();
    }

    #[test]
    fn a_queue_keeps_its_tracks_its_album_and_what_it_left_behind() {
        let left = moved("queues");
        let (queues, record) = (left.read("queues.json"), shores(&left));
        let player = &queues["a player"];
        assert_eq!((&player["tracks"][0]["id"], &player["tracks"][0]["album_id"]), (&json!("tr-48"), &json!(record)));
        assert_eq!(player["autoplay"]["seed"], json!(["tr-48"]));
        assert_eq!(player["kept"][0]["tracks"][0]["id"], "tr-48", "a queue left behind too");
        assert_eq!(player["kept"][0]["autoplay"]["within"]["id"], format!("like:{record}@50"));
        assert!(table(&part(&left), "queues.json")["moved"].as_u64() > Some(0));
        left.gone();
    }

    /// A Keep waiting for its conversion and its review go by one id, or Undo no longer finds the job.
    #[test]
    fn a_job_and_its_review_stay_one_and_a_waiting_track_keeps_its_album() {
        let left = moved("jobs");
        let job = left.read("convert-queue.json");
        let arrived = job["list"][0]["album"]["id"].as_str().unwrap();
        assert!(crate::serve::ids::for_life(arrived), "{job}");
        assert_eq!(left.sql::<String>("userdata.sqlite", "SELECT undo FROM album_review WHERE state = 'rejected'"), format!("job:{arrived}"));
        let intake = left.read("track-intake.json");
        assert!(crate::serve::ids::for_life(intake["items"][0]["album"]["id"].as_str().unwrap()), "{intake}");
        left.gone();
    }

    /// Undo holds a store against what the change left, so a change must say what the store says now.
    #[test]
    fn a_change_from_before_says_what_the_stores_say() {
        let left = moved("journal");
        let change = |id: i64| serde_json::from_str::<Json>(&left.sql::<String>("journal.sqlite", &format!("SELECT change FROM changes WHERE id = {id}"))).unwrap();
        let journal = Journal::open(Some(&left.data.join("journal.sqlite"))).unwrap();
        assert!(matches!(journal.change(1).unwrap().unwrap().0, crate::journal::Change::LeftOut { .. }), "read as Undo reads it");
        assert_eq!(change(1)["after"]["tracks"], json!(["tr-82"]));
        assert_eq!(change(2)["after"], json!(["tr-48"]));
        assert_eq!(table(&part(&left), "journal: changes")["moved"], 4);
        left.gone();
    }

    #[test]
    fn an_agreed_list_keeps_its_tracks_in_their_places() {
        let left = moved("sync");
        assert_eq!(left.sql::<String>("playlist-sync.sqlite", "SELECT base FROM links"), "[83,48,87]", "the retired one too");
        assert_eq!(table(&part(&left), "playlist-sync: links.base")["moved"], 1);
        left.gone();
    }

    /// The merge's plays, hearts and ratings: added up, not lost and not counted twice, each play still its own.
    #[test]
    fn what_two_copies_tracks_held_adds_up_on_the_track_that_stands() {
        let left = moved("tracks");
        assert_eq!(left.sql::<i64>("userdata.sqlite", "SELECT count(*) FROM plays WHERE song = 'tr-48'"), 2);
        assert_eq!(left.sql::<i64>("userdata.sqlite", "SELECT play_count FROM annotation WHERE item = 'tr-48' AND user = 'ann'"), 3);
        assert_eq!(left.sql::<i64>("userdata.sqlite", "SELECT rating FROM annotation WHERE item = 'tr-48' AND user = 'ann'"), 5);
        let part = part(&left);
        let (rows, plays) = (table(&part, "annotation: tracks"), table(&part, "plays"));
        assert_eq!((rows["before"].as_u64(), rows["after"].as_u64(), rows["merged"].as_u64()), (Some(6), Some(5), Some(1)), "{rows}");
        assert_eq!((plays["before"].as_u64(), plays["after"].as_u64(), plays["moved"].as_u64()), (Some(6), Some(6), Some(2)));
        left.gone();
    }

    /// An album's hearts on its two copies are the record's, and what went by its base is the record's too.
    #[test]
    fn the_tables_move_to_the_record_by_both_forms() {
        let left = moved("albums");
        let (both, tide) = (shores(&left), tide(&left));
        let plays: i64 = left.sql("userdata.sqlite", &format!("SELECT play_count FROM annotation WHERE item = '{both}' AND user = 'ann'"));
        assert_eq!(plays, 3, "Music's and Incoming's");
        assert_eq!(left.sql::<i64>("userdata.sqlite", &format!("SELECT count(*) FROM album_tags WHERE album = '{tide}'")), 1, "by its base");
        assert_eq!(left.sql::<i64>("userdata.sqlite", &format!("SELECT count(*) FROM added WHERE album = '{tide}'")), 1, "by its id");
        let filters = left.sql::<String>("userdata.sqlite", "SELECT value FROM preference WHERE key = 'filters'");
        assert_eq!(serde_json::from_str::<Json>(&filters).unwrap()["albums"]["conditions"][0]["value"]["values"], json!([tide]));
        left.gone();
    }

    /// An artist's heart, by the map from their name's hash.
    #[test]
    fn an_artists_heart_is_on_their_id() {
        let left = moved("artists");
        let id: i64 = left.sql("catalogue.sqlite", "SELECT id FROM artists WHERE name = 'Glass Harbour'");
        assert_eq!(left.sql::<i64>("userdata.sqlite", &format!("SELECT starred FROM annotation WHERE item = 'ar-{id}' AND user = 'ann'")), 1);
        left.gone();
    }

    /// Between any two steps, and in the last: a restart finishes it, and what it leaves is what a start that never stopped
    /// leaves, its report too. The report's `at` aside, row for row.
    #[test]
    fn a_crash_between_any_two_steps_finishes_on_restart() {
        let straight = Left::new("straight");
        straight.start().steps(&|_| Ok(())).unwrap();
        let expected = straight.left();
        straight.gone();
        for stop in STEPS {
            let left = Left::new(&format!("stop-{}", stop.replace(' ', "-")));
            let start = left.start();
            let stopped = start.steps(&|step| if step == stop { anyhow::bail!("stopped") } else { Ok(()) });
            assert!(stopped.is_err(), "{stop}");
            let over = start.userdata.first_start_done().unwrap();
            assert_eq!(over, stop == "user data", "over only once its last step has committed: {stop}");
            drop(start);
            left.start().steps(&|_| Ok(())).unwrap();
            let after = left.left();
            for (place, rows) in &expected {
                assert_eq!(after.get(place), Some(rows), "{place}, stopped after {stop}");
            }
            assert_eq!(after.len(), expected.len());
            left.gone();
        }
    }

    /// A stop inside a step: part of the stores moved, the last unreadable, the report written by nothing. The last step's
    /// transaction falls whole.
    #[test]
    fn a_stop_inside_a_step_goes_over_it_again() {
        let left = Left::new("inside");
        let intake = left.data.join("track-intake.json");
        let was = std::fs::read(&intake).unwrap();
        std::fs::write(&intake, b"not json").unwrap();
        let start = left.start();
        assert!(start.steps(&|_| Ok(())).is_err(), "a store that cannot be read stops the move");
        assert!(!start.userdata.first_start_done().unwrap());
        drop(start);
        assert!(left.read("crates.json").to_string().contains("tr-82"), "the stores before it are moved");
        std::fs::write(&intake, was).unwrap();
        left.start().steps(&|_| Ok(())).unwrap();
        assert!(left.read("track-intake.json")["items"][0]["album"]["id"].as_str().is_some_and(crate::serve::ids::for_life));

        let left = Left::new("last");
        Connection::open(left.data.join("userdata.sqlite")).unwrap().execute_batch("CREATE TABLE migration_report (part TEXT PRIMARY KEY, at INTEGER NOT NULL, report TEXT NOT NULL) WITHOUT ROWID; CREATE TRIGGER no_report BEFORE INSERT ON migration_report BEGIN SELECT RAISE(ABORT, 'no'); END;").unwrap();
        let before: Vec<String> = Connection::open(left.data.join("userdata.sqlite")).unwrap().prepare("SELECT item FROM annotation ORDER BY item").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        let start = left.start();
        assert!(start.steps(&|_| Ok(())).is_err());
        assert!(!start.userdata.first_start_done().unwrap());
        drop(start);
        let after: Vec<String> = Connection::open(left.data.join("userdata.sqlite")).unwrap().prepare("SELECT item FROM annotation ORDER BY item").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert_eq!(after, before, "the report goes in last, so a failure there leaves nothing moved");
        left.gone();
    }

    /// Done is done: a start after it changes nothing, whatever it could find.
    #[test]
    fn a_restart_changes_nothing() {
        let left = Left::new("restart");
        left.start().steps(&|_| Ok(())).unwrap();
        let (rows, at) = (left.left(), left.sql::<i64>("userdata.sqlite", "SELECT at FROM migration_report WHERE part = 'records'"));
        Connection::open(left.data.join("userdata.sqlite")).unwrap().execute("INSERT INTO annotation (user, item, kind, rating) VALUES ('ann', 'al-5vcUvYmuWJAgyxLXEUmaPz', 'album', 1)", []).unwrap();
        let heard = std::sync::Mutex::new(Vec::new());
        left.start().steps(&|step| Ok(heard.lock().unwrap().push(step.to_string()))).unwrap();
        assert!(heard.lock().unwrap().is_empty(), "no step ran");
        Connection::open(left.data.join("userdata.sqlite")).unwrap().execute("DELETE FROM annotation WHERE item = 'al-5vcUvYmuWJAgyxLXEUmaPz'", []).unwrap();
        assert_eq!(left.left(), rows);
        assert_eq!(left.sql::<i64>("userdata.sqlite", "SELECT at FROM migration_report WHERE part = 'records'"), at);
        left.gone();
    }

    /// Q26: a library not there stops the start, says which, and nothing has moved: a catalogue step run now would still have
    /// its rows, but the owner's word is to refuse.
    #[test]
    fn a_library_not_mounted_stops_the_start_and_says_which() {
        let left = Left::new("mount");
        std::fs::remove_file(left.libraries[1].join("a file")).unwrap();
        let start = left.start();
        let refused = start.steps(&|_| Ok(())).unwrap_err().to_string();
        assert!(refused.contains(&*left.libraries[1].to_string_lossy()), "{refused}");
        assert_eq!(start.catalogue.first_records_done().unwrap(), None);
        drop(start);
        std::fs::write(left.libraries[1].join("a file"), b"x").unwrap();
        left.start().steps(&|_| Ok(())).unwrap();
        left.gone();

        // A folder gone altogether: no server finds it, so `open` parks its rows, and its id for life says it is missed.
        let left = Left::new("gone");
        let start = left.start();
        start.catalogue.set_folders(left.libraries[..2].to_vec()).unwrap();
        let refused = start.steps(&|_| Ok(())).unwrap_err().to_string();
        assert!(refused.contains(&*left.libraries[2].to_string_lossy()), "{refused}");
        left.gone();
    }

    /// M2: a restore of less than everything. A backup of the catalogue from before the first start has the generation it always
    /// had, so the user data takes it for its own, and its albums would come to other numbers than every row names: `al-5` another
    /// album, with no error and no report.
    #[test]
    fn a_catalogue_from_before_the_first_start_stops_the_start() {
        let left = moved("restored");
        for file in ["catalogue.sqlite", "catalogue.sqlite-wal", "catalogue.sqlite-shm"] {
            std::fs::remove_file(left.data.join(file)).ok();
        }
        left.write("catalogue.sqlite", CATALOGUE);
        let start = left.start();
        let rows = left.left();
        let refused = start.steps(&|_| Ok(())).unwrap_err().to_string();
        assert!(refused.contains("before the first start"), "{refused}");
        drop(start);
        assert_eq!(left.left(), rows, "nothing written");
        left.gone();
    }

    /// S1: 0.15 served a file that failed its last read no more, and kept its format. A new reader makes it a row waiting, and a row
    /// waiting with a format is served, so it came here with no tags, four of six enough to take the album's old id with them.
    #[test]
    fn a_file_that_failed_is_no_album_at_the_first_start() {
        let left = Left::new("failed");
        let failed = "rel LIKE 'Glass Harbour/Tide Tables%' AND rel NOT LIKE '%/01 %' AND rel NOT LIKE '%/02 %'";
        let db = Connection::open(left.data.join("catalogue.sqlite")).unwrap();
        db.execute_batch(&format!("UPDATE files SET state = 2 WHERE {failed}; DELETE FROM tags WHERE {failed};")).unwrap();
        drop(db);
        let start = left.start();
        start.steps(&|_| Ok(())).unwrap();
        assert_eq!(start.catalogue.named_now("al-4FwVmWLNdYpZZSEtkan6Uy"), Some(tide(&left)), "the id it was served by");
        drop(start);
        left.gone();
    }

    /// S3: a play written from a song's last play is safe to repeat by its event only while the song keeps its id, and a merge moves
    /// the song and leaves the event. Told over at the next open, a play already there counts twice.
    #[test]
    fn a_restart_after_a_merge_adds_no_play() {
        let left = moved("week");
        let plays = |left: &Left| left.sql::<i64>("userdata.sqlite", "SELECT count(*) FROM plays");
        let before = plays(&left);
        assert_eq!(left.sql::<i64>("userdata.sqlite", "SELECT count(*) FROM plays WHERE song = 'tr-47' AND context = 'backfill:annotation'"), 1, "moved, with its event");
        drop(UserData::open(Some(&left.data.join("userdata.sqlite"))).unwrap());
        assert_eq!(plays(&left), before);
        left.gone();
    }

    /// A library that changed place: what came before went by the place it had and what came after by the place it has, a row of
    /// each for one album, on the second day as on the first. They are one row, by each table's own rule. Either way round, as a
    /// library can go back: Far Bank lies at the second place, under the first as well, and Just Arrived the other way about.
    #[test]
    fn an_albums_two_ids_are_one_row_with_what_each_held() {
        let left = moved("two-ids");
        for (who, plays, record) in [("ann", 4, on(&left, 5)), ("bob", 3, on(&left, 16))] {
            let count = |table: &str, column: &str| left.sql::<i64>("userdata.sqlite", &format!("SELECT count(*) FROM {table} WHERE {column} = '{record}'"));
            assert_eq!(left.sql::<i64>("userdata.sqlite", &format!("SELECT count(*) FROM annotation WHERE user = '{who}' AND item = '{record}'")), 1);
            assert_eq!((held(&left, who, "play_count", &record), held(&left, who, "starred", &record), held(&left, who, "starred_at", &record)), (plays, 1, 1760880000), "the plays of one and the heart of the other");
            assert_eq!(count("added", "album"), 1);
            assert_eq!(left.sql::<String>("userdata.sqlite", &format!("SELECT at || ' ' || origin FROM added WHERE album = '{record}'")), "1760880000 jf", "the earlier, whole");
            assert_eq!(count("album_complete", "album"), 1);
            assert_eq!(left.sql::<Option<String>>("userdata.sqlite", &format!("SELECT found FROM album_complete WHERE album = '{record}'")), None, "the look under the id it has now, the later");
        }
        let differed = part(&left)["differed"].as_array().unwrap().iter().find(|d| d["what"] == "album_complete").cloned().unwrap();
        assert_eq!(differed["count"].as_u64(), Some(2), "and what went with the other is there to read: {differed}");
        // The same on a release both Incoming libraries hold: the first id real, the place the second had.
        let both = shores(&left);
        assert_eq!((held(&left, "bob", "play_count", &both), held(&left, "bob", "starred", &both)), (2, 1));
        left.gone();
    }

    /// One place, two libraries, on a real first start: a release in both, on two records. What lay under the id of the place stays
    /// with the album there now, a row being one row, and the other copy keeps what came to it under the place it has.
    #[test]
    fn what_a_place_held_stays_on_the_album_there_now() {
        let left = Left::new("one-place");
        let music = "folder = 0 AND rel LIKE 'Salt Meridian/Bonus Round%'";
        let db = Connection::open(left.data.join("catalogue.sqlite")).unwrap();
        db.execute_batch(&format!("UPDATE files SET folder = 1 WHERE {music}; UPDATE tags SET folder = 1 WHERE {music};")).unwrap();
        drop(db);
        let user = Connection::open(left.data.join("userdata.sqlite")).unwrap();
        user.execute("INSERT INTO annotation (user, item, kind, play_count, rating, starred, own_count) VALUES ('ann', 'al-4NM6uQYT2XxMI7vrJ6L3oK', 'album', 9, 0, 0, 9)", []).unwrap();
        drop(user);
        left.start().steps(&|_| Ok(())).unwrap();
        let (there_now, moved_along) = (on(&left, 51), on(&left, 12));
        assert_ne!(there_now, moved_along, "two records");
        assert_eq!((held(&left, "ann", "play_count", &there_now), held(&left, "ann", "play_count", &moved_along)), (6, 9));
        left.gone();
    }

    /// Rows under an id no album has now, each through a real first start: a Keep's, under the id its album had in Incoming; one
    /// under what Music called an album that waits in Incoming now; and, of a release on two records, one under a place in
    /// Incoming, on the copy that lay there and not on Music's.
    #[test]
    fn rows_under_an_id_no_album_has_reach_their_record() {
        let left = moved("worked-out");
        assert_eq!(held(&left, "ann", "play_count", &on(&left, 63)), 3, "the Keep's");
        assert_eq!(held(&left, "ann", "play_count", &on(&left, 16)), 5, "what Music called it");
        assert_eq!(held(&left, "ann", "play_count", &on(&left, 12)), 6, "the copy in Incoming");
        assert_eq!(left.sql::<i64>("userdata.sqlite", &format!("SELECT count(*) FROM annotation WHERE item = '{}'", on(&left, 51))), 0, "not Music's");
        left.gone();
    }

    /// m1: a review and a hearing from before a library changed place read on the copy where it lies, or a card heard reads unheard.
    #[test]
    fn what_went_by_the_place_a_library_had_reads_on_the_copy_where_it_lies() {
        let left = moved("place");
        let bank = on(&left, 5);
        let user = UserData::open(Some(&left.data.join("userdata.sqlite"))).unwrap();
        assert_eq!(user.review(&bank, Some(2)).unwrap().map(|r| r.state).as_deref(), Some("rejected"));
        assert!(user.intake_heard(&bank, Some(2)).unwrap().is_some());
        drop(user);
        left.gone();
    }

    /// m8 on 0.15's data: a file's shape and beats lay at the place its library had, and read as nothing there. Either way round:
    /// Far Bank's at the first place for the second, Just Arrived's at the second for the first.
    #[test]
    fn a_files_shape_and_beats_follow_the_place_its_library_moved_to() {
        let left = moved("shapes");
        let folder = |table: &str, file: &str| left.sql::<i64>("catalogue.sqlite", &format!("SELECT folder FROM {table} WHERE rel LIKE '%/{file}'"));
        for table in ["analysis_shape", "beat_this"] {
            assert_eq!((folder(table, "01 Far Bank.flac"), folder(table, "01 Just Arrived.mp3")), (2, 1), "{table}");
        }
        let part = part(&left);
        assert_eq!((part["shapes_moved"].as_u64(), part["beats_moved"].as_u64()), (Some(2), Some(2)), "{part}");
        left.gone();
    }

    /// CR13 through the user data's step: what named a file gone a while, one in a trash and one in a folder parked by 0.15 is on
    /// its record, as the album's return would need it to be.
    #[test]
    fn what_no_list_shows_keeps_what_named_it() {
        let left = moved("unserved");
        assert_eq!(held(&left, "ann", "play_count", &on(&left, 88)), 2, "gone a while");
        assert_eq!(held(&left, "ann", "play_count", &on(&left, 89)), 1, "in a trash");
        assert_eq!(left.sql::<i64>("userdata.sqlite", &format!("SELECT count(*) FROM album_tags WHERE album = '{}'", on(&left, 89))), 1, "by its base");
        assert_eq!(held(&left, "ann", "play_count", &on(&left, 90)), 7, "parked");
        assert_eq!(part(&left)["unmatched"].as_array().unwrap().iter().map(|u| u["count"].as_u64().unwrap()).sum::<u64>(), 0);
        left.gone();
    }

    /// A device told of a heart hears it went from the track that left, and came to the one that stands.
    #[test]
    fn a_device_told_of_a_heart_hears_it_went_to_the_track_that_took_it() {
        let left = moved("told");
        let said = |track: &str| left.sql::<String>("userdata.sqlite", &format!("SELECT op FROM change_log WHERE item = 'track:cratarr/{track}'"));
        assert_eq!((said("tr-19"), said("tr-48")), ("deleted".to_string(), "changed".to_string()));
        left.gone();
    }

    /// By the map the first start made, not one a test makes from what it then looks for: no table of the user data, no change in
    /// the journal and no store keeps an old id that had somewhere to go.
    #[test]
    fn nothing_with_somewhere_to_go_stays_under_an_old_id() {
        let left = moved("scan");
        let old = |text: &str| text.split(|c: char| !c.is_ascii_alphanumeric() && c != '-').any(|word| word.len() == 25 && (word.starts_with("al-") || word.starts_with("ar-")));
        for (place, rows) in left.left() {
            if !place.starts_with("catalogue") && !place.starts_with("playlist-sync") && place != "userdata.sqlite: migration_report" {
                assert_eq!(rows.iter().find(|row| old(row)), None, "{place}");
            }
        }
        left.gone();
    }

    /// A catalogue whose ids were lost has nothing to map: it waits, the user data as it was, and nothing says it is over.
    #[test]
    fn lost_ids_stop_the_first_start_with_nothing_moved() {
        let left = Left::new("lost");
        let start = left.start();
        start.catalogue.set_ids_lost(true);
        start.steps(&|_| Ok(())).unwrap();
        assert!(!start.userdata.first_start_done().unwrap());
        assert_eq!(start.catalogue.first_records_done().unwrap(), None);
        drop(start);
        let crates = left.read("crates.json").to_string();
        assert!(crates.contains("al-4FwVmWLNdYpZZSEtkan6Uy"), "{crates}");
        left.gone();
    }

    /// A new data folder has nothing to move, and is over at once, or every start would go over it for nothing.
    #[test]
    fn a_new_data_folder_is_done_at_once() {
        let d = std::env::temp_dir().join(format!("cratarr-first-start-new-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        let start = FirstStart {
            catalogue: Arc::new(Catalogue::open(Some(&d.join("catalogue.sqlite")), vec![d.clone()]).unwrap()),
            userdata: Arc::new(UserData::open(Some(&d.join("userdata.sqlite"))).unwrap()),
            data: Some(d.clone()),
            kinds: vec![(0, Kind::Main)],
            rules: Rules::default(),
        };
        start.steps(&|_| Ok(())).unwrap();
        assert!(start.userdata.first_start_done().unwrap());
        drop(start);
        assert!(Path::new(&d).join("journal.sqlite").exists());
        std::fs::remove_dir_all(&d).ok();
    }
}
