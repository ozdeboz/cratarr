//! Renumber (0.9.6, the owner): an album's disc and track numbers set from its release's order. MusicBrainz's releases
//! of the album's release group first, then Discogs' of its master; a digital release preferred, else a CD, else vinyl
//! or a cassette. Discs are kept where they are CD-sized (the grouping makes sense, as a CD's does, not as a record's
//! sides do): a CD's are, a record's and a cassette's are put one after another on one disc, numbered from 01; a
//! digital release's are kept when a CD of the group has as many, or each runs over 40 minutes. With no release that
//! holds every track, the files' own order on their discs. Written into the tags (the audio untouched), the old numbers
//! kept first for Undo.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use super::musicbrainz::Asked as _;

/// A track matches a release's when their lengths are this close (s).
const LENGTH_SLACK: i64 = 5;
/// A digital release's medium this long or longer is CD-sized (s).
const CD_SIZED_SECS: u32 = 40 * 60;
/// Discogs releases read at most, the likeliest formats first (Discogs is paced: one a second).
const DISCOGS_RELEASES: usize = 4;

/// One of the album's tracks as the library has it.
#[derive(Debug, Clone)]
pub struct Track {
    pub id: String,
    pub path: PathBuf,
    pub file: String,
    pub title: String,
    pub secs: u32,
    pub disc: Option<u32>,
    pub track: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct At {
    pub disc: u32,
    pub track: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Now {
    pub disc: Option<u32>,
    pub track: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub id: String,
    pub file: String,
    pub title: String,
    pub now: Now,
    pub to: At,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReleaseFacts {
    pub title: String,
    pub format: String,
    pub country: Option<String>,
    pub year: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Totals {
    pub discs: u32,
    /// Each disc's track count, disc 1 first.
    pub tracks_per_disc: Vec<u32>,
}

/// What Renumber would write.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Preview {
    /// `musicbrainz`, `discogs` or `files`.
    pub from: &'static str,
    pub release: Option<ReleaseFacts>,
    /// `kept` or `flattened`.
    pub discs: &'static str,
    /// Why: "CD", "Vinyl", "Cassette", "Digital, CD-sized", "Digital", "files".
    pub why: String,
    pub tracks: Vec<Row>,
    pub totals: Totals,
}

/// What a release's format is, as far as Renumber cares: its preference and its discs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Digital,
    Cd,
    /// Vinyl, cassette: sides and discs put one after another.
    Record,
    Other,
}

fn kind_of(format: &str) -> Kind {
    let f = format.to_ascii_lowercase();
    // MusicBrainz's "Digital Media"; Discogs' "File" ("File, FLAC, Album" in a master's versions).
    if f.contains("digital") || f.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| w == "file") {
        Kind::Digital
    } else if f.contains("cd") {
        Kind::Cd
    } else if f.contains("vinyl") || f.contains("cassette") || f.contains("lp") || f.contains("acetate") {
        Kind::Record
    } else {
        Kind::Other
    }
}

/// A release's medium: its tracks' titles and lengths (s), in order.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Medium {
    pub(super) format: String,
    pub(super) tracks: Vec<(String, Option<u32>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Release {
    /// MusicBrainz's id (none for a Discogs release).
    pub(super) id: String,
    pub(super) facts: ReleaseFacts,
    pub(super) media: Vec<Medium>,
}

impl Release {
    /// Its kind: by its first medium's format (a release of mixed media is rare).
    fn kind(&self) -> Kind {
        self.media.first().map_or(Kind::Other, |m| kind_of(&m.format))
    }
}

/// A title as compared: loose, its "(Remastered)" or "- Live" left off too.
fn titled(t: &str) -> [String; 2] {
    let base = t.split(" (").next().unwrap_or(t).split(" [").next().unwrap_or(t).split(" - ").next().unwrap_or(t);
    [super::import::loose(t), super::import::loose(base)]
}

pub(super) fn same_title(a: &str, b: &str) -> bool {
    let (x, y) = (titled(a), titled(b));
    !x[0].is_empty() && (x[0] == y[0] || (!x[1].is_empty() && x[1] == y[1]))
}

/// Where each of the album's tracks is on the release (medium, position, both from 0), when every one is there: by title,
/// the closest length first.
fn place_on(tracks: &[Track], r: &Release) -> Option<Vec<(usize, usize)>> {
    let mut used = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(tracks.len());
    for t in tracks {
        let best = r
            .media
            .iter()
            .enumerate()
            .flat_map(|(m, med)| med.tracks.iter().enumerate().map(move |(p, x)| (m, p, x)))
            .filter(|(m, p, (title, secs))| {
                !used.contains(&(*m, *p))
                    && same_title(&t.title, title)
                    && secs.is_none_or(|s| t.secs == 0 || (i64::from(s) - i64::from(t.secs)).abs() <= LENGTH_SLACK)
            })
            .min_by_key(|(_, _, (_, secs))| secs.map_or(i64::MAX, |s| (i64::from(s) - i64::from(t.secs)).abs()))?;
        used.insert((best.0, best.1));
        out.push((best.0, best.1));
    }
    Some(out)
}

/// Whether a release's media are kept as discs (and why), given the group's other releases (a CD of as many media).
fn keeps_discs(r: &Release, group: &[Release]) -> (bool, String) {
    match r.kind() {
        Kind::Cd => (true, "CD".into()),
        Kind::Record => (false, if r.media.iter().any(|m| m.format.to_ascii_lowercase().contains("cassette")) { "Cassette".into() } else { "Vinyl".into() }),
        Kind::Digital | Kind::Other if r.media.len() > 1 => {
            let as_cd = group.iter().any(|g| g.kind() == Kind::Cd && g.media.len() == r.media.len());
            let long = r.media.iter().all(|m| m.tracks.iter().map(|(_, s)| s.unwrap_or(0)).sum::<u32>() >= CD_SIZED_SECS);
            let name = if r.kind() == Kind::Digital { "Digital" } else { "Release" };
            if as_cd || long { (true, format!("{name}, CD-sized")) } else { (false, name.to_string()) }
        }
        Kind::Digital => (true, "Digital".into()),
        Kind::Other => (true, "Release".into()),
    }
}

/// The preview from a release the tracks are all on.
fn from_release(tracks: &[Track], r: &Release, places: &[(usize, usize)], group: &[Release], from: &'static str) -> Preview {
    let (kept, why) = keeps_discs(r, group);
    let lens: Vec<u32> = r.media.iter().map(|m| m.tracks.len() as u32).collect();
    let rows = tracks
        .iter()
        .zip(places)
        .map(|(t, &(m, p))| {
            let to = if kept { At { disc: m as u32 + 1, track: p as u32 + 1 } } else { At { disc: 1, track: lens[..m].iter().sum::<u32>() + p as u32 + 1 } };
            Row { id: t.id.clone(), file: t.file.clone(), title: t.title.clone(), now: Now { disc: t.disc, track: t.track }, to }
        })
        .collect();
    let totals = if kept { Totals { discs: lens.len() as u32, tracks_per_disc: lens } } else { Totals { discs: 1, tracks_per_disc: vec![lens.iter().sum()] } };
    Preview { from, release: Some(r.facts.clone()), discs: if kept { "kept" } else { "flattened" }, why, tracks: rows, totals }
}

/// The preview from the files' own order: each disc they are on (none: 1) kept, its tracks in file-name order from 01.
fn from_files(tracks: &[Track]) -> Preview {
    let mut order: Vec<&Track> = tracks.iter().collect();
    order.sort_by(|a, b| a.disc.unwrap_or(1).cmp(&b.disc.unwrap_or(1)).then_with(|| a.file.cmp(&b.file)));
    let discs = order.iter().map(|t| t.disc.unwrap_or(1)).max().unwrap_or(1);
    let mut per = vec![0u32; discs as usize];
    let rows = order
        .into_iter()
        .map(|t| {
            let d = t.disc.unwrap_or(1).max(1);
            per[d as usize - 1] += 1;
            Row { id: t.id.clone(), file: t.file.clone(), title: t.title.clone(), now: Now { disc: t.disc, track: t.track }, to: At { disc: d, track: per[d as usize - 1] } }
        })
        .collect();
    Preview { from: "files", release: None, discs: "kept", why: "files".into(), tracks: rows, totals: Totals { discs, tracks_per_disc: per } }
}

/// The release to number by: of those every track is on, the preferred kind first (digital, CD, record), then the one
/// closest in size to the album.
fn choose<'a>(tracks: &[Track], releases: &'a [Release]) -> Option<(&'a Release, Vec<(usize, usize)>)> {
    releases
        .iter()
        .filter_map(|r| place_on(tracks, r).map(|p| (r, p)))
        .min_by_key(|(r, _)| (r.kind(), r.media.iter().map(|m| m.tracks.len()).sum::<usize>().abs_diff(tracks.len())))
}

/// The discs Renumber would number the album's tracks on from these releases (0.13, Health's Disc numbers): its own
/// choice of release, and the discs only where it keeps them (a double LP stays one disc); none where no release holds
/// every track.
pub(super) fn discs_from(tracks: &[Track], releases: &[Release]) -> Option<u32> {
    let (r, places) = choose(tracks, releases)?;
    if !keeps_discs(r, releases).0 {
        return Some(1);
    }
    Some(places.iter().map(|(m, _)| *m).collect::<std::collections::HashSet<_>>().len() as u32)
}

// ---- MusicBrainz

fn mb_release(v: &Value) -> Release {
    let media = v["media"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| Medium {
            format: m["format"].as_str().unwrap_or_default().to_string(),
            tracks: m["tracks"].as_array().into_iter().flatten().map(|t| (t["title"].as_str().unwrap_or_default().to_string(), t["length"].as_u64().map(|ms| (ms / 1000) as u32))).collect(),
        })
        .collect::<Vec<_>>();
    let format = media.first().map(|m| m.format.clone()).unwrap_or_default();
    Release {
        id: v["id"].as_str().unwrap_or_default().to_string(),
        facts: ReleaseFacts {
            title: v["title"].as_str().unwrap_or_default().to_string(),
            format: if media.len() > 1 { format!("{}× {format}", media.len()) } else { format },
            country: v["country"].as_str().map(str::to_string),
            year: v["date"].as_str().and_then(|d| d.get(..4)).and_then(|y| y.parse().ok()),
        },
        media,
    }
}

/// The release group's releases with their tracks (one call, at most 100 releases).
async fn musicbrainz(http: &reqwest::Client, release: &str) -> Result<Vec<Release>> {
    let Some(rg) = super::discogs::release_group_of(http, release).await? else { return Ok(Vec::new()) };
    group_releases(http, &rg).await
}

/// A release group's releases with their tracks (one call, at most 100 releases): Renumber's, and `incomplete`'s.
pub(super) async fn group_releases(http: &reqwest::Client, rg: &str) -> Result<Vec<Release>> {
    let v: Value = http
        .get(format!("https://musicbrainz.org/ws/2/release?release-group={rg}&inc=media+recordings&limit=100&fmt=json"))
        .asked()
        .await?
        .error_for_status()?
        .json()
        .await?;
    tokio::time::sleep(super::discover::MB_PAUSE).await;
    Ok(v["releases"].as_array().into_iter().flatten().map(mb_release).collect())
}

// ---- Discogs

/// "4:05" as seconds.
pub(super) fn duration(s: &str) -> Option<u32> {
    let mut secs = 0u32;
    for part in s.trim().split(':') {
        secs = secs * 60 + part.trim().parse::<u32>().ok()?;
    }
    (secs > 0).then_some(secs)
}

/// A Discogs tracklist position's medium: "2-5" disc 2, "B3" side B, "3" the one.
fn medium_of(position: &str) -> String {
    let p = position.trim();
    if let Some((d, _)) = p.split_once(['-', '.']) {
        return d.to_string();
    }
    p.chars().take_while(|c| c.is_ascii_alphabetic()).collect()
}

pub(super) fn discogs_release(v: &Value) -> Release {
    let format = v["formats"].as_array().and_then(|f| f.first()).and_then(|f| f["name"].as_str()).unwrap_or_default().to_string();
    let mut media: Vec<(String, Medium)> = Vec::new();
    for t in v["tracklist"].as_array().into_iter().flatten().filter(|t| t["type_"].as_str().is_none_or(|k| k == "track")) {
        let key = medium_of(t["position"].as_str().unwrap_or_default());
        let track = (t["title"].as_str().unwrap_or_default().to_string(), t["duration"].as_str().and_then(duration));
        match media.last_mut() {
            Some((k, m)) if *k == key => m.tracks.push(track),
            _ => media.push((key, Medium { format: format.clone(), tracks: vec![track] })),
        }
    }
    Release {
        id: String::new(),
        facts: ReleaseFacts {
            title: v["title"].as_str().unwrap_or_default().to_string(),
            format,
            country: v["country"].as_str().map(str::to_string),
            year: v["year"].as_u64().filter(|y| *y > 0).map(|y| y as u32),
        },
        media: media.into_iter().map(|(_, m)| m).collect(),
    }
}

/// The master's releases, the likeliest formats first (files, CDs, records), a few read.
async fn discogs(rg: Option<&str>, artist: &str, title: &str, tell: &(dyn Fn(u64, Option<u64>) + Sync)) -> Result<Vec<Release>> {
    let Some(master) = super::discogs::master_of(rg, artist, title).await? else { return Ok(Vec::new()) };
    let mut ids = super::discogs::version_formats(master, 50).await?;
    ids.sort_by_key(|(_, f)| kind_of(f));
    ids.truncate(DISCOGS_RELEASES);
    let total = Some(ids.len() as u64);
    let mut out = Vec::new();
    for (n, (id, _)) in ids.into_iter().enumerate() {
        tell(n as u64, total);
        if let Some(v) = super::discogs::release_raw(id).await? {
            out.push(discogs_release(&v));
        }
    }
    Ok(out)
}

// ---- The look and the writing

pub(super) fn tracks_of(album: &str) -> Result<(Vec<Track>, Option<String>, String, String)> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let (folder, _, mbid, artist, name) = served.library.album_place(album)?.context("No such album in Cratarr's own library")?;
    let root = served.library.roots().into_iter().find(|(f, _)| *f == folder).map(|(_, r)| r).context("Its library is not there")?;
    let tracks = served
        .library
        .album_tracks(album)?
        .into_iter()
        .map(|(id, rel, title, ms, disc, track)| Track {
            id,
            file: Path::new(&rel).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| rel.clone()),
            path: Path::new(&root).join(&rel),
            title,
            secs: (ms / 1000) as u32,
            disc,
            track,
        })
        .collect();
    Ok((tracks, mbid.filter(|m| super::discogs::is_mbid(m)), artist, name))
}

