//! The Opus copies the Android app streams on mobile data (the owner, 2026-10-24: kept on the NAS, so the app can seek
//! in them): a lossless track's copy (`opus_copy::copy`) made once, kept in the data folder's `opus-copies`, and served
//! as a file, with its length and byte ranges. Downloads of smaller copies take the same.
//!
//! A copy is made at the encoder's own pace into its file, apart from whoever reads it: a request while it is being
//! made follows the file as it grows. That answer has no length, so the play it starts cannot seek; the app keeps the
//! next tracks ahead, whole, so theirs can. At most [`CAP`] bytes are kept; the copies served longest ago go first.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio::sync::watch;

/// What the copies may take together on disk.
pub const CAP: u64 = 5 << 30;
/// How long a request waits for a copy being made before following it as it grows: a short track is done by then, and
/// served whole (seekable).
const WAIT: Duration = Duration::from_secs(2);

/// How far a copy being made has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Made {
    /// Bytes in its file so far.
    Writing(u64),
    Done,
    Failed,
}

pub struct OpusCopies {
    dir: PathBuf,
    cap: u64,
    making: Mutex<HashMap<String, watch::Receiver<Made>>>,
}

impl OpusCopies {
    /// The copies in `data_dir`'s `opus-copies` (half-made ones from before a restart thrown away); none without a data
    /// folder.
    pub fn open(data_dir: Option<&str>) -> Option<Arc<Self>> {
        Self::open_in(PathBuf::from(data_dir?).join("opus-copies"), CAP)
    }

    fn open_in(dir: PathBuf, cap: u64) -> Option<Arc<Self>> {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!("Opus copies: {}: {e}", dir.display());
            return None;
        }
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            if entry.path().extension().is_some_and(|e| e == "part") {
                std::fs::remove_file(entry.path()).ok();
            }
        }
        Some(Arc::new(OpusCopies { dir, cap, making: Mutex::new(HashMap::new()) }))
    }

    /// A lossless file's copy: kept, made now, or followed while it is being made. `None` for a file that is not
    /// lossless (or whose copy failed): it is sent as it is.
    pub async fn serve(self: &Arc<Self>, path: PathBuf, gains: (Option<f64>, Option<f64>), headers: &HeaderMap) -> Option<Response> {
        let meta = tokio::fs::metadata(&path).await.ok()?;
        let mtime = meta.modified().ok().and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        let key = key(&path, meta.len(), mtime, gains);
        let kept = self.dir.join(format!("{key}.opus"));
        if tokio::fs::metadata(&kept).await.is_ok() {
            return Some(self.send_kept(&kept, headers).await);
        }
        let mut made = self.start(&key, path, gains).await?;
        let _ = tokio::time::timeout(WAIT, made.wait_for(|m| !matches!(m, Made::Writing(_)))).await;
        let now = *made.borrow();
        match now {
            Made::Done => Some(self.send_kept(&kept, headers).await),
            Made::Failed => None,
            Made::Writing(_) => Some(follow(self.dir.join(format!("{key}.part")), made).await),
        }
    }

    /// A kept copy as a file; served now, so it is the last to go.
    async fn send_kept(&self, kept: &Path, headers: &HeaderMap) -> Response {
        let k = kept.to_path_buf();
        let _ = tokio::task::spawn_blocking(move || std::fs::File::options().write(true).open(&k).and_then(|f| f.set_modified(SystemTime::now()))).await;
        crate::serve::files::serve_file(kept, headers, false).await
    }

    /// The copy's progress: one already being made, else one started now. `None` for a file that is not lossless.
    async fn start(self: &Arc<Self>, key: &str, path: PathBuf, gains: (Option<f64>, Option<f64>)) -> Option<watch::Receiver<Made>> {
        if let Some(made) = self.making.lock().unwrap_or_else(|e| e.into_inner()).get(key) {
            return Some(made.clone());
        }
        let probe = path.clone();
        let lossless = tokio::task::spawn_blocking(move || crate::convert::Source::open(&probe).ok().flatten().is_some()).await.ok()?;
        if !lossless {
            return None;
        }
        let part = self.dir.join(format!("{key}.part"));
        let (begun, made) = {
            let mut making = self.making.lock().unwrap_or_else(|e| e.into_inner());
            // Started by another while this one probed.
            if let Some(made) = making.get(key) {
                return Some(made.clone());
            }
            // The file is there before anyone follows it.
            let file = std::fs::File::create(&part).ok()?;
            let (tx, made) = watch::channel(Made::Writing(0));
            making.insert(key.to_string(), made.clone());
            ((tx, file), made)
        };
        let (me, key) = (self.clone(), key.to_string());
        tokio::spawn(async move {
            let (tx, file) = begun;
            let permit = crate::opus_copy::COPIES.acquire().await;
            let (p, k) = (part.clone(), me.dir.join(format!("{key}.opus")));
            let tx = Arc::new(tx);
            let progress = tx.clone();
            let done = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
                let out = Progress { file, written: 0, tx: progress };
                let made = crate::opus_copy::copy(&path, out, gains)?;
                if made {
                    std::fs::rename(&p, &k)?;
                }
                Ok(made)
            })
            .await;
            drop(permit);
            let ok = matches!(done, Ok(Ok(true)));
            if !ok {
                if let Ok(Err(e)) = &done {
                    tracing::warn!("Opus copy {key}: {e:#}");
                }
                std::fs::remove_file(&part).ok();
            }
            let _ = tx.send(if ok { Made::Done } else { Made::Failed });
            me.making.lock().unwrap_or_else(|e| e.into_inner()).remove(&key);
            if ok {
                let me = me.clone();
                let _ = tokio::task::spawn_blocking(move || me.prune()).await;
            }
        });
        Some(made)
    }

    /// The copies over [`CAP`] together let go, those served longest ago first, down to nine tenths of it.
    fn prune(&self) {
        let mut copies: Vec<(SystemTime, u64, PathBuf)> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "opus"))
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                Some((m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len(), e.path()))
            })
            .collect();
        let mut total: u64 = copies.iter().map(|c| c.1).sum();
        if total <= self.cap {
            return;
        }
        copies.sort();
        for (_, len, path) in copies {
            if total <= self.cap / 10 * 9 {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }
}

