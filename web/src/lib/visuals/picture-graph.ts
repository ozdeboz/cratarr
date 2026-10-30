// The picture graph (decision 0057, step 3): the v2 patch's picture side as what is drawn each frame,
// apart from how anything draws (the nodes, GL), so it runs in Node's tests.
// - `compile` (on a change of structure only): the picture and light cables, what reaches the Screen, the order it is
//   drawn in (a Feedback's output is last frame's, so it is a source; the Feedback itself comes after its input and
//   after every reader of what it held), refusing a cycle not through a Feedback and, in this version, any light but
//   one flame's own loop.
// - `plan` (each frame): the lazy rules. A Mix input at amount 0 is cut with what feeds only it; a module at rest
//   passes its picture on (an alias); a Mix of one picture over, added or lightened at 1 is that picture; one coming
//   back (its morph's target above neutral) keeps running. Then each live output's readers.
// - `walk` (each frame): the plan drawn with targets from a `Pool` by size and format, each given back once its last
//   reader has drawn; a Feedback adopts its input's target rather than copying it.
// Devices are opened first (v2.ts `flatten`): they cost nothing. A polymorphic port takes light or a picture, its
// module's output the type of what comes in. A module's picture outputs are one picture.
import { ENGINE_MODULES, GRAPH_MODULES, LIGHT_CARRIERS, MIX_MODES, PICTURE_MODULES, portsOf, showPartTypes, SHOW_PARTS, type Port, type PatchV2, type PortRef, type Ports, type PortType } from './ports';
import type { ModuleType } from './patch';
import { EXTRA_TYPES } from './registry';
import { EFFECT_TYPES, effectOfType } from './effect-modules';
import { flatten } from './v2';
// A converted patch's picture side (v2.ts's hook).
import './picture-wiring';

export type TypeOf = (type: string) => (ModuleType & { ports?: Ports }) | undefined;

/** The picture side's kinds of port. */
export type PictureType = Extract<PortType, 'picture' | 'light'>;
const isPictureType = (t: PortType | undefined): t is PictureType => t === 'picture' || t === 'light';

/** One picture input of a module: its port and type, and the output feeding it. */
export type Feed = { port: string; type: PictureType; from: PortRef };

/** A module of the picture graph: its picture inputs in the order its type declares them; `shown`: a part of Show's
 *  on a picture fused into Show's pass (compile). */
export type PictureNode = { id: string; type: string; inputs: Feed[]; shown?: true };

export type Compiled = {
  /** What reaches the Screen, in the order it is drawn. */
  order: PictureNode[];
  screen: string | null;
  /** Why some of the patch is not drawn, each a sentence; what is left is drawn. */
  errors: string[];
};

/** Every module type a converted patch holds: the extras', today's chain's (`chain`: patch.ts cratarrTypes), the
 *  engine's, the graph's, the picture modules (Glow, Tint…) and Show's parts. */
export function v2Types(chain: readonly ModuleType[]): TypeOf {
  const show = chain.find((t) => t.type === 'show');
  const all = new Map<string, ModuleType & { ports?: Ports }>([...EXTRA_TYPES, ...chain, ...ENGINE_MODULES, ...GRAPH_MODULES, ...PICTURE_MODULES, ...EFFECT_TYPES, ...(show ? showPartTypes(show) : [])].map((t) => [t.type, t]));
  return (t) => all.get(t);
}

/** The passes a picture graph is drawn with, in their order: the flame's own loop on its light, its points, Show's
 *  pass (what takes the light, and the parts on a picture fused into it), the compose step (every other picture
 *  module), the screen. A module's pass is by what it takes, not where it stands: `stagesOf`. */
export const STAGES = ['carry', 'points', 'show', 'compose', 'screen'] as const;
export type Stage = (typeof STAGES)[number];
const SHOWN = new Set(SHOW_PARTS.map((p) => p.type));

