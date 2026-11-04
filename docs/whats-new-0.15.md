# What's new in 0.15

Since 0.14.0. 0.15 is what preview.1 held, with the fixes found on it (2026-11-04). Each user's own music, one track
id and the library on the device are 0.16 ([0101](decisions/0101-own-music-one-track-id-library-on-the-device.md),
[0100](decisions/0100-cratarrs-own-records-artists-labels.md), [0102](decisions/0102-the-tape-engine-on-the-device.md)).
The notes for each part are in [decisions](decisions).

## Other servers in the app

- **Modes per server** ([0098](decisions/0098-modes-per-server.md)): a Jellyfin or Subsonic server in the app, a mode
  beside Cratarr's and Local, added in This device › Servers. Its Home, records, playlists (edited on that server),
  search and hearts, its own Radio; downloads kept per server. Home is per server, Appearance shared.

## Scrobbling

- **From each device or through the server** ([0091](decisions/0091-scrobbling-from-the-device.md)), each account's
  choice in Tracking: the player that sounds a track tells ListenBrainz and Last.fm itself, with this device's own
  sign-ins; through the server, as before, other servers' plays handed over to it.

## Words

- **Words' controls** ([0097](decisions/0097-words.md)): in Catalogue, "Written to files" (which kinds go into the
  files) and Sources and spellings (each kind's sources, one word spelled as one). A record's Edit sets its styles,
  moods and tags by hand, a track's Edit its own; a word's menu says where it came from, renames it everywhere, merges
  it into another, or takes it off a record.
- **Type and year are written to the files by default.** Health lists the records whose files say otherwise, for review;
  nothing is written by itself.

## Fetching

- **Soulseek in Cratarr** ([0099](decisions/0099-soulseek-in-cratarr.md)): whole records judged by their release,
  fetched on a tick, each file checked. Lidarr and Soulseek are siblings: Fetchers in Connections, and "Fetched by" on a
  library. Soulseek takes only approved Requests, and in 0.15 only records MusicBrainz knows (Discogs- and Bandcamp-only
  records come with 0100 in 0.16). It waits until a library is fetched by it.
- **Missing tracks, again and again:** each says how often and when it is looked for next (a ladder of retries, never
  given up, Search now overriding); kept across a restart; the next candidate when a user fails.

## Sound and rhythm

- **Rhythm:** "four on the floor" and a time signature ("4/4") as terms in the crate sentence and as conditions. The time
  signature is filled at the first start; four on the floor once the library is measured again.
- **Sound on the playlist map:** a fourth closeness switch beside shared tracks, genre and artists, from each pair's
  energy, danceability, tempo and moods.

## The tape and playing

- **The tape measured truthfully,** in the app and the page: no guessing where the sound is; a brake from playing
  backwards slows down, never first speeding up.
- **A player back after more than five minutes** gets its queue back paused, where it was, not playing; within five
  minutes it plays on.
- **The app's heard place** only while it sounds (a turn from backwards play no longer reports it gone).

## Moving about

- **The review a sheet** over where it was opened from, not a panel of its own.
- **What the page shows is read again** when the server says it changed: Health, the Library, the panel's rows.
- **Go to album from the player** keeps the album open; the player closes within a second of a Go to from it.
- **A panel's left glass** takes taps again beside the page.
- **Health:** a broken seek table's fix offered; a file's row opens to its problem and its fix.
- **Similar albums** asked once a record.
