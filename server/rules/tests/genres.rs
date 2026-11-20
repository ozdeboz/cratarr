//! The genres (decision 0097, amended 2026-11-20): the shipped list (GEN.8) and the one split, after the exceptions and
//! then a library's keep list (GEN.1 to GEN.3). Each scenario's Nevers are checked in its own test.

use cratarr_rules::genres::{self, Genres};

/// The genre set's first table, as shipped: each genre and its words, folded as `genres::key` folds them.
const TABLE: &[(&str, &[&str])] = &[
    ("Rock", &["rock"]),
    ("Pop", &["pop"]),
    ("Electronic", &["electronic"]),
    ("Dance", &["dance", "electronic dance music", "edm"]),
    ("Ambient", &["ambient"]),
    ("Experimental", &["experimental"]),
    ("Jazz", &["jazz"]),
    ("Blues", &["blues"]),
    ("Soul", &["soul"]),
    ("Funk", &["funk"]),
    ("R&B", &["r b", "rnb", "r n b"]),
    ("Hip Hop", &["hip hop", "hiphop", "rap"]),
    ("Reggae", &["reggae"]),
    ("Latin", &["latin"]),
    ("Folk", &["folk"]),
    ("Country", &["country"]),
    ("World", &["world", "world music"]),
    ("Classical", &["classical"]),
    ("Metal", &["metal"]),
    ("Punk", &["punk"]),
    ("Soundtrack", &["soundtrack"]),
    ("Spoken Word", &["spoken word"]),
    ("Children's", &["children s", "children s music"]),
    ("Religious", &["religious"]),
    ("Easy Listening", &["easy listening"]),
    ("New Age", &["new age"]),
];

fn texts(v: &[&str]) -> Vec<String> {
    v.iter().map(|w| w.to_string()).collect()
}

