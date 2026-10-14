# 0019: What the remote takes from Finamp's player

- Status: accepted
- Date: 2026-10-14
- Builds on: 0006 (players), 0018 (the player and its chrome)

## Context

Cratarr's player is a remote for a player elsewhere (Finamp on the tablet), driven through the media server's
commands. Finamp's full player was inventoried from its source (branch `redesign`, 2026-10-14): whole-screen and
cover gestures, the "Playing from" title, artist and album chips, a heart (add to playlist; long-press favourite),
feature chips (explicit, play count, codec @ bitrate, bit depth, sample rate, normalisation, speed, volume), a
seekable progress slider, shuffle / previous / play / next / repeat-or-radio, a bottom row (output, queue, lyrics),
a track menu (shuffle, sleep timer, speed, repeat, play / play next / add to queue, add to or remove from playlists,
instant mix, radio, download, favourite, playlist from the queue, stop and clear, delete from server), a queue sheet
(previous, current, Next Up, queue; tap to jump, drag to reorder, swipe to remove) and a lyrics screen.

## Decision

The owner chose (2026-10-14):

- **Now, as a remote:**
  - **Info chips** under the title, read-only: play count, codec @ bitrate, bit depth / sample rate, explicit, as the
    media server knows them.
  - **A track menu** for the playing track (a ☰ button in the player, and a right-click on the cover): play next,
    add to queue, the playlists that hold it (add or remove), the favourite, and **New playlist from the queue**
    (what plays and what comes next, saved as a Cratarr playlist; only tracks in the library).
  - **A queue view**, opened by a button in the player (swipe up stays the playlists): the current track, then what
    comes next as the player reports it. A tap plays that track. Nothing else: no reordering or removing.
- **Never:** lyrics; choosing the device's audio output (speaker, Bluetooth; the browser and the system decide where
  a web app's sound goes).
- **Output = local or remote** (owner, 2026-10-14), for when Cratarr plays music itself: Finamp's output button
  becomes a switch between **local** (Cratarr plays in the browser) and **remote** (it controls a media-server player,
  as today: Finamp on the tablet), handing the queue and the position over. In two stages: first the local player
  without telling the media server about itself; later it reports itself as a media-server player (a Jellyfin
  session), so the server, and through it Home Assistant, can see and control it. Not in the remote-only version.
  Settled with the owner (2026-10-14):
  - **Switching moves the music:** the queue and the position go to the new side, which plays from there; the old side
    stops (as Finamp's and Spotify's devices).
  - **The switch is the player's output button** (Finamp's place for it), opening a sheet with "This device" and the
    media server's players; it replaces today's player picker ("Playing on Finamp · Lenovo Tab ▾").
  - **Stage 1 is accepted without Home Assistant:** local playback is for the phone or headphones; until stage 2 Home
    Assistant does not see it (the amp's in-use sensor and the salon routing keep following Finamp on the tablet).
- **Later, when Cratarr is a player itself:** shuffle and repeat, seeking by dragging, the sleep timer, speed, radio
  and instant mix, downloads, the "Playing from" source, artist and album pages, editing the queue, keyboard
  shortcuts.

## Consequences

- The server reports more about the playing track (the info chips) and gains requests for one track (play next, add
  to queue), for jumping in the queue and for a playlist made from the queue.
- Jumping in the queue depends on what the player does with the media server's play command; it is tried on the real
  player before it ships, and dropped if the player cannot do it without losing its queue.
- The rest of Finamp's player stays out until Cratarr plays music itself.
