//! The genres (decision 0097, amended 2026-11-20): the shipped list (GEN.8) and the one split in three layers, the
//! exceptions first, then the rule of where a value was written, a library's or a service's (GEN.1 to GEN.3). Each
//! scenario's Nevers are checked in its own test.

use cratarr_rules::genres::{self, Genres, rules};

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

/// A library's rules when it sets none of its own: "/", ";" and ",", no keep list.
fn library_defaults() -> Vec<String> {
    texts(&["/", ";", ","])
}

/// Whether a value holds one of the shipped exceptions' forms, ignoring case.
fn holds_an_exception(value: &str) -> bool {
    let value = value.to_lowercase();
    genres::shipped().exceptions().iter().any(|e| value.contains(&e.value.to_lowercase()))
}

/// The Nevers every split keeps, under any rule: no empty word, and no "& Country", "and Country" or "ean Music"; where
/// "," splits, no part's leading "&" or "and" kept; and no split on "&": a value holding none of the rule's separators
/// and no exception's form is one word, whatever its "&".
fn never(value: &str, separators: &[String], got: &[String], rule: &str) {
    for w in got {
        assert!(!w.is_empty(), "{rule}: {value:?} gave an empty word");
        assert!(!["& Country", "and Country", "ean Music"].contains(&w.as_str()), "{rule}: {value:?} gave {w:?}");
        if separators.iter().any(|s| s.trim() == ",") {
            assert!(!w.starts_with('&') && !w.to_lowercase().starts_with("and "), "{rule}: {value:?} kept a leading word: {w:?}");
        }
    }
    if !holds_an_exception(value) && !separators.iter().any(|s| value.contains(s.as_str())) {
        assert_eq!(got, [value.trim()], "{rule}: {value:?} is one word");
    }
}

