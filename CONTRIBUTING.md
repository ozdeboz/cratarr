# Contributing

## Branches

- `main` is the trunk: what the next preview or release is built from, and nothing else.
- Work happens on a short-lived branch off `main`, named for the task (`playlist-stats`, `folders`), one task each.
  When it is done and tested it goes into `main` once — a fast-forward, or a single merge when `main` moved meanwhile
  — and the branch is deleted. Branches are not merged back and forth between each other.
- Before a branch goes in, bring `main` into it (merge or rebase) and test that, so `main` only ever gets what was
  tested together.
- Every build that is put to use gets a tag on the commit it was built from (`preview-50`), so what runs can always be
  found again.

## Before a change goes in

- Server: `cargo test` in `server/` (the whole suite, not only the part changed).
- Web: `npm run check` in `web/` (svelte-check, no errors or warnings), and the change looked at in a browser, on a
  phone's width too where it shows there (`npm run check:phone`).
- Choices that shape the product or its data are written down first in [docs/decisions](docs/decisions)
  (`NNNN-slug.md`: status, date, context, decision, consequences); a later change to one is an amendment to it, dated.
  UI follows [docs/ui-conventions.md](docs/ui-conventions.md).

## Commits

- One subject line that says what changed for the person using Cratarr, then why, and anything a reader of the code
  later would need: what was measured, what was tried and dropped, which decision it follows.
- Tests that pin the behaviour come in the same commit as the change.
