# 0073: Spotify as a Library source: what you keep there, what Cratarr has of it, asked for and played

- Status: accepted (the owner, 2026-10-27: connected per account; all four kinds of list; covered and missing marked;
  requested as in Discover; a missing album plays a Bandcamp preview, else on Spotify through Connect, never Spotify's
  sound through Cratarr; a Library source, no page of its own; "9.9")
- Date: 2026-10-27
- Release: 0.9.9
- Builds on: 0051 (accounts), 0067 (one import, its matcher; the Exportify CSV), 0069 (Discover, its marks), 0070
  (requests), 0071 (Bandcamp previews)

## Context

Someone coming to Cratarr keeps their music in Spotify: saved albums and tracks, playlists, followed artists. Exportify's
CSV (0067) brings a playlist in once, matched; it does not say what of someone's Spotify Cratarr has, nor keep up.

Spotify's Web API as of 2026-10 (checked on developer.spotify.com, 2026-10-27):
- **Development mode:** the app's owner needs Premium; at most 5 users, each added by hand in Spotify's dashboard; the
  quota is per developer account. An install brings its own app (Client ID), so its 5 are its own.
- **Reading:** the saved albums, saved tracks, playlists and followed artists come paged, with whole objects (no batch
  `GET /tracks|/albums|/artists` is needed, nor offered). A playlist one only follows, not one's own nor collaborative,
  cannot be read in development mode: Exportify's CSV covers it. `external_ids` (ISRC for tracks, UPC for albums) is
  back since March 2026. Search returns 10 a page.
- **Sign-in:** Authorization Code with PKCE, no client secret; the redirect address must be HTTPS (a loopback address
  aside) and listed on the app in Spotify's dashboard.

## Decision

**Each Cratarr account connects its own Spotify once; Cratarr keeps that account's Spotify library, says per item what
it has of it, and lets it be asked for or played.**

- **Connection, per account (0051), in Settings › Account › Spotify** (a page of its own: Connect, connected as, last
  read, Refresh, Disconnect): the server holds the account's Spotify sign-in (its refresh token, in the data folder,
  never logged nor sent to a page); every device signed in to the account sees it. Connecting opens Spotify's consent
  page (in the Android app, in the browser); Spotify sends the browser back to Cratarr's own HTTPS address
  (`/api/spotify/callback`; the page gives it, as Last.fm's sign-in does, since only the page knows the address it is
  on), which keeps the sign-in for the account that started it and returns to that page (the app's by
  `cratarr://connected/spotify`). Disconnecting forgets the sign-in and the lists.
- **The install's own app, in Settings › Connections › Spotify (administrators):** the Client ID, with the redirect
  address to list in Spotify's dashboard (the HTTPS one: Spotify refuses plain http but for loopback), and that its 5
  users are added there. The app's owner (Premium) need not be an administrator of Cratarr: a household member can
  make it and hand over the Client ID. No Client ID, no Spotify.
- **What is read (all four):** saved albums, saved tracks, one's own and collaborative playlists with their tracks,
  followed artists. Scopes: `user-library-read`, `playlist-read-private`, `playlist-read-collaborative`,
  `user-follow-read`, and for Connect `user-read-playback-state`, `user-modify-playback-state`.
- **Kept up in the background:** read in full on connecting, then again every 6 hours and when asked (Refresh), a page
  at a time, paced and backing off on 429 as Spotify says (`Retry-After`). Kept per account in `spotify.sqlite`.
