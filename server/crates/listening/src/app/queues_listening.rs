//! A song heard, as the Subsonic apps' plays and Playing's reports count it (moved from `serve::queues`, which keeps its name):
//! below the HTTP handlers, as Playing calls it.

use std::sync::Arc;

use crate::serve::Serve;

/// A song heard (decision 0064): through the one way in for listening (decision 0068).
pub async fn record(serve: &Arc<Serve>, user: &str, song: &str, heard: &crate::userdata::Heard) -> anyhow::Result<()> {
    crate::app::listening::heard(serve, user, song, heard).await.map(|_| ())
}
