//! The one command path through the router `main` serves (SCENARIOS-GF CP1, CP6, CP7 and CP9; GF's core): artist Out everywhere,
//! the core's real kind, answers `{ result, undo, rev }` with one token kind, `j<n>`, at once, and is undone through `POST
//! /api/undo` alone, by the one rule of who may undo; its Undo answers a Redo, a new Change on the same path. The path's own test
//! kind runs in `app::commands`' tests (AP1, CP3, CP4, CP8, CP9).

use axum::body::Body;
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::whole_router::Whole;

/// `path` asked with that cookie, `body` as JSON.
async fn send(w: &Whole, cookie: &str, method: Method, path: &str, body: Value) -> (StatusCode, Value) {
    w.send(("cookie", cookie), method, path, Body::from(body.to_string())).await
}

/// Glass Harbour out everywhere, for the account signed in as `cookie`.
async fn out(w: &Whole, cookie: &str) -> (StatusCode, Value) {
    send(w, cookie, Method::PUT, "/api/me/excluded-artists/glass%20harbour", json!({ "name": "Glass Harbour" })).await
}

async fn undo(w: &Whole, cookie: &str, tokens: &[&str]) -> (StatusCode, Value) {
    send(w, cookie, Method::POST, "/api/undo", json!({ "tokens": tokens })).await
}

/// The artists the account excluded everywhere, by name.
async fn listed(w: &Whole, cookie: &str) -> Vec<String> {
    let (status, list) = send(w, cookie, Method::GET, "/api/me/excluded-artists", Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    list.as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap().to_string()).collect()
}

/// One token kind: `j` and its number.
fn one_kind(token: &str) -> bool {
    token.strip_prefix('j').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The server with alice and bob (users) and dave (a second administrator) beside its administrator `me`.
async fn accounts(name: &str) -> Whole {
    let w = Whole::open(name).await;
    for (user, admin) in [("alice", false), ("bob", false), ("dave", true)] {
        w.served.users.from_directory(user, admin, None).unwrap();
    }
    w
}

/// CP1 (its core half, on Out): the answer carries one token of one kind at once, and the rev it made; the Undo goes through
/// `POST /api/undo`, names the Change in the server's words and carries its rev and its Redo, itself a token of the same kind;
/// the Redo does it again. Nothing changed, no token. Red first: Out answers `{key, name, at, undo: "c…"}` (a token of the
/// playlists' kind, no rev).
#[tokio::test]
async fn one_token_one_route() {
    let w = accounts("one-token").await;
    let alice = w.session("alice");
    let (status, made) = out(&w, &alice).await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!((made["result"]["key"].as_str(), made["result"]["name"].as_str()), (Some("glassharbour"), Some("Glass Harbour")), "{made}");
    let token = made["undo"].as_str().unwrap_or_else(|| panic!("a token at once: {made}"));
    assert!(one_kind(token), "{token}");
    let rev = made["rev"].as_i64().unwrap_or_else(|| panic!("its rev: {made}"));
    assert!(rev > 0);
    // Out again: nothing changes, so no token; the rev it stands at.
    let (_, again) = out(&w, &alice).await;
    assert!(again.get("undo").is_none_or(Value::is_null), "{again}");
    assert_eq!(again["rev"].as_i64(), Some(rev), "{again}");
    // Undone through the one route: named in the server's words, its rev, and a Redo of the same kind.
    let (status, undone) = undo(&w, &alice, &[token]).await;
    assert_eq!(status, StatusCode::OK, "{undone}");
    assert_eq!(undone["undone"][0]["token"].as_str(), Some(token), "{undone}");
    assert_eq!(undone["undone"][0]["name"].as_str(), Some("Glass Harbour excluded everywhere."), "{undone}");
    assert!(undone["rev"].as_i64().unwrap() > rev, "{undone}");
    let redo = undone["undone"][0]["redo"].as_str().unwrap_or_else(|| panic!("its Redo: {undone}")).to_string();
    assert!(one_kind(&redo) && redo != token, "{redo}");
    assert_eq!(undone["redo"][0].as_str(), Some(redo.as_str()), "the toast's Redo");
    assert!(listed(&w, &alice).await.is_empty());
    // The Redo: out again, and its own Undo's Redo in turn.
    let (status, redone) = undo(&w, &alice, &[redo.as_str()]).await;
    assert_eq!(status, StatusCode::OK, "{redone}");
    assert_eq!(listed(&w, &alice).await, ["Glass Harbour"]);
    let again_on = redone["undone"][0]["redo"].as_str().unwrap_or_else(|| panic!("{redone}")).to_string();
    assert!(one_kind(&again_on), "{again_on}");
    // Taken off: a Change of its own, its token of the same kind.
    let (status, off) = send(&w, &alice, Method::DELETE, "/api/me/excluded-artists/glass%20harbour", Value::Null).await;
    assert_eq!((status, off["result"]["key"].as_str()), (StatusCode::OK, Some("glassharbour")), "{off}");
    assert!(off["undo"].as_str().is_some_and(one_kind), "{off}");
    // CP5: the Change that put them on again meets that change on top, and changes nothing.
    let (status, on_top) = undo(&w, &alice, &[again_on.as_str()]).await;
    assert_eq!((status, on_top["error"].as_str()), (StatusCode::CONFLICT, Some("It changed since; nothing was undone.")), "{on_top}");
    assert!(listed(&w, &alice).await.is_empty(), "nothing changed");
    // Undone once: a second Undo of the same token is refused in the one shape.
    let (status, second) = undo(&w, &alice, &[token]).await;
    assert_eq!((status, second["error"].as_str()), (StatusCode::CONFLICT, Some("That can no longer be undone.")), "{second}");
}

/// CP6 (its core half, on Out, per account): the account's alone to undo, administrators included; a key that doesn't sign in
/// as it can't; each 403 before anything is changed. Red first: an Out token today is a `c…`, refused to every other account as
/// "That can no longer be undone." (409), never in the rights' shape.
#[tokio::test]
async fn who_may_undo_is_one_rule() {
    let w = accounts("who-may-undo").await;
    let (alice, bob, me, dave) = (w.session("alice"), w.session("bob"), w.session("me"), w.session("dave"));
    let (_, made) = out(&w, &alice).await;
    let token = made["undo"].as_str().unwrap_or_else(|| panic!("{made}")).to_string();
    for (who, cookie) in [("root", &me), ("dave", &dave), ("bob", &bob)] {
        let (status, refused) = undo(&w, cookie, &[token.as_str()]).await;
        assert_eq!((status, refused["error"].as_str()), (StatusCode::FORBIDDEN, Some("That isn't yours to undo.")), "{who}: {refused}");
    }
    // Home Assistant's key (an administrator's, controlling the players) changes no account's own data.
    let (status, refused) = w.send(("authorization", &w.key("me", "control")), Method::POST, "/api/undo", Body::from(json!({ "tokens": [token] }).to_string())).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "the control key: {refused}");
    // A reading key is turned away at the door, before the path (the fence: "This key only reads.").
    let (status, _) = w.send(("authorization", &w.key("alice", "read")), Method::POST, "/api/undo", Body::from(json!({ "tokens": [token] }).to_string())).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "the reading key");
    assert_eq!(listed(&w, &alice).await, ["Glass Harbour"], "refused: nothing changed");
    let (status, undone) = undo(&w, &alice, &[token.as_str()]).await;
    assert_eq!((status, undone["undone"].as_array().map(Vec::len)), (StatusCode::OK, Some(1)), "alice, her own: {undone}");
    assert!(listed(&w, &alice).await.is_empty());
}

