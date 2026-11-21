//! The record's words, in one place (DOMAIN-CORE §4.5; SCENARIOS-DC1 RS2, RS23; decision 0104 point 2): what each state says
//! ([`shown`], [`STATES`], shared/states.json) and each entry's title, icon and group ([`entry`], [`ENTRIES`],
//! shared/entries.json). An exhaustive `match` on the state, so a state without its words does not compile. The page draws what
//! is sent; it writes no state's words of its own (RS2's Never).
//!
//! A title may hold one blank: `{type}` the record's type's word ("View EP"), `{name}` what it goes to ("Go to Bonobo"),
//! `{library}` a library's name ("Move to MORE").

use cratarr_rules::holding::Holding;
use serde::Serialize;

use crate::view::{Group, Is, Shown};

/// What a record never made, or forgotten, answers (RV4): the existing notice says it.
pub const GONE: &str = "That record is gone";

/// What a state says, as the table writes it: its name, and its words with their blanks (`{has}`, `{of}`, `{library}`,
/// `{owner}`). A time where it has one comes beside the words (`Shown::at`), said by the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StateWords {
    pub is: Is,
    pub words: &'static str,
}

/// Every state's words (RS2): "In your library", "Not in your library", "Partly there · 7 of 10", "Arrived", "In bob's music",
/// "Was in MORE until …". Never "In the library", "Here", "Partly here", "Missing albums" or "Records arrived" (A.6).
pub const STATES: [StateWords; 6] = [
    StateWords { is: Is::NotHad, words: "Not in your library" },
    StateWords { is: Is::Partly, words: "Partly there · {has} of {of}" },
    StateWords { is: Is::Arrived, words: "Arrived" },
    StateWords { is: Is::Had, words: "In your library" },
    StateWords { is: Is::Elsewhere, words: "In {owner}'s music" },
    StateWords { is: Is::Retired, words: "Was in {library} until" },
];

fn words_of(is: Is) -> &'static str {
    STATES.iter().find(|s| s.is == is).map(|s| s.words).expect("every state has its words")
}

/// What `shown` reads beside the holding: whether an administrator picked the list the record counts by (OQ-VR-3 (b): a had
/// record's own sheet and page give no count until one is), the owner of the copy someone else holds whole, and a library's
/// name by its catalogue folder.
pub struct Says<'a> {
    pub picked: bool,
    pub owner: Option<&'a str>,
    pub library: &'a dyn Fn(i64) -> String,
}

/// What a record's holding says on its sheet and page (RS2). Partly with no list picked says "In your library", with no count
/// (OQ-VR-3 (b), the owner 2026-11-20; VR16): "Partly there · 7 of 10" stays a request's, and the record's once a list is picked.
pub fn shown(holding: &Holding, says: &Says) -> Shown {
    let said = |is: Is, words: String, at: Option<i64>| Shown { is, words, at };
    match *holding {
        Holding::NotHad => said(Is::NotHad, words_of(Is::NotHad).to_string(), None),
        Holding::Partly { has, of } if says.picked => {
            said(Is::Partly, words_of(Is::Partly).replace("{has}", &has.to_string()).replace("{of}", &of.to_string()), None)
        }
        Holding::Partly { .. } => said(Is::Partly, words_of(Is::Had).to_string(), None),
        Holding::Arrived => said(Is::Arrived, words_of(Is::Arrived).to_string(), None),
        Holding::Had => said(Is::Had, words_of(Is::Had).to_string(), None),
        Holding::Elsewhere => {
            said(Is::Elsewhere, words_of(Is::Elsewhere).replace("{owner}", says.owner.unwrap_or("someone else")), None)
        }
        Holding::Retired { since, from } => said(Is::Retired, words_of(Is::Retired).replace("{library}", &(says.library)(from)), Some(since)),
    }
}

/// An entry of the record's sheet as the table writes it (RS23): its name (what an action or a link is called in the answer),
/// its title with its blank, its icon, its group (the menu's card, decision 0058).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub name: &'static str,
    pub title: &'static str,
    pub icon: &'static str,
    pub group: Group,
}

const fn e(name: &'static str, title: &'static str, icon: &'static str, group: Group) -> Entry {
    Entry { name, title, icon, group }
}

