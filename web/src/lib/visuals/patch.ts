// The visuals as a patch (decision 0056): layers (a source, its modules, its own loop), a blend, the picture-level
// loop, finish, the modulators and the cables, each knob continuous with a neutral. Step 1: the model, the module types
// of today's pipeline, the Cratarr chain that is exactly that pipeline, and the way to and from today's settings
// (settings.svelte.ts), losing nothing, so the panel and the renderer can move over one at a time. Pure.

import { ASPECTS, BAND_SIGNALS, isRoute, routeKey, TRACK_SIGNALS, type Aspect } from './routes';
import { curveFrom, valueOf as keptValue, type RouteValue } from './curves';
import type { VisualSettings } from './settings.svelte';
import { COOLING_MOST, LIFT_MOST } from './fluid';

/** A knob: its range, its neutral (where its module does nothing) and its default. A level is a share its cables let
 *  through under its setting; a swing moves about it; a step changes only while its module is at neutral (0056's
 *  exceptions). A device knob is the device's: never morphed, shared or saved with a state. */
export type Knob = {
  id: string;
  label: string;
  min: number;
  max: number;
  neutral: number;
  default: number;
  kind: 'level' | 'swing' | 'step';
  device?: boolean;
  /** The kind of busyness it makes (0057): the cables into it run through the Compressor keyed by that kind. */
  busy?: Busy;
  /** A polyphonic knob (0057 §4b): one value a channel, kept as `<id>.<channel>` among its module's knobs. */
  channels?: number;
  /** A time (seconds): its gain stretches it (set ÷ gain), as a time CV does (0057 §7e). */
  time?: boolean;
  /** How its value follows its position (0057): 'exp' an exponential taper, as a frequency or time knob has (equal
   *  steps of position multiply the value). A macro moves a knob along its taper, so a macro on an exponential knob
   *  multiplies. */
  taper?: 'exp';
  /** Never rolled by Randomize (VCV's randomizeEnabled off): a choice of how, not a value. */
  randomize?: false;
  /** A step's choices by name, the first for its least step: what the panel and the graph say in place of its number
   *  (a yes or a no a switch, two or three a track, more a sheet). */
  options?: readonly string[];
};

/** Where a module can sit: a layer's source, a module on a layer (or after the blend), a loop (a layer's own, or the
 *  picture's), finish, the whole frame, or a modulator (no picture). */
/** The kinds of busyness a Meter measures and a Compressor is keyed by (meters.ts's busyness measures but the flashes, which
 *  the Screen's guard holds). */
export const BUSY = ['motion', 'shape', 'colour', 'zoom', 'looks'] as const;
export type Busy = (typeof BUSY)[number];

export type Place = 'source' | 'module' | 'loop' | 'finish' | 'frame' | 'modulator';

/** A module's type. A modulator's `inputs` are its ports (`in`, `trigger`): cables into one (to `{module, knob: port}`)
 *  add up, and the engine hands each port's value to its step as `knobs[port]`, so modulators chain. */
export type ModuleType = { type: string; label: string; place: Place; knobs: Knob[]; inputs?: string[] };

/** A module in a patch: its type and its knobs' values (the state). */
/** `label`: its own name, where its type's says too little (a device's; a VCA in Loudness Follow by the row it follows). */
export type Module = { id: string; type: string; knobs: Record<string, number>; label?: string };

/** A signal as a cable takes it: the music's (a band's, or the whole track's), against which reference its 1 =
 *  typical is measured (0056; this track by default), fast or slow. `value`: read that value as the signal's would
 *  be (an ADSR's strike, against the hits' typical) instead of the signal now; its speed does not apply. */
export type SignalRef = { signal: string; band?: number; reference?: 'track' | 'lately' | 'absolute'; speed?: 'fast' | 'slow'; value?: number };

/** A cable: from a signal or a modulator to a knob, its value as a route's (an amount, or a curve). */
export type Cable = {
  id: string;
  from: SignalRef | { modulator: string };
  to: { module: string; knob: string };
  value: RouteValue;
  mode: 'level' | 'swing';
};

