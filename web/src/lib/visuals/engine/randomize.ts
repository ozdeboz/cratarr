// Randomize (decision 0057 §4d–4e; VCV Rack's): new values for a patch's knobs within their ranges, the structure as
// it is (no module, cable or macro added or taken away), what is pinned kept. A seed makes it repeat: a new track is
// Randomize with the track's seed (the same track, the same patch: the same values; Reset to seed is it again), the
// dice with a new one. It keeps nothing from one call to the next. A scope says which knobs it reaches (the dice's
// long press: everything, or a category's; one module's from its menu).
//
// As VCV has it: each knob, uniformly within its range (VCV's ParamQuantity::randomize), unless it says not to (its
// `randomize: false`, VCV's randomizeEnabled); and modules that draw their knobs together do so themselves (VCV's
// onRandomize). Here that is the Cratarr chain's draw, one over the flame, the effect slots, the loop effect and the
// facets (flame.ts drawing: the flame's form surveyed until it draws something), so a track's seed gives the values it
// gave before 0057. Each other module draws from its own stream of the seed, so one module more or less changes no
// other's draw.
//
// Pinned (0057 §4d), left as it is and drawn around: a knob (`<module>.<knob>`), a polyphonic knob's channel
// (`<module>.<knob>.<channel>`) or all of them (`<module>.<knob>`), a whole module or device (`module:<id>`), a whole
// category (`category:<name>`, rack.ts). A cable's amount (`cable:<id>`) is Randomize's Cables scope's.

import { drawing, PRESET_KEYS, randomFrom, seedOf, type Preset } from '../flame';
import { SETTING_KNOB, type Knob, type Module, type ModuleType } from '../patch';
import { isDevice, type PatchV2 } from '../ports';
import { categoryOf, deviceCategory, portsFrom } from '../rack';

/** Which knobs Randomize reaches: a module's knob (a polyphonic one's channels together). */
export type Scope = (module: Module, knob: Knob) => boolean;
export const EVERYTHING: Scope = () => true;

export type RandomizeOptions = {
  seed: number;
  scope?: Scope;
  /** How far from where each knob is (0: not at all; 1, the default: anywhere in its range). The chain's draw, drawn as
   *  one, is drawn whole. */
  reach?: number;
};

/** The new values: each module's knobs by id (a device's modules under `<device>/<module>`, as flatten names them). */
export type Randomized = Record<string, Record<string, number>>;

/** The Cratarr chain's draw: each key flame.ts's drawing gives (PRESET_KEYS), the knob it is in the chain. */
const CHAIN_DRAW = PRESET_KEYS.flatMap((key) => (SETTING_KNOB[key] ? [{ key, module: SETTING_KNOB[key][0], knob: SETTING_KNOB[key][1] }] : []));
const CHAIN_DRAWN = new Set(CHAIN_DRAW.map((l) => `${l.module}.${l.knob}`));

/** The knobs a track's seed reached before 0057 (the chain's draw): what the equivalence holds Randomize to. */
export const SEEDED_BEFORE: Scope = (m, k) => CHAIN_DRAWN.has(`${m.id}.${k.id}`);

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const finite = (v: unknown, or: number) => (typeof v === 'number' && Number.isFinite(v) ? v : or);
const anyKnob = (id: string): Knob => ({ id, label: id, min: 0, max: 1, neutral: 0, default: 0, kind: 'swing' });

/** Randomize `p` (a generator: the chain's draw yields between the flame's drafts), its modules' types by type. */
export function* randomize(p: PatchV2, types: Record<string, ModuleType>, opts: RandomizeOptions): Generator<void, Randomized> {
  const out: Randomized = {};
  const ports = portsFrom(types);
  // A module's category (rack.ts): its type's, a device's by what it exposes.
  const category = (m: Module) => {
    if (isDevice(m)) {
      const d = ports(m);
      return d ? deviceCategory(d) : null;
    }
    return types[m.type] ? categoryOf(types[m.type]) : null;
  };
  yield* into(out, p, (t) => types[t], category, opts, '', new Set(p.pins), false);
  return out;
}

function* into(
  out: Randomized,
  p: PatchV2,
  typeOf: (type: string) => ModuleType | undefined,
  category: (m: Module) => string | null,
  opts: RandomizeOptions,
  at: string,
  pins: ReadonlySet<string>,
  held: boolean,
): Generator<void, void> {
  const scope = opts.scope ?? EVERYTHING;
  const reach = clamp(finite(opts.reach, 1), 0, 1);
  // A device's pins are its own (its patch's), with the outer patch's on it and its category's.
  const pinned = new Set([...pins, ...p.pins]);
  const byId = new Map(p.modules.map((m) => [m.id, m]));
  const knobOf = (m: Module, id: string) => typeOf(m.type)?.knobs.find((k) => k.id === id.replace(/\.\d+$/, '')) ?? anyKnob(id);
  const whole = (m: Module) => held || pinned.has(`module:${m.id}`) || pinned.has(`category:${category(m)}`);
  const isPinned = (m: Module, id: string) => whole(m) || pinned.has(`${m.id}.${id}`) || pinned.has(`${m.id}.${id.replace(/\.\d+$/, '')}`);
  const set = (m: Module, id: string, v: number) => {
    if (isPinned(m, id) || !scope(m, knobOf(m, id))) return;
    (out[at + m.id] ??= {})[id] = v;
  };

  // The Cratarr chain's draw, as one: what is pinned of it kept, and drawn around (as before 0057).
  const chain = at ? [] : CHAIN_DRAW.filter((l) => byId.has(l.module));
  if (chain.length) {
    const fixed: Partial<Record<keyof Preset, number>> = {};
    for (const l of chain) {
      const m = byId.get(l.module)!;
      if (isPinned(m, l.knob)) fixed[l.key as keyof Preset] = finite(m.knobs[l.knob], knobOf(m, l.knob).default);
    }
    const drawn = yield* drawing(opts.seed, fixed as Partial<Preset>);
    for (const l of chain) set(byId.get(l.module)!, l.knob, drawn.preset[l.key as keyof Preset] as number);
  }
  const claimed = new Set(chain.map((l) => `${l.module}.${l.knob}`));

  for (const m of p.modules) {
    if (isDevice(m)) {
      yield* into(out, m.patch, typeOf, category, { ...opts, seed: seedOf(`${opts.seed}/${m.id}`) }, `${at}${m.id}/`, pinned, whole(m));
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
