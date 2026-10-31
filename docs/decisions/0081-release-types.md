# 0081: Release types: MusicBrainz's release group, else the Discogs release that is the album

- Status: accepted (the owner, for 0.9.6: "hide EPs and singles"; amended for 0.10 and 0.13)
- Date: 0.9.6, written down 2026-10-30
- Builds on: 0.9.5's original years from MusicBrainz and its Discogs connection, 0024 (crates)
- Amended by: the amendments of 2026-10-30 (a Discogs release must fit the album; editions) and 2026-10-31 (the
  owner's rule, by tracks and length) below

## Context

Crates filter by an album's type (Album, EP, Single, and secondary types such as Compilation or Live),
so EPs and singles can be hidden. The files rarely say which an album is.

This record writes down what 0.9.6 built (`server/src/app/originals.rs`, `discogs.rs`, `serve/library.rs`
`release_type_of`), so that the change below has a decision to amend.

## Decision

- **MusicBrainz first.** An album with a MusicBrainz release takes its release group's primary and secondary types,
  asked in the background with its original year and kept in user data (`release_types`, `mb:<release>`).
- **Discogs else.** An album MusicBrainz gave no type takes the format descriptions ("EP", "Single", "LP",
  "Compilation") of its Discogs master's main release, kept by album (`dc:<album id>`). The master is MusicBrainz's link
  from the release group, else Discogs' search by artist and title.
- **A guess for a single only.** With neither, three tracks or fewer under 15 minutes is a single (`guess`); nothing
  is ever an EP by its length.
- **Kept, not written.** The library is built with them (`albums.release_type`, `type_from`); no file changes.

## Consequences

- One type per album, with where it came from, for the axes and OpenSubsonic.
- A release group's type is every one of its editions'.

## Amendment (2026-10-30): a Discogs release must fit the album; editions

Eight of the ten singles and EPs Discogs typed were over 30 minutes: albums typed from their same-named title-track
single or EP (The Avalanches' Since I Left You a single, Wagner's 220-minute Die Walküre an EP). And expanded or
deluxe editions of an EP stayed EPs (Aphex Twin's Digeridoo, 68.6 minutes), because the release group's type is every
edition's. The owner, for 0.10: fix both; about 45 long dance EPs are genuine and match their release, so no rule by
length alone.

- **The Discogs release must be the album.** Every one of Discogs' first five search results named as the album is
  tried in turn (MusicBrainz's link alone, where there is one), and a master's main release is taken only when its
  tracklist fits the album: by length where Discogs gives every track's, within 15%; else by tracks, within two or a
  fifth, whichever is more (an index track's parts each count, headings none). None fits: the type is left unknown,
  never taken from another record.
- **An edition is compared with its group's first release.** An EP or single from MusicBrainz is compared with its
  release group's first release (an official one where there is one, the earliest dated). More than twice its tracks,
  or more than twice its length where MusicBrainz gives every track's, and the album is an Album (`type_from` stays
  `musicbrainz`). A long EP as long as its group's own release stays an EP; one with no release to compare with too.
- **What an older rule found is redone by the passes.** Each kept type carries the rule it was found by
  (`release_types.rule`, `RELEASE_TYPES_RULE` = 1). Every one of Discogs' and every EP or single of MusicBrainz's found
  by an older rule is asked again, the old type kept until the new one is found; no wipe by hand. Albums Discogs typed
  are among those its pass looks at again.

## Amendment (2026-10-31): the owner's rule, by tracks and length

Going through the Library's Singles and EPs, the owner found MusicBrainz's and Discogs' types wrong for their purpose:
MusicBrainz's Single means one main song with versions or B-sides, not a short release (Passiflora, one track of 21.7
minutes; Kime Ne, 56.6 minutes), and an EP of either may be an album of eight to twelve tracks (Plantation, ten tracks,
63 minutes). Every verdict below is the owner's, for 0.13. It replaces the first amendment's edition rule and the guess
for a single.

- **The type said is kept as said** (`release_types`, unchanged), and the rule reads it as the library is built
  (`serve::library::release_type_of`). In order:
  1. **Incomplete**: an album found missing tracks of the smallest release that holds it (`app::incomplete`, the
     check Health shows) is `incomplete`, whatever else is true. Only where that was found, never guessed.
  2. **The name wins**: "… EP" makes an EP, "… LP" an album.
  3. **Seven tracks or more, or over 45 minutes**: an album, however short. Not Apple's 30 minutes: the owner's EPs reach
     44 minutes (Afro Damba, 43.6), their albums start at 47 (Flowery Rave Tales, 47.1).
  4. **Four to six tracks**: an EP.
  5. **One to three**: an EP where over 15 minutes or a track is 10 minutes or longer, else a single.
  6. **A Single said is an EP at most**: one song in versions stays an EP however long (the owner: "56 minutes for
     Kimene, capped at EP").
- **The same for albums nothing is said of**: the rule types them too; before, only a short one was guessed a single.
  A Broadcast (Other) stays as said.
- **Various Artists is a compilation**: an album whose album artist is Various Artists has the compilation secondary
  type besides (COD3 QR 015 had none from MusicBrainz).
- **Where from, and why**: `type_from` names the source that said a type (`musicbrainz`, `discogs`; none where none
  did), and `type_why` the rule that decided where the type is not the one said (`incomplete`, `name`, `tracks`,
  `length`, `one song`). Incomplete is a kind of its own beside Album, EP and Single.
- **The edition rule is gone**: the first amendment's comparison with a group's first release (`edition_kind`) is
  removed; the album's own tracks and length decide. MusicBrainz's types it changed are asked again
  (`RELEASE_TYPES_RULE` = 2, every one of MusicBrainz's found by an older rule), the old type kept until then.
