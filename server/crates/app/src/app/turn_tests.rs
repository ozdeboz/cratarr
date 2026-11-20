//! The turn's tests that reach the app's areas (its class over Soulseek's, the gate's and the list's errors), moved up from the
//! turn's module in the clients crate with the turn's placing.

use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::HeaderMap;

use super::turn::fakes::{self, headers, kind, print};
use super::turn::{ACOUSTID, Failure, GITHUB, MUSICBRAINZ, Pinned, Refused, Stuck, Turn, class};

/// A slskd at a local address whose batches answer `status`, Retry-After `said`, `body`; its own state `state`.
async fn slskd(status: u16, said: Option<&'static str>, body: serde_json::Value, state: serde_json::Value) -> crate::slskd::Slskd {
    use axum::response::IntoResponse;
    let code = axum::http::StatusCode::from_u16(status).unwrap();
    let app = axum::Router::new()
        .route(
            "/api/v0/transfers/downloads/batches",
            axum::routing::post(move || {
                let mut r = (code, axum::Json(body.clone())).into_response();
                if let Some(said) = said {
                    r.headers_mut().insert(axum::http::header::RETRY_AFTER, said.parse().unwrap());
                }
                std::future::ready(r)
            }),
        )
        .route("/api/v0/application", axum::routing::get(move || std::future::ready(axum::Json(state.clone()))));
    crate::slskd::Slskd::new(&fakes::serve(app).await, "key").unwrap()
}

/// slskd's error fetching a folder of one file from `peer-a`.
async fn slskd_says(s: &crate::slskd::Slskd) -> anyhow::Error {
    let one = crate::slskd::Offered { filename: "d\\01.flac".into(), size: 1, extension: None, bit_rate: None, bit_depth: None, sample_rate: None, length: None, is_locked: false };
    s.fetch_many(&crate::slskd::new_batch(), "peer-a", &[one], || true).await.unwrap_err()
}

/// MusicBrainz asked in a turn of its own (`asked_in`) through `http`: the error its asker meets.
async fn musicbrainz_says(http: &reqwest::Client, url: String) -> anyhow::Error {
    let turn = Turn::new(MUSICBRAINZ);
    match crate::app::musicbrainz::asked_in(&turn, http.get(url)).await {
        Ok(r) => r.error_for_status().unwrap_err().into(),
        Err(e) => e,
    }
}

/// AcoustID at a fake of its own answering `status` and `body`: the error its lookup meets.
async fn acoustid_says(status: u16, body: String) -> anyhow::Error {
    let code = axum::http::StatusCode::from_u16(status).unwrap();
    let app = axum::Router::new().route("/v2/lookup", axum::routing::post(move || std::future::ready((code, body.clone()))));
    let _fake = crate::acoustid::FakeAcoustid::at(format!("{}/v2/lookup", fakes::serve(app).await));
    crate::acoustid::lookup("key", &print()).await.unwrap_err()
}

