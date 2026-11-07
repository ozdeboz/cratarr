//! The proof that moving the rules changes no answer (decision 0101 point 27): every crate's tracks and Radio's picks of
//! one data folder, for every account, written as sorted lines, so that `cmp` says whether two runs agree and nothing
//! needs reading. It stays out of the shipped binary: the "before" and the "after" are each build's test binary, kept,
//! each run on its own copy of one data folder (it is opened as a server opens it, so never the folder itself):
//!
//! `PROOF_DATA=<a copy> PROOF_NOW=<epoch s> PROOF_SEED=<n> PROOF_OUT=<file> <test binary> --ignored --exact
//! app::radio::proof::answers_of_a_data_folder`, and `PROOF_TRACKS=<n>` for how many tracks seed Radio, spread over the
//! library (300 unless said; a test binary is slow on a whole library).
//!
//! `now` and the seed come from outside and every case takes a fresh generator, so one change stays one change. No pass
//! runs: the catalogue is read as it is, and both runs read the same files. Each account runs as itself, as a request
//! does (`who`). Home's moment is pinned with the clock: this process has heard no weather. Bandcamp's suggestions are
//! not asked: they are the web's.
//!
//! `the_same_seed_gives_the_same_answer_end_to_end` is the proof on the fixture library, in the gate.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::Ordering;

use cratarr_rules::index::Marks;
use cratarr_rules::index::device::Device;
use cratarr_rules::radio::DEFAULT_COUNT;

use super::*;
use crate::app::App;
use crate::catalogue::AudioMuseRow;
use crate::crates::{self, Condition, Value};
use crate::serve::Serve;
use crate::serve::who::{Scope, Who, acting_as};
use crate::source::cratarr::KIND;
use crate::userdata::{Ended, Heard, Kind};

/// Radio's every way of asking: each mode, those from before the amendment too, and its inserts by all three.
const ASKS: [(Mode, RadioBy); 11] = [
    (Mode::Similar, RadioBy::Track),
    (Mode::Artist, RadioBy::Track),
    (Mode::Era, RadioBy::Track),
    (Mode::Random, RadioBy::Track),
    (Mode::Taste, RadioBy::Track),
    (Mode::Continuous, RadioBy::Track),
    (Mode::AlbumMix, RadioBy::Track),
    (Mode::Reshuffle, RadioBy::Track),
    (Mode::Radio, RadioBy::Track),
    (Mode::Radio, RadioBy::Artist),
    (Mode::Radio, RadioBy::Era),
];

/// A data folder opened as `main` opens it, without the passes that read the files and the web.
async fn open(data: &Path) -> (Arc<App>, Arc<Serve>) {
    let dir = data.to_string_lossy().into_owned();
    let env = |k: &str| (k == "CRATARR_DATA_DIR").then(|| dir.clone());
    let cfg = crate::config::Config::from_lookup(env).unwrap();
    let saved = crate::settings::load(&data.join("settings.json"), &env).unwrap().expect("a data folder has its settings");
    let served = Serve::open(Some(data), saved.tag_rules.clone(), &env).unwrap();
    crate::serve::set_instance(served.clone());
    let app = App::new(cfg, saved, crate::settings::Origin::File, Some(served.clone())).await.unwrap();
    served.attach_catalogue(app.library.catalogue());
    served.set_playlists(app.playlists.clone());
    (app, served)
}

/// The library built from the catalogue, where it is not yet: a build of another layout starts with an empty one.
async fn built(served: &Arc<Serve>) {
    let s = served.clone();
    tokio::task::spawn_blocking(move || s.refresh()).await.unwrap().unwrap();
}

fn who(served: &Serve, account: &str) -> Who {
    Who { user: account.to_string(), admin: served.users.get(account).is_some_and(|u| u.admin), scope: Scope::Full, home: true }
}

fn crate_within(id: &str) -> super::super::play::Collection {
    Within::Crate { id: id.to_string(), without: Vec::new() }.collection()
}

