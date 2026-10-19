// Randomize (decision 0057 §4d–4e; VCV Rack's): new values for a patch's knobs within their ranges, the structure as
// it is (no module, cable or macro added or taken away), what is pinned kept. A seed makes it repeat: a track's look is
// Randomize with the track's seed (the same track, the same patch: the same look); the dice, with a new one. A scope
// says which knobs it reaches (the dice's long press: everything, the sources', the picture's, the control's, one
// module's).
//
// As VCV has it: each knob, uniformly within its range (VCV's ParamQuantity::randomize), unless it says not to (its
// `randomize: false`, VCV's randomizeEnabled); and a module that draws its knobs together does so itself (VCV's
// onRandomize). Here that is today's look, one draw over the flame, the effect slots, the loop effect and the facets
// (flame.ts drawing: the flame's form surveyed until it draws something), so a track's look keeps its values. Each
// other module draws from its own stream of the seed, so one module more or less changes no other's draw.

import { drawing, PRESET_KEYS, randomFrom, seedOf, type Preset } from '../flame';
import { SETTING_KNOB, type Knob, type Module, type ModuleType } from '../patch';
import { isDevice, type PatchV2 } from '../ports';

/** Which knobs Randomize reaches: a module's knob (a polyphonic one's channels together). */
export type Scope = (module: Module, knob: Knob) => boolean;
export const EVERYTHING: Scope = () => true;

export type RandomizeOptions = {
  seed: number;
  scope?: Scope;
  /** How far from where each knob is (0: not at all; 1, the default: anywhere in its range). Today's look, drawn as
   *  one, is drawn whole. */
  reach?: number;
};

/** The new values: each module's knobs by id (a device's modules under `<device>/<module>`, as flatten names them). */
export type Randomized = Record<string, Record<string, number>>;

/** Today's look: each of its keys (flame.ts PRESET_KEYS), the knob it is in the Cratarr chain. */
const LOOK = PRESET_KEYS.flatMap((key) => (SETTING_KNOB[key] ? [{ key, module: SETTING_KNOB[key][0], knob: SETTING_KNOB[key][1] }] : []));
const LOOK_KNOBS = new Set(LOOK.map((l) => `${l.module}.${l.knob}`));

/** The knobs today's look reaches (a track's look before 0057): what the equivalence holds Randomize to. */
export const TODAYS_LOOK: Scope = (m, k) => LOOK_KNOBS.has(`${m.id}.${k.id}`);

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const finite = (v: unknown, or: number) => (typeof v === 'number' && Number.isFinite(v) ? v : or);
const anyKnob = (id: string): Knob => ({ id, label: id, min: 0, max: 1, neutral: 0, default: 0, kind: 'swing' });

/** Randomize `p` (a generator: today's look yields between the flame's drafts). `typeOf`: a module's type. */
export function* randomize(p: PatchV2, typeOf: (type: string) => ModuleType | undefined, opts: RandomizeOptions): Generator<void, Randomized> {
  const out: Randomized = {};
  yield* into(out, p, typeOf, opts, '');
  return out;
}

function* into(out: Randomized, p: PatchV2, typeOf: (type: string) => ModuleType | undefined, opts: RandomizeOptions, at: string): Generator<void, void> {
  const scope = opts.scope ?? EVERYTHING;
  const reach = clamp(finite(opts.reach, 1), 0, 1);
  const pinned = new Set(p.pins);
  const byId = new Map(p.modules.map((m) => [m.id, m]));
  const knobOf = (m: Module, id: string) => typeOf(m.type)?.knobs.find((k) => k.id === id.replace(/\.\d+$/, '')) ?? anyKnob(id);
  const set = (m: Module, id: string, v: number) => {
    if (pinned.has(`${m.id}.${id}`) || !scope(m, knobOf(m, id))) return;
    (out[at + m.id] ??= {})[id] = v;
  };

  // Today's look, one draw: what is pinned of it kept (and drawn around, as Another look did).
  const look = at ? [] : LOOK.filter((l) => byId.has(l.module));
  if (look.length) {
    const fixed: Partial<Record<keyof Preset, number>> = {};
    for (const l of look) {
      const m = byId.get(l.module)!;
      if (pinned.has(`${l.module}.${l.knob}`)) fixed[l.key as keyof Preset] = finite(m.knobs[l.knob], knobOf(m, l.knob).default);
    }
    const drawn = yield* drawing(opts.seed, fixed as Partial<Preset>);
    for (const l of look) set(byId.get(l.module)!, l.knob, drawn.preset[l.key as keyof Preset] as number);
  }
  const claimed = new Set(look.map((l) => `${l.module}.${l.knob}`));

  for (const m of p.modules) {
    if (isDevice(m)) {
      yield* into(out, m.patch, typeOf, { ...opts, seed: seedOf(`${opts.seed}/${m.id}`) }, `${at}${m.id}/`);
      continue;
    }
    const t = typeOf(m.type);
    if (!t) continue;
    const rnd = randomFrom(seedOf(`${opts.seed}/${m.id}`));
    for (const knob of t.knobs) {
      const ids = knob.channels ? Array.from({ length: knob.channels }, (_, c) => `${knob.id}.${c}`) : [knob.id];
      for (const id of ids) {
        // Drawn whether or not it is set, so what one knob is reached by changes no other's draw.
        const u = rnd();
        if (knob.randomize === false || claimed.has(`${m.id}.${id}`)) continue;
        const now = finite(m.knobs[id], knob.default);
        const target = knob.kind === 'step' ? Math.min(knob.max, Math.floor(knob.min + u * (knob.max - knob.min + 1))) : knob.min + u * (knob.max - knob.min);
        const v = reach >= 1 ? target : now + (target - now) * reach;
        set(m, id, clamp(knob.kind === 'step' ? Math.round(v) : v, knob.min, knob.max));
      }
    }
  }
}
