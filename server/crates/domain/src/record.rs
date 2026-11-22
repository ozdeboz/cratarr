//! The record as one object (OBJECT-MAP-2 §2.1; SCENARIOS-DC1 RV1-RV10, CN4, CN6, RS1-RS5, RS14; decision 0104 points 4-6):
//! its entity, card, row and chip worked out from what the store reads of it ([`RecordFacts`]), for one viewer in one place.
//! Pure: the app reads the facts in one read (HD14) after the change log's rev (LV2), and hands them here.
//!
//! - **Its state** is the holding (`holding()`, rules) in the place's library set (HD7), said by the one wording table
//!   (`words::shown`).
//! - **Its actions** are what `decide()` allows of every action the record could take here: the server sends them, each with its
//!   address (today's handler, its copy named where it acts on one), and the page draws them (CN9, CN13). A yes asked first is
//!   its `confirm`.
//! - **Its sections** are the when-table's (`when::sections`): the one order, the place's first after the head, only those that
//!   hold something.
//! - **Its versions** are its copies in the place's set and its editions, the one that plays marked (RS4, RS5).

use cratarr_rules::holding::{CopyFacts, Holding, Kept, Library, On, holding_of};
use cratarr_rules::names::credit_text;

use crate::can::{Action, ArrivalFacts, Decision, Record, RecordCopy, Target, Viewer, decide};
use crate::view::{ActionView, Card, Chip, Entity, FactView, Fields, Link, Method, Named, RecordProperties, Row, Section, Version, VersionKind, What};
use crate::when::{Place, library_set, play_order, sections};
use crate::words::{Says, entry, from_copy, record_word, shown, titled};

/// Cratarr's own records' source, as the page's and the handlers' addresses name it.
pub const SOURCE: &str = "cratarr";

/// A record's id as every answer spells it (TY3): `al-<n>`.
pub fn record_id(id: i64) -> String {
    format!("al-{id}")
}

/// A library as the record's facts name it: its catalogue folder, its id (the page's and the handlers' `from` and `to`), its
/// name as this viewer reads it ("Your music" for theirs), and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryOf {
    pub folder: i64,
    pub id: String,
    pub name: String,
    pub library: Library,
}

/// One copy of a record as the store reads it: the holding's facts, its quality as a line says it ("FLAC", "MP3 320"), when it
/// came, the conversion at work on it, and its arrival where it is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyOf {
    pub facts: CopyFacts,
    pub quality: Option<String>,
    pub added: i64,
    pub job: Option<u64>,
    pub arrival: Option<ArrivalFacts>,
}

/// Another record of its album (an edition, F12): its id, its tracks, and its copy that plays (library and quality).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditionOf {
    pub id: i64,
    pub tracks: u16,
    pub library: Option<String>,
    pub quality: Option<String>,
}

/// What the store reads of a record for its views (HD14: one read).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordFacts {
    pub id: i64,
    pub title: String,
    /// Its release type ("Album", "EP"…), and its secondary ones ("Compilation"…) as a hand set them (where `hand` names
    /// "secondary": final for its kind, VR49).
    pub kind: Option<String>,
    pub secondary: Vec<String>,
    pub year: Option<i32>,
    pub artists: Vec<Named>,
    pub labels: Vec<Named>,
    pub genres: Vec<String>,
    pub cover: Option<String>,
    /// The facts a hand set ("title", "year", "type"…): their `by`.
    pub hand: Vec<String>,
    /// Every copy the store knows, present or gone.
    pub copies: Vec<CopyOf>,
    /// The shortest list that judges it, in tracks (the pick's, where one judges).
    pub shortest: Option<u16>,
    /// The only lists it keeps are guesses' (VR12 rule 6): not known whole.
    pub only_guesses: bool,
    /// An administrator picked the list it counts by.
    pub picked: bool,
    /// Every list it keeps, by reference (VR01), and the pick among them: its versions and its tracklist fact (VR38).
    pub lists: Vec<crate::facts::List>,
    pub pick: Option<crate::facts::Reference>,
    /// Each source's year and type of it, per reference, each a guess's or not (FA15, VR40): what `resolve()` reads (VR39).
    pub years: Vec<crate::facts::Said<i32>>,
    pub types: Vec<crate::facts::Said<String>>,
    /// Each service's secondary types of it, per reference, each a guess's or not, and the rule's (rule 8's compilation): with the
    /// hand's, what `secondaries_now()` reads (VR49).
    pub secondaries: Vec<crate::facts::Said<Vec<String>>>,
    pub rule_secondary: Vec<String>,
    /// The release type rule's type over its picked list's titles and lengths (VR39, M10), where one is picked.
    pub picked_kind: Option<String>,
    /// The catalogue folder of the copy that plays for this viewer in this place (`copy_for` over the place's set), if one does.
    pub plays: Option<i64>,
    /// A page that previews it (Bandcamp's).
    pub preview: Option<String>,
    /// The open requests of it (or of its release group): each asker and the request's id (`rq-<n>`).
    pub requests: Vec<(String, String)>,
    /// What Replace in may put in a main library's place (RY01's own copies, an arrival's better copy): (from, into) by folder.
    pub replacing: Vec<(i64, i64)>,
    pub editions: Vec<EditionOf>,
    /// Every library a copy may go to or lies in.
    pub libraries: Vec<LibraryOf>,
    /// What its credits are worked out from (SCENARIOS-CRED): every source's credits, the artists here they may name, its tracks.
    pub credits: crate::credits::Credits,
}

/// Who sees it and from where: the viewer, the place, the libraries their switch has off, the change log's rev it was read at.
pub struct Seen<'a> {
    pub viewer: &'a Viewer,
    pub place: &'a Place,
    pub off: &'a [i64],
    pub rev: i64,
}

impl RecordFacts {
    fn library(&self, folder: i64) -> Option<&LibraryOf> {
        self.libraries.iter().find(|l| l.folder == folder)
    }

    fn library_name(&self, folder: i64) -> String {
        self.library(folder).map_or_else(|| format!("library {folder}"), |l| l.name.clone())
    }

