//! Cratarr's memory, watched (2026-10-24: eighteen restarts in a day at the NAS's 768 MB, each a moment of "cannot
//! reach", and nothing in the log said what had run): four times a second its resident memory is read, and when it
//! jumps the log says by how much and what was running then: the requests not yet answered (the path's first three
//! parts, and for how long) and the analysis' file decode (`app::analysis_pass`). Once a jump; again only after it
//! has come down.
//!
//! The budget (the audit, 2026-10-29): a decode starts only while there is room for one under the container's limit
//! (`room`). The analysis pass waits for it; a decode asked for now is not made without it.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A jump this large (MB) over the lowest of the last while is written down.
const JUMP_MB: u64 = 150;
/// How long the lowest is remembered.
const LOW_FOR: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_millis(250);
/// What one decode may take on top of what runs (MB): the jumps the log has seen were 150 and more.
const DECODE_MB: u64 = 250;
/// Of the container's limit, how much Cratarr fills at most (the rest for the kernel's pages).
const FILL: f64 = 0.9;
/// How often a wait for room looks again.
const ROOM_EVERY: Duration = Duration::from_secs(5);

static NEXT: AtomicU64 = AtomicU64::new(0);
static REQUESTS: Mutex<Option<HashMap<u64, (String, Instant)>>> = Mutex::new(None);
static JOB: Mutex<Option<(String, Instant)>> = Mutex::new(None);

/// A request being answered, until dropped.
pub struct Request(u64);

pub fn request(path: &str) -> Request {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    REQUESTS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(id, (path.to_string(), Instant::now()));
    Request(id)
}

impl Drop for Request {
    fn drop(&mut self) {
        if let Some(m) = REQUESTS.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            m.remove(&self.0);
        }
    }
}

/// The analysis' decode under way, until dropped.
pub struct Job;

pub fn job(what: String) -> Job {
    *JOB.lock().unwrap_or_else(|e| e.into_inner()) = Some((what, Instant::now()));
    Job
}

impl Drop for Job {
    fn drop(&mut self) {
        *JOB.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// Resident memory in MB (Linux; none elsewhere).
fn resident_mb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb: u64 = status.lines().find(|l| l.starts_with("VmRSS:"))?.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}

/// The container's memory limit in MB (cgroup v2, else v1); none when there is none.
fn limit_mb() -> Option<u64> {
    static LIMIT: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *LIMIT.get_or_init(|| {
        ["/sys/fs/cgroup/memory.max", "/sys/fs/cgroup/memory/memory.limit_in_bytes"]
            .iter()
            .find_map(|f| std::fs::read_to_string(f).ok())
            .and_then(|t| t.trim().parse::<u64>().ok())
            .filter(|b| *b < 1 << 50)
            .map(|b| b / (1024 * 1024))
    })
}

/// Whether a decode fits: what is resident and one decode within the limit's fill; with either unknown, it does.
fn fits(resident: Option<u64>, limit: Option<u64>) -> bool {
    match (resident, limit) {
        (Some(r), Some(l)) => r + DECODE_MB <= (l as f64 * FILL) as u64,
        _ => true,
    }
}

/// Whether there is room for a decode now.
pub fn room() -> bool {
    fits(resident_mb(), limit_mb())
}

/// Waits until there is room for a decode; the wait is written down once.
pub async fn wait_for_room() {
    let mut said = false;
    while !room() {
        if !said {
            tracing::info!(target: "memory", mb = resident_mb().unwrap_or(0), limit_mb = limit_mb().unwrap_or(0), "waiting for room to decode");
            said = true;
        }
        tokio::time::sleep(ROOM_EVERY).await;
    }
}

/// What was running, as one line.
fn running() -> (String, String) {
    let mut requests: Vec<(String, Duration)> = REQUESTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|m| m.values().map(|(p, at)| (p.clone(), at.elapsed())).collect())
        .unwrap_or_default();
    requests.sort_by(|a, b| b.1.cmp(&a.1));
    let requests = requests.iter().map(|(p, d)| format!("{p} {:.1}s", d.as_secs_f64())).collect::<Vec<_>>().join(", ");
    let job = JOB.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|(w, at)| format!("{w} {:.1}s", at.elapsed().as_secs_f64())).unwrap_or_default();
    (requests, job)
}

/// Whether `now` is a jump over `low` not yet written down; and whether it has come down again.
fn jumped(low: u64, now: u64, said: bool) -> (bool, bool) {
    let jump = now >= low + JUMP_MB;
    (jump && !said, now < low + JUMP_MB / 2)
}

/// Watches for as long as Cratarr runs.
pub fn start() {
    if resident_mb().is_none() {
        return;
    }
    std::thread::Builder::new()
        .name("memwatch".into())
        .spawn(|| {
            let mut lows: std::collections::VecDeque<(Instant, u64)> = Default::default();
            let mut said = false;
            loop {
                std::thread::sleep(EVERY);
                let Some(now) = resident_mb() else { continue };
                let at = Instant::now();
                while lows.front().is_some_and(|(t, _)| at.duration_since(*t) > LOW_FOR) {
                    lows.pop_front();
                }
                let low = lows.iter().map(|(_, m)| *m).min().unwrap_or(now);
                lows.push_back((at, now));
                let (say, down) = jumped(low, now, said);
                if say {
                    let (requests, job) = running();
                    tracing::warn!(target: "memory", mb = now, from_mb = low, requests = %requests, job = %job, "memory jumped");
                    said = true;
                } else if down {
                    said = false;
                }
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_jump_is_said_once_and_again_after_it_came_down() {
        assert_eq!(jumped(200, 300, false), (false, false));
        assert_eq!(jumped(200, 400, false), (true, false));
        assert_eq!(jumped(200, 600, true), (false, false), "still up: said already");
        assert_eq!(jumped(200, 250, true), (false, true), "down again");
    }

    #[test]
    fn a_decode_fits_under_the_limit_or_waits() {
        assert!(fits(Some(276), Some(768)), "the NAS at rest: 276 + 250 under 691");
        assert!(!fits(Some(450), Some(768)));
        assert!(fits(Some(450), None), "no limit known");
        assert!(fits(None, Some(768)), "nothing resident known");
    }

    #[test]
    fn what_runs_is_named_until_it_ends() {
        let r = request("/api/test/memwatch");
        let j = job("visuals: a.flac".into());
        let (requests, job) = running();
        assert!(requests.contains("/api/test/memwatch"));
        assert!(job.starts_with("visuals: a.flac"));
        drop((r, j));
        assert!(!running().0.contains("/api/test/memwatch"));
    }
}
