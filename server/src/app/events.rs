//! The page's live channel (server-sent events): what plays and which playlists hold it, the conversion queue, Cratarr's
//! own players (Play On), a note when an album action changed a list, and the account's favourites as they change. Pages open one stream; each connection is sent only what changed for it.
//!
//! What plays is noticed by watching the servers' players: about once a second, and only while some page listens.
//! A server that cannot say cheaply what its players do (`Source::players`) is asked for the whole state every two
//! seconds instead, which is what the pages did themselves before.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::broadcast;

use super::App;

/// How often the players are looked at while someone listens.
const WATCH: Duration = Duration::from_secs(1);
/// The state is sent again at least this often (a position drifts, another page changed a playlist).
const RESEND: Duration = Duration::from_secs(10);

/// One message on the channel: an SSE event name and its JSON data.
#[derive(Debug, Clone)]
pub struct Message {
    pub name: &'static str,
    pub data: String,
}

impl Message {
    fn of(name: &'static str, data: &impl Serialize) -> Option<Self> {
        serde_json::to_string(data).ok().map(|data| Self { name, data })
    }
}

/// Tells the connections that a player may have changed; runs the watcher while any listen.
pub struct PlayerWatch {
    tell: broadcast::Sender<()>,
}

impl Default for PlayerWatch {
    fn default() -> Self {
        Self { tell: broadcast::channel(16).0 }
    }
}

impl PlayerWatch {
    /// Listen; starts the watcher when this is the first.
    fn subscribe(&self, app: &Arc<App>) -> broadcast::Receiver<()> {
        let rx = self.tell.subscribe();
        if self.tell.receiver_count() == 1 {
            let (tell, app) = (self.tell.clone(), Arc::downgrade(app));
            tokio::spawn(async move {
                let mut last: Vec<Option<String>> = Vec::new();
                while tell.receiver_count() > 0 {
                    let Some(app) = app.upgrade() else { return };
                    let mut now = Vec::new();
                    for (_, s) in app.library.connected_sources() {
                        now.push(s.players().await.ok().flatten());
                    }
                    drop(app);
                    // Unknown (a server that cannot tell, or did not answer) counts as changed.
                    if now != last || now.iter().any(Option::is_none) {
                        let _ = tell.send(());
                        last = now;
                    }
                    tokio::time::sleep(if last.iter().any(Option::is_none) { 2 * WATCH } else { WATCH }).await;
                }
            });
        }
        rx
    }
}

