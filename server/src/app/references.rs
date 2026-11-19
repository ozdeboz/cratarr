//! A record's references (0.14, the owner, 2026-11-02: one Health item for them): each source that may know the record
//! (MusicBrainz, Discogs, Bandcamp, AcoustID), as agreeing, differing, found (known, not compared) or missing, with what
//! it says (its tracks, year, type, page); which one counts; and whether the record was checked by hand. A record no
//! reference speaks for is Health's item.
//!
//! - **Kept in the catalogue** (decision 0100, `catalogue::records`): what each source knows the record by, and the tracklist
//!   each gives. What 0.14 kept as one JSON an album (`Chosen`, in `record_references`) moved there at 0.17's step
//!   (`step_017`), which alone reads it now.
//! - **One measure of agreeing:** a source's list agrees with the files as `identity::agrees` says, row and count alike, and
//!   the one that counts is `identity::counting`'s, worked out whenever a list arrives (`work_out`) unless chosen by hand: "Use
//!   this", or a link pasted.
//! - **A list read once:** one not kept yet is read when the record is opened, and kept: MusicBrainz's release (its own titles,
//!   which a match keeps none of), Discogs' release. Bandcamp's page is looked up when the record is opened, as before, and read
//!   then; what it found is kept as a guess.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::catalogue::Catalogue;
use crate::catalogue::identity::{self, Listed, RefTrack};
use crate::catalogue::records::{RecordRow, Ref, ReferenceRow};
use crate::serve::ids;

/// The sources, in the order a record's entry lists them.
pub const SOURCES: [&str; 4] = ["musicbrainz", "discogs", "bandcamp", "acoustid"];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct References {
    pub album: String,
    pub title: String,
    pub artist: String,
    pub sources: Vec<Source>,
    /// The source that counts: worked out from the lists and the files, or chosen ("Use this", a link pasted); none where
    /// none fits and none was chosen.
    pub counting: Option<String>,
    pub by_hand: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Source {
    pub source: &'static str,
    /// agrees, differs, found (known, not held against the record), missing.
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracks: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Its tracks' lengths (ms), in its order, where it has them: side by side with the record's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lengths: Option<Vec<i64>>,
    /// Found by guessing (a search by name, not an id the files carry), until confirmed once.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub guessed: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub confirmed: bool,
    /// One sentence: what it says, or why it differs.
    pub said: String,
}

/// What 0.14 kept of a record's references (`record_references`, JSON an album). Read by 0.17's step alone, which moves it
/// into the catalogue: `counting` a choice by hand, `by_hand` "Checked by hand", the links pasted, the guesses confirmed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Chosen {
    #[serde(default)]
    pub counting: Option<String>,
    #[serde(default)]
    pub by_hand: bool,
    /// Links pasted by hand, by source.
    #[serde(default)]
    pub links: std::collections::BTreeMap<String, String>,
    /// Sources found by guessing that were confirmed once.
    #[serde(default)]
    pub confirmed: std::collections::BTreeSet<String>,
}

/// What a pasted link names: a MusicBrainz release or release group, a Discogs release or master (a language before it, a
/// name after its number, or Discogs' older `/master/view/N`, which 0.16 kept as pasted), a Bandcamp album page. None for
/// anything else.
pub fn ref_of_link(url: &str) -> Option<Ref> {
    let url = url.trim();
    let u = reqwest::Url::parse(url).ok()?;
    if !matches!(u.scheme(), "https" | "http") {
        return None;
    }
    let host = u.host_str()?.to_ascii_lowercase();
    let segments: Vec<&str> = u.path_segments()?.filter(|s| !s.is_empty()).collect();
    if host == "musicbrainz.org" || host.ends_with(".musicbrainz.org") {
        let [kind @ ("release" | "release-group"), id, ..] = segments.as_slice() else { return None };
        let id = id.to_ascii_lowercase();
        return super::discogs::is_mbid(&id).then(|| Ref::new("musicbrainz", kind, &id));
    }
    if host == "discogs.com" || host.ends_with(".discogs.com") {
        let kind = segments.iter().position(|s| *s == "release" || *s == "master")?;
        let at = if segments.get(kind + 1) == Some(&"view") { kind + 1 } else { kind };
        let number: u64 = segments.get(at + 1)?.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()?;
        return Some(Ref::new("discogs", segments[kind], &number.to_string()));
    }
    if host.ends_with(".bandcamp.com") && u.path().starts_with("/album/") {
        return crate::bandcamp::album_page(url).map(|page| Ref::new("bandcamp", "album", &page));
    }
    None
}

/// A reference's page at its source.
pub(crate) fn url_of(r: &Ref) -> Option<String> {
    Some(match (r.source.as_str(), r.kind.as_str()) {
        ("musicbrainz", "release" | "release-group") => format!("https://musicbrainz.org/{}/{}", r.kind, r.value),
        ("discogs", "release" | "master") => format!("https://www.discogs.com/{}/{}", r.kind, r.value),
        ("bandcamp", "album") => r.value.clone(),
        _ => return None,
    })
}

/// MusicBrainz's list of a release, as `track_fetch` reads it: its own titles, lengths and recordings.
pub fn list_of_release(v: &super::track_fetch::ReleaseView) -> Vec<RefTrack> {
    v.tracks
        .iter()
        .map(|t| RefTrack { disc: t.disc, position: t.position, title: t.title.clone(), length_ms: t.length_ms, recording: Some(t.recording.clone()).filter(|r| !r.is_empty()) })
        .collect()
}

/// Discogs' list of a release (its JSON), each of its media in turn, as Renumber reads them; no recordings.
pub fn list_of_discogs(v: &serde_json::Value) -> Vec<RefTrack> {
    let release = super::renumber::discogs_release(v);
    release
        .media
        .iter()
        .zip(1..)
        .flat_map(|(m, disc)| m.tracks.iter().zip(1..).map(move |((title, secs), position)| RefTrack { disc, position, title: title.clone(), length_ms: secs.map(|s| i64::from(s) * 1000), recording: None }))
        .collect()
}

/// Bandcamp's list of an album page.
pub fn list_of_bandcamp(a: &crate::bandcamp::Album) -> Vec<RefTrack> {
    a.tracks.iter().map(|t| RefTrack { disc: 1, position: t.n, title: t.title.clone(), length_ms: t.duration_secs.map(|s| (s * 1000.0).round() as i64), recording: None }).collect()
}

/// A source's list where one reference names it alone: a MusicBrainz release, a Discogs release. A release group or a master
/// has no one list.
async fn read_list(r: &Ref) -> Result<Option<Vec<RefTrack>>> {
    Ok(match (r.source.as_str(), r.kind.as_str()) {
        ("musicbrainz", "release") => Some(list_of_release(&super::track_fetch::release_by_id(&r.value).await?)),
        ("discogs", "release") => super::discogs::release_raw(r.value.parse()?).await?.map(|v| list_of_discogs(&v)),
        _ => None,
    })
}

// ---- A record's counting list (C51a)

/// A source by the name the pages give it.
fn source_name(source: &str) -> &'static str {
    match source {
        "musicbrainz" => "MusicBrainz",
        "discogs" => "Discogs",
        "bandcamp" => "Bandcamp",
        _ => "Its source",
    }
}

/// The lists a record's Soulseek folders are judged by (R33, N17, as the owner amended it): the first in the design's order whose
/// every track has its length, folders judged by titles and lengths; and the first without them, read before it, judged by titles
/// and count only where the first finds no folder (`soulseek::untimed_folders`). One at least.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Judging {
    pub timed: Option<(String, Vec<RefTrack>)>,
    pub untimed: Option<(String, Vec<RefTrack>)>,
}

/// Whether every track of a list has its length: a list a folder can be judged by, by titles and lengths (R33).
pub fn timed(list: &[RefTrack]) -> bool {
    list.iter().all(|t| t.length_ms.is_some_and(|ms| ms > 0))
}

/// A record's lists, the one that counts first (C51a, Q31: what judges a Soulseek folder), each read once through `read` and kept
/// (`reference_tracks`), with its source (`Judging`):
/// - **Counted** (by hand, or worked out): that source's list, kept or read now; where it has no lengths, the others in the
///   design's order, for one with them (R33), the count left as it is.
/// - **Not had**: no files to hold a list against, so "none until chosen" is not its case: its references count in the
///   design's order (`identity::COUNTING_ORDER`: MusicBrainz's release, else its release group's; Discogs'; Bandcamp's album),
///   the first whose list answers, and that source counts from then on, lengths or none. One that answers nothing (no such
///   release) gives way to the next, and so does one without lengths, for judging (R33).
/// - **Had, no count**: its files and no list agree, so none counts until an administrator picks one (the owner, 2026-11-16):
///   nothing is read. One whose files are short of the list they name (W4: "partly there") is judged by that list, counted by
///   nothing.
///
/// `Ok(Err(why))`: no list to judge by, and why in few words. `Err`: a source could not be reached; asked again later.
pub async fn tracklist<R, F>(cat: &Arc<Catalogue>, record: i64, read: R) -> Result<std::result::Result<Judging, String>>
where
    R: Fn(Ref) -> F,
    F: std::future::Future<Output = Result<Option<Read>>>,
{
    let j = judge(cat, record, read).await?;
    if let Some(e) = j.stop {
        return Err(e);
    }
    match (j.answer, j.off) {
        // Switched off (R23, N16) where no other gave a list: out of reach until it is on again.
        (Err(_), Some(off)) => Err(off),
        (answer, _) => Ok(answer),
    }
}

/// What a source said of a record's list where it gave none (F1): switched off, held back after Bandcamp's bot check, not
/// answering (out of reach, or a page's read cut at `PAGE_READ_AT_MOST`), or no list of it (no such release, none with tracks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Off,
    Held,
    Unanswered,
    None,
}

impl Outcome {
    /// What a reader's failure says: a source switched off, Bandcamp's bot check, else no answer.
    pub(crate) fn of(e: &anyhow::Error) -> Outcome {
        if e.is::<SwitchedOff>() {
            Outcome::Off
        } else if crate::bandcamp::not_read(e) {
            Outcome::Held
        } else {
            Outcome::Unanswered
        }
    }

    /// As a record not had's page says it (`Said::said`).
    fn word(self) -> &'static str {
        match self {
            Outcome::Off => "off",
            Outcome::Held => "held",
            Outcome::Unanswered => "unanswered",
            Outcome::None => "none",
        }
    }
}

/// What `tracklist`'s reading came to (F1.11: one set of outcomes, two wordings: Soulseek's `why`, the page's `no_list`): its answer
/// where no source stopped it; what each source read said where it gave no list, in the order read, up to the first that did not
/// answer; that one's failure, and a source's switched off.
struct Judged {
    answer: std::result::Result<Judging, String>,
    said: Vec<(&'static str, Outcome)>,
    /// The failure that stopped it: a source out of reach, Bandcamp's bot check.
    stop: Option<anyhow::Error>,
    /// A source switched off, passed over.
    off: Option<anyhow::Error>,
}

/// `tracklist`'s reading and keeping, each source's outcome kept (`Judged`): `tracklist` words it for Soulseek, a page's read
/// (`read_soon_with`) for the page.
async fn judge<R, F>(cat: &Arc<Catalogue>, record: i64, read: R) -> Result<Judged>
where
    R: Fn(Ref) -> F,
    F: std::future::Future<Output = Result<Option<Read>>>,
{
    let answered = |answer: std::result::Result<Judging, String>| Judged { answer, said: Vec::new(), stop: None, off: None };
    let c = cat.clone();
    let Some((row, refs, kept, short)) = tokio::task::spawn_blocking(move || -> Result<_> {
        let Some(row) = c.record(record)? else { return Ok(None) };
        let refs = c.references_of(&ids::record(row.id))?;
        let kept = c.reference_tracks(row.id)?;
        // Had, short of the list its files name (W4: "partly there"; Health's "Ask for the full album").
        let short = row.had && c.requested(&[row.id])?.get(&row.id).is_some_and(|q| q.list.is_some_and(|l| q.present < l));
        Ok(Some((row, refs, kept, short)))
    })
    .await??
    else {
        return Ok(answered(Err("No such record".into())));
    };
    let kept_of = |source: &str| kept.iter().find(|(s, l)| s == source && !l.is_empty()).map(|(_, l)| l.clone());
    let keep = |source: &'static str, list: Vec<RefTrack>, release: Option<Ref>, count: bool, read: Option<(Ref, ReadMore)>| {
        let (c, id, had) = (cat.clone(), row.id, row.had);
        async move {
            tokio::task::spawn_blocking(move || -> Result<()> {
                c.set_reference_tracks(id, source, &list)?;
                // The release a release group's list was read from (R25, N6): kept, as found, so the record knows which edition
                // its list is, and a release its files name later is told from it.
                if let Some(release) = release {
                    c.set_reference(&ids::record(id), &release, "search")?;
                }
                if count {
                    c.set_counting(id, Some(source), false)?;
                }
                // F15.1, F18.2: what the answer says besides its list, for a record not had: its master joins it, its artists credit it.
                if let Some((read, more)) = read.filter(|_| !had) {
                    let artists: Vec<(Ref, String, bool)> = more
                        .artists
                        .iter()
                        .map(|(r, name)| {
                            let clean = super::discogs::clean_name(name);
                            let marked = r.source == "discogs" && clean != name.trim();
                            (r.clone(), clean, !marked)
                        })
                        .collect();
                    c.learn_from_read(id, &read, &more.also, &artists)?;
                }
                Ok(())
            })
            .await?
        }
    };
    let sources: Vec<&'static str> = match row.counting.as_deref() {
        // Its own first; the others in the order where its own has no lengths (R33).
        Some(counting) => identity::COUNTING_ORDER.iter().copied().filter(|s| *s == counting).chain(identity::COUNTING_ORDER.iter().copied().filter(|s| *s != counting)).collect(),
        // Had, short of the list its files name: searched by that list (W4), nothing counted by it.
        None if row.had && short && kept_of("musicbrainz").is_some() => vec!["musicbrainz"],
        None if row.had => return Ok(answered(Err("No list counts until one is chosen".into()))),
        None => identity::COUNTING_ORDER.to_vec(),
    };
    let mut counted = row.counting.is_some() || row.had;
    let mut judging = Judging::default();
    let mut said: Vec<(&'static str, Outcome)> = Vec::new();
    let mut stop = None;
    let mut off = None;
    for (n, source) in sources.into_iter().enumerate() {
        // Its own list alone, the others only where it has no lengths (R33); where its own source is switched off, the next judges
        // and its count stays (Y12).
        if n > 0 && row.counting.is_some() && judging.untimed.is_none() && off.is_none() {
            break;
        }
        let list = match kept_of(source) {
            Some(list) => {
                if !counted {
                    keep(source, list.clone(), None, true, None).await?;
                    counted = true;
                }
                list
            }
            None => {
                let Some((r, _)) = main_ref(&refs, source) else { continue };
                match read(r.clone()).await {
                    Ok(Some((list, release, more))) if !list.is_empty() => {
                        keep(source, list.clone(), release, !counted, Some((r.clone(), more))).await?;
                        counted = true;
                        list
                    }
                    Ok(_) => {
                        said.push((source, Outcome::None));
                        continue;
                    }
                    Err(e) => {
                        let outcome = Outcome::of(&e);
                        said.push((source, outcome));
                        // Switched off (R23, N16): passed over for the next list; where none answers, out of reach until it is on again.
                        if outcome == Outcome::Off {
                            off = Some(e);
                            continue;
                        }
                        // Out of reach, Bandcamp's bot check among them (R23, N3): asked again later, nothing said against the record.
                        stop = Some(e);
                        break;
                    }
                }
            }
        };
        if timed(&list) {
            judging.timed = Some((source.to_string(), list));
            break;
        }
        // Without its tracks' lengths (R33, N17): no judge of a folder by itself, but the fallback where the next list with them
        // finds none; kept, and counting where nothing does, as the record page may show it.
        if judging.untimed.is_none() {
            judging.untimed = Some((source.to_string(), list));
        }
    }
    let answer = if judging.timed.is_some() || judging.untimed.is_some() {
        Ok(judging)
    } else {
        let none: Vec<String> = said.iter().filter(|(_, o)| *o == Outcome::None).map(|(s, _)| format!("{} has no list of it", source_name(s))).collect();
        Err(if none.is_empty() { "No list to judge by".to_string() } else { none.join("; ") })
    };
    Ok(Judged { answer, said, stop, off })
}

