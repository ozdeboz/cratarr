//! Plays told by players (decision 0033, step 4): each an event with an id of its own and the time it was played,
//! sent when the player can (late, after a time offline) and again until it is answered, counted once however often
//! it comes. Since decision 0064 a player tells every track heard, with how much of it and how it ended; a player from
//! before tells only counted plays. The ids counted are kept in the user data for 30 days from when they came, longer
//! than a player keeps an untold play, so one sent again after a restart still counts once (0.12, decision 0087).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;

use super::PlayingService;
use crate::userdata::UserData;
use crate::userdata::offline::KEEP_SECS;

/// Where the ids counted were kept before 0.12 (in the data folder): taken into the user data once, then gone.
const OLD_FILE: &str = "plays-seen.json";

/// One play, as a player tells it.
#[derive(Debug, Clone, Deserialize)]
pub struct PlayEvent {
    /// Made by the player, once per play: the same play sent again has the same id.
    pub id: String,
    pub source: String,
    pub track: String,
    /// When it was played (epoch milliseconds).
    pub at: i64,
    /// How it ended (decision 0064); absent from a player that tells only counted plays.
    #[serde(default)]
    pub ended: Option<crate::userdata::Ended>,
    /// Whether it counts as a play (0022: the player's rule); absent, it does.
    #[serde(default)]
    pub counts: Option<bool>,
    #[serde(default)]
    pub heard_secs: Option<f64>,
    #[serde(default)]
    pub length_secs: Option<f64>,
    /// Where it sounded: the player's name, and the output's own (the car's Bluetooth).
    #[serde(default)]
    pub player: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    /// What started it: `album:<id>`, `playlist:<id>`, `crate:<id>`, `radio`, `queue:<id>`.
    #[serde(default)]
    pub context: Option<String>,
}

impl PlayEvent {
    /// As the log keeps it (decision 0064).
    pub fn heard(&self) -> crate::userdata::Heard {
        crate::userdata::Heard {
            at: self.at / 1000,
            heard_secs: self.heard_secs,
            length_secs: self.length_secs,
            ended: self.ended.unwrap_or(crate::userdata::Ended::Finished),
            counts: self.counts.unwrap_or(true),
            player: self.player.clone(),
            output: self.output.clone(),
            context: self.context.clone(),
            event: Some(self.id.clone()),
            // Set as it is counted (`PlayingService::hear`): Play On knows whether its player was following then.
            followed: false,
        }
    }
}

pub struct Plays {
    /// Counted ids and when they came (epoch seconds): a cache of those kept in the user data. One writer at a time, so
    /// a play sent twice at once counts once.
    seen: tokio::sync::Mutex<HashMap<String, i64>>,
    /// Where the ids counted are kept (decision 0087); none where a test builds the app without it: in memory only.
    kept: Option<Arc<UserData>>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The ids counted before 0.12 (`plays-seen.json`) taken into the user data, kept 30 days from now; the file goes.
fn take_in_old_file(file: &Path, kept: &UserData) {
    let Ok(text) = std::fs::read_to_string(file) else { return };
    let old: HashMap<String, i64> = serde_json::from_str(&text).unwrap_or_default();
    let at = now();
    let rows: Vec<(String, i64)> = old.into_keys().map(|id| (id, at)).collect();
    match kept.add_plays_seen(&rows) {
        Ok(()) => {
            if let Err(e) = std::fs::remove_file(file) {
                tracing::warn!("{} taken in but not removed: {e}", file.display());
            }
        }
        Err(e) => tracing::warn!("the plays counted before could not be taken in: {e:#}"),
    }
}

impl Plays {
    pub fn new(data_dir: Option<&str>, kept: Option<Arc<UserData>>) -> Self {
        if let (Some(dir), Some(ud)) = (data_dir, kept.as_deref()) {
            take_in_old_file(&Path::new(dir).join(OLD_FILE), ud);
        }
        Self { seen: tokio::sync::Mutex::new(HashMap::new()), kept }
    }

    /// Counts the plays not counted before. Returns the ids the player may forget: counted now, or before. A play
    /// that cannot be counted now (its server away) is left out, so the player sends it again later.
    pub async fn take(&self, playing: &PlayingService, events: Vec<PlayEvent>) -> Vec<String> {
        self.take_with(events, |e| async move { playing.hear(&e).await.map_err(|e| e.to_string()) })
            .await
    }

