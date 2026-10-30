# 0034: One Library page, and moving albums between libraries

- Status: accepted (the owner's answers and a review, 2026-10-16)
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

- its library, as a dot (below);
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
- **Lidarr:** "a folder Lidarr fetches into" is one of Lidarr's own root folders, read from its API (no setting).
  - Moving an album out of one unmonitors the album (today's Keep). If Music is one of them, Music → Incoming
    unmonitors as well: the album left.
  - Moving an album into one: Cratarr asks Lidarr for nothing, but Lidarr sees the files on its next rescan, as it
    already does after today's Keep.

Everything 0013 promises stays, now for any move:

- the plan is checked first (year, names, write access) and a refusal shows at once;
- the queue: one album and one file at a time, kept across restarts. A plain rename (same disk, "As it is") still
  waits its turn behind conversions, so a move is never promised to be instant;
- Cancel while the album waits or converts;
- Undo through the journal (files back, track ids back, Lidarr monitoring back);
- the media server hears of the move by folder;
- a playing album goes on playing on its new item;
- the event carries every track's move (`AlbumAdopted` generalises to `AlbumMoved`, with from and to), so playlists
  and play data follow;
- **the journal keeps reading old entries:** `AlbumMoved` takes `#[serde(alias = "album_adopted")]`, with from and to
  derived from the old entry's from_dir and to_dir, so Undo still works for every Keep made before;
- **track ids are kept:** the catalogue rows move with the files, as for a Keep today. Play On queues, the app's
  downloads and Subsonic song ids survive a move. Moves across file systems already work: moves.rs copies, then
  deletes, when a rename can't.

**Replace** is a move that takes the place of the album it upgrades in the target library. Pairing, the refusal when an
old track has no match, and the old folder going to the trash all stay as in 0013.

**Delete** works from any library, to the trash on that disk (outside the Music folder, 0010 and 0013), with Undo for
the trash days. Deleting from the **primary library** (Music: the one Music folder) takes **an extra step in its
confirmation**. The owner: "Primary should get an extra step in the confirmation." Deleting from another library
confirms as today.

**A library's colour and dot.** Each non-primary library has a colour, chosen in Settings → Libraries; a new library
gets one from a small palette until the user picks. The primary library has none, because it is the norm.

- An album from a non-primary library carries **a small dot in its library's colour** where it shows: its row
  (beside the title), its tile (beside the title), the panel's album header, and its menu's card.
- The filter's library chips show the same dot, so the dot means the same thing everywhere.
- The dots show while more than one library is switched on. With one on, every album would carry the same dot.
- No library names on rows or tiles: the dot is the label. The owner: "That really keeps it clean."
- The colour is kept with the library's settings on the server (settings.json, beside its use and naming template),
  so every browser and app shows the same one.

**The server** (the shapes are a sketch for the build):

- `GET /api/albums` lists the followed server's albums of its used libraries, each with its library and facts.
  - It takes Collection's cap (100,000; Incoming's list stops at 1,000), or pages, so Music is never cut off.
  - It replaces `/api/incoming` and `/api/collection`; those stay until the page has moved, then go.
- `POST /api/albums/{source}/{album}/move {to, year?}`, and `.../replace {to, year?}`, replace `keep` and today's
  `replace`. `to` is a library's id, not its name. `delete` stays.
- **Verdicts against every library** are cheap. A Music album's "an upgrade waits" is the inverse of today's map,
  which names the Music folder. Comparing Incoming libraries with each other takes one more small index per folder.
  Write access is cached per folder (600 s), not per album, so listing every Music album costs nothing more there.
- Home Assistant's `/api/library/*` calls keep their shapes:
  - promote moves into the Music library (with Replace when the album is an upgrade), converting as today, since
    Music's format policy is "CD format";
  - reject deletes.

**A dashboard page** (its own decision, later) brings back the quick look: new arrivals from the Incoming libraries,
what converts, and more.

## Settled with the owner (2026-10-16)

1. **Any direction:** "yes anyway, since it's generic". An album moves from any used library to any other of the
   same server, Music back into Incoming included. The target's policies decide what happens on the way in.

## One Music folder, as today (review, 2026-10-16)

The server has exactly one Music folder: the catalogue's folder 0. Every folder from 1 on is Incoming. Hanging on that:

- shelf.rs `Shelf::Music` / `Incoming`;
- the verdict index (folder 0 only);
- playlist lines (relative to folder 0, `../` for anything else) and the playlist store, which lives in Music;
- Subsonic album ids (bare for folder 0, `{folder}\` elsewhere, serve/ids.rs);
- album tags under `album_base` ("as it is or would be in Music");
- `music_name()`.

So this decision keeps **one Music folder**. Several Music libraries may still share it, as 0009 allows. Several Music
*folders* would need their own decision: folder roles, playlist lines across folders, album ids. With one Music folder,
Home Assistant's promote ("into Music") stays unambiguous.

## Settled with the owner, later the same day

- **Library labels:** only while more than one library is switched on, as a coloured dot rather than a name (above).
- **Delete from Music:** yes, with an extra confirmation step for the primary library (above).

## Consequences

- The navigation loses one entry: Library replaces Incoming and Collection. The Incoming tab's count of albums
  converting moves to Library, and later to the dashboard.
- A Music album gets the menu Incoming albums have: Move, Replace where it applies, and Delete (with its extra
  step).
- Music → Incoming changes the album's Subsonic id: it gains its folder's prefix. Tags follow through `album_base`,
  but an app that cached the album id holds a stale one.
- Settings → Libraries gains each library's format policy and, for a non-primary library, its colour.
- Work split:
  - web: the Library page, facts and dots on every album, Move and Delete (with the primary's extra step)
    in the menus and the select bar (several albums at once go through the queue one by one), the colour picker in
    Settings → Libraries, and the redirects;
  - server: one list endpoint with facts against every library, move and replace with the target's
    policies, delete from any library, the format policy and colour settings, and HA's calls on the new move.

## Amendment (2026-10-16, as built): each library keeps its own trash

The owner: "either library-specific trash bins or one global trash bin, but not one library's trash kept in another's
bin". Each library keeps its own hidden `.cratarr-trash`, Music's too: a delete stays an instant rename on its own disk.

- **Delete** from any library puts the album in that library's trash. This replaces "outside the Music folder" above.
- **Replace** puts the old Music album in Music's trash, no longer in Incoming's. Entries made before still undo from
  where their moves say.
- The trash days empty every library's trash.
- Lidarr is told only when an album leaves an Incoming folder (a move or a delete). Music stays out of Lidarr, so
  reading Lidarr's own root folders (above) waits until a library other than the Incoming ones is one of them.
- Built: `GET /api/albums`, `GET`/`POST /api/albums/{source}/{album}/move {to, year?}`,
  `POST /api/albums/{source}/{album}/delete`, and each library's format and colour in `libraries.policies`.
  Replace into a library other than Music is still to come.
