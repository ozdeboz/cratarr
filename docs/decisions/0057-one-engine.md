# 0057: One engine: everything that changes the picture is a module or a cable you can see

- Status: accepted (the owner, 2026-10-20, in claude-a5's session: "Yes, point one now", "I won't settle for anything
  less than the North Star", "So divide up the work so that the other sessions can be put to good use")
- Date: 2026-10-20
- Builds on: 0056 (the visuals as a patch) and its amendment (no matrix, fewer lines); replaces how 0046's mapping
  (aspects.ts, routes.ts) runs, not what the picture is

## Context

After 70 the owner asked whether the patch's processing matches what the graph shows: "the degree of processing relates
to the number of components in the graphical user interface. Unless … there is a lot of hidden control and hidden
routes." It does not, yet:

- Two engines. Today's chain runs through aspects.ts on the routes' matrix. The patch's extras run through evaluate.ts on
  cables. chain-cables.ts bridges the two.
- Hidden processing:
  - The loudness envelope scales every cable.
  - Each track brings its own look.
  - The walks wander the look and the behaviour.
  - The Flash setting's course-and-swing filters lights, glow, size and saturation.
  - The turning clock runs at the tempo.
  - The parts' evolution and their steps on hits.
  - The moments step symmetry, lens and form.
  - The governor and the flash meter hold the picture back.
  - Intensity, Flash, Speed and Wild act everywhere without a cable.
- Signals, modulator outputs and beats share one untyped shape. A picture cannot be an input.

We looked at how the established modular tools are built:

- **VCV Rack**: one engine; a module's processing apart from its panel; a cable is an output into an input, read one
  step late so feedback is legal anywhere; normalled connections in Eurorack terms.
- **TouchDesigner**: operator families by data type; cooking only what is needed; any parameter driven by a channel;
  networks nested in components.
- **Max/Jitter**: events, signals and matrices kept apart; sub-patches; a presentation mode over the same patch.
- **cables.gl**: typed ports; explicit trigger wires for what runs when.
- **Hydra**: a picture modulating another picture's coordinates.
- **Bitwig Grid**: modulators with a per-target amount, drawn as rings on the target knob.
- **Reaktor**: blocks built from core parts, with macros.
- **ISF**: a shader declares its inputs; the host builds its controls from that.

## Decision: the North Star

**One patch, one engine. Everything that changes the picture is a module, a cable or a macro, and it shows in the graph
and the rack. The Cratarr look is a chain built from those modules, nothing special-cased.** Nothing less ships as done.

### 1. Typed ports

Six kinds of signal, each with its own wire in the graph:

| Type | Carries | Wire |
|---|---|---|
| `picture` | a texture each frame | solid |
| `light` | the flame's accumulated light before Show displays it (its RGBA and its bands' shares); the flame's own loop works on it, and only Show turns it into a picture (claude-632) | solid, thick |
| `audio` | the sound itself: its recent samples each frame (the page's own player, or the app's `wave`); Music gives it, the Waves source takes it (the owner: "is video and audio essentially the same?": no) | double line |
| `points` | a set of positions with values per point (a grid's points; TouchDesigner's geometry for instancing) | chain line |
| `value` | a number each frame (a signal on 1 = typical; a modulator's own range; a knob's) | dashed |
| `trigger` | events (a beat, a band's hit, a gate opening), each with its strength | dotted |

- A module type declares its ports, the way ISF does: knobs are `value` inputs with a range, a neutral and a default;
  other inputs and outputs are typed ports. Its panel, its graph node and its validation all come from that.
- A cable goes from an output port into an input port of the same type. A `trigger` may go into a `value` port as a
  pulse; nothing else converts.
- A cable keeps its amount, curve, mode (level/swing), walk and pin (0056). It gains a `depth` input (`value`): what
  scales it. The envelope is normalled into every cable's depth (below).

### 2. One engine

- One evaluator per frame computes every `value` and `trigger` in the patch, in dependency order. A cycle reads the
  last frame's value (VCV Rack's rule, evaluate.ts already does it).
