# 0073: Spotify as a Library source: what you keep there, what Cratarr has of it, and asking for the rest

- Status: accepted (the owner, 2026-10-27: per account; all four kinds of list; covered and missing marked; requested as
  in Discover; a missing album plays its Bandcamp preview; a Library source, no page of its own; "9.9". Amended the
  same day: no playing on Spotify, "insight plus requesting what's missing"; no install-wide app, each Premium user
  brings their own; and Spotify's data export for everyone, free accounts too)
- Date: 2026-10-27
- Release: 0.9.9
- Builds on: 0051 (accounts), 0067 (one import, its matcher; the Exportify CSV), 0069 (Discover, its marks), 0070
  (requests), 0071 (Bandcamp previews)

## Context

Someone coming to Cratarr keeps their music in Spotify: saved albums and tracks, playlists, followed artists. Exportify's
CSV (0067) brings a playlist in once, matched; it does not say what of someone's Spotify Cratarr has, nor keep up.

Spotify's Web API as of 2026-10 (checked on developer.spotify.com, 2026-10-27):
- **Development mode:** the app's owner needs Premium; at most 5 users, each added by hand in Spotify's dashboard; the
  quota is per developer account. The app's owner needs no adding: someone using their own app is its only user.
- **Reading:** the saved albums, saved tracks, playlists and followed artists come paged, with whole objects (no batch
  `GET /tracks|/albums|/artists` is needed, nor offered). A playlist one only follows, not one's own nor collaborative,
  cannot be read in development mode: Exportify's CSV covers it. `external_ids` (ISRC for tracks, UPC for albums) is
  back since March 2026.
- **Sign-in:** Authorization Code with PKCE, no client secret; the redirect address must be HTTPS (a loopback address
  aside) and listed on the app in Spotify's dashboard.

Cratarr's install has no Spotify Premium of its own, so it can have no app of its own. Anyone, free accounts too, can
ask Spotify for their data ("Download your data", Account data): a zip, a few days later, holding (seen in exports from
2021 to 2026) `YourLibrary.json`, its `tracks` each `{ artist, album, track, uri }` (`uri`: `spotify:track:…`), and
`Playlist1.json` (more files for many playlists), `{ playlists: [{ name, lastModifiedDate, description,
numberOfFollowers, items: [{ track: { trackName, artistName, albumName, trackUri }, episode, localTrack, addedDate }]
}] }`, beside the `StreamingHistory*.json` the import reads already (0067). Spotify's help page says the library part
also holds saved albums and artists (names, creators, URIs); their keys are read where they are.

## Decision

**Each Cratarr account's Spotify library is kept by Cratarr, from a live connection with the account's own Spotify
app, or from Spotify's data export; Cratarr says per item what it has of it, and the rest can be asked for.**

- **Two ways in, one set of lists:** whichever way they came, the account's lists (saved albums, saved tracks, its
  playlists with their tracks, followed artists) are kept per account in `spotify.sqlite`, with Spotify's own ids (an
  export's URIs carry them), and the Library and the endpoints show them alike. With both, the live connection's are
  shown; the export's are kept, and shown again once the connection is let go.
