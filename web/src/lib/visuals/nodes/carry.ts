// The flame's own loop on its light (decision 0057 step 3: the carry modules, Trails, Tunnel, the Loop effect and
// Fluid, as one pass): the light kept from one frame to the next, faded, read nearer the middle or by the loop's
// effect, or carried along the fluid's flow (fluid.ts) as it fades, into the other set of the light's buffers.
import { FLOW_IN_LIGHT, Fluid, gridFor, type Loop } from '../fluid';
import { loopFunction } from '../effects';
import { program } from '../sources/gl';
import type { CarryParams } from '../stage-values';
import type { Light } from './light';
import { FULL, uniforms } from './shared';

/** The fluid as shown follows the Fluid setting (as its routes move it) over these (s) rising and falling, and is let go
 *  under the least (of the fluid's own: a fiftieth of Fluid's whole range). */
const FLUID_RISE = 0.3;
const FLUID_FALL = 1.5;
const FLUID_LEAST = 0.002;

const FADE = `#version 300 es
precision mediump float;
uniform float uKeep;
layout(location = 0) out vec4 o;
layout(location = 1) out vec4 bandA;
layout(location = 2) out vec4 bandB;
void main() { o = vec4(0.0, 0.0, 0.0, uKeep); bandA = o; bandB = o; }`;

// The tunnel (zoom.ts), where nothing flows: the light kept, read nearer the middle (drawn larger), in place of the fade.
const tunnelSource = (loop: string) => `#version 300 es
precision highp float;
uniform sampler2D uAcc;
uniform sampler2D uBandsA;
uniform sampler2D uBandsB;
uniform vec2 uSize;
uniform float uZoom;
uniform float uKeep;
uniform float uLoopA;
uniform vec4 uLoopTime;
uniform float uLoopFrames;
uniform vec2 uLoopK;
layout(location = 0) out vec4 o;
layout(location = 1) out vec4 bandA;
layout(location = 2) out vec4 bandB;
bool bad(float x) { return (floatBitsToUint(x) & 0x7f800000u) == 0x7f800000u; }
vec4 kept(vec4 v) { return bad(dot(v, vec4(1.0))) ? vec4(0.0) : v * uKeep; }
${loop}
void main() {
  vec2 at = 0.5 + (gl_FragCoord.xy / uSize - 0.5) * uZoom;
  at = fx_loop(at, uLoopA, uLoopTime, uLoopFrames, uSize.x / uSize.y, uLoopK);
  o = kept(textureLod(uAcc, at, 0.0));
  bandA = kept(textureLod(uBandsA, at, 0.0));
  bandB = kept(textureLod(uBandsB, at, 0.0));
}`;

/** The loop's effect this frame. */
export type LoopNow = { id: number; amount: number; time: Loop['time']; frames: number; knobs?: readonly [number, number] };

export class Carry {
  private gl: WebGL2RenderingContext;
  private fade: WebGLProgram;
  /** The carry pass (the tunnel, the loop slot) by its loop effect's id (0: none). */
  private carrying2 = new Map<number, WebGLProgram>();
  /** When a loop effect's program was last made (the frames meanwhile are not the device's pace), and the loop
   *  effects the fluid's light pass has had. */
  madeAt = -Infinity;
  private lightLoops = new Set<number>();
  /** The fluid the light flows in (decision 0047): made the first time the Fluid setting asks for it (nothing of it
   *  before), let go after ten seconds unasked, and not made again where this device cannot (`fluidOut`). */
  fluid: Fluid | null = null;
  private fluidOut = false;
  private fluidUnwanted = 0;
  // The fluid as shown, and its read-back: followed rather than switched (2026-10-18: Fluid's routes crossing 0 stopped
  // the flow on the spot, its warp gone with it, and the smeared light fell apart into specks every time the music
  // tipped it). Rising over a third of a second, falling over a second and a half; while any is left the fluid keeps
  // flowing (its pushes stopped, its motion dying away), and only once it is all but nothing is the light faded as
  // without one, the same as a flow at rest.
  private fluidShown = 0;
  warpShown = 0;
  private empty: WebGLVertexArrayObject;
  private now: () => number;
  private u: (p: WebGLProgram, name: string) => WebGLUniformLocation | null;

  constructor(gl: WebGL2RenderingContext, now: () => number) {
    this.gl = gl;
    this.now = now;
    this.u = uniforms(gl);
    this.fade = program(gl, FULL, FADE);
    this.empty = gl.createVertexArray()!;
  }

  /** The fluid's grid for the light's new size. */
  resize(width: number, height: number) {
    this.fluid?.resize(...gridFor(width, height));
  }

