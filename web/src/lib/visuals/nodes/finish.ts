// The effects' pass (decision 0057 step 3: a run of effect modules in the walk, nodes/effect-runs.ts): one pass of a
// run's program from its picture, its Echo's trail and the fluid's flow into its target. And the picture Show draws
// the walk from, at the light's size: made when a walk first asks, let go of ten seconds after the last.
import { type EffectClocks, MOST_PICTURE } from '../effects';
import type { Fluid } from '../fluid';
import type { EffectsParams } from '../stage-values';
import { none, uniforms } from './shared';

/** A pass's targets: what it reads as the last picture (0) and what it draws into (1), at `w` x `h`. */
export type FxTargets = { tex: WebGLTexture[]; fbo: WebGLFramebuffer[]; w: number; h: number; cur: number; last: number };
/** The picture Show draws, at the buffer's size, and when a walk last asked for it. */
export type Shown = { tex: WebGLTexture; fbo: WebGLFramebuffer; w: number; h: number; last: number };

export class Finish {
  private gl: WebGL2RenderingContext;
  private shown: Shown | null = null;
  /** When an effects pass's program was last made (making it makes a frame or two late). */
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

  /** The picture Show draws, at the buffer's size (`width` x `height`). */
  targets(width: number, height: number): Shown {
    const gl = this.gl;
    if (this.shown && (this.shown.w !== width || this.shown.h !== height)) this.freeFx();
    if (!this.shown) {
      const tex = gl.createTexture()!;
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      // Read past its edge (a kaleidoscope reaching into the corners), the picture is mirrored, not smeared.
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.MIRRORED_REPEAT);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.MIRRORED_REPEAT);
      const fbo = gl.createFramebuffer()!;
      gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
      gl.bindTexture(gl.TEXTURE_2D, null);
      this.shown = { tex, fbo, w: width, h: height, last: 0 };
    }
    this.shown.last = this.now();
    return this.shown;
  }

  freeFx() {
    if (!this.shown) return;
    this.gl.deleteTexture(this.shown.tex);
    this.gl.deleteFramebuffer(this.shown.fbo);
    this.shown = null;
  }

  /** Lets go of Show's picture ten seconds after a walk last asked for it. */
  idle() {
    if (this.shown && this.now() - this.shown.last > 10_000) this.freeFx();
  }

  /** An effects pass (`p`, effects.ts pictureShader) from the picture (and the last one, for echoes, and the flow, for
   *  what modulates by it) into the next, which it gives. */
  finish(p: WebGLProgram, fx: NonNullable<EffectsParams>, into: FxTargets, picture: WebGLTexture, clocks: EffectClocks, frames: number, fluid: Fluid | null): WebGLTexture {
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
    gl.bindTexture(gl.TEXTURE_2D, fluid ? fluid.velocity : none(gl));
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
  }
}
