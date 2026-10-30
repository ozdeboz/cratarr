//! Whether Cratarr reaches what it calls out to (0.9.6, the owner: every connection gets a Health signal when it
//! fails): MusicBrainz, Discogs, Last.fm, ListenBrainz, the Cover Art Archive, Bandcamp, TheAudioDB, Wikimedia. Each
//! request sent through `Sent` notes, by its host, whether the service answered (any answer but busy or refusing)
//! or not. Kept in memory since the server started.

use std::collections::HashMap;
use std::sync::Mutex;

/// A service failing this many times in a row and for this long, no answer between, is failing: a busy moment (a few
/// 503s while MusicBrainz is shared) is not (the sim, 2026-10-26).
const FAILURES: u32 = 3;
const FAILING_SECS: i64 = 5 * 60;

#[derive(Debug, Clone, Default)]
pub struct State {
    /// When it last answered (0: not since the server started).
    pub last_ok: i64,
    /// The last time it could not be asked, and why.
    pub last_error: Option<(i64, String)>,
    /// Since when it has failed without an answer in between, and how many times.
    pub failing_since: i64,
    pub failures: u32,
}

static STATES: Mutex<Option<HashMap<&'static str, State>>> = Mutex::new(None);

/// The connection a request goes to, by its host; none for Cratarr's own servers and the like.
pub fn name_of(url: &reqwest::Url) -> Option<&'static str> {
    let host = url.host_str()?.to_ascii_lowercase();
    let is = |h: &str| host == h || host.ends_with(&format!(".{h}"));
    Some(if is("musicbrainz.org") {
        "musicbrainz"
    } else if is("discogs.com") {
        "discogs"
    } else if is("audioscrobbler.com") || is("last.fm") {
        "lastfm"
    } else if is("listenbrainz.org") {
        "listenbrainz"
    } else if is("coverartarchive.org") || is("archive.org") {
        "coverartarchive"
    } else if is("bandcamp.com") || is("bcbits.com") {
        "bandcamp"
    } else if is("theaudiodb.com") {
        "theaudiodb"
    } else if is("wikidata.org") || is("wikimedia.org") || is("wikipedia.org") {
        "wikimedia"
    } else if is("spotify.com") || is("scdn.co") || is("spotifycdn.com") {
        "spotify"
    } else {
        return None;
    })
}

/// How a connection is called on Settings and in Health.
pub fn label(name: &str) -> &'static str {
    match name {
        "musicbrainz" => "MusicBrainz",
        "discogs" => "Discogs",
        "lastfm" => "Last.fm",
        "listenbrainz" => "ListenBrainz",
        "coverartarchive" => "Cover Art Archive",
        "bandcamp" => "Bandcamp",
        "theaudiodb" => "TheAudioDB",
        "wikimedia" => "Wikimedia",
        "spotify" => "Spotify",
        _ => "A connection",
    }
}

/// A request's answer noted for its connection.
pub fn note(name: &'static str, r: &reqwest::Result<reqwest::Response>) {
    let now = super::moment::now_secs();
    let failed = match r {
        Ok(r) => {
            let s = r.status();
            (s.is_server_error() || s == reqwest::StatusCode::TOO_MANY_REQUESTS).then(|| format!("{} answered {s}", label(name)))
        }
        Err(e) => Some(plain(name, e)),
    };
    let mut states = STATES.lock().unwrap_or_else(|e| e.into_inner());
    let s = states.get_or_insert_with(HashMap::new).entry(name).or_default();
    match failed {
        None => {
            s.last_ok = now;
            (s.failing_since, s.failures) = (0, 0);
        }
        Some(why) => {
            if s.failures == 0 {
                s.failing_since = now;
            }
            s.failures += 1;
            s.last_error = Some((now, why));
        }
    }
}

/// A request's error without its address (an artist's name in a query is no one's business in Health).
fn plain(name: &str, e: &reqwest::Error) -> String {
    let who = label(name);
    if e.is_timeout() {
        format!("{who} did not answer in time")
    } else if e.is_connect() {
        let said = format!("{e:?}");
        if said.contains("dns error") { format!("{who}'s address could not be looked up") } else { format!("{who} could not be reached") }
    } else {
        let said = e.to_string();
        format!("{who}: {}", said.split(" for url").next().unwrap_or(&said))
    }
}

/// A connection's state, as noted since the server started.
pub fn state(name: &str) -> State {
    STATES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|s| s.get(name)).cloned().unwrap_or_default()
}

/// The connections failing now: failed at least `FAILURES` times over `FAILING_SECS` or more, with no answer since.
pub fn failing(now: i64) -> Vec<(&'static str, State)> {
    let states = STATES.lock().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<(&'static str, State)> = states
        .iter()
        .flat_map(|s| s.iter())
        .filter(|(_, s)| s.failures >= FAILURES && now - s.failing_since >= FAILING_SECS)
        .map(|(n, s)| (*n, s.clone()))
        .collect();
    out.sort_by_key(|(n, _)| *n);
    out
}

/// A request sent, its answer noted for its connection (by its host).
pub trait Sent {
    fn sent(self) -> impl std::future::Future<Output = reqwest::Result<reqwest::Response>> + Send;
}

impl Sent for reqwest::RequestBuilder {
    fn sent(self) -> impl std::future::Future<Output = reqwest::Result<reqwest::Response>> + Send {
        async move {
            let (client, req) = self.build_split();
            let req = req?;
            let name = name_of(req.url());
            // Its address left out of any error (0.9.7): a query holds keys, signatures and session keys (Last.fm's
            // `api_key`, `api_sig`, `sk`), and the error reaches the page, Health and Connections.
            let r = client.execute(req).await.map_err(reqwest::Error::without_url);
            if let Some(n) = name {
                note(n, &r);
            }
            r
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_error_never_carries_the_address_and_its_keys() {
        let e = reqwest::Client::new().get("http://127.0.0.1:1/2.0/?api_key=KEY123&sk=SESSION456").sent().await.unwrap_err();
        let said = format!("{e:#} {e:?}");
        assert!(!said.contains("KEY123") && !said.contains("SESSION456"), "{said}");
    }

    #[test]
    fn a_connection_is_known_by_its_host_and_fails_after_a_few_tries() {
        let url = |u: &str| reqwest::Url::parse(u).unwrap();
        assert_eq!(name_of(&url("https://musicbrainz.org/ws/2/release/x")), Some("musicbrainz"));
        assert_eq!(name_of(&url("https://labs.api.listenbrainz.org/similar-artists/json")), Some("listenbrainz"));
        assert_eq!(name_of(&url("https://f4.bcbits.com/img/a_16.jpg")), Some("bandcamp"));
        assert_eq!(name_of(&url("http://lidarr:8686/api/v1/artist")), None);
        assert_eq!(name_of(&url("https://evil-musicbrainz.org/")), None);
        let mut states = STATES.lock().unwrap();
        let s = states.get_or_insert_with(HashMap::new).entry("theaudiodb").or_default();
        (s.failures, s.failing_since) = (3, 1000);
        drop(states);
        assert!(failing(1000 + 60).iter().all(|(n, _)| *n != "theaudiodb"), "three tries in a minute are a busy moment");
        assert!(failing(1000 + FAILING_SECS).iter().any(|(n, _)| *n == "theaudiodb"), "five minutes without an answer are not");
        STATES.lock().unwrap().get_or_insert_with(HashMap::new).get_mut("theaudiodb").unwrap().failures = 2;
        assert!(failing(1000 + 3600).iter().all(|(n, _)| *n != "theaudiodb"), "two failures, however long ago, are not");
    }
}
