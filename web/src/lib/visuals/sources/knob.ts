// The sources' knobs (decision 0056): continuous, each with a range, a neutral and a default. A level's neutral is its
// least (none of it: nothing drawn); a swing's is its default.
import type { Knob, ModuleType } from '../patch';

export const knob = (id: string, label: string, min: number, max: number, def: number, kind: 'level' | 'swing' = 'swing'): Knob => ({
  id,
  label,
  min,
  max,
  neutral: kind === 'level' ? min : def,
  default: def,
  kind,
});

/** Every knob of a type as the cables left it, kept in its range; one missing or not a number at its default. */
export function values(type: ModuleType, knobs: Record<string, number>): Record<string, number> {
  const out: Record<string, number> = {};
  for (const k of type.knobs) {
    const v = knobs[k.id];
    out[k.id] = typeof v === 'number' && Number.isFinite(v) ? Math.min(k.max, Math.max(k.min, v)) : k.default;
  }
  return out;
}

export const defaults = (type: ModuleType): Record<string, number> => Object.fromEntries(type.knobs.map((k) => [k.id, k.default]));
export const neutrals = (type: ModuleType): Record<string, number> => Object.fromEntries(type.knobs.map((k) => [k.id, k.neutral]));
