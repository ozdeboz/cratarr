# 0044: Folders, chosen and shown

- Status: accepted (the owner, 2026-10-17: "Approve.")
- Date: 2026-10-17
- Builds on: 0009 (libraries and their uses), 0032 (Cratarr is its own source), 0034 (each library's own settings)

## Context

Cratarr's own source finds its libraries itself: the music folders under what is mounted into its container (0032).
Settings → Libraries can only give each one a use (Music, Incoming, not used) and its settings (0034). A folder it does
not find (empty, nested deeper, no audio yet) cannot be added; one it finds cannot be removed. Paths show as text,
without browsing, free space, track count or whether Cratarr can write there until something fails. What writes into
Music (the playlists' folder, the trash, file naming) sits in different places; other servers' paths (Jellyfin's) are
mapped to Cratarr's mounts silently, with only a warning when that fails. The owner: "less than nominal path
management", asked how comparable apps do it.

Jellyfin's and Plex's libraries, the *arr apps' Root Folders (Lidarr, Sonarr, Radarr) and Audiobookshelf all have a
folder added explicitly, picked in a browser of what the server can see, shown with its path, free space and problems;
nothing is added unasked. The *arr apps add a remote path mapping where another app sees the same files elsewhere.

## Decision

**One Folders list** (Settings → Libraries), the Root Folders model:

1. **Folders are chosen.** Each is a row: its name, its path as Cratarr sees it, its use (Music, Incoming, Off), and a
   status line: tracks, free space, writable or not. A folder that is gone or cannot be read says so in one line.
2. **Add folder** opens a browser of what Cratarr can see (its mounts only, nothing above them), each folder with how
   much audio is in it and whether Cratarr can write there. The music folders Cratarr finds by itself are offered as
   suggestions to add, never added silently.
3. **Remove** stops using a folder; its files stay as they are.
4. **What writes into a Music folder is under it:** the playlists' folder, the trash and how long it keeps things,
   file naming.
5. **Other servers** get a path mapping row only where their paths do not map to Cratarr's mounts by themselves.
6. **Mounting** a disk or share into the container stays outside Cratarr (a Docker change); Cratarr keeps its one-line
   hint for a folder it cannot see.
7. **Nothing changes on the day:** the libraries found today become chosen folders once, by themselves.

Errors are one short line; no other prose.

## Consequences

- A new folder (an empty Incoming, a second disk) is added before music is in it, and nothing appears unasked.
- Problems (read-only, missing, a full disk) show before an action fails.
- The libraries' ids stay their paths, so uses, policies and everything kept by library carry over.

## The work

- Cratarr's own source listing the chosen folders and offering the found ones as suggestions (`source/cratarr.rs`'s
  library methods).
- The browser, the folders' status (free space, writable, tracks), the settings and their one-time carry-over, the
  path mappings, and the page.