/// GEN.1: each value splits by the rule of where it was written (GEN-DESIGN §4's three layers): the exceptions first
/// under every rule, then that rule, passed in; "&" never splits; a "," split drops a part's leading "&" or "and" (R1:
/// the word "and"; OQ-GEN-21).
#[test]
fn each_value_splits_by_the_rule_of_where_it_was_written() {
    // (a) A library's files, by that library's rules; here the defaults.
    let library: &[(&str, &[&str])] = &[
        ("Funk / Soul", &["Funk", "Soul"]),
        ("Folk, World, & Country", &["Folk", "World", "Country"]),
        ("Folk, World, and Country", &["Folk", "World", "Country"]),
        ("House/Abstract/Experimental", &["House", "Abstract", "Experimental"]),
        ("Electronic; Downtempo", &["Electronic", "Downtempo"]),
        ("Drum/Bass", &["Drum", "Bass"]),
        ("Drum & Bass", &["Drum & Bass"]),
        ("Rhythm & Blues", &["Rhythm & Blues"]),
        ("Stage & Screen", &["Stage & Screen"]),
        ("Dub & Reggae", &["Dub & Reggae"]),
        ("Rock and Roll", &["Rock and Roll"]),
        (" Jazz ; ;", &["Jazz"]),
        // Repeats dropped ignoring case, the first spelling kept, under every rule.
        ("House/house; HOUSE, Techno", &["House", "Techno"]),
    ];
    // (b) Discogs, its fixed rule: "/" and ","; a part's leading "&" or "and" dropped.
    let discogs: &[(&str, &[&str])] = &[
        ("Funk / Soul", &["Funk", "Soul"]),
        ("Folk, World, & Country", &["Folk", "World", "Country"]),
        ("Stage & Screen", &["Stage & Screen"]),
        ("Brass & Military", &["Brass & Military"]),
        ("Non-Music", &["Non-Music"]),
        ("Andean Music", &["Andean Music"]),
        ("RnB/Swing", &["R&B", "New Jack Swing"]),
        ("Cut-up/DJ", &["Turntablism"]),
        ("Funk / Soul, funk", &["Funk", "Soul"]),
    ];
    // (c) Last.fm and Bandcamp, no split: each tag is one value already. The exceptions still come first.
    let lastfm: &[(&str, &[&str])] = &[("Club/Dance", &["Club/Dance"]), ("RnB/Swing", &["R&B", "New Jack Swing"])];
    let bandcamp: &[(&str, &[&str])] = &[
        ("hip-hop/rap", &["hip-hop/rap"]),
        ("drum & bass", &["drum & bass"]),
        ("RnB/Swing", &["R&B", "New Jack Swing"]),
    ];
    // (d) A typed word: the exceptions apply, the split doesn't (OQ-GEN-24).
    let typed: &[(&str, &[&str])] = &[
        ("Drum/Bass", &["Drum/Bass"]),
        ("Funk / Soul", &["Funk / Soul"]),
        ("RnB/Swing", &["R&B", "New Jack Swing"]),
    ];
    // (e) MusicBrainz, its fixed rule: "/" only (OQ-GEN-20).
    let musicbrainz: &[(&str, &[&str])] = &[
        ("pop/rock", &["pop", "rock"]),
        ("contemporary pop/rock", &["contemporary pop", "rock"]),
        ("r&b/hip-hop", &["r&b", "hip-hop"]),
    ];
    // (f) AudioMuse, no split: each name as its line gives it.
    let audiomuse: &[(&str, &[&str])] = &[("Hip-Hop", &["Hip-Hop"]), ("Progressive rock", &["Progressive rock"])];

    let groups = [
        ("a library's files, the defaults", library_defaults(), library),
        ("Discogs", rules::separators(rules::DISCOGS), discogs),
        ("Last.fm", rules::separators(rules::LASTFM), lastfm),
        ("Bandcamp", rules::separators(rules::BANDCAMP), bandcamp),
        ("typed", rules::separators(rules::TYPED), typed),
        ("MusicBrainz", rules::separators(rules::MUSICBRAINZ), musicbrainz),
        ("AudioMuse", rules::separators(rules::AUDIOMUSE), audiomuse),
    ];
    for (rule, separators, cases) in &groups {
        for (value, want) in cases.iter() {
            let got = genres::split(value, separators, &[]);
            assert_eq!(got, *want, "{rule}: {value:?}");
            never(value, separators, &got, rule);
        }
    }

    // Never MusicBrainz's or AudioMuse's words split on ";" or ",".
    for (rule, separators) in [("MusicBrainz", rules::MUSICBRAINZ), ("AudioMuse", rules::AUDIOMUSE)] {
        assert_eq!(genres::split("rock; pop, jazz", &rules::separators(separators), &[]), ["rock; pop, jazz"], "{rule}");
    }
    // Never a split on "&" under any rule: "&" is no genre separator, so a rule holding it splits on its others alone.
    assert_eq!(genres::split("Drum & Bass; R&B", &texts(&["&", " & ", ";"]), &[]), ["Drum & Bass", "R&B"]);
}

