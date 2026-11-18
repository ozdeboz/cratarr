//! Replace (decision 0013): an upgrade in Incoming takes the place of its album in a main library, the one that album
//! is in (0101 point 2). Each old track is paired with a new file, which takes over its track id (playlists and play
//! data follow); the old files go to that library's own trash. Refused when an old track has no match. Write-ahead
//! like Keep and Delete; Undo puts it all back.
//!
//! Replace from someone's own music (RY01; OQ2, the owner: "Copy in, admins") is the same Replace with the new copy copied, not
//! moved: the own copy stays as it was, and the main library's copy is made anew on the same tracks (`replacing_from_copy`).

use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;

use super::{EventId, IncomingService, KeepResult, Located, PLAYDATA_PLAN, PlayData, files_under, queue};
use crate::app::ToggleError;
use crate::app::library::epoch_now;
use crate::incoming::{AlbumFile, ReplacePlan, ReplaceRefusal};
use crate::journal::{Entry, Event, TrackMove};

/// What Replace would do with an album, for the page to show before asking.
#[derive(Debug, Serialize)]
pub struct ReplacePreview {
    pub title: String,
    /// The album it replaces (its folder, in its main library).
    pub old: String,
    /// Where the new album goes in that library, when it can go ahead.
    pub folder: Option<String>,
    /// Old tracks the new files take over.
    pub tracks: usize,
    /// New tracks the old album does not have.
    pub added: usize,
    pub refused: Option<ReplaceRefused>,
    /// Lossless files above that library's quality limit, converted first (Replace then waits in the queue).
    pub convert: usize,
    /// Whether a Lidarr is set, to be told to stop fetching the album; without one it may fetch it again.
    pub acquisition: bool,
    /// Both covers, side by side for the choice (0.9.5): the old album's and the new copy's.
    pub covers: ReplaceCovers,
    /// How the new copy's tags differ from the old album's, the differences only (0.9.7, the owner): album-wide
    /// once (`track` none), else by track.
    pub diff: Vec<TagDiff>,
    /// What the new copy lacks that the album had ("Year", "MusicBrainz ids", "4 fewer tags").
    pub lost: Vec<String>,
    /// What "Replace and carry over" brings into the new copy, and what follows the album by itself.
    pub carry: Vec<String>,
    /// How many files it writes: every file of the new copy, music and others (RY01's sheet says it, as Copy to's does).
    pub files: usize,
    /// The new copy's library: someone's own music for a Replace from it (RY01), whose copy it is; else its Incoming library.
    pub from: Option<String>,
    /// The own copy's folder in its library, as this preview read it (RY01; RF10, the review of runs 11 and 12, RA9): its tap carries it
    /// back, so it takes the copy its sheet showed, of two in one library. None for Replace from Incoming.
    pub from_dir: Option<String>,
}

impl ReplacePreview {
    /// A Replace from someone's own copy refused before its plan (RY01): that copy is not the one offered, and why.
    pub(crate) fn refused_from_copy(title: String, old: String, from: String, why: String) -> Self {
        ReplacePreview {
            title,
            old,
            folder: None,
            tracks: 0,
            added: 0,
            refused: Some(ReplaceRefused::Other(why)),
            convert: 0,
            acquisition: false,
            covers: ReplaceCovers::default(),
            diff: Vec::new(),
            lost: Vec::new(),
            carry: Vec::new(),
            files: 0,
            from: Some(from),
            from_dir: None,
        }
    }
}

/// What the sheet of a Replace from someone's own copy showed (RY01, OQ-G), carried by its tap: the plan made at the tap must show
/// the same, or it is refused as changed since.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceSheet {
    pub folder: String,
    pub tracks: usize,
    pub added: usize,
    pub files: usize,
}

impl ReplaceSheet {
    /// What a plan's sheet shows.
    fn of(plan: &ReplacePlan) -> Self {
        ReplaceSheet { folder: plan.folder.clone(), tracks: plan.takes_over.len(), added: plan.added, files: plan.moves.len() }
    }
}

/// The files a Replace from someone's own copy changed, for the servers and the player (`back`: its Undo's), as (where it was,
/// where it is): the old files into the trash, and each copy as come in the place of the old file on its track (the first old
/// file's, for a track the old copy did not have). The own copy's folder never: it stays as it was.
pub(super) fn moved_pairs(event: &Event, back: bool) -> Vec<((i64, String), (i64, String))> {
    let Event::AlbumReplacedFromCopy { copies, replaced, .. } = event else { return Vec::new() };
    let Some(first) = replaced.first() else { return Vec::new() };
    let came = copies.iter().map(|m| (replaced.iter().find(|r| r.track.is_some() && r.track == m.track).unwrap_or(first).from.clone(), m.to.clone()));
    replaced.iter().map(|r| (r.from.clone(), r.to.clone())).chain(came).map(|(a, b)| if back { (b, a) } else { (a, b) }).collect()
}

/// What a Replace from someone's own copy's player follows (`playing_among`): its tracks in order, each one's id before and after
/// (a copy is on its old file's track), and the old files leaving.
#[allow(clippy::type_complexity)]
fn playing_of_copy(event: &Event) -> (Vec<i64>, Vec<(i64, i64)>, Vec<(i64, String)>) {
    let Event::AlbumReplacedFromCopy { copies, replaced, .. } = event else { return Default::default() };
    let order: Vec<i64> = copies.iter().filter_map(|m| m.track).collect();
    let before = order.iter().map(|t| (*t, *t)).collect();
    (order, before, replaced.iter().map(|r| r.from.clone()).collect())
}

/// The catalogue rows after a Replace from someone's own copy's files (`finish_from_copy`), safe to repeat: the main library's old
/// rows into its trash, marked trashed with the token, letting go of their tracks (as Replace's old files do); the copies' rows made
/// from the own copy's (`Catalogue::copy_rows`: on the same tracks, read and analysed), each shared as its copy is (`shared`, in
/// the copies' order). The own copy's rows as they were (OQ-E). Blocking.
pub(crate) fn rows_of_copy(c: &crate::catalogue::Catalogue, event: &Event, shared: &[bool]) -> Result<()> {
    let Event::AlbumReplacedFromCopy { copies, replaced, token, .. } = event else { return Ok(()) };
    for r in replaced {
        if !c.has_row(r.to.0, &r.to.1) {
            c.move_into_trash((r.from.0, &r.from.1), (r.to.0, &r.to.1), token)?;
        }
    }
    c.set_tracks(&replaced.iter().map(|r| (r.to.clone(), None)).collect::<Vec<_>>())?;
    let rows: Vec<((i64, String), (i64, String), bool)> = copies.iter().enumerate().map(|(i, m)| (m.from.clone(), m.to.clone(), shared.get(i).copied().unwrap_or(false))).collect();
    c.copy_rows(&rows)?;
    Ok(())
}

/// The catalogue rows after a Replace from someone's own copy's Undo (`undo_from_copy`), safe to repeat: each copy it took back
/// (`taken`: its path and its trash token) marked trashed with its token, letting go of its track; the main library's old rows back
/// from the trash at their places, on their tracks again. Blocking.
pub(crate) fn rows_back_of_copy(c: &crate::catalogue::Catalogue, event: &Event, taken: &[(String, String)]) -> Result<()> {
    let Event::AlbumReplacedFromCopy { replaced, to_dir, .. } = event else { return Ok(()) };
    let folder = to_dir.0;
    let trashed = |token: &str| format!("{}/{token}", crate::trash::TRASH_DIR);
    for (rel, token) in taken {
        if !c.has_row(folder, &trashed(token)) && c.has_row(folder, rel) {
            c.mark_trashed(folder, rel, token)?;
        }
    }
    c.set_tracks(&taken.iter().map(|(_, token)| ((folder, trashed(token)), None)).collect::<Vec<_>>())?;
    for r in replaced {
        if !c.has_row(r.from.0, &r.from.1) {
            c.move_files(r.to.0, &r.to.1, r.from.0, &r.from.1)?;
        }
    }
    c.set_tracks(&replaced.iter().map(|r| (r.from.clone(), r.track)).collect::<Vec<_>>())?;
    c.changed_outside_pass();
    Ok(())
}

/// A Replace from someone's own copy going on (`IncomingService::alone`): its record and main library, given back when it ends.
pub(super) struct Alone {
    key: (String, i64),
    held: super::Replacing,
}

impl Drop for Alone {
    fn drop(&mut self) {
        self.held.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.key);
    }
}

/// A tag the new copy has otherwise (0.9.7).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagDiff {
    /// The new file's name; none where every track differs alike (the album's year, genre, artist).
    pub track: Option<String>,
    pub field: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

/// The tags Replace shows the differences of.
const COMPARED: [&str; 13] = [
    "title",
    "artist",
    "album",
    "albumartist",
    "date",
    "originaldate",
    "genre",
    "tracknumber",
    "discnumber",
    "musicbrainz_albumid",
    "musicbrainz_releasegroupid",
    "musicbrainz_trackid",
    "musicbrainz_albumartistid",
];
/// What Cratarr knows of the album that carries over into the new copy: its MusicBrainz ids, and its numbers (as
/// Renumber may have set them).
const CARRIED: [&str; 7] =
    ["musicbrainz_albumid", "musicbrainz_releasegroupid", "musicbrainz_albumartistid", "musicbrainz_trackid", "musicbrainz_artistid", "tracknumber", "discnumber"];

type Named = std::collections::BTreeMap<String, String>;

/// A file's named tags, the first value of each.
fn named(path: &std::path::Path) -> Named {
    let Ok((tags, _)) = crate::catalogue::tags::read(path) else { return Named::default() };
    let mut out = Named::new();
    for (_, name, value) in tags.items {
        if let Some(n) = name {
            out.entry(n.to_string()).or_insert(value);
        }
    }
    out
}

/// The differences of each pair (the old file in its main library, the new one in Incoming), what the new copy loses,
/// and what carrying over brings. Blocking: it reads every file's tags.
fn compare(pairs: &[(PathBuf, PathBuf)], keep_cover: bool) -> (Vec<TagDiff>, Vec<String>, Vec<String>) {
    let read: Vec<(String, Named, Named)> =
        pairs.iter().map(|(o, n)| (n.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(), named(o), named(n))).collect();
    let mut diff = Vec::new();
    for field in COMPARED {
        let differs: Vec<(&String, Option<&String>, Option<&String>)> =
            read.iter().map(|(f, o, n)| (f, o.get(field), n.get(field))).filter(|(_, o, n)| o.map(|v| v.trim()) != n.map(|v| v.trim())).collect();
        if differs.is_empty() {
            continue;
        }
        // Every track the same change: once, for the album.
        if differs.len() == read.len() && differs.iter().all(|d| (d.1, d.2) == (differs[0].1, differs[0].2)) {
            diff.push(TagDiff { track: None, field: field.into(), old: differs[0].1.cloned(), new: differs[0].2.cloned() });
        } else {
            diff.extend(differs.into_iter().map(|(f, o, n)| TagDiff { track: Some(f.clone()), field: field.into(), old: o.cloned(), new: n.cloned() }));
        }
    }
    let lacks = |field: &str| read.iter().any(|(_, o, n)| o.contains_key(field) && !n.contains_key(field));
    let mut lost = Vec::new();
    let said: [(&[&str], &str); 5] = [
        (&["date", "originaldate"], "Year"),
        (&["genre"], "Genre"),
        (&["albumartist"], "Album artist"),
        (&["musicbrainz_albumid", "musicbrainz_trackid", "musicbrainz_releasegroupid"], "MusicBrainz ids"),
        (&["tracknumber"], "Track numbers"),
    ];
    for (fields, name) in said {
        if fields.iter().any(|f| lacks(f)) {
            lost.push(name.to_string());
        }
    }
    let (had, has) = read.iter().fold((0, 0), |(a, b), (_, o, n)| (a + o.len(), b + n.len()));
    if has < had {
        lost.push(format!("{} fewer tags", had - has));
    }
    let differs_in = |fields: &[&str]| read.iter().any(|(_, o, n)| fields.iter().any(|f| o.contains_key(*f) && o.get(*f) != n.get(*f)));
    let mut carry = Vec::new();
    if differs_in(&CARRIED[..5]) {
        carry.push("MusicBrainz ids".to_string());
    }
    if differs_in(&CARRIED[5..]) {
        carry.push("Track numbers".to_string());
    }
    if keep_cover {
        carry.push("Cover".to_string());
    }
    // These follow the album by its id, whatever the files say.
    carry.extend(["Release type", "Album tags"].map(String::from));
    (diff, lost, carry)
}

