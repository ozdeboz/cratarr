//! Duplicate albums: the same release sitting in two folders (`Melanie - …` and `Mélanie - …`). This module finds
//! candidate folders by name and compares them. It only reads; nothing here changes the library.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

/// The audio files an album holds: the catalogue's list, so the two never disagree.
use crate::catalogue::walk::AUDIO;

/// Lower-case, accents folded to their base letter, everything but letters and digits dropped. Two folder names
/// that fold to the same key are duplicate candidates.
pub fn fold(name: &str) -> String {
    name.chars().flat_map(char::to_lowercase).filter_map(base_letter).collect()
}

fn base_letter(c: char) -> Option<char> {
    let c = unaccented(c);
    c.is_alphanumeric().then_some(c)
}

/// A lower-case letter with its accent taken off ('é' -> 'e'); anything else as it is.
pub fn unaccented(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ı' => 'i',
        'ł' => 'l',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
        'ř' => 'r',
        'ś' | 'š' | 'ş' => 's',
        'ť' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        c => c,
    }
}

/// Folder names that share a folded key, in groups of two or more. Hidden folders are never candidates.
pub fn candidate_groups(folders: &[String]) -> Vec<Vec<String>> {
    let mut by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in folders.iter().filter(|f| !f.starts_with('.')) {
        by_key.entry(fold(f)).or_default().push(f.clone());
    }
    by_key
        .into_values()
        .filter(|g| g.len() > 1)
        .map(|mut g| {
            g.sort();
            g
        })
        .collect()
}

/// The leading track number of a file name (`1-03 …` and `03 …` both give the track; the disc is kept apart).
pub fn track_key(file: &str) -> Option<String> {
    let head: String = file.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
    let head = head.trim_matches('-');
    (!head.is_empty() && head.chars().any(|c| c.is_ascii_digit())).then(|| head.trim_start_matches('0').to_string())
}

