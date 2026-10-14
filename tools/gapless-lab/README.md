# Gapless lab (decision 0022)

Measures, to the sample, whether the local player's continuous stream (MSE) plays tracks without a gap, on any
browser, including a phone or tablet, without anyone listening.

1. `./make-signals.sh` (ffmpeg + python with numpy): the test tracks and references.
2. `python -I serve.py`: serves the lab on port 5180, hands out tests (`/job`) and takes results and raw recordings.
3. On the device: open `http://<this machine>:5180/lab.html`, tap Start once, keep the page in front with the screen
   on (Android pauses a page in the background). Without https the page records with a ScriptProcessor, which can
   lose 128-sample blocks; the analysis compares locally at each boundary, so such losses elsewhere do not count.
4. `./tablet-run.sh`: FLAC, MP3 and FLAC/MP3/FLAC, each analysed by `analyse.py` (the jump at each boundary in
   samples: 0 = gapless; + lost, - repeated).

Result 2026-10-14 (Lenovo Tab, Chrome 155 on Android 10, and Edge on Windows): jump 0 at every boundary for all three,
once MP3s are trimmed by LAME's encoder delay only (Chrome's decoder drops the Info frame and adds no decoder delay;
trimming the textbook 1152 + delay + 529 lost 1681 samples per track, the click heard first).
