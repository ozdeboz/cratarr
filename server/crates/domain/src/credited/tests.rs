//! The record's one credit rule's pure rows (DC3a 3a-1), read from `tests/vectors/credit-rule.json` (written from the scenarios):
//! SCENARIOS-NAMES NAMES.18, NAMES.20, NAMES.22, NAMES.24; SCENARIOS-DC3 F54.1; SCENARIOS-COUPLING COU.10; SCENARIOS-CRED CRED.19,
//! CRED.25, CRED.27 (q2), (q3).

use serde::Deserialize;

use super::*;

const VECTORS: &str = include_str!("../../tests/vectors/credit-rule.json");

#[derive(Deserialize)]
struct Vectors {
    records: Vec<Row>,
    parts: Vec<PartsRow>,
}

/// One of a credit's artists as a row wants it: its name, its id (a source's own), its role (main unless said), its phrase.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
struct Want {
    name: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    phrase: Option<String>,
}

impl Want {
    fn of(e: &Entry) -> Want {
        Want {
            name: e.name.clone(),
            id: e.person.as_ref().map(|p| p.id.clone()).filter(|id| !id.is_empty()),
            role: (e.role == ListRole::Featuring).then(|| "featuring".to_string()),
            phrase: e.phrase.clone(),
        }
    }

    fn part(p: &cratarr_rules::names::Part) -> Want {
        Want { name: p.name.clone(), id: p.id.clone(), role: None, phrase: p.phrase.clone() }
    }
}

