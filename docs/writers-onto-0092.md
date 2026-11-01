# The writers onto one path (0.14)

How each place that changes files today moves onto [0092](decisions/0092-one-way-to-change-files.md)'s path: a plan,
the review sheet where Cratarr proposes, the yes, one run in Changes to files with its Undo, and the record's state
line. Written 2026-11-01 from a read of the page (trunk at 0.14's start); design only, for the owner and a5 before
code. The owner's choices are at the end.

0092's rule, applied below: **what Cratarr proposes is reviewed** on the sheet first; **what you set yourself on what
you see** (halve a tempo, choose a cover, Keep, Replace, move, delete) is the yes already. Either way the change is
one run in Changes to files, with Undo there and on the record's line.

## Each writer

| Writer (today) | Proposes or chosen | 0.14: before | 0.14: after | Goes |
|---|---|---|---|---|
| **Health's fixes** per album: year, genre, album artist, MusicBrainz release, rule names (HealthFixes; POST /api/health/fixes/apply) | proposes | The review sheet, titled by the finding ("Missing years"): each record's fields, now struck → proposed, with the source and "likely" marks; the genre picked in the sheet. Button "Fix n" | A run "Missing years · n records"; the record's line | The inline list in Health's panel; the withUndo toast; Health's own re-read after (the run's news does it) |
| **Apply all certain** (ConfirmSheet) | proposes | The same sheet, every certain fix ticked | One run | The ConfirmSheet |
| **The length fixes** (reread, length, trim; the same fixes path) | proposes | The same sheet; the field reads "Length 3:12 → 3:05" with "the file's header" as what changes | One run; the header's old bytes kept beside the journal (0092) | Their own before-files |
| **Intake check before Keep** (IntakeCheck in Incoming) | proposes | The sheet for that one record, "Before Keep" | One run, before the Keep's move | The inline Accept list |
| **Renumber** (HealthRenumber; look task, then POST …/renumber) | proposes | The sheet "Renumber · the 1998 CD release": each track's disc and number now → after; while the release is looked up, the sheet says so with its meter (a plan being made) | One run | The withUndo toast |
| **The tempo** (TempoStep: ÷2, ×2, Reset on a track or a selection) | chosen | None: the tap is the yes | A run "Tempo halved · 12 tracks" | The withUndo toast (a short toast stays, its Undo the run's: choice 1) |
| **A cover** chosen or uploaded (AlbumMenu's Change cover; Health's) | chosen | None: the choice is the yes | A run "Cover · Album"; the embedded cover's bytes kept beside the journal | The withUndo toast (a short toast stays: 1) |
| **Single-track intake**: Missing tracks, Find a better copy, Work through (ReplaceTrackSheet) | chosen (the copy, after Cratarr's check) | The sheet it has: candidates, the fetch checked, "Add" | A run "Missing tracks · Album" | Its hand-made Undo toast |
| **Find missing tracks** in bulk (Health; /api/track-intake) | proposes (Cratarr's copies) | The review sheet: each album's tracks with the copy found, a copy to choose per track, none ticked at the start | One run per approval | Its per-row Undo (the run's per-album Undo instead) |
| **Replace in Music**, **move to a library**, **delete** (AlbumMenu, SeveralAlbums; folders) | chosen | Their own sheets as now (Replace's preview, the move's preview, Delete's question) | A run each ("Moved · 5 records"); Undo there | The withUndo toasts (a short toast stays: 1); in 0.14 after the tag writers (3) |
| **Record Edit** (overrides) | — | Writes no file. With "Write types and years" on, an edit shows as the record's "files differ · Review" | — | — |

## What the page gains

- **One review sheet for every kind of plan**: today it shows types and years. It gains a plan still being made (a
  look-up's meter: Renumber, the bulk intake's search); a choice inside a field (the genre, a track's copy); and a
  tick per field where a record's proposals differ in certainty (2).
- **Changes to files** lists every kind of run, titled by the server; a run's page shows its records as written, with
  Undo per album (as now).
- **The record's line** follows every kind of run, not only types and years.
- **Retired** for files: `/api/undo` tokens (the toast's Undo becomes the run's), HealthFixes' inline list, the certain-fixes ConfirmSheet,
  ReplaceTrackSheet's Undo toast, the bulk intake's row Undo. withUndo stays for what is not a file (playlists,
  crates, queues).

## What the server gives (a5)

- A plan per kind on `GET /api/file-changes/plans/{kind}`: `fixes:<finding>`, `renumber:<album>` (with the look's
  progress until it is made), `intake:<album>`, `tracks` (the bulk intake). Fields may carry choices (`options`) and a
  certainty.
- `POST …/plans/{kind}/apply` takes the ticked records, their stamps, and the choices made.
- The chosen writers (tempo, cover, track add, move, replace, delete) answer with a run's status (`FileRun`), not
  `{undo[]}`. Their Undo is the run's.
- One `file_run` event for all of them; the record's `files_state` for each.

## Order

0092's: Health's fixes (the length fixes with them), Renumber, the tempo, the covers, single-track intake. Then the
folder writers (move, replace, delete), in 0.14 too (3).

## The owner's choices (2026-11-01)

1. **After a chosen change** (tempo, cover, move, delete): the short Undo toast stays, its Undo the run's own (one
   mechanism, two places): a slip is undone at once.
2. **A tick per field** where a record's proposals differ in certainty: certain ones ticked, likely ones not; one tick
   per record elsewhere.
3. **The folder writers** (move, Replace, delete) come onto the path in 0.14, after the tag writers; their own sheets
   stay.
