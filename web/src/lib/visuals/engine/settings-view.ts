// A patch seen as today's settings (decision 0057, the switch): what the renderer's side and the meters still read
// (`how`: the effects, the dive, the show, the look) while they read settings, each from the knob toV2 put it in. The
// inverse of toV2's mapping (v2.ts): the Cratarr chain found beside its Flame (top level, or inside the Cratarr device),
// the Show's parts inside its device, the Envelope's, Accent's and Clock's knobs for Intensity, Flash and Speed, Music's
// hold for each band's Linger. A column's scale is in its cables' amounts now (0057 §7e): those settings read 1. What a
// patch has no knob for, as `base` has it (the device's own settings: quality, frame rate, brightness).

import type { Module } from '../patch';
import { SETTING_KNOB } from '../patch';
import { SHOW_PARTS, type PatchV2 } from '../ports';
import { COLUMN_SCALES } from '../routes';
import { BANDS, type VisualSettings } from '../settings.svelte';

/** Settings toV2 moved onto the engine's own modules (v2.ts MOVED): their module's type and knob. */
const MOVED: Readonly<Record<string, [string, string]>> = { intensity: ['envelope', 'intensity'], flash: ['accent', 'accent'], speed: ['clock', 'speed'] };
/** The column scales folded into the cables (all but the fluid's stirring, scaled where it stirs). */
const FOLDED = new Set<string>(Object.entries(COLUMN_SCALES).filter(([a]) => a !== 'stir' && a !== 'flow' && a !== 'swirl').map(([, k]) => k));
const SHOW_PART = new Map(SHOW_PARTS.map((p) => [p.knob, p.type]));

/** `p` as today's settings, over `base`. */
export function settingsOfPatch(p: PatchV2, base: VisualSettings): VisualSettings {
  const byId = new Map(p.modules.map((m) => [m.id, m]));
  const ofType = (type: string) => p.modules.find((m) => m.type === type);
  const flame = ofType('flame');
  const at = flame ? flame.id.slice(0, flame.id.lastIndexOf('/') + 1) : '';
  const inChain = (rel: string) => byId.get(at + rel);
  const out = { ...base } as unknown as Record<string, unknown>;
  const read = (m: Module | undefined, knob: string) => {
    const v = m?.knobs[knob];
    return typeof v === 'number' && Number.isFinite(v) ? v : undefined;
  };
  for (const [setting, [module, knob]] of Object.entries(SETTING_KNOB)) {
    if (FOLDED.has(setting)) {
      out[setting] = 1;
      continue;
    }
    const moved = MOVED[setting];
    const v = moved ? read(ofType(moved[0]), moved[1]) : module === 'show' ? read(inChain(`show/${SHOW_PART.get(knob) ?? knob}`), knob) : read(inChain(module), knob);
    if (v !== undefined) out[setting] = v;
  }
  const music = ofType('music');
  out.lighting = (base.lighting ?? []).map((v, b) => read(music, `hold.${b}`) ?? v);
  for (const k of ['continuity', 'evolution'] as const) out[k] = Array.from({ length: BANDS }, (_, b) => read(flame, `${k}.${b}`) ?? base[k][b]);
  return out as unknown as VisualSettings;
}