- **What Cratarr has of it, per item:** *in the library*, *partly* (an album or playlist some of whose tracks are
  here), *missing*, or *requested* (0070, with the request's own status). Matched in this order:
  1. through MusicBrainz: the ISRC to recordings, the UPC to a release and so its release group, matched against the
     recording and release group ids the library's tags carry (paced, as the other MusicBrainz work; answers kept);
  2. by name, as 0067's matcher does for the Exportify CSV: at once, until MusicBrainz has answered.
  Reading `ISRC` and `BARCODE` from the files' own tags can come later, if MusicBrainz leaves too much to names.
  A followed artist is *in the library* when the library has any of their albums.
- **Request:** the existing flow (0070, `POST /api/requests/want`) with the album's names, its release group when
  matched, and its cover; the marks as Discover's (0069).
- **Play:**
  - covered: Cratarr's own copy, as any track of the library (a playlist plays the tracks it has);
  - a missing album: its Bandcamp preview (0071) where one is found;
  - otherwise handed to Spotify through Connect: on the account's Spotify app or speaker of choice, Premium needed. The
    sound stays in Spotify. **Never Spotify's sound through Cratarr's player.**
- **Where:** a source in the Library beside the others: its shelves and flows (albums, tracks, playlists, artists)
  with the cover marks, in the view menu once the account is connected; covered and missing narrowed by the Filter
  sheet's own switches (everything shows, marked, out of the box). No page of its own.

## Endpoints

All are the signed-in account's own; `status` is `in`, `partly`, `missing` or `requested`.

- `GET /api/spotify`: the state.
  `{ client_id_set, connected: { user, name, since } | null, sync: { state: "idle"|"running"|"failed",
  at, error, albums, tracks, playlists, artists } }`
- `PUT /api/spotify/client` (administrators): `{ client_id }`; empty removes it.
- `POST /api/spotify/connect`: `{ callback, app }` (`callback`: the page's `location.origin + "/api/spotify/callback"`;
  `app`: begun in the Android app) → `{ url }`, Spotify's consent page to open. `GET /api/spotify/callback?code&state`
  (open to anyone, found by its one-time `state`) keeps the sign-in, then sends the browser back to Settings › Account ›
  Spotify (`/#/settings?view=settings%3Aaccount%2Fspotify`; in the app `cratarr://connected/spotify`), with
  `&spotify_error=…` when it failed.
- `DELETE /api/spotify`: disconnects.
- `POST /api/spotify/sync`: reads it again now; answers at once with the state.
- `GET /api/spotify/albums?status=&q=`: `{ albums: [ { id, title, artist, year, cover, upc, added_at, status, have,
  of, album: { source, id } | null, request: Request | null, preview: { source: "bandcamp", id } | null } ] }`
- `GET /api/spotify/tracks?status=&q=`: `{ tracks: [ { id, title, artist, album, length_ms, isrc, added_at, status,
  track: { source, id } | null, request: Request | null } ] }` (`request`: the album's).
- `GET /api/spotify/playlists`: `{ playlists: [ { id, name, owner, collaborative, cover, of, have, status } ] }`;
  `GET /api/spotify/playlists/{id}`: the same with `tracks` as above, in the playlist's order.
- `GET /api/spotify/artists?status=`: `{ artists: [ { id, name, photo, status, have, artist: { id } | null } ] }`
  (`have`: their albums in the library).
- `cover` and `photo` are Cratarr's own addresses (`/api/spotify/cover/{kind}/{id}`), relayed and kept as the other
  covers are.
- `GET /api/spotify/devices`: `{ devices: [ { id, name, type, active } ] }`, the account's Spotify Connect devices.
- `POST /api/spotify/play`: `{ uri, device_id? }` plays a Spotify album, track or playlist on Spotify; 403 with
  Spotify's reason without Premium, 404 with no device.

## Consequences

- A sign-in to another service is kept per account for the first time: in the data folder, the account's by name,
  forgotten with the account or on disconnecting.
- Development mode's 5 users are the install's limit, set in Spotify's dashboard, not in Cratarr; an install that needs
  more applies to Spotify for extended quota.
- The catalogue reads two more tags (ISRC, barcode); the library's matching gets better for 0067's imports as well.
- Playlists one only follows stay with Exportify's CSV until Spotify allows reading them.
- Cratarr never plays Spotify's audio: covered is our copy, missing is a preview or Spotify itself.
