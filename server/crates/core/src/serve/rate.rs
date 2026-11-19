//! A runaway client, flagged (the owner, 2026-10-23: "Cratarr has no request-rate warning of its own, so a runaway client
//! isn't flagged"). Every request is counted per client (its address, as the nearest proxy saw it) by the minute; a
//! client that asks more than [LIMIT] times a minute, two minutes running, is written down as a warning in the
//! diagnostics log (area `server`, decision 0045): its address, how often, the connection it last asked on (a page's or
//! a player's), and what it asked most (the path's first three parts, `/api/players/Phone`). Two minutes, so a page
//! loading a shelf's covers at once is not one. Once more each minute it goes on; the log adds those to its row. Nothing
//! is refused: the proxy throttles, Cratarr says who.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// Requests a minute above which a client is out of the ordinary: a page and its player ask a few dozen.
const LIMIT: u32 = 600;
/// The minute's length (s).
const MINUTE: u64 = 60;

#[derive(Default)]
struct Client {
    /// The minute counted (since the start), its count, and the minute before's.
    minute: u64,
    count: u32,
    before: u32,
    conn: Option<String>,
    paths: HashMap<String, u32>,
}

/// Cratarr's count, fed by every request (`api::request_ids`).
pub static RATES: std::sync::LazyLock<Rates> = std::sync::LazyLock::new(Rates::new);

pub struct Rates {
    start: Instant,
    clients: Mutex<HashMap<String, Client>>,
}

/// What is said of a runaway client.
#[derive(Debug, PartialEq)]
pub struct Runaway {
    pub requests: u32,
    pub before: u32,
    pub top_path: String,
    pub top_count: u32,
    pub conn: Option<String>,
}

/// The path's first three parts: what was asked, without the item.
pub fn kind_of(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).take(3).collect();
    format!("/{}", parts.join("/"))
}

impl Rates {
    pub fn new() -> Self {
        Self { start: Instant::now(), clients: Mutex::default() }
    }

    /// One request of `client`'s, at `secs` since the start: a runaway, once its minute is over the limit after a minute
    /// that was too.
    fn count(&self, client: &str, conn: Option<&str>, path: &str, secs: u64) -> Option<Runaway> {
        let minute = secs / MINUTE;
        let mut clients = self.clients.lock().unwrap_or_else(|p| p.into_inner());
        // Many clients: those quiet for two minutes are forgotten.
        if clients.len() > 64 {
            clients.retain(|_, c| c.minute + 2 > minute);
        }
        let c = clients.entry(client.to_string()).or_default();
        if c.minute != minute {
            c.before = if c.minute + 1 == minute { c.count } else { 0 };
            c.minute = minute;
            c.count = 0;
            c.paths.clear();
        }
        c.count += 1;
        if let Some(conn) = conn {
            c.conn = Some(conn.to_string());
        }
        *c.paths.entry(kind_of(path)).or_default() += 1;
        if c.count != LIMIT + 1 || c.before <= LIMIT {
            return None;
        }
        let (top_path, top_count) = c.paths.iter().max_by_key(|(_, n)| **n).map(|(p, n)| (p.clone(), *n)).unwrap_or_default();
        Some(Runaway { requests: c.count, before: c.before, top_path, top_count, conn: c.conn.clone() })
    }

    /// One request, written down when it makes its client a runaway.
    pub fn seen(&self, client: &str, conn: Option<&str>, path: &str) {
        if let Some(r) = self.count(client, conn, path, self.start.elapsed().as_secs()) {
            tracing::warn!(
                target: "server",
                client,
                requests = r.requests,
                last_minute = r.before,
                path = r.top_path.as_str(),
                of_them = r.top_count,
                conn = r.conn.as_deref(),
                "client requests too often"
            );
        }
    }
}

impl Default for Rates {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_is_flagged_after_two_busy_minutes_not_one_burst() {
        let r = Rates::new();
        // A burst of covers in one minute: nothing.
        for _ in 0..1000 {
            assert_eq!(r.count("10.0.0.2", None, "/api/cover/jellyfin/abc", 5), None);
        }
        // A quiet minute, then a busy one: still nothing.
        for _ in 0..1000 {
            assert_eq!(r.count("10.0.0.2", None, "/api/cover/jellyfin/abc", 125), None);
        }
        // Busy again the next minute: flagged once, at the limit, with what it asked most.
        let mut flagged = Vec::new();
        for i in 0..900 {
            let path = if i % 3 == 0 { "/api/library" } else { "/api/players/Phone/state" };
            flagged.extend(r.count("10.0.0.2", Some("tab-1"), path, 185));
        }
        assert_eq!(
            flagged,
            vec![Runaway { requests: LIMIT + 1, before: 1000, top_path: "/api/players/Phone".into(), top_count: 400, conn: Some("tab-1".into()) }]
        );
        // Another client is counted apart.
        assert_eq!(r.count("10.0.0.3", None, "/api/library", 185), None);
    }
}