export type Layer = { id: string; source: Module; modules: Module[]; loop: Module[]; blend: { mode: (typeof BLENDS)[number]; amount: number } };

export type Macro = { id: string; on: string; label: string; value: number; targets: { module: string; knob: string; from: number; to: number }[] };

export type Patch = {
  v: 1;
  frame: Module;
  layers: Layer[];
  loop: Module[];
  finish: Module[];
  modulators: Module[];
  cables: Cable[];
  macros: Macro[];
  /** What the dice, the walks and a track leave as it is: knobs (`module.knob`) and cables (`cable:<id>`). */
  pins: string[];
};

/** The patch beyond today's chain, as the settings keep it (`VisualSettings.extra`): the layers after the flame, the
 *  picture's own loop, the modulators, the cables today's routes cannot hold (from a modulator; to a knob no route
 *  column reaches; with a reference or a speed), and the macros. */
export type PatchExtra = { layers: Layer[]; loop: Module[]; modulators: Module[]; cables: Cable[]; macros: Macro[] };
export const emptyExtra = (): PatchExtra => ({ layers: [], loop: [], modulators: [], cables: [], macros: [] });

/** What a frame gives the modules that draw (step 4). */
export type FrameInfo = {
  dt: number;
  time: number;
  beat: { index: number; since: number; length: number; strength: number } | null;
  /** The music's signals, 1 = typical (0056). */
  signals: (ref: SignalRef) => number;
  /** The sound's recent samples (−1..1), where there are any (the page's own player, or the app's: 0056 step 5). */
  waveform: Float32Array | null;
  /** The drawing module's typed inputs this frame by port (0057 §1), where the compositor passes them: an `audio` port's
   *  samples. */
  inputs?: Readonly<Record<string, Float32Array | null>>;
};

/** A module that draws or changes a picture (step 4): into `out`, from `input` (the picture before it; none for a
 *  source), at `width` × `height`, with its knobs as the cables have moved them this frame. */
export interface PictureModule {
  draw(gl: WebGL2RenderingContext, input: WebGLTexture | null, out: WebGLFramebuffer | null, width: number, height: number, knobs: Record<string, number>, frame: FrameInfo): void;
  dispose(gl: WebGL2RenderingContext): void;
}

/** A modulator (0056; the modulators/): its value this frame from the signals and its knobs; 0 at neutral. */
export interface Modulator {
  step(dt: number, signals: (ref: SignalRef) => number, knobs: Record<string, number>): number;
}

// ---- Today's pipeline as module types --------------------------------------------------------------------------

/** A knob over a setting of today's: its range, label and kind; neutral where the module does nothing. */
const knob = (id: string, label: string, [min, max]: [number, number], def: number, kind: Knob['kind'], neutral = kind === 'swing' ? def : min, device = false): Knob => ({
  id,
  label,
  min,
  max,
  neutral,
  default: def,
  kind,
  ...(device ? { device } : {}),
});

/** Each band's times, one knob each (the flame's parts). */
const BAND_KNOBS = ['lighting', 'evolution', 'continuity'] as const;
const BAND_LABEL = { lighting: 'Linger', evolution: 'Evolution', continuity: 'Continuity' } as const;
const BANDS = 6;

/** Today's settings, each the knob of a module of the Cratarr chain: [module, knob]. The look's settings are the
 *  flame's; the display's are its show; the fluid's, the zooms', the trails' and the loop effect's are the flame's own
 *  loop; the effects are finish; the rest the whole frame's. */
