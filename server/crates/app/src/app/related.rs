//! Related artists to explore, on an artist's page (0.9.2; the owner, 2026-10-25: "recommended artists, for example.
//! related artists to explore. That's the best."; "prioritize Bandcamp playable artists"): one ranked row from the
//! artists ListenBrainz finds listened to alongside (its similar artists, by MusicBrainz id) and MusicBrainz's members
//! and collaborators. One the library has opens its own page; a new one its album on Bandcamp, where one plays. Those
//! that play (the library's, and the new ones on Bandcamp) come first. Looked up in the background the first time the
//! page asks, kept 30 days for every account (userdata `related_artists`): MusicBrainz and Bandcamp at one request a
//! second.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::import::loose;
use super::references::Outcome;
use super::musicbrainz::Asked as _;
use super::reach::Sent as _;

/// ListenBrainz's similar artists, as its own artist pages ask for them (its labs API, with the algorithm they use).
const LB_LABS: &str = "https://labs.api.listenbrainz.org/similar-artists/json";
const LB_ALGORITHM: &str = "session_based_days_7500_session_300_contribution_5_threshold_10_limit_100_filter_True_skip_30";
/// Of ListenBrainz's similar artists, how many are taken; of all, how many the row shows.
const SIMILAR: usize = 30;
const MOST: usize = 20;
/// Of the new artists, the most related this many are looked for on Bandcamp (MusicBrainz, then Bandcamp).
const BANDCAMP_LOOKS: usize = 12;
/// Looked up again after this long.
const FRESH_SECS: i64 = 30 * 86_400;
/// What MusicBrainz's relations rank at, against ListenBrainz's most alike (1).
const RELATED_ON_MB: f64 = 0.8;

/// An artist related to another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Related {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbid: Option<String>,
    /// Why, in a few words each: "Listened alongside", "Band member", "Worked together".
    pub why: Vec<String>,
    /// Their id in the library, whose page they open; none: new to the library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Their number for their name (C55, G5), as they are sent: "Name (n)" where the list holds two of a name. Never kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<i64>,
    /// An album of theirs that plays on Bandcamp (a new artist's way in), and its title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandcamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    /// How related, as ranked (kept, so Discover can weigh them: the most related 1, roughly).
    #[serde(default)]
    pub score: f64,
}

/// What a look found for an artist (by MusicBrainz id), and when.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Found {
    pub at: i64,
    pub artists: Vec<Related>,
}

/// An artist an artist page asks about (C55): who they are, and their MusicBrainz id.
#[derive(Debug, Clone, PartialEq)]
pub struct Asked {
    /// Their id for life, as served (`ar-<n>`).
    pub id: String,
    pub name: String,
    /// Their number among the artists of their name, and whether another artist has it (`Catalogue::artist_named`).
    pub number: Option<i64>,
    pub namesakes: bool,
    pub mbid: Option<String>,
}

/// The artist an artist page asks about (C55: its view, About, related artists, the photo), Cratarr's own:
/// - by their id for life, that artist, named as the catalogue names them, though another artist has the name;
/// - by name, the first artist of it (a bare name from outside opens the lowest number), named as asked.
///
/// Their MusicBrainz id is their own references' first (`Catalogue::artist_known`), else the library's row of them. None: a name
/// no artist has, or an id no artist has (forgotten, or never given). Blocking.
pub fn asked(serve: &crate::serve::Serve, id: Option<&str>, name: &str) -> Result<Option<Asked>> {
    let cat = serve.catalogue();
    let artist = match id {
        Some(id) => crate::serve::ids::artist_number(id),
        None if name.trim().is_empty() => None,
        None => cat.artist_of(name.trim())?,
    };
    let Some(named) = artist.map(|a| cat.artist_named(a)).transpose()?.flatten() else { return Ok(None) };
    let served = crate::serve::ids::artist(named.id);
    let own = cat.artist_known(named.id)?.and_then(|(_, refs)| refs.into_iter().find(|(r, _)| r.source == "musicbrainz" && r.kind == "artist").map(|(r, _)| r.value));
    let mbid = match own {
        Some(mbid) => Some(mbid),
        None => serve.library.artist(&served)?.and_then(|a| a.music_brainz_id),
    };
    Ok(Some(Asked {
        id: served,
        name: if id.is_some() { named.name } else { name.trim().to_string() },
        number: named.number,
        namesakes: named.namesakes,
        mbid: mbid.filter(|m| !m.is_empty()),
    }))
}

