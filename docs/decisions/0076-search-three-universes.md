# 0076: Global search in three tiers: the library, what is known elsewhere, and beyond

- Status: accepted (the owner, 2026-10-27: "global search should first look through our library, then through the
  indexed universe, and then through the unindexed universe"; MusicBrainz and Discogs "both together beyond your
  library"; Find more music one step earlier, automatically; "a good 9.13 feature"; and every group's results shown
  whole: "there is no way to show all of the categories sub-results")
- Date: 2026-10-27
- Release: 0.9.13
- Builds on: 0072 (one search, structured), 0.9.3's Find more music, 0069 (Discover), 0070 (requests), 0071 (Bandcamp
  previews), 0073 (Spotify lists), 0.9.5's Discogs connection

## Context

Global search (0072) answers from the library and, outside it, from two lists built ahead: the albums missing from the
library's artists (MusicBrainz's discographies) and what Discover leaves out. A live MusicBrainz search exists, but only
on its own page, Find more music, reached when a search finds nothing anywhere. Discogs is asked only in the
background, for covers, genres and styles; its search (styles, labels, catalogue numbers, pressings: where MusicBrainz
is thin) is never offered.

The owner's words for it: the **library**; the **indexed universe**, music not in the library that Cratarr knows of
through what is connected; and the **unindexed universe**, what only a live search finds.

## Decision

**One search, three tiers in this order, each thing once, in the first tier that has it.**

1. **Library:** as 0072 has it.
2. **Known elsewhere** (the indexed universe): from what Cratarr already keeps, at once: the albums missing from the
   library's artists, Discover's suggestions and what it leaves out, the account's Spotify lists (0073), its requests
   (0070), New to you's artists and their Bandcamp finds. Each item says why it is known (on your Spotify, missing from
   an artist you have, suggested, requested).
3. **Beyond** (the unindexed universe): MusicBrainz and Discogs asked live, together, merged: one album once (Discogs'
   master and MusicBrainz's release group joined by MusicBrainz's own Discogs links, else by name), marked with where
   it was found. Asked automatically once the words settle (about half a second, three letters at least); answers kept
   per query, so typing back and forth asks nothing again. Paced as each service asks (MusicBrainz one a second;
   Discogs' minute shared with its background work, which waits its turn).

- **Shown in tiers, asked together:** the library and the known answer at once; Beyond fills in below them, so nothing
  above it moves. The **Top** row is the best of all three: an exact match beyond outranks a weak one in the library;
  the tiers' own groups keep their order.
- **Opened as a missing album is** (lib/ElsewhereAlbum): Bandcamp's preview where found, else the menu with Want and
  the source's page. Want requests it by its MusicBrainz release group where known (a Discogs find mapped through
  MusicBrainz), else by name.
- **Every group shows all it found:** a group shows its first few, its total beside its name, and "Show all" opens
  that kind alone, the whole list in the same order, more as it scrolls (paged by the server). The library's groups,
  the known-elsewhere kinds and Beyond alike.
- **Find more music** becomes Beyond's "Show all": the full list, Discogs now beside MusicBrainz.
- Discogs' terms: its data is shown with its attribution.

## Endpoints (sketch, fixed when built)

- `GET /api/search?q=&kinds=&limit=&offset=`: `offset` pages one kind's whole list ("Show all": `kinds` one kind,
  `limit` a page, `offset` where the page before ended; the group's `total` says how many there are). It gains the
  known-elsewhere kinds (`spotify`, `suggested`, `requested`, beside
  `missing` and `excluded`), each item with `known: [why…]`.
- `GET /api/search/beyond?q=&limit=&offset=` → `{ q, albums: [{ title, artist, year, cover, release_group?, discogs?,
  sources: ["musicbrainz"|"discogs"], bandcamp?, request? }], artists: [...] }`: asked by the page once the words settle;
  cached per query.

## Consequences

- Finding music no longer ends at the library's edge; the three universes read in the order of how surely they are
  yours.
- Every settled search spends a few calls to MusicBrainz and Discogs; the cache keeps repeats free.
- The search's dedupe needs the known-elsewhere lists matched against each other and against the library by release
  group first, names second.
