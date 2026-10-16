# 0036: Radio: the music goes on, from the owner's own library

- Status: proposed
- Date: 2026-10-16
- Builds on: 0030 (OpenSubsonic), 0031 (Play On), 0032 (Cratarr is its own source), 0033 (Cratarr holds the queues;
  lanes, repeat)

## Context

The owner wants music that keeps going when the queue ends, chosen from their own library: Finamp's radio (0.9.21 beta)
and Symfonium's Smart queue. The research, written up 2026-10-16:

- **Finamp's radio:**
  - Off by default; turned on in the queue panel or by a long press on repeat. Repeat is unavailable while it is on.
  - The queue always plays first, and radio tracks come after it, marked. Turning radio off removes the upcoming ones.
  - Modes: Similar (to the queue's source, closest first, drifting), Continuous (each track near the one before),
    Album mix, Reshuffle and Random.
  - "Start radio" leaves out the source's own tracks; an Instant Mix includes them.
  - Jellyfin's similarity is genre tags only. Finamp recommends AudioMuse (sound analysis) for real similarity.
- **Symfonium:**
  - Smart queue appends at the end, by genre, artist, mood, style, sonic analysis or random.
  - Smart flow inserts tracks between songs: Shuffle specialist, Double shot, Artist fan, Era enthusiast, and three
    sonic modes.
  - Its Radio mix is a one-off batch.

Cratarr has no sound analysis yet. It knows artists, album artists, genres, years, album tags, the owner's favourites,
when each track last played, and which playlists hold it. Playlists are the owner's own curation, which neither app
uses as a signal.

The owner settled the questions on 2026-10-16 (in the Home Assistant session):

1. Radio and repeat-all exclude each other. Repeat-one stays possible.
2. The switch is a radio button in the player's action row, where the device button was. It is lit while radio is on
   and opens a sheet with on/off and the mode.
3. The default mode is Similar.
4. Similar stays close to the queue's source and drifts slowly.
5. Tracks played in the last 7 days are skipped unless the candidates run out.
6. Similar's signals carry equal weight: shared playlists, artist, genre, era and album tags.
7. The first version's modes: Similar, Reshuffle, Random, Continuous, Album mix, Era and Artist.
8. "Start radio" from an album or playlist leaves out its own tracks.

## Decision

**Radio is a setting of the queue**, as repeat is (0033): `radio` on the queue, absent while off, holding the mode and
the seed.

- **Command:** `{command: "radio", on, mode?, seed?}`. Like repeat, it is a setting and does not raise the queue's
  version.
  - Turning radio off removes the upcoming radio tracks. That is an edit: it raises the version, and the player is told
    the new upcoming tracks.
- **Repeat-all:** radio on turns repeat-all off, and repeat-all on turns radio off (with its tracks removed).
  Repeat-one and radio live together.
- **Reports and handover:** players report `radio: {mode, seed}` (null while off), and Cratarr takes it from them, as
  it takes repeat. The player view carries it, so Home Assistant sees it. A handover takes it to the new player.
- **A new play** keeps radio on, so the music goes on after the new album too. The new album or playlist becomes the
  seed, and the old radio tracks go with the rest of the old queue.
- **"Start radio"** (the track and playlist menus, where the page holds the queue): the first radio tracks like the
  item are played, and then `radio {on, seed}` is sent. Those are two requests, which may arrive in either order. So a
  play of radio tracks only keeps the seed it has.

**Radio tracks are a lane of their own: `lane: "radio"`, without `n`.** They are never the album's or playlist's own
tracks.

- They come after the album or playlist and after what was added by hand.
- Shuffle leaves them out and keeps them last.
- Repeat-all's next round skips them.
- They are marked in the queue sheet.

**The player that plays fills the lane**, as it builds repeat-all's next round. When fewer than 3 tracks are left ahead
of the playing one, it asks Cratarr for more:

- **Online:** `POST /api/radio/next` with `{mode, seed, exclude, last, count}`.
  - `seed`: the radio's seed, or else the queue's own album or playlist tracks (ids).
  - `exclude`: every track id in the queue.
  - `last`: the playing track's id.

  The answer is `{tracks}`: queue tracks already in the radio lane. About 5 at a time; Album mix gives one whole album.
- **Offline (the app):** it scores its downloads by the same rules. For that it needs, per track: artist, album artist,
  genres, year, album tags, playlist membership and the last play.
- **A page that plays itself** (the browser's own stream) does the same from `localPlayer`.

**How tracks are chosen.** Cratarr's own library only (`tr-…`). Excluded: `exclude`, the seed's own tracks when the
radio was started from an item, and tracks played in the last 7 days. Those 7 days apply until fewer than `count`
candidates are left; then the excluded ones come back, least recently played first.

- **Similar** scores each candidate against the seed on five signals, each from 0 to 1, with equal weight:
  - **playlists:** the most seed tracks any one of its playlists holds, as a share of the seed;
  - **artist:** how many seed tracks share one of its artists or its album artist, as a share (full at a quarter of the
    seed);
  - **genre:** the same, for its genres;
  - **era:** 1 at the seed's middle year, falling to 0 ten years away;
  - **tags:** the same as artist, for its album's tags.

  Favourites get a small boost (+0.05) and every candidate a small random nudge (up to 0.1), so the radio does not
  play the same order every time. It picks among the 30 best, weighted by score, with spacing: no two tracks of one
  album in a batch, and never the same artist twice in a row. As the closest ones are played, they join `exclude`, and
  the radio drifts outward by itself.
- **Continuous:** Similar with the playing track as its only seed track.
- **Album mix:** albums scored by their tracks' mean Similar score, leaving out the seed's albums and albums already
  queued. The best goes in whole, in its order.
- **Era:** random tracks within 5 years of the playing track (of the seed's middle year when it has none).
- **Artist:** random tracks by the playing track's artists and the seed's most common artists.
- **Reshuffle:** the seed's tracks again in a fresh order, never starting with the track that just played. The 7 days
  do not apply.
- **Random:** random tracks from the seed. When those run out, from the whole library.

**Crates as the radio's input** (the owner, 2026-10-16: yes to both; no radio page). A crate's action bar has two more
actions where the page holds the queue:

- **Start radio:** the crate as the seed. Tracks like it, from the whole library, none of its own. Up to 300 of its
  tracks make the seed.
- **Play as radio:** the crate as the boundary (`radio.within` / the request's `within`: the crate's id). Only the
  crate's tracks, as its favourites toggle is set, read from the crate on each refill. They are chosen the radio's
  way: Similar follows the playing track, the 7 days and the spacing apply, the crate's own tracks may play. Once all
  have been queued, a new round begins, those queued longest ago first and not the last few again.
- **A new play** leaves the crate, as it clears the seed.

**OpenSubsonic:** `getSimilarSongs`, `getSimilarSongs2` and the similar artists of `getArtistInfo2` answer from the same
scorer (Similar, the item as the seed). Today they answer empty, so any Subsonic app gets radio from this too.

**Later (phase 2):** AudioMuse fingerprints imported into Cratarr. Cratarr searches them itself, without AudioMuse
running, and gives sound-based modes (Sounds like, Mood, Steady), offline on the phone too. Their tempo and loudness
meet 0035's analysis. The proposal is in the owner's hand-off notes of 2026-10-16.

## Consequences

- Radio needs nothing new in the files and runs on the catalogue as it is.
- Until 0035 or AudioMuse, similarity is only as good as the tags and playlists. The playlists signal makes up for thin
  genre tags.
- The scorer keeps an index of the library in memory (about 30,000 tracks), rebuilt when the library is rebuilt.
