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
fn listed_of_musicbrainz(l: Listing) -> Option<LabelListed> {
    let reference = l.refs.into_iter().next()?;
    Some(LabelListed { reference, title: l.title, artist: l.artist, year: l.year, catno: String::new(), kind: l.kind, cover: None })
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
    /// Its records by year, newest first (none last), then by title.
    pub rows: Vec<LabelEntry>,
    /// Whether a list of it is being read: the page asks again until none is.
    pub looking: bool,
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
    pub request: Option<super::requests::Mark>,
}

impl LabelEntry {
    /// A record's row: its own facts and references, had by the one had rule.
    fn of_record(q: &Requested) -> Self {
        let group = q.groups.first().cloned();
        let discogs = |kind: &str| q.refs.iter().find(|(s, k, _)| s == "discogs" && k == kind).and_then(|(.., v)| v.parse::<u64>().ok());
        let cover = match q.holder {
            Some(holder) => Some(format!("/api/albums/{}/{}/cover", crate::source::cratarr::KIND.id, ids::record(holder))),
            None => super::cover_cache::addresses_of(q.row.cover.as_deref(), group.as_deref(), q.bandcamp.as_deref()).into_iter().next(),
        };
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
        }
    }

    /// A source's row no record holds yet: not had.
    fn of_row(l: &LabelListed) -> Self {
        let mut e = LabelEntry { title: l.title.clone(), artist: l.artist.clone(), year: l.year, kind: l.kind.clone(), ..LabelEntry::default() };
        e.fill(l);
        e.cover = l.cover.clone().or_else(|| e.release_group.as_ref().map(|g| format!("/api/covers/release-group/{g}")));
        e
    }

    /// What a source's row of the same album adds: its catalogue number and its reference where the row has none, and the facts
    /// the record lacks.
    fn fill(&mut self, l: &LabelListed) {
        if self.catno.is_none() && !l.catno.is_empty() {
            self.catno = Some(l.catno.clone());
        }
        let r = &l.reference;
        match (r.source.as_str(), r.kind.as_str()) {
            ("musicbrainz", "release-group") if self.release_group.is_none() => self.release_group = Some(r.value.clone()),
            ("discogs", "master") if self.discogs_master.is_none() => self.discogs_master = r.value.parse().ok(),
            ("discogs", "release") if self.discogs_release.is_none() => self.discogs_release = r.value.parse().ok(),
            _ => {}
        }
        if self.year.is_none() {
            self.year = l.year;
        }
        if self.kind.is_none() {
            self.kind = l.kind.clone();
        }
        if self.artist.is_empty() {
            self.artist = l.artist.clone();
        }
    }
}

/// A label's rows from what the catalogue keeps (C56, `LabelKept`): each record once, as merged now, its catalogue number the
/// first its ties give; a source's row of a record (by its reference) is that record's row, and one no record holds is a row of
/// its own. By year, newest first, then by title and artist.
fn rows_of(kept: &LabelKept) -> Vec<LabelEntry> {
    let mut by_record: BTreeMap<i64, LabelEntry> = BTreeMap::new();
    let mut loose: Vec<LabelEntry> = Vec::new();
    for (record, catno) in &kept.tied {
        let Some(q) = kept.records.get(record) else { continue };
        let row = by_record.entry(q.row.id).or_insert_with(|| LabelEntry::of_record(q));
        if row.catno.is_none() && !catno.is_empty() {
            row.catno = Some(catno.clone());
        }
    }
    for (listed, record) in &kept.rows {
        match record.and_then(|r| kept.records.get(&r)) {
            Some(q) => by_record.entry(q.row.id).or_insert_with(|| LabelEntry::of_record(q)).fill(listed),
            None => loose.push(LabelEntry::of_row(listed)),
        }
    }
    let mut rows: Vec<LabelEntry> = by_record.into_values().chain(loose).collect();
    rows.sort_by(|a, b| b.year.cmp(&a.year).then_with(|| a.title.cmp(&b.title)).then_with(|| a.artist.cmp(&b.artist)));
    rows
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

/// How long a source that did not answer is left before an open asks it again; "More" asks at once.
const RETRY_SECS: i64 = 15 * 60;
/// The longest a page of a label's list may take: past it, it counts as not read, and `looking` ends.
const READ_AT_MOST: Duration = Duration::from_secs(if cfg!(test) { 5 } else { 300 });

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
    let refs: Vec<Ref> = tokio::task::spawn_blocking(move || c.references_of(&subject)).await??.into_iter().map(|(r, _)| r).collect();
    if let Some(id) = label_id_at(&refs, source.source()) {
        return Ok(Some(id));
    }
    let found = source.find(&label.name, &refs).await?;
    let id = label_id_at(found.iter().map(|(r, _)| r), source.source());
    let (c, subject) = (cat.clone(), ids::label(label.id));
    tokio::task::spawn_blocking(move || -> Result<()> {
        for (r, how) in &found {
            c.set_reference(&subject, r, how)?;
        }
        Ok(())
    })
    .await??;
    Ok(id)
}

