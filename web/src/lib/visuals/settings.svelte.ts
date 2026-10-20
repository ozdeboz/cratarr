// The visuals' settings (decision 0046, amended; 0050): how much the device draws (Quality and the frame rate: ceilings
// the automatic step-down stays under), how the show behaves (intensity, flash, speed, brightness, the grain and haze), the look (the preset: the flame's form, detail, twist, warp, symmetry, mirror, lens, colour family, hue,
// lightness, vividness, spread and purity, which a track's key or the dice set and the picture follows; one the owner
// moved is pinned), each band's times (how long its light rings, how quickly its part evolves, and how much of its
// motion comes in steps on its own transients), and the routes: how much each of the music's signals drives each aspect
// of the picture (routes.ts).
// Changed live from the visuals' panel and kept on this device (a phone and a tablet want different ones); a browser
// that keeps nothing (a private window) starts from the defaults each time.

import { FORMS, LENS_DEPTH, LENS_SIDES, LENSES, PRESET_KEYS, type Preset } from './flame';
import { ENGINE_EFFECTS, LOOP_EFFECTS } from './effects';
import { defaultRoutes, isRoute, keyNow, SCALE_KEYS, type RouteValue } from './routes';
import { curveFrom, valueOf } from './curves';
import { QUALITY } from './quality';
import { emptyExtra, extraFrom, type PatchExtra } from './patch';
import { extraType } from './registry';

const KEY = 'cratarr-visuals';
/** The kept settings' form: 3 since 65 (Quality, Haze, Family as Colours: what they replace is dropped, not converted,
 *  by the owner's choice); 2 since 64 (Fluid on its finer scale); none before. */
const FORM = 3;
/** Fluid's whole range: this much of the fluid's own. In 63 the slider went to all of it, and the owner wanted only its
 *  first few percent ("between 0 and 5% fluid dynamics is about all I want … or maybe 10% should be 100%", 2026-10-18).
 *  64.1 tried a fifth of that (its top washed the owner's phone out: a route took Fluid past it on every punch); the
 *  owner kept 64's ("restore 64's full range, otherwise it'll probably not be good"): with the slider now the most a
 *  route gives Fluid, and Stir, Flow and Swirl as routes, the washouts are tamed where they come from. */
export const FLUID_FULL = 0.1;
/** A Fluid kept or saved before 64, on its finer scale. */
export const fluidFrom63 = (v: number) => Math.min(1, v / FLUID_FULL);
/** The bands the routes are kept for (the server's six). */
export const BANDS = 6;

