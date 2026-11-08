//! A device answering for itself (PREVIEW2's D18 to D20, 0101's points 17 to 19): what Cratarr says of crates, of Sounds like and of what
//! comes next, worked out here from what the device keeps, by the rules the server runs. JSON in and out through [`Device::ask`], so
//! the page and the app ask the same one thing in the same words, and neither holds a decision of its own.
//!
//! - **`facts` are made once, and hold the one copy of each sound** (the tablet held 173 MB in rules): `Alike` and Radio's index share
//!   them, and each is made only when something first asks.
//! - **What only the server can say stands as it last said it.** Fits, counts and what an account set come in [`Marks`] and
//!   [`Own`]. A count is the server's last plus this device's plays not told yet ([`Asked::Plays`]; the owner: D18). New for you and
//!   Grave digging are the tracks last worked out (`Kept::shelf`), as nothing here knows what is new for whom.
//! - **A list is the true one:** what is not on this device is on it, and a page shows so. Only a pick leaves out what cannot play, as
//!   Radio and Autoplay must play it, with no notice (the owner, 2026-11-06).
//! - **Libraries off count as on the server:** by `without`, a page's own keys.
//!
//! Left to the last copy a page holds, as nothing here can say them: a moment's crate, a filter's axes, a list of what keeps what out.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use rustc_hash::{FxHashMap, FxHashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use super::own::{Kept, List, Own};
use super::{Index, Marks, Sounds};
use crate::crates::{self, Alike, Condition, Facts, Favourites, Use, View, World};
use crate::names::{name_key, natural_keys};
use crate::radio::{self, Ask};
use crate::similarity::Weights;
use crate::{Rng, ids};

/// How Cratarr's own library is named in what a page says.
const SOURCE: &str = "cratarr";
/// How far back Radio leaves what was played for later, and what a crate's card calls new (s).
const WEEK: i64 = 7 * 86_400;
/// A list's top genres, and a card's.
const GENRES_SHOWN: usize = 5;
const CARD_GENRES: usize = 3;
const CARD_COVERS: usize = 4;

/// A play this device has not told yet, as it is kept to be told.
#[derive(Debug, Clone, Deserialize)]
pub struct Unsent {
    #[serde(default)]
    source: Option<String>,
    track: String,
    /// When it was played (epoch ms).
    at: i64,
    #[serde(default)]
    ended: Option<String>,
    /// Whether it counts as a play; absent, it does.
    #[serde(default)]
    counts: Option<bool>,
}

/// The orders a crate's list takes, as the page names them (the server's `CrateOrder`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Order {
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
    #[default]
    BestMatch,
}

/// How a crate is looked at, as the server's `CrateLook`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Look {
    #[serde(default)]
    pub favourites: Option<Favourites>,
    #[serde(default)]
    pub favourite_albums: Option<Favourites>,
    #[serde(default)]
    pub without: Vec<String>,
    #[serde(default)]
    pub filter: Vec<Condition>,
    #[serde(default)]
    pub order: Order,
    #[serde(default)]
    pub down: bool,
}

/// What Radio and Autoplay stay inside, as the page's collection says it.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Within {
    Crate {
        id: String,
        #[serde(default)]
        without: Vec<String>,
    },
    Playlist {
        id: String,
    },
    Album {
        id: String,
    },
    Artist {
        name: String,
    },
    Tracks {
        ids: Vec<String>,
    },
}

/// What a page or the app asks. `now`: epoch seconds, as nothing here has a clock.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "ask", rename_all = "snake_case")]
pub enum Asked {
    /// The plays this device has not told yet: asked first, and whenever they change.
    Plays { plays: Vec<Unsent> },
    /// The Crates page's list.
    Crates {
        #[serde(default)]
        without: Vec<String>,
        now: i64,
    },
    /// A crate's definition.
    Crate { id: String },
    /// A page of a crate's rows.
    Rows {
        id: String,
        #[serde(flatten)]
        look: Look,
        offset: usize,
        limit: usize,
        now: i64,
    },
    /// A crate's tempo spread.
    Tempo {
        id: String,
        #[serde(flatten)]
        look: Look,
        now: i64,
    },
    /// What comes next: `here`, what plays on this device; `draw`, what seeds the generator.
    Pick {
        #[serde(flatten)]
        what: Ask,
        #[serde(default)]
        within: Option<Within>,
        #[serde(default)]
        here: Option<Vec<String>>,
        draw: u64,
        now: i64,
    },
}

