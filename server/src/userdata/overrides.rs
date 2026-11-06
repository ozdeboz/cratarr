//! A record's facts set by hand (0.13, the owner: "a coherent and unified way to modify type, alongside other attributes
//! of records"; decision 0089): its type, secondary types, year, genres and whether it is classical,
//! each winning over its files, MusicBrainz, Discogs and the rules until it is reset. Household-wide and by the record, so a
//! setting holds whichever copies it has. The classical marks (0.10) are its
//! `classical`. A record's own words (title, artists, track numbers) are its files': those are fixed in its tags.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::UserData;

pub const SCHEMA: &str = "
-- A record's fact set by hand (0.13): by the record and the fact's name, its value as JSON, and when.
CREATE TABLE IF NOT EXISTS record_overrides (album TEXT NOT NULL, field TEXT NOT NULL, value TEXT NOT NULL, at INTEGER NOT NULL,
  PRIMARY KEY (album, field)) WITHOUT ROWID;
";

/// The release types a record can be set to.
pub const TYPES: [&str; 5] = ["album", "ep", "single", "broadcast", "other"];

/// What is set of one record; none: as Cratarr finds it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overrides {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genres: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classical: Option<bool>,
    /// Its styles, moods and tags (decision 0097: the words' hand layer), each winning its kind until reset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub styles: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moods: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl Overrides {
    /// The facts set, by their names.
    pub fn fields(&self) -> Vec<&'static str> {
        let set = [
            ("type", self.kind.is_some()),
            ("secondary", self.secondary.is_some()),
            ("year", self.year.is_some()),
            ("genres", self.genres.is_some()),
            ("classical", self.classical.is_some()),
            ("styles", self.styles.is_some()),
            ("moods", self.moods.is_some()),
            ("tags", self.tags.is_some()),
        ];
        set.into_iter().filter(|(_, on)| *on).map(|(f, _)| f).collect()
    }

    /// One kept fact read in; one of another name, or a value not of its kind, is left out.
    fn read(&mut self, field: &str, value: Value) {
        match field {
            "type" => self.kind = value.as_str().map(str::to_string),
            "secondary" => self.secondary = serde_json::from_value(value).ok(),
            "year" => self.year = value.as_u64().and_then(|y| u32::try_from(y).ok()),
            "genres" => self.genres = serde_json::from_value(value).ok(),
            "classical" => self.classical = value.as_bool(),
            "styles" => self.styles = serde_json::from_value(value).ok(),
            "moods" => self.moods = serde_json::from_value(value).ok(),
            "tags" => self.tags = serde_json::from_value(value).ok(),
            _ => {}
        }
    }
}

/// A fact as it is set: its value as kept, or why it cannot be (the words the page shows). `null` is no value: the
/// fact is reset.
pub fn check(field: &str, value: &Value) -> std::result::Result<Option<Value>, String> {
    if value.is_null() {
        return match field {
            "type" | "secondary" | "year" | "genres" | "classical" | "styles" | "moods" | "tags" => Ok(None),
            _ => Err(format!("{field} cannot be set.")),
        };
    }
    let words = |what: &str| -> std::result::Result<Vec<String>, String> {
        let list = value.as_array().ok_or_else(|| format!("{what} is a list."))?;
        let mut out: Vec<String> = Vec::new();
        for w in list {
            let w = w.as_str().ok_or_else(|| format!("{what} is a list of words."))?.trim();
            if !w.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(w)) {
                out.push(w.to_string());
            }
        }
        Ok(out)
    };
    match field {
        "type" => {
            let t = value.as_str().map(|t| t.trim().to_lowercase()).filter(|t| TYPES.contains(&t.as_str()));
            t.map(|t| Some(Value::from(t))).ok_or_else(|| "A type is album, ep, single, incomplete or other.".into())
        }
        "secondary" => {
            let list: Vec<String> = words("secondary")?.into_iter().map(|w| w.to_lowercase()).collect();
            match list.iter().find(|w| !crate::serve::library::SECONDARY.contains(&w.as_str())) {
                Some(w) => Err(format!("{w} is not a secondary type.")),
                None => Ok(Some(Value::from(list))),
            }
        }
        "year" => value.as_u64().filter(|y| (1..=9999).contains(y)).map(|y| Some(Value::from(y))).ok_or_else(|| "A year is a number.".into()),
        "genres" => Ok(Some(Value::from(words("genres")?))),
        "styles" => Ok(Some(Value::from(words("styles")?))),
        "moods" => Ok(Some(Value::from(words("moods")?))),
        "tags" => Ok(Some(Value::from(words("tags")?))),
        "classical" => value.as_bool().map(|c| Some(Value::from(c))).ok_or_else(|| "classical is yes or no.".into()),
        _ => Err(format!("{field} cannot be set.")),
    }
}

/// The classical marks (0.10) moved in as `classical` (a mark of `classical`: yes; `not`: no), and their table emptied.
/// Safe to repeat.
pub(super) fn move_classical_marks(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "INSERT OR IGNORE INTO record_overrides (album, field, value, at)
           SELECT album, 'classical', CASE mark WHEN 'classical' THEN 'true' ELSE 'false' END, at FROM classical_marks;
         DELETE FROM classical_marks;",
    )?;
    Ok(())
}

