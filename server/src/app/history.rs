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

/// One's own listening, for what reads a taste from it (the taste profile, Sounds like your week): what was heard while
/// following another listener left out (the owner, 2026-10-30). The history itself shows it all.
pub fn own(rows: Vec<Logged>) -> Vec<Logged> {
    rows.into_iter().filter(|r| !r.followed).collect()
}

/// Sounds like your week's seed: the songs heard since `since`, weighted (`weighted`). None while the log does not
/// reach back that far (it starts empty, decision 0064): the caller goes by the last plays then.
pub fn since(userdata: &UserData, user: &str, since: i64, now: i64) -> Result<Option<Vec<(String, f32)>>> {
    match userdata.heard_from(user)? {
        // What played while they slept is not theirs (the owner, 2026-10-23).
        Some(first) if first <= since => Ok(Some(weighted(&own(userdata.heard_since(user, since)?), now))),
        _ => Ok(None),
    }
}

/// The periods Insights looks back over (decision 0065; one for all of Insights, the owner 2026-10-23: the past week,
/// month or year, or all), and how Listening groups them: days for a week and a month, weeks for a year and for all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    Week,
    Month,
    Year,
    All,
}

/// How far back All reaches at most (weeks).
const ALL_WEEKS: i64 = 5 * 52;

impl Period {
    /// How many buckets, and how long each (days); All's weeks from the log's first (`log_from`, a local day) on.
    fn buckets(self, today: i64, log_from: Option<i64>) -> (i64, i64) {
        match self {
            Period::Week => (7, 1),
            Period::Month => (30, 1),
            Period::Year => (52, 7),
            Period::All => (log_from.map_or(1, |f| ((today - f).div_euclid(7) + 1).clamp(1, ALL_WEEKS)), 7),
        }
    }

    /// Where the period begins (epoch s) for what counts it but Listening (Collection's albums added since): rolling,
    /// the past 7, 30 or 365 days; None for all.
    pub fn since(self, now: i64) -> Option<i64> {
        match self {
            Period::Week => Some(now - 7 * DAY),
            Period::Month => Some(now - 30 * DAY),
            Period::Year => Some(now - 365 * DAY),
            Period::All => None,
        }
    }
}

/// What Listening needs of a song: who, which album, its genre.
#[derive(Debug, Clone, Default)]
pub struct SongFacts {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_id: String,
    pub genre: Option<String>,
}

/// One bucket of listening time: from `start` (epoch s, local midnight), seconds heard.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Bucket {
    pub start: i64,
    pub secs: f64,
}

/// A top artist, album or track: its name (and by whom), how often it was listened to, how long.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Top {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    pub plays: u32,
    pub secs: f64,
}

/// Insights' Listening for a period (decision 0065).
#[derive(Debug, Clone, serde::Serialize)]
pub struct Listening {
    pub period: Period,
    /// The first moment the period covers (epoch s).
    pub from: i64,
    /// Where the log begins (epoch s); none while it is empty. Before it, nothing is known.
    pub log_from: Option<i64>,
    pub total_secs: f64,
    /// Listens that count (not skipped), as the counts do.
    pub plays: u32,
    pub time: Vec<Bucket>,
    /// Taste over time, further back than the period (the owner, 2026-10-23: "one line per genre, the way last.fm does
    /// it").
    pub taste: Taste,
    pub artists: Vec<Top>,
    pub albums: Vec<Top>,
    pub tracks: Vec<Top>,
}

/// Taste over time: the past weeks (a week chosen), months (a month) or years (a year); per step, each top genre's
/// share of the time listened then (0 to 1). The top genres are those heard longest over all the steps, at most five.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Taste {
    /// "week", "month" or "year".
    pub step: &'static str,
    /// Each step's start (epoch s, local midnight), oldest first.
    pub starts: Vec<i64>,
    pub genres: Vec<String>,
    /// `shares[i][j]`: step i's share of genre j; 0 where nothing was heard then.
    pub shares: Vec<Vec<f64>>,
}

