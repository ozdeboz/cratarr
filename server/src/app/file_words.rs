//! A record's words into its files (0.15, decision 0097's controls): for each kind the Catalogue sheet sets to Write
//! (genre by default; style, mood and tags where chosen), Health lists the records whose files' field differs from
//! Cratarr's words (`words:<kind>`), and the review sheet (`fix:words:<kind>`) offers each file's field as it has it
//! and as Cratarr has it, with where it comes from; a run on the one path (`file_runs`) writes the ticked, one Undo.
//!
//! - **The fields:** genre as Genre, style as STYLE (Vorbis; ID3v2 `TXXX:STYLE`), mood as MOOD, tags as Grouping.
//! - **A track's words** are its own where it has them, else its record's (`words::store::Words::of_track`).
//! - **Never emptied:** where Cratarr has no word of a kind for a track, its file keeps what it has.
//!
//! Nothing is written by itself (decision 0092): Write only decides what Health lists.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail};

use super::file_runs::{self, Proposal, flat};
use super::words::store::{Kind, Source};
use crate::journal::{FileWrite, RunRecord};

/// A kind's plan: `fix:words:<kind>`.
const PREFIX: &str = "fix:words:";

/// Whether a plan is one of these.
pub fn owns(kind: &str) -> bool {
    kind.strip_prefix(PREFIX).is_some_and(|k| Kind::of(k).is_some())
}

/// The field a kind is written as, by the tag reader's names.
fn field_of(kind: Kind) -> &'static str {
    match kind {
        Kind::Genre => "genre",
        Kind::Style => "style",
        Kind::Mood => "mood",
        Kind::Tag => "grouping",
    }
}

/// The sheet's title.
fn title_of(kind: Kind) -> &'static str {
    match kind {
        Kind::Genre => "Genres into the files",
        Kind::Style => "Styles into the files",
        Kind::Mood => "Moods into the files",
        Kind::Tag => "Tags into the files",
    }
}

/// Where a track's words come from, as the sheet says it: each source once, strongest first.
fn from_of(words: &[super::words::store::Word]) -> String {
    let mut sources: Vec<Source> = words.iter().flat_map(|w| w.sources.iter().copied()).collect();
    sources.sort();
    sources.dedup();
    let named: Vec<&str> = sources
        .iter()
        .map(|s| match s {
            Source::Hand => "set by hand",
            Source::Files => "the files",
            Source::Audiomuse => "AudioMuse",
            Source::Household => "the household's tags",
            Source::Musicbrainz => "MusicBrainz",
            Source::Discogs => "Discogs",
            Source::Lastfm => "Last.fm",
        })
        .collect();
    named.join(", ")
}

/// What one file gets: its field now and Cratarr's words, where they differ (the same words in any order do not).
pub fn file_plan(has: &[String], words: &[super::words::store::Word]) -> Option<(Vec<String>, Vec<String>, String)> {
    if words.is_empty() {
        return None;
    }
    let want: Vec<String> = words.iter().map(|w| w.word.clone()).collect();
    let (mut a, mut b) = (flat(has), flat(&want));
    a.sort();
    b.sort();
    if a == b {
        return None;
    }
    Some((has.to_vec(), want, from_of(words)))
}

/// Every file of Cratarr's own records (only `albums` where given) whose `kind` differs from Cratarr's words. Blocking.
pub fn proposal(serve: &crate::serve::Serve, kind: Kind, albums: Option<&[String]>) -> Result<Proposal> {
    proposal_of(serve, &*super::words::stored(serve)?, kind, albums)
}

/// `proposal`, from these words.
fn proposal_of(serve: &crate::serve::Serve, stored: &super::words::Stored, kind: Kind, albums: Option<&[String]>) -> Result<Proposal> {
    let field = field_of(kind);
    let cat = serve.catalogue();
    let roots = cat.folders();
    let values = cat.named_values(&[field])?;
    let mut p = Proposal { kind: format!("{PREFIX}{}", kind.name()), title: title_of(kind).into(), ..Proposal::default() };
    let db = serve.library.db();
    let mut st = db.prepare(
        "SELECT a.id, a.name, a.artist, c.song, c.folder, c.rel, s.title
         FROM albums a JOIN copy_files c ON c.album = a.id LEFT JOIN songs s ON s.id = c.song ORDER BY a.n, s.id IS NULL, s.disc_no, s.track_no, c.rel",
    )?;
    let rows = st.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?, r.get::<_, String>(5)?, r.get::<_, Option<String>>(6)?))
    })?;
    let none: Vec<String> = Vec::new();
    for row in rows {
        let (album, title, artist, song, folder, rel, track) = row?;
        // A copy with tracks of its own has no `songs` row, and the record's words are its too.
        let track = track.unwrap_or_else(|| super::renumber::file_name(&rel));
        if albums.is_some_and(|a| !a.contains(&album)) {
            continue;
        }
        let Some(words) = stored.records.get(&album) else { continue };
        let Some(root) = roots.get(folder) else { continue };
        let has = values.get(&(folder, rel.clone())).and_then(|v| v.get(field)).unwrap_or(&none);
        let Some((before, after, from)) = file_plan(has, words.of_track(&song, kind)) else { continue };
        if !p.records.iter().any(|r| r.album == album) {
            p.records.push(RunRecord { album: album.clone(), title, artist });
        }
        p.files.push(FileWrite {
            album,
            path: root.join(&rel),
            track: Some(track),
            before: vec![(field.to_string(), before)],
            after: vec![(field.to_string(), after)],
            from: vec![(field.to_string(), from)],
            undone: false,
            whole: None,
        });
    }
    Ok(p)
}

