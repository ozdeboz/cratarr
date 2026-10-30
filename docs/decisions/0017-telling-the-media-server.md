# 0017: Telling the media server about moved albums, and noticing when it stops listening

- Status: accepted
- Date: 2026-10-13
- Builds on: 0011 (the media-server port), 0013 (Keep and Replace), 0016 (Music's layout)

## Context

Keep, Replace and Delete move or remove album folders. Jellyfin learns of that from its own watcher (real-time
monitoring, on for the reference install's Incoming library, off for Music), from a notice
(`POST /Library/Media/Updated`), or at its scheduled scan. What was seen on the first real Keep (2026-10-13):

- **Twelve "file created" notices** for the files in the new Music folder led to nothing; the Incoming side, which
  Jellyfin watches, was read again about 60 s after the move (its watcher's delay).
- **A notice for the new folder** made Jellyfin read Music again, but a read of the library root found nothing new
  then, because Jellyfin had stopped processing changes (below). After a restart, asking the library to look
  (`POST /Items/{library}/Refresh`) found the new folder in 10 s. There is no narrower request: Jellyfin reads the
  folder that holds the new one again, all the way down (decision 0016 on what that costs).
- **The album was playing when it was kept.** About a minute later Jellyfin's Incoming watcher removed the old items;
  the player's progress reports then named an item that was gone, and Jellyfin's database refused them ("FOREIGN KEY
  constraint failed" on `/Sessions/Playing/Progress`). From then on no library change was saved: a scan stayed at 0%
  for 25 minutes, and even cancelling it hung, until Jellyfin was restarted. Reads kept working. Why the library work
  hung is not known; the refused reports are the only anomaly in the log.

## Decision

- **Notices go per album folder,** not per file: the folder a move left (gone) and the folder it made (created), each
  once.
- **The library that gets a new folder is asked to look at once** (`Jellyfin::refresh_library`), instead of waiting for
  the server's watcher or scan.
- **A playing album is stopped before its files move** (Keep, Replace and Delete): a stopped player reports nothing,
  so no report can name an item that is gone. After Keep and Replace the same track plays again on its new item, from
  where it stopped, with the rest of the album after it; only then is the server told the old folders are gone
  (`app/incoming/carry_on.rs`).
- **Cratarr notices a server that stopped processing changes,** without adding load to it:
  - its library scan at the same progress for 10 minutes (one request to `/ScheduledTasks` a minute);
  - moved files that do not show up within 3 minutes (the carry-on gives up; the player stays stopped).

  The page says so once, in a toast, and the connection line keeps saying it: "Jellyfin has stopped processing
  changes. Restarting it fixes this." It clears once a scan finishes after it was noticed, or after 30 minutes.
- **Cratarr never restarts the server.** That belongs to whoever runs it (on the reference install, its monitoring).

## Consequences

- A kept album shows in the media server within seconds, at the price of one read of the library that holds it. In
  a flat library that is a full read (decision 0016).
- Playing never continues on an item that is about to vanish, so the one known trigger is gone from Cratarr's own
  actions. Others remain outside Cratarr (a file replaced while it plays, by Lidarr say); the detection covers them.
- The detection is new and has not yet seen a stuck server.
