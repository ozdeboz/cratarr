//! `renumber`'s file name, in base: a track's file name as the sheets show it, which the words' writer names too.
//! `renumber` re-exports it.

use std::path::Path;

pub(super) fn file_name(rel: &str) -> String {
    Path::new(rel).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| rel.to_string())
}
