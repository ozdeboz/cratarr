# 0069: Discover: music outside the library, from what you listen to

- Status: accepted (the owner, 2026-10-24: "I wouldn't be interested in suggestions within my library, but
  suggestions outside of"; "we need a discover page next to playlists")
- Date: 2026-10-24
- Builds on: 0013 (Incoming: Keep and Delete), 0036 (radio), 0064 (the listening history), 0067 (one import, its
  matcher), 0068 (the listening services)

## Context

Everything Cratarr suggests so far comes from the library: the radio, Home, More music, the crates. What is not there
yet, Cratarr cannot point to. The listening services can: ListenBrainz works out recommendations for each listener from
everyone's listens (collaborative filtering, by MusicBrainz ids), and makes weekly playlists for them (Weekly
Exploration: music new to them); Last.fm says which artists are like an artist, from everyone's scrobbles. Lidarr,
where it is connected, fetches what is asked for into Incoming, where it is heard and kept or deleted (0013).

## Decision

**A page of its own, Discover, beside Playlists**: albums the library does not have, each with why it is suggested.

- **Whose:** the signed-in account's, from its own services (0068): ListenBrainz by its user (the one it scrobbles as,
  else the one it imported from), Last.fm likewise.
- **From:**
  - ListenBrainz's recommendations for the user (recordings, scored) and its playlists made for them (Weekly
    Exploration first), named through ListenBrainz's metadata: each recording's album (release group) and artist.
  - Last.fm: artists like the user's most played (`user.getTopArtists`, then `artist.getSimilar`), each new artist's
    best known album (`artist.getTopAlbums`).
- **Only outside the library:** an album the library has (by MusicBrainz release group where both know it, else by
  name and artist, through 0067's matcher) is left out; so is one already in Incoming. An artist the library has may
  still come with an album it lacks; an artist it has not gets "New to you".
- **Why, on each:** "Recommended by ListenBrainz", "In your Weekly Exploration", "Like Bonobo and Air, which you play"
  (Last.fm). Where more than one source names it, all of them, and it ranks higher.
- **Each album:** its cover (Cover Art Archive, by release group), artist, year; links to listen elsewhere
  (MusicBrainz, ListenBrainz, Last.fm); **Want**, with Lidarr connected: Lidarr adds the artist where it has not (its
  other albums unmonitored), monitors that album and searches, so it arrives in Incoming; **Not interested**, kept,
  so it never comes back (and an artist dismissed twice is left out).
- **Fresh:** looked up again once a day for each account that opens Discover, as a task (`app::tasks`, the progress
  convention); kept in between, so the page opens at once. Refresh on the page asks again now.
- **No services connected:** the page says what Discover needs and opens Connections.

## Consequences

- Cratarr asks ListenBrainz, Last.fm, MusicBrainz's cover archive and, with Want, Lidarr; nothing is sent but the
  user names and ids they already have. Their limits are kept (Last.fm's five calls a second, ListenBrainz's
  rate-limit headers).
- ListenBrainz's recommendations are marked experimental in its documentation; when it answers 204 (none made for the
  user yet), the page says so, and the other sources still fill it.
- Not yet: a preview to hear in Cratarr (no source plays what the library does not have), suggestions by track rather
  than album, steering by mood or genre.

## Amendment, 2026-10-24: more sources, for every account (0.9)

The owner, 2026-10-24 ("this all sounds good for 0.9"), after a read of what held the page back: only the account's
own ListenBrainz and Last.fm fed it, so an account without them saw nothing; ListenBrainz makes its recommendations
only for users with enough listens, on its own schedule; Last.fm gave one album (often a compilation) for each of 30
artists new to the library, and nothing for an artist the library has; 60 at most, from sources that change slowly.

- **Gaps in the library's artists' discographies (first):** MusicBrainz's release groups for each artist the library
  has (albums and EPs, official, released), those it has none of suggested ("By Air, whom you have"), ranked by how
  much the account plays that artist (its own listening history, 0064). No service needed; each comes with its
  release group, so Want and the library's match are exact.
- **The account's own listening history as the seed:** its most played artists from Cratarr's plays (0064), not
  Last.fm's; Last.fm's similar artists asked with one key for the whole server (Last.fm's `artist.getSimilar` needs no
  user), so every account gets them, a Last.fm account or not. An account's own Last.fm still counts where it has one.
- **ListenBrainz's fresh releases:** new albums by artists the account listens to (`/user/{user}/fresh_releases`),
  where it has ListenBrainz.
- Kept: ListenBrainz's recommendations and Weekly Exploration, Last.fm's artists like yours, every rule above (only
  outside the library, Not interested, the answers, one suggestion with all its reasons).
