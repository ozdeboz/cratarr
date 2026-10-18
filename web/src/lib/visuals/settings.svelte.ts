// The visuals' settings (decision 0046, amended): how much the device draws (points, steps, resolution, frame rate:
// ceilings the automatic step-down stays under), how the show looks (intensity, speed, brightness, purity, spread,
// symmetry), each band's times (how long its light rings, how quickly its part evolves, and how much of its motion comes
// in steps on its own transients), and the routes: how much each of the music's signals drives each aspect of the
// picture (routes.ts).
// Changed live from the visuals' panel and kept on this device (a phone and the tablet want different ones); a
// browser that keeps nothing (a private window) starts from the defaults each time.

import { defaultRoutes, isRoute } from './routes';

const KEY = 'cratarr-visuals';
/** The bands the routes are kept for (the server's six). */
export const BANDS = 6;

export type VisualSettings = {
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
  /** Motion: the turning and its steps (0..2). */
  speed: number;
  /** The most brightness shown, after everything (0.1..1). */
  brightness: number;
  /** How purely each band's part is its own colour (0: points blend their bands' colours, 1: each its band's alone). */
  purity: number;
  /** The parts' offsets scaled apart (separate clusters) or together (one shape): 1 is the seed's own. */
  spread: number;
  /** The flame's rotations: 0 its own (the seed's), else that many arms. */
  symmetry: number;
  /** Its mirrors: 0 its own, 1 none, 2 as many as its arms (dihedral). */
  mirror: number;
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
  speed: 1,
  brightness: 1,
  // Six parts mixing their colours go grey: mostly each its band's own.
  purity: 0.75,
  spread: 1,
  symmetry: 0,
  mirror: 0,
  // A kick rings longer than a hat.
  lighting: [0.2, 0.18, 0.15, 0.13, 0.1, 0.08],
  evolution: [0.25, 0.25, 0.25, 0.25, 0.25, 0.25],
  continuity: [0.5, 0.5, 0.5, 0.5, 0.5, 0.5],
  routes: defaultRoutes(BANDS),
};

/** Each setting's range, as its slider has it (the bands' times in seconds, though their sliders double). A kept value
 *  outside it (from a version whose range was another) is brought into it. */
export const RANGES: Record<keyof VisualSettings, [number, number]> = {
  points: [14, 21],
  steps: [1, 16],
  resolution: [0.25, 1],
  fps: [10, 60],
  intensity: [0, 1],
  speed: [0, 2],
  brightness: [0.1, 1],
  purity: [0, 1],
  spread: [0.25, 2.5],
  symmetry: [0, 8],
  mirror: [0, 2],
  lighting: [0.02, 1],
  evolution: [0.02, 2],
  continuity: [0, 1],
  routes: [-1, 1],
};

const within = (key: keyof typeof RANGES, v: number) => Math.min(RANGES[key][1], Math.max(RANGES[key][0], v));

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
  lighting: [...DEFAULTS.lighting],
  evolution: [...DEFAULTS.evolution],
  continuity: [...DEFAULTS.continuity],
  routes: { ...DEFAULTS.routes },
});

function load(): VisualSettings {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<VisualSettings>;
    const out = fresh();
    for (const k of Object.keys(DEFAULTS) as (keyof VisualSettings)[]) {
      const v = saved[k];
      if (typeof v === 'number' && Number.isFinite(v)) (out as Record<string, unknown>)[k] = within(k, v);
      // Six bands' times; three (kept before the six bands, 2026-10-18) each for two of them.
      else if (Array.isArray(v) && (v.length === 6 || v.length === 3) && v.every((x) => typeof x === 'number' && Number.isFinite(x))) {
        const six = v.length === 6 ? v : v.flatMap((x) => [x, x]);
        (out as Record<string, unknown>)[k] = six.map((x) => within(k, x));
      }
    }
    // The routes kept: each one there is, over the defaults (a 0 kept turns a default off).
    if (saved.routes && typeof saved.routes === 'object') {
      for (const [key, v] of Object.entries(saved.routes)) {
        if (typeof v === 'number' && Number.isFinite(v) && isRoute(key, BANDS)) out.routes[key] = within('routes', v);
      }
    }
    return out;
  } catch {
    return fresh();
  }
}

export const visualSettings = $state<VisualSettings>(load());

/** Keeps them on this device. */
export function saveVisualSettings() {
  try {
    localStorage.setItem(KEY, JSON.stringify(visualSettings));
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
