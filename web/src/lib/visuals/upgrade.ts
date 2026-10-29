// A patch made by toV2, kept from before (on this device, in a snapshot) or sent by the phone, made current as it loads:
// each change of the model since. Those that hold however often they run run every time (decision 0077: macros one
// level down; the shipped devices named by what they carry); those that would undo a later choice run once, by the
// patch's revision (`rev`).
import { namedDevices } from './device-names';
import { macrosOneLevel } from './macro-levels';
import type { ModuleType } from './patch';
import { RANDOMIZE_SCOPES, type PatchV2 } from './ports';
import { LOOK_FACE, lookMacros } from './rack-face';

/** The revision a patch is brought to. 1: the shipped Dice rolls the face (0077); 2: the rack has the look's macros
 *  (rack-face.ts), at where its Flame's knobs are. */
export const REV = 2;

/** A new track's Dice (Music's New track into its trigger) on Everything, as toV2 made it before 0077: on the face. */
function diceOnFace(p: PatchV2) {
  const typeOf = new Map(p.modules.map((m) => [m.id, m.type]));
  const triggered = new Set(p.cables.filter((c) => typeOf.get(c.from.module) === 'music' && c.from.port === 'newtrack' && c.to.port === 'trigger').map((c) => c.to.module));
  for (const m of p.modules) if (m.type === 'dice' && triggered.has(m.id) && Math.round(m.knobs.scope ?? 0) === RANDOMIZE_SCOPES.indexOf('everything')) m.knobs.scope = RANDOMIZE_SCOPES.indexOf('macros');
}

/** A rack with a Flame at its level and none of the look's macros: them, first on its face. */
function lookOnFace(p: PatchV2) {
  const flame = p.modules.find((m) => m.type === 'flame');
  if (!flame || p.macros.some((x) => LOOK_FACE.some((f) => f.id === x.id))) return;
  p.macros = [...lookMacros(flame), ...p.macros];
}

/** `p` made current (in place), and `p`. */
export function upgraded(p: PatchV2, types: Record<string, ModuleType>): PatchV2 {
  macrosOneLevel(p);
  namedDevices(p, types);
  if ((p.rev ?? 0) < 1) diceOnFace(p);
  if ((p.rev ?? 0) < 2) lookOnFace(p);
  p.rev = REV;
  return p;
}