- **Live, for a Premium account, with its own Spotify app:** the account's owner makes an app in Spotify's dashboard
  (Premium needed; as its owner they need no adding) and enters its Client ID on their own page, Settings › Account ›
  Spotify (a page of its own: the Client ID with the redirect address to list on the app, Connect, connected as, last
  read, Refresh, Disconnect). The Client ID and the sign-in (its refresh token) are the account's own, in the data
  folder, never logged nor sent to a page. Connecting opens Spotify's consent page (in the Android app, in the browser);
  Spotify sends the browser back to Cratarr's own HTTPS address (`/api/spotify/callback`; the page gives it, as Last.fm's
  sign-in does, since only the page knows the address it is on), which keeps the sign-in for the account that started
  it and returns to that page (the app's by `cratarr://connected/spotify`). Scopes: `user-library-read`,
  `playlist-read-private`, `playlist-read-collaborative`, `user-follow-read`; nothing that plays.
  Read in full on connecting, then again every 6 hours and when asked (Refresh), a page at a time, paced and backing off
  on 429 as Spotify says (`Retry-After`). Disconnecting forgets the sign-in and the live lists.
- **From the export, for everyone:** the account data zip (or its `YourLibrary.json` and `Playlist*.json` alone)
  through the import (0067, Settings › Import): its listens into the history as before, and its library as the
  account's Spotify lists, a snapshot replacing the one before, previewed first as every import is. No followed
  playlists in it (Spotify leaves them out), nor ISRCs or UPCs.
- **What Cratarr has of it, per item:** *in the library*, *partly* (an album or playlist some of whose tracks are
  here), *missing*, or *requested* (0070, with the request's own status). Matched in this order:
  1. through MusicBrainz: an ISRC to recordings, a UPC to a release and so its release group, matched against the
     recording and release group ids the library's tags carry (live lists only; paced, as the other MusicBrainz work;
     answers kept);
  2. by name, as 0067's matcher does for the Exportify CSV: at once, until MusicBrainz has answered, and for an export
     throughout, MusicBrainz asked by name for an album's release group where the name alone finds none here.
  Reading `ISRC` and `BARCODE` from the files' own tags can come later, if MusicBrainz leaves too much to names.
  A followed artist is *in the library* when the library has any of their albums.
- **Request:** the existing flow (0070, `POST /api/requests/want`) with the album's names, its release group when
  matched, and its cover; the marks as Discover's (0069).
- **Play:** what is covered plays Cratarr's own copy, as anywhere (a playlist plays the tracks it has); a missing album
  its Bandcamp preview (0071) where one is found. Nothing plays on Spotify, and Spotify's sound never plays through
  Cratarr.
- **Where:** a source in the Library beside the others: its shelves and flows (albums, tracks, playlists, artists)
  with the cover marks, in the view menu once the account has lists either way; covered and missing narrowed by the
  Filter sheet's own switches (everything shows, marked, out of the box). No page of its own.

## Endpoints

All are the signed-in account's own; `status` is `in`, `partly`, `missing` or `requested`. Times (`since`, `at`, `added_at`) are epoch seconds.

- `GET /api/spotify`: the state.
  `{ client_id: string | null, live: { user, name, since, sync: { state: "idle"|"running"|"failed", at, error } } |
  null, imported: { at, file } | null, shown: "live"|"imported"|null, counts: { albums, tracks, playlists, artists } }`
  (`counts`: of the lists shown).
- `PUT /api/spotify/client`: `{ client_id }`, the account's own app; empty removes it (and disconnects).
- `POST /api/spotify/connect`: `{ callback, app }` (`callback`: the page's `location.origin + "/api/spotify/callback"`;
  `app`: begun in the Android app) → `{ url }`, Spotify's consent page to open. `GET /api/spotify/callback?code&state`
  (open to anyone, found by its one-time `state`) keeps the sign-in, then sends the browser back to Settings › Account ›
  Spotify (`/#/settings?view=settings%3Aaccount%2Fspotify`; in the app `cratarr://connected/spotify`), with
  `&spotify_error=…` when it failed.
- `DELETE /api/spotify`: disconnects (the export's lists stay).
- `POST /api/spotify/sync`: reads the live lists again now; answers at once with the state.
- The export: `POST /api/import/file?name=&preview=` as any import file. What it brings (and would) is
  `{ kind: "spotify", history: Tally | null, library: { albums, tracks, playlists, artists, in, partly, missing } }`.
  `DELETE /api/spotify/import` forgets the export's lists (the listens stay, undone as before by
  `DELETE /api/import/file?from=spotify`).
- `GET /api/spotify/albums?status=&q=`: `{ albums: [ { id, title, artist, year, cover, added_at, status, have, of,
  release_group: string | null, album: { source, id } | null, request: Request | null, preview: { source: "bandcamp",
  id } | null } ] }` (`year`, `cover`, `added_at` null where an export does not say; `release_group` once MusicBrainz
  has found it: a missing album opens as any other missing album, and Want passes it on).
- `GET /api/spotify/tracks?status=&q=`: `{ tracks: [ { id, title, artist, album, release_group: string | null,
  length_ms, added_at, status, track: { source, id } | null, request: Request | null } ] }` (`album`: its name;
  `release_group` and `request`: its album's).
- `GET /api/spotify/playlists`: `{ playlists: [ { id, name, owner, collaborative, cover, of, have, status } ] }`;
  `GET /api/spotify/playlists/{id}`: the same with `tracks` as above, in the playlist's order. An export's playlists
  have no Spotify id: theirs is made from their name.
- `GET /api/spotify/artists?status=`: `{ artists: [ { id, name, photo, status, have, artist: { id } | null } ] }`
  (`have`: their albums in the library).
- `cover` and `photo` are Cratarr's own addresses (`/api/spotify/cover/{kind}/{id}`), relayed and kept as the other
  covers are; an export brings none, so its albums get the library's cover where covered, else Cover Art Archive's by
  release group once MusicBrainz has found one.

## Consequences

- A sign-in to another service, and an app's id, are kept per account for the first time: in the data folder, the
  account's by name, forgotten with the account or on disconnecting.
- The live way needs Premium and an app of one's own; the export works for anyone but is as old as its download, and
  is read again only when uploaded again.
- Playlists one only follows stay with Exportify's CSV.
- Cratarr never plays Spotify's audio, nor plays anything on Spotify.
