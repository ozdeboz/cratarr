use std::sync::Arc;

use serde_json::{Value as Json, json};

use super::*;
use crate::crates::{Value, like_crate};
use crate::index::own::{Kept, List, Own};
use crate::index::{Artist, Kind, Library, Mark, Marks, Record, RecordCopy, Rows, Sounds, Track};
use crate::similarity::Signal;

const NOW: i64 = 1_800_000_000;
const MUSIC: &str = "/music";
const OTHER: &str = "/other";

fn track(id: i64, record: i64, title: &str, artist: i64, library: i64, f: Facts) -> Track {
    let line = format!("{}/{title}.flac", if library == 0 { "A" } else { "../other/B" });
    let facts = Facts { rel: line, folders: vec![library], track: Some(id), artists: vec![format!("artist {artist}")], credits: vec![format!("artist{artist}")], credit_names: vec![format!("artist {artist}")], born: Some(NOW - 86_400 * id), ..f };
    Track { id, record, disc: Some(1), number: Some(id as u32), title: title.to_string(), artist: Some(format!("artist {artist}")), artist_ids: vec![artist], facts }
}

/// Six tracks on three records, of two artists, in two libraries: `one`..`four` in Music, `five` and `six` in the other.
fn rows() -> Rows {
    let genre = |g: &str, bpm: f64, length: f64| Facts { genres: vec![g.to_string()], bpm: Some(bpm), length: Some(length), year: Some(2000.0), ..Facts::default() };
    let record = |id: i64, title: &str, artist: i64, library: i64| Record { id, title: title.into(), sort: title.into(), artist: format!("artist {artist}"), artist_ids: vec![artist], copies: vec![RecordCopy { library, dir: title.into(), tracks: 2, ..RecordCopy::default() }], ..Record::default() };
    Rows {
        libraries: vec![
            Library { id: 0, key: MUSIC.into(), name: "Music".into(), kind: Kind::Main, order: 0, ..Library::default() },
            Library { id: 1, key: OTHER.into(), name: "Other".into(), kind: Kind::Own, order: 1, ..Library::default() },
        ],
        artists: vec![Artist { id: 1, name: "artist 1".into(), sort: "artist 1".into(), spelled: vec![] }, Artist { id: 2, name: "artist 2".into(), sort: "artist 2".into(), spelled: vec![] }],
        records: vec![record(1, "First", 1, 0), record(2, "Second", 2, 0), record(3, "Third", 2, 1)],
        tracks: vec![
            track(1, 1, "one", 1, 0, genre("Dub", 100.0, 60.0)),
            track(2, 1, "two 10", 1, 0, genre("Dub", 110.0, 60.0)),
            track(3, 2, "two 9", 2, 0, genre("Dub", 120.0, 60.0)),
            track(4, 2, "four", 2, 0, genre("Jazz", 130.0, 60.0)),
            track(5, 3, "five", 2, 1, genre("Jazz", 140.0, 60.0)),
            track(6, 3, "six", 2, 1, genre("Dub", 150.0, 60.0)),
        ],
    }
}

fn crate_of(id: &str, conditions: Vec<Condition>) -> Kept {
    Kept { id: id.into(), name: id.into(), conditions, listed: true, ..Kept::default() }
}

fn dub() -> Vec<Condition> {
    vec![Condition::new("genre", false, Value::AnyOf { values: vec!["Dub".into()] })]
}

fn device_of(own: Own, marks: &[Mark]) -> Device {
    let index = Index::build(1, &rows()).unwrap();
    let sounds = Sounds::build(1, &(1..=6).map(|t| (t, vec![t as f32, 7.0 - t as f32, 1.0])).collect::<Vec<_>>());
    let bytes = Marks::build(NOW - 3600, marks).with_own(serde_json::to_string(&own).unwrap()).save();
    Device::new(index, Some(&sounds), &Marks::load(&bytes).unwrap())
}

fn device() -> Device {
    device_of(Own { crates: vec![crate_of("dub", dub())], ..Own::default() }, &[])
}

fn ask(d: &mut Device, asked: Json) -> Json {
    serde_json::from_str(&d.ask(&asked.to_string()).unwrap_or_else(|| panic!("no answer to {asked}"))).unwrap()
}

/// A crate's rows by their track numbers, as a list shows them.
fn tracks(d: &mut Device, id: &str, look: Json) -> Vec<i64> {
    let mut asked = look;
    asked.as_object_mut().unwrap().extend([("ask".into(), json!("rows")), ("id".into(), json!(id)), ("offset".into(), json!(0)), ("limit".into(), json!(100)), ("now".into(), json!(NOW))]);
    ask(d, asked)["rows"].as_array().unwrap().iter().map(|r| ids::track_of(r["id"].as_str().unwrap()).unwrap()).collect()
}

fn own_order() -> Json {
    json!({ "order": "own" })
}

#[test]
fn a_crate_holds_what_its_conditions_hold_newest_first() {
    assert_eq!(tracks(&mut device(), "dub", own_order()), [1, 2, 3, 6]);
}

