//! What an incoming album is, compared with the library: new, an upgrade of an album already there, or a duplicate of
//! one. Pure logic over plain descriptions of albums; where the descriptions come from (the catalogue, the source) is
//! the caller's business.

use std::collections::HashMap;

use serde::Serialize;

use crate::albums::fold;

/// An album as far as the comparison needs it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AlbumFacts {
    /// MusicBrainz release group, when the files carry one: the most reliable way to recognise the same album.
    pub release_group: Option<String>,
    pub artist: String,
    pub title: String,
    pub tracks: u32,
    pub quality: Quality,
}

/// How good the files are, as far as an upgrade is concerned. Lossless beats any lossy; between lossy files the lowest
/// track bitrate counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Quality {
    #[default]
    Unknown,
    Lossy { kbps: u32 },
    Lossless,
}

/// A lossy album counts as better only from this many kbps more (320 against 321 is the same thing).
const BITRATE_STEP: u32 = 32;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Verdict {
    /// Not in the library.
    New,
    /// In the library, and the incoming copy is better: worth replacing.
    Upgrade { library: usize, why: String },
    /// In the library, and the incoming copy is not better.
    Duplicate { library: usize, why: String },
}

/// Compare one incoming album with the library albums (`library[i]`, referred to by index in the verdict). The
/// library's index (below) does this for real; this plain version is what the tests hold it to.
#[cfg(test)]
pub fn classify(incoming: &AlbumFacts, library: &[AlbumFacts]) -> Verdict {
    let Some(i) = find_same(incoming, library) else { return Verdict::New };
    let owned = &library[i];
    match better(incoming.quality, owned.quality) {
        Some(why) => Verdict::Upgrade { library: i, why },
        None => Verdict::Duplicate { library: i, why: describe_same(incoming, owned) },
    }
}

/// The library album that is the same album: by release group first, else by artist and title with case, accents
/// and punctuation folded away (as the Albums comparison does). `None` when nothing matches. (The tests' plain
/// version, as `classify`.)
#[cfg(test)]
pub fn find_same(incoming: &AlbumFacts, library: &[AlbumFacts]) -> Option<usize> {
    if let Some(rg) = incoming.release_group.as_deref().filter(|r| !r.is_empty()) {
        if let Some(i) = library.iter().position(|l| l.release_group.as_deref() == Some(rg)) {
            return Some(i);
        }
    }
    let key = (fold(&incoming.artist), fold(&incoming.title));
    if key.0.is_empty() || key.1.is_empty() {
        return None;
    }
    library.iter().position(|l| (fold(&l.artist), fold(&l.title)) == key)
}

/// The library's albums, looked up by release group and by folded artist + title. Built once per catalogue refresh, so
/// classifying a whole Incoming list is cheap.
pub struct LibraryIndex {
    /// (album directory in the library, facts).
    pub albums: Vec<(String, AlbumFacts)>,
    by_release_group: HashMap<String, usize>,
    by_name: HashMap<(String, String), usize>,
}

impl LibraryIndex {
    pub fn new(albums: Vec<(String, AlbumFacts)>) -> Self {
        let mut by_release_group = HashMap::new();
        let mut by_name = HashMap::new();
        for (i, (_, f)) in albums.iter().enumerate() {
            if let Some(rg) = f.release_group.as_deref().filter(|r| !r.is_empty()) {
                by_release_group.entry(rg.to_string()).or_insert(i);
            }
            let key = (fold(&f.artist), fold(&f.title));
            if !key.0.is_empty() && !key.1.is_empty() {
                by_name.entry(key).or_insert(i);
            }
        }
        Self { albums, by_release_group, by_name }
    }

    /// As `classify`, against the whole library.
    pub fn classify(&self, incoming: &AlbumFacts) -> Verdict {
        let by_rg = incoming.release_group.as_deref().filter(|r| !r.is_empty()).and_then(|r| self.by_release_group.get(r));
        let by_name = || self.by_name.get(&(fold(&incoming.artist), fold(&incoming.title)));
        let Some(&i) = by_rg.or_else(by_name) else { return Verdict::New };
        let owned = &self.albums[i].1;
        match better(incoming.quality, owned.quality) {
            Some(why) => Verdict::Upgrade { library: i, why },
            None => Verdict::Duplicate { library: i, why: describe_same(incoming, owned) },
        }
    }
}

