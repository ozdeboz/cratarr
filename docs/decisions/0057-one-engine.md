# 0057: One engine: everything that changes the picture is a module or a cable you can see

- Status: accepted (the owner, 2026-10-20: "Yes, point one now", "I won't settle for anything less than the North
  Star")
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

## Words (the owner, 2026-10-20)

- **Rack:** the instrument, the container of modules and devices. Not "chain" or "synthesizer" (those words in this
  record before this section mean the rack).
- **Patch:** the rack's current state: its wiring and knob values ("patch is the current state").
- **Snapshot:** any saved patch ("snapshot is any saved patch"). A preset is a snapshot that comes with the app.
- **Device:** a group of modules shown as one, with its macros (Show, Cratarr, the nine of §6b).
- **Module:** one unit (a Flame, a Blur, an LFO).

What shows (labels, help, the panel, the graph) uses these words today. The code's names (`chains.ts`, `Chain`) follow
in step 6, as that code is replaced; `PatchV2` and the store's `patch` already mean the patch.

## Decision: the North Star

**One patch, one engine. Everything that changes the picture is a module, a cable or a macro, and it shows in the graph
and the rack. The Cratarr look is a chain built from those modules, nothing special-cased.** Nothing less ships as done.

### 1. Typed ports

Six kinds of signal, each with its own wire in the graph:

| Type | Carries | Wire |
|---|---|---|
| `picture` | a texture each frame | solid |
| `light` | the flame's accumulated light before Show displays it (its RGBA and its bands' shares); the flame's own loop works on it, and only Show turns it into a picture | solid, thick |
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

Each of today's hidden behaviours becomes a module with ports, in the graph:

