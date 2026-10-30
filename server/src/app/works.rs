//! What MusicBrainz says of the library's releases as music (0.10, the owner, 2026-10-30: Classical as a kind, and
//! Other versions): one question per release, `release/<id>` with its recordings, their works and the works'
//! composers, the artist credits and the performers' relations, its genres and its links (Discogs'), kept in the user
//! data (`mb_releases`, `mb_works`) as the release types are. Then the artists of the records that may be classical
//! (their album artists and their works' composers): MusicBrainz's "is person", so a performance name is the person it
//! is, and their Discogs page; and from Discogs, only through those links, the artists' real names and aliases and the
//! releases' genres. In MusicBrainz's background asker after the release types (`originals`), at its pace; Discogs' in
//! Discogs' own rounds. Each answer is kept as it comes, so a pass stopped goes on where it was.
//!
//! Works and movements (0.12, decision 0086): the same question carries each work's parent ("parts", backward: the
//! movement is a part of the symphony) with its number there (the relation's ordering key) and the parent's title, so
//! no question is added; a release kept before they were read is asked again first (`releases_pass`).

use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::musicbrainz::Asked as _;

/// Releases or artists asked about in one pass.
const BATCH: usize = 200;
/// Between two questions: the release types' pace (the two take turns, never at once).
const PAUSE: Duration = Duration::from_secs(super::musicbrainz::PACE_SECS);
/// What was found is asked again after this; a release MusicBrainz did not know, sooner.
const AGAIN_SECS: i64 = 180 * 86_400;
const NONE_AGAIN_SECS: i64 = 30 * 86_400;
/// MusicBrainz busy (503): once more after this.
const BUSY: Duration = Duration::from_secs(3);
/// Discogs' links and artists asked about in one of its rounds (at its own pace, `discogs::get`).
const DISCOGS_BATCH: usize = 300;
/// The one question per release (the owner's, with the artists' relations, which the works' composers need, and the
/// genres and links): each work's composers come with the work-level relations of artists.
const INC: &str = "recordings+recording-level-rels+work-rels+work-level-rels+artist-credits+artist-rels+url-rels+genres+release-groups+release-group-level-rels";
/// The same without genres and links, should MusicBrainz refuse the long one.
const INC_PLAIN: &str = "recordings+recording-level-rels+work-rels+work-level-rels+artist-credits+artist-rels";
/// MusicBrainz's Various Artists.
const VARIOUS: &str = "89ad4ac3-39f7-470e-963a-56509c546377";

/// An artist as a work's composer: their MusicBrainz id, name and sort name ("Brahms, Johannes").
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub mbid: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_name: Option<String>,
}

/// What a performer did, in the order a record's performers are shown (the owner: conductor, orchestra, soloists, then
/// the others).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Conductor,
    Orchestra,
    Soloist,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Performer {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbid: Option<String>,
    pub role: Role,
}

/// A track of the release that performs works: its recording, disc and place on it, and the works.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackWorks {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording: Option<String>,
    pub disc: u32,
    pub position: u32,
    pub works: Vec<String>,
}

/// A release as kept (`mb_releases.facts`): its genres and its release group's, its verified Discogs links
/// (`release:<id>`, `master:<id>`, the release's own first), its performers in order, whether someone is credited as
/// performing (`performed`), and its tracks that perform works.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stored {
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub discogs: Vec<String>,
    #[serde(default)]
    pub performers: Vec<Performer>,
    #[serde(default)]
    pub performed: bool,
    #[serde(default)]
    pub tracks: Vec<TrackWorks>,
    /// Its works' parents were read (0.12, decision 0086): a release kept before is asked again first.
    #[serde(default)]
    pub with_parents: bool,
}

/// A work: its MusicBrainz id, title and composers, and the work it is a part of where MusicBrainz says (0.12).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Work {
    pub mbid: String,
    pub title: String,
    pub composers: Vec<Person>,
    pub parent: Option<Parent>,
}

/// The work a work is a part of (0.12, decision 0086: a movement's symphony, an aria's act): its MusicBrainz id and
/// title, and the part's number in it (the relation's ordering key; none where MusicBrainz gives none).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parent {
    pub mbid: String,
    pub title: String,
    pub number: Option<u32>,
}

