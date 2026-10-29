// Randomize (decision 0057 §4d–4e; VCV Rack's): new values for a patch's knobs within their ranges, the structure as
// it is (no module, cable or macro added or taken away), what is pinned kept. A seed makes it repeat: a new track is
// Randomize with the track's seed (the same track, the same patch: the same values; Reset to seed is it again), the
// dice with a new one. It keeps nothing from one call to the next. A scope says which knobs it reaches (the dice's
// long press: everything, or a category's; one module's from its menu).
//
// As VCV has it: each knob, uniformly within its range (VCV's ParamQuantity::randomize), unless it says not to (its
// `randomize: false`, VCV's randomizeEnabled); and modules that draw their knobs together do so themselves (VCV's
// onRandomize). Here that is the Cratarr device's draw, one over the flame, the effects, the loop effect and the
// facets (flame.ts drawing: the flame's form surveyed until it draws something), so a track's seed gives the values it
// gave before 0057. The effects are modules (0057 §3), and Randomize keeps the rack as it is (the owner): each effect
// module keeps its effect, the draw drawn around it, its Amount and controls drawn; a slot with no module, nothing. Each other module draws from its own stream of the seed, so one module more or less changes no
// other's draw.
//
// Pinned (0057 §4d), left as it is and drawn around: a knob (`<module>.<knob>`), a polyphonic knob's channel
// (`<module>.<knob>.<channel>`) or all of them (`<module>.<knob>`), a whole module or device (`module:<id>`), a whole
// category (`category:<name>`, rack.ts). A cable's amount (`cable:<id>`) is Randomize's Cables scope's.

import { drawing, PRESET_KEYS, randomFrom, seedOf, type Preset } from '../flame';
import { SETTING_KNOB, type Knob, type Module, type ModuleType } from '../patch';
import { isDevice, type CableV2, type PatchV2 } from '../ports';
import type { Curve, RouteValue } from '../curves';
import { categoryOf, deviceCategory, portsFrom } from '../rack';
import { followsOf } from './follow';
import { ownKnob, placeOf, slotKnobOf, slotModules, slotOf, SLOT_IDS, SLOT_SETTINGS, type Slot } from './effect-slots';

/** Which knobs Randomize reaches: a module's knob (a polyphonic one's channels together). */
export type Scope = (module: Module, knob: Knob) => boolean;
export const EVERYTHING: Scope = () => true;

export type RandomizeOptions = {
  seed: number;
  scope?: Scope;
  /** How far from where each knob is (0: not at all; 1, the default: anywhere in its range). The chain's draw, drawn as
   *  one, is drawn whole. */
  reach?: number;
  /** The macro that owns a knob, by the knob's module's flattened id (0077: one a knob, its owner rolls it), and where
   *  each owner of a knob in scope is collected: an owned knob is never rolled itself. */
  owned?: (module: string, knob: string) => string | undefined;
  owners?: Set<string>;
};

/** The new values: each module's knobs by id (a device's modules under `<device>/<module>`, as flatten names them). */
export type Randomized = Record<string, Record<string, number>>;

/** The slots' settings, each its slot and which of it. */
const SLOT_KEY = new Map(SLOT_IDS.flatMap((slot) => (Object.entries(SLOT_SETTINGS[slot]) as [keyof (typeof SLOT_SETTINGS)[Slot], string][]).map(([which, key]) => [key, { slot, which }] as const)));
/** The Cratarr device's draw: each key flame.ts's drawing gives (PRESET_KEYS), the knob it is in the chain (the slots'
 *  apart: their modules are the effects'). */
const CHAIN_DRAW = PRESET_KEYS.flatMap((key) => (SETTING_KNOB[key] && !SLOT_KEY.has(key) ? [{ key, module: SETTING_KNOB[key][0], knob: SETTING_KNOB[key][1] }] : []));
const CHAIN_DRAWN = new Set(CHAIN_DRAW.map((l) => `${l.module}.${l.knob}`));
const SLOT_DRAWN = PRESET_KEYS.filter((key) => SLOT_KEY.has(key));

