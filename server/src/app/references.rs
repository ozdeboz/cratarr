//! A record's references (0.14, the owner, 2026-11-02: one Health item for them): each source that may know the record
//! (MusicBrainz, Discogs, Bandcamp, AcoustID), as agreeing, differing, found (known, not compared) or missing, with what
//! it says (its tracks, year, type, page); which one counts ("Use this", or a link pasted by hand); and whether the record
//! was checked by hand. A record with no source agreeing, none chosen and not checked by hand is Health's item.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// The sources, in the order a record's entry lists them.
pub const SOURCES: [&str; 4] = ["musicbrainz", "discogs", "bandcamp", "acoustid"];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct References {
    pub album: String,
    pub title: String,
    pub artist: String,
    pub sources: Vec<Source>,
    /// The source that counts, chosen ("Use this") or by its link pasted; none where none was chosen.
    pub counting: Option<String>,
    pub by_hand: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Source {
    pub source: &'static str,
    /// agrees, differs, found (known, not held against the record), missing.
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracks: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Its tracks' lengths (ms), in its order, where it has them: side by side with the record's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lengths: Option<Vec<i64>>,
    /// Found by guessing (a search by name, not an id the files carry), until confirmed once.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub guessed: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub confirmed: bool,
    /// One sentence: what it says, or why it differs.
    pub said: String,
}

/// A track's length this close to the record's agrees (ms), as matching's likely.
const NEAR_MS: i64 = 5_000;

/// A source's track list held against the record's: agrees where as many tracks, each this near; else differs, said.
fn held(lengths: &[Option<i64>], record: &[i64]) -> (&'static str, String) {
    if lengths.len() != record.len() {
        return ("differs", format!("{} tracks; the record has {}", lengths.len(), record.len()));
    }
    let off = lengths.iter().zip(record).filter(|(l, r)| l.is_some_and(|l| (l - **r).abs() > NEAR_MS)).count();
    match off {
        0 => ("agrees", format!("All {} tracks, each within {} s", record.len(), NEAR_MS / 1000)),
        n => ("differs", format!("{n} of {} lengths differ", record.len())),
    }
}

/// What was chosen of a record's references, kept (`record_references`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Chosen {
    #[serde(default)]
    pub counting: Option<String>,
    #[serde(default)]
    pub by_hand: bool,
    /// Links pasted by hand, by source.
    #[serde(default)]
    pub links: std::collections::BTreeMap<String, String>,
    /// Sources found by guessing that were confirmed once.
    #[serde(default)]
    pub confirmed: std::collections::BTreeSet<String>,
}

/// The source a pasted link is of: MusicBrainz's release, Discogs' release or master, a Bandcamp album page.
pub fn source_of_link(url: &str) -> Option<&'static str> {
    let u = reqwest::Url::parse(url.trim()).ok()?;
    if !matches!(u.scheme(), "https" | "http") {
        return None;
    }
    let host = u.host_str()?.to_ascii_lowercase();
    let path = u.path();
    match () {
        _ if (host == "musicbrainz.org" || host.ends_with(".musicbrainz.org")) && (path.starts_with("/release/") || path.starts_with("/release-group/")) => Some("musicbrainz"),
        _ if (host == "discogs.com" || host.ends_with(".discogs.com")) && (path.contains("/release/") || path.contains("/master/")) => Some("discogs"),
        _ if host.ends_with(".bandcamp.com") && path.starts_with("/album/") => Some("bandcamp"),
        _ => None,
    }
}

fn chosen_of(served: &crate::serve::Serve, album: &str) -> Result<Chosen> {
    Ok(served.userdata.record_references(album)?.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default())
}

fn keep(served: &crate::serve::Serve, album: &str, c: &Chosen) -> Result<()> {
    served.userdata.set_record_references(album, &serde_json::to_string(c)?)
}

