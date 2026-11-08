//! The Filter sheet's engine (decision 0061): a filter is a crate's conditions, unsaved, on the list in view. One
//! question for every list: given the conditions and what the list is, which of its items pass, how many of how many,
//! and the axes with their values and counts in this list (the sheet's chips, most common first; the ranges' span; the
//! tempo spread). Items that hold tracks (the Library's albums, the playlists) are tested with each condition's
//! quantifier, any or all of their tracks (`crates::passes`); in a list of tracks each track is an item of one.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{App, ToggleError};
use crate::crates::{self, Condition, Facts, Use, Value};

/// What the list in view is.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum List {
    /// The Library's albums on the server asked for, of the libraries switched on (`without`).
    Library,
    /// The Library's artists (the filter API, 2026-10-29): each album artist of the libraries switched on, with the
    /// albums they are an artist of.
    Artists,
    /// The Playlists page's list: the account's own playlists and those shared with it.
    Playlists,
    /// A playlist's tracks.
    Playlist { id: String },
    /// A crate's tracks as it is looked at: its toggles as given, else as saved; `without` from the request.
    Crate {
        id: String,
        #[serde(default)]
        favourites: Option<crates::Favourites>,
        #[serde(default)]
        favourite_albums: Option<crates::Favourites>,
    },
    /// An album's tracks, or an artist's, on their own server.
    Album { source: String, id: String },
    Artist { source: String, name: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub conditions: Vec<crates::Condition>,
    pub list: List,
    /// The libraries switched off (`source/library` keys): the Library's albums and a crate's tracks leave theirs out;
    /// a playlist, an album, an artist and the playlists are what they hold.
    #[serde(default)]
    pub without: Vec<String>,
    /// The list's own switches that are on, as the page's SWITCHES has them (`switches_of`); one the list has not is
    /// refused.
    #[serde(default)]
    pub switches: Vec<String>,
    /// What keeps each item out too (`Filtered::blocked`; decision 0095, amended 2026-11-02), asked when what the page
    /// shows is empty. Not for a crate: its rows say it (`CrateLook::explain`).
    #[serde(default)]
    pub explain: bool,
}

/// The switches each list has, as the page's SWITCHES (lib/pageView) has them, each with one meaning (the filter API,
/// 2026-10-29: worked out here per list, beside its conditions, not by the page):
/// - the albums: Favourites only, the album hearted; Unplayed only, no track of it played; 4 stars and up, the album's
///   own stars (each record under every type it has: 0.13, the owner, the types as layers, no ranking);
/// - the artists: Favourites only, the artist hearted; Unplayed only, no track of theirs played; 4 stars and up, as
///   their conditions, one of their albums;
/// - the playlists: Favourites only, the playlist hearted; Mine only, the account's own;
/// - a playlist's tracks: Favourites only, the track hearted; Unplayed only, the track never played; Hide missing
///   tracks, rows without a file left out.
///
/// Played is the plays log's, the asking account's (`uses_of`), everywhere.
pub fn switches_of(list: &List) -> &'static [&'static str] {
    match list {
        List::Library => &["favourites", "unplayed", "topRated"],
        List::Artists => &["favourites", "unplayed", "topRated"],
        List::Playlists => &["favourites", "mine"],
        List::Playlist { .. } => &["favourites", "unplayed", "missing"],
        List::Crate { .. } | List::Album { .. } | List::Artist { .. } => &[],
    }
}

/// A switch's word, as the page's SWITCHES has it: its chip's (decision 0095).
fn switch_label(key: &str) -> &str {
    match key {
        "favourites" => "Favourites only",
        "unplayed" => "Unplayed only",
        "topRated" => "4 stars and up",
        "mine" => "Mine only",
        "missing" => "Hide missing tracks",
        other => other,
    }
}

/// What a list's items are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Items {
    Albums,
    Artists,
    Playlists,
    Tracks,
}

/// The answer: the items that pass by their keys (an album's id, a playlist's id, a track row's `entry`; none for a
/// crate, whose rows come filtered: `CrateLook::filter`), how many pass of how many, and the list's axes and tempo
/// spread, over all its items (what there is to choose from), not only those that pass.
#[derive(Debug, Serialize)]
pub struct Filtered {
    pub source: String,
    pub items: Items,
    pub keys: Vec<String>,
    pub passed: usize,
    pub total: usize,
    pub axes: Vec<crates::AxisView>,
    pub tempo: crates::TempoBands,
    /// A list of tracks' profile (the owner, 2026-10-23), over all its tracks; none for albums and playlists.
    pub profile: Option<crates::Profile>,
    /// What left everything out, when nothing passes (decision 0095): each condition and switch of the filter, the
    /// Library's libraries switched off. None while something passes, or when nothing would with every cause lifted.
    pub reasons: Vec<crates::Reason>,
    /// How many would pass with every cause lifted; `total` while something passes.
    pub all_back: usize,
    /// What keeps each item out, by its key, when asked (`Request::explain`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<crates::Blocked>,
}

/// A line's facts; none for a line the catalogue has no file for (a missing one, or switched off).
fn facts_of<'a>(by_line: &HashMap<&str, &'a Facts>, none: &'a Facts, line: Option<&str>) -> &'a Facts {
    line.and_then(|l| by_line.get(l).copied()).unwrap_or(none)
}

