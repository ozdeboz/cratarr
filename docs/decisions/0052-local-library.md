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
  shuffle, repeat, sleep timer and the visuals (from the live sound: these files have no analysis). It works without a
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
  switch, a5), the server (a player in local mode: its reports leave Cratarr's queue alone, control commands pass,
  queue edits are refused, a play takes it back).
