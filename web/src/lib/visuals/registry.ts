// Every module type a patch can hold beyond today's chain (decision 0056): the new sources (sources/, claude-632's), the
// effect engines as modules a layer can carry (a kaleidoscope on the shapes), the picture's own loop (loopmods.ts) and
// the modulators (modulators/, claude-632's). What the kept patch is checked against (patch.ts `extraFrom`).

import { ENGINE_EFFECTS, ENGINES } from './effects';
import { LOOP_TYPES } from './loopmods';
import { MODULATORS } from './modulators/modulators';
import type { Knob, ModuleType } from './patch';
import { SOURCE_TYPES } from './sources/index';

const knob = (id: string, label: string, min: number, max: number, def: number, kind: Knob['kind'], neutral: number): Knob => ({ id, label, min, max, neutral, default: def, kind });

/** An effect engine as a module (0050's three): which of its effects (a step, from the first), its Amount (a level:
 *  none at 0) and the effect's two controls. Added to a layer, it starts on its first effect at all of it. */
export const ENGINE_TYPES: ModuleType[] = ENGINES.map((e) => ({
  type: `${e.key}-engine`,
  label: e.label,
  place: 'module',
  knobs: [
    knob('effect', 'Effect', 1, ENGINE_EFFECTS[e.key].length, 1, 'step', 1),
    knob('amount', 'Amount', 0, 1, 1, 'level', 0),
    knob('k1', 'Control 1', 0, 1, 0.5, 'swing', 0.5),
    knob('k2', 'Control 2', 0, 1, 0.5, 'swing', 0.5),
  ],
}));

export const EXTRA_TYPES: ModuleType[] = [...SOURCE_TYPES, ...ENGINE_TYPES, ...LOOP_TYPES, ...Object.values(MODULATORS)];

const BY_TYPE = new Map(EXTRA_TYPES.map((t) => [t.type, t]));
export const extraType = (type: string): ModuleType | undefined => BY_TYPE.get(type);