/// A crate's row, in the names a page's lists give it (the server's `PlaylistRow`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
struct Row {
    entry: String,
    file: String,
    id: Option<String>,
    name: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    duration_secs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    album_artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disc: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    track: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bpm: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    play_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_played: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    added: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    album_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    album_tracks: Option<u32>,
    missing: bool,
}

/// A crate as worked out for a look.
struct Def {
    name: String,
    conditions: Vec<Condition>,
    favourites: Favourites,
    albums: Favourites,
    left: Vec<String>,
}

/// The library as a device answers from it. See the module's notes.
pub struct Device {
    index: Arc<Index>,
    facts: Vec<Facts>,
    own: Own,
    /// Each file by its track, and by every line it goes by.
    place: HashMap<i64, usize>,
    lines: FxHashMap<String, usize>,
    /// What the server last said of each track.
    fits: HashMap<i64, f64>,
    skipped: HashMap<i64, f64>,
    said: HashMap<i64, Use>,
    /// Which of the account's playlists hold each line.
    membership: HashMap<String, Vec<String>>,
    favourites: HashSet<String>,
    favourite_albums: HashSet<String>,
    /// `said`, with the plays not told yet, by line as a crate reads them.
    uses: HashMap<String, Use>,
    unsent: Vec<Unsent>,
    /// How many tracks each record holds.
    sizes: OnceLock<HashMap<i64, u32>>,
    alike: OnceLock<Alike>,
    radio: OnceLock<radio::Index>,
}

impl Device {
    /// From what is kept. `sounds`: none, and no Sounds like finds anything by sound.
    pub fn new(index: impl Into<Arc<Index>>, sounds: Option<&Sounds>, marks: &Marks) -> Self {
        let index = index.into();
        let facts = index.facts(sounds);
        Self::over(index, facts, marks)
    }

    /// Over facts already made: a test's, whose sounds are the server's own.
    pub fn over(index: impl Into<Arc<Index>>, facts: Vec<Facts>, marks: &Marks) -> Self {
        let index = index.into();
        let own: Own = serde_json::from_str(marks.own()).unwrap_or_default();
        let place: HashMap<i64, usize> = facts.iter().enumerate().filter_map(|(i, f)| Some((f.track?, i))).collect();
        let lines = facts.iter().enumerate().flat_map(|(i, f)| f.every_line().map(move |l| (l.to_string(), i))).collect();
        let rel = |track: &i64| place.get(track).map(|&i| facts[i].rel.clone());
        let mut membership: HashMap<String, Vec<String>> = HashMap::new();
        for list in &own.lists {
            for line in list.tracks.iter().filter_map(rel) {
                let names = membership.entry(line).or_default();
                if !names.contains(&list.name) {
                    names.push(list.name.clone());
                }
            }
        }
        let favourites = own.favourites.iter().filter_map(rel).collect();
        let hearted: HashSet<String> = own.favourite_albums.iter().map(|r| super::album_id(*r)).collect();
        let favourite_albums = facts.iter().filter(|f| f.album.as_deref().is_some_and(|a| hearted.contains(a))).map(|f| f.rel.clone()).collect();
        let rows = marks.rows();
        let fits = rows.iter().filter_map(|m| Some((m.track, m.fit?))).collect();
        let skipped = rows.iter().filter_map(|m| Some((m.track, m.skipped?))).collect();
        let said: HashMap<i64, Use> = rows.iter().map(|m| (m.track, Use { plays: m.plays, last: m.last, rating: m.rating, skips: m.skips })).collect();
        let mut device = Self { index, facts, own, place, lines, fits, skipped, said, membership, favourites, favourite_albums, uses: HashMap::new(), unsent: Vec::new(), sizes: OnceLock::new(), alike: OnceLock::new(), radio: OnceLock::new() };
        device.count();
        device
    }

    /// What a crate reads of how each track was used: the server's last, and this device's plays since.
    fn count(&mut self) {
        let mut by_track = self.said.clone();
        for p in &self.unsent {
            let Some(track) = p.source.as_deref().is_none_or(|s| s == SOURCE).then(|| ids::track_of(&p.track)).flatten() else { continue };
            let u = by_track.entry(track).or_default();
            if p.counts != Some(false) {
                u.plays += 1;
                u.last = u.last.max(Some(p.at / 1000));
            } else if p.ended.as_deref() == Some("skipped") {
                u.skips += 1;
            }
        }
        self.uses = by_track.into_iter().filter_map(|(track, u)| Some((self.facts[*self.place.get(&track)?].rel.clone(), u))).collect();
    }

