//! The listening history (decision 0064): what an account heard, as the features ask it. The log (`userdata`'s
//! `plays`) is read here and nowhere else.

use anyhow::Result;

use crate::userdata::{Ended, Logged, UserData};

/// Older listening counts less: half as much this long before now.
pub const HALF_LIFE_SECS: f64 = 3.0 * 24.0 * 3600.0;

/// Each song heard in `rows`, with how much it counts at `now`: every listen not skipped adds one, halved every
/// `HALF_LIFE_SECS` back; a song only skipped (or stopped on before it counted) is there with nothing, so what asks can
/// leave it out too. The most first.
pub fn weighted(rows: &[Logged], now: i64) -> Vec<(String, f32)> {
    let mut by_song: Vec<(String, f32)> = Vec::new();
    let mut at: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for r in rows {
        let listened = r.counted || r.ended == Ended::Finished;
        let w = if listened { 0.5f64.powf((now - r.at).max(0) as f64 / HALF_LIFE_SECS) as f32 } else { 0.0 };
        match at.get(r.song.as_str()) {
            Some(&k) => by_song[k].1 += w,
            None => {
                at.insert(&r.song, by_song.len());
                by_song.push((r.song.clone(), w));
            }
        }
    }
    by_song.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    by_song
}

/// Sounds like your week's seed: the songs heard since `since`, weighted (`weighted`). None while the log does not
/// reach back that far (it starts empty, decision 0064): the caller goes by the last plays then.
pub fn since(userdata: &UserData, user: &str, since: i64, now: i64) -> Result<Option<Vec<(String, f32)>>> {
    match userdata.heard_from(user)? {
        Some(first) if first <= since => Ok(Some(weighted(&userdata.heard_since(user, since)?, now))),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(song: &str, at: i64, ended: Ended, counted: bool) -> Logged {
        Logged { song: song.into(), at, heard_secs: None, length_secs: None, ended, counted, context: None }
    }

    #[test]
    fn heard_often_and_lately_counts_most_and_a_skip_nothing() {
        let now = 1_000_000;
        let day = 86_400;
        let rows = [
            row("old", now - 6 * day, Ended::Finished, true),
            row("often", now - day, Ended::Finished, true),
            row("often", now - day, Ended::Finished, true),
            row("often", now - day, Ended::Stopped, true),
            row("skipped", now - 60, Ended::Skipped, false),
            row("lately", now - 60, Ended::Finished, true),
        ];
        let w = weighted(&rows, now);
        let names: Vec<&str> = w.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(names, ["often", "lately", "old", "skipped"]);
        assert_eq!(w[3].1, 0.0, "a skip adds nothing");
        assert!((w[1].1 - 1.0).abs() < 0.01, "a minute ago: about one");
        assert!((w[2].1 - 0.25).abs() < 0.01, "six days back: two half-lives");
    }
}
