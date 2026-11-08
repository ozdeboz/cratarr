# 0103: Cratarr peers — a friend's Cratarr as a library of your own

- Status: accepted (the owner, 2026-11-04, one decision at a time by question in a5's session); step 1 for 0.18
- Date: 2026-11-04
- Builds on: 0085 (offline first), 0098 (modes per server), 0100 (records and copies), 0101 (stable track ids,
  libraries as equals, own music, the library's index and its change feed), 0070 (requests), docs/plans.md (External
  library, shape A: through Cratarr's server)
- Changes: 2026-10-28's "extra servers only as modes; no combined library across servers, ever" (0098's context), for
  Cratarr peers only
- Leads to: share links and peer invites as iroh tickets; servers reaching each other without a public address

## Context

The owner, 2026-11-04, brainstormed in another session and planned in a5's and 632's: friends who each self-host
Cratarr and use each other's libraries. Their choices:

1. Each user self-hosts their own library and connects with chosen peers; each can use the others' libraries.
2. "I want play on primarily supported at account level and not necessarily transported across peer servers": your
   devices stay players of your own server only.
3. What a peer may do is set per peer.
4. Progressive enhancement: every step useful on its own, nothing shown that cannot be consumed, nothing offered that
   cannot be upheld.

On 2026-10-28 the owner settled "because of our heavy feature set, we will offer extra servers only as modes": another
server's tracks have none of the analysis that crates, Radio, sounds-like and the waveforms run on, so its music stays
in a mode of its own (0098). A Cratarr peer brings that analysis along. For Cratarr peers that settlement is reversed
(the owner, 2026-11-04: "Cratarr peers inside yours"); a Jellyfin or a Navidrome stays a mode.

## Decision

**A friend's Cratarr is one more library of your server.** Your server signs in to it with a peer account, mirrors
its catalogue with its analysis, and relays its streams. To your devices a peer's track is a track of your server:
Play On, queues, groups, Home Assistant, the history, favourites and scrobbles need nothing new. The friend's server
sees one account streaming, never your players, your users or their plays.

- **The peer account.** On the friend's server, a kind of account of its own, made by an administrator in Settings,
  with a key. It reads the library and streams, nothing else: no page sign-in, no devices, no Play On, no users or
  plays visible. What it sees and may do is what that friend allows; deleting it ends the link. A new peer starts with:
  - the main libraries on; Incoming and users' own music off, each switchable per peer. A user's own music is offered
    to peers only when that user allows it;
  - keep offline, ask for albums and share links on, each switchable per peer.
- **The audio is relayed** by your server, with range and seek. The device's tape engine (0102) reads it as any
  stream; the server makes no tape sound of a peer's track.
- **The analysis comes along, always:** tempo and beat grid, loudness, waveforms, AudioMuse's vectors. It is data about
  the music, like the tags, and small next to the audio, so crates, Radio, sounds-like and the waveforms work on a
  friend's tracks from the first sync. Your server never analyses or converts a peer's track itself.
- **Read-only.** Nothing writes a peer's files, and a peer's library is never a Keep, Move or Replace target. Every
  writer (0092's one way to change files, words and covers into files, conversion, the Edit sheet) leaves it out, and
  the page offers none of them on its tracks.
- **Away.** While a peer does not answer, its tracks are dimmed and queues skip them; nothing is retired. A track the
  friend removed is retired as your own are, its tags remembered (0101 point 11).
- **Your data stays yours.** Favourites, ratings, plays and scrobbles of a peer's tracks are kept on your server, by
  your own stable track ids.
- **Reaching each other.** HTTPS to an address: a public one (behind a reverse proxy), or a private network's name
  such as Tailscale's machine sharing, which gives a friend only the Cratarr machine and its port, revocably. That
  needs no code; the documentation says how.

## Steps, each useful on its own

1. **0.18: a peer as a read-only library.** Add a peer (its address and key; a health check by its instance id);
   mirror its catalogue, analysis included, through 0.16's index and change feed; relay its streams; its own dot and
   per-user switches; dimmed while away; plays, favourites and scrobbles kept here. Two copies of an album, yours and a
   friend's, still show apart, each with its dot.
2. **0.19:**
   - playlists and crates holding a friend's tracks;
   - keeping them offline on devices, where the peer allows;
   - a bounded cache of relayed tracks: a size cap the administrator sets, oldest out first, its size in Health, so a
     replay costs the friend nothing;
   - a record at several places shown once (0100's copies): your own copy plays whenever you have it, else the best
     reachable friend's copy, each copy's dot in its menu;
   - asking a friend for an album, approved in their Requests (0070).
3. **After a trial: iroh.** Share links as tickets first, with no public address needed; then peer invites and the
   transport by key, with a relay on the owner's server as home relay and n0's public relays as an optional fallback.
   iroh 1.0 is out; iroh-blobs, for verified streaming with ranges, is not yet.
- Later: bandwidth limits per peer.
- Not: Jellyfin or Navidrome peers. They stay modes (0098).

## How, on the server (a5's audit, 2026-11-04, on 0.16's foundation)

Every lookup goes (source, id) → track → a present `files` row → its folder's path. Where that is baked in deepest, and
what step 1 does about it:

- **A track exists only while it has a present `files` row.** A peer is a catalogue folder of a kind of its own: a
  stand-in root that is never walked, its `files` rows written by the sync from the peer's index (the peer's relative
  path, size and modified time). Lines, crate facts and playlist entries keep working, being path arithmetic.
- **Analysis is keyed by file identity** (folder, path, size, modified time): mirrored rows carry the peer's size and
  modified time, so they join as a file's do.
- **Libraries are walked on every pass:** the walk, the tag reading and `identify` leave peer folders out; the sync
  fills them, and an away peer keeps its rows.
- **The only write guard is the source:** each writer checks the folder instead, the same kind of folder test that
  Keep and Replace got for main libraries (0101 point 2).
- **Track ids:** a peer's `tr-N` collides with ours, so each peer track gets an id of your server's own, kept by (the
  peer's instance id, its track id) across syncs.
- **Covers, their prints and the waveforms** are mirrored into a cache, as remote covers are now; the band waves,
  spectrum and beats are relayed or left out; snippets are left out.
- **OpenSubsonic** names a peer's library by its name, not its path.
- **The peer's export** is 0.16's index and change feed (0101 §16, not built yet), read by the peer account with what a
  server needs beyond a device: the files' paths, sizes and times, the analysis with AudioMuse's vectors, the cover
  prints.

Sizes, roughly: the relay M, covers and waveforms M, the analysis L, the writers M, scanning and Health M, the export
and its feed L beyond 0.16's, track ids and playlists S to M, OpenSubsonic S. Tests: two servers in the e2e suite, one
the other's peer.

## The page and the app

A friend's library is no mode (0098): it sits in Cratarr's own mode beside your libraries, and the mode switcher lists
no peers. The page shows only what can be upheld (0085's rule), so what a peer track offers follows what that friend
allows.

- **One more library.** The Library's list carries it as any other (LibraryInfo `kind` "peer"): its name (the friend's
  server's own, changeable when it is added), its colour, and what the friend allows (keep offline, ask for albums,
  share links). Its dot marks its records, tracks and search results while the viewer has two or more libraries on
  (0101 point 1), and its name stands in a record's About: your server · ● its name.
- **Switches.** Each user switches it on or off for themselves, as any library, in the libraries-off kept on their
  account (0101 point 14), so Subsonic apps, the page and the app agree. A new peer's library starts on for everyone,
  as any library an administrator adds (the owner, 2026-11-04); nothing is seeded off.
- **Away.** While the friend's server does not answer, the list says so (`seen` false, `seen_why` "<name> is not
  answering"), and the page:
  - dims its records and tracks wherever they show, as music out of reach shows away from Cratarr (lib/away);
  - leaves them out of what plays, with a word only when everything chosen was theirs;
  - marks its chip in the Filter sheet "Away";
  - keeps them in playlists, crates and favourites (nothing is retired while away). Health's libraries say since when.
- **What a friend's record or track offers.** Play, queue, Radio and Sounds like from it, favourite and rate, its
  About, its artist and its record. Never the Edit sheet, words or covers into files, fixes, conversion, Keep, Move,
  Copy, Replace or Delete: it is read-only, so it is no target either. Share only where that friend allows share links.
  Step 2 adds:
  - its playlists and crates;
  - Download to this device, only where the friend allows keep offline;
  - "Ask <name> for it", only where they allow asking for albums;
  - for a record at several places, each copy with its dot in the menu, your own first.
- **Adding a friend, administrators only**, with Settings' own groups, rows and sheets (no new patterns):
  - *Your side*, Settings › Libraries › Add a library › A friend's Cratarr: their address and the key they gave. Cratarr
    then checks it and shows what it found before anything is added: their server's name, the libraries they share and
    what they allow. Then its name and colour, as any library's. Its page shows:
    - whether it answers, its last sync, and its records and tracks;
    - what the friend allows, read from them and not set here;
    - Remove, which asks once: its tracks go; your favourites and plays of them stay, by id.
  - *The friend's side*, Settings › Users › Add › A peer account: its name, the libraries it sees, and its switches:
    - main libraries on; Incoming and each user's own music off;
    - keep offline, ask for albums and share links on.
    The key shows once with a Copy, as an API key does (0.13). The account's page shows when it last synced and
    streamed, and Remove, which ends the link.
  - *A user's own music*: their own Settings › Account has "Friends' servers may see my music", off by default. Only
    then can an administrator switch it on for a peer.
- **The app.** The same page, with nothing native of its own.
  - The device's index (0101 point 16) carries a peer's records as it does any library's, switched-off ones hidden.
  - Offline, a friend's tracks are "Not on this device" unless kept there (step 2, where the friend allows), and the
    offline search covers them.
  - Play On, groups and Home Assistant need nothing new, since your server relays the stream. The tape engine (0102)
    plays it as it plays any stream.

What the server gives the page for it: LibraryInfo `kind` "peer" with what the friend allows (keep offline, ask for
albums, share links); away as `seen` false and `seen_why`; and the per-user switch "Friends' servers may see my music"
on the account, which the peer accounts' own-music switches obey.

## Consequences

- The runway: 0.17 keeps the seven Kima picks and audiobooks' first part (the owner, 2026-11-04); peers' step 1 is
  0.18, on top of 0.16's index and feed.
- docs/plans.md: the External library section points here for Cratarr peers.
- A friend's administrator sees that your server streams a track, as one account, and nothing else.
