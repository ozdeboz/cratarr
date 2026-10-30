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
    /// What it sets: year, genre, albumartist, musicbrainz, cover (its `to` the picture's address), rule (a genre or
    /// an artist the rules name otherwise: `from` → `to`).
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
    /// A genre fix's choices (0.9.8, the owner: "the option to use multiple found genres"): every genre and style found
    /// for the album, each with its sources; the administrator ticks several (`choose`). `to` is the strongest.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Choice>,
}

/// One genre or style a genre fix offers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Choice {
    pub word: String,
    /// genre or style, as the sources have it.
    pub kind: &'static str,
    /// Its sources as said ("Discogs and Last.fm").
    pub source: String,
    /// certain where two sources or more say it.
    pub certainty: &'static str,
}

/// The genres chosen for genre fixes (by fix id), each among what the fix offers: written as several genres ("; "
/// between them, as the rules split them). A fix chosen for that is not a genre fix, or offered none of them, refused.
pub fn choose(fixes: &mut [Fix], chosen: &HashMap<String, Vec<String>>) -> Result<()> {
    for (id, words) in chosen {
        let Some(fix) = fixes.iter_mut().find(|f| &f.id == id) else { continue };
        if fix.field != "genre" {
            bail!("Only a genre fix takes chosen genres");
        }
        let mut picked: Vec<String> = Vec::new();
        for w in words {
            let found = fix.candidates.iter().map(|c| c.word.as_str()).chain(std::iter::once(fix.to.as_str())).find(|c| c.eq_ignore_ascii_case(w.trim()));
            if let Some(c) = found
                && !picked.iter().any(|p| p.eq_ignore_ascii_case(c))
            {
                picked.push(c.to_string());
            }
        }
        if picked.is_empty() {
            bail!("None of those genres was found for it");
        }
        let to = picked.join("; ");
        for (_, set) in fix.writes.iter_mut() {
            *set = vec![("genre", Some(to.clone()))];
        }
        fix.to = to;
    }
    Ok(())
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
        "tracknumber" => ItemKey::TrackNumber,
        "discnumber" => ItemKey::DiscNumber,
        _ => return None,
    })
}

/// A track's tag values by name, as the tag reader reads them (the first of each).
pub(crate) fn values(track: &Path, names: &[&str]) -> Result<Vec<Option<String>>> {
    let (tags, _) = crate::catalogue::tags::read(track)?;
    Ok(names.iter().map(|n| tags.items.iter().find(|(_, name, _)| *name == Some(*n)).map(|(_, _, v)| v.clone())).collect())
}

/// Writes these values into a track (none: removes the tag), read back from the checked copy before it replaces it.
pub(crate) fn write(track: &Path, set: &[(&'static str, Option<String>)]) -> Result<()> {
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
    /// Files written whole (0.9.14, a length's fixes: `length_fixes`): each with its original's copy.
    #[serde(default)]
    files: Vec<(PathBuf, PathBuf)>,
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

/// Writes the fixes into their albums' tracks, the values before kept first, and reads them again at once; a cover is
/// set as a chosen one is (`covers::set`, its own undo). Returns the undo tokens, and the fixes that could not be
/// written with why (the rest stand).
pub async fn apply(fixes: Vec<Fix>) -> Result<(Vec<String>, Vec<(String, String)>)> {
    let (covers, fixes): (Vec<Fix>, Vec<Fix>) = fixes.into_iter().partition(|f| f.field == "cover");
    let mut tokens = Vec::new();
    let mut failed = Vec::new();
    for c in covers {
        match super::covers::set(&c.album, &c.to).await {
            Ok(t) => tokens.push(t),
            Err(e) => failed.push((c.id, super::plain(format!("{e:#}")))),
        }
    }
    let (token, more) = tokio::task::spawn_blocking(move || -> Result<(String, Vec<(String, String)>)> {
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
                        failed.push((fix.id.clone(), super::plain(format!("{}: {e:#}", track.display()))));
                        continue;
                    }
                };
                // Kept before anything changes, so Undo has every track that may have been written.
                before.tracks.push((track.clone(), names.iter().map(|n| n.to_string()).zip(was).collect()));
                std::fs::write(&file, serde_json::to_vec(&before)?)?;
                match write(track, set) {
                    Ok(()) => written.push(track.clone()),
                    Err(e) => failed.push((fix.id.clone(), super::plain(format!("{}: {e:#}", track.display())))),
                }
            }
        }
        // A length's fixes (0.9.14): a file read again, or its length written into it, its original kept first.
        let backups = file.with_extension("");
        for (n, fix) in fixes.iter().filter(|f| super::length_fixes::FIELDS.contains(&f.field)).enumerate() {
            match super::length_fixes::apply(fix, &backups, n) {
                Ok((path, kept)) => {
                    if let Some(kept) = kept {
                        before.files.push((path.clone(), kept));
                        std::fs::write(&file, serde_json::to_vec(&before)?)?;
                    }
                    written.push(path);
                }
                Err(e) => failed.push((fix.id.clone(), super::plain(format!("{e:#}")))),
            }
        }
        if let Some(served) = crate::serve::instance() {
            served.reread(&written);
        }
        Ok((format!("{UNDO}{id}"), failed))
    })
    .await??;
    tokens.push(token);
    failed.extend(more);
    Ok((tokens, failed))
}

