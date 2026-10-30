//! Playlists (decision 0011): the Music folder's playlist files, edits and repairs, Play, and telling the servers
//! about changes.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;

use super::ToggleError;
use super::library::{Library, label_of};
use std::collections::{HashMap, HashSet};

use crate::crates::{self, CrateDef};
use crate::journal::{Change, EventId, Journal, TextChange, fingerprint};
use crate::playlists::{Edited, PlaylistView, Repair, Repairs, Store};
use crate::source::{self, PlaylistChange, PlaylistEvent};
use crate::sync_state::Kind;

/// The most tracks one play sends: Finamp holds at most 100 in its queue (seen 2026-10-12; user: "we go by 100").
pub const PLAY_MAX: usize = 100;

/// A playlist found by a spoken name, as its tracks on a server (decision 0015): the caller plays them itself.
#[derive(Debug, Serialize)]
pub struct ByName {
    /// The playlist it was taken for; `None` when nothing fits what was said.
    pub playlist: Option<String>,
    /// The server's ids of its tracks, in order or shuffled, at most the limit asked for.
    pub tracks: Vec<String>,
}

/// What crates are worked out on (see `PlaylistService::crate_library`).
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
/// seeds its songs.
async fn like_crate(served: Option<Arc<crate::serve::Serve>>, id: &str, item: &str, count: usize) -> Result<CrateDef, ToggleError> {
    let served = served.ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
    if item == crates::WEEK {
        let me = asking_user(Some(&*served)).ok_or_else(|| ToggleError::Refused("Sounds like your week is an account's own: sign in.".into()))?;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let since = now - crates::WEEK_SECS;
        // The history's week (decision 0064): how often and how lately each was heard; skipped ones left out too. Until
        // the log reaches back a week: the songs last played within it, alike.
        let week = tokio::task::spawn_blocking(move || -> Result<Vec<(String, f32)>> {
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
    let found = tokio::task::spawn_blocking(move || -> Result<Option<(String, Vec<String>)>> {
        if crate::serve::ids::track_of(&item).is_some() {
            return Ok(served.library.songs_by_id(std::slice::from_ref(&item))?.pop().map(|s| (s.title, vec![s.id])));
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

/// An account's taste as Insights' Profile has it, every source (`taste::profiles`): its favourites, playlists, plays
/// and skips, and what it excluded, on `lib`'s libraries switched on. Blocking.
fn taste_in(lib: &CrateLibrary, ud: &crate::userdata::UserData, user: &str, excluded: &[String]) -> Result<crate::similarity::Profile, ToggleError> {
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
    let sources = super::taste::Sources {
        favourites: starred.iter().filter_map(|s| place(s)).collect(),
        playlists: lib.membership().iter().filter_map(|(line, lists)| Some((alike.place_of_line(line).filter(|&i| lib.on(i))?, lists.len()))).collect(),
        plays: played.into_iter().collect(),
        excluded: excluded.iter().filter_map(|line| alike.place_of_line(line).filter(|&i| lib.on(i))).collect(),
        skipped: skips.into_iter().collect(),
    };
    Ok(super::taste::profiles(alike, &sources, &[]).0)
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

/// A ranked crate's fit as its Not for sheet shows it (`PlaylistService::crate_fit`).
#[derive(Debug, serde::Serialize)]
pub struct FitView {
    pub learned: Vec<FitTrait>,
    pub rules: Vec<crates::Condition>,
    pub turned_down: usize,
}

/// A ranked crate's fit in the list of them all (`PlaylistService::crate_fits`).
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

/// Home's moment (`PlaylistService::moment`): its name, its albums (Cratarr's own ids), and the weather it was made of.
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

/// Home's looks back (`PlaylistService::digging`).
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
    let made = Arc::new(crates::Alike::of(files, membership));
    *kept = Some((key.0, key.1, made.clone()));
    made
}

pub(super) struct CrateLibrary {
    pub(super) source: String,
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
        self.files.get(i).is_some_and(|f| !self.switched_off.contains(&f.folder))
    }

    /// Whether these conditions can be worked out: what they need of the favourites is known.
    fn knows(&self, conditions: &[crates::Condition]) -> bool {
        (!crates::needs_favourites(conditions) || self.favourites.is_some())
            && (!crates::needs_favourite_albums(conditions) || self.favourite_albums.is_some())
    }

    /// The files of the libraries switched on, newest first.
    pub(super) fn files(&self) -> impl Iterator<Item = &crates::Facts> {
        self.files.iter().filter(|f| !self.switched_off.contains(&f.folder))
    }

    /// What the axes test about the library as a whole.
    pub(super) fn world(&self) -> crates::World<'_> {
        crates::World {
            membership: &self.membership,
            favourites: self.favourites.as_ref(),
            favourite_albums: self.favourite_albums.as_ref(),
            uses: self.uses.as_ref(),
            now: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64),
        }
    }

    /// Which of one's own playlists hold each line.
    pub(super) fn membership(&self) -> &HashMap<String, Vec<String>> {
        &self.membership
    }

    /// What the viewer did with each line, when known.
    pub(super) fn uses(&self) -> Option<&HashMap<String, crates::Use>> {
        self.uses.as_ref()
    }

    /// The conditions with their Sounds like worked out (`crates::resolved`), once for all the files.
    pub(super) fn resolved(&self, conditions: &[crates::Condition]) -> Vec<crates::Condition> {
        if !conditions.iter().any(|c| c.value.ranks()) {
            return conditions.to_vec();
        }
        // The closest among what the crate may show: its libraries switched on, its other conditions held.
        let w = self.world();
        let among: Vec<&crates::Facts> = self.files().filter(|f| crates::holds_besides_like(conditions, f, &w)).collect();
        crates::resolved_in(conditions, self.alike(), &among, &self.by, &self.membership)
    }

    /// The crate's lines, newest first; a Sounds like crate's closest first (`crates::closest_first`).
    pub(super) fn select(&self, conditions: &[crates::Condition]) -> Vec<String> {
        let resolved = self.resolved(conditions);
        let mut lines: Vec<String> = crates::select(self.files(), &resolved, &self.world()).into_iter().map(str::to_string).collect();
        crates::closest_first(&mut lines, &resolved);
        lines
    }
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
    /// The order its list shows (decision 0060), applied before its rows are paged, and to what Play plays.
    #[serde(default)]
    pub order: CrateOrder,
    #[serde(default)]
    pub down: bool,
}

/// The orders a crate's list takes (decision 0060), as the page names them: its own (newest first) or by a field;
/// `down` turns it round.
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
}

/// A name as the page compares names (`localeCompare` at base sensitivity, numeric): case and the common accents
/// aside (`name_key`), each run of digits as its number.
#[cfg(test)]
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    natural_keys(&name_key(a), &name_key(b))
}

/// A name in lower case, the common accents taken off: what `natural_keys` compares.
fn name_key(s: &str) -> Vec<char> {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à'..='å' => 'a',
            'ç' => 'c',
            'è'..='ë' => 'e',
            'ì'..='ï' => 'i',
            'ñ' => 'n',
            'ò'..='ö' | 'ø' => 'o',
            'ù'..='ü' => 'u',
            'ý' | 'ÿ' => 'y',
            c => c,
        })
        .collect()
}

fn natural_keys(x: &[char], y: &[char]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut i, mut j) = (0, 0);
    while i < x.len() && j < y.len() {
        if x[i].is_ascii_digit() && y[j].is_ascii_digit() {
            let (si, sj) = (i, j);
            while i < x.len() && x[i].is_ascii_digit() {
                i += 1;
            }
            while j < y.len() && y[j].is_ascii_digit() {
                j += 1;
            }
            let digits = |d: &[char]| -> String { d.iter().skip_while(|c| **c == '0').collect() };
            let (n, m) = (digits(&x[si..i]), digits(&y[sj..j]));
            match n.len().cmp(&m.len()).then_with(|| n.cmp(&m)) {
                Ordering::Equal => continue,
                o => return o,
            }
        }
        match x[i].cmp(&y[j]) {
            Ordering::Equal => {
                i += 1;
                j += 1;
            }
            o => return o,
        }
    }
    (x.len() - i).cmp(&(y.len() - j))
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

/// A playlist's facts for the Playlists page's table (decision 0042): how long, its top genres, how much of it the
/// owner never played and when they last played any of it, and when its file last changed.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct PlaylistStats {
    pub seconds: u64,
    /// Its three most common genres (its tracks' albums'), most common first.
    pub genres: Vec<String>,
    /// Never played by the owner (Cratarr's own plays); `None` while there is no owner to ask about.
    pub unplayed: Option<usize>,
    /// The latest play of any of its tracks (epoch seconds).
    pub last_played: Option<i64>,
    /// When its file last changed (epoch seconds).
    pub changed: i64,
}

/// A playlist's facts from its lines (library-relative), the Music folder's songs and the owner's plays.
fn playlist_stats(
    lines: &[String],
    songs: &HashMap<String, crate::serve::library::RelSong>,
    plays: Option<&HashMap<String, (u32, Option<i64>)>>,
    changed: i64,
) -> PlaylistStats {
    let known: Vec<&crate::serve::library::RelSong> = lines.iter().filter_map(|l| songs.get(l)).collect();
    PlaylistStats {
        seconds: known.iter().filter_map(|s| s.duration_ms).filter(|d| *d > 0).map(|d| d as u64).sum::<u64>() / 1000,
        genres: crates::top_genres(known.iter().map(|s| s.genres.as_slice()), 3),
        unplayed: plays.map(|p| known.iter().filter(|s| !p.contains_key(&s.song)).count()),
        last_played: plays.and_then(|p| known.iter().filter_map(|s| p.get(&s.song).and_then(|(_, at)| *at)).max()),
        changed,
    }
}

/// The playlists' facts as last worked out, and what they were worked out from (the playlists' files, the library's
/// build, the owner's plays): the table asks again whenever the list changes, and the answer is kept until one of
/// those does.
type StatsKept = std::sync::Mutex<Option<(String, std::collections::BTreeMap<String, PlaylistStats>)>>;

/// An album's cover, as `/api/cover/{source}/{album_id}` serves it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CoverRef {
    pub source: String,
    pub album_id: String,
}

/// A crate's picture's file name: its id where that is a plain name, else the id in hex.
fn image_file(id: &str) -> String {
    if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        format!("{id}.jpg")
    } else {
        format!("x{}.jpg", id.bytes().map(|b| format!("{b:02x}")).collect::<String>())
    }
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

/// One line of a playlist as its view shows it (decision 0023). `entry` is the line as the `.m3u` has it (library-
/// relative), `file` its file name; the rest is what the catalogue knows, `None` where it does not.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct PlaylistRow {
    pub entry: String,
    pub file: String,
    /// The server's track id, when the file is matched to one.
    pub id: Option<String>,
    pub name: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_secs: Option<f64>,
    /// The panel's orders (decision 0025's 2026-10-16 amendment): from the file's tags,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    /// its tempo (the BPM tag, else the analysis', decision 0035),
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bpm: Option<u32>,
    /// the user's plays of the track and when it was last played,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_played: Option<i64>,
    /// and when the entry came into the playlist (none for one held before Cratarr kept that; a crate's row: when its
    /// track came into the library).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<i64>,
    /// Its album's id where it is known: an artist's rows, and a playlist's or a crate's on Cratarr's own source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_id: Option<String>,
    /// How many tracks that album has (the Records view's "3 of 10"), where its id is known on Cratarr's own source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_tracks: Option<u32>,
    /// No such file in the Music folder.
    pub missing: bool,
    /// The library holds another recording of its work (0.10, Other versions): an album's rows on Cratarr's own
    /// library; absent where not, or not said.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_versions: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct PlayResult {
    /// "Finamp on Lenovo Tab".
    pub player: String,
    /// Tracks sent (at most 500; to a Jellyfin player, PLAY_MAX).
    pub sent: usize,
    /// Playable tracks in the playlist.
    pub total: usize,
    /// Lines whose track is missing or not known to the server yet; left out.
    pub unknown: usize,
}

/// Whether `entries` only reorders or leaves out lines of `expected` (each line at most as often as it was there).
fn is_rearrangement(expected: &[String], entries: &[String]) -> bool {
    let mut left: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for e in expected {
        *left.entry(e.as_str()).or_default() += 1;
    }
    entries.iter().all(|e| match left.get_mut(e.as_str()) {
        Some(n) if *n > 0 => {
            *n -= 1;
            true
        }
        _ => false,
    })
}

/// The safe repairs of the playlists `within` (all of them without), matched as the store names playlists: ignoring case.
fn safe_within(safe: Vec<Repair>, within: Option<&[String]>) -> Vec<Repair> {
    match within {
        None => safe,
        Some(w) => safe.into_iter().filter(|r| w.iter().any(|p| p.eq_ignore_ascii_case(&r.playlist))).collect(),
    }
}

/// What an export needs of a file (its names, its format for the estimates) from the catalogue's row of it.
fn export_facts(row: crate::catalogue::FileRow, format: Option<String>, kbps: Option<u32>) -> crate::export::Facts {
    crate::export::Facts {
        artist: row.artist,
        title: row.title,
        album: row.album,
        album_artist: row.album_artist,
        format,
        kbps,
        duration_ms: row.duration_ms.filter(|d| *d > 0).map(|d| d as u64),
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
        assert_eq!(image_file("c-12ab"), "c-12ab.jpg");
        assert_eq!(image_file("../x"), "x2e2e2f78.jpg");
    }

    #[test]
    fn a_playlists_facts_count_its_lines() {
        let song = |id: &str, ms: i64, genres: &[&str]| RelSong {
            song: id.into(),
            album: "al".into(),
            duration_ms: Some(ms),
            created: None,
            genres: genres.iter().map(|g| g.to_string()).collect(),
        };
        let songs: HashMap<String, RelSong> = [
            ("A/1.flac".to_string(), song("tr-1", 200_000, &["Dub", "Reggae"])),
            ("A/2.flac".to_string(), song("tr-2", 100_500, &["dub"])),
            ("B/1.flac".to_string(), song("tr-3", 60_000, &["Jazz"])),
        ]
        .into_iter()
        .collect();
        let plays: HashMap<String, (u32, Option<i64>)> = [("tr-1".to_string(), (3, Some(500))), ("tr-3".to_string(), (1, Some(900)))].into_iter().collect();
        let lines: Vec<String> = ["A/1.flac", "A/2.flac", "B/1.flac", "gone.flac"].iter().map(|s| s.to_string()).collect();
        let s = playlist_stats(&lines, &songs, Some(&plays), 42);
        assert_eq!((s.seconds, s.unplayed, s.last_played, s.changed), (360, Some(1), Some(900), 42));
        assert_eq!(s.genres, vec!["Dub", "Jazz", "Reggae"]);
        let none = playlist_stats(&lines, &songs, None, 42);
        assert_eq!((none.unplayed, none.last_played), (None, None), "no owner: not known");
    }
}

#[cfg(test)]
mod undo_tests {
    use super::*;

    /// The service over an empty Music folder of its own, no servers, the journal and the crates in memory.
    async fn service() -> (PathBuf, PlaylistService) {
        service_with(None).await
    }