/// Works a filter out on a list (`want`: the server, else the first connected; an album's or an artist's own), each
/// item judged once (`crates::Verdicts`, decision 0095 amended 2026-11-02): by the exact set of causes failing it. When
/// nothing passes, or `explain` asks, the list is judged from every library too, so what only the libraries switched
/// off hold is judged with them as one more cause; the reasons, what lifting each brings back, all there would be with
/// every cause lifted, and what keeps each item out are all read from those verdicts.
pub async fn filter(app: &Arc<App>, want: Option<&str>, r: Request) -> Result<Filtered, ToggleError> {
    let causes = Causes::of(&r)?;
    let drawn_off = matches!(r.list, List::Library | List::Artists) && !r.without.is_empty();
    let (mut answer, mut verdicts) = work_out(app, want, &r, &causes, true, r.explain || drawn_off).await?;
    let mut named = causes.named.clone();
    if drawn_off && (answer.passed == 0 || r.explain) {
        let mine: Vec<String> = r.without.iter().filter(|k| k.starts_with(&format!("{}/", answer.source))).cloned().collect();
        if !mine.is_empty() {
            let bit = crates::cause_bit(named.len());
            named.push(crates::Reason::libraries_off(&mine, 0).into());
            let (every, judged) = work_out(app, want, &Request { without: Vec::new(), ..r.clone() }, &causes, false, true).await?;
            verdicts.add_off(&answer.keys, &judged, &every.keys, bit);
        }
    }
    if r.explain && !matches!(r.list, List::Crate { .. }) {
        answer.blocked = Some(verdicts.blocked(named.clone()));
    }
    answer.all_back = answer.total;
    if answer.passed == 0 {
        answer.reasons = verdicts.reasons(&named);
        answer.all_back = verdicts.all();
    }
    Ok(answer)
}

/// The filter's causes on a list (decision 0095), as the verdicts' bits number them: each condition that is on and
/// asks something, then each switch that is on (the libraries switched off are one more, last: `filter` adds it).
struct Causes {
    named: Vec<crates::BlockCause>,
    /// Each bit, as the request has its conditions that ask something (`Condition::complete`); 0 for one that is off.
    conditions: Vec<u64>,
    switches: Vec<String>,
}

impl Causes {
    fn of(r: &Request) -> Result<Self, ToggleError> {
        let mut named: Vec<crates::BlockCause> = Vec::new();
        let mut conditions = Vec::new();
        for c in r.conditions.iter().filter(|c| c.complete()) {
            conditions.push(match c.enabled {
                true => {
                    named.push(crates::Reason::new(crates::Why::Filter, Some(c.axis.clone()), crates::axis_label(&c.axis).unwrap_or(c.axis.as_str()), 0).into());
                    crates::cause_bit(named.len() - 1)
                }
                false => 0,
            });
        }
        named.extend(r.switches.iter().map(|k| crates::Reason::new(crates::Why::Filter, Some(k.clone()), switch_label(k), 0).into()));
        // One bit is kept for the libraries switched off.
        if named.len() >= crates::MAX_CAUSES {
            return Err(ToggleError::Refused(format!("A filter asks {} things at most.", crates::MAX_CAUSES - 1)));
        }
        Ok(Self { named, conditions, switches: r.switches.clone() })
    }

    /// A switch's bit; 0 while it is off.
    fn switch(&self, k: &str) -> u64 {
        let first = self.named.len() - self.switches.len();
        self.switches.iter().position(|s| s == k).map_or(0, |j| crates::cause_bit(first + j))
    }
}

/// `bit` unless `ok`.
fn unless(ok: bool, bit: u64) -> u64 {
    if ok { 0 } else { bit }
}

/// The bits of the conditions an item's tracks fail (each condition with its bit).
fn fails(tracks: &[&Facts], conditions: &[(Condition, u64)], w: &crates::World) -> u64 {
    conditions.iter().filter(|(c, _)| !crates::passes(tracks, std::slice::from_ref(c), w)).fold(0, |set, (_, bit)| set | bit)
}

