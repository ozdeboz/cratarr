//! `GET /api/auth/check`: whether a request's HTTP Basic login is one of Cratarr's accounts (the Settings page's
//! Authentication section, decision 0030). It is meant for a reverse proxy's forward auth (Caddy's `forward_auth`,
//! Traefik's `forwardAuth`, nginx's `auth_request`): the proxy asks it before passing a request on, and answers the
//! browser with its 401 and `WWW-Authenticate`, so the browser asks for the login. Cratarr's own pages have no sign-in
//! of their own yet; this is the fallback login that stays when an external directory is added later.
//!
//! 204 for a known account and its password, else 401. The password goes through the same check as a Subsonic
//! login's `p` (constant time, the same work for an unknown user).

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use super::Serve;
use super::users::Users;

pub fn router(serve: Arc<Serve>) -> Router {
    Router::new().route("/api/auth/check", get(check)).with_state(serve)
}

async fn check(State(serve): State<Arc<Serve>>, headers: HeaderMap) -> Response {
    match login_of(headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()), &serve.users) {
        Some(name) => (StatusCode::NO_CONTENT, [("x-cratarr-user", HeaderValue::from_str(&name).unwrap_or(HeaderValue::from_static("")))]).into_response(),
        None => (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, HeaderValue::from_static("Basic realm=\"Cratarr\", charset=\"UTF-8\""))]).into_response(),
    }
}

/// The account an `Authorization: Basic` value logs in as.
fn login_of(authorization: Option<&str>, users: &Users) -> Option<String> {
    let (scheme, encoded) = authorization?.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let decoded = String::from_utf8(base64(encoded.trim())?).ok()?;
    let (name, password) = decoded.split_once(':')?;
    // As a Subsonic `p` in its `enc:` hex form, so the password is compared exactly as typed, whatever it holds.
    let hex: String = password.bytes().map(|b| format!("{b:02x}")).collect();
    users.check(Some(name), None, None, Some(&format!("enc:{hex}"))).ok().map(|u| u.name)
}

/// Standard base64 (RFC 4648, padding optional).
fn base64(s: &str) -> Option<Vec<u8>> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let s = s.trim_end_matches('=').as_bytes();
    if s.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for &c in s {
        acc = (acc << 6) | u32::from(value(c)?);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decodes_as_rfc_4648_says() {
        assert_eq!(base64("").unwrap(), b"");
        assert_eq!(base64("Zg==").unwrap(), b"f");
        assert_eq!(base64("Zm9vYg").unwrap(), b"foob");
        assert_eq!(base64("Zm9vYmFy").unwrap(), b"foobar");
        assert!(base64("Zm9v!").is_none());
    }

    #[test]
    fn only_an_account_and_its_password_get_in() {
        let users = Users::open(None).unwrap();
        // The RFC 7617 example, "Aladdin:open sesame"; then a password with a colon and one that looks like `enc:`.
        users.set("Aladdin", "open sesame").unwrap();
        assert_eq!(login_of(Some("Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ=="), &users).as_deref(), Some("Aladdin"));
        assert_eq!(login_of(Some("basic QWxhZGRpbjpvcGVuIHNlc2FtZQ"), &users).as_deref(), Some("Aladdin"));
        assert_eq!(login_of(Some("Basic QWxhZGRpbjpvcGVuIHNlc2FtZT8="), &users), None, "a wrong password");
        assert_eq!(login_of(Some("Basic TmlnaHQ6b3BlbiBzZXNhbWU="), &users), None, "an unknown user");
        assert_eq!(login_of(Some("Bearer QWxhZGRpbjpvcGVuIHNlc2FtZQ=="), &users), None);
        assert_eq!(login_of(None, &users), None);
        users.set("Aladdin", "a:b").unwrap();
        assert_eq!(login_of(Some("Basic QWxhZGRpbjphOmI="), &users).as_deref(), Some("Aladdin"));
        users.set("Aladdin", "enc:41").unwrap();
        assert_eq!(login_of(Some("Basic QWxhZGRpbjplbmM6NDE="), &users).as_deref(), Some("Aladdin"));
        assert_eq!(login_of(Some("Basic QWxhZGRpbjpB"), &users), None, "\"A\" is not the password \"enc:41\"");
    }

    #[test]
    fn with_no_account_nobody_gets_in() {
        let users = Users::open(None).unwrap();
        assert_eq!(login_of(Some("Basic QWxhZGRpbjo="), &users), None);
    }
}
