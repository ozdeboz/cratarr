//! One's own account and, for the administrators, everyone's (decision 0051, several users): `/api/me` (who one is, what
//! one may do, whether one's OpenSubsonic password is set, one's devices and sign-ins) and `/api/users` (the Users panel).
//! With signing in off (one user) there is no one to be: `/api/me` answers 404, and the page takes it for the owner.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, put};
use serde_json::json;

use super::signin::session_of;
use super::users::{Permissions, User};
use super::{Serve, now};

pub fn router(serve: Arc<Serve>) -> Router {
    Router::new()
        .route("/api/me", get(me))
        .route("/api/me/subsonic-password", put(set_app_password))
        .route("/api/me/sessions/{id}", delete(end_own_session))
        .merge(
            Router::new()
                .route("/api/users", get(users))
                .route("/api/users/{name}/permissions", put(set_permissions))
                .route("/api/users/{name}/sessions/{id}", delete(end_session))
                .layer(axum::middleware::from_fn(super::access::admin)),
        )
        .with_state(serve)
}

fn refused(status: StatusCode, error: &str) -> Response {
    (status, axum::Json(json!({ "error": error }))).into_response()
}

/// An account's sign-ins, the one this request came with marked `current`.
fn sessions_of(serve: &Serve, user: &str, current: Option<&str>) -> Vec<serde_json::Value> {
    serve
        .sessions
        .list(now())
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.user == user)
        .map(|s| json!({ "id": s.id, "device": s.device, "created": s.created, "last_used": s.last_used, "current": current == Some(s.id.as_str()) }))
        .collect()
}

/// An account as the Users panel shows it. Its devices come with Play On's owners (decision 0051's second stage).
fn view(serve: &Serve, u: &User, current: Option<&str>) -> serde_json::Value {
    json!({
        "name": u.name, "admin": u.admin, "active": u.active, "directory": u.directory, "permissions": u.permissions,
        "app_password": !u.password.is_empty(), "devices": [], "sessions": sessions_of(serve, &u.name, current),
    })
}

async fn me(State(serve): State<Arc<Serve>>, headers: HeaderMap) -> Response {
    if !serve.sign_in_required() {
        return refused(StatusCode::NOT_FOUND, "Signing in is off: there is one user.");
    }
    let Some(u) = serve.current_user().and_then(|n| serve.users.get(&n)) else { return refused(StatusCode::NOT_FOUND, "No one is signed in.") };
    let current = session_of(&serve, &headers).map(|s| s.id);
    axum::Json(view(&serve, &u, current.as_deref())).into_response()
}

#[derive(serde::Deserialize)]
struct AppPassword {
    password: String,
}

/// One's own OpenSubsonic password (an account from the directory has none until then: its apps sign in with this).
async fn set_app_password(State(serve): State<Arc<Serve>>, axum::Json(b): axum::Json<AppPassword>) -> Response {
    let Some(me) = serve.current_user() else { return refused(StatusCode::NOT_FOUND, "No one is signed in.") };
    match serve.users.set_app_password(&me, &b.password) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(why) => refused(StatusCode::BAD_REQUEST, why),
    }
}

async fn end_own_session(State(serve): State<Arc<Serve>>, Path(id): Path<String>) -> Response {
    let Some(me) = serve.current_user() else { return refused(StatusCode::NOT_FOUND, "No one is signed in.") };
    end(&serve, &me, &id)
}

/// Ends `user`'s sign-in `id`; not found when it is someone else's.
fn end(serve: &Serve, user: &str, id: &str) -> Response {
    let theirs = serve.sessions.list(now()).ok().and_then(|l| l.into_iter().find(|s| s.id == id)).is_some_and(|s| s.user == user);
    if !theirs {
        return refused(StatusCode::NOT_FOUND, "No such sign-in.");
    }
    match serve.sessions.revoke(id) {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => refused(StatusCode::INTERNAL_SERVER_ERROR, &format!("{e:#}")),
    }
}

async fn users(State(serve): State<Arc<Serve>>, headers: HeaderMap) -> Response {
    let current = session_of(&serve, &headers).map(|s| s.id);
    let list: Vec<_> = serve.users.all().iter().map(|u| view(&serve, u, current.as_deref())).collect();
    axum::Json(json!({ "users": list })).into_response()
}

async fn set_permissions(State(serve): State<Arc<Serve>>, Path(name): Path<String>, axum::Json(p): axum::Json<Permissions>) -> Response {
    match serve.users.set_permissions(&name, p) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => refused(StatusCode::NOT_FOUND, "No such user."),
        Err(e) => refused(StatusCode::INTERNAL_SERVER_ERROR, &format!("{e:#}")),
    }
}

async fn end_session(State(serve): State<Arc<Serve>>, Path((name, id)): Path<(String, String)>) -> Response {
    end(&serve, &name, &id)
}

/// The hourly check (decision 0051): who is still in the directory's group, and who of them is an administrator. One
/// who left can no longer sign in; their sign-ins end and their kept password stops standing in when the directory is
/// down; their data stays. Nothing changes when the directory cannot be asked.
pub async fn check_directory(serve: &Serve) {
    let Some((members, admins)) = serve.directory.census().await else { return };
    for u in serve.users.all().into_iter().filter(|u| u.directory) {
        if members.iter().any(|m| m == &u.name) {
            if u.active && u.admin != admins.iter().any(|a| a == &u.name) {
                let _ = serve.users.from_directory(&u.name, !u.admin);
            }
            continue;
        }
        match serve.users.deactivate(&u.name) {
            Ok(true) => {
                let ended = serve.sessions.revoke_user(&u.name, None).unwrap_or(0);
                serve.directory.forget_account(&u.name);
                tracing::warn!(target: "signin", user = %u.name, sessions = ended, "left the directory's group: deactivated");
            }
            Ok(false) => {}
            Err(e) => tracing::warn!(target: "signin", user = %u.name, "could not deactivate: {e:#}"),
        }
    }
}

/// Runs the check every hour (the owner: "it will hardly ever happen. Maybe every hour is fine").
pub fn watch_directory(serve: Arc<Serve>) {
    tokio::spawn(async move {
        let mut every = tokio::time::interval(std::time::Duration::from_secs(3600));
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            every.tick().await;
            check_directory(&serve).await;
        }
    });
}
