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

Four kinds of signal, each with its own wire in the graph:

| Type | Carries | Wire |
|---|---|---|
| `picture` | a texture each frame | solid |
| `light` | the flame's accumulated light before Show displays it (its RGBA and its bands' shares); the flame's own loop works on it, and only Show turns it into a picture (claude-632) | solid, thick |
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
| **Track** | source | a new track's pulse, and the track's own look (its preset) as outputs into the flame's look knobs |
| **Envelope** | modulator | the loudness as heard over a fifth of a second (Intensity sets how far it counts); normalled into every cable's depth |
| **Clock** | modulator | the tempo's turning clock (once round in 32 beats at Speed); its phase feeds whatever turns |
| **Flash** | modulator, one per target | the course-and-swing filter (a slow course, Flash's share of the swing about it) |
| **Walk** (Look, Behaviour) | modulator | drift.ts's walks |
| **Flame** | picture source | the fractal flame; its parts' evolution (per band), continuity and steps on hits as its own knobs and `trigger` inputs, normalled from Music's hits |
| **Show** | picture module | size, glow, saturation, smoothing, haze, waves |
| **Fluid**, **Tunnel**, **Dive**, **Trails**, **Loop effect** | picture modules in the flame's own loop | as today |
| **Effects** | picture modules, one per effect | today three fixed slots (Kaleidoscope, EYEDO, Hydra), each choosing one sub-effect by a step knob; see below |
| **Moments** | trigger outputs of the module that has them | another symmetry, lens or form at a step's neutral (morph.ts) |
| **Governor** | modulator watching a picture, as many as wanted | the governor: levers per kind of busyness, cabled (normalled by kind) into what they hold back |
| **Taste** | source, one per user | the learned limits (the thumbs), normalled into every Governor |
| **Screen** (with its flash guard) | the output | the flash rule: fixed, always on, last |

- **Effects, each its own module.** The owner asked: "And effects would also become modules, correct?" Yes.
  - Every effect is a module of its own: Kaleid, Repeat, Pixelate, Posterize, Rotate, Scroll, Magnify, Ripple,
    Stream, Echo, Colorama, Hue, Saturate, Rings, Interference, Pulse, Burst, Halo.
  - Each is wired freely (§4), as many as wanted, in any order.
  - The three engines stop being slots. Kaleidoscope, EYEDO and Hydra become groups in the module picker.
  - The Effects setting becomes a macro.
  - The Loop effect is the same effect modules in the flame's own loop.
  - Old patches convert: a slot with its sub-effect becomes that one module, at the slot's place.
- **The governor, the taste and the flash guard** (the owner: "If there aren't multiple governors, then it's not truly
  a modular component. And if it's not truly a modular component, then it's not properly integrated into the
  architecture"). Three parts, each where it belongs:
  - **Governor: a module, as many as wanted.** Like a compressor's sidechain, it takes a `picture` in (what it
    watches) and gives `value` outs, one lever per kind of busyness. A lever is 1 while that busyness is within its
    limit, and falls while it is over. Cables carry the levers wherever they should hold back.
    - By default each lever is normalled into the depth of the cables into knobs of its kind. Every knob declares its
      kind (`busy`: motion, shape, colour, zoom, looks).
    - A default patch has one Governor watching the whole picture. Another on a layer holds back that layer's own
      cables, and a nested chain may bring its own.
    - Its knobs are its limits, one per kind, scaled by its Wild.
    - **It is a sidechain compressor, not an invention** (the owner: "sidechaining is something we don't have to invent
      ourselves"). Per kind of busyness:
      - the detector is that meter;
      - the threshold is its limit;
      - ratio, knee, attack and release shape the lever;
      - the lever is the gain reduction.
      The gain computer and its ballistics follow the standard design (Giannoulis, Massberg and Reiss, "Digital
      Dynamic Range Compressor Design", JAES 2012). Today's levers already behave like one; they are restated as
      such, with the same defaults.
  - **Taste: the learning, a source module.** One per user. Its outputs are the learned limits, one per kind,
    normalled into every Governor's limits, which then act as offsets on them.
    - The thumbs teach Taste by 0048's rules. The flow is unchanged: one thumb on the picture as it is.
    - The thumb's kind is the one that stood out most against its limit in the 20 seconds before, read from the
      Governors watching what is shown. The whole picture's watcher counts first.
    - What was learned before is Taste's state as it is.
  - **Flash guard: part of the Screen,** like a master limiter: three flashes a second over the area the owner chose
    (WCAG). It is safety, not taste: never learned, never removable, the last thing before the picture is shown. It
    holds the picture itself, and holds back the Flash modules' swing through their own normalled lever.
  - The precheck and the dice read the same Taste and Governor limits.
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

### 5. Pictures as inputs

A module may take a second `picture` input: Hydra's modulate, TouchDesigner's composite inputs. The first ones:

- **Displace**: one picture moves another's coordinates (the flame stirring the shapes);
- **Mask**: the window mode of 70.1, as a module;
- **Tint**: the Flame colours of 70.1, as a module, from any picture.
- **Glow**: 70.1's glow, as a module of its own (one meaning a module).

The tint and glow knobs built into the sources for 70.1 move into these modules.

### 6. Nesting

A chain can be one module of another patch, its chosen knobs exposed as its macros (Reaktor's blocks, Max's
sub-patches). Presets are chains, and a preset can be dropped in as a part.

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
| 2 | **Engine:** the one evaluator over typed ports; aspects.ts's behaviours ported into the Music, Track, Envelope, Clock, Flash, Walk modules and the Flame's part logic; the precheck, the recipe and the dice on the new engine; equivalence tests | claude-5f |
| 3 | **Picture graph:** the compositor over typed picture ports, lazy; the Flame, Show and loop modules as nodes in it (render.ts driven by knob values, not AspectsNow); the Governor (a sidechain compressor), Taste and the Screen's flash guard; Displace, Mask, Tint and Glow; the Mali cost of each | claude-632 |
| 4 | **Views:** typed wires and ports, normalled cables shown faint, rings, thumbnails and meters, bypass, undo (graph: claude-632; rack: claude-a5) | claude-632, claude-a5 |
| 5 | **Nesting and macros:** chains as modules, the whole frame's macros | claude-a5 |
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
- Where a Governor's levers show in the rack.
