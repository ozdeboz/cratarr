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
/// name after its number, or Discogs' older `/master/view/N`, which 0.16 kept as pasted), a Bandcamp album page. None for
/// anything else.
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
        let kind = segments.iter().position(|s| *s == "release" || *s == "master")?;
        let at = if segments.get(kind + 1) == Some(&"view") { kind + 1 } else { kind };
        let number: u64 = segments.get(at + 1)?.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()?;
        return Some(Ref::new("discogs", segments[kind], &number.to_string()));
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

// ---- A record's counting list (C51a)

/// A source by the name the pages give it.
fn source_name(source: &str) -> &'static str {
    match source {
        "musicbrainz" => "MusicBrainz",
        "discogs" => "Discogs",
        "bandcamp" => "Bandcamp",
        _ => "Its source",
    }
}

/// A record's list, the one that counts (C51a, Q31: what judges a Soulseek folder), read once through `read` and kept
/// (`reference_tracks`), with its source:
/// - **Counted** (by hand, or worked out): that source's list, kept or read now.
/// - **Not had**: no files to hold a list against, so "none until chosen" is not its case: its references count in the
///   design's order (`identity::COUNTING_ORDER`: MusicBrainz's release, else its release group's; Discogs'; Bandcamp's album),
///   the first whose list answers, and that source counts from then on. One that answers nothing (no such release, Bandcamp's
///   bot check) gives way to the next.
/// - **Had, no count**: its files and no list agree, so none counts until an administrator picks one (the owner, 2026-11-16):
///   nothing is read.
///
/// `Ok(Err(why))`: no list to judge by, and why in few words. `Err`: a source could not be reached; asked again later.
pub async fn tracklist<R, F>(cat: &Arc<Catalogue>, record: i64, read: R) -> Result<std::result::Result<(String, Vec<RefTrack>), String>>
where
    R: Fn(Ref) -> F,
    F: std::future::Future<Output = Result<Option<Vec<RefTrack>>>>,
{
    let c = cat.clone();
    let Some((row, refs, kept)) = tokio::task::spawn_blocking(move || -> Result<_> {
        let Some(row) = c.record(record)? else { return Ok(None) };
        let refs = c.references_of(&ids::record(row.id))?;
        let kept = c.reference_tracks(row.id)?;
        Ok(Some((row, refs, kept)))
    })
    .await??
    else {
        return Ok(Err("No such record".into()));
    };
    let kept_of = |source: &str| kept.iter().find(|(s, l)| s == source && !l.is_empty()).map(|(_, l)| l.clone());
    let keep = |source: &'static str, list: Vec<RefTrack>, count: bool| {
        let (c, id) = (cat.clone(), row.id);
        async move {
            tokio::task::spawn_blocking(move || -> Result<()> {
                c.set_reference_tracks(id, source, &list)?;
                if count {
                    c.set_counting(id, Some(source), false)?;
                }
                Ok(())
            })
            .await?
        }
    };
    let sources: Vec<&'static str> = match row.counting.as_deref() {
        Some(counting) => identity::COUNTING_ORDER.iter().copied().filter(|s| *s == counting).collect(),
        None if row.had => return Ok(Err("No list counts until one is chosen".into())),
        None => identity::COUNTING_ORDER.to_vec(),
    };
    let mut why: Vec<String> = Vec::new();
    for source in sources {
        if let Some(list) = kept_of(source) {
            if row.counting.is_none() {
                keep(source, list.clone(), true).await?;
            }
            return Ok(Ok((source.to_string(), list)));
        }
        let Some((r, _)) = main_ref(&refs, source) else { continue };
        match read(r.clone()).await {
            Ok(Some(list)) if !list.is_empty() => {
                keep(source, list.clone(), row.counting.is_none()).await?;
                return Ok(Ok((source.to_string(), list)));
            }
            Ok(_) => why.push(format!("{} has no list of it", source_name(source))),
            Err(e) if crate::bandcamp::not_read(&e) => why.push(format!("{} didn't answer", source_name(source))),
            Err(e) => return Err(e),
        }
    }
    Ok(Err(if why.is_empty() { "No list to judge by".to_string() } else { why.join("; ") }))
}

