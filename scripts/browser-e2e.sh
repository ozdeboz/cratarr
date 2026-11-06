#!/bin/sh
# The browser end-to-end tests (decision 0093's end-to-end suite, its page side; with a5, 2026-11-03): the page under
# development, in a real browser, against a real server that is thrown away after the run. Not a gate for previews.
#
#   scripts/browser-e2e.sh [image]        (from web/: npm run e2e:browser)
#
# In a temporary folder: the fixture library (`--make-fixtures`) and a fresh data folder; the image (cratarr:dev unless
# named) started on them in docker on a free port; two invented accounts made through /api/users, their passwords kept
# in that folder and never printed; the page served by Vite against it; the tests in web/e2e run; then the server, the
# page and the folder go.
#
# CHROMIUM: a Chromium (or Playwright's headless shell) already on the machine; playwright-core downloads none.
# HEAVY_LOCK: a lock file to hold while it runs (the server and a browser are heavy on a small machine); waits for it.
set -eu
IMAGE=${1:-cratarr:dev}
REPO=$(cd "$(dirname "$0")/.." && pwd)
export MSYS_NO_PATHCONV=1
NAME=cratarr-browser-e2e-$$

# A folder as the docker daemon sees it: from Git Bash on Windows the daemon is Docker CE inside WSL, where C:/x is
# /mnt/c/x; elsewhere the folder itself.
host() {
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
      w=$(cd "$1" && pwd -W)
      d=$(printf %s "$w" | cut -c1 | tr 'A-Z' 'a-z')
      echo "/mnt/$d/$(printf %s "$w" | cut -c4-)"
      ;;
    *) (cd "$1" && pwd) ;;
  esac
}
# A folder as Node sees it: from Git Bash on Windows, C:/x rather than /c/x.
native() {
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) (cd "$1" && pwd -W) ;;
    *) (cd "$1" && pwd) ;;
  esac
}
free_port() { node -e "const s=require('net').createServer().listen(0,'127.0.0.1',()=>{console.log(s.address().port);s.close()})"; }

if [ -n "${HEAVY_LOCK:-}" ]; then
  while [ -e "$HEAVY_LOCK" ]; do echo "waiting for $HEAVY_LOCK: $(cat "$HEAVY_LOCK" 2>/dev/null)"; sleep 30; done
  echo "browser e2e ($IMAGE), $(date -u +%FT%TZ)" > "$HEAVY_LOCK"
fi
TMP=$(mktemp -d "${TMPDIR:-/tmp}/cratarr-browser-e2e.XXXXXX")
VITE=
cleanup() {
  [ -n "$VITE" ] && kill "$VITE" 2> /dev/null || true
  docker rm -f "$NAME" > /dev/null 2>&1 || true
  rm -rf "$TMP"
  [ -n "${HEAVY_LOCK:-}" ] && rm -f "$HEAVY_LOCK" || true
}
trap cleanup EXIT INT TERM

mkdir -p "$TMP/data" "$TMP/fixtures"
H=$(host "$TMP")
# On a Windows folder (WSL's drvfs) only root may set a file's permissions: there the server runs as root.
case "$H" in /mnt/*) AS="--user 0:0" ;; *) AS="" ;; esac
docker run --rm $AS -v "$H/fixtures:/fixtures" --entrypoint /cratarr-server "$IMAGE" --make-fixtures /fixtures/library > /dev/null
cat > "$TMP/data/settings.json" << 'JSON'
{
  "version": 3,
  "music_folder": "/fixtures/library",
  "sources": [
    {
      "id": "cratarr",
      "kind": "cratarr",
      "enabled": true,
      "libraries": { "music": ["/fixtures/library"], "incoming": [] },
      "values": { "folders": ["/fixtures/library"] }
    }
  ]
}
JSON
SERVER_PORT=$(free_port)
docker run -d --name "$NAME" --memory 768m $AS -p "127.0.0.1:$SERVER_PORT:8080" -v "$H/data:/data" -v "$H/fixtures:/fixtures" "$IMAGE" > /dev/null
SERVER=http://127.0.0.1:$SERVER_PORT
i=0
until curl -fsS -m 3 "$SERVER/api/albums" > /dev/null 2>&1; do
  i=$((i + 1))
  [ "$i" -gt 90 ] && { echo "the server did not answer"; docker logs --tail 30 "$NAME"; exit 1; }
  sleep 1
done

# Two invented accounts: the first an administrator (with none yet, every caller is one), the second made by it. From
# the first, sign-in is asked of everyone.
ACCOUNTS=$(native "$TMP")/accounts
(umask 077 && cd "$REPO/web" && node e2e/accounts.mjs "$SERVER" "$ACCOUNTS")

PAGE_PORT=$(free_port)
(cd "$REPO/web" && CRATARR_URL=$SERVER exec node node_modules/vite/bin/vite.js --host 127.0.0.1 --port "$PAGE_PORT" --strictPort > "$TMP/vite.log" 2>&1) &
VITE=$!
i=0
until curl -fsS -m 3 "http://127.0.0.1:$PAGE_PORT/" > /dev/null 2>&1; do
  i=$((i + 1))
  [ "$i" -gt 60 ] && { echo "the page did not answer"; cat "$TMP/vite.log"; exit 1; }
  sleep 1
done

cd "$REPO/web"
E2E_PAGE=http://127.0.0.1:$PAGE_PORT E2E_ACCOUNTS=$ACCOUNTS node --test --test-concurrency=1 e2e/*.e2e.mjs
