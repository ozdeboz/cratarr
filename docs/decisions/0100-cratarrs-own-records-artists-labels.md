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

## Amended 2026-11-18: built in 0.17's first preview

0.16 built the records you have. The rest of this decision came in 0.17's first preview. How it is built is in
[records-and-copies.md](../records-and-copies.md); what the build settled is here. The open points took the recommended
answer unless the owner chose otherwise, as noted.

- **Merging (point 4)** is `same_listing`. A link from either side decides, both ways. A release or a Bandcamp album
  names one record; two release groups or masters that differ are two, and the same one says nothing, as a deluxe
  edition shares its group. Without a link: the artist, the title and the tracklist. A list with no track lengths never
  merges; it may count where every title agrees.
- **Two records found to be one:** the one with copies keeps its id, else the older. The other's id answers by it.
- **Records you do not have are kept while something uses them** (the owner, 2026-11-16). One that no list showed and
  nobody opened for six months is forgotten, and a tombstone keeps its id. A request, a choice by hand, a merge, a
  Soulseek item or a retired record keeps it. Kept tracklists go after thirty days unopened; an artist left with nothing
  is forgotten too. The library's artists' lists are read again once a month, spread over the month.
- **Kept at open:** a label's page and search's "beyond" keep their sources' rows as a cache, not as records. A record
  is made only when someone opens, wants or links an album, and an artist only when opened, never by a name.
- **Labels (point 3):** a label's page lists its records from MusicBrainz's and Discogs' lists, and the records tied to
  it. Bandcamp's label pages are not read: Bandcamp answers them with a browser check, and Cratarr does not work around
  it (the owner, 2026-11-17). The first hundred of each list are read at open; "More" reads on.
- **Bandcamp in the missing list:** an artist's albums come from Bandcamp's search on the artist's own address, not from
  their `/music` page. Bandcamp has an administrator's switch, and after a browser check Cratarr stops asking it for a
  day.
- **Artists:** one page by id for every artist, with many albums here, one or none. Offline is the exception: a device
  holds only what the library has (the owner, 2026-11-16). Each artist of a name has a number for life, in the order
  they were made; a bare name from outside opens the first. An artist the files credit is not split in two by references
  yet.
- **A retired record** is a record you do not have, with Want and "Was in Music until <date>". A record you do not have
  keeps its Bandcamp preview.
- **Soulseek (point 5)** judges by the record's counting list, whichever source gives it. A record with no list anywhere
  is Stuck, and says why. A file with no MusicBrainz recording is judged by its sound, title and length, and AcoustID is
  not asked.
- **A request names its record,** found by its id, group, Bandcamp page or names, never by a title alone.
