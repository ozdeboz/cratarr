#!/bin/bash
# Runs the three gapless tests on whatever lab client polls (the tablet), one after the other, and analyses each.
cd "$(dirname "$0")"
run() {
  id=$1; tr=$2; ref=$3
  rm -f capture-$id.f32 result-$id.json
  printf '{"id":"%s","tracks":%s,"ref":"%s"}' "$id" "$tr" "$ref" > job.json
  echo "$(date +%T) published $id"
  for i in $(seq 1 180); do [ -f capture-$id.f32 ] && break; sleep 1; done
  if [ ! -f result-$id.json ]; then echo "$(date +%T) $id: no result after 180 s"; return; fi
  python -c "import json; d=json.load(open('result-$id.json')); print(d.get('error') or ('rate %s, dropped blocks %s, captured %s s' % (d.get('sample_rate'), d.get('dropped_blocks'), d.get('captured_s'))))"
  [ -f capture-$id.f32 ] && python -I analyse.py capture-$id.f32 "$ref" 429093,900081 | grep -E "^capture|^boundary"
}
run tab-flac '[{"url":"n1.flac.mp4","kind":"flac","samples":429093},{"url":"n2.flac.mp4","kind":"flac","samples":470988},{"url":"n3.flac.mp4","kind":"flac","samples":422919}]' noiseref.f32
run tab-mp3 '[{"url":"n1.mp3","kind":"mp3","samples":429093},{"url":"n2.mp3","kind":"mp3","samples":470988},{"url":"n3.mp3","kind":"mp3","samples":422919}]' mp3ref.f32
run tab-mixed '[{"url":"n1.flac.mp4","kind":"flac","samples":429093},{"url":"n2.mp3","kind":"mp3","samples":470988},{"url":"n3.flac.mp4","kind":"flac","samples":422919}]' mixedref.f32
rm -f job.json
echo "$(date +%T) done"
