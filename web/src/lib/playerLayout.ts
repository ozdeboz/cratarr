// The full-screen player's layout (decision 0018): a space budget, not breakpoints. The player measures its own
// controls at each step and decides which elements give way, how large the cover is, and whether the cover sits
// above the controls (portrait) or beside them (landscape).

/** What gives way when the player runs out of height, first to last. The title, play/pause and the playlist count
 *  never do. Step n is dropped at level n + 1. */
export const STEPS = ['gaps', 'buttons', 'pill', 'chips', 'title', 'times', 'progress'] as const;
export type Step = (typeof STEPS)[number];

/** The share of the largest possible cover that the cover may give up to keep each step, at a 300 px cover. */
const ALLOWANCE: Record<Step, number> = {
  gaps: 0.04,
  buttons: 0.04,
  pill: 0.08,
  chips: 0.1,
  title: 0.12,
  times: 0.12,
  progress: 0.16,
};
/** The cover never gets smaller than this; below it the panel scrolls. */
export const MIN_COVER = 96;
/** The controls' width: in portrait at most this, in landscape a share of the width within these bounds. */
const CONTROLS_MAX = 560;
const LANDSCAPE_SHARE = 0.45;
const LANDSCAPE_MIN = 280;
/** Between the cover and the controls (and around the cover in landscape). */
const GAP = 24;

/** A larger cover may give up more: 1× up to 300 px, growing to 3× from 900 px. */
function allowance(step: Step, largest: number): number {
  const scale = Math.min(3, Math.max(1, (largest - 300) / 300));
  return Math.min(0.6, ALLOWANCE[step] * scale);
}

export type Fit = { landscape: boolean; level: number; cover: number; controls: number };

/** Lays the player out in a box of `width` × `height`. `measure(landscape, level, controlsWidth)` returns the
 *  controls' height with steps `0..level-1` dropped. */
export function fit(width: number, height: number, measure: (landscape: boolean, level: number, controlsWidth: number) => number): Fit {
  // Portrait: the controls under the cover. Drop steps while keeping the next one would cost the cover more than its
  // allowance.
  const pw = Math.min(width, CONTROLS_MAX);
  // The largest cover there can be: with everything that can give way gone (the title and play/pause always need
  // room, so the whole short side is never on offer). Each step's allowance is a share of this.
  const largest = Math.max(MIN_COVER, Math.min(width, height - measure(false, STEPS.length, pw) - GAP));
  let level = 0;
  let cover = Math.min(width, height - measure(false, 0, pw) - GAP);
  while (level < STEPS.length && cover < largest * (1 - allowance(STEPS[level], largest))) {
    level += 1;
    cover = Math.min(width, height - measure(false, level, pw) - GAP);
  }
  const portrait: Fit = { landscape: false, level, cover: Math.max(MIN_COVER, Math.floor(cover)), controls: pw };

  // Landscape: the controls beside the cover; the cover takes the height, so only the controls' own height counts.
  const lw = Math.min(CONTROLS_MAX, Math.max(LANDSCAPE_MIN, Math.round(width * LANDSCAPE_SHARE)));
  let llevel = 0;
  while (llevel < STEPS.length && measure(true, llevel, lw) > height) llevel += 1;
  const lcover = Math.floor(Math.min(width - lw - GAP, height));
  const landscape: Fit = { landscape: true, level: llevel, cover: lcover, controls: lw };

  // The layout that gives the larger cover wins (not the device's orientation).
  return lcover > portrait.cover && lcover >= MIN_COVER ? landscape : portrait;
}
