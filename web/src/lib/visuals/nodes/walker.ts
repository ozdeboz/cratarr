// The compose step as the graph's own walk (decision 0057 §4; step 3): every picture module between Show and the
// effects a node, whatever the patch wires: the sources, the layers' modules and engines, Glow, Tint, Mask, Displace,
// Threshold, Blur and Bend on pictures, Warp, Keep, Mix (in one pass), Feedback. Drawn in the graph's order under the
// lazy rules (picture-graph.ts plan), each into a target of the pool given back once its last reader has drawn, a
// Feedback adopting its input's target (walk). A loop of Feedback → Warps and Keeps → a Mix keeping the brighter with
// another picture is drawn as one pass (compose.ts LOOP: MilkDrop's feedback).
import { LOOP, MODULE_MAKERS, mixShader, withInputs } from '../compose';
import { keptPerSecond } from '../loopmods';
import { MIX_INPUTS, MIX_MODES } from '../ports';
import type { FrameInfo } from '../patch';
import { newWalkState, outKey, plan, Pool, walk, type Compiled, type PictureNode, type Plan, type Spec, type WalkState } from '../picture-graph';
import type { PortRef } from '../ports';
import { createSource, SOURCE_TYPES } from '../sources/index';
import { program } from '../sources/gl';
import { createNode, nodeOf, PICTURE_NODES, type ModuleNode } from './modules';

type Target = { tex: WebGLTexture; fbo: WebGLFramebuffer };
/** A loop drawn as one pass, by its Mix: its Feedback, and the Warps and Keeps between them. */
type Loop = { feedback: string; chain: { id: string; type: string }[] };
/** A frame's walk: its plan, the module it starts from, the output it gives on, the loops drawn as one pass. */
export type Walk = { p: Plan; from: string | null; out: PortRef; loops: Map<string, Loop> };

