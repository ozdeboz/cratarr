//! Scrobbling from the device or through the server (0.15, decision 0091): each account's choice, the plays of other
//! servers' tracks a device hands the server to send, and the keys going once the server has sent what it held.
//!
//! The choice is the account's setting `scrobble` (`device` or `server`). Never set, it is `server` for an account that
//! has a service connected on the server (so no one's scrobbling stops unannounced) and `device` for any other (the
//! private choice, the start). Under `device` nothing new is queued for the account; what was queued is sent, and each
//! service's keys go once its part of the queue is empty (`keys_sent`).

use anyhow::Result;
use rusqlite::{Connection, params};

use super::UserData;

pub const SCHEMA: &str = "
-- Other servers' plays a device handed over (decision 0091), by the device's id for each, per account, and when they
-- came (epoch s): a play sent again is queued once.
CREATE TABLE IF NOT EXISTS scrobble_seen (
  user TEXT NOT NULL, id TEXT NOT NULL, at INTEGER NOT NULL, PRIMARY KEY (user, id)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS scrobble_seen_at ON scrobble_seen (at);
";

/// Last.fm takes a scrobble at most 14 days late; ListenBrainz any time.
pub const LASTFM_LATEST_SECS: i64 = 14 * 24 * 3600;
/// How far ahead of the server's clock a device's play may say it was (clocks differ a little).
pub const AHEAD_SECS: i64 = 300;
/// At most this many plays in one hand-over.
pub const MOST: usize = 500;

/// Where an account's plays are scrobbled from (decision 0091).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Way {
    /// Each device tells the services itself; the server holds no keys for the account.
    Device,
    /// The server sends, signed in once for all devices (decision 0068).
    Server,
}

/// A play of another server's track, handed over by a device (`POST /api/scrobbles`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Named {
    /// The device's id for the play (decision 0087), kept per account so it is queued once ("now playing" has none).
    #[serde(default, skip_serializing)]
    pub id: String,
    /// When it was heard (epoch s).
    #[serde(default, skip_serializing)]
    pub at: i64,
    pub artist: String,
    pub title: String,
    #[serde(default)]
    pub album: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
    /// The recording's MusicBrainz id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbid: Option<String>,
}

/// What became of a hand-over.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Handed {
    /// Queued now.
    pub queued: usize,
    /// Handed over before: queued once already.
    pub again: usize,
    /// Never sendable: no artist or title, or heard in the future.
    pub refused: usize,
}

/// The queue's column for a play the library does not know (`named`: JSON, `Named` without its id and time), added to
/// a queue from before it. Safe to repeat.
pub(super) fn add_named(conn: &Connection) -> Result<()> {
    let has: bool = conn.prepare("SELECT 1 FROM pragma_table_info('scrobble_queue') WHERE name = 'named'")?.exists([])?;
    if !has {
        conn.execute_batch("ALTER TABLE scrobble_queue ADD COLUMN named TEXT")?;
    }
    Ok(())
}

/// SQL true where account `?1` scrobbles from each device (its setting, or no setting and no service connected).
pub(super) const FROM_DEVICE: &str = "coalesce(
    (SELECT json_extract(settings, '$.scrobble') FROM user_settings WHERE user = ?1),
    CASE WHEN EXISTS (SELECT 1 FROM scrobble_target WHERE user = ?1) THEN 'server' ELSE 'device' END) = 'device'";

impl UserData {
    /// Where `user`'s plays are scrobbled from.
    pub fn scrobble_way(&self, user: &str) -> Result<Way> {
        let device: bool = self.db().query_row(&format!("SELECT {FROM_DEVICE}"), [user], |r| r.get(0))?;
        Ok(if device { Way::Device } else { Way::Server })
    }