/// What one answer about a release gives: the release as kept, and its works.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Release {
    pub stored: Stored,
    pub works: Vec<Work>,
}

/// The role a relation of a recording or a release gives an artist; none for one that is no performing.
fn role_of(kind: &str) -> Option<Role> {
    match kind {
        "conductor" | "chorus master" => Some(Role::Conductor),
        "performing orchestra" => Some(Role::Orchestra),
        "instrument" | "vocal" | "performer" => Some(Role::Soloist),
        _ => None,
    }
}

/// A performer added once (by MusicBrainz id, else by name), with the first role among those they have.
fn add_performer(into: &mut Vec<Performer>, name: &str, mbid: Option<&str>, role: Role) {
    let same = |p: &Performer| match (p.mbid.as_deref(), mbid) {
        (Some(a), Some(b)) => a == b,
        _ => super::import::loose(&p.name) == super::import::loose(name),
    };
    match into.iter_mut().find(|p| same(&**p)) {
        Some(p) => p.role = p.role.min(role),
        None => into.push(Performer { name: name.to_string(), mbid: mbid.map(str::to_string), role }),
    }
}

/// The performers among relations (`relations` of a release or a recording); whether there was one.
fn performer_rels(relations: &Value, into: &mut Vec<Performer>) -> bool {
    let mut any = false;
    for r in relations.as_array().into_iter().flatten() {
        let (Some(role), Some(name)) = (role_of(r["type"].as_str().unwrap_or_default()), r["artist"]["name"].as_str()) else { continue };
        any = true;
        add_performer(into, name, r["artist"]["id"].as_str(), role);
    }
    any
}

/// The artists of an artist credit, each once: (MusicBrainz id, name); Various Artists left out.
fn credits(credit: &Value, into: &mut Vec<(Option<String>, String)>) {
    for c in credit.as_array().into_iter().flatten() {
        let name = c["artist"]["name"].as_str().or_else(|| c["name"].as_str()).unwrap_or_default();
        let id = c["artist"]["id"].as_str().map(str::to_string);
        if name.is_empty() || id.as_deref() == Some(VARIOUS) || crate::crates::various(name) {
            continue;
        }
        if !into.iter().any(|(i, n)| (id.is_some() && *i == id) || n == name) {
            into.push((id, name.to_string()));
        }
    }
}

/// A work's composers, from its relations.
fn composers_of(relations: &Value) -> Vec<Person> {
    let mut out: Vec<Person> = Vec::new();
    for r in relations.as_array().into_iter().flatten().filter(|r| r["type"].as_str() == Some("composer")) {
        let a = &r["artist"];
        let (Some(id), Some(name)) = (a["id"].as_str(), a["name"].as_str()) else { continue };
        if !out.iter().any(|p| p.mbid == id) {
            out.push(Person { mbid: id.to_string(), name: name.to_string(), sort_name: a["sort-name"].as_str().map(str::to_string) });
        }
    }
    out
}

/// The work a work is a part of, from its relations (0.12): the first "parts" relation pointing backward (MusicBrainz:
/// the parent has the part), with its ordering key. A work's own parts (forward) are not its parent.
fn parent_of(work: &str, relations: &Value) -> Option<Parent> {
    relations
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["type"].as_str() == Some("parts") && r["direction"].as_str() == Some("backward"))
        .find_map(|r| {
            let w = &r["work"];
            let id = w["id"].as_str().filter(|id| !id.is_empty() && *id != work)?;
            let number = r["ordering-key"].as_u64().and_then(|n| u32::try_from(n).ok());
            Some(Parent { mbid: id.to_string(), title: w["title"].as_str().unwrap_or_default().to_string(), number })
        })
}

/// The links among relations (their URLs).
fn urls_of(relations: &Value) -> Vec<String> {
    relations.as_array().into_iter().flatten().filter_map(|r| r["url"]["resource"].as_str().map(str::to_string)).collect()
}