export const SETTING_KNOB: Record<string, [string, string]> = {
  quality: ['frame', 'quality'],
  fps: ['frame', 'fps'],
  brightness: ['frame', 'brightness'],
  intensity: ['frame', 'intensity'],
  flash: ['frame', 'flash'],
  speed: ['frame', 'speed'],
  wild: ['frame', 'wild'],
  loose: ['frame', 'loose'],
  effects: ['frame', 'effects'],
  form: ['flame', 'form'],
  detail: ['flame', 'detail'],
  twist: ['flame', 'twist'],
  warp: ['flame', 'warp'],
  symmetry: ['flame', 'symmetry'],
  mirror: ['flame', 'mirror'],
  final: ['flame', 'final'],
  lensSides: ['flame', 'lensSides'],
  lensDepth: ['flame', 'lensDepth'],
  family: ['flame', 'family'],
  hue: ['flame', 'hue'],
  lightness: ['flame', 'lightness'],
  vividness: ['flame', 'vividness'],
  spread: ['flame', 'spread'],
  purity: ['flame', 'purity'],
  shape: ['flame', 'shape'],
  rotation: ['flame', 'rotation'],
  lights: ['flame', 'lights'],
  presence: ['flame', 'presence'],
  size: ['flame', 'zoom'],
  glow: ['show', 'glow'],
  saturation: ['show', 'saturation'],
  smoothing: ['show', 'smoothing'],
  haze: ['show', 'haze'],
  waves: ['show', 'waves'],
  fluid: ['fluid', 'fluid'],
  stir: ['fluid', 'stir'],
  flow: ['fluid', 'flow'],
  swirl: ['fluid', 'swirl'],
  facets: ['fluid', 'facets'],
  lattice: ['fluid', 'lattice'],
  tunnel: ['tunnel', 'tunnel'],
  dive: ['dive', 'dive'],
  trails: ['trails', 'trails'],
  fxLoop: ['loop-effect', 'effect'],
  fxLoopAmount: ['loop-effect', 'amount'],
  fxLoopK1: ['loop-effect', 'k1'],
  fxLoopK2: ['loop-effect', 'k2'],
  fx1: ['fx1', 'effect'],
  fx1Amount: ['fx1', 'amount'],
  fx1K1: ['fx1', 'k1'],
  fx1K2: ['fx1', 'k2'],
  fx2: ['fx2', 'effect'],
  fx2Amount: ['fx2', 'amount'],
  fx2K1: ['fx2', 'k1'],
  fx2K2: ['fx2', 'k2'],
  fx3: ['fx3', 'effect'],
  fx3Amount: ['fx3', 'amount'],
  fx3K1: ['fx3', 'k1'],
  fx3K2: ['fx3', 'k2'],
};

/** Each route column's knob: where today's routes into it become cables. Most are their setting's knob; the Glow
 *  column's is Glow's (its setting is `glow`, routes.ts COLUMN_SCALES); Look's and Behaviour's walks are the frame's. */
export const ASPECT_KNOB: Record<Aspect, [string, string]> = {
  shape: ['flame', 'shape'],
  rotation: ['flame', 'rotation'],
  lights: ['flame', 'lights'],
  hue: ['flame', 'hue'],
  spread: ['flame', 'spread'],
  presence: ['flame', 'presence'],
  brightness: ['show', 'glow'],
  size: ['flame', 'zoom'],
  saturation: ['show', 'saturation'],
  trails: ['trails', 'trails'],
  smoothing: ['show', 'smoothing'],
  haze: ['show', 'haze'],
  fluid: ['fluid', 'fluid'],
  stir: ['fluid', 'stir'],
  flow: ['fluid', 'flow'],
  swirl: ['fluid', 'swirl'],
  waves: ['show', 'waves'],
  tunnel: ['tunnel', 'tunnel'],
  dive: ['dive', 'dive'],
  effects: ['frame', 'effects'],
  facets: ['fluid', 'facets'],
  look: ['frame', 'lookWalk'],
  behaviour: ['frame', 'behaviourWalk'],
  family: ['flame', 'family'],
  form: ['flame', 'form'],
  symmetry: ['flame', 'symmetry'],
  mirror: ['flame', 'mirror'],
  final: ['flame', 'final'],
  lattice: ['fluid', 'lattice'],
  fx1: ['fx1', 'effect'],
  fx1Amount: ['fx1', 'amount'],
  fx1K1: ['fx1', 'k1'],
  fx1K2: ['fx1', 'k2'],
  fx2: ['fx2', 'effect'],
  fx2Amount: ['fx2', 'amount'],
  fx2K1: ['fx2', 'k1'],
  fx2K2: ['fx2', 'k2'],
  fx3: ['fx3', 'effect'],
  fx3Amount: ['fx3', 'amount'],
  fx3K1: ['fx3', 'k1'],
  fx3K2: ['fx3', 'k2'],
  fxLoop: ['loop-effect', 'effect'],
  fxLoopAmount: ['loop-effect', 'amount'],
  fxLoopK1: ['loop-effect', 'k1'],
  fxLoopK2: ['loop-effect', 'k2'],
};

