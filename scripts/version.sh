#!/bin/sh
# One version for everything (the audit, 2026-10-29): VERSION at the repository's root is it. The Android app reads
# it as it builds (its versionCode worked out from it); the server, the page and the desktop app keep a copy where
# their tools want one, written here.
#
#   scripts/version.sh 0.9.19   set it everywhere
#   scripts/version.sh --check  say whether every copy is VERSION's (CI runs this)
#
# A version is plain X.Y.Z: a hotfix is the next patch (0.9.19, never 0.9.18-1, which semver reads as a prerelease
# of 0.9.18, older than it, and which Docker's `latest` does not move to).
set -eu
cd "$(dirname "$0")/.."

plain() { printf '%s' "$1" | grep -Eq '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'; }

# The server's packages, as the lock names them: cratarr-server and its crates (server/crates/*). The lock lists each at
# the one version (SPLIT-PLAN 6.6), or `--locked` fails after a bump; the rules keep their own.
server_packages() {
  printf 'cratarr-server\n'
  for d in server/crates/*/; do [ -f "$d/Cargo.toml" ] && printf 'cratarr-%s\n' "$(basename "$d")"; done
}

copies() {
  # [workspace.package]'s version, the first in the file.
  printf 'server/Cargo.toml %s\n' "$(sed -n 's/^version = "\(.*\)"$/\1/p' server/Cargo.toml | head -1)"
  for p in $(server_packages); do
    printf 'server/Cargo.lock %s %s\n' "$p" "$(sed -n "/^name = \"$p\"\$/{n;s/^version = \"\(.*\)\"\$/\1/p;}" server/Cargo.lock)"
  done
  for f in web/package.json desktop/package.json; do
    printf '%s %s\n' "$f" "$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$f" | head -1)"
  done
}

if [ "${1:-}" = "--check" ]; then
  want=$(cat VERSION)
  plain "$want" || { echo "VERSION is not X.Y.Z: $want"; exit 1; }
  bad=$(copies | awk -v w="$want" '$NF != w')
  [ -z "$bad" ] || { echo "Not VERSION's ($want):"; echo "$bad"; exit 1; }
  echo "$want everywhere"
  exit 0
fi

v=${1:?usage: scripts/version.sh X.Y.Z | --check}
plain "$v" || { echo "$v is not X.Y.Z (a hotfix is the next patch)"; exit 1; }
printf '%s\n' "$v" > VERSION
sed -i "0,/^version = \".*\"\$/s//version = \"$v\"/" server/Cargo.toml
for p in $(server_packages); do
  sed -i "/^name = \"$p\"\$/{n;s/^version = \".*\"\$/version = \"$v\"/;}" server/Cargo.lock
done
for f in web/package.json desktop/package.json; do
  sed -i "0,/^  \"version\": \".*\",\$/s//  \"version\": \"$v\",/" "$f"
done
"$0" --check
