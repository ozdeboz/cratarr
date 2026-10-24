//! Artists' descriptions and photos (the owner, 2026-10-23: Home's Artists to explore, and "slowly build our database
//! with this data"): from TheAudioDB with its public key, as Jellyfin reads it with its own. A builder works through the
//! library's artists at ten a minute while nobody asks for one; one asked for (Home's open artist, not read yet) is read
//! at once. Kept in `artist-info.sqlite`, the photo on disk beside it; read again after a few months, one not found
//! sooner. Personal use: the server reads it for its own listeners and publishes nothing.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::library::epoch_now;

/// TheAudioDB's public key (its documented free one; Jellyfin's plugin carries a key of its own).
const KEY: &str = "123";
const BASE: &str = "https://www.theaudiodb.com/api/v1/json";
/// The builder's pace while nobody asks: ten a minute (the owner, 2026-10-23); TheAudioDB allows thirty.
const EVERY: Duration = Duration::from_secs(6);
/// Read again after this long; one TheAudioDB did not know sooner.
const FOUND_KEEP: i64 = 90 * 86_400;
const MISSING_KEEP: i64 = 30 * 86_400;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS artist_info (
  key TEXT PRIMARY KEY, name TEXT NOT NULL, mbid TEXT, adb TEXT, bio TEXT, photo TEXT, read_at INTEGER NOT NULL);";

/// What is known of an artist.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Info {
    pub bio: Option<String>,
    /// The photo's file in the photos folder, when there is one.
    #[serde(skip)]
    pub photo: Option<String>,
}

pub struct ArtistInfo {
    db: Mutex<Connection>,
    photos: PathBuf,
    http: reqwest::Client,
    /// One read at a time, the builder's and an asked one's alike.
    turn: tokio::sync::Mutex<()>,
}

/// An artist's key: the MusicBrainz id where known, else the name as written, in lower case.
pub fn key(name: &str, mbid: Option<&str>) -> String {
    mbid.filter(|m| !m.is_empty()).map_or_else(|| format!("name:{}", name.to_lowercase()), |m| format!("mbid:{m}"))
}

