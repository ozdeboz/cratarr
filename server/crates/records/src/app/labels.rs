//! A label's page (C56): its records by year, newest first, each had or not, with its catalogue number and its request, from what
//! the catalogue keeps and from MusicBrainz's and Discogs' lists of it. Never Bandcamp's: its `/music` pages are no longer read
//! (O4).
//!
//! - **A cache, not records** (Q33, the owner): the sources' rows are kept thirty days (`catalogue::labels`; RT01's daily pass
//!   lets them go), then read again at the next open. No record or artist is made of a label's list. A row opened is made a
//!   record (`open_record`, "kept at open"), by its reference, never by its names.
//! - **A hundred at a time** (OQ10 (a)): at open, the first hundred of each source's list not read in thirty days is read in the
//!   background while the page answers what is kept (`looking`). "More" reads the next hundred of one source. One read of a
//!   label's list at a source at a time.
//! - **Where it is known:** its MusicBrainz and Discogs ids are its references; else MusicBrainz's search finds it by its name,
//!   and Discogs' id is MusicBrainz's link to it. What is found is kept as its references.
//! - **What you have, apart (F10):** the page answers what the house has of the label (`had`: the one had rule, in any library)
//!   apart from the rest (`not_had`); a source's row no record holds goes with the had record its names point at (Z6), so a had
//!   album is never among the rest for want of a reference. While a list is read, `reading` names its source; after, `no_list`
//!   says which could not be had, in F1's words.
//! - **Labels About shows (F9):** every one opens its page where it has a reference (`LabelOf`): the label holding its Discogs id
//!   or MBID, else at the tap (`open_label`) the one label of its name that holds none of that source's (PB3: one real label, one
//!   page), else one made with it; never by a name alone; one known by its name alone opens the one label of that name, where exactly
//!   one has it (X8).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::catalogue::Catalogue;
use crate::catalogue::labels::{LabelKept, LabelListed, ROWS_KEPT};
use crate::catalogue::records::{Listing, Ref, Requested};
use crate::serve::ids;

/// What a source finds of a label (C56): the references to keep, each with how it was found (`search`, `link`).
pub type Found = Vec<(Ref, &'static str)>;

/// A page of a label's list at a source: its rows, and where the next page starts (none: its end).
pub type Page = (Vec<LabelListed>, Option<i64>);

/// One of the sources a label's list is read from (C56): MusicBrainz and Discogs (`sources`); the tests' stand in for them.
pub trait LabelList: Send + Sync {
    /// Its name, as `references.source` has it.
    fn source(&self) -> &'static str;
    /// Whether it is switched on: one switched off is not read.
    fn on(&self) -> bool {
        true
    }
    /// The label found there, by its name or through what it is known by already (`refs`): the references to keep, its own among
    /// them where it knows the label.
    fn find<'a>(&'a self, name: &'a str, refs: &'a [Ref]) -> crate::source::Fut<'a, Found>;
    /// A page of its list of the label it knows as `id`, from `from` (none: its first).
    fn page<'a>(&'a self, id: &'a str, from: Option<i64>) -> crate::source::Fut<'a, Page>;
}

/// MusicBrainz's list of a label: its releases, each release group once (`discover::mb_label_releases`), a hundred releases from
/// an offset, a page's question in MusicBrainz's turn; the label by its id there, else MusicBrainz's search for its name.
struct MusicBrainzLabel;

impl LabelList for MusicBrainzLabel {
    fn source(&self) -> &'static str {
        "musicbrainz"
    }

    fn on(&self) -> bool {
        super::switches::is_on(super::switches::MUSICBRAINZ)
    }

    fn find<'a>(&'a self, name: &'a str, _: &'a [Ref]) -> crate::source::Fut<'a, Found> {
        Box::pin(async move {
            Ok(match super::discover::mb_label_by_name(&super::discover::http()?, name).await? {
                Some(id) => vec![(Ref::new("musicbrainz", "label", &id), "search")],
                None => Vec::new(),
            })
        })
    }

    fn page<'a>(&'a self, id: &'a str, from: Option<i64>) -> crate::source::Fut<'a, Page> {
        Box::pin(async move {
            let offset = usize::try_from(from.unwrap_or(0)).unwrap_or(0);
            let (listings, next) = super::discover::mb_label_releases(&super::discover::http()?, id, offset).await?;
            Ok((listings.into_iter().filter_map(listed_of_musicbrainz).collect(), next.and_then(|n| i64::try_from(n).ok())))
        })
    }
}

/// A row of MusicBrainz's list as kept: by its release group, with no catalogue number.
pub(super) fn listed_of_musicbrainz(l: Listing) -> Option<LabelListed> {
    let reference = l.refs.into_iter().next()?;
    Some(LabelListed { reference, title: l.title, artist: l.artist, year: l.year, catno: String::new(), kind: l.kind, cover: None, also: l.says })
}

/// Discogs' list of a label: its releases as its label page lists them (`discogs::label_releases`), a page of a hundred, newest
/// first; the label by its id there, else MusicBrainz's link to it (never a search by name). With MusicBrainz switched off and no
/// id kept, it cannot be found: the read fails, and the label is not taken for unknown there.
struct DiscogsLabel;

impl LabelList for DiscogsLabel {
    fn source(&self) -> &'static str {
        "discogs"
    }

    fn on(&self) -> bool {
        super::switches::is_on(super::switches::DISCOGS)
    }

    fn find<'a>(&'a self, name: &'a str, refs: &'a [Ref]) -> crate::source::Fut<'a, Found> {
        Box::pin(async move {
            let mut found = Vec::new();
            let mbid = match refs.iter().find(|r| r.source == "musicbrainz" && r.kind == "label") {
                Some(r) => r.value.clone(),
                None => match super::discover::mb_label_by_name(&super::discover::http()?, name).await? {
                    Some(id) => {
                        found.push((Ref::new("musicbrainz", "label", &id), "search"));
                        id
                    }
                    None => return Ok(found),
                },
            };
            if let Some(id) = super::discogs::label_linked(&mbid).await? {
                found.push((Ref::new("discogs", "label", &id.to_string()), "link"));
            }
            Ok(found)
        })
    }

    fn page<'a>(&'a self, id: &'a str, from: Option<i64>) -> crate::source::Fut<'a, Page> {
        Box::pin(async move {
            let Ok(id) = id.parse::<u64>() else { return Ok((Vec::new(), None)) };
            let page = u32::try_from(from.unwrap_or(1)).unwrap_or(1).max(1);
            Ok(match super::discogs::label_releases(id, page).await? {
                Some((listed, pages)) => (listed.iter().map(listed_of_discogs).collect(), (page < pages).then(|| i64::from(page) + 1)),
                None => (Vec::new(), None),
            })
        })
    }
}

/// A row of Discogs' list as kept: by its master, or the release Discogs gives no master of; typed by what its format says.
fn listed_of_discogs(l: &super::discogs::Listed) -> LabelListed {
    LabelListed {
        reference: Ref::new("discogs", if l.master { "master" } else { "release" }, &l.id.to_string()),
        title: l.title.clone(),
        artist: l.artist.clone(),
        year: l.year,
        catno: l.catno.clone().unwrap_or_default(),
        kind: crate::serve::library::release_words(l.format.as_deref().unwrap_or_default().split(',')).0,
        cover: l.thumb.clone(),
        also: Vec::new(),
    }
}

/// The two sources a label's list is read from (C56): MusicBrainz and Discogs, never Bandcamp.
pub fn sources() -> Vec<Arc<dyn LabelList>> {
    vec![Arc::new(MusicBrainzLabel), Arc::new(DiscogsLabel)]
}

/// A source as "More" names it (`?more=`), where it is one of a label's lists.
pub fn source_named(name: &str) -> Option<&'static str> {
    ["musicbrainz", "discogs"].into_iter().find(|s| *s == name)
}

/// A label's page (C56): `GET /api/labels/{id}`.
#[derive(Debug, Clone, Serialize)]
pub struct LabelPage {
    /// Its id for life (`la-<n>`).
    pub id: String,
    pub name: String,
    /// What the house has of it (F10): its records had by the one had rule, in any library (Music, anyone's own music, Incoming),
    /// from what is kept, at once; by year, newest first (none last), then by title.
    pub had: Vec<LabelEntry>,
    /// The rest (F10): its records not had and its sources' rows no record holds, with their catalogue numbers and requests; by year
    /// as `had`.
    pub not_had: Vec<LabelEntry>,
    /// Whether a list of it is being read: the page asks again until none is.
    pub looking: bool,
    /// While `looking`: the source whose list is being read now, the first in their order (F10, as F8's sections).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading: Option<&'static str>,
    /// Once none is read: each source whose list could not be had (F10, F1's words: `off`, switched off with its list not kept;
    /// `unanswered`, failed within the quarter hour), in their order; absent while `looking`, and where every source answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_list: Option<Vec<super::references::Said>>,
    /// Whether each source's list reads on (`?more=<source>`).
    pub more: More,
    /// Its ids at MusicBrainz and Discogs, where known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub musicbrainz: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discogs: Option<String>,
}

/// Whether each source's list of a label reads on (OQ10 (a)).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct More {
    pub musicbrainz: bool,
    pub discogs: bool,
}

/// A row of a label's page (C56): a record, or a source's row no record holds yet.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LabelEntry {
    pub title: String,
    pub artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    /// Its catalogue number on the label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catno: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Its cover's address: its holder's where it is had, else its record's, else the source's picture or its release group's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    /// What the sources know it by: what `POST /api/records/open` takes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discogs_master: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discogs_release: Option<u64>,
    /// The record that holds it (`al-<n>`), as merged now; none until a row is opened (Q33).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<String>,
    /// Whether the library has it: the one had rule's holder.
    pub had: bool,
    /// An open request's mark for its record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<super::requests_base::Mark>,
    /// How many editions of one album its record's row stands for, its own among them (F12.1, `app::editions`); none: one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editions: Option<u32>,
}

impl LabelEntry {
    /// A record's row: its own facts and references, had by the one had rule.
    fn of_record(q: &Requested) -> Self {
        let group = q.groups.first().cloned();
        let discogs = |kind: &str| q.refs.iter().find(|(s, k, _)| s == "discogs" && k == kind).and_then(|(.., v)| v.parse::<u64>().ok());
        // F16.1: the record's own address (had: the record that holds it), which tries each of its pictures.
        let cover = Some(super::cover_cache::record_address(q.holder.unwrap_or(q.row.id)));
        LabelEntry {
            title: q.row.title.clone(),
            artist: q.row.artist.clone(),
            year: q.row.year,
            catno: None,
            kind: q.row.kind.clone(),
            cover,
            discogs_master: discogs("master"),
            discogs_release: discogs("release"),
            release_group: group,
            record: Some(ids::record(q.row.id)),
            had: q.holder.is_some(),
            request: None,
            editions: None,
        }
    }

    /// A source's row no record holds yet: not had. Its artist as the page names them, without Discogs' namesake mark.
    fn of_row(l: &LabelListed) -> Self {
        let mut e = LabelEntry { title: l.title.clone(), artist: super::discogs::clean_name(&l.artist), year: l.year, kind: l.kind.clone(), ..LabelEntry::default() };
        e.fill(l);
        e.cover = l.cover.clone().or_else(|| e.release_group.as_ref().map(|g| format!("/api/covers/release-group/{g}")));
        e
    }

    /// What a source's row of the same album adds: its catalogue number beside the others (A2.3: every one), its references where
    /// the row has none of their kind (its own, and the album's elsewhere its source names), and the facts the record lacks.
    fn fill(&mut self, l: &LabelListed) {
        add_catno(&mut self.catno, &l.catno);
        for r in std::iter::once(&l.reference).chain(l.also.iter().filter(|r| r.kind != "artist")) {
            match (r.source.as_str(), r.kind.as_str()) {
                ("musicbrainz", "release-group") if self.release_group.is_none() => self.release_group = Some(r.value.clone()),
                ("discogs", "master") if self.discogs_master.is_none() => self.discogs_master = r.value.parse().ok(),
                ("discogs", "release") if self.discogs_release.is_none() => self.discogs_release = r.value.parse().ok(),
                _ => {}
            }
        }
        if self.year.is_none() {
            self.year = l.year;
        }
        if self.kind.is_none() {
            self.kind = l.kind.clone();
        }
        if self.artist.is_empty() {
            self.artist = super::discogs::clean_name(&l.artist);
        }
    }

    /// The album references it holds, as (source, kind, value): what tells it from another album of its name (A2.3).
    fn album_refs(&self) -> Vec<(String, String, String)> {
        let group = self.release_group.iter().map(|g| ("musicbrainz".to_string(), "release-group".to_string(), g.to_ascii_lowercase()));
        let master = self.discogs_master.iter().map(|m| ("discogs".to_string(), "master".to_string(), m.to_string()));
        let release = self.discogs_release.iter().map(|r| ("discogs".to_string(), "release".to_string(), r.to_string()));
        group.chain(master).chain(release).collect()
    }
}

/// A row's album references, as (source, kind, value): its own and the album's elsewhere its source names (A2.3).
fn row_refs(l: &LabelListed) -> Vec<(String, String, String)> {
    std::iter::once(&l.reference).chain(l.also.iter().filter(|r| r.kind != "artist")).map(|r| (r.source.clone(), r.kind.clone(), r.value.to_ascii_lowercase())).collect()
}

