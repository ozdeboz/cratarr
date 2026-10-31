// The tape's settings (Settings › This device › Tape; lib/prefs, the app's too, lib/app): kept apart from both, as each
// imports the other. Start and stop, the tape's own mechanics, and its six motions (the owner, 2026-10-28: each on its
// own, one setting each, its bottom Off): back to the start (Previous), a seek back, holding back; forward to the end
// (Next), a seek forward, holding forward (Previous or Next, ← or →, held).

/** A motion's envelope as the players use it (lib/windShape, lib/localPlayer): `ratio`, a wind to a point's length,
 *  seconds per minute wound (at most WIND_CAP_MS); `speed`, its speed limit or a hold's starting speed (×);
 *  `attackMs`/`releaseMs`, the spin-up and the brake; `pieceMs`, each heard piece where a wind is cut short; `gainDb`,
 *  how much quieter the tape is while it winds. Made from the settings (pointWind, holdWind), never kept. */
export type Wind = { ratio: number; speed: number; attackMs: number; releaseMs: number; pieceMs: number; gainDb: number };
/** A hold's envelope: also how it gets faster (doubling every `doublingS` seconds of its stream) and how fast at most. */
export type HoldWind = Wind & { doublingS: number; top: number };

/** The tape's mechanics, every motion's alike: spin-up, the one brake (a Vinyl pause's too), the wind volume. */
export type TapeMech = { attackMs: number; brakeMs: number; gainDb: number };
/** The winds to a point: Previous's back to the start, Next's on to the end, a seek back or forward. */
export type PointMotion = 'toStart' | 'seekBack' | 'toEnd' | 'seekFwd';
export type HoldMotion = 'holdBack' | 'holdFwd';
export type Motion = PointMotion | HoldMotion;

/** Previous restarts the track after this many seconds (Finamp: 5): every player's rule, the app's and Kodi's (the
 *  server's) too, so the remote predicts with it as well (shared/rules.json). */
export const RESTART_AFTER = 5;
/** A wind to a point's longest (holding has no end but the edge). */
export const WIND_CAP_MS = 12_000;
/** Each setting's range, step and starting value (the owner, 2026-10-31: "better defaults": spin-up and brake 0.8 s,
 *  the wind at -6 dB). */
export const TAPE = {
  attackMs: { min: 100, max: 1500, step: 50, start: 800 },
  brakeMs: { min: 100, max: 1500, step: 50, start: 800 },
  gainDb: { min: -24, max: 0, step: 1, start: -6 },
} as const;
const POINT = { min: 0, max: 2, step: 0.05, start: 1 } as const;
const HOLD = { min: 0, max: 12, step: 0.5, start: 4.5 } as const;
/** Each motion's one setting, 0: off (the plain way: a jump, a seek, no hold). A wind to a point's duration (s per
 *  minute wound); a hold's time to its top speed (s, its spin-up in it). */
export const MOTION = { toStart: POINT, seekBack: POINT, holdBack: HOLD, toEnd: POINT, seekFwd: POINT, holdFwd: HOLD } as const;
/** What is no longer set (the owner, 2026-10-28: good defaults): a wind to a point's speed limit, a hold's starting
 *  and top speed. A hold's top is the fastest a browser plays a track (16x), so letting go hands the tape back to the
 *  track's own player at once, at the speed it has, never a cut-over (the owner, 2026-10-28). */
export const POINT_SPEED = 8;
export const HOLD_START = 4;
export const HOLD_TOP = 16;
/** A hold's true speed at most (×): past its heard top it goes on getting faster by cutting, heard in pieces at that
 *  top (flight_max; the owner, 2026-10-28: top speed is the heard speed, not the flight's). */
export const FLIGHT_TOP = 128;
/** How long each heard piece of a wind cut short is (ms; the owner, 2026-10-28: no setting, a good default). */
export const PIECE_MS = 350;
/** A held wind's brake, let go or paused (the owner, 2026-10-29: "our fixed duration brake is really kind of weird.
 *  When spinning down from high speeds, braking takes longer"): constant deceleration, DECEL_X × a second at its mean,
 *  from its speed `v0` to its end's `to` (1× forwards, a stand backwards or paused); the Brake as set is its shortest,
 *  a slow wind's as before. Its shape stays the raised cosine. The app's Plan.lengthMs is the same rule
 *  (0.9.15-2); taps, seeks and the tape's own stop keep the Brake. */