/** The columns whose routes are levels (a share under the setting); every other column's swing about it (aspects.ts). */
const LEVEL_ASPECTS = new Set<Aspect>(['fluid', 'smoothing', 'haze', 'tunnel', 'dive', 'effects', 'facets', 'fx1Amount', 'fx2Amount', 'fx3Amount', 'fxLoopAmount']);

/** The knobs that take a step at a time (0056's exceptions: changed only at their module's neutral). The lens, the
 *  mirror and the colour family will mix by the flame's own maths (step 6); until then they are steps as today. */
const STEP_KNOBS = new Set(['form', 'symmetry', 'mirror', 'final', 'family', 'lattice', 'effect']);
/** The device's knobs. */
const DEVICE_KNOBS = new Set(['quality', 'fps', 'brightness']);
/** The column scales' and the levels' neutral is their least (the music leaves it at rest, or nothing of the module);
 *  the look's has none of its own: a source's neutral is its layer's blend at 0. */
const LOOK_KNOBS = new Set(['form', 'detail', 'twist', 'warp', 'symmetry', 'mirror', 'final', 'lensSides', 'lensDepth', 'family', 'hue', 'lightness', 'vividness', 'spread', 'purity']);

const LABELS: Record<string, string> = {
  quality: 'Quality', fps: 'FPS', brightness: 'Brightness', intensity: 'Intensity', flash: 'Flash', speed: 'Speed', wild: 'Wild',
  loose: 'Loose', effects: 'Effects', lookWalk: 'Look walk', behaviourWalk: 'Behaviour walk', form: 'Form', detail: 'Detail',
  twist: 'Twist', warp: 'Warp', symmetry: 'Symmetry', mirror: 'Mirror', final: 'Lens', lensSides: 'Lens sides', lensDepth: 'Lens depth', family: 'Colours', hue: 'Hue',
  lightness: 'Lightness', vividness: 'Vividness', spread: 'Spread', purity: 'Purity', shape: 'Shape', rotation: 'Rotation',
  lights: 'Lights', presence: 'Presence', size: 'Size', glow: 'Glow', saturation: 'Saturation', smoothing: 'Smoothing',
  haze: 'Haze', waves: 'Waves', fluid: 'Fluid', stir: 'Stir', flow: 'Flow', swirl: 'Swirl', facets: 'Facets',
  lattice: 'Lattice', zoom: 'Zoom', tunnel: 'Tunnel', dive: 'Dive', trails: 'Trails', effect: 'Effect', amount: 'Amount',
  k1: 'Control 1', k2: 'Control 2',
};

