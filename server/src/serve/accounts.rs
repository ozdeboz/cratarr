//! One's own account and, for the administrators, everyone's (decision 0051, several users): `/api/me` (who one is and
//! the name one goes by, what one may do, whether one's OpenSubsonic password is set, one's devices and sign-ins) and
//! `/api/users` (the Users panel).
//! A device is a Play On player of the account's (`{ id, name, kind, online, last_seen }`, its id its name).
//! With signing in off (one user) there is no one to be: `/api/me` answers 404, and the page takes it for the owner.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, put};
use serde_json::json;

use super::signin::session_of;
use super::users::{Permissions, Refusal, User};
use super::{Serve, now};

pub fn router(serve: Arc<Serve>) -> Router {
    Router::new()
        .route("/api/me", get(me))
        .route("/api/me/password", put(change_own_password))
        .route("/api/me/subsonic-password", put(set_app_password))
        .route("/api/me/sessions/{id}", delete(end_own_session))
        .route("/api/me/devices/{id}", delete(forget_own_device))
        .route("/api/me/settings", get(own_settings).put(set_own_settings))
        .route("/api/me/history", delete(forget_own_history))
        .merge(
            Router::new()
                .route("/api/users", get(users).post(create_user))
                .route("/api/users/{name}/role", put(set_role))
                .route("/api/users/{name}/password", put(set_password))
                .route("/api/users/{name}/active", put(set_active))
                .route("/api/users/{name}/permissions", put(set_permissions))
                .route("/api/users/{name}/sessions/{id}", delete(end_session))
                .route("/api/users/{name}/devices/{id}", delete(forget_device))
                .layer(axum::middleware::from_fn(super::access::admin)),
        )
        .with_state(serve)
}

fn refused(status: StatusCode, error: &str) -> Response {
    (status, axum::Json(json!({ "error": error }))).into_response()
}

