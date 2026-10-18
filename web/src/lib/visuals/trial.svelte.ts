// The rating page's trial (decision 0046, amended): while `settings` is set, the visuals draw with it instead of the
// saved settings, and nothing of it is saved or shared; `meters` is the loop's meters now, and `loop` what else the
// loop knows that a rating must be read against (the music's own activity, whether the flash rule held the picture
// back, the device and its pace), for the page to record. `rating.run` is whether the page runs (0: not), a new number
// for each start; kept here rather than in the visuals, as they are drawn anew where they move (beside the page or over
// it) and a run goes on there.
import type { MeterValues } from './meters';
import type { VisualSettings } from './settings.svelte';

/** What the loop knows now, beside the meters (every 250 ms). */
export type TrialLoop = {
  /** The music: its intensity as heard (0..1), beats a second, hits a second (all bands). */
  intensity: number;
  bps: number;
  hitsPerSecond: number;
  /** The music's activity as the governor's budgets take it (0..1: `budgetOf`). */
  activity: number;
  /** The flash rule's lever: 1 open, less where it held Flash's swing back. */
  swing: number;
  /** How often the look drawn changed other than by how it behaves (a track's look, a recalled one, one from the
   *  group, another colour mode or the cover's colours: not Drift's walk nor the Symmetry moments), and whether such a
   *  change is still blending in. */
  looks: number;
  settling: boolean;
  /** The picture's size on the screen (CSS pixels). */
  view: [number, number];
  /** What the device draws: points, steps, frames a second; and which device it is. */
  points: number;
  steps: number;
  fps: number;
  device: string;
};

export const trial = $state<{ settings: VisualSettings | null; meters: MeterValues | null; loop: TrialLoop | null }>({ settings: null, meters: null, loop: null });

export const rating = $state({ run: 0 });
