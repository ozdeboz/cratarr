# 0034: One Library page, and moving albums between libraries

- Status: proposed
- Date: 2026-10-16
- Builds on: 0009 (library uses), 0013 (Keep and Replace), 0028 (Collection and one album list), 0032 (Cratarr is its
  own source)
- Changes: 0028 (Incoming and Collection become one page), 0013 (Keep becomes a move)

## Context

Incoming and Collection have grown into the same page:

- the same list (`lib/AlbumShelf.svelte`) with the same filter, View, list header, select mode and select bar;
- the same reading on the server (`app/shelf.rs`, per library);
- every listed album says which library it came from (`source::Album.library`, 2026-10-16).

The filter's library switches (2026-10-16) already decide which libraries a page shows. They are remembered with the
server as the one filter that holds everywhere. Since the Jellyfin source went off (2026-10-16), a browser follows one
server.

What only Incoming has today:

- per album, its verdict (an upgrade or a duplicate of an album in Music), whether Cratarr can write its folder, and
  its conversion while it is being kept;
- the trash days;
- Keep, Replace and Delete.

The owner, 2026-10-16:

- "these are two pages of the same; our whole keep flow is only a special case of moving content between libraries";
- one Library tab; a dashboard page later keeps the nuance and the quick access;
- "keep becomes a generic move".

## Decision

**One Library page** (`#/library`) takes the place of Incoming and Collection in the navigation.

- It lists every album of the followed server's used libraries (Music and Incoming, decision 0009) as one list.
- Which libraries show is the filter's library switches: remembered, and the last one on stays on.
  - Every used library's chip is a switch on this page.
  - The page is not split per library: an album's library is one of its facts.
- Order, direction, layout, size and dates stay as they are. The default order is Added, newest first, so a new
  arrival shows on top.
- `#/incoming` and `#/collection` open Library and change no switch, because the switches are remembered.

**An album's facts show wherever it shows**, not only on Incoming:

- its library;
- how it relates to albums in the other used libraries: an upgrade of one, or a duplicate of one (0013's verdicts,
  now against every other used library, not only Music);
- whether Cratarr can write its folder;
- its move while one runs (waiting, converting n of N).

**Move to a library replaces Keep.** An album moves from its library to another used library of the same server, from
its menu or, for several, from the select bar.

The target library's policies apply on the way in:

- **Naming:** the target's naming template (0013, already per library).
- **Format:** the target's format policy, a new per-library setting in Settings → Libraries. "CD format" converts
  lossless files above CD to 16-bit/44.1 kHz FLAC, as Keep does today. "As it is" moves the files unchanged. Music's
  setting is "CD format" and Incoming's "As it is", as today's behaviour.
- **Lidarr:**
  - moving an album out of a folder Lidarr fetches into unmonitors it (today's Keep);
  - moving one into such a folder leaves Lidarr alone. Lidarr fetches; Cratarr does not ask it to.

Everything 0013 promises stays, now for any move:

- the plan is checked first (year, names, write access) and a refusal shows at once;
- the queue: one album and one file at a time, kept across restarts;
- Cancel while the album waits or converts;
- Undo through the journal (files back, track ids back, Lidarr monitoring back);
- the media server hears of the move by folder;
- a playing album goes on playing on its new item;
- the event carries every track's move (`AlbumAdopted` generalises to `AlbumMoved`, with from and to), so playlists
  and play data follow.

**Replace** is a move that takes the place of the album it upgrades in the target library. Pairing, the refusal when an
old track has no match, and the old folder going to the trash all stay as in 0013.

**Delete** works from any library: to the trash on that disk, outside the Music folder (0010, 0013), and Undo puts it
back. Today it works from Incoming only.

**The server** (claude-5f's side; the shapes are a sketch for the build):

- `GET /api/albums` lists the followed server's albums of its used libraries, each with its library and facts. It
  replaces `/api/incoming` and `/api/collection`; those stay until the page has moved, then go.
- `POST /api/albums/{source}/{album}/move {to, year?}`, and `.../replace {to, year?}`, replace `keep` and today's
  `replace`. `delete` stays.
- Home Assistant's `/api/library/*` calls keep their shapes:
  - promote moves into the Music library (with Replace when the album is an upgrade);
  - reject deletes.

**A dashboard page** (its own decision, later) brings back the quick look: new arrivals from the Incoming libraries,
what converts, and more.

## Open questions for the owner

1. **Any direction?** The proposal is any used library to any other of the same server, including Music back into
   Incoming. The target's policies decide what happens on the way in.
2. **More than one Music library?** Several are allowed, as 0009 already allows. A move between two of them is then a
   rename plus the target's format policy.
3. **Library names on the page.** An album's library shows as its name (a chip on the row, a fact on the tile), or
   only when more than one library is switched on.

## Consequences

- The navigation loses one entry: Library replaces Incoming and Collection. The Incoming tab's count of albums
  converting moves to Library, and later to the dashboard.
- A Music album gets the menu Incoming albums have: Move, Delete, and Replace where it applies.
- Settings → Libraries gains each library's format policy.
- Work split:
  - web (claude-a5): the Library page, facts on every album, Move in the menus and the select bar (several albums at
    once go through the queue one by one), and the redirects;
  - server (claude-5f): one list endpoint with facts against every library, move and replace with the target's
    policies, the format policy setting, and HA's calls on the new move.
