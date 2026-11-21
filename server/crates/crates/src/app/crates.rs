//! Crates (decision 0024): the built-in ones and one's own, Sounds like, the fits and the taste that rank them, Home's
//! crates and what is excluded everywhere (decisions 0083, 0094). Worked out over the playlist service, which keeps their
//! stores, the journal and its Undo, the favourites heard and the rows (SP3-U: split from it; the crates use the
//! playlists, never the reverse).

// An artist Out everywhere, on the one command path (GF).
pub mod out;

use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;

use super::ToggleError;
use super::playlists::{PlaylistRow, PlaylistService, account_of, asking_account, asking_user, songs_by_line};
use std::collections::{HashMap, HashSet};

use crate::crates::{self, CrateDef, by_fit};
use cratarr_rules::names::{name_key, natural_keys};
use crate::journal::Change;
#[cfg(test)]
use super::playlists_base::natural;

/// What crates are worked out on (see `CrateService::crate_library`).
/// A page of a crate's rows: the crate's size, and (on the first page) its genres, most shared first.
#[derive(Serialize)]
pub struct CratePage {
    pub source: String,
    pub total: usize,
    /// The most rows this page could hold (the limit asked for, within what the server serves a page): a page with
    /// fewer is the crate's last.
    pub limit: usize,
    pub rows: Vec<PlaylistRow>,
    pub genres: Vec<String>,
    /// What it left out, and why (decision 0095): the libraries switched off, its toggles, what is excluded everywhere,
    /// each condition of the filter on it.
    pub reasons: Vec<crates::Reason>,
    /// How many it would hold with every cause lifted (its left-out list kept: that is the crate's own).
    pub all_back: usize,
    /// What keeps each track out, by its `entry`, when asked (`explain`; decision 0095, amended 2026-11-02).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<crates::Blocked>,
    /// A word's crate (`genre:`, `style:`, `mood:`, `tag:`): the word and every source that says it (decision 0097).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word: Option<super::words::WordOf>,
}

/// A Sounds like curve (lib/LikeCurve): how alike the closest are, closest first, and the profile at every step its
/// handle can stop at.
#[derive(Debug, serde::Serialize)]
pub struct LikeCurve {
    pub alike: Vec<f64>,
    pub profiles: Vec<LikeStep>,
}

#[derive(Debug, serde::Serialize)]
pub struct LikeStep {
    pub count: usize,
    pub profile: crates::Profile,
}

/// What sounds like a track or an album (`like:<item>[@<count>]`, `crates::like_crate`): named after it, an album's
/// seeds its songs. An artist's too (0.11, quick crates: `ar-…`, seeded by their id, their own tracks left out).
async fn like_crate(served: Option<Arc<crate::serve::Serve>>, id: &str, item: &str, count: usize) -> Result<CrateDef, ToggleError> {
    let served = served.ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
    if item == crates::WEEK {
        let me = asking_user(Some(&*served)).ok_or_else(|| ToggleError::Refused("Sounds like your week is an account's own: sign in.".into()))?;
        let now = super::moment::now_secs();
        let since = now - crates::WEEK_SECS;
        // The history's week (decision 0064): how often and how lately each was heard; skipped ones left out too. Until
        // the log reaches back a week: the songs last played within it, alike.
        let week = crate::run::spawn_blocking(move || -> Result<Vec<(String, f32)>> {
            if let Some(heard) = super::history::since(&served.userdata, &me, since, now)? {
                return Ok(heard);
            }
            // One's own last plays: none heard while following (the owner, 2026-10-30).
            let played = served.userdata.used(&me, crate::userdata::Kind::Song)?;
            let mut week: Vec<(String, f32)> =
                played.into_iter().filter(|(_, a)| a.own_date.is_some_and(|d| d >= since)).map(|(song, _)| (song, 1.0)).collect();
            week.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(week)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        return Ok(crates::week_crate(id, week, count));
    }
    if item == crates::NOW {
        let m = super::moment::now();
        return Ok(crates::moment_crate(id, &m.name, m.target, count));
    }
    let item = item.to_string();
    let found = crate::run::spawn_blocking(move || -> Result<Option<(String, Vec<String>)>> {
        if crate::serve::ids::track_of(&item).is_some() {
            return Ok(served.library.songs_by_id(std::slice::from_ref(&item))?.pop().map(|s| (s.title, vec![s.id])));
        }
        if item.starts_with(crate::serve::ids::ARTIST) {
            return Ok(served.library.artist(&item)?.map(|a| (a.name, vec![item.clone()])));
        }
        Ok(served.library.album(&item)?.map(|a| (a.name, a.song.unwrap_or_default().into_iter().map(|s| s.id).collect())))
    })
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))??;
    let (name, songs) = found.ok_or(ToggleError::UnknownPlaylist)?;
    Ok(crates::like_crate(id, &format!("Sounds like {name}"), songs, count))
}

/// What a user did with each line (the crates' Play count, Last played and Rating): the songs' plays and last plays,
/// and their stars: a song's own, else its album's (the filter API, 2026-10-29: one Rating, as the Library's tracks'
/// 4 stars and up reads it), by the files' lines. Blocking.
fn uses_of(serve: Option<&crate::serve::Serve>, user: &str, files: &[crates::Facts]) -> anyhow::Result<HashMap<String, crates::Use>> {
    let Some(serve) = serve else { return Ok(HashMap::new()) };
    let line_of: HashMap<i64, &str> = files.iter().filter_map(|f| Some((f.track?, f.rel.as_str()))).collect();
    let line = |song: &str| crate::serve::ids::track_of(song).and_then(|t| line_of.get(&t).copied());
    let mut out: HashMap<String, crates::Use> = HashMap::new();
    for (song, (plays, last)) in serve.userdata.song_plays(user)? {
        if let Some(l) = line(&song) {
            let u = out.entry(l.to_string()).or_default();
            (u.plays, u.last) = (plays, last);
        }
    }
    let stars = |kind| -> anyhow::Result<HashMap<String, u8>> {
        Ok(serve.userdata.used(user, kind)?.into_iter().filter(|(_, a)| a.rating > 0).map(|(id, a)| (id, a.rating)).collect())
    };
    // Skips as the taste counts them (`sources_in`): plays that ended skipped before they counted.
    for l in super::history::own(serve.userdata.heard_since(user, 0)?) {
        if l.ended == crate::userdata::Ended::Skipped
            && !l.counted
            && let Some(line) = line(&l.song)
        {
            out.entry(line.to_string()).or_default().skips += 1;
        }
    }
    let (own, rated) = (stars(crate::userdata::Kind::Song)?, stars(crate::userdata::Kind::Album)?);
    if !own.is_empty() || !rated.is_empty() {
        for (song, album) in serve.library.song_albums()? {
            if let (Some(r), Some(l)) = (own.get(&song).or_else(|| rated.get(&album)), line(&song)) {
                out.entry(l.to_string()).or_default().rating = Some(*r);
            }
        }
    }
    Ok(out)
}

/// How much of an album's length heard makes it no longer new to a listener (the owner, 2026-10-23: "5%").
const HEARD_SHARE: f64 = 0.05;

/// How many each of Home's looks back shows; and after how many days unplayed an album is half as far from the listening
/// as one never played (Grave digging).
const DIG: usize = 12;

/// New for you (`new_for_you`), Home's shelf and its crate alike: how many albums it keeps, at most how many of one album
/// artist (the owner, 2026-10-30: "only show the top 3 albums for each album artist"), how many of the newest it reads
/// at a time, and how far down the newest it reads at most.
const NEW_ALBUMS: usize = 12;

const NEW_PER_ARTIST: usize = 3;

const NEW_PAGE: usize = 60;

const NEW_FURTHEST: usize = 300;

const UNPLAYED_HALF_DAYS: f64 = 180.0;

/// Whether tracks, each its length and how often it was heard, were heard `HEARD_SHARE` of their length.
fn heard_share(tracks: impl IntoIterator<Item = (u64, u32)>) -> bool {
    let (length, played) = tracks.into_iter().fold((0u64, 0u64), |(l, p), (secs, plays)| (l + secs, p + secs * u64::from(plays)));
    length > 0 && played as f64 >= HEARD_SHARE * length as f64
}

/// Whether one of Cratarr's own albums was heard (`heard_share`), by these plays (a song's own count first); `None`
/// for an album the library does not have. Blocking.
fn album_heard(serve: &crate::serve::Serve, plays: &HashMap<String, (u32, Option<i64>)>, id: &str) -> Result<Option<bool>, ToggleError> {
    let Some(album) = serve.library.album(id)? else { return Ok(None) };
    let songs = album.song.unwrap_or_default();
    Ok(Some(heard_share(songs.iter().map(|s| (s.duration, plays.get(&s.id).map_or(0, |p| p.0))))))
}

/// New for you's walk down the newest albums (`new_for_you`): `page` gives `NEW_PAGE` of them from an offset, newest
/// first, each its id and album artist; `out` says which to leave out. At most `NEW_PER_ARTIST` of one album artist,
/// matched loosely (`import::loose`: case, spacing, a leading "the"; an album without one counts toward none), and
/// `NEW_ALBUMS` in all, read on page by page until those are found, a page comes back short, or `NEW_FURTHEST` are read.
/// A compilation counts toward no artist's share (the owner, 2026-10-30: "No cap for compilations"): each on its own.
/// An album whose album artist is one of `artists_out` (excluded everywhere, decision 0083) is left out too, counted
/// toward no one.
fn new_picks(
    mut page: impl FnMut(usize) -> Result<Vec<(String, String, bool)>, ToggleError>,
    mut out: impl FnMut(&str) -> Result<bool, ToggleError>,
    artists_out: &HashSet<String>,
) -> Result<Vec<String>, ToggleError> {
    let mut picks = Vec::new();
    let mut of: HashMap<String, usize> = HashMap::new();
    let mut offset = 0;
    while offset < NEW_FURTHEST {
        let albums = page(offset)?;
        let read = albums.len();
        for (id, artist, compilation) in albums {
            let artist = artist.trim();
            if super::excluded_artists::credits_excluded(&[artist], artists_out) {
                continue;
            }
            let key = if compilation || artist.is_empty() || artist == crate::serve::library::UNKNOWN_ARTIST { String::new() } else { super::import::loose(artist) };
            // An artist with their share already: the walk does not ask whether this one was heard.
            if !key.is_empty() && of.get(&key).is_some_and(|&n| n >= NEW_PER_ARTIST) {
                continue;
            }
            if out(&id)? {
                continue;
            }
            if !key.is_empty() {
                *of.entry(key).or_default() += 1;
            }
            picks.push(id);
            if picks.len() == NEW_ALBUMS {
                return Ok(picks);
            }
        }
        if read < NEW_PAGE {
            break;
        }
        offset += read;
    }
    Ok(picks)
}

/// An account's taste as Insights' Profile has it (`taste::profiles`): its favourites, playlists, plays and skips, and
/// what it excluded, on `lib`'s libraries switched on; a source the account switched off adds nothing (decision 0094,
/// `taste::off_of`). Blocking.
fn taste_in(lib: &CrateLibrary, ud: &crate::userdata::UserData, user: &str, excluded: &[String]) -> Result<crate::similarity::Profile, ToggleError> {
    Ok(super::taste::profiles(lib.alike(), &sources_in(lib, ud, user, excluded)?, &super::taste::off_of(ud, user)).0)
}

/// An account's profile of some of its taste's sources (`taste_in`, `skipped_in`). Blocking.
type ProfileOf = fn(&CrateLibrary, &crate::userdata::UserData, &str, &[String]) -> Result<crate::similarity::Profile, ToggleError>;

/// What an account skips, as a profile (decision 0094: skips keep tracks away in Radio and Autoplay): the avoid
/// profile of its skips alone (excluding is a hard no of its own), none while its skipped source is off. Blocking.
fn skipped_in(lib: &CrateLibrary, ud: &crate::userdata::UserData, user: &str, excluded: &[String]) -> Result<crate::similarity::Profile, ToggleError> {
    let mut off = super::taste::off_of(ud, user);
    off.push("excluded");
    Ok(super::taste::profiles(lib.alike(), &sources_in(lib, ud, user, excluded)?, &off).1)
}

/// An account's taste's sources (`taste::Sources`), on `lib`'s libraries switched on. Blocking.
fn sources_in(lib: &CrateLibrary, ud: &crate::userdata::UserData, user: &str, excluded: &[String]) -> Result<super::taste::Sources, ToggleError> {
    let alike = lib.alike();
    let place = |song: &str| crate::serve::ids::track_of(song).and_then(|t| alike.place_of_track(t)).filter(|&i| lib.on(i));
    let starred = ud.starred(user, crate::userdata::Kind::Song)?;
    let mut skips: HashMap<usize, u32> = HashMap::new();
    let mut played: HashMap<usize, u32> = HashMap::new();
    for l in super::history::own(ud.heard_since(user, 0)?) {
        let Some(i) = place(&l.song) else { continue };
        if l.ended == crate::userdata::Ended::Skipped && !l.counted {
            *skips.entry(i).or_default() += 1;
        } else if l.counted || l.ended == crate::userdata::Ended::Finished {
            *played.entry(i).or_default() += 1;
        }
    }
    Ok(super::taste::Sources {
        favourites: starred.iter().filter_map(|s| place(s)).collect(),
        playlists: lib.membership().iter().filter_map(|(line, lists)| Some((alike.place_of_line(line).filter(|&i| lib.on(i))?, lists.len()))).collect(),
        plays: played.into_iter().collect(),
        excluded: excluded.iter().filter_map(|song| place(song)).collect(),
        skipped: skips.into_iter().collect(),
    })
}

/// Whether an album's album artist is excluded everywhere (decision 0083, the owner 2026-10-30): its artist as shown,
/// or any of its album artists, by any credit.
fn album_out(a: &crate::serve::api::Album, artists: &HashSet<String>) -> bool {
    use super::excluded_artists::credits_excluded;
    credits_excluded(&[a.artist.as_str()], artists) || credits_excluded(&a.artists.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), artists)
}

/// The songs of these files that credit an artist excluded everywhere (decision 0083), by their ids: what Radio and
/// Autoplay, the shuffles and Subsonic's random songs leave out.
fn songs_crediting(facts: &[crates::Facts], artists: &HashSet<String>) -> Vec<String> {
    facts.iter().filter(|f| crates::credits_out(f, artists)).filter_map(|f| Some(crate::serve::ids::song(f.track?))).collect()
}

/// An artist a crate leaves out (decision 0083: Not in this crate), as its view lists them: their key (what takes it
/// back) and their name.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NotArtist {
    pub key: String,
    pub name: String,
}

/// How many albums Home's moment shows, drawn from how many of the nearest; how many of an album's tracks must be
/// measured (and at least half of them); how long one played is left out (days).
const MOMENT_ALBUMS: usize = 10;

const MOMENT_POOL: usize = 30;

const MOMENT_TRACKS: u32 = 3;

const MOMENT_RESTED_DAYS: i64 = 14;

/// A ranked crate's fit as its Not for sheet shows it (`CrateService::crate_fit`).
#[derive(Debug, serde::Serialize)]
pub struct FitView {
    pub learned: Vec<FitTrait>,
    pub rules: Vec<crates::Condition>,
    pub turned_down: usize,
}

/// A ranked crate's fit in the list of them all (`CrateService::crate_fits`).
#[derive(Debug, serde::Serialize)]
pub struct FitSummary {
    #[serde(rename = "crate")]
    pub crate_id: String,
    pub name: String,
    /// What it sounds like ("Cobalt Hour", "your week"): its Not for sheet's title is "Not for" it.
    pub of: String,
    pub avoiding: Vec<crates::Trait>,
    pub rules: Vec<crates::Condition>,
    pub turned_down: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct FitTrait {
    #[serde(rename = "trait")]
    pub r#trait: crates::Trait,
    pub key: String,
    pub dismissed: bool,
}

/// Home's moment (`CrateService::moment`): its name, its albums (Cratarr's own ids), and the weather it was made of.
#[derive(Debug, serde::Serialize)]
pub struct MomentView {
    pub name: String,
    /// The sky Home paints behind it, and whether after sunset (`moment::Moment`).
    pub sky: &'static str,
    pub dark: bool,
    pub albums: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weather: Option<super::moment::Weather>,
}

/// Home's looks back (`CrateService::digging`).
#[derive(Debug, serde::Serialize)]
pub struct Digging {
    pub past: Vec<DugTrack>,
    pub graves: Vec<DugAlbum>,
}

/// A favourite track and when it was last played (epoch s).
#[derive(Debug, serde::Serialize)]
pub struct DugTrack {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub album: String,
    pub album_id: String,
    pub last_played: i64,
}

/// An album (Cratarr's own) and when anything of it was last played (epoch s; none: never).
#[derive(Debug, serde::Serialize)]
pub struct DugAlbum {
    pub id: String,
    pub last_played: Option<i64>,
}

/// The one similarity's library over these files and playlists, made once and shared (0.9.7: each of Home's
/// requests made its own over every file, three at a time on each Home, and the sim went past 768 MB). Made again
/// when the files (the crates' facts, read again) or the playlists' members change.
fn shared_alike(files: &Arc<Vec<crates::Facts>>, membership: &HashMap<String, Vec<String>>) -> Arc<crates::Alike> {
    use std::hash::{Hash, Hasher};
    static KEPT: std::sync::Mutex<Option<(usize, u64, Arc<crates::Alike>)>> = std::sync::Mutex::new(None);
    let mut members: Vec<(&String, &Vec<String>)> = membership.iter().collect();
    members.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let mut h = std::collections::hash_map::DefaultHasher::new();
    members.hash(&mut h);
    let key = (Arc::as_ptr(files) as usize, h.finish());
    let mut kept = KEPT.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((p, m, a)) = kept.as_ref()
        && (*p, *m) == key
    {
        return a.clone();
    }
    // The one before let go first: two at once over every file is what the limit cannot hold.
    *kept = None;
    let known = crate::serve::instance().and_then(|s| s.artist_ids().inspect_err(|e| tracing::warn!(reason = %format!("{e:#}"), "the artists' ids could not be read: no artist seeds")).ok());
    let made = Arc::new(crates::Alike::of(files, membership, &crates::artist_ids(files, &known.unwrap_or_default())));
    *kept = Some((key.0, key.1, made.clone()));
    made
}

pub struct CrateLibrary {
    pub source: String,
    /// Every library's files (`Catalogue::crate_facts`); `files()` leaves out those switched off.
    files: Arc<Vec<crates::Facts>>,
    /// The catalogue folders of the libraries switched off (decision 0024, amended 2026-10-23).
    switched_off: HashSet<i64>,
    membership: HashMap<String, Vec<String>>,
    favourites: Option<HashSet<String>>,
    /// The lines of the viewer's favourite albums (decision 0024, amended 2026-10-19), when asked for.
    favourite_albums: Option<HashSet<String>>,
    /// The library as the one similarity sees it, made when a Sounds like first asks (`crates::Alike`), shared by
    /// every request over the same files and playlists (`shared_alike`).
    alike: std::sync::OnceLock<Arc<crates::Alike>>,
    /// What the viewer did with each line (plays, last played, rating: the crates' listener conditions); `None` when
    /// no one asks (Home Assistant, voice).
    uses: Option<HashMap<String, crates::Use>>,
    /// What makes music alike to the viewer: their Similar by (the radio's), else all signals alike.
    by: crate::similarity::Weights,
    /// The lines that fit the viewer's taste (Fits your taste), when a condition asks (`crate_library_for`).
    fits: Option<HashSet<String>>,
}

impl CrateLibrary {
    /// The library as the one similarity sees it (`crates::Alike`): every file, any library, with its playlists.
    pub(super) fn alike(&self) -> &crates::Alike {
        self.alike.get_or_init(|| shared_alike(&self.files, &self.membership))
    }

    /// The viewer's Similar by.
    pub(super) fn by(&self) -> &crate::similarity::Weights {
        &self.by
    }

    /// Whether the file at a place (`alike`'s, the files' order) is in a library switched on.
    pub(super) fn on(&self, i: usize) -> bool {
        self.files.get(i).is_some_and(|f| crates::on_for(&f.folders, &self.switched_off))
    }

