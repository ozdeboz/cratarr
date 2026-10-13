# 0016: Music's layout: one folder per album-artist credit, never a Lidarr root

- Status: accepted
- Date: 2026-10-13
- Builds on: 0013 (Music named by its template, Lidarr not responsible for the collection)

## Context

On the reference install Music is flat: 2,230 album folders side by side (`Artist - Album (Year)`). A media server
finds new things by reading again the folder that holds them, and Jellyfin reads it again all the way down. In a flat
library that folder is Music itself: a kept album that showed up 10 s after the library was asked to look kept
Jellyfin at 100–200% CPU for 10–15 minutes on the NAS's N95 while it checked the other 24,000 tracks (2026-10-13).
Nested, a new album's folder sits in its artist's folder, and only that is read again.

Nesting means choosing what the first level is when an album has several album artists:

- **one folder per album-artist credit**, as MusicBrainz Picard does (`%albumartist%/%album%`): "Asuna & Jan
  Jelinek" gets its own folder next to `Asuna/` and `Jan Jelinek/`;
- **one folder per primary artist**, as Lidarr does: an album belongs to exactly one artist (Lidarr lists "Signals
  Bulletin", by Asuna & Jan Jelinek, under Asuna only, though both are in it).

Spotify and Discogs need not choose: they keep no files, and an album shows on every credited artist's page. Jellyfin
builds its artist pages from the tags (`albumartist`, several values) wherever the folder is, and so does Cratarr.

On the reference install about 4% of albums (99 of 2,231) have a combined credit, and 85 of the 91 combinations occur
on one album only.

## Decision

- **One folder per album-artist credit.** The folder is the `albumartist` tag, as written: `{albumartist}/{album}
  ({originalyear})` / `{disc-}{track:02} {artist} - {title}` on the reference install once it is nested.
- **No "primary artist" rule.** Cratarr's naming template offers no token that picks one artist out of a credit.
- **Compilations go under `Various Artists/`,** MusicBrainz's name for them. On the reference install the 132 albums
  tagged "Various" get that album artist in the same pass that moves them.
- **The folder shows the credit; the list tags link the artists.** The album-artist tag (`albumartist`) holds the
  credit as text ("King Tubby meets Lee 'Scratch' Perry"), and the folder is made from it. `albumartists` (and
  `artists` per track) holds each artist apart, so a media server links the album to every one of them (Jellyfin:
  "prefer non-standard artists tags" on, its own delimiter splitting off). This is how Picard writes them.
- **Names come from the library, not from MusicBrainz.** MusicBrainz is used as evidence only: which release an album
  is (checked against its track list), how many artists a one-string credit holds, and which spellings in the library
  belong to one artist. Its names are not a naming source: a moniker is sometimes an artist of its own (Aphex Twin /
  AFX) and sometimes another name of one (Caribou / Manitoba), and a credit is printed per release, so one artist's
  credit varies ("Andre Bratten" on 2 of 7 releases). Following it would split one artist over several folders. The
  owner settles spelling variants once; the folder follows the tags.
- **Characters, as Picard does:** every character Windows cannot take becomes `_`: a `/` inside a value
  (`yes/and` → `yes_and`), `:` `?` `"` `*` `<` `>` `|` `\`, a folder name's last dot (`Howie B.` → `Howie B_`) and a
  name's first dot. The reference library used `-`, `!` and `'` for some of these (37 album folders, 259 tracks);
  those names are brought to `_` in the same pass as the move, so Music keeps one convention.
- **Music is never one of Lidarr's root folders.** Lidarr fetches into Incoming; Cratarr moves albums into Music and
  names them (decision 0013). Lidarr's one-artist-per-album model would want collaborations moved into one partner's
  folder, and its renames would undo the template.

Why not one folder per primary artist:

- **What it would give:** cheaper scans only for a new collaboration whose primary artist already has a folder (at
  most ~4% of new albums); no extra folders for spellings of the same credit ("A & B", "A and B"), a handful; and
  Lidarr's layout for collaborations, only where its choice and ours agree.
- **What it would cost:** a rule the tags do not hold. Splitting credit strings gets single artists wrong ("Ak'chamel,
  The Giver Of Illness", "Earth, Wind & Fire"); MusicBrainz artist ids would be needed, and 14% of the reference
  library has none. An album would move whenever the rule's input changed.
- **"All of an artist's albums in one folder"** is not a gain either way: collaborations sit in another folder in both
  layouts. Artist pages come from the tags.

## Consequences

- A path follows from the tags alone, and Cratarr's naming check (Settings → Names in Music) can measure every folder
  against them.
- A new collaboration is a new first-level folder; reading the library root again for it costs what a flat library
  costs for every album. That happens for a few percent of new albums.
- Moving the reference library to the nested layout is its own job, done offline in one window with Cratarr stopped:
  the tags are rewritten and every file moved from one old-to-new path list, `cratarr-server --rewrite-playlists`
  rewrites the playlists (and Cratarr's backups of them) from the same list, and the catalogue is made afresh, which
  makes every sync link merge afresh. Play data in the media server follows by path, outside Cratarr. Until then the
  flat template stays in use. Every file changes path, so the offsite copy uploads the whole library again; the owner
  bundles the pending tag work into the same pass, so that happens once.