/// Every crate the account sees, in two lines: `crate`, as `crate_members` holds it (search's answer: permanent, Home's and
/// its own, with what it left out and excluded), and `plays`, as it plays. Only the second works out what asks for the
/// taste (Second chance). Then, as it plays, what is made when asked for: the genres, the moment, and the Sounds likes
/// it keeps a fit or tracks left out of (gap Q41: what is listed or saved, plus `genre:*`). The lines, and the ids of the
/// crates it sees.
async fn crates_of(app: &Arc<App>, account: &str) -> (Vec<String>, Vec<String>) {
    let cat = app.library.catalogue();
    let (mut out, mut ids) = (Vec::new(), Vec::new());
    for (id, _, lines) in app.playlists.crate_members(&[]).await.unwrap() {
        let tracks: Vec<String> = cat.line_track_ids(KIND.id, &lines).unwrap().into_iter().zip(lines).map(|(track, line)| track.unwrap_or(line)).collect();
        out.push(format!("{account}\tcrate\t{id}\t{}", tracks.join(" ")));
        ids.push(id);
    }
    let mut made: BTreeSet<String> = cat.crate_facts().unwrap().iter().flat_map(|f| f.genres.iter()).map(|g| format!("{}{g}", crates::GENRE)).collect();
    made.insert(format!("{}{}", crates::LIKE, crates::NOW));
    made.extend(app.playlists.crate_fits().await.unwrap().into_iter().map(|f| f.crate_id));
    made.extend(ids.iter().cloned());
    for id in made {
        out.push(match super::super::play::own_songs(app, &crate_within(&id), Some(account)).await {
            Ok(tracks) => format!("{account}\tplays\t{id}\t{}", tracks.join(" ")),
            // A crate that cannot be made now is an answer too, and must stay one.
            Err(e) => format!("{account}\tplays\t{id}\tnot made: {e:?}"),
        });
    }
    (out, ids)
}

/// What one account's Radio is asked with.
struct Asking<'a> {
    account: &'a str,
    /// The index as `next` makes it for this account, and with its taste on for Your taste.
    plain: &'a Index,
    tasted: &'a Index,
    recent: &'a HashMap<String, i64>,
    /// The request as its settings make it.
    asked: Ask,
    seed: u64,
}

impl Asking<'_> {
    /// One line for each way of asking after `track`, Autoplay seeded by `seed`, Radio's inserts by the track alone.
    fn lines(&self, inside: &str, within: Option<&[usize]>, seed: &[String], track: &str) -> Vec<String> {
        let mut out = Vec::new();
        for (mode, by) in ASKS {
            for (albums, skip_favourites) in [(false, false), (false, true), (true, false), (true, true)] {
                let seed = if mode == Mode::Radio { vec![track.to_string()] } else { seed.to_vec() };
                let req = Ask { mode, by, albums, skip_favourites, exclude: seed.clone(), seed, last: Some(track.to_string()), count: Some(DEFAULT_COUNT), ..self.asked.clone() };
                let index = if mode == Mode::Taste { self.tasted } else { self.plain };
                let picks = choose(index, &req, self.recent, within, &mut Rng::seeded(self.seed));
                let ids: Vec<&str> = picks.iter().map(|&i| index.songs[i].id.as_str()).collect();
                let (albums, favourites) = (if albums { "albums" } else { "tracks" }, if skip_favourites { "no-favourites" } else { "favourites" });
                out.push(format!("{}\tradio\t{mode:?}\t{by:?}\t{albums}\t{favourites}\t{inside}\t{track}\t{}", self.account, ids.join(" ")));
            }
        }
        out
    }
}