async fn look(album: &str, progress: &(dyn Fn(&str, u64, Option<u64>) + Send + Sync)) -> Result<Preview> {
    let a = album.to_string();
    let (tracks, release, artist, name) = tokio::task::spawn_blocking(move || tracks_of(&a)).await??;
    if tracks.is_empty() {
        bail!("That album has no tracks");
    }
    let http = super::discover::http()?;
    progress("MusicBrainz", 0, Some(1));
    let group = match &release {
        Some(r) => musicbrainz(&http, r).await.unwrap_or_else(|e| {
            tracing::info!("renumber: MusicBrainz: {e:#}");
            Vec::new()
        }),
        None => Vec::new(),
    };
    progress("MusicBrainz", 1, Some(1));
    if let Some((r, places)) = choose(&tracks, &group) {
        return Ok(from_release(&tracks, r, &places, &group, "musicbrainz"));
    }
    let rg = match &release {
        Some(r) => super::discogs::release_group_of(&http, r).await.unwrap_or_default(),
        None => None,
    };
    let tell = |n: u64, total: Option<u64>| progress("Discogs", n, total);
    let found = discogs(rg.as_deref(), &artist, &name, &tell).await.unwrap_or_else(|e| {
        tracing::info!("renumber: Discogs: {e:#}");
        Vec::new()
    });
    if let Some((r, places)) = choose(&tracks, &found) {
        return Ok(from_release(&tracks, r, &places, &found, "discogs"));
    }
    Ok(from_files(&tracks))
}

