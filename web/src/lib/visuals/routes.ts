// The visuals' routes (decision 0046, amended): every signal the music gives can drive every main aspect of the
// picture, each by an amount from -100 to +100 %, as a synthesiser's modulation matrix does. The owner: "For each aspect
// of the music we can get a signal for, I want to be able to control its effect on each of the main aspects of the
// visualization." An aspect adds up what its routes give it (each signal, 0..1, times its amount) and moves from its
// rest by that (Visuals.svelte has each one's range). The loudness as heard stays the envelope: every other signal's
// routes are scaled by it, so a quiet passage is a small show whatever its parts do.

import { curveFrom, heightAt, isStraight, tableOf, type RouteValue } from './curves';
import type { VisualSettings } from './settings.svelte';
export type { Curve, RouteValue } from './curves';

/** Signals of the whole track. */
export const TRACK_SIGNALS = ['loudness', 'beat', 'downbeat', 'tempo', 'section', 'harmony', 'centroid', 'key', 'swell', 'buildUp', 'track'] as const;
/** Signals of each band. */
export const BAND_SIGNALS = ['level', 'flux', 'hits', 'percussive', 'harmonic', 'percussiveHits', 'crest', 'width', 'bandSwell', 'envelope'] as const;
export type TrackSignal = (typeof TRACK_SIGNALS)[number];
export type BandSignal = (typeof BAND_SIGNALS)[number];
export type Signal = TrackSignal | BandSignal;

export const SIGNAL_LABEL: Record<Signal, string> = {
  loudness: 'Loudness',
  beat: 'Beat',
  downbeat: 'Downbeat',
  tempo: 'Tempo',
  section: 'Section',
  harmony: 'Harmony',
  centroid: 'Timbre',
  key: 'Key',
  swell: 'Swell',
  buildUp: 'Build-up',
  track: 'Track',
  level: 'Level',
  flux: 'Flux',
  hits: 'Hits',
  percussive: 'Percussive',
  harmonic: 'Harmonic',
  percussiveHits: 'Drum hits',
  crest: 'Punch',
  width: 'Width',
  bandSwell: 'Swell',
  envelope: 'Envelope',
};

/** The picture's aspects. Stir, Flow and Swirl are the fluid's (64.1; the owner asked that what moves it be routes,
 *  not hidden): read by stirring.ts at its own moments (`heightOn`), not added up each frame. */
export const ASPECTS = [
  'shape',
  'rotation',
  'lights',
  'hue',
  'spread',
  'presence',
  'brightness',
  'size',
  'saturation',
  'trails',
  'smoothing',
  'haze',
  'fluid',
  'stir',
  'flow',
  'swirl',
  'waves',
  'tunnel',
  'dive',
  'effects',
  'facets',
  'look',
  'behaviour',
  // The look's own (65.1; the owner: "I want matrix targets for all effect controls", "Also for lens and other
  // discrete"), each named for the setting it moves, in the Look's order: its discrete settings at moments, each
  // engine's choice at moments, its Amount a level and its two controls swinging about where they are set.
  'family',
  'form',
  'symmetry',
  'mirror',
  'final',
  'lattice',
  'fx1',
  'fx1Amount',
  'fx1K1',
  'fx1K2',
  'fx2',
  'fx2Amount',
  'fx2K1',
  'fx2K2',
  'fx3',
  'fx3Amount',
  'fx3K1',
  'fx3K2',
  'fxLoop',
  'fxLoopAmount',
  'fxLoopK1',
  'fxLoopK2',
] as const;
export type Aspect = (typeof ASPECTS)[number];
/** Aspects of each part of the flame: a band's routes act on its own part, the whole track's on every part. */
export const PART_ASPECTS: readonly Aspect[] = ['shape', 'rotation', 'lights', 'hue', 'spread', 'presence'];
/** Aspects that act at moments: as what their routes give rises past a half, the setting moves on (a new symmetry, a
 *  new lens, another of an engine's effects…). */
