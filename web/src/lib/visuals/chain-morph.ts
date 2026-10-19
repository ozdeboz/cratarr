// The patch's extras through claude-632's morph (decision 0056, step 6): what the page morphs, and what it draws while
// a chain change is under way.
//
// - The morph moves the extras' modules (the layers past the flame, the picture's loop, the modulators): today's chain
//   keeps aspects.ts and its own blends.
// - A module the new chain no longer has fades out where it stood: drawn (`withLeaving`) until it is at neutral and
//   the morph lets it go. A whole layer the same. One whose layer is gone, or whose id another module took, leaves at
//   once (nowhere to draw it).
// - The patch the evaluator reads has the extras' knobs as the morph shows them (`morphedPatch`).

import type { Leaving } from './morph';
import type { Layer, Module, Patch, PatchExtra } from './patch';

/** The extras' modules the morph moves, in the patch's order. */
export const morphModules = (extra: PatchExtra): Module[] => [
  ...extra.layers.flatMap((l) => [l.source, ...l.modules, ...l.loop]),
  ...extra.loop,
  ...extra.modulators,
];

/** `now` with what `was` had from `leaving` inserted where it stood (after the one before it that is still there). */
function merged<T extends { id: string }>(now: T[], was: T[], leaving: (x: T) => T | null): T[] {
  const out = [...now];
  const taken = new Set(now.map((x) => x.id));
  for (let i = 0; i < was.length; i++) {
    if (taken.has(was[i].id)) continue;
    const kept = leaving(was[i]);
    if (!kept) continue;
    let at = 0;
    for (let j = i - 1; j >= 0; j--) {
      const k = out.findIndex((x) => x.id === was[j].id);
      if (k >= 0) {
        at = k + 1;
        break;
      }
    }
    out.splice(at, 0, kept);
    taken.add(kept.id);
  }
  return out;
}

/** The extras to draw: `now`, with the modules and layers of `was` still fading out (`leaving`, by module id). */
export function withLeaving(now: PatchExtra, was: PatchExtra, leaving: ReadonlySet<string>): PatchExtra {
  if (!leaving.size) return now;
  const ids = new Set(morphModules(now).map((m) => m.id));
  const going = (m: Module) => (leaving.has(m.id) && !ids.has(m.id) ? m : null);
  const wasLayers = new Map(was.layers.map((l) => [l.id, l]));
  const layers = merged<Layer>(
    now.layers.map((l) => {
      const before = wasLayers.get(l.id);
      return before ? { ...l, modules: merged(l.modules, before.modules, going), loop: merged(l.loop, before.loop, going) } : l;
    }),
    was.layers,
    // A layer gone whole: drawn while its source fades (its modules with it).
    (l) => (going(l.source) ? { ...l, modules: l.modules.filter(going), loop: l.loop.filter(going) } : null),
  );
  return { ...now, layers, loop: merged(now.loop, was.loop, going) };
}

/** The ids of the modules fading out. */
export const leavingIds = (leaving: Leaving[]): Set<string> => new Set(leaving.map((l) => l.id));

/** `p` with each module's knobs as `knobs` has them, by module id (the rest as they are). */
export function morphedPatch(p: Patch, knobs: Record<string, Record<string, number>>): Patch {
  const k = (m: Module): Module => (knobs[m.id] ? { ...m, knobs: knobs[m.id] } : m);
  return {
    ...p,
    layers: p.layers.map((l) => ({ ...l, source: k(l.source), modules: l.modules.map(k), loop: l.loop.map(k) })),
    loop: p.loop.map(k),
    modulators: p.modulators.map(k),
  };
}

/** Whether the two have the same modules (each id with the same type): if not, the change is a chain's. */
export function sameModules(a: PatchExtra, b: PatchExtra): boolean {
  const key = (e: PatchExtra) =>
    [...e.layers.map((l) => `layer:${l.id}`), ...morphModules(e).map((m) => `${m.id}|${m.type}`)].sort().join(',');
  return key(a) === key(b);
}

/** The dice within a chain (the owner: "chains become presets, then within that we can freely evolve"): each of the
 *  extras' knobs anew, its modules and cables as they are, morphed to. A level between its neutral and three quarters
 *  of its way up, a swing anywhere in its range, a step any of its whole steps; the device's knobs, and modulators'
 *  (how the patch moves, not how it looks), kept. `rand` 0..1. */
export function rolledExtra(extra: PatchExtra, types: Record<string, { knobs: { id: string; min: number; max: number; neutral: number; kind: string; device?: boolean }[] }>, rand: () => number): PatchExtra {
  const roll = (m: Module): Module => {
    const knobs = { ...m.knobs };
    for (const k of types[m.type]?.knobs ?? []) {
      if (k.device || !(k.id in knobs)) continue;
      if (k.kind === 'step') knobs[k.id] = Math.min(k.max, Math.ceil(k.min) + Math.floor(rand() * (Math.floor(k.max) - Math.ceil(k.min) + 1)));
      else if (k.kind === 'level') knobs[k.id] = k.neutral + (k.max - k.neutral) * 0.75 * rand();
      else knobs[k.id] = k.min + (k.max - k.min) * rand();
    }
    return { ...m, knobs };
  };
  return {
    ...extra,
    layers: extra.layers.map((l) => ({ ...l, source: roll(l.source), modules: l.modules.map(roll), loop: l.loop.map(roll) })),
    loop: extra.loop.map(roll),
  };
}