/// Looks up the album's order as a task (Discogs is paced); its result is the `Preview`. A look running for the album is
/// that one.
pub fn start(account: &str, album: &str) -> Result<super::tasks::Task, String> {
    let kind = format!("renumber:{album}");
    if let Some(t) = super::tasks::tasks().of(account).into_iter().find(|t| t.kind == kind && t.finished.is_none()) {
        return Ok(t);
    }
    let a = album.to_string();
    super::tasks::spawn(&kind, account, false, move |h| async move {
        let progress = move |name: &str, done: u64, total: Option<u64>| h.step(name, done, total);
        let preview = look(&a, &progress).await?;
        // Matched to a release (the owner, 2026-10-26): its gaps are tracks not held; Health leaves them out.
        if preview.from != "files"
            && let Some(serve) = crate::serve::instance()
        {
            let (ud, album) = (serve.userdata.clone(), a.clone());
            tokio::task::spawn_blocking(move || ud.set_renumber_matched(&album, super::moment::now_secs())).await??;
        }
        Ok(preview)
    })
}

/// Renumber on the one path (decision 0092's writers' map, 0.14): its kind of change, `renumber:<album>`.
pub const KIND: &str = "renumber";

/// The numbers a look's preview gives each track (its JSON, as the task keeps it), with the totals, as the tag reader
/// names the fields: each track's tracknumber, totaltracks, discnumber and totaldiscs.
fn planned(preview: &serde_json::Value) -> (Vec<(String, Vec<(String, u32)>)>, String) {
    let totals = &preview["totals"];
    let discs = totals["discs"].as_u64().unwrap_or(1) as u32;
    let per: Vec<u32> = totals["tracks_per_disc"].as_array().map(|a| a.iter().filter_map(|n| n.as_u64()).map(|n| n as u32).collect()).unwrap_or_default();
    let tracks = preview["tracks"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| {
                    let (disc, track) = (r["to"]["disc"].as_u64()? as u32, r["to"]["track"].as_u64()? as u32);
                    let mut fields = vec![("tracknumber".to_string(), track)];
                    if let Some(total) = per.get(disc.saturating_sub(1) as usize).copied().filter(|n| *n >= track) {
                        fields.push(("totaltracks".to_string(), total));
                    }
                    fields.push(("discnumber".to_string(), disc));
                    fields.push(("totaldiscs".to_string(), discs.max(disc)));
                    Some((r["id"].as_str()?.to_string(), fields))
                })
                .collect()
        })
        .unwrap_or_default();
    let r = &preview["release"];
    let title = match (r["format"].as_str(), r["year"].as_u64()) {
        (Some(f), Some(y)) => format!("Renumber · the {y} {f} release"),
        (Some(f), None) => format!("Renumber · the {f} release"),
        _ => "Renumber · the files' order".to_string(),
    };
    (tracks, title)
}