/// A page of the label's list at `source` read and kept (C56): from its start (`from` none), where its list then starts again,
/// or from where it stands. A source that does not know the label keeps that, so it is not asked again for thirty days.
async fn read_page(cat: &Arc<Catalogue>, label: &LabelAsked, source: &dyn LabelList, from: Option<i64>) -> Result<()> {
    let (rows, next) = match known_at(cat, label, source).await? {
        Some(id) => source.page(&id, from).await?,
        None => (Vec::new(), None),
    };
    let (c, id, name, at) = (cat.clone(), label.id, source.source(), super::moment::now_secs());
    tokio::task::spawn_blocking(move || c.keep_label_rows(id, name, &rows, next, from.is_none(), at)).await?
}

/// A page of the label's list at `source` read in a task of its own (C56), unless one is being read (one at a time) or, but for
/// "More" (`by_hand`), one failed lately. Whether one is being read now.
fn start_read(cat: &Arc<Catalogue>, label: &Arc<LabelAsked>, source: &Arc<dyn LabelList>, from: Option<i64>, by_hand: bool, now: i64) -> bool {
    let key = (Arc::as_ptr(cat) as usize, label.id, source.source());
    {
        let mut all = readings().lock().unwrap_or_else(|p| p.into_inner());
        match all.get(&key) {
            Some(Reading::Now) => return true,
            Some(Reading::Failed(at)) if !by_hand && now - at <= RETRY_SECS => return false,
            _ => {}
        }
        all.insert(key, Reading::Now);
    }
    let (cat, label, source) = (cat.clone(), label.clone(), source.clone());
    tokio::spawn(async move {
        let mut settles = Settles(Some(key), false);
        match tokio::time::timeout(READ_AT_MOST, read_page(&cat, &label, source.as_ref(), from)).await {
            Ok(Ok(())) => settles.1 = true,
            Ok(Err(e)) => tracing::info!(target: "discover", label = label.id, source = source.source(), "a label's list not read: {e:#}"),
            Err(_) => tracing::info!(target: "discover", label = label.id, source = source.source(), "a label's list not read: no answer in time"),
        }
    });
    true
}

/// A label's page (C56): its rows as kept, at once (`rows_of`), while the first hundred of each source's list not read in thirty
/// days (`ROWS_KEPT`), or never, is read in the background; with `more`, the next hundred of that source's (OQ10 (a)). Whether a
/// list of it is being read (`looking`), so the page asks again until none is, and whether each source's reads on. None: no such
/// label.
pub async fn page(cat: Arc<Catalogue>, label: i64, more: Option<&str>, sources: &[Arc<dyn LabelList>]) -> Result<Option<LabelPage>> {
    let c = cat.clone();
    let Some(kept) = tokio::task::spawn_blocking(move || c.label_kept(label)).await?? else { return Ok(None) };
    let asked = Arc::new(LabelAsked { id: label, name: kept.label.name.clone() });
    let now = super::moment::now_secs();
    let mut looking = false;
    for source in sources.iter().filter(|s| s.on()) {
        let by_hand = more == Some(source.source());
        let fresh = kept.reads.get(source.source()).filter(|r| now - r.at <= ROWS_KEPT);
        // Never read, or thirty days ago: from its start. Read since: on from where it stands, for "More" alone.
        let from = match fresh {
            None => Some(None),
            Some(r) if by_hand => r.next.map(Some),
            Some(_) => None,
        };
        looking |= match from {
            Some(from) => start_read(&cat, &asked, source, from, by_hand, now),
            None => being_read(&cat, label, source.source()),
        };
    }
    let reads_on = |name: &str| sources.iter().any(|s| s.source() == name && s.on()) && kept.reads.get(name).is_some_and(|r| r.next.is_some());
    Ok(Some(LabelPage {
        id: ids::label(label),
        name: kept.label.name.clone(),
        rows: rows_of(&kept),
        looking,
        more: More { musicbrainz: reads_on("musicbrainz"), discogs: reads_on("discogs") },
        musicbrainz: label_id_at(&kept.refs, "musicbrainz"),
        discogs: label_id_at(&kept.refs, "discogs"),
    }))
}

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
/// does not (its type, its cover). Tied to the label it was opened from. Never by its names alone: a row with no reference, or no
/// title, is none. Blocking.
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
    let listing = Listing {
        artist: said(&o.artist, listed.as_ref().map(|(_, l)| &l.artist)),
        year: o.year.or(listed.as_ref().and_then(|(_, l)| l.year)),
        kind: listed.as_ref().and_then(|(_, l)| l.kind.clone()),
        cover: listed.as_ref().and_then(|(_, l)| l.cover.clone()),
        title,
        refs,
        ..Listing::default()
    };
    let Some(record) = cat.keep_listing(None, &[listing], "link")?.into_iter().next() else { return Ok(None) };
    if let Some(label) = label {
        let catno = listed.as_ref().filter(|(of, _)| *of == label).map(|(_, l)| l.catno.as_str()).unwrap_or_default();
        cat.tie_label(record, label, catno)?;
    }
    Ok(Some(record))
}

