# 0012: Playlists become the user's own Jellyfin playlists, kept in step with the .m3u files both ways

- Status: accepted (2026-10-13); its one switch for all playlists ("Playlists belong to") and the move out of
  Music are replaced by per-playlist sync links (decision 0014). The sync engine stays.
- Date: 2026-10-12
- Builds on: decision 0011 (track identity, events). Changes decision 0002.

## Context

Cratarr is used by one person. Its playlists are `.m3u` files in the Music folder, which Jellyfin imports as
playlists visible to every account with access to the library (six on the reference install). Jellyfin's own
ownership and sharing do not apply to imported `.m3u` playlists. The owner wants the playlists to be theirs.

Checked on the reference install (Jellyfin 10.11, read-only):

- A playlist can be created for one user with `IsPublic: false` (private), and shared later with chosen users.
- Its whole track list can be replaced in one request, with the ids in the body (no query-string limit: the
  5,041-track playlist fits).
- Jellyfin skips folders whose name starts with a dot: the `.m3u` backups in `Music/.cratarr-backups` never became
  playlists (0 of 67).

## Decision

- Settings asks, per server, whose playlists these are: a Jellyfin user, chosen from the server's users.
- The playlist module (decision 0011) holds each playlist as an ordered list of track ids. It keeps two copies in
  step:
  - the `.m3u` file, moved into a dot-folder inside Music so Jellyfin no longer imports it. It stays portable and is
    what export and backups use;
  - the chosen user's private Jellyfin playlist.
- In step both ways. A change made in Cratarr is written to both copies. A change made in Jellyfin (in Finamp, say) is
  read back on the background pass, by comparing playlists' change stamps, and merged into the `.m3u`. The merge is
  three-way, against the last state both copies agreed on:
  - added on either side: added;
  - removed on either side: removed;
  - reordered on one side: that order;
  - the same spot changed on both sides: the newer change wins.

  A missing track (a line whose file is gone) has no Jellyfin id. It stays in the `.m3u` and is never treated as
  removed.
- Incoming tracks may be in playlists. Adopting the album keeps them (same track ids, decision 0011). Rejecting it
  takes them out of both copies, and Undo puts them back.
  The journal's "playlists" handler (`app/follow.rs`) does this:
  - Keep and Replace: each `.m3u` line naming a moved file is renamed to the file's new place.
  - Delete: the album's lines are taken out, after recording where each one stood. Undo puts them back there.
  - The Jellyfin copies follow through the sync. The sync skips its merge while an album action is under way, so a
    line naming a file that just moved is never mistaken for a removed track.
- Only playlists Cratarr tracks are synced. Jellyfin playlists made by others (the weekly ListenBrainz ones) are
  left alone.
- Sync state (which `.m3u` goes with which Jellyfin playlist, and the last agreed list) lives in the data folder,
  next to the catalogue and the journal.

## The owner's choices (2026-10-13)

- The playlists belong to the `jellyfin` account (the one the tablet plays as).
- The `.m3u` files stay the hub, in `Music/.playlists`: the alternative (Jellyfin as the only copy, files as a
  write-only backup) would tie every playlist to one server, and Cratarr is meant to be able to bridge servers.
- Home Assistant's references (the voice playlist list, the Chill id in welcome-home) are updated as part of the
  migration.
- The data folder goes into the NAS backups.

## Migration: part of the program

There is no manual step: choosing an owner in Settings ("Playlists belong to") is the switch, and deploying a
version is all a migration takes. The next background pass:

1. moves the `.m3u` files into `Music/.playlists`, each line given `../` so it still points at the same file. A file
   is written there in full before the one in Music goes; a pass cut short is finished by the next one; a file found
   in both places with different content stops the move and is reported, nothing decided;
2. tells Jellyfin the imported playlists are gone;
3. makes a private Jellyfin playlist per file for the owner (or links one of the same name that is already there).

Until an owner is set, nothing moves and the files stay shared `.m3u` playlists, as before.

## Safe to repeat and to interrupt

Links are matched by the server copy's id and remember both agreed names (here and on the server), so a rename made
on either side, even one cut short, is never taken for a deletion. A rename made in Cratarr is recorded before the
file is renamed. A listing on either side that suddenly lacks most linked playlists is not believed. A server copy
made but not yet remembered is found again by its name, not made twice. A file edited while a pass merged is left
for the next pass. Deleting what is already gone counts as done. Membership edits from the page say the state
wanted (in or out), so a retried request changes nothing more.

## Consequences

- Every playlist gets a new Jellyfin id. Whatever refers to the old ids needs them again: Home Assistant's voice
  playlist list (68 entries), the Chill playlist id in the welcome-home automation, and playlists downloaded in
  Finamp for offline use.
- Other Jellyfin accounts no longer see these playlists, unless they are shared with them.
- The data folder now holds state that cannot be rebuilt (catalogue track ids, the journal, the sync state), so it
  must be backed up.