/// The sources a look reads the recommendations from, in the order it asks them (F8): ListenBrainz's similar artists, MusicBrainz's
/// relations. Bandcamp is asked after them only for the new artists' albums.
const LISTS: [&str; 2] = ["listenbrainz", "musicbrainz"];

/// What a look reads (F8): ListenBrainz's similar artists, MusicBrainz's relations, an album of a new artist's on Bandcamp. The
/// server's ask those services (`Services`); a test's are fakes that keep what they are asked.
pub trait Lists: Send + Sync {
    fn similar<'a>(&'a self, mbid: &'a str) -> crate::source::Fut<'a, Vec<(String, String, f64)>>;
    fn related<'a>(&'a self, mbid: &'a str) -> crate::source::Fut<'a, (MbRelated, Vec<(String, String, String)>)>;
    fn album<'a>(&'a self, mbid: &'a str, name: &'a str) -> crate::source::Fut<'a, Option<(String, String)>>;
}

/// The services themselves, through one client, made when first asked (`discover::http`).
pub struct Services(std::sync::OnceLock<reqwest::Client>);

impl Services {
    pub fn new() -> Arc<dyn Lists> {
        Arc::new(Services(std::sync::OnceLock::new()))
    }

    fn http(&self) -> Result<&reqwest::Client> {
        if let Some(http) = self.0.get() {
            return Ok(http);
        }
        let http = super::discover::http()?;
        Ok(self.0.get_or_init(|| http))
    }
}

impl Lists for Services {
    fn similar<'a>(&'a self, mbid: &'a str) -> crate::source::Fut<'a, Vec<(String, String, f64)>> {
        Box::pin(async move { lb_similar(self.http()?, mbid).await })
    }

    fn related<'a>(&'a self, mbid: &'a str) -> crate::source::Fut<'a, (MbRelated, Vec<(String, String, String)>)> {
        Box::pin(async move { mb_related(self.http()?, mbid).await })
    }

    fn album<'a>(&'a self, mbid: &'a str, name: &'a str) -> crate::source::Fut<'a, Option<(String, String)>> {
        Box::pin(async move { Ok(on_bandcamp(self.http()?, mbid, name).await) })
    }
}

/// The artists being looked up now (by MusicBrainz id), each with the source the look asks now (F8: the page's `reading`): one look
/// each at a time.
static LOOKING: Mutex<Option<HashMap<String, &'static str>>> = Mutex::new(None);

fn looking() -> std::sync::MutexGuard<'static, Option<HashMap<String, &'static str>>> {
    LOOKING.lock().unwrap_or_else(|p| p.into_inner())
}

/// A look of the artist begun, asking `LISTS`' first, unless one runs already: whether it begins.
fn begins(mbid: &str) -> bool {
    let mut all = looking();
    let all = all.get_or_insert_with(HashMap::new);
    if all.contains_key(mbid) {
        return false;
    }
    all.insert(mbid.to_string(), LISTS[0]);
    true
}

/// A look of the artist asking `source` now.
fn asks(mbid: &str, source: &'static str) {
    if let Some(now) = looking().as_mut().and_then(|all| all.get_mut(mbid)) {
        *now = source;
    }
}

/// Ends a look of the artist, whatever becomes of it (a panic, a task dropped).
struct Ends(String);

impl Drop for Ends {
    fn drop(&mut self) {
        if let Some(all) = looking().as_mut() {
            all.remove(&self.0);
        }
    }
}

