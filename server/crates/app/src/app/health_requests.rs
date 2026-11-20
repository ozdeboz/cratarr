//! Health's checks of the requests (decision 0063), beside them (moved from `app::health`, which keeps their names): the
//! fetchers out of reach, Soulseek's records held, its queue set aside, the requests waiting for an answer, and the disk floor.

use super::health_findings::{Finding, ITEMS, Item, Level, Open, finding, n};

/// The fetchers out of reach for over an hour (X1: a dead fetcher raises an alert), each named with since when. None, no finding.
/// Pure.
pub fn fetchers_down(down: &[(&'static str, i64)], now: i64) -> Option<Finding> {
    if down.is_empty() {
        return None;
    }
    let name = |f: &str| if f == "lidarr" { "Lidarr" } else { "Soulseek (slskd)" };
    let count = down.len() as u64;
    let mut f = finding("requests", "requests:fetcher-down", Level::Warning, count, n(count, "fetcher can't be reached", "fetchers can't be reached"));
    f.items = down.iter().map(|(fetcher, since)| Item { title: name(fetcher).to_string(), line: Some(match (now - since) / 60 {
                m if m < 120 => format!("for {m} min"),
                m => format!("for {} hours", m / 60),
            }), ..Item::default() }).collect();
    Some(f)
}

/// Soulseek's records held a day for its own disk's trouble (Y9): slskd could not store their files three times in a row (its disk
/// full or read-only, its folder not writable or gone: U1), so their users are not asked again every five minutes. `held` as `soulseek::cant_store`
/// gives them, the latest held first: its why is the detail. None held, no finding. Pure.
pub fn soulseek_cant_store(held: &[(String, String)]) -> Option<Finding> {
    soulseek_held("requests:soulseek-store", "can't store files", held)
}

/// Soulseek's records held a day because their placing failed three times in a row (Y12): the Incoming library gone or not writable,
/// a name it cannot take, every folder another record's. `held` as `soulseek::cant_place` gives them, the latest held first: its
/// why is the detail. None held, no finding. Pure.
pub fn soulseek_cant_place(held: &[(String, String)]) -> Option<Finding> {
    soulseek_held("requests:soulseek-place", "can't place albums", held)
}

/// Soulseek's records held a day for one trouble (Y9, Y12): "Soulseek {state}", a warning, as it tries again by itself; the latest
/// held record's why as its detail, and each held record listed. None held, no finding. Pure.
fn soulseek_held(key: &str, state: &str, held: &[(String, String)]) -> Option<Finding> {
    let (_, why) = held.first()?;
    let count = held.len() as u64;
    let mut f = finding("requests", key, Level::Warning, count, format!("Soulseek {state}"));
    f.name = "Soulseek".to_string();
    (f.thing, f.state) = (Some(f.name.clone()), Some(state.into()));
    f.detail = Some(why.clone());
    f.items = held.iter().map(|(record, _)| Item { title: record.clone(), line: Some("held a day".into()), ..Item::default() }).collect();
    Some(f)
}

/// Soulseek's queue as the starts left it (P7, W7; said in the log alone before): one that could not be read and was set aside,
/// read from the data folder (`soulseek::queues_set_aside`), so it is said after a restart too, until the file set aside is put back or
/// removed: a warning, its detail how to put it back. And one that could be neither read nor set aside at this start (`frozen`),
/// to which nothing is written: critical. Blocking.
pub fn soulseek_queue(data: &std::path::Path, frozen: bool) -> Vec<Finding> {
    let mut out = Vec::new();
    if frozen {
        // No count: it is kept or not ("Soulseek queue · not kept").
        let mut f = finding("requests", "requests:soulseek-frozen", Level::Critical, 0, "Soulseek's queue can't be kept");
        f.detail = Some(
            "soulseek.json in the data folder could not be read nor set aside, so Cratarr writes nothing to it, and the queue is not \
             kept over a restart. Make the data folder writable, or move the file out of it, and start Cratarr again."
                .into(),
        );
        out.push(f);
    }
    let aside = super::soulseek::queues_set_aside(data);
    if let Some((latest, _)) = aside.first() {
        let count = aside.len() as u64;
        let mut f = finding("requests", "requests:soulseek-set-aside", Level::Warning, count, "Soulseek's queue was set aside");
        f.detail = Some(format!(
            "It could not be read at a start, so Cratarr set it aside as {latest} and began an empty queue. To put it back: stop \
             Cratarr, rename {latest} in the data folder to soulseek.json, and start Cratarr. Or remove {latest}."
        ));
        f.items = aside.iter().map(|(name, at)| Item { title: name.clone(), line: Some("set aside".into()), since: *at, ..Item::default() }).collect();
        out.push(f);
    }
    out
}

/// Requests waiting for an answer (decision 0070), each named, opening the Requests page; the fetchers out of reach (X1);
/// Soulseek's records held for its own disk's trouble (Y9), or for their placing failing (Y12); and Soulseek's queue set aside or
/// not kept (P7).
pub(super) fn check_requests(served: Option<&crate::serve::Serve>) -> anyhow::Result<Vec<Finding>> {
    let now = super::moment::now_secs();
    let queue = served.and_then(|s| s.data_dir.as_deref()).map(|d| soulseek_queue(d, super::soulseek::frozen())).unwrap_or_default();
    let down: Vec<Finding> = fetchers_down(&super::requests::fetchers_down(now), now)
        .into_iter()
        .chain(soulseek_cant_store(&super::soulseek::cant_store(now)))
        .chain(soulseek_cant_place(&super::soulseek::cant_place(now)))
        .chain(queue)
        .collect();
    let Some(serve) = served else { return Ok(down.into_iter().collect()) };
    // Those whose state says they wait for an answer (W5): one whose album the library has meanwhile waits for no one. Each by its
    // record's names (C49b): a label's "Artist - Title" is the artist's title.
    let mut waiting = super::requests::waiting(serve)?;
    if waiting.is_empty() {
        return Ok(down.into_iter().collect());
    }
    let count = waiting.len() as u64;
    waiting.truncate(ITEMS);
    let mut f = finding("requests", "requests:waiting", Level::Info, count, format!("{} waiting", n(count, "request", "requests")));
    f.open = Some(Open { page: "requests", switch: None, thing: None });
    f.items = waiting.into_iter().map(|r| Item { title: r.album, line: Some(format!("{} · asked by {}", r.artist, r.user)), ..Item::default() }).collect();
    Ok(std::iter::once(f).chain(down).collect())
}

/// The disk floor (AL03; Q6, OQ-F5): the server short of space, one warning for the whole server, never one per user: its detail the
/// free space where users' music lands, the floor, and what the trash there holds, as emptying it frees that; opening Settings. None
/// while it is not short. What cratarr-ha reads is `/api/health`'s flag; Cratarr calls no one. Blocking.
pub(crate) fn short_of_space(served: &crate::serve::Serve, disks: &dyn super::room::Disks) -> Option<Finding> {
    use super::room::{MARGIN, size_text};
    let space = super::room::space_on(served, disks);
    if !space.short {
        return None;
    }
    // No count: it is short or not ("Disk space · short").
    let mut f = finding("server", "server:short-of-space", Level::Warning, 0, "The server is short of space");
    let mut detail = format!(
        "{} free, below the floor of {}. Users' new Wants wait until {} is free.",
        size_text(space.free.unwrap_or(0)),
        size_text(space.floor),
        size_text(space.floor.saturating_add(MARGIN))
    );
    let trash = super::room::in_the_trash(served).unwrap_or_else(|e| {
        tracing::warn!(target: "room", reason = %format!("{e:#}"), "what the trash holds was not read");
        0
    });
    if trash > 0 {
        detail.push_str(&format!(" The trash holds {}: emptying it frees that.", size_text(trash)));
    }
    f.detail = Some(detail);
    f.open = Some(Open { page: "settings", switch: None, thing: None });
    Some(f)
}