    /// Whether these conditions can be worked out: what they need of the favourites is known.
    fn knows(&self, conditions: &[crates::Condition]) -> bool {
        (!crates::needs_favourites(conditions) || self.favourites.is_some())
            && (!crates::needs_favourite_albums(conditions) || self.favourite_albums.is_some())
    }

    /// The files of the libraries switched on, newest first.
    pub fn files(&self) -> impl Iterator<Item = &crates::Facts> {
        self.files.iter().filter(|f| crates::on_for(&f.folders, &self.switched_off))
    }

    /// This library as the rules work a crate out over it.
    fn view<'a>(&'a self) -> crates::View<'a, impl Fn() -> &'a crates::Alike> {
        crates::View { files: &self.files, switched_off: &self.switched_off, world: self.world(), by: &self.by, alike: || self.alike() }
    }

    /// What the axes test about the library as a whole.
    pub fn world(&self) -> crates::World<'_> {
        crates::World {
            membership: &self.membership,
            favourites: self.favourites.as_ref(),
            favourite_albums: self.favourite_albums.as_ref(),
            uses: self.uses.as_ref(),
            now: super::moment::now_secs(),
            fits: self.fits.as_ref(),
        }
    }

    /// Which of one's own playlists hold each line.
    pub fn membership(&self) -> &HashMap<String, Vec<String>> {
        &self.membership
    }

    /// What the viewer did with each line, when known.
    pub fn uses(&self) -> Option<&HashMap<String, crates::Use>> {
        self.uses.as_ref()
    }

    /// The conditions with their Sounds like worked out (`crates::View::resolved`), once for all the files.
    pub fn resolved(&self, conditions: &[crates::Condition]) -> Vec<crates::Condition> {
        self.view().resolved(&one_spelling(conditions), false)
    }

    /// The crate's lines, newest first; a Sounds like crate's closest first (`crates::closest_first`).
    pub fn select(&self, conditions: &[crates::Condition]) -> Vec<String> {
        self.view().select(&one_spelling(conditions), false)
    }

    /// `crates::View::select_saying`: `without`, the libraries switched off on the page, this source's of them.
    fn select_saying(&self, conditions: &[crates::Condition], causes: &[crates::Cause], without: &[String], explain: bool) -> (Vec<String>, Vec<crates::Reason>, usize, Option<crates::Judged>) {
        let mine: Vec<String> = without.iter().filter(|k| k.starts_with(&format!("{}/", self.source))).cloned().collect();
        self.view().select_saying(&one_spelling(conditions), causes, &mine, explain)
    }
}

/// What keeps each track out (`crates::Judged`), the page's search (`queries`, any of them: lib/search's, on the row's
/// title or file, artist and album) one more cause when it asks one; `passing` the lines the crate shows, `rows` each
/// line's row.
fn what_blocks(judged: crates::Judged, passing: &[String], queries: &[String], rows: &HashMap<String, PlaylistRow>) -> crates::Blocked {
    let queries: Vec<&str> = queries.iter().map(|q| q.trim()).filter(|q| !q.is_empty()).collect();
    if queries.is_empty() {
        return judged.blocked();
    }
    let found = |line: &str| rows.get(line).is_some_and(|r| words_find(&queries, &[r.name.as_deref().unwrap_or(&r.file), r.artist.as_deref().unwrap_or(""), r.album.as_deref().unwrap_or("")]));
    judged.searched(passing, |line| !found(line)).blocked()
}

/// Whether any of `queries` meets these texts as the page's search meets a row (lib/search `matches`, lib/matcher's
/// TextMatcher: the texts not blank, all as one name and each also its own).
fn words_find(queries: &[&str], texts: &[&str]) -> bool {
    use cratarr_rules::matcher::{Index, Name};
    let kept: Vec<String> = texts.iter().filter(|t| !t.trim().is_empty()).map(|t| t.to_string()).collect();
    if kept.is_empty() {
        return false;
    }
    let aliases = if kept.len() > 1 { kept.clone() } else { Vec::new() };
    let index = Index::new(vec![Name { text: kept.join(" "), aliases }]);
    queries.iter().any(|q| !index.query(q, 1).is_empty())
}

/// Conditions on words as the store spells them (decision 0097, rule 4: one spelling everywhere): "Hip-Hop" or "hip
/// hop" asked finds the Hip Hop the tracks carry. A word the store does not know is asked as written.
fn one_spelling(conditions: &[crates::Condition]) -> Vec<crates::Condition> {
    let Some(stored) = super::words::stored_now() else { return conditions.to_vec() };
    conditions
        .iter()
        .map(|c| {
            let mut c = c.clone();
            let axis = c.axis.clone();
            if let crates::Value::AnyOf { values } | crates::Value::AllOf { values } = &mut c.value {
                for v in values.iter_mut() {
                    if let Some(w) = stored.word(&axis, v) {
                        *v = w.word;
                    }
                }
            }
            c
        })
        .collect()
}

/// How a crate is looked at, besides its definition: its two toggles as the page shows them (a tap may be on its way;
/// `None`: as saved), the libraries switched off on the page (`source/library` keys; decision 0024, amended
/// 2026-10-23: a crate draws from every library switched on), and the filter on it (decision 0061: conditions,
/// unsaved, on the list in view). The default is the crate as saved, from every library, unfiltered: what Home
/// Assistant, voice and Play On get.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
pub struct CrateLook {
    #[serde(default)]
    pub favourites: Option<crates::Favourites>,
    #[serde(default)]
    pub favourite_albums: Option<crates::Favourites>,
    #[serde(default)]
    pub without: Vec<String>,
    #[serde(default)]
    pub filter: Vec<crates::Condition>,
    /// The order its list shows (decision 0060), applied before its rows are paged, and to what Play plays; asked
    /// for none, Best match (`CrateOrder::opened`).
    #[serde(default = "CrateOrder::opened")]
    pub order: CrateOrder,
    #[serde(default)]
    pub down: bool,
    /// What keeps each track out, asked for with the page (decision 0095, amended 2026-11-02): only when what it
    /// shows is empty.
    #[serde(default)]
    pub explain: bool,
    /// The page's search, its queries (any of them), judged as one more cause with `explain`.
    #[serde(default)]
    pub search: Vec<String>,
}

/// The orders a crate's list takes (decision 0060), as the page names them: its own (newest first), by a field, or by
/// fit to the asking account's taste (Best match, decision 0094); `down` turns it round.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CrateOrder {
    #[default]
    Own,
    Title,
    Artist,
    AlbumArtist,
    Album,
    Year,
    Track,
    Length,
    Tempo,
    Plays,
    LastPlayed,
    Added,
    BestMatch,
}

impl CrateOrder {
    /// What every crate opens on when the page or a play names no order (the owner, 2026-11-02, amending decision
    /// 0094: "Every crate"; there is no crate order of its own to show). `Default` stays the crate as saved, newest
    /// first: what Home's shelf, a download, Home Assistant and voice take, asking no one's taste.
    pub fn opened() -> Self {
        CrateOrder::BestMatch
    }
}

/// Rows in an order of the page's list (decision 0060), as lib/PlaylistView orders a playlist's: names by `natural`;
/// numbers with one a row does not have first (last, turned round); plays counting none as 0; equals keeping the
/// crate's own order (the sort is stable). Its own order turned round is the rows reversed.
fn sort_rows(rows: &mut Vec<PlaylistRow>, by: CrateOrder, down: bool) {
    let way = |o: std::cmp::Ordering| if down { o.reverse() } else { o };
    let text = |r: &PlaylistRow| -> String {
        match by {
            CrateOrder::Title => r.name.clone().unwrap_or_else(|| r.file.clone()),
            CrateOrder::Artist => r.artist.clone().unwrap_or_default(),
            CrateOrder::AlbumArtist => r.album_artist.clone().unwrap_or_default(),
            _ => r.album.clone().unwrap_or_default(),
        }
    };
    let number = |r: &PlaylistRow| -> Option<f64> {
        match by {
            CrateOrder::Length => r.duration_secs,
            CrateOrder::Tempo => r.bpm.map(f64::from),
            CrateOrder::Year => r.year.map(f64::from),
            CrateOrder::Track => r.track.map(|t| f64::from(r.disc.unwrap_or(1)) * 10_000.0 + f64::from(t)),
            CrateOrder::Plays => Some(f64::from(r.play_count.unwrap_or(0))),
            CrateOrder::LastPlayed => r.last_played.map(|t| t as f64),
            _ => r.added.map(|t| t as f64),
        }
    };
    match by {
        CrateOrder::Own if down => rows.reverse(),
        CrateOrder::Own => {}
        CrateOrder::Title | CrateOrder::Artist | CrateOrder::AlbumArtist | CrateOrder::Album => {
            // Each name keyed once (thousands of rows), the rows then put in their keys' order.
            let keys: Vec<Vec<char>> = rows.iter().map(|r| name_key(&text(r))).collect();
            let mut order: Vec<usize> = (0..rows.len()).collect();
            order.sort_by(|&a, &b| way(natural_keys(&keys[a], &keys[b])));
            let mut taken: Vec<Option<PlaylistRow>> = rows.drain(..).map(Some).collect();
            rows.extend(order.into_iter().filter_map(|i| taken[i].take()));
        }
        _ => rows.sort_by(|a, b| way(number(a).partial_cmp(&number(b)).unwrap_or(std::cmp::Ordering::Equal))),
    }
}

/// A crate as the Crates page lists it (decision 0024); `count` is `None` when not known (the server's favourites could
/// not be read).
#[derive(Debug, Serialize)]
pub struct CrateView {
    pub id: String,
    pub name: String,
    /// Its favourites toggle's setting (decision 0024, amended).
    pub favourites: crates::Favourites,
    /// Its favourite albums toggle's setting (decision 0024, amended 2026-10-19); `counts` are counted with it.
    pub favourite_albums: crates::Favourites,
    /// How many of its tracks were left out by hand (decision 0024, amended 2026-10-19); `counts` leave them out.
    pub left_out: usize,
    /// How many tracks it has with each setting of the toggle (the page shows the one it is set to, and the next at
    /// once on a tap); `None` where the favourites are not known (no server connected).
    pub counts: CrateCounts,
    pub permanent: bool,
    /// What its card shows, as its favourites toggle is set; `None` where that is not known (favourites not read).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<CrateStats>,
    /// A picture of its own was set (`GET /api/crates/{id}/image`).
    pub image: bool,
}

/// A crate's card (the owner's, 2026-10-16): its size and length, how much of it was never played, what came this week,
/// its top genres and a few of its albums' covers.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct CrateStats {
    pub tracks: usize,
    pub seconds: u64,
    /// Never played by the owner (Cratarr's own plays); `None` while there is no owner to ask about.
    pub unplayed: Option<usize>,
    /// Came in the last seven days.
    pub new_week: usize,
    /// Its three most common genres, most common first.
    pub genres: Vec<String>,
    /// Up to four of its albums, for a mosaic: those with most of its tracks first, then the newest.
    pub covers: Vec<CoverRef>,
}

/// An album's cover, as `/api/cover/{source}/{album_id}` serves it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CoverRef {
    pub source: String,
    pub album_id: String,
}

/// A crate's card from its lines (`now`: epoch seconds).
fn crate_stats(
    lines: &[String],
    facts: &HashMap<&str, &crates::Facts>,
    songs: &HashMap<String, crate::serve::library::RelSong>,
    played: Option<&HashSet<String>>,
    now: i64,
) -> CrateStats {
    const WEEK: i64 = 7 * 86_400;
    // What Cratarr's first look at the library found was there before it: not new, however young Cratarr is.
    let first_look = songs.values().filter_map(|s| s.created).min().unwrap_or(0) + 86_400;
    let known: Vec<&crate::serve::library::RelSong> = lines.iter().filter_map(|l| songs.get(l)).collect();
    let mut genres: Vec<(String, usize)> = Vec::new();
    for f in lines.iter().filter_map(|l| facts.get(l.as_str())) {
        for g in &f.genres {
            match genres.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(g)) {
                Some((_, n)) => *n += 1,
                None => genres.push((g.clone(), 1)),
            }
        }
    }
    genres.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    // Albums: (album, its tracks here, its newest).
    let mut albums: Vec<(&str, usize, i64)> = Vec::new();
    for s in &known {
        let at = s.created.unwrap_or(0);
        match albums.iter_mut().find(|(a, _, _)| *a == s.album) {
            Some((_, n, newest)) => {
                *n += 1;
                *newest = (*newest).max(at);
            }
            None => albums.push((&s.album, 1, at)),
        }
    }
    albums.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)).then(a.0.cmp(b.0)));
    CrateStats {
        tracks: lines.len(),
        seconds: known.iter().filter_map(|s| s.duration_ms).filter(|d| *d > 0).map(|d| d as u64).sum::<u64>() / 1000,
        unplayed: played.map(|p| known.iter().filter(|s| !p.contains(&s.song)).count()),
        new_week: known.iter().filter(|s| s.created.is_some_and(|c| c >= now - WEEK && c > first_look)).count(),
        genres: genres.into_iter().take(3).map(|(g, _)| g).collect(),
        covers: albums
            .into_iter()
            .take(4)
            .map(|(a, _, _)| CoverRef { source: crate::source::cratarr::KIND.id.to_string(), album_id: a.to_string() })
            .collect(),
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CrateCounts {
    pub all: Option<usize>,
    pub only: Option<usize>,
    pub none: Option<usize>,
}

/// A crate's lines in an order (decision 0060), by what they were worked out from, and when: its pages are asked for
/// one after the other, and each would otherwise read every row of it again.
type SortedKept = std::sync::Mutex<Option<(String, std::time::Instant, Arc<Vec<String>>)>>;

/// An account's fits to its taste, every track's by its song id, as last worked out and when (`fits_of`): a list's
/// pages and its panels ask one after the other.
type FitsKept = std::sync::Mutex<Option<(String, std::time::Instant, Arc<HashMap<String, f64>>)>>;

/// How long a sorted crate is kept: the plays it may be ordered by change as it plays.
const SORTED_KEEP: std::time::Duration = std::time::Duration::from_secs(30);

/// The lines whose album has the asking account's heart on (decision 0024, amended 2026-10-19): the Library's album
/// favourites, the viewer's own whoever's crate it is, and Cratarr's own library telling each line's album, whatever
/// server the crate is shown for. `None` when they cannot be read (no library, no account).
async fn favourite_album_lines(served: Option<Arc<crate::serve::Serve>>, cat: Arc<crate::catalogue::Catalogue>) -> Option<HashSet<String>> {
    let serve = served?;
    let me = serve.current_user()?;
    crate::run::spawn_blocking(move || {
        let albums: HashSet<String> = serve.userdata.starred(&me, crate::userdata::Kind::Album).ok()?.into_iter().collect();
        if albums.is_empty() {
            return Some(HashSet::new());
        }
        let songs = songs_by_line(&serve, &cat);
        Some(songs.into_iter().filter(|(_, s)| albums.contains(&s.album)).map(|(line, _)| line).collect())
    })
    .await
    .ok()
    .flatten()
}

/// Albums' tracks as the library has them now, by album, each by its song id: a crate's albums left out by hand (decision
/// 0024, amended 2026-10-19), a rule, so a track the album gets later is out too.
async fn tracks_of_albums(served: Option<Arc<crate::serve::Serve>>, cat: Arc<crate::catalogue::Catalogue>, albums: HashSet<String>) -> HashMap<String, Vec<String>> {
    if albums.is_empty() {
        return HashMap::new();
    }
    let Some(serve) = served else { return HashMap::new() };
    crate::run::spawn_blocking(move || {
        let mut tracks: HashMap<String, Vec<String>> = HashMap::new();
        for s in songs_by_line(&serve, &cat).into_values() {
            if albums.contains(&s.album) {
                tracks.entry(s.album).or_default().push(s.song);
            }
        }
        tracks
    })
    .await
    .unwrap_or_default()
}

/// A crate's tracks to leave out: its own left out, its albums' tracks now, and the tracks excluded everywhere.
fn left_tracks(left: &crates::LeftOut, album_tracks: &HashMap<String, Vec<String>>, excluded: &[String]) -> Vec<String> {
    if left.is_empty() {
        return excluded.to_vec();
    }
    let of_albums = left.albums.iter().filter_map(|a| album_tracks.get(a)).flatten();
    left.tracks.iter().chain(of_albums).chain(excluded).cloned().collect()
}

/// A list with `items` added (the new ones at its end, sorted) or, `add` false, taken out.
fn edited(before: &[String], items: impl IntoIterator<Item = String>, add: bool) -> Vec<String> {
    let items: HashSet<String> = items.into_iter().collect();
    let mut after = before.to_vec();
    if add {
        let mut new: Vec<String> = items.into_iter().filter(|i| !before.contains(i)).collect();
        new.sort();
        after.extend(new);
    } else {
        after.retain(|i| !items.contains(i));
    }
    after
}

/// Albums of Cratarr's own library by id (those it has).
async fn library_albums(served: Option<Arc<crate::serve::Serve>>, ids: Vec<String>) -> Result<Vec<crate::serve::api::Album>, ToggleError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let serve = served.ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
    Ok(crate::run::spawn_blocking(move || serve.library.albums_by_id(&ids)).await.map_err(|e| anyhow::anyhow!("{e}"))??)
}

/// An album left out of a crate, for its editor's list: its title and artist none once it is gone from the library.
#[derive(Debug, Clone, Serialize)]
pub struct LeftOutAlbum {
    pub id: String,
    pub title: Option<String>,
    pub artist: Option<String>,
}

/// Crates over the playlists (the crates' half of the service that held both, SP3-U): what Home, the Crates page, Play,
/// Radio, search and the devices ask of crates. App builds it on the playlist service.
pub struct CrateService {
    /// The playlists, the crates' stores, the journal and its Undo, the favourites heard, the rows: what the crates are
    /// worked out over.
    playlists: Arc<PlaylistService>,
    /// The last crate put in an order other than its own (see `crate_in_order`).
    sorted: SortedKept,
    /// The last account's fits to its taste (see `fits_of`).
    fits: FitsKept,
}

/// The tests here read the playlist service's own through the crate service, as they did when it was one.
#[cfg(test)]
impl std::ops::Deref for CrateService {
    type Target = PlaylistService;

    fn deref(&self) -> &PlaylistService {
        &self.playlists
    }
}

impl CrateService {
    pub fn new(playlists: Arc<PlaylistService>) -> Self {
        Self { playlists, sorted: SortedKept::default(), fits: FitsKept::default() }
    }

    /// The playlist service the crates are worked out over.
    pub fn playlists(&self) -> &Arc<PlaylistService> {
        &self.playlists
    }

    /// Ask the media server for the favourites once, at start, when a crate shows by them: the first visit to Crates
    /// then need not wait the minute that may take.
    pub async fn warm_favourites(&self) {
        if let Err(e) = self.favourites_into_conditions().await {
            tracing::warn!(target: "library", reason = %e, "own crates' toggles not written into their conditions");
        }
        let first = self.playlists.crate_stores().first().clone();
        let Ok(set) = first.all_favourites().await else { return };
        let needed = crates::permanent().into_iter().chain(first.list().await.unwrap_or_default()).any(|c| {
            crates::needs_favourites(&c.conditions) || set.get(&c.id).copied().unwrap_or(crates::Favourites::All) != crates::Favourites::All
        });
        if let (true, Ok((sid, source))) = (needed, self.playlists.source_of(None)) {
            self.playlists.favourite_lines(&sid, &source).await;
        }
    }

    /// What every account keeps in its crates follows ids a take moved (`Serve::follow_taken`): lists, seeds and all that goes by
    /// a Sounds like's id, its picture, and what an Undo holds them against. Each changes nothing the second time. One store that
    /// cannot be read stops no other, and says so: the next check goes over them all.
    pub async fn follow_tracks(&self, moved: &crate::moved::Moved) -> Result<()> {
        let mut followed = Ok(());
        for (account, store) in self.playlists.crate_stores().every() {
            match store.follow(moved).await {
                Ok(false) => {}
                Ok(true) => self.playlists.excluded_tracks_moved(account.as_deref()),
                Err(e) => followed = followed.and(Err(e)),
            }
        }
        followed?;
        if let Some(pictures) = self.crate_images() {
            moved.images(&pictures, &mut Default::default())?;
        }
        self.playlists.journal().move_changes(moved)?;
        Ok(())
    }

