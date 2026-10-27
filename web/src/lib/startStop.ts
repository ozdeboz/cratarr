// Start and stop's settings (lib/prefs; the app's too, lib/app): kept apart from both, as each imports the other.

/** Winding (decision 0074; the owner, 2026-10-27): an envelope on the tape's speed. `ratio`: a tap's length, seconds
 *  per minute wound (at most WIND_CAP_MS); `speed`: the speed limit (the sustain, ×); `attackMs`/`releaseMs`: the
 *  spin-up and the brake; `pieceMs`: each heard piece where a tap is cut short; `gainDb`: how much quieter the tape is
 *  while it winds; `seekWinds`: a seek winds there instead of jumping; `edges`: holding past a track's edge stops, or
 *  winds on through the queue. */
export type Wind = {
  ratio: number;
  speed: number;
  attackMs: number;
  releaseMs: number;
  pieceMs: number;
  gainDb: number;
  seekWinds: boolean;
  edges: 'stop' | 'queue';
};
/** A direction's own envelope (the owner, 2026-10-27: rewind and fast-forward each to one's taste; `wind` holds
 *  rewind's, `windFwd` fast-forward's; seeking and the edges are the two's alike). */
export type WindCurve = Pick<Wind, 'ratio' | 'speed' | 'attackMs' | 'releaseMs' | 'pieceMs' | 'gainDb'>;
/** A tap's longest (holding has no end but the edge). */
export const WIND_CAP_MS = 12_000;
/** Each tuning's range, step and starting value. */
export const WIND = {
  ratio: { min: 0.5, max: 2, step: 0.1, start: 1 },
  speed: { min: 2, max: 16, step: 0.5, start: 8 },
  attackMs: { min: 100, max: 1500, step: 50, start: 400 },
  releaseMs: { min: 100, max: 1500, step: 50, start: 250 },
  pieceMs: { min: 100, max: 1000, step: 50, start: 350 },
  gainDb: { min: -24, max: 0, step: 1, start: -12 },
} as const;

/** `rewind`: Previous and Next wind the tape (a tap to the track's start, a hold for as long as held); `wind`: how.
 *  0.9.10's tunings are gone, not carried over: everyone starts from winding's (the owner, 2026-10-27). */
export type StartStop = {
  mode: 'off' | 'vinyl' | 'fade';
  ms: number;
  skips: boolean;
  rewind: boolean;
  wind: Wind;
  windFwd: WindCurve;
};
export const RAMP_MS = { min: 200, max: 3000 };
type Range = { min: number; max: number; start: number };
const within = (v: unknown, r: Range) => (typeof v === 'number' && Number.isFinite(v) ? Math.min(r.max, Math.max(r.min, v)) : r.start);
/** A Start and stop as kept (here, or told by the app): what is missing or out of range at its start. */
export function startStopOf(x: Partial<StartStop>): StartStop {
  return {
    mode: x.mode === 'vinyl' || x.mode === 'fade' ? x.mode : 'off',
    ms: typeof x.ms === 'number' ? Math.min(RAMP_MS.max, Math.max(RAMP_MS.min, x.ms)) : 800,
    skips: x.skips === true,
    rewind: x.rewind === true,
    wind: windOf(x.wind),
    windFwd: curveOf(x.windFwd),
  };
}

/** A direction's envelope as kept: what is missing or out of range at its start. */
export function curveOf(x: Partial<WindCurve> | undefined): WindCurve {
  const w = x ?? {};
  return {
    ratio: within(w.ratio, WIND.ratio),
    speed: within(w.speed, WIND.speed),
    attackMs: within(w.attackMs, WIND.attackMs),
    releaseMs: within(w.releaseMs, WIND.releaseMs),
    pieceMs: within(w.pieceMs, WIND.pieceMs),
    gainDb: within(w.gainDb, WIND.gainDb),
  };
}
/** Every tuning at its start (Reset to defaults). */
export const CURVE_START: WindCurve = curveOf(undefined);

/** Winding's tunings as kept: what is missing or out of range at its start. */
export function windOf(x: Partial<Wind> | undefined): Wind {
  const w = x ?? {};
  return {
    ratio: within(w.ratio, WIND.ratio),
    speed: within(w.speed, WIND.speed),
    attackMs: within(w.attackMs, WIND.attackMs),
    releaseMs: within(w.releaseMs, WIND.releaseMs),
    pieceMs: within(w.pieceMs, WIND.pieceMs),
    gainDb: within(w.gainDb, WIND.gainDb),
    seekWinds: w.seekWinds === true,
    edges: w.edges === 'queue' ? 'queue' : 'stop',
  };
}