impl ArtistInfo {
    /// Opens (or makes) the store in `data_dir`; none without a data folder.
    pub fn open(data_dir: Option<&str>) -> Option<Arc<Self>> {
        let dir = PathBuf::from(data_dir?);
        let open = || -> Result<Self> {
            let db = Connection::open(dir.join("artist-info.sqlite"))?;
            db.execute_batch(SCHEMA)?;
            let photos = dir.join("artist-photos");
            std::fs::create_dir_all(&photos)?;
            Ok(Self { db: Mutex::new(db), photos, http: reqwest::Client::builder().timeout(Duration::from_secs(20)).build()?, turn: Default::default() })
        };
        match open() {
            Ok(s) => Some(Arc::new(s)),
            Err(e) => {
                tracing::warn!(target: "artists", reason = %e, "the artist store could not be opened");
                None
            }
        }
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// What is known, if it was read.
    pub fn get(&self, key: &str) -> Option<Info> {
        self.db()
            .query_row("SELECT bio, photo FROM artist_info WHERE key = ?1", [key], |r| Ok(Info { bio: r.get(0)?, photo: r.get(1)? }))
            .optional()
            .ok()
            .flatten()
    }

    /// The photo's path on disk.
    pub fn photo_path(&self, file: &str) -> PathBuf {
        self.photos.join(file)
    }

    /// Whether `key` wants reading: never read, or read too long ago.
    fn due(&self, key: &str, now: i64) -> bool {
        let row: Option<(i64, bool)> = self
            .db()
            .query_row("SELECT read_at, (bio IS NOT NULL OR photo IS NOT NULL) FROM artist_info WHERE key = ?1", [key], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .ok()
            .flatten();
        match row {
            None => true,
            Some((at, found)) => now - at > if found { FOUND_KEEP } else { MISSING_KEEP },
        }
    }

    /// Reads one artist from TheAudioDB (by MusicBrainz id where known, else by name) and keeps what it says.
    pub async fn read(&self, name: &str, mbid: Option<&str>) -> Result<Info> {
        let _turn = self.turn.lock().await;
        let key = key(name, mbid);
        let url = match mbid.filter(|m| !m.is_empty()) {
            Some(m) => format!("{BASE}/{KEY}/artist-mb.php?i={}", urlencode(m)),
            None => format!("{BASE}/{KEY}/search.php?s={}", urlencode(name)),
        };
        let answer: serde_json::Value = self.http.get(&url).send().await?.error_for_status()?.json().await?;
        let found = answer["artists"].as_array().and_then(|a| a.first()).cloned().unwrap_or_default();
        let text = |field: &str| found[field].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        // English is the plain field; the others carry their language.
        let bio = text("strBiography").or_else(|| text("strBiographyEN"));
        let adb = text("idArtist");
        let mut photo = None;
        if let (Some(id), Some(thumb)) = (&adb, text("strArtistThumb")) {
            // A smaller copy where TheAudioDB has one, else the original.
            let file = format!("{id}.jpg");
            for u in [format!("{thumb}/medium"), thumb.clone()] {
                if let Ok(r) = self.http.get(&u).send().await
                    && r.status().is_success()
                    && let Ok(bytes) = r.bytes().await
                    && !bytes.is_empty()
                {
                    std::fs::write(self.photos.join(&file), &bytes)?;
                    photo = Some(file);
                    break;
                }
            }
        }
        self.db().execute(
            "INSERT INTO artist_info (key, name, mbid, adb, bio, photo, read_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (key) DO UPDATE SET name = excluded.name, mbid = excluded.mbid, adb = excluded.adb, bio = excluded.bio,
               photo = excluded.photo, read_at = excluded.read_at",
            params![key, name, mbid, adb, bio, photo, epoch_now()],
        )?;
        Ok(Info { bio, photo })
    }

    /// What is known of an artist, read now when it never was (Home's open artist); what was known on a failure.
    pub async fn get_or_read(&self, name: &str, mbid: Option<&str>) -> Option<Info> {
        let key = key(name, mbid);
        if !self.due(&key, epoch_now()) {
            return self.get(&key);
        }
        match self.read(name, mbid).await {
            Ok(info) => Some(info),
            Err(e) => {
                tracing::debug!(target: "artists", artist = name, reason = %e, "TheAudioDB did not answer");
                self.get(&key)
            }
        }
    }

    /// The builder: one artist due every `EVERY`, the library's in turn, until it is stopped with the server.
    pub fn build_slowly(self: &Arc<Self>) {
        let me = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(EVERY).await;
                let Some(served) = crate::serve::instance() else { continue };
                let lib = served.library.clone();
                let Ok(Ok(artists)) = tokio::task::spawn_blocking(move || lib.artist_names()).await else { continue };
                let now = epoch_now();
                let next = artists.into_iter().find(|(name, mbid)| !crate::crates::various(name) && me.due(&key(name, mbid.as_deref()), now));
                let Some((name, mbid)) = next else {
                    // All read: look again in an hour.
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                    continue;
                };
                if let Err(e) = me.read(&name, mbid.as_deref()).await {
                    tracing::debug!(target: "artists", artist = %name, reason = %e, "TheAudioDB did not answer");
                    // Noted as read, found nothing: tried again in a month, not in a loop.
                    let _ = me.db().execute(
                        "INSERT INTO artist_info (key, name, mbid, read_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (key) DO UPDATE SET read_at = excluded.read_at",
                        params![key(&name, mbid.as_deref()), name, mbid, epoch_now()],
                    );
                }
            }
        });
    }
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_artist_is_known_by_its_musicbrainz_id_else_its_name() {
        assert_eq!(key("Amon Tobin", Some("630662ea")), "mbid:630662ea");
        assert_eq!(key("Amon Tobin", None), "name:amon tobin");
        assert_eq!(key("Amon Tobin", Some("")), "name:amon tobin");
        assert_eq!(urlencode("Bjørn Torske"), "Bj%C3%B8rn%20Torske");
    }

    #[test]
    fn what_was_read_is_due_again_after_months_and_sooner_when_nothing_was_found() {
        let dir = std::env::temp_dir().join(format!("cratarr-artists-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let s = ArtistInfo::open(dir.to_str()).unwrap();
        let now = epoch_now();
        assert!(s.due("mbid:x", now), "never read");
        s.db().execute("INSERT INTO artist_info (key, name, bio, read_at) VALUES ('mbid:x', 'X', 'A bio.', ?1)", [now]).unwrap();
        s.db().execute("INSERT INTO artist_info (key, name, read_at) VALUES ('name:y', 'Y', ?1)", [now - 40 * 86_400]).unwrap();
        assert!(!s.due("mbid:x", now));
        assert!(s.due("name:y", now), "nothing found forty days ago: due");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