#[test]
fn a_page_of_rows_says_the_whole_and_what_a_list_shows() {
    let mut d = device();
    let page = ask(&mut d, json!({ "ask": "rows", "id": "dub", "order": "own", "offset": 1, "limit": 2, "now": NOW }));
    assert_eq!(page["total"], 4);
    assert_eq!(page["rows"].as_array().unwrap().len(), 2);
    let row = &page["rows"][0];
    assert_eq!((row["id"].as_str(), row["name"].as_str(), row["album"].as_str(), row["album_artist"].as_str()), (Some("tr-2"), Some("two 10"), Some("First"), Some("artist 1")));
    assert_eq!((row["entry"].as_str(), row["file"].as_str(), row["album_id"].as_str(), row["album_tracks"].as_u64()), (Some("A/two 10.flac"), Some("two 10.flac"), Some("al-1"), Some(2)));
    assert_eq!((row["missing"].as_bool(), row["disc"].as_u64(), row["bpm"].as_u64()), (Some(false), Some(1), Some(110)));
    assert!(page["genres"].as_array().unwrap().is_empty(), "the first page's alone");
    let first = ask(&mut d, json!({ "ask": "rows", "id": "dub", "order": "own", "offset": 0, "limit": 1, "now": NOW }));
    assert_eq!(first["genres"], json!(["Dub"]));
}

#[test]
fn best_match_goes_by_the_servers_last_fits_those_it_knows_nothing_of_after() {
    let marks = [Mark { track: 3, fit: Some(0.9), ..Mark::default() }, Mark { track: 6, fit: Some(0.5), ..Mark::default() }];
    let mut d = device_of(Own { crates: vec![crate_of("dub", dub())], ..Own::default() }, &marks);
    assert_eq!(tracks(&mut d, "dub", json!({})), [3, 6, 1, 2], "a crate opens on it, asked for or not");
    assert_eq!(tracks(&mut d, "dub", json!({ "down": true })), [2, 1, 6, 3]);
}

#[test]
fn a_list_orders_by_a_field_as_the_servers_does() {
    let mut d = device();
    assert_eq!(tracks(&mut d, "dub", json!({ "order": "title" })), [1, 6, 3, 2], "two 9 before two 10: numbers by what they count");
    assert_eq!(tracks(&mut d, "dub", json!({ "order": "tempo", "down": true })), [6, 3, 2, 1]);
    assert_eq!(tracks(&mut d, "dub", json!({ "order": "own", "down": true })), [6, 3, 2, 1]);
}

#[test]
fn a_count_is_the_servers_last_and_this_devices_plays_not_told_yet() {
    let often = vec![Condition::new("plays", false, Value::Range { min: Some(3.0), max: None })];
    let marks = [Mark { track: 1, plays: 2, last: Some(NOW - 500), ..Mark::default() }];
    let mut d = device_of(Own { crates: vec![crate_of("often", often)], ..Own::default() }, &marks);
    assert!(tracks(&mut d, "often", own_order()).is_empty(), "two plays are not three");
    ask(&mut d, json!({ "ask": "plays", "plays": [{ "source": "cratarr", "track": "tr-1", "at": NOW * 1000, "counts": true }] }));
    assert_eq!(tracks(&mut d, "often", own_order()), [1]);
    let row = &ask(&mut d, json!({ "ask": "rows", "id": "often", "order": "own", "offset": 0, "limit": 1, "now": NOW }))["rows"][0];
    assert_eq!((row["play_count"].as_u64(), row["last_played"].as_i64()), (Some(3), Some(NOW)));
    ask(&mut d, json!({ "ask": "plays", "plays": [] }));
    assert!(tracks(&mut d, "often", own_order()).is_empty(), "told, it is the server's to say");
}

#[test]
fn a_play_that_did_not_count_is_no_play_a_skip_counts_as_one() {
    let skipped = vec![Condition::new("skips", false, Value::Range { min: Some(1.0), max: None })];
    let often = vec![Condition::new("plays", false, Value::Range { min: Some(1.0), max: None })];
    let mut d = device_of(Own { crates: vec![crate_of("skipped", skipped), crate_of("often", often)], ..Own::default() }, &[]);
    ask(&mut d, json!({ "ask": "plays", "plays": [
        { "track": "tr-2", "at": NOW * 1000, "counts": false, "ended": "skipped" },
        { "track": "tr-3", "at": NOW * 1000, "counts": false, "ended": "stopped" },
        { "source": "elsewhere", "track": "tr-4", "at": NOW * 1000 },
    ] }));
    assert_eq!(tracks(&mut d, "skipped", own_order()), [2]);
    assert!(tracks(&mut d, "often", own_order()).is_empty());
}