/// A release as MusicBrainz answers the works' question (`INC`): its genres and links, its tracks' works with their
/// composers and parents (0.12), and its performers (the performing relations of the release and its recordings, then
/// the artists credited who composed none of its works). Someone is credited as performing (`performed`) where there is such a
/// relation, or a credited artist who is not one of the works' composers: a composer credited alone performs nothing
/// by that.
pub fn release_of(v: &Value) -> Release {
    let mut stored = Stored::default();
    for g in [&v["genres"], &v["release-group"]["genres"]] {
        for name in g.as_array().into_iter().flatten().filter_map(|g| g["name"].as_str()) {
            if !stored.genres.iter().any(|x| x.eq_ignore_ascii_case(name)) {
                stored.genres.push(name.to_string());
            }
        }
    }
    if let Some(id) = super::discogs::id_in(&urls_of(&v["relations"]), "release") {
        stored.discogs.push(format!("release:{id}"));
    }
    if let Some(id) = super::discogs::id_in(&urls_of(&v["release-group"]["relations"]), "master") {
        stored.discogs.push(format!("master:{id}"));
    }
    let mut works: Vec<Work> = Vec::new();
    let mut performers: Vec<Performer> = Vec::new();
    let mut credited: Vec<(Option<String>, String)> = Vec::new();
    let mut performed = performer_rels(&v["relations"], &mut performers);
    credits(&v["artist-credit"], &mut credited);
    for (mi, medium) in v["media"].as_array().into_iter().flatten().enumerate() {
        let disc = medium["position"].as_u64().map_or(mi as u32 + 1, |d| d as u32);
        for (ti, t) in medium["tracks"].as_array().into_iter().flatten().enumerate() {
            let position = t["position"].as_u64().map(|p| p as u32).or_else(|| t["number"].as_str().and_then(|n| n.parse().ok())).unwrap_or(ti as u32 + 1);
            let rec = &t["recording"];
            credits(&t["artist-credit"], &mut credited);
            credits(&rec["artist-credit"], &mut credited);
            performed |= performer_rels(&rec["relations"], &mut performers);
            let mut here: Vec<String> = Vec::new();
            for r in rec["relations"].as_array().into_iter().flatten().filter(|r| r["type"].as_str() == Some("performance")) {
                let w = &r["work"];
                let Some(id) = w["id"].as_str() else { continue };
                if !here.iter().any(|h| h == id) {
                    here.push(id.to_string());
                }
                if !works.iter().any(|x| x.mbid == id) {
                    works.push(Work {
                        mbid: id.to_string(),
                        title: w["title"].as_str().unwrap_or_default().to_string(),
                        composers: composers_of(&w["relations"]),
                        parent: parent_of(id, &w["relations"]),
                    });
                }
            }
            if !here.is_empty() {
                stored.tracks.push(TrackWorks { recording: rec["id"].as_str().map(str::to_string), disc, position, works: here });
            }
        }
    }
    let composers: std::collections::HashSet<String> = works.iter().flat_map(|w| w.composers.iter().map(|c| c.mbid.clone())).collect();
    for (id, name) in &credited {
        if id.as_ref().is_some_and(|i| composers.contains(i)) {
            continue;
        }
        performed = true;
        add_performer(&mut performers, name, id.as_deref(), Role::Other);
    }
    // In the owner's order; within a role, as MusicBrainz lists them.
    performers.sort_by_key(|p| p.role);
    stored.performers = performers;
    stored.performed = performed;
    stored.with_parents = true;
    Release { stored, works }
}

