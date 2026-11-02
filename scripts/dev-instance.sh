#!/bin/sh
# The development instance (decision 0093): a second Cratarr in docker on the development machine, with a data folder
# and music of its own, fenced off from the world. Made again from this script whenever it is run; its data folder is
# kept until it is thrown away by hand.
#
#   scripts/dev-instance.sh <folder> [port]
#
# <folder> holds, and is made with:
#   music/     the sample of real albums (scripts/dev-sample.sh copies them there), one folder per album artist
#   fixtures/  the fixture library (`cratarr-server --make-fixtures`), made once
#   data/      the instance's own settings, catalogue, user data and journal: never copied anywhere
#
# The fixtures are an Incoming library: Cratarr has one Music folder (the first Music library with audio), the sample.
#
# Fenced off: one account, the owner's, with sign-in off (nobody signs in; it is for the machine's own network: what is
# an account's own, Home's shelves and an artist's page, needs an account to be, 2026-11-02), MusicBrainz switched off
# at its first start, no service keys, no Lidarr, slskd or Jellyfin. 768 MB, as production.
set -eu
DIR=${1:?usage: scripts/dev-instance.sh <folder> [port]}
PORT=${2:-8095}
NAME=cratarr-dev
IMAGE=cratarr:dev
REPO=$(cd "$(dirname "$0")/.." && pwd)
export MSYS_NO_PATHCONV=1
# A folder as the docker daemon sees it: from Git Bash on Windows the daemon is Docker CE inside WSL, where C:/x is
# /mnt/c/x; elsewhere the folder itself.
host() {
  p=$(cd "$1" && pwd)
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) d=$(printf %s "$p" | cut -d/ -f2); echo "/mnt/$d/${p#/$d/}" ;;
    *) echo "$p" ;;
  esac
}

mkdir -p "$DIR/music" "$DIR/fixtures" "$DIR/data"
H=$(host "$DIR")
# On a Windows folder (WSL's drvfs) only root may set a file's permissions, which every writer does when a written copy
# takes a file's place: there the instance runs as root. Elsewhere, as the image's own user.
case "$H" in /mnt/*) AS="--user 0:0" ;; *) AS="" ;; esac
echo "building $IMAGE from $REPO"
(cd "$REPO" && docker build -q -t "$IMAGE" .) > /dev/null

if [ ! -d "$DIR/fixtures/library" ]; then
  echo "making the fixture library"
  docker run --rm $AS -v "$H/fixtures:/fixtures" --entrypoint /cratarr-server "$IMAGE" --make-fixtures /fixtures/library
fi

if [ ! -f "$DIR/data/settings.json" ]; then
  cat > "$DIR/data/settings.json" <<'JSON'
{
  "version": 3,
  "music_folder": "/music",
  "sources": [
    {
      "id": "cratarr",
      "kind": "cratarr",
      "enabled": true,
      "libraries": { "music": ["/music"], "incoming": ["/fixtures/library"] },
      "values": { "folders": ["/music", "/fixtures/library"] }
    }
  ]
}
JSON
  # The one account, there before the first start: a Cratarr that starts with an account keeps sign-in off (a new one
  # without would require it, once someone has an account).
  printf '%s
' '[{ "name": "owner", "password": "", "admin": true }]' > "$DIR/data/subsonic-users.json"
  chmod 600 "$DIR/data/subsonic-users.json"
  FIRST=1
else
  FIRST=0
fi

docker rm -f "$NAME" > /dev/null 2>&1 || true
docker run -d --name "$NAME" --restart unless-stopped --memory 768m $AS \
  -p "$PORT:8080" \
  -v "$H/data:/data" -v "$H/music:/music" -v "$H/fixtures:/fixtures" \
  "$IMAGE" > /dev/null

echo "waiting for it to answer on :$PORT"
i=0
until curl -fsS "http://127.0.0.1:$PORT/api/albums" > /dev/null 2>&1; do
  i=$((i + 1))
  [ "$i" -gt 120 ] && { echo "it did not answer"; docker logs --tail 40 "$NAME"; exit 1; }
  sleep 1
done
if [ "$FIRST" = 1 ]; then
  curl -fsS -X PUT -H 'content-type: application/json' -d '{"enabled":false}' "http://127.0.0.1:$PORT/api/settings/musicbrainz" > /dev/null
  echo "MusicBrainz switched off"
fi
echo "up: http://127.0.0.1:$PORT"