impl UserData {
    /// Every record's facts set, by its id.
    pub fn overrides(&self) -> Result<HashMap<String, Overrides>> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT album, field, value FROM record_overrides")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        let mut out: HashMap<String, Overrides> = HashMap::new();
        for row in rows {
            let (album, field, value) = row?;
            if let Ok(v) = serde_json::from_str(&value) {
                out.entry(album).or_default().read(&field, v);
            }
        }
        Ok(out)
    }

    /// One record's facts set.
    pub fn overrides_of(&self, album: &str) -> Result<Overrides> {
        let db = self.db();
        let mut st = db.prepare_cached("SELECT field, value FROM record_overrides WHERE album = ?1")?;
        let rows = st.query_map([album], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Overrides::default();
        for row in rows {
            let (field, value) = row?;
            if let Ok(v) = serde_json::from_str(&value) {
                out.read(&field, v);
            }
        }
        Ok(out)
    }

    /// Sets (or, with none, resets) these facts of these records, as one write. The facts are `check`ed already.
    pub fn set_overrides(&self, albums: &[String], facts: &[(String, Option<Value>)], at: i64) -> Result<()> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut set = tx.prepare_cached("INSERT OR REPLACE INTO record_overrides (album, field, value, at) VALUES (?1, ?2, ?3, ?4)")?;
            let mut reset = tx.prepare_cached("DELETE FROM record_overrides WHERE album = ?1 AND field = ?2")?;
            for album in albums {
                for (field, value) in facts {
                    match value {
                        Some(v) => set.execute(params![album, field, v.to_string(), at])?,
                        None => reset.execute(params![album, field])?,
                    };
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// A cheap summary of the facts set (`field` one name, or every one): it changes when any does.
    pub fn overrides_stamp(&self, field: Option<&str>) -> Result<i64> {
        let (n, sum): (i64, i64) = self.db().query_row(
            "SELECT count(*), coalesce(sum(at + length(value) * 1000003 + unicode(value) * 7919), 0) FROM record_overrides
             WHERE ?1 IS NULL OR field = ?1",
            [field],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(n.wrapping_mul(1_000_003).wrapping_add(sum))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn facts_are_checked_as_they_are_set() {
        assert_eq!(check("type", &json!("EP")), Ok(Some(json!("ep"))));
        assert!(check("type", &json!("mixtape")).is_err());
        assert_eq!(check("secondary", &json!(["Live", "live", " Compilation "])), Ok(Some(json!(["live", "compilation"]))));
        assert!(check("secondary", &json!(["gig"])).is_err(), "only the secondary types");
        assert_eq!(check("year", &json!(1972)), Ok(Some(json!(1972))));
        assert!(check("year", &json!("1972")).is_err());
        assert_eq!(check("genres", &json!(["Jazz", " jazz", "", "Soul"])), Ok(Some(json!(["Jazz", "Soul"]))));
        assert_eq!(check("classical", &json!(false)), Ok(Some(json!(false))));
        assert_eq!(check("genres", &Value::Null), Ok(None), "null resets");
        assert!(check("title", &json!("Kind of Blue")).is_err(), "a file's own words are fixed in its tags");
    }

    #[test]
    fn facts_are_set_and_reset_for_several_records_at_once() {
        let u = UserData::open(None).unwrap();
        let albums = ["al-1".to_string(), "al-2".to_string()];
        u.set_overrides(&albums, &[("type".into(), Some(json!("ep"))), ("year".into(), Some(json!(1999)))], 100).unwrap();
        let before = u.overrides_stamp(None).unwrap();
        u.set_overrides(&albums[..1], &[("year".into(), None), ("genres".into(), Some(json!(["Dub"])))], 200).unwrap();
        assert_ne!(u.overrides_stamp(None).unwrap(), before);
        let all = u.overrides().unwrap();
        assert_eq!(all["al-1"], Overrides { kind: Some("ep".into()), genres: Some(vec!["Dub".into()]), ..Default::default() });
        assert_eq!(all["al-2"], Overrides { kind: Some("ep".into()), year: Some(1999), ..Default::default() });
        assert_eq!(u.overrides_of("al-1").unwrap().fields(), ["type", "genres"]);
        assert_eq!(u.overrides_of("al-3").unwrap(), Overrides::default());
    }

    #[test]
    fn the_classical_marks_move_in_as_classical() {
        let u = UserData::open(None).unwrap();
        u.db()
            .execute_batch("INSERT INTO classical_marks (album, mark, at) VALUES ('al-1', 'classical', 5), ('al-2', 'not', 6);")
            .unwrap();
        move_classical_marks(&u.db()).unwrap();
        move_classical_marks(&u.db()).unwrap();
        let all = u.overrides().unwrap();
        assert_eq!((all["al-1"].classical, all["al-2"].classical), (Some(true), Some(false)));
        assert_eq!(u.classical_marks().unwrap().len(), 2, "read as marks still");
    }
}
