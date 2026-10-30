# 0022: The local player

- Status: accepted
- Date: 2026-10-14
- Builds on: 0006 (players), 0018 (the player and its chrome), 0019 (output: local or remote)

## Context

Cratarr's player is a remote for a media-server player (Finamp on the tablet). Decision 0019 made output a switch
between that (remote) and Cratarr playing in the browser itself (local), local first without telling the media
server about itself. This record is how local playback works.

A browser has three ways to play a list of tracks: an audio element per track (near-gapless: a few to tens of
milliseconds between tracks), Web Audio (sample-accurate, but each track decoded whole into memory, about 85 MB for
four minutes, and at risk of being stopped by Android in the background), and Media Source Extensions (MSE): one
audio element fed one continuous stream, the next track appended to it. Only MSE gives true gapless playback that
also runs on a locked phone. Its costs: browsers do not take a bare FLAC file in MSE (FLAC goes in fragmented MP4),
and MP3 and AAC files carry encoder padding at their start and end that must be cut to the sample, or the gap stays.

## Decision

The owner chose (2026-10-14), after the sizes were put to him (near-gapless about 1,500 lines and one test evening;
true gapless about 3,000 lines and at least two listening evenings):

- **True gapless playback from the first version**, through **one continuous MSE stream**: the playing track and the
  next ones appended to a single audio element, a timeline that maps stream time to the track and the position in
  it, seeking anywhere in it, and the stream's buffer kept within what a phone allows.
- **The audio comes through Cratarr**, never from the browser to the media server directly (the server's key stays on
  the server). Cratarr reads the **original files** from its own mount (the catalogue knows each track's file);
  only a track it cannot see is fetched from the media server.
- **Original quality**: no transcoding. FLAC is repackaged, not re-encoded, into fragmented MP4 by Cratarr's server
  (frames copied as they are); MP3 goes to the stream as it is. The padding information in MP3 (LAME/Xing header) and
  AAC (iTunSMPB) files tells how many samples to cut at each end; the stream's append window cuts them.
- **Plays count**: when a track has played (the media server's rule: most of it), Cratarr marks it played for the
  account it plays as (play count, last played). That account (also for favourites) is the Jellyfin connection's
  "Plays as" setting; left on Automatic it is the followed player's account, else the server's only account or its
  administrator (2026-10-14: before, nothing was counted and the hearts went when no Finamp session was open). Nothing else is reported in this stage: the media server and Home
  Assistant do not see the local player (decision 0019, stage 1).
- **Output switch** as decided in 0019: the player's output button, "This device" and the media server's players;
  switching moves the queue and the position and stops the other side.
- **The lock screen and media keys** work through the Media Session API (title, artist, cover, play/pause, next,
  previous, seeking).
- **First a feasibility spike**, before the rest is built: a FLAC album and an MP3 album played gaplessly through MSE
  on the tablet, from the repackaged stream, judged by ear by the owner.

## Feasibility (2026-10-14)

Measured to the sample with the gapless lab (`tools/gapless-lab`): reproducible noise cut into three tracks at awkward
points, played through one MSE stream, recorded silently by the page and compared with what a perfect player outputs.
On the owner's tablet (Chrome 155, Android 10) and on Windows: **no jump at any boundary** for FLAC (repackaged into
fragmented MP4), for MP3, and for FLAC, MP3 and FLAC mixed in one stream (switching format with `changeType`).

- **MP3 is trimmed by LAME's encoder delay only** (the `delay` field of its LAME header) at the start, and to its exact
  length at the end. Chrome's decoder drops the Info frame and adds no decoder delay; the textbook trim (1152 + delay +
  529) cut 1681 samples too many, heard as a click.
- MP3 has no timestamps of its own, so the stream runs in **sequence mode**; `timestampOffset` places each track.
- A page in the background on Android is paused: the player keeps playing from the lock screen through Media Session,
  but the lab needs its page in front.
- **The engine itself** (lib/localPlayer.svelte.ts, measured with web/engine-lab.html through the server's own FLAC
  repackaging): no jump at any boundary on the tablet and on Windows for FLAC, MP3 and FLAC/MP3/FLAC played as a
  playlist, with no stalls. FLAC runs in segments mode (its fragments carry their sample times, which also places a
  seek exactly), MP3 in sequence mode.

## Consequences

- The server gains a streaming module (files, ranges, FLAC to fragmented MP4) and routes for track lists (a playlist,
  an album, the queue) and for marking a play. **It must stream with small, bounded buffers** (a few FLAC frames at a
  time, never a whole file): production runs with a 64 MB memory limit and already uses 48-55 MB with the catalogue
  loaded (measured 2026-10-13); a larger limit is the owner's call on the NAS side.
- The interface gains a playback engine beside the remote (lib/remote.svelte.ts), and the player shows whichever
  output is active.
- Testing at the track boundaries is by ear and falls to the owner: skip, seek, pause, screen lock, and FLAC and MP3
  mixed.
- Stage 2 (the local player reporting itself as a media-server session, so Home Assistant sees it) builds on this.