/// The quality of a set of tracks from their formats and lowest bitrate: lossless only when every track is.
pub fn quality_of<'a>(formats: impl IntoIterator<Item = &'a str>, min_kbps: Option<u32>) -> Quality {
    let formats: Vec<String> = formats.into_iter().map(str::to_uppercase).collect();
    let lossless = |f: &str| matches!(f, "FLAC" | "ALAC" | "WAV" | "AIFF" | "APE" | "WV");
    if formats.is_empty() {
        Quality::Unknown
    } else if formats.iter().all(|f| lossless(f)) {
        Quality::Lossless
    } else {
        min_kbps.map_or(Quality::Unknown, |kbps| Quality::Lossy { kbps })
    }
}

/// Why `new` is better than `old`, or `None` when it is not.
fn better(new: Quality, old: Quality) -> Option<String> {
    match (new, old) {
        (Quality::Lossless, Quality::Lossy { kbps }) => Some(format!("lossless, yours is {kbps} kbps")),
        (Quality::Lossless, Quality::Unknown) => Some("lossless".into()),
        (Quality::Lossy { kbps: n }, Quality::Lossy { kbps: o }) if n >= o + BITRATE_STEP => Some(format!("{n} kbps, yours is {o} kbps")),
        _ => None,
    }
}

fn describe_same(new: &AlbumFacts, old: &AlbumFacts) -> String {
    let q = |q: Quality| match q {
        Quality::Lossless => "lossless".to_string(),
        Quality::Lossy { kbps } => format!("{kbps} kbps"),
        Quality::Unknown => "unknown quality".to_string(),
    };
    let tracks = if new.tracks != old.tracks { format!(", {} tracks against your {}", new.tracks, old.tracks) } else { String::new() };
    if new.quality == old.quality {
        return format!("{}, same as yours{tracks}", q(new.quality));
    }
    format!("{}, yours is {}{tracks}", q(new.quality), q(old.quality))
}

/// One track as Replace pairs them: the library album's tracks against the incoming album's files (decision 0011 §4).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackFacts {
    /// MusicBrainz recording id (Picard's `musicbrainz_trackid`).
    pub recording: Option<String>,
    /// `None` counts as disc 1.
    pub disc: Option<u32>,
    pub number: Option<u32>,
    pub title: String,
    pub duration_ms: Option<u64>,
}

/// Which incoming file takes over which library track. A paired file inherits the old track's identity, so playlists
/// and play data follow it; indexes refer to the two slices given to `pair`, in ascending order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pairing {
    /// (old track, new file).
    pub pairs: Vec<(usize, usize)>,
    /// New files with no old track: new tracks.
    pub added: Vec<usize>,
    /// Old tracks with no new file: missing from now on (Repairs shows them).
    pub retired: Vec<usize>,
}

/// Same title, different place on the album: the lengths must agree this closely.
const SAME_TITLE_MS: u64 = 3_000;
/// Same place, different title (a typo, garbled characters): the lengths must agree this closely.
const SAME_PLACE_MS: u64 = 2_000;

