# 0033: Cratarr holds the queues

- Status: accepted
- Date: 2026-10-15

## Context

Since 0031 each player holds its own queue: the app's player (or a page) has the tracks, Cratarr keeps a copy of what
the player reports, and commands name places in the queue (`move {from, to}`, `remove {index}`, `jump {index}`). That
has shown its limits within a day:

- A place is right only for the queue it was read from. Moving a track on the phone while the tablet changed its own
  queue broke the tablet's player; Home Assistant's "remove 3" hits whatever is third when it arrives.
- The same track twice in a queue cannot be told apart.
- The queue lives on the device: updating the tablet's app lost it (it was saved and played back by hand).
- Nothing plays when the phone has no connection to Cratarr, and nothing could be put back together afterwards.
- Other Subsonic apps cannot see what plays where.

The design below was worked out with the owner on 2026-10-15, from how others solve it (Jellyfin SyncPlay, Lyrion,
MPD, Spotify Connect, OpenSubsonic's saved queue, Pocket Casts' Up Next, AntennaPod, Audiobookshelf, Finamp; leases
with fencing tokens). Finamp has nothing to copy here: it never uploads what it played offline.

## Decision

**One owner per queue.** Each player's queue is held by Cratarr. The app holds its queue itself only while it cannot
reach Cratarr (no switch to throw: it happens when the connection is gone) and gives it back when it reconnects. A page
in a browser never holds a queue: it plays one Cratarr holds, and is a Play On target only while it is open.

**Entries, not places.**

- Each entry in a queue has an id of its own (a UUID, which the app can make offline too), never reused and apart from
  the track's id, so a track can be in a queue twice.
- Edits name entries: insert after entry X, remove X, move X after Y, jump to X. A place sent by Home Assistant
  (`remove {index}`) is turned into the entry there when the command arrives; the player only ever sees entries.
- Each queue has a version, one up on every change. The push tells "queue Q is now at version N" (as `queue_version`
  does today); the queue itself is read when wanted.
- A queue is saved after every change and kept when its device goes away, so an app update or a crash loses nothing.