/// A copy's name: its file as it is now (path, size, time) and the gains written into it.
fn key(path: &Path, size: u64, mtime: u64, gains: (Option<f64>, Option<f64>)) -> String {
    let mut h = Sha256::new();
    h.update(format!("{}\0{size}\0{mtime}\0{:?}\0{}", path.display(), gains, crate::opus_copy::BITRATE));
    h.finalize().iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// The copy's file, written as it is made, telling how far it has come.
struct Progress {
    file: std::fs::File,
    written: u64,
    tx: Arc<watch::Sender<Made>>,
}

impl Write for Progress {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.file.write(buf)?;
        self.written += n as u64;
        self.tx.send_replace(Made::Writing(self.written));
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

/// A copy being made, sent as it grows (no length: the page or app reads it to its end).
async fn follow(part: PathBuf, mut made: watch::Receiver<Made>) -> Response {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(4);
    // Opened now: it keeps reading once the copy is moved to its kept name.
    let file = tokio::fs::File::open(&part).await;
    tokio::spawn(async move {
        let Ok(mut file) = file else {
            let _ = tx.send(Err(std::io::Error::other("the copy went"))).await;
            return;
        };
        let mut buf = vec![0u8; 64 * 1024];
        let mut sent = 0u64;
        loop {
            let now = *made.borrow_and_update();
            if now == Made::Failed {
                // Broken off, so the half copy is not taken for a whole one.
                let _ = tx.send(Err(std::io::Error::other("the copy failed"))).await;
                return;
            }
            let ready = match now {
                Made::Writing(n) => n,
                _ => u64::MAX,
            };
            if sent < ready {
                match file.read(&mut buf).await {
                    Ok(0) if now == Made::Done => return,
                    Ok(0) => {}
                    Ok(n) => {
                        if tx.send(Ok(axum::body::Bytes::copy_from_slice(&buf[..n]))).await.is_err() {
                            return;
                        }
                        sent += n as u64;
                        continue;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        return;
                    }
                }
            }
            if made.changed().await.is_err() {
                // The maker is gone: what it wrote is all there is.
                if *made.borrow() != Made::Done {
                    let _ = tx.send(Err(std::io::Error::other("the copy stopped"))).await;
                    return;
                }
            }
        }
    });
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/ogg"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (headers, axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-opus-copies-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    async fn body(r: Response) -> Vec<u8> {
        axum::body::to_bytes(r.into_body(), usize::MAX).await.unwrap().to_vec()
    }

    #[tokio::test]
    async fn a_copy_is_made_once_then_served_whole_with_ranges() {
        let d = dir("once");
        let music = d.join("a.wav");
        crate::opus_copy::tests::wav(&music, 44_100, 3.0);
        let copies = OpusCopies::open_in(d.join("copies"), CAP).unwrap();
        let first = copies.serve(music.clone(), (None, None), &HeaderMap::new()).await.unwrap();
        let first = body(first).await;
        assert_eq!(&first[..4], b"OggS");
        // Kept: the same bytes, now with a length and ranges.
        let kept: Vec<_> = std::fs::read_dir(d.join("copies")).unwrap().flatten().map(|e| e.path()).collect();
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(kept[0].extension().unwrap(), "opus");
        let again = copies.serve(music.clone(), (None, None), &HeaderMap::new()).await.unwrap();
        assert_eq!(again.headers()[header::CONTENT_LENGTH].to_str().unwrap(), first.len().to_string());
        assert_eq!(body(again).await, first);
        let mut range = HeaderMap::new();
        range.insert(header::RANGE, HeaderValue::from_static("bytes=100-199"));
        let part = copies.serve(music.clone(), (None, None), &range).await.unwrap();
        assert_eq!(part.status(), axum::http::StatusCode::PARTIAL_CONTENT);
        assert_eq!(body(part).await, first[100..200].to_vec());
        // Other gains are another copy.
        copies.serve(music.clone(), (Some(-3.0), None), &HeaderMap::new()).await.unwrap();
        assert_eq!(std::fs::read_dir(d.join("copies")).unwrap().count(), 2);
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn a_lossy_file_gets_none_and_the_copies_keep_under_their_cap() {
        let d = dir("cap");
        let lossy = d.join("a.mp3");
        std::fs::write(&lossy, b"not lossless").unwrap();
        let copies = OpusCopies::open_in(d.join("copies"), 1).unwrap();
        assert!(copies.serve(lossy, (None, None), &HeaderMap::new()).await.is_none());
        // Two copies over a cap of one byte: after each is made, the older goes.
        for (name, secs) in [("a.wav", 2.0), ("b.wav", 2.5)] {
            let music = d.join(name);
            crate::opus_copy::tests::wav(&music, 44_100, secs);
            body(copies.serve(music, (None, None), &HeaderMap::new()).await.unwrap()).await;
        }
        let left: Vec<_> = std::fs::read_dir(d.join("copies")).unwrap().flatten().collect();
        assert!(left.len() <= 1, "{left:?}");
        // A half copy from before a restart is thrown away.
        std::fs::write(d.join("copies").join("x.part"), b"half").unwrap();
        OpusCopies::open_in(d.join("copies"), CAP).unwrap();
        assert!(!d.join("copies").join("x.part").exists());
        std::fs::remove_dir_all(&d).ok();
    }

    #[tokio::test]
    async fn one_being_made_is_followed_as_it_grows() {
        let d = dir("follow");
        let part = d.join("k.part");
        std::fs::write(&part, b"").unwrap();
        let (tx, made) = watch::channel(Made::Writing(0));
        let r = follow(part.clone(), made).await;
        assert!(r.headers().get(header::CONTENT_LENGTH).is_none(), "no length while it is made");
        let writer = tokio::spawn(async move {
            let mut f = std::fs::File::options().append(true).open(&part).unwrap();
            for piece in [&b"OggS one "[..], b"two ", b"three"] {
                tokio::time::sleep(Duration::from_millis(30)).await;
                f.write_all(piece).unwrap();
                tx.send_replace(Made::Writing(f.metadata().unwrap().len()));
            }
            tx.send_replace(Made::Done);
        });
        assert_eq!(body(r).await, b"OggS one two three");
        writer.await.unwrap();
        std::fs::remove_dir_all(&d).ok();
    }
}