export const MOMENT_ASPECTS = ['family', 'form', 'symmetry', 'mirror', 'final', 'lattice', 'fx1', 'fx2', 'fx3', 'fxLoop'] as const satisfies readonly Aspect[];
/** The look's columns (65.1), which the dice leave as they are (recipe.ts): they move what the look is, not how it
 *  follows the music, and no recipe draws routes into them. */
export const LOOK_COLUMNS: readonly Aspect[] = [...MOMENT_ASPECTS.filter((a) => a !== 'symmetry'), 'fx1Amount', 'fx1K1', 'fx1K2', 'fx2Amount', 'fx2K1', 'fx2K2', 'fx3Amount', 'fx3K1', 'fx3K2', 'fxLoopAmount', 'fxLoopK1', 'fxLoopK2'];
/** The effects' controls, swinging about where they are set (each engine's two, then the Loop's), in effects.ts's
 *  `routed.knobs` order. */
export const KNOB_ASPECTS = ['fx1K1', 'fx1K2', 'fx2K1', 'fx2K2', 'fx3K1', 'fx3K2', 'fxLoopK1', 'fxLoopK2'] as const;
/** The effects' Amounts, as levels (each engine's, then the Loop's), in `routed.amounts` order. */
export const AMOUNT_ASPECTS = ['fx1Amount', 'fx2Amount', 'fx3Amount', 'fxLoopAmount'] as const;

export const ASPECT_LABEL: Record<Aspect, string> = {
  shape: 'Shape',
  rotation: 'Rotation',
  lights: 'Lights',
  hue: 'Hue',
  spread: 'Spread',
  presence: 'Presence',
  brightness: 'Glow',
  size: 'Size',
  saturation: 'Saturation',
  trails: 'Trails',
  smoothing: 'Smoothing',
  haze: 'Haze',
  fluid: 'Fluid',
  stir: 'Stir',
  flow: 'Flow',
  swirl: 'Swirl',
  waves: 'Waves',
  tunnel: 'Tunnel',
  dive: 'Dive',
  effects: 'Effects',
  facets: 'Facets',
  look: 'Look',
  behaviour: 'Behaviour',
  family: 'Colours',
  form: 'Form',
  symmetry: 'Symmetry',
  mirror: 'Mirror',
  final: 'Lens',
  lattice: 'Lattice',
  fx1: 'Kaleidoscope',
  fx1Amount: 'Kaleidoscope amount',
  fx1K1: 'Kaleidoscope control 1',
  fx1K2: 'Kaleidoscope control 2',
  fx2: 'EYEDO',
  fx2Amount: 'EYEDO amount',
  fx2K1: 'EYEDO control 1',
  fx2K2: 'EYEDO control 2',
  fx3: 'Hydra',
  fx3Amount: 'Hydra amount',
  fx3K1: 'Hydra control 1',
  fx3K2: 'Hydra control 2',
  fxLoop: 'Loop',
  fxLoopAmount: 'Loop amount',
  fxLoopK1: 'Loop control 1',
  fxLoopK2: 'Loop control 2',
};

/** The columns that have no setting of their own otherwise, each its scale's setting (64.2; the owner: "is it
 *  impossible to have each column their own slider?"): what its routes give it, times that (1 as before, 0 at rest).
 *  The Glow column's is `glow`: `brightness` is the display's. */
export const COLUMN_SCALES = {
  shape: 'shape',
  rotation: 'rotation',
  lights: 'lights',
  brightness: 'glow',
  size: 'size',
  saturation: 'saturation',
  trails: 'trails',
  stir: 'stir',
  flow: 'flow',
  swirl: 'swirl',
  presence: 'presence',
  waves: 'waves',
} as const satisfies Partial<Record<Aspect, keyof VisualSettings>>;
export const SCALE_KEYS = Object.values(COLUMN_SCALES);

/** Each one's place in its list, for the dense arrays below. */
const places = <T extends string>(list: readonly T[]) => Object.fromEntries(list.map((x, i) => [x, i])) as Record<T, number>;
export const T = places(TRACK_SIGNALS);
export const B = places(BAND_SIGNALS);
export const A = places(ASPECTS);
const NA = ASPECTS.length;
const isPart = ASPECTS.map((a) => PART_ASPECTS.includes(a));
const isMoment = ASPECTS.map((a) => (MOMENT_ASPECTS as readonly Aspect[]).includes(a));