#[cfg(test)]
mod tests {
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

    fn titles(shown: &LabelPage) -> Vec<&str> {
        shown.rows.iter().map(|r| r.title.as_str()).collect()
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
        let listed: Vec<(&str, Option<i32>, Option<&str>, bool, Option<String>)> = shown.rows.iter().map(|r| (r.title.as_str(), r.year, r.catno.as_deref(), r.had, r.record.clone())).collect();
        assert_eq!(
            listed,
            [
                ("Undertow", Some(2019), Some("LTR-030"), false, Some(ids::record(wanted))),
                ("Paper Kites", Some(2016), None, false, None),
                ("Tide Tables", Some(2012), Some("LTR-012"), true, Some(ids::record(had))),
                ("Open Water", Some(2003), None, false, Some(ids::record(older))),
            ]
        );
        assert_eq!(shown.rows[0].release_group.as_deref(), Some(GROUP_1), "the source's row and the record are one row");
        assert_eq!(shown.rows[1].cover, Some(format!("/api/covers/release-group/{GROUP_2}")));
        assert_eq!(shown.rows[2].cover, Some(format!("/api/albums/cratarr/{}/cover", ids::record(had))));
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
        assert!(shown.rows.iter().all(|r| r.record.is_none() && !r.had), "{:?}", shown.rows);
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
        let listed: Vec<(&str, Option<String>, bool, Option<&str>)> = shown.rows.iter().map(|r| (r.title.as_str(), r.record.clone(), r.had, r.catno.as_deref())).collect();
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
        assert_eq!(first.rows.len(), 100, "the first hundred");
        assert_eq!(first.more, More { musicbrainz: true, discogs: false }, "Discogs does not know it");
        assert!(discogs.asked.lock().unwrap().is_empty(), "nor is it read there");
        assert!(first.rows.windows(2).all(|w| w[0].year >= w[1].year), "newest first");
        let asked = page(cat.clone(), label, Some("musicbrainz"), &sources).await.unwrap().unwrap();
        let again = page(cat.clone(), label, Some("musicbrainz"), &sources).await.unwrap().unwrap();
        assert!(asked.looking && again.looking);
        gate.add_permits(1);
        let all = settled(&cat, label, None, &sources).await;
        assert_eq!(all.rows.len(), 140, "the next hundred, here the last forty");
        assert_eq!(*musicbrainz.asked.lock().unwrap(), [None, Some(100)], "one read at a time: the second More started none");
        assert_eq!(all.more, More::default(), "at its end");
        settled(&cat, label, Some("musicbrainz"), &sources).await;
        assert_eq!(musicbrainz.asked.lock().unwrap().len(), 2, "no more to read");
        let known: Vec<(String, String)> = cat.references_of(&ids::label(label)).unwrap().into_iter().map(|(r, row)| (r.value, row.how)).collect();
        assert_eq!(known, [(LABEL_MBID.to_string(), "search".to_string())], "its MusicBrainz id found by its name, kept");
    }
}
