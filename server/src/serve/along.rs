//! Listening along and queue requests (decision 0079; the owner, 2026-10-30), as the pages and the app ask them:
//!
//! - `GET /api/follow/leaders`: those one may follow and their players; `GET /api/follow/followers`: who follows one's
//!   players, with those whose requests go straight in.
//! - `PUT`/`DELETE /api/players/{mine}/follow`: one's own player follows another's, or stops.
//! - `POST /api/queue-requests`: a track asked of a player one may follow; `GET /api/queue-requests` (`?player=`): those
//!   waiting for one's answer; `GET /api/queue-requests/mine`: one's own; `POST /api/queue-requests/{id}/accept` and
//!   `/decline`: the leader's (or an administrator's) answer.
//!
//! One's own settings say whether others may follow (`followable`) and whose requests go straight in (`auto_accept`).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use super::Serve;
use crate::api::ApiError;
use crate::app::queue_requests::Asker;
use crate::app::{FollowTarget, PlayOn, QueueTrack, ToggleError};

#[derive(Clone)]
pub struct Along {
    on: Arc<PlayOn>,
    serve: Arc<Serve>,
}

pub fn router(on: Arc<PlayOn>, serve: Arc<Serve>) -> Router {
    Router::new()
        .route("/api/follow/leaders", get(leaders))
        .route("/api/follow/followers", get(followers))
        .route("/api/players/{name}/follow", put(follow).delete(unfollow))
        .route("/api/queue-requests", get(waiting).post(ask))
        .route("/api/queue-requests/mine", get(mine))
        .route("/api/queue-requests/{id}/accept", post(accept))
        .route("/api/queue-requests/{id}/decline", post(decline))
        .with_state(Along { on, serve })
}

/// An account's own settings (`/api/me/settings`), none where none are kept.
fn settings(serve: &Serve, user: &str) -> serde_json::Value {
    serve.userdata.settings_of(user).map(|(_, s)| s).unwrap_or_default()
}

/// "Others may follow me" (the owner, 2026-10-30): off unless the account turned it on.
pub fn followable(serve: &Serve, user: &str) -> bool {
    settings(serve, user)["followable"].as_bool() == Some(true)
}

/// The users whose queue requests go straight into `user`'s queue.
pub fn auto_accept(serve: &Serve, user: &str) -> Vec<String> {
    settings(serve, user)["auto_accept"].as_array().map(|a| a.iter().filter_map(|u| u.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

/// Who asks: their account, whether from away from home, whether an administrator.
fn asker(serve: &Serve) -> Result<(String, bool, bool), ToggleError> {
    let user = serve.current_user().ok_or_else(|| ToggleError::Refused("Sign in to listen along.".into()))?;
    let away = crate::serve::who::current().is_some_and(|w| !w.home);
    Ok((user, away, super::access::administering()))
}

async fn leaders(State(a): State<Along>) -> Result<Response, ApiError> {
    let (user, away, _) = asker(&a.serve)?;
    let serve = a.serve.clone();
    Ok(Json(a.on.leaders(&user, away, &|u: &str| followable(&serve, u))).into_response())
}

async fn followers(State(a): State<Along>) -> Result<Response, ApiError> {
    let (user, _, _) = asker(&a.serve)?;
    Ok(Json(a.on.followers(&user, &auto_accept(&a.serve, &user))).into_response())
}

async fn follow(State(a): State<Along>, Path(name): Path<String>, Json(to): Json<FollowTarget>) -> Result<Response, ApiError> {
    let (user, away, _) = asker(&a.serve)?;
    let open = followable(&a.serve, &to.user);
    a.on.follow(&user, &name, to.clone(), away, open)?;
    Ok(Json(json!({ "following": to })).into_response())
}

async fn unfollow(State(a): State<Along>, Path(name): Path<String>) -> Result<Response, ApiError> {
    let (user, _, _) = asker(&a.serve)?;
    a.on.unfollow(&user, &name)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Deserialize)]
struct Ask {
    to: FollowTarget,
    track: QueueTrack,
    /// `next` or `last` (the default).
    #[serde(default)]
    how: Option<String>,
}

/// `next` or `last`; anything else refused.
pub(super) fn how_of(how: Option<&str>) -> Result<&'static str, ToggleError> {
    match how {
        None | Some("last") => Ok("last"),
        Some("next") => Ok("next"),
        Some(other) => Err(ToggleError::BadSettings(format!("{other} is not next or last."))),
    }
}

/// A track asked of a player one may follow (following it now or not: one at the party asks without listening along),
/// a Bandcamp preview too; straight in when the leader marked the asker so.
async fn ask(State(a): State<Along>, Json(b): Json<Ask>) -> Result<Response, ApiError> {
    let (user, away, _) = asker(&a.serve)?;
    let how = how_of(b.how.as_deref())?;
    let open = followable(&a.serve, &b.to.user);
    a.on.may_follow(&user, &b.to, away, open)?;
    let auto = auto_accept(&a.serve, &b.to.user).contains(&user);
    let r = a.on.ask_for(Asker::User(user), &b.to.player, &b.to.user, b.track, how, None, auto).await?;
    Ok(Json(json!({ "id": r.id, "state": r.state })).into_response())
}

#[derive(Deserialize)]
struct OnePlayer {
    #[serde(default)]
    player: Option<String>,
}

/// Those waiting for the asking account's answer, on its players (one of them, `?player=`), the oldest first.
async fn waiting(State(a): State<Along>, Query(q): Query<OnePlayer>) -> Result<Response, ApiError> {
    let (user, _, _) = asker(&a.serve)?;
    Ok(Json(a.on.requests.waiting_for(&user, q.player.as_deref())).into_response())
}

async fn mine(State(a): State<Along>) -> Result<Response, ApiError> {
    let (user, _, _) = asker(&a.serve)?;
    Ok(Json(a.on.requests.of_user(&user)).into_response())
}

fn no_such() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "No such request." }))).into_response()
}

async fn accept(State(a): State<Along>, Path(id): Path<String>) -> Result<Response, ApiError> {
    let (user, _, admin) = asker(&a.serve)?;
    if a.on.requests.get(&id).is_none() {
        return Ok(no_such());
    }
    let r = a.on.accept(&id, &user, admin).await?;
    Ok(Json(json!({ "id": r.id, "state": r.state })).into_response())
}

async fn decline(State(a): State<Along>, Path(id): Path<String>) -> Result<Response, ApiError> {
    let (user, _, admin) = asker(&a.serve)?;
    if a.on.requests.get(&id).is_none() {
        return Ok(no_such());
    }
    let r = a.on.decline(&id, &user, admin)?;
    Ok(Json(json!({ "id": r.id, "state": r.state })).into_response())
}