/** The module types of today's pipeline, their knobs from today's settings' ranges and defaults. */
export function cratarrTypes(ranges: Record<string, [number, number]>, defaults: VisualSettings): ModuleType[] {
  const byModule = new Map<string, Knob[]>();
  const add = (module: string, k: Knob) => (byModule.get(module) ?? byModule.set(module, []).get(module)!).push(k);
  for (const [setting, [module, id]] of Object.entries(SETTING_KNOB)) {
    const def = (defaults as unknown as Record<string, number>)[setting];
    // The look's knobs swing about where they are set (their neutral is their own setting); every other is a level,
    // nothing of it at its least.
    const kind: Knob['kind'] = STEP_KNOBS.has(id) ? 'step' : LOOK_KNOBS.has(setting) ? 'swing' : 'level';
    const neutral = LOOK_KNOBS.has(setting) ? def : ranges[setting][0];
    add(module, knob(id, LABELS[id] ?? id, ranges[setting], def, kind, neutral, DEVICE_KNOBS.has(id)));
  }
  // The walks: cable ends with no setting of their own (Look's and Behaviour's columns).
  add('frame', knob('lookWalk', LABELS.lookWalk, [0, 1], 0, 'swing', 0));
  add('frame', knob('behaviourWalk', LABELS.behaviourWalk, [0, 1], 0, 'swing', 0));
  for (const key of BAND_KNOBS)
    for (let b = 0; b < BANDS; b++)
      add('flame', { ...knob(`${key}.${b}`, `${BAND_LABEL[key]} ${b + 1}`, ranges[key], defaults[key][b], 'swing', defaults[key][b]), ...(key === 'evolution' ? { time: true } : {}) });
  // Fit (step 7): how far the Flame frames itself, centred and sized to the view as its points fall (1, as it always
  // did; 0, where they fall). Not the dice's: a roll would lose the flame off the screen.
  add('flame', { ...knob('fit', 'Fit', [0, 1], 1, 'level', 0), randomize: false });
  // Fire (0.9.18; the owner via fire-e, 2026-10-29: in the Fluid, not a module of its own): Lift,
  // how hard the densest light rises by itself (heights a second, each second); Open walls, the floor and top from
  // sealed to open; Cooling, how fast the light burns out as it climbs (fluid.ts Flow). At 0, the Fluid as it was. Not
  // the dice's: a roll would open a box that was built sealed.
  add('fluid', { ...knob('lift', 'Lift', [0, LIFT_MOST], 0, 'level', 0), randomize: false });
  add('fluid', { ...knob('open', 'Open walls', [0, 1], 0, 'level', 0), randomize: false });
  add('fluid', { ...knob('cooling', 'Cooling', [0, COOLING_MOST], 0, 'level', 0), randomize: false });
  const type = (id: string, label: string, place: Place): ModuleType => ({ type: id, label, place, knobs: byModule.get(id) ?? [] });
  return [
    type('frame', 'Whole frame', 'frame'),
    type('flame', 'Flame', 'source'),
    type('show', 'Show', 'module'),
    type('fluid', 'Fluid', 'loop'),
    type('tunnel', 'Tunnel', 'loop'),
    type('dive', 'Dive', 'loop'),
    type('trails', 'Trails', 'loop'),
    type('loop-effect', 'Loop effect', 'loop'),
    { ...type('fx1', 'Effect 1', 'finish'), type: 'kaleidoscope-engine' },
    { ...type('fx2', 'Effect 2', 'finish'), type: 'eyedo-engine' },
    { ...type('fx3', 'Effect 3', 'finish'), type: 'hydra-engine' },
  ];
}

// ---- The patch beyond today's chain, as kept: checked -------------------------------------------------------------

/** The Cratarr chain's modules and their knobs (cables from the extras may reach them). */
const CHAIN_KNOBS = (() => {
  const out = new Map<string, Set<string>>();
  const add = (m: string, k: string) => (out.get(m) ?? out.set(m, new Set()).get(m)!).add(k);
  for (const [m, k] of Object.values(SETTING_KNOB)) add(m, k);
  add('frame', 'lookWalk');
  add('frame', 'behaviourWalk');
  for (const key of BAND_KNOBS) for (let b = 0; b < BANDS; b++) add('flame', `${key}.${b}`);
  return out;
})();
const ID = /^[a-z0-9][a-z0-9-]{0,39}$/;
// A layer's blends: Mix's first modes, by the same names and places (ports.ts MIX_MODES).
export const BLENDS = ['add', 'screen', 'over', 'multiply', 'window'] as const;
const clampTo = (v: unknown, lo: number, hi: number, d: number) => (typeof v === 'number' && Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : d);
const isObject = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);
const listOf = (v: unknown): unknown[] => (Array.isArray(v) ? v : []);

