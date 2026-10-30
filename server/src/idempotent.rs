//! A change sent again gives its first answer (0.9.15, the page's offline changes: the owner, 2026-10-28, "the best PWA
//! offline support possible"). A page out of reach keeps its changes (a heart, a rating, a playlist's tracks) and sends
//! them when Cratarr answers again; one whose answer got lost on the way is sent once more. With an `Idempotency-Key`
//! header (as Stripe's and the IETF draft's), Cratarr makes each change once: the same key again, from the same account,
//! for the same method and path, gets the first answer back (`Idempotent-Replayed: true`) without doing it again.
//! Kept a day in memory, and since 0.12 (decision 0087: an outbox may send a change days later, across a restart) 30
//! days in the user data too, where the answer is small enough; a change still being made is known in memory only.

use axum::body::{Body, HttpBody};
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::userdata::UserData;
use crate::userdata::offline::KeptAnswer;

/// How long an answer is kept for a key sent again.
const KEEP: Duration = Duration::from_secs(24 * 3600);
/// The most keys kept at once; past it the oldest go.
const MOST: usize = 10_000;
/// An answer kept only up to this size (a change's answers are small); a bigger one is not kept, and the key with it.
const BIGGEST: u64 = 256 << 10;
/// An answer kept in the user data (30 days) only up to this size; a bigger one is kept in memory alone.
const BIGGEST_KEPT: usize = 64 << 10;

/// What a key gave: still being made, or its answer.
enum Kept {
    Making { at: Instant, asked: String },
    Done { at: Instant, asked: String, status: StatusCode, kind: Option<HeaderValue>, body: axum::body::Bytes },
}

impl Kept {
    fn at(&self) -> Instant {
        match self {
            Kept::Making { at, .. } | Kept::Done { at, .. } => *at,
        }
    }

    fn asked(&self) -> &str {
        match self {
            Kept::Making { asked, .. } | Kept::Done { asked, .. } => asked,
        }
    }
}

static KEYS: LazyLock<Mutex<HashMap<(String, String), Kept>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// A key as sent: up to 255 visible characters (a UUID, as the page makes them).
fn key_of(request: &Request) -> Option<String> {
    let v = request.headers().get("idempotency-key")?.to_str().ok()?.trim();
    ((1..=255).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_graphic())).then(|| v.to_string())
}

fn changes(method: &Method) -> bool {
    matches!(*method, Method::POST | Method::PUT | Method::PATCH | Method::DELETE)
}

/// A kept answer as the first time, saying it is one.
fn replayed(status: StatusCode, kind: Option<&HeaderValue>, body: axum::body::Bytes) -> Response {
    let mut r = Response::new(Body::from(body));
    *r.status_mut() = status;
    if let Some(kind) = kind {
        r.headers_mut().insert(header::CONTENT_TYPE, kind.clone());
    }
    r.headers_mut().insert("idempotent-replayed", HeaderValue::from_static("true"));
    r
}

fn sent_with_another() -> Response {
    (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(json!({ "error": "This key was sent with another change." }))).into_response()
}

