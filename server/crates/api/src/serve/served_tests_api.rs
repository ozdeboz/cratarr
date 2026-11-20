//! Tests moved up from served's files (the app split's phase 2): each sends requests through the routes (`serve::routes`),
//! so they go up to the api with them. By origin, their bodies as they were but for paths.

/// From `serve::signin`'s tests.
#[cfg(test)]
mod signin {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use axum::http::{HeaderValue, StatusCode};
    use axum::response::Response;
    use tower::ServiceExt;

    use crate::catalogue::Rules;
    use crate::serve::Serve;
    use crate::serve::sessions::LIFETIME;
    use crate::serve::signin::testing::FIREFOX;
    use crate::serve::signin::{COOKIE, WINDOW};

    fn served() -> Arc<Serve> {
        let s = Serve::open(None, Rules::default(), &|_| None).unwrap();
        s.users.set("me", "pw").unwrap();
        s.sessions.set_required(false).unwrap();
        s
    }

    async fn send(s: &Arc<Serve>, req: HttpRequest<Body>) -> Response {
        crate::serve::routes::router(s.clone()).oneshot(req).await.unwrap()
    }

    async fn body(r: Response) -> String {
        String::from_utf8(axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap()
    }

    fn header<'a>(r: &'a Response, name: &str) -> &'a str {
        r.headers().get(name).map_or("", |v| v.to_str().unwrap())
    }

    /// A sign-in form posted through the proxy over HTTPS, from `client`.
    fn sign_in_form(user: &str, password: &str, client: &str) -> HttpRequest<Body> {
        let form = serde_urlencoded::to_string([("username", user), ("password", password), ("next", "/#/crates")]).unwrap();
        HttpRequest::post("/signin")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("host", "cratarr.example")
            .header("origin", "https://cratarr.example")
            .header("x-forwarded-proto", "https")
            .header("x-forwarded-for", client)
            .header("user-agent", FIREFOX)
            .body(Body::from(form))
            .unwrap()
    }

    /// The proxy asking whether a request may pass.
    fn check(uri: &str, cookie: Option<&str>, mode: &str) -> HttpRequest<Body> {
        let mut b = HttpRequest::get("/api/auth/check").header("x-forwarded-method", "GET").header("x-forwarded-uri", uri).header("sec-fetch-mode", mode);
        if let Some(c) = cookie {
            b = b.header("cookie", c);
        }
        b.body(Body::empty()).unwrap()
    }

    /// The `name=token` part of a response's Set-Cookie.
    fn cookie_of(r: &Response) -> String {
        header(r, "set-cookie").split(';').next().unwrap().to_string()
    }

    #[tokio::test]
    async fn a_sign_in_gives_a_session_the_proxy_lets_through_until_sign_out() {
        let s = served();
        let r = send(&s, sign_in_form("me", "pw", "203.0.113.7")).await;
        assert_eq!(r.status(), StatusCode::SEE_OTHER);
        assert_eq!(header(&r, "location"), "/#/crates", "back where the page was");
        let set = header(&r, "set-cookie").to_string();
        for part in ["__Host-cratarr-session=", "Path=/", &format!("Max-Age={LIFETIME}"), "Secure", "HttpOnly", "SameSite=Lax"] {
            assert!(set.contains(part), "{set} lacks {part}");
        }
        assert!(!set.contains("Domain"), "a __Host- cookie has none");
        let cookie = cookie_of(&r);

        let ok = send(&s, check("/api/library", Some(&cookie), "cors")).await;
        assert_eq!(ok.status(), StatusCode::NO_CONTENT);
        assert_eq!(header(&ok, "x-cratarr-user"), "me");
        let me = body(send(&s, HttpRequest::get("/api/auth/session").header("cookie", &cookie).body(Body::empty()).unwrap()).await).await;
        assert!(me.contains(r#""user":"me""#), "{me}");
        let list: serde_json::Value =
            serde_json::from_str(&body(send(&s, HttpRequest::get("/api/auth/sessions").header("cookie", &cookie).body(Body::empty()).unwrap()).await).await).unwrap();
        assert_eq!(list["sessions"][0]["device"], "Firefox on Windows");
        assert_eq!(list["sessions"][0]["current"], true);

        let out = send(
            &s,
            HttpRequest::post("/signout")
                .header("cookie", &cookie)
                .header("host", "cratarr.example")
                .header("origin", "https://cratarr.example")
                .header("x-forwarded-proto", "https")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!((out.status(), header(&out, "location")), (StatusCode::SEE_OTHER, "/signin"));
        assert!(header(&out, "set-cookie").contains("Max-Age=0"));
        assert_eq!(send(&s, check("/api/library", Some(&cookie), "cors")).await.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn without_a_session_a_page_goes_to_the_sign_in_page_and_the_rest_is_refused() {
        let s = served();
        let page = send(&s, check("/library?x=1", None, "navigate")).await;
        assert_eq!(page.status(), StatusCode::SEE_OTHER);
        assert_eq!(header(&page, "location"), "/signin?next=%2Flibrary%3Fx%3D1");
        let api = send(&s, check("/api/library", None, "cors")).await;
        assert_eq!(api.status(), StatusCode::UNAUTHORIZED);
        assert!(header(&api, "www-authenticate").starts_with("Cookie realm=\"Cratarr\""), "no Basic: no browser login box");
        assert_eq!(send(&s, check("/signin?next=%2F", None, "navigate")).await.status(), StatusCode::NO_CONTENT);
        assert_eq!(send(&s, check("/rest/ping.view", None, "no-cors")).await.status(), StatusCode::NO_CONTENT);
        // The old login is no way in any more.
        let basic = HttpRequest::get("/api/auth/check")
            .header("x-forwarded-uri", "/api/library")
            .header("authorization", "Basic bWU6cHc=")
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(&s, basic).await.status(), StatusCode::UNAUTHORIZED);
        let forged = send(&s, check("/api/library", Some(&format!("{COOKIE}={}", "a".repeat(64))), "cors")).await;
        assert_eq!(forged.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_wrong_password_reads_as_an_unknown_account_and_tries_are_throttled() {
        let s = served();
        let wrong = send(&s, sign_in_form("me", "nope", "203.0.113.7")).await;
        let unknown = send(&s, sign_in_form("nobody", "pw", "203.0.113.8")).await;
        assert_eq!((wrong.status(), unknown.status()), (StatusCode::UNAUTHORIZED, StatusCode::UNAUTHORIZED));
        assert!(wrong.headers().get("set-cookie").is_none());
        let (wrong, unknown) = (body(wrong).await, body(unknown).await);
        assert!(wrong.contains("Wrong user name or password.") && unknown.contains("Wrong user name or password."));
        assert!(wrong.contains(r#"value="me""#), "the user name stays filled in");

        // Ten failures for the account, from several clients: the account waits, even with its password, from anywhere.
        for i in 1..10 {
            send(&s, sign_in_form("me", "nope", &format!("198.51.100.{i}"))).await;
        }
        let held = send(&s, sign_in_form("me", "pw", "192.0.2.1")).await;
        assert_eq!(held.status(), StatusCode::TOO_MANY_REQUESTS);
        let wait: i64 = header(&held, "retry-after").parse().unwrap();
        assert!((1..=WINDOW).contains(&wait), "{wait}");
        assert!(body(held).await.contains("Too many tries."));
        // Ten failures from one client: it waits, whichever account it tries.
        s.users.set("you", "pw2").unwrap();
        for _ in 0..10 {
            send(&s, sign_in_form("someone", "x", "203.0.113.99")).await;
        }
        assert_eq!(send(&s, sign_in_form("you", "pw2", "203.0.113.99")).await.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(send(&s, sign_in_form("you", "pw2", "203.0.113.100")).await.status(), StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn a_sign_in_from_another_site_is_refused() {
        let s = served();
        let mut req = sign_in_form("me", "pw", "203.0.113.7");
        req.headers_mut().insert("origin", HeaderValue::from_static("https://evil.example"));
        let r = send(&s, req).await;
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
        assert!(r.headers().get("set-cookie").is_none());
    }

    #[tokio::test]
    async fn each_sign_in_is_a_new_session_and_a_new_password_ends_the_others() {
        let s = served();
        let first = cookie_of(&send(&s, sign_in_form("me", "pw", "203.0.113.7")).await);
        let mut again = sign_in_form("me", "pw", "203.0.113.7");
        again.headers_mut().insert("cookie", HeaderValue::from_str(&first).unwrap());
        let second = cookie_of(&send(&s, again).await);
        assert_ne!(first, second);
        assert_eq!(send(&s, check("/", Some(&first), "cors")).await.status(), StatusCode::UNAUTHORIZED, "the old one ended");
        let other = cookie_of(&send(&s, sign_in_form("me", "pw", "203.0.113.8")).await);

        let change = HttpRequest::post("/api/subsonic/password")
            .header("content-type", "application/json")
            .header("cookie", &second)
            .body(Body::from(r#"{"username":"me","current":"pw","password":"pw2"}"#))
            .unwrap();
        assert_eq!(send(&s, change).await.status(), StatusCode::OK);
        assert_eq!(send(&s, check("/", Some(&second), "cors")).await.status(), StatusCode::NO_CONTENT, "the device that changed it");
        assert_eq!(send(&s, check("/", Some(&other), "cors")).await.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn over_plain_http_the_cookie_cannot_be_a_host_cookie() {
        let s = served();
        let mut req = sign_in_form("me", "pw", "192.168.1.20");
        req.headers_mut().remove("x-forwarded-proto");
        req.headers_mut().insert("origin", HeaderValue::from_static("http://cratarr.example"));
        let r = send(&s, req).await;
        let set = header(&r, "set-cookie");
        assert!(set.starts_with("cratarr-session=") && !set.contains("Secure"), "{set}");
        assert_eq!(send(&s, check("/", Some(&cookie_of(&r)), "cors")).await.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn the_page_is_its_own_and_not_kept() {
        let s = served();
        let r = send(&s, HttpRequest::get("/signin?next=%2Fcrates").body(Body::empty()).unwrap()).await;
        assert_eq!(r.status(), StatusCode::OK);
        let csp = header(&r, "content-security-policy").to_string();
        assert!(csp.contains("default-src 'none'") && csp.contains("frame-ancestors 'none'") && csp.contains("form-action 'self'"), "{csp}");
        assert_eq!(header(&r, "cache-control"), "no-store");
        assert_eq!(header(&r, "x-frame-options"), "DENY");
        let html = body(r).await;
        assert!(html.contains(r#"name="next" value="/crates""#));
        assert!(html.contains(r#"autocomplete="current-password""#));
        let nonce = csp.split("'nonce-").nth(1).unwrap().split('\'').next().unwrap();
        assert!(html.contains(&format!(r#"<script nonce="{nonce}">"#)));
        let named = send(&s, sign_in_form("<b>me</b>", "x", "203.0.113.5")).await;
        assert!(body(named).await.contains("&lt;b&gt;me&lt;/b&gt;"), "what was typed is shown as text");
    }
}

/// From `serve::ldap`'s tests.
#[cfg(test)]
mod ldap {
    use std::collections::BTreeMap;

    use crate::serve::ldap::Config;
    use crate::serve::ldap::testing::config;

    #[tokio::test]
    async fn a_password_is_asked_again_by_the_name_the_directory_knows_and_holds_for_that_account_alone() {
        use axum::body::Body;
        use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, header};
        use tower::ServiceExt;
        let d = std::env::temp_dir().join(format!("cratarr-ldap-mapped-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let pw = d.join("bind");
        std::fs::write(&pw, "x").unwrap();
        let s = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        s.users.set("me", "pw").unwrap();
        s.users.from_directory("rob", false, None).unwrap();
        s.users.set("robin", "own").unwrap();
        // Nothing listens on port 1, so a sign-in the directory took stands in for it: robin's, who is the account rob.
        let accounts = BTreeMap::from([("robin".to_string(), "rob".to_string())]);
        s.directory.set_config(Config { url: "ldap://127.0.0.1:1".into(), bind_password_file: pw.display().to_string(), accounts, ..config() }).unwrap();
        let now = crate::serve::now();
        s.directory.remember("robin", "secret", now).await;
        assert_eq!(s.sign_in("robin", "secret").await.as_deref(), Some("rob"), "the case");

        // Account → App password, signed in as the account, which the directory knows by the other name.
        let cookie = |user: &str| format!("{}={}", crate::serve::signin::COOKIE, s.sessions.create(user, "d", now).unwrap().0);
        let rob = cookie("rob");
        let set = |current: &str| {
            let body = serde_json::json!({ "current": current, "password": "for-apps" }).to_string();
            Request::put("/api/me/subsonic-password").header("content-type", "application/json").header("cookie", rob.as_str()).body(Body::from(body)).unwrap()
        };
        let routes = crate::serve::routes::router(s.clone()).layer(axum::middleware::from_fn_with_state(s.clone(), crate::serve::access::require));
        assert_eq!(routes.clone().oneshot(set("wrong")).await.unwrap().status(), StatusCode::FORBIDDEN);
        assert_eq!(routes.oneshot(set("secret")).await.unwrap().status(), StatusCode::NO_CONTENT);
        assert!(s.users.check(Some("rob"), None, None, Some("for-apps")).is_ok(), "his apps sign in with it");

        // The account named as the directory's name is another account: what signs in as rob is no password of its own.
        let robin = {
            let mut h = HeaderMap::new();
            h.insert(header::COOKIE, HeaderValue::from_str(&cookie("robin")).unwrap());
            h
        };
        assert_eq!(crate::serve::access::reauthenticate(&s, &robin, "own").await.ok().as_deref(), Some("robin"));
        assert!(crate::serve::access::reauthenticate(&s, &robin, "secret").await.is_err());
        std::fs::remove_dir_all(&d).ok();
    }
}

/// From `serve`'s tests.
#[cfg(test)]
mod serve {
    use axum::http::HeaderMap;

    use crate::serve::testing::*;
    use crate::serve::{ids, routes};
    use crate::userdata::Kind as Item;

    /// An app's saved list can hold an old id too: a heart or a rating left on it sits on an id no list shows, and once followed
    /// nothing moves it.
    #[tokio::test]
    async fn a_heart_and_a_rating_on_a_taken_id_land_on_the_track_that_took_it() {
        let (roots, cat, serve) = on_two_folders("star-taken");
        let one = [("A/01 One.flac", "One")];
        add(&cat, &roots[0], 0, &one);
        add(&cat, &roots[1], 1, &one);
        serve.refresh().unwrap();
        let stays = ids::song(cat.track_of(0, "A/01 One.flac").unwrap());
        let old = ids::song(*cat.moved_tracks().unwrap().keys().next().expect("the second copy's id was taken"));
        serve.users.set("me", "a password").unwrap();
        let me = serve.users.get("me").unwrap();
        for (method, query) in [("star", format!("id={old}")), ("setRating", format!("id={old}&rating=4"))] {
            let said = routes::handle(&serve, &me, method, &routes::Params::parse(Some(&query), &[]), &HeaderMap::new()).await;
            assert!(said.is_ok(), "{method}");
        }
        let a = serve.userdata.get("me", Item::Song, &[stays.clone()]).unwrap().remove(&stays).unwrap_or_default();
        assert_eq!((a.starred_at.is_some(), a.rating), (true, 4));
        roots.iter().for_each(|r| drop(std::fs::remove_dir_all(r)));
    }
}

/// From `serve::access`'s tests: each sends its requests through the whole server's routes (`serve::routes`).
#[cfg(test)]
mod access {
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use axum::http::StatusCode;
    use axum::routing::get;
    use serde_json::json;
    use tower::ServiceExt;

    use crate::catalogue::Rules;
    use crate::serve::access::testing::served;
    use crate::serve::access::{HOME_ASSISTANT, require, sign_with};
    use crate::serve::signin::COOKIE;
    use crate::serve::{Serve, now};

    /// The whole server's routes behind the requirement, as `main` has them.
    fn app(s: &Arc<Serve>) -> Router {
        crate::serve::routes::router(s.clone())
            .route("/api/library", get(|| async { "the library" }))
            .route("/", get(|| async { "the page" }))
            .layer(axum::middleware::from_fn_with_state(s.clone(), require))
    }

    async fn status(s: &Arc<Serve>, req: HttpRequest<Body>) -> (StatusCode, Option<String>) {
        let r = app(s).oneshot(req).await.unwrap();
        (r.status(), r.headers().get("location").map(|v| v.to_str().unwrap().to_string()))
    }

    fn get_(uri: &str) -> HttpRequest<Body> {
        HttpRequest::get(uri).body(Body::empty()).unwrap()
    }

    /// A JSON request, and its answer's status and body.
    async fn send_json(s: &Arc<Serve>, method: &str, uri: &str, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
        let req = HttpRequest::builder().method(method).uri(uri).header("content-type", "application/json").body(Body::from(body.to_string())).unwrap();
        let r = app(s).oneshot(req).await.unwrap();
        let status = r.status();
        let bytes = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
    }

    #[tokio::test]
    async fn with_signing_in_off_one_still_signed_in_is_told_who_they_are() {
        let s = served(false);
        s.users.from_directory("ann", false, Some(" Ann Example ")).unwrap();
        let (token, _) = s.sessions.create("ann", "d", now()).unwrap();
        let as_ann = |uri: &str| HttpRequest::get(uri).header("cookie", format!("{COOKIE}={token}")).body(Body::empty()).unwrap();
        let me = app(&s).oneshot(as_ann("/api/me")).await.unwrap();
        assert_eq!(me.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(me.into_body(), 1 << 16).await.unwrap()).unwrap();
        assert_eq!((body["name"].as_str(), body["admin"].as_bool()), (Some("ann"), Some(false)), "ann, no administrator");
        assert_eq!(body["display_name"].as_str(), Some("Ann Example"), "the name the directory gives her, trimmed");
        assert_eq!(status(&s, as_ann("/api/users")).await.0, StatusCode::FORBIDDEN, "as the server treats her");
        // No one signed in: the one user, as before.
        assert_eq!(status(&s, HttpRequest::get("/api/me").body(Body::empty()).unwrap()).await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn the_users_panel_makes_and_keeps_cratarr_accounts_and_says_why_not() {
        // Signing in off: the owner ("me") administers.
        let s = served(false);
        let (status, ann) = send_json(&s, "POST", "/api/users", json!({ "name": "ann", "password": "pw", "admin": false })).await;
        assert_eq!((status, ann["name"].as_str(), ann["directory"].as_bool(), ann["admin"].as_bool()), (StatusCode::OK, Some("ann"), Some(false), Some(false)));
        let (status, taken) = send_json(&s, "POST", "/api/users", json!({ "name": "Ann", "password": "pw" })).await;
        assert_eq!((status, taken["error"].as_str()), (StatusCode::CONFLICT, Some("That name is taken.")));
        assert_eq!(send_json(&s, "POST", "/api/users", json!({ "name": "a/b", "password": "pw" })).await.0, StatusCode::BAD_REQUEST);
        let (status, own) = send_json(&s, "PUT", "/api/users/me/role", json!({ "admin": false })).await;
        assert_eq!((status, own["error"].as_str()), (StatusCode::CONFLICT, Some("You cannot change your own role.")));
        assert_eq!(send_json(&s, "PUT", "/api/users/ann/password", json!({ "password": "new" })).await.0, StatusCode::NO_CONTENT);
        assert_eq!(send_json(&s, "PUT", "/api/users/ann/active", json!({ "active": false })).await.0, StatusCode::NO_CONTENT);
        assert!(s.users.sign_in("ann", "new").is_none(), "removed");
        assert_eq!(send_json(&s, "PUT", "/api/users/nobody/active", json!({ "active": true })).await.0, StatusCode::NOT_FOUND);
        // One's own password: the current one asked.
        let (status, wrong) = send_json(&s, "PUT", "/api/me/password", json!({ "current": "nope", "password": "x" })).await;
        assert_eq!((status, wrong["error"].as_str()), (StatusCode::FORBIDDEN, Some("The current password is wrong.")));
        assert_eq!(send_json(&s, "PUT", "/api/me/password", json!({ "current": "pw", "password": "x" })).await.0, StatusCode::NO_CONTENT);
        assert!(s.users.sign_in("me", "x").is_some());
        // A Cratarr account's app password is its password.
        assert_eq!(send_json(&s, "PUT", "/api/me/subsonic-password", json!({ "current": "x", "password": "y" })).await.0, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn a_password_takes_a_sign_in_never_a_key_and_an_app_password_the_password_too() {
        let s = served(true);
        // A directory's administrator, with a key that controls for her, as Home Assistant's does. No directory here, so
        // what she signs in with is this.
        s.users.from_directory("ann", true, None).unwrap();
        s.users.set_app_password("ann", "old").unwrap();
        let (token, _) = s.sessions.create("ann", "d", now()).unwrap();
        let (hers, _) = s.sessions.new_key("Home Assistant", Some("ann"), "control", None, now()).unwrap();
        let by_session = ("cookie", format!("{COOKIE}={token}"));
        let by_key = ("authorization", format!("Bearer {hers}"));
        let send = |by: &(&str, String), method: &str, uri: &str, body: serde_json::Value| {
            HttpRequest::builder().method(method).uri(uri).header("content-type", "application/json").header(by.0, by.1.as_str()).body(Body::from(body.to_string())).unwrap()
        };
        let set = |by: &(&str, String), current: &str, password: &str| send(by, "PUT", "/api/me/subsonic-password", json!({ "current": current, "password": password }));
        assert_eq!(status(&s, set(&by_key, "old", "new")).await.0, StatusCode::FORBIDDEN, "a key, even with the password");
        assert_eq!(status(&s, set(&by_session, "wrong", "new")).await.0, StatusCode::FORBIDDEN, "a page left open");
        assert!(s.users.sign_in("ann", "old").is_some(), "neither changed it");
        assert_eq!(status(&s, set(&by_session, "old", "new")).await.0, StatusCode::NO_CONTENT);
        assert!(s.users.sign_in("ann", "new").is_some());

        // Cratarr's own account, by the key of its owner, who knows the password.
        let (mine, _) = s.sessions.new_key("A script", Some("me"), "control", None, now()).unwrap();
        let by_mine = ("authorization", format!("Bearer {mine}"));
        assert_eq!(status(&s, send(&by_mine, "PUT", "/api/me/password", json!({ "current": "pw", "password": "x" }))).await.0, StatusCode::FORBIDDEN);
        assert_eq!(status(&s, send(&by_mine, "POST", "/api/subsonic/password", json!({ "username": "me", "current": "pw", "password": "x" }))).await.0, StatusCode::FORBIDDEN);
        assert!(s.users.sign_in("me", "pw").is_some(), "the other road to the same password");

        // An account with no password yet gives none as its current one: the fence above stands on this road too.
        s.users.from_directory("bo", false, None).unwrap();
        let (token, _) = s.sessions.create("bo", "d", now()).unwrap();
        let by_bo = ("cookie", format!("{COOKIE}={token}"));
        assert_eq!(status(&s, send(&by_bo, "POST", "/api/subsonic/password", json!({ "username": "bo", "current": "", "password": "x" }))).await.0, StatusCode::BAD_REQUEST);
        assert!(s.users.get("bo").unwrap().password.is_empty());
    }

    #[tokio::test]
    async fn a_refusal_is_in_the_diagnostics_log_with_why_and_without_what_it_carried() {
        use tracing_subscriber::layer::SubscriberExt;
        let s = served(true);
        let (layer, pending) = crate::diagnostics::layer();
        let log = crate::diagnostics::Diagnostics::start(None, pending).unwrap();
        let _logging = crate::diagnostics::log_here(tracing_subscriber::registry().with(layer));
        let linked = HttpRequest::get("/api/library?exp=1&sig=abc").header("x-forwarded-for", "84.1.2.3").body(Body::empty()).unwrap();
        assert_eq!(status(&s, linked).await.0, StatusCode::UNAUTHORIZED);
        let keyed = HttpRequest::get("/api/library").header("authorization", "Bearer cratarr_0000").body(Body::empty()).unwrap();
        assert_eq!(status(&s, keyed).await.0, StatusCode::UNAUTHORIZED);
        log.flush();
        let rows = log.query(&crate::diagnostics::Query::default()).unwrap();
        let seen: Vec<_> = rows.iter().map(|r| (r.what.as_str(), r.fields["path"].as_str().unwrap(), r.fields["reason"].as_str().unwrap(), r.fields["status"].as_u64().unwrap())).collect();
        assert_eq!(seen, [("request refused", "/api/library", "key not valid", 401), ("request refused", "/api/library", "link not valid", 401)]);
        assert_eq!((rows[0].fields["client"].as_str(), rows[1].fields["client"].as_str()), (Some("unknown"), Some("84.1.2.3")));
        assert!(rows.iter().all(|r| !r.fields.to_string().contains("abc") && !r.fields.to_string().contains("cratarr_0000")), "neither the signature nor the key");
    }

    #[tokio::test]
    async fn an_account_goes_as_far_as_it_may_and_a_key_no_further_than_its_scope() {
        let s = served(true);
        s.users.from_directory("ann", false, None).unwrap();
        let signed_in = |user: &str, uri: &str| {
            let (token, _) = s.sessions.create(user, "d", now()).unwrap();
            HttpRequest::get(uri).header("cookie", format!("{COOKIE}={token}")).body(Body::empty()).unwrap()
        };
        for admins in ["/api/auth/keys", "/api/subsonic/import/sources", "/api/subsonic/accounts", "/api/users"] {
            assert_eq!(status(&s, signed_in("me", admins)).await.0, StatusCode::OK, "{admins}");
            assert_eq!(status(&s, signed_in("ann", admins)).await.0, StatusCode::FORBIDDEN, "{admins}");
        }
        let me = app(&s).oneshot(signed_in("ann", "/api/me")).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(me.into_body(), 1 << 16).await.unwrap()).unwrap();
        assert_eq!((body["name"].as_str(), body["admin"].as_bool()), (Some("ann"), Some(false)));
        assert!(body["display_name"].is_null(), "none given: null");

        let keyed = |key: &str, method: &str, uri: &str| HttpRequest::builder().method(method).uri(uri).header("authorization", format!("Bearer {key}")).body(Body::empty()).unwrap();
        let (reads, _) = s.sessions.new_key("Dashboard", Some("me"), "read", None, now()).unwrap();
        assert_eq!(status(&s, keyed(&reads, "GET", "/api/library")).await.0, StatusCode::OK);
        assert_eq!(status(&s, keyed(&reads, "POST", "/api/library")).await.0, StatusCode::FORBIDDEN, "it only reads");
        // The MCP server (0.13) is asked by POST and only reads: past the check (no route here, so 404).
        assert_eq!(status(&s, keyed(&reads, "POST", "/mcp")).await.0, StatusCode::NOT_FOUND, "a reading key may ask the MCP server");
        assert_eq!(status(&s, keyed(&reads, "POST", "/mcpx")).await.0, StatusCode::FORBIDDEN, "nothing else");
        let (controls, _) = s.sessions.new_key("Home Assistant", Some("me"), "control", Some(HOME_ASSISTANT), now()).unwrap();
        assert_eq!(status(&s, keyed(&controls, "GET", "/api/auth/keys")).await.0, StatusCode::FORBIDDEN, "an administrator's key is no administrator");
        // One who left the directory's group is no one.
        s.users.deactivate("ann").unwrap();
        assert_eq!(status(&s, signed_in("ann", "/api/library")).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn while_it_is_off_or_there_is_no_account_everything_passes() {
        let s = served(false);
        assert_eq!(status(&s, get_("/api/library")).await.0, StatusCode::OK);
        // A key sent while it is off still shows as used: the check before switching on.
        let (key, made) = s.sessions.new_key("Home Assistant", None, "control", Some(HOME_ASSISTANT), now()).unwrap();
        let keyed = HttpRequest::get("/api/library").header("authorization", format!("Bearer {key}")).body(Body::empty()).unwrap();
        assert_eq!(status(&s, keyed).await.0, StatusCode::OK);
        assert!(s.sessions.keys().unwrap().iter().find(|k| k.id == made.id).unwrap().last_used.is_some());
        // Not required, still heard: one signed in is themselves, not the owner (decision 0051).
        assert_eq!(status(&s, get_("/api/auth/keys")).await.0, StatusCode::OK, "anyone else: the owner");
        s.users.from_directory("ann", false, None).unwrap();
        let (token, _) = s.sessions.create("ann", "d", now()).unwrap();
        let ann = HttpRequest::get("/api/auth/keys").header("cookie", format!("{COOKIE}={token}")).body(Body::empty()).unwrap();
        assert_eq!(status(&s, ann).await.0, StatusCode::FORBIDDEN);
        let fresh = Serve::open(None, Rules::default(), &|_| None).unwrap();
        fresh.sessions.set_required(true).unwrap();
        assert_eq!(status(&fresh, get_("/api/library")).await.0, StatusCode::OK, "no account to sign in with yet");
    }

    #[tokio::test]
    async fn required_only_a_session_a_key_or_a_signed_link_pass() {
        let s = served(true);
        assert_eq!(status(&s, get_("/api/library")).await.0, StatusCode::UNAUTHORIZED);
        let page = HttpRequest::get("/?x=1").header("sec-fetch-mode", "navigate").body(Body::empty()).unwrap();
        assert_eq!(status(&s, page).await, (StatusCode::SEE_OTHER, Some("/signin?next=%2F%3Fx%3D1".into())));
        for open in ["/api/health", "/signin", "/icons/icon-192.png", "/rest/ping.view"] {
            assert_ne!(status(&s, get_(open)).await.0, StatusCode::UNAUTHORIZED, "{open}");
        }

        let (token, _) = s.sessions.create("me", "d", now()).unwrap();
        let with_session = HttpRequest::get("/api/library").header("cookie", format!("{COOKIE}={token}")).body(Body::empty()).unwrap();
        assert_eq!(status(&s, with_session).await.0, StatusCode::OK);

        let (key, made) = s.sessions.new_key("Home Assistant", None, "control", Some(HOME_ASSISTANT), now()).unwrap();
        let with_key = |k: &str| HttpRequest::get("/api/library").header("authorization", format!("Bearer {k}")).body(Body::empty()).unwrap();
        assert_eq!(status(&s, with_key(&key)).await.0, StatusCode::OK);
        assert_eq!(status(&s, with_key("cratarr_0000")).await.0, StatusCode::UNAUTHORIZED);
        s.sessions.revoke_key(&made.id).unwrap();
        assert_eq!(status(&s, with_key(&key)).await.0, StatusCode::UNAUTHORIZED, "revoked");

        let secret = s.sessions.link_secret().unwrap();
        let link = sign_with(&secret, "http://h/api/library", now() + 60);
        let u = reqwest::Url::parse(&link).unwrap();
        let q = format!("{}?{}", u.path(), u.query().unwrap());
        assert_eq!(status(&s, get_(&q)).await.0, StatusCode::OK);
        let expired = sign_with(&secret, "http://h/api/library", now() - 1);
        let e = reqwest::Url::parse(&expired).unwrap();
        assert_eq!(status(&s, get_(&format!("{}?{}", e.path(), e.query().unwrap()))).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(status(&s, get_(&q.replace("/api/library", "/"))).await.0, StatusCode::UNAUTHORIZED, "another path");
    }

    #[tokio::test]
    async fn the_switch_and_a_new_key_need_the_password_again() {
        let s = served(false);
        let put = |pw: &str| {
            HttpRequest::put("/api/auth/required")
                .header("content-type", "application/json")
                .body(Body::from(format!(r#"{{"on":true,"password":"{pw}"}}"#)))
                .unwrap()
        };
        assert_eq!(status(&s, put("wrong")).await.0, StatusCode::FORBIDDEN);
        assert_eq!(s.sessions.required(), Some(false));
        assert_eq!(status(&s, put("pw")).await.0, StatusCode::OK);
        assert_eq!(s.sessions.required(), Some(true));

        let (token, _) = s.sessions.create("me", "d", now()).unwrap();
        let mint = |pw: &str| {
            HttpRequest::post("/api/auth/keys")
                .header("content-type", "application/json")
                .header("cookie", format!("{COOKIE}={token}"))
                .body(Body::from(format!(r#"{{"name":"Home Assistant","password":"{pw}","purpose":"home_assistant"}}"#)))
                .unwrap()
        };
        assert_eq!(status(&s, mint("wrong")).await.0, StatusCode::FORBIDDEN);
        let r = app(&s).oneshot(mint("pw")).await.unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(r.into_body(), 1 << 16).await.unwrap()).unwrap();
        let key = body["key"].as_str().unwrap();
        assert!(s.sessions.check_key(key, now()).is_some());
        let listed = app(&s).oneshot(HttpRequest::get("/api/auth/keys").header("cookie", format!("{COOKIE}={token}")).body(Body::empty()).unwrap()).await.unwrap();
        let text = String::from_utf8(axum::body::to_bytes(listed.into_body(), 1 << 16).await.unwrap().to_vec()).unwrap();
        assert!(text.contains("Home Assistant") && !text.contains(key), "listed without the key");
        let ha = app(&s)
            .oneshot(HttpRequest::get("/api/connections/home-assistant").header("cookie", format!("{COOKIE}={token}")).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let ha: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(ha.into_body(), 1 << 16).await.unwrap()).unwrap();
        assert_eq!(ha["keys"][0]["purpose"], "home_assistant", "shown with its connection");
        // A key for the MCP server (0.13): read only, whatever is asked.
        let mcp = HttpRequest::post("/api/auth/keys")
            .header("content-type", "application/json")
            .header("cookie", format!("{COOKIE}={token}"))
            .body(Body::from(r#"{"name":"Claude Code","password":"pw","purpose":"mcp","scope":"control"}"#))
            .unwrap();
        assert_eq!(status(&s, mcp).await.0, StatusCode::OK);
        let made = s.sessions.keys().unwrap().into_iter().find(|k| k.name == "Claude Code").unwrap();
        assert_eq!((made.purpose.as_deref(), made.scope.as_str()), (Some("mcp"), "read"));
    }

    #[tokio::test]
    async fn a_lost_sessions_file_leaves_signing_in_required() {
        let dir = std::env::temp_dir().join(format!("cratarr-lost-sessions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let s = Serve::open(Some(&dir), Rules::default(), &|_| None).unwrap();
        s.users.set("me", "pw").unwrap();
        let (key, _) = s.sessions.new_key("Home Assistant", None, "control", Some(HOME_ASSISTANT), now()).unwrap();
        drop(s);
        // The accounts stay: left out of a restore, or deleted because it was corrupt.
        for file in ["sessions.sqlite", "sessions.sqlite-wal", "sessions.sqlite-shm"] {
            let _ = std::fs::remove_file(dir.join(file));
        }
        let s = Serve::open(Some(&dir), Rules::default(), &|_| None).unwrap();
        assert!(s.sign_in_required());
        assert_eq!(status(&s, get_("/api/library")).await.0, StatusCode::UNAUTHORIZED, "not the owner, as anyone");
        let keyed = HttpRequest::get("/api/library").header("authorization", format!("Bearer {key}")).body(Body::empty()).unwrap();
        assert_eq!(status(&s, keyed).await.0, StatusCode::UNAUTHORIZED, "a key it no longer knows");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
