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
colors." Then: "Include a button to change the current set of parameters, like a random next
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

The owner: "I also want the overall show to be proportional to the overall loudness. Do you
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

The owner: "I want a set of sliders to modify the visualization in real time. Especially
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

Three problems the owner saw: "seemingly at random big flares fill the entire screen, on some
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

The owner wanted more variety than three transforms of a few favoured variations, with half of them
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

The page's own audio is offered to the live analysis (live.ts) through the levelling's route
(`audioTap` in localPlayer): an element can feed only one, so the route is made at no change of level where levelling
is off; never in Safari or while another player plays.

## Amendment, 2026-10-18: routes, every signal on every aspect

The owner: "For each aspect of the music we can get a signal for, I want to be able to control its
effect on each of the main aspects of the visualization", all of them but stems; amounts from -100 to +100 %, today's
look as the defaults, standard controls, live, kept on the device, a Reset.

**The signals** (each 0..1 now; format 3, decision 0035, drawn 50 ms ahead): of the whole track, the **loudness** as
heard (as the Intensity setting lets it count), the **beat** and the **downbeat** (a pulse at each, gone within a fifth
of a second; a downbeat as surely as the server is of where bars start), the **tempo** (120 BPM is 1), a **section**'s
start (its boundary's strength, fading over a second), **harmony** (how much the chords are moving), **timbre** (the
spectral centroid), the **key** (the chroma's direction on the circle of fifths, followed over two seconds: a full turn
round the wheel), **swell** and **build-up**; of each band, its **level**, **flux**, **hits** (each fading over the
band's Lighting time), **percussive** and **harmonic** parts, **drum hits**, **punch** (crest factor), **width** and
**swell**.

**The aspects:** of each of the six parts (a band's routes act on its own, the whole track's on all six): **Shape**
(larger, its variations given over to its band's own: bubble, eyefish, swirl, horseshoe, cylinder, sinusoidal from sub
to air), **Rotation** (a full amount turns it once round in 16 beats; negative turns it back), **Lights**, **Hue** (a
full amount once round the wheel) and **Spread** (doubled or halved from the setting's); of the whole picture,
**Glow**, **Size**, **Saturation**, **Trails** (each frame keeping more or less of the last) and two moments:
**Reseed** (a new flame, as the dice give) and **Symmetry** (another number of arms, where the setting is Auto), each
as what its routes give rises past a half, again only after it has fallen under a third and two seconds on. Shape, Hue
and Spread follow over the band's Evolution time; Continuity shares a part's motion and evolution between steady and
steps on its band's hits, as before.

Each aspect adds up its routes (a signal times its amount) and moves from its rest by that: the rest is what nothing
driving it shows (half the size, a third of the colour, no lights, no turning). **The loudness is the envelope:** every
other signal's routes are scaled by it, so a quiet passage is a small show whatever its parts do; while paused, only the
loudness's own routes act.

**The defaults are the look before:** Loudness to Size and Saturation +100 % and to Rotation +20 %; Beat to Glow +20 %
and to Size +8 %; each band's Level to Shape +100 %, Rotation +60 % and Hue +8 %, its Hits to Lights +100 %. Every other
route is 0. The Beat and Bands sliders are gone: they were these routes' amounts.

**The grid,** in the panel (now up to 500 px wide; it stays open until it is closed or the picture is tapped): a row a
signal, the whole track's then the bands', a column an aspect, each cell coloured by its amount (green up, pink down;
numbered where there is room); a tap on a cell gives its slider, filling from 0 at its middle, held in sight at the
panel's foot. The bands' rows are the picked band's or all six at once (their mean shown, dashed where they differ; a
change sets all six). A bar under each signal's name shows it now. **Reset** puts the routes back, the other settings
as they are; the panel's own Reset puts back everything. Kept on the device with the other settings (only the routes
there are; a 0 kept turns a default off).

**Until the server has a track's signals,** where the page plays it, the bands come from the sound as it plays
(live.ts, on the levelling's route): level, flux and hits; the rest wait for the server. Where neither (the
app, another player), each band stands at half its level with no hits, and the parts step on the beats, as before.

