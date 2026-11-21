//! The record's view and its route (SCENARIOS-DC1 RV1-RV4, RV10, LV4, TY1-TY3, TY5, CN7-CN9; decision 0104 point 5):
//! `GET /api/records/{id}?from=<place>&without=<source>/<library>…&opener=<place>`, one answer for a record wherever it shows,
//! for the viewer asking and the place it opens from. The record is read once (app's `core::records`) and worked out by the
//! domain (`cratarr_domain::record`): its state, the actions `decide()` allows with their addresses, the when-table's sections.
//!
//! - **A merged record answers as the one it went into** (RV3); one never made or forgotten is a 404 with the words table's
//!   words (RV4).
//! - **The place** is the `from` word (RS1's table), the libraries switched off its opener's `without`, as today's openers send
//!   them; the record's own page names its opener (`opener`). A GET makes and joins nothing.

use axum::Json;
use axum::extract::{Path, RawQuery};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use cratarr_domain::record::{EditionOf, Handler, action_of};
use cratarr_domain::view::{Entity, RecordProperties};
use cratarr_domain::when::Place;
use serde_json::json;

use crate::app::App;
use crate::app::core::records::{Around, libraries_of, record_entity};
use crate::ctx::Ctx;

/// The words a record no longer here answers with (RV4).
pub use cratarr_domain::words::GONE;

/// `from`, `opener` and every `without` of a query.
fn asked(raw: Option<&str>) -> (String, Option<String>, Vec<String>) {
    let pairs: Vec<(String, String)> = raw
        .unwrap_or("")
        .split('&')
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (crate::api::form_decode(k), crate::api::form_decode(v)))
        .collect();
    let one = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).filter(|v| !v.is_empty());
    let without = pairs.iter().filter(|(k, v)| k == "without" && !v.is_empty()).map(|(_, v)| v.clone()).collect();
    (one("from").unwrap_or_else(|| "library".to_string()), one("opener"), without)
}

/// The answer: the entity, or a 404 in the table's words for a record never made or forgotten (RV4).
pub fn answered(found: Option<Entity<RecordProperties>>) -> Response {
    match found {
        Some(entity) => Json(entity).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": GONE }))).into_response(),
    }
}

fn refused(status: StatusCode, words: &str) -> Response {
    (status, Json(json!({ "error": words }))).into_response()
}

/// What a record's view reads beside the record, for `account` (its libraries named as they see them): the libraries in use, the
/// conversions at work, who asked for what, the open requests of the record, and its editions.
pub async fn around(app: &App, serve: &std::sync::Arc<crate::serve::Serve>, account: &str, record: i64) -> Around {
    let mut used = Vec::new();
    for (a, _) in app.library.connected().into_iter().filter(|(a, _)| a.cfg.kind == crate::source::cratarr::KIND.id) {
        for u in a.libraries() {
            let name = a.placement.libraries.iter().find(|p| p.id == u.id).map_or_else(|| u.id.clone(), |p| p.name.clone());
            let fetcher = a.cfg.libraries.get(&u.id).and_then(|s| s.fetcher.clone());
            used.push((u.id, name, u.kind, u.owner, fetcher));
        }
    }
    let libraries = libraries_of(serve, &used, account);
    let album = crate::serve::ids::record(record);
    let jobs = app
        .incoming
        .jobs()
        .into_iter()
        .filter(|j| j.source == crate::source::cratarr::KIND.id && j.album_id == album && j.at_work())
        .map(|j| (j.place.0, j.id))
        .collect();
    let requests = crate::app::requests::open_of(serve, record).await.into_iter().map(|r| (r.user, r.id)).collect();
    let (s, lib, a) = (serve.clone(), app.library.clone(), album.clone());
    let read = crate::run::spawn_blocking(move || {
        let asked = (*crate::app::requests::requesters(&s)).clone();
        let editions = crate::app::editions::of_record(&lib, &s, &a).unwrap_or_default();
        (asked, editions)
    })
    .await
    .unwrap_or_default();
    let (asked, editions) = read;
    let editions = editions
        .into_iter()
        .filter(|e| e.album != album)
        .filter_map(|e| {
            let id = crate::serve::ids::record_number(&e.album)?;
            let first = e.copies.first();
            let library = first.and_then(|c| libraries.iter().find(|l| l.folder == c.folder)).map(|l| l.name.clone());
            let quality = first.map(|c| cratarr_domain::words::quality(c.format.as_deref(), c.kbps.map(i64::from)));
            Some(EditionOf { id, tracks: u16::try_from(e.tracks).unwrap_or(u16::MAX), library, quality })
        })
        .collect();
    Around { libraries, kinds: serve.library.kinds(), jobs, asked, requests, editions }
}

