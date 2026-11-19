//! Tests moved up into this crate from the crates below, by their module (SPLIT-PLAN 3.2): each needs this crate.

/// From `crate::app::retention`'s tests.
#[cfg(test)]
mod retention {
    use std::collections::BTreeSet;

    use crate::app::retention::*;
    use crate::catalogue::Catalogue;
    use crate::serve::ids;
    use crate::app::retention::testing::*;

    /// RT01: Cratarr's album forgotten answers "no longer known" (410), by its tombstone; one never known, as before.
    #[test]
    fn a_forgotten_record_answers_no_longer_known() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let gone = unseen(&cat, "Undertow");
        assert_eq!(cat.forget_quiet(NOW, &BTreeSet::new()).unwrap(), [gone]);
        let answer = unknown_album(&cat, &ids::record(gone)).expect("no longer known");
        assert_eq!(answer.to_string(), "This album is no longer known: nobody had opened it or seen it in a list for six months.");
        let status = axum::response::IntoResponse::into_response(crate::api::ApiError::from(answer)).status();
        assert_eq!(status, axum::http::StatusCode::GONE);
        assert!(unknown_album(&cat, &ids::record(gone + 1)).is_none(), "never known: answered as before");
    }
}

/// From `crate::serve::access`'s tests.
#[cfg(test)]
mod access {
    use axum::Router;
    use axum::routing::get;

    use crate::serve::access::*;
    use crate::serve::now;
    use crate::serve::signin::COOKIE;
    use crate::serve::who::current;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use tower::ServiceExt;
    use crate::serve::access::testing::*;

    #[tokio::test]
    async fn a_request_is_from_home_as_the_client_after_the_proxies_screen_is() {
        let s = served(true);
        let (token, _) = s.sessions.create("me", "d", now()).unwrap();
        let app = || {
            Router::new()
                .route("/api/home", get(|| async { format!("{:?}", current().map(|w| w.home)) }))
                .layer(axum::middleware::from_fn_with_state(s.clone(), require))
                .layer(axum::middleware::from_fn(crate::api::request_ids))
        };
        let home = |peer: &str, forwarded: Option<&str>| {
            let mut b = HttpRequest::get("/api/home").header("cookie", format!("{COOKIE}={token}"));
            if let Some(f) = forwarded {
                b = b.header("x-forwarded-for", f);
            }
            let mut req = b.body(Body::empty()).unwrap();
            req.extensions_mut().insert(axum::extract::ConnectInfo(std::net::SocketAddr::new(peer.parse().unwrap(), 50000)));
            let r = app().oneshot(req);
            async move { String::from_utf8(axum::body::to_bytes(r.await.unwrap().into_body(), 64).await.unwrap().to_vec()).unwrap() }
        };
        assert_eq!(home("192.168.1.20", None).await, "Some(true)");
        assert_eq!(home("203.0.113.9", None).await, "Some(false)");
        assert_eq!(home("203.0.113.9", Some("192.168.1.20")).await, "Some(false)", "a peer not believed names no one");
        assert_eq!(home("172.19.0.3", Some("203.0.113.9")).await, "Some(false)", "a local proxy names the client");
        assert_eq!(home("172.19.0.3", Some("192.168.1.20")).await, "Some(true)");
        assert_eq!(home("2001:db8:7::5", None).await, "Some(false)");
        crate::serve::proxy::set_home(vec!["2001:db8:7::/48".parse().unwrap()]);
        assert_eq!(home("2001:db8:7::5", None).await, "Some(true)", "a home network named");
        crate::serve::proxy::set_home(Vec::new());
    }
}

/// From `crate::serve::listener`'s tests.
#[cfg(test)]
mod listener {
    use crate::serve::listener::*;
    use crate::serve::listener::tests::*;
    use crate::source::cratarr::KIND;

    #[test]
    fn the_waveform_is_the_file_that_plays() {
        let t = two("wave");
        for (folder, rel, level) in [(0, FLAC, 1u8), (1, MP3, 2)] {
            let a = crate::analysis::Analysis {
                version: crate::analysis::VERSION,
                duration_secs: 200.0,
                loudness_lufs: -14.0,
                track_gain_db: -4.0,
                track_peak: 0.9,
                blocks: crate::analysis::Loudness(vec![(1, 40)]),
                bpm: Some(120.0),
                bpm_confidence: 0.9,
                waveform: vec![level; crate::analysis::WAVEFORM_BINS],
                complete: Some(true),
                beats: None,
                rhythm: None,
            };
            let (_, size, mtime) = t.cat.file_to_analyse(folder, rel).unwrap();
            t.cat.store_analysis(folder, rel, size, mtime, &a).unwrap();
        }
        let wave = |who: &Listener, asked: Option<&str>| crate::api::waveform_for(&t.cat, who, KIND.id, &t.song, asked).unwrap();
        let (row, other) = (t.who(None, &[]), t.who(Some("ann"), &[0]));
        let (tag_row, bins_row) = wave(&row, None);
        let (tag_other, bins_other) = wave(&other, None);
        assert_eq!((bins_row.unwrap()[0], bins_other.unwrap()[0]), (1, 2));
        assert_ne!(tag_row, tag_other, "a browser that kept one file's must not be told it is still good for the other");
        assert_eq!(wave(&other, Some(&tag_other)).1, None, "not modified");
        assert!(wave(&other, Some(&tag_row)).1.is_some());
    }
}
