//! `.m3u` text read: one library-relative path per line (`Folder/File.mp3`); `#` lines are comments. Cratarr's own
//! playlists are kept in its database (decision 0101); their `.m3u` files of 0.15, and the media servers' copies, are
//! read as this.

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

#[cfg(test)]
mod tests {
    use super::*;

    fn p(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
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
    }
}
