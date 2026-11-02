//! Health's fixes and an arrival's intake on the one path (decision 0092; the writers' map, 0.14): what Cratarr proposes
//! is reviewed on the sheet, the ticked written as one run in Changes to files, its Undo the run's.
//!
//! - `fix:<finding>` (`fix:library:year`): a finding's fixes, each record's fields now → proposed, with the source.
//! - `intake:<album>`: every fix one record would get before it is kept.
//!
//! Each field says how sure it is (certain ones come ticked, likely ones not: the owner's choice 2, 2026-11-01); a genre
//! offers what was found to choose from. A cover and a file's length are not tags: they keep their own way until they
//! come onto the path.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::file_runs::{self, FieldOption, PlanView, Proposal};
use super::fixes::Fix;
use crate::journal::{FileWrite, RunRecord};

/// A finding's fixes (`fix:<finding>`).
pub const FIX: &str = "fix";
/// A record's fixes before it is kept (`intake:<album>`).
pub const INTAKE: &str = "intake";

/// What the sheet chose, besides the records ticked: per record the fields ticked (none named: every field), and a
/// genre's words chosen among those offered.
#[derive(Debug, Default, Deserialize)]
pub struct Chosen {
    #[serde(default)]
    pub fields: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub genres: HashMap<String, Vec<String>>,
}

/// Whether a kind of plan is one of these (`fix:…`, `intake:…`).
pub fn owns(kind: &str) -> bool {
    kind.split_once(':').is_some_and(|(k, rest)| (k == FIX || k == INTAKE) && !rest.is_empty())
}

/// The fix's tags, by the tag reader's names, and the field each belongs to on the sheet.
fn on_path(f: &Fix) -> bool {
    f.field != "cover" && !super::length_fixes::FIELDS.contains(&f.field)
}

/// The sheet's title: the finding's ("Missing years"), or the record's intake.
fn title_of(kind: &str) -> String {
    match kind.split_once(':') {
        Some((FIX, finding)) => super::health::name_of(finding).to_string(),
        _ => "Before Keep".to_string(),
    }
}

/// The fixes a plan proposes, and whether more are still being looked for (a match with MusicBrainz).
async fn fixes_of(app: &super::App, kind: &str) -> Result<(Vec<Fix>, bool)> {
    let (k, rest) = kind.split_once(':').context("Not a plan")?;
    let (fixes, making) = match k {
        FIX => {
            let making = matches!(rest, "library:album-artist" | "library:musicbrainz") && crate::serve::who::current().is_some_and(|w| super::matching::running(&w.user));
            (super::fixes::of_finding(app, rest).await?, making)
        }
        _ => super::fixes::of_album(app, rest).await?,
    };
    Ok((fixes.into_iter().filter(on_path).collect(), making))
}

/// What a plan's sheet shows of its fields besides old → new: how sure each is, and a genre's choices.
#[derive(Debug, Default)]
struct Notes {
    /// (album, tag) → certain or likely.
    certainty: HashMap<(String, String), &'static str>,
    /// album → a genre's choices.
    genres: HashMap<String, Vec<FieldOption>>,
}

