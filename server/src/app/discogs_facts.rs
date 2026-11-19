//! What Discogs says of the library's albums and artists, and of Discover's release groups (0.9.8, the owner: Missing
//! genres lacked Discogs' genres and styles; labels and catalogue numbers, credits, artist profiles; Discover's genres),
//! kept a while (`UserData::discogs_facts`). An album is asked when its fixes or its page are read, and in the background
//! for the albums missing a genre or a year; a release group in a Discover look. An album's genres and styles go into
//! the library's words as Discogs' too (as the harvest's), so every place the words reach has them.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;

use super::discogs::{AlbumFacts, ArtistFacts, MbLabel};
use crate::catalogue::records::Ref;
use crate::userdata::words::RawWord;

/// Asked again after this long; where Discogs had nothing, sooner.
const AGAIN: i64 = 30 * 86_400;
const NONE_AGAIN: i64 = 7 * 86_400;
/// Asked again after this long where MusicBrainz's link could not be had (F47): what Discogs' search found stands in till then.
const LINK_AGAIN: i64 = 15 * 60;
/// Albums asked in one background round.
const ROUND: usize = 60;

fn due(kept: Option<&(i64, Option<String>)>, now: i64) -> bool {
    kept.is_none_or(|(at, facts)| now - at >= if facts.is_some() { AGAIN } else { NONE_AGAIN })
}

/// The time facts read at `now` are kept as read: now; where MusicBrainz's link could not be had (F47), as long before as makes them
/// due again after `LINK_AGAIN`.
fn kept_at(now: i64, found: bool, link_unread: bool) -> i64 {
    if link_unread { now - if found { AGAIN } else { NONE_AGAIN } + LINK_AGAIN } else { now }
}

fn parse<T: serde::de::DeserializeOwned>(kept: Option<(i64, Option<String>)>) -> Option<T> {
    kept.and_then(|(_, f)| f).and_then(|f| serde_json::from_str(&f).ok())
}

/// An album's subject, as its words are kept.
fn album_subject(album: &str) -> String {
    format!("album:{album}")
}

/// An album's facts as kept, not asked for. Blocking.
pub fn album_kept(served: &crate::serve::Serve, album: &str) -> Result<Option<AlbumFacts>> {
    Ok(parse(served.userdata.discogs_facts(&album_subject(album))?))
}

/// Every album's labels as kept from Discogs, by its id (0.11, quick crates: the crates' Label where an
/// album's files name none): what the rounds kept so far, never asked for here. Blocking.
pub fn kept_labels(served: &crate::serve::Serve) -> Result<HashMap<String, Vec<String>>> {
    let mut out = HashMap::new();
    for (subject, json) in served.userdata.discogs_facts_of("album:")? {
        let (Some(album), Ok(kept)) = (subject.strip_prefix("album:"), serde_json::from_str::<Labelled>(&json)) else { continue };
        let names = label_names(&kept.labels);
        if !names.is_empty() {
            out.insert(album.to_string(), names);
        }
    }
    Ok(out)
}

/// An album's facts as read for its labels alone (the crates' Label, F9's `label_named`).
#[derive(serde::Deserialize)]
struct Labelled {
    #[serde(default)]
    labels: Vec<super::discogs::Label>,
}

/// Where an album's labels at MusicBrainz are kept (F9): beside its Discogs facts, as the files' release names them.
const MUSICBRAINZ_LABELS: &str = "musicbrainz-labels:";

fn musicbrainz_subject(album: &str) -> String {
    format!("{MUSICBRAINZ_LABELS}{album}")
}

/// An album's labels as its files' MusicBrainz release names them (F9), as kept beside its facts; none where none were read.
/// Blocking.
pub fn musicbrainz_labels(served: &crate::serve::Serve, album: &str) -> Result<Vec<MbLabel>> {
    Ok(parse(served.userdata.discogs_facts(&musicbrainz_subject(album))?).unwrap_or_default())
}

