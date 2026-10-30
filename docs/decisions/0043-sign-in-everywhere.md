# 0043: Sign-in everywhere

- Status: accepted (the owner, 2026-10-17: "Yes, to sign in everywhere")
- Date: 2026-10-17
- Builds on: 0039 (the sign-in page and sessions; this amends its "no sign-in at home"), 0040 (sign-in through a
  directory), 0031 (the app), 0030 (the Subsonic API and its own logins)

## Context

Since 0039 Cratarr has its own sign-in page and sessions, but only the reverse proxy asks for them, and only away from
home. Anything that reaches Cratarr on the home network (its own address, `http://<nas>:8081`, or the public name
through the router) gets in without signing in. The owner (2026-10-17): "We should first require a sign-in everywhere."

Not everything that talks to Cratarr is a person at a page. Home Assistant calls its API, the bedroom Kodi plays its
links (Play On, 0031: Cratarr hands Kodi `/api/file/…` addresses, and in a group a sound stream from its own listener),
the apps' players fetch streams, scripts call it, and Docker asks its health. Subsonic apps already sign in with their
own logins (0030).

## Decision

**Cratarr itself requires a sign-in on every request**, from anywhere, whatever address it comes to. The proxy's check
(0039) stays as it is in front of it, but no longer decides who gets in.

Three ways in, nothing else:

1. **A session** from Cratarr's sign-in page (0039, through the directory or Cratarr's own accounts, 0040): browsers
   and the app's pages. A session belongs to one address (browsers keep cookies per address): a device signs in once
   on each address it uses. The owner keeps both of the app's addresses (the public one and the local network one,
   0031), so the app signs in on each it uses: its page shows the sign-in page there once.
2. **An API key**, for what cannot sign in on a page (Home Assistant, scripts): one per client, made in Settings →
   Authentication with a name, shown once when made, kept only as its SHA-256 (as sessions are), sent as
   `Authorization: Bearer <key>`, listed with when it was last used, and revocable one by one. A key acts as the owner,
   as everything at home did until now.
3. **A signed link**, for players that cannot send a header (Kodi's playlist, the group's sound stream): Cratarr signs
   the addresses it hands them (HMAC-SHA256 over the path and an expiry, with a secret kept in the data folder),
   valid for a day. A link is good for that path only, and only until it expires.

Without one of these, a page load goes to the sign-in page (303, and back after) and anything else gets 401 with
Cratarr's `Cookie` challenge, as away from home now.

**Open without a sign-in, and only these:** the sign-in page and signing out; what the sign-in page shows (its icons;
the web app's manifest and service worker); `/api/health` (Docker's check, the app's address check and Home
Assistant's outage alert; it tells only that Cratarr runs and how many playlists it has); `/api/auth/check` (the
proxy's question, which only answers whether a request may pass); `/rest` (the Subsonic API checks its own logins);
and, while Cratarr has no account at all, everything (there is nothing to sign in with: the first account is made on
its page, and from then on the requirement holds). No address is trusted, the NAS's own included.

**What calls Cratarr today** (a survey, 2026-10-17): Home Assistant, 13 `rest_command`s
(players and their commands, playlists by name, albums and their tags and review, Incoming, tracks and their
favourite) every few minutes and on demand, and its health sensor each minute; Kodi through its links; the apps;
Docker's health check; one-off analysis imports run on the NAS (`/api/analysis/import`), which get a key of their
own. Cratarr's webhook to Home Assistant goes the other way and is unchanged.

**The app** signs in through its page as a browser does, on each of its addresses; its own requests (streams,
downloads, Play On) carry the session of the address in use. Its player is a device like any other in Settings →
Authentication, named for what it is: the app adds a marker with its player's name ("Cratarr app · Salon tablet").

**Rollout, without breaking anything:** built with the requirement off; then Home Assistant gets its key (its calls
changed in its configuration), Kodi's links are signed (nothing to do on Kodi), and the owner's devices sign in;
then the requirement is switched on (Settings → Authentication), and stays on. A new install has it on from the start,
its first account made on the page.

## Options not taken

- **Trusted addresses at home** (Home Assistant's and Kodi's addresses let through): simpler, but anything that takes
  or fakes such an address gets in, and nothing can be cut off on its own.
- **One address for the app** (the public one at home too, through the router): one sign-in per device, but the owner
  keeps the local network address (Prefer local network, 0031).
- **Enforcing at the proxy only:** Cratarr's own address on the home network bypasses the proxy.

## Consequences

- Nobody on the home network (a guest's phone, a compromised device) gets at the library, the settings or the players
  without signing in.
- Home Assistant's calls carry a key (its configuration changes once); a key that leaks is revoked alone.
- Each device signs in once per address it uses; sessions last as 0039 has them (90 days unused, 400 at most).
- Kodi's links stop working a day after they were handed out: a long queue is handed out again as it plays.
- Settings → Authentication grows: API keys under the signed-in devices.

## The work

- Cratarr's side: the requirement for every route, API keys, signed links, the page, the app. The requirement wraps
  the whole router in one place, so the OpenSubsonic routes are covered without changing them, `/rest` excepted.
- Home Assistant's calls with the key (a header on each `rest_command`, the key in its secrets), and the proxy, if it
  changes. The analysis imports' key.
- The devices list's naming and times (asked for the same night): with this, in the same pass.

## Amendment (2026-10-17): sessions.sqlite is backed up

It holds the API keys (their hashes), the signed links' secret and whether the requirement is on, not only sessions
(0039 left it out of the backups on purpose). Without it a restore would come back with the requirement reset to off
for an install from before and every key unknown, so the backups dump it with the other databases from now on; it holds
hashes only.

## Amendment (2026-10-24): keys live with what uses them

The owner: API keys belong in the panel of what uses them, not in Authentication. The only client that needs one is
Home Assistant, so Settings → Connections has a Home Assistant section: when it last called (its key's last use), the
weather it last told, and its key, made (with the password again, shown once), and revoked there. Each key carries a
purpose (`home_assistant`); the key named "Home Assistant" from before took that purpose when the column came. There is
no list of other clients. Authentication keeps the sign-in requirement and the directory.