/// What a source's answer says besides its list (F14.5, F15, F18.2): other references of the album it names (`also`: a Discogs
/// release's master, a MusicBrainz release's Bandcamp page), and the release's artists by their id there, each with their name as
/// that source writes it. Test readers give none.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadMore {
    pub also: Vec<Ref>,
    pub artists: Vec<(Ref, String)>,
}

/// A list read through a reference (`tracklist`'s reader): its tracks, the release they were read from where the reference was a
/// release group's (R25: kept as a reference), and what the answer says besides (`ReadMore`).
pub type Read = (Vec<RefTrack>, Option<Ref>, ReadMore);

/// What a Discogs release's answer says besides its list (F15, F18.2): its master, where it was read as a release (`as_release`),
/// and its artists by their Discogs ids, Discogs' "Various" none.
pub fn discogs_more(v: &serde_json::Value, as_release: bool) -> ReadMore {
    let master = v["master_id"].as_u64().filter(|m| *m > 0 && as_release).map(|m| Ref::new("discogs", "master", &m.to_string()));
    let artists = v["artists"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| Some((a["id"].as_u64().filter(|id| *id > 0)?, a["name"].as_str()?.trim().to_string())))
        .filter(|(_, name)| !name.is_empty() && !name.eq_ignore_ascii_case("various"))
        .map(|(id, name)| (Ref::new("discogs", "artist", &id.to_string()), name))
        .collect();
    ReadMore { also: master.into_iter().collect(), artists }
}

/// How long a record's list its page asked for and could not have is left before a page asks for it again (C54), unless a source
/// it said was switched off is on again, or Bandcamp's hold it said was lifted (F1.6): read at once then.
const PAGE_READ_AGAIN_SECS: i64 = 15 * 60;

/// The longest a page's read may take (F1.9), as an artist's list's (`discover`'s `LIST_READ_AT_MOST`): past it, the source it waits
/// on did not answer.
const PAGE_READ_AT_MOST: std::time::Duration = std::time::Duration::from_secs(if cfg!(test) { 1 } else { 300 });

/// What a source said of a record not had's list where it gave none, as its page tells it (F1, `AlbumMore::no_list`): `off`
/// (switched off), `held` (Bandcamp held back after its bot check, `until` when), `unanswered` (out of reach, or too slow), `none`
/// (no list of it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Said {
    pub source: &'static str,
    pub said: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<i64>,
}

