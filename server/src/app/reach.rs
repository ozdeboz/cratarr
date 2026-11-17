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
    let failed = match r {
        Ok(r) => {
            let s = r.status();
            (s.is_server_error() || s == reqwest::StatusCode::TOO_MANY_REQUESTS).then(|| format!("{} answered {s}", label(name)))
        }
        Err(e) => Some(plain(name, e)),
    };
    noted(name, failed);
}

/// A connection's answer that came, but was no answer (O4: Bandcamp's bot check), noted as a failure.
pub fn failed(name: &'static str, why: String) {
    noted(name, Some(why));
}

fn noted(name: &'static str, failed: Option<String>) {
    let now = super::moment::now_secs();
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

// ---- One outbound guard (O1)

/// Cratarr's name to the services it asks, with its project's address (as Discogs and MusicBrainz ask a client to give).
pub const USER_AGENT: &str = concat!("Cratarr/", env!("CARGO_PKG_VERSION"), " +https://github.com/ozdeboz/cratarr");

/// What a client of Cratarr's reads, and so where it may go (O1): each purpose its hosts, exactly or as their subdomains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Bandcamp's pages and its search box.
    BandcampPages,
    /// Bandcamp's media: the streams a preview plays.
    BandcampMedia,
    /// Covers: the Cover Art Archive's, Discogs', Spotify's, Bandcamp's pictures.
    Covers,
    /// Artists' photos: TheAudioDB's, Wikimedia's, Discogs'.
    ArtistPhotos,
    /// The metadata services, at their fixed hosts.
    Metadata,
}

impl Purpose {
    fn hosts(self) -> &'static [&'static str] {
        match self {
            Purpose::BandcampPages => &["bandcamp.com"],
            Purpose::BandcampMedia => &["bcbits.com"],
            Purpose::Covers => &["coverartarchive.org", "archive.org", "discogs.com", "scdn.co", "spotifycdn.com", "bcbits.com"],
            Purpose::ArtistPhotos => &["theaudiodb.com", "wikimedia.org", "discogs.com"],
            Purpose::Metadata => &[
                "musicbrainz.org",
                "listenbrainz.org",
                "discogs.com",
                "audioscrobbler.com",
                "last.fm",
                "theaudiodb.com",
                "wikidata.org",
                "wikimedia.org",
                "wikipedia.org",
                "coverartarchive.org",
                "archive.org",
                "acoustid.org",
                "spotify.com",
                "scdn.co",
                "spotifycdn.com",
            ],
        }
    }

    /// Whether an address is this purpose's: https, on one of its hosts by name (never by a number).
    pub fn allows(self, url: &reqwest::Url) -> bool {
        url.scheme() == "https" && self.has_host(url)
    }

    /// Whether its host is one of the purpose's by name: an address by its number never ends as a name does.
    fn has_host(self, url: &reqwest::Url) -> bool {
        let Some(host) = url.host_str().map(str::to_ascii_lowercase) else { return false };
        self.hosts().iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
    }
}

/// A client for a purpose (O1): https only; a redirect followed only while every hop stays on the purpose's hosts (one off them is
/// answered as it came, unfollowed); loopback, private, link-local and shared addresses refused on the address connected to
/// (`Public`), so a name pointing home is refused too; Cratarr's User-Agent.
pub fn client(purpose: Purpose) -> reqwest::ClientBuilder {
    reqwest::Client::builder().https_only(true).redirect(staying(purpose, true)).dns_resolver(std::sync::Arc::new(Public)).user_agent(USER_AGENT)
}

/// The redirect rule of `client`, plain http let through where `https` is false (the tests' fake servers).
fn staying(purpose: Purpose, https: bool) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |a| {
        let stays = if https { purpose.allows(a.url()) } else { purpose.has_host(a.url()) };
        if a.previous().len() >= 10 {
            a.error("too many redirects")
        } else if stays {
            a.follow()
        } else {
            a.stop()
        }
    })
}

/// A client for a service an administrator set up, on the LAN (O1: Lidarr, Jellyfin, slskd, Subsonic, Kodi, webhooks): never a
/// redirect, so a key it carries stays on the host given.
pub fn lan_client() -> reqwest::ClientBuilder {
    reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())
}

/// Whether an address is out in the world: not loopback, private, link-local, shared (CGNAT), unique-local, unspecified, broadcast
/// or documentation's (O1).
pub fn is_public(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified() || v4.is_broadcast() || v4.is_documentation() || a == 0 || (a == 100 && (64..128).contains(&b)))
        }
        std::net::IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_public(std::net::IpAddr::V4(v4)),
            None => !(v6.is_loopback() || v6.is_unspecified() || (v6.segments()[0] & 0xfe00) == 0xfc00 || (v6.segments()[0] & 0xffc0) == 0xfe80),
        },
    }
}

/// Names resolved as the system resolves them, every address `is_public` refuses left out (O1): judged on the address connected
/// to, so a public name pointing home, or rebound there, is refused.
struct Public;

