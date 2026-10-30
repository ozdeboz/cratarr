// The one engine's modules for what the old engine (aspects.ts, gone in 0.10) did out of sight (decision 0057, §3):
// each a small stepper with its own state, its knobs and its outputs, so the graph can show it and the evaluator can
// wire it. Pure; the golden frames hold what they give (tests/golden-frames.test.ts).

/** A beat's turn of the transforms: once around in 32 beats. */
export const STEP = (2 * Math.PI) / 32;

/** Floor: none at 0, then from 0.002 of the display's scale (a stray speck's) up to 1 (the flame's mean), evenly in its
 *  log. */
export const floorOf = (v: number) => (v > 0 ? 0.002 * 500 ** v : 0);

/** Haze (−1..1, 65, decision 0050) as the display shows it: Depth's and Floor's own curves on one axis, 0 as their
 *  defaults were (gamma 3, no floor). More haze lifts the faint mist (the gamma up to 6); less cuts it (the gamma down
 *  to 1.5, a floor up to Floor's 75 %: a fifth of the flame's mean; its 100 %, the mean, blacked out half the flame). */
export const hazeOf = (h: number) => ({ gamma: 3 * 2 ** h, floor: h < 0 ? floorOf(0.75 * -h) : 0 });

/** The Loudness (an Envelope Follower on Music's loudness): the loudness as heard, followed (rising over `attack`,
 *  falling over `release`, s), as far as Intensity lets it count (`level`, what a cable from the loudness reads); `out`
 *  that, while playing, else nothing: every cable's depth (normalled). `glow`: 1 playing, dimmer paused, followed over
 *  half a second. */
export type EnvelopeState = { intensity: number; lit: number; glow: number };
export const newEnvelope = (): EnvelopeState => ({ intensity: 0.5, lit: 0.5, glow: 1 });
export function stepEnvelope(st: EnvelopeState, heard: number, playing: boolean, intensity: number, dt: number, attack = 0.2, release = 0.2) {
  st.intensity += (heard - st.intensity) * Math.min(1, dt / Math.max(1e-3, heard > st.intensity ? attack : release));
  st.lit = 1 - intensity * (1 - st.intensity);
  st.glow += ((playing ? 1 : 0.55) - st.glow) * Math.min(1, dt * 2);
  return { level: st.lit, out: playing ? st.lit : 0, glow: st.glow };
}

/** Clock: the turning's rate at the track's tempo (`bps`), a route's full amount turning a part once round in 16 beats,
 *  at Speed (and the sidechaining's pace); a quarter with reduced motion; paused, it slows to a stop over two thirds of a
 *  second. */
/** Its rate (radians a second) and where in a turn it is (`phase`, 0..1: its Turn output). */
export type ClockState = { rate: number; phase: number };
export const newClock = (): ClockState => ({ rate: 0, phase: 0 });
export function stepClock(st: ClockState, bps: number, playing: boolean, speed: number, pace: number, dt: number) {
  const want = playing ? STEP * bps * speed * pace : 0;
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

/** Accent (was Flash): a quickly changing value as its slow course (followed twice over SLOW_S: a beat's or a hit's
 *  swing all but gone from it, a passage's change kept) and Accent's share of its swing about that course (`held`: as
 *  much of it as the sidechaining lets through). One per target, its course its own. */
export const SLOW_S = 0.3;
export type AccentState = number[];
export const newAccent = (): AccentState => [];
/** How far the courses follow this frame: slower while the sidechaining holds the swing back. */
export const courseStep = (dt: number, swingLever: number) => 1 - Math.exp((-dt * swingLever) / SLOW_S);
export function stepAccent(course: AccentState, v: number, follow: number, accent: number, swingLever: number, held = 1): number {
  if (!course.length) course.push(v, v);
  course[0] += (v - course[0]) * follow;
  course[1] += (course[0] - course[1]) * follow;
  return course[1] + accent * swingLever * held * (v - course[1]);
}

/** A trigger's strike on one channel, as an ADSR reads it: how long since the last (s; Infinity: none yet) and its
 *  velocity, and the next one where it is known ahead (the server's hits): how long until it, and its velocity. */
export type Strike = { since: number; velocity: number; next: { until: number; velocity: number } | null };
export const NO_STRIKE: Strike = { since: Infinity, velocity: 0, next: null };

/** An ADSR's knobs for one channel (s; `sustain` a share of the velocity). */
export type AdsrKnobs = { attack: number; decay: number; sustain: number; release: number; retrigger?: number };

const smooth = (x: number) => (x <= 0 ? 0 : x >= 1 ? 1 : x * x * (3 - 2 * x));

/** ADSR (decision 0057; what Linger did out of sight): a channel's level now, from its strikes. Each strike rises over
 *  the attack (smoothly, ending on the strike where it is known ahead; heard live, it is on at once), falls over the
 *  decay to `sustain` of its velocity (no decay: none), then rings down from there over the release (a time constant,
 *  gone after five). A strike to come while the last still rings (`retrigger`): 0, the more of the two (it never dips
 *  under what is ringing until the new strike lands: today's lights); 1, as a synth's envelope does, its attack from the
 *  level the ringing was at when the attack began. The triggers are strikes, gates of no length: what a held gate
 *  sustains comes with the gates. */
export function adsrOf(s: Strike, k: AdsrKnobs): number {
  const decay = Math.max(0, k.decay);
  const release = Math.max(1e-6, k.release);
  /** The last strike's level `since` seconds after it. */
  const ring = (since: number) => {
    if (!(since >= 0) || since === Infinity) return 0;
    const after = decay > 0 ? Math.max(0, since - decay) : since;
    const from = decay > 0 ? 1 - (1 - clamp01(k.sustain)) * Math.min(1, since / decay) : 1;
    return after > release * 5 ? 0 : s.velocity * from * (decay > 0 && since < decay ? 1 : Math.exp(-after / release));
  };
  const on = ring(s.since);
  if (!s.next || !(k.attack > 0)) return on;
  const rise = smooth(1 - s.next.until / k.attack);
  if ((k.retrigger ?? 0) >= 0.5) {
    const start = ring(s.since - (k.attack - s.next.until));
    return start + (s.next.velocity - start) * rise;
  }
  return Math.max(on, s.next.velocity * rise);
}
const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
