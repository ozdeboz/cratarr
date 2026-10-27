// The tape's settings (Settings › This device › Tape; lib/prefs, the app's too, lib/app): kept apart from both, as each
// imports the other. Start and stop, the tape's own mechanics, and its four motions (the owner, 2026-10-27: two
// settings per motion, the rest shared): back and forward to a point (Previous to the start, Next to the end, a seek
// that winds), and holding back and forward (Previous or Next, ← or →, held).

/** A motion's envelope as the players use it (lib/windShape, lib/localPlayer): `ratio`, a wind to a point's length,
 *  seconds per minute wound (at most WIND_CAP_MS); `speed`, its speed limit or a hold's starting speed (×);
 *  `attackMs`/`releaseMs`, the spin-up and the brake; `pieceMs`, each heard piece where a wind is cut short; `gainDb`,
 *  how much quieter the tape is while it winds. Made from the settings (pointWind, holdWind), never kept. */
export type Wind = { ratio: number; speed: number; attackMs: number; releaseMs: number; pieceMs: number; gainDb: number };
/** A hold's envelope: also how it gets faster (doubling every `doublingS` seconds of its stream) and how fast at most. */
export type HoldWind = Wind & { doublingS: number; top: number };

/** The tape's mechanics, every motion's alike: spin-up, the one brake (a Vinyl pause's too), the wind volume, how a hold
 *  gets faster, how a cut wind's pieces are. */
export type TapeMech = { attackMs: number; brakeMs: number; gainDb: number; doublingS: number; pieceMs: number };
/** A wind to a point: how long it takes (s per minute wound) and how fast it may go (×). */
export type PointCurve = { ratio: number; speed: number };
/** A hold: how fast it starts and how fast it may get (×). */
export type HoldCurve = { start: number; top: number };

/** A wind to a point's longest (holding has no end but the edge). */
export const WIND_CAP_MS = 12_000;
/** Each setting's range, step and starting value. */
export const TAPE = {
  attackMs: { min: 100, max: 1500, step: 50, start: 400 },
  brakeMs: { min: 100, max: 1500, step: 50, start: 250 },
  gainDb: { min: -24, max: 0, step: 1, start: -12 },
  doublingS: { min: 0.5, max: 5, step: 0.25, start: 1.5 },
  pieceMs: { min: 100, max: 1000, step: 50, start: 350 },
} as const;
export const POINT = {
  ratio: { min: 0.5, max: 2, step: 0.1, start: 1 },
  speed: { min: 2, max: 16, step: 0.5, start: 8 },
} as const;
/** Forward to a point: its duration reaching down to half (the owner, 2026-10-27). */
export const POINT_FWD = { ...POINT, ratio: { min: 0.25, max: 2, step: 0.05, start: 1 } } as const;
export const HOLD = {
  start: { min: 2, max: 16, step: 0.5, start: 8 },
  top: { min: 8, max: 64, step: 4, start: 64 },
} as const;

/** `mode`, `ms`, `stopMs`: Start and stop's effect, its start (the wind-up) and its stop (the wind-down; its own, the
 *  tape's Brake being the tape's only: the owner, 2026-10-27); `skips`: it for next and previous too; `rewind`: Previous and
 *  Next wind the tape; `seekWinds`: a seek winds there instead of jumping; `edges`: holding past a track's edge stops,
 *  or winds on through the queue. No older settings carried over (the owner: everyone from the defaults). */
export type StartStop = {
  mode: 'off' | 'vinyl' | 'fade';
  ms: number;
  stopMs: number;
  skips: boolean;
  rewind: boolean;
  tape: TapeMech;
  back: PointCurve;
  fwd: PointCurve;
  holdBack: HoldCurve;
  holdFwd: HoldCurve;
  seekWinds: boolean;
  edges: 'stop' | 'queue';
};
export const RAMP_MS = { min: 200, max: 3000 };
type Range = { min: number; max: number; start: number };
const within = (v: unknown, r: Range) => (typeof v === 'number' && Number.isFinite(v) ? Math.min(r.max, Math.max(r.min, v)) : r.start);
/** Each of `ranges` as kept in `x`: what is missing or out of range at its start. */
function kept<R extends Record<string, Range>>(x: unknown, ranges: R): { [K in keyof R]: number } {
  const o = (x && typeof x === 'object' ? x : {}) as Record<string, unknown>;
  return Object.fromEntries(Object.entries(ranges).map(([k, r]) => [k, within(o[k], r)])) as { [K in keyof R]: number };
}
/** The starting values of `ranges` (Reset to defaults). */
export const startsOf = <R extends Record<string, Range>>(ranges: R) => kept(undefined, ranges);
/** A Start and stop as kept (here, or told by the app). */
export function startStopOf(x: Partial<StartStop>): StartStop {
  return {
    mode: x.mode === 'vinyl' || x.mode === 'fade' ? x.mode : 'off',
    ms: typeof x.ms === 'number' ? Math.min(RAMP_MS.max, Math.max(RAMP_MS.min, x.ms)) : 800,
    stopMs: typeof x.stopMs === 'number' ? Math.min(RAMP_MS.max, Math.max(RAMP_MS.min, x.stopMs)) : 800,
    skips: x.skips === true,
    rewind: x.rewind === true,
    tape: kept(x.tape, TAPE),
    back: kept(x.back, POINT),
    fwd: kept(x.fwd, POINT_FWD),
    holdBack: kept(x.holdBack, HOLD),
    holdFwd: kept(x.holdFwd, HOLD),
    seekWinds: x.seekWinds === true,
    edges: x.edges === 'queue' ? 'queue' : 'stop',
  };
}

/** A wind to a point that way, as the players use it. */
export function pointWind(s: StartStop, dir: 1 | -1): Wind {
  const p = dir > 0 ? s.fwd : s.back;
  return { ratio: p.ratio, speed: p.speed, attackMs: s.tape.attackMs, releaseMs: s.tape.brakeMs, pieceMs: s.tape.pieceMs, gainDb: s.tape.gainDb };
}
/** A hold that way, as the players use it. */
export function holdWind(s: StartStop, dir: 1 | -1): HoldWind {
  const h = dir > 0 ? s.holdFwd : s.holdBack;
  return { ...pointWind(s, dir), speed: h.start, top: Math.max(h.top, h.start), doublingS: s.tape.doublingS };
}