/// The filter's answer on a list (its axes, tempo spread and profile counted when `full`), and the verdict on each of
/// its items (their keys kept with `keys`).
async fn work_out(app: &Arc<App>, want: Option<&str>, r: &Request, causes: &Causes, full: bool, keys: bool) -> Result<(Filtered, crates::Verdicts), ToggleError> {
    if let Some(c) = r.conditions.iter().find(|c| !crates::is_axis(&c.axis)) {
        return Err(ToggleError::Refused(format!("Unknown axis: {}.", c.axis)));
    }
    let known = switches_of(&r.list);
    if let Some(k) = r.switches.iter().find(|k| !known.contains(&k.as_str())) {
        return Err(ToggleError::Refused(format!("Unknown switch: {k}.")));
    }
    let on = |k: &str| r.switches.iter().any(|s| s == k);
    let bit = |k: &str| causes.switch(k);
    // Rows that ask nothing yet are left out: not sent on, not read for (Condition::complete).
    let asked: Vec<Condition> = r.conditions.iter().filter(|c| c.complete()).cloned().collect();
    // A switch on hearts asks for them to be read, as a condition on them would: an album's, a track's.
    let hearts = match &r.list {
        List::Library if on("favourites") => Some(Condition::new("favourite_album", false, Value::Yes)),
        List::Playlist { .. } if on("favourites") => Some(Condition::new("favourite", false, Value::Yes)),
        _ => None,
    };
    let want = match &r.list {
        List::Album { source, .. } | List::Artist { source, .. } => Some(source.as_str()),
        _ => want,
    };
    // A crate's own conditions, worked out on the same reading of the library as the filter's.
    let shown = match &r.list {
        List::Crate { id, favourites, favourite_albums } => {
            let look = super::CrateLook { favourites: *favourites, favourite_albums: *favourite_albums, ..Default::default() };
            app.playlists.crate_shown(id, &look).await?
        }
        _ => Vec::new(),
    };
    let all: Vec<Condition> = shown.iter().chain(&asked).chain(hearts.iter()).cloned().collect();
    // Home's crates are of every library, whatever is switched off (the owner's one exception, 2026-11-02).
    let drawn = match &r.list {
        List::Crate { id, .. } => !crates::home_crate(id),
        List::Library | List::Artists | List::Artist { .. } => true,
        _ => false,
    };
    let lib = app.playlists.crate_library_for(&all, want, if drawn { &r.without } else { &[] }).await?;
    let by_line = crates::by_line(lib.files());
    let none = Facts::default();
    let uses = lib.uses();
    let one = |key: String, line: Option<&str>| Item::of(key, vec![facts_of(&by_line, &none, line)], 0);
    // A heart's switch is its condition (read above): the album's for an album, the track's for a track.
    let w = lib.world();
    let conditions: Vec<(Condition, u64)> =
        lib.resolved(&asked).into_iter().zip(causes.conditions.iter().copied()).chain(hearts.clone().map(|h| (h, bit("favourites")))).collect();
    // The Library's records are weighed on their types by the record (0.10): its Type rows apart, on each record's
    // types as listed (Classical among them; 0.13: every one, the layers joined as `crates::types_hold` joins them).
    let (kind_rows, conditions): (Vec<(Condition, u64)>, Vec<(Condition, u64)>) = match r.list {
        List::Library => conditions.into_iter().partition(|(c, _)| c.axis == "type"),
        _ => (Vec::new(), conditions),
    };
    let mut record_kinds: Option<Vec<Vec<String>>> = None;
    let (items, grouped): (Items, Vec<Item>) = match &r.list {
        List::Crate { .. } => (Items::Tracks, lib.select(&shown).iter().map(|l| one(String::new(), Some(l.as_str()))).collect()),
        List::Playlist { id } => {
            let rows = app.playlists.entries(id).await?;
            let led = app.playlists.led_entries(id).await?;
            let row = |(l, at): (&String, &String)| {
                let f = facts_of(&by_line, &none, Some(at.as_str()));
                Item::of(l.clone(), vec![f], unless(!on("unplayed") || never_played(f, uses), bit("unplayed")) | unless(!on("missing") || !f.rel.is_empty(), bit("missing")))
            };
            (Items::Tracks, rows.iter().zip(&led).map(row).collect())
        }
        List::Playlists => {
            let lists = app.playlists.snapshot().await?;
            let hearted: Option<std::collections::HashSet<String>> = match on("favourites") {
                true => Some(app.playlists.favorites(Some(&lib.source)).await?.into_iter().filter(|(_, on)| *on).map(|(n, _)| n).collect()),
                false => None,
            };
            let mine: Option<std::collections::HashSet<String>> = match on("mine") {
                true => Some(app.playlists.own_playlist_names().await?.into_iter().collect()),
                false => None,
            };
            let of = |lines: &[String]| lines.iter().map(|l| facts_of(&by_line, &none, Some(l.as_str()))).collect();
            let item = |(id, lines, _): (String, Vec<String>, usize)| {
                let set = unless(hearted.as_ref().is_none_or(|h| h.contains(&id)), bit("favourites")) | unless(mine.as_ref().is_none_or(|m| m.contains(&id)), bit("mine"));
                Item::of(id, of(&lines), set)
            };
            (Items::Playlists, lists.into_iter().map(item).collect())
        }
        List::Album { source, id } => {
            let rows = app.playing.album_view(source, id).await?.rows;
            (Items::Tracks, row_lines(app, source, rows).await.into_iter().map(|(e, l)| one(e, l.as_deref())).collect())
        }
        List::Artist { source, name } => {
            let rows = app.playing.artist_view(source, name, &r.without).await?.rows;
            (Items::Tracks, row_lines(app, source, rows).await.into_iter().map(|(e, l)| one(e, l.as_deref())).collect())
        }
        List::Library | List::Artists => {
            let (listed, _) = library_albums(app, &lib.source, &r.without).await;
            // Cratarr's own albums by their own tracks, wherever their files are (2026-10-29, the owner's "Stellar -
            // Tunnel": two copies in two artists' folders, so the album's folder was the library's root, and the
            // filter found none of its files and left it out of every answer, negated ones too).
            let own = own_album_files(&lib.source, lib.files()).await;
            // Another server's albums by their folder: its tracks are the files whose lines are in its folder's.
            let mut by_dir: HashMap<&str, Vec<&Facts>> = HashMap::new();
            for f in lib.files() {
                for line in f.every_line() {
                    let tracks = by_dir.entry(line.rsplit_once('/').map_or("", |(d, _)| d)).or_default();
                    if !tracks.last().is_some_and(|t| std::ptr::eq(*t, f)) {
                        tracks.push(f);
                    }
                }
            }
            let tracks_of = |a: &Listed| match own.get(&a.id) {
                Some(files) => files.clone(),
                None => a.dir.as_ref().and_then(|d| by_dir.get(d.as_str()).cloned()).unwrap_or_default(),
            };
            // 4 stars and up: the album's own stars, as the account sees the album.
            let stars = match on("topRated") {
                true => Some(app.playing.album_uses(&lib.source).await.map_err(|_| ToggleError::CannotPlay("The albums' stars could not be read.".into()))?),
                false => None,
            };
            let rated = |id: &str| stars.as_ref().is_none_or(|s| s.get(id).is_some_and(|u| u.rating >= TOP_RATED));
            let albums: Vec<AlbumItem> = listed.iter().map(|a| AlbumItem { id: a.id.clone(), artists: a.artists.clone(), tracks: tracks_of(a), rated: rated(&a.id) }).collect();
            if matches!(r.list, List::Library) {
                let unplayed = on("unplayed");
                let kinds: Vec<Vec<String>> = listed.iter().map(|a| a.kinds.clone()).collect();
                let items = albums
                    .iter()
                    .zip(&kinds)
                    .map(|(a, k)| {
                        let set = unless(a.rated, bit("topRated")) | unless(!unplayed || a.tracks.iter().all(|f| never_played(f, uses)), bit("unplayed")) | kinds_fail(&kind_rows, k);
                        Item::of(a.id.clone(), a.tracks.clone(), set)
                    })
                    .collect();
                record_kinds = Some(kinds);
                (Items::Albums, items)
            } else {
                let hearted: Option<std::collections::HashSet<String>> = match on("favourites") {
                    true => Some(app.playing.favorite_artists(&lib.source).await?.iter().map(|n| n.to_lowercase()).collect()),
                    false => None,
                };
                let looks = ArtistLook { conditions: &conditions, world: &w, uses, hearted: hearted.as_ref(), unplayed: on("unplayed"), bit: &bit };
                (Items::Artists, artists_of(&lib.source, &albums, &looks))
            }
        }
    };
    let mut verdicts = crates::Verdicts::new(keys);
    let mut keys: Vec<String> = Vec::new();
    for i in &grouped {
        let set = i.set(&conditions, &w);
        if set == 0 {
            keys.push(i.key.clone());
        }
        verdicts.judge(set, || i.key.clone());
    }
    let (source, total, passed) = (lib.source.clone(), grouped.len(), verdicts.passed);
    if !full {
        let answer = Filtered { source, items, keys, passed, total, axes: Vec::new(), tempo: Default::default(), profile: None, reasons: Vec::new(), all_back: 0, blocked: None };
        return Ok((answer, verdicts));
    }
    let tracks: Vec<&[&Facts]> = grouped.iter().map(|i| i.tracks.as_slice()).collect();
    let mut axes = crates::list_axes(&tracks, &app.playlists.own_playlist_names().await?, Some(lib.membership()), lib.uses(), super::moment::now_secs());
    // The Library's Type axis counts records by their types (a record under each it has: the counts overlap).
    if let Some(kinds) = &record_kinds
        && let Some(axis) = axes.iter_mut().find(|a| a.key == "type")
    {
        kind_axis(axis, kinds);
    }
    let tempo = crates::tempo_bands(tracks.iter().flat_map(|t| t.iter().map(|f| f.bpm)));
    let profile = (items == Items::Tracks).then(|| {
        let mine: Vec<&Facts> = tracks.iter().flat_map(|t| t.iter().copied()).filter(|f| !f.rel.is_empty()).collect();
        let library: Vec<&Facts> = lib.files().collect();
        crates::profile(&mine, &library)
    });
    // A crate's rows are paged on the server: they come filtered there, so its thousands of lines are not sent here.
    let keys = if matches!(r.list, List::Crate { .. }) { Vec::new() } else { keys };
    let answer = Filtered { source, items, keys, passed, total, axes, tempo, profile, reasons: Vec::new(), all_back: 0, blocked: None };
    Ok((answer, verdicts))
}

