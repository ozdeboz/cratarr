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
