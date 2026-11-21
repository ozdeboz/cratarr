//! The References door (CU15, `ports::References`): the one way Cratarr asks an outside source who a record is. Three questions:
//! a reference's tracklist (C51a's reader: MusicBrainz's release or release group, Discogs' release or master, Bandcamp's album),
//! the recordings AcoustID knows a print as, and the releases MusicBrainz finds by an artist and a title. Identification only (Q10):
//! Discover's discographies, labels and related artists stay where they are.
//!
//! - **The real adapter** (`Real`) asks each source in its own turn (TN) and keeps each answer's class (Paused, Away, NotFound …):
//!   it adds no pace of its own. It alone calls `acoustid::lookup`, MusicBrainz's release search and `references::read_reference`
//!   (`only_the_door_asks_who_a_record_is`).
//! - **The callers** take the door as it is installed (`door()`): the gate (`track_fetch`), a card's hearing (`intake_card`),
//!   matching, and Soulseek's list (through `references::tracklist`'s reader, `read`). A test installs a stand-in (`StandIn`) on its
//!   thread, as `FakeAcoustid` and `FakeMusicBrainz` are installed: it answers only the questions it was built with, and any other
//!   is a violation.

use std::sync::Arc;

use anyhow::{Result, bail};
use serde_json::Value;

use crate::acoustid::{Found, Print};
use crate::catalogue::identity::{Ref, RefTrack};
use crate::source::Fut;

use super::references::{Read, ReadMore};

/// What a reference's list answers: its tracks, the release they were read from where the reference was a release group's (R25),
/// what the answer says besides (`ReadMore`), and, for a MusicBrainz release, what matching reads of it besides its list (`About`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Listing {
    pub tracks: Vec<RefTrack>,
    pub release: Option<Ref>,
    pub more: ReadMore,
    pub about: Option<About>,
}

impl Listing {
    /// As `references::tracklist`'s reader answers (`Read`).
    pub fn read(self) -> Read {
        (self.tracks, self.release, self.more)
    }

    /// A reader's answer as a listing, with nothing about it besides.
    pub fn of_read((tracks, release, more): Read) -> Listing {
        Listing { tracks, release, more, about: None }
    }
}

/// What a MusicBrainz release says of itself besides its list, as matching reads it: its title, artist (and the first credit's
/// id), year, first medium's format, release group, and each track's first artist's id, in the list's order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct About {
    pub title: String,
    pub artist: String,
    pub artist_id: Option<String>,
    pub year: Option<u32>,
    pub format: String,
    pub group: Option<String>,
    pub by: Vec<Option<String>>,
}

/// A release MusicBrainz's search finds by an artist and a title: its id, the search's score (0 to 100) and its tracks' count
/// where the answer gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub id: String,
    pub score: i64,
    pub tracks: Option<usize>,
}

/// The door (`ports::References`, CU15): who a record is, asked of the outside world. Each answer keeps its class (TN.1): a source
/// paused says until when, one switched off says so, one with no such thing answers none.
pub trait References: Send + Sync {
    /// A reference's list (C51a's reader): none where the source has no such thing.
    fn tracklist(&self, r: Ref) -> Fut<'_, Option<Listing>>;
    /// The recordings AcoustID knows a print as, each with its score.
    fn recordings<'a>(&'a self, print: &'a Print) -> Fut<'a, Vec<Found>>;
    /// The releases MusicBrainz's search finds by an artist and a title, the best first.
    fn find<'a>(&'a self, artist: &'a str, title: &'a str) -> Fut<'a, Vec<Candidate>>;
}

/// The real adapter: each source in its own turn, as each always was asked. AcoustID with the server's key, or the one it was made
/// with (the tests').
#[derive(Default)]
pub struct Real {
    acoustid_key: Option<String>,
}

impl Real {
    /// The adapter with the server's own keys.
    pub fn new() -> Real {
        Real::default()
    }

    /// The adapter asking AcoustID with `key` (the tests', whose fake AcoustID wants one).
    #[cfg(any(test, feature = "test-support"))]
    pub fn keyed(key: &str) -> Real {
        Real { acoustid_key: Some(key.to_string()) }
    }
}

/// Where the server's AcoustID key is kept (user data's server choices).
pub const ACOUSTID_KEY: &str = "acoustid:key";

