//! Plays told by players (decision 0033, step 4): each an event with an id of its own and the time it was played,
//! sent when the player can (late, after a time offline) and again until it is answered, counted once however often
//! it comes. Since decision 0064 a player tells every track heard, with how much of it and how it ended; a player from
//! before tells only counted plays. The ids counted are kept a while (`plays-seen.json`), longer than a player keeps an untold play.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

use super::PlayingService;

/// Counted ids are forgotten after this long (epoch seconds); a player gives up on a play after 30 days.
const KEEP_SECS: i64 = 60 * 24 * 3600;

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
    /// Counted ids and when they were played (epoch seconds). One writer at a time, so a play sent twice at once
    /// counts once.
    seen: tokio::sync::Mutex<HashMap<String, i64>>,
    file: Option<PathBuf>,
}

impl Plays {
    pub fn new(data_dir: Option<&str>) -> Self {
        let file = data_dir.map(|d| PathBuf::from(d).join("plays-seen.json"));
        let seen = file.as_deref().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Self { seen: tokio::sync::Mutex::new(seen), file }
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
        let mut done = Vec::new();
        let mut changed = false;
        for e in events {
            if seen.contains_key(&e.id) {
                done.push(e.id);
                continue;
            }
            let (id, at, what) = (e.id.clone(), e.at / 1000, format!("{}/{}", e.source, e.track));
            match count(e).await {
                Ok(()) => {
                    seen.insert(id.clone(), at);
                    done.push(id);
                    changed = true;
                }
                Err(err) => tracing::warn!("a play of {what} is not counted yet: {err}"),
            }
        }
        if changed {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
            seen.retain(|_, at| now - *at < KEEP_SECS);
            if let (Some(file), Ok(body)) = (self.file.clone(), serde_json::to_string(&*seen)) {
                let tmp = file.with_extension("json.tmp");
                let saved = tokio::task::spawn_blocking(move || std::fs::write(&tmp, body).and_then(|()| std::fs::rename(&tmp, &file))).await;
                if !matches!(saved, Ok(Ok(()))) {
                    tracing::warn!("the plays counted could not be saved: a play sent again after a restart may count twice");
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
        let plays = Plays::new(dir.to_str());
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
        // Kept over a restart.
        let again = Plays::new(dir.to_str());
        assert_eq!(again.take_with(vec![play("a", "tr-1")], count).await, vec!["a".to_string()]);
        assert_eq!(counted.load(Ordering::SeqCst), 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
