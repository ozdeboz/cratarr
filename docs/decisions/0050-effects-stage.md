# 0050: Visuals 65: band envelopes, fewer sliders, an effects stage, Facets

- Status: accepted (the owner, 2026-10-18: "Now, for 65, the Hydra effect layer … So plan for it"; "facets with square
  and X going 65"; "maybe the whole touch interaction should be deferred to 66, since we have sufficient for 65"; on the
  open questions: "Yes, agree with the softness slider and the square or hex lattice working in the fluid only. Now,
  we'd probably do best by putting this under the looks section with the looks dice. I don't care about old snapshots.
  And yes, sync the group properly in 65, all at once"). claude-5f's two sections below are still to be filled in.
- Date: 2026-10-18
- Builds on: 0046 (the visuals, their routes and settings), 0047 (the fluid), 0048 (the randomisation and its limits);
  0049 (the pulse) is separate

## Context

64 and 64.1 made the visuals follow the music by routes with curves, walk under their set values, and stay watchable
by measured limits. Three things the owner asked for next were queued for 65, in this order:

1. **Band envelopes** (the owner, through claude-5f): each band's part lit and present by the sound itself, the attack
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

claude-5f makes the envelope (from the hits and the band's level: an attack, a hold while the sound holds, a release),
its signal row, and its limits in the precheck. claude-a5 makes Presence in the drawing: a band's part weighs in the
chaos game (its transform's share of the points) by a new **Presence** route column, so a part can come and go with its
sound, not only light up. Lighting is renamed **Linger** (how long a part's light rings on).

*claude-5f to fill in: the envelope's definition and its defaults.*

### Fewer sliders

- **Quality** (one ceiling) in place of Points, Steps and Resolution. FPS stays. The device's pace (0046's Load) still
  takes passes and points away under it; Quality is only how far it may go up.
- **Haze** (−100 % to +100 %) in place of Floor and Depth.
- **Family** folded into **Colours**.

Nothing old is converted (the owner: "I don't care about old snapshots"): a snapshot, a device's kept settings or a
group's shared ones from before 65 load as they are, without their Points, Steps, Resolution, Floor, Depth and Family
(Quality and Haze at their defaults) and without routes into Floor or Depth. Loading one must still never fail.

*claude-5f (who brought the owner's list) to fill in: how Haze maps onto the floor and the gamma, and how Quality
divides into points, steps and resolution.*

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
effect is designed, claude-5f measures what that pass and a chain of five cost on the tablet (Mali G52) and the phone
(Mali G615).

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

**Who does what** (agreed in 64): claude-a5 builds the stage, the compiler and the reports' plumbing; claude-5f ports
the effects, measures the devices, and writes the help texts and the effects' limits.

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

The owner tried both on the ThinkBook's preview (branch a5/zoom-preview, fc71f300: render.ts, fluid.ts and a page over
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
- **Settings and routes** (proposed; to settle with the owner before building): Tunnel and Dive as two speeds (0:
  off; they can run together), each with a route column, so the music can quicken them (a build-up diving faster).
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