/// Cratarr's own albums' files (`source` its own library's), by each album's id: its tracks', through the library's
/// songs; none for another server's (an album there is its folder).
async fn own_album_files<'a>(source: &str, files: impl Iterator<Item = &'a Facts>) -> HashMap<String, Vec<&'a Facts>> {
    if source != crate::source::cratarr::KIND.id {
        return HashMap::new();
    }
    let Some(served) = crate::serve::instance() else { return HashMap::new() };
    let Ok(Ok(album_of)) = tokio::task::spawn_blocking(move || served.library.song_album_map()).await else { return HashMap::new() };
    by_album(files, &album_of)
}

/// The files of each album, by its id (`album_of`: a song's album, by the song's id): each file with a catalogue track
/// goes to its song's album.
fn by_album<'a>(files: impl Iterator<Item = &'a Facts>, album_of: &HashMap<String, String>) -> HashMap<String, Vec<&'a Facts>> {
    let mut out: HashMap<String, Vec<&Facts>> = HashMap::new();
    for f in files {
        if let Some(album) = f.track.and_then(|t| album_of.get(&crate::serve::ids::song(t))) {
            out.entry(album.clone()).or_default().push(f);
        }
    }
    out
}

/// An album's or an artist's rows by their `entry`, each with its file's line (the catalogue's link of the server's
/// id), none for one it cannot place.
async fn row_lines(app: &Arc<App>, source: &str, rows: Vec<super::playlists::PlaylistRow>) -> Vec<(String, Option<String>)> {
    let (c, sid, ids) = (app.library.catalogue(), source.to_string(), rows.iter().map(|r| r.id.clone().unwrap_or_default()).collect::<Vec<_>>());
    let lines: Vec<Option<String>> = tokio::task::spawn_blocking(move || {
        ids.iter().map(|id| c.track_of_source(&sid, id).and_then(|t| c.row_line(t))).collect()
    })
    .await
    .unwrap_or_default();
    rows.into_iter().map(|r| r.entry).zip(lines.into_iter().chain(std::iter::repeat(None))).collect()
}

/// An album of the Library as listed: its id, its folder's line (none where the server's path for it is not known),
/// its album artists (as the page's Artists view takes them: its artists, else its artist), its kinds (its release
/// type's and Classical, `classical::kinds_of`).
struct Listed {
    id: String,
    dir: Option<String>,
    artists: Vec<String>,
    kinds: Vec<String>,
}

