//! The one-time pass over a whole library on another computer (decision 0035: the ThinkBook, over the Music share):
//! `cratarr-server --analyse <folder> --out <file> [--threads N]`. Every audio file under the folder is analysed, on
//! every core, and written as one JSON line, `{"rel":…,"size":…,"mtime":…,"analysis":{…}}`, which the server's
//! import (`POST /api/analysis/import`) takes while the file is as it was.
//!
//! The folder is walked as the catalogue walks it, so `rel`, `size` and `mtime` are the catalogue's own. A run that
//! stops is started again with the same command: the lines already in the file are kept and their files skipped
//! while they are unchanged.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::catalogue::tags;
use crate::catalogue::walk::{self, Found};

const USAGE: &str = "usage: cratarr-server --analyse <music folder> --out <file> [--threads N]";

#[derive(Serialize, Deserialize)]
struct Line {
    rel: String,
    size: u64,
    mtime: i64,
    analysis: super::Analysis,
}

/// What a run did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub analysed: usize,
    /// Already in the file, as the file is now.
    pub kept: usize,
    pub failed: usize,
}

/// The command line after `--analyse`. Returns the exit code.
pub fn run(args: &[String]) -> Result<i32> {
    let (mut folder, mut out, mut threads) = (None, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => out = it.next().map(PathBuf::from),
            "--threads" => threads = Some(it.next().and_then(|n| n.parse::<usize>().ok()).filter(|&n| n > 0).context(USAGE)?),
            _ if folder.is_none() && !a.starts_with("--") => folder = Some(PathBuf::from(a)),
            _ => bail!("{USAGE}"),
        }
    }
    let (Some(folder), Some(out)) = (folder, out) else { bail!("{USAGE}") };
    let threads = threads.unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()));
    let counts = analyse_folder(&folder, &out, threads, &mut |done, todo, failed, started| {
        let rate = done as f64 / started.elapsed().as_secs_f64().max(1.0);
        let left = (todo - done) as f64 / rate.max(1e-9);
        eprintln!("{done} of {todo} ({failed} failed), {rate:.1} a second, about {:.0} min left", left / 60.0);
    })?;
    eprintln!("analysed {}, kept {}, failed {}", counts.analysed, counts.kept, counts.failed);
    Ok(if counts.failed > 0 { 1 } else { 0 })
}

/// Analyses what under `folder` is not yet in `out` as it is now, on `threads` threads, appending to `out`.
/// `progress(done, todo, failed, started)` is called every so often.
pub fn analyse_folder(
    folder: &Path,
    out: &Path,
    threads: usize,
    progress: &mut dyn FnMut(usize, usize, usize, Instant),
) -> Result<Counts> {
    let (found, skipped) = walk::walk(folder).with_context(|| format!("reading {}", folder.display()))?;
    if skipped > 0 {
        eprintln!("{skipped} folders or files could not be read and are left out");
    }
    let have = done_already(out)?;
    let (todo, kept): (Vec<Found>, Vec<Found>) =
        found.into_iter().partition(|f| have.get(&f.rel).is_none_or(|&(size, mtime)| (size, mtime) != (f.size, f.mtime)));
    let mut counts = Counts { kept: kept.len(), ..Counts::default() };
    if todo.is_empty() {
        return Ok(counts);
    }

    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(out).with_context(|| format!("opening {}", out.display()))?;
    let todo = Arc::new(todo);
    let next = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = mpsc::channel::<(String, Result<String>)>();
    let workers: Vec<_> = (0..threads.min(todo.len()))
        .map(|_| {
            let (todo, next, tx, folder) = (todo.clone(), next.clone(), tx.clone(), folder.to_path_buf());
            std::thread::spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(f) = todo.get(i) else { break };
                    if tx.send((f.rel.clone(), line_of(&folder, f))).is_err() {
                        break;
                    }
                }
            })
        })
        .collect();
    drop(tx);

    let started = Instant::now();
    let mut said = Instant::now();
    for (rel, line) in rx {
        match line {
            Ok(line) => {
                writeln!(file, "{line}").with_context(|| format!("writing {}", out.display()))?;
                counts.analysed += 1;
            }
            Err(e) => {
                eprintln!("{rel}: {e:#}");
                counts.failed += 1;
            }
        }
        if said.elapsed().as_secs() >= 30 {
            file.flush()?;
            progress(counts.analysed + counts.failed, todo.len(), counts.failed, started);
            said = Instant::now();
        }
    }
    file.flush()?;
    for w in workers {
        w.join().ok();
    }
    Ok(counts)
}