/** A route's key: the signal, its band (`-` for the whole track's), the aspect. */
export const routeKey = (signal: Signal, band: number | null, aspect: Aspect) => `${signal}|${band ?? '-'}|${aspect}`;

/** A route's key as kept before, as it is now: Reseed's column became Drift's (64), and Drift's is Look's (64.1: the
 *  owner, "Instead of having drift as a target, we should have looks and behavior as a target"). */
export const keyNow = (key: string) =>
  key.endsWith('|reseed') ? `${key.slice(0, -'reseed'.length)}look` : key.endsWith('|drift') ? `${key.slice(0, -'drift'.length)}look` : key;

/** Whether `key` names a route there is: a track signal with `-`, a band signal with a band under `bands`. */
export function isRoute(key: string, bands: number): boolean {
  const [signal, band, aspect] = key.split('|');
  if (!(ASPECTS as readonly string[]).includes(aspect)) return false;
  if ((TRACK_SIGNALS as readonly string[]).includes(signal)) return band === '-';
  return (BAND_SIGNALS as readonly string[]).includes(signal) && /^\d$/.test(band) && Number(band) < bands;
}

/** The routes that make the picture as it was before them: the loudness sets the size and the colour (and, as the
 *  envelope, everything else) and turns the whole, the beat flashes and swells it; each band's level evolves, turns
 *  and tints its part, its hits light it; a new track brings its own look (Track into Drift at -100 %: all the way
 *  back to it). Every other route is 0. */
export function defaultRoutes(bands: number): Record<string, number> {
  const r: Record<string, number> = {
    [routeKey('track', null, 'look')]: -1,
    [routeKey('track', null, 'behaviour')]: -1,
    [routeKey('loudness', null, 'size')]: 1,
    [routeKey('loudness', null, 'saturation')]: 1,
    [routeKey('loudness', null, 'rotation')]: 0.2,
    [routeKey('beat', null, 'brightness')]: 0.2,
    [routeKey('beat', null, 'size')]: 0.08,
    // The fluid's stirring as it always was (stirring.ts): the beats', the downbeats' and a new section's pushes; the
    // flow as free as the loudness, breathing with the beat; the swells' and a new section's whirls.
    [routeKey('beat', null, 'stir')]: 1,
    [routeKey('downbeat', null, 'stir')]: 1,
    [routeKey('section', null, 'stir')]: 1,
    [routeKey('loudness', null, 'flow')]: 1,
    [routeKey('beat', null, 'flow')]: 1,
    [routeKey('swell', null, 'swirl')]: 1,
    [routeKey('section', null, 'swirl')]: 1,
    // Slow giant waves as the music swells (65).
    [routeKey('swell', null, 'waves')]: 1,
  };
  for (let b = 0; b < bands; b++) {
    r[routeKey('level', b, 'shape')] = 1;
    r[routeKey('level', b, 'rotation')] = 0.6;
    r[routeKey('level', b, 'hue')] = 0.08;
    r[routeKey('hits', b, 'lights')] = 1;
    // Each band's part as present as its sound (65): its envelope, a hit's flare and what the sound holds.
    r[routeKey('envelope', b, 'presence')] = 1;
    // Each band's hits kick its part; its level stirs it all the while (as its Continuity shares them).
    r[routeKey('hits', b, 'stir')] = 1;
    r[routeKey('level', b, 'stir')] = 1;
  }
  return r;
}

/** The routes compiled (decision 0046, amended for 64.1): `track[signal * aspects + aspect]` and `bands[band][…]` each
 *  route's end (its amount, as a straight one gives it); where a route is curved, its table of heights by signal
 *  (`curves.ts`), else null; each one's peak (the most it gives, never under 0: what a level's route can let through);
 *  and the routes there are, so a frame passes over none of the others. */
