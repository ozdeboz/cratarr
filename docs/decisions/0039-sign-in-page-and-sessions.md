# 0039: Cratarr's own sign-in page and sessions

- Status: accepted (the owner, 2026-10-17)
- Date: 2026-10-17
- Builds on: 0030 (Cratarr's accounts; the proxy's forward auth as the fallback login), 0031 (the app; its 2026-10-16
  amendment "Signed in away from home" is replaced by this)

## Context

Away from home the reverse proxy let a request through only with a Cratarr account, as HTTP Basic: the browser's own
login box, and in the app a User name and Password in Settings → Network, kept on the device and sent with every
request. The owner asked (2026-10-17) whether credentials belong in a settings panel at all, and proposed a sign-in
form that a request without a valid session is sent to; to be built "aligned with recommended standard best
practices".

What Basic left wrong: the password kept in plain text on each device and sent with every request (one request seen is
the password); no signing out, nor signing one device out without changing the password for all; the login asked
before Settings could even load; and the browser's challenge-and-retry, whose 401s got the owner's phone banned by the
proxy's CrowdSec (2026-10-16).

## Decision

Cratarr gets its own sign-in page and per-device sessions, as the OWASP Authentication and Session Management cheat
sheets advise. The proxy still decides who is asked: away from home only (0030); Cratarr's pages, Home Assistant, Kodi
and the app need no sign-in on the home network, where plain-HTTP clients could not hold a secure cookie anyway.

- **The sign-in page** (`GET /signin`, server-rendered): a plain form password managers fill in, needing no script;
  Cratarr's colours and icon. Its headers: a strict Content-Security-Policy (nonce for its one small script and its
  style, `form-action 'self'`, `frame-ancestors 'none'`), `Cache-Control: no-store`, `X-Frame-Options: DENY`.
- **Signing in** (`POST /signin`): the account is checked in one place (`Serve::sign_in`, constant time, password as
  typed); an unknown account and a wrong password get the same answer. Tries are throttled per client and per account:
  ten failures in fifteen minutes make either wait (429 with Retry-After; failures 401), both visible in the proxy's log
  to CrowdSec. The Origin must be this site's. Each sign-in is a new session; one the browser held ends. After it, the
  browser goes back where it was (`next`: a path on this site only, never another site), its `#route` too.
- **Sessions:** a 256-bit token from the operating system's random source, given to the browser as a cookie
  `__Host-cratarr-session` (`Secure; HttpOnly; SameSite=Lax; Path=/`, no Domain; `cratarr-session` without `Secure`
  over plain HTTP). Only the token's SHA-256 is kept, in `sessions.sqlite` (owner-only, outside the user data and its
  backups). A session ends after 90 days unused or 400 days after its sign-in (the cookie's Max-Age, browsers' own
  limit), at sign-out (`POST /signout`), when signed out from Settings → Authentication (the list of signed-in devices,
  each named from its browser), and, for all of an account's other devices, when its password changes.
- **The proxy's check** (`GET /api/auth/check`, for Caddy's `forward_auth` and the like) accepts the session and no
  longer Basic. It lets through by itself the sign-in page, the web app's manifest, service worker and icons, and the
  OpenSubsonic API (its own logins), so a proxy can send everything to it. Without a session, a page load is sent to
  the sign-in page (303); anything else gets 401 with a `Cookie` challenge, which no browser answers with a login box.
- **The page** goes to the sign-in page on Cratarr's challenge, once, and comes back after; its service worker never
  serves the sign-in page nor swallows the redirect to it.
- **The app** keeps no password: Settings → Network loses User name and Password, and those kept before are deleted.
  Away from home its WebView shows the sign-in page like any page; the app's own requests (streams, downloads, Play
  On) carry the WebView's session cookie. A refusal still holds them back ten minutes, or until the page signs in again.
- **Accounts stay Cratarr's own** (0030), the fallback that stays when a directory comes.

Options not taken:
- Keeping Basic at the proxy, with the app's stored login: all of the above stays wrong.
- A token the page keeps in its storage and sends in a header: page scripts can read it; an `HttpOnly` cookie is the
  advised home for a browser session.
- Cratarr enforcing sign-in on every request itself: at home, plain-HTTP clients (Home Assistant's `/api/library`,
  Kodi's streams, the app on the local network address) cannot hold a secure cookie; which network needs a sign-in is
  the proxy's to say, as 0030 had it.

## Consequences

- No password on any device; a device is signed out on its own, from Settings → Authentication.
- No browser login box away from home; the sign-in page instead, the place the page was in kept.
- Brute force meets the throttle and, through the proxy's log, CrowdSec; ordinary use no longer makes 401 storms (the
  page leaves for the sign-in page on the first, the app waits).
- The accounts' passwords themselves stay as 0030 keeps them (readable, in an owner-only file): the Subsonic API's
  token login needs them. OpenSubsonic API keys would let them be kept as hashes; not now.
- A lost phone's session works until it is signed out in Settings → Authentication, or 90 days unused.

## Follow-ups

- A directory (LLDAP, the owner's set-up, 2026-10-17): asked first at `Serve::sign_in`,
  Cratarr's own account the fallback; a successful bind could refresh a local verifier so the fallback works while the
  directory is down (no runtime dependency, 0030). Subsonic logins stay Cratarr's own accounts or API keys.
- The proxy's stopgap for the old app (a 503 to it away from home) goes once the app with this sign-in is on the
  owner's devices.
