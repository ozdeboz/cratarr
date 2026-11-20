# 0106: The server crate split — small crates for a small edit-and-test loop

- Status: accepted (the owner, 2026-11-17 for after 0.17; moved up 2026-11-19 to right after 0.17's last preview);
  the first change of 0.18
- Date: 2026-11-19
- Builds on: 0104 (the domain core: the split gives its parts their homes), 0092 (one way to change files: the gate's
  own crate)

## Context

The server was one crate of about 150,000 lines. Every edit compiled it twice, the binary and the test harness, and
linked one large test binary. A check took about a minute, a filtered test run one and a half to four, the end-to-end
suite eighteen. The owner found the development loop "highly inefficient" and moved the split up from after 0.17's
release to the moment 0.17's last preview is cut.

## Decision

1. **Crates:**
   - `rules`: pure, shared with the page through wasm, unchanged;
   - `core`;
   - `media`;
   - `store`;
   - `clients`: the outside services;
   - `gate`: the file gate, reachable only through itself;
   - `app`;
   - `api`;
   - a thin `cratarr-server` binary.

   It is a directed graph with no cycles.
2. **Moves only:**
   - every back-reference is broken first, inside the one crate, while it is green: types and constants moved down, a
     trait in the lower crate, a few files moved up;
   - then the files move with their paths kept inside their new crate, and each crate re-exports the ones below under
     their old names;
   - no formatter runs, so the diffs stay moves and import lines.
3. **Each step ends green,** with the same test counts before and after.
4. **Phase 2,** cutting `app` by object area, comes with the domain core's runs: 85% of recent commits touch `app`.
5. **One boxing spawn helper** follows the moves in its own commit, against the compile time that tasks' future types
   cost.

## Consequences

- **0.17's release branch is cut before the split.** 0.17's later fixes are carried to the split tree by hand.
- **Expected, to be measured with the same one-line edits as before the split:**
  - a store edit and its tests drop from about 68 s to about 18 s;
  - an api edit from 93 s to about 20 s;
  - the small crates' to a few seconds;
  - app edits are about 1.8× faster until phase 2.
- **Tooling:** the test runner takes a package, and the image build and CI build the workspace.
