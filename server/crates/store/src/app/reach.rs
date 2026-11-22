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
    } else if is("acoustid.org") {
        "acoustid"
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
        "acoustid" => "AcoustID",
        "slskd" => "slskd",
        "github" => "GitHub",
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
    /// Bandcamp's media: the streams a preview plays. A stream may start on Bandcamp's own stream hosts (`starts_at`, Y10); every
    /// redirect stays on its media hosts.
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

    /// Whether an address may be a request's first hop (Y10): the purpose's own, and for Bandcamp's media also Bandcamp's own stream
    /// addresses on its domain (`bandcamp_stream`). A redirect from there is followed only onto the purpose's hosts (`client`), so a
    /// stream there goes on to `*.bcbits.com` or nowhere.
    pub fn starts_at(self, url: &reqwest::Url) -> bool {
        self.allows(url) || (self == Purpose::BandcampMedia && bandcamp_stream(url))
    }

    /// An address on one of the purpose's hosts given as plain http (older TheAudioDB entries name their photos so), made https
    /// before it is checked (Y11); any other as it is.
    pub fn upgraded(self, mut url: reqwest::Url) -> reqwest::Url {
        if url.scheme() == "http" && url.port().is_none() && self.has_host(&url) {
            let _ = url.set_scheme("https");
        }
        url
    }

    /// Whether its host is one of the purpose's by name: an address by its number never ends as a name does.
    fn has_host(self, url: &reqwest::Url) -> bool {
        let Some(host) = url.host_str().map(str::to_ascii_lowercase) else { return false };
        self.hosts().iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
    }
}

/// Bandcamp's own stream addresses on its own domain (Y10, allowed without a live look, the owner's default): `bandcamp.com`'s
/// `/stream_redirect` and the `popplers<n>.bandcamp.com` hosts older pages name, https on its usual port. Never an artist's page.
fn bandcamp_stream(url: &reqwest::Url) -> bool {
    let host = url.host_str().map(str::to_ascii_lowercase).unwrap_or_default();
    // Its number there (V9): `popplers.bandcamp.com` is none of Bandcamp's stream hosts.
    let popplers = host.strip_suffix(".bandcamp.com").and_then(|h| h.strip_prefix("popplers")).is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    url.scheme() == "https" && url.port().is_none() && (popplers || (host == "bandcamp.com" && url.path() == "/stream_redirect"))
}

/// A client for a purpose (O1): https only; a redirect followed only while every hop stays on the purpose's hosts (one off them is
/// answered as it came, unfollowed); loopback, private, link-local and shared addresses refused on the address connected to
/// (`Public`), so a name pointing home is refused too; Cratarr's User-Agent. No proxy from the environment (Y11): a proxy would look
/// the names up, not `Public`.
pub fn client(purpose: Purpose) -> reqwest::ClientBuilder {
    reqwest::Client::builder().https_only(true).no_proxy().redirect(staying(purpose, true)).dns_resolver(std::sync::Arc::new(Public)).user_agent(USER_AGENT)
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
/// redirect, so a key it carries stays on the host given; no proxy from the environment (Y11).
pub fn lan_client() -> reqwest::ClientBuilder {
    reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
}

/// Whether an address is out in the world: not loopback, private, link-local, shared (CGNAT), unique-local, unspecified, broadcast
/// or documentation's (O1); nor multicast, the benchmark range (198.18/15), the reserved 240/4 or the old site-local fec0::/10 (Y11).
/// An IPv4 address carried in an IPv6 one (mapped, the old compatible `::a.b.c.d`, NAT64's 64:ff9b::/96, 6to4's 2002::/16) is judged
/// as itself; nor Teredo's 2001::/32 nor the IETF's protocol range 192.0.0.0/24 (V9).
pub fn is_public(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
                || a == 0
                || a >= 240
                || (a == 100 && (64..128).contains(&b))
                || (a == 198 && (b & 0xfe) == 18)
                || (a == 192 && b == 0 && c == 0))
        }
        std::net::IpAddr::V6(v6) => {
            let s = v6.segments();
            let nat64 = (s[..6] == [0x64, 0xff9b, 0, 0, 0, 0]).then(|| std::net::Ipv4Addr::from(u128::from(v6) as u32));
            let six_to_four = (s[0] == 0x2002).then(|| std::net::Ipv4Addr::from((u32::from(s[1]) << 16) | u32::from(s[2])));
            match v6.to_ipv4().or(nat64).or(six_to_four) {
                Some(v4) => is_public(std::net::IpAddr::V4(v4)),
                None => !(v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80 || (s[0] & 0xffc0) == 0xfec0 || (s[0] == 0x2001 && s[1] == 0)),
            }
        }
    }
}