/** The modules a Spatial roll reaches whole, and the Flame's knobs it reaches. */
const SPATIAL = new Set(['grid', 'field', 'fold', 'instance', 'transform', 'push']);
const FLAME_SPATIAL = new Set(['zoom', 'rotation', 'symmetry', 'mirror', 'fit']);

/** The knobs a Lightness roll reaches, by their ids or labels. */
const LIGHT = /lightness|bright|exposure|glow|gamma/i;

/** The knobs a Colours roll reaches, by their ids or labels. */
const COLOUR = /hue|sat|tint|colou?r|palette/i;

/** The modules a Response roll reaches: how much and how fast the music moves things. */
const RESPONSE: ReadonlySet<string> = new Set(['envelope', 'adsr', 'vca', 'shaper', 'clock', 'accent']);

/** A Dice's scope (ports.ts RANDOMIZE_SCOPES) as Randomize's: everything, or a category's modules (rack.ts); the
 *  cables' amounts are another roll (null here). */
export function scopeNamed(name: string, types: Record<string, ModuleType>): Scope | null {
  if (name === 'everything') return EVERYTHING;
  if (name === 'cables' || name === 'macros') return null;
  // Response: how the rack answers the music, not how it looks: what lies between the music and the picture (the
  // followers, ADSRs, Shapers, the Clock and the Accent; the cables' amounts with them, visuals-loop rollOf).
  if (name === 'response') return (m) => RESPONSE.has(m.type);
  // Spatial: where things are and how space is laid out (the owner: so a rack without a Grid still rolls): the Grid, the
  // Field, the folds and copies, the Streamlines' shape (not their colours), the Flame's framing and symmetry.
  if (name === 'spatial') return (m, k) => SPATIAL.has(m.type) || (m.type === 'streamlines' && !COLOUR.test(k.id)) || (m.type === 'flame' && FLAME_SPATIAL.has(k.id));
  // Colours: every knob that sets a colour (a hue, its spread, a saturation, a tint, the Flame's palette), nothing else.
  // Lightness: every knob that sets how light the picture is (a lightness, a brightness, the exposure, a glow, a gamma).
  if (name === 'lightness') return (_m, k) => LIGHT.test(k.id) || LIGHT.test(k.label ?? '');
  if (name === 'colours') return (_m, k) => COLOUR.test(k.id) || COLOUR.test(k.label ?? '');
  const ports = portsFrom(types);
  return (m) => {
    if (isDevice(m)) {
      const d = ports(m);
      return !!d && deviceCategory(d) === name;
    }
    return !!types[m.type] && categoryOf(types[m.type]) === name;
  };
}