- One compositor renders the `picture` graph from the sources to the screen, in dependency order.
- Both are lazy, as TouchDesigner is:
  - only what reaches the screen is computed;
  - a module at neutral is skipped (its output is its input);
  - so is a layer at blend 0, with the flame among them;
  - and a modulator nothing listens to.
- aspects.ts, the routes' matrix and chain-cables.ts are retired. Every behaviour they hold becomes a module (3).

### 3. The Cratarr chain as modules

Each of today's hidden behaviours becomes a module with ports, in the graph and the rack:

| Module | Kind | What it is today |
|---|---|---|
| **Music** | source of values and triggers | the analysis: the whole track's signals, each band's level, hits and envelope; reference and speed per cable (0056) |
| **Track** | source | a new track's pulse and its seed: a new track is a seeded Randomize of the patch (§4e) |
| **Loudness** (type `envelope`) | modulator | the loudness as heard over a fifth of a second (Intensity sets how far it counts); normalled into every cable's depth |
| **Clock** | modulator | the tempo's turning clock (once round in 32 beats at Speed); its phase feeds whatever turns |
| **Flash** | modulator, one per target | the course-and-swing filter (a slow course, Flash's share of the swing about it) |
| **Random** (the walks) | modulator | drift.ts's Look and Behaviour walks: no module of their own (the owner: "we already have walks … we have control generators"); a Random cabled where the walk acts. Drift is renamed Random, as VCV, Vital and Bitwig name it, with a Shape knob from smooth (Drift as it was) to stepped (a random sample-and-hold) |
| **Flame** | picture source | the fractal flame; its parts' evolution (per band), continuity and steps on hits as its own knobs and `trigger` inputs, normalled from Music's hits |
| **Show**, split (below) | Tone map, Curve, Saturate, Blur, Scale, Bend | size, glow, saturation, smoothing, haze, waves |
| **Fluid**, **Tunnel**, **Dive**, **Trails**, **Loop effect** | picture modules in the flame's own loop | as today |
| **Effects** | picture modules, one per effect | today three fixed slots (Kaleidoscope, EYEDO, Hydra), each choosing one sub-effect by a step knob; see below |
| **Moments** | trigger outputs of the module that has them | another symmetry, lens or form at a step's neutral (morph.ts) |
| **Meter** | detector: picture in, busyness values out | the governor's meters |
| **Compressor** | standard sidechain compressor on values | the governor's holding back, as normalled wiring |
| **Screen** (with its flash guard) | the output | the flash rule: fixed, always on, last |

- **Effects, each its own module, in this version** (the owner: "effects on a layer, as in completely modular effects,
  is also in the current version plan"). The owner asked: "And effects would also become modules, correct?" Yes.
  - Every effect is a module of its own: Kaleid, Repeat, Pixelate, Posterize, Rotate, Scroll, Magnify, Ripple,
    Stream, Echo, Colorama, Hue, Saturate, Rings, Interference, Pulse, Burst, Halo.
  - Each is wired freely (§4), as many as wanted, in any order.
  - The three engines stop being slots. Kaleidoscope, EYEDO and Hydra become groups in the module picker.
  - The Effects setting becomes a macro.
  - The Loop effect is the same effect modules in the flame's own loop.
  - Old patches convert: a slot with its sub-effect becomes that one module, at the slot's place.
- **No governor: sidechaining, done the standard way** (the owner: "We shouldn't call it the governor. We should
  remove the governor and implement sidechaining as it is supposed to be done … we shouldn't reinvent it"). The
  governor goes. Its job is done by standard parts, wired as any sidechain is:
  - **Meter: a detector.** Picture in, `value` outs: motion, shape churn, colour churn, zoom pumping, new looks a
    minute (meters.ts's measures). It is like an envelope follower on audio. As many as wanted, on any picture.
  - **Compressor: a sidechain compressor on values.** Inputs: `in` (the signal it compresses, e.g. a cable's
    modulation) and `key` (the sidechain, e.g. a Meter's motion). Knobs: threshold, ratio, knee, attack, release,
    makeup. Outputs: `out` (the compressed signal) and `reduction` (the gain reduction, to drive anything else, as a
    ducker does). The gain computer and its ballistics follow the textbook design (Giannoulis, Massberg and Reiss,
    "Digital Dynamic Range Compressor Design", JAES 2012); nothing of our own.
  - **The default wiring is normalled:** a Meter on the screen's picture, and a Compressor per kind keyed by it,
    through which the cables into knobs of that kind run (each knob declares its kind, `busy`). Today's behaviour is
    that wiring with today's limits as the thresholds. Everything in it can be seen, re-keyed, re-patched or removed.
  - **No Taste, no learning in this version** (the owner: "we drop taste altogether. We defer it to a future version,
    maybe not even the next version"). The thumbs and learning.ts go. The Compressors' thresholds start at the limits
    as they stand when the patch is converted (learned values included, frozen), and are plain knobs from then on. The
    server keeps what was learned, untouched, for whenever the learning returns.
  - **No rating page** (the owner: "Yes, the rating page should also go"). It existed to calibrate the governor and the
    learning. Its fitting (fit.ts) and the trial's plumbing go with aspects.ts.
  - **The Screen's flash guard** is a limiter on the output: three flashes a second over the area the owner chose
    (WCAG). It is safety, not taste: never learned, never removable, last.
  - The precheck and the dice read the same thresholds.
- **Show, split into standard modules and kept as a device** (§6; the owner: "it makes it more coherent"). Show is the flame's output stage, a
  sound synth's channel strip, and each of its parts becomes the module it is. Only the first converts:
  | Module | Was Show's | Sound equivalent |
  |---|---|---|
  | **Tone map** (light in, picture out) | the log-density display, its Brightness (`glow`) | compressor / limiter |
  | **Curve** | Haze (gamma, floor) | waveshaper |
  | **Saturate** | Saturation | drive |
  | **Blur** | Smoothing | low-pass filter |
  | **Scale** | Size | output level |
  | **Bend** | Waves (the slow giant waves) | a slow warp; later the Warp field's |
  - Curve, Saturate, Blur, Scale and Bend are ordinary picture modules, usable on any source (Shapes, Waves, a Mix), so
    every picture can get the flame's finish.
  - The 70.1 Glow (a halo) stays its own module, beside these.
  - The conversion gives today's chain the six in Show's order. The routes into `show.<knob>` go into the module that
    took that knob.
- **Normalled cables.** The connections every chain has by default are shown faint and can be unplugged: Envelope into
  each cable's depth, Music's hits into the flame's steps, the Clock into the turning, Track's look into the flame's look.
  Plugging another cable into such a port breaks the normal, as in Eurorack.
- **Macros.** The whole frame's Intensity, Flash, Speed and Wild become the top patch's macros, with visible cables into
  what they move.

### 4. Freely routable pictures

The owner: "freely routable". The picture side is a free graph, as the value side is, not a fixed line of places.

- **Any picture output into any picture input.**
  - One output may feed several inputs (fan-out).
  - A module may take several picture inputs.
  - Nothing has a fixed position.
- **Blend becomes a module.** Mix takes two or more pictures, each with its mode (add, screen, over, multiply, window,
  lighten: the brighter, as today's picture loop keeps)
  and amount. "Layers" are no longer a structure, just sources mixed.
- **Feedback becomes a module.** It gives the picture its own output had last frame (TouchDesigner's Feedback; VCV
  Rack's one-step cable). Today's picture loop is wired from parts: Feedback → Warp → Keep → Mix with the new picture.
  - The flame's own loop is wired the same way.
  - A loop can be made around any part, at any size.
  - Several loops can run at once.
- **The screen is a module,** with its flash guard; whatever reaches it is drawn. Several outputs can be mixed into it.
- **Places are views, not structure.** The graph's lanes (sources, picture, modulators) and the rack's groups (layers,
  loop, finish) are worked out from the wiring, not stored.
- **Cycles.** A picture cycle must pass through a Feedback module; the graph refuses any other.

- **Full screen** (the owner: MilkDrop "has full screen graphics. Everything is still a shape drawn on blackness"). Three
  things make MilkDrop fill the screen, and each gets a module:
  - **Full-screen sources:** Plasma (noise), Gradient, Cells, the Cover.
  - **A Warp field:** every point moving its own way (swirl, ripple, push toward a point), driven by values or by a
    picture through Displace. Not one move for the whole picture.
  - **Feedback that fills:** a high Keep with an outward zoom, so trails reach the edges and the screen never goes
    back to black.

### 4b. Control sources, after VCV

The owner asked how looping and envelopes fit, keeping it modular. VCV Rack's own answer is small modules that make
control signals; we take its set as it is, in the order the owner agreed. **1 and 2 ship with this version; 3 and 4
come later** (the owner):

1. **ADSR** (an envelope: attack, decay, sustain, release on each gate or trigger), **polyphonic**: fed Music's hits
   (six channels, one a band) it runs six envelopes, one into each part of the flame. It is the visible form of
   today's hidden "Linger" (the bands' lighting), so it belongs to the North Star itself.
2. **The Clock's outputs**:
   - Beat, Bar and Phrase as triggers, with a divider and multiplier;
   - a **clocked LFO**: rate in beats when its clock input is plugged, reset on the downbeat, sine, triangle, saw and
     square.
   Everything rhythmic syncs through a visible trigger line, never a hidden tempo.
3. **Sequencer**: 8 or 16 steps, each a knob, stepped by a clock input, reset input, length; a value out and a gate out.
   It replaces the Counter.
4. **Looper**: records a value (a knob's movement, a macro) over a number of bars and loops it in sync with the Clock;
   record, overdub, clear. In the rack it sits on the knob itself: hold Record, turn the knob.

- **Filter bank and band control, the modular way** (the owner: multiband control is how modular synths do it; "this
  completely replaces our previous implementation of band control"). In a synth, multiband control is a filter bank
  (a crossover splitting the audio into bands), a follower on each band, and polyphonic cables carrying the bands
  on. So:
  - **Filter bank:** `audio` in, six bands out on one 6-channel cable, a standard crossover (Linkwitz–Riley) with
    movable split points. Into followers, or any module, per band. For the live sound, or bands of one's own.
  - **Music keeps its analysed bands** as the precise source: measured on the server over the whole track, each
    against the track's own typical, with hits, flux and percussive/harmonic that a live filter bank cannot give. They
    come out as 6-channel cables too.
  - **Everything band-specific becomes polyphony:**
    - a band is a channel of a cable, not a kind of route;
    - a module fed six channels runs six times (one ADSR, six envelopes);
    - the flame's six parts take six channels;
    - per-band knobs (Evolution, Continuity, the ADSR's release that was Linger) are polyphonic knobs, one value a
      channel.
  - **Retired with it:**
    - the band routes and their "Every band / one band" choice;
    - the Bands pages and the band pins;
    - the `lighting`/`evolution`/`continuity` settings arrays.
    The conversion turns them into channels and polyphonic knobs.
- VCV's other basics are named and sorted as VCV does: Sample & Hold, Random (our Drift), Slew (our followers).
- Each shows its state on its node and its row (VCV's lights): the LFO's wave with a moving dot, the ADSR's shape
  lighting as it runs, the Sequencer's step.
- A knob's "+ Add" offers "New ▸ LFO / ADSR / Sequencer / Looper": the module and its cable in one step, synced to
  the Clock.
- The loudness follower, so as not to be confused with the ADSR, is named Loudness.

### 4c. Grids

The owner asked whether grids are always kaleidoscopic or mosaic, how they control things, how shapes follow them and
how they are modules. A grid is a lattice (square, hexagonal, triangular, polar rings, a spiral) and a symmetry; mosaic
(translation), kaleidoscope (rotation and mirror) and pixelate (a colour a cell) are uses of one. Learned, not invented:
Hydra's `kaleid`, `repeat` and `pixelate` (already ours, 0050); the 17 wallpaper groups (the mathematics of symmetry
since the 1890s, as symmetry tools such as Kali use it); TouchDesigner's Grid SOP, instancing and TOP to CHOP; cables.gl's
points and instancing ops (MIT, ported with a notice); VCV's polyphony; the monome grid.

In this version:
- **Grid:** a source of `points`. Knobs: lattice (square, hexagonal, triangular, rings, spiral), count, spacing,
  rotation, jitter. Per-point values out as polyphonic channels: index, distance from the centre, angle, row, column.
- **Fold:** a picture folded by a grid's symmetry. Modes: mosaic, kaleidoscope, mirror, pixelate, and the wallpaper
  groups beyond. It takes Hydra's Kaleid, Repeat and Pixelate as its first modes, and the fluid's hidden lattice
  switch becomes a Fold too. The group mathematics is taken from the literature, not from anyone's shader, so the
  licence stays clean.
- **Instance:** draws a source once at each point of a `points` input. Shapes follow a grid this way: Grid → Instance
  → Shapes. Each instance gets its point's values as channels, so distance can drive size, angle rotation, and index a
  phase that runs a wave across the grid.

Later, with the Sequencer and the Looper:
- **Sample:** a picture read at each point, giving one value a point. The picture becomes a controller.
- **A grid sequencer** (the monome's): the Clock stepping through the cells.

### 4d. The dice: Randomize, an action

The owner: "the dice become a randomized action". As VCV Rack's Randomize: not a module but an action on one module,
or on the whole patch, giving its knobs new values within their ranges. Pins are kept, the structure (modules,
cables) is never touched, and the patch store makes it undoable. "Another look" reseeds the Track's look. Wild sets
how far it goes. The double tap's "loose" (twice the governor's limits) goes with the governor.

### 4e. A track's look: its seed

Today a track draws its own look from a seed of its own (the same track, the same look): the flame's shape and
colours, and which effects are on. The owner: "the seed is used to control whatever's in the current synthesizer?
That's clean". So:

- A new track is a **seeded Randomize** (§4d) of the patch as it is: the track's seed gives every knob that is not
  pinned a value in its range, morphed in over a track's time. The same track gives the same look while the patch is
  the same.
- It never touches the structure: no module or cable is added, removed or switched. Effects you placed stay.
- **Track** gives its `seed` (and its pulse); which knobs a track moves is set where Randomize's reach is set (the
  pins, and Wild for how far).
- "Another look" is the same Randomize with a new seed.

### 5. Pictures as inputs

A module may take a second `picture` input: Hydra's modulate, TouchDesigner's composite inputs. The first ones:

- **Displace**: one picture moves another's coordinates (the flame stirring the shapes);
- **Mask**: the window mode of 70.1, as a module;
- **Tint**: the Flame colours of 70.1, as a module, from any picture.
- **Glow**: 70.1's glow, as a module of its own (one meaning a module).

The tint and glow knobs built into the sources for 70.1 move into these modules.

### 6. Modules, devices and presets

Three layers, after Ableton's Racks and Reaktor's macros (VCV Rack has only selections: saved groups that come back as
loose modules). The owner: "Show could be a device. A device consists of multiple modules. A preset is a multi-routed
set of components, modules of which some are inserted as devices, but they are in essence just modules."

- **Module:** the standard unit (Tone map, Curve, Blur, an LFO, a Mix, …).
- **Device:** a group of modules with their wiring inside, shown as one node with a few macro knobs (chosen knobs of the
  modules in it). It can be opened, in the graph and the rack, to see and change its modules and cables: nothing in it
  is hidden. It can be collapsed again, and nested: a device may hold devices. Its ports are the ports of its modules
  that it exposes.
  - **Show is a device:** Tone map, Curve, Saturate, Blur, Scale and Bend, wired in the order that draws exactly today's
    picture (claude-632's harness decides it), with Brightness, Haze, Saturation, Smoothing, Size and Waves as its
    macros. So Show stays one familiar thing on the surface and is standard modules underneath.
  - Other devices are made the same way: select modules, make a device, choose its macros, save it.
- **Preset:** a saved patch: modules and cables, some of them inside devices. The chains (Calm, Pulse, Flow …) and the
  examples are presets. A device can be saved and inserted alone too.
- In the model a device is a module of type `device` whose `patch` is a v2 patch of its own, plus its exposed ports and
  macros. The engine and the compositor flatten it before running, so it costs nothing.

### 7. One store

- The patch is the truth: kept, shared and saved in snapshots as a v2 patch.
- A v2 patch is only `{ modules, cables, macros, pins }`: modules with their knobs, cables between typed ports. No
  layers, loop or finish (§4). 0056's `Patch` shape becomes a view worked out from it while the old code still reads
  that shape.
- One store (`patch.svelte.ts`) holds every edit, with undo and redo. Both views read and write only through it.
- Old settings and snapshots (v1) are converted once through 0056's lossless `cratarrPatch`.

### 8. Seeing it work

- Every picture node shows a live thumbnail.
- Every modulator shows a moving meter.
- Every knob's ring shows what moves it and where it is now (Bitwig).
- **Bypass** sends a module to its neutral through the morph; a second tap brings it back.

## Steps and owners

Every step keeps the visuals as they are. Steps 2 and 3 are proved by a frame-by-frame comparison against today's
engine over recorded tracks (the precheck's harness), within a tolerance, before the old code goes.

| Step | What | Owner |
|---|---|---|
| 1 | **Model:** port types and declarations for every module type; the v2 patch as the truth; the store with undo; the v1 → v2 migration; the chain modules' declarations (3) with their normalled cables | claude-a5 |
| 2 | **Engine:** the one evaluator over typed ports; aspects.ts's behaviours ported into the Music, Track, Envelope, Clock, Flash modules (the walks as Drifts) and the Flame's part logic; the Compressor; the control sources of §4b (ADSR and the Clock's outputs with a clocked LFO now; Sequencer and Looper later); the Grid's points and channels (§4c); the precheck, the recipe and the dice on the new engine; equivalence tests | claude-5f |
| 3 | **Picture graph:** the compositor over typed picture ports, lazy; Fold (one module for the grid's symmetries) and Instance (§4c); the Flame, Show and loop modules as nodes in it (render.ts driven by knob values, not AspectsNow); the Meter and the Screen's flash guard; Displace, Mask, Tint and Glow; the Mali cost of each | claude-632 |
| 4 | **Views:** typed wires and ports, normalled cables shown faint, rings, thumbnails and meters, bypass, undo (graph: claude-632; rack: claude-a5) | claude-632, claude-a5 |
| 5 | **Devices and macros:** the device type (a module holding a patch, its exposed ports and macros), flattening for the engine and compositor, Show as a device, open/collapse in the rack (graph: claude-632), the whole frame's macros | claude-a5 |
| 6 | **Retire:** aspects.ts, the routes' matrix, chain-cables.ts, the old settings keys | whoever's step frees it |

claude-a5 coordinates and owns this record. Interfaces are fixed in step 1 (patch.ts) before steps 2 and 3 build on
them. Until step 1 lands, steps 2 and 3 work behind today's interfaces (the equivalence harness; the compositor's
graph walk).

### Conversion details (claude-632, step 3)

- A v1 effects stage samples once with every shape effect applied in slot order, then colours at the final
  coordinates. As one module per effect, the same picture needs the colour effects and Echo first, in slot order, then
  the shape effects in reverse slot order. Echo together with a shape effect cannot match exactly: it converts within
  a measured tolerance, shown to the owner.
- A module whose morph is bringing it back (target above neutral) keeps running while it is at neutral, so the flame
  coming back from blend 0 has its light already gathered, as today.

## Consequences

- One mental model: what the graph shows is what runs. Hidden control is a bug.
- Tracks, snapshots and groups keep working through the one-time migration; a v2 page reads v1 state, not the reverse.
  Every device of a sync group needs the same version (as with 66's snapshots).
- The tablet profits from laziness: an off flame or an idle modulator costs nothing.
- It is a large change to the core of 70. It ships as a whole when the North Star is met, as 70 did.

## Open questions

- Whether a band's six outputs show as one row with six channels (VCV's polyphonic cables) or as six ports. The graph's
  "Level · 6 bands" bundle suggests channels.
- Where the Compressors' reduction shows in the rack.