/// An artist's "is person" relations (MusicBrainz: a person performs as a performance name), from the artist's
/// (`artist`, its MusicBrainz id) lookup: (the performance name, the person, the person's name). From the performance
/// name the relation points backward to the person (Aphex Twin is person Richard D. James); from the person, forward
/// to each name they perform as.
pub fn persons_of(artist: &str, v: &Value) -> Vec<(String, String, String)> {
    let own = v["name"].as_str().unwrap_or_default();
    v["relations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["type"].as_str() == Some("is person"))
        .filter_map(|r| {
            let other = &r["artist"];
            let id = other["id"].as_str()?;
            match r["direction"].as_str() {
                Some("backward") => Some((artist.to_string(), id.to_string(), other["name"].as_str().unwrap_or_default().to_string())),
                _ => Some((id.to_string(), artist.to_string(), own.to_string())),
            }
        })
        .filter(|(alias, person, _)| alias != person)
        .collect()
}

/// MusicBrainz's answer to one question.
enum Answer {
    Found(Value),
    /// No such thing.
    None,
    /// The question refused (400).
    Refused,
}

/// One question to MusicBrainz, once more when it is busy.
async fn ask(http: &reqwest::Client, url: &str) -> Result<Answer> {
    let mut again = true;
    loop {
        let r = http.get(url).asked().await?;
        match r.status() {
            reqwest::StatusCode::SERVICE_UNAVAILABLE if std::mem::take(&mut again) => {
                tokio::time::sleep(BUSY).await;
                continue;
            }
            reqwest::StatusCode::NOT_FOUND => return Ok(Answer::None),
            reqwest::StatusCode::BAD_REQUEST => return Ok(Answer::Refused),
            _ => return Ok(Answer::Found(r.error_for_status()?.json().await?)),
        }
    }
}

/// A release as the works' question gets it; none where MusicBrainz has no such release.
async fn release(http: &reqwest::Client, release: &str) -> Result<Option<Value>> {
    let url = |inc: &str| format!("https://musicbrainz.org/ws/2/release/{release}?inc={inc}&fmt=json");
    match ask(http, &url(INC)).await? {
        Answer::Found(v) => Ok(Some(v)),
        Answer::None => Ok(None),
        Answer::Refused => {
            tokio::time::sleep(PAUSE).await;
            match ask(http, &url(INC_PLAIN)).await? {
                Answer::Found(v) => Ok(Some(v)),
                Answer::None | Answer::Refused => Ok(None),
            }
        }
    }
}

/// One pass (`originals` runs it when the release types have nothing to ask): the releases not asked yet, else the
/// artists. How many questions were asked.
pub async fn pass() -> Result<usize> {
    let asked = releases_pass().await?;
    if asked > 0 {
        return Ok(asked);
    }
    artists_pass().await
}

/// Up to `BATCH` releases of the library's albums not asked about yet (or due again), then those kept before their
/// works' parents were read (0.12, decision 0086: asked again first, at the same pace, so they fill in within hours
/// rather than at their next asking), each kept as it comes.
async fn releases_pass() -> Result<usize> {
    let Some(served) = crate::serve::instance() else { return Ok(0) };
    let now = super::moment::now_secs();
    let (lib, ud) = (served.library.clone(), served.userdata.clone());
    let due: Vec<String> = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
        let asked = ud.mb_releases_asked(now - AGAIN_SECS, now - NONE_AGAIN_SECS)?;
        let without_parents = ud.mb_releases_without_parents()?;
        let all = lib.album_releases()?;
        let mut due: Vec<String> = all.iter().filter(|r| !asked.contains(*r)).take(BATCH).cloned().collect();
        let room = BATCH - due.len();
        due.extend(all.iter().filter(|r| asked.contains(*r) && without_parents.contains(*r)).take(room).cloned());
        Ok(due)
    })
    .await??;
    if due.is_empty() {
        return Ok(0);
    }
    let http = super::discover::http()?;
    let mut n = 0;
    for r in &due {
        let found = match release(&http, r).await {
            Ok(v) => v.map(|v| release_of(&v)),
            // MusicBrainz out of reach (or switched off): what was found is kept, the rest asked next time.
            Err(e) => {
                tracing::info!("works: stopped after {n} of {}: {e:#}", due.len());
                break;
            }
        };
        let facts = match &found {
            Some(f) => Some(serde_json::to_string(&f.stored)?),
            None => None,
        };
        let mut works: Vec<crate::userdata::works::ReadWork> = Vec::new();
        for w in found.iter().flat_map(|f| f.works.iter()) {
            let parent = w.parent.as_ref().map(|p| (p.mbid.clone(), p.title.clone(), p.number));
            works.push((w.mbid.clone(), w.title.clone(), serde_json::to_string(&w.composers)?, parent));
        }
        let (ud, key) = (served.userdata.clone(), r.clone());
        tokio::task::spawn_blocking(move || ud.set_mb_release(&key, facts.as_deref(), &works, super::moment::now_secs())).await??;
        n += 1;
        tokio::time::sleep(PAUSE).await;
    }
    tracing::info!("works: {n} releases asked of MusicBrainz");
    Ok(n)
}

