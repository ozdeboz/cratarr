// The rating page's trial (decision 0046, amended): while `settings` is set, the visuals draw with it instead of the
// saved settings, and nothing of it is saved or shared; `meters` is the loop's meters now, for the page to record.
import type { MeterValues } from './meters';
import type { VisualSettings } from './settings.svelte';

export const trial = $state<{ settings: VisualSettings | null; meters: MeterValues | null }>({ settings: null, meters: null });