/// Pair an album's old tracks with its new files, surest evidence first: the MusicBrainz recording; then the same disc
/// and number with the same title; then the same title with the same length (another numbering); then, only when both
/// albums have the same number of tracks, the same disc and number with the same length. A pass pairs two tracks only
/// when each is the other's single candidate, so nothing is guessed: what is left stays unpaired.
pub fn pair(old: &[TrackFacts], new: &[TrackFacts]) -> Pairing {
    let close = |a: Option<u64>, b: Option<u64>, ms: u64| matches!((a, b), (Some(a), Some(b)) if a.abs_diff(b) <= ms);
    let place = |t: &TrackFacts| t.number.map(|n| (t.disc.unwrap_or(1), n));
    let same_place = |o: &TrackFacts, n: &TrackFacts| place(o).is_some() && place(o) == place(n);
    let folded_old: Vec<String> = old.iter().map(|t| fold(&t.title)).collect();
    let folded_new: Vec<String> = new.iter().map(|t| fold(&t.title)).collect();
    let same_title = |o: usize, n: usize| !folded_old[o].is_empty() && folded_old[o] == folded_new[n];
    let same_count = old.len() == new.len();

    let passes: [&dyn Fn(usize, usize) -> bool; 4] = [
        &|o, n| matches!((&old[o].recording, &new[n].recording), (Some(a), Some(b)) if !a.is_empty() && a == b),
        &|o, n| same_place(&old[o], &new[n]) && same_title(o, n),
        &|o, n| same_title(o, n) && close(old[o].duration_ms, new[n].duration_ms, SAME_TITLE_MS),
        &|o, n| same_count && same_place(&old[o], &new[n]) && close(old[o].duration_ms, new[n].duration_ms, SAME_PLACE_MS),
    ];
    let mut old_taken = vec![false; old.len()];
    let mut new_taken = vec![false; new.len()];
    let mut pairs = Vec::new();
    for matches in passes {
        let candidates: Vec<Vec<usize>> = (0..old.len())
            .map(|o| if old_taken[o] { Vec::new() } else { (0..new.len()).filter(|&n| !new_taken[n] && matches(o, n)).collect() })
            .collect();
        for o in 0..old.len() {
            let [n] = candidates[o][..] else { continue };
            let wanted_by = candidates.iter().filter(|c| c.contains(&n)).count();
            if wanted_by == 1 {
                old_taken[o] = true;
                new_taken[n] = true;
                pairs.push((o, n));
            }
        }
    }
    pairs.sort_unstable();
    Pairing {
        pairs,
        added: (0..new.len()).filter(|&n| !new_taken[n]).collect(),
        retired: (0..old.len()).filter(|&o| !old_taken[o]).collect(),
    }
}

impl TrackFacts {
    /// From a file's tags and length.
    pub fn of(tags: &crate::naming::Tags, duration_ms: Option<u64>) -> Self {
        let number = |name: &str| tags.first(name).and_then(|v| v.split('/').next()?.trim().parse().ok());
        TrackFacts {
            recording: tags.first("musicbrainz_trackid").map(str::to_string),
            disc: number("discnumber"),
            number: number("tracknumber"),
            title: tags.first("title").unwrap_or_default().to_string(),
            duration_ms,
        }
    }
}

/// A music file of an album as Replace reads it: its path inside the album folder, its tags and its length.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AlbumFile {
    pub rel: String,
    pub tags: crate::naming::Tags,
    pub duration_ms: Option<u64>,
}

/// What Replace will do (decision 0013), worked out before anything moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacePlan {
    /// The new album's folder, relative to Music.
    pub folder: String,
    /// The new album's files: path inside its Incoming folder → path relative to Music (music files first).
    pub moves: Vec<(String, String)>,
    /// (old file inside the old album's folder, new file inside the Incoming album's folder): the new one takes over
    /// the old one's track.
    pub takes_over: Vec<(String, String)>,
    /// New tracks without an old one (bonus tracks).
    pub added: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum ReplaceRefusal {
    /// Neither album's tags give a year: the page asks for it.
    #[error("the album's year is not known")]
    NeedsYear,
    /// Old tracks the new album has no match for, by title: Replace would lose them.
    #[error("no match for {}", .0.join(", "))]
    Unmatched(Vec<String>),
    #[error("{0}")]
    Names(String),
    #[error("Cratarr knows no music files in this album yet")]
    Empty,
}