/// What a record's handler asks first (CN9, M7): `decide()` for the viewer asking, on the record as it is read now, of the action
/// its route and body name (`action_of`, the view's address read back). A refusal in the table's words: a right a 403, a state a
/// 409, an upload hidden a 404. Another server's album is no record (nothing to decide here); an action `action_of` cannot name
/// (no copy said of several, a target not made yet) is the handler's own to answer.
pub async fn decided(ctx: &Ctx, source: &str, album: &str, handler: Handler, from: Option<&str>, to: Option<&str>) -> Result<(), crate::app::ToggleError> {
    use crate::app::ToggleError;
    use cratarr_domain::can::{Decision, Why, decide};
    if source != crate::source::cratarr::KIND.id {
        return Ok(());
    }
    let (Ok(serve), Some(number)) = (ctx.serve(), crate::serve::ids::record_number(album)) else { return Ok(()) };
    let mut viewer = crate::app::requests::viewer(&serve);
    let around = around(&ctx.app, &serve, &viewer.account, number).await;
    // No library of Cratarr's own in use: nothing names a copy here.
    if around.libraries.is_empty() {
        return Ok(());
    }
    viewer.own = around.libraries.iter().find(|l| matches!(&l.library, cratarr_domain::holding::Library::Own(o) if *o == viewer.account)).map(|l| l.folder);
    let (s, v) = (serve.clone(), viewer.clone());
    let read = crate::run::spawn_blocking(move || crate::app::core::records::read(&s, number, &around, &v, &Place::Library, &[]))
        .await
        .map_err(|e| ToggleError::Upstream(e.into()))?
        .map_err(ToggleError::Upstream)?;
    let Some((_, facts)) = read else { return Err(ToggleError::NotFound(GONE.into())) };
    let Some(action) = action_of(&facts, handler, from, to) else { return Ok(()) };
    match decide(&viewer, &action, &facts.record(&viewer)) {
        Decision::Allow | Decision::AskFirst(_) => Ok(()),
        Decision::Deny(why @ Why::Right(_)) => Err(ToggleError::Forbidden(why.words().into())),
        Decision::Deny(why @ Why::State(_)) => Err(ToggleError::Refused(why.words().into())),
        Decision::Deny(why @ Why::Hidden) => Err(ToggleError::NotFound(why.words().into())),
    }
}

