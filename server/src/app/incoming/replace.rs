//! Replace (decision 0013): an upgrade in Incoming takes the place of its album in Music. Each old track is paired
//! with a new file, which takes over its track id (playlists and play data follow); the old files go to the trash in
//! the Incoming folder. Refused when an old track has no match. Write-ahead like Keep and Delete; Undo puts it all back.

use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;

use super::{IncomingService, KeepResult, Located, PLAYDATA_PLAN, PlayData, files_under, queue, verdicts};
use crate::app::ToggleError;
use crate::app::library::epoch_now;
use crate::incoming::{AlbumFile, ReplacePlan, ReplaceRefusal};
use crate::journal::{Entry, Event, TrackMove};

/// What Replace would do with an album, for the page to show before asking.
#[derive(Debug, Serialize)]
pub struct ReplacePreview {
    pub title: String,
    /// The album in Music it replaces (its folder).
    pub old: String,
    /// Where the new album goes in Music, when it can go ahead.
    pub folder: Option<String>,
    /// Old tracks the new files take over.
    pub tracks: usize,
    /// New tracks the old album does not have.
    pub added: usize,
    pub refused: Option<ReplaceRefused>,
    /// Hi-res files converted to CD format first (Replace then waits in the queue).
    pub convert: usize,
    /// Whether a Lidarr is set, to be told to stop fetching the album; without one it may fetch it again.
    pub acquisition: bool,
    /// Both covers, side by side for the choice (0.9.5): the album's in Music and the new copy's.
    pub covers: ReplaceCovers,
    /// How the new copy's tags differ from the album's in Music, the differences only (0.9.7, the owner): album-wide
    /// once (`track` none), else by track.
    pub diff: Vec<TagDiff>,
    /// What the new copy lacks that the album had ("Year", "MusicBrainz ids", "4 fewer tags").
    pub lost: Vec<String>,
    /// What "Replace and carry over" brings into the new copy, and what follows the album by itself.
    pub carry: Vec<String>,
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

/// The differences of each pair (the old file in Music, the new one in Incoming), what the new copy loses, and what
/// carrying over brings. Blocking: it reads every file's tags.
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

/// Each old track in Music with the new file taking it over, as paths.
fn pairs_of(music: &std::path::Path, old_dir: &str, at: &Located, plan: &ReplacePlan) -> Vec<(PathBuf, PathBuf)> {
    plan.takes_over.iter().map(|(o, n)| (music.join(old_dir).join(o), at.root.join(&at.dir.dir).join(n))).collect()
}


/// The two covers Replace chooses between (0.9.5): `keep` the one in Music, or take the `new` copy's.
#[derive(Debug, Default, Serialize)]
pub struct ReplaceCovers {
    pub current: Option<String>,
    pub new: Option<String>,
}

/// The cover of the album in Music at `old_dir` and of the new copy, as the page shows covers.
fn covers_of(source_id: &str, album_id: &str, old_dir: &str) -> ReplaceCovers {
    let current = crate::serve::instance()
        .and_then(|s| s.library.album_in(0, old_dir).ok().flatten())
        .map(|id| format!("/api/albums/{}/{id}/cover", crate::source::cratarr::KIND.id));
    ReplaceCovers { current, new: Some(format!("/api/albums/{source_id}/{album_id}/cover")) }
}

/// The Music album's cover written into the new copy's folder before Replace (0.9.5, `cover: keep`): its cover file
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

/// Replace worked out: the album in Music it replaces and what moves.
struct Planned {
    /// The old album's folder, relative to Music.
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
            duration_ms: lengths.get(&rel).copied(),
            rel: rel.strip_prefix(&prefix).unwrap_or(&rel).to_string(),
            tags: crate::naming::Tags(tags),
        })
        .collect())
}

