// FROZEN (decision 0057, step 3, C1): compose.ts as it was at 28d83ed9, for the picture lab only (picture-lab.ts's
// `legacy` pipeline), the picture every step of the picture graph is compared with. Never changed; nothing else
// imports it. It goes when the old code goes.

// The patch's picture past the flame (decision 0056, step 4): the layers after the flame, each drawn by its source
// into a picture of its own and blended over the flame's; then the picture's own loop, the last frame's picture moved
// (Warp) and kept (Keep) under the new one: MilkDrop's feedback, as modules. The flame's own loop stays inside the
// flame (render.ts). The renderer calls `run` between its show and finish when `active` says there is something to do;
// with nothing beyond today's chain it is never called, and the picture is exactly as before.

import { keptPerSecond } from './loopmods';
import { BLENDS, type FrameInfo, type Layer, type Module, type PatchExtra, type PictureModule } from './patch';
import { createSource } from './sources/index';
import { createFluidLoop, FLUID_LOOP } from './fluid-loop';
import { createEngine } from './engine-module';

/** The picture modules the compositor runs by type: those a layer carries after its source, and those in the
 *  picture's own loop besides Warp and Keep (drawn here). Each draws `input` into `out`, an exact copy at its neutral. */
export const MODULE_MAKERS: Record<string, (gl: WebGL2RenderingContext) => PictureModule> = {
  [FLUID_LOOP.type]: () => createFluidLoop(),
  // The effect engines on a layer (claude-5f's engine-module.ts): a kaleidoscope on the shapes.
  'kaleidoscope-engine': (gl) => createEngine('kaleidoscope-engine', gl),
  'eyedo-engine': (gl) => createEngine('eyedo-engine', gl),
  'hydra-engine': (gl) => createEngine('hydra-engine', gl),
};
import { done, program } from './sources/gl';

