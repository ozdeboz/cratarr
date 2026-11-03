# 0094: Taste in sequences and lists

- Status: accepted (the owner, 2026-11-02, by question sets after a5 listed where the taste is used: "we need to deep
  dive now, … how the user taste profile affects sequences and lists"). For 0.14. The server's parts are a5's, the
  page's 632's.
- Date: 2026-11-02
- Builds on: 0036 (radio), 0024 (crates), 0060 (one set of sorts and filters); amends 0036 and 0060

## Context

- **A listener's taste** (`app::taste`) is read from their favourites, the playlists holding a track, their plays,
  their skips and what they excluded: a "taste" profile and an "avoid" profile over the one similarity.
- **It shaped almost nothing.** Grave digging and an artist's top tracks were ordered by it; Insights showed it. Radio
  and Autoplay went by likeness to what they started from, with a fixed nudge for favourites; skips taught them
  nothing; the avoid profile was worked out and used nowhere.
- **Its switches did not hold.** Settings › Profile's sources were kept per browser and changed only what Insights
  showed; Grave digging and top tracks read every source whatever they said.
- **A crate's radio was two buttons** with nearly one name: Start radio (like the crate, from everywhere, its own
  tracks left out) and Play as radio (only its tracks, endlessly); and a crate's "own order" was named "Crate order" or
  "Newest first" whatever it was.

## Decision

1. **One taste per account.** Settings › Profile's source switches are the account's, kept on the server, and every
   use of the taste reads them: Insights, Grave digging, an artist's top tracks, Best match, the radio's Your taste and
   skips keeping tracks away. A source switched off adds nothing anywhere.
2. **Taste is chosen, never mixed in unseen.** What follows taste says so: a sort named Best match, a radio mode named
   Your taste. Other orders and modes do not lean on it.
3. **Best match on every list** of tracks or records: crates, playlists, an artist's, the Library. Its order is the
   server's (fit to the account's taste); the default only on Grave digging and an artist's top tracks.
4. **Skips keep tracks away, gently,** in Radio and Autoplay, while skips are switched on: a skipped track and those
   very like it (the avoid profile) come later and less often, never left out for good. Excluding stays the only hard no.
5. **One radio sheet, wherever radio starts** (crates, playlists, records, artists, a selection), built from existing
   pieces (amends 0036):
   - **Where:** only this list, endlessly (where there is one to stay inside), or beyond it: like it, from the
     libraries switched on for Radio, Bandcamp where allowed.
   - **How it moves on:** Similar, Same artists, Same era, Random, Your taste.
   - **Whole records** on or off.
   - It opens on the listener's last choices. The crate's Play as radio button goes; its wording is open.
   - The server's `within` takes the page's collection (a crate, playlist, record, artist or tracks; none: beyond).
6. **A list's own order is named by what it is** (amends 0060). An own crate has none: it was newest added first, the
   same as Added, which becomes its default. A Sounds like crate: Closest first. New for you: As on Home. Grave
   digging: Best match. "Crate order" goes.

## Consequences

- What a listener tells Cratarr (favourite, skip, exclude, a source switched off) changes what plays in the places
  that say they follow it, on every device.
- Radio's fixed nudge for favourites stays as part of Similar; Your taste is where favourites weigh fully.
- The Profile switches leave the browser's preferences; a browser's old switches are not carried over (no backwards
  compatibility): each account starts with every source on.

## Amendment, 2026-11-02: every crate opens on Best match

The owner, asked which lists open on Best match: "Every crate." This covers one's own crates and the built-in ones:
New for you, Sounds like, the genres and Grave digging. It amends #3 and #6. An own crate's default is Best match, not
Added. New for you's As on Home and Sounds like's Closest first stay as choices, but neither is the default. The
header names the order. Playlists keep their own order, and the Library keeps the one chosen there; Best match is
still offered on every list.

On the server, a crate's rows, its preview and a play of it are ordered by Best match when no order is asked for
(`CrateOrder::opened`). Callers that ask no one's taste keep the crate as saved, newest first: Home's shelf, a
download, Home Assistant and voice. Without an account, Best match is the crate's own order anyway.