/// The app hands in the plays not told yet before every ask: handed in again they count once, more after them count each, and
/// a list with the first gone (told to Cratarr) counts as it is. At each step, as a device handed that list once.
#[test]
fn plays_handed_in_before_every_ask_count_as_the_list_handed_in_once() {
    let played = vec![Condition::new("plays", false, Value::Range { min: Some(1.0), max: None })];
    let open = || device_of(Own { crates: vec![crate_of("played", played.clone())], ..Own::default() }, &[Mark { track: 1, plays: 1, last: Some(NOW - 900), ..Mark::default() }]);
    let counts = |d: &mut Device, plays: &Json| {
        ask(d, json!({ "ask": "plays", "plays": plays }));
        let rows = ask(d, json!({ "ask": "rows", "id": "played", "order": "own", "offset": 0, "limit": 10, "now": NOW }));
        rows["rows"].as_array().unwrap().iter().map(|r| (r["id"].clone(), r["play_count"].clone(), r["last_played"].clone())).collect::<Vec<_>>()
    };
    let play = |track: i64, ago: i64| json!({ "track": ids::song(track), "at": (NOW - ago) * 1000 });
    let (a, b, c, d) = (play(1, 30), play(2, 20), play(1, 10), play(3, 5));
    let mut held = open();
    for plays in [json!([a]), json!([a, b]), json!([a, b]), json!([a, b, c, d]), json!([b, c, d]), json!([c, d, a]), json!([d]), json!([])] {
        assert_eq!(counts(&mut held, &plays), counts(&mut open(), &plays), "{plays}");
    }
    assert_eq!(counts(&mut held, &json!([a, b, c])), [(json!("tr-1"), json!(3), json!(NOW - 10)), (json!("tr-2"), json!(1), json!(NOW - 20))]);
}

#[test]
fn what_was_left_out_and_excluded_stays_out_of_a_crate() {
    let left = Kept { left_out: vec![2], left: 1, ..crate_of("dub", dub()) };
    let mut d = device_of(Own { crates: vec![left], excluded: vec![3], ..Own::default() }, &[]);
    assert_eq!(tracks(&mut d, "dub", own_order()), [1, 6]);
}

#[test]
fn an_artist_excluded_everywhere_leaves_every_crate() {
    let mut d = device_of(Own { crates: vec![crate_of("dub", dub())], artists_out: vec!["artist2".into()], ..Own::default() }, &[]);
    assert_eq!(tracks(&mut d, "dub", own_order()), [1, 2]);
}

#[test]
fn a_fixed_crates_toggles_are_as_saved_until_a_page_shows_them_otherwise() {
    let genre = Kept { favourites: Favourites::Only, ..crate_of("genre:Dub", crates::genre_crate("genre:Dub").unwrap().conditions) };
    let mut d = device_of(Own { crates: vec![genre], favourites: vec![1, 6], ..Own::default() }, &[]);
    assert_eq!(tracks(&mut d, "genre:Dub", own_order()), [1, 6]);
    assert_eq!(tracks(&mut d, "genre:Dub", json!({ "order": "own", "favourites": "all" })), [1, 2, 3, 6]);
}

#[test]
fn a_word_crate_is_made_when_asked_for_and_shows_all_its_tracks() {
    let mut d = device_of(Own { favourites: vec![1], ..Own::default() }, &[]);
    assert_eq!(tracks(&mut d, "genre:Jazz", own_order()), [4, 5]);
    assert_eq!(tracks(&mut d, "genre:Jazz", json!({ "order": "own", "favourites": "only" })), Vec::<i64>::new());
    assert_eq!(tracks(&mut d, "genre:Dub", json!({ "order": "own", "favourites": "only" })), [1]);
    assert_eq!(tracks(&mut d, "genre:Dub", json!({ "order": "own", "favourites": "none" })), [2, 3, 6]);
}

#[test]
fn a_favourite_album_is_every_track_of_its_record() {
    let mut d = device_of(Own { favourite_albums: vec![2], ..Own::default() }, &[]);
    assert_eq!(tracks(&mut d, "genre:Dub", json!({ "order": "own", "favourite_albums": "only" })), [3]);
}

#[test]
fn a_library_switched_off_takes_what_lies_only_there_and_says_so() {
    let mut d = device();
    assert_eq!(tracks(&mut d, "dub", json!({ "order": "own", "without": ["cratarr//other"] })), [1, 2, 3]);
    let mut other = device_of(Own { crates: vec![crate_of("jazz", vec![Condition::new("genre", false, Value::AnyOf { values: vec!["Jazz".into()] })])], ..Own::default() }, &[]);
    assert_eq!(tracks(&mut other, "jazz", json!({ "order": "own", "without": ["cratarr//music"] })), [5]);
}

#[test]
fn a_playlist_is_read_by_its_name_and_unlisted_by_being_in_none() {
    let list = List { id: "Mix".into(), name: "Mix".into(), tracks: vec![1, 2] };
    let listed = vec![Condition::new("playlist", false, Value::AnyOf { values: vec!["Mix".into()] })];
    let unlisted = vec![Condition::new("playlist", true, Value::AnyOf { values: Vec::new() })];
    let mut d = device_of(Own { crates: vec![crate_of("listed", listed), crate_of("unlisted", unlisted)], lists: vec![list], ..Own::default() }, &[]);
    assert_eq!(tracks(&mut d, "listed", own_order()), [1, 2]);
    assert_eq!(tracks(&mut d, "unlisted", own_order()), [3, 4, 5, 6]);
}

