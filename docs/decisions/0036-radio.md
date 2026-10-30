# 0036: Radio: the music goes on, from the owner's own library

- Status: accepted (built 2026-10-16, ea8ab39d and on, as amended: Radio's inserts and Autoplay apart; marked at the
  owner's word, 2026-10-19)
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

The owner settled the questions on 2026-10-16:

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

## Amendment (2026-10-16): Radio and Autoplay, apart (as Symfonium's)

Comparing Symfonium's queue, the owner chose two ways for the music to go on, apart from each other. What this
decision built so far is the second kind, renamed **Autoplay** (the owner's choice of name over "Smart queue": the
word Apple Music and Spotify use for exactly this).

**Radio inserts.** It is an on/off switch with no modes.
- While it is on, each of the queue's own tracks (no lane) that starts playing gets about 5 tracks like it right
  after it, before the queue's next own track. They go in the `radio` lane, marked as now. A radio track gets none of
  its own, so there are no chains.
- "Like it": Similar with that track as the only seed, its artist weighed double. Symfonium's inserts are mostly the
  same artist.
- Turning it off removes the radio tracks not yet played.
- Command: `{command: "radio", on}`.
- Its button: the player's radio button and one in the queue's head. A tap switches it, and it is lit while on. The
  radio sheet goes.

**Autoplay appends** when the queue runs out. This is the lane described above, under its own name: `lane:
"autoplay"`, marked with its icon.
- Command: `{command: "autoplay", mode, albums?, restart?}`.
  - `mode`: `stop` (off), `similar`, `artist`, `era` or `random`.
  - `albums`: whole albums instead of tracks while albums were playing. This was Album mix.
  - `restart`: a playlist that ends starts again, shuffled if it played shuffled, instead of the mode. This was
    Reshuffle.
- Continuous goes: Radio does that now.
- Its button: ∞ in the queue's head, lit while on, opening its sheet. The sheet lists Stop, Similar, Same artist,
  Same era and Random as a choice, then the two switches Queue albums and Restart playlists.
- "Start radio" (the track and playlist menus) and a crate's "Start radio" and "Play as radio" keep their names and
  what they do. They set Autoplay with the item as its seed or bound, since they go on past the queue's end.
- Repeat-all and Autoplay exclude each other, as before. Radio lives with both. Repeat-all's next round still skips
  radio tracks, and they come again as each own track plays.
- Reports: `radio: bool` and `autoplay: {mode, albums, restart, seed, within}` (null while off).

**Home Assistant.** Its music starts set Autoplay (`similar`), so the music goes on at the end as now, and is not
interleaved with radio tracks. Voice follows the split: "radio on" and "radio off" go to Radio, the mode phrases to
Autoplay, and "album mix" sets `albums`. During the change-over Cratarr accepts the old command:
- a `radio` command with `mode`, `seed` or `within` is the old shape and maps to Autoplay (Album mix maps to
  `similar` with `albums`, Reshuffle to `restart`);
- one with only `on` is the new Radio;
- a bare `radio {on: false}` switches Radio off only. (A first draft had it stop Autoplay too, so that "radio off" by
  voice would stop what Home Assistant started; but then the player's own radio switch would stop Autoplay as well.
  Home Assistant moves to `autoplay` itself instead, so the rule is not needed.)

**Work**, in four parts: the lane rule (queue.rs), the page's fill (localPlayer), the app's fill, and the buttons with
Autoplay's sheet.

### Radio's choice and count (2026-10-16, the owner)

Radio gets choices after all. One sheet holds both: Radio on top, Autoplay below. It opens from a floating pill in
the queue and from the player's radio button.
- **Like what:** Like the track (as above: Similar seeded by the track, its artist weighed double), Same artist
  (Autoplay's Artist seeded by the track) or Same era (Autoplay's Era, within five years of it). Off is the switch.
- **How many** after each own track: 3, 5 (the default) or 10. The server keeps 1 to 10.
- Command: `{command: "radio", on, by?, count?}`, `by` being `track`, `artist` or `era`. What is left out stays as it
  was. Not `mode`: during the change-over a `mode` marks the old command. Cratarr tells the player the command in
  full.
- Another choice or count while on takes the inserts still to come out, and the playing own track gets them anew.
  Set while off, it is kept for the next on.
- The insert request: `POST /api/radio/next {mode: "radio", by, seed: [track], count, …}`. The tracks still come in
  the radio lane, whatever `by` is.
- The queue keeps them as `radio_by` and `radio_count` (left out while the defaults). The player view carries
  `radio`, `radio_by` and `radio_count`; reports may carry `radio_by` and `radio_count` beside `radio: bool`.

### Ignore favourites (2026-10-16, the owner)

A switch in the More music sheet, "Ignore favourites", for discovery: Radio's inserts and Autoplay's picks both leave
out every track the owner has hearted. One switch covers both, like the sheet's other switches.
- Command: `{command: "skip_favourites", on}`. A change takes Radio's and Autoplay's picks still to come out; the
  player picks anew (Radio for the playing track, Autoplay when the queue runs low).
- The request: `skip_favourites: true` on `POST /api/radio/next`. The scorer drops favourites from its pool before it
  chooses. A heart set in the last ten minutes may still slip through, because the index is cached that long.
- The queue keeps `skip_favourites` (left out while off); the player view carries it; reports may carry it
  (`skip_favourites` in Play On reports, `skipFavourites` over the app's bridge); a handover passes it on.

## Amendment (2026-10-24): no Restart playlists

The owner: "We can just remove the restart playlist function altogether. I don't like it." More music has no Restart
playlists switch, and neither the page's player nor the app's starts a playlist again at its end. The `restart` field
stays in the `autoplay` command and reports for older players and servers; set or not, it does nothing.

## Amendment (2026-10-25): a library choice of its own, and Bandcamp (0.9.5)

The owner: Radio and Autoplay get a library choice of their own, apart from the pages' library filter, with Bandcamp
as an extra source. This amends "Cratarr's own library only" above.

- **Where it is kept.** It is the account's own setting, kept on the server in the account's settings (`GET`/`PUT
  /api/me/settings`) beside `similar_by`: `more_music: { libraries: string[] | null, bandcamp: boolean }`. The
  libraries are Cratarr's own library ids, its folders. `null` or absent means all of them; `bandcamp` absent means
  off. Every player of the account reads it: the page's own player through `POST /api/radio/next`, and Cratarr's
  players (Play On) through their account.
- **Libraries.** Radio and Autoplay choose only songs in the libraries switched on. A seed from a library that is
  switched off still seeds, but it is never picked. A crate that plays keeps its own libraries. With none of the chosen
  libraries there (all switched off), `radio/next` says so: "No library is switched on for Radio and Autoplay."
- **Bandcamp.** With `bandcamp` on, the account's Discover suggestions that play on Bandcamp join the candidates "as
  they come", with no fixed share. They compete on likeness alone, by the same weights as the library's songs: their
  artist, the library artists Discover found them from (its `alike` seeds and "Like …" reasons), their styles as
  genres, and their year. There is no sound and no playlists for them. Each one, the most alike first, takes the place
  of the least alike pick it is more alike than. This applies to Similar, Continuous and Radio, not inside a crate.
  Such a pick is one streaming track of its album, chosen at random, as a Bandcamp preview: source `bandcamp`, id
  `bc.…`. Following decision 0071 it is never a play and never scrobbled, and Want stays in its menu.
- **Picks already queued.** After a change, the page drops Radio's and Autoplay's picks still to come and asks again.
  Cratarr's own players let the picks they already queued run out.