/// Health's findings: per kind set to Write, the records whose files differ (`words:<kind>`), each opening its record.
pub async fn findings(served: &Arc<crate::serve::Serve>) -> Result<Vec<(Kind, Vec<RunRecord>)>> {
    let s = served.clone();
    tokio::task::spawn_blocking(move || -> Result<Vec<(Kind, Vec<RunRecord>)>> {
        let mut out = Vec::new();
        for kind in super::words::store::KINDS {
            if !super::words::catalogue::writes(&s, kind.name()) {
                continue;
            }
            let p = proposal(&s, kind, None)?;
            if !p.records.is_empty() {
                out.push((kind, p.records));
            }
        }
        Ok(out)
    })
    .await?
}

fn kind_of(plan: &str) -> Result<Kind> {
    plan.strip_prefix(PREFIX).and_then(Kind::of).context("Not a plan for words")
}

/// The review sheet: every record whose files differ (only `albums` where given).
pub async fn plan(plan: &str, albums: Option<Vec<String>>) -> Result<file_runs::PlanView> {
    let kind = kind_of(plan)?;
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    let p = tokio::task::spawn_blocking(move || proposal(&served, kind, albums.as_deref())).await??;
    Ok(file_runs::view(&p.kind, &p.title, &p.records, &p.files, false))
}

/// The ticked records written (an administrator's apply on the review sheet), each set aside where it is no longer what
/// the sheet showed (its stamp): a run on the one path.
pub async fn apply(app: &Arc<super::App>, plan: &str, albums: Vec<String>, stamps: Option<HashMap<String, String>>) -> Result<file_runs::Status> {
    let kind = kind_of(plan)?;
    let served = crate::serve::instance().context("Cratarr's own library is not open")?;
    anyhow::ensure!(!albums.is_empty(), "No records ticked");
    let name = format!("{PREFIX}{}", kind.name());
    anyhow::ensure!(!file_runs::busy(&name), "Already writing");
    let p = tokio::task::spawn_blocking(move || -> Result<Proposal> {
        if !super::words::catalogue::writes(&served, kind.name()) {
            bail!("Writing {}s into the files is switched off", kind.name());
        }
        Ok(file_runs::ticked(proposal(&served, kind, Some(&albums))?, &albums, stamps.as_ref()))
    })
    .await??;
    anyhow::ensure!(!p.files.is_empty() || !p.failed.is_empty(), "Nothing differs");
    Ok(file_runs::start(app, p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::words::store::Word;

    fn w(word: &str, sources: &[Source]) -> Word {
        Word { word: word.into(), sources: sources.to_vec() }
    }

    /// A song's id is its text ("tr-1"): read as a number, every plan failed, and with it Health's library check.
    #[test]
    fn a_plan_reads_the_library_it_is_for() {
        let dir = std::env::temp_dir().join(format!("cratarr-file-words-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("A")).unwrap();
        std::fs::write(dir.join("A/1.mp3"), b"x").unwrap();
        let cat = std::sync::Arc::new(crate::catalogue::Catalogue::open(None, vec![dir.clone()]).unwrap());
        cat.walk_all();
        cat.pretend_read_for_tests();
        cat.identify().unwrap();
        let serve = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(cat);
        serve.refresh().unwrap();
        assert_eq!(serve.library.db().query_row("SELECT count(*) FROM songs", [], |r| r.get::<_, i64>(0)).unwrap(), 1, "a song to read");
        for kind in crate::app::words::store::KINDS {
            assert!(proposal(&serve, kind, None).unwrap().files.is_empty(), "no words stored: nothing to write");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A word is the record's or its track's: every copy's file gets it, and a copy on tracks of its own the record's.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_word_reaches_every_copy_and_its_undo_takes_it_out() {
        use super::super::every_copy::{app, tag, two_copies, undone, written};
        use super::super::words::store::{KindWords, Words};
        for shared in [true, false] {
            let (c, app) = (two_copies(&format!("words-{shared}"), shared), app(&format!("words-{shared}")).await);
            let words = Words { kinds: vec![KindWords { kind: Kind::Mood, hand: false, words: vec![w("Calm", &[Source::Hand])] }], tracks: Vec::new() };
            let stored = super::super::words::Stored { records: std::collections::HashMap::from([(c.album.clone(), words)]), ..Default::default() };
            let p = proposal_of(&c.serve, &stored, Kind::Mood, None).unwrap();
            assert_eq!(p.files.len(), 4, "every file of both copies");
            let token = written(&app, p).await;
            assert!(c.files().iter().all(|f| tag(f, "mood") == ["Calm"]));
            undone(&app, &token).await;
            assert!(c.files().iter().all(|f| tag(f, "mood").is_empty()));
        }
    }

    #[test]
    fn a_file_gets_cratarrs_words_where_they_differ_and_is_never_emptied() {
        let has = vec!["Electronic; Dub".to_string()];
        // The same words, another order and case: nothing.
        assert_eq!(file_plan(&has, &[w("dub", &[Source::Files]), w("Electronic", &[Source::Files])]), None);
        // Another word: the whole field, said where from.
        let (before, after, from) = file_plan(&has, &[w("Dub Techno", &[Source::Hand]), w("Electronic", &[Source::Files, Source::Discogs])]).unwrap();
        assert_eq!((before, after), (has.clone(), vec!["Dub Techno".to_string(), "Electronic".to_string()]));
        assert_eq!(from, "set by hand, the files, Discogs");
        // Cratarr has none: the file keeps its own.
        assert_eq!(file_plan(&has, &[]), None);
        assert!(owns("fix:words:style") && !owns("fix:words:colour") && !owns("fix:library:genre"));
    }
}