export type VisualSettings = Preset & {
  /** How much the device may draw at most (0..1, 65): its points, steps and resolution, as quality.ts `ceilingOf` maps
   *  it (60 % the defaults before 65); the device's pace takes it lower where frames come late. */
  quality: number;
  /** The most frames a second. */
  fps: number;
  /** How much the music's loudness sets the show's intensity (0: always all, 1: as loud as it is). */
  intensity: number;
  /** How much of the picture's quick changes show (its lights, glow, size and colour on hits and beats): 0 only their
   *  slow course, 1 all of them, 2 twice as much. */
  flash: number;
  /** Motion: the turning and its steps (0..2). */
  speed: number;
  /** Calm ↔ Wild (0..1): how much busier than its budgets the sidechaining lets the show be (engine/sidechain.ts), half as much to
   *  twice as much; as the standing limits at its middle. */
  wild: number;
  /** The most brightness shown, after everything (0.1..1). */
  brightness: number;
  /** The grain: how far the sparse parts are blurred (0: the raw grain; 0.5: three pixels, as before; 0..1). */
  smoothing: number;
  /** Haze (−1..1, 65, decision 0050): more of the faint mist lifted into view (the display's gamma from 3 up to 6), or
   *  less, the faint cut to black (the gamma down to 1.5, a floor up to a fifth of the flame's mean); 0 as the defaults
   *  were before 65. Its routes take it toward 0, never past what is set. aspects.ts. */
  haze: number;
  /** How much the light flows (0..1, decision 0047): 0 its trails only fade where they were drawn, as before (nothing of
   *  the fluid made); above it they are carried along by a flow the music stirs, like smoke. Its route moves it. Its
   *  whole range is `FLUID_FULL` of the fluid's own. */
  fluid: number;
  /** The infinite zooms' speeds (0..1 of their most, 65; zoom.ts): Tunnel streams the light outward for ever, Dive
   *  zooms into one part of the flame for ever; 0 off, both may run. Levels, as Fluid is: their routes the conditions. */
  tunnel: number;
  /** The most of the look's effects (0..1, 65): the Effects column's routes are its conditions, as Fluid's. */
  effects: number;
  /** How much of the flame layer shows (0..1, 70): the flame is a layer like the others, and at 0 it is gone, as it
   *  is after Start from scratch (the owner: "nothing at all"). The flame layer's blend amount in the patch. */
  flameLayer: number;
  /** A double tap's roll stands (1): the group is held back at Wild's most, up to twice the limits, till the next roll, an
   *  undo, Reset or a recall (65; the owner: "sync the group properly"). Not a slider: the dice set it. */
  loose: number;
  dive: number;
  /** Each band's part's weight in the drawing, the scale of its Presence column (65; routes.ts `COLUMN_SCALES`), and
   *  the slow giant waves' (the Waves column). */
  presence: number;
  waves: number;
  /** Each column of the routes that has no setting of its own, its scale (0..1, 64.2; the owner: "is it impossible to
   *  have each column their own slider?"): what its routes give it, times this. 1 as before; 0 the music leaves it at
   *  rest. routes.ts `COLUMN_SCALES`. */
  shape: number;
  rotation: number;
  lights: number;
  glow: number;
  size: number;
  saturation: number;
  trails: number;
  stir: number;
  flow: number;
  swirl: number;
  /** The look's settings the owner moved: kept as they are when a track or the dice load another preset, until
   *  unpinned. */
  pins: string[];
  /** Each band's (sub, bass, low mid, mid, upper mid, air) light after a transient: how long it rings (seconds). */
  lighting: number[];
  /** Each band's loudness evolving its transform: how long it takes to follow (seconds). */
  evolution: number[];
  /** Each band's motion and evolution: 0 in steps on its own transients, 1 steady, between them a share of each. */
  continuity: number[];
  /** Each route's amount (-1..1) by its key (`routeKey`); one not here is 0. */
  routes: Record<string, RouteValue>;
  /** The patch beyond today's chain (decision 0056, patch.ts): the layers after the flame, the picture's own loop, the
   *  modulators, the cables routes cannot hold, the macros. */
  extra: PatchExtra;
};

const touch = typeof matchMedia !== 'undefined' && matchMedia('(pointer: coarse)').matches;

// Many points in few steps: each step is a transform feedback, which ANGLE on Direct3D 11 (Chrome and Edge on Windows)
// flushes after; the same points a frame in 3 steps rather than 12 took 4.0 ms against 4.8 on an RTX 4060 (2026-10-17).
export const DEFAULTS: VisualSettings = {
  quality: QUALITY,
  fps: touch ? 30 : 60,
  intensity: 1,
  flash: 1,
  speed: 1,
  wild: 0.5,
  brightness: 1,
  smoothing: 0.5,
  haze: 0,
  fluid: 0,
  tunnel: 0,
  effects: 1,
  flameLayer: 1,
  loose: 0,
  dive: 0,
  presence: 1,
  waves: 1,
  shape: 1,
  rotation: 1,
  lights: 1,
  glow: 1,
  size: 1,
  saturation: 1,
  trails: 1,
  stir: 1,
  flow: 1,
  swirl: 1,
  // The look: the playing track's preset replaces these (those not pinned) as soon as the visuals show.
  form: 0,
  detail: 0.5,
  twist: 0.7,
  warp: 0.8,
  symmetry: 1,
  mirror: 0,
  final: 0,
  family: 0,
  hue: 0,
  lightness: 0.5,
  vividness: 0.5,
  spread: 1,
  // Six parts mixing their colours go grey: mostly each its band's own.
  purity: 0.75,
  fx1: 0,
  fx1Amount: 0,
  fx2: 0,
  fx2Amount: 0,
  fx3: 0,
  fx3Amount: 0,
  fxLoop: 0,
  fxLoopAmount: 0,
  fx1K1: 0.5,
  fx1K2: 0.5,
  fx2K1: 0.5,
  fx2K2: 0.5,
  fx3K1: 0.5,
  fx3K2: 0.5,
  fxLoopK1: 0.5,
  fxLoopK2: 0.5,
  facets: 0,
  lattice: 0,
  lensSides: LENS_SIDES,
  lensDepth: LENS_DEPTH,
  pins: [],
  // A kick rings longer than a hat.
  lighting: [0.2, 0.18, 0.15, 0.13, 0.1, 0.08],
  evolution: [0.25, 0.25, 0.25, 0.25, 0.25, 0.25],
  continuity: [0.5, 0.5, 0.5, 0.5, 0.5, 0.5],
  routes: defaultRoutes(BANDS),
  extra: emptyExtra(),
};

