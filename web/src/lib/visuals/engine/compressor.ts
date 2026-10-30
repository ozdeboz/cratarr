// A sidechain compressor for values (decision 0057; the owner: standard sidechaining, "we shouldn't reinvent it"): the
// textbook design of Giannoulis, Massberg and Reiss, "Digital Dynamic Range Compressor Design: A Tutorial and
// Analysis", JAES 60(6), 2012. The key's level in dB; a soft-knee gain computer (their eq. 4); the gain reduction
// smoothed by the smooth decoupled peak detector (their eq. 17), attack and release as time constants in seconds,
// so any frame rate gives the same; makeup; the input scaled by the gain. The key is what is measured (a meter of the
// picture), the input what is held back (a knob's cable): a sidechain.

/** The knobs: threshold in the key's own units (where reduction begins), ratio (1: none; Infinity: a limiter), knee
 *  width in dB (0: hard), attack and release in seconds, makeup in dB. */
export type CompressorKnobs = { threshold: number; ratio: number; knee: number; attack: number; release: number; makeup: number };

export type CompressorState = { peak: number; smooth: number };
export const newCompressor = (): CompressorState => ({ peak: 0, smooth: 0 });

/** The quietest level the key is measured at (dB): nothing is never louder than this. */
const FLOOR_DB = -120;
const toDb = (v: number) => (v > 0 && Number.isFinite(v) ? Math.max(FLOOR_DB, 20 * Math.log10(v)) : FLOOR_DB);

/** The static curve (their eq. 4): the output level in dB for an input level `x` in dB. */
export function gainComputer(x: number, thresholdDb: number, ratio: number, knee: number): number {
  const slope = ratio === Infinity ? 0 : 1 / Math.max(1, ratio);
  const over = x - thresholdDb;
  if (knee > 0 && 2 * Math.abs(over) <= knee) return x + ((slope - 1) * (over + knee / 2) ** 2) / (2 * knee);
  return 2 * over > knee ? thresholdDb + over * slope : x;
}

/** One step, `dt` seconds on: the input held back by what the key asks. `reduction` in dB (0: none); `gain` the
 *  factor (makeup in it). */
export function stepCompressor(st: CompressorState, input: number, key: number, k: CompressorKnobs, dt: number): { out: number; reduction: number; gain: number } {
  const x = toDb(key);
  const want = x - gainComputer(x, toDb(k.threshold), k.ratio, Math.max(0, k.knee));
  const step = Number.isFinite(dt) ? Math.max(0, dt) : 0;
  const a = k.attack > 0 ? Math.exp(-step / k.attack) : 0;
  const r = k.release > 0 ? Math.exp(-step / k.release) : 0;
  // The smooth decoupled peak detector: the release's level, not under what is asked; then smoothed by the attack.
  st.peak = Math.max(want, r * st.peak + (1 - r) * want);
  st.smooth = a * st.smooth + (1 - a) * st.peak;
  const reduction = Math.max(0, st.smooth);
  const gain = 10 ** ((k.makeup - reduction) / 20);
  return { out: input * gain, reduction, gain };
}

/** A ducker (the broadcast sidechain: while the key is over its threshold, the input is held down toward a fixed
 *  depth, coming in over the attack; held that long after the key last went over; let back over the release once the
 *  key is under its threshold times `hysteresis`, and in between held where it is, the window of a broadcast AGC's
 *  gating): what a meter that counts (flashes a second, looks a minute) needs, where a compressor's reduction in
 *  proportion to how far over would settle with the count still over. The window lets it settle as deep as the key
 *  needs instead of swinging between none and all. */
export type DuckerKnobs = { threshold: number; depth: number; attack: number; hold: number; release: number; hysteresis: number };
export type DuckerState = { reduction: number; since: number };
export const newDucker = (): DuckerState => ({ reduction: 0, since: Infinity });

export function stepDucker(st: DuckerState, input: number, key: number, k: DuckerKnobs, dt: number): { out: number; reduction: number; gain: number } {
  const step = Number.isFinite(dt) ? Math.max(0, dt) : 0;
  const v = Number.isFinite(key) ? key : Infinity;
  if (v > k.threshold) st.since = 0;
  else st.since += step;
  const target = v > k.threshold ? k.depth : v < k.threshold * k.hysteresis && st.since >= k.hold ? 0 : st.reduction;
  const time = target > st.reduction ? k.attack : k.release;
  st.reduction += (target - st.reduction) * (time > 0 ? 1 - Math.exp(-step / time) : 1);
  const gain = 10 ** (-st.reduction / 20);
  return { out: input * gain, reduction: st.reduction, gain };
}