/// What a record not had's page has of its list where none is kept (F1): being read (`looking`), from which source now (F6:
/// `reading`), or why there is none.
#[derive(Debug, Clone, PartialEq)]
pub enum PageList {
    Looking(Option<&'static str>),
    Said(Vec<Said>),
}

impl Said {
    /// What `source` said at `now` (`Outcome`), as the pages word it: Bandcamp's hold until when. A record not had's page (F1) and an
    /// artist's sections (F8) alike.
    pub(crate) fn of(source: &'static str, outcome: Outcome, now: i64) -> Said {
        Said { source, said: outcome.word(), until: if outcome == Outcome::Held { crate::bandcamp::held_until(now) } else { None } }
    }
}

/// Where a page's read of a record's list stands (C54): being read, asking which source now (F6; before its first question, the
/// first it will ask), or ended (when) without one, with what its sources said.
enum PageReadState {
    Reading(Option<&'static str>),
    Ended(i64, Vec<(&'static str, Outcome)>),
}

/// The records whose list a page asked for (C54), by the catalogue and the record, each with its catalogue: while the entry holds
/// it, its address is no other catalogue's; one gone is let go of as a read begins.
type PageReads = std::sync::Mutex<HashMap<(usize, i64), (std::sync::Weak<Catalogue>, PageReadState)>>;

fn page_reads() -> std::sync::MutexGuard<'static, HashMap<(usize, i64), (std::sync::Weak<Catalogue>, PageReadState)>> {
    static READS: std::sync::OnceLock<PageReads> = std::sync::OnceLock::new();
    READS.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner())
}

/// Ends a page's read, whatever becomes of its task: what its sources said and when, where it came to no list; else (a list kept, an
/// error, a panic, a task dropped) let go of, so the next open shows the rows or reads again.
struct PageRead((usize, i64), Option<(i64, Vec<(&'static str, Outcome)>)>);

impl Drop for PageRead {
    fn drop(&mut self) {
        let mut reads = page_reads();
        match self.1.take() {
            Some((at, said)) => {
                if let Some(entry) = reads.get_mut(&self.0) {
                    entry.1 = PageReadState::Ended(at, said);
                }
            }
            None => {
                reads.remove(&self.0);
            }
        }
    }
}

/// A source switched off (R23, X3, Y12): asked nothing.
fn switched_off(source: &str) -> Option<SwitchedOff> {
    use super::switches::{BANDCAMP, DISCOGS, MUSICBRAINZ, is_on};
    match source {
        "musicbrainz" if !is_on(MUSICBRAINZ) => Some(SwitchedOff("MusicBrainz")),
        "discogs" if !is_on(DISCOGS) => Some(SwitchedOff("Discogs")),
        "bandcamp" if !is_on(BANDCAMP) => Some(SwitchedOff("Bandcamp")),
        _ => None,
    }
}

/// Why a page's read asks a source nothing at `now` (F1.1, F1.5): switched off, or Bandcamp held back after its bot check.
fn refused(source: &str, now: i64) -> Option<anyhow::Error> {
    if let Some(off) = switched_off(source) {
        return Some(off.into());
    }
    (source == "bandcamp" && crate::bandcamp::held_until(now).is_some()).then(|| crate::bandcamp::NotRead.into())
}

/// Whether what a page's read said still stands at `now` (F1.6): each source it said was switched off still is, and Bandcamp still
/// held back where it said so.
pub(crate) fn still_said(said: &[(&'static str, Outcome)], now: i64) -> bool {
    said.iter().all(|(source, o)| match o {
        Outcome::Off => switched_off(source).is_some(),
        Outcome::Held => crate::bandcamp::held_until(now).is_some(),
        Outcome::Unanswered | Outcome::None => true,
    })
}

/// What the page says of its sources at `now` (`Said`), in the design's order (`identity::COUNTING_ORDER`): Bandcamp's hold until when.
fn page_words(said: &[(&'static str, Outcome)], now: i64) -> Vec<Said> {
    let at = |s: &str| identity::COUNTING_ORDER.iter().position(|o| *o == s).unwrap_or(usize::MAX);
    let mut words: Vec<Said> =
        said.iter().map(|&(source, o)| Said::of(source, o, now)).collect();
    words.sort_by_key(|w| at(w.source));
    words
}

/// What a page's read of `key` has to say at `now`: being read, or what its sources said within `PAGE_READ_AGAIN_SECS`, where that
/// still stands.
fn page_read_known(key: (usize, i64), cat: &Arc<Catalogue>, now: i64) -> Option<PageList> {
    match page_reads().get(&key).filter(|(c, _)| c.upgrade().is_some_and(|c| Arc::ptr_eq(&c, cat)))? {
        (_, PageReadState::Reading(source)) => Some(PageList::Looking(*source)),
        (_, PageReadState::Ended(at, said)) => ((0..PAGE_READ_AGAIN_SECS).contains(&(now - at)) && still_said(said, now)).then(|| PageList::Said(page_words(said, now))),
    }
}

/// A page's read of `key` begun where none is being read, `first` the source it asks first, the catalogues gone let go of first.
/// Whether it begins.
fn page_read_begins(key: (usize, i64), cat: &Arc<Catalogue>, first: Option<&'static str>) -> bool {
    let mut reads = page_reads();
    reads.retain(|_, (c, _)| c.strong_count() > 0);
    if matches!(reads.get(&key), Some((c, PageReadState::Reading(_))) if c.upgrade().is_some_and(|c| Arc::ptr_eq(&c, cat))) {
        return false;
    }
    reads.insert(key, (Arc::downgrade(cat), PageReadState::Reading(first)));
    true
}

/// A page's read of `key` asking `source` now (F6: the page's loader names it).
fn page_read_asks(key: (usize, i64), source: &str) {
    let source = identity::COUNTING_ORDER.iter().copied().find(|s| *s == source);
    if let Some((_, PageReadState::Reading(asking))) = page_reads().get_mut(&key) {
        *asking = source;
    }
}

/// What an ended read of `key` said let go of: its sources say so unasked now.
fn page_read_unasked(key: (usize, i64)) {
    let mut reads = page_reads();
    if matches!(reads.get(&key), Some((_, PageReadState::Ended(..)))) {
        reads.remove(&key);
    }
}

/// A source a page's read would ask (`said_unasked`'s reader).
#[derive(Debug)]
struct ToAsk;

impl std::fmt::Display for ToAsk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("to be asked")
    }
}

impl std::error::Error for ToAsk {}

/// What a record's sources say at `now` without one being asked (F1.1, F1.5): where every source a read would ask is switched off or
/// held back, what each says (`Err`), in `tracklist`'s way; else the source a read asks first (F6; none where a list was kept
/// meanwhile).
async fn said_unasked(cat: &Arc<Catalogue>, record: i64, now: i64) -> Result<std::result::Result<Option<&'static str>, Vec<(&'static str, Outcome)>>> {
    let j = judge(cat, record, |r: Ref| std::future::ready(Err::<Option<Read>, _>(refused(&r.source, now).unwrap_or_else(|| anyhow::Error::from(ToAsk))))).await?;
    if j.answer.is_ok() {
        return Ok(Ok(None));
    }
    if j.stop.is_some_and(|e| e.is::<ToAsk>()) {
        return Ok(Ok(j.said.last().map(|(source, _)| *source)));
    }
    Ok(Err(j.said))
}

/// A record not had's list asked for by its page at `now` where none is kept (C54, F1), its sources read through `read`
/// (`read_reference` for the server's page): in the background, as `tracklist` reads and keeps (`judge`), so the page lists its rows
/// when asked again; a page's, so read while background reading is paused (Y7). One read a record at a time, cut at
/// `PAGE_READ_AT_MOST`. Where every source it would read is switched off or Bandcamp held back, said at once, nothing read; a read that
/// came to no list is said, and read again after `PAGE_READ_AGAIN_SECS`, or at once where a source it said was off is on again or
/// Bandcamp's hold was lifted. `Looking` while it is read (the page's `looking`), with the source it asks now (`reading`, F6), else
/// why it has no list (`no_list`).
pub async fn read_soon_with<R, F>(cat: &Arc<Catalogue>, record: i64, now: i64, read: R) -> PageList
where
    R: Fn(Ref) -> F + Send + Sync + 'static,
    F: std::future::Future<Output = Result<Option<Read>>> + Send + 'static,
{
    let key = (Arc::as_ptr(cat) as usize, record);
    if let Some(known) = page_read_known(key, cat, now) {
        return known;
    }
    let first = match said_unasked(cat, record, now).await {
        Ok(Err(said)) => {
            page_read_unasked(key);
            return PageList::Said(page_words(&said, now));
        }
        Ok(Ok(first)) => first,
        Err(e) => {
            tracing::debug!(target: "library", record, "a page's list: {e:#}");
            None
        }
    };
    if !page_read_begins(key, cat, first) {
        return page_read_known(key, cat, now).unwrap_or(PageList::Looking(first));
    }
    let (cat, read) = (cat.clone(), Arc::new(read));
    let (deadline, started) = (tokio::time::Instant::now() + PAGE_READ_AT_MOST, std::time::Instant::now());
    // Each source asked as the page may: nothing of one switched off or held back, none waited on past the read's end.
    let asking = move |r: Ref| {
        let read = read.clone();
        async move {
            if let Some(e) = refused(&r.source, now) {
                return Err(e);
            }
            page_read_asks(key, &r.source);
            match tokio::time::timeout_at(deadline, (*read)(r)).await {
                Ok(answer) => answer,
                Err(_) => Err(anyhow::anyhow!("No answer within {} s", PAGE_READ_AT_MOST.as_secs())),
            }
        }
    };
    tokio::spawn(async move {
        let mut ends = PageRead(key, None);
        match judge(&cat, record, asking).await {
            Ok(j) => {
                if let Err(why) = &j.answer {
                    tracing::debug!(target: "library", record, "a page's list not read: {why}");
                    ends.1 = Some((now + i64::try_from(started.elapsed().as_secs()).unwrap_or(0), j.said));
                }
            }
            Err(e) => tracing::debug!(target: "library", record, "a page's list not read: {e:#}"),
        }
    });
    PageList::Looking(first)
}

/// Tests: where a page's read of `record` stands: being read (true), ended without a list (false), or none kept.
#[cfg(test)]
pub(crate) fn page_read_of(cat: &Arc<Catalogue>, record: i64) -> Option<bool> {
    page_reads().get(&(Arc::as_ptr(cat) as usize, record)).map(|(_, s)| matches!(s, PageReadState::Reading(_)))
}

/// A source switched off: no answer about any record, and none against it (R23).
#[derive(Debug)]
pub struct SwitchedOff(pub &'static str);

impl std::fmt::Display for SwitchedOff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is switched off", self.0)
    }
}

impl std::error::Error for SwitchedOff {}

/// A reference's list, read from its source (C51a, `tracklist`'s reader): a MusicBrainz release's own; a release group's, one
/// of its releases (the first official one with tracks, else the first with tracks, as 0.16's Soulseek chose); a Discogs
/// release's, a master's main release's; a Bandcamp album's. None where the source has no such thing (an answer "not found");
/// Bandcamp's bot check is its `NotRead`. Every MusicBrainz question in its turn (S10).
pub async fn read_reference(r: Ref) -> Result<Option<Read>> {
    let not_found = |e: &anyhow::Error| e.downcast_ref::<reqwest::Error>().and_then(reqwest::Error::status) == Some(reqwest::StatusCode::NOT_FOUND);
    let release = |id: String| async move {
        match super::track_fetch::release_by_id(&id).await {
            Ok(v) => Ok(Some(list_of_release(&v))),
            Err(e) if not_found(&e) => Ok(None),
            Err(e) => Err(e),
        }
    };
    // Discogs switched off is passed over as MusicBrainz is (X3): out of reach until it is on, nothing said against the record.
    // Bandcamp too (Y12): its plain "switched off" ended the read, so no other list was tried.
    if let Some(off) = switched_off(&r.source) {
        return Err(off.into());
    }
    match (r.source.as_str(), r.kind.as_str()) {
        ("musicbrainz", "release") => Ok(release(r.value).await?.map(|list| (list, None, ReadMore::default()))),
        ("musicbrainz", "release-group") => match release_of_group(&r.value).await? {
            Some(id) => Ok(release(id.clone()).await?.map(|list| (list, Some(Ref::new("musicbrainz", "release", &id)), ReadMore::default()))),
            None => Ok(None),
        },
        ("discogs", "release") => Ok(super::discogs::release_raw(r.value.parse()?).await?.map(|v| (list_of_discogs(&v), None, discogs_more(&v, true)))),
        ("discogs", "master") => match super::discogs::master(r.value.parse()?).await?.and_then(|m| m.main_release) {
            Some(main) => Ok(super::discogs::release_raw(main).await?.map(|v| (list_of_discogs(&v), None, discogs_more(&v, false)))),
            None => Ok(None),
        },
        ("bandcamp", "album") => match crate::bandcamp::Bandcamp::get().album(&r.value).await {
            Ok(a) => Ok(Some((list_of_bandcamp(&a), None, ReadMore::default()))),
            Err(e) if crate::bandcamp::not_read(&e) => Err(e),
            // Bandcamp has no such page (`Bandcamp::text` says its status in words).
            Err(e) if e.to_string().starts_with("Bandcamp answered 404") => Ok(None),
            Err(e) => Err(e),
        },
        _ => Ok(None),
    }
}

/// The release a release group is judged by: its first official one with tracks, else its first with tracks (0.16's
/// Soulseek's `reference`). None where MusicBrainz lists none with tracks.
async fn release_of_group(group: &str) -> Result<Option<String>> {
    use super::musicbrainz::Asked as _;
    #[derive(Deserialize)]
    struct Browse {
        #[serde(default)]
        releases: Vec<serde_json::Value>,
    }
    let http = super::discover::http()?;
    let url = format!("https://musicbrainz.org/ws/2/release?release-group={group}&inc=media&fmt=json&limit=25");
    let b: Browse = http.get(url).asked().await?.error_for_status()?.json().await?;
    let tracks = |v: &serde_json::Value| v["media"].as_array().map_or(0, |m| m.iter().map(|x| x["track-count"].as_u64().unwrap_or(0)).sum::<u64>());
    let one = b.releases.iter().filter(|v| tracks(v) > 0).find(|v| v["status"].as_str() == Some("Official")).or_else(|| b.releases.iter().find(|v| tracks(v) > 0));
    Ok(one.and_then(|v| v["id"].as_str()).map(str::to_string))
}

/// The record's tracks as a list is held against them: the copy that plays, in its order, each by its title and length.
/// Blocking.
fn files_of(served: &crate::serve::Serve, album: &str) -> Result<Vec<Listed>> {
    Ok(served.library.album_tracks(album)?.into_iter().map(|(_, _, _, title, ms, _, _)| Listed::of(&title, ms as i64)).collect())
}

/// The source that counts for a record (`identity::counting`), from the lists kept and its files, and kept, unless chosen by
/// hand. No list, no count: a certain match from 0.16 counts once its list is read and agrees (`read_lists`). Its counting
/// now. Blocking.
pub fn work_out_in(cat: &Catalogue, record: i64, files: &[Listed]) -> Result<Option<String>> {
    let Some(row) = cat.record(record)? else { return Ok(None) };
    if row.counted_by_hand {
        return Ok(row.counting);
    }
    // No files to hold its lists against: a had record (not served now) counts nothing new; one not had counts its lists in the
    // design's order (R7, R19; C51a's rule). The one rule for a list read, a match kept and Discogs' facts.
    if files.is_empty() && row.had {
        return Ok(row.counting);
    }
    let kept = cat.reference_tracks(record)?;
    let lists: Vec<(&str, &[RefTrack])> = kept.iter().map(|(s, l)| (s.as_str(), l.as_slice())).filter(|(_, l)| !l.is_empty()).collect();
    let counting = identity::counting(None, &lists, files);
    if counting != row.counting {
        cat.set_counting(record, counting.as_deref(), false)?;
    }
    Ok(counting)
}

/// The lists of records waiting since 0.17's step (`Catalogue::lists_to_read`, 0.16's certain matches), up to `n`, the oldest
/// first: each release's list read (`read`; none, MusicBrainz has no such release), kept where the record still holds the
/// release, and held against its files (`files`) by `work_out_in`, so MusicBrainz counts only where the list agrees
/// (`identity::agrees`). Either way the record waits no more: on Health's references item where nothing speaks for it. A
/// question that fails stops the pass there; the rest wait for the next. How many were read.
pub async fn read_lists<R, F>(cat: &Arc<Catalogue>, files: Arc<dyn Fn(i64) -> Result<Vec<Listed>> + Send + Sync>, read: R, n: usize) -> Result<usize>
where
    R: Fn(String) -> F,
    F: std::future::Future<Output = Result<Option<Vec<RefTrack>>>>,
{
    let waiting = {
        let cat = cat.clone();
        tokio::task::spawn_blocking(move || cat.lists_to_read(n)).await??
    };
    let mut read_n = 0;
    for (record, release) in waiting {
        let list = match read(release.clone()).await {
            Ok(list) => list,
            // MusicBrainz out of reach: the rest wait for the next pass, nothing counted against them.
            Err(e) if out_of_reach(&e) => {
                tracing::info!(target: "library", "the lists 0.17's step left: stopped after {read_n}: {e:#}");
                break;
            }
            // This release's question failed: counted against it, and the rest go on (R7).
            Err(e) => {
                let cat = cat.clone();
                if tokio::task::spawn_blocking(move || cat.list_failed(record, LIST_TRIES)).await?? {
                    tracing::info!(target: "library", "the lists 0.17's step left: record {record} let go of after {LIST_TRIES} tries: {e:#}");
                }
                continue;
            }
        };
        read_n += 1;
        let (cat, files) = (cat.clone(), files.clone());
        tokio::task::spawn_blocking(move || -> Result<()> {
            if let Some(list) = list.filter(|l| !l.is_empty())
                && cat.subject_of(&Ref::new("musicbrainz", "release", &release))? == [ids::record(record)]
            {
                cat.set_reference_tracks(record, "musicbrainz", &list)?;
                // Held against its files as a match kept and Discogs' facts are (`work_out_in`: with none, a had record counts nothing).
                work_out_in(&cat, record, &files(record)?)?;
            }
            cat.list_read(record)
        })
        .await??;
    }
    Ok(read_n)
}

/// How many failed questions a record's list is asked again after, before the record is let go of to Health's item.
const LIST_TRIES: i64 = 3;

/// Whether an error is MusicBrainz out of reach, not an answer about one release: no answer at all, busy or down (a 5xx), any
/// other refusal than "no such release" (its rate limit's 429, a 401 or 403 from it or a proxy: R15, R23), or switched off.
fn out_of_reach(e: &anyhow::Error) -> bool {
    let busy = |s: reqwest::StatusCode| s.is_server_error() || (s.is_client_error() && !no_such_status(s));
    e.downcast_ref::<reqwest::Error>().is_some_and(|r| r.is_connect() || r.is_timeout() || r.status().is_some_and(busy)) || !super::switches::is_on(super::switches::MUSICBRAINZ)
}

/// Whether MusicBrainz answered that it will not give this release: 404, no such release, or 400, an id it refuses (R23, N13).
fn no_such(e: &anyhow::Error) -> bool {
    e.downcast_ref::<reqwest::Error>().and_then(reqwest::Error::status).is_some_and(no_such_status)
}

fn no_such_status(s: reqwest::StatusCode) -> bool {
    s == reqwest::StatusCode::NOT_FOUND || s == reqwest::StatusCode::BAD_REQUEST
}

/// How many lists `lists_pass` reads in one pass.
const LISTS: usize = 200;

/// The background asker's pass over the lists 0.17's step left to read (`read_lists`), each a question to MusicBrainz on its
/// turn, behind a page's (`musicbrainz::behind`, which the asker sets), every other turn at most. How many were read.
pub async fn lists_pass() -> Result<usize> {
    let Some(served) = crate::serve::instance() else { return Ok(0) };
    let cat = served.catalogue().clone();
    let files: Arc<dyn Fn(i64) -> Result<Vec<Listed>> + Send + Sync> = Arc::new(move |record| files_of(&served, &ids::record(record)));
    let read = |release: String| async move {
        let read = match super::track_fetch::release_by_id(&release).await {
            Ok(v) => Ok(Some(list_of_release(&v))),
            // MusicBrainz will not answer for it (no such release, an id it refuses): no list, let go of at once (R7).
            Err(e) if no_such(&e) => Ok(None),
            Err(e) => Err(e),
        };
        tokio::time::sleep(std::time::Duration::from_secs(super::musicbrainz::PACE_SECS)).await;
        read
    };
    read_lists(&cat, files, read, LISTS).await
}

/// `work_out_in` for an album of Cratarr's own library. Blocking.
pub fn work_out(served: &crate::serve::Serve, album: &str) -> Result<Option<String>> {
    let Some(record) = ids::record_number(album) else { return Ok(None) };
    work_out_in(served.catalogue(), record, &files_of(served, album)?)
}

/// A match matching found (`album_matches` keeps it as before), kept as references too: its release and release group, how
/// the files say (their own release: `files`, else `search`), and the release's own list; the count worked out with it. A
/// certain match takes its release from another record that only held it by its files or a guess: MusicBrainz matched this
/// one's tracks to it (one release's files on two records, G1). Its list is kept only where its release is this record's.
/// Blocking.
pub fn keep_match(served: &crate::serve::Serve, album: &str, m: &super::matching::Match, list: &[RefTrack], named: bool) -> Result<()> {
    let Some(record) = ids::record_number(album) else { return Ok(()) };
    keep_match_in(served.catalogue(), record, album, m, list, named, &files_of(served, album)?)
}

/// `keep_match`, `files` the record's.
fn keep_match_in(cat: &Catalogue, record: i64, album: &str, m: &super::matching::Match, list: &[RefTrack], named: bool, files: &[Listed]) -> Result<()> {
    let how = if named { "files" } else { "search" };
    let release = Ref::new("musicbrainz", "release", &m.release);
    let held = if m.certain { cat.take_reference(album, &release, how)? } else { cat.set_reference(album, &release, how)? };
    if let Some(group) = &m.release_group {
        cat.set_reference(album, &Ref::new("musicbrainz", "release-group", group), how)?;
    }
    if held && !list.is_empty() {
        cat.set_reference_tracks(record, "musicbrainz", list)?;
        work_out_in(cat, record, files)?;
    }
    Ok(())
}

/// What Discogs said of a record (`discogs_facts` keeps it as before), kept as references too: its master and main release,
/// found by the record's name or MusicBrainz's link (the facts do not say which: `search`, which a link at 0.17's step makes
/// surer), and the main release's list; the count worked out with it. Blocking.
pub fn keep_discogs(served: &crate::serve::Serve, album: &str, f: &super::discogs::AlbumFacts) -> Result<()> {
    let Some(record) = ids::record_number(album) else { return Ok(()) };
    keep_discogs_in(served.catalogue(), record, album, f, &files_of(served, album)?)
}

/// `keep_discogs`, `files` the record's.
fn keep_discogs_in(cat: &Catalogue, record: i64, album: &str, f: &super::discogs::AlbumFacts, files: &[Listed]) -> Result<()> {
    if let Some(master) = f.master {
        cat.set_reference(album, &Ref::new("discogs", "master", &master.to_string()), "search")?;
    }
    let held = match f.release {
        Some(release) => cat.set_reference(album, &Ref::new("discogs", "release", &release.to_string()), "search")?,
        None => false,
    };
    if held && !f.tracks.is_empty() {
        cat.set_reference_tracks(record, "discogs", &f.tracks)?;
        work_out_in(cat, record, files)?;
    }
    Ok(())
}

/// A source's list held against the record's files, as `identity::agrees` decides: the one measure, so a row never says it
/// agrees where the count says it does not. Its state, and what it says.
fn held(list: &[RefTrack], files: &[Listed]) -> (&'static str, String) {
    if identity::agrees(list, files) {
        return ("agrees", format!("All {} tracks, each within 2 s", files.len()));
    }
    if list.len() != files.len() {
        return ("differs", format!("{} tracks; the record has {}", list.len(), files.len()));
    }
    let off = list.iter().zip(files).filter(|(t, f)| !Listed::of(&t.title, t.length_ms.unwrap_or(f.duration_ms)).same_track(f)).count();
    ("differs", format!("{off} of {} tracks differ", files.len()))
}

/// The one reference of `source` that speaks for the record: a link pasted by hand, else its release (or album page) before
/// its release group or master, else the surest found.
fn main_ref<'k>(refs: &'k [(Ref, ReferenceRow)], source: &str) -> Option<&'k (Ref, ReferenceRow)> {
    let kind = |r: &Ref| match r.kind.as_str() {
        "release" | "album" => 0,
        "release-group" | "master" => 1,
        _ => 2,
    };
    let how = |h: &str| match h {
        "hand" => 0,
        "files" => 1,
        "link" => 2,
        _ => 3,
    };
    refs.iter().filter(|(r, _)| r.source == source).min_by_key(|(r, row)| (row.how != "hand", kind(r), how(&row.how)))
}

/// What is kept of a record, as its references are told.
struct Kept {
    title: String,
    artist: String,
    refs: Vec<(Ref, ReferenceRow)>,
    lists: Vec<(String, Vec<RefTrack>)>,
    row: RecordRow,
    /// Matching's find (`album_matches`): the year it read and why it matched.
    matched: Option<super::matching::Match>,
    /// What Discogs said (`discogs_facts`): its year.
    discogs: Option<super::discogs::AlbumFacts>,
    heard: Option<super::intake_card::Heard>,
    types: HashMap<String, (Option<String>, Vec<String>)>,
    files: Vec<Listed>,
}

/// A record not had (D18, C54) is no album of the library: its names are the catalogue's. Blocking.
fn kept(served: &crate::serve::Serve, album: &str, record: i64) -> Result<Kept> {
    let cat = served.catalogue();
    let row = cat.record(record)?.context("No such album in Cratarr's own library")?;
    let (title, artist) = match served.library.album(album)? {
        Some(rec) => (rec.name, rec.artist),
        None if !row.had => (row.title.clone(), row.artist.clone()),
        None => bail!("No such album in Cratarr's own library"),
    };
    let matched = served.userdata.album_matches(&[album.to_string()])?.remove(album).and_then(|(_, j)| serde_json::from_str(&j?).ok());
    Ok(Kept {
        title,
        artist,
        refs: cat.references_of(album)?,
        lists: cat.reference_tracks(record)?,
        row,
        matched,
        discogs: super::discogs_facts::album_kept(served, album).ok().flatten(),
        heard: served.userdata.intake_heard(album, None)?.and_then(|(_, j)| serde_json::from_str(&j).ok()),
        types: served.userdata.release_types()?,
        files: files_of(served, album)?,
    })
}

/// One source's row, from its reference and its list.
fn source_of(source: &'static str, k: &Kept, found: &str, missing: &str) -> Source {
    let main = main_ref(&k.refs, source);
    let list = k.lists.iter().find(|(s, _)| s == source).map(|(_, l)| l.as_slice()).filter(|l| !l.is_empty());
    let (state, said) = match (list, main) {
        // A record not had (D18): no files to hold its list against.
        (Some(l), _) if !k.row.had => ("found", format!("{} tracks", l.len())),
        (Some(l), _) => held(l, &k.files),
        (None, Some((_, row))) if row.how == "hand" => ("agrees", "Pasted by hand".to_string()),
        (None, Some(_)) => ("found", found.to_string()),
        (None, None) => ("missing", missing.to_string()),
    };
    Source {
        source,
        state,
        tracks: list.map(<[RefTrack]>::len),
        year: None,
        kind: None,
        url: main.and_then(|(r, _)| url_of(r)),
        lengths: list.map(|l| l.iter().map(|t| t.length_ms.unwrap_or(0)).collect()),
        guessed: main.is_some_and(|(_, row)| row.how == "search"),
        confirmed: main.is_some_and(|(_, row)| row.confirmed),
        said,
    }
}

/// Every source's row, `bandcamp_year` as its page said it when read now.
fn sources(k: &Kept, bandcamp_year: Option<u32>) -> Vec<Source> {
    let mut mb = source_of("musicbrainz", k, "Found by its name", "No release matched yet");
    if let Some((r, row)) = main_ref(&k.refs, "musicbrainz") {
        let matched = k.matched.as_ref().filter(|m| m.release == r.value || m.release_group.as_deref() == Some(r.value.as_str()));
        let release = matched.map(|m| m.release.as_str()).or((r.kind == "release").then_some(r.value.as_str()));
        mb.year = matched.and_then(|m| m.year);
        mb.kind = release.and_then(|rel| k.types.get(&format!("mb:{rel}"))).and_then(|t| t.0.clone());
        if mb.state == "found" {
            match (row.how.as_str(), matched) {
                ("files", _) => mb.said = "The files name this release".into(),
                (_, Some(m)) => mb.said = m.why.clone(),
                _ => {}
            }
        }
    }
    let mut discogs = source_of("discogs", k, "Discogs knows it", "Discogs has not said");
    discogs.year = main_ref(&k.refs, "discogs").and(k.discogs.as_ref()).and_then(|d| d.year);
    let mut bandcamp = source_of("bandcamp", k, "Its page on Bandcamp", "No page found on Bandcamp");
    bandcamp.year = bandcamp_year;
    // AcoustID: its tracks as heard (the intake's hearing), no reference of its own.
    let acoustid = match k.heard.as_ref().filter(|h| !h.no_acoustid) {
        Some(h) => {
            let known = h.tracks.iter().filter(|t| !t.recordings.is_empty()).count();
            Source {
                source: "acoustid",
                state: if known == h.tracks.len() { "agrees" } else { "differs" },
                tracks: Some(known),
                year: None,
                kind: None,
                url: None,
                lengths: None,
                guessed: false,
                confirmed: false,
                said: format!("AcoustID knows {known} of {} tracks", h.tracks.len()),
            }
        }
        None => Source { source: "acoustid", state: "missing", tracks: None, year: None, kind: None, url: None, lengths: None, guessed: false, confirmed: false, said: "Not heard yet".into() },
    };
    vec![mb, discogs, bandcamp, acoustid]
}

/// A record's references as kept, a list not kept yet read now (Bandcamp's page looked up when the record is opened).
pub async fn of(app: &Arc<super::App>, album: &str) -> Result<References> {
    let served = app.served.clone().context("Cratarr's own library is not open")?;
    let record = ids::record_number(album).context("No such album in Cratarr's own library")?;
    let (s, a) = (served.clone(), album.to_string());
    let k = tokio::task::spawn_blocking(move || kept(&s, &a, record)).await??;
    let mut read: Vec<(String, Vec<RefTrack>)> = Vec::new();
    for source in ["musicbrainz", "discogs"] {
        if k.lists.iter().any(|(s, l)| s == source && !l.is_empty()) {
            continue;
        }
        let Some((r, _)) = main_ref(&k.refs, source) else { continue };
        match read_list(r).await {
            Ok(Some(list)) if !list.is_empty() => read.push((source.to_string(), list)),
            Ok(_) => {}
            Err(e) => tracing::debug!("references: {source} of {album} not read: {e:#}"),
        }
    }
    // Bandcamp: its page as kept, else looked up now (kept a month, a miss too), and read, as when it was first built.
    let kept_page = main_ref(&k.refs, "bandcamp").map(|(r, _)| r.value.clone());
    let page = match &kept_page {
        Some(p) => Some(p.clone()),
        None => {
            let group = k.refs.iter().find(|(r, _)| r.source == "musicbrainz" && r.kind == "release-group").map(|(r, _)| r.value.clone()).or_else(|| k.matched.as_ref().and_then(|m| m.release_group.clone()));
            super::discover::find_on_bandcamp(&served, &k.artist, &k.title, group.as_deref()).await.ok().flatten().map(|p| crate::bandcamp::album_page(&p).unwrap_or(p))
        }
    };
    let page_read = match &page {
        Some(p) => crate::bandcamp::Bandcamp::get().album(p).await.ok(),
        None => None,
    };
    let year = page_read.as_ref().and_then(|a| a.released.as_deref()).and_then(|d| d.get(..4)?.parse().ok());
    if let Some(a) = &page_read {
        let list = list_of_bandcamp(a);
        if !k.lists.iter().any(|(s, l)| s == "bandcamp" && *l == list) {
            read.push(("bandcamp".into(), list));
        }
    }
    let found = page.filter(|_| kept_page.is_none());
    let (s, a) = (served.clone(), album.to_string());
    let k = tokio::task::spawn_blocking(move || -> Result<Kept> {
        let cat = s.catalogue();
        if let Some(p) = &found {
            cat.set_reference(&a, &Ref::new("bandcamp", "album", p), "search")?;
        }
        for (source, list) in &read {
            cat.set_reference_tracks(record, source, list)?;
        }
        if !read.is_empty() {
            work_out(&s, &a)?;
        }
        kept(&s, &a, record)
    })
    .await??;
    Ok(References { album: album.to_string(), title: k.title.clone(), artist: k.artist.clone(), sources: sources(&k, year), counting: k.row.counting.clone(), by_hand: k.row.checked_by_hand })
}

/// What the administrator chose of a record's references: the source that counts ("Use this"; none: none chosen), a
/// link pasted (its source then counts), or checked by hand.
#[derive(Debug, Default, Deserialize)]
pub struct Choose {
    #[serde(default)]
    pub counting: Option<Option<String>>,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub by_hand: Option<bool>,
    /// A source found by guessing, confirmed once.
    #[serde(default)]
    pub confirm: Option<String>,
}

/// What `choose` writes, in the catalogue, `files` the record's: a source chosen counts by hand, and taken back the count is
/// worked out again; a link pasted is a reference by hand, and its source counts. Whether the record became checked by hand
/// now. Blocking.
fn chose(cat: &Catalogue, record: i64, album: &str, c: &Choose, files: &[Listed]) -> Result<bool> {
    if let Some(counting) = &c.counting {
        if let Some(src) = counting
            && !SOURCES.contains(&src.as_str())
        {
            bail!("No such source: {src}");
        }
        cat.set_counting(record, counting.as_deref(), true)?;
        if counting.is_none() {
            work_out_in(cat, record, files)?;
        }
    }
    if let Some(link) = c.link.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        let r = ref_of_link(link).context("Paste a release's page on MusicBrainz or Discogs, or an album's on Bandcamp")?;
        cat.set_reference(album, &r, "hand")?;
        cat.set_counting(record, Some(&r.source), true)?;
        // The list its source kept was another reference's (D18): read by the link from now on, so Search now judges by it.
        cat.forget_reference_tracks(record, &r.source)?;
    }
    if let Some(src) = &c.confirm {
        if !SOURCES.contains(&src.as_str()) {
            bail!("No such source: {src}");
        }
        cat.confirm_reference(album, src)?;
    }
    let was = cat.record(record)?.is_some_and(|r| r.checked_by_hand);
    if let Some(b) = c.by_hand {
        cat.set_checked_by_hand(record, b)?;
    }
    Ok(c.by_hand == Some(true) && !was)
}

pub async fn choose(app: &Arc<super::App>, album: &str, c: Choose) -> Result<References> {
    let served = app.served.clone().context("Cratarr's own library is not open")?;
    let record = ids::record_number(album).context("No such album in Cratarr's own library")?;
    let (s, a) = (served.clone(), album.to_string());
    tokio::task::spawn_blocking(move || choose_in(&s, &a, record, &c)).await??;
    app.collection.refresh().await;
    app.collection.tell_changed();
    of(app, album).await
}

/// `choose` in the catalogue and the user data. A record not had (D18, C54) is no album of the library: its choice is the
/// catalogue's alone, with no files to write its facts to. Blocking.
fn choose_in(s: &crate::serve::Serve, a: &str, record: i64, c: &Choose) -> Result<()> {
    let rec = s.library.album(a)?;
    if rec.is_none() && !s.catalogue().record(record)?.is_some_and(|r| !r.had) {
        bail!("No such album in Cratarr's own library");
    }
    let checked = chose(s.catalogue(), record, a, c, &files_of(s, a)?)?;
    // Checked by hand: the record is its own reference for its type and year from now on, as it shows them (set as
    // by hand, so no source's guess changes them after).
    if checked && let Some(rec) = rec {
        let mut facts: Vec<(String, Option<serde_json::Value>)> = Vec::new();
        if let Some(kind) = rec.release.kind.clone() {
            facts.push(("type".into(), Some(serde_json::Value::String(kind))));
        }
        if let Some(y) = rec.year.filter(|y| *y >= 1000) {
            facts.push(("year".into(), Some(serde_json::json!(y))));
        }
        if !facts.is_empty() {
            s.userdata.set_overrides(&[a.to_string()], &facts, super::moment::now_secs())?;
            s.refresh()?;
        }
    }
    Ok(())
}

/// The records no reference speaks for (Health's References item): the catalogue's one query (`Catalogue::unreferenced`),
/// of those the library lists, each as it is named there. Blocking.
pub fn unreferenced(served: &crate::serve::Serve) -> Result<Vec<(String, String, String)>> {
    let listed: std::collections::HashSet<String> = served.library.album_ids()?.into_iter().collect();
    let mut out = Vec::new();
    for record in served.catalogue().unreferenced(super::switches::is_on(super::switches::MUSICBRAINZ))? {
        let album = ids::record(record);
        if !listed.contains(&album) {
            continue;
        }
        let Some((_, artist, name)) = served.library.album_names(&album)? else { continue };
        out.push((album, name, artist));
    }
    Ok(out)
}

/// Tests: what a page's read looks at that the whole process shares (Y12): every source and background reading switched on, Bandcamp
/// not held back, under the one test lock (`switches::TEST_STATE`); put back as they were when it ends.
#[cfg(test)]
pub(crate) struct TestState {
    was: Vec<(&'static str, bool)>,
    held: i64,
    _one: tokio::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl TestState {
    pub(crate) async fn set() -> TestState {
        use super::switches::{BACKGROUND, BANDCAMP, DISCOGS, MUSICBRAINZ, TEST_STATE, set_for_tests};
        let one = TEST_STATE.lock().await;
        let was = [MUSICBRAINZ, DISCOGS, BANDCAMP, BACKGROUND].into_iter().map(|name| (name, set_for_tests(name, true))).collect();
        TestState { was, held: crate::bandcamp::hold_for_tests(0), _one: one }
    }
}

#[cfg(test)]
impl Drop for TestState {
    fn drop(&mut self) {
        for &(name, on) in &self.was {
            super::switches::set_for_tests(name, on);
        }
        crate::bandcamp::hold_for_tests(self.held);
    }
}

/// Tests: waits for a page's read of `record` to end (ten seconds at most).
#[cfg(test)]
pub(crate) async fn page_read_settled(cat: &Arc<Catalogue>, record: i64) {
    for _ in 0..1_000 {
        if page_read_of(cat, record) != Some(true) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("a page's read of record {record} never ended");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_link_names_its_source_kind_and_value() {
        let r = |url: &str| ref_of_link(url).map(|r| (r.source, r.kind, r.value));
        let named = |s: &str, k: &str, v: &str| Some((s.to_string(), k.to_string(), v.to_string()));
        let mbid = "0b6f4bd4-2f0c-4b7e-9a51-6c1c0d8f7a11";
        assert_eq!(r(&format!("https://musicbrainz.org/release/{mbid}")), named("musicbrainz", "release", mbid));
        assert_eq!(r(&format!("https://beta.musicbrainz.org/release-group/{}/", mbid.to_uppercase())), named("musicbrainz", "release-group", mbid));
        assert_eq!(r("https://musicbrainz.org/release/not-an-id"), None);
        assert_eq!(r("https://www.discogs.com/release/123-Air-Moon-Safari"), named("discogs", "release", "123"));
        assert_eq!(r("https://www.discogs.com/de/master/456"), named("discogs", "master", "456"));
        assert_eq!(r("https://www.discogs.com/Air-Moon-Safari/master/0456"), named("discogs", "master", "456"));
        assert_eq!(r("https://www.discogs.com/master/view/456"), named("discogs", "master", "456"), "Discogs' older link, kept by 0.16");
        assert_eq!(r("  https://air.bandcamp.com/album/moon-safari?from=search  "), named("bandcamp", "album", "https://air.bandcamp.com/album/moon-safari"));
        assert_eq!(r("https://air.bandcamp.com/track/la-femme"), None, "a track, not an album");
        assert_eq!(r("https://example.org/release/1"), None);
        assert_eq!(r("not a link"), None);
    }

    fn list(tracks: &[(&str, Option<i64>)]) -> Vec<RefTrack> {
        tracks.iter().zip(1..).map(|((title, length_ms), position)| RefTrack { disc: 1, position, title: title.to_string(), length_ms: *length_ms, recording: None }).collect()
    }

    fn files(tracks: &[(&str, i64)]) -> Vec<Listed> {
        tracks.iter().map(|(t, ms)| Listed::of(t, *ms)).collect()
    }

    /// A row agrees exactly where the count would take it: `identity::agrees`, titles and two seconds.
    #[test]
    fn a_row_agrees_by_the_one_measure() {
        let held_files = files(&[("Slack Water", 200_000), ("Neap", 180_000)]);
        for (l, agrees) in [
            (list(&[("Slack Water", Some(201_000)), ("Neap", Some(181_000))]), true),
            (list(&[("Slack Water", None), ("Neap (feat. A Guest)", Some(180_000))]), true),
            (list(&[("Slack Water", Some(201_000)), ("Neap", Some(184_000))]), false),
            (list(&[("Slack Water", Some(200_000)), ("Spring Tide", Some(180_000))]), false),
            (list(&[("Slack Water", Some(200_000))]), false),
        ] {
            assert_eq!(held(&l, &held_files).0 == "agrees", agrees, "{l:?}");
            assert_eq!(identity::agrees(&l, &held_files), agrees);
        }
        assert_eq!(held(&list(&[("Slack Water", Some(200_000)), ("Spring Tide", Some(180_000))]), &held_files).1, "1 of 2 tracks differ");
        assert_eq!(held(&list(&[("Slack Water", Some(200_000))]), &held_files).1, "1 tracks; the record has 2");
    }

    /// A record had: its tracks on it (no files are read here).
    fn had(cat: &Catalogue, title: &str, tracks: usize) -> (i64, String) {
        let record = cat.record_for_tests(title, tracks);
        (record, ids::record(record))
    }

    /// 0.14's "Use this" before whatever agrees, a list arriving or not; taken back, the count is worked out again.
    #[test]
    fn a_hand_choice_comes_first() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (record, album) = had(&cat, "Tide Tables", 2);
        let tracks = files(&[("Slack Water", 200_000), ("Neap", 180_000)]);
        let choose = |counting: Option<&str>| Choose { counting: Some(counting.map(str::to_string)), ..Choose::default() };
        chose(&cat, record, &album, &choose(Some("discogs")), &tracks).unwrap();
        cat.set_reference_tracks(record, "musicbrainz", &list(&[("Slack Water", Some(200_000)), ("Neap", Some(180_000))])).unwrap();
        assert_eq!(work_out_in(&cat, record, &tracks).unwrap().as_deref(), Some("discogs"), "a list that agrees does not take its place");
        let row = cat.record(record).unwrap().unwrap();
        assert_eq!((row.counting.as_deref(), row.counted_by_hand), (Some("discogs"), true));
        chose(&cat, record, &album, &choose(None), &tracks).unwrap();
        let row = cat.record(record).unwrap().unwrap();
        assert_eq!((row.counting.as_deref(), row.counted_by_hand), (Some("musicbrainz"), false), "taken back: the one that fits");
        assert!(chose(&cat, record, &album, &choose(Some("nowhere")), &tracks).is_err());
    }

    /// A link pasted names what it names: a reference by hand, its source counting, taken from a record that guessed it.
    #[test]
    fn a_pasted_link_is_a_hand_reference() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let ((record, album), (_, other)) = (had(&cat, "Two Rooms", 2), had(&cat, "Two Rooms (Other)", 2));
        let release = Ref::new("discogs", "release", "2000002");
        assert!(cat.set_reference(&other, &release, "search").unwrap());
        let paste = Choose { link: Some(" https://www.discogs.com/release/2000002-Noor-Field-Two-Rooms ".into()), ..Choose::default() };
        chose(&cat, record, &album, &paste, &[]).unwrap();
        assert_eq!(cat.subject_of(&release).unwrap(), [album.clone()]);
        let refs = cat.references_of(&album).unwrap();
        assert_eq!(refs.iter().map(|(r, row)| (r.clone(), row.how.as_str())).collect::<Vec<_>>(), [(release.clone(), "hand")]);
        let row = cat.record(record).unwrap().unwrap();
        assert_eq!((row.counting.as_deref(), row.counted_by_hand), (Some("discogs"), true));
        let k = Kept { title: String::new(), artist: String::new(), refs, lists: Vec::new(), row, matched: None, discogs: None, heard: None, types: HashMap::new(), files: Vec::new() };
        let discogs = &sources(&k, None)[1];
        assert_eq!((discogs.state, discogs.said.as_str(), discogs.guessed), ("agrees", "Pasted by hand", false));
        assert_eq!(discogs.url.as_deref(), Some("https://www.discogs.com/release/2000002"));
        assert!(chose(&cat, record, &album, &Choose { link: Some("https://example.org/x".into()), ..Choose::default() }, &[]).is_err());
    }

    fn matched(release: &str, certain: bool) -> crate::app::matching::Match {
        crate::app::matching::Match {
            release: release.into(),
            release_group: Some("00000000-0000-4000-8000-000000000101".into()),
            title: "Tide Tables".into(),
            artist: "Glass Harbour".into(),
            artist_id: None,
            year: Some(2012),
            certain,
            why: String::new(),
            tracks: Vec::new(),
            lengths: Vec::new(),
        }
    }

    /// A match kept as references, with the release's own list (C43b). A likely one takes nothing another record holds; a
    /// certain one does, as MusicBrainz matched its tracks to it.
    #[test]
    fn a_match_becomes_a_reference_with_its_tracklist() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let ((record, album), (_, other)) = (had(&cat, "Tide Tables", 2), had(&cat, "Tide Tables (Bonus)", 3));
        let tracks = files(&[("Slack Water", 200_000), ("Neap", 180_000)]);
        let own = list(&[("Slack Water", Some(200_400)), ("Neap", Some(180_000))]);
        let release = Ref::new("musicbrainz", "release", "00000000-0000-4000-8000-000000000001");
        cat.set_reference(&other, &release, "files").unwrap();
        keep_match_in(&cat, record, &album, &matched(&release.value, false), &own, false, &tracks).unwrap();
        assert_eq!(cat.subject_of(&release).unwrap(), [other.clone()], "a likely match takes nothing");
        assert!(cat.reference_tracks(record).unwrap().is_empty(), "nor keeps a list of a release not its own");
        keep_match_in(&cat, record, &album, &matched(&release.value, true), &own, false, &tracks).unwrap();
        assert_eq!(cat.subject_of(&release).unwrap(), [album.clone()], "a certain match: its tracks are the release's");
        let how = || cat.references_of(&album).unwrap().into_iter().map(|(r, row)| (r.kind, row.how)).collect::<Vec<_>>();
        assert_eq!(how(), [("release".to_string(), "search".to_string()), ("release-group".to_string(), "search".to_string())]);
        assert_eq!(cat.reference_tracks(record).unwrap(), [("musicbrainz".to_string(), own.clone())], "the release's titles");
        assert_eq!(cat.record(record).unwrap().unwrap().counting.as_deref(), Some("musicbrainz"), "worked out as it arrived");
        keep_match_in(&cat, record, &album, &matched(&release.value, true), &own, true, &tracks).unwrap();
        assert!(how().iter().all(|(_, how)| how == "files"), "matched again by its files' own release: surer");
    }

