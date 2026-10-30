// A patch seen as today's settings (decision 0057, the switch): what the renderer's side and the meters still read
// (`how`: the effects, the dive, the show, the look) while they read settings, each from the knob toV2 put it in. The
// inverse of toV2's mapping (v2.ts): the Cratarr device's modules found beside its Flame (at the top level, or inside it),
// the Show's parts inside its device, the Loudness's, Accent's and Clock's knobs for Intensity, Flash and Speed (as the
// rack's macros set them; Wild the rack's macro's own value), Music's hold for each band's Linger, the effects' slots
// from the effect modules (0057 §3). A column's scale is in its cables' amounts now (0057 §7e): those settings read 1.
// What a patch has no knob for, as `base` has it (the device's own settings: quality, frame rate, brightness).

import type { Module } from '../patch';
import { SETTING_KNOB } from '../patch';
import { ownKnob, placeOf, slotModules, SLOT_IDS, SLOT_SETTINGS } from './effect-slots';
import { SHOW_PARTS, type PatchV2 } from '../ports';
import type { EffectSettings } from '../effects';
import { COLUMN_SCALES } from '../routes';
import { BANDS, type VisualSettings } from '../settings.svelte';
import { asSet } from '../v2';

/** Settings toV2 moved onto the engine's own modules (v2.ts MOVED): their module's type and knob. */
const MOVED: Readonly<Record<string, [string, string]>> = { intensity: ['envelope', 'intensity'], flash: ['accent', 'accent'], speed: ['clock', 'speed'] };
/** The column scales folded into the cables (all but the fluid's stirring, scaled where it stirs). */
const FOLDED = new Set<string>(Object.entries(COLUMN_SCALES).filter(([a]) => a !== 'stir' && a !== 'flow' && a !== 'swirl').map(([, k]) => k));
const SHOW_PART = new Map(SHOW_PARTS.map((p) => [p.knob, p.type]));

/** The effects' slots as the light loop's and the meters' old side still read them (EffectSettings): each slot's effect,
 *  Amount and controls from the effect module standing for it (0057 §3, its macros in); a slot with none, none of it. */
export function effectSlotsOf(p: PatchV2): EffectSettings {
  const out: Record<string, number> = {};
  // As set: its macros' through the one rule (v2.ts asSet, macro-map.ts).
  const read = (m: Module, knob: string) => asSet(p, m, knob);
  const effects = slotModules(p.modules);
  for (const slot of SLOT_IDS) {
    const names = SLOT_SETTINGS[slot];
    const m = effects.get(slot);
    out[names.effect] = m ? placeOf(slot, m.type) : 0;
    out[names.amount] = m ? (read(m, 'amount') ?? 1) : 0;
    for (const k of ['k1', 'k2'] as const) {
      const own = m && ownKnob(m.type, k);
      out[names[k]] = (m && own ? read(m, own) : undefined) ?? 0.5;
    }
  }
  return out as unknown as EffectSettings;
}

/** `p` as today's settings, over `base`. */
export function settingsOfPatch(p: PatchV2, base: VisualSettings): VisualSettings {
  const byId = new Map(p.modules.map((m) => [m.id, m]));
  // The Loudness: the Envelope Follower on Music's loudness (its input uncabled), as the engine finds it.
  const ofType = (type: string) => p.modules.find((m) => m.type === type && (type !== 'envelope' || !p.cables.some((c) => c.to.module === m.id && c.to.port === 'in')));
  const flame = ofType('flame');
  const at = flame ? flame.id.slice(0, flame.id.lastIndexOf('/') + 1) : '';
  const inChain = (rel: string) => byId.get(at + rel);
  const out = { ...base } as unknown as Record<string, unknown>;
  const read = (m: Module | undefined, knob: string) => asSet(p, m, knob);
  for (const [setting, [module, knob]] of Object.entries(SETTING_KNOB)) {
    if (FOLDED.has(setting)) {
      out[setting] = 1;
      continue;
    }
    const moved = MOVED[setting];
    const v = moved ? read(ofType(moved[0]), moved[1]) : module === 'show' ? read(inChain(`show/${SHOW_PART.get(knob) ?? knob}`), knob) : read(inChain(module), knob);
    if (v !== undefined) out[setting] = v;
  }
  // The effects as modules (0057 §3): each slot's effect, Amount and controls from the module standing for it; a slot
  // with none, none of it.
  const effects = slotModules(p.modules);
  for (const slot of SLOT_IDS) {
    const names = SLOT_SETTINGS[slot];
    const m = effects.get(slot);
    if (!m) {
      out[names.amount] = 0;
      continue;
    }
    out[names.effect] = placeOf(slot, m.type);
    out[names.amount] = read(m, 'amount') ?? 1;
    for (const k of ['k1', 'k2'] as const) {
      const own = ownKnob(m.type, k);
      out[names[k]] = (own && read(m, own)) ?? 0.5;
    }
  }
  // Effects is the rack's macro on the effects' Amounts now (the Whole frame gone): folded into them, it reads 1.
  if (!ofType('frame')) out.effects = 1;
  const wild = p.macros.find((x) => x.id === 'wild');
  if (wild && Number.isFinite(wild.value)) out.wild = wild.value;
  const music = ofType('music');
  out.lighting = (base.lighting ?? []).map((v, b) => read(music, `hold.${b}`) ?? v);
  for (const k of ['continuity', 'evolution'] as const) out[k] = Array.from({ length: BANDS }, (_, b) => read(flame, `${k}.${b}`) ?? base[k][b]);
  return out as unknown as VisualSettings;
}