    /// The answer to what is asked, as JSON; none where it cannot be read, or this device has nothing to say to it: the asker then goes
    /// where any read goes.
    pub fn ask(&mut self, asked: &str) -> Option<String> {
        let answer = match serde_json::from_str::<Asked>(asked).ok()? {
            Asked::Plays { plays } => {
                self.unsent = plays;
                self.count();
                Some(json!({}))
            }
            // Nothing of the account's held: a list of none would be a wrong answer, and the server always has some.
            Asked::Crates { without, now } => (!self.own.crates.is_empty()).then(|| self.crates(&without, now)),
            Asked::Crate { id } => self.crate_def(&id),
            Asked::Rows { id, look, offset, limit, now } => self.rows(&id, &look, offset, limit, now),
            Asked::Tempo { id, look, now } => self.tempo(&id, &look, now),
            Asked::Pick { what, within, here, draw, now } => Some(self.pick(what, within.as_ref(), here.as_deref(), draw, now)),
        };
        answer.map(|a| a.to_string())
    }

    fn by(&self) -> Weights {
        self.own.similar_by.as_deref().map_or(Weights::EQUAL, Weights::ranked)
    }

    /// The library as the one similarity sees it: made when a Sounds like first asks.
    fn alike(&self) -> &Alike {
        self.alike.get_or_init(|| Alike::of(&self.facts, &self.membership, &self.index.artist_ids()))
    }

    /// Radio's index, the listener's marks on its songs: made when a pick first asks. A song's taste is read by Your taste alone.
    fn radio(&self) -> &radio::Index {
        self.radio.get_or_init(|| {
            let mut member: HashMap<String, Vec<u32>> = HashMap::new();
            for (n, list) in self.own.together.iter().enumerate() {
                for track in list {
                    member.entry(ids::song(*track)).or_default().push(n as u32);
                }
            }
            let mut index = self.index.radio(&self.facts, &member);
            let (hearted, out): (HashSet<i64>, HashSet<i64>) = (self.own.favourites.iter().copied().collect(), self.excluded());
            for s in &mut index.songs {
                let Some(track) = ids::track_of(&s.id) else { continue };
                s.favourite = hearted.contains(&track);
                s.excluded = out.contains(&track);
                s.taste = self.fits.get(&track).copied();
                s.skipped = self.skipped.get(&track).copied().unwrap_or(0.0);
            }
            index
        })
    }

    /// What every pick leaves out: what was excluded, and every track crediting an artist that was.
    fn excluded(&self) -> HashSet<i64> {
        let keys: HashSet<String> = self.own.artists_out.iter().cloned().collect();
        let crediting = self.facts.iter().filter(|f| crates::credits_out(f, &keys)).filter_map(|f| f.track);
        self.own.excluded.iter().copied().chain(crediting).collect()
    }

    /// The folders of the libraries a page's `without` names.
    fn off(&self, without: &[String]) -> HashSet<i64> {
        let named = |key: &str| without.iter().any(|w| w.strip_prefix(SOURCE).and_then(|w| w.strip_prefix('/')) == Some(key));
        self.index.libraries().into_iter().filter(|l| named(&l.key)).map(|l| l.id).collect()
    }

    /// The lines that fit the taste (Fits your taste): `crates::FITS_SHARE` of what is on and has a fit, the best fitting. None while
    /// there is no taste.
    fn fitting(&self, off: &HashSet<i64>) -> Option<HashSet<String>> {
        let mut fit: Vec<(String, f64, &str)> = self.facts.iter().filter(|f| crates::on_for(&f.folders, off)).filter_map(|f| Some((ids::song(f.track?), *self.fits.get(&f.track?)?, f.rel.as_str()))).collect();
        if fit.is_empty() {
            return None;
        }
        fit.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let keep = ((fit.len() as f64 * crates::FITS_SHARE).ceil() as usize).max(1);
        Some(fit.into_iter().take(keep).map(|(_, _, line)| line.to_string()).collect())
    }

    fn world<'a>(&'a self, fits: Option<&'a HashSet<String>>, now: i64) -> World<'a> {
        World { membership: &self.membership, favourites: Some(&self.favourites), favourite_albums: Some(&self.favourite_albums), uses: Some(&self.uses), now, fits }
    }

