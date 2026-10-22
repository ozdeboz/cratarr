// The effects in the compose step's walk (decision 0057 §3): each effect a module (an `fx-<name>`, or an engine on one of
// its effects), and a line of them, each read only by the next, drawn as one pass (effects.ts pictureShader: up to
// three). An Echo trails its visible Feedback (Feedback → Echo → a Mix keeping the brighter beside the picture), drawn
// in that pass, the Feedback read as its trail; one with none trails nothing (no hidden trail of its own). Pure: the
// walk is rewritten so that each run is one module (at its last step's place, reading the run's picture and its Echo's
// trail) before the lazy rules and the pool see it.
import { effectOf, ENGINES, engineEffect, MOST_PICTURE, type Effect, type Knobs } from '../effects';
import { effectKnobIds, effectOfType } from '../effect-modules';
import { engineOf } from '../engine-module';
import type { PictureNode } from '../picture-graph';

/** An effect as a pass draws it: which, how much, its two knobs. */
export type Fx = { id: number; amount: number; k: Knobs };

/** A run drawn as one pass, by its last step's id: its effects' modules in the line's order, the Feedback its Echo
 *  trails (the run's `trails` input). */
export type Run = { fx: string[]; trails: string | null };

const HALF: Knobs = [0.5, 0.5];

/** The effect a module draws, its knobs as `knobs` gives them (an `fx-<name>`'s own, an engine's k1 and k2); null
 *  for a module that is no effect. */
export function fxOfModule(type: string, knobs: Record<string, number>): Fx | null {
  const e = effectOfType(type);
  const engine = engineOf(type);
  if (!e && !engine) return null;
  const id = e ? e.id : engineEffect(engine!, knobs.effect ?? 1);
  const names = e ? effectKnobIds(e) : ['k1', 'k2'];
  return { id, amount: knobs.amount ?? 0, k: [knobs[names[0]] ?? 0.5, knobs[names[1]] ?? 0.5] };
}

/** The slot an effect was drawn in before it was a module of its own (its engine's: Kaleidoscope 0, EYEDO 1, Hydra 2). */
export const slotOf = (e: Effect) => ENGINES.findIndex((x) => x.key === e.engine);

/** The effects' runs of `order` (`fx`: each module's effect, null for none), and the order with each run one module
 *  (type `fx-run`). `barrier`: the module observed, which ends a run (its picture its own). */