/// A record's references as kept (no source asked now but Bandcamp's page, looked up when the record is opened).
pub async fn of(app: &Arc<super::App>, album: &str) -> Result<References> {
    let served = app.served.clone().context("Cratarr's own library is not open")?;
    let (s, a) = (served.clone(), album.to_string());
    let (rec, mbid, matched, discogs, heard, chosen, types) = tokio::task::spawn_blocking(move || -> Result<_> {
        let rec = s.library.album(&a)?.context("No such album in Cratarr's own library")?;
        let mbid = s.library.album_names(&a)?.and_then(|p| p.0).filter(|m| super::discogs::is_mbid(m));
        let matched: Option<super::matching::Match> = s.userdata.album_matches(std::slice::from_ref(&a))?.remove(&a).and_then(|(_, j)| serde_json::from_str(&j?).ok());
        let discogs = super::discogs_facts::album_kept(&s, &a).ok().flatten();
        let heard: Option<super::intake_card::Heard> = s.userdata.intake_heard(&a, None)?.and_then(|(_, j)| serde_json::from_str(&j).ok());
        let chosen = chosen_of(&s, &a)?;
        let types = s.userdata.release_types()?;
        Ok((rec, mbid, matched, discogs, heard, chosen, types))
    })
    .await??;
    let n = rec.song_count as usize;
    let a = album.to_string();
    let record: Vec<i64> = tokio::task::spawn_blocking(move || super::renumber::tracks_of(&a).map(|t| t.0.iter().map(|t| i64::from(t.secs) * 1000).collect::<Vec<_>>()))
        .await??;
    let mut sources = Vec::new();
    // MusicBrainz: the release matched (certain: it agrees), else the one the files name.
    let mb = match (&matched, &mbid) {
        (Some(m), _) => Source {
            source: "musicbrainz",
            state: if m.certain && m.tracks.len() == n { "agrees" } else { "differs" },
            tracks: Some(m.tracks.len()),
            year: m.year,
            kind: types.get(&format!("mb:{}", m.release)).and_then(|t| t.0.clone()),
            url: Some(format!("https://musicbrainz.org/release/{}", m.release)),
            lengths: (!m.lengths.is_empty()).then(|| m.lengths.iter().map(|l| l.unwrap_or(0) * 1000).collect()),
            // Found by its name, not named by the files.
            guessed: mbid.as_deref() != Some(m.release.as_str()),
            confirmed: false,
            said: m.why.clone(),
        },
        (None, Some(id)) => Source {
            source: "musicbrainz",
            state: "found",
            tracks: None,
            year: None,
            kind: types.get(&format!("mb:{id}")).and_then(|t| t.0.clone()),
            url: Some(format!("https://musicbrainz.org/release/{id}")),
            lengths: None,
            guessed: false,
            confirmed: false,
            said: "The files name this release".into(),
        },
        (None, None) => missing("musicbrainz", "No release matched yet"),
    };
    sources.push(mb);
    // Discogs: its release's track list held against the record's (read now: Discogs is paced), else its year.
    let discogs_tracks = match discogs.as_ref().and_then(|d| d.release) {
        Some(r) => super::discogs::release_raw(r).await.ok().flatten().map(|v| super::renumber::discogs_release(&v)),
        None => None,
    };
    sources.push(match &discogs {
        Some(d) => {
            let lengths: Option<Vec<Option<i64>>> = discogs_tracks.as_ref().map(|r| r.media.iter().flat_map(|m| m.tracks.iter().map(|t| t.1.map(|s| i64::from(s) * 1000))).collect());
            let (state, said) = match (&lengths, d.year, rec.year) {
                (Some(l), ..) if !l.is_empty() => held(l, &record),
                (_, Some(a), Some(b)) if a != b => ("differs", format!("Its year is {a}, the record's {b}")),
                (_, Some(_), Some(_)) => ("agrees", "Its year is the record's".into()),
                _ => ("found", "Discogs knows it".into()),
            };
            Source {
                source: "discogs",
                state,
                tracks: lengths.as_ref().map(Vec::len),
                year: d.year,
                kind: None,
                url: d.url.clone(),
                lengths: lengths.map(|l| l.into_iter().map(|x| x.unwrap_or(0)).collect()),
                // Discogs is found by the record's name.
                guessed: true,
                confirmed: false,
                said,
            }
        }
        None => missing("discogs", "Discogs has not said"),
    });
    // Bandcamp: its page, looked up now where it never was (kept a month), its track list held against the record's.
    let page = match chosen.links.get("bandcamp") {
        Some(l) => Some(l.clone()),
        None => super::discover::find_on_bandcamp(&served, &rec.artist, &rec.name, matched.as_ref().and_then(|m| m.release_group.as_deref())).await.ok().flatten(),
    };
    sources.push(match page {
        Some(url) => {
            let read = crate::bandcamp::Bandcamp::get().album(&url).await.ok();
            let lengths: Option<Vec<Option<i64>>> = read.as_ref().map(|a| a.tracks.iter().map(|t| t.duration_secs.map(|s| (s * 1000.0).round() as i64)).collect());
            let (state, said) = match &lengths {
                Some(l) if !l.is_empty() => held(l, &record),
                _ => ("found", "Its page on Bandcamp".into()),
            };
            Source {
                source: "bandcamp",
                state,
                tracks: lengths.as_ref().map(Vec::len),
                year: read.as_ref().and_then(|a| a.released.as_deref()).and_then(|d| d.get(..4)?.parse().ok()),
                kind: None,
                url: Some(url),
                lengths: lengths.map(|l| l.into_iter().map(|x| x.unwrap_or(0)).collect()),
                // Bandcamp is found through its search box's answers.
                guessed: true,
                confirmed: false,
                said,
            }
        }
        None => missing("bandcamp", "No page found on Bandcamp"),
    });
    // AcoustID: its tracks as heard (the intake's hearing).
    sources.push(match heard.filter(|h| !h.no_acoustid) {
        Some(h) => {
            let known = h.tracks.iter().filter(|t| !t.recordings.is_empty()).count();
            Source {
                source: "acoustid",
                state: if known == h.tracks.len() { "agrees" } else { "differs" },
                tracks: Some(known),
                year: None,
                kind: None,
                url: None,
                lengths: None,
                guessed: false,
                confirmed: false,
                said: format!("AcoustID knows {known} of {} tracks", h.tracks.len()),
            }
        }
        None => missing("acoustid", "Not heard yet"),
    });
    // A link pasted by hand stands for its source (no guess); a guess confirmed once says so.
    for s in &mut sources {
        if let Some(url) = chosen.links.get(s.source) {
            s.url = Some(url.clone());
            s.guessed = false;
            if s.state == "missing" || s.state == "found" {
                s.state = "agrees";
                s.said = "Pasted by hand".into();
            }
        }
        if s.guessed && chosen.confirmed.contains(s.source) {
            s.confirmed = true;
        }
    }
    Ok(References { album: album.to_string(), title: rec.name, artist: rec.artist, sources, counting: chosen.counting, by_hand: chosen.by_hand })
}