/// Radio's picks for the account: through the real index and `ready`, after a spread of `tracks` tracks of the library,
/// each with its album as Autoplay's seed, and inside each crate of `crates`, after its first track.
async fn picks_of(app: &Arc<App>, account: &str, crates: &[String], seed: u64, tracks: usize) -> Vec<String> {
    let (index, recent) = ready().await.unwrap();
    let asked = match as_asked(NextRequest::default()) {
        Ok((req, _, _)) => req.ask,
        Err(e) => return vec![format!("{account}\tradio\trefused: {e:?}")],
    };
    let plain = with_taste(app, index.clone(), Some(account), false).await.unwrap();
    let tasted = with_taste(app, index, Some(account), true).await.unwrap();
    let asking = Asking { account, plain: &plain, tasted: &tasted, recent: &recent, asked, seed };
    let mut out = Vec::new();
    let step = (plain.songs.len() / tracks.max(1)).max(1);
    for song in plain.songs.iter().step_by(step).take(tracks) {
        let album: Vec<String> = plain.albums[&song.album_n].iter().map(|&i| plain.songs[i].id.clone()).collect();
        out.extend(asking.lines("-", None, &album, &song.id));
    }
    for id in crates {
        let Ok(songs) = super::super::play::own_songs(app, &crate_within(id), Some(account)).await else { continue };
        let places = plain.find(&songs);
        let Some(&first) = places.first() else { continue };
        let track = plain.songs[first].id.clone();
        out.extend(asking.lines(id, Some(&places), std::slice::from_ref(&track), &track));
    }
    out
}

/// What every account gets of this data folder, sorted.
async fn answers(app: &Arc<App>, served: &Arc<Serve>, seed: u64, tracks: usize) -> Vec<String> {
    let mut out = Vec::new();
    for account in served.users.names() {
        let (crates, ids) = acting_as(who(served, &account), crates_of(app, &account)).await;
        out.extend(crates);
        out.extend(acting_as(who(served, &account), picks_of(app, &account, &ids, seed, tracks)).await);
    }
    out.sort();
    out
}

fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name}: see this file's top"))
}

/// The run itself: see this file's top.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a proof's run on a copy of a data folder: see this file's top"]
async fn answers_of_a_data_folder() {
    let data = PathBuf::from(var("PROOF_DATA"));
    crate::app::moment::PINNED.store(var("PROOF_NOW").parse().expect("PROOF_NOW: epoch seconds"), Ordering::Relaxed);
    // What the last run heard of the favourites is only a head start, and asked again in the background: an answer that
    // lands whenever it lands. Gone, the first asking waits for it.
    std::fs::remove_file(data.join("favourites.json")).ok();
    let (app, served) = open(&data).await;
    built(&served).await;
    let tracks = std::env::var("PROOF_TRACKS").map_or(300, |n| n.parse().expect("PROOF_TRACKS: how many"));
    let lines = answers(&app, &served, var("PROOF_SEED").parse().expect("PROOF_SEED: a number"), tracks).await;
    assert!(!lines.is_empty(), "no account here: an empty file would prove nothing");
    std::fs::write(var("PROOF_OUT"), lines.join("\n") + "\n").unwrap();
}

/// What AudioMuse would say of a track, made up and the same every time: every signal it gives, close enough together
/// for the sound to count (`similarity::SOUND_LO`).
fn heard_of(track: i64) -> AudioMuseRow {
    let mut r = Rng::seeded(track as u64);
    let styles = ["ambient", "dub techno", "drone", "post-rock", "downtempo", "idm", "minimal", "field recording"];
    let style = format!("{}:{:.3},{}:{:.3}", styles[r.below(styles.len())], 0.5 + 0.4 * r.unit(), styles[r.below(styles.len())], 0.1 + 0.4 * r.unit());
    let moods = ["happy", "sad", "relaxed", "aggressive", "party", "danceable"].map(|m| format!("{m}:{:.3}", 0.45 + 0.25 * r.unit())).join(",");
    let embedding = (0..16).flat_map(|_| (0.5 + r.unit() as f32).to_le_bytes()).collect();
    AudioMuseRow { track, energy: Some(r.unit()), style: Some(style), moods: Some(moods), embedding: Some(embedding), ..AudioMuseRow::default() }
}

