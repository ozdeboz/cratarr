//! What Cratarr has of an account's Spotify lists (decision 0073): each item *in* the library, *partly*, *missing* or
//! *requested*. Through MusicBrainz first (an ISRC to its recordings, a UPC to its releases, matched against the ids
//! the library's tags carry), else by name as 0067's matcher finds an import's tracks. The answers MusicBrainz gave are
//! read from the store; asking it is `super::ask_musicbrainz`'s, in the background.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{Album, Artist, Lists, Playlist, Track};
use crate::app::import::{Matcher, Track as Wanted, key};
use crate::userdata::Request;

pub const IN: &str = "in";
pub const PARTLY: &str = "partly";
pub const MISSING: &str = "missing";
pub const REQUESTED: &str = "requested";

/// The library as matching sees it: its songs (0067's matcher), its albums by MusicBrainz release, its songs' albums,
/// and its artists by name.
pub struct Library {
    pub source: String,
    matcher: Matcher,
    by_release: HashMap<String, String>,
    album_of: HashMap<String, String>,
    /// An artist's id and how many albums it has, by its name's key.
    artists: HashMap<String, (String, u32)>,
}

impl Library {
    pub fn read(path: &Path, source: &str) -> Result<Self> {
        let matcher = Matcher::read(path)?;
        let db = crate::db::open_read_only(path)?;
        let mut by_release = HashMap::new();
        let mut st = db.prepare("SELECT mbid, id FROM albums WHERE mbid IS NOT NULL AND mbid <> ''")?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (mbid, id) = row?;
            by_release.entry(mbid.to_lowercase()).or_insert(id);
        }
        let mut album_of = HashMap::new();
        let mut st = db.prepare("SELECT id, album_id FROM songs")?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (song, album) = row?;
            album_of.insert(song, album);
        }
        let mut artists = HashMap::new();
        let mut st = db.prepare("SELECT name, id, album_count FROM artists ORDER BY album_count DESC")?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, u32>(2)?)))? {
            let (name, id, albums) = row?;
            artists.entry(key(&name)).or_insert((id, albums));
        }
        Ok(Self { source: source.to_string(), matcher, by_release, album_of, artists })
    }
}

impl Library {
    /// Whether the library has an album by its title and artist (0076: Beyond marks what is here already).
    pub fn has_album(&self, title: &str, artist: &str) -> bool {
        self.matcher.find_album(title, Some(artist)).is_some()
    }
}

/// What MusicBrainz answered, as kept (`store::mb`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Release {
    /// The releases with the UPC as barcode.
    pub releases: Vec<String>,
    pub release_group: Option<String>,
}

pub struct Answers(pub HashMap<String, String>);

impl Answers {
    pub fn recordings(&self, isrc: &str) -> Vec<String> {
        self.0.get(&isrc_key(isrc)).and_then(|v| serde_json::from_str(v).ok()).unwrap_or_default()
    }
    pub fn release(&self, upc: &str) -> Release {
        self.0.get(&upc_key(upc)).and_then(|v| serde_json::from_str(v).ok()).unwrap_or_default()
    }
    pub fn named(&self, artist: &str, album: &str) -> Option<String> {
        self.0.get(&name_key(artist, album)).and_then(|v| serde_json::from_str(v).ok()).flatten()
    }
}

pub fn isrc_key(isrc: &str) -> String {
    format!("isrc:{}", isrc.to_uppercase())
}

pub fn upc_key(upc: &str) -> String {
    format!("upc:{}", upc.trim_start_matches('0'))
}

pub fn name_key(artist: &str, album: &str) -> String {
    format!("name:{}|{}", key(artist), key(album))
}