/// A reference's list, read from its source (C51a, `tracklist`'s reader): a MusicBrainz release's own; a release group's, one
/// of its releases (the first official one with tracks, else the first with tracks, as 0.16's Soulseek chose); a Discogs
/// release's, a master's main release's; a Bandcamp album's. None where the source has no such thing (an answer "not found");
/// Bandcamp's bot check is its `NotRead`. Every MusicBrainz question in its turn (S10).
pub async fn read_reference(r: Ref) -> Result<Option<Vec<RefTrack>>> {
    let not_found = |e: &anyhow::Error| e.downcast_ref::<reqwest::Error>().and_then(reqwest::Error::status) == Some(reqwest::StatusCode::NOT_FOUND);
    let release = |id: String| async move {
        match super::track_fetch::release_by_id(&id).await {
            Ok(v) => Ok(Some(list_of_release(&v))),
            Err(e) if not_found(&e) => Ok(None),
            Err(e) => Err(e),
        }
    };
    match (r.source.as_str(), r.kind.as_str()) {
        ("musicbrainz", "release") => release(r.value).await,
        ("musicbrainz", "release-group") => match release_of_group(&r.value).await? {
            Some(id) => release(id).await,
            None => Ok(None),
        },
        ("discogs", "release") => Ok(super::discogs::release_raw(r.value.parse()?).await?.map(|v| list_of_discogs(&v))),
        ("discogs", "master") => match super::discogs::master(r.value.parse()?).await?.and_then(|m| m.main_release) {
            Some(main) => Ok(super::discogs::release_raw(main).await?.map(|v| list_of_discogs(&v))),
            None => Ok(None),
        },
        ("bandcamp", "album") => match crate::bandcamp::Bandcamp::get().album(&r.value).await {
            Ok(a) => Ok(Some(list_of_bandcamp(&a))),
            Err(e) if crate::bandcamp::not_read(&e) => Err(e),
            // Bandcamp has no such page (`Bandcamp::text` says its status in words).
            Err(e) if e.to_string().starts_with("Bandcamp answered 404") => Ok(None),
            Err(e) => Err(e),
        },
        _ => Ok(None),
    }
}

/// The release a release group is judged by: its first official one with tracks, else its first with tracks (0.16's
/// Soulseek's `reference`). None where MusicBrainz lists none with tracks.
async fn release_of_group(group: &str) -> Result<Option<String>> {
    use super::musicbrainz::Asked as _;
    #[derive(Deserialize)]
    struct Browse {
        #[serde(default)]
        releases: Vec<serde_json::Value>,
    }
    let http = super::discover::http()?;
    let url = format!("https://musicbrainz.org/ws/2/release?release-group={group}&inc=media&fmt=json&limit=25");
    let b: Browse = http.get(url).asked().await?.error_for_status()?.json().await?;
    let tracks = |v: &serde_json::Value| v["media"].as_array().map_or(0, |m| m.iter().map(|x| x["track-count"].as_u64().unwrap_or(0)).sum::<u64>());
    let one = b.releases.iter().filter(|v| tracks(v) > 0).find(|v| v["status"].as_str() == Some("Official")).or_else(|| b.releases.iter().find(|v| tracks(v) > 0));
    Ok(one.and_then(|v| v["id"].as_str()).map(str::to_string))
}

/// The record's tracks as a list is held against them: the copy that plays, in its order, each by its title and length.
/// Blocking.
fn files_of(served: &crate::serve::Serve, album: &str) -> Result<Vec<Listed>> {
    Ok(served.library.album_tracks(album)?.into_iter().map(|(_, _, _, title, ms, _, _)| Listed::of(&title, ms as i64)).collect())
}

/// The source that counts for a record (`identity::counting`), from the lists kept and its files, and kept, unless chosen by
/// hand. No list, no count: a certain match from 0.16 counts once its list is read and agrees (`read_lists`). Its counting
/// now. Blocking.
pub fn work_out_in(cat: &Catalogue, record: i64, files: &[Listed]) -> Result<Option<String>> {
    let Some(row) = cat.record(record)? else { return Ok(None) };
    if row.counted_by_hand {
        return Ok(row.counting);
    }
    let kept = cat.reference_tracks(record)?;
    let lists: Vec<(&str, &[RefTrack])> = kept.iter().map(|(s, l)| (s.as_str(), l.as_slice())).filter(|(_, l)| !l.is_empty()).collect();
    let counting = identity::counting(None, &lists, files);
    if counting != row.counting {
        cat.set_counting(record, counting.as_deref(), false)?;
    }
    Ok(counting)
}