/// The catalogue rows follow Replace's moves (or their Undo, `back`): the old files' rows into the trash and the new
/// files' into Music, then the new files hold the old tracks' ids and the old files let go (back: each its own again).
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
    /// Work out Replace for an album: the album it replaces and what moves, or why not. Changes nothing.
    async fn plan_replace(&self, at: &Located, year: Option<&str>) -> Result<Result<Planned, ReplaceRefused>, ToggleError> {
        let music = PathBuf::from(self.lib.music().ok_or_else(|| ToggleError::Refused("no Music library is set".into()))?);
        let (library, incoming) = self.owned_index().await.ok_or_else(|| ToggleError::Refused("the catalogue is not ready yet".into()))?;
        let old_dir = verdicts(std::slice::from_ref(&at.album), &library, &incoming)
            .remove(&at.album.id)
            .map(|v| v.library)
            .ok_or_else(|| ToggleError::Refused(format!("{} is not in Music", at.album.title)))?;
        let (c, folder, dir, root, year, template) =
            (self.lib.catalogue(), at.dir.folder, at.dir.dir.clone(), at.root.clone(), year.map(str::to_string), self.lib.template());
        tokio::task::spawn_blocking(move || -> Result<Result<Planned, ReplaceRefused>> {
            let new = album_files(&c, folder, &dir)?;
            let old = album_files(&c, 0, &old_dir)?;
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
                return Ok(Err(ReplaceRefused::Other(format!("Music already has a folder named {}", plan.folder))));
            }
            // Ids the new files take over must exist: a track still being read has none yet.
            if plan.takes_over.iter().any(|(o, _)| c.track_of(0, &format!("{old_dir}/{o}")).is_none()) {
                return Ok(Err(ReplaceRefused::Other("Cratarr is still reading the album in Music; try again in a minute".into())));
            }
            Ok(Ok(Planned { old_files: files_under(&music.join(&old_dir)), old_dir, plan }))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .map_err(ToggleError::from)
    }

    /// What Replace would do with an album, for the page to show before asking. Changes nothing.
    pub async fn replace_preview(&self, source_id: &str, album_id: &str, year: Option<&str>) -> Result<ReplacePreview, ToggleError> {
        let at = self.locate(source_id, album_id).await?;
        let (library, incoming) = self.owned_index().await.ok_or_else(|| ToggleError::Refused("the catalogue is not ready yet".into()))?;
        let old = verdicts(std::slice::from_ref(&at.album), &library, &incoming).remove(&at.album.id).map(|v| v.library).unwrap_or_default();
        let convert = self.to_convert(&at).await.len();
        let acquisition = self.acquisition().is_some();
        let music = PathBuf::from(self.lib.music().unwrap_or_default());
        Ok(match self.plan_replace(&at, year).await? {
            Ok(p) => {
                let pairs = pairs_of(&music, &p.old_dir, &at, &p.plan);
                let (diff, lost, carry) = tokio::task::spawn_blocking(move || compare(&pairs, true)).await.unwrap_or_default();
                ReplacePreview {
                    covers: covers_of(source_id, album_id, &p.old_dir),
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
                }
            }
            Err(r) => ReplacePreview {
                covers: covers_of(source_id, album_id, &old),
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
            },
        })
    }

    /// Replace (decision 0013): the old album's files go to Music's own trash (each library keeps its own, 2026-10-16;
    /// before, Incoming's), the new ones into Music
    /// named per the library's template, each taking over its old track's id. Lidarr stops fetching the album and the
    /// old tracks' play data goes to the new server items (handlers). Undo puts it all back.
    /// Hi-res files are converted first, in the queue: the plan is checked now, so a refusal shows at once.
    pub async fn replace(&self, source_id: &str, album_id: &str, year: Option<&str>, keep: bool, carry: bool) -> Result<KeepResult, ToggleError> {
        self.refuse_if_queued(album_id)?;
        let at = self.locate(source_id, album_id).await?;
        let planned = self.plan_replace(&at, year).await?.map_err(refused)?;
        let music = PathBuf::from(self.lib.music().unwrap_or_default());
        if !self.can_write(&at.root).await || !self.can_write(&music).await {
            return Err(ToggleError::Refused("Cratarr cannot write in Incoming or Music".into()));
        }
        // The cover in Music kept (0.9.5, the owner's choice in the ask): written into the new copy first, so it moves
        // in with it (a conversion's tracks get it inside again from the cover file).
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
        let convert = self.to_convert(&at).await;
        if !convert.is_empty() {
            return Ok(self.enqueue(source_id, at, queue::Action::Replace, year, convert, planned.plan.folder, 0));
        }
        self.replace_now(&at, year, Vec::new(), None).await
    }

    /// Replace, its files ready (`converted`: what conversion did just before). `job`: the queued job doing it, told
    /// the event's id as soon as it is intended.
    pub(super) async fn replace_now(
        &self,
        at: &Located,
        year: Option<&str>,
        converted: Vec<crate::journal::Converted>,
        job: Option<u64>,
    ) -> Result<KeepResult, ToggleError> {
        self.complete_covers(at).await;
        let Planned { old_dir, old_files, plan } = self.plan_replace(at, year).await?.map_err(refused)?;
        let music = PathBuf::from(self.lib.music().unwrap_or_default());
        if !self.can_write(&at.root).await || !self.can_write(&music).await {
            return Err(ToggleError::Refused("Cratarr cannot write in Incoming or Music".into()));
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
                    track: c.track_of(0, &from),
                    from: (0, from),
                    // Into Music's own trash: each library keeps its own (the owner, 2026-10-16).
                    to: (0, format!("{}/{token}/{rel}", crate::trash::TRASH_DIR)),
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
                let old = plan.takes_over.iter().find(|(_, n)| n == rel).and_then(|(o, _)| c.track_of(0, &format!("{old_dir}/{o}")));
                TrackMove { track: old.or(own), from: (inc, from), to: (0, to.clone()), was: old.and(own) }
            })
            .collect();
        // Before anything moves: what users did with the old tracks' server items (the server forgets them after);
        // the new items get it once they exist (the play-data handler).
        // Both copies count: the old album's tracks and their upgrades in Incoming (played while deciding), each
        // user's data on the two combined into one for the track that lives on.
        let mut play: Vec<PlayData> = Vec::new();
        let kept: Vec<&TrackMove> = replaced.iter().filter(|r| r.track.is_some() && moves.iter().any(|m| m.track == r.track)).collect();
        for (source, s) in self.lib.connected_sources() {
            let old_rels: Vec<String> = kept.iter().map(|r| r.from.1.clone()).collect();
            let old_items = c.track_ids(&source, 0, &old_rels).unwrap_or_default();
            let new_rels: Vec<String> = moves.iter().map(|m| m.from.1.clone()).collect();
            let new_items = c.track_ids(&source, inc, &new_rels).unwrap_or_default();
            let olds = kept.iter().map(|r| r.track).zip(old_items);
            let news = moves.iter().map(|m| m.track).zip(new_items);
            for (track, item) in olds.chain(news) {
                let (Some(track), Some(item)) = (track, item) else { continue };
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
                        None => play.push(PlayData { source: source.clone(), track, old_item: item.clone(), user, data }),
                    }
                }
            }
        }
        let event = Event::AlbumReplaced {
            source: at.source.clone(),
            album_id: at.album.id.clone(),
            title: at.album.title.clone(),
            from_dir: (inc, dir),
            to_dir: (0, plan.folder.clone()),
            moves,
            replaced,
            token,
            release_group: at.dir.release_group.clone(),
            converted,
        };
        let id = self.journal.intend(now, &event)?;
        if let Some(job) = job {
            self.queue.intended(job, id);
        }
        self.journal.done(id, PLAYDATA_PLAN, now, &serde_json::to_value(&play).map_err(anyhow::Error::from)?)?;
        // Playing from either copy: carried on with the new one after the move (the old items go). A file of the old
        // album is known by the id its upgrade takes over; a file of the upgrade by its own id until then.
        let (order, before): (Vec<i64>, Vec<(i64, i64)>) = match &event {
            Event::AlbumReplaced { moves, .. } => (
                moves.iter().filter_map(|m| m.track).collect(),
                moves.iter().filter_map(|m| m.track.map(|t| (m.was.unwrap_or(t), t))).chain(moves.iter().filter_map(|m| m.track.map(|t| (t, t)))).collect(),
            ),
            _ => (Vec::new(), Vec::new()),
        };
        let mut playing = self.playing_among(&order, &before).await;
        if let Some(p) = playing.as_mut() {
            self.hold(p).await;
        }
        let steps = self.steps(&event).ok_or_else(|| ToggleError::Refused("the folders are not in use any more".into()))?;
        let files = self.files.clone();
        let moved = tokio::task::spawn_blocking(move || files.move_all(&steps)).await.map_err(|e| anyhow::anyhow!("{e}"));
        if let Err(e) = moved.and_then(|r| r) {
            self.journal.forget(id)?;
            return Err(ToggleError::Refused(format!("{e:#}")));
        }
        self.finish_replace(&event, false, playing.is_some()).await;
        self.journal.applied(id)?;
        self.hidden.lock().await.insert(at.album.id.clone());
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
        let Event::AlbumReplaced { album_id, title, .. } = &entry.event else { unreachable!() };
        if entry.undone {
            return Err(ToggleError::Refused(format!("{title} is already back in Incoming")));
        }
        let steps = self.undo_steps(&entry.event).ok_or_else(|| ToggleError::Refused("the folders are not in use any more".into()))?;
        let files = self.files.clone();
        tokio::task::spawn_blocking(move || files.move_all(&steps))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
        self.finish_replace(&entry.event, true, false).await;
        self.unconvert_rows(&entry.event).await;
        self.journal.mark_undone(entry.id, epoch_now())?;
        self.resume_fetching(entry.id, title).await;
        self.hidden.lock().await.remove(album_id);
        *self.owned.lock().await = None;
        self.lib.catalogue().poke();
        Ok(title.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::Catalogue;
    use crate::moves::Step;

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
