// The visuals' settings (decision 0046, amended): how much the device draws (points, steps, resolution, frame rate:
// ceilings the automatic step-down stays under), how the show behaves (intensity, flash, speed, brightness, colours, the
// grain), the look (the preset: the flame's form, detail, twist, warp, symmetry, mirror, lens, colour family, hue,
// lightness, vividness, spread and purity, which a track's key or the dice set and the picture follows; one the owner
// moved is pinned), each band's times (how long its light rings, how quickly its part evolves, and how much of its
// motion comes in steps on its own transients), and the routes: how much each of the music's signals drives each aspect
// of the picture (routes.ts).
// Changed live from the visuals' panel and kept on this device (a phone and a tablet want different ones); a browser
// that keeps nothing (a private window) starts from the defaults each time.

import { FORMS, LENSES, PRESET_KEYS, type Preset } from './flame';
import { defaultRoutes, isRoute, keyNow } from './routes';

const KEY = 'cratarr-visuals';
/** The kept settings' form: 2 since 64 (Fluid on its finer scale); none before. */
const FORM = 2;
/** Fluid's whole range: this much of the fluid's own. In 63 the slider went to all of it, and the owner wanted only its
 *  first few percent ("between 0 and 5% fluid dynamics is about all I want … or maybe 10% should be 100%", 2026-10-18). */
export const FLUID_FULL = 0.1;
/** A Fluid kept or saved before 64, on its finer scale. */
export const fluidFrom63 = (v: number) => Math.min(1, v / FLUID_FULL);
/** The bands the routes are kept for (the server's six). */
export const BANDS = 6;

export type VisualSettings = Preset & {
  /** The most points the chaos game runs (log2: 14 is 16k, 21 is 2M). */
  points: number;
  /** The most steps of every point drawn per frame. */
  steps: number;
  /** The drawing's size against the screen's own pixels (0.25..1); null: this screen's default (`autoResolution`). */
  resolution: number | null;
  /** The most frames a second. */
  fps: number;
  /** How much the music's loudness sets the show's intensity (0: always all, 1: as loud as it is). */
  intensity: number;
  /** How much of the picture's quick changes show (its lights, glow, size and colour on hits and beats): 0 only their
   *  slow course, 1 all of them, 2 twice as much. */
  flash: number;
  /** Motion: the turning and its steps (0..2). */
  speed: number;
  /** Calm ↔ Wild (0..1): how much busier than its budgets the governor lets the show be (governor.ts), half as much to
   *  twice as much; as the standing limits at its middle. */
  wild: number;
  /** The most brightness shown, after everything (0.1..1). */
  brightness: number;
  /** The grain: how far the sparse parts are blurred into a haze (0: the raw grain; 0.5: three pixels, as before),
   *  how faint a part goes black (0: none), and how much of the faint detail is lifted (the display's gamma: 0.5 is 3,
   *  as before). Each 0..1; their routes move them from there. */
  smoothing: number;
  floor: number;
  depth: number;
  /** How much the light flows (0..1, decision 0047): 0 its trails only fade where they were drawn, as before (nothing of
   *  the fluid made); above it they are carried along by a flow the music stirs, like smoke. Its route moves it. Its
   *  whole range is `FLUID_FULL` of the fluid's own. */
  fluid: number;
  /** The colours: 0 the preset's own (its family), 1 the playing track's cover's, 2 a fire's. */
  colours: number;
  /** The look's settings the owner moved: kept as they are when a track or the dice load another preset, until
   *  unpinned. */
  pins: string[];
  /** Each band's (sub, bass, low mid, mid, presence, air) light after a transient: how long it rings (seconds). */
  lighting: number[];
  /** Each band's loudness evolving its transform: how long it takes to follow (seconds). */
  evolution: number[];
  /** Each band's motion and evolution: 0 in steps on its own transients, 1 steady, between them a share of each. */
  continuity: number[];
  /** Each route's amount (-1..1) by its key (`routeKey`); one not here is 0. */
  routes: Record<string, number>;
};

