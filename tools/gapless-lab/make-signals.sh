#!/bin/bash
# Makes the test material (needs ffmpeg and python with numpy): 30 s of reproducible noise (xorshift32, the same
# generator lab.html computes), cut into three tracks at 429093 and 900081 samples (not on any frame boundary), as FLAC
# repackaged into fragmented MP4 (as Cratarr's server will do) and as MP3 (LAME 192k), plus the references the
# analysis compares against: the noise itself, the MP3s decoded and trimmed exactly by ffmpeg, and FLAC/MP3/FLAC mixed.
set -e
cd "$(dirname "$0")"
python -I - <<'PYEOF'
import wave, struct
SR, N = 44100, 1323000
x = 2463534242; vals = []
for i in range(N):
    x ^= (x << 13) & 0xffffffff; x ^= x >> 17; x ^= (x << 5) & 0xffffffff
    vals.append(int(round((x / 4294967296 * 2 - 1) * 0.3 * 32767)))
w = wave.open('noise.wav', 'wb'); w.setnchannels(2); w.setsampwidth(2); w.setframerate(SR)
w.writeframes(b''.join(struct.pack('<hh', v, v) for v in vals)); w.close()
PYEOF
q="-hide_banner -loglevel error -y"
for spec in "1 0 429093" "2 429093 900081" "3 900081 1323000"; do set -- $spec
  ffmpeg $q -i noise.wav -af "atrim=start_sample=$2:end_sample=$3,asetpts=PTS-STARTPTS" n$1.wav
  ffmpeg $q -i n$1.wav -c:a flac n$1.flac
  ffmpeg $q -i n$1.flac -c:a copy -strict -2 -f mp4 -movflags +frag_keyframe+empty_moov+default_base_moof n$1.flac.mp4
  ffmpeg $q -i n$1.wav -c:a libmp3lame -b:a 192k n$1.mp3
  ffmpeg $q -i n$1.mp3 -ac 1 -f f32le n$1.mp3.f32
done
python -I -c "
import numpy as np, wave
w = wave.open('noise.wav'); x = (np.frombuffer(w.readframes(w.getnframes()), dtype='<i2')[0::2] / 32768).astype('<f4')
x.tofile('noiseref.f32')
m = np.concatenate([np.fromfile('n%d.mp3.f32' % i, dtype='<f4') for i in (1, 2, 3)]); m.tofile('mp3ref.f32')
np.concatenate([x[:429093], m[429093:900081], x[900081:]]).astype('<f4').tofile('mixedref.f32')
"
echo made
