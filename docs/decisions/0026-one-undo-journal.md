# 0026: One undo journal

- Status: accepted
- Date: 2026-10-14
- Builds on: 0011 §6 (the journal), 0013 (Keep and Replace), 0024 (crates), 0025 (the panel)
- Replaces: the playlists' "back to before the last change" undo (`/api/undo`), Incoming's own `/api/incoming/undo`

## Context

Undo exists in three places, each its own way: a deleted playlist and the Repair tab's fixes go "back to before the
playlist's last change" (wrong when anything changed it in between: another device, a two-way sync, the next action);
Incoming's Delete, Keep and Replace go through the journal (0011 §6), which records an event before acting, keeps each
handler's result, marks events undone and survives restarts; crates have only one backup of `crates.json`, unused.
Creating, renaming, Spoken as, edit mode's Save and everything about crates cannot be undone at all, and every page
builds its own undo toast. The owner wants one undo, robust, the same everywhere, decided on 2026-10-14.

## Decision

- **The journal becomes the app's.** Its events grow from Incoming's three to every change a person makes to playlists
  and crates. One action is one event, however many playlists it touches (Fix 12 tracks in 4 is one; a Delete of 3
  playlists is three requests, whose three tokens the page undoes together). Incoming's events, its handlers
  (`app/follow.rs`) and their per-event idempotence stay as they are: playlist and crate changes are kept in a table of
  their own in the same file (`changes`), so the album handlers never see them, and their tokens say which table they
  point into (`c<id>` for a change, Incoming's event id as it is).
- **Every change returns an undo token** (its event's id): New playlist, New playlist from the queue, Make playlist
  from a crate, Rename, Spoken as, edit mode's Save, Delete (one or several), the Repair tab's fixes, a crate's Save
  (new or changed), a crate's Delete, and Incoming's Delete, Keep and Replace (also once converted). Not journaled:
  what one tap undoes by tapping again (a track into or out of a playlist, a favourite, a pin, the crate favourites
  toggle), sync links (linking again restores the copy), playback, Settings.
- **One way to undo:** `POST /api/undo {tokens}` (an action's tokens, undone newest first); the server knows what each
  kind of event means. It answers with what
  it did and a **redo token**: the undo is an event of its own, and undoing that event performs the action afresh, as
  a new event with its own handlers and its own token (never the undone event run again). A redo of Keep or Replace
  starts from the album's folder (the catalogue folder and directory in the event) and looks up the album there now,
  since the server may have given it a new id once its files were back in Incoming; if it is not there, "It changed
  since".
- **An event holds what undoing it needs and the state it left:**
  - a playlist's change: the file it replaced, in the event itself, and a fingerprint of the file it left (FNV-1a, 64
    bits). Undo puts that file back, not "the newest backup", through the store's normal commit: the servers are told
    of it as of any change, and the undo's own event holds the file it replaced, for the redo. (In the event rather
    than a reference to the store's backups: a playlist edited 20 times since would have rotated its backup out, and
    playlist files are small);
  - a created playlist: its name and content (undo sets it aside as a deleted playlist, so redo can restore it);
  - a deleted playlist: its name (the store keeps it aside; undo restores it);
  - a rename: both names; Spoken as: the names before and after;
  - a crate: its definition before and after (none before for a new crate, none after for a deleted one), in the event
    itself: crates are small, so they need no versions of their own.
- **Undo is safe:** it first checks that what the action left is still there, and otherwise refuses with "It changed
  since". A playlist's content equals what the action left; a renamed playlist still has the new name and the old one
  is free; a restored name is free; a crate's definition is unchanged; Incoming as now (the files still where the
  action put them and the old place free; a Delete's trash entry still there; converted files unchanged since). An
  undo that stopped halfway (a crash) is safe to repeat.
- **How long:** undo is offered only in the toast right after an action (and Redo right after an Undo); there is no
  history of changes to undo from later. A token works while its event is in the journal and what it needs still
  exists (a deleted playlist's copy kept aside, the trash not yet emptied). Incoming's events are forgotten when
  its trash is emptied, as now, so never later than the trash keeps their files; playlist and crate events, which
  only the toast offers, are forgotten after 24 hours (the trash empties only when Incoming is used, so it cannot be
  their bound).
- **The page:** one helper shows an action's toast with Undo and, after an Undo, "Undone." with Redo; every action
  goes through it. The per-feature undo code goes.

## Consequences

- The journal and its pruning move from Incoming to the app; Incoming's code calls the shared journal as before.
- The playlist store's changes report the file they replaced and the one they left, and it can put a file back (or
  delete one) only while the file is what a change left. Its backups stay as protection, not as undo's source.
- `crates.json`'s single backup stays as protection against a broken write; it is not part of undo.
- Two devices: an undo token from one works on the other (the journal is on the server), and the safety check stops it
  where the other device changed the same thing since.