/// Whether two rows' references say two albums (A2.3, X8): two release groups, or two Discogs masters. Discogs releases are editions:
/// two of them say nothing.
fn two_albums(a: &[(String, String, String)], b: &[(String, String, String)]) -> bool {
    [("musicbrainz", "release-group"), ("discogs", "master")].iter().any(|(s, k)| {
        let of = |x: &[(String, String, String)]| x.iter().filter(|(xs, xk, _)| xs == s && xk == k).map(|(.., v)| v.clone()).collect::<Vec<_>>();
        let (x, y) = (of(a), of(b));
        !x.is_empty() && !y.is_empty() && !x.iter().any(|v| y.contains(v))
    })
}

/// A label's rows from what the catalogue keeps (C56, `LabelKept`): each record once, as merged now, its catalogue number the
/// first its ties give; a source's row of a record (by its reference) is that record's row, as is one no record holds whose names
/// point at a had record (`by_names`, in the rows' order, those records read in `holders` where `kept` has them not: F10), and any
/// other is a row of its own. By year, newest first, then by title and artist.
fn rows_of(kept: &LabelKept, by_names: &[Option<i64>], holders: &HashMap<i64, Requested>) -> Vec<LabelEntry> {
    let mut by_record: BTreeMap<i64, LabelEntry> = BTreeMap::new();
    for (record, catno) in &kept.tied {
        let Some(q) = kept.records.get(record) else { continue };
        let row = by_record.entry(q.row.id).or_insert_with(|| LabelEntry::of_record(q));
        if row.catno.is_none() && !catno.is_empty() {
            row.catno = Some(catno.clone());
        }
    }
    let mut loose_rows: Vec<&LabelListed> = Vec::new();
    for (i, (listed, record)) in kept.rows.iter().enumerate() {
        let named = || by_names.get(i).copied().flatten().and_then(|h| kept.records.get(&h).or_else(|| holders.get(&h)));
        match record.and_then(|r| kept.records.get(&r)).or_else(named) {
            Some(q) => by_record.entry(q.row.id).or_insert_with(|| LabelEntry::of_record(q)).fill(listed),
            None => loose_rows.push(listed),
        }
    }
    // A2.3: one album, one row. A source's row no record holds is the row of another of its album: one it shares a reference with
    // (MusicBrainz's link to the Discogs release), else the one of its artist and title whose references say no second album (two
    // release groups, two masters), where the name is one album's alone (X8). A record is never another record's row.
    let key = |artist: &str, title: &str| (crate::serve::ids::artist_key(artist), crate::catalogue::identity::title_key(title));
    let mut entries: Vec<(LabelEntry, Vec<(String, String, String)>)> = by_record.into_values().map(|e| {
        let refs = e.album_refs();
        (e, refs)
    }).collect();
    for listed in loose_rows {
        let mine = row_refs(listed);
        let named = key(&super::discogs::clean_name(&listed.artist), &listed.title);
        let shared = entries.iter().position(|(_, refs)| refs.iter().any(|r| mine.contains(r)));
        let at = shared.or_else(|| {
            let of_name: Vec<usize> = entries.iter().enumerate().filter(|(_, (e, refs))| key(&e.artist, &e.title) == named && !two_albums(refs, &mine)).map(|(i, _)| i).collect();
            match of_name[..] {
                [one] if !named.0.is_empty() && !named.1.is_empty() => Some(one),
                _ => None,
            }
        });
        match at {
            Some(i) => {
                entries[i].0.fill(listed);
                entries[i].1.extend(mine);
            }
            None => entries.push((LabelEntry::of_row(listed), mine)),
        }
    }
    let mut rows: Vec<LabelEntry> = entries.into_iter().map(|(e, _)| e).collect();
    rows.sort_by(|a, b| b.year.cmp(&a.year).then_with(|| a.title.cmp(&b.title)).then_with(|| a.artist.cmp(&b.artist)));
    rows
}

/// A label's rows as its page shows them (F10): what the house has of it by the one had rule (in any library: Music, anyone's own
/// music, Incoming), then the rest, each by year as `rows_of` sorts them. A source's row no record holds is the had record its names
/// point at, where they point at one (the had rule's names clause, Z6: for display alone, nothing made or tied), so a had album is
/// never among the rest for want of a reference. Blocking.
fn split(cat: &Catalogue, kept: &LabelKept) -> Result<(Vec<LabelEntry>, Vec<LabelEntry>)> {
    let loose: Vec<usize> = (0..kept.rows.len()).filter(|&i| kept.rows[i].1.is_none()).collect();
    let mut by_names: Vec<Option<i64>> = vec![None; kept.rows.len()];
    let mut holders: HashMap<i64, Requested> = HashMap::new();
    if !loose.is_empty() {
        let asked: Vec<(Ref, &str, &str)> = loose.iter().map(|&i| (kept.rows[i].0.reference.clone(), kept.rows[i].0.artist.as_str(), kept.rows[i].0.title.as_str())).collect();
        for (&i, held) in loose.iter().zip(cat.by_reference(&asked, &[])?.named) {
            by_names[i] = held;
        }
        let wanted: Vec<i64> = by_names.iter().flatten().copied().filter(|h| !kept.records.contains_key(h)).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        if !wanted.is_empty() {
            holders = cat.requested(&wanted)?;
        }
    }
    Ok(rows_of(kept, &by_names, &holders).into_iter().partition(|r| r.had))
}

/// Why a source's list of a label is not had now (F10, as F8's sections, in F1's words), where it is not being read: switched off
/// with its list not kept (`off`); failed within `RETRY_SECS` (`unanswered`, asked again after it). None where it answered.
fn unread(cat: &Arc<Catalogue>, label: i64, source: &'static str, kept: bool, on: bool, now: i64) -> Option<super::references::Said> {
    let said = if on {
        let failed = readings().lock().unwrap_or_else(|p| p.into_inner()).get(&(Arc::as_ptr(cat) as usize, label, source)).copied();
        matches!(failed, Some(Reading::Failed(at)) if now - at <= RETRY_SECS).then_some("unanswered")
    } else {
        (!kept).then_some("off")
    };
    said.map(|said| super::references::Said { source, said, until: None })
}

/// A label as its lists are read: its id and name.
struct LabelAsked {
    id: i64,
    name: String,
}

/// A label's list at a source being read now, or failed at a time; a read done is kept in the catalogue (`label_reads`).
#[derive(Debug, Clone, Copy)]
enum Reading {
    Now,
    Failed(i64),
}

/// Who is being read where: by catalogue (its address), label and source.
type Readings = std::sync::Mutex<HashMap<(usize, i64, &'static str), Reading>>;

fn readings() -> &'static Readings {
    static READINGS: std::sync::OnceLock<Readings> = std::sync::OnceLock::new();
    READINGS.get_or_init(Default::default)
}

/// How long a source that did not answer is left before it is asked again, by an open or by "More" (Z8).
const RETRY_SECS: i64 = 15 * 60;
/// The longest a page of a label's list may take: past it, it counts as not read, and `looking` ends.
const READ_AT_MOST: Duration = Duration::from_secs(if cfg!(any(test, feature = "test-support")) { 5 } else { 300 });

/// Ends a read, whatever becomes of its task: done (kept in the catalogue), else failed now (an error, a panic, a task dropped).
struct Settles(Option<(usize, i64, &'static str)>, bool);

impl Drop for Settles {
    fn drop(&mut self) {
        if let Some(key) = self.0.take() {
            let mut all = readings().lock().unwrap_or_else(|p| p.into_inner());
            if self.1 {
                all.remove(&key);
            } else {
                all.insert(key, Reading::Failed(super::moment::now_secs()));
            }
        }
    }
}

/// Whether a label's list at a source is being read now.
fn being_read(cat: &Arc<Catalogue>, label: i64, source: &'static str) -> bool {
    matches!(readings().lock().unwrap_or_else(|p| p.into_inner()).get(&(Arc::as_ptr(cat) as usize, label, source)), Some(Reading::Now))
}

/// A label's finding at its sources, one at a time (by catalogue and label): two sources' reads ask MusicBrainz for its name once.
fn finding(cat: &Arc<Catalogue>, label: i64) -> Arc<tokio::sync::Mutex<()>> {
    type Findings = std::sync::Mutex<HashMap<(usize, i64), Arc<tokio::sync::Mutex<()>>>>;
    static FINDING: std::sync::OnceLock<Findings> = std::sync::OnceLock::new();
    FINDING.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner()).entry((Arc::as_ptr(cat) as usize, label)).or_default().clone()
}

/// A label's id at `source` among these references.
fn label_id_at<'r>(refs: impl IntoIterator<Item = &'r Ref>, source: &str) -> Option<String> {
    refs.into_iter().find(|r| r.source == source && r.kind == "label").map(|r| r.value.clone())
}

/// What `source` knows the label by (C56): its reference there, else found now (`LabelList::find`) and kept as the label's, one
/// finding at a time. None: the source does not know it.
async fn known_at(cat: &Arc<Catalogue>, label: &LabelAsked, source: &dyn LabelList) -> Result<Option<String>> {
    let one = finding(cat, label.id);
    let _one = one.lock().await;
    let (c, subject) = (cat.clone(), ids::label(label.id));
    let refs: Vec<Ref> = crate::run::spawn_blocking(move || c.references_of(&subject)).await??.into_iter().map(|(r, _)| r).collect();
    if let Some(id) = label_id_at(&refs, source.source()) {
        return Ok(Some(id));
    }
    let found = source.find(&label.name, &refs).await?;
    let (c, subject, source_name) = (cat.clone(), ids::label(label.id), source.source());
    // A2.4.3: only an id the label could keep is read by: one another label holds is theirs, and nothing is read or kept under this
    // one from that source (the log names the holder).
    crate::run::spawn_blocking(move || -> Result<Option<String>> {
        let mut id = None;
        for (r, how) in &found {
            let kept = c.set_reference(&subject, r, how)?;
            if r.source == source_name && r.kind == "label" {
                if kept {
                    id = Some(r.value.clone());
                } else {
                    let holder = c.subject_of(r)?.into_iter().find(|s| *s != subject).unwrap_or_default();
                    tracing::info!(target: "discover", label = %subject, holder = %holder, source = source_name, "a label's list not read: its id there is another label's");
                }
            }
        }
        Ok(id)
    })
    .await?
}

/// A page of the label's list at `source` read and kept (C56): from its start (`from` none), where its list then starts again,
/// or from where it stands. A source that does not know the label keeps that, so it is not asked again for thirty days.
async fn read_page(cat: &Arc<Catalogue>, label: &LabelAsked, source: &dyn LabelList, from: Option<i64>) -> Result<()> {
    let (rows, next) = match known_at(cat, label, source).await? {
        Some(id) => source.page(&id, from).await?,
        None => (Vec::new(), None),
    };
    let (c, id, name, at) = (cat.clone(), label.id, source.source(), super::moment::now_secs());
    crate::run::spawn_blocking(move || c.keep_label_rows(id, name, &rows, next, from.is_none(), at)).await?
}

/// A page of the label's list at `source` read in a task of its own (C56), unless one is being read (one at a time) or one failed
/// within `RETRY_SECS`: "More" waits too (Z8), so it cannot ask a source over and over. Whether one is being read now.
fn start_read(cat: &Arc<Catalogue>, label: &Arc<LabelAsked>, source: &Arc<dyn LabelList>, from: Option<i64>, now: i64) -> bool {
    let key = (Arc::as_ptr(cat) as usize, label.id, source.source());
    {
        let mut all = readings().lock().unwrap_or_else(|p| p.into_inner());
        match all.get(&key) {
            Some(Reading::Now) => return true,
            Some(Reading::Failed(at)) if now - at <= RETRY_SECS => return false,
            _ => {}
        }
        all.insert(key, Reading::Now);
    }
    let (cat, label, source) = (cat.clone(), label.clone(), source.clone());
    crate::run::spawn(async move {
        let mut settles = Settles(Some(key), false);
        match tokio::time::timeout(READ_AT_MOST, read_page(&cat, &label, source.as_ref(), from)).await {
            Ok(Ok(())) => settles.1 = true,
            Ok(Err(e)) => tracing::info!(target: "discover", label = label.id, source = source.source(), "a label's list not read: {e:#}"),
            Err(_) => tracing::info!(target: "discover", label = label.id, source = source.source(), "a label's list not read: no answer in time"),
        }
    });
    true
}