#[derive(Deserialize)]
struct Row {
    row: String,
    scenario: String,
    says: String,
    album: Option<FilesCredit>,
    tracks: Vec<FilesCredit>,
    #[serde(default)]
    sources: Vec<SourceCredit>,
    artists: Vec<Want>,
    text: String,
    rule: u8,
    compilation: bool,
    #[serde(default)]
    by: Option<Vec<String>>,
    #[serde(default)]
    versions: Option<Vec<String>>,
    #[serde(default)]
    track_artists: Option<Vec<Vec<String>>>,
    #[serde(default)]
    track_credits: Option<Vec<Vec<Want>>>,
    #[serde(default)]
    track_texts: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct PartsRow {
    row: String,
    scenario: String,
    says: String,
    parts: Vec<cratarr_rules::names::Part>,
    #[serde(default)]
    whole: Vec<String>,
    #[serde(default)]
    source: Vec<cratarr_rules::names::Part>,
    then: Vec<Want>,
    #[serde(default)]
    text: Option<String>,
}

fn vectors() -> Vectors {
    serde_json::from_str(VECTORS).expect("tests/vectors/credit-rule.json reads")
}

/// Every row of `scenario`, each checked whole; the failures listed together.
fn check(scenario: &str) {
    let rows: Vec<Row> = vectors().records.into_iter().filter(|r| r.scenario == scenario).collect();
    assert!(!rows.is_empty(), "{scenario}: no rows");
    let mut wrong: Vec<String> = Vec::new();
    for r in rows {
        let said = RecordSaid { album: r.album.clone(), tracks: r.tracks.clone(), sources: r.sources.clone(), whole: Vec::new() };
        let got = credited_artists(&said);
        let artists: Vec<Want> = got.entries.iter().map(Want::of).collect();
        let mut says = |what: &str, want: String, got: String| wrong.push(format!("({}) {what}: want {want}, got {got}. {}", r.row, r.says));
        if artists != r.artists {
            says("artists", format!("{:?}", r.artists), format!("{artists:?}"));
        }
        if got.shown() != r.text {
            says("text", r.text.clone(), got.shown());
        }
        if got.rule != r.rule {
            says("rule", r.rule.to_string(), got.rule.to_string());
        }
        if got.compilation != r.compilation {
            says("compilation", r.compilation.to_string(), got.compilation.to_string());
        }
        if let Some(by) = &r.by
            && &got.by != by
        {
            says("by", format!("{by:?}"), format!("{:?}", got.by));
        }
        let versions = r.versions.clone().unwrap_or_default();
        if got.versions != versions {
            says("versions", format!("{versions:?}"), format!("{:?}", got.versions));
        }
        if let Some(want) = &r.track_artists {
            let names: Vec<Vec<String>> = got.tracks.iter().map(|t| t.iter().filter(|e| e.role == ListRole::Main).map(|e| e.name.clone()).collect()).collect();
            if &names != want {
                says("track artists", format!("{want:?}"), format!("{names:?}"));
            }
        }
        if let Some(want) = &r.track_credits {
            let credits: Vec<Vec<Want>> = got.tracks.iter().map(|t| t.iter().map(Want::of).collect()).collect();
            if &credits != want {
                says("track credits", format!("{want:?}"), format!("{credits:?}"));
            }
        }
        if let Some(want) = &r.track_texts {
            let texts: Vec<String> =
                got.tracks.iter().map(|t| cratarr_rules::names::credit_text(&t.iter().map(|e| (e.name.as_str(), e.phrase.as_deref())).collect::<Vec<_>>())).collect();
            if &texts != want {
                says("track texts", format!("{want:?}"), format!("{texts:?}"));
            }
        }
        // Never a featured artist among the record's artists unless the record's own credit features it (CRED.25), never a
        // placeholder part an artist (F54.1 (n), T-DC3-38 for parts).
        for e in got.mains() {
            if got.entries.len() > 1 && cratarr_rules::crates::placeholder(&e.name) {
                says("never a placeholder part an artist", String::new(), e.name.clone());
            }
        }
    }
    assert!(wrong.is_empty(), "{scenario}'s rows:\n{}", wrong.join("\n"));
}

/// NAMES.20: one credit rule for a record, in order (rows 1-4), its rows (h)-(l) and (ft1)-(ft3); with COU.10 (a), (b) (a record
/// whose files name no album artist is credited by the same table: OQ-COU-3 (a); (b) as the coordinator corrected it, Kelpe alone).
/// Never a record artist from (j)'s partial tracks; never (h)'s Anz one; never (l) split; never a featured artist raised to the
/// record. Red first: no rule, the record's artist line is the files' album artist (`record_credits`, records.rs).
#[test]
fn one_credit_rule_in_order() {
    check("NAMES.20");
    check("COU.10");
}

/// NAMES.18: a Various Artists record of 2 to 4 artists shows them and is no compilation; 5 or more stays Various Artists, a
/// compilation; a reference's credit first (rows (a)-(g)). Never Perspeeks a compilation; never (c) split into five; never (d)'s or
/// (e)'s credit replaced by the tracks' artists. Red first: the record's artist is the files' "Various Artists".
#[test]
fn a_split_of_two_to_four_artists_shows_them_and_is_no_compilation() {
    check("NAMES.18");
}

/// F54.1: a credit splits only where the files or a source name each artist, rows (m)-(z2) and (w) as corrected for GEN.63. Never
/// (n), (n2), (n4), (o)-(r) or (z1) split; never (s) two main artists; never (m)'s compound given a part's id; never (n4) credited by its
/// line's name over the id its files give. Red first: (m), (n), (n2), (n4), (x) and (s) at the record; (n3), (o)-(r), (t)-(w), (z1),
/// (z2) are guards.
#[test]
fn a_credit_splits_only_where_files_or_a_source_name_each() {
    check("F54.1");
}

/// CRED.25 (the record's half): featuring is per track, per record only where the release's own credit says so, and never rises.
/// Same Boat's Rowan Fell alone; Harbour's Rowan Fell with Tamsin Kell featuring on the record, printed "Rowan Fell feat. Tamsin
/// Kell"; ALL's Yann Tiersen; Night Signals' Rowan Fell (and CRED.27 (q2), (q3): tracks 3 and 6 read "Rowan Fell & Oda Brenn").
/// Never a featured artist a record artist. Red first: no role at the record; Harbour's line one artist.
#[test]
fn featuring_is_per_track_and_never_rises() {
    check("CRED.25");
}

/// NAMES.22 and NAMES.24's source rows: a source's credit confirms the record's or is a version that differs, none chosen, no artist
/// dropped or added by one; a confirming source's own phrase prints ("Commodo x Alfa Mist"). Red first: no source's credit is read.
#[test]
fn a_sources_credit_confirms_or_differs_and_drops_no_one() {
    check("NAMES.22");
    check("NAMES.24");
}

/// CRED.19, F54's half: a role value splits only where the files or a source name each person, through `credit_parts` alone; and
/// F54.1's rows for one credit. Never a credit cut on a bare "&"; never "Friends" a person; never a split list of CRED's own. Red
/// first: (b), (m), (n), (n4) (no rule: the parts as given); (a), (c)-(f), (z1) are guards.
#[test]
fn role_credits_split_by_f54_alone() {
    let mut wrong: Vec<String> = Vec::new();
    for r in vectors().parts {
        let whole: Vec<&str> = r.whole.iter().map(String::as_str).collect();
        let got = cratarr_rules::names::credit_parts(&r.parts, &whole, &r.source);
        let parts: Vec<Want> = got.parts.iter().map(Want::part).collect();
        if parts != r.then || got.text != r.text {
            wrong.push(format!("{} ({}): want {:?} text {:?}, got {parts:?} text {:?}. {}", r.scenario, r.row, r.then, r.text, got.text, r.says));
        }
    }
    assert!(wrong.is_empty(), "credit_parts' rows:\n{}", wrong.join("\n"));
}