/** The knobs a track's seed reached before 0057 (the chain's draw): what the equivalence holds Randomize to. */
export const SEEDED_BEFORE: Scope = (m, k) => CHAIN_DRAWN.has(`${m.id}.${k.id}`) || (!!slotOf(m.id, m.type) && !!slotKnobOf(m.type, k.id));

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
    const owner = opts.owned?.(at + m.id, id);
    if (owner) return void opts.owners?.add(owner);
    (out[at + m.id] ??= {})[id] = v;
  };

  // The Cratarr device's draw, as one: what is pinned of it kept, and drawn around (as before 0057).
  const chain = at ? [] : CHAIN_DRAW.filter((l) => byId.has(l.module));
  const effects = chain.length ? slotModules(p.modules) : new Map<Slot, Module>();
  const claimed = new Set(chain.map((l) => `${l.module}.${l.knob}`));
  for (const m of effects.values()) for (const k of ['amount', ownKnob(m.type, 'k1'), ownKnob(m.type, 'k2')]) if (k) claimed.add(`${m.id}.${k}`);
  if (chain.length) {
    const fixed: Partial<Record<keyof Preset, number>> = {};
    for (const l of chain) {
      const m = byId.get(l.module)!;
      if (isPinned(m, l.knob)) fixed[l.key as keyof Preset] = finite(m.knobs[l.knob], knobOf(m, l.knob).default);
    }
    // Each effect module keeps its effect (the rack as it is); a knob of it pinned, that knob.
    for (const key of SLOT_DRAWN) {
      const { slot, which } = SLOT_KEY.get(key)!;
      const m = effects.get(slot);
      if (!m) continue;
      const knob = which === 'effect' ? null : which === 'amount' ? 'amount' : ownKnob(m.type, which);
      if (which === 'effect') fixed[key as keyof Preset] = placeOf(slot, m.type);
      else if (knob && isPinned(m, knob)) fixed[key as keyof Preset] = finite(m.knobs[knob], knobOf(m, knob).default);
    }
    const drawn = yield* drawing(opts.seed, fixed as Partial<Preset>);
    for (const l of chain) set(byId.get(l.module)!, l.knob, drawn.preset[l.key as keyof Preset] as number);
    // Each effect module's Amount and controls, as the draw has its slot's.
    for (const [slot, m] of effects) {
      const names = SLOT_SETTINGS[slot];
      set(m, 'amount', finite(drawn.preset[names.amount as keyof Preset], 0));
      for (const k of ['k1', 'k2'] as const) {
        const own = ownKnob(m.type, k);
        if (own) set(m, own, finite(drawn.preset[names[k] as keyof Preset], 0.5));
      }
    }
  }

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
        if (knob.randomize === false || claimed.has(`${m.id}.${id}`) || KEPT_KNOB(m.type, id)) continue;
        const now = finite(m.knobs[id], knob.default);
        const target = knob.kind === 'step' ? Math.min(knob.max, Math.floor(knob.min + u * (knob.max - knob.min + 1))) : knob.min + u * (knob.max - knob.min);
        const v = reach >= 1 ? target : now + (target - now) * reach;
        set(m, id, clamp(knob.kind === 'step' ? Math.round(v) : v, knob.min, knob.max));
      }
    }
  }
}

/** What a roll keeps as it is (the owner: no more dark rolls): the plumbing's knobs (a VCA's Gain at 0 shuts its
 *  signal off, a Gate's or Ducker's threshold holds everything back) and a Mix's blend modes (Multiply, Window and
 *  Subtract over a dark layer blacken the picture). Its amounts still roll. */
const KEPT_TYPES = new Set(['vca', 'merge', 'gate', 'ducker', 'meter', 'screen', 'dice']);
const KEPT_KNOB = (type: string, id: string) => KEPT_TYPES.has(type) || (type === 'mix' && /^mode\d+$/.test(id));

/** The modules whose cables are a patch's plumbing (what a conversion wires: VCAs, Merges, Gates, Shapers, the
 *  sidechains), not amounts to roll. */
const PLUMBING = new Set(['vca', 'merge', 'gate', 'shaper', 'ducker', 'meter', 'screen', 'dice']);

/** The Cables roll (0057 §4d, Randomize's Cables scope): new amounts for a patch's existing cables, none added or taken
 *  away, what is pinned (`cable:<id>`) kept. It reaches a cable that runs into a knob (not a module's input port) from
 *  anything but the plumbing: the amounts a patch is played with. Each amount drawn uniformly over −1..1 from the
 *  cable's own stream of the seed (one cable more or less changes no other's), as far as `reach` takes it from where it
 *  is; a curve keeps its shape, scaled to the new height. */