/// Applies fixes as `apply`, in a task of its own (0.9.8: a page that stops waiting, a phone gone to sleep, does not cut
/// it off half-way), and a line in the log. The undo tokens, how many fixes were written whole, and those not with why.
pub async fn apply_all(fixes: Vec<Fix>) -> Result<(Vec<String>, usize, Vec<(String, String)>)> {
    let started = std::time::Instant::now();
    let done: Vec<(String, String, &'static str)> = fixes.iter().map(|f| (f.id.clone(), f.album.clone(), f.field)).collect();
    let (tokens, failed) = tokio::spawn(apply(fixes)).await??;
    let mut written = 0;
    for (id, ..) in &done {
        if !failed.iter().any(|(f, _)| f == id) {
            written += 1;
        }
    }
    tracing::info!(target: "library", fixes = done.len(), written, failed = failed.len(), ms = started.elapsed().as_millis() as u64, "fixes applied");
    Ok((tokens, written, failed))
}

/// Undoes fixes (`f…`): each track's values as they were. Safe to repeat.
pub async fn undo(token: &str) -> Result<()> {
    let id = token.strip_prefix(UNDO).filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit())).context("Not a fix's undo")?.to_string();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let file = undo_file(&id)?;
        let before: Before = serde_json::from_slice(&std::fs::read(&file).context("Those fixes can no longer be undone")?)?;
        let mut back = Vec::new();
        // Files written whole: their originals back, in place.
        for (path, kept) in &before.files {
            match std::fs::copy(kept, path) {
                Ok(_) => back.push(path.clone()),
                Err(e) => tracing::warn!("fixes: {} not given back: {e:#}", path.display()),
            }
        }
        for (track, was) in &before.tracks {
            let set: Vec<(&'static str, Option<String>)> = was.iter().filter_map(|(n, v)| Some((static_name(n)?, v.clone()))).collect();
            match write(track, &set) {
                Ok(()) => back.push(track.clone()),
                Err(e) => tracing::warn!("fixes: {} not given its tags back: {e:#}", track.display()),
            }
        }
        std::fs::remove_file(&file).ok();
        std::fs::remove_dir_all(file.with_extension("")).ok();
        if let Some(served) = crate::serve::instance() {
            served.reread(&back);
        }
        Ok(())
    })
    .await?
}

/// A tag name as `item_key` knows it.
fn static_name(n: &str) -> Option<&'static str> {
    [
        "date",
        "genre",
        "albumartist",
        "musicbrainz_albumid",
        "musicbrainz_releasegroupid",
        "musicbrainz_albumartistid",
        "musicbrainz_artistid",
        "musicbrainz_trackid",
        "tracknumber",
        "discnumber",
    ]
    .into_iter()
        .find(|k| *k == n)
}

