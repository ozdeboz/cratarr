// A patch made by toV2, kept from before (on this device, in a snapshot) or sent by the phone, made current as it loads:
// each change of the model since. Those that hold however often they run run every time (decision 0077: macros one
// level down; the shipped devices named by what they carry); those that would undo a later choice run once, by the
// patch's revision (`rev`).
import { namedDevices } from './device-names';
import { macrosOneLevel } from './macro-levels';
import type { ModuleType } from './patch';
import { RANDOMIZE_SCOPES, type PatchV2 } from './ports';

/** The revision a patch is brought to. 1: the shipped Dice rolls the face (0077). */
export const REV = 1;

/** A new track's Dice (Music's New track into its trigger) on Everything, as toV2 made it before 0077: on the face. */
function diceOnFace(p: PatchV2) {
  const typeOf = new Map(p.modules.map((m) => [m.id, m.type]));
  const triggered = new Set(p.cables.filter((c) => typeOf.get(c.from.module) === 'music' && c.from.port === 'newtrack' && c.to.port === 'trigger').map((c) => c.to.module));
  for (const m of p.modules) if (m.type === 'dice' && triggered.has(m.id) && Math.round(m.knobs.scope ?? 0) === RANDOMIZE_SCOPES.indexOf('everything')) m.knobs.scope = RANDOMIZE_SCOPES.indexOf('macros');
}

/** `p` made current (in place), and `p`. */
export function upgraded(p: PatchV2, types: Record<string, ModuleType>): PatchV2 {
  macrosOneLevel(p);
  namedDevices(p, types);
  if ((p.rev ?? 0) < 1) diceOnFace(p);
  p.rev = REV;
  return p;
}