    fn word(&self) -> &'static str {
        let secondary: Vec<String> = self.secondaries_now().into_iter().map(|s| s.word).collect();
        record_word(self.kind_now().value.as_deref(), &secondary)
    }

    /// Its secondary types (VR49): the hand's list where one is set, final; else the union of its services' (no guess's) and the
    /// rule's, each word naming its sources. One rule for the Head, References' Type item, the served words and the device's index.
    pub fn secondaries_now(&self) -> Vec<crate::facts::Secondary> {
        let hand = self.hand.iter().any(|h| h == "secondary").then_some(self.secondary.as_slice());
        crate::facts::secondaries(hand, &self.secondaries, &self.rule_secondary)
    }

    /// Whether a copy of it is present anywhere.
    fn had(&self) -> bool {
        self.copies.iter().any(|c| c.facts.present > 0)
    }

    /// Its year (FA1, FA13, VR39): the hand's; a had record's by its files; else its sources' as equals, each read per reference, a
    /// guess's never: the pick's where a list is picked, the one they agree on, none while they differ (their versions, none
    /// chosen; the Head shows no year until a hand picks, OQ-VR-9 (c)). A record no source has said a year of keeps the one it
    /// was made with.
    pub fn year_now(&self) -> crate::facts::Resolved<i32> {
        let hand = if self.hand.iter().any(|h| h == "year") { self.year } else { None };
        crate::facts::record_year(hand, self.had(), self.year, &self.years, self.pick.as_ref())
    }

    /// Its release type (FA13, VR39): the hand's; a had record's by the release type rule over its copy (as built); a record not
    /// had's by the rule over its picked list, else its sources' type facts where they all agree, else none until a hand picks, as
    /// the year (never a source by its name or the order asked). A record no source has said a type of keeps the one it was made with. One rule for the Head, its
    /// rows and the device's index.
    pub fn kind_now(&self) -> crate::facts::Resolved<String> {
        let hand = if self.hand.iter().any(|h| h == "type") { self.kind.clone() } else { None };
        let rule = if self.had() { self.kind.clone() } else { None };
        if hand.is_none() && rule.is_none() && self.picked_kind.is_none() && !self.types.iter().any(|s| !s.guessed && s.value.is_some()) {
            return crate::facts::Resolved { value: self.kind.clone(), by: Vec::new(), versions: Vec::new() };
        }
        crate::facts::resolve_type(hand.as_deref(), rule.as_deref(), self.picked_kind.as_deref(), &self.types)
    }

    /// Its present copies: what its actions name by their place in this list.
    fn present(&self) -> Vec<&CopyOf> {
        self.copies.iter().filter(|c| c.facts.present > 0).collect()
    }

    fn facts(&self) -> Vec<CopyFacts> {
        self.copies.iter().map(|c| c.facts.clone()).collect()
    }

    /// What judges its copies whole (VR12 rules 1, 5, 6).
    pub fn kept(&self) -> Kept {
        if self.only_guesses { Kept::OnlyGuesses } else { Kept::of(self.shortest) }
    }

    /// What `decide` reads of it for this viewer: its present copies, the one that plays here, its preview, its holding as the
    /// requests read it (every library, HD7), its open requests.
    pub fn record(&self, viewer: &Viewer) -> Record {
        let present = self.present();
        Record {
            copies: present
                .iter()
                .map(|c| RecordCopy { folder: c.facts.folder, library: c.facts.library.clone(), added: c.added, converting: c.job.is_some(), arrival: c.arrival.clone() })
                .collect(),
            plays: self.plays.and_then(|f| present.iter().position(|c| c.facts.folder == f)),
            preview: self.preview.is_some(),
            holding: holding_of(&self.facts(), self.kept(), &viewer.account, &On::all()),
            open_requests: self.requests.iter().map(|(asker, _)| asker.clone()).collect(),
        }
    }

    /// The holding in the place's set (HD7).
    pub fn holding(&self, seen: &Seen) -> Holding {
        holding_of(&self.facts(), self.kept(), &seen.viewer.account, &library_set(seen.place, seen.off))
    }
}

/// What a view was read under that is no row (LV4): the libraries the place counts, the viewer's role, scope and permissions. A
/// new epoch is a new key: a switch, a right or a key's scope changing reads every record again.
pub fn epoch(viewer: &Viewer, place: &Place, off: &[i64]) -> String {
    let mut without = library_set(place, off).without;
    without.sort_unstable();
    let off: Vec<String> = without.iter().map(i64::to_string).collect();
    let may = [("request", viewer.may.request), ("offline", viewer.may.offline), ("playlists", viewer.may.playlists)];
    let may: Vec<&str> = may.iter().filter(|(_, on)| *on).map(|(w, _)| *w).collect();
    format!("{}|{:?}|{:?}|{}|{}", viewer.account, viewer.role, viewer.scope, may.join(","), off.join(","))
}

/// One candidate: the action `decide` judges, its entry's name, and its title's blank.
struct Candidate {
    action: Action,
    name: &'static str,
    fill: String,
    section: Section,
}

fn target(l: &LibraryOf) -> Target {
    Target { folder: l.folder, library: l.library.clone() }
}