/// `GET /api/records/{id}`: the record's entity for the viewer asking, from the place asked (RV1).
pub async fn record(ctx: Ctx, Path(id): Path<String>, RawQuery(raw): RawQuery) -> Response {
    let Ok(serve) = ctx.serve() else { return refused(StatusCode::CONFLICT, "Cratarr's own library is not open.") };
    let Some(number) = crate::serve::ids::record_number(&id) else { return answered(None) };
    let (from, opener, without) = asked(raw.as_deref());
    let Some(place) = Place::of(&from, &[], opener.as_deref()) else { return refused(StatusCode::BAD_REQUEST, "No place is called that.") };
    let mut viewer = crate::app::requests::viewer(&serve);
    let around = around(&ctx.app, &serve, &viewer.account, number).await;
    viewer.own = around.libraries.iter().find(|l| matches!(&l.library, cratarr_domain::holding::Library::Own(o) if *o == viewer.account)).map(|l| l.folder);
    let off = ctx.app.library.folders_off(&without);
    let s = serve.clone();
    let read = crate::run::spawn_blocking(move || record_entity(&s, number, &around, &viewer, &place, &off)).await;
    match read {
        Ok(Ok(found)) => answered(found),
        Ok(Err(e)) => {
            tracing::warn!(target: "server", record = %id, reason = %format!("{e:#}"), "a record's view was not read");
            refused(StatusCode::BAD_GATEWAY, "The record could not be read.")
        }
        Err(e) => {
            tracing::warn!(target: "server", record = %id, reason = %e, "a record's view was not read");
            refused(StatusCode::BAD_GATEWAY, "The record could not be read.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cratarr_domain::can::{ArrivalFacts, May, Viewer};
    use cratarr_domain::holding::{CopyFacts, Fetcher, Library};
    use cratarr_domain::record::{CopyOf, LibraryOf, RecordFacts, Seen, entity, epoch};
    use cratarr_domain::view::{Method, Named};

    const MUSIC: i64 = 0;
    const IN: i64 = 1;
    const OWN_A: i64 = 2;
    const MORE: i64 = 4;

    fn libraries() -> Vec<LibraryOf> {
        let lib = |folder: i64, id: &str, name: &str, library: Library| LibraryOf { folder, id: id.into(), name: name.into(), library };
        vec![
            lib(MUSIC, "/music/MUSIC", "MUSIC", Library::Main),
            lib(IN, "/music/IN", "IN", Library::Incoming { fetcher: Fetcher::Lidarr, uploader: None }),
            lib(OWN_A, "/music/own/alice", "alice's music", Library::Own("alice".into())),
            lib(MORE, "/music/MORE", "MORE", Library::Main),
        ]
    }

    fn copy(folder: i64, library: Library) -> CopyOf {
        CopyOf { facts: CopyFacts { folder, library, present: 10, paired: 10, went: None }, quality: Some("FLAC".into()), added: 1, job: None, arrival: None }
    }

    /// Night Ferry with every kind of copy root may act on: MUSIC's (playing), OWN-A's better one, an arrival in IN asked by alice,
    /// MORE's being converted; a request of alice's open; its artist and genre.
    fn every_action() -> RecordFacts {
        let mut converting = copy(MORE, Library::Main);
        converting.job = Some(7);
        let mut arrival = copy(IN, Library::Incoming { fetcher: Fetcher::Lidarr, uploader: None });
        arrival.arrival = Some(ArrivalFacts { state: cratarr_domain::arrival::Arrival::Waiting, asked_by: vec!["alice".into()], by: None });
        RecordFacts {
            id: 42,
            title: "Night Ferry".into(),
            kind: Some("Album".into()),
            artists: vec![Named { id: "ar-3".into(), name: "Glass Harbour".into(), catno: None }],
            genres: vec!["Electronic".into()],
            copies: vec![copy(MUSIC, Library::Main), copy(OWN_A, Library::Own("alice".into())), arrival, converting],
            shortest: Some(12),
            plays: Some(MUSIC),
            requests: vec![("alice".into(), "5".into())],
            replacing: vec![(OWN_A, MUSIC), (IN, MUSIC)],
            libraries: libraries(),
            ..RecordFacts::default()
        }
    }

    fn root() -> Viewer {
        Viewer::new("root", true, None, May::default(), None)
    }

    /// The API's routes as its sources declare them: (method, path pattern), `{x}` any one segment.
    fn routes() -> Vec<(String, String)> {
        let src = concat!(include_str!("../../api.rs"), include_str!("../routes.rs"), include_str!("../library_api.rs"));
        let mut out = Vec::new();
        for part in src.split(".route(").skip(1) {
            // A route's path, its string first after any line break.
            let Some(part) = part.trim_start().strip_prefix('"') else { continue };
            let Some((path, rest)) = part.split_once('"') else { continue };
            let call = rest.split(".route(").next().unwrap_or(rest);
            for (word, method) in [("get(", "GET"), ("post(", "POST"), ("put(", "PUT"), ("delete(", "DELETE")] {
                if call.contains(word) {
                    out.push((method.to_string(), path.to_string()));
                }
            }
        }
        out
    }

    fn matches(pattern: &str, href: &str) -> bool {
        let (p, h): (Vec<&str>, Vec<&str>) = (pattern.split('/').collect(), href.split('?').next().unwrap_or(href).split('/').collect());
        p.len() == h.len() && p.iter().zip(&h).all(|(a, b)| (a.starts_with('{') && a.ends_with('}') && !b.is_empty()) || a == b)
    }

    /// RV10: every action carries its address and the one shape: each `href` a route the router takes, by its method; Delete with
    /// its confirm naming its copy, Play none. Red first: no record action has an address (the page calls each handler its own way).
    #[test]
    fn every_record_action_is_routed() {
        let routes = routes();
        assert!(routes.len() > 100, "the router's routes read: {}", routes.len());
        let place = Place::Library;
        let (root, alice) = (root(), Viewer::new("alice", false, None, May { request: true, offline: true, playlists: true }, Some(OWN_A)));
        let mut seen = 0;
        for v in [&root, &alice] {
            for facts in [every_action(), RecordFacts { copies: Vec::new(), plays: None, preview: Some("https://kite.example.org/album/a".into()), ..every_action() }] {
                let e = entity(&facts, &Seen { viewer: v, place: &place, off: &[], rev: 1 });
                for a in &e.actions {
                    let method = match a.method {
                        Method::Get => "GET",
                        Method::Post => "POST",
                        Method::Put => "PUT",
                    };
                    assert!(routes.iter().any(|(m, p)| m == method && matches(p, &a.href)), "{} {} ({}) is no route", method, a.href, a.name);
                    seen += 1;
                }
            }
        }
        assert!(seen > 20, "{seen} actions");
        let roots = entity(&every_action(), &Seen { viewer: &root, place: &place, off: &[], rev: 1 });
        let delete = roots.actions.iter().find(|a| a.name == "delete_from").expect("root's Delete");
        assert!(delete.confirm.as_deref().is_some_and(|c| c.starts_with("Delete from")), "{delete:?}");
        assert!(roots.actions.iter().find(|a| a.name == "play").is_some_and(|a| a.confirm.is_none()));
    }

    /// RV4: a record never made, or forgotten, answers 404 with the table's words; never an empty entity.
    #[test]
    fn a_record_gone_answers_404_with_the_tables_words() {
        let serve = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        serve.attach_catalogue(std::sync::Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap()));
        let cat = serve.catalogue();
        let gone = cat.record_for_tests("Night Ferry", 0);
        cat.db_for_tests().execute("DELETE FROM records WHERE id = ?1", [gone]).unwrap();
        let read = crate::app::core::records::record_entity(&serve, gone, &Around::default(), &root(), &Place::Library, &[]).unwrap();
        assert!(read.is_none(), "forgotten: none");
        let res = answered(read);
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(GONE, "That record is gone");
        assert!(crate::app::core::records::record_entity(&serve, 999_999, &Around::default(), &root(), &Place::Library, &[]).unwrap().is_none(), "never made");
    }

    /// LV4: what is no row goes in the epoch and the key: the place's libraries, the viewer's role, scope and permissions. A switch
    /// changes the epoch of a place that follows it (the Library, an artist's page, a crate: the owner's 21:37 and 23:14), not of one
    /// that counts every library (a work, Discover); a permission or a key's scope changes every one of that viewer's.
    #[test]
    fn the_epoch_names_what_is_no_row() {
        let may = May { request: true, offline: true, playlists: true };
        let alice = Viewer::new("alice", false, None, may, Some(OWN_A));
        let no_offline = Viewer::new("alice", false, None, May { offline: false, ..may }, Some(OWN_A));
        let by_key = Viewer::new("alice", false, Some(cratarr_domain::can::Scope::Read), may, Some(OWN_A));
        let crate_ = Place::Crate { without: Vec::new() };
        for place in [Place::Library, Place::Artist, crate_.clone()] {
            assert_ne!(epoch(&alice, &place, &[]), epoch(&alice, &place, &[MUSIC]), "{place:?} follows the switch");
        }
        for place in [Place::Work, Place::Discover] {
            assert_eq!(epoch(&alice, &place, &[]), epoch(&alice, &place, &[MUSIC]), "{place:?} counts every library");
        }
        for place in [Place::Library, Place::Work, crate_] {
            assert_ne!(epoch(&alice, &place, &[]), epoch(&no_offline, &place, &[]), "a permission: every epoch");
            assert_ne!(epoch(&alice, &place, &[]), epoch(&by_key, &place, &[]), "a key's scope");
        }
        assert_ne!(epoch(&alice, &Place::Library, &[]), epoch(&root(), &Place::Library, &[]), "the viewer");
        assert_eq!(epoch(&alice, &Place::Library, &[MORE, MUSIC]), epoch(&alice, &Place::Library, &[MUSIC, MORE]), "one set, one epoch");
    }

    /// TY3: one id, one name: a record is `al-<n>` under `id` in its entity, row, card and chip; its title under `title`; revs and
    /// times are numbers. Never `album`, `subject` or a bare number for its id.
    #[test]
    fn a_record_is_named_one_way() {
        let facts = every_action();
        let root = root();
        let seen = Seen { viewer: &root, place: &Place::Library, off: &[], rev: 140 };
        let shapes = [
            serde_json::to_value(entity(&facts, &seen)).unwrap(),
            serde_json::to_value(cratarr_domain::record::row(&facts, &seen)).unwrap(),
            serde_json::to_value(cratarr_domain::record::card(&facts, &seen)).unwrap(),
            serde_json::to_value(cratarr_domain::record::chip(&facts)).unwrap(),
        ];
        for v in &shapes {
            assert_eq!(v["id"], "al-42");
            for never in ["album", "subject", "record_id"] {
                assert!(v.get(never).is_none(), "{never} in {v}");
            }
        }
        assert_eq!(shapes[0]["properties"]["title"]["value"], "Night Ferry");
        for v in &shapes[1..3] {
            assert_eq!(v["title"], "Night Ferry");
            assert!(v["rev"].is_number());
        }
        assert!(shapes[0]["rev"].is_number());
    }

    /// The handler an offered action's address asks, by its route.
    fn handler_at(href: &str) -> Option<Handler> {
        [("/favorite", Handler::Favourite), ("/rating", Handler::Rating), ("/move", Handler::Move), ("/delete", Handler::Delete), ("/covers", Handler::ChangeCover)]
            .into_iter()
            .find(|(end, _)| href.ends_with(end))
            .map(|(_, h)| h)
            .or_else(|| href.starts_with("/api/incoming/replace/").then_some(Handler::Replace))
            .or_else(|| (href == "/api/albums/overrides").then_some(Handler::Edit))
    }

    /// CN9: the view and the handler agree. Every action a view offers, `decide` lets through, and its handler, reading its address
    /// back (`action_of`), asks `decide` of that very action; every other the record could take here is refused in the table's
    /// words, a right a 403, a state a 409, an upload hidden a 404. The dry run is `decide` itself. Red first: the handlers decide
    /// apart from the view (albums.rs's `move_refused`, `may_delete`; the page's `offersOf`).
    #[test]
    fn every_offered_action_is_taken_and_every_other_refused() {
        use cratarr_domain::can::{Decision, decide};
        use cratarr_domain::record::{handler_of, judged};
        let may = May { request: true, offline: true, playlists: true };
        let viewers = [
            root(),
            Viewer::new("root", true, Some(cratarr_domain::can::Scope::Read), May::default(), None),
            Viewer::new("alice", false, None, may, Some(OWN_A)),
            Viewer::new("bob", false, None, may, None),
            Viewer::new("carol", false, None, May::default(), None),
        ];
        let not_had = RecordFacts { copies: Vec::new(), plays: None, ..every_action() };
        let only_hers = RecordFacts { copies: vec![copy(OWN_A, Library::Own("alice".into()))], plays: Some(OWN_A), replacing: Vec::new(), ..every_action() };
        let mut checked = 0;
        for facts in [every_action(), not_had, only_hers] {
            for v in &viewers {
                for place in [Place::Library, Place::Search, Place::Incoming] {
                    let seen = Seen { viewer: v, place: &place, off: &[], rev: 1 };
                    let offered = entity(&facts, &seen).actions;
                    let record = facts.record(v);
                    for a in &offered {
                        let Some(handler) = handler_at(&a.href) else { continue };
                        let fields = a.fields.clone().unwrap_or_default();
                        let action = action_of(&facts, handler, fields.from.as_deref(), fields.to.as_deref()).unwrap_or_else(|| panic!("{} read back", a.title));
                        assert!(!matches!(decide(v, &action, &record), Decision::Deny(_)), "{} offered to {} and refused by its handler", a.title, v.account);
                        checked += 1;
                    }
                    for (action, d) in judged(&facts, &seen) {
                        if let Decision::Deny(why) = d {
                            assert!(!why.words().is_empty(), "{action:?}: a reason with words");
                            assert!([403, 409, 404].contains(&why.status()));
                            if let Some(h) = handler_of(&action) {
                                assert!(offered.iter().all(|a| handler_at(&a.href) != Some(h) || {
                                    let f = a.fields.clone().unwrap_or_default();
                                    action_of(&facts, h, f.from.as_deref(), f.to.as_deref()) != Some(action.clone())
                                }), "{action:?} refused yet offered to {}", v.account);
                            }
                        }
                    }
                }
            }
        }
        assert!(checked > 10, "{checked} offered actions read back");
    }

    /// RS19 (the server's half): an action on a copy gone while the sheet was open is refused, a 409 in the table's words, and moves
    /// nothing: the handler's `decide` reads the record as it is now.
    #[test]
    fn an_action_on_a_copy_gone_is_refused_with_the_tables_words() {
        use cratarr_domain::can::{Decision, Why, decide};
        let mut facts = every_action();
        let root = root();
        // IN's copy deleted outside Cratarr: the sheet still names it.
        facts.copies.retain(|c| c.facts.folder != IN);
        let replace = action_of(&facts, Handler::Replace, Some("/music/IN"), Some("/music/MUSIC")).unwrap();
        let d = decide(&root, &replace, &facts.record(&root));
        assert_eq!(d, Decision::Deny(Why::State("That copy is gone")));
        assert_eq!(Why::State("That copy is gone").status(), 409);
    }

    /// CN9 (its handlers): every record handler of the actions RS01 offers that change a record, a copy or an arrival asks
    /// `decide` first, before the app is asked anything: the Library's Delete, Save to, Move to and Add to, the admin routes' Keep,
    /// Delete and Replace, Change cover.
    #[test]
    fn every_record_handler_decides_first() {
        let src = include_str!("../../api.rs");
        for handler in ["delete_incoming", "delete_album", "move_album", "keep_incoming", "replace_incoming", "set_album_cover"] {
            let start = src.find(&format!("async fn {handler}(")).unwrap_or_else(|| panic!("{handler}"));
            let body = &src[start..];
            let body = &body[..body.find("\n}\n").unwrap()];
            // Its body, after its signature (whose types may name what it calls).
            let body = &body[body.find("{\n").unwrap()..];
            let decided = body.find("views::record::decided(").unwrap_or_else(|| panic!("{handler} decides first"));
            for asked in ["app.incoming.", "app.delete_album(", "app.move_album(", "app.replace_album(", "app.set_", "file_runs::"] {
                if let Some(at) = body.find(asked) {
                    assert!(decided < at, "{handler}: decide before {asked}");
                }
            }
        }
    }

    /// The types the record's answers are written in, each as the page imports it from `generated/`.
    const TYPES: [&str; 18] = [
        "Entity", "RecordProperties", "FactView", "Named", "RecordIs", "Shown", "ActionView", "ActionFields", "PlayWhat", "Method",
        "EntryGroup", "LinkView", "Section", "Version", "VersionKind", "RecordRow", "RecordCard", "RecordChip",
    ];

    fn generated() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../web/src/lib/generated")
    }

    fn export_all(dir: &std::path::Path) {
        use ts_rs::TS;
        let cfg = ts_rs::Config::from_env().with_out_dir(dir);
        Entity::<RecordProperties>::export_all(&cfg).unwrap();
        cratarr_domain::view::Row::export_all(&cfg).unwrap();
        cratarr_domain::view::Card::export_all(&cfg).unwrap();
        cratarr_domain::view::Chip::export_all(&cfg).unwrap();
    }

    /// TY1: the record's view types are generated (ts-rs), each its own file under `web/src/lib/generated`, for the page to import.
    /// Red first: none is (the page's api.ts writes `AlbumView` and the copies by hand).
    #[test]
    fn record_types_are_exported() {
        let dir = std::env::temp_dir().join(format!("cratarr-ty1-{}", std::process::id()));
        export_all(&dir);
        for t in TYPES {
            assert!(dir.join(format!("{t}.ts")).exists(), "{t} written");
            assert!(generated().join(format!("{t}.ts")).exists(), "{t} in web/src/lib/generated");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// TY2: a stale type fails: the types written afresh to a scratch folder are the ones in `web/src/lib/generated`, byte for byte.
    #[test]
    fn the_generated_types_are_current() {
        let dir = std::env::temp_dir().join(format!("cratarr-ty2-{}", std::process::id()));
        export_all(&dir);
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let fresh = std::fs::read_to_string(&path).unwrap();
            let kept = std::fs::read_to_string(generated().join(&name)).unwrap_or_default().replace('\r', "");
            assert_eq!(kept, fresh, "web/src/lib/generated/{name} is not the server's type: write it again");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// TY5: no type is written twice: every type the generated folder declares, in one file alone, the domain's and the rules
    /// crate's (`Holding`'s kin) beside the app's.
    #[test]
    fn no_type_is_written_twice() {
        let mut declared: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for entry in std::fs::read_dir(generated()).unwrap() {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            for line in text.lines().filter(|l| l.starts_with("export type ")) {
                let name = line["export type ".len()..].split(['<', ' ', '=']).next().unwrap().to_string();
                if let Some(other) = declared.insert(name.clone(), file.clone()) {
                    panic!("{name} written twice: {other} and {file}");
                }
            }
        }
        for t in TYPES {
            assert_eq!(declared.get(t).map(String::as_str), Some(format!("{t}.ts").as_str()), "{t} in its own file");
        }
    }
}
