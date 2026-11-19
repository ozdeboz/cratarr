//! Discover's Around artists you play (0.9.7, the owner: nine seeds "will get me very repetitive results still"): the
//! related artists (`app::related`) of three of the account's artists at a time, an album each, taken from every artist
//! it has played (with a MusicBrainz id), the most played first. Refresh moves on to the three most played not shown
//! yet; once every one has been, the round starts again. The group after the one showing is looked up ahead (related
//! artists and their albums are kept), so Refresh shows it at once and Discover's look waits on none of them. Kept per
//! account on the server, so a reload or another device goes on where it was.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::discover::Suggestion;

/// The seeds a group has.
const GROUP: usize = 3;
/// How long Refresh waits for its group before the page is told it is on its way (and asks again).
const WAIT: Duration = Duration::from_secs(5);
/// How long a group being made waits for the look ahead of the same account to finish (it looks up the same seeds).
const AHEAD_WAIT: Duration = Duration::from_secs(180);

/// An account's Around, as kept (`UserData::discover_around`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// The seeds shown this round (MusicBrainz ids), the group showing among them.
    #[serde(default)]
    pub shown: Vec<String>,
    /// The group showing: its seeds (name, MusicBrainz id), and their suggestions once made.
    #[serde(default)]
    pub seeds: Vec<(String, String)>,
    #[serde(default)]
    pub albums: Option<Vec<Suggestion>>,
    /// The group after it, looked up ahead.
    #[serde(default)]
    pub next: Vec<(String, String)>,
}

/// The accounts whose group showing is being made, and those whose next group is being looked up.
static MAKING: Mutex<Option<HashSet<String>>> = Mutex::new(None);
static AHEAD: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// One change of an account's State at a time (read, changed, kept).
static CHANGING: Mutex<()> = Mutex::new(());

fn claim(set: &Mutex<Option<HashSet<String>>>, account: &str) -> bool {
    set.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashSet::new).insert(account.to_string())
}

fn release(set: &Mutex<Option<HashSet<String>>>, account: &str) {
    if let Some(s) = set.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        s.remove(account);
    }
}

fn busy(set: &Mutex<Option<HashSet<String>>>, account: &str) -> bool {
    set.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|s| s.contains(account))
}

fn kept(serve: &crate::serve::Serve, account: &str) -> Result<State> {
    Ok(serve.userdata.discover_around(account)?.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default())
}

/// The account's State read, changed by `change` and kept, one change at a time.
fn change<T>(serve: &crate::serve::Serve, account: &str, change: impl FnOnce(&mut State) -> T) -> Result<(State, T)> {
    let _one = CHANGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut st = kept(serve, account)?;
    let out = change(&mut st);
    serve.userdata.set_discover_around(account, &serde_json::to_string(&st)?)?;
    Ok((st, out))
}

/// The account's seeds: every artist it has played with a MusicBrainz id, the most played first.
fn pool(artists: &[super::discover::OwnArtist]) -> Vec<(String, String)> {
    artists.iter().filter(|a| a.plays > 0).filter_map(|a| Some((a.name.clone(), a.mbid.clone()?))).collect()
}

/// The next group of `pool` (the most played first): the most played neither shown this round nor `showing`; where
/// fewer are left, the round starts again and the group is filled from the top, the group showing last of all.
fn pick(pool: &[(String, String)], shown: &[String], showing: &[(String, String)]) -> Vec<(String, String)> {
    let in_showing = |s: &(String, String)| showing.iter().any(|x| x.1 == s.1);
    let mut group: Vec<(String, String)> = pool.iter().filter(|s| !shown.contains(&s.1) && !in_showing(s)).take(GROUP).cloned().collect();
    for pass in [false, true] {
        for s in pool {
            if group.len() >= GROUP {
                return group;
            }
            if !group.contains(s) && (pass || !in_showing(s)) {
                group.push(s.clone());
            }
        }
    }
    group
}

/// The seeds shown once `group` shows: theirs added, after the round began again where it holds one shown already.
fn shown_with(shown: &[String], group: &[(String, String)]) -> Vec<String> {
    let mut out = if group.iter().any(|g| shown.contains(&g.1)) { Vec::new() } else { shown.to_vec() };
    out.extend(group.iter().map(|g| g.1.clone()));
    out
}

