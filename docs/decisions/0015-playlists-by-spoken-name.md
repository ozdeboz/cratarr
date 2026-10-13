# 0015: Playlists by spoken name: Cratarr answers, the assistant plays

- Status: accepted
- Date: 2026-10-13
- Builds on: 0014 (Cratarr is the playlist server; Jellyfin copies are targets)

## Context

A voice assistant (Home Assistant on the reference install) plays playlists by name ("play chill music"). It used a
fixed list of spoken names, each mapped to a Jellyfin playlist id. Since 0014 those ids belong to copies:

- a rename makes a new copy with a new id;
- a new playlist has no copy until Jellyfin imports it;
- a playlist without a Shared link has none at all.

The assistant already knows how to wake the player and cast tracks to it, and that knowledge is specific to each home.

## Decision

- **Cratarr answers, it does not play.** `GET /api/playlists/by-name?name=…&shuffle=…&limit=…` returns the playlist
  meant (or `null`) and its tracks as the server's ids, in order or shuffled, at most 100 (default 50). The assistant
  casts them with its own routine. Cratarr asks the source of truth, not a copy, so new and renamed playlists play
  at once and no Jellyfin playlist id is needed anywhere.
- **Loose matching, never a guess**, done in `voice.rs`:
  - the name folded (case, accents and punctuation gone);
  - then the one name that begins with what was said, else the one name it begins with;
  - then the clearly closest name;
  - a tie is no answer, and words around a name ("music", "playlist") are left out.
- **Spoken names per playlist** ("japanese" for Kokoro de Kiku), edited under "Spoken as…" in the playlist menu,
  kept with the sync state (they follow a rename).
- **No fallback to Jellyfin's copies** when Cratarr does not answer: one path. The assistant says that Cratarr is
  not answering, as it reports its other failures.
- **The assistant side stays the assistant's.** It owns its sentences, which device plays, waking it, and casting.
  On the reference install:
  - the existing `PlayPlaylist` intent, its sentence "play {playlist} [music|playlist|songs]" with a free-text name
    in place of the fixed list;
  - a `rest_command` to Cratarr;
  - the play script taking track ids.

  Not a sentence trigger: Home Assistant runs every trigger that matches, so a catch-all "play {name}" would also
  fire with each other "play …" automation. An intent is matched only when no trigger did. With a word after the
  name ("play chill music") it outranks the built-in media search; a bare "play chill" goes to that search.

## Consequences

- A voice assistant needs network access to Cratarr. Behind sign-in (planned, as Seerr does with Jellyfin) it
  will need an API key: a Voice assistant card in Settings will hold it, with a snippet to copy.
- Buttons and dashboards that hold Jellyfin playlist ids can move to asking by name, and a rename then breaks nothing.
