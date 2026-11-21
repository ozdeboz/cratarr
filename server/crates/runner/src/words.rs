//! The one wording table for jobs (SCENARIOS-RN JR6; DOMAIN-CORE C.8): every kind of wait, hold, retry and progress Cratarr has,
//! the runner state it is, and its words. `shared/states.json` holds it for the page (Health › Activity), so a job's words are
//! written once: here, read by the server's rows and by the page alike. A word's `{name}` is filled from the job; a time is the
//! page's `when()` there, and the server's test clock here.

use serde::Serialize;

use crate::state::State;

/// Words by their key: what a job says, its parts filled from the job.
pub const WORDS: [(&str, &str); 19] = [
    ("waits_for_room", "Waits for room · {used} of {room} GB"),
    ("waits_for_space", "Waits for space · {free} GB free"),
    ("waits_for_release", "Waits for its release · {day}"),
    ("not_found_yet", "Not found yet · again in {in}"),
    ("stuck", "Stuck: {why} · tried again at {at}"),
    ("paused_until", "Paused until {until}"),
    ("percent", "{percent}%"),
    ("waits_to_be_checked", "Waits to be checked"),
    ("checked", "{done} of {of} checked"),
    ("files", "{done} of {of} files"),
    ("count", "{done} of {of}"),
    ("waits_for_turn", "Waits for its turn to {what}"),
    ("waits_for_memory", "Waits for memory"),
    ("resting", "Resting"),
    ("background_off", "Background reading is paused"),
    ("asked_for", "asked for"),
    ("catching_up", "catching up"),
    ("again_at", "Tried again at {at}"),
    ("owed", "Lidarr is told again at {at}"),
];

/// A row of C.8: what today's code calls it, the runner state it is, what it waits for (none: nothing), its words' key, and its
/// words as one example says them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Kind {
    pub today: &'static str,
    pub state: State,
    pub waits: Option<&'static str>,
    pub words: &'static str,
    pub example: &'static str,
}

const fn kind(today: &'static str, state: State, waits: Option<&'static str>, words: &'static str, example: &'static str) -> Kind {
    Kind { today, state, waits, words, example }
}

/// C.8's rows, every kind: a request's waits; Soulseek's Waiting and Stuck; Soulseek held; Fetching, Fetched, Checking; the
/// outbox's owed stops; the background reading; conversions, the scrobble queue, imports, track fetches, the intake card's hearing.
pub const C8: [Kind; 18] = [
    kind("request: waits for room", State::Waiting, Some("room"), "waits_for_room", "Waits for room · 130 of 100 GB"),
    kind("request: waits for space", State::Waiting, Some("disk"), "waits_for_space", "Waits for space · 3 GB free"),
    kind("request: waits for its release", State::Scheduled, None, "waits_for_release", "Waits for its release · 4 Dec"),
    kind("soulseek: waiting", State::Scheduled, None, "not_found_yet", "Not found yet · again in 4 hours"),
    kind("soulseek: stuck", State::Retrying, None, "stuck", "Stuck: No folder holds every track · tried again at 15:00"),
    kind("soulseek: held", State::Waiting, Some("source"), "paused_until", "Paused until 14:05"),
    kind("soulseek: fetching", State::Running, None, "percent", "45%"),
    kind("soulseek: fetched", State::Waiting, Some("check"), "waits_to_be_checked", "Waits to be checked"),
    kind("soulseek: checking", State::Running, None, "checked", "4 of 9 checked"),
    kind("lidarr: owed stops", State::Retrying, None, "owed", "Lidarr is told again at 15:00"),
    kind("discogs: typing", State::Running, None, "count", "30 of 300"),
    kind("musicbrainz: re-reads", State::Running, None, "count", "12 of 43"),
    kind("conversion", State::Running, None, "files", "3 of 10 files"),
    kind("scrobble queue", State::Retrying, None, "again_at", "Tried again at 15:00"),
    kind("import", State::Running, None, "count", "120 of 400"),
    kind("track fetch", State::Running, None, "percent", "45%"),
    kind("intake card: hearing", State::Running, None, "files", "4 of 9 files"),
    kind("decode: waits for its turn", State::Waiting, Some("decode"), "waits_for_turn", "Waits for its turn to decode"),
];