**Two lanes.** The context (the album or playlist that was played) and the entries added by hand ("play next", "add to
end"). Play next puts the newest batch first, right after what plays; add to end keeps the order given. A batch stays
together. Playing something new replaces the context, not what was added by hand.

**Shuffle** is a second order over the same entries (or its seed), so turning it off brings the first order back, and
another device can rebuild it.

**Handing over** (0031) is one action at Cratarr: the queue, the entry that plays, its position and when that position
was read. The new player works the position out from that; the old one is told to stop, not to pause.

**Plays are not queue edits.** Each play is an event with an id of its own (device, track, start) and its own time,
counted once however often it is sent. The position belongs to the player that makes the sound; a late, older report
never moves it back, and small differences are ignored.

**Back online.** The app keeps a log of its offline edits (`op_id`, its own sequence number, the version it started
from, the edit). Cratarr remembers per device the last sequence number it applied, in the same write as the edits, so
an edit sent twice counts once.

1. If Cratarr's queue is still at the version the app started from, the app's queue is taken as it is.
2. Otherwise Cratarr makes the app's edits again, in order, on its own queue: removing an entry that is gone does
   nothing, moving one is dropped, inserting after one goes after the nearest earlier entry still there (else at the
   end). Removed and played entries are kept as gone until the device has seen that, so nothing comes back. Each edit
   gets its own outcome; one failing never stops the rest. The app takes the result and makes again whatever it did
   meanwhile.
3. If the whole queue was replaced in the meantime, nothing is made again: the queue that is playing wins and the other
   is kept as a snapshot that can be played back, and that expires by itself. No question to the owner.
4. After more than 24 hours offline the app's queue is not made again either: it is kept as such a snapshot. Its plays
   are still counted.
5. The entry that plays is never disturbed.

Everything is ordered by Cratarr's own counter, never by a device's clock.

**Leases.** The 75 s lease of 0031 stays for being online. Holding a queue (the app, offline) is a lease with a number
(an epoch) that goes up each time the queue changes hands: Cratarr takes it back only when someone else wants to edit
it after the lease ran out, or on an explicit "take over" elsewhere. An app coming back with an old number goes through
the steps above, never straight over Cratarr's queue. The app times its lease on its own clock since boot and lets it
end a little early.

**Other Subsonic apps** see the queues (saved queue, now playing) but do not change them.

## Steps

1. The queues at Cratarr: entries with ids, versions, saved per player; commands turned into entry edits; the app's
   player plays Cratarr's queue and reports the entry that plays (`current_entry_id`) and its position, no longer its
   whole queue.
2. The page's own player (in a browser) plays a queue Cratarr holds.
3. The two lanes; shuffle as a second order.
4. Offline: downloads in the app (as Finamp's) and a cache of what comes next, the edit log, the steps back online,
   the lease numbers, the 24-hour rule, plays as events.
5. Other Subsonic apps see the queues.

## Consequences

- Home Assistant's commands stay as they are; places are still accepted. A player's view gains `current_entry_id`,
  and its queue's entries their ids.
- The app is no longer the judge of its queue while online: an edit made on it goes to Cratarr first, so it needs the
  connection (or works offline under step 4).
- A device gone for long, or a queue replaced while it was away, can no longer bring an old queue back.
- Removing a track from the library while a device is offline is not covered here: it belongs with the library's own
  sync, on the same reconnect.

## Amendment (2026-10-15): step 4 as built

- **Downloads** are in the app: the owner's chosen tracks (as the files are, or as Cratarr's smaller copies: 160 kb/s
  Ogg Opus made from lossless files, `?as=opus`; lossy files come as they are), and the next five tracks of the queue,
  kept as they would be streamed anyway. The player reads every track from the device first. Over Wi-Fi only unless
  mobile data is allowed. The page's Downloads view (in the app) plays them without a server.
- **Away from home** (added 2026-10-16, the owner: "work when away"): Cratarr is out of reach when the proxy refuses
  (its 403 off the home network; Cratarr never answers 403), a stopped Cratarr gets the proxy's empty 502 to 504, or
  there is no network. The app then opens on Downloads, the pages say "Cannot reach the Cratarr server", the page asks
  every 30 s instead of every 2, and the service worker keeps the downloaded albums' covers.
- **Plays are events**, as decided: the app counts them itself (90% through, with their own time), keeps them until
  Cratarr has them, and Cratarr counts each once (`POST /api/plays`).
- **No edits to make again on reconnect, yet.** Cratarr refuses commands for a player that is not connected, so while
  the app holds its queue (offline) nothing else changes it: on reconnect its queue is simply taken (step 1). The edit
  log, the steps back online and the 24-hour rule are needed once Cratarr accepts edits for an absent player (Home
  Assistant queueing for the phone while it is away, say); they wait for that.

## Amendment (2026-10-16): repeat

Decided by the owner. **Repeat** is one of three modes: **off** (the queue ends and stops, as now), **one** (the playing
track starts again when it ends), **all** (after the last track the album or playlist starts again from its first).

It is a setting of the queue, held by its owner as shuffle is: Cratarr's, or the app's while it holds its queue offline.
It moves with a handover. The player that plays applies it when a track ends: only it knows that moment, and it has to
work offline and without gaps.

1. Tracks added by hand play once and are used up; only the album or playlist starts again.
2. Shuffle and repeat all: each round is shuffled anew, and a round does not start with the track that just played.
3. Next or previous under repeat one go to another track; repeat one stays on, for that track.
4. Something new played turns repeat one off; repeat all stays on.
5. Previous on the first track under repeat all goes to the last.

Every round counts its plays again. A player's view tells its repeat mode; `repeat {mode: off|one|all}` sets it.

## Amendment (2026-10-16, accepted): queue history

The owner asked for Symfonium's queue switcher: every play keeps its own named queue, and an earlier queue can be
switched back to where it left off, renamed or deleted, with the playing one marked.

**What a queue is.** A play of something new starts a new queue: an album, a playlist, a crate, a genre, a radio or
"Start radio" (`Command::Play`). Edits (add, move, remove, shuffle, repeat, radio on/off) change the queue that plays
and start nothing. The queue left behind is kept whole:

- its tracks with their entries;
- where it stood: the playing entry and the position;
- its repeat, shuffle and radio.

**Held where the queues are held.** Cratarr keeps each player's queues beside the one that plays (`queues.json`), the
newest first.

- **Name:** the play's own ("Chill", an album's title, "Radio: Trip Hop"), until renamed.
- **How many:** at most 20 per player. The oldest unrenamed one goes first; a renamed one stays until deleted.
- **A handover:** the queue moves on to the other player as its new queue. The player it left keeps it, as it stood.

**Commands** (Play On, and the page's own calls when it drives a player):

- `{command: "queue", id}`: the player switches to that queue and plays it from where it stood, playing if it played,
  else paused there. The queue it left goes back into the list as it stood.
- `{command: "queue_rename", id, name}`.
- `{command: "queue_delete", id}`. Not the playing one: switch away first.

**What the page sees.** A player's full view (`GET /api/players/{name}`) carries
`queues: [{id, name, tracks, from, updated, playing}]`, newest first, with the playing one marked. The live channel
says when the list changes, the way it already says when the queue does.

**Offline.** The app holds one queue while it has no connection, as now. Switching needs Cratarr, and a queue played
offline goes back to Cratarr as the playing one when the app reconnects (step 1).

**Settled with the owner (2026-10-16):**

1. A browser playing itself keeps a history of its own, in that browser only (the same list and switch; another
   browser or device does not see it).
2. A switch takes the queue up where it was left: its track and position, playing if music played, else paused there.
3. 20 queues per player.

**How a player tells a new queue.** Each queue has an id. A player that starts something new (the page in the app
playing an album, say) gives it a new id and its name, and reports them (`queue_id`, `queue_name`). Cratarr then keeps
the queue that played before in the list, as it stood when last reported. A play Cratarr sends (Home Assistant, a page
driving a player, a switch) carries the id and name it chose, so the player reports them back unchanged. An app from
before this reports neither, and its queue simply plays on as one.

## Amendment (2026-10-16, the owner's go): step 5, what Subsonic apps see

"Seen, not changed" alone would have broken the apps. Their source was read on 2026-10-16:
- Apps that use the saved queue also write it, often. Supersonic and DSub save on every change, airsonic-refix every
  10 s, Youamp every 5 s, Psysonic by default.
- Many take the server's queue in place of their own: when they start (Supersonic, refix, Youamp, Cassette), or every
  10 s while paused (Psysonic).
- DSub asks to continue whenever the server's queue is newer than its own.

So a player's live queue given back, with the apps' saves dropped, would replace their queue with a Kodi's again and
again. A refused save shows errors (Feishin, refix). The servers differ too: Navidrome, gonic and nghe keep one queue
per account, the last save winning; Ampache keeps one per account and app.

- **Each app's save is its own:** one per account and app (Subsonic's `c`; every install of an app shares it), the
  song that plays kept by its place. A save without songs clears it. It never touches a player's queue.
