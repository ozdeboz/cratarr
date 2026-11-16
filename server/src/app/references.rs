//! A record's references (0.14, the owner, 2026-11-02: one Health item for them): each source that may know the record
//! (MusicBrainz, Discogs, Bandcamp, AcoustID), as agreeing, differing, found (known, not compared) or missing, with what
//! it says (its tracks, year, type, page); which one counts; and whether the record was checked by hand. A record no
//! reference speaks for is Health's item.
//!
//! - **Kept in the catalogue** (decision 0100, `catalogue::records`): what each source knows the record by, and the tracklist
//!   each gives. What 0.14 kept as one JSON an album (`Chosen`, in `record_references`) moved there at 0.17's step
//!   (`step_017`), which alone reads it now.
//! - **One measure of agreeing:** a source's list agrees with the files as `identity::agrees` says, row and count alike, and
//!   the one that counts is `identity::counting`'s, worked out whenever a list arrives (`work_out`) unless chosen by hand: "Use
//!   this", or a link pasted.
//! - **A list read once:** one not kept yet is read when the record is opened, and kept: MusicBrainz's release (its own titles,
//!   which a match keeps none of), Discogs' release. Bandcamp's page is looked up when the record is opened, as before, and read
//!   then; what it found is kept as a guess.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::catalogue::Catalogue;
use crate::catalogue::identity::{self, Listed, RefTrack};
use crate::catalogue::records::{RecordRow, Ref, ReferenceRow};
use crate::serve::ids;