- MusicBrainz's one request a second is the pace: the discographies are looked up a few artists a day, most played
  first, and kept, so the page fills over days and opens at once; the links (Bandcamp, YouTube) only for what shows.
- As built (after 0.9, the owner's speed-up: the sources wait until 0.9 is out): 25 artists' discographies a look (one a
  second), never read first, then those read longest ago, each kept 30 days for every account; albums and EPs released
  by today with no secondary type, with MusicBrainz's genres; "Missing from Air's albums", named MusicBrainz. Fresh
  releases: the last 30 days, albums and EPs only ("New from Air"). The server's key: Connections → Discover → "Last.fm
  key for everyone", else `LASTFM_API_KEY`; the account's own Last.fm, where it has one, is asked instead. Every account
  looks now, with services or without.

## Amendment, 2026-10-24: a card shows, its menu acts (0.9)

The owner, 2026-10-24: the links and buttons drawn into each card go into a menu, opened by a tap anywhere on the card;
the card shows as much as is known of the suggestion.

- **A tap on a card opens its menu** (the sheet every item's menu is: the cover, its name and chips on top): Want (or
  Request, 0070), Not interested; to listen elsewhere, each through the Open/Copy sheet: Bandcamp, YouTube,
  ListenBrainz, Last.fm, MusicBrainz; the artist in the library, where it is there; and why it is suggested, every
  reason in full. The card itself has no buttons.
- **Hide** beside Not interested (the owner, 2026-10-24: "a way to hide suggestions without disliking"): the album
  leaves the list and does not come back, and nothing is held against its artist (only Not interested counts towards
  leaving an artist out). Both have Undo in their toast, and Undo has Redo, as every action has (the one undo, 0026:
  Discover's tokens are the signed-in account's own).
- **No "which you play"** after each reason ("Like Bonobo", not "Like Bonobo, which you play"): every card would say
  it (the owner, 2026-10-24); looks kept from before read without it.
- **The card:** the cover, album, artist, year; its type (album, EP, live); its genres, two or three; its length and how
  many tracks; the reason, short ("Like Air and Bonobo", "Missing from Air's albums", "ListenBrainz recommends");
  New to you where the artist is (no label otherwise: "By an artist you have" on every other card said only that New
  to you was not there, the owner, 2026-10-24); how many listen to it (Last.fm's listeners, where known); and, with 0070, the
  request's status.
- **Genres:** MusicBrainz's own genres (its curated list), else the services' tags kept only where they name a genre
  on that list (no "seen live").
- **Where they come from, cheaply:** ListenBrainz's names come with their tags in the same call; Last.fm's album info
  brings tags and listeners (five calls a second); MusicBrainz's discography lookup brings each album's genres with
  it. Length and track count need one MusicBrainz release each (one a second): looked up for the cards in view first,
  then the rest, and kept, so a card fills in as it is seen the first time and opens complete after.

## Amendment, 2026-10-24: shown as soon as found (0.9)

The owner, 2026-10-24: the first look waited behind the page's loader until every album's listening links (Bandcamp,
YouTube: MusicBrainz, one request a second) were found. Now the suggestions show the moment they are known; what is
still looked up (the links, a card's length and track count) goes on behind them, shown as every page shows work in
progress (the line and bar at the top, the page action's ring), each card filling in as its part arrives. The loader
shows only until there is a first suggestion.

## Amendment, 2026-10-25: a page of sections, not a list (0.9.1)

The owner, 2026-10-25: "The discover page can be so much better than a list of cards, no matter how good the cards
are. The cards are fucking fine. What I need is a more dynamic page"; inspired by Home's varied, organic sections
("I don't want you to match its layout precisely"), each section taking the form its content suits. Mock-up accepted
("Yes, I like the gaps, I like the masonry", artifact EfDukRyZTPCM9pyh5iNbSv); the genre cloud declined.

- **Pick of the day:** one suggestion large (cover, facts, why it is here as a quote), Play on Bandcamp and its menu.
- **Discography gaps** (the owner's title; the artists most played first, as the server ranks them): per artist the library has, its albums in time order: those in the library faded,
  the missing ones larger and lit (the discography source).
- **Around the artists you play:** per group of seed artists ("Like A and B"), the seeds in the middle and their
  suggestions in a masonry around them.
- **New to you:** one new artist in the light (picture, a few lines of biography, the album suggested), the other new
  artists as chips.
- **New from artists you have:** the fresh releases, newest first.
- **Your Weekly Exploration:** a compact list, as the playlist it is.
- **Everything else** as the cards (the cards are kept as they are), the pill's search and filter over all of it.
- **Bandcamp first:** within every section what plays on Bandcamp comes first by default, and the pick of the day is
  always one that plays (the owner, 2026-10-25).
