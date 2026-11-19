# 0105: No source ranks above another — the hand, then the files, then the services as equals

- Status: accepted (the owner, 2026-11-19, by question while testing 0.17's first preview); 0.17 in part, the rest
  in 0.18
- Date: 2026-11-19
- Builds on: 0097 (words: hand > files > services), 0100 (records, artists and labels), 0104 (the domain core)
- Changes: the counting order of MusicBrainz, Discogs and Bandcamp, which decided whose tracklist won

## Context

A record not had showed MusicBrainz's tracklist where Discogs listed more, because MusicBrainz was asked first and its
answer counted. The owner: one metadata provider is not more important than another, and when several versions are
found, none should be chosen automatically. The same week showed:

- a busy MusicBrainz making Discogs' reading fail with it;
- a Bandcamp page found by searching a name;
- a source's mark on a chip that said nothing about where the shown list came from.

## Decision

1. **The counting order only says who is asked first,** never whose answer wins. A busy or failing source is passed
   over and asked again later. Its absence never becomes an answer, and it never stops another source.
2. **Agreeing sources are one list,** with every source named. **Disagreeing sources are versions,** none chosen: the
   page shows each with its sources and its track count, and an administrator may pick one.
3. **The hand wins,** then the files, then the services as equals. A record you have shows its files' list. No
   source's list counts for it until an administrator picks one.
4. **The tracklist mark:** a list is the files' by default, unmarked. Any list that is not the files' names its
   sources, every agreeing one. So a record not had is always marked, and a record had only after a pick.
5. **Bandcamp is reached only through MusicBrainz's link,** never by searching a name.
6. **Credits are facts with sources too.** Where the files' artist tag differs from the release the files themselves
   name, the files line offers to write the fuller credit. The files stay the authority until written.

## Consequences

- 0.17:
  - a busy source passed over and asked again;
  - Bandcamp through MusicBrainz's link only;
  - Discogs' reading no longer stopped by a busy MusicBrainz;
  - an artist's Discogs list keeping their own records.
- 0.18 (with 0104's facts):
  - versions shown, none chosen;
  - the tracklist mark by rule 4;
  - credits as facts.
- Soulseek takes any whole version. A request is judged by the shortest list.