/// The fixes of a finding (`library:year`, `library:genre`, `library:album-artist`, `library:musicbrainz`), for each of
/// its albums that has one, the certain first. Blocking parts apart.
pub async fn of_finding(app: &super::App, key: &str) -> Result<Vec<Fix>> {
    // Tracks shorter than their length (0.9.14): a fix a track.
    if key.starts_with("files:") {
        let k = key.to_string();
        return tokio::task::spawn_blocking(move || super::length_fixes::of_finding(&k)).await?;
    }
    let served = app.served.clone().context("Cratarr's own library is not open")?;
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
            "library:rules" => (ruled_albums(&s)?.iter().filter_map(|a| rule_fix(&s, a).transpose()).collect::<Result<Vec<_>>>()?, Vec::new()),
            _ => (Vec::new(), Vec::new()),
        })
    })
    .await??;
    if !to_match.is_empty() {
        fixes.extend(super::matching::fixes(&served, key, to_match).await?);
    }
    // Missing genres and years: their albums asked of Discogs in the background, a round at a time (0.9.8); what it
    // says shows the next time the fixes are read.
    if matches!(key, "library:genre" | "library:year") {
        let (s, k, have) = (served.clone(), key.to_string(), fixes.iter().map(|f| f.album.clone()).collect::<std::collections::HashSet<_>>());
        let lacking = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
            Ok(s.library.lacking()?.into_iter().filter(|l| if k == "library:genre" { l.genre } else { l.year }).map(|l| l.id).filter(|a| !have.contains(a)).collect())
        })
        .await??;
        super::discogs_facts::fill(served.clone(), lacking);
    }
    fixes.sort_by(|a, b| (a.certainty != "certain").cmp(&(b.certainty != "certain")).then_with(|| a.id.cmp(&b.id)));
    Ok(fixes)
}

/// The fix of each album without a year whose release MusicBrainz dated: its first release's year, as `date`.
fn year_fixes<'a>(served: &crate::serve::Serve, albums: impl Iterator<Item = &'a str>) -> Result<Vec<Fix>> {
    let years = served.userdata.original_years()?;
    let mut out = Vec::new();
    for album in albums {
        let Some((_, _, release, _, _)) = served.library.album_place(album)? else { continue };
        // MusicBrainz's first release date, else Discogs' master's year (0.9.8).
        let (year, source, certainty, why) = match release.as_ref().and_then(|r| years.get(r)) {
            Some(y) => (*y, "MusicBrainz", "certain", "Its release's first release date on MusicBrainz"),
            None => match super::discogs_facts::album_kept(served, album)?.and_then(|f| f.year) {
                Some(y) => (y, "Discogs", "likely", "Its master's year on Discogs"),
                None => continue,
            },
        };
        let paths = track_paths(album)?;
        out.push(Fix {
            id: format!("year:{album}"),
            album: album.to_string(),
            field: "year",
            from: None,
            to: year.to_string(),
            source: source.into(),
            certainty,
            why: why.into(),
            writes: paths.into_iter().map(|p| (p, vec![("date", Some(year.to_string()))])).collect(),
            candidates: Vec::new(),
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
        let Some(found) = merged.albums.get(album) else { continue };
        let Some(best) = found.iter().find(|w| w.kind == "genre") else { continue };
        let paths = track_paths(album)?;
        let sources = names(&best.sources);
        let candidates: Vec<Choice> = found
            .iter()
            .filter(|w| matches!(w.kind, "genre" | "style"))
            .map(|w| Choice { word: w.word.clone(), kind: w.kind, source: names(&w.sources), certainty: if w.sources.len() >= 2 { "certain" } else { "likely" } })
            .collect();
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
            candidates,
        });
    }
    Ok(out)
}

/// The albums whose files' genres or artists a rule names otherwise (0.9.7): the files' tags as the catalogue read
/// them, held against the rules' aliases. None without aliases.
pub fn ruled_albums(served: &crate::serve::Serve) -> Result<Vec<String>> {
    let rules = served.rules();
    if rules.genre_aliases.is_empty() && rules.artist_aliases.is_empty() {
        return Ok(Vec::new());
    }
    let cat = served.catalogue();
    // Artists split as the library splits them (a slash only spaced, `artist_separators_as_used`).
    let artist_seps = crate::catalogue::normalise::artist_separators_as_used(&rules.artist_separators);
    let mut places = Vec::new();
    for (folder, rel, name, value) in cat.tag_values(&["genre", "artist", "albumartist"])? {
        let aliases = if name == "genre" { &rules.genre_aliases } else { &rules.artist_aliases };
        let seps = if name == "genre" { &rules.genre_separators } else { &artist_seps };
        let keep = if name == "genre" { &rules.genre_keep } else { &rules.artist_keep };
        if crate::catalogue::normalise::split(&value, seps, keep).iter().any(|v| crate::catalogue::normalise::aliased(v, aliases) != v.trim()) {
            places.push((folder, rel));
        }
    }
    served.library.albums_of_files(&places)
}

