// The shipped rack's face (decision 0077, the owner: "Eight"): the look's macros over the Flame's knobs, beside the
// rack's Effects (the eighth, made with the effects) and its behaviour (Intensity, Accent, Speed, Wild). Each maps the
// knobs over what a track's look was drawn within (flame.ts drawing), so the dice, rolling them, draw looks as a new
// track's always were; their value, at first, from where the Flame's knobs are.

import { DRAWN_FINALS, FORMS } from './flame';
import type { Macro, Module } from './patch';

type Face = { id: string; label: string; targets: readonly (readonly [knob: string, from: number, to: number])[] };

/** The look's macros: their names, the Flame's knobs each maps and over what (`look-`: apart from a module's id at the
 *  rack's level, which macros share). */
export const LOOK_FACE: readonly Face[] = [
  { id: 'look-form', label: 'Form', targets: [['form', 0, FORMS - 1]] },
  { id: 'look-shape', label: 'Shape', targets: [['detail', 0.15, 0.85], ['twist', 0.3, 1], ['warp', 0.55, 1]] },
  { id: 'look-symmetry', label: 'Symmetry', targets: [['symmetry', 1, 8], ['mirror', 0, 1]] },
  { id: 'look-lens', label: 'Lens', targets: [['final', 0, DRAWN_FINALS], ['lensSides', 3, 12], ['lensDepth', 0.2, 0.8]] },
  // The four families of hues a track's look picks; Cover and Fire are chosen by hand.
  { id: 'look-colours', label: 'Colours', targets: [['family', 0, 3]] },
  { id: 'look-hue', label: 'Hue', targets: [['hue', 0, 1]] },
  { id: 'look-light', label: 'Light', targets: [['lightness', 0.23, 0.77], ['vividness', 0.3, 0.7]] },
];

/** The look's macros over `flame` (its id, at the level the macros sit), the look as it is: each macro at the mean of
 *  its knobs' places in their ranges, and each range, its width as drawn, through where its knob is at that value (a
 *  knob where its range does not reach, its range moved to it), so nothing moves as they come and a roll ranges round
 *  the look as it was. */
export function lookMacros(flame: Module): Macro[] {
  return LOOK_FACE.map((f) => {
    const now = (knob: string, from: number) => flame.knobs[knob] ?? from;
    const at = f.targets.map(([knob, from, to]) => (to > from ? Math.min(1, Math.max(0, (now(knob, from) - from) / (to - from))) : 0));
    const value = at.reduce((a, b) => a + b, 0) / at.length;
    return {
      id: f.id,
      on: 'rack',
      label: f.label,
      value,
      targets: f.targets.map(([knob, from, to]) => {
        const lo = now(knob, from) - (to - from) * value;
        return { module: flame.id, knob, from: lo, to: lo + (to - from) };
      }),
    };
  });
}