/// A label's page (C56, F10): what the house has of it (`had`, the one had rule) apart from the rest (`not_had`), from what is kept,
/// at once (`split`), while the first hundred of each source's list not read in thirty days (`ROWS_KEPT`), or never, is read in the
/// background; with `more`, the next hundred of that source's (OQ10 (a)). Whether a list of it is being read (`looking`), so the page
/// asks again until none is, and from which source (`reading`); once none is, why a source's list is not had (`no_list`, `unread`);
/// whether each source's reads on. None: no such label.
pub async fn page(cat: Arc<Catalogue>, label: i64, more: Option<&str>, sources: &[Arc<dyn LabelList>]) -> Result<Option<LabelPage>> {
    let c = cat.clone();
    let Some(kept) = crate::run::spawn_blocking(move || c.label_kept(label)).await?? else { return Ok(None) };
    let asked = Arc::new(LabelAsked { id: label, name: kept.label.name.clone() });
    let now = super::moment::now_secs();
    let (mut looking, mut reading, mut no_list) = (false, None, Vec::new());
    for source in sources {
        let name = source.source();
        let fresh = kept.reads.get(name).filter(|r| now - r.at <= ROWS_KEPT);
        if !source.on() {
            no_list.extend(unread(&cat, label, name, fresh.is_some(), false, now));
            continue;
        }
        let by_hand = more == Some(name);
        // Never read, or thirty days ago: from its start. Read since: on from where it stands, for "More" alone.
        let from = match fresh {
            None => Some(None),
            Some(r) if by_hand => r.next.map(Some),
            Some(_) => None,
        };
        let now_reading = match from {
            Some(from) => start_read(&cat, &asked, source, from, now),
            None => being_read(&cat, label, name),
        };
        looking |= now_reading;
        if now_reading {
            reading.get_or_insert(name);
        } else {
            no_list.extend(unread(&cat, label, name, fresh.is_some(), true, now));
        }
    }
    // Its rows read once the reads' states are known: a read that ended before is in them; one ending now is still `looking`.
    let c = cat.clone();
    let read = crate::run::spawn_blocking(move || -> Result<Option<(LabelKept, Vec<LabelEntry>, Vec<LabelEntry>)>> {
        let Some(kept) = c.label_kept(label)? else { return Ok(None) };
        let (had, not_had) = split(&c, &kept)?;
        Ok(Some((kept, had, not_had)))
    })
    .await??;
    let Some((kept, had, not_had)) = read else { return Ok(None) };
    let reads_on = |name: &str| sources.iter().any(|s| s.source() == name && s.on()) && kept.reads.get(name).is_some_and(|r| r.next.is_some());
    Ok(Some(LabelPage {
        id: ids::label(label),
        name: kept.label.name.clone(),
        had,
        not_had,
        looking,
        reading,
        no_list: (!looking && !no_list.is_empty()).then_some(no_list),
        more: More { musicbrainz: reads_on("musicbrainz"), discogs: reads_on("discogs") },
        musicbrainz: label_id_at(&kept.refs, "musicbrainz"),
        discogs: label_id_at(&kept.refs, "discogs"),
    }))
}

/// The longest title or artist a row opened, or an artist opened, may have (Z8): longer, it is no album's or artist's.
pub const NAME_MOST: usize = 500;

/// A row of a list opened (C56, Q33's "kept at open"; C57's Beyond too): what names its album at its sources, what the row says
/// of it, and the label whose page it was opened from, where it was.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Opened {
    #[serde(default)]
    pub release_group: Option<String>,
    #[serde(default)]
    pub discogs_master: Option<u64>,
    #[serde(default)]
    pub discogs_release: Option<u64>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub year: Option<i32>,
    /// The label's id (`la-<n>`) where it was opened from its page: the record is tied to it.
    #[serde(default)]
    pub label: Option<String>,
}

impl Opened {
    /// What it names at its sources: the one a source gives one album alone first (a Discogs release), then its release group (a
    /// MusicBrainz id, as MusicBrainz writes it) and its master.
    fn refs(&self) -> Vec<Ref> {
        let group = self.release_group.as_deref().map(|g| g.trim().to_ascii_lowercase()).filter(|g| super::discogs::is_mbid(g));
        self.discogs_release
            .map(|r| Ref::new("discogs", "release", &r.to_string()))
            .into_iter()
            .chain(group.map(|g| Ref::new("musicbrainz", "release-group", &g)))
            .chain(self.discogs_master.map(|m| Ref::new("discogs", "master", &m.to_string())))
            .collect()
    }
}

/// The record a row opened is (C56, Q33: "kept at open"): the one that holds its reference, as merged now, else one made now with
/// it (`Catalogue::keep_listing`: seen now, RT01's rules from then on), what a source's kept row says of it filling what the row
/// does not (its type, its cover). Tied to the label it was opened from only where that label's own kept row lists it (Z8, U10): a
/// real open from a label's page always has one, so an album found otherwise, or made up by any account (made now from ids no
/// label listed), is not tied to a label by asking. Never by its names alone: a row with no reference, or no title, is none; a title
/// or an artist longer than `NAME_MOST` is refused. Blocking.
pub fn open_record(cat: &Catalogue, o: &Opened) -> Result<Option<i64>> {
    let refs = o.refs();
    let label = o.label.as_deref().and_then(ids::label_number);
    let mut listed = None;
    for r in &refs {
        if let Some(found) = cat.label_row_of(r, label)? {
            listed = Some(found);
            break;
        }
    }
    let said = |own: &str, kept: Option<&String>| Some(own.trim()).filter(|s| !s.is_empty()).map(str::to_string).or_else(|| kept.cloned()).unwrap_or_default();
    let title = said(&o.title, listed.as_ref().map(|(_, l)| &l.title));
    if refs.is_empty() || title.is_empty() {
        return Ok(None);
    }
    let artist = said(&o.artist, listed.as_ref().map(|(_, l)| &l.artist));
    if title.chars().count() > NAME_MOST || artist.chars().count() > NAME_MOST {
        anyhow::bail!("A title or an artist that long is no album's.");
    }
    // RG5 (X8 across the whole list): judged with the other rows of its list, a label's page's kept rows (`Catalogue::keep_opened`)
    // or Beyond's as it listed them (`beyond::listed_others`).
    let listed_others = if label.is_some() { 0 } else { super::beyond::listed_others(&refs, super::moment::now_secs()) };
    let listing = Listing {
        artist,
        year: o.year.or(listed.as_ref().and_then(|(_, l)| l.year)),
        kind: listed.as_ref().and_then(|(_, l)| l.kind.clone()),
        cover: listed.as_ref().and_then(|(_, l)| l.cover.clone()),
        title,
        refs,
        ..Listing::default()
    };
    // F18.1 (e): a name Discogs marks as another's of the same name ("Glass Harbour (2)") is told by no name: it joins nothing by it
    // (X8) and credits nobody by it; the record shows the name without the mark.
    let raw = listed.as_ref().map(|(_, l)| l.artist.clone()).filter(|a| !a.trim().is_empty()).unwrap_or_else(|| listing.artist.clone());
    let marked = super::discogs::clean_name(&raw) != raw.trim();
    let artist_ref = listed.as_ref().and_then(|(_, l)| l.also.iter().find(|r| r.kind == "artist").cloned());
    let listing = Listing { artist: super::discogs::clean_name(&listing.artist), ..listing };
    let Some(record) = cat.keep_opened(&listing, label, if marked { 2 } else { listed_others })? else { return Ok(None) };
    // F18.1: opened from a label's page, credited at once, by the artist reference its row names, else by its name where one artist
    // has it. Beyond's opens credit nobody (F2.8, Z3) until its release is read (F18.2).
    if label.is_some() {
        cat.credit_opened(record, artist_ref.as_ref(), &listing.artist, !marked)?;
    }
    if let Some(label) = label {
        // Through this label's own kept row alone, with its catalogue number there (U10).
        if let Some(catno) = listed.as_ref().filter(|(of, _)| *of == label).map(|(_, l)| l.catno.as_str()) {
            cat.tie_label(record, label, catno)?;
        }
    }
    Ok(Some(record))
}

// ---- F9: every label About shows opens its page where it has a reference

/// What opens a label's page where no label is kept for it yet (F9): its reference at a source, Discogs' id or MusicBrainz's MBID,
/// which `POST /api/labels/open` finds or makes the label by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LabelRef {
    pub source: &'static str,
    pub id: String,
}

/// A label as About shows it (F9; every answer that names an album's labels: the album's `labels`, its Discogs answer's `labels`):
/// its name and the album's catalogue number there; its id for life (`la-<n>`) where a label is kept for it, else its reference
/// (`ref`) for `POST /api/labels/open`; neither, text: no reference, and a name two labels share, or none has.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LabelOf {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catno: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<LabelRef>,
}

/// A label as a source or a page names it (F9): its name, the album's catalogue number there, and its reference there where it
/// gives one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LabelNamed {
    pub name: String,
    pub catno: Option<String>,
    pub reference: Option<Ref>,
}

/// Discogs' "Not On Label" (self-released): no label, as the crates read it (`discogs_facts::label_names`), so shown as text.
fn no_label(name: &str) -> bool {
    name.trim().to_lowercase().starts_with("not on label")
}

/// Labels as About shows them (F9), in their order, on one reader (`Catalogue::labels_shown`): one with a reference, the label that
/// holds it (`id`), else the reference itself (`ref`), never a label of its name; one with none, the one label of its name where
/// exactly one has it (X8); "Not On Label" text. Nothing is made. Blocking.
pub fn shown(cat: &Catalogue, named: &[LabelNamed]) -> Result<Vec<LabelOf>> {
    let asked: Vec<(Option<Ref>, String)> =
        named.iter().map(|n| if no_label(&n.name) { (None, String::new()) } else { (n.reference.clone(), n.name.trim().to_string()) }).collect();
    let known = cat.labels_shown(&asked)?;
    Ok(named
        .iter()
        .zip(asked)
        .zip(known)
        .map(|((n, (r, _)), id)| LabelOf {
            name: n.name.trim().to_string(),
            catno: n.catno.as_deref().map(str::trim).filter(|c| !c.is_empty()).map(str::to_string),
            reference: r.filter(|_| id.is_none()).and_then(|r| Some(LabelRef { source: source_named(&r.source)?, id: r.value })),
            id: id.map(ids::label),
        })
        .collect())
}

/// A page elsewhere's label (Bandcamp's `label`), as About shows it (F9): by its name alone, the one label of its name where exactly
/// one has it. Blocking.
pub fn of_text(cat: &Catalogue, text: &str) -> Result<Vec<LabelOf>> {
    let name = text.trim();
    if name.is_empty() {
        return Ok(Vec::new());
    }
    shown(cat, &[LabelNamed { name: name.to_string(), ..LabelNamed::default() }])
}

/// An album's labels at Discogs and MusicBrainz as About shows them (F9): Discogs' main release's, each by its id there (facts kept
/// before C46 have none: by its name alone), then those of the files' release at MusicBrainz, by their MBIDs, of a name Discogs
/// names none of. Blocking.
pub fn about(cat: &Catalogue, discogs: &[super::discogs::Label], musicbrainz: &[super::discogs::MbLabel]) -> Result<Vec<LabelOf>> {
    use crate::catalogue::labels::bare_key;
    // One a label (A2.5): Discogs names a label once per catalogue number; by its id, else by its bare name, an entry with no id
    // going with the one of its bare name that has one. Two ids are two labels.
    let mut one: Vec<super::discogs::Label> = Vec::new();
    for l in discogs {
        let same = |m: &super::discogs::Label| match (m.id, l.id) {
            (Some(a), Some(b)) => a == b,
            _ => bare_key(&m.name) == bare_key(&l.name),
        };
        match one.iter_mut().find(|m| same(m)) {
            Some(m) => {
                add_catno(&mut m.catno, l.catno.as_deref().unwrap_or_default());
                m.id = m.id.or(l.id);
            }
            None => one.push(l.clone()),
        }
    }
    let at_discogs: std::collections::HashSet<String> = one.iter().map(|l| bare_key(&l.name)).collect();
    let named: Vec<LabelNamed> = one
        .iter()
        .map(|l| LabelNamed { name: l.name.clone(), catno: l.catno.clone(), reference: l.id.map(|id| Ref::new("discogs", "label", &id.to_string())) })
        .chain(
            musicbrainz
                .iter()
                .filter(|l| !at_discogs.contains(&bare_key(&l.name)))
                .map(|l| LabelNamed { name: l.name.clone(), catno: l.catno.clone(), reference: Some(Ref::new("musicbrainz", "label", &l.mbid)) }),
        )
        .collect();
    shown(cat, &named)
}

/// A catalogue number added to a label's shown ones (A2.5): after them, ", " between, each once; an empty one adds nothing.
pub fn add_catno(shown: &mut Option<String>, catno: &str) {
    let catno = catno.trim();
    if catno.is_empty() || shown.as_deref().is_some_and(|s| s.split(", ").any(|c| c == catno)) {
        return;
    }
    *shown = Some(match shown.take() {
        Some(s) if !s.is_empty() => format!("{s}, {catno}"),
        _ => catno.to_string(),
    });
}

/// A label About shows, opened (F9, `POST /api/labels/open`): its reference at a source, and its name as shown.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LabelOpened {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
}

/// What a label opened came to (F9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelOpen {
    /// The label: found by its reference, else the one label of its name given it, else made now with it.
    Opened(i64),
    /// No reference a label is opened by (Discogs' id, a number; MusicBrainz's MBID): a name alone opens none.
    NoReference,
    /// No label holds it, and it is no reference an album's labels were kept with, by that name, for one who is no administrator (as
    /// Z8's artists).
    NotNamed,
}

/// The reference a label is opened by (F9): Discogs' id, MusicBrainz's MBID as MusicBrainz writes it; none else.
fn label_reference(source: &str, id: &str) -> Option<Ref> {
    let id = id.trim();
    match source.trim() {
        "discogs" => id.parse::<u64>().ok().filter(|n| *n > 0).map(|n| Ref::new("discogs", "label", &n.to_string())),
        "musicbrainz" => Some(id.to_ascii_lowercase()).filter(|m| super::discogs::is_mbid(m)).map(|m| Ref::new("musicbrainz", "label", &m)),
        _ => None,
    }
}

