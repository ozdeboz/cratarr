//! Spotify's data export (decision 0073, amended), for every account, free ones too: the "Account data" zip's
//! `YourLibrary.json` (`tracks`: `{ artist, album, track, uri }`; `albums` and `artists` read where they are, their keys
//! as Spotify's help page names them) and `Playlist1.json`… (`playlists`: `{ name, lastModifiedDate, items: [{ track:
//! { trackName, artistName, albumName, trackUri }, addedDate }] }`). No ISRCs nor UPCs, no covers, no followed
//! playlists. Its items keep Spotify's ids where a URI gives them, so they meet the live lists' own.

use serde_json::Value;

use super::{Album, Artist, Lists, Playlist, Track};

/// Whether a file of the export holds library lists (not its streaming history).
pub fn is_library(name: &str) -> bool {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name).to_lowercase();
    base == "yourlibrary.json" || (base.starts_with("playlist") && base.ends_with(".json") && base[8..base.len() - 5].chars().all(|c| c.is_ascii_digit()))
}

/// Whether a JSON object is one of those files (uploaded alone, not in its zip).
pub fn looks_like(v: &Value) -> bool {
    v.get("playlists").is_some_and(Value::is_array) || (v.get("tracks").is_some_and(Value::is_array) && v.get("tracks").and_then(|t| t.get(0)).is_none_or(|t| t.get("track").is_some()))
}

/// The lists the export's library files hold, in their order; `None` when none of them was there.
pub fn lists(files: &[Value]) -> Option<Lists> {
    if files.is_empty() {
        return None;
    }
    let mut out = Lists::default();
    for f in files {
        for t in f["tracks"].as_array().into_iter().flatten() {
            let (Some(title), Some(artist)) = (text(t, "track"), text(t, "artist")) else { continue };
            let album = text(t, "album").unwrap_or_default();
            out.tracks.push(track(id_of(t.get("uri"), "track", &[&artist, &album, &title]), title, artist, album, None));
        }
        for a in f["albums"].as_array().into_iter().flatten() {
            let (Some(title), Some(artist)) = (text(a, "album"), text(a, "artist")) else { continue };
            out.albums.push(Album { id: id_of(a.get("uri"), "album", &[&artist, &title]), title, artist, ..Default::default() });
        }
        for a in f["artists"].as_array().into_iter().flatten() {
            let Some(name) = text(a, "name").or_else(|| text(a, "artist")) else { continue };
            out.artists.push(Artist { id: id_of(a.get("uri"), "artist", &[&name]), name, photo: None });
        }
        for p in f["playlists"].as_array().into_iter().flatten() {
            let Some(name) = text(p, "name") else { continue };
            let tracks = p["items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| {
                    let t = e.get("track").filter(|t| !t.is_null())?;
                    let (title, artist) = (text(t, "trackName")?, text(t, "artistName")?);
                    let album = text(t, "albumName").unwrap_or_default();
                    let added = text(e, "addedDate").and_then(|d| crate::nowplaying::parse_iso_epoch(&format!("{d}T00:00:00Z")));
                    Some(track(id_of(t.get("trackUri"), "track", &[&artist, &album, &title]), title, artist, album, added))
                })
                .collect();
            // No id of its own: one made from its name, the same at every upload (a second of one name numbered).
            let base = format!("x.{}", crate::app::import::key(&name));
            let n = out.playlists.iter().filter(|q| q.id == base || q.id.starts_with(&format!("{base}.")) ).count();
            let id = if n == 0 { base } else { format!("{base}.{n}") };
            out.playlists.push(Playlist { id, name, owner: None, collaborative: false, cover: None, tracks });
        }
    }
    Some(out)
}

fn track(id: String, title: String, artist: String, album: String, added_at: Option<i64>) -> Track {
    Track { id, title, artist, album, added_at, ..Default::default() }
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Spotify's id from its URI (`spotify:track:…`); without one, an id made from the names (`x.` first: none of
/// Spotify's own starts so).
fn id_of(uri: Option<&Value>, kind: &str, names: &[&str]) -> String {
    let prefix = format!("spotify:{kind}:");
    if let Some(id) = uri.and_then(Value::as_str).and_then(|u| u.strip_prefix(&prefix)).filter(|id| id.chars().all(|c| c.is_ascii_alphanumeric())) {
        return id.to_string();
    }
    format!("x.{}", names.iter().map(|n| crate::app::import::key(n)).collect::<Vec<_>>().join("|"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exports_library_and_playlists_are_read_with_spotifys_ids() {
        let library = serde_json::json!({ "tracks": [
            { "artist": "The Cranberries", "album": "No Need To Argue", "track": "Zombie", "uri": "spotify:track:2IZZqH4K02UIYg5EohpNHF" },
            { "artist": "Björk", "album": "Post", "track": "Hyperballad" } ],
          "albums": [{ "artist": "Björk", "album": "Homogenic", "uri": "spotify:album:abc123" }],
          "artists": [{ "name": "Portishead", "uri": "spotify:artist:def456" }],
          "shows": [], "episodes": [], "bannedTracks": [] });
        let playlists = serde_json::json!({ "playlists": [{ "name": "TEST", "lastModifiedDate": "2020-10-20", "items": [
            { "track": { "trackName": "Year of the Young", "artistName": "Smith & Thell", "albumName": "Year of the Young",
              "trackUri": "spotify:track:0cgyeBU54kjmI54TflMANg" }, "episode": null, "localTrack": null, "addedDate": "2024-08-17" },
            { "track": null, "episode": { "episodeName": "A podcast" }, "localTrack": null } ],
          "description": null, "numberOfFollowers": 0 }, { "name": "TEST", "items": [] }] });
        assert!(looks_like(&library) && looks_like(&playlists));
        assert!(is_library("MyData/YourLibrary.json") && is_library("Playlist12.json") && !is_library("StreamingHistory0.json"));
        let l = lists(&[library, playlists]).unwrap();
        assert_eq!(l.tracks[0].id, "2IZZqH4K02UIYg5EohpNHF");
        assert!(l.tracks[1].id.starts_with("x."));
        assert_eq!((l.albums[0].id.as_str(), l.artists[0].id.as_str()), ("abc123", "def456"));
        let p = &l.playlists[0];
        assert_eq!((p.name.as_str(), p.tracks.len(), p.tracks[0].id.as_str()), ("TEST", 1, "0cgyeBU54kjmI54TflMANg"));
        assert_eq!(p.tracks[0].added_at, Some(1_723_852_800));
        assert_ne!(l.playlists[0].id, l.playlists[1].id, "two of one name");
    }
}
