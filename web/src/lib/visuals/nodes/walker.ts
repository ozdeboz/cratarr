// The compose step as the graph's own walk (decision 0057 §4; step 3): every picture module between Show and the
// effects a node, whatever the patch wires: the sources, the layers' modules and engines, Glow, Tint, Mask, Displace,
// Threshold, Blur and Bend on pictures, Warp, Keep, Mix (in one pass), Feedback. Drawn in the graph's order under the
// lazy rules (picture-graph.ts plan), each into a target of the pool given back once its last reader has drawn, a
// Feedback adopting its input's target (walk). A loop of Feedback → Warps and Keeps → a Mix keeping the brighter with
// another picture (on either of its inputs) is drawn as one pass (compose.ts LOOP: MilkDrop's feedback); a line of
// effects too (effect-runs.ts), an Echo trailing its visible Feedback.
import { LOOP, MODULE_MAKERS, mixShader, withInputs } from '../compose';
import { EffectClocks, pictureShader } from '../effects';
import type { Fluid } from '../fluid';
import { effectRuns, passOf, runIdle, type Fx, type Run } from './effect-runs';
import type { Finish } from './finish';
import { FULL, none } from './shared';
import { keptPerSecond, WarpClock, warpFrame, warpUniforms } from '../loopmods';
import { MIX_INPUTS, MIX_MODES } from '../ports';
import type { FrameInfo } from '../patch';
import { newWalkState, outKey, plan, Pool, walk, type Compiled, type PictureNode, type Plan, type Spec, type WalkState } from '../picture-graph';
import type { PortRef } from '../ports';
import { createSource, SOURCE_TYPES } from '../sources/index';
import { program } from '../sources/gl';
import { createNode, nodeOf, PICTURE_NODES, type ModuleNode } from './modules';
import { transformIdle } from '../transform';
import type { Point } from '../points-pictures';
import { POINTS_NODES } from './points-pictures';
import type { FlowField } from './modules';

type Target = { tex: WebGLTexture; fbo: WebGLFramebuffer };

/** How long a module's choice takes to change (0.10; the owner, 2026-10-29: continuous over discrete): its picture
 *  of the choice before and of the new one crossfaded, eased. */
export const CHOICE_GLIDE_MS = 600;

/** Where a module's choice is gliding: the choice it had (its steps' values) and when it changed. */
type Glide = { key: string; last: Record<string, number>; from: Record<string, number> | null; start: number };

/** A module's choices changed: `steps`' values as they were a frame before, and how far the glide to the new ones is
 *  (0..1, eased; null when none runs). Pure: the walker's, and a test's. */
export function choiceGlide(g: Glide | undefined, steps: readonly string[], k: Record<string, number>, now: number, ms = CHOICE_GLIDE_MS): { glide: Glide; from: Record<string, number> | null; s: number } {
  const key = steps.map((s) => Math.round(k[s] ?? 0)).join('|');
  if (!g) return { glide: { key, last: { ...k }, from: null, start: now }, from: null, s: 1 };
  if (g.key !== key) {
    g.from = Object.fromEntries(steps.map((s) => [s, g.last[s] ?? 0]));
    g.start = now;
    g.key = key;
  }
  g.last = { ...k };
  const u = g.from ? (now - g.start) / ms : 1;
  if (!g.from || u >= 1) {
    g.from = null;
    return { glide: g, from: null, s: 1 };
  }
  return { glide: g, from: { ...k, ...g.from }, s: u * u * (3 - 2 * u) };
}
/** A loop drawn as one pass, by its Mix: its Feedback, the Warps and Keeps between them, the Mix's input the new
 *  picture comes in at (the other is the loop's). */
type Loop = { feedback: string; chain: { id: string; type: string }[]; now: 'in1' | 'in2' };
/** A frame's walk: its plan, the module it starts from, the output it gives on, the loops drawn as one pass. */
export type Walk = { p: Plan; from: string | null; out: PortRef; loops: Map<string, Loop>; runs: Map<string, Run>; fx: Map<string, Fx> };
/** What the effects' passes take beside their knobs: the track's time and beat, this frame's length in sixtieths of a
 *  second, the fluid (what modulates by its flow). */
