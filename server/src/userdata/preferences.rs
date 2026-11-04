//! The person's preferences on their account (decision 0101 point 14): pinned items, libraries switched off, filters,
//! Home's header picture, crate drafts and Radio's last choice, followed from device to device. Per preference the
//! latest change wins, the whole value; offline a device works from its cached copy. Only the libraries switched off
//! mean anything to the server (Subsonic's music folders); the others are the page's, kept and handed back.

use std::collections::BTreeMap;

use anyhow::Result;
use rusqlite::params;

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
            let _ = self.preferences.send(PreferenceChanged { account: account.to_string(), key: key.to_string() });
        }
        Ok(Preference { value: serde_json::from_str(&value).unwrap_or(serde_json::Value::Null), changed })
    }

    /// The libraries `account` has switched off, as `source/library`. Blocking.
    pub fn hidden_libraries(&self, account: &str) -> Result<Vec<String>> {
        let value = self.preferences(account)?.remove(HIDDEN_LIBRARIES).map(|p| p.value);
        Ok(value.and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default())
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
        assert_eq!(u.hidden_libraries("ann").unwrap(), Vec::<String>::new());
        u.set_preference("ann", HIDDEN_LIBRARIES, &json!(["cratarr/m", "jf/x"]), 5).unwrap();
        assert_eq!(u.hidden_libraries("ann").unwrap(), ["cratarr/m", "jf/x"]);
    }
}