/** The extras as kept or come from the group, checked against the module types there are (`typeOf`): a module of a
 *  type there is not, in a place it cannot sit, or with an id taken, is left out; its knobs brought into their ranges
 *  (a missing one at its default); a cable whose ends are not there, or a macro's target, left out. */
export function extraFrom(kept: unknown, typeOf: (type: string) => ModuleType | undefined): PatchExtra {
  const out = emptyExtra();
  if (!isObject(kept)) return out;
  const taken = new Set<string>([...CHAIN_KNOBS.keys(), 'flame', 'loop', 'finish']);
  const knobsOf = new Map<string, Set<string>>(CHAIN_KNOBS);
  const moduleFrom = (v: unknown, places: Place[]): Module | null => {
    if (!isObject(v) || typeof v.id !== 'string' || !ID.test(v.id) || taken.has(v.id) || typeof v.type !== 'string') return null;
    const t = typeOf(v.type);
    if (!t || !places.includes(t.place)) return null;
    const given = isObject(v.knobs) ? v.knobs : {};
    const knobs = Object.fromEntries(t.knobs.map((k) => [k.id, clampTo(given[k.id], k.min, k.max, k.default)]));
    taken.add(v.id);
    knobsOf.set(v.id, new Set([...t.knobs.map((k) => k.id), ...(t.inputs ?? [])]));
    return { id: v.id, type: t.type, knobs };
  };
  const modulesFrom = (v: unknown, places: Place[]) => listOf(v).map((m) => moduleFrom(m, places)).filter((m): m is Module => m !== null);
  for (const l of listOf(kept.layers)) {
    if (!isObject(l) || typeof l.id !== 'string' || !ID.test(l.id) || taken.has(l.id)) continue;
    taken.add(l.id);
    const source = moduleFrom(l.source, ['source']);
    if (!source) continue;
    const blend = isObject(l.blend) ? l.blend : {};
    const mode = (BLENDS as readonly unknown[]).includes(blend.mode) ? (blend.mode as Layer['blend']['mode']) : 'add';
    out.layers.push({ id: l.id, source, modules: modulesFrom(l.modules, ['module']), loop: modulesFrom(l.loop, ['loop']), blend: { mode, amount: clampTo(blend.amount, 0, 1, 1) } });
  }
  out.loop = modulesFrom(kept.loop, ['loop']);
  out.modulators = modulesFrom(kept.modulators, ['modulator']);
  const modulatorIds = new Set(out.modulators.map((m) => m.id));
  const cableIds = new Set<string>();
  for (const c of listOf(kept.cables)) {
    if (!isObject(c) || typeof c.id !== 'string' || c.id.length > 80 || cableIds.has(c.id) || !isObject(c.from) || !isObject(c.to)) continue;
    // Show's Size is the Flame's zoom (0057): a cable kept before into show.size goes there.
    const to = c.to.module === 'show' && c.to.knob === 'size' ? { ...c.to, module: 'flame', knob: 'zoom' } : c.to;
    if (typeof to.module !== 'string' || typeof to.knob !== 'string' || !knobsOf.get(to.module)?.has(to.knob)) continue;
    let from: Cable['from'] | null = null;
    if (typeof c.from.modulator === 'string') from = modulatorIds.has(c.from.modulator) ? { modulator: c.from.modulator } : null;
    else if (typeof c.from.signal === 'string') {
      const track = (TRACK_SIGNALS as readonly string[]).includes(c.from.signal);
      const band = (BAND_SIGNALS as readonly string[]).includes(c.from.signal);
      const b = c.from.band;
      if (track || (band && typeof b === 'number' && Number.isInteger(b) && b >= 0 && b < BANDS)) {
        const ref: SignalRef = { signal: c.from.signal, ...(band ? { band: b as number } : {}) };
        if (c.from.reference === 'track' || c.from.reference === 'lately' || c.from.reference === 'absolute') ref.reference = c.from.reference;
        if (c.from.speed === 'fast' || c.from.speed === 'slow') ref.speed = c.from.speed;
        from = ref;
      }
    }
    if (!from) continue;
    const curve = typeof c.value === 'number' ? null : curveFrom(c.value);
    if (typeof c.value !== 'number' && !curve) continue;
    cableIds.add(c.id);
    const value: RouteValue = typeof c.value === 'number' ? clampTo(c.value, -1, 1, 0) : keptValue(curve!);
    out.cables.push({ id: c.id, from, to: { module: to.module, knob: to.knob }, value, mode: c.mode === 'level' ? 'level' : 'swing' });
  }
  for (const m of listOf(kept.macros)) {
    if (!isObject(m) || typeof m.id !== 'string' || !ID.test(m.id) || typeof m.on !== 'string') continue;
    const targets = listOf(m.targets)
      .filter((t): t is Record<string, unknown> => isObject(t) && typeof t.module === 'string' && typeof t.knob === 'string' && !!knobsOf.get(t.module)?.has(t.knob))
      .map((t) => ({ module: t.module as string, knob: t.knob as string, from: clampTo(t.from, -1e6, 1e6, 0), to: clampTo(t.to, -1e6, 1e6, 1) }));
    out.macros.push({ id: m.id, on: m.on, label: typeof m.label === 'string' ? m.label.slice(0, 40) : 'Macro', value: clampTo(m.value, 0, 1, 0), targets });
  }
  return out;
}

