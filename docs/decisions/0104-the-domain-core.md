# 0104: The domain core — one home per object, its state deciding what is shown

- Status: accepted (the owner, 2026-11-19, by question while testing 0.17's first preview); built in 0.18
- Date: 2026-11-19
- Builds on: 0058 (one menu system), 0070 (requests), 0092 (one way to change files), 0095 (one way to say nothing), 0096
  (libraries equal), 0097 (words), 0100 (records, artists and labels), 0101 (own music, one track id), 0105 (no source
  ranks above another)
- Leads to: 0106 (the crate split, shaped along the core's lines), 0107 (pictures as facts)

## Context

Testing 0.17's first preview found the same kinds of fault again and again. Each new surface had grown its own copy of
what a record, a request or a Soulseek item is:

- nine sheets for one record;
- a request's state worked out in four places and read by matching its words in 25;
- six rules for whether a record is had;
- approval words left behind after the approval step went;
- two dates for one delivery;
- a filter sheet that showed nothing of a crate's own.

Fixing them one at a time never ended, because the next surface copied the logic again.

The owner chose to re-found the domain layer, not the app ("Domain core now"), using established models rather than
inventing:

- an object model (OOUX's objects, relationships, calls to action and attributes);
- statecharts;
- server-sent actions (the hypermedia idea);
- decision tables ("when" clauses).

## Decision

1. **Objects:** record, copy, request, Soulseek item, artist, label, track, library, account, crate, playlist and
   Health issue. Each has one key and one home. A version, an arrival and a request's standing are views worked out from
   them, never stored apart. Names follow MusicBrainz's model where they meet: release group, release, recording, work.
2. **Machines:** the request, the Soulseek item and the arrival each move by one pure `next(state, event)`. A record's
   holding is worked out from its copies by one rule. Waits, holds, retries and progress are job states, shared with
   0.18's job runner. Each state's words come from one table. The pure domain (the machines, the request's standing,
   `decide()` and `can()`, the wording and when-tables, the facts' resolving, the matcher, the views' types) lives in a
   crate of its own, `cratarr-domain`: pure, above `rules`, below `store` (0106). `rules` keeps what the device also
   runs: a record's holding and its pairing of files to tracks, and the lists (amended 2026-11-20).
3. **Facts with their sources** (0105): every attribute keeps each source's value, the files' and the hand's. One rule
   resolves them: the hand, then the files, then the services as equals. Disagreement shows as versions. Pictures are
   facts too (0107).
4. **Permissions:** one `can(viewer, action, object, state)` on the server. The page never works out a right itself.
5. **What the server sends per object:** one view of the same shape wherever the object shows. It carries the
   attributes, the state with its words and dates, the actions this viewer may take now, the sections that apply, and a
   version. The page's types are generated from the server's.
6. **The record sheet:** one sheet per object kind. The record's state decides its sections, the viewer decides the
   actions, and the opening place adds only its context (its tracks in this crate) and the order (what you came for
   first). A list is the object's kind plus a filter, an order and a view. Each kind declares once what it can be
   filtered by.
7. **One command path:** every action is a command: plan, a yes where asked, apply, a journal entry, one Undo. It
   merges with the file gate (0092).
8. **One identity matcher:** every join (artists, labels, records and editions, tracks, Soulseek folders, list rows)
   goes through one matcher that says why. The owner's rules are its hard limits: never an id moved or merged by a
   name, and same-name artists each keep their own id. Two copies whose tracks pair one for one are one record; each
   copy keeps the edition its files are.
9. **Live updates per object:** the server names the objects that changed, and one page cache keyed by object serves
   every surface. Nothing polls.

## Consequences

- 0.18 builds it beside the old code and moves the surfaces onto it one object kind at a time, deleting each old path
  as it moves:
  1. records and the one record sheet, with live updates;
  2. requests and Soulseek;
  3. artists and labels, with the picture picker.
- The design, its prior art and an inventory of today's duplications are kept with the plan. The prior art includes
  MusicBrainz's model, beets' matching, Seerr's request states, Lidarr's accept-or-reject rules and Siren's entity
  shape.
- The data changes are rehearsed together on a copy of production's data before the upgrade: a request column, a table
  for the facts with no home of their own, and the others 0.18's plan lists with its risks (amended 2026-11-20; first
  written as two).
- A new page dependency, a query cache (MIT), comes in with live updates.
