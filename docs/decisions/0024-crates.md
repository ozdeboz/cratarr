# 0024: Crates

- Status: accepted
- Date: 2026-10-14
- Builds on: 0011 (the catalogue), 0013 (Incoming), 0023 (the panel)

## Context

After Incoming (decision 0013) an album is in Music, but in no playlist: putting its tracks where they belong is the
second step, and nothing in Cratarr helps with it. The predecessor (M3U Library Manager, its Curation page) had
"Collect": the favourite tracks in no playlist, written to a frozen `Unplaylisted_<time>.m3u` and opened in a player to
sort by ear. It had no other selection, the snapshots piled up and went stale as one sorted.

## Decision

The owner decided (2026-10-14):

- **A page, Crates,** between Incoming and Playlists: Incoming · Crates · Playlists · Insights · Repairs (Settings
  apart).
- **A crate is a dynamic playlist:** a cross-section of the library along its axes (playlist membership, favourites,
  later BPM, genre, year, plays…), worked out when it is looked at. A track sorted into a playlist leaves "Unlisted" by
  itself. Only crates are dynamic; playlists never are.
- **Permanent crates,** always there, not renamed or deleted:
  - **Favourites:** the tracks the account Cratarr plays as (decision 0022's "Plays as") has as favourites.
  - **Unlisted:** the tracks in Music in no playlist.
  - **Unlisted favourites:** both (the predecessor's Collect).
  Only Music counts (not Incoming).
- **In a crate the newest come first** (when the catalogue first saw the track; a kept Incoming album keeps the time it
  arrived there), so what was just kept is on top.
- **Opened like a playlist:** the page shows the crates as chips with their live counts; a crate opens in the panel
  (decision 0023) as a playlist does: Play, Shuffle, Play next, Add to queue, a tap plays from a track, the hearts and the
  track menu; no edit mode (a crate changes by sorting its tracks into playlists). Long crates come in pages.
- **Make playlist:** a crate's tracks at that moment saved as a new, ordinary playlist (a snapshot, named when made),
  which then opens in the panel; the crate stays.
- **Crates of one's own** (owner, 2026-10-14, issue by issue):
  1. **Built in a third panel mode,** the crate builder, beside the player and the playlist/crate view: "New crate" (on
     the Crates page) opens it empty, a crate's edit button opens it on that crate; saving shows the crate.
  2. **Kept on the server, in one file** (`crates.json` in Cratarr's data folder, the previous version kept beside it
     for undo), so every device has the same crates.
  3. **Rules:** a crate is a list of conditions, all of which must hold, each one negatable. OR within an axis: a
     condition takes several values (genre is any of rock, jazz) or a range (year 1990–1999). Groups (OR across axes)
     later; the stored format carries a version for that. The permanent crates are definitions in the same model
     (Favourites: favourite is yes; Unlisted: NOT in any playlist; Unlisted favourites: both).
  4. **A generic axis registry:** each axis (on the server) declares its kind (yes/no, range, values, membership), its
     domain (the genres there are, the years' span) and how it is evaluated; the server lists them and the builder draws
     its controls from the list, so a new axis needs no builder work. First axes: favourite, playlist membership, genre,
     year, BPM.
  5. **Freshness:** library axes (the catalogue, the playlists) are always current; axes from the media server
     (favourites; later plays) are kept about a minute and dropped when Cratarr itself changes them.
  6. **Live preview:** while building, each change (after ~300 ms) shows the unsaved crate's count and first tracks.
  7. **BPM from the files' BPM tag;** an outside analysis job (not in Cratarr's 64 MB container) can later fill the gaps
     through the same axis.
  8. **Identity:** a crate has an id that never changes (the panel's address, links); its name is unique among crates
     and is not a permanent crate's name (it may equal a playlist's).
  9. **The draft** has its own address (`?open=crate-new`, `?open=crate-edit:<id>`) and is kept in the browser while
     open: it survives the player over it, a page change, turning the tablet and a reload; Save or Cancel ends it.

## Consequences

- The server works out a crate from the catalogue (Music's files, their first-seen time), the playlists' lines and the
  media server's favourites; a crate's rows are the playlist view's rows, so the panel shows them the same way.
- The phone's top bar has one page more: Repairs moves under More.
- `docs/ui-conventions.md` describes the page.

## Amendment (2026-10-14): the favourites toggle

The owner decided:

- **A crate's favourites are a toggle, not a crate.** In a crate's panel, Play next and Add to queue give way to one
  button that cycles **No favourites → Only favourites → All tracks** (back to No favourites); its icon and label say
  what the crate shows now: a crossed-out heart, a filled heart, an outline heart. A crate not set yet shows No
  favourites. It is a view, remembered per crate (on the server, amended below), not part of the crate: the server adds the condition
  when the crate is looked at, so the list, its count, Play, Shuffle and Make playlist all follow it.
- **The permanent crates Favourites and Unlisted favourites go;** Unlisted stays. The library's favourites as a whole
  are a crate of one's own without conditions, on Only favourites.
- A crate keeps Play and Shuffle; Play next and Add to queue stay with playlists.
- ~~The crate chips on the Crates page count the crate itself (All tracks).~~ Replaced below.

## Amendment (2026-10-14, later): the setting is the server's

The owner decided: crates live on the server, so their favourites setting does too. It is kept beside the crates in
`crates.json` (`favourites`, by crate id, the permanent crates' as well; a crate not in it shows No favourites), the
same on every device, and set with `PUT /api/crates/{id}/favourites`; a change to a crate's definition keeps it, and
it is not journaled (decision 0026: a tap undoes it by tapping again). The crates list gives each crate's setting and
its size with each of the three, so a chip on the Crates page shows the size and a small heart (in a favourite
playlist's heart's place, filled, outlined or crossed) for its setting, and a tap shows the next at once. The panel's
rows still name the setting they show (a tap may be on its way to the server); Make playlist waits for it and uses
the saved one. The settings a browser kept before are sent to the server once, then forgotten there.

## Amendment (2026-10-14, evening): favourites answered from what was heard last

Issue 5's "kept about a minute" made the Crates page wait: Jellyfin took a minute to list 4,786 favourites (about
13 ms each, however paged), so every visit after a quiet minute waited for it, and a browser gave up ("Cannot reach
the Cratarr server"). Now the server answers from the favourites it heard last, at once, and asks again in the
background once they are a minute old (one asking at a time; a server that does not answer keeps what was heard, and is
asked again a minute later). Cratarr's own heart changes are applied to what was heard at once, instead of having it
asked for again. Only the very first asking waits, and that one starts when Cratarr starts. A request waiting for it
holds nothing large meanwhile: those waiting with the library's facts in hand were the likely cause of a 64 MB restart.

What was heard is also kept in `favourites.json` in the data folder (2026-10-15), so after a restart the crates answer
at once from it while the server is asked again in the background; the file is only a head start, never the truth.

## Amendment (2026-10-19): a favourite albums toggle beside the favourites one

The owner: "We currently have a way to filter out favorited tracks, but I also want a way to filter out favorited
albums"; of one toggle covering both or a second one, "a second toggle."

- **A second toggle, for albums.** Beside the favourites toggle, in a crate's panel, a button cycles **No favourite
  albums → Only favourite albums → All albums**.
  - A favourite album is one with its heart on (the Library's). "No favourite albums" leaves out every track on
    one; "Only favourite albums" keeps only those.
  - Its icon is the album's disc: crossed out, filled, or outline.
  - A crate not set yet shows **All albums**, so nothing changes until it is tapped.
- **It works as the favourites toggle does.** It is a view remembered per crate on the server (`favourite_albums` in
  `crates.json`, set with `PUT /api/crates/{id}/favourite-albums`), not part of the crate, and not journaled. The
  server adds its condition when the crate is looked at, so the list, its size, the tempo spread, Play, Shuffle, Make
  playlist and Play as radio all follow both toggles.
- **The two combine.** A track shows when both allow it: "No favourites" with "Only favourite albums" is the tracks
  of favourite albums that are not favourites themselves.
- **The Crates page.** A crate's sizes are counted with its albums toggle as it is set, and its chip shows a small
  disc for it beside the heart, as on the toggle (the owner: "the crate card also has an icon representing the
  state").
- **The crate builder** has the matching condition, "Favourite album" (yes or no), as it has "Favourite".

## Amendment (2026-10-19): a crate's card shuffles it

The owner: "a play button on the crates, which automatically plays it with shuffle."

- **A crate's picture on the Crates page is its play button.** A tap shuffles the crate as a new queue under its
  name; the rest of the card views it, as before. The button follows the rule of a cover that plays (decided
  2026-10-16): its icon sits in the picture's lower right corner. Here it is Shuffle's icon, since that is what it
  does.
- **Both toggles apply.** The tracks are the ones the crate's two toggles let through, as it shows them.
- ~~**A big crate is sampled.** A crate comes 500 tracks at a time, so the queue takes up to three stretches of 500 from
  anywhere in it, shuffled together. A crate of up to 1,500 tracks plays whole.~~ Replaced below (2026-10-23).

## Amendment (2026-10-19): exclusions, a crate's and Cratarr's

The owner: "whenever a crate has added tracks to the queue, those tracks should get a button to remove them from the
crate"; then, of the open points, a list per crate, the track leaving the queue too, an Undo toast; later: not in the
Queue sheet ("It makes no sense to have the button there") but in the crate's panel and the full player; "I want
crate exclusions and I want global exclusions"; "exclude album from crate"; and the word: "exclude, not remove". Its
icon is the crate with a cross where its handle is (the owner chose it from three); the global one's is a ban.

- **A crate's exclusions** (`left_out` in `crates.json`), for every crate: one's own, Unlisted and a genre's. Tracks,
  and whole albums (the album's tracks now and later). They beat the conditions and both toggles (an excluded favourite
  stays out), and everything that reads a crate's tracks leaves them out: its rows and size, the tempo spread, Play and
  Shuffle (its card's too), Make playlist, radio within it. `POST`/`DELETE /api/crates/{id}/left-out {source, ids,
  albums}` exclude and put back, journaled (Undo); `GET` lists them (`rows`, `albums`); each crate carries `left_out`.
- **Global exclusions** (`/api/excluded`, the same three calls with `ids`): out of every crate, and never picked by
  Radio, Autoplay or a shuffle of the library or the favourites; a track still plays when chosen (its album, a
  playlist, a track tapped, a queue taken up).
- **Where**: in a crate's panel, a swipe left on a track (Exclude); and a long press on any heart opens the heart's
  sheet (the owner, 2026-10-19: every track
  row's, the full and the mini player's, the queue's playing card's): Favourite, the crate's two where they apply (a
  crate's rows; in the players while a queue the crate started plays its own track; never in the Queue sheet), and
  "Exclude globally". The crate's Excludes are in the sheet only, not in the track menus (one thing in one place:
  "Heart sheet only").
- **The queue**: what is excluded leaves what plays here too (a crate's: the queue it started, its own tracks; global:
  any but those added by hand); the playing one gives way to the next. Undo puts it back in the crate or the list, not
  in the queue.
- **Undo** follows in a toast ("Excluded from Test.", "Excluded globally."), the one notice of its kind.
- **Putting back**: "N excluded" under a crate's name in its panel (any crate, which the editor is not: Unlisted and a
  genre's have none) opens its list (lib/ExcludedSheet), and so does, for the global ones, "Excluded (N)" in the
  Library's filter sheet while there are any (lib/FilterSheet, under its switches; the list opens as a sheet of its own
  over it). Each put back, or Put all back, with Undo again. The global ones were first a line at the top of the
  Library page; the owner moved them (2026-10-19, for 70: "It takes up a lot of real estate"; "the filter sheet is a
  pretty good idea, actually"; "it would open its own exclusion sheet from the filter sheet"), not into Settings →
  Account, as a review had recommended.

## Amendment (2026-10-19): excluded from a crate, and everywhere (the server's part)

The owner, for release 69: "I want crate exclusions and I want global exclusions." Global means every automatic pick.
The word in the page is **Exclude**; the server's names stay `left_out` for a crate.

- **From a crate.** A crate's left-out list holds single tracks (their lines) and, as a rule, whole albums (their ids:
  the album's tracks now and later). `POST`/`DELETE /api/crates/{id}/left-out {source, ids?, albums?}` →
  `{left_out, undo}`; `GET` → `{source, rows, albums: [{id, title, artist}]}`, `rows` the single tracks. A crate's
  `left_out` counts both. Journaled for Undo.
- **Everywhere.** Each account has one list of excluded tracks (lines in its `crates.json`). `POST`/`DELETE
  /api/excluded {source, ids}` → `{excluded, undo}`; `GET /api/excluded` → `{source, excluded, rows}`. Journaled.
  - Left out of every crate (one's own, Unlisted, genres), as a crate's left-out list is: rows, counts, stats, the
    tempo spread, Make playlist, a crate's radio, Kept.
  - Never picked by Radio, Autoplay, Similar songs, the shuffles of the library and of the favourites, or Subsonic's
    random songs.
  - Still played when chosen by hand: an album, a playlist, a search, tracks named one by one, a queue taken up.

## Amendment (2026-10-23): what plays is the server's choice

The owner found that Unlisted, with more than 10,000 tracks, played the same few each time from its panel: the panel
played the rows it had loaded (its first page). Then: "we should limit to maximum 500 tracks in our queue."

- **One operation plays any collection**, a crate (both toggles), a genre, a playlist, an album, an artist or tracks
  named one by one: `POST /api/play` (server/src/app/play.rs), from every surface and for every player. The server
  picks the tracks; the page's own player queues what it answers, and on the server's player (or one of Cratarr's by
  name) the server queues them itself.
- **At most 500 tracks a queue** (one constant on the server). Play takes the collection's order: its first 500, or
  from a given track on (those before it fill up the 500 near the end). Shuffle draws 500 evenly from all of it, then
  mixes them; a crate of 500 or fewer plays whole. A Jellyfin player (Finamp) gets the 100 it holds.
- A crate's rows say the page size they kept (`limit`), so no caller takes 500 for the 1,000 it asked.

## Amendment (2026-10-23, later): one way to choose, for every play

Every play chooses its tracks through `app::play::choose` and the rules' `pick` (decision 0059), at most 500:

- Play On's `play` and `add` with a playlist (by name, as before), the favourites or the library (`/api/players/{name}/
  command`, Home Assistant's): before, a playlist and the favourites had no limit. The library is drawn at random, 100
  unless `limit` says; `limit` caps the others too. An album of Cratarr's own plays in its order, at most 500, from
  `index`; a shuffle is drawn from it.
- A playlist's Play (`POST /api/playlists/{id}/play {mode}`): 500 on Cratarr's own player (was 100); a Jellyfin player
  still gets the 100 it holds. `unknown` counts the lines without a playable track.
- The voice assistant's play by name (`GET /api/playlists/by-name?name&shuffle&limit`): its limit stays (50 unless
  asked, at most 100); a shuffle is drawn from the whole playlist.
- `POST /api/play` also plays `{"kind": "favourites"}` and `{"kind": "library"}`, and takes `limit`.
