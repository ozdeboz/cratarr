// Where each of the music's signals has been lately, for the curve editor's envelope (decision 0046, amended for 64.1;
// the owner: "this gives us the best way to tune the curve to the signal"): the last half minute at ten a second, the
// whole track's signals and each band's. Pure.

/** How many of a signal's values are kept (half a minute at ten a second), and the bins across its 0..1. */
export const HISTORY = 300;
export const BINS = 60;

export type History = {
  tracks: number;
  bandSignals: number;
  bands: number;
  /** Where the next values go, and how many there are (at most `HISTORY`). */
  at: number;
  count: number;
  /** A signal's values one after the other: the track's by signal, the bands' by band, then signal. */
  track: Float32Array;
  band: Float32Array;
};

export const newHistory = (tracks: number, bandSignals: number, bands: number): History => ({
  tracks,
  bandSignals,
  bands,
  at: 0,
  count: 0,
  track: new Float32Array(tracks * HISTORY),
  band: new Float32Array(bands * bandSignals * HISTORY),
});

/** The signals now, remembered (the oldest let go of). */
export function remember(h: History, track: ArrayLike<number>, bands: ArrayLike<number>[]) {
  for (let s = 0; s < h.tracks; s++) h.track[s * HISTORY + h.at] = track[s] ?? 0;
  for (let b = 0; b < h.bands; b++) for (let s = 0; s < h.bandSignals; s++) h.band[(b * h.bandSignals + s) * HISTORY + h.at] = bands[b]?.[s] ?? 0;
  h.at = (h.at + 1) % HISTORY;
  h.count = Math.min(HISTORY, h.count + 1);
}

/** How often a signal has been at each bin (a track's signal, or a band's: one band's, or with `band` null all six's),
 *  lightly smoothed (1-2-3-2-1), the most 1; null while nothing is remembered. */
export function densityOf(h: History, which: { track: number } | { signal: number; band: number | null }): Float32Array | null {
  if (!h.count) return null;
  const counts = new Float32Array(BINS);
  const add = (from: Float32Array, start: number) => {
    for (let i = 0; i < h.count; i++) counts[Math.min(BINS - 1, Math.max(0, Math.floor(from[start + i] * BINS)))]++;
  };
  if ('track' in which) add(h.track, which.track * HISTORY);
  else for (let b = 0; b < h.bands; b++) if (which.band === null || which.band === b) add(h.band, (b * h.bandSignals + which.signal) * HISTORY);
  const KERNEL = [1, 2, 3, 2, 1];
  const out = new Float32Array(BINS);
  let most = 0;
  for (let i = 0; i < BINS; i++) {
    let sum = 0;
    let weight = 0;
    for (let k = -2; k <= 2; k++)
      if (i + k >= 0 && i + k < BINS) {
        sum += KERNEL[k + 2] * counts[i + k];
        weight += KERNEL[k + 2];
      }
    out[i] = sum / weight;
    most = Math.max(most, out[i]);
  }
  if (most > 0) for (let i = 0; i < BINS; i++) out[i] /= most;
  return out;
}
