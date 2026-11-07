//! The person's preferences on their account (decision 0101 point 14): pinned items, libraries switched off, filters,
//! Home's header picture, crate drafts and Radio's last choice, followed from device to device. Per preference the
//! latest change wins, the whole value; offline a device works from its cached copy. Only the libraries switched off
//! mean anything to the server (which copy plays); the others are the page's, kept and handed back.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use rusqlite::{OptionalExtension, params};

use super::UserData;

pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS preference (
  account TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, changed INTEGER NOT NULL,
  PRIMARY KEY (account, key)) WITHOUT ROWID;
";

/// The libraries switched off, as `source/library` (the page's `without` keys).
pub const HIDDEN_LIBRARIES: &str = "hidden_libraries";

/// The preferences an account keeps; any other key is refused.
pub const KEYS: [&str; 6] = [HIDDEN_LIBRARIES, "pinned", "filters", "home_picture", "crate_drafts", "radio_last"];

/// The largest value kept, as JSON (a crate draft is the largest, far below it).
pub const MOST_BYTES: usize = 256 * 1024;

/// A preference as kept: its value, and when it was changed (epoch ms).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Preference {
    pub value: serde_json::Value,
    pub changed: i64,
}

/// A change of a preference that was kept, for the account's open pages (app/events.rs: `changed`, area
/// `preferences`).
#[derive(Debug, Clone, PartialEq)]
pub struct PreferenceChanged {
    pub account: String,
    pub key: String,
}

/// Why a preference is refused before it is looked at: an unknown key, a value of the wrong shape, too big.
pub fn refusal(key: &str, value: &serde_json::Value) -> Option<String> {
    if !KEYS.contains(&key) {
        return Some(format!("{key} is not a preference"));
    }
    if key == HIDDEN_LIBRARIES && !value.as_array().is_some_and(|a| a.iter().all(serde_json::Value::is_string)) {
        return Some("The libraries switched off are a list of names".into());
    }
    (value.to_string().len() > MOST_BYTES).then(|| format!("A preference holds at most {} KB", MOST_BYTES / 1024))
}

/// A library's switch as it reads once `moved`, (was, is) paths, are followed. A key is `source/library`, and the id of one of
/// Cratarr's own libraries is its path, where another server's is that server's own and never moves with a folder.
fn key_moved(key: &str, moved: &[(String, String)]) -> Option<String> {
    let (source, library) = key.split_once('/')?;
    moved.iter().find(|(was, _)| was == library).map(|(_, is)| format!("{source}/{is}"))
}

/// Each account's filters moved (`Moved::filters`), in the caller's transaction. One that moved is changed `now` (epoch ms): the
/// latest change wins, so a device's copy from before must not win back the old ids. One that did not keep its time.
pub(crate) fn move_filters(conn: &rusqlite::Connection, moved: &crate::moved::Moved, now: i64) -> Result<crate::moved::Report> {
    let rows: Vec<(String, String)> = conn.prepare("SELECT account, value FROM preference WHERE key = 'filters'")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    let (n, mut walk) = (rows.len() as u64, crate::moved::Walk::default());
    for (account, text) in rows {
        let Ok(mut filters) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        let was = filters.clone();
        moved.filters(&mut filters, &mut walk);
        if filters != was {
            conn.execute("UPDATE preference SET value = ?2, changed = ?3 WHERE account = ?1 AND key = 'filters'", params![account, filters.to_string(), now])?;
        }
    }
    let mut report = crate::moved::Report::default();
    report.walked("preference: filters", n, walk);
    Ok(report)
}

