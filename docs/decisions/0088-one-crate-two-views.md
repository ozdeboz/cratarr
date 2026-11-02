# 0088: One crate, two views; and or or

- Status: accepted (the owner's choices, 2026-10-31, with the shapes the server confirmed); for 0.13
- Date: 2026-10-31
- Builds on: 0024 (crates as conditions), 0061 (the condition cards), 0.11's quick crates and 0.12's sentence (the crate
  search's mockup B, "B with A's graph"), 0085 (offline first: names the device can show)

## Context

A crate grabbed as a sentence ("Hip Hop from the 1990s that sounds like Miles Davis") was saved as its conditions only.
Opened again (the owner, 2026-10-31, on the tablet: a saved crate "like Miles Davis · 1990s · like Nas"):

- the conditions held ids, not names: a Sounds like's seeds were `of: ['ar-…', 'ar-…']`, with nothing to say who they
  were;
- the builder's Sounds like card showed the cut alone: no seeds, none to add or take out, no "their own tracks";
- the sentence was gone: saving turned the terms into conditions one way, and the crate reopened in the builder, which
  cannot read conditions back into a sentence.

So a grabbed crate could be edited only before it was saved. And a crate's several terms of one kind always joined one
way: a Sounds like's seeds blended into one profile (between Miles Davis and Nas, one track at 60%), a kind's values
any of them.

## Decision

**One crate, two views.** Every crate opens with a Sentence / Conditions switch over the same conditions (the owner's
choice among seven ways to make a saved crate editable):

- The sentence says what it can of the conditions (the page reads them back into terms, `chipsOf`, the inverse of
  `conditionsOf`); what it cannot say (a negated or muted condition, an open range, a Sounds like cut by how many or
  weighted, a second row of one kind, any other axis) shows as "and N more conditions", a tap away in the other view.
- The conditions view is the builder's cards, the Sounds like card now showing its seeds by name, taking one out,
  adding an artist, album or track by search, and "Their own tracks".
- An edit in either is the crate's conditions, so the other shows it. The crate opens on the sentence where the
  sentence can say all of it, else on the conditions.
- The server keeps conditions only; no sentence is stored. It returns names for the ids the conditions hold.

**And or or.** Sounds like and the kinds a track carries several of get the choice (the owner's choice: "Sounds like +
multi-value kinds"):

- Sounds like: **and** blends the seeds into one profile (the default, the owner's); **or** is near any one of them.
- Genre, style, mood and tag: **or** is any of the values (the default); **and** is all of them.
- Not for year, label or album: a track has one of each, so "and" would always be empty, and is not offered (0085:
  nothing offered that cannot be upheld).
- In the sentence the word is the choice: "and" or "or" typed between two terms of one kind sets it, and the word in the
  sentence, tapped, flips it. In the builder a two-way switch on the card (two choices: a slim track).

## The shapes (the server's, confirmed 2026-10-31)

- **Sounds like:** `mode?: "blend" | "either"`, blend the default and left out of the JSON when blended. Either: one
  closeness per seed; a track passes when it is close enough (`min`) to at least one seed, ranked by its best seed;
  `count` caps as before. `/api/like-curve` takes the same `mode`, so the curve counts as the crate does.
- **All of:** `{"kind": "all_of", "values": [...]}`, on genre, style, mood and tag only; any other axis answers 422
  naming the axis. Negated it means "not all of these".
- **Names:** `names: {"<id>": {"name", "kind"}}`, kind artist, album or track, in `GET /api/crates/{id}` and in
  `/api/crates/preview`'s answer, for every `ar-`, `al-` and `tr-` id the conditions hold (a Sounds like's `of`, an
  album axis's values). An id the library no longer has is left out; the page shows it as "an artist no longer in the
  library" (or album, or track), removable.

## Consequences

- A grabbed crate stays editable as a sentence after it is saved, and any crate made in the builder reads as a sentence
  where it can.
- Built-in crates (a "Sounds like X" cut by how many) open on their conditions; switched to the sentence, the parts it
  can say show and the rest is counted.
- Two seeds that sound little alike can make a crate with **or** where **and** made almost nothing.
- The page's `chipsOf` and `conditionsOf` must stay each other's inverse: a test reads crates back and forth.