// ---- The Cratarr chain: today's settings as a patch, and back ---------------------------------------------------

const MODULE_TYPE: Record<string, string> = {
  frame: 'frame', flame: 'flame', show: 'show', fluid: 'fluid', tunnel: 'tunnel', dive: 'dive', trails: 'trails',
  'loop-effect': 'loop-effect', fx1: 'kaleidoscope-engine', fx2: 'eyedo-engine', fx3: 'hydra-engine',
};

/** A pin as kept today (`detail`, `lighting:2`, `route:<key>`) as the patch keeps it (`flame.detail`,
 *  `flame.lighting.2`, `cable:<key>`). */
function pinOf(pin: string): string | null {
  if (pin.startsWith('route:')) return `cable:${pin.slice(6)}`;
  const band = /^(lighting|evolution|continuity):(\d)$/.exec(pin);
  if (band) return `flame.${band[1]}.${band[2]}`;
  const k = SETTING_KNOB[pin];
  return k ? `${k[0]}.${k[1]}` : null;
}
function pinFrom(pin: string): string | null {
  if (pin.startsWith('cable:')) return `route:${pin.slice(6)}`;
  const band = /^flame\.(lighting|evolution|continuity)\.(\d)$/.exec(pin);
  if (band) return `${band[1]}:${band[2]}`;
  for (const [setting, [m, k]] of Object.entries(SETTING_KNOB)) if (`${m}.${k}` === pin) return setting;
  return null;
}

/** Today's settings as the Cratarr chain: the flame layer (its show, its own loop), finish, the frame; each route a
 *  cable. Losing nothing: `settingsOf(cratarrPatch(s))` is `s` again. */
