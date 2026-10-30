//! Point every playlist line at a file's new place after the library was moved with Cratarr stopped (decision
//! 0016's restructure): `cratarr-server --rewrite-playlists <music folder> <moves.tsv> [--apply]`. The list holds one
//! moved file per line, `old<TAB>new`, both relative to the Music folder. Covers the playlists in Music, in its
//! playlist folder, and the backups of both (an Undo or a restore must not bring the old lines back). Without
//! `--apply` it only reports. With it, each changed file is first copied into `.cratarr-rewrite-<stamp>` in Music.
//!
//! A line naming no file in the list is repaired when its folder had exactly one file of the same name but for the
//! extension or capitals (an MP3 replaced by its FLAC, a title's case corrected): it follows that file.
//!
//! Offline only: a running Cratarr would rewrite the same files.

use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::playlists::{BACKUP_DIR, PLAYLIST_DIR, relative_to};

/// What became of one file's lines.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Counts {
    /// Lines that now name the moved file.
    pub moved: usize,
    /// Of `moved`: lines that named a twin of the file (see the module), as they were.
    pub repaired: Vec<String>,
    /// Lines the list does not name (their file did not move, or was missing already).
    pub kept: usize,
    /// Of `kept`: lines whose file is not there.
    pub missing: Vec<String>,
}

/// The moves: old → new, both relative to the Music folder.
#[derive(Debug, Default)]
pub struct Moves {
    to: HashMap<String, String>,
    /// The old paths by `twin_key`.
    twins: HashMap<String, Vec<String>>,
}

impl Moves {
    fn insert(&mut self, old: String, new: String) {
        self.twins.entry(twin_key(&old)).or_default().push(old.clone());
        self.to.insert(old, new);
    }

    /// Where the file a line names went, and whether the line named a twin of it.
    fn find(&self, rel: &str) -> Option<(&str, bool)> {
        if let Some(new) = self.to.get(rel) {
            return Some((new, false));
        }
        match self.twins.get(&twin_key(rel)).map(Vec::as_slice) {
            Some([only]) => self.to.get(only).map(|new| (new.as_str(), true)),
            _ => None,
        }
    }
}

/// A file's folder and name without the extension, in lower case: the same for an MP3 and the FLAC that replaced
/// it, or a title before and after its case was corrected.
fn twin_key(rel: &str) -> String {
    let stem = rel.rsplit_once('.').filter(|(_, ext)| !ext.contains('/')).map_or(rel, |(s, _)| s);
    stem.to_lowercase()
}

/// Read the list: `old<TAB>new` per line; blank lines and `#` lines skipped. Refuses one old path given two places,
/// and two old paths sent to one.
pub fn parse_moves(text: &str) -> Result<Moves> {
    let mut moves = Moves::default();
    let mut taken: HashMap<String, String> = HashMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((old, new)) = line.split_once('\t') else { bail!("line {}: no tab between the old and new path", n + 1) };
        let (old, new) = (clean(old), clean(new));
        if old.is_empty() || new.is_empty() || new.contains('\t') {
            bail!("line {}: expected exactly two paths", n + 1);
        }
        if let Some(was) = moves.to.get(&old)
            && *was != new
        {
            bail!("line {}: {old} is moved to both {was} and {new}", n + 1);
        }
        if let Some(other) = taken.get(&new)
            && *other != old
        {
            bail!("line {}: {other} and {old} are both moved to {new}", n + 1);
        }
        if moves.to.contains_key(&old) {
            continue;
        }
        taken.insert(new.clone(), old.clone());
        moves.insert(old, new);
    }
    Ok(moves)
}

/// A relative path as compared: forward slashes, no `./`, no leading or trailing slash.
fn clean(p: &str) -> String {
    p.trim().replace('\\', "/").split('/').filter(|s| !s.is_empty() && *s != ".").collect::<Vec<_>>().join("/")
}

/// `base` joined with `rel` without touching the disk: `.` dropped, `..` taken back.
fn joined(base: &Path, rel: &str) -> PathBuf {
    let mut out = base.to_path_buf();
    for part in Path::new(&rel.replace('\\', "/")).components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(s) => out.push(s),
            _ => {}
        }
    }
    out
}

/// The folder a playlist file's lines are relative to: its own, or for a backup the folder it was backed up from.
pub fn home_of(file: &Path) -> PathBuf {
    let dir = file.parent().unwrap_or(Path::new(""));
    if dir.file_name().is_some_and(|n| n == BACKUP_DIR) { dir.parent().unwrap_or(dir).to_path_buf() } else { dir.to_path_buf() }
}

