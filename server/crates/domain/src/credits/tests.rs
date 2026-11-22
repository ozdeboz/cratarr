//! SCENARIOS-CRED's pure rows (DC3c): CRED.1's roles table, CRED.2's merge by id, CRED.18's one order, read from
//! `tests/vectors/credits.json` (written from the scenario).

use serde::Deserialize;

use super::*;

const VECTORS: &str = include_str!("../../tests/vectors/credits.json");

#[derive(Deserialize)]
struct Vectors {
    roles: Vec<RoleRow>,
    here: Here,
    merges: Vec<Merge>,
    lists: Vec<ListRow>,
}

/// CRED.24's rows: a source's artist credit (the files' line, list, ids, title and kept names; a service's entries with their
/// joining text) and the ordered list it is.
#[derive(Deserialize)]
struct ListRow {
    row: String,
    says: String,
    source: String,
    #[serde(default)]
    line: String,
    #[serde(default)]
    parts: Vec<String>,
    #[serde(default)]
    ids: Vec<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    whole: Vec<String>,
    #[serde(default)]
    placeholder: Vec<String>,
    #[serde(default)]
    entries: Vec<ListEntry>,
    list: Vec<Entry>,
}

#[derive(Deserialize)]
struct ListEntry {
    name: String,
    join: String,
    #[serde(default)]
    person: Option<String>,
}

#[derive(Deserialize)]
struct RoleRow {
    source: String,
    word: String,
    #[serde(default)]
    attributes: Vec<String>,
    value: String,
    reads: Vec<Reading>,
    #[serde(default)]
    says: String,
}

#[derive(Deserialize)]
struct Merge {
    case: String,
    all: Vec<u32>,
    #[serde(default)]
    orders: bool,
    said: Vec<Said>,
    credits: serde_json::Value,
}

fn vectors() -> Vectors {
    serde_json::from_str(VECTORS).expect("tests/vectors/credits.json reads")
}

/// CRED.1: every source's role word has one role (the files' tag names as Picard writes them, ID3v2's TIPL/TMCL/IPLS pairs by
/// their role, MusicBrainz's relationship types and attributes, Discogs' role strings): an instrument the same from every source,
/// a compound Discogs string split on its commas outside brackets, a bracket read as the role's own word, an instrument's qualifier
/// or the instruments, MusicBrainz's qualifiers kept apart from the instrument; a word no row names kept under its own word. Never
/// a role worked out outside the table, a word dropped, a compound filed whole under an unknown word, a qualifier shown as an
/// instrument, a pair's role lost. Red first: no table (`role_of`, rec/works.rs, maps six performing words to four roles and drops
/// the rest).
#[test]
fn every_sources_role_word_has_one_role() {
    let v = vectors();
    let mut wrong = Vec::new();
    for r in &v.roles {
        let read = read_role(&r.source, &r.word, &r.attributes, &r.value);
        if read != r.reads {
            wrong.push(format!("{} {:?} {:?} {:?} ({}): {:?}, the table says {:?}", r.source, r.word, r.attributes, r.value, r.says, read, r.reads));
        }
    }
    assert!(wrong.is_empty(), "{} of {} rows:\n{}", wrong.len(), v.roles.len(), wrong.join("\n"));
    // Every role the table names is one of the one order's; none is dropped.
    for (source, word, role) in TABLE {
        assert!(role_at(role) < ROLES.len(), "{source} {word}: {role} is in the one order");
    }
    assert!(INSTRUMENTS.iter().all(|i| *i == i.to_lowercase()), "instruments are folded");
}

