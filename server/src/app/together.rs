//! Sync play (decision 0037), step 1: measuring, before any group exists. Cratarr's time, which each player keeps its
//! clock's offset to (a timed ping, the shortest round trip winning), and samples: what each player plays at a moment
//! of Cratarr's time, asked of all at once and compared, so how far apart two players on the same track are is known.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Cratarr's time: epoch milliseconds, to the microsecond.
pub fn now_ms() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

/// What a player plays at a moment of Cratarr's time, as it says.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// The measurement it answers.
    pub id: u64,
    #[serde(default)]
    pub entry: Option<String>,
    /// The track (source/id).
    #[serde(default)]
    pub track: Option<String>,
    #[serde(default)]
    pub playing: bool,
    /// Where it is in the track: what is heard (Media3 counts the output's delay in).
    pub position_ms: f64,
    /// When, in Cratarr's time (its own clock and its offset).
    pub at_ms: f64,
    /// Its clock's offset to Cratarr's and the round trip that measured it.
    #[serde(default)]
    pub offset_ms: f64,
    #[serde(default)]
    pub rtt_ms: f64,
    /// The middle round trip of its last pings, and how far the better half's offsets lie apart: how much to trust it.
    #[serde(default)]
    pub rtt_median_ms: f64,
    #[serde(default)]
    pub offset_spread_ms: f64,
}

/// A player's sample brought to the measurement's moment.
#[derive(Debug, Clone, Serialize)]
pub struct Measured {
    pub name: String,
    pub track: Option<String>,
    pub entry: Option<String>,
    pub playing: bool,
    /// Where it was at the measurement's moment `at_ms` (a playing one moved on from its sample at the normal speed).
    pub position_ms: f64,
    pub offset_ms: f64,
    pub rtt_ms: f64,
    pub rtt_median_ms: f64,
    pub offset_spread_ms: f64,
}

/// Two players on the same track: how far apart (`a` ahead when positive).
#[derive(Debug, Clone, Serialize)]
pub struct Apart {
    pub a: String,
    pub b: String,
    pub apart_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Measure {
    /// The moment compared, in Cratarr's time.
    pub at_ms: f64,
    pub players: Vec<Measured>,
    pub apart: Vec<Apart>,
    /// Asked and not heard from in time.
    pub silent: Vec<String>,
}

/// Measurements waiting for their samples.
#[derive(Default)]
pub struct Samples {
    next: AtomicU64,
    waiting: Mutex<HashMap<u64, Vec<(String, Sample)>>>,
}

/// How long the players have to answer.
const WAIT: Duration = Duration::from_millis(1500);

impl Samples {
    /// A new measurement's id.
    pub fn open(&self) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.waiting.lock().unwrap_or_else(|p| p.into_inner()).insert(id, Vec::new());
        id
    }

    /// A player's answer; one for no measurement (late) is dropped.
    pub fn answer(&self, name: &str, s: Sample) {
        if let Some(list) = self.waiting.lock().unwrap_or_else(|p| p.into_inner()).get_mut(&s.id) {
            list.retain(|(n, _)| n != name);
            list.push((name.to_string(), s));
        }
    }

    /// The answers of `asked`, once all came or the time is up; the measurement closes.
    pub async fn wait(&self, id: u64, asked: &[String]) -> Measure {
        let until = Instant::now() + WAIT;
        loop {
            let done = self.waiting.lock().unwrap_or_else(|p| p.into_inner()).get(&id).is_none_or(|l| l.len() >= asked.len());
            if done || Instant::now() >= until {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        let got = self.waiting.lock().unwrap_or_else(|p| p.into_inner()).remove(&id).unwrap_or_default();
        compare(got, asked)
    }
}

/// The samples brought to one moment (the latest sample's) and compared pair by pair on the same entry's track.
fn compare(got: Vec<(String, Sample)>, asked: &[String]) -> Measure {
    let at = got.iter().map(|(_, s)| s.at_ms).fold(f64::MIN, f64::max).max(0.0);
    let players: Vec<Measured> = got
        .iter()
        .map(|(name, s)| Measured {
            name: name.clone(),
            track: s.track.clone(),
            entry: s.entry.clone(),
            playing: s.playing,
            position_ms: if s.playing { s.position_ms + (at - s.at_ms) } else { s.position_ms },
            offset_ms: s.offset_ms,
            rtt_ms: s.rtt_ms,
            rtt_median_ms: s.rtt_median_ms,
            offset_spread_ms: s.offset_spread_ms,
        })
        .collect();
    let mut apart = Vec::new();
    for (i, a) in players.iter().enumerate() {
        for b in &players[i + 1..] {
            if a.playing && b.playing && a.track.is_some() && a.track == b.track {
                apart.push(Apart { a: a.name.clone(), b: b.name.clone(), apart_ms: a.position_ms - b.position_ms });
            }
        }
    }
    let silent = asked.iter().filter(|n| !got.iter().any(|(g, _)| g == *n)).cloned().collect();
    Measure { at_ms: at, players, apart, silent }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(id: u64, track: &str, position_ms: f64, at_ms: f64) -> Sample {
        Sample { id, track: Some(track.into()), playing: true, position_ms, at_ms, ..Sample::default() }
    }

    #[tokio::test]
    async fn samples_are_brought_to_one_moment_and_compared_on_the_same_track() {
        let samples = Samples::default();
        let id = samples.open();
        samples.answer("tablet", s(id, "cratarr/tr-1", 10_000.0, 1_000_000.0));
        samples.answer("phone", s(id, "cratarr/tr-1", 10_030.0, 1_000_020.0));
        samples.answer("late", s(id + 1, "cratarr/tr-1", 0.0, 0.0));
        let m = samples.wait(id, &["tablet".into(), "phone".into(), "kitchen".into()]).await;
        assert_eq!(m.at_ms, 1_000_020.0);
        // The tablet at 10 000 ms, 20 ms before: 10 020 at the moment; the phone at 10 030: the phone 10 ms ahead.
        assert_eq!(m.apart.len(), 1);
        assert!((m.apart[0].apart_ms - -10.0).abs() < 1e-6, "{:?}", m.apart);
        assert_eq!(m.silent, vec!["kitchen".to_string()]);
        // Closed: a late answer goes nowhere.
        samples.answer("tablet", s(id, "cratarr/tr-1", 0.0, 0.0));
        assert!(samples.waiting.lock().unwrap().is_empty());
    }
}
