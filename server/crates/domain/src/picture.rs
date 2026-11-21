//! A record's one picture resolver (OBJECT-MAP-2 §2.1, the record's Picture; SCENARIOS-COUPLING COU.11, COU.12, COU.20; SCENARIOS-DC1
//! FA14; DOMAIN-CORE D.1's pictures row): which picture every record-level surface shows, and what it is by. Pure: the store hands
//! it what it keeps and whether the deciding copy's files hold a picture.
//!
//! - **The hand first** (FA14, T-COU-8): a chosen cover (Change cover's, Replace's "keep the old one", a 0.17 pick carried at the step)
//!   wins over the files and every service, and no re-read, Replace, fetch or step replaces it.
//! - **Then the files**: the deciding copy's cover file, else the picture inside its files that speak, in track order (T-COU-22).
//! - **Then the services as candidates**: where the files have none, the first to answer stays shown, kept, until a hand picks
//!   (COU.12 (a)); it shows with the service switched off (T-COU-7).

/// What a record's picture is: the store's kept cover, its files', or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shows {
    Kept,
    Files,
    None,
}

/// The picture a record shows and what it is by (`hand`, `files@<library>`, a service).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub shows: Shows,
    pub by: Vec<String>,
}

/// How a kept cover was chosen that sits below the files: a service's first answer.
pub const FIRST: &str = "first";

/// The resolver: `kept` the store's cover (how it was chosen, its source), `files` the deciding copy's `files@<library>` where its
/// files hold a picture.
pub fn resolve(kept: Option<(&str, &str)>, files: Option<&str>) -> Picture {
    let _ = kept;
    match files {
        Some(files) => Picture { shows: Shows::Files, by: vec![files.to_string()] },
        None => Picture { shows: Shows::None, by: Vec::new() },
    }
}

/// T-COU-20 (FA16's "the later `at` stays"): of two records' kept covers meeting in a merge, whether `ours` stays over `theirs`: a
/// choice over a service's first answer, else the later; on a tie, ours.
pub fn later(ours_how: &str, ours_at: i64, theirs_how: &str, theirs_at: i64) -> bool {
    let _ = (ours_how, ours_at, theirs_how, theirs_at);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FA14 (COU.11, COU.12's rule): the hand's pick wins; else the files; else a service's first answer, kept. Red first: no resolver
    /// (every surface read the files).
    #[test]
    fn a_cover_is_a_fact_and_the_hand_wins() {
        let by = |p: Picture| (p.shows, p.by);
        assert_eq!(by(resolve(None, Some("files@MUSIC"))), (Shows::Files, vec!["files@MUSIC".to_string()]), "by its files");
        assert_eq!(by(resolve(Some(("hand", "coverartarchive")), Some("files@MUSIC"))), (Shows::Kept, vec!["hand".to_string()]), "the hand over the files");
        for how in ["kept", "017"] {
            assert_eq!(resolve(Some((how, "files@MUSIC")), Some("files@IN")).shows, Shows::Kept, "{how}: a choice");
        }
        assert_eq!(by(resolve(Some((FIRST, "coverartarchive")), None)), (Shows::Kept, vec!["coverartarchive".to_string()]), "a service's first, kept");
        assert_eq!(resolve(Some((FIRST, "coverartarchive")), Some("files@MUSIC")).shows, Shows::Files, "the files over a service");
        assert_eq!(resolve(None, None).shows, Shows::None);
        // T-COU-20: the later pick stays; a choice over a service's first.
        assert!(later("hand", 20, "hand", 10) && !later("hand", 10, "hand", 20));
        assert!(later("hand", 1, FIRST, 99) && !later(FIRST, 99, "hand", 1));
    }
}
