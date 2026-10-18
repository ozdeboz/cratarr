// Drift (decision 0046, amended): the look walking with the music, in place of Reseed's jumps. The owner: "The reseed
// route target should essentially only affect looks, and it should walk proportionally instead of randomizing", "So
// random direction proportional distance", and a Track row to steer what a new track does to the look.
//
// - The walk moves only the look's smooth settings (Detail, Twist, Warp, Hue, Lightness, Vividness, Spread, Purity)
//   that are not pinned, in a direction drawn anew each bar (from the track and the bar's number, so every device of a
//   sync group walks alike), as far as the routed signal reaches: Drift at +100% for a second of a full signal is a
//   step of `STEP`. Below 0 it walks back toward home instead.
// - Home is where the track put the look; the walk stays within `TETHER` of it, so it never leaves ground the track's
//   own look has checked.
// - A new track moves the look by the Track row's routed amount into Drift: −100% all the way to the new track's own
//   look (as before Drift), −50% halfway, 0 not at all, above 0 away from it by as much of the tether. The choice
//   settings (Form, Symmetry, Mirror, Lens, Family) change only past halfway.
//
// Pure: `stepDrift` returns the settings to change this frame (none where nothing moves).

import { PRESET_KEYS, type Preset } from './flame';

/** The settings that walk, and the choices that only a new track changes. */
export const SMOOTH = ['detail', 'twist', 'warp', 'hue', 'lightness', 'vividness', 'spread', 'purity'] as const;
const CHOICES = PRESET_KEYS.filter((k) => !(SMOOTH as readonly string[]).includes(k));
/** How far the walk goes in a second of full signal at +100% (in the settings' 0..1 ranges, all together). */
export const STEP = 0.5;
/** How far from home the walk may go (the same measure). */
export const TETHER = 0.3;
/** Spread's range: walked in its logarithm, so a step is the same ratio wherever it is. */
const SPREAD: [number, number] = [0.25, 2.5];

export type DriftInput = {
  /** The Drift column's routed sum now (signed). */
  amount: number;
  /** The Track row's routed amount into Drift this frame (signed; 0 where there is none). */
  trackEvent: number;
  dt: number;
  /** The track's seed, and the bar now (each bar a new direction). */
  seed: number;
  bar: number;
  pins: string[];
  /** The track's own look, and the settings now. */
  own: Preset;
  now: Preset;
};

export type Drift = {
  seed: number | null;
  /** The track's move done (its event taken). */
  moved: boolean;
  home: Record<string, number> | null;
  bar: number | null;
  direction: Record<string, number>;
};

export function newDrift(): Drift {
  return { seed: null, moved: false, home: null, bar: null, direction: {} };
}

/** A setting on its 0..1 walking scale, and back. */
const toUnit = (k: string, v: number) => (k === 'spread' ? Math.log(v / SPREAD[0]) / Math.log(SPREAD[1] / SPREAD[0]) : v);
const fromUnit = (k: string, u: number) => (k === 'spread' ? SPREAD[0] * (SPREAD[1] / SPREAD[0]) ** u : u);
/** The way from `a` to `b` on a setting's scale: Hue the short way round. */
const toward = (k: string, a: number, b: number) => (k === 'hue' ? ((((b - a) % 1) + 1.5) % 1) - 0.5 : b - a);
const settle = (k: string, u: number) => (k === 'hue' ? ((u % 1) + 1) % 1 : Math.min(1, Math.max(0, u)));

/** Random numbers from a seed (mulberry32). */
function randomFrom(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** A direction over `keys`, of length one, the same for the same track and bar. */
function directionFor(seed: number, bar: number, keys: readonly string[]): Record<string, number> {
  const rnd = randomFrom(Math.imul(seed ^ 0x9e3779b9, 31) ^ Math.imul(bar + 1, 0x85ebca6b));
  const d: Record<string, number> = {};
  let len = 0;
  for (const k of keys) {
    // Gaussian (Box–Muller): every way as likely as another.
    const g = Math.sqrt(-2 * Math.log(1 - rnd())) * Math.cos(2 * Math.PI * rnd());
    d[k] = g;
    len += g * g;
  }
  len = Math.sqrt(len) || 1;
  for (const k of keys) d[k] /= len;
  return d;
}

export function stepDrift(s: Drift, i: DriftInput): Partial<Preset> {
  const free = SMOOTH.filter((k) => !i.pins.includes(k));
  const out: Partial<Preset> = {};
  const unitNow = (k: string) => toUnit(k, i.now[k as keyof Preset]);
  const set = (k: string, u: number) => {
    const v = fromUnit(k, settle(k, u));
    if (Math.abs(v - i.now[k as keyof Preset]) > 1e-5) (out as Record<string, number>)[k] = v;
  };

  // A new track: home is where the look stands until the Track row says otherwise.
  if (s.seed !== i.seed) {
    s.seed = i.seed;
    s.moved = false;
    s.home = Object.fromEntries(SMOOTH.map((k) => [k, unitNow(k)]));
    s.bar = null;
  }
  // The track's move, once: toward its own look (below 0) or away (above), then home is where it lands.
  if (!s.moved && i.trackEvent !== 0) {
    s.moved = true;
    const f = Math.min(1, Math.max(-1, -i.trackEvent));
    const home: Record<string, number> = { ...s.home! };
    if (f > 0) {
      for (const k of free) home[k] = unitNow(k) + f * toward(k, unitNow(k), toUnit(k, i.own[k]));
      if (f > 0.5) for (const k of CHOICES) if (!i.pins.includes(k) && i.own[k] !== i.now[k]) (out as Record<string, number>)[k] = i.own[k];
    } else {
      const d = directionFor(i.seed, -1, free);
      for (const k of free) home[k] = unitNow(k) + -f * TETHER * d[k];
    }
    for (const k of free) {
      home[k] = settle(k, home[k]);
      set(k, home[k]);
    }
    s.home = home;
    return out;
  }

  // The walk: this bar's direction out (Drift above 0), or back home (below), as far as the signal reaches.
  if (!(i.amount !== 0 && i.dt > 0 && free.length && s.home)) return out;
  if (s.bar !== i.bar) {
    s.bar = i.bar;
    s.direction = directionFor(i.seed, i.bar, free);
  }
  const far = Math.abs(i.amount) * STEP * i.dt;
  const at: Record<string, number> = Object.fromEntries(free.map((k) => [k, unitNow(k)]));
  if (i.amount > 0) {
    for (const k of free) at[k] += far * (s.direction[k] ?? 0);
  } else {
    const back = Object.fromEntries(free.map((k) => [k, toward(k, at[k], s.home![k])]));
    const gap = Math.hypot(...free.map((k) => back[k]));
    const go = Math.min(1, gap > 1e-9 ? far / gap : 0);
    for (const k of free) at[k] += go * back[k];
  }
  // The tether: no further from home than TETHER (along the way back to it).
  const off = Object.fromEntries(free.map((k) => [k, toward(k, s.home![k], at[k])]));
  const dist = Math.hypot(...free.map((k) => off[k]));
  if (dist > TETHER) for (const k of free) at[k] = s.home[k] + (off[k] * TETHER) / dist;
  for (const k of free) set(k, at[k]);
  return out;
}