impl Period {
    /// Taste over time's steps within the period (the owner, 2026-10-23: bound by it, as everything else): a week's
    /// days, a month's weeks, a year's months, all's years since the log began (`first`, a local day).
    fn taste(self, today: i64, first: Option<i64>) -> (i64, &'static str) {
        match self {
            Period::Week => (7, "day"),
            Period::Month => (5, "week"),
            Period::Year => (12, "month"),
            Period::All => (first.map_or(1, |f| (civil(today).0 - civil(f).0 + 1).clamp(1, 10)), "year"),
        }
    }

    /// The earliest moment this period's figures may need (epoch s), whatever the time zone: its taste's first step.
    pub fn reaches_back(self, now: i64) -> i64 {
        now - match self {
            Period::Week => 7 + 1,
            Period::Month => 5 * 7 + 7,
            Period::Year => 12 * 31 + 31,
            Period::All => ALL_WEEKS * 7 + 7,
        } * DAY
    }
}

/// The civil date (year, month 1–12, day) of a day since 1970-01-01 (Howard Hinnant's algorithm).
pub(crate) fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The day since 1970-01-01 of a civil date (its inverse).
fn days_of(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Taste over time from the log's rows, the steps of `period`'s taste up to `today` (a local day); `first`: the log's
/// first day.
fn taste(rows: &[Logged], facts: &std::collections::HashMap<String, SongFacts>, period: Period, today: i64, tz: i64, first: Option<i64>) -> Taste {
    use std::collections::HashMap;
    let (count, step) = period.taste(today, first);
    // Each step's first day, oldest first: days, Mondays, the 1st of a month, the 1st of January.
    let (y, m, _) = civil(today);
    let firsts: Vec<i64> = (0..count)
        .rev()
        .map(|back| match step {
            "day" => today - back,
            "week" => today - (today + 3).rem_euclid(7) - 7 * back,
            "month" => {
                let months = y * 12 + i64::from(m) - 1 - back;
                days_of(months.div_euclid(12), months.rem_euclid(12) as u32 + 1, 1)
            }
            _ => days_of(y - back, 1, 1),
        })
        .collect();
    let mut per_step: Vec<HashMap<String, f64>> = vec![HashMap::new(); firsts.len()];
    let mut overall: HashMap<String, f64> = HashMap::new();
    for r in rows {
        let day = (r.at + tz).div_euclid(DAY);
        let Some(i) = firsts.iter().rposition(|&f| day >= f) else { continue };
        if day > today {
            continue;
        }
        let secs = heard_secs(r);
        let genre = facts.get(&r.song).and_then(|f| f.genre.clone()).filter(|g| !g.is_empty()).unwrap_or_else(|| "No genre".into());
        *per_step[i].entry(genre.clone()).or_default() += secs;
        *overall.entry(genre).or_default() += secs;
    }
    let mut ranked: Vec<(String, f64)> = overall.into_iter().filter(|(g, _)| g != "No genre").collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let genres: Vec<String> = ranked.into_iter().take(GENRES).map(|(g, _)| g).collect();
    let shares = per_step
        .iter()
        .map(|m| {
            let all: f64 = m.values().sum();
            genres.iter().map(|g| if all > 0.0 { m.get(g).copied().unwrap_or(0.0) / all } else { 0.0 }).collect()
        })
        .collect();
    Taste { step, starts: firsts.iter().map(|&f| f * DAY - tz).collect(), genres, shares }
}

/// How long a logged listen was heard: what the player said, else a finished track's length.
fn heard_secs(r: &Logged) -> f64 {
    r.heard_secs.or(if r.ended == Ended::Finished { r.length_secs } else { None }).unwrap_or(0.0).max(0.0)
}

const DAY: i64 = 86_400;
/// At most this many in each top list.
const TOP: usize = 10;
/// At most this many genres in taste over time (five lines: a legend, the categorical slots in order).
const GENRES: usize = 5;

/// Listening over `period` up to `now` from the log's `rows` (since `from`), each song's facts known by id; days begin
/// at local midnight, `tz` seconds east of UTC; a year's weeks on Mondays.
pub fn listening(rows: &[Logged], facts: &std::collections::HashMap<String, SongFacts>, period: Period, now: i64, tz: i64, log_from: Option<i64>) -> Listening {
    use std::collections::HashMap;
    let today = (now + tz).div_euclid(DAY);
    let (count, days) = period.buckets(today, log_from.map(|f| (f + tz).div_euclid(DAY)));
    // The last bucket holds today (a year's: this week, from its Monday; 1970-01-01 was a Thursday).
    let last = if days == 7 { today - (today + 3).rem_euclid(7) } else { today };
    let first = last - (count - 1) * days;
    let start = |day: i64| day * DAY - tz;
    let from = start(first);
    let mut time: Vec<Bucket> = (0..count).map(|i| Bucket { start: start(first + i * days), secs: 0.0 }).collect();

    let bucket_of = |at: i64| -> Option<usize> {
        let day = (at + tz).div_euclid(DAY);
        (day >= first && day < last + days).then(|| ((day - first) / days) as usize)
    };
    let (mut artists, mut albums, mut tracks): (HashMap<String, Top>, HashMap<String, Top>, HashMap<String, Top>) = Default::default();
    let (mut total, mut plays) = (0.0, 0u32);
    for r in rows {
        let Some(b) = bucket_of(r.at) else { continue };
        let secs = heard_secs(r);
        let listened = r.ended != Ended::Skipped && (r.counted || r.ended == Ended::Finished);
        time[b].secs += secs;
        total += secs;
        if listened {
            plays += 1;
        }
        let f = facts.get(&r.song).cloned().unwrap_or_default();
        let add = |map: &mut HashMap<String, Top>, id: &str, name: &str, by: Option<&str>| {
            if id.is_empty() || name.is_empty() {
                return;
            }
            let t = map.entry(id.to_string()).or_insert_with(|| Top { id: id.to_string(), name: name.to_string(), by: by.map(str::to_string), plays: 0, secs: 0.0 });
            t.secs += secs;
            if listened {
                t.plays += 1;
            }
        };
        add(&mut artists, &f.artist, &f.artist, None);
        add(&mut albums, &f.album_id, &f.album, Some(&f.artist));
        add(&mut tracks, &r.song, &f.title, Some(&f.artist));
    }
    let top = |map: HashMap<String, Top>| {
        let mut v: Vec<Top> = map.into_values().filter(|t| t.plays > 0).collect();
        v.sort_by(|a, b| b.plays.cmp(&a.plays).then(b.secs.total_cmp(&a.secs)).then_with(|| a.name.cmp(&b.name)));
        v.truncate(TOP);
        v
    };
    let taste = taste(rows, facts, period, today, tz, log_from.map(|f| (f + tz).div_euclid(DAY)));
    Listening { period, from, log_from, total_secs: total, plays, time, taste, artists: top(artists), albums: top(albums), tracks: top(tracks) }
}

/// The log since `from` and where it begins, for Listening (the log is read here only). An import counts as the log
/// here: `heard_from` is where players began telling (imports left out), and history from ListenBrainz before it is
/// still history heard (0064, amended).
pub fn heard(userdata: &UserData, user: &str, from: i64) -> Result<(Vec<Logged>, Option<i64>)> {
    let rows = userdata.heard_since(user, from)?;
    let first = rows.iter().map(|r| r.at).min();
    let told = userdata.heard_from(user)?;
    Ok((rows, first.into_iter().chain(told).min()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(song: &str, at: i64, ended: Ended, counted: bool) -> Logged {
        Logged { song: song.into(), at, heard_secs: None, length_secs: None, ended, counted, context: None, followed: false }
    }

    #[test]
    fn what_was_heard_while_following_is_history_but_not_ones_taste() {
        let mut along = row("b", 20, Ended::Finished, true);
        along.followed = true;
        let rows = vec![row("a", 10, Ended::Finished, true), along];
        assert_eq!(own(rows.clone()).iter().map(|r| r.song.as_str()).collect::<Vec<_>>(), ["a"], "the taste reads one's own");
        assert_eq!(weighted(&rows, 30).len(), 2, "the history keeps both");
    }

    #[test]
    fn listening_adds_time_by_day_and_ranks_what_was_listened_to() {
        let day = 86_400;
        // Noon on a Wednesday (1970-01-07 is one; 2,000 weeks later still a Wednesday), in UTC.
        let now = 2_000 * 7 * day + 6 * day + 12 * 3600;
        let mut facts = std::collections::HashMap::new();
        let fact = |t: &str, a: &str, g: &str| SongFacts { title: t.into(), artist: a.into(), album: format!("{a} LP"), album_id: format!("al-{a}"), genre: Some(g.into()) };
        facts.insert("a".to_string(), fact("A", "Ann", "House"));
        facts.insert("b".to_string(), fact("B", "Bob", "Jazz"));
        let heard = |song: &str, at: i64, secs: f64, ended: Ended| Logged { song: song.into(), at, heard_secs: Some(secs), length_secs: None, ended, counted: ended == Ended::Finished, context: None, followed: false };
        let rows = [
            heard("a", now - 3600, 200.0, Ended::Finished),
            heard("a", now - day, 100.0, Ended::Finished),
            heard("b", now - day, 30.0, Ended::Skipped),
            heard("b", now - 8 * day, 300.0, Ended::Finished),
        ];
        let l = listening(&rows, &facts, Period::Week, now, 0, Some(now - 10 * day));
        assert_eq!(l.time.len(), 7);
        assert_eq!((l.time[6].secs, l.time[5].secs), (200.0, 130.0), "today, yesterday (a skip's time counts)");
        assert_eq!(l.total_secs, 330.0, "eight days back is outside the week");
        assert_eq!(l.plays, 2, "the skip is not a listen");
        assert_eq!(l.artists.iter().map(|t| (t.name.as_str(), t.plays)).collect::<Vec<_>>(), [("Ann", 2)], "only skipped: not a top artist");
        // Taste, within the week: its days; today House alone, yesterday House 100 s and Jazz 30 s; eight days back
        // is not in it.
        assert_eq!((l.taste.step, l.taste.starts.len()), ("day", 7));
        assert_eq!(l.taste.genres, ["House", "Jazz"]);
        assert_eq!(l.taste.shares[6], [1.0, 0.0]);
        assert_eq!(l.taste.shares[5], [100.0 / 130.0, 30.0 / 130.0]);
        // A year: weeks from Mondays, this week last.
        let y = listening(&rows, &facts, Period::Year, now, 0, None);
        assert_eq!(y.time.len(), 52);
        assert_eq!((y.time[51].start + 2 * day + 12 * 3600, y.time[51].secs), (now, 330.0), "Monday's midnight; this week's");
        assert_eq!(y.time[50].secs, 300.0);
    }

    #[test]
    fn taste_goes_back_by_calendar_months_and_years() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(days_of(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(days_of(2026, 10, 23) - days_of(2026, 10, 1), 22);
        let today = days_of(2026, 1, 15);
        let empty = std::collections::HashMap::new();
        // A month's weeks (Thursday the 15th: Monday the 12th last), a year's months, all's years since the log began.
        let weeks = taste(&[], &empty, Period::Month, today, 0, None);
        assert_eq!((weeks.step, weeks.starts.len(), weeks.starts[4], weeks.starts[0]), ("week", 5, days_of(2026, 1, 12) * DAY, days_of(2025, 12, 15) * DAY));
        let months = taste(&[], &empty, Period::Year, today, 0, None);
        assert_eq!((months.starts[0], months.starts[11]), (days_of(2025, 2, 1) * DAY, days_of(2026, 1, 1) * DAY), "twelve months, across the year's turn");
        let years = taste(&[], &empty, Period::All, today, 0, Some(days_of(2023, 6, 1)));
        assert_eq!((years.starts.len(), years.starts[0]), (4, days_of(2023, 1, 1) * DAY));
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