/// A word by its key.
pub fn word(key: &str) -> Option<&'static str> {
    WORDS.iter().find(|(k, _)| *k == key).map(|(_, w)| *w)
}

/// The words of `key`, each `{name}` filled from `parts` (a part not given stays as it is). None for a key the table lacks.
pub fn say(key: &str, parts: &[(&str, &str)]) -> Option<String> {
    let mut out = word(key)?.to_string();
    for (name, value) in parts {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    Some(out)
}

/// A count as the page writes it: thousands apart ("1,204").
pub fn number(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The table as `shared/states.json` holds it.
pub fn table() -> serde_json::Value {
    serde_json::json!({
        "words": WORDS.iter().map(|(k, w)| (k.to_string(), serde_json::Value::from(*w))).collect::<serde_json::Map<_, _>>(),
        "c8": C8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The parts each example fills.
    fn parts_of(k: &Kind) -> Vec<(&'static str, &'static str)> {
        match k.words {
            "waits_for_room" => vec![("used", "130"), ("room", "100")],
            "waits_for_space" => vec![("free", "3")],
            "waits_for_release" => vec![("day", "4 Dec")],
            "not_found_yet" => vec![("in", "4 hours")],
            "stuck" => vec![("why", "No folder holds every track"), ("at", "15:00")],
            "paused_until" => vec![("until", "14:05")],
            "percent" => vec![("percent", "45")],
            "checked" if k.today == "soulseek: checking" => vec![("done", "4"), ("of", "9")],
            "files" if k.today == "intake card: hearing" => vec![("done", "4"), ("of", "9")],
            "files" => vec![("done", "3"), ("of", "10")],
            "count" if k.today == "import" => vec![("done", "120"), ("of", "400")],
            "count" if k.today == "discogs: typing" => vec![("done", "30"), ("of", "300")],
            "count" => vec![("done", "12"), ("of", "43")],
            "waits_for_turn" => vec![("what", "decode")],
            "again_at" | "owed" => vec![("at", "15:00")],
            _ => Vec::new(),
        }
    }

    /// JR6: each of C.8's rows has its runner state and its words through the one table, and `shared/states.json` holds the table
    /// as the server has it. Never a kind with no state, a state with no words, nor words written apart from the table (each
    /// example is the table's words filled).
    #[test]
    fn every_c8_row_has_its_job_state_and_words() {
        for k in C8 {
            let said = say(k.words, &parts_of(&k)).unwrap_or_else(|| panic!("{}: no words for {}", k.today, k.words));
            assert_eq!(said, k.example, "{}: its words from the table", k.today);
            assert!(!said.contains('{'), "{}: every part filled", k.today);
        }
        let states: std::collections::HashSet<State> = C8.iter().map(|k| k.state).collect();
        for s in [State::Waiting, State::Scheduled, State::Running, State::Retrying] {
            assert!(states.contains(&s), "{s:?} has words");
        }
        for want in ["Waits for room · 130 of 100 GB", "Waits for its release · 4 Dec", "Not found yet · again in 4 hours", "Paused until 14:05", "45%", "Waits to be checked", "4 of 9 checked"] {
            assert!(C8.iter().any(|k| k.example == want), "{want}");
        }
        let shared: serde_json::Value = serde_json::from_str(include_str!("../../../../shared/states.json")).unwrap();
        assert_eq!(shared, table(), "shared/states.json is the table");
        assert_eq!((number(1_204), number(3_580), number(31_639), number(999)), ("1,204".into(), "3,580".into(), "31,639".into(), "999".into()));
    }
}