/// Every action the record could take here, before `decide` (CN1's rows; CN4, CN6's Library section).
fn candidates(r: &RecordFacts, seen: &Seen) -> Vec<Candidate> {
    let word = r.word().to_string();
    let c = |action: Action, name: &'static str, fill: String, section: Section| Candidate { action, name, fill, section };
    let mut out = Vec::new();
    for name in play_order(seen.place) {
        let action = match name {
            "play" => Action::Play,
            "shuffle" => Action::Shuffle,
            "play_next" => Action::PlayNext,
            "add_to_queue" => Action::AddToQueue,
            "start_radio" => Action::StartRadio,
            "want" => Action::Want,
            "requested" => Action::Requested,
            // Resume reads the account's resume point: the page's until it is a fact here.
            _ => continue,
        };
        out.push(c(action, name, String::new(), Section::Play));
    }
    let present = r.present();
    let on = library_set(seen.place, seen.off);
    let several = present.len() > 1;
    // A record of several copies names the copy each entry acts on (copiesRule's `named`, the owner testing 0.16.0).
    let named = |name: &'static str, to: &LibraryOf, from: i64| if several { (name, from_copy(&to.name, &r.library_name(from))) } else { (name, to.name.clone()) };
    // The Library section (CN6, RS7): each present copy's. Its targets are the libraries that hold no copy of it, never Incoming
    // (F34); an Incoming copy's, a main library or the viewer's own music (decision 0101 point 2). A copy being converted offers
    // only Cancel conversion.
    for (i, copy) in present.iter().enumerate() {
        let here = copy.facts.folder;
        // Play from: a record of two copies or more, each on here but the one that plays (copiesRule's `playFromEntries`).
        if several && on.counts(here) && Some(here) != r.plays {
            out.push(c(Action::PlayFrom { copy: i }, "play_from", r.library_name(here), Section::Library));
        }
        if copy.job.is_some() {
            out.push(c(Action::CancelConversion { copy: i }, "cancel_conversion", String::new(), Section::Library));
            continue;
        }
        let waiting = matches!(copy.facts.library, Library::Incoming { .. });
        let open = |l: &&LibraryOf| !present.iter().any(|p| p.facts.folder == l.folder) && !matches!(l.library, Library::Incoming { .. });
        for to in r.libraries.iter().filter(open) {
            if waiting {
                if matches!(to.library, Library::Main) || seen.viewer.own == Some(to.folder) {
                    let (name, fill) = named("add_to", to, here);
                    out.push(c(Action::AddTo { copy: i, to: target(to) }, name, fill, Section::Arrival));
                }
                continue;
            }
            let moves = matches!(copy.facts.library, Library::Main) && matches!(to.library, Library::Main);
            let (action, name) = if moves { (Action::MoveTo { copy: i, to: target(to) }, "move_to") } else { (Action::SaveTo { copy: i, to: target(to) }, "save_to") };
            let (name, fill) = named(name, to, here);
            out.push(c(action, name, fill, Section::Library));
        }
        for (_, into) in r.replacing.iter().filter(|(from, _)| *from == here) {
            let Some(into) = r.library(*into) else { continue };
            let action = if waiting { Action::Replace { copy: i, into: target(into) } } else { Action::ReplaceIn { into: target(into) } };
            let (name, fill) = named("replace_in", into, here);
            out.push(c(action, name, fill, if waiting { Section::Arrival } else { Section::Library }));
        }
        if waiting {
            out.push(c(Action::DeleteArrival { copy: i }, "delete_arrival", String::new(), Section::Arrival));
        } else {
            let (name, fill) = if several { ("delete_from", r.library_name(here)) } else { ("delete", word.clone()) };
            out.push(c(Action::Delete { copy: i }, name, fill, Section::Library));
        }
    }
    for (action, name) in [(Action::Favourite, "favourite"), (Action::Rating, "rating"), (Action::AddToPlaylist, "add_to_playlist")] {
        out.push(c(action, name, String::new(), Section::Go));
    }
    // On this device (RS8): Sync and Download… where it doesn't keep the record, Remove from this device where it does; which of
    // them shows is the device's own kept state, never a right: the server sends the three by the one right (CN1's `sync`).
    for (action, name) in [(Action::Download, "download"), (Action::Sync, "sync"), (Action::Sync, "remove_from_device")] {
        out.push(c(action, name, String::new(), Section::Device));
    }
    for (action, name) in [(Action::Edit, "edit"), (Action::ChangeCover, "change_cover")] {
        out.push(c(action, name, String::new(), Section::Go));
    }
    if matches!(r.holding(seen), Holding::Partly { .. }) {
        out.push(c(Action::MissingTracks, "missing_tracks", String::new(), Section::Go));
    }
    out
}

/// A value as one segment of a path: every byte but a letter, a digit, `-`, `.`, `_` and `~` percent-encoded.
fn encoded(value: &str) -> String {
    value.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { char::from(b).to_string() } else { format!("%{b:02X}") }).collect()
}

/// The address of an action on record `id` (RV10): today's handler, as the page calls it, its copy named by its library's id.
fn address(r: &RecordFacts, action: &Action) -> Option<(Method, String, Fields)> {
    let id = record_id(r.id);
    let album = format!("/api/albums/{SOURCE}/{id}");
    let present = r.present();
    let library_id = |copy: usize| present.get(copy).and_then(|c| r.library(c.facts.folder)).map(|l| l.id.clone());
    let play = |how: &str, source: &str, of: &str, from: Option<String>| {
        let what = What { kind: "album".into(), source: source.into(), id: of.into() };
        Some((Method::Post, "/api/play".to_string(), Fields { what: Some(what), how: Some(how.into()), from, ..Fields::default() }))
    };
    match action {
        // Play and Play from: the record's tracks (its view), queued by the page, from the named copy.
        Action::Play => match (&r.plays, &r.preview) {
            // A preview: its Bandcamp album's view, as the page reads it (a GET, so a reading key's too: CHECK-DC1's ruling 1).
            (None, Some(page)) => Some((Method::Get, format!("/api/albums/bandcamp/{}", encoded(page)), Fields::default())),
            _ => Some((Method::Get, album.clone(), Fields::default())),
        },
        Action::Shuffle => play("shuffle", SOURCE, &id, None),
        Action::PlayNext => play("next", SOURCE, &id, None),
        Action::AddToQueue => play("last", SOURCE, &id, None),
        Action::StartRadio => Some((Method::Post, "/api/radio/next".into(), Fields { what: Some(What { kind: "album".into(), source: SOURCE.into(), id }), ..Fields::default() })),
        Action::PlayFrom { copy } => Some((Method::Get, album.clone(), Fields { from: library_id(*copy), ..Fields::default() })),
        Action::Want => Some((Method::Post, "/api/requests/want".into(), Fields { record: Some(id), ..Fields::default() })),
        Action::Requested => {
            let rq = r.requests.first().map(|(_, rq)| rq.clone())?;
            Some((Method::Get, format!("/api/requests/{rq}"), Fields::default()))
        }
        Action::Favourite => Some((Method::Post, format!("{album}/favorite"), Fields::default())),
        Action::Rating => Some((Method::Post, format!("{album}/rating"), Fields::default())),
        // The record's tracks, which the page adds to a playlist and the device keeps.
        Action::AddToPlaylist | Action::Sync => Some((Method::Get, album.clone(), Fields::default())),
        Action::Download => Some((Method::Post, "/api/export".into(), Fields { what: Some(What { kind: "album".into(), source: SOURCE.into(), id }), ..Fields::default() })),
        Action::Edit => Some((Method::Put, "/api/albums/overrides".into(), Fields { record: Some(id), ..Fields::default() })),
        Action::ChangeCover => Some((Method::Post, format!("{album}/covers"), Fields::default())),
        Action::MissingTracks => Some((Method::Post, "/api/track-search".into(), Fields { record: Some(id), ..Fields::default() })),
        Action::SaveTo { copy, to } | Action::MoveTo { copy, to } | Action::AddTo { copy, to } => {
            let to = r.library(to.folder)?.id.clone();
            Some((Method::Post, format!("{album}/move"), Fields { from: library_id(*copy), to: Some(to), ..Fields::default() }))
        }
        Action::ReplaceIn { into } => {
            let from = r.replacing.iter().find(|(_, i)| *i == into.folder).and_then(|(f, _)| r.library(*f)).map(|l| l.id.clone());
            Some((Method::Post, format!("/api/incoming/replace/{SOURCE}/{id}"), Fields { from, to: Some(r.library(into.folder)?.id.clone()), ..Fields::default() }))
        }
        Action::Replace { copy, into } => {
            Some((Method::Post, format!("/api/incoming/replace/{SOURCE}/{id}"), Fields { from: library_id(*copy), to: Some(r.library(into.folder)?.id.clone()), ..Fields::default() }))
        }
        Action::CancelConversion { copy } => {
            let job = present.get(*copy)?.job?;
            Some((Method::Post, format!("/api/incoming/jobs/{job}/cancel"), Fields::default()))
        }
        Action::Delete { copy } => Some((Method::Post, format!("{album}/delete"), Fields { from: library_id(*copy), ..Fields::default() })),
        // An arrival's Delete goes where its asker's does: the record's own route, its copy named.
        Action::DeleteArrival { copy } => Some((Method::Post, format!("{album}/delete"), Fields { from: library_id(*copy), ..Fields::default() })),
        // An arrival's Undo is its journal event's (AR4, the Arrival section's, with the command path).
        Action::Undo { .. } => None,
        Action::Resume | Action::Share | Action::WebSearch | Action::GeniusSearch | Action::GoTo | Action::View | Action::SoundsLike | Action::RemoveFromDevice => None,
    }
}

