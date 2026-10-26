//! Health's fixes (0.9.7, the owner: every metadata problem gets a proposed fix with its source; core facts are written
//! into the files after an administrator approves, never before): what a finding's album would get, from where, and
//! how sure that is. Applied one at a time or every certain one at once: written into the tracks' tags (the audio
//! untouched, through `retag::rewrite`'s checked copy), the values before kept for Undo, the album read again at once.
//!
//! - **Year**: MusicBrainz's first release date of the album's release (the original years kept, `originals`).
//! - **Genre**: the album's strongest genre among the library's words (`app::words`); certain where two sources say it.
//! - **Album artist, MusicBrainz ids**: from a match with MusicBrainz's releases (`app::matching`), certain where every
//!   track is within 2 s and has its title.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// What an undo token starts with.
pub const UNDO: char = 'f';

/// A fix as the page shows it, its JSON kept stable (the page builds on it).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fix {
    /// `<field>:<album>`: the same fix keeps its id while it stands.
    pub id: String,
    pub album: String,
    /// What it sets: year, genre, albumartist, musicbrainz.
    pub field: &'static str,
    /// What the album has now (none: nothing).
    pub from: Option<String>,
    pub to: String,
    /// Where it comes from ("MusicBrainz", "Discogs and Last.fm"…).
    pub source: String,
    /// certain (Apply all certain applies it) or likely (applied one by one).
    pub certainty: &'static str,
    pub why: String,
    /// What each track's tags get (the page has no need of it).
    #[serde(skip)]
    pub writes: Vec<(PathBuf, Vec<(&'static str, Option<String>)>)>,
}

/// The tags a fix may write, by their names as the tag reader names them.
fn item_key(name: &str) -> Option<lofty::tag::ItemKey> {
    use lofty::tag::ItemKey;
    Some(match name {
        "date" => ItemKey::RecordingDate,
        "genre" => ItemKey::Genre,
        "albumartist" => ItemKey::AlbumArtist,
        "musicbrainz_albumid" => ItemKey::MusicBrainzReleaseId,
        "musicbrainz_releasegroupid" => ItemKey::MusicBrainzReleaseGroupId,
        "musicbrainz_albumartistid" => ItemKey::MusicBrainzReleaseArtistId,
        "musicbrainz_artistid" => ItemKey::MusicBrainzArtistId,
        "musicbrainz_trackid" => ItemKey::MusicBrainzRecordingId,
        _ => return None,
    })
}

/// A track's tag values by name, as the tag reader reads them (the first of each).
fn values(track: &Path, names: &[&str]) -> Result<Vec<Option<String>>> {
    let (tags, _) = crate::catalogue::tags::read(track)?;
    Ok(names.iter().map(|n| tags.items.iter().find(|(_, name, _)| *name == Some(*n)).map(|(_, _, v)| v.clone())).collect())
}

/// Writes these values into a track (none: removes the tag), read back from the checked copy before it replaces it.
fn write(track: &Path, set: &[(&'static str, Option<String>)]) -> Result<()> {
    let names: Vec<&str> = set.iter().map(|(n, _)| *n).collect();
    let keys: Vec<(lofty::tag::ItemKey, Option<String>)> =
        set.iter().map(|(n, v)| Ok((item_key(n).with_context(|| format!("no tag {n}"))?, v.clone()))).collect::<Result<_>>()?;
    crate::retag::rewrite(
        track,
        |tag| {
            for (k, v) in &keys {
                tag.remove_key(k.clone());
                if let Some(v) = v {
                    tag.insert_text(k.clone(), v.clone());
                }
            }
        },
        |copy| {
            let got = values(copy, &names)?;
            for ((n, want), got) in set.iter().zip(got) {
                if got.as_deref().map(str::trim) != want.as_deref().map(str::trim) {
                    bail!("{n} read back as {got:?}, not {want:?}");
                }
            }
            Ok(())
        },
    )
}

/// What Undo needs: each track's values before, by tag name.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Before {
    tracks: Vec<(PathBuf, Vec<(String, Option<String>)>)>,
}

fn undo_file(id: &str) -> Result<PathBuf> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let data = served.data_dir.clone().context("Cratarr's data folder is needed")?;
    Ok(Path::new(&data).join("fix-undo").join(format!("{id}.json")))
}

/// An album's tracks' files (as Renumber reads them).
fn track_paths(album: &str) -> Result<Vec<PathBuf>> {
    Ok(super::renumber::tracks_of(album)?.0.into_iter().map(|t| t.path).collect())
}

/// Writes the fixes into their albums' tracks, the values before kept first, and reads them again at once. Returns
/// the undo token, and the fixes that could not be written with why (the rest stand).
pub async fn apply(fixes: Vec<Fix>) -> Result<(String, Vec<(String, String)>)> {
    tokio::task::spawn_blocking(move || -> Result<(String, Vec<(String, String)>)> {
        let id = format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis());
        let file = undo_file(&id)?;
        std::fs::create_dir_all(file.parent().context("no folder")?)?;
        let mut before = Before::default();
        let mut failed = Vec::new();
        let mut written = Vec::new();
        for fix in &fixes {
            for (track, set) in &fix.writes {
                let names: Vec<&str> = set.iter().map(|(n, _)| *n).collect();
                let was = match values(track, &names) {
                    Ok(v) => v,
                    Err(e) => {
                        failed.push((fix.id.clone(), format!("{}: {e:#}", track.display())));
                        continue;
                    }
                };
                // Kept before anything changes, so Undo has every track that may have been written.
                before.tracks.push((track.clone(), names.iter().map(|n| n.to_string()).zip(was).collect()));
                std::fs::write(&file, serde_json::to_vec(&before)?)?;
                match write(track, set) {
                    Ok(()) => written.push(track.clone()),
                    Err(e) => failed.push((fix.id.clone(), format!("{}: {e:#}", track.display()))),
                }
            }
        }
        if let Some(served) = crate::serve::instance() {
            served.reread(&written);
        }
        Ok((format!("{UNDO}{id}"), failed))
    })
    .await?
}

