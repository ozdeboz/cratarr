//! F12.1 (the owner: "One card, 2 editions"): which records are editions of one album, so the pages show them as one card (an
//! artist's page, search's Records, a label's page) and an album's page lists them. Display only: never a join or a merge (Q8: two
//! tracklists are two records).

use std::collections::{BTreeSet, HashMap};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::Catalogue;
use crate::incoming::TrackFacts;
use crate::serve::ids;

/// A record as editions are told apart (`groups_of`): its id, the artists credited on it, its title's key (`identity::title_key`),
/// its MusicBrainz release groups, how many live tracks it has, and whether a main library holds a copy of it.
#[derive(Debug, Clone, Default)]
struct Facts {
    id: i64,
    artists: BTreeSet<i64>,
    key: String,
    groups: BTreeSet<String>,
    tracks: usize,
    main: bool,
    /// The records it is linked to as an edition by evidence stored at a join (WM.12's placement: `edition_links`).
    linked: BTreeSet<i64>,
}

fn id_list(ids: &[i64]) -> String {
    ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
}

/// Whether most tracks of the shorter of two tracklists pair with the longer's, as Replace pairs them (`incoming::pair`): more than
/// half of the shorter's. Pure.
fn paired_most(a: &[TrackFacts], b: &[TrackFacts]) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    !short.is_empty() && crate::incoming::pair(short, long).pairs.len() * 2 > short.len()
}

/// Which of these records are editions of one album (F12.1), each group the edition shown first: one a main library holds a copy
/// of, else the fullest (most live tracks), else the oldest. Two records are one album's where both hold MusicBrainz release groups
/// and share one; else, where one artist is credited on both, their titles have one key and most tracks of the shorter pair with
/// the longer's (`paired_most`). A record joins no group holding a release group where its own differ: two release groups are two
/// albums, a name several albums share (X8) stays as many cards, and two artists of one name are two. Groups of two or more.
/// `tracks_of` is asked only of two records of one artist and one title's key.
fn groups_of(facts: &[Facts], mut tracks_of: impl FnMut(i64) -> Result<Vec<TrackFacts>>) -> Result<Vec<Vec<i64>>> {
    let mut lists: HashMap<i64, Vec<TrackFacts>> = HashMap::new();
    let mut edition = |a: &Facts, b: &Facts| -> Result<bool> {
        if a.linked.contains(&b.id) || b.linked.contains(&a.id) {
            return Ok(true);
        }
        if !a.groups.is_empty() && !b.groups.is_empty() {
            return Ok(!a.groups.is_disjoint(&b.groups));
        }
        if a.key.is_empty() || a.key != b.key || a.artists.is_disjoint(&b.artists) {
            return Ok(false);
        }
        for r in [a.id, b.id] {
            if !lists.contains_key(&r) {
                let tracks = tracks_of(r)?;
                lists.insert(r, tracks);
            }
        }
        Ok(paired_most(&lists[&a.id], &lists[&b.id]))
    };
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, f) in facts.iter().enumerate() {
        let mut joined = None;
        for (g, members) in groups.iter().enumerate() {
            let linked = members.iter().any(|&m| facts[m].linked.contains(&f.id) || f.linked.contains(&facts[m].id));
            if !linked && members.iter().any(|&m| !facts[m].groups.is_empty() && !f.groups.is_empty() && facts[m].groups.is_disjoint(&f.groups)) {
                continue;
            }
            let mut one = false;
            for &m in members {
                if edition(&facts[m], f)? {
                    one = true;
                    break;
                }
            }
            if one {
                joined = Some(g);
                break;
            }
        }
        match joined {
            Some(g) => groups[g].push(i),
            None => groups.push(vec![i]),
        }
    }
    Ok(groups
        .into_iter()
        .filter(|g| g.len() > 1)
        .map(|mut g| {
            g.sort_by_key(|&i| (!facts[i].main, std::cmp::Reverse(facts[i].tracks), facts[i].id));
            g.into_iter().map(|i| facts[i].id).collect()
        })
        .collect())
}

