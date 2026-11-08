//! Adjust BPM (the owner, 2026-10-23): an administrator halves or doubles the tempo of one track or a selection when
//! its measure came out at double or half (the classic: a hip-hop track at 168 instead of 84), or resets it to
//! Cratarr's own measure. The library's, for everyone: the corrected value is written into the file's BPM tag
//! (crate::retag, the audio untouched), and the tag is then simply the truth; nothing plays any differently.
//! Cratarr's own measure (the analysis, decision 0035) stays as it was, carried over to the rewritten file.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// What to do to the tracks' tempo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Adjust {
    Half,
    Double,
    /// To Cratarr's own measure.
    Reset,
}

/// One track's tempo as the sheet shows it: now (its tag, else the measure) and Cratarr's measure.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrackTempo {
    pub id: String,
    pub bpm: Option<u32>,
    pub measured: Option<u32>,
}

/// One track's tempo changed: its BPM tag before (none: it had none) and after, for the undo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TempoChange {
    pub source: String,
    pub id: String,
    pub before: Option<u32>,
    pub after: Option<u32>,
}

/// A track's file: its folder in the catalogue, its path there, and where it is.
pub(super) fn place(cat: &crate::catalogue::Catalogue, source: &str, id: &str) -> Option<(i64, String, PathBuf)> {
    let (folder, rel) = cat.place_of_source(source, id)?;
    let path = cat.folders().get(folder)?.join(&rel);
    Some((folder, rel, path))
}

/// A BPM as a whole number, as BPM tags are; a tag that is no number is none.
fn whole(v: Option<f64>) -> Option<u32> {
    v.filter(|b| b.is_finite() && *b > 0.0).map(|b| b.round() as u32)
}

/// A track's tempo: its tag's, else Cratarr's measure (as the library has it), and the measure.
pub(super) fn tempo_of(cat: &crate::catalogue::Catalogue, source: &str, id: &str) -> Option<TrackTempo> {
    let (folder, rel, path) = place(cat, source, id)?;
    let measured = whole(cat.analysis_of(folder, &rel).and_then(|a| a.bpm).map(f64::from));
    let tag = whole(crate::retag::bpm(&path).and_then(|v| v.trim().parse().ok()));
    Some(TrackTempo { id: id.to_string(), bpm: tag.or(measured), measured })
}

/// What a track's tempo becomes: none where there is nothing to go from (no tempo; no measure to reset to).
pub fn adjusted(t: &TrackTempo, how: Adjust) -> Option<u32> {
    match how {
        Adjust::Half => t.bpm.map(|b| (b as f64 / 2.0).round() as u32).filter(|b| *b > 0),
        Adjust::Double => t.bpm.map(|b| b * 2),
        Adjust::Reset => t.measured,
    }
}

/// The tracks' BPMs halved, doubled or reset as a run on the one path (decision 0092's writers' map, 0.14: the tap is
/// the yes; Undo is the run's): each of Cratarr's own tracks whose tag changes, its BPM now → after. With the tracks'
/// tempos as they will be. The tempo is the track's, so every copy's file gets it, each from what it holds itself. Blocking
/// for the plan; the run writes in the background.
pub(super) fn proposal(served: &crate::serve::Serve, ids: &[String], how: Adjust) -> Result<(super::file_runs::Proposal, Vec<TrackTempo>)> {
    let cat = served.catalogue();
    let own = crate::source::cratarr::KIND.id;
    let mut p = super::file_runs::Proposal { kind: "tempo".into(), ..Default::default() };
    let (mut tempos, mut tracks) = (Vec::new(), 0);
    for id in ids {
        let Some(now) = tempo_of(cat, own, id) else { continue };
        let Some(to) = adjusted(&now, how) else {
            tempos.push(now);
            continue;
        };
        tempos.push(TrackTempo { bpm: Some(to), ..now.clone() });
        let Some((_, _, album, title)) = served.library.song_file(id)? else { continue };
        let had_files = p.files.len();
        let from = match how {
            Adjust::Half => "halved",
            Adjust::Double => "doubled",
            Adjust::Reset => "Cratarr's measure",
        };
        for (folder, rel) in served.library.files_of_track(id)? {
            let path = cat.folders().get(folder).context("its library is not there")?.join(&rel);
            let had = crate::retag::bpm(&path);
            if whole(had.as_deref().and_then(|v| v.trim().parse().ok())) == Some(to) {
                continue;
            }
            if !p.records.iter().any(|r| r.album == album)
                && let Some((_, artist, name)) = served.library.album_names(&album)?
            {
                p.records.push(crate::journal::RunRecord { album: album.clone(), title: name, artist });
            }
            p.files.push(crate::journal::FileWrite {
                album: album.clone(),
                path,
                track: Some(title.clone()),
                before: vec![("bpm".into(), had.into_iter().collect())],
                after: vec![("bpm".into(), vec![to.to_string()])],
                from: vec![("bpm".into(), from.into())],
                undone: false,
                whole: None,
            });
        }
        tracks += usize::from(p.files.len() > had_files);
    }
    let n = tracks;
    let what = match how {
        Adjust::Half => "Tempo halved",
        Adjust::Double => "Tempo doubled",
        Adjust::Reset => "Tempo measured",
    };
    p.title = format!("{what} · {n} track{}", if n == 1 { "" } else { "s" });
    Ok((p, tempos))
}

/// Writes a track's BPM tag (none: removes it); the catalogue reads it again at once and carries Cratarr's analysis of
/// it over to the rewritten file, so its measure and beats stay (`Catalogue::reread`). Blocking.
pub(super) fn write(cat: &crate::catalogue::Catalogue, source: &str, id: &str, to: Option<u32>) -> Result<()> {
    let (_, _, path) = place(cat, source, id).context("the track's file is not known")?;
    crate::retag::set_bpm(&path, to)?;
    cat.reread(std::slice::from_ref(&path)).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halving_doubling_and_resetting() {
        let t = |bpm, measured| TrackTempo { id: "tr-1".into(), bpm, measured };
        assert_eq!(adjusted(&t(Some(168), Some(168)), Adjust::Half), Some(84));
        assert_eq!(adjusted(&t(Some(173), None), Adjust::Half), Some(87), "a whole number");
        assert_eq!(adjusted(&t(Some(84), Some(168)), Adjust::Double), Some(168));
        assert_eq!(adjusted(&t(Some(84), Some(168)), Adjust::Reset), Some(168), "to the measure");
        assert_eq!(adjusted(&t(Some(84), None), Adjust::Reset), None, "no measure, nothing to reset to");
        assert_eq!(adjusted(&t(None, None), Adjust::Double), None, "no tempo, nothing to go from");
    }
}