const touch = typeof matchMedia !== 'undefined' && matchMedia('(pointer: coarse)').matches;

// Many points in few steps: each step is a transform feedback, which ANGLE on Direct3D 11 (Chrome and Edge on Windows)
// flushes after; the same points a frame in 3 steps rather than 12 took 4.0 ms against 4.8 on an RTX 4060 (2026-10-17).
export const DEFAULTS: VisualSettings = {
  points: 19,
  steps: 3,
  resolution: null,
  fps: touch ? 30 : 60,
  intensity: 1,
  flash: 1,
  speed: 1,
  wild: 0.5,
  brightness: 1,
  smoothing: 0.5,
  floor: 0,
  depth: 0.5,
  fluid: 0,
  colours: 0,
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
  pins: [],
  // A kick rings longer than a hat.
  lighting: [0.2, 0.18, 0.15, 0.13, 0.1, 0.08],
  evolution: [0.25, 0.25, 0.25, 0.25, 0.25, 0.25],
  continuity: [0.5, 0.5, 0.5, 0.5, 0.5, 0.5],
  routes: defaultRoutes(BANDS),
};

type Ranged = Exclude<keyof VisualSettings, 'pins'>;

/** Each setting's range, as its slider has it (the bands' times in seconds, though their sliders double). A kept value
 *  outside it (from a version whose range was another) is brought into it. */
export const RANGES: Record<Ranged, [number, number]> = {
  points: [14, 21],
  steps: [1, 16],
  resolution: [0.25, 1],
  fps: [10, 60],
  intensity: [0, 1],
  flash: [0, 2],
  speed: [0, 2],
  wild: [0, 1],
  brightness: [0.1, 1],
  smoothing: [0, 1],
  floor: [0, 1],
  depth: [0, 1],
  fluid: [0, 1],
  colours: [0, 2],
  form: [0, FORMS - 1],
  detail: [0, 1],
  twist: [0, 1],
  warp: [0, 1],
  symmetry: [1, 8],
  mirror: [0, 1],
  final: [0, LENSES.length],
  family: [0, 3],
  hue: [0, 1],
  lightness: [0, 1],
  vividness: [0, 1],
  spread: [0.25, 2.5],
  purity: [0, 1],
  lighting: [0.02, 1],
  evolution: [0.02, 2],
  continuity: [0, 1],
  routes: [-1, 1],
};

const within = (key: Ranged, v: number) => Math.min(RANGES[key][1], Math.max(RANGES[key][0], v));

/** The pixels the display pass draws by default: about a megapixel. The points fall into a buffer of at most 0.6 MP
 *  anyway; drawing that out to every pixel of a large screen cost the most of a frame on integrated graphics (5.5 ms of
 *  22 at 2560 x 1600, measured 2026-10-17) and showed little more. */
const AUTO_PIXELS = 1.2e6;

/** This screen's default resolution: up to `AUTO_PIXELS` of its own pixels. */
export function autoResolution(devicePixels: number): number {
  return Math.min(1, Math.max(0.25, Math.sqrt(AUTO_PIXELS / Math.max(1, devicePixels))));
}

/** The defaults, their lists their own (the settings change in place). */
const fresh = (): VisualSettings => ({
  ...DEFAULTS,
  pins: [],
  lighting: [...DEFAULTS.lighting],
  evolution: [...DEFAULTS.evolution],
  continuity: [...DEFAULTS.continuity],
  routes: { ...DEFAULTS.routes },
});

const isPresetKey = (k: string): k is keyof Preset => (PRESET_KEYS as readonly string[]).includes(k);

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
    if (k === 'pins') {
      if (Array.isArray(v)) out.pins = v.filter((x): x is string => typeof x === 'string' && isPresetKey(x));
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
      if (typeof v === 'number' && Number.isFinite(v) && isRoute(key, BANDS)) out.routes[key] = within('routes', v);
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