/// An account's listening on the fixture library, a turn apart from the other's: favourites, plays in the week and
/// before it, skips, a playlist, crates of its own, what it left out and excluded.
async fn listens(app: &Arc<App>, served: &Serve, account: &str, turn: usize, songs: &[String], albums: &HashMap<String, String>, now: i64) {
    let every = |n: usize| -> Vec<String> { songs.iter().enumerate().filter(|(i, _)| i % n == turn).map(|(_, s)| s.clone()).collect() };
    let ud = &served.userdata;
    ud.set_starred(account, Kind::Song, &every(7), true).unwrap();
    let album = albums[&songs[turn * 20]].clone();
    ud.set_starred(account, Kind::Album, std::slice::from_ref(&album), true).unwrap();
    // Enough of the week's for a sounds' grouping (`crates::GROUP_FROM`), and some from before it; the library's last
    // albums not at all, or nothing is new for anyone.
    let heard = every(2);
    for (k, song) in heard.iter().take(15).enumerate() {
        ud.count_play(account, (song, song), &albums[song], &[], now - 3600 * 8 * (k as i64 + 1)).unwrap();
    }
    for (k, song) in heard.iter().take(15).step_by(4).enumerate() {
        ud.count_play(account, (song, song), &albums[song], &[], now - 86_400 * (30 + k as i64)).unwrap();
    }
    for (k, song) in every(5).iter().enumerate() {
        for n in 0..3 {
            let skipped = Heard { ended: Ended::Skipped, counts: false, ..Heard::counted(now - 3600 * (3 * k as i64 + n + 1)) };
            ud.record(account, (song, song), &albums[song], &[], &skipped).unwrap();
        }
    }
    let cat = app.library.catalogue();
    let list = ["Mix", "Road"][turn];
    let lines: Vec<String> = every(4).iter().filter_map(|s| crate::serve::playlists::line_of_song(&cat, s)).collect();
    app.playlists.create_with(list, &lines).await.unwrap();
    let calm = vec![Condition::new("energy", false, Value::Range { min: None, max: Some(60.0) })];
    let (own, _) = app.playlists.crate_save(None, "Calm", calm).await.unwrap();
    let like = Value::Like { of: vec![album.clone()], count: 20, min: None, own: None, mode: None, fresh: false, weights: Vec::new(), unlisted: false, avoid: crates::Avoid::default() };
    app.playlists.crate_save(None, "Like a record", vec![Condition::new("like", false, like)]).await.unwrap();
    app.playlists.crate_leave_out(&own, None, &every(13), &[], true).await.unwrap();
    // Turned down for a Sounds like: what it learns to avoid. Kept for a playlist's: `crate_fits` lists both.
    app.playlists.crate_leave_out(&format!("{}{album}", crates::LIKE), None, &every(9), &[], true).await.unwrap();
    let dismissed = crates::Fit { dismissed: vec!["tempo".to_string()], ..crates::Fit::default() };
    app.playlists.set_crate_fit(&format!("{}{}{list}", crates::LIKE, crates::LIST), dismissed).await.unwrap();
    app.playlists.exclude(None, &every(17), true).await.unwrap();
    // A fixed crate's toggles as saved, for every device.
    app.playlists.set_crate_favourites("unlisted", [crates::Favourites::Only, crates::Favourites::None][turn]).await.unwrap();
    app.playlists.set_crate_favourite_albums("unlisted", [crates::Favourites::All, crates::Favourites::Only][turn]).await.unwrap();
    if turn == 1 {
        app.playlists.exclude_artist("Kite Theory").await.unwrap();
    }
}