impl UserData {
    /// Every preference `account` keeps, by key. Blocking.
    pub fn preferences(&self, account: &str) -> Result<BTreeMap<String, Preference>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT key, value, changed FROM preference WHERE account = ?1")?;
        let rows = st.query_map([account], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (key, value, changed) = row?;
            out.insert(key, Preference { value: serde_json::from_str(&value).unwrap_or(serde_json::Value::Null), changed });
        }
        Ok(out)
    }

    /// Keep `value` as `account`'s `key` when `changed` (epoch ms) is later than the kept change: the latest change wins,
    /// a tie keeps what is kept. A change dated ahead of the server's clock counts as now, as every change a device dates
    /// (`ctx::ChangeAt`), so a device whose clock runs ahead cannot outlast later changes. Answers what is kept afterwards;
    /// a kept change is told to the account's open pages. Blocking.
    pub fn set_preference(&self, account: &str, key: &str, value: &serde_json::Value, changed: i64) -> Result<Preference> {
        let changed = changed.min(super::now_ms());
        let db = self.db();
        let kept = db.execute(
            "INSERT INTO preference (account, key, value, changed) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (account, key) DO UPDATE SET value = excluded.value, changed = excluded.changed \
             WHERE excluded.changed > preference.changed",
            params![account, key, value.to_string(), changed],
        )? > 0;
        let (value, changed): (String, i64) =
            db.query_row("SELECT value, changed FROM preference WHERE account = ?1 AND key = ?2", params![account, key], |r| Ok((r.get(0)?, r.get(1)?)))?;
        drop(db);
        if kept {
            if key == HIDDEN_LIBRARIES {
                self.hidden.lock().unwrap_or_else(|p| p.into_inner()).remove(account);
            }
            let _ = self.preferences.send(PreferenceChanged { account: account.to_string(), key: key.to_string() });
        }
        Ok(Preference { value: serde_json::from_str(&value).unwrap_or(serde_json::Value::Null), changed })
    }

    /// `move_filters`, in a transaction of its own. Blocking.
    pub fn move_filters(&self, moved: &crate::moved::Moved) -> Result<crate::moved::Report> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let report = move_filters(&tx, moved, super::now_ms())?;
        tx.commit()?;
        Ok(report)
    }

    /// The libraries `account` has switched off, as `source/library`. Blocking.
    pub fn hidden_libraries(&self, account: &str) -> Result<Vec<String>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT value FROM preference WHERE account = ?1 AND key = ?2")?;
        let value: Option<String> = st.query_row(params![account, HIDDEN_LIBRARIES], |r| r.get(0)).optional()?;
        Ok(value.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
    }

    /// `hidden_libraries` as every stream asks for it: read once and kept until the account changes it, where the change
    /// is told (`set_preference`). The lock held over the read, so a change landing meanwhile cannot leave the old list.
    /// One that cannot be read counts as nothing off, and is asked again.
    pub fn hidden_libraries_kept(&self, account: &str) -> Arc<Vec<String>> {
        let mut kept = self.hidden.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(list) = kept.get(account) {
            return list.clone();
        }
        match self.hidden_libraries(account) {
            Ok(list) => kept.entry(account.to_string()).or_insert(Arc::new(list)).clone(),
            Err(e) => {
                tracing::warn!(account, reason = %format!("{e:#}"), "the libraries switched off not read");
                Arc::default()
            }
        }
    }

    /// `library` (`source/library`) switched off for each of `accounts`, beside what they have off: a library that starts
    /// off for them (0101 point 6: someone's own music, at its making). Only then: one switched on again stays on.
    /// Blocking.
    pub fn switch_library_off(&self, accounts: &[String], library: &str) -> Result<()> {
        for account in accounts {
            let mut off = self.hidden_libraries(account)?;
            if !off.iter().any(|k| k == library) {
                off.push(library.to_string());
                self.set_preference(account, HIDDEN_LIBRARIES, &serde_json::json!(off), super::now_ms())?;
            }
        }
        Ok(())
    }

    /// What each account names a library by, as `moved`, (was, is) paths, leaves it (`Catalogue::paths_to_follow`): the libraries
    /// they have off, and those Radio and Autoplay choose from. Each name moves once, so two libraries that changed places do not
    /// end up as they were. Dated now, as a move of filters is: a device's copy from before must not win the old path back, and
    /// turn on what was off. Blocking.
    pub fn follow_library_paths(&self, moved: &[(String, String)]) -> Result<()> {
        let mut told = Vec::new();
        {
            let mut db = self.db();
            let tx = db.transaction()?;
            let rows: Vec<(String, String)> = tx.prepare("SELECT account, value FROM preference WHERE key = ?1")?.query_map([HIDDEN_LIBRARIES], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            for (account, text) in rows {
                let Ok(off) = serde_json::from_str::<Vec<String>>(&text) else { continue };
                let mut now_off: Vec<String> = Vec::with_capacity(off.len());
                for key in &off {
                    let key = key_moved(key, moved).unwrap_or_else(|| key.clone());
                    if !now_off.contains(&key) {
                        now_off.push(key);
                    }
                }
                if now_off != off {
                    tx.execute("UPDATE preference SET value = ?3, changed = ?4 WHERE account = ?1 AND key = ?2", params![account, HIDDEN_LIBRARIES, serde_json::json!(now_off).to_string(), super::now_ms()])?;
                    told.push(account);
                }
            }
            let rows: Vec<(String, i64, String)> = tx.prepare("SELECT user, version, settings FROM user_settings")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
            for (user, version, text) in rows {
                let Ok(mut settings) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
                let Some(chosen) = settings["more_music"]["libraries"].as_array_mut() else { continue };
                let mut changed = false;
                for library in chosen.iter_mut() {
                    if let Some((_, is)) = moved.iter().find(|(was, _)| library.as_str().is_some_and(|l| std::path::Path::new(l) == std::path::Path::new(was))) {
                        *library = serde_json::Value::String(is.clone());
                        changed = true;
                    }
                }
                if changed {
                    tx.execute("UPDATE user_settings SET version = ?2, settings = ?3 WHERE user = ?1", params![user, super::now_ms().max(version + 1), settings.to_string()])?;
                }
            }
            tx.commit()?;
        }
        for account in told {
            self.hidden.lock().unwrap_or_else(|p| p.into_inner()).remove(&account);
            let _ = self.preferences.send(PreferenceChanged { account, key: HIDDEN_LIBRARIES.to_string() });
        }
        Ok(())
    }

    /// Every kept change of a preference from now on, whoever's.
    pub fn subscribe_preferences(&self) -> tokio::sync::broadcast::Receiver<PreferenceChanged> {
        self.preferences.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A device's copy of a filter holds the old ids and an older time. The latest change wins, so the move is dated now, or
    /// the copy wins and the old ids come back; one that holds nothing to move keeps its time.
    #[test]
    fn a_filter_that_moved_is_the_latest_change() {
        let u = UserData::open(None).unwrap();
        let album = |id: &str| json!({ "albums": { "on": [], "conditions": [{ "axis": "album", "not": false, "value": { "kind": "any_of", "values": [id] } }] } });
        u.set_preference("ann", "filters", &album("al-old"), 100).unwrap();
        u.set_preference("bob", "filters", &album("al-5"), 100).unwrap();
        u.set_preference("ann", "pinned", &json!(["al-old"]), 100).unwrap();
        let moved = crate::moved::Moved { ids: [("al-old".to_string(), "al-7".to_string())].into(), ..Default::default() };
        let report = u.move_filters(&moved).unwrap();
        let kept = |who: &str, key: &str| u.preferences(who).unwrap().remove(key).unwrap();
        assert_eq!(kept("ann", "filters").value, album("al-7"));
        assert!(kept("ann", "filters").changed > 100);
        assert_eq!(kept("bob", "filters").changed, 100, "nothing moved, nothing changed");
        assert_eq!(kept("ann", "pinned").value, json!(["al-old"]), "another preference is no filter");
        assert_eq!(u.set_preference("ann", "filters", &album("al-old"), 200).unwrap().value, album("al-7"), "the device's copy loses");
        let t = report.table_named("preference: filters").unwrap();
        assert_eq!((t.before, t.moved), (2, 1));
    }

    /// 0101 point 6: someone's own music starts off for the others, beside what they have off; only at its making.
    #[test]
    fn a_library_switched_off_for_others_joins_what_they_have_off() {
        let u = UserData::open(None).unwrap();
        u.set_preference("ann", HIDDEN_LIBRARIES, &json!(["cratarr/x"]), 1).unwrap();
        u.switch_library_off(&["ann".to_string(), "bo".to_string()], "cratarr//own/cy").unwrap();
        assert_eq!(u.hidden_libraries("ann").unwrap(), ["cratarr/x", "cratarr//own/cy"]);
        assert_eq!(u.hidden_libraries("bo").unwrap(), ["cratarr//own/cy"]);
        let changed = u.preferences("bo").unwrap()[HIDDEN_LIBRARIES].changed;
        u.switch_library_off(&["bo".to_string()], "cratarr//own/cy").unwrap();
        assert_eq!(u.preferences("bo").unwrap()[HIDDEN_LIBRARIES].changed, changed, "already off: nothing changes");
    }

    /// A library's switch says its path, so a library renamed or on a moved mount would come back on for whoever has it off.
    #[test]
    fn a_library_switched_off_stays_off_when_its_path_changes() {
        let u = UserData::open(None).unwrap();
        u.set_preference("ann", HIDDEN_LIBRARIES, &json!(["cratarr//media/a", "jf/abc", "cratarr//media/keep"]), 100).unwrap();
        u.set_preference("bo", HIDDEN_LIBRARIES, &json!(["cratarr//media/keep"]), 100).unwrap();
        assert_eq!(*u.hidden_libraries_kept("ann"), ["cratarr//media/a", "jf/abc", "cratarr//media/keep"], "kept, as a stream asks for it");
        let mut told = u.subscribe_preferences();
        u.follow_library_paths(&[("/media/a".to_string(), "/media/b".to_string())]).unwrap();
        assert_eq!(u.hidden_libraries("ann").unwrap(), ["cratarr//media/b", "jf/abc", "cratarr//media/keep"], "in its place, another server's id untouched");
        assert_eq!(*u.hidden_libraries_kept("ann"), ["cratarr//media/b", "jf/abc", "cratarr//media/keep"], "a stream hears of it at once");
        assert_eq!(told.try_recv().unwrap(), PreferenceChanged { account: "ann".into(), key: HIDDEN_LIBRARIES.into() }, "her open pages too");
        assert!(told.try_recv().is_err(), "nothing moved for bo, nothing told");
        assert_eq!(u.preferences("bo").unwrap()[HIDDEN_LIBRARIES].changed, 100, "and it keeps its time");
        let old = json!(["cratarr//media/a"]);
        assert_eq!(u.set_preference("ann", HIDDEN_LIBRARIES, &old, 200).unwrap().value, json!(["cratarr//media/b", "jf/abc", "cratarr//media/keep"]), "a device's copy from before loses");
    }

    /// Each name moves once: taken one library at a time, two that changed places would end up as they were.
    #[test]
    fn two_libraries_that_changed_places_keep_their_switches() {
        let u = UserData::open(None).unwrap();
        u.set_preference("ann", HIDDEN_LIBRARIES, &json!(["cratarr//media/a"]), 100).unwrap();
        u.follow_library_paths(&[("/media/a".to_string(), "/media/b".to_string()), ("/media/b".to_string(), "/media/a".to_string())]).unwrap();
        assert_eq!(u.hidden_libraries("ann").unwrap(), ["cratarr//media/b"]);
    }

    /// Radio's list names what is on, so a rename would turn the library off for Radio, and could leave it nothing to choose from.
    #[test]
    fn a_library_radio_chooses_from_stays_chosen_when_its_path_changes() {
        let u = UserData::open(None).unwrap();
        let v = u.put_settings("ann", 0, &json!({ "more_music": { "libraries": ["/media/a", "/media/keep"], "bandcamp": true }, "theme": 1 })).unwrap().unwrap();
        u.put_settings("bo", 0, &json!({ "theme": 2 })).unwrap().unwrap();
        let (bos, _) = u.settings_of("bo").unwrap();
        u.follow_library_paths(&[("/media/a".to_string(), "/media/b".to_string())]).unwrap();
        let (version, settings) = u.settings_of("ann").unwrap();
        assert_eq!(settings, json!({ "more_music": { "libraries": ["/media/b", "/media/keep"], "bandcamp": true }, "theme": 1 }));
        assert!(version > v, "a page holding the list as it was is told it is stale");
        assert_eq!(u.settings_of("bo").unwrap().0, bos, "nothing moved, nothing changed");
    }

    /// 0101 point 14: per preference the latest change wins, the whole value; a tie keeps what is kept; each account's
    /// own; a clock ahead counts as now; a kept change is told, a refused one is not.
    #[test]
    fn the_latest_change_of_a_preference_wins_whole() {
        let u = UserData::open(None).unwrap();
        let mut told = u.subscribe_preferences();
        let p = u.set_preference("ann", "pinned", &json!(["a", "b"]), 1_000).unwrap();
        assert_eq!(p, Preference { value: json!(["a", "b"]), changed: 1_000 });
        assert_eq!(told.try_recv().unwrap(), PreferenceChanged { account: "ann".into(), key: "pinned".into() });
        // An older change (a device offline since) loses, and hears what is kept.
        assert_eq!(u.set_preference("ann", "pinned", &json!(["old"]), 900).unwrap().value, json!(["a", "b"]));
        assert_eq!(u.set_preference("ann", "pinned", &json!(["tie"]), 1_000).unwrap().value, json!(["a", "b"]), "a tie keeps it");
        assert!(told.try_recv().is_err(), "nothing kept, nothing told");
        // A later one wins whole: no merging of lists.
        assert_eq!(u.set_preference("ann", "pinned", &json!(["c"]), 2_000).unwrap(), Preference { value: json!(["c"]), changed: 2_000 });
        assert!(u.preferences("bob").unwrap().is_empty(), "each account's own");
        // A device whose clock runs a day ahead is taken as now: a change made a moment later still wins.
        let ahead = super::super::now_ms() + 86_400_000;
        let p = u.set_preference("ann", "radio_last", &json!({"how": "similar"}), ahead).unwrap();
        assert!(p.changed <= super::super::now_ms(), "dated now, not a day ahead");
        std::thread::sleep(std::time::Duration::from_millis(2));
        let later = super::super::now_ms();
        assert_eq!(u.set_preference("ann", "radio_last", &json!({"how": "random"}), later).unwrap().value, json!({"how": "random"}));
        let all = u.preferences("ann").unwrap();
        assert_eq!(all.keys().collect::<Vec<_>>(), ["pinned", "radio_last"]);
    }

    #[test]
    fn the_libraries_switched_off_are_a_list_and_unknown_keys_are_refused() {
        let u = UserData::open(None).unwrap();
        assert!(refusal("theme", &json!("dark")).is_some(), "a device's own setting is no preference");
        assert!(refusal(HIDDEN_LIBRARIES, &json!("cratarr/m")).is_some());
        assert!(refusal(HIDDEN_LIBRARIES, &json!(["cratarr/m", 3])).is_some());
        assert!(refusal("filters", &json!("x".repeat(MOST_BYTES + 1))).is_some(), "too big");
        assert_eq!(refusal(HIDDEN_LIBRARIES, &json!(["cratarr/m"])), None);
        assert_eq!(*u.hidden_libraries_kept("ann"), Vec::<String>::new());
        assert_eq!(u.hidden_libraries("ann").unwrap(), Vec::<String>::new());
        u.set_preference("ann", HIDDEN_LIBRARIES, &json!(["cratarr/m", "jf/x"]), 5).unwrap();
        assert_eq!(u.hidden_libraries("ann").unwrap(), ["cratarr/m", "jf/x"]);
        assert_eq!(*u.hidden_libraries_kept("ann"), ["cratarr/m", "jf/x"], "a stream hears of a switch at once");
    }
}
