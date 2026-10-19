// The effect engines as modules (decision 0056; the owner's example: "a kaleidoscope on the shape layer"): 0050's three
// engines (effects.ts), each running one of its effects on whatever picture it is put on, a layer's or the blended one.
// The same GLSL as the finish's: the picture read where the effect's shapes say, then its colours and echoes. Each module
// keeps its own: its effect's clock (EffectClocks, on the frame's time), and for an echo its own last picture.
//
// Its neutral is exact: at Amount 0 a plain copy. An effect that holds where its time took it (Rotate, Scroll) eases
// back to its rest as Amount nears 0, so a morph to neutral does not jump at the end. The layer's alpha is kept, so a
// kaleidoscope on the shapes leaves what lies between them clear.

import { EffectClocks, ENGINE_EFFECTS, MOST_PICTURE, beatOf, effectOf, pictureShader, type Engine } from './effects';
import type { FrameInfo, PictureModule } from './patch';
import { program } from './sources/gl';

const VERTEX = `#version 300 es
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;
const COPY = `#version 300 es
precision highp float;
uniform sampler2D uPicture;
uniform vec2 uView;
out vec4 o;
void main() { o = textureLod(uPicture, gl_FragCoord.xy / uView, 0.0); }`;

/** Below this Amount, a held effect's time is eased back to rest (over Amount 0 to this). */
const HOLD_EASE = 0.1;

/** The engine an engine module's type names (`kaleidoscope-engine` → kaleidoscope), or null. */
export function engineOf(type: string): Engine | null {
  const key = type.replace(/-engine$/, '');
  return key in ENGINE_EFFECTS ? (key as Engine) : null;
}

/** The effect an engine module runs at its Effect knob (a step: the engine's list from 1), and the time it is drawn
 *  at: a held one's eased to rest as Amount nears 0. Pure. */
export function engineFrame(engine: Engine, knobs: Record<string, number>, time: readonly [number, number]): { id: number; amount: number; k: [number, number]; time: [number, number] } {
  const list = ENGINE_EFFECTS[engine];
  const place = Math.min(list.length, Math.max(1, Math.round(knobs.effect ?? 1)));
  const id = list[place - 1]?.id ?? 0;
  const unit = (v: number | undefined, d: number) => (typeof v === 'number' && Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : d);
  const amount = unit(knobs.amount, 0);
  const ease = effectOf(id)?.holds ? Math.min(1, amount / HOLD_EASE) : 1;
  return { id, amount, k: [unit(knobs.k1, 0.5), unit(knobs.k2, 0.5)], time: [time[0] * ease, time[1] * ease] };
}

type Target = { texture: WebGLTexture; fbo: WebGLFramebuffer; w: number; h: number };

/** A running engine module of `type` (`ENGINE_TYPES`'). */
export function createEngine(type: string, gl: WebGL2RenderingContext): PictureModule {
  const engine = engineOf(type);
  if (!engine) throw new Error(`No such engine: ${type}`);
  const clocks = new EffectClocks();
  const programs = new Map<number, WebGLProgram | null>();
  const locations = new Map<WebGLProgram, Map<string, WebGLUniformLocation | null>>();
  const vao = gl.createVertexArray()!;
  let copy: WebGLProgram | null = null;
  // An echo's own last picture: drawn into `cur` (reading `prev`), copied out, then the two swapped.
  let cur: Target | null = null;
  let prev: Target | null = null;
  const floats = !!gl.getExtension('EXT_color_buffer_float');

  const at = (p: WebGLProgram, name: string) => {
    let m = locations.get(p);
    if (!m) locations.set(p, (m = new Map()));
    if (!m.has(name)) m.set(name, gl.getUniformLocation(p, name));
    return m.get(name) ?? null;
  };
  const shaderOf = (id: number): WebGLProgram | null => {
    if (!programs.has(id)) {
      const source = pictureShader([id], true);
      let p: WebGLProgram | null = null;
      try {
        p = source ? program(gl, VERTEX, source) : null;
      } catch (e) {
        // One that does not compile here: the picture as it is, rather than no frame.
        console.warn('Visuals: an effect did not compile here', id, e);
      }
      programs.set(id, p);
    }
    return programs.get(id) ?? null;
  };
  const target = (w: number, h: number): Target => {
    const texture = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, texture);
    for (const [k, v] of [
      [gl.TEXTURE_MIN_FILTER, gl.LINEAR],
      [gl.TEXTURE_MAG_FILTER, gl.LINEAR],
      [gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE],
      [gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE],
    ])
      gl.texParameteri(gl.TEXTURE_2D, k, v);
    if (floats) gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA16F, w, h, 0, gl.RGBA, gl.HALF_FLOAT, null);
    else gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    const fbo = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    return { texture, fbo, w, h };
  };
  const free = (t: Target | null) => {
    if (!t) return;
    gl.deleteFramebuffer(t.fbo);
    gl.deleteTexture(t.texture);
  };
  const drawCopy = (from: WebGLTexture, into: WebGLFramebuffer | null, w: number, h: number) => {
    copy ??= program(gl, VERTEX, COPY);
    gl.bindFramebuffer(gl.FRAMEBUFFER, into);
    gl.viewport(0, 0, w, h);
    gl.useProgram(copy);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, from);
    gl.uniform1i(at(copy, 'uPicture'), 0);
    gl.uniform2f(at(copy, 'uView'), w, h);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  };

  return {
    draw(gl, input, out, width, height, knobs, frame: FrameInfo) {
      if (!input) return;
      gl.disable(gl.BLEND);
      gl.bindVertexArray(vao);
      const fx = engineFrame(engine, knobs, [0, 0]);
      // The clock runs on the frame's time, at the effect's speed for its amount and knobs (as the finish's).
      clocks.run({ ids: [fx.id], amounts: [fx.amount], knobs: [fx.k], loop: 0, loopAmount: 0, t: frame.time });
      const now = engineFrame(engine, knobs, clocks.times[0]);
      const p = now.amount > 0 ? shaderOf(now.id) : null;
      if (!p) {
        drawCopy(input, out, width, height);
        return;
      }
      const echo = effectOf(now.id)?.kind === 'echo';
      if (echo && (!cur || cur.w !== width || cur.h !== height)) {
        free(cur);
        free(prev);
        cur = target(width, height);
        prev = target(width, height);
      }
      gl.bindFramebuffer(gl.FRAMEBUFFER, echo ? cur!.fbo : out);
      gl.viewport(0, 0, width, height);
      gl.useProgram(p);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, input);
      gl.activeTexture(gl.TEXTURE1);
      gl.bindTexture(gl.TEXTURE_2D, echo ? prev!.texture : input);
      gl.activeTexture(gl.TEXTURE2);
      gl.bindTexture(gl.TEXTURE_2D, input);
      gl.uniform1i(at(p, 'uPicture'), 0);
      gl.uniform1i(at(p, 'uPrev'), 1);
      gl.uniform1i(at(p, 'uFlow'), 2);
      gl.uniform2f(at(p, 'uFlowScale'), 0, 0);
      gl.uniform2f(at(p, 'uView'), width, height);
      gl.uniform1f(at(p, 'uAspect'), width / height);
      gl.uniform1fv(at(p, 'uFxA'), Float32Array.from({ length: MOST_PICTURE }, (_, k) => (k === 0 ? now.amount : 0)));
      gl.uniform2fv(at(p, 'uFxT'), Float32Array.from({ length: MOST_PICTURE * 2 }, (_, i) => (i < 2 ? now.time[i] : 0)));
      gl.uniform2fv(at(p, 'uFxK'), Float32Array.from({ length: MOST_PICTURE * 2 }, (_, i) => (i < 2 ? now.k[i] : 0.5)));
      gl.uniform1f(at(p, 'uFxFrames'), Math.max(0, frame.dt) * 60);
      gl.uniform4f(at(p, 'uFxBeat'), ...beatOf(frame.beat));
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      if (echo) {
        drawCopy(cur!.texture, out, width, height);
        [cur, prev] = [prev, cur];
      }
    },
    dispose(gl) {
      for (const p of programs.values()) if (p) gl.deleteProgram(p);
      programs.clear();
      if (copy) gl.deleteProgram(copy);
      copy = null;
      free(cur);
      free(prev);
      cur = prev = null;
      gl.deleteVertexArray(vao);
    },
  };
}