export function effectRuns(order: PictureNode[], fx: (n: PictureNode) => Fx | null, barrier: string | null = null): { runs: Map<string, Run>; order: PictureNode[] } {
  const byId = new Map(order.map((n) => [n.id, n]));
  const isFx = new Set(order.filter((n) => fx(n)).map((n) => n.id));
  const readers = new Map<string, number>();
  for (const n of order) for (const f of n.inputs) readers.set(f.from.module, (readers.get(f.from.module) ?? 0) + 1);
  const kind = (id: string) => effectOf(fx(byId.get(id)!)?.id ?? 0)?.kind;
  // An Echo trailing a Feedback, beside the picture in a Mix keeping the brighter: the Mix is the Echo's step.
  const echoAt = new Map<string, { echo: string; feedback: string }>();
  for (const m of order) {
    if (m.type !== 'mix' || m.inputs.length !== 2 || !m.inputs.some((f) => f.port === 'in1')) continue;
    const e = byId.get(m.inputs.find((f) => f.port === 'in2')?.from.module ?? '');
    const fb = e && e.inputs.length === 1 ? byId.get(e.inputs[0].from.module) : undefined;
    // Not the Echo observed (a barrier): its own pass, then the Mix.
    if (e && fb?.type === 'feedback' && isFx.has(e.id) && kind(e.id) === 'echo' && e.id !== barrier) echoAt.set(m.id, { echo: e.id, feedback: fb.id });
  }
  const looped = new Set([...echoAt.values()].map((x) => x.echo));
  const main = (n: PictureNode) => n.inputs.find((f) => f.port === (echoAt.has(n.id) ? 'in1' : 'picture'));
  const isStep = (n: PictureNode) => echoAt.has(n.id) || (isFx.has(n.id) && !looped.has(n.id) && n.inputs.length === 1 && !!main(n));
  // The lines: a step goes on from the one before where that one is read by it alone, up to three a pass.
  const groups: string[][] = [];
  const groupOf = new Map<string, string[]>();
  for (const n of order) {
    if (!isStep(n)) continue;
    const from = main(n)!.from.module;
    const before = groupOf.get(from);
    const goesOn = before && before[before.length - 1] === from && from !== barrier && before.length < MOST_PICTURE && readers.get(from) === 1;
    const group = goesOn ? before : (groups.push([]), groups[groups.length - 1]);
    group.push(n.id);
    groupOf.set(n.id, group);
  }
  const runs = new Map<string, Run>();
  const steps = new Map<string, string[]>();
  const add = (group: string[], trails: string | null) => {
    const ids = group.map((s) => echoAt.get(s)?.echo ?? s);
    runs.set(group[group.length - 1], { fx: ids, trails });
    steps.set(group[group.length - 1], group);
  };
  for (const group of groups) {
    const last = group[group.length - 1];
    const loops = group.flatMap((s) => (echoAt.has(s) ? [echoAt.get(s)!] : []));
    const trails = loops.find((l) => byId.get(l.feedback)!.inputs[0]?.from.module === last)?.feedback ?? null;
    if (!loops.length || (loops.length === 1 && trails)) add(group, trails);
    // A loop not of the run's own picture: each step its own pass, the Mix drawn as a Mix.
    else for (const s of group) if (!echoAt.has(s)) add([s], null);
  }
  // An effect in no run (an Echo in a loop drawn as a Mix, one read twice …): a pass of its own.
  const inRun = new Set([...runs.values()].flatMap((r) => r.fx));
  for (const n of order) if (isFx.has(n.id) && !inRun.has(n.id) && n.inputs.length === 1) add([n.id], null);
  // Each run one module at its last step's place: its picture the first step's input, its Echo's trail its Feedback's.
  const gone = new Set<string>();
  for (const [last, r] of runs) {
    for (const s of steps.get(last)!) if (s !== last) gone.add(s);
    for (const id of r.fx) if (id !== last) gone.add(id);
  }
  const out = order.flatMap((n): PictureNode[] => {
    if (!runs.has(n.id)) return gone.has(n.id) ? [] : [n];
    const r = runs.get(n.id)!;
    const head = byId.get(steps.get(n.id)![0])!;
    const input = echoAt.has(head.id) || isStep(head) ? main(head)! : head.inputs[0];
    return [{ id: n.id, type: 'fx-run', inputs: [{ ...input, port: 'picture' }, ...(r.trails ? [{ port: 'trails', type: 'picture' as const, from: { module: r.trails, port: 'picture' } }] : [])] }];
  });
  return { runs, order: out };
}

/** Whether a run draws nothing: each effect at nothing, and none that rests away where its time took it. */
export const runIdle = (fxs: Fx[]) => fxs.every((f) => f.amount <= 0 && !effectOf(f.id)?.holds);

/** A run's effects as pictureShader takes them (its ids, amounts and knobs by place, up to three): the colours and
 *  echoes in the line's order, then the shapes the last first (a shape reads its input where it moves). */
export function passOf(fxs: Fx[]): { ids: number[]; amounts: number[]; knobs: Knobs[] } {
  const kind = (f: Fx) => effectOf(f.id)?.kind;
  const order = [...fxs.filter((f) => kind(f) !== 'shape'), ...fxs.filter((f) => kind(f) === 'shape').reverse()];
  return {
    ids: Array.from({ length: MOST_PICTURE }, (_, i) => order[i]?.id ?? 0),
    amounts: Array.from({ length: MOST_PICTURE }, (_, i) => order[i]?.amount ?? 0),
    knobs: Array.from({ length: MOST_PICTURE }, (_, i) => order[i]?.k ?? HALF),
  };
}