    fn kept(&self, id: &str) -> Option<&Kept> {
        self.own.crates.iter().find(|k| k.id == id)
    }

    /// A record's tracks as its album plays: by disc, number and line.
    fn tracks_of(&self, record: i64) -> Vec<i64> {
        let album = super::album_id(record);
        let mut tracks: Vec<(Option<u32>, Option<u32>, &str, i64)> = Vec::new();
        for f in self.facts.iter().filter(|f| f.album.as_deref() == Some(album.as_str())) {
            let Some(id) = f.track else { continue };
            let t = self.index.track(id);
            tracks.push((t.as_ref().and_then(|t| t.disc), t.as_ref().and_then(|t| t.number), f.rel.as_str(), id));
        }
        tracks.sort();
        tracks.into_iter().map(|t| t.3).collect()
    }

    /// A Sounds like made when asked for, as the server makes one: of a track, a record, an artist or a playlist.
    fn like(&self, id: &str, item: &str, count: usize) -> Option<crates::CrateDef> {
        if let Some(list) = item.strip_prefix(crates::LIST) {
            let found: &List = self.own.lists.iter().find(|l| l.id == list)?;
            return Some(crates::playlist_crate(id, list.rsplit('/').next().unwrap_or(list), found.tracks.iter().map(|t| ids::song(*t)).collect(), count));
        }
        let (name, seeds) = if let Some(track) = ids::track_of(item) {
            (self.index.track(track)?.title, vec![item.to_string()])
        } else if let Some(artist) = item.strip_prefix(ids::ARTIST) {
            (self.index.artist(artist.parse().ok()?)?.name, vec![item.to_string()])
        } else {
            let record: i64 = item.strip_prefix(ids::ALBUM)?.parse().ok()?;
            (self.index.record(record)?.title, self.tracks_of(record).into_iter().map(ids::song).collect())
        };
        Some(crates::like_crate(id, &format!("Sounds like {name}"), seeds, count))
    }

    /// Conditions kept for one size of a Sounds like, at another: how many it holds is all that differs.
    fn sized(conditions: &[Condition], count: usize) -> Vec<Condition> {
        let mut out = conditions.to_vec();
        for c in &mut out {
            if let crates::Value::Like { count: held, .. } = &mut c.value {
                *held = count;
            }
        }
        out
    }

    /// What a crate is here; none where this device cannot make it (a moment's, one of an item it does not hold).
    fn def(&self, id: &str) -> Option<Def> {
        let liked = crates::like_of(id);
        let kept = self.kept(id).or_else(|| self.kept(&crates::kept_id(id)));
        if let Some(k) = kept {
            let left: Vec<String> = k.left_out.iter().map(|t| ids::song(*t)).collect();
            let conditions = match (&k.shelf, liked) {
                (Some(shelf), _) => {
                    let lines = shelf.iter().filter_map(|t| Some(self.facts[*self.place.get(t)?].rel.clone())).collect();
                    let mut all = crates::lines_crate(id, &k.name, lines).conditions;
                    all.extend(k.conditions.iter().cloned());
                    all
                }
                (None, Some((_, count))) if k.id != id => Self::sized(&k.conditions, count),
                _ => k.conditions.clone(),
            };
            return Some(Def { name: k.name.clone(), conditions, favourites: k.favourites, albums: k.favourite_albums, left });
        }
        let made = crates::genre_crate(id).or_else(|| liked.and_then(|(item, count)| self.like(id, item, count)))?;
        Some(Def { name: made.name, conditions: made.conditions, favourites: Favourites::All, albums: Favourites::All, left: Vec::new() })
    }

    /// A crate's conditions as they show for this look, and which of them each cause added: as the server's `crate_causes`.
    fn shown(&self, id: &str, def: &Def, look: &Look) -> (Vec<Condition>, Vec<crates::Cause>) {
        let (tracks, albums) = if crates::is_fixed(id) { (look.favourites.unwrap_or(def.favourites), look.favourite_albums.unwrap_or(def.albums)) } else { (Favourites::All, Favourites::All) };
        let excluded: Vec<String> = self.own.excluded.iter().map(|t| ids::song(*t)).collect();
        let artists: HashSet<String> = self.own.artists_out.iter().cloned().collect();
        crates::shown_saying(&def.conditions, &def.left, tracks, albums, &excluded, &artists, &look.filter)
    }

