# 0056: The visuals as a patch: layers, modules, modulators and cables, morphing all the way

- Status: accepted (the owner, 2026-10-19, through a design talk in claude-a5's session; then "Better start working on
  this now", "This is the next version!!" and "Make sure the team gets their jobs")
- Date: 2026-10-19
- Builds on: 0046 (the visuals), 0047 (the fluid), 0048 (the randomisation and its limits), 0050 (the effects stage),
  0049 (the beats); replaces how 0046's routes and settings are shown and kept, not what the picture is

## Context

The owner asked what Cratarr's visuals can learn from MilkDrop, wanting to "integrate as much of it as possible while
keeping things more simple than they are right now". Over the talk the aim sharpened:

- "Less control is not more coherence": keep every control; make the structure coherent.
- "One of the most important differences I want to maintain is our ability to morph between different states. So as
  little discrete control as possible, like a modular synthesizer"; "being able to chain more freely might be the
  solution".
- "What I want to be able to do is put a kaleidoscope on the shape layer, for example."
- "A solution to our matrix would be a graphical modular builder. Then no unused space!"; "We do need a nested control
  panel now!"
- "Chains become presets, then within that we can freely evolve."
- "We have exceptions since not all can be continuous."
- MilkDrop's presets "come later": first learn from them what works and how to organise our pipeline.

What was learnt:

- **MilkDrop's pipeline** (Butterchurn and projectM implement the same one): per-frame equations; per-vertex equations
  making one motion field; a warp pass carrying the last frame along it with a decay (the feedback, MilkDrop's core);
  waves, shapes, borders drawn on top; a composite shader to the screen; presets blended or cut. Its inputs: bass, mid
  and treble as energy against their own average of the last ~4 s (1 = normal lately), each with a slower form
  (`_att`). Butterchurn takes samples given to it (`render({audioLevels})`), so a page can feed it its own sound.
- **A study of 9,109 curated presets** (projectM's Cream of the Crop; text matching, so approximate):
  - presets build modulators first: summed volume, moving averages, peak and mean trackers, adaptive beat thresholds,
    counters; only then do they drive the picture;
  - their beat tests sit at 1.0 ("above typical"; bass's upper quartile 1.2);
  - the music goes to drawing (alpha, colour, radius, position) and to motion (bass on zoom in ~1,200); rotation,
    shift and centre drift with time; decay and finishing are almost never audio-driven;
  - shaders read the slow inputs, equations the raw ones;
  - about 80% use warp and comp shaders; the look lives in the feedback; blurred copies of the picture are used
    heavily (64/44/34% for the three levels).
- **Open-source VJ tools:**
  - Hydra (AGPL; sources → geometry → colour → blend → modulate chains, four outputs that feed back; 0050 ported its
    functions);
  - cables.gl (MIT since 2024-08; wire patching in WebGL);
  - ossia score (CeCILL / GPLv3; ISF shaders wired by cables, interpolation between events);
  - Le Biniou (GPL-2.0+; plugin layers and sequences);
  - ISF (a shader format that declares its own inputs).
- **Sources the others use** fall into seven families: oscillators and gradients, noise and cells, shapes, lines drawn
  by the sound, points (our flame is the fractal kind), media (camera, video, images, text), and the previous frame.
  For a music player, the playing track's cover and title are natural sources too.
- **Preset licences:** almost no MilkDrop preset carries a licence (Cream of the Crop's LICENSE.md only assumes public
  domain). The clean ones found: M0Rf30/projectm-presets (MIT, 6 usable) and 0jar/milkdrop-presets (WTFPL, 11).
  Including presets is for later, and only with an explicit grant (the owner: "no funny stuff! Source it well").

## Decision

### The patch

The visuals are a **patch**: a small graph the picture flows through, and the cables that move its knobs.

```
 layer: flame  ─→ [its modules] ─┐
 layer: shapes ─→ [kaleidoscope] ─┼─→ blend ─→ ┌→ loop: move → keep ─┐ ─→ finish ─→ screen
 layer: waves  ─→ [its modules] ─┘             └─────────────────────┘
 modulators: beat clock, followers, drift … ──(cables)──→ any knob
```

- **Layers** are sources, each with its own modules (a kaleidoscope on the shape layer only) and, where the source
  needs one, its own loop. The flame keeps its accumulation as its own loop: today's fluid, Tunnel, Dive, loop effect,
  Facets and fade run inside it, so today's look is kept exactly. Layers are blended (amount and mode).
- **The loop** is the picture-level feedback after the blend (MilkDrop's warp): move modules, then keep (the decay). It
  is new; the flame's own loop is not it.
- **Finish** turns the picture into what is shown: colour, the effects (0050's), and the flash meter last, always.
- **Stages are positions**: a module may sit on a layer, after the blend, in the loop or in finish, and means a
  different thing in each (a kaleidoscope on the shapes, on everything, or compounding in the loop).
- **Modulators** are modules too, with no picture: followers, fast and slow forms, "above typical" gates, peak and
  mean trackers, beat and bar clocks, drift and LFOs, sample-and-hold, envelopes. Their outputs and the music's
  signals feed **cables**.
- **A cable** runs from a signal or a modulator to a knob, with today's route value on it (an amount, or a curve), as
  a level (a share under the knob's setting) or a swing (about it), as routes are now (0046, amended).
- **Macros**: a container (a layer, the loop, finish, a group) shows a few macro knobs on its row one level up, each
  moving many knobs inside; a macro is a knob, so it is cabled and morphs.

### Chains and states

- **A chain** is the patch's structure: its modules, their order and places, its cables and macros. Chains are the
  presets: chosen, saved, shared, rated.
- **A state** is every knob's value. Within a chain the state evolves freely (the music, the walks, the dice) and
  morphs.
- **Every knob is continuous** with a range and a **neutral**, where its module does nothing (an amount at 0, a
  kaleidoscope with no segments, a decay that keeps nothing more than the source draws). A morph between two states is
  always defined.
- **A chain change morphs through neutral** (the owner's pick): modules both chains share carry their state over;
  the others fade in from their neutral or out to it. Never a cut.
- **The exceptions** (the owner: "not all can be continuous"):
  - lens, mirror, colour family and form mix by the flame's own maths (variations add by weight; Electric Sheep pads
    flames with do-nothing transforms to morph them, the same principle);
  - symmetry order, an effect slot's choice and the lattice are **steps**: they change only while their module is at
    its neutral, and a moment that asks for one dips through neutral and back over about a beat;
  - Quality, FPS, the points and the brightness cap are the device's, not the state's: never morphed or shared.
- **The dice** become "evolve" (a new state for this chain) and "another chain".

### Signals: 1 = typical

- Every signal is on one scale: **1 = typical**, so there is room above and below to swing (MilkDrop's presets test
  exactly that: "above 1").
- The reference is **this track** by default (the whole song's median; the drop stays the biggest moment), and each
  signal row may choose **lately** (the last few seconds, MilkDrop's) or **absolute** (loudness, timbre, width), the
  owner's pick.
- Each signal comes **fast and slow**; a cable chooses.

### The panel

- **Nested**, by the settings' North Star: groups of rows; every thing (a layer, a module, a knob's cables, a
  modulator) is a row that opens its own page; a breadcrumb (Visuals › Shapes › Kaleidoscope) and back. A knob's
  cables are its things.
- **A graph builder** where there is room (Svelte Flow, MIT, xyflow): the same patch drawn, picture cables and
  modulation cables, tapping a node opens its page. On a phone the nested pages are the builder (the rack, with
  drag-to-modulate rings on the knobs).
- Groups: Chain; Whole frame (Intensity, Flash, Speed); Layers; Loop; Finish; Modulators.

### Interfaces (web/src/lib/visuals/patch.ts, step 1)

```ts
type Knob = { id: string; label: string; min: number; max: number; neutral: number; default: number;
  kind: 'level' | 'swing' | 'step'; device?: boolean };
type ModuleType = { type: string; label: string; place: 'source' | 'module' | 'loop' | 'finish' | 'modulator';
  knobs: Knob[] };
type Module = { id: string; type: string; knobs: Record<string, number> };
type SignalRef = { signal: string; band?: number; reference?: 'track' | 'lately' | 'absolute'; speed?: 'fast' | 'slow' };
type Cable = { from: SignalRef | { modulator: string }; to: { module: string; knob: string }; value: RouteValue;
  mode: 'level' | 'swing' };
type Layer = { id: string; source: Module; modules: Module[]; loop: Module[]; blend: { mode: string; amount: number } };
type Patch = { v: 1; layers: Layer[]; loop: Module[]; finish: Module[]; modulators: Module[]; cables: Cable[];
  macros: { id: string; on: string; label: string; value: number;
    targets: { module: string; knob: string; from: number; to: number }[] }[] };
```

**Modulators** (web/src/lib/visuals/modulators/, claude-632's): each a `ModuleType` with `place: 'modulator'` and

```ts
type Signals = (ref: SignalRef) => number;   // 1 = typical
interface Modulator { step(dt: number, signals: Signals, knobs: Record<string, number>): number }
```

pure TypeScript, testable in Node; continuous knobs only; output 0 at its neutral.

### Today's code in the patch

| Today | In the patch |
|---|---|
| flame.ts, render.ts's points and show | the **flame** layer: source knobs (form, detail, twist, warp, symmetry and mirror as steps, lens, colour, the parts' shape, rotation, lights, hue, spread, presence, Linger, Evolution, Continuity); its show (size, glow, saturation, smoothing, haze, waves) |
| fluid.ts (claude-5f), zoom.ts, the loop effect, Facets, the fade | the flame's own loop: Fluid (stir, flow, swirl), Tunnel, Dive, Loop effect, Facets (lattice a step), Trails |
| effects.ts (0050) | finish modules (Effect 1–3: a step choice, amount, two knobs); later on any layer |
| routes.ts, curves.ts, aspects.ts | cables; the level-or-swing rule kept; Behaviour's and Look's walks become drift modulators cabled to their containers' macros |
| settings.svelte.ts | the state; device settings apart; pins become pinned knobs |
| signals.ts, beats.ts, live.ts, music.svelte.ts, envelope | the signal rows; normalised (step 5) |
| recipe.ts, precheck.ts, meters, flashmeter | evolve (the dice), the chain's check, the governor: their roles unchanged |
| shared.svelte.ts, snapshots | the chain plus its state shared; snapshots are states, presets are chains |
| Visuals.svelte's panel | the nested panel and the graph |

### Steps (each keeps the visuals working)

1. The patch model: types, the module types of today's pipeline, the **Cratarr chain** equal to it, and a lossless
   conversion to and from today's settings (round-trip tested). Nothing on screen changes. (claude-a5)
2. The nested panel drawn from the patch, over today's renderer, replacing the Behaviour and Look lists and the route
   grid. (claude-a5)
3. The graph builder on wide screens (Svelte Flow). (claude-a5)
4. The renderer as modules: the flame layer as today, the picture-level loop, finish, new sources (shapes, waves:
   MilkDrop's kinds, written from scratch), per-layer modules; the cost of a layer measured on both Mali devices, the
   layers capped by it. Fluid as a module (claude-5f's file), a picture-loop variant beside the flame's. (claude-a5,
   claude-5f)
5. Normalisation: the server sends each signal's typical value beside today's scale (claude-5f); "lately" and the
   fast/slow forms worked out in the page; the modulators (claude-632). The app's live sound sends a short waveform
   for the Waves source (claude-5f).
6. Morphing: the neutral rule, chain changes, chains as presets, evolve. (claude-a5)

## Consequences

- 71 is this release (the owner). Steps 1–3 can ship without a picture change; 4–6 change what can be built.
- Old snapshots and shared group state are converted once (`v` in the kept settings); the flame's look stays as drawn.
- The flash meter and the governor stay last and whole: no chain gets past them.
- A layer costs a pass on Mali (0047's measure); the number of layers is capped by what the tablet affords.
- MilkDrop presets may come later as a source of chains, only with an explicit licence.

## Open questions

- Symmetry order: through neutral (as a step), or its angles sliding (the picture whole, its symmetry lost on the way)?
- Which sources beyond the flame, shapes and waves (noise and cells, oscillators, the cover, the title)?
- The layer cap, once measured.

## Sources

- Butterchurn: https://github.com/jberg/butterchurn (renderer.js takes `audioLevels`; audioLevels.js: the ratios to
  the long average)
- projectM Cream of the Crop: https://github.com/projectM-visualizer/presets-cream-of-the-crop (LICENSE.md, issue #8)
- Hydra's functions: https://github.com/hydra-synth/hydra-synth (src/glsl/glsl-functions.js)
- cables.gl open source (MIT): https://cdm.link/2024/08/cables-gl-open-offline/
- ossia score: https://ossia.io/score/about.html, https://ossia.io/score-docs/examples/video/video-basics.html
- Le Biniou: https://appstream.debian.org/trixie/main/metainfo/lebiniou.html
- Svelte Flow (xyflow, MIT): https://svelteflow.dev