/// The Library's albums on a server, of the libraries not switched off (as the Library page shows them), each with
/// the line of its folder, as its files' lines begin, and its album artists; and the album artists of each album the
/// libraries switched off leave out (decision 0095: what they hold).
async fn library_albums(app: &Arc<App>, source: &str, without: &[String]) -> (Vec<Listed>, Vec<Vec<String>>) {
    let view = app.albums(Some(source)).await;
    let Some(s) = view.sources.into_iter().find(|s| s.id == source) else { return (Vec::new(), Vec::new()) };
    let roots = app.library.connected().into_iter().find(|(a, _)| a.cfg.id == source).map(|(a, _)| a.placement.roots.clone()).unwrap_or_default();
    let cat = app.library.catalogue();
    let folders = cat.folders();
    let album_artists = |artist: Option<String>, artists: Vec<String>| -> Vec<String> { if artists.is_empty() { artist.into_iter().collect() } else { artists } };
    // On while any copy is: a record's list entry names the copy that plays, and that one can be the library switched off.
    let (on, off): (Vec<_>, Vec<_>) = s.albums.into_iter().partition(|a| {
        let held = a.libraries();
        held.is_empty() || held.iter().any(|l| !without.contains(&format!("{source}/{l}")))
    });
    let listed = on
        .into_iter()
        .map(|a| {
            let dir = a.path.as_deref().and_then(|p| folder_of(&roots, &folders, p)).and_then(|(i, rel)| cat.line_of(i, &rel));
            let kinds = super::classical::kinds_of(&a.release);
            Listed { id: a.id, dir, artists: album_artists(a.artist, a.artists), kinds }
        })
        .collect();
    (listed, off.into_iter().map(|a| album_artists(a.artist, a.artists)).collect())
}

/// 4 stars and up.
const TOP_RATED: u8 = 4;

/// Whether a record's kinds (as weighed: its primary view, or all) pass the Type rows, as a condition on a value axis
/// holds (`crates::holds`): any of the values, or none of them negated; a value the axis does not take never holds.
/// The bits of the Type rows a record's kinds fail (each row with its bit).
fn kinds_fail(rows: &[(Condition, u64)], kinds: &[String]) -> u64 {
    rows.iter().filter(|(c, _)| !kinds_pass(std::slice::from_ref(c), kinds)).fold(0, |set, (_, bit)| set | bit)
}

fn kinds_pass(rows: &[Condition], kinds: &[String]) -> bool {
    rows.iter().filter(|c| c.enabled && c.complete()).all(|c| {
        let matched = match &c.value {
            Value::AnyOf { values } => crates::types_hold(values, kinds),
            _ => false,
        };
        matched != c.not
    })
}

/// The Type axis over records by their kinds as weighed: each kind with how many records have it, most first.
fn kind_axis(axis: &mut crates::AxisView, kinds: &[Vec<String>]) {
    let mut seen: Vec<(String, usize)> = Vec::new();
    for ks in kinds {
        let mut here: Vec<String> = Vec::new();
        for k in ks {
            let low = k.to_lowercase();
            if here.contains(&low) {
                continue;
            }
            here.push(low.clone());
            match seen.iter_mut().find(|(n, _)| n.to_lowercase() == low) {
                Some(e) => e.1 += 1,
                None => seen.push((k.clone(), 1)),
            }
        }
    }
    seen.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    (axis.values, axis.counts) = seen.into_iter().unzip();
    axis.layers = axis.values.iter().map(|v| crates::type_layer(v)).collect();
    axis.known = kinds.iter().filter(|k| !k.is_empty()).count();
    axis.varies = false;
}

/// An item of a list: the keys it is answered by (an artist by every spelling of their name), its tracks (what its
/// axes count), whether its switches hold, and whether that is its whole answer (an artist, judged by their albums).
struct Item<'a> {
    key: String,
    tracks: Vec<&'a Facts>,
    /// The causes it fails besides the conditions over its tracks (its switches, its types); or, `judged` (an
    /// artist's, album by album), all it fails.
    set: u64,
    judged: bool,
}

impl<'a> Item<'a> {
    fn of(key: String, tracks: Vec<&'a Facts>, set: u64) -> Self {
        Self { key, tracks, set, judged: false }
    }

    /// The causes it fails (none: it passes): its own, and the conditions its tracks fail.
    fn set(&self, conditions: &[(Condition, u64)], w: &crates::World) -> u64 {
        if self.judged { self.set } else { self.set | fails(&self.tracks, conditions, w) }
    }
}

/// A track never played by the asking account (the plays log's: `uses_of`); with no account, nothing is known played.
fn never_played(f: &Facts, uses: Option<&HashMap<String, Use>>) -> bool {
    uses.is_none_or(|u| u.get(&f.rel).is_none_or(|u| u.plays == 0 && u.last.is_none()))
}

/// An album as the Artists view weighs it: its id, its album artists, its tracks, and whether its own stars pass 4
/// stars and up (or that switch is off).
struct AlbumItem<'a> {
    id: String,
    artists: Vec<String>,
    tracks: Vec<&'a Facts>,
    rated: bool,
}

/// How the Artists view looks: the conditions (each with its bit) and their world (an album's), the plays log, the
/// hearted artists (lower case; none: Favourites only is off), Unplayed only, and each switch's bit.
struct ArtistLook<'a, 'w> {
    conditions: &'a [(Condition, u64)],
    world: &'a crates::World<'w>,
    uses: Option<&'a HashMap<String, Use>>,
    hearted: Option<&'a std::collections::HashSet<String>>,
    unplayed: bool,
    bit: &'a dyn Fn(&str) -> u64,
}