export type FxFrame = { t: number; beat?: [number, number, number, number]; frames: number; fluid: Fluid | null };

const VERTEX = `#version 300 es
out vec2 vUv;
void main() {
  vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  vUv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

const SOURCES = new Set(SOURCE_TYPES.map((t) => t.type));
const LIGHTEN = MIX_MODES.indexOf('lighten');

/** The loops of `order` drawn as one pass: a Mix one of whose inputs comes from its own Feedback through Warps and
 *  Keeps only (each read by nothing else), the other kept where the brighter: the loop on its first input (today's
 *  conversion), or on its second at full over the first at full (a device's). Not one with `barrier` (the module
 *  observed) among its Warps and Keeps: that one is drawn module by module. */
export function loopsOf(order: PictureNode[], knobs: (id: string) => Record<string, number>, barrier: string | null = null): Map<string, Loop> {
  const byId = new Map(order.map((n) => [n.id, n]));
  const readers = new Map<string, number>();
  for (const n of order) for (const f of n.inputs) readers.set(f.from.module, (readers.get(f.from.module) ?? 0) + 1);
  const loops = new Map<string, Loop>();
  for (const m of order) {
    const k = knobs(m.id);
    if (m.type !== 'mix' || m.inputs.length !== 2 || !m.inputs.some((f) => f.port === 'in2') || Math.round(k.mode2 ?? 1) !== LIGHTEN) continue;
    for (const [port, now] of [['in1', 'in2'], ['in2', 'in1']] as const) {
      if (port === 'in2' && ((k.amount1 ?? 1) < 1 || (k.amount2 ?? 1) < 1)) continue;
      const chain: Loop['chain'] = [];
      let at = byId.get(m.inputs.find((f) => f.port === port)?.from.module ?? '');
      while (at && (at.type === 'warp' || at.type === 'keep') && readers.get(at.id) === 1 && at.inputs.length === 1) {
        chain.unshift({ id: at.id, type: at.type });
        at = byId.get(at.inputs[0].from.module);
      }
      if (at?.type === 'feedback' && at.inputs[0]?.from.module === m.id && chain.length && !chain.some((x) => x.id === barrier)) {
        loops.set(m.id, { feedback: at.id, chain, now });
        break;
      }
    }
  }
  return loops;
}

export class Walker {
  private gl: WebGL2RenderingContext;
  private spec: Spec = { w: 0, h: 0, format: 'rgba8' };
  private pool: Pool<Target>;
  private state: WalkState<Target> = newWalkState();
  private nodes = new Map<string, { type: string; node: ModuleNode | null }>();
  private mixes = new Map<number, WebGLProgram>();
  private copy: WebGLProgram;
  /** Two pictures crossfaded (a choice gliding). */
  private fade: WebGLProgram;
  private glides = new Map<string, Glide>();
  /** A module type's choices (its step knobs), the renderer's from the picture types: they glide. */
  steps: (type: string) => readonly string[] = () => [];
  private loop: WebGLProgram;
  private empty: WebGLVertexArrayObject;
  /** Nothing, for a loop's first frame. */
  private black: Target | null = null;
  /** The frames' time summed (the Warps' wobble rolls on with it). */
  private time = 0;
  /** What the last walk gave on (read after it): back to the pool at the next. */
  private given: Target | null = null;
  /** The effects' passes (Finish's), their programs by effects, each run's clocks. */
  private fin: Finish;
  private now: () => number;
  private fxPrograms = new Map<string, WebGLProgram | null>();
  private clocks = new Map<string, EffectClocks>();
  /** Each loop's Warp clocks (its petals' turn, its rings' way out), by its Feedback. */
  private warpClocks = new Map<string, WarpClock>();
  /** What the observed module shows this frame (`run`'s `observe`): its picture, or why it has none. */
  seen: { tex: WebGLTexture } | { why: string } | null = null;
  /** The last plan's modules at rest (each what it passes on), the effects fused into a run's pass, its Feedbacks. */
  private aliases = new Map<string, PortRef>();
  private inRuns = new Set<string>();
  private feedbacks = new Set<string>();
  private fluidField: FlowField | null = null;
  private fields: (id: string) => FlowField | null = () => null;

  constructor(gl: WebGL2RenderingContext, fin: Finish, now: () => number) {
    this.gl = gl;
    this.fin = fin;
    this.now = now;
    this.spec.format = gl.getExtension('EXT_color_buffer_float') || gl.getExtension('EXT_color_buffer_half_float') ? 'rgba16f' : 'rgba8';
    this.pool = new Pool<Target>(
      (s) => this.target(s),
      (t) => this.free(t),
    );
    this.copy = program(gl, VERTEX, `#version 300 es