/// A data folder of the fixture library (`PROOF_MAKE/music`, `PROOF_MAKE/data`): read and built as a first start reads it,
/// with two accounts that have listened up to `PROOF_NOW`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "made for `the_same_seed_gives_the_same_answer_end_to_end`, in a process of its own"]
async fn a_data_folder_of_the_fixture_library() {
    let root = PathBuf::from(var("PROOF_MAKE"));
    let now: i64 = var("PROOF_NOW").parse().unwrap();
    let (music, data) = (root.join("music"), root.join("data"));
    crate::fixtures::make(&music).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    let folder = music.to_string_lossy();
    let settings = serde_json::json!({
        "version": 4,
        "music_folder": folder,
        "sources": [{ "id": KIND.id, "kind": KIND.id, "enabled": true, "libraries": [{ "id": folder, "kind": "main" }], "values": { "folders": [folder] } }],
    });
    std::fs::write(data.join("settings.json"), serde_json::to_vec_pretty(&settings).unwrap()).unwrap();
    let (app, served) = open(&data).await;
    let cat = app.library.catalogue();
    let c = cat.clone();
    tokio::task::spawn_blocking(move || {
        assert_eq!(c.walk_all(), Vec::<String>::new());
        while c.read_tags_batch(100).unwrap() > 0 {}
        c.identify().unwrap();
    })
    .await
    .unwrap();
    built(&served).await;
    for (id, source) in app.library.connected_sources() {
        crate::catalogue::link_source(&cat, &id, &source).await.unwrap();
    }
    let songs = served.library.all_song_ids().unwrap();
    let albums = served.library.albums_of(&songs).unwrap();
    assert_eq!(songs.len(), 51, "the fixture library, built");
    // Not everything was heard: what is not known must not count.
    let heard: Vec<AudioMuseRow> = songs.iter().enumerate().filter(|(i, _)| i % 5 != 4).filter_map(|(_, s)| crate::serve::ids::track_of(s)).map(heard_of).collect();
    cat.replace_audiomuse(&heard).unwrap();
    for (turn, account) in ["ann", "bob"].into_iter().enumerate() {
        served.users.set(account, "pw").unwrap();
        acting_as(who(&served, account), listens(&app, &served, account, turn, &songs, &albums, now)).await;
    }
}

/// A device that fetched all it fetches of this account, the sounds as the server holds them: eight bits move a Sounds like a little
/// (D11 says how far), and here the rules are what is compared.
async fn device_of(app: &Arc<App>, served: &Arc<Serve>, account: &str) -> Device {
    let libraries = app.library.index_libraries(served);
    let (s, a) = (served.clone(), account.to_string());
    let (index, facts) = tokio::task::spawn_blocking(move || crate::app::device_index::whole_of(&s, &libraries, Some(&a))).await.unwrap().unwrap();
    let own = crate::app::device_index::own(app, Some(account.to_string()), false).await.unwrap();
    let marks = Marks::load(&own.bytes).unwrap();
    let heard: HashMap<i64, Arc<[f32]>> = facts.iter().filter_map(|f| Some((f.track?, f.sound.clone()?))).collect();
    let mut held = index.facts(None);
    held.iter_mut().for_each(|f| f.sound = f.track.and_then(|t| heard.get(&t).cloned()));
    Device::over(index, held, &marks)
}

/// The tracks a device says a crate holds, in its own order; none where it has nothing to say.
fn device_crate(device: &mut Device, id: &str, now: i64) -> Option<Vec<String>> {
    let asked = serde_json::json!({ "ask": "rows", "id": id, "order": "own", "offset": 0, "limit": 100_000, "now": now });
    let answer: serde_json::Value = serde_json::from_str(&device.ask(&asked.to_string())?).unwrap();
    Some(answer["rows"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap().to_string()).collect())
}

/// Every crate of `crates_of`'s and a Sounds like of a spread of tracks, records and artists, on a device and on the server. A moment
/// is the weather's, and says nothing on a device. How many held tracks, and what differs, one line each.
async fn crates_against(app: &Arc<App>, device: &mut Device, account: &str, now: i64) -> (usize, Vec<String>) {
    let cat = app.library.catalogue();
    let facts = cat.crate_facts().unwrap();
    let mut ids: BTreeSet<String> = app.playlists.crate_members(&[]).await.unwrap().into_iter().map(|(id, _, _)| id).collect();
    ids.extend(facts.iter().flat_map(|f| f.genres.iter()).map(|g| format!("{}{g}", crates::GENRE)));
    ids.extend(app.playlists.crate_fits().await.unwrap().into_iter().map(|f| f.crate_id));
    ids.extend(facts.iter().step_by(7).filter_map(|f| Some(format!("{}{}", crates::LIKE, f.song()?))));
    ids.extend(facts.iter().step_by(11).filter_map(|f| Some(format!("{}{}", crates::LIKE, f.album.as_deref()?))));
    ids.insert(crates::SECOND_CHANCE.to_string());
    let (mut held, mut out) = (0, Vec::new());
    for id in ids {
        let server = super::super::play::own_songs(app, &crate_within(&id), Some(account)).await.ok();
        let here = device_crate(device, &id, now);
        held += usize::from(server.as_ref().is_some_and(|tracks| !tracks.is_empty()));
        if server != here {
            out.push(format!("{account}: {id}: {server:?} on the server, {here:?} on the device"));
        }
    }
    (held, out)
}

