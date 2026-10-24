# 0071: Listening before Want: Bandcamp previews in Cratarr's own player

- Status: accepted (the owner, 2026-10-24: "I do want to include this. Most definitely", after reading that GitHub's
  rules do not forbid it and that Bandcamp's streams carry no copy protection; "Yes, Bandcamp built in for 0.9";
  "Bandcamp indexers, not Bandcamp downloaders. I don't want to download from Bandcamp. I want to play from Bandcamp.")
- Date: 2026-10-24
- Release: 0.9
- Builds on: 0033 (the queue and its tracks), 0064 and 0068 (listening history, one way in), 0069 (Discover), 0070
  (Requests)

## Context

Discover (0069) suggests albums the library does not have, each with links to listen elsewhere: Bandcamp, YouTube, a
search where no page is known. Listening means leaving Cratarr, on another device's browser or app, and coming back to
Want or Not interested. Many of the suggestions are on Bandcamp, whose album pages carry each track's stream (MP3 at
128 kb/s) in a block of data in the page, plain and unprotected: what Bandcamp's own player plays. A browser cannot
read those pages from Cratarr's (Bandcamp does not allow it); the server can.

NewPipe (GPL-3.0-or-later) and yt-dlp (the Unlicense) both read Bandcamp this way, for years and on GitHub. YouTube is
another matter: it changes its scrambling and its checks every few weeks, NewPipeExtractor (the YouTube part included)
is Java, too heavy for the NAS's 768 MB, and a Rust port would need the same fixes every few weeks.

## Decision

**A Discover suggestion on Bandcamp plays as a preview in Cratarr's own player.**

- **Where the album's page is found** (once, while Discover looks, with the rest of the suggestion, one request a
  second, as with MusicBrainz):
  1. the release group's own Bandcamp link on MusicBrainz (its url-rels, which `listen_links` reads already);
  2. else the artist's own Bandcamp page on MusicBrainz (the artist's url-rels): its list of releases
     (`/music`), the album found by its title as the import's matcher compares names (`app::import::key`): the
     artist's own page, so no namesakes;
  3. else Bandcamp's search box, as its own site asks it (`/api/bcsearch_public_api/1/autocomplete_elastic`, JSON),
     taken only where both the artist and the title match;
  4. else none: no Play and no Bandcamp link (the owner, 2026-10-24; most major labels' albums are not on Bandcamp).
  Bandcamp's search page (`/search`) is not read: it answers with a check for browsers (a "Client Challenge"),
  which NewPipe gets past by sending the cookie the check looks for; Cratarr does not get around such checks (the
  owner, 2026-10-24). The album and artist pages and the search box answer as any page does.
  Only pages on `bandcamp.com` are read (an artist's own domain is left to its link).
- **Play on its card** plays the album's tracks in Cratarr's player, on whatever plays now (this device, a player on
  Play On, the app; not the server's own outputs, Snapcast and Kodi, which play the library's files): a queue as any
  other (0033), each track `source: "bandcamp"`, its id the album's page and the
  track's number (so a queue saved, or sent to another player, still plays it), its name, artist, album and length
  read from the page.
- **The server reads and relays:** the album's page (its `data-tralbum` block: the tracks, their lengths, each track's
  `mp3-128` stream), kept a few minutes (the streams' addresses expire, a new read gives new ones); each stream relayed
  under Cratarr's own addresses (`/api/stream/bandcamp/…`, `/api/file/bandcamp/…`, `/api/cover/bandcamp/…`), so every
  player that plays the library's tracks plays these the same way, the app included, with nothing new in it. Nothing
  is kept on disk: played, never downloaded.
- **A preview is not listening:** never a play, never in the history, never scrobbled (`app::listening` leaves out
  every track not in the library, and these by their source before that). The visuals draw what they draw for a track
  without its analysis.
- **Code:** written for Cratarr in Rust after yt-dlp's Bandcamp extractor (`yt_dlp/extractor/bandcamp.py`, the
  Unlicense: public domain), which reads the page the same way; credited in the code.

## Later

- **YouTube**, for what is not on Bandcamp: through yt-dlp, as an optional service of its own beside Cratarr (as
  AudioMuse is), updated apart from Cratarr as YouTube changes; not in 0.9.

## Consequences

- Most independent releases can be heard before Want, without leaving Cratarr or its player.
- Bandcamp's pages can change: the reading is one small module, and a page it cannot read leaves the suggestion as it
  was (its links), with nothing else waiting on it.
- The NAS relays the stream (128 kb/s per listener): nothing to store, little to send.

## Amended 2026-10-25 (0.9.1): a Bandcamp album is an album

The owner: Bandcamp's albums should open in the album panel ("I don't see why not"). A tap on a suggestion that plays
on Bandcamp opens it there (a long-press is its menu); its tracks play through the relay, and what Cratarr keeps of
its own albums (the heart, the rating, Resume, similar albums) it leaves out. A preview's track is one in the player
too: its menu has Discover's own group (Want, Not interested), its Share gives its page on Bandcamp, Info and the
details row say what it is (the details from its album's page; sample rate and channels read on the device from its
MP3 frames), and its waveform is worked out on the device, a few seconds decoded at a time.

Bandcamp is asked politely: one page or search at a time, at most one a second, server-wide; after a "too many" (429)
or "unavailable" (503) none for as long as it asks (a minute where it does not say). A new look keeps the pages the
one before found instead of looking them up again, so what plays stays playable while a sync runs.
