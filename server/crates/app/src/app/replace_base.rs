//! The catalogue rows after a Replace from someone's own copy, below the areas whose tests make them (moved from
//! `incoming::replace`, which keeps its name).

use anyhow::Result;

use crate::journal::Event;

/// The catalogue rows after a Replace from someone's own copy's files (`finish_from_copy`), safe to repeat: the main library's old
/// rows into its trash, marked trashed with the token, letting go of their tracks (as Replace's old files do); the copies' rows made
/// from the own copy's (`Catalogue::copy_rows`: on the same tracks, read and analysed), each shared as its copy is (`shared`, in
/// the copies' order). The own copy's rows as they were (OQ-E). Blocking.
pub(crate) fn rows_of_copy(c: &crate::catalogue::Catalogue, event: &Event, shared: &[bool]) -> Result<()> {
    let Event::AlbumReplacedFromCopy { copies, replaced, token, .. } = event else { return Ok(()) };
    for r in replaced {
        if !c.has_row(r.to.0, &r.to.1) {
            c.move_into_trash((r.from.0, &r.from.1), (r.to.0, &r.to.1), token)?;
        }
    }
    c.set_tracks(&replaced.iter().map(|r| (r.to.clone(), None)).collect::<Vec<_>>())?;
    let rows: Vec<((i64, String), (i64, String), bool)> = copies.iter().enumerate().map(|(i, m)| (m.from.clone(), m.to.clone(), shared.get(i).copied().unwrap_or(false))).collect();
    c.copy_rows(&rows)?;
    Ok(())
}
