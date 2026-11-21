//! An artist Out everywhere (decision 0083), on the one path (GF core's real kind): `PUT` and `DELETE /api/me/excluded-artists/
//! {key}` are commands whose answer is `{ result, undo, rev }`, the token `j<n>`; its Undo goes through `POST /api/undo` alone
//! and answers a Redo, a new Change on the path. Per account (the account's alone to undo), a day to undo it. Its write and its
//! change-log row are one transaction in the user data (CP9).

use std::sync::Arc;

use cratarr_domain::can::{Decision, Scope, Viewer, Why, said};
use cratarr_domain::change::{Change, ChangeId, ChangeKind, Refusal};
use serde::{Deserialize, Serialize};

use super::super::ToggleError;
use super::super::commands::{Applied, Boxed, Command, Commands, Failed, Plan, Planned, Shows, Undoable, Undone};
use crate::serve::Serve;

/// What it answers: the artist as listed (put on), or their key alone (taken off).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outed {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
}

/// What its Change keeps: the artist, and whether it put them on.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Data {
    key: String,
    name: String,
    out: bool,
}

/// Put on the asking account's everywhere list, or taken off.
pub struct Out {
    serve: Arc<Serve>,
    key: String,
    name: String,
    out: bool,
}

impl Out {
    /// `name` put on the list, by their key (`excluded_artists::key`).
    pub fn on(serve: Arc<Serve>, name: &str) -> Self {
        let name = name.trim().to_string();
        Self { serve, key: super::super::excluded_artists::key(&name), name, out: true }
    }

    /// The artist of `key` taken off it.
    pub fn off(serve: Arc<Serve>, key: &str) -> Self {
        Self { serve, key: super::super::excluded_artists::key(key), name: String::new(), out: false }
    }
}

/// Changing what one's own account picks: a sign-in or a full key; a key that reads or controls changes none of it.
fn may(v: &Viewer) -> Decision {
    if v.scope == Scope::Full { Decision::Allow } else { Decision::Deny(Why::Right(said::KEY)) }
}

fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> impl std::future::Future<Output = Result<T, ToggleError>> {
    async move { Ok(crate::run::spawn_blocking(f).await.map_err(|e| anyhow::anyhow!("{e}"))??) }
}

/// The account's list as it stands: `key`'s row, if they are on it.
async fn listed(serve: &Arc<Serve>, user: &str, key: &str) -> Result<Option<crate::userdata::ExcludedArtist>, ToggleError> {
    let (serve, user, key) = (serve.clone(), user.to_string(), key.to_string());
    blocking(move || Ok(serve.userdata.excluded_artist_list(&user)?.into_iter().find(|a| a.key == key))).await
}

fn user(v: &Viewer) -> Result<String, ToggleError> {
    Some(v.account.clone()).filter(|a| !a.is_empty()).ok_or_else(|| ToggleError::Refused("Sign in first: what is excluded is an account's own.".into()))
}

impl Command for Out {
    type Result = Outed;

    fn kind(&self) -> ChangeKind {
        ChangeKind::ArtistOut
    }

    fn decide(&self, v: &Viewer) -> Decision {
        may(v)
    }