/// Undoes fixes (`f…`): each track's values as they were. Safe to repeat.
pub async fn undo(token: &str) -> Result<()> {
    let id = token.strip_prefix(UNDO).filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit())).context("Not a fix's undo")?.to_string();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let file = undo_file(&id)?;
        let before: Before = serde_json::from_slice(&std::fs::read(&file).context("Those fixes can no longer be undone")?)?;
        let mut back = Vec::new();
        for (track, was) in &before.tracks {
            let set: Vec<(&'static str, Option<String>)> = was.iter().filter_map(|(n, v)| Some((static_name(n)?, v.clone()))).collect();
            match write(track, &set) {
                Ok(()) => back.push(track.clone()),
                Err(e) => tracing::warn!("fixes: {} not given its tags back: {e:#}", track.display()),
            }
        }
        std::fs::remove_file(&file).ok();
        if let Some(served) = crate::serve::instance() {
            served.reread(&back);
        }
        Ok(())
    })
    .await?
}

/// A tag name as `item_key` knows it.
fn static_name(n: &str) -> Option<&'static str> {
    ["date", "genre", "albumartist", "musicbrainz_albumid", "musicbrainz_releasegroupid", "musicbrainz_albumartistid", "musicbrainz_artistid", "musicbrainz_trackid"]
        .into_iter()
        .find(|k| *k == n)
}

/// The fixes of a finding (`library:year`, `library:genre`, `library:album-artist`, `library:musicbrainz`), for each of
/// its albums that has one, the certain first. Blocking parts apart.
pub async fn of_finding(app: &super::App, key: &str) -> Result<Vec<Fix>> {
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let music = app.library.folders().music;
    let folders = app.library.catalogue().folders();
    let music = music.and_then(|m| folders.iter().position(|f| f == Path::new(&m))).map(|i| i as i64);
    let (s, k) = (served.clone(), key.to_string());
    // Year and genre from what is kept; album artist and MusicBrainz ids from a match (`matching`), its albums here.
    let (mut fixes, to_match) = tokio::task::spawn_blocking(move || -> Result<(Vec<Fix>, Vec<String>)> {
        let lacking: Vec<crate::serve::library::Lacks> = s.library.lacking()?.into_iter().filter(|r| music.is_none_or(|m| r.folder == m)).collect();
        let ids = |pick: fn(&crate::serve::library::Lacks) -> bool| lacking.iter().filter(|r| pick(r)).map(|r| r.id.clone()).collect::<Vec<_>>();
        Ok(match k.as_str() {
            "library:year" => (year_fixes(&s, lacking.iter().filter(|r| r.year).map(|r| r.id.as_str()))?, Vec::new()),
            "library:genre" => (genre_fixes(&s, lacking.iter().filter(|r| r.genre).map(|r| r.id.as_str()))?, Vec::new()),
            "library:album-artist" => (Vec::new(), ids(|r| r.album_artist)),
            "library:musicbrainz" => (Vec::new(), ids(|r| r.musicbrainz)),
            _ => (Vec::new(), Vec::new()),
        })
    })
    .await??;
    if !to_match.is_empty() {
        fixes.extend(super::matching::fixes(&served, key, to_match).await?);
    }
    fixes.sort_by(|a, b| (a.certainty != "certain").cmp(&(b.certainty != "certain")).then_with(|| a.id.cmp(&b.id)));
    Ok(fixes)
}