/// CP9 and CP7 (their core half, on Out): the answer's rev already holds its own Change, written with it in one transaction; at
/// once, her other device asking what changed since the rev it read names it at a rev no higher than the answer's; the Undo's
/// likewise, at its own rev. Red first: Out answers no rev.
#[tokio::test]
async fn an_answer_carries_the_rev_of_its_own_change() {
    let w = accounts("rev-at-once").await;
    let (page, phone) = (w.session("alice"), w.session("alice"));
    let (_, read) = send(&w, &phone, Method::GET, "/api/changes?since=0&kinds=excluded", Value::Null).await;
    let at = read["rev"].as_i64().unwrap_or_else(|| panic!("{read}"));
    let (_, made) = out(&w, &page).await;
    let rev = made["rev"].as_i64().unwrap_or_else(|| panic!("its rev: {made}"));
    let names = |since: i64, upto: i64| {
        let w = &w;
        let phone = phone.clone();
        async move {
            let (_, seen) = send(w, &phone, Method::GET, &format!("/api/changes?since={since}&kinds=excluded"), Value::Null).await;
            seen["changes"].as_array().is_some_and(|c| c.iter().any(|c| c["kind"] == "excluded" && c["id"] == "artists" && c["rev"].as_i64().is_some_and(|r| r <= upto)))
        }
    };
    assert!(names(at, rev).await, "the phone, at once: what changed since it read names it at a rev no higher than the answer's");
    let (_, undone) = undo(&w, &page, &[made["undo"].as_str().unwrap()]).await;
    let undo_rev = undone["rev"].as_i64().unwrap_or_else(|| panic!("{undone}"));
    assert!(undo_rev > rev && names(rev, undo_rev).await, "the Undo, at its own rev: {undone}");
}
