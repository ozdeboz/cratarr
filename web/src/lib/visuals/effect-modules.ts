// One module per effect (decision 0057 §3): each of effects.ts's effects a module type of its own (`fx-<name>`): its
// Amount and its own knobs (named as their labels). What today's three slots (an engine choosing one of its effects by
// a step) and the loop's slot convert to: the chosen effect's module, its knobs from the slot's, none for a slot on no
// effect or at amount 0. The effects that may run in the flame's own loop take light or a picture.
import { EFFECTS, effectOf, engineEffect, ENGINES, loopEffect, type Effect } from './effects';
import { engineOf } from './engine-module';
import type { Knob, Module, ModuleType } from './patch';
import { MIX_MODES, type CableV2, type PatchV2, type Port, type Ports } from './ports';

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

/** A slot the dice and the moments set an effect on: the line's three (each its engine's effects) and the loop's. */
export type Slot = 'fx1' | 'fx2' | 'fx3' | 'loop-effect';
const LINE: readonly Slot[] = ['fx1', 'fx2', 'fx3'];

const takes = (slot: Slot, e: Effect) => (slot === 'loop-effect' ? e.slots.includes('loop') : e.slots.includes('picture') && e.engine === ENGINES[LINE.indexOf(slot)].key);

/** The module on `slot` at `p`'s top level: an effect's of the slot's engine (the loop's: `loop-<name>`); none where
 *  the slot draws nothing. */
export function slotModule(p: PatchV2, slot: Slot): Module | null {
  return (
    p.modules.find((m) => {
      const e = effectOfType(m.type);
      return !!e && (slot === 'loop-effect' ? m.id === `loop-${e.name}` : !m.id.startsWith('loop-') && takes(slot, e));
    }) ?? null
  );
}

type End = [module: string, port: string];
const wire = (from: End, to: End): CableV2 => ({ id: `picture:${from[0]}.${from[1]}>${to[0]}.${to[1]}`, from: { module: from[0], port: from[1] }, to: { module: to[0], port: to[1] }, value: 1 });
const kind = (m: Module) => effectOfType(m.type)?.kind;

/** `p` with `effect` (effects.ts id; 0 for none) on `slot` (0057 §3, for the dice and the moments): the slot's module
 *  swapped for the effect's (the cables into its knobs, its pins and the macros' targets carried to the knob at the
 *  same place, Amount as it is; dropped where the new effect has no such knob), added into its line, or (effect 0,
 *  amount 0) taken out and the line closed. The line is rewired as toV2 wires it (picture-wiring.ts): its effects in
 *  their order, an Echo in its visible Feedback and Mix loop. An effect the slot does not take
 *  leaves `p` as it is. Pure: a new patch. */
export function placeEffect(p: PatchV2, slot: Slot, effect: number, knobs: { amount: number; k1?: number; k2?: number }): PatchV2 {
  const q = structuredClone(p);
  const e = effectOf(effect);
  if (e && !takes(slot, e)) return q;
  const old = slotModule(q, slot);
  const was = old ? effectKnobIds(effectOfType(old.type)!) : [];
  const names = e ? effectKnobIds(e) : [];
  const next: Module | null =
    e && knobs.amount > 0
      ? {
          id: slot === 'loop-effect' ? `loop-${e.name}` : effectType(e),
          type: effectType(e),
          knobs: { amount: knobs.amount, ...Object.fromEntries(names.map((n, i) => [n, [knobs.k1, knobs.k2][i] ?? old?.knobs[was[i]] ?? 0.5])) },
        }
      : null;
  if (!old && !next) return q;
  // Where a knob of the old module is on the new one: Amount as it is, the others by their place.
  const at = (module: string, k: string): End | null => {
    if (module !== old?.id) return [module, k];
    const to = !next ? null : k === 'amount' || k === 'picture' ? k : names[was.indexOf(k)];
    return next && to ? [next.id, to] : null;
  };
  q.pins = q.pins.flatMap((pin) => {
    const dot = pin.indexOf('.');
    const to = dot < 0 ? [pin] : at(pin.slice(0, dot), pin.slice(dot + 1));
    return to ? [to.join('.')] : [];
  });
  q.macros = q.macros.map((x) => ({
    ...x,
    targets: x.targets.flatMap((t) => {
      const to = at(t.module, t.knob);
      return to ? [{ ...t, module: to[0], knob: to[1] }] : [];
    }),
  }));
  // Each cable of the old module's on the new one's (none where it goes).
  const moved = (c: CableV2): CableV2[] => {
    if (!old || (c.from.module !== old.id && c.to.module !== old.id)) return [c];
    if (!next) return [];
    const to = at(c.to.module, c.to.port);
    return to ? [{ ...c, from: c.from.module === old.id ? { ...c.from, module: next.id } : c.from, to: { ...c.to, module: to[0], port: to[1] } }] : [];
  };
  return slot === 'loop-effect' ? onLoop(q, old, next, moved) : onLine(q, slot, next, moved);
}

/** The loop's slot on the flame's light: after what carries it first (its Feedback, Trails, Fluid, Tunnel), before
 *  what carries it on (Dive, the flame). */
