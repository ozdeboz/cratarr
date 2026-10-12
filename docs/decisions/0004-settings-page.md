# 0004: Connection settings can be changed from the page

- Status: accepted; the file format and the per-key overrides are superseded by
  [0005](0005-pluggable-sources.md) (the secret rules stand, generalised there)
- Date: 2026-10-12

## Context

All configuration came from environment variables, so changing the Jellyfin address, key, followed user, preferred
player or library folders meant editing the container. The page should show what Cratarr runs with and let these be
changed. The page has no login (LAN only, as for playlist edits), and the API key gives admin rights on Jellyfin.

## Decision

- `CRATARR_DATA_DIR` names a folder for `settings.json`. Without it the Settings page shows everything but cannot
  save. The file holds the same keys as the environment variables and wins over them, so validation and defaults are
  shared (`Config::from_lookup`). Only `JELLYFIN_URL`, `JELLYFIN_API_KEY`, `JELLYFIN_USER_ID`,
  `CRATARR_PLAYER_CLIENT`, `CRATARR_MUSIC_PREFIXES` and `CRATARR_JELLYFIN_PLAYLIST_DIR` are read from or written to
  it. The bind address and playlist folder are tied to the container's port and mount, so they stay in the
  environment; other keys in the file are ignored with a warning.
- A value saved equal to the environment's is left out of the file, so the file holds only real overrides.
- A save is checked first: the config must validate, Jellyfin must answer, and the key must be able to read the
  chosen user. Only then is the file written (temporary file and rename, mode 0600) and the new connection swapped in
  while running. Saves are serialised.
- The API key is never sent to the page (only whether one is set). An empty key field keeps the saved key, but
  only for the saved address: a new address needs the key typed again. Otherwise anyone who can open the page could
  point Cratarr at their own server and receive the stored key. The same rule applies to "Test connection".
- Display preferences (theme, playlist order and size, progress bar) are per browser, in `localStorage`.

## Consequences

- Anyone on the LAN can change the connection settings, as they can edit playlists today. They cannot read the key.
- The container needs a writable data folder for saving; the file contains the key and must not be committed or
  shared. Losing it falls back to the environment.
- A broken saved setting that still passed the check (say, wrong library folders) is fixed from the page, or by
  deleting `settings.json`.