    /// The count is worked out as each list arrives, and follows the lists (C43b): none fitting, none counting.
    #[test]
    fn counting_follows_a_new_list() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (record, album) = had(&cat, "Tide Tables", 2);
        let tracks = files(&[("Slack Water", 200_000), ("Neap", 180_000)]);
        let fits = list(&[("Slack Water", Some(200_000)), ("Neap", Some(180_000))]);
        let bonus = list(&[("Slack Water", Some(200_000)), ("Neap", Some(180_000)), ("Bonus", Some(90_000))]);
        let counting = || cat.record(record).unwrap().unwrap().counting;
        assert_eq!(work_out_in(&cat, record, &tracks).unwrap(), None, "no list");
        let discogs = crate::app::discogs::AlbumFacts { master: Some(1000001), release: Some(2000001), tracks: bonus.clone(), ..Default::default() };
        keep_discogs_in(&cat, record, &album, &discogs, &tracks).unwrap();
        assert_eq!(counting(), None, "Discogs' main release has a bonus track");
        assert_eq!(cat.references_of(&album).unwrap().len(), 2, "its master and release, kept all the same");
        keep_match_in(&cat, record, &album, &matched("00000000-0000-4000-8000-000000000001", true), &fits, false, &tracks).unwrap();
        assert_eq!(counting().as_deref(), Some("musicbrainz"));
        cat.set_reference_tracks(record, "bandcamp", &fits).unwrap();
        work_out_in(&cat, record, &tracks).unwrap();
        assert_eq!(counting().as_deref(), Some("musicbrainz"), "of two that fit, the first in order");
        cat.set_reference_tracks(record, "musicbrainz", &bonus).unwrap();
        work_out_in(&cat, record, &tracks).unwrap();
        assert_eq!(counting().as_deref(), Some("bandcamp"), "MusicBrainz's read again with another track");
        cat.set_reference_tracks(record, "bandcamp", &bonus).unwrap();
        work_out_in(&cat, record, &tracks).unwrap();
        assert_eq!(counting(), None, "none fits: none counts");
        chose(&cat, record, &album, &Choose { counting: Some(Some("discogs".into())), ..Choose::default() }, &tracks).unwrap();
        cat.set_reference_tracks(record, "musicbrainz", &fits).unwrap();
        work_out_in(&cat, record, &tracks).unwrap();
        assert_eq!(counting().as_deref(), Some("discogs"), "chosen by hand, whatever arrives");
    }

