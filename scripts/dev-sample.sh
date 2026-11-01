#!/bin/sh
# The development instance's sample (decision 0093): the albums a list names, copied from the music server over SSH,
# read only there (tar reads, nothing is written on it), into the development instance's music folder, one folder per
# album artist as on the server. An album already copied is left as it is (its writers may have changed it); remove
# its folder to copy it again.
#
#   scripts/dev-sample.sh <sample list> <music folder> [ssh host]
#
# The list: one album folder per line as the server has it (its root, then `Artist/Album`), anything after a tab a
# comment; `#` lines are comments. The roots it knows are ROOTS below; a path under another is refused.
set -eu

list=${1:?the sample list}
dest=${2:?the music folder}
host=${3:-nas}
ROOTS='/Volume1/media/Music /Volume1/media/lidarr-music'

mkdir -p "$dest"
copied=0
kept=0
while IFS= read -r line || [ -n "$line" ]; do
  path=${line%%	*}
  case "$path" in '' | '#'*) continue ;; esac
  root=
  for r in $ROOTS; do
    case "$path" in "$r"/*) root=$r ;; esac
  done
  if [ -z "$root" ]; then
    echo "not under a known root, left out: $path" >&2
    continue
  fi
  rel=${path#"$root"/}
  if [ -d "$dest/$rel" ]; then
    kept=$((kept + 1))
    continue
  fi
  echo "copying $rel"
  # The path goes to the server's shell quoted, so names with spaces, quotes and other signs arrive as they are.
  quoted=$(printf "%s" "$rel" | sed "s/'/'\\\\''/g")
  ssh "$host" "tar -C '$root' -cf - -- '$quoted'" | tar -C "$dest" -xf -
  copied=$((copied + 1))
done < "$list"
echo "$copied copied, $kept already there"
