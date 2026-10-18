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

(How the renderer and the music use it: to be added with that side.)

## Consequences

- The visuals gain a second kind of motion that answers the music without repeating; at none they are as before.
- About 1.5 ms of a tablet's frame while it flows (the step-down of 0046 makes room for it as for anything else).
- Its brightness changes are the picture's own (it writes only into the renderer's buffers), so the flash limits
  planned for the visuals measure them with the rest.
- A third way of drawing to keep in step with the Mali rules; its own lab (a page of three blobs stirred by three
  turning forces) shows a change at a glance.