/// The fix of each album without a year whose release MusicBrainz dated: its first release's year, as `date`.
fn year_fixes<'a>(served: &crate::serve::Serve, albums: impl Iterator<Item = &'a str>) -> Result<Vec<Fix>> {
    let years = served.userdata.original_years()?;
    let mut out = Vec::new();
    for album in albums {
        let Some((_, _, Some(release), _, _)) = served.library.album_place(album)? else { continue };
        let Some(year) = years.get(&release) else { continue };
        let paths = track_paths(album)?;
        out.push(Fix {
            id: format!("year:{album}"),
            album: album.to_string(),
            field: "year",
            from: None,
            to: year.to_string(),
            source: "MusicBrainz".into(),
            certainty: "certain",
            why: "Its release's first release date on MusicBrainz".into(),
            writes: paths.into_iter().map(|p| (p, vec![("date", Some(year.to_string()))])).collect(),
        });
    }
    Ok(out)
}

/// The fix of each album without a genre that the library's words give one: its strongest genre, certain where two
/// sources or more say it.
fn genre_fixes<'a>(served: &crate::serve::Serve, albums: impl Iterator<Item = &'a str>) -> Result<Vec<Fix>> {
    let merged = super::words::merged(served)?;
    let mut out = Vec::new();
    for album in albums {
        let Some(best) = merged.albums.get(album).and_then(|w| w.iter().find(|w| w.kind == "genre")) else { continue };
        let paths = track_paths(album)?;
        let sources = names(&best.sources);
        out.push(Fix {
            id: format!("genre:{album}"),
            album: album.to_string(),
            field: "genre",
            from: None,
            to: best.word.clone(),
            source: sources.clone(),
            certainty: if best.sources.len() >= 2 { "certain" } else { "likely" },
            why: format!("The genre {sources} give it most"),
            writes: paths.into_iter().map(|p| (p, vec![("genre", Some(best.word.clone()))])).collect(),
        });
    }
    Ok(out)
}

/// Sources as said: "MusicBrainz", "MusicBrainz and Discogs", "MusicBrainz, Discogs and Last.fm".
fn names(sources: &[String]) -> String {
    let shown: Vec<&str> = sources
        .iter()
        .map(|s| match s.as_str() {
            "musicbrainz" => "MusicBrainz",
            "discogs" => "Discogs",
            "lastfm" => "Last.fm",
            "files" => "the files",
            "household" => "the household's tags",
            other => other,
        })
        .collect();
    match shown.split_last() {
        None => String::new(),
        Some((last, [])) => last.to_string(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// The fixes asked for by id, worked out again now (a fix applies as it stands, not as it was shown).
pub async fn by_ids(app: &super::App, ids: &[String]) -> Result<Vec<Fix>> {
    let mut findings: HashMap<&'static str, Vec<&String>> = HashMap::new();
    for id in ids {
        let finding = match id.split_once(':').map(|(f, _)| f) {
            Some("year") => "library:year",
            Some("genre") => "library:genre",
            Some("albumartist") => "library:album-artist",
            Some("musicbrainz") => "library:musicbrainz",
            _ => bail!("Not a fix: {id}"),
        };
        findings.entry(finding).or_default().push(id);
    }
    let mut out = Vec::new();
    for (finding, wanted) in findings {
        out.extend(of_finding(app, finding).await?.into_iter().filter(|f| wanted.contains(&&f.id)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_are_said_as_a_list() {
        assert_eq!(names(&["musicbrainz".into()]), "MusicBrainz");
        assert_eq!(names(&["musicbrainz".into(), "discogs".into(), "lastfm".into()]), "MusicBrainz, Discogs and Last.fm");
        assert_eq!(names(&[]), "");
    }

    #[test]
    fn every_tag_a_fix_writes_has_its_key() {
        for n in ["date", "genre", "albumartist", "musicbrainz_albumid", "musicbrainz_releasegroupid", "musicbrainz_albumartistid", "musicbrainz_artistid", "musicbrainz_trackid"] {
            assert!(item_key(n).is_some() && static_name(n) == Some(n), "{n}");
        }
    }
}