/// The sources, in the order a record's entry lists them.
pub const SOURCES: [&str; 4] = ["musicbrainz", "discogs", "bandcamp", "acoustid"];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct References {
    pub album: String,
    pub title: String,
    pub artist: String,
    pub sources: Vec<Source>,
    /// The source that counts: worked out from the lists and the files, or chosen ("Use this", a link pasted); none where
    /// none fits and none was chosen.
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

/// What 0.14 kept of a record's references (`record_references`, JSON an album). Read by 0.17's step alone, which moves it
/// into the catalogue: `counting` a choice by hand, `by_hand` "Checked by hand", the links pasted, the guesses confirmed.
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

/// What a pasted link names: a MusicBrainz release or release group, a Discogs release or master (a language before it, a
/// name after its number), a Bandcamp album page. None for anything else.
pub fn ref_of_link(url: &str) -> Option<Ref> {
    let url = url.trim();
    let u = reqwest::Url::parse(url).ok()?;
    if !matches!(u.scheme(), "https" | "http") {
        return None;
    }
    let host = u.host_str()?.to_ascii_lowercase();
    let segments: Vec<&str> = u.path_segments()?.filter(|s| !s.is_empty()).collect();
    if host == "musicbrainz.org" || host.ends_with(".musicbrainz.org") {
        let [kind @ ("release" | "release-group"), id, ..] = segments.as_slice() else { return None };
        let id = id.to_ascii_lowercase();
        return super::discogs::is_mbid(&id).then(|| Ref::new("musicbrainz", kind, &id));
    }
    if host == "discogs.com" || host.ends_with(".discogs.com") {
        let at = segments.iter().position(|s| *s == "release" || *s == "master")?;
        let number: u64 = segments.get(at + 1)?.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()?;
        return Some(Ref::new("discogs", segments[at], &number.to_string()));
    }
    if host.ends_with(".bandcamp.com") && u.path().starts_with("/album/") {
        return crate::bandcamp::album_page(url).map(|page| Ref::new("bandcamp", "album", &page));
    }
    None
}

/// A reference's page at its source.
fn url_of(r: &Ref) -> Option<String> {
    Some(match (r.source.as_str(), r.kind.as_str()) {
        ("musicbrainz", "release" | "release-group") => format!("https://musicbrainz.org/{}/{}", r.kind, r.value),
        ("discogs", "release" | "master") => format!("https://www.discogs.com/{}/{}", r.kind, r.value),
        ("bandcamp", "album") => r.value.clone(),
        _ => return None,
    })
}

/// MusicBrainz's list of a release, as `track_fetch` reads it: its own titles, lengths and recordings.
pub fn list_of_release(v: &super::track_fetch::ReleaseView) -> Vec<RefTrack> {
    v.tracks
        .iter()
        .map(|t| RefTrack { disc: t.disc, position: t.position, title: t.title.clone(), length_ms: t.length_ms, recording: Some(t.recording.clone()).filter(|r| !r.is_empty()) })
        .collect()
}

/// Discogs' list of a release (its JSON), each of its media in turn, as Renumber reads them; no recordings.
pub fn list_of_discogs(v: &serde_json::Value) -> Vec<RefTrack> {
    let release = super::renumber::discogs_release(v);
    release
        .media
        .iter()
        .zip(1..)
        .flat_map(|(m, disc)| m.tracks.iter().zip(1..).map(move |((title, secs), position)| RefTrack { disc, position, title: title.clone(), length_ms: secs.map(|s| i64::from(s) * 1000), recording: None }))
        .collect()
}

/// Bandcamp's list of an album page.
pub fn list_of_bandcamp(a: &crate::bandcamp::Album) -> Vec<RefTrack> {
    a.tracks.iter().map(|t| RefTrack { disc: 1, position: t.n, title: t.title.clone(), length_ms: t.duration_secs.map(|s| (s * 1000.0).round() as i64), recording: None }).collect()
}

/// A source's list where one reference names it alone: a MusicBrainz release, a Discogs release. A release group or a master
/// has no one list.
async fn read_list(r: &Ref) -> Result<Option<Vec<RefTrack>>> {
    Ok(match (r.source.as_str(), r.kind.as_str()) {
        ("musicbrainz", "release") => Some(list_of_release(&super::track_fetch::release_by_id(&r.value).await?)),
        ("discogs", "release") => super::discogs::release_raw(r.value.parse()?).await?.map(|v| list_of_discogs(&v)),
        _ => None,
    })
}

/// The record's tracks as a list is held against them: the copy that plays, in its order, each by its title and length.
/// Blocking.
fn files_of(served: &crate::serve::Serve, album: &str) -> Result<Vec<Listed>> {
    Ok(served.library.album_tracks(album)?.into_iter().map(|(_, _, _, title, ms, _, _)| Listed::of(&title, ms as i64)).collect())
}

/// The source that counts for a record (`identity::counting`), from the lists kept and its files, and kept, unless chosen by
/// hand. One counted before any list of it was read (0.17's step, by a certain match) stands until its list is read or another
/// source's agrees. Its counting now. Blocking.
pub fn work_out_in(cat: &Catalogue, record: i64, files: &[Listed]) -> Result<Option<String>> {
    let Some(row) = cat.record(record)? else { return Ok(None) };
    if row.counted_by_hand {
        return Ok(row.counting);
    }
    let kept = cat.reference_tracks(record)?;
    let lists: Vec<(&str, &[RefTrack])> = kept.iter().map(|(s, l)| (s.as_str(), l.as_slice())).filter(|(_, l)| !l.is_empty()).collect();
    let worked = identity::counting(None, &lists, files);
    let unread = row.counting.as_deref().is_some_and(|c| !lists.iter().any(|(s, _)| *s == c));
    let counting = if worked.is_none() && unread { row.counting.clone() } else { worked };
    if counting != row.counting {
        cat.set_counting(record, counting.as_deref(), false)?;
    }
    Ok(counting)
}

/// `work_out_in` for an album of Cratarr's own library. Blocking.
pub fn work_out(served: &crate::serve::Serve, album: &str) -> Result<Option<String>> {
    let Some(record) = ids::record_number(album) else { return Ok(None) };
    work_out_in(served.catalogue(), record, &files_of(served, album)?)
}

/// A source's list held against the record's files, as `identity::agrees` decides: the one measure, so a row never says it
/// agrees where the count says it does not. Its state, and what it says.
fn held(list: &[RefTrack], files: &[Listed]) -> (&'static str, String) {
    if identity::agrees(list, files) {
        return ("agrees", format!("All {} tracks, each within 2 s", files.len()));
    }
    if list.len() != files.len() {
        return ("differs", format!("{} tracks; the record has {}", list.len(), files.len()));
    }
    let off = list.iter().zip(files).filter(|(t, f)| !Listed::of(&t.title, t.length_ms.unwrap_or(f.duration_ms)).same_track(f)).count();
    ("differs", format!("{off} of {} tracks differ", files.len()))
}

/// The one reference of `source` that speaks for the record: a link pasted by hand, else its release (or album page) before
/// its release group or master, else the surest found.
fn main_ref<'k>(refs: &'k [(Ref, ReferenceRow)], source: &str) -> Option<&'k (Ref, ReferenceRow)> {
    let kind = |r: &Ref| match r.kind.as_str() {
        "release" | "album" => 0,
        "release-group" | "master" => 1,
        _ => 2,
    };
    let how = |h: &str| match h {
        "hand" => 0,
        "files" => 1,
        "link" => 2,
        _ => 3,
    };
    refs.iter().filter(|(r, _)| r.source == source).min_by_key(|(r, row)| (row.how != "hand", kind(r), how(&row.how)))
}

