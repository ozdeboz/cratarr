//! Beyond (decision 0076, C57): MusicBrainz's albums and artists and Discogs' masters for the words, as search shows them past
//! what Cratarr knows. Its results are a cache, not records (Q33, OQ12 (a)): kept a few minutes per query in memory
//! (`discover::musicbrainz_found`, `discogs::search_masters`), and nothing is made when it answers.
//!
//! - **No name merge.** A MusicBrainz row and a Discogs row are one only where a record holds both the release group and the
//!   master; two albums or two artists of one name from the sources stay two.
//! - **Here by reference alone.** An album row names the record that holds its release group or its master (`record`), as merged
//!   now, and is had by the one had rule on that record. An artist row names the artist who holds their MusicBrainz id
//!   (`artist`), had where the library has albums by them. A name alone names no record.
//! - **Had through its names, for display alone (Z6).** A row no record holds is had where the had rule's names clause would hold
//!   it: a live had record of its folded artist and title, holding no reference that contradicts the row's, the name one album's
//!   (Y1). It names no record, and nothing is made or tied.
//! - **Made at open:** an album through `POST /api/records/open` (C56, `labels::open_record`), an artist through
//!   `POST /api/artists/open` (`open_asking`), never by a name, but for the one live artist of the name who holds no MusicBrainz
//!   id (Z2) where the id is MusicBrainz's first sure match for the name, asked at open as the monthly read asks it (T6).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;

use super::discogs::FoundMaster;
use super::discover::{FoundAlbum, FoundArtist};
use super::requests::{Mark, Marks};
use crate::catalogue::Catalogue;
use crate::catalogue::records::{ByReference, Ref};
use crate::serve::ids;

/// An album Beyond lists (C57).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BeyondAlbum {
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub cover: Option<String>,
    /// Its MusicBrainz release group, and Discogs' master with its page there (shown with "Data from Discogs"): what
    /// `POST /api/records/open` takes.
    pub release_group: Option<String>,
    pub discogs: Option<u64>,
    pub discogs_url: Option<String>,
    /// Where it was found: `musicbrainz`, `discogs`, or both where a record ties them.
    pub sources: Vec<&'static str>,
    /// The record that holds its reference (`al-<n>`), as merged now; none until it is opened.
    pub record: Option<String>,
    /// Whether the library has it: the one had rule on its record.
    pub have: bool,
    /// An open request's mark: its record's, else by its release group or names as a list with no record finds one.
    pub request: Option<Mark>,
}

/// An artist Beyond lists (C57): MusicBrainz's, by their MusicBrainz id.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BeyondArtist {
    pub mbid: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disambiguation: Option<String>,
    /// The artist who holds that MusicBrainz id (`ar-<n>`); none until they are opened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    /// That artist's number for their name (C55, G5): "Name (n)" where the list holds two of a name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<i64>,
    /// Whether the library has albums by that artist.
    pub have: bool,
}

/// What the catalogue says of Beyond's rows (`known`): each MusicBrainz album's record and each Discogs master's, in their order,
/// those records read, and each artist's id and whether the library has albums by them.
#[derive(Debug, Clone, Default)]
pub struct Known {
    pub groups: Vec<Option<i64>>,
    pub masters: Vec<Option<i64>>,
    /// Each MusicBrainz album's and each Discogs master's had record through its names, where no record holds it (Z6).
    pub groups_named: Vec<Option<i64>>,
    pub masters_named: Vec<Option<i64>>,
    pub here: ByReference,
    /// The artists found's numbers for their names (G5), by artist.
    pub numbers: HashMap<i64, i64>,
}

/// A MusicBrainz id as MusicBrainz writes it.
fn mbid(id: &str) -> String {
    id.trim().to_ascii_lowercase()
}

/// What the catalogue holds of what Beyond found (C57), by reference alone (`Catalogue::by_reference`), on one reader: nothing is
/// made. Blocking.
pub fn known(cat: &Catalogue, albums: &[FoundAlbum], masters: &[FoundMaster], artists: &[FoundArtist]) -> Result<Known> {
    let refs: Vec<(Ref, &str, &str)> = albums
        .iter()
        .map(|a| (Ref::new("musicbrainz", "release-group", &mbid(&a.release_group)), a.artist.as_str(), a.title.as_str()))
        .chain(masters.iter().map(|d| (Ref::new("discogs", "master", &d.id.to_string()), d.artist.as_str(), d.title.as_str())))
        .collect();
    let people: Vec<Ref> = artists.iter().map(|a| Ref::new("musicbrainz", "artist", &mbid(&a.mbid))).collect();
    let mut here = cat.by_reference(&refs, &people)?;
    let masters = here.records.split_off(albums.len().min(here.records.len()));
    let groups = std::mem::take(&mut here.records);
    let masters_named = here.named.split_off(albums.len().min(here.named.len()));
    let groups_named = std::mem::take(&mut here.named);
    let numbers = cat.artist_numbers(&here.artists.iter().flatten().map(|(id, _)| *id).collect::<Vec<_>>())?;
    Ok(Known { groups, masters, groups_named, masters_named, here, numbers })
}

impl Known {
    /// Whether a record holds this reference.
    fn holds(&self, record: Option<i64>, source: &str, kind: &str, value: &str) -> bool {
        record.and_then(|r| self.here.read.get(&r)).is_some_and(|q| q.refs.iter().any(|(s, k, v)| s == source && k == kind && v.eq_ignore_ascii_case(value)))
    }

    /// Whether a record is had: the one had rule's holder.
    fn had(&self, record: Option<i64>) -> bool {
        record.and_then(|r| self.here.read.get(&r)).is_some_and(|q| q.holder.is_some())
    }

    /// Whether a row is had: its record by the one had rule, else, where no record holds it, through its names (Z6).
    fn row_had(&self, record: Option<i64>, named: &[Option<i64>], i: usize) -> bool {
        match record {
            Some(_) => self.had(record),
            None => named.get(i).copied().flatten().is_some(),
        }
    }
}

/// An album row's request: its record's mark, else as a list with no record id finds one (`requests::mark_of`).
fn request_of(marks: Option<&Marks>, record: Option<i64>, release_group: Option<&str>, artist: &str, title: &str) -> Option<Mark> {
    let marks = marks?;
    match record {
        Some(r) => marks.of_record(r),
        None => super::requests::mark_of(marks, release_group, artist, title),
    }
}

