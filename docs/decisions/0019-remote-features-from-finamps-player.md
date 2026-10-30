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

## Amendment (2026-10-14): seeking by dragging

With the local player (decision 0022) Cratarr plays music itself, so seeking by dragging moved out of "later": playing
on this device the progress bar is Finamp's slider (the handle and the time follow the finger, the player seeks on
release). Seeking the server's player waits until it has been tried on the real player, as jumping in the queue does.

## Amendment (2026-10-19): the cover's overview, and a row the user fills

The owner: "Currently, the play count, BPM, and sound quality are also mentioned on the player itself. That's not
good"; "a circular gesture on the album cover ... blur the album cover and show the genres and the mood", "and also
the release date"; then "we should let users choose what to display on that extra bar"; "there should be an info
button on the left side"; "They should also be able to hide the bar from the settings page. And when there is nothing
in the bar, there should be a chip to go to the settings page to add one"; "for long content options, we'd need short
format versions. So make a list that is exhaustive"; "make the album of the song clickable while not making its style
any different".

- **The cover's overview** (lib/CoverOverview): a circle drawn on the full player's cover, or the row's info button,
  blurs the cover and shows over it every fact known about the track, in full: the genres as chips, then mood, release
  date, tempo, key, plays, last played, quality, label, catalogue number, when added, energy and danceability (bars),
  composer, track (and disc), explicit. A tap on the cover or another circle brings the cover back; a new track does
  too. Read from the file's details (its tags, tempo, genres, quality) and the plays; mood, energy and danceability
  from AudioMuse once the server imports them. None for the device's own music.
- **The info chips go.** In their place a row: the info button at its start, then the facts the user picks, short
  ("2021", "Am", "FLAC 24/96", "Richter"), in the order the user drags them (Settings → Display → Player row, each
  with a switch). None picked: an Add chip leads there. The row can be hidden there. It gives way when the player is
  short of height, as the chips did.
- **The album's name** under the title opens the album in the panel, as the artist's chip opens the artist; it looks
  as before.
- **The waveform lens** (the owner's idea: "It's awesome. We build it like this."): holding the seek bar over a
  waveform shows a 130 px round lens instead of the dot (the plain bar keeps its dot), its bottom edge 36 px above the
  waveform's middle, a stem from the point and a white line across the waveform there. Inside, the waveform under it
  three times as finely (the same 512 bins, a bar per 3 px), played in the accent left of the point, and a line at
  the point; its centre keeps 10 px inside the player. The time where it will land sits above it. Release seeks.
