//! A device answering for itself, for the app (PREVIEW2's D18 to D20): `index::device` through UniFFI, so a crate, a Sounds like and a
//! pick are the page's answer with no page open, and what a player decides of its queue, as `radio` says it. Kotlin decides nothing
//! here: it hands in what it keeps and does what it is told.

use std::sync::{Arc, Mutex};

use cratarr_rules::index::device::Device;
use cratarr_rules::index::{Index, Marks, Sounds};
use cratarr_rules::radio::{self, KeptTrack, Queued};
use cratarr_rules::{Lane, Rng};

/// What the app asks of what it keeps, held in memory for as long as it asks: see `DeviceAnswers.kt`.
#[derive(uniffi::Object)]
pub struct RulesDevice(Mutex<Device>);

#[uniffi::export]
impl RulesDevice {
    /// What is asked, as JSON (`Device::ask`): the answer as JSON, none where it has none.
    pub fn ask(&self, asked: String) -> Option<String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).ask(&asked)
    }
}

/// A device over what is kept; none where the index cannot be read. Sounds and an account's own are read if they can be: without them
/// it answers what it can.
#[uniffi::export]
pub fn device_open(index: Vec<u8>, sounds: Option<Vec<u8>>, own: Option<Vec<u8>>) -> Option<Arc<RulesDevice>> {
    let index = Index::load(&index).ok()?;
    let sounds = sounds.and_then(|bytes| Sounds::load(&bytes).ok());
    let marks = own.and_then(|bytes| Marks::load(&bytes).ok()).unwrap_or_default();
    Some(Arc::new(RulesDevice(Mutex::new(Device::new(index, sounds.as_ref(), &marks)))))
}

/// Which lane a queue's track lies in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum QueueLane {
    Own,
    Next,
    Radio,
    Autoplay,
}

impl From<QueueLane> for Lane {
    fn from(lane: QueueLane) -> Self {
        match lane {
            QueueLane::Own => Self::Own,
            QueueLane::Next => Self::Next,
            QueueLane::Radio => Self::Radio,
            QueueLane::Autoplay => Self::Autoplay,
        }
    }
}

/// A track of a queue, as repeat all reads it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QueueTrack {
    pub key: String,
    pub place: Option<u32>,
    pub lane: QueueLane,
}

/// A track kept on the device.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct KeptPick {
    pub key: String,
    pub artist: Option<String>,
}

/// How many tracks Autoplay appends.
#[uniffi::export]
pub fn autoplay_batch() -> u32 {
    radio::AUTOPLAY_BATCH as u32
}

/// Whether Autoplay asks now.
#[uniffi::export]
pub fn autoplay_due(count: u32, playing: u32) -> bool {
    radio::autoplay_due(count as usize, playing as usize)
}

/// Where Radio's inserts go for the track at `playing`; none where they would not.
#[uniffi::export]
pub fn radio_place(lanes: Vec<QueueLane>, playing: u32) -> Option<u32> {
    let lanes: Vec<Lane> = lanes.into_iter().map(Into::into).collect();
    radio::radio_place(&lanes, playing as usize).map(|at| at as u32)
}

/// The next round of repeat all, as places in `queue`; none while the track at `playing` is not the last.
#[uniffi::export]
pub fn next_round(queue: Vec<QueueTrack>, playing: u32, shuffled: bool, draw: u64) -> Vec<u32> {
    let queue: Vec<Queued> = queue.into_iter().map(|t| Queued { key: t.key, place: t.place, lane: t.lane.into() }).collect();
    radio::next_round(&queue, playing as usize, shuffled, &mut Rng::seeded(draw)).into_iter().map(|i| i as u32).collect()
}

/// What comes next with no index on the device, as places in `kept`.
#[uniffi::export]
pub fn picks_from_kept(kept: Vec<KeptPick>, queued: Vec<String>, artists: Vec<String>, any: bool, count: u32, draw: u64) -> Vec<u32> {
    let kept: Vec<KeptTrack> = kept.into_iter().map(|t| KeptTrack { key: t.key, artist: t.artist }).collect();
    radio::from_kept(&kept, &queued, &artists, any, count as usize, &mut Rng::seeded(draw)).into_iter().map(|i| i as u32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHOLE: &[u8] = include_bytes!("../../rules/tests/vectors/index/whole.bin");
    const SOUNDS: &[u8] = include_bytes!("../../rules/tests/vectors/index/sounds.bin");

    #[test]
    fn a_device_opens_on_what_is_kept_and_on_nothing_else() {
        assert!(device_open(WHOLE.to_vec(), Some(SOUNDS.to_vec()), None).is_some());
        assert!(device_open(WHOLE.to_vec(), Some(vec![1, 2, 3]), Some(vec![4])).is_some(), "without what cannot be read");
        assert!(device_open(vec![1, 2, 3], None, None).is_none(), "no index, nothing to answer from");
    }

    #[test]
    fn what_is_asked_goes_through_as_it_is() {
        let device = device_open(WHOLE.to_vec(), None, None).unwrap();
        assert_eq!(device.ask(r#"{"ask":"rows","id":"genre:none","offset":0,"limit":1,"now":1}"#.to_string()).map(|a| a.contains("\"total\":0")), Some(true));
        assert_eq!(device.ask("not json".to_string()), None);
    }

    #[test]
    fn a_queue_goes_by_the_rules_crates_word() {
        assert!(autoplay_due(3, 0));
        assert!(!autoplay_due(4, 0));
        assert_eq!(autoplay_batch(), 5);
        assert_eq!(radio_place(vec![QueueLane::Own, QueueLane::Next, QueueLane::Own], 0), Some(2));
        assert_eq!(radio_place(vec![QueueLane::Own, QueueLane::Radio], 0), None);
        let queue = vec![QueueTrack { key: "b".into(), place: Some(1), lane: QueueLane::Own }, QueueTrack { key: "a".into(), place: Some(0), lane: QueueLane::Own }];
        assert_eq!(next_round(queue, 1, false, 1), [1, 0]);
        let kept = vec![KeptPick { key: "a".into(), artist: Some("X".into()) }, KeptPick { key: "b".into(), artist: Some("Y".into()) }];
        assert_eq!(picks_from_kept(kept, vec!["a".into()], vec![], false, 5, 1), [1]);
    }
}
