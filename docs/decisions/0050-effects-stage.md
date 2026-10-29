# 0050: Visuals 65: band envelopes, fewer sliders, an effects stage, Facets

- Status: accepted (the owner, 2026-10-18: "Now, for 65, the Hydra effect layer … So plan for it"; "facets with square
  and X going 65"; "maybe the whole touch interaction should be deferred to 66, since we have sufficient for 65"; on the
  open questions: "Yes, agree with the softness slider and the square or hex lattice working in the fluid only. Now,
  we'd probably do best by putting this under the looks section with the looks dice. I don't care about old snapshots.
  And yes, sync the group properly in 65, all at once"). The sections below on the envelope, Haze and Quality, filled in, are agreed too (the owner,
  2026-10-18: "Agree").
- Date: 2026-10-18
- Builds on: 0046 (the visuals, their routes and settings), 0047 (the fluid), 0048 (the randomisation and its limits);
  0049 (the pulse) is separate

## Context

64 and 64.1 made the visuals follow the music by routes with curves, walk under their set values, and stay watchable
by measured limits. Three things the owner asked for next were queued for 65, in this order:

1. **Band envelopes** (the owner): each band's part lit and present by the sound itself, the attack
   on its hit, then following the band's level while the sound holds. A struck drum flares; a pad glows.
2. **Fewer sliders**: the owner said yes to one Quality in place of Points, Steps and Resolution (FPS stays), one Haze
   in place of Floor and Depth, and Family folded into Colours; no to joining Symmetry and Mirror; later for per-band
   Evolution, Continuity and Lighting.
3. **An effects stage** after Hydra (hydra-synth): its effects ported into the visuals' own shaders, not hydra-synth
   embedded (option b: one renderer, one GL context, the flash meter and the governor kept whole). And **Facets**: the
   blocks the owner partly liked when they were a bug (Mali's mipmaps, fixed in 64), made a deliberate look with a
   square and a hex lattice.

Touch, the first layer one could play by hand, was planned with the stage. The owner moved all of it to 66.

## Decision

65 ships whole (the owner: "all at once"): everything below in one release.

### Band envelopes and Presence

The envelope comes from the hits and the band's level (an attack, a hold while the sound holds, a release), with its
signal row and its limits in the precheck. Presence is in the drawing: a band's part weighs in the
chaos game (its transform's share of the points) by a new **Presence** route column, so a part can come and go with its
sound, not only light up. Lighting is renamed **Linger** (how long a part's light rings on). The fifth band, Presence until now, is renamed **Upper mid** so the
column alone has the name (the owner chose it over renaming the column).

**The envelope**: each band's own, 0 to 1, a new band signal row **Envelope** that any route can take
(by default each band's Envelope into its Presence at 100%; Hits into Lights stays as it is). Followed each frame, the
same in the visuals and in the precheck (one pure step in music.svelte.ts's family, so the governor measures what is
shown):

- **Attack**: on each of the band's hits it jumps to that hit's strength (as the Hits signal takes it, at most 1),
  rising into it over the same lead the Hits signal has where the hits are known ahead, so its top is on the moment.
- **Hold**: after the attack it falls toward what the sound still holds, **Sustain × the band's level now**, and stops
  there; while the sound holds, the envelope holds. Where it is under that (a pad, never struck, swelling in), it rises
  to it over 50 ms.
- **Release**: whenever the envelope is above what the sound holds (after a hit, or as a held sound ends) it falls
  toward it with the band's **Linger** as its time (each hit's light rings exactly as the Hits signal's does today).

So a struck drum flares and is gone as its level drops; a pad, never struck, glows at its level; a struck chord flares,
then holds while it rings. **Defaults:** Sustain 60% (a held sound at full level glows at most at 60% of a full hit's
flare, so the hits still stand out), one setting for all bands; Linger as today's Lighting (Sub 0.2 s to Air
0.08 s). **Limits:** none of its own in the precheck: what it moves is measured where it shows (the flash meter, the
Presence and Lights it drives), and its steps are the hits' own, which the flash rule already counts.

### Fewer sliders

- **Quality** (one ceiling) in place of Points, Steps and Resolution. FPS stays. The device's pace (0046's Load) still
  takes passes and points away under it; Quality is only how far it may go up.
- **Haze** (−100 % to +100 %) in place of Floor and Depth.
- **Family** folded into **Colours**.

Nothing old is converted (the owner: "I don't care about old snapshots"): a snapshot, a device's kept settings or a
group's shared ones from before 65 load as they are, without their Points, Steps, Resolution, Floor, Depth and Family
(Quality and Haze at their defaults) and without routes into Floor or Depth. Loading one must still never fail.

**Haze** is Depth and Floor on one axis, 0 exactly today's defaults (Floor 0, Depth 50 %: gamma 3):

- **Up to +100 %, more haze**: the gamma rises from 3 to 6 (Depth's top today), lifting the faint mist and grain
  around the flame; no floor.
- **Down to −100 %, less haze**: the gamma falls from 3 to 1.5 (Depth's bottom), and a floor comes up from a stray
  speck's (0.002 of the flame's mean) to a fifth of the flame's mean (Floor 75 % today) as a cut to black, leaving the
  bright structure clean. Today's 100 % (the mean itself) is not reached: it blacks out half the flame.
- In numbers, with h from −1 to 1: gamma = 3 · 2^h; floor = none for h ≥ 0, else 0.002 · 500^(0.75 · −h) of the
  flame's mean. Both are today's own curves, so a Haze moves the picture as Depth and Floor did.
- **Its routes** are a level's, as Fluid's (0046's limits): they take Haze toward 0 (neither more nor less haze), as far
  as the sound says, never past what is set, either side. Smoothing stays its own slider. The two ends are to be looked
  at on the lab's screenshots (phone and tablet) before 65 ships: the −100 % floor in particular.

**Quality** is one ceiling, 0 to 100 %, on the same ladder the device's pace walks (steps, resolution,
points), so the two never disagree; **60 % is exactly today's defaults** (2^19 points, 3 steps, the screen's default
resolution of about a megapixel):

- **Points**: 2^(16 + 5q), so 64k at 0, 512k at 60 %, 2M at 100 % (the slider's old range was 16k to 2M; under 64k the
  flame thins to dust, and the pace still halves under the ceiling down to its own least).
- **Resolution**: from 60 % of the screen's default at 0 (the pace's own lowest) up to the default at 60 %, then on to
  the screen's own pixels at 100 % (beyond the default only on a device that has them to give).
- **Steps**: 1 at 0, 3 at 60 %, 6 at 100 %, whole numbers in between.
- Every device starts at 60 %; the pace takes it lower where frames come late (as now: steps first, then resolution,
  then points), and climbs back no higher than Quality. Quality is the device's own, never shared, kept in a snapshot
  as Points, Steps and Resolution were.

### The effects stage

**Two slots.** The **picture** slot comes after the show pass: the picture's colour and shape (kaleidoscope, repeat,
pixelate, posterize, colorama, hue, modulate, and Hydra's feedback of the last picture, scaled and turned). The
**feedback** slot sits in the flame's own loop, where the fluid advects: shape only (scroll, scale, rotate, kaleid,
modulate), since the buffers there hold how many points fell on each pixel and each band's share, not colours. All
three of the loop's buffers are carried alike, as the fluid carries them.