/// What is kept of a record, as its references are told.
struct Kept {
    title: String,
    artist: String,
    refs: Vec<(Ref, ReferenceRow)>,
    lists: Vec<(String, Vec<RefTrack>)>,
    row: RecordRow,
    /// Matching's find (`album_matches`): the year it read and why it matched.
    matched: Option<super::matching::Match>,
    /// What Discogs said (`discogs_facts`): its year.
    discogs: Option<super::discogs::AlbumFacts>,
    heard: Option<super::intake_card::Heard>,
    types: HashMap<String, (Option<String>, Vec<String>)>,
    files: Vec<Listed>,
}

/// Blocking.
fn kept(served: &crate::serve::Serve, album: &str, record: i64) -> Result<Kept> {
    let rec = served.library.album(album)?.context("No such album in Cratarr's own library")?;
    let cat = served.catalogue();
    let row = cat.record(record)?.context("No such album in Cratarr's own library")?;
    let matched = served.userdata.album_matches(&[album.to_string()])?.remove(album).and_then(|(_, j)| serde_json::from_str(&j?).ok());
    Ok(Kept {
        title: rec.name,
        artist: rec.artist,
        refs: cat.references_of(album)?,
        lists: cat.reference_tracks(record)?,
        row,
        matched,
        discogs: super::discogs_facts::album_kept(served, album).ok().flatten(),
        heard: served.userdata.intake_heard(album, None)?.and_then(|(_, j)| serde_json::from_str(&j).ok()),
        types: served.userdata.release_types()?,
        files: files_of(served, album)?,
    })
}

/// One source's row, from its reference and its list.
fn source_of(source: &'static str, k: &Kept, found: &str, missing: &str) -> Source {
    let main = main_ref(&k.refs, source);
    let list = k.lists.iter().find(|(s, _)| s == source).map(|(_, l)| l.as_slice()).filter(|l| !l.is_empty());
    let (state, said) = match (list, main) {
        (Some(l), _) => held(l, &k.files),
        (None, Some((_, row))) if row.how == "hand" => ("agrees", "Pasted by hand".to_string()),
        (None, Some(_)) => ("found", found.to_string()),
        (None, None) => ("missing", missing.to_string()),
    };
    Source {
        source,
        state,
        tracks: list.map(<[RefTrack]>::len),
        year: None,
        kind: None,
        url: main.and_then(|(r, _)| url_of(r)),
        lengths: list.map(|l| l.iter().map(|t| t.length_ms.unwrap_or(0)).collect()),
        guessed: main.is_some_and(|(_, row)| row.how == "search"),
        confirmed: main.is_some_and(|(_, row)| row.confirmed),
        said,
    }
}