/// Up to `BATCH` artists of the records that may be classical (their genres open the gate, `classical::gate`): their
/// album artists and their works' composers, not asked yet (or due again).
async fn artists_pass() -> Result<usize> {
    let Some(served) = crate::serve::instance() else { return Ok(0) };
    let now = super::moment::now_secs();
    let (lib, ud) = (served.library.clone(), served.userdata.clone());
    let due: Vec<String> = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
        let asked = ud.artists_asked(now - AGAIN_SECS)?;
        let releases = ud.mb_releases()?;
        let works = ud.mb_works()?;
        let mut out: Vec<String> = Vec::new();
        for a in lib.work_albums()? {
            let stored: Option<Stored> = a.release.as_ref().and_then(|r| releases.get(r)).and_then(|j| serde_json::from_str(j).ok());
            if !super::classical::gate(a.genres.iter().chain(stored.iter().flat_map(|s| s.genres.iter())).map(String::as_str)) {
                continue;
            }
            let mut ids: Vec<String> = a.artists.iter().filter_map(|x| x.mbid.clone()).collect();
            for w in stored.iter().flat_map(|s| s.tracks.iter()).flat_map(|t| t.works.iter()) {
                if let Some((_, composers)) = works.get(w) {
                    ids.extend(serde_json::from_str::<Vec<Person>>(composers).unwrap_or_default().into_iter().map(|p| p.mbid));
                }
            }
            for id in ids {
                if super::discogs::is_mbid(&id) && !asked.contains(&id) && !out.contains(&id) {
                    out.push(id);
                }
            }
            if out.len() >= BATCH {
                break;
            }
        }
        out.truncate(BATCH);
        Ok(out)
    })
    .await??;
    if due.is_empty() {
        return Ok(0);
    }
    let http = super::discover::http()?;
    let mut n = 0;
    for id in &due {
        let url = format!("https://musicbrainz.org/ws/2/artist/{id}?inc=artist-rels+url-rels&fmt=json");
        let v = match ask(&http, &url).await {
            Ok(Answer::Found(v)) => Some(v),
            Ok(Answer::None | Answer::Refused) => None,
            Err(e) => {
                tracing::info!("works: artists stopped after {n} of {}: {e:#}", due.len());
                break;
            }
        };
        let persons = v.as_ref().map(|v| persons_of(id, v)).unwrap_or_default();
        let discogs = v.as_ref().and_then(|v| super::discogs::id_in(&urls_of(&v["relations"]), "artist"));
        let (ud, key) = (served.userdata.clone(), id.clone());
        tokio::task::spawn_blocking(move || -> Result<()> {
            ud.add_persons(&persons)?;
            ud.set_artist_asked(&key, discogs, super::moment::now_secs())
        })
        .await??;
        n += 1;
        tokio::time::sleep(PAUSE).await;
    }
    tracing::info!("works: {n} artists asked of MusicBrainz");
    Ok(n)
}