fn missing(source: &'static str, said: &str) -> Source {
    Source { source, state: "missing", tracks: None, year: None, kind: None, url: None, lengths: None, guessed: false, confirmed: false, said: said.into() }
}

/// What the administrator chose of a record's references: the source that counts ("Use this"; none: none chosen), a
/// link pasted (its source then counts), or checked by hand.
#[derive(Debug, Default, Deserialize)]
pub struct Choose {
    #[serde(default)]
    pub counting: Option<Option<String>>,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub by_hand: Option<bool>,
    /// A source found by guessing, confirmed once.
    #[serde(default)]
    pub confirm: Option<String>,
}

pub async fn choose(app: &Arc<super::App>, album: &str, c: Choose) -> Result<References> {
    let served = app.served.clone().context("Cratarr's own library is not open")?;
    let (s, a) = (served.clone(), album.to_string());
    tokio::task::spawn_blocking(move || -> Result<()> {
        s.library.album(&a)?.context("No such album in Cratarr's own library")?;
        let mut kept = chosen_of(&s, &a)?;
        if let Some(counting) = c.counting {
            if let Some(src) = &counting
                && !SOURCES.contains(&src.as_str())
            {
                bail!("No such source: {src}");
            }
            kept.counting = counting;
        }
        if let Some(link) = c.link.map(|l| l.trim().to_string()).filter(|l| !l.is_empty()) {
            let src = source_of_link(&link).context("Paste a release's page on MusicBrainz or Discogs, or an album's on Bandcamp")?;
            kept.links.insert(src.to_string(), link);
            kept.counting = Some(src.to_string());
        }
        if let Some(src) = c.confirm {
            if !SOURCES.contains(&src.as_str()) {
                bail!("No such source: {src}");
            }
            kept.confirmed.insert(src);
        }
        let checked = c.by_hand == Some(true) && !kept.by_hand;
        if let Some(b) = c.by_hand {
            kept.by_hand = b;
        }
        keep(&s, &a, &kept)?;
        // Checked by hand: the record is its own reference for its type and year from now on, as it shows them (set as
        // by hand, so no source's guess changes them after).
        if checked {
            let rec = s.library.album(&a)?.context("No such album in Cratarr's own library")?;
            let mut facts: Vec<(String, Option<serde_json::Value>)> = Vec::new();
            if let Some(kind) = rec.release.kind.clone() {
                facts.push(("type".into(), Some(serde_json::Value::String(kind))));
            }
            if let Some(y) = rec.year.filter(|y| *y >= 1000) {
                facts.push(("year".into(), Some(serde_json::json!(y))));
            }
            if !facts.is_empty() {
                s.userdata.set_overrides(std::slice::from_ref(&a), &facts, super::moment::now_secs())?;
                s.refresh()?;
            }
        }
        Ok(())
    })
    .await??;
    app.collection.refresh().await;
    app.collection.tell_changed();
    of(app, album).await
}