/// Names resolved as the system resolves them, every address `is_public` refuses left out (O1): judged on the address connected
/// to, so a public name pointing home, or rebound there, is refused.
struct Public;

impl reqwest::dns::Resolve for Public {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let found: Vec<std::net::SocketAddr> = match looked_up(name.as_str()).await {
                Ok(found) => found,
                Err(e) => return Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
            };
            let public: Vec<std::net::SocketAddr> = found.into_iter().filter(|a| is_public(a.ip())).collect();
            if public.is_empty() {
                return Err(format!("{} is not a public address", name.as_str()).into());
            }
            // Tests: a name of the test's world, once judged, is dialled at the test's fake on this machine.
            #[cfg(any(test, feature = "test-support"))]
            let public = world::dialled(name.as_str(), public);
            Ok(Box::new(public.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// A name's addresses as the system resolves them (tests: as the test's world says, never by DNS, while one is set).
async fn looked_up(name: &str) -> std::io::Result<Vec<std::net::SocketAddr>> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(found) = world::resolves(name) {
        return found;
    }
    Ok(tokio::net::lookup_host((name, 0)).await?.collect())
}

/// Tests (SCENARIOS-COUPLING COU.21): a world of the test's own for an address an administrator gives. While it is set, every name
/// `Public` resolves is answered from its table, never by DNS (a name not in it is refused), and judged as ever; a name judged public
/// is then dialled at the test's fake on this machine, and each hop of an address goes to that fake as plain http (`wire`), its name
/// kept for the fake to read. The judge (`is_public`), the resolver (`Public`) and the hop rule are production's: only the lookup and
/// the wire are the test's. One world at a time in a process (`set` waits for the last to go).
#[cfg(any(test, feature = "test-support"))]
pub mod world {
    use std::collections::HashMap;
    use std::net::{IpAddr, SocketAddr};
    use std::sync::{Mutex, MutexGuard};

    struct Set {
        names: HashMap<String, Vec<IpAddr>>,
        port: u16,
    }

    static SET: Mutex<Option<Set>> = Mutex::new(None);
    static ONE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn set() -> MutexGuard<'static, Option<Set>> {
        SET.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The world of one test, until dropped.
    pub struct World {
        _one: tokio::sync::MutexGuard<'static, ()>,
    }

    impl World {
        /// Set: each name resolving to its addresses, the test's fake listening on `port` on this machine.
        pub async fn set(names: &[(&str, &[&str])], port: u16) -> World {
            let one = ONE.lock().await;
            let names = names.iter().map(|(n, ips)| (n.to_ascii_lowercase(), ips.iter().map(|ip| ip.parse().expect("an address")).collect())).collect();
            *set() = Some(Set { names, port });
            World { _one: one }
        }
    }

    impl Drop for World {
        fn drop(&mut self) {
            *set() = None;
        }
    }

    /// A name's addresses in the world, while one is set: its own, or refused where it has none.
    pub(super) fn resolves(name: &str) -> Option<std::io::Result<Vec<SocketAddr>>> {
        let s = set();
        let s = s.as_ref()?;
        Some(match s.names.get(&name.to_ascii_lowercase()) {
            Some(ips) => Ok(ips.iter().map(|ip| SocketAddr::new(*ip, 0)).collect()),
            None => Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{name} is no name of the test's world"))),
        })
    }

    /// A name's judged addresses as the wire dials them: the fake on this machine for a name of the world.
    pub(super) fn dialled(name: &str, judged: Vec<SocketAddr>) -> Vec<SocketAddr> {
        if set().as_ref().is_some_and(|s| s.names.contains_key(&name.to_ascii_lowercase())) { vec![SocketAddr::from(([127, 0, 0, 1], 0))] } else { judged }
    }

    /// An address's hop as the wire carries it: in a world, plain http to the fake's port, its name kept (the fake reads its Host).
    pub fn wire(url: reqwest::Url) -> reqwest::Url {
        let Some(port) = set().as_ref().map(|s| s.port) else { return url };
        let mut to = url;
        let _ = to.set_scheme("http");
        let _ = to.set_port(Some(port));
        to
    }

    /// Whether a world is set.
    pub fn is_set() -> bool {
        set().is_some()
    }
}

/// An image's answer (Y11: the Spotify picture relay, an administrator's chosen cover): its type an image's, its body read as
/// `read_capped` reads it; an empty one is no image.
pub async fn read_image(resp: reqwest::Response, cap: usize) -> anyhow::Result<Vec<u8>> {
    if !resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|t| t.starts_with("image/")) {
        anyhow::bail!("the answer is not an image");
    }
    let bytes = read_capped(resp, cap).await?;
    if bytes.is_empty() {
        anyhow::bail!("the image is empty");
    }
    Ok(bytes)
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
            // Tests: while the stand-ins are set, an ask of a source this knows is answered by them, never sent (LK.0's harness 1).
            #[cfg(any(test, feature = "test-support"))]
            if let Some(n) = name
                && let Some(answer) = stand_ins::answer(&req, n).await
            {
                return Ok(answer);
            }
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

/// Tests (SCENARIOS-LOOKUPS LK.0's harness, its first part): every outside ask's one door, `Sent::sent`, intercepted. While a test's
/// stand-ins are set, each ask of a source `name_of` knows is counted as it arrives (its source, method, host and path) and answered
/// by them (none: refused, 599), never sent; a source may be held until the test lets it answer. The LAN clients are not counted.
/// One test's stand-ins at a time in a process (`set` waits for the last to go).
#[cfg(any(test, feature = "test-support"))]
pub mod stand_ins {
    use std::sync::{Arc, Mutex};

    /// One ask, as the stand-ins saw it: and the route's scope it was made in (LK.0's harness 2), and whether inside a runner job's
    /// work (LK.0b).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Asked {
        pub source: &'static str,
        pub method: String,
        pub host: String,
        pub path: String,
        pub query: String,
        pub scope: Option<u64>,
        pub job: bool,
    }

    /// What answers an ask: its status and body (JSON or a page), or none (refused).
    pub type Answer = Arc<dyn Fn(&Asked) -> Option<(u16, String)> + Send + Sync>;

    struct Set {
        answer: Answer,
        asked: Vec<Asked>,
        held: Vec<&'static str>,
    }

    static SET: Mutex<Option<Set>> = Mutex::new(None);
    static ONE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    static LET_GO: tokio::sync::Notify = tokio::sync::Notify::const_new();

    fn set() -> std::sync::MutexGuard<'static, Option<Set>> {
        SET.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A process given over to the stand-ins (LK.0's scan, each of its asks in a process of its own): every ask of a known source
    /// goes to them, MusicBrainz's web service at its own address too (`discover::mb_ws`), so none passes uncounted.
    static EVERYTHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    /// This process given over to the stand-ins, for good.
    pub fn give_everything() {
        EVERYTHING.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Whether this process is given over to the stand-ins.
    pub fn everything() -> bool {
        EVERYTHING.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The stand-ins of one test, until dropped.
    pub struct StandIns {
        _one: tokio::sync::MutexGuard<'static, ()>,
    }

    impl StandIns {
        /// Set: every ask of a known source answered by `answer`.
        pub async fn set(answer: Answer) -> StandIns {
            let one = ONE.lock().await;
            *set() = Some(Set { answer, asked: Vec::new(), held: Vec::new() });
            StandIns { _one: one }
        }

        /// Every ask so far, in the order they came.
        pub fn asked(&self) -> Vec<Asked> {
            set().as_ref().map(|s| s.asked.clone()).unwrap_or_default()
        }

        /// How many asks of `source` so far.
        pub fn count(&self, source: &str) -> usize {
            self.asked().iter().filter(|a| a.source == source).count()
        }

        /// The asks counted so far forgotten.
        pub fn clear(&self) {
            if let Some(s) = set().as_mut() {
                s.asked.clear();
            }
        }

        /// `source`'s answers held (its asks counted as they come, answered once let go).
        pub fn hold(&self, source: &'static str) {
            if let Some(s) = set().as_mut() {
                s.held.push(source);
            }
        }

        /// Every held source answers.
        pub fn let_go(&self) {
            if let Some(s) = set().as_mut() {
                s.held.clear();
            }
            LET_GO.notify_waiters();
        }
    }

    impl Drop for StandIns {
        fn drop(&mut self) {
            *set() = None;
            LET_GO.notify_waiters();
        }
    }

    /// The stand-ins' answer to `req`, an ask of `source`, where they are set: counted now, answered once its source is let go.
    pub(super) async fn answer(req: &reqwest::Request, source: &'static str) -> Option<reqwest::Response> {
        let url = req.url();
        // A test's own fake server on this machine (an address with its port) is that test's, never the stand-ins'.
        if url.port().is_some() {
            return None;
        }
        let scope = crate::run::scope::current();
        let asked = Asked {
            source,
            method: req.method().to_string(),
            host: url.host_str().unwrap_or_default().to_string(),
            path: url.path().to_string(),
            query: url.query().unwrap_or_default().to_string(),
            scope: scope.as_ref().map(|s| s.id),
            job: scope.is_some_and(|s| s.job),
        };
        let answer = {
            let mut s = set();
            let s = s.as_mut()?;
            s.asked.push(asked.clone());
            s.answer.clone()
        };
        loop {
            let let_go = LET_GO.notified();
            match set().as_ref() {
                Some(s) if s.held.contains(&source) => {}
                _ => break,
            }
            let_go.await;
        }
        let (status, body) = answer(&asked).unwrap_or((599, "no stand-in".into()));
        let r = axum::http::Response::builder().status(status).header("content-type", if body.starts_with('<') { "text/html" } else { "application/json" }).body(body).ok()?;
        Some(reqwest::Response::from(r))
    }
}

/// Test helpers, for this module's tests and those moved up to the crates above (SPLIT-PLAN 3.2).
#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    /// A server of the tests' own on this machine, answering as `app` does: its port.
    pub async fn fake(app: axum::Router) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        crate::run::spawn(async move { axum::serve(listener, app).await.unwrap() });
        port
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::testing::*;

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

    /// Y10: a preview's stream may start on Bandcamp's own stream hosts and is followed from there onto its media hosts only: a
    /// redirect anywhere else, a Bandcamp page too, is answered as it came. No other Bandcamp address starts a stream.
    #[tokio::test]
    async fn a_stream_on_bandcamps_own_host_is_followed_to_bcbits_only() {
        use axum::{response::Redirect, routing::get};
        let port = std::sync::Arc::new(std::sync::OnceLock::<u16>::new());
        let (p, q) = (port.clone(), port.clone());
        let app = axum::Router::new()
            .route("/download/track", get(move || std::future::ready(Redirect::temporary(&format!("http://t4.bcbits.com:{}/stream/1", p.get().unwrap())))))
            .route("/stream_redirect", get(move || std::future::ready(Redirect::temporary(&format!("http://air.bandcamp.com:{}/album/x", q.get().unwrap())))))
            .route("/stream/1", get(|| async { "the stream" }))
            .route("/album/x", get(|| async { "a page" }));
        let at = fake(app).await;
        port.set(at).unwrap();
        let c = at_fake(Purpose::BandcampMedia, &["popplers5.bandcamp.com", "bandcamp.com", "t4.bcbits.com", "air.bandcamp.com"], at);
        let followed = c.get(format!("http://popplers5.bandcamp.com:{at}/download/track")).send().await.unwrap();
        assert_eq!(followed.text().await.unwrap(), "the stream", "followed onto bcbits.com");
        let off = c.get(format!("http://bandcamp.com:{at}/stream_redirect")).send().await.unwrap();
        assert!(off.status().is_redirection(), "not followed onto a Bandcamp page: {}", off.status());
        let url = |u: &str| reqwest::Url::parse(u).unwrap();
        let media = Purpose::BandcampMedia;
        assert!(media.starts_at(&url("https://popplers5.bandcamp.com/download/track?enc=mp3-128&id=1")));
        assert!(media.starts_at(&url("https://bandcamp.com/stream_redirect?enc=mp3-128&track_id=1")));
        assert!(media.starts_at(&url("https://t4.bcbits.com/stream/1")), "its media hosts, as ever");
        assert!(!media.starts_at(&url("https://air.bandcamp.com/album/x")) && !media.starts_at(&url("https://bandcamp.com/album/x")), "no other Bandcamp page");
        assert!(!media.starts_at(&url("http://popplers5.bandcamp.com/download/track")), "https only");
        assert!(!media.starts_at(&url("https://popplers5.bandcamp.com:8443/download/track")), "its usual port only");
        assert!(!media.allows(&url("https://popplers5.bandcamp.com/download/track")), "a first hop, never a redirect's");
        assert!(!Purpose::Covers.starts_at(&url("https://popplers5.bandcamp.com/download/track")), "Bandcamp's media alone");
    }

    /// V9 (the review of run 8b's nits): a `popplers` host is one of Bandcamp's stream hosts only with its number.
    #[test]
    fn a_popplers_host_without_its_number_starts_no_stream() {
        let url = |u: &str| reqwest::Url::parse(u).unwrap();
        assert!(Purpose::BandcampMedia.starts_at(&url("https://popplers5.bandcamp.com/download/track")));
        assert!(!Purpose::BandcampMedia.starts_at(&url("https://popplers.bandcamp.com/download/track")));
    }

    /// V9: a 6to4 address is judged by the IPv4 address it carries; Teredo's and the IETF's protocol range (192.0.0.0/24) are not
    /// out in the world.
    #[test]
    fn six_to_four_teredo_and_the_ietf_range_are_judged() {
        for ip in ["2002:c0a8:0114::1", "2002:7f00:0001::", "2002:0a00:0005:1::1", "2001:0:4136:e378:8000:63bf:3fff:fdd2", "192.0.0.1", "192.0.0.170"] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["2002:5db8:d822::1", "2001:4860:4860::8888", "192.0.1.1", "192.1.0.1"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
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

    /// Y11: the special ranges are not out in the world either: multicast, 198.18/15, 240/4, fec0::/10; an IPv4 address carried in
    /// an IPv6 one (the old compatible form, NAT64's) is judged as itself.
    #[test]
    fn special_ranges_and_carried_addresses_are_judged() {
        let refused = ["224.0.0.1", "239.255.255.250", "198.18.0.1", "198.19.255.255", "240.0.0.1", "255.255.255.255", "ff02::1", "fec0::1", "::192.168.1.20", "::127.0.0.1", "64:ff9b::192.168.1.20", "64:ff9b::10.0.0.5"];
        for ip in refused {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["198.17.255.255", "198.20.0.1", "223.255.255.1", "64:ff9b::93.184.216.34", "::93.184.216.34"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
    }

    /// Y11: a photo host's address given as plain http is read over https; any other address stays as it is, and is not the photos'.
    #[test]
    fn a_photo_hosts_http_address_is_read_over_https() {
        let up = |u: &str| Purpose::ArtistPhotos.upgraded(reqwest::Url::parse(u).unwrap());
        let thumb = up("http://www.theaudiodb.com/images/media/artist/thumb/x.jpg");
        assert_eq!(thumb.as_str(), "https://www.theaudiodb.com/images/media/artist/thumb/x.jpg");
        assert!(Purpose::ArtistPhotos.allows(&thumb));
        assert_eq!(up("http://upload.wikimedia.org/a.jpg").scheme(), "https");
        let away = up("http://photos.example.org/x.jpg");
        assert!(away.scheme() == "http" && !Purpose::ArtistPhotos.allows(&away), "not one of its hosts");
        assert_eq!(up("http://www.theaudiodb.com:8080/x.jpg").scheme(), "http", "an odd port left as it is");
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

    /// Y11: a picture relayed or chosen is an image, read in pieces and cut at its cap: a page in its place is refused, a longer one
    /// cut there.
    #[tokio::test]
    async fn a_picture_read_is_an_image_cut_at_its_cap() {
        use axum::{http::header::CONTENT_TYPE, routing::get};
        let app = axum::Router::new()
            .route("/page", get(|| async { ([(CONTENT_TYPE, "text/html")], "<html>".repeat(10)) }))
            .route("/big", get(|| async { ([(CONTENT_TYPE, "image/jpeg")], vec![0xFFu8; 300_000]) }))
            .route("/small", get(|| async { ([(CONTENT_TYPE, "image/jpeg")], vec![0xFFu8; 1_000]) }));
        let at = fake(app).await;
        let c = reqwest::Client::new();
        let ask = |p: &str| c.get(format!("http://127.0.0.1:{at}{p}")).send();
        assert!(read_image(ask("/page").await.unwrap(), 100_000).await.unwrap_err().to_string().contains("not an image"));
        assert!(read_image(ask("/big").await.unwrap(), 100_000).await.unwrap_err().to_string().contains("cut there"));
        assert_eq!(read_image(ask("/small").await.unwrap(), 100_000).await.unwrap().len(), 1_000);
    }

    #[tokio::test]
    async fn the_stand_ins_answer_and_count_every_outside_ask() {
        let stand_ins = stand_ins::StandIns::set(std::sync::Arc::new(|a: &stand_ins::Asked| (a.source == "musicbrainz").then(|| (200, "{\"id\":\"r1\"}".to_string())))).await;
        let c = client(Purpose::Metadata).build().unwrap();
        let answer = c.get("https://musicbrainz.org/ws/2/release/r1?fmt=json").sent().await.unwrap();
        assert_eq!(answer.status(), 200);
        assert_eq!(answer.text().await.unwrap(), "{\"id\":\"r1\"}", "answered by the stand-in, never sent");
        let refused = c.get("https://api.discogs.com/releases/1").sent().await.unwrap();
        assert_eq!(refused.status().as_u16(), 599, "no stand-in: refused");
        let asked = stand_ins.asked();
        assert_eq!(asked.iter().map(|a| (a.source, a.path.as_str())).collect::<Vec<_>>(), [("musicbrainz", "/ws/2/release/r1"), ("discogs", "/releases/1")]);
        assert_eq!(asked[0].query, "fmt=json");
        // A held source's ask is counted as it comes and answered once let go.
        stand_ins.clear();
        stand_ins.hold("musicbrainz");
        let held = crate::run::spawn(async move { client(Purpose::Metadata).build().unwrap().get("https://musicbrainz.org/ws/2/x").sent().await.map(|r| r.status().as_u16()) });
        for _ in 0..100 {
            if stand_ins.count("musicbrainz") == 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(stand_ins.count("musicbrainz"), 1, "counted as it came");
        assert!(!held.is_finished(), "held");
        stand_ins.let_go();
        assert_eq!(held.await.unwrap().unwrap(), 200);
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