/// The proposal of a finished look: each track's numbers now → as the release has them, only where they differ.
/// Blocking.
fn proposal(album: &str, preview: &serde_json::Value) -> Result<super::file_runs::Proposal> {
    let (tracks, artist, name) = tracks_of(album).map(|(t, _, a, n)| (t, a, n))?;
    let (numbers, title) = planned(preview);
    let mut p = super::file_runs::Proposal { kind: format!("{KIND}:{album}"), title, ..Default::default() };
    for (id, fields) in numbers {
        let Some(t) = tracks.iter().find(|t| t.id == id) else { continue };
        let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
        let now = super::file_runs::fields_of(&t.path, &names)?;
        let (mut before, mut after, mut from) = (Vec::new(), Vec::new(), Vec::new());
        for (n, v) in fields {
            let had = now.get(&n).cloned().unwrap_or_default();
            let want = vec![v.to_string()];
            if super::file_runs::flat(&had) != want {
                before.push((n.clone(), had));
                after.push((n.clone(), want));
                from.push((n, preview["from"].as_str().unwrap_or("files").to_string()));
            }
        }
        if !after.is_empty() {
            p.files.push(crate::journal::FileWrite { album: album.to_string(), path: t.path.clone(), track: Some(t.title.clone()), before, after, from, undone: false, whole: None });
        }
    }
    if !p.files.is_empty() {
        p.records.push(crate::journal::RunRecord { album: album.to_string(), title: name, artist });
    }
    Ok(p)
}

