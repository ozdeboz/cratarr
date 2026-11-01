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
# Fenced off: no accounts (nobody signs in; it is for the machine's own network), MusicBrainz switched off at its
# first start, no service keys, no Lidarr, slskd or Jellyfin. 768 MB, as production.
set -eu
DIR=${1:?usage: scripts/dev-instance.sh <folder> [port]}
PORT=${2:-8095}
NAME=cratarr-dev
IMAGE=cratarr:dev
# A path docker takes on Windows too (Git Bash: C:/…, not /c/…).
REPO=$(cd "$(dirname "$0")/.." && (pwd -W 2>/dev/null || pwd))
export MSYS_NO_PATHCONV=1

mkdir -p "$DIR/music" "$DIR/fixtures" "$DIR/data"
echo "building $IMAGE from $REPO"
docker build -q -t "$IMAGE" "$REPO" > /dev/null

if [ ! -d "$DIR/fixtures/library" ]; then
  echo "making the fixture library"
  docker run --rm -v "$DIR/fixtures:/fixtures" --entrypoint /cratarr-server "$IMAGE" --make-fixtures /fixtures/library
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
      "libraries": { "music": ["/music", "/fixtures/library"] },
      "values": { "folders": ["/music", "/fixtures/library"] }
    }
  ]
}
JSON
  FIRST=1
else
  FIRST=0
fi

docker rm -f "$NAME" > /dev/null 2>&1 || true
docker run -d --name "$NAME" --restart unless-stopped --memory 768m \
  -p "$PORT:8080" \
  -v "$DIR/data:/data" -v "$DIR/music:/music" -v "$DIR/fixtures:/fixtures" \
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