/// GEN.2: the owner's exceptions list, applied first, ignoring case and inside a longer value (R2): its rows under a
/// library's default rules, and the forms alone under the no-split rule (Last.fm's, Bandcamp's, a typed word's).
#[test]
fn the_owners_exceptions_come_before_the_split() {
    let library: &[(&str, &[&str])] = &[
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
    let unsplit: &[(&str, &[&str])] = &[
        ("RnB/Swing", &["R&B", "New Jack Swing"]),
        ("rnb/swing", &["R&B", "New Jack Swing"]),
        ("Cut-up/DJ", &["Turntablism"]),
    ];
    let defaults = library_defaults();
    let no_split: Vec<String> = Vec::new();
    for (separators, cases) in [(&defaults, library), (&no_split, unsplit)] {
        for (value, want) in cases {
            let got = genres::split(value, separators, &[]);
            assert_eq!(got, *want, "{value:?} by {separators:?}");
            // Never "Swing", "DJ", "Cut-up" or "RnB" made of either exception: that would be a split before it.
            if holds_an_exception(value) {
                for made in ["Swing", "DJ", "Cut-up", "RnB"] {
                    assert!(!got.iter().any(|w| w.eq_ignore_ascii_case(made)), "{value:?}: {made} made of an exception: {got:?}");
                }
            }
        }
    }
}

/// GEN.3: each library splits its files by its own rules, after the exceptions; each service by its own rule, alike in
/// every library (GEN-DESIGN §4, OQ-GEN-22).
#[test]
fn each_library_splits_its_files_by_its_own_rules_and_each_service_by_its_own() {
    // Each library's tag rules: its separators and its keep list. MUSIC's keep list is the install's canon-tags pair.
    let music = (texts(&["/", ";", ","]), texts(&["Folk, World, & Country", "Funk / Soul"]));
    let incoming = (texts(&["/", ";", ","]), Vec::<String>::new());
    let archive = (texts(&[";"]), texts(&["RnB/Swing"]));
    let libraries = [("MUSIC", &music), ("IN", &incoming), ("ARCHIVE", &archive)];

    // A record's files' GENRE in each library, and its words there: MUSIC's, IN's, ARCHIVE's.
    let rows: &[(&str, &[&str], &[&str], &[&str])] = &[
        ("Funk / Soul; Electronic", &["Funk / Soul", "Electronic"], &["Funk", "Soul", "Electronic"], &["Funk / Soul", "Electronic"]),
        (
            "Jazz/Rock/Folk, World, & Country",
            &["Jazz", "Rock", "Folk, World, & Country"],
            &["Jazz", "Rock", "Folk", "World", "Country"],
            &["Jazz/Rock/Folk, World, & Country"],
        ),
        ("Drum/Bass; Jazz", &["Drum", "Bass", "Jazz"], &["Drum", "Bass", "Jazz"], &["Drum/Bass", "Jazz"]),
        // Never the keep list undoing an exception: the exceptions took "RnB/Swing" before ARCHIVE's keep list saw it.
        ("RnB/Swing; Jazz", &["R&B", "New Jack Swing", "Jazz"], &["R&B", "New Jack Swing", "Jazz"], &["R&B", "New Jack Swing", "Jazz"]),
    ];
    // Never one library's rules on another's files: each row's three answers differ where the rules do.
    for (value, in_music, in_incoming, in_archive) in rows {
        for ((name, (separators, keep)), want) in libraries.iter().zip([in_music, in_incoming, in_archive]) {
            let got = genres::split(value, separators, keep);
            assert_eq!(got, *want, "{name}: {value:?}");
            never(value, separators, &got, name);
        }
    }
    // A keep list matches ignoring case, inside a longer value, as before.
    assert_eq!(genres::split("funk / soul/Jazz", &music.0, &music.1), ["funk / soul", "Jazz"]);

    // The services' words, alike in every library: each by its own rule, never the library's. Discogs' "Funk / Soul" is
    // Funk and Soul on all three records, ARCHIVE's included; Last.fm's "Club/Dance" is one word on all three.
    let (discogs, lastfm) = (rules::separators(rules::DISCOGS), rules::separators(rules::LASTFM));
    assert_eq!(genres::split("Funk / Soul", &discogs, &[]), ["Funk", "Soul"]);
    assert_eq!(genres::split("Club/Dance", &lastfm, &[]), ["Club/Dance"]);
    // Never a library's rules on a service's words: ARCHIVE's would keep Discogs' "Funk / Soul" whole, and MUSIC's and
    // IN's "/" would split Last.fm's "Club/Dance".
    assert_eq!(genres::split("Funk / Soul", &archive.0, &archive.1), ["Funk / Soul"]);
    for (separators, keep) in [&music, &incoming] {
        assert_eq!(genres::split("Club/Dance", separators, keep), ["Club", "Dance"]);
    }

    // Sorted (the words store's sort, GEN.10): "Funk / Soul" and "Folk, World, & Country", kept whole, are no genre and
    // the shipped list links neither, so from a file's GENRE each is a style. ARCHIVE's "Drum/Bass" is one word, folded
    // as "Drum & Bass" is, and no genre; nor are MUSIC's and IN's Drum and Bass.
    let list = genres::shipped();
    for kept in &music.1 {
        assert_eq!(list.genre_of(kept), None, "{kept}");
        assert!(list.links().iter().all(|l| genres::key(&l.style) != genres::key(kept)), "{kept} is linked");
    }
    assert_eq!(genres::key("Drum/Bass"), "drum bass");
    assert_eq!(genres::key("Drum/Bass"), genres::key("Drum & Bass"));
    for word in ["Drum/Bass", "Drum", "Bass"] {
        assert_eq!(list.genre_of(word), None, "{word}");
    }
}

/// GEN's links (GEN-DESIGN §5): the shipped map is GENRE-SET-DRAFT.md's links as the owner approved them (2026-11-21 02:58,
/// "The draft is entirely okay."): every style of its "By genre" lists to the genres it is listed under, the five fixed links
/// among them; "Instrumental" with no link (the owner's point, as proposed); the unlinked styles and the words that are no
/// style with none. A style is found by its key.
#[test]
fn the_shipped_links_are_the_approved_genre_set() {
    let list = genres::shipped();
    assert_eq!(list.links().len(), 407, "the 402 styles the set links, besides \"Instrumental\", and the five fixed");
    let approved: &[(&str, &[&str])] = &[
        ("Deep House", &["Dance"]),
        ("deep-house", &["Dance"]),
        ("DUB", &["Electronic", "Reggae"]),
        ("Downtempo", &["Electronic"]),
        ("Acid Jazz", &["Electronic", "Jazz", "Soul", "Funk", "Hip Hop"]),
        ("Psychedelic", &["Electronic", "Soul", "Funk"]),
        ("Pop Rock", &["Rock", "Pop"]),
        ("Neo Soul", &["Soul", "R&B"]),
        ("Ambient Dub", &["Ambient", "Reggae"]),
        ("Latin Jazz", &["Jazz", "Latin"]),
        ("Drill And Bass", &["Electronic", "Dance"]),
        ("Ragga HipHop", &["Hip Hop", "Reggae"]),
        ("Jazz-Funk", &["Jazz"]),
        ("Leftfield", &["Electronic"]),
        ("Modern Classical", &["Classical"]),
        ("Contemporary", &["Classical"]),
        ("Gospel", &["Religious"]),
        ("Lounge", &["Easy Listening"]),
        ("Score", &["Soundtrack"]),
        ("Poetry", &["Spoken Word"]),
        ("Afrobeat", &["Funk"]),
        ("African", &["World"]),
        ("Neofolk", &["Folk"]),
        ("Americana", &["Country"]),
        ("Funeral Doom Metal", &["Metal"]),
        ("Piano Blues", &["Blues"]),
        ("Drone", &["Ambient"]),
        ("Field Recording", &["Experimental"]),
        ("Contemporary R&B", &["R&B"]),
        ("Reggaeton", &["Latin"]),
        ("Bossanova", &["Latin"]),
        ("Bossa Nova", &["Jazz"]),
        // The five fixed (GEN-DESIGN §3, §5).
        ("New Jack Swing", &["R&B"]),
        ("Turntablism", &["Hip Hop", "Electronic"]),
        ("Stage & Screen", &["Soundtrack"]),
        ("Non-Music", &["Spoken Word"]),
        ("Brass & Military", &[]),
    ];
    for (style, want) in approved {
        assert_eq!(list.genres_of(style), *want, "{style}");
    }
    // No link: Instrumental (the owner's point); the styles nobody gave one of the 26; the words that are no style (Health's); a
    // genre's own word; an exception's written form (read as its words first).
    let none = [
        "Instrumental", "chillout", "Progressive", "Cut-up", "DJ", "Rock Pop", "alternative", "Singer-Songwriter", "Électronique", "Afro", "Darkwave",
        "Down House", "Dub & Reggae", "french indie pop", "Future", "Hauntology", "Improv", "Ragtime", "Chamber", "op. 108", "In a Silent Way", "Dance",
        "Cut-up/DJ", "RnB/Swing",
    ];
    for word in none {
        assert!(list.genres_of(word).is_empty(), "{word}: {:?}", list.genres_of(word));
        assert!(list.links().iter().all(|l| genres::key(&l.style) != genres::key(word)), "{word} is in the map");
    }
    // Every genre with styles in the set has some linked to it; Punk, Children's and New Age have none there.
    for g in list.genres() {
        let linked = list.links().iter().filter(|l| l.genres.contains(&g.name)).count();
        assert_eq!(linked == 0, ["Punk", "Children's", "New Age"].contains(&g.name.as_str()), "{}: {linked} styles", g.name);
    }
}
