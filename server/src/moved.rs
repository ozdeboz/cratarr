//! Ids that move at the first start of 0.16 (decision 0101 point 21, docs/records-and-copies.md): the shape of what each
//! place says of it in the migration's report. Nothing calls this until the first start does.

#![allow(dead_code)] // the first start's (C34)

use serde::Serialize;

/// A table or a store as the migration's report reads it (`userdata::put_migration_part`): its rows before and after, and
/// how many of them moved.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Table {
    pub name: String,
    pub before: u64,
    pub after: u64,
    pub moved: u64,
}

/// What found no match in one place: how many, and up to ten of them. They stay as they were.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Unmatched {
    pub what: String,
    pub count: u64,
    pub sample: Vec<String>,
}

/// How many samples an unmatched row keeps.
const SAMPLES: usize = 10;

/// What a step of the move found, in the report's two lists.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Report {
    pub tables: Vec<Table>,
    pub unmatched: Vec<Unmatched>,
}

impl Report {
    pub fn table(&mut self, name: &str, before: u64, after: u64, moved: u64) {
        self.tables.push(Table { name: name.to_string(), before, after, moved });
    }

    /// A place's ids that found no match: a row for each place, none found too, so the report says it looked.
    pub fn unmatched(&mut self, what: &str, mut ids: Vec<String>) {
        let count = ids.len() as u64;
        ids.truncate(SAMPLES);
        self.unmatched.push(Unmatched { what: what.to_string(), count, sample: ids });
    }

    /// The rows of one table, by its name.
    pub fn table_named(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name == name)
    }

    /// How many found no match, in all.
    pub fn unmatched_count(&self) -> u64 {
        self.unmatched.iter().map(|u| u.count).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_says_where_it_looked_and_keeps_ten_samples() {
        let mut r = Report::default();
        r.unmatched("nothing", Vec::new());
        r.unmatched("many", (0..25).map(|n| format!("al-{n}")).collect());
        assert_eq!(r.unmatched_count(), 25);
        assert_eq!((r.unmatched[0].count, r.unmatched[1].sample.len()), (0, 10));
    }
}