/// GEN.8: the 26 and their words, one shipped list, with the exceptions and the fixed links.
#[test]
fn the_shipped_list_holds_the_26_their_words_and_the_fixed_links() {
    assert_eq!(genres::SHIPPED, include_str!("../../../shared/genres.json"), "the rules read shared/genres.json");
    let list = genres::shipped();

    // The 26, by name, in order; each one's words folded as the table has them (C9: R&B's are r&b, rnb, r n b).
    let names: Vec<&str> = list.genres().iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, TABLE.iter().map(|(name, _)| *name).collect::<Vec<&str>>());
    assert_eq!(names.len(), 26, "never a 27th genre");
    for (genre, (name, words)) in list.genres().iter().zip(TABLE) {
        let folded: Vec<String> = genre.words.iter().map(|w| genres::key(w)).collect();
        assert_eq!(folded, *words, "{name}'s words");
        assert_eq!(list.genre_of(name), Some(*name), "{name} is its own genre");
    }

    // Their words fold by the key to their genre.
    let words = [
        ("Dance", "Dance"),
        ("EDM", "Dance"),
        ("Electronic Dance Music", "Dance"),
        ("R&B", "R&B"),
        ("rnb", "R&B"),
        ("RnB", "R&B"),
        ("R'n'B", "R&B"),
        ("Hip Hop", "Hip Hop"),
        ("hip-hop", "Hip Hop"),
        ("HipHop", "Hip Hop"),
        ("Rap", "Hip Hop"),
        ("World Music", "World"),
        ("Children's Music", "Children's"),
        ("easy listening", "Easy Listening"),
    ];
    for (word, genre) in words {
        assert_eq!(list.genre_of(word), Some(genre), "{word}");
        assert_eq!(list.genre_of_key(&genres::key(word)), Some(genre), "{word}, by its key");
    }

    // Not genres, compounds whole among them.
    let not = ["electronica", "indie", "alternative", "Funk / Soul", "Folk, World, & Country", "Stage & Screen", "Non-Music", "Brass & Military", "House", "Techno"];
    for word in not {
        assert_eq!(list.genre_of(word), None, "{word} is no genre");
    }

    // The exceptions: exactly the two.
    let exceptions: Vec<(&str, Vec<&str>)> = list.exceptions().iter().map(|e| (e.value.as_str(), e.words.iter().map(String::as_str).collect())).collect();
    assert_eq!(exceptions, [("RnB/Swing", vec!["R&B", "New Jack Swing"]), ("Cut-up/DJ", vec!["Turntablism"])]);

    // The fixed links.
    let links: Vec<(&str, Vec<&str>)> = list.links().iter().map(|l| (l.style.as_str(), l.genres.iter().map(String::as_str).collect())).collect();
    let fixed = [
        ("New Jack Swing", vec!["R&B"]),
        ("Turntablism", vec!["Hip Hop", "Electronic"]),
        ("Stage & Screen", vec!["Soundtrack"]),
        ("Non-Music", vec!["Spoken Word"]),
        ("Brass & Military", vec![]),
    ];
    for link in fixed {
        assert!(links.contains(&link), "{link:?} in {links:?}");
    }

    // Never a family above them: the list holds these fields alone, a genre its name and words alone.
    let raw: serde_json::Value = serde_json::from_str(genres::SHIPPED).unwrap();
    let mut fields: Vec<&str> = raw.as_object().unwrap().keys().map(String::as_str).collect();
    fields.sort_unstable();
    assert_eq!(fields, ["about", "exceptions", "genres", "links"]);
    for g in raw["genres"].as_array().unwrap() {
        let mut fields: Vec<&str> = g.as_object().unwrap().keys().map(String::as_str).collect();
        fields.sort_unstable();
        assert_eq!(fields, ["name", "words"], "{g}");
    }

    // Never a link from anything but a style to one of the 26.
    for l in list.links() {
        assert_eq!(list.genre_of(&l.style), None, "a link goes from a style: {}", l.style);
        assert!(l.genres.iter().all(|g| names.contains(&g.as_str())), "{} links to the 26 alone: {:?}", l.style, l.genres);
    }

    // And a list that breaks any of it is refused when read, so an edit of it fails here, not on a server.
    let refused = [
        (r#"{"genres":[{"name":"Rock","words":["rock"]}],"families":[]}"#, "a field beside the genres"),
        (r#"{"genres":[{"name":"Rock","words":["rock"],"family":"Guitar"}]}"#, "a family above a genre"),
        (r#"{"genres":[{"name":"Rock","words":["rock"]},{"name":"Pop","words":["Rock"]}]}"#, "a word of two genres"),
        (r#"{"genres":[{"name":"Rock","words":["rock"]}],"links":[{"style":"ROCK","genres":["Rock"]}]}"#, "a link from a genre"),
        (r#"{"genres":[{"name":"Rock","words":["rock"]}],"links":[{"style":"Surf","genres":["Guitar"]}]}"#, "a link to no genre"),
        (r#"{"genres":[{"name":"Rock","words":["rock"]}],"exceptions":[{"value":"A/B","words":[]}]}"#, "an exception of no words"),
        (r#"{"exceptions":[]}"#, "no genres"),
    ];
    for (json, why) in refused {
        assert!(Genres::from_json(json).is_err(), "{why}: {json}");
    }
    assert!(Genres::from_json(r#"{"genres":[{"name":"Rock","words":["rock"]}],"links":[{"style":"Surf","genres":["Rock"]}]}"#).is_ok());
}

/// GEN.1: the one split, on "/", ";" and ",", never on "&"; a leading "&" or "and" dropped (R1: the word "and").
#[test]
fn one_split_on_slash_semicolon_comma_never_ampersand() {
    let cases: &[(&str, &[&str])] = &[
        ("Funk / Soul", &["Funk", "Soul"]),
        ("Folk, World, & Country", &["Folk", "World", "Country"]),
        ("Folk, World, and Country", &["Folk", "World", "Country"]),
        ("House/Abstract/Experimental", &["House", "Abstract", "Experimental"]),
        ("Electronic; Downtempo", &["Electronic", "Downtempo"]),
        ("pop/rock", &["pop", "rock"]),
        ("contemporary pop/rock", &["contemporary pop", "rock"]),
        ("r&b/hip-hop", &["r&b", "hip-hop"]),
        ("Club/Dance", &["Club", "Dance"]),
        ("Drum & Bass", &["Drum & Bass"]),
        ("Rhythm & Blues", &["Rhythm & Blues"]),
        ("Stage & Screen", &["Stage & Screen"]),
        ("Dub & Reggae", &["Dub & Reggae"]),
        ("Rock and Roll", &["Rock and Roll"]),
        ("Andean Music", &["Andean Music"]),
        (" Jazz ; ;", &["Jazz"]),
        // Repeats dropped ignoring case, the first spelling kept.
        ("House/house; HOUSE, Techno", &["House", "Techno"]),
    ];
    for (value, want) in cases {
        let got = genres::split(value, &[]);
        assert_eq!(got, *want, "{value:?}");
        for w in &got {
            assert!(!w.is_empty(), "{value:?}: an empty word");
            assert!(!w.starts_with('&'), "{value:?}: {w:?}");
            assert!(!w.to_lowercase().starts_with("and "), "{value:?}: {w:?}");
            assert!(!["& Country", "and Country", "ean Music"].contains(&w.as_str()), "{value:?}: {w:?}");
        }
        // Never a split on "&": a value holding no separator is one word, whatever its "&".
        if !value.contains(genres::SEPARATORS) {
            assert_eq!(got, [value.trim()], "{value:?}");
        }
    }
}

/// GEN.2: the owner's exceptions list, applied before the split, ignoring case and inside a longer value (R2).
#[test]
fn the_owners_exceptions_come_before_the_split() {
    let cases: &[(&str, &[&str])] = &[
        ("RnB/Swing", &["R&B", "New Jack Swing"]),
        ("Cut-up/DJ", &["Turntablism"]),
        ("Hip Hop; RnB/Swing", &["Hip Hop", "R&B", "New Jack Swing"]),
        ("Cut-up/DJ, Abstract", &["Turntablism", "Abstract"]),
        ("rnb/swing", &["R&B", "New Jack Swing"]),
        // Jazz's Swing, and the two alone, as any word.
        ("Swing", &["Swing"]),
        ("Cut-up", &["Cut-up"]),
        ("DJ", &["DJ"]),
    ];
    for (value, want) in cases {
        let got = genres::split(value, &[]);
        assert_eq!(got, *want, "{value:?}");
        // Never "Swing", "DJ", "Cut-up" or "RnB" made of either exception: that would be a split before it.
        if value.to_lowercase().contains("rnb/swing") || value.to_lowercase().contains("cut-up/dj") {
            for made in ["Swing", "DJ", "Cut-up", "RnB"] {
                assert!(!got.iter().any(|w| w.eq_ignore_ascii_case(made)), "{value:?}: {made} made of an exception: {got:?}");
            }
        }
    }
}

/// GEN.3: a library's keep list keeps its own forms whole, after the exceptions.
#[test]
fn the_keep_list_keeps_a_librarys_own_forms_after_the_exceptions() {
    // The install's canon-tags pair.
    let first = texts(&["Folk, World, & Country", "Funk / Soul"]);
    assert_eq!(genres::split("Funk / Soul; Electronic", &first), ["Funk / Soul", "Electronic"]);
    assert_eq!(genres::split("Jazz/Rock/Folk, World, & Country", &first), ["Jazz", "Rock", "Folk, World, & Country"]);
    // Never a kept compound a genre: neither is one of the 26's words, and the shipped list links neither. Sorted, from
    // a file's GENRE, each is a style (the words store's sort, GEN.10).
    let list = genres::shipped();
    for kept in &first {
        assert_eq!(list.genre_of(kept), None, "{kept}");
        assert!(list.links().iter().all(|l| genres::key(&l.style) != genres::key(kept)), "{kept} is linked");
    }
    // Never the keep list undoing an exception: the exceptions took "RnB/Swing" before the keep list saw it.
    let second = texts(&["RnB/Swing"]);
    assert_eq!(genres::split("RnB/Swing", &second), ["R&B", "New Jack Swing"]);
    assert_eq!(genres::split("Soul; RnB/Swing", &second), ["Soul", "R&B", "New Jack Swing"]);
    // A keep list matches ignoring case, inside a longer value, as before.
    assert_eq!(genres::split("funk / soul/Jazz", &first), ["funk / soul", "Jazz"]);
}
