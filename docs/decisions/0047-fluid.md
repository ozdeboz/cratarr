# 0047: Visuals: the flame's light in a fluid

- Status: accepted (the owner, 2026-10-18: "our flames appear to me much like orbital physics. It's very organic, but
  it's also somewhat predictable. Now, another way of having such dancing movement is through fluid dynamics", then,
  of the proposal below, "I want it in the next version too, but only if we can keep it development properly separate")
- Date: 2026-10-18
- Builds on: 0046 (the visuals: each track's own fractal flame, its routes and settings)

## Context

A fractal flame is a chaos game: points keep jumping between a few maps, so the picture is an attractor, and however
the maps are turned and evolved it moves like orbits round a fixed shape: organic, but its motions come round again.
A fluid's motion does not: its whirls meet, split and fold, and never repeat. Four ways of bringing one in were weighed:

1. **A fluid layer of its own** behind or over the flame: simple, but two pictures on one screen, not one.
2. **The flame's trails carried by the fluid**: its light becomes the fluid's ink, drifting, curling and folding where
   it would only have faded. One picture; the fluid can run on a coarse grid, so it costs little.
3. **The fluid bending the picture as it is drawn** (its velocity as a warp): the flame as if seen through water, its
   shape kept.
4. **A swirling field without physics** (curl noise): organic and nearly free, but it does not answer back as a fluid.

## Decision

2, with 3 as an option: the light the flame draws is carried by a flow the music stirs, in place of the fade; the
flow can also bend the picture as it is shown. One setting, Fluid, from none (the visuals exactly as before: nothing
of the fluid is made or run) to all; it is shared by a sync group and kept in snapshots like the others, and a route
column can drive it.

**The fluid** (web/src/lib/visuals/fluid.ts) is Stam's stable fluids ("Real-Time Fluid Dynamics for Games", 2003; GPU
Gems 1, ch. 38) on a grid of its own, about 95 x 55 cells shaped as the picture: each step the forces and vorticity
confinement (Fedkiw, Stam and Jensen, 2001: the small whirls numerical damping takes away, given back), six Jacobi
iterations of the pressure (the velocity's divergence worked out in each; the edges are walls), then the pressure's
gradient taken away and the flow carried along by itself; then the light carried along by the flow at the picture's
own size, as much of it kept as the fade would have kept. Written from the papers; nothing is copied.

It is written for the devices Cratarr's visuals run on, with what the Mali GPUs taught (0046, 2026-10-18): 16-bit float
textures throughout (a Mali-G52 cannot filter 32-bit ones), no array constructors in a shader, a value's bits only
saying whether it is a number, never `isnan`; and as few passes as possible, since on a Mali each pass (a flush of
its tiles) cost more than the sums in it: seventeen passes added 4.3 ms a frame on the tablet, the grid's size hardly
counting, the eight of a step now 1.2 to 1.7 ms (Mali-G52, 862 x 501, against the fade, 2026-10-18; the phone's
Mali-G615 3.0 ms with its page in the background; Intel UHD 0.3 ms). The fluid's own budget was 3 ms on the tablet.

**Kept apart in its making**, as the owner asked: the fluid is one file of its own, written and tested on its own;
the renderer calls it through a few methods (the seam below) and the visuals turn the music into its forces, each
side changed by its own hand only; Fluid at none leaves the drawing exactly as it was.

### The seam

`Fluid.supported(gl)`; `new Fluid(gl, gridW, gridH)` (throws where it cannot run: the visuals then offer no Fluid)
with `gridFor(width, height)`; per frame `push(forces)` (where, which way, how far, how hard: up to sixteen), `step(dt,
{ energy, swirl })` (how long the motion lasts; how strongly its whirls are kept), then `advect(light, dst, keep,
width, height)` in place of the fade (all three of the renderer's targets, into its other set); `velocity` (cells a
second, `uvPerCell` to texture coordinates) for the warp; `resize`, `dispose`.

**The renderer's side** (render.ts): the fluid is made the first time the Fluid setting asks for it (nothing of it
before, and nothing where it cannot be made: the setting then does nothing), resized with the buffer, and let go after
ten seconds unasked (so a route passing through none does not make it again and again). A frame with Fluid above none
pushes, steps, and advects the light into the other set of buffers (the one the scrub copies into) and draws the points
on there; the display reads the picture back along the flow by `warp` seconds of it (0.12 at Fluid 100 %). At none the
fade runs as before, and the display does not sample the flow.