/// The server's AcoustID key, where set. Blocking.
pub fn acoustid_key(ud: &crate::userdata::UserData) -> Result<Option<String>> {
    Ok(ud.import_choice(ACOUSTID_KEY)?.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
}

impl References for Real {
    fn tracklist(&self, r: Ref) -> Fut<'_, Option<Listing>> {
        Box::pin(async move {
            if r.source == "musicbrainz" && r.kind == "release" {
                return musicbrainz_release(&r.value).await;
            }
            Ok(super::references::read_reference(r).await?.map(Listing::of_read))
        })
    }

    fn recordings<'a>(&'a self, print: &'a Print) -> Fut<'a, Vec<Found>> {
        Box::pin(async move {
            let ud = crate::serve::instance().map(|s| s.userdata.clone());
            let key = match (self.acoustid_key.clone(), ud) {
                (Some(key), _) => Some(key),
                (None, Some(ud)) => crate::run::spawn_blocking(move || acoustid_key(&ud)).await??,
                (None, None) => None,
            };
            let Some(key) = key else { bail!("AcoustID is not set up: Settings › Server") };
            crate::acoustid::lookup(&key, print).await
        })
    }

    fn find<'a>(&'a self, artist: &'a str, title: &'a str) -> Fut<'a, Vec<Candidate>> {
        Box::pin(async move { musicbrainz_search(artist, title).await })
    }
}

/// A MusicBrainz release read whole, in MusicBrainz's turn (`track_fetch_records::release_by_id`): its list, its Bandcamp page where
/// it links one, and what matching reads of it. None where MusicBrainz has no such release.
async fn musicbrainz_release(id: &str) -> Result<Option<Listing>> {
    let not_found = |e: &anyhow::Error| e.downcast_ref::<reqwest::Error>().and_then(reqwest::Error::status) == Some(reqwest::StatusCode::NOT_FOUND);
    match super::track_fetch_records::release_by_id(id).await {
        Ok(v) => {
            let about = About {
                title: v.title.clone(),
                artist: v.artist.clone(),
                artist_id: v.artist_id.clone(),
                year: v.year,
                format: v.format.clone(),
                group: v.group.clone(),
                by: v.tracks.iter().map(|t| t.artist_id.clone()).collect(),
            };
            Ok(Some(Listing { tracks: super::references::list_of_release(&v), release: None, more: super::references::musicbrainz_more(&v.links), about: Some(about) }))
        }
        Err(e) if not_found(&e) => Ok(None),
        Err(e) => Err(e),
    }
}

/// A Lucene phrase, its quotes and backslashes escaped.
fn phrase(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// MusicBrainz's search for releases by their title and artist (matching's, 0.9.7), in its turn: up to ten, the best first.
async fn musicbrainz_search(artist: &str, title: &str) -> Result<Vec<Candidate>> {
    use super::musicbrainz::Asked as _;
    let query = format!("release:{} AND artist:{}", phrase(title), phrase(artist));
    let mut url = reqwest::Url::parse(&format!("{}/release", super::discover::mb_ws()))?;
    url.query_pairs_mut().append_pair("query", &query).append_pair("limit", "10").append_pair("fmt", "json");
    let v: Value = super::discover::http()?.get(url).asked().await?.error_for_status()?.json().await?;
    Ok(v["releases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| Some(Candidate { id: r["id"].as_str()?.to_string(), score: r["score"].as_i64().unwrap_or(0), tracks: r["track-count"].as_u64().map(|n| n as usize) }))
        .collect())
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    // The test's stand-in on this thread (`StandIn::install`).
    static STAND_IN: std::cell::RefCell<Option<Arc<dyn References>>> = const { std::cell::RefCell::new(None) };
}

/// The door as installed: the real adapter, or a test's stand-in on this thread.
pub fn door() -> Arc<dyn References> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(d) = STAND_IN.with(|s| s.borrow().clone()) {
        return d;
    }
    Arc::new(Real::new())
}

/// `references::tracklist`'s reader (C51a) through the door as installed now.
pub fn read(r: Ref) -> impl std::future::Future<Output = Result<Option<Read>>> + Send + 'static {
    let d = door();
    async move { Ok(d.tracklist(r).await?.map(Listing::read)) }
}

/// A reader through `d`.
pub fn read_through(d: Arc<dyn References>) -> impl Fn(Ref) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Read>>> + Send>> + Send + Sync + Clone + 'static {
    move |r: Ref| {
        let d = d.clone();
        Box::pin(async move { Ok(d.tracklist(r).await?.map(Listing::read)) })
    }
}

/// What AcoustID knows of a print, through the door as installed now.
pub fn recordings(print: Print) -> impl std::future::Future<Output = Result<Vec<Found>>> + Send + 'static {
    let d = door();
    async move { d.recordings(&print).await }
}

