# Releasing

For maintainers. How a version is made; where and how a maintainer runs it is their own.

## Branches

- `trunk` holds the next version's work.
- When a version's scope is settled, `release/X.Y` is cut from `trunk`. From then on, that version's fixes land on
  the release branch and are merged forward into `trunk`; new work stays on `trunk`.

## A preview

1. The version everywhere: `scripts/version.sh X.Y.Z-preview.N` (`--check` says whether every copy agrees).
2. Everything in [CONTRIBUTING.md](CONTRIBUTING.md#before-a-change-goes-in) passes, the end-to-end suite included:
   `docker build --target e2e .` ([0093](docs/decisions/0093-test-and-development-data.md)). A preview is built only
   once it passes, and a change to how files are written reaches one only after it was proved there.
3. The image: `docker build -t cratarr:X.Y.Z-preview.N .` on the release branch.
4. Tried before it reaches anyone's library: on an instance that is nobody's production, with a copy of real data,
   its music read-only and its outside services (scrobbling, downloads, other servers) switched off.

## A release

1. What changed, for people: `docs/whats-new-X.Y.md`, from the decisions and plans it closes.
2. The version: `scripts/version.sh X.Y.Z` (plain X.Y.Z; a fix after it is the next patch).
3. The checks of a preview, again.
4. The tag `vX.Y.Z` on the release branch, and the image built from it.