/// The lists of records waiting since 0.17's step (`Catalogue::lists_to_read`, 0.16's certain matches), up to `n`, the oldest
/// first: each release's list read (`read`; none, MusicBrainz has no such release), kept where the record still holds the
/// release, and held against its files (`files`) by `work_out_in`, so MusicBrainz counts only where the list agrees
/// (`identity::agrees`). Either way the record waits no more: on Health's references item where nothing speaks for it. A
/// question that fails stops the pass there; the rest wait for the next. How many were read.
pub async fn read_lists<R, F>(cat: &Arc<Catalogue>, files: Arc<dyn Fn(i64) -> Result<Vec<Listed>> + Send + Sync>, read: R, n: usize) -> Result<usize>
where
    R: Fn(String) -> F,
    F: std::future::Future<Output = Result<Option<Vec<RefTrack>>>>,
{
    let waiting = {
        let cat = cat.clone();
        tokio::task::spawn_blocking(move || cat.lists_to_read(n)).await??
    };
    let mut read_n = 0;
    for (record, release) in waiting {
        let list = match read(release.clone()).await {
            Ok(list) => list,
            // MusicBrainz out of reach: the rest wait for the next pass, nothing counted against them.
            Err(e) if out_of_reach(&e) => {
                tracing::info!(target: "library", "the lists 0.17's step left: stopped after {read_n}: {e:#}");
                break;
            }
            // This release's question failed: counted against it, and the rest go on (R7).
            Err(e) => {
                let cat = cat.clone();
                if tokio::task::spawn_blocking(move || cat.list_failed(record, LIST_TRIES)).await?? {
                    tracing::info!(target: "library", "the lists 0.17's step left: record {record} let go of after {LIST_TRIES} tries: {e:#}");
                }
                continue;
            }
        };
        read_n += 1;
        let (cat, files) = (cat.clone(), files.clone());
        tokio::task::spawn_blocking(move || -> Result<()> {
            if let Some(list) = list.filter(|l| !l.is_empty())
                && cat.subject_of(&Ref::new("musicbrainz", "release", &release))? == [ids::record(record)]
            {
                cat.set_reference_tracks(record, "musicbrainz", &list)?;
                // No files to hold it against (the record not served now): kept, and nothing counts by it (R7).
                let files = files(record)?;
                if !files.is_empty() {
                    work_out_in(&cat, record, &files)?;
                }
            }
            cat.list_read(record)
        })
        .await??;
    }
    Ok(read_n)
}

/// How many failed questions a record's list is asked again after, before the record is let go of to Health's item.
const LIST_TRIES: i64 = 3;

/// Whether an error is MusicBrainz out of reach, not an answer about one release: no answer at all, busy or down (a 5xx), limiting
/// its rate (429, R15), or switched off.
fn out_of_reach(e: &anyhow::Error) -> bool {
    let busy = |s: reqwest::StatusCode| s.is_server_error() || s == reqwest::StatusCode::TOO_MANY_REQUESTS;
    e.downcast_ref::<reqwest::Error>().is_some_and(|r| r.is_connect() || r.is_timeout() || r.status().is_some_and(busy)) || !super::switches::is_on(super::switches::MUSICBRAINZ)
}

/// Whether MusicBrainz answered that it will not give this release (no such release, an id it refuses: a 4xx other than its
/// rate limit's 429, which is out of reach).
fn no_such(e: &anyhow::Error) -> bool {
    e.downcast_ref::<reqwest::Error>().and_then(reqwest::Error::status).is_some_and(|s| s.is_client_error() && s != reqwest::StatusCode::TOO_MANY_REQUESTS)
}

/// How many lists `lists_pass` reads in one pass.
const LISTS: usize = 200;