/// These records' facts as `groups_of` tells editions apart, the live ones alone, each once, in their order: a few queries whatever
/// their number.
fn facts_in(db: &Connection, records: &[i64], mains: &[i64]) -> Result<Vec<Facts>> {
    use super::identity::title_key;
    let mut seen = BTreeSet::new();
    let asked: Vec<i64> = records.iter().copied().filter(|r| seen.insert(*r)).collect();
    let list = id_list(&asked);
    let mut by_id: HashMap<i64, Facts> = HashMap::new();
    let mut st = db.prepare(&format!("SELECT id, COALESCE(title, '') FROM records WHERE id IN ({list}) AND merged_into IS NULL AND retired_at IS NULL"))?;
    for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (id, title) = row?;
        by_id.insert(id, Facts { id, key: title_key(&title), ..Facts::default() });
    }
    let mut st = db.prepare(&format!("SELECT record, of FROM edition_links WHERE record IN ({list}) OR of IN ({list})"))?;
    for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
        let (record, of) = row?;
        if let Some(f) = by_id.get_mut(&record) {
            f.linked.insert(of);
        }
        if let Some(f) = by_id.get_mut(&of) {
            f.linked.insert(record);
        }
    }
    let mut st = db.prepare(&format!("SELECT record, artist FROM record_artists WHERE record IN ({list})"))?;
    for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
        let (record, artist) = row?;
        if let Some(f) = by_id.get_mut(&record) {
            f.artists.insert(artist);
        }
    }
    let subjects = asked.iter().map(|r| format!("'{}'", ids::record(*r))).collect::<Vec<_>>().join(",");
    let mut st = db.prepare(&format!("SELECT subject, value FROM \"references\" WHERE source = 'musicbrainz' AND kind = 'release-group' AND subject IN ({subjects})"))?;
    for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (subject, value) = row?;
        if let Some(f) = ids::record_number(&subject).and_then(|r| by_id.get_mut(&r)) {
            f.groups.insert(value.to_ascii_lowercase());
        }
    }
    let mut st = db.prepare(&format!("SELECT record, count(*) FROM tracks WHERE record IN ({list}) AND retired_at IS NULL GROUP BY record"))?;
    for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
        let (record, n) = row?;
        if let Some(f) = by_id.get_mut(&record) {
            f.tracks = usize::try_from(n).unwrap_or(0);
        }
    }
    if !mains.is_empty() {
        let mut st = db.prepare(&format!(
            "SELECT DISTINCT t.record FROM tracks t JOIN files f ON f.track = t.id WHERE t.record IN ({list}) AND t.retired_at IS NULL AND {} AND f.folder IN ({})",
            super::PRESENT_F,
            id_list(mains)
        ))?;
        for row in st.query_map([], |r| r.get::<_, i64>(0))? {
            if let Some(f) = by_id.get_mut(&row?) {
                f.main = true;
            }
        }
    }
    Ok(asked.into_iter().filter_map(|r| by_id.remove(&r)).collect())
}