/// The look's task for the album, started where none is (Discogs is paced); its preview once found.
fn looked(account: &str, album: &str) -> Result<Option<serde_json::Value>> {
    let t = start(account, album).map_err(|e| anyhow::anyhow!(e))?;
    if t.finished.is_none() {
        return Ok(None);
    }
    if let Some(e) = t.error {
        bail!("{e}");
    }
    t.result.context("The look found nothing").map(Some)
}

/// Renumber's review sheet: each track's disc and number now → after; `making` while the release is looked up (the
/// sheet asks again).
pub async fn plan(account: &str, album: &str) -> Result<super::file_runs::PlanView> {
    let Some(preview) = looked(account, album)? else {
        return Ok(super::file_runs::PlanView { kind: format!("{KIND}:{album}"), title: "Renumber".into(), records: Vec::new(), making: true });
    };
    let a = album.to_string();
    let p = tokio::task::spawn_blocking(move || proposal(&a, &preview)).await??;
    Ok(super::file_runs::view(&p.kind, &p.title, &p.records, &p.files, false))
}

/// The numbers as the sheet showed them written (an administrator's yes): a run on the one path, set aside where the
/// files changed since (its stamp).
pub async fn apply_plan(app: &std::sync::Arc<super::App>, account: &str, album: &str, stamps: Option<std::collections::HashMap<String, String>>) -> Result<super::file_runs::Status> {
    let kind = format!("{KIND}:{album}");
    anyhow::ensure!(!super::file_runs::busy(&kind), "Already writing");
    let preview = looked(account, album)?.context("The release is still being looked up")?;
    let a = album.to_string();
    let p = tokio::task::spawn_blocking(move || -> Result<super::file_runs::Proposal> { Ok(super::file_runs::ticked(proposal(&a, &preview)?, &[a.clone()], stamps.as_ref())) }).await??;
    anyhow::ensure!(!p.files.is_empty() || !p.failed.is_empty(), "Its numbers are as the release has them already");
    Ok(super::file_runs::start(app, p))
}

