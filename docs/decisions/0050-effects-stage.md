# 0050: Visuals 65: band envelopes, fewer sliders, an effects stage, Facets

- Status: proposed (claude-a5, 2026-10-18; the owner: "Now, for 65, the Hydra effect layer … So plan for it", "facets
  with square and X going 65", and on the plan: "maybe the whole touch interaction should be deferred to 66, since we
  have sufficient for 65")
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

### Three releases

65 ships in steps, as 64 and 64.1 did, each one whole on its own:

- **65**: the band envelopes and Presence; the fewer sliders, with their migration.
- **65.1**: the effects stage.
- **65.2**: Facets.

### Band envelopes and Presence (65)

claude-5f makes the envelope (from the hits and the band's level: an attack, a hold while the sound holds, a release),
its signal row, and its limits in the precheck. claude-a5 makes Presence in the drawing: a band's part weighs in the
chaos game (its transform's share of the points) by a new **Presence** route column, so a part can come and go with its
sound, not only light up. Lighting is renamed **Linger** (how long a part's light rings on).

*claude-5f to fill in: the envelope's definition and its defaults.*

### Fewer sliders (65)

- **Quality** (one ceiling) in place of Points, Steps and Resolution. FPS stays. The device's pace (0046's Load) still
  takes passes and points away under it; Quality is only how far it may go up.
- **Haze** (−100 % to +100 %) in place of Floor and Depth.
- **Family** folded into **Colours**.

Both columns of the routes into Floor and Depth become one into Haze; a route kept, shared or saved into either is read
into Haze. Saved snapshots are converted on recall (their form goes to 4), so an old one looks as it did; the shared
settings are marked, as Fluid's scale was in 64.1.

*claude-5f (who brought the owner's list) to fill in: how Haze maps onto the floor and the gamma, and how Quality divides into points, steps and
resolution.*

### The effects stage (65.1)

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

**Settings and routes.** The chain is part of the look (pinnable like the look's other settings) and the dice may draw
one; Another look (the stars) stays as it is, as the owner wanted in 0048. One **Effects** route column moves the chain
as a whole (each effect's amount scaled by it), so the routes and their curves drive the effects as they drive the
rest; a column per effect would crowd the grid, and can come later if one is missed.

**Who does what** (agreed in 64): claude-a5 builds the stage, the compiler and the reports' plumbing; claude-5f ports
the effects, measures the devices, and writes the help texts and the effects' limits.

**Held to the limits.** The flash meter already reads the screen, so it sees the effects with no change. The other
meters (motion, shape and colour churn, zoom pumping) are counted from what the flame does, not from the screen: each
effect reports what it adds (a turning kaleidoscope adds motion, a cycling colorama adds colour churn), so the governor
can hold an effect back as it holds the flame back. The precheck (which draws no picture) runs the same reports.

### Facets (65.2)

Facets are drawn where the blocks were: in the fluid's light, as it is carried along the flow. Each cell of a lattice
takes on one light, softened at its edges by **Facets** (0 %: none, as today; 100 %: hard tiles). The lattice is
**square** or **hex** (**Lattice**, a look setting). The cells move with the flow, which a mosaic laid over the picture
would not; that is what the owner liked. Facets is a route column too. Without Fluid there is no flow, so Facets does
nothing (marked in its help).

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

## Open for the owner

1. **Facets as above** (proposed in 64, not yet confirmed): a softness slider and a square or hex Lattice in the look,
   a Facets route column, in the fluid only.
2. **Effects on the dice**: recommended yes (a full roll may draw a chain), the stars unchanged.
3. **Old snapshots after the fewer sliders**: recommended converted on recall, so each looks as it did.
4. **The double tap's room across a sync group** (from 64.1: today only the device tapped governs at twice the limits;
   the others in the group get the routes and govern at their own Wild): shared, or this device only. Recommended
   shared, as a group setting, in 65.
5. **The three releases** (65, 65.1, 65.2), or 65 whole.

## Consequences

- Nothing changes on screen until a chain is set: 65.1 with no effect must draw exactly as 65 (to be checked pixel for
  pixel in the lab).
- Every effect needs its meters report before it ships, or the governor cannot hold it.
- Saved snapshots and shared settings carry a form number again (4); the recall code keeps every older form readable.
- The panel gains Quality, Haze, the effects' settings and three route columns (Presence, Effects, Facets), and loses
  Points, Steps, Resolution, Floor, Depth and Family.