/// Plan Replace: pair the old album's tracks with the new album's files (refused when an old track has no match),
/// and name the new files by the library's template. A year missing from the new tags comes from `year`, else from
/// the old album's tags.
pub fn plan_replace(
    template: &crate::naming::Template,
    old: &[AlbumFile],
    new: &[AlbumFile],
    new_others: &[String],
    year: Option<&str>,
) -> Result<ReplacePlan, ReplaceRefusal> {
    if new.is_empty() {
        return Err(ReplaceRefusal::Empty);
    }
    let facts = |files: &[AlbumFile]| files.iter().map(|f| TrackFacts::of(&f.tags, f.duration_ms)).collect::<Vec<_>>();
    let (old_facts, new_facts) = (facts(old), facts(new));
    let pairing = pair(&old_facts, &new_facts);
    if !pairing.retired.is_empty() {
        let title = |i: usize| match old_facts[i].title.as_str() {
            "" => old[i].rel.rsplit('/').next().unwrap_or(&old[i].rel).to_string(),
            t => t.to_string(),
        };
        return Err(ReplaceRefusal::Unmatched(pairing.retired.iter().map(|&i| title(i)).collect()));
    }
    let old_year = old.iter().find_map(|f| f.tags.release_year());
    let year = year.map(str::to_string).or(old_year);
    let music: Vec<(String, crate::naming::Tags)> = new.iter().map(|f| (f.rel.clone(), f.tags.clone())).collect();
    let keep = plan_keep(template, &music, new_others, year.as_deref()).map_err(|e| match e {
        KeepRefusal::NeedsYear => ReplaceRefusal::NeedsYear,
        KeepRefusal::Names(n) => ReplaceRefusal::Names(n),
        KeepRefusal::Empty => ReplaceRefusal::Empty,
    })?;
    Ok(ReplacePlan {
        folder: keep.folder,
        moves: keep.moves,
        takes_over: pairing.pairs.iter().map(|&(o, n)| (old[o].rel.clone(), new[n].rel.clone())).collect(),
        added: pairing.added.len(),
    })
}

/// What Keep will do with an album (decision 0013), worked out before anything moves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeepPlan {
    /// The album's new folder, relative to Music.
    pub folder: String,
    /// (path relative to the album's Incoming folder → path relative to Music), music files first, in their order.
    pub moves: Vec<(String, String)>,
}

/// Why Keep cannot name an album yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum KeepRefusal {
    /// No year in the tags: the page asks for it, and Keep tries again with it.
    #[error("the album's year is not known")]
    NeedsYear,
    /// The tags cannot give every file its own name (a track number missing, two tracks named alike…).
    #[error("{0}")]
    Names(String),
    #[error("Cratarr knows no music files in this album yet")]
    Empty,
}

/// Name an album's files for Music: its music files (path inside the album folder, tags) by the library's template,
/// its other files (a cover, a booklet) keeping their place inside the new folder. `year`, when given, stands in for a
/// missing (original) release year.
pub fn plan_keep(
    template: &crate::naming::Template,
    music: &[(String, crate::naming::Tags)],
    others: &[String],
    year: Option<&str>,
) -> Result<KeepPlan, KeepRefusal> {
    use crate::naming::{RenderError, Track};
    if music.is_empty() {
        return Err(KeepRefusal::Empty);
    }
    let year = year.map(str::trim).filter(|y| y.len() == 4 && y.bytes().all(|b| b.is_ascii_digit()));
    let tracks: Vec<Track> = music
        .iter()
        .map(|(rel, tags)| {
            let mut tags = tags.clone();
            if let Some(y) = year {
                let has = |name: &str| tags.0.iter().any(|(n, v)| n.eq_ignore_ascii_case(name) && !v.trim().is_empty());
                if !has("originaldate") && !has("date") {
                    tags.0.push(("originaldate".into(), y.into()));
                }
            }
            Track { file: rel.rsplit('/').next().unwrap_or(rel).to_string(), tags }
        })
        .collect();
    let names = template.render(&tracks).map_err(|e| match e {
        RenderError::Missing { token: "originalyear" | "year", .. } => KeepRefusal::NeedsYear,
        e => KeepRefusal::Names(e.to_string()),
    })?;
    let mut moves: Vec<(String, String)> =
        music.iter().zip(&names.files).map(|((rel, _), name)| (rel.clone(), format!("{}/{name}", names.folder))).collect();
    for rel in others {
        let to = format!("{}/{rel}", names.folder);
        if moves.iter().any(|(_, t)| t.eq_ignore_ascii_case(&to)) {
            return Err(KeepRefusal::Names(format!("{rel} would take the name of a music file")));
        }
        moves.push((rel.clone(), to));
    }
    Ok(KeepPlan { folder: names.folder, moves })
}

/// Two folder names that differ only in capitals or accents: one folder on a disk that ignores case.
fn spelling_key(name: &str) -> String {
    name.chars().flat_map(char::to_lowercase).map(crate::albums::unaccented).collect()
}