/// CRED.2: one store per record and per track, merged by id (rows (a)-(i)): the files' name with no id is the person a source
/// credits on the same record by that name (T-CRED-4); two sources' persons one only where one artist here holds both references
/// (T-CRED-5); namesakes never joined by a name (X8); a credit on every track the record's; a featured artist a featuring credit, a
/// track artist who is no record artist an "artist" credit, a record artist never under "artist". Red first: no merge and no store.
#[test]
fn one_credits_store_merged_by_id() {
    let v = vectors();
    let mut wrong = Vec::new();
    for m in v.merges.iter().filter(|m| !m.orders) {
        let got = serde_json::to_value(credits_of(&m.said, &v.here, &m.all)).unwrap();
        if got != m.credits {
            wrong.push(format!("{}:\n  got  {got}\n  want {}", m.case, m.credits));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Every order of a list's items.
fn orders<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let first = rest.remove(i);
        for mut o in orders(&rest) {
            o.insert(0, first.clone());
            out.push(o);
        }
    }
    out
}

/// CRED.2 (j) and CRED.18: a credit is never ranked by its source. (a)-(e) with both sources' piano and the two mixers, the sources
/// answering in each of their six orders (and every credit's rows in that order): the same credits, the same order, the same `by`;
/// within a role by first track, then shown name; both mixers shown; `by` in the fixed source order. Never a credit dropped, hidden,
/// ordered first or marked by the source naming it or the order it answered; never a "main" source. Red first: no store (the page
/// orders by Discogs alone, lib/AlbumAbout.svelte:83).
#[test]
fn credits_are_never_ranked_by_source() {
    let v = vectors();
    for m in v.merges.iter().filter(|m| m.orders) {
        let sources: Vec<String> = {
            let mut s: Vec<String> = Vec::new();
            for x in &m.said {
                if !s.contains(&x.source) {
                    s.push(x.source.clone());
                }
            }
            s
        };
        for order in orders(&sources) {
            let said: Vec<Said> = order.iter().flat_map(|src| m.said.iter().filter(move |x| &x.source == src)).cloned().collect();
            let got = serde_json::to_value(credits_of(&said, &v.here, &m.all)).unwrap();
            assert_eq!(got, m.credits, "{} in the order {order:?}", m.case);
            let mut backwards = said.clone();
            backwards.reverse();
            let got = serde_json::to_value(credits_of(&backwards, &v.here, &m.all)).unwrap();
            assert_eq!(got, m.credits, "{} in the order {order:?}, each source's rows the other way round", m.case);
        }
    }
}

/// CRED.24: a credit is an ordered list, each artist with its role and the phrase after it, per source (rows (p1)-(p18) and ALL's
/// tracks): "/" and ";" separators, never a phrase; "&", "x" and the like printed phrases between artists the files or a source
/// name apart; "feat.", "ft." and "featuring" (whole words, case folded) the Featuring role, the artist before keeping the marker
/// as spelled; a title's "(feat. B)" featuring B, the title whole; "Feat" with no dot, and a kept name, no marker; a placeholder
/// featured nowhere (F54's rule, handed in); a featured part one name (no split on a bare "&"). Never a separator kept as a phrase, a
/// marker read inside a name, a placeholder featured, a featured part split on a bare "&", a title rewritten. Red first: no list
/// (MusicBrainz's credit joined into one string with its first id, rec/track_fetch_records.rs:55-60; the works pass dropping the
/// phrases; Discogs' `join` never read; the files' ARTIST served raw).
#[test]
fn a_credit_is_an_ordered_list_of_artist_role_and_phrase() {
    let v = vectors();
    let mut wrong = Vec::new();
    for r in &v.lists {
        let got = match r.source.as_str() {
            "files" => {
                let placeholder = |name: &str| r.placeholder.iter().any(|p| name_key(p) == name_key(name));
                files_list(&r.line, &r.parts, &r.ids, r.title.as_deref(), &r.whole, &placeholder)
            }
            source => {
                let entries: Vec<(String, Option<Listed>, String)> =
                    r.entries.iter().map(|e| (e.name.clone(), e.person.as_ref().map(|id| Listed { source: source.to_string(), id: id.clone() }), e.join.clone())).collect();
                source_list(&entries)
            }
        };
        if got != r.list {
            wrong.push(format!("({}) {}:\n  got  {}\n  want {}", r.row, r.says, serde_json::to_string(&got).unwrap(), serde_json::to_string(&r.list).unwrap()));
        }
    }
    assert!(wrong.is_empty(), "{} of {} rows:\n{}", wrong.len(), v.lists.len(), wrong.join("\n"));
}