/** The pass a module is drawn in, by what it takes (none for a Feedback: it keeps a picture, it draws none). Show's
 *  are what take the light (Bend and Blur on it, Tone map) and the parts on a picture fused after them (`shown`);
 *  Bend, Blur, Curve and Saturate on any other picture are modules like the rest. */
export function stageOf(n: PictureNode): Stage | null {
  if (n.type === 'feedback') return null;
  const light = n.inputs.some((f) => f.type === 'light');
  if (LIGHT_CARRIERS.has(n.type) || (effectOfType(n.type) && light)) return 'carry';
  // A Transform on the light is the points' camera's view (transform.ts): drawn as they land.
  if (LIGHT_ROLES.points.has(n.type) || (n.type === 'transform' && n.inputs[0]?.type === 'light')) return 'points';
  if (n.shown || (SHOWN.has(n.type) && (light || LIGHT_ROLES.tonemap.has(n.type)))) return 'show';
  if (n.type === 'screen') return 'screen';
  return 'compose';
}

/** Why a compiled graph cannot be drawn in its passes (a module reading one drawn in a later pass: a picture into
 *  the light), or null. */
export function stageRefusal(c: Compiled): string | null {
  const byId = new Map(c.order.map((n) => [n.id, n]));
  for (const n of c.order) {
    const s = stageOf(n);
    if (!s) continue;
    for (const f of n.inputs) {
      const from = byId.get(f.from.module);
      const fs = from && stageOf(from);
      if (fs && STAGES.indexOf(fs) > STAGES.indexOf(s)) return `${n.id} takes ${from.id}'s picture, drawn after it: not drawn.`;
    }
  }
  return null;
}

/** The stages a compiled graph needs, in their order, or null where one of its modules reads another drawn in a later
 *  pass (`stageRefusal`). */
export function stagesOf(c: Compiled): Stage[] | null {
  if (stageRefusal(c)) return null;
  const used = new Set(c.order.map(stageOf));
  return STAGES.filter((s) => used.has(s));
}

/** An output's key: `module.port`. */
export const outKey = (r: PortRef) => `${r.module}.${r.port}`;

/** The flame's own loop (light) by type: the points (light in and out), Tone map (light in, a picture out: the only
 *  one), and what carries the light from one frame to the next (any polymorphic module on light too). */
export const LIGHT_ROLES = {
  points: new Set(['flame']),
  tonemap: new Set(['tonemap']),
  carry: LIGHT_CARRIERS,
};