**Chains as data.** An effect chain is a list of effects with their amounts (`[{ fx: 'kaleid', n: 6 }, { fx:
'modulate', by: 'flow', amount: 0.3 }]`), compiled into one shader per slot (the GLSL functions ported from Hydra, AGPL
like Cratarr, credited) and rebuilt only when the chain's shape changes; the amounts are uniforms, so the music moves
them every frame without a rebuild. What an effect modulates by can be the visuals' own: the fluid's flow, the bands'
lights, noise.

**The cost.** With no effect, nothing changes: the show pass draws to the screen as today. With an effect, the show
pass draws into a picture at the buffer's size (at most 0.6 megapixels), and the picture slot draws that onto the
screen. The screen is still drawn once at full size, so the extra cost is one pass at the buffer's size. Before any
effect is designed, what that pass and a chain of five cost is measured on the tablet (Mali G52) and the phone (Mali
G615).

**In the look** (the owner: "under the looks section with the looks dice"). The chain is a setting of the look: in the
panel's Look group, pinnable like the look's other settings, and drawn by Another look (the stars) with the rest of a
look, so also by the dice's full roll (which draws another look); not by the double tap (how it moves only). One
**Effects** route column moves the chain as a whole (each effect's amount scaled by it), so the routes and their curves
drive the effects as they drive the rest; a column per effect would crowd the grid, and can come later if one is
missed.

**Held to the limits.** The flash meter already reads the screen, so it sees the effects with no change. The other
meters (motion, shape and colour churn, zoom pumping) are counted from what the flame does, not from the screen: each
effect reports what it adds (a turning kaleidoscope adds motion, a cycling colorama adds colour churn), so the governor
can hold an effect back as it holds the flame back. The precheck (which draws no picture) runs the same reports.

**The work** (agreed in 64): the stage, the compiler and the reports' plumbing; the effects ported, the devices
measured, the help texts and the effects' limits.

### Facets

Facets are drawn where the blocks were: in the fluid's light, as it is carried along the flow. Each cell of a lattice
takes on one light, softened at its edges by **Facets** (0 %: none, as today; 100 %: hard tiles). The lattice is
**square** or **hex** (**Lattice**). The cells move with the flow, which a mosaic laid over the picture would not; that
is what the owner liked. Without Fluid there is no flow, so Facets does nothing (marked in its help). Both are settings
of the look, as the effects are: in the Look group, pinnable, drawn by Another look. Facets is a route column too.

### The double tap's room, for the whole group

64.1 gave the double tap room up to twice the limits on the device tapped only; the others in its sync group got the
routes and governed them at their own Wild. In 65 the room is the group's (the owner: "sync the group properly"): the
roll shares that it stands, every device in the group governs at Wild's most while it does, and the next roll, an undo,
Reset or a recall on any of them ends it for all. A device alone is a group of one, as before.

### Waves

Slow, giant waves across the screen (the owner asked what they would cost: "65 candidates"). A **Waves** route column
beside Stir, Flow and Swirl (64.1) sets how high they run, as the routes say (Swell's, by default, to be tuned in the
lab); a new section may turn their way.

- **With Fluid**: a broad, slow current added in the fluid's force pass, so the light rides on it as it rides on the
  stirring. It must be one the fluid keeps: each step takes out whatever presses the flow together or pulls it apart
  (the pressure), so a wave that pushes and pulls along its way would be undone; one that rolls across it (shear), or a
  row of slow whirls, stays.
- **Without Fluid** (none made at 0): the picture bent along the wave in the show pass, as the warp already bends it
  along the flow. It moves nothing, only how it shows.
- **The cost**: on Mali a pass costs, not the arithmetic in it (0047: seventeen passes added 4.3 ms a frame on the
  tablet, the grid's size hardly counting; the fluid's whole step 1.2 to 1.7 ms there). Both forms add a few operations
  to passes that already run (about 5,200 cells, or the screen's pixels once), well under 0.1 ms; a pass of their own
  would cost about 0.25 ms on the tablet, so neither gets one.

### Infinite zoom: the tunnel and the dive

The owner tried both on the ThinkBook's preview (fc71f300: render.ts, fluid.ts and a page over
a made-up track) and wants both: "Basically, yes, I want both, so this is serious version 65 material."

- **The tunnel**: the light kept from frame to frame is read a little nearer the middle each frame, so it streams
  outward for ever, as if flying into it; the flame itself keeps its size. Where the fluid flows it is a few
  operations in its light pass; at Fluid 0 a pass of its own in place of the fade (one pass at the buffer's size).
- **The dive**: the camera zooms toward the fixed point of the flame's part that is most nearly a turned, shrunk copy
  of the whole (fitted on the CPU four times a second as the flame moves), by that part's own shrink and turn; a share
  of the points, growing through the loop, is drawn as that part's copy of them, so after one loop the part has become
  the whole again and the zoom never ends. Its cost: the points so drawn go through that part once more in the
  points' pass (to be measured on the tablet before the design is fixed). Its seam: how far the part is from a pure
  turn and shrink (the preview showed 20 % to 39 % on the made-up track's looks) is how much the picture morphs as a
  loop restarts; 65 prefers well-fitting parts and eases the seam.
- **Settings and routes** (the owner: "Yes, that's the idea. These sliders should be routable"): Tunnel and Dive are
  two sliders in Behaviour, their speeds (0: off; both can run at once), and two route columns. Each is a level, as
  Fluid is since 64.1: its slider the most it gives, its column's routes the conditions for reaching it (a build-up
  diving faster, a quiet passage slowing the tunnel); no route, the slider's speed as set. The dice roll them, and
  they pin, as the other behaviour sliders do.
- **Held to the limits**: the zoom-pumping meter is to take a steady zoom as calm, not as pumping; the dive's turn is
  reported to the motion meter as the effects report theirs.

### Not in 65

- **Touch** (the owner: deferred to 66): touch as a signal row; drag stirring the fluid, a tap rippling, a pinch and a
  twist; a play mode behind a hand icon, so the taps the visuals already use (controls, seek, play and pause) stay as
  they are; a phone as an instrument for the salon screen through its sync group. 66 begins with one place for all the
  visuals' pointer handling.
- **A track's character** (the owner, 2026-10-18, "loaded as a 66 candidate": "we could map between the waveform
  analysis and the seed settings … with sufficient rules, we could have a working model"): the music sets the ranges,
  the seed picks inside them. A few numbers per track from its analysis (tempo and drum density, drums against
  sustained sound, bass weight, brightness of the sound, loudness and its range, how much the sections differ, major or
  minor) narrow what the seed may draw: fast and drum-heavy toward quick, flashing, short-lingering; pads toward fluid
  and smooth; minor and slow toward darker, cooler colours (crossmodal correspondences; Palmer et al., PNAS 2013, for
  music and colour). The seed keeps every track's look its own and alike on every device; a track not analysed yet
  falls back to the seed alone; what the rules make passes the precheck and the governor as the dice's recipes do; the
  thumbs, each with its track's character, can tune the rules later. After 65, since the envelopes and Presence change
  what several settings mean. First step: the character of the 11 calibration tracks and what each would get, shown to
  the owner before anything is built.
- **The pulse** (0049): its own decision.

## Consequences

- Nothing changes on screen until a chain is set: with no effect, 65 must draw exactly as 64.1 (to be checked pixel
  for pixel in the lab).
- Every effect needs its meters report before it ships, or the governor cannot hold it.
- Another look now also changes effects and Facets; to keep one, pin it.
- Settings from before 65 lose what 65 removes, unconverted, by the owner's choice.
- The group's shared settings carry one more key: whether a double tap's room stands.
- The panel gains Quality, Haze, the effects' settings, Facets and Lattice, Tunnel and Dive, and six route columns
  (Presence, Effects, Facets, Waves, Tunnel, Dive), and loses Points, Steps, Resolution, Floor, Depth and Family.

## Amendment, 2026-10-18: as built for 65 (the owner: "All in 65, all now")

- **Fewer sliders** as filled in above: Quality (quality.ts `ceilingOf`; 60 % exactly the defaults before), Haze
  (aspects.ts `hazeOf`; one route column in place of Floor and Depth, a level either way from 0), and the Colours as
  the look's Family 0 to 5 (Spectrum, Arc, Two tones, Three drawn with each look; Cover and Fire only by hand, so
  pinned; under them the bands keep Spectrum's hues). Kept settings and snapshots are form 3; what they replaced is
  dropped, not converted.
- **The fifth band is Upper mid**, so the Presence column alone has the name (the owner chose it).
- **The Envelope** (envelope.ts, pure and tested) is the last band signal, into each band's Presence at 100 % by
  default; Lighting is labelled **Linger** (its key stays `lighting`).
- **The effects:** fifteen. Kaleid and Posterize (the reference ports), ten more after Hydra (Repeat, Pixelate,
  Colorama, Hue, Ripple, Stream, Echo, Rotate, Scroll, Saturate), and three after EYESY's modes (the owner: "for effects
  check out EYESY"): Magnify, Rings and Interference, the ideas only, our own shaders (its repository has no licence).
- **Held to the limits, three gaps closed:** the meters did not see Presence (a part's weight moves the points between
  the parts; shape churn now counts the share that moves); the precheck did not count a recipe's effects (effects.ts
  `settingsReport`, the same for the visuals' meters and the precheck); and it did not count a dive's turn (zoom.ts
  `diveTurnOf`, fitted once a look).
- **Calibration** (506 runs: 11 tracks, two places each, Standard over three track looks a track as tracks draw them,
  26 of 66 with effects): Standard typical and most, motion 0.272 and 0.466, shape 0.279 and 0.683 (0.155 and 0.423
  before Presence was seen), colour 0.036 and 0.354, pumping 0.068 and 0.351. By the rule only shape churn's limit
  moves, 0.55 to 0.9; the thumbs' reference values follow. Looks with effects run 5 to 14 % busier than those without.
  52 of 440 recipes refused (motion 46, flash 9).
- **The rating mode's Standard** takes every Behaviour slider's default (64.2's column scales kept the device's).
- **Not done yet:** the stage's cost on the tablet (Mali G52) and the phone (Mali G615), to be measured when the devices
  are reachable (both were off the network). **Not pixel for pixel:** the chaos game's points are random each frame, so
  two runs never match; with no effect set the stage stands aside (render.ts `chainFor` null) and the show pass draws
  to the screen as before, while 65's own defaults (Envelope into Presence, Swell into Waves) change Standard's picture
  on purpose.

## Amendment, 2026-10-18: the engines, their controls and the beat; the governor reaches everything (65, the build)

- **Three engines** (the owner: "a kaleidoscope engine, exyedo engine, and maybe one more … multiple subeffects"),
  each one sub-effect at a time, run in this order: **Kaleidoscope** (Kaleid, Repeat, Rotate, Scroll, Pixelate,
  Magnify), **EYEDO** (Pulse, Burst, Halo, Rings, Interference) and **Hydra** (Echo, Ripple, Stream, Colorama, Hue,
  Saturate, Posterize); the loop takes one of seven shapes. Each chosen effect has **its own controls**, one or two (the
  owner: "one or two of the most relevant controls per chosen subeffect"), at their middle as made. Picture effects
  hear **the beat** (`fx_beat`: where in the beat and the bar, how strong); Pulse, Burst and Halo fire on it.
- **The governor reaches all of it** (an audit; the owner: "Agree! Fix it"; then "no shortcuts no
  concessions"). Every meter's lever now reaches what the meter counts:
  - each effect's amount is held by the flash rule's swing and by the lever of every meter its report counts toward
    (effects.ts `heldBack`), so an effect is held as the flame is, not the flame in its place;
  - Presence slows as shape churn is held (and the points moving between the parts count as shape churn);
  - the Waves' height is held by the motion lever and their motion counted where nothing flows; the zooms' streaming
    out (the Tunnel, the Dive's zoom beside its turn, Echo's) counts as the motion it is (k e-folds a second moves a
    point as k radians a second do);
  - a full roll draws each try's look first and checks the recipe with it, so the look shown is the look checked;
  - the precheck counts what the visuals count (the effects' reports, the dive's turn, the waves, the zooms).
- **Reduced motion** rests every effect that changes over time (Posterize, Hue, Saturate stay), as the zooms rest.
- **The limits stay the flame's own.** With effects drawn into track looks, Standard's most motion doubled (one hip-hop
  look's Echo and Rings); raising the limits to fit would have loosened the governor for the flame too. Instead the
  limits stay a quarter above what the flame itself does (motion 0.6, shape 0.8, colour 0.45, pumping 0.45) and a
  drawn look's effects are drawn within the room left above that at the quietest passage's budget (budget.ts
  `DRAWN_ROOM`; flame.ts). Recalibrated on the final code (506 runs): Standard over drawn looks peaks where the flame
  does (motion 0.466, pumping 0.351), none over a limit; 83 of 440 recipes refused (motion 79: the zooms they roll).
- **The Mali review** (read-only, against our gotchas) found two that would have shown only on
  the devices: pow of a negative in Burst and Halo (NaN, the whole picture blank between beats) and the loop's effects
  running on the track's time (spinning faster through a track, compounding per frame); both fixed, with the review's
  smaller ones. Each effect place runs its own clock, so a change of amount never jumps an angle. The pace no longer
  steps down for the hitch of a new effect program (on the PC each change had cost about 25 s at a lower quality).
- **Measured on the PC** (Edge, CPU slowed four times): the stage 0.29 ms of GPU to
  exist, 0.41 with three effects; the loop's read-back 0.86; the fluid's light carrying a loop effect and Facets 0.56;
  frames held at 55 fps but for a loop effect's few doubled frames. **On the devices still to come:** neither the
  tablet nor the phone was reachable; the same script runs there when they are.

## Amendment, 2026-10-19: the look's own route columns (65.1)

- **Every effect control a route target** (the owner: "I want matrix targets for all effect controls"), and **Lens and
  the other discrete settings** ("Also for lens and other discrete"; "Go" on the design): twenty-one columns at the
  grid's end, in the Look's order, each named for the setting it moves (routes.ts):
  - each engine's and the Loop's **Amount** a level, as Fluid's: its slider the most, its routes the conditions;
  - their two **controls** swings about where their sliders set them, a route's full amount across the whole range
    (stepped controls, as Kaleid's sides, move in steps); a column is named for the chosen effect's control (Sides,
    Spin…), "—" where the effect has none, its routes resting until one has;
  - **Colours, Form, Symmetry, Mirror, Lens, Lattice and each engine's choice** at moments, as Symmetry was: each time
    a route rises past half way the setting moves to another value (the same on every device of a group), a pinned
    one staying. The Colours move only among the look's four families (Cover and Fire are chosen by hand); an engine
    set to None stays off, and one that moves to another effect takes its controls back to their middle. Symmetry's
    moment moves Symmetry alone now that Mirror has its own.
- **Moments are new looks**: a moment was never counted (neither live nor in the precheck), so the governor's gap
  between moments had no meter behind it; the looks-a-minute meter counts each now.
- The effects' reports, the governor's hold on them and the precheck all take the routed Amounts and controls; the
  Look's rows mark where the routes have them now. No default routes go into these columns, and the dice leave them as
  they are (a roll keeps base's routes into them), so nothing changes on screen until the owner routes something.