**Measured** (the real component in a lab page over a made-up format 3 track, 120 BPM, sections every six seconds, on
the PC's Intel graphics): every signal moves its bar; a cell's slider changes what is drawn within a frame (Loudness to
Size at -100 %: the scale from 0.70 to 0.29 in a quiet section) and is kept; Sub's Hits to Lights at 0 darkens the sub's
part alone; Section to Reseed at +100 % gave a new flame at each section's start; the grid fits a 390 px phone (no
sideways scroll) and a 1280 px tablet. The routes' sum costs 3.5 µs a frame with all 704 routes set (0.85 ms to compile
them, on a change only); 56 frames a second at 512k points as before, the panel's grid open adding 0.1 ms of layout a
frame. The live fallback, through the page's own player on a click track with the server's signals answering "not
ready": the sub and bass parts lit on each kick (two a second), the upper bands on each hat (four a second).

The upper three of the six parts had evolved toward no variation of their own (the three bands' three; linear for the
rest) since six parts came in: each has its own now.

## Amendment, 2026-10-18: Flash, the grain, and colours from the cover or a fire

The owner after preview 59: "I have a hard time controlling flashiness. Fix that please. One global slider could help
too?", then "addition to rainbow gradient, I want to have the option for two other color modes, namely album cover and
fire", and the grain controls that were offered; "put it all in and ship it".