/// TN.1: every service's failures, as each gives them, have their class: a user's, the file's, nothing there, away (no deadline),
/// paused until when (Retry-After where given, else the source's default), or stuck. Never a 429 or MusicBrainz's 503 away or a
/// user's; never a deadline before now, nor past a quarter hour but Bandcamp's day and GitHub's reset, each to its own cap.
#[tokio::test]
async fn every_failure_has_its_class() {
    use crate::app::switches::{BACKGROUND, MUSICBRAINZ as MUSICBRAINZ_ON, TEST_STATE, set_for_tests};
    let _one = TEST_STATE.lock().await;
    let pinned = Pinned::now();
    let t0 = pinned.0;
    let null = serde_json::Value::Null;
    let (on, off) = (serde_json::json!({ "server": { "isLoggedIn": true } }), serde_json::json!({ "server": { "isLoggedIn": false } }));
    let paused = |secs: i64| Failure::Paused { until: t0 + secs };
    let away = Failure::Away(String::new());
    let mut cases: Vec<(&str, anyhow::Error, Failure)> = Vec::new();

    // slskd.
    cases.push(("slskd 404", slskd_says(&slskd(404, None, null.clone(), on.clone()).await).await, Failure::User));
    let full = serde_json::json!({ "batch": null, "failures": [{ "message": "queue full" }] });
    cases.push(("slskd queue full", slskd_says(&slskd(200, None, full, on.clone()).await).await, Failure::User));
    cases.push(("slskd 500 signed in", slskd_says(&slskd(500, None, null.clone(), on.clone()).await).await, Failure::User));
    for status in [401, 403, 502, 503, 504] {
        cases.push(("slskd refusing or not up", slskd_says(&slskd(status, None, null.clone(), on.clone()).await).await, away.clone()));
    }
    cases.push(("slskd 500 not signed in", slskd_says(&slskd(500, None, null.clone(), off.clone()).await).await, away.clone()));
    let gone = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap();
    cases.push(("slskd not listening", slskd_says(&crate::slskd::Slskd::new(&format!("http://{gone}"), "key").unwrap()).await, away.clone()));
    // slskd's 429 is no limit answer: its lane asks again until slskd takes it (0.17's round 1.3), so it is no one's failure.

    // MusicBrainz, its 503s the captured one (`fakes::musicbrainz_busy`).
    let mb = axum::Router::new()
        .route("/ws/2/release/busy", axum::routing::get(|| async { fakes::musicbrainz_busy(Some(None)) }))
        .route("/ws/2/release/busy7", axum::routing::get(|| async { fakes::musicbrainz_busy(Some(Some("7"))) }))
        .route("/ws/2/release/global", axum::routing::get(|| async { fakes::musicbrainz_busy(None) }))
        .route("/ws/2/release/limit", axum::routing::get(|| async { axum::http::StatusCode::TOO_MANY_REQUESTS }))
        .route("/ws/2/release/gone", axum::routing::get(|| async { axum::http::StatusCode::NOT_FOUND }))
        .route("/ws/2/release/refused", axum::routing::get(|| async { axum::http::StatusCode::BAD_REQUEST }))
        .route("/ws/2/release/broken", axum::routing::get(|| async { axum::http::StatusCode::INTERNAL_SERVER_ERROR }))
        .route(
            "/ws/2/release/slow",
            axum::routing::get(|| async {
                tokio::time::sleep(Duration::from_secs(2)).await;
                "{}"
            }),
        );
    let at = fakes::serve(mb).await;
    let http = reqwest::Client::builder().user_agent(super::reach::USER_AGENT).timeout(Duration::from_millis(300)).build().unwrap();
    let mb = |path: &str| format!("{at}/ws/2/release/{path}");
    cases.push(("MusicBrainz 503", musicbrainz_says(&http, mb("busy")).await, paused(5)));
    cases.push(("MusicBrainz 503 Retry-After 7", musicbrainz_says(&http, mb("busy7")).await, paused(7)));
    cases.push(("MusicBrainz's global limiter as captured", musicbrainz_says(&http, mb("global")).await, paused(1)));
    cases.push(("MusicBrainz 429", musicbrainz_says(&http, mb("limit")).await, paused(5)));
    cases.push(("MusicBrainz 404", musicbrainz_says(&http, mb("gone")).await, Failure::NotFound));
    cases.push(("MusicBrainz 400", musicbrainz_says(&http, mb("refused")).await, Failure::NotFound));
    cases.push(("MusicBrainz 500", musicbrainz_says(&http, mb("broken")).await, away.clone()));
    cases.push(("MusicBrainz too slow", musicbrainz_says(&http, mb("slow")).await, away.clone()));
    let was = set_for_tests(MUSICBRAINZ_ON, false);
    cases.push(("MusicBrainz switched off", musicbrainz_says(&http, mb("gone")).await, away.clone()));
    set_for_tests(MUSICBRAINZ_ON, was);
    let was = set_for_tests(BACKGROUND, false);
    cases.push(("background reading paused", crate::app::musicbrainz::behind(musicbrainz_says(&http, mb("gone"))).await, away.clone()));
    set_for_tests(BACKGROUND, was);

    // AcoustID, its error bodies stand-ins until a real one is seen.
    let error = |code: u32, message: &str| format!(r#"{{"status":"error","error":{{"code":{code},"message":"{message}"}}}}"#);
    cases.push(("AcoustID 429 code 14", acoustid_says(429, error(14, "rate limit exceeded")).await, paused(60)));
    cases.push(("AcoustID code 4", acoustid_says(400, error(4, "invalid API key")).await, away.clone()));
    cases.push(("AcoustID 503 code 13", acoustid_says(503, error(13, "service is temporary unavailable")).await, away.clone()));
    cases.push(("AcoustID code 3", acoustid_says(400, error(3, "invalid fingerprint")).await, Failure::File));
    let slow = fakes::serve(axum::Router::new().route(
        "/v2/lookup",
        axum::routing::post(|| async {
            tokio::time::sleep(Duration::from_secs(2)).await;
            "{}"
        }),
    ))
    .await;
    let timed = crate::acoustid::lookup_in(&Turn::new(ACOUSTID), |form| crate::acoustid::posted(http.clone(), format!("{slow}/v2/lookup"), form), "key", &print()).await;
    cases.push(("AcoustID too slow", timed.unwrap_err(), away.clone()));

    // Discogs.
    let said = |v: &str| headers(&[("retry-after", v.to_string())]);
    cases.push(("Discogs 429", crate::app::discogs::too_many_said(&HeaderMap::new(), t0).1.into(), paused(60)));
    cases.push(("Discogs 429 Retry-After 30", crate::app::discogs::too_many_said(&said("30"), t0).1.into(), paused(30)));
    cases.push(("Discogs 404", reqwest::get(mb("gone")).await.unwrap().error_for_status().unwrap_err().into(), Failure::NotFound));

    // Bandcamp.
    cases.push(("Bandcamp's bot check", crate::bandcamp::NotRead.into(), paused(86_400)));
    for status in [StatusCode::TOO_MANY_REQUESTS, StatusCode::SERVICE_UNAVAILABLE] {
        cases.push(("Bandcamp busy", crate::bandcamp::refusal(status, &HeaderMap::new(), t0), paused(60)));
        cases.push(("Bandcamp busy Retry-After 90", crate::bandcamp::refusal(status, &said("90"), t0), paused(90)));
    }
    cases.push(("Bandcamp 404", crate::bandcamp::refusal(StatusCode::NOT_FOUND, &HeaderMap::new(), t0), Failure::NotFound));

    // GitHub.
    let spent = |reset: i64| headers(&[("x-ratelimit-remaining", "0".into()), ("x-ratelimit-reset", reset.to_string())]);
    let github = |status: StatusCode, h: HeaderMap| -> anyhow::Error {
        match Turn::new(GITHUB).heard_answer(status, &h) {
            Err(p) => p.into(),
            Ok(()) => Refused::new("github", status, format!("GitHub answered {status}")).into(),
        }
    };
    cases.push(("GitHub's limit spent", github(StatusCode::FORBIDDEN, spent(t0 + 1800)), paused(1800)));
    cases.push(("GitHub's limit spent, its reset far off", github(StatusCode::FORBIDDEN, spent(t0 + 7200)), paused(3600)));
    cases.push(("GitHub 403", github(StatusCode::FORBIDDEN, headers(&[("x-ratelimit-remaining", "12".into())])), away.clone()));
    cases.push(("GitHub 429", github(StatusCode::TOO_MANY_REQUESTS, HeaderMap::new()), paused(60)));

    // The gate.
    let p = |secs: f64, damaged: u32, pop: bool| crate::acoustid::Print {
        fingerprint: String::new(),
        duration_secs: secs,
        damaged,
        flaws: crate::analysis::flaws::Findings { pop_at_end: pop, ..Default::default() },
    };
    use crate::app::track_fetch::{known_as, refused_for};
    cases.push(("frames that would not decode", refused_for(&p(240.0, 3, false), Some(240_000)).unwrap_err(), Failure::File));
    cases.push(("a length 4 s off", refused_for(&p(236.0, 0, false), Some(240_000)).unwrap_err(), Failure::File));
    cases.push(("a flaw", refused_for(&p(240.0, 0, true), Some(240_000)).unwrap_err(), Failure::File));
    let other = crate::acoustid::Found { score: 0.97, recordings: vec![crate::acoustid::Recording { id: "another".into() }] };
    cases.push(("another recording", known_as(&[other], "0b6f4bd4-2f0c-4b7e-9a51-6c1c0d8f7a11").unwrap_err(), Failure::File));

    // Soulseek.
    let cat = Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap());
    let record = cat.record_for_tests("Quiet Lanes", 0);
    let nothing = |_: crate::catalogue::records::Ref| std::future::ready(Ok::<Option<crate::app::references::Read>, anyhow::Error>(None));
    let why = crate::app::references::tracklist(&cat, record, nothing).await.unwrap().unwrap_err();
    assert_eq!(why, "No list to judge by");
    cases.push(("no list to judge by", Stuck(why).into(), Failure::Stuck));
    cases.push(("no library to put it in", crate::app::soulseek::taking(&crate::settings::Saved::default()).unwrap_err(), Failure::Stuck));

    let mut wrong = Vec::new();
    for (name, e, want) in &cases {
        let got = class(e);
        if kind(got.clone()) != kind(want.clone()) {
            wrong.push(format!("{name}: {got:?}, not {want:?} ({e:#})"));
        }
        if let Failure::Paused { until } = got {
            let most = if name.starts_with("Bandcamp's bot check") { 86_400 } else if name.starts_with("GitHub's limit") { 3600 } else { 900 };
            assert!(until >= t0 && until <= t0 + most, "{name}: {until}");
        }
        if name.contains("429") || name.starts_with("MusicBrainz 503") {
            assert!(matches!(got, Failure::Paused { .. }), "{name}: never away or a user's");
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}
