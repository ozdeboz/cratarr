// The rating page's trial (decision 0046, amended): while `settings` is set, the visuals draw with it instead of the
// saved settings, and nothing of it is saved or shared; `meters` is the loop's meters now, and `loop` what else the
// loop knows that a rating must be read against (the music's own activity, whether the flash rule held the picture
// back, the device and its pace), for the page to record.
import type { MeterValues } from './meters';
import type { VisualSettings } from './settings.svelte';

/** What the loop knows now, beside the meters (every 250 ms). */
export type TrialLoop = {
  /** The music: its intensity as heard (0..1), beats a second, hits a second (all bands). */
  intensity: number;
  bps: number;
  hitsPerSecond: number;
  /** The flash rule's lever: 1 open, less where it held Flash's swing back. */
  swing: number;
  /** What the device draws: points, steps, frames a second; and which device it is. */
  points: number;
  steps: number;
  fps: number;
  device: string;
};

export const trial = $state<{ settings: VisualSettings | null; meters: MeterValues | null; loop: TrialLoop | null }>({ settings: null, meters: null, loop: null });