/// The Library's artists from its albums (each album counts for each of its album artists): an artist (by name, in any
/// case) passes when one of their albums passes the conditions and 4 stars and up, they are hearted (Favourites only)
/// and none of their tracks was played (Unplayed only) (the page's meaning before, here now). What keeps an artist out
/// is what keeps out their album that comes back with the fewest causes lifted (the first such), and their own
/// switches. Keyed `{source}\u{0}{name in lower case}`, as the page's LibraryArtists keys them.
fn artists_of<'a>(source: &str, albums: &[AlbumItem<'a>], look: &ArtistLook) -> Vec<Item<'a>> {
    let mut order: Vec<String> = Vec::new();
    let mut by: HashMap<String, (Vec<&'a Facts>, Option<u64>)> = HashMap::new();
    for a in albums {
        let set = unless(a.rated, (look.bit)("topRated")) | fails(&a.tracks, look.conditions, look.world);
        for name in &a.artists {
            let low = name.to_lowercase();
            let e = by.entry(low.clone()).or_insert_with(|| {
                order.push(low.clone());
                (Vec::new(), None)
            });
            e.0.extend(a.tracks.iter().copied());
            if e.1.is_none_or(|best| set.count_ones() < best.count_ones()) {
                e.1 = Some(set);
            }
        }
    }
    order
        .into_iter()
        .map(|low| {
            let (tracks, best) = by.remove(&low).unwrap_or_default();
            let hearted = unless(look.hearted.is_none_or(|h| h.contains(&low)), (look.bit)("favourites"));
            let unplayed = unless(!look.unplayed || tracks.iter().all(|f| never_played(f, look.uses)), (look.bit)("unplayed"));
            Item { key: format!("{source}\u{0}{low}"), tracks, set: best.unwrap_or(0) | hearted | unplayed, judged: true }
        })
        .collect()
}