impl reqwest::dns::Resolve for Public {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let found: Vec<std::net::SocketAddr> = match tokio::net::lookup_host((name.as_str(), 0)).await {
                Ok(found) => found.collect(),
                Err(e) => return Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
            };
            let public: Vec<std::net::SocketAddr> = found.into_iter().filter(|a| is_public(a.ip())).collect();
            if public.is_empty() {
                return Err(format!("{} is not a public address", name.as_str()).into());
            }
            Ok(Box::new(public.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// An answer's body read in pieces and stopped at `cap` bytes (O1): a longer one is refused, never held whole.
pub async fn read_capped(mut resp: reqwest::Response, cap: usize) -> anyhow::Result<Vec<u8>> {
    let over = || anyhow::anyhow!("the answer was longer than {} KB: cut there", cap / 1024);
    if resp.content_length().is_some_and(|n| n > cap as u64) {
        return Err(over());
    }
    let mut out = Vec::new();
    while let Some(piece) = resp.chunk().await? {
        if out.len() + piece.len() > cap {
            return Err(over());
        }
        out.extend_from_slice(&piece);
    }
    Ok(out)
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

    /// A server of the tests' own on this machine, answering as `app` does: its port.
    async fn fake(app: axum::Router) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        port
    }

    /// `client`'s rules but plain http, its names pointing at the fake server (`resolve` stands in for the world's DNS).
    fn at_fake(purpose: Purpose, names: &[&str], port: u16) -> reqwest::Client {
        let mut b = reqwest::Client::builder().redirect(staying(purpose, false));
        for n in names {
            b = b.resolve(n, std::net::SocketAddr::from(([127, 0, 0, 1], port)));
        }
        b.build().unwrap()
    }

    /// O1: a Bandcamp page that redirects off Bandcamp (a custom domain, an address at home) is answered as it came, not followed;
    /// one that stays on Bandcamp is.
    #[tokio::test]
    async fn a_bandcamp_page_redirecting_off_bandcamp_is_not_followed() {
        use axum::{response::Redirect, routing::get};
        let port = std::sync::Arc::new(std::sync::OnceLock::<u16>::new());
        let p = port.clone();
        let app = axum::Router::new()
            .route("/album/away", get(|| async { Redirect::temporary("http://router.example.org/admin") }))
            .route("/album/moved", get(move || std::future::ready(Redirect::temporary(&format!("http://other.bandcamp.com:{}/album/here", p.get().unwrap())))))
            .route("/album/here", get(|| async { "the page" }))
            .route("/admin", get(|| async { "the router's own page" }));
        let at = fake(app).await;
        port.set(at).unwrap();
        let c = at_fake(Purpose::BandcampPages, &["glassharbour.bandcamp.com", "other.bandcamp.com", "router.example.org"], at);
        let away = c.get(format!("http://glassharbour.bandcamp.com:{at}/album/away")).send().await.unwrap();
        assert!(away.status().is_redirection(), "not followed off Bandcamp: {}", away.status());
        let moved = c.get(format!("http://glassharbour.bandcamp.com:{at}/album/moved")).send().await.unwrap();
        assert_eq!(moved.text().await.unwrap(), "the page", "followed while it stays on Bandcamp");
        let url = |u: &str| reqwest::Url::parse(u).unwrap();
        assert!(Purpose::BandcampPages.allows(&url("https://glassharbour.bandcamp.com/album/x")));
        assert!(!Purpose::BandcampPages.allows(&url("http://glassharbour.bandcamp.com/album/x")), "https only");
        assert!(!Purpose::BandcampPages.allows(&url("https://bandcamp.com.example.org/album/x")) && !Purpose::BandcampPages.allows(&url("https://notbandcamp.com/x")));
        assert!(!Purpose::Covers.allows(&url("https://192.168.1.20/cover.jpg")), "never an address by its number");
    }

    /// O1: an address at home is refused on the address a name resolves to, whatever the name: loopback, private, link-local, shared.
    #[tokio::test]
    async fn a_private_address_is_refused_after_dns() {
        for ip in ["127.0.0.1", "10.0.0.5", "172.17.0.2", "192.168.1.20", "169.254.1.1", "100.64.0.1", "0.0.0.0", "::1", "fe80::1", "fd00::1", "::ffff:192.168.1.20"] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["93.184.216.34", "2606:2800:220:1::1"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
        let e = client(Purpose::Metadata).build().unwrap().get("https://localhost/ws/2/").send().await.unwrap_err();
        assert!(format!("{e:?}").contains("is not a public address"), "{e:?}");
    }

    /// O1: an answer longer than its cap is cut there and refused, never read whole.
    #[tokio::test]
    async fn an_answer_over_the_cap_is_cut() {
        use axum::routing::get;
        let app = axum::Router::new().route("/long", get(|| async { "x".repeat(300_000) })).route("/short", get(|| async { "x".repeat(1_000) }));
        let at = fake(app).await;
        let c = reqwest::Client::new();
        let long = c.get(format!("http://127.0.0.1:{at}/long")).send().await.unwrap();
        assert!(read_capped(long, 100_000).await.unwrap_err().to_string().contains("cut there"));
        let short = c.get(format!("http://127.0.0.1:{at}/short")).send().await.unwrap();
        assert_eq!(read_capped(short, 100_000).await.unwrap().len(), 1_000);
    }

    /// O1: a service an administrator set up (slskd here) never follows a redirect: its key stays on the host given.
    #[tokio::test]
    async fn an_admin_service_never_follows_a_redirect() {
        use axum::{response::Redirect, routing::get};
        let done = serde_json::json!({ "transfers": [{ "id": "t1", "filename": "x", "state": "Completed, Succeeded", "percentComplete": 100.0 }] });
        let app = axum::Router::new()
            .route("/api/v0/transfers/downloads/batches/{id}", get(|| async { Redirect::temporary("/elsewhere") }))
            .route("/elsewhere", get(move || std::future::ready(axum::Json(done.clone()))));
        let at = fake(app).await;
        let slskd = crate::slskd::Slskd::new(&format!("http://127.0.0.1:{at}"), "key").unwrap();
        assert!(slskd.transfers("b1").await.is_err(), "the redirect not followed, its answer not read");
        let r = lan_client().build().unwrap().get(format!("http://127.0.0.1:{at}/api/v0/transfers/downloads/batches/b1")).send().await.unwrap();
        assert!(r.status().is_redirection());
    }

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
