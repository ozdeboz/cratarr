# 0052: Local mode, the device's own music

- Status: accepted (the owner, 2026-10-19: "Yes.")
- Date: 2026-10-19
- Builds on: 0031 (the Android app and Play On), 0033 (Cratarr holds the queues), 0038 (kept tracks), 0051 (several users)
- Takes up: the "Local library" plan

## Context

The app plays what Cratarr's server serves; its kept tracks (0038) are the server's tracks copied to the device. Music
files that are only on a phone or a tablet (copied there, recorded, bought elsewhere) do not exist for Cratarr.

The owner (2026-10-19): local libraries first, played only on the device that has them, on any device with the app.
Then: "possibly a local mode so that the app can be used for local playback, but mixing it without having all the
features is weird"; and, asked whether a local queue goes away: no, each mode keeps its own.

## Decision

- **Two modes, never mixed.** The app plays either Cratarr (the server's music and queue, as now) or **this device**: a
  plain player of the device's own music. Their tracks never share a queue. A switch in the app changes the mode; each
  mode keeps its own queue (its tracks, the one that plays, the position, shuffle, repeat), so switching back resumes
  where it was, both ways.
- **Found by Android.** Local mode reads the device's music from Android's media index (MediaStore: music, not
  ringtones, alarms or recordings), with Android's audio permission, asked for the first time local mode is opened. No
  index of Cratarr's own; the server keeps nothing of it.
- **A player of its own.** Its albums, artists and tracks with their covers, searchable; play, play next, add to queue,
  shuffle, repeat and sleep timer; no visuals (the owner, 2026-10-19: "In local mode, the visuals feature is not
  supported."). It works without a
  connection to Cratarr. Its plays are not counted; nothing goes into crates, playlists, radio or Insights.
- **Seen and steered from outside, not edited.** While a device is in local mode Cratarr knows it (`mode: device`) and
  what plays (title, artist, album), so pages and Home Assistant show it, and may pause, resume, skip, seek and change
  its volume. Its queue is not Cratarr's to change: queue edits are refused ("That device plays its own music."), and
  Cratarr's queue for the device stays as it was meanwhile.
- **Cratarr takes over when asked.** A play from Cratarr (Home Assistant, another device, a handover to it, a queue
  taken up) switches the device back to Cratarr mode and plays; the local queue stays for when local mode comes back.

## Consequences

- One more permission, asked only when local mode is first opened.
- The app holds two queues; the server only ever holds Cratarr's.
- Bringing such a file into the library (Incoming) stays a separate step for later.
- Work: the app (the mode, MediaStore, covers, the local queue, its reports), the page (local mode's views and the
  switch), the server (a player in local mode: its reports leave Cratarr's queue alone, control commands pass,
  queue edits are refused, a play takes it back).

## As built (the app and the server, 2026-10-19)

- **The app's bridge** (the page inside the app):
  - The `app` message has `mode` (`cratarr` or `device`) and `deviceMusic` (`unasked`, `denied` or `granted`: Android's
    permission); it comes again whenever the mode changes, from the page or from Cratarr.
  - `{type: 'device-music', op: 'mode', mode}` switches; to `device` without the permission, Android asks first (refused:
    the mode stays, `deviceMusic` is `denied`). The player's next report has the new mode's queue.
  - `{type: 'device-music', op: 'library'}` is answered with `{type: 'device-library', tracks}` (newest first), or
    `{type: 'device-library', denied: true}`. A track: `{source: 'device', id, name, artist, album, album_id, track,
    disc, year, duration_secs, added}`; `track` is its number on the album (not `n`, a track's place in what plays).
  - `{type: 'device-library-changed'}` comes, unasked, when Android's index of the device's music changes (a couple of
    seconds after the last change): the page asks for the library again when it shows it.
  - Covers: `/__device/cover/{album_id}` on the page's own origin, a JPEG from Android or a 404.
  - Local folders (amended below): the `app` message's `local` (a folder on, the permission given: the switch shows);
    `{op: 'allow'}` asks for the permission; `{op: 'folders'}` is answered with `{type: 'device-folders', folders:
    [{path, name, tracks, on}]}` (or `denied: true`); `{op: 'folder', path, on}` turns one on (asking for the
    permission first when needed) or off, answered with `app` and `device-folders`. `mode` to `device` and the
    library need a folder on; the library holds only theirs.
  - Playing uses the `player` calls as ever. Their reports carry `mode`. A `queue` call with the other mode's tracks
    switches to that mode first; tracks of the other mode in `queue`, `add` or `upcoming` are left out. `radio`,
    `autoplay` and `skip_favourites` do nothing in local mode.
- **The app**: each mode's queue is put by on a switch (its tracks, the playing one and where, repeat, shuffle) and
  taken up again, paused; local mode's is also kept as it goes, so it lasts a restart. Entering local mode leaves a
  group. Radio, Autoplay, Levelling, keeping tracks ahead and counting plays are Cratarr's only. Without the permission
  any more, the app starts in Cratarr's mode.
- **The server**: a report with `mode: device` and `device_track` leaves the player's queue, history and give-back alone;
  the player's view then has `mode: device` and that track. Commands: pause, resume, toggle, next, previous, seek,
  volume, duck, unduck and sleep pass; stop pauses; play, a queue taken up and a handover to it pass (the app switches
  back); the rest, and a handover from it, are refused with "That device plays its own music."

## Live sound (the owner, 2026-10-19)

"I do want live sound, but no live visuals." The app sends the page the sound as it is heard (LiveSound: the
spectrum's six bands, energy and flux each 12 ms, and the K-weighted mean square), the page's live analysis keeping its
statistics: in the app the music plays natively, so the visuals of a track the server has not analysed yet have
nothing to listen to otherwise. Page to app `{type: 'live', on}`; app to page, about twenty times a second, `{type:
'live', at, tickMs, channels, meanSquare, ticks: [{energy: [6], flux: [6]}]}`, `at` the first tick's time as heard
(epoch ms). Local mode shows no visuals.

## The switch (the owner, 2026-10-19)

- **At the top of the page menu**: "Cratarr | Local", where the connection line is. The menu below shows what that mode
  has; in Local: the device's albums, artists and tracks with search, the player, the queue and the menus, nothing that
  is Cratarr's only (hearts, playlists, crates, radio, Insights).
- **The sidebar's top, made "more professional"** (mock B, "One switcher"): one bordered button with Cratarr's mark,
  the mode's name, and under it "● Connected" (Cratarr) or the device's name (Local), with a music note before it while
  Local plays; a tap opens the two to choose from. The owner: "I only want to show the drop-down component if the
  user has enabled any local directories. Otherwise, we show it as it is now. Also, I don't want any other icon than
  our own at the moment."
- **Local folders.** Local mode reads only the device's music folders turned on in Settings (This device): the folders
  Android found music in, by their top folder ("Music", "Download"), each with its number of tracks and a switch; the
  first one turned on asks for Android's permission. The switch shows once one is on; the last one off is Cratarr's
  music again.
- **The phone** (mock C): a slim row above the tabs, the same switch as the sidebar's in one line (the mark, the mode,
  "● Connected" or the device's name with its music note, the chooser's arrows), shown only as the sidebar's is: with a
  local folder on, else the bar stays as now. The owner: "It's either a slim row above the tabs or the expanded menu.
  And since I don't want to design the expanded menu now, we go for the slim row above the tabs." (The expanded menu:
  More growing the bar by a row, for later.)
- **A switch is the whole app, the player too**: what plays pauses and is put by, the other mode's queue comes back
  paused, where it was (as built).
- **Named Local**, the owner's own word; "This device" stays the name of the player here (where music plays, Settings).