    /// Home's crates are of every library, whatever is switched off (the owner, 2026-11-02).
    fn switched_off(&self, id: &str, without: &[String]) -> HashSet<i64> {
        if crates::home_crate(id) { HashSet::new() } else { self.off(without) }
    }

    /// What a crate's list orders by, of a line.
    fn row(&self, line: &str) -> Row {
        let Some(&at) = self.lines.get(line) else { return Row { entry: line.to_string(), file: file_of(line), missing: true, ..Row::default() } };
        let f = &self.facts[at];
        let t = f.track.and_then(|id| self.index.track(id));
        let record = t.as_ref().and_then(|t| self.index.record(t.record));
        let used = self.uses.get(&f.rel).filter(|u| u.plays > 0 || u.last.is_some());
        Row {
            entry: line.to_string(),
            file: file_of(line),
            id: f.song(),
            name: t.as_ref().map(|t| t.title.clone()),
            artist: t.as_ref().and_then(|t| t.artist.clone()),
            album: record.as_ref().map(|r| r.title.clone()),
            duration_secs: f.length,
            album_artist: record.as_ref().map(|r| r.artist.clone()).filter(|a| !a.is_empty()),
            disc: t.as_ref().and_then(|t| t.disc),
            track: t.as_ref().and_then(|t| t.number),
            year: f.year.map(|y| y as i32),
            bpm: f.bpm.map(|b| b.round() as u32),
            play_count: used.map(|u| u.plays),
            last_played: used.and_then(|u| u.last),
            added: f.born,
            album_id: f.album.as_deref().map(str::to_string),
            album_tracks: t.as_ref().and_then(|t| self.sizes().get(&t.record).copied()),
            missing: false,
        }
    }

    fn sizes(&self) -> &HashMap<i64, u32> {
        self.sizes.get_or_init(|| {
            let mut sizes = HashMap::new();
            for record in &self.index.packed.tracks.record {
                *sizes.entry(*record).or_default() += 1;
            }
            sizes
        })
    }

    /// A crate's lines in the order its list shows, the server's `crate_in_order`.
    fn ordered(&self, mut lines: Vec<String>, look: &Look) -> Vec<String> {
        match look.order {
            Order::Own => {
                if look.down {
                    lines.reverse();
                }
                lines
            }
            Order::BestMatch => {
                let fit = |l: &str| self.fits.get(&self.facts[*self.lines.get(l)?].track?).copied();
                crates::by_fit(lines, fit, look.down)
            }
            order => {
                let rows: Vec<Row> = lines.iter().map(|l| self.row(l)).collect();
                sorted(rows, order, look.down).into_iter().map(|r| r.entry).collect()
            }
        }
    }

    fn rows(&self, id: &str, look: &Look, offset: usize, limit: usize, now: i64) -> Option<Json> {
        let def = self.def(id)?;
        let (shown, causes) = self.shown(id, &def, look);
        let without = if crates::home_crate(id) { &[][..] } else { &look.without[..] };
        let off = self.switched_off(id, &look.without);
        let fits = if crates::needs_fits(&shown) { Some(self.fitting(&off)?) } else { None };
        let by = self.by();
        let view = View { files: &self.facts, switched_off: &off, world: self.world(fits.as_ref(), now), by: &by, alike: || self.alike() };
        let mine: Vec<String> = without.iter().filter(|k| k.starts_with(&format!("{SOURCE}/"))).cloned().collect();
        let (lines, reasons, all_back, _) = view.select_saying(&shown, &causes, &mine, false);
        let genres = if offset == 0 {
            let chosen: FxHashSet<&str> = lines.iter().map(String::as_str).collect();
            crates::top_genres(self.facts.iter().filter(|f| crates::on_for(&f.folders, &off) && chosen.contains(f.rel.as_str())).map(|f| f.genres.as_slice()), GENRES_SHOWN)
        } else {
            Vec::new()
        };
        let total = lines.len();
        let lines = self.ordered(lines, look);
        let rows: Vec<Row> = lines.iter().skip(offset).take(limit).map(|l| self.row(l)).collect();
        Some(json!({ "source": SOURCE, "total": total, "limit": limit, "rows": rows, "genres": genres, "reasons": reasons, "all_back": all_back }))
    }