/// The layer, inside the access check (who asks is known there). A change without a key goes as always. `kept`: the
/// user data the answers are kept in for 30 days (decision 0087); none: in memory only.
pub async fn once(State(kept): State<Option<Arc<UserData>>>, request: Request, next: Next) -> Response {
    if !changes(request.method()) {
        return next.run(request).await;
    }
    let Some(key) = key_of(&request) else {
        return next.run(request).await;
    };
    let whose = crate::serve::who::current().map(|w| w.user).unwrap_or_default();
    let asked = format!("{} {}", request.method(), request.uri().path_and_query().map_or("/", |p| p.as_str()));
    let id = (whose, key);
    let now = Instant::now();
    {
        let mut keys = KEYS.lock().unwrap();
        keys.retain(|_, k| now.duration_since(k.at()) < KEEP);
        if let Some(k) = keys.get(&id) {
            if k.asked() != asked {
                return sent_with_another();
            }
            return match k {
                Kept::Making { .. } => {
                    let mut r = (StatusCode::CONFLICT, axum::Json(json!({ "error": "This change is still being made." }))).into_response();
                    r.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(1));
                    r
                }
                Kept::Done { status, kind, body, .. } => replayed(*status, kind.as_ref(), body.clone()),
            };
        }
        if keys.len() >= MOST {
            if let Some(oldest) = keys.iter().min_by_key(|(_, k)| k.at()).map(|(id, _)| id.clone()) {
                keys.remove(&oldest);
            }
        }
        // Being made from here: the same key at once waits for it, while the user data is asked below.
        keys.insert(id.clone(), Kept::Making { at: now, asked: asked.clone() });
    }
    // Made before the memory forgot it (a restart, a day since): its first answer, from the user data.
    if let Some(ud) = kept.clone() {
        let (user, key) = id.clone();
        if let Ok(Ok(Some(k))) = tokio::task::spawn_blocking(move || ud.kept_answer(&user, &key)).await {
            if k.asked != asked {
                KEYS.lock().unwrap().remove(&id);
                return sent_with_another();
            }
            let status = StatusCode::from_u16(k.status).unwrap_or(StatusCode::OK);
            let kind = k.kind.as_deref().and_then(|v| HeaderValue::from_str(v).ok());
            let body = axum::body::Bytes::from(k.body);
            KEYS.lock().unwrap().insert(id, Kept::Done { at: now, asked, status, kind: kind.clone(), body: body.clone() });
            return replayed(status, kind.as_ref(), body);
        }
    }
    let response = next.run(request).await;
    // A server's failure is not kept: sent again, the change is tried again. Nor is an answer too big, or streamed.
    let size = response.body().size_hint();
    let keep = !response.status().is_server_error() && size.upper().is_some_and(|u| u <= BIGGEST);
    if !keep {
        KEYS.lock().unwrap().remove(&id);
        return response;
    }
    let (parts, body) = response.into_parts();
    let Ok(body) = axum::body::to_bytes(body, BIGGEST as usize).await else {
        KEYS.lock().unwrap().remove(&id);
        return (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({ "error": "The answer could not be read." }))).into_response();
    };
    let kind = parts.headers.get(header::CONTENT_TYPE).cloned();
    if let Some(ud) = kept.filter(|_| body.len() <= BIGGEST_KEPT) {
        let (user, key) = id.clone();
        let a = KeptAnswer {
            asked: asked.clone(),
            status: parts.status.as_u16(),
            kind: kind.as_ref().and_then(|k| k.to_str().ok()).map(str::to_string),
            body: body.to_vec(),
        };
        match tokio::task::spawn_blocking(move || ud.keep_answer(&user, &key, &a)).await {
            Ok(Ok(())) => {}
            failed => tracing::warn!("a change's answer not kept past a restart: {failed:?}"),
        }
    }
    KEYS.lock().unwrap().insert(id, Kept::Done { at: now, asked, status: parts.status, kind, body: body.clone() });
    Response::from_parts(parts, Body::from(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::post;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;

    fn app(made: Arc<AtomicUsize>) -> axum::Router {
        axum::Router::new()
            .route(
                "/api/x",
                post(move || {
                    let made = made.clone();
                    async move { axum::Json(json!({ "made": made.fetch_add(1, Ordering::SeqCst) + 1 })) }
                }),
            )
            .route("/api/broken", post(|| async { StatusCode::BAD_GATEWAY }))
            .layer(axum::middleware::from_fn_with_state(None::<Arc<UserData>>, once))
    }

    fn app_keeping(made: Arc<AtomicUsize>, kept: Arc<UserData>) -> axum::Router {
        axum::Router::new()
            .route(
                "/api/x",
                post(move || {
                    let made = made.clone();
                    async move { axum::Json(json!({ "made": made.fetch_add(1, Ordering::SeqCst) + 1 })) }
                }),
            )
            .layer(axum::middleware::from_fn_with_state(Some(kept), once))
    }

    fn ask(path: &str, key: Option<&str>) -> Request {
        let mut r = Request::builder().method("POST").uri(path);
        if let Some(k) = key {
            r = r.header("Idempotency-Key", k);
        }
        r.body(Body::empty()).unwrap()
    }

    async fn text(r: Response) -> String {
        String::from_utf8(axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap()
    }

    #[tokio::test]
    async fn a_change_sent_again_with_its_key_is_made_once_and_answered_as_the_first_time() {
        let made = Arc::new(AtomicUsize::new(0));
        let first = app(made.clone()).oneshot(ask("/api/x", Some("k-once-1"))).await.unwrap();
        assert_eq!(text(first).await, r#"{"made":1}"#);
        let again = app(made.clone()).oneshot(ask("/api/x", Some("k-once-1"))).await.unwrap();
        assert_eq!(again.headers().get("idempotent-replayed").unwrap(), "true");
        assert_eq!(again.headers().get(header::CONTENT_TYPE).unwrap(), "application/json");
        assert_eq!(text(again).await, r#"{"made":1}"#);
        assert_eq!(made.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn without_a_key_or_with_another_each_change_is_made() {
        let made = Arc::new(AtomicUsize::new(0));
        app(made.clone()).oneshot(ask("/api/x", None)).await.unwrap();
        app(made.clone()).oneshot(ask("/api/x", None)).await.unwrap();
        app(made.clone()).oneshot(ask("/api/x", Some("k-other-1"))).await.unwrap();
        app(made.clone()).oneshot(ask("/api/x", Some("k-other-2"))).await.unwrap();
        assert_eq!(made.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn a_key_sent_with_another_change_is_refused() {
        let made = Arc::new(AtomicUsize::new(0));
        app(made.clone()).oneshot(ask("/api/x", Some("k-moved-1"))).await.unwrap();
        let r = app(made.clone()).oneshot(ask("/api/x?y=1", Some("k-moved-1"))).await.unwrap();
        assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(made.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_servers_failure_is_tried_again() {
        let made = Arc::new(AtomicUsize::new(0));
        let r = app(made.clone()).oneshot(ask("/api/broken", Some("k-broken-1"))).await.unwrap();
        assert_eq!(r.status(), StatusCode::BAD_GATEWAY);
        assert!(!KEYS.lock().unwrap().contains_key(&(String::new(), "k-broken-1".to_string())));
    }

    #[tokio::test]
    async fn a_change_sent_again_after_a_restart_is_answered_from_the_user_data() {
        let made = Arc::new(AtomicUsize::new(0));
        let kept = Arc::new(UserData::open(None).unwrap());
        let first = app_keeping(made.clone(), kept.clone()).oneshot(ask("/api/x", Some("k-kept-1"))).await.unwrap();
        assert_eq!(text(first).await, r#"{"made":1}"#);
        // The memory forgets (a restart); the user data does not.
        KEYS.lock().unwrap().remove(&(String::new(), "k-kept-1".to_string()));
        let again = app_keeping(made.clone(), kept.clone()).oneshot(ask("/api/x", Some("k-kept-1"))).await.unwrap();
        assert_eq!(again.headers().get("idempotent-replayed").unwrap(), "true");
        assert_eq!(text(again).await, r#"{"made":1}"#);
        assert_eq!(made.load(Ordering::SeqCst), 1);
        KEYS.lock().unwrap().remove(&(String::new(), "k-kept-1".to_string()));
        let moved = app_keeping(made.clone(), kept).oneshot(ask("/api/x?y=1", Some("k-kept-1"))).await.unwrap();
        assert_eq!(moved.status(), StatusCode::UNPROCESSABLE_ENTITY, "kept with its change");
    }
}