/// The background asker's pass over the lists 0.17's step left to read (`read_lists`), each a question to MusicBrainz on its
/// turn, behind a page's (`musicbrainz::behind`, which the asker sets), every other turn at most. How many were read.
pub async fn lists_pass() -> Result<usize> {
    let Some(served) = crate::serve::instance() else { return Ok(0) };
    let cat = served.catalogue().clone();
    let files: Arc<dyn Fn(i64) -> Result<Vec<Listed>> + Send + Sync> = Arc::new(move |record| files_of(&served, &ids::record(record)));
    let read = |release: String| async move {
        let read = match super::track_fetch::release_by_id(&release).await {
            Ok(v) => Ok(Some(list_of_release(&v))),
            // MusicBrainz will not answer for it (no such release, an id it refuses): no list, let go of at once (R7).
            Err(e) if no_such(&e) => Ok(None),
            Err(e) => Err(e),
        };
        tokio::time::sleep(std::time::Duration::from_secs(super::musicbrainz::PACE_SECS)).await;
        read
    };
    read_lists(&cat, files, read, LISTS).await
}

/// `work_out_in` for an album of Cratarr's own library. Blocking.
pub fn work_out(served: &crate::serve::Serve, album: &str) -> Result<Option<String>> {
    let Some(record) = ids::record_number(album) else { return Ok(None) };
    work_out_in(served.catalogue(), record, &files_of(served, album)?)
}

/// A match matching found (`album_matches` keeps it as before), kept as references too: its release and release group, how
/// the files say (their own release: `files`, else `search`), and the release's own list; the count worked out with it. A
/// certain match takes its release from another record that only held it by its files or a guess: MusicBrainz matched this
/// one's tracks to it (one release's files on two records, G1). Its list is kept only where its release is this record's.
/// Blocking.
pub fn keep_match(served: &crate::serve::Serve, album: &str, m: &super::matching::Match, list: &[RefTrack], named: bool) -> Result<()> {
    let Some(record) = ids::record_number(album) else { return Ok(()) };
    keep_match_in(served.catalogue(), record, album, m, list, named, &files_of(served, album)?)
}

/// `keep_match`, `files` the record's.
fn keep_match_in(cat: &Catalogue, record: i64, album: &str, m: &super::matching::Match, list: &[RefTrack], named: bool, files: &[Listed]) -> Result<()> {
    let how = if named { "files" } else { "search" };
    let release = Ref::new("musicbrainz", "release", &m.release);
    let held = if m.certain { cat.take_reference(album, &release, how)? } else { cat.set_reference(album, &release, how)? };
    if let Some(group) = &m.release_group {
        cat.set_reference(album, &Ref::new("musicbrainz", "release-group", group), how)?;
    }
    if held && !list.is_empty() {
        cat.set_reference_tracks(record, "musicbrainz", list)?;
        work_out_in(cat, record, files)?;
    }
    Ok(())
}

/// What Discogs said of a record (`discogs_facts` keeps it as before), kept as references too: its master and main release,
/// found by the record's name or MusicBrainz's link (the facts do not say which: `search`, which a link at 0.17's step makes
/// surer), and the main release's list; the count worked out with it. Blocking.
pub fn keep_discogs(served: &crate::serve::Serve, album: &str, f: &super::discogs::AlbumFacts) -> Result<()> {
    let Some(record) = ids::record_number(album) else { return Ok(()) };
    keep_discogs_in(served.catalogue(), record, album, f, &files_of(served, album)?)
}

