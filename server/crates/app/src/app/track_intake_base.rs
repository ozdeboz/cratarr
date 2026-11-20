//! The bulk queue's stand-ins, which Soulseek's tests share (moved from `track_intake`'s tests, which keep their names): slskd and
//! AcoustID answered within the test, the epoch clock pinned with tokio's, a tone. Test-only.

#[cfg(test)]
pub(crate) mod testing {
    use std::collections::{HashMap, VecDeque};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use anyhow::Result;
    use axum::http::{Method, StatusCode};
    use axum::response::{IntoResponse, Response};
    use serde_json::{Value, json};

    use crate::app::moment::now_secs as now;
    use crate::app::turn::Pinned;

    pub(crate) const T0: i64 = 1_800_000_000;

    /// The epoch clock pinned at t0 and moved on by the test (`moment::PINNED`), tokio's paused clock moved on as far with it.
    pub(crate) struct Clock(std::cell::Cell<i64>);

    impl Clock {
        pub(crate) fn at_t0() -> (Pinned, Self) {
            crate::app::moment::PINNED.store(T0, Ordering::Relaxed);
            (Pinned(T0), Clock(std::cell::Cell::new(0)))
        }

        pub(crate) async fn to(&self, secs: i64) {
            let by = secs - self.0.replace(secs);
            crate::app::moment::PINNED.store(T0 + secs, Ordering::Relaxed);
            tokio::time::advance(Duration::from_secs(by as u64)).await;
        }
    }

    /// Ten seconds of a tone as FLAC in `dir`, as the gate passes one (`track_fetch`'s old drive's file).
    pub(crate) fn tone(dir: &Path) -> PathBuf {
        let name = format!("intake-tone-{}", dir.file_name().unwrap().to_string_lossy());
        let w = crate::analysis::tests::wav(&name, 10.0, |t| 0.2 * (t * (440.0 + 200.0 * (t * 0.7).sin()) * std::f64::consts::TAU).sin() as f32);
        let out = dir.join("tone.flac");
        crate::convert::to_cd_flac(&w, &out).unwrap();
        std::fs::remove_file(&w).ok();
        out
    }

    /// slskd as the bulk queue and Soulseek meet it, answering within the test (`Slskd::within`). Its searches and batches answer as
    /// scripted (else 200 and 201), each search finding what `found` holds for its words; a batch taken lands its files in slskd's
    /// folder (each `file`), as slskd's would. All asked is noted with the moment (epoch s) it was asked. A search, a batch or a
    /// batch's read while its 429 holds, or anything that is not slskd's API as they ask it, is a violation, answered 418. Until
    /// `away_until` (epoch s) all it is asked is answered 503, slskd not up: 0.17's stand-in for a while slskd holds its askers off,
    /// as slskd has no pause of its own there (round 1.3's lane: a 429 is asked again in it).
    #[derive(Default)]
    pub(crate) struct FakeSlskd {
        pub(crate) away_until: i64,
        pub(crate) searches: Mutex<VecDeque<(u16, Option<&'static str>)>>,
        pub(crate) batches: Mutex<VecDeque<u16>>,
        pub(crate) found: HashMap<&'static str, Value>,
        pub(crate) folder: PathBuf,
        pub(crate) file: PathBuf,
        pub(crate) asked: Mutex<Vec<(String, String, i64, Value)>>,
        pub(crate) held: Mutex<i64>,
        pub(crate) violations: Mutex<Vec<String>>,
    }

    impl FakeSlskd {
        fn answer(&self, method: &Method, path: &str, body: &[u8]) -> Response {
            let now = now();
            let body: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
            self.asked.lock().unwrap().push((method.to_string(), path.to_string(), now, body.clone()));
            if now < self.away_until {
                return (StatusCode::SERVICE_UNAVAILABLE, "Service Unavailable").into_response();
            }
            let held = *self.held.lock().unwrap();
            let parts: Vec<&str> = path.trim_start_matches("/api/v0/").split('/').collect();
            match (method.as_str(), parts.as_slice()) {
                ("POST", ["searches"]) => {
                    if now < held {
                        self.violation(format!("a search at {now}, held until {held}"));
                    }
                    let (status, said) = self.searches.lock().unwrap().pop_front().unwrap_or((200, None));
                    let mut r = (StatusCode::from_u16(status).unwrap(), axum::Json(json!({ "id": body["id"], "searchText": body["searchText"] }))).into_response();
                    if let Some(said) = said {
                        *self.held.lock().unwrap() = now + said.parse::<i64>().unwrap();
                        r.headers_mut().insert(axum::http::header::RETRY_AFTER, said.parse().unwrap());
                    }
                    r
                }
                ("GET", ["searches", _]) => axum::Json(json!({ "isComplete": true })).into_response(),
                ("GET", ["searches", id, "responses"]) => {
                    let asked = self.asked.lock().unwrap();
                    let words = asked.iter().find(|(m, p, _, b)| m == "POST" && p.ends_with("/searches") && b["id"] == *id).map(|(.., b)| b["searchText"].clone());
                    axum::Json(words.and_then(|w| self.found.get(w.as_str()?).cloned()).unwrap_or_else(|| json!([]))).into_response()
                }
                ("DELETE", ["searches", _]) => StatusCode::NO_CONTENT.into_response(),
                ("POST", ["transfers", "downloads", "batches"]) => {
                    if now < held {
                        self.violation(format!("a batch at {now}, held until {held}"));
                    }
                    let status = self.batches.lock().unwrap().pop_front().unwrap_or(201);
                    if status >= 300 {
                        return (StatusCode::from_u16(status).unwrap(), "Service Unavailable").into_response();
                    }
                    let to = self.folder.join(body["options"]["destination"].as_str().unwrap_or_default());
                    std::fs::create_dir_all(&to).unwrap();
                    // Each file of the batch (a track, or a record's folder), as slskd lands it.
                    for f in body["files"].as_array().into_iter().flatten() {
                        std::fs::copy(&self.file, to.join(crate::slskd::base_name(f["filename"].as_str().unwrap_or_default()))).unwrap();
                    }
                    (StatusCode::CREATED, axum::Json(json!({ "batch": { "transfers": [] }, "failures": [] }))).into_response()
                }
                ("GET", ["transfers", "downloads", "batches", id]) => {
                    if now < held {
                        self.violation(format!("a batch read at {now}, held until {held}"));
                    }
                    let done = json!({ "id": format!("t-{id}"), "filename": "x", "state": "Completed, Succeeded", "percentComplete": 100.0 });
                    axum::Json(json!({ "transfers": [done] })).into_response()
                }
                ("DELETE", ["transfers", "downloads", _, _]) => StatusCode::NO_CONTENT.into_response(),
                _ => {
                    self.violation(format!("{method} {path}"));
                    StatusCode::IM_A_TEAPOT.into_response()
                }
            }
        }