    fn tempo(&self, id: &str, look: &Look, now: i64) -> Option<Json> {
        let def = self.def(id)?;
        let (shown, _) = self.shown(id, &def, look);
        let off = self.switched_off(id, &look.without);
        let fits = if crates::needs_fits(&shown) { Some(self.fitting(&off)?) } else { None };
        let by = self.by();
        let view = View { files: &self.facts, switched_off: &off, world: self.world(fits.as_ref(), now), by: &by, alike: || self.alike() };
        let lines = view.select(&shown, false);
        let chosen: FxHashSet<&str> = lines.iter().map(String::as_str).collect();
        let bands = crates::tempo_bands(self.facts.iter().filter(|f| chosen.contains(f.rel.as_str())).map(|f| f.bpm));
        serde_json::to_value(bands).ok()
    }

    /// A crate's definition, as `GET /api/crates/{id}` says it. Home's are a list of lines, not conditions anyone sets.
    fn crate_def(&self, id: &str) -> Option<Json> {
        let def = self.def(id)?;
        let conditions: Vec<&Condition> = def.conditions.iter().filter(|c| !matches!(c.value, crates::Value::Within { .. })).collect();
        let not_artists: Vec<Json> = crates::not_artists(&def.conditions).into_iter().map(|(key, name)| json!({ "key": key, "name": name })).collect();
        Some(json!({ "id": id, "name": def.name, "conditions": conditions, "not_artists": not_artists, "names": self.names_of(&def.conditions) }))
    }

    /// The names of what conditions name by id (`crates::named_ids`).
    fn names_of(&self, conditions: &[Condition]) -> serde_json::Map<String, Json> {
        let mut out = serde_json::Map::new();
        for id in crates::named_ids(conditions) {
            let name = if let Some(track) = ids::track_of(&id) {
                self.index.track(track).map(|t| t.title)
            } else if let Some(artist) = id.strip_prefix(ids::ARTIST).and_then(|a| a.parse().ok()) {
                self.index.artist(artist).map(|a| a.name)
            } else {
                id.strip_prefix(ids::ALBUM).and_then(|r| r.parse().ok()).and_then(|r| self.index.record(r)).map(|r| r.title)
            };
            if let (Some(name), Some(kind)) = (name, crates::NameKind::of(&id)) {
                out.insert(id, json!(crates::Named { name, kind }));
            }
        }
        out
    }