/// Every source's row, `bandcamp_year` as its page said it when read now.
fn sources(k: &Kept, bandcamp_year: Option<u32>) -> Vec<Source> {
    let mut mb = source_of("musicbrainz", k, "Found by its name", "No release matched yet");
    if let Some((r, row)) = main_ref(&k.refs, "musicbrainz") {
        let matched = k.matched.as_ref().filter(|m| m.release == r.value || m.release_group.as_deref() == Some(r.value.as_str()));
        let release = matched.map(|m| m.release.as_str()).or((r.kind == "release").then_some(r.value.as_str()));
        mb.year = matched.and_then(|m| m.year);
        mb.kind = release.and_then(|rel| k.types.get(&format!("mb:{rel}"))).and_then(|t| t.0.clone());
        if mb.state == "found" {
            match (row.how.as_str(), matched) {
                ("files", _) => mb.said = "The files name this release".into(),
                (_, Some(m)) => mb.said = m.why.clone(),
                _ => {}
            }
        }
    }
    let mut discogs = source_of("discogs", k, "Discogs knows it", "Discogs has not said");
    discogs.year = main_ref(&k.refs, "discogs").and(k.discogs.as_ref()).and_then(|d| d.year);
    let mut bandcamp = source_of("bandcamp", k, "Its page on Bandcamp", "No page found on Bandcamp");
    bandcamp.year = bandcamp_year;
    // AcoustID: its tracks as heard (the intake's hearing), no reference of its own.
    let acoustid = match k.heard.as_ref().filter(|h| !h.no_acoustid) {
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
        None => Source { source: "acoustid", state: "missing", tracks: None, year: None, kind: None, url: None, lengths: None, guessed: false, confirmed: false, said: "Not heard yet".into() },
    };
    vec![mb, discogs, bandcamp, acoustid]
}