/// One file's line: its tags for the genres (the tempo's rules), then the analysis.
fn line_of(folder: &Path, f: &Found) -> Result<String> {
    let path = folder.join(&f.rel);
    let genres: Vec<String> = match tags::read(&path) {
        Ok((t, _)) => t.items.into_iter().filter(|(_, name, _)| *name == Some("genre")).map(|(_, _, v)| v).collect(),
        Err(_) => Vec::new(),
    };
    let analysis = super::analyse(&path, &genres)?;
    Ok(serde_json::to_string(&Line { rel: f.rel.clone(), size: f.size, mtime: f.mtime, analysis })?)
}

/// The files already in `out` (the last line of each counts), as they were analysed: rel → (size, mtime). Lines of an
/// older analysis version, or cut short by a stop, do not count.
fn done_already(out: &Path) -> Result<HashMap<String, (u64, i64)>> {
    let mut have = HashMap::new();
    let file = match std::fs::File::open(out) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(have),
        Err(e) => return Err(e).with_context(|| format!("reading {}", out.display())),
    };
    for line in std::io::BufReader::new(file).lines() {
        let line = line?;
        if let Ok(l) = serde_json::from_str::<Line>(&line)
            && l.analysis.version == super::VERSION
        {
            have.insert(l.rel, (l.size, l.mtime));
        }
    }
    Ok(have)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_analysed_once_and_a_changed_file_again() {
        let d = std::env::temp_dir().join(format!("cratarr-batch-{}", std::process::id()));
        std::fs::create_dir_all(d.join("A/B (2001)")).unwrap();
        let sine = super::super::tests::wav("batch", 3.0, |t| 0.2 * (2.0 * std::f64::consts::PI * 440.0 * t).sin() as f32);
        std::fs::copy(&sine, d.join("A/B (2001)/01 x.wav")).unwrap();
        std::fs::copy(&sine, d.join("A/B (2001)/02 y.wav")).unwrap();
        std::fs::write(d.join("A/B (2001)/03 broken.flac"), b"not sound").unwrap();
        std::fs::remove_file(&sine).ok();
        let out = d.join("out.jsonl");
        let mut quiet = |_: usize, _: usize, _: usize, _: Instant| {};

        let first = analyse_folder(&d, &out, 2, &mut quiet).unwrap();
        assert_eq!(first, Counts { analysed: 2, kept: 0, failed: 1 });
        let text = std::fs::read_to_string(&out).unwrap();
        let lines: Vec<Line> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        let mut rels: Vec<&str> = lines.iter().map(|l| l.rel.as_str()).collect();
        rels.sort();
        assert_eq!(rels, ["A/B (2001)/01 x.wav", "A/B (2001)/02 y.wav"], "relative, with forward slashes");
        let meta = std::fs::metadata(d.join("A/B (2001)/01 x.wav")).unwrap();
        assert_eq!(lines.iter().find(|l| l.rel.ends_with("01 x.wav")).unwrap().size, meta.len());
        assert!((lines[0].analysis.duration_secs - 3.0).abs() < 0.01);

        // Again: nothing new. Then one file changes and only it is analysed again.
        assert_eq!(analyse_folder(&d, &out, 2, &mut quiet).unwrap(), Counts { analysed: 0, kept: 2, failed: 1 });
        let longer = super::super::tests::wav("batch2", 4.0, |t| 0.2 * (2.0 * std::f64::consts::PI * 440.0 * t).sin() as f32);
        std::fs::copy(&longer, d.join("A/B (2001)/02 y.wav")).unwrap();
        std::fs::remove_file(&longer).ok();
        assert_eq!(analyse_folder(&d, &out, 2, &mut quiet).unwrap(), Counts { analysed: 1, kept: 1, failed: 1 });
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_command_line_needs_a_folder_and_an_out() {
        assert!(run(&["/music".into()]).is_err());
        assert!(run(&["--out".into(), "x".into()]).is_err());
        assert!(run(&["/music".into(), "--out".into(), "x".into(), "--threads".into(), "0".into()]).is_err());
    }
}
