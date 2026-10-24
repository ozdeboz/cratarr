// Each look of the full-screen player's bands with its own settings (the owner, 2026-10-24: a default for each look,
// the user's overrides remembered per look, a reset each). What the user changed is kept apart from the defaults
// (prefs.playerBandsTunes), so a better default reaches everyone who left a setting alone.

import { type BandLookName, type BandTune, prefs } from './prefs.svelte';

/** Each look as it starts: the owner's own tuning on the tablet (2026-10-24, 15:55–16:01), measured from the sliders'
 *  knobs in the screenshots (each slider's own step, then its value); Hairlines' Window and Cycles as the tablet kept
 *  them at 19:31 (the owner, for 0.8.9). */
export const TUNE_DEFAULTS: Record<BandLookName, BandTune> = {
  aurora: { speed: 6.24, window: 4.017, cycles: 0.25, height: 1.443, count: 3, gap: 0, direction: 'left', taper: 0, fade: 0 },
  sirius: { speed: 0.231, window: 1.002, cycles: 1.693, height: 2.5, count: 4, gap: 0, direction: 'right', taper: 0.84, fade: 0 },
  hairlines: { speed: 1.623, window: 0.25, cycles: 4, height: 0.61, count: 12, gap: 1, direction: 'left', taper: 1, fade: 1 },
  meter: { speed: 10.4, window: 0.25, cycles: 1, height: 1.179, count: 32, gap: 0, direction: 'left', taper: 0, fade: 0 },
};

/** A look's settings: its defaults with the user's changes over them. */
export const tuneOf = (look: BandLookName): BandTune => ({ ...TUNE_DEFAULTS[look], ...prefs.playerBandsTunes[look] });

/** Sets one of a look's settings; one back at its default is no longer an override. */
export function setTune<K extends keyof BandTune>(look: BandLookName, key: K, value: BandTune[K]) {
  const mine = { ...prefs.playerBandsTunes[look] };
  if (value === TUNE_DEFAULTS[look][key]) delete mine[key];
  else mine[key] = value;
  prefs.playerBandsTunes = { ...prefs.playerBandsTunes, [look]: mine };
}

/** Whether a look is as it starts. */
export const tuneIsDefault = (look: BandLookName) => !Object.keys(prefs.playerBandsTunes[look] ?? {}).length;

/** A look back to its defaults; returns the undo. */
export function resetTune(look: BandLookName): () => void {
  const was = prefs.playerBandsTunes[look];
  const { [look]: _, ...rest } = prefs.playerBandsTunes;
  prefs.playerBandsTunes = rest;
  return () => (prefs.playerBandsTunes = { ...prefs.playerBandsTunes, [look]: was });
}