    /// What a crate's card says of `lines`.
    fn card(&self, lines: &[String], now: i64) -> Json {
        let first_look = self.facts.iter().filter_map(|f| f.born).min().unwrap_or(0) + 86_400;
        let known: Vec<&Facts> = lines.iter().filter_map(|l| Some(&self.facts[*self.lines.get(l)?])).collect();
        // Each genre counted under its first spelling, in the order first met, found by its letters as `eq_ignore_ascii_case` reads
        // them: a crate of thousands of tracks is not read once for each.
        let mut genres: Vec<(String, usize)> = Vec::new();
        let mut genre_at: FxHashMap<String, usize> = FxHashMap::default();
        for g in known.iter().flat_map(|f| &f.genres) {
            let at = *genre_at.entry(g.to_ascii_lowercase()).or_insert_with(|| {
                genres.push((g.clone(), 0));
                genres.len() - 1
            });
            genres[at].1 += 1;
        }
        genres.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
        // Its records: how many of its tracks, and the newest of them.
        let mut albums: Vec<(&str, usize, i64)> = Vec::new();
        let mut album_at: FxHashMap<&str, usize> = FxHashMap::default();
        for f in &known {
            let (Some(album), born) = (f.album.as_deref(), f.born.unwrap_or(0)) else { continue };
            let at = *album_at.entry(album).or_insert_with(|| {
                albums.push((album, 0, born));
                albums.len() - 1
            });
            let (_, n, newest) = &mut albums[at];
            *n += 1;
            *newest = (*newest).max(born);
        }
        albums.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)).then(a.0.cmp(b.0)));
        // Never played: by the plays the server last counted and this device's since.
        let unplayed = known.iter().filter(|f| !self.uses.get(&f.rel).is_some_and(|u| u.plays > 0)).count();
        json!({
            "tracks": lines.len(),
            "seconds": known.iter().filter_map(|f| f.length).map(|s| s as u64).sum::<u64>(),
            "unplayed": unplayed,
            "new_week": known.iter().filter(|f| f.born.is_some_and(|b| b >= now - WEEK && b > first_look)).count(),
            "genres": genres.into_iter().take(CARD_GENRES).map(|(g, _)| g).collect::<Vec<_>>(),
            "covers": albums.into_iter().take(CARD_COVERS).map(|(a, _, _)| json!({ "source": SOURCE, "album_id": a })).collect::<Vec<_>>(),
        })
    }

    /// The Crates page's list. A Sounds like costs a pass over the library for each answer, so its sizes for the other two settings of
    /// the toggle are left unknown; the list says so as it does of favourites not read.
    fn crates(&self, without: &[String], now: i64) -> Json {
        let (by, artists) = (self.by(), self.own.artists_out.iter().cloned().collect::<HashSet<String>>());
        let excluded: Vec<String> = self.own.excluded.iter().map(|t| ids::song(*t)).collect();
        let mut out = Vec::new();
        for k in self.own.crates.iter().filter(|k| k.listed) {
            let Some(def) = self.def(&k.id) else { continue };
            let fixed = crates::is_fixed(&k.id);
            let (conditions, tracks, albums) = if fixed { (def.conditions.clone(), def.favourites, def.albums) } else { crates::toggles_apart(&def.conditions) };
            let gone: Vec<String> = def.left.iter().chain(&excluded).cloned().collect();
            let shown = |tracks: Favourites| crates::leaving_out_artists(&artists, &crates::leaving_out(&gone, &crates::shown(&conditions, tracks, albums)));
            let off = self.switched_off(&k.id, without);
            let fits = if crates::needs_fits(&conditions) { self.fitting(&off) } else { None };
            let view = View { files: &self.facts, switched_off: &off, world: self.world(fits.as_ref(), now), by: &by, alike: || self.alike() };
            let ranks = conditions.iter().any(|c| c.value.ranks());
            // Its sizes at the toggle's three settings from one selection: a setting is one condition more, so its tracks are the
            // favourites among them or the rest. A Sounds like picks its closest anew at each, and says only its own setting's.
            let every = if ranks { Vec::new() } else { view.select(&shown(Favourites::All), false) };
            let held: Vec<&Facts> = every.iter().filter_map(|l| Some(&self.facts[*self.lines.get(l)?])).collect();
            let among = |setting: Favourites| crates::select(held.iter().copied(), &setting.with(&[]), &view.world);
            let lines: Vec<String> = match (ranks, tracks) {
                (true, _) => view.select(&shown(tracks), false),
                (false, Favourites::All) => every.clone(),
                (false, _) => among(tracks).into_iter().map(str::to_string).collect(),
            };
            let count = |setting: Favourites| match (setting == tracks, ranks) {
                (true, _) => Some(lines.len()),
                (false, true) => None,
                (false, false) if setting == Favourites::All => Some(every.len()),
                (false, false) => Some(among(setting).len()),
            };
            out.push(json!({
                "id": k.id,
                "name": k.name,
                "favourites": tracks,
                "favourite_albums": albums,
                "left_out": k.left,
                "counts": { "all": count(Favourites::All), "only": count(Favourites::Only), "none": count(Favourites::None) },
                "permanent": k.permanent,
                "stats": self.card(&lines, now),
                "image": k.image,
            }));
        }
        Json::Array(out)
    }

    /// The tracks of what a pick stays inside, in the order the server reads them.
    fn within(&self, what: &Within, now: i64) -> Vec<String> {
        match what {
            Within::Crate { id, without } => {
                let look = Look { without: without.clone(), order: Order::Own, ..Look::default() };
                self.rows_all(id, &look, now)
            }
            Within::Playlist { id } => self.own.lists.iter().find(|l| l.id == *id).map(|l| l.tracks.iter().map(|t| ids::song(*t)).collect()).unwrap_or_default(),
            Within::Album { id } => id.strip_prefix(ids::ALBUM).and_then(|r| r.parse().ok()).map(|r| self.tracks_of(r).into_iter().map(ids::song).collect()).unwrap_or_default(),
            Within::Artist { name } => {
                let key = crate::excluded_artists::key(name);
                self.facts.iter().filter(|f| f.credits.contains(&key)).filter_map(Facts::song).collect()
            }
            Within::Tracks { ids } => ids.clone(),
        }
    }

    /// A crate's every track, in the order of its look.
    fn rows_all(&self, id: &str, look: &Look, now: i64) -> Vec<String> {
        let Some(def) = self.def(id) else { return Vec::new() };
        let (shown, _) = self.shown(id, &def, look);
        let off = self.switched_off(id, &look.without);
        let fits = if crates::needs_fits(&shown) { self.fitting(&off) } else { None };
        let by = self.by();
        let view = View { files: &self.facts, switched_off: &off, world: self.world(fits.as_ref(), now), by: &by, alike: || self.alike() };
        let lines = self.ordered(view.select(&shown, false), look);
        lines.iter().filter_map(|l| self.facts[*self.lines.get(l)?].song()).collect()
    }

    /// What was played within the week, with when: what the server said, and what this device has not told it.
    fn recent(&self, now: i64) -> HashMap<String, i64> {
        let mut out: HashMap<String, i64> = self.own.recent.iter().filter(|r| r.1 >= now - WEEK).map(|(t, at)| (ids::song(*t), *at)).collect();
        for p in self.unsent.iter().filter(|p| p.counts != Some(false) && p.at / 1000 >= now - WEEK) {
            out.insert(p.track.clone(), p.at / 1000);
        }
        out
    }

    /// What comes next, as `POST /api/radio/next` says it, among what plays here. Bandcamp's suggestions are the web's.
    fn pick(&self, mut ask: Ask, within: Option<&Within>, here: Option<&[String]>, draw: u64, now: i64) -> Json {
        let index = self.radio();
        if ask.similar_by.is_none() {
            ask.similar_by.clone_from(&self.own.similar_by);
        }
        ask.folders.clone_from(&self.own.libraries);
        let within: Option<Vec<usize>> = within.map(|w| index.find(&self.within(w, now)));
        let here: Option<HashSet<usize>> = here.map(|h| index.find(h).into_iter().collect());
        let picks = radio::choose_among(index, &ask, &self.recent(now), within.as_deref(), here.as_ref(), &mut Rng::seeded(draw));
        let lane = if ask.mode == radio::Mode::Radio { "radio" } else { "autoplay" };
        let tracks: Vec<Json> = picks.iter().map(|&i| &index.songs[i]).map(|s| json!({ "lane": lane, "source": SOURCE, "id": s.id, "name": s.title, "artist": s.artist, "album": s.album, "album_id": s.album_id, "duration_secs": s.duration_secs })).collect();
        json!({ "tracks": tracks })
    }
}