/** The picture graph of `patch` (its devices opened): what reaches the Screen, in order. */
export function compile(patch: PatchV2, typeOf: TypeOf): Compiled {
  const flat = flatten(patch);
  // A module given twice (the same id and type: the conversion's sidechain and the wiring both add the Screen) is one.
  const seen = new Set<string>();
  const p = { ...flat, modules: flat.modules.filter((m) => !seen.has(`${m.id}:${m.type}`) && !!seen.add(`${m.id}:${m.type}`)) };
  const errors: string[] = [];
  const byId = new Map(p.modules.map((m) => [m.id, m]));
  const ports = new Map<string, Ports>();
  for (const m of p.modules) {
    const t = typeOf(m.type);
    if (t) ports.set(m.id, portsOf(t));
  }
  const screens = p.modules.filter((m) => m.type === 'screen');
  if (screens.length !== 1) return { order: [], screen: null, errors: [screens.length ? 'A patch has one Screen.' : 'Nothing reaches a Screen: there is none.'] };
  const screen = screens[0].id;

  // What an output gives: its type, or a polymorphic one's what comes into its module's polymorphic input.
  const outPort = (id: string, port: string): Port | undefined => ports.get(id)?.out.find((x) => x.id === port);
  const typeOut = (id: string, port: string, seen = new Set<string>()): PortType | undefined => {
    const o = outPort(id, port);
    if (!o?.polymorphic || seen.has(id)) return o?.type;
    seen.add(id);
    const poly = ports.get(id)!.in.find((x) => x.polymorphic);
    const c = poly && p.cables.find((x) => x.to.module === id && x.to.port === poly.id && isPictureType(outPort(x.from.module, x.from.port)?.type));
    return (c && typeOut(c.from.module, c.from.port, seen)) || o.type;
  };
  // The picture and light cables into each module, one an input.
  const into = new Map<string, Feed[]>();
  for (const c of p.cables) {
    const type = typeOut(c.from.module, c.from.port);
    if (!isPictureType(type)) continue;
    const port = ports.get(c.to.module)?.in.find((x) => x.id === c.to.port);
    if (!port) {
      errors.push(`${c.from.module}'s ${type} goes to no input (${c.to.module}.${c.to.port}).`);
      continue;
    }
    if (port.type !== type && !(port.polymorphic && isPictureType(port.type))) {
      errors.push(`${c.to.module}.${c.to.port} takes a ${port.type}, not ${c.from.module}'s ${type}.`);
      continue;
    }
    const feeds = into.get(c.to.module) ?? [];
    if (feeds.some((f) => f.port === c.to.port)) {
      errors.push(`${c.to.module}.${c.to.port} takes one ${type}: ${c.from.module}'s is left out.`);
      continue;
    }
    feeds.push({ port: c.to.port, type, from: { module: c.from.module, port: c.from.port } });
    into.set(c.to.module, feeds);
  }
  // In the order the type declares its inputs: the first is what a module at rest passes on.
  const nodeOf = (id: string): PictureNode => {
    const declared = ports.get(id)?.in.map((x) => x.id) ?? [];
    const inputs = [...(into.get(id) ?? [])].sort((a, b) => declared.indexOf(a.port) - declared.indexOf(b.port));
    return { id, type: byId.get(id)!.type, inputs };
  };

  // What reaches the Screen: back along the cables, a Feedback's input with it.
  const reached = new Map<string, PictureNode>();
  const back = [screen];
  while (back.length) {
    const id = back.pop()!;
    if (reached.has(id) || !byId.has(id)) continue;
    const n = nodeOf(id);
    reached.set(id, n);
    for (const f of n.inputs) back.push(f.from.module);
  }

  // A module after what feeds it; a Feedback after its readers (it takes its input's picture once they have read
  // what it held). The patch's order where nothing decides.
  const isFeedback = (id: string) => byId.get(id)?.type === 'feedback';
  const after = new Map<string, Set<string>>([...reached.keys()].map((id) => [id, new Set<string>()]));
  for (const n of reached.values())
    for (const f of n.inputs) {
      if (isFeedback(f.from.module)) after.get(f.from.module)!.add(n.id);
      else after.get(n.id)!.add(f.from.module);
    }
  const rank = new Map(p.modules.map((m, i) => [m.id, i]));
  const waiting = [...reached.keys()].sort((a, b) => rank.get(a)! - rank.get(b)!);
  const placed = new Set<string>();
  let order: PictureNode[] = [];
  const cut = new Set<string>();
  while (waiting.length) {
    const i = waiting.findIndex((id) => [...after.get(id)!].every((d) => placed.has(d) || cut.has(d)));
    if (i >= 0) {
      const [id] = waiting.splice(i, 1);
      placed.add(id);
      order.push(reached.get(id)!);
      continue;
    }
    // Stuck: the modules on a cycle are left out (what reads them goes on without them).
    const onCycle = waiting.filter((id) => reaches(id, id, after, cut));
    errors.push(`A picture cycle must pass through a Feedback: ${onCycle.join(', ')} not drawn.`);
    for (const id of onCycle) {
      cut.add(id);
      waiting.splice(waiting.indexOf(id), 1);
    }
  }
  const lightOut = new Set(order.filter((n) => ports.get(n.id)?.out.some((o) => typeOut(n.id, o.id) === 'light')).map((n) => n.id));
  const light = lightCheck(order, lightOut);
  errors.push(...light.errors);
  // What is left out is left out of what reads it too.
  const out = new Set([...reached.keys()].filter((id) => !placed.has(id) || light.refused.has(id)));
  order = order.filter((n) => !out.has(n.id)).map((n) => ({ ...n, inputs: n.inputs.filter((f) => !out.has(f.from.module)) }));
  return { order: showFused(order), screen, errors };
}