/// What is off in an album's discs (0.13, for the owner's review with an assistant: albums that lost their disc
/// numbers), none when nothing is: its track numbers start again at 1 on one disc (1–10, then 1–9: two discs' tracks
/// without their disc numbers), or the release that holds it has its tracks on more discs (`released`, `incomplete`)
/// than its files say. `numbers`: each track's (disc, track).
pub fn discs_off(numbers: &[(Option<u32>, Option<u32>)], released: Option<u32>) -> Option<String> {
    let on: std::collections::BTreeSet<u32> = numbers.iter().map(|(d, _)| d.unwrap_or(1).max(1)).collect();
    let local = on.len().max(1) as u32;
    let mut seen = std::collections::BTreeMap::<u32, u32>::new();
    for n in numbers.iter().filter_map(|(_, t)| *t).filter(|n| *n > 0) {
        *seen.entry(n).or_default() += 1;
    }
    // Every number from 1 up to at least 2 there twice or more: the numbers start again.
    let again = (1..).take_while(|n| seen.get(n).is_some_and(|c| *c > 1)).count() >= 2;
    let restarts = local == 1 && again;
    let more = released.filter(|r| *r > local);
    match (restarts, more) {
        (true, Some(r)) => Some(format!("Numbers start again at 1; MusicBrainz has it on {r} discs")),
        (true, None) => Some("Numbers start again at 1 without disc numbers".into()),
        (false, Some(r)) => Some(format!("On {r} discs at MusicBrainz; the files say {local}")),
        (false, None) => None,
    }
}