/// One page's stream: the state for the source it follows, the queue, and list changes, from now on.
pub fn connect(app: &Arc<App>, follow: Option<String>) -> tokio::sync::mpsc::Receiver<Message> {
    let (out, rx) = tokio::sync::mpsc::channel::<Message>(16);
    let mut player = app.events.subscribe(app);
    let mut applied = app.journal.subscribe();
    let mut undone = app.journal.subscribe_undone();
    let mut jobs = app.incoming.subscribe_jobs();
    let mut arrived = app.incoming.subscribe_incoming();
    let mut collected = app.collection.subscribe();
    let mut players = app.play_on.subscribe();
    let mut tasks = super::tasks::tasks().subscribe();
    // The account's favourites as they change, by any way (userdata `Starred`), where Cratarr's own library is open.
    let mut starred = app.served.as_ref().map(|s| s.userdata.subscribe_starred());
    // Albums' covers as they change (0.9.7): their new stamps.
    let mut covers = app.served.as_ref().map(|s| s.subscribe_covers());
    // Single-track replacement's downloads as they go (0.9.8).
    let mut fetches = super::track_fetch::subscribe();
    let app = app.clone();
    tokio::spawn(crate::serve::who::carried(async move {
        // Whose tasks this page is told of (`app::tasks`): the account it is signed in as.
        let account = app.served.as_ref().and_then(|s| s.current_user());
        for t in account.as_deref().map(|a| super::tasks::tasks().of(a)).unwrap_or_default() {
            if let Some(m) = Message::of("task", &t)
                && out.send(m).await.is_err()
            {
                return;
            }
        }
        let mut sent = String::new();
        let send_state = async |sent: &mut String| -> bool {
            match app.playing.state(follow.as_deref()).await {
                Ok(view) => match Message::of("state", &view) {
                    Some(m) if m.data != *sent => {
                        *sent = m.data.clone();
                        out.send(m).await.is_ok()
                    }
                    _ => !out.is_closed(),
                },
                Err(e) => {
                    tracing::debug!("events: state: {e:#}");
                    !out.is_closed()
                }
            }
        };
        if let Some(m) = Message::of("jobs", &app.incoming.jobs())
            && out.send(m).await.is_err()
        {
            return;
        }
        if !send_state(&mut sent).await {
            return;
        }
        // Cratarr's own players (Play On, decision 0031): their list now and on every change.
        let send_players = async || match Message::of("players", &app.play_on.list()) {
            Some(m) => out.send(m).await.is_ok(),
            None => true,
        };
        if !send_players().await {
            return;
        }
        // Whether the journal's two channels, the Incoming one and the Collection one still send (a closed one is left
        // alone).
        let mut open = [true; 4];
        let mut resend = tokio::time::interval(RESEND);
        resend.reset();
        loop {
            let alive = tokio::select! {
                r = player.recv() => match r {
                    Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) => send_state(&mut sent).await,
                    Err(broadcast::error::RecvError::Closed) => false,
                },
                _ = resend.tick() => send_state(&mut sent).await,
                r = jobs.recv() => match r {
                    Ok(list) => match Message::of("jobs", &list) {
                        Some(m) => out.send(m).await.is_ok(),
                        None => true,
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => match Message::of("jobs", &app.incoming.jobs()) {
                        Some(m) => out.send(m).await.is_ok(),
                        None => true,
                    },
                    Err(broadcast::error::RecvError::Closed) => false,
                },
                r = applied.recv(), if open[0] => changed(&out, r, &mut open[0], &["incoming", "playlists"]).await,
                r = undone.recv(), if open[1] => changed(&out, r, &mut open[1], &["incoming", "playlists"]).await,
                r = arrived.recv(), if open[2] => changed(&out, r, &mut open[2], &["incoming"]).await,
                r = collected.recv(), if open[3] => changed(&out, r, &mut open[3], &["collection"]).await,
                r = tasks.recv() => match r {
                    Ok(t) if account.as_deref() == Some(t.account.as_str()) => match Message::of("task", &t) {
                        Some(m) => out.send(m).await.is_ok(),
                        None => true,
                    },
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => true,
                    Err(broadcast::error::RecvError::Closed) => false,
                },
                r = players.recv() => match r {
                    Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) => send_players().await,
                    Err(broadcast::error::RecvError::Closed) => false,
                },
                r = fetches.recv() => match r {
                    Ok(t) if crate::serve::access::administering() => match Message::of("fetch", &t) {
                        Some(m) => out.send(m).await.is_ok(),
                        None => true,
                    },
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => true,
                    Err(broadcast::error::RecvError::Closed) => false,
                },
                r = async {
                    match starred.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => match r {
                    // Each of the account's hearts at once, then the state (the player's heart) without waiting.
                    Ok(s) if account.as_deref() == Some(s.user.as_str()) => {
                        let mut ok = true;
                        for source in app.library.own_sources() {
                            let told = serde_json::json!({ "source": source, "kind": s.kind, "items": s.items, "on": s.on });
                            if let Some(m) = Message::of("favourite", &told) {
                                ok = ok && out.send(m).await.is_ok();
                            }
                        }
                        ok && send_state(&mut sent).await
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => true,
                    Err(broadcast::error::RecvError::Closed) => {
                        starred = None;
                        true
                    }
                },
                r = async {
                    match covers.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => match r {
                    Ok(changes) => covers_changed(&out, &app.library.own_sources(), &changes).await,
                    // Too many to tell one by one: the page reads every stamp again.
                    Err(broadcast::error::RecvError::Lagged(_)) => covers_changed(&out, &app.library.own_sources(), &[]).await,
                    Err(broadcast::error::RecvError::Closed) => {
                        covers = None;
                        true
                    }
                },
                () = out.closed() => false,
            };
            if !alive {
                return;
            }
        }
    }));
    rx
}

/// Above this many covers changed at once (a library restructured), the page is told to read every stamp again.
const COVERS_ONE_BY_ONE: usize = 200;

/// Covers changed: each told (`cover`: source, album, its stamp), or, for none or too many, all at once (`covers`: the
/// page reads `GET /api/covers/stamps` again).
async fn covers_changed(out: &tokio::sync::mpsc::Sender<Message>, sources: &[String], changes: &[crate::serve::CoverChange]) -> bool {
    for source in sources {
        if changes.is_empty() || changes.len() > COVERS_ONE_BY_ONE {
            if let Some(m) = Message::of("covers", &serde_json::json!({ "source": source }))
                && out.send(m).await.is_err()
            {
                return false;
            }
            continue;
        }
        for c in changes {
            if let Some(m) = Message::of("cover", &serde_json::json!({ "source": source, "album": c.album, "v": c.v }))
                && out.send(m).await.is_err()
            {
                return false;
            }
        }
    }
    true
}

/// An album action was applied or undone, or new albums arrived: the pages showing those `areas` read them again.
async fn changed<T: Clone>(
    out: &tokio::sync::mpsc::Sender<Message>,
    r: Result<T, broadcast::error::RecvError>,
    open: &mut bool,
    areas: &[&str],
) -> bool {
    if matches!(r, Err(broadcast::error::RecvError::Closed)) {
        *open = false; // the sender went away; the stream itself carries on
        return true;
    }
    for area in areas {
        if let Some(m) = Message::of("changed", &serde_json::json!({ "area": area }))
            && out.send(m).await.is_err()
        {
            return false;
        }
    }
    true
}