type Ranged = Exclude<keyof VisualSettings, 'pins' | 'extra'>;

/** Each setting's range, as its slider has it (the bands' times in seconds, though their sliders double). A kept value
 *  outside it (from a version whose range was another) is brought into it. */
export const RANGES: Record<Ranged, [number, number]> = {
  quality: [0, 1],
  fps: [10, 60],
  intensity: [0, 1],
  flash: [0, 2],
  speed: [0, 2],
  wild: [0, 1],
  brightness: [0.1, 1],
  smoothing: [0, 1],
  haze: [-1, 1],
  fluid: [0, 1],
  tunnel: [0, 1],
  dive: [0, 1],
  presence: [0, 1],
  waves: [0, 1],
  shape: [0, 1],
  rotation: [0, 1],
  lights: [0, 1],
  glow: [0, 1],
  size: [0, 1],
  saturation: [0, 1],
  trails: [0, 1],
  stir: [0, 1],
  flow: [0, 1],
  swirl: [0, 1],
  form: [0, FORMS - 1],
  detail: [0, 1],
  twist: [0, 1],
  warp: [0, 1],
  symmetry: [1, 8],
  mirror: [0, 1],
  final: [0, LENSES.length],
  // Family is the Colours setting (65): the preset's four (Spectrum, Arc, Two tones, Three), then Cover and Fire.
  family: [0, 5],
  hue: [0, 1],
  lightness: [0, 1],
  vividness: [0, 1],
  spread: [0.25, 2.5],
  purity: [0, 1],
  fx1: [0, ENGINE_EFFECTS.kaleidoscope.length],
  fx1Amount: [0, 1],
  fx2: [0, ENGINE_EFFECTS.eyedo.length],
  fx2Amount: [0, 1],
  fx3: [0, ENGINE_EFFECTS.hydra.length],
  fx3Amount: [0, 1],
  fxLoop: [0, LOOP_EFFECTS.length],
  fxLoopAmount: [0, 1],
  fx1K1: [0, 1],
  fx1K2: [0, 1],
  fx2K1: [0, 1],
  fx2K2: [0, 1],
  fx3K1: [0, 1],
  fx3K2: [0, 1],
  fxLoopK1: [0, 1],
  fxLoopK2: [0, 1],
  facets: [0, 1],
  lattice: [0, 1],
  lensSides: [3, 48],
  lensDepth: [0, 1],
  effects: [0, 1],
  flameLayer: [0, 1],
  loose: [0, 1],
  lighting: [0.02, 1],
  evolution: [0.02, 2],
  continuity: [0, 1],
  routes: [-1, 1],
};

const within = (key: Ranged, v: number) => Math.min(RANGES[key][1], Math.max(RANGES[key][0], v));

/** The defaults, their lists their own (the settings change in place). */
const fresh = (): VisualSettings => ({
  ...DEFAULTS,
  pins: [],
  lighting: [...DEFAULTS.lighting],
  evolution: [...DEFAULTS.evolution],
  continuity: [...DEFAULTS.continuity],
  routes: { ...DEFAULTS.routes },
  extra: emptyExtra(),
});

const isPresetKey = (k: string): k is keyof Preset => (PRESET_KEYS as readonly string[]).includes(k);
/** The behaviour's sliders that pin as the Look's do, moved by hand (the owner, 2026-10-18: "I try to pin the behavior
 *  instead of the look"): a pinned one the dice and Drift leave as it is. The columns' scales are among them. */
export const BEHAVIOUR_PINS = ['flash', 'speed', 'intensity', 'fluid', 'smoothing', 'haze', 'tunnel', 'dive', 'effects', ...SCALE_KEYS] as const;
/** Each band's times, pinned one by one (`lighting:2`: the Low mid's Lighting): the dice leave a pinned one. */
export const BAND_TIMES = ['lighting', 'evolution', 'continuity'] as const;
export const bandPin = (key: (typeof BAND_TIMES)[number], band: number) => `${key}:${band}`;
export const BAND_PINS = BAND_TIMES.flatMap((k) => Array.from({ length: BANDS }, (_, b) => bandPin(k, b)));
/** Whether `k` can be pinned: the Look's settings, the behaviour's sliders, each band's times and a route (`route:`
 *  and its key; a pin lives on a route): what is pinned the dice, Drift and a track leave as it is (the owner: "I also
 *  want to implement pin for the matrix"). */
