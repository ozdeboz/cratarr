//! Classical, a kind of record (0.10, the owner, 2026-10-30). Display only: the folders on disk and Autoplay stay as
//! they are.

/// The genres that open the gate (the owner's words, any case; "Neo-Classical" however it is hyphenated). A genre is
/// one of them as a whole: Indian Classical, or Classical Crossover, is not.
const GATE: [&str; 12] = [
    "classical",
    "opera",
    "modern classical",
    "contemporary classical",
    "neo classical",
    "neoclassical",
    "baroque",
    "romantic",
    "symphony",
    "concerto",
    "chamber music",
    "art song",
];

/// Whether a genre is one of the gate's.
pub fn classical_genre(genre: &str) -> bool {
    let key = crate::search::words(genre).join(" ");
    GATE.contains(&key.as_str())
}

/// Whether a record's genres (its files', MusicBrainz's, Discogs' by a verified link) open the gate: the tag alone is
/// never enough (`confirmed`).
pub fn gate<'a>(genres: impl IntoIterator<Item = &'a str>) -> bool {
    genres.into_iter().any(classical_genre)
}
