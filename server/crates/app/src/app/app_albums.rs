//! The Library page's one list and the album actions as the app answers them: they are Incoming's (`albums::Albums`), on
//! the app as their host.

use super::App;
use super::albums::{Albums, AlbumsView, ReplaceAsk, WantCopy};
use super::{CopySheet, DeleteResult, KeepPreview, KeepResult, ReplacePreview, ToggleError};

impl App {
    /// Every server with used libraries (or only `want`): all their albums in one list, with their facts.
    pub async fn albums(&self, want: Option<&str>) -> AlbumsView {
        Albums::of(self).albums(want).await
    }

    /// Delete a copy the viewer may, into its library's trash (decision 0034). The record stays while another copy does.
    pub async fn delete_album(&self, source: &str, album: &str, from: Option<&str>) -> Result<DeleteResult, ToggleError> {
        Albums::of(self).delete_album(source, album, from).await
    }

    /// `account`'s copy of a record the library holds, in their own music (C52, T5: `Albums::copy_for`).
    pub(super) async fn copy_for(&self, account: &str, record: i64, sharing: bool) -> WantCopy {
        Albums::of(self).copy_for(account, record, sharing).await
    }

    /// What Replace would do (decision 0013), for the page to show before asking. Changes nothing.
    pub async fn replace_preview(&self, source: &str, album: &str, from: Option<&str>, to: Option<&str>, year: Option<&str>) -> Result<ReplacePreview, ToggleError> {
        Albums::of(self).replace_preview(source, album, from, to, year).await
    }

    /// Replace (decision 0013) of an Incoming copy, or (RY01) from someone's own copy, the administrators' as its route is.
    pub async fn replace_album(&self, source: &str, album: &str, ask: ReplaceAsk<'_>) -> Result<KeepResult, ToggleError> {
        Albums::of(self).replace_album(source, album, ask).await
    }

    /// Delete an album of someone's own music from the Users panel: an administrator's, only there.
    pub async fn delete_own_album(&self, source: &str, album: &str, from: Option<&str>) -> Result<DeleteResult, ToggleError> {
        Albums::of(self).delete_own_album(source, album, from).await
    }

    /// Move a copy into another used library of its server (decision 0034).
    pub async fn move_album(&self, source: &str, album: &str, from: Option<&str>, to: &str, year: Option<&str>, sheet: Option<CopySheet>) -> Result<KeepResult, ToggleError> {
        Albums::of(self).move_album(source, album, from, to, year, sheet).await
    }

    /// What that move would do.
    pub async fn move_preview(&self, source: &str, album: &str, from: Option<&str>, to: &str, year: Option<&str>) -> Result<KeepPreview, ToggleError> {
        Albums::of(self).move_preview(source, album, from, to, year).await
    }

    /// `source`'s own-music libraries as the viewer sees them: (id, name, theirs).
    pub fn own_music_seen(&self, source: &str) -> Vec<(String, String, bool)> {
        Albums::of(self).own_music_seen(source)
    }
}
