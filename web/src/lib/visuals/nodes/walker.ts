// The compose step as the graph's own walk (decision 0057 §4; step 3): every picture module between Show and the
// effects a node, whatever the patch wires (a device's Glow, Tint, Threshold, Blur…): the sources, the layers'
// modules and engines, Glow, Tint, Mask, Displace, Threshold, Blur and Bend on pictures, Warp, Keep, Mix (in one
// pass), Feedback. Drawn in the graph's order under the lazy rules (picture-graph.ts plan), each into a target of the
// pool given back once its last reader has drawn, a Feedback adopting its input's target (walk). The converted chain
// keeps compose.ts's fused passes; this draws the rest.
import { MODULE_MAKERS, mixShader } from '../compose';
import { MIX_INPUTS } from '../ports';
import type { FrameInfo } from '../patch';
import { newWalkState, outKey, plan, Pool, walk, type Compiled, type PictureNode, type Plan, type Spec, type WalkState } from '../picture-graph';
import type { PortRef } from '../ports';
import { createSource, SOURCE_TYPES } from '../sources/index';
import { program } from '../sources/gl';
import { createNode, nodeOf, PICTURE_NODES, type ModuleNode } from './modules';

type Target = { tex: WebGLTexture; fbo: WebGLFramebuffer };
/** A frame's walk: its plan, the module it starts from, the output it gives on. */
export type Walk = { p: Plan; from: string; out: PortRef };

const VERTEX = `#version 300 es
out vec2 vUv;
void main() {
  vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  vUv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

const SOURCES = new Set(SOURCE_TYPES.map((t) => t.type));

/** Whether a module of `type` is drawn by the walk (a source, a module, a Mix or a Feedback). */
export const walked = (type: string) => SOURCES.has(type) || type in MODULE_MAKERS || PICTURE_NODES.has(type) || type === 'mix' || type === 'feedback';

export class Walker {
  private gl: WebGL2RenderingContext;
  private spec: Spec = { w: 0, h: 0, format: 'rgba8' };
  private pool: Pool<Target>;
  private state: WalkState<Target> = newWalkState();
  private nodes = new Map<string, { type: string; node: ModuleNode | null }>();
  private mixes = new Map<number, WebGLProgram>();
  private copy: WebGLProgram;
  private empty: WebGLVertexArrayObject;
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
    this.empty = gl.createVertexArray()!;
  }

  /** This frame's walk of `c`'s modules `ids` (the compose step's), from what module `from` drew (Show's) to what
   *  module `to` gives, under the lazy rules (`knobs`: each module's this frame); null where it draws nothing (all of
   *  it at rest: Show's picture goes on as it is). */
  planned(c: Compiled, ids: ReadonlySet<string>, from: string, to: string, knobs: (id: string) => Record<string, number>): Walk | null {
    const sub: Compiled = { order: c.order.filter((n) => ids.has(n.id)), screen: to, errors: [] };
    const p = plan(sub, { knobs });
    let out = { module: to, port: 'picture' };
    for (let i = 0; p.alias.has(out.module) && i < sub.order.length; i++) out = p.alias.get(out.module)!;
    return out.module === from ? null : { p, from, out };
  }

  /** The walk drawn from `picture` (`from`'s): the texture the effects or the screen read next. */
  run(w: Walk, picture: WebGLTexture, width: number, height: number, knobs: (id: string) => Record<string, number>, frame: FrameInfo): WebGLTexture {
    const gl = this.gl;
    this.sized(width, height);
    if (this.given && ![...this.state.held.values()].some((h) => h.t === this.given)) this.pool.give(this.given);
    this.given = null;
    const { p, from, out } = w;
    // What the walk gives on is read after it: one reader more, so it is not given back within.
    p.readers.set(outKey(out), (p.readers.get(outKey(out)) ?? 0) + 1);
    const outside: Target = { tex: picture, fbo: null as unknown as WebGLFramebuffer };
    const values = walk(p, this.pool, { spec: () => this.spec, draw: (n, inputs, t) => this.draw(n, inputs, t!, knobs(n.id), frame), copy: (a, b) => this.copied(a, b) }, this.state, new Map([[from, outside]]));
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
    node.draw(gl, { picture: inputs.get('picture')?.tex ?? null, by: inputs.get('by')?.tex ?? null }, out.fbo, w, h, k, frame);
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
    gl.uniform1f(gl.getUniformLocation(p, 'uBaseGain'), first ? (k.amount1 ?? 1) : 0);
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

  /** Lets go of the nodes of modules no longer in the patch. */
  prune(ids: ReadonlySet<string>) {
    for (const [id, r] of this.nodes) if (!ids.has(id)) (r.node?.dispose(this.gl), this.nodes.delete(id));
  }

  /** At a new size every target goes, and the Feedbacks start from nothing. */
  private sized(width: number, height: number) {
    if (width === this.spec.w && height === this.spec.h) return;
    this.pool.clear();
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
    this.gl.deleteVertexArray(this.empty);
  }
}