/// Beyond's album rows (C57), MusicBrainz's first, then Discogs' masters, each album once by reference alone: a master goes on a
/// MusicBrainz row only where a record holds both its master and the row's release group (or both are one record), and rows
/// whose records are one record are one row. Never by a name: two albums of one name from two sources are two rows. Each names
/// its record and is had by the one had rule on it; a row no record holds is not had. Pure.
pub fn albums(mb: &[FoundAlbum], discogs: &[FoundMaster], known: &Known, marks: Option<&Marks>) -> Vec<BeyondAlbum> {
    let mut rows: Vec<BeyondAlbum> = Vec::new();
    // Each row's record, and the row each record is on.
    let mut records: Vec<Option<i64>> = Vec::new();
    let mut on: HashMap<i64, usize> = HashMap::new();
    let mut groups: HashSet<String> = HashSet::new();
    for (i, a) in mb.iter().enumerate() {
        let record = known.groups.get(i).copied().flatten();
        if !groups.insert(mbid(&a.release_group)) || record.is_some_and(|r| on.contains_key(&r)) {
            continue;
        }
        if let Some(r) = record {
            on.insert(r, rows.len());
        }
        records.push(record);
        rows.push(BeyondAlbum {
            title: a.title.clone(),
            artist: a.artist.clone(),
            year: a.year,
            cover: Some(a.cover.clone()).filter(|c| !c.is_empty()),
            release_group: Some(a.release_group.clone()),
            discogs: None,
            discogs_url: None,
            sources: vec!["musicbrainz"],
            record: record.map(ids::record),
            have: known.row_had(record, &known.groups_named, i),
            request: request_of(marks, record, Some(a.release_group.as_str()), &a.artist, &a.title),
        });
    }
    let mut masters: HashSet<u64> = HashSet::new();
    for (i, d) in discogs.iter().enumerate() {
        if !masters.insert(d.id) {
            continue;
        }
        let record = known.masters.get(i).copied().flatten();
        let master = d.id.to_string();
        let tied = match record.and_then(|r| on.get(&r).copied()) {
            Some(at) => Some(at),
            None => (0..rows.len()).find(|&j| {
                rows[j].discogs.is_none()
                    && rows[j].release_group.as_deref().is_some_and(|g| known.holds(records[j], "discogs", "master", &master) || known.holds(record, "musicbrainz", "release-group", g))
            }),
        };
        if let Some(at) = tied {
            // The album a MusicBrainz row is, or another master of a record listed already: one album once.
            let row = &mut rows[at];
            if row.discogs.is_none() {
                row.discogs = Some(d.id);
                row.discogs_url = Some(d.url.clone());
                if !row.sources.contains(&"discogs") {
                    row.sources.push("discogs");
                }
            }
            continue;
        }
        if let Some(r) = record {
            on.insert(r, rows.len());
        }
        records.push(record);
        rows.push(BeyondAlbum {
            title: d.title.clone(),
            artist: d.artist.clone(),
            year: d.year,
            cover: d.cover.clone(),
            release_group: None,
            discogs: Some(d.id),
            discogs_url: Some(d.url.clone()),
            sources: vec!["discogs"],
            record: record.map(ids::record),
            have: known.row_had(record, &known.masters_named, i),
            request: request_of(marks, record, None, &d.artist, &d.title),
        });
    }
    rows
}

/// Beyond's artist rows (C57), as MusicBrainz lists them, each by their MusicBrainz id: two of one name are two. Each names the
/// artist who holds that id, had where the library has albums by them; never by a name. Pure.
pub fn artists(found: &[FoundArtist], known: &Known) -> Vec<BeyondArtist> {
    found
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let here = known.here.artists.get(i).copied().flatten();
            BeyondArtist {
                mbid: a.mbid.clone(),
                name: a.name.clone(),
                disambiguation: a.disambiguation.clone(),
                artist: here.map(|(id, _)| ids::artist(id)),
                number: here.and_then(|(id, _)| known.numbers.get(&id).copied()),
                have: here.is_some_and(|(_, had)| had),
            }
        })
        .collect()
}

/// An artist of Beyond's opened (C57, OQ12 (a): kept when opened): the artist who holds their MusicBrainz id; else, where the id
/// is MusicBrainz's first sure match for the name (`sure`, T6: `open_asking` asks it), the one live artist of the name who holds
/// none (Z2); else one of their own made now, though another has the name (C55: the name's next number). The one opened holds it
/// (`Catalogue::artist_by_reference`). Never by the name alone: without a MusicBrainz id, or a name, none. Blocking.
pub fn open_artist(cat: &Catalogue, id: &str, name: &str, sure: bool) -> Result<Option<i64>> {
    let Some((id, name)) = opening(id, name)? else { return Ok(None) };
    Ok(Some(cat.artist_by_reference(&Ref::new("musicbrainz", "artist", &id), &name, "search", sure)?))
}

/// The MusicBrainz id and the name an artist is opened by, as MusicBrainz writes the id and the name trimmed: none without an id or
/// a name; a name longer than an artist's refused.
fn opening(id: &str, name: &str) -> Result<Option<(String, String)>> {
    let (id, name) = (mbid(id), name.trim());
    if !super::discogs::is_mbid(&id) || name.is_empty() {
        return Ok(None);
    }
    if name.chars().count() > super::labels::NAME_MOST {
        anyhow::bail!("A name that long is no artist's.");
    }
    Ok(Some((id, name.to_string())))
}

/// How long the open waits for MusicBrainz's answer (T6): past it, as when MusicBrainz is away, the id is not the name's sure match.
const ASKED_FOR: Duration = Duration::from_secs(10);

/// An artist opened (C57; T6, the review of run 10's finding 6): `open_artist`, the id taken as the name's sure match only where it
/// is MusicBrainz's first sure match for the name, asked as the monthly read asks it (`discover::mb_artist_by_name`,
/// `artist:"<name>"`): one question, whoever opens and whatever listed them (Beyond, a card, an administrator's name). MusicBrainz
/// switched off (no question asked), away, or not answering within `ASKED_FOR`: not sure, so a new artist holds the id, which the
/// monthly read gives the library's artist later where the name says so (OQ-K, T7). Nothing asked for an open that opens none.
pub async fn open_asking(cat: Arc<Catalogue>, id: &str, name: &str) -> Result<Option<i64>> {
    let Some((id, name)) = opening(id, name)? else { return Ok(None) };
    let sure = sure_of(&id, &name).await;
    tokio::task::spawn_blocking(move || open_artist(&cat, &id, &name, sure)).await?
}