/// A stand-in door for the tests (CU15's fakes): built with the questions it expects, each answered as scripted. Anything else is a
/// violation, answered with an error and kept (`violations`); every question is kept as asked (`asked`).
#[cfg(any(test, feature = "test-support"))]
#[derive(Default)]
pub struct StandIn {
    lists: std::sync::Mutex<Vec<(Ref, Answer<Option<Listing>>)>>,
    prints: std::sync::Mutex<Option<Answer<Vec<Found>>>>,
    finds: std::sync::Mutex<Vec<((String, String), Answer<Vec<Candidate>>)>>,
    asked: std::sync::Mutex<Vec<Asked>>,
    violations: std::sync::Mutex<Vec<String>>,
}

/// A scripted answer: as given, or an error made fresh at each question (an error is no `Clone`).
#[cfg(any(test, feature = "test-support"))]
pub enum Answer<T> {
    Gives(T),
    Fails(Arc<dyn Fn() -> anyhow::Error + Send + Sync>),
}

#[cfg(any(test, feature = "test-support"))]
impl<T: Clone> Answer<T> {
    fn give(&self) -> Result<T> {
        match self {
            Answer::Gives(t) => Ok(t.clone()),
            Answer::Fails(e) => Err(e()),
        }
    }
}

/// A question the stand-in was asked: a tracklist by its reference, a print's recordings by its fingerprint and length, a find by its
/// artist and title.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, PartialEq)]
pub enum Asked {
    Tracklist(Ref),
    Recordings { fingerprint: String, duration_secs: f64 },
    Find(String, String),
}

#[cfg(any(test, feature = "test-support"))]
impl StandIn {
    pub fn new() -> Self {
        Self::default()
    }

    /// Expects `r`'s list, answering `list`.
    pub fn tracklist(self, r: Ref, list: Option<Listing>) -> Self {
        self.lists.lock().unwrap().push((r, Answer::Gives(list)));
        self
    }

    /// Expects `r`'s list, failing with what `e` makes.
    pub fn tracklist_fails(self, r: Ref, e: impl Fn() -> anyhow::Error + Send + Sync + 'static) -> Self {
        self.lists.lock().unwrap().push((r, Answer::Fails(Arc::new(e))));
        self
    }

    /// Expects prints' recordings (any print), answering `found`.
    pub fn recordings(self, found: Vec<Found>) -> Self {
        *self.prints.lock().unwrap() = Some(Answer::Gives(found));
        self
    }

    /// Expects prints' recordings, failing with what `e` makes.
    pub fn recordings_fail(self, e: impl Fn() -> anyhow::Error + Send + Sync + 'static) -> Self {
        *self.prints.lock().unwrap() = Some(Answer::Fails(Arc::new(e)));
        self
    }

    /// Expects a find of `artist`'s `title`, answering `found`.
    pub fn find(self, artist: &str, title: &str, found: Vec<Candidate>) -> Self {
        self.finds.lock().unwrap().push(((artist.to_string(), title.to_string()), Answer::Gives(found)));
        self
    }

    /// The questions asked, in order.
    pub fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }

    /// The questions it did not expect.
    pub fn violations(&self) -> Vec<String> {
        self.violations.lock().unwrap().clone()
    }

    /// This stand-in as the door on this thread until the guard is dropped; the guard reads what it was asked.
    pub fn install(self) -> Installed {
        let me = Arc::new(self);
        STAND_IN.with(|s| *s.borrow_mut() = Some(me.clone()));
        Installed(me)
    }

    fn violation<T>(&self, said: String) -> Result<T> {
        self.violations.lock().unwrap().push(said.clone());
        bail!("not expected: {said}")
    }
}

#[cfg(any(test, feature = "test-support"))]
impl References for StandIn {
    fn tracklist(&self, r: Ref) -> Fut<'_, Option<Listing>> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(Asked::Tracklist(r.clone()));
            let answer = self.lists.lock().unwrap().iter().find(|(k, _)| *k == r).map(|(_, a)| a.give());
            match answer {
                Some(a) => a,
                None => self.violation(format!("tracklist {}:{}:{}", r.source, r.kind, r.value)),
            }
        })
    }

    fn recordings<'a>(&'a self, print: &'a Print) -> Fut<'a, Vec<Found>> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(Asked::Recordings { fingerprint: print.fingerprint.clone(), duration_secs: print.duration_secs });
            let answer = self.prints.lock().unwrap().as_ref().map(Answer::give);
            match answer {
                Some(a) => a,
                None => self.violation("recordings".to_string()),
            }
        })
    }

    fn find<'a>(&'a self, artist: &'a str, title: &'a str) -> Fut<'a, Vec<Candidate>> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(Asked::Find(artist.to_string(), title.to_string()));
            let answer = self.finds.lock().unwrap().iter().find(|((a, t), _)| a == artist && t == title).map(|(_, a)| a.give());
            match answer {
                Some(a) => a,
                None => self.violation(format!("find {artist} / {title}")),
            }
        })
    }
}