/// The records no reference speaks for (Health's References item): none agreeing as kept (MusicBrainz's release
/// matched with every track, or named by the files; a link pasted), none chosen, not checked by hand. Blocking.
pub fn unreferenced(served: &crate::serve::Serve) -> Result<Vec<(String, String, String)>> {
    let mut out = Vec::new();
    let ids = served.library.album_ids()?;
    let matches = served.userdata.album_matches(&ids)?;
    let chosen = served.userdata.all_record_references()?;
    for id in ids {
        if let Some(c) = chosen.get(&id).and_then(|j| serde_json::from_str::<Chosen>(j).ok())
            && (c.by_hand || c.counting.is_some() || !c.links.is_empty())
        {
            continue;
        }
        let Some((mbid, artist, name)) = served.library.album_names(&id)? else { continue };
        if mbid.as_deref().is_some_and(super::discogs::is_mbid) {
            continue;
        }
        let certain = matches.get(&id).and_then(|(_, j)| serde_json::from_str::<super::matching::Match>(j.as_deref()?).ok()).is_some_and(|m| m.certain);
        if certain {
            continue;
        }
        out.push((id, name, artist));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_link_is_known_by_its_source() {
        assert_eq!(source_of_link("https://musicbrainz.org/release/0b6f4bd4-2f0c-4b7e-9a51-6c1c0d8f7a11"), Some("musicbrainz"));
        assert_eq!(source_of_link("https://www.discogs.com/release/123-Air-Moon-Safari"), Some("discogs"));
        assert_eq!(source_of_link("https://www.discogs.com/master/456"), Some("discogs"));
        assert_eq!(source_of_link("https://air.bandcamp.com/album/moon-safari"), Some("bandcamp"));
        assert_eq!(source_of_link("https://air.bandcamp.com/track/la-femme"), None, "a track, not an album");
        assert_eq!(source_of_link("https://example.com/release/1"), None);
        assert_eq!(source_of_link("not a link"), None);
    }

    #[test]
    fn a_track_list_agrees_where_as_many_tracks_each_near() {
        let record = [200_000, 180_000];
        assert_eq!(held(&[Some(201_000), Some(182_000)], &record).0, "agrees");
        assert_eq!(held(&[Some(201_000), Some(190_000)], &record), ("differs", "1 of 2 lengths differ".to_string()));
        assert_eq!(held(&[Some(201_000)], &record), ("differs", "1 tracks; the record has 2".to_string()));
        assert_eq!(held(&[None, Some(180_000)], &record).0, "agrees", "a length it does not give is not held against it");
    }
}
