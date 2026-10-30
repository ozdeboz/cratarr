//! What Discogs says of the library's albums and artists, and of Discover's release groups (0.9.8, the owner: Missing
//! genres lacked Discogs' genres and styles; labels and catalogue numbers, credits, artist profiles; Discover's genres),
//! kept a while (`UserData::discogs_facts`). An album is asked when its fixes or its page are read, and in the background
//! for the albums missing a genre or a year; a release group in a Discover look. An album's genres and styles go into
//! the library's words as Discogs' too (as the harvest's), so every place the words reach has them.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;

use super::discogs::{AlbumFacts, ArtistFacts};
use crate::userdata::words::RawWord;

/// Asked again after this long; where Discogs had nothing, sooner.
const AGAIN: i64 = 30 * 86_400;
const NONE_AGAIN: i64 = 7 * 86_400;
/// Albums asked in one background round.
const ROUND: usize = 60;

fn due(kept: Option<&(i64, Option<String>)>, now: i64) -> bool {
    kept.is_none_or(|(at, facts)| now - at >= if facts.is_some() { AGAIN } else { NONE_AGAIN })
}

fn parse<T: serde::de::DeserializeOwned>(kept: Option<(i64, Option<String>)>) -> Option<T> {
    kept.and_then(|(_, f)| f).and_then(|f| serde_json::from_str(&f).ok())
}

/// An album's subject: its Music-form id, as its words are kept.
fn album_subject(served: &crate::serve::Serve, album: &str) -> Result<String> {
    Ok(format!("album:{}", served.library.album_base(album)?.unwrap_or_else(|| album.to_string())))
}

/// An album's facts as kept, not asked for. Blocking.
pub fn album_kept(served: &crate::serve::Serve, album: &str) -> Result<Option<AlbumFacts>> {
    Ok(parse(served.userdata.discogs_facts(&album_subject(served, album)?)?))
}

/// Every album's labels as kept from Discogs, by its Music-form id (0.11, quick crates: the crates' Label where an
/// album's files name none): what the rounds kept so far, never asked for here. Blocking.
pub fn kept_labels(served: &crate::serve::Serve) -> Result<HashMap<String, Vec<String>>> {
    #[derive(serde::Deserialize)]
    struct Labelled {
        #[serde(default)]
        labels: Vec<super::discogs::Label>,
    }
    let mut out = HashMap::new();
    for (subject, json) in served.userdata.discogs_facts_of("album:")? {
        let (Some(base), Ok(kept)) = (subject.strip_prefix("album:"), serde_json::from_str::<Labelled>(&json)) else { continue };
        let names = label_names(&kept.labels);
        if !names.is_empty() {
            out.insert(base.to_string(), names);
        }
    }
    Ok(out)
}

/// Labels' names as the crates offer them: each once, Discogs' "Not On Label" (self-released, no label) none.
fn label_names(labels: &[super::discogs::Label]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in labels {
        let name = l.name.trim();
        if name.is_empty() || name.to_lowercase().starts_with("not on label") || out.iter().any(|x| x.eq_ignore_ascii_case(name)) {
            continue;
        }
        out.push(name.to_string());
    }
    out
}