/// A record's references as kept, a list not kept yet read now (Bandcamp's page looked up when the record is opened).
pub async fn of(app: &Arc<super::App>, album: &str) -> Result<References> {
    let served = app.served.clone().context("Cratarr's own library is not open")?;
    let record = ids::record_number(album).context("No such album in Cratarr's own library")?;
    let (s, a) = (served.clone(), album.to_string());
    let k = tokio::task::spawn_blocking(move || kept(&s, &a, record)).await??;
    let mut read: Vec<(String, Vec<RefTrack>)> = Vec::new();
    for source in ["musicbrainz", "discogs"] {
        if k.lists.iter().any(|(s, l)| s == source && !l.is_empty()) {
            continue;
        }
        let Some((r, _)) = main_ref(&k.refs, source) else { continue };
        match read_list(r).await {
            Ok(Some(list)) if !list.is_empty() => read.push((source.to_string(), list)),
            Ok(_) => {}
            Err(e) => tracing::debug!("references: {source} of {album} not read: {e:#}"),
        }
    }
    // Bandcamp: its page as kept, else looked up now (kept a month, a miss too), and read, as when it was first built.
    let kept_page = main_ref(&k.refs, "bandcamp").map(|(r, _)| r.value.clone());
    let page = match &kept_page {
        Some(p) => Some(p.clone()),
        None => {
            let group = k.refs.iter().find(|(r, _)| r.source == "musicbrainz" && r.kind == "release-group").map(|(r, _)| r.value.clone()).or_else(|| k.matched.as_ref().and_then(|m| m.release_group.clone()));
            super::discover::find_on_bandcamp(&served, &k.artist, &k.title, group.as_deref()).await.ok().flatten().map(|p| crate::bandcamp::album_page(&p).unwrap_or(p))
        }
    };
    let page_read = match &page {
        Some(p) => crate::bandcamp::Bandcamp::get().album(p).await.ok(),
        None => None,
    };
    let year = page_read.as_ref().and_then(|a| a.released.as_deref()).and_then(|d| d.get(..4)?.parse().ok());
    if let Some(a) = &page_read {
        let list = list_of_bandcamp(a);
        if !k.lists.iter().any(|(s, l)| s == "bandcamp" && *l == list) {
            read.push(("bandcamp".into(), list));
        }
    }
    let found = page.filter(|_| kept_page.is_none());
    let (s, a) = (served.clone(), album.to_string());
    let k = tokio::task::spawn_blocking(move || -> Result<Kept> {
        let cat = s.catalogue();
        if let Some(p) = &found {
            cat.set_reference(&a, &Ref::new("bandcamp", "album", p), "search")?;
        }
        for (source, list) in &read {
            cat.set_reference_tracks(record, source, list)?;
        }
        if !read.is_empty() {
            work_out(&s, &a)?;
        }
        kept(&s, &a, record)
    })
    .await??;
    Ok(References { album: album.to_string(), title: k.title.clone(), artist: k.artist.clone(), sources: sources(&k, year), counting: k.row.counting.clone(), by_hand: k.row.checked_by_hand })
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

/// What `choose` writes, in the catalogue, `files` the record's: a source chosen counts by hand, and taken back the count is
/// worked out again; a link pasted is a reference by hand, and its source counts. Whether the record became checked by hand
/// now. Blocking.
fn chose(cat: &Catalogue, record: i64, album: &str, c: &Choose, files: &[Listed]) -> Result<bool> {
    if let Some(counting) = &c.counting {
        if let Some(src) = counting
            && !SOURCES.contains(&src.as_str())
        {
            bail!("No such source: {src}");
        }
        cat.set_counting(record, counting.as_deref(), true)?;
        if counting.is_none() {
            work_out_in(cat, record, files)?;
        }
    }
    if let Some(link) = c.link.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        let r = ref_of_link(link).context("Paste a release's page on MusicBrainz or Discogs, or an album's on Bandcamp")?;
        cat.set_reference(album, &r, "hand")?;
        cat.set_counting(record, Some(&r.source), true)?;
    }
    if let Some(src) = &c.confirm {
        if !SOURCES.contains(&src.as_str()) {
            bail!("No such source: {src}");
        }
        cat.confirm_reference(album, src)?;
    }
    let was = cat.record(record)?.is_some_and(|r| r.checked_by_hand);
    if let Some(b) = c.by_hand {
        cat.set_checked_by_hand(record, b)?;
    }
    Ok(c.by_hand == Some(true) && !was)
}