/// A record's handler, as it is asked (CN9): which of today's routes, before its body is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handler {
    Favourite,
    Rating,
    Edit,
    ChangeCover,
    /// `…/move`: Save to, Move to, or an arrival's Add to, by the copy's library and the target's.
    Move,
    /// `/api/incoming/keep/…`: an arrival added.
    Keep,
    /// `/api/incoming/replace/…`: a better copy in a main library's copy's place.
    Replace,
    /// `…/delete`: a kept copy, or an arrival by whoever asked for it.
    Delete,
    /// `/api/incoming/delete/…`: an arrival.
    DeleteArrival,
}

/// The action a handler is asked, from its route and its body's `from` and `to` (library ids): the address read back, so the
/// handler's `decide` judges what the view offered (CN9). None where it names no copy of a record of several, or a target that is
/// no library here yet (own music before its first use): the handler's own words answer those.
pub fn action_of(r: &RecordFacts, handler: Handler, from: Option<&str>, to: Option<&str>) -> Option<Action> {
    // The record's own: no copy to name.
    match handler {
        Handler::Favourite => return Some(Action::Favourite),
        Handler::Rating => return Some(Action::Rating),
        Handler::Edit => return Some(Action::Edit),
        Handler::ChangeCover => return Some(Action::ChangeCover),
        _ => {}
    }
    let present = r.present();
    let copy = match from {
        Some(id) => present.iter().position(|c| r.library(c.facts.folder).is_some_and(|l| l.id == id)).unwrap_or(present.len()),
        None if present.len() == 1 => 0,
        None => return None,
    };
    let waiting = present.get(copy).is_some_and(|c| matches!(c.facts.library, Library::Incoming { .. }));
    let target = |id: Option<&str>| -> Option<Target> { r.libraries.iter().find(|l| Some(l.id.as_str()) == id).map(target) };
    Some(match handler {
        Handler::Favourite => Action::Favourite,
        Handler::Rating => Action::Rating,
        Handler::Edit => Action::Edit,
        Handler::ChangeCover => Action::ChangeCover,
        Handler::Move | Handler::Keep => {
            let to = target(to)?;
            let from_main = present.get(copy).is_some_and(|c| matches!(c.facts.library, Library::Main));
            if waiting || handler == Handler::Keep {
                Action::AddTo { copy, to }
            } else if from_main && matches!(to.library, Library::Main) {
                Action::MoveTo { copy, to }
            } else {
                Action::SaveTo { copy, to }
            }
        }
        Handler::Replace => {
            let into = target(to)?;
            // A copy named and gone is named still: `decide` says it is gone (RS19).
            if waiting || copy >= present.len() { Action::Replace { copy, into } } else { Action::ReplaceIn { into } }
        }
        Handler::Delete | Handler::DeleteArrival if waiting => Action::DeleteArrival { copy },
        Handler::Delete | Handler::DeleteArrival => Action::Delete { copy },
    })
}

/// The handler an action's address asks (`action_of`'s other way): none for what plays, goes or asks no record handler.
pub fn handler_of(action: &Action) -> Option<Handler> {
    Some(match action {
        Action::Favourite => Handler::Favourite,
        Action::Rating => Handler::Rating,
        Action::Edit => Handler::Edit,
        Action::ChangeCover => Handler::ChangeCover,
        Action::SaveTo { .. } | Action::MoveTo { .. } | Action::AddTo { .. } => Handler::Move,
        Action::ReplaceIn { .. } | Action::Replace { .. } => Handler::Replace,
        Action::Delete { .. } | Action::DeleteArrival { .. } => Handler::Delete,
        _ => return None,
    })
}

/// Every action the record could take here with what `decide` says of each (CN9): the view offers the ones allowed, a handler
/// refuses the others in the table's words.
pub fn judged(r: &RecordFacts, seen: &Seen) -> Vec<(Action, Decision)> {
    let record = r.record(seen.viewer);
    candidates(r, seen).into_iter().map(|c| (c.action.clone(), decide(seen.viewer, &c.action, &record))).collect()
}

/// The actions this viewer may take on it here (CN9: every one `decide` allows; nothing else), each with its address and its
/// confirm where `decide` asks a yes first, in the Play row's order, then by section and group.
pub fn actions(r: &RecordFacts, seen: &Seen) -> Vec<ActionView> {
    let record = r.record(seen.viewer);
    let mut out = Vec::new();
    for cand in candidates(r, seen) {
        let confirm = match decide(seen.viewer, &cand.action, &record) {
            Decision::Allow => None,
            Decision::AskFirst(_) => Some(confirm_steps(r, &cand)),
            Decision::Deny(_) => continue,
        };
        let Some((method, href, fields)) = address(r, &cand.action) else { continue };
        let e = entry(cand.name);
        // Want's and Requested's own card (F36) is the Play row's last (or first, from Discover: RS3).
        out.push(ActionView {
            name: cand.name.to_string(),
            title: titled(cand.name, &cand.fill),
            icon: e.icon.to_string(),
            group: e.group,
            section: cand.section,
            method,
            href,
            fields: Some(fields),
            confirm,
            away: e.away,
        });
    }
    out
}