    /// R19 (R7's rest): with no files to hold its lists against, a had record (not served now) counts nothing new, and a record not
    /// had counts its lists in the design's order (C51a's rule): one rule for a list read, a match kept and Discogs' facts.
    #[test]
    fn with_no_files_a_had_record_counts_nothing_and_one_not_had_counts_in_order() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (had, not_had) = (cat.record_for_tests("Signal", 1), cat.record_for_tests("Lamplight", 0));
        for record in [had, not_had] {
            cat.set_reference_tracks(record, "bandcamp", &list(&[("Lamplight", Some(200_000))])).unwrap();
            cat.set_reference_tracks(record, "discogs", &list(&[("Lamplight", Some(200_000))])).unwrap();
        }
        assert_eq!(work_out_in(&cat, had, &[]).unwrap(), None);
        assert_eq!(cat.record(had).unwrap().unwrap().counting, None);
        assert_eq!(work_out_in(&cat, not_had, &[]).unwrap().as_deref(), Some("discogs"), "Discogs before Bandcamp");
    }

    /// A record waiting for a list, its release its own, and a list of `titles` for it.
    fn waiting(cat: &Catalogue, title: &str, release: &str) -> i64 {
        let record = cat.record_for_tests(title, 1);
        assert!(cat.set_reference(&ids::record(record), &Ref::new("musicbrainz", "release", release), "search").unwrap());
        cat.wait_for_list_for_tests(record, release);
        record
    }

    fn files_none() -> Arc<dyn Fn(i64) -> Result<Vec<Listed>> + Send + Sync> {
        Arc::new(|_| Ok(Vec::new()))
    }