    /// As `take`, counting with `count`.
    async fn take_with<F, Fut>(&self, events: Vec<PlayEvent>, count: F) -> Vec<String>
    where
        F: Fn(PlayEvent) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        let mut seen = self.seen.lock().await;
        let now = now();
        // Those not in the cache asked of the user data at once: counted before a restart, they are not counted again.
        let unknown: Vec<String> = events.iter().filter(|e| !seen.contains_key(&e.id)).map(|e| e.id.clone()).collect();
        if let (Some(ud), false) = (self.kept.clone(), unknown.is_empty()) {
            match tokio::task::spawn_blocking(move || ud.plays_seen(&unknown)).await {
                Ok(Ok(found)) => seen.extend(found.into_iter().map(|id| (id, now))),
                // Not known whether they were counted: none of them is, the player sends them again later.
                failed => {
                    tracing::warn!("the plays counted could not be read; the new ones wait: {failed:?}");
                    return events.into_iter().filter(|e| seen.contains_key(&e.id)).map(|e| e.id).collect();
                }
            }
        }
        let mut done = Vec::new();
        let mut counted = Vec::new();
        for e in events {
            if seen.contains_key(&e.id) {
                done.push(e.id);
                continue;
            }
            let (id, what) = (e.id.clone(), format!("{}/{}", e.source, e.track));
            match count(e).await {
                Ok(()) => {
                    seen.insert(id.clone(), now);
                    counted.push((id.clone(), now));
                    done.push(id);
                }
                Err(err) => tracing::warn!("a play of {what} is not counted yet: {err}"),
            }
        }
        if !counted.is_empty() {
            seen.retain(|_, at| now - *at < KEEP_SECS);
            if let Some(ud) = self.kept.clone() {
                let saved = tokio::task::spawn_blocking(move || ud.add_plays_seen(&counted)).await;
                if !matches!(saved, Ok(Ok(()))) {
                    tracing::warn!("the plays counted could not be kept: a play sent again after a restart may count twice");
                }
            }
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn play(id: &str, track: &str) -> PlayEvent {
        PlayEvent {
            id: id.into(),
            source: "cratarr".into(),
            track: track.into(),
            at: 1_792_000_000_000,
            ended: None,
            counts: None,
            heard_secs: None,
            length_secs: None,
            player: None,
            output: None,
            context: None,
        }
    }

    #[tokio::test]
    async fn a_play_sent_again_counts_once_and_one_not_counted_is_sent_again() {
        let dir = std::env::temp_dir().join(format!("cratarr-plays-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("user.sqlite");
        let plays = Plays::new(dir.to_str(), Some(Arc::new(UserData::open(Some(&file)).unwrap())));
        let counted = AtomicUsize::new(0);
        let count = |e: PlayEvent| {
            let counted = &counted;
            async move {
                if e.track == "tr-away" {
                    return Err("its server is away".to_string());
                }
                counted.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        };
        let done = plays.take_with(vec![play("a", "tr-1"), play("b", "tr-away")], count).await;
        assert_eq!(done, vec!["a".to_string()], "b is left for later");
        // The answer was lost; the player sends both again: a is not counted twice.
        let done = plays.take_with(vec![play("a", "tr-1"), play("b", "tr-away")], count).await;
        assert_eq!(done, vec!["a".to_string()]);
        assert_eq!(counted.load(Ordering::SeqCst), 1);
        // Kept over a restart: the user data opened again, the cache empty.
        drop(plays);
        let again = Plays::new(dir.to_str(), Some(Arc::new(UserData::open(Some(&file)).unwrap())));
        assert_eq!(again.take_with(vec![play("a", "tr-1")], count).await, vec!["a".to_string()]);
        assert_eq!(counted.load(Ordering::SeqCst), 1);
        drop(again);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn the_plays_counted_before_are_taken_in_from_their_old_file() {
        let dir = std::env::temp_dir().join(format!("cratarr-plays-old-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(OLD_FILE), r#"{"old":1792000000}"#).unwrap();
        let plays = Plays::new(dir.to_str(), Some(Arc::new(UserData::open(None).unwrap())));
        assert!(!dir.join(OLD_FILE).exists(), "taken in, the file goes");
        let counted = AtomicUsize::new(0);
        let count = |_: PlayEvent| {
            let counted = &counted;
            async move {
                counted.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        };
        assert_eq!(plays.take_with(vec![play("old", "tr-1"), play("new", "tr-2")], count).await, vec!["old".to_string(), "new".to_string()]);
        assert_eq!(counted.load(Ordering::SeqCst), 1, "only the new one counts");
        std::fs::remove_dir_all(&dir).ok();
    }
}
