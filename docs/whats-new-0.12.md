# What's new in 0.12

Since 0.11.0; 0.11.1's fixes are in it too. The notes for each part are in [decisions](decisions).

## Crates from a few words

- **Grab a crate.** At the top of Crates a line waits for words; until you type, Crates is its usual page. Type
  freely, "hip hop 90s Miles Davis": years and decades are taken out first, then the longest names that match
  something, each a term. A comma still splits.
- **It reads as a sentence.** "Hip Hop *from the* 1990s *that sounds like* Miles Davis": the terms underlined, the
  words between them saying how each counts, and typing goes on after "and…". A tap on a term shows its other
  readings, a field for another word, Sounds like or By, and Remove. Several terms of one kind join with "or";
  different kinds all apply at once.
- **How alike,** for an artist to sound like: the closeness curve under the sentence, counted among the tracks the
  other words let through, so its numbers are the crate's.
- **From anywhere.** In global search, words that read as two or more kinds show the crate they make first: four of
  its covers, its sentence, its count, **Grab this crate** and Play. **Sounds like this** in a track's, an album's
  or an artist's menu, and **Grab a crate** in a genre's or a label's, open Crates with the crate started.

## Snippets

- **A track's menu plays its liveliest 20 seconds** a moment after it opens, its waveform under the menu's head with
  that stretch lit and a playhead running; **Play from 1:52** plays the track from there. Closing the menu stops it;
  a quick ⋮ and away plays nothing.
- **Your music ducks** to a fifth of its volume meanwhile and comes back after, when it plays on this device; a
  snippet never touches the queue or a Play On player. In the Android app the app plays it.
- **In the crate search** each track has a round button: it opens the row on the same waveform and plays its
  snippet; again, it stops. One at a time.
- **Settings → This device → Snippets in menus** turns the menus' snippets off.
- A snippet reads only what the server has already worked out (a track's waveform and beats): without them it starts
  at a fixed point, and nothing is analysed for a menu.

## Search

- **One matcher for the server and the page,** so both find the same: separators don't count ("hiphop" is Hip Hop),
  a typo still finds the name, and the best match leads (exact, then the start of a name, then close). The page's
  copy loads in the background after it opens and is kept for offline.
- **Results have menus**: a long-press, right-click or ⋮ on a track, album, artist, genre, label, crate or playlist
  in global search opens its own menu.

## The waveform

- **The same number of bars across it, always**, while you hold Previous or Next, through the zoom and across tracks.
  At a steady zoom the bars only slide; through the zoom they widen or narrow from the middle. No flashes, no
  sideways jumps, and each track keeps its own colour during and after a hold.

## Classical

- **A composer's works come first,** most recorded first, then their records. A work opens its recordings, each
  playable as its movements in order.
- **On an album, a work's movements sit under its heading**, named by movement only ("I. Allegro con brio"), with
  Play and a menu for the work. Movements, recordings and headings have their menus (Play next, Add to queue, the
  snippet).
- **Search finds works**, by title, catalogue number or composer ("goldberg bach", "BWV 988", "op 67"): a Works
  group with each work's composers and how many recordings you have; a tap opens it.

## Lists

- **One table header everywhere**: a tap on a column's name orders the list by it, a second turns it round, and an
  arrow shows which. A crate seen as records is a table too.

## Offline

- **Downloads works offline**: what's on the device keeps its details and small covers, so it shows, searches and
  orders with no network. Downloads from before 0.12 fill in by themselves while Cratarr answers.
- **Nothing done offline is lost.** Favourites, ratings, playlist changes, exclusions and plays are kept on the device
  first and sent when Cratarr answers again, each once ("N to send" on the connection line meanwhile). The last
  change wins.
- **Changes from your other devices arrive** within a minute while the page is open.
- **Your own playlists can be edited offline**: what you take out or move is sent as it was done.
- **Menus offer only what works away**, with one line, "The rest needs Cratarr."; the crate search and New playlist
  say so in one line too.

## Players and the app

- **One sign-in for the app's two addresses**: signed in at home or over the internet, the app is signed in on both.
- **A browser's and the desktop app's queues are kept by Cratarr**, so the Queues you see are the same on the device
  and from another device driving it.
- **The media server's player is gone from Cratarr's own list of players.** It returns as its server's own mode
  (0.13).

## Fixed in 0.11.1

- The player's menu no longer blinks its Want, Not interested and Other versions.
- Driving another player, the device shows where it really is through a wind; a late wind to a point still lands
  there.
- Discover no longer fills the server's memory when asked many times at once.

## Not yet

- **MusicBrainz's genre list.** Genre aliases are a hand-kept list for now (DnB, R&B, EDM and the like): search's
  genres and a crate's words know them, so "dnb" finds Drum and Bass. Reading MusicBrainz's whole genre list waits for
  later, and a page's own filter (the pill on a genre list) doesn't use the aliases yet.