**Flash** (0 to 200 %, 100 % the look before): each quickly changing aspect (each part's Lights, the Glow, Size and
Saturation) is its slow course (the value followed twice over 0.3 s) and Flash's share of its swing about that course.
At 0 a passage's change still shows and a beat's or a hit's swing is gone; at 200 % the swing is twice as large. It acts
on whatever the routes give, not on the default routes alone. Flashes were hard to tame because:
- every hit lit at least 0.3 of its light however weak it was: a hit's light is now its strength;
- each light came on in one frame (a short Lighting time only made that a sharper strobe): where the hits and beats are
  known ahead (the server's), a hit's light, a beat's and a downbeat's pulse and a section's mark now rise over the
  40 ms before it, their most still on the moment (a rise after it would have put them behind the beat). The sound
  heard as it plays cannot be got ahead of: its hits still come on at once;
- a Lights route below 0 did nothing once the sum was under 0: Lights now runs from -1 to 3, below 0 dimming its part
  (to 0.4 at -1, with no glow). The six bands' lights do not add up on a pixel: each is weighed by its share of the
  pixel's points, and the shares sum to one.

**The grain** (each 0 to 100 %, a route's full amount moving it half that, and a column of the routes each):
- **Smoothing:** how far the sparse parts are blurred into a haze: the density estimation's widest ring, 12 px at most
  (3, as before, at 50 %); past three pixels the ring reads coarser levels of the buffer, made that frame, so it is a
  haze rather than eight copies of each speck.
- **Floor:** the faintest density shown, from a stray speck's to about the flame's mean; fainter goes black, over a
  span from half of it to twice it so its edge does not crawl. 0 by default.
- **Depth:** the display's gamma, 1.5 to 6 (3, as before, at 50 %): higher lifts more of the faint detail.

**Colours** (Rainbow, Cover, Fire): Rainbow is the seed's own (the four families). Cover gives the bands the playing
track's cover's six colours (palette.ts: k-means in OKLab, each pixel weighted by its chroma), darkest to
the sub and lightest to the air, their lightness brought into 0.58 to 0.86 and their chroma raised a third (up to
0.26), since on black and through each other they went pale; asked for as the track starts (the next one's too), they
join the track's blend, and the seed's own stand in where there is no cover. Fire runs the bands from a deep red at
the sub through orange to a yellow at the air, and the display adds a blackbody's heat by density: the faintest parts a
deep red to black, the densest yellow, white only at the very hottest. Each part has its own lightness for these (a
point's is its bands', by how near its colour is to each). The dice give another flame in every mode, its colours the
mode's.

**Buffers:** 32-bit floats are taken only where they can be filtered as well as blended. A tablet's Mali-G52 blends
them but has no OES_texture_float_linear: its picture was read pixel by pixel and scaled up blocky, grainier, in 59. It
now draws in 16-bit again. (A phone's Mali-G615 has the extension and keeps 32-bit. This paragraph first named the
phone: the debug link had gone to the tablet, the only device then attached.)

**Measured** (the lab page over the made-up track, the PC's Intel graphics, and the tablet off-screen over its debug
link): the sub's light varied by 0.07 (standard deviation) at Flash 0, 0.19 at 100 % and 0.40 at 200 %, its mean alike;
it rose from 40 ms before each hit and was most on it. Real covers: The Low End Theory's reds and greens, Force Of The
Wind's ochres and blues, R.I.P's black and white as greys; no cover, the seed's own. Smoothing's coarser levels, Fire
and Floor drew on the tablet as on the PC, in 16- and 32-bit, through three scrubs that found nothing. The grid's 14
columns fit a 390 px phone.

## Amendment, 2026-10-18: the look is the sliders, a track's preset sets them

The owner: "I'd like our seed, whether random or not, to be a preset on the sliders, instead of the sliders being an
operation on the seed", and three dials for the form rather than one number, and pins.

**The look's settings are the flame:** Form, Detail, Twist, Warp, Symmetry, Mirror, Lens, Spread, Family, Hue,
Lightness, Vividness and Purity. The flame is made from them alone, so moving one moves what is shown and nothing about
the look stays hidden in a seed:
- **Form** (0 to 999): which variations each of the six parts is made of (36 kinds, one to three a part), and each
  part's own difference from the dials below, so six parts are not one. A choice, not a dial: neighbouring numbers are
  unrelated forms.
- **Detail:** how far the parts contract (0.3 to 1 of their size, each part a quarter either side): tight, dense cores
  to large, wispy forms. **Twist:** how far they turn, and their post maps with them: straight arms to spirals.
  **Warp:** how much of each part is its variations rather than plain: gentle to wild.
- **Symmetry** (1 to 8 arms) and **Mirror** (Off, On): no Auto any more; the preset says. **Lens:** the final
  transform, Off or one of twelve warps of the whole picture.
- **Family** (Spectrum, Arc, Two tones, Three), **Hue** (where the colours start), **Lightness** and **Vividness**
  (OKLCH 0.55 to 0.85 and chroma 0.06 to 0.26): the colours, under Rainbow; Cover and Fire colour the parts their own
  way. Spread and Purity as before.

**Presets:** a track's key (or the dice, or a Reseed moment) draws a preset of them, checked as the seeds were (24
draws at most, the first whose flame passes, else the best). It loads into the settings, and the panel shows what is
loaded. A track's preset blends in over six seconds; the dice, a moment, or a choice (Form, Symmetry, Mirror, Lens,
Family, the colour mode) over a second and a half; a dial dragged shows at once.

**Pins:** a look setting the owner moves is pinned, with a pin by its value. Tracks, the dice and the moments leave it
as it is (the check draws the rest around it), until its pin is tapped (back to the preset's own value) or the panel's
Reset (every pin gone, the track's own preset back). A Symmetry moment changes Symmetry and Mirror where they are not
pinned. Kept settings from before carry over: a Symmetry or Mirror that was not Auto, a Spread not 100 % and a Purity not
75 % come in pinned.

How the show behaves stays out of the preset: Points, Steps, Resolution, FPS, Intensity, Flash, Speed, Brightness,
Colours, the grain, each band's times and the routes.

**Measured** (the lab page): a fresh device loads track A's preset, nothing pinned; Detail moved pins it and the flame's
transforms change at once; the dice change all else; track B on the same device keeps Detail; unpinned, Detail goes
back to B's own; Form and Symmetry pinned, then Reset: no pins, B's own; settings kept from 60 (Symmetry 3, Mirror On,
Spread 250 %, Purity 100 %) came in pinned. The look's rows fit a 390 px phone.

## Amendment, 2026-10-18: shared in a sync group, saved looks, moments every device agrees on

The owner: "the visualizations should be shared between the sync play group so that controlling it in one place
controls it in another", and "I want to be able to save a current snapshot and recall it". Their choices, of the
options put to them: everything that shapes the picture is shared; a joining device adopts the group's settings; a
snapshot holds all of it, on the server; recalling one pins what it sets.

**Shared:** the look and its pins, Colours, Intensity, Flash, Speed, the grain, the routes and each band's times. Each
device keeps its own Points, Steps, Resolution, FPS (what it can afford) and Brightness (its room). The group's settings
live on the server, per group, while it lasts (`/api/groups/{group}/visuals`); the page follows the group its player is
in (the app's own, or the one it plays on), while the visuals show.
- Joining: the group's settings are taken; a group with none yet takes the joiner's (a create-only write: where another
  device got there first, the server refuses it, and the page takes that one's).
- Leaving: what shows stays, as kept on the device.
- What the owner changes is sent as it is made, only what changed (the routes route by route), a drag once every
  150 ms; the server merges it and sends it to every device on the live channel. A page knows its own changes when they
  come back (`writer`, a name of its own per load), takes none older than the last it took, and none over a change of
  its own still being sent. A track's preset and the music's moments are not sent: every device works them out alike.
- The moments (a new look, a new symmetry) are seeded from the track and the beat they come on (the half second where
  it has no beats), not at random, so every device in a group makes the same choice at the same moment.

**Snapshots** (`/api/visuals/snapshots`): a row in the panel. Save keeps everything shared, named after the track
playing (with the form of its settings, `v`, for later readers); a tap on one recalls it, every look setting pinned,
and to the group; held (or a right click), one is renamed or deleted.

**Standard** (the owner: "keep a snapshot of the current settings so we don't lose our current production … one
standard, non-removable preset"): always first among them, it cannot be renamed or deleted, and it is data in the page,
not the defaults read at the time, so later defaults leave it as it is. It holds the visuals as preview 62 made them:
each track's own look (it pins nothing: recalled, every pin is let go and the playing track's look is drawn again) and
everything else as it was then. A recall puts a snapshot's routes back whole, any route it does not have turned off,
so routes set since, or defaults added since, do not stay on.

**Measured** (the lab page against a stand-in server and live channel, the page playing on a player in a group): an
empty group got this device's settings; a slider moved sent only itself; another device's change came in, this page's
own echo and an older change did not; a change of its own still being sent stayed over another device's; the dice sent
the look; a route only itself; out of the group nothing more went; a group's settings were taken on joining; the race
to make them lost to another device, its settings taken; a snapshot saved, recalled (its look and Flash back, all 13
look settings pinned, all sent), renamed by a hold, deleted by a right click.


## Amendment, 2026-10-18: the saved looks in a gallery, every slider kept

The owner, on 63: four snapshots all named after the track they were saved on ("Kartlav"); they want proper control:
"one button, a gallery", "when you press the bookmark button, a sheet opens, because that's our standard format of
display"; and "it should save all of the sliders".

- **The bookmark** (top right, beside the settings and Another look) opens the app's standard sheet (lib/MenuSheet,
  raised over the full-screen visuals) with a gallery of small pictures: Save first, Standard second, then the saved
  ones, newest first. The panel's row of them is gone: one place.
- **Save** keeps every slider and a picture of the visuals as they are (160 x 90, a JPEG of the middle of the canvas,
  taken in the frame it is drawn: the canvas keeps nothing after it shows; a few kilobytes), named after the track and
  the time ("Kartlav · 21:04"), the name at once ready to change. A tap brings one back; held, it is renamed or deleted.
- **Every slider:** besides those shared in a sync group, a snapshot keeps the five each device keeps its own (Points,
  Steps, Resolution, FPS, Brightness). Recalled, those come back on the device that recalls it only, never sent to the
  group. Snapshots carry their form (`v`): 1, the shared settings (63's, which leave the device's own as they are);
  2, every slider. Standard holds production's for the five, Resolution and FPS each device's own default.
- The pictures are the server's: a snapshot's optional `picture`, a JPEG or PNG data URL of at most
  48 KB, never part of the settings (so never sent to a group).

Measured (the lab page, a stand-in server): the sheet opened over the visuals and read the saved ones; Save sent 32
settings (v 2) and a 4.8 KB picture as "Lab · 11:45", its name then edited in place; a recall put Flash and Points back,
Points on this device only (nothing of the five sent to the group); one from 63 left Points as they were; Standard gave
Points 19, Steps 3, Resolution and FPS the device's own, Brightness 100 %, nothing pinned; held, one was deleted.

The snapshots on the server: kept by Cratarr in the user data (`visual_snapshot`), per user (the first
administrator's, as Cratarr's own calls act on), newest first: `GET/POST /api/visuals/snapshots`,
`PATCH/DELETE /api/visuals/snapshots/{id}`, each `{ id, name, settings, created, picture? }`. The settings are the page's
JSON, opaque to the server (with the page's `v`), at most 64 KB; names 1 to 200 characters; a PATCH renames, re-pictures
or both. Standard is the page's, not stored.

## Amendment, 2026-10-18: Drift and the Track row, the player a page drives, seeking, the Cratarr lens (64)

- **Drift in place of Reseed** (the owner: "The reseed route target should essentially only affect looks, and it should
  walk proportionally instead of randomizing", "So random direction proportional distance"). The column's routes walk
  the look's smooth settings (Detail, Twist, Warp, Hue, Lightness, Vividness, Spread, Purity, those not pinned) in a
  direction drawn anew each bar from the track and the bar's number (every device of a group walks alike), as far as the
  routed signal reaches, within reach of where the track set them; below 0 they walk back (drift.ts). The
  choices (Form, Symmetry, Mirror, Lens, Family) do not walk. Ten steps a second, none with reduced motion; not saved,
  not sent. Routes kept into Reseed are Drift's.
- **The Track row** (the owner: "Yes, I want it in 64"): a whole-track signal, all of it as a track starts (the visuals
  opening on one too), gone over its first beat. Into Drift it sets how far a new track brings its own look: −100 % all
  of it (the default: as before), −50 % half way (the choices only past half way), 0 none, above 0 away from it. A Reset,
  the dice and a group's look still come whole. A snapshot from before the row recalls with its −100 %. Into any other
  aspect it is a pulse at each new track.
- **Shared with the player a page drives:** the key is the player's group's leader where it is in one, else the
  player's own name (the owner: "My phone is most definitely controlling my salon tablet … So they are in a group to
  some extent"), so a phone's page driving the salon tablet shares the tablet's own page's visuals.
- **Play and pause** between previous and next (the owner: "We are missing the play pause button").
- **Seeking by taps** (the owner: "Double tap is 10 seconds forward or back. Triple tap makes that 30 seconds", "each
  subsequent tap after the 30 seconds should be 30 seconds per tap"): on the right forward, on the left back, each step
  as it comes, how far shown on that side for a moment. A single tap still shows or hides the controls, once no second
  follows (300 ms). The local player seeks at once; one of Cratarr's players is moved by the server from where it is
  (`POST /api/players/{name}/command { "command": "seek", "by_ms" }`); others are not moved.
- **Full screen from the panel** shows the visuals: moving between beside the page and over it no longer goes back a
  step of the history (which the visuals moved to took as their close).
- **The Cratarr lens** (the owner: "the circle with a 60-degree slice on the right side taken out", then "we can actually
  get the Cratarr logo itself in high resolution, including all details"): a Lens drawing the flame into Cratarr's mark,
  the record's rings outside its slice and the thin ones in it (Mark.svelte's radii; the shader's code written from the
  same list). Each point goes into the slice or the rest by its angle, as their areas share, and its distance into their
  rings by area. It takes the whole of the final transform. Presets still draw their lens among the others, so every
  track's own look stays as it was; this one is picked.
- **The dice and the governor:** Another look keeps the look-only dice; the dice draw everything, a look and how it
  behaves, checked against the track first, with an undo. Both, the meters, the rating page and the governor are
  decision 0048's.

Measured (the lab page): a track change at the default gave the next track's own look exactly; at 0 the look stayed; at
−50 % every smooth setting went 0.49 to 0.51 of the way; a +100 % walk reached the tether (0.3) and stayed within it, and
−100 % went home at once. Moved beside, over, beside, over and closed, the visuals stayed open each time and the history
went back once. Taps: "+10 s", "+30 s", "+60 s", "−10 s", "−90 s" as asked, the controls untouched.

## Amendment, 2026-10-18: routes as curves, settings as limits, Behaviour, pins in the matrix, the dice's double tap (64.1)

- **Settings are limits** (the owner: "Wouldn't it be better if the slider would be the limit? … So the sliders become
  the attenuators", then "The other sliders are simply limits"). For the four levels, Smoothing, Floor, Depth and
  Fluid, the slider is the most there is, and each route into one is a condition for reaching it: the value is the
  slider times every route's factor, each between 0 and 1, so it never passes the slider nor goes below 0, in any order
  (a proposal; the owner: "Awesome, so this is feasible"). A route's factor is 1 − e · (its curve's highest
  point − its curve now), e the loudness envelope (quiet, the slider as before); a band signal's six factors combine
  by their geometric mean, as one condition. Flash and Speed already limit their columns' swing and pace.
- **Each route a curve** (the owner: "Instead of the slider, I'm looking for a curve setting", then "the curves are
  only for … the matrix routes", "every route", "one graph for either positive or negative … The color can auto adjust
  based on the value selected"). A cubic from 0 at no signal through two handles to an end at full signal, from −100 %
  to +100 % about a middle line, green above it and pink below; the old amounts are straight curves, so every setup,
  snapshot and group setting reads as before (a route is a number while straight). The curve takes the raw signal, the
  loudness envelope after it. Swings add the curves' values as the amounts were added; levels take them as above.
  Computed from 33-point tables; straight routes keep the multiply.
- **The editor** below the grid in place of the amount slider: the handles, a live dot where the signal is now, and
  behind the curve, dimmer, where the signal has been over the last 30 s of music, mirrored about the middle line like
  a waveform (the owner, after three sketches: "this gives us the best way to tune the curve to the signal"). Shaped
  squares carry a small curve mark.
- **Behaviour** (the owner: "We should get a header for the behavior section"): Intensity, Flash,
  Speed, Smoothing, Floor, Depth and Fluid under a header of their own; each pins by moving it as the Look's do (the
  dice leave a pinned one; a recall pins them; Standard and Reset let go).
- **Look and Behaviour in place of Drift** (the owner: "instead of having drift as a target, we should have looks and
  behavior as a target"): Look is Drift as it was; Behaviour walks the behaviour settings below what is set and back,
  never above (an attenuating walk, held in memory, never saved: a factor in [1 − tether, 1] per setting), the mark under
  each slider showing where it is now. The Track row works on both; its default −100 % on Behaviour starts each track
  from what is set. Routes kept into Drift are Look's.
- **Routes that walk** (the owner: "behavior should also affect those matrix routes that subscribe to it … by limiting
  the fill of cells vertically", "as an attenuating walk"): a route's Walk switch (off by default) puts it on the same
  walk; its square fills only as far as it is.
- **Pins in the matrix** (the owner): shaping a square pins its route (a whole column could be
  pinned too, until 64.2). The dice leave pinned routes and columns as they are; a pinned route still walks if its switch is on. A
  folded corner marks a pinned square.
- **The dice keep the Track row**: a roll put in the recipe's routes whole, and recipes have none, so since 64 a new
  track stopped bringing its own look after the dice.
- **The dice's double tap** (the owner: "Normal tap is the same as now, and double tap becomes this randomize 1 to 5",
  then "double tap could allow beyond limits", "twice the limits"): only how the visuals move, the routes into Shape,
  Lights, Rotation and Size, Speed, Flash and the bands' times (recipe.ts `MOVEMENT`); the look, its colours, the grain
  and smoke and every other route stay. Until the next roll, an undo, a Reset or a recall, the precheck and the governor
  take Wild as 100 % (twice the limits); the flash rule stays.
- **Save over** a snapshot (the owner: "Yes"): its settings and a new picture under its name and in its place.
- **The grid scrolls across** (the owner: with 19 columns a cell was 13 px on a phone): only the grid moves sideways,
  the signals' names and the band chips stay at the left, the columns' names in a row of their own kept at the top of
  the panel and moving with the grid; cells at least 28 px, their numbers at every width.
- **Each band's times pin** (the owner: "pinning the behavior sliders themselves, just like the look slider"): a band's
  Lighting, Evolution and Continuity pin as the Behaviour settings do; the dice leave a pinned one, a recall pins them.
- **The fluid's stirring as routes** (0047, amended): three more columns, Stir, Flow and Swirl.
- **Calibration:** run again on this mapping, Standard measures as in 64 (its limits and the thumbs' reference values
  unchanged); 47 of 440 random recipes refused.

## Amendment, 2026-10-18: Close back on top, the editor over the names, zero, the cells as curves, pins on routes, each column its slider, Full screen leaves (64.2)

- **The top controls back where they were**: 64.1's scrolling grid named its header's first cell `.corner`, the top
  control row's class, so the row became sticky and left-aligned on a dark band over Close, with a strip above it (the
  owner: "I no longer have the close button"). The cell has a name of its own.
- **The curve editor over the grid's sticky names** (the owner: "there is a Z index problem with the sticky label
  columns").
- **Zero** in the editor (the owner: "I want a zero button in the graph editor"): the route to nothing, its square
  empty; as any change to a square, it pins the route.
- **Each square shows its curve**, scaled so its largest swing fills it (the owner: "The curves aren't normalized in the
  grid cells"), in place of the number; it **fills from the bottom as far as its route gives now** (its curve where its
  signal is, times its walk, through the loudness as the routes act), green or pink by direction, moving with the music
  (the owner: "the cell's fill should show the current value").
- **Outlines** (the owner: "our selection highlight could be dashed, while pinned routes get a solid highlight"): the
  picked square dashed, a pinned route solid, **a route that walks** a border of long dashes and short gaps marching at
  the Behaviour walk's pace (the owner: "a walking border, long solid dashes, short empty gaps. Match speed"), drawn
  heavier where it is pinned too.
- **Pins live on routes** (the owner: "A pin lives on a route", "I never asked for a column pin"): 64.1's column pins
  are gone, from the editor, the dice and the settings (one kept is dropped on reading); the look's and the behaviour's
  settings pin on their own sliders.
- **Each column its slider** (the owner: "Are there columns which do not yet have a slider … Or is it impossible to have
  each column their own slider?", then chose where, what and the dice): Shape, Rotation, Lights, Glow, Size,
  Saturation, Trails, Stir, Flow and Swirl, which had none, each get one in Behaviour, every column's slider there in
  the grid's order after Intensity, Flash and Speed (Smoothing, Floor, Depth and Fluid in their places). Each **scales**
  what its column's routes give, like Fluid's: at 100% as before, lower every swing as much smaller in the same shape,
  at 0 the column at rest (Size half, Saturation a third, Glow even, Trails as usual, nothing turned, lit or stirred).
  Pinned when moved, walked by Behaviour as the other sliders are, shared in a group and kept in a snapshot (one saved
  before them recalls each at 100%). Hue, Spread and Symmetry keep the Look's sliders (where the column starts from).
  The Glow column's setting is `glow` (`brightness` is the display's). routes.ts `COLUMN_SCALES`.
- **The dice roll them too** (the owner chose it): each between a half and all of it, so every column still moves,
  drawn after everything else (a seed's routes and other settings are what they were); a double tap rolls the four
  movement columns' (Shape, Lights, Rotation, Size); a pinned one stays.
- **Calibration** run again: Standard (every scale at 100%) measures exactly as in 64.1, so the governor's limits and
  the thumbs' reference values are unchanged; the dice's recipes are calmer by a median 12 to 24% per meter, and 7 of
  440 are refused (64.1: 47).
- **Full screen leaves too** (the owner: "Our full screen mode has no exit mode, so it can become stuck"): the player's
  and the visuals' Full screen button stayed only until full screen, after which only the browser's Esc or back left
  it; it now turns into Exit full screen. (The visuals showcase site, which hid Close, was stuck that way; it has
  its own way out now.) The Cratarr app's view offers no full screen (its WebView has none), so none of it shows there.

## Amendment, 2026-10-18: the dice roll the columns' sliders from three quarters (64.3)

- 64.2's dice rolled each column's slider from a half to all of it, and came out calmer than 64.1's by a median 12 to
  24% per meter. Asked whether to keep 64.1's liveliness by rolling from three quarters, the owner: "Yes". Now 5 to 12%
  calmer than 64.1, 26 of 440 recipes refused (64.1: 47; 64.2: 7); Standard measures as before.