/// A thing of Cratarr's own an item is.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ref {
    pub source: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlbumView {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub cover: Option<String>,
    pub added_at: Option<i64>,
    pub status: &'static str,
    pub have: u32,
    pub of: u32,
    pub release_group: Option<String>,
    pub album: Option<Ref>,
    pub request: Option<Request>,
    /// The page looks for Bandcamp's itself, as for every missing album (lib/ElsewhereAlbum).
    pub preview: Option<Ref>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrackView {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub release_group: Option<String>,
    pub length_ms: Option<u64>,
    pub added_at: Option<i64>,
    pub status: &'static str,
    pub track: Option<Ref>,
    pub request: Option<Request>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistView {
    pub id: String,
    pub name: String,
    pub owner: Option<String>,
    pub collaborative: bool,
    pub cover: Option<String>,
    pub of: u32,
    pub have: u32,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracks: Option<Vec<TrackView>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtistView {
    pub id: String,
    pub name: String,
    pub photo: Option<String>,
    pub status: &'static str,
    pub have: u32,
    pub artist: Option<ArtistRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArtistRef {
    pub id: String,
}

/// Matching one account's lists.
pub struct Marks<'a> {
    pub library: &'a Library,
    pub answers: &'a Answers,
    /// The account's requests by their key (a release group, else `artist|album` keys), the latest first.
    pub requests: HashMap<String, Request>,
}

/// An open request (asked for, not declined nor taken back) makes a missing item *requested*.
fn open(r: &Request) -> bool {
    r.status == crate::app::requests::PENDING || r.status == crate::app::requests::APPROVED
}

pub fn requests_by_key(all: Vec<Request>) -> HashMap<String, Request> {
    let mut out = HashMap::new();
    for r in all {
        out.entry(r.album_key.clone()).or_insert(r);
    }
    out
}

impl Marks<'_> {
    fn song(&self, t: &Track, artist: &str, album: Option<&str>) -> Option<String> {
        let length = t.length_ms.map(|ms| ms as f64 / 1000.0);
        let recordings = t.isrc.as_deref().map(|i| self.answers.recordings(i)).unwrap_or_default();
        let wanted = |recording| Wanted { recording, artist, title: &t.title, album, length_secs: length };
        recordings.iter().find_map(|r| self.library.matcher.find(&wanted(Some(r.as_str())))).or_else(|| self.library.matcher.find(&wanted(None))).map(str::to_string)
    }

    fn request(&self, release_group: Option<&str>, artist: &str, album: &str) -> Option<Request> {
        release_group
            .and_then(|rg| self.requests.get(rg))
            .or_else(|| self.requests.get(&format!("{}|{}", key(artist), key(album))))
            .cloned()
    }

    fn status(&self, have: u32, of: u32, found: bool, request: Option<&Request>) -> &'static str {
        let there = if of > 0 { have == of } else { found };
        if there {
            IN
        } else if request.is_some_and(open) {
            REQUESTED
        } else if have > 0 || found {
            PARTLY
        } else {
            MISSING
        }
    }

    pub fn album(&self, a: &Album, cover: Option<String>) -> AlbumView {
        let release = a.upc.as_deref().map(|u| self.answers.release(u)).unwrap_or_default();
        let mut found = release.releases.iter().find_map(|r| self.library.by_release.get(&r.to_lowercase())).cloned();
        found = found.or_else(|| self.library.matcher.find_album(&a.title, Some(&a.artist)).map(str::to_string));
        let mut have = 0;
        for t in &a.tracks {
            let track = Track { title: t.title.clone(), length_ms: t.length_ms, ..Default::default() };
            if let Some(song) = self.song(&track, &a.artist, Some(&a.title)) {
                have += 1;
                if found.is_none() {
                    found = self.library.album_of.get(&song).cloned();
                }
            }
        }
        let of = a.tracks.len() as u32;
        let release_group = release.release_group.or_else(|| self.answers.named(&a.artist, &a.title));
        let request = self.request(release_group.as_deref(), &a.artist, &a.title);
        let status = self.status(have, of, found.is_some(), request.as_ref());
        AlbumView {
            id: a.id.clone(),
            title: a.title.clone(),
            artist: a.artist.clone(),
            year: a.year,
            cover,
            added_at: a.added_at,
            status,
            have,
            of,
            release_group,
            album: found.map(|id| Ref { source: self.library.source.clone(), id }),
            // In the library, it is the library's (the owner, 2026-10-30): requested only on the Requests page.
            request: request.filter(|_| status != IN),
            preview: None,
        }
    }

    pub fn track(&self, t: &Track) -> TrackView {
        let song = self.song(t, &t.artist, Some(&t.album).filter(|a| !a.is_empty()).map(String::as_str));
        let album_artist = t.album_artist.as_deref().unwrap_or(&t.artist);
        let release_group = self.answers.named(album_artist, &t.album);
        let request = self.request(release_group.as_deref(), album_artist, &t.album);
        let status = self.status(u32::from(song.is_some()), 1, song.is_some(), request.as_ref());
        TrackView {
            id: t.id.clone(),
            title: t.title.clone(),
            artist: t.artist.clone(),
            album: t.album.clone(),
            release_group,
            length_ms: t.length_ms,
            added_at: t.added_at,
            status,
            track: song.map(|id| Ref { source: self.library.source.clone(), id }),
            request: request.filter(|_| status != IN),
        }
    }

    pub fn playlist(&self, p: &Playlist, cover: Option<String>, with_tracks: bool) -> PlaylistView {
        let tracks: Vec<TrackView> = p.tracks.iter().map(|t| self.track(t)).collect();
        let of = tracks.len() as u32;
        let have = tracks.iter().filter(|t| t.status == IN).count() as u32;
        let status = if have == of { IN } else if have > 0 { PARTLY } else { MISSING };
        PlaylistView {
            id: p.id.clone(),
            name: p.name.clone(),
            owner: p.owner.clone(),
            collaborative: p.collaborative,
            cover,
            of,
            have,
            status,
            tracks: with_tracks.then_some(tracks),
        }
    }

    pub fn artist(&self, a: &Artist, photo: Option<String>) -> ArtistView {
        let here = self.library.artists.get(&key(&a.name)).filter(|(_, albums)| *albums > 0);
        ArtistView {
            id: a.id.clone(),
            name: a.name.clone(),
            photo,
            status: if here.is_some() { IN } else { MISSING },
            have: here.map_or(0, |(_, n)| *n),
            artist: here.map(|(id, _)| ArtistRef { id: id.clone() }),
        }
    }
}

/// What MusicBrainz is to be asked about the lists: their ISRCs, UPCs, and the names of albums without a UPC.
pub fn questions(lists: &Lists) -> Vec<String> {
    let mut out = Vec::new();
    for a in &lists.albums {
        match &a.upc {
            Some(u) => out.push(upc_key(u)),
            None => out.push(name_key(&a.artist, &a.title)),
        }
    }
    for t in lists.tracks.iter().chain(lists.playlists.iter().flat_map(|p| &p.tracks)) {
        if let Some(i) = &t.isrc {
            out.push(isrc_key(i));
        }
        if !t.album.is_empty() {
            out.push(name_key(t.album_artist.as_deref().unwrap_or(&t.artist), &t.album));
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Library {
        let mut matcher = Matcher::default();
        matcher.add("s1", "Zombie", Some("The Cranberries"), "No Need To Argue", Some("rec-zombie"), Some(306.0));
        matcher.add_album("al1", "No Need To Argue", Some("The Cranberries"));
        Library {
            source: "cratarr".into(),
            matcher,
            by_release: HashMap::from([("rel-nnta".into(), "al1".into())]),
            album_of: HashMap::from([("s1".into(), "al1".into())]),
            artists: HashMap::from([(key("The Cranberries"), ("ar1".into(), 3))]),
        }
    }

    fn request(key: &str, status: &str) -> Request {
        serde_json::from_value(serde_json::json!({ "id": 1, "user": "me", "album_key": key, "artist_key": "", "release_group": null,
            "album": "Homogenic", "artist": "Björk", "year": null, "cover": null, "at": 0, "status": status, "reason": null })).unwrap()
    }

    #[test]
    fn items_are_in_partly_missing_or_requested() {
        let lib = library();
        let answers = Answers(HashMap::from([
            (isrc_key("IEACJ9400015"), r#"["rec-zombie"]"#.to_string()),
            (upc_key("0731452280826"), r#"{"releases":["rel-nnta"],"release_group":"rg-nnta"}"#.to_string()),
            (name_key("Björk", "Homogenic"), r#""rg-homogenic""#.to_string()),
        ]));
        let marks = Marks { library: &lib, answers: &answers, requests: requests_by_key(vec![request("rg-homogenic", "pending")]) };
        // By ISRC, though the title differs.
        let t = Track { id: "t1".into(), title: "Zombie (Remastered)".into(), artist: "The Cranberries".into(), album: "No Need To Argue".into(), isrc: Some("IEACJ9400015".into()), ..Default::default() };
        let v = marks.track(&t);
        assert_eq!((v.status, v.track), (IN, Some(Ref { source: "cratarr".into(), id: "s1".into() })));
        // By UPC, two tracks of which one is here.
        let a = Album { id: "a1".into(), title: "No Need To Argue (Deluxe)".into(), artist: "The Cranberries".into(), upc: Some("731452280826".into()),
            tracks: vec![super::super::AlbumTrack { title: "Zombie".into(), length_ms: Some(306_000) }, super::super::AlbumTrack { title: "Ode to My Family".into(), length_ms: None }], ..Default::default() };
        let v = marks.album(&a, None);
        assert_eq!((v.status, v.have, v.of, v.release_group.as_deref()), (PARTLY, 1, 2, Some("rg-nnta")));
        assert_eq!(v.album.unwrap().id, "al1");
        // An export's album (no tracks, no UPC), asked for.
        let a = Album { id: "x.b|h".into(), title: "Homogenic".into(), artist: "Björk".into(), ..Default::default() };
        let v = marks.album(&a, None);
        assert_eq!((v.status, v.release_group.as_deref()), (REQUESTED, Some("rg-homogenic")));
        // Declined: missing again.
        let marks = Marks { library: &lib, answers: &answers, requests: requests_by_key(vec![request("rg-homogenic", "declined")]) };
        assert_eq!(marks.album(&a, None).status, MISSING);
        assert_eq!(marks.artist(&Artist { id: "x".into(), name: "the cranberries".into(), photo: None }, None).have, 3);
    }
}
