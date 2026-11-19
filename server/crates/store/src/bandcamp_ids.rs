//! Bandcamp's pure parts the stores need (split from `bandcamp`, which re-exports them; SPLIT-PLAN P7): the source a preview's
//! tracks carry, their ids, Bandcamp's pages, and a release named the way a label's account names it.

/// The source a preview's tracks carry in a queue (`QueueTrack::source`): their streams and covers are relayed under
/// Cratarr's own addresses by it (`/api/stream/bandcamp/…`, `/api/file/…`, `/api/cover/…`).
pub const SOURCE: &str = "bandcamp";
/// What a preview's ids start with: no library id does, so listening leaves them out by the id alone.
pub const ID: &str = "bc.";

/// A preview's id read back: its album's page (on Bandcamp only) and the track's number.
pub fn read_id(id: &str) -> Option<(String, u32)> {
    use base64::Engine as _;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(id.strip_prefix(ID)?).ok()?;
    let (n, url) = std::str::from_utf8(&raw).ok()?.split_once('|')?;
    Some((album_page(url)?, n.parse().ok()?))
}

/// A preview's track (decision 0071): never a play, never in the history, never scrobbled.
pub fn is_preview(id: &str) -> bool {
    id.starts_with(ID)
}

/// Why a preview is refused wherever a track would be kept (a playlist, a download, a copy, a kept item): it is
/// played, never kept, and bought on Bandcamp (decision 0071, amended 2026-10-30).
pub const NOT_KEPT: &str = "A Bandcamp preview is not kept: buy it on Bandcamp.";

/// A page on Bandcamp's own domain only (`bandcamp.com` and its artists' subdomains): no other address is read. Its
/// address made plain (https, no port (Y11), no query, no fragment).
pub fn page(url: &str) -> Option<String> {
    let mut u = reqwest::Url::parse(url).ok()?;
    let host = u.host_str()?.to_ascii_lowercase();
    if !(host == "bandcamp.com" || host.ends_with(".bandcamp.com")) {
        return None;
    }
    u.set_scheme("https").ok()?;
    u.set_port(None).ok()?;
    u.set_query(None);
    u.set_fragment(None);
    Some(u.to_string())
}

/// An album's page (or a lone track's, which Bandcamp shows as an album of one).
pub fn album_page(url: &str) -> Option<String> {
    let p = page(url)?;
    let path = reqwest::Url::parse(&p).ok()?.path().to_string();
    (path.starts_with("/album/") || path.starts_with("/track/")).then_some(p)
}

/// A release as a label names it, sorted out (`label_release`): its artist, its title, and the label where it is a label's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    pub artist: String,
    pub title: String,
    pub label: Option<String>,
}

/// A release named the way a label's own account names its artists' on Bandcamp, sorted out (S12; C49a's rule too). The
/// owner's 0.16.2 case: "GROUCH - further (Up Records)" by "Up Records". Where the album's artist is the label's own (the
/// `label` the caller knows, an account or a publisher; or the name an "Artist - Title" closes on in brackets, the label
/// crediting itself, R18), the artist is the title's before " - ", the title what follows, a closing "(Label)" goes, and the label is that
/// account. The label's own release with no artist before a dash (its compilation) keeps the label as its artist. Any other
/// release is as named: a real artist's title with a dash in it, and a closing "(Remastered)" that names no label, stay.
/// Pure.
pub fn label_release(artist: &str, title: &str, label: Option<&str>) -> Named {
    let same = |a: &str, b: &str| {
        let (a, b) = (cratarr_rules::names::key(a), cratarr_rules::names::key(b));
        !a.is_empty() && a == b
    };
    let (artist, title) = (artist.trim(), title.trim());
    // The title's closing brackets: what comes before, and what they say.
    let closing = title.strip_suffix(')').and_then(|t| t.rfind('(').map(|at| (t[..at].trim_end(), &t[at + 1..])));
    let dash_at = |t: &str| [" - ", " \u{2013} ", " \u{2014} "].iter().filter_map(|d| t.find(d).map(|at| (at, d.len()))).min();
    let artist_title = |t: &str| dash_at(t).is_some_and(|(at, len)| !t[..at].trim().is_empty() && !t[at + len..].trim().is_empty());
    // Brackets naming the account say it is a label's only of an "Artist - Title" (R18, F10): a band's "Live (Orbit Lane)" by
    // Orbit Lane is its title.
    let credits_itself = closing.is_some_and(|(before, inside)| same(inside, artist) && artist_title(before));
    let labels = label.is_some_and(|l| same(l, artist)) || credits_itself;
    if !labels {
        return Named { artist: artist.to_string(), title: title.to_string(), label: None };
    }
    let title = match closing {
        Some((before, inside)) if same(inside, artist) && !before.is_empty() => before,
        _ => title,
    };
    match dash_at(title).map(|(at, len)| (title[..at].trim(), title[at + len..].trim())) {
        Some((who, what)) if !who.is_empty() && !what.is_empty() => Named { artist: who.to_string(), title: what.to_string(), label: Some(artist.to_string()) },
        _ => Named { artist: artist.to_string(), title: title.to_string(), label: Some(artist.to_string()) },
    }
}
