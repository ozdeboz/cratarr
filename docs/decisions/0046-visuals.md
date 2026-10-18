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

## Amendment, 2026-10-17: the sound in three bands, and the dice

The owner, after the beats: "I want you to couple transients in different frequency bands to different colors. I also
want you to couple the loudness of the different frequency bands to evolutionary parameters of their respective
colors." Then, in claude-5f's session: "Include a button to change the current set of parameters, like a random next
visualization switch."

**The bands** (decision 0035, amended the same day): the server hears each track in three bands, low (below 150 Hz:
kick drum, bass), middle (300 Hz to 2.5 kHz: snare, voices, most instruments) and high (above 5 kHz: hats, cymbals),
through filters falling 24 dB an octave at each edge, at the file's own rate. For each band it keeps the loudness every
50 ms (in dB, over the 30 dB under the band's own loudest in the track, so a quiet band moves as much as a loud one)
and its transients: where its level rises by 6 dB or more within 20 ms, more than the rises around it, while the band
is not quiet, one per 120 ms (low), 90 (middle) or 60 (high), each with its strength. Worked out in the same decode as
the beats, only when the visuals first ask: about 20 to 30 KB a track, not kept for the whole library.

**A colour for each band:** a flame now always has three transforms (and a symmetry's rotations), one for each band,
low to high, each its own stretch of the palette as before. Beside the colours, the drawing counts how much of each
band's colour a pixel's points are (by how near a point's colour is to each band's), so:

- **a band's transient lights its own colour:** the points of that colour brighten (by up to 60 %) and glow in it,
  fading over 0.18 s (low), 0.14 (middle) or 0.09 (high). The kick lights one colour, the snare another, the hats the
  third. The beat's own flash is smaller when the bands are known (10 % against 35 %); the swell stays.
- **a band's loudness evolves its own transform**, followed over a quarter of a second: it turns further on each beat
  (0.4 to 1.6 times the beat's step), its linear part grows (0.94 to 1.06 of its size), its variations give way to its
  band's own (up to 35 %: bubble for the low band, round and heavy; swirl for the middle; sinusoidal for the high, a
  fine ripple), and its colour moves along the palette (by up to 0.04 either way). Reduced motion: less morphing, softer
  lights.

**The dice:** beside the close button, top right, showing and fading with it. A press blends in a flame from a random
seed over 1.5 s; another press, another one; the next track brings back its own.

**Measured:**

- On test sounds through the server's code: a tone at 60 Hz, 1 kHz and 9 kHz each reaches its own band's top and stays
  at or under 40 of 255 in the others, at 44.1 and 96 kHz. A kick (55 Hz), a snare (900 and 1300 Hz) and hats (7.5 to
  11 kHz) at 120 BPM: each band's transients are within 15 ms of its own hits, 90 % or more of them found, none from the
  others'. Silence: quiet bands, no transients.
- A new track whose beats and bands the page asks for at once is decoded once; a track with beats kept but no bands is
  decoded for its bands alone and its beats stay byte for byte.
- In a browser over a test track: each band's light matched its own transients frame by frame (apart from frames within
  2 ms of one); 52 to 56 frames a second at 12 passes over 131,000 points on the PC's integrated graphics. The second
  buffer doubles what each point writes; the load adapts to the tablet.

**Consequences:**

- Every track's flame is new (three transforms each, where it was two to four).
- What the visuals draw from the sound is still only what the server worked out; a track is decoded once (about a
  second) the first time the visuals show it.

## Amendment, 2026-10-17: as intense as the music is loud

The owner (in claude-5f's session): "I also want the overall show to be proportional to the overall loudness. Do you
think we've sufficiently implemented that now? Probably not. So fix it in the next version." It was not: the loudness
(the waveform, against the track's own loudest moment) set only the brightness, between 60 % and 110 %; silence still
showed at 60 %, a quiet track looked as intense as a loud one, and the bands, each against its own loudest, kept a soft
passage with busy hats looking busy.

**Now one intensity drives the whole show:** the loudness as heard. The server keeps EBU R128's momentary loudness
(400 ms) every tenth of a second with the bands (their format 2; a format 1 kept since preview 55 counts as not worked
out and is worked out again; the page asks with `?v=2`, past a browser's month-old copy). Where the player levels
(decision 0035: the app's setting in the app, the page's own in a browser), the page adds the gain it plays the track
at (its track gain, never above full nor past its peak). From -45 LUFS (and silence) to -10, in equal steps of dB, the
intensity runs from nothing to all, followed over a fifth of a second, and scales:

- the brightness, after the gamma, from 3 % to all: near dark in silence and a fade;
- the size the flame is drawn at, from 80 % to all (in the drawing, so its edges stay whole);
- the turning, from a tenth to all: near still in silence;
- the beat's flash and swell, and the bands' lights;
- the bands' loudness, which now acts within it: a soft passage evolves the flame softly, however busy its hats.

**Measured** (a browser over test tracks): a ramp from -60 LUFS rising 2.5 dB a second shows at 3 % up to -45, 32 % at
-35, 56 % at -25, 88 % at -15, all at -8. A quiet track at -28 LUFS shows at 51 %, levelled or not (its gain of +10 dB
is never applied above full); a loud master at -9 LUFS at all unlevelled, 78 % levelled (its -9 dB bring it to -18).

## Amendment, 2026-10-17: the settings panel, and what a frame costs

The owner (in claude-5f's session): "I want a set of sliders to modify the visualization in real time. Especially
important are a few sliders with which we can tune down the compute that is required"; then "some performance problems,
and I'm currently running it on a laptop with a powerful GPU. And I want sliders, not presets. Make the controls look
standard. And on top of the sliders that you already proposed, I also want to have a slider for maximum brightness."

**The panel:** a sliders button beside the dice opens it over the lower part of the screen; it shows and fades with the
other controls (eight seconds after the last touch while open). Native range inputs in the app's accent, as in
Settings: a one-word label, the slider, its value. Every change shows at once and is kept on the device
(`cratarr-visuals` in the browser's storage, so a phone and the tablet keep their own); Reset brings back the defaults.
Under the sliders, the frames a second the device reaches and the points and steps it draws.

| Setting | What it is | Range | Default |
|---|---|---|---|
| Points | the most points the chaos game runs | 16k to 2M | 512k |
| Steps | the most steps of every point drawn a frame | 1 to 16 | 3 |
| Resolution | the drawing's size against the screen's own pixels | 25 to 100 % | about a megapixel |
| FPS | the most frames a second | 10 to 60 | 30 on touch screens, 60 elsewhere |
| Intensity | how much the loudness sets the show's intensity | 0 to 100 % | 100 % |
| Speed | the turning | 0 to 200 % | 100 % |
| Beat | a beat's flash and swell | 0 to 200 % | 100 % |
| Bands | the bands' lights | 0 to 200 % | 100 % |
| Brightness | the most brightness shown, applied last (no flash goes past it) | 10 to 100 % | 100 % |
| Lighting (sub, mid, treble) | how long a band's light rings after its transient | 20 ms to 1 s | 180, 140, 90 ms |
| Evolution (sub, mid, treble) | how long a band's loudness takes to evolve its transform | 20 ms to 2 s | 250 ms each |

Points and Steps are ceilings: the device's own pace still steps them down when it falls behind. The owner asked for the
bands' times while this was being built ("band dependent time sliders … one for evolution, and one for lighting … so
sub-evolution, sub-lighting, mid-evolution, mid-lighting, and so on"): a grid of three rows (sub, mid, treble) by two
(lighting, evolution), on a doubling scale; their defaults are the times the bands had. Checked in the browser: each
band's light fades over its own time (sub at 500 ms and treble at 30 ms when set so, in step with the expected fade
frame by frame).

**What a frame costs**, measured on this project's laptop (2560 x 1600 screen; its Intel UHD and its RTX 4060, through
ANGLE on Direct3D 11 as Chrome, Edge and Brave draw on Windows), each frame waited for (a pixel read back):

- The points are the cost: about 10 ns a point a step on the Intel, 2.5 on the RTX. Today's top (128k points, 12
  steps) took 22 ms on the Intel (no 60 frames a second), 5.2 on the RTX.
- The display pass drew every pixel of the screen (up to twice the CSS pixels): 5.5 ms of the Intel's 22 at
  2560 x 1600, against 1 at half a megapixel; the buffer the points fall into barely matters (21 to 25 ms from 0.3 to
  2.4 MP). The drawing is now about a megapixel by default (the Resolution setting), the screen scaling it up; the buffer
  is still at most 0.6 MP.
- Each step is a transform feedback, which ANGLE on Direct3D 11 flushes after (the owner's laptop reports it): the same
  1.57 M points a frame in 3 steps rather than 12 took 4.0 ms against 4.8 on the RTX (the Intel the same). The defaults
  are now 512k points in 3 steps.
- 16-bit float buffers are 4 to 20 % quicker than 32-bit ones and half their memory; they are now the default wherever
  a browser draws into them.
- **WebGL's `powerPreference` does not choose the GPU in Chrome or Edge on Windows:** the browser draws on the one
  Windows gives it when it starts (its per-app graphics setting), whatever a page asks. It is asked for all the same
  (other systems heed it). The owner's laptop already draws on its GTX 1650 Max-Q.
- **The step-down had a fault:** after its judgement, the count of quick frames went on, and every frame after added a
  step back: from one step to the most within a second, then behind again, over and over (the load swung rather than
  settled). Each window is now judged once and the next starts afresh; it is judged every second (or 30 frames), and a
  device far behind (frames twice as long as they should be) gives up half its steps at once.
- **Frames are paced to the display:** a frame is drawn every so many of the display's (measured), as many as keep it at
  or under the FPS setting; a rate that is no whole part of the display's alternated between two frame lengths.

**Measured in the browser** (the real component, the Intel, 1280 x 800 at twice the pixels): the defaults hold 56 frames a
second (the most the headless browser gives) at 512k x 3 into 1386 x 866; the lowest (16k x 1, 25 %) the same; the
highest asked (2M x 16 at the whole screen) starts at 3.6 a second and has stepped itself down to 2M x 1 at 45 a second
within five seconds.

## Amendment, 2026-10-18: steady or in steps, less of a show rather than darker, the panel at the right

The owner: "Per band, I want a slider for how continuous versus discrete the animations are. And I'm also not happy
with the way the entire screen dims when there is little loudness. It's a very shitty way to achieve what I want, which
is less show on less loudness." And: "I want the sliders to show in a panel on the right side instead of the centered
sheet from the bottom."

**Continuity, a band's own** (sub, mid, treble; 0 to 100 %, 50 % by default): it shares the band's motion and evolution
between steady and steps. Steady, its transform turns at its speed and evolves as its loudness goes; in steps, it turns
on the band's own transients (the beats, while those are not known), each step eased in over 80 ms and as large as the
turning it stands for (the band's speed over its transients a second, on average; a strong transient further, a weak
one less), and evolves to the band's loudness at that transient, held until the next. The beats' own steps for every
band, as before, are gone: the sub band at 0 % steps on the kicks. Reduced motion: steady. Measured in the browser (a
kick, snare and hats at 120 BPM): at 0 %, 76 % of each band's turning comes in the 100 ms after its own transients
(10 to 20 % would, steady); at 100 %, as steady; at 50 %, between (48 % for the snare); the turning over the run the
same either way (3.4 and 3.7 radians).

**Less show, at full brightness:** the loudness no longer dims the picture (only pausing does). In silence or a fade
the flame is drawn at half its size, with a third of its colour (towards its own grey), still (a tenth of the turning),
unlit (no flash, swell or band lights) and plain (its bands' evolution follows the loudness too); all of it comes back
with the loudness, linearly in dB from -45 to -10 LUFS as before. Measured: -55 LUFS, 50 % size and 35 % colour;
-35, 65 % and 54 %; -15, 93 % and 91 %; -8, all; the brightness the same throughout.

**The panel** is at the right, below the buttons, from top to bottom (340 pixels wide, nearly all a phone's width), and
scrolls; each band its own group: Lighting, Evolution, Continuity.

## Amendment, 2026-10-18: flares, specks and colours

Three problems the owner saw (with claude-5f): "seemingly at random big flares fill the entire screen, on some
patterns"; "less congruence between the colours and the bands than I would wish, another slider maybe"; black specks
appearing after playing a while on the MSI (GTX 1650), "maybe since 56", never on the tablet.

**Measured first** (the real component in a lab page, steady test music, on the RTX 4060 through ANGLE on Direct3D 11,
as Chrome draws on the MSI; three minutes each in 16-bit and 32-bit buffers, both scanned every 30 s):

- **The flares are the flame's geometry:** measured on the CPU each second as drawn, the flame's extent (the frame that
  holds 96 % of its points) went from 0.63 to 3.1 times the frame it was given, as its transforms turned; the screen's
  brightness followed it (correlation 0.72), not the music.
- **The specks:** no value not a number or infinite in either kind of buffer on the RTX. In 32-bit ones the densest
  pixels reach some 60,000 points, near a 16-bit float's largest (65,504); in 16-bit ones they stop at 2048, as rounding
  to nearest makes them. A GPU whose half-float blending rounds up (Direct3D allows it) carries them on into infinity,
  which a fade only multiplies: black for good. That fits all three clues: the 16-bit default came with 56; the MSI's
  GPU; the tablet's (16-bit, rounding to nearest) never. Not reproduced here, so made impossible rather than chased.

**Now:**

- **Framing and exposure follow the flame:** twice a second its points are measured on the CPU (6,000 steps) and the
  camera follows their middle and size over a second and a half; the display's exposure follows how much of the frame
  they cover (a flame spread over most of it shown dimmer, one gathered into a few cells brighter, against a third),
  so its shape's changes do not show as light. Every turning transform's linear part is kept from stretching anything
  more than 0.95 (its larger singular value), so turning and growing never carry it through a shape that flies apart.
  Measured with the lights off: the extent 0.67 to 1.34 of the frame, the screen's brightness within 0.23 to 0.39 and
  never near white; with the lights on the brightness no longer follows the extent (correlation 0.33).
- **32-bit buffers again wherever a GPU can blend into them** (16-bit only where not, as on the tablet). Every two to
  four seconds both buffers are copied through a pass that turns anything not a number into nothing and anything
  infinite into the most a 16-bit float holds; the display reads every sample through the same test; points' places and
  colours are tested from their bits, not with isnan (which a compiler may assume away). Should the scrub find any, the
  panel's figure says how many times ("· 3 cleared"), which on the MSI would confirm the cause.
- **Colours:** each of the flame's band transforms has its own hue, evenly round the colour wheel from a random start,
  at one lightness and chroma (OKLCH, the perceptual scale). A point's colour is a place in the colour plane, not on a
  line as before: one moving between two bands' colours goes paler on the way, never through a third band's, as it did.
  **Purity** (0 to 100 %, 50 % by default, in the panel's look): how far each band's transform takes a point's colour
  to its own; at 100 % each band's part of the flame is its colour alone.
- **The step-down holds the FPS setting** rather than settling under it (the tablet settled at 24 to 25 of 30): a frame
  more than 8 % late for its slot counts late; more than a fifth of a second's frames late takes a step down (steps,
  then the resolution down to 60 % of the setting's, then points); a step back up only after seconds in a row with no
  late frame (3 at first, doubled whenever a step up is taken back within five seconds, up to a minute). New points
  take 30 steps unseen (was 20).

## Amendment, 2026-10-18: more kinds of flame, Spread, Symmetry and Mirror

The owner (with claude-5f) wanted more variety than three transforms of a few favoured variations, with half of them
symmetric and one palette recipe, and settings for what makes a flame's "centres" (its vortices): their spread and
symmetry.

- **Six parts,** one for each of the music's six bands (format 3; a server's three bands each drive two), every one its
  own colour (the owner: "separating the bands is the point").
- **36 of the paper's variations,** picked nearly evenly (the plainest three times as often as the rest, the ones that
  tear the picture apart or scatter it into dust a third): waves, popcorn, rings, fan, noise, blur, arch, square, rays,
  blade, secant2 and twintrian added to the 24.
- **A post map** (flam3's "post": a turn and a little scaling after the variations) on a third of the parts, which bends
  their shapes round; **a final transform** on three flames in ten (a warp of the whole picture as it is drawn: a sphere,
  a swirl, a lens, a heart; one of twelve, mixed with the plain picture).
- **Symmetry:** a third none, two fifths rotations (2 to 8 arms), the rest mirrored too (dihedral, 1 to 6).
- **Colour families,** every one keeping the bands apart: the spectrum (six hues evenly round), an arc (a third to over
  half of the wheel), two tones (across the wheel, the bands taking them in turn, near and far), three (each for two
  neighbouring bands). Purity's default is 75 %: six parts blending go grey.
- **The check before a flame is shown is looser** (from 4 to 70 % of its frame covered; a score of 0.5 enough), not for
  haze, which stays as strict: it is grain however long the flame is drawn.
- **Spread** (25 to 250 %): the parts' offsets from the middle scaled, together into one shape or apart into clusters.
  **Symmetry** (Auto, the seed's own, or 1 to 8 arms) and **Mirror** (Auto, Off, On): a change blends in over a second
  and a half, as the dice do. The auto-framing keeps any of them on the screen.

**What it costs:** on the PC's Intel graphics, 512k points in 3 steps (each frame waited for): 21 ms a frame for a flame
without a final transform, 23 to 27 with one, against 17 to 20 before (the six bands' shares, a third buffer, and the
final transform's warp: given a switch of its own twelve rather than all 36, which had cost it 33). The step-down takes
it: the Intel holds 57 frames a second at 512k points in one step, at 85 % of the default resolution. The bands' shares
are kept in 16-bit floats beside 32-bit colours (they are only ever weighed against the count). And the step-down
learnt the display's frame interval from every frame: a device behind on all of them calls back as slowly as it draws,
which made every frame look on time; it now learns it from quick frames only.

The page's own audio is offered to the live analysis (claude-5f's live.ts) through the levelling's route
(`audioTap` in localPlayer): an element can feed only one, so the route is made at no change of level where levelling
is off; never in Safari or while another player plays.
