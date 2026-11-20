//! Cratarr's own tracks' tempo as a run (moved from `PlaylistService::tempo_run`): an administrator's check, the tracks'
//! proposal and a run on the one path, across the areas, so the app's.

use std::sync::Arc;

use super::{App, ToggleError};

impl App {
    /// Cratarr's own tracks' tempo halved, doubled or reset as a run on the one path (decision 0092's writers' map,
    /// 0.14): the tracks' tempos as they will be, and the run (none where nothing changes).
    pub async fn tempo_run(self: &Arc<Self>, ids: &[String], how: super::tempo::Adjust) -> Result<(Vec<super::tempo::TrackTempo>, Option<super::file_runs::Status>), ToggleError> {
        if !crate::serve::access::administering() {
            return Err(ToggleError::Forbidden("Adjusting a track's BPM is for administrators.".into()));
        }
        let (served, list) = (crate::serve::instance().ok_or_else(|| anyhow::anyhow!("Cratarr's own library is not open"))?, ids.to_vec());
        let (p, tempos) = crate::run::spawn_blocking(move || super::tempo::proposal(&served, &list, how)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        let run = (!p.files.is_empty()).then(|| super::file_runs::start(self, p));
        Ok((tempos, run))
    }
}