    /// Every crate (the permanent ones first, then one's own) with how many tracks it holds now (decision 0024), from
    /// the libraries not in `without` (`source/library` keys: those switched off on the page).
    /// Every track in one of the account's own crates (not Unlisted: the one that is about what no playlist has), as
    /// each shows it (its favourites settings, what it leaves out), from the libraries not in `without`: Insights'
    /// Listed (the owner, 2026-10-23).
    pub async fn crated_lines(&self, want: Option<&str>, without: &[String]) -> Result<HashSet<String>, ToggleError> {
        let own = self.own_crates()?.list().await?;
        if own.is_empty() {
            return Ok(HashSet::new());
        }
        let set = self.own_crates()?.all_favourites().await?;
        let albums_set = self.own_crates()?.all_favourite_albums().await?;
        let left_set = self.own_crates()?.all_left_out().await?;
        let album_tracks = tracks_of_albums(self.playlists.served(), self.playlists.lib().catalogue(), left_set.values().flat_map(|l| l.albums.iter().cloned()).collect()).await;
        let excluded = self.own_crates()?.excluded().await?;
        let artists = self.artists_out().await?;
        let none = crates::LeftOut::default();
        // An own crate's toggles are its conditions (one truth): only a built-in's are kept apart.
        let favourites_of = |id: &str| if crates::is_fixed(id) { set.get(id).copied().unwrap_or(crates::Favourites::All) } else { crates::Favourites::All };
        let albums_of = |id: &str| if crates::is_fixed(id) { albums_set.get(id).copied().unwrap_or(crates::Favourites::All) } else { crates::Favourites::All };
        let needed = own.iter().any(|c| crates::needs_favourites(&c.conditions) || favourites_of(&c.id) != crates::Favourites::All);
        let needed_albums = own.iter().any(|c| crates::needs_favourite_albums(&c.conditions) || albums_of(&c.id) != crates::Favourites::All);
        let lib = self.crate_library(want, needed, needed_albums, without).await?;
        let mut lines = HashSet::new();
        for c in &own {
            let gone = left_tracks(left_set.get(&c.id).unwrap_or(&none), &album_tracks, &excluded);
            let shown = crates::leaving_out_artists(&artists, &crates::leaving_out(&gone, &crates::shown(&c.conditions, favourites_of(&c.id), albums_of(&c.id))));
            if lib.knows(&shown) {
                lines.extend(lib.select(&shown));
            }
        }
        Ok(lines)
    }

    pub async fn crates(&self, want: Option<&str>, without: &[String]) -> Result<Vec<CrateView>, ToggleError> {
        let own = self.own_crates()?.list().await?;
        // The built-in crates (0.9.3, the owner's word): Home's that are crates beside the permanent Unlisted, each made as
        // Home makes it (one that cannot be made now, an account's week without sign-in say, is left out); none removable.
        let mut built_in = Vec::new();
        for id in [crates::HOME_NEW, crates::HOME_WEEK, crates::HOME_GRAVES] {
            match self.crate_def(id).await {
                Ok(def) => built_in.push(def),
                Err(e) => tracing::debug!("crates: the built-in {id} is left out now: {e}"),
            }
        }
        let all: Vec<(bool, CrateDef)> =
            self.permanent_crates().await?.into_iter().chain(built_in).map(|c| (true, c)).chain(own.into_iter().map(|c| (false, c))).collect();
        let set = self.own_crates()?.all_favourites().await?;
        let albums_set = self.own_crates()?.all_favourite_albums().await?;
        let albums_of = |id: &str| if crates::is_fixed(id) { albums_set.get(id).copied().unwrap_or(crates::Favourites::All) } else { crates::Favourites::All };
        let left_set = self.own_crates()?.all_left_out().await?;
        let album_tracks = tracks_of_albums(self.playlists.served(), self.playlists.lib().catalogue(), left_set.values().flat_map(|l| l.albums.iter().cloned()).collect()).await;
        // Out of every crate too (decision 0024, amended 2026-10-19): the tracks excluded everywhere, and every track
        // crediting an artist excluded everywhere (decision 0083).
        let excluded = self.own_crates()?.excluded().await?;
        let artists = self.artists_out().await?;
        let none = crates::LeftOut::default();
        let left_of = |id: &str| left_set.get(id).unwrap_or(&none);
        // The favourites are asked for only when a crate shows by them (its conditions, or its toggle): asking the
        // media server for every favourite is slow, and a list of crates that need none must not wait for it. The
        // other settings' counts are then unknown (null) until it is asked.
        let needed = all.iter().any(|(_, c)| {
            crates::needs_favourites(&c.conditions) || set.get(&c.id).copied().unwrap_or(crates::Favourites::All) != crates::Favourites::All
        });
        let needed_albums = all.iter().any(|(_, c)| crates::needs_favourite_albums(&c.conditions) || albums_of(&c.id) != crates::Favourites::All);
        let mut lib = self.crate_library(want, needed, needed_albums, without).await?;
        // Fits your taste where a crate asks it (Second chance): its count unknown where it cannot be (signed out).
        if all.iter().any(|(_, c)| crates::needs_fits(&c.conditions)) {
            lib.fits = self.fitting(&lib, without).await.ok();
        }
        // Home's crates show what Home showed, of every library whatever is switched off (the owner, 2026-10-24), as their
        // panel does: counted so too (2026-11-02: New for you's card said 0 tracks with Incoming switched off).
        let every = if without.is_empty() { None } else { Some(self.crate_library(want, needed, needed_albums, &[]).await?) };
        let lib_of = |id: &str| if crates::home_crate(id) { every.as_ref().unwrap_or(&lib) } else { &lib };
        let count = |lib: &CrateLibrary, conditions: Vec<crates::Condition>| lib.knows(&conditions).then(|| lib.select(&conditions).len());
        // The cards' facts: the songs of every folder by their lines, and what the account asking has played.
        let songs_played = match self.playlists.served() {
            Some(serve) => {
                let (me, c) = (serve.current_user(), self.playlists.lib().catalogue());
                crate::run::spawn_blocking(move || {
                let songs = songs_by_line(&serve, &c);
                let played = me.and_then(|me| serve.userdata.played(&me).ok());
                (songs, played)
                })
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?
            }
            None => (HashMap::new(), None),
        };
        let (songs, played) = songs_played;
        let facts = crates::by_line(lib.files.iter().chain(every.iter().flat_map(|e| e.files.iter())));
        let now = super::moment::now_secs();
        let images = self.crate_images();
        Ok(all
            .into_iter()
            .map(|(permanent, mut c)| {
                // An own crate's toggles are its conditions (one truth): said as its heart and disc, counted per setting.
                let (favourites, favourite_albums) = if crates::is_fixed(&c.id) {
                    (set.get(&c.id).copied().unwrap_or(crates::Favourites::All), albums_of(&c.id))
                } else {
                    let (rest, tracks, albums) = crates::toggles_apart(&c.conditions);
                    c.conditions = rest;
                    (tracks, albums)
                };
                let left = left_of(&c.id);
                let gone = left_tracks(left, &album_tracks, &excluded);
                let shown = crates::leaving_out_artists(&artists, &crates::leaving_out(&gone, &crates::shown(&c.conditions, favourites, favourite_albums)));
                let lib = lib_of(&c.id);
                let stats = lib.knows(&shown).then(|| crate_stats(&lib.select(&shown), &facts, &songs, played.as_ref(), now));
                // Per setting of the favourites toggle, the albums toggle as it is set and the left-out list.
                let counted = |tracks: crates::Favourites| {
                    count(lib, crates::leaving_out_artists(&artists, &crates::leaving_out(&gone, &crates::shown(&c.conditions, tracks, favourite_albums))))
                };
                CrateView {
                    counts: CrateCounts {
                        all: counted(crates::Favourites::All),
                        only: counted(crates::Favourites::Only),
                        none: counted(crates::Favourites::None),
                    },
                    favourites,
                    favourite_albums,
                    left_out: left.len(),
                    stats,
                    image: images.as_ref().is_some_and(|d| d.join(crates::image_file(&c.id)).is_file()),
                    id: c.id,
                    name: c.name,
                    permanent,
                }
            })
            .collect())
    }

    /// Where crates' own pictures are kept (in the data folder); `None` without one.
    fn crate_images(&self) -> Option<std::path::PathBuf> {
        self.playlists.lib().data_dir().map(|d| std::path::PathBuf::from(d).join("crate-images"))
    }

    /// Sets a crate's own picture: any picture Cratarr can read, kept as JPEG at most 800 px on its longer side.
    pub async fn set_crate_image(&self, id: &str, data: Vec<u8>) -> Result<(), ToggleError> {
        self.crate_def(id).await?;
        self.may_picture(id)?;
        let Some(dir) = self.crate_images() else { return Err(ToggleError::SettingsReadOnly) };
        let file = dir.join(crates::image_file(id));
        crate::run::spawn_blocking(move || -> Result<()> {
            let jpeg = crate::serve::art::jpeg_within(&data, 800)?;
            std::fs::create_dir_all(&dir)?;
            let tmp = file.with_extension("jpg.tmp");
            std::fs::write(&tmp, jpeg)?;
            std::fs::rename(&tmp, &file)?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))
    }

    /// A crate's own picture and when it was set; `None` when it has none.
    pub async fn crate_image(&self, id: &str) -> Option<(Vec<u8>, std::time::SystemTime)> {
        self.crate_def(id).await.ok()?;
        let file = self.crate_images()?.join(crates::image_file(id));
        crate::run::spawn_blocking(move || {
            let modified = std::fs::metadata(&file).and_then(|m| m.modified()).ok()?;
            Some((std::fs::read(&file).ok()?, modified))
        })
        .await
        .ok()
        .flatten()
    }