/** `order` with Show's parts on a picture that follow straight on from Tone map in Show's own order (Curve, then
 *  Saturate), each read by the next alone, marked `shown`: drawn in Show's pass, as one. */
function showFused(order: PictureNode[]): PictureNode[] {
  const place = (t: string) => SHOW_PARTS.findIndex((p) => p.type === t);
  const readers = (id: string) => order.filter((n) => n.inputs.some((f) => f.from.module === id));
  const fused = new Set<string>();
  for (const t of order.filter((n) => LIGHT_ROLES.tonemap.has(n.type))) {
    let at = t;
    for (;;) {
      const r = readers(at.id);
      const next = r.length === 1 && r[0].inputs.length === 1 && SHOWN.has(r[0].type) ? r[0] : null;
      if (!next || place(next.type) <= place(at.type)) break;
      fused.add(next.id);
      at = next;
    }
  }
  return fused.size ? order.map((n) => (fused.has(n.id) ? { ...n, shown: true as const } : n)) : order;
}

/** Whether `to` is reached from `from` along `after` (not through `cut`). */
function reaches(from: string, to: string, after: Map<string, Set<string>>, cut: Set<string>): boolean {
  const seen = new Set<string>();
  const next = [...(after.get(from) ?? [])];
  while (next.length) {
    const id = next.pop()!;
    if (id === to) return true;
    if (seen.has(id) || cut.has(id)) continue;
    seen.add(id);
    next.push(...(after.get(id) ?? []));
  }
  return false;
}

/** The light this version draws: one flame's own loop (Feedback → carry … → the points), and from the points through
 *  what passes light on into Tone map's read (Bend, Blur: polymorphic, fused into its pass) to Tone map, one or
 *  several. `lightOut`: the modules giving light. */
function lightCheck(order: PictureNode[], lightOut: Set<string>): { errors: string[]; refused: Set<string> } {
  const errors: string[] = [];
  const refused = new Set<string>();
  const lightIn = (n: PictureNode) => n.inputs.filter((f) => f.type === 'light');
  const users = order.filter((n) => lightIn(n).length);
  if (!users.length) return { errors, refused };
  const byId = new Map(order.map((n) => [n.id, n]));
  const points = order.filter((n) => LIGHT_ROLES.points.has(n.type));
  if (points.length > 1) errors.push(`This version draws one flame's light: ${points.slice(1).map((n) => n.id).join(', ')} left out.`);
  const flame = points[0];
  // Back from the points through the carries to a Feedback whose input is the points' light.
  const loop = new Set<string>();
  let whole = false;
  if (flame) {
    loop.add(flame.id);
    let at = lightIn(flame)[0]?.from.module;
    const carries = (id: string) => LIGHT_ROLES.carry.has(byId.get(id)?.type ?? '') || (lightOut.has(id) && byId.get(id)?.type !== 'feedback');
    while (at && carries(at) && !loop.has(at)) {
      loop.add(at);
      at = lightIn(byId.get(at)!)[0]?.from.module;
    }
    const fb = at ? byId.get(at) : undefined;
    whole = fb?.type === 'feedback' && lightIn(fb)[0]?.from.module === flame.id;
    if (fb) loop.add(fb.id);
    if (!whole) errors.push(`${flame.id}'s light must come round through a Feedback (Feedback → carry … → ${flame.id}).`);
  }
  // After the points: what passes the light on, to Tone map (in order, so each sees what it reads).
  const onward = new Set<string>(flame ? [flame.id] : []);
  for (const n of users) {
    if (whole && loop.has(n.id)) continue;
    const passes = lightOut.has(n.id) && !LIGHT_ROLES.carry.has(n.type) && !LIGHT_ROLES.points.has(n.type);
    if (whole && lightIn(n).every((f) => onward.has(f.from.module)) && (LIGHT_ROLES.tonemap.has(n.type) || passes)) {
      if (lightOut.has(n.id)) onward.add(n.id);
      continue;
    }
    refused.add(n.id);
    if (whole) errors.push(`${n.id} cannot take light in this version: only the flame's own loop, and on to Tone map, do.`);
  }
  if (!whole) for (const id of loop) refused.add(id);
  for (const n of points.slice(1)) refused.add(n.id);
  return { errors, refused };
}

