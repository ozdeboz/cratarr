//! Listens into an account's history (decision 0067), the one routine for every import's listens: the part of `import` that
//! takes the server, which sits above the matcher (SPLIT-PLAN P6).

use std::collections::HashMap;

use anyhow::Result;

use super::{Counter, Matcher, Tally, Track};

/// A listen an import brings into the history (decision 0067): its track, when, and its own id there.
pub struct Listened<'a> {
    pub track: Track<'a>,
    pub at: i64,
    pub event: String,
    /// The player it was heard on, where the source knows: else [`into_history`]'s, the source's own name.
    pub player: Option<&'a str>,
}

/// Another listen of the same song this near (seconds) is the same listen (a scrobbler that fed two services).
const SAME: i64 = 60;

/// Listens into `account`'s history, logged as heard from `context` (`lastfm`, `file:spotify`, …): finished, not
/// counted (counts come from a server's import), `player` named. One the history has already, from anywhere (the same
/// song within a minute), is not added twice. For a preview, nothing written. Blocking.
pub fn into_history(
    serve: &crate::serve::Serve,
    songs: &Matcher,
    account: &str,
    context: &str,
    player: &str,
    listens: &[Listened],
    preview: bool,
) -> Result<Tally> {
    let mut heard: HashMap<String, Vec<i64>> = HashMap::new();
    for l in serve.userdata.heard_since(account, 0)? {
        heard.entry(l.song).or_default().push(l.at);
    }
    let near = |heard: &HashMap<String, Vec<i64>>, song: &str, at: i64| heard.get(song).is_some_and(|ts| ts.iter().any(|t| (t - at).abs() <= SAME));
    let mut c = Counter::default();
    for l in listens {
        let Some(song) = songs.find(&l.track) else {
            c.missed(&l.track);
            continue;
        };
        if near(&heard, song, l.at) {
            c.found(false);
            continue;
        }
        if !preview {
            let Some((pid, album, artists)) = serve.library.play_targets(song)? else {
                c.missed(&l.track);
                continue;
            };
            let h = crate::userdata::Heard {
                at: l.at,
                heard_secs: l.track.length_secs,
                length_secs: l.track.length_secs,
                ended: crate::userdata::Ended::Finished,
                counts: false,
                player: Some(l.player.unwrap_or(player).to_string()),
                output: None,
                context: Some(context.to_string()),
                event: Some(l.event.clone()),
                followed: false,
            };
            if !serve.userdata.record(account, (song, &pid), &album, &artists, &h)? {
                c.found(false);
                continue;
            }
        }
        heard.entry(song.to_string()).or_default().push(l.at);
        c.found(true);
    }
    Ok(c.tally())
}
