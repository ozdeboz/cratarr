// The dice (0057 §4d, §7f; decision 0077, the owner: "keep the dice coherent too"): one roll, the face of where it is,
// from a seed. A new track's look is the track's seed, the dice's tap a fresh one, Reset to seed the track's again; the
// Dice module rolls the face of its level. Over frames: the Flame's survey yields.
// - The rack's face: its macros and its devices' (every level's, one pinned or mapped one level up kept), each within
//   the range its maker chose. Where they move the Flame's look, the look is surveyed: it must draw something (above ½,
//   as a track's look always was), the seed after tried until it does, else the best of them.
// - A device's own Randomize: its face (its macros); a module's: its knobs, its face.
// - The long press's eleven scopes (the owner: "Keep the 11 scopes"): knobs by kind, Everything, the cables, the
//   response, the macros; the scopes pinned (`scope:<name>`, the dice's column) kept by every roll.
// No check against the track (the owner: "No"): a macro's range and the Calm, live, keep a roll within bounds.

import type { Module, ModuleType } from '../patch';
import { drawScore, type Preset } from '../flame';
import { isDevice, type CableV2, type PatchV2, type RANDOMIZE_SCOPES } from '../ports';
import { applyMacros, asSet, flatten } from '../v2';
import { LOOK_KNOBS } from './flame-node';
import { randomize, randomizeCables, randomizeMacros, scopeNamed, type Randomized, type Scope } from './randomize';

/** What a roll reaches: a scope of RANDOMIZE_SCOPES (`macros`: the face), or one module by its path (its own Randomize:
 *  a device's macros, a module's knobs). */
export type Roll = (typeof RANDOMIZE_SCOPES)[number] | { module: string[] };
/** A roll's outcome: knob values by flattened module id, cables' values by id, macros' by their node. */
export type Rolled = { values: Randomized; cables: Record<string, CableV2['value']>; macros?: Record<string, number> };

/** How many seeds a face's roll tries for a look that draws, and the score that is enough (flame.ts drawing's). */
const TRIES = 24;
const DRAWS = 0.5;

/** The module at a path. */
function moduleAt(p: PatchV2, path: readonly string[]): Module | undefined {
  let modules = p.modules;
  let m: Module | undefined;
  for (const step of path) {
    m = modules.find((x) => x.id === step);
    if (!m) return undefined;
    modules = isDevice(m) ? m.patch.modules : [];
  }
  return m;
}

/** The look the Flame would draw with these macros: how well it draws, or null where no macro moves its look. */
function lookScore(patch: PatchV2, macros: Record<string, number>, types: Record<string, ModuleType>, seed: number): number | null {
  const candidate = structuredClone(patch);
  applyMacros(candidate, macros);
  const flat = flatten(candidate);
  const flame = flat.modules.find((m) => m.type === 'flame');
  if (!flame || !flat.macros.some((x) => x.targets.some((t) => t.module === flame.id && (LOOK_KNOBS as readonly string[]).includes(t.knob)))) return null;
  const knobs = types.flame?.knobs ?? [];
  const look = Object.fromEntries(LOOK_KNOBS.map((k) => [k, asSet(flat, flame, k, types) ?? knobs.find((x) => x.id === k)?.default ?? 0]));
  return drawScore(look as unknown as Preset, seed);
}

/** The macro owning each knob of a flattened patch (0077: one a knob), climbed to the outermost (a device's macro mapped
 *  one level up is its mapper's): rolling a knob a macro owns rolls its owner. */
function ownersOf(patch: PatchV2): (module: string, knob: string) => string | undefined {
  const flat = flatten(patch);
  return (module, knob) => {
    const base = knob.replace(/\.\d+$/, '');
    let owner = flat.macros.find((x) => x.targets.some((t) => t.module === module && (t.knob === knob || t.knob === base)));
    for (let depth = 0; owner && depth < 8; depth++) {
      const up = flat.macros.find((y) => y.targets.some((t) => t.module === owner!.id && t.knob === 'value'));
      if (!up) break;
      owner = up;
    }
    return owner?.id;
  };
}

/** A face's roll: its macros from `seed` (those in `only`, where given), the next seeds tried where the look would draw
 *  nothing. */
function* faceRoll(patch: PatchV2, types: Record<string, ModuleType>, seed: number, reach: number, at?: readonly string[], only?: ReadonlySet<string>): Generator<void, Record<string, number>> {
  let best: { macros: Record<string, number>; score: number } | null = null;
  for (let t = 0; t < TRIES; t++) {
    if (t > 0) yield;
    const all = randomizeMacros(patch, { seed: seed + t, reach, at });
    const macros = only ? Object.fromEntries(Object.entries(all).filter(([k]) => only.has(k))) : all;
    const score = lookScore(patch, macros, types, seed + t);
    if (score === null || score > DRAWS) return macros;
    if (!best || score > best.score) best = { macros, score };
  }
  return best!.macros;
}

/** A roll of `patch` over `roll` from `seed`, a fraction `reach` of each control's range from where it is (1: anywhere). */
export function* rollOf(patch: PatchV2, types: Record<string, ModuleType>, seed: number, roll: Roll, reach = 1): Generator<void, Rolled> {
  // The scopes pinned (`scope:<name>`, the dice's column): kept by every roll, whatever its scope.
  const held = patch.pins.filter((x) => x.startsWith('scope:')).map((x) => x.slice('scope:'.length));
  if (typeof roll !== 'string') {
    // A device's Randomize: its face; a module's: its knobs.
    const m = moduleAt(patch, roll.module);
    if (m && isDevice(m) && m.macros.length) return { values: {}, cables: {}, macros: yield* faceRoll(patch, types, seed, reach, roll.module) };
    const id = roll.module[roll.module.length - 1];
    const owners = new Set<string>();
    const values = yield* randomize(patch, types, { seed, scope: (x) => x.id === id, reach, owned: ownersOf(patch), owners });
    return { values, cables: {}, ...(owners.size ? { macros: yield* faceRoll(patch, types, seed, reach, undefined, owners) } : {}) };
  }
  if (roll === 'macros') return { values: {}, cables: {}, macros: held.includes('macros') ? {} : yield* faceRoll(patch, types, seed, reach) };
  const heldScopes = held.map((x) => scopeNamed(x, types)).filter((x): x is Scope => !!x);
  const asked = scopeNamed(roll, types);
  const knobsScope: Scope | null = asked && heldScopes.length ? (m, k) => asked(m, k) && !heldScopes.some((h) => h(m, k)) : asked;
  const withCables = (roll === 'everything' || roll === 'cables' || roll === 'response') && !held.includes('cables') && !held.includes('response');
  // A knob a macro owns is rolled through its owner; Everything rolls every macro besides.
  const owners = new Set<string>();
  const values: Randomized = knobsScope ? yield* randomize(patch, types, { seed, scope: knobsScope, reach, owned: ownersOf(patch), owners }) : {};
  const cables = withCables ? randomizeCables(patch, types, { seed, reach }) : {};
  const everything = roll === 'everything';
  if (held.includes('macros') || (!everything && !owners.size)) return { values, cables };
  return { values, cables, macros: yield* faceRoll(patch, types, seed, reach, undefined, everything ? undefined : owners) };
}