/// Rewrite one playlist's text. `home` is the folder its lines are relative to; `exists` says whether a file (by its
/// path relative to Music) is there, for the report. Comments, blank lines, line endings and a BOM stay as they are.
pub fn rewrite(content: &str, music: &Path, home: &Path, moves: &Moves, exists: &dyn Fn(&str) -> bool) -> (String, Counts) {
    let mut counts = Counts::default();
    let mut out = String::with_capacity(content.len());
    for line in content.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let ending = &line[body.len()..];
        let bom = body.starts_with('\u{feff}');
        let entry = body.trim_start_matches('\u{feff}').trim();
        if entry.is_empty() || entry.starts_with('#') {
            out.push_str(line);
            continue;
        }
        let rel = joined(home, entry).strip_prefix(music).ok().map(|r| clean(&r.to_string_lossy()));
        let found = rel.as_ref().and_then(|r| moves.find(r));
        let new = found.and_then(|(n, twin)| relative_to(home, &joined(music, n)).map(|l| (l, twin)));
        match new {
            Some((new, twin)) => {
                counts.moved += 1;
                if twin {
                    counts.repaired.push(entry.to_string());
                }
                if bom {
                    out.push('\u{feff}');
                }
                out.push_str(&new);
                out.push_str(ending);
            }
            None => {
                counts.kept += 1;
                if rel.as_deref().is_none_or(|r| !exists(r)) {
                    counts.missing.push(entry.to_string());
                }
                out.push_str(line);
            }
        }
    }
    (out, counts)
}

/// Every playlist file this covers: `*.m3u` in Music, in its playlist folder and in each account's folder there
/// (decision 0051), and the `*.bak` backups of each.
fn playlist_files(music: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs = vec![music.to_path_buf(), music.join(PLAYLIST_DIR)];
    if let Ok(rd) = std::fs::read_dir(music.join(PLAYLIST_DIR)) {
        let mut accounts: Vec<PathBuf> = rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()) && e.file_name().to_str().is_some_and(crate::playlists::account_folder_ok))
            .map(|e| e.path())
            .collect();
        accounts.sort();
        dirs.extend(accounts);
    }
    for (dir, ext) in dirs.iter().flat_map(|d| [(d.clone(), "m3u"), (d.join(BACKUP_DIR), "bak")]) {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)))
            .collect();
        files.sort();
        out.extend(files);
    }
    out
}

/// The command. Returns the exit code: 0 when every line found its file (or `--apply` wrote everything), 1 otherwise.
pub fn run(args: &[String]) -> Result<i32> {
    let apply = args.iter().any(|a| a == "--apply");
    let paths: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let [music, list] = paths[..] else {
        bail!("usage: cratarr-server --rewrite-playlists <music folder> <moves.tsv> [--apply]");
    };
    let music = PathBuf::from(music);
    if !music.is_dir() {
        bail!("{} is not a folder", music.display());
    }
    let moves = parse_moves(&std::fs::read_to_string(list).with_context(|| format!("reading {list}"))?)?;
    let exists = |rel: &str| music.join(rel).is_file();
    let new_there = moves.to.values().filter(|n| exists(n)).count();
    println!("{} moves; {} of their new files are there", moves.to.len(), new_there);

    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let safety = music.join(format!(".cratarr-rewrite-{stamp}"));
    let mut total = Counts::default();
    let mut changed: BTreeMap<PathBuf, String> = BTreeMap::new();
    for file in playlist_files(&music) {
        let text = std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
        let (new, c) = rewrite(&text, &music, &home_of(&file), &moves, &exists);
        let name = file.strip_prefix(&music).unwrap_or(&file).display().to_string();
        if c.moved > 0 || !c.missing.is_empty() {
            println!("{name}: {} moved ({} repaired), {} kept, {} missing", c.moved, c.repaired.len(), c.kept, c.missing.len());
            for r in c.repaired.iter().take(5) {
                println!("    repaired: {r}");
            }
            for m in c.missing.iter().take(5) {
                println!("    missing: {m}");
            }
        }
        total.moved += c.moved;
        total.repaired.extend(c.repaired);
        total.kept += c.kept;
        total.missing.extend(c.missing);
        if new != text {
            changed.insert(file, new);
        }
    }
    println!(
        "total: {} files to change, {} lines moved ({} repaired), {} kept, {} missing",
        changed.len(),
        total.moved,
        total.repaired.len(),
        total.kept,
        total.missing.len()
    );
    if !apply {
        println!("nothing written (add --apply)");
        return Ok(i32::from(!total.missing.is_empty()));
    }
    for (file, new) in &changed {
        let rel = file.strip_prefix(&music).unwrap_or(file);
        let keep = safety.join(rel);
        std::fs::create_dir_all(keep.parent().unwrap_or(&safety))?;
        std::fs::copy(file, &keep).with_context(|| format!("copying {} aside", file.display()))?;
        write_in_place(file, new)?;
    }
    println!("written; the old files are in {}", safety.display());
    Ok(0)
}