- **The queue an app is given** (`getPlayQueue`) is the one started or saved last: an app's save, or the queue something
  new was last started in on one of Cratarr's players (a play, a handover, a queue of its own the player reports). The
  player's name is `changedBy`, the queue as it plays now and where it is. A player playing on, Radio's and Autoplay's
  inserts and edits leave the queue's start time (`started`) alone. So an app that takes the server's queue takes a
  player's once, after something new started there.
- **By index** (OpenSubsonic `indexBasedQueue`): a song in the queue twice is told apart. Feishin shows its server-queue
  buttons only with it.
- A queue given is cut to 1000 songs around the one that plays (100 before). Songs the library no longer serves are
  left out, and the place of the one that plays moves along with them (Navidrome's bug #6297 was a stale place).
- **What plays where** (`getNowPlaying`): each of Cratarr's players that plays or is paused, under its name, with a
  number that stays the same, its state and position. Each app that reports is listed too: `reportPlayback`
  (OpenSubsonic `playbackReport`) or `scrobble` with `submission=false`. These sessions are Navidrome's:
  - a report that stops past half the song or 4 minutes counts a play, unless `ignoreScrobble`;
  - a session ends 5 s after its song while playing, and 30 minutes after a pause.
- No `jukeboxControl`: it hands an app the whole queue of one device, every server ties it to a single output, and
  Symfonium refuses it. Driving Cratarr's players stays Play On's.

## Amendment (2026-10-17): a handover from a player that is offline

A handover takes the queue Cratarr holds, so it works from a player that is offline too (Home Assistant moving the
phone's music to the tablet after the phone dropped off). The stop it sends the player it leaves cannot reach it then.
On 2026-10-17 the phone came back 8½ minutes later without a queue, and was given back the one that had moved, playing.

- The player left is stopped in Cratarr as the stop would have left it: its queue empty, nothing playing. Back without
  a queue, it is given nothing.
- Back still playing an entry of the queue that moved, it is told to stop. Music started on it since plays on.

## Amendment (2026-10-19): queue history as built

Built on the owner's "build it as decided" (2026-10-19), after the salon tablet lost a queue of 142 tracks that day: its
app, updated, reported nothing queued before it connected, and Cratarr kept only the queue that plays.

- **When a queue is kept:** a play of something new, a stop, a handover (the player left keeps it), and a queue a
  player reports that is another one: named by the player with an id of its own (`queue_id`, `queue_name`: the page in
  the app played an album there), or something new it started, or none at all. Kept whole, as it stood: its tracks with
  their entries, the entry that played, the position, whether it played, repeat, Radio and Autoplay.
- **Names:** as the one who plays gives it (the page sends the album's title or the playlist with each play, `name`),
  else the playlist's, "Favourites", "Library", else the album, artist or first track the tracks share. `from` says
  what it was started from: `playlist`, `album`, `favourites`, `library`, `tracks`, or `player` (started there).
- **The list** is saved with the queues (`queues.json`); queues from before get an id and a name as they load. A full
  view carries `queues` (the playing one first and marked, then newest first); every view carries `queues_version`,
  which goes up as the list changes. 20 in all with the playing one; the oldest not renamed goes first.
- **Commands** `queue {id}`, `queue_rename {id, name}` (the playing one too; at most 100 characters), `queue_delete
  {id}` (not the playing one), as decided. A switch takes the queue up whole: nothing of the queue it leaves comes
  along (unlike a new play, which keeps what was added by hand); its shuffle stays in its order.
- **A browser** playing itself keeps its own list in that browser (`localStorage`), as decided; Cratarr keeps none for
  it. The app and the players driven from a page are Cratarr's to keep.
- **For Home Assistant** (the owner, 2026-10-19: "proper Home Assistant integration"): `add` takes a playlist, an
  album or the favourites by name as `play` does (`{command: "add", album: "al-…", how: "next"}`), and every view
  carries `queues` (so the players stream and the webhook push the list). `play {radio: true}` starts a radio (decision
  0036): Autoplay seeded from the playlist, album, favourites or tracks named, else from the track that plays, in the
  player's Autoplay mode; its first five tracks play as a queue named "Radio: …", `from` `radio`.