/// Where an album's folder is in the catalogue (folder, its path there), from the server's path for it: the server's
/// root for it exchanged for the folder Cratarr sees there (`Placement::roots`), or the path as it is (Cratarr's own
/// library names its folders as Cratarr sees them).
fn folder_of(roots: &[crate::placement::FolderRoot], folders: &crate::catalogue::InUse, path: &str) -> Option<(i64, String)> {
    let path = path.replace('\\', "/");
    let path = path.trim_end_matches('/');
    let seen = roots
        .iter()
        .find_map(|r| {
            let rest = path.strip_prefix(r.root.trim_end_matches('/'))?;
            rest.starts_with('/').then(|| format!("{}{rest}", r.folder.trim_end_matches('/')))
        })
        .unwrap_or_else(|| path.to_string());
    let seen = std::path::Path::new(&seen);
    folders.iter().find_map(|(i, f)| {
        let rel = seen.strip_prefix(f).ok()?.to_string_lossy().replace('\\', "/");
        (!rel.is_empty()).then_some((i, rel))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_takes_only_its_own_switches() {
        assert_eq!(switches_of(&List::Library), ["favourites", "unplayed", "topRated"]);
        assert_eq!(switches_of(&List::Artists), ["favourites", "unplayed", "topRated"]);
        assert_eq!(switches_of(&List::Playlists), ["favourites", "mine"]);
        assert_eq!(switches_of(&List::Playlist { id: "x".into() }), ["favourites", "unplayed", "missing"]);
        assert!(switches_of(&List::Crate { id: "c".into(), favourites: None, favourite_albums: None }).is_empty());
        let r: Request = serde_json::from_value(serde_json::json!({ "list": { "kind": "artists" }, "switches": ["unplayed"] })).unwrap();
        assert_eq!((r.list, r.switches), (List::Artists, vec!["unplayed".to_string()]));
    }

    #[test]
    fn the_library_s_type_rows_join_within_a_layer_by_or_and_across_by_and() {
        let k = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let records = [k(&["Album"]), k(&["Album", "Classical"]), k(&["Album", "Compilation"]), k(&["EP", "Live"]), k(&["Album", "Live"]), k(&["Broadcast"])];
        let row = |not: bool, values: &[&str]| Condition::new("type", not, Value::AnyOf { values: values.iter().map(|v| v.to_string()).collect() });
        let passing = |rows: &[Condition]| -> Vec<usize> { records.iter().enumerate().filter(|(_, r)| kinds_pass(rows, r)).map(|(n, _)| n).collect() };
        // Every record under each type it has: no ranking.
        assert_eq!(passing(&[row(false, &["Album"])]), [0, 1, 2, 4]);
        assert_eq!(passing(&[row(false, &["Classical"])]), [1]);
        // One layer: or. Two layers: and.
        assert_eq!(passing(&[row(false, &["Album", "EP"])]), [0, 1, 2, 3, 4]);
        assert_eq!(passing(&[row(false, &["Album", "Live"])]), [4], "live albums, not the live EP");
        assert_eq!(passing(&[row(false, &["Album", "EP", "Live"])]), [3, 4]);
        assert_eq!(passing(&[row(false, &["Compilation", "Live"])]), [2, 3, 4]);
        assert_eq!(passing(&[row(false, &["Album", "Classical"])]), [1]);
        // A negated row holds where the row does not; two rows join with and.
        assert_eq!(passing(&[row(true, &["Compilation"])]), [0, 1, 3, 4, 5]);
        assert_eq!(passing(&[row(false, &["Album"]), row(true, &["Compilation"])]), [0, 1, 4]);
        assert_eq!(passing(&[row(false, &["Broadcast"])]), [5]);
        // The Type axis counts each record under every type it has, and says each value's layer.
        let mut axis = crates::list_axes(&[], &[], None, None, 0).into_iter().find(|a| a.key == "type").unwrap();
        kind_axis(&mut axis, &records);
        let at = |v: &str| axis.values.iter().position(|x| x == v).map(|i| (axis.counts[i], axis.layers[i]));
        assert_eq!((at("Album"), at("Live"), at("Classical"), at("Broadcast")), (Some((4, 1)), Some((2, 2)), Some((1, 3)), Some((1, 1))));
        assert_eq!(axis.known, 6);
    }

    #[test]
    fn an_artist_is_found_by_the_classical_kind_of_their_records() {
        let t = |rel: &str, types: &[&str]| Facts { rel: rel.into(), release_types: types.iter().map(|x| x.to_string()).collect(), ..Facts::default() };
        let (arrau, moby) = (t("x/1", &["Album", "Classical"]), t("y/1", &["Album"]));
        let albums = [
            AlbumItem { id: "al-1".into(), artists: vec!["Claudio Arrau".into()], tracks: vec![&arrau], rated: true },
            AlbumItem { id: "al-2".into(), artists: vec!["Moby".into()], tracks: vec![&moby], rated: true },
        ];
        let (membership, favourites) = (HashMap::new(), std::collections::HashSet::new());
        let w = crates::World { membership: &membership, favourites: Some(&favourites), favourite_albums: None, uses: None, now: 0, fits: None };
        let not_classical = [(Condition::new("type", true, Value::AnyOf { values: vec!["Classical".into()] }), 1)];
        let look = ArtistLook { conditions: &not_classical, world: &w, uses: None, hearted: None, unplayed: false, bit: &|_| 0 };
        let keys: Vec<String> = artists_of("cratarr", &albums, &look).iter().filter(|i| i.set(&not_classical, &w) == 0).map(|i| i.key.clone()).collect();
        assert_eq!(keys, ["cratarr\u{0}moby"]);
    }

    #[test]
    fn a_filter_that_leaves_nothing_names_each_of_its_causes() {
        // Decision 0095: "No records." was said as if there were none; now each condition and switch is a chip.
        let r: Request = serde_json::from_value(serde_json::json!({
            "conditions": [
                { "axis": "energy", "value": { "kind": "range", "min": 80 } },
                { "axis": "genre", "value": { "kind": "any_of", "values": [] } },
                { "axis": "genre", "value": { "kind": "any_of", "values": ["Dub"] }, "enabled": false }
            ],
            "list": { "kind": "library" },
            "switches": ["favourites"]
        }))
        .unwrap();
        let causes = Causes::of(&r).unwrap();
        let said: Vec<(Option<&str>, &str)> = causes.named.iter().map(|c| (c.key.as_deref(), c.label.as_str())).collect();
        assert_eq!(said, [(Some("energy"), "Energy"), (Some("favourites"), "Favourites only")], "a row asking nothing, or muted, is no cause");
        assert!(causes.named.iter().all(|c| c.kind == crates::Why::Filter));
        // The bits: the energy row's first, the muted row none (the row asking nothing is not among them), the switch's next.
        assert_eq!((causes.conditions.as_slice(), causes.switch("favourites"), causes.switch("unplayed")), (&[1, 0][..], 2, 0));
        let json = serde_json::to_value(&causes.named[0]).unwrap();
        assert_eq!(json, serde_json::json!({ "kind": "filter", "key": "energy", "label": "Energy" }));
        let off = serde_json::to_value(crates::Reason::libraries_off(&["cratarr/A".into(), "cratarr/B".into()], 3)).unwrap();
        assert_eq!(off, serde_json::json!({ "kind": "libraries_off", "label": "2 libraries off", "back": 3 }));
    }

    #[test]
    fn never_played_is_the_plays_logs() {
        let f = Facts { rel: "a/1".into(), ..Facts::default() };
        let mut uses: HashMap<String, Use> = HashMap::new();
        assert!(never_played(&f, None), "no account: nothing known played");
        assert!(never_played(&f, Some(&uses)), "no entry: never played");
        uses.insert("a/1".into(), Use { plays: 0, last: Some(5), rating: None, skips: 0 });
        assert!(!never_played(&f, Some(&uses)), "a last play counts, whatever the count says");
        uses.insert("a/1".into(), Use { plays: 2, last: None, rating: None, skips: 0 });
        assert!(!never_played(&f, Some(&uses)));
    }

    #[test]
    fn an_artist_passes_by_one_of_their_albums_and_their_own_switches() {
        let t = |rel: &str, genre: &str| Facts { rel: rel.into(), genres: vec![genre.into()], ..Facts::default() };
        let (rock, jazz, more) = (t("x/1", "Rock"), t("y/1", "Jazz"), t("z/1", "Jazz"));
        fn album<'a>(id: &str, artists: &[&str], tracks: Vec<&'a Facts>, rated: bool) -> AlbumItem<'a> {
            AlbumItem { id: id.into(), artists: artists.iter().map(|a| a.to_string()).collect(), tracks, rated }
        }
        // X has a rock album; Y (also spelt "y") two jazz ones; Z is on the rock one too.
        let albums = [album("al-1", &["X", "Z"], vec![&rock], true), album("al-2", &["Y"], vec![&jazz], true), album("al-3", &["y"], vec![&more], false)];
        let (membership, favourites) = (HashMap::new(), std::collections::HashSet::new());
        let w = crates::World { membership: &membership, favourites: Some(&favourites), favourite_albums: None, uses: None, now: 0, fits: None };
        let rock_only = [(Condition::new("genre", false, Value::AnyOf { values: vec!["rock".into()] }), 1)];
        let keys = |items: Vec<Item>, conditions: &[(Condition, u64)]| items.iter().filter(|i| i.set(conditions, &w) == 0).map(|i| i.key.clone()).collect::<Vec<_>>();
        let bit = |k: &str| -> u64 {
            match k {
                "favourites" => 2,
                "unplayed" => 4,
                "topRated" => 8,
                _ => 0,
            }
        };
        let look = ArtistLook { conditions: &rock_only, world: &w, uses: None, hearted: None, unplayed: false, bit: &bit };
        assert_eq!(keys(artists_of("cratarr", &albums, &look), &rock_only), ["cratarr\u{0}x", "cratarr\u{0}z"]);
        // What keeps Y out of rock: their album that comes back with the fewest causes lifted, the rated one.
        let sets: Vec<u64> = artists_of("cratarr", &albums, &look).iter().map(|i| i.set(&rock_only, &w)).collect();
        assert_eq!(sets, [0, 0, 1]);
        let look = ArtistLook { conditions: &[], world: &w, uses: None, hearted: None, unplayed: false, bit: &bit };
        let all = artists_of("cratarr", &albums, &look);
        assert_eq!(all.len(), 3, "three artists, Y in either spelling once");
        assert_eq!(keys(all, &[]), ["cratarr\u{0}x", "cratarr\u{0}z", "cratarr\u{0}y"], "keyed by the name in lower case");
        // Favourites only: the artist hearted, in any case.
        let hearted: std::collections::HashSet<String> = ["y".to_string()].into();
        let look = ArtistLook { conditions: &[], world: &w, uses: None, hearted: Some(&hearted), unplayed: false, bit: &bit };
        assert_eq!(keys(artists_of("cratarr", &albums, &look), &[]), ["cratarr\u{0}y"]);
        // Unplayed only: none of their tracks played.
        let uses: HashMap<String, Use> = [("x/1".to_string(), Use { plays: 1, last: Some(1), rating: None, skips: 0 })].into();
        let look = ArtistLook { conditions: &[], world: &w, uses: Some(&uses), hearted: None, unplayed: true, bit: &bit };
        assert_eq!(keys(artists_of("cratarr", &albums, &look), &[]), ["cratarr\u{0}y"], "X and Z share the played album");
    }

    #[test]
    fn an_albums_files_are_its_tracks_wherever_they_are() {
        // The owner's "Stellar - Tunnel": two copies in two artists' folders, one album; its folder would be the root.
        let f = |rel: &str, track: i64| Facts { rel: rel.into(), track: Some(track), ..Facts::default() };
        let files = [f("Steve Brenner & Peter D. Gulch/Stellar - Tunnel (1985)/01.flac", 1), f("Steve Brenner & Peter Gulch/Stellar - Tunnel (1985)/01.flac", 2), f("Other/Album/01.flac", 3), Facts { rel: "Loose/02.flac".into(), ..Facts::default() }];
        let album_of: HashMap<String, String> = [(crate::serve::ids::song(1), "al-st".to_string()), (crate::serve::ids::song(2), "al-st".to_string()), (crate::serve::ids::song(3), "al-o".to_string())].into();
        let by = by_album(files.iter(), &album_of);
        assert_eq!(by["al-st"].iter().map(|x| x.rel.as_str()).collect::<Vec<_>>(), [files[0].rel.as_str(), files[1].rel.as_str()]);
        assert_eq!(by["al-o"].len(), 1);
        assert_eq!(by.values().map(Vec::len).sum::<usize>(), 3, "a file of no catalogue track goes nowhere");
    }

    #[test]
    fn an_albums_folder_is_found_through_the_servers_root() {
        let roots = [crate::placement::FolderRoot { folder: "/media/Music".into(), root: "/data/music/".into() }];
        let folders: crate::catalogue::InUse = [(0, "/media/Music"), (1, "/media/lidarr-music")].into_iter().map(|(i, p)| (i, std::path::PathBuf::from(p))).collect();
        assert_eq!(folder_of(&roots, &folders, "/data/music/A/B (2020)/"), Some((0, "A/B (2020)".into())));
        assert_eq!(folder_of(&roots, &folders, "/media/lidarr-music/X/New"), Some((1, "X/New".into())), "Cratarr's own: as it is");
        assert_eq!(folder_of(&roots, &folders, "/data/musical/A"), None, "a root is a whole folder");
        assert_eq!(folder_of(&roots, &folders, "/media/Music"), None, "the folder itself is no album");
    }

    #[test]
    fn a_list_is_named_by_its_kind() {
        let r: Request = serde_json::from_value(serde_json::json!({
            "conditions": [{ "axis": "genre", "value": { "kind": "any_of", "values": ["Dub"] }, "quantifier": "all" }],
            "list": { "kind": "album", "source": "cratarr", "id": "al-1" },
            "without": ["cratarr//media/Incoming"]
        }))
        .unwrap();
        assert_eq!(r.list, List::Album { source: "cratarr".into(), id: "al-1".into() });
        assert_eq!((r.conditions[0].quantifier, r.conditions[0].enabled), (crates::Quantifier::All, true));
        let r: Request = serde_json::from_value(serde_json::json!({ "list": { "kind": "library" } })).unwrap();
        assert_eq!((r.list, r.conditions.len(), r.without.len()), (List::Library, 0, 0));
    }
}