/// What the last look of an artist here could not hear (F8), by MusicBrainz id: when, and each source's outcome (F1's: switched off,
/// didn't answer), in the order the look asked them; none where it heard every source.
type Unheard = HashMap<String, (i64, Vec<(&'static str, Outcome)>)>;

static UNHEARD: Mutex<Option<Unheard>> = Mutex::new(None);

fn unheard() -> std::sync::MutexGuard<'static, Option<Unheard>> {
    UNHEARD.lock().unwrap_or_else(|p| p.into_inner())
}

fn unheard_of(mbid: &str) -> Option<(i64, Vec<(&'static str, Outcome)>)> {
    unheard().as_ref()?.get(mbid).cloned()
}

/// What is kept for the artist, and whether it is due to be looked up at `now`: never, or longer ago than `FRESH_SECS`; or the last look here could not
/// hear a source (F8): one it said was switched off is read at once once it is on again (F1.6), one that did not answer after
/// `discover::LIST_RETRY_SECS` whatever is kept, not before.
pub fn kept(serve: &crate::serve::Serve, mbid: &str, now: i64) -> Result<(Option<Found>, bool)> {
    let found = serve.userdata.related_artists(mbid)?.and_then(|(_, json)| serde_json::from_str::<Found>(&json).ok());
    let old = found.as_ref().is_none_or(|f| now - f.at > FRESH_SECS);
    let due = match unheard_of(mbid) {
        Some((_, said)) if !super::references::still_said(&said, now) => true,
        Some((at, said)) if said.iter().any(|(_, o)| *o != Outcome::Off) => now - at > super::discover::LIST_RETRY_SECS,
        _ => old,
    };
    Ok((found, due))
}

/// The artist's related artists as kept, looked up now (and kept) when none are or they are old: for Discover, which
/// waits for them in its own background look (0.9.2: the masonry's artists get their tiles from here). While another
/// look of the same artist runs, what is kept (possibly nothing).
pub async fn kept_or_look(serve: &Arc<crate::serve::Serve>, name: &str, mbid: &str) -> Result<Found> {
    let (s, key) = (serve.clone(), mbid.to_string());
    let (found, due) = crate::run::spawn_blocking(move || kept(&s, &key, super::moment::now_secs())).await??;
    if !due || !begins(mbid) {
        // A1.2.3: marked as the library is now.
        let mut found = found.unwrap_or_default();
        found.artists = marked(serve, std::mem::take(&mut found.artists)).await?;
        return Ok(found);
    }
    let _ends = Ends(mbid.to_string());
    looked(serve, name, mbid, Services::new().as_ref()).await
}

/// Looks the artist up in the background through `lists`, unless that runs already (`looked`).
pub fn start(serve: Arc<crate::serve::Serve>, name: String, mbid: String, lists: Arc<dyn Lists>) {
    if !begins(&mbid) {
        return;
    }
    crate::run::spawn(async move {
        let _ends = Ends(mbid.clone());
        if let Err(e) = looked(&serve, &name, &mbid, lists.as_ref()).await {
            tracing::info!("related artists of {name}: {e:#}");
        }
    });
}

/// A look (`look`) and what came of it: what it found kept for every account where it heard a source (one that heard none keeps
/// nothing, so what was kept stays and a later open asks again, F8); what it could not hear remembered (`unheard_of`), forgotten where
/// it heard every source.
async fn looked(serve: &Arc<crate::serve::Serve>, name: &str, mbid: &str, lists: &dyn Lists) -> Result<Found> {
    let (found, said) = look(serve, name, mbid, lists).await?;
    let heard = LISTS.iter().any(|l| !said.iter().any(|(s, _)| s == l));
    {
        let mut all = unheard();
        let all = all.get_or_insert_with(HashMap::new);
        if said.is_empty() {
            all.remove(mbid);
        } else {
            all.insert(mbid.to_string(), (found.at, said));
        }
    }
    if heard {
        let (ud, key, json) = (serve.userdata.clone(), mbid.to_string(), serde_json::to_string(&found)?);
        let at = found.at;
        crate::run::spawn_blocking(move || ud.set_related_artists(&key, at, &json)).await??;
    }
    Ok(found)
}

/// An artist's recommendations as their page asks for them at `now` (F8): those kept, a look begun through `lists` where due
/// (`kept`); whether a look runs (`looking`) and the source it asks now (`reading`); where none runs, what the last look could not
/// hear, and MusicBrainz where it is switched off now (`no_list`, in F1's words), in the order a look asks them.
#[derive(Debug, Default, Serialize)]
pub struct Answer {
    pub artists: Vec<Related>,
    pub looking: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_list: Option<Vec<super::references::Said>>,
}

pub async fn answer(serve: &Arc<crate::serve::Serve>, name: &str, mbid: &str, now: i64, lists: Arc<dyn Lists>) -> Result<Answer> {
    let (s, key) = (serve.clone(), mbid.to_string());
    let (found, due) = crate::run::spawn_blocking(move || kept(&s, &key, now)).await??;
    if due {
        start(serve.clone(), name.to_string(), mbid.to_string(), lists);
    }
    let (looking, reading, no_list) = said_now(mbid, now);
    let artists = marked(serve, found.map(|f| f.artists).unwrap_or_default()).await?;
    Ok(Answer { artists, looking, reading, no_list })
}

/// What the looks of the artist say at `now` (F8): whether one runs, and the source it asks now; where none runs, what the last look
/// here could not hear, and MusicBrainz where it is switched off now, in F1's words (`references::Said`), in the order a look asks them;
/// none where every source answered.
fn said_now(mbid: &str, now: i64) -> (bool, Option<&'static str>, Option<Vec<super::references::Said>>) {
    if let Some(source) = looking().as_ref().and_then(|all| all.get(mbid).copied()) {
        return (true, Some(source), None);
    }
    let mut said = unheard_of(mbid).map(|(_, said)| said).unwrap_or_default();
    // Switched off now, whatever the last look heard (F8.1 (c)).
    if !super::switches::is_on(super::switches::MUSICBRAINZ) {
        said.retain(|(s, _)| *s != "musicbrainz");
        said.push(("musicbrainz", Outcome::Off));
    }
    let words: Vec<super::references::Said> =
        LISTS.iter().flat_map(|l| said.iter().filter(move |(s, _)| s == l)).map(|&(s, o)| super::references::Said::of(s, o, now)).collect();
    (false, None, (!words.is_empty()).then_some(words))
}

/// Related artists as the library has them now (A1.2): each the catalogue's artist holding their MusicBrainz id, with albums here;
/// else, by their name, the one library artist of it holding no MusicBrainz id (a namesake holding another id, or two of the name, is
/// no one: new to the library). Marked at every answer: a look kept a month carries no marks of its own, so an artist the library
/// gained since is marked at once.
async fn marked(serve: &Arc<crate::serve::Serve>, mut artists: Vec<Related>) -> Result<Vec<Related>> {
    let cat = serve.catalogue().clone();
    crate::run::spawn_blocking(move || -> Result<Vec<Related>> {
        for r in artists.iter_mut() {
            r.id = cat.library_artist_of(r.mbid.as_deref(), &r.name)?.map(crate::serve::ids::artist);
        }
        Ok(artists)
    })
    .await?
}

/// ListenBrainz's similar artists: (MusicBrainz id, name, its score).
async fn lb_similar(http: &reqwest::Client, mbid: &str) -> Result<Vec<(String, String, f64)>> {
    #[derive(Deserialize)]
    struct Similar {
        artist_mbid: String,
        name: String,
        #[serde(default)]
        score: f64,
    }
    let list: Vec<Similar> = http
        .get(LB_LABS)
        .query(&[("artist_mbids", mbid), ("algorithm", LB_ALGORITHM)])
        .sent()
        .await
        .context("ListenBrainz did not answer")?
        .error_for_status()?
        .json()
        .await
        .context("ListenBrainz's similar artists could not be read")?;
    Ok(list.into_iter().filter(|s| s.artist_mbid != mbid).take(SIMILAR).map(|s| (s.artist_mbid, s.name, s.score)).collect())
}

/// A related artist as MusicBrainz names them: (their MusicBrainz id, name, why).
type MbRelated = Vec<(String, String, &'static str)>;

/// MusicBrainz's artists related to an artist, each with why: the bands they play in and their members, those they
/// worked with, those who played with them, the person a performance name is and the names a person performs as (0.10:
/// kept, `works::persons_of`, so an alias is the real person); and those "is person" relations as found.
async fn mb_related(http: &reqwest::Client, mbid: &str) -> Result<(MbRelated, Vec<(String, String, String)>)> {
    #[derive(Deserialize)]
    struct Artist {
        id: String,
        name: String,
    }
    #[derive(Deserialize)]
    struct Relation {
        #[serde(rename = "type", default)]
        kind: String,
        #[serde(default)]
        direction: String,
        artist: Option<Artist>,
    }
    #[derive(Deserialize)]
    struct Lookup {
        #[serde(default)]
        relations: Vec<Relation>,
    }
    let url = format!("https://musicbrainz.org/ws/2/artist/{mbid}?inc=artist-rels&fmt=json");
    let raw: serde_json::Value = http.get(url).asked().await?.error_for_status()?.json().await?;
    let persons = super::works::persons_of(mbid, &raw);
    let found: Lookup = serde_json::from_value(raw)?;
    let related = found
        .relations
        .into_iter()
        .filter_map(|r| {
            let why = match (r.kind.as_str(), r.direction.as_str()) {
                ("member of band", "backward") => "Band member",
                ("member of band", _) => "Their band",
                ("collaboration", _) => "Worked together",
                ("supporting musician" | "instrumental supporting musician" | "vocal supporting musician", _) => "Played together",
                ("subgroup", _) => "Their group",
                ("is person", "backward") => "Same person",
                ("is person", _) => "Performs as",
                _ => return None,
            };
            let a = r.artist?;
            (a.id != mbid).then_some((a.id, a.name, why))
        })
        .collect();
    Ok((related, persons))
}

/// An album of the artist that plays on Bandcamp, and its title: their own Bandcamp page only as MusicBrainz links it (O5), an
/// album the search box lists on it (O4: its `/music` page is not read). None with Bandcamp switched off.
async fn on_bandcamp(http: &reqwest::Client, mbid: &str, name: &str) -> Option<(String, String)> {
    if !super::switches::is_on(super::switches::BANDCAMP) {
        return None;
    }
    let urls = super::discover::mb_artist_urls(http, mbid).await.ok()?;
    let own = urls.iter().filter_map(|u| crate::bandcamp::page(u)).find(|p| reqwest::Url::parse(p).ok().and_then(|u| u.host_str().map(|h| h != "bandcamp.com")).unwrap_or(false))?;
    crate::bandcamp::Bandcamp::get().albums_on(&own, name).await.ok()?.into_iter().next()
}

/// Looks up the artists related to one (`name`, `mbid`) through `lists`: ListenBrainz's and MusicBrainz's, the library's marked, the
/// new ones' Bandcamp albums found, ranked: those that play first, then the most related. Each source it asks said as it asks it, and
/// what it could not hear (F8): ListenBrainz or MusicBrainz not answering, MusicBrainz switched off (asked nothing then, nor for the
/// new ones' albums on Bandcamp, which it links), in F1's outcomes.
async fn look(serve: &Arc<crate::serve::Serve>, name: &str, mbid: &str, lists: &dyn Lists) -> Result<(Found, Vec<(&'static str, Outcome)>)> {
    let mut said: Vec<(&'static str, Outcome)> = Vec::new();
    let mut by: HashMap<String, Related> = HashMap::new();
    let mut add = |id: String, name: String, why: &str, score: f64| {
        let r = by.entry(id.clone()).or_insert_with(|| Related { name, mbid: Some(id), why: Vec::new(), id: None, number: None, bandcamp: None, album: None, score: 0.0 });
        if !r.why.iter().any(|w| w == why) {
            r.why.push(why.to_string());
        }
        // A second source counts extra.
        r.score = if r.score > 0.0 { r.score + score + 0.5 } else { score };
    };
    asks(mbid, "listenbrainz");
    match lists.similar(mbid).await {
        Ok(similar) => {
            let most = similar.iter().map(|s| s.2).fold(0.0, f64::max).max(f64::EPSILON);
            for (id, n, s) in similar {
                add(id, n, "Listened alongside", s / most);
            }
        }
        Err(e) => {
            tracing::info!("related artists of {name}: ListenBrainz: {e:#}");
            said.push(("listenbrainz", Outcome::of(&e)));
        }
    }
    let musicbrainz = super::switches::is_on(super::switches::MUSICBRAINZ);
    let related = if musicbrainz {
        asks(mbid, "musicbrainz");
        lists.related(mbid).await
    } else {
        said.push(("musicbrainz", Outcome::Off));
        Ok(Default::default())
    };
    match related {
        Ok((related, persons)) => {
            for (id, n, why) in related {
                add(id, n, why, RELATED_ON_MB);
            }
            if !persons.is_empty() {
                let ud = serve.userdata.clone();
                if let Err(e) = crate::run::spawn_blocking(move || ud.add_persons(&persons)).await.map_err(anyhow::Error::from).and_then(|r| r) {
                    tracing::info!("related artists of {name}: who is who not kept: {e:#}");
                }
            }
        }
        Err(e) => {
            tracing::info!("related artists of {name}: MusicBrainz: {e:#}");
            said.push(("musicbrainz", Outcome::of(&e)));
        }
    }
    let mut artists: Vec<Related> = by.into_values().filter(|r| loose(&r.name) != loose(name)).collect();
    // The library's (A1.2): by their MusicBrainz id first, by name only where it can be no other.
    artists = marked(serve, artists).await?;
    // The new ones on Bandcamp, the most related first.
    artists.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    let mut looked = 0;
    for r in artists.iter_mut().filter(|r| musicbrainz && r.id.is_none()) {
        if looked == BANDCAMP_LOOKS {
            break;
        }
        if looked == 0 && super::switches::is_on(super::switches::BANDCAMP) {
            asks(mbid, "bandcamp");
        }
        looked += 1;
        if let Some(id) = r.mbid.clone()
            && let Ok(Some((title, page))) = lists.album(&id, &r.name).await
        {
            (r.album, r.bandcamp) = (Some(title), Some(page));
        }
    }
    // Those that play first (the owner: "prioritize Bandcamp playable artists"), then the most related.
    artists.sort_by(|a, b| {
        let plays = |r: &Related| r.id.is_some() || r.bandcamp.is_some();
        plays(b).cmp(&plays(a)).then_with(|| b.score.total_cmp(&a.score)).then_with(|| a.name.cmp(&b.name))
    });
    artists.truncate(MOST);
    Ok((Found { at: super::moment::now_secs(), artists }, said))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_look_reads_back_and_says_when_it_is_due() {
        let found = Found {
            at: super::super::moment::now_secs(),
            artists: vec![Related {
                name: "Extrawelt".into(),
                mbid: Some("m1".into()),
                why: vec!["Listened alongside".into()],
                id: None,
                number: None,
                bandcamp: Some("https://extrawelt.bandcamp.com/album/x".into()),
                album: Some("Fernweh".into()),
                score: 0.75,
            }],
        };
        let json = serde_json::to_string(&found).unwrap();
        let back: Found = serde_json::from_str(&json).unwrap();
        assert_eq!(back.artists[0].bandcamp.as_deref(), Some("https://extrawelt.bandcamp.com/album/x"));
        assert_eq!((back.artists[0].album.as_deref(), back.artists[0].score), (Some("Fernweh"), 0.75), "kept with its rank, for Discover");
        assert!(super::super::moment::now_secs() - back.at <= FRESH_SECS, "fresh");
    }

    /// C55: About, related artists and the photo go by the artist asked for (`asked`): by id, that one, named as the catalogue
    /// names them, with their own MusicBrainz id, though another has the name; by name, the first of it. An id no artist has is
    /// no one's.
    #[test]
    fn about_and_related_go_by_id() {
        let serve = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap()));
        let cat = serve.catalogue();
        let (first, second) = (cat.artist_for("Rone").unwrap(), cat.artist_apart("Rone").unwrap());
        let (french, other) = ("00000000-0000-4000-8000-000000000301", "00000000-0000-4000-8000-000000000302");
        for (artist, mbid) in [(first, french), (second, other)] {
            assert!(cat.set_reference(&crate::serve::ids::artist(artist), &crate::catalogue::records::Ref::new("musicbrainz", "artist", mbid), "files").unwrap());
        }
        let id = crate::serve::ids::artist;
        let by = |asked_id: Option<&str>, name: &str| asked(&serve, asked_id, name).unwrap().map(|a| (a.id, a.name, a.mbid));
        assert_eq!(by(Some(id(second).as_str()), ""), Some((id(second), "Rone".to_string(), Some(other.to_string()))), "by id: that one");
        assert_eq!(by(None, "RONE"), Some((id(first), "RONE".to_string(), Some(french.to_string()))), "a bare name: the first");
        assert_eq!(by(Some("ar-999999"), "Rone"), None, "an id no artist has");
    }

    // ---- A1.2: related artists marked by their MusicBrainz id

    /// A serve with a catalogue, and a library artist of `name` (a had record, credited first), holding MusicBrainz id `mbid` where given.
    fn library_artist(serve: &crate::serve::Serve, name: &str, mbid: Option<&str>) -> i64 {
        let cat = serve.catalogue();
        let artist = cat.artist_apart(name).unwrap();
        let record = cat.record_for_tests(&format!("{name}'s album"), 2);
        cat.db_for_tests().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'files')", rusqlite::params![record, artist]).unwrap();
        if let Some(m) = mbid {
            assert!(cat.set_reference(&crate::serve::ids::artist(artist), &crate::catalogue::records::Ref::new("musicbrainz", "artist", m), "files").unwrap());
        }
        artist
    }

    fn serve_with_catalogue() -> Arc<crate::serve::Serve> {
        let serve = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap()));
        serve
    }

    fn related(name: &str, mbid: &str) -> Related {
        Related { name: name.into(), mbid: Some(mbid.into()), why: vec!["Listened alongside".into()], id: None, number: None, bandcamp: None, album: None, score: 0.5 }
    }

    /// A1.2.1: a related "Mo’ Horizons" carries MusicBrainz id A7, which the library's "mo'horizons" holds: marked theirs (`id`), so
    /// not new, and Discover's "Like <seed>" skips it.
    #[tokio::test]
    async fn a_related_artist_is_the_library_artist_who_holds_their_id() {
        const A7: &str = "00000000-0000-4000-8000-000000000c07";
        let serve = serve_with_catalogue();
        let horizons = library_artist(&serve, "mo'horizons", Some(A7));
        let marks = marked(&serve, vec![related("Mo\u{2019} Horizons", A7)]).await.unwrap();
        assert_eq!(marks[0].id.as_deref(), Some(crate::serve::ids::artist(horizons).as_str()), "theirs by the id");
    }

    /// A1.2.2: a related "Glass Harbour" with id A8 is the library's by name only where it can be no other: (a) one library "Glass
    /// Harbour" holding no MusicBrainz id: marked; (b) one holding another id: new; (c) two of the name: new.
    #[tokio::test]
    async fn a_related_artist_by_name_only_where_one_with_no_id_has_it() {
        const A8: &str = "00000000-0000-4000-8000-000000000c08";
        let serve = serve_with_catalogue();
        let one = library_artist(&serve, "Glass Harbour", None);
        assert_eq!(marked(&serve, vec![related("Glass Harbour", A8)]).await.unwrap()[0].id.as_deref(), Some(crate::serve::ids::artist(one).as_str()), "(a)");
        let serve = serve_with_catalogue();
        library_artist(&serve, "Glass Harbour", Some("00000000-0000-4000-8000-000000000c09"));
        assert_eq!(marked(&serve, vec![related("Glass Harbour", A8)]).await.unwrap()[0].id, None, "(b) a namesake holding another id: new");
        let serve = serve_with_catalogue();
        library_artist(&serve, "Glass Harbour", None);
        library_artist(&serve, "Glass Harbour", None);
        assert_eq!(marked(&serve, vec![related("Glass Harbour", A8)]).await.unwrap()[0].id, None, "(c) two of the name: new");
    }

    /// A1.2.3: a look kept a month carries no marks of its own: each answer marks again, so an artist the library gained since is marked
    /// at once, and one kept marked who is no longer the library's is not.
    #[tokio::test]
    async fn a_kept_look_is_marked_as_the_library_is_now() {
        const SEED: &str = "00000000-0000-4000-8000-000000000c10";
        const A9: &str = "00000000-0000-4000-8000-000000000c11";
        let serve = serve_with_catalogue();
        let stale = Related { id: Some("ar-999999".into()), ..related("Paper Lantern", "00000000-0000-4000-8000-000000000c12") };
        let found = Found { at: super::super::moment::now_secs(), artists: vec![related("Tape Club", A9), stale] };
        serve.userdata.set_related_artists(SEED, found.at, &serde_json::to_string(&found).unwrap()).unwrap();
        let club = library_artist(&serve, "Tape Club", Some(A9));
        let lists: Arc<dyn Lists> = Arc::new(Fake { asked: Default::default(), similar: || Ok(Vec::new()) });
        let answered = answer(&serve, "Seed", SEED, super::super::moment::now_secs(), lists).await.unwrap();
        let ids: Vec<Option<String>> = answered.artists.iter().map(|a| a.id.clone()).collect();
        assert_eq!(ids, [Some(crate::serve::ids::artist(club)), None], "marked as the library is now");
        let discover = kept_or_look(&serve, "Seed", SEED).await.unwrap();
        assert_eq!(discover.artists[0].id, Some(crate::serve::ids::artist(club)), "Discover's look too");
    }

    // ---- F8 (preview 1's fix batch): the recommendations say what they read, and what they could not

    /// The recommendations' sources in a test: ListenBrainz answering `similar`, MusicBrainz a collaborator, Bandcamp no album; each ask
    /// kept by its source's id.
    struct Fake {
        asked: std::sync::Mutex<Vec<&'static str>>,
        similar: fn() -> Result<Vec<(String, String, f64)>>,
    }

    impl Lists for Fake {
        fn similar<'a>(&'a self, _: &'a str) -> crate::source::Fut<'a, Vec<(String, String, f64)>> {
            self.asked.lock().unwrap().push("listenbrainz");
            let answer = (self.similar)();
            Box::pin(async move { answer })
        }

        fn related<'a>(&'a self, _: &'a str) -> crate::source::Fut<'a, (MbRelated, Vec<(String, String, String)>)> {
            self.asked.lock().unwrap().push("musicbrainz");
            Box::pin(async { Ok((vec![("00000000-0000-4000-8000-000000000812".to_string(), "Salt Meridian".to_string(), "Worked together")], Vec::new())) })
        }

        fn album<'a>(&'a self, _: &'a str, _: &'a str) -> crate::source::Fut<'a, Option<(String, String)>> {
            self.asked.lock().unwrap().push("bandcamp");
            Box::pin(async { Ok(None) })
        }
    }

    fn fake(similar: fn() -> Result<Vec<(String, String, f64)>>) -> Arc<Fake> {
        Arc::new(Fake { asked: Default::default(), similar })
    }

    /// The recommendations asked at `now` once no look runs (a test fails rather than waits for good).
    async fn ended(serve: &Arc<crate::serve::Serve>, name: &str, mbid: &str, now: i64, lists: &Arc<Fake>) -> Answer {
        for _ in 0..500 {
            let a = answer(serve, name, mbid, now, lists.clone()).await.unwrap();
            if !a.looking {
                return a;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("still looking")
    }

    /// F8.1 (c), as corrected (the recommendations read ListenBrainz and MusicBrainz, never Last.fm): while a look runs
    /// they say so, naming the source it asks first; MusicBrainz switched off, a look asks it nothing (nor for the new artists' albums on
    /// Bandcamp, which MusicBrainz links), still reads ListenBrainz, and they say MusicBrainz is switched off, in F1's words; switched on
    /// again, a look at once that asks it. A look that heard no source (ListenBrainz's connection reset, MusicBrainz off) keeps nothing
    /// and names both; a later open within the retry time asks nothing, one after it asks again.
    #[tokio::test]
    async fn the_recommendations_say_what_they_read_and_what_is_off() {
        use crate::app::switches::{MUSICBRAINZ, set_for_tests};
        let _state = super::super::references::TestState::set().await;
        set_for_tests(MUSICBRAINZ, false);
        let serve = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap()));
        let (mbid, t) = ("00000000-0000-4000-8000-000000000810", super::super::moment::now_secs());
        let lists = fake(|| Ok(vec![("00000000-0000-4000-8000-000000000811".into(), "Glass Harbour".into(), 0.9)]));
        let first = answer(&serve, "Kite Theory", mbid, t, lists.clone()).await.unwrap();
        assert_eq!((first.looking, first.reading, first.no_list.clone()), (true, Some("listenbrainz"), None), "a look runs, asking ListenBrainz first");
        let off = ended(&serve, "Kite Theory", mbid, t, &lists).await;
        assert_eq!(off.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Glass Harbour"], "ListenBrainz still read");
        let shown = serde_json::to_value(&off).unwrap();
        assert_eq!(shown["no_list"], serde_json::json!([{ "source": "musicbrainz", "said": "off" }]), "{shown}");
        assert!(shown.get("reading").is_none() && shown["looking"] == false, "{shown}");
        assert_eq!(*lists.asked.lock().unwrap(), ["listenbrainz"], "MusicBrainz asked nothing, nor Bandcamp through its links");
        assert!(!answer(&serve, "Kite Theory", mbid, t + 60, lists.clone()).await.unwrap().looking, "not looked again while it stays off");
        // Switched on again: a look at once, MusicBrainz asked.
        set_for_tests(MUSICBRAINZ, true);
        assert!(answer(&serve, "Kite Theory", mbid, t + 60, lists.clone()).await.unwrap().looking, "read at once");
        let on = ended(&serve, "Kite Theory", mbid, t + 60, &lists).await;
        assert_eq!(on.no_list, None, "every source answered");
        let mut names: Vec<&str> = on.artists.iter().map(|a| a.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["Glass Harbour", "Salt Meridian"]);
        assert_eq!(*lists.asked.lock().unwrap(), ["listenbrainz", "listenbrainz", "musicbrainz", "bandcamp", "bandcamp"]);
        // A look that heard no source: nothing kept, both named; asked again only after the retry time.
        set_for_tests(MUSICBRAINZ, false);
        let unheard = "00000000-0000-4000-8000-000000000813";
        let reset = fake(|| Err(anyhow::anyhow!("error sending request: Connection reset by peer (os error 104)")));
        assert!(answer(&serve, "Paper Lantern", unheard, t, reset.clone()).await.unwrap().looking);
        let none = ended(&serve, "Paper Lantern", unheard, t, &reset).await;
        assert!(none.artists.is_empty());
        assert_eq!(
            serde_json::to_value(&none).unwrap()["no_list"],
            serde_json::json!([{ "source": "listenbrainz", "said": "unanswered" }, { "source": "musicbrainz", "said": "off" }])
        );
        assert!(serve.userdata.related_artists(unheard).unwrap().is_none(), "nothing kept");
        assert!(!answer(&serve, "Paper Lantern", unheard, t + 60, reset.clone()).await.unwrap().looking, "not asked again within the retry time");
        let retry = super::super::discover::LIST_RETRY_SECS;
        assert!(answer(&serve, "Paper Lantern", unheard, t + retry + 60, reset.clone()).await.unwrap().looking, "a later open asks again");
        ended(&serve, "Paper Lantern", unheard, t + 60, &reset).await;
        assert_eq!(*reset.asked.lock().unwrap(), ["listenbrainz", "listenbrainz"]);
    }
}