export type Matrix = {
  track: Float32Array;
  bands: Float32Array[];
  trackTable: (Float32Array | null)[];
  bandTable: (Float32Array | null)[][];
  trackPeak: Float32Array;
  bandPeak: Float32Array[];
  trackOn: Int32Array;
  bandOn: Int32Array[];
  /** The routes that walk with the behaviour, by key, and each cell's place among them (−1: it does not walk). */
  walks: string[];
  trackWalk: Int16Array;
  bandWalk: Int16Array[];
};

export function compile(routes: Record<string, RouteValue>, bands: number): Matrix {
  const cells = (signals: number) => ({
    end: new Float32Array(signals * NA),
    table: Array<Float32Array | null>(signals * NA).fill(null),
    peak: new Float32Array(signals * NA),
    on: [] as number[],
    walk: new Int16Array(signals * NA).fill(-1),
  });
  const walks: string[] = [];
  const t = cells(TRACK_SIGNALS.length);
  const per = Array.from({ length: bands }, () => cells(BAND_SIGNALS.length));
  for (const [key, value] of Object.entries(routes)) {
    if (!isRoute(key, bands)) continue;
    const curve = typeof value === 'number' ? null : curveFrom(value);
    const end = curve ? curve.h : typeof value === 'number' && Number.isFinite(value) ? value : 0;
    const bent = curve && !isStraight(curve) ? tableOf(curve) : null;
    if (!end && !bent) continue;
    const [signal, band, aspect] = key.split('|');
    const into = band === '-' ? t : per[Number(band)];
    const cell = (band === '-' ? T[signal as TrackSignal] : B[signal as BandSignal]) * NA + A[aspect as Aspect];
    into.end[cell] = end;
    into.table[cell] = bent ? bent.table : null;
    into.peak[cell] = bent ? bent.peak : Math.max(0, end);
    into.on.push(cell);
    if (curve?.walks) into.walk[cell] = walks.push(key) - 1;
  }
  return {
    track: t.end,
    bands: per.map((b) => b.end),
    trackTable: t.table,
    bandTable: per.map((b) => b.table),
    trackPeak: t.peak,
    bandPeak: per.map((b) => b.peak),
    trackOn: Int32Array.from(t.on),
    bandOn: per.map((b) => Int32Array.from(b.on)),
    walks,
    trackWalk: t.walk,
    bandWalk: per.map((b) => b.walk),
  };
}

/** What each aspect is given now: `whole` the whole track's routes (and, for the aspects of the whole picture, the
 *  bands' too: their mean, or for a moment their largest), `band[b]` band b's own routes (for its part's aspects);
 *  `level` each level's share of its setting the routes let through (1: all of it; `LEVEL_ASPECTS`). */
export type Given = { whole: Float32Array; band: Float32Array[]; level: Float32Array };

export const given = (bands: number): Given => ({
  whole: new Float32Array(NA),
  band: Array.from({ length: bands }, () => new Float32Array(NA)),
  level: new Float32Array(NA).fill(1),
});

/** The levels (64.1; the owner: "The other sliders are simply limits"): Smoothing, Haze, Fluid, Tunnel and Dive are each
 *  the most its setting gives (Haze, either way from 0: the most of it, more or less), and each of its routes a condition for reaching it: a route lets through all of it where its
 *  curve is at its peak, less as it falls short (as much less as the envelope says), never under nothing; they
 *  multiply. A straight one of amount a gives 1 − e·a·(1 − s) rising with its signal s, of −a 1 − e·a·s falling; quiet
 *  (e → 0) every route lets all through. The bands' routes into one: their geometric mean, as the swings take the bands'
 *  mean. */
export const LEVEL_ASPECTS: readonly Aspect[] = ['smoothing', 'haze', 'fluid', 'tunnel', 'dive', 'effects', 'facets', ...AMOUNT_ASPECTS];
const isLevel = ASPECTS.map((a) => LEVEL_ASPECTS.includes(a));

/** What a route gives at its signal `v`: its curve's height, a straight one's amount times the signal. */
const heightOf = (table: Float32Array | null, end: number, v: number) => (table ? heightAt(table, v) : end * v);
/** What a route gives at `x` at a moment of its own, not each frame through `give` (the fluid's pushes, stirring.ts):
 *  its curve's height, a straight one's amount times `x`, none where there is no route; walking, its walk's share
 *  (`walk`: `Matrix.walks` order). `band` null: the whole track's. */