/// The plan with each of its folders spelled as Music already spells it (decision 0020): Lidarr's "DARKSIDE/Spiral
/// (2014)" goes into the "Darkside/" that is there, instead of beside it. `existing(parent)` lists the folder names in
/// `parent` ("" for Music itself). A folder Music has under one other spelling only is taken; with none, or several,
/// the rendered name stays.
pub fn follow_spelling(plan: KeepPlan, existing: impl Fn(&str) -> Vec<String>) -> KeepPlan {
    let mut folder = String::new();
    for part in plan.folder.split('/') {
        let names = existing(&folder);
        let key = spelling_key(part);
        let mut same = names.iter().filter(|n| spelling_key(n) == key);
        let name = match (same.next(), same.next()) {
            _ if names.iter().any(|n| n == part) => part,
            (Some(only), None) => only.as_str(),
            _ => part,
        };
        if !folder.is_empty() {
            folder.push('/');
        }
        folder.push_str(name);
    }
    if folder == plan.folder {
        return plan;
    }
    let old = format!("{}/", plan.folder);
    let moves = plan.moves.into_iter().map(|(from, to)| (from, to.strip_prefix(&old).map_or(to.clone(), |rest| format!("{folder}/{rest}")))).collect();
    KeepPlan { folder, moves }
}