        fn violation(&self, v: String) {
            self.violations.lock().unwrap().push(v);
        }

        /// The batches it was asked for: each one's user and file.
        pub(crate) fn batches_asked(&self) -> Vec<(String, String)> {
            let asked = self.asked.lock().unwrap();
            let batches = asked.iter().filter(|(m, p, ..)| m == "POST" && p.ends_with("/batches"));
            batches.map(|(.., b)| (b["username"].as_str().unwrap_or_default().to_string(), b["files"][0]["filename"].as_str().unwrap_or_default().to_string())).collect()
        }

        /// The searches it was asked for: each one's words, and when.
        pub(crate) fn searches_asked(&self) -> Vec<(String, i64)> {
            let asked = self.asked.lock().unwrap();
            let searches = asked.iter().filter(|(m, p, ..)| m == "POST" && p.ends_with("/searches"));
            searches.map(|(_, _, at, b)| (b["searchText"].as_str().unwrap_or_default().to_string(), *at)).collect()
        }
    }

    /// slskd at a test address, answered within the test by `fake`.
    pub(crate) fn slskd_at(url: &str, fake: &Arc<FakeSlskd>) -> crate::slskd::Slskd {
        let fake = fake.clone();
        let router = axum::Router::new().fallback(move |method: Method, uri: axum::http::Uri, body: axum::body::Bytes| {
            let fake = fake.clone();
            async move { fake.answer(&method, uri.path(), &body) }
        });
        crate::slskd::Slskd::within(url, "key", router)
    }

    /// AcoustID as the gate asks it, through `acoustid::lookup_in`'s send (TN-a1's seam): its limit (HTTP 429, code 14, the stand-in
    /// body) where scripted, else ok, knowing the file as each of "Tide Tables"' recordings; each lookup's moment noted (epoch s, and
    /// tokio's). A lookup whose `meta` is not `recordingids` or without the key, one under 333 ms after the one before, or one while
    /// its limit holds (its Retry-After where it says one, `said`, else its own default, a minute), is a violation.
    #[derive(Default)]
    pub(crate) struct FakeAcoustid {
        pub(crate) limits: Mutex<VecDeque<bool>>,
        pub(crate) said: Option<i64>,
        pub(crate) asked: Mutex<Vec<(i64, tokio::time::Instant)>>,
        pub(crate) held: Mutex<i64>,
        pub(crate) violations: Mutex<Vec<String>>,
    }

    impl FakeAcoustid {
        pub(crate) fn answer(&self, form: Vec<(&'static str, String)>) -> std::future::Ready<Result<crate::acoustid::Heard>> {
            let (now, at) = (now(), tokio::time::Instant::now());
            let field = |k: &str| form.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
            let mut violations = self.violations.lock().unwrap();
            if field("meta") != Some("recordingids") || field("client") != Some("key") {
                violations.push(format!("asked {form:?}"));
            }
            let mut asked = self.asked.lock().unwrap();
            if asked.last().is_some_and(|(_, last)| at - *last < Duration::from_millis(333)) {
                violations.push(format!("two lookups under 333 ms apart at {now}"));
            }
            let mut held = self.held.lock().unwrap();
            if now < *held {
                violations.push(format!("a lookup at {now}, held until {held}"));
            }
            asked.push((now, at));
            let mut headers = reqwest::header::HeaderMap::new();
            let (status, body) = if self.limits.lock().unwrap().pop_front().unwrap_or(false) {
                *held = now + self.said.unwrap_or(60);
                if let Some(said) = self.said {
                    headers.insert(reqwest::header::RETRY_AFTER, said.into());
                }
                (429, r#"{"status":"error","error":{"code":14,"message":"rate limit exceeded"}}"#.to_string())
            } else {
                let known = json!([{ "id": "rec-slack" }, { "id": "rec-neap" }, { "id": "rec-spring" }]);
                (200, json!({ "status": "ok", "results": [{ "score": 0.97, "recordings": known }] }).to_string())
            };
            std::future::ready(Ok(crate::acoustid::Heard { status: reqwest::StatusCode::from_u16(status).unwrap(), headers, body }))
        }
    }
}
