# 0009: Uses are set once, on the server's libraries; the folders behind them are found

- Status: accepted
- Date: 2026-10-12
- Supersedes: the "music libraries inside the playlist folder" default of decision 0005, and the extra folders of
  decision 0008

## Context

In 0.3.x the use of music was asked twice in Settings. Each server's libraries had roles (Music, an optional
Incoming), and the catalogue had its own list of "other music folders". On the reference install both said
"incoming" about the same music, which was confusing. The page also never said when Cratarr could not see a chosen
library's files: the incoming library was not mounted into Cratarr at all, which went unnoticed because the Incoming
list only asks Jellyfin.

A version that set uses on mounted folders instead was built and looked at on 2026-10-12, and rejected from the
user's side. It started from container paths the user never chose (`/incoming`). A library that was not mounted
silently vanished from the page. The Incoming list, which never needed the files, disappeared until a mount was
added. The *arr stack points the other way: the user picks from what they already know (Seerr picks Sonarr's root
folders from Sonarr's list), mounts are the job of whoever runs the containers, and an app that cannot reach a
folder says so plainly (the *arrs' health checks) instead of fixing it.

## Decision

- Settings → Libraries lists each server's music libraries by name. Each is **Music**, **Incoming** or **Not used**.
  This is the only place a use is set.
- Behind each library, Cratarr finds the folder that holds its files, as before (decision 0005), now trying every
  folder mounted into the container (read from `/proc/self/mountinfo`, `mounts.rs`). Each library shows that
  folder. A Music or Incoming library whose files Cratarr cannot see says so. The message names the folder the
  server reports for it and what to do: mount it into Cratarr and restart. Cratarr never mounts anything itself:
  that would need the Docker socket.
- Music: the folder its files are in holds the playlists. Saving is refused when Cratarr cannot see a Music
  library's files, or when Music libraries lie in different folders.
  - The folder is saved too (`music_folder`), so playlists work while the server is down.
  - `CRATARR_PLAYLIST_DIR` is only the first-run default. Until something is saved, the libraries found in it count
    as Music.
- Incoming: its list comes from the server alone, so it works with or without a mount. Features that need the files
  use the folder when Cratarr can see it.
- The catalogue covers the Music folder (id 0) and the Incoming folders it can see. There is no list of extra
  folders any more. Its status stays a separate block under the libraries, with tag splitting.
- `settings.json` becomes version 3: `incoming` holds a list of library ids (version 2's single id is read as a list
  of one) and `music_folder` is added. A version 2 `folders` list is ignored.
- With no Music library, Playlists and Repairs say so and point to Settings. Everything else works.

## Consequences

- Nothing on the page is a container path the user has to understand, except in the warning that asks for a mount.
- The folder in that warning is the server's own path. On the reference install that is the host path
  (`/Volume1/media/lidarr-music`), but a server in its own container may report its container path. So the page
  calls it the server's folder, not the path to mount.
- A mount may be one library's folder or a parent of several, as the *arr stack mounts one `/data` for everything.
  The library's own folder inside it comes from the server: Jellyfin's top items of a library (albums or artist
  folders) carry their paths, and their parent is the library's folder. One broad mount at install means a new
  library never needs a compose edit. Decision 0010 makes that mount writable: Incoming is written too now.
- The catalogue keeps a folder's rows when the folder's path changes but the same files are there (checked on a
  sample), so rearranging mounts does not cost a refill.
- Placement asks each server what it did before: its library list and three sample paths per library. Each sample
  is then checked against every mounted folder.
