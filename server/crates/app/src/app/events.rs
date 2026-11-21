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
            crate::run::spawn(async move {
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
    let mut tasks = crate::runner::runner().subscribe();
    // Health › Activity's jobs as they change (RN, administrators' only, R8): a patch per change, a step at most four a second.
    let mut activity = crate::runner::runner().subscribe_activity();
    // The account's favourites as they change, by any way (userdata `Starred`), where Cratarr's own library is open.
    let mut starred = app.served.as_ref().map(|s| s.userdata.subscribe_starred());
    // The account's preferences as another of its devices changes them (decision 0101 point 14).
    let mut preferences = app.served.as_ref().map(|s| s.userdata.subscribe_preferences());
    // Albums' covers as they change (0.9.7): their new stamps.
    let mut covers = app.served.as_ref().map(|s| s.subscribe_covers());
    // Records a view shows as they change (LV3): the change log's rev and their ids.
    let mut records = app.served.as_ref().map(|s| s.subscribe_records());
    // Single-track replacement's downloads as they go (0.9.8).
    let mut fetches = super::track_fetch::subscribe();
    // Runs changing the library's files (decision 0092), as they go.
    let mut runs = super::file_runs::subscribe();
    // The library's words as they change, by any way (0.15, decision 0097).
    let mut words = super::words::subscribe();
    let app = app.clone();
    crate::run::spawn(crate::serve::who::carried(async move {
        // Whose tasks this page is told of (`app::tasks`): the account it is signed in as.
        let account = app.served.as_ref().and_then(|s| s.current_user());
        for t in account.as_deref().map(|a| crate::runner::runner().of(a)).unwrap_or_default() {
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
        // The jobs this page is sent (G8): an administrator's every one, a user's their own.
        let admin = crate::serve::access::administering();
        let shown = |jobs: Vec<super::JobView>| super::incoming::shown_jobs(jobs, account.as_deref(), admin);
        if let Some(m) = Message::of("jobs", &shown(app.incoming.jobs()))
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
        let mut burst = Burst::default();
        loop {
            let alive = tokio::select! {
                r = player.recv() => match r {
                    Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) => send_state(&mut sent).await,
                    Err(broadcast::error::RecvError::Closed) => false,
                },
                _ = resend.tick() => send_state(&mut sent).await,
                r = jobs.recv() => match r {
                    Ok(list) => match Message::of("jobs", &shown(list)) {
                        Some(m) => out.send(m).await.is_ok(),
                        None => true,
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => match Message::of("jobs", &shown(app.incoming.jobs())) {
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
                    // Its own jobs, and the Cratarr-owned ones it follows (N8): never another's.
                    Ok(t) if account.as_deref().is_some_and(|a| t.told_to(a)) => match Message::of("task", &t) {
                        Some(m) => out.send(m).await.is_ok(),
                        None => true,
                    },
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => true,
                    Err(broadcast::error::RecvError::Closed) => false,
                },
                r = activity.recv() => match r {
                    Ok(row) if crate::serve::access::administering() => match Message::of("activity", &row) {
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
                r = words.recv() => match r {
                    Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) => match Message::of("changed", &serde_json::json!({ "area": "words" })) {
                        Some(m) => out.send(m).await.is_ok(),
                        None => true,
                    },
                    Err(broadcast::error::RecvError::Closed) => false,
                },
                r = runs.recv() => match r {
                    Ok(s) if crate::serve::access::administering() => match Message::of("file_run", &s) {
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
                    match preferences.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => match r {
                    // Only to the account's own pages: which preference changed, the page reads them again.
                    Ok(p) if account.as_deref() == Some(p.account.as_str()) => {
                        match Message::of("changed", &serde_json::json!({ "area": "preferences", "key": p.key })) {
                            Some(m) => out.send(m).await.is_ok(),
                            None => true,
                        }
                    }
                    Ok(_) => true,
                    // Some went by unheard: whichever they were, the page reads them all again.
                    Err(broadcast::error::RecvError::Lagged(_)) => match Message::of("changed", &serde_json::json!({ "area": "preferences" })) {
                        Some(m) => out.send(m).await.is_ok(),
                        None => true,
                    },
                    Err(broadcast::error::RecvError::Closed) => {
                        preferences = None;
                        true
                    }
                },
                r = async {
                    match records.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => match r {
                    Ok(c) => match records_told(&c, &mut burst, std::time::Instant::now()) {
                        Some(m) => out.send(m).await.is_ok(),
                        None => true,
                    },
                    // Some went by unheard: the page reads the records on screen again.
                    Err(broadcast::error::RecvError::Lagged(_)) => match Message::of("changed", &serde_json::json!({ "stale": "record" })) {
                        Some(m) => out.send(m).await.is_ok(),
                        None => true,
                    },
                    Err(broadcast::error::RecvError::Closed) => {
                        records = None;
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

/// Above this many records told in one second, the page is told the records are stale in place of their ids (LV12, the review's M4):
/// it reads what is on screen.
const RECORDS_ONE_BY_ONE: usize = 200;

/// How many records a stream told in the second it is in.
#[derive(Debug, Default)]
struct Burst {
    since: Option<std::time::Instant>,
    told: usize,
}

/// What a stream is told of records changed (LV3, LV12): `changed { rev, ids }`, every page alike (a record row is everyone's:
/// what one account alone sees, its hearts, comes as its favourites); past `RECORDS_ONE_BY_ONE` in a second, `changed { rev,
/// stale: "record" }` once that second. Pure.
fn records_told(c: &crate::serve::RecordsChanged, burst: &mut Burst, now: std::time::Instant) -> Option<Message> {
    if burst.since.is_none_or(|s| now.duration_since(s) >= std::time::Duration::from_secs(1)) {
        *burst = Burst { since: Some(now), told: 0 };
    }
    let before = burst.told;
    burst.told += c.ids.len();
    if burst.told <= RECORDS_ONE_BY_ONE {
        Message::of("changed", &serde_json::json!({ "rev": c.rev, "ids": c.ids }))
    } else if before <= RECORDS_ONE_BY_ONE {
        Message::of("changed", &serde_json::json!({ "rev": c.rev, "stale": "record" }))
    } else {
        // Told stale already this second: the page reads what is on screen once.
        None
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::{RecordChanged, RecordsChanged};

    fn changed(rev: i64, ids: std::ops::Range<i64>) -> RecordsChanged {
        RecordsChanged { rev, ids: ids.map(|n| RecordChanged { id: format!("al-{n}"), op: "changed", into: None }).collect() }
    }

    fn data(m: Option<Message>) -> serde_json::Value {
        let m = m.expect("told");
        assert_eq!(m.name, "changed");
        serde_json::from_str(&m.data).unwrap()
    }

    /// LV3: a record's change is told as `changed { rev, ids }`, the same to every stream: a record row is everyone's (the log's
    /// `account` ''), and what one account alone sees, its hearts, is its favourites' (the starred channel, to its pages alone). A
    /// merged record names where it went.
    #[test]
    fn a_change_is_told_to_whose_view_it_changes() {
        let night_ferry = RecordsChanged { rev: 140, ids: vec![RecordChanged { id: "al-42".into(), op: "changed", into: None }] };
        let now = std::time::Instant::now();
        let told: Vec<serde_json::Value> = ["root", "alice", "bob"].iter().map(|_| data(records_told(&night_ferry, &mut Burst::default(), now))).collect();
        for t in &told {
            assert_eq!(t, &serde_json::json!({ "rev": 140, "ids": [{ "id": "al-42", "op": "changed" }] }));
        }
        let merged = RecordsChanged { rev: 141, ids: vec![RecordChanged { id: "al-21".into(), op: "merged", into: Some("al-20".into()) }] };
        assert_eq!(data(records_told(&merged, &mut Burst::default(), now))["ids"][0], serde_json::json!({ "id": "al-21", "op": "merged", "into": "al-20" }));
    }

    /// LV12 (kind 4): past 200 ids in a second, "records stale" in place of the ids, once that second; the next second, ids again.
    /// Never 11,000 ids in one message.
    #[test]
    fn a_burst_tells_records_stale() {
        let start = std::time::Instant::now();
        let mut burst = Burst::default();
        assert_eq!(data(records_told(&changed(100, 0..150), &mut burst, start))["ids"].as_array().unwrap().len(), 150);
        let stale = data(records_told(&changed(101, 150..11_000), &mut burst, start + std::time::Duration::from_millis(300)));
        assert_eq!(stale, serde_json::json!({ "rev": 101, "stale": "record" }));
        assert!(records_told(&changed(102, 11_000..11_010), &mut burst, start + std::time::Duration::from_millis(600)).is_none(), "told once that second");
        let later = data(records_told(&changed(103, 0..5), &mut burst, start + std::time::Duration::from_millis(1_200)));
        assert_eq!(later["ids"].as_array().unwrap().len(), 5, "a new second: ids again");
        // One burst alone past the limit: stale, never its ids.
        assert_eq!(data(records_told(&changed(104, 0..11_000), &mut Burst::default(), start))["stale"], "record");
    }
}
