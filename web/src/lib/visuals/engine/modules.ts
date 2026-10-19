// The one engine's modules for what aspects.ts did out of sight (decision 0057, §3): each a small stepper with its own
// state, its knobs and its outputs, so the graph can show it and the evaluator can wire it. Pure; the same arithmetic
// as aspects.ts, so the equivalence harness holds them to it.

/** A beat's turn of the transforms: once around in 32 beats (aspects.ts's STEP). */
export const STEP = (2 * Math.PI) / 32;

/** Envelope: the loudness as heard, followed over a fifth of a second, as far as Intensity lets it count (`level`, what
 *  a cable from the loudness reads); `out` that, while playing, else nothing: every cable's depth (normalled). `glow`:
 *  1 playing, dimmer paused, followed over half a second. */
export type EnvelopeState = { intensity: number; lit: number; glow: number };
export const newEnvelope = (): EnvelopeState => ({ intensity: 0.5, lit: 0.5, glow: 1 });
export function stepEnvelope(st: EnvelopeState, heard: number, playing: boolean, intensity: number, dt: number) {
  st.intensity += (heard - st.intensity) * Math.min(1, dt * 5);
  st.lit = 1 - intensity * (1 - st.intensity);
  st.glow += ((playing ? 1 : 0.55) - st.glow) * Math.min(1, dt * 2);
  return { level: st.lit, out: playing ? st.lit : 0, glow: st.glow };
}

/** Clock: the turning's rate at the track's tempo (`bps`), a route's full amount turning a part once round in 16 beats,
 *  at Speed (and the sidechaining's pace); a quarter with reduced motion; paused, it slows to a stop over two thirds of a
 *  second. */
export type ClockState = { rate: number };
export const newClock = (): ClockState => ({ rate: 0 });
export function stepClock(st: ClockState, bps: number, playing: boolean, speed: number, pace: number, still: boolean, dt: number) {
  const want = playing ? STEP * bps * (still ? 0.25 : 1) * speed * pace : 0;
  st.rate += (want - st.rate) * Math.min(1, dt * 1.5);
  return st.rate;
}

/** Track: all of it as a track starts (the visuals opening on one too), gone over its first beat. */
export type TrackState = { key: string | null | undefined; at: number };
export const newTrack = (): TrackState => ({ key: undefined, at: 0 });
export function stepTrack(st: TrackState, key: string | null, now: number, bps: number): number {
  if (key !== st.key) {
    st.key = key;
    st.at = now;
  }
  return Math.max(0, 1 - ((now - st.at) / 1000) * bps);
}

/** Flash: a quickly changing value as its slow course (followed twice over SLOW_S: a beat's or a hit's swing all but
 *  gone from it, a passage's change kept) and Flash's share of its swing about that course (`held`: as much of it as
 *  the sidechaining lets through). One per target, its course its own. */
export const SLOW_S = 0.3;
export type FlashState = number[];
export const newFlash = (): FlashState => [];
/** How far the courses follow this frame: slower while the sidechaining holds the swing back. */
export const courseStep = (dt: number, swingLever: number) => 1 - Math.exp((-dt * swingLever) / SLOW_S);
export function stepFlash(course: FlashState, v: number, follow: number, flash: number, swingLever: number, held = 1): number {
  if (!course.length) course.push(v, v);
  course[0] += (v - course[0]) * follow;
  course[1] += (course[0] - course[1]) * follow;
  return course[1] + flash * swingLever * held * (v - course[1]);
}

/** Moments: what a step's cables give rising past a half fires it; ready again once that falls under a third, and not
 *  within MOMENT_GAP_MS (times the sidechaining's gap). */
export const MOMENT_GAP_MS = 2000;
export type MomentState = { armed: boolean; at: number };
export const newMoment = (): MomentState => ({ armed: true, at: -Infinity });
export function stepMoment(mo: MomentState, given: number, now: number, gapLever: number): boolean {
  if (given < 1 / 3) mo.armed = true;
  if (!(given > 0.5 && mo.armed && now - mo.at > MOMENT_GAP_MS * gapLever)) return false;
  mo.armed = false;
  mo.at = now;
  return true;
}