**The music's side** (Visuals.svelte): Fluid is a setting of the show (0 to 100 %, none by default, shared in a sync
group, none in the Standard snapshot) and a route column (a route's full amount moving it half its range, as the
grain's). Four times a second, while it flows, where each band's part lies on the screen is measured on the CPU (the
middle of the points its transform takes, its lens taken, through the renderer's camera: flame.ts `partsAt`). Each
frame each part is pushed there by its Lights (its hits, by default; through the routes, whatever lights it), outward
from the middle and round the way the part turns, harder with Fluid; a beat pushes out from the middle at four points;
the loudness as heard is the flow's energy (a quiet passage settles, a loud one flows on), the swells add whirls.
Measured in the lab: at none nothing made and the picture as before; at 30 % the flame keeps its shape, curling where
its parts are lit and settling in a quiet section; at 100 % the light is smoke.

## Consequences

- The visuals gain a second kind of motion that answers the music without repeating; at none they are as before.
- About 1.5 ms of a tablet's frame while it flows (the step-down of 0046 makes room for it as for anything else).
- Its brightness changes are the picture's own (it writes only into the renderer's buffers), so the flash limits
  planned for the visuals measure them with the rest.
- A third way of drawing to keep in step with the Mali rules; its own lab (a page of three blobs stirred by three
  turning forces) shows a change at a glance.

## Amendment, 2026-10-18: a finer scale, Speed, stirred in time, and no blocks (64)

- **A finer scale** (the owner: "between 0 and 5% fluid dynamics is about all I want … or maybe 10% should be 100%"):
  Fluid's whole range is a tenth of the fluid's own (`FLUID_FULL`); a Fluid kept or saved in 63 is carried over.
- **At the Speed setting's pace** (the owner: "speed should scale the fluid's pace as well"), and the governor's.
- **Stirred in time** (stirring.ts; the owner asked how the fluid could follow the beat the way the flames
  do, with no new slider): each band's hits kick its part, 60 ms ahead where the server knows them (the music says how
  soon each band's next hit comes); the beats, downbeats and sections kick from the middle; steady bands stir with
  their level; the flow breathes with the pulse. With reduced motion only the steady stirring.
- **No blocks on Mali:** the light is carried at its own level (`textureLod(…, 0)`). The light's coarser levels, made
  for a wide smoothing and stale otherwise, were blended in wherever the flow squeezed, stepping at every cell's edge
  and piling up frame on frame into square blocks: the phone's G615 blends 7 % of the next level for a 5 % squeeze, 26 %
  for 20 % (measured off-screen); desktop drivers round small levels away, so the labs never showed it. The owner liked
  the look a little: Facets, square and hex, will bring it back deliberately (65).

## Amendment, 2026-10-18: 64's range kept, the stirring as routes, and eased in and out (64.1)

- **The range stays 64's** (`FLUID_FULL` 0.1). The owner's phone washed out in cycles: a route took Fluid to about 35 %
  of 64's range at each punch in the presence band ("without that +60%, the effect is gone"). Swept on the phone's own
  settings, the busy centre's contrast fell from about a fifth of the range up (its p10–p90 spread about 0.35 at 10 %,
  0.1 to 0.2 from 30 %, noisy over the test track's loud and quiet sections), and a fifth was tried. The owner kept
  64's: "even when I have the fluid on max … max fluid looks great", "restore 64's full range, otherwise it'll probably
  not be good". The phone's case is fixed where it came from: the slider is now the most a route gives Fluid (0046,
  amended), so no route takes it past what is set.
- **The stirring as routes** (the owner: "Yes, but I do want it in 64.1"): what was built into
  stirring.ts (each band's hits, the beats, downbeats, sections, the bands' levels, the loudness, the swell) is three
  route columns, Stir (the pushes), Flow (the flow's energy and breathing) and Swirl (the whirls), every default route
  straight at 1, so the defaults stir exactly as before (a test compares every force frame by frame). Across a
  curve is the push's own strength; loudness does not scale them. The dice leave them as they are, as the Track row.
  Tempo's hold on the turning and loudness through Intensity stay built in, said in Speed's and Intensity's help.
- **Eased in and out** (the owner: "Yes, fix it now"): Fluid tipping across 0 made the flow stop dead and
  the smeared light fall apart into specks; the renderer now follows Fluid and the warp (rising over 0.3 s, falling over
  1.5 s), the flow coasting to rest, the same textures carried on when it comes back.
- **The busy centre's contrast** (the owner asked whether it can be helped): at Fluid 0 a quarter to nearly half of the
  centre's pixels hit a colour channel's top (the lights and the beat's flash multiply where it is already dense). A
  hue-keeping soft shoulder instead of each channel's clip removed that (24 % to 0) but did not add contrast (the
  centre's spread 0.30 to 0.25): the flatness is the log display squeezing dense areas. Local exposure (brightness
  against the density around a point) is the lead; not taken yet.