/// Whether a MusicBrainz id is its first sure match for a name (T6): its answer to the name's one question. Any failure, or none in
/// time: not sure, said in the log.
async fn sure_of(id: &str, name: &str) -> bool {
    let asked = async { super::discover::mb_artist_by_name(&super::discover::http()?, name).await };
    match tokio::time::timeout(ASKED_FOR, asked).await {
        Ok(Ok(found)) => found.is_some_and(|f| mbid(&f) == id),
        Ok(Err(e)) => {
            tracing::info!(target: "beyond", "{name} opened as not sure: MusicBrainz not asked who they are: {e:#}");
            false
        }
        Err(_) => {
            tracing::info!(target: "beyond", "{name} opened as not sure: MusicBrainz did not answer in time");
            false
        }
    }
}

/// How long an artist Beyond listed may be opened by anyone (Z8): a day from when it was listed, so a page left open still opens it.
pub const LISTED_FOR: i64 = 86_400;

/// A MusicBrainz artist Beyond listed (Z8, U6): the name it listed them by, and when.
#[derive(Debug, Clone)]
struct Listed {
    name: String,
    at: i64,
}

/// The MusicBrainz artists Beyond listed, by id (Z8).
fn listed_ids() -> &'static std::sync::Mutex<HashMap<String, Listed>> {
    static LISTED: std::sync::OnceLock<std::sync::Mutex<HashMap<String, Listed>>> = std::sync::OnceLock::new();
    LISTED.get_or_init(Default::default)
}

/// Beyond's artists as it answers them at `now` (Z8, U6): each may be opened by anyone for `LISTED_FOR`, by the name it was listed
/// by (`may_open`). Whether the id is the name's sure match is asked at open (T6, `open_asking`), never read from this answer to
/// other words. Those listed longer ago are let go of.
pub fn keep_listed(found: &[FoundArtist], now: i64) {
    let mut all = listed_ids().lock().unwrap_or_else(|p| p.into_inner());
    all.retain(|_, l| now - l.at <= LISTED_FOR);
    for a in found {
        all.insert(mbid(&a.mbid), Listed { name: a.name.clone(), at: now });
    }
}

/// Artists Discover's cards name by their MusicBrainz id (G7), as Beyond lists its own: each may be opened by anyone for
/// `LISTED_FOR`, by the name the card gives, and is the name's sure match only as every open asks it (T6, `open_asking`).
pub fn keep_carded(cards: &[(String, String)], now: i64) {
    let mut all = listed_ids().lock().unwrap_or_else(|p| p.into_inner());
    for (id, name) in cards {
        let id = mbid(id);
        if super::discogs::is_mbid(&id) && !name.trim().is_empty() {
            all.entry(id).and_modify(|l| l.at = l.at.max(now)).or_insert(Listed { name: name.trim().to_string(), at: now });
        }
    }
}