const FULL = `#version 300 es
out vec2 vUv;
void main() {
  vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  vUv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

// A layer over the picture so far (both premultiplied): added, screened, laid over, multiplied, or the picture seen
// through it (window: dimmed by `uAmount` where the layer is not), by `uAmount`. Before that, the layer meets the
// picture: its glow (its light spread over a ring of taps, `uGlow`) and its colours the picture's around it (`uTint`:
// the hue of a wider ring's mean, at the layer's own brightness; its own where the picture is dark). Taps one by one,
// no arrays (Mali's float[] miscompiles).
const BLEND = `#version 300 es
precision highp float;
in vec2 vUv;
out vec4 outColor;
uniform sampler2D uBase;
uniform sampler2D uLayer;
uniform int uMode;
uniform float uAmount;
uniform float uBaseGain;
uniform float uTint;
uniform float uGlow;
uniform vec2 uAspect;
vec4 ring(sampler2D t, float r) {
  vec2 d = uAspect * r;
  float h = 0.7071;
  return texture(t, vUv + vec2(d.x, 0.0)) + texture(t, vUv - vec2(d.x, 0.0)) + texture(t, vUv + vec2(0.0, d.y)) +
    texture(t, vUv - vec2(0.0, d.y)) + texture(t, vUv + d * h) + texture(t, vUv - d * h) +
    texture(t, vUv + vec2(d.x, -d.y) * h) + texture(t, vUv + vec2(-d.x, d.y) * h);
}
void main() {
  vec4 a = texture(uBase, vUv) * uBaseGain;
  vec4 b = texture(uLayer, vUv);
  if (uGlow > 0.0) b += (ring(uLayer, 0.012) * 0.5 + ring(uLayer, 0.03) * 0.3) / 8.0 * 2.5 * uGlow;
  if (uTint > 0.0) {
    vec3 m = ring(uBase, 0.06).rgb / 8.0 * uBaseGain;
    float top = max(max(m.r, m.g), m.b);
    float lit = max(max(b.r, b.g), b.b);
    b.rgb = mix(b.rgb, lit * m / max(top, 1e-3), uTint * smoothstep(0.01, 0.12, top));
  }
  float cover = clamp(max(max(b.r, b.g), max(b.b, b.a)) * 1.5, 0.0, 1.0);
  b *= uAmount;
  vec3 c;
  if (uMode == 1) c = a.rgb + b.rgb - a.rgb * b.rgb;
  else if (uMode == 2) c = b.rgb + a.rgb * (1.0 - b.a);
  else if (uMode == 3) c = a.rgb * mix(vec3(1.0), b.rgb / max(b.a, 1e-4), b.a);
  else if (uMode == 4) c = a.rgb * mix(1.0 - uAmount, 1.0, cover);
  else c = a.rgb + b.rgb;
  outColor = vec4(c, 1.0);
}`;

// The picture's own loop: the last picture read where the Warp moves it from (about the middle: zoomed, turned,
// drifted, stretched, wobbled), kept by the Keep's share, and the new picture over it: the brighter of the two, so the
// trails fade as they are kept and never build past the picture itself (our layers are whole pictures each frame,
// not MilkDrop's few lines; added up they would run to white).
const LOOP = `#version 300 es
precision highp float;
in vec2 vUv;
out vec4 outColor;
uniform sampler2D uNow;
uniform sampler2D uLast;
uniform float uAspect;
uniform float uZoom;
uniform float uTurn;
uniform vec2 uDrift;
uniform float uStretch;
uniform vec3 uWobble;
uniform float uKeep;
void main() {
  vec2 p = (vUv - 0.5) * vec2(uAspect, 1.0);
  p /= uZoom;
  float c = cos(uTurn), s = sin(uTurn);
  p = vec2(c * p.x - s * p.y, s * p.x + c * p.y);
  p *= vec2(1.0 / uStretch, uStretch);
  p += uWobble.x * vec2(sin(p.y * uWobble.y + uWobble.z), cos(p.x * uWobble.y - uWobble.z));
  vec2 uv = p / vec2(uAspect, 1.0) + 0.5 - uDrift;
  vec3 last = textureLod(uLast, uv, 0.0).rgb;
  vec3 now = texture(uNow, vUv).rgb;
  outColor = vec4(max(now, uKeep * last), 1.0);
}`;

type Target = { tex: WebGLTexture; fbo: WebGLFramebuffer };

export class Compositor {
  private gl: WebGL2RenderingContext;
  private blend: WebGLProgram;
  private loop: WebGLProgram;
  private empty: WebGLVertexArrayObject;
  private half: boolean;
  private w = 0;
  private h = 0;
  /** Each layer's source, running, by its source module's id and type. */
  private sources = new Map<string, { type: string; module: PictureModule | null }>();
  /** Every other running module (a layer's, the loop's), by its module's id. */
  private running = new Map<string, { type: string; module: PictureModule | null }>();
  private scratch: Target[] = [];
  private layerTargets = new Map<string, Target>();
  private mix: Target[] = [];
  private feedback: Target[] = [];
  private fed = 0;
  private time = 0;

  constructor(gl: WebGL2RenderingContext) {
    this.gl = gl;
    this.blend = program(gl, FULL, BLEND);
    this.loop = program(gl, FULL, LOOP);
    this.empty = gl.createVertexArray()!;
    // Half floats where they can be drawn into (the loop's faint trails would band in 8 bits), else 8 bits.
    this.half = !!gl.getExtension('EXT_color_buffer_float') || !!gl.getExtension('EXT_color_buffer_half_float');
  }

  /** Whether there is anything to compose: the flame layer less than whole, a layer past the flame with something of
   *  it, or the picture's loop. */
  static active(extra: PatchExtra, knobsOf: (m: Module) => Record<string, number>, flame = 1): boolean {
    return flame < 1 || extra.layers.some((l) => l.blend.amount > 0) || extra.loop.some((m) => (knobsOf(m).amount ?? 0) > 0);
  }

  /** The picture composed from `picture` (the flame's, `width` × `height`; `flame` of it shown): the texture finish
   *  is to read next. */
  run(picture: WebGLTexture, width: number, height: number, extra: PatchExtra, knobsOf: (m: Module) => Record<string, number>, frame: FrameInfo, flame = 1): WebGLTexture {
    const gl = this.gl;
    this.sized(width, height);
    this.time += frame.dt;
    // The layers, each drawn alone, then over what there is so far; the flame's share taken with the first (or alone,
    // where no other layer draws).
    let base: WebGLTexture = picture;
    let gain = Math.min(1, Math.max(0, flame));
    let into = 0;
    const over = (drawn: WebGLTexture, mode: Layer['blend']['mode'], amount: number, tint = 0, glow = 0) => {
      const t = this.mix[into];
      gl.bindFramebuffer(gl.FRAMEBUFFER, t.fbo);
      gl.viewport(0, 0, this.w, this.h);
      gl.disable(gl.BLEND);
      gl.useProgram(this.blend);
      this.bind(0, base, 'uBase', this.blend);
      this.bind(1, drawn, 'uLayer', this.blend);
      gl.uniform1i(gl.getUniformLocation(this.blend, 'uMode'), BLENDS.indexOf(mode));
      gl.uniform1f(gl.getUniformLocation(this.blend, 'uTint'), tint);
      gl.uniform1f(gl.getUniformLocation(this.blend, 'uGlow'), glow);
      gl.uniform2f(gl.getUniformLocation(this.blend, 'uAspect'), Math.min(1, this.h / this.w), Math.min(1, this.w / this.h));
      gl.uniform1f(gl.getUniformLocation(this.blend, 'uAmount'), amount);
      gl.uniform1f(gl.getUniformLocation(this.blend, 'uBaseGain'), gain);
      this.full();
      base = t.tex;
      gain = 1;
      into = 1 - into;
    };
    for (const layer of extra.layers) {
      if (layer.blend.amount <= 0) continue;
      const drawn = this.drawLayer(layer, knobsOf, frame);
      const k = knobsOf(layer.source);
      if (drawn) over(drawn, layer.blend.mode, layer.blend.amount, k.tint ?? 0, k.glow ?? 0);
    }
    if (gain < 1) over(picture, 'add', 0);
    // The picture's own loop: its Warps summed, its Keeps multiplied (each by its Amount).
    const warps = extra.loop.filter((m) => m.type === 'warp');
    const keeps = extra.loop.filter((m) => m.type === 'keep');
    let keep = 1;
    let kept = false;
    for (const m of keeps) {
      const k = knobsOf(m);
      if ((k.amount ?? 0) <= 0) continue;
      kept = true;
      keep *= 1 - k.amount + k.amount * keptPerSecond(k.trails ?? 0.5) ** frame.dt;
    }
    if (!kept) {
      this.fed = 0;
      return base;
    }
    let zoom = 0, turn = 0, dx = 0, dy = 0, stretch = 0, wobble = 0, size = 0.5;
    for (const m of warps) {
      const k = knobsOf(m);
      const a = k.amount ?? 0;
      zoom += a * (k.zoom ?? 0);
      turn += a * (k.turn ?? 0);
      dx += a * (k.driftX ?? 0);
      dy += a * (k.driftY ?? 0);
      stretch += a * (k.stretch ?? 0);
      wobble += a * (k.wobble ?? 0);
      size = k.wobbleSize ?? size;
    }
    const dt = frame.dt;
    let last = this.feedback[this.fed];
    // The loop's other modules (claude-5f's fluid…), in order, on the last picture before it is warped and kept.
    let scratchAt = 0;
    for (const m of extra.loop) {
      if (m.type === 'warp' || m.type === 'keep') continue;
      const run = this.module(m.id, m.type);
      if (!run) continue;
      const out = this.scratch[scratchAt];
      run.draw(gl, last.tex, out.fbo, this.w, this.h, knobsOf(m), frame);
      done(gl);
      last = out;
      scratchAt = 1 - scratchAt;
    }
    const next = this.feedback[1 - this.fed];
    gl.bindFramebuffer(gl.FRAMEBUFFER, next.fbo);
    gl.viewport(0, 0, this.w, this.h);
    gl.disable(gl.BLEND);
    gl.bindVertexArray(null);
    gl.useProgram(this.loop);
    this.bind(0, base, 'uNow', this.loop);
    this.bind(1, last.tex, 'uLast', this.loop);
    const u = (n: string) => gl.getUniformLocation(this.loop, n);
    gl.uniform1f(u('uAspect'), this.w / this.h);
    // A second's worth at full: zoom half again (in or out), half a turn, a fifth of the picture across.
    gl.uniform1f(u('uZoom'), Math.exp(0.4 * zoom * dt));
    gl.uniform1f(u('uTurn'), Math.PI * turn * dt);
    gl.uniform2f(u('uDrift'), 0.2 * dx * dt, 0.2 * dy * dt);
    gl.uniform1f(u('uStretch'), Math.exp(0.2 * stretch * dt));
    gl.uniform3f(u('uWobble'), 0.02 * wobble * Math.min(1, dt * 30), 4 + 20 * size, this.time * 1.7);
    gl.uniform1f(u('uKeep'), keep);
    this.full();
    this.fed = 1 - this.fed;
    done(gl);
    return next.tex;
  }

  /** A layer's picture: its source drawn into its target (and, later, its modules after it). */
  private drawLayer(layer: Layer, knobsOf: (m: Module) => Record<string, number>, frame: FrameInfo): WebGLTexture | null {
    const gl = this.gl;
    let s = this.sources.get(layer.source.id);
    if (!s || s.type !== layer.source.type) {
      s?.module?.dispose(gl);
      let module: PictureModule | null = null;
      try {
        module = createSource(layer.source.type, gl);
      } catch (e) {
        console.warn('Visuals: a layer could not be made here', layer.source.type, e);
      }
      s = { type: layer.source.type, module };
      this.sources.set(layer.source.id, s);
    }
    if (!s.module) return null;
    let t = this.layerTargets.get(layer.id);
    if (!t) {
      t = this.target();
      this.layerTargets.set(layer.id, t);
    }
    s.module.draw(gl, null, t.fbo, this.w, this.h, knobsOf(layer.source), frame);
    done(gl);
    // Its modules, in order, each from the last picture into the other scratch (then copied back by being the next
    // one's input): the layer's picture is the last one written.
    let picture = t.tex;
    let into = 0;
    for (const m of layer.modules) {
      const run = this.module(m.id, m.type);
      if (!run) continue;
      const out = this.scratch[into];
      run.draw(gl, picture, out.fbo, this.w, this.h, knobsOf(m), frame);
      done(gl);
      picture = out.tex;
      into = 1 - into;
    }
    return picture;
  }

  /** A running module of `type` for module `id` (made on first use; none for a type the compositor does not run). */
  private module(id: string, type: string): PictureModule | null {
    let r = this.running.get(id);
    if (!r || r.type !== type) {
      r?.module?.dispose(this.gl);
      let module: PictureModule | null = null;
      try {
        module = MODULE_MAKERS[type]?.(this.gl) ?? null;
      } catch (e) {
        console.warn('Visuals: a module could not be made here', type, e);
      }
      r = { type, module };
      this.running.set(id, r);
    }
    return r.module;
  }

  /** Lets go of what layers no longer in the patch held. */
  prune(extra: PatchExtra) {
    const sources = new Set(extra.layers.map((l) => l.source.id));
    for (const [id, s] of this.sources) if (!sources.has(id)) (s.module?.dispose(this.gl), this.sources.delete(id));
    const layers = new Set(extra.layers.map((l) => l.id));
    for (const [id, t] of this.layerTargets) if (!layers.has(id)) (this.free(t), this.layerTargets.delete(id));
    const modules = new Set([...extra.layers.flatMap((l) => l.modules.map((m) => m.id)), ...extra.loop.map((m) => m.id)]);
    for (const [id, r] of this.running) if (!modules.has(id)) (r.module?.dispose(this.gl), this.running.delete(id));
  }

  private sized(width: number, height: number) {
    if (width === this.w && height === this.h && this.mix.length) return;
    for (const t of [...this.mix, ...this.feedback, ...this.scratch, ...this.layerTargets.values()]) this.free(t);
    this.layerTargets.clear();
    this.w = width;
    this.h = height;
    this.mix = [this.target(), this.target()];
    this.feedback = [this.target(), this.target()];
    this.scratch = [this.target(), this.target()];
    this.fed = 0;
  }

  private target(): Target {
    const gl = this.gl;
    const tex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, tex);
    if (this.half) gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA16F, this.w, this.h, 0, gl.RGBA, gl.HALF_FLOAT, null);
    else gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, this.w, this.h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    // Read past its edge (a warp reaching out), the picture is mirrored, not smeared.
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

  private bind(unit: number, tex: WebGLTexture, name: string, p: WebGLProgram) {
    const gl = this.gl;
    gl.activeTexture(gl.TEXTURE0 + unit);
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.uniform1i(gl.getUniformLocation(p, name), unit);
    gl.activeTexture(gl.TEXTURE0);
  }

  private full() {
    const gl = this.gl;
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
  }

  dispose() {
    const gl = this.gl;
    for (const s of [...this.sources.values(), ...this.running.values()]) s.module?.dispose(gl);
    for (const t of [...this.mix, ...this.feedback, ...this.scratch, ...this.layerTargets.values()]) this.free(t);
    gl.deleteProgram(this.blend);
    gl.deleteProgram(this.loop);
    gl.deleteVertexArray(this.empty);
  }
}