export const isPinnable = (k: string) =>
  isPresetKey(k) ||
  (BEHAVIOUR_PINS as readonly string[]).includes(k) ||
  BAND_PINS.includes(k) || (k.startsWith('route:') && isRoute(k.slice(6), BANDS));

function load(): VisualSettings {
  try {
    const kept = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Record<string, unknown>;
    // Kept by 63 (the only one with a Fluid before the form was kept): its Fluid on the finer scale.
    if (kept.keptForm === undefined && typeof kept.fluid === 'number') kept.fluid = fluidFrom63(kept.fluid);
    return keptFrom(kept);
  } catch {
    return fresh();
  }
}

/** Settings as kept (on this device, or come from another: shared.svelte.ts), over the defaults: each value brought
 *  into its range, anything not a setting left out. */
export function keptFrom(kept: Record<string, unknown>): VisualSettings {
  const saved = { ...kept };
  const out = fresh();
  // Kept before the look was a preset (2026-10-18): Symmetry 0 and Mirror 0 were Auto, Mirror 1 Off and 2 On;
  // Spread and Purity were settings of their own. One set away from its default then is pinned now.
  const before = !Array.isArray(saved.pins);
  if (before) {
    const pins: string[] = [];
    if (typeof saved.symmetry === 'number' && saved.symmetry >= 1) pins.push('symmetry');
    else delete saved.symmetry;
    if (saved.mirror === 1 || saved.mirror === 2) {
      saved.mirror = saved.mirror === 2 ? 1 : 0;
      pins.push('mirror');
    } else delete saved.mirror;
    if (typeof saved.spread === 'number' && saved.spread !== 1) pins.push('spread');
    if (typeof saved.purity === 'number' && saved.purity !== 0.75) pins.push('purity');
    saved.pins = pins;
  }
  for (const k of Object.keys(DEFAULTS) as (keyof VisualSettings)[]) {
    const v = saved[k];
    if (k === 'extra') out.extra = extraFrom(v, extraType);
    else if (k === 'pins') {
      if (Array.isArray(v)) out.pins = v.filter((x): x is string => typeof x === 'string' && isPinnable(x));
    } else if (typeof v === 'number' && Number.isFinite(v)) (out as Record<string, unknown>)[k] = within(k, v);
    // Six bands' times; three (kept before the six bands, 2026-10-18) each for two of them.
    else if (Array.isArray(v) && (v.length === 6 || v.length === 3) && v.every((x) => typeof x === 'number' && Number.isFinite(x))) {
      const six = v.length === 6 ? v : v.flatMap((x) => [x, x]);
      (out as Record<string, unknown>)[k] = six.map((x) => within(k, x));
    }
  }
  // The routes kept: each one there is, over the defaults (a 0 kept turns a default off).
  if (saved.routes && typeof saved.routes === 'object') {
    for (const [kept, v] of Object.entries(saved.routes as Record<string, unknown>)) {
      const key = keyNow(kept);
      if (!isRoute(key, BANDS)) continue;
      // An amount (a straight route), or a curve (64.1) brought into its ranges and kept as a number where straight.
      if (typeof v === 'number' && Number.isFinite(v)) out.routes[key] = within('routes', v);
      else {
        const c = curveFrom(v);
        if (c) out.routes[key] = valueOf(c);
      }
    }
  }
  return out;
}

export const visualSettings = $state<VisualSettings>(load());

/** Keeps them on this device. */
export function saveVisualSettings() {
  try {
    localStorage.setItem(KEY, JSON.stringify({ ...visualSettings, keptForm: FORM }));
  } catch {
    /* kept for this visit only */
  }
}

export function resetVisualSettings() {
  Object.assign(visualSettings, fresh());
  try {
    localStorage.removeItem(KEY);
  } catch {
    /* nothing kept */
  }
}

/** The routes back to the defaults, the other settings as they are. */
export function resetRoutes() {
  visualSettings.routes = defaultRoutes(BANDS);
  saveVisualSettings();
}

/** The look's settings as they are pinned. */
export function pinned(): Partial<Preset> {
  const out: Partial<Preset> = {};
  for (const k of visualSettings.pins) if (isPresetKey(k)) out[k] = visualSettings[k];
  return out;
}

/** A preset into the look's settings not pinned. */
export function loadPreset(p: Preset) {
  for (const k of PRESET_KEYS) if (!visualSettings.pins.includes(k)) visualSettings[k] = p[k];
  saveVisualSettings();
}
