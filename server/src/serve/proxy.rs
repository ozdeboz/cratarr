//! Whose `X-Forwarded-*` headers Cratarr believes (as Jellyfin's KnownProxies; the owner, 2026-10-23). The client's
//! address (the sign-in throttle, the diagnostics log, a runaway client) and whether it came over HTTPS are read from
//! them, so a client that reaches Cratarr's port directly could otherwise name any address it likes. With
//! `CRATARR_PROXIES` set (addresses or ranges, comma-separated: `172.19.0.0/16, 192.168.178.5`), a request from any other
//! peer has those headers taken off before anything reads them, and the first such request of each peer is written
//! down. Unset, a peer on a local address is believed (loopback, the private ranges, link-local, unique local: a proxy
//! on the same computer, its Docker network or the home network; 0.9.8, before others run Cratarr on their own) and
//! any other is not, and the first peer that forwards is written down, so its address can be named.

use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::{Mutex, OnceLock};

use axum::http::HeaderMap;
use ipnet::IpNet;

/// The headers a proxy sets for the client.
const FORWARDED: [&str; 5] = ["x-forwarded-for", "x-forwarded-proto", "x-forwarded-host", "x-forwarded-uri", "x-forwarded-method"];

static PROXIES: OnceLock<Option<Vec<IpNet>>> = OnceLock::new();
/// The peers already written down.
static NOTED: Mutex<Option<HashSet<IpAddr>>> = Mutex::new(None);

/// `CRATARR_PROXIES` as read: the ranges, `None` when unset. A part that is not an address or a range is left out.
pub fn parse(text: Option<&str>) -> Option<Vec<IpNet>> {
    let text = text?.trim();
    if text.is_empty() {
        return None;
    }
    Some(
        text.split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse::<IpNet>().ok().or_else(|| p.parse::<IpAddr>().ok().map(IpNet::from)))
            .collect(),
    )
}

/// Set once, at the start.
pub fn set(proxies: Option<Vec<IpNet>>) {
    if let Some(p) = &proxies {
        tracing::info!(target: "server", proxies = %p.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "), "forwarded headers believed from these only");
    }
    let _ = PROXIES.set(proxies);
}

fn first_time(peer: IpAddr) -> bool {
    NOTED.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashSet::new).insert(peer)
}

/// Whether `peer` may say who the client is: one named, or, with none named, one on a local address.
fn believed(proxies: Option<&[IpNet]>, peer: IpAddr) -> bool {
    match proxies {
        Some(p) => p.iter().any(|n| n.contains(&peer)),
        None => local(peer),
    }
}

/// Loopback, the private ranges, link-local, and IPv6's unique local addresses (an IPv4 address in IPv6 as itself).
fn local(peer: IpAddr) -> bool {
    match peer {
        IpAddr::V4(a) => a.is_loopback() || a.is_private() || a.is_link_local(),
        IpAddr::V6(a) => match a.to_ipv4_mapped() {
            Some(v4) => local(IpAddr::V4(v4)),
            None => a.is_loopback() || (a.segments()[0] & 0xfe00) == 0xfc00 || (a.segments()[0] & 0xffc0) == 0xfe80,
        },
    }
}

/// A request's forwarding headers, taken off unless its peer is a proxy named; the first of each peer written down.
pub fn screen(headers: &mut HeaderMap, peer: Option<IpAddr>) {
    let Some(peer) = peer else { return };
    if !headers.contains_key("x-forwarded-for") {
        return;
    }
    let proxies = PROXIES.get().and_then(|p| p.as_deref());
    if believed(proxies, peer) {
        if proxies.is_none() && first_time(peer) {
            tracing::info!(target: "server", peer = %peer, "forwarded by a proxy: CRATARR_PROXIES may name it");
        }
        return;
    }
    if first_time(peer) {
        tracing::warn!(target: "server", peer = %peer, "forwarded headers from a peer not in CRATARR_PROXIES (unset: not local): ignored");
    }
    for h in FORWARDED {
        headers.remove(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxies_are_addresses_or_ranges_and_others_are_not_believed() {
        assert_eq!(parse(None), None);
        assert_eq!(parse(Some("  ")), None);
        let p = parse(Some("172.19.0.0/16, 192.168.178.5 ,nonsense,::1")).unwrap();
        assert_eq!(p.len(), 3);
        let at = |a: &str| believed(Some(&p), a.parse().unwrap());
        assert!(at("172.19.0.10"));
        assert!(at("192.168.178.5"));
        assert!(at("::1"));
        assert!(!at("192.168.178.20"), "a client on the LAN reaching the port itself");
        assert!(believed(None, "192.168.178.20".parse().unwrap()), "unset: a local peer");
        assert!(believed(None, "172.19.0.3".parse().unwrap()), "unset: a Docker network's proxy");
        assert!(believed(None, "::1".parse().unwrap()));
        assert!(believed(None, "fd00::5".parse().unwrap()), "unset: IPv6's unique local");
        assert!(believed(None, "::ffff:10.0.0.2".parse().unwrap()), "an IPv4 address in IPv6");
        assert!(!believed(None, "84.1.2.3".parse().unwrap()), "unset: a peer on the internet names no one");
        assert!(!believed(None, "2a02:a210::1".parse().unwrap()));
    }
}