/// `GET /api/albums/{source}/{album}/discogs`'s answer (F9): Discogs' facts as given (`album`), their `labels` as About shows them
/// (`labels::about`: Discogs' with their ids, then MusicBrainz's of the files' release, kept beside them, of a name Discogs names
/// none of); `{}` where neither says anything. Blocking.
pub fn answer(served: &crate::serve::Serve, album: &str, facts: Option<&AlbumFacts>) -> Result<serde_json::Value> {
    answer_with(served.catalogue(), facts, &musicbrainz_labels(served, album)?)
}

/// `answer`, MusicBrainz's labels given. Blocking.
fn answer_with(cat: &crate::catalogue::Catalogue, facts: Option<&AlbumFacts>, musicbrainz: &[MbLabel]) -> Result<serde_json::Value> {
    let labels = super::labels::about(cat, facts.map_or(&[][..], |f| f.labels.as_slice()), musicbrainz)?;
    let mut answer = match facts {
        Some(f) => serde_json::to_value(f)?,
        None if labels.is_empty() => return Ok(serde_json::json!({})),
        None => serde_json::json!({}),
    };
    if let Some(o) = answer.as_object_mut() {
        o.insert("labels".into(), serde_json::to_value(labels)?);
    }
    Ok(answer)
}

/// The name a label's reference was kept with (F9: who may make a label by it, `labels::open_label`): an album's label at Discogs by
/// its id (its facts), or at MusicBrainz by its MBID (its release's, kept beside them); none where nothing kept names it. Blocking.
pub fn label_named(served: &crate::serve::Serve, r: &Ref) -> Result<Option<(String, bool)>> {
    if r.kind != "label" {
        return Ok(None);
    }
    match r.source.as_str() {
        "discogs" => {
            let Ok(id) = r.value.parse::<u64>() else { return Ok(None) };
            for (_, json) in served.userdata.discogs_facts_of("album:")? {
                if let Ok(kept) = serde_json::from_str::<Labelled>(&json)
                    && let Some(l) = kept.labels.into_iter().find(|l| l.id == Some(id))
                {
                    return Ok(Some((l.name, l.namesake)));
                }
            }
        }
        "musicbrainz" => {
            for (_, json) in served.userdata.discogs_facts_of(MUSICBRAINZ_LABELS)? {
                if let Ok(kept) = serde_json::from_str::<Vec<MbLabel>>(&json)
                    && let Some(l) = kept.into_iter().find(|l| l.mbid == r.value)
                {
                    return Ok(Some((l.name, false)));
                }
            }
        }
        _ => {}
    }
    Ok(None)
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
    let (subject, kept, names) = tokio::task::spawn_blocking(move || -> Result<_> {
        let subject = album_subject(&a);
        Ok((subject.clone(), s.userdata.discogs_facts(&subject)?, s.library.album_names(&a)?))
    })
    .await??;
    let now = super::moment::now_secs();
    if !due(kept.as_ref(), now) {
        return Ok(parse(kept));
    }
    let Some((release, artist, name)) = names else { return Ok(None) };
    // The files' release's group, and its labels at MusicBrainz (F9): the one question, as before.
    let (group, musicbrainz) = match release.filter(|r| super::discogs::is_mbid(r)) {
        Some(r) => match super::discogs::release_group_and_labels(&super::discover::http()?, &r).await {
            Ok((group, labels)) => (group, Some(labels)),
            Err(_) => (None, None),
        },
        None => (None, None),
    };
    let facts = super::discogs::album_facts(group.as_deref(), &artist, &name).await?;
    let words: Vec<RawWord> = facts
        .iter()
        .flat_map(|f| {
            let genres = f.genres.iter().map(|g| RawWord { word: g.clone(), kind: "genre".into(), weight: 1.0 });
            genres.chain(f.styles.iter().map(|s| RawWord { word: s.clone(), kind: "style".into(), weight: 1.0 }))
        })
        .collect();
    let (s, json, a, kept) = (served.clone(), facts.as_ref().map(serde_json::to_string).transpose()?, album.to_string(), facts.clone());
    let labels = musicbrainz.as_ref().map(serde_json::to_string).transpose()?;
    tokio::task::spawn_blocking(move || -> Result<()> {
        // The release's labels at MusicBrainz, kept beside its facts (F9), where MusicBrainz answered.
        if let Some(labels) = &labels {
            s.userdata.set_discogs_facts(&musicbrainz_subject(&a), Some(labels), now)?;
        }
        s.userdata.set_discogs_facts(&subject, json.as_deref(), now)?;
        s.userdata.set_words(&subject, "discogs", &words, true, now)?;
        // In the references too (C43b), with its main release's list.
        match &kept {
            Some(f) => super::references::keep_discogs(&s, &a, f),
            None => Ok(()),
        }
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
    let (facts, link_unread) = super::discogs::artist_facts(mbid.filter(|m| super::discogs::is_mbid(m)), name).await?;
    let (ud, json) = (served.userdata.clone(), facts.as_ref().map(serde_json::to_string).transpose()?);
    let at = kept_at(now, facts.is_some(), link_unread);
    tokio::task::spawn_blocking(move || ud.set_discogs_facts(&subject, json.as_deref(), at)).await??;
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
                if due(s.userdata.discogs_facts(&album_subject(&a))?.as_ref(), now) {
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

    /// F47: an artist's facts read while MusicBrainz's link could not be had are asked again after a quarter hour, found or not; read
    /// with it, after a month (none found: a week).
    #[test]
    fn facts_read_without_musicbrainzs_link_are_asked_again_soon() {
        let now = 1_800_000_000;
        let kept = |found: bool, unread: bool| (kept_at(now, found, unread), found.then(|| "{}".to_string()));
        for found in [true, false] {
            assert!(!due(Some(&kept(found, true)), now + LINK_AGAIN - 1), "kept a while");
            assert!(due(Some(&kept(found, true)), now + LINK_AGAIN), "asked again after a quarter hour");
            assert!(!due(Some(&kept(found, false)), now + 86_400), "with the link: kept");
        }
        assert!(due(Some(&kept(true, false)), now + AGAIN) && due(Some(&kept(false, false)), now + NONE_AGAIN));
    }

    #[test]
    fn discogs_labels_are_named_once_and_not_on_label_is_no_label() {
        let l = |name: &str| Label { name: name.into(), catno: None, id: None, namesake: false };
        let names = label_names(&[l("Warp Records"), l("warp records"), l("Not On Label (Air Self-released)"), l(" "), l("Bleep")]);
        assert_eq!(names, ["Warp Records", "Bleep"]);
    }

    /// F9.1 (b), (e): Discogs' answer names an album's labels as About shows them: Discogs' each with its catalogue number, by the
    /// label its id is held by (`id`), else that id to open one by (`ref`); "Not On Label" as text; then MusicBrainz's of the files'
    /// release (by their MBIDs) of a name Discogs names none of. The rest of Discogs' facts as before; MusicBrainz's alone where
    /// Discogs has none; `{}` where neither names any. Nothing made by answering.
    #[test]
    fn the_discogs_answer_names_each_label_as_about_shows_it() {
        let cat = crate::catalogue::Catalogue::open(None, Vec::new()).unwrap();
        let held = cat.open_label(&Ref::new("discogs", "label", "23528"), "Low Tide Records", true).unwrap();
        let (anthem, nonesuch) = ("00000000-0000-4000-8000-000000000901", "00000000-0000-4000-8000-000000000902");
        let label = |name: &str, catno: Option<&str>, id: u64| Label { name: name.into(), catno: catno.map(str::to_string), id: Some(id), namesake: false };
        let facts = AlbumFacts {
            master: Some(3001),
            year: Some(2021),
            labels: vec![label("Low Tide Records", Some("LTR-012"), 23528), label("International Anthem", Some("IARC0041"), 4401), label("Not On Label (Kite Theory Self-released)", None, 1818)],
            ..AlbumFacts::default()
        };
        let mb = |name: &str, mbid: &str, catno: Option<&str>| MbLabel { name: name.into(), mbid: mbid.into(), catno: catno.map(str::to_string) };
        let musicbrainz = [mb("International Anthem", anthem, Some("IARC0041")), mb("Nonesuch", nonesuch, Some("075597"))];
        let v = answer_with(&cat, Some(&facts), &musicbrainz).unwrap();
        assert_eq!(
            v["labels"],
            serde_json::json!([
                { "name": "Low Tide Records", "catno": "LTR-012", "id": crate::serve::ids::label(held) },
                { "name": "International Anthem", "catno": "IARC0041", "ref": { "source": "discogs", "id": "4401" } },
                { "name": "Not On Label (Kite Theory Self-released)" },
                { "name": "Nonesuch", "catno": "075597", "ref": { "source": "musicbrainz", "id": nonesuch } },
            ])
        );
        assert_eq!((v["master"].as_u64(), v["year"].as_u64()), (Some(3001), Some(2021)), "the rest as before");
        let alone = serde_json::json!({ "labels": [{ "name": "Nonesuch", "catno": "075597", "ref": { "source": "musicbrainz", "id": nonesuch } }] });
        assert_eq!(answer_with(&cat, None, &musicbrainz[1..]).unwrap(), alone, "MusicBrainz's alone");
        assert_eq!(answer_with(&cat, None, &[]).unwrap(), serde_json::json!({}));
        let made: i64 = cat.db_for_tests().query_row("SELECT count(*) FROM labels", [], |r| r.get(0)).unwrap();
        assert_eq!(made, 1, "nothing made by answering");
    }

    /// F9: a reference an album's labels were kept with, at Discogs (its facts) or at MusicBrainz (its release's labels, kept beside
    /// them), names the label one may be made by, by that name; another names none. The crates read Discogs' labels alone, as before.
    #[test]
    fn a_label_is_named_by_what_its_album_kept() {
        let (nonesuch, other) = ("00000000-0000-4000-8000-000000000902", "00000000-0000-4000-8000-000000000903");
        let served = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        let facts = r#"{"labels":[{"name":"Low Tide Records","catno":"LTR-012","id":23528},{"name":"Kite Records"}]}"#;
        served.userdata.set_discogs_facts("album:al-1", Some(facts), 1).unwrap();
        let kept = serde_json::to_string(&[MbLabel { name: "Nonesuch".into(), mbid: nonesuch.into(), catno: None }]).unwrap();
        served.userdata.set_discogs_facts(&musicbrainz_subject("al-1"), Some(&kept), 1).unwrap();
        let named = |r: Ref| label_named(&served, &r).unwrap();
        assert_eq!(named(Ref::new("discogs", "label", "23528")), Some(("Low Tide Records".to_string(), false)));
        assert_eq!(named(Ref::new("musicbrainz", "label", nonesuch)), Some(("Nonesuch".to_string(), false)));
        assert_eq!(named(Ref::new("discogs", "label", "4401")), None);
        assert_eq!(named(Ref::new("musicbrainz", "label", other)), None);
        assert_eq!(named(Ref::new("discogs", "master", "23528")), None, "a label's reference alone");
        assert_eq!(musicbrainz_labels(&served, "al-1").unwrap().len(), 1);
        assert_eq!(kept_labels(&served).unwrap().get("al-1"), Some(&vec!["Low Tide Records".to_string(), "Kite Records".to_string()]), "the crates as before");
        assert_eq!(kept_labels(&served).unwrap().len(), 1, "MusicBrainz's kept labels are no album's facts");
    }
}