/// An album's facts: as kept while fresh, else asked now (MusicBrainz's link from its release, else Discogs' search by
/// its artist and name).
pub async fn album(served: &Arc<crate::serve::Serve>, album: &str) -> Result<Option<AlbumFacts>> {
    let (s, a) = (served.clone(), album.to_string());
    let (subject, kept, place) = tokio::task::spawn_blocking(move || -> Result<_> {
        let subject = album_subject(&s, &a)?;
        Ok((subject.clone(), s.userdata.discogs_facts(&subject)?, s.library.album_place(&a)?))
    })
    .await??;
    let now = super::moment::now_secs();
    if !due(kept.as_ref(), now) {
        return Ok(parse(kept));
    }
    let Some((_, _, release, artist, name)) = place else { return Ok(None) };
    let group = match release.filter(|r| super::discogs::is_mbid(r)) {
        Some(r) => super::discogs::release_group_of(&super::discover::http()?, &r).await.ok().flatten(),
        None => None,
    };
    let facts = super::discogs::album_facts(group.as_deref(), &artist, &name).await?;
    let words: Vec<RawWord> = facts
        .iter()
        .flat_map(|f| {
            let genres = f.genres.iter().map(|g| RawWord { word: g.clone(), kind: "genre".into(), weight: 1.0 });
            genres.chain(f.styles.iter().map(|s| RawWord { word: s.clone(), kind: "style".into(), weight: 1.0 }))
        })
        .collect();
    let (s, json) = (served.clone(), facts.as_ref().map(serde_json::to_string).transpose()?);
    tokio::task::spawn_blocking(move || -> Result<()> {
        s.userdata.set_discogs_facts(&subject, json.as_deref(), now)?;
        s.userdata.set_words(&subject, "discogs", &words, true, now)
    })
    .await??;
    Ok(facts)
}

/// An artist's facts, by their MusicBrainz id where known, else their name.
pub async fn artist(served: &Arc<crate::serve::Serve>, name: &str, mbid: Option<&str>) -> Result<Option<ArtistFacts>> {
    let subject = format!("artist:{}", super::words::key(name));
    let (ud, key) = (served.userdata.clone(), subject.clone());
    let kept = tokio::task::spawn_blocking(move || ud.discogs_facts(&key)).await??;
    let now = super::moment::now_secs();
    if !due(kept.as_ref(), now) {
        return Ok(parse(kept));
    }
    let facts = super::discogs::artist_facts(mbid.filter(|m| super::discogs::is_mbid(m)), name).await?;
    let (ud, json) = (served.userdata.clone(), facts.as_ref().map(serde_json::to_string).transpose()?);
    tokio::task::spawn_blocking(move || ud.set_discogs_facts(&subject, json.as_deref(), now)).await??;
    Ok(facts)
}

/// A release group's genres and styles (Discover).
pub async fn release_group(served: &Arc<crate::serve::Serve>, group: &str) -> Result<Option<AlbumFacts>> {
    let subject = format!("rg:{group}");
    let (ud, key) = (served.userdata.clone(), subject.clone());
    let kept = tokio::task::spawn_blocking(move || ud.discogs_facts(&key)).await??;
    let now = super::moment::now_secs();
    if !due(kept.as_ref(), now) {
        return Ok(parse(kept));
    }
    let facts = super::discogs::release_group_facts(group).await?;
    let (ud, json) = (served.userdata.clone(), facts.as_ref().map(serde_json::to_string).transpose()?);
    tokio::task::spawn_blocking(move || ud.set_discogs_facts(&subject, json.as_deref(), now)).await??;
    Ok(facts)
}

/// A round of albums asked in the background, one round at a time (Missing genres and years: those not asked yet,
/// shown the next time the fixes are read).
pub fn fill(served: Arc<crate::serve::Serve>, albums: Vec<String>) {
    static RUNNING: AtomicBool = AtomicBool::new(false);
    if albums.is_empty() || RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async move {
        let s = served.clone();
        let todo = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
            let now = super::moment::now_secs();
            let mut out = Vec::new();
            for a in albums {
                if due(s.userdata.discogs_facts(&album_subject(&s, &a)?)?.as_ref(), now) {
                    out.push(a);
                }
                if out.len() == ROUND {
                    break;
                }
            }
            Ok(out)
        })
        .await;
        for a in todo.ok().and_then(Result::ok).unwrap_or_default() {
            if let Err(e) = album(&served, &a).await {
                tracing::debug!("discogs: {a}: {e:#}");
            }
        }
        RUNNING.store(false, Ordering::SeqCst);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::discogs::Label;

    #[test]
    fn discogs_labels_are_named_once_and_not_on_label_is_no_label() {
        let l = |name: &str| Label { name: name.into(), catno: None };
        let names = label_names(&[l("Warp Records"), l("warp records"), l("Not On Label (Air Self-released)"), l(" "), l("Bleep")]);
        assert_eq!(names, ["Warp Records", "Bleep"]);
    }
}
