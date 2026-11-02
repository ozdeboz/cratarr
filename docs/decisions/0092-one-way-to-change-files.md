# 0092: One way to change files

- Status: accepted (the owner, 2026-10-31: "do we now have a properly unified path of file+tag handling? I want a
  clean and coherent architecture!", "Not scattered, but logical please", "we should probably think of our rules,
  wherever we have appropriate ones, to decide how this all should be presented to our users"; by question set: draft
  it now, before types into files is merged; always asked; files follow Cratarr; the server's parts reviewed by a5 the
  same day). For 0.13. The server's parts are a5's to build, the page's 632's.
- Date: 2026-10-31
- Builds on: 0008 (the catalogue: tags read in two layers), 0011 and 0026 (the journal, the app's one undo journal),
  0013 (Keep and Replace: moves all or none), 0053 (tags without touching the audio), Health's fixes (0.9.7: core facts
  written after an administrator approves, never before), 0080 (single-track intake, the March 2026 lesson), 0089
  (facts set by hand)
- Amends: 0089 (with types into files on, what was set by hand goes into the files too)

## Context

Reading is one path: the catalogue reads every file's tags in two layers (0008). Writing tags is nearly one:
`retag::rewrite` writes a copy, checks its audio byte for byte against the original's and only then takes its place,
changing only the keys an edit touches. Health's fixes, Renumber, the covers inside the files, the tempo and
single-track intake all write through it.

Everything around the writing is each writer's own:

- **Undo.** Health's fixes, Renumber, the length fixes, single-track intake, the covers and the tempo each keep their
  own "before" files and their own Undo. Only Incoming and the trash use the journal, which 0026 made the app's one
  undo journal. Types into files (0.13) would have had none yet.
- **Moves.** Keep and Replace move all of an album's files or none (`moves.rs`); single-track intake, the trash and the
  shelf move files their own ways.
- **When a yes is needed.** Health's fixes wait for an administrator's tap; types into files was to write small
  differences by itself.
- **The length fixes** change an MP3's info frame or a FLAC's STREAMINFO outside the checked copy.
- **What the person sees.** Each writer says its progress, its failures and its Undo in its own shape.

## Decision

**Every change to the library's files is a plan, gated, applied one way, recorded once and told once.**

1. **A plan:** for each file, what changes and why. Kinds: tags set or cleared by key; a cover embedded or written
   beside; an audio header (the length fixes); a move; a delete into the trash. Each change carries its reason and its
   source (Health's finding, Renumber's release, the Edit sheet, the type rule, a Keep), and what the file held before.
   A plan is made from the catalogue (quick), and each file is read again just before it is written: one whose fields
   no longer match the plan's before (Lidarr or a person changed it meanwhile) is set aside as "changed since it was
   planned", not written.
2. **The gate, one rule:** nothing is written into the library's files without an administrator's yes to that plan
   (Health's fixes' rule; the March 2026 lesson, 0080). A switch never writes by itself: it decides what is offered.
   The yes is either the tap that is the action (Keep, Replace, Renumber, an intake's approval, an Edit sheet's Save
   where it writes files) or the tap on what is offered (Health's Fix, Write). However few files, it is asked (the
   owner: always asked). Write, Fix and Renumber are administrators'; anyone else's Edit sheet Save changes only
   Cratarr's facts, and their files follow at the next Write an administrator gives.
3. **Applied one way:** one album at a time, paced, and one run at a time: a single lock over every change to the
   library's files (types into files, the fixes, Renumber, Keep and Replace, intake, the trash's moves); a second run
   waits, and says it is waiting. Tags and audio headers through the checked copy (only what changes; the audio byte
   for byte, or for a header the frames its check names); moves all or none; deletes into the trash. A file that fails
   is set aside with its reason, and its album's other files are still written; the run goes on.
4. **Recorded once:** a run is one journal entry (0011, 0026) holding each file's before. A tag's before is small and
   goes in the entry; an embedded cover's (megabytes) and a header's bytes are kept as files beside the journal, named
   in the entry and pruned with it. The journal keeps runs as long as the trash keeps what it holds (`trash_days`). One
   Undo per run, and per album within it where the page offers that; an album's Undo undoes what was written of it.
   Undo checks as writing does: it puts a file back only where the file still holds what the run wrote, else sets it
   aside as "changed since" and says so, never overwriting it. The writers' own before-files go.
5. **Told once:** a run's status in one shape, for every writer: its `id`, what it is, its `state` (waiting, writing,
   done), `done` of `of`, `failed: [{album, why}]`, when it ran, its undo token and `undone`. Told on the live channel
   to open pages.

**How the page shows it: three pieces, the same for every writer** (the owner, 2026-11-01, on the second round of
mockups: "Yes, this is a lot better"; learned from Picard's original and new values, Lidarr's Preview Retag and
Activity, beets' diff, MediaMonkey's tag sync; mockups at https://claude.ai/artifact/3tfcXWgYpuTbe799uRQgTL):

- **Before: the review sheet.** One panel page for every plan: its title says what proposes it ("Types into files",
  "Renumber · the 1998 CD release", "Missing years"); the records, each with a tick where there are several; under each,
  its fields as the file has them, struck, and as Cratarr has them, each saying where the new value comes from (the type
  rule, set by hand, MusicBrainz). One button at its foot with the count, named for the writer ("Write 11",
  "Renumber", "Fix"). Someone who is not an administrator sees the same sheet without the button.
- **On the item: the record's state line.** Under a record's title in its panel, the details line where "7 of 12
  tracks" sits: its files differ from Cratarr (Review), waiting to write, writing (the meter, "2 of 4"), written
  (Undo), or a file changed since it was planned (Review). Nothing while the files agree with Cratarr.
- **During and after: Changes to files.** Health › Files: every run in one list, Now (the one writing, with the
  meter; those waiting), then by day: done with Undo, what was set aside, undone. A run opens as the review sheet as
  written, with Undo per album; kept as long as the journal keeps it.
- **Health's lines point.** A kind of difference is a line with its count and ›, opening the review sheet; "Changes
  to files" names what is writing and waiting. No meter and no Undo in them: one place for each.
- **What is reviewed:** whatever Cratarr proposes (a rule's types, a finding's year, a release's numbers, a fact set by
  hand going into the files). What you set yourself on what you see (halve this track's tempo) and moves you chose
  (Keep, Replace) are the yes already. Every run lands in Changes to files, with Undo.
- **A setting's switch** only decides what Health lists. Its words never promise a write.

**Amendment to 0089:** with "Write types and years into files" on, the files follow Cratarr: what Cratarr holds for
a record's types, year and Classical, found or set by hand, is what Write writes. A file's own words (title, artists,
track numbers) stay as 0089 has them: fixed in the tags, never set by hand in Cratarr.

## Consequences

- One place answers "what was done to this file, and how to undo it": the journal.
- The writers become plans on the one path, in this order: types into files (new, so first), Health's fixes,
  Renumber, the tempo, the covers, the length fixes, single-track intake. Each one's own before-files and Undo go as it
  moves. Their tests move with them, and one set of tests holds the path (gate, apply, record, undo).
- Every writer's progress, failures and Undo look the same on the page.
- Not on this path: derived files (the CD-format copies, the Opus and stream forms), made beside the library, not
  changes to its files; and playlists' `.m3u` files, which are edits of a person's own list on their own path (0025),
  journaled already.
