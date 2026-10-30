//! A change sent again gives its first answer (0.9.15, the page's offline changes: the owner, 2026-10-28, "the best PWA
//! offline support possible"). A page out of reach keeps its changes (a heart, a rating, a playlist's tracks) and sends
//! them when Cratarr answers again; one whose answer got lost on the way is sent once more. With an `Idempotency-Key`
//! header (as Stripe's and the IETF draft's), Cratarr makes each change once: the same key again, from the same account,
//! for the same method and path, gets the first answer back (`Idempotent-Replayed: true`) without doing it again.
//! Kept a day, in memory: a restart forgets them, as it forgets what was half done.

use axum::body::{Body, HttpBody};
use axum::extract::Request;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// How long an answer is kept for a key sent again.
const KEEP: Duration = Duration::from_secs(24 * 3600);
/// The most keys kept at once; past it the oldest go.
const MOST: usize = 10_000;
/// An answer kept only up to this size (a change's answers are small); a bigger one is not kept, and the key with it.
const BIGGEST: u64 = 256 << 10;

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

/// The layer, inside the access check (who asks is known there). A change without a key goes as always.
pub async fn once(request: Request, next: Next) -> Response {
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
                return (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(json!({ "error": "This key was sent with another change." }))).into_response();
            }
            return match k {
                Kept::Making { .. } => {
                    let mut r = (StatusCode::CONFLICT, axum::Json(json!({ "error": "This change is still being made." }))).into_response();
                    r.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(1));
                    r
                }
                Kept::Done { status, kind, body, .. } => {
                    let mut r = Response::new(Body::from(body.clone()));
                    *r.status_mut() = *status;
                    if let Some(kind) = kind {
                        r.headers_mut().insert(header::CONTENT_TYPE, kind.clone());
                    }
                    r.headers_mut().insert("idempotent-replayed", HeaderValue::from_static("true"));
                    r
                }
            };
        }
        if keys.len() >= MOST {
            if let Some(oldest) = keys.iter().min_by_key(|(_, k)| k.at()).map(|(id, _)| id.clone()) {
                keys.remove(&oldest);
            }
        }
        keys.insert(id.clone(), Kept::Making { at: now, asked: asked.clone() });
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
    KEYS.lock().unwrap().insert(
        id,
        Kept::Done { at: now, asked, status: parts.status, kind: parts.headers.get(header::CONTENT_TYPE).cloned(), body: body.clone() },
    );
    Response::from_parts(parts, Body::from(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::post;
    use std::sync::Arc;
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
            .layer(axum::middleware::from_fn(once))
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
}