/// Carries what Cratarr knows of the album into the new copy before Replace (0.9.7, "Replace and carry over"): each new
/// file gets its old one's MusicBrainz ids and numbers where they differ. The new copy is changed where it lies, so an
/// Undo of Replace brings it back to Incoming as it was carried. Never stops Replace: what could not be written is
/// logged.
fn carry_over(pairs: &[(PathBuf, PathBuf)]) {
    for (old, new) in pairs {
        let (o, n) = (named(old), named(new));
        let set: Vec<(&'static str, Option<String>)> =
            CARRIED.iter().filter(|f| o.contains_key(**f) && o.get(**f) != n.get(**f)).map(|f| (*f, o.get(*f).cloned())).collect();
        if set.is_empty() {
            continue;
        }
        if let Err(e) = crate::app::fixes::write(new, &set) {
            tracing::warn!("replace: {} not carried over: {e:#}", new.display());
        }
    }
}

/// Each old track in its main library (at `music`) with the new file taking it over, as paths.
fn pairs_of(music: &std::path::Path, old_dir: &str, at: &Located, plan: &ReplacePlan) -> Vec<(PathBuf, PathBuf)> {
    plan.takes_over.iter().map(|(o, n)| (music.join(old_dir).join(o), at.root.join(&at.dir.dir).join(n))).collect()
}


/// The two covers Replace chooses between (0.9.5): `keep` the old album's, or take the `new` copy's.
#[derive(Debug, Default, Serialize)]
pub struct ReplaceCovers {
    pub current: Option<String>,
    pub new: Option<String>,
}

/// An album's cover as the page asks for it, of its copy in `library`: two copies of a record have one id, and the address
/// alone can tell their pictures apart.
fn cover_at(source_id: &str, album_id: &str, library: Option<&str>) -> String {
    let named = library.map_or_else(String::new, |l| format!("?from={}", crate::api::form_urlencoded_segment(l)));
    format!("/api/albums/{source_id}/{album_id}/cover{named}")
}

/// The cover of the album at `old_dir` in the main library in catalogue folder `main` (`old`, its library's id), and of
/// the new copy (`new`, its library's id), as the page shows covers.
fn covers_of(source_id: &str, album_id: &str, new: Option<&str>, main: i64, old: Option<&str>, old_dir: &str) -> ReplaceCovers {
    let current = crate::serve::instance().and_then(|s| s.library.album_in(main, old_dir).ok().flatten()).map(|id| cover_at(crate::source::cratarr::KIND.id, &id, old));
    ReplaceCovers { current, new: Some(cover_at(source_id, album_id, new)) }
}

/// The old album's cover written into the new copy's folder before Replace (0.9.5, `cover: keep`): its cover file
/// (else the picture inside its first track) beside the new tracks and inside each, the copy's own cover file set aside
/// in the data folder. Never stops Replace: what could not be done is logged.
fn keep_cover(old: &std::path::Path, new: &std::path::Path) {
    let picture = match crate::serve::art::cover_file(old) {
        Some(f) => std::fs::read(&f).ok(),
        None => crate::covers_inside::tracks_of(old).iter().find_map(|t| crate::serve::art::embedded(t)).map(|(_, b)| b),
    };
    let Some(data) = picture else {
        tracing::info!("covers: {} has no cover to keep", old.display());
        return;
    };
    let mime = crate::serve::art::sniff(&data);
    let ext = match mime {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "jpg",
    };
    if let Some(own) = crate::serve::art::cover_file(new) {
        let aside = crate::serve::instance().and_then(|s| s.data_dir.clone()).map(|d| std::path::Path::new(&d).join("cover-undo").join(format!("replace-{}", epoch_now())));
        if let Some(aside) = aside
            && std::fs::create_dir_all(&aside).is_ok()
            && let Some(name) = own.file_name()
        {
            let _ = std::fs::copy(&own, aside.join(name));
        }
        if let Err(e) = std::fs::remove_file(&own) {
            tracing::warn!("covers: {}: {e}", own.display());
            return;
        }
    }
    if let Err(e) = crate::covers_inside::write_file(&new.join(format!("cover.{ext}")), &data) {
        tracing::warn!("covers: {}: {e:#}", new.display());
        return;
    }
    for (t, why) in crate::covers_inside::replace(&crate::covers_inside::tracks_of(new), Some((&data, mime))) {
        tracing::warn!("covers: {} kept its own picture: {why}", t.display());
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum ReplaceRefused {
    /// The page asks for the year and tries again.
    NeedsYear,
    /// Old tracks without a match, by title.
    Unmatched(Vec<String>),
    /// Another reason, said as is.
    Other(String),
}

/// Replace worked out: the album it replaces, in a main library, and what moves.
struct Planned {
    /// The catalogue folder of the main library the old album is in: the new copy goes there (0101 point 2).
    main: i64,
    /// That folder on disk.
    root: PathBuf,
    /// The old album's folder, relative to it.
    old_dir: String,
    /// Every file of the old album (music and others), relative to its folder.
    old_files: Vec<String>,
    plan: ReplacePlan,
}

/// The music files of an album folder with their tags and lengths, paths relative to the album folder. Blocking.
fn album_files(c: &crate::catalogue::Catalogue, folder: i64, dir: &str) -> Result<Vec<AlbumFile>> {
    let prefix = format!("{dir}/");
    let lengths = c.album_lengths(folder, dir)?;
    Ok(c.album_tags(folder, dir)?
        .into_iter()
        .map(|(rel, tags)| AlbumFile {
            track: c.track_of(folder, &rel),
            duration_ms: lengths.get(&rel).copied(),
            rel: rel.strip_prefix(&prefix).unwrap_or(&rel).to_string(),
            tags: crate::naming::Tags(tags),
        })
        .collect())
}

/// The catalogue rows follow Replace's moves (or their Undo, `back`): the old files' rows into the trash and the new
/// files' into the main library, then the new files hold the old tracks' ids and the old files let go (back: each its
/// own again).
/// In the files' order, a row moving only while its place is free: safe to repeat, also when a new file has an old
/// one's name. Blocking.
fn rows_follow(c: &crate::catalogue::Catalogue, moves: &[TrackMove], replaced: &[TrackMove], token: &str, back: bool) -> Result<()> {
    let takes_over = |m: &TrackMove| m.track.is_some() && replaced.iter().any(|r| r.track == m.track);
    let mut tracks: Vec<((i64, String), Option<i64>)> = Vec::new();
    let free = |(folder, rel): &(i64, String)| !c.has_row(*folder, rel);
    if back {
        for m in moves {
            if free(&m.from) {
                c.move_files(m.to.0, &m.to.1, m.from.0, &m.from.1)?;
            }
            tracks.push((m.from.clone(), if takes_over(m) { m.was } else { m.track }));
        }
        for r in replaced {
            if free(&r.from) {
                c.move_files(r.to.0, &r.to.1, r.from.0, &r.from.1)?;
            }
            tracks.push((r.from.clone(), r.track));
        }
    } else {
        for r in replaced {
            if free(&r.to) {
                c.move_into_trash((r.from.0, &r.from.1), (r.to.0, &r.to.1), token)?;
            }
            tracks.push((r.to.clone(), None));
        }
        for m in moves {
            if free(&m.to) {
                c.move_files(m.from.0, &m.from.1, m.to.0, &m.to.1)?;
            }
            tracks.push((m.to.clone(), m.track));
        }
    }
    c.set_tracks(&tracks)
}

fn refused(r: ReplaceRefused) -> ToggleError {
    ToggleError::Refused(match r {
        ReplaceRefused::NeedsYear => "the album's year is not known".into(),
        ReplaceRefused::Unmatched(titles) => format!("no match for {}", titles.join(", ")),
        ReplaceRefused::Other(why) => why,
    })
}

impl IncomingService {
    /// The main library `to` names for Replace, as its catalogue folder (0101 point 2: any main library); none named, none.
    fn replaced_in(&self, source_id: &str, to: Option<&str>) -> Result<Option<i64>, ToggleError> {
        let Some(to) = to else { return Ok(None) };
        let (folder, _) = self.target_of(source_id, to)?;
        if !self.lib.main_folders().contains(&folder) {
            return Err(ToggleError::Refused("Replace takes the place of a copy in a main library".into()));
        }
        Ok(Some(folder))
    }

    /// The copy Replace takes the place of: the record's own, in a main library (C37a: two copies of one record, never a match by
    /// name), as its catalogue folder and its folder there. Named (`into`, the library the page chose or the job was planned for),
    /// the record's copy there; else its copy in the first main library in the libraries' order. A record kept in no main library:
    /// another record's copy there that its tracks pair with one for one (RF2, `Serve::paired_editions`, as 0.16's Replace found an
    /// upgrade's album), whose tracks, plays, hearts and playlist places the new files take over.
    async fn replaced_copy(&self, at: &Located, into: Option<i64>) -> Result<(i64, String), ToggleError> {
        if let Some(folder) = into
            && let Some(dir) = super::copy_dir_in(&at.album.id, folder).await
        {
            return Ok((folder, dir));
        }
        let (mains, id, place) = (self.lib.main_folders(), at.album.id.clone(), (at.dir.folder, at.dir.dir.clone()));
        let found = match crate::serve::instance() {
            Some(served) => tokio::task::spawn_blocking(move || -> Result<Option<(i64, String)>> {
                let copies = served.library.copies_of(&id)?;
                let own = copies
                    .iter()
                    .filter(|c| mains.contains(&c.folder) && into.is_none_or(|i| i == c.folder))
                    .min_by(|a, b| (a.order, a.folder, &a.dir).cmp(&(b.order, b.folder, &b.dir)))
                    .map(|c| (c.folder, c.dir.clone()));
                if own.is_some() || copies.iter().any(|c| mains.contains(&c.folder)) {
                    return Ok(own);
                }
                let Some(waiting) = copies.into_iter().find(|c| (c.folder, c.dir.clone()) == place) else { return Ok(None) };
                let paired = served.paired_editions(&[(id.clone(), waiting)], &mains)?.remove(&id);
                Ok(paired.map(|(_, kept)| (kept.folder, kept.dir)).filter(|(folder, _)| into.is_none_or(|i| i == *folder)))
            })
            .await
            .ok()
            .and_then(Result::ok)
            .flatten(),
            None => None,
        };
        found.ok_or_else(|| ToggleError::Refused(format!("{} is not in a main library", at.album.title)))
    }

    /// Work out Replace for an album: the album it replaces and what moves, or why not. Changes nothing.
    async fn plan_replace(&self, at: &Located, into: Option<i64>, year: Option<&str>) -> Result<Result<Planned, ReplaceRefused>, ToggleError> {
        let replaced = self.replaced_copy(at, into).await?;
        self.plan_replace_of(at, replaced, year).await
    }

    /// Work out Replace of the copy at (`main`, `old_dir`), a main library's, by the copy at `at`. Changes nothing.
    async fn plan_replace_of(&self, at: &Located, (main, old_dir): (i64, String), year: Option<&str>) -> Result<Result<Planned, ReplaceRefused>, ToggleError> {
        let (music, name) = (self.root_of(main)?, self.library_name(&at.source, main));
        let (c, folder, dir, root, year, template) =
            (self.lib.catalogue(), at.dir.folder, at.dir.dir.clone(), at.root.clone(), year.map(str::to_string), self.lib.template());
        // The served library the guard below reads (RF11): the running server's, or a unit test's (`incoming::served`).
        let served = super::served();
        tokio::task::spawn_blocking(move || -> Result<Result<Planned, ReplaceRefused>> {
            // The old album's folder goes to the trash whole, so what else lies in it must not; at the library's root, the library
            // would go with it.
            if old_dir.is_empty() {
                return Ok(Err(ReplaceRefused::Other("Cratarr cannot see that album's folder".into())));
            }
            if let Some(served) = served.as_deref()
                && let Some(old) = served.library.album_in(main, &old_dir)?
                && !super::whole_copy_in(Some(served), &old, main, &music, &old_dir)?
            {
                return Ok(Err(ReplaceRefused::Other("Cratarr cannot see that album's folder".into())));
            }
            let new = album_files(&c, folder, &dir)?;
            let old = album_files(&c, main, &old_dir)?;
            let new_others: Vec<String> = files_under(&root.join(&dir)).into_iter().filter(|f| !new.iter().any(|m| &m.rel == f)).collect();
            let plan = match crate::incoming::plan_replace(&template, &old, &new, &new_others, year.as_deref()) {
                Ok(p) => {
                    let keep = crate::incoming::KeepPlan { folder: p.folder, moves: p.moves };
                    let keep = crate::incoming::follow_spelling(keep, |parent| crate::incoming::folders_in(&music, parent));
                    crate::incoming::ReplacePlan { folder: keep.folder, moves: keep.moves, ..p }
                }
                Err(ReplaceRefusal::NeedsYear) => return Ok(Err(ReplaceRefused::NeedsYear)),
                Err(ReplaceRefusal::Unmatched(titles)) => return Ok(Err(ReplaceRefused::Unmatched(titles))),
                Err(e) => return Ok(Err(ReplaceRefused::Other(e.to_string()))),
            };
            if plan.folder != old_dir && music.join(&plan.folder).exists() {
                return Ok(Err(ReplaceRefused::Other(format!("{name} already has a folder named {}", plan.folder))));
            }
            // Ids the new files take over must exist: a track still being read has none yet.
            if plan.takes_over.iter().any(|(o, _)| c.track_of(main, &format!("{old_dir}/{o}")).is_none()) {
                return Ok(Err(ReplaceRefused::Other(format!("Cratarr is still reading the album in {name}; try again in a minute"))));
            }
            Ok(Ok(Planned { main, old_files: files_under(&music.join(&old_dir)), root: music, old_dir, plan }))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .map_err(ToggleError::from)
    }

    /// What Replace would do with an album, for the page to show before asking. Changes nothing.
    pub async fn replace_preview(&self, source_id: &str, album_id: &str, from: Option<&str>, to: Option<&str>, year: Option<&str>) -> Result<ReplacePreview, ToggleError> {
        let at = self.locate(source_id, album_id, from).await?;
        let into = self.replaced_in(source_id, to)?;
        let (main, old) = self.replaced_copy(&at, into).await?;
        // Converted above the target's quality limit, as Keep's (the owner, 2026-11-02: a library's format holds).
        let convert = self.to_convert(&at, self.quality_at(source_id, main)).await.len();
        let acquisition = self.acquisition().is_some();
        // Two copies of a record have one id: each picture is asked of its library. Another server's has its own id.
        let own = !at.album.copies.is_empty();
        let from_dir: Option<String> = None;
        let (new, kept) = (at.album.library.clone().filter(|_| own), self.library_at(source_id, main).filter(|_| own));
        let from = at.album.library.clone();
        Ok(match self.plan_replace(&at, into, year).await? {
            Ok(p) => {
                let pairs = pairs_of(&p.root, &p.old_dir, &at, &p.plan);
                let (diff, lost, carry) = tokio::task::spawn_blocking(move || compare(&pairs, true)).await.unwrap_or_default();
                ReplacePreview {
                    covers: covers_of(source_id, album_id, new.as_deref(), p.main, kept.as_deref(), &p.old_dir),
                    title: at.album.title,
                    old: p.old_dir,
                    folder: Some(p.plan.folder),
                    tracks: p.plan.takes_over.len(),
                    added: p.plan.added,
                    refused: None,
                    convert,
                    acquisition,
                    diff,
                    lost,
                    carry,
                    files: p.plan.moves.len(),
                    from,
                    from_dir,
                }
            }
            Err(r) => ReplacePreview {
                covers: covers_of(source_id, album_id, new.as_deref(), main, kept.as_deref(), &old),
                title: at.album.title,
                old,
                folder: None,
                tracks: 0,
                added: 0,
                refused: Some(r),
                convert,
                acquisition,
                diff: Vec::new(),
                lost: Vec::new(),
                carry: Vec::new(),
                files: 0,
                from,
                from_dir,
            },
        })
    }

    /// RY01's plan: Replace's (`plan_replace_of`) of the main library's copy at `main` by the own copy at `at`, every old track paired
    /// with a file on the very same track (two copies of one record: the copy keeps the old file's plays, hearts, ratings and lists,
    /// RY01.10), and every file of the own copy there to copy. Changes nothing.
    async fn plan_from_copy(&self, at: &Located, main: (i64, String), year: Option<&str>) -> Result<Result<Planned, ReplaceRefused>, ToggleError> {
        let planned = match self.plan_replace_of(at, main, year).await? {
            Ok(p) => p,
            Err(r) => return Ok(Err(r)),
        };
        let c = self.lib.catalogue();
        let name = |rel: &str| rel.rsplit('/').next().unwrap_or(rel).to_string();
        let other: Vec<String> = planned
            .plan
            .takes_over
            .iter()
            .filter(|(o, n)| c.track_of(planned.main, &format!("{}/{o}", planned.old_dir)) != c.track_of(at.dir.folder, &format!("{}/{n}", at.dir.dir)))
            .map(|(o, _)| name(o))
            .collect();
        if !other.is_empty() {
            return Ok(Err(ReplaceRefused::Unmatched(other)));
        }
        // Every file of the own copy there to copy, and none of them in Music already as it is (a tap a moment after another's).
        let (from, to, moves) = (at.root.join(&at.dir.dir), planned.root.clone(), planned.plan.moves.clone());
        let looked = tokio::task::spawn_blocking(move || -> Option<String> {
            if let Some((rel, _)) = moves.iter().find(|(rel, _)| !from.join(rel).is_file()) {
                return Some(format!("{} is no longer there", name(rel)));
            }
            let there = moves.iter().any(|(rel, target)| super::stamp_of(&to.join(target)).is_some_and(|s| super::stamp_of(&from.join(rel)) == Some(s)));
            there.then(|| "That copy is in its place already".to_string())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        if let Some(why) = looked {
            return Ok(Err(ReplaceRefused::Other(why)));
        }
        Ok(Ok(planned))
    }

    /// What Replace in a main library from someone's own copy would do (RY01), for the page to show before asking: Replace's sheet with
    /// no cover to choose and nothing carried over (neither copy is written to, OQ-A), nothing converted (it goes in as it is, OQ-B),
    /// Lidarr not told, how many files it writes, and whose copy it is (`from`). Changes nothing.
    pub async fn replace_from_copy_preview(&self, source_id: &str, album: crate::source::Album, main: (i64, String), year: Option<&str>) -> Result<ReplacePreview, ToggleError> {
        let at = self.locate_listed(source_id, album).await?;
        let (from, from_dir) = (at.album.library.clone(), Some(at.dir.dir.clone()));
        Ok(match self.plan_from_copy(&at, main.clone(), year).await? {
            Ok(p) => {
                let pairs = pairs_of(&p.root, &p.old_dir, &at, &p.plan);
                let (diff, lost, _) = tokio::task::spawn_blocking(move || compare(&pairs, false)).await.unwrap_or_default();
                ReplacePreview {
                    title: at.album.title,
                    old: p.old_dir,
                    folder: Some(p.plan.folder.clone()),
                    tracks: p.plan.takes_over.len(),
                    added: p.plan.added,
                    refused: None,
                    convert: 0,
                    acquisition: false,
                    covers: ReplaceCovers::default(),
                    diff,
                    lost,
                    carry: Vec::new(),
                    files: p.plan.moves.len(),
                    from,
                    from_dir,
                }
            }
            Err(r) => ReplacePreview {
                title: at.album.title,
                old: main.1,
                folder: None,
                tracks: 0,
                added: 0,
                refused: Some(r),
                convert: 0,
                acquisition: false,
                covers: ReplaceCovers::default(),
                diff: Vec::new(),
                lost: Vec::new(),
                carry: Vec::new(),
                files: 0,
                from,
                from_dir,
            },
        })
    }

    /// Replace in a main library from someone's own copy (RY01; OQ2, the owner: "Copy in, admins"), as its sheet showed it (`sheet`,
    /// OQ-G): the own copy listed alone (`album`) in the place of the main library's copy at `main`. Told on Health's list of changes
    /// as it goes, as Replace is. Administrators' alone, as its route is.
    pub async fn replace_from_copy(&self, source_id: &str, album: crate::source::Album, main: (i64, String), year: Option<&str>, sheet: &ReplaceSheet) -> Result<KeepResult, ToggleError> {
        let at = self.locate_listed(source_id, album).await?;
        let (what, title) = self.replaced_from_named(&at.source, at.dir.folder, main.0, &at.album.title);
        let run = crate::app::file_runs::track(&self.journal, what, title);
        let done = self.replacing_from_copy(&at, main.clone(), year, Some(sheet)).await;
        match &done {
            Ok(k) => run.moved(k.undo.clone(), self.read_in(&at, main.0)),
            Err(e) => run.failed(&at.album.id, e.to_string()),
        }
        done
    }

    /// One album action at a time on a record's copy in a library (RY01.13, RF15: two taps at once, one goes and the other is told
    /// what is going on): Replace from someone's own copy, Copy to, and their Undos, `doing` saying which ("being replaced"). Held until
    /// the action ends.
    pub(super) fn alone(&self, album: &str, folder: i64, title: &str, doing: &'static str) -> Result<Alone, ToggleError> {
        let key = (album.to_string(), folder);
        let mut going = self.replacing.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(held) = going.get(&key) {
            return Err(ToggleError::Refused(format!("{title} is {held}")));
        }
        going.insert(key.clone(), doing);
        Ok(Alone { key, held: self.replacing.clone() })
    }

    /// RY01's action: the own copy at `at` copied in the place of the main library's copy at `main` (the record's copy there). Refused
    /// while that copy is being converted, or replaced by another tap at once (RY01.13); its plan (`plan_from_copy`), met against the
    /// sheet its tap carries where it has one (OQ-G); the intent in the journal first; then Music's old files out and the copies in
    /// with their rows (`finish_from_copy`), its progress a task of its account's (the light rule for 0.17: `app::tasks`); then
    /// applied. Nothing is written into or beside either copy (OQ-A), nothing converted (OQ-B), Lidarr not told. A failure while
    /// copying leaves Music's old files back and forgets the intent; one after the copies stand keeps it, for the pass to finish (T4).
    pub(super) async fn replacing_from_copy(&self, at: &Located, main: (i64, String), year: Option<&str>, sheet: Option<&ReplaceSheet>) -> Result<KeepResult, ToggleError> {
        self.refuse_if_queued(&at.album.id, main.0)?;
        let _alone = self.alone(&at.album.id, main.0, &at.album.title, "being replaced")?;
        let now = epoch_now();
        let (event, planned, music) = self.event_from_copy(at, main, year, now).await?;
        if sheet.is_some_and(|s| *s != planned) {
            return Err(ToggleError::Refused(format!("Changed since you looked. {} would not be written as its sheet showed: look again.", at.album.title)));
        }
        if !self.can_write(&music).await {
            return Err(ToggleError::Refused("Cratarr cannot write in that library".into()));
        }
        let c = self.lib.catalogue();
        let Event::AlbumReplacedFromCopy { copies, replaced, to_dir, .. } = &event else { unreachable!() };
        let play = self.play_data_of_copy(to_dir.0, replaced, copies).await;
        let underway = self.intend(now, &event)?;
        let id = underway.id();
        self.journal.done(id, PLAYDATA_PLAN, now, &serde_json::to_value(&play).map_err(anyhow::Error::from)?)?;
        // Playing from the old copy: carried on with the new one (the own copy plays on as it was).
        let (order, before, leaving) = playing_of_copy(&event);
        let mut playing = self.playing_among(&order, &before, &leaving).await;
        if let Some(p) = playing.as_mut() {
            self.hold(p).await;
        }
        // Its progress where long work shows it, as its account's: its files to copy, then done. A second of the account's at once
        // goes on all the same, with no task of its own.
        let (files, doing) = (planned.files as u64, format!("Copying {}", at.album.title));
        let task = crate::serve::who::current().and_then(|w| crate::app::tasks::tasks().begin(Self::REPLACE_TASK, &w.user, false).ok());
        if let Some(t) = &task {
            t.step(&doing, 0, Some(files));
        }
        let done = self.finish_from_copy(id, &event, playing.is_some(), Some(underway.clone())).await;
        if let Some(t) = task {
            if done.is_ok() {
                t.step(&doing, files, Some(files));
            }
            t.finish(done.as_ref().map(|_| ()).map_err(|(_, e)| anyhow::anyhow!("{e:#}")));
        }
        match done {
            Ok(()) => {}
            // The copies stand (T4): the intent stays, for the pass to finish and the entry to have its Undo.
            Err((true, e)) => return Err(ToggleError::Refused(format!("{e:#}"))),
            Err((false, e)) => {
                self.journal.forget(id)?;
                return Err(ToggleError::Refused(format!("{e:#}")));
            }
        }
        self.journal.applied(id)?;
        drop(underway);
        *self.owned.lock().await = None;
        c.poke();
        // The backup's list of shared copies, anew (C41).
        crate::app::backup_list::refresh();
        if let Some(p) = playing {
            super::carry_on::carry_on(self.lib.clone(), p, &event);
        }
        Ok(KeepResult { undo: Some(id.to_string()), title: at.album.title.clone(), folder: planned.folder, queued: false })
    }

    /// RY01's event for the own copy at `at` in the place of the main library's copy at `main`, as planned now (`plan_from_copy`):
    /// Music's old files into its trash under the second `now`, the copies on the tracks they are on. With the plan's sheet, and
    /// that library's folder on disk. Changes nothing.
    pub(super) async fn event_from_copy(&self, at: &Located, main: (i64, String), year: Option<&str>, now: i64) -> Result<(Event, ReplaceSheet, PathBuf), ToggleError> {
        let Planned { main, root: music, old_dir, old_files, plan } = self.plan_from_copy(at, main, year).await?.map_err(refused)?;
        let c = self.lib.catalogue();
        // A trash entry of its own, so whatever lies under its token is this Replace's (an Undo's copies can lie under a second's).
        let mut now = now;
        while music.join(crate::trash::TRASH_DIR).join(crate::trash::token(now, &old_dir)).exists() {
            now += 1;
        }
        let token = crate::trash::token(now, &old_dir);
        let (own, dir) = (at.dir.folder, at.dir.dir.clone());
        let replaced: Vec<TrackMove> = old_files
            .iter()
            .map(|rel| {
                let from = format!("{old_dir}/{rel}");
                TrackMove { track: c.track_of(main, &from), from: (main, from), to: (main, format!("{}/{token}/{rel}", crate::trash::TRASH_DIR)), was: None }
            })
            .collect();
        let copies: Vec<TrackMove> = plan
            .moves
            .iter()
            .map(|(rel, to)| {
                let from = format!("{dir}/{rel}");
                TrackMove { track: c.track_of(own, &from), from: (own, from), to: (main, to.clone()), was: None }
            })
            .collect();
        // Each own file as it is now, at the intent: what a copy of it is told by from then on (RF6).
        let sources: Vec<PathBuf> = copies.iter().map(|m| at.root.join(&m.from.1)).collect();
        let stamps: Vec<Option<(u64, i64)>> =
            tokio::task::spawn_blocking(move || sources.iter().map(|p| super::stamp_of(p)).collect()).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        let event = Event::AlbumReplacedFromCopy {
            source: at.source.clone(),
            album_id: at.album.id.clone(),
            title: at.album.title.clone(),
            from_dir: (own, dir),
            to_dir: (main, plan.folder.clone()),
            copies,
            stamps,
            replaced,
            token,
        };
        Ok((event, ReplaceSheet::of(&plan), music))
    }

    /// What users did with the old copy's tracks on the other servers (RY01), read before anything moves (the server forgets them
    /// after), for the copies' new items there once they exist (the play-data handler). The own copy's items stay with it, and
    /// Cratarr's own item is the track, which stays.
    async fn play_data_of_copy(&self, main: i64, replaced: &[TrackMove], copies: &[TrackMove]) -> Vec<PlayData> {
        let c = self.lib.catalogue();
        let kept: Vec<&TrackMove> = replaced.iter().filter(|r| r.track.is_some() && copies.iter().any(|m| m.track == r.track)).collect();
        let mut play = Vec::new();
        for (source, s) in self.lib.connected_sources().into_iter().filter(|(source, _)| source != crate::source::cratarr::KIND.id) {
            let rels: Vec<String> = kept.iter().map(|r| r.from.1.clone()).collect();
            let items = c.track_ids(&source, main, &rels).unwrap_or_default();
            for (r, item) in kept.iter().zip(items) {
                let (Some(track), Some(item)) = (r.track, item) else { continue };
                let Some(to) = copies.iter().find(|m| m.track == Some(track)).map(|m| m.to.clone()) else { continue };
                match s.user_data(&item).await {
                    Ok(users) => play.extend(users.into_iter().map(|(user, data)| PlayData { source: source.clone(), track, old_item: item.clone(), to: to.clone(), user, data })),
                    Err(e) => tracing::warn!("{source}: could not read the play data of {item}: {e:#}"),
                }
            }
        }
        play
    }

    /// RY01's files and rows (`replacing_from_copy`, and the pass for one a stop cut short), safe to repeat: Music's old files into its
    /// trash, then every file of the own copy copied in (`Files::copy_all`: shared blocks where the disk can, byte for byte, the times
    /// kept), what the copies are kept on the entry (`COPIED`: each one's size and time as copied, for its Undo), then the rows
    /// (`rows_of_copy`). A copy that fails takes away every copy it or a try before it made, each told by its own file's size and time
    /// as the intent was written (RF6: whatever that file became since), and puts the old files back: all or none, and it says nothing
    /// stands written (false). Where the old files cannot be put back, an error is logged and it says written (true), so the intent
    /// stays for the next pass: never Music's album in its trash with no entry. A failure once the copies stand says so too (true).
    /// `held`: the action's mark as under way, kept through its files. The servers hear of the main library's folders alone.
    pub(super) async fn finish_from_copy(&self, id: EventId, event: &Event, hold_gone: bool, held: Option<super::Underway>) -> Result<(), (bool, anyhow::Error)> {
        let Event::AlbumReplacedFromCopy { replaced, title, stamps, .. } = event else { return Ok(()) };
        let (before, after) = (|e: anyhow::Error| (false, e), |e: anyhow::Error| (true, e));
        // A library out of use moves nothing: the intent stays for the pass, which forgets it where nothing went.
        let mut out = self.steps(event).ok_or_else(|| after(anyhow::anyhow!("the folders are not in use any more")))?;
        let ins = out.split_off(replaced.len().min(out.len()));
        let (files, stamps) = (self.files.clone(), stamps.clone());
        let copied = tokio::task::spawn_blocking(move || -> Result<(Vec<bool>, Vec<Option<(u64, i64)>>), (bool, anyhow::Error)> {
            let _held = held;
            // An old file in the trash under this token already went (a try cut short): a copy may lie at its name by now.
            let left: Vec<crate::moves::Step> = out.iter().filter(|s| !s.to.0.join(&s.to.1).exists()).cloned().collect();
            match files.move_all(&left).and_then(|()| files.copy_all(&ins)) {
                Ok(shared) => Ok((shared, ins.iter().map(|s| super::stamp_of(&s.to.0.join(&s.to.1))).collect())),
                Err(e) => {
                    for (s, was) in ins.iter().zip(&stamps) {
                        let to = s.to.0.join(&s.to.1);
                        if was.is_some() && super::stamp_of(&to) == *was {
                            std::fs::remove_file(&to).ok();
                        }
                    }
                    files.clear_parts(&ins);
                    match files.move_back(&out) {
                        Ok(()) => Err(before(e)),
                        Err(back) => Err(after(anyhow::anyhow!("{e:#}; the old files could not be put back: {back:#}"))),
                    }
                }
            }
        })
        .await
        .map_err(|e| before(anyhow::anyhow!("{e}")))?;
        let (shared, made) = match copied {
            Ok(done) => done,
            Err((true, e)) => {
                tracing::error!("replace: {title} not put back as it was, so it stays for the next pass: {e:#}");
                return Err((true, e));
            }
            Err(e) => return Err(e),
        };
        self.journal.done(id, super::COPIED, epoch_now(), &serde_json::json!(made)).map_err(after)?;
        let (c, written) = (self.lib.catalogue(), event.clone());
        tokio::task::spawn_blocking(move || rows_of_copy(&c, &written, &shared)).await.map_err(|e| after(anyhow::anyhow!("{e}")))?.map_err(after)?;
        super::tell_moved(&self.lib, title, &moved_pairs(event, false), if hold_gone { super::Told::Came } else { super::Told::Both }).await;
        Ok(())
    }

    /// Undo a Replace from someone's own copy (RY01; OQ-D, as T1c has it for Copy to): file by file, each copied file still as copied
    /// into the main library's trash under a second of its own (`trash::trash_file`, its token kept on the entry), then the old files
    /// back from the trash under their names, all or none, their rows on their tracks again; the copies' rows trashed, letting go of
    /// theirs; the folders it leaves empty go. Refused whole, before anything moves, where a copied file was rewritten since (naming
    /// it, and saying only that the library's old copy cannot come back: RF7, no advice, for none would work), where none of the
    /// copies is there as copied (a later album at its place), or where an old file cannot come back (gone from the trash, or its name
    /// taken since). Files with new names added since stay. Its files move as one plan, all or none (RF9): an Undo cut short is finished
    /// by the next tap or by the pass, what is in the trash under its second counting as done; one that cannot go on puts back what it
    /// took (`put_back_undo`), so Music never holds neither copy. One Undo of an entry at a time (`undoing`). The own copy is never
    /// touched. Once.
    pub(super) async fn undo_from_copy(&self, entry: &Entry) -> Result<String, ToggleError> {
        let gone = || ToggleError::Refused("that can no longer be undone".into());
        let Event::AlbumReplacedFromCopy { source, album_id, title, to_dir, copies, replaced, .. } = &entry.event else { return Err(gone()) };
        if entry.undone {
            return Err(ToggleError::Refused(format!("{title} is already back as it was")));
        }
        let _undoing = self.undoing(entry.id, title)?;
        let _alone = self.alone(album_id, to_dir.0, title, "being undone")?;
        let catalogue = self.lib.catalogue();
        // The copies held on their records while its files move, to its end (RF15, as Replace's Undo holds them).
        let moving = catalogue.copies_moving();
        let root = catalogue.folders().get(to_dir.0).cloned().ok_or_else(|| ToggleError::Refused("that library is not in use any more".into()))?;
        let made: Vec<Option<(u64, i64)>> = self.journal.result(entry.id, super::COPIED).ok().flatten().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        let begun = self.journal.result(entry.id, super::UNCOPIED).ok().flatten().and_then(|v| v["stamp"].as_i64());
        let plan: Vec<(String, Option<(u64, i64)>)> = copies.iter().enumerate().map(|(i, m)| (m.to.1.clone(), made.get(i).copied().flatten())).collect();
        let olds: Vec<(String, String)> = replaced.iter().map(|r| (r.from.1.clone(), r.to.1.clone())).collect();
        let name = |rel: &str| rel.rsplit('/').next().unwrap_or(rel).to_string();
        // How each copy lies, and whether every old file can come back, before anything moves.
        let (r, p, t, library) = (root.clone(), plan.clone(), title.clone(), self.library_name(source, to_dir.0));
        let looked = tokio::task::spawn_blocking(move || -> Result<Vec<super::Lies>, String> {
            use super::Lies;
            let trashed = |at: i64, rel: &str| r.join(crate::trash::TRASH_DIR).join(crate::trash::token(at, rel));
            let lies: Vec<Lies> = p
                .iter()
                .map(|(rel, was)| {
                    let here = r.join(rel);
                    if begun.is_some_and(|at| trashed(at, rel).is_file()) {
                        Lies::Done
                    } else if was.is_some() && super::stamp_of(&here) == *was {
                        Lies::AsCopied
                    } else if here.exists() {
                        Lies::Rewritten
                    } else {
                        Lies::Gone
                    }
                })
                .collect();
            if !lies.iter().any(|l| matches!(l, Lies::Done | Lies::AsCopied)) {
                return Err(format!("None of the files copied into the place of {t} is there as it was copied"));
            }
            if let Some(((rel, _), _)) = p.iter().zip(&lies).find(|(_, l)| **l == Lies::Rewritten) {
                // What is so, and no more (RF7): undoing that change writes the file anew, never as copied again.
                return Err(format!("{} has changed since {t} was copied in, so {library}'s old copy cannot come back", name(rel)));
            }
            let leaving: Vec<&String> = p.iter().zip(&lies).filter(|(_, l)| matches!(l, Lies::Done | Lies::AsCopied)).map(|((rel, _), _)| rel).collect();
            for (from, kept) in &olds {
                let (place, kept) = (r.join(from), r.join(kept));
                if kept.is_file() {
                    if place.exists() && !leaving.contains(&from) {
                        return Err(format!("{} exists again; not overwriting it", name(from)));
                    }
                } else if !(begun.is_some() && place.is_file()) {
                    return Err(format!("{} is no longer in the trash", name(from)));
                }
            }
            Ok(lies)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        let lies = match looked {
            Ok(lies) => lies,
            // An Undo cut short that cannot go on puts back what it took (RF9).
            Err(why) => {
                if let Some(stamp) = begun {
                    self.put_back_undo(entry, stamp).await;
                }
                return Err(ToggleError::Refused(why));
            }
        };
        // The second the copies go under: an Undo begun before keeps its own; a new one takes one no copy's name is taken in.
        let stamp = match begun {
            Some(at) => at,
            None => {
                let mut at = epoch_now();
                while copies.iter().any(|m| root.join(crate::trash::TRASH_DIR).join(crate::trash::token(at, &m.to.1)).exists()) {
                    at += 1;
                }
                self.journal.done(entry.id, super::UNCOPIED, epoch_now(), &serde_json::json!({ "stamp": at, "tokens": [] }))?;
                at
            }
        };
        // One plan, all or none (RF9): each copy still as copied into the trash under that second, then Music's old files back to their
        // places. A stop is finished by the pass; a failure puts back what it took: Music never holds neither copy.
        let at = |rel: &str| (root.clone(), rel.to_string());
        let mut steps: Vec<crate::moves::Step> = plan
            .iter()
            .zip(&lies)
            .filter(|(_, l)| **l == super::Lies::AsCopied)
            .map(|((rel, _), _)| crate::moves::Step { from: at(rel), to: at(&format!("{}/{}", crate::trash::TRASH_DIR, crate::trash::token(stamp, rel))) })
            .collect();
        steps.extend(replaced.iter().map(|x| crate::moves::Step { from: at(&x.to.1), to: at(&x.from.1) }));
        let taken: Vec<(String, String)> = plan
            .iter()
            .zip(&lies)
            .filter(|(_, l)| matches!(l, super::Lies::Done | super::Lies::AsCopied))
            .map(|((rel, _), _)| (rel.clone(), crate::trash::token(stamp, rel)))
            .collect();
        let (r, files, rels, held) = (root.clone(), self.files.clone(), plan.iter().map(|(rel, _)| rel.clone()).collect::<Vec<_>>(), moving.clone());
        let moved = tokio::task::spawn_blocking(move || -> Result<()> {
            let _held = held;
            files.move_all(&steps)?;
            super::prune_up_to(&r, &rels);
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .and_then(|done| done);
        if let Err(e) = moved {
            self.put_back_undo(entry, stamp).await;
            return Err(ToggleError::Refused(format!("{e:#}")));
        }
        let (c, event, kept) = (catalogue.clone(), entry.event.clone(), taken.clone());
        let followed = tokio::task::spawn_blocking(move || rows_back_of_copy(&c, &event, &kept)).await.map_err(|e| anyhow::anyhow!("{e}")).and_then(|r| r);
        if let Err(e) = followed {
            tracing::warn!("catalogue: could not put back the files of {title}: {e:#}");
        }
        let now = epoch_now();
        let tokens: Vec<&String> = taken.iter().map(|(_, token)| token).collect();
        self.journal.done(entry.id, super::UNCOPIED, now, &serde_json::json!({ "stamp": stamp, "tokens": tokens }))?;
        self.journal.mark_undone(entry.id, now)?;
        super::tell_moved(&self.lib, title, &moved_pairs(&entry.event, true), super::Told::Both).await;
        *self.owned.lock().await = None;
        catalogue.poke();
        // The backup's list of shared copies, anew (C41).
        crate::app::backup_list::refresh();
        Ok(title.clone())
    }

    /// Replace (decision 0013): the old album's files go to its main library's own trash (each library keeps its own,
    /// 2026-10-16; before, Incoming's), the new ones into that library (0101 point 2) named per the library's template,
    /// each taking over its old track's id. Lidarr stops fetching the album and the old tracks' play data goes to the
    /// new server items (handlers). Undo puts it all back.
    /// Lossless files above that library's quality limit are converted first, in the queue: the plan is checked now,
    /// so a refusal shows at once.
    #[allow(clippy::too_many_arguments)]
    pub async fn replace(&self, source_id: &str, album_id: &str, from: Option<&str>, to: Option<&str>, year: Option<&str>, keep: bool, carry: bool) -> Result<KeepResult, ToggleError> {
        let at = self.locate(source_id, album_id, from).await?;
        self.refuse_if_queued(album_id, at.dir.folder)?;
        let into = self.replaced_in(source_id, to)?;
        let planned = self.plan_replace(&at, into, year).await?.map_err(refused)?;
        let music = planned.root.clone();
        if !self.can_write(&at.root).await || !self.can_write(&music).await {
            return Err(ToggleError::Refused("Cratarr cannot write in both libraries".into()));
        }
        // The old album's cover kept (0.9.5, the owner's choice in the ask): written into the new copy first, so it
        // moves in with it (a conversion's tracks get it inside again from the cover file).
        if keep {
            let (old, new) = (music.join(&planned.old_dir), at.root.join(&at.dir.dir));
            if let Err(e) = tokio::task::spawn_blocking(move || keep_cover(&old, &new)).await {
                tracing::warn!("covers: {e}");
            }
        }
        // What Cratarr knows of the album carried into the new copy (0.9.7, "Replace and carry over").
        if carry {
            let pairs = pairs_of(&music, &planned.old_dir, &at, &planned.plan);
            if let Err(e) = tokio::task::spawn_blocking(move || carry_over(&pairs)).await {
                tracing::warn!("replace: {e}");
            }
        }
        let quality = self.quality_at(source_id, planned.main);
        let convert = self.to_convert(&at, quality).await;
        if let (false, Some(limit)) = (convert.is_empty(), quality.limit()) {
            return Ok(self.enqueue(source_id, at, queue::Action::Replace, year, convert, limit, planned.plan.folder, planned.main));
        }
        self.replace_now(&at, planned.main, year, Vec::new(), None).await
    }

    /// Replace, its files ready (`converted`: what conversion did just before), of the copy in catalogue folder `into`, the
    /// one that was previewed. `job`: the queued job doing it, told the event's id as soon as it is intended.
    /// Replace, told on Health's list of changes to files as it goes, as Keep is (`keep_now`): moving, then adding until
    /// the server has read the album, then done.
    pub(super) async fn replace_now(
        &self,
        at: &Located,
        into: i64,
        year: Option<&str>,
        converted: Vec<crate::journal::Converted>,
        job: Option<u64>,
    ) -> Result<KeepResult, ToggleError> {
        let run = crate::app::file_runs::track(&self.journal, "replace", format!("Replaced: {}", at.album.title));
        let done = self.replacing(at, into, year, converted, job).await;
        match &done {
            Ok(k) => run.moved(k.undo.clone(), self.read_in(at, into)),
            Err(e) => run.failed(&at.album.id, e.to_string()),
        }
        done
    }

    /// Replace's moving.
    async fn replacing(
        &self,
        at: &Located,
        into: i64,
        year: Option<&str>,
        converted: Vec<crate::journal::Converted>,
        job: Option<u64>,
    ) -> Result<KeepResult, ToggleError> {
        self.complete_covers(at).await;
        let Planned { main, root: music, old_dir, old_files, plan } = self.plan_replace(at, Some(into), year).await?.map_err(refused)?;
        if !self.can_write(&at.root).await || !self.can_write(&music).await {
            return Err(ToggleError::Refused("Cratarr cannot write in both libraries".into()));
        }
        let c = self.lib.catalogue();
        let now = epoch_now();
        let token = crate::trash::token(now, &old_dir);
        let (inc, dir) = (at.dir.folder, at.dir.dir.clone());
        let replaced: Vec<TrackMove> = old_files
            .iter()
            .map(|rel| {
                let from = format!("{old_dir}/{rel}");
                TrackMove {
                    track: c.track_of(main, &from),
                    from: (main, from),
                    // Into its library's own trash: each library keeps its own (the owner, 2026-10-16).
                    to: (main, format!("{}/{token}/{rel}", crate::trash::TRASH_DIR)),
                    was: None,
                }
            })
            .collect();
        let moves: Vec<TrackMove> = plan
            .moves
            .iter()
            .map(|(rel, to)| {
                let from = format!("{dir}/{rel}");
                let own = c.track_of(inc, &from);
                let old = plan.takes_over.iter().find(|(_, n)| n == rel).and_then(|(o, _)| c.track_of(main, &format!("{old_dir}/{o}")));
                TrackMove { track: old.or(own), from: (inc, from), to: (main, to.clone()), was: old.and(own) }
            })
            .collect();
        // Before anything moves: what users did with the old tracks' server items (the server forgets them after);
        // the new items get it once they exist (the play-data handler).
        // Both copies count: the old album's tracks and their upgrades in Incoming (played while deciding), each
        // user's data on the two combined into one for the track that lives on. Cratarr's own item is the track, which stays.
        let mut play: Vec<PlayData> = Vec::new();
        let kept: Vec<&TrackMove> = replaced.iter().filter(|r| r.track.is_some() && moves.iter().any(|m| m.track == r.track)).collect();
        for (source, s) in self.lib.connected_sources().into_iter().filter(|(source, _)| source != crate::source::cratarr::KIND.id) {
            let old_rels: Vec<String> = kept.iter().map(|r| r.from.1.clone()).collect();
            let old_items = c.track_ids(&source, main, &old_rels).unwrap_or_default();
            let new_rels: Vec<String> = moves.iter().map(|m| m.from.1.clone()).collect();
            let new_items = c.track_ids(&source, inc, &new_rels).unwrap_or_default();
            let olds = kept.iter().map(|r| r.track).zip(old_items);
            let news = moves.iter().map(|m| m.track).zip(new_items);
            for (track, item) in olds.chain(news) {
                let (Some(track), Some(item)) = (track, item) else { continue };
                let Some(to) = moves.iter().find(|m| m.track == Some(track)).map(|m| m.to.clone()) else { continue };
                let users = match s.user_data(&item).await {
                    Ok(u) => u,
                    Err(e) => {
                        tracing::warn!("{source}: could not read the play data of {item}: {e:#}");
                        continue;
                    }
                };
                for (user, data) in users {
                    match play.iter_mut().find(|p| p.source == source && p.track == track && p.user == user) {
                        Some(p) => p.data = s.combine_user_data(&p.data, &data),
                        None => play.push(PlayData { source: source.clone(), track, old_item: item.clone(), to: to.clone(), user, data }),
                    }
                }
            }
        }
        let event = Event::AlbumReplaced {
            source: at.source.clone(),
            album_id: at.album.id.clone(),
            title: at.album.title.clone(),
            from_dir: (inc, dir),
            to_dir: (main, plan.folder.clone()),
            moves,
            replaced,
            token,
            release_group: at.dir.release_group.clone(),
            converted,
        };
        let underway = self.intend(now, &event)?;
        let id = underway.id();
        if let Some(job) = job {
            self.queue.intended(job, id);
        }
        self.journal.done(id, PLAYDATA_PLAN, now, &serde_json::to_value(&play).map_err(anyhow::Error::from)?)?;
        // Playing from either copy: carried on with the new one after the move (the old items go). A file of the old
        // album is known by the id its upgrade takes over; a file of the upgrade by its own id until then.
        let (order, before, leaving): (Vec<i64>, Vec<(i64, i64)>, Vec<(i64, String)>) = match &event {
            Event::AlbumReplaced { moves, replaced, .. } => (
                moves.iter().filter_map(|m| m.track).collect(),
                moves.iter().filter_map(|m| m.track.map(|t| (m.was.unwrap_or(t), t))).chain(moves.iter().filter_map(|m| m.track.map(|t| (t, t)))).collect(),
                moves.iter().chain(replaced).map(|m| m.from.clone()).collect(),
            ),
            _ => Default::default(),
        };
        let mut playing = self.playing_among(&order, &before, &leaving).await;
        if let Some(p) = playing.as_mut() {
            self.hold(p).await;
        }
        let steps = self.steps(&event).ok_or_else(|| ToggleError::Refused("the folders are not in use any more".into()))?;
        let (files, held) = (self.files.clone(), underway.clone());
        let moved = tokio::task::spawn_blocking(move || {
            let _held = held;
            files.move_all(&steps)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"));
        if let Err(e) = moved.and_then(|r| r) {
            self.journal.forget(id)?;
            return Err(ToggleError::Refused(format!("{e:#}")));
        }
        self.finish_replace(&event, false, playing.is_some()).await;
        self.journal.applied(id)?;
        self.hidden.lock().await.insert(super::hidden_key(&at.album));
        *self.owned.lock().await = None;
        c.poke();
        if let Some(p) = playing {
            super::carry_on::carry_on(self.lib.clone(), p, &event);
        }
        Ok(KeepResult { undo: Some(id.to_string()), title: at.album.title.clone(), folder: plan.folder, queued: false })
    }

    /// After the files moved (or moved back, `back`): the catalogue rows follow, the new files hold the old tracks'
    /// ids (or their own again), and the servers hear of it at once. Safe to repeat.
    /// `hold_gone`: as for Keep (`finish_adopt`).
    pub(super) async fn finish_replace(&self, event: &Event, back: bool, hold_gone: bool) {
        let Event::AlbumReplaced { moves, replaced, token, title, .. } = event else { return };
        let (c, moves_, replaced_, token) = (self.lib.catalogue(), moves.clone(), replaced.clone(), token.clone());
        let done = tokio::task::spawn_blocking(move || rows_follow(&c, &moves_, &replaced_, &token, back))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .and_then(|r| r);
        if let Err(e) = done {
            tracing::warn!("catalogue: could not follow the files of {title}: {e:#}");
        }
        let mut changes: Vec<((i64, String), (i64, String))> = Vec::new();
        for m in replaced.iter().chain(moves) {
            changes.push(if back { (m.to.clone(), m.from.clone()) } else { (m.from.clone(), m.to.clone()) });
        }
        super::tell_moved(&self.lib, title, &changes, if hold_gone { super::Told::Came } else { super::Told::Both }).await;
    }

    /// Undo a Replace: the new files back into Incoming with their own ids, the old ones back from the trash with
    /// theirs, Lidarr fetching as before.
    pub(super) async fn undo_replace(&self, entry: &Entry) -> Result<String, ToggleError> {
        let Event::AlbumReplaced { source, album_id, title, from_dir, .. } = &entry.event else { unreachable!() };
        if entry.undone {
            return Err(ToggleError::Refused(format!("{title} is already back in Incoming")));
        }
        let steps = self.undo_steps(&entry.event).ok_or_else(|| ToggleError::Refused("the folders are not in use any more".into()))?;
        let moving = self.lib.catalogue().copies_moving();
        let (files, held) = (self.files.clone(), moving.clone());
        tokio::task::spawn_blocking(move || {
            let _held = held;
            files.move_all(&steps)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
        self.finish_replace(&entry.event, true, false).await;
        self.unconvert_rows(&entry.event).await;
        self.journal.mark_undone(entry.id, epoch_now())?;
        self.resume_fetching(entry.id, title).await;
        self.unhide(source, album_id, from_dir.0).await;
        *self.owned.lock().await = None;
        self.lib.catalogue().poke();
        Ok(title.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{RY01_DIR, bobs_copy, copy_at, lying, ry01_file, with_three_own};
    use super::*;
    use crate::catalogue::Catalogue;
    use crate::moves::Step;

    /// Each file under `dir`, its bytes alone, whatever it is called.
    fn bytes_under(dir: &std::path::Path) -> std::collections::BTreeSet<Vec<u8>> {
        lying(dir).into_values().map(|(b, _)| b).collect()
    }

    /// RY01.1, without a server: the event copies from Bob's music (folder 3) into Music (folder 0) and moves only Music's own files,
    /// its cover too, into Music's trash; no step has Bob's folder as its target; no release group, so Lidarr is told nothing. Done,
    /// Music's copy is Bob's files alone, as many as its sheet said, their bytes and times, on the old files' tracks; Bob's copy as it
    /// was; one journal entry, applied, holding each copy's size and time as copied.
    #[tokio::test]
    async fn ry01_plans_copies_from_bob_and_moves_only_musics_files() {
        let (svc, music, inc, ann, bob) = with_three_own("ry01-plan", RY01_DIR, RY01_DIR, Some("2019"), 2, 2).await;
        let at = bobs_copy(&svc, &bob, RY01_DIR).await;
        let (event, sheet, root) = svc.event_from_copy(&at, (0, RY01_DIR.into()), None, 100).await.unwrap();
        assert_eq!((root, sheet.clone()), (music.clone(), ReplaceSheet { folder: RY01_DIR.into(), tracks: 2, added: 0, files: 3 }));
        let Event::AlbumReplacedFromCopy { copies, replaced, from_dir, to_dir, .. } = &event else { panic!("{event:?}") };
        assert_eq!((from_dir.0, to_dir.0), (3, 0));
        assert!(copies.iter().all(|m| m.from.0 == 3 && m.to.0 == 0), "copies from Bob's music into Music");
        assert!(replaced.iter().all(|r| r.from.0 == 0 && r.to.0 == 0 && r.to.1.starts_with(".cratarr-trash/100/")), "Music's files alone, into Music's trash");
        assert_eq!(replaced.len(), 3, "the MP3s and the cover");
        assert!(svc.steps(&event).unwrap().iter().all(|s| s.to.0 != bob), "no step has Bob's folder as its target");
        assert_eq!(event.album().1, None, "no release group: Lidarr is not told");
        let (theirs, c) = (lying(&bob), svc.lib.catalogue());
        let olds: Vec<Option<i64>> = (1..=2).map(|n| c.track_of(0, &ry01_file(RY01_DIR, n, "mp3"))).collect();
        // OQ-G: a tap whose sheet showed otherwise is refused, nothing written.
        let (musics, other) = (lying(&music), ReplaceSheet { files: 4, ..sheet.clone() });
        let e = svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, Some(&other)).await.unwrap_err();
        assert!(e.to_string().contains("Changed since you looked"), "{e}");
        assert!(lying(&music) == musics && svc.journal.album_events_since(0).unwrap().is_empty(), "nothing written");
        // A copy on another track is no copy of Music's: the pairing refuses it, by the old file's name.
        let elsewhere = c.track_of(0, &ry01_file(RY01_DIR, 1, "mp3")).unwrap();
        c.give_track_for_tests(3, &ry01_file(RY01_DIR, 2, "flac"), elsewhere);
        let refused = svc.plan_from_copy(&at, (0, RY01_DIR.into()), None).await.unwrap().err().expect("refused");
        assert!(matches!(refused, ReplaceRefused::Unmatched(_)), "{refused:?}");
        c.give_track_for_tests(3, &ry01_file(RY01_DIR, 2, "flac"), olds[1].unwrap());
        let done = svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, Some(&sheet)).await.unwrap();
        assert_eq!(lying(&bob), theirs, "Bob's copy as it was, nothing beside it");
        let new = lying(&music.join(&done.folder));
        assert_eq!(new.len(), sheet.files, "as many files as the sheet said");
        let theirs_named: std::collections::BTreeMap<String, (Vec<u8>, std::time::SystemTime)> = theirs.into_iter().map(|(k, v)| (k.strip_prefix(&format!("{RY01_DIR}/")).unwrap().to_string(), v)).collect();
        assert_eq!(new, theirs_named, "Bob's files alone, their bytes and times");
        for n in 1..=2 {
            assert_eq!(c.track_of(0, &ry01_file(RY01_DIR, n, "flac")), olds[n as usize - 1], "on the old file's track");
        }
        let id: crate::journal::EventId = done.undo.unwrap().parse().unwrap();
        assert!(svc.journal.get(id).unwrap().is_some_and(|e| e.applied && matches!(e.event, Event::AlbumReplacedFromCopy { .. })));
        let made: Vec<Option<(u64, i64)>> = serde_json::from_value(svc.journal.result(id, super::super::COPIED).unwrap().unwrap()).unwrap();
        assert!(made.len() == 3 && made.iter().all(Option::is_some), "each copy's size and time as copied: {made:?}");
        // A second tap a moment after (RY01.13, two administrators): the copy is in Music's place already, so it is refused.
        let now = lying(&music);
        let e = svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, Some(&sheet)).await.unwrap_err();
        assert!(e.to_string().contains("in its place already"), "{e}");
        assert_eq!(lying(&music), now, "nothing moved by it");
        assert_eq!(svc.journal.album_events_since(0).unwrap().len(), 1, "one journal entry");
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// Files as the disk has them, but the own copy's files vanish after the first `n` are copied (RY01.5: Bob deletes his copy
    /// mid-copy): those copied as `moves::copy` copies them, then "not there".
    struct VanishesAfter(usize);

    impl crate::ports::Files for VanishesAfter {
        fn trash(&self, folder: &std::path::Path, rel: &str, now_epoch: i64) -> anyhow::Result<String> {
            crate::trash::trash(folder, rel, now_epoch)
        }
        fn restore(&self, folder: &std::path::Path, token: &str) -> anyhow::Result<String> {
            crate::trash::restore(folder, token)
        }
        fn purge(&self, folder: &std::path::Path, now_epoch: i64, keep_days: u32) -> anyhow::Result<usize> {
            crate::trash::purge(folder, now_epoch, keep_days)
        }
        fn can_write(&self, _folder: &std::path::Path) -> bool {
            true
        }
        fn move_all(&self, plan: &[Step]) -> anyhow::Result<()> {
            crate::moves::apply(plan)
        }
        fn move_back(&self, plan: &[Step]) -> anyhow::Result<()> {
            crate::moves::revert(plan)
        }
        fn clear_parts(&self, plan: &[Step]) {
            crate::moves::clear_parts(plan)
        }
        fn copy_all(&self, plan: &[Step]) -> anyhow::Result<Vec<bool>> {
            for s in plan.iter().take(self.0) {
                crate::moves::clone_file(&s.from.0.join(&s.from.1), &s.to.0.join(&s.to.1))?;
            }
            anyhow::bail!("{} is not there", plan[self.0].from.1)
        }
        fn shares(&self, _from: &std::path::Path, _to: &std::path::Path) -> bool {
            false
        }
    }

    /// RY01.5: a file of Bob's vanishes during the copy: the action answers refused with the reason; Music's old files back under
    /// their names, bytes and times, on their tracks, not trashed; nothing of the copy left in Music (no file, no part, no folder made
    /// for it); no journal entry; its task ended with that error. Bob's other files untouched.
    #[tokio::test]
    async fn a_copy_cut_short_puts_musics_files_back_and_leaves_nothing() {
        let (svc, music, inc, ann, bob) = with_three_own("ry01-vanish", RY01_DIR, RY01_DIR, Some("2019"), 2, 2).await;
        let svc = IncomingService { files: std::sync::Arc::new(VanishesAfter(1)), ..svc };
        let (before, theirs) = (lying(&music), lying(&bob));
        let c = svc.lib.catalogue();
        let olds: Vec<Option<i64>> = (1..=2).map(|n| c.track_of(0, &ry01_file(RY01_DIR, n, "mp3"))).collect();
        let at = bobs_copy(&svc, &bob, RY01_DIR).await;
        const ACCOUNT: &str = "ry01-vanish";
        let who = crate::serve::who::Who { user: ACCOUNT.into(), admin: true, scope: crate::serve::who::Scope::Full, home: true };
        let e = crate::serve::who::acting_as(who, svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, None)).await.unwrap_err();
        assert!(e.to_string().contains("is not there"), "refused with the reason: {e}");
        assert_eq!(lying(&music), before, "Music's album as it was, nothing of the copy, no part, no trash left");
        for (n, t) in (1..=2).zip(&olds) {
            assert_eq!(super::super::tests::row_at(&c, 0, &ry01_file(RY01_DIR, n, "mp3")), Some((*t, None)), "on its track, not trashed");
        }
        assert!(!c.has_row(0, &ry01_file(RY01_DIR, 1, "flac")));
        assert!(svc.journal.intents().unwrap().is_empty() && svc.journal.album_events_since(0).unwrap().is_empty(), "no entry");
        let tasks = crate::app::tasks::tasks().of(ACCOUNT);
        assert!(tasks.iter().any(|t| t.kind == "replace" && t.finished.is_some() && t.error.as_deref().is_some_and(|e| e.contains("is not there"))), "its task ended with the error: {tasks:?}");
        assert_eq!(lying(&bob), theirs, "Bob's files untouched");
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// RF11 (the review of runs 11 and 12, RA18; RY01.6 case 3): Music's copy whose folder holds another record's copy inside it (a
    /// subfolder of another album) is refused, "Cratarr cannot see that album's folder", and nothing moves: the old folder goes to the
    /// trash whole, so what else lies in it must not. The guard reads the served library, which this test gives it on its thread
    /// (`served_in_tests`); with none, as every RY01 unit test ran before, it was skipped and the case untested. Its folder its copy's
    /// alone, the plan goes ahead.
    #[tokio::test]
    async fn replace_from_a_copy_refuses_a_music_folder_another_record_lies_in() {
        use crate::settings::LibraryKind;
        let (svc, music, inc, ann, bob) = with_three_own("rf11-nested", RY01_DIR, RY01_DIR, Some("2019"), 2, 2).await;
        let c = svc.lib.catalogue();
        let served = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        served.library.set_kinds(vec![(0, LibraryKind::Main), (1, LibraryKind::Incoming), (2, LibraryKind::Own), (3, LibraryKind::Own)]);
        served.attach_catalogue(c.clone());
        let build = || {
            let rules = crate::catalogue::Rules::default();
            let (_, print) = served.library.change(&c, &rules).unwrap();
            served.library.build_from(&c, &rules, &print).unwrap();
        };
        build();
        super::super::served_in_tests(Some(served.clone()));
        let at = bobs_copy(&svc, &bob, RY01_DIR).await;
        assert!(svc.plan_from_copy(&at, (0, RY01_DIR.into()), None).await.unwrap().is_ok(), "Music's folder its copy's alone: planned");
        let bonus = format!("{RY01_DIR}/Bonus/01 B - Other.mp3");
        std::fs::create_dir_all(music.join(RY01_DIR).join("Bonus")).unwrap();
        std::fs::write(music.join(&bonus), b"another record").unwrap();
        c.walk_all();
        c.pretend_read_for_tests();
        c.tag_for_tests(0, &bonus, &[("albumartist", "B"), ("artist", "B"), ("album", "Other"), ("title", "Other One"), ("tracknumber", "1")]);
        c.identify().unwrap();
        build();
        let before = lying(&music);
        let refused = svc.plan_from_copy(&at, (0, RY01_DIR.into()), None).await.unwrap().err();
        let tapped = svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, None).await;
        super::super::served_in_tests(None);
        assert!(matches!(refused, Some(ReplaceRefused::Other(ref why)) if why.contains("cannot see that album's folder")), "{refused:?}");
        assert!(tapped.is_err(), "the tap refused too");
        assert_eq!(lying(&music), before, "nothing moved");
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// RY01.6, the plan on disk. (1) Music's template names Bob's copy as Music's old folder: allowed, and after it the folder holds
    /// Bob's files alone, Music's old ones, its cover too, in Music's trash. (2) That name taken by another record's folder: refused
    /// ("already has a folder named"), the other record's file untouched. (3) Music's copy at the library's root: refused ("cannot see
    /// that album's folder"), nothing moved. (4) No year in either copy's tags: the sheet asks it; with one, the folder is named by it
    /// and no tag is written into either copy's files.
    #[tokio::test]
    async fn replace_from_a_copy_refuses_a_folder_another_record_holds() {
        // (1)
        let (svc, music, inc, ann, bob) = with_three_own("ry01-taken1", RY01_DIR, RY01_DIR, Some("2019"), 2, 2).await;
        let (old, theirs) = (bytes_under(&music.join(RY01_DIR)), bytes_under(&bob.join(RY01_DIR)));
        let at = bobs_copy(&svc, &bob, RY01_DIR).await;
        svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, None).await.unwrap();
        assert_eq!(lying(&music.join(RY01_DIR)).into_keys().collect::<Vec<_>>(), [ry01_file("", 1, "flac"), ry01_file("", 2, "flac"), "cover.jpg".to_string()], "Bob's files alone");
        assert_eq!(bytes_under(&music.join(RY01_DIR)), theirs);
        assert_eq!(bytes_under(&music.join(crate::trash::TRASH_DIR)), old, "Music's old files, its cover too, in Music's trash");
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
        // (2)
        let (svc, music, inc, ann, bob) = with_three_own("ry01-taken2", "A/Album", RY01_DIR, Some("2019"), 2, 2).await;
        std::fs::create_dir_all(music.join(RY01_DIR)).unwrap();
        std::fs::write(music.join(RY01_DIR).join("other.mp3"), b"another record").unwrap();
        let before = lying(&music);
        let at = bobs_copy(&svc, &bob, RY01_DIR).await;
        let refused = svc.plan_from_copy(&at, (0, "A/Album".into()), None).await.unwrap().err().expect("refused");
        assert!(matches!(refused, ReplaceRefused::Other(ref why) if why.contains("already has a folder named")), "{refused:?}");
        assert!(svc.replacing_from_copy(&at, (0, "A/Album".into()), None, None).await.is_err());
        assert_eq!(std::fs::read(music.join(RY01_DIR).join("other.mp3")).unwrap(), b"another record");
        assert_eq!(lying(&music), before, "nothing moved");
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
        // (3)
        let (svc, music, inc, ann, bob) = with_three_own("ry01-taken3", "", RY01_DIR, Some("2019"), 2, 2).await;
        let before = lying(&music);
        let at = bobs_copy(&svc, &bob, RY01_DIR).await;
        let refused = svc.plan_from_copy(&at, (0, String::new()), None).await.unwrap().err().expect("refused");
        assert!(matches!(refused, ReplaceRefused::Other(ref why) if why.contains("cannot see that album's folder")), "{refused:?}");
        assert!(svc.replacing_from_copy(&at, (0, String::new()), None, None).await.is_err());
        assert_eq!(lying(&music), before, "nothing moved");
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
        // (4)
        let (svc, music, inc, ann, bob) = with_three_own("ry01-taken4", "A/Album", "A/Album", None, 2, 2).await;
        let theirs = lying(&bob);
        let at = bobs_copy(&svc, &bob, "A/Album").await;
        let refused = svc.plan_from_copy(&at, (0, "A/Album".into()), None).await.unwrap().err().expect("the sheet asks the year");
        assert!(matches!(refused, ReplaceRefused::NeedsYear), "{refused:?}");
        let done = svc.replacing_from_copy(&at, (0, "A/Album".into()), Some("2019"), None).await.unwrap();
        assert_eq!(done.folder, RY01_DIR, "the year in the folder's name");
        assert_eq!(bytes_under(&music.join(RY01_DIR)), bytes_under(&bob.join("A/Album")), "never as a tag into the copies' files");
        assert_eq!(lying(&bob), theirs, "nor into Bob's");
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// RY01.13, the light rule: a Replace from Bob's copy is a task of its account's, of kind "replace", stepping "Copying Zebra" with
    /// its files done of all of them (0 of 3, then 3 of 3), finished with no error. A second action of the account's at once, while
    /// one of its kind runs, goes on with no task of its own. No new queue or state file: `app::tasks` alone.
    #[tokio::test]
    async fn a_replace_from_a_copy_tells_its_progress_as_a_task() {
        let (svc, music, inc, ann, bob) = with_three_own("ry01-task", RY01_DIR, RY01_DIR, Some("2019"), 2, 2).await;
        let at = bobs_copy(&svc, &bob, RY01_DIR).await;
        const ACCOUNT: &str = "ry01-root";
        let mut told = crate::app::tasks::tasks().subscribe();
        let who = crate::serve::who::Who { user: ACCOUNT.into(), admin: true, scope: crate::serve::who::Scope::Full, home: true };
        crate::serve::who::acting_as(who.clone(), svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, None)).await.unwrap();
        let mut seen = Vec::new();
        loop {
            match told.try_recv() {
                Ok(t) if t.account == ACCOUNT => seen.push(t),
                Ok(_) | Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {}
                Err(_) => break,
            }
        }
        let steps: Vec<(String, u64, Option<u64>)> = seen.iter().filter_map(|t| t.step.as_ref().map(|s| (s.name.clone(), s.done, s.total))).collect();
        assert!(steps.contains(&("Copying Zebra".into(), 0, Some(3))) && steps.contains(&("Copying Zebra".into(), 3, Some(3))), "{steps:?}");
        let last = seen.last().expect("told");
        assert!(last.kind == "replace" && last.finished.is_some() && last.error.is_none(), "{last:?}");
        // Another of its kind running for the account: the action goes on, with no task of its own.
        let undo = svc.journal.album_events_since(0).unwrap()[0].0;
        svc.undo(&undo.to_string()).await.unwrap();
        let running = crate::app::tasks::tasks().begin("replace", ACCOUNT, false).unwrap();
        crate::serve::who::acting_as(who, svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, None)).await.unwrap();
        let mine: Vec<u64> = crate::app::tasks::tasks().of(ACCOUNT).iter().filter(|t| t.finished.is_none()).map(|t| t.id).collect();
        assert_eq!(mine, [running.id()], "no task of its own, and none left running");
        running.finish(Ok::<(), anyhow::Error>(()));
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// RY01.13 (cases 2 and 4, as far as one server's tests can hold them): a Replace from Bob's copy while another of the record's
    /// copy in Music goes on is refused ("being replaced"), and while a conversion job waits on Music's copy ("being converted"); either
    /// way nothing is written and no entry kept.
    #[tokio::test]
    async fn a_replace_from_a_copy_being_converted_or_replaced_is_refused() {
        let (svc, music, inc, ann, bob) = with_three_own("ry01-busy", RY01_DIR, RY01_DIR, Some("2019"), 2, 2).await;
        let at = bobs_copy(&svc, &bob, RY01_DIR).await;
        let before = lying(&music);
        let other = svc.alone("al-1", 0, "Zebra", "being replaced").unwrap();
        let e = svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, None).await.unwrap_err();
        assert!(e.to_string().contains("being replaced"), "{e}");
        drop(other);
        let musics = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), RY01_DIR)).await.unwrap();
        svc.enqueue("cratarr", musics, queue::Action::Keep, None, vec![ry01_file("", 1, "mp3")], crate::convert::CD, "B/Album".into(), 1);
        let e = svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, None).await.unwrap_err();
        assert!(e.to_string().contains("being converted"), "{e}");
        assert_eq!(lying(&music), before, "nothing written");
        assert!(svc.journal.intents().unwrap().is_empty() && svc.journal.album_events_since(0).unwrap().is_empty());
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// S.2 case 3 (with RY01.2): Music's MP3 and Bob's FLAC each 8 files saying 10, on the same 8 tracks, short alike: the sheet counts
    /// 8 tracks taken over, and after the Replace Music's copy still lacks tracks by the numbers Health's "missing tracks" reads.
    #[tokio::test]
    async fn both_short_alike_the_sheet_counts_eight_and_musics_copy_still_lacks_tracks() {
        use crate::catalogue::identity::{Judged, Lacks, lacks};
        let (svc, music, inc, ann, bob) = with_three_own("ry01-short8", RY01_DIR, RY01_DIR, Some("2019"), 8, 10).await;
        let at = bobs_copy(&svc, &bob, RY01_DIR).await;
        let (_, sheet, _) = svc.event_from_copy(&at, (0, RY01_DIR.into()), None, 100).await.unwrap();
        assert_eq!((sheet.tracks, sheet.added, sheet.files), (8, 0, 9), "8 tracks, and the cover");
        svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, Some(&sheet)).await.unwrap();
        let c = svc.lib.catalogue();
        let numbers = c.numbers_under(0, RY01_DIR).unwrap();
        assert_eq!(numbers.len(), 8);
        assert_eq!(lacks(&Judged { files: numbers.into_iter().map(|(_, n)| n).collect(), ..Judged::default() }), Lacks::Short, "still missing tracks");
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-replace-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn put(root: &std::path::Path, rel: &str, body: &[u8]) {
        std::fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
        std::fs::write(root.join(rel), body).unwrap();
    }

    #[test]
    fn the_new_files_take_over_the_old_tracks_and_undo_gives_each_its_own_back() {
        let (music, inc) = (tmp("music"), tmp("inc"));
        put(&music, "X/01.mp3", b"old one");
        put(&music, "X/02.mp3", b"old two");
        put(&inc, "A/1.flac", b"new one");
        put(&inc, "A/2.flac", b"new two");
        put(&inc, "A/3.flac", b"bonus");
        let c = Catalogue::open(None, vec![music.clone(), inc.clone()]).unwrap();
        c.walk_all();
        c.pretend_read_for_tests();
        c.identify().unwrap();
        let id = |f: i64, r: &str| c.track_of(f, r).unwrap();
        let (old1, old2, own1, own2, own3) = (id(0, "X/01.mp3"), id(0, "X/02.mp3"), id(1, "A/1.flac"), id(1, "A/2.flac"), id(1, "A/3.flac"));
        let token = "100/X";
        let mv = |track, from: (i64, &str), to: (i64, &str), was| TrackMove { track, from: (from.0, from.1.into()), to: (to.0, to.1.into()), was };
        let replaced = vec![
            mv(Some(old1), (0, "X/01.mp3"), (1, ".cratarr-trash/100/X/01.mp3"), None),
            mv(Some(old2), (0, "X/02.mp3"), (1, ".cratarr-trash/100/X/02.mp3"), None),
        ];
        // The second new file keeps the old one's name: the plan moves the old file out first.
        let moves = vec![
            mv(Some(old1), (1, "A/1.flac"), (0, "X/01.flac"), Some(own1)),
            mv(Some(old2), (1, "A/2.flac"), (0, "X/02.mp3"), Some(own2)),
            mv(Some(own3), (1, "A/3.flac"), (0, "X/03.flac"), None),
        ];
        let roots = [music.clone(), inc.clone()];
        let steps: Vec<Step> = replaced
            .iter()
            .chain(&moves)
            .map(|m| Step { from: (roots[m.from.0 as usize].clone(), m.from.1.clone()), to: (roots[m.to.0 as usize].clone(), m.to.1.clone()) })
            .collect();

        crate::moves::apply(&steps).unwrap();
        rows_follow(&c, &moves, &replaced, token, false).unwrap();
        rows_follow(&c, &moves, &replaced, token, false).unwrap();
        assert_eq!(c.file_of(old1), Some((0, "X/01.flac".into())));
        assert_eq!(c.file_of(old2), Some((0, "X/02.mp3".into())));
        assert_eq!(std::fs::read(music.join("X/02.mp3")).unwrap(), b"new two");
        assert_eq!(c.file_of(own3), Some((0, "X/03.flac".into())), "a bonus track keeps its own id");
        assert_eq!(c.file_of(own1), None);
        c.walk_all();
        assert_eq!(c.identify().unwrap(), (0, 0), "the rows match the disk: nothing new, nothing moved");

        crate::moves::revert(&steps).unwrap();
        rows_follow(&c, &moves, &replaced, token, true).unwrap();
        rows_follow(&c, &moves, &replaced, token, true).unwrap();
        assert_eq!(c.file_of(old1), Some((0, "X/01.mp3".into())));
        assert_eq!(c.file_of(old2), Some((0, "X/02.mp3".into())));
        assert_eq!(std::fs::read(music.join("X/02.mp3")).unwrap(), b"old two");
        assert_eq!(c.file_of(own1), Some((1, "A/1.flac".into())));
        assert_eq!(c.file_of(own2), Some((1, "A/2.flac".into())));
        assert_eq!(c.file_of(own3), Some((1, "A/3.flac".into())));
        c.walk_all();
        assert_eq!(c.identify().unwrap(), (0, 0));
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }
}
