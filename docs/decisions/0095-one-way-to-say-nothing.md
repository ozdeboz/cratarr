# 0095: One way to say nothing

- Status: accepted (the owner, 2026-11-02: "we do not yet have a unified way of zero results lists … no unified way for
  being able to reason about why there are zero results … no streamlined way for managing the looks in a single place
  … not a unified way to offer users options … not a unified way in which the loading spinner or content visibility
  interact with each other, or the zero results message"; "popover element could have something to do with how we
  should unify"; by question set: both, the errors fixed in 0.13 and this in 0.14). The page's parts are 632's, the
  server's a5's.
- Date: 2026-11-02
- Builds on: 0.13's empty page search (the words' callout, "A2"); docs/ui-conventions.md (Loading, Empty); the truth
  principle (everything shows the truth)

## Context

An inventory of release/0.13 (2026-11-02) found about 85 places that show a list, each saying "nothing" its own way:

- **Looks:** about 45 sentences written in place with a class of their own (`.empty`, `.muted`, `.note` and six
  more), 14 settings rows standing in, 15 sections left out without a word, 8 empty frames ("0 tracks" over a blank
  list), and one shared element: the words' callout, on a page's own search only.
- **Reasons:** most places give none; 12 take one from the server, 8 work one out. A filter that leaves nothing is
  never named ("No records.", as if there were none).
- **Actions:** the callout's ✕ is the only shared one. No empty list offers to clear a filter, switch a library on or
  try again.
- **Loading:** five signals (the large loader at once, the list loader after 300 ms, small marks, "…", nothing); four
  lists keep what they show while a new answer comes.
- **Errors in disguise:** Health read "All good." when its report failed; several lists read "nothing" after a failed
  request; five spun for ever; Home could go blank. Fixed in 0.13 as bugs; this decision keeps them from coming back.

## Decision

Every list passes through the same states, in this order, each with one look kept in one place.

1. **Loading.** What shows stays until the new answer is in; a sign only after a short wait (`LOADER_AFTER_MS`);
   "nothing" never before the answer (the owner, 2026-10-23: nothing shown that something else then replaces).
2. **Error.** One message in place, saying what could not be read, with **Try again**. Never an empty list, never a
   loader without end.
3. **Nothing, caused here.** When the listener's words, filters or settings left nothing, **the popover** says so: one
   element on pages, panels and sheets, its tip on the control that caused it (the magnifier, the funnel, the
   libraries' switch). Each cause is a chip with ✕ (the words; each filter; "favourites only"; "2 libraries off"),
   and **Clear all** when there are several: removing a chip is the action. It shows only when nothing on that surface
   answers (a crate card or the results found everywhere are answers).
4. **Nothing, truly.** A list with nothing in it that nothing here caused (a new playlist, an empty queue): plain text
   in place, from one component, with its one way on ("Download…, in an album's menu, keeps it here").

**Reasons are data, not sentences.** A list hands the popover or the empty text its reasons (what, the chip's label,
the action), never its own wording. The server says why it left things out (a filter, libraries switched off,
exclusions, favourites only) where it decides it: the filter's answer, a crate's pages, the Library.

**One component each,** built on the platform's popover and anchor positioning, measuring itself where a browser lacks
anchor positioning. The phone's and the tablet's WebViews (155, 156) have both.

## Consequences

- The places' own "No … match." sentences, the empty frames and the silently left-out sections go; a Home shelf left
  out because it has nothing stays left out, one that failed says so.
- docs/ui-conventions.md's Loading and Empty follow this text.
- A new list gets the four states by using the components, not by writing them again.

## Amendment, 2026-11-02: the true attribution

The owner: "I want the real true attribution so that if the filters leave a beautiful list but the search narrows it
to zero, then we know where to point at. And if the search has a list but the filter narrows it down, we should point
to the filter." Counts of what lifting one cause brings back cannot say that. They miss a combination, and the page's
own search words cannot be combined with them.

**One judgement.** The server judges each thing a list holds once, by the exact set of causes that fail it: each
condition of the filter, each switch, a crate's toggles and what is excluded everywhere, and the libraries switched
off. Everything said about an empty list is read from those sets, so no two parts can disagree:

- the reasons;
- each reason's `back` (the things only that cause keeps out);
- `all_back` (everything held);
- `blocked`, the page's attribution.

**`blocked`, on request.** With `explain` (the filter's body, a crate's rows and its preview as `explain=1`), the
answer carries:

- the causes that are on;
- every thing one of them keeps out, grouped by the exact set that does, with its keys as the answer gives them.

What no cause keeps out is not listed. The page asks only when what it shows is empty. It adds its own search as one
more cause per thing, and points at the set that truly empties the list.

**An artist** is kept out by what keeps out the album of theirs that comes back with the fewest causes lifted, plus
their own switches. A **Sounds like** is judged on the closest it picked among what every cause lets through. Lifting
a cause picks again, so a crate with one still selects again for the counts.

On a crate, the page's search words are judged on the server as one more cause (`q`, each of the page's queries, with
`explain=1`). Its blocked tracks never reach the page as rows, so their words can only be judged there. They are
matched as the page matches a row: by the same matcher, on the title (or file name), the artist and the album.