    /// Other servers' plays handed over by a device, queued for each service the account connected (Last.fm's only
    /// within its 14 days), each once by its id. Refused (none queued) unless the account scrobbles through the server
    /// with tracking on: the caller asks first.
    pub fn hand_over(&self, user: &str, plays: &[Named], now: i64) -> Result<Handed> {
        let mut handed = Handed::default();
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut seen = tx.prepare_cached("INSERT OR IGNORE INTO scrobble_seen (user, id, at) VALUES (?1, ?2, ?3)")?;
            let mut queue = tx.prepare_cached(
                "INSERT INTO scrobble_queue (user, target, song, at, named)
                 SELECT ?1, target, '', ?2, ?3 FROM scrobble_target
                 WHERE user = ?1 AND broken IS NULL AND off = 0 AND (target != 'lastfm' OR ?2 >= ?4)",
            )?;
            for p in plays {
                if p.id.trim().is_empty() || p.artist.trim().is_empty() || p.title.trim().is_empty() || p.at > now + AHEAD_SECS {
                    handed.refused += 1;
                    continue;
                }
                if seen.execute(params![user, p.id, now])? == 0 {
                    handed.again += 1;
                    continue;
                }
                queue.execute(params![user, p.at, serde_json::to_string(p)?, now - LASTFM_LATEST_SECS])?;
                handed.queued += 1;
            }
        }
        tx.commit()?;
        Ok(handed)
    }

    /// The services whose keys went for accounts that scrobble from each device, once the server sent what it held
    /// for them (decision 0091: the queue first, then the keys, for good). As (account, service).
    pub fn keys_sent(&self) -> Result<Vec<(String, String)>> {
        let mut db = self.db();
        let tx = db.transaction()?;
        let gone: Vec<(String, String)> = {
            let mut st = tx.prepare(
                "SELECT t.user, t.target FROM scrobble_target t
                 WHERE NOT EXISTS (SELECT 1 FROM scrobble_queue q WHERE q.user = t.user AND q.target = t.target)",
            )?;
            let all = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            let mut device = tx.prepare(&format!("SELECT {FROM_DEVICE}"))?;
            let mut out = Vec::new();
            for (user, target) in all {
                if device.query_row([&user], |r| r.get::<_, bool>(0))? {
                    out.push((user, target));
                }
            }
            out
        };
        for (user, target) in &gone {
            tx.execute("DELETE FROM scrobble_target WHERE user = ?1 AND target = ?2", params![user, target])?;
        }
        tx.commit()?;
        Ok(gone)
    }

    /// Forgets the hand-overs' ids older than `cut` (epoch s).
    pub(super) fn forget_old_scrobble_seen(conn: &Connection, cut: i64) -> Result<usize> {
        Ok(conn.execute("DELETE FROM scrobble_seen WHERE at < ?1", [cut])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(id: &str, at: i64) -> Named {
        Named { id: id.into(), at, artist: "Air".into(), title: "Talisman".into(), album: "Moon Safari".into(), album_artist: None, duration_secs: Some(256), mbid: None }
    }

    fn settings(u: &UserData, user: &str, s: serde_json::Value) {
        let (base, _) = u.settings_of(user).unwrap();
        u.put_settings(user, base, &s).unwrap().unwrap();
    }

    #[test]
    fn an_account_scrobbles_from_its_devices_unless_it_has_a_service_on_the_server_or_chose_so() {
        let u = UserData::open(None).unwrap();
        assert_eq!(u.scrobble_way("me").unwrap(), Way::Device, "a new account: the private choice");
        u.set_scrobble_target("me", "listenbrainz", "me", "{}").unwrap();
        assert_eq!(u.scrobble_way("me").unwrap(), Way::Server, "a service connected before: its scrobbling goes on");
        settings(&u, "me", serde_json::json!({ "scrobble": "device" }));
        assert_eq!(u.scrobble_way("me").unwrap(), Way::Device, "chosen");
        settings(&u, "me", serde_json::json!({ "scrobble": "server" }));
        assert_eq!(u.scrobble_way("me").unwrap(), Way::Server);
    }

    #[test]
    fn a_hand_over_queues_each_play_once_per_service_and_last_fm_only_within_its_14_days() {
        let u = UserData::open(None).unwrap();
        u.set_scrobble_target("me", "listenbrainz", "me", "{}").unwrap();
        u.set_scrobble_target("me", "lastfm", "me", "{}").unwrap();
        let now = 2_000_000_000;
        let old = now - LASTFM_LATEST_SECS - 60;
        let mut nameless = play("c", now);
        nameless.title = " ".into();
        let h = u.hand_over("me", &[play("a", now - 60), play("b", old), nameless, play("d", now + AHEAD_SECS + 1)], now).unwrap();
        assert_eq!(h, Handed { queued: 2, again: 0, refused: 2 });
        let due = u.due_scrobbles(i64::MAX, 10).unwrap();
        let to: Vec<(&str, i64)> = due.iter().map(|q| (q.target.as_str(), q.at)).collect();
        assert_eq!(to.len(), 3, "a to both, b to ListenBrainz only: {to:?}");
        assert!(to.contains(&("listenbrainz", old)) && !to.contains(&("lastfm", old)));
        let named: Named = serde_json::from_str(due[0].named.as_deref().unwrap()).unwrap();
        assert_eq!((named.artist.as_str(), named.title.as_str()), ("Air", "Talisman"));
        assert_eq!(u.hand_over("me", &[play("a", now - 60)], now).unwrap(), Handed { queued: 0, again: 1, refused: 0 }, "sent again: once");
        assert_eq!(u.due_scrobbles(i64::MAX, 10).unwrap().len(), 3);
        assert_eq!(u.hand_over("you", &[play("a", now - 60)], now).unwrap().queued, 1, "ids are per account");
    }

    #[test]
    fn a_counted_play_is_queued_through_the_server_and_not_from_the_device_but_counted_either_way() {
        let u = UserData::open(None).unwrap();
        u.set_scrobble_target("me", "listenbrainz", "me", "{}").unwrap();
        assert!(u.record("me", ("tr-1", "pid1"), "al-1", &[], &super::super::Heard::counted(500)).unwrap());
        assert_eq!(u.due_scrobbles(i64::MAX, 10).unwrap().len(), 1, "through the server");
        settings(&u, "me", serde_json::json!({ "scrobble": "device" }));
        assert!(u.record("me", ("tr-2", "pid2"), "al-1", &[], &super::super::Heard::counted(600)).unwrap());
        assert_eq!(u.due_scrobbles(i64::MAX, 10).unwrap().len(), 1, "from the device: nothing new for the server to send");
        assert_eq!(u.play_stats("me", &["tr-2".to_string()]).unwrap().get("tr-2"), Some(&(1, Some(600))), "still counted");
    }

    #[test]
    fn switched_to_the_device_nothing_new_is_queued_and_the_keys_go_once_the_queue_is_sent() {
        let u = UserData::open(None).unwrap();
        u.set_scrobble_target("me", "listenbrainz", "me", "{}").unwrap();
        u.set_scrobble_target("me", "lastfm", "me", "{}").unwrap();
        u.hand_over("me", &[play("a", 1_000)], 2_000).unwrap();
        settings(&u, "me", serde_json::json!({ "scrobble": "device" }));
        assert!(u.keys_sent().unwrap().is_empty(), "two plays still to go");
        let lb: Vec<i64> = u.due_scrobbles(i64::MAX, 10).unwrap().iter().filter(|q| q.target == "listenbrainz").map(|q| q.id).collect();
        u.scrobbled(&lb).unwrap();
        assert_eq!(u.keys_sent().unwrap(), vec![("me".to_string(), "listenbrainz".to_string())]);
        assert_eq!(u.scrobble_targets("me").unwrap().len(), 1, "Last.fm's keys wait for its play");
        u.scrobbled(&u.due_scrobbles(i64::MAX, 10).unwrap().iter().map(|q| q.id).collect::<Vec<_>>()).unwrap();
        assert_eq!(u.keys_sent().unwrap().len(), 1);
        assert!(u.scrobble_targets("me").unwrap().is_empty(), "the keys gone for good");
        assert_eq!(u.scrobble_way("me").unwrap(), Way::Device);
    }
}
