// A sidechain compressor for values (decision 0057; the owner: the governor goes, "we shouldn't reinvent it"): the
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
