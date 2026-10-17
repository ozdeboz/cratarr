# 0046: Visuals: each track's own fractal flame, full screen

- Status: accepted (the owner, 2026-10-17: "make a full screen visualization mode accessible from the full screen
  player", then "Time to install it on my tablet now")
- Date: 2026-10-17
- Builds on: 0018 (the full player), 0035 (audio analysis: waveform and tempo)

## Context

The owner wanted visuals in the style of Electric Sheep (Scott Draves): slowly morphing, glowing fractal flames, full
screen, following the music. The salon tablet in particular is a display as much as a player. Electric Sheep's own
flames ("sheep") cannot be shipped: the generated ones are licensed CC BY-NC 3.0 (human-made ones CC BY, Gold Sheep
under their own terms), and non-commercial terms do not combine with the AGPL (0029). Its client is GPL-2.0-only, read
only for us.

What was researched (2026-10-17), and what came of it:

- **The algorithm** is published: Draves and Reckase, "The Fractal Flame Algorithm" (2003, revised 2008; flam3.com):
  transforms of an affine map and weighted nonlinear variations, the chaos game with a colour coordinate, a log-density
  display with gamma and vibrancy, density estimation, symmetry. Electric Sheep animates a flame by turning the linear
  part of each transform once around per loop, and blends two flames by interpolating each affine column by angle and
  log length (flam3's default "log" interpolation), with a smoothstep. Written from the paper, nothing is copied.
- **Code we may reuse**, should it come to that: flam3 (GPL-3.0-or-later), JWildfire (LGPL-2.1-or-later), Apophysis
  (GPL-2.0-or-later), Butterchurn (MIT); not the Electric Sheep client, nor anything without a licence. Bundled content
  (sheep, MilkDrop presets, flam3's and JWildfire's palettes) stays out: their authors' rights are unclear or
  non-commercial.
- **Alternatives** looked at: strange attractors (Clifford, de Jong), curl-noise trails, domain-warped noise,
  reaction-diffusion, Physarum, MilkDrop-style feedback, Buddhabrot. Flames are the look asked for; the attractors and
  curl noise could reuse the same drawing later.
- **Mobile GPUs** (the tablet's Mali-G52 MC2, the phone's Mali-G615): float buffers are near universal on Android
  WebGL2, blending into 32-bit floats too (EXT_float_blend); a 16-bit float buffer stops counting near 2048 points a
  pixel. WebGPU is not certain in the app's WebView.

## Decision

- **Where:** a Visuals button (sparkles) in the full player's top bar, right side, while something plays. It opens the
  visuals over everything, full screen where the browser allows, on black; the screen is kept on (Wake Lock) where
  the browser allows. A tap or a mouse move shows the close button and the track (title, artist) for three seconds;
  Esc, Back or a swipe down closes them.
- **Each track its own flame:** made from the track's id (`source/id`), so a track always looks the same. A flame is
  2–4 random transforms (contractions turned and sheared, 1–3 variations of 24 from the paper's list, gentle ones more
  often), with rotational symmetry about half the time (flam3's way: rotations that keep the colour and do not turn),
  each transform its own stretch of a cosine palette (Inigo Quilez's formula) of two or three hues. Before it is shown
  it is run on the CPU (12,000 steps): one that escapes, collapses to a point or a line, fills its frame or scatters
  thinly over it is passed over (up to 40 tries; the best one otherwise), and it is framed by where its points fall.
- **Drawing (WebGL2):** points carry the chaos game from frame to frame by transform feedback, one step per pass, each
  pass also drawing them as single pixels added into a float buffer that fades a little every frame (7 %): the picture
  is the last second or so of points, which gives a flame the samples it needs without drawing them all every frame.
  New points take 20 steps unseen first (flam3's fuse). The screen shows the buffer by the paper's log-density display,
  gamma 3, vibrancy 1, flam3's gamma threshold, and a blur that widens where few points fell (a cheap density
  estimation). 32-bit floats where they can be blended into, else 16-bit, else 8.
- **Following the music,** from what Cratarr knows rather than the sound (in the app the music plays natively, out of
  the page's reach, and on another player it plays elsewhere): the transforms turn once per 32 beats at the track's
  tempo (decision 0035; without one, once per 21 s); the brightness follows the track's waveform at the playing
  position, between the player's reports moved on with the frame clock; paused, the turning slows to a stop and the
  picture dims. A new track's flame blends in over six seconds. Reduced motion turns at a quarter of the speed.
- **Load:** 30 frames a second on touch devices, 60 elsewhere; the buffer at most 0.6 megapixels, scaled up to the
  screen. The device's frame times set the work: late frames take passes away (then points: 32k to 256k), quick ones
  add passes back.

## Consequences

- Nothing is downloaded or bundled for it; it works offline with downloads as with anything else.
- On the PC's integrated graphics (Intel UHD) it draws 786,000 points a frame at 57 frames a second. The tablet's GPU
  is far weaker; the load adapts to it, and how it looks there is to be checked on the tablet.
- Without a confident tempo the turning is a steady default. (Beats themselves, which the analysis did not keep at
  first: the amendment below.)
- Some seeds still give a flame that fills the screen with haze; the check is a heuristic and can be tightened.
- The name Electric Sheep is not used in the interface ("Visuals").

## Amendment, 2026-10-17: on the beat

The owner, on the tablet: "My main problem is that the visualizations do not sync with the beat of the song. Fix
that please!"

**Why they did not:** the visuals knew a track's tempo, never where its beats fall. Turning at the tempo has the
beat's speed but no phase: it cannot land on a beat. The loudness came from the waveform, whose 512 bins are about
half a second each in a four-minute track, as long as a beat; nothing in it pulses with the music. The position
between the player's reports was moved on with the frame clock and set back at each report, a jump of up to a frame.

**Now:**

- **The server keeps every beat** (decision 0035, amended the same day): taktus's beat tracker (Ellis 2007's dynamic
  programming: the path through the onsets that lands on the most while keeping its steps even) at the tempo the
  analysis keeps, so a track counted slow pulses on its slow beats, and one without a tempo has none. With each beat
  its strength: the onset there, against the track's strong beats. The page reads them from
  `GET /api/track/{source}/{id}/beats`, for the playing track and the next one.
- **The position to the frame:** in the app, the app's last report (once a second) and the time since it; in a
  browser playing itself, its audio's own clock; from another player, its reports through the server, eased toward
  rather than jumped to. The picture is drawn 50 ms ahead of that: a frame reaches the screen a frame or two after it
  is drawn, and a flash a little early reads as on the beat, a little late as after it.
- **Each beat:** the picture flashes (up to 35 % brighter, after the gamma, so the faint parts show it too) and swells
  (4 % larger about its middle), fading within a fifth of a second (sooner at a fast tempo); a strong beat fully, a
  weak one at about a third. The transforms turn a step on each beat (once around in 32 beats), half of the step in
  its first fifth. Between known beats (before they arrive, a break, no tempo) they turn steadily at the tempo as
  before. Reduced motion: a quarter of the turning, spread evenly, no swell, a softer flash.

**Measured:**

- Click tracks through the whole analysis (decode, onsets, tracker): every beat within 15 ms of its click at 120 BPM,
  after 8 s of silence (no beat in the silence), and through a tempo rising evenly from 120 to 126; an accented click
  comes out stronger.
- The visuals in a browser over a 120 BPM grid: every frame's flash was the one its beat calls for, apart from frames
  within 2 ms of a beat (the lab's own clock); the first frame after a beat came 8 ms after it on average, 17 at most
  (60 frames a second; up to 33 at the tablet's 30).
- The library, once live (preview 54): of 55 tracks on four albums (techno, big beat, two of hip-hop), 46 have beats,
  each grid at its track's tempo within about 1 BPM. On five of them against librosa 1.0's beat tracker: it finds 90 to
  98 % of ours on the techno and big beat tracks (within 70 ms), 78 % on a hip-hop one, 50 % on one where it drifts
  itself (87 % of its beats on a kick, 99 % of ours). Ours come a steady 18 to 20 ms before librosa's and before the
  kicks' onsets (low band, librosa's onset detector). With the 50 ms lead, a flash should come some 20 to 40 ms before
  its kick on the tablet, within what reads as on the beat.

**Consequences:**

- A track's beats are worked out when first asked for, where its analysis has none (every analysis made before this):
  about a second on the NAS, one at a time, then kept. Until they come, the turning is steady.
- About a third of the tracks the tempo was checked on have no confident tempo (half of the ambient ones), so no beats:
  steady turning, as before.
- What the player reports is what the visuals follow. Delay after the player (Bluetooth speakers, an amplifier's own
  processing) is not counted unless the player counts it.