/// A record's live tracks as Replace pairs them (`incoming::pair`, as RF2's `Serve::paired_editions` reads a copy): each by its first
/// present file's named tags and length, its place the record's where the tags give none. In track order.
fn tracks_in(db: &Connection, record: i64) -> Result<Vec<TrackFacts>> {
    let rows: Vec<(i64, Option<i64>, Option<i64>, Option<i64>, Option<String>, Option<i64>)> = db
        .prepare_cached(&format!(
            "SELECT t.id, t.disc, t.number, f.folder, f.rel, f.duration_ms FROM tracks t LEFT JOIN files f ON f.track = t.id AND {}
             WHERE t.record = ?1 AND t.retired_at IS NULL ORDER BY t.disc, t.number, t.id, f.folder, f.rel",
            super::PRESENT_F
        ))?
        .query_map([record], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut tags = db.prepare_cached("SELECT name, value FROM tags WHERE folder = ?1 AND rel = ?2 AND name IS NOT NULL ORDER BY n")?;
    let mut out = Vec::new();
    let mut last = None;
    for (track, disc, number, folder, rel, ms) in rows {
        if last == Some(track) {
            continue;
        }
        last = Some(track);
        let named: Vec<(String, String)> = match (folder, &rel) {
            (Some(folder), Some(rel)) => tags.query_map(params![folder, rel], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?,
            _ => Vec::new(),
        };
        let mut facts = TrackFacts::of(&crate::naming::Tags(named), rel.as_deref().unwrap_or_default(), ms.and_then(|m| u64::try_from(m).ok()).filter(|m| *m > 0));
        facts.disc = facts.disc.or(disc.and_then(|d| u32::try_from(d).ok()));
        facts.number = facts.number.or(number.and_then(|n| u32::try_from(n).ok()));
        out.push(facts);
    }
    Ok(out)
}

/// RG6 (PA6, the libraries switched on): which of these records hold copies in the catalogue folders `off` alone (the libraries a
/// viewer has switched off, other accounts' own music among them where it is off): every present file of their live tracks there. One
/// holding no copy (a record not had) is not. Nothing off: none.
fn hidden_in(db: &Connection, records: &[i64], off: &[i64]) -> Result<BTreeSet<i64>> {
    if off.is_empty() || records.is_empty() {
        return Ok(BTreeSet::new());
    }
    let mut st = db.prepare(&format!(
        "SELECT t.record FROM tracks t JOIN files f ON f.track = t.id WHERE t.record IN ({}) AND t.retired_at IS NULL AND {} GROUP BY t.record HAVING sum(f.folder NOT IN ({})) = 0",
        id_list(records),
        super::PRESENT_F,
        id_list(off)
    ))?;
    let hidden = st.query_map([], |r| r.get::<_, i64>(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(hidden)
}

impl Catalogue {
    /// Which of these records are editions of one album (F12.1, `groups_of`), each group the edition shown first: one with a copy in
    /// one of `mains` (the main libraries), else the fullest. Records gone (merged, retired) are left out. Nothing is written: they
    /// stay two records (Q8). Of the records the viewer sees (RG6): `off` the catalogue folders of the libraries they have off. On a
    /// reader, never the writer's lock. Blocking.
    pub fn edition_groups(&self, records: &[i64], mains: &[i64], off: &[i64]) -> Result<Vec<Vec<i64>>> {
        if records.len() < 2 {
            return Ok(Vec::new());
        }
        let read = |db: &Connection| -> Result<Vec<Vec<i64>>> {
            // RG6 (the libraries switched on): a record whose every copy lies in a library the viewer has off is no edition they see.
            let hidden = hidden_in(db, records, off)?;
            let seen: Vec<i64> = records.iter().copied().filter(|r| !hidden.contains(r)).collect();
            let facts = facts_in(db, &seen, mains)?;
            groups_of(&facts, |r| tracks_in(db, r))
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }

    /// The live records that may be editions of `record` (F12.1, `edition_groups` tells which are): those holding one of its
    /// MusicBrainz release groups, and those an artist credited on it is credited on whose title has its title's key. In id order.
    /// Blocking.
    pub fn editions_near(&self, record: i64) -> Result<Vec<i64>> {
        use super::identity::title_key;
        let read = |db: &Connection| -> Result<Vec<i64>> {
            let mut out = BTreeSet::new();
            let mut st = db.prepare_cached(
                "SELECT DISTINCT b.subject FROM \"references\" a JOIN \"references\" b
                   ON b.source = 'musicbrainz' AND b.kind = 'release-group' AND lower(b.value) = lower(a.value) AND b.subject != a.subject
                 WHERE a.subject = ?1 AND a.source = 'musicbrainz' AND a.kind = 'release-group'",
            )?;
            for subject in st.query_map([ids::record(record)], |r| r.get::<_, String>(0))? {
                if let Some(r) = ids::record_number(&subject?) {
                    out.insert(r);
                }
            }
            let title: Option<String> = db.prepare_cached("SELECT COALESCE(title, '') FROM records WHERE id = ?1")?.query_row([record], |r| r.get(0)).optional()?;
            let key = title.map(|t| title_key(&t)).unwrap_or_default();
            if !key.is_empty() {
                let mut st = db.prepare_cached(
                    "SELECT DISTINCT r.id, COALESCE(r.title, '') FROM record_artists a JOIN record_artists b ON b.artist = a.artist JOIN records r ON r.id = b.record
                     WHERE a.record = ?1 AND b.record != ?1 AND r.merged_into IS NULL AND r.retired_at IS NULL",
                )?;
                for row in st.query_map([record], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
                    let (id, title) = row?;
                    if title_key(&title) == key {
                        out.insert(id);
                    }
                }
            }
            out.remove(&record);
            Ok(out.into_iter().collect())
        };
        match self.reader() {
            Some(c) => read(&c),
            None => read(&self.db()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::records::Ref;

    /// A had record of `title`, credited to `artist`, its tracks `titles` in order, each a file in catalogue folder `folder` tagged
    /// with its title.
    fn with_tracks(cat: &Catalogue, artist: i64, title: &str, titles: &[&str], folder: i64) -> i64 {
        let record = cat.record_for_tests(title, titles.len());
        let db = cat.db();
        db.execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'files')", params![record, artist]).unwrap();
        let tracks: Vec<i64> = {
            let mut st = db.prepare("SELECT id FROM tracks WHERE record = ?1 ORDER BY number").unwrap();
            st.query_map([record], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
        };
        for (track, name) in tracks.iter().zip(titles) {
            let rel = format!("{title}-{record}/{track}.flac");
            db.execute("INSERT INTO files (folder, rel, size, mtime, track) VALUES (?1, ?2, 1, 1, ?3)", params![folder, rel, track]).unwrap();
            db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (?1, ?2, 0, 'TITLE', 'title', ?3)", params![folder, rel, name]).unwrap();
        }
        record
    }

    fn records_count(cat: &Catalogue) -> i64 {
        cat.db().query_row("SELECT count(*) FROM records", [], |r| r.get(0)).unwrap()
    }

    /// F12.1 (the owner: "One card, 2 editions"): which records are editions of one album. (a) Music's untagged MP3 of ten tracks and
    /// Incoming's FLAC of the same ten and two more, holding the release group, are one album's, the main library's shown first, else
    /// the fullest; (d) a record not had of that group (a list's) is one of them, never a card of its own. (b) Two records of one
    /// release group with no track in common: one. (c) Two albums of one artist and one title with no track paired (X8): two. Two
    /// artists' albums of one title, their tracks paired: two. Two release groups that differ, their tracks paired: two. Never a join
    /// or a merge: every record answers as itself.
    #[test]
    fn editions_of_one_album_are_grouped_and_stay_two_records() {
        const G: &str = "00000000-0000-4000-8000-000000001201";
        const G2: &str = "00000000-0000-4000-8000-000000001202";
        const G3: &str = "00000000-0000-4000-8000-000000001203";
        const G4: &str = "00000000-0000-4000-8000-000000001204";
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let [artist, namesake] = ["Kite Theory", "Glass Harbour"].map(|n| cat.artist_for(n).unwrap());
        let group = |record: i64, g: &str| {
            cat.set_reference(&ids::record(record), &Ref::new("musicbrainz", "release-group", g), "files").unwrap();
        };
        let (music, incoming) = (1, 2);
        let ten: Vec<String> = (1..=10).map(|n| format!("Part {n}")).collect();
        let ten: Vec<&str> = ten.iter().map(String::as_str).collect();
        let twelve: Vec<&str> = ten.iter().copied().chain(["Part 10 (Remix)", "Reprise"]).collect();
        // (a), (d)
        let mp3 = with_tracks(&cat, artist, "Night Walks", &ten, music);
        let flac = with_tracks(&cat, artist, "Night Walks EP", &twelve, incoming);
        group(flac, G);
        let not_had = cat.record_for_tests("Night Walks", 0);
        cat.db().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'listing')", params![not_had, artist]).unwrap();
        group(not_had, G);
        // (b)
        let undertow = [with_tracks(&cat, artist, "Undertow", &["Low", "High"], incoming), with_tracks(&cat, artist, "Undertow (Live)", &["Shore", "Sea"], music)];
        group(undertow[0], G2);
        group(undertow[1], G2);
        // (c), X8: two albums of one name; two artists of one title; two release groups that differ.
        let orbital = [with_tracks(&cat, artist, "Orbital", &["One", "Two", "Three"], music), with_tracks(&cat, artist, "Orbital", &["Four", "Five", "Six"], music)];
        let tide = [with_tracks(&cat, artist, "Tide Tables", &ten, music), with_tracks(&cat, namesake, "Tide Tables", &ten, incoming)];
        let kites = [with_tracks(&cat, artist, "Paper Kites", &ten, music), with_tracks(&cat, artist, "Paper Kites", &ten, incoming)];
        group(kites[0], G3);
        group(kites[1], G4);
        let all: Vec<i64> = [mp3, flac, not_had].into_iter().chain(undertow).chain(orbital).chain(tide).chain(kites).collect();
        let before = records_count(&cat);
        assert_eq!(cat.edition_groups(&all, &[music], &[]).unwrap(), [vec![mp3, flac, not_had], vec![undertow[1], undertow[0]]], "the main library's first");
        assert_eq!(cat.edition_groups(&all, &[], &[]).unwrap(), [vec![flac, mp3, not_had], vec![undertow[0], undertow[1]]], "else the fullest, then the oldest");
        assert_eq!(cat.editions_near(mp3).unwrap(), [flac, not_had], "of its artist and its title's key");
        assert_eq!(cat.editions_near(not_had).unwrap(), [mp3, flac], "and of its release group");
        assert_eq!(records_count(&cat), before, "nothing made");
        for r in all {
            assert_eq!(cat.record(r).unwrap().unwrap().id, r, "al-{r} answers as itself: no merge");
        }
    }

    /// RG6 (PA6, the libraries switched on): the editions grouped are those the viewer has a library on for. Music's "Night Walks",
    /// Incoming's and bob's own copy of it, each a record of its own of one release group: one group of three for a viewer with every
    /// library on; with bob's own music off, the two others; with Incoming off too, none (one edition left).
    #[test]
    fn editions_follow_the_libraries_the_viewer_has_on() {
        const G: &str = "00000000-0000-4000-8000-000000001261";
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let artist = cat.artist_for("Kite Theory").unwrap();
        let (music, incoming, bobs) = (1, 2, 3);
        let ten: Vec<String> = (1..=10).map(|n| format!("Part {n}")).collect();
        let ten: Vec<&str> = ten.iter().map(String::as_str).collect();
        let records = [music, incoming, bobs].map(|folder| with_tracks(&cat, artist, "Night Walks", &ten, folder));
        for r in records {
            cat.set_reference(&ids::record(r), &Ref::new("musicbrainz", "release-group", G), "files").unwrap();
        }
        assert_eq!(cat.edition_groups(&records, &[music], &[]).unwrap(), [records.to_vec()], "every library on: three");
        assert_eq!(cat.edition_groups(&records, &[music], &[bobs]).unwrap(), [records[..2].to_vec()], "bob's own music off: his copy is no edition shown");
        assert_eq!(cat.edition_groups(&records, &[music], &[bobs, incoming]).unwrap(), Vec::<Vec<i64>>::new(), "one edition left: no group");
    }
}