export function cratarrPatch(s: VisualSettings): Patch {
  const modules = new Map<string, Module>();
  const m = (id: string) => modules.get(id) ?? modules.set(id, { id, type: MODULE_TYPE[id], knobs: {} }).get(id)!;
  for (const [setting, [module, id]] of Object.entries(SETTING_KNOB)) m(module).knobs[id] = (s as unknown as Record<string, number>)[setting];
  m('frame').knobs.lookWalk = 0;
  m('frame').knobs.behaviourWalk = 0;
  for (const key of BAND_KNOBS) for (let b = 0; b < BANDS; b++) m('flame').knobs[`${key}.${b}`] = s[key][b];
  const cables: Cable[] = [];
  for (const [key, value] of Object.entries(s.routes)) {
    if (!isRoute(key, BANDS)) continue;
    const [signal, band, aspect] = key.split('|') as [string, string, Aspect];
    const [module, k] = ASPECT_KNOB[aspect];
    cables.push({
      id: key,
      from: band === '-' ? { signal } : { signal, band: Number(band) },
      to: { module, knob: k },
      value,
      mode: LEVEL_ASPECTS.has(aspect) ? 'level' : 'swing',
    });
  }
  const extra = s.extra ?? emptyExtra();
  return {
    v: 1,
    frame: m('frame'),
    layers: [
      { id: 'flame', source: m('flame'), modules: [m('show')], loop: [m('fluid'), m('tunnel'), m('dive'), m('trails'), m('loop-effect')], blend: { mode: 'add', amount: s.flameLayer ?? 1 } },
      ...extra.layers,
    ],
    loop: extra.loop,
    finish: [m('fx1'), m('fx2'), m('fx3')],
    modulators: extra.modulators,
    cables: [...cables, ...extra.cables],
    macros: extra.macros,
    pins: s.pins.map(pinOf).filter((p): p is string => p !== null),
  };
}

/** Every module of a patch, by id. */
export function modulesOf(p: Patch): Map<string, Module> {
  const out = new Map<string, Module>([[p.frame.id, p.frame]]);
  for (const l of p.layers) for (const x of [l.source, ...l.modules, ...l.loop]) out.set(x.id, x);
  for (const x of [...p.loop, ...p.finish, ...p.modulators]) out.set(x.id, x);
  return out;
}

/** The Cratarr chain back as today's settings (what the renderer and the panel read until steps 2 and 4). A cable
 *  from a signal to a knob that a route column reaches is that route; anything else (a modulator's cable, a module
 *  today's pipeline has not) is left out, as today's settings cannot hold it. */
export function settingsOf(p: Patch, base: VisualSettings): VisualSettings {
  const mods = modulesOf(p);
  const out = { ...base, lighting: [...base.lighting], evolution: [...base.evolution], continuity: [...base.continuity], routes: {} as Record<string, RouteValue> } as VisualSettings;
  for (const [setting, [module, id]] of Object.entries(SETTING_KNOB)) {
    const v = mods.get(module)?.knobs[id];
    if (typeof v === 'number') (out as unknown as Record<string, number>)[setting] = v;
  }
  for (const key of BAND_KNOBS)
    for (let b = 0; b < BANDS; b++) {
      const v = mods.get('flame')?.knobs[`${key}.${b}`];
      if (typeof v === 'number') out[key][b] = v;
    }
  // A cable a route can hold (from a signal, plainly, to a knob a route column reaches) is that route; every other
  // is kept with the extras.
  const aspectOf = new Map(ASPECTS.map((a) => [ASPECT_KNOB[a].join('.'), a]));
  const extraCables: Cable[] = [];
  for (const c of p.cables) {
    const aspect = aspectOf.get(`${c.to.module}.${c.to.knob}`);
    const key = 'signal' in c.from && aspect && !c.from.reference && !c.from.speed ? routeKey(c.from.signal as never, c.from.band ?? null, aspect) : null;
    if (key && isRoute(key, BANDS) && key === c.id) out.routes[key] = c.value;
    else extraCables.push(c);
  }
  const flame = p.layers.find((l) => l.id === 'flame');
  if (flame) out.flameLayer = flame.blend.amount;
  out.extra = { layers: p.layers.filter((l) => l.id !== 'flame'), loop: p.loop, modulators: p.modulators, cables: extraCables, macros: p.macros };
  out.pins = p.pins.map(pinFrom).filter((x): x is string => x !== null);
  return out;
}
