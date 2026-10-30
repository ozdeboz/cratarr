//! Cratarr describes the tracks it is told of (0.9.6, decision 0033 amended; the owner, after "Go to album" went
//! missing on the full-screen player): a queue holds a track's identity (its source and id, its entry, lane and place),
//! and Cratarr says what it is, wherever it is shown, whatever a player reported. Cratarr's own tracks (and other
//! servers' ids that are files of its own) from the catalogue, their favourite from the account's own data; a Bandcamp
//! preview from its album's page; another server's track as it was reported.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// The most tracks described in one call.
pub const MOST: usize = 500;

#[derive(Debug, Clone, Deserialize)]
pub struct Asked {
    pub source: String,
    pub id: String,
}

/// A track as Cratarr describes it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Described {
    pub source: String,
    pub id: String,
    pub name: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_id: Option<String>,
    pub duration_secs: Option<f64>,
    /// Whether it can be added to a playlist: a file Cratarr can see.
    pub addable: bool,
    /// Whether the account has it as a favourite; `None` where that is not known (another server's, a preview).
    pub favorite: Option<bool>,
    /// Whether Cratarr knows it at all.
    pub found: bool,
    /// Its file, as the catalogue has it (a playlist's line is made from it).
    #[serde(skip)]
    pub path: Option<String>,
    /// Its id in Cratarr's own library (another server's track that is a file of Cratarr's).
    #[serde(skip)]
    pub own: Option<String>,
}

/// Cratarr's own tracks among these (by another server's id too), described from the catalogue, with `account`'s
/// favourites; the others `None`. Blocking (the catalogue, the account's data).
pub fn ours(serve: &crate::serve::Serve, asked: &[(String, String)], account: Option<&str>) -> Result<Vec<Option<Described>>> {
    let own: Vec<Option<String>> = asked
        .iter()
        .map(|(source, id)| {
            if source == crate::source::cratarr::KIND.id {
                Some(id.clone())
            } else if source == crate::bandcamp::SOURCE {
                None
            } else {
                serve.catalogue().track_of_source(source, id).map(crate::serve::ids::song)
            }
        })
        .collect();
    let ids: Vec<String> = own.iter().flatten().cloned().collect::<HashSet<_>>().into_iter().collect();
    if ids.is_empty() {
        return Ok(vec![None; asked.len()]);
    }
    let songs: HashMap<String, crate::serve::api::Song> = serve.library.songs_by_id(&ids)?.into_iter().map(|s| (s.id.clone(), s)).collect();
    let starred: HashSet<String> = match account {
        Some(a) => serve.userdata.starred(a, crate::userdata::Kind::Song)?.into_iter().collect(),
        None => HashSet::new(),
    };
    Ok(asked
        .iter()
        .zip(&own)
        .map(|((source, id), o)| {
            let s = songs.get(o.as_ref()?)?;
            Some(Described {
                source: source.clone(),
                id: id.clone(),
                name: s.title.clone(),
                artist: Some(s.artist.clone()).filter(|a| !a.is_empty()),
                album: Some(s.album.clone()).filter(|a| !a.is_empty()),
                album_id: Some(s.album_id.clone()),
                duration_secs: Some(s.duration as f64).filter(|d| *d > 0.0),
                addable: true,
                favorite: account.map(|_| starred.contains(&s.id)),
                found: true,
                path: Some(s.path.clone()),
                own: Some(s.id.clone()),
            })
        })
        .collect())
}

/// A Bandcamp preview's track from its album's page (read in turn, kept a few minutes): its album is the page's
/// (`track_id(page, 0)`, decision 0071).
pub(crate) async fn bandcamp(id: &str) -> Option<Described> {
    let (page, n) = crate::bandcamp::read_id(id)?;
    let a = crate::bandcamp::Bandcamp::get().album(&page).await.ok()?;
    let t = a.tracks.iter().find(|t| t.n == n)?;
    Some(Described {
        source: crate::bandcamp::SOURCE.to_string(),
        id: id.to_string(),
        name: t.title.clone(),
        artist: t.artist.clone().or(Some(a.artist.clone())),
        album: Some(a.title.clone()),
        album_id: Some(crate::bandcamp::track_id(&page, 0)),
        duration_secs: t.duration_secs,
        addable: false,
        favorite: None,
        found: true,
        path: None,
        own: None,
    })
}

/// These tracks described, in the order asked: Cratarr's own from the catalogue, Bandcamp's from their pages, the rest
/// unknown (`found: false`).
pub async fn describe(asked: Vec<Asked>, account: Option<String>) -> Result<Vec<Described>> {
    let pairs: Vec<(String, String)> = asked.iter().map(|a| (a.source.clone(), a.id.clone())).collect();
    let mut out = match crate::serve::instance() {
        Some(serve) => {
            let p = pairs.clone();
            tokio::task::spawn_blocking(move || ours(&serve, &p, account.as_deref())).await??
        }
        None => vec![None; pairs.len()],
    };
    for (i, (source, id)) in pairs.iter().enumerate() {
        if out[i].is_none() && source == crate::bandcamp::SOURCE {
            out[i] = bandcamp(id).await;
        }
    }
    Ok(out
        .into_iter()
        .zip(pairs)
        .map(|(d, (source, id))| d.unwrap_or(Described { source, id, ..Described::default() }))
        .collect())
}

/// A queue's tracks with what Cratarr knows of them in place of what was reported (Cratarr's own; the rest as they
/// are). Blocking.
pub fn fill(serve: &crate::serve::Serve, tracks: &mut [super::QueueTrack]) {
    let pairs: Vec<(String, String)> = tracks.iter().map(|t| (t.source.clone(), t.id.clone())).collect();
    let Ok(found) = ours(serve, &pairs, None) else { return };
    for (t, d) in tracks.iter_mut().zip(found) {
        if let Some(d) = d {
            t.name = d.name;
            t.artist = d.artist;
            t.album = d.album;
            t.album_id = d.album_id;
            t.duration_secs = d.duration_secs.or(t.duration_secs);
        }
    }
}

/// A player's view with its track and queue described.
pub fn fill_view(view: &mut super::playon::PlayerView) {
    let Some(serve) = crate::serve::instance() else { return };
    if let Some(t) = view.track.as_mut() {
        fill(&serve, std::slice::from_mut(t));
        if view.mode == "cratarr" {
            (view.title, view.artist, view.album) = (Some(t.name.clone()).filter(|n| !n.is_empty()), t.artist.clone(), t.album.clone());
        }
    }
    if let Some(q) = view.queue.as_mut() {
        fill(&serve, q);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_unknown_track_is_said_to_be_unknown_in_its_place() {
        let d = describe(vec![Asked { source: "elsewhere".into(), id: "x1".into() }, Asked { source: "elsewhere".into(), id: "x2".into() }], None).await.unwrap();
        assert_eq!(d.iter().map(|d| (d.id.as_str(), d.found)).collect::<Vec<_>>(), [("x1", false), ("x2", false)]);
    }
}