/// The folder names in `music/parent`, for `follow_spelling`. Hidden folders are not candidates.
pub fn folders_in(music: &std::path::Path, parent: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(music.join(parent)) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| !n.starts_with('.'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(pairs: &[(&str, &str)]) -> crate::naming::Tags {
        crate::naming::Tags(pairs.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect())
    }

    fn darkside_plan() -> KeepPlan {
        KeepPlan {
            folder: "DARKSIDE/Spiral (2014)".into(),
            moves: vec![
                ("01 Gone Too Soon.flac".into(), "DARKSIDE/Spiral (2014)/01 DARKSIDE - Gone Too Soon.flac".into()),
                ("cover.jpg".into(), "DARKSIDE/Spiral (2014)/cover.jpg".into()),
            ],
        }
    }

    #[test]
    fn an_album_goes_into_the_artist_folder_music_already_has() {
        let music = |parent: &str| match parent {
            "" => vec!["Darkside".to_string(), "Air".into()],
            _ => vec!["Psychic (2013)".to_string()],
        };
        let plan = follow_spelling(darkside_plan(), music);
        assert_eq!(plan.folder, "Darkside/Spiral (2014)");
        assert_eq!(plan.moves[0], ("01 Gone Too Soon.flac".to_string(), "Darkside/Spiral (2014)/01 DARKSIDE - Gone Too Soon.flac".to_string()));
        assert_eq!(plan.moves[1].1, "Darkside/Spiral (2014)/cover.jpg");
    }

    #[test]
    fn accents_count_as_the_same_spelling_and_a_case_twin_album_is_found() {
        let plan = KeepPlan { folder: "Andre Bratten/picture music (2022)".into(), moves: vec![] };
        let music = |parent: &str| match parent {
            "" => vec!["André Bratten".to_string()],
            _ => vec!["Picture Music (2022)".to_string()],
        };
        // The album folder follows too, so "Music already has a folder" refuses the twin.
        assert_eq!(follow_spelling(plan, music).folder, "André Bratten/Picture Music (2022)");
    }

    #[test]
    fn the_rendered_spelling_stays_when_music_has_it_none_or_several() {
        let exact = |p: &str| if p.is_empty() { vec!["DARKSIDE".to_string(), "Darkside".into()] } else { vec![] };
        assert_eq!(follow_spelling(darkside_plan(), exact), darkside_plan());
        let several = |p: &str| if p.is_empty() { vec!["DarkSide".to_string(), "Darkside".into()] } else { vec![] };
        assert_eq!(follow_spelling(darkside_plan(), several), darkside_plan());
        let other = |p: &str| if p.is_empty() { vec!["Dark Side".to_string()] } else { vec![] };
        assert_eq!(follow_spelling(darkside_plan(), other), darkside_plan(), "only capitals and accents, not spaces");
    }

    fn moon_safari(with_year: bool) -> Vec<(String, crate::naming::Tags)> {
        let base = |n: &str, t: &str| {
            let mut v = vec![("albumartist", "Air"), ("artist", "Air"), ("album", "Moon Safari"), ("tracknumber", n), ("title", t)];
            if with_year {
                v.push(("originaldate", "1998-01-16"));
            }
            tags(&v)
        };
        vec![("01 - Sexy Boy.flac".to_string(), base("1", "Sexy Boy")), ("02 - Talisman.flac".to_string(), base("2", "Talisman"))]
    }

    #[test]
    fn keep_names_music_by_the_template_and_keeps_other_files_in_place() {
        let plan = plan_keep(&Default::default(), &moon_safari(true), &["cover.jpg".to_string(), "Scans/back.jpg".to_string()], None).unwrap();
        assert_eq!(plan.folder, "Air - Moon Safari (1998)");
        assert_eq!(
            plan.moves,
            vec![
                ("01 - Sexy Boy.flac".to_string(), "Air - Moon Safari (1998)/01 Air - Sexy Boy.flac".to_string()),
                ("02 - Talisman.flac".to_string(), "Air - Moon Safari (1998)/02 Air - Talisman.flac".to_string()),
                ("cover.jpg".to_string(), "Air - Moon Safari (1998)/cover.jpg".to_string()),
                ("Scans/back.jpg".to_string(), "Air - Moon Safari (1998)/Scans/back.jpg".to_string()),
            ]
        );
    }

    #[test]
    fn a_missing_year_is_asked_for_and_then_used() {
        assert_eq!(plan_keep(&Default::default(), &moon_safari(false), &[], None), Err(KeepRefusal::NeedsYear));
        assert_eq!(plan_keep(&Default::default(), &moon_safari(false), &[], Some("98")), Err(KeepRefusal::NeedsYear), "a year has four digits");
        assert_eq!(plan_keep(&Default::default(), &moon_safari(false), &[], Some("1998")).unwrap().folder, "Air - Moon Safari (1998)");
    }

    #[test]
    fn names_that_clash_or_are_missing_refuse_keep() {
        let mut twice = moon_safari(true);
        twice[1].1 = twice[0].1.clone();
        assert!(matches!(plan_keep(&Default::default(), &twice, &[], None), Err(KeepRefusal::Names(_))));
        assert_eq!(plan_keep(&Default::default(), &[], &["cover.jpg".to_string()], None), Err(KeepRefusal::Empty));
    }

    fn track(number: u32, title: &str, secs: u64) -> TrackFacts {
        TrackFacts { recording: None, disc: None, number: Some(number), title: title.into(), duration_ms: Some(secs * 1000) }
    }

    #[test]
    fn the_same_album_in_another_format_pairs_track_by_track() {
        let old = vec![track(1, "Sexy Boy", 298), track(2, "Kelly Watch the Stars", 225), track(3, "Talisman", 256)];
        // FLAC lengths differ slightly from the MP3's; titles differ in case and punctuation.
        let new = vec![track(1, "Sexy boy", 297), track(2, "Kelly Watch The Stars!", 226), track(3, "Talisman", 256)];
        assert_eq!(pair(&old, &new), Pairing { pairs: vec![(0, 0), (1, 1), (2, 2)], added: vec![], retired: vec![] });
    }

    #[test]
    fn recordings_win_over_numbers() {
        let mut old = vec![track(1, "A", 100), track(2, "B", 200)];
        let mut new = vec![track(1, "B", 200), track(2, "A", 100)];
        old[0].recording = Some("rec-a".into());
        new[1].recording = Some("rec-a".into());
        let p = pair(&old, &new);
        assert_eq!(p.pairs, vec![(0, 1), (1, 0)], "A by recording, B by title and length");
    }

    #[test]
    fn another_numbering_pairs_by_title_and_length_and_a_bonus_track_is_added() {
        let old = vec![track(1, "Intro", 60), track(2, "Song", 240)];
        let new = vec![track(1, "Bonus Demo", 180), track(2, "Intro", 61), track(3, "Song", 239)];
        assert_eq!(pair(&old, &new), Pairing { pairs: vec![(0, 1), (1, 2)], added: vec![0], retired: vec![] });
    }

    #[test]
    fn a_garbled_title_pairs_by_place_and_length_only_when_the_track_lists_match() {
        let old = vec![track(1, "CafÃ© Del Mar", 300), track(2, "Second", 200)];
        let new = vec![track(1, "Café Del Mar", 301), track(2, "Second", 200)];
        assert_eq!(pair(&old, &new).pairs, vec![(0, 0), (1, 1)]);
        let longer = vec![track(1, "Café Del Mar", 301), track(2, "Second", 200), track(3, "Third", 100)];
        let p = pair(&old, &longer);
        assert_eq!((p.pairs, p.added, p.retired), (vec![(1, 1)], vec![0, 2], vec![0]), "other track lists: no guessing by place");
    }

    #[test]
    fn ambiguity_is_never_guessed() {
        // Two interludes of nearly the same length, renumbered: either could be either.
        let old = vec![track(3, "Interlude", 60), track(7, "Interlude", 61)];
        let new = vec![track(4, "Interlude", 60), track(8, "Interlude", 61)];
        assert_eq!(pair(&old, &new), Pairing { pairs: vec![], added: vec![0, 1], retired: vec![0, 1] });
    }

    #[test]
    fn a_track_gone_from_the_new_album_is_retired_and_discs_count() {
        let mut old = vec![track(1, "One", 100), track(1, "Other", 150)];
        old[1].disc = Some(2);
        let mut new = vec![track(1, "Other", 150)];
        new[0].disc = Some(2);
        assert_eq!(pair(&old, &new), Pairing { pairs: vec![(1, 0)], added: vec![], retired: vec![0] });
    }

    fn album(artist: &str, title: &str, rg: Option<&str>, tracks: u32, quality: Quality) -> AlbumFacts {
        AlbumFacts { release_group: rg.map(str::to_string), artist: artist.into(), title: title.into(), tracks, quality }
    }

    fn library() -> Vec<AlbumFacts> {
        vec![
            album("AIR", "Moon Safari", Some("rg-moon"), 10, Quality::Lossy { kbps: 320 }),
            album("9 Lazy 9", "Paradise Blown", None, 17, Quality::Lossless),
            album("Mélanie", "Cri D'Amour", None, 13, Quality::Lossy { kbps: 320 }),
        ]
    }

    #[test]
    fn a_lossless_copy_of_an_owned_mp3_album_is_an_upgrade() {
        let v = classify(&album("Air", "Moon Safari", Some("rg-moon"), 10, Quality::Lossless), &library());
        assert_eq!(v, Verdict::Upgrade { library: 0, why: "lossless, yours is 320 kbps".into() });
    }

    #[test]
    fn the_same_quality_again_is_a_duplicate() {
        let v = classify(&album("9 Lazy 9", "Paradise Blown", None, 17, Quality::Lossless), &library());
        assert_eq!(v, Verdict::Duplicate { library: 1, why: "lossless, same as yours".into() });
        let mp3 = classify(&album("AIR", "Moon Safari", Some("rg-moon"), 10, Quality::Lossy { kbps: 321 }), &library());
        assert!(matches!(mp3, Verdict::Duplicate { library: 0, .. }), "321 against 320 is no upgrade: {mp3:?}");
    }

    #[test]
    fn a_weaker_copy_is_a_duplicate_and_says_why() {
        let v = classify(&album("Melanie", "Cri d'Amour", None, 12, Quality::Lossy { kbps: 192 }), &library());
        assert_eq!(v, Verdict::Duplicate { library: 2, why: "192 kbps, yours is 320 kbps, 12 tracks against your 13".into() });
    }

    #[test]
    fn names_match_without_case_accents_or_punctuation_but_release_groups_win() {
        assert_eq!(find_same(&album("air", "moon safari!", None, 10, Quality::Unknown), &library()), Some(0));
        let renamed = album("AIR", "Moon Safari (Deluxe)", Some("rg-moon"), 12, Quality::Lossless);
        assert_eq!(find_same(&renamed, &library()), Some(0), "same release group, different title");
    }

    #[test]
    fn the_index_classifies_like_the_plain_function() {
        let lib = library();
        let index = LibraryIndex::new(lib.iter().cloned().enumerate().map(|(i, f)| (format!("dir{i}"), f)).collect());
        for probe in [
            album("Air", "Moon Safari", Some("rg-moon"), 10, Quality::Lossless),
            album("9 Lazy 9", "Paradise Blown", None, 17, Quality::Lossless),
            album("Melanie", "Cri d'Amour", None, 12, Quality::Lossy { kbps: 192 }),
            album("Jonti", "Twirligig", Some("rg-jonti"), 14, Quality::Lossless),
        ] {
            assert_eq!(index.classify(&probe), classify(&probe, &lib), "{probe:?}");
        }
    }

    #[test]
    fn quality_is_lossless_only_when_every_track_is() {
        assert_eq!(quality_of(["FLAC", "flac"], Some(900)), Quality::Lossless);
        assert_eq!(quality_of(["FLAC", "MP3"], Some(192)), Quality::Lossy { kbps: 192 });
        assert_eq!(quality_of(["MP3"], None), Quality::Unknown);
        assert_eq!(quality_of([], Some(320)), Quality::Unknown);
    }

    #[test]
    fn anything_else_is_new() {
        assert_eq!(classify(&album("Jonti", "Twirligig", Some("rg-jonti"), 14, Quality::Lossless), &library()), Verdict::New);
        assert_eq!(classify(&album("", "", None, 0, Quality::Unknown), &library()), Verdict::New, "no names: no match");
    }

    fn file(rel: &str, n: &str, title: &str, ms: u64, extra: &[(&str, &str)]) -> AlbumFile {
        let mut v = vec![("albumartist", "Air"), ("artist", "Air"), ("album", "Moon Safari"), ("tracknumber", n), ("title", title)];
        v.extend_from_slice(extra);
        AlbumFile { rel: rel.into(), tags: tags(&v), duration_ms: Some(ms) }
    }

    #[test]
    fn replace_pairs_every_old_track_and_names_the_new_files_with_the_old_albums_year() {
        let old = vec![
            file("01 Air - Sexy Boy.mp3", "1", "Sexy Boy", 298_000, &[("originaldate", "1998")]),
            file("02 Air - Talisman.mp3", "2", "Talisman", 256_000, &[("originaldate", "1998")]),
        ];
        let new = vec![
            file("01 - Sexy Boy.flac", "1", "Sexy Boy", 298_400, &[]),
            file("02 - Talisman.flac", "2", "Talisman", 256_100, &[]),
            file("03 - Bonus.flac", "3", "Bonus", 100_000, &[]),
        ];
        let plan = plan_replace(&Default::default(), &old, &new, &["cover.jpg".into()], None).unwrap();
        assert_eq!(plan.folder, "Air - Moon Safari (1998)");
        assert_eq!(
            plan.takes_over,
            vec![
                ("01 Air - Sexy Boy.mp3".to_string(), "01 - Sexy Boy.flac".to_string()),
                ("02 Air - Talisman.mp3".to_string(), "02 - Talisman.flac".to_string()),
            ]
        );
        assert_eq!(plan.added, 1);
        assert_eq!(plan.moves.len(), 4);
        assert_eq!(plan.moves[2], ("03 - Bonus.flac".to_string(), "Air - Moon Safari (1998)/03 Air - Bonus.flac".to_string()));
    }

    #[test]
    fn replace_is_refused_when_an_old_track_would_be_lost() {
        let old = vec![file("01.mp3", "1", "Sexy Boy", 298_000, &[]), file("02.mp3", "2", "Talisman", 256_000, &[])];
        let new = vec![file("01.flac", "1", "Sexy Boy", 298_000, &[("date", "1998")])];
        assert_eq!(plan_replace(&Default::default(), &old, &new, &[], None), Err(ReplaceRefusal::Unmatched(vec!["Talisman".into()])));
        assert_eq!(plan_replace(&Default::default(), &old, &[], &[], None), Err(ReplaceRefusal::Empty));
    }

    #[test]
    fn replace_asks_for_the_year_only_when_neither_album_has_one() {
        let old = vec![file("01.mp3", "1", "Sexy Boy", 298_000, &[])];
        let new = vec![file("01.flac", "1", "Sexy Boy", 298_000, &[])];
        assert_eq!(plan_replace(&Default::default(), &old, &new, &[], None), Err(ReplaceRefusal::NeedsYear));
        assert_eq!(plan_replace(&Default::default(), &old, &new, &[], Some("1998")).unwrap().folder, "Air - Moon Safari (1998)");
    }
}