/// Discogs' part, in its own rounds (`discogs::start`), only through MusicBrainz's links: the genres of the linked
/// releases (the release's own, else its master) of albums whose files' and MusicBrainz's genres leave the gate shut,
/// and the real names and aliases of the linked artists.
pub async fn discogs_pass() -> Result<()> {
    // Switched off: asked nothing, and nothing kept as if Discogs had nothing.
    if !super::switches::is_on(super::switches::DISCOGS) {
        return Ok(());
    }
    let Some(served) = crate::serve::instance() else { return Ok(()) };
    let now = super::moment::now_secs();
    let (lib, ud) = (served.library.clone(), served.userdata.clone());
    let (links, artists): (Vec<String>, Vec<u64>) = tokio::task::spawn_blocking(move || -> Result<(Vec<String>, Vec<u64>)> {
        let asked = ud.discogs_links_asked(now - AGAIN_SECS)?;
        let releases = ud.mb_releases()?;
        let mut links: Vec<String> = Vec::new();
        for a in lib.work_albums()? {
            let Some(stored) = a.release.as_ref().and_then(|r| releases.get(r)).and_then(|j| serde_json::from_str::<Stored>(j).ok()) else { continue };
            if super::classical::gate(a.genres.iter().chain(&stored.genres).map(String::as_str)) {
                continue;
            }
            if let Some(link) = stored.discogs.first()
                && !asked.contains(link)
                && !links.contains(link)
            {
                links.push(link.clone());
            }
            if links.len() >= DISCOGS_BATCH {
                break;
            }
        }
        let named = ud.discogs_names_asked(now - AGAIN_SECS)?;
        let mut artists: Vec<u64> = ud.artist_discogs()?.into_values().filter(|d| !named.contains(d)).collect();
        artists.sort_unstable();
        artists.dedup();
        artists.truncate(DISCOGS_BATCH);
        Ok((links, artists))
    })
    .await??;
    for link in links {
        let genres = match super::discogs::linked_genres(&link).await {
            Ok(g) => g,
            Err(e) => {
                tracing::info!("works: Discogs' genres stopped: {e:#}");
                break;
            }
        };
        let ud = served.userdata.clone();
        tokio::task::spawn_blocking(move || ud.set_discogs_link_genres(&link, &genres, super::moment::now_secs())).await??;
    }
    for id in artists {
        let (real, aliases) = match super::discogs::artist_names(id).await {
            Ok(names) => names.unwrap_or_default(),
            Err(e) => {
                tracing::info!("works: Discogs' names stopped: {e:#}");
                break;
            }
        };
        let ud = served.userdata.clone();
        tokio::task::spawn_blocking(move || ud.set_discogs_names(id, real.as_deref(), &aliases, super::moment::now_secs())).await??;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A release of Brahms' piano works played by Claudio Arrau, as MusicBrainz answers the works' question (cut down:
    /// two discs, three tracks, one without a work).
    fn arrau() -> Value {
        serde_json::from_str(include_str!("../../tests/fixtures/musicbrainz-release-works.json")).unwrap()
    }

    #[test]
    fn a_release_gives_its_works_composers_performers_genres_and_links() {
        let r = release_of(&arrau());
        assert_eq!(r.stored.genres, ["classical", "romantic"]);
        assert_eq!(r.stored.discogs, ["release:1234567", "master:76543"]);
        assert_eq!(r.works.len(), 2);
        let sonata = &r.works[0];
        assert_eq!((sonata.mbid.as_str(), sonata.title.as_str()), ("w-sonata-3", "Piano Sonata no. 3 in F minor, op. 5"));
        assert_eq!(sonata.composers, [Person { mbid: "a-brahms".into(), name: "Johannes Brahms".into(), sort_name: Some("Brahms, Johannes".into()) }]);
        // Two tracks perform works, the encore none: only those kept. The fixture's are whole works: no parent.
        assert_eq!(r.stored.tracks.len(), 2);
        assert!(r.works.iter().all(|w| w.parent.is_none()));
        assert!(r.stored.with_parents);
        assert_eq!(r.stored.tracks[0], TrackWorks { recording: Some("rec-1".into()), disc: 1, position: 1, works: vec!["w-sonata-3".into()] });
        assert_eq!((r.stored.tracks[1].disc, r.stored.tracks[1].position), (2, 1));
        // The conductor first, then the orchestra, then the soloist; Brahms, credited, composed the works: no performer.
        let shown: Vec<(&str, Role)> = r.stored.performers.iter().map(|p| (p.name.as_str(), p.role)).collect();
        assert_eq!(shown, [("Bernard Haitink", Role::Conductor), ("Royal Concertgebouw Orchestra", Role::Orchestra), ("Claudio Arrau", Role::Soloist)]);
        assert!(r.stored.performed);
    }

    #[test]
    fn a_composer_credited_alone_performs_nothing_by_that() {
        // As a pop album whose songs link to works its artist composed: works and composers, nobody performing.
        let v = serde_json::json!({
            "artist-credit": [{ "name": "Moby", "artist": { "id": "a-moby", "name": "Moby" } }],
            "media": [{ "position": 1, "tracks": [{ "position": 1, "recording": { "id": "r1", "relations": [
                { "type": "performance", "target-type": "work", "work": { "id": "w1", "title": "Porcelain", "relations": [
                    { "type": "composer", "target-type": "artist", "direction": "backward", "artist": { "id": "a-moby", "name": "Moby", "sort-name": "Moby" } }
                ] } }
            ] } }] }]
        });
        let r = release_of(&v);
        assert_eq!(r.works[0].composers[0].mbid, "a-moby");
        assert!(!r.stored.performed);
        assert!(r.stored.performers.is_empty());
        // Another artist credited performs.
        let mut v = v;
        v["artist-credit"] = serde_json::json!([{ "name": "Moby", "artist": { "id": "a-moby", "name": "Moby" } }, { "name": "Budapest Art Orchestra", "artist": { "id": "a-bao", "name": "Budapest Art Orchestra" } }]);
        let r = release_of(&v);
        assert!(r.stored.performed);
        assert_eq!(r.stored.performers[0].name, "Budapest Art Orchestra");
    }

    #[test]
    fn a_kept_release_reads_back() {
        let r = release_of(&arrau());
        let back: Stored = serde_json::from_str(&serde_json::to_string(&r.stored).unwrap()).unwrap();
        assert_eq!(back, r.stored);
        assert_eq!(serde_json::from_str::<Stored>("{}").unwrap(), Stored::default(), "an empty row reads as nothing found");
    }

    #[test]
    fn a_movement_knows_its_work_and_its_number_there() {
        // As MusicBrainz answers the works' question (cut down from Kleiber's Fifth): two movements, the first also
        // arranged by another (a work relation forward), the second's parent without an ordering key.
        let v = serde_json::json!({ "media": [{ "position": 1, "tracks": [
            { "position": 1, "recording": { "id": "r1", "relations": [
                { "type": "performance", "target-type": "work", "direction": "forward", "work": { "id": "w-5-1", "title": "Symphony no. 5 in C minor, op. 67: I. Allegro con brio", "relations": [
                    { "type": "composer", "target-type": "artist", "direction": "backward", "artist": { "id": "a-lvb", "name": "Ludwig van Beethoven", "sort-name": "Beethoven, Ludwig van" } },
                    { "type": "arrangement", "target-type": "work", "direction": "forward", "work": { "id": "w-liszt", "title": "Symphony no. 5, S. 464 no. 5: I." } },
                    { "type": "parts", "target-type": "work", "direction": "backward", "ordering-key": 1, "attributes": ["movement"], "work": { "id": "w-5", "title": "Symphony no. 5 in C minor, op. 67" } }
                ] } }
            ] } },
            { "position": 2, "recording": { "id": "r2", "relations": [
                { "type": "performance", "target-type": "work", "direction": "forward", "work": { "id": "w-5-2", "title": "Symphony no. 5 in C minor, op. 67: II. Andante con moto", "relations": [
                    { "type": "parts", "target-type": "work", "direction": "backward", "work": { "id": "w-5", "title": "Symphony no. 5 in C minor, op. 67" } }
                ] } }
            ] } }
        ] }] });
        let r = release_of(&v);
        assert_eq!(r.works[0].parent, Some(Parent { mbid: "w-5".into(), title: "Symphony no. 5 in C minor, op. 67".into(), number: Some(1) }));
        assert_eq!(r.works[1].parent.as_ref().map(|p| (p.mbid.as_str(), p.number)), Some(("w-5", None)));
        // A whole work's own parts (forward) are not its parent.
        let whole = serde_json::json!([{ "type": "parts", "direction": "forward", "ordering-key": 1, "work": { "id": "w-5-1", "title": "I." } }]);
        assert_eq!(parent_of("w-5", &whole), None);
    }

    #[test]
    fn a_performance_name_is_its_person_from_either_side() {
        let aphex = serde_json::json!({ "name": "Aphex Twin", "relations": [
            { "type": "is person", "direction": "backward", "artist": { "id": "rdj", "name": "Richard D. James" } },
            { "type": "member of band", "direction": "backward", "artist": { "id": "x", "name": "X" } }
        ] });
        assert_eq!(persons_of("aphex", &aphex), [("aphex".to_string(), "rdj".to_string(), "Richard D. James".to_string())]);
        let rdj = serde_json::json!({ "name": "Richard D. James", "relations": [
            { "type": "is person", "direction": "forward", "artist": { "id": "aphex", "name": "Aphex Twin" } },
            { "type": "is person", "direction": "forward", "artist": { "id": "afx", "name": "AFX" } }
        ] });
        let found = persons_of("rdj", &rdj);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0], ("aphex".to_string(), "rdj".to_string(), "Richard D. James".to_string()));
    }
}