/// The account's own artists and what the library has; none where it keeps no play history (its seeds are its plays).
async fn own(app: &Arc<super::App>, serve: &Arc<crate::serve::Serve>, account: &str) -> Result<Option<(super::discover::Have, Vec<super::discover::OwnArtist>)>> {
    let (ud, a) = (serve.userdata.clone(), account.to_string());
    if !tokio::task::spawn_blocking(move || ud.tracking(&a)).await? {
        return Ok(None);
    }
    Ok(Some(super::discover::own(serve, app.library.catalogue(), account).await?))
}

/// The group showing, and whether it is still being made (the page asks again). One is picked and made where none is.
pub async fn showing(app: &Arc<super::App>, serve: &Arc<crate::serve::Serve>, account: &str) -> Result<(Vec<String>, Vec<Suggestion>, bool)> {
    let Some((_, artists)) = own(app, serve, account).await? else { return Ok(Default::default()) };
    let pool = pool(&artists);
    let (s, a) = (serve.clone(), account.to_string());
    let (st, _) = tokio::task::spawn_blocking(move || {
        change(&s, &a, |st| {
            if st.seeds.is_empty() {
                let group = pick(&pool, &st.shown, &[]);
                (st.shown, st.seeds, st.albums) = (shown_with(&st.shown, &group), group, None);
            }
        })
    })
    .await??;
    if st.albums.is_none() && !st.seeds.is_empty() {
        make(app.clone(), serve.clone(), account.to_string());
    } else {
        ahead(app.clone(), serve.clone(), account.to_string());
    }
    answered(serve, account, st).await
}

