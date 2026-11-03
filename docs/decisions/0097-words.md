# 0097: Words — one truth, from every source to the files

- Status: accepted (the owner, 2026-11-02, by question sets in 632's session: "We need a North Star. And we need a
  robust road ahead"; earlier the same day in a5's session: mood is AudioMuse's, genre written back to the files,
  style, mood and tags Cratarr's own, a Catalogue sheet for writing them). The server's parts are a5's, the page's
  632's.
- Date: 2026-11-02
- Builds on: 0089 (facts set by hand), 0092 (one way to change files), 0.9.7's library words, 0.12's genre aliases,
  0095 (one way to say nothing)

## Context

A record's words (genre, style, mood, tags) came from several places and were kept in two stores that did not agree:

- **Crate conditions** read each track's facts: the files' Genre, STYLE, MOOD and Grouping tags, and AudioMuse's styles
  and moods.
- **Search, the word crates and similarity** read the library words, per record: the files' genres, MusicBrainz,
  Discogs, Last.fm and the household's tags, sorted into kinds by the word.

So a style could be found by search and not filter a crate, "Tag" meant the files' Grouping in one place and the
harvest's leftover words in another, a mood condition counted the files' own MOOD words (a crate of "melancholic"
gave whole albums), and only type, year, genres and Classical could be set by hand. Writing to the files had one
switch (types and years, Settings › Libraries) and a second was planned for words.

## Decision: the North Star

1. **Cratarr's data is the truth; the files mirror it.** A record's words live in Cratarr with their sources; a
   file read in is a source, not the truth; the files are written from Cratarr where the listener chooses.
2. **Words are a record's; a track has its own where it differs** (a compilation's tracks, a work's movement). A
   track reads its record's words and its own; every reader reads this one store.
3. **Hand, then the files, then the services.** What is set by hand wins (each field with Reset back to its
   sources); then what the files say; then the services (MusicBrainz, Discogs, Last.fm), weighted, a word below the
   floor left out. **Mood is AudioMuse's** (CLAP), hand able to override it for a record; the files' MOOD words are
   read again as genre, style or tag (a genre where they are one, "religious" a style, the rest tags).
4. **One spelling per word.** Case, spaces and hyphens fold by themselves; real synonyms are one alias list per kind
   (as 0.12's genres'), kept in the Catalogue sheet. Every reader sees the one spelling.

## The controls

- **One place writes to the files: the Catalogue sheet** (Server libraries › Catalogue). Each kind — type, year,
  genre, style, mood, tags — is "Leave the files' own" or "Write Cratarr's" (genre, type and year write by default).
  0.13's "Write types and years into files" switch moves into it. Each kind names its sources (no counts), each
  source on or off for that kind (off: its words leave the kind everywhere, kept to come back), and the kind's alias
  list.
- **Writing is always reviewed** (0092): Health lists the records whose files differ from Cratarr's, per kind set to
  Write; Review shows each field as the files have it and as Cratarr has it, and where from; Write; one Undo per run.
- **By hand:** a record's Edit sheet (genre, style, mood, tags, as 0089's fields, a selection at once), a track's Edit
  (its own words), a word's menu (a long-press on its chip: Rename everywhere, Merge into…, which writes the alias
  list; Remove from this record).
- **Shown:** a record's words under its title, one labelled row per kind it has (Genre, Style, Mood, Tags), each
  word a chip: a tap its crate, a long-press its menu. Where a word comes from: its menu's header ("Style · Discogs,
  AudioMuse"; "set by hand"), the Catalogue sheet, and Edit.

## The road

1. This decision (0.14).
2. **The store** (a5, 0.14): each record's and track's words by kind, in layers with their sources; the files'
   MOOD read again; the aliases folded.
3. **Every reader on it** (0.14): crate conditions, search, the word crates, similarity, the filter, 0095's reasons;
   the two old stores gone.
4. **The record's rows per kind** (632, 0.14).
5. **The controls** (0.15): the Catalogue sheet (writing, sources, aliases), Edit's style, mood and tags, a track's
   Edit, the words' menus; Health's differences per kind.

Each step on staging before the next.

## Kept from drifting

- **One reader test:** a shared fixture (`shared/words.json`, as `shared/rules.json` and `shared/addresses.json`): a
  record's words as every reader must see them; the server's and the page's tests check theirs against it.
- **The North Star in the docs:** these four rules at the top of docs/ui-conventions.md (Words) and in the
  architecture.
- **Health watches it:** the records whose files differ from Cratarr's, per kind set to Write.

## Consequences

- A word found by search is a word a crate's condition finds, and the other way round; a mood is a measured mood.
- The files change only where the listener chose a kind to be written, and only after Review.
- Nothing a source said is lost: a word switched off or folded stays in Cratarr, its source named.