/// A label About shows, opened (F9): the label that holds its reference, found by anyone; else the one label of its name given it, or
/// one made now with it (`Catalogue::open_label`), only where the reference is one an album's labels were kept with (`named`: its name
/// then), by that name, or by an administrator, by the name given; never by a name alone. Blocking.
pub fn open_label(cat: &Catalogue, o: &LabelOpened, named: impl FnOnce(&Ref) -> Result<Option<(String, bool)>>, administering: bool) -> Result<LabelOpen> {
    let Some(r) = label_reference(&o.source, &o.id) else { return Ok(LabelOpen::NoReference) };
    if let Some(Some(id)) = cat.labels_shown(&[(Some(r.clone()), String::new())])?.first() {
        return Ok(LabelOpen::Opened(*id));
    }
    let asked = o.name.trim();
    // A1.8.4: a label Discogs marks as a namesake ("Source (2)") never joins a label of its name: it is another.
    let (name, namesake) = match named(&r)? {
        Some((kept, namesake)) if ids::artist_key(kept.trim()) == ids::artist_key(asked) => (kept.trim().to_string(), namesake),
        _ if administering && !asked.is_empty() => (asked.to_string(), false),
        _ => return Ok(LabelOpen::NotNamed),
    };
    if name.chars().count() > NAME_MOST {
        anyhow::bail!("A name that long is no label's.");
    }
    Ok(LabelOpen::Opened(cat.open_label(&r, &name, !namesake)?))
}

#[cfg(any(test, feature = "test-support"))]
pub mod tests {
    // Shared with the tests of the crates above (test-support): idle where no test calls it.
    #![cfg_attr(not(test), allow(dead_code, unused_imports))]
    use super::*;

    const LABEL_MBID: &str = "00000000-0000-4000-8000-000000000560";
    const GROUP_1: &str = "00000000-0000-4000-8000-000000000561";
    const GROUP_2: &str = "00000000-0000-4000-8000-000000000562";
    const DAY: i64 = 86_400;

    /// A source of the test's own: the label it finds by name (none: it does not know it), its pages by where they start, and what
    /// it was asked for, in turn; a page past the first waits for `gate` where there is one.
    struct Stub {
        source: &'static str,
        knows: Option<&'static str>,
        pages: fn(Option<i64>) -> Page,
        asked: std::sync::Mutex<Vec<Option<i64>>>,
        gate: Option<Arc<tokio::sync::Semaphore>>,
    }

    impl LabelList for Stub {
        fn source(&self) -> &'static str {
            self.source
        }

