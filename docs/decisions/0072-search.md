# 0072: One search, structured: the pages', the panel's and the global one

Status: accepted, 2026-10-25 (the owner). For 0.9.3.

## Context

The owner, 2026-10-25: "If all of our lower level searches are structured, then having one global search becomes easy
… we should probably first focus at lower level searches being structured." The rule for every search is in
ui-conventions ("Search: the page finds, the panel narrows"): one query in the pill, the page shows what matches,
deep (a playlist by its tracks), the item opened in the panel shows what of it matches.

What there is now:

- **The server already searches** (serve/library.rs, the Subsonic `search3`): contentless FTS5 tables over songs,
  albums and artists, `unicode61 remove_diacritics 2`, each word as `(word OR word*)`, as Navidrome's.
- **The pages filter in the browser** (`matchesSearch` in lib/pageView): every word as a substring of the item's own
  texts, lower-cased, accents kept. So "bjork" finds Björk through a Subsonic app but not on Cratarr's own pages. A
  page cannot search deep: it holds the playlists, not their tracks.

What others do, read for this:

- Meilisearch's multi-search has two answers to one request: per index (grouped by kind) or "federated", one list
  merged by relevancy with a weight per index ([multi-search](https://www.meilisearch.com/docs/capabilities/multi_search/overview),
  [federated](https://meilisearch.com/docs/capabilities/multi_search/getting_started/federated_search)). A UI that
  shows both a top result and groups needs both.
- Spotify shows curated top results by default and filter chips by kind under the field (Top, Artists, Songs, Playlists,
  Albums…) ([Routenote](https://routenote.com/blog/spotify-adds-filters-to-search-results-on-the-mobile-app/)).
- NN/g: the default scope is everything; the scope shows by the field and at the top of the results; changing it
  after seeing results is what people do; never two search fields for one query
  ([scoped search](https://www.nngroup.com/articles/scoped-search/)).
- SQLite FTS5 ([docs](https://sqlite.org/fts5.html)): bm25, lower is better, with weights per column; `highlight()`
  and `snippet()` mark what matched; `prefix=` indexes speed up prefix queries; the trigram tokenizer finds
  substrings, but nothing under three characters. Navidrome 0.63 ranks exact matches above prefix-only ones, after its
  FTS5 move made short and non-ASCII names hard to find
  ([linuxiac](https://linuxiac.com/navidrome-0-63-music-server-and-streamer-adds-karaoke-style-lyrics/)).

## Decision

**One contract for every search**, so the global one only puts the others together.

**Deep search** (the owner, 2026-10-25): the words looked for *within* a set, among its members (an album by one of its
tracks, a playlist or a crate by the tracks in it), where an ordinary search matches the set's own attributes (its
title, its artist, its name). A set found deep says why (the member that matched, how many), and opened from the
search it shows itself narrowed to the words: its members that match.

1. **The query, read one way** on the server and in the browser (server `search::words`, web `lib/search.ts`, tested
   against the same examples): Unicode NFKD, combining marks dropped (as `remove_diacritics 2`), case folded,
   punctuation to spaces, split on spaces. Every word must match (AND), as a word's start (`word*`). A whole-word or
   whole-name match ranks above a prefix one (Navidrome's lesson). No operators: quotes and FTS syntax are words.

2. **A result, one shape** for every kind:

   ```
   { kind, subject, title, sub, cover?, score, matched: [{ on, text, count? }] }
   ```

   `kind`: album, artist, track, playlist, crate, genre, and the pages' own (request, download, problem, setting).
   `subject`: what the panel opens (`album:cratarr/<id>`, `playlist:<name>`…), so a result opens as anything does.
   `matched` says why it is there: `on` the name, an artist, an album, a track, a tag; `text` the part that matched;
   `count` how many (a playlist: "3 tracks match"). The page shows it under the item; the panel narrows to it.

3. **The server answers by kind, deep**: `GET /api/search?q=&kinds=&limit=` →
   `{ q, groups: { <kind>: { total, items } }, top: [...] }`. `groups` is the per-kind answer (Meilisearch's
   multi-index), `top` the merged one (its federated: by score, a weight per kind) for global search's Top results.
   Deep through the library's FTS: a playlist or a crate matches by its tracks (the tracks' FTS matches joined to the
   lists holding them), an album by its tracks, an artist by their albums. bm25 weights favour a name over its parts.

4. **Pages ask their kind**: Library albums, Playlists, Crates, Artists use `/api/search` with their kind and the
   pill's words; what the server gives back is what shows, each item with its `matched` line. Pages whose items only
   the browser holds (Settings, Health, Requests, Downloads, Insights) keep matching in the browser, but through
   `lib/search.ts`: the same words, the same shape. Each such page registers its matcher, so global search reaches
   them too.

5. **The panel narrows with the same words**: an album's, a playlist's or a crate's rows to those matching, the
   `matched` line's count the same as the rows shown.

6. **Global search last, by then mostly assembly**: everything by default (NN/g), Top results first, then a group per
   kind, a chip per kind to narrow after seeing results (Spotify's filters), the scope said at the top. Where it lives
   is decided when the pieces exist (the owner, 2026-10-25: no preference yet).

7. **What is not in the library is found too** (the owner, 2026-10-25): an album missing from an artist the library
   has (the discographies Discover reads) and what Discover leaves out (Not interested, Hide, artists left out) are
   kinds of their own (`missing`, `excluded`), so a title one looks for shows up even when it is not there, marked as
   missing or excluded. Insights › Collection › Discovery lists both, one level deeper each.

## Consequences

- Accents and case no longer split the server's search from the pages' ("bjork" finds Björk everywhere).
- Pages that held everything in the browser to filter (Playlists) may hold less: the server answers what matches.
- Search is a server feature with tests per kind; the browser side shrinks to `lib/search.ts` and its examples.
- Global search needs no matching of its own: `top` and `groups` from the server, plus the registered page matchers.