/// `keep_discogs`, `files` the record's.
fn keep_discogs_in(cat: &Catalogue, record: i64, album: &str, f: &super::discogs::AlbumFacts, files: &[Listed]) -> Result<()> {
    if let Some(master) = f.master {
        cat.set_reference(album, &Ref::new("discogs", "master", &master.to_string()), "search")?;
    }
    let held = match f.release {
        Some(release) => cat.set_reference(album, &Ref::new("discogs", "release", &release.to_string()), "search")?,
        None => false,
    };
    if held && !f.tracks.is_empty() {
        cat.set_reference_tracks(record, "discogs", &f.tracks)?;
        work_out_in(cat, record, files)?;
    }
    Ok(())
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
    for record in served.catalogue().unreferenced(super::switches::is_on(super::switches::MUSICBRAINZ))? {
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
        assert_eq!(r("https://www.discogs.com/master/view/456"), named("discogs", "master", "456"), "Discogs' older link, kept by 0.16");
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

    fn matched(release: &str, certain: bool) -> crate::app::matching::Match {
        crate::app::matching::Match {
            release: release.into(),
            release_group: Some("00000000-0000-4000-8000-000000000101".into()),
            title: "Tide Tables".into(),
            artist: "Glass Harbour".into(),
            artist_id: None,
            year: Some(2012),
            certain,
            why: String::new(),
            tracks: Vec::new(),
            lengths: Vec::new(),
        }
    }

    /// A match kept as references, with the release's own list (C43b). A likely one takes nothing another record holds; a
    /// certain one does, as MusicBrainz matched its tracks to it.
    #[test]
    fn a_match_becomes_a_reference_with_its_tracklist() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let ((record, album), (_, other)) = (had(&cat, "Tide Tables", 2), had(&cat, "Tide Tables (Bonus)", 3));
        let tracks = files(&[("Slack Water", 200_000), ("Neap", 180_000)]);
        let own = list(&[("Slack Water", Some(200_400)), ("Neap", Some(180_000))]);
        let release = Ref::new("musicbrainz", "release", "00000000-0000-4000-8000-000000000001");
        cat.set_reference(&other, &release, "files").unwrap();
        keep_match_in(&cat, record, &album, &matched(&release.value, false), &own, false, &tracks).unwrap();
        assert_eq!(cat.subject_of(&release).unwrap(), [other.clone()], "a likely match takes nothing");
        assert!(cat.reference_tracks(record).unwrap().is_empty(), "nor keeps a list of a release not its own");
        keep_match_in(&cat, record, &album, &matched(&release.value, true), &own, false, &tracks).unwrap();
        assert_eq!(cat.subject_of(&release).unwrap(), [album.clone()], "a certain match: its tracks are the release's");
        let how = || cat.references_of(&album).unwrap().into_iter().map(|(r, row)| (r.kind, row.how)).collect::<Vec<_>>();
        assert_eq!(how(), [("release".to_string(), "search".to_string()), ("release-group".to_string(), "search".to_string())]);
        assert_eq!(cat.reference_tracks(record).unwrap(), [("musicbrainz".to_string(), own.clone())], "the release's titles");
        assert_eq!(cat.record(record).unwrap().unwrap().counting.as_deref(), Some("musicbrainz"), "worked out as it arrived");
        keep_match_in(&cat, record, &album, &matched(&release.value, true), &own, true, &tracks).unwrap();
        assert!(how().iter().all(|(_, how)| how == "files"), "matched again by its files' own release: surer");
    }

    /// The count is worked out as each list arrives, and follows the lists (C43b): none fitting, none counting.
    #[test]
    fn counting_follows_a_new_list() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (record, album) = had(&cat, "Tide Tables", 2);
        let tracks = files(&[("Slack Water", 200_000), ("Neap", 180_000)]);
        let fits = list(&[("Slack Water", Some(200_000)), ("Neap", Some(180_000))]);
        let bonus = list(&[("Slack Water", Some(200_000)), ("Neap", Some(180_000)), ("Bonus", Some(90_000))]);
        let counting = || cat.record(record).unwrap().unwrap().counting;
        assert_eq!(work_out_in(&cat, record, &tracks).unwrap(), None, "no list");
        let discogs = crate::app::discogs::AlbumFacts { master: Some(1000001), release: Some(2000001), tracks: bonus.clone(), ..Default::default() };
        keep_discogs_in(&cat, record, &album, &discogs, &tracks).unwrap();
        assert_eq!(counting(), None, "Discogs' main release has a bonus track");
        assert_eq!(cat.references_of(&album).unwrap().len(), 2, "its master and release, kept all the same");
        keep_match_in(&cat, record, &album, &matched("00000000-0000-4000-8000-000000000001", true), &fits, false, &tracks).unwrap();
        assert_eq!(counting().as_deref(), Some("musicbrainz"));
        cat.set_reference_tracks(record, "bandcamp", &fits).unwrap();
        work_out_in(&cat, record, &tracks).unwrap();
        assert_eq!(counting().as_deref(), Some("musicbrainz"), "of two that fit, the first in order");
        cat.set_reference_tracks(record, "musicbrainz", &bonus).unwrap();
        work_out_in(&cat, record, &tracks).unwrap();
        assert_eq!(counting().as_deref(), Some("bandcamp"), "MusicBrainz's read again with another track");
        cat.set_reference_tracks(record, "bandcamp", &bonus).unwrap();
        work_out_in(&cat, record, &tracks).unwrap();
        assert_eq!(counting(), None, "none fits: none counts");
        chose(&cat, record, &album, &Choose { counting: Some(Some("discogs".into())), ..Choose::default() }, &tracks).unwrap();
        cat.set_reference_tracks(record, "musicbrainz", &fits).unwrap();
        work_out_in(&cat, record, &tracks).unwrap();
        assert_eq!(counting().as_deref(), Some("discogs"), "chosen by hand, whatever arrives");
    }

    /// A record waiting for a list, its release its own, and a list of `titles` for it.
    fn waiting(cat: &Catalogue, title: &str, release: &str) -> i64 {
        let record = cat.record_for_tests(title, 1);
        assert!(cat.set_reference(&ids::record(record), &Ref::new("musicbrainz", "release", release), "search").unwrap());
        cat.wait_for_list_for_tests(record, release);
        record
    }

    fn files_none() -> Arc<dyn Fn(i64) -> Result<Vec<Listed>> + Send + Sync> {
        Arc::new(|_| Ok(Vec::new()))
    }

    /// R7 (N3): a list read for a record with no files to hold it against (not served now) is kept, and nothing counts by it:
    /// the record waits no more, and is on Health's item.
    #[tokio::test]
    async fn a_list_read_with_no_files_counts_nothing() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = waiting(&cat, "Signal", "r7");
        let read = |_| async { Ok(Some(list(&[("Signal", Some(180_000))]))) };
        assert_eq!(read_lists(&cat, files_none(), read, 10).await.unwrap(), 1);
        assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "no files, no count");
        assert_eq!(cat.reference_tracks(record).unwrap().len(), 1, "the list kept");
        assert!(cat.lists_to_read(10).unwrap().is_empty());
        assert_eq!(cat.unreferenced(true).unwrap(), [record]);
    }

    /// R7 (N4): a release whose question always fails lets the rest through: the others are read in the same pass, and it is
    /// let go of after its tries, onto Health's item; MusicBrainz refusing it (a 4xx, `lists_pass`) lets it go at once.
    #[tokio::test]
    async fn a_release_that_always_fails_lets_the_rest_through() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let (broken, fine) = (waiting(&cat, "Broken", "r1"), waiting(&cat, "Signal", "r2"));
        let files: Arc<dyn Fn(i64) -> Result<Vec<Listed>> + Send + Sync> = Arc::new(|_| Ok(files(&[("Signal", 180_000)])));
        let read = |release: String| async move { if release == "r1" { anyhow::bail!("not understood") } else { Ok(Some(list(&[("Signal", Some(180_000))]))) } };
        assert_eq!(read_lists(&cat, files.clone(), read, 10).await.unwrap(), 1, "the other read in the same pass");
        assert_eq!(cat.record(fine).unwrap().unwrap().counting.as_deref(), Some("musicbrainz"));
        assert_eq!(cat.lists_to_read(10).unwrap(), [(broken, "r1".to_string())], "tried again later");
        for _ in 1..LIST_TRIES {
            read_lists(&cat, files.clone(), read, 10).await.unwrap();
        }
        assert!(cat.lists_to_read(10).unwrap().is_empty(), "let go of after its tries");
        assert_eq!(cat.unreferenced(true).unwrap(), [broken]);
    }

    /// A server of the test's own answering every question with `status`: what MusicBrainz busy, down or limiting its rate is to
    /// a reader, as `error_for_status` gives it.
    async fn serving(status: u16) -> String {
        let app = axum::Router::new().fallback(move || async move { axum::http::StatusCode::from_u16(status).unwrap() });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/ws/2/release/r1", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        url
    }

    /// The error a reader meets asking `url`.
    async fn asked(url: String) -> anyhow::Error {
        reqwest::get(url).await.unwrap().error_for_status().unwrap_err().into()
    }

    /// R15 (F4): MusicBrainz busy or down (a 5xx) or limiting its rate (a 429) is out of reach, never the release's own failure:
    /// each pass stops at it, and no record waiting for a list is counted against or let go of, however many passes it lasts.
    #[tokio::test]
    async fn a_musicbrainz_outage_lets_no_record_go() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let mut waits = vec![waiting(&cat, "Signal", "r1"), waiting(&cat, "Lamplight", "r2")];
        waits.sort();
        for status in [503, 502, 429] {
            let url = serving(status).await;
            let read = |_| {
                let url = url.clone();
                async move { Err(asked(url).await) }
            };
            for _ in 0..=LIST_TRIES {
                assert_eq!(read_lists(&cat, files_none(), read, 10).await.unwrap(), 0);
            }
            let mut left: Vec<i64> = cat.lists_to_read(10).unwrap().into_iter().map(|(r, _)| r).collect();
            left.sort();
            assert_eq!(left, waits, "{status}: both still wait");
        }
        assert!(cat.unreferenced(true).unwrap().is_empty(), "none on Health's item");
    }

    /// R15 (F4): a release MusicBrainz says it will not give (a 404) has no list, at once; its rate limit (a 429) is no such answer.
    #[tokio::test]
    async fn a_musicbrainz_rate_limit_is_no_answer_about_a_release() {
        assert!(no_such(&asked(serving(404).await).await));
        assert!(!no_such(&asked(serving(429).await).await));
        assert!(!no_such(&asked(serving(503).await).await));
    }

    /// R7 (N4): with MusicBrainz switched off no list comes, so a record waiting for one is on Health's item.
    #[test]
    fn with_musicbrainz_off_waiting_records_are_on_health() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let record = waiting(&cat, "Signal", "r7");
        assert!(cat.unreferenced(true).unwrap().is_empty(), "MusicBrainz on: it waits");
        assert_eq!(cat.unreferenced(false).unwrap(), [record], "off: on the item");
    }

    /// A record not had, of `title`, holding these references (`link`).
    fn wanted(cat: &Catalogue, title: &str, refs: &[(&str, &str, &str)]) -> i64 {
        let record = cat.record_for_tests(title, 0);
        for (source, kind, value) in refs {
            assert!(cat.set_reference(&ids::record(record), &Ref::new(source, kind, value), "link").unwrap());
        }
        record
    }

    /// A reader answering each reference by its source: a list of `titles` for those named, the rest as `other` says; it
    /// counts what it was asked.
    fn reader(lists: &'static [(&'static str, &'static [&'static str])], asked: Arc<std::sync::Mutex<Vec<String>>>, other: fn() -> Result<Option<Vec<RefTrack>>>) -> impl Fn(Ref) -> std::future::Ready<Result<Option<Vec<RefTrack>>>> {
        move |r: Ref| {
            asked.lock().unwrap().push(format!("{}:{}", r.source, r.kind));
            let answer = match lists.iter().find(|(s, _)| *s == r.source) {
                Some((_, titles)) => Ok(Some(list(&titles.iter().map(|t| (*t, Some(200_000))).collect::<Vec<_>>()))),
                None => other(),
            };
            std::future::ready(answer)
        }
    }

    /// C51a: a record's counting list is read once and kept: asked again, the kept list answers and nothing is read.
    #[tokio::test]
    async fn tracklist_reads_once_and_keeps() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Signal", &[("musicbrainz", "release", "r7")]);
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let read = reader(&[("musicbrainz", &["Signal"])], asked.clone(), || Ok(None));
        let (source, first) = tracklist(&cat, record, &read).await.unwrap().unwrap();
        let (_, again) = tracklist(&cat, record, &read).await.unwrap().unwrap();
        assert_eq!((source.as_str(), first.len(), again), ("musicbrainz", 1, first.clone()));
        assert_eq!(asked.lock().unwrap().len(), 1, "read once");
        assert_eq!(cat.reference_tracks(record).unwrap(), [("musicbrainz".to_string(), first)], "kept");
    }

    /// C51a: the list that counts is the counting source's: a record counted by Discogs reads Discogs', though MusicBrainz is
    /// first in the order.
    #[tokio::test]
    async fn tracklist_follows_counting() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Two Rooms", &[("musicbrainz", "release", "r2"), ("discogs", "release", "2000002")]);
        cat.set_counting(record, Some("discogs"), true).unwrap();
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let read = reader(&[("musicbrainz", &["Hall"]), ("discogs", &["Hall", "Stair"])], asked.clone(), || Ok(None));
        let (source, list) = tracklist(&cat, record, &read).await.unwrap().unwrap();
        assert_eq!((source.as_str(), list.len()), ("discogs", 2));
        assert_eq!(*asked.lock().unwrap(), ["discogs:release"]);
    }

    /// C51a: a record not had counts its first reference in the design's order whose list answers: MusicBrainz before Discogs
    /// before Bandcamp; one answering nothing (no such release) gives way to the next. That source counts from then on.
    #[tokio::test]
    async fn a_record_not_had_counts_its_first_reference_in_order() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let all = wanted(&cat, "Night Walks", &[("bandcamp", "album", "https://kitetheory.bandcamp.com/album/night-walks"), ("discogs", "release", "2000003"), ("musicbrainz", "release", "r3")]);
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let every = reader(&[("musicbrainz", &["Lamplight"]), ("discogs", &["Lamplight"]), ("bandcamp", &["Lamplight"])], asked.clone(), || Ok(None));
        assert_eq!(tracklist(&cat, all, &every).await.unwrap().unwrap().0, "musicbrainz");
        assert_eq!(cat.record(all).unwrap().unwrap().counting.as_deref(), Some("musicbrainz"), "it counts from then on");
        let gone = wanted(&cat, "Undated", &[("bandcamp", "album", "https://kitetheory.bandcamp.com/album/undated"), ("musicbrainz", "release", "r4")]);
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mb_gone = reader(&[("bandcamp", &["Morning"])], asked.clone(), || Ok(None));
        assert_eq!(tracklist(&cat, gone, &mb_gone).await.unwrap().unwrap().0, "bandcamp", "MusicBrainz has no such release: the next");
        assert_eq!(*asked.lock().unwrap(), ["musicbrainz:release", "bandcamp:album"]);
    }

    /// C51a, the owner's rule: a record with files where no list agrees counts nothing, and nothing is read for it, until an
    /// administrator picks a list. Its own lists are not tried in order: that is a record not had's way alone.
    #[tokio::test]
    async fn a_record_with_files_where_nothing_agrees_counts_nothing() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = cat.record_for_tests("Tide Tables", 2);
        assert!(cat.set_reference(&ids::record(record), &Ref::new("musicbrainz", "release", "r1"), "files").unwrap());
        cat.set_reference_tracks(record, "musicbrainz", &list(&[("Other", Some(1_000)), ("Neap", Some(180_000))])).unwrap();
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let read = reader(&[("musicbrainz", &["Slack Water", "Neap"])], asked.clone(), || Ok(None));
        assert_eq!(tracklist(&cat, record, &read).await.unwrap(), Err("No list counts until one is chosen".to_string()));
        assert!(asked.lock().unwrap().is_empty(), "nothing read");
        assert_eq!(cat.record(record).unwrap().unwrap().counting, None);
    }

    /// C51a: Bandcamp's bot check where Bandcamp is the only reference: no list, and why in few words; nothing counts.
    #[tokio::test]
    async fn a_record_with_only_bandcamp_behind_its_check_has_no_list() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "After Hours", &[("bandcamp", "album", "https://paperlantern.bandcamp.com/album/after-hours")]);
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let checked = reader(&[], asked, || Err(anyhow::Error::from(crate::bandcamp::NotRead)));
        assert_eq!(tracklist(&cat, record, &checked).await.unwrap(), Err("Bandcamp didn't answer".to_string()));
        let none = wanted(&cat, "Quiet Lanes", &[]);
        assert_eq!(tracklist(&cat, none, &checked).await.unwrap(), Err("No list to judge by".to_string()));
        assert_eq!(cat.record(record).unwrap().unwrap().counting, None);
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
        assert_eq!(cat.unreferenced(true).unwrap(), [alone, guessed], "a guess and a group are no reference that speaks");
    }
}