    /// R7 (N3): a list read for a record with no files to hold it against (not served now) is kept, and nothing counts by it:
    /// the record waits no more, and is on Health's item.
    #[tokio::test]
    async fn a_list_read_with_no_files_counts_nothing() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = waiting(&cat, "Signal", "r7");
        let read = |_| async { Ok(Some(list(&[("Signal", Some(180_000))]))) };
        assert_eq!(read_lists(&cat, files_none(), read, 10).await.unwrap(), 1);
        assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "no files, no count");
        assert_eq!(cat.reference_tracks(record).unwrap().len(), 1, "the list kept");
        assert!(cat.lists_to_read(10).unwrap().is_empty());
        assert_eq!(cat.unreferenced(true).unwrap(), [record]);
    }

    /// R7 (N4): a release whose question always fails lets the rest through: the others are read in the same pass, and it is
    /// let go of after its tries, onto Health's item; MusicBrainz refusing it (a 4xx, `lists_pass`) lets it go at once.
    #[tokio::test]
    async fn a_release_that_always_fails_lets_the_rest_through() {
        // MusicBrainz switched off makes any failure "out of reach" (`out_of_reach`): the switches as they are by default, under
        // the one test lock, or a test switching it off meanwhile stops this pass at its first failure.
        let _state = super::TestState::set().await;
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let (broken, fine) = (waiting(&cat, "Broken", "r1"), waiting(&cat, "Signal", "r2"));
        let files: Arc<dyn Fn(i64) -> Result<Vec<Listed>> + Send + Sync> = Arc::new(|_| Ok(files(&[("Signal", 180_000)])));
        let read = |release: String| async move { if release == "r1" { anyhow::bail!("not understood") } else { Ok(Some(list(&[("Signal", Some(180_000))]))) } };
        assert_eq!(read_lists(&cat, files.clone(), read, 10).await.unwrap(), 1, "the other read in the same pass");
        assert_eq!(cat.record(fine).unwrap().unwrap().counting.as_deref(), Some("musicbrainz"));
        assert_eq!(cat.lists_to_read(10).unwrap(), [(broken, "r1".to_string())], "tried again later");
        for _ in 1..LIST_TRIES {
            read_lists(&cat, files.clone(), read, 10).await.unwrap();
        }
        assert!(cat.lists_to_read(10).unwrap().is_empty(), "let go of after its tries");
        assert_eq!(cat.unreferenced(true).unwrap(), [broken]);
    }

    /// A server of the test's own answering every question with `status`: what MusicBrainz busy, down or limiting its rate is to
    /// a reader, as `error_for_status` gives it.
    async fn serving(status: u16) -> String {
        let app = axum::Router::new().fallback(move || async move { axum::http::StatusCode::from_u16(status).unwrap() });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/ws/2/release/r1", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        url
    }

    /// The error a reader meets asking `url`.
    async fn asked(url: String) -> anyhow::Error {
        reqwest::get(url).await.unwrap().error_for_status().unwrap_err().into()
    }

    /// R15 (F4): MusicBrainz busy or down (a 5xx) or limiting its rate (a 429) is out of reach, never the release's own failure:
    /// each pass stops at it, and no record waiting for a list is counted against or let go of, however many passes it lasts.
    #[tokio::test]
    async fn a_musicbrainz_outage_lets_no_record_go() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let mut waits = vec![waiting(&cat, "Signal", "r1"), waiting(&cat, "Lamplight", "r2")];
        waits.sort();
        for status in [503, 502, 429] {
            let url = serving(status).await;
            let read = |_| {
                let url = url.clone();
                async move { Err(asked(url).await) }
            };
            for _ in 0..=LIST_TRIES {
                assert_eq!(read_lists(&cat, files_none(), read, 10).await.unwrap(), 0);
            }
            let mut left: Vec<i64> = cat.lists_to_read(10).unwrap().into_iter().map(|(r, _)| r).collect();
            left.sort();
            assert_eq!(left, waits, "{status}: both still wait");
        }
        assert!(cat.unreferenced(true).unwrap().is_empty(), "none on Health's item");
    }

    /// R23 (N13): a 401 or 403 from MusicBrainz, or a proxy in front of it, is out of reach, not "no such release": no record waiting
    /// for a list is let go of, however many passes it lasts; a 404 or a 400 is that answer.
    #[tokio::test]
    async fn a_forbidden_answer_lets_no_record_go() {
        for status in [403, 401] {
            assert!(!no_such(&asked(serving(status).await).await), "{status}");
        }
        assert!(no_such(&asked(serving(404).await).await) && no_such(&asked(serving(400).await).await));
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let mut waits = vec![waiting(&cat, "Signal", "r1"), waiting(&cat, "Lamplight", "r2")];
        waits.sort();
        let url = serving(403).await;
        let read = |_| {
            let url = url.clone();
            async move { Err(asked(url).await) }
        };
        for _ in 0..=LIST_TRIES {
            read_lists(&cat, files_none(), read, 10).await.unwrap();
        }
        let mut left: Vec<i64> = cat.lists_to_read(10).unwrap().into_iter().map(|(r, _)| r).collect();
        left.sort();
        assert_eq!(left, waits);
    }

    /// R23 (N16): with MusicBrainz switched off, a record not had is judged by its next list in the order; one MusicBrainz alone
    /// counts is out of reach until it is on again, nothing said against it.
    #[tokio::test]
    async fn with_musicbrainz_off_a_record_is_judged_by_its_next_list() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let both = wanted(&cat, "Night Walks", &[("musicbrainz", "release", "r5"), ("discogs", "release", "2000005")]);
        let off = |r: Ref| {
            std::future::ready(if r.source == "musicbrainz" { Err(anyhow::Error::from(SwitchedOff("MusicBrainz"))) } else { Ok(Some((list(&[("Lamplight", Some(200_000))]), None, Default::default()))) })
        };
        assert_eq!(tracklist(&cat, both, off).await.unwrap().unwrap().timed.unwrap().0, "discogs");
        let alone = wanted(&cat, "Undated", &[("musicbrainz", "release", "r6")]);
        assert!(tracklist(&cat, alone, off).await.unwrap_err().is::<SwitchedOff>());
    }

    /// R33 (N17, the owner's): a list without its tracks' lengths judges no Soulseek folder by itself: kept, and counting where
    /// nothing counts, it gives way to the next list in the design's order that has them, and stands beside it as the fallback.
    #[tokio::test]
    async fn a_list_without_lengths_gives_way_to_the_next_with_them() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Night Walks", &[("discogs", "release", "2000007"), ("bandcamp", "album", "https://kitetheory.bandcamp.com/album/night-walks")]);
        let read = |r: Ref| std::future::ready(Ok(Some((list(&[("Lamplight", if r.source == "discogs" { None } else { Some(200_000) })]), None, Default::default()))));
        let first = tracklist(&cat, record, read).await.unwrap().unwrap();
        assert_eq!((first.timed.map(|(s, _)| s), first.untimed.map(|(s, _)| s)), (Some("bandcamp".to_string()), Some("discogs".to_string())));
        assert_eq!(cat.record(record).unwrap().unwrap().counting.as_deref(), Some("discogs"), "the first list counts, lengths or none");
        let nothing = |_| std::future::ready(Err::<Option<Read>, _>(anyhow::anyhow!("nothing is read again")));
        let again = tracklist(&cat, record, nothing).await.unwrap().unwrap();
        assert_eq!(again.timed.map(|(s, _)| s).as_deref(), Some("bandcamp"), "both kept: the counted list first, then the next");
    }

    /// X3: with Discogs switched off, a Discogs reference is passed over as MusicBrainz's is: nothing is asked of Discogs.
    #[tokio::test]
    async fn with_discogs_off_a_record_is_judged_by_its_next_list() {
        let _one = super::super::switches::TEST_STATE.lock().await;
        let was = super::super::switches::set_for_tests(super::super::switches::DISCOGS, false);
        let read = read_reference(Ref::new("discogs", "release", "2000009")).await;
        super::super::switches::set_for_tests(super::super::switches::DISCOGS, was);
        assert!(read.unwrap_err().is::<SwitchedOff>());
    }

    /// Y12: with Bandcamp switched off, a record counted by Bandcamp's list (the GROUCH kind) is judged by its next list, its count
    /// kept: Bandcamp's reference says it is switched off, and nothing is asked of Bandcamp.
    #[tokio::test]
    async fn with_bandcamp_off_a_record_is_judged_by_its_next_list() {
        let _one = super::super::switches::TEST_STATE.lock().await;
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Further", &[("bandcamp", "album", "https://grouch.bandcamp.com/album/further"), ("discogs", "release", "2000011")]);
        cat.set_counting(record, Some("bandcamp"), false).unwrap();
        let read = |r: Ref| async move { if r.source == "bandcamp" { read_reference(r).await } else { Ok(Some((list(&[("Lamplight", Some(200_000))]), None, Default::default()))) } };
        let was = super::super::switches::set_for_tests(super::super::switches::BANDCAMP, false);
        let judged = tracklist(&cat, record, read).await;
        super::super::switches::set_for_tests(super::super::switches::BANDCAMP, was);
        assert_eq!(judged.unwrap().unwrap().timed.map(|(s, _)| s).as_deref(), Some("discogs"));
        assert_eq!(cat.record(record).unwrap().unwrap().counting.as_deref(), Some("bandcamp"), "its count kept");
    }

    /// R15 (F4): a release MusicBrainz says it will not give (a 404) has no list, at once; its rate limit (a 429) is no such answer.
    #[tokio::test]
    async fn a_musicbrainz_rate_limit_is_no_answer_about_a_release() {
        assert!(no_such(&asked(serving(404).await).await));
        assert!(!no_such(&asked(serving(429).await).await));
        assert!(!no_such(&asked(serving(503).await).await));
    }

    /// R7 (N4): with MusicBrainz switched off no list comes, so a record waiting for one is on Health's item.
    #[test]
    fn with_musicbrainz_off_waiting_records_are_on_health() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let record = waiting(&cat, "Signal", "r7");
        assert!(cat.unreferenced(true).unwrap().is_empty(), "MusicBrainz on: it waits");
        assert_eq!(cat.unreferenced(false).unwrap(), [record], "off: on the item");
    }

    /// A record not had, of `title`, holding these references (`link`).
    fn wanted(cat: &Catalogue, title: &str, refs: &[(&str, &str, &str)]) -> i64 {
        let record = cat.record_for_tests(title, 0);
        for (source, kind, value) in refs {
            assert!(cat.set_reference(&ids::record(record), &Ref::new(source, kind, value), "link").unwrap());
        }
        record
    }

    /// A reader answering each reference by its source: a list of `titles` for those named, the rest as `other` says; it
    /// counts what it was asked.
    fn reader(lists: &'static [(&'static str, &'static [&'static str])], asked: Arc<std::sync::Mutex<Vec<String>>>, other: fn() -> Result<Option<Read>>) -> impl Fn(Ref) -> std::future::Ready<Result<Option<Read>>> {
        move |r: Ref| {
            asked.lock().unwrap().push(format!("{}:{}", r.source, r.kind));
            let answer = match lists.iter().find(|(s, _)| *s == r.source) {
                Some((_, titles)) => Ok(Some((list(&titles.iter().map(|t| (*t, Some(200_000))).collect::<Vec<_>>()), None, Default::default()))),
                None => other(),
            };
            std::future::ready(answer)
        }
    }

    /// R25 (N6): the release a release group's list was read from is kept as the record's reference, as found (`search`), so the
    /// record knows which edition its list is.
    #[tokio::test]
    async fn a_groups_release_read_is_kept_as_a_reference() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Signal", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000190")]);
        let release = "00000000-0000-4000-8000-000000000191";
        let read = |_| std::future::ready(Ok(Some((list(&[("Signal", Some(200_000))]), Some(Ref::new("musicbrainz", "release", release)), Default::default()))));
        tracklist(&cat, record, read).await.unwrap().unwrap();
        let refs = cat.references_of(&ids::record(record)).unwrap();
        assert!(refs.iter().any(|(r, row)| (r.kind.as_str(), r.value.as_str(), row.how.as_str()) == ("release", release, "search")), "{refs:?}");
    }

    /// C51a: a record's counting list is read once and kept: asked again, the kept list answers and nothing is read.
    #[tokio::test]
    async fn tracklist_reads_once_and_keeps() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Signal", &[("musicbrainz", "release", "r7")]);
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let read = reader(&[("musicbrainz", &["Signal"])], asked.clone(), || Ok(None));
        let (source, first) = tracklist(&cat, record, &read).await.unwrap().unwrap().timed.unwrap();
        let (_, again) = tracklist(&cat, record, &read).await.unwrap().unwrap().timed.unwrap();
        assert_eq!((source.as_str(), first.len(), again), ("musicbrainz", 1, first.clone()));
        assert_eq!(asked.lock().unwrap().len(), 1, "read once");
        assert_eq!(cat.reference_tracks(record).unwrap(), [("musicbrainz".to_string(), first)], "kept");
    }

    /// C51a: the list that counts is the counting source's: a record counted by Discogs reads Discogs', though MusicBrainz is
    /// first in the order.
    #[tokio::test]
    async fn tracklist_follows_counting() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Two Rooms", &[("musicbrainz", "release", "r2"), ("discogs", "release", "2000002")]);
        cat.set_counting(record, Some("discogs"), true).unwrap();
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let read = reader(&[("musicbrainz", &["Hall"]), ("discogs", &["Hall", "Stair"])], asked.clone(), || Ok(None));
        let (source, list) = tracklist(&cat, record, &read).await.unwrap().unwrap().timed.unwrap();
        assert_eq!((source.as_str(), list.len()), ("discogs", 2));
        assert_eq!(*asked.lock().unwrap(), ["discogs:release"]);
    }

    /// D18 (C54): the References panel works on a record not had. It is read with its names from the catalogue, its list as found
    /// (no files to hold it against). A link pasted on it is its hand reference and counts, the list kept from another reference
    /// let go of, so Search now (`tracklist`) reads what was pasted; "Use this" the same.
    #[tokio::test]
    async fn the_references_panel_works_on_a_record_not_had() {
        let served = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        served.attach_catalogue(Arc::new(Catalogue::open(None, Vec::new()).unwrap()));
        let cat = served.catalogue().clone();
        let record = wanted(&cat, "Night Walks", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000192"), ("discogs", "release", "2000010")]);
        let album = ids::record(record);
        cat.set_reference_tracks(record, "musicbrainz", &list(&[("Lamplight", Some(200_000)), ("Bonus", Some(90_000))])).unwrap();
        cat.set_counting(record, Some("musicbrainz"), false).unwrap();
        let k = kept(&served, &album, record).unwrap();
        assert_eq!((k.title.as_str(), k.artist.as_str()), ("Night Walks", "Glass Harbour"));
        let mb = &sources(&k, None)[0];
        assert_eq!((mb.state, mb.said.as_str(), mb.tracks), ("found", "2 tracks", Some(2)), "no files to hold it against");
        let release = "00000000-0000-4000-8000-000000000193";
        choose_in(&served, &album, record, &Choose { link: Some(format!("https://musicbrainz.org/release/{release}")), ..Choose::default() }).unwrap();
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let read = {
            let asked = asked.clone();
            move |r: Ref| {
                asked.lock().unwrap().push(format!("{}:{}", r.kind, r.value));
                std::future::ready(Ok(Some((list(&[("Lamplight", Some(200_000))]), None, Default::default()))))
            }
        };
        let (source, pasted) = tracklist(&cat, record, &read).await.unwrap().unwrap().timed.unwrap();
        assert_eq!((source.as_str(), pasted.len()), ("musicbrainz", 1), "the pasted release's list, not the one kept before");
        assert_eq!(*asked.lock().unwrap(), [format!("release:{release}")]);
        asked.lock().unwrap().clear();
        choose_in(&served, &album, record, &Choose { counting: Some(Some("discogs".into())), ..Choose::default() }).unwrap();
        assert_eq!(tracklist(&cat, record, &read).await.unwrap().unwrap().timed.unwrap().0, "discogs", "Use this");
        assert_eq!(*asked.lock().unwrap(), ["release:2000010"]);
    }

    /// C51a: a record not had counts its first reference in the design's order whose list answers: MusicBrainz before Discogs
    /// before Bandcamp; one answering nothing (no such release) gives way to the next. That source counts from then on.
    #[tokio::test]
    async fn a_record_not_had_counts_its_first_reference_in_order() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let all = wanted(&cat, "Night Walks", &[("bandcamp", "album", "https://kitetheory.bandcamp.com/album/night-walks"), ("discogs", "release", "2000003"), ("musicbrainz", "release", "r3")]);
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let every = reader(&[("musicbrainz", &["Lamplight"]), ("discogs", &["Lamplight"]), ("bandcamp", &["Lamplight"])], asked.clone(), || Ok(None));
        assert_eq!(tracklist(&cat, all, &every).await.unwrap().unwrap().timed.unwrap().0, "musicbrainz");
        assert_eq!(cat.record(all).unwrap().unwrap().counting.as_deref(), Some("musicbrainz"), "it counts from then on");
        let gone = wanted(&cat, "Undated", &[("bandcamp", "album", "https://kitetheory.bandcamp.com/album/undated"), ("musicbrainz", "release", "r4")]);
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mb_gone = reader(&[("bandcamp", &["Morning"])], asked.clone(), || Ok(None));
        assert_eq!(tracklist(&cat, gone, &mb_gone).await.unwrap().unwrap().timed.unwrap().0, "bandcamp", "MusicBrainz has no such release: the next");
        assert_eq!(*asked.lock().unwrap(), ["musicbrainz:release", "bandcamp:album"]);
    }

    /// C51a, the owner's rule: a record with files where no list agrees counts nothing, and nothing is read for it, until an
    /// administrator picks a list. Its own lists are not tried in order: that is a record not had's way alone.
    #[tokio::test]
    async fn a_record_with_files_where_nothing_agrees_counts_nothing() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = cat.record_for_tests("Tide Tables", 2);
        assert!(cat.set_reference(&ids::record(record), &Ref::new("musicbrainz", "release", "r1"), "files").unwrap());
        cat.set_reference_tracks(record, "musicbrainz", &list(&[("Other", Some(1_000)), ("Neap", Some(180_000))])).unwrap();
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let read = reader(&[("musicbrainz", &["Slack Water", "Neap"])], asked.clone(), || Ok(None));
        assert_eq!(tracklist(&cat, record, &read).await.unwrap(), Err("No list counts until one is chosen".to_string()));
        assert!(asked.lock().unwrap().is_empty(), "nothing read");
        assert_eq!(cat.record(record).unwrap().unwrap().counting, None);
    }

    /// C51a, R23 (N3): Bandcamp's bot check where Bandcamp is the only reference is Bandcamp out of reach, nothing said against the
    /// record: asked again later; with no reference at all, no list, and why in few words; nothing counts.
    #[tokio::test]
    async fn a_record_with_only_bandcamp_behind_its_check_has_no_list() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "After Hours", &[("bandcamp", "album", "https://paperlantern.bandcamp.com/album/after-hours")]);
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let checked = reader(&[], asked, || Err(anyhow::Error::from(crate::bandcamp::NotRead)));
        assert!(crate::bandcamp::not_read(&tracklist(&cat, record, &checked).await.unwrap_err()));
        let none = wanted(&cat, "Quiet Lanes", &[]);
        assert_eq!(tracklist(&cat, none, &checked).await.unwrap(), Err("No list to judge by".to_string()));
        assert_eq!(cat.record(record).unwrap().unwrap().counting, None);
    }

    /// Health's item: what 0.16 left off it stays off, in one query. A guess confirmed and checked by hand, the record's.
    #[test]
    fn a_record_no_reference_speaks_for_is_healths() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let [(alone, _), (counted, _), (checked, _), (_, named_id), (_, pasted_id), (guessed, guessed_id)] =
            ["Alone", "Counted", "Checked", "Named", "Pasted", "Guessed"].map(|t| had(&cat, t, 1));
        cat.set_counting(counted, Some("musicbrainz"), false).unwrap();
        let none = Choose { by_hand: Some(true), ..Choose::default() };
        assert!(chose(&cat, checked, &ids::record(checked), &none, &[]).unwrap(), "checked now");
        assert!(!chose(&cat, checked, &ids::record(checked), &none, &[]).unwrap(), "and not again");
        cat.set_reference(&named_id, &Ref::new("musicbrainz", "release", "00000000-0000-4000-8000-000000000001"), "files").unwrap();
        cat.set_reference(&pasted_id, &Ref::new("bandcamp", "album", "https://glassharbour.bandcamp.com/album/x"), "hand").unwrap();
        cat.set_reference(&guessed_id, &Ref::new("musicbrainz", "release", "00000000-0000-4000-8000-000000000002"), "search").unwrap();
        cat.set_reference(&guessed_id, &Ref::new("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000102"), "files").unwrap();
        assert_eq!(cat.confirm_reference(&guessed_id, "musicbrainz").unwrap(), 2);
        assert!(cat.references_of(&guessed_id).unwrap().iter().all(|(_, row)| row.confirmed));
        had(&cat, "Not Had", 0);
        assert_eq!(cat.unreferenced(true).unwrap(), [alone, guessed], "a guess and a group are no reference that speaks");
    }

    // ---- F1 (preview 1's fix batch): a record not had says why it has no tracks

    use crate::app::playing::{RecordPage, page_at};
    use crate::app::switches::{BACKGROUND, BANDCAMP, DISCOGS, MUSICBRAINZ, set_for_tests};
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

    /// A Bandcamp page of the tests' artist.
    const PAGE: &str = "https://kitetheory.bandcamp.com/album/answered";

    /// A record not had's page as the server answers it at `now`, its list asked for through `read`.
    async fn opened<R, F>(cat: &Arc<Catalogue>, record: i64, now: i64, read: R) -> crate::app::AlbumView
    where
        R: Fn(Ref) -> F + Send + Sync + 'static,
        F: std::future::Future<Output = Result<Option<Read>>> + Send + 'static,
    {
        match page_at(cat, record, now, read).await.unwrap() {
            Some(RecordPage::NotHad(v)) => *v,
            _ => panic!("al-{record} is no page of a record not had"),
        }
    }

    /// What the page says a source said.
    fn saying(source: &'static str, said: &'static str) -> Said {
        Said { source, said, until: None }
    }

    /// What a reader was asked, in order.
    fn asks() -> Arc<std::sync::Mutex<Vec<String>>> {
        Arc::new(std::sync::Mutex::new(Vec::new()))
    }

    /// `reader`, minding the MusicBrainz switch as `read_reference` does: switched off, it says so and is asked nothing.
    fn minding(lists: &'static [(&'static str, &'static [&'static str])], asked: Arc<std::sync::Mutex<Vec<String>>>) -> impl Fn(Ref) -> std::future::Ready<Result<Option<Read>>> + Send + Sync + 'static {
        let read = reader(lists, asked, || Ok(None));
        move |r: Ref| {
            if r.source == "musicbrainz" && !crate::app::switches::is_on(MUSICBRAINZ) {
                return std::future::ready(Err(SwitchedOff("MusicBrainz").into()));
            }
            read(r)
        }
    }

    type Asking = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Read>>> + Send>>;

    /// A reader held at a gate until the test opens it, then answering any reference with one track; it counts its questions.
    fn gated(gate: &Arc<tokio::sync::Semaphore>, asked: &Arc<AtomicUsize>) -> impl Fn(Ref) -> Asking + Clone + Send + Sync + 'static {
        let (gate, asked) = (gate.clone(), asked.clone());
        move |_: Ref| -> Asking {
            asked.fetch_add(1, SeqCst);
            let gate = gate.clone();
            Box::pin(async move {
                let _open = gate.acquire().await;
                Ok::<Option<Read>, anyhow::Error>(Some((list(&[("Lamplight", Some(200_000))]), None, Default::default())))
            })
        }
    }

    /// Waits for a gated reader's first question (its read begun), then a beat for any other.
    async fn first_asked(asked: &AtomicUsize) {
        for _ in 0..1_000 {
            if asked.load(SeqCst) > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("nothing was asked");
    }

    /// The titles of a page's rows.
    fn titles(v: &crate::app::AlbumView) -> Vec<String> {
        v.rows.iter().map(|r| r.name.clone().unwrap_or_default()).collect()
    }

    /// `gated`, but MusicBrainz's release `r62` has none at once, so a read of it moves on; it keeps what it is asked (source:value).
    fn following(gate: &Arc<tokio::sync::Semaphore>, asked: &Arc<std::sync::Mutex<Vec<String>>>) -> impl Fn(Ref) -> Asking + Clone + Send + Sync + 'static {
        let (gate, asked) = (gate.clone(), asked.clone());
        move |r: Ref| -> Asking {
            asked.lock().unwrap().push(format!("{}:{}", r.source, r.value));
            let gate = gate.clone();
            Box::pin(async move {
                if r.value == "r62" {
                    return Ok::<Option<Read>, anyhow::Error>(None);
                }
                let _open = gate.acquire().await;
                Ok::<Option<Read>, anyhow::Error>(Some((list(&[("Lamplight", Some(200_000))]), None, Default::default())))
            })
        }
    }

    /// Waits until `asked` holds `what`.
    async fn until_asked(asked: &std::sync::Mutex<Vec<String>>, what: &str) {
        for _ in 0..1_000 {
            if asked.lock().unwrap().iter().any(|a| a == what) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("{what} was never asked");
    }

    /// F6 (F1.12's loader): while a record not had's list is read, its page is told the source the read asks now: before its first
    /// question the first it will ask, then each it moves on to; with MusicBrainz switched off, Discogs from the start. None once the
    /// read has ended.
    #[tokio::test]
    async fn a_read_says_which_source_it_reads() {
        let _state = TestState::set().await;
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let asked = asks();
        let t = super::super::moment::now_secs();
        // MusicBrainz asked, waiting at its gate.
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let waits = wanted(&cat, "Answered", &[("musicbrainz", "release", "r61"), ("discogs", "release", "2000061")]);
        let v = opened(&cat, waits, t, following(&gate, &asked)).await;
        assert_eq!((v.more.looking, v.more.reading), (true, Some("musicbrainz")), "the first it will ask");
        until_asked(&asked, "musicbrainz:r61").await;
        assert_eq!(opened(&cat, waits, t, following(&gate, &asked)).await.more.reading, Some("musicbrainz"), "while MusicBrainz is asked");
        gate.add_permits(8);
        page_read_settled(&cat, waits).await;
        let v = opened(&cat, waits, t + 1, following(&gate, &asked)).await;
        assert_eq!((v.rows.len(), v.more.looking, v.more.reading), (1, false, None), "ended: no source named");
        assert!(serde_json::to_value(&v).unwrap().get("reading").is_none());
        // MusicBrainz has none: the read moves on to Discogs, and says so.
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let moves = wanted(&cat, "Break Synonyms", &[("musicbrainz", "release", "r62"), ("discogs", "release", "2000062")]);
        assert_eq!(opened(&cat, moves, t, following(&gate, &asked)).await.more.reading, Some("musicbrainz"));
        until_asked(&asked, "discogs:2000062").await;
        assert_eq!(opened(&cat, moves, t, following(&gate, &asked)).await.more.reading, Some("discogs"), "it follows the read");
        gate.add_permits(8);
        page_read_settled(&cat, moves).await;
        // MusicBrainz switched off: Discogs from the start.
        set_for_tests(MUSICBRAINZ, false);
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let off = wanted(&cat, "The Curved Line", &[("musicbrainz", "release", "r63"), ("discogs", "release", "2000063")]);
        assert_eq!(opened(&cat, off, t, following(&gate, &asked)).await.more.reading, Some("discogs"), "MusicBrainz off: Discogs first");
        until_asked(&asked, "discogs:2000063").await;
        assert_eq!(opened(&cat, off, t, following(&gate, &asked)).await.more.reading, Some("discogs"), "while Discogs is asked");
        gate.add_permits(8);
        page_read_settled(&cat, off).await;
        assert!(!asked.lock().unwrap().iter().any(|a| a == "musicbrainz:r63"), "MusicBrainz asked nothing while off");
    }

    /// F1.2: a source that does not answer is said once its read ends, and read again after a quarter hour, not before; meanwhile the
    /// next source is neither read nor kept nor counted (C51a: the first that answers counts from then on), and never "none".
    #[tokio::test]
    async fn a_source_that_does_not_answer_is_said_and_read_again_after_a_quarter_hour() {
        let _state = TestState::set().await;
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Answered", &[("musicbrainz", "release", "r12"), ("bandcamp", "album", PAGE)]);
        let asked = asks();
        let read = || reader(&[("bandcamp", &["Lamplight", "Morning"])], asked.clone(), || Err(anyhow::anyhow!("MusicBrainz answered 503 Service Unavailable")));
        let t = super::super::moment::now_secs();
        let first = opened(&cat, record, t, read()).await;
        assert!(first.more.looking && first.rows.is_empty() && first.more.no_list.is_none(), "at t: being read");
        page_read_settled(&cat, record).await;
        let later = opened(&cat, record, t + 60, read()).await;
        assert!(!later.more.looking && later.rows.is_empty(), "a minute on: read, nothing to show");
        assert_eq!(later.more.no_list, Some(vec![saying("musicbrainz", "unanswered")]));
        assert_eq!(*asked.lock().unwrap(), ["musicbrainz:release"], "MusicBrainz alone, once; Bandcamp not while MusicBrainz is away");
        assert!(cat.reference_tracks(record).unwrap().is_empty(), "nothing kept");
        assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "nothing counted");
        let again = opened(&cat, record, t + 960, read()).await;
        assert!(again.more.looking && again.more.no_list.is_none(), "a quarter hour on: read again");
        page_read_settled(&cat, record).await;
        assert_eq!(*asked.lock().unwrap(), ["musicbrainz:release", "musicbrainz:release"]);
    }

    /// F1.3: none of a record's references gives a list: each named as having none, nothing counted, no list kept; never "off" or
    /// "unanswered", nor the three lost to one word.
    #[tokio::test]
    async fn a_record_no_reference_lists_says_which_have_none() {
        let _state = TestState::set().await;
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Answered", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000513"), ("discogs", "master", "4002"), ("bandcamp", "album", PAGE)]);
        let asked = asks();
        let t = super::super::moment::now_secs();
        assert!(opened(&cat, record, t, reader(&[], asked.clone(), || Ok(None))).await.more.looking);
        page_read_settled(&cat, record).await;
        let v = opened(&cat, record, t + 1, reader(&[], asked.clone(), || Ok(None))).await;
        assert!(!v.more.looking && v.rows.is_empty());
        assert_eq!(v.more.no_list, Some(vec![saying("musicbrainz", "none"), saying("discogs", "none"), saying("bandcamp", "none")]));
        assert_eq!(cat.record(record).unwrap().unwrap().counting, None, "nothing counted");
        assert!(cat.reference_tracks(record).unwrap().is_empty(), "no list kept");
        assert_eq!(*asked.lock().unwrap(), ["musicbrainz:release-group", "discogs:master", "bandcamp:album"], "each read once");
    }

    /// F1.5: Bandcamp held back after its bot check says until when: (a) a record with only its page, at once, nothing read; (b) one
    /// whose MusicBrainz release has none, once its read ends, Bandcamp asked nothing while held. After the hold, or Bandcamp switched
    /// off and on (its hold lifted), (a) is read at once. Never "unanswered" of a hold Cratarr keeps itself.
    #[tokio::test]
    async fn bandcamp_held_back_says_until_when() {
        let _state = TestState::set().await;
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let t = super::super::moment::now_secs();
        let until = t + 3_600;
        crate::bandcamp::hold_for_tests(until);
        let alone = wanted(&cat, "Answered", &[("bandcamp", "album", PAGE)]);
        let both = wanted(&cat, "Kyverdale Road", &[("musicbrainz", "release", "r15"), ("bandcamp", "album", "https://kitetheory.bandcamp.com/album/kyverdale-road")]);
        let lifted = wanted(&cat, "The Curved Line", &[("bandcamp", "album", "https://kitetheory.bandcamp.com/album/the-curved-line")]);
        let asked = asks();
        let read = || {
            let asked = asked.clone();
            move |r: Ref| {
                asked.lock().unwrap().push(format!("{}:{}", r.source, r.kind));
                std::future::ready(if r.source == "bandcamp" { Err(anyhow::Error::from(crate::bandcamp::NotRead)) } else { Ok::<Option<Read>, anyhow::Error>(None) })
            }
        };
        let held = Said { source: "bandcamp", said: "held", until: Some(until) };
        let a = opened(&cat, alone, t, read()).await;
        assert!(!a.more.looking, "(a) at once, nothing read");
        assert_eq!(a.more.no_list, Some(vec![held.clone()]));
        assert!(asked.lock().unwrap().is_empty());
        assert!(opened(&cat, both, t, read()).await.more.looking, "(b) MusicBrainz is to be read");
        page_read_settled(&cat, both).await;
        let b = opened(&cat, both, t + 1, read()).await;
        assert!(!b.more.looking);
        assert_eq!(b.more.no_list, Some(vec![saying("musicbrainz", "none"), held.clone()]));
        assert_eq!(*asked.lock().unwrap(), ["musicbrainz:release"], "Bandcamp asked nothing while held");
        assert!(opened(&cat, alone, until + 1, read()).await.more.looking, "(a) after its hold: read at once");
        page_read_settled(&cat, alone).await;
        assert_eq!(opened(&cat, lifted, t, read()).await.more.no_list, Some(vec![held.clone()]));
        set_for_tests(BANDCAMP, false);
        crate::bandcamp::switched(None, false);
        set_for_tests(BANDCAMP, true);
        crate::bandcamp::switched(None, true);
        assert!(opened(&cat, lifted, t, read()).await.more.looking, "switched off and on: its hold lifted, read at once");
        page_read_settled(&cat, lifted).await;
    }

    /// F1.6 (kind 3): a source said to be switched off that is on since is read at once: a record said off at once, and one whose read
    /// came to "MusicBrainz off, Discogs unanswered" a minute before; MusicBrainz is asked, and its list shown at the next answer.
    /// Never "switched off" said while it is on, nor a quarter hour waited.
    #[tokio::test]
    async fn a_source_switched_on_since_is_read_at_once() {
        let _state = TestState::set().await;
        set_for_tests(MUSICBRAINZ, false);
        set_for_tests(DISCOGS, false);
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let off = wanted(&cat, "Answered", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000516"), ("discogs", "master", "4001")]);
        let away = wanted(&cat, "Break Synonyms", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000517"), ("discogs", "master", "4006")]);
        let asked = asks();
        let read = || reader(&[("musicbrainz", &["Lamplight"])], asked.clone(), || Err(anyhow::anyhow!("Discogs answered 503 Service Unavailable")));
        let t = super::super::moment::now_secs();
        assert_eq!(opened(&cat, off, t, read()).await.more.no_list, Some(vec![saying("musicbrainz", "off"), saying("discogs", "off")]));
        set_for_tests(DISCOGS, true);
        assert!(opened(&cat, away, t, read()).await.more.looking);
        page_read_settled(&cat, away).await;
        assert_eq!(opened(&cat, away, t + 1, read()).await.more.no_list, Some(vec![saying("musicbrainz", "off"), saying("discogs", "unanswered")]));
        assert_eq!(*asked.lock().unwrap(), ["discogs:master"], "nothing asked of MusicBrainz while it was off");
        set_for_tests(MUSICBRAINZ, true);
        for record in [off, away] {
            let v = opened(&cat, record, t + 60, read()).await;
            assert!(v.more.looking && v.more.no_list.is_none(), "{record}: read at once, nothing said off");
            page_read_settled(&cat, record).await;
            let v = opened(&cat, record, t + 61, read()).await;
            assert_eq!((titles(&v), v.more.looking, v.more.no_list.clone()), (vec!["Lamplight".to_string()], false, None), "{record}: MusicBrainz's list shown");
        }
        assert_eq!(asked.lock().unwrap().iter().filter(|a| *a == "musicbrainz:release-group").count(), 2, "MusicBrainz asked for each");
    }

    /// F1.8 (kind 4): two opens at once read once; both say it is being read, and once it ends one list is kept and each next answer
    /// has its rows. Never two reads of one record at once, nor a read ended left `looking`.
    #[tokio::test]
    async fn two_opens_read_once() {
        let _state = TestState::set().await;
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Fourth: The Golden Eagle", &[("musicbrainz", "release", "r18")]);
        let (gate, asked) = (Arc::new(tokio::sync::Semaphore::new(0)), Arc::new(AtomicUsize::new(0)));
        let read = gated(&gate, &asked);
        let t = super::super::moment::now_secs();
        let (a, b) = tokio::join!(opened(&cat, record, t, read.clone()), opened(&cat, record, t, read.clone()));
        assert!(a.more.looking && b.more.looking, "both see it read");
        first_asked(&asked).await;
        assert_eq!(asked.load(SeqCst), 1, "one read");
        gate.add_permits(8);
        page_read_settled(&cat, record).await;
        assert_eq!(cat.reference_tracks(record).unwrap().len(), 1, "one list kept");
        let (a, b) = tokio::join!(opened(&cat, record, t + 1, read.clone()), opened(&cat, record, t + 1, read.clone()));
        for v in [a, b] {
            assert_eq!((v.rows.len(), v.more.looking, v.more.no_list.clone()), (1, false, None), "each has the rows");
        }
        assert_eq!(asked.load(SeqCst), 1, "nothing read again");
    }

    /// F1.9 (kind 4): a read that never ends is cut at `PAGE_READ_AT_MOST` (a second in tests) and said unanswered: never `looking` for
    /// good, never left "being read".
    #[tokio::test]
    async fn a_page_read_that_never_ends_is_cut_and_said_unanswered() {
        let _state = TestState::set().await;
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Answered", &[("musicbrainz", "release", "r19")]);
        let silent = |_: Ref| std::future::pending::<Result<Option<Read>>>();
        let t = super::super::moment::now_secs();
        assert!(opened(&cat, record, t, silent).await.more.looking);
        tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
        page_read_settled(&cat, record).await;
        let v = opened(&cat, record, t + 2, silent).await;
        assert!(!v.more.looking, "cut: no longer looking");
        assert_eq!(v.more.no_list, Some(vec![saying("musicbrainz", "unanswered")]));
        assert_eq!(page_read_of(&cat, record), Some(false), "ended, not left being read");
    }

    /// F1.10 (kind 4): a record merged into another while its page's read waits on Bandcamp: the list lands on the record it is now,
    /// whose page (opened by the old id) shows its rows. Never a list kept on a merged-away record.
    #[tokio::test]
    async fn a_list_read_during_a_merge_lands_on_the_record_it_is_now() {
        let _state = TestState::set().await;
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let a = wanted(&cat, "Kyverdale Road", &[("bandcamp", "album", "https://kitetheory.bandcamp.com/album/kyverdale-road")]);
        let b = wanted(&cat, "Kyverdale Road", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000510")]);
        let (gate, asked) = (Arc::new(tokio::sync::Semaphore::new(0)), Arc::new(AtomicUsize::new(0)));
        let read = gated(&gate, &asked);
        let t = super::super::moment::now_secs();
        assert!(opened(&cat, a, t, read.clone()).await.more.looking);
        first_asked(&asked).await;
        cat.merge_records(a, b).unwrap();
        gate.add_permits(8);
        page_read_settled(&cat, a).await;
        let kept = cat.reference_tracks(b).unwrap();
        assert!(kept.iter().any(|(s, l)| s == "bandcamp" && l.len() == 1), "on the record it is now: {kept:?}");
        let on_a: i64 = cat.db_for_tests().query_row("SELECT COUNT(*) FROM reference_tracks WHERE record = ?1", [a], |r| r.get(0)).unwrap();
        assert_eq!(on_a, 0, "none on the record merged away");
        let v = opened(&cat, a, t + 1, read.clone()).await;
        assert_eq!((v.id.clone(), titles(&v)), (ids::record(b), vec!["Lamplight".to_string()]), "its old id opens the page it is now, with its rows");
    }

    /// F1.11, the pause (a pin): a page's read is the page's, not background reading's: it goes while background reading is paused,
    /// its list kept and shown; nothing says "paused".
    #[tokio::test]
    async fn a_page_read_goes_while_background_reading_is_paused() {
        let _state = TestState::set().await;
        set_for_tests(BACKGROUND, false);
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Answered", &[("musicbrainz", "release", "r111")]);
        let asked = asks();
        let read = || reader(&[("musicbrainz", &["Lamplight", "Morning"])], asked.clone(), || Ok(None));
        let t = super::super::moment::now_secs();
        let v = opened(&cat, record, t, read()).await;
        assert!(v.more.looking && v.more.no_list.is_none(), "read, no word of a pause");
        page_read_settled(&cat, record).await;
        let v = opened(&cat, record, t + 1, read()).await;
        assert_eq!((v.rows.len(), v.more.looking, v.more.no_list.clone()), (2, false, None));
        assert_eq!(cat.reference_tracks(record).unwrap().len(), 1, "its list kept");
        assert_eq!(*asked.lock().unwrap(), ["musicbrainz:release"]);
    }

    /// F1.11, Y12 (a pin): MusicBrainz switched off, Bandcamp's list answers: its rows show and Bandcamp counts; MusicBrainz switched
    /// on later reads nothing new for it, its count kept. As before.
    #[tokio::test]
    async fn with_musicbrainz_off_a_pages_read_counts_bandcamp_and_keeps_it() {
        let _state = TestState::set().await;
        set_for_tests(MUSICBRAINZ, false);
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Answered", &[("musicbrainz", "release", "r112"), ("bandcamp", "album", PAGE)]);
        let asked = asks();
        let read = || minding(&[("musicbrainz", &["Other"]), ("bandcamp", &["Lamplight", "Morning"])], asked.clone());
        let t = super::super::moment::now_secs();
        assert!(opened(&cat, record, t, read()).await.more.looking);
        page_read_settled(&cat, record).await;
        let shown = vec!["Lamplight".to_string(), "Morning".to_string()];
        let v = opened(&cat, record, t + 1, read()).await;
        assert_eq!((titles(&v), v.more.looking), (shown.clone(), false), "Bandcamp's rows");
        assert_eq!(cat.record(record).unwrap().unwrap().counting.as_deref(), Some("bandcamp"), "Bandcamp counts");
        set_for_tests(MUSICBRAINZ, true);
        let v = opened(&cat, record, t + 2, read()).await;
        assert_eq!((titles(&v), v.more.looking), (shown, false), "nothing new read");
        assert_eq!(*asked.lock().unwrap(), ["bandcamp:album"]);
        assert_eq!(cat.record(record).unwrap().unwrap().counting.as_deref(), Some("bandcamp"), "its count stays");
    }

    /// F1.11, RT01: a kept list let go of after thirty days unopened is read again at the next open, and shown.
    #[tokio::test]
    async fn a_list_let_go_of_after_thirty_days_is_read_again_when_opened() {
        let _state = TestState::set().await;
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Answered", &[("musicbrainz", "release", "r113")]);
        let asked = asks();
        let read = || reader(&[("musicbrainz", &["Lamplight"])], asked.clone(), || Ok(None));
        let t = super::super::moment::now_secs();
        assert!(opened(&cat, record, t, read()).await.more.looking);
        page_read_settled(&cat, record).await;
        assert_eq!(opened(&cat, record, t + 1, read()).await.rows.len(), 1);
        cat.db_for_tests().execute("UPDATE reference_tracks SET at = ?1", [t - 45 * 86_400]).unwrap();
        cat.db_for_tests().execute("UPDATE records SET opened_at = ?1", [t - 45 * 86_400]).unwrap();
        assert_eq!(cat.forget_quiet_lists(t, &Default::default()).unwrap(), 1, "let go of");
        let v = opened(&cat, record, t + 2, read()).await;
        assert!(v.more.looking && v.rows.is_empty() && v.more.no_list.is_none(), "read again");
        page_read_settled(&cat, record).await;
        assert_eq!(opened(&cat, record, t + 3, read()).await.rows.len(), 1);
        assert_eq!(asked.lock().unwrap().len(), 2);
    }

    /// F1.11, Soulseek's Stuck: for a record whose sources all have none, `tracklist`'s why names the same sources as the page's
    /// `no_list`: one set of outcomes, two wordings. Never two different source lists for one record.
    #[tokio::test]
    async fn the_page_and_soulseek_name_the_same_sources() {
        let _state = TestState::set().await;
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let record = wanted(&cat, "Answered", &[("musicbrainz", "release-group", "00000000-0000-4000-8000-000000000511"), ("discogs", "master", "4011"), ("bandcamp", "album", PAGE)]);
        let asked = asks();
        let none = || reader(&[], asked.clone(), || Ok(None));
        let t = super::super::moment::now_secs();
        opened(&cat, record, t, none()).await;
        page_read_settled(&cat, record).await;
        let listed = opened(&cat, record, t + 1, none()).await.more.no_list.expect("the page says why");
        let why = tracklist(&cat, record, none()).await.unwrap().expect_err("no list to judge by");
        let named: Vec<&str> = identity::COUNTING_ORDER.iter().copied().filter(|s| why.contains(source_name(s))).collect();
        assert_eq!(listed.iter().map(|s| s.source).collect::<Vec<_>>(), named, "{why}");
        assert_eq!(named.len(), 3, "{why}");
        assert!(listed.iter().all(|s| s.said == "none"), "{listed:?}");
    }

    /// F18.2: records credited to nobody (opened from a label's rows by a name no artist has, two have, or Discogs marks as shared);
    /// each release's read names its artists by Discogs id, one release a collaboration of two. Each artist found by the id, else the
    /// one live artist of the name holding no Discogs id, else one made; credited in the release's order. A record credited before
    /// keeps its credit (one the release contradicts is said in the log, nothing moved); never a second Discogs id on one artist.
    #[tokio::test]
    async fn a_release_read_credits_the_artists_it_names() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let field = cat.artist_for("Noor Field").unwrap();
        let held = cat.artist_for("Orbit Lane").unwrap();
        assert!(cat.set_reference(&ids::artist(held), &Ref::new("discogs", "artist", "9300"), "link").unwrap());
        let (signal, together, lamplight) = (wanted(&cat, "Signal", &[("discogs", "release", "3800001")]), wanted(&cat, "Together", &[("discogs", "release", "3800002")]), wanted(&cat, "Lamplight", &[("discogs", "release", "3800003")]));
        let credited_before = wanted(&cat, "Undertow", &[("discogs", "release", "3800004")]);
        cat.db_for_tests().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'listing')", rusqlite::params![credited_before, field]).unwrap();
        let read = |r: Ref| {
            let artists: Vec<(Ref, String)> = match r.value.as_str() {
                "3800001" => vec![(Ref::new("discogs", "artist", "9101"), "Noor Field".into())],
                "3800002" => vec![(Ref::new("discogs", "artist", "9102"), "Kite Theory".into()), (Ref::new("discogs", "artist", "9300"), "Orbit Lane (2)".into())],
                "3800003" => vec![(Ref::new("discogs", "artist", "9103"), "Glass Harbour (2)".into())],
                _ => vec![(Ref::new("discogs", "artist", "9104"), "Tape Club".into())],
            };
            std::future::ready(Ok(Some((list(&[("One", Some(200_000))]), None, ReadMore { also: Vec::new(), artists }))))
        };
        let t = crate::app::moment::now_secs();
        for r in [signal, together, lamplight, credited_before] {
            opened(&cat, r, t, read).await;
            page_read_settled(&cat, r).await;
        }
        let artist_holding = |id: &str| cat.subject_of(&Ref::new("discogs", "artist", id)).unwrap().iter().find_map(|s| ids::artist_number(s));
        assert_eq!(cat.record_artists(signal).unwrap(), [field], "the one Noor Field holding no Discogs id, given it");
        assert_eq!(artist_holding("9101"), Some(field));
        let kite = cat.artist_of("Kite Theory").unwrap().expect("made by the release's id");
        assert_eq!(cat.record_artists(together).unwrap(), [kite, held], "a collaboration, in the release's order, each by their id");
        let made = cat.record_artists(lamplight).unwrap();
        assert_eq!(made.len(), 1);
        assert_ne!(cat.artist_named(made[0]).unwrap().unwrap().name, "", "a name Discogs marks as shared: an artist of their own");
        assert_eq!(artist_holding("9103"), Some(made[0]));
        assert_eq!(cat.record_artists(credited_before).unwrap(), [field], "credited before: kept");
        assert_eq!(artist_holding("9104"), None, "nothing moved, nothing made for it");
        let discogs_ids: i64 = cat.db_for_tests().query_row("SELECT max(n) FROM (SELECT count(*) AS n FROM \"references\" WHERE subject GLOB 'ar-*' AND source = 'discogs' AND kind = 'artist' GROUP BY subject)", [], |r| r.get(0)).unwrap();
        assert_eq!(discogs_ids, 1, "never a second Discogs id on one artist");
    }
}
