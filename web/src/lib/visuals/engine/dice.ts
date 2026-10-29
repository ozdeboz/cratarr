// The dice (0057 §4d, §7f): the rack's knobs rolled over a scope, as a new track's Dice rolls them (a performance) or
// the panel's dice (an edit, checked against the playing track): what the frame loop worked out itself until the
// owner's "engine purity" (2026-10-29). Over frames: the flame's survey and the checks yield.
// - What rolls: the scope asked (or the modules asked: a module's own Randomize), less the scopes pinned (`scope:<name>`,
//   the dice's column), kept by every roll; the cables' curves where the scope takes them; the macros alone for the
//   Macros scope (the rack's own ranges, so no check).
// - The check (the panel's dice): this seed and the next ones against the track, the first within the limits taken,
//   else the least over them. The limits are the ones the rack's Calm holds the picture to: each kind's Ducker's
//   threshold as set (Wild, the macro on them, in), not a macro found by its id. A rack without one: the standing ones.

import type { Beats } from '../beats';
import { BUDGETED, type Budgeted } from '../budget';
import type { ModuleType } from '../patch';
import type { CableV2, PatchV2, RANDOMIZE_SCOPES } from '../ports';
import { precheckingPatch, type Checked } from '../precheck';
import type { VisualSettings } from '../settings.svelte';
import type { Signals } from '../signals';
import { applyKnobs, flatten, METER_BUDGET, THRESHOLD_OVER_LIMIT } from '../v2';
import { randomize, randomizeCables, randomizeMacros, scopeNamed, type Randomized, type Scope } from './randomize';
import { STANDING } from './sidechain';

/** What a roll reaches: a scope of RANDOMIZE_SCOPES, or modules by id (a module's own Randomize). */
export type Roll = (typeof RANDOMIZE_SCOPES)[number] | { module: string[] };
/** A roll's outcome: knob values by flattened module id, cables' values by id, the macros' (the Macros scope). */
export type Rolled = { values: Randomized; cables: Record<string, CableV2['value']>; macros?: Record<string, number> };
/** What the panel's dice are checked against: the playing track's signals and beats, and where in it. */
export type DiceCheck = { signals: Signals; beats: Beats | null; position: number };

/** How many seeds the panel's dice try against the track before taking the least over its limits. */
const TRIES = 8;

/** The limits the rack's Calm holds the picture to: each Ducker keyed by a Meter's kind, its threshold as set (its
 *  macros in, along its taper) over toV2's margin; a kind none holds, the standing limit. */
export function diceLimits(patch: PatchV2): Record<Budgeted, number> {
  const flat = flatten(patch);
  const out = { ...STANDING } as Record<Budgeted, number>;
  const typeOf = new Map(flat.modules.map((m) => [m.id, m.type]));
  for (const c of flat.cables) {
    const kind = typeOf.get(c.from.module) === 'meter' && typeOf.get(c.to.module) === 'ducker' && c.to.port === 'key' ? METER_BUDGET[c.from.port] : undefined;
    const m = kind && flat.modules.find((x) => x.id === c.to.module);
    if (!kind || !m) continue;
    let threshold = m.knobs.threshold;
    for (const x of flat.macros)
      for (const t of x.targets)
        if (t.module === m.id && t.knob === 'threshold') {
          const at = Math.min(1, Math.max(0, x.value));
          threshold = t.from > 0 && t.to > 0 ? t.from * (t.to / t.from) ** at : t.from + (t.to - t.from) * at;
        }
    if (typeof threshold === 'number' && Number.isFinite(threshold) && threshold > 0) out[kind] = threshold / THRESHOLD_OVER_LIMIT;
  }
  for (const k of BUDGETED) if (!(out[k] > 0)) out[k] = STANDING[k];
  return out;
}

/** A roll of `patch` over `roll` from `seed` (a fraction `reach` of each knob's range from where it is: 1 anywhere),
 *  checked against the track where `check` is given. */
export function* rollOf(patch: PatchV2, types: Record<string, ModuleType>, how: VisualSettings, seed: number, roll: Roll, check: DiceCheck | null, reach = 1): Generator<void, Rolled> {
  const scope = typeof roll === 'string' ? roll : null;
  const ids = typeof roll === 'string' ? null : new Set(roll.module);
  // The scopes pinned (`scope:<name>`, the dice's column): kept by every roll, whatever its scope.
  const held = patch.pins.filter((x) => x.startsWith('scope:')).map((x) => x.slice('scope:'.length));
  const heldScopes = held.map((x) => scopeNamed(x, types)).filter((x): x is Scope => !!x);
  const asked: Scope | null = scope ? scopeNamed(scope, types) : (m) => ids!.has(m.id);
  const knobsScope: Scope | null = asked && heldScopes.length ? (m, k) => asked(m, k) && !heldScopes.some((h) => h(m, k)) : asked;
  const withCables = (scope === 'everything' || scope === 'cables' || scope === 'response') && !held.includes('cables') && !held.includes('response');
  const values: Randomized = knobsScope ? yield* randomize(patch, types, { seed, scope: knobsScope, reach }) : {};
  const cables = withCables ? randomizeCables(patch, types, { seed, reach }) : {};
  // Macros: the rack's own ranges, so no check against the track.
  if (scope === 'macros') return { values, cables, macros: held.includes('macros') ? {} : randomizeMacros(patch, { seed, reach }) };
  if (!check) return { values, cables };
  const limits = diceLimits(patch);
  let best: { values: Randomized; cables: typeof cables; by: number } | null = null;
  for (let t = 0; t < TRIES; t++) {
    const tryValues: Randomized = t === 0 ? values : knobsScope ? yield* randomize(patch, types, { seed: seed + t, scope: knobsScope, reach }) : {};
    const tryCables = t === 0 ? cables : withCables ? randomizeCables(patch, types, { seed: seed + t, reach }) : {};
    const candidate = structuredClone(patch);
    applyKnobs(candidate, tryValues);
    for (const c of candidate.cables) if (c.id in tryCables) c.value = tryCables[c.id];
    const checked: Checked = yield* precheckingPatch(candidate, how, check.signals, check.beats, check.position, limits);
    if (checked.ok) return { values: tryValues, cables: tryCables };
    const by = checked.over.reduce((sum, m) => sum + ((checked.worst as Record<string, number>)[m] ?? 0) / ((limits as Record<string, number>)[m] ?? 1), 0);
    if (!best || by < best.by) best = { values: tryValues, cables: tryCables, by };
  }
  return best ?? { values, cables };
}