  /** The light kept from the last frame into this one (bound, blending on), `since` seconds on: faded where it was;
   *  read nearer the middle (the tunnel) or by the loop's effect; or carried along the fluid's flow, its Lift reading the
   *  light at `heatK` (nodes/show.ts densityScale). The fluid drawn with, if any. */
  draw(light: Light, look: CarryParams, since: number, loop: LoopNow | null, heatK = 0): Fluid | null {
    const gl = this.gl;
    const follow = (shown: number, to: number) => shown + (to - shown) * (1 - Math.exp(-since / (to > shown ? FLUID_RISE : FLUID_FALL)));
    this.fluidShown = follow(this.fluidShown, Math.max(0, look.fluid));
    this.warpShown = follow(this.warpShown, Math.max(0, look.warp));
    if (this.fluidShown < FLUID_LEAST && look.fluid < FLUID_LEAST) this.fluidShown = this.warpShown = 0;
    const fluid = this.fluidFor(this.fluidShown, light);
    if (fluid) {
      if (loop && !this.lightLoops.has(loop.id)) {
        this.lightLoops.add(loop.id);
        this.madeAt = this.now();
      }
      fluid.push(look.forces);
      fluid.step(look.dt, look.flow, look.flow.lift && light.acc ? { light: light.acc, k: heatK } : null);
      const other = 1 - light.cur;
      fluid.advect({ acc: light.acc!, bandsA: light.bands!, bandsB: light.bandsB! }, light.fbos[other], look.keep, light.width, light.height, this.tunnelZoom(look.tunnel, since), loop ? { key: String(loop.id), glsl: loopFunction(loop.id, FLOW_IN_LIGHT), ...loop } : undefined, look.facets);
      light.cur = other;
      light.use();
      gl.bindFramebuffer(gl.FRAMEBUFFER, light.accFbo);
      gl.viewport(0, 0, light.width, light.height);
      gl.enable(gl.BLEND);
    } else if (look.tunnel > 0 || loop) {
      this.tunnel(light, look.keep, this.tunnelZoom(look.tunnel, since), loop);
      gl.bindFramebuffer(gl.FRAMEBUFFER, light.accFbo);
      gl.viewport(0, 0, light.width, light.height);
      gl.enable(gl.BLEND);
    } else {
      gl.useProgram(this.fade);
      gl.uniform1f(this.u(this.fade, 'uKeep'), look.keep);
      gl.blendFunc(gl.ZERO, gl.SRC_ALPHA);
      gl.bindVertexArray(this.empty);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    }
    return fluid;
  }

  /** The tunnel's factor this frame at `rate`: read this much nearer the middle (under 1: drawn larger). */
  private tunnelZoom(rate: number, since: number) {
    return rate > 0 ? Math.exp(-rate * since) : 1;
  }

  /** The tunnel's pass where nothing flows: the buffers copied into the other set, read `zoom` nearer the middle and
   *  kept as the fade keeps them. */
  private tunnel(light: Light, keep: number, zoom: number, loop: LoopNow | null = null) {
    const gl = this.gl;
    const other = 1 - light.cur;
    gl.disable(gl.BLEND);
    gl.bindFramebuffer(gl.FRAMEBUFFER, light.fbos[other]);
    gl.viewport(0, 0, light.width, light.height);
    gl.bindVertexArray(this.empty);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, light.accs[light.cur]);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, light.bandSets[light.cur]);
    gl.activeTexture(gl.TEXTURE2);
    gl.bindTexture(gl.TEXTURE_2D, light.bandSetsB[light.cur]);
    const id = loop?.id ?? 0;
    let p = this.carrying2.get(id);
    if (!p) {
      // One that does not compile on this device: the carry without it, rather than no frame.
      try {
        p = program(gl, FULL, tunnelSource(loopFunction(id)));
      } catch (e) {
        console.warn('Visuals: a loop effect did not compile here', id, e);
        p = this.carrying2.get(0) ?? program(gl, FULL, tunnelSource(loopFunction(0)));
      }
      this.carrying2.set(id, p);
      this.madeAt = this.now();
    }
    gl.useProgram(p);
    const u = (n: string) => this.u(p!, n);
    gl.uniform1f(u('uLoopA'), loop?.amount ?? 0);
    gl.uniform4f(u('uLoopTime'), ...(loop?.time ?? ([0, 0, 0, 0] as const)));
    gl.uniform1f(u('uLoopFrames'), loop?.frames ?? 1);
    gl.uniform2f(u('uLoopK'), loop?.knobs?.[0] ?? 0.5, loop?.knobs?.[1] ?? 0.5);
    gl.uniform1i(u('uAcc'), 0);
    gl.uniform1i(u('uBandsA'), 1);
    gl.uniform1i(u('uBandsB'), 2);
    gl.uniform2f(u('uSize'), light.width, light.height);
    gl.uniform1f(u('uZoom'), zoom);
    gl.uniform1f(u('uKeep'), keep);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.activeTexture(gl.TEXTURE0);
    light.cur = other;
    light.use();
  }

  /** The fluid, where the Fluid setting asks for one (`amount` over 0) and this device can make it; let go after ten
   *  seconds unasked, so a passing 0 does not make it again and again. */
  private fluidFor(amount: number, light: Light): Fluid | null {
    const now = this.now();
    if (!(amount > 0)) {
      if (this.fluid && now - this.fluidUnwanted > 10_000) {
        this.fluid.dispose();
        this.fluid = null;
      }
      return null;
    }
    this.fluidUnwanted = now;
    if (this.fluid || this.fluidOut) return this.fluid;
    try {
      if (!Fluid.supported(this.gl)) throw new Error('not supported');
      this.fluid = new Fluid(this.gl, ...gridFor(light.width, light.height));
    } catch (e) {
      console.warn('Visuals: no fluid on this device', e);
      this.fluidOut = true;
    }
    return this.fluid;
  }

  dispose() {
    const gl = this.gl;
    this.fluid?.dispose();
    this.fluid = null;
    gl.deleteVertexArray(this.empty);
    gl.deleteProgram(this.fade);
    for (const p of this.carrying2.values()) gl.deleteProgram(p);
  }
}