fn file_of(line: &str) -> String {
    line.rsplit('/').next().unwrap_or(line).to_string()
}

/// Rows in an order of the page's list, as the server's `sort_rows`: names by their natural order; numbers with one a row does not have
/// first (last, turned round); plays counting none as 0; equals keeping the crate's own order.
fn sorted(mut rows: Vec<Row>, by: Order, down: bool) -> Vec<Row> {
    let way = |o: std::cmp::Ordering| if down { o.reverse() } else { o };
    let text = |r: &Row| -> String {
        match by {
            Order::Title => r.name.clone().unwrap_or_else(|| r.file.clone()),
            Order::Artist => r.artist.clone().unwrap_or_default(),
            Order::AlbumArtist => r.album_artist.clone().unwrap_or_default(),
            _ => r.album.clone().unwrap_or_default(),
        }
    };
    let number = |r: &Row| -> Option<f64> {
        match by {
            Order::Length => r.duration_secs,
            Order::Tempo => r.bpm.map(f64::from),
            Order::Year => r.year.map(f64::from),
            Order::Track => r.track.map(|t| f64::from(r.disc.unwrap_or(1)) * 10_000.0 + f64::from(t)),
            Order::Plays => Some(f64::from(r.play_count.unwrap_or(0))),
            Order::LastPlayed => r.last_played.map(|t| t as f64),
            _ => r.added.map(|t| t as f64),
        }
    };
    if matches!(by, Order::Title | Order::Artist | Order::AlbumArtist | Order::Album) {
        let mut keyed: Vec<(Vec<char>, Row)> = rows.into_iter().map(|r| (name_key(&text(&r)), r)).collect();
        keyed.sort_by(|a, b| way(natural_keys(&a.0, &b.0)));
        return keyed.into_iter().map(|(_, r)| r).collect();
    }
    rows.sort_by(|a, b| way(number(a).partial_cmp(&number(b)).unwrap_or(std::cmp::Ordering::Equal)));
    rows
}

#[cfg(test)]
mod tests;
