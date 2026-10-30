// The one macro rule (decision 0077; the architecture audit, 2026-10-29: "one macro resolver, six today"): a macro at
// `at` (0..1) sets each of its targets to its place between the target's `from` and `to`, along the knob's taper (an
// exponential knob's geometrically: 0057's Wild on a Ducker's threshold); several on one knob, the last one's. The
// engine (its live values), the patch as set (v2.ts asSet, a device's macro under the rack's), the evaluator and the
// faces all read a macro through here. Pure.

export type MacroTarget = { from: number; to: number };

/** A target's value with its macro at `at`. */
export function mapped(t: MacroTarget, at: number, exp = false): number {
  return exp && t.from > 0 && t.to > 0 ? t.from * (t.to / t.from) ** at : t.from + (t.to - t.from) * at;
}

/** A knob's base through the macros on it (`targets`, each with its macro's value now: `valueOf`), the last one's. */
export function throughMacros<T extends MacroTarget>(base: number, targets: Iterable<T>, valueOf: (t: T) => number, exp = false): number {
  for (const t of targets) base = mapped(t, valueOf(t), exp);
  return base;
}

/** A macro's own value, within 0..1. */
export const macroAt = (v: number) => Math.min(1, Math.max(0, v));