#[test]
fn fits_your_taste_is_the_best_quarter_of_what_is_on() {
    let fit = vec![Condition::new("fit", false, Value::Yes)];
    let marks: Vec<Mark> = (1..=6).map(|t| Mark { track: t, fit: Some(t as f64 / 10.0), ..Mark::default() }).collect();
    let mut d = device_of(Own { crates: vec![crate_of("fits", fit.clone())], ..Own::default() }, &marks);
    assert_eq!(tracks(&mut d, "fits", own_order()), [5, 6], "a quarter of six, rounded up");
    assert_eq!(tracks(&mut d, "fits", json!({ "order": "own", "without": ["cratarr//other"] })), [4], "of what is on");
    let mut no_taste = device_of(Own { crates: vec![crate_of("fits", fit)], ..Own::default() }, &[]);
    assert!(no_taste.ask(&json!({ "ask": "rows", "id": "fits", "offset": 0, "limit": 5, "now": NOW }).to_string()).is_none(), "no taste: nothing to say, as the server refuses");
}

#[test]
fn home_s_shelves_show_their_last_copy_of_every_library() {
    let shelf = Kept { shelf: Some(vec![5, 1]), permanent: true, ..crate_of("home:new", Vec::new()) };
    let mut d = device_of(Own { crates: vec![shelf], ..Own::default() }, &[]);
    assert_eq!(tracks(&mut d, "home:new", own_order()), [5, 1], "as Home showed them");
    assert_eq!(tracks(&mut d, "home:new", json!({ "order": "own", "without": ["cratarr//other"] })), [5, 1], "whatever is switched off");
    let def = ask(&mut d, json!({ "ask": "crate", "id": "home:new" }));
    assert_eq!(def["conditions"], json!([]), "a list of lines, not conditions anyone sets");
}

#[test]
fn a_shelf_holds_to_what_was_added_to_it() {
    let shelf = Kept { shelf: Some(vec![5, 1]), ..crate_of("home:new", crates::with_not_artist(&[], "artist 2").unwrap()) };
    let mut d = device_of(Own { crates: vec![shelf], ..Own::default() }, &[]);
    assert_eq!(tracks(&mut d, "home:new", own_order()), [1]);
}

#[test]
fn a_sounds_like_is_made_on_the_device_and_is_the_true_list() {
    let mut d = device();
    let like = tracks(&mut d, "like:tr-1@10", own_order());
    assert_eq!(like.len(), 6, "every track, here or not: a list leaves nothing out");
    assert_eq!(like[0], 1, "closest first");
    assert_eq!(ask(&mut d, json!({ "ask": "crate", "id": "like:tr-1" }))["name"], "Sounds like one");
    let record = tracks(&mut d, "like:al-1@10", own_order());
    assert!(record.len() >= 2 && record[..2].contains(&1) && record[..2].contains(&2), "{record:?}");
    let of_record = ask(&mut d, json!({ "ask": "crate", "id": "like:al-1" }));
    assert_eq!(of_record["name"], "Sounds like First");
    assert_eq!(of_record["conditions"][0]["value"]["of"], json!(["tr-1", "tr-2"]), "all of it");
    let artist = tracks(&mut d, "like:ar-1@10", own_order());
    assert!(!artist.contains(&1) && !artist.contains(&2), "an artist's own are the seed: {artist:?}");
    assert!(d.ask(&json!({ "ask": "rows", "id": "like:tr-99", "offset": 0, "limit": 5, "now": NOW }).to_string()).is_none(), "of what it does not hold: nothing to say");
}

#[test]
fn a_sounds_like_kept_with_what_it_learned_goes_by_it_at_any_size() {
    let of = vec!["tr-1".to_string()];
    let kept = Kept { left_out: vec![2], ..crate_of("like:tr-1", like_crate("like:tr-1", "Sounds like one", of, 3).conditions) };
    let mut d = device_of(Own { crates: vec![kept], ..Own::default() }, &[]);
    let like = tracks(&mut d, "like:tr-1@5", own_order());
    assert_eq!(like.len(), 5);
    assert!(!like.contains(&2), "turned down for it: {like:?}");
}

#[test]
fn a_sounds_like_of_a_playlist_is_read_from_its_tracks() {
    let list = List { id: "people/Mix".into(), name: "Mix".into(), tracks: vec![1, 2] };
    let mut d = device_of(Own { lists: vec![list], ..Own::default() }, &[]);
    let like = tracks(&mut d, "like:list:people/Mix@10", own_order());
    assert!(!like.contains(&1) && !like.contains(&2) && !like.is_empty(), "its own left out: {like:?}");
    assert_eq!(ask(&mut d, json!({ "ask": "crate", "id": "like:list:people/Mix" }))["name"], "Sounds like Mix");
}

#[test]
fn with_no_crates_kept_there_is_no_list_to_give() {
    let asked = json!({ "ask": "crates", "without": [], "now": NOW }).to_string();
    assert!(device_of(Own::default(), &[]).ask(&asked).is_none(), "an empty page would say the account has none");
}

