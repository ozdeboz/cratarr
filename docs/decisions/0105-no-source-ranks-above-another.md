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

## Amended 2026-11-20: a site's stamp is no value; artists' names (the owner, by question; for 0.18)

- **A value a download site stamped into a tag is no value.** Where the files' tags become facts, one pure rule cleans
  them: a value that is only a web address, a bare domain or "ripped by" is no value, and one trailing a real value is
  cut. The raw tag stays recorded. Health lists the files whose tags carry a website, opening the write sheet (0092).
- **An artist's name is a fact with sources too** (rule 6 grown): in 0.18 artists' names become facts with sources, the
  hand, then the files, then the services, and an artist can be renamed, by hand too. No tags are written for this in
  0.17.
- **Open, for the owner: an artist's name where the files pair it with another artist's id** (a group's record whose
  tags name the person). Recommended: an artist's name comes from its own id. The files' name counts for an artist only
  where it sits beside that artist's id; elsewhere the services' name for the id shows, and the files line offers the
  write; a hand rename is a hand fact, with Reset. The other answers: the hand, then the files, strictly, so both show the
  person's name, told apart by their numbers, until the tags are written or a hand renames; or write those records'
  artist tags in 0.18's first run.
