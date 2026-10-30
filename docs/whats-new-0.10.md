# What's new in 0.10

Since 0.9.18. The notes for each part are in [decisions](decisions).

## Listening together

- **Follow another listener.** Anyone who switches on "Let others follow me" (Play On's sheet, at its foot) can be
  followed: your player plays what theirs plays, in time, wherever you are. Plays heard while following count in your
  history, not in your taste.
- **Ask for a track.** Following someone, Play next and Add to queue become a request. It shows in their queue where
  it would go, "Asked by" you, with a thumbs up and a thumbs down; they can mark people whose requests go straight in.
  You hear back either way ([0079](decisions/0079-follow-and-queue-requests.md)).
- **A guest page** at `/guest`, on your home network only and off until you switch it on (Settings → Authentication):
  a guest gives a name, sees what plays, searches the library and asks for a track, which you accept as any request.
- **The screen follows the music.** When another device takes the music over, the one it left, and every screen that
  showed it, show the player it went to. Followers, requests, shared visuals and the sleep timer go along.
- **Shared devices at home.** Another user can play on, follow or control a device you share only from your home
  network. You use your own devices from anywhere. Settings → Authentication → Home networks names any address that
  counts as home besides the local ones.

## The library

- **Classical** is a kind of record beside Albums, EPs, Singles and Compilations: it opens on its composers, and shows
  each record's performers. Each record shows in its own kind only, unless the filter says otherwise. An
  administrator can mark a record Classical or not ([0082](decisions/0082-classical-and-other-versions.md)).
- **Other versions** in a track's menu: the library's other recordings of the same piece.
- **Length** as a column and an order of the Library.
- **Excluding an artist:** "Exclude everywhere" (out of every crate, Radio, Autoplay, Discover and Home) or "Not in
  this crate", from the artist's menu; listed in Settings → Listening ([0083](decisions/0083-excluding-an-artist.md)).
  The Artist filter now counts every credit ("Air feat. Beck" is Beck's too).
- **Crates as Records or Tracks**, in a crate's View sheet.
- **New for you** shows at most three albums of an artist, compilations aside.
- **Requested** marks go once an album is in the library (the Requests page keeps them).

## Menus and downloads

- **One Download…** on everything with tracks, crates and queues included. In the app: to this device, or as files.
  Formats: the original files, FLAC at CD quality, Opus, and now MP3 at 320 kb/s or V0. One file comes as itself,
  several as one zip. Bandcamp previews are never downloaded.
- **Bandcamp previews in the queue**, on any player: Play next and Add to queue.
- **Every menu** follows one table: the same entries in the same groups and order everywhere.

## Getting a missing track

- From Soulseek or **from a file on the server** (its download folder), in an album's Missing tracks.
- Each copy is checked before it is added: whole and undamaged, the right length, the right recording (AcoustID), and
  now **truly lossless** (no lossy cutoff in a "FLAC") and **free of clicks, pops and clipping**. A copy that fails is
  refused with the reason ([0080](decisions/0080-single-track-intake.md)).

## Around the app

- **The sidebar** keeps one section open at a time (Library's kinds or Discover's modes), and the current page in sight.
- **Search** keeps its results when you go back from one, and Show all has a back of its own.
- **The visuals** can open by themselves after the screen is left alone while music plays (a long press on the
  visuals button, or Settings → This device), from a snapshot of your choice; a knob that steps (a Lens's kind, a
  Mix's mode) now glides from one to the next.
- **Home greets you by your directory's display name.**
- **Waking up:** a computer (the desktop app) or a browser that slept reconnects at once.

## Under the hood

The player, the tape, navigation and each device's settings were rebuilt so that each has one place in the code, and
the server, the page and the Android app now share their message types and their rules from one source. Little of it
should show, but it touches playing everywhere: if something plays, winds or stops differently from 0.9.18, please
say so.

## Updating

- Versions are plain numbers from now on (0.10.0, 0.10.1); a hotfix is a patch version.
- Settings → Appearance's "Capital first letter" for the greeting is gone (the display name replaces it).
- Export… and Download once are one entry now, Download….