| Module | Kind | What it is today |
|---|---|---|
| **Music** | source of values and triggers | the analysis: the whole track's signals, each band's level, hits and envelope; reference and speed per cable (0056) |
| ~~Track~~ | — | folded into Music (§7f): its New track trigger and Track seed |
| **Envelope Follower** (type `envelope`) | modulator | the loudness as heard over a fifth of a second (Intensity sets how far it counts); keys the Loudness Follow device's VCA (§7f), normalled into nothing |
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
    maybe not even the next version"). The thumbs and learning.ts go. The Duckers' thresholds start at the default
    limits (nothing learned) and are plain knobs from then on, Wild a macro on them. What the server stored of the
    learning has no reader left, and goes with the /limits retirement.
  - **No rating page** (the owner: "Yes, the rating page should also go"). It existed to calibrate the governor and the
    learning. Its fitting (fit.ts) and the trial's plumbing go with aspects.ts.
  - **The Screen's flash guard** is a limiter on the output: three flashes a second over the area the owner chose
    (WCAG). It is safety, not taste: never learned, never removable, last.
  - The precheck and the dice read the same thresholds.
  - **Thresholds and Wild** (the owner): each Ducker's threshold starts at its default limit × 1.5,
    nothing learned. **Wild is a macro on the five thresholds**, moving each along its exponential taper (a knob's
    taper: equal steps of position multiply the value, as a frequency knob's do) from half to twice. So Wild gives
    exactly today's factor 2^(2(wild − ½)). Randomize's reach is a setting of its own, no longer Wild's.
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

### 3b. Five categories, no look and behaviour

The owner: "we should make no distinction between look and behavior"; then agreed to five categories by what a module
does with signals, as modular synths and video synthesizers group them (LZX, TouchDesigner):

| Category | Does | Ours |
|---|---|---|
| **Sources** | make a picture | Flame, Regular shapes, Irregular shapes, Waves, Noise; later Gradient, Cells, Cover |
| **Effects** | a picture in, a picture out | Fold, Bend, Blur, Tone map, Curve, Saturate, Warp, Keep, Glow, Tint, Displace, the flame's own loop |
| **Mixers** | several pictures into one | Mix, Mask, Feedback |
| **Modulators** | the signals that move knobs | LFO, Random, ADSR, Clock, Compressor, Grid; later Sequencer, Looper |
| **In / Out** | where signals enter and leave | Music, Filter bank, Meter; Screen |

- Each module type has its category (rack.ts `categoryOf`); a device's follows from what it exposes.
- The graph's lanes, the module picker and Randomize's scopes follow the categories.
- Look and Behaviour go everywhere: settings, pins, walks, scopes.
- **A whole category can be pinned** (the owner): Randomize then leaves every module in it as it is, a new track's
  too. It is saved with a preset like any pin (`category:<name>`).

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
  - **Full-screen sources:** Noise, in this version (the owner, 2026-10-20: noise and grain "together with filter can
    be used to great effect"); Gradient, Cells and the Cover later.
    - **Noise:** monochrome (colour a Tint's, §4g), its scale, detail (octaves) and speed. At its finest scale, new each
      frame, it is grain: no Grain module.
    - **Filters on pictures:** Blur is the low-pass (noise into clouds); the high-pass is a picture less its blur, by
      Mix's new **Subtract** mode (edges, grit). Nothing invented: LZX's noise, Hydra's `noise()`, TouchDesigner's
      Noise TOP.
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
- The loudness follower is named **Envelope Follower** (the owner, 2026-10-20; the standard name: "Envelope" clashes with
  Music's per-band Envelope output and with the ADSR, an envelope generator).

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

- **Its scopes** (the owner: "randomize everything, randomize effects, randomize generators … when long pressing the
  randomize button, … a drop down with the various different options"):
  - Everything;
  - Sources, Effects, Mixers, Modulators: the five categories' knobs (§3b; In / Out has none to roll);
  - Cables: the amounts of the cables there are.
  - The scopes are the categories, so a new module falls into one by itself.
- **The button:** a tap randomizes everything. A long press opens the app's menu (MenuSheet, as every long-press menu)
  with the scopes. The double tap's movement-only roll goes.
- **A module's own menu** has Randomize too, for that module alone (VCV's per-module Randomize).
- **Pins keep knobs still against randomizing** (the owner): a pinned knob is left as it is by Randomize, by its
  scopes, by "Another look" and by a new track's seeded Randomize. Cables still move it: a cable is a choice, not a
  roll. A pin can be on one knob, one channel of a polyphonic knob, a cable's amount, or a whole module or device. It
  shows as a pin mark on the knob's ring.
- **A preset keeps its pins** (the owner: "when loading a preset, its pinned knobs stay pinned"). A saved preset, a
  snapshot and a device all save their pins with them, as the v2 patch's `pins` does. Loading one sets the pins to
  its own; nothing pinned before carries over into it. Undo brings back the pins with the rest.

### 4e. A track's look: its seed

Today a track draws its own look from a seed of its own (the same track, the same look): the flame's shape and
colours, and which effects are on. The owner: "the seed is used to control whatever's in the current synthesizer?
That's clean". So:

- A new track is a **seeded Randomize** (§4d) of the patch as it is, scope **Everything** (the owner chose it: motion
  and generators too, not only the look): the track's seed gives every knob that is not pinned a value in its range,
  morphed in over a track's time. Pins keep what should stay. The same track gives the same look while the patch is
  the same.
- It never touches the structure: no module or cable is added, removed or switched. Effects you placed stay.
- **Track** gives its `seed` (and its pulse); which knobs a track moves is set where Randomize's reach is set (the
  pins, and Wild for how far).
- "Another look" is the same Randomize with a new seed.
- **Reset to seed** (the owner, beside resetting the whole synthesizer): the playing track's own seeded Randomize again,
  as when the track began. Whatever the dice, "Another look" or the knobs changed since is put back to the track's own
  values, pins kept, structure untouched, undoable, morphed in. Resetting the whole synthesizer stays what it is: back
  to an empty patch (Start from scratch). Both are in the Reset menu.

### 4f. Lens, a module of its own

The owner: "lenses aren't inherently coupled to flame. Lens should be a modular thing." Today's lenses (Eyefish,
Bubble, Spherical, Swirl …) are the flame's final transform. They become a **Lens** module:

- Knobs: its kind (a step, with `next`) and its amount.
- Polymorphic, as Blur is:
  - on `points` (the flame's points, a Grid's) it moves each point, exactly as the flame's final transform does;
  - on a picture it remaps the pixels the same way (TouchDesigner's Remap, Hydra's modulate).
- The flame's own lens is a Lens cabled between the flame's points and its light, inside the Cratarr device. It can be
  unplugged, swapped, or joined by others anywhere. The conversion places it so, exact.

### 4g. One set of shapes

The owner (2026-10-20): "I want the same set of shapes everywhere shapes are used." Two modules work with shapes:
Shapes (it draws them) and Lens (it sends points into them). Each had its own: Shapes the regular polygons, Lens Heart
and Cratarr. Now one set, each shape defined once (`shape-set.ts`) and used by both:

- **Regular:** polygon (3–48 sides) and star (3–48 points, a depth; depth 0 the polygon). They morph: the sides go
  through the fractions, the depth through its range.
- **Irregular**, a list stepped through (with `next`), never morphed:
  - Heart and Cratarr (the logo: the record's rings and its slice, `MARK`);
  - ring, cross, arrow, crescent, drop (the owner's pick of common symbols);
  - the 11 forms of the warps (Spherical, Swirl, Julia, Fisheye, Eyefish, Bubble, Polar, Disc, Sinusoidal, Horseshoe,
    Handkerchief): each the form its warp makes of a disc, as Heart's is.

The owner: "a difference between morphable shapes and non-morphable shapes … two different modules":

- **Regular shapes** (the Shapes module of 70, renamed): sides and depth. It also gives its **corners** as `points`,
  so a corner fractal (Sierpinski, Koch) is standard modules: corners → Instance (a smaller copy at each) → Feedback.
  Only regular shapes have corners; a feedback fractal (Fractal Spiral) takes any picture.
- **Irregular shapes**: a shape knob (a step).
- Both keep the same layout knobs (count on a ring, spread, orbit, spin, twist, taper) and the same stroke: an outline
  of a width, a fill, or both. One code; they differ only in how they pick their shape.
- **Monochrome** (the owner: "all monochrome"): they draw light only. Colour is a Tint's after them (its own hue, or a
  picture's colours by its second input), one meaning a module; hue, hue spread and saturation leave them for Tint, as
  70.1's tint and glow do (§5). The conversion puts a Tint behind every coloured Shapes of today, so it looks the same.
- **Lens stays one module** (the owner): its kind steps through the 11 warps, Polygon, Star and the irregular shapes;
  Polygon and Star take Sides and Depth knobs. A warp kind moves points as today; a shape kind sends them into the
  shape evenly, as the Cratarr lens does. The kinds of today keep their numbers, so every track's look stays.

### 4h. A modular Fluid

The owner, 2026-10-20: "Yes, this is what I want"; "Fluid should also be applicable to
grids". No hidden couplings left:

- **Polymorphic:** Fluid takes light, a picture or points and gives the same type out: in the flame's loop, on any
  layer, in the finish, as many as wanted. On points it moves them along its flow, so Grid → Fluid → Instance makes a
  grid that swims.
- **Its forces in sight:** Stir, Flow and Swirl stay cable-able knobs; where the pushes land is a `points` input
  (`at`), normalled to the flame's new `points` output (where its parts are), as today.
- **Its flow out:** a new port type, `flow` (a velocity field). Fluid's **Flow field** output is normalled into Show's
  Bend (today's hidden read-back) and into Stream's (the Loop effect's and Hydra's), and can be cabled into Displace.
- The default patch wires it as today, picture identical. On the tablet a fluid costs about 1.2–1.7 ms: at most one
  or two there.

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
    picture (a harness decides it), with Brightness, Haze, Saturation, Smoothing, Size and Waves as its
    macros. So Show stays one familiar thing on the surface and is standard modules underneath.
  - Other devices are made the same way: select modules, make a device, choose its macros, save it.
- **Preset:** a saved patch: modules and cables, some of them inside devices. The chains (Calm, Pulse, Flow …) and the
  examples are presets. A device can be saved and inserted alone too.
- In the model a device is a module of type `device` whose `patch` is a v2 patch of its own, plus its exposed ports and
  macros. The engine and the compositor flatten it before running, so it costs nothing.

### 6b. The devices we offer

The owner asked which sets of modules are common and powerful enough to offer as devices, and agreed to nine, from
the preset study and other modular tools, plus one new standard module, **Threshold** (a luma key: only a picture's
bright parts). Each sits in the module picker, opens to show its modules, and has a few macros:

| Device | Inside | Macros |
|---|---|---|
| Feedback Tunnel | Feedback → Warp → Keep → Mix (lighten) | Zoom, Turn, Trail |
| Kick | Music's bass hits → Beat gate (above typical) → ADSR | Sensitivity, Punch, Decay |
| Breather | Loudness → Slow follower | Smoothness, Depth |
| Wanderer | Random (smooth), clocked to the bars | Speed, Range |
| Bloom | Blur → Threshold → Mix (add) | Threshold, Size, Strength |
| Scope | Music's sound → Waves → Glow → Tint | Height, Glow, Colour |
| Grid Particles | Grid → Instance → Shapes, distance into size | Count, Spacing, Pulse |
| Echo Trails | Feedback → Echo → Mix (lighten) | Length, Drift |
| Fractal Spiral | Feedback → Fold (kaleidoscope) → Warp → Keep | Folds, Turn, Depth |

### 7. One store

- The patch is the truth: kept, shared and saved in snapshots as a v2 patch.
- A v2 patch is only `{ modules, cables, macros, pins }`: modules with their knobs, cables between typed ports. No
  layers, loop or finish (§4). 0056's `Patch` shape becomes a view worked out from it while the old code still reads
  that shape.
- One store (`patch.svelte.ts`) holds every edit, with undo and redo. Both views read and write only through it.
- Old settings and snapshots (v1) are converted once through 0056's lossless `cratarrPatch`.

### 7b. One interface on every screen

The owner: "Phones get what everybody gets, no difference." The graph is the interface everywhere, phones included.
There is no separate phone rack. The panel's pages (a module's knobs, a cable's sheet, the pickers) open beside the
graph where there is room and as sheets over it where there is not. On a phone that means:

- touch throughout: pinch to zoom, drag to pan and to cable, tap to open, long press for the menu;
- semantic zoom so labels stay readable;
- the jump chips to reach a part quickly.

The graph is built for touch from the start, not adapted to it later.

How the two sit (the owner, 2026-10-20):

- **The graph is the page:** the rack drawn, filling the screen over the picture once the rack is open.
- **The panel is its inspector:** the views of what is picked (the top view: the rack's macros, the preset, the
  categories; a module's, a device's, a port's, a cable's; the pickers), as TouchDesigner's parameters beside its network.
  - Where there is room, a column at the right beside the graph.
  - On a phone, a sheet over the graph from below. It covers the graph, which stays as it is underneath (no resizing,
    no redrawing); it is as tall as its view needs, up to nearly the top, then scrolls inside.

### 7b2. Pins at four levels, several picked at once

The owner (2026-10-20): pins at four levels, a **knob**, a **module**, a **device** and a **category** (a device's
modules each by their own category inside it: §3b). **Multi-select** of modules, devices and cables (not knobs: "We
don't implement multi-select for knobs"), to pin or unpin them all at once; cables picked may be deleted together. This
undoes 70's "a long press on a cable gives Delete or Reroute (NOT multi-select)": a long press now also starts picking,
in the graph (SelectBar) and in the panel alike.

### 7b3. Framing in sight: Transform, and the Camera a device

The owner (2026-10-20): the auto-framing, exposure and colours become visible. A **Transform** module (across, up,
scale, turn: knobs to cable) moves light or a picture; the **Camera** is a device of it, Envelope Followers following
the Flame's new `x`, `y` and `size` outputs into the Transform ("Yes" to Transform with the Camera a device). The
Flame's `coverage` is there for the exposure; Music's `cover` output (a picture) gives its colours through a Tint.

### 7c. Sync groups: not in this version

The owner: "since I won't be sync grouping in the upcoming version, spend no time on this." Sync groups are handled at
the very end, after this version. Until then a v2 patch is not shared in a group, and the group features stay off for
the new visuals.

### 7d. One morph

One morph moves every knob of the patch, devices' included; today's separate look blend goes. How long a change
glides depends on what caused it (the owner agreed):

- a knob you turn: 0.15 s;
- a preset, Randomize or "Another look": 1.5 s;
- a new track, and Reset to seed: 6 s.

The more directly you cause it, the faster it follows. Rewired picture paths crossfade over the same time. A change
mid-glide starts from where the picture is. Steps change only at neutral. Undo jumps.

### 7e. The old architecture's oddities, set straight

The owner asked what quirks of the old architecture were left, and agreed to all nine ("make it so"). Each becomes
what a modular system does:

1. **Cables only add.** The "level" mode (a cable secretly multiplying its knob) goes. Multiplying is a visible
   **VCA** (a module: in × gain), as in every modular synth. Today's level cables convert to VCAs, exactly.
2. **A knob is only its value.** The column scales (a knob that also scaled every cable into it) fold into each
   cable's own amount, curves included.
3. **Steps take a visible trigger.** The comparator hidden in each step (above ½, ready again under ⅓, at most every
   2 s) becomes a **Gate** (our comparator) into the step's `next` trigger input.
4. **Bands are merged in sight.** A six-channel cable into a one-channel knob goes through a **Merge** (sum, mean or
   max: VCV's poly to mono), no longer silently averaged.
5. **No curve walks by itself.** A walking cable becomes a **Random** into that cable's depth.
6. **Pausing dims in sight.** Music gives `playing`. The dimming while paused is a cable from it (through a VCA) into
   Tone map's brightness, which can be unplugged.
7. **One knob, one meaning.** Haze splits into Curve's **gamma** and **black level**, the standard pair.
8. **One way of reading the hits.** After the switch, every cable that reads the hits raw ('absolute', as converted
   routes do) changes in one visible step to "against this track's typical", as new cables read them.
9. **Accent, not Flash.** The course-and-swing filter is renamed Accent, so "flash" means only the Screen's flash
   guard.

10. **A cable is amount, depth and channel** (the owner: "yes, amount, depth, channel. and most definitely amount as
    a curve"). Its amount is a curve (the plain amount the straight one); its depth what scales it (normalled to the
    Envelope); its channel one band of a polyphonic source. Everything else leaves the wire: its mode (item 1), its
    walk (item 5), its speed (Music gives a slow output beside each signal, `<signal>:slow`, or a Slow follower does
    it), and its reading (a reference a Music row, item 8: how a signal is read is settled at its source, not per
    wire).

Kept on purpose, documented: reduced motion (the system's accessibility setting, as the flash guard is safety), and
each module's own inner maths (the flame's parts and light ranges).

### 7f. No special modules: common patterns as devices

The owner, 2026-10-20: "yes, folded into music"; "yes, no special module needs to
remain"; on the follower's normal into every depth, "Yes, please"; on shipping the patterns as devices, "agree". These
supersede what §3, §4b, §4e and §7e say of Track, the Loudness and the depth normal.

- **Track folds into Music.** Music gives **New track** (a trigger) and **Track seed** (0..1, the same for the same
  track). No Track module, no Track logic.
- **A track's look is the dice's own Randomize** (the owner, 2026-10-20: "same randomize", not a Random per knob): a
  **Dice** module (the owner: beside Random, the value generator, it is called Dice), the panel's dice as a module, with a `trigger` and a `seed` input and its scope as a knob (Everything,
  Sources, Effects, Mixers, Modulators, Cables). It draws as the dice do: the flame's settings rolled together and
  checked until the flame draws something (`drawing`/`survey`), pins kept. Music's New track → Dice, seeded by
  Track seed, so every track keeps exactly today's look. The dice in the panel trigger the same Randomize with a fresh
  seed.
- **Track's pulse** ("all of it as a track starts, gone over its first beat") is New track → an ADSR whose release is
  about a beat.
- **No depth normal.** A cable has no depth unless one is cabled. The loudness scales once at the source: a
  polyphonic **VCA on Music's outputs** (the band rows through as channels), keyed by the Envelope Follower; everything
  from Music goes through it. Loudness's own row bypasses it, as a visible cable from Music directly. A modulator that
  followed the loudness gets the same VCA at its output. The Follower's out-0-when-paused goes: pausing is the one
  Playing → VCA → Tone map cable. A straight cable stays identical (cable × envelope = VCA(source) × cable). A
  **curved** cable (its value a curve) would not, the curve then reading the quieted signal; the owner: keep them
  exact. Such a cable reads Music before the VCA, through a visible **Shaper** (its curve) and then a VCA of its own
  keyed by the Follower. The picture harness proves the whole identical.
- **Common patterns ship as devices**, and the default patch is built from them. No special code: each opens, edits,
  and can be unmade into loose modules. toV2 emits them, so converted patches show them:
  - **Loudness Follow:** Envelope Follower → the VCA on Music's outputs (macros Intensity, Time);
  - **Track Look:** Music's New track and Track seed → the Dice (scope Everything, the pins kept);
  - **Pause Dim:** Music's Playing → VCA → Tone map's level;
  - **Calm:** Meter → a Ducker per busy kind → the knobs' gain inputs (§7e's sidechain, as a device; Wild its macro);
  - **Hit Lights:** Music's Hits → ADSR → the flame's lights (Linger);
  - Feedback Tunnel and Echo Trails are among the nine already (§6b).

### 8. Seeing it work

- Every picture node shows a live thumbnail.
- Every modulator shows a moving meter.
- Every knob's ring shows what moves it and where it is now (Bitwig).
- **Bypass** sends a module to its neutral through the morph; a second tap brings it back.

## Steps

Every step keeps the visuals as they are. Steps 2 and 3 are proved by a frame-by-frame comparison against today's
engine over recorded tracks (the precheck's harness), within a tolerance, before the old code goes.

| Step | What |
|---|---|
| 1 | **Model:** port types and declarations for every module type; the v2 patch as the truth; the store with undo; the v1 → v2 migration; the chain modules' declarations (3) with their normalled cables |
| 2 | **Engine:** the one evaluator over typed ports; aspects.ts's behaviours ported into the Music, Track, Envelope, Clock, Flash modules (the walks as Drifts) and the Flame's part logic; the Compressor; the control sources of §4b (ADSR and the Clock's outputs with a clocked LFO now; Sequencer and Looper later); the Grid's points and channels (§4c); the precheck, the recipe and the dice on the new engine; equivalence tests |
| 3 | **Picture graph:** the compositor over typed picture ports, lazy; Fold (one module for the grid's symmetries) and Instance (§4c); the Flame, Show and loop modules as nodes in it (render.ts driven by knob values, not AspectsNow); the Meter and the Screen's flash guard; Displace, Mask, Tint and Glow; the Mali cost of each |
| 4 | **Views:** typed wires and ports, normalled cables shown faint, rings, thumbnails and meters, bypass, undo; the graph on every screen, phones included, touch first (§7b) |
| 5 | **Devices and macros:** the device type (a module holding a patch, its exposed ports and macros), flattening for the engine and compositor, Show as a device, open/collapse in the rack, the whole frame's macros |
| 6 | **Retire:** aspects.ts, the routes' matrix, chain-cables.ts, the old settings keys |

Interfaces are fixed in step 1 (patch.ts) before steps 2 and 3 build on
them. Until step 1 lands, steps 2 and 3 work behind today's interfaces (the equivalence harness; the compositor's
graph walk).

### The switch (the owner, 2026-10-20: "the switch comes first")

The app on the v2 patch as its one truth, the engine and the compositor in its frame loop, the graph and the panel
both views of the store; then step 6; then a tablet check and a release. New modules (the Fluid's picture side, grids,
edges) wait. Small differences in the picture are accepted (the owner: "identical was not necessary"); curved cables
stay exact (§7f).

| Part | What |
|---|---|
| S1 | **The store as truth** (`visuals-patch.svelte.ts`): the patch (v2, unflattened) with undo and redo; loaded from its own key, else converted once from the saved settings (toV2); saved; this device's own settings (Quality, FPS, Brightness) apart from it; the phone driving the tablet sends the patch whole (sync groups are not in this version) |
| S2 | **The frame loop out of the page** (`visuals-loop.ts`) and **onto the store**: the engine on the flattened patch (a new object only when its structure changes), the look from the Flame's knobs as the engine moves them, the moments and the Dice from the engine's frame, the one morph on v2, the walks gone (Randoms in the patch); `precheckPatch` and the dice's try-and-pick |
| S3 | **The panel over the store** (`VisualsPanel.svelte` and v2 cable pages): pages by the five categories (rack.ts), a module's knobs from its type, cables as CableV2 (Delete, Reroute, the curve), pins (knobs and whole categories), Randomize's long-press scopes, Reset (to seed, everything), presets and snapshots as v2 patches with their pins |
| S4 | **The graph v2-only** (`patch` and `onchange` from the store); the harness over the switched app; the tablet check |
| S5 | **Step 6:** aspects.ts, routes.ts's matrix, chain-cables.ts, evaluate.ts on extras, drift.ts, recipe.ts, the v1 keys |

Order, so the page is never edited by two at once: S2's extraction of the loop first; meanwhile S1 and
S3 are built as new files; then the page mounts the store, the loop and the panel. The seam is the store's API:
`patch` (raw state), `change(f, key?)` (one undo step, coalesced by key), `replace(p)`, `undo()`, `redo()`.

After the switch (the owner, 2026-10-20), with the Fluid's picture side, the grids and the edges: a **Monitor** (In /
Out), its one polymorphic input (a picture, light, points as dots, a flow as arrows) shown live in its node and on its
page; a tap solos it, its signal full screen in place of the Screen's (TouchDesigner's viewers, VCV's Scope).

### Conversion details (step 3)

- A v1 effects stage samples once with every shape effect applied in slot order, then colours at the final
  coordinates. As one module per effect, the same picture needs the colour effects and Echo first, in slot order, then
  the shape effects in reverse slot order. Echo together with a shape effect cannot match exactly: it converts within
  a measured tolerance, shown to the owner.
- A module whose morph is bringing it back (target above neutral) keeps running while it is at neutral, so the flame
  coming back from blend 0 has its light already gathered, as today.

### Purity (the owner, 2026-10-29)

The owner: "the lack of logic is in the UI of the panel". The panel shows the engine's model, so the model is fixed and
the panel follows. The owner chose all four steps, in order:

1. **Model.**
   - A step knob names its choices (Knob `options`). The panel shows a yes or a no as a switch, two or three as a
     slim track, more in a sheet.
   - The panel's knobs are the graph's (`knobViews`): a band's six as one knob, with a slider a band on its page.
   - A shipped device's ports and modules are named by what they carry (device-names.ts), its loudness one input led
     to them all.
   - One `portsOfModule` and one `moduleName` serve every view.
2. **Hidden work into modules.**
   - The Flame's look, colours, Fit and place on the screen are its own (engine/flame-node.ts); its second morph is
     gone.
   - The Fluid's stirring is the engine's.
   - A step's Next is the engine's; a pinned knob is held where values are written (`applyKnobs`).
   - The dice live in engine/dice.ts, their limits the Calm's (not a macro by its id).
   - The engine reads no settings: a module the rack lacks does nothing. The frame loop reads no settings view.
3. **Truthful ports, §7e's leftovers.**
   - Done: the rack's Flash is Accent.
   - Left:
     - Curve's Haze as gamma and black level (item 7);
     - the Meter measuring what is cabled into it (it reads the screen);
     - the renderer's fixed ids (`layers`, `light-feedback`).
4. **The player's band glass on the one clock** (lib/trackPlace, the visuals' Music's): one curve for both its
   engines.

Tried and left out: one polyphonic VCA a source in Loudness Follow. The engine decides a VCA's channels from a list of
signals rather than its port's channels, so the picture changed.

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