// ---- Each frame: the lazy rules ----

export type FrameRules = {
  /** A module's knobs as drawn this frame. */
  knobs: (id: string) => Record<string, number>;
  /** Whether a module is at rest: at its neutral, and its own state too (a clock still turning, a trail still dying
   *  away, a fluid still flowing): its picture is then its first input's. */
  idle?: (id: string) => boolean;
  /** Where a knob's morph is going (its value as drawn, where it is not moving). */
  target?: (id: string, knob: string) => number;
  /** Whether a module's morph is bringing it back (its target above neutral): it keeps running while at rest, so
   *  what it gathers is there as it comes (the flame coming back from blend 0). */
  returning?: (id: string) => boolean;
};

export type Plan = {
  /** The modules drawn this frame, in order. */
  live: PictureNode[];
  /** What each live module reads this frame, by its input port: the output that draws it (through the aliases). */
  reads: Map<string, Map<string, PortRef>>;
  /** The modules whose picture is another's this frame (at rest; a Mix of one picture), by module. */
  alias: Map<string, PortRef>;
  /** Each read output's readers this frame, by `outKey`. */
  readers: Map<string, number>;
};

const NEVER_LAZY = new Set(['screen', 'feedback']);
const MIX_ALIASED = new Set<number>([MIX_MODES.indexOf('over'), MIX_MODES.indexOf('add'), MIX_MODES.indexOf('lighten')]);

/** This frame's drawing of `c` under the lazy rules. */
export function plan(c: Compiled, rules: FrameRules): Plan {
  const byId = new Map(c.order.map((n) => [n.id, n]));
  const idle = rules.idle ?? (() => false);
  const returning = rules.returning ?? (() => false);
  const target = rules.target ?? ((id: string, knob: string) => rules.knobs(id)[knob] ?? 0);
  const amountOf = (f: Feed) => `amount${f.port.replace(/^in/, '')}`;
  // The inputs that count this frame: a Mix's at amount 0 cut, unless its amount is coming back.
  const used = (n: PictureNode): Feed[] => {
    if (n.type !== 'mix') return n.inputs;
    const k = rules.knobs(n.id);
    return n.inputs.filter((f) => (k[amountOf(f)] ?? 1) > 0 || target(n.id, amountOf(f)) > 0);
  };
  const alias = new Map<string, PortRef>();
  for (const n of c.order) {
    if (NEVER_LAZY.has(n.type)) continue;
    const inputs = used(n);
    if (n.type === 'mix') {
      const k = rules.knobs(n.id);
      const f = inputs[0];
      if (inputs.length === 1 && MIX_ALIASED.has(Math.round(k[`mode${f.port.replace(/^in/, '')}`] ?? 0)) && (k[amountOf(f)] ?? 1) >= 1) alias.set(n.id, f.from);
    } else if (inputs.length && idle(n.id) && !returning(n.id)) alias.set(n.id, inputs[0].from);
  }
  const resolve = (r: PortRef): PortRef => {
    let at = r;
    for (let guard = 0; alias.has(at.module) && guard < byId.size; guard++) at = alias.get(at.module)!;
    return at;
  };
  // What is drawn: back from the Screen along what counts, through the aliases.
  const live = new Set<string>();
  const back = c.screen ? [c.screen] : [];
  while (back.length) {
    const id = back.pop()!;
    const n = byId.get(id);
    if (!n || live.has(id)) continue;
    live.add(id);
    for (const f of used(n)) back.push(resolve(f.from).module);
  }
  const order = c.order.filter((n) => live.has(n.id));
  const reads = new Map<string, Map<string, PortRef>>();
  const readers = new Map<string, number>();
  for (const n of order) {
    const r = new Map<string, PortRef>();
    for (const f of used(n)) {
      const from = resolve(f.from);
      r.set(f.port, from);
      readers.set(outKey(from), (readers.get(outKey(from)) ?? 0) + 1);
    }
    reads.set(n.id, r);
  }
  return { live: order, reads, alias, readers };
}