#[test]
fn a_moment_says_nothing_here() {
    assert!(device().ask(&json!({ "ask": "rows", "id": "like:now", "offset": 0, "limit": 5, "now": NOW }).to_string()).is_none());
}

#[test]
fn a_crates_tempo_counts_its_tracks() {
    let mut d = device();
    let bands = ask(&mut d, json!({ "ask": "tempo", "id": "dub", "order": "own", "now": NOW }));
    assert_eq!((bands["with_tempo"].as_u64(), bands["total"].as_u64()), (Some(4), Some(4)), "{bands}");
}

#[test]
fn a_crates_definition_names_what_it_names_by_id() {
    let mut d = device_of(Own { crates: vec![crate_of("record", vec![Condition::new("album", false, Value::AnyOf { values: vec!["al-2".into()] })])], ..Own::default() }, &[]);
    let def = ask(&mut d, json!({ "ask": "crate", "id": "record" }));
    assert_eq!(def["names"]["al-2"]["name"], "Second", "{def}");
    assert_eq!(def["conditions"][0]["axis"], "album");
}

fn listed(d: &mut Device, id: &str) -> Json {
    ask(d, json!({ "ask": "crates", "now": NOW })).as_array().unwrap().iter().find(|c| c["id"] == id).unwrap_or_else(|| panic!("no {id}")).clone()
}

#[test]
fn the_crates_page_lists_what_is_listed_with_its_size_at_each_setting() {
    let own = Own { crates: vec![Kept { favourites: Favourites::None, image: true, ..crate_of("genre:Dub", crates::genre_crate("genre:Dub").unwrap().conditions) }, Kept { listed: false, ..crate_of("genre:Jazz", Vec::new()) }], favourites: vec![1], ..Own::default() };
    let mut d = device_of(own, &[]);
    let dub = listed(&mut d, "genre:Dub");
    assert_eq!((dub["counts"]["all"].as_u64(), dub["counts"]["only"].as_u64(), dub["counts"]["none"].as_u64()), (Some(4), Some(1), Some(3)));
    assert_eq!((dub["favourites"].as_str(), dub["image"].as_bool(), dub["left_out"].as_u64()), (Some("none"), Some(true), Some(0)));
    assert_eq!(dub["stats"]["tracks"], 3, "as its toggle is set");
    assert_eq!(tracks(&mut d, "genre:Dub", own_order()).len(), 3, "and it opens so");
    assert_eq!(ask(&mut d, json!({ "ask": "crates", "now": NOW })).as_array().unwrap().len(), 1, "one set something on is not listed");
}

#[test]
fn an_own_crates_toggles_are_its_conditions() {
    let conditions = crates::with_toggle(&dub(), "favourite", Favourites::Only);
    let mut d = device_of(Own { crates: vec![crate_of("mine", conditions)], favourites: vec![2, 6], ..Own::default() }, &[]);
    let mine = listed(&mut d, "mine");
    assert_eq!((mine["favourites"].as_str(), mine["counts"]["only"].as_u64(), mine["counts"]["all"].as_u64()), (Some("only"), Some(2), Some(4)));
    assert_eq!(tracks(&mut d, "mine", own_order()), [2, 6]);
}

#[test]
fn a_card_says_its_length_what_was_never_played_and_what_came_this_week() {
    let marks = [Mark { track: 1, plays: 4, ..Mark::default() }, Mark { track: 2, plays: 1, last: Some(NOW), ..Mark::default() }];
    let mut d = device_of(Own { crates: vec![crate_of("dub", dub())], ..Own::default() }, &marks);
    let card = &listed(&mut d, "dub")["stats"];
    assert_eq!((card["tracks"].as_u64(), card["seconds"].as_u64(), card["unplayed"].as_u64()), (Some(4), Some(240), Some(2)));
    assert_eq!(card["new_week"], 3, "born this week, and after the first look: tracks 1 to 3 of 6, the first look being the oldest's day after");
    assert_eq!(card["genres"], json!(["Dub"]));
    assert_eq!(card["covers"].as_array().unwrap().len(), 3);
    ask(&mut d, json!({ "ask": "plays", "plays": [{ "track": "tr-3", "at": NOW * 1000, "counts": true }] }));
    assert_eq!(listed(&mut d, "dub")["stats"]["unplayed"], 1, "a play not told yet is a play");
}

#[test]
fn a_cards_genres_go_by_their_first_spelling_and_its_covers_by_size_then_newest() {
    let mut d = device();
    for (t, born, genre) in [(1, 100, "Dub"), (2, 300, "dub"), (3, 200, "Dub"), (4, 250, "Jazz")] {
        let i = d.place[&t];
        d.facts[i].born = Some(born);
        d.facts[i].genres = vec![genre.into()];
    }
    let of = |d: &Device, t: i64| d.facts[d.place[&t]].clone();
    let lines: Vec<String> = [1, 2, 3, 4].iter().map(|t| of(&d, *t).rel).collect();
    let card = d.card(&lines, NOW);
    assert_eq!(card["genres"], json!(["Dub", "Jazz"]), "dub is Dub, as first spelled");
    let covers: Vec<&str> = card["covers"].as_array().unwrap().iter().map(|c| c["album_id"].as_str().unwrap()).collect();
    assert_eq!(covers, [of(&d, 1).album.unwrap().as_ref(), of(&d, 3).album.unwrap().as_ref()], "two tracks each: the newest first");
}