/// What is off in an album's numbering, per disc (none when nothing is): numbers twice, numbers missing below the
/// highest (`gaps`: not for an album a renumber look matched to a release, whose gaps are tracks not held; the owner,
/// 2026-10-26), tracks without one. `numbers`: each track's (disc, track).
pub fn off(numbers: &[(Option<u32>, Option<u32>)], gaps: bool) -> Option<String> {
    let mut discs: std::collections::BTreeMap<u32, Vec<Option<u32>>> = std::collections::BTreeMap::new();
    for (d, t) in numbers {
        discs.entry(d.unwrap_or(1).max(1)).or_default().push(*t);
    }
    let several = discs.len() > 1;
    let mut parts = Vec::new();
    for (disc, tracks) in &discs {
        let mut said = Vec::new();
        let mut seen = std::collections::BTreeMap::<u32, u32>::new();
        for n in tracks.iter().flatten().filter(|n| **n > 0) {
            *seen.entry(*n).or_default() += 1;
        }
        let twice: Vec<String> = seen.iter().filter(|(_, c)| **c > 1).map(|(n, _)| n.to_string()).collect();
        if !twice.is_empty() {
            said.push(format!("track {} twice", twice.join(", ")));
        }
        let top = seen.keys().max().copied().unwrap_or(0);
        let missing: Vec<String> = (1..top).filter(|n| !seen.contains_key(n)).map(|n| n.to_string()).collect();
        if gaps && !missing.is_empty() {
            said.push(format!("{} missing", if missing.len() > 6 { format!("{} numbers", missing.len()) } else { missing.join(", ") }));
        }
        let unnumbered = tracks.iter().filter(|n| n.is_none_or(|n| n == 0)).count();
        if unnumbered > 0 {
            said.push(format!("{unnumbered} unnumbered"));
        }
        if !said.is_empty() {
            let text = said.join("; ");
            parts.push(if several { format!("Disc {disc}: {text}") } else { text });
        }
    }
    let line = parts.join(" · ");
    let mut c = line.chars();
    (!line.is_empty()).then(|| c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_album_that_lost_its_disc_numbers_is_told() {
        let one = |n: u32| (None, Some(n));
        let restarted: Vec<_> = (1..=10).chain(1..=9).map(one).collect();
        assert_eq!(discs_off(&restarted, None).as_deref(), Some("Numbers start again at 1 without disc numbers"));
        assert_eq!(discs_off(&restarted, Some(2)).as_deref(), Some("Numbers start again at 1; MusicBrainz has it on 2 discs"));
        let whole: Vec<_> = (1..=12).map(one).collect();
        assert_eq!(discs_off(&whole, Some(2)).as_deref(), Some("On 2 discs at MusicBrainz; the files say 1"));
        assert_eq!(discs_off(&whole, Some(1)), None);
        let numbered: Vec<_> = (1..=10).map(|n| (Some(1), Some(n))).chain((1..=9).map(|n| (Some(2), Some(n)))).collect();
        assert_eq!(discs_off(&numbered, Some(2)), None, "its disc numbers kept");
        let twice = [one(1), one(2), one(3), one(3)];
        assert_eq!(discs_off(&twice, None), None, "one number twice: Track numbers, not discs");
    }

    fn track(id: &str, title: &str, secs: u32) -> Track {
        Track { id: id.into(), path: PathBuf::from(format!("{id}.flac")), file: format!("{id}.flac"), title: title.into(), secs, disc: None, track: None }
    }

    fn release(format: &str, media: &[&[(&str, u32)]]) -> Release {
        Release {
            id: String::new(),
            facts: ReleaseFacts { title: "R".into(), format: format.into(), country: None, year: None },
            media: media.iter().map(|m| Medium { format: format.into(), tracks: m.iter().map(|(t, s)| (t.to_string(), Some(*s))).collect() }).collect(),
        }
    }

    #[test]
    fn what_is_off_is_said_per_disc() {
        assert_eq!(off(&[(None, Some(1)), (None, Some(2)), (None, Some(3))], true), None);
        assert_eq!(off(&[(None, Some(1)), (None, Some(3)), (None, Some(3)), (None, None)], true).as_deref(), Some("Track 3 twice; 2 missing; 1 unnumbered"));
        assert_eq!(off(&[(Some(1), Some(1)), (Some(2), Some(1)), (Some(2), Some(1))], true).as_deref(), Some("Disc 2: track 1 twice"));
        assert_eq!(off(&[(None, Some(1)), (None, Some(4))], false), None, "matched to a release: its gaps are tracks not held");
        assert_eq!(off(&[(None, Some(1)), (None, Some(4)), (None, None)], false).as_deref(), Some("1 unnumbered"));
    }

    #[test]
    fn health_s_discs_are_the_ones_renumber_would_number_on() {
        let tracks = [track("a", "One", 200), track("b", "Two", 181), track("c", "Three", 240), track("d", "Four", 100)];
        let sides: &[&[(&str, u32)]] = &[&[("One", 201), ("Two", 180)], &[("Three", 240), ("Four", 99)]];
        assert_eq!(discs_from(&tracks, &[release("CD", sides)]), Some(2), "a CD keeps its discs");
        assert_eq!(discs_from(&tracks, &[release("12\" Vinyl", sides)]), Some(1), "a double LP stays one disc");
        assert_eq!(discs_from(&tracks[..2], &[release("CD", sides)]), Some(1), "its tracks on one of them");
        assert_eq!(discs_from(&tracks, &[]), None, "no release holds it");
    }

    #[test]
    fn a_record_is_put_on_one_disc_and_a_cd_keeps_its_discs() {
        let tracks = [track("a", "One", 200), track("b", "Two (Remastered)", 181), track("c", "Three", 240), track("d", "Four", 100)];
        let vinyl = release("12\" Vinyl", &[&[("One", 201), ("Two", 180)], &[("Three", 240), ("Four", 99)]]);
        let (r, places) = choose(&tracks, std::slice::from_ref(&vinyl)).unwrap();
        let p = from_release(&tracks, r, &places, &[], "musicbrainz");
        assert_eq!((p.discs, p.why.as_str()), ("flattened", "Vinyl"));
        assert_eq!(p.tracks.iter().map(|r| (r.to.disc, r.to.track)).collect::<Vec<_>>(), [(1, 1), (1, 2), (1, 3), (1, 4)]);
        let cd = release("CD", &[&[("One", 200), ("Two", 181)], &[("Three", 240), ("Four", 100)]]);
        let (r, places) = choose(&tracks, std::slice::from_ref(&cd)).unwrap();
        let p = from_release(&tracks, r, &places, &[], "musicbrainz");
        assert_eq!(p.tracks.iter().map(|r| (r.to.disc, r.to.track)).collect::<Vec<_>>(), [(1, 1), (1, 2), (2, 1), (2, 2)]);
        assert_eq!(p.totals, Totals { discs: 2, tracks_per_disc: vec![2, 2] });
    }

    #[test]
    fn a_digital_release_is_preferred_and_keeps_its_discs_when_cd_sized() {
        let tracks = [track("a", "One", 200), track("b", "Two", 181)];
        let digital = release("Digital Media", &[&[("One", 200)], &[("Two", 181)]]);
        let cd = release("CD", &[&[("One", 200)], &[("Two", 181)]]);
        let all = [cd.clone(), digital.clone()];
        let (r, places) = choose(&tracks, &all).unwrap();
        assert_eq!(r.kind(), Kind::Digital, "digital first");
        let p = from_release(&tracks, r, &places, &all, "musicbrainz");
        assert_eq!((p.discs, p.why.as_str()), ("kept", "Digital, CD-sized"), "a CD of the group has as many discs");
        let p = from_release(&tracks, &digital, &places, &[], "musicbrainz");
        assert_eq!(p.discs, "flattened", "short and no CD like it");
    }

    #[test]
    fn a_track_not_on_the_release_leaves_the_files_order() {
        let tracks = [track("b", "Two", 181), track("a", "One", 200), track("x", "Bonus", 50)];
        assert!(choose(&tracks, &[release("CD", &[&[("One", 200), ("Two", 181)]])]).is_none());
        let p = from_files(&tracks);
        assert_eq!(p.tracks.iter().map(|r| (r.id.as_str(), r.to.track)).collect::<Vec<_>>(), [("a", 1), ("b", 2), ("x", 3)]);
    }

    #[test]
    fn discogs_positions_make_its_media() {
        let v = serde_json::json!({ "title": "R", "formats": [{ "name": "Vinyl" }], "tracklist": [
            { "position": "A1", "title": "One", "duration": "3:20", "type_": "track" },
            { "position": "", "title": "Side B", "type_": "heading" },
            { "position": "B1", "title": "Two", "duration": "1:00:05", "type_": "track" }] });
        let r = discogs_release(&v);
        assert_eq!(r.media.len(), 2);
        assert_eq!(r.media[1].tracks[0], ("Two".to_string(), Some(3605)));
        assert_eq!(medium_of("2-5"), "2");
    }

    #[test]
    fn a_look_s_preview_is_each_track_s_numbers() {
        let preview = serde_json::json!({
            "from": "musicbrainz", "release": { "title": "A", "format": "CD", "year": 1998 },
            "tracks": [{ "id": "s1", "to": { "disc": 1, "track": 1 } }, { "id": "s2", "to": { "disc": 2, "track": 1 } }],
            "totals": { "discs": 2, "tracks_per_disc": [1, 1] }
        });
        let (tracks, title) = planned(&preview);
        assert_eq!(title, "Renumber · the 1998 CD release");
        let n = |v: &[(&str, u32)]| v.iter().map(|(k, x)| (k.to_string(), *x)).collect::<Vec<_>>();
        assert_eq!(tracks[1], ("s2".to_string(), n(&[("tracknumber", 1), ("totaltracks", 1), ("discnumber", 2), ("totaldiscs", 2)])));
    }
}