        fn find<'a>(&'a self, _: &'a str, _: &'a [Ref]) -> crate::source::Fut<'a, Found> {
            let found = self.knows.map(|id| vec![(Ref::new(self.source, "label", id), "search")]).unwrap_or_default();
            Box::pin(async move { Ok(found) })
        }

        fn page<'a>(&'a self, _: &'a str, from: Option<i64>) -> crate::source::Fut<'a, Page> {
            self.asked.lock().unwrap().push(from);
            Box::pin(async move {
                if from.is_some()
                    && let Some(gate) = &self.gate
                {
                    let _turn = gate.acquire().await?;
                }
                Ok((self.pages)(from))
            })
        }
    }

    fn stub(source: &'static str, knows: Option<&'static str>, pages: fn(Option<i64>) -> Page) -> Arc<Stub> {
        Arc::new(Stub { source, knows, pages, asked: Default::default(), gate: None })
    }

    /// A row of MusicBrainz's list, by its release group.
    fn group_row(group: &str, title: &str, year: i32) -> LabelListed {
        LabelListed { reference: Ref::new("musicbrainz", "release-group", group), title: title.into(), artist: "Glass Harbour".into(), year: Some(year), kind: Some("album".into()), ..Default::default() }
    }

    /// Discogs' row of Night Walks: by its master, with its catalogue number and its picture.
    fn night_walks() -> LabelListed {
        LabelListed {
            reference: Ref::new("discogs", "master", "3001"),
            title: "Night Walks".into(),
            artist: "Kite Theory".into(),
            year: Some(2020),
            catno: "LTR-040".into(),
            kind: Some("ep".into()),
            cover: Some("https://i.discogs.com/night-walks.jpg".into()),
            also: Vec::new(),
        }
    }

    /// `count` rows of MusicBrainz's list from the `from`th, a year each.
    fn hundred(from: usize, count: usize) -> Vec<LabelListed> {
        (from..from + count).map(|i| group_row(&format!("00000000-0000-4000-8000-{:012}", 9000 + i), &format!("Record {i}"), 1990 + (i % 30) as i32)).collect()
    }

    /// The page once no read runs: `looking` must end (a test fails rather than waits for good). `more` is asked the first time.
    async fn settled(cat: &Arc<Catalogue>, label: i64, more: Option<&str>, sources: &[Arc<dyn LabelList>]) -> LabelPage {
        let mut more = more;
        for _ in 0..500 {
            let answer = page(cat.clone(), label, more.take(), sources).await.unwrap().expect("a label");
            if !answer.looking {
                return answer;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("still looking")
    }

    /// Every row's title: what is had, then the rest (F10).
    fn titles(shown: &LabelPage) -> Vec<&str> {
        shown.had.iter().chain(&shown.not_had).map(|r| r.title.as_str()).collect()
    }

    /// C56: a label's page from what the catalogue keeps, newest first: its records each once with its catalogue number, had by
    /// the one had rule or not; a source's row of a record (by its reference) is that record's row, and one no record holds is a
    /// row of its own; nothing read where no source is asked. An id no label has is none.
    #[tokio::test]
    async fn a_label_page_lists_its_kept_rows_by_year() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let label = cat.label_for("Low Tide Records").unwrap();
        let (had, wanted, older) = (cat.record_for_tests("Tide Tables", 2), cat.record_for_tests("Undertow", 0), cat.record_for_tests("Open Water", 0));
        {
            let db = cat.db_for_tests();
            for (record, year, catno) in [(had, 2012, "LTR-012"), (wanted, 2019, "LTR-030"), (older, 2003, "")] {
                db.execute("UPDATE records SET year = ?2, kind = 'album' WHERE id = ?1", rusqlite::params![record, year]).unwrap();
                db.execute("INSERT INTO record_labels (record, label, catno, how) VALUES (?1, ?2, ?3, 'listing')", rusqlite::params![record, label, catno]).unwrap();
            }
        }
        cat.set_reference(&ids::record(wanted), &Ref::new("musicbrainz", "release-group", GROUP_1), "link").unwrap();
        let rows = [group_row(GROUP_1, "Undertow", 2019), group_row(GROUP_2, "Paper Kites", 2016)];
        cat.keep_label_rows(label, "musicbrainz", &rows, None, true, crate::app::moment::now_secs()).unwrap();
        let shown = page(cat.clone(), label, None, &[]).await.unwrap().expect("a label");
        fn listed(rows: &[LabelEntry]) -> Vec<(&str, Option<i32>, Option<&str>, bool, Option<String>)> {
            rows.iter().map(|r| (r.title.as_str(), r.year, r.catno.as_deref(), r.had, r.record.clone())).collect()
        }
        assert_eq!(listed(&shown.had), [("Tide Tables", Some(2012), Some("LTR-012"), true, Some(ids::record(had)))], "F10: what the house has");
        assert_eq!(
            listed(&shown.not_had),
            [
                ("Undertow", Some(2019), Some("LTR-030"), false, Some(ids::record(wanted))),
                ("Paper Kites", Some(2016), None, false, None),
                ("Open Water", Some(2003), None, false, Some(ids::record(older))),
            ],
            "the rest"
        );
        assert_eq!(shown.not_had[0].release_group.as_deref(), Some(GROUP_1), "the source's row and the record are one row");
        assert_eq!(shown.not_had[0].cover, Some(super::super::cover_cache::record_address(wanted)), "F16.1: a record's row, its record's address");
        assert_eq!(shown.not_had[1].cover, Some(format!("/api/covers/release-group/{GROUP_2}")), "a row no record holds: its source's");
        assert_eq!(shown.had[0].cover, Some(format!("/api/albums/cratarr/{}/cover", ids::record(had))));
        assert_eq!((shown.id, shown.name.as_str(), shown.looking, shown.more), (ids::label(label), "Low Tide Records", false, More::default()));
        assert!(page(cat.clone(), label + 1, None, &[]).await.unwrap().is_none(), "no such label");
    }

    /// C56 (Q33): a label's list makes no record nor artist; a row opened makes its record of what the source's row says, tied to
    /// the label it was opened from, and the page's row is that record, once.
    #[tokio::test]
    async fn a_label_row_makes_no_record_until_opened() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let label = cat.label_for("Low Tide Records").unwrap();
        let records = || cat.db_for_tests().query_row("SELECT count(*) FROM records", [], |r| r.get::<_, i64>(0)).unwrap();
        let musicbrainz = stub("musicbrainz", Some(LABEL_MBID), |_| (vec![group_row(GROUP_1, "Undertow", 2019), group_row(GROUP_2, "Paper Kites", 2016)], None));
        let discogs = stub("discogs", Some("23528"), |_| (vec![night_walks()], None));
        let sources: Vec<Arc<dyn LabelList>> = vec![musicbrainz.clone(), discogs.clone()];
        let shown = settled(&cat, label, None, &sources).await;
        assert_eq!(titles(&shown), ["Night Walks", "Undertow", "Paper Kites"]);
        assert!(shown.had.is_empty() && shown.not_had.iter().all(|r| r.record.is_none() && !r.had), "{shown:?}");
        assert_eq!(records(), 0, "no record made of a label's list");
        assert_eq!(cat.artist_of("Kite Theory").unwrap(), None, "nor an artist");
        assert_eq!((shown.musicbrainz.as_deref(), shown.discogs.as_deref()), (Some(LABEL_MBID), Some("23528")), "what the sources know it by, kept");

        let opened = Opened { discogs_master: Some(3001), title: "Night Walks".into(), artist: "Kite Theory".into(), year: Some(2020), label: Some(ids::label(label)), ..Opened::default() };
        let night = open_record(&cat, &opened).unwrap().expect("made");
        assert_eq!(records(), 1);
        let row = cat.record(night).unwrap().unwrap();
        assert_eq!((row.title.as_str(), row.kind.as_deref(), row.cover.as_deref()), ("Night Walks", Some("ep"), Some("https://i.discogs.com/night-walks.jpg")), "what the source's row says");
        let ties: Vec<(i64, String)> = cat.record_labels(night).unwrap().into_iter().map(|(l, catno)| (l.id, catno)).collect();
        assert_eq!(ties, [(label, "LTR-040".to_string())], "tied to the label it was opened from");
        let shown = settled(&cat, label, None, &sources).await;
        let listed: Vec<(&str, Option<String>, bool, Option<&str>)> = shown.not_had.iter().map(|r| (r.title.as_str(), r.record.clone(), r.had, r.catno.as_deref())).collect();
        assert_eq!(listed.len(), 3, "one row: {listed:?}");
        assert_eq!(listed[0], ("Night Walks", Some(ids::record(night)), false, Some("LTR-040")), "the row is its record");
        assert_eq!(*musicbrainz.asked.lock().unwrap(), [None], "read once in thirty days");
    }

    /// C56: a row opened twice is one record, by its reference: a record of the same names with another reference is not it, and one
    /// merged into another answers that one. Never by its names alone; seen as it is made.
    #[test]
    fn opening_a_row_makes_its_record_once_by_reference() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let namesake = cat.record_for_tests("Night Walks", 0);
        cat.db_for_tests().execute("UPDATE records SET artist = 'Kite Theory' WHERE id = ?1", [namesake]).unwrap();
        cat.set_reference(&ids::record(namesake), &Ref::new("musicbrainz", "release-group", GROUP_2), "link").unwrap();
        let opened = Opened { release_group: Some(GROUP_1.to_uppercase()), title: "Night Walks".into(), artist: "Kite Theory".into(), year: Some(2020), ..Opened::default() };
        let first = open_record(&cat, &opened).unwrap().expect("made");
        assert_ne!(first, namesake, "a record of its names with another reference is another");
        assert_eq!(open_record(&cat, &opened).unwrap(), Some(first), "opened again: the same record");
        let group = Ref::new("musicbrainz", "release-group", GROUP_1);
        assert!(cat.references_of(&ids::record(first)).unwrap().iter().any(|(r, _)| *r == group), "by its reference, as MusicBrainz writes it");
        let seen: Option<i64> = cat.db_for_tests().query_row("SELECT seen_at FROM records WHERE id = ?1", [first], |r| r.get(0)).unwrap();
        assert!(seen.is_some(), "seen as it is made: RT01's rules from then on");
        // Merged into another (G3): its reference answers that one.
        let deluxe = Opened { discogs_master: Some(3001), title: "Night Walks (Deluxe)".into(), artist: "Kite Theory".into(), ..Opened::default() };
        let deluxe = open_record(&cat, &deluxe).unwrap().expect("made");
        cat.merge_records(first, deluxe).unwrap();
        assert_eq!(open_record(&cat, &opened).unwrap(), Some(deluxe), "as merged now");
        // Never by its names alone.
        let named = Opened { title: "Night Walks".into(), artist: "Kite Theory".into(), ..Opened::default() };
        assert_eq!(open_record(&cat, &named).unwrap(), None);
        assert_eq!(open_record(&cat, &Opened { release_group: Some("not an id".into()), ..named }).unwrap(), None);
        let made: i64 = cat.db_for_tests().query_row("SELECT count(*) FROM records", [], |r| r.get(0)).unwrap();
        assert_eq!(made, 3, "nothing made by names");
    }

    /// C56: a source's rows are kept thirty days: an open reads again, from its start, a list read longer ago, by the id kept for
    /// the label, and not one read since; RT01's daily pass lets go of what was read more than thirty days before it.
    #[tokio::test]
    async fn label_rows_expire_after_thirty_days() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let label = cat.label_for("Low Tide Records").unwrap();
        let now = crate::app::moment::now_secs();
        cat.set_reference(&ids::label(label), &Ref::new("musicbrainz", "label", LABEL_MBID), "search").unwrap();
        cat.keep_label_rows(label, "musicbrainz", &[group_row(GROUP_1, "Undertow", 2019)], Some(100), true, now - 31 * DAY).unwrap();
        cat.keep_label_rows(label, "discogs", &[night_walks()], None, true, now - 29 * DAY).unwrap();
        let musicbrainz = stub("musicbrainz", None, |_| (vec![group_row(GROUP_1, "Undertow", 2019), group_row(GROUP_2, "Paper Kites", 2016)], None));
        let discogs = stub("discogs", Some("23528"), |_| (Vec::new(), None));
        let sources: Vec<Arc<dyn LabelList>> = vec![musicbrainz.clone(), discogs.clone()];
        let shown = settled(&cat, label, None, &sources).await;
        assert_eq!(*musicbrainz.asked.lock().unwrap(), [None], "read thirty-one days ago: read again from its start");
        assert!(discogs.asked.lock().unwrap().is_empty(), "read twenty-nine days ago: kept");
        assert_eq!(titles(&shown), ["Night Walks", "Undertow", "Paper Kites"]);
        assert!(!shown.more.musicbrainz, "where the new read ends");
        assert_eq!(cat.forget_label_rows(now + 2 * DAY).unwrap(), 1, "Discogs' row, read thirty-one days before");
        let kept = cat.label_kept(label).unwrap().unwrap();
        let mut left: Vec<&str> = kept.rows.iter().map(|(l, _)| l.title.as_str()).collect();
        left.sort();
        assert_eq!(left, ["Paper Kites", "Undertow"]);
        assert_eq!(kept.reads.keys().collect::<Vec<_>>(), ["musicbrainz"], "Discogs' list read anew at the next open");
    }

    /// Z8 (the review's finding 8), U10: a row opened from a label's page is tied to that label only where it is found through that
    /// label's own kept row: an album the library has, opened by its reference with a label, is not tied to it, nor through another
    /// label's row, nor a record made now with no row of the label's.
    #[test]
    fn a_label_ties_only_a_record_made_now_or_found_through_its_rows() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (low, kite) = (cat.label_for("Low Tide Records").unwrap(), cat.label_for("Kite Records").unwrap());
        let owned = cat.record_for_tests("Tide Tables", 2);
        cat.set_reference(&ids::record(owned), &Ref::new("musicbrainz", "release-group", GROUP_1), "files").unwrap();
        let ties = |record: i64| cat.record_labels(record).unwrap().into_iter().map(|(l, catno)| (l.id, catno)).collect::<Vec<_>>();
        let opened = |group: &str, label: i64| {
            let row = Opened { release_group: Some(group.into()), title: "Tide Tables".into(), artist: "Glass Harbour".into(), label: Some(ids::label(label)), ..Opened::default() };
            open_record(&cat, &row).unwrap().expect("a record")
        };
        assert_eq!(opened(GROUP_1, low), owned, "found by its reference");
        assert!(ties(owned).is_empty(), "found, not through the label's rows: not tied");
        let kept = LabelListed { catno: "KR-1".into(), ..group_row(GROUP_1, "Tide Tables", 2012) };
        cat.keep_label_rows(kite, "musicbrainz", &[kept], None, true, crate::app::moment::now_secs()).unwrap();
        assert_eq!(opened(GROUP_1, low), owned);
        assert!(ties(owned).is_empty(), "another label's kept row ties it to none");
        assert_eq!(opened(GROUP_1, kite), owned);
        assert_eq!(ties(owned), [(kite, "KR-1".to_string())], "found through the label's own kept row: tied");
        let made = opened(GROUP_2, low);
        assert_ne!(made, owned);
        assert!(ties(made).is_empty(), "made now, no row of the label's: not tied (U10)");
    }

    /// U10 (the review of 8c/8d, finding 10): any account posting ids no label listed, with a label, makes a record that is tied to
    /// no label, so the label's page shows nothing of it; the same opened through the label's own kept row is tied, with its number.
    #[test]
    fn a_made_up_row_opened_with_a_label_is_not_tied() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let label = cat.label_for("Low Tide Records").unwrap();
        let made_up = Opened { discogs_release: Some(9_999_001), title: "x".repeat(40), artist: "Anyone".into(), label: Some(ids::label(label)), ..Opened::default() };
        let record = open_record(&cat, &made_up).unwrap().expect("made");
        assert!(cat.record_labels(record).unwrap().is_empty(), "tied to no label");
        cat.keep_label_rows(label, "discogs", &[night_walks()], None, true, crate::app::moment::now_secs()).unwrap();
        let opened = Opened { discogs_master: Some(3001), title: "Night Walks".into(), artist: "Kite Theory".into(), label: Some(ids::label(label)), ..Opened::default() };
        let night = open_record(&cat, &opened).unwrap().expect("made");
        let ties: Vec<(i64, String)> = cat.record_labels(night).unwrap().into_iter().map(|(l, catno)| (l.id, catno)).collect();
        assert_eq!(ties, [(label, "LTR-040".to_string())], "through its own kept row: tied");
    }

    /// Z8: a row opened with a title or an artist longer than `NAME_MOST` is refused, and nothing is made; one as long is opened.
    #[test]
    fn an_opened_rows_names_are_capped() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let row = |title: &str, artist: &str| Opened { release_group: Some(GROUP_1.into()), title: title.into(), artist: artist.into(), ..Opened::default() };
        let (long, most) = ("é".repeat(NAME_MOST + 1), "é".repeat(NAME_MOST));
        assert!(open_record(&cat, &row(&long, "Glass Harbour")).is_err(), "a title that long");
        assert!(open_record(&cat, &row("Tide Tables", &long)).is_err(), "an artist that long");
        let made: i64 = cat.db_for_tests().query_row("SELECT count(*) FROM records", [], |r| r.get(0)).unwrap();
        assert_eq!(made, 0, "nothing made");
        assert!(open_record(&cat, &row(&most, &most)).unwrap().is_some(), "as long as may be, counted in letters");
    }

    /// Z8: a source that failed is not asked again for fifteen minutes, by an open nor by "More".
    #[tokio::test]
    async fn more_waits_after_a_failure_too() {
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
        struct Failing(AtomicUsize);
        impl LabelList for Failing {
            fn source(&self) -> &'static str {
                "musicbrainz"
            }

            fn find<'a>(&'a self, _: &'a str, _: &'a [Ref]) -> crate::source::Fut<'a, Found> {
                self.0.fetch_add(1, SeqCst);
                let answer: Result<Found> = Err(anyhow::anyhow!("out of reach"));
                Box::pin(async move { answer })
            }

            fn page<'a>(&'a self, _: &'a str, _: Option<i64>) -> crate::source::Fut<'a, Page> {
                Box::pin(async { Ok((Vec::new(), None)) })
            }
        }
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        // Kept for the process: no later test's catalogue takes its address, and with it this failure.
        std::mem::forget(cat.clone());
        let label = cat.label_for("Low Tide Records").unwrap();
        let failing = Arc::new(Failing(AtomicUsize::new(0)));
        let sources: Vec<Arc<dyn LabelList>> = vec![failing.clone()];
        settled(&cat, label, None, &sources).await;
        assert_eq!(failing.0.load(SeqCst), 1, "asked once, and failed");
        settled(&cat, label, Some("musicbrainz"), &sources).await;
        settled(&cat, label, None, &sources).await;
        assert_eq!(failing.0.load(SeqCst), 1, "not asked again within fifteen minutes, by More either");
    }

    /// C56 (OQ10 (a)): a label's first hundred at a source at open, "More" the next hundred, one read at a time: a second "More"
    /// while one reads starts none, and at its end there is no more. A source that does not know the label is not read; the
    /// label's MusicBrainz id, found by its name, is kept as its reference.
    #[tokio::test]
    async fn more_reads_the_next_hundred() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let label = cat.label_for("Low Tide Records").unwrap();
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let musicbrainz = Arc::new(Stub {
            source: "musicbrainz",
            knows: Some(LABEL_MBID),
            pages: |from| match from {
                None => (hundred(0, 100), Some(100)),
                Some(_) => (hundred(100, 40), None),
            },
            asked: Default::default(),
            gate: Some(gate.clone()),
        });
        let discogs = stub("discogs", None, |_| (hundred(500, 100), Some(2)));
        let sources: Vec<Arc<dyn LabelList>> = vec![musicbrainz.clone(), discogs.clone()];
        let first = settled(&cat, label, None, &sources).await;
        assert_eq!(first.not_had.len(), 100, "the first hundred");
        assert_eq!(first.more, More { musicbrainz: true, discogs: false }, "Discogs does not know it");
        assert!(discogs.asked.lock().unwrap().is_empty(), "nor is it read there");
        assert!(first.not_had.windows(2).all(|w| w[0].year >= w[1].year), "newest first");
        let asked = page(cat.clone(), label, Some("musicbrainz"), &sources).await.unwrap().unwrap();
        let again = page(cat.clone(), label, Some("musicbrainz"), &sources).await.unwrap().unwrap();
        assert!(asked.looking && again.looking);
        gate.add_permits(1);
        let all = settled(&cat, label, None, &sources).await;
        assert_eq!(all.not_had.len(), 140, "the next hundred, here the last forty");
        assert_eq!(*musicbrainz.asked.lock().unwrap(), [None, Some(100)], "one read at a time: the second More started none");
        assert_eq!(all.more, More::default(), "at its end");
        settled(&cat, label, Some("musicbrainz"), &sources).await;
        assert_eq!(musicbrainz.asked.lock().unwrap().len(), 2, "no more to read");
        let known: Vec<(String, String)> = cat.references_of(&ids::label(label)).unwrap().into_iter().map(|(r, row)| (r.value, row.how)).collect();
        assert_eq!(known, [(LABEL_MBID.to_string(), "search".to_string())], "its MusicBrainz id found by its name, kept");
    }

    // ---- F9 (preview 1's fix batch): every label About shows opens its page where it has a reference

    /// The name a reference was kept with, as `discogs_facts::label_named` answers it: every one, by this name (the test's).
    fn named_as(name: &'static str) -> impl Fn(&Ref) -> Result<Option<(String, bool)>> {
        move |_| Ok(Some((name.to_string(), false)))
    }

    /// A source of the test's own that knows no label by its name: how often it was asked to find one, and the ids its list was read
    /// by, in turn.
    struct Asking {
        source: &'static str,
        finds: std::sync::atomic::AtomicUsize,
        ids: std::sync::Mutex<Vec<String>>,
    }

    impl Asking {
        fn new(source: &'static str) -> Arc<Self> {
            Arc::new(Asking { source, finds: Default::default(), ids: Default::default() })
        }

        fn asked(&self) -> (Vec<String>, usize) {
            (self.ids.lock().unwrap().clone(), self.finds.load(std::sync::atomic::Ordering::SeqCst))
        }
    }

    impl LabelList for Asking {
        fn source(&self) -> &'static str {
            self.source
        }

        fn find<'a>(&'a self, _: &'a str, _: &'a [Ref]) -> crate::source::Fut<'a, Found> {
            self.finds.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async { Ok(Vec::new()) })
        }

        fn page<'a>(&'a self, id: &'a str, _: Option<i64>) -> crate::source::Fut<'a, Page> {
            self.ids.lock().unwrap().push(id.to_string());
            Box::pin(async { Ok((Vec::new(), None)) })
        }
    }

    fn labels_kept(cat: &Catalogue) -> i64 {
        cat.db_for_tests().query_row("SELECT count(*) FROM labels", [], |r| r.get(0)).unwrap()
    }

    /// F9.1 (b), (e): a label About shows by its Discogs id, or by its MBID, opens the label found by that reference, else one made now
    /// with it, by the name it was kept with: opened twice, or by two at once, one label; never a label of another name (one the files
    /// name, holding no reference); found by anyone, by any name. A later read of About names it by its id (nothing left to open); a
    /// later read of its list reads it at that source by that reference, finding nothing by its name. (The one label of its name it
    /// joins: `a_label_opened_by_its_reference_is_the_one_label_of_its_name`, RG9.)
    #[tokio::test]
    async fn a_label_is_found_or_made_by_its_reference_once() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let files = cat.label_for("Quiet Pier").unwrap();
        let by_discogs = LabelOpened { source: "discogs".into(), id: "23528".into(), name: "Low Tide Records".into() };
        let LabelOpen::Opened(made) = open_label(&cat, &by_discogs, named_as("Low Tide Records"), false).unwrap() else { panic!("not opened") };
        assert_ne!(made, files, "by its reference alone: a label of another name is not it");
        assert_eq!(open_label(&cat, &by_discogs, named_as("Low Tide Records"), false).unwrap(), LabelOpen::Opened(made), "opened again: the same label");
        let anyone = LabelOpened { id: " 23528 ".into(), name: "Something Else".into(), ..by_discogs.clone() };
        assert_eq!(open_label(&cat, &anyone, |_| Ok(None), false).unwrap(), LabelOpen::Opened(made), "found by its reference, by anyone, by any name");
        assert_eq!(labels_kept(&cat), 2);
        assert_eq!(cat.label(made).unwrap().map(|l| l.name), Some("Low Tide Records".to_string()));
        let refs = |id: i64| cat.references_of(&ids::label(id)).unwrap().into_iter().map(|(r, _)| r).collect::<Vec<_>>();
        assert_eq!(refs(made), [Ref::new("discogs", "label", "23528")]);
        assert!(refs(files).is_empty(), "a label of another name is given nothing");
        // MusicBrainz's MBID, as MusicBrainz writes it.
        let by_mbid = LabelOpened { source: "musicbrainz".into(), id: LABEL_MBID.to_uppercase(), name: "Kite Records".into() };
        let LabelOpen::Opened(kite) = open_label(&cat, &by_mbid, named_as("Kite Records"), false).unwrap() else { panic!("not opened") };
        assert_eq!(open_label(&cat, &by_mbid, named_as("Kite Records"), false).unwrap(), LabelOpen::Opened(kite));
        assert_eq!(refs(kite), [Ref::new("musicbrainz", "label", LABEL_MBID)]);
        assert_eq!(labels_kept(&cat), 3);
        // Two at once: one label.
        let at_once = LabelOpened { id: "4401".into(), name: "Paper Lantern".into(), ..by_discogs.clone() };
        let both: Vec<LabelOpen> = std::thread::scope(|s| {
            let runs: Vec<_> = (0..2).map(|_| s.spawn(|| open_label(&cat, &at_once, named_as("Paper Lantern"), false).unwrap())).collect();
            runs.into_iter().map(|r| r.join().unwrap()).collect()
        });
        assert_eq!(both[0], both[1], "one label");
        assert_eq!(labels_kept(&cat), 4);
        // A later read of About: each by its id.
        let discogs = [crate::app::discogs::Label { name: "Low Tide Records".into(), catno: Some("LTR-012".into()), id: Some(23528), namesake: false }];
        let musicbrainz = [crate::app::discogs::MbLabel { name: "Kite Records".into(), mbid: LABEL_MBID.into(), catno: None }];
        assert_eq!(
            serde_json::to_value(about(&cat, &discogs, &musicbrainz).unwrap()).unwrap(),
            serde_json::json!([{ "name": "Low Tide Records", "catno": "LTR-012", "id": ids::label(made) }, { "name": "Kite Records", "id": ids::label(kite) }])
        );
        // A later read of its list: at Discogs by its id, at MusicBrainz by its MBID, nothing found by its name there.
        let (musicbrainz, discogs) = (Asking::new("musicbrainz"), Asking::new("discogs"));
        let sources: Vec<Arc<dyn LabelList>> = vec![musicbrainz.clone(), discogs.clone()];
        let shown = settled(&cat, made, None, &sources).await;
        assert_eq!(discogs.asked(), (vec!["23528".to_string()], 0), "Discogs' list read by its id");
        assert_eq!(shown.discogs.as_deref(), Some("23528"));
        let (musicbrainz, discogs) = (Asking::new("musicbrainz"), Asking::new("discogs"));
        let sources: Vec<Arc<dyn LabelList>> = vec![musicbrainz.clone(), discogs];
        let shown = settled(&cat, kite, None, &sources).await;
        assert_eq!(musicbrainz.asked(), (vec![LABEL_MBID.to_string()], 0), "MusicBrainz's list read by its MBID");
        assert_eq!(shown.musicbrainz.as_deref(), Some(LABEL_MBID));
        assert_eq!(labels_kept(&cat), 4, "nothing made by reading");
    }

    /// F9 (as Z8's artists): a label no label holds is made only by a reference an album's labels were kept with (at Discogs or
    /// MusicBrainz, `discogs_facts::label_named`), by the name it was kept with, or by an administrator, by the name given; never by a
    /// name alone, nor by what is no Discogs id or MBID. Nothing is made where it is refused.
    #[test]
    fn a_label_is_made_only_by_a_reference_the_server_named() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let open = |source: &str, id: &str, name: &str, named: Option<&'static str>, administering: bool| {
            open_label(&cat, &LabelOpened { source: source.into(), id: id.into(), name: name.into() }, move |_| Ok(named.map(|n| (n.to_string(), false))), administering).unwrap()
        };
        for (source, id) in [("", ""), ("bandcamp", "23528"), ("discogs", "x1"), ("discogs", "0"), ("discogs", ""), ("musicbrainz", "not an id"), ("musicbrainz", "23528")] {
            assert_eq!(open(source, id, "Low Tide Records", Some("Low Tide Records"), true), LabelOpen::NoReference, "{source:?} {id:?}: a name alone opens none");
        }
        assert_eq!(open("discogs", "23528", "Low Tide Records", None, false), LabelOpen::NotNamed, "named by nothing kept");
        assert_eq!(open("discogs", "23528", "Kite Records", Some("Low Tide Records"), false), LabelOpen::NotNamed, "by another name than it was kept with");
        assert_eq!(open("discogs", "23528", " ", None, true), LabelOpen::NotNamed, "an administrator names it");
        assert_eq!(labels_kept(&cat), 0, "nothing made");
        let LabelOpen::Opened(kept) = open("discogs", "23528", "low tide records", Some("Low Tide Records"), false) else { panic!("not opened") };
        assert_eq!(cat.label(kept).unwrap().map(|l| l.name), Some("Low Tide Records".to_string()), "by the name it was kept with");
        let LabelOpen::Opened(any) = open("musicbrainz", LABEL_MBID, " Quiet Pier ", None, true) else { panic!("not opened") };
        assert_eq!(cat.label(any).unwrap().map(|l| l.name), Some("Quiet Pier".to_string()), "an administrator's, by the name given");
        let long = LabelOpened { source: "discogs".into(), id: "77".into(), name: "\u{e9}".repeat(NAME_MOST + 1) };
        assert!(open_label(&cat, &long, |_| Ok(None), true).is_err(), "a name that long is no label's");
        assert_eq!(labels_kept(&cat), 2);
    }

    /// F9.1 (c), (d): a label known by its name alone (a page elsewhere's label, Discogs' kept before C46 with no id) opens the one
    /// label of its name where exactly one has it; with two of its name, or none, it is text, and nothing is made. Discogs' "Not On
    /// Label" (self-released) is text, though Discogs gives it an id.
    #[test]
    fn a_label_known_by_name_alone_opens_only_the_one_of_its_name() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let low = cat.label_for("Low Tide Records").unwrap();
        let text = |t: &str| serde_json::to_value(of_text(&cat, t).unwrap()).unwrap();
        assert_eq!(text(" LOW TIDE RECORDS "), serde_json::json!([{ "name": "LOW TIDE RECORDS", "id": ids::label(low) }]), "(c) the one label of its name");
        assert_eq!(text("Quiet Pier"), serde_json::json!([{ "name": "Quiet Pier" }]), "no label of its name: text");
        assert_eq!(text(" "), serde_json::json!([]));
        let old = [crate::app::discogs::Label { name: "Low Tide Records".into(), catno: Some("LTR-012".into()), id: None, namesake: false }];
        let at_discogs = |labels: &[crate::app::discogs::Label]| serde_json::to_value(about(&cat, labels, &[]).unwrap()).unwrap();
        assert_eq!(at_discogs(&old), serde_json::json!([{ "name": "Low Tide Records", "catno": "LTR-012", "id": ids::label(low) }]), "kept with no id: by its name");
        // (d) A second label of the name, made by its reference, the files' label holding another Discogs id (RG9: by the first, the
        // files' label was joined): the name opens neither.
        assert_eq!(cat.open_label(&Ref::new("discogs", "label", "23528"), "Low Tide Records", true).unwrap(), low, "the one label of its name");
        let second = cat.open_label(&Ref::new("discogs", "label", "777"), "Low Tide Records", true).unwrap();
        assert_ne!(second, low);
        assert_eq!(text("Low Tide Records"), serde_json::json!([{ "name": "Low Tide Records" }]), "(d) two labels of its name: text");
        assert_eq!(at_discogs(&old), serde_json::json!([{ "name": "Low Tide Records", "catno": "LTR-012" }]));
        let self_released = [crate::app::discogs::Label { name: "Not On Label (Kite Theory Self-released)".into(), catno: None, id: Some(1818), namesake: false }];
        assert_eq!(at_discogs(&self_released), serde_json::json!([{ "name": "Not On Label (Kite Theory Self-released)" }]), "no label: text");
        assert_eq!(labels_kept(&cat), 2, "nothing made by showing");
    }

    /// RG9 (PB3, the review of preview 1's fixes): a label opened by its Discogs id or its MBID that no label holds is the one label
    /// of its name, where exactly one has it (X8) and it holds none of that source's references: it is given the reference, as an
    /// outside row joins the record the house has of its names (F11.2). One real label never gets two pages: the files' label keeps
    /// its albums and takes the Discogs id; a later About names it by that id, a name-only chip of its name (a Bandcamp album's) still
    /// opens it, and a later read of its page reads Discogs' list by that id. Where the one label of its name holds another id of that
    /// source, a label is made by the reference alone; where two labels have the name, it joins neither; never a second label for one
    /// reference.
    #[tokio::test]
    async fn a_label_opened_by_its_reference_is_the_one_label_of_its_name() {
        const MBID_2: &str = "00000000-0000-4000-8000-000000000569";
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let refs = |id: i64| cat.references_of(&ids::label(id)).unwrap().into_iter().map(|(r, _)| r).collect::<Vec<_>>();
        // The files name Low Tide Records on an album: its label, with no reference yet.
        let files = cat.label_for("Low Tide Records").unwrap();
        let record = cat.record_for_tests("Night Walks", 2);
        cat.tie_label(record, files, "LTR-012").unwrap();
        // Its Discogs id, from a record not had's About: the files' label, given the id.
        let by_discogs = LabelOpened { source: "discogs".into(), id: "23528".into(), name: "low tide records".into() };
        assert_eq!(open_label(&cat, &by_discogs, named_as("Low Tide Records"), false).unwrap(), LabelOpen::Opened(files), "the one label of its name");
        assert_eq!(refs(files), [Ref::new("discogs", "label", "23528")], "the files' label keeps the Discogs id");
        assert_eq!(cat.label_records(files).unwrap(), [(record, "LTR-012".to_string())], "its page holds the files' albums");
        assert_eq!(cat.label(files).unwrap().map(|l| l.name), Some("Low Tide Records".to_string()), "its name as it was");
        assert_eq!(labels_kept(&cat), 1, "one real label, one page");
        assert_eq!(open_label(&cat, &by_discogs, named_as("Low Tide Records"), false).unwrap(), LabelOpen::Opened(files), "opened again: the same");
        // A later About names it by its id; a name-only chip of its name still opens it.
        let at_discogs = [crate::app::discogs::Label { name: "Low Tide Records".into(), catno: Some("LTR-012".into()), id: Some(23528), namesake: false }];
        assert_eq!(
            serde_json::to_value(about(&cat, &at_discogs, &[]).unwrap()).unwrap(),
            serde_json::json!([{ "name": "Low Tide Records", "catno": "LTR-012", "id": ids::label(files) }])
        );
        assert_eq!(serde_json::to_value(of_text(&cat, "Low Tide Records").unwrap()).unwrap(), serde_json::json!([{ "name": "Low Tide Records", "id": ids::label(files) }]));
        // A later read of its page: Discogs' list by that id, nothing found by its name.
        let (musicbrainz, discogs) = (Asking::new("musicbrainz"), Asking::new("discogs"));
        let sources: Vec<Arc<dyn LabelList>> = vec![musicbrainz, discogs.clone()];
        let shown = settled(&cat, files, None, &sources).await;
        assert_eq!(discogs.asked(), (vec!["23528".to_string()], 0), "Discogs' list read by its id");
        assert_eq!(shown.discogs.as_deref(), Some("23528"));
        // By its MBID: the one label of its name, which holds none of MusicBrainz's, given it beside its Discogs id.
        let kite = cat.label_for("Kite Records").unwrap();
        let by_mbid = LabelOpened { source: "musicbrainz".into(), id: LABEL_MBID.to_uppercase(), name: "Kite Records".into() };
        assert_eq!(open_label(&cat, &by_mbid, named_as("Kite Records"), false).unwrap(), LabelOpen::Opened(kite));
        let kite_discogs = LabelOpened { source: "discogs".into(), id: "4401".into(), name: "Kite Records".into() };
        assert_eq!(open_label(&cat, &kite_discogs, named_as("Kite Records"), false).unwrap(), LabelOpen::Opened(kite));
        assert_eq!(refs(kite), [Ref::new("discogs", "label", "4401"), Ref::new("musicbrainz", "label", LABEL_MBID)]);
        assert_eq!(labels_kept(&cat), 2);
        // Another Discogs id of Low Tide Records' name: the one of its name holds another of Discogs', so it is made by its reference.
        let other = LabelOpened { source: "discogs".into(), id: "777".into(), name: "Low Tide Records".into() };
        let LabelOpen::Opened(second) = open_label(&cat, &other, named_as("Low Tide Records"), false).unwrap() else { panic!("not opened") };
        assert_ne!(second, files, "a label holding another id at that source is another label");
        assert_eq!(open_label(&cat, &other, named_as("Low Tide Records"), false).unwrap(), LabelOpen::Opened(second), "never a second label for one reference");
        assert_eq!((refs(files), refs(second)), (vec![Ref::new("discogs", "label", "23528")], vec![Ref::new("discogs", "label", "777")]));
        assert_eq!(labels_kept(&cat), 3);
        // Two labels of its name now (X8): its MBID joins neither, and its name opens neither.
        let x8 = LabelOpened { source: "musicbrainz".into(), id: MBID_2.into(), name: "Low Tide Records".into() };
        let LabelOpen::Opened(third) = open_label(&cat, &x8, named_as("Low Tide Records"), false).unwrap() else { panic!("not opened") };
        assert!(third != files && third != second, "X8: a name two labels have joins neither");
        assert_eq!((refs(files).len(), refs(second).len()), (1, 1), "each keeps its own");
        assert_eq!(labels_kept(&cat), 4);
        assert_eq!(serde_json::to_value(of_text(&cat, "Low Tide Records").unwrap()).unwrap(), serde_json::json!([{ "name": "Low Tide Records" }]));
    }

    // ---- F10 (preview 1's fix batch): a label page shows what the house has apart from what it doesn't

    /// A source whose list is the test's, each page read once the test lets it (`gate`); it knows the label as `LABEL_MBID`.
    struct Gated {
        source: &'static str,
        rows: Vec<LabelListed>,
        gate: Arc<tokio::sync::Semaphore>,
    }

    impl LabelList for Gated {
        fn source(&self) -> &'static str {
            self.source
        }

        fn find<'a>(&'a self, _: &'a str, _: &'a [Ref]) -> crate::source::Fut<'a, Found> {
            let found = vec![(Ref::new(self.source, "label", LABEL_MBID), "search")];
            Box::pin(async move { Ok(found) })
        }

        fn page<'a>(&'a self, _: &'a str, _: Option<i64>) -> crate::source::Fut<'a, Page> {
            Box::pin(async move {
                let _turn = self.gate.acquire().await?;
                Ok((self.rows.clone(), None))
            })
        }
    }

    /// A source that does not answer (F10).
    struct Unanswering(&'static str);

    impl LabelList for Unanswering {
        fn source(&self) -> &'static str {
            self.0
        }

        fn find<'a>(&'a self, _: &'a str, _: &'a [Ref]) -> crate::source::Fut<'a, Found> {
            Box::pin(async { Err(anyhow::anyhow!("out of reach")) })
        }

        fn page<'a>(&'a self, _: &'a str, _: Option<i64>) -> crate::source::Fut<'a, Page> {
            Box::pin(async { Ok((Vec::new(), None)) })
        }
    }

    /// A source switched off (F10): never asked.
    struct Off(&'static str);

    impl LabelList for Off {
        fn source(&self) -> &'static str {
            self.0
        }

        fn on(&self) -> bool {
            false
        }

        fn find<'a>(&'a self, _: &'a str, _: &'a [Ref]) -> crate::source::Fut<'a, Found> {
            Box::pin(async { Err(anyhow::anyhow!("asked though switched off")) })
        }

        fn page<'a>(&'a self, _: &'a str, _: Option<i64>) -> crate::source::Fut<'a, Page> {
            Box::pin(async { Err(anyhow::anyhow!("asked though switched off")) })
        }
    }

    /// What is had of a label page, in its order: each title, record and mark.
    fn had_of(p: &LabelPage) -> Vec<(&str, Option<String>, bool)> {
        p.had.iter().map(|r| (r.title.as_str(), r.record.clone(), r.had)).collect()
    }

    /// F10.1: a label page answers what the house has of the label apart from the rest. Its three records had, one in Music, one in
    /// Incoming, one in an own music (the one had rule: any library), are `had` at once, by year, while its list is read; once read,
    /// `had` is the same three in the same order, the list's row of each among them (two by their references, one by its names alone,
    /// Z6), and `not_had` the twelve others by year with their catalogue numbers, the record not had among them. A record that arrives
    /// is had at the next read. Never a had record among the rest; never one list of both.
    #[tokio::test]
    async fn a_label_page_shows_what_is_had_apart() {
        let cat = Arc::new(Catalogue::open(None, ["/music", "/incoming", "/own/ann"].map(std::path::PathBuf::from).to_vec()).unwrap());
        let label = cat.label_for("Low Tide Records").unwrap();
        let group = |n: usize| format!("00000000-0000-4000-8000-{:012}", 7000 + n);
        let tied = |title: &str, tracks: usize, year: i32| {
            let record = cat.record_for_tests(title, tracks);
            let db = cat.db_for_tests();
            db.execute("UPDATE records SET year = ?2, kind = 'album' WHERE id = ?1", rusqlite::params![record, year]).unwrap();
            db.execute("INSERT INTO record_labels (record, label, catno, how) VALUES (?1, ?2, '', 'files')", rusqlite::params![record, label]).unwrap();
            record
        };
        // Had: Tide Tables in Music, Open Water in Incoming, Harbour Lights in Ann's own music; Undertow not had.
        let (tide, open, lights) = (tied("Tide Tables", 2, 2012), tied("Open Water", 1, 2015), tied("Harbour Lights", 1, 2009));
        for (record, folder) in [(tide, 0), (open, 1), (lights, 2)] {
            cat.copy_for_tests(record, folder);
        }
        let undertow = tied("Undertow", 0, 2019);
        for (record, n) in [(tide, 1), (open, 2), (undertow, 3)] {
            cat.set_reference(&ids::record(record), &Ref::new("musicbrainz", "release-group", &group(n)), "files").unwrap();
        }
        // Its list: the three had (Harbour Lights by a group no record holds: its names alone), Undertow, and eleven no record holds.
        let mut rows = vec![
            group_row(&group(1), "Tide Tables", 2012),
            group_row(&group(2), "Open Water", 2015),
            group_row(&group(4), "Harbour Lights", 2009),
            LabelListed { catno: "LTR-030".into(), ..group_row(&group(3), "Undertow", 2019) },
        ];
        rows.extend((0..11).map(|i| LabelListed { catno: format!("LTR-1{i:02}"), ..group_row(&group(10 + i), &format!("Record {i}"), 2000 + i as i32) }));
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let sources: Vec<Arc<dyn LabelList>> = vec![Arc::new(Gated { source: "musicbrainz", rows, gate: gate.clone() })];
        let at_once = page(cat.clone(), label, None, &sources).await.unwrap().expect("a label");
        assert!(at_once.looking, "its list being read");
        let three = [("Open Water", Some(ids::record(open)), true), ("Tide Tables", Some(ids::record(tide)), true), ("Harbour Lights", Some(ids::record(lights)), true)];
        assert_eq!(had_of(&at_once), three, "at once, by year: Incoming's, Music's, the own music's");
        assert_eq!(at_once.not_had.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["Undertow"]);
        gate.add_permits(1);
        let read = settled(&cat, label, None, &sources).await;
        assert_eq!(had_of(&read), three, "the list's arrival moves none of them");
        assert_eq!(read.had[2].release_group, Some(group(4)), "Harbour Lights' row of the list, by its names alone (Z6)");
        let rest: Vec<(&str, Option<&str>, bool)> = read.not_had.iter().map(|r| (r.title.as_str(), r.catno.as_deref(), r.had)).collect();
        let others: Vec<(String, String)> = (0..11).rev().map(|i| (format!("Record {i}"), format!("LTR-1{i:02}"))).collect();
        let mut expected = vec![("Undertow", Some("LTR-030"), false)];
        expected.extend(others.iter().map(|(t, c)| (t.as_str(), Some(c.as_str()), false)));
        assert_eq!(rest, expected, "the twelve others by year, newest first, with their catalogue numbers");
        let had: Vec<&Option<String>> = read.had.iter().map(|r| &r.record).collect();
        assert!(read.not_had.iter().all(|r| !r.had && (r.record.is_none() || !had.contains(&&r.record))), "never a had record among the rest");
        // Undertow arrives, in the own music: had at the next read.
        cat.db_for_tests().execute("INSERT INTO tracks (born, record, disc, number) VALUES (1, ?1, 1, 1)", [undertow]).unwrap();
        cat.copy_for_tests(undertow, 2);
        let after = settled(&cat, label, None, &sources).await;
        assert_eq!(had_of(&after).first().map(|(t, ..)| *t), Some("Undertow"), "{:?}", had_of(&after));
        assert_eq!((after.had.len(), after.not_had.len()), (4, 11));
        assert!(after.not_had.iter().all(|r| r.title != "Undertow"));
        let v = serde_json::to_value(&after).unwrap();
        assert!(v.get("rows").is_none() && v["had"].is_array() && v["not_had"].is_array(), "two lists, not one: {v}");
    }

    /// F10.1 (as F8's sections, in F1's words): while a label's list is read, its page names the source it reads and says nothing
    /// missing; once none is read, each source whose list could not be had: one that failed (`unanswered`, until it is asked again a
    /// quarter hour on), one switched off with its list not kept (`off`). One switched off whose list is kept says nothing.
    #[tokio::test]
    async fn a_label_page_says_what_it_reads_and_what_it_could_not() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        // Kept for the process: no later test's catalogue takes its address, and with it these failures.
        std::mem::forget(cat.clone());
        let label = cat.label_for("Low Tide Records").unwrap();
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let musicbrainz: Arc<dyn LabelList> = Arc::new(Gated { source: "musicbrainz", rows: vec![group_row(GROUP_1, "Undertow", 2019)], gate: gate.clone() });
        let sources: Vec<Arc<dyn LabelList>> = vec![musicbrainz, Arc::new(Unanswering("discogs"))];
        let asked = page(cat.clone(), label, None, &sources).await.unwrap().expect("a label");
        assert!(asked.looking);
        assert_eq!((asked.reading, asked.no_list.is_none()), (Some("musicbrainz"), true), "the source read now; nothing said missing while one is read");
        gate.add_permits(1);
        let read = settled(&cat, label, None, &sources).await;
        let said = |p: &LabelPage| serde_json::to_value(p).unwrap();
        assert_eq!(said(&read).get("reading"), None, "none read now");
        assert_eq!(said(&read)["no_list"], serde_json::json!([{ "source": "discogs", "said": "unanswered" }]), "F1's words");
        assert_eq!(titles(&read), ["Undertow"], "what MusicBrainz listed");
        // MusicBrainz switched off: its list kept, nothing said of it; a label never read there, said off.
        let off: Vec<Arc<dyn LabelList>> = vec![Arc::new(Off("musicbrainz")), Arc::new(Unanswering("discogs"))];
        let kept = settled(&cat, label, None, &off).await;
        assert_eq!(said(&kept)["no_list"], serde_json::json!([{ "source": "discogs", "said": "unanswered" }]));
        let never = settled(&cat, cat.label_for("Kite Records").unwrap(), None, &off).await;
        assert_eq!(said(&never)["no_list"], serde_json::json!([{ "source": "musicbrainz", "said": "off" }, { "source": "discogs", "said": "unanswered" }]));
    }

    /// A2.4.3: a label whose search at a source finds an id another label holds reads nothing there: nothing read or kept under it from
    /// that source (no page asked), and its page says no reason for it (the source answered: not this label's).
    #[tokio::test]
    async fn a_label_reads_no_list_by_an_id_it_could_not_keep() {
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let holder = cat.label_for("Warp").unwrap();
        assert!(cat.set_reference(&ids::label(holder), &Ref::new("musicbrainz", "label", LABEL_MBID), "link").unwrap());
        let variant = {
            let db = cat.db_for_tests();
            db.execute("INSERT INTO labels (born, name, key) VALUES (1, 'Warp Imprint', 'warp imprint')", []).unwrap();
            db.last_insert_rowid()
        };
        let musicbrainz = stub("musicbrainz", Some(LABEL_MBID), |_| (vec![group_row(GROUP_1, "Night Walks", 2020)], None));
        let shown = settled(&cat, variant, None, &[musicbrainz.clone() as Arc<dyn LabelList>]).await;
        assert!(musicbrainz.asked.lock().unwrap().is_empty(), "no page read by an id another label holds");
        assert!(shown.had.is_empty() && shown.not_had.is_empty(), "nothing kept under it");
        assert!(shown.no_list.is_none(), "no reason said for that source");
        assert_eq!(shown.musicbrainz, None, "the id stays the holder's");
        assert!(cat.references_of(&ids::label(variant)).unwrap().is_empty());
    }

    /// A1.8.4: a Discogs label named "Source (2)" (Discogs' own namesake mark) never joins the house's "Source" by name: opened, it is a
    /// label of its own; one Discogs gives no mark joins the one of its name, as PB3 has it.
    #[test]
    fn a_discogs_label_namesake_joins_no_label_by_name() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let house = cat.label_for("Source").unwrap();
        let namesake = LabelOpened { source: "discogs".into(), id: "2880".into(), name: "Source".into() };
        let LabelOpen::Opened(opened) = open_label(&cat, &namesake, |_| Ok(Some(("Source".to_string(), true))), false).unwrap() else { panic!("not opened") };
        assert_ne!(opened, house, "a namesake: a label of its own");
        assert!(cat.references_of(&ids::label(house)).unwrap().is_empty(), "the house's Source holds no Discogs id");
        let plain = LabelOpened { source: "discogs".into(), id: "2881".into(), name: "Others".into() };
        let others = cat.label_for("Others").unwrap();
        assert_eq!(open_label(&cat, &plain, |_| Ok(Some(("Others".to_string(), false))), false).unwrap(), LabelOpen::Opened(others), "no mark: the one of its name");
        // The facts keep Discogs' mark: "Source (2)" named Source, marked.
        let v = serde_json::json!({ "labels": [{ "name": "Source (2)", "catno": "SRC 1", "id": 2880 }] });
        let read = crate::app::discogs::labels_of_for_tests(&v);
        assert_eq!((read[0].name.as_str(), read[0].namesake), ("Source", true));
    }

    // ---- F18.1, F15.1, A2.3

    /// A row of a label's Discogs list: by its release, of `title` by `artist`.
    pub fn discogs_row(release: &str, title: &str, artist: &str, catno: &str) -> LabelListed {
        LabelListed { reference: Ref::new("discogs", "release", release), title: title.into(), artist: artist.into(), year: Some(2020), catno: catno.into(), ..Default::default() }
    }

    /// A row opened from `label`'s page as the page sends it (`POST /api/records/open`): its references, names and the label.
    pub fn open_row(cat: &Catalogue, label: i64, l: &LabelListed) -> i64 {
        let r = &l.reference;
        let o = Opened {
            release_group: (r.kind == "release-group").then(|| r.value.clone()),
            discogs_master: (r.source == "discogs" && r.kind == "master").then(|| r.value.parse().unwrap()),
            discogs_release: (r.source == "discogs" && r.kind == "release").then(|| r.value.parse().unwrap()),
            title: l.title.clone(),
            artist: crate::app::discogs::clean_name(&l.artist),
            year: l.year,
            label: Some(ids::label(label)),
        };
        open_record(cat, &o).unwrap().expect("a record")
    }

    /// F18.1: rows opened from a label's page credit their artist at once. (a) MusicBrainz's row names its credit's artist by id (A5):
    /// credited to an artist found or made by it, holding A5; again, the same one. (b) A Discogs row by "Glass Harbour", the one artist
    /// of the name: credited to them. (c) No artist of the name, (d) two of it, (e) a name Discogs marks "Glass Harbour (2)": nobody yet
    /// (Z3), and no artist made by a name. Never one credited to an artist holding another MusicBrainz id than its row names.
    #[test]
    fn a_row_opened_credits_its_artist_by_reference_else_the_one_of_its_name() {
        const A5: &str = "00000000-0000-4000-8000-000000000a51";
        const A6: &str = "00000000-0000-4000-8000-000000000a52";
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let label = cat.label_for("Low Tide Records").unwrap();
        let harbour = cat.artist_for("Glass Harbour").unwrap();
        let (club, twin) = (cat.artist_for("Tape Club").unwrap(), cat.artist_apart("Tape Club").unwrap());
        let lane = cat.artist_for("Orbit Lane").unwrap();
        assert!(cat.set_reference(&ids::artist(lane), &Ref::new("musicbrainz", "artist", A6), "files").unwrap());
        let mb = |group: &str, title: &str, artist: &str, id: &str| LabelListed {
            reference: Ref::new("musicbrainz", "release-group", group),
            title: title.into(),
            artist: artist.into(),
            also: vec![Ref::new("musicbrainz", "artist", id)],
            ..Default::default()
        };
        let (a, a2) = (mb("00000000-0000-4000-8000-000000000a61", "Night Walks", "Kite Theory", A5), mb("00000000-0000-4000-8000-000000000a62", "Morning", "Orbit Lane", A5));
        cat.keep_label_rows(label, "musicbrainz", &[a.clone(), a2.clone()], None, true, 1).unwrap();
        let rows = [
            discogs_row("3700001", "Tide Tables", "Glass Harbour", "LTR-1"),
            discogs_row("3700002", "Signal", "Noor Field", "LTR-2"),
            discogs_row("3700003", "Undertow", "Tape Club", "LTR-3"),
            discogs_row("3700004", "Lamplight", "Glass Harbour (2)", "LTR-4"),
        ];
        cat.keep_label_rows(label, "discogs", &rows, None, true, 1).unwrap();
        let credits = |record: i64| cat.record_artists(cat.record(record).unwrap().unwrap().id).unwrap();
        let opened = open_row(&cat, label, &a);
        let kite = credits(opened);
        assert_eq!(kite.len(), 1, "(a) credited at once");
        assert_eq!(cat.subject_of(&Ref::new("musicbrainz", "artist", A5)).unwrap(), [ids::artist(kite[0])], "(a) the artist holding A5");
        assert_eq!(credits(open_row(&cat, label, &a)), kite, "(a) again: the same one, never two artists for one reference");
        let morning = credits(open_row(&cat, label, &a2));
        assert_eq!(morning, kite, "the holder of A5, whatever name the row gives");
        assert!(!morning.contains(&lane), "never the Orbit Lane holding another id");
        assert_eq!(credits(open_row(&cat, label, &rows[0])), [harbour], "(b) the one artist of the name");
        assert!(credits(open_row(&cat, label, &rows[1])).is_empty(), "(c) nobody yet");
        assert_eq!(cat.artist_of("Noor Field").unwrap(), None, "(c) no artist made by a name alone");
        let undertow = credits(open_row(&cat, label, &rows[2]));
        assert!(undertow.is_empty() && !undertow.contains(&club) && !undertow.contains(&twin), "(d) two of the name: nobody yet");
        assert!(credits(open_row(&cat, label, &rows[3])).is_empty(), "(e) a name Discogs marks as shared: nobody, though one Glass Harbour is here");
    }

    /// A2.3.1: MusicBrainz's label page (the browse asked with url-rels, one question) whose release of group G links Discogs release
    /// 3600010, and Discogs' rows holding 3600010 with its catalogue number: one row for that album, with G, 3600010 and the number.
    #[tokio::test]
    async fn a_musicbrainz_label_row_carries_its_discogs_release() {
        const G: &str = "00000000-0000-4000-8000-000000000a31";
        let answer = serde_json::json!({ "release-count": 1, "releases": [{
            "id": "00000000-0000-4000-8000-000000000a32",
            "release-group": { "id": G, "title": "Cenotes", "primary-type": "Album", "first-release-date": "2024-03-01" },
            "artist-credit": [{ "name": "Glass Harbour", "joinphrase": "", "artist": { "id": "00000000-0000-4000-8000-000000000a33" } }],
            "relations": [{ "type": "discogs", "url": { "resource": "https://www.discogs.com/release/3600010-Glass-Harbour-Cenotes" } }]
        }] });
        let listed = crate::app::discover::label_listings(&answer);
        assert_eq!(listed[0].says.iter().filter(|r| r.kind == "release").cloned().collect::<Vec<_>>(), [Ref::new("discogs", "release", "3600010")], "the link kept with the row");
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let label = cat.label_for("Wonderwheel Recordings").unwrap();
        let mb: Vec<LabelListed> = listed.into_iter().filter_map(listed_of_musicbrainz).collect();
        cat.keep_label_rows(label, "musicbrainz", &mb, None, true, 1).unwrap();
        cat.keep_label_rows(label, "discogs", &[discogs_row("3600010", "Cenotes", "Glass Harbour", "WW-010")], None, true, 1).unwrap();
        let page = settled(&cat, label, None, &[]).await;
        assert_eq!(page.not_had.len(), 1, "one row: {:?}", page.not_had);
        let row = &page.not_had[0];
        assert_eq!((row.release_group.as_deref(), row.discogs_release, row.catno.as_deref()), (Some(G), Some(3600010), Some("WW-010")));
    }

    /// A2.3.2: a label's rows by Glass Harbour: "Cenotes" (group G6) and "Cenotes" (a Discogs release, no master known): one row;
    /// "Tropical Fish" (a record) and two loose releases of that title: one row; "Orbital" (G7) and "Orbital" (G8): two rows. The row
    /// carries every catalogue number and its first reference of each source.
    #[tokio::test]
    async fn a_label_shows_one_row_for_one_artist_and_title_unless_two_albums_are_known() {
        let (g6, g7, g8) = ("00000000-0000-4000-8000-000000000a46", "00000000-0000-4000-8000-000000000a47", "00000000-0000-4000-8000-000000000a48");
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let label = cat.label_for("Wonderwheel Recordings").unwrap();
        let group = |g: &str, title: &str| LabelListed { reference: Ref::new("musicbrainz", "release-group", g), title: title.into(), artist: "Glass Harbour".into(), year: Some(2024), ..Default::default() };
        cat.keep_label_rows(label, "musicbrainz", &[group(g6, "Cenotes"), group(g7, "Orbital"), group(g8, "Orbital")], None, true, 1).unwrap();
        let fish = cat.record_for_tests("Tropical Fish", 0);
        cat.db_for_tests().execute("UPDATE records SET artist = 'Glass Harbour', year = 2024 WHERE id = ?1", [fish]).unwrap();
        assert!(cat.set_reference(&ids::record(fish), &Ref::new("discogs", "release", "3600050"), "link").unwrap());
        cat.tie_label(fish, label, "WW-050").unwrap();
        cat.keep_label_rows(
            label,
            "discogs",
            &[discogs_row("3600046", "Cenotes", "Glass Harbour", "WW-046"), discogs_row("3600051", "Tropical Fish", "Glass Harbour", "WW-051"), discogs_row("3600052", "Tropical Fish", "Glass Harbour", "WW-052")],
            None,
            true,
            1,
        )
        .unwrap();
        let page = settled(&cat, label, None, &[]).await;
        let named = |title: &str| page.not_had.iter().filter(|r| r.title == title).collect::<Vec<_>>();
        let cenotes = named("Cenotes");
        assert_eq!(cenotes.len(), 1, "one row for Cenotes: {:?}", page.not_had);
        assert_eq!((cenotes[0].release_group.as_deref(), cenotes[0].discogs_release, cenotes[0].catno.as_deref()), (Some(g6), Some(3600046), Some("WW-046")));
        let fishes = named("Tropical Fish");
        assert_eq!(fishes.len(), 1, "one row for Tropical Fish");
        assert_eq!(fishes[0].record.as_deref(), Some(ids::record(fish).as_str()), "the record's row");
        assert_eq!(fishes[0].catno.as_deref(), Some("WW-050, WW-051, WW-052"), "every catalogue number");
        assert_eq!(named("Orbital").len(), 2, "two release groups: two rows (X8)");
    }

    /// A2.3.3: the one row opens the record that holds any of its references, else one made with them all; the next open of any of
    /// them is that record; a label's rows still let go after thirty days (rows, not records).
    #[tokio::test]
    async fn one_label_row_opens_one_record() {
        const G: &str = "00000000-0000-4000-8000-000000000a71";
        let cat = Arc::new(Catalogue::open(None, Vec::new()).unwrap());
        let label = cat.label_for("Wonderwheel Recordings").unwrap();
        let mb = LabelListed { reference: Ref::new("musicbrainz", "release-group", G), title: "Cenotes".into(), artist: "Glass Harbour".into(), also: vec![Ref::new("discogs", "release", "3600070")], ..Default::default() };
        cat.keep_label_rows(label, "musicbrainz", &[mb], None, true, 1).unwrap();
        cat.keep_label_rows(label, "discogs", &[discogs_row("3600070", "Cenotes", "Glass Harbour", "WW-070")], None, true, 1).unwrap();
        let page = settled(&cat, label, None, &[]).await;
        assert_eq!(page.not_had.len(), 1);
        let row = &page.not_had[0];
        let o = Opened { release_group: row.release_group.clone(), discogs_master: row.discogs_master, discogs_release: row.discogs_release, title: row.title.clone(), artist: row.artist.clone(), year: row.year, label: Some(ids::label(label)) };
        let record = open_record(&cat, &o).unwrap().unwrap();
        let held: Vec<(String, String)> = cat.references_of(&ids::record(record)).unwrap().into_iter().map(|(r, _)| (r.kind, r.value)).collect();
        assert!(held.contains(&("release-group".into(), G.into())) && held.contains(&("release".into(), "3600070".into())), "made with them all: {held:?}");
        let by_release = Opened { discogs_release: Some(3600070), title: "Cenotes".into(), artist: "Glass Harbour".into(), ..Default::default() };
        assert_eq!(open_record(&cat, &by_release).unwrap(), Some(record), "any of its references: that record");
        assert_eq!(cat.forget_label_rows(1 + ROWS_KEPT + 1).unwrap(), 2, "rows let go after thirty days");
        assert!(cat.record(record).unwrap().is_some(), "the record stays");
    }
}
