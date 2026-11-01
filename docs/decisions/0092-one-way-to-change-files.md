# 0092: One way to change files

- Status: proposed (the owner, 2026-10-31: "do we now have a properly unified path of file+tag handling? I want a
  clean and coherent architecture!", "Not scattered, but logical please", "we should probably think of our rules,
  wherever we have appropriate ones, to decide how this all should be presented to our users"; by question set: draft
  it now, before types into files is merged; always asked; files follow Cratarr). For 0.13. The server's parts are
  a5's to review and build, the page's 632's.
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
   source (Health's finding, Renumber's release, the Edit sheet, the type rule, a Keep).
2. **The gate, one rule:** nothing is written into the library's files without an administrator's yes to that plan
   (Health's fixes' rule; the March 2026 lesson, 0080). A switch never writes by itself: it decides what is offered.
   The yes is either the tap that is the action (Keep, Replace, Renumber, an intake's approval, an Edit sheet's Save
   where it writes files) or the tap on what is offered (Health's Fix, Write). However few files, it is asked (the
   owner: always asked).
3. **Applied one way:** one album at a time, paced. Tags and audio headers through the checked copy (only what
   changes; the audio byte for byte, or for a header the frames its check names); moves all or none; deletes into the
   trash. A file that fails is set aside with its reason, and the run goes on.
4. **Recorded once:** a run is one journal entry (0011, 0026) holding each file's before. One Undo per run, and per
   album within it where the page offers that. The writers' own before-files go.
5. **Told once:** a run's status in one shape, for every writer: what it is, `done` of `of`, `failed: [{album, why}]`,
   when it ran, its undo token. Told on the live channel to open pages.

**How the page shows it** (from the rules there are: the truth principle, one progress meter, minimal words, Undo
after an action):

- **Where the files differ from Cratarr, Health says so:** one line for each kind of difference, how many, and one
  action ("12 records' files differ from their types and years · Write"). An item open shows its before and after.
- **While a run writes:** "Writing 3 of 12" with the one meter, in the line that started it; the page stays usable.
- **After a run, in the same place:** what it did ("Wrote 12 records") with Undo, and each failure with its reason
  (the gate's own sentence). No notice beyond that.
- **A setting's switch** only decides what Health offers. Its words never promise a write.

**Amendment to 0089:** with "Write types and years into files" on, the files follow Cratarr: what Cratarr holds for
a record's types, year and Classical, found or set by hand, is what Write writes. A file's own words (title, artists,
track numbers) stay as 0089 has them: fixed in the tags, never set by hand in Cratarr.

## Consequences

- One place answers "what was done to this file, and how to undo it": the journal.
- The writers become plans on the one path, in this order: types into files (new, so first), Health's fixes,
  Renumber, the tempo, the covers, the length fixes, single-track intake. Each one's own before-files and Undo go as it
  moves. Their tests move with them, and one set of tests holds the path (gate, apply, record, undo).
- Every writer's progress, failures and Undo look the same on the page.
- Derived files (the CD-format copies, the Opus and stream forms) are made beside the library, not changes to its
  files: not on this path.