/// The record's entries, by group in the menu's order (the page's `MENUS.album` and `MENUS.bandcamp` rows, their words and icons
/// as AlbumMenu draws them: the same groups, order and words).
pub const ENTRIES: [Entry; 33] = [
    e("play", "Play", "play", Group::Play),
    e("resume", "Resume", "resume", Group::Play),
    e("shuffle", "Shuffle", "shuffle", Group::Play),
    e("play_next", "Play next", "playNext", Group::Play),
    e("add_to_queue", "Add to queue", "listAdd", Group::Play),
    e("start_radio", "Start radio", "radio", Group::Play),
    e("want", "Want", "circlePlus", Group::Want),
    e("requested", "Requested", "inbox", Group::Want),
    e("view", "View {type}", "squareArrowOutUpRight", Group::Go),
    e("go_artist", "Go to {name}", "user", Group::Go),
    e("go_genre", "Go to {name}", "swatch", Group::Go),
    e("sounds_like", "Sounds like this", "music", Group::Go),
    e("favourite", "Favourite", "heart", Group::Feel),
    e("rating", "Rating", "star", Group::Feel),
    e("add_to_playlist", "Add to playlist", "playlists", Group::Collect),
    e("download", "Download…", "arrowDownToLine", Group::Take),
    e("sync", "Sync to this device", "download", Group::Take),
    e("remove_from_device", "Remove from this device", "trash", Group::Take),
    e("share", "Share", "share", Group::Take),
    e("web_search", "Web search", "search", Group::Know),
    e("genius_search", "Genius search", "search", Group::Know),
    e("edit", "Edit", "pencil", Group::Change),
    e("change_cover", "Change cover", "image", Group::Change),
    e("missing_tracks", "Missing tracks…", "search", Group::Change),
    e("play_from", "Play from {library}", "play", Group::Library),
    e("cancel_conversion", "Cancel conversion", "close", Group::Library),
    e("save_to", "Save to {library}", "save", Group::Library),
    e("move_to", "Move to {library}", "intoBox", Group::Library),
    e("add_to", "Add to {library}", "plus", Group::Library),
    e("replace_in", "Replace in {library}", "swap", Group::Library),
    e("delete", "Delete {type}", "trash", Group::Remove),
    e("delete_from", "Delete from {library}", "trash", Group::Remove),
    e("delete_arrival", "Delete", "trash", Group::Remove),
];

/// An entry by its name (RS23): one table for the record's entries' words, icons and groups.
pub fn entry(name: &str) -> &'static Entry {
    ENTRIES.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("an entry the table does not know: {name}"))
}

/// An entry's title with its blank filled.
pub fn titled(name: &str, fill: &str) -> String {
    let title = entry(name).title;
    ["{type}", "{name}", "{library}"].iter().fold(title.to_string(), |t, blank| t.replace(blank, fill))
}

/// Where a record has several copies, the library an entry goes to and the copy it acts on, both named (copiesRule's `named`;
/// the owner, testing 0.16.0: "Move to lidarr-music" moved Music's copy where the FLAC was meant): "MORE from MUSIC".
pub fn from_copy(to: &str, copy: &str) -> String {
    format!("{to} from {copy}")
}

/// A copy's quality as its line says it (the page's `qualityOfCopy`; SB23, never "kb/s"): its format, and a lossy one's bitrate
/// ("MP3 320", "FLAC"); "Mixed formats" where its files differ. An MP4 at 500 kb/s or more is ALAC, lossless (`copies::lossless`).
pub fn quality(format: Option<&str>, kbps: Option<i64>) -> String {
    let Some(format) = format else { return "Mixed formats".to_string() };
    let lossless = matches!(format, "FLAC" | "WAV" | "AIFF" | "APE" | "WAVPACK") || (format == "MP4" && kbps.is_some_and(|k| k >= 500));
    match kbps.filter(|k| !lossless && *k > 0) {
        Some(k) => format!("{format} {k}"),
        None => format.to_string(),
    }
}

