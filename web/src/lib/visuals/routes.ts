// The visuals' routes (decision 0046, amended): every signal the music gives can drive every main aspect of the
// picture, each by an amount from -100 to +100 %, as a synthesiser's modulation matrix does. The owner: "For each aspect
// of the music we can get a signal for, I want to be able to control its effect on each of the main aspects of the
// visualization." An aspect adds up what its routes give it (each signal, 0..1, times its amount) and moves from its
// rest by that (Visuals.svelte has each one's range). The loudness as heard stays the envelope: every other signal's
// routes are scaled by it, so a quiet passage is a small show whatever its parts do.

/** Signals of the whole track. */
export const TRACK_SIGNALS = ['loudness', 'beat', 'downbeat', 'tempo', 'section', 'harmony', 'centroid', 'key', 'swell', 'buildUp'] as const;
/** Signals of each band. */
export const BAND_SIGNALS = ['level', 'flux', 'hits', 'percussive', 'harmonic', 'percussiveHits', 'crest', 'width', 'bandSwell'] as const;
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
  level: 'Level',
  flux: 'Flux',
  hits: 'Hits',
  percussive: 'Percussive',
  harmonic: 'Harmonic',
  percussiveHits: 'Drum hits',
  crest: 'Punch',
  width: 'Width',
  bandSwell: 'Swell',
};

/** The picture's aspects. */
export const ASPECTS = ['shape', 'rotation', 'lights', 'hue', 'spread', 'brightness', 'size', 'saturation', 'trails', 'smoothing', 'floor', 'depth', 'fluid', 'reseed', 'symmetry'] as const;
export type Aspect = (typeof ASPECTS)[number];
/** Aspects of each part of the flame: a band's routes act on its own part, the whole track's on every part. */
export const PART_ASPECTS: readonly Aspect[] = ['shape', 'rotation', 'lights', 'hue', 'spread'];
/** Aspects that act at moments: as what their routes give rises past a half (a new flame, a new symmetry). */
export const MOMENT_ASPECTS: readonly Aspect[] = ['reseed', 'symmetry'];

export const ASPECT_LABEL: Record<Aspect, string> = {
  shape: 'Shape',
  rotation: 'Rotation',
  lights: 'Lights',
  hue: 'Hue',
  spread: 'Spread',
  brightness: 'Glow',
  size: 'Size',
  saturation: 'Saturation',
  trails: 'Trails',
  smoothing: 'Smoothing',
  floor: 'Floor',
  depth: 'Depth',
  fluid: 'Fluid',
  reseed: 'Reseed',
  symmetry: 'Symmetry',
};

/** Each one's place in its list, for the dense arrays below. */
const places = <T extends string>(list: readonly T[]) => Object.fromEntries(list.map((x, i) => [x, i])) as Record<T, number>;
export const T = places(TRACK_SIGNALS);
export const B = places(BAND_SIGNALS);
export const A = places(ASPECTS);
const NA = ASPECTS.length;
const isPart = ASPECTS.map((a) => PART_ASPECTS.includes(a));
const isMoment = ASPECTS.map((a) => MOMENT_ASPECTS.includes(a));

/** A route's key: the signal, its band (`-` for the whole track's), the aspect. */
export const routeKey = (signal: Signal, band: number | null, aspect: Aspect) => `${signal}|${band ?? '-'}|${aspect}`;

/** Whether `key` names a route there is: a track signal with `-`, a band signal with a band under `bands`. */
export function isRoute(key: string, bands: number): boolean {
  const [signal, band, aspect] = key.split('|');
  if (!(ASPECTS as readonly string[]).includes(aspect)) return false;
  if ((TRACK_SIGNALS as readonly string[]).includes(signal)) return band === '-';
  return (BAND_SIGNALS as readonly string[]).includes(signal) && /^\d$/.test(band) && Number(band) < bands;
}

/** The routes that make the picture as it was before them: the loudness sets the size and the colour (and, as the
 *  envelope, everything else) and turns the whole, the beat flashes and swells it; each band's level evolves, turns
 *  and tints its part, its hits light it. Every other route is 0. */
export function defaultRoutes(bands: number): Record<string, number> {
  const r: Record<string, number> = {
    [routeKey('loudness', null, 'size')]: 1,
    [routeKey('loudness', null, 'saturation')]: 1,
    [routeKey('loudness', null, 'rotation')]: 0.2,
    [routeKey('beat', null, 'brightness')]: 0.2,
    [routeKey('beat', null, 'size')]: 0.08,
  };
  for (let b = 0; b < bands; b++) {
    r[routeKey('level', b, 'shape')] = 1;
    r[routeKey('level', b, 'rotation')] = 0.6;
    r[routeKey('level', b, 'hue')] = 0.08;
    r[routeKey('hits', b, 'lights')] = 1;
  }
  return r;
}

/** The routes as dense amounts: `track[signal * aspects + aspect]`, `bands[band][signal * aspects + aspect]`. */
export type Matrix = { track: Float32Array; bands: Float32Array[] };

export function compile(routes: Record<string, number>, bands: number): Matrix {
  const track = new Float32Array(TRACK_SIGNALS.length * NA);
  const per = Array.from({ length: bands }, () => new Float32Array(BAND_SIGNALS.length * NA));
  for (const [key, amount] of Object.entries(routes)) {
    if (!amount || !isRoute(key, bands)) continue;
    const [signal, band, aspect] = key.split('|');
    if (band === '-') track[T[signal as TrackSignal] * NA + A[aspect as Aspect]] = amount;
    else per[Number(band)][B[signal as BandSignal] * NA + A[aspect as Aspect]] = amount;
  }
  return { track, bands: per };
}

/** What each aspect is given now: `whole` the whole track's routes (and, for the aspects of the whole picture, the
 *  bands' too: their mean, or for a moment their largest), `band[b]` band b's own routes (for its part's aspects). */
export type Given = { whole: Float32Array; band: Float32Array[] };

export const given = (bands: number): Given => ({ whole: new Float32Array(NA), band: Array.from({ length: bands }, () => new Float32Array(NA)) });

/** Adds up the routes into `out`: `track` the whole track's signals now (`T` order, 0..1), `bands[b]` band b's (`B`
 *  order); `envelope` (0..1) scales every signal's routes but the loudness's own. */
export function give(m: Matrix, track: Float32Array, bands: Float32Array[], envelope: number, out: Given): Given {
  const { whole, band } = out;
  whole.fill(0);
  for (let s = 0; s < TRACK_SIGNALS.length; s++) {
    const v = track[s] * (s === T.loudness ? 1 : envelope);
    if (!v) continue;
    for (let a = 0; a < NA; a++) whole[a] += m.track[s * NA + a] * v;
  }
  const n = Math.min(band.length, bands.length, m.bands.length);
  for (let b = 0; b < band.length; b++) {
    const to = band[b];
    to.fill(0);
    if (b >= n) continue;
    const amounts = m.bands[b];
    for (let s = 0; s < BAND_SIGNALS.length; s++) {
      const v = bands[b][s] * envelope;
      if (!v) continue;
      for (let a = 0; a < NA; a++) to[a] += amounts[s * NA + a] * v;
    }
  }
  if (n) {
    for (let a = 0; a < NA; a++) {
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