const VERTEX = `#version 300 es
out vec2 vUv;
void main() {
  vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  vUv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

const SOURCES = new Set(SOURCE_TYPES.map((t) => t.type));
const LIGHTEN = MIX_MODES.indexOf('lighten');

/** The loops of `order` drawn as one pass: a Mix whose first input comes from its own Feedback through Warps and Keeps
 *  only (each read by nothing else), its second kept where the brighter. */
export function loopsOf(order: PictureNode[], knobs: (id: string) => Record<string, number>): Map<string, Loop> {
  const byId = new Map(order.map((n) => [n.id, n]));
  const readers = new Map<string, number>();
  for (const n of order) for (const f of n.inputs) readers.set(f.from.module, (readers.get(f.from.module) ?? 0) + 1);
  const loops = new Map<string, Loop>();
  for (const m of order) {
    if (m.type !== 'mix' || m.inputs.length !== 2 || !m.inputs.some((f) => f.port === 'in2') || Math.round(knobs(m.id).mode2 ?? 1) !== LIGHTEN) continue;
    const chain: Loop['chain'] = [];
    let at = byId.get(m.inputs.find((f) => f.port === 'in1')?.from.module ?? '');
    while (at && (at.type === 'warp' || at.type === 'keep') && readers.get(at.id) === 1 && at.inputs.length === 1) {
      chain.unshift({ id: at.id, type: at.type });
      at = byId.get(at.inputs[0].from.module);
    }
    if (at?.type === 'feedback' && at.inputs[0]?.from.module === m.id && chain.length) loops.set(m.id, { feedback: at.id, chain });
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
  private loop: WebGLProgram;
  private empty: WebGLVertexArrayObject;
  /** Nothing, for a loop's first frame. */
  private black: Target | null = null;
  /** The frames' time summed (the Warps' wobble rolls on with it). */
  private time = 0;
  /** What the last walk gave on (read after it): back to the pool at the next. */
  private given: Target | null = null;

  constructor(gl: WebGL2RenderingContext) {
    this.gl = gl;
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
    this.loop = program(gl, VERTEX, LOOP);
    this.empty = gl.createVertexArray()!;
  }

  /** This frame's walk of `c`'s modules `ids` (the compose step's), from what module `from` drew (Show's) to what
   *  module `to` gives, under the lazy rules (`knobs`: each module's this frame); null where it draws nothing (all of
   *  it at rest: Show's picture goes on as it is). */
  planned(c: Compiled, ids: ReadonlySet<string>, from: string | null, to: string, knobs: (id: string) => Record<string, number>): Walk | null {
    const sub: Compiled = { order: c.order.filter((n) => ids.has(n.id)), screen: to, errors: [] };
    const loops = loopsOf(sub.order, knobs);
    // A loop none of whose Keeps keeps anything is gone: its Mix is the other picture.
    const kept = (l: Loop) => l.chain.some((x) => x.type === 'keep' && (knobs(x.id).amount ?? 0) > 0);
    const p = plan(sub, { knobs: (id) => (loops.has(id) && !kept(loops.get(id)!) ? { ...knobs(id), amount1: 0 } : knobs(id)) });
    let out = { module: to, port: 'picture' };
    for (let i = 0; p.alias.has(out.module) && i < sub.order.length; i++) out = p.alias.get(out.module)!;
    return out.module === from ? null : { p, from, out, loops };
  }

  /** The walk drawn from `picture` (`from`'s; null: nothing, black): the texture the effects or the screen read next. */
  run(w: Walk, given: WebGLTexture | null, width: number, height: number, knobs: (id: string) => Record<string, number>, frame: FrameInfo): WebGLTexture {
    this.sized(width, height);
    if (this.given && ![...this.state.held.values()].some((h) => h.t === this.given)) this.pool.give(this.given);
    this.given = null;
    this.time += frame.dt;
    const { p, from, out, loops } = w;
    this.black ??= this.target(this.spec);
    const picture = given ?? this.black.tex;
    const fused = new Set([...loops.values()].flatMap((l) => l.chain.map((x) => x.id)));
    // What the walk gives on is read after it: one reader more, so it is not given back within.
    p.readers.set(outKey(out), (p.readers.get(outKey(out)) ?? 0) + 1);
    const outside: Target = { tex: picture, fbo: null as unknown as WebGLFramebuffer };
    const io = {
      spec: (n: PictureNode) => (fused.has(n.id) ? null : this.spec),
      draw: (n: PictureNode, inputs: Map<string, Target | null>, t: Target | null) => {
        if (fused.has(n.id)) return;
        const l = loops.get(n.id);
        if (l) return this.loopPass(l, inputs.get('in2')?.tex ?? picture, t!, knobs, frame);
        this.draw(n, inputs, t!, knobs(n.id), frame);
      },
      copy: (a: Target, b: Target) => this.copied(a, b),
    };
    const values = walk(p, this.pool, io, this.state, new Map(from ? [[from, outside]] : []));
    const got = values.get(out.module) ?? null;
    if (!got || got === outside) return picture;
    this.given = got;
    return got.tex;
  }

  private draw(n: PictureNode, inputs: Map<string, Target | null>, out: Target, k: Record<string, number>, frame: FrameInfo) {
    const gl = this.gl;
    const { w, h } = this.spec;
    if (n.type === 'mix') return this.mix(inputs, out, k);
    const node = this.node(n.id, n.type);
    if (!node) return;
    node.draw(gl, { picture: inputs.get('picture')?.tex ?? null, by: inputs.get('by')?.tex ?? null }, out.fbo, w, h, k, withInputs(n.type, frame));
  }

  /** A loop in one pass: its last picture (its Feedback's) read where its Warps (summed) move it from, kept by its
   *  Keeps' shares (multiplied), and `now` over it, the brighter of the two. */
  private loopPass(l: Loop, now: WebGLTexture, out: Target, knobs: (id: string) => Record<string, number>, frame: FrameInfo) {
    const gl = this.gl;
    const dt = frame.dt;
    let keep = 1;
    let zoom = 0, turn = 0, dx = 0, dy = 0, stretch = 0, wobble = 0, size = 0.5;
    for (const { id, type } of l.chain) {
      const k = knobs(id);
      const a = k.amount ?? 0;
      if (type === 'keep') {
        if (a > 0) keep *= 1 - a + a * keptPerSecond(k.trails ?? 0.5) ** dt;
        continue;
      }
      zoom += a * (k.zoom ?? 0);
      turn += a * (k.turn ?? 0);
      dx += a * (k.driftX ?? 0);
      dy += a * (k.driftY ?? 0);
      stretch += a * (k.stretch ?? 0);
      wobble += a * (k.wobble ?? 0);
      size = k.wobbleSize ?? size;
    }
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
    // A second's worth at full: zoom half again (in or out), half a turn, a fifth of the picture across.
    gl.uniform1f(u('uZoom'), Math.exp(0.4 * zoom * dt));
    gl.uniform1f(u('uTurn'), Math.PI * turn * dt);
    gl.uniform2f(u('uDrift'), 0.2 * dx * dt, 0.2 * dy * dt);
    gl.uniform1f(u('uStretch'), Math.exp(0.2 * stretch * dt));
    gl.uniform3f(u('uWobble'), 0.02 * wobble * Math.min(1, dt * 30), 4 + 20 * size, this.time * 1.7);
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
    bind(0, first?.tex ?? rest[0]?.t?.tex ?? null, 'uBase');
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
        node = source ? nodeOf(source) : type in MODULE_MAKERS ? nodeOf(MODULE_MAKERS[type](this.gl)) : PICTURE_NODES.has(type) ? createNode(type, this.gl) : null;
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
    for (const h of this.state.held.values()) if (h.pooled) this.pool.give(h.t);
    if (this.given) this.pool.give(this.given);
    this.state = newWalkState();
    this.given = null;
  }

  /** Lets go of the nodes of modules no longer in the patch. */
  prune(ids: ReadonlySet<string>) {
    for (const [id, r] of this.nodes) if (!ids.has(id)) (r.node?.dispose(this.gl), this.nodes.delete(id));
  }

  /** At a new size every target goes, and the Feedbacks start from nothing. */
  private sized(width: number, height: number) {
    if (width === this.spec.w && height === this.spec.h) return;
    this.pool.clear();
    if (this.black) this.free(this.black);
    this.black = null;
    this.state = newWalkState();
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
    this.gl.deleteProgram(this.loop);
    if (this.black) this.free(this.black);
    this.gl.deleteVertexArray(this.empty);
  }
}
