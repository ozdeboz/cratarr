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
  account it plays as (play count, last played). Nothing else is reported in this stage: the media server and Home
  Assistant do not see the local player (decision 0019, stage 1).
- **Output switch** as decided in 0019: the player's output button, "This device" and the media server's players;
  switching moves the queue and the position and stops the other side.
- **The lock screen and media keys** work through the Media Session API (title, artist, cover, play/pause, next,
  previous, seeking).
- **First a feasibility spike**, before the rest is built: a FLAC album and an MP3 album played gaplessly through MSE
  on the tablet, from the repackaged stream, judged by ear by the owner.

## Consequences

- The server gains a streaming module (files, ranges, FLAC to fragmented MP4) and routes for track lists (a playlist,
  an album, the queue) and for marking a play.
- The interface gains a playback engine beside the remote (lib/remote.svelte.ts), and the player shows whichever
  output is active.
- Testing at the track boundaries is by ear and falls to the owner: skip, seek, pause, screen lock, and FLAC and MP3
  mixed.
- Stage 2 (the local player reporting itself as a media-server session, so Home Assistant sees it) builds on this.