/// The Crates page on a device and on the server: the list, and each crate's card, sizes and toggles. A Sounds like's sizes at the
/// settings it is not set to are the server's alone (one pass over the library is a device's): where a device has none, they are not
/// compared. How many crates, and what differs, one line each.
async fn list_against(app: &Arc<App>, device: &mut Device, account: &str, now: i64) -> (usize, Vec<String>) {
    let server = serde_json::to_value(app.playlists.crates(None, &[]).await.unwrap()).unwrap();
    let asked = serde_json::json!({ "ask": "crates", "without": [], "now": now });
    let here: serde_json::Value = serde_json::from_str(&device.ask(&asked.to_string()).unwrap()).unwrap();
    let (server, mut here) = (server.as_array().unwrap().clone(), here.as_array().unwrap().clone());
    let ids = |list: &[serde_json::Value]| -> Vec<String> { list.iter().map(|c| c["id"].as_str().unwrap_or_default().to_string()).collect() };
    let mut out = Vec::new();
    if ids(&server) != ids(&here) {
        out.push(format!("{account}: the list: {:?} on the server, {:?} on the device", ids(&server), ids(&here)));
    }
    for (s, h) in server.iter().zip(here.iter_mut()) {
        for setting in ["all", "only", "none"] {
            if h["counts"][setting].is_null() {
                h["counts"][setting] = s["counts"][setting].clone();
            }
        }
        if s != h {
            out.push(format!("{account}: {}: {s} on the server, {h} on the device", s["id"]));
        }
    }
    (server.len(), out)
}