/// An album's rule fix: its tracks' genres and artists as the rules name them, where any differs (each value split as
/// the rules split it, aliased, written joined by "; "); certain, the rules being the administrator's own.
fn rule_fix(served: &crate::serve::Serve, album: &str) -> Result<Option<Fix>> {
    let rules = served.rules();
    if rules.genre_aliases.is_empty() && rules.artist_aliases.is_empty() {
        return Ok(None);
    }
    let artist_seps = crate::catalogue::normalise::artist_separators_as_used(&rules.artist_separators);
    let fields: [(&'static str, &[crate::catalogue::normalise::Alias], &[String], &[String]); 3] = [
        ("genre", &rules.genre_aliases, &rules.genre_separators, &rules.genre_keep),
        ("artist", &rules.artist_aliases, &artist_seps, &rules.artist_keep),
        ("albumartist", &rules.artist_aliases, &artist_seps, &rules.artist_keep),
    ];
    let names: Vec<&str> = fields.iter().map(|f| f.0).collect();
    let (mut writes, mut said) = (Vec::new(), Vec::<String>::new());
    for path in track_paths(album)? {
        let now = values(&path, &names)?;
        let mut set = Vec::new();
        for ((name, aliases, seps, keep), value) in fields.iter().zip(now) {
            let Some(value) = value else { continue };
            let parts = crate::catalogue::normalise::split(&value, seps, keep);
            let named: Vec<String> = parts.iter().map(|p| crate::catalogue::normalise::aliased(p, aliases)).collect();
            if named.iter().zip(&parts).any(|(n, p)| n != p.trim()) {
                for (n, p) in named.iter().zip(&parts).filter(|(n, p)| *n != p.trim()) {
                    let line = format!("{} → {n}", p.trim());
                    if !said.contains(&line) {
                        said.push(line);
                    }
                }
                set.push((*name, Some(crate::catalogue::normalise::dedupe(named).join("; "))));
            }
        }
        if !set.is_empty() {
            writes.push((path, set));
        }
    }
    let Some((first, rest)) = said.split_first() else { return Ok(None) };
    let (from, to) = first.split_once(" → ").map(|(f, t)| (f.to_string(), t.to_string())).unwrap_or_default();
    Ok(Some(Fix {
        id: format!("rule:{album}"),
        album: album.to_string(),
        field: "rule",
        from: Some(from),
        to,
        source: "Rule".into(),
        certainty: "certain",
        why: if rest.is_empty() { format!("Rule: {first}") } else { format!("Rules: {first}, {}", rest.join(", ")) },
        writes,
        candidates: Vec::new(),
    }))
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

/// The fixes asked for by id (`<field>:<album>`), worked out again now for their albums (a fix applies as it stands,
/// not as it was shown), in Music or in Incoming.
pub async fn by_ids(app: &super::App, ids: &[String]) -> Result<Vec<Fix>> {
    let mut albums: HashMap<&str, Vec<&String>> = HashMap::new();
    let mut out = Vec::new();
    for id in ids {
        let (field, album) = id.split_once(':').with_context(|| format!("Not a fix: {id}"))?;
        // A length's (0.9.14): by its song, worked out again now.
        if super::length_fixes::FIELDS.contains(&field) {
            let served = app.served.clone().context("Cratarr's own library is not open")?;
            let (f, s) = (field.to_string(), album.to_string());
            if let Some(fix) = tokio::task::spawn_blocking(move || super::length_fixes::fix(&served, &f, &s)).await?? {
                out.push(fix);
            }
            continue;
        }
        albums.entry(album).or_default().push(id);
    }
    for (album, wanted) in albums {
        // Only the fields asked for (0.9.8: a genre's apply worked out the album's cover candidates and match too).
        let fields: Vec<&str> = wanted.iter().filter_map(|id| id.split_once(':').map(|(f, _)| f)).collect();
        out.extend(of_album_only(app, album, Some(&fields)).await?.0.into_iter().filter(|f| wanted.contains(&&f.id)));
    }
    Ok(out)
}

/// Every fix one album has (0.9.7, Incoming's intake: what an arrival would get before it is kept), wherever it is: a
/// year, a genre, the album artist and MusicBrainz ids from a match, a cover where it has none (the first the cover
/// look finds; certain from the Cover Art Archive). And whether its match is still being looked for.
pub async fn of_album(app: &super::App, album: &str) -> Result<(Vec<Fix>, bool)> {
    of_album_only(app, album, None).await
}

/// `of_album`, only the fields named (`year`, `genre`, `rule`, `albumartist`, `musicbrainz`, `cover`); all of them for
/// none.
async fn of_album_only(app: &super::App, album: &str, fields: Option<&[&str]>) -> Result<(Vec<Fix>, bool)> {
    let wants = |f: &str| fields.is_none_or(|fs| fs.contains(&f));
    let served = app.served.clone().context("Cratarr's own library is not open")?;
    let (s, a) = (served.clone(), album.to_string());
    let (rule, year, genre) = (wants("rule"), wants("year"), wants("genre"));
    let lacks = tokio::task::spawn_blocking(move || s.library.lacks_of(&a)).await??;
    // Discogs' genres, styles and year for it, where it lacks one (0.9.8: asked now, not only by the background pass).
    if let Some(l) = &lacks
        && ((genre && l.genre) || (year && l.year))
        && let Err(e) = super::discogs_facts::album(&served, album).await
    {
        tracing::debug!("discogs: {album}: {e:#}");
    }
    let (s, a) = (served.clone(), album.to_string());
    let (mut fixes, lacks) = tokio::task::spawn_blocking(move || -> Result<(Vec<Fix>, Option<crate::serve::library::Lacks>)> {
        let Some(l) = lacks else { return Ok((Vec::new(), None)) };
        let mut out: Vec<Fix> = if rule { rule_fix(&s, &a)?.into_iter().collect() } else { Vec::new() };
        if year && l.year {
            out.extend(year_fixes(&s, std::iter::once(a.as_str()))?);
        }
        if genre && l.genre {
            out.extend(genre_fixes(&s, std::iter::once(a.as_str()))?);
        }
        Ok((out, Some(l)))
    })
    .await??;
    let Some(l) = lacks else { return Ok((fixes, false)) };
    if wants("albumartist") && l.album_artist {
        fixes.extend(super::matching::fixes(&served, "library:album-artist", vec![album.to_string()]).await?);
    }
    if wants("musicbrainz") && l.musicbrainz {
        fixes.extend(super::matching::fixes(&served, "library:musicbrainz", vec![album.to_string()]).await?);
    }
    if wants("cover") && l.cover
        && let Ok(found) = super::covers::candidates(album).await
        && let Some(c) = found.candidates.first()
    {
        let size = c.width.zip(c.height).map(|(w, h)| format!(" · {w}×{h}")).unwrap_or_default();
        fixes.push(Fix {
            id: format!("cover:{album}"),
            album: album.to_string(),
            field: "cover",
            from: None,
            to: c.url.clone(),
            source: if c.from == "discogs" { "Discogs" } else { "Cover Art Archive" }.into(),
            certainty: if c.from == "coverartarchive" { "certain" } else { "likely" },
            why: format!("{}{size}", c.what),
            writes: Vec::new(),
            candidates: Vec::new(),
        });
    }
    let matching = served.current_user().is_some_and(|a| super::matching::running(&a));
    Ok((fixes, matching))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn several_found_genres_may_be_chosen_and_only_those() {
        let choice = |w: &str| Choice { word: w.into(), kind: "genre", source: "Discogs".into(), certainty: "likely" };
        let fix = || Fix {
            id: "genre:al-1".into(),
            album: "al-1".into(),
            field: "genre",
            from: None,
            to: "House".into(),
            source: "Last.fm".into(),
            certainty: "likely",
            why: String::new(),
            writes: vec![(PathBuf::from("/m/01.flac"), vec![("genre", Some("House".into()))])],
            candidates: vec![choice("House"), choice("Deep House"), choice("Electronic")],
        };
        let mut fixes = vec![fix()];
        choose(&mut fixes, &HashMap::from([("genre:al-1".to_string(), vec!["electronic".to_string(), "Deep House".to_string(), "Deep House".to_string()])])).unwrap();
        assert_eq!(fixes[0].to, "Electronic; Deep House", "as found, once each, in the order chosen");
        assert_eq!(fixes[0].writes[0].1, vec![("genre", Some("Electronic; Deep House".to_string()))]);
        let mut fixes = vec![fix()];
        assert!(choose(&mut fixes, &HashMap::from([("genre:al-1".to_string(), vec!["Polka".to_string()])])).is_err(), "not one it found");
        let mut fixes = vec![fix()];
        choose(&mut fixes, &HashMap::new()).unwrap();
        assert_eq!(fixes[0].to, "House", "none chosen: the strongest");
    }

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
