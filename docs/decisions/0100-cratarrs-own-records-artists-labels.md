# 0100: Cratarr's own records, artists and labels, mapped to every source

- Status: accepted (the owner, 2026-11-03: "We should have a master record"; its four choices by question; for 0.16)
- Date: 2026-11-03
- Builds on: 0097 (Cratarr's data is the truth), 0.14's references (MusicBrainz, Discogs, Bandcamp per record, one that
  counts), 0069 (Discover, discographies), 0070 (Requests), 0076 (three-tier search), 0099 (Soulseek in Cratarr)
- Changes: 0099 (a record judged by its reference, whichever source it is; never a MusicBrainz id required)

## Context

Soulseek in Cratarr was meant to need no MusicBrainz ("otherwise we could have just used Lidarr", the owner;
docs/plans.md: "Searching Soulseek needs no MusicBrainz; judging what was found does: a reference says which tracks
and lengths the release has"). As built in 0.15 it takes a Request only with a MusicBrainz release group and judges a
folder by MusicBrainz's release. Around it, MusicBrainz alone decides what a record is outside the library:

- **An artist's missing albums** are MusicBrainz's release groups only, kept by release group; an artist MusicBrainz
  cannot name has none. Discogs' discography is unused though the artist's Discogs id is known; Bandcamp's only adds
  a play link.
- **An artist one has nothing of** opens only from Discover, a Bandcamp album or Spotify; search's "beyond" opens
  musicbrainz.org. No Discogs profile, no related artists.
- **Labels** are a name from the tags or Discogs, no id, no page, no discography.
- **Want** works for anything, but a record without a release group never reaches Soulseek; one found on Discogs or
  Bandcamp alone never gets one.

0.14's references already hold, per record in the library, what MusicBrainz, Discogs and Bandcamp say and which one
counts. Nothing holds a record one does not have yet, nor an artist or a label.

## Decision

**Cratarr keeps its own record, artist and label, each mapped to whichever sources know it; none is required.**

1. **A record of Cratarr's own**, owned or not: its own id, its title, artist, year, type, and its references (a
   MusicBrainz release group and release, a Discogs master and release, a Bandcamp album), one counting. A record in
   the library is one of these with its files; a missing one, a Request, a Soulseek fetch point at the same record.
2. **An artist of Cratarr's own**: its references (MusicBrainz artist, Discogs artist, Bandcamp page), its records from
   every source's discography merged into Cratarr's records (one record per release, however many sources know it).
3. **A label of Cratarr's own**: its references (MusicBrainz label, Discogs label, Bandcamp label page), its records
   from each source's catalogue merged the same way. A label opens a page of its own: what one has, what one has not.
4. **Merging** across sources: by the sources' own links first (MusicBrainz's Discogs and Bandcamp URLs), then by
   artist, title and the tracklist (count and lengths), never by name alone; what cannot be told apart stays two.
5. **Soulseek judges by the record's reference**, whichever counts: its tracklist's titles and lengths. A Request
   points at a record; a record any source knows can be fetched.

## The owner's choices (2026-11-03, by question)

- **The reference that counts** is the one that fits: the source whose tracklist agrees with the files (a record one
  has, or the folder Soulseek found); where there is nothing to hold it against yet, MusicBrainz, then Discogs, then
  Bandcamp. Another can still be chosen by hand (0.14).
- **An artist's page** lists albums and EPs one does not have, as today's missing list: no singles, compilations, live
  or remix releases.
- **A label's page** is its whole catalogue from every source, by year, what one has marked, the rest one tap from Want.
- **Search's "beyond"** opens Cratarr's own artist and record pages, each source's site one tap from its references.

## Consequences

- One missing list from three discographies; artist pages for anyone; label pages; Soulseek without MusicBrainz.
- Records, artists and labels kept on the server (the store grows with what one looks at, not the whole catalogues).
- 0.15 ships Soulseek as built (MusicBrainz records only), said in What's new; this is 0.16.
