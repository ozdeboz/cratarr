// A range condition's scale (lib/ConditionCard, DensityRange): from where its tracks are, not from the library's lowest
// to its highest value (the owner, 2026-11-02: BPM ran 30 to 999, a few tags of 999 squeezing every real tempo into a
// sixth of the bar). Each tail holding no more than `tail` of the tracks is cut off; a handle at an end is an open end,
// so what lies beyond stays reachable. Pure.

/** The bins and their span with the thin tails cut off (each at most `tail` of all); whole where nothing is cut. */
export function trimScale(bins: readonly number[], min: number, max: number, tail = 0.005): { bins: number[]; min: number; max: number } {
  const total = bins.reduce((n, b) => n + b, 0);
  if (!total || bins.length < 3 || max <= min) return { bins: [...bins], min, max };
  const step = (max - min) / bins.length;
  let i = 0;
  for (let seen = 0; i < bins.length - 1 && seen + bins[i] <= total * tail; i++) seen += bins[i];
  let j = bins.length - 1;
  for (let seen = 0; j > i && seen + bins[j] <= total * tail; j--) seen += bins[j];
  return { bins: bins.slice(i, j + 1), min: min + i * step, max: min + (j + 1) * step };
}
