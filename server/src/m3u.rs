//! `.m3u` text handling. Playlists are plain files with one library-relative path per line
//! (`Folder/File.mp3`); `#` lines are comments. Edits touch only the lines they must, so the rest of the file
//! (comments, odd entries, line endings, a UTF-8 BOM) is preserved byte for byte.

/// A track's library-relative path, as it appears in playlists, from the path Jellyfin reports for it.
/// `None` when the track is not inside one of the library prefixes (for example an incoming/sandbox track).
pub fn library_entry(path: &str, prefixes: &[String]) -> Option<String> {
    let p = path.replace('\\', "/");
    prefixes.iter().find_map(|pre| {
        let pre = pre.replace('\\', "/");
        let pre = pre.trim_end_matches('/');
        if pre.is_empty() {
            return None;
        }
        let rel = p.strip_prefix(pre)?.strip_prefix('/')?;
        (!rel.is_empty()).then(|| rel.to_string())
    })
}

/// The comparable form of a playlist line, or `None` for blanks and comments.
fn entry_of(line: &str) -> Option<String> {
    let t = line.trim_start_matches('\u{feff}').trim();
    (!t.is_empty() && !t.starts_with('#')).then(|| t.replace('\\', "/"))
}

pub fn entries(content: &str) -> impl Iterator<Item = String> + '_ {
    content.lines().filter_map(entry_of)
}

pub fn contains(content: &str, entry: &str) -> bool {
    entries(content).any(|e| e == entry)
}

/// Append `entry`, keeping the file's line-ending style.
pub fn with_added(content: &str, entry: &str) -> String {
    let eol = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = String::with_capacity(content.len() + entry.len() + 2);
    out.push_str(content);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(eol);
    }
    out.push_str(entry);
    out.push_str(eol);
    out
}

/// Remove every line equal to `entry`; returns the new text and how many lines went.
pub fn with_removed(content: &str, entry: &str) -> (String, usize) {
    let mut removed = 0;
    let mut out = String::with_capacity(content.len());
    for line in content.split_inclusive('\n') {
        if entry_of(line).as_deref() == Some(entry) {
            removed += 1;
        } else {
            out.push_str(line);
        }
    }
    (out, removed)
}

/// Keep the first occurrence of every entry and drop the later copies; comments and blank lines stay.
pub fn with_deduped(content: &str) -> (String, usize) {
    let mut seen = std::collections::HashSet::new();
    let mut removed = 0;
    let mut out = String::with_capacity(content.len());
    for line in content.split_inclusive('\n') {
        let repeated = entry_of(line).is_some_and(|e| !seen.insert(e));
        if repeated {
            removed += 1;
        } else {
            out.push_str(line);
        }
    }
    (out, removed)
}

/// The playlist as exactly `entries`, in order: what comes before the first entry (a BOM, `#EXTM3U`, comments) and
/// the file's line-ending style stay; comments between or after entries do not (Cratarr's files have none).
pub fn with_entries(content: &str, entries: &[String]) -> String {
    let eol = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = String::with_capacity(content.len());
    for line in content.split_inclusive('\n') {
        if entry_of(line).is_some() {
            break;
        }
        out.push_str(line);
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(eol);
    }
    if out.is_empty() && content.starts_with('\u{feff}') {
        out.push('\u{feff}');
    }
    for e in entries {
        out.push_str(e);
        out.push_str(eol);
    }
    out
}

/// Every entry line with `prefix` put in front ("../" when the files move one folder down), all else kept as is.
pub fn with_prefix(content: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(content.len() + prefix.len() * 64);
    for line in content.split_inclusive('\n') {
        if entry_of(line).is_some() {
            let bom = line.starts_with('\u{feff}');
            let body = line.trim_start_matches('\u{feff}');
            if bom {
                out.push('\u{feff}');
            }
            let lead = body.len() - body.trim_start().len();
            out.push_str(&body[..lead]);
            out.push_str(prefix);
            out.push_str(&body[lead..]);
        } else {
            out.push_str(line);
        }
    }
    out
}

