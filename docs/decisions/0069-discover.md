# 0069: Discover: music outside the library, from what you listen to

- Status: accepted (the owner, 2026-10-24: "I wouldn't be interested in suggestions within my library, but
  suggestions outside of"; "we need a discover page next to playlists")
- Date: 2026-10-24
- Builds on: 0013 (Incoming: Keep and Delete), 0036 (radio), 0064 (the listening history), 0067 (one import, its
  matcher), 0068 (the listening services)

## Context

Everything Cratarr suggests so far comes from the library: the radio, Home, More music, the crates. What is not there
yet, Cratarr cannot point to. The listening services can: ListenBrainz works out recommendations for each listener from
everyone's listens (collaborative filtering, by MusicBrainz ids), and makes weekly playlists for them (Weekly
Exploration: music new to them); Last.fm says which artists are like an artist, from everyone's scrobbles. Lidarr,
where it is connected, fetches what is asked for into Incoming, where it is heard and kept or deleted (0013).

## Decision

**A page of its own, Discover, beside Playlists**: albums the library does not have, each with why it is suggested.

- **Whose:** the signed-in account's, from its own services (0068): ListenBrainz by its user (the one it scrobbles as,
  else the one it imported from), Last.fm likewise.
- **From:**
  - ListenBrainz's recommendations for the user (recordings, scored) and its playlists made for them (Weekly
    Exploration first), named through ListenBrainz's metadata: each recording's album (release group) and artist.
  - Last.fm: artists like the user's most played (`user.getTopArtists`, then `artist.getSimilar`), each new artist's
    best known album (`artist.getTopAlbums`).
- **Only outside the library:** an album the library has (by MusicBrainz release group where both know it, else by
  name and artist, through 0067's matcher) is left out; so is one already in Incoming. An artist the library has may
  still come with an album it lacks; an artist it has not gets "New to you".
- **Why, on each:** "Recommended by ListenBrainz", "In your Weekly Exploration", "Like Bonobo and Air, which you play"
  (Last.fm). Where more than one source names it, all of them, and it ranks higher.
- **Each album:** its cover (Cover Art Archive, by release group), artist, year; links to listen elsewhere
  (MusicBrainz, ListenBrainz, Last.fm); **Want**, with Lidarr connected: Lidarr adds the artist where it has not (its
  other albums unmonitored), monitors that album and searches, so it arrives in Incoming; **Not interested**, kept,
  so it never comes back (and an artist dismissed twice is left out).
- **Fresh:** looked up again once a day for each account that opens Discover, as a task (`app::tasks`, the progress
  convention); kept in between, so the page opens at once. Refresh on the page asks again now.
- **No services connected:** the page says what Discover needs and opens Connections.

## Consequences

- Cratarr asks ListenBrainz, Last.fm, MusicBrainz's cover archive and, with Want, Lidarr; nothing is sent but the
  user names and ids they already have. Their limits are kept (Last.fm's five calls a second, ListenBrainz's
  rate-limit headers).
- ListenBrainz's recommendations are marked experimental in its documentation; when it answers 204 (none made for the
  user yet), the page says so, and the other sources still fill it.
- Not yet: a preview to hear in Cratarr (no source plays what the library does not have), suggestions by track rather
  than album, steering by mood or genre.