/// Whether an artist may be opened, and by which name (Z8, the review's finding 8: random ids used up a name's numbers for good;
/// U6): by anyone where Beyond listed them within `LISTED_FOR`, by the name it listed them by (another name is refused: one
/// account could give a library artist an unrelated id); by an administrator (`administering`), any, by the name given. The name
/// to open by (`open_asking`, which asks whether the id is its sure match); none: refused.
pub fn may_open(id: &str, name: &str, administering: bool, now: i64) -> Option<String> {
    let listed = listed_ids().lock().unwrap_or_else(|p| p.into_inner()).get(&mbid(id)).filter(|l| now - l.at <= LISTED_FOR).cloned();
    match listed {
        Some(l) if ids::artist_key(name.trim()) == ids::artist_key(l.name.trim()) => Some(l.name),
        _ if administering => Some(name.trim().to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROUP_1: &str = "00000000-0000-4000-8000-000000000571";
    const GROUP_2: &str = "00000000-0000-4000-8000-000000000572";
    const GROUP_3: &str = "00000000-0000-4000-8000-000000000573";
    const ARTIST_1: &str = "00000000-0000-4000-8000-000000000581";
    const ARTIST_2: &str = "00000000-0000-4000-8000-000000000582";
    const ARTIST_3: &str = "00000000-0000-4000-8000-000000000583";
    const ARTIST_4: &str = "00000000-0000-4000-8000-000000000584";

    fn album(group: &str, title: &str, artist: &str) -> FoundAlbum {
        FoundAlbum { release_group: group.into(), title: title.into(), artist: artist.into(), year: Some(2020), kind: Some("Album".into()), cover: format!("/api/covers/release-group/{group}"), have: false }
    }

    fn master(id: u64, title: &str, artist: &str) -> FoundMaster {
        FoundMaster { id, title: title.into(), artist: artist.into(), year: Some(2020), cover: None, url: format!("https://www.discogs.com/master/{id}") }
    }

    fn person(id: &str, name: &str) -> FoundArtist {
        FoundArtist { mbid: id.into(), name: name.into(), disambiguation: None, have: false }
    }

    /// T6's fake MusicBrainz (an axum router, as `lidarr.rs`'s tests make one), this thread's MusicBrainz while the guard lives
    /// (`discover::FakeMusicBrainz`). It answers `GET /ws/2/artist` only where the query is exactly `artist:"<name>"` of a name it
    /// knows (its quotes and backslashes escaped), `fmt=json` and `limit=3`, nothing else, and a User-Agent is sent: each name its
    /// status and body (a 503 says to wait a second). Anything else, Beyond's unfielded words, another limit or another path, is
    /// refused with 400. Every request is counted.
    async fn fake_musicbrainz(answers: Vec<(&'static str, u16, serde_json::Value)>) -> (crate::app::discover::FakeMusicBrainz, Arc<std::sync::atomic::AtomicUsize>) {
        use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
        use axum::response::IntoResponse;
        let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = asked.clone();
        let answer = move |method: Method, uri: Uri, headers: HeaderMap| {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let pairs: Vec<(String, String)> = reqwest::Url::parse(&format!("http://fake{uri}")).unwrap().query_pairs().into_owned().collect();
            let field = |k: &str| pairs.iter().filter(|(p, _)| p == k).map(|(_, v)| v.as_str()).collect::<Vec<_>>();
            let fielded = |name: &str| format!("artist:\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""));
            let known = answers.iter().find(|(name, _, _)| field("query") == [fielded(name).as_str()]);
            let asked_right = method == Method::GET
                && uri.path() == "/ws/2/artist"
                && pairs.len() == 3
                && field("fmt") == ["json"]
                && field("limit") == ["3"]
                && headers.get(header::USER_AGENT).is_some_and(|a| a.as_bytes() == super::super::reach::USER_AGENT.as_bytes());
            let reply = match known {
                Some((_, status, body)) if asked_right => {
                    let status = StatusCode::from_u16(*status).unwrap();
                    let mut wait = HeaderMap::new();
                    if status == StatusCode::SERVICE_UNAVAILABLE {
                        wait.insert(header::RETRY_AFTER, axum::http::HeaderValue::from_static("1"));
                    }
                    (status, wait, axum::Json(body.clone())).into_response()
                }
                _ => (StatusCode::BAD_REQUEST, "not a question this fake knows").into_response(),
            };
            std::future::ready(reply)
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let at = format!("http://{}/ws/2", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, axum::Router::new().fallback(answer)).await.unwrap() });
        (crate::app::discover::FakeMusicBrainz::at(at), asked)
    }

    /// An artist as MusicBrainz's search answers them, with its score.
    fn found(id: &str, name: &str, score: u64) -> serde_json::Value {
        serde_json::json!({ "id": id, "name": name, "score": score })
    }

    /// The MusicBrainz ids an artist holds, and how each was found.
    fn holds(cat: &Catalogue, artist: i64) -> Vec<(String, String)> {
        cat.references_of(&ids::artist(artist)).unwrap().into_iter().map(|(r, row)| (r.value, row.how)).collect()
    }

    /// The records an artist is credited on.
    fn credits(cat: &Catalogue, artist: i64) -> Vec<i64> {
        let db = cat.db_for_tests();
        let mut st = db.prepare("SELECT record FROM record_artists WHERE artist = ?1 ORDER BY record").unwrap();
        st.query_map([artist], |r| r.get::<_, i64>(0)).unwrap().map(Result::unwrap).collect()
    }

    /// The library's "Grouch", untagged (no MusicBrainz id), credited on a had record by their files: the artist and the record.
    fn library_grouch(cat: &Catalogue) -> (i64, i64) {
        let grouch = cat.artist_for("Grouch").unwrap();
        let had = cat.record_for_tests("Fuzzy Logic", 2);
        cat.db_for_tests().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'files')", rusqlite::params![had, grouch]).unwrap();
        (grouch, had)
    }

    /// A record of this title by Kite Theory holding these references; had with `tracks`.
    fn record(cat: &Catalogue, title: &str, tracks: usize, refs: &[(&str, &str, &str)]) -> i64 {
        let record = cat.record_for_tests(title, tracks);
        cat.db_for_tests().execute("UPDATE records SET artist = 'Kite Theory' WHERE id = ?1", [record]).unwrap();
        for (source, kind, value) in refs {
            cat.set_reference(&ids::record(record), &Ref::new(source, kind, value), "link").unwrap();
        }
        record
    }

    fn rows(cat: &Catalogue, mb: &[FoundAlbum], discogs: &[FoundMaster]) -> Vec<BeyondAlbum> {
        albums(mb, discogs, &known(cat, mb, discogs, &[]).unwrap(), None)
    }

    /// C57: an album MusicBrainz and Discogs both find by one name is two rows, unless a record holds both its release group and
    /// its master; two release groups of one name are two rows; two artists of one name are two.
    #[test]
    fn beyond_never_merges_by_name() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let mb = [album(GROUP_1, "Night Walks", "Kite Theory"), album(GROUP_2, "Night Walks", "Kite Theory")];
        let discogs = [master(3001, "Night Walks", "Kite Theory")];
        let sources = |rows: &[BeyondAlbum]| rows.iter().map(|r| (r.release_group.clone(), r.discogs, r.sources.clone())).collect::<Vec<_>>();
        assert_eq!(
            sources(&rows(&cat, &mb, &discogs)),
            [(Some(GROUP_1.to_string()), None, vec!["musicbrainz"]), (Some(GROUP_2.to_string()), None, vec!["musicbrainz"]), (None, Some(3001), vec!["discogs"])],
            "no reference ties them"
        );
        // A record of the master alone, and another of the release group: still two.
        record(&cat, "Night Walks", 0, &[("discogs", "master", "3001")]);
        let walks = record(&cat, "Night Walks", 0, &[("musicbrainz", "release-group", GROUP_2)]);
        assert_eq!(rows(&cat, &mb, &discogs).len(), 3, "two records, two albums");
        // The release group's record holds the master too: one row, found in both.
        cat.set_reference(&ids::record(walks), &Ref::new("discogs", "master", "3001"), "link").unwrap();
        assert_eq!(
            sources(&rows(&cat, &mb, &discogs)),
            [(Some(GROUP_1.to_string()), None, vec!["musicbrainz"]), (Some(GROUP_2.to_string()), Some(3001), vec!["musicbrainz", "discogs"])],
            "a record ties them"
        );
        let people = [person(ARTIST_1, "Rone"), person(ARTIST_2, "Rone")];
        let listed = artists(&people, &known(&cat, &[], &[], &people).unwrap());
        assert_eq!(listed.iter().map(|a| a.mbid.as_str()).collect::<Vec<_>>(), [ARTIST_1, ARTIST_2], "two of one name are two");
    }

    /// C57: an album row names the record that holds its release group or its master, as merged now, and is had by the one had
    /// rule on it; a row of a had album's names that no reference ties to it names none, and is had through its names alone
    /// (Z6: the untagged Lamplight); artists by their MusicBrainz id alone.
    #[test]
    fn a_beyond_row_names_its_record_by_reference_only() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let had = record(&cat, "Night Walks", 2, &[("musicbrainz", "release-group", GROUP_1)]);
        let wanted = record(&cat, "Paper Kites", 0, &[("discogs", "master", "3002")]);
        let merged = record(&cat, "Undertow", 0, &[("musicbrainz", "release-group", GROUP_2)]);
        let into = record(&cat, "Undertow (Deluxe)", 0, &[]);
        cat.merge_records(merged, into).unwrap();
        // Had by its names, untagged: Beyond's rows of those names name no record, and are had through their names (Z6).
        record(&cat, "Lamplight", 2, &[]);
        let mb = [album(&GROUP_1.to_uppercase(), "Night Walks", "Kite Theory"), album(GROUP_2, "Undertow", "Kite Theory"), album(GROUP_3, "Lamplight", "Kite Theory")];
        let discogs = [master(3002, "Paper Kites", "Kite Theory"), master(3003, "Lamplight", "Kite Theory")];
        let listed = rows(&cat, &mb, &discogs);
        let named: Vec<(&str, Option<String>, bool)> = listed.iter().map(|r| (r.title.as_str(), r.record.clone(), r.have)).collect();
        assert_eq!(
            named,
            [
                ("Night Walks", Some(ids::record(had)), true),
                ("Undertow", Some(ids::record(into)), false),
                ("Lamplight", None, true),
                ("Paper Kites", Some(ids::record(wanted)), false),
                ("Lamplight", None, true),
            ],
            "by reference alone, merges followed; a name alone is no record, and had through its names"
        );
        // An artist by their MusicBrainz id: had where the library has albums by them; one of the same name with no id is not them.
        let theirs = cat.artist_for("Kite Theory").unwrap();
        cat.db_for_tests().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'files')", rusqlite::params![had, theirs]).unwrap();
        let people = [person(ARTIST_1, "Kite Theory")];
        assert_eq!(artists(&people, &known(&cat, &[], &[], &people).unwrap())[0].artist, None, "the library's Kite Theory holds no MusicBrainz id");
        cat.set_reference(&ids::artist(theirs), &Ref::new("musicbrainz", "artist", ARTIST_1), "files").unwrap();
        let listed = artists(&people, &known(&cat, &[], &[], &people).unwrap());
        assert_eq!((listed[0].artist.clone(), listed[0].have), (Some(ids::artist(theirs)), true));
    }

    /// C57 (OQ12 (a)): an artist of Beyond's opened is the artist who holds their MusicBrainz id, else one made then with it, never
    /// one of the same name that holds another id, nor one of two of the name (Z2): a second of a name is numbered 2; the same id
    /// opened again is the same artist.
    #[test]
    fn opening_a_beyond_artist_never_merges_by_name() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let first = open_artist(&cat, ARTIST_1, "Rone", true).unwrap().expect("made");
        assert_eq!(open_artist(&cat, &ARTIST_1.to_uppercase(), "Rone", true).unwrap(), Some(first), "the same id: the same artist");
        let second = open_artist(&cat, ARTIST_2, "Rone", true).unwrap().expect("made");
        assert_ne!(first, second, "another id of the same name: another artist");
        let number = |artist: i64| cat.artist_named(artist).unwrap().unwrap().number;
        assert_eq!((number(first), number(second)), (Some(1), Some(2)));
        // Two artists of a name with no MusicBrainz id: which one, nothing says, so neither (Z2).
        let (theirs, twin) = (cat.artist_for("Kite Theory").unwrap(), cat.artist_apart("Kite Theory").unwrap());
        let opened = open_artist(&cat, ARTIST_3, "Kite Theory", true).unwrap().expect("made");
        assert!(opened != theirs && opened != twin);
        assert_eq!(number(opened), Some(3));
        // One who holds the id by their files is the one opened.
        let harbour = cat.artist_for("Glass Harbour").unwrap();
        cat.set_reference(&ids::artist(harbour), &Ref::new("musicbrainz", "artist", ARTIST_4), "files").unwrap();
        assert_eq!(open_artist(&cat, ARTIST_4, "Glass Harbour", false).unwrap(), Some(harbour));
        // Never by the name alone.
        assert_eq!(open_artist(&cat, "", "Rone", true).unwrap(), None);
        assert_eq!(open_artist(&cat, "not an id", "Rone", true).unwrap(), None);
        let known_by = |artist: i64| cat.references_of(&ids::artist(artist)).unwrap().into_iter().map(|(r, row)| (r.value, row.how)).collect::<Vec<_>>();
        assert_eq!(known_by(second), [(ARTIST_2.to_string(), "search".to_string())], "their MusicBrainz id kept");
    }

    /// Z2 (the review's finding 2): with no artist holding the id opened, the one live artist of its name who holds no MusicBrainz
    /// artist id is them, given it as `search`, as the monthly read finds them by their name: not a second of the name. One holding
    /// another id, or two of the name, are not: a new artist.
    #[test]
    fn a_beyond_artist_is_the_library_artist_of_its_name_with_no_id() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let grouch = cat.artist_for("Grouch").unwrap();
        assert_eq!(open_artist(&cat, ARTIST_1, "grouch", true).unwrap(), Some(grouch), "the library's Grouch, not Grouch (2)");
        let holds = |artist: i64| cat.references_of(&ids::artist(artist)).unwrap().into_iter().map(|(r, row)| (r.value, row.how)).collect::<Vec<_>>();
        assert_eq!(holds(grouch), [(ARTIST_1.to_string(), "search".to_string())], "given the id, as the monthly read would");
        let number = |artist: i64| cat.artist_named(artist).unwrap().unwrap().number;
        assert_eq!(number(grouch), Some(1));
        assert_eq!(open_artist(&cat, ARTIST_1, "Grouch", true).unwrap(), Some(grouch), "held: the same artist");
        let other = open_artist(&cat, ARTIST_2, "Grouch", true).unwrap().expect("made");
        assert_ne!(other, grouch, "the one of the name holds another id: another artist");
        assert_eq!(number(other), Some(2));
        let (club, twin) = (cat.artist_for("Tape Club").unwrap(), cat.artist_apart("Tape Club").unwrap());
        let made = open_artist(&cat, ARTIST_3, "Tape Club", true).unwrap().expect("made");
        assert!(made != club && made != twin, "two of the name with no id: neither");
        assert_eq!(number(made), Some(3));
    }

    /// Z2: the files' MusicBrainz artist id takes it from an artist who holds it by `search` alone and is credited on no had record
    /// (one opened from Beyond while the name was two artists'); never from one credited on a had record, nor from one holding it
    /// by their files or by hand.
    #[test]
    fn a_files_artist_id_takes_it_from_a_beyond_artist() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (grouch, twin) = (cat.artist_for("Grouch").unwrap(), cat.artist_apart("Grouch").unwrap());
        let beyond = open_artist(&cat, ARTIST_1, "Grouch", true).unwrap().expect("made");
        assert!(beyond != grouch && beyond != twin);
        let files = |artist: i64, id: &str| cat.set_reference(&ids::artist(artist), &Ref::new("musicbrainz", "artist", id), "files").unwrap();
        let holds = |artist: i64| cat.references_of(&ids::artist(artist)).unwrap().into_iter().map(|(r, row)| (r.value, row.how)).collect::<Vec<_>>();
        assert!(files(grouch, ARTIST_1), "taken from the Beyond artist");
        assert_eq!(holds(grouch), [(ARTIST_1.to_string(), "files".to_string())]);
        assert!(holds(beyond).is_empty(), "let go of");
        // Credited on a had record: kept.
        let credited = open_artist(&cat, ARTIST_2, "Grouch", true).unwrap().expect("made");
        let album = record(&cat, "Night Walks", 2, &[]);
        cat.db_for_tests().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'listing')", rusqlite::params![album, credited]).unwrap();
        assert!(!files(twin, ARTIST_2), "credited on a had record: kept");
        assert_eq!(holds(credited), [(ARTIST_2.to_string(), "search".to_string())]);
        // Held by the files, or by hand: kept.
        assert!(!files(twin, ARTIST_1), "held by the files: kept");
        cat.set_reference(&ids::artist(beyond), &Ref::new("musicbrainz", "artist", ARTIST_3), "hand").unwrap();
        assert!(!files(twin, ARTIST_3), "held by hand: kept");
    }

    /// U4 (the review of 8c/8d, finding 4): the files' artist id taken from a Beyond artist credited on no had record brings the
    /// records that artist's list made: they are the taker's, so their page and Discover list them, one the taker is credited on
    /// already once. The Beyond artist is left with no credit, noted bare, and goes a day later (Z10).
    #[test]
    fn a_files_takeover_brings_the_beyond_artists_records() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (grouch, twin) = (cat.artist_for("Grouch").unwrap(), cat.artist_apart("Grouch").unwrap());
        let beyond = open_artist(&cat, ARTIST_1, "Grouch", true).unwrap().expect("made");
        assert!(beyond != grouch && beyond != twin);
        let credit = |record: i64, artist: i64| {
            cat.db_for_tests().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'listing')", rusqlite::params![record, artist]).unwrap();
        };
        let listed = [record(&cat, "Night Walks", 0, &[("musicbrainz", "release-group", GROUP_1)]), record(&cat, "Signal", 0, &[])];
        let both = record(&cat, "Lamplight", 0, &[]);
        for r in listed.iter().chain([&both]) {
            credit(*r, beyond);
        }
        credit(both, grouch);
        let credits = |artist: i64| {
            let db = cat.db_for_tests();
            let mut st = db.prepare("SELECT record FROM record_artists WHERE artist = ?1 ORDER BY record").unwrap();
            st.query_map([artist], |r| r.get::<_, i64>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()
        };
        assert!(cat.set_reference(&ids::artist(grouch), &Ref::new("musicbrainz", "artist", ARTIST_1), "files").unwrap(), "taken");
        assert_eq!(credits(grouch), [listed[0], listed[1], both], "the Beyond artist's records are the taker's");
        assert!(credits(beyond).is_empty(), "left with nothing");
        let bare: i64 = cat.db_for_tests().query_row("SELECT count(*) FROM artists_bare WHERE artist = ?1", [beyond], |r| r.get(0)).unwrap();
        assert_eq!(bare, 1, "noted bare: forgotten a day later");
    }

    /// Z6 (the review's finding 6): a row no record holds is had where the had rule's names clause would hold it, a live had record
    /// of its folded artist and title holding no reference that contradicts the row's; it names no record, and nothing is made or
    /// tied. Not where that record holds another release group, nor where the name is two albums' (Y1), nor for another artist.
    #[test]
    fn a_beyond_row_of_an_untagged_owned_album_is_had_and_names_no_record() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        record(&cat, "Lamplight", 2, &[]);
        record(&cat, "Signal", 2, &[("musicbrainz", "release-group", GROUP_2)]);
        record(&cat, "Two Rooms", 2, &[]);
        record(&cat, "Two Rooms", 0, &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000574")]);
        let count = |table: &str| cat.db_for_tests().query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| r.get::<_, i64>(0)).unwrap();
        let before = [count("records"), count("references"), count("record_labels"), count("record_artists")];
        let mb = [
            album(GROUP_1, "Lamplight", "Kite Theory"),
            album(GROUP_3, "Signal", "Kite Theory"),
            album("00000000-0000-4000-8000-000000000575", "Two Rooms", "Kite Theory"),
            album("00000000-0000-4000-8000-000000000576", "Lamplight", "Tape Club"),
        ];
        let discogs = [master(3005, "LAMPLIGHT", "kite theory")];
        let listed = rows(&cat, &mb, &discogs);
        let had: Vec<(&str, &str, Option<String>, bool)> = listed.iter().map(|r| (r.title.as_str(), r.artist.as_str(), r.record.clone(), r.have)).collect();
        assert_eq!(
            had,
            [
                ("Lamplight", "Kite Theory", None, true),
                ("Signal", "Kite Theory", None, false),
                ("Two Rooms", "Kite Theory", None, false),
                ("Lamplight", "Tape Club", None, false),
                ("LAMPLIGHT", "kite theory", None, true),
            ],
            "had through its names alone, folded; another release group, a name of two albums and another artist are not"
        );
        assert_eq!(listed.len(), 5, "no merge by names: the two Lamplights of Kite Theory are two rows");
        assert_eq!([count("records"), count("references"), count("record_labels"), count("record_artists")], before, "nothing made or tied");
    }

    /// Z8 (the review's finding 8): an artist is opened by anyone only where Beyond listed them within the day; by an administrator,
    /// any. A name longer than an artist's is refused.
    #[test]
    fn an_artist_is_opened_only_as_beyond_listed_them() {
        const LISTED: &str = "00000000-0000-4000-8000-000000000591";
        const NEVER: &str = "00000000-0000-4000-8000-000000000592";
        let now = crate::app::moment::now_secs();
        assert!(may_open(LISTED, "Rone", false, now).is_none(), "not listed yet");
        keep_listed(&[person(LISTED, "Rone")], now);
        assert!(may_open(&LISTED.to_uppercase(), "Rone", false, now + 3600).is_some(), "listed: by anyone");
        assert!(may_open(LISTED, "Rone", false, now + LISTED_FOR + 1).is_none(), "a day on: no more");
        assert!(may_open(NEVER, "Rone", false, now).is_none(), "never listed");
        assert!(may_open(NEVER, "Rone", true, now).is_some(), "an administrator: any");
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        assert!(open_artist(&cat, LISTED, &"x".repeat(super::super::labels::NAME_MOST + 1), true).is_err(), "a name that long is no artist's");
        assert!(open_artist(&cat, LISTED, "Rone", true).unwrap().is_some());
    }

    /// U6 (the review of 8c/8d, finding 6): an artist Beyond listed is opened by the name it listed them by. Another name is refused
    /// to anyone but an administrator, whose name is taken as given; one the page sends in other letters is the listed name. Whether
    /// the id is the name's sure match is asked at open (T6).
    #[test]
    fn an_open_with_another_name_than_beyond_listed_is_refused() {
        const LISTED: &str = "00000000-0000-4000-8000-000000000593";
        let now = crate::app::moment::now_secs();
        keep_listed(&[person(LISTED, "Rone")], now);
        assert_eq!(may_open(LISTED, "Grouch", false, now), None, "another name: refused");
        assert_eq!(may_open(LISTED, " rone ", false, now), Some("Rone".to_string()), "the listed name");
        assert_eq!(may_open(LISTED, "Grouch", true, now), Some("Grouch".to_string()), "an administrator's, as given");
    }

    /// G7: an artist a Discover card names by id is opened by anyone by the card's name; whether the id is the name's sure match is
    /// asked at open, as for every open (T6).
    #[test]
    fn a_cards_artist_is_opened_by_anyone() {
        const CARDED: &str = "00000000-0000-4000-8000-000000000597";
        let now = crate::app::moment::now_secs();
        assert_eq!(may_open(CARDED, "Zero 7", false, now), None);
        keep_carded(&[(CARDED.to_uppercase(), "Zero 7".into()), ("not an id".into(), "Air".into())], now);
        assert_eq!(may_open(CARDED, "Zero 7", false, now), Some("Zero 7".to_string()));
        assert_eq!(may_open(CARDED, "Air", false, now), None, "by the card's name alone");
    }

    /// U6, with T6: a library artist of the name with no MusicBrainz id is given the id only where it is MusicBrainz's first sure
    /// match for the name, asked at open (`open_asking`) as the monthly read asks it. Opening a second namesake on purpose (the
    /// rapper, beside the library's local band) makes a new artist, and so does a match MusicBrainz is not sure of; the first sure
    /// one is the library's.
    #[tokio::test]
    async fn a_beyond_open_of_a_second_namesake_makes_a_new_artist() {
        use crate::app::switches::{MUSICBRAINZ, TEST_STATE, set_for_tests};
        const FIRST: &str = "00000000-0000-4000-8000-000000000594";
        const SECOND: &str = "00000000-0000-4000-8000-000000000595";
        const UNSURE: &str = "00000000-0000-4000-8000-000000000596";
        let _one = TEST_STATE.lock().await;
        let was = set_for_tests(MUSICBRAINZ, true);
        let grouches = serde_json::json!({ "artists": [found(FIRST, "Grouch", 100), found(SECOND, "Grouch", 100)] });
        let lanterns = serde_json::json!({ "artists": [found(UNSURE, "Paper Lantern", 80)] });
        let (fake, asked) = fake_musicbrainz(vec![("Grouch", 200, grouches.clone()), ("GROUCH", 200, grouches), ("Paper Lantern", 200, lanterns)]).await;
        let now = crate::app::moment::now_secs();
        keep_listed(&[person(FIRST, "Grouch"), person(SECOND, "GROUCH"), person(UNSURE, "Paper Lantern")], now);
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let (grouch, lantern) = (cat.artist_for("Grouch").unwrap(), cat.artist_for("Paper Lantern").unwrap());
        let second = open_listed(&cat, SECOND, "GROUCH", now).await;
        let unsure = open_listed(&cat, UNSURE, "Paper Lantern", now).await;
        let fresh = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let library = fresh.artist_for("Grouch").unwrap();
        let first = open_listed(&fresh, FIRST, "Grouch", now).await;
        drop(fake);
        set_for_tests(MUSICBRAINZ, was);
        assert_ne!(second, grouch, "the second of the name: a new artist");
        assert!(holds(&cat, grouch).is_empty(), "the library's keeps no id");
        assert_ne!(unsure, lantern, "MusicBrainz not sure: a new artist");
        assert_eq!(first, library, "the first sure match: the library's Grouch, given the id");
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 3, "one question an open");
    }

    /// An artist opened as `may_open` lets anyone open them by this name (`open_asking`): the one made or found.
    async fn open_listed(cat: &Arc<Catalogue>, id: &str, name: &str, now: i64) -> i64 {
        let name = may_open(id, name, false, now).expect("listed");
        open_asking(cat.clone(), id, &name).await.unwrap().expect("opened")
    }

    /// T6.1 (the review of run 10, finding 6): Beyond lists the rapper "Grouch" first for "grouch living legends", but MusicBrainz's
    /// first sure match for the name is the band. Opening the rapper asks that one question and makes "Grouch (2)" holding his id:
    /// the library's id-less Grouch is never given it, and keeps its records.
    #[tokio::test]
    async fn a_namesake_found_by_other_words_is_not_sure() {
        use crate::app::switches::{MUSICBRAINZ, TEST_STATE, set_for_tests};
        const RAPPER: &str = "00000000-0000-4000-8000-000000000601";
        const BAND: &str = "00000000-0000-4000-8000-000000000602";
        let _one = TEST_STATE.lock().await;
        let was = set_for_tests(MUSICBRAINZ, true);
        let (fake, asked) = fake_musicbrainz(vec![("Grouch", 200, serde_json::json!({ "artists": [found(BAND, "Grouch", 100), found(RAPPER, "Grouch", 98)] }))]).await;
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let (grouch, had) = library_grouch(&cat);
        let now = crate::app::moment::now_secs();
        // Beyond's answer to "grouch living legends": the rapper, first, scored 100 for those words.
        keep_listed(&[person(RAPPER, "Grouch")], now);
        let name = may_open(RAPPER, "grouch", false, now).expect("listed");
        let opened = open_asking(cat.clone(), RAPPER, &name).await;
        drop(fake);
        set_for_tests(MUSICBRAINZ, was);
        let opened = opened.unwrap().expect("opened");
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 1, "one question");
        assert_ne!(opened, grouch, "never the library's Grouch");
        assert_eq!(cat.artist_named(opened).unwrap().unwrap().number, Some(2), "Grouch (2)");
        assert_eq!(holds(&cat, opened), [(RAPPER.to_string(), "search".to_string())], "holding the rapper's id by search");
        assert!(holds(&cat, grouch).is_empty(), "the library's Grouch holds no id");
        assert_eq!(credits(&cat, grouch), [had], "and keeps its records");
    }

    /// T6.2: where the rapper is MusicBrainz's first sure match for the name, the library's id-less Grouch is given his id, as the
    /// monthly read would give it. A card's artist (G7) and an administrator's name ask the same question: one an open.
    #[tokio::test]
    async fn the_names_first_sure_match_is_the_library_artist() {
        use crate::app::switches::{MUSICBRAINZ, TEST_STATE, set_for_tests};
        const RAPPER: &str = "00000000-0000-4000-8000-000000000603";
        const BAND: &str = "00000000-0000-4000-8000-000000000604";
        const AIR: &str = "00000000-0000-4000-8000-000000000605";
        const ZERO: &str = "00000000-0000-4000-8000-000000000606";
        let _one = TEST_STATE.lock().await;
        let was = set_for_tests(MUSICBRAINZ, true);
        let (fake, asked) = fake_musicbrainz(vec![
            ("Grouch", 200, serde_json::json!({ "artists": [found(RAPPER, "Grouch", 100), found(BAND, "Grouch", 90)] })),
            ("Air", 200, serde_json::json!({ "artists": [found(AIR, "Air", 100)] })),
            ("Zero 7", 200, serde_json::json!({ "artists": [found(ZERO, "Zero 7", 100)] })),
        ])
        .await;
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let (grouch, had) = library_grouch(&cat);
        let (air, zero) = (cat.artist_for("Air").unwrap(), cat.artist_for("Zero 7").unwrap());
        let now = crate::app::moment::now_secs();
        keep_listed(&[person(RAPPER, "Grouch")], now);
        let opened = open_asking(cat.clone(), RAPPER, &may_open(RAPPER, "Grouch", false, now).expect("listed")).await;
        keep_carded(&[(AIR.to_string(), "Air".to_string())], now);
        let carded = open_asking(cat.clone(), AIR, &may_open(AIR, "Air", false, now).expect("carded")).await;
        let given = open_asking(cat.clone(), ZERO, &may_open(ZERO, "Zero 7", true, now).expect("an administrator's")).await;
        drop(fake);
        set_for_tests(MUSICBRAINZ, was);
        assert_eq!(opened.unwrap(), Some(grouch), "the name's first sure match: the library's Grouch");
        assert_eq!(holds(&cat, grouch), [(RAPPER.to_string(), "search".to_string())], "given the id, as the monthly read would give it");
        assert_eq!(credits(&cat, grouch), [had]);
        assert_eq!((carded.unwrap(), given.unwrap()), (Some(air), Some(zero)), "a card's and an administrator's open: the same question");
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 3, "one question an open");
    }

    /// T6.3 (OQ-K): MusicBrainz answering 503, or switched off (then asked nothing at all, though it would be sure): not sure.
    /// "Grouch (2)" holds the rapper's id, the library's Grouch is untouched, and the open does not fail.
    #[tokio::test]
    async fn musicbrainz_away_at_open_is_never_sure() {
        use crate::app::switches::{MUSICBRAINZ, TEST_STATE, set_for_tests};
        const RAPPER: &str = "00000000-0000-4000-8000-000000000607";
        let _one = TEST_STATE.lock().await;
        let was = set_for_tests(MUSICBRAINZ, true);
        let now = crate::app::moment::now_secs();
        keep_listed(&[person(RAPPER, "Grouch")], now);
        let name = may_open(RAPPER, "Grouch", false, now).expect("listed");
        // Away: MusicBrainz answers 503.
        let (fake, asked) = fake_musicbrainz(vec![("Grouch", 503, serde_json::json!({ "error": "busy" }))]).await;
        let away = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let (away_grouch, away_had) = library_grouch(&away);
        let away_opened = open_asking(away.clone(), RAPPER, &name).await;
        let away_asked = asked.load(std::sync::atomic::Ordering::SeqCst);
        drop(fake);
        // Switched off: nothing asked, though MusicBrainz would say he is the name's.
        let (fake, asked) = fake_musicbrainz(vec![("Grouch", 200, serde_json::json!({ "artists": [found(RAPPER, "Grouch", 100)] }))]).await;
        set_for_tests(MUSICBRAINZ, false);
        let off = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let (off_grouch, off_had) = library_grouch(&off);
        let off_opened = open_asking(off.clone(), RAPPER, &name).await;
        set_for_tests(MUSICBRAINZ, was);
        let off_asked = asked.load(std::sync::atomic::Ordering::SeqCst);
        drop(fake);
        assert_eq!((away_asked, off_asked), (1, 0), "away: asked once; switched off: no request at all");
        for (cat, grouch, had, opened) in [(&away, away_grouch, away_had, away_opened), (&off, off_grouch, off_had, off_opened)] {
            let opened = opened.expect("the open does not fail").expect("opened");
            assert_ne!(opened, grouch, "not sure: never the library's Grouch");
            assert_eq!(cat.artist_named(opened).unwrap().unwrap().number, Some(2), "Grouch (2)");
            assert_eq!(holds(cat, opened), [(RAPPER.to_string(), "search".to_string())], "holding the rapper's id by search");
            assert!(holds(cat, grouch).is_empty(), "the library's Grouch untouched");
            assert_eq!(credits(cat, grouch), [had]);
        }
    }

    /// G5: an artist row names the artist who holds its id with their number for the name, and the catalogue gives each of a name
    /// theirs, in the order they were made; one no artist holds has none.
    #[test]
    fn an_artist_row_carries_its_number() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let first = open_artist(&cat, ARTIST_1, "Rone", true).unwrap().expect("made");
        let second = open_artist(&cat, ARTIST_2, "Rone", true).unwrap().expect("made");
        assert_eq!(cat.artist_numbers(&[first, second, 999_999]).unwrap(), HashMap::from([(first, 1), (second, 2)]));
        let people = [person(ARTIST_2, "Rone"), person(ARTIST_3, "Rone")];
        let rows = artists(&people, &known(&cat, &[], &[], &people).unwrap());
        assert_eq!(rows.iter().map(|r| (r.artist.clone(), r.number)).collect::<Vec<_>>(), [(Some(ids::artist(second)), Some(2)), (None, None)]);
    }

    /// C57: Beyond's answer makes nothing: no record, artist or reference, whatever it finds.
    #[test]
    fn beyond_makes_nothing_when_it_answers() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        record(&cat, "Night Walks", 2, &[("musicbrainz", "release-group", GROUP_1)]);
        cat.artist_for("Kite Theory").unwrap();
        let count = |table: &str| cat.db_for_tests().query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| r.get::<_, i64>(0)).unwrap();
        let before = [count("records"), count("artists"), count("references"), count("record_artists")];
        let mb = [album(GROUP_1, "Night Walks", "Kite Theory"), album(GROUP_2, "Undertow", "Kite Theory")];
        let discogs = [master(3001, "Paper Kites", "Kite Theory")];
        let people = [person(ARTIST_1, "Kite Theory"), person(ARTIST_2, "Tape Club")];
        let here = known(&cat, &mb, &discogs, &people).unwrap();
        let (listed, found) = (albums(&mb, &discogs, &here, None), artists(&people, &here));
        assert_eq!((listed.len(), found.len()), (3, 2));
        assert_eq!(listed.iter().filter(|a| a.record.is_some()).count(), 1, "the one a record holds");
        assert!(found.iter().all(|a| a.artist.is_none()), "no artist holds their ids");
        assert_eq!([count("records"), count("artists"), count("references"), count("record_artists")], before, "nothing made");
    }
}