/// The fixes as a proposal: each track's tags now (read again from its file) → what its fixes give, said where from.
/// Blocking.
fn proposal(kind: &str, fixes: &[Fix]) -> Result<(Proposal, Notes)> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let mut p = Proposal { kind: kind.to_string(), title: title_of(kind), ..Proposal::default() };
    let mut notes = Notes::default();
    let mut by_album: Vec<(String, Vec<&Fix>)> = Vec::new();
    for f in fixes {
        match by_album.iter_mut().find(|(a, _)| *a == f.album) {
            Some((_, v)) => v.push(f),
            None => by_album.push((f.album.clone(), vec![f])),
        }
    }
    for (album, fixes) in by_album {
        let Some((_, _, _, artist, title)) = served.library.album_place(&album)? else { continue };
        let names: HashMap<std::path::PathBuf, String> = super::renumber::tracks_of(&album).map(|t| t.0.into_iter().map(|t| (t.path, t.title)).collect()).unwrap_or_default();
        // Per file, every fix's tags, in the order the fixes come.
        let mut files: Vec<(std::path::PathBuf, Vec<(String, Vec<String>)>, Vec<(String, String)>)> = Vec::new();
        for f in &fixes {
            for (path, set) in &f.writes {
                let at = match files.iter().position(|(p, ..)| p == path) {
                    Some(i) => i,
                    None => {
                        files.push((path.clone(), Vec::new(), Vec::new()));
                        files.len() - 1
                    }
                };
                for (tag, value) in set {
                    let (_, after, from) = &mut files[at];
                    after.retain(|(n, _)| n != tag);
                    after.push((tag.to_string(), value.iter().cloned().collect()));
                    from.retain(|(n, _)| n != tag);
                    from.push((tag.to_string(), f.source.clone()));
                    notes.certainty.insert((album.clone(), tag.to_string()), f.certainty);
                }
            }
            if f.field == "genre" && !f.candidates.is_empty() {
                notes.genres.insert(
                    album.clone(),
                    f.candidates.iter().map(|c| FieldOption { value: c.word.clone(), kind: Some(c.kind), from: c.source.clone(), certainty: c.certainty }).collect(),
                );
            }
        }
        let mut any = false;
        for (path, after, from) in files {
            let tags: Vec<&str> = after.iter().map(|(n, _)| n.as_str()).collect();
            let now = file_runs::fields_of(&path, &tags)?;
            let before: Vec<(String, Vec<String>)> = after.iter().map(|(n, _)| (n.clone(), now.get(n).cloned().unwrap_or_default())).collect();
            // Only what differs from what the file holds.
            let differs: Vec<usize> = (0..after.len()).filter(|&i| file_runs::flat(&before[i].1) != file_runs::flat(&after[i].1)).collect();
            if differs.is_empty() {
                continue;
            }
            any = true;
            let pick = |v: &[(String, Vec<String>)]| differs.iter().map(|&i| v[i].clone()).collect::<Vec<_>>();
            let from = from.into_iter().filter(|(n, _)| differs.iter().any(|&i| after[i].0 == *n)).collect();
            let track = names.get(&path).cloned();
            p.files.push(FileWrite { album: album.clone(), path, track, before: pick(&before), after: pick(&after), from, undone: false });
        }
        if any {
            p.records.push(RunRecord { album, title, artist });
        }
    }
    Ok((p, notes))
}

/// The review sheet of a plan: its records' fields, how sure each is, a genre's choices; `making` while a match is still
/// looked for (the sheet asks again).
pub async fn plan(app: &super::App, kind: &str) -> Result<PlanView> {
    let (fixes, making) = fixes_of(app, kind).await?;
    let k = kind.to_string();
    let (p, notes) = tokio::task::spawn_blocking(move || proposal(&k, &fixes)).await??;
    let mut view = file_runs::view(&p.kind, &p.title, &p.records, &p.files, false);
    for r in &mut view.records {
        for f in &mut r.fields {
            f.certainty = notes.certainty.get(&(r.album.clone(), f.field.clone())).copied();
            if f.field == "genre" {
                f.options = notes.genres.get(&r.album).cloned().unwrap_or_default();
            }
        }
    }
    view.making = making;
    Ok(view)
}