/// What Jellyfin knows about one track; every field is optional because Jellyfin may not have scanned it yet.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Known {
    pub seconds: Option<f64>,
    pub kbps: Option<u32>,
    pub plays: u32,
    pub favourite: bool,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct TrackView {
    pub file: String,
    pub key: Option<String>,
    pub bytes: u64,
    pub seconds: Option<f64>,
    pub kbps: Option<u32>,
    pub plays: u32,
    pub favourite: bool,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct AlbumView {
    pub folder: String,
    pub tracks: Vec<TrackView>,
    pub total_bytes: u64,
    pub cover_bytes: Option<u64>,
    pub plays: u32,
    pub favourites: u32,
    /// Playlist lines that point into this folder, per playlist.
    pub playlist_lines: Vec<(String, usize)>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Comparison {
    pub same_track_count: bool,
    /// Tracks that exist in both folders under the same track number.
    pub matched: usize,
    /// Of the matched tracks, how many have lengths within a second of each other.
    pub same_length: usize,
    /// Of the matched tracks, how many have identical file sizes.
    pub same_size: usize,
    /// "same", "similar" or "different"; wording for the person, not a promise of byte equality.
    pub verdict: &'static str,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Suggestion {
    pub keep: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AlbumGroup {
    pub albums: Vec<AlbumView>,
    /// Each later album against the first.
    pub comparisons: Vec<Comparison>,
    pub suggestion: Suggestion,
}

pub fn compare(a: &AlbumView, b: &AlbumView) -> Comparison {
    let by_key = |al: &AlbumView| -> BTreeMap<String, TrackView> {
        al.tracks.iter().filter_map(|t| t.key.clone().map(|k| (k, t.clone()))).collect()
    };
    let (ma, mb) = (by_key(a), by_key(b));
    let mut matched = 0;
    let mut same_length = 0;
    let mut same_size = 0;
    for (k, ta) in &ma {
        let Some(tb) = mb.get(k) else { continue };
        matched += 1;
        if matches!((ta.seconds, tb.seconds), (Some(x), Some(y)) if (x - y).abs() <= 1.0) {
            same_length += 1;
        }
        if ta.bytes == tb.bytes {
            same_size += 1;
        }
    }
    let same_track_count = a.tracks.len() == b.tracks.len();
    let verdict = if same_track_count && matched == a.tracks.len() && same_length == matched && matched > 0 {
        if same_size == matched { "same" } else { "similar" }
    } else if matched * 2 >= a.tracks.len().max(b.tracks.len()) && matched > 0 {
        "similar"
    } else {
        "different"
    };
    Comparison { same_track_count, matched, same_length, same_size, verdict }
}

/// Which folder to keep, and why: playlists referencing it, then listening history, then more tracks, then a bigger
/// cover. The first folder in name order wins a tie. These are hints; the person decides.
pub fn suggest(albums: &[AlbumView]) -> Suggestion {
    let score = |a: &AlbumView| {
        let refs: usize = a.playlist_lines.iter().map(|(_, n)| n).sum();
        (refs, a.plays, a.tracks.len(), a.cover_bytes.unwrap_or(0))
    };
    let best = albums.iter().enumerate().max_by_key(|(i, a)| (score(a), std::cmp::Reverse(*i))).map(|(_, a)| a);
    let Some(best) = best else {
        return Suggestion { keep: String::new(), reasons: vec![] };
    };
    let mut reasons = Vec::new();
    for other in albums.iter().filter(|a| a.folder != best.folder) {
        let (bs, os) = (score(best), score(other));
        if bs.0 > os.0 {
            reasons.push(format!("in {} playlist lines vs {}", bs.0, os.0));
        }
        if bs.1 > os.1 {
            reasons.push(format!("{} plays vs {}", bs.1, os.1));
        }
        if bs.2 > os.2 {
            reasons.push(format!("{} tracks vs {}", bs.2, os.2));
        }
        if bs.3 > os.3 {
            reasons.push(format!("larger cover ({} KB vs {} KB)", bs.3 / 1024, os.3 / 1024));
        }
    }
    reasons.dedup();
    Suggestion { keep: best.folder.clone(), reasons }
}

/// Audio files and the cover size of one album folder (blocking; run it off the async threads).
pub fn read_folder(music: &Path, folder: &str) -> Result<(Vec<(String, u64)>, Option<u64>)> {
    anyhow::ensure!(!folder.is_empty() && !folder.contains(['/', '\\', '\0']) && !folder.starts_with('.'), "bad folder name");
    let dir = music.join(folder);
    let mut files = Vec::new();
    let mut cover: Option<(usize, u64)> = None;
    for e in std::fs::read_dir(&dir).with_context(|| format!("cannot read {}", dir.display()))? {
        let e = e?;
        let meta = e.metadata()?;
        let Some(name) = e.file_name().to_str().map(str::to_string) else { continue };
        if !meta.is_file() {
            continue;
        }
        let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
        if AUDIO.contains(&ext.as_str()) {
            files.push((name, meta.len()));
        } else if let Some(rank) = crate::covers::candidate_rank(&name) {
            if cover.is_none_or(|(r, _)| rank < r) {
                cover = Some((rank, meta.len()));
            }
        }
    }
    files.sort();
    Ok((files, cover.map(|(_, len)| len)))
}

pub fn build_view(
    folder: &str,
    files: Vec<(String, u64)>,
    cover_bytes: Option<u64>,
    known: &BTreeMap<String, Known>,
    playlist_lines: Vec<(String, usize)>,
) -> AlbumView {
    let tracks: Vec<TrackView> = files
        .into_iter()
        .map(|(file, bytes)| {
            let k = known.get(&file).cloned().unwrap_or_default();
            TrackView { key: track_key(&file), file, bytes, seconds: k.seconds, kbps: k.kbps, plays: k.plays, favourite: k.favourite }
        })
        .collect();
    AlbumView {
        folder: folder.to_string(),
        total_bytes: tracks.iter().map(|t| t.bytes).sum(),
        plays: tracks.iter().map(|t| t.plays).sum(),
        favourites: tracks.iter().filter(|t| t.favourite).count() as u32,
        cover_bytes,
        playlist_lines,
        tracks,
    }
}

/// The album-title part of a `Artist - Album (Year)` folder name, used as a Jellyfin search term.
pub fn search_term(folder: &str) -> String {
    let after = folder.split_once(" - ").map_or(folder, |(_, rest)| rest);
    match after.rfind(" (") {
        Some(i) if after.ends_with(')') => after[..i].to_string(),
        _ => after.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(file: &str, bytes: u64, seconds: f64) -> TrackView {
        TrackView { file: file.into(), key: track_key(file), bytes, seconds: Some(seconds), kbps: Some(320), plays: 0, favourite: false }
    }

    fn album(folder: &str, tracks: Vec<TrackView>) -> AlbumView {
        AlbumView {
            folder: folder.into(),
            total_bytes: tracks.iter().map(|t| t.bytes).sum(),
            plays: 0,
            favourites: 0,
            cover_bytes: None,
            playlist_lines: vec![],
            tracks,
        }
    }

    #[test]
    fn accents_and_punctuation_fold_away() {
        assert_eq!(fold("Mélanie - Cri D'Amour (2022)"), fold("Melanie - Cri D'Amour (2022)"));
        assert_ne!(fold("Melanie De Biasio - Lilies (2017)"), fold("Melanie Charles - Lilies (2017)"));
    }

    #[test]
    fn only_folders_with_the_same_key_are_grouped() {
        let f = vec![
            "Mélanie - Cri D'Amour (2022)".to_string(),
            "Melanie - Cri D'Amour (2022)".to_string(),
            "Melanie De Biasio - Lilies (2017)".to_string(),
            ".hidden".to_string(),
        ];
        assert_eq!(candidate_groups(&f), vec![vec!["Melanie - Cri D'Amour (2022)".to_string(), "Mélanie - Cri D'Amour (2022)".to_string()]]);
    }

    #[test]
    fn track_keys_ignore_leading_zeros_and_keep_the_disc() {
        assert_eq!(track_key("03 Artist - Song.mp3").as_deref(), Some("3"));
        assert_eq!(track_key("1-03 Artist - Song.mp3").as_deref(), Some("1-03"));
        assert_eq!(track_key("Artist - Song.mp3"), None);
    }

    #[test]
    fn identical_lengths_with_different_sizes_are_similar_not_same() {
        let a = album("a", vec![t("01 x.mp3", 100, 200.0), t("02 y.mp3", 100, 180.0)]);
        let b = album("b", vec![t("01 x.mp3", 120, 200.4), t("02 y.mp3", 100, 180.0)]);
        let c = compare(&a, &b);
        assert_eq!((c.matched, c.same_length, c.same_size, c.verdict), (2, 2, 1, "similar"));
    }

    #[test]
    fn identical_sizes_and_lengths_are_the_same() {
        let a = album("a", vec![t("01 x.mp3", 100, 200.0)]);
        assert_eq!(compare(&a, &a.clone()).verdict, "same");
    }

    #[test]
    fn different_albums_are_different() {
        let a = album("a", vec![t("01 x.mp3", 100, 200.0), t("02 y.mp3", 100, 180.0)]);
        let b = album("b", vec![t("07 z.mp3", 100, 60.0)]);
        assert_eq!(compare(&a, &b).verdict, "different");
    }

    #[test]
    fn the_copy_with_more_plays_is_suggested() {
        let mut a = album("a", vec![t("01 x.mp3", 100, 200.0)]);
        let mut b = album("b", vec![t("01 x.mp3", 100, 200.0)]);
        a.plays = 18;
        b.plays = 102;
        let s = suggest(&[a, b]);
        assert_eq!(s.keep, "b");
        assert!(s.reasons.iter().any(|r| r.contains("102 plays vs 18")));
    }

    #[test]
    fn playlist_references_outweigh_plays() {
        let mut a = album("a", vec![t("01 x.mp3", 100, 200.0)]);
        let mut b = album("b", vec![t("01 x.mp3", 100, 200.0)]);
        a.playlist_lines = vec![("Chill".into(), 1)];
        b.plays = 500;
        assert_eq!(suggest(&[a, b]).keep, "a");
    }

    #[test]
    fn search_term_is_the_album_title() {
        assert_eq!(search_term("Mélanie - Cri D'Amour (2022)"), "Cri D'Amour");
        assert_eq!(search_term("Various - Dubbel-Dubb (2020)"), "Dubbel-Dubb");
        assert_eq!(search_term("No Dashes Here"), "No Dashes Here");
    }

    #[test]
    fn folder_names_cannot_escape_the_music_folder() {
        assert!(read_folder(Path::new("/nonexistent"), "../etc").is_err());
        assert!(read_folder(Path::new("/nonexistent"), "a/b").is_err());
    }
}