/// The yeses an action asks first, in order (RS25): its own question; and a Delete of the main library's copy (the first main
/// library, which takes every fallback) asks again, decision 0034's extra step ("Primary should get an extra step in the
/// confirmation"). The page asks each in turn and sends nothing on a no; it reads no library's kind to decide them.
fn confirm_steps(r: &RecordFacts, cand: &Candidate) -> Vec<String> {
    let mut steps = vec![format!("{}?", titled(cand.name, &cand.fill))];
    if let Action::Delete { copy } = cand.action {
        let primary = r.libraries.iter().find(|l| l.library == Library::Main).map(|l| l.folder);
        let folder = r.present().get(copy).map(|c| c.facts.folder);
        if folder.is_some() && folder == primary {
            steps.push(format!("{} is your main library. Delete it anyway?", r.library_name(folder.unwrap_or_default())));
        }
    }
    steps
}

/// Where it goes from here (Siren's links): View (never on its own page, RS14), Go to its artists and genres, Sounds like this,
/// Share and the web searches. `href` is the panel's subject the page opens over where it is (lib/panel: `album:`, `artist:`,
/// `crate:genre:`), or an outside address.
pub fn links(r: &RecordFacts, seen: &Seen) -> Vec<Link> {
    let id = record_id(r.id);
    let link = |name: &str, fill: &str, href: String| {
        let e = entry(name);
        Link { name: name.to_string(), title: titled(name, fill), icon: e.icon.to_string(), group: e.group, section: Section::Go, href, away: e.away }
    };
    let mut out = Vec::new();
    if seen.place.when().view {
        out.push(link("view", r.word(), format!("album:{SOURCE}/{id}")));
    }
    for a in &r.artists {
        out.push(link("go_artist", &a.name, format!("artist:{SOURCE}/{}", a.id)));
    }
    for g in &r.genres {
        out.push(link("go_genre", g, format!("crate:genre:{g}")));
    }
    let had = r.present().iter().any(|c| !matches!(&c.facts.library, Library::Incoming { uploader: Some(u), .. } if u != &seen.viewer.account));
    if had {
        out.push(link("sounds_like", "", format!("crate-new:like:album:{id}")));
    }
    out.push(link("share", "", format!("album:{SOURCE}/{id}")));
    let words = format!("{} {}", credit_text(&r.artists.iter().map(|a| (a.name.as_str(), None)).collect::<Vec<_>>()), r.title);
    let q: String = words.bytes().map(|b| if b.is_ascii_alphanumeric() { char::from(b).to_string() } else if b == b' ' { "+".into() } else { format!("%{b:02X}") }).collect();
    out.push(link("web_search", "", format!("https://duckduckgo.com/?q={q}")));
    out.push(link("genius_search", "", format!("https://genius.com/search?q={q}")));
    out
}

/// Its versions here (RS4, RS5): its present copies in the place's set, in the order they would play (the one that plays first,
/// marked, with no tap; the others Play from them), then its editions, each opening its record. None for a record of one version.
pub fn versions(r: &RecordFacts, seen: &Seen) -> Vec<Version> {
    let on = library_set(seen.place, seen.off);
    let id = record_id(r.id);
    let mut copies: Vec<&CopyOf> = r.present().into_iter().filter(|c| on.counts(c.facts.folder) && !hidden(c, seen.viewer)).collect();
    copies.sort_by_key(|c| (Some(c.facts.folder) != r.plays, c.facts.folder));
    let mut out: Vec<Version> = copies
        .iter()
        .map(|c| {
            let plays = Some(c.facts.folder) == r.plays;
            Version {
                kind: VersionKind::Copy,
                id: r.library(c.facts.folder).map_or_else(|| c.facts.folder.to_string(), |l| l.id.clone()),
                record: id.clone(),
                library: Some(r.library_name(c.facts.folder)),
                quality: c.quality.clone(),
                tracks: None,
                plays,
                action: (!plays).then(|| "play_from".to_string()),
                sources: Vec::new(),
            }
        })
        .collect();
    out.extend(r.editions.iter().map(|e| Version {
        kind: VersionKind::Edition,
        id: record_id(e.id),
        record: record_id(e.id),
        library: e.library.clone(),
        quality: e.quality.clone(),
        tracks: Some(e.tracks),
        plays: false,
        action: Some("view".to_string()),
        sources: Vec::new(),
    }));
    // A record not had: its services' lists are its versions, none chosen (VR4, FA5; OQ-VR-2 (a): each line opens its version in
    // place); a guess's list none (VR8). A record had: its copies and editions alone, the lists in References (OQ-DC1-4 (a), VR34).
    if r.present().is_empty() {
        let t = crate::facts::tracklist(None, &r.lists, r.pick.as_ref());
        let pick = |v: &crate::facts::ListVersion| r.pick.as_ref().is_some_and(|p| v.holds(p));
        out.extend(t.versions.iter().map(|v| Version {
            kind: VersionKind::Tracklist,
            id: v.refs.iter().map(|x| format!("{}:{}:{}", x.source, x.kind, x.value)).collect::<Vec<_>>().join(" "),
            record: id.clone(),
            library: None,
            quality: None,
            tracks: Some(u16::try_from(v.tracks.len()).unwrap_or(u16::MAX)),
            plays: pick(v),
            // Use this is an administrator's (VR5); a line opens its version's tracks in place for everyone.
            action: (seen.viewer.role == crate::can::Role::Administrator && !pick(v)).then(|| "use_this".to_string()),
            sources: v.by.iter().map(|s| crate::facts::source_word(s).to_string()).collect(),
        }));
    }
    if out.len() < 2 {
        out.clear();
    }
    out
}

/// An upload waiting, hidden from all but its uploader and the administrators (UP03).
fn hidden(c: &CopyOf, v: &Viewer) -> bool {
    matches!(&c.facts.library, Library::Incoming { uploader: Some(u), .. } if u != &v.account && !v.role.administers())
}

fn properties(r: &RecordFacts) -> RecordProperties {
    let fact = |name: &str| if r.hand.iter().any(|h| h == name) { vec!["hand".to_string()] } else { Vec::new() };
    let names: Vec<&str> = r.artists.iter().map(|a| a.name.as_str()).collect();
    let (year, kind) = (r.year_now(), r.kind_now());
    RecordProperties {
        type_why: crate::facts::type_why(&kind.by).to_string(),
        kind: FactView { value: kind.value, by: kind.by, versions: Vec::new() },
        secondary: Some(r.secondaries_now().into_iter().map(|s| crate::view::FactVersion { value: s.word, by: s.by }).collect::<Vec<_>>()).filter(|s| !s.is_empty()),
        title: FactView { value: r.title.clone(), by: fact("title"), versions: Vec::new() },
        artists: FactView { value: r.artists.clone(), by: fact("artist"), versions: Vec::new() },
        credit: credit_text(&names.iter().map(|n| (*n, None)).collect::<Vec<_>>()),
        year: FactView {
            value: year.value,
            by: year.by,
            versions: year.versions.into_iter().map(|v| crate::view::FactVersion { value: Some(v.value), by: v.by }).collect(),
        },
        labels: FactView::of(r.labels.clone()),
        cover: FactView { value: Some(cratarr_rules::ids::record_picture(&record_id(r.id), r.cover.as_deref())), by: fact("cover"), versions: Vec::new() },
        credits: Vec::new(),
    }
}