precision highp float;
in vec2 vUv;
out vec4 o;
uniform sampler2D uIn;
void main() { o = texture(uIn, vUv); }`);
    this.fade = program(gl, VERTEX, `#version 300 es
precision highp float;
in vec2 vUv;
out vec4 o;
uniform sampler2D uA;
uniform sampler2D uB;
uniform float uK;
void main() { o = mix(texture(uA, vUv), texture(uB, vUv), uK); }`);
    this.loop = program(gl, VERTEX, LOOP);
    this.empty = gl.createVertexArray()!;
  }

  /** This frame's walk of `c`'s modules `ids` (the compose step's), from what module `from` drew (Show's) to what
   *  module `to` gives, under the lazy rules (`knobs`: each module's this frame); null where it draws nothing (all of
   *  it at rest: Show's picture goes on as it is). `observe`: the module observed, a barrier to fusing a pass across. */
  planned(c: Compiled, ids: ReadonlySet<string>, from: string | null, to: string, knobs: (id: string) => Record<string, number>, fxOf: (n: PictureNode) => Fx | null, observe: string | null = null): Walk | null {
    const all = c.order.filter((n) => ids.has(n.id));
    const fx = new Map(all.flatMap((n) => {
      const f = fxOf(n);
      return f ? [[n.id, f] as const] : [];
    }));
    // Each line of effects one module (effect-runs.ts), at rest where each of its effects is.
    const { runs, order } = effectRuns(all, (n) => fx.get(n.id) ?? null, observe);
    const sub: Compiled = { order, screen: to, errors: [] };
    const loops = loopsOf(sub.order, knobs, observe);
    // A loop none of whose Keeps keeps anything is gone: its Mix is the other picture.
    const kept = (l: Loop) => l.chain.some((x) => x.type === 'keep' && (knobs(x.id).amount ?? 0) > 0);
    const types = new Map(order.map((n) => [n.id, n.type]));
    const idle = (id: string) => (runs.has(id) ? runIdle(runs.get(id)!.fx.map((f) => fx.get(f)!)) : types.get(id) === 'transform' && transformIdle(knobs(id)));
    const p = plan(sub, { knobs: (id) => (loops.has(id) && !kept(loops.get(id)!) ? { ...knobs(id), [loops.get(id)!.now === 'in2' ? 'amount1' : 'amount2']: 0 } : knobs(id)), idle });
    // What it gives on, by the port the Screen (or what follows) reads it at (a Transform's is `out`).
    const port = c.order.flatMap((n) => (ids.has(n.id) ? [] : n.inputs)).find((f) => f.from.module === to)?.from.port ?? 'picture';
    let out = { module: to, port };
    for (let i = 0; p.alias.has(out.module) && i < sub.order.length; i++) out = p.alias.get(out.module)!;
    this.aliases = p.alias;
    this.inRuns = new Set([...runs].flatMap(([id, r]) => r.fx.filter((f) => f !== id)));
    this.feedbacks = new Set(order.filter((n) => n.type === 'feedback').map((n) => n.id));
    return out.module === from ? null : { p, from, out, loops, runs, fx };
  }

  /** The module whose picture `id`'s is this frame, as the last plan has it: itself, or what it passes on at rest. */
  source(id: string): string {
    let at = id;
    for (let i = 0; this.aliases.has(at) && i <= this.aliases.size; i++) at = this.aliases.get(at)!.module;
    return at;
  }

  /** The walk drawn from `picture` (`from`'s; null: nothing, black): the texture the effects or the screen read next.
   *  `observe`: the module observed, its target pinned till `unpin` (after its read). `sources`: pictures drawn
   *  outside the walk by their modules (the Music's cover). */
  run(w: Walk, given: WebGLTexture | null, width: number, height: number, knobs: (id: string) => Record<string, number>, frame: FrameInfo, effects: FxFrame, points: (id: string) => readonly Point[] | null = () => null, observe: string | null = null, sources: ReadonlyMap<string, WebGLTexture> = new Map(), fields: (id: string) => FlowField | null = () => null): WebGLTexture {
    // The flow field this frame for the modules that read one (Streamlines): a Field's cabled in, else the Fluid's.
    this.fluidField = effects.fluid ? { tex: effects.fluid.velocity, uvPerCell: effects.fluid.uvPerCell } : null;
    this.fields = fields;
    this.sized(width, height);
    if (this.given && ![...this.state.held.values()].some((h) => h.t === this.given)) this.pool.give(this.given);
    this.given = null;
    this.time += frame.dt;
    const { p, from, out, loops, runs } = w;
    this.black ??= this.target(this.spec);
    const picture = given ?? this.black.tex;
    const fused = new Set([...loops.values()].flatMap((l) => l.chain.map((x) => x.id)));
    // What the walk gives on is read after it: one reader more, so it is not given back within.
    p.readers.set(outKey(out), (p.readers.get(outKey(out)) ?? 0) + 1);
    const outside: Target = { tex: picture, fbo: null as unknown as WebGLFramebuffer };
    const drawn = new Map([...sources].map(([id, tex]) => [id, { tex, fbo: null as unknown as WebGLFramebuffer }]));
    const io = {
      spec: (n: PictureNode) => (fused.has(n.id) ? null : drawn.has(n.id) ? ('own' as const) : this.spec),
      draw: (n: PictureNode, inputs: Map<string, Target | null>, t: Target | null) => {
        if (fused.has(n.id)) return;
        if (drawn.has(n.id)) return drawn.get(n.id);
        const r = runs.get(n.id);
        const l = loops.get(n.id);
        if (r) this.fxPass(n.id, r, w.fx, inputs.get('picture')?.tex ?? picture, inputs.get('trails')?.tex ?? null, t!, effects);
        else if (l) this.loopPass(l, inputs.get(l.now)?.tex ?? picture, t!, knobs, frame);
        else this.draw(n, inputs, t!, knobs(n.id), frame, points(n.id));
      },
      copy: (a: Target, b: Target) => this.copied(a, b),
    };
    const at = observe ? this.source(observe) : null;
    const values = walk(p, this.pool, io, this.state, new Map(from ? [[from, outside]] : []), at && !fused.has(at) ? at : null);
    this.seen = at ? this.seenOf(at, values, fused) : null;
    const got = values.get(out.module) ?? null;
    if (!got || got === outside) return picture;
    this.given = got;
    return got.tex;
  }

  /** What module `at` shows after the walk: its picture (pinned, or kept anyway), or why it has none. */
  private seenOf(at: string, values: Map<string, Target | null>, fused: ReadonlySet<string>): { tex: WebGLTexture } | { why: string } {
    if (fused.has(at)) return { why: 'Fused into its loop’s pass.' };
    if (this.inRuns.has(at)) return { why: 'Fused into an effects pass.' };
    // A Feedback's picture as it holds it for the next frame (out of the pool till then).
    if (this.feedbacks.has(at)) {
      const h = this.state.held.get(at);
      return h ? { tex: h.t.tex } : { why: 'Not drawn this frame.' };
    }
    const t = values.get(at);
    return t ? { tex: t.tex } : { why: 'Not drawn this frame.' };
  }

  /** The observed module's target back to the pool, once read. */
  unpin() {
    if (this.state.pinned) this.pool.give(this.state.pinned);
    this.state.pinned = null;
  }

  /** A module drawn; a choice of its changing, drawn with the choice before too (its clocks held: no time passes for it)
   *  and the two crossfaded over CHOICE_GLIDE_MS. */
  private draw(n: PictureNode, inputs: Map<string, Target | null>, out: Target, k: Record<string, number>, frame: FrameInfo, points: readonly Point[] | null) {
    const steps = this.steps(n.type);
    const g = steps.length ? choiceGlide(this.glides.get(n.id), steps, k, this.now()) : null;
    if (g) this.glides.set(n.id, g.glide);
    if (!g?.from) return this.drawOnce(n, inputs, out, k, frame, points);
    const [now, before] = [this.pool.take(this.spec), this.pool.take(this.spec)];
    this.drawOnce(n, inputs, now, k, frame, points);
    this.drawOnce(n, inputs, before, g.from, { ...frame, dt: 0 }, points);
    this.faded(before, now, out, g.s);
    this.pool.give(now);
    this.pool.give(before);
  }

  /** `a` into `b` by `k`, into `out`. */
  private faded(a: Target, b: Target, out: Target, k: number) {
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, out.fbo);
    gl.viewport(0, 0, this.spec.w, this.spec.h);
    gl.disable(gl.BLEND);
    gl.useProgram(this.fade);
    [a, b].forEach((t, i) => {
      gl.activeTexture(gl.TEXTURE0 + i);
      gl.bindTexture(gl.TEXTURE_2D, t.tex);
    });
    gl.uniform1i(gl.getUniformLocation(this.fade, 'uA'), 0);
    gl.uniform1i(gl.getUniformLocation(this.fade, 'uB'), 1);
    gl.uniform1f(gl.getUniformLocation(this.fade, 'uK'), k);
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
    gl.activeTexture(gl.TEXTURE0);
  }

  private drawOnce(n: PictureNode, inputs: Map<string, Target | null>, out: Target, k: Record<string, number>, frame: FrameInfo, points: readonly Point[] | null) {
    const gl = this.gl;
    const { w, h } = this.spec;
    if (n.type === 'mix') return this.mix(inputs, out, k);
    const node = this.node(n.id, n.type);
    // One that cannot be drawn here gives nothing, not what its target held last.
    if (!node) {
      gl.bindFramebuffer(gl.FRAMEBUFFER, out.fbo);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      return;
    }
    node.draw(gl, { picture: (inputs.get('picture') ?? inputs.get('in'))?.tex ?? null, by: inputs.get('by')?.tex ?? null, field: this.fields(n.id) ?? this.fluidField, windows: [inputs.get('window2')?.tex ?? null, inputs.get('window3')?.tex ?? null], along: inputs.get('along')?.tex ?? null }, out.fbo, w, h, k, withInputs(n.type, frame), points);
  }

  /** A run of effects in one pass (Finish's), from `input`, its Echo trailing `trails` (its visible Feedback's; none:
   *  nothing); into `out`. */
  private fxPass(id: string, r: Run, fx: Map<string, Fx>, input: WebGLTexture, trails: WebGLTexture | null, out: Target, f: FxFrame) {
    const pass = passOf(r.fx.map((m) => fx.get(m)!));
    const key = pass.ids.join(',');
    if (!this.fxPrograms.has(key)) {
      const source = pictureShader(pass.ids, true);
      let p: WebGLProgram | null = null;
      try {
        p = source ? program(this.gl, FULL, source) : null;
      } catch (e) {
        console.warn('Visuals: an effects pass did not compile here', pass.ids, e);
      }
      this.fxPrograms.set(key, p);
      this.fin.madeAt = this.now();
    }
    const p = this.fxPrograms.get(key);
    let clocks = this.clocks.get(id);
    if (!clocks) this.clocks.set(id, (clocks = new EffectClocks()));
    const effects = { ...pass, loop: 0, loopAmount: 0, t: f.t, beat: f.beat };
    clocks.run(effects);
    const prev = trails ?? this.black!.tex;
    if (p) this.fin.finish(p, effects, { tex: [prev, out.tex], fbo: [out.fbo, out.fbo], w: this.spec.w, h: this.spec.h, cur: 0, last: 0 }, input, clocks, f.frames, f.fluid);
    else this.copied({ tex: input, fbo: null as unknown as WebGLFramebuffer }, out);
  }

  /** A loop in one pass: its last picture (its Feedback's) read where its Warps (summed) move it from, kept by its
   *  Keeps' shares (multiplied), and `now` over it, the brighter of the two. */
  private loopPass(l: Loop, now: WebGLTexture, out: Target, knobs: (id: string) => Record<string, number>, frame: FrameInfo) {
    const gl = this.gl;
    const dt = frame.dt;
    let keep = 1;
    const warps: Record<string, number>[] = [];
    for (const { id, type } of l.chain) {
      const k = knobs(id);
      const a = k.amount ?? 0;
      if (type === 'keep') {
        if (a > 0) keep *= 1 - a + a * keptPerSecond(k.trails ?? 0.5) ** dt;
        continue;
      }
      warps.push(k);
    }
    // The loop's clocks (its Warps' petals and rings), its own, run on at the last Warp's speeds.
    let clock = this.warpClocks.get(l.feedback);
    if (!clock) this.warpClocks.set(l.feedback, (clock = new WarpClock()));
    if (warps.length) clock.run(warps[warps.length - 1], dt);
    const w = warpFrame(warps, dt, this.spec.w / this.spec.h, this.time, clock);
    const last = this.state.held.get(l.feedback)?.t ?? this.black!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, out.fbo);
    gl.viewport(0, 0, this.spec.w, this.spec.h);
    gl.disable(gl.BLEND);
    gl.bindVertexArray(null);
    gl.useProgram(this.loop);
    const u = (n: string) => gl.getUniformLocation(this.loop, n);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, now);
    gl.uniform1i(u('uNow'), 0);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, last.tex);
    gl.uniform1i(u('uLast'), 1);
    gl.activeTexture(gl.TEXTURE0);
    gl.uniform1f(u('uAspect'), this.spec.w / this.spec.h);
    gl.uniform2f(u('uDrift'), w.drift[0], w.drift[1]);
    for (const [n, v] of Object.entries(warpUniforms(w))) {
      if (typeof v === 'number') gl.uniform1f(u(n), v);
      else if (v.length === 2) gl.uniform2f(u(n), v[0], v[1]);
      else if (v.length === 3) gl.uniform3f(u(n), v[0], v[1], v[2]);
      else gl.uniform4f(u(n), v[0], v[1], v[2], v[3]);
    }
    gl.uniform1f(u('uKeep'), keep);
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
  }

  /** Mix in one pass: the first input under the rest (its amount its share), each next by its mode and amount. */
  private mix(inputs: Map<string, Target | null>, out: Target, k: Record<string, number>) {
    const gl = this.gl;
    const ins = Array.from({ length: MIX_INPUTS }, (_, i) => ({ i: i + 1, t: inputs.get(`in${i + 1}`) ?? null }));
    const first = ins[0].t;
    const rest = ins.slice(1).filter((x) => x.t);
    let p = this.mixes.get(rest.length);
    if (!p) this.mixes.set(rest.length, (p = program(gl, VERTEX, mixShader(rest.length))));
    gl.bindFramebuffer(gl.FRAMEBUFFER, out.fbo);
    gl.viewport(0, 0, this.spec.w, this.spec.h);
    gl.disable(gl.BLEND);
    gl.useProgram(p);
    const bind = (unit: number, t: WebGLTexture | null, name: string) => {
      gl.activeTexture(gl.TEXTURE0 + unit);
      gl.bindTexture(gl.TEXTURE_2D, t);
      gl.uniform1i(gl.getUniformLocation(p!, name), unit);
    };
    // Without a first, the rest over nothing.
    bind(0, first?.tex ?? rest[0]?.t?.tex ?? none(gl), 'uBase');
    // The first's share of the picture (a morph may run past 1 or below 0 for a frame).
    gl.uniform1f(gl.getUniformLocation(p, 'uBaseGain'), first ? Math.min(1, Math.max(0, k.amount1 ?? 1)) : 0);
    rest.forEach(({ i, t }, j) => {
      bind(1 + j, t!.tex, `uLayer${j}`);
      gl.uniform1i(gl.getUniformLocation(p!, `uMode${j}`), Math.round(k[`mode${i}`] ?? 1));
      gl.uniform1f(gl.getUniformLocation(p!, `uAmount${j}`), k[`amount${i}`] ?? 1);
    });
    gl.activeTexture(gl.TEXTURE0);
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
  }

  /** A Feedback's copy of a picture it does not own. */
  private copied(from: Target, to: Target) {
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, to.fbo);
    gl.viewport(0, 0, this.spec.w, this.spec.h);
    gl.disable(gl.BLEND);
    gl.useProgram(this.copy);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, from.tex);
    gl.uniform1i(gl.getUniformLocation(this.copy, 'uIn'), 0);
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
  }

  /** The running node of module `id` (made on first use; none for one that cannot be made here). */
  private node(id: string, type: string): ModuleNode | null {
    let r = this.nodes.get(id);
    if (!r || r.type !== type) {
      r?.node?.dispose(this.gl);
      let node: ModuleNode | null = null;
      try {
        const source = SOURCES.has(type) ? createSource(type, this.gl) : null;
        node = source ? nodeOf(source) : type in MODULE_MAKERS ? nodeOf(MODULE_MAKERS[type](this.gl)) : type in POINTS_NODES ? POINTS_NODES[type](this.gl) : PICTURE_NODES.has(type) ? createNode(type, this.gl) : null;
      } catch (e) {
        console.warn('Visuals: a module could not be made here', type, e);
      }
      r = { type, node };
      this.nodes.set(id, r);
    }
    return r.node;
  }

  /** Lets go of every Feedback's last picture (the loops start from nothing). */
  forget() {
    this.unpin();
    for (const h of this.state.held.values()) if (h.pooled) this.pool.give(h.t);
    this.clocks.clear();
    if (this.given) this.pool.give(this.given);
    this.state = newWalkState();
    this.seen = null;
    this.given = null;
  }

  /** Lets go of the nodes of modules no longer in the patch. */
  prune(ids: ReadonlySet<string>) {
    for (const [id, r] of this.nodes) if (!ids.has(id)) (r.node?.dispose(this.gl), this.nodes.delete(id));
    for (const id of this.glides.keys()) if (!ids.has(id)) this.glides.delete(id);
  }

  /** At a new size every target goes, and the Feedbacks start from nothing. */
  private sized(width: number, height: number) {
    if (width === this.spec.w && height === this.spec.h) return;
    this.pool.clear();
    if (this.black) this.free(this.black);
    this.black = null;
    this.state = newWalkState();
    this.seen = null;
    this.given = null;
    this.spec = { ...this.spec, w: width, h: height };
  }

  private target(s: Spec): Target {
    const gl = this.gl;
    const tex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, tex);
    if (s.format === 'rgba16f') gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA16F, s.w, s.h, 0, gl.RGBA, gl.HALF_FLOAT, null);
    else gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, s.w, s.h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.MIRRORED_REPEAT);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.MIRRORED_REPEAT);
    const fbo = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.bindTexture(gl.TEXTURE_2D, null);
    return { tex, fbo };
  }

  private free(t: Target) {
    this.gl.deleteTexture(t.tex);
    this.gl.deleteFramebuffer(t.fbo);
  }

  dispose() {
    for (const r of this.nodes.values()) r.node?.dispose(this.gl);
    this.pool.clear();
    for (const p of this.mixes.values()) this.gl.deleteProgram(p);
    this.gl.deleteProgram(this.copy);
    this.gl.deleteProgram(this.fade);
    this.gl.deleteProgram(this.loop);
    for (const p of this.fxPrograms.values()) if (p) this.gl.deleteProgram(p);
    if (this.black) this.free(this.black);
    this.gl.deleteVertexArray(this.empty);
  }
}