/// Refresh: on to the next group (the one looked up ahead), shown once made, waiting a few seconds for it.
pub async fn next(app: &Arc<super::App>, serve: &Arc<crate::serve::Serve>, account: &str) -> Result<(Vec<String>, Vec<Suggestion>, bool)> {
    let Some((_, artists)) = own(app, serve, account).await? else { return Ok(Default::default()) };
    let pool = pool(&artists);
    let (s, a) = (serve.clone(), account.to_string());
    tokio::task::spawn_blocking(move || {
        change(&s, &a, |st| {
            // The one looked up ahead, unless a seed of it is no longer the account's (its plays deleted).
            let ahead = std::mem::take(&mut st.next);
            let group = if !ahead.is_empty() && ahead.iter().all(|s| pool.contains(s)) { ahead } else { pick(&pool, &st.shown, &st.seeds) };
            (st.shown, st.seeds, st.albums) = (shown_with(&st.shown, &group), group, None);
        })
    })
    .await??;
    make(app.clone(), serve.clone(), account.to_string());
    let until = tokio::time::Instant::now() + WAIT;
    loop {
        let (s, a) = (serve.clone(), account.to_string());
        let st = tokio::task::spawn_blocking(move || kept(&s, &a)).await??;
        if st.albums.is_some() || tokio::time::Instant::now() >= until {
            return answered(serve, account, st).await;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// The group's seeds' names and its suggestions not answered since (as `discover::kept` reads the look's), and whether
/// they are still being made.
async fn answered(serve: &Arc<crate::serve::Serve>, account: &str, st: State) -> Result<(Vec<String>, Vec<Suggestion>, bool)> {
    let making = st.albums.is_none() && !st.seeds.is_empty();
    let (ud, a) = (serve.userdata.clone(), account.to_string());
    let (answered, no_artists, everywhere) = tokio::task::spawn_blocking(move || super::discover::leaves_out(&ud, &a)).await??;
    let mut albums = st.albums.unwrap_or_default();
    albums.retain(|s| !answered.contains(&s.key) && !no_artists.contains(&s.artist_key) && !super::discover::excluded_everywhere(s, &everywhere));
    Ok((st.seeds.into_iter().map(|(name, _)| name).collect(), albums, making))
}

/// The group showing's suggestions as kept, for what is done with one of them (Want, Hide, Not interested).
pub fn suggestions(serve: &crate::serve::Serve, account: &str) -> Result<Vec<Suggestion>> {
    Ok(kept(serve, account)?.albums.unwrap_or_default())
}

/// The group showing made in the background (after the look ahead, which asks for the same seeds), and kept; then the
/// next group looked up ahead.
fn make(app: Arc<super::App>, serve: Arc<crate::serve::Serve>, account: String) {
    if !claim(&MAKING, &account) {
        return;
    }
    tokio::spawn(async move {
        let until = tokio::time::Instant::now() + AHEAD_WAIT;
        while busy(&AHEAD, &account) && tokio::time::Instant::now() < until {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        if let Err(e) = make_now(&app, &serve, &account).await {
            tracing::info!("discover: around artists not made: {e:#}");
        }
        release(&MAKING, &account);
        ahead(app, serve, account);
    });
}

async fn make_now(app: &Arc<super::App>, serve: &Arc<crate::serve::Serve>, account: &str) -> Result<()> {
    let (s, a) = (serve.clone(), account.to_string());
    let seeds = tokio::task::spawn_blocking(move || kept(&s, &a)).await??.seeds;
    if seeds.is_empty() {
        return Ok(());
    }
    let Some((have, _)) = own(app, serve, account).await? else { return Ok(()) };
    let candidates = super::discover::from_related(serve, &super::discover::http()?, &seeds, &|_, _, _| {}).await;
    let (ud, a) = (serve.userdata.clone(), account.to_string());
    let (answered, no_artists, everywhere) = tokio::task::spawn_blocking(move || super::discover::leaves_out(&ud, &a)).await??;
    let mut albums = super::discover::one_list(candidates, &|c| have.has(c), &have.known, &answered, &no_artists);
    albums.retain(|s| !super::discover::excluded_everywhere(s, &everywhere));
    super::cover_cache::warm(albums.iter().filter_map(|s| s.cover.clone()).collect());
    let (s, a) = (serve.clone(), account.to_string());
    tokio::task::spawn_blocking(move || {
        change(&s, &a, |st| {
            // Kept for the group it was made for only (another Refresh may have moved on meanwhile).
            if st.seeds == seeds {
                st.albums = Some(albums);
            }
        })
    })
    .await??;
    Ok(())
}

/// The group after the one showing picked and looked up in the background (its related artists and their albums kept,
/// so making it later asks nothing).
fn ahead(app: Arc<super::App>, serve: Arc<crate::serve::Serve>, account: String) {
    if busy(&MAKING, &account) || !claim(&AHEAD, &account) {
        return;
    }
    tokio::spawn(async move {
        let looked = async {
            let Some((_, artists)) = own(&app, &serve, &account).await? else { return Ok(()) };
            let pool = pool(&artists);
            let (s, a) = (serve.clone(), account.clone());
            let (st, _) = tokio::task::spawn_blocking(move || {
                change(&s, &a, |st| {
                    if st.next.is_empty() {
                        st.next = pick(&pool, &st.shown, &st.seeds);
                    }
                })
            })
            .await??;
            // Asked again where it was picked before: what was kept is read (nothing asked), what was not is looked up.
            if !st.next.is_empty() {
                super::discover::from_related(&serve, &super::discover::http()?, &st.next, &|_, _, _| {}).await;
            }
            anyhow::Ok(())
        };
        if let Err(e) = looked.await {
            tracing::info!("discover: around artists' next group not looked up: {e:#}");
        }
        release(&AHEAD, &account);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(n: usize) -> Vec<(String, String)> {
        (1..=n).map(|i| (format!("A{i}"), format!("m{i}"))).collect()
    }

    fn ids(g: &[(String, String)]) -> Vec<&str> {
        g.iter().map(|(_, m)| m.as_str()).collect()
    }

    #[test]
    fn each_refresh_takes_the_most_played_not_shown_and_the_round_starts_again_once_all_were() {
        let p = pool(7);
        let mut shown = Vec::new();
        let mut showing: Vec<(String, String)> = Vec::new();
        let mut seen = Vec::new();
        for _ in 0..3 {
            let g = pick(&p, &shown, &showing);
            shown = shown_with(&shown, &g);
            seen.push(ids(&g).join(","));
            showing = g;
        }
        assert_eq!(seen, vec!["m1,m2,m3", "m4,m5,m6", "m7,m1,m2"], "the last one, then the round again from the top");
        assert_eq!(shown, vec!["m7", "m1", "m2"], "the new round counts from there");
        let g = pick(&p, &shown, &showing);
        assert_eq!(ids(&g), vec!["m3", "m4", "m5"]);
    }

    #[test]
    fn a_small_pool_shows_what_there_is() {
        let p = pool(3);
        let g = pick(&p, &[], &[]);
        assert_eq!(ids(&g), vec!["m1", "m2", "m3"]);
        let shown = shown_with(&[], &g);
        assert_eq!(ids(&pick(&p, &shown, &g)), vec!["m1", "m2", "m3"], "the same three, there being no others");
        assert!(pick(&[], &[], &[]).is_empty());
    }
}