/// A stand-in installed on this thread (`StandIn::install`); dropped, the door is the real one again.
#[cfg(any(test, feature = "test-support"))]
pub struct Installed(Arc<StandIn>);

#[cfg(any(test, feature = "test-support"))]
impl std::ops::Deref for Installed {
    type Target = StandIn;
    fn deref(&self) -> &StandIn {
        &self.0
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for Installed {
    fn drop(&mut self) {
        STAND_IN.with(|s| *s.borrow_mut() = None);
    }
}

/// A list of `tracks` (title, ms), on disc 1 in order, as a stand-in's tracklist answers it.
#[cfg(any(test, feature = "test-support"))]
pub fn listing(tracks: &[(&str, Option<i64>)]) -> Listing {
    let tracks = tracks.iter().zip(1..).map(|((title, ms), position)| RefTrack { disc: 1, position, title: title.to_string(), length_ms: *ms, recording: None }).collect();
    Listing { tracks, ..Listing::default() }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::app::references::tests::wanted;
    use crate::app::references::tracklist;
    use crate::app::references_turn::SwitchedOff;
    use crate::app::turn::{Failure, Pinned, class};
    use crate::catalogue::Catalogue;

    fn release(id: &str) -> Ref {
        Ref::new("musicbrainz", "release", id)
    }

    /// CU15.4: Soulseek's list through the door (`references::tracklist`, its reader the door): C51a's order, MusicBrainz's release
    /// asked first, then Discogs', then Bandcamp's (decision 0105 rule 1: the order says only who is asked first); a source switched
    /// off passed over; "no such release" giving way to the next; C51a's own cases answered by the stand-in as by their readers. A
    /// list is read once for a record in a round. (Soulseek's own job reads no Bandcamp page: THE Bandcamp rule, DC2's KM13.)
    #[tokio::test]
    async fn soulseek_reads_its_list_through_the_door() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let (bandcamp, discogs) = (Ref::new("bandcamp", "album", "https://kitetheory.bandcamp.com/album/night-walks"), Ref::new("discogs", "release", "2000003"));
        let all = wanted(&cat, "Night Walks", &[("bandcamp", "album", &bandcamp.value), ("discogs", "release", "2000003"), ("musicbrainz", "release", "r3")]);
        let lamplight = || Some(listing(&[("Lamplight", Some(200_000))]));
        let door = StandIn::new().tracklist(release("r3"), lamplight()).tracklist(discogs.clone(), lamplight()).tracklist(bandcamp.clone(), lamplight()).install();
        assert_eq!(tracklist(&cat, all, read).await.unwrap().unwrap().timed.unwrap().0, "musicbrainz", "MusicBrainz's release asked first");
        assert_eq!(tracklist(&cat, all, read).await.unwrap().unwrap().timed.unwrap().0, "musicbrainz", "kept: asked again, nothing read");
        assert_eq!(door.asked(), [Asked::Tracklist(release("r3"))], "read once");
        assert!(door.violations().is_empty(), "{:?}", door.violations());
        drop(door);

        // No such release gives way to the next.
        let undated = Ref::new("bandcamp", "album", "https://kitetheory.bandcamp.com/album/undated");
        let gone = wanted(&cat, "Undated", &[("bandcamp", "album", &undated.value), ("musicbrainz", "release", "r4")]);
        let door = StandIn::new().tracklist(release("r4"), None).tracklist(undated.clone(), Some(listing(&[("Morning", Some(200_000))]))).install();
        assert_eq!(tracklist(&cat, gone, read).await.unwrap().unwrap().timed.unwrap().0, "bandcamp", "MusicBrainz has no such release: the next");
        assert_eq!(door.asked(), [Asked::Tracklist(release("r4")), Asked::Tracklist(undated)]);
        drop(door);

        // A source switched off is passed over (X3, Y12), and alone it is out of reach.
        let both = wanted(&cat, "Night Walks II", &[("musicbrainz", "release", "r5"), ("discogs", "release", "2000005")]);
        let five = Ref::new("discogs", "release", "2000005");
        let door = StandIn::new().tracklist_fails(release("r5"), || SwitchedOff("MusicBrainz").into()).tracklist(five, lamplight()).install();
        assert_eq!(tracklist(&cat, both, read).await.unwrap().unwrap().timed.unwrap().0, "discogs");
        drop(door);
        let alone = wanted(&cat, "Alone", &[("musicbrainz", "release", "r6")]);
        let door = StandIn::new().tracklist_fails(release("r6"), || SwitchedOff("MusicBrainz").into()).install();
        assert!(tracklist(&cat, alone, read).await.unwrap_err().is::<SwitchedOff>());
        assert!(door.violations().is_empty(), "{:?}", door.violations());
    }

    /// Strips what only the tests compile from a source file: an item under `#[cfg(test)]` (or `#[cfg(any(test, …))]`), and lines that
    /// are comments.
    fn production(text: &str) -> String {
        let mut out = String::new();
        let mut lines = text.lines();
        while let Some(line) = lines.next() {
            let t = line.trim_start();
            if t.starts_with("#[cfg(test)]") || t.starts_with("#[cfg(any(test") {
                let (mut depth, mut opened) = (0i32, false);
                for l in lines.by_ref() {
                    let code = l.trim_start();
                    if code.starts_with("//") {
                        continue;
                    }
                    for c in code.chars() {
                        match c {
                            '{' => {
                                depth += 1;
                                opened = true;
                            }
                            '}' => depth -= 1,
                            _ => {}
                        }
                    }
                    if (opened && depth <= 0) || (!opened && code.trim_end().ends_with(';')) {
                        break;
                    }
                }
                continue;
            }
            if t.starts_with("//") {
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    /// Every `.rs` file of the server's crates and binary, but the tests' own files.
    fn server_sources() -> Vec<(std::path::PathBuf, String)> {
        fn walk(dir: &std::path::Path, out: &mut Vec<(std::path::PathBuf, String)>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                if p.is_dir() {
                    if name != "tests" && name != "target" {
                        walk(&p, out);
                    }
                } else if name.ends_with(".rs") && !name.ends_with("_tests.rs") && name != "tests.rs" {
                    out.push((p.clone(), std::fs::read_to_string(&p).unwrap()));
                }
            }
        }
        let server = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut out = Vec::new();
        walk(&server.join("crates"), &mut out);
        walk(&server.join("src"), &mut out);
        out
    }

    /// CU15.5: only the door asks who a record is. Outside the tests, AcoustID's lookup and `references::read_reference` are called
    /// only from the door's adapter (this file), and matching asks MusicBrainz nothing itself. Never a new caller outside it.
    #[test]
    fn only_the_door_asks_who_a_record_is() {
        let word = |code: &str, w: &str| {
            code.match_indices(w).any(|(at, _)| {
                let before = code[..at].chars().next_back();
                let after = code[at + w.len()..].chars().next();
                !before.is_some_and(|c| c.is_alphanumeric() || c == '_') && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
            })
        };
        let mut found = Vec::new();
        for (path, text) in server_sources() {
            if path.ends_with("references_door.rs") {
                continue;
            }
            let code = production(&text);
            for line in code.lines() {
                let defines = line.contains("fn read_reference") || line.contains("fn lookup(");
                if !defines && (line.contains("acoustid::lookup(") || word(line, "read_reference")) {
                    found.push(format!("{}: {}", path.display(), line.trim()));
                }
            }
            if path.ends_with("matching.rs") {
                for asks in ["musicbrainz.org", "Asked as _", ".asked()", "mb_ws("] {
                    if code.contains(asks) {
                        found.push(format!("{}: matching asks MusicBrainz itself ({asks})", path.display()));
                    }
                }
            }
        }
        assert!(found.is_empty(), "asked outside the door:\n{}", found.join("\n"));
    }

    /// CU15.6: the real adapter keeps each source's class through the door: MusicBrainz held until t0+60 answers Paused until then,
    /// at once, its question never sent; AcoustID's code 14 answers Paused until its minute is out. The door adds no pace of its own:
    /// its questions go at each source's turn.
    #[tokio::test(start_paused = true)]
    async fn the_door_keeps_each_sources_class() {
        let _one = crate::app::switches::TEST_STATE.write().await;
        let t0 = Pinned::now();
        let t0 = t0.0;
        let turn = &crate::app::musicbrainz::TURN;
        let was = turn.as_held();
        turn.hold_for(Duration::from_secs(60));
        let door = Real::keyed("key");
        let start = tokio::time::Instant::now();
        let held = door.tracklist(release("00000000-0000-4000-8000-000000000301")).await;
        let group = door.tracklist(Ref::new("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000302")).await;
        turn.put_back(was);
        assert_eq!(class(&held.unwrap_err()), Failure::Paused { until: t0 + 60 }, "paused until then, never NotFound or Away");
        assert_eq!(class(&group.unwrap_err()), Failure::Paused { until: t0 + 60 }, "a release group's read alike");
        assert_eq!(tokio::time::Instant::now(), start, "told at once: no wait of the door's own");

        let error = r#"{"status":"error","error":{"code":14,"message":"rate limit exceeded"}}"#;
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let _fake = {
            let asked = asked.clone();
            crate::acoustid::FakeAcoustid::answering(move |_| {
                let mut asked = asked.lock().unwrap();
                asked.push(tokio::time::Instant::now());
                let (status, body) = if asked.len() == 1 { (200, r#"{"status":"ok","results":[]}"#) } else { (429, error) };
                crate::acoustid::Heard { status: reqwest::StatusCode::from_u16(status).unwrap(), headers: Default::default(), body: body.to_string() }
            })
        };
        let print = crate::app::turn::fakes::print();
        door.recordings(&print).await.unwrap();
        let limited = door.recordings(&print).await.unwrap_err();
        assert_eq!(class(&limited), Failure::Paused { until: t0 + 60 }, "AcoustID's limit is its minute's pause");
        let sent = asked.lock().unwrap().clone();
        assert_eq!(sent[1] - sent[0], Duration::from_millis(350), "at AcoustID's turn, and no more apart");
    }

    /// CU15.7: two askers at once share AcoustID's one turn through the real door: a card's hearing of 7 tracks and an item's check of
    /// 5 files, 12 lookups in all, each 350 ms or more after the one before, in the order they came. One asker's error stays its own.
    #[tokio::test(start_paused = true)]
    async fn two_askers_share_acoustids_one_turn() {
        let asked = Arc::new(std::sync::Mutex::new(Vec::<(tokio::time::Instant, String)>::new()));
        let _fake = {
            let asked = asked.clone();
            crate::acoustid::FakeAcoustid::answering(move |form| {
                let print = form.iter().find(|(k, _)| *k == "fingerprint").map(|(_, v)| v.clone()).unwrap_or_default();
                asked.lock().unwrap().push((tokio::time::Instant::now(), print.clone()));
                // The card's third track is a print AcoustID cannot read (its code 3): the card's error alone.
                let body = if print == "card-3" { r#"{"status":"error","error":{"code":3,"message":"invalid fingerprint"}}"# } else { r#"{"status":"ok","results":[{"score":0.97,"recordings":[{"id":"r"}]}]}"# };
                crate::acoustid::Heard { status: reqwest::StatusCode::OK, headers: Default::default(), body: body.to_string() }
            })
        };
        let door: Arc<dyn References> = Arc::new(Real::keyed("key"));
        let print = |name: String| Print { fingerprint: name, ..crate::app::turn::fakes::print() };
        let asker = |who: &'static str, n: usize| {
            let door = door.clone();
            async move {
                let mut answers = Vec::new();
                for i in 1..=n {
                    answers.push(door.recordings(&print(format!("{who}-{i}"))).await.map(|f| f.len()));
                }
                answers
            }
        };
        let (card, check) = tokio::join!(asker("card", 7), asker("check", 5));
        let sent = asked.lock().unwrap().clone();
        assert_eq!(sent.len(), 12, "every lookup asked once");
        assert!(sent.windows(2).all(|w| w[1].0 - w[0].0 >= Duration::from_millis(350)), "{sent:?}");
        let order = |who: &str| sent.iter().filter(|(_, p)| p.starts_with(who)).map(|(_, p)| p.clone()).collect::<Vec<_>>();
        assert_eq!(order("card"), (1..=7).map(|i| format!("card-{i}")).collect::<Vec<_>>(), "in the order they came");
        assert_eq!(order("check"), (1..=5).map(|i| format!("check-{i}")).collect::<Vec<_>>());
        assert!(card[2].is_err() && card.iter().enumerate().all(|(i, a)| i == 2 || a.is_ok()), "the card's error is the card's: {card:?}");
        assert!(check.iter().all(|a| matches!(a, Ok(1))), "none of it on the check: {check:?}");
    }
}
