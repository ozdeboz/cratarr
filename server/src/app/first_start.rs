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
    /// The paths of the libraries a server lists with no folder there now.
    pub unplaced: Vec<PathBuf>,
    pub rules: Rules,
}

impl FirstStart {
    /// Off the runtime: at a real library's size it takes a while.
    pub async fn run(self) -> Result<()> {
        tokio::task::spawn_blocking(move || self.steps(&|_| Ok(()))).await.context("it stopped")?
    }

    /// What a step found: kept from an earlier start, or found now and kept.
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
            return Ok(());
        }
        // No ids to map: moved now, every row would be written off as unmatched, and nothing could move it later. What names
        // them waits, as all else does, for the catalogue restored or its ids started afresh (`Library::fresh_track_ids`).
        if self.catalogue.ids_lost() {
            tracing::error!("the first start of 0.16 waits: the catalogue's track ids were lost");
            return Ok(());
        }
        // Q26: the safe side of "never dropped": a library not mounted is no library that lost its music.
        let away = self.catalogue.folders_not_there(&self.unplaced)?;
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
            for (file, sql) in [
                ("catalogue.sqlite", include_str!("../../tests/fixtures/0.15/catalogue.sql")),
                ("userdata.sqlite", include_str!("../../tests/fixtures/0.15/userdata.sql")),
                ("journal.sqlite", include_str!("../../tests/fixtures/0.15/journal.sql")),
                ("playlist-sync.sqlite", include_str!("../../tests/fixtures/0.15/playlist-sync.sql")),
            ] {
                let places = ["@music@", "@incoming@", "@second@"].into_iter().zip(&left.libraries);
                let sql = places.fold(sql.to_string(), |sql, (name, path)| sql.replace(name, &path.to_string_lossy()));
                Connection::open(left.data.join(file)).unwrap().execute_batch(&format!("BEGIN;\n{sql}\nCOMMIT;")).unwrap();
            }
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

        /// What `Library::new` hands it: every database opened anew, as a start does.
        fn start(&self) -> FirstStart {
            let catalogue = Catalogue::open(Some(&self.data.join("catalogue.sqlite")), self.libraries.to_vec()).unwrap();
            FirstStart {
                catalogue: Arc::new(catalogue),
                userdata: Arc::new(UserData::open(Some(&self.data.join("userdata.sqlite"))).unwrap()),
                data: Some(self.data.clone()),
                kinds: vec![(0, Kind::Main), (1, Kind::Incoming), (2, Kind::Incoming)],
                unplaced: Vec::new(),
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
                    let kept: Vec<usize> = (0..columns.len()).filter(|i| !(TIMES.contains(&columns[*i].as_str()) && ["records", "tracks", "artists", "migration_report", "files", "preference", "track_moved"].contains(&table.as_str()))).collect();
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
        format!("al-{}", left.sql::<i64>("catalogue.sqlite", "SELECT record FROM tracks WHERE id = 83"))
    }

    /// N4 on 0.15's data: the main library's tracks stand, whichever album a hash met first, and the report's numbers say so.
    #[test]
    fn the_main_librarys_tracks_stand_and_the_report_says_what_it_found() {
        let left = moved("report");
        let track_of = |folder: i64, rel: &str| left.sql::<i64>("catalogue.sqlite", &format!("SELECT track FROM files WHERE folder = {folder} AND rel = 'Salt Meridian/Both Shores (2018)/{rel}'"));
        assert_eq!((track_of(0, "01 Landing.flac"), track_of(1, "01 Landing.mp3"), track_of(2, "01 Landing.flac")), (48, 48, 48));
        assert_eq!((track_of(1, "03 Return.mp3"), track_of(2, "03 Return.flac")), (46, 46));
        let part = part(&left);
        assert_eq!((part["tracks_merged"].as_u64(), part["bases_on_two_records"].as_u64()), (Some(6), Some(3)), "{part}");
        assert_eq!(part["copies"].as_u64().unwrap(), part["records"].as_u64().unwrap() + 2, "Both Shores' three copies are one record");
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
        assert_eq!((rows["before"].as_u64(), rows["after"].as_u64(), rows["merged"].as_u64()), (Some(5), Some(4), Some(1)), "{rows}");
        assert_eq!((plays["before"].as_u64(), plays["after"].as_u64(), plays["moved"].as_u64()), (Some(5), Some(5), Some(1)));
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

        // A folder gone altogether: no server finds it, so `open` parks its rows, and only the library's path says it is missed.
        let left = Left::new("gone");
        let mut start = left.start();
        start.catalogue.set_folders(left.libraries[..2].to_vec()).unwrap();
        start.unplaced = vec![left.libraries[2].clone()];
        let refused = start.steps(&|_| Ok(())).unwrap_err().to_string();
        assert!(refused.contains(&*left.libraries[2].to_string_lossy()), "{refused}");
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
            unplaced: Vec::new(),
            rules: Rules::default(),
        };
        start.steps(&|_| Ok(())).unwrap();
        assert!(start.userdata.first_start_done().unwrap());
        drop(start);
        assert!(Path::new(&d).join("journal.sqlite").exists());
        std::fs::remove_dir_all(&d).ok();
    }
}