// ---- Targets: a pool by size and format ----

export type Format = 'rgba8' | 'rgba16f';
export type Spec = { w: number; h: number; format: Format };
const specKey = (s: Spec) => `${s.w}x${s.h}:${s.format}`;

/** Targets to draw into, kept by size and format and given out again once given back; let go of after `unused` ms
 *  not taken (as the walk's targets, nodes/walker.ts). */
export class Pool<T> {
  private free = new Map<string, { t: T; at: number }[]>();
  private specs = new Map<T, Spec>();
  private make: (s: Spec) => T;
  private release: (t: T) => void;
  private now: () => number;
  private unused: number;
  constructor(make: (s: Spec) => T, release: (t: T) => void, now: () => number = () => performance.now(), unused = 10_000) {
    this.make = make;
    this.release = release;
    this.now = now;
    this.unused = unused;
  }

  take(s: Spec): T {
    const kept = this.free.get(specKey(s))?.pop();
    if (kept) return kept.t;
    const t = this.make(s);
    this.specs.set(t, s);
    return t;
  }

  give(t: T) {
    const s = this.specs.get(t);
    if (!s) return;
    const k = specKey(s);
    if (!this.free.has(k)) this.free.set(k, []);
    this.free.get(k)!.push({ t, at: this.now() });
  }

  /** Lets go of the targets not taken for `unused` ms. */
  sweep() {
    const now = this.now();
    for (const [k, list] of this.free) {
      const old = list.filter((x) => now - x.at >= this.unused);
      for (const x of old) (this.release(x.t), this.specs.delete(x.t));
      this.free.set(k, list.filter((x) => now - x.at < this.unused));
    }
  }

  /** The targets made and not let go of, in use or free. */
  get size(): number {
    return this.specs.size;
  }

  /** Lets go of every target, those still out too (a resize: what holds one lets go of it). */
  clear() {
    for (const t of this.specs.keys()) this.release(t);
    this.specs.clear();
    this.free.clear();
  }
}

/** What a walk asks of the nodes: each one's target (`own`: it keeps its own, as the flame its light; null: none, the
 *  screen), its drawing, and a copy (a Feedback of a picture something else keeps). */
export type WalkIO<T> = {
  spec: (n: PictureNode) => Spec | 'own' | null;
  /** Draws `n` from its inputs (null: nothing yet, black) into `out`; gives back its own picture where it keeps one. */
  draw: (n: PictureNode, inputs: Map<string, T | null>, out: T | null) => T | void;
  copy: (from: T, to: T) => void;
};

/** What a walk keeps from one frame to the next: each Feedback's picture (`pooled`: the pool's, given back when the
 *  next replaces it). A resize clears it, with the pool. `pinned`: the observed module's target, kept out of the pool
 *  past its last reader for the caller to read (and give back) after the walk. */
export type WalkState<T> = { held: Map<string, { t: T; pooled: boolean }>; pinned?: T | null };
export const newWalkState = <T>(): WalkState<T> => ({ held: new Map(), pinned: null });