/// Replace a file's text atomically (a temporary file beside it, then a rename), keeping its permissions; a backup
/// also keeps its modification time.
fn write_in_place(file: &Path, text: &str) -> Result<()> {
    let meta = std::fs::metadata(file)?;
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = file.with_file_name(format!(".{name}.rewrite"));
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::set_permissions(&tmp, meta.permissions())?;
    if file.extension().is_some_and(|e| e == "bak")
        && let Ok(m) = meta.modified()
    {
        std::fs::File::options().write(true).open(&tmp)?.set_modified(m)?;
    }
    std::fs::rename(&tmp, file).with_context(|| format!("replacing {}", file.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moves(pairs: &[(&str, &str)]) -> Moves {
        let mut m = Moves::default();
        for (a, b) in pairs {
            m.insert(a.to_string(), b.to_string());
        }
        m
    }

    const MUSIC: &str = "/media/Music";

    #[test]
    fn lines_in_music_follow_their_files_and_the_rest_stays_byte_for_byte() {
        let m = moves(&[("A - X (1999)/01 a.mp3", "A/X (1999)/01 a.mp3")]);
        let text = "\u{feff}#EXTM3U\r\nA - X (1999)/01 a.mp3\r\n\r\nB - Y/02 b.mp3\r\n";
        let music = Path::new(MUSIC);
        let (out, c) = rewrite(text, music, music, &m, &|_| true);
        assert_eq!(out, "\u{feff}#EXTM3U\r\nA/X (1999)/01 a.mp3\r\n\r\nB - Y/02 b.mp3\r\n");
        assert_eq!((c.moved, c.kept, c.missing.len()), (1, 1, 0));
    }

    #[test]
    fn lines_in_the_playlist_folder_keep_their_dots() {
        let m = moves(&[("A - X/01 a.mp3", "A/X/01 a.mp3")]);
        let music = Path::new(MUSIC);
        let home = music.join(PLAYLIST_DIR);
        let (out, _) = rewrite("../A - X/01 a.mp3\n", music, &home, &m, &|_| true);
        assert_eq!(out, "../A/X/01 a.mp3\n");
    }

    #[test]
    fn a_backup_reads_its_lines_from_the_folder_it_was_backed_up_from() {
        let music = Path::new(MUSIC);
        assert_eq!(home_of(&music.join(BACKUP_DIR).join("Chill.1.bak")), music);
        assert_eq!(home_of(&music.join(PLAYLIST_DIR).join(BACKUP_DIR).join("Chill.1.bak")), music.join(PLAYLIST_DIR));
        assert_eq!(home_of(&music.join("Chill.m3u")), music);
    }

    #[test]
    fn a_first_line_with_a_bom_keeps_it() {
        let m = moves(&[("a.mp3", "A/a.mp3")]);
        let music = Path::new(MUSIC);
        let (out, _) = rewrite("\u{feff}a.mp3\n", music, music, &m, &|_| true);
        assert_eq!(out, "\u{feff}A/a.mp3\n");
    }

    #[test]
    fn backslashes_and_dots_in_a_line_still_find_the_file() {
        let m = moves(&[("A - X/01 a.mp3", "A/X/01 a.mp3")]);
        let music = Path::new(MUSIC);
        let (out, c) = rewrite("./A - X\\01 a.mp3\n", music, music, &m, &|_| true);
        assert_eq!((out.as_str(), c.moved), ("A/X/01 a.mp3\n", 1));
    }

    #[test]
    fn lines_the_list_does_not_name_are_kept_and_missing_ones_reported() {
        let m = moves(&[]);
        let music = Path::new(MUSIC);
        let (out, c) = rewrite("gone.mp3\nthere.mp3\n/elsewhere/x.mp3\n", music, music, &m, &|r| r == "there.mp3");
        assert_eq!(out, "gone.mp3\nthere.mp3\n/elsewhere/x.mp3\n");
        assert_eq!(c.kept, 3);
        assert_eq!(c.missing, vec!["gone.mp3".to_string(), "/elsewhere/x.mp3".to_string()]);
    }

    #[test]
    fn a_line_naming_a_twin_follows_the_one_file_it_was_and_otherwise_stays() {
        let m = moves(&[
            ("Z - W/03 Z - Somersault.flac", "Z/W/03 Z - Somersault.flac"),
            ("A - Z/10 A - Reading a Wave.mp3", "A/Z/10 A - Reading a Wave.mp3"),
            ("B - Y/01 B - Song.flac", "B/Y/01 B - Song.flac"),
            ("B - Y/01 B - Song.ogg", "B/Y/01 B - Song.ogg"),
        ]);
        let music = Path::new(MUSIC);
        let text = "Z - W/03 Z - Somersault.mp3\nA - Z/10 A - Reading A Wave.mp3\nB - Y/01 B - Song.mp3\n";
        let (out, c) = rewrite(text, music, music, &m, &|_| false);
        assert_eq!(out, "Z/W/03 Z - Somersault.flac\nA/Z/10 A - Reading a Wave.mp3\nB - Y/01 B - Song.mp3\n");
        assert_eq!(c.repaired, vec!["Z - W/03 Z - Somersault.mp3".to_string(), "A - Z/10 A - Reading A Wave.mp3".to_string()]);
        assert_eq!(c.missing, vec!["B - Y/01 B - Song.mp3".to_string()], "two twins: not guessed");
    }

    #[test]
    fn the_list_refuses_one_file_sent_two_ways_and_two_files_sent_to_one() {
        assert!(parse_moves("# header\n\na\tA/a\n").is_ok());
        assert!(parse_moves("a\tA/a\na\tB/a\n").is_err());
        assert!(parse_moves("a\tA/a\nb\tA/a\n").is_err());
        assert!(parse_moves("a A/a\n").is_err());
        assert!(parse_moves("\tA/a\n").is_err(), "no old path");
        assert!(parse_moves("a\t\n").is_err(), "no new path");
        assert!(parse_moves("a\tA/a\tB/a\n").is_err(), "a third field");
        assert_eq!(parse_moves("./x\\a.mp3\tX/a.mp3\r\n").unwrap().find("x/a.mp3"), Some(("X/a.mp3", false)));
    }

    #[test]
    fn apply_writes_every_playlist_and_backup_and_keeps_the_old_ones_aside() {
        let root = std::env::temp_dir().join(format!("cratarr-rewrite-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let music = root.join("Music");
        for d in [music.join("A/X"), music.join(BACKUP_DIR), music.join(PLAYLIST_DIR).join(BACKUP_DIR)] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(music.join("A/X/01 a.mp3"), "").unwrap();
        std::fs::write(music.join("Chill.m3u"), "A - X/01 a.mp3\n").unwrap();
        std::fs::write(music.join(BACKUP_DIR).join("Chill.5.bak"), "A - X/01 a.mp3\n").unwrap();
        std::fs::write(music.join(PLAYLIST_DIR).join("Chill.m3u"), "../A - X/01 a.mp3\n").unwrap();
        std::fs::write(music.join(PLAYLIST_DIR).join(BACKUP_DIR).join("Chill.7.bak"), "../A - X/01 a.mp3\n").unwrap();
        // Not a playlist, though it reads like one: left alone.
        std::fs::write(music.join("notes.txt"), "A - X/01 a.mp3\n").unwrap();
        // A backup keeps its time (the newest-first order of backups is by name, but a person looks at times).
        let then = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        for f in [music.join(BACKUP_DIR).join("Chill.5.bak"), music.join("Chill.m3u")] {
            std::fs::File::options().write(true).open(f).unwrap().set_modified(then).unwrap();
        }
        let list = root.join("moves.tsv");
        std::fs::write(&list, "A - X/01 a.mp3\tA/X/01 a.mp3\n").unwrap();
        let args = |extra: &[&str]| {
            let mut v = vec![music.display().to_string(), list.display().to_string()];
            v.extend(extra.iter().map(|s| s.to_string()));
            v
        };

        assert_eq!(run(&args(&[])).unwrap(), 0);
        assert_eq!(std::fs::read_to_string(music.join("Chill.m3u")).unwrap(), "A - X/01 a.mp3\n", "a report writes nothing");

        assert_eq!(run(&args(&["--apply"])).unwrap(), 0);
        let read = |p: PathBuf| std::fs::read_to_string(p).unwrap();
        assert_eq!(read(music.join("Chill.m3u")), "A/X/01 a.mp3\n");
        assert_eq!(read(music.join(BACKUP_DIR).join("Chill.5.bak")), "A/X/01 a.mp3\n");
        assert_eq!(read(music.join(PLAYLIST_DIR).join("Chill.m3u")), "../A/X/01 a.mp3\n");
        assert_eq!(read(music.join(PLAYLIST_DIR).join(BACKUP_DIR).join("Chill.7.bak")), "../A/X/01 a.mp3\n");
        assert_eq!(read(music.join("notes.txt")), "A - X/01 a.mp3\n");
        let modified = |p: PathBuf| std::fs::metadata(p).unwrap().modified().unwrap();
        assert_eq!(modified(music.join(BACKUP_DIR).join("Chill.5.bak")), then, "a backup keeps its time");
        assert_ne!(modified(music.join("Chill.m3u")), then, "a playlist shows it changed");
        let aside: Vec<_> = std::fs::read_dir(&music)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".cratarr-rewrite-"))
            .collect();
        assert_eq!(aside.len(), 1);
        assert_eq!(read(aside[0].path().join("Chill.m3u")), "A - X/01 a.mp3\n");

        // Run again: nothing left to change.
        assert_eq!(run(&args(&["--apply"])).unwrap(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }
}