    fn plan<'a>(&'a self, v: &'a Viewer) -> Boxed<'a, Result<Plan<Outed>, ToggleError>> {
        Box::pin(async move {
            let user = user(v)?;
            if self.key.is_empty() {
                return Err(ToggleError::Refused("Which artist?".into()));
            }
            let now = listed(&self.serve, &user, &self.key).await?;
            let name = match (&now, self.out) {
                // On already: the name is the latest said, and nothing to undo (as before the path).
                (Some(_), true) => {
                    let (serve, key, name) = (self.serve.clone(), self.key.clone(), self.name.clone());
                    let u = user.clone();
                    blocking(move || serve.userdata.set_artist_out(&u, &key, &name, true)).await?;
                    let a = listed(&self.serve, &user, &self.key).await?;
                    return Ok(Plan::Nothing(Outed { key: self.key.clone(), name: a.as_ref().map(|a| a.name.clone()), at: a.map(|a| a.at) }));
                }
                (None, false) => return Ok(Plan::Nothing(Outed { key: self.key.clone(), name: None, at: None })),
                (None, true) => self.name.clone(),
                (Some(a), false) => a.name.clone(),
            };
            let words = if self.out { format!("{name} excluded everywhere.") } else { format!("{name} back everywhere.") };
            let data = Data { key: self.key.clone(), name, out: self.out };
            Ok(Plan::Change(Planned {
                object: format!("artist:{}", self.key),
                name: words,
                data: serde_json::to_value(data).map_err(|e| anyhow::anyhow!("{e}"))?,
            }))
        })
    }

    fn apply<'a>(&'a self, v: &'a Viewer, _id: ChangeId, planned: &'a Planned) -> Boxed<'a, Result<Applied<Outed>, ToggleError>> {
        Box::pin(async move {
            let user = user(v)?;
            let d: Data = serde_json::from_value(planned.data.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
            let (serve, u, key, name, out) = (self.serve.clone(), user.clone(), d.key.clone(), d.name.clone(), d.out);
            let Some(rev) = blocking(move || serve.userdata.set_artist_out(&u, &key, &name, out)).await? else {
                return Err(ToggleError::Refused(Refusal::ChangedSince(None).words().into()));
            };
            let result = if out {
                let a = listed(&self.serve, &user, &d.key).await?;
                Outed { key: d.key, name: Some(d.name), at: a.map(|a| a.at) }
            } else {
                Outed { key: d.key, name: None, at: None }
            };
            Ok(Applied { result, rev: Some(rev), records: Vec::new() })
        })
    }
}

/// Its Undo: the artist back as they were, where the list still holds what the Change left; the Undo's Change is the Redo.
pub struct OutKind {
    pub serve: Arc<Serve>,
}

fn data(c: &Change) -> Result<Data, ToggleError> {
    Ok(serde_json::from_str(&c.data).map_err(|e| anyhow::anyhow!("{e}"))?)
}

impl Undoable for OutKind {
    fn kind(&self) -> ChangeKind {
        ChangeKind::ArtistOut
    }

    fn may(&self, v: &Viewer, _c: &Change) -> Decision {
        may(v)
    }

    fn shows<'a>(&'a self, c: &'a Change) -> Boxed<'a, Result<Shows, ToggleError>> {
        Box::pin(async move {
            let d = data(c)?;
            let Some(user) = c.who.as_deref() else { return Ok(Shows::NotApplied) };
            let on = listed(&self.serve, user, &d.key).await?.is_some();
            Ok(if on == d.out { Shows::Applied } else { Shows::NotApplied })
        })
    }

    fn undo<'a>(&'a self, path: &'a Commands, c: &'a Change, by: &'a Viewer, _parts: Option<&'a [String]>) -> Boxed<'a, Result<Undone, Failed>> {
        Box::pin(async move {
            let d = data(c).map_err(Failed::Error)?;
            let user = c.who.as_deref().ok_or(Failed::Refused(Refusal::NoLonger))?;
            if listed(&self.serve, user, &d.key).await.map_err(Failed::Error)?.is_some() != d.out {
                return Err(Failed::Refused(Refusal::ChangedSince(None)));
            }
            let back = Out { serve: self.serve.clone(), key: d.key, name: d.name, out: !d.out };
            let answer = path.run(by, None, &back).await.map_err(Failed::Error)?;
            let redo = answer.undo.as_deref().and_then(ChangeId::of).ok_or(Failed::Refused(Refusal::ChangedSince(None)))?;
            Ok(Undone { rev: Some(answer.rev), redo: Some(redo), records: Vec::new() })
        })
    }
}