/// An account change refused, as the Users panel shows it.
fn refusal(r: Refusal) -> Response {
    let status = match r {
        Refusal::Bad(_) => StatusCode::BAD_REQUEST,
        Refusal::Conflict(_) => StatusCode::CONFLICT,
        Refusal::Missing => StatusCode::NOT_FOUND,
        Refusal::Unsaved => StatusCode::INTERNAL_SERVER_ERROR,
    };
    refused(status, r.message())
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

/// An account's devices: its Play On players.
fn devices_of(serve: &Serve, user: &str) -> Vec<serde_json::Value> {
    let Some(on) = serve.play_on() else { return Vec::new() };
    on.list_for(Some(user))
        .into_iter()
        .map(|p| json!({ "id": p.name, "name": p.name, "kind": p.kind, "online": p.online, "last_seen": p.seen }))
        .collect()
}

/// An account as the Users panel shows it. `display_name`: the name the directory gives it (null: none), which the page
/// greets one with, else the name.
fn view(serve: &Serve, u: &User, current: Option<&str>) -> serde_json::Value {
    json!({
        "name": u.name, "display_name": u.display_name, "admin": u.admin, "active": u.active, "directory": u.directory, "permissions": u.permissions,
        "app_password": !u.password.is_empty(), "devices": devices_of(serve, &u.name), "sessions": sessions_of(serve, &u.name, current),
    })
}

async fn me(State(serve): State<Arc<Serve>>, headers: HeaderMap) -> Response {
    // Off, one still signed in is that account all the same (`access::require`): the page must show what they may,
    // not the administrator's (a test account's phone showed the server's settings, each refused, 2026-10-24).
    if !serve.sign_in_required() && session_of(&serve, &headers).is_none() {
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

/// One's own OpenSubsonic password, for an account from the directory (none until then: its apps sign in with this). A
/// Cratarr account's own password is its OpenSubsonic one too (`/api/me/password`).
async fn set_app_password(State(serve): State<Arc<Serve>>, axum::Json(b): axum::Json<AppPassword>) -> Response {
    let Some(me) = serve.current_user() else { return refused(StatusCode::NOT_FOUND, "No one is signed in.") };
    if serve.users.get(&me).is_some_and(|u| !u.directory) {
        return refused(StatusCode::CONFLICT, "Your password is your app password too: change that.");
    }
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

async fn forget_own_device(State(serve): State<Arc<Serve>>, Path(id): Path<String>) -> Response {
    let Some(me) = serve.current_user() else { return refused(StatusCode::NOT_FOUND, "No one is signed in.") };
    forget(&serve, &me, &id)
}

/// Forgets `user`'s device `id` (its queue with it); not found when it is someone else's.
fn forget(serve: &Serve, user: &str, id: &str) -> Response {
    let Some(on) = serve.play_on() else { return refused(StatusCode::NOT_FOUND, "No such device.") };
    match on.forget(id, user) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => refused(StatusCode::NOT_FOUND, "No such device."),
        Err(e) => refused(StatusCode::CONFLICT, &e.to_string()),
    }
}

async fn forget_device(State(serve): State<Arc<Serve>>, Path((name, id)): Path<(String, String)>) -> Response {
    forget(&serve, &name, &id)
}

/// "Delete play history" (0.9.5): the account's plays and what was made of them; its favourites, ratings and
/// exclusions stay. The page asks once more before.
async fn forget_own_history(State(serve): State<Arc<Serve>>) -> Response {
    let Some(me) = serve.current_user() else { return refused(StatusCode::NOT_FOUND, "No one is signed in.") };
    let ud = serve.userdata.clone();
    match tokio::task::spawn_blocking(move || ud.forget_history(&me)).await {
        Ok(Ok(removed)) => axum::Json(json!({ "removed": removed })).into_response(),
        Ok(Err(e)) => refused(StatusCode::INTERNAL_SERVER_ERROR, &format!("{e:#}")),
        Err(e) => refused(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// One's own settings (decision 0051's split: the server's are the administrators', these each user's): what the apps
/// keep for the account (Radio's and Autoplay's defaults, leaving favourites out, and the like), as `{ version, settings }`.
/// The server reads two of them (decision 0079): `followable` (Others may follow me; absent: off) and `auto_accept`
/// (the users whose queue requests go straight in).
/// Signed in or not: while signing in is off, the owner's.
async fn own_settings(State(serve): State<Arc<Serve>>) -> Response {
    let Some(me) = serve.current_user() else { return refused(StatusCode::NOT_FOUND, "No one is signed in.") };
    match serve.userdata.settings_of(&me) {
        Ok((version, settings)) => axum::Json(json!({ "version": version, "settings": settings })).into_response(),
        Err(e) => refused(StatusCode::INTERNAL_SERVER_ERROR, &format!("{e:#}")),
    }
}

/// `base`: the version last had (0 while none is kept); `settings`: all of them, as the app keeps them.
#[derive(serde::Deserialize)]
struct SettingsBody {
    base: i64,
    settings: serde_json::Map<String, serde_json::Value>,
}

/// The most one's settings may hold, as JSON (bytes).
const SETTINGS_BYTES: usize = 16 * 1024;

async fn set_own_settings(State(serve): State<Arc<Serve>>, axum::Json(b): axum::Json<SettingsBody>) -> Response {
    let Some(me) = serve.current_user() else { return refused(StatusCode::NOT_FOUND, "No one is signed in.") };
    let settings = serde_json::Value::Object(b.settings);
    if settings.to_string().len() > SETTINGS_BYTES {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "Those settings are too large to keep.");
    }
    match serve.userdata.put_settings(&me, b.base, &settings) {
        Ok(Ok(version)) => {
            // "Others may follow me" off (decision 0079): whoever follows them follows no one now.
            if settings["followable"].as_bool() != Some(true)
                && let Some(on) = serve.play_on()
            {
                on.end_following_of(&me);
            }
            axum::Json(json!({ "version": version })).into_response()
        }
        Ok(Err(crate::userdata::Stale)) => refused(StatusCode::CONFLICT, "They changed meanwhile."),
        Err(e) => refused(StatusCode::INTERNAL_SERVER_ERROR, &format!("{e:#}")),
    }
}

#[derive(serde::Deserialize)]
struct OwnPassword {
    current: String,
    password: String,
}

/// One's own password, for a Cratarr account (any user; the directory keeps its own accounts'): the current one asked.
/// The other sign-ins end.
async fn change_own_password(State(serve): State<Arc<Serve>>, headers: HeaderMap, axum::Json(b): axum::Json<OwnPassword>) -> Response {
    let Some(me) = serve.current_user() else { return refused(StatusCode::NOT_FOUND, "No one is signed in.") };
    if serve.users.get(&me).is_some_and(|u| u.directory) {
        return refused(StatusCode::CONFLICT, "Set in the directory.");
    }
    if let Err(why) = serve.users.change_password(&me, &b.current, &b.password) {
        let status = if why == "The current password is wrong." { StatusCode::FORBIDDEN } else { StatusCode::BAD_REQUEST };
        return refused(status, why);
    }
    let keep = session_of(&serve, &headers).map(|s| s.id);
    let _ = serve.sessions.revoke_user(&me, keep.as_deref());
    StatusCode::NO_CONTENT.into_response()
}

#[derive(serde::Deserialize)]
struct NewUser {
    name: String,
    password: String,
    #[serde(default)]
    admin: bool,
}

/// A new Cratarr account (decision 0051, amended): answered as the Users panel shows it.
async fn create_user(State(serve): State<Arc<Serve>>, axum::Json(b): axum::Json<NewUser>) -> Response {
    match serve.users.create(&b.name, &b.password, b.admin) {
        Ok(u) => {
            tracing::info!(target: "signin", user = %u.name, admin = u.admin, "account made");
            axum::Json(view(&serve, &u, None)).into_response()
        }
        Err(r) => refusal(r),
    }
}

#[derive(serde::Deserialize)]
struct Role {
    admin: bool,
}

async fn set_role(State(serve): State<Arc<Serve>>, Path(name): Path<String>, axum::Json(b): axum::Json<Role>) -> Response {
    match serve.users.set_admin(&name, b.admin, serve.current_user().as_deref()) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(r) => refusal(r),
    }
}

/// A Cratarr account's password, set by an administrator: its sign-ins end (the administrator's own, when it is theirs,
/// stays).
async fn set_password(State(serve): State<Arc<Serve>>, headers: HeaderMap, Path(name): Path<String>, axum::Json(b): axum::Json<AppPassword>) -> Response {
    if let Err(r) = serve.users.set_password(&name, &b.password) {
        return refusal(r);
    }
    let keep = (serve.current_user().as_deref() == Some(name.as_str())).then(|| session_of(&serve, &headers).map(|s| s.id)).flatten();
    let _ = serve.sessions.revoke_user(&name, keep.as_deref());
    StatusCode::NO_CONTENT.into_response()
}

#[derive(serde::Deserialize)]
struct Active {
    active: bool,
}

/// A Cratarr account removed (inactive, its data kept) or restored.
async fn set_active(State(serve): State<Arc<Serve>>, Path(name): Path<String>, axum::Json(b): axum::Json<Active>) -> Response {
    match serve.users.set_active(&name, b.active, serve.current_user().as_deref()) {
        Ok(changed) => {
            if changed && !b.active {
                let ended = left(&serve, &name).await;
                tracing::warn!(target: "signin", user = %name, sessions = ended, "account removed: deactivated");
            } else if changed {
                tracing::info!(target: "signin", user = %name, "account restored");
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(r) => refusal(r),
    }
}

/// An account that may no longer use Cratarr (removed, or out of the directory's group): its sign-ins end and its
/// devices stop playing for it (paused: their queues stay, as its data does). Returns how many sign-ins ended.
async fn left(serve: &Serve, name: &str) -> usize {
    let ended = serve.sessions.revoke_user(name, None).unwrap_or(0);
    if let Some(on) = serve.play_on() {
        for p in on.list_for(Some(name)).into_iter().filter(|p| p.online && p.playing) {
            if let Err(e) = on.command(&p.name, crate::app::PlayOnCommand::Pause).await {
                tracing::warn!(target: "signin", user = %name, player = %p.name, "could not stop its player: {e}");
            }
        }
    }
    ended
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

/// The hourly check (decision 0051): who is still in the directory's group, who of them is an administrator, and the
/// name each goes by (kept on any account of theirs, Cratarr's own of the same name too). One who left can no longer
/// sign in; their sign-ins end and their kept password stops standing in when the directory is down; their data stays.
/// Nothing changes when the directory cannot be asked.
pub async fn check_directory(serve: &Serve) {
    let Some((members, admins)) = serve.directory.census().await else { return };
    for u in serve.users.all() {
        if let Some(m) = members.iter().find(|m| m.name == u.name) {
            let display_name = m.display_name.as_deref();
            let kept = if u.directory && u.active && u.admin != admins.iter().any(|a| a == &u.name) {
                serve.users.from_directory(&u.name, !u.admin, display_name).map(|_| ())
            } else {
                serve.users.set_display_name(&u.name, display_name).map(|_| ())
            };
            if let Err(e) = kept {
                tracing::warn!(target: "signin", user = %u.name, "could not keep what the directory says of the account: {e:#}");
            }
            continue;
        }
        if !u.directory {
            continue;
        }
        match serve.users.deactivate(&u.name) {
            Ok(true) => {
                let ended = left(serve, &u.name).await;
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