/// The inverse of `with_prefix`: `prefix` taken off the front of every entry line that has it, all else kept as is.
#[cfg(test)]
pub fn without_prefix(content: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(content.len());
    for line in content.split_inclusive('\n') {
        if entry_of(line).is_some() {
            let bom = line.starts_with('\u{feff}');
            let body = line.trim_start_matches('\u{feff}');
            if bom {
                out.push('\u{feff}');
            }
            let lead = body.len() - body.trim_start().len();
            out.push_str(&body[..lead]);
            out.push_str(body[lead..].strip_prefix(prefix).unwrap_or(&body[lead..]));
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Replace every line whose entry equals `from` with `to`, keeping the line's ending and a leading BOM.
pub fn with_replaced(content: &str, from: &str, to: &str) -> (String, usize) {
    let mut replaced = 0;
    let mut out = String::with_capacity(content.len() + to.len());
    for line in content.split_inclusive('\n') {
        if entry_of(line).as_deref() == Some(from) {
            replaced += 1;
            if line.starts_with('\u{feff}') {
                out.push('\u{feff}');
            }
            out.push_str(to);
            out.push_str(&line[line.trim_end_matches(['\r', '\n']).len()..]);
        } else {
            out.push_str(line);
        }
    }
    (out, replaced)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefix_goes_on_entries_only() {
        assert_eq!(with_prefix("#EXTM3U\r\nA/1.mp3\r\n\r\nB/2.mp3", "../"), "#EXTM3U\r\n../A/1.mp3\r\n\r\n../B/2.mp3");
        assert_eq!(with_prefix("\u{feff}A/1.mp3\n", "../"), "\u{feff}../A/1.mp3\n");
    }

    #[test]
    fn a_whole_list_keeps_the_header_line_endings_and_bom() {
        let e = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(with_entries("a.mp3\nb.mp3\n", &e(&["b.mp3", "c.mp3"])), "b.mp3\nc.mp3\n");
        assert_eq!(with_entries("#EXTM3U\r\na.mp3\r\n", &e(&["c.mp3"])), "#EXTM3U\r\nc.mp3\r\n");
        assert_eq!(with_entries("\u{feff}a.mp3\n", &e(&["c.mp3"])), "\u{feff}c.mp3\n");
        assert_eq!(with_entries("", &e(&["c.mp3"])), "c.mp3\n");
        assert_eq!(with_entries("#EXTM3U\na.mp3\n", &[]), "#EXTM3U\n");
    }

    #[test]
    fn deduping_keeps_the_first_copy_comments_and_endings() {
        let c = "#c\nA/1.mp3\r\nB/2.mp3\nA\\1.mp3\n\nB/2.mp3";
        let (out, n) = with_deduped(c);
        assert_eq!(n, 2, "the backslash copy of A/1.mp3 and the second B/2.mp3");
        assert_eq!(out, "#c\nA/1.mp3\r\nB/2.mp3\n\n");
        assert_eq!(with_deduped("A/1.mp3\n"), ("A/1.mp3\n".to_string(), 0));
    }

    #[test]
    fn replacing_keeps_everything_else_and_the_line_endings() {
        let c = "#c\nA/1.mp3\r\nB/2.mp3\nA/1.mp3";
        let (out, n) = with_replaced(c, "A/1.mp3", "A/1.flac");
        assert_eq!(n, 2);
        assert_eq!(out, "#c\nA/1.flac\r\nB/2.mp3\nA/1.flac");
    }

    #[test]
    fn replacing_an_absent_entry_changes_nothing() {
        let c = "A/1.mp3\n";
        assert_eq!(with_replaced(c, "X/9.mp3", "X/9.flac"), (c.to_string(), 0));
    }

    #[test]
    fn replacing_keeps_a_bom_and_matches_across_backslashes() {
        let (out, n) = with_replaced("\u{feff}A\\1.mp3\nB/2.mp3\n", "A/1.mp3", "A/1.flac");
        assert_eq!((out.as_str(), n), ("\u{feff}A/1.flac\nB/2.mp3\n", 1));
    }

    fn p(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_prefix_comes_off_exactly_as_it_went_on() {
        let text = "\u{feff}#EXTM3U\r\n#EXTINF:1,A\r\nA/01.mp3\r\n  B/02.mp3\r\n\r\n../incoming/C/03.flac\r\n";
        assert_eq!(without_prefix(&with_prefix(text, "../"), "../"), text);
    }

    #[test]
    fn library_entry_strips_the_first_matching_prefix() {
        let pre = p(&["/data/media/Music", "/Volume1/media/Music"]);
        assert_eq!(
            library_entry("/data/media/Music/Artist - Album (2020)/01 Artist - Song.mp3", &pre).as_deref(),
            Some("Artist - Album (2020)/01 Artist - Song.mp3")
        );
        assert_eq!(
            library_entry("/Volume1/media/Music/A/B.flac", &pre).as_deref(),
            Some("A/B.flac")
        );
    }

    #[test]
    fn tracks_outside_the_library_have_no_entry() {
        let pre = p(&["/data/media/Music"]);
        assert_eq!(library_entry("/data/media/lidarr-music/A/B.flac", &pre), None);
        assert_eq!(library_entry("/data/media/Music-old/A/B.flac", &pre), None);
        assert_eq!(library_entry("/data/media/Music", &pre), None);
        assert_eq!(library_entry("/data/media/Music/", &pre), None);
        assert_eq!(library_entry("/x/y.flac", &[]), None);
        assert_eq!(library_entry("/x/y.flac", &p(&["", "/"])), None);
    }

    #[test]
    fn backslashes_and_trailing_slashes_are_tolerated() {
        assert_eq!(library_entry(r"\data\media\Music\A\B.mp3", &p(&["/data/media/Music/"])).as_deref(), Some("A/B.mp3"));
    }

    #[test]
    fn entries_skip_comments_and_blanks_and_a_bom() {
        let c = "\u{feff}#EXTM3U\n\nA/1.mp3\r\n  B/2.mp3  \n# note\nC\\3.mp3\n";
        assert_eq!(entries(c).collect::<Vec<_>>(), vec!["A/1.mp3", "B/2.mp3", "C/3.mp3"]);
        assert!(contains(c, "B/2.mp3"));
        assert!(!contains(c, "note"));
    }

    #[test]
    fn adding_appends_with_the_files_own_line_endings() {
        assert_eq!(with_added("A/1.mp3\n", "B/2.mp3"), "A/1.mp3\nB/2.mp3\n");
        assert_eq!(with_added("A/1.mp3\r\n", "B/2.mp3"), "A/1.mp3\r\nB/2.mp3\r\n");
        assert_eq!(with_added("A/1.mp3", "B/2.mp3"), "A/1.mp3\nB/2.mp3\n", "a missing final newline is repaired");
        assert_eq!(with_added("", "B/2.mp3"), "B/2.mp3\n");
    }

    #[test]
    fn removing_drops_every_copy_and_nothing_else() {
        let c = "#c\nA/1.mp3\nB/2.mp3\r\nA/1.mp3\nC/3.mp3";
        let (out, n) = with_removed(c, "A/1.mp3");
        assert_eq!(n, 2);
        assert_eq!(out, "#c\nB/2.mp3\r\nC/3.mp3");
    }

    #[test]
    fn removing_something_absent_changes_nothing() {
        let c = "A/1.mp3\nB/2.mp3\n";
        assert_eq!(with_removed(c, "X/9.mp3"), (c.to_string(), 0));
    }

    #[test]
    fn removal_matches_across_backslash_and_whitespace_differences() {
        let (out, n) = with_removed("  A\\1.mp3  \nB/2.mp3\n", "A/1.mp3");
        assert_eq!((out.as_str(), n), ("B/2.mp3\n", 1));
    }

    #[test]
    fn odd_existing_lines_survive_untouched() {
        let c = "Owiny Sigoma Band - Power Punch!!! (2013)/\nA/1.mp3\n";
        let (out, _) = with_removed(c, "A/1.mp3");
        assert_eq!(out, "Owiny Sigoma Band - Power Punch!!! (2013)/\n");
    }
}
