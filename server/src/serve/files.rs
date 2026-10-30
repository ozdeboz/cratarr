//! Sending a song's file (`stream`, `download`) with byte ranges, so players can seek and resume. The range handling
//! follows nghe's `http/header.rs` and `http/binary` (https://github.com/vnghia/nghe, AGPL-3.0, Copyright vnghia),
//! translated: one range per request (`bytes=a-b`, `bytes=a-`, `bytes=-n`), 206 with `Content-Range`, 416 when it lies
//! outside the file, an ETag from the file's size and time. Changed: the body is sent through a channel, as Cratarr's
//! own stream is, read from disk in pieces (the container's memory is small; a FLAC can be 100 MB).

use std::path::Path;

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Bytes read per piece.
const PIECE: usize = 256 * 1024;

/// A requested byte range of a file of `len` bytes, as (first, last) inclusive; `Err` when it cannot be met.
pub fn range(header: Option<&str>, len: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(spec) = header.and_then(|h| h.trim().strip_prefix("bytes=")) else { return Ok(None) };
    // Several ranges: answer the whole file, as many servers do.
    if spec.contains(',') || len == 0 {
        return Ok(None);
    }
    let (a, b) = spec.split_once('-').ok_or(())?;
    let (a, b) = (a.trim(), b.trim());
    let (first, last) = match (a.is_empty(), b.is_empty()) {
        (false, true) => (a.parse::<u64>().map_err(|_| ())?, len - 1),
        (false, false) => (a.parse::<u64>().map_err(|_| ())?, b.parse::<u64>().map_err(|_| ())?.min(len - 1)),
        (true, false) => {
            let n = b.parse::<u64>().map_err(|_| ())?.min(len);
            if n == 0 {
                return Err(());
            }
            (len - n, len - 1)
        }
        (true, true) => return Err(()),
    };
    if first > last || first >= len {
        return Err(());
    }
    Ok(Some((first, last)))
}

pub async fn serve_file(path: &Path, headers: &HeaderMap, download: bool) -> Response {
    let Ok(meta) = tokio::fs::metadata(path).await else { return StatusCode::NOT_FOUND.into_response() };
    let len = meta.len();
    let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
    let etag = crate::covers::etag(modified, len);
    let suffix = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let mut out = HeaderMap::new();
    out.insert(header::CONTENT_TYPE, HeaderValue::from_static(super::api::content_type(suffix)));
    out.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Ok(v) = HeaderValue::from_str(&etag) {
        out.insert(header::ETAG, v);
    }
    if download {
        let name = path.file_name().map(|n| n.to_string_lossy().replace('"', "'")).unwrap_or_default();
        if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", name.replace(|c: char| !c.is_ascii(), "_"))) {
            out.insert(header::CONTENT_DISPOSITION, v);
        }
    }
    let wanted = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let (status, first, last) = match range(wanted, len) {
        Ok(Some((a, b))) => {
            if let Ok(v) = HeaderValue::from_str(&format!("bytes {a}-{b}/{len}")) {
                out.insert(header::CONTENT_RANGE, v);
            }
            (StatusCode::PARTIAL_CONTENT, a, b)
        }
        Ok(None) => (StatusCode::OK, 0, len.saturating_sub(1)),
        Err(()) => {
            if let Ok(v) = HeaderValue::from_str(&format!("bytes */{len}")) {
                out.insert(header::CONTENT_RANGE, v);
            }
            return (StatusCode::RANGE_NOT_SATISFIABLE, out).into_response();
        }
    };
    let count = if len == 0 { 0 } else { last - first + 1 };
    out.insert(header::CONTENT_LENGTH, HeaderValue::from(count));
    let Ok(mut file) = tokio::fs::File::open(path).await else { return StatusCode::NOT_FOUND.into_response() };
    if first > 0 && file.seek(std::io::SeekFrom::Start(first)).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(4);
    tokio::spawn(async move {
        let mut left = count;
        let mut buf = vec![0u8; PIECE];
        while left > 0 {
            let want = (left as usize).min(PIECE);
            match file.read(&mut buf[..want]).await {
                Ok(0) => break,
                Ok(n) => {
                    left -= n as u64;
                    if tx.send(Ok(Bytes::copy_from_slice(&buf[..n]))).await.is_err() {
                        return; // the player went away
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            }
        }
    });
    (status, out, Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_as_players_ask_for_them() {
        assert_eq!(range(None, 100), Ok(None));
        assert_eq!(range(Some("bytes=0-"), 100), Ok(Some((0, 99))));
        assert_eq!(range(Some("bytes=10-19"), 100), Ok(Some((10, 19))));
        assert_eq!(range(Some("bytes=90-200"), 100), Ok(Some((90, 99))), "the end is clipped");
        assert_eq!(range(Some("bytes=-10"), 100), Ok(Some((90, 99))), "the last ten");
        assert_eq!(range(Some("bytes=100-"), 100), Err(()), "past the end");
        assert_eq!(range(Some("bytes=5-2"), 100), Err(()));
        assert_eq!(range(Some("bytes=0-1,5-6"), 100), Ok(None), "several: the whole file");
    }

    #[tokio::test]
    async fn a_range_sends_just_those_bytes() {
        let dir = std::env::temp_dir().join(format!("cratarr-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("t.mp3");
        std::fs::write(&f, (0u8..200).collect::<Vec<_>>()).unwrap();
        let mut h = HeaderMap::new();
        h.insert(header::RANGE, HeaderValue::from_static("bytes=10-19"));
        let r = serve_file(&f, &h, false).await;
        assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(r.headers()[header::CONTENT_RANGE], "bytes 10-19/200");
        assert_eq!(r.headers()[header::CONTENT_TYPE], "audio/mpeg");
        let body = axum::body::to_bytes(r.into_body(), 1000).await.unwrap();
        assert_eq!(body.to_vec(), (10u8..20).collect::<Vec<_>>());
        let whole = serve_file(&f, &HeaderMap::new(), true).await;
        assert_eq!(whole.status(), StatusCode::OK);
        assert!(whole.headers()[header::CONTENT_DISPOSITION].to_str().unwrap().contains("t.mp3"));
        assert_eq!(axum::body::to_bytes(whole.into_body(), 1000).await.unwrap().len(), 200);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
