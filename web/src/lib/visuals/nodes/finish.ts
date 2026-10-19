// The picture's effects after the light is shown (decision 0057 step 3: the effect modules, fused as today's chain):
// one pass of the chain's program from the picture, the last finished one (Echo) and the fluid's flow into the next
// finished one; its targets at the light's size, made when a chain first asks and let go of ten seconds after the last.
import { type EffectClocks, MOST_PICTURE, pictureShader } from '../effects';
import type { Fluid } from '../fluid';
import { program } from '../sources/gl';
import type { Shown } from '../render';
import { FULL, uniforms } from './shared';

/** The effects stage's targets: the two finished pictures and the picture (2), the buffer's size. */
export type FxTargets = { tex: WebGLTexture[]; fbo: WebGLFramebuffer[]; w: number; h: number; cur: number; last: number };

export class Finish {
  private gl: WebGL2RenderingContext;
  /** The chains' programs by their effects. */
  private fxPrograms = new Map<string, WebGLProgram | null>();
  private fx: FxTargets | null = null;
  /** When a chain's program was last made (making it makes a frame or two late). */
  madeAt = -Infinity;
  private empty: WebGLVertexArrayObject;
  private now: () => number;
  private u: (p: WebGLProgram, name: string) => WebGLUniformLocation | null;

  constructor(gl: WebGL2RenderingContext, now: () => number) {
    this.gl = gl;
    this.now = now;
    this.u = uniforms(gl);
    this.empty = gl.createVertexArray()!;
  }

  /** The picture chain's program for these effects, or null where none would show (none set, all at nothing, or one
   *  that would not compile here: then the stage stands aside for good for that chain). */
  chainFor(fx: Shown['effects'], clocks: EffectClocks): WebGLProgram | null {
    if (!fx || !(fx.amounts.some((a, k) => a > 0 && fx.ids[k]) || clocks.holding())) return null;
    const ids = fx.ids.slice(0, MOST_PICTURE);
    const key = ids.join(',');
    if (!this.fxPrograms.has(key)) {
      const source = pictureShader(ids);
      let p: WebGLProgram | null = null;
      try {
        p = source ? program(this.gl, FULL, source) : null;
      } catch (e) {
        console.warn('Visuals: an effects chain did not compile here', ids, e);
      }
      this.fxPrograms.set(key, p);
      this.madeAt = this.now();
    }
    return this.fxPrograms.get(key) ?? null;
  }

  /** The stage's targets at the buffer's size (`width` x `height`): two finished pictures (one written, one the last) and the picture. */
  targets(width: number, height: number): FxTargets {
    const gl = this.gl;
    if (this.fx && (this.fx.w !== width || this.fx.h !== height)) this.freeFx();
    if (!this.fx) {
      const tex: WebGLTexture[] = [];
      const fbo: WebGLFramebuffer[] = [];
      for (let i = 0; i < 3; i++) {
        const t = gl.createTexture()!;
        gl.bindTexture(gl.TEXTURE_2D, t);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
        // Read past its edge (a kaleidoscope reaching into the corners), the picture is mirrored, not smeared.
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.MIRRORED_REPEAT);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.MIRRORED_REPEAT);
        const f = gl.createFramebuffer()!;
        gl.bindFramebuffer(gl.FRAMEBUFFER, f);
        gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, t, 0);
        tex.push(t);
        fbo.push(f);
      }
      gl.bindTexture(gl.TEXTURE_2D, null);
      this.fx = { tex, fbo, w: width, h: height, cur: 0, last: 0 };
    }
    this.fx.last = this.now();
    return this.fx;
  }

  freeFx() {
    const gl = this.gl;
    if (!this.fx) return;
    for (const t of this.fx.tex) gl.deleteTexture(t);
    for (const f of this.fx.fbo) gl.deleteFramebuffer(f);
    this.fx = null;
  }

  /** Lets go of the targets ten seconds after a chain last asked for them. */
  idle() {
    if (this.fx && this.now() - this.fx.last > 10_000) this.freeFx();
  }

  /** The picture slot after the show pass: the chain's pass from the picture (and the last finished one, for echoes,
   *  and the flow, for what modulates by it) into the next finished one, which it gives. */
  finish(p: WebGLProgram, fx: NonNullable<Shown['effects']>, into: FxTargets, picture: WebGLTexture, clocks: EffectClocks, frames: number, fluid: Fluid | null): WebGLTexture {
    const gl = this.gl;
    const next = 1 - into.cur;
    gl.bindFramebuffer(gl.FRAMEBUFFER, into.fbo[next]);
    gl.viewport(0, 0, into.w, into.h);
    gl.useProgram(p);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, picture);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, into.tex[into.cur]);
    gl.activeTexture(gl.TEXTURE2);
    gl.bindTexture(gl.TEXTURE_2D, fluid ? fluid.velocity : null);
    gl.activeTexture(gl.TEXTURE0);
    const u = (n: string) => this.u(p, n);
    gl.uniform1i(u('uPicture'), 0);
    gl.uniform1i(u('uPrev'), 1);
    gl.uniform1i(u('uFlow'), 2);
    gl.uniform2f(u('uFlowScale'), fluid ? fluid.uvPerCell[0] : 0, fluid ? fluid.uvPerCell[1] : 0);
    gl.uniform2f(u('uView'), into.w, into.h);
    gl.uniform1f(u('uAspect'), into.w / into.h);
    gl.uniform1fv(u('uFxA'), Float32Array.from({ length: MOST_PICTURE }, (_, k) => fx.amounts[k] ?? 0));
    gl.uniform2fv(u('uFxT'), Float32Array.from({ length: MOST_PICTURE * 2 }, (_, i) => clocks.times[i >> 1][i & 1]));
    gl.uniform1f(u('uFxFrames'), frames);
    gl.uniform2fv(u('uFxK'), Float32Array.from({ length: MOST_PICTURE * 2 }, (_, i) => fx.knobs?.[i >> 1]?.[i & 1] ?? 0.5));
    gl.uniform4f(u('uFxBeat'), ...(fx.beat ?? ([0, 1000, 0, 0] as const)));
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    into.cur = next;
    gl.bindVertexArray(null);
    return into.tex[next];
  }

  dispose() {
    this.freeFx();
    this.gl.deleteVertexArray(this.empty);
    for (const p of this.fxPrograms.values()) if (p) this.gl.deleteProgram(p);
  }
}