    /// `service`, with Cratarr's own library as `main` hands it.
    async fn service_with(served: Option<Arc<crate::serve::Serve>>) -> (PathBuf, PlaylistService) {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("cratarr-undo-{}-{n}", std::process::id()));
        // Its playlist folder from the start (decision 0014), so the store never moves meanwhile.
        std::fs::create_dir_all(d.join(crate::playlists::PLAYLIST_DIR)).unwrap();
        let lib = Library::new(None, Some(d.to_string_lossy().into_owned()), Vec::new(), Default::default(), None).await.unwrap();
        let journal = Arc::new(Journal::open(None).unwrap());
        let state = crate::sync_state::SyncState::open(None).unwrap();
        let sync = Arc::new(super::super::sync::SyncService::new(lib.clone(), state, journal.clone()));
        (d.join(crate::playlists::PLAYLIST_DIR), PlaylistService::new(lib, sync, journal, served))
    }

    fn refused(r: Result<Option<String>, ToggleError>) -> String {
        match r {
            Err(ToggleError::Refused(m)) => m,
            other => panic!("not refused: {other:?}"),
        }
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

    /// A service reads the library it was made with, not a global one (the audit, 0.10): two in one process, each
    /// with its own, do not see each other's.
    #[tokio::test]
    async fn a_service_reads_its_own_library_and_not_another() {
        let open = || crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        let (one, other) = (open(), open());
        one.userdata.set_playlist_shared("Chill", true).unwrap();
        let (d1, s1) = service_with(Some(one)).await;
        let (d2, s2) = service_with(Some(other)).await;
        let (d3, none) = service().await;
        assert!(s1.shared_keys().contains("Chill"), "shared in its own library");
        assert!(s2.shared_keys().is_empty(), "the other library shares nothing");
        assert!(none.shared_keys().is_empty(), "nor does a service without one");
        for d in [d1, d2, d3] {
            std::fs::remove_dir_all(&d).ok();
        }
    }

    #[tokio::test]
    async fn each_account_has_its_own_playlists_in_a_folder_of_its_own() {
        let (d, s) = service().await;
        let ann = s.lib.store_for(Some("ann")).unwrap();
        assert!(ann.view(None).await.unwrap().is_empty(), "none until her first");
        ann.create("Chill").await.unwrap();
        ann.replace_entries_if("Chill", &[], &["A/1.mp3".to_string()]).await.unwrap();
        assert_eq!(std::fs::read_to_string(d.join("ann/Chill.m3u")).unwrap(), "../../A/1.mp3
", "Music-relative, from her folder");
        // The first administrator's are as they were: hers is not among them, and one of the same name is another.
        s.create("Chill").await.unwrap();
        let mine: Vec<(String, bool)> = s.view(None).await.unwrap().into_iter().map(|p| (p.id, p.mine)).collect();
        assert_eq!(mine, [("Chill".to_string(), true)]);
        assert!(s.entries("Chill").await.unwrap().is_empty());
        // Keys tell them apart in Cratarr's own records; Cratarr's own work covers every account's.
        let top = s.lib.store_for(None).unwrap();
        assert_eq!((top.key("Chill"), ann.key("Chill")), ("Chill".to_string(), "ann/Chill".to_string()));
        assert_eq!((ann.name_of_key("ann/Chill"), ann.name_of_key("Chill"), top.name_of_key("ann/Chill")), (Some("Chill"), None, None));
        let all: Vec<String> = s.lib.all_stores().iter().map(|st| st.key("Chill")).collect();
        assert_eq!(all, ["Chill", "ann/Chill"]);
        // Another's, not shared, is as if there were none; and it is never another's to change.
        assert!(matches!(s.reach("ann/Chill"), Err(ToggleError::UnknownPlaylist)));
        assert!(matches!(s.set_shared("ann/Chill", true), Err(ToggleError::UnknownPlaylist)));
        std::fs::remove_dir_all(d.parent().unwrap()).ok();
    }

    #[tokio::test]
    async fn an_undo_is_undone_by_its_redo_and_happens_only_once() {
        let (d, s) = service().await;
        let lines = |v: &[&str]| v.iter().map(|l| l.to_string()).collect::<Vec<_>>();
        let read = |n: &str| std::fs::read_to_string(d.join(format!("{n}.m3u"))).ok();
        let made = s.create_with("Chill", &lines(&["A/1.mp3", "A/2.mp3"])).await.unwrap().unwrap();
        let edited = s.edit("Chill", &lines(&["A/1.mp3", "A/2.mp3"]), &lines(&["A/2.mp3"])).await.unwrap().unwrap();
        assert_eq!(read("Chill").as_deref(), Some("../A/2.mp3\n"));

        let redo = s.undo(&edited).await.unwrap().unwrap();
        assert_eq!(read("Chill").as_deref(), Some("../A/1.mp3\n../A/2.mp3\n"), "the edit undone");
        assert_eq!(refused(s.undo(&edited).await), "That was undone already.");
        let again = s.undo(&redo).await.unwrap().unwrap();
        assert_eq!(read("Chill").as_deref(), Some("../A/2.mp3\n"), "redone, as a change of its own");
        s.undo(&again).await.unwrap();
        assert_eq!(read("Chill").as_deref(), Some("../A/1.mp3\n../A/2.mp3\n"), "and the redo undone");

        let unmade = s.undo(&made).await.unwrap().unwrap();
        assert_eq!(read("Chill"), None, "the new playlist set aside");
        s.undo(&unmade).await.unwrap();
        assert_eq!(read("Chill").as_deref(), Some("../A/1.mp3\n../A/2.mp3\n"), "and back");
        assert_eq!(refused(s.undo("c999").await), "That can no longer be undone.");
        assert_eq!(refused(s.undo("nonsense").await), "That can no longer be undone.");
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn nothing_is_undone_when_it_changed_since() {
        let (d, s) = service().await;
        let one = vec!["A/1.mp3".to_string()];
        let made = s.create_with("Chill", &one).await.unwrap().unwrap();
        let cleared = s.edit("Chill", &one, &[]).await.unwrap().unwrap();
        // Another device (or a sync) changes it meanwhile.
        std::fs::write(d.join("Chill.m3u"), "../B/9.mp3\n").unwrap();
        assert_eq!(refused(s.undo(&cleared).await), "It changed since; nothing was undone.");
        assert_eq!(refused(s.undo(&made).await), "It changed since; nothing was undone.");
        assert_eq!(std::fs::read_to_string(d.join("Chill.m3u")).unwrap(), "../B/9.mp3\n", "left alone");
        std::fs::write(d.join("Chill.m3u"), "").unwrap();
        s.undo(&cleared).await.unwrap();
        assert_eq!(std::fs::read_to_string(d.join("Chill.m3u")).unwrap(), "../A/1.mp3\n", "a refused undo can be tried again");

        let renamed = s.rename("Chill", "Calm").await.unwrap().unwrap();
        s.create("Chill").await.unwrap();
        assert_eq!(refused(s.undo(&renamed).await), "It changed since; nothing was undone.", "the old name is taken");
        std::fs::remove_file(d.join("Chill.m3u")).unwrap();
        s.undo(&renamed).await.unwrap();
        assert!(d.join("Chill.m3u").exists() && !d.join("Calm.m3u").exists());

        let deleted = s.delete("Chill").await.unwrap().unwrap();
        s.create("Chill").await.unwrap();
        assert_eq!(refused(s.undo(&deleted).await), "It changed since; nothing was undone.", "a new one has the name");
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
        assert!(s.sync.spoken().remove("Chill").unwrap_or_default().is_empty());
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

    #[tokio::test]
    async fn an_artist_is_excluded_everywhere_once_per_account_and_taken_back() {
        let served = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        served.users.set("me", "pw").unwrap();
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
mod edit_tests {
    use super::{Repair, is_rearrangement, safe_within};

    #[test]
    fn fixing_all_in_the_repair_tab_touches_only_the_selected_playlists() {
        let r = |p: &str| Repair { playlist: p.into(), from: format!("{p}.mp3"), to: format!("{p}.flac") };
        let all = vec![r("Chill"), r("Jazz"), r("chill")];
        assert_eq!(safe_within(all.clone(), None).len(), 3, "no selection: every safe repair");
        let only = safe_within(all, Some(&["CHILL".into()]));
        assert_eq!(only.iter().map(|x| x.playlist.as_str()).collect::<Vec<_>>(), ["Chill", "chill"]);
        assert!(safe_within(vec![r("Jazz")], Some(&[])).is_empty(), "an empty selection fixes nothing");
    }

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn an_edit_may_only_reorder_or_leave_out_its_own_lines() {
        let was = v(&["a", "b", "b", "c"]);
        assert!(is_rearrangement(&was, &v(&["c", "b", "a", "b"])), "reordered");
        assert!(is_rearrangement(&was, &v(&["b", "c"])), "some left out");
        assert!(is_rearrangement(&was, &[]), "all left out");
        assert!(!is_rearrangement(&was, &v(&["a", "d"])), "a line it did not have");
        assert!(!is_rearrangement(&was, &v(&["a", "a"])), "a line more often than it was there");
    }
}

pub struct PlaylistService {
    lib: Arc<Library>,
    /// Keeps the user's own server playlists in step (decision 0012).
    sync: Arc<super::sync::SyncService>,
    /// One's own crates (decision 0024).
    crate_stores: CrateStores,
    /// Where every change is kept for its undo (decision 0026).
    journal: Arc<Journal>,
    /// The favourite tracks' lines for the crates (see `favourite_lines`).
    favourites: Arc<FavouritesKept>,
    /// The playlists' table's facts (see `playlist_stats`).
    stats: StatsKept,
    /// The last crate put in an order other than its own (see `crate_in_order`).
    sorted: SortedKept,
    /// Cratarr's own library (decision 0030): its plays, stars and shared playlists, and who asks. `main` hands it; a
    /// test may not.
    served: Option<Arc<crate::serve::Serve>>,
}

/// A crate's lines in an order (decision 0060), by what they were worked out from, and when: its pages are asked for
/// one after the other, and each would otherwise read every row of it again.
type SortedKept = std::sync::Mutex<Option<(String, std::time::Instant, Arc<Vec<String>>)>>;

/// How long a sorted crate is kept: the plays it may be ordered by change as it plays.
const SORTED_KEEP: std::time::Duration = std::time::Duration::from_secs(30);

/// The account the running request is for, whose favourites the crates show.
fn asking_user(served: Option<&crate::serve::Serve>) -> Option<String> {
    served.and_then(|s| s.current_user())
}

/// The lines whose album has the asking account's heart on (decision 0024, amended 2026-10-19): the Library's album
/// favourites, the viewer's own whoever's crate it is, and Cratarr's own library telling each line's album, whatever
/// server the crate is shown for. `None` when they cannot be read (no library, no account).
async fn favourite_album_lines(served: Option<Arc<crate::serve::Serve>>, cat: Arc<crate::catalogue::Catalogue>) -> Option<HashSet<String>> {
    let serve = served?;
    let me = serve.current_user()?;
    tokio::task::spawn_blocking(move || {
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

/// The lines of albums' tracks as the library has them now, by album: a crate's albums left out by hand (decision 0024,
/// amended 2026-10-19), a rule, so a track the album gets later is out too.
async fn album_lines(served: Option<Arc<crate::serve::Serve>>, cat: Arc<crate::catalogue::Catalogue>, albums: HashSet<String>) -> HashMap<String, Vec<String>> {
    if albums.is_empty() {
        return HashMap::new();
    }
    let Some(serve) = served else { return HashMap::new() };
    tokio::task::spawn_blocking(move || {
        let mut lines: HashMap<String, Vec<String>> = HashMap::new();
        for (line, s) in songs_by_line(&serve, &cat) {
            if albums.contains(&s.album) {
                lines.entry(s.album).or_default().push(line);
            }
        }
        lines
    })
    .await
    .unwrap_or_default()
}

/// Cratarr's own library's songs by their playlist lines, in every folder in use: Music's and the Incoming ones' (a
/// crate draws from both; an Incoming song's line climbs out of Music, `Catalogue::line_of`). Blocking.
fn songs_by_line(serve: &crate::serve::Serve, cat: &crate::catalogue::Catalogue) -> HashMap<String, crate::serve::library::RelSong> {
    let mut out = HashMap::new();
    for folder in 0..cat.folders().len() as i64 {
        for (rel, song) in serve.library.songs_by_rel(folder).unwrap_or_default() {
            if let Some(line) = cat.line_of(folder, &rel) {
                out.insert(line, song);
            }
        }
    }
    out
}

/// Whether a playlist line's file is there: in Music, or, for a line that climbs out of it, in the Incoming folder it
/// names (`Catalogue::path_of_line`). Blocking.
pub(super) fn line_exists(cat: &crate::catalogue::Catalogue, music: Option<&std::path::Path>, line: &str) -> bool {
    if line.starts_with("../") {
        return cat.path_of_line(line).is_some_and(|p| p.is_file());
    }
    music.is_some_and(|d| crate::playlists::entry_exists(d, line))
}

/// A crate's lines to leave out: its own tracks left out, its albums' tracks now, and the tracks excluded everywhere.
fn left_lines(left: &crates::LeftOut, album_tracks: &HashMap<String, Vec<String>>, excluded: &[String]) -> Vec<String> {
    if left.is_empty() {
        return excluded.to_vec();
    }
    let of_albums = left.albums.iter().filter_map(|a| album_tracks.get(a)).flatten();
    left.lines.iter().chain(of_albums).chain(excluded).cloned().collect()
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
    Ok(tokio::task::spawn_blocking(move || serve.library.albums_by_id(&ids)).await.map_err(|e| anyhow::anyhow!("{e}"))??)
}

/// An album left out of a crate, for its editor's list: its title and artist none once it is gone from the library.
#[derive(Debug, Clone, Serialize)]
pub struct LeftOutAlbum {
    pub id: String,
    pub title: Option<String>,
    pub artist: Option<String>,
}

/// The account whose playlists and crates the running request's are (decision 0051): another account's, or `None` for
/// the first administrator's (theirs are the ones from before accounts), as for Cratarr's own work.
fn asking_account(served: Option<&crate::serve::Serve>) -> Option<String> {
    let s = served?;
    let me = s.current_user()?;
    (Some(&me) != s.users.owner().as_ref()).then_some(me)
}

/// Each account's crates (decision 0051: one's own, not shared): the first administrator's in `crates.json` as
/// before, another's in `users/<account>/crates.json`. One store each, as each keeps its crates in memory.
struct CrateStores {
    data_dir: Option<String>,
    first: Arc<crates::CrateStore>,
    others: std::sync::Mutex<HashMap<String, Arc<crates::CrateStore>>>,
}

impl CrateStores {
    fn new(data_dir: Option<&str>) -> Self {
        Self { data_dir: data_dir.map(str::to_string), first: Arc::new(crates::CrateStore::new(data_dir)), others: Default::default() }
    }

    fn of(&self, account: Option<&str>) -> Result<Arc<crates::CrateStore>, ToggleError> {
        let Some(account) = account else { return Ok(self.first.clone()) };
        if !crate::playlists::account_folder_ok(account) {
            return Err(ToggleError::Refused(format!("{account} cannot name a folder for its crates.")));
        }
        let mut others = self.others.lock().unwrap_or_else(|p| p.into_inner());
        let store = others.entry(account.to_string()).or_insert_with(|| Arc::new(crates::CrateStore::of_account(self.data_dir.as_deref(), account)));
        Ok(store.clone())
    }
}

/// The account a playlist owner's name stands for (`None`: the first administrator).
fn account_of(served: Option<&crate::serve::Serve>, owner: &str) -> Option<String> {
    let first = served.and_then(|s| s.users.owner());
    (Some(owner) != first.as_deref()).then(|| owner.to_string())
}

/// The name shown as the owner of an account's playlists.
fn owner_name(served: Option<&crate::serve::Serve>, account: Option<&str>) -> String {
    account.map(str::to_string).or_else(|| served.and_then(|s| s.users.owner())).unwrap_or_default()
}

/// A playlist as the asking account names it (decision 0051): its own by name, another's shared one as `owner/name`.
pub struct Reached {
    pub store: Arc<Store>,
    /// Its name among its owner's playlists.
    pub name: String,
    /// Whose it is (shown), whether it is shared, and whether it is the asking account's own to change.
    pub owner: String,
    pub shared: bool,
    pub mine: bool,
}

impl Reached {
    /// Its key in Cratarr's own records (`Store::key`).
    pub fn key(&self) -> String {
        self.store.key(&self.name)
    }
}

/// How long the favourites heard are answered before they are asked for again (in the background).
const FAVOURITES_KEEP: std::time::Duration = std::time::Duration::from_secs(60);

/// The media server's favourites as the crates last heard them, and the one asking for them now.
#[derive(Default)]
struct FavouritesKept {
    last: std::sync::Mutex<Option<Kept>>,
    asking: Arc<tokio::sync::Mutex<()>>,
    /// Where what was heard is kept between runs (`favourites.json` in the data folder), so the first Crates after a
    /// start answers at once instead of waiting for the server (a minute or more for thousands of favourites).
    file: Option<std::path::PathBuf>,
    /// Cratarr's own library, whose account asks (`asking_user`).
    served: Option<Arc<crate::serve::Serve>>,
}

/// `favourites.json`: the last answer, by server and whose favourites they are.
#[derive(Serialize, serde::Deserialize)]
struct KeptFile {
    source: String,
    #[serde(default)]
    user: Option<String>,
    lines: Vec<String>,
}

#[derive(Clone)]
struct Kept {
    at: std::time::Instant,
    source: String,
    /// Whose favourites (decision 0051): the account that asked.
    user: Option<String>,
    /// The last of Cratarr's own changes (`crates::favourite_changed`) the answer holds.
    upto: u64,
    /// `None`: the server did not answer.
    lines: Option<Arc<HashSet<String>>>,
}

impl FavouritesKept {
    /// What the last run heard, if kept: due to be asked again at once (in the background).
    fn load(file: Option<std::path::PathBuf>, served: Option<Arc<crate::serve::Serve>>) -> Self {
        let last = file.as_ref().and_then(|f| std::fs::read(f).ok()).and_then(|b| serde_json::from_slice::<KeptFile>(&b).ok()).map(|k| {
            let now = std::time::Instant::now();
            Kept { at: now.checked_sub(FAVOURITES_KEEP).unwrap_or(now), source: k.source, user: k.user, upto: 0, lines: Some(Arc::new(k.lines.into_iter().collect())) }
        });
        Self { last: std::sync::Mutex::new(last), asking: Arc::default(), file, served }
    }

    /// Keep an answer for the next run. A failure costs only the wait after the next start.
    async fn save(&self, source: &str, user: Option<String>, lines: &Arc<HashSet<String>>) {
        let Some(file) = self.file.clone() else { return };
        let body = KeptFile { source: source.to_string(), user, lines: lines.iter().cloned().collect() };
        let written = tokio::task::spawn_blocking(move || -> Result<()> {
            let tmp = file.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec(&body)?)?;
            std::fs::rename(&tmp, &file)?;
            Ok(())
        });
        if let Ok(Err(e)) = written.await {
            tracing::warn!("keeping the favourites for the next start: {e:#}");
        }
    }

    fn kept(&self, source: &str) -> Option<Kept> {
        let user = asking_user(self.served.as_deref());
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone().filter(|k| k.source == source && k.user == user)
    }

    /// Ask the server (the caller holds `asking`). When it does not answer, what was heard before is kept, for another
    /// minute before asking again.
    async fn ask(&self, c: Arc<crate::catalogue::Catalogue>, source: &str, from: &Arc<dyn source::Source>) -> Kept {
        let (upto, user) = (crates::favourite_changes_made(), asking_user(self.served.as_deref()));
        let lines = match from.favorite_tracks().await {
            Ok(ids) => {
                let s = source.to_string();
                match tokio::task::spawn_blocking(move || c.lines_of(&s, &ids)).await {
                    Ok(Ok(lines)) => Some(Arc::new(lines)),
                    Ok(Err(e)) => {
                        tracing::warn!("favourites for the crates: {e:#}");
                        None
                    }
                    Err(e) => {
                        tracing::warn!("favourites for the crates: {e}");
                        None
                    }
                }
            }
            Err(e) => {
                tracing::warn!("favourites for the crates: {e:#}");
                None
            }
        };
        if let Some(l) = &lines {
            self.save(source, user.clone(), l).await;
        }
        let now = std::time::Instant::now();
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let kept = match (lines, last.clone().filter(|k| k.source == source && k.user == user && k.lines.is_some())) {
            (Some(lines), _) => {
                crates::favourite_changes_held(source, upto);
                Kept { at: now, source: source.to_string(), user, upto, lines: Some(lines) }
            }
            (None, Some(before)) => Kept { at: now, ..before },
            (None, None) => Kept { at: now, source: source.to_string(), user, upto, lines: None },
        };
        *last = Some(kept.clone());
        kept
    }

    /// The kept lines with Cratarr's own changes made since applied.
    async fn with_changes(kept: Kept, c: Arc<crate::catalogue::Catalogue>) -> Option<HashSet<String>> {
        let lines = kept.lines?;
        let changes = crates::favourite_changes_since(&kept.source, kept.upto);
        if changes.is_empty() {
            return Some((*lines).clone());
        }
        let s = kept.source;
        let lines = tokio::task::spawn_blocking(move || {
            let mut lines = (*lines).clone();
            for (track, on) in changes {
                let of = c.lines_of(&s, std::slice::from_ref(&track))?;
                if on {
                    lines.extend(of);
                } else {
                    lines.retain(|l| !of.contains(l));
                }
            }
            anyhow::Ok(lines)
        });
        lines.await.ok()?.ok()
    }
}

impl PlaylistService {
    pub(super) fn new(lib: Arc<Library>, sync: Arc<super::sync::SyncService>, journal: Arc<Journal>, served: Option<Arc<crate::serve::Serve>>) -> Self {
        let crate_stores = CrateStores::new(lib.data_dir());
        let favourites = Arc::new(FavouritesKept::load(lib.data_dir().map(|d| std::path::PathBuf::from(d).join("favourites.json")), served.clone()));
        Self { lib, sync, crate_stores, journal, favourites, stats: StatsKept::default(), sorted: SortedKept::default(), served }
    }

    /// Cratarr's own library, as the services built on this one (Play On, Playing, Insights) read it.
    pub(crate) fn served(&self) -> Option<Arc<crate::serve::Serve>> {
        self.served.clone()
    }

    /// The account whose playlists and crates the running request's are (`asking_account`).
    pub(crate) fn asking_account(&self) -> Option<String> {
        asking_account(self.served.as_deref())
    }

    /// Every playlist's facts for the Playlists page's table (decision 0042), by id. Worked out again only when a
    /// playlist's file, the library's build or the owner's plays changed since.
    pub async fn playlist_stats(&self) -> Result<std::collections::BTreeMap<String, PlaylistStats>, ToggleError> {
        let lists = self.visible_lines().await?;
        let Some(serve) = self.served.clone() else { return Ok(Default::default()) };
        let stamps: String = lists.iter().map(|(id, changed, lines)| format!("{id}:{changed}:{};", lines.len())).collect();
        let s = serve.clone();
        // Read here: the blocking task does not carry the request's account.
        let me = serve.current_user();
        let key_and_owner = tokio::task::spawn_blocking(move || {
            let plays = me.as_ref().and_then(|me| s.userdata.plays_print(me).ok());
            (format!("{stamps}|{}|{plays:?}", s.built_at()), me)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        let (key, me) = key_and_owner;
        if let Some((k, kept)) = self.stats.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
            && *k == key
        {
            return Ok(kept.clone());
        }
        let worked = tokio::task::spawn_blocking(move || {
            let songs = serve.library.songs_by_rel(0).unwrap_or_default();
            let plays = me.and_then(|me| serve.userdata.song_plays(&me).ok());
            lists.into_iter().map(|(id, changed, lines)| (id, playlist_stats(&lines, &songs, plays.as_ref(), changed))).collect::<std::collections::BTreeMap<_, _>>()
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        let answer = worked.clone();
        *self.stats.lock().unwrap_or_else(|p| p.into_inner()) = Some((key, worked));
        Ok(answer)
    }

    /// Ask the media server for the favourites once, at start, when a crate shows by them: the first visit to Crates
    /// then need not wait the minute that may take.
    pub async fn warm_favourites(&self) {
        let first = self.crate_stores.first.clone();
        let Ok(set) = first.all_favourites().await else { return };
        let needed = crates::permanent().into_iter().chain(first.list().await.unwrap_or_default()).any(|c| {
            crates::needs_favourites(&c.conditions) || set.get(&c.id).copied().unwrap_or(crates::Favourites::None) != crates::Favourites::All
        });
        if let (true, Ok((sid, source))) = (needed, self.source_of(None)) {
            self.favourite_lines(&sid, &source).await;
        }
    }

    /// Keep a change in the journal; its undo token (decision 0026). A journal that cannot be written costs only the
    /// undo: the change itself is done.
    fn keep(&self, change: Change) -> Option<String> {
        match self.journal.record_change(super::library::epoch_now(), &change, asking_account(self.served.as_deref()).as_deref()) {
            Ok(id) => Some(format!("c{id}")),
            Err(e) => {
                tracing::warn!("journal: cannot keep a change for its undo: {e:#}");
                None
            }
        }
    }

    /// Keep the playlists' files that changed (`None` when none did).
    fn keep_texts(&self, edits: Vec<Edited>) -> Option<String> {
        let changes: Vec<TextChange> = edits
            .into_iter()
            .filter(|e| e.before != e.after)
            .map(|e| TextChange { left: fingerprint(&e.after), playlist: e.id, before: e.before })
            .collect();
        (!changes.is_empty()).then(|| self.keep(Change::Texts { changes })).flatten()
    }

    /// Undo a change by its token (decision 0026): only while what it left is still there (else "It changed since",
    /// and nothing is undone), and only once. Returns the redo's token: the undo is a change of its own.
    pub async fn undo(&self, token: &str) -> Result<Option<String>, ToggleError> {
        let gone = || ToggleError::Refused("That can no longer be undone.".into());
        let id: EventId = token.strip_prefix('c').and_then(|n| n.parse().ok()).ok_or_else(gone)?;
        let (change, undone, account) = self.journal.change(id)?.ok_or_else(gone)?;
        // One's own changes only (decision 0051).
        if account != asking_account(self.served.as_deref()) {
            return Err(gone());
        }
        if undone || !self.journal.claim_change(id, super::library::epoch_now())? {
            return Err(ToggleError::Refused("That was undone already.".into()));
        }
        match self.reverse(change, account.as_deref()).await {
            Ok(redo) => Ok(self.keep(redo)),
            Err(e) => {
                if let Err(j) = self.journal.release_change(id) {
                    tracing::warn!("journal: {j:#}");
                }
                Err(e)
            }
        }
    }

    /// Put back what a change replaced, after checking that everything it left is still there. Returns the change
    /// that undoes this undo.
    async fn reverse(&self, change: Change, account: Option<&str>) -> Result<Change, ToggleError> {
        let changed = || ToggleError::Refused("It changed since; nothing was undone.".into());
        let store = self.lib.store_for(account)?;
        let left = |text: Option<String>| text.map(|t| fingerprint(&t));
        match change {
            Change::Texts { changes } => {
                for c in &changes {
                    if left(store.text(&c.playlist).await) != Some(c.left) {
                        return Err(changed());
                    }
                }
                let mut redo = Vec::new();
                for c in changes {
                    match store.put_back_if(&c.playlist, c.left, &c.before).await? {
                        Some(e) => redo.push(TextChange { left: fingerprint(&e.after), playlist: e.id, before: e.before }),
                        None => tracing::warn!("undo: {} changed while it was being put back", c.playlist),
                    }
                }
                self.tell_sources(&store, redo.iter().map(|c| (c.playlist.clone(), PlaylistChange::Edited)).collect());
                Ok(Change::Texts { changes: redo })
            }
            Change::Created { playlists } => {
                for (name, l) in &playlists {
                    if left(store.text(name).await) != Some(*l) {
                        return Err(changed());
                    }
                }
                let mut gone = Vec::new();
                for (name, l) in playlists {
                    if store.delete_if(&name, l).await? {
                        gone.push(name);
                    }
                }
                self.tell_sources(&store, gone.iter().map(|n| (n.clone(), PlaylistChange::Deleted)).collect());
                Ok(Change::Deleted { playlists: gone })
            }
            Change::Deleted { playlists } => {
                for name in &playlists {
                    if store.text(name).await.is_some() {
                        return Err(changed());
                    }
                }
                let mut back = Vec::new();
                for name in playlists {
                    let text = store.restore_deleted(&name).await?;
                    if store.account().is_none() {
                        self.sync.restore_links(&name).await?;
                    }
                    back.push((name, fingerprint(&text)));
                }
                Ok(Change::Created { playlists: back })
            }
            Change::Renamed { from, to } => {
                if store.text(&to).await.is_none() || store.text(&from).await.is_some() {
                    return Err(changed());
                }
                self.rename_unkept(&store, &to, &from).await?;
                Ok(Change::Renamed { from: to, to: from })
            }
            Change::Spoken { playlist, before, after } => {
                let key = store.key(&playlist);
                if self.sync.spoken().remove(&key).unwrap_or_default() != after || store.text(&playlist).await.is_none() {
                    return Err(changed());
                }
                self.sync.set_spoken(&key, &before)?;
                Ok(Change::Spoken { playlist, before: after, after: before })
            }
            Change::Crate { before, after } => {
                let Some(id) = after.as_ref().or(before.as_ref()).map(|c| c.id.clone()) else { return Err(changed()) };
                let crates = self.crate_stores.of(account)?;
                if crates.list().await?.into_iter().find(|c| c.id == id) != after {
                    return Err(changed());
                }
                match &before {
                    Some(b) => crates.put(b.clone()).await.map_err(ToggleError::Refused)?,
                    None => {
                        crates.delete(&id).await?;
                    }
                }
                Ok(Change::Crate { before: after, after: before })
            }
            Change::LeftOut { crate_id, before, after } => {
                let crates = self.crate_stores.of(account)?;
                if crates.left_out(&crate_id).await? != after {
                    return Err(changed());
                }
                crates.set_left_out(&crate_id, before.clone()).await?;
                Ok(Change::LeftOut { crate_id, before: after, after: before })
            }
            Change::Excluded { before, after } => {
                let crates = self.crate_stores.of(account)?;
                if crates.excluded().await? != after {
                    return Err(changed());
                }
                crates.set_excluded(before.clone()).await?;
                Ok(Change::Excluded { before: after, after: before })
            }
            Change::Added { crate_id, before, after } => {
                let crates = self.crate_stores.of(account)?;
                if crates.added(&crate_id).await? != after {
                    return Err(changed());
                }
                crates.set_added(&crate_id, before.clone()).await?;
                Ok(Change::Added { crate_id, before: after, after: before })
            }
            Change::ArtistOut { user, key, name, out } => {
                let serve = self.served.clone().ok_or_else(changed)?;
                let (u, k, n) = (user.clone(), key.clone(), name.clone());
                tokio::task::spawn_blocking(move || -> Result<(), ToggleError> {
                    if serve.userdata.excluded_artists(&u)?.contains(&k) != out {
                        return Err(changed());
                    }
                    if out {
                        serve.userdata.include_artist(&u, &k)?;
                    } else {
                        serve.userdata.exclude_artist(&u, &k, &n)?;
                    }
                    Ok(())
                })
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))??;
                Ok(Change::ArtistOut { user, key, name, out: !out })
            }
            Change::Tempo { tracks } => {
                // Each track's BPM tag as the change left it, else "it changed since"; then the tags before.
                if !crate::serve::access::administering() {
                    return Err(ToggleError::Forbidden("Adjusting a track's BPM is for administrators.".into()));
                }
                let cat = self.lib.catalogue();
                let back = tokio::task::spawn_blocking(move || -> Result<Vec<super::tempo::TempoChange>, ToggleError> {
                    for t in &tracks {
                        let (_, _, path) = super::tempo::place(&cat, &t.source, &t.id).ok_or_else(changed)?;
                        let now = crate::retag::bpm(&path).and_then(|v| v.trim().parse::<f64>().ok()).map(|b| b.round() as u32);
                        if now != t.after {
                            return Err(changed());
                        }
                    }
                    let mut redo = Vec::new();
                    for t in tracks {
                        super::tempo::write(&cat, &t.source, &t.id, t.before)?;
                        redo.push(super::tempo::TempoChange { before: t.after, after: t.before, ..t });
                    }
                    Ok(redo)
                })
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))??;
                Ok(Change::Tempo { tracks: back })
            }
        }
    }

    /// Every playlist, marked where it holds `entry` (the playing track's line), with its links.
    /// The asking account's own, and the ones others share (decision 0051), by the ids it knows them by.
    pub(super) async fn view(&self, entry: Option<&str>) -> Result<Vec<PlaylistView>> {
        let Ok(own) = self.lib.store_for(asking_account(self.served.as_deref()).as_deref()) else { return Ok(Vec::new()) };
        let shared = self.shared_keys();
        let mut list = own.view(entry).await?;
        let (mut links, mut spoken) = (self.sync.links(), self.sync.spoken());
        let mine = owner_name(self.served.as_deref(), own.account());
        for p in &mut list {
            let key = own.key(&p.id);
            // Kept in step with other servers: the first administrator's only.
            if own.account().is_none() {
                p.links = links.remove(&p.id).unwrap_or_default();
            }
            p.spoken = spoken.remove(&key).unwrap_or_default();
            (p.owner, p.shared, p.mine) = (mine.clone(), shared.contains(&key), true);
        }
        for store in self.others(&own) {
            if !shared.iter().any(|k| store.name_of_key(k).is_some()) {
                continue;
            }
            let owner = owner_name(self.served.as_deref(), store.account());
            for mut p in store.view(entry).await? {
                if shared.contains(&store.key(&p.id)) {
                    p.id = format!("{owner}/{}", p.name);
                    (p.owner, p.shared, p.mine) = (owner.clone(), true, false);
                    list.push(p);
                }
            }
        }
        list.sort_by_key(|p| p.name.to_lowercase());
        Ok(list)
    }

    /// The other accounts' playlists, beside `own`.
    fn others(&self, own: &Store) -> Vec<Arc<Store>> {
        self.lib.all_stores().into_iter().filter(|s| s.account() != own.account()).collect()
    }

    /// The keys of the playlists their owners share.
    fn shared_keys(&self) -> HashSet<String> {
        let Some(serve) = self.served.clone() else { return HashSet::new() };
        serve.userdata.shared_playlists().inspect_err(|e| tracing::warn!("the shared playlists not read: {e:#}")).unwrap_or_default()
    }

    /// `id` as the asking account names it (decision 0051), to read: its own playlist by name, or another's shared
    /// one as `owner/name` (one not shared is as if there were none).
    pub fn reach(&self, id: &str) -> Result<Reached, ToggleError> {
        let me = asking_account(self.served.as_deref());
        let (account, name) = match id.split_once('/') {
            Some((owner, name)) => (account_of(self.served.as_deref(), owner), name),
            None => (me.clone(), id),
        };
        let store = self.lib.store_for(account.as_deref())?;
        let shared = self.shared_keys().contains(&store.key(name));
        let mine = account == me;
        if !mine && !shared {
            return Err(ToggleError::UnknownPlaylist);
        }
        Ok(Reached { owner: owner_name(self.served.as_deref(), store.account()), store, name: name.to_string(), shared, mine })
    }

    /// `id` to change: the asking account's own only (another's shared one is played and copied).
    fn own(&self, id: &str) -> Result<(Arc<Store>, String), ToggleError> {
        let r = self.reach(id)?;
        if !r.mine {
            return Err(ToggleError::Forbidden("Only its owner changes it: copy it into your own.".into()));
        }
        Ok((r.store, r.name))
    }

    /// The asking account's playlists and the ones others share, with their lines and when each last changed, by id.
    pub(super) async fn visible_lines(&self) -> Result<Vec<(String, i64, Vec<String>)>, ToggleError> {
        let Ok(own) = self.lib.store_for(asking_account(self.served.as_deref()).as_deref()) else { return Ok(Vec::new()) };
        let mut out = own.lines_of_all().await?;
        let shared = self.shared_keys();
        for store in self.others(&own) {
            if !shared.iter().any(|k| store.name_of_key(k).is_some()) {
                continue;
            }
            let owner = owner_name(self.served.as_deref(), store.account());
            for (name, changed, lines) in store.lines_of_all().await? {
                if shared.contains(&store.key(&name)) {
                    out.push((format!("{owner}/{name}"), changed, lines));
                }
            }
        }
        Ok(out)
    }

    /// Shares one's own playlist with everyone, or stops (decision 0051): they see it, play it and copy it.
    pub fn set_shared(&self, id: &str, on: bool) -> Result<(), ToggleError> {
        let (store, name) = self.own(id)?;
        if !store.path_exists(&name) {
            return Err(ToggleError::UnknownPlaylist);
        }
        let serve = self.served.clone().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        // As the account's permissions say (decision 0051); stopping is always one's own to do.
        let may = serve.current_user().and_then(|u| serve.users.get(&u)).is_none_or(|u| u.permissions.share);
        if on && !may {
            return Err(ToggleError::Forbidden("This account may not share playlists.".into()));
        }
        Ok(serve.userdata.set_playlist_shared(&store.key(&name), on)?)
    }

    /// `name`, or the first free one after it ("Evening 2") among the asking account's own playlists.
    pub async fn free_own_name(&self, name: &str) -> Result<String, ToggleError> {
        let own = self.lib.store_for(asking_account(self.served.as_deref()).as_deref())?;
        let taken: HashSet<String> = own.view(None).await?.into_iter().map(|p| p.id.to_lowercase()).collect();
        Ok(crate::playlists::free_name(name, |n| taken.contains(&n.to_lowercase())))
    }

    /// A copy of a playlist (another's shared one, or one's own) among one's own, under a free name. Returns its name
    /// and the undo token.
    pub async fn copy(&self, id: &str) -> Result<(String, Option<String>), ToggleError> {
        let from = self.reach(id)?;
        let entries = from.store.entries(&from.name).await?;
        let own = self.lib.store_for(asking_account(self.served.as_deref()).as_deref())?;
        let taken: HashSet<String> = own.view(None).await?.into_iter().map(|p| p.id.to_lowercase()).collect();
        let name = crate::playlists::free_name(&from.name, |n| taken.contains(&n.to_lowercase()));
        let undo = self.create_with(&name, &entries).await?;
        Ok((name, undo))
    }

    /// A playlist id the asking account would know a key by (`Store::key`): its own by name, another's as `owner/name`.
    pub fn id_of_key(&self, key: &str) -> String {
        let me = asking_account(self.served.as_deref());
        match self.lib.store_for(me.as_deref()).ok().and_then(|own| own.name_of_key(key).map(str::to_string)) {
            Some(name) => name,
            None => match key.split_once('/') {
                Some((account, name)) => format!("{account}/{name}"),
                None => format!("{}/{key}", owner_name(self.served.as_deref(), None)),
            },
        }
    }

    /// The playlist meant by what was said (decision 0015), by its name or one it is spoken as; none when nothing fits.
    /// Its tracks are chosen as for any play (app::play::by_name).
    pub async fn spoken(&self, said: &str) -> Result<Option<String>, ToggleError> {
        let list = self.view(None).await?;
        let named: Vec<crate::voice::Named> = list.iter().map(|p| crate::voice::Named { name: &p.id, spoken: &p.spoken }).collect();
        Ok(crate::voice::find(crate::voice::spoken_name(said), &named).map(str::to_string))
    }

    /// Set the other names a playlist is spoken as. Blank ones are dropped. Returns the undo token (none when nothing
    /// changed).
    pub async fn set_spoken(&self, id: &str, names: &[String]) -> Result<Option<String>, ToggleError> {
        let (store, name) = self.own(id)?;
        if store.entries(&name).await.is_err() {
            return Err(ToggleError::UnknownPlaylist);
        }
        let names: Vec<String> = names.iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
        let key = store.key(&name);
        let before = self.sync.spoken().remove(&key).unwrap_or_default();
        self.sync.set_spoken(&key, &names)?;
        Ok((before != names).then(|| self.keep(Change::Spoken { playlist: name, before, after: names })).flatten())
    }

    /// What a playlist can be linked to.
    pub async fn targets(&self) -> Vec<super::sync::TargetChoice> {
        self.sync.choices().await
    }

    /// Link a playlist to a target (decision 0014), or change how it is kept.
    pub async fn link(&self, id: &str, target: &str) -> Result<(), ToggleError> {
        let kind = Kind::from_key(target).ok_or_else(|| ToggleError::Refused("not a target".into()))?;
        self.sync.link(id, kind).await
    }

    /// Take a link off: its copy on the target is deleted.
    pub async fn unlink(&self, id: &str, target: &str) -> Result<(), ToggleError> {
        let kind = Kind::from_key(target).ok_or_else(|| ToggleError::Refused("not a target".into()))?;
        self.sync.unlink(id, &kind).await
    }

    /// Make `entry` a member of the playlist or not (`Some`), or flip it (`None`, older pages). Only a real change
    /// is passed on.
    pub(super) async fn set_entry(&self, playlist_id: &str, entry: &str, member: Option<bool>) -> Result<(), ToggleError> {
        let (store, name) = self.own(playlist_id)?;
        let changed = match member {
            Some(m) => store.set_member(&name, entry, m).await?,
            None => {
                store.toggle(&name, entry).await?;
                true
            }
        };
        if changed {
            self.tell_sources(&store, vec![(name, PlaylistChange::Edited)]);
        }
        Ok(())
    }

    /// Why none of a playlist's tracks can be played on the server: usually the catalogue is still matching files to
    /// it (a first fill takes minutes).
    pub async fn why_none(&self, want: Option<&str>) -> Result<String, ToggleError> {
        let sources = self.lib.connected();
        let (active, _) = want
            .and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .cloned()
            .ok_or_else(|| ToggleError::CannotPlay("no server is connected".into()))?;
        let c = self.lib.catalogue();
        let status = tokio::task::spawn_blocking(move || c.status()).await.ok().and_then(Result::ok);
        Ok(match status {
            Some(st) if st.linked.get(&active.cfg.id).copied().unwrap_or(0) < st.files => format!(
                "Cratarr is still matching your music with {} ({} of {} tracks); try again in a few minutes",
                label_of(&active.cfg),
                st.linked.get(&active.cfg.id).copied().unwrap_or(0),
                st.files
            ),
            _ => format!("none of this playlist's tracks are on {}", label_of(&active.cfg)),
        })
    }

    /// A playlist's tracks for the local player (decision 0022): the server they are known on, their ids in file order,
    /// and how many lines it does not know.
    pub async fn track_ids(&self, id: &str, want: Option<&str>) -> Result<(String, Vec<String>, usize), ToggleError> {
        let (active, _, ids, unknown) = self.server_ids(id, want).await?;
        Ok((active.cfg.id.clone(), ids, unknown))
    }

    /// A playlist's rows for the playlist view (decision 0023), in file order: each entry with what the catalogue knows
    /// of its file (the tags, the length, the server's id when matched) and whether the file is there. One read of the
    /// catalogue, no request to the server. Returns the server the ids are for.
    pub async fn rows(&self, id: &str, want: Option<&str>) -> Result<(String, Vec<PlaylistRow>), ToggleError> {
        let r = self.reach(id)?;
        let entries = r.store.entries(&r.name).await?;
        let sources = self.lib.connected();
        let source = want
            .and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .map(|(a, _)| a.cfg.id.clone())
            .unwrap_or_default();
        let mut rows = self.rows_of(entries.clone(), &source).await?;
        // The user's plays of each track and when each entry came, as the user data keeps them (the panel's orders).
        if let Some(serve) = self.served.clone() {
            let (id, rs) = (r.key(), rows.iter().map(|r| r.id.clone().unwrap_or_default()).collect::<Vec<_>>());
            let me = serve.current_user();
            let facts = tokio::task::spawn_blocking(move || {
                let plays = match me {
                    Some(me) => serve.userdata.play_stats(&me, &rs).unwrap_or_default(),
                    None => Default::default(),
                };
                let added = serve.userdata.entries_added(&id, &entries).unwrap_or_default();
                (plays, added)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
            let (plays, added) = facts;
            for (i, row) in rows.iter_mut().enumerate() {
                if let Some((count, last)) = row.id.as_ref().and_then(|t| plays.get(t)) {
                    row.play_count = Some(*count);
                    row.last_played = *last;
                }
                row.added = added.get(i).copied().flatten();
            }
        }
        Ok((source, rows))
    }

    /// Rows for these lines (a playlist's, or a page of a crate's), with what the catalogue knows of their files.
    pub(super) async fn rows_of(&self, entries: Vec<String>, source: &str) -> Result<Vec<PlaylistRow>, ToggleError> {
        let (c, sid, music, served) = (self.lib.catalogue(), source.to_string(), self.lib.music(), self.served.clone());
        let rows = tokio::task::spawn_blocking(move || -> Result<Vec<PlaylistRow>> {
            let facts = c.line_rows(&sid, &entries)?;
            // Cratarr's own tracks with their album, as the library has it (the app's downloads are told it, and find
            // their album and its cover by it).
            let (albums, album_tracks): (HashMap<String, String>, HashMap<String, u32>) = match served {
                Some(serve) if sid == crate::source::cratarr::KIND.id => {
                    let ids: Vec<String> = facts.iter().filter_map(|f| f.track_id.clone()).collect();
                    let albums = serve.library.albums_of(&ids)?;
                    // Each album's size, for the Records view's "3 of 10" (the owner, 2026-10-30).
                    let mut of: Vec<String> = albums.values().cloned().collect();
                    of.sort();
                    of.dedup();
                    let sizes = serve.library.albums_by_id(&of)?.into_iter().map(|a| (a.id, a.song_count)).collect();
                    (albums, sizes)
                }
                _ => Default::default(),
            };
            let dir = music.map(std::path::PathBuf::from);
            Ok(entries
                .into_iter()
                .zip(facts)
                .map(|(entry, f)| PlaylistRow {
                    album_id: f.track_id.as_ref().and_then(|t| albums.get(t).cloned()),
                    album_tracks: f.track_id.as_ref().and_then(|t| albums.get(t)).and_then(|a| album_tracks.get(a).copied()),
                    missing: !line_exists(&c, dir.as_deref(), &entry),
                    file: entry.rsplit('/').next().unwrap_or(&entry).to_string(),
                    entry,
                    id: f.track_id,
                    name: f.title,
                    artist: f.artist,
                    album: f.album,
                    duration_secs: f.duration_ms.map(|ms| ms as f64 / 1000.0),
                    album_artist: f.album_artist,
                    disc: f.disc,
                    track: f.track,
                    year: f.year,
                    bpm: f.bpm,
                    play_count: None,
                    last_played: None,
                    added: None,
                    has_versions: None,
                })
                .collect())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        Ok(rows)
    }

    /// An export's plan (decision 0007): the playlists' lines as files in the Music folder, each file once, with its
    /// size, the tags its name needs and its format for the estimates. Read from the disk and the catalogue; no request
    /// to the server.
    pub async fn export_plan(&self, names: &[String]) -> Result<crate::export::Plan, ToggleError> {
        let music = self.lib.music().ok_or(ToggleError::NoMusicFolder)?;
        let mut lists = Vec::new();
        for n in names {
            let r = self.reach(n)?;
            lists.push((r.name.clone(), r.store.entries(&r.name).await?));
        }
        let c = self.lib.catalogue();
        let plan = tokio::task::spawn_blocking(move || {
            crate::export::plan(std::path::Path::new(&music), lists, |rels| {
                let rows = c.line_rows("", rels)?;
                Ok(rels
                    .iter()
                    .zip(rows)
                    .map(|(rel, row)| {
                        let (format, kbps) = c.place_of_line(rel).map(|(f, r)| c.format_of(f, &r)).unwrap_or_default();
                        export_facts(row, format, kbps)
                    })
                    .collect())
            })
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        Ok(plan)
    }

    /// An export's plan for a list of tracks by their servers' ids (decision 0058: an album, an artist's, a selection,
    /// a queue): each track's file wherever the catalogue has it (the Music folder or another library's), with what it
    /// knows of it. A track it cannot place is left out, named in the report by its server and id; so is a Bandcamp
    /// preview's.
    pub async fn export_tracks(&self, name: &str, tracks: Vec<(String, String)>) -> Result<crate::export::Plan, ToggleError> {
        self.export_places(name, move |c: &crate::catalogue::Catalogue| {
            tracks.iter().map(|(s, id)| (format!("{s} {id}"), export_place(c, s, id))).collect()
        })
        .await
    }

    /// An export's plan for files (folder, rel), each with the label the report names it by when it cannot be placed.
    async fn export_places(
        &self,
        name: &str,
        places: impl FnOnce(&crate::catalogue::Catalogue) -> Vec<(String, Option<(i64, String)>)> + Send + 'static,
    ) -> Result<crate::export::Plan, ToggleError> {
        let c = self.lib.catalogue();
        let name = name.to_string();
        let plan = tokio::task::spawn_blocking(move || -> anyhow::Result<crate::export::Plan> {
            let folders = c.folders();
            let (labels, places): (Vec<String>, Vec<Option<(i64, String)>>) = places(&*c).into_iter().unzip();
            let rows = c.place_rows(&places)?;
            let found = labels
                .into_iter()
                .zip(places)
                .zip(rows)
                .map(|((label, place), row)| {
                    let (folder, rel) = place.ok_or(label)?;
                    let path = folders.get(folder as usize).ok_or_else(|| rel.clone())?.join(&rel);
                    let (format, kbps) = c.format_of(folder, &rel);
                    Ok((path, rel, export_facts(row, format, kbps)))
                })
                .collect();
            Ok(crate::export::plan_files(&name, found))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        Ok(plan)
    }

    /// Where an export makes its converted files before sending them: the data folder's `export`, else the system's
    /// temporary folder.
    pub fn export_work(&self) -> std::path::PathBuf {
        match self.lib.data_dir() {
            Some(d) => std::path::Path::new(d).join("export"),
            None => std::env::temp_dir().join("cratarr-export"),
        }
    }

    /// The server (`want`, else the first connected) and its id.
    pub(super) fn source_of(&self, want: Option<&str>) -> Result<(String, Arc<dyn source::Source>), ToggleError> {
        let sources = self.lib.connected();
        want.and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .map(|(a, s)| (a.cfg.id.clone(), s.clone()))
            .ok_or_else(|| ToggleError::Refused("No server is connected.".into()))
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
        let album_tracks = album_lines(self.served.clone(), self.lib.catalogue(), left_set.values().flat_map(|l| l.albums.iter().cloned()).collect()).await;
        let excluded = self.own_crates()?.excluded().await?;
        let artists = self.artists_out().await?;
        let none = crates::LeftOut::default();
        let favourites_of = |id: &str| set.get(id).copied().unwrap_or(crates::Favourites::None);
        let albums_of = |id: &str| albums_set.get(id).copied().unwrap_or(crates::Favourites::All);
        let needed = own.iter().any(|c| crates::needs_favourites(&c.conditions) || favourites_of(&c.id) != crates::Favourites::All);
        let needed_albums = own.iter().any(|c| crates::needs_favourite_albums(&c.conditions) || albums_of(&c.id) != crates::Favourites::All);
        let lib = self.crate_library(want, needed, needed_albums, without).await?;
        let mut lines = HashSet::new();
        for c in &own {
            let gone = left_lines(left_set.get(&c.id).unwrap_or(&none), &album_tracks, &excluded);
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
        let albums_of = |id: &str| albums_set.get(id).copied().unwrap_or(crates::Favourites::All);
        let left_set = self.own_crates()?.all_left_out().await?;
        let album_tracks = album_lines(self.served.clone(), self.lib.catalogue(), left_set.values().flat_map(|l| l.albums.iter().cloned()).collect()).await;
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
            crates::needs_favourites(&c.conditions) || set.get(&c.id).copied().unwrap_or(crates::Favourites::None) != crates::Favourites::All
        });
        let needed_albums = all.iter().any(|(_, c)| crates::needs_favourite_albums(&c.conditions) || albums_of(&c.id) != crates::Favourites::All);
        let lib = self.crate_library(want, needed, needed_albums, without).await?;
        let count = |conditions: Vec<crates::Condition>| lib.knows(&conditions).then(|| lib.select(&conditions).len());
        // The cards' facts: the songs of every folder by their lines, and what the account asking has played.
        let songs_played = match self.served.clone() {
            Some(serve) => {
                let (me, c) = (serve.current_user(), self.lib.catalogue());
                tokio::task::spawn_blocking(move || {
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
        let facts: HashMap<&str, &crates::Facts> = lib.files.iter().map(|f| (f.rel.as_str(), f)).collect();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let images = self.crate_images();
        Ok(all
            .into_iter()
            .map(|(permanent, c)| {
                let favourites = set.get(&c.id).copied().unwrap_or(crates::Favourites::None);
                let favourite_albums = albums_of(&c.id);
                let left = left_of(&c.id);
                let gone = left_lines(left, &album_tracks, &excluded);
                let shown = crates::leaving_out_artists(&artists, &crates::leaving_out(&gone, &crates::shown(&c.conditions, favourites, favourite_albums)));
                let stats = lib.knows(&shown).then(|| crate_stats(&lib.select(&shown), &facts, &songs, played.as_ref(), now));
                // Per setting of the favourites toggle, the albums toggle as it is set and the left-out list.
                let counted = |tracks: crates::Favourites| {
                    count(crates::leaving_out_artists(&artists, &crates::leaving_out(&gone, &crates::shown(&c.conditions, tracks, favourite_albums))))
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
                    image: images.as_ref().is_some_and(|d| d.join(image_file(&c.id)).is_file()),
                    id: c.id,
                    name: c.name,
                    permanent,
                }
            })
            .collect())
    }

    /// Where crates' own pictures are kept (in the data folder); `None` without one.
    fn crate_images(&self) -> Option<std::path::PathBuf> {
        self.lib.data_dir().map(|d| std::path::PathBuf::from(d).join("crate-images"))
    }

    /// Sets a crate's own picture: any picture Cratarr can read, kept as JPEG at most 800 px on its longer side.
    pub async fn set_crate_image(&self, id: &str, data: Vec<u8>) -> Result<(), ToggleError> {
        self.crate_def(id).await?;
        self.may_picture(id)?;
        let Some(dir) = self.crate_images() else { return Err(ToggleError::SettingsReadOnly) };
        let file = dir.join(image_file(id));
        tokio::task::spawn_blocking(move || -> Result<()> {
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
        let file = self.crate_images()?.join(image_file(id));
        tokio::task::spawn_blocking(move || {
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
        if let Some(file) = self.crate_images().map(|d| d.join(image_file(id))) {
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
    pub async fn crate_members(&self) -> Result<Vec<(String, String, Vec<String>)>, ToggleError> {
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
        let album_tracks = album_lines(self.served.clone(), self.lib.catalogue(), left_set.values().flat_map(|l| l.albums.iter().cloned()).collect()).await;
        let excluded = self.own_crates()?.excluded().await?;
        let artists = self.artists_out().await?;
        let none = crates::LeftOut::default();
        let favourites_of = |id: &str| set.get(id).copied().unwrap_or(crates::Favourites::None);
        let albums_of = |id: &str| albums_set.get(id).copied().unwrap_or(crates::Favourites::All);
        let needed = all.iter().any(|c| crates::needs_favourites(&c.conditions) || favourites_of(&c.id) != crates::Favourites::All);
        let needed_albums = all.iter().any(|c| crates::needs_favourite_albums(&c.conditions) || albums_of(&c.id) != crates::Favourites::All);
        let lib = self.crate_library(None, needed, needed_albums, &[]).await?;
        Ok(all
            .into_iter()
            .filter_map(|c| {
                let gone = left_lines(left_set.get(&c.id).unwrap_or(&none), &album_tracks, &excluded);
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
        let mut def = self.made_def(id).await?;
        if crates::is_fixed(id) {
            def.conditions.extend(self.own_crates()?.added(&crates::kept_id(id)).await?);
        }
        Ok(def)
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
                None => like_crate(self.served.clone(), id, item, count).await?,
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
        let serve = self.served.clone().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let (name, albums) = if id == crates::HOME_NEW {
            ("New for you", self.new_for_you().await?)
        } else {
            ("Grave digging", self.digging(None, &[]).await?.graves.into_iter().map(|g| g.id).collect())
        };
        let lines = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
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
        let serve = self.served.clone().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let lines = self.entries(list).await?;
        let songs = tokio::task::spawn_blocking(move || {
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
        let album_tracks = album_lines(self.served.clone(), self.lib.catalogue(), left.albums.iter().cloned().collect()).await;
        let down: HashSet<String> = left.lines.into_iter().chain(album_tracks.into_values().flatten()).collect();
        let lib = self.crate_library(None, false, false, &[]).await?;
        let traits = tokio::task::spawn_blocking(move || {
            let all: Vec<&crates::Facts> = lib.files.iter().collect();
            let turned: Vec<&crates::Facts> = all.iter().copied().filter(|f| down.contains(&f.rel)).collect();
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
                Some(list) if self.entries(list).await.is_ok() => format!("Sounds like {}", list.rsplit('/').next().unwrap_or(list)),
                Some(_) => continue,
                None => match like_crate(self.served.clone(), &id, item, count).await {
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
        Ok(self.own_crates()?.set_fit(&format!("{}{item}", crates::LIKE), fit).await?)
    }

    /// The lines these conditions select from the libraries not in `without`, newest first, and the server they are for.
    pub(super) async fn crate_lines_of(&self, conditions: &[crates::Condition], want: Option<&str>, without: &[String]) -> Result<(String, Vec<String>), ToggleError> {
        let (lib, lines) = self.crate_selection(conditions, want, without).await?;
        Ok((lib.source, lines))
    }

    /// The library a crate is worked out from, and its lines.
    async fn crate_selection(&self, conditions: &[crates::Condition], want: Option<&str>, without: &[String]) -> Result<(CrateLibrary, Vec<String>), ToggleError> {
        let lib = self.crate_library_for(conditions, want, without).await?;
        let lines = lib.select(conditions);
        Ok((lib, lines))
    }

    /// The library these conditions are worked out on, with what they need of the favourites (refused when those
    /// cannot be read): a crate's, a filter's (decision 0061).
    pub(super) async fn crate_library_for(&self, conditions: &[crates::Condition], want: Option<&str>, without: &[String]) -> Result<CrateLibrary, ToggleError> {
        let lib = self.crate_library(want, crates::needs_favourites(conditions), crates::needs_favourite_albums(conditions), without).await?;
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
        let shown = self.crate_shown(id, look).await?;
        self.crate_page(&shown, want, offset, limit, look).await
    }

    /// A crate's conditions as its toggles show it (as given, else as saved), with its left-out tracks and the filter on
    /// it.
    pub(super) async fn crate_shown(&self, id: &str, look: &CrateLook) -> Result<Vec<crates::Condition>, ToggleError> {
        let def = self.crate_def(id).await?;
        let fav = match look.favourites {
            Some(f) => f,
            None => self.crate_favourites(id).await?,
        };
        let albums = match look.favourite_albums {
            Some(a) => a,
            None => self.own_crates()?.favourite_albums(id).await?,
        };
        let left = self.own_crates()?.left_out(id).await?;
        let album_tracks = album_lines(self.served.clone(), self.lib.catalogue(), left.albums.iter().cloned().collect()).await;
        let excluded = self.own_crates()?.excluded().await?;
        let artists = self.artists_out().await?;
        let mut shown = crates::leaving_out_artists(
            &artists,
            &crates::leaving_out(&left_lines(&left, &album_tracks, &excluded), &crates::shown(&def.conditions, fav, albums)),
        );
        shown.extend(look.filter.iter().cloned());
        Ok(shown)
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
                    self.keep(Change::Added { crate_id: kept, before, after })
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
                    self.keep(Change::Added { crate_id: kept, before, after })
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
        self.artists_out_of(asking_user(self.served.as_deref())).await
    }

    /// The keys of the artists `user` excluded everywhere; none without Cratarr's own library or an account.
    async fn artists_out_of(&self, user: Option<String>) -> Result<HashSet<String>, ToggleError> {
        let (Some(serve), Some(user)) = (self.served.clone(), user) else { return Ok(HashSet::new()) };
        Ok(tokio::task::spawn_blocking(move || serve.userdata.excluded_artists(&user)).await.map_err(|e| anyhow::anyhow!("{e}"))??)
    }

    /// The account the everywhere list is the asking one's of, and Cratarr's own library: refused without either.
    fn artist_lister(&self) -> Result<(Arc<crate::serve::Serve>, String), ToggleError> {
        let serve = self.served.clone().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
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
        let (new, listed) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let new = serve.userdata.exclude_artist(&u, &k, &n)?;
            Ok((new, serve.userdata.excluded_artist_list(&u)?.into_iter().find(|a| a.key == k)))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        let listed = listed.unwrap_or(crate::userdata::ExcludedArtist { key: key.clone(), name: name.clone(), at: super::library::epoch_now() });
        let undo = new.then(|| self.keep(Change::ArtistOut { user, key, name, out: true })).flatten();
        Ok((listed, undo))
    }

    /// An artist taken back off the asking account's everywhere list (by their key): picked again. Their key, and the
    /// undo token (none when they were not on it).
    pub async fn include_artist(&self, key: &str) -> Result<(String, Option<String>), ToggleError> {
        let (serve, user) = self.artist_lister()?;
        let key = super::excluded_artists::key(key);
        let (u, k) = (user.clone(), key.clone());
        let gone = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let was = serve.userdata.excluded_artist_list(&u)?.into_iter().find(|a| a.key == k);
            serve.userdata.include_artist(&u, &k)?;
            Ok(was)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        let undo = gone.and_then(|a| self.keep(Change::ArtistOut { user, key: key.clone(), name: a.name, out: false }));
        Ok((key, undo))
    }

    /// The asking account's everywhere list, the newest first (Settings › Listening).
    pub async fn excluded_artist_list(&self) -> Result<Vec<crate::userdata::ExcludedArtist>, ToggleError> {
        let (serve, user) = self.artist_lister()?;
        Ok(tokio::task::spawn_blocking(move || serve.userdata.excluded_artist_list(&user)).await.map_err(|e| anyhow::anyhow!("{e}"))??)
    }

    /// Tracks taken out of a crate by hand, or put back (`out` false), by their ids on a server, and whole albums by
    /// their ids (decision 0024, amended 2026-10-19): kept in the crate's left-out list, the tracks as their lines, an
    /// album as a rule. Returns how many it holds now (tracks and albums) and the undo token.
    pub async fn crate_leave_out(&self, id: &str, want: Option<&str>, ids: &[String], albums: &[String], out: bool) -> Result<(usize, Option<String>), ToggleError> {
        self.crate_def(id).await?;
        let lines = self.lines_of(want, ids).await?;
        // Only albums the library has go in; any comes back out.
        let albums: Vec<String> = if out { library_albums(self.served.clone(), albums.to_vec()).await?.into_iter().map(|a| a.id).collect() } else { albums.to_vec() };
        let store = self.own_crates()?;
        let before = store.left_out(id).await?;
        let after = crates::LeftOut { lines: edited(&before.lines, lines, out), albums: edited(&before.albums, albums, out) };
        if after == before {
            return Ok((after.len(), None));
        }
        store.set_left_out(id, after.clone()).await?;
        let undo = self.keep(Change::LeftOut { crate_id: id.to_string(), before, after: after.clone() });
        Ok((after.len(), undo))
    }

    /// A crate's tracks left out by hand, as rows on a server, and its albums left out, for its editor to put back.
    pub async fn crate_left_out(&self, id: &str, want: Option<&str>) -> Result<(String, Vec<PlaylistRow>, Vec<LeftOutAlbum>), ToggleError> {
        self.crate_def(id).await?;
        let (sid, _) = self.source_of(want)?;
        let left = self.own_crates()?.left_out(id).await?;
        let found = library_albums(self.served.clone(), left.albums.clone()).await?;
        let albums = left
            .albums
            .iter()
            .map(|a| {
                let f = found.iter().find(|x| &x.id == a);
                LeftOutAlbum { id: a.clone(), title: f.map(|x| x.name.clone()), artist: f.map(|x| x.artist.clone()) }
            })
            .collect();
        let rows = self.rows_of(left.lines, &sid).await?;
        Ok((sid, rows, albums))
    }

    /// Tracks' lines (Music's, or an Incoming track's climbing out of it) by their ids on a server.
    async fn lines_of(&self, want: Option<&str>, ids: &[String]) -> Result<HashSet<String>, ToggleError> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }
        let (sid, _) = self.source_of(want)?;
        let (c, ids) = (self.lib.catalogue(), ids.to_vec());
        Ok(tokio::task::spawn_blocking(move || c.lines_of(&sid, &ids)).await.map_err(|e| anyhow::anyhow!("{e}"))??)
    }

    /// Tracks excluded from every automatic pick, or put back (`out` false), by their ids on a server (decision 0024,
    /// amended 2026-10-19: global exclusions), the asking account's: out of every crate as a crate's left-out tracks
    /// are, and never picked by Radio, Autoplay, or a shuffle of the library or the favourites; played by hand, they
    /// play. Returns how many there are now and the undo token.
    pub async fn exclude(&self, want: Option<&str>, ids: &[String], out: bool) -> Result<(usize, Option<String>), ToggleError> {
        let lines = self.lines_of(want, ids).await?;
        let store = self.own_crates()?;
        let before = store.excluded().await?;
        let after = edited(&before, lines, out);
        if after == before {
            return Ok((after.len(), None));
        }
        store.set_excluded(after.clone()).await?;
        let undo = self.keep(Change::Excluded { before, after: after.clone() });
        Ok((after.len(), undo))
    }

    /// The tracks excluded everywhere, how many and as rows on a server, for the Library's list to put back.
    pub async fn excluded_rows(&self, want: Option<&str>) -> Result<(String, usize, Vec<PlaylistRow>), ToggleError> {
        let (sid, _) = self.source_of(want)?;
        let lines = self.own_crates()?.excluded().await?;
        let n = lines.len();
        Ok((sid.clone(), n, self.rows_of(lines, &sid).await?))
    }

    /// Cratarr's own tracks by their song ids, as rows (Home's Favourite tracks), in the order given; songs whose file is
    /// gone are left out.
    pub async fn song_rows(&self, ids: Vec<String>) -> Result<Vec<PlaylistRow>, ToggleError> {
        let cat = self.lib.catalogue();
        let lines = tokio::task::spawn_blocking(move || ids.iter().filter_map(|id| crate::serve::playlists::line_of_song(&cat, id)).collect::<Vec<_>>())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.rows_of(lines, crate::source::cratarr::KIND.id).await
    }

    /// The Cratarr song ids of the tracks `user` excluded everywhere (`None`: the first administrator), for the picks
    /// that leave them out: Radio's and Autoplay's, the shuffles of the library and the favourites, Subsonic's random songs.
    /// With them every song crediting an artist `user` excluded everywhere (decision 0083), by the crates' facts.
    pub async fn excluded_songs_of(&self, user: Option<&str>) -> Result<HashSet<String>, ToggleError> {
        let account = user.and_then(|u| account_of(self.served.as_deref(), u));
        let lines = self.crate_stores.of(account.as_deref())?.excluded().await?;
        let whose = user.map(str::to_string).or_else(|| self.served.as_ref().and_then(|s| s.users.owner()));
        let artists = self.artists_out_of(whose).await?;
        if lines.is_empty() && artists.is_empty() {
            return Ok(HashSet::new());
        }
        let cat = self.lib.catalogue();
        Ok(tokio::task::spawn_blocking(move || -> Result<HashSet<String>> {
            let mut songs: HashSet<String> = lines.iter().filter_map(|l| crate::serve::playlists::song_of_line(&cat, l)).collect();
            if !artists.is_empty() {
                songs.extend(songs_crediting(&cat.crate_facts()?, &artists));
            }
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

    /// A crate's favourites toggle as set; a genre not set shows all its tracks (a crate not set shows no favourites).
    async fn crate_favourites(&self, id: &str) -> Result<crates::Favourites, ToggleError> {
        match self.own_crates()?.all_favourites().await?.get(id) {
            Some(f) => Ok(*f),
            None if crates::genre_crate(id).is_some() || id.starts_with(crates::LIKE) || crates::home_crate(id) => Ok(crates::Favourites::All),
            None => Ok(self.own_crates()?.favourites(id).await?),
        }
    }

    /// Sets a crate's favourites toggle (decision 0024, amended): one setting per crate, for every device. Not journaled:
    /// a tap undoes it by tapping again.
    pub async fn set_crate_favourites(&self, id: &str, fav: crates::Favourites) -> Result<(), ToggleError> {
        self.crate_def(id).await?;
        Ok(self.own_crates()?.set_favourites(id, fav).await?)
    }

    /// Sets a crate's favourite albums toggle (decision 0024, amended 2026-10-19), as the favourites toggle is set.
    pub async fn set_crate_favourite_albums(&self, id: &str, albums: crates::Favourites) -> Result<(), ToggleError> {
        self.crate_def(id).await?;
        Ok(self.own_crates()?.set_favourite_albums(id, albums).await?)
    }

    /// A page of the rows these conditions select (a crate's, as `crate_shown` gives them; the builder's live preview,
    /// decision 0024, issue 6), from the libraries not in `look.without`, in `look`'s order (its toggles and filter are
    /// in the conditions already).
    pub async fn crate_page(&self, conditions: &[crates::Condition], want: Option<&str>, offset: usize, limit: usize, look: &CrateLook) -> Result<CratePage, ToggleError> {
        let (lib, lines) = self.crate_selection(conditions, want, &look.without).await?;
        // On the first page only: the panel shows them on top.
        let genres = if offset == 0 {
            let chosen: HashSet<&str> = lines.iter().map(String::as_str).collect();
            crates::top_genres(lib.files().filter(|f| chosen.contains(f.rel.as_str())).map(|f| f.genres.as_slice()), super::playing::GENRES_SHOWN)
        } else {
            Vec::new()
        };
        let total = lines.len();
        let lines = self.crate_in_order(&lib, lines, conditions, look).await?;
        let page: Vec<String> = lines.iter().skip(offset).take(limit).cloned().collect();
        let mut rows = self.rows_of(page, &lib.source).await?;
        self.with_plays_and_added(&mut rows, &lib).await;
        Ok(CratePage { source: lib.source, total, limit, rows, genres })
    }

    /// The lines these conditions select, in `look`'s order, and the server they are for: what Play plays as the list
    /// shows it.
    pub(super) async fn crate_lines_in_order(&self, conditions: &[crates::Condition], want: Option<&str>, look: &CrateLook) -> Result<(String, Vec<String>), ToggleError> {
        let (lib, lines) = self.crate_selection(conditions, want, &look.without).await?;
        let lines = self.crate_in_order(&lib, lines, conditions, look).await?;
        Ok((lib.source.clone(), lines.to_vec()))
    }

    /// A crate's lines (in its own order) in `look`'s order. By a field, every row of it is read and sorted, and the
    /// answer kept a little while for the next pages (`SORTED_KEEP`).
    async fn crate_in_order(&self, lib: &CrateLibrary, mut lines: Vec<String>, conditions: &[crates::Condition], look: &CrateLook) -> Result<Arc<Vec<String>>, ToggleError> {
        if look.order == CrateOrder::Own {
            if look.down {
                lines.reverse();
            }
            return Ok(Arc::new(lines));
        }
        // What the order was worked out from: the conditions, the server, the libraries, the order, who asks (their
        // plays), and the library's facts as read (a new reading after any change).
        let key = format!("{conditions:?}\n{}\n{:?}\n{:?} {}\n{:?}\n{:p}", lib.source, look.without, look.order, look.down, asking_user(self.served.as_deref()), Arc::as_ptr(&lib.files));
        if let Some((k, at, kept)) = self.sorted.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
            && *k == key
            && at.elapsed() < SORTED_KEEP
        {
            return Ok(kept.clone());
        }
        let mut rows = self.rows_of(lines, &lib.source).await?;
        self.with_plays_and_added(&mut rows, lib).await;
        sort_rows(&mut rows, look.order, look.down);
        let sorted = Arc::new(rows.into_iter().map(|r| r.entry).collect::<Vec<_>>());
        *self.sorted.lock().unwrap_or_else(|p| p.into_inner()) = Some((key, std::time::Instant::now(), sorted.clone()));
        Ok(sorted)
    }

    /// A crate's rows with what its list orders by besides the files' tags (decision 0060): the asking account's plays
    /// of each track and when it last played it (Cratarr's own plays, by the line's song, whatever server the crate is
    /// shown for), and when it came into the library (the catalogue's first sight of it).
    async fn with_plays_and_added(&self, rows: &mut [PlaylistRow], lib: &CrateLibrary) {
        let born: HashMap<&str, i64> = lib.files.iter().filter_map(|f| Some((f.rel.as_str(), f.born?))).collect();
        for r in rows.iter_mut() {
            r.added = born.get(r.entry.as_str()).copied();
        }
        let Some(serve) = self.served.clone() else { return };
        let Some(me) = serve.current_user() else { return };
        let (c, lines) = (self.lib.catalogue(), rows.iter().map(|r| r.entry.clone()).collect::<Vec<_>>());
        let plays = tokio::task::spawn_blocking(move || {
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
        let undo = self.create_with(name, &lines).await?;
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
        let before = match id {
            Some(id) => self.own_crate(id).await?,
            None => None,
        };
        let id = self.own_crates()?.save(id, name, conditions).await.map_err(ToggleError::Refused)?;
        let after = self.own_crate(&id).await?;
        let undo = (before != after).then(|| self.keep(Change::Crate { before, after })).flatten();
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
        Ok(self.keep(Change::Crate { before, after: None }))
    }

    /// How alike the tracks that sound most like these seeds (song ids) are, closest first, 0 to 100, at most
    /// `crates::LIKE_MOST` of them: the Sounds like curve (lib/LikeCurve), cut where the crate ends.
    /// Over what a seed crate may show, as its tracks are picked (`CrateLibrary::resolved`): the libraries not in
    /// `without`, less the tracks excluded everywhere. With the profile at every step its handle can stop at
    /// (`crates::profiles_along`), so the profile follows a drag (the owner, 2026-10-23).
    pub async fn like_curve(&self, of: &[String], weights: &[f32], want: Option<&str>, without: &[String], fresh: bool) -> Result<LikeCurve, ToggleError> {
        let lib = self.crate_library(want, false, false, without).await?;
        let excluded: HashSet<String> = self.own_crates()?.excluded().await?.into_iter().collect();
        let artists = self.artists_out().await?;
        let (of, weights, lib) = (of.to_vec(), weights.to_vec(), Arc::new(lib));
        tokio::task::spawn_blocking(move || {
            let among: Vec<&crates::Facts> = lib.files().filter(|f| !excluded.contains(&f.rel) && !crates::credits_out(f, &artists)).collect();
            let mut closest = crates::closest_weighted(&of, &weights, lib.alike(), &among, lib.by());
            // Fresh: the seeds left out, as the crate leaves them (crates::resolved).
            if fresh {
                let seeds: HashSet<i64> = of.iter().filter_map(|id| crate::serve::ids::track_of(id)).collect();
                closest.retain(|(f, _)| !f.track.is_some_and(|t| seeds.contains(&t)));
            }
            let alike = closest.iter().map(|(_, a)| (a * 10.0).round() / 10.0).collect();
            let tracks: Vec<&crates::Facts> = closest.iter().map(|(f, _)| *f).collect();
            let scales = crates::Scales::of(&lib.files().collect::<Vec<_>>());
            let profiles = crates::profiles_along(&tracks, &scales).into_iter().map(|(count, profile)| LikeStep { count, profile }).collect();
            LikeCurve { alike, profiles }
        })
        .await
        .map_err(|e| ToggleError::from(anyhow::anyhow!("{e}")))
    }

    /// Tracks' tempo as Adjust BPM shows it (app::tempo): each one's BPM now and Cratarr's measure.
    pub async fn tempo_of(&self, source: &str, ids: &[String]) -> Result<Vec<super::tempo::TrackTempo>, ToggleError> {
        let (cat, source, ids) = (self.lib.catalogue(), source.to_string(), ids.to_vec());
        Ok(tokio::task::spawn_blocking(move || ids.iter().filter_map(|id| super::tempo::tempo_of(&cat, &source, id)).collect())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?)
    }

    /// Adjust BPM (app::tempo): an administrator's, the library's for everyone. Writes each track's new BPM into its
    /// file and keeps the change for its undo; tracks with nothing to go from are left as they are. Returns the tracks'
    /// tempo after it, and the undo's token.
    pub async fn adjust_tempo(&self, source: &str, ids: &[String], how: super::tempo::Adjust) -> Result<(Vec<super::tempo::TrackTempo>, Option<String>), ToggleError> {
        if !crate::serve::access::administering() {
            return Err(ToggleError::Forbidden("Adjusting a track's BPM is for administrators.".into()));
        }
        let (cat, src, list) = (self.lib.catalogue(), source.to_string(), ids.to_vec());
        let changes = tokio::task::spawn_blocking(move || -> Result<Vec<super::tempo::TempoChange>, ToggleError> {
            let mut changes = Vec::new();
            for id in &list {
                let Some(now) = super::tempo::tempo_of(&cat, &src, id) else { continue };
                let Some(to) = super::tempo::adjusted(&now, how) else { continue };
                let (_, _, path) = super::tempo::place(&cat, &src, id).ok_or_else(|| ToggleError::Refused("A track's file is not known.".into()))?;
                let before = crate::retag::bpm(&path).and_then(|v| v.trim().parse::<f64>().ok()).map(|b| b.round() as u32);
                if before == Some(to) {
                    continue;
                }
                super::tempo::write(&cat, &src, id, Some(to)).map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
                changes.push(super::tempo::TempoChange { source: src.clone(), id: id.clone(), before, after: Some(to) });
            }
            Ok(changes)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        let undo = if changes.is_empty() { None } else { self.keep(Change::Tempo { tracks: changes }) };
        Ok((self.tempo_of(source, ids).await?, undo))
    }

    /// The axes and their domains in the libraries not in `without`, for the builder (decision 0024, issue 4).
    pub async fn crate_axes(&self, want: Option<&str>, without: &[String]) -> Result<Vec<crates::AxisView>, ToggleError> {
        let lib = self.crate_library(want, false, false, without).await?;
        let one: Vec<[&crates::Facts; 1]> = lib.files().map(|f| [f]).collect();
        let items: Vec<&[&crates::Facts]> = one.iter().map(|f| f.as_slice()).collect();
        Ok(crates::list_axes(&items, &self.own_playlist_names().await?, None, lib.uses()))
    }

    /// One's own playlists' names: the membership axis' values.
    pub(super) async fn own_playlist_names(&self) -> Result<Vec<String>, ToggleError> {
        Ok(self.own_store()?.snapshot().await?.into_iter().map(|(name, _, _)| name).collect())
    }

    /// What a crate is worked out on: every library's files newest first with their facts, less the folders of those in
    /// `without` (decision 0024, amended 2026-10-23), which playlists hold each line, and (when needed) the favourites,
    /// kept for a minute (decision 0024, issue 5).
    async fn crate_library(&self, want: Option<&str>, favourites: bool, albums: bool, without: &[String]) -> Result<CrateLibrary, ToggleError> {
        let (sid, source) = self.source_of(want)?;
        // First, so a request waiting for them holds nothing large meanwhile.
        let favourites = if favourites { self.favourite_lines(&sid, &source).await } else { None };
        let favourite_albums = if albums { favourite_album_lines(self.served.clone(), self.lib.catalogue()).await } else { None };
        let c = self.lib.catalogue();
        let folders = c.folders();
        let off: Vec<std::path::PathBuf> = match self.lib.connected().into_iter().find(|(a, _)| a.cfg.id == sid) {
            Some((a, _)) if !without.is_empty() => super::library::folders_switched_off(&a, without).into_iter().map(std::path::PathBuf::from).collect(),
            _ => Vec::new(),
        };
        let switched_off = folders.iter().enumerate().filter(|(_, f)| off.contains(f)).map(|(i, _)| i as i64).collect();
        let files = tokio::task::spawn_blocking(move || c.crate_facts()).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        let mut membership: HashMap<String, Vec<String>> = HashMap::new();
        for (name, lines, _) in self.own_store()?.snapshot().await? {
            for line in lines {
                membership.entry(line).or_default().push(name.clone());
            }
        }
        let by = asking_user(self.served.as_deref()).and_then(|u| super::radio::similar_by_of(&u)).map_or(crate::similarity::Weights::EQUAL, |o| crate::similarity::Weights::ranked(&o));
        let uses = match asking_user(self.served.as_deref()) {
            Some(user) => {
                let (files, served) = (files.clone(), self.served.clone());
                tokio::task::spawn_blocking(move || uses_of(served.as_deref(), &user, &files)).await.map_err(|e| anyhow::anyhow!("{e}"))?.ok()
            }
            None => None,
        };
        Ok(CrateLibrary { source: sid, files, switched_off, membership, favourites, favourite_albums, alike: Default::default(), uses, by })
    }

    /// The favourite tracks' lines, from the media server. What was heard last is answered at once, with Cratarr's own
    /// changes since applied; when it is a minute old, it is asked for again in the background. Only the first asking
    /// (nothing heard yet from this server) waits. One asks at a time: Jellyfin took a minute for 4,786 favourites
    /// (2026-10-14), and asking again while it works makes it slower. A server that does not answer is asked again a
    /// minute later.
    async fn favourite_lines(&self, sid: &str, source: &Arc<dyn source::Source>) -> Option<HashSet<String>> {
        let f = &self.favourites;
        let kept = match f.kept(sid) {
            Some(k) => {
                if k.at.elapsed() >= FAVOURITES_KEEP
                    && let Ok(turn) = f.asking.clone().try_lock_owned()
                {
                    let (f, c, s, from) = (f.clone(), self.lib.catalogue(), sid.to_string(), source.clone());
                    // For the account asking: their favourites.
                    tokio::spawn(crate::serve::who::carried(async move {
                        let _turn = turn;
                        f.ask(c, &s, &from).await;
                    }));
                }
                k
            }
            None => {
                let _turn = f.asking.lock().await;
                match f.kept(sid) {
                    Some(k) => k,
                    None => f.ask(self.lib.catalogue(), sid, source).await,
                }
            }
        };
        FavouritesKept::with_changes(kept, self.lib.catalogue()).await
    }

    /// A playlist's tracks as a server's ids, in file order (`want` = the server to use, default the first connected
    /// one), and how many lines it does not know (missing files, or not linked yet).
    async fn server_ids(
        &self,
        id: &str,
        want: Option<&str>,
    ) -> Result<(Arc<super::library::Active>, Arc<dyn source::Source>, Vec<String>, usize), ToggleError> {
        let r = self.reach(id)?;
        let entries = r.store.entries(&r.name).await?;
        let sources = self.lib.connected();
        let (active, source) = want
            .and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .cloned()
            .ok_or_else(|| ToggleError::CannotPlay("no server is connected".into()))?;
        let (c, sid) = (self.lib.catalogue(), active.cfg.id.clone());
        let found = tokio::task::spawn_blocking(move || c.line_track_ids(&sid, &entries))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))??;
        let unknown = found.iter().filter(|f| f.is_none()).count();
        Ok((active, source, found.into_iter().flatten().collect(), unknown))
    }

    /// After an edit: the playlist's copies follow (the shared one at once, the others with the next pass). Only the
    /// first administrator's are kept in step with other servers (decision 0051).
    fn tell_sources(&self, store: &Store, changes: Vec<(String, PlaylistChange)>) {
        if store.account().is_none() {
            self.sync.changed(changes.into_iter().map(|(name, _)| name).collect());
        }
    }

    /// The asking account's own playlists.
    fn own_store(&self) -> Result<Arc<Store>, ToggleError> {
        self.lib.store_for(asking_account(self.served.as_deref()).as_deref())
    }

    /// The asking account's own crates.
    fn own_crates(&self) -> Result<Arc<crates::CrateStore>, ToggleError> {
        self.crate_stores.of(asking_account(self.served.as_deref()).as_deref())
    }

    /// 0 when there is no Music folder.
    pub async fn count(&self) -> Result<usize> {
        Ok(self.view(None).await?.len())
    }

    /// One's own playlists' repairs.
    pub async fn repairs(&self) -> Result<Repairs> {
        self.own_store()?.repairs().await
    }

    /// Apply the repairs the client chose (`None` = every safe one, or only those of the playlists `within`). The store
    /// only accepts pairs it currently offers. The sources are then told about each changed playlist. Returns the
    /// changed playlists and the undo token (one for all of them).
    pub async fn apply_repairs(&self, only: Option<Repair>, within: Option<&[String]>) -> Result<(Vec<String>, Option<String>), ToggleError> {
        let store = self.own_store()?;
        let wanted = match only {
            Some(one) => vec![one],
            None => safe_within(store.repairs().await?.safe, within),
        };
        let edits = store.apply_repairs(&wanted).await?;
        let changed: Vec<String> = edits.iter().map(|e| e.id.clone()).collect();
        self.tell_sources(&store, changed.iter().map(|n| (n.clone(), PlaylistChange::Edited)).collect());
        Ok((changed, self.keep_texts(edits)))
    }

    /// A missing line pointed at a track found for it (Health's missing tracks; `Store::relink` checks both): the
    /// playlists that changed and the undo token, as a repair's.
    pub async fn relink(&self, one: Repair) -> Result<(Vec<String>, Option<String>), ToggleError> {
        let store = self.own_store()?;
        let edits = store.relink(&one).await?;
        let changed: Vec<String> = edits.iter().map(|e| e.id.clone()).collect();
        self.tell_sources(&store, changed.iter().map(|n| (n.clone(), PlaylistChange::Edited)).collect());
        Ok((changed, self.keep_texts(edits)))
    }

    /// Album folders that look like the same release twice, each group compared. Read-only. Track details come
    /// from the first source that knows the folder.
    pub async fn album_duplicates(&self) -> Result<Vec<crate::albums::AlbumGroup>> {
        use crate::albums::*;
        let Some(music) = self.lib.music().map(PathBuf::from) else { return Ok(Vec::new()) };
        let stores = self.lib.all_stores();
        let dir = music.clone();
        let folders: Vec<String> = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
            Ok(std::fs::read_dir(&dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .collect())
        })
        .await??;
        let sources = self.lib.connected();
        let mut out = Vec::new();
        for group in candidate_groups(&folders) {
            let mut albums = Vec::new();
            for folder in &group {
                let (m, f) = (music.clone(), folder.clone());
                let (files, cover) = tokio::task::spawn_blocking(move || read_folder(&m, &f)).await??;
                let mut known = Default::default();
                for (a, s) in &sources {
                    match s.album_details(folder).await {
                        Ok(k) if !k.is_empty() => {
                            known = k;
                            break;
                        }
                        Ok(_) => {}
                        Err(e) => tracing::warn!("source {} has no details for {folder:?}: {e:#}", a.cfg.id),
                    }
                }
                // Every account's playlists that hold it, by their keys.
                let mut lines = Vec::new();
                for store in &stores {
                    lines.extend(store.lines_in_folder(folder).await?.into_iter().map(|(name, n)| (store.key(&name), n)));
                }
                albums.push(build_view(folder, files, cover, &known, lines));
            }
            let comparisons = albums.iter().skip(1).map(|b| compare(&albums[0], b)).collect();
            let suggestion = suggest(&albums);
            out.push(AlbumGroup { albums, comparisons, suggestion });
        }
        Ok(out)
    }

    /// A new playlist is Shared, as playlists were before sync links (decision 0014). Returns the undo token.
    pub async fn create(&self, name: &str) -> Result<Option<String>, ToggleError> {
        let store = self.own_store()?;
        store.create(name).await?;
        if store.account().is_none() {
            self.sync.new_playlist(name).await?;
        }
        Ok(self.keep(Change::Created { playlists: vec![(name.to_string(), fingerprint(""))] }))
    }

    /// A playlist edited in its view (decision 0023): its lines put in a new order or left out, as one write. Only the
    /// lines it has (`expected`, as the view showed them) can be kept; it is refused when the playlist changed since.
    /// Returns the undo token (none when nothing changed).
    pub async fn edit(&self, id: &str, expected: &[String], entries: &[String]) -> Result<Option<String>, ToggleError> {
        if !is_rearrangement(expected, entries) {
            return Err(ToggleError::Refused("An edit can only reorder or remove the playlist's own lines.".into()));
        }
        let (store, name) = self.own(id)?;
        let Some(edit) = store.replace_entries_if(&name, expected, entries).await? else {
            return Err(ToggleError::Refused("The playlist changed meanwhile: look at it again.".into()));
        };
        self.tell_sources(&store, vec![(name, PlaylistChange::Edited)]);
        Ok(self.keep_texts(vec![edit]))
    }

    /// Take the repeats out of one playlist, the first copy of each track staying: the user's "Remove repeats"
    /// (decision 0025, amended 2026-10-16). Returns the undo token, none when it held none.
    pub async fn remove_repeats(&self, id: &str) -> Result<Option<String>, ToggleError> {
        let (store, name) = self.own(id)?;
        let Some(edit) = store.remove_repeats(&name).await? else { return Ok(None) };
        self.tell_sources(&store, vec![(name, PlaylistChange::Edited)]);
        Ok(self.keep_texts(vec![edit]))
    }

    /// Add these tracks (ids on `source_id`, in order) at a playlist's end, whether or not it holds them already: the
    /// page asked and the user chose Add again (decision 0025, amended 2026-10-16). With `only_new` (0.9.4: many ticked
    /// tracks into a playlist in one call) those it holds already, and repeats among them, are left out instead. Tracks
    /// outside the library are left out. Returns how many were added and the undo token (none when nothing was).
    pub async fn append_tracks(&self, id: &str, source_id: &str, ids: &[String], only_new: bool) -> Result<(usize, Option<String>), ToggleError> {
        // A Bandcamp preview is never in a playlist (decision 0071): it is bought on Bandcamp.
        if source_id == crate::bandcamp::SOURCE {
            return Err(ToggleError::Refused(crate::bandcamp::NOT_KEPT.into()));
        }
        let gone = || ToggleError::Refused("Those tracks are not on a connected server.".into());
        let (a, s) = self.lib.connected().into_iter().find(|(a, _)| a.cfg.id == source_id).ok_or_else(gone)?;
        let (store, name) = self.own(id)?;
        let folders = self.lib.folders();
        let mut entries = Vec::new();
        for track in ids {
            if !crate::covers::valid_item_id(track) {
                continue;
            }
            if let Ok(t) = s.track(track).await
                && let Some(e) = a.line(&folders, &t)
            {
                entries.push(e);
            }
        }
        if entries.is_empty() {
            return Err(ToggleError::NotInLibrary);
        }
        if only_new {
            let mut seen: HashSet<String> = store.entries(&name).await?.into_iter().collect();
            entries.retain(|e| seen.insert(e.clone()));
            if entries.is_empty() {
                return Ok((0, None));
            }
        }
        let edit = store.append(&name, &entries).await?;
        self.tell_sources(&store, vec![(name, PlaylistChange::Edited)]);
        Ok((entries.len(), self.keep_texts(vec![edit])))
    }

    /// The asking account's playlists and the ones others share, by id, with their lines and missing-track counts
    /// (the OpenSubsonic playlists, decision 0030).
    pub async fn snapshot(&self) -> Result<Vec<(String, Vec<String>, usize)>, ToggleError> {
        let Ok(own) = self.own_store() else { return Ok(Vec::new()) };
        let mut out = own.snapshot().await?;
        let shared = self.shared_keys();
        for store in self.others(&own) {
            if !shared.iter().any(|k| store.name_of_key(k).is_some()) {
                continue;
            }
            let owner = owner_name(self.served.as_deref(), store.account());
            out.extend(store.snapshot().await?.into_iter().filter(|(n, _, _)| shared.contains(&store.key(n))).map(|(n, l, d)| (format!("{owner}/{n}"), l, d)));
        }
        Ok(out)
    }

    /// Every account's playlists' lines, for what draws on all of them (Radio's sense of what goes together).
    pub async fn snapshot_all(&self) -> Result<Vec<(String, Vec<String>, usize)>, ToggleError> {
        let mut out = Vec::new();
        for store in self.lib.all_stores() {
            out.extend(store.snapshot().await?.into_iter().map(|(n, l, d)| (store.key(&n), l, d)));
        }
        Ok(out)
    }

    /// The asking account's taste and what it avoids (app::taste; Insights › Listening › Profile): from its favourites,
    /// its own playlists, its plays and skips (the log's, none while it slept), its exclusions; only tracks of the
    /// libraries switched on (`without`: those off, Insights' switches), over the whole library's rarity. `period`
    /// (Insights' one) narrows the plays and skips; favourites, playlists and exclusions have no time.
    /// `off`: sources switched off (`taste::SOURCES`).
    pub async fn taste(&self, want: Option<&str>, without: &[String], period: super::history::Period, off: &[String]) -> Result<super::taste::TasteView, ToggleError> {
        let off: Vec<&'static str> = super::taste::SOURCES.into_iter().filter(|s| off.iter().any(|o| o == s)).collect();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let since = period.since(now).unwrap_or(0);
        let user = asking_user(self.served.as_deref()).ok_or_else(|| ToggleError::Refused("A taste is an account's own: sign in.".into()))?;
        let serve = self.served.clone().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let lib = self.crate_library(want, false, false, without).await?;
        let excluded = self.own_crates()?.excluded().await?;
        tokio::task::spawn_blocking(move || -> Result<super::taste::TasteView, ToggleError> {
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
            let excluded = excluded.iter().filter_map(|line| alike.place_of_line(line).filter(|&i| lib.on(i))).collect();
            let mut skips: HashMap<usize, u32> = HashMap::new();
            for l in heard.iter().filter(|l| l.ended == crate::userdata::Ended::Skipped && !l.counted) {
                if let Some(i) = place(&l.song) {
                    *skips.entry(i).or_default() += 1;
                }
            }
            let sources = super::taste::Sources { favourites, playlists, plays, excluded, skipped: skips.into_iter().collect() };
            Ok(super::taste::taste(alike, &sources, &off))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
    }

    /// Of these albums (Cratarr's own), those the asking account has heard a good part of (`HEARD_SHARE` of their length,
    /// by their tracks' own plays times their lengths, none heard while following, `album_heard`): what New for you
    /// leaves out (the owner, 2026-10-23).
    pub async fn heard_albums(&self, ids: Vec<String>) -> Result<Vec<String>, ToggleError> {
        let user = asking_user(self.served.as_deref()).ok_or_else(|| ToggleError::Refused("What was heard is an account's own: sign in.".into()))?;
        let serve = self.served.clone().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        tokio::task::spawn_blocking(move || -> Result<Vec<String>, ToggleError> {
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
        let user = asking_user(self.served.as_deref());
        let serve = self.served.clone().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let artists = self.artists_out().await?;
        tokio::task::spawn_blocking(move || -> Result<Vec<String>, ToggleError> {
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
    /// most like the taste (`taste::profiles`, every source) and furthest from the listening, never played furthest. Both
    /// read one's own plays, none heard while following (the owner, 2026-10-30).
    pub async fn digging(&self, want: Option<&str>, without: &[String]) -> Result<Digging, ToggleError> {
        let user = asking_user(self.served.as_deref()).ok_or_else(|| ToggleError::Refused("Home's looks back are an account's own: sign in.".into()))?;
        let serve = self.served.clone().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let lib = self.crate_library(want, false, false, without).await?;
        let excluded = self.own_crates()?.excluded().await?;
        // Neither shows what credits an artist excluded everywhere (decision 0083): a track by its credits, an album by
        // its album artist.
        let artists = self.artists_out().await?;
        tokio::task::spawn_blocking(move || -> Result<Digging, ToggleError> {
            let alike = lib.alike();
            let place = |song: &str| crate::serve::ids::track_of(song).and_then(|t| alike.place_of_track(t)).filter(|&i| lib.on(i));
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
            // Grave digging: the taste as Insights' Profile has it, every source; the albums played before, each as one
            // profile against it; of the closest, the ones unplayed longest.
            let taste = taste_in(&lib, ud, &user, &excluded)?;
            // Every album of the libraries switched on, its tracks' places; its last play, and how much its artist is
            // played (of the most played artist's plays).
            let songs = serve.library.all_song_ids()?;
            let mut by_album: HashMap<String, Vec<(usize, f64)>> = HashMap::new();
            for (song, album) in serve.library.albums_of(&songs)? {
                if let Some(i) = place(&song) {
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
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
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
        let user = asking_user(self.served.as_deref()).ok_or_else(|| ToggleError::Refused("Home's moment is an account's own: sign in.".into()))?;
        let serve = self.served.clone().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        if let Some(tz) = tz {
            super::moment::page_tz(tz);
        }
        let m = super::moment::now();
        let lib = self.crate_library(want, false, false, without).await?;
        let artists_out = self.artists_out().await?;
        tokio::task::spawn_blocking(move || -> Result<MomentView, ToggleError> {
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

    /// An artist's Top recommendations (the owner, 2026-10-23: "a list of tracks which follows taste", in place of
    /// their most played): their tracks of the libraries switched on, those that fit the asking account's taste best
    /// first (`taste_in`, as Grave digging's), at most `count`, as song ids.
    pub async fn artist_picks(&self, name: &str, without: &[String], count: usize) -> Result<Vec<String>, ToggleError> {
        let user = asking_user(self.served.as_deref()).ok_or_else(|| ToggleError::Refused("Recommendations are an account's own: sign in.".into()))?;
        let serve = self.served.clone().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let lib = self.crate_library(None, false, false, without).await?;
        let excluded = self.own_crates()?.excluded().await?;
        let name = name.to_string();
        tokio::task::spawn_blocking(move || -> Result<Vec<String>, ToggleError> {
            let taste = taste_in(&lib, &serve.userdata, &user, &excluded)?;
            let alike = lib.alike();
            let mut fit: Vec<(String, f64)> = lib
                .files
                .iter()
                .enumerate()
                .filter(|(i, f)| lib.on(*i) && f.artists.iter().any(|a| a.eq_ignore_ascii_case(&name)))
                .filter_map(|(i, f)| Some((crate::serve::ids::song(f.track?), alike.compare(&taste, &alike.profile(&[(i, 1.0)]), lib.by())?)))
                .collect();
            fit.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            Ok(fit.into_iter().take(count).map(|(id, _)| id).collect())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
    }

    /// Playlists like a playlist (one's own or a shared one; the owner, 2026-10-23: same kind under each list), among
    /// those the asking account sees, the closest first, with how alike (0 to 100): radio::similar_lists.
    pub async fn similar_playlists(&self, id: &str, count: usize) -> Result<Vec<(String, f64)>, ToggleError> {
        let Some(serve) = self.served.clone() else { return Ok(Vec::new()) };
        let lists = self.visible_lines().await?;
        let id = id.to_string();
        let (seed, candidates) = tokio::task::spawn_blocking(move || {
            let cat = serve.catalogue();
            let songs = |lines: &[String]| lines.iter().filter_map(|l| crate::serve::playlists::song_of_line(cat, l)).collect::<Vec<_>>();
            let mut seed = Vec::new();
            let mut candidates = Vec::new();
            for (list, _, lines) in &lists {
                if *list == id {
                    seed = songs(lines);
                } else {
                    candidates.push((list.clone(), songs(lines)));
                }
            }
            (seed, candidates)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        super::radio::similar_lists(seed, candidates, count).await
    }

    /// One playlist's lines (one's own, or a shared one).
    pub async fn entries(&self, id: &str) -> Result<Vec<String>, ToggleError> {
        let r = self.reach(id)?;
        Ok(r.store.entries(&r.name).await?)
    }

    /// A playlist's lines replaced as one write, as `edit`, but the new list may also hold lines it did not have (an
    /// OpenSubsonic client adding songs, decision 0030). The caller makes the lines from the catalogue, never from what
    /// a client sent. Refused when the playlist changed since `expected`. Repeats are kept as the client wrote them
    /// (decision 0025, amended 2026-10-16). Returns the undo token (none when nothing changed).
    pub async fn set_entries(&self, id: &str, expected: &[String], entries: &[String]) -> Result<Option<String>, ToggleError> {
        let (store, name) = self.own(id)?;
        let Some(edit) = store.replace_entries_if(&name, expected, entries).await? else {
            return Err(ToggleError::Refused("The playlist changed meanwhile: look at it again.".into()));
        };
        self.tell_sources(&store, vec![(name, PlaylistChange::Edited)]);
        Ok(self.keep_texts(vec![edit]))
    }

    /// The server (`want`, else the first connected) a playlist favourite is kept on.
    fn favourites_source(&self, want: Option<&str>) -> Result<Arc<dyn source::Source>, ToggleError> {
        let sources = self.lib.connected();
        want.and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .map(|(_, s)| s.clone())
            .ok_or_else(|| ToggleError::Refused("No server is connected.".into()))
    }

    /// Which playlists (by name) are favourites, as Finamp's playlist hearts (decision 0023).
    pub async fn favorites(&self, want: Option<&str>) -> Result<std::collections::BTreeMap<String, bool>, ToggleError> {
        let s = self.favourites_source(want)?;
        s.playlist_favorites().await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// Makes a playlist a favourite or not (every copy on the server).
    pub async fn set_favorite(&self, id: &str, want: Option<&str>, on: bool) -> Result<(), ToggleError> {
        let s = self.favourites_source(want)?;
        s.set_playlist_favorite(id, on).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// A new playlist holding these entries, in order (a playlist made from the queue, decision 0019). Returns the
    /// undo token.
    pub async fn create_with(&self, name: &str, entries: &[String]) -> Result<Option<String>, ToggleError> {
        let store = self.own_store()?;
        store.create(name).await?;
        // Filled only while still empty: never over an edit made since it was created.
        let Some(filled) = store.replace_entries_if(name, &[], entries).await? else {
            return Err(ToggleError::Refused("The new playlist changed while it was being filled.".into()));
        };
        if store.account().is_none() {
            self.sync.new_playlist(name).await?;
        }
        Ok(self.keep(Change::Created { playlists: vec![(name.to_string(), fingerprint(&filled.after))] }))
    }

    /// Returns the undo token.
    pub async fn rename(&self, from: &str, to: &str) -> Result<Option<String>, ToggleError> {
        let (store, from) = self.own(from)?;
        self.rename_unkept(&store, &from, to).await?;
        Ok(self.keep(Change::Renamed { from, to: to.to_string() }))
    }

    async fn rename_unkept(&self, store: &Store, from: &str, to: &str) -> Result<(), ToggleError> {
        if store.account().is_none() {
            // Through the sync, so the server copies keep following the playlist (decision 0012).
            self.sync.rename(from, to, store.rename(from, to)).await?;
        } else {
            store.rename(from, to).await?;
        }
        self.tell_sources(store, vec![(from.to_string(), PlaylistChange::Deleted), (to.to_string(), PlaylistChange::Created)]);
        // Its suggestions keep what was turned down for it (`crates::playlist_crate`'s left-out list).
        let crates = self.own_crates()?;
        let (old, new) = (format!("{}{}{from}", crates::LIKE, crates::LIST), format!("{}{}{to}", crates::LIKE, crates::LIST));
        let left = crates.left_out(&old).await?;
        if !left.is_empty() {
            crates.set_left_out(&new, left).await?;
            crates.set_left_out(&old, crates::LeftOut::default()).await?;
        }
        let fit = crates.fit(&old).await?;
        if !fit.is_empty() {
            crates.set_fit(&new, fit).await?;
            crates.set_fit(&old, crates::Fit::default()).await?;
        }
        // Subsonic clients keep a playlist's id (`pl-` and its name) across a rename (decision 0030); it stays shared.
        if let Some(serve) = self.served.clone() {
            serve.playlist_renamed(&store.key(from), &store.key(to));
        }
        Ok(())
    }

    /// The store keeps it aside; its undo brings it back with the links it had (unshared). Returns the undo token.
    pub async fn delete(&self, id: &str) -> Result<Option<String>, ToggleError> {
        let (store, name) = self.own(id)?;
        store.delete(&name).await?;
        if let Some(serve) = self.served.clone()
            && let Err(e) = serve.userdata.set_playlist_shared(&store.key(&name), false)
        {
            tracing::warn!("{name}: no longer shared, but that was not kept: {e:#}");
        }
        self.tell_sources(&store, vec![(name.clone(), PlaylistChange::Deleted)]);
        Ok(self.keep(Change::Deleted { playlists: vec![name] }))
    }

    /// Re-check which playlist entries point at missing files (the library changes without the playlists): every
    /// account's.
    pub async fn refresh_dead(&self) {
        for store in self.lib.all_stores() {
            store.refresh_dead().await;
        }
    }
}

/// Best effort, in the background: every connected source hears about changed playlist files, each with the file's
/// path as it sees it.
pub(super) fn tell_sources(lib: &Library, changes: Vec<(String, PlaylistChange)>) {
    let Some(music) = lib.music() else { return };
    for (a, s) in lib.connected() {
        let events: Vec<PlaylistEvent> = changes
            .iter()
            .map(|(name, change)| PlaylistEvent { name: name.clone(), path: a.placement.playlist_path(&music, name), change: *change })
            .collect();
        tokio::spawn(async move {
            if let Err(e) = s.playlists_changed(events).await {
                tracing::warn!("source {}: could not pass on playlist changes: {e:#}", a.cfg.id);
            }
        });
    }
}

/// Where an export finds a track by its server's id: its file wherever the catalogue has it. Never a Bandcamp
/// preview's (decision 0071: played, never downloaded), whatever the catalogue holds: it is left out as missing.
fn export_place(c: &crate::catalogue::Catalogue, source: &str, id: &str) -> Option<(i64, String)> {
    if source == crate::bandcamp::SOURCE {
        return None;
    }
    c.track_of_source(source, id).and_then(|t| c.file_of(t))
}

#[cfg(test)]
mod export_place_tests {
    use super::*;

    #[test]
    fn a_bandcamp_preview_is_never_exported() {
        let root = std::env::temp_dir().join(format!("cratarr-export-place-{}", std::process::id()));
        std::fs::create_dir_all(root.join("A")).unwrap();
        std::fs::write(root.join("A/01.mp3"), "a").unwrap();
        let cat = crate::catalogue::Catalogue::open(None, vec![root.clone()]).unwrap();
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        // Even were a file linked to it.
        let link = [("t1".to_string(), 0, "A/01.mp3".to_string())];
        cat.record_links("jellyfin", "lib", &link, 1).unwrap();
        cat.record_links(crate::bandcamp::SOURCE, "lib", &link, 1).unwrap();
        assert_eq!(export_place(&cat, "jellyfin", "t1"), Some((0, "A/01.mp3".to_string())));
        assert_eq!(export_place(&cat, crate::bandcamp::SOURCE, "t1"), None, "left out as missing");
        assert_eq!(export_place(&cat, crate::bandcamp::SOURCE, "https://x.bandcamp.com/album/y#1"), None);
        std::fs::remove_dir_all(&root).ok();
    }
}

#[cfg(test)]
mod favourites_tests {
    use super::*;

    #[tokio::test]
    async fn the_favourites_heard_are_kept_for_the_next_start_and_asked_again_at_once() {
        let dir = std::env::temp_dir().join(format!("cratarr-fav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("favourites.json");
        let first = FavouritesKept::load(Some(file.clone()), None);
        assert!(first.kept("jellyfin").is_none(), "nothing kept yet");
        first.save("jellyfin", None, &Arc::new(["A/1.flac".to_string(), "B/2.mp3".to_string()].into_iter().collect())).await;

        let next = FavouritesKept::load(Some(file), None);
        let k = next.kept("jellyfin").expect("kept from the last run");
        assert_eq!(k.lines.as_deref().map(|l| l.len()), Some(2));
        assert!(k.at.elapsed() >= FAVOURITES_KEEP, "due to be asked again");
        assert!(next.kept("other").is_none(), "only for the server it came from");
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod left_out_tests {
    use super::*;

    #[test]
    fn a_crates_lines_left_out_are_its_tracks_its_albums_tracks_now_and_those_excluded_everywhere() {
        let left = crates::LeftOut { lines: vec!["a.flac".into()], albums: vec!["al-1".into(), "al-gone".into()] };
        let album_tracks: HashMap<String, Vec<String>> = [("al-1".to_string(), vec!["x/1.flac".to_string(), "x/2.flac".to_string()])].into();
        assert_eq!(left_lines(&left, &album_tracks, &["e.flac".to_string()]), ["a.flac", "x/1.flac", "x/2.flac", "e.flac"]);
        assert!(left_lines(&crates::LeftOut::default(), &HashMap::new(), &[]).is_empty());
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
        let f = |line: &str, folder: i64, genre: &str| crates::Facts { rel: line.into(), folder, genres: vec![genre.into()], ..Default::default() };
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
        }
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
        assert_eq!(crates::axes(library(&[1]).files(), &[]).iter().find(|a| a.key == "genre").unwrap().values, ["Dub", "Rock"], "the axes of what is on");
        let off = library(&[1]);
        assert_eq!((0..4).map(|i| off.on(i)).collect::<Vec<_>>(), [false, true, false, true], "the taste's places: Incoming off");
    }

    #[test]
    fn an_incoming_line_is_there_when_its_file_is() {
        let root = std::env::temp_dir().join(format!("cratarr-lines-{}", std::process::id()));
        let (m, inc) = (root.join("Music"), root.join("in"));
        std::fs::create_dir_all(m.join("A")).unwrap();
        std::fs::create_dir_all(inc.join("X")).unwrap();
        std::fs::write(m.join("A/1.flac"), "a").unwrap();
        std::fs::write(inc.join("X/1.flac"), "x").unwrap();
        let cat = crate::catalogue::Catalogue::open(None, vec![m.clone(), inc.clone()]).unwrap();
        assert!(line_exists(&cat, Some(m.as_path()), "A/1.flac"));
        assert!(line_exists(&cat, Some(m.as_path()), "../in/X/1.flac"), "an Incoming track's line");
        assert!(!line_exists(&cat, Some(m.as_path()), "../in/X/2.flac"));
        assert!(!line_exists(&cat, Some(m.as_path()), "../elsewhere/X/1.flac"), "in no folder of the catalogue");
        std::fs::remove_dir_all(&root).ok();
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
        assert_eq!(CrateLook::default().order, CrateOrder::Own);
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