/// `Asking::lines`' every way of asking after a spread of tracks, and inside each crate, on a device and with the server's index. How
/// many picked anything, and what differs, one line each.
async fn picks_against(app: &Arc<App>, device: &mut Device, account: &str, seed: u64, now: i64) -> (usize, Vec<String>) {
    let (index, recent) = ready().await.unwrap();
    let asked = as_asked(NextRequest::default()).unwrap().0.ask;
    let plain = with_taste(app, index.clone(), Some(account), false).await.unwrap();
    let tasted = with_taste(app, index, Some(account), true).await.unwrap();
    let (mut picked, mut out) = (0, Vec::new());
    let mut against = |inside: Option<(&str, &[usize])>, seed_tracks: &[String], track: &str| {
        for (mode, by) in ASKS {
            for (albums, skip_favourites) in [(false, false), (false, true), (true, false), (true, true)] {
                let of = if mode == Mode::Radio { vec![track.to_string()] } else { seed_tracks.to_vec() };
                let req = Ask { mode, by, albums, skip_favourites, exclude: of.clone(), seed: of, last: Some(track.to_string()), count: Some(DEFAULT_COUNT), ..asked.clone() };
                let index = if mode == Mode::Taste { &tasted } else { &plain };
                let server: Vec<String> = choose(index, &req, &recent, inside.map(|i| i.1), &mut Rng::seeded(seed)).iter().map(|&i| index.songs[i].id.clone()).collect();
                let mut ask = serde_json::to_value(&req).unwrap();
                let fields = ask.as_object_mut().unwrap();
                fields.insert("ask".into(), "pick".into());
                fields.insert("draw".into(), seed.into());
                fields.insert("now".into(), now.into());
                if let Some((id, _)) = inside {
                    fields.insert("within".into(), serde_json::json!({ "kind": "crate", "id": id }));
                }
                let answer: serde_json::Value = serde_json::from_str(&device.ask(&ask.to_string()).unwrap()).unwrap();
                let here: Vec<String> = answer["tracks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect();
                picked += usize::from(!server.is_empty());
                if server != here {
                    out.push(format!("{account}: {mode:?} {by:?} albums {albums} no favourites {skip_favourites} inside {:?} after {track}: {server:?} on the server, {here:?} on the device", inside.map(|i| i.0)));
                }
            }
        }
    };
    let step = (plain.songs.len() / 12).max(1);
    for song in plain.songs.iter().step_by(step) {
        let album: Vec<String> = plain.albums[&song.album_n].iter().map(|&i| plain.songs[i].id.clone()).collect();
        against(None, &album, &song.id);
    }
    for (id, _, _) in app.playlists.crate_members(&[]).await.unwrap() {
        let Ok(songs) = super::super::play::own_songs(app, &crate_within(&id), Some(account)).await else { continue };
        let places = plain.find(&songs);
        let Some(&first) = places.first() else { continue };
        let track = plain.songs[first].id.clone();
        against(Some((&id, &places)), std::slice::from_ref(&track), &track);
    }
    (picked, out)
}

/// What differs between a device and the server, by account: see `a_device_answers_as_the_server_does_end_to_end`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "run by `a_device_answers_as_the_server_does_end_to_end`, in a process of its own"]
async fn a_device_against_a_data_folder() {
    let data = PathBuf::from(var("PROOF_DATA"));
    let now: i64 = var("PROOF_NOW").parse().expect("PROOF_NOW: epoch seconds");
    crate::app::moment::PINNED.store(now, Ordering::Relaxed);
    let (app, served) = open(&data).await;
    built(&served).await;
    let mut differing = Vec::new();
    for account in served.users.names() {
        let ((listed, list), (held, crates), (picked, picks)) = acting_as(who(&served, &account), async {
            let mut device = device_of(&app, &served, &account).await;
            let list = list_against(&app, &mut device, &account, now).await;
            (list, crates_against(&app, &mut device, &account, now).await, picks_against(&app, &mut device, &account, 7, now).await)
        })
        .await;
        println!("{account}: {listed} crates listed, {held} crates held tracks, {picked} asks picked some");
        assert!(listed >= 4 && held >= 20 && picked >= 500, "too little compared to prove anything: {listed} listed, {held} crates, {picked} picks");
        differing.extend(list);
        differing.extend(crates);
        differing.extend(picks);
    }
    assert!(differing.is_empty(), "{} differ:\n{}", differing.len(), differing.iter().take(30).map(|d| d.chars().take(500).collect::<String>()).collect::<Vec<_>>().join("\n"));
}

/// D18 to D20's proof on the fixture (PREVIEW2: same crate, same answer; same seed, same picks on the same data): two accounts that
/// listened, with crates, a playlist, what they left out and excluded; everything a device fetches of each; every crate and a spread of
/// Sounds likes as the server plays them, and Radio in every mode after a spread of tracks and inside each crate.
#[test]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
fn a_device_answers_as_the_server_does_end_to_end() {
    let root = std::env::temp_dir().join(format!("cratarr-proof-device-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    let now: i64 = 1_800_000_000;
    alone("a_data_folder_of_the_fixture_library", &[("PROOF_MAKE", root.display().to_string()), ("PROOF_NOW", now.to_string())]);
    alone("a_device_against_a_data_folder", &[("PROOF_DATA", root.join("data").display().to_string()), ("PROOF_NOW", now.to_string())]);
    std::fs::remove_dir_all(&root).ok();
}

fn copy_folder(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (from, to) = (e.path(), to.join(e.file_name()));
        if from.is_dir() {
            copy_folder(&from, &to);
        } else {
            std::fs::copy(&from, &to).unwrap();
        }
    }
}

/// One of this file's tests, in a process of its own: what comes of a process' own start must show, and a server's
/// once-only state (`serve::set_instance`) is one to a process.
fn alone(test: &str, vars: &[(&str, String)]) {
    let name = format!("{}::{test}", module_path!().split_once("::").unwrap().1);
    let ran = Command::new(std::env::current_exe().unwrap()).args(["--ignored", "--exact", &name]).envs(vars.iter().map(|(k, v)| (*k, v))).status().unwrap();
    assert!(ran.success(), "{name}");
}

/// The proof on the fixture: a run of a build compared with itself shows the noise is gone, in processes of their own, on
/// copies of one data folder. Another seed and another day must change the answers: the two are really what decides.
#[test]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
fn the_same_seed_gives_the_same_answer_end_to_end() {
    let root = std::env::temp_dir().join(format!("cratarr-proof-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    let now: i64 = 1_800_000_000;
    alone("a_data_folder_of_the_fixture_library", &[("PROOF_MAKE", root.display().to_string()), ("PROOF_NOW", now.to_string())]);
    let answers = |copy: &str, seed: u64, now: i64| -> String {
        let (data, out) = (root.join(copy), root.join(format!("{copy}.txt")));
        copy_folder(&root.join("data"), &data);
        alone(
            "answers_of_a_data_folder",
            &[("PROOF_DATA", data.display().to_string()), ("PROOF_NOW", now.to_string()), ("PROOF_SEED", seed.to_string()), ("PROOF_OUT", out.display().to_string())],
        );
        std::fs::read_to_string(out).unwrap()
    };
    let (one, two) = (answers("one", 7, now), answers("two", 7, now));
    let differing: Vec<(&str, &str)> = one.lines().zip(two.lines()).filter(|(a, b)| a != b).collect();
    assert_eq!(one.lines().count(), two.lines().count());
    assert!(differing.is_empty(), "{} of {} lines differ, first {:?}", differing.len(), one.lines().count(), differing[0]);

    // Every account, every way of asking and every kind of crate is in it, and holds something.
    let of = |account: &str, kind: &str, what: &str| -> Vec<String> {
        let start = format!("{account}\t{kind}\t{what}");
        one.lines().filter(|l| l.starts_with(&start)).map(str::to_string).collect()
    };
    let tracks_of = |line: &str| line.rsplit('\t').next().unwrap_or("").split_whitespace().count();
    for account in ["ann", "bob"] {
        for (mode, by) in ASKS {
            let lines = of(account, "radio", &format!("{mode:?}\t{by:?}\t"));
            assert!(lines.iter().any(|l| tracks_of(l) > 0), "{account} {mode:?} {by:?}: no picks");
            for albums in ["tracks", "albums"] {
                assert!(lines.iter().any(|l| l.contains(&format!("\t{albums}\t"))), "{account} {mode:?}: {albums}");
            }
        }
        assert!(one.lines().any(|l| l.starts_with(&format!("{account}\tradio\tSimilar\tTrack\ttracks\tfavourites\tunlisted\t"))), "{account}: inside a crate");
        for id in ["unlisted", "second-chance", "like:week", "home:new", "home:graves", "like:now", "genre:Ambient"] {
            let line = of(account, "plays", &format!("{id}\t"));
            assert!(line.len() == 1 && tracks_of(&line[0]) > 0 && !line[0].contains("not made"), "{account} {id}: {line:?}");
        }
        for id in ["unlisted", "like:week", "home:new", "home:graves"] {
            assert!(of(account, "crate", &format!("{id}\t")).iter().any(|l| tracks_of(l) > 0), "{account} {id}: as search holds it");
        }
        assert!(!of(account, "plays", "like:al-").is_empty() && !of(account, "plays", "like:list:").is_empty(), "{account}: its kept Sounds likes");
    }

    let other_seed = answers("seed", 8, now);
    let (crates, picks): (Vec<_>, Vec<_>) = one.lines().zip(other_seed.lines()).partition(|(a, _)| !a.contains("\tradio\t"));
    assert!(crates.iter().all(|(a, b)| a == b), "a crate draws nothing");
    assert!(picks.iter().any(|(a, b)| a != b), "the seed decides Radio's picks");
    assert_ne!(one, answers("later", 7, now + 30 * 86_400), "the day decides what counts as lately");
    std::fs::remove_dir_all(&root).ok();
}