function onLoop(q: PatchV2, old: Module | null, next: Module | null, moved: (c: CableV2) => CableV2[]): PatchV2 {
  if (old) {
    const into = q.cables.find((c) => c.to.module === old.id && c.to.port === 'picture');
    const outs = q.cables.filter((c) => c.from.module === old.id && c.from.port === 'picture');
    q.modules = q.modules.flatMap((m) => (m === old ? (next ? [next] : []) : [m]));
    q.cables = q.cables.flatMap(moved);
    // Taken out: what fed it feeds what it fed.
    if (!next && into) q.cables.push(...outs.map((o) => wire([into.from.module, into.from.port], [o.to.module, o.to.port])));
    return q;
  }
  const before = ['tunnel', 'fluid', 'trails', 'light-feedback'];
  const c = before.flatMap((id) => q.cables.filter((x) => x.from.module === id && !before.includes(x.to.module)))[0];
  if (!c || !next) return q;
  q.cables = q.cables.filter((x) => x !== c);
  q.cables.push(wire([c.from.module, c.from.port], [next.id, 'picture']), wire([next.id, 'picture'], [c.to.module, c.to.port]));
  q.modules.splice(q.modules.findIndex((m) => m.id === c.from.module) + 1, 0, next);
  return q;
}

/** A slot of the line (from the layers or the picture's own loop into the Screen): the line wired anew. */
function onLine(q: PatchV2, slot: Slot, next: Module | null, moved: (c: CableV2) => CableV2[]): PatchV2 {
  const before = LINE.flatMap((s) => slotModule(q, s) ?? []);
  const slots = LINE.flatMap((s) => (s === slot ? (next ?? []) : (slotModule(q, s) ?? [])));
  const line = new Set(before.flatMap((m) => [m.id, `${m.id}-mix`, `${m.id}-feedback`]).filter((id) => q.modules.some((m) => m.id === id)));
  // Where the line starts (back from the Screen along its own pictures), and what its end feeds.
  const feed = (id: string, port: string) => q.cables.find((c) => c.to.module === id && c.to.port === port);
  const screen = q.modules.find((m) => m.type === 'screen');
  let into = screen && feed(screen.id, 'picture');
  const tail = into && line.has(into.from.module) ? into.from.module : null;
  for (let i = 0; into && line.has(into.from.module) && i <= line.size; i++) into = feed(into.from.module, into.from.module.endsWith('-mix') ? 'in1' : 'picture');
  const start: End | null = into ? [into.from.module, into.from.port] : null;
  const ends = tail ? q.cables.filter((c) => c.from.module === tail && c.from.port === 'picture' && !line.has(c.to.module)) : screen && start ? q.cables.filter((c) => c.to.module === screen.id && c.to.port === 'picture') : [];
  // The line's own picture cables go (they are made again); the rest of its modules' move with them.
  const own = (c: CableV2) => ends.includes(c) || (line.has(c.to.module) && ['picture', 'in1', 'in2'].includes(c.to.port)) || (line.has(c.from.module) && /-(mix|feedback)$/.test(c.from.module));
  q.cables = q.cables.filter((c) => !own(c)).flatMap(moved);
  const ordered = [...slots.filter((m) => kind(m) !== 'shape'), ...slots.filter((m) => kind(m) === 'shape').reverse()];
  const looped = ordered.filter((m) => kind(m) === 'echo');
  const made: Module[] = [];
  const cables: CableV2[] = [];
  let end = start;
  for (const m of ordered) {
    made.push(m);
    if (looped.includes(m)) {
      const [mix, fb] = [`${m.id}-mix`, `${m.id}-feedback`];
      made.push({ id: fb, type: 'feedback', knobs: {} }, { id: mix, type: 'mix', knobs: { mode1: MIX_MODES.indexOf('over'), amount1: 1, mode2: MIX_MODES.indexOf('lighten'), amount2: 1 } });
      if (end) cables.push(wire(end, [mix, 'in1']));
      cables.push(wire([fb, 'picture'], [m.id, 'picture']), wire([m.id, 'picture'], [mix, 'in2']));
      end = [mix, 'picture'];
    } else {
      if (end) cables.push(wire(end, [m.id, 'picture']));
      end = [m.id, 'picture'];
    }
  }
  if (end) for (const m of looped) cables.push(wire(end, [`${m.id}-feedback`, 'picture']));
  if (end) cables.push(...ends.map((c) => ({ ...c, ...wire(end!, [c.to.module, c.to.port]), value: c.value })));
  // Its modules where the old ones were, or before the Screen.
  const first = q.modules.findIndex((m) => line.has(m.id));
  const kept = q.modules.filter((m) => !line.has(m.id));
  const i = first >= 0 ? q.modules.slice(0, first).filter((m) => !line.has(m.id)).length : kept.findIndex((m) => m.type === 'screen');
  kept.splice(i < 0 ? kept.length : i, 0, ...made);
  q.modules = kept;
  const ids = new Set(q.cables.map((c) => c.id));
  q.cables.push(...cables.filter((c) => !ids.has(c.id) && !!ids.add(c.id)));
  return q;
}