export function heightOn(m: Matrix, signal: Signal, band: number | null, aspect: Aspect, x: number, walk?: Float32Array): number {
  const a = A[aspect];
  if (band === null) {
    const s = T[signal as TrackSignal];
    if (s === undefined) return 0;
    const cell = s * NA + a;
    const w = m.trackWalk[cell];
    return heightOf(m.trackTable[cell], m.track[cell], x) * (w < 0 || !walk ? 1 : (walk[w] ?? 1));
  }
  const s = B[signal as BandSignal];
  if (s === undefined || !m.bands[band]) return 0;
  const cell = s * NA + a;
  const w = m.bandWalk[band][cell];
  return heightOf(m.bandTable[band][cell], m.bands[band][cell], x) * (w < 0 || !walk ? 1 : (walk[w] ?? 1));
}

/** A level's route's share let through; walking (`w` under 1), its pull as much less. */
const letThrough = (peak: number, y: number, e: number, w = 1) => 1 - w * (1 - Math.min(1, Math.max(0, 1 - e * (peak - y))));

/** Adds up the routes into `out`: `track` the whole track's signals now (`T` order, 0..1), `bands[b]` band b's (`B`
 *  order); `envelope` (0..1) scales every signal's routes but the loudness's own. The swings add up what each route's
 *  curve gives at its signal, times the envelope; the levels multiply what each lets through. `walk`: each walking
 *  route's factor now (`Matrix.walks` order, 1: as set), its share of what it gives. */
export function give(m: Matrix, track: Float32Array, bands: Float32Array[], envelope: number, out: Given, walk?: Float32Array): Given {
  const walkOf = (i: number) => (i < 0 || !walk ? 1 : (walk[i] ?? 1));
  const { whole, band, level } = out;
  whole.fill(0);
  level.fill(1);
  for (let i = 0; i < m.trackOn.length; i++) {
    const cell = m.trackOn[i];
    const s = (cell / NA) | 0;
    const a = cell - s * NA;
    const e = s === T.loudness ? 1 : envelope;
    const y = heightOf(m.trackTable[cell], m.track[cell], track[s]);
    const w = walkOf(m.trackWalk[cell]);
    if (isLevel[a]) level[a] *= letThrough(m.trackPeak[cell], y, e, w);
    else whole[a] += e * y * w;
  }
  const n = Math.min(band.length, bands.length, m.bands.length);
  const bandLevel = levelScratch(n);
  for (let b = 0; b < band.length; b++) {
    const to = band[b];
    to.fill(0);
    if (b >= n) continue;
    const lv = bandLevel[b];
    lv.fill(1);
    const on = m.bandOn[b];
    for (let i = 0; i < on.length; i++) {
      const cell = on[i];
      const s = (cell / NA) | 0;
      const a = cell - s * NA;
      const y = heightOf(m.bandTable[b][cell], m.bands[b][cell], bands[b][s]);
      const w = walkOf(m.bandWalk[b][cell]);
      if (isLevel[a]) lv[a] *= letThrough(m.bandPeak[b][cell], y, envelope, w);
      else to[a] += envelope * y * w;
    }
  }
  if (n) {
    for (let a = 0; a < NA; a++) {
      if (isLevel[a]) {
        let product = 1;
        for (let b = 0; b < n; b++) product *= bandLevel[b][a];
        level[a] *= product ** (1 / n);
        continue;
      }
      if (isPart[a]) continue;
      let sum = 0;
      let most = -Infinity;
      for (let b = 0; b < n; b++) {
        sum += band[b][a];
        most = Math.max(most, band[b][a]);
      }
      whole[a] += isMoment[a] ? most : sum / n;
    }
  }
  return out;
}

// Each band's levels, kept from call to call (no garbage each frame).
let scratch: Float32Array[] = [];
function levelScratch(n: number): Float32Array[] {
  while (scratch.length < n) scratch.push(new Float32Array(NA));
  return scratch;
}