/// A record as its release type names it (the owner, 2026-11-01; OQ-F35): album, EP, single, broadcast; a compilation whatever
/// its primary type; a record where it has none or is Other. As the page's `recordWord` says it.
pub fn record_word(kind: Option<&str>, secondary: &[String]) -> &'static str {
    if secondary.iter().any(|s| s.eq_ignore_ascii_case("compilation")) {
        return "compilation";
    }
    match kind.map(str::to_ascii_lowercase).as_deref() {
        Some("album") => "album",
        Some("ep") => "EP",
        Some("single") => "single",
        Some("broadcast") => "broadcast",
        _ => "record",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: i64 = 1_740_960_000;

    fn says(picked: bool) -> Says<'static> {
        fn names(folder: i64) -> String {
            match folder {
                0 => "MUSIC".to_string(),
                4 => "MORE".to_string(),
                other => format!("library {other}"),
            }
        }
        Says { picked, owner: Some("bob"), library: &names }
    }

    /// RS2: every holding has its words (an exhaustive match: a state without them does not compile), Signal's 7 of 10 says no count
    /// until a list is picked (OQ-VR-3 (b)), a retired record names the library its last copy left and when (HD9's words). Red first:
    /// no table of the states' words; the page writes them (recordNotHad.ts, requestState.ts, "Was in Music until" whatever the
    /// library).
    #[test]
    fn every_holding_has_its_words() {
        let line = |h: Holding, picked: bool| shown(&h, &says(picked));
        assert_eq!(line(Holding::Had, false), Shown { is: Is::Had, words: "In your library".into(), at: None });
        assert_eq!(line(Holding::NotHad, false).words, "Not in your library");
        assert_eq!(line(Holding::Partly { has: 7, of: 10 }, true).words, "Partly there · 7 of 10");
        let unpicked = line(Holding::Partly { has: 7, of: 10 }, false);
        assert_eq!((unpicked.is, unpicked.words.as_str()), (Is::Partly, "In your library"), "no count until an administrator picks");
        assert_eq!(line(Holding::Arrived, false).words, "Arrived");
        assert_eq!(line(Holding::Elsewhere, false).words, "In bob's music");
        assert_eq!(line(Holding::Retired { since: T, from: 4 }, false), Shown { is: Is::Retired, words: "Was in MORE until".into(), at: Some(T) });
        // Never the words A.6 retired, nor a state written twice.
        for s in STATES {
            for never in ["In the library", "Here", "Partly here", "Missing albums", "Records arrived", "Was in Music"] {
                assert!(!s.words.starts_with(never) || s.words == "In your library", "{never} in {s:?}");
            }
            assert_eq!(STATES.iter().filter(|o| o.is == s.is).count(), 1, "{:?} said twice", s.is);
        }
    }

    /// RS23 (its words half): one table for the record's entries, each name once, the groups in the menu's order; the words the page
    /// has today ("View EP", "Go to Bonobo", "Save to …", "Add to …", "Move to …", "Replace in …", "Delete EP"). Red first: the
    /// record's entries are the page's (`MENUS.album`, `MENUS.bandcamp`, AlbumMenu's icons, copiesRule's words).
    #[test]
    fn the_record_menu_is_the_servers_table() {
        for x in ENTRIES {
            assert_eq!(ENTRIES.iter().filter(|o| o.name == x.name).count(), 1, "{} twice", x.name);
        }
        let groups: Vec<Group> = ENTRIES.iter().map(|x| x.group).collect();
        let mut sorted = groups.clone();
        sorted.sort();
        assert_eq!(groups, sorted, "the groups in the menu's order");
        assert_eq!(titled("view", record_word(Some("EP"), &[])), "View EP");
        assert_eq!(titled("delete", record_word(Some("Album"), &["Compilation".into()])), "Delete compilation");
        assert_eq!(titled("go_artist", "Bonobo"), "Go to Bonobo");
        for (name, words) in [("save_to", "Save to MORE"), ("add_to", "Add to MORE"), ("move_to", "Move to MORE"), ("replace_in", "Replace in MORE")] {
            assert_eq!(titled(name, "MORE"), words);
        }
        assert_eq!(record_word(None, &[]), "record");
        assert_eq!(entry("want").icon, "circlePlus");
        assert_eq!(entry("requested").title, "Requested");
    }
}
