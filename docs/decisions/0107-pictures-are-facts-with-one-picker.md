# 0107: Pictures are facts, with one picker for every kind

- Status: accepted (the owner, 2026-11-19, by question); built in 0.18 with the core's artists and labels
- Date: 2026-11-19
- Builds on: 0104 (the domain core), 0105 (no source ranks above another), the cover rules of the record pages

## Context

An artist's photo was taken from TheAudioDB alone, and one such photo was stretched in TheAudioDB's own upload. The
owner: an artist's picture is just one type of metadata, and like a record's cover it should have a picker. Discogs
often has a better one.

## Decision

1. **A record's cover, an artist's photo, a crate's or a playlist's picture** each keep every source's candidate. For
   artists those are Discogs, TheAudioDB, Wikimedia Commons, and Bandcamp through MusicBrainz's link. An upload sits
   beside them.
2. **No source ranks above another.** Until a hand picks, the first source to answer shows.
3. **The pick:**
   - is kept on the object's own id, never on a name, so namesakes stay apart;
   - is never replaced by a source's re-read;
   - has one Undo;
   - is an administrator's;
   - a device's own picture stays first on that device.
4. **A picture's address carries a version,** so devices and the offline cache show a new pick.
5. **One picture picker** serves every kind: the record's cover picker made general.
6. **The original is kept,** and each place crops it to its shape.
7. **No new services for pictures** for now. Others may be weighed much later.

## Consequences

- The library's hand-picked pictures are data the backup must hold. A source's pictures can always be read again.
- The music folders get no artist image. A flat library has no artist folder, so other apps don't see the pick.
