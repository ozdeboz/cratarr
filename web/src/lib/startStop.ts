// Start and stop's settings (lib/prefs; the app's too, lib/app): kept apart from both, as each imports the other.

/** `rewind…`: Tape rewind's tunings (the owner, 2026-10-27): the whole rewind's length (ms), its cruising speed (×),
 *  each heard piece of it where the middle is cut short (ms), and the share of it the spin-up and the brake take each. */
export type StartStop = {
  mode: 'off' | 'vinyl' | 'fade';
  ms: number;
  skips: boolean;
  rewind: boolean;
  rewindMs: number;
  rewindSpeed: number;
  rewindPiece: number;
  rewindRamp: number;
};
export const RAMP_MS = { min: 200, max: 3000 };
/** Tape rewind's tunings: each one's range, step and starting value. */
export const REWIND = {
  ms: { min: 1000, max: 8000, step: 250, start: 2500 },
  speed: { min: 2, max: 16, step: 0.5, start: 6 },
  piece: { min: 100, max: 1000, step: 50, start: 350 },
  ramp: { min: 0.05, max: 0.4, step: 0.05, start: 0.2 },
} as const;
type Range = { min: number; max: number; start: number };
const within = (v: unknown, r: Range) => (typeof v === 'number' && Number.isFinite(v) ? Math.min(r.max, Math.max(r.min, v)) : r.start);
/** A Start and stop as kept (here, or told by the app): what is missing or out of range at its start. */
export function startStopOf(x: Partial<StartStop>): StartStop {
  return {
    mode: x.mode === 'vinyl' || x.mode === 'fade' ? x.mode : 'off',
    ms: typeof x.ms === 'number' ? Math.min(RAMP_MS.max, Math.max(RAMP_MS.min, x.ms)) : 800,
    skips: x.skips === true,
    rewind: x.rewind === true,
    rewindMs: within(x.rewindMs, REWIND.ms),
    rewindSpeed: within(x.rewindSpeed, REWIND.speed),
    rewindPiece: within(x.rewindPiece, REWIND.piece),
    rewindRamp: within(x.rewindRamp, REWIND.ramp),
  };
}