pub async fn choose(app: &Arc<super::App>, album: &str, c: Choose) -> Result<References> {
    let served = app.served.clone().context("Cratarr's own library is not open")?;
    let record = ids::record_number(album).context("No such album in Cratarr's own library")?;
    let (s, a) = (served.clone(), album.to_string());
    tokio::task::spawn_blocking(move || -> Result<()> {
        let rec = s.library.album(&a)?.context("No such album in Cratarr's own library")?;
        let checked = chose(s.catalogue(), record, &a, &c, &files_of(&s, &a)?)?;
        // Checked by hand: the record is its own reference for its type and year from now on, as it shows them (set as
        // by hand, so no source's guess changes them after).
        if checked {
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

/// The records no reference speaks for (Health's References item): the catalogue's one query (`Catalogue::unreferenced`),
/// of those the library lists, each as it is named there. Blocking.
pub fn unreferenced(served: &crate::serve::Serve) -> Result<Vec<(String, String, String)>> {
    let listed: std::collections::HashSet<String> = served.library.album_ids()?.into_iter().collect();
    let mut out = Vec::new();
    for record in served.catalogue().unreferenced()? {
        let album = ids::record(record);
        if !listed.contains(&album) {
            continue;
        }
        let Some((_, artist, name)) = served.library.album_names(&album)? else { continue };
        out.push((album, name, artist));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_link_names_its_source_kind_and_value() {
        let r = |url: &str| ref_of_link(url).map(|r| (r.source, r.kind, r.value));
        let named = |s: &str, k: &str, v: &str| Some((s.to_string(), k.to_string(), v.to_string()));
        let mbid = "0b6f4bd4-2f0c-4b7e-9a51-6c1c0d8f7a11";
        assert_eq!(r(&format!("https://musicbrainz.org/release/{mbid}")), named("musicbrainz", "release", mbid));
        assert_eq!(r(&format!("https://beta.musicbrainz.org/release-group/{}/", mbid.to_uppercase())), named("musicbrainz", "release-group", mbid));
        assert_eq!(r("https://musicbrainz.org/release/not-an-id"), None);
        assert_eq!(r("https://www.discogs.com/release/123-Air-Moon-Safari"), named("discogs", "release", "123"));
        assert_eq!(r("https://www.discogs.com/de/master/456"), named("discogs", "master", "456"));
        assert_eq!(r("https://www.discogs.com/Air-Moon-Safari/master/0456"), named("discogs", "master", "456"));
        assert_eq!(r("  https://air.bandcamp.com/album/moon-safari?from=search  "), named("bandcamp", "album", "https://air.bandcamp.com/album/moon-safari"));
        assert_eq!(r("https://air.bandcamp.com/track/la-femme"), None, "a track, not an album");
        assert_eq!(r("https://example.org/release/1"), None);
        assert_eq!(r("not a link"), None);
    }

    fn list(tracks: &[(&str, Option<i64>)]) -> Vec<RefTrack> {
        tracks.iter().zip(1..).map(|((title, length_ms), position)| RefTrack { disc: 1, position, title: title.to_string(), length_ms: *length_ms, recording: None }).collect()
    }

    fn files(tracks: &[(&str, i64)]) -> Vec<Listed> {
        tracks.iter().map(|(t, ms)| Listed::of(t, *ms)).collect()
    }

    /// A row agrees exactly where the count would take it: `identity::agrees`, titles and two seconds.
    #[test]
    fn a_row_agrees_by_the_one_measure() {
        let held_files = files(&[("Slack Water", 200_000), ("Neap", 180_000)]);
        for (l, agrees) in [
            (list(&[("Slack Water", Some(201_000)), ("Neap", Some(181_000))]), true),
            (list(&[("Slack Water", None), ("Neap (feat. A Guest)", Some(180_000))]), true),
            (list(&[("Slack Water", Some(201_000)), ("Neap", Some(184_000))]), false),
            (list(&[("Slack Water", Some(200_000)), ("Spring Tide", Some(180_000))]), false),
            (list(&[("Slack Water", Some(200_000))]), false),
        ] {
            assert_eq!(held(&l, &held_files).0 == "agrees", agrees, "{l:?}");
            assert_eq!(identity::agrees(&l, &held_files), agrees);
        }
        assert_eq!(held(&list(&[("Slack Water", Some(200_000)), ("Spring Tide", Some(180_000))]), &held_files).1, "1 of 2 tracks differ");
        assert_eq!(held(&list(&[("Slack Water", Some(200_000))]), &held_files).1, "1 tracks; the record has 2");
    }

    /// A record had: its tracks on it (no files are read here).
    fn had(cat: &Catalogue, title: &str, tracks: usize) -> (i64, String) {
        let record = cat.record_for_tests(title, tracks);
        (record, ids::record(record))
    }

    /// 0.14's "Use this" before whatever agrees, a list arriving or not; taken back, the count is worked out again.
    #[test]
    fn a_hand_choice_comes_first() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (record, album) = had(&cat, "Tide Tables", 2);
        let tracks = files(&[("Slack Water", 200_000), ("Neap", 180_000)]);
        let choose = |counting: Option<&str>| Choose { counting: Some(counting.map(str::to_string)), ..Choose::default() };
        chose(&cat, record, &album, &choose(Some("discogs")), &tracks).unwrap();
        cat.set_reference_tracks(record, "musicbrainz", &list(&[("Slack Water", Some(200_000)), ("Neap", Some(180_000))])).unwrap();
        assert_eq!(work_out_in(&cat, record, &tracks).unwrap().as_deref(), Some("discogs"), "a list that agrees does not take its place");
        let row = cat.record(record).unwrap().unwrap();
        assert_eq!((row.counting.as_deref(), row.counted_by_hand), (Some("discogs"), true));
        chose(&cat, record, &album, &choose(None), &tracks).unwrap();
        let row = cat.record(record).unwrap().unwrap();
        assert_eq!((row.counting.as_deref(), row.counted_by_hand), (Some("musicbrainz"), false), "taken back: the one that fits");
        assert!(chose(&cat, record, &album, &choose(Some("nowhere")), &tracks).is_err());
    }

    /// A link pasted names what it names: a reference by hand, its source counting, taken from a record that guessed it.
    #[test]
    fn a_pasted_link_is_a_hand_reference() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let ((record, album), (_, other)) = (had(&cat, "Two Rooms", 2), had(&cat, "Two Rooms (Other)", 2));
        let release = Ref::new("discogs", "release", "2000002");
        assert!(cat.set_reference(&other, &release, "search").unwrap());
        let paste = Choose { link: Some(" https://www.discogs.com/release/2000002-Noor-Field-Two-Rooms ".into()), ..Choose::default() };
        chose(&cat, record, &album, &paste, &[]).unwrap();
        assert_eq!(cat.subject_of(&release).unwrap(), [album.clone()]);
        let refs = cat.references_of(&album).unwrap();
        assert_eq!(refs.iter().map(|(r, row)| (r.clone(), row.how.as_str())).collect::<Vec<_>>(), [(release.clone(), "hand")]);
        let row = cat.record(record).unwrap().unwrap();
        assert_eq!((row.counting.as_deref(), row.counted_by_hand), (Some("discogs"), true));
        let k = Kept { title: String::new(), artist: String::new(), refs, lists: Vec::new(), row, matched: None, discogs: None, heard: None, types: HashMap::new(), files: Vec::new() };
        let discogs = &sources(&k, None)[1];
        assert_eq!((discogs.state, discogs.said.as_str(), discogs.guessed), ("agrees", "Pasted by hand", false));
        assert_eq!(discogs.url.as_deref(), Some("https://www.discogs.com/release/2000002"));
        assert!(chose(&cat, record, &album, &Choose { link: Some("https://example.org/x".into()), ..Choose::default() }, &[]).is_err());
    }

    /// Health's item: what 0.16 left off it stays off, in one query. A guess confirmed and checked by hand, the record's.
    #[test]
    fn a_record_no_reference_speaks_for_is_healths() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let [(alone, _), (counted, _), (checked, _), (_, named_id), (_, pasted_id), (guessed, guessed_id)] =
            ["Alone", "Counted", "Checked", "Named", "Pasted", "Guessed"].map(|t| had(&cat, t, 1));
        cat.set_counting(counted, Some("musicbrainz"), false).unwrap();
        let none = Choose { by_hand: Some(true), ..Choose::default() };
        assert!(chose(&cat, checked, &ids::record(checked), &none, &[]).unwrap(), "checked now");
        assert!(!chose(&cat, checked, &ids::record(checked), &none, &[]).unwrap(), "and not again");
        cat.set_reference(&named_id, &Ref::new("musicbrainz", "release", "00000000-0000-4000-8000-000000000001"), "files").unwrap();
        cat.set_reference(&pasted_id, &Ref::new("bandcamp", "album", "https://glassharbour.bandcamp.com/album/x"), "hand").unwrap();
        cat.set_reference(&guessed_id, &Ref::new("musicbrainz", "release", "00000000-0000-4000-8000-000000000002"), "search").unwrap();
        cat.set_reference(&guessed_id, &Ref::new("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000102"), "files").unwrap();
        assert_eq!(cat.confirm_reference(&guessed_id, "musicbrainz").unwrap(), 2);
        assert!(cat.references_of(&guessed_id).unwrap().iter().all(|(_, row)| row.confirmed));
        had(&cat, "Not Had", 0);
        assert_eq!(cat.unreferenced().unwrap(), [alone, guessed], "a guess and a group are no reference that speaks");
    }
}