/// Its state line here (RS2): the holding in the place's set, said.
pub fn state(r: &RecordFacts, seen: &Seen) -> crate::view::Shown {
    let h = r.holding(seen);
    let owner = r.copies.iter().find_map(|c| match &c.facts.library {
        Library::Own(owner) if owner != &seen.viewer.account && c.facts.present > 0 => Some(owner.clone()),
        _ => None,
    });
    let name = |folder: i64| r.library_name(folder);
    shown(&h, &Says { picked: r.picked, owner: owner.as_deref(), library: &name })
}

/// The record's entity for this viewer here (RV1): one answer, had or not (RV2).
pub fn entity(r: &RecordFacts, seen: &Seen) -> Entity<RecordProperties> {
    let actions = actions(r, seen);
    let links = links(r, seen);
    let entities = versions(r, seen);
    let context = seen.place.when().context.is_some();
    let sections = sections(seen.place, |s| match s {
        Section::Head => true,
        Section::Context => context,
        Section::Versions => !entities.is_empty(),
        s => actions.iter().any(|a| a.section == s) || links.iter().any(|l| l.section == s),
    });
    Entity {
        class: "record".into(),
        id: record_id(r.id),
        rev: seen.rev,
        epoch: epoch(seen.viewer, seen.place, seen.off),
        properties: RecordProperties { credits: crate::credits::credits_for(&r.credits, seen.viewer), ..properties(r) },
        state: state(r, seen),
        actions,
        sections,
        entities,
        links,
    }
}

/// Its row in a list (RV5, M3): its properties' values and its state line.
pub fn row(r: &RecordFacts, seen: &Seen) -> Row {
    let names: Vec<&str> = r.artists.iter().map(|a| a.name.as_str()).collect();
    Row {
        id: record_id(r.id),
        rev: seen.rev,
        epoch: epoch(seen.viewer, seen.place, seen.off),
        title: r.title.clone(),
        credit: credit_text(&names.iter().map(|n| (*n, None)).collect::<Vec<_>>()),
        kind: r.kind_now().value,
        year: r.year_now().value,
        cover: Some(cratarr_rules::ids::record_picture(&record_id(r.id), r.cover.as_deref())),
        state: state(r, seen),
    }
}

/// Its card (RS6's one card per album): its row and how many versions it folds.
pub fn card(r: &RecordFacts, seen: &Seen) -> Card {
    let versions = u16::try_from(versions(r, seen).len().max(1)).unwrap_or(u16::MAX);
    Card { row: row(r, seen), versions }
}

