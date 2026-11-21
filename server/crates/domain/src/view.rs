//! What the server sends of an object (DOMAIN-CORE §6; decision 0104 point 5; SCENARIOS-DC1 RV1-RV10, TY1-TY5; OBJECT-MAP-2
//! §1): one shape per part, the same wherever the object shows, its TypeScript written by ts-rs into the page's `generated/`.
//!
//! - **The entity** ([`Entity`]): a page's or a sheet's object. Its properties, its state as said ([`Shown`]), the actions this
//!   viewer may take now ([`ActionView`], each with its address: Siren's shape, DOMAIN-CORE B.6), the sections that apply in the
//!   place's order ([`Section`]), its versions, and the links it goes to. Read at the change log's rev (`rev`, LV2) and keyed by
//!   what is no row (`epoch`, LV4).
//! - **The card, the row and the chip** ([`Card`], [`Row`], [`Chip`]): one shape each, thin (M3): no actions, sections, sources
//!   or links. A list's object reads its entity only when a sheet or its page opens it.
//! - **Times are epoch seconds and revs numbers** (RV8, TY3): the page says them.

use serde::Serialize;

pub use crate::when::Section;

/// The state as said (RS2): what it is, its words, and the time the words end on ("Was in MORE until …"), which the page says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct Shown {
    pub is: Is,
    pub words: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub at: Option<i64>,
}

/// A record's state by name (the holding's, HD1): one word each, the words beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RecordIs"))]
#[cfg_attr(test, ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Is {
    NotHad,
    Partly,
    Arrived,
    Had,
    Elsewhere,
    Retired,
    // Only guesses' lists kept (VR12 rule 6): not known whole, no count until a pick.
    Unknown,
}

/// How an action is asked of the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub enum Method {
    #[serde(rename = "GET")]
    Get,
    #[serde(rename = "POST")]
    Post,
    #[serde(rename = "PUT")]
    Put,
}

/// The menu's group an entry sits in (decision 0058: A the large actions, then B to H), the page's `GROUP_ORDER`'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "EntryGroup"))]
#[cfg_attr(test, ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Group {
    /// The large actions (the Play row).
    Play,
    /// Want or Requested, a card of its own (F36).
    Want,
    Go,
    Discover,
    Feel,
    Collect,
    Take,
    Know,
    Change,
    Library,
    Remove,
}

/// What an action's body carries (Siren's fields): the record's own handlers' as they are today, each left out where none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "ActionFields"))]
#[cfg_attr(test, ts(export))]
pub struct Fields {
    /// `/api/play`'s: what plays (`{ kind: "album", source, id }`) and how.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub what: Option<What>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub how: Option<String>,
    /// The copy it acts on, by its library's id (it holds `/`, so never in the path).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub from: Option<String>,
    /// Where it goes, by its library's id.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub to: Option<String>,
    /// A Want's record (`al-<n>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub record: Option<String>,
}

/// What `/api/play` plays: a record of a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PlayWhat"))]
#[cfg_attr(test, ts(export))]
pub struct What {
    pub kind: String,
    pub source: String,
    pub id: String,
}

/// One action this viewer may take now (RV10): its entry's name, words, icon and group (`words::entry`), the section it is drawn
/// in, and its address: today's handler, its copy named where it acts on one. `confirm`: the yeses `decide` asks for first, in order
/// (RS25: a main library's Delete asks twice, decision 0034).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct ActionView {
    pub name: String,
    pub title: String,
    pub icon: String,
    pub group: Group,
    pub section: Section,
    pub method: Method,
    pub href: String,
    #[serde(skip_serializing_if = "is_empty")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub fields: Option<Fields>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub confirm: Option<Vec<String>>,
    /// Whether it works with Cratarr away (RS8, RS17): the device offers it from its cached entity only where this is true.
    pub away: bool,
}

fn is_empty(f: &Option<Fields>) -> bool {
    f.as_ref().is_none_or(|f| *f == Fields::default())
}