/** Draws `p` in order, with targets from `pool`, each given back as its last reader has drawn. A Feedback gives what
 *  it held from the start of the frame; at its place it adopts its input's target, copying only one not the pool's
 *  (another's own), and keeping the light's own buffers as they are. `given`: pictures drawn outside the walk, by
 *  module. `pin`: the observed module, whose target is not given back but left in `state.pinned`. Gives every
 *  module's picture. */
export function walk<T>(p: Plan, pool: Pool<T>, io: WalkIO<T>, state: WalkState<T>, given: ReadonlyMap<string, T> = new Map(), pin: string | null = null): Map<string, T | null> {
  const value = new Map<string, T | null>(given);
  const pooled = new Set<T>();
  const left = new Map<string, number>();
  for (const [key, n] of p.readers) {
    const m = key.slice(0, key.lastIndexOf('.'));
    left.set(m, (left.get(m) ?? 0) + n);
  }
  state.pinned = null;
  for (const n of p.live) if (n.type === 'feedback') value.set(n.id, state.held.get(n.id)?.t ?? null);
  const back = (module: string, t: T) => {
    pooled.delete(t);
    if (module === pin) state.pinned = t;
    else pool.give(t);
  };
  // One of `module`'s readers has drawn: its target back to the pool after the last.
  const read = (module: string) => {
    const n = (left.get(module) ?? 0) - 1;
    left.set(module, n);
    const t = value.get(module);
    if (n <= 0 && t && pooled.has(t)) back(module, t);
  };
  for (const n of p.live) {
    const reads = [...(p.reads.get(n.id) ?? new Map<string, PortRef>())];
    if (n.type === 'feedback') {
      const from = reads[0]?.[1];
      const t = from ? (value.get(from.module) ?? null) : null;
      const s = io.spec(n);
      let held: { t: T; pooled: boolean } | null = null;
      if (t && pooled.has(t)) {
        // Adopted: the pool's no longer, though whatever reads it later this frame still can.
        pooled.delete(t);
        held = { t, pooled: true };
      } else if (t && s && s !== 'own') {
        const to = pool.take(s);
        io.copy(t, to);
        held = { t: to, pooled: true };
      } else if (t) held = { t, pooled: false };
      if (from) read(from.module);
      const was = state.held.get(n.id);
      if (was?.pooled && was.t !== held?.t) pool.give(was.t);
      if (held) state.held.set(n.id, held);
      else state.held.delete(n.id);
      continue;
    }
    const s = io.spec(n);
    const out = s && s !== 'own' ? pool.take(s) : null;
    const own = io.draw(n, new Map(reads.map(([port, from]) => [port, value.get(from.module) ?? null])), out);
    value.set(n.id, out ?? own ?? null);
    if (out) pooled.add(out);
    for (const [, from] of reads) read(from.module);
    // Read by nothing: back at once.
    if (out && !(left.get(n.id) ?? 0)) back(n.id, out);
  }
  return value;
}

/** The compose step's modules (with its loops' Feedbacks), the module whose picture it starts from (Show's; none
 *  where there is no flame), and the one it gives on (into the screen); none where it has none. */
export function composeOf(c: Compiled): { ids: Set<string>; from: string | null; to: string } | null {
  const byId = new Map(c.order.map((n) => [n.id, n]));
  const ids = new Set(c.order.filter((n) => stageOf(n) === 'compose').map((n) => n.id));
  for (const n of c.order) if (n.type === 'feedback' && n.inputs.some((f) => ids.has(f.from.module))) ids.add(n.id);
  const from = c.order.flatMap((n) => (ids.has(n.id) ? n.inputs : [])).find((f) => stageOf(byId.get(f.from.module)!) === 'show')?.from.module;
  const to = c.order.flatMap((n) => (ids.has(n.id) || n.type === 'feedback' ? [] : n.inputs)).find((f) => ids.has(f.from.module))?.from.module;
  return ids.size && to ? { ids, from: from ?? null, to } : null;
}
