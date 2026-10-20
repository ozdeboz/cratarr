// One module per effect (decision 0057 §3): each of effects.ts's effects a module type of its own (`fx-<name>`): its
// Amount and its own knobs (named as their labels). What today's three slots (an engine choosing one of its effects by
// a step) and the loop's slot convert to: the chosen effect's module, its knobs from the slot's, none for a slot on no
// effect or at amount 0. The effects that may run in the flame's own loop take light or a picture.
import { EFFECTS, effectOf, engineEffect, loopEffect, type Effect } from './effects';
import { engineOf } from './engine-module';
import type { Knob, Module, ModuleType } from './patch';
import type { Port, Ports } from './ports';

/** An effect's module type. */
export const effectType = (e: Effect) => `fx-${e.name}`;

/** An effect's own knobs' ids, in its order (today's slots kept them as k1 and k2). */
export const effectKnobIds = (e: Effect): string[] => e.knobs.map((k) => k.label.toLowerCase());

const BY_TYPE = new Map(EFFECTS.map((e) => [effectType(e), e]));

/** The effect a module of `type` is; null for any other type. */
export const effectOfType = (type: string): Effect | null => BY_TYPE.get(type) ?? null;

const knob = (id: string, label: string, kind: Knob['kind'], def: number, neutral: number): Knob => ({ id, label, min: 0, max: 1, neutral, default: def, kind });

/** Each effect as a module type: Amount (none at 0), then its own knobs; a picture in and out, light too for one
 *  the loop takes. */
export const EFFECT_TYPES: readonly (ModuleType & { ports: Ports })[] = EFFECTS.map((e) => {
  const knobs = [knob('amount', 'Amount', 'level', 1, 0), ...e.knobs.map((k, i) => knob(effectKnobIds(e)[i], k.label, 'swing', 0.5, 0.5))];
  const poly = e.slots.includes('loop');
  const io: Port = { id: 'picture', label: 'Picture', type: 'picture', ...(poly ? { polymorphic: true } : {}) };
  return {
    type: effectType(e),
    label: e.name[0].toUpperCase() + e.name.slice(1),
    place: 'module',
    knobs,
    ports: { in: [io, ...knobs.map((k): Port => ({ id: k.id, label: k.label, type: 'value' }))], out: [io] },
  };
});

/** The effect a slot of today's draws (an engine's `fx1`–`fx3`, or the loop's `loop-effect`), at its step; 0 for none. */
export function slotEffect(m: Module): number {
  if (m.type === 'loop-effect') return loopEffect(m.knobs.effect ?? 0);
  const engine = engineOf(m.type);
  return engine ? engineEffect(engine, m.knobs.effect ?? 0) : 0;
}

/** A slot as its effect's module: the picture's by its type, the loop's `loop-<name>`; none on no effect or at 0. */
export function effectModuleOf(m: Module): Module | null {
  const e = effectOf(slotEffect(m));
  if (!e || !((m.knobs.amount ?? 0) > 0)) return null;
  const [n1, n2] = effectKnobIds(e);
  return {
    id: m.type === 'loop-effect' ? `loop-${e.name}` : effectType(e),
    type: effectType(e),
    knobs: { amount: m.knobs.amount, ...(n1 ? { [n1]: m.knobs.k1 ?? 0.5 } : {}), ...(n2 ? { [n2]: m.knobs.k2 ?? 0.5 } : {}) },
  };
}

/** Where a slot's knob is on its effect's module: Amount as it is, K1 and K2 the effect's own; null for its step
 *  (the effect is the module's type now) and a knob the effect has not. */
export function effectKnobOf(m: Module, knob: string): string | null {
  const e = effectOf(slotEffect(m));
  if (!e || knob === 'effect') return null;
  if (knob === 'amount') return 'amount';
  return knob === 'k1' || knob === 'k2' ? (effectKnobIds(e)[knob === 'k1' ? 0 : 1] ?? null) : null;
}

/** Today's slots in the Cratarr chain, by id (an engine module on a layer stays as it is). */
export const SLOTS: ReadonlySet<string> = new Set(['fx1', 'fx2', 'fx3', 'loop-effect']);

type Wired = { to: { module: string; port: string } };

/** Today's slots among `modules` as their effects' modules (0057 §3), the cables into their knobs, the pins and the
 *  macros' targets moved with them; a slot's step (its effect) and a slot with no module gone with what went into it. */
export function slotsAsEffects<C extends Wired, M extends { targets: { module: string; knob: string }[] }>(modules: Module[], cables: C[], pins: string[], macros: M[]): { modules: Module[]; cables: C[]; pins: string[]; macros: M[] } {
  const slots = new Map(modules.filter((m) => SLOTS.has(m.id)).map((m) => [m.id, m]));
  const made = new Map([...slots].map(([id, m]) => [id, effectModuleOf(m)]));
  const at = (module: string, knob: string): [string, string] | null => {
    const slot = slots.get(module);
    if (!slot) return [module, knob];
    const m = made.get(module);
    const k = m && effectKnobOf(slot, knob);
    return m && k ? [m.id, k] : null;
  };
  const out = modules.flatMap((m) => (slots.has(m.id) ? (made.get(m.id) ? [made.get(m.id)!] : []) : [m]));
  const wires = cables.flatMap((c) => {
    const to = at(c.to.module, c.to.port);
    return to ? [{ ...c, to: { ...c.to, module: to[0], port: to[1] } }] : [];
  });
  const kept = pins.flatMap((p) => {
    const dot = p.indexOf('.');
    if (dot < 0) return [p];
    const to = at(p.slice(0, dot), p.slice(dot + 1));
    return to ? [`${to[0]}.${to[1]}`] : [];
  });
  const moved = macros.map((x) => ({
    ...x,
    targets: x.targets.flatMap((t) => {
      const to = at(t.module, t.knob);
      return to ? [{ ...t, module: to[0], knob: to[1] }] : [];
    }),
  }));
  return { modules: out, cables: wires, pins: kept, macros: moved };
}