/// The ticked records written (an administrator's yes on the sheet): only the fields ticked of each, a genre as chosen,
/// each record set aside where it is no longer what the sheet showed (its stamp). A run on the one path.
pub async fn apply(app: &Arc<super::App>, kind: &str, albums: Vec<String>, stamps: Option<HashMap<String, String>>, chosen: Chosen) -> Result<file_runs::Status> {
    anyhow::ensure!(!albums.is_empty(), "No records ticked");
    anyhow::ensure!(!file_runs::busy(kind), "Already writing");
    let (fixes, _) = fixes_of(app, kind).await?;
    // A genre's words, each among those its fix offered.
    for (album, words) in &chosen.genres {
        let offered: Vec<&str> =
            fixes.iter().filter(|f| f.album == *album && f.field == "genre").flat_map(|f| f.candidates.iter().map(|c| c.word.as_str()).chain([f.to.as_str()])).collect();
        if let Some(w) = words.iter().find(|w| !offered.iter().any(|o| o.eq_ignore_ascii_case(w.trim()))) {
            anyhow::bail!("{w} was not found for it");
        }
    }
    let k = kind.to_string();
    let p = tokio::task::spawn_blocking(move || -> Result<Proposal> {
        let (p, _) = proposal(&k, &fixes)?;
        let mut p = file_runs::ticked(p, &albums, stamps.as_ref());
        choose(&mut p, &chosen);
        Ok(p)
    })
    .await??;
    anyhow::ensure!(!p.files.is_empty() || !p.failed.is_empty(), "Nothing to fix: look again");
    Ok(file_runs::start(app, p))
}

/// The sheet's choices kept to: a record's fields ticked (the rest left as the file has them), a genre's chosen words.
fn choose(p: &mut Proposal, chosen: &Chosen) {
    for f in &mut p.files {
        if let Some(keep) = chosen.fields.get(&f.album) {
            let kept = |n: &String| keep.contains(n);
            f.before.retain(|(n, _)| kept(n));
            f.after.retain(|(n, _)| kept(n));
            f.from.retain(|(n, _)| kept(n));
        }
        if let Some(words) = chosen.genres.get(&f.album).filter(|w| !w.is_empty())
            && let Some((_, after)) = f.after.iter_mut().find(|(n, _)| n == "genre")
        {
            *after = words.iter().map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect();
        }
    }
    p.files.retain(|f| !f.after.is_empty());
    let left: Vec<&str> = p.files.iter().map(|f| f.album.as_str()).collect();
    p.records.retain(|r| left.contains(&r.album.as_str()) || p.failed.iter().any(|x| x.album == r.album));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn a_plan_is_named_by_its_kind() {
        assert!(owns("fix:library:year"));
        assert!(owns("intake:al-1"));
        assert!(!owns("fix:"));
        assert!(!owns("types"));
    }

    #[test]
    fn the_sheet_s_choices_are_what_is_written() {
        let file = |album: &str, after: Vec<(String, Vec<String>)>| FileWrite {
            album: album.into(),
            path: format!("/m/{album}.flac").into(),
            track: None,
            before: after.iter().map(|(n, _)| (n.clone(), Vec::new())).collect(),
            from: after.iter().map(|(n, _)| (n.clone(), "MusicBrainz".to_string())).collect(),
            after,
            undone: false,
        };
        let mut p = Proposal {
            kind: "intake:al-1".into(),
            title: "Before Keep".into(),
            records: vec![RunRecord { album: "al-1".into(), title: "A".into(), artist: "B".into() }, RunRecord { album: "al-2".into(), title: "C".into(), artist: "D".into() }],
            files: vec![
                file("al-1", vec![("date".into(), s(&["1998"])), ("genre".into(), s(&["Rock"]))]),
                file("al-2", vec![("date".into(), s(&["2001"]))]),
            ],
            failed: Vec::new(),
        };
        let chosen = Chosen {
            // al-1: only the genre, chosen as two; al-2: none of its fields, so it goes.
            fields: HashMap::from([("al-1".to_string(), s(&["genre"])), ("al-2".to_string(), Vec::new())]),
            genres: HashMap::from([("al-1".to_string(), s(&["Rock", " Krautrock "]))]),
        };
        choose(&mut p, &chosen);
        assert_eq!(p.files.len(), 1);
        assert_eq!(p.files[0].after, vec![("genre".to_string(), s(&["Rock", "Krautrock"]))]);
        assert_eq!(p.files[0].from, vec![("genre".to_string(), "MusicBrainz".to_string())]);
        assert_eq!(p.records.iter().map(|r| r.album.as_str()).collect::<Vec<_>>(), ["al-1"]);
    }
}
