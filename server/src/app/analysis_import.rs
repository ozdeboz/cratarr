//! Analyses made elsewhere (decision 0035: the ThinkBook's one-time pass), taken into the catalogue: JSON lines, one
//! file each, `{"rel":…,"size":…,"mtime":…,"analysis":{…}}`, `rel` relative to the library folder the import is for,
//! with forward slashes. A line is taken only while the catalogue's file is as it was analysed (the same size and
//! mtime); the others are counted, not kept.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::catalogue::Catalogue;

/// What an import did with its lines.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct ImportCounts {
    /// Taken.
    pub stored: u64,
    /// The file is there, but not as it was analysed (another size or mtime).
    pub stale: u64,
    /// No such file in the folder (not yet seen, or gone).
    pub unknown: u64,
    /// Not a line of this kind.
    pub bad: u64,
}

#[derive(Deserialize)]
struct Line {
    rel: String,
    size: u64,
    mtime: i64,
    analysis: crate::analysis::Analysis,
}

/// A batch of whole lines (their newlines included or not), checked against the catalogue and kept in one go. Blocking.
pub fn import_batch(cat: &Catalogue, folder: i64, lines: &[Vec<u8>], counts: &mut ImportCounts) -> Result<()> {
    let mut keep = Vec::new();
    for raw in lines {
        let text = raw.trim_ascii();
        if text.is_empty() {
            continue;
        }
        let Ok(line) = serde_json::from_slice::<Line>(text) else {
            counts.bad += 1;
            continue;
        };
        let rel = line.rel.trim_start_matches("./").trim_start_matches('/').to_string();
        match cat.file_stamp(folder, &rel) {
            None => counts.unknown += 1,
            Some((size, mtime)) if size == line.size && mtime == line.mtime => keep.push((folder, rel, size, mtime, line.analysis)),
            Some(_) => counts.stale += 1,
        }
    }
    cat.store_analyses(&keep)?;
    counts.stored += keep.len() as u64;
    Ok(())
}

/// The folder an import is for, by its path as the server sees it (`/media/Music`, a trailing slash or not).
pub fn folder_of(cat: &Catalogue, path: &str) -> Option<i64> {
    let want = path.trim_end_matches('/');
    cat.folders().iter().position(|p| p.to_string_lossy().trim_end_matches('/') == want).map(|i| i as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_taken_only_while_its_file_is_as_it_was() {
        let d = std::env::temp_dir().join(format!("cratarr-import-{}", std::process::id()));
        std::fs::create_dir_all(d.join("A/B")).unwrap();
        std::fs::write(d.join("A/B/01 x.flac"), b"some sound").unwrap();
        let cat = Catalogue::open(None, vec![d.clone()]).unwrap();
        cat.walk_all();
        cat.pretend_read_for_tests();
        assert_eq!(folder_of(&cat, &format!("{}/", d.display())), Some(0));
        let (size, mtime) = cat.file_stamp(0, "A/B/01 x.flac").unwrap();
        let a = serde_json::json!({
            "version": crate::analysis::VERSION, "duration_secs": 200.0, "loudness_lufs": -12.0, "track_gain_db": -6.0,
            "track_peak": 0.98, "blocks": [[520, 30]], "bpm": 128.0, "bpm_confidence": 0.9, "waveform": vec![9; 512]
        });
        let line = |rel: &str, size: u64, mtime: i64| serde_json::json!({ "rel": rel, "size": size, "mtime": mtime, "analysis": a }).to_string().into_bytes();
        let lines = vec![
            line("A/B/01 x.flac", size, mtime),
            line("A/B/01 x.flac", size + 1, mtime),
            line("A/B/02 gone.flac", 1, 1),
            b"not json".to_vec(),
            b"   ".to_vec(),
        ];
        let mut counts = ImportCounts::default();
        import_batch(&cat, 0, &lines, &mut counts).unwrap();
        assert_eq!(counts, ImportCounts { stored: 1, stale: 1, unknown: 1, bad: 1 });
        assert_eq!(cat.analysis_of(0, "A/B/01 x.flac").unwrap().track_gain_db, -6.0);
        std::fs::remove_dir_all(&d).ok();
    }
}