#[test]
fn a_sounds_like_lists_its_size_at_its_setting_alone() {
    let like = crates::like_crate("like:week", "Sounds like your week", vec!["tr-1".into()], 3);
    let mut d = device_of(Own { crates: vec![Kept { favourites: Favourites::All, ..crate_of("like:week", like.conditions) }], ..Own::default() }, &[]);
    let week = listed(&mut d, "like:week");
    assert_eq!((week["counts"]["all"].as_u64(), week["counts"]["none"].is_null(), week["counts"]["only"].is_null()), (Some(3), true, true));
}

fn pick(d: &mut Device, asked: Json) -> Vec<String> {
    let mut asked = asked;
    let o = asked.as_object_mut().unwrap();
    o.insert("ask".into(), json!("pick"));
    o.entry("draw").or_insert(json!(7));
    o.insert("now".into(), json!(NOW));
    ask(d, asked)["tracks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect()
}

#[test]
fn a_pick_is_a_queues_track_in_its_lane() {
    let mut d = device();
    let answer = ask(&mut d, json!({ "ask": "pick", "mode": "radio", "draw": 7, "last": "tr-1", "count": 1, "now": NOW, "exclude": ["tr-1"] }));
    let t = &answer["tracks"][0];
    assert_eq!((t["lane"].as_str(), t["source"].as_str()), (Some("radio"), Some("cratarr")));
    assert!(t["name"].is_string() && t["album_id"].is_string() && t["duration_secs"].is_number(), "{t}");
    let autoplay = ask(&mut d, json!({ "ask": "pick", "mode": "similar", "draw": 7, "last": "tr-1", "count": 1, "now": NOW }));
    assert_eq!(autoplay["tracks"][0]["lane"], "autoplay");
}

#[test]
fn a_pick_holds_only_what_plays_here() {
    let mut d = device();
    for seed in 0..20 {
        let picks = pick(&mut d, json!({ "mode": "similar", "last": "tr-1", "count": 5, "here": ["tr-3", "tr-5"], "draw": seed }));
        assert!(picks.iter().all(|p| p == "tr-3" || p == "tr-5"), "{picks:?}");
    }
    assert!(pick(&mut d, json!({ "mode": "similar", "last": "tr-1", "count": 5, "here": [] })).is_empty(), "nothing here: nothing, and no word");
}

#[test]
fn a_pick_leaves_out_what_is_excluded_by_track_and_by_artist() {
    let mut d = device_of(Own { excluded: vec![2], artists_out: vec!["artist2".into()], ..Own::default() }, &[]);
    assert_eq!(pick(&mut d, json!({ "mode": "random", "count": 5 })), ["tr-1"]);
}

#[test]
fn a_pick_leaves_favourites_out_when_asked() {
    let mut d = device_of(Own { favourites: vec![1, 2, 3, 4], ..Own::default() }, &[]);
    let mut picks = pick(&mut d, json!({ "mode": "random", "count": 5, "skip_favourites": true }));
    picks.sort();
    assert_eq!(picks, ["tr-5", "tr-6"]);
}

#[test]
fn a_pick_keeps_to_the_libraries_more_music_chose() {
    let mut d = device_of(Own { libraries: Some(vec![1]), ..Own::default() }, &[]);
    let mut picks = pick(&mut d, json!({ "mode": "random", "count": 5 }));
    picks.sort();
    assert_eq!(picks, ["tr-5", "tr-6"]);
}

#[test]
fn what_was_played_this_week_waits_while_there_is_enough_else() {
    let mut d = device_of(Own { recent: vec![(1, NOW - 100), (2, NOW - WEEK - 100)], ..Own::default() }, &[]);
    ask(&mut d, json!({ "ask": "plays", "plays": [{ "track": "tr-3", "at": (NOW - 50) * 1000 }, { "track": "tr-4", "at": (NOW - 50) * 1000, "counts": false }] }));
    for seed in 0..20 {
        let picks = pick(&mut d, json!({ "mode": "random", "count": 3, "draw": seed }));
        assert!(!picks.contains(&"tr-1".to_string()) && !picks.contains(&"tr-3".to_string()), "{picks:?}");
    }
    let mut seen: Vec<String> = (0..40).flat_map(|seed| pick(&mut d, json!({ "mode": "random", "count": 3, "draw": seed }))).collect();
    seen.sort();
    seen.dedup();
    assert_eq!(seen, ["tr-2", "tr-4", "tr-5", "tr-6"], "before the week, and what did not count, may come");
}

#[test]
fn your_taste_goes_by_the_servers_last_fits() {
    let marks: Vec<Mark> = (1..=6).map(|t| Mark { track: t, fit: Some(if t == 5 { 0.9 } else { 0.01 }), ..Mark::default() }).collect();
    let mut d = device_of(Own::default(), &marks);
    let first = (0..30).filter(|seed| pick(&mut d, json!({ "mode": "taste", "count": 1, "draw": seed })) == ["tr-5"]).count();
    assert!(first >= 25, "the one that fits: {first} of 30");
}

#[test]
fn what_is_like_the_skips_comes_less_often() {
    let marks = [Mark { track: 5, skipped: Some(1.0), ..Mark::default() }];
    let mut d = device_of(Own::default(), &marks);
    let (mut skipped, mut other) = (0, 0);
    for seed in 0..200 {
        let picks = pick(&mut d, json!({ "mode": "random", "count": 1, "draw": seed }));
        skipped += usize::from(picks == ["tr-5"]);
        other += usize::from(picks == ["tr-1"]);
    }
    assert!(skipped * 3 < other, "{skipped} against {other}");
}

#[test]
fn a_pick_stays_inside_what_it_was_told_to() {
    let list = List { id: "Mix".into(), name: "Mix".into(), tracks: vec![2, 4] };
    let mut d = device_of(Own { crates: vec![crate_of("dub", dub())], lists: vec![list], ..Own::default() }, &[]);
    let only = |d: &mut Device, within: Json| -> Vec<String> {
        let mut all: Vec<String> = (0..20).flat_map(|seed| pick(d, json!({ "mode": "random", "count": 2, "draw": seed, "within": within }))).collect();
        all.sort();
        all.dedup();
        all
    };
    assert_eq!(only(&mut d, json!({ "kind": "crate", "id": "dub" })), ["tr-1", "tr-2", "tr-3", "tr-6"]);
    assert_eq!(only(&mut d, json!({ "kind": "playlist", "id": "Mix" })), ["tr-2", "tr-4"]);
    assert_eq!(only(&mut d, json!({ "kind": "album", "source": "cratarr", "id": "al-3" })), ["tr-5", "tr-6"]);
    assert_eq!(only(&mut d, json!({ "kind": "artist", "source": "cratarr", "name": "artist 1" })), ["tr-1", "tr-2"]);
    assert_eq!(only(&mut d, json!({ "kind": "tracks", "source": "cratarr", "ids": ["tr-4"] })), ["tr-4"]);
}

#[test]
fn similar_by_goes_by_the_accounts_setting_unless_asked_otherwise() {
    // Dub by another artist, or Jazz by the same on the same record: by genre alone the first, by artist the other.
    let of_genre = |d: &mut Device, by: Json| {
        (0..30).filter(|seed| pick(d, json!({ "mode": "radio", "draw": seed, "exclude": ["tr-3"], "last": "tr-3", "count": 1, "here": ["tr-1", "tr-4"], "similar_by": by }))[0] == "tr-1").count()
    };
    let mut d = device_of(Own { similar_by: Some(vec![Signal::Genres]), ..Own::default() }, &[]);
    let as_set = of_genre(&mut d, Json::Null);
    assert!(as_set >= 20, "as the account set it: {as_set} of 30");
    let asked = of_genre(&mut d, json!(["artists"]));
    assert!(asked <= 10, "asked otherwise: {asked} of 30");
}

#[test]
fn what_cannot_be_read_says_nothing() {
    let mut d = device();
    assert_eq!(d.ask("not json"), None);
    assert_eq!(d.ask(r#"{"ask":"nothing of the kind"}"#), None);
    assert_eq!(d.ask(r#"{"ask":"rows","id":"no such crate","offset":0,"limit":5,"now":1}"#), None);
}

#[test]
fn marks_that_say_nothing_of_the_rest_are_an_account_with_nothing_set() {
    let index = Index::build(1, &rows()).unwrap();
    let mut d = Device::new(index, None, &Marks::build(NOW, &[]));
    assert_eq!(tracks(&mut d, "genre:Dub", own_order()), [1, 2, 3, 6]);
}

#[test]
fn one_copy_of_a_sound_serves_the_crates_and_radio() {
    let mut d = device();
    tracks(&mut d, "like:tr-1", own_order());
    pick(&mut d, json!({ "mode": "similar", "last": "tr-1", "count": 1 }));
    let sound = d.facts[0].sound.clone().unwrap();
    assert_eq!(Arc::strong_count(&sound), 4, "here, the facts, and the two libraries: nothing else holds it");
}

/// What a device answers in at the real library's size, on this PC, in an unoptimised test build: read the times for how much there is
/// to do, as the tablet's are D22's.
#[test]
#[ignore = "a measure, read in a release build: --ignored --nocapture --test-threads=1"]
fn measured_device_answers_at_34_000_tracks() {
    use std::time::{Duration, Instant};
    let (rows, heard) = crate::index::made_up(34_000, 1);
    let index = Arc::new(Index::build(1, &rows).unwrap());
    let sounds = Sounds::build(1, &heard);
    let tracks: Vec<i64> = rows.tracks.iter().map(|t| t.id).collect();
    let marks: Vec<Mark> = tracks.iter().enumerate().map(|(n, t)| Mark { track: *t, fit: Some(n as f64 / 40_000.0), skipped: Some(n as f64 / 90_000.0), plays: (n % 7) as u32, last: Some(NOW - n as i64), ..Mark::default() }).collect();
    // Heard, and apart across the library, as a playlist's are: a round with one unheard among them makes one pass of a Sounds like.
    let heard_from = |count: usize| -> Vec<String> { (0..count).map(|k| ids::song(heard[k * (heard.len() / count)].0)).collect() };
    let seeds = heard_from(12);
    let two_conditions = vec![Condition::new("year", false, Value::Range { min: Some(1990.0), max: Some(2010.0) }), Condition::new("energy", false, Value::Range { min: Some(30.0), max: None })];
    let own = Own {
        crates: vec![crate_of("big", two_conditions), crate_of("like:week", like_crate("like:week", "Sounds like your week", seeds.clone(), 200).conditions)],
        favourites: tracks.iter().step_by(11).copied().collect(),
        recent: tracks.iter().step_by(170).map(|t| (*t, NOW - 3_600)).collect(),
        ..Own::default()
    };
    let bytes = Marks::build(NOW, &marks).with_own(serde_json::to_string(&own).unwrap()).save();
    let marks = Marks::load(&bytes).unwrap();
    // As the app asks (DeviceAnswers.ask): the plays not told yet handed in before every ask, and the first ask of its kind on a
    // device just opened making what that kind reads (the similarity, Radio's index). The app's log calls that ask warm.
    let unsent: Vec<Json> = tracks.iter().step_by(700).map(|t| json!({ "track": ids::song(*t), "at": NOW * 1000, "counts": true })).collect();
    let plays = json!({ "ask": "plays", "plays": unsent }).to_string();
    let first_pick = json!({ "ask": "pick", "mode": "radio", "draw": 7, "seed": [seeds[1]], "exclude": [seeds[1]], "last": seeds[1], "count": 5, "now": NOW });
    for (what, asked) in [("the Crates page", json!({ "ask": "crates", "now": NOW })), ("Pick: radio", first_pick)] {
        let asked = asked.to_string();
        let at = Instant::now();
        let mut d = Device::new(index.clone(), Some(&sounds), &marks);
        let opened = at.elapsed();
        let at = Instant::now();
        d.ask(&plays).unwrap();
        let told = at.elapsed();
        let at = Instant::now();
        d.ask(&asked).unwrap();
        let first = at.elapsed();
        let at = Instant::now();
        d.ask(&plays).unwrap();
        d.ask(&asked).unwrap();
        println!("As the app asks, {what}: opened {opened:?}, plays told {told:?}, first ask {first:?}, then plays and ask {:?}", at.elapsed());
    }
    let at = Instant::now();
    let mut d = Device::new(index, Some(&sounds), &marks);
    println!("opened {:?}, own {} bytes, own's text {} bytes", at.elapsed(), bytes.len(), marks.own().len());
    let once = |d: &mut Device, asked: &str| {
        let at = Instant::now();
        let answer = d.ask(asked).unwrap();
        (at.elapsed(), answer.len())
    };
    let mut timed = |what: &str, asked: Json| {
        let asked = asked.to_string();
        let (first, size) = once(&mut d, &asked);
        let mut took: Vec<Duration> = (0..9).map(|_| once(&mut d, &asked).0).collect();
        took.sort();
        println!("{what}: first {first:?}, median {:?}, worst {:?}, {size} bytes", took[4], took[8]);
    };
    timed("Crate, 2 conditions, Best match", json!({ "ask": "rows", "id": "big", "offset": 0, "limit": 100, "now": NOW }));
    timed("Crate, 2 conditions, own order", json!({ "ask": "rows", "id": "big", "order": "own", "offset": 0, "limit": 100, "now": NOW }));
    timed("Sounds like, 12 seeds", json!({ "ask": "rows", "id": "like:week", "offset": 0, "limit": 100, "now": NOW }));
    timed("Sounds like, 1 seed", json!({ "ask": "rows", "id": format!("like:{}@200", seeds[0]), "offset": 0, "limit": 100, "now": NOW }));
    timed("The Crates page", json!({ "ask": "crates", "now": NOW }));
    let here = heard_from(2_000);
    for mode in ["similar", "radio", "artist", "era", "random", "taste"] {
        timed(&format!("Pick: {mode}"), json!({ "ask": "pick", "mode": mode, "draw": 7, "seed": [seeds[1]], "exclude": [seeds[1]], "last": seeds[1], "count": 5, "now": NOW }));
    }
    let queue = heard_from(500);
    timed("Pick: similar, full queue", json!({ "ask": "pick", "mode": "similar", "draw": 7, "seed": queue, "exclude": queue, "last": queue[0], "count": 5, "now": NOW }));
    for mode in ["similar", "radio", "random"] {
        timed(&format!("Pick: {mode}, among 2,000 here"), json!({ "ask": "pick", "mode": mode, "draw": 7, "seed": [seeds[1]], "exclude": [seeds[1]], "last": seeds[1], "count": 5, "here": here, "now": NOW }));
    }
}