    /// The crate shows its albums' covers again.
    pub async fn remove_crate_image(&self, id: &str) -> Result<(), ToggleError> {
        self.crate_def(id).await?;
        self.may_picture(id)?;
        if let Some(file) = self.crate_images().map(|d| d.join(crates::image_file(id))) {
            match tokio::fs::remove_file(&file).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(anyhow::Error::from(e).into()),
            }
        }
        Ok(())
    }

    /// Every crate the account sees, by id and name: the built-in ones and its own (search, decision 0072).
    pub async fn crate_names(&self) -> Result<Vec<(String, String)>, ToggleError> {
        let mut out: Vec<(String, String)> = crates::permanent().into_iter().map(|c| (c.id, c.name)).collect();
        for (id, name) in [(crates::HOME_NEW, "New for you"), (crates::HOME_WEEK, "Sounds like your week"), (crates::HOME_GRAVES, "Grave digging")] {
            out.push((id.to_string(), name.to_string()));
        }
        out.extend(self.own_crates()?.list().await?.into_iter().map(|c| (c.id, c.name)));
        Ok(out)
    }

    /// Every crate the account sees with the lines it shows now, as the Crates page shows them from every library
    /// (built-in and its own; one that cannot be made now left out): search finds a crate by its tracks (decision 0072).
    pub async fn crate_members(&self, without: &[String]) -> Result<Vec<(String, String, Vec<String>)>, ToggleError> {
        let mut all = self.permanent_crates().await?;
        for id in [crates::HOME_NEW, crates::HOME_WEEK, crates::HOME_GRAVES] {
            match self.crate_def(id).await {
                Ok(def) => all.push(def),
                Err(e) => tracing::debug!("search: the built-in {id} is left out now: {e}"),
            }
        }
        all.extend(self.own_crates()?.list().await?);
        let set = self.own_crates()?.all_favourites().await?;
        let albums_set = self.own_crates()?.all_favourite_albums().await?;
        let left_set = self.own_crates()?.all_left_out().await?;
        let album_tracks = tracks_of_albums(self.playlists.served(), self.playlists.lib().catalogue(), left_set.values().flat_map(|l| l.albums.iter().cloned()).collect()).await;
        let excluded = self.own_crates()?.excluded().await?;
        let artists = self.artists_out().await?;
        let none = crates::LeftOut::default();
        // An own crate's toggles are its conditions (one truth): only a built-in's are kept apart.
        let favourites_of = |id: &str| if crates::is_fixed(id) { set.get(id).copied().unwrap_or(crates::Favourites::All) } else { crates::Favourites::All };
        let albums_of = |id: &str| if crates::is_fixed(id) { albums_set.get(id).copied().unwrap_or(crates::Favourites::All) } else { crates::Favourites::All };
        let needed = all.iter().any(|c| crates::needs_favourites(&c.conditions) || favourites_of(&c.id) != crates::Favourites::All);
        let needed_albums = all.iter().any(|c| crates::needs_favourite_albums(&c.conditions) || albums_of(&c.id) != crates::Favourites::All);
        // Of the libraries switched on, as the crate opens (the owner, 2026-11-02); Home's of every library, as theirs.
        let lib = self.crate_library(None, needed, needed_albums, without).await?;
        let every = if without.is_empty() { None } else { Some(self.crate_library(None, needed, needed_albums, &[]).await?) };
        Ok(all
            .into_iter()
            .filter_map(|c| {
                let lib = if crates::home_crate(&c.id) { every.as_ref().unwrap_or(&lib) } else { &lib };
                let gone = left_tracks(left_set.get(&c.id).unwrap_or(&none), &album_tracks, &excluded);
                let shown = crates::leaving_out_artists(&artists, &crates::leaving_out(&gone, &crates::shown(&c.conditions, favourites_of(&c.id), albums_of(&c.id))));
                lib.knows(&shown).then(|| (c.id, c.name, lib.select(&shown)))
            })
            .collect())
    }

    /// A built-in crate is everyone's: its picture is the administrators' to change (decision 0051).
    fn may_picture(&self, id: &str) -> Result<(), ToggleError> {
        let built_in = crates::permanent().iter().any(|c| c.id == id) || crates::genre_crate(id).is_some() || id.starts_with(crates::LIKE) || crates::home_crate(id);
        if built_in && !crate::serve::access::administering() {
            return Err(ToggleError::Forbidden("A built-in crate's picture is for administrators.".into()));
        }
        Ok(())
    }

    /// A crate's definition: a permanent one, or one's own; a fixed one with the conditions the account added to it
    /// (decision 0083: Not in this crate).
    pub async fn crate_def(&self, id: &str) -> Result<CrateDef, ToggleError> {
        let now = self.like_now(id).await;
        let id = now.as_deref().unwrap_or(id);
        let mut def = self.made_def(id).await?;
        if crates::is_fixed(id) {
            def.conditions.extend(self.own_crates()?.added(&crates::kept_id(id)).await?);
        }
        Ok(def)
    }

    /// A Sounds like as a link from before 0.16 names it: its seed went, so the crate is the one of what the seed became, its count
    /// kept. None for any other.
    async fn like_now(&self, id: &str) -> Option<String> {
        let (seed, _) = crates::like_of(id)?;
        let (catalogue, old) = (self.playlists.lib().catalogue(), seed.to_string());
        let now = crate::run::spawn_blocking(move || catalogue.named_now(&old)).await.ok()??;
        Some(id.replacen(seed, &now, 1))
    }

    /// The names of the library ids in conditions (0.13, decision 0088: `crates::named_ids`, a Sounds like's seeds, the
    /// Album axis' values), by id: an artist's name, an album's or a track's title, as the library has them, in one
    /// lookup. An id the library does not have is left out; none without Cratarr's own library.
    pub async fn condition_names(&self, conditions: &[crates::Condition]) -> Result<std::collections::BTreeMap<String, crates::Named>, ToggleError> {
        let ids = crates::named_ids(conditions);
        let Some(serve) = self.playlists.served().filter(|_| !ids.is_empty()) else { return Ok(Default::default()) };
        let found = crate::run::spawn_blocking(move || serve.library.names_of(&ids)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        Ok(found
            .into_iter()
            .filter_map(|(id, name)| {
                let kind = crates::NameKind::of(&id)?;
                Some((id, crates::Named { name, kind }))
            })
            .collect())
    }

    /// The permanent crates, each with the conditions the account added to it (decision 0083).
    async fn permanent_crates(&self) -> Result<Vec<CrateDef>, ToggleError> {
        let store = self.own_crates()?;
        let mut all = crates::permanent();
        for c in &mut all {
            c.conditions.extend(store.added(&crates::kept_id(&c.id)).await?);
        }
        Ok(all)
    }

    /// A crate's definition as made: a permanent one, a genre's, Home's, a Sounds like, or one's own.
    async fn made_def(&self, id: &str) -> Result<CrateDef, ToggleError> {
        if let Some(c) = crates::permanent().into_iter().find(|c| c.id == id).or_else(|| crates::genre_crate(id)) {
            return Ok(c);
        }
        if crates::home_crate(id) {
            return self.home_crate(id).await;
        }
        if let Some((item, count)) = crates::like_of(id) {
            let mut def = match item.strip_prefix(crates::LIST) {
                Some(list) => self.playlist_like(id, list, count).await?,
                None => like_crate(self.playlists.served(), id, item, count).await?,
            };
            let (fit, avoid, _) = self.fit_of(item).await?;
            for c in &mut def.conditions {
                if let crates::Value::Like { avoid: a, .. } = &mut c.value {
                    *a = avoid.clone();
                }
            }
            def.conditions.extend(fit.rules);
            return Ok(def);
        }
        self.own_crates()?.list().await?.into_iter().find(|c| c.id == id).ok_or(ToggleError::UnknownPlaylist)
    }

    /// Home's shelf as a crate (`crates::HOME_NEW`): New for you, the shelf's albums (`new_for_you`); Grave digging,
    /// `digging`'s albums. Their tracks album by album, in the shelf's order.
    async fn home_crate(&self, id: &str) -> Result<CrateDef, ToggleError> {
        let serve = self.playlists.served().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let (name, albums) = if id == crates::HOME_NEW {
            ("New for you", self.new_for_you().await?)
        } else {
            ("Grave digging", self.digging(None, &[]).await?.graves.into_iter().map(|g| g.id).collect())
        };
        let lines = crate::run::spawn_blocking(move || -> Result<Vec<String>> {
            let cat = serve.catalogue();
            let mut lines = Vec::new();
            for id in &albums {
                let Some(album) = serve.library.album(id)? else { continue };
                lines.extend(album.song.unwrap_or_default().iter().filter_map(|s| crate::serve::playlists::line_of_song(cat, &s.id)));
            }
            Ok(lines)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        Ok(crates::lines_crate(id, name, lines))
    }

    /// A playlist's suggestions (`crates::playlist_crate`): its tracks the seeds, named after it.
    async fn playlist_like(&self, id: &str, list: &str, count: usize) -> Result<CrateDef, ToggleError> {
        let serve = self.playlists.served().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let lines = self.playlists.entries(list).await?;
        let songs = crate::run::spawn_blocking(move || {
            let cat = serve.catalogue();
            lines.iter().filter_map(|l| crate::serve::playlists::song_of_line(cat, l)).collect::<Vec<_>>()
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        let name = list.rsplit('/').next().unwrap_or(list);
        Ok(crates::playlist_crate(id, name, songs, count))
    }

    /// A ranked crate's fit (the owner, 2026-10-23: playlist fit models, for every ranked crate), by its item (`like:`'s,
    /// without a count): its rules and dismissals as kept, what it should not be like (the traits learned from what was
    /// turned down for it, its left-out tracks and albums', those dismissed left out), and every trait learned with how
    /// many were turned down.
    async fn fit_of(&self, item: &str) -> Result<(crates::Fit, crates::Avoid, (Vec<crates::Trait>, usize)), ToggleError> {
        let base = format!("{}{item}", crates::LIKE);
        let store = self.own_crates()?;
        let fit = store.fit(&base).await?;
        let left = store.left_out(&base).await?;
        if left.is_empty() {
            return Ok((fit, crates::Avoid::default(), (Vec::new(), 0)));
        }
        let album_tracks = tracks_of_albums(self.playlists.served(), self.playlists.lib().catalogue(), left.albums.iter().cloned().collect()).await;
        let down: HashSet<String> = left.tracks.into_iter().chain(album_tracks.into_values().flatten()).collect();
        let lib = self.crate_library(None, false, false, &[]).await?;
        let traits = crate::run::spawn_blocking(move || {
            let all: Vec<&crates::Facts> = lib.files.iter().collect();
            let turned: Vec<&crates::Facts> = all.iter().copied().filter(|f| f.song().is_some_and(|s| down.contains(&s))).collect();
            (crates::learned(&turned, &all), turned.len())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        let kept: Vec<crates::Trait> = traits.0.iter().filter(|t| !fit.dismissed.contains(&t.key())).cloned().collect();
        let avoid = crates::Avoid::of(kept, traits.1);
        Ok((fit, avoid, traits))
    }

    /// A ranked crate's fit as its Not for sheet shows it: every trait learned (whether dismissed), its rules, how many
    /// were turned down.
    pub async fn crate_fit(&self, id: &str) -> Result<FitView, ToggleError> {
        let (item, _) = crates::like_of(id).ok_or_else(|| ToggleError::Refused("Only a Sounds like crate learns what does not fit.".into()))?;
        let (fit, _, (learned, turned_down)) = self.fit_of(item).await?;
        let learned = learned.into_iter().map(|t| FitTrait { dismissed: fit.dismissed.contains(&t.key()), key: t.key(), r#trait: t }).collect();
        Ok(FitView { learned, rules: fit.rules, turned_down })
    }

    /// Every ranked crate whose fit holds anything (what was turned down for it, rules, dismissals), the asking
    /// account's, for Settings' Musical taste (76): each with its name, what it avoids (the traits
    /// learned, those dismissed left out), its rules and how many were turned down. A crate that no longer resolves (a
    /// playlist gone) is left out.
    pub async fn crate_fits(&self) -> Result<Vec<FitSummary>, ToggleError> {
        let store = self.own_crates()?;
        let mut ids: std::collections::BTreeSet<String> = store.all_fits().await?.into_keys().collect();
        ids.extend(store.all_left_out().await?.into_iter().filter(|(id, l)| id.starts_with(crates::LIKE) && !l.is_empty()).map(|(id, _)| id));
        let mut out = Vec::new();
        for id in ids {
            let Some((item, count)) = crates::like_of(&id) else { continue };
            let name = match item.strip_prefix(crates::LIST) {
                Some(list) if self.playlists.entries(list).await.is_ok() => format!("Sounds like {}", list.rsplit('/').next().unwrap_or(list)),
                Some(_) => continue,
                None => match like_crate(self.playlists.served(), &id, item, count).await {
                    Ok(def) => def.name,
                    Err(_) => continue,
                },
            };
            let (fit, avoid, (_, turned_down)) = self.fit_of(item).await?;
            let of = name.strip_prefix("Sounds like ").unwrap_or(&name).to_string();
            out.push(FitSummary { crate_id: id.clone(), name, of, avoiding: avoid.traits, rules: fit.rules, turned_down });
        }
        Ok(out)
    }

    /// Its rules and dismissals, as the sheet leaves them.
    pub async fn set_crate_fit(&self, id: &str, fit: crates::Fit) -> Result<(), ToggleError> {
        let (item, _) = crates::like_of(id).ok_or_else(|| ToggleError::Refused("Only a Sounds like crate learns what does not fit.".into()))?;
        if let Some(c) = fit.rules.iter().find(|c| !crates::is_axis(&c.axis)) {
            return Err(ToggleError::Refused(format!("Unknown axis: {}.", c.axis)));
        }
        if let Some(why) = crates::not_taken(&fit.rules) {
            return Err(ToggleError::Unanswerable(why));
        }
        Ok(self.own_crates()?.set_fit(&format!("{}{item}", crates::LIKE), fit).await?)
    }

    /// The lines these conditions select from the libraries not in `without`, newest first, and the server they are for.
    pub(super) async fn crate_lines_of(&self, conditions: &[crates::Condition], want: Option<&str>, without: &[String]) -> Result<(String, Vec<String>), ToggleError> {
        let (lib, lines) = self.crate_selection(conditions, want, without).await?;
        Ok((lib.source, lines))
    }

    /// The profile of what these conditions select from the libraries not in `without` (0.17, the owner: a crate being made or
    /// changed shows its profile only when asked, folded until its pill opens it), as a saved crate's is counted (`filter`).
    pub async fn crate_profile(&self, conditions: &[crates::Condition], want: Option<&str>, without: &[String]) -> Result<crates::Profile, ToggleError> {
        let (lib, lines) = self.crate_selection(conditions, want, without).await?;
        let chosen: HashSet<&str> = lines.iter().map(String::as_str).collect();
        let mine: Vec<&crates::Facts> = lib.files().filter(|f| chosen.contains(f.rel.as_str())).collect();
        let library: Vec<&crates::Facts> = lib.files().collect();
        Ok(crates::profile(&mine, &library))
    }

    /// The library a crate is worked out from, and its lines.
    async fn crate_selection(&self, conditions: &[crates::Condition], want: Option<&str>, without: &[String]) -> Result<(CrateLibrary, Vec<String>), ToggleError> {
        let lib = self.crate_library_for(conditions, want, without).await?;
        let lines = lib.select(conditions);
        Ok((lib, lines))
    }

    /// The library these conditions are worked out on, with what they need of the favourites (refused when those
    /// cannot be read): a crate's, a filter's (decision 0061).
    pub async fn crate_library_for(&self, conditions: &[crates::Condition], want: Option<&str>, without: &[String]) -> Result<CrateLibrary, ToggleError> {
        // Refused before any work (0.13, decision 0088): all of on an axis a track has one value on.
        if let Some(why) = crates::not_taken(conditions) {
            return Err(ToggleError::Unanswerable(why));
        }
        let mut lib = self.crate_library(want, crates::needs_favourites(conditions), crates::needs_favourite_albums(conditions), without).await?;
        if crates::needs_fits(conditions) {
            lib.fits = Some(self.fitting(&lib, without).await?);
        }
        if crates::needs_favourites(conditions) && lib.favourites.is_none() {
            return Err(ToggleError::CannotPlay("The server's favourites could not be read.".into()));
        }
        if crates::needs_favourite_albums(conditions) && lib.favourite_albums.is_none() {
            return Err(ToggleError::CannotPlay("The favourite albums could not be read.".into()));
        }
        Ok(lib)
    }

    /// A page of a crate's rows (it can hold thousands): from `offset`, at most `limit`, with the crate's size, as it is
    /// looked at (its toggles, the libraries switched off).
    pub async fn crate_rows(&self, id: &str, want: Option<&str>, offset: usize, limit: usize, look: &CrateLook) -> Result<CratePage, ToggleError> {
        let (shown, causes) = self.crate_causes(id, look).await?;
        let mut page = self.crate_page_saying(&shown, &causes, want, offset, limit, look).await?;
        // A word's crate names the word and its sources (decision 0097: its menu's header).
        if let Some((kind, name)) = id.split_once(':')
            && super::words::store::Kind::of(kind).is_some()
            && let Some(served) = self.playlists.served()
        {
            let (kind, name) = (kind.to_string(), name.trim().to_string());
            page.word = crate::run::spawn_blocking(move || super::words::stored(&served).ok().and_then(|s| s.word(&kind, &name))).await.ok().flatten();
        }
        Ok(page)
    }

    /// A crate's conditions as its toggles show it (as given, else as saved), with its left-out tracks and the filter on
    /// it.
    pub async fn crate_shown(&self, id: &str, look: &CrateLook) -> Result<Vec<crates::Condition>, ToggleError> {
        Ok(self.crate_causes(id, look).await?.0)
    }

    /// `crate_shown`'s conditions, and which of them each cause added (decision 0095): its own and its left-out list
    /// first (what the crate is), then its toggles, what is excluded everywhere and each condition of the filter.
    async fn crate_causes(&self, id: &str, look: &CrateLook) -> Result<(Vec<crates::Condition>, Vec<crates::Cause>), ToggleError> {
        let def = self.crate_def(id).await?;
        // An own crate's toggles are its conditions (one truth): a built-in's as the page shows them, else as kept.
        let (fav, albums) = match crates::is_fixed(id) {
            true => (
                match look.favourites {
                    Some(f) => f,
                    None => self.crate_favourites(id).await?,
                },
                match look.favourite_albums {
                    Some(a) => a,
                    None => self.own_crates()?.favourite_albums(id).await?,
                },
            ),
            false => (crates::Favourites::All, crates::Favourites::All),
        };
        let left = self.own_crates()?.left_out(id).await?;
        let album_tracks = tracks_of_albums(self.playlists.served(), self.playlists.lib().catalogue(), left.albums.iter().cloned().collect()).await;
        let excluded = self.own_crates()?.excluded().await?;
        let artists = self.artists_out().await?;
        Ok(crates::shown_saying(&def.conditions, &left_tracks(&left, &album_tracks, &[]), fav, albums, &excluded, &artists, &look.filter))
    }

    /// What a device holds of `user`'s beside its marks (`index::own`), read as a request of theirs reads it: every crate it has, as it
    /// would be made now, and what those and a pick read.
    pub async fn device_own(&self, user: &str) -> Result<cratarr_rules::index::own::Own, ToggleError> {
        use cratarr_rules::index::own::{Kept, List, Own};
        let serve = self.playlists.served().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let cat = self.playlists.lib().catalogue();
        let c = cat.clone();
        let files = crate::run::spawn_blocking(move || c.crate_facts()).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        let track_of_line: HashMap<&str, i64> = files.iter().filter_map(|f| Some((f.track?, f))).flat_map(|(t, f)| f.every_line().map(move |l| (l, t))).collect();
        let tracks_of = |lines: &[String]| -> Vec<i64> { lines.iter().filter_map(|l| track_of_line.get(l.as_str()).copied()).collect() };
        let numbers = |songs: &[String]| -> Vec<i64> { songs.iter().filter_map(|s| crate::serve::ids::track_of(s)).collect() };

        let store = self.own_crates()?;
        let (saved_tracks, saved_albums, left_out) = (store.all_favourites().await?, store.all_favourite_albums().await?, store.all_left_out().await?);
        let album_tracks = tracks_of_albums(self.playlists.served(), cat.clone(), left_out.values().flat_map(|l| l.albums.iter().cloned()).collect()).await;
        // What the Crates page lists, then every fixed crate the account set something on: those are made on a device with it kept.
        let own: Vec<String> = store.list().await?.into_iter().map(|c| c.id).collect();
        let listed: Vec<(String, bool)> = crates::permanent()
            .into_iter()
            .map(|c| c.id)
            .chain([crates::HOME_NEW, crates::HOME_WEEK, crates::HOME_GRAVES].map(str::to_string))
            .map(|id| (id, true))
            .chain(own.iter().map(|id| (id.clone(), false)))
            .collect();
        let set: std::collections::BTreeSet<String> =
            saved_tracks.keys().chain(saved_albums.keys()).chain(left_out.keys()).cloned().chain(store.all_fits().await?.into_keys()).chain(store.all_added().await?.into_keys()).collect();
        let mut kept = Vec::new();
        let images = self.crate_images();
        for (id, permanent, listed) in listed.iter().map(|(id, p)| (id.clone(), *p, true)).chain(set.into_iter().filter(|id| !listed.iter().any(|(l, _)| l == id)).map(|id| (id, false, false))) {
            // One that cannot be made now is left out, as the page's list leaves it out.
            let Ok(def) = self.crate_def(&id).await else { continue };
            let shelf = crates::home_crate(&id).then(|| {
                let lines = def.conditions.iter().find_map(|c| match &c.value {
                    crates::Value::Within { ranked, .. } => Some(tracks_of(ranked)),
                    _ => None,
                });
                lines.unwrap_or_default()
            });
            let conditions = one_spelling(&def.conditions).into_iter().filter(|c| !matches!(c.value, crates::Value::Within { .. })).collect();
            let none = crates::LeftOut::default();
            let left = left_out.get(&id).unwrap_or(&none);
            kept.push(Kept {
                conditions,
                favourites: if crates::is_fixed(&id) { self.crate_favourites(&id).await? } else { crates::Favourites::All },
                favourite_albums: if crates::is_fixed(&id) { store.favourite_albums(&id).await? } else { crates::Favourites::All },
                left_out: numbers(&left_tracks(left, &album_tracks, &[])),
                left: left.len(),
                shelf,
                listed,
                permanent,
                image: images.as_ref().is_some_and(|d| d.join(crates::image_file(&id)).is_file()),
                name: def.name,
                id,
            });
        }
        let own_store = self.playlists.own_store()?;
        let mut lists = Vec::new();
        for (name, lines, _) in own_store.snapshot().await? {
            lists.push(List { id: own_store.key(&name), tracks: tracks_of(&lines), name });
        }
        let together = self.playlists.snapshot_all().await?.iter().map(|(_, lines, _)| tracks_of(lines)).collect();
        let (sid, source) = self.playlists.source_of(None)?;
        let hearted = self.playlists.favourite_lines(&sid, &source).await.unwrap_or_default();
        let favourites = files.iter().filter(|f| f.every_line().any(|l| hearted.contains(l))).filter_map(|f| f.track).collect();
        let excluded = numbers(&store.excluded().await?);
        let artists_out = self.artists_out_of(Some(user.to_string())).await?.into_iter().collect();
        let (s, who) = (serve.clone(), user.to_string());
        let (albums, recent, libraries) = crate::run::spawn_blocking(move || -> Result<_> {
            let albums = s.userdata.starred(&who, crate::userdata::Kind::Album)?;
            Ok((albums, super::crates_from_radio::recent_of(&s, &who)?, super::crates_from_radio::folders_of(&s, &who)))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        Ok(Own {
            favourites,
            favourite_albums: albums.iter().filter_map(|a| a.strip_prefix(crate::serve::ids::ALBUM)?.parse().ok()).collect(),
            excluded,
            artists_out,
            lists,
            together,
            similar_by: super::crates_from_radio::similar_by_of(user),
            libraries,
            crates: kept,
            recent: recent.into_iter().filter_map(|(song, at)| Some((crate::serve::ids::track_of(&song)?, at))).collect(),
        })
    }

    /// Not in this crate (decision 0083, the owner 2026-10-30): "Artist is not `name`" added to the crate, once. One's
    /// own crate gets it in its own conditions, as one would set it by hand (its undo the crate's); a fixed crate, which
    /// has no filter of one's own, among the conditions the account added to it. Returns the artists the crate leaves
    /// out so, and the undo token (none when it left them out already).
    pub async fn crate_not_artist(&self, id: &str, name: &str) -> Result<(Vec<NotArtist>, Option<String>), ToggleError> {
        if super::excluded_artists::key(name).is_empty() {
            return Err(ToggleError::Refused("Which artist?".into()));
        }
        let undo = if crates::is_fixed(id) {
            self.crate_def(id).await?;
            let (store, kept) = (self.own_crates()?, crates::kept_id(id));
            let before = store.added(&kept).await?;
            match crates::with_not_artist(&before, name) {
                Some(after) => {
                    store.set_added(&kept, after.clone()).await?;
                    self.playlists.keep(Change::Added { crate_id: kept, before, after })
                }
                None => None,
            }
        } else {
            let def = self.own_crate(id).await?.ok_or(ToggleError::UnknownPlaylist)?;
            match crates::with_not_artist(&def.conditions, name) {
                Some(after) => self.crate_save(Some(id), &def.name, after).await?.1,
                None => None,
            }
        };
        Ok((self.crate_not_artists(id).await?, undo))
    }

    /// An artist a crate leaves out taken back (by their key): its "Artist is not" rows no longer name them. Returns the
    /// artists it still leaves out so, and the undo token (none when it named them nowhere).
    pub async fn crate_not_artist_back(&self, id: &str, key: &str) -> Result<(Vec<NotArtist>, Option<String>), ToggleError> {
        let key = super::excluded_artists::key(key);
        let undo = if crates::is_fixed(id) {
            let (store, kept) = (self.own_crates()?, crates::kept_id(id));
            let before = store.added(&kept).await?;
            match crates::without_not_artist(&before, &key) {
                Some(after) => {
                    store.set_added(&kept, after.clone()).await?;
                    self.playlists.keep(Change::Added { crate_id: kept, before, after })
                }
                None => None,
            }
        } else {
            let def = self.own_crate(id).await?.ok_or(ToggleError::UnknownPlaylist)?;
            match crates::without_not_artist(&def.conditions, &key) {
                Some(after) => self.crate_save(Some(id), &def.name, after).await?.1,
                None => None,
            }
        };
        Ok((self.crate_not_artists(id).await?, undo))
    }

    /// The artists a crate leaves out by its "Artist is not" rows (decision 0083), as its view lists them: one's own
    /// crate's in its conditions, a fixed crate's among those the account added to it.
    pub async fn crate_not_artists(&self, id: &str) -> Result<Vec<NotArtist>, ToggleError> {
        let conditions = if crates::is_fixed(id) {
            self.own_crates()?.added(&crates::kept_id(id)).await?
        } else {
            self.own_crate(id).await?.ok_or(ToggleError::UnknownPlaylist)?.conditions
        };
        Ok(crates::not_artists(&conditions).into_iter().map(|(key, name)| NotArtist { key, name }).collect())
    }

    /// The keys of the artists the asking account excluded everywhere (decision 0083), read once per pick.
    async fn artists_out(&self) -> Result<HashSet<String>, ToggleError> {
        self.artists_out_of(asking_user(self.playlists.served().as_deref())).await
    }

    /// The keys of the artists `user` excluded everywhere; none without Cratarr's own library or an account.
    async fn artists_out_of(&self, user: Option<String>) -> Result<HashSet<String>, ToggleError> {
        let (Some(serve), Some(user)) = (self.playlists.served(), user) else { return Ok(HashSet::new()) };
        Ok(crate::run::spawn_blocking(move || serve.userdata.excluded_artists(&user)).await.map_err(|e| anyhow::anyhow!("{e}"))??)
    }

    /// The account the everywhere list is the asking one's of, and Cratarr's own library: refused without either.
    fn artist_lister(&self) -> Result<(Arc<crate::serve::Serve>, String), ToggleError> {
        let serve = self.playlists.served().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let user = asking_user(Some(&*serve)).ok_or_else(|| ToggleError::Refused("Sign in first: what is excluded is an account's own.".into()))?;
        Ok((serve, user))
    }

    /// Exclude everywhere (decision 0083, the owner 2026-10-30): the asking account's every automatic pick (every crate,
    /// Radio and Autoplay, the shuffles, Discover, Home's shelves) leaves out a track crediting the artist anywhere and
    /// an album whose album artist they are. Played by hand, they play. Returns them as listed, and the undo token (none
    /// when they were listed already).
    pub async fn exclude_artist(&self, name: &str) -> Result<(crate::userdata::ExcludedArtist, Option<String>), ToggleError> {
        let (serve, user) = self.artist_lister()?;
        let name = name.trim().to_string();
        let key = super::excluded_artists::key(&name);
        if key.is_empty() {
            return Err(ToggleError::Refused("Which artist?".into()));
        }
        let (u, k, n) = (user.clone(), key.clone(), name.clone());
        let (new, listed) = crate::run::spawn_blocking(move || -> anyhow::Result<_> {
            let new = serve.userdata.exclude_artist(&u, &k, &n)?;
            Ok((new, serve.userdata.excluded_artist_list(&u)?.into_iter().find(|a| a.key == k)))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        let listed = listed.unwrap_or(crate::userdata::ExcludedArtist { key: key.clone(), name: name.clone(), at: super::library::epoch_now() });
        let undo = new.then(|| self.playlists.keep(Change::ArtistOut { user, key, name, out: true })).flatten();
        Ok((listed, undo))
    }

    /// An artist taken back off the asking account's everywhere list (by their key): picked again. Their key, and the
    /// undo token (none when they were not on it).
    pub async fn include_artist(&self, key: &str) -> Result<(String, Option<String>), ToggleError> {
        let (serve, user) = self.artist_lister()?;
        let key = super::excluded_artists::key(key);
        let (u, k) = (user.clone(), key.clone());
        let gone = crate::run::spawn_blocking(move || -> anyhow::Result<_> {
            let was = serve.userdata.excluded_artist_list(&u)?.into_iter().find(|a| a.key == k);
            serve.userdata.include_artist(&u, &k)?;
            Ok(was)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        let undo = gone.and_then(|a| self.playlists.keep(Change::ArtistOut { user, key: key.clone(), name: a.name, out: false }));
        Ok((key, undo))
    }

    /// The asking account's everywhere list, the newest first (Settings › Listening).
    pub async fn excluded_artist_list(&self) -> Result<Vec<crate::userdata::ExcludedArtist>, ToggleError> {
        let (serve, user) = self.artist_lister()?;
        Ok(crate::run::spawn_blocking(move || serve.userdata.excluded_artist_list(&user)).await.map_err(|e| anyhow::anyhow!("{e}"))??)
    }

    /// Tracks taken out of a crate by hand, or put back (`out` false), by their ids on a server, and whole albums by
    /// their ids (decision 0024, amended 2026-10-19): kept in the crate's left-out list, the tracks by their song ids, an
    /// album as a rule. Returns how many it holds now (tracks and albums) and the undo token.
    pub async fn crate_leave_out(&self, id: &str, want: Option<&str>, ids: &[String], albums: &[String], out: bool) -> Result<(usize, Option<String>), ToggleError> {
        self.crate_def(id).await?;
        let tracks = self.playlists.tracks_of(want, ids, out).await?;
        // Only albums the library has go in; any comes back out.
        let albums: Vec<String> = if out { library_albums(self.playlists.served(), albums.to_vec()).await?.into_iter().map(|a| a.id).collect() } else { albums.to_vec() };
        let store = self.own_crates()?;
        let before = store.left_out(id).await?;
        let after = crates::LeftOut { tracks: edited(&before.tracks, tracks, out), albums: edited(&before.albums, albums, out) };
        if after == before {
            return Ok((after.len(), None));
        }
        store.set_left_out(id, after.clone()).await?;
        let undo = self.playlists.keep(Change::LeftOut { crate_id: id.to_string(), before, after: after.clone() });
        Ok((after.len(), undo))
    }

    /// A crate's tracks left out by hand, as rows on a server, and its albums left out, for its editor to put back.
    pub async fn crate_left_out(&self, id: &str, want: Option<&str>) -> Result<(String, Vec<PlaylistRow>, Vec<LeftOutAlbum>), ToggleError> {
        self.crate_def(id).await?;
        let (sid, _) = self.playlists.source_of(want)?;
        let left = self.own_crates()?.left_out(id).await?;
        let found = library_albums(self.playlists.served(), left.albums.clone()).await?;
        let albums = left
            .albums
            .iter()
            .map(|a| {
                let f = found.iter().find(|x| &x.id == a);
                LeftOutAlbum { id: a.clone(), title: f.map(|x| x.name.clone()), artist: f.map(|x| x.artist.clone()) }
            })
            .collect();
        let rows = self.playlists.track_rows(left.tracks, &sid).await?;
        Ok((sid, rows, albums))
    }

    /// Tracks excluded from every automatic pick, or put back (`out` false), by their ids on a server (decision 0024,
    /// amended 2026-10-19: global exclusions), the asking account's: out of every crate as a crate's left-out tracks
    /// are, and never picked by Radio, Autoplay, or a shuffle of the library or the favourites; played by hand, they
    /// play. Returns how many there are now and the undo token.
    pub async fn exclude(&self, want: Option<&str>, ids: &[String], out: bool) -> Result<(usize, Option<String>), ToggleError> {
        let tracks = self.playlists.tracks_of(want, ids, out).await?;
        let store = self.own_crates()?;
        let before = store.excluded().await?;
        let after = edited(&before, tracks, out);
        if after == before {
            return Ok((after.len(), None));
        }
        store.set_excluded(after.clone()).await?;
        self.playlists.excluded_tracks_changed();
        let undo = self.playlists.keep(Change::Excluded { before, after: after.clone() });
        Ok((after.len(), undo))
    }

    /// The tracks excluded everywhere, how many and as rows on a server, for the Library's list to put back.
    pub async fn excluded_rows(&self, want: Option<&str>) -> Result<(String, usize, Vec<PlaylistRow>), ToggleError> {
        let (sid, _) = self.playlists.source_of(want)?;
        let tracks = self.own_crates()?.excluded().await?;
        let n = tracks.len();
        Ok((sid.clone(), n, self.playlists.track_rows(tracks, &sid).await?))
    }

    /// Cratarr's own tracks by their song ids, as rows (Home's Favourite tracks), in the order given; songs whose file is
    /// gone are left out.
    pub async fn song_rows(&self, ids: Vec<String>) -> Result<Vec<PlaylistRow>, ToggleError> {
        let cat = self.playlists.lib().catalogue();
        let lines = crate::run::spawn_blocking(move || ids.iter().filter_map(|id| crate::serve::playlists::line_of_song(&cat, id)).collect::<Vec<_>>())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.playlists.rows_of(lines, crate::source::cratarr::KIND.id).await
    }

    /// The Cratarr song ids of the tracks `user` excluded everywhere (`None`: the first administrator), for the picks
    /// that leave them out: Radio's and Autoplay's, the shuffles of the library and the favourites, Subsonic's random songs.
    /// With them every song crediting an artist `user` excluded everywhere (decision 0083), by the crates' facts.
    pub async fn excluded_songs_of(&self, user: Option<&str>) -> Result<HashSet<String>, ToggleError> {
        let account = user.and_then(|u| account_of(self.playlists.served().as_deref(), u));
        let mut songs: HashSet<String> = self.playlists.crate_stores().of(account.as_deref())?.excluded().await?.into_iter().collect();
        let whose = user.map(str::to_string).or_else(|| self.playlists.served().as_ref().and_then(|s| s.users.owner()));
        let artists = self.artists_out_of(whose).await?;
        if artists.is_empty() {
            return Ok(songs);
        }
        let cat = self.playlists.lib().catalogue();
        Ok(crate::run::spawn_blocking(move || -> Result<HashSet<String>> {
            songs.extend(songs_crediting(&cat.crate_facts()?, &artists));
            Ok(songs)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??)
    }

    /// A crate's tempo spread (decision 0041), its tracks chosen as its rows are (as it is looked at): counted here, so
    /// a genre of thousands of tracks does not send them all to be counted.
    pub async fn crate_tempo(&self, id: &str, want: Option<&str>, look: &CrateLook) -> Result<crates::TempoBands, ToggleError> {
        let shown = self.crate_shown(id, look).await?;
        let (lib, lines) = self.crate_selection(&shown, want, &look.without).await?;
        let chosen: HashSet<&str> = lines.iter().map(String::as_str).collect();
        Ok(crates::tempo_bands(lib.files().filter(|f| chosen.contains(f.rel.as_str())).map(|f| f.bpm)))
    }

    /// A crate's favourites toggle as set; all of them where it was not, as the Crates page shows it (`CrateStore::favourites`).
    async fn crate_favourites(&self, id: &str) -> Result<crates::Favourites, ToggleError> {
        Ok(self.own_crates()?.favourites(id).await?)
    }

    /// Sets a crate's favourites toggle (decision 0024, amended): an own crate's is its Favourite condition (the owner,
    /// 2026-11-02: one truth), saved as the crate is, with its Undo; a built-in's one setting per crate, for every device,
    /// not journaled (a tap undoes it by tapping again).
    pub async fn set_crate_favourites(&self, id: &str, fav: crates::Favourites) -> Result<Option<String>, ToggleError> {
        self.set_toggle(id, "favourite", fav).await
    }

    /// Sets a crate's favourite albums toggle (decision 0024, amended 2026-10-19), as the favourites toggle is set.
    pub async fn set_crate_favourite_albums(&self, id: &str, albums: crates::Favourites) -> Result<Option<String>, ToggleError> {
        self.set_toggle(id, "favourite_album", albums).await
    }

    async fn set_toggle(&self, id: &str, axis: &str, to: crates::Favourites) -> Result<Option<String>, ToggleError> {
        let def = self.crate_def(id).await?;
        if !crates::is_fixed(id) {
            return Ok(self.crate_save(Some(id), &def.name, crates::with_toggle(&def.conditions, axis, to)).await?.1);
        }
        match axis {
            "favourite_album" => self.own_crates()?.set_favourite_albums(id, to).await?,
            _ => self.own_crates()?.set_favourites(id, to).await?,
        }
        Ok(None)
    }

    /// Own crates' toggles kept apart before 0.14 written into their conditions, once (the owner, 2026-11-02: one
    /// truth): each as it showed (a crate never set showed no favourites), the setting kept apart then all.
    pub async fn favourites_into_conditions(&self) -> Result<(), ToggleError> {
        const DONE: &str = "crates:favourites-in-conditions";
        let Some(served) = self.playlists.served() else { return Ok(()) };
        if served.userdata.import_choice(DONE).ok().flatten().is_some_and(|c| c == "1") {
            return Ok(());
        }
        let store = self.own_crates()?;
        for c in store.list().await? {
            let (tracks, albums) = (store.favourites(&c.id).await?, store.favourite_albums(&c.id).await?);
            if (tracks, albums) != (crates::Favourites::All, crates::Favourites::All) {
                let conditions = crates::with_toggle(&crates::with_toggle(&c.conditions, "favourite", tracks), "favourite_album", albums);
                store.save(Some(&c.id), &c.name, conditions).await.map_err(ToggleError::Refused)?;
            }
            store.set_favourites(&c.id, crates::Favourites::All).await?;
            store.set_favourite_albums(&c.id, crates::Favourites::All).await?;
        }
        served.userdata.set_import_choice(DONE, "1")?;
        tracing::info!(target: "library", "own crates' favourites toggles are their conditions now");
        Ok(())
    }

    /// A page of the rows these conditions select (a crate's, as `crate_shown` gives them; the builder's live preview,
    /// decision 0024, issue 6), from the libraries not in `look.without`, in `look`'s order (its toggles and filter are
    /// in the conditions already).
    pub async fn crate_page(&self, conditions: &[crates::Condition], want: Option<&str>, offset: usize, limit: usize, look: &CrateLook) -> Result<CratePage, ToggleError> {
        self.crate_page_saying(conditions, &[], want, offset, limit, look).await
    }

    /// `crate_page`, saying which of `causes` (and the libraries switched off) left what out (decision 0095).
    async fn crate_page_saying(&self, conditions: &[crates::Condition], causes: &[crates::Cause], want: Option<&str>, offset: usize, limit: usize, look: &CrateLook) -> Result<CratePage, ToggleError> {
        let lib = self.crate_library_for(conditions, want, &look.without).await?;
        let (lines, reasons, all_back, judged) = lib.select_saying(conditions, causes, &look.without, look.explain);
        let blocked = match judged {
            Some(judged) => {
                // The page's words are judged on each track's row: read for every track a cause keeps out, and those shown.
                let searched = look.search.iter().any(|q| !q.trim().is_empty());
                let rows: HashMap<String, PlaylistRow> = match searched {
                    true => {
                        let all: Vec<String> = judged.verdicts.kept_keys().chain(lines.iter().map(String::as_str)).map(str::to_string).collect();
                        self.playlists.rows_of(all, &lib.source).await?.into_iter().map(|r| (r.entry.clone(), r)).collect()
                    }
                    false => HashMap::new(),
                };
                Some(what_blocks(judged, &lines, &look.search, &rows))
            }
            None => None,
        };
        // On the first page only: the panel shows them on top.
        let genres = if offset == 0 {
            let chosen: HashSet<&str> = lines.iter().map(String::as_str).collect();
            crates::top_genres(lib.files().filter(|f| chosen.contains(f.rel.as_str())).map(|f| f.genres.as_slice()), super::crates_from_radio::GENRES_SHOWN)
        } else {
            Vec::new()
        };
        let total = lines.len();
        let lines = self.crate_in_order(&lib, lines, conditions, look).await?;
        let page: Vec<String> = lines.iter().skip(offset).take(limit).cloned().collect();
        let mut rows = self.playlists.rows_of(page, &lib.source).await?;
        self.with_plays_and_added(&mut rows, &lib).await;
        Ok(CratePage { source: lib.source, total, limit, rows, genres, reasons, all_back, blocked, word: None })
    }

    /// The lines these conditions select, in `look`'s order, and the server they are for: what Play plays as the list
    /// shows it.
    pub async fn crate_lines_in_order(&self, conditions: &[crates::Condition], want: Option<&str>, look: &CrateLook) -> Result<(String, Vec<String>), ToggleError> {
        let (lib, lines) = self.crate_selection(conditions, want, &look.without).await?;
        let lines = self.crate_in_order(&lib, lines, conditions, look).await?;
        Ok((lib.source.clone(), lines.to_vec()))
    }

    /// A crate's lines (in its own order) in `look`'s order. By a field, every row of it is read and sorted (by Best
    /// match, every line's fit), and the answer kept a little while for the next pages (`SORTED_KEEP`).
    async fn crate_in_order(&self, lib: &CrateLibrary, mut lines: Vec<String>, conditions: &[crates::Condition], look: &CrateLook) -> Result<Arc<Vec<String>>, ToggleError> {
        if look.order == CrateOrder::Own {
            if look.down {
                lines.reverse();
            }
            return Ok(Arc::new(lines));
        }
        // What the order was worked out from: the conditions, the server, the libraries, the order, who asks (their
        // plays), and the library's facts as read (a new reading after any change).
        let key = format!("{conditions:?}\n{}\n{:?}\n{:?} {}\n{:?}\n{:p}", lib.source, look.without, look.order, look.down, asking_user(self.playlists.served().as_deref()), Arc::as_ptr(&lib.files));
        if let Some((k, at, kept)) = self.sorted.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
            && *k == key
            && at.elapsed() < SORTED_KEEP
        {
            return Ok(kept.clone());
        }
        let sorted = if look.order == CrateOrder::BestMatch {
            // By fit to the asking account's taste; no one asking: its own order.
            let fit = match asking_user(self.playlists.served().as_deref()) {
                Some(user) => self.taste_fits(&user, &look.without).await?,
                None => HashMap::new(),
            };
            let alike = lib.alike();
            let song = |line: &str| Some(crate::serve::ids::song(lib.files[alike.place_of_line(line)?].track?));
            let fit = |l: &str| fit.get(&song(l)?).copied();
            // A Sounds like's crate: Most alike, its closeness first and the taste among those as alike (0.17).
            if crates::ranks_alike(conditions) { Arc::new(crates::most_alike(lines, &lib.resolved(conditions), fit, look.down)) } else { Arc::new(by_fit(lines, fit, look.down)) }
        } else {
            let mut rows = self.playlists.rows_of(lines, &lib.source).await?;
            self.with_plays_and_added(&mut rows, lib).await;
            sort_rows(&mut rows, look.order, look.down);
            Arc::new(rows.into_iter().map(|r| r.entry).collect::<Vec<_>>())
        };
        *self.sorted.lock().unwrap_or_else(|p| p.into_inner()) = Some((key, std::time::Instant::now(), sorted.clone()));
        Ok(sorted)
    }

    /// A crate's rows with what its list orders by besides the files' tags (decision 0060): the asking account's plays
    /// of each track and when it last played it (Cratarr's own plays, by the line's song, whatever server the crate is
    /// shown for), and when it came into the library (the catalogue's first sight of it).
    async fn with_plays_and_added(&self, rows: &mut [PlaylistRow], lib: &CrateLibrary) {
        let facts = crates::by_line(lib.files.iter());
        for r in rows.iter_mut() {
            r.added = facts.get(r.entry.as_str()).and_then(|f| f.born);
        }
        let Some(serve) = self.playlists.served() else { return };
        let Some(me) = serve.current_user() else { return };
        let (c, lines) = (self.playlists.lib().catalogue(), rows.iter().map(|r| r.entry.clone()).collect::<Vec<_>>());
        let plays = crate::run::spawn_blocking(move || {
            let songs: Vec<Option<String>> = lines.iter().map(|l| crate::serve::playlists::song_of_line(&c, l)).collect();
            let known: Vec<String> = songs.iter().flatten().cloned().collect();
            let stats = serve.userdata.play_stats(&me, &known).unwrap_or_default();
            songs.into_iter().map(|s| s.and_then(|s| stats.get(&s).copied())).collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        for (r, p) in rows.iter_mut().zip(plays) {
            if let Some((count, last)) = p {
                r.play_count = Some(count);
                r.last_played = last;
            }
        }
    }

    /// Make playlist: the crate's tracks now, as a new, ordinary playlist (a snapshot; decision 0024).
    /// The crate as its toggles are saved, from the libraries not in `without`. Returns how many tracks it got, and the
    /// undo token.
    pub async fn crate_to_playlist(&self, id: &str, want: Option<&str>, name: &str, without: &[String]) -> Result<(usize, Option<String>), ToggleError> {
        let shown = self.crate_shown(id, &CrateLook::default()).await?;
        let (_, lines) = self.crate_lines_of(&shown, want, without).await?;
        if lines.is_empty() {
            return Err(ToggleError::Refused("That crate is empty.".into()));
        }
        let undo = self.playlists.create_with(name, &lines).await?;
        Ok((lines.len(), undo))
    }

    /// One's own crate, if there is one with that id.
    async fn own_crate(&self, id: &str) -> Result<Option<CrateDef>> {
        Ok(self.own_crates()?.list().await?.into_iter().find(|c| c.id == id))
    }

    /// Saves one's own crate (a new one without `id`); returns its id and the undo token.
    pub async fn crate_save(&self, id: Option<&str>, name: &str, conditions: Vec<crates::Condition>) -> Result<(String, Option<String>), ToggleError> {
        if id.is_some_and(crates::is_permanent) {
            return Err(ToggleError::Refused("A permanent crate cannot be changed.".into()));
        }
        if let Some(why) = crates::not_taken(&conditions) {
            return Err(ToggleError::Unanswerable(why));
        }
        let before = match id {
            Some(id) => self.own_crate(id).await?,
            None => None,
        };
        let id = self.own_crates()?.save(id, name, conditions).await.map_err(ToggleError::Refused)?;
        let after = self.own_crate(&id).await?;
        let undo = (before != after).then(|| self.playlists.keep(Change::Crate { before, after })).flatten();
        Ok((id, undo))
    }

    /// Returns the undo token.
    pub async fn crate_delete(&self, id: &str) -> Result<Option<String>, ToggleError> {
        if crates::is_permanent(id) {
            return Err(ToggleError::Refused("A permanent crate cannot be deleted.".into()));
        }
        let before = self.own_crate(id).await?;
        if !self.own_crates()?.delete(id).await? {
            return Err(ToggleError::UnknownPlaylist);
        }
        Ok(self.playlists.keep(Change::Crate { before, after: None }))
    }

    /// How alike the tracks that sound most like these seeds (song ids; 0.11, quick crates: an album's or an artist's,
    /// `crates::Alike::seed_places`) are, closest first, 0 to 100 (the scale of a Sounds like's `min`, so the page shows
    /// where that cut falls), at most `crates::LIKE_MOST` of them: the Sounds like curve (lib/LikeCurve), cut where the
    /// crate ends.
    /// Over what a seed crate may show, as its tracks are picked (`CrateLibrary::resolved`): the libraries not in
    /// `without`, less the tracks excluded everywhere. With the profile at every step its handle can stop at
    /// (`crates::profiles_along`), so the profile follows a drag (the owner, 2026-10-23). Among only the tracks that pass
    /// the crate's other `conditions` (0.11, quick crates; the owner: "Filtered curve in 0.11"), as its Sounds like picks
    /// its closest among them, so the curve counts what the crate's preview counts. Its seeds read by `mode` (0.13,
    /// decision 0088), as the crate's Sounds like reads them (`crates::closest_fit`).
    #[allow(clippy::too_many_arguments)]
    pub async fn like_curve(
        &self,
        of: &[String],
        weights: &[f32],
        mode: crates::LikeMode,
        want: Option<&str>,
        without: &[String],
        fresh: bool,
        own: Option<bool>,
        conditions: &[crates::Condition],
    ) -> Result<LikeCurve, ToggleError> {
        let lib = self.crate_library_for(conditions, want, without).await?;
        let excluded: HashSet<String> = self.own_crates()?.excluded().await?.into_iter().collect();
        let artists = self.artists_out().await?;
        let (of, weights, lib, conditions) = (of.to_vec(), weights.to_vec(), Arc::new(lib), conditions.to_vec());
        crate::run::spawn_blocking(move || {
            // Its own left out as the crate leaves them (`crates::resolved`): fresh, its track and album seeds; an
            // artist seed's own tracks unless `own` keeps them.
            let out = crates::SeedsOut::of(&of, fresh, own, lib.alike());
            let w = lib.world();
            let among: Vec<&crates::Facts> = lib
                .files()
                .filter(|f| !f.song().is_some_and(|s| excluded.contains(&s)) && !crates::credits_out(f, &artists) && !out.holds(f) && crates::holds_besides_like(&conditions, f, &w))
                .collect();
            let closest = crates::closest_weighted(&of, &weights, mode, lib.alike(), &among, lib.by());
            let alike = closest.iter().map(|(_, a)| (a * 10.0).round() / 10.0).collect();
            let tracks: Vec<&crates::Facts> = closest.iter().map(|(f, _)| *f).collect();
            let scales = crates::Scales::of(&lib.files().collect::<Vec<_>>());
            let profiles = crates::profiles_along(&tracks, &scales).into_iter().map(|(count, profile)| LikeStep { count, profile }).collect();
            LikeCurve { alike, profiles }
        })
        .await
        .map_err(|e| ToggleError::from(anyhow::anyhow!("{e}")))
    }

    /// The axes and their domains in the libraries not in `without`, for the builder (decision 0024, issue 4).
    pub async fn crate_axes(&self, want: Option<&str>, without: &[String]) -> Result<Vec<crates::AxisView>, ToggleError> {
        let lib = self.crate_library(want, false, false, without).await?;
        let one: Vec<[&crates::Facts; 1]> = lib.files().map(|f| [f]).collect();
        let items: Vec<&[&crates::Facts]> = one.iter().map(|f| f.as_slice()).collect();
        Ok(crates::list_axes(&items, &self.playlists.own_playlist_names().await?, None, lib.uses(), super::moment::now_secs()))
    }

    /// What a crate is worked out on: every library's files newest first with their facts, less the folders of those in
    /// `without` (decision 0024, amended 2026-10-23), which playlists hold each line, and (when needed) the favourites,
    /// kept for a minute (decision 0024, issue 5).
    async fn crate_library(&self, want: Option<&str>, favourites: bool, albums: bool, without: &[String]) -> Result<CrateLibrary, ToggleError> {
        let (sid, source) = self.playlists.source_of(want)?;
        // First, so a request waiting for them holds nothing large meanwhile.
        let favourites = if favourites { self.playlists.favourite_lines(&sid, &source).await } else { None };
        let favourite_albums = if albums { favourite_album_lines(self.playlists.served(), self.playlists.lib().catalogue()).await } else { None };
        let c = self.playlists.lib().catalogue();
        let switched_off = self.playlists.lib().folders_off(without).into_iter().collect();
        let files = crate::run::spawn_blocking(move || c.crate_facts()).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        let mut membership: HashMap<String, Vec<String>> = HashMap::new();
        for (name, lines, _) in self.playlists.own_store()?.snapshot().await? {
            for line in lines {
                membership.entry(line).or_default().push(name.clone());
            }
        }
        let by = asking_user(self.playlists.served().as_deref()).and_then(|u| super::crates_from_radio::similar_by_of(&u)).map_or(crate::similarity::Weights::EQUAL, |o| crate::similarity::Weights::ranked(&o));
        let uses = match asking_user(self.playlists.served().as_deref()) {
            Some(user) => {
                let (files, served) = (files.clone(), self.playlists.served());
                crate::run::spawn_blocking(move || uses_of(served.as_deref(), &user, &files)).await.map_err(|e| anyhow::anyhow!("{e}"))?.ok()
            }
            None => None,
        };
        Ok(CrateLibrary { source: sid, files, switched_off, membership, favourites, favourite_albums, alike: Default::default(), uses, by, fits: None })
    }

    /// The asking account's own crates.
    fn own_crates(&self) -> Result<Arc<crates::CrateStore>, ToggleError> {
        self.playlists.crate_stores().of(asking_account(self.playlists.served().as_deref()).as_deref())
    }

    /// The asking account's taste and what it avoids (app::taste; Insights › Listening › Profile): from its favourites,
    /// its own playlists, its plays and skips (the log's, none while it slept), its exclusions; only tracks of the
    /// libraries switched on (`without`: those off, Insights' switches), over the whole library's rarity. `period`
    /// (Insights' one) narrows the plays and skips; favourites, playlists and exclusions have no time.
    /// The sources the account switched off add nothing (decision 0094, `taste::off_of`).
    pub async fn taste(&self, want: Option<&str>, without: &[String], period: super::history::Period) -> Result<super::taste::TasteView, ToggleError> {
        let now = super::moment::now_secs();
        let since = period.since(now).unwrap_or(0);
        let user = asking_user(self.playlists.served().as_deref()).ok_or_else(|| ToggleError::Refused("A taste is an account's own: sign in.".into()))?;
        let serve = self.playlists.served().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let lib = self.crate_library(want, false, false, without).await?;
        let excluded = self.own_crates()?.excluded().await?;
        crate::run::spawn_blocking(move || -> Result<super::taste::TasteView, ToggleError> {
            let alike = lib.alike();
            let place = |song: &str| crate::serve::ids::track_of(song).and_then(|t| alike.place_of_track(t)).filter(|&i| lib.on(i));
            let ud = &serve.userdata;
            let favourites = ud.starred(&user, crate::userdata::Kind::Song)?.iter().filter_map(|s| place(s)).collect();
            let playlists = lib.membership().iter().filter_map(|(line, lists)| Some((alike.place_of_line(line).filter(|&i| lib.on(i))?, lists.len()))).collect();
            // Plays from the log (the counts have no times), finished or counted.
            let mut played: HashMap<usize, u32> = HashMap::new();
            // What was heard while following another is not one's taste (the owner, 2026-10-30).
            let heard = super::history::own(ud.heard_since(&user, since)?);
            for l in heard.iter().filter(|l| l.counted || l.ended == crate::userdata::Ended::Finished) {
                if let Some(i) = place(&l.song) {
                    *played.entry(i).or_default() += 1;
                }
            }
            let plays = played.into_iter().collect();
            let excluded = excluded.iter().filter_map(|song| place(song)).collect();
            let mut skips: HashMap<usize, u32> = HashMap::new();
            for l in heard.iter().filter(|l| l.ended == crate::userdata::Ended::Skipped && !l.counted) {
                if let Some(i) = place(&l.song) {
                    *skips.entry(i).or_default() += 1;
                }
            }
            let sources = super::taste::Sources { favourites, playlists, plays, excluded, skipped: skips.into_iter().collect() };
            Ok(super::taste::taste(alike, &sources, &super::taste::off_of(ud, &user)))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
    }

    /// Of these albums (Cratarr's own), those the asking account has heard a good part of (`HEARD_SHARE` of their length,
    /// by their tracks' own plays times their lengths, none heard while following, `album_heard`): what New for you
    /// leaves out (the owner, 2026-10-23).
    pub async fn heard_albums(&self, ids: Vec<String>) -> Result<Vec<String>, ToggleError> {
        let user = asking_user(self.playlists.served().as_deref()).ok_or_else(|| ToggleError::Refused("What was heard is an account's own: sign in.".into()))?;
        let serve = self.playlists.served().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        crate::run::spawn_blocking(move || -> Result<Vec<String>, ToggleError> {
            let plays = serve.userdata.own_song_plays(&user)?;
            let mut heard = Vec::new();
            for id in ids {
                if album_heard(&serve, &plays, &id)? == Some(true) {
                    heard.push(id);
                }
            }
            Ok(heard)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
    }

    /// New for you, Home's shelf and its crate alike (the owner, 2026-10-23 and 2026-10-30): of Cratarr's own library
    /// (every folder in use, Music's and the Incoming ones', as the crates are), the newest albums by when the library
    /// added them, less those without a cover and those the asking account heard `HEARD_SHARE` of (one's own plays, none
    /// heard while following), at most `NEW_PER_ARTIST` of one album artist (`new_picks`); none whose album artist
    /// the account excluded everywhere (decision 0083). Their ids, newest first. Without an account to ask about
    /// (sign-in off), none is heard.
    pub async fn new_for_you(&self) -> Result<Vec<String>, ToggleError> {
        let user = asking_user(self.playlists.served().as_deref());
        let serve = self.playlists.served().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let artists = self.artists_out().await?;
        crate::run::spawn_blocking(move || -> Result<Vec<String>, ToggleError> {
            let plays = match &user {
                Some(user) => serve.userdata.own_song_plays(user)?,
                None => HashMap::new(),
            };
            let coverless = serve.library.coverless()?;
            new_picks(
                |offset| {
                    let page = serve.library.album_list(&crate::serve::read::AlbumOrder::Newest, NEW_PAGE, offset, None)?;
                    // Those the library has no date for come last, and none of them is new: the walk ends at the first.
                    Ok(page
                        .into_iter()
                        .take_while(|a| a.created.is_some())
                        .map(|a| {
                            let compilation = a.is_compilation || a.release_types.iter().any(|t| t == "Compilation");
                            (a.id, a.artist, compilation)
                        })
                        .collect())
                },
                |id| Ok(coverless.contains(id) || album_heard(&serve, &plays, id)? != Some(false)),
                &artists,
            )
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
    }

    /// Home's two looks back (the owner, 2026-10-23), of the libraries switched on: Blast from the past, the favourite
    /// tracks unplayed longest (played before: a favourite never played was never buried); Grave digging, the albums
    /// most like the taste (`taste::profiles`, the sources switched on) and furthest from the listening, never played
    /// furthest. Both read one's own plays, none heard while following (the owner, 2026-10-30).
    pub async fn digging(&self, want: Option<&str>, without: &[String]) -> Result<Digging, ToggleError> {
        let user = asking_user(self.playlists.served().as_deref()).ok_or_else(|| ToggleError::Refused("Home's looks back are an account's own: sign in.".into()))?;
        let serve = self.playlists.served().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let lib = self.crate_library(want, false, false, without).await?;
        let excluded = self.own_crates()?.excluded().await?;
        // Neither shows what credits an artist excluded everywhere (decision 0083): a track by its credits, an album by
        // its album artist.
        let artists = self.artists_out().await?;
        crate::run::spawn_blocking(move || -> Result<Digging, ToggleError> {
            let alike = lib.alike();
            let place = |song: &str| crate::serve::ids::track_of(song).and_then(|t| alike.place_of_track(t)).filter(|&i| lib.on(i));
            // Grave digging is of every library, whatever is switched off: its crate is Home's (the owner's one exception,
            // 2026-11-02); Blast from the past of those switched on.
            let place_any = |song: &str| crate::serve::ids::track_of(song).and_then(|t| alike.place_of_track(t));
            let ud = &serve.userdata;
            let plays = ud.own_song_plays(&user)?;
            let starred = ud.starred(&user, crate::userdata::Kind::Song)?;
            // Blast from the past: the favourites by their last play, the oldest first.
            let mut past: Vec<(String, i64)> = starred
                .iter()
                .filter(|s| place(s).is_some_and(|i| !crates::credits_out(&lib.files[i], &artists)))
                .filter_map(|s| Some((s.clone(), plays.get(s)?.1?)))
                .collect();
            past.sort_by_key(|p| p.1);
            // Of albums with a cover only (Home shows none without, the owner 2026-10-24).
            let coverless = serve.library.coverless()?;
            let album_of: HashMap<String, String> = serve.library.albums_of(&past.iter().map(|p| p.0.clone()).collect::<Vec<_>>())?.into_iter().collect();
            past.retain(|(s, _)| album_of.get(s).is_none_or(|a| !coverless.contains(a)));
            past.truncate(DIG);
            let songs: HashMap<String, crate::serve::api::Song> = serve.library.songs_by_id(&past.iter().map(|p| p.0.clone()).collect::<Vec<_>>())?.into_iter().map(|s| (s.id.clone(), s)).collect();
            let past = past
                .into_iter()
                .filter_map(|(id, last)| {
                    let s = songs.get(&id)?;
                    Some(DugTrack { id, name: s.title.clone(), artist: s.artist.clone(), album: s.album.clone(), album_id: s.album_id.clone(), last_played: last })
                })
                .collect();
            // Grave digging: the taste as Insights' Profile has it, the sources switched on; the albums played before,
            // each as one profile against it; of the closest, the ones unplayed longest.
            let taste = taste_in(&lib, ud, &user, &excluded)?;
            // Every album of every library, its tracks' places; its last play, and how much its artist is played (of the
            // most played artist's plays).
            let songs = serve.library.all_song_ids()?;
            let mut by_album: HashMap<String, Vec<(usize, f64)>> = HashMap::new();
            for (song, album) in serve.library.albums_of(&songs)? {
                if let Some(i) = place_any(&song) {
                    by_album.entry(album).or_default().push((i, 1.0));
                }
            }
            let last: HashMap<String, i64> = ud.used(&user, crate::userdata::Kind::Album)?.into_iter().filter_map(|(id, a)| Some((id, a.own_date?))).collect();
            let artist_plays: HashMap<String, u32> = ud.used(&user, crate::userdata::Kind::Artist)?.into_iter().map(|(id, a)| (id, a.own_count)).collect();
            let most = artist_plays.values().copied().max().unwrap_or(0).max(1) as f64;
            let ids: Vec<String> = by_album.keys().cloned().collect();
            let found = serve.library.albums_by_id(&ids)?;
            let out: HashSet<String> = found.iter().filter(|a| album_out(a, &artists)).map(|a| a.id.clone()).collect();
            let artist_of: HashMap<String, String> = found.into_iter().map(|a| (a.id, a.artist_id)).collect();
            let now = super::moment::now_secs();
            // How far an album is from the listening (the owner, 2026-10-23: weigh the unplayed, "the albums that are
            // totally opposite our actual listening behaviour while matching our profile"): never played, 1; played,
            // nearer the more lately (half at UNPLAYED_HALF_DAYS); an artist played much pulls it nearer, up to half.
            let distance = |id: &str| {
                let unplayed = match last.get(id) {
                    None => 1.0,
                    Some(&t) => {
                        let days = ((now - t).max(0) as f64) / 86_400.0;
                        days / (days + UNPLAYED_HALF_DAYS)
                    }
                };
                let familiar = artist_of.get(id).and_then(|a| artist_plays.get(a)).map_or(0.0, |&n| n as f64 / most);
                unplayed * (1.0 - 0.5 * familiar)
            };
            // Home shows no album without a cover (the owner, 2026-10-24).
            let coverless = serve.library.coverless()?;
            let mut albums: Vec<(String, f64)> = by_album
                .iter()
                .filter(|(id, _)| !coverless.contains(*id) && !out.contains(*id))
                .filter_map(|(id, set)| Some((id.clone(), alike.compare(&taste, &alike.profile(set), lib.by())? * distance(id))))
                .collect();
            albums.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let graves = albums.into_iter().take(DIG).map(|(id, _)| DugAlbum { last_played: last.get(&id).copied(), id }).collect();
            Ok(Digging { past, graves })
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
    }

        /// Home's moment (the owner, 2026-10-23; `moment::moment`): its name, and a few albums near it of the libraries
    /// switched on (their heard tracks, on average), one per artist, those played lately left out; a new draw of the
    /// nearest each day and each moment. `tz`: the page's clock (seconds east of UTC).
    pub async fn moment(&self, want: Option<&str>, without: &[String], tz: Option<i64>) -> Result<MomentView, ToggleError> {
        let user = asking_user(self.playlists.served().as_deref()).ok_or_else(|| ToggleError::Refused("Home's moment is an account's own: sign in.".into()))?;
        let serve = self.playlists.served().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        if let Some(tz) = tz {
            super::moment::page_tz(tz);
        }
        let m = super::moment::now();
        let lib = self.crate_library(want, false, false, without).await?;
        let artists_out = self.artists_out().await?;
        crate::run::spawn_blocking(move || -> Result<MomentView, ToggleError> {
            let alike = lib.alike();
            let scales = crates::Scales::of(&lib.files().collect::<Vec<_>>());
            let off = |song: &str| {
                let i = crate::serve::ids::track_of(song).and_then(|t| alike.place_of_track(t)).filter(|&i| lib.on(i))?;
                scales.off(&lib.files[i], &m.target)
            };
            // Per album: how many of its tracks were measured, how far they are in all, and how many it has.
            let mut by_album: HashMap<String, (u32, f64, u32)> = HashMap::new();
            for (song, album) in serve.library.albums_of(&serve.library.all_song_ids()?)? {
                let a = by_album.entry(album).or_default();
                a.2 += 1;
                if let Some(o) = off(&song) {
                    a.0 += 1;
                    a.1 += o;
                }
            }
            let now = super::moment::now_secs();
            let rested = now - MOMENT_RESTED_DAYS * 86_400;
            // Played lately as one's own: what was heard while following leaves an album in (the owner, 2026-10-30).
            let last: HashMap<String, i64> = serve.userdata.used(&user, crate::userdata::Kind::Album)?.into_iter().filter_map(|(id, a)| Some((id, a.own_date?))).collect();
            let mut near: Vec<(String, f64)> = by_album
                .into_iter()
                .filter(|(id, (n, _, all))| *n >= MOMENT_TRACKS && *n * 2 >= *all && last.get(id).is_none_or(|&t| t < rested))
                .map(|(id, (n, sum, _))| (id, sum / f64::from(n)))
                .collect();
            // Home shows no album without a cover (the owner, 2026-10-24).
            let coverless = serve.library.coverless()?;
            near.retain(|(id, _)| !coverless.contains(id));
            near.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            near.truncate(MOMENT_POOL * 4);
            let found = serve.library.albums_by_id(&near.iter().map(|a| a.0.clone()).collect::<Vec<_>>())?;
            // None whose album artist is excluded everywhere (decision 0083).
            let out: HashSet<&str> = found.iter().filter(|a| album_out(a, &artists_out)).map(|a| a.id.as_str()).collect();
            near.retain(|(id, _)| !out.contains(id.as_str()));
            let artist_of: HashMap<String, String> = found.iter().map(|a| (a.id.clone(), a.artist_id.clone())).collect();
            let mut artists = HashSet::new();
            let mut pool: Vec<(String, f64)> = near.into_iter().filter(|(id, _)| artist_of.get(id).is_none_or(|a| artists.insert(a.clone()))).take(MOMENT_POOL).collect();
            // A draw of the pool that holds through the day and the moment, then nearest first.
            let draw = |id: &str| {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                (id, now.div_euclid(86_400), &m.name).hash(&mut h);
                h.finish()
            };
            pool.sort_by_key(|a| draw(&a.0));
            pool.truncate(MOMENT_ALBUMS);
            pool.sort_by(|a, b| a.1.total_cmp(&b.1));
            Ok(MomentView { name: m.name, sky: m.sky, dark: m.dark, albums: pool.into_iter().map(|a| a.0).collect(), weather: super::moment::told(now) })
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
    }

    /// How well each track of the libraries not in `without` fits `user`'s taste (`taste_in`, `Alike::fit`; decision
    /// 0094: Best match, Radio's Your taste), by its song id; none while there is no taste.
    pub async fn taste_fits(&self, user: &str, without: &[String]) -> Result<HashMap<String, f64>, ToggleError> {
        self.fits_to(user, without, taste_in).await
    }

    /// How well these tracks (song ids) fit the asking account's taste (decision 0094: Best match on every list; the
    /// lists the page orders itself, a playlist's, a record's, an artist's, order by these): those whose fit is known.
    /// Every library's, kept a little while (`SORTED_KEEP`). Signed out: refused.
    pub async fn fits_of(&self, ids: &[String]) -> Result<HashMap<String, f64>, ToggleError> {
        let user = asking_user(self.playlists.served().as_deref()).ok_or_else(|| ToggleError::Refused("Best match is an account's own: sign in.".into()))?;
        let kept = self.fits.lock().unwrap_or_else(|p| p.into_inner()).as_ref().filter(|(u, at, _)| *u == user && at.elapsed() < SORTED_KEEP).map(|(_, _, f)| f.clone());
        let all = match kept {
            Some(all) => all,
            None => {
                let all = Arc::new(self.taste_fits(&user, &[]).await?);
                *self.fits.lock().unwrap_or_else(|p| p.into_inner()) = Some((user, std::time::Instant::now(), all.clone()));
                all
            }
        };
        Ok(ids.iter().filter_map(|id| Some((id.clone(), *all.get(id)?))).collect())
    }

    /// What `user` did with each track, by its id: the crates' Play count, Last played, Rating and Skips, as `uses_of` has them by
    /// line, for a device to count on when it cannot ask (0101's point 18).
    pub async fn uses_by_track(&self, user: &str) -> Result<HashMap<i64, crates::Use>, ToggleError> {
        let (serve, cat, user) = (self.playlists.served(), self.playlists.lib().catalogue(), user.to_string());
        crate::run::spawn_blocking(move || -> Result<HashMap<i64, crates::Use>, ToggleError> {
            let files = cat.crate_facts()?;
            let by_line = uses_of(serve.as_deref(), &user, &files)?;
            Ok(files.iter().filter_map(|f| Some((f.track?, *by_line.get(&f.rel)?))).collect())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
    }

    /// How like each track of the libraries not in `without` is to what `user` skips (`skipped_in`; decision 0094:
    /// skips keep tracks away in Radio and Autoplay), by its song id; none while nothing is skipped or skips are off.
    pub async fn skipped_fits(&self, user: &str, without: &[String]) -> Result<HashMap<String, f64>, ToggleError> {
        self.fits_to(user, without, skipped_in).await
    }

    /// The lines of `lib`'s libraries switched on (those not in `without`) that fit the asking account's taste (Fits
    /// your taste): the best fitting `crates::FITS_SHARE` of those whose fit is known, as Best match orders them.
    /// Signed out, or no taste yet: refused.
    async fn fitting(&self, lib: &CrateLibrary, without: &[String]) -> Result<HashSet<String>, ToggleError> {
        let user = asking_user(self.playlists.served().as_deref()).ok_or_else(|| ToggleError::CannotPlay("Fits your taste is an account's own: sign in.".into()))?;
        let mut fit: Vec<(String, f64)> = self.taste_fits(&user, without).await?.into_iter().collect();
        if fit.is_empty() {
            return Err(ToggleError::CannotPlay("Fits your taste needs a taste: favourites, playlists or plays.".into()));
        }
        fit.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let keep = ((fit.len() as f64 * crates::FITS_SHARE).ceil() as usize).max(1);
        let songs: HashSet<String> = fit.into_iter().take(keep).map(|(song, _)| song).collect();
        Ok(lib.files.iter().filter(|f| f.track.is_some_and(|t| songs.contains(&crate::serve::ids::song(t)))).map(|f| f.rel.clone()).collect())
    }

    async fn fits_to(&self, user: &str, without: &[String], profile: ProfileOf) -> Result<HashMap<String, f64>, ToggleError> {
        let serve = self.playlists.served().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let lib = self.crate_library(None, false, false, without).await?;
        let excluded = self.own_crates()?.excluded().await?;
        let user = user.to_string();
        crate::run::spawn_blocking(move || -> Result<HashMap<String, f64>, ToggleError> {
            let taste = profile(&lib, &serve.userdata, &user, &excluded)?;
            let alike = lib.alike();
            Ok(lib.files.iter().enumerate().filter(|(i, _)| lib.on(*i)).filter_map(|(i, f)| Some((crate::serve::ids::song(f.track?), alike.fit(&taste, i, lib.by())?))).collect())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
    }

    /// An artist's Top recommendations (the owner, 2026-10-23: "a list of tracks which follows taste", in place of
    /// their most played): their tracks of the libraries switched on, those that fit the asking account's taste best
    /// first (`taste_in`, `Alike::fit`: as Best match), at most `count`, as song ids.
    /// `only`: their tracks by song id where the page knows the artist by id (G6), so a namesake's tracks are not theirs; else by name.
    pub async fn artist_picks(&self, name: &str, without: &[String], count: usize, only: Option<std::collections::HashSet<String>>) -> Result<Vec<String>, ToggleError> {
        let user = asking_user(self.playlists.served().as_deref()).ok_or_else(|| ToggleError::Refused("Recommendations are an account's own: sign in.".into()))?;
        let serve = self.playlists.served().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let lib = self.crate_library(None, false, false, without).await?;
        let excluded = self.own_crates()?.excluded().await?;
        let name = name.to_string();
        crate::run::spawn_blocking(move || -> Result<Vec<String>, ToggleError> {
            let taste = taste_in(&lib, &serve.userdata, &user, &excluded)?;
            let alike = lib.alike();
            let mut fit: Vec<(String, f64)> = lib
                .files
                .iter()
                .enumerate()
                .filter(|(i, f)| lib.on(*i) && (only.is_some() || f.artists.iter().any(|a| a.eq_ignore_ascii_case(&name))))
                .filter_map(|(i, f)| Some((crate::serve::ids::song(f.track?), i)))
                .filter(|(song, _)| only.as_ref().is_none_or(|o| o.contains(song)))
                .filter_map(|(song, i)| Some((song, alike.fit(&taste, i, lib.by())?)))
                .collect();
            fit.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            Ok(fit.into_iter().take(count).map(|(id, _)| id).collect())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
    }
}

#[cfg(test)]
mod card_tests {
    use super::*;
    use crate::serve::library::RelSong;

    #[test]
    fn a_crates_card_counts_its_lines() {
        let now = 1_000_000_000;
        let facts = [
            crates::Facts { rel: "A/1.flac".into(), genres: vec!["Dub".into(), "Reggae".into()], year: None, bpm: None, ..Default::default() },
            crates::Facts { rel: "A/2.flac".into(), genres: vec!["dub".into()], year: None, bpm: None, ..Default::default() },
            crates::Facts { rel: "B/1.flac".into(), genres: vec!["Jazz".into()], year: None, bpm: None, ..Default::default() },
        ];
        let by: HashMap<&str, &crates::Facts> = facts.iter().map(|f| (f.rel.as_str(), f)).collect();
        let song = |id: &str, album: &str, ms: i64, created: i64| RelSong { song: id.into(), album: album.into(), duration_ms: Some(ms), created: Some(created), genres: Vec::new() };
        let songs: HashMap<String, RelSong> = [
            ("old.flac".to_string(), song("tr-0", "al-z", 1_000, now - 300 * 86_400)),
            ("A/1.flac".to_string(), song("tr-1", "al-a", 200_000, now - 30 * 86_400)),
            ("A/2.flac".to_string(), song("tr-2", "al-a", 100_500, now - 30 * 86_400)),
            ("B/1.flac".to_string(), song("tr-3", "al-b", 60_000, now - 86_400)),
        ]
        .into_iter()
        .collect();
        let played: HashSet<String> = ["tr-1".to_string()].into_iter().collect();
        let lines: Vec<String> = ["A/1.flac", "A/2.flac", "B/1.flac", "gone.flac"].iter().map(|s| s.to_string()).collect();
        let s = crate_stats(&lines, &by, &songs, Some(&played), now);
        assert_eq!((s.tracks, s.seconds, s.unplayed, s.new_week), (4, 360, Some(2), 1));
        assert_eq!(s.genres, vec!["Dub", "Jazz", "Reggae"], "spelt as first met, counted regardless of capitals");
        assert_eq!(s.covers.iter().map(|c| c.album_id.as_str()).collect::<Vec<_>>(), vec!["al-a", "al-b"]);
        assert_eq!(crate_stats(&lines, &by, &songs, None, now).unplayed, None, "no owner: not known");
        assert_eq!(crates::image_file("c-12ab"), "c-12ab.jpg");
        assert_eq!(crates::image_file("../x"), "x2e2e2f78.jpg");
    }
}

#[cfg(test)]
mod undo_tests {
    use super::*;
    use super::super::library::Library;
    use super::super::playlists::testing::refused;
    use crate::journal::Journal;
    use std::path::PathBuf;

    /// The playlists' tests' service (`playlists::testing`), with the crates over it.
    async fn service() -> (PathBuf, CrateService) {
        service_with(None).await
    }

    /// `service`, with Cratarr's own library as `main` hands it.
    async fn service_with(served: Option<Arc<crate::serve::Serve>>) -> (PathBuf, CrateService) {
        let (d, playlists) = super::super::playlists::testing::service_with(served).await;
        (d, CrateService::new(Arc::new(playlists)))
    }

    #[tokio::test]
    async fn a_bandcamp_preview_goes_in_no_playlist_and_seeds_no_crate() {
        let (d, s) = service().await;
        s.create("Chill").await.unwrap();
        let preview = crate::bandcamp::track_id("https://a.bandcamp.com/album/b", 1);
        let added = s.append_tracks("Chill", crate::bandcamp::SOURCE, std::slice::from_ref(&preview), false).await;
        assert!(matches!(added, Err(ToggleError::Refused(m)) if m == crate::bandcamp::NOT_KEPT));
        assert!(s.entries("Chill").await.unwrap().is_empty());
        let like = crates::like_crate("like", "Like it", vec![preview], crates::LIKE_COUNT).conditions;
        assert!(matches!(s.crate_save(None, "Like it", like).await, Err(ToggleError::Refused(m)) if m == crate::bandcamp::NOT_KEPT));
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn crates_and_spoken_names_are_put_back() {
        let (d, s) = service().await;
        let cond = |v: &str| crates::Condition::new("genre", false, crates::Value::AnyOf { values: vec![v.into()] });
        let (id, made) = s.crate_save(None, "Jazz", vec![cond("Jazz")]).await.unwrap();
        let (_, changed) = s.crate_save(Some(&id), "Jazzy", vec![cond("Bebop")]).await.unwrap();
        s.undo(&changed.unwrap()).await.unwrap();
        assert_eq!(s.crate_def(&id).await.unwrap().name, "Jazz");
        let gone = s.crate_delete(&id).await.unwrap().unwrap();
        assert!(s.crate_def(&id).await.is_err());
        s.undo(&gone).await.unwrap();
        assert_eq!(s.crate_def(&id).await.unwrap().conditions, vec![cond("Jazz")], "back with its own id");
        s.crate_save(Some(&id), "Jazz", vec![cond("Swing")]).await.unwrap();
        assert_eq!(refused(s.undo(&made.unwrap()).await), "It changed since; nothing was undone.");

        s.create("Chill").await.unwrap();
        let said = s.set_spoken("Chill", &["calm".into()]).await.unwrap().unwrap();
        assert_eq!(s.set_spoken("Chill", &["calm".into()]).await.unwrap(), None, "no change, no token");
        s.undo(&said).await.unwrap();
        assert!(s.sync().spoken().remove("Chill").unwrap_or_default().is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    /// `service` with its crates and pictures in a data folder, as a take finds them, and the take: `tr-5` went to `tr-1`.
    async fn before_a_take(name: &str) -> (PathBuf, CrateService, crate::moved::Moved) {
        let d = std::env::temp_dir().join(format!("cratarr-take-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        let dir = d.to_string_lossy().into_owned();
        let db = Arc::new(crate::userdata::UserData::open(None).unwrap());
        let lib = Library::new(Some(dir.clone()), Some(dir), Vec::new(), Default::default(), None, db, None).await.unwrap();
        let journal = Arc::new(Journal::open(None).unwrap());
        let sync = Arc::new(super::super::sync::SyncService::new(lib.clone(), crate::sync_state::SyncState::open(None).unwrap(), journal.clone()));
        let moved = crate::moved::Moved { ids: [("tr-5".to_string(), "tr-1".to_string())].into(), ..Default::default() };
        (d, CrateService::new(Arc::new(PlaylistService::new(lib, sync, journal, None))), moved)
    }

    /// A take names no account, so one store's trouble is no reason to leave the others: each is followed, and the answer says one
    /// was not, so nothing marks it done. The next check goes over them all.
    #[tokio::test]
    async fn a_store_that_cannot_be_read_stops_no_other_and_is_gone_over_again() {
        let (d, s, taken) = before_a_take("unread").await;
        let others = ["bob", "cy", "dan", "eve", "fay"];
        let excluded = r#"{ "version": 1, "crates": [], "excluded": ["tr-5"] }"#;
        for account in others {
            std::fs::create_dir_all(d.join("users").join(account)).unwrap();
            std::fs::write(d.join("users").join(account).join("crates.json"), excluded).unwrap();
        }
        std::fs::create_dir_all(d.join("users/amy")).unwrap();
        std::fs::write(d.join("users/amy/crates.json"), "{ half written").unwrap();

        assert!(s.follow_tracks(&taken).await.is_err(), "not all followed");
        for account in others {
            assert_eq!(s.crate_stores().of(Some(account)).unwrap().excluded().await.unwrap(), ["tr-1"], "{account}");
        }
        std::fs::write(d.join("users/amy/crates.json"), excluded).unwrap();
        s.follow_tracks(&taken).await.unwrap();
        assert_eq!(s.crate_stores().of(Some("amy")).unwrap().excluded().await.unwrap(), ["tr-1"], "mended, the next check reaches it");
        std::fs::remove_dir_all(&d).ok();
    }

    /// A Sounds like's picture is named by its seed: left behind, the crate shows its albums' covers again.
    #[tokio::test]
    async fn a_sounds_likes_picture_goes_with_its_seed_at_a_take() {
        let (d, s, taken) = before_a_take("picture").await;
        std::fs::create_dir_all(d.join("crate-images")).unwrap();
        std::fs::write(d.join("crate-images").join(crates::image_file("like:tr-5")), "a picture").unwrap();
        s.follow_tracks(&taken).await.unwrap();
        assert!(d.join("crate-images").join(crates::image_file("like:tr-1")).is_file());
        std::fs::remove_dir_all(&d).ok();
    }

    /// Undo holds a list against what the change left, so a change from before a take must read as the list does now, or it says
    /// the list changed since.
    #[tokio::test]
    async fn an_undo_from_before_a_take_finds_the_list_as_it_left_it() {
        let (d, s, taken) = before_a_take("undo").await;
        let (first, left) = (s.crate_stores().first().clone(), crates::LeftOut { tracks: vec!["tr-5".into()], albums: Vec::new() });
        first.set_left_out("c1", left.clone()).await.unwrap();
        let undo = s.keep(Change::LeftOut { crate_id: "c1".into(), before: crates::LeftOut::default(), after: left }).unwrap();
        s.follow_tracks(&taken).await.unwrap();
        assert_eq!(first.left_out("c1").await.unwrap().tracks, ["tr-1"]);
        s.undo(&undo).await.unwrap();
        assert!(first.left_out("c1").await.unwrap().is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn all_of_where_a_track_has_one_value_is_refused_saved_previewed_or_drawn() {
        // 0.13, decision 0088: refused as unanswerable (422), naming the axis; on the axes of several values, kept.
        let (d, s) = service().await;
        let years = vec![crates::Condition::new("year", false, crates::Value::AllOf { values: vec!["1994".into(), "2005".into()] })];
        let said = |e: Option<ToggleError>| match e {
            Some(ToggleError::Unanswerable(m)) => m,
            other => panic!("not refused as unanswerable: {other:?}"),
        };
        assert!(said(s.crate_save(None, "Years", years.clone()).await.err()).contains("Year"));
        assert!(said(s.crate_page(&years, None, 0, 50, &CrateLook::default()).await.err()).contains("Year"));
        assert!(said(s.crate_lines_in_order(&years, None, &CrateLook::default()).await.err()).contains("Year"), "what Play plays");
        assert!(said(s.crate_profile(&years, None, &[]).await.err()).contains("Year"), "its profile, asked while it is made");
        let curve = s.like_curve(&["tr-1".to_string()], &[], crates::LikeMode::Either, None, &[], false, None, &years).await;
        assert!(said(curve.err()).contains("Year"));
        let jazz_funk = vec![crates::Condition::new("genre", false, crates::Value::AllOf { values: vec!["Jazz".into(), "Funk".into()] })];
        let (id, _) = s.crate_save(None, "Jazz-funk", jazz_funk.clone()).await.unwrap();
        assert_eq!(s.crate_def(&id).await.unwrap().conditions, jazz_funk);
        assert!(s.condition_names(&jazz_funk).await.unwrap().is_empty(), "no library ids, no names");
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn not_in_this_crate_goes_in_ones_own_crates_filter_once_and_is_taken_back() {
        // Decision 0083: one's own crate's "Artist is not", as set by hand; its undo the crate's.
        let (d, s) = service().await;
        let cond = |v: &str| crates::Condition::new("genre", false, crates::Value::AnyOf { values: vec![v.into()] });
        let not_air = crates::Condition::new("artist", true, crates::Value::AnyOf { values: vec!["Air".into()] });
        let (id, _) = s.crate_save(None, "Jazz", vec![cond("Jazz")]).await.unwrap();
        let (other, _) = s.crate_save(None, "Bebop", vec![cond("Bebop")]).await.unwrap();
        let (listed, undo) = s.crate_not_artist(&id, "Air").await.unwrap();
        assert_eq!(listed, [NotArtist { key: "air".into(), name: "Air".into() }]);
        let (_, again) = s.crate_not_artist(&id, "AIR").await.unwrap();
        assert!(undo.is_some() && again.is_none(), "once");
        assert_eq!(s.crate_def(&id).await.unwrap().conditions, vec![cond("Jazz"), not_air.clone()]);
        assert_eq!(s.crate_def(&other).await.unwrap().conditions, vec![cond("Bebop")], "that crate only");
        let (left, back) = s.crate_not_artist_back(&id, "air").await.unwrap();
        assert!(left.is_empty());
        assert_eq!(s.crate_def(&id).await.unwrap().conditions, vec![cond("Jazz")], "taken back");
        assert_eq!(s.crate_not_artist_back(&id, "air").await.unwrap().1, None, "nothing left to take back");
        s.undo(&back.unwrap()).await.unwrap();
        assert_eq!(s.crate_def(&id).await.unwrap().conditions, vec![cond("Jazz"), not_air], "the take-back undone");
        assert!(matches!(s.crate_not_artist(&id, "  ").await, Err(ToggleError::Refused(_))));
        assert!(matches!(s.crate_not_artist("c-none", "Air").await, Err(ToggleError::UnknownPlaylist)));
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn a_fixed_crate_keeps_the_artists_left_out_of_it_beside_it() {
        // Decision 0083: a fixed crate has no filter of one's own; what is added is kept by its id, applied with its
        // own conditions.
        let (d, s) = service().await;
        let not_air = crates::Condition::new("artist", true, crates::Value::AnyOf { values: vec!["Air".into()] });
        let (listed, undo) = s.crate_not_artist("genre:Jazz", "Air").await.unwrap();
        assert_eq!(listed, [NotArtist { key: "air".into(), name: "Air".into() }]);
        assert_eq!(s.crate_def("genre:Jazz").await.unwrap().conditions[1..], [not_air]);
        assert_eq!(s.crate_def("genre:Rock").await.unwrap().conditions.len(), 1, "that crate only");
        assert_eq!(s.crate_not_artist("genre:Jazz", "air").await.unwrap().1, None, "once");
        assert_eq!(s.crate_not_artists("genre:Jazz").await.unwrap().len(), 1);
        // The permanent Unlisted, as the Crates page lists it.
        s.crate_not_artist("unlisted", "Bonobo").await.unwrap();
        assert_eq!(s.permanent_crates().await.unwrap()[0].conditions.len(), 2);
        let (left, _) = s.crate_not_artist_back("unlisted", "bonobo").await.unwrap();
        assert!(left.is_empty() && s.permanent_crates().await.unwrap()[0].conditions.len() == 1, "taken back");
        s.undo(&undo.unwrap()).await.unwrap();
        assert!(s.crate_not_artists("genre:Jazz").await.unwrap().is_empty(), "undone");
        assert_eq!(s.crate_def("genre:Jazz").await.unwrap().conditions.len(), 1);
        std::fs::remove_dir_all(&d).ok();
    }

    /// `data` section 9: a Sounds like kept as a link goes to the crate of what its seed became, its count kept. Anything else is
    /// what it says it is.
    #[tokio::test]
    async fn an_old_sounds_like_is_the_crate_of_what_its_seed_became() {
        let (d, s) = service().await;
        s.lib().catalogue().artist_for("Björk").unwrap();
        s.lib().catalogue().note_artists_moved().unwrap();
        let old = "ar-5vcUvYmuWJAgyxLXEUmaPz";
        assert_eq!(s.like_now(&format!("like:{old}@50")).await.as_deref(), Some("like:ar-1@50"));
        assert_eq!(s.like_now(&format!("like:{old}")).await.as_deref(), Some("like:ar-1"));
        for stands in ["like:ar-1", "like:al-5vcUvYmuWJAgyxLXEUmaPz", "like:list:Chill", "genre:Dub", "c-mine"] {
            assert_eq!(s.like_now(stands).await, None, "{stands}");
        }
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn an_artist_is_excluded_everywhere_once_per_account_and_taken_back() {
        let served = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        served.users.set("me", "pw").unwrap();
        served.sessions.set_required(false).unwrap();
        let (d, s) = service_with(Some(served.clone())).await;
        let (a, undo) = s.exclude_artist("The Avalanches").await.unwrap();
        assert_eq!((a.key.as_str(), a.name.as_str()), ("avalanches", "The Avalanches"));
        assert!(undo.is_some());
        assert_eq!(s.exclude_artist("the avalanches").await.unwrap().1, None, "once, by their key");
        assert_eq!(s.artists_out().await.unwrap(), HashSet::from(["avalanches".to_string()]), "what every pick reads");
        assert_eq!(s.excluded_songs_of(Some("me")).await.unwrap(), HashSet::new(), "no file credits them here");
        assert!(served.userdata.excluded_artists("you").unwrap().is_empty(), "each account its own");
        let (key, back) = s.include_artist("avalanches").await.unwrap();
        assert!(key == "avalanches" && s.excluded_artist_list().await.unwrap().is_empty(), "taken back");
        assert_eq!(s.include_artist("avalanches").await.unwrap().1, None, "not on it any more");
        s.undo(&back.unwrap()).await.unwrap();
        assert_eq!(s.excluded_artist_list().await.unwrap().len(), 1, "the take-back undone");
        assert!(matches!(s.exclude_artist(" ").await, Err(ToggleError::Refused(_))));
        let (d2, none) = service().await;
        assert!(matches!(none.exclude_artist("Air").await, Err(ToggleError::Refused(_))), "Cratarr's own library is needed");
        assert!(none.artists_out().await.unwrap().is_empty());
        for d in [d, d2] {
            std::fs::remove_dir_all(&d).ok();
        }
    }

    #[test]
    fn radio_and_autoplay_leave_out_every_song_crediting_an_artist_excluded_everywhere() {
        // Decision 0083: the songs `excluded_songs_of` adds, which the radio marks excluded (`radio::ready`).
        let f = |track: i64, artists: &[&str]| {
            let (credits, credit_names) = crate::app::excluded_artists::credited(artists);
            crates::Facts { rel: format!("{track}.flac"), track: Some(track), artists: artists.iter().map(|a| a.to_string()).collect(), credits, credit_names, ..Default::default() }
        };
        let facts = [f(1, &["Air"]), f(2, &["Bonobo feat. Air"]), f(3, &["Bonobo"]), f(4, &["Various", "Air"])];
        let out = HashSet::from(["air".to_string()]);
        assert_eq!(songs_crediting(&facts, &out), [crate::serve::ids::song(1), crate::serve::ids::song(2), crate::serve::ids::song(4)]);
        assert!(songs_crediting(&facts, &HashSet::new()).is_empty());
    }

    #[test]
    fn an_album_is_out_when_its_album_artist_is() {
        let album = |artist: &str, others: &[&str]| crate::serve::api::Album {
            artist: artist.into(),
            artists: others.iter().map(|n| crate::serve::api::ArtistRef { id: String::new(), name: n.to_string() }).collect(),
            ..Default::default()
        };
        let out = HashSet::from(["beck".to_string()]);
        assert!(album_out(&album("Beck", &[]), &out));
        assert!(album_out(&album("Air & Beck", &[]), &out), "a collaboration");
        assert!(album_out(&album("Air", &["Air", "Beck"]), &out), "any of its album artists");
        assert!(!album_out(&album("Air", &["Air"]), &out));
    }
}

#[cfg(test)]
mod left_out_tests {
    use super::*;

    #[test]
    fn what_a_crate_leaves_out_is_its_tracks_its_albums_tracks_now_and_those_excluded_everywhere() {
        let left = crates::LeftOut { tracks: vec!["tr-1".into()], albums: vec!["al-1".into(), "al-gone".into()] };
        let album_tracks: HashMap<String, Vec<String>> = [("al-1".to_string(), vec!["tr-2".to_string(), "tr-3".to_string()])].into();
        assert_eq!(left_tracks(&left, &album_tracks, &["tr-4".to_string()]), ["tr-1", "tr-2", "tr-3", "tr-4"]);
        assert!(left_tracks(&crates::LeftOut::default(), &HashMap::new(), &[]).is_empty());
    }

    #[test]
    fn a_list_takes_new_items_at_its_end_sorted_and_gives_items_back() {
        let before = vec!["b".to_string(), "a".to_string()];
        assert_eq!(edited(&before, ["d".to_string(), "a".to_string(), "c".to_string()], true), ["b", "a", "c", "d"], "a stays where it was");
        assert_eq!(edited(&before, ["b".to_string(), "z".to_string()], false), ["a"]);
    }
}

#[cfg(test)]
mod library_tests {
    use super::*;

    /// Music's files (folder 0) and an Incoming folder's (1), newest first; "../in/X/1.flac" is in a playlist.
    fn library(switched_off: &[i64]) -> CrateLibrary {
        let f = |line: &str, folder: i64, genre: &str| crates::Facts { rel: line.into(), folders: vec![folder], genres: vec![genre.into()], ..Default::default() };
        let files = vec![f("../in/X/1.flac", 1, "Dub"), f("A/1.flac", 0, "Dub"), f("../in/X/2.flac", 1, "Jazz"), f("B/1.flac", 0, "Rock")];
        let membership = HashMap::from([("../in/X/1.flac".to_string(), vec!["Road".to_string()])]);
        CrateLibrary {
            source: "cratarr".into(),
            files: Arc::new(files),
            switched_off: switched_off.iter().copied().collect(),
            membership,
            favourites: None,
            favourite_albums: None,
            alike: Default::default(),
            uses: None,
            by: crate::similarity::Weights::EQUAL,
            fits: None,
        }
    }

    #[test]
    fn a_track_is_on_while_any_copy_is() {
        let unlisted = &crates::permanent()[0].conditions;
        let held = |off: &[i64]| {
            let mut lib = library(off);
            let mut files = lib.files.to_vec();
            // Rock's track has a copy in own music, folder 2, beside the one in Music.
            files[3].folders = vec![0, 2];
            lib.files = Arc::new(files);
            lib
        };
        let lib = held(&[0]);
        assert_eq!(lib.select(unlisted), ["../in/X/2.flac", "B/1.flac"], "Music off: it stays, by its other copy");
        assert!(lib.on(3));
        assert_eq!(lib.files().count(), 3);
        let lib = held(&[2]);
        assert!(lib.on(3) && lib.select(unlisted).contains(&"B/1.flac".to_string()), "own music off: it stays, by Music's");
        let lib = held(&[0, 2]);
        assert!(!lib.on(3) && !lib.select(unlisted).contains(&"B/1.flac".to_string()), "with every copy off it goes");
        assert_eq!(lib.files().count(), 2);
        let (facts, album_of) = (held(&[]).files, HashMap::from([("tr-4".to_string(), "al-1".to_string())]));
        let mut files = facts.to_vec();
        files[3].track = Some(4);
        let words = |off: &[i64]| super::super::words::index(&files, &album_of, &off.iter().copied().collect()).len();
        assert_eq!((words(&[0]), words(&[0, 2])), (1, 0), "a word's count too");
    }

    #[test]
    fn a_crate_draws_from_every_library_switched_on() {
        let unlisted = &crates::permanent()[0].conditions;
        assert_eq!(library(&[]).select(unlisted), ["A/1.flac", "../in/X/2.flac", "B/1.flac"], "Incoming's tracks too, newest first");
        assert_eq!(library(&[1]).select(unlisted), ["A/1.flac", "B/1.flac"], "Incoming switched off");
        assert_eq!(library(&[0]).select(unlisted), ["../in/X/2.flac"], "Music switched off");
        // A crate of Music's tracks only is the same whether Incoming is on or not.
        let rock = [crates::Condition::new("genre", false, crates::Value::AnyOf { values: vec!["rock".into()] })];
        assert_eq!(library(&[]).select(&rock), library(&[1]).select(&rock));
        let off = library(&[1]);
        let one: Vec<[&crates::Facts; 1]> = off.files().map(|f| [f]).collect();
        let items: Vec<&[&crates::Facts]> = one.iter().map(|f| f.as_slice()).collect();
        assert_eq!(crates::list_axes(&items, &[], None, None, 0).iter().find(|a| a.key == "genre").unwrap().values, ["Dub", "Rock"], "the axes of what is on");
        assert_eq!((0..4).map(|i| off.on(i)).collect::<Vec<_>>(), [false, true, false, true], "the taste's places: Incoming off");
    }

    #[test]
    fn a_crate_says_why_it_left_things_out() {
        // Decision 0095: the libraries switched off, a toggle, nothing; the page makes them chips.
        let genre = |g: &str| crates::Condition::new("genre", false, crates::Value::AnyOf { values: vec![g.into()] });
        let without = ["cratarr/Incoming".to_string(), "other/Music".to_string()];
        let (lines, reasons, all, _) = library(&[1]).select_saying(&[genre("Jazz")], &[], &without, false);
        assert!(lines.is_empty());
        let off = crates::Reason::new(crates::Why::LibrariesOff, Some("cratarr/Incoming".into()), "1 library off", 1);
        assert_eq!((reasons, all), (vec![off], 1), "Incoming's one jazz track would come back; another server's library is not this one's");
        let mut hearted = library(&[]);
        hearted.favourites = Some(HashSet::from(["B/1.flac".to_string()]));
        let shown = [vec![genre("Dub")], crates::Favourites::Only.with(&[])].concat();
        let only = [crates::Cause { kind: crates::Why::Favourites, key: None, label: "Favourites only".into(), at: 1..2 }];
        let heart = crates::Reason::new(crates::Why::Favourites, None, "Favourites only", 2);
        assert_eq!(hearted.select_saying(&shown, &only, &[], false), (Vec::new(), vec![heart], 2, None), "no dub favourite");
        // Asked to explain: the verdicts with their causes; the page's words judged on rows, as its search does.
        let judged = hearted.select_saying(&shown, &only, &[], true).3.unwrap();
        assert_eq!(judged.causes.len(), 1);
        let row = |entry: &str, name: &str| (entry.to_string(), PlaylistRow { entry: entry.into(), name: Some(name.into()), ..Default::default() });
        let rows: HashMap<String, PlaylistRow> = [row("A/1.flac", "Kingston Town"), row("../in/X/2.flac", "Other")].into();
        let blocked = what_blocks(judged, &[], &["kingston".to_string()], &rows);
        assert_eq!(blocked.causes.last().map(|c| c.kind), Some(crates::Why::Search));
        assert!(words_find(&["kings"], &["Kingston Town", "UB40"]), "a word's start, as the page's search finds it");
        assert!(!words_find(&["polka"], &["Kingston Town", "UB40"]));
        assert_eq!(hearted.select_saying(&shown[..1], &[], &[], false).0, hearted.select(&shown[..1]), "what it keeps is `select`'s");
        assert_eq!(library(&[1]).select_saying(&[genre("Polka")], &[], &without, false), (Vec::new(), Vec::new(), 0, None), "truly empty: no reason");
    }
}

#[cfg(test)]
mod order_tests {
    use super::*;

    fn row(entry: &str, name: Option<&str>, bpm: Option<u32>, plays: Option<u32>) -> PlaylistRow {
        PlaylistRow { entry: entry.into(), file: format!("{entry}.flac"), name: name.map(str::to_string), bpm, play_count: plays, ..Default::default() }
    }

    fn entries(rows: &[PlaylistRow]) -> Vec<&str> {
        rows.iter().map(|r| r.entry.as_str()).collect()
    }

    #[test]
    fn a_crate_sorts_as_the_pages_list_does() {
        let rows = || vec![row("a", Some("Track 10"), Some(120), Some(3)), row("b", Some("track 9"), None, None), row("c", None, Some(90), Some(3)), row("d", Some("Été"), Some(120), None)];
        let sorted = |by, down| {
            let mut r = rows();
            sort_rows(&mut r, by, down);
            entries(&r).iter().map(|e| e.to_string()).collect::<Vec<_>>()
        };
        assert_eq!(sorted(CrateOrder::Own, false), ["a", "b", "c", "d"]);
        assert_eq!(sorted(CrateOrder::Own, true), ["d", "c", "b", "a"]);
        // Names: case and accents aside, numbers as numbers; no title: the file's name ("c.flac").
        assert_eq!(sorted(CrateOrder::Title, false), ["c", "d", "b", "a"]);
        assert_eq!(sorted(CrateOrder::Title, true), ["a", "b", "d", "c"]);
        // Numbers: none first; equals in the crate's own order, turned round too.
        assert_eq!(sorted(CrateOrder::Tempo, false), ["b", "c", "a", "d"]);
        assert_eq!(sorted(CrateOrder::Tempo, true), ["a", "d", "c", "b"]);
        // Plays: none is 0.
        assert_eq!(sorted(CrateOrder::Plays, true), ["a", "c", "b", "d"]);
    }

    #[test]
    fn names_compare_as_the_page_compares_them() {
        use std::cmp::Ordering::*;
        assert_eq!(natural("Track 9", "track 10"), Less);
        assert_eq!(natural("abc", "ABC"), Equal);
        assert_eq!(natural("Émile", "emile"), Equal);
        assert_eq!(natural("a02", "a2"), Equal);
        assert_eq!(natural("ab", "abc"), Less);
    }

    #[test]
    fn the_order_is_named_as_the_page_names_it() {
        let look: CrateLook = serde_json::from_value(serde_json::json!({ "order": "albumArtist", "down": true })).unwrap();
        assert_eq!((look.order, look.down), (CrateOrder::AlbumArtist, true));
        let look: CrateLook = serde_json::from_value(serde_json::json!({ "order": "lastPlayed" })).unwrap();
        assert_eq!((look.order, look.down), (CrateOrder::LastPlayed, false));
        assert_eq!(CrateLook::default().order, CrateOrder::Own, "as saved, for those asking no one's taste");
        let look: CrateLook = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(look.order, CrateOrder::BestMatch, "a crate opens on Best match");
        let look: CrateLook = serde_json::from_value(serde_json::json!({ "order": "bestMatch" })).unwrap();
        assert_eq!(look.order, CrateOrder::BestMatch);
    }

    #[test]
    fn best_match_puts_the_closest_to_the_taste_first_and_without_one_keeps_its_own_order() {
        let lines = || ["a", "b", "c", "d"].map(str::to_string).to_vec();
        let fit = |l: &str| match l {
            "a" => Some(0.2),
            "c" => Some(0.9),
            "d" => Some(0.5),
            _ => None,
        };
        assert_eq!(by_fit(lines(), fit, false), ["c", "d", "a", "b"], "nothing known of b: last");
        assert_eq!(by_fit(lines(), fit, true), ["b", "a", "d", "c"]);
        assert_eq!(by_fit(lines(), |_| None, false), ["a", "b", "c", "d"], "no taste: the list's own order");
    }
}

#[cfg(test)]
mod new_for_you_tests {
    use super::*;

    /// Albums newest first, by these album artists: `al-0` the newest.
    fn newest(artists: &[&str]) -> Vec<(String, String, bool)> {
        artists.iter().enumerate().map(|(i, a)| (format!("al-{i}"), a.to_string(), false)).collect()
    }

    /// New for you's picks of `albums`, read a page at a time as from the library, `heard` left out.
    fn walk(albums: &[(String, String, bool)], heard: &[&str]) -> Vec<String> {
        walk_without(albums, heard, &[])
    }

    /// As `walk`, these artists excluded everywhere.
    fn walk_without(albums: &[(String, String, bool)], heard: &[&str], out: &[&str]) -> Vec<String> {
        let out: HashSet<String> = out.iter().map(|a| crate::app::excluded_artists::key(a)).collect();
        new_picks(|offset| Ok(albums.iter().skip(offset).take(NEW_PAGE).cloned().collect()), |id| Ok(heard.contains(&id)), &out).unwrap()
    }

    #[test]
    fn an_album_by_an_artist_excluded_everywhere_is_left_out_and_counted_toward_no_one() {
        // Decision 0083: the album artist counts, by any credit; the next albums take their places.
        let artists = ["Air", "Bonobo", "Air & Beck", "Caribou"];
        assert_eq!(walk_without(&newest(&artists), &[], &["Air"]), ["al-1", "al-3"]);
        assert_eq!(walk_without(&newest(&artists), &[], &["beck"]), ["al-0", "al-1", "al-3"]);
        assert_eq!(walk_without(&newest(&artists), &[], &[]).len(), 4, "none excluded, none left out");
    }

    #[test]
    fn one_artists_ten_newest_give_three_of_theirs_then_the_next_artists() {
        let mut artists = vec!["The Avalanches"; 10];
        artists.extend(["Bonobo", "Caribou", "Bonobo"]);
        assert_eq!(walk(&newest(&artists), &[]), ["al-0", "al-1", "al-2", "al-10", "al-11", "al-12"]);
    }

    #[test]
    fn compilations_count_toward_no_artists_three() {
        // Five Various Artists compilations among the newest: all of them, each on its own.
        let mut albums = newest(&["Various Artists"; 5]);
        for a in &mut albums {
            a.2 = true;
        }
        albums.extend(newest(&["Bonobo"; 2]).into_iter().map(|(id, a, c)| (format!("{id}-b"), a, c)));
        assert_eq!(walk(&albums, &[]), ["al-0", "al-1", "al-2", "al-3", "al-4", "al-0-b", "al-1-b"]);
    }

    #[test]
    fn an_album_heard_five_percent_or_more_is_left_out() {
        // A 50-minute album: 2½ minutes heard is 5%; a second less is not.
        assert!(heard_share([(150, 1), (2850, 0)]));
        assert!(!heard_share([(149, 1), (2851, 0)]));
        assert!(heard_share([(75, 2), (2925, 0)]), "a track heard twice counts twice");
        assert!(!heard_share(Vec::<(u64, u32)>::new()) && !heard_share([(0, 3)]), "nothing to hear is not heard");
        // Left out, and not counted toward its artist's three.
        let artists = ["Air", "Air", "Air", "Air", "Air", "Moby"];
        assert_eq!(walk(&newest(&artists), &["al-0", "al-1"]), ["al-2", "al-3", "al-4", "al-5"]);
    }

    #[test]
    fn no_more_than_twelve() {
        let artists: Vec<String> = (0..30).map(|i| format!("Artist {i}")).collect();
        let picks = walk(&newest(&artists.iter().map(String::as_str).collect::<Vec<_>>()), &[]);
        assert_eq!(picks.len(), NEW_ALBUMS);
        assert_eq!(picks.last().map(String::as_str), Some("al-11"), "the twelve newest");
    }

    #[test]
    fn an_album_without_an_album_artist_counts_toward_no_one() {
        let artists = ["", "", "", "", " ", crate::serve::library::UNKNOWN_ARTIST, crate::serve::library::UNKNOWN_ARTIST, ""];
        assert_eq!(walk(&newest(&artists), &[]).len(), artists.len());
    }

    #[test]
    fn the_album_artist_is_matched_loosely() {
        let artists = ["The Avalanches", "the avalanches ", "THE AVALANCHES", "Avalanches", "Bonobo"];
        assert_eq!(walk(&newest(&artists), &[]), ["al-0", "al-1", "al-2", "al-4"]);
    }

    #[test]
    fn the_walk_reads_on_past_the_first_page_and_no_further_than_its_bound() {
        // The 90 newest by three artists: their nine, then three from the second page.
        let mut artists: Vec<String> = (0..90).map(|i| ["Air", "Bonobo", "Caribou"][i % 3].to_string()).collect();
        artists.extend((0..10).map(|i| format!("Artist {i}")));
        let picks = walk(&newest(&artists.iter().map(String::as_str).collect::<Vec<_>>()), &[]);
        assert_eq!(picks.len(), NEW_ALBUMS);
        assert_eq!(picks[9..].to_vec(), ["al-90", "al-91", "al-92"]);
        // One artist's newest further down than it reads: theirs alone.
        let mut artists = vec!["Air".to_string(); NEW_FURTHEST + 20];
        artists.extend((0..10).map(|i| format!("Artist {i}")));
        assert_eq!(walk(&newest(&artists.iter().map(String::as_str).collect::<Vec<_>>()), &[]), ["al-0", "al-1", "al-2"]);
    }
}
