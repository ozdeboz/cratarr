// The picture's own loop (decision 0056): after the layers are blended, the last frame's picture carried along and
// kept, then the new one added: MilkDrop's warp and decay, as two modules. Each has an Amount whose neutral (0) does
// nothing, so the loop morphs in and out; every other knob swings about its default, where it does nothing either.
// Their drawing is the compositor's (compose.ts). Rates are a second's, so the picture moves alike at any frame rate.

import type { Knob, ModuleType } from './patch';

const knob = (id: string, label: string, min: number, max: number, def: number, kind: 'level' | 'swing' = 'swing'): Knob => ({
  id,
  label,
  min,
  max,
  neutral: kind === 'level' ? min : def,
  default: def,
  kind,
});

/** Warp: the last picture moved before it is kept. Zoom (in, out), turn, drift across, stretch, and a wobble that
 *  bends it as water does; their rates a second. */
export const WARP: ModuleType = {
  type: 'warp',
  label: 'Warp',
  place: 'loop',
  knobs: [
    knob('amount', 'Amount', 0, 1, 1, 'level'),
    knob('zoom', 'Zoom', -1, 1, 0),
    knob('turn', 'Turn', -1, 1, 0),
    knob('driftX', 'Drift across', -1, 1, 0),
    knob('driftY', 'Drift up', -1, 1, 0),
    knob('stretch', 'Stretch', -1, 1, 0),
    knob('wobble', 'Wobble', 0, 1, 0),
    knob('wobbleSize', 'Wobble size', 0, 1, 0.5),
  ],
};

/** Keep: how much of the last picture stays (Amount; 0 none, as without a loop) and how long its trails last. */
export const KEEP: ModuleType = {
  type: 'keep',
  label: 'Keep',
  place: 'loop',
  knobs: [knob('amount', 'Amount', 0, 1, 1, 'level'), knob('trails', 'Trails', 0, 1, 0.5)],
};

export const LOOP_TYPES: ModuleType[] = [WARP, KEEP];

/** Keep's Trails (0..1) as the share of the picture kept a second: from a tenth of a second's trail to six seconds'. */
export const keptPerSecond = (trails: number) => Math.exp(-1 / (0.1 * 60 ** Math.min(1, Math.max(0, trails))));