export function randomizeCables(p: PatchV2, types: Record<string, ModuleType>, opts: { seed: number; reach?: number }): Record<string, RouteValue> {
  const reach = clamp(finite(opts.reach, 1), 0, 1);
  const pinned = new Set(p.pins);
  const typeOf = new Map(p.modules.map((m) => [m.id, m.type]));
  const out: Record<string, RouteValue> = {};
  // A cable through the Loudness Follow is the cable from the signal behind it (its curve, a curved one's, on the
  // cable in: that one rolled).
  const follows = followsOf(p);
  for (const into of p.cables) {
    const via = follows.get(into.from.module);
    const c = via && typeof into.value === 'number' && into.value === 1 && typeof via.value !== 'number' ? via : into;
    const from = typeOf.get((via ?? into).from.module) ?? (via ?? into).from.module;
    const to = typeOf.get(into.to.module);
    if (pinned.has(`cable:${into.id}`) || PLUMBING.has(from) || !to || PLUMBING.has(to)) continue;
    if (!types[to]?.knobs.some((k) => k.id === into.to.port.replace(/\.\d+$/, ''))) continue;
    const now = typeof c.value === 'number' ? c.value : c.value.h;
    const drawn = randomFrom(seedOf(`${opts.seed}/cable/${c.id}`))() * 2 - 1;
    const amount = now + (drawn - now) * reach;
    out[c.id] = typeof c.value === 'number' ? amount : scaled(c.value, now, amount);
  }
  return out;
}

/** A curve at another height, its shape kept (its handles' heights in proportion). */
function scaled(c: Curve, from: number, to: number): Curve {
  const k = from !== 0 ? to / from : 0;
  return from !== 0 ? { ...c, h1: [c.h1[0], c.h1[1] * k], h2: [c.h2[0], c.h2[1] * k], h: to } : { h1: [1 / 3, to / 3], h2: [2 / 3, (2 * to) / 3], h: to, ...(c.walks ? { walks: true } : {}) };
}

/** The Macros roll (0077: the dice play the face, within the ranges its maker chose): every macro of every level a new
 *  value, drawn over 0..1 from its own stream of the seed, as far as `reach` takes it from where it is; one pinned at its
 *  level (`macro:<id>`) kept, and one a macro one level up maps (its owner rolls). Keyed by the macro's node as the
 *  engine runs it (the rack's by its id, a device's `<device>/macro:<id>`). `at`: a device's path, its face alone (its
 *  own Randomize). */
export function randomizeMacros(p: PatchV2, opts: { seed: number; reach?: number; at?: readonly string[] }): Record<string, number> {
  const reach = clamp(finite(opts.reach, 1), 0, 1);
  const out: Record<string, number> = {};
  const roll = (key: string, value: number) => {
    const now = clamp(finite(value, 0.5), 0, 1);
    const drawn = randomFrom(seedOf(`${opts.seed}/macro/${key}`))();
    out[key] = now + (drawn - now) * reach;
  };
  const level = (macros: readonly { id: string; value: number; targets: readonly { module: string; knob: string }[] }[], pins: readonly string[], modules: readonly Module[], path: string[], owned: ReadonlySet<string>) => {
    for (const x of macros) if (!pins.includes(`macro:${x.id}`) && !owned.has(x.id)) roll(path.length ? `${path.join('/')}/macro:${x.id}` : x.id, x.value);
    for (const m of modules) {
      if (!isDevice(m)) continue;
      const mapped = new Set(macros.flatMap((x) => x.targets.filter((t) => t.module === m.id && t.knob.startsWith('macro:')).map((t) => t.knob.slice('macro:'.length))));
      level(m.macros, m.patch.pins, m.patch.modules, [...path, m.id], mapped);
    }
  };
  if (!opts.at?.length) {
    level(p.macros, p.pins, p.modules, [], new Set());
    return out;
  }
  // A device's own face: from where it sits.
  let modules = p.modules;
  let found: Module | undefined;
  for (const step of opts.at) {
    found = modules.find((m) => m.id === step);
    if (!found || !isDevice(found)) return out;
    modules = found.patch.modules;
  }
  if (found && isDevice(found)) level(found.macros, found.patch.pins, found.patch.modules, [...opts.at], new Set());
  return out;
}
