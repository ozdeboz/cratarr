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
- Without a confident tempo the turning is a steady default; beats themselves (their times, not only the tempo) would
  let it pulse on them, which the analysis does not keep yet.
- Some seeds still give a flame that fills the screen with haze; the check is a heuristic and can be tightened.
- The name Electric Sheep is not used in the interface ("Visuals").