/// A place it goes, drawn in a section like an action but asking the server nothing (Siren's links): Go to, View, Share, Web
/// search. `href` is the page's address (shared/addresses.json) or an outside one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "LinkView"))]
#[cfg_attr(test, ts(export))]
pub struct Link {
    pub name: String,
    pub title: String,
    pub icon: String,
    pub group: Group,
    pub section: Section,
    pub href: String,
    /// Whether it works with Cratarr away (RS17): a navigation and the web searches do.
    pub away: bool,
}

/// A fact as the view carries it (FA1): its value and the sources it is by. `by` is `resolve()`'s (VR01): until it lands, the
/// hand where a hand set it, else none said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct FactView<T> {
    pub value: T,
    pub by: Vec<String>,
}

impl<T> FactView<T> {
    pub fn of(value: T) -> Self {
        Self { value, by: Vec::new() }
    }
}

/// An artist or a label as a record names it: by its id for life and its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct Named {
    pub id: String,
    pub name: String,
    /// A label's catalogue numbers on this record, joined.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub catno: Option<String>,
}

/// A record's properties (RV1): each a fact. `type` is its release type; `credit` its artists as one text, by the one formatter
/// (rules' `credit_text`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct RecordProperties {
    #[serde(rename = "type")]
    pub kind: FactView<Option<String>>,
    pub title: FactView<String>,
    pub artists: FactView<Vec<Named>>,
    pub credit: String,
    pub year: FactView<Option<i32>>,
    pub labels: FactView<Vec<Named>>,
    pub cover: FactView<Option<String>>,
}

/// One line of the Versions section (RS4): a copy, or an edition (another record of the album). `plays` marks the one that plays
/// here; `action` what a tap does (Play from it, or open the edition), none on the line that plays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct Version {
    pub kind: VersionKind,
    /// A copy's library's id; an edition's record id.
    pub id: String,
    pub record: String,
    /// Its library's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub library: Option<String>,
    /// "FLAC", "MP3 320": never "kb/s" (SB23).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub quality: Option<String>,
    /// Its tracks, where its count is not the record's.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub tracks: Option<u16>,
    pub plays: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub action: Option<String>,
    /// A tracklist's sources, in the order asked (VR3, VR4: "9 tracks · MusicBrainz, Bandcamp"), each named.
    pub sources: Vec<String>,
}

/// What a version line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
#[serde(rename_all = "snake_case")]
pub enum VersionKind {
    Copy,
    Edition,
    Tracklist,
}

/// A page's or a sheet's object (RV1): one answer for a record, had or not (RV2), the page and its sheet reading one (RV9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct Entity<P> {
    pub class: String,
    pub id: String,
    /// The change log's newest rev, read before its rows (LV2).
    pub rev: i64,
    /// What it was read under that is no row (LV4): the place's libraries, the viewer's role and permissions.
    pub epoch: String,
    pub properties: P,
    pub state: Shown,
    pub actions: Vec<ActionView>,
    pub sections: Vec<Section>,
    pub entities: Vec<Version>,
    pub links: Vec<Link>,
}

/// A record in a list (RV5, M3): thin, its properties' values and its state line; no actions, sections, sources or links.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RecordRow"))]
#[cfg_attr(test, ts(export))]
pub struct Row {
    pub id: String,
    pub rev: i64,
    pub epoch: String,
    pub title: String,
    pub credit: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub cover: Option<String>,
    pub state: Shown,
}

/// A record as a tile in a grid or a shelf (OBJECT-MAP-2 §2.1; RS6, "one card per album"): the row's fields and how many versions
/// it folds ("2 versions" from two on).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RecordCard"))]
#[cfg_attr(test, ts(export))]
pub struct Card {
    #[serde(flatten)]
    pub row: Row,
    pub versions: u16,
}

/// A small link naming a record (OBJECT-MAP-2 §2.1's chip): its id, its title, and its type's word ("EP"), which opens it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RecordChip"))]
#[cfg_attr(test, ts(export))]
pub struct Chip {
    pub id: String,
    pub title: String,
    pub word: String,
}