export const DECEL_X = 64;
/** The brake's three rules as one set (the 0075 spike, 2026-10-30: the same rules from the Rust rules crate, as
 *  WebAssembly, lib/wasm/brake.ts): its length, its speed `t` ms in, and its way (track s) after `s` seconds. */
export type BrakeRules = {
  lengthMs: (v0: number, to: number, brakeMs: number) => number;
  speed: (v0: number, to: number, ms: number, t: number) => number;
  travel: (v: number, ms: number, s: number, to: number) => number;
};
/** This page's own: a raised cosine from `v0` to `to` over `ms`, then `to`. */
export const JS_BRAKE: BrakeRules = {
  lengthMs: (v0, to, brakeMs) => Math.max(brakeMs, Math.round((Math.abs(v0 - to) / DECEL_X) * 1000)),
  speed: (v0, to, ms, t) => to + ((v0 - to) * (1 + Math.cos(Math.PI * Math.min(1, Math.max(0, t / ms))))) / 2,
  travel: (v, ms, s, to) => {
    const T = ms / 1000;
    const u = Math.min(s, T);
    return to * u + ((v - to) / 2) * (u + (T / Math.PI) * Math.sin((Math.PI * u) / T)) + to * Math.max(0, s - T);
  },
};
let brake: BrakeRules = JS_BRAKE;
/** The rules in use from now on (the spike's switch: lib/wasm/brake.ts sets the WebAssembly's once it is ready). */
export const useBrake = (b: BrakeRules) => void (brake = b);
export const brakeLengthMs = (v0: number, to: number, brakeMs: number) => brake.lengthMs(v0, to, brakeMs);
/** Such a brake's speed `t` ms into it (×). */
export const brakeSpeed = (v0: number, to: number, ms: number, t: number) => brake.speed(v0, to, ms, t);
/** How far such a brake from `v`× to `to`× over `ms` has gone after `s` seconds (track s). */
export const brakeTravel = (v: number, ms: number, s: number, to = 1) => brake.travel(v, ms, s, to);

/** `mode`, `ms`, `stopMs`: Start and stop's effect, its start (the wind-up) and its stop (the wind-down; its own, the
 *  tape's Brake being the tape's only: the owner, 2026-10-27); `skips`: it for next and previous too; `rewind`: the tape
 *  winds at all; `motions`: each motion's setting (0: off); `edges`: holding past a track's edge stops, or winds on
 *  through the queue. No older settings carried over (the owner: everyone from the defaults). */
export type StartStop = {
  mode: 'off' | 'vinyl' | 'fade';
  ms: number;
  stopMs: number;
  skips: boolean;
  rewind: boolean;
  tape: TapeMech;
  motions: Record<Motion, number>;
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
/** The starting values of `ranges` (a reset). */
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
    motions: kept(x.motions, MOTION),
    edges: x.edges === 'queue' ? 'queue' : 'stop',
  };
}

/** Whether a motion winds the tape: winding on, and it not off. */
export const windsBy = (s: StartStop, m: Motion) => s.rewind && s.motions[m] > 0;
/** A wind to a point, as the players use it. */
export function pointWind(s: StartStop, m: PointMotion): Wind {
  return { ratio: s.motions[m], speed: POINT_SPEED, attackMs: s.tape.attackMs, releaseMs: s.tape.brakeMs, pieceMs: PIECE_MS, gainDb: s.tape.gainDb };
}
/** A hold that way, as the players use it: from HOLD_START, doubling to HOLD_TOP in its time to top speed (after its
 *  spin-up). Off, it still gives the tape's mechanics (the direction knob's spin-up and brake). */
export function holdWind(s: StartStop, dir: 1 | -1, top = HOLD_TOP): HoldWind {
  const t = s.motions[dir > 0 ? 'holdFwd' : 'holdBack'];
  // Where the player plays slower at most (a browser's: lib/rates), it starts lower too, so it still gets faster.
  const start = Math.min(HOLD_START, top / 2);
  const doubling = Math.max(0.05, t - s.tape.attackMs / 1000) / Math.log2(top / start);
  return { ...pointWind(s, dir > 0 ? 'toEnd' : 'toStart'), speed: start, top, doublingS: t > 0 ? doubling : 1.5 };
}