/// Its chip: its id, its title and its type's word.
pub fn chip(r: &RecordFacts) -> Chip {
    Chip { id: record_id(r.id), title: r.title.clone(), word: r.word().to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::can::May;
    use cratarr_rules::holding::Fetcher;

    // SCENARIOS-DC1's libraries by their catalogue folders, and root's own music (CN6's "Save to Your music").
    const MUSIC: i64 = 0;
    const IN: i64 = 1;
    const OWN_A: i64 = 2;
    const OWN_B: i64 = 3;
    const MORE: i64 = 4;
    const IN2: i64 = 5;
    const ROOTS: i64 = 7;
    const ALL: May = May { request: true, offline: true, playlists: true };

    fn root() -> Viewer {
        Viewer::new("root", true, None, May::default(), Some(ROOTS))
    }

    fn alice() -> Viewer {
        Viewer::new("alice", false, None, ALL, Some(OWN_A))
    }

    /// The libraries as `viewer` names them: their own music "Your music", another's "<owner>'s music".
    fn libraries(viewer: &str) -> Vec<LibraryOf> {
        let own = |folder: i64, owner: &str| LibraryOf {
            folder,
            id: format!("/music/own/{owner}"),
            name: if owner == viewer { "Your music".into() } else { format!("{owner}'s music") },
            library: Library::Own(owner.into()),
        };
        let main = |folder: i64, name: &str| LibraryOf { folder, id: format!("/music/{name}"), name: name.into(), library: Library::Main };
        let incoming = |folder: i64, name: &str, fetcher: Fetcher| LibraryOf {
            folder,
            id: format!("/music/{name}"),
            name: name.into(),
            library: Library::Incoming { fetcher, uploader: None },
        };
        vec![main(MUSIC, "MUSIC"), incoming(IN, "IN", Fetcher::Lidarr), own(OWN_A, "alice"), own(OWN_B, "bob"), main(MORE, "MORE"), incoming(IN2, "IN2", Fetcher::Soulseek), own(ROOTS, "root")]
    }

    fn copy(folder: i64, library: Library, quality: &str, added: i64) -> CopyOf {
        CopyOf { facts: CopyFacts { folder, library, present: 10, paired: 10, went: None }, quality: Some(quality.into()), added, job: None, arrival: None }
    }

    /// Night Ferry `al-42` in MUSIC (MP3) and OWN-A (FLAC, better: RY01's offer into MUSIC), as `viewer` reads it from the Library.
    fn night_ferry(viewer: &str) -> RecordFacts {
        RecordFacts {
            id: 42,
            title: "Night Ferry".into(),
            kind: Some("Album".into()),
            artists: vec![Named { id: "ar-3".into(), name: "Glass Harbour".into(), catno: None }],
            copies: vec![copy(MUSIC, Library::Main, "MP3 320", 100), copy(OWN_A, Library::Own("alice".into()), "FLAC", 200)],
            shortest: Some(10),
            plays: Some(MUSIC),
            replacing: vec![(OWN_A, MUSIC)],
            libraries: libraries(viewer),
            ..RecordFacts::default()
        }
    }

    fn titles(actions: &[ActionView], section: Section) -> Vec<String> {
        actions.iter().filter(|a| a.section == section).map(|a| a.title.clone()).collect()
    }

    /// CN6: the Library section's actions name their copy and follow the server (`decide`): root moves MUSIC's copy to MORE, saves it
    /// into his own and bob's music, puts OWN-A's better copy in MUSIC's place, deletes MUSIC's copy after a yes, and may save
    /// alice's copy into MUSIC (A.4's 7th: the server's verdict holds); never into Incoming, never OWN-A's copy deleted. Alice deletes
    /// her own copy after a yes, and moves and saves nothing. Each entry names its copy, as the record has two. Red first: the page
    /// decides them (copiesRule's offers from AlbumCopy's rights, AlbumMenu's `me.admin`).
    #[test]
    fn each_copy_is_named_and_decide_holds_the_library_section() {
        let (root, alice) = (root(), alice());
        let place = Place::Library;
        let seen = |v| Seen { viewer: v, place: &place, off: &[], rev: 100 };
        let roots = actions(&night_ferry("root"), &seen(&root));
        let library = titles(&roots, Section::Library);
        let from_music = ["Move to MORE from MUSIC", "Save to Your music from MUSIC", "Save to bob's music from MUSIC", "Delete from MUSIC"];
        // Alice's copy too (A.4's 7th: an administrator saves from another's own music, whoever may delete it).
        let from_hers = ["Replace in MUSIC from alice's music", "Save to MORE from alice's music", "Save to Your music from alice's music"];
        for want in from_music.iter().chain(&from_hers) {
            assert!(library.iter().any(|t| t == want), "root: {want} in {library:?}");
        }
        for never in ["IN from", "IN2 from", "Delete from alice's music", "Save to MUSIC from", "Save to alice's music from"] {
            assert!(!library.iter().any(|t| t.contains(never)), "root: {never} in {library:?}");
        }
        let delete = roots.iter().find(|a| a.title == "Delete from MUSIC").unwrap();
        assert_eq!(delete.confirm.as_ref().map(|c| c[0].as_str()), Some("Delete from MUSIC?"), "a yes first");
        assert_eq!(delete.fields.as_ref().and_then(|f| f.from.as_deref()), Some("/music/MUSIC"), "its copy named by its library's id");
        let moved = roots.iter().find(|a| a.title == "Move to MORE from MUSIC").unwrap();
        assert_eq!((moved.href.as_str(), moved.fields.as_ref().unwrap().to.as_deref()), ("/api/albums/cratarr/al-42/move", Some("/music/MORE")));
        let alices = actions(&night_ferry("alice"), &seen(&alice));
        assert_eq!(titles(&alices, Section::Library), ["Play from Your music", "Delete from Your music"], "hers: Play from and her own Delete");
        assert!(alices.iter().all(|a| !a.title.starts_with("Move") && !a.title.starts_with("Save")));
    }

    /// RS25 (decision 0034, 2026-11-21): Delete from the main library asks twice, in the server's words: "Delete from
    /// MUSIC?" and "MUSIC is your main library. Delete it anyway?"; another library's Delete (IN's arrival, OWN-A's) asks once, and
    /// an action that asks nothing carries no step. Red first: `confirm` one string, the second step dropped (PAGE3-REPORT).
    #[test]
    fn deleting_from_the_main_library_asks_twice() {
        let (root, alice) = (root(), alice());
        let place = Place::Library;
        let roots = actions(&night_ferry("root"), &Seen { viewer: &root, place: &place, off: &[], rev: 1 });
        let music = roots.iter().find(|a| a.title == "Delete from MUSIC").unwrap();
        assert_eq!(music.confirm.clone().unwrap(), ["Delete from MUSIC?", "MUSIC is your main library. Delete it anyway?"], "two steps, in order");
        let hers = actions(&night_ferry("alice"), &Seen { viewer: &alice, place: &place, off: &[], rev: 1 });
        let own = hers.iter().find(|a| a.title == "Delete from Your music").unwrap();
        assert_eq!(own.confirm.as_ref().map(Vec::len), Some(1), "not the main library: one step");
        let mut more = night_ferry("root");
        more.copies[1] = copy(MORE, Library::Main, "FLAC", 200);
        more.replacing.clear();
        let mores = actions(&more, &Seen { viewer: &root, place: &place, off: &[], rev: 1 });
        let other_main = mores.iter().find(|a| a.title == "Delete from MORE").unwrap();
        assert_eq!(other_main.confirm.as_ref().map(Vec::len), Some(1), "MORE is a main library, not the main one: one step");
        assert!(roots.iter().filter(|a| a.name == "play").all(|a| a.confirm.is_none()), "nothing asked: no step");
    }

    /// RS8 and RS17 (the server's half): every action and link the record sends says whether it works with Cratarr away, by the
    /// entries' table (the page's `WORKS_AWAY` for a record): the large actions that play what the device holds, View and Go to,
    /// Favourite, Rating, Add to playlist, Remove from this device and the web searches do; Start radio, Sync, Download…, Want, the
    /// Library section's and the Remove group's need Cratarr. Red first: no `away` mark (the page's own set, menuTable.ts).
    #[test]
    fn every_record_action_says_whether_it_works_away() {
        let root = root();
        let place = Place::Library;
        let seen = Seen { viewer: &root, place: &place, off: &[], rev: 1 };
        let mut r = night_ferry("root");
        r.genres = vec!["Ambient".into()];
        let e = entity(&r, &seen);
        let away: Vec<(&str, bool)> = e.actions.iter().map(|a| (a.name.as_str(), a.away)).chain(e.links.iter().map(|l| (l.name.as_str(), l.away))).collect();
        for works in ["play", "shuffle", "play_next", "add_to_queue", "go_artist", "go_genre", "favourite", "rating", "add_to_playlist", "remove_from_device", "web_search", "genius_search"] {
            assert!(away.contains(&(works, true)), "{works} works away: {away:?}");
        }
        for needs in ["start_radio", "sync", "download", "move_to", "save_to", "replace_in", "delete_from", "play_from", "edit", "change_cover", "sounds_like", "share"] {
            assert!(away.iter().all(|(n, a)| *n != needs || !*a), "{needs} needs Cratarr: {away:?}");
        }
        for needs in ["start_radio", "sync", "download", "move_to", "delete_from", "share"] {
            assert!(away.iter().any(|(n, _)| *n == needs), "{needs} sent, marked: {away:?}");
        }
        // View, from a place that offers it; and the table says it for every entry, the page reading no set of its own.
        let search = Place::Search;
        let from_search = entity(&r, &Seen { viewer: &root, place: &search, off: &[], rev: 1 });
        assert!(from_search.links.iter().any(|l| l.name == "view" && l.away), "View works away");
        assert!(crate::words::ENTRIES.iter().all(|x| x.away == crate::words::WORKS_AWAY.contains(&x.name)));
        assert!(!entry("start_radio").away && !entry("want").away, "never Start radio; Want needs Cratarr");
    }

    fn listed(source: &str, kind: &str, value: &str, n: u32) -> crate::facts::List {
        let tracks = (1..=n).map(|i| crate::facts::RefTrack { disc: 1, position: i, title: format!("A{i}"), length_ms: Some(200_000 + i64::from(i) * 7_000), recording: None }).collect();
        crate::facts::List { reference: crate::facts::Reference::new(source, kind, value), tracks, at: 1, guessed: false }
    }

    /// Answered `al-7`, not had: MusicBrainz's 9 and Bandcamp's 9 agreeing, Discogs' 11 (VR4).
    fn answered() -> RecordFacts {
        RecordFacts {
            id: 7,
            title: "Answered".into(),
            artists: vec![Named { id: "ar-9".into(), name: "Kite Theory".into(), catno: None }],
            lists: vec![listed("musicbrainz", "release", "R7", 9), listed("bandcamp", "album", "P7", 9), listed("discogs", "release", "D7", 11)],
            shortest: Some(9),
            libraries: libraries("alice"),
            ..RecordFacts::default()
        }
    }

    /// VR4 and FA5 (the entity's half): a record not had's versions are its lists, none chosen: two Tracklist lines, "9 tracks ·
    /// MusicBrainz, Bandcamp" and "11 tracks · Discogs", neither marked; root's lines carry Use this, alice's none; the card says 2
    /// versions; a guess's list is no line (VR8). Red first: a record not had had no versions (part 1: copies and editions alone).
    #[test]
    fn a_record_not_hads_versions_are_its_lists() {
        let (root, alice) = (root(), alice());
        let place = Place::Search;
        let mut r = answered();
        r.lists.push(crate::facts::List { guessed: true, ..listed("bandcamp", "album", "P7s", 7) });
        let roots = versions(&r, &Seen { viewer: &root, place: &place, off: &[], rev: 1 });
        let lines: Vec<(VersionKind, Option<u16>, Vec<&str>, bool, Option<&str>)> =
            roots.iter().map(|v| (v.kind, v.tracks, v.sources.iter().map(String::as_str).collect(), v.plays, v.action.as_deref())).collect();
        assert_eq!(
            lines,
            [
                (VersionKind::Tracklist, Some(9), vec!["MusicBrainz", "Bandcamp"], false, Some("use_this")),
                (VersionKind::Tracklist, Some(11), vec!["Discogs"], false, Some("use_this")),
            ]
        );
        let hers = versions(&r, &Seen { viewer: &alice, place: &place, off: &[], rev: 1 });
        assert!(hers.iter().all(|v| v.action.is_none()), "a user picks nothing");
        assert_eq!(card(&r, &Seen { viewer: &alice, place: &place, off: &[], rev: 1 }).versions, 2, "the card says 2 versions");
        // Picked (VR5, FA6): the pick's line marked, first; no Use this on it.
        r.pick = Some(crate::facts::Reference::new("discogs", "release", "D7"));
        let picked = versions(&r, &Seen { viewer: &root, place: &place, off: &[], rev: 1 });
        assert_eq!((picked[0].tracks, picked[0].plays, picked[0].action.as_deref()), (Some(11), true, None));
    }

    /// VR34, VR35 (one record; OQ-DC1-4 (a)): a had record's Versions are its copies and editions alone: a service's differing
    /// list is no line (References shows it); a record not had that gets its first copy loses its Tracklist lines. Red first: none
    /// (no Tracklist lines at all before VR01).
    #[test]
    fn a_had_records_versions_are_its_copies_and_editions() {
        let root = root();
        let place = Place::Library;
        let seen = Seen { viewer: &root, place: &place, off: &[], rev: 1 };
        let mut had = night_ferry("root");
        had.lists = vec![listed("musicbrainz", "release", "R42", 10), listed("discogs", "release", "D42", 12)];
        assert!(versions(&had, &seen).iter().all(|v| v.kind == VersionKind::Copy), "copies alone: {:?}", versions(&had, &seen));
        let mut now_had = answered();
        now_had.copies = vec![copy(IN2, Library::Incoming { fetcher: Fetcher::Soulseek, uploader: None }, "FLAC", 100)];
        assert!(versions(&now_had, &seen).iter().all(|v| v.kind != VersionKind::Tracklist), "VR35: its lists are no lines once it is had");
    }

    /// VR16 (b) and VR47 (the record's view): a had record's own page shows no count until an administrator picks a list, then
    /// the count; VR12 rule 6: only guesses kept, not known whole, no count. Red first: part 1's entity counted by `shortest`
    /// whatever judged it (no pick read; no rule 6).
    #[test]
    fn a_had_records_own_page_counts_as_the_owner_picks() {
        let root = root();
        let place = Place::Library;
        let seen = Seen { viewer: &root, place: &place, off: &[], rev: 1 };
        let mut seven = night_ferry("root");
        seven.copies.truncate(1);
        seven.copies[0].facts.present = 7;
        seven.copies[0].facts.paired = 7;
        let e = entity(&seven, &seen);
        assert_eq!((e.state.is, e.state.words.as_str()), (crate::view::Is::Partly, "In your library"), "no count before a pick");
        seven.picked = true;
        assert_eq!(entity(&seven, &seen).state.words, "Partly there · 7 of 10", "VR47: the pick shows the count");
        seven.picked = false;
        seven.only_guesses = true;
        seven.shortest = None;
        let unknown = entity(&seven, &seen);
        assert_eq!((unknown.state.is, unknown.state.words.as_str()), (crate::view::Is::Unknown, "Not known whole"), "rule 6: no count");
    }

    /// RV2's shape: a record not had has the same entity, its state saying what it is; the preview's Play row and Want, nothing that
    /// needs a copy.
    #[test]
    fn a_record_not_had_has_the_entitys_shape() {
        let alice = alice();
        let place = Place::Search;
        let answered = RecordFacts {
            id: 7,
            title: "Answered".into(),
            kind: Some("EP".into()),
            artists: vec![Named { id: "ar-9".into(), name: "Kite Theory".into(), catno: None }],
            preview: Some("https://kitetheory.example.org/album/answered".into()),
            shortest: Some(9),
            libraries: libraries("alice"),
            ..RecordFacts::default()
        };
        let seen = Seen { viewer: &alice, place: &place, off: &[], rev: 5 };
        let e = entity(&answered, &seen);
        assert_eq!((e.class.as_str(), e.id.as_str(), e.state.words.as_str()), ("record", "al-7", "Not in your library"));
        let names: Vec<&str> = e.actions.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["play", "want"], "the preview's Play row, then Want");
        assert!(e.entities.is_empty(), "no copy, no versions");
        assert_eq!(e.sections, [Section::Head, Section::Play, Section::Go]);
        assert_eq!(chip(&answered), Chip { id: "al-7".into(), title: "Answered".into(), word: "EP".into() });
    }
}
