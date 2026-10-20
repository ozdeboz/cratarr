// The patch's picture past the flame (decision 0056, step 4; 0057 step 3, the compose step of the picture graph): the
// layers after the flame, each drawn by its source into a picture of its own and blended over the flame's (the
// `layers` Mix); then the picture's own loop (loop-feedback → Warp, Keep … → loop-mix, Lighten), drawn as one pass:
// the last frame's picture moved and kept under the new one, MilkDrop's feedback. Its targets come from a pool by
// size and format (picture-graph.ts); the Feedback keeps the pass's own target for the next frame, no copy. The
// renderer calls `run` between its show and the effects when `composeActive` says there is something to do.
import { keptPerSecond } from './loopmods';
import type { FrameInfo, Layer, Module, PatchExtra, PictureModule } from './patch';
import { Pool, type Spec } from './picture-graph';
import { createSource, SOURCE_TYPES } from './sources/index';
import { MIX_MODES, portsOf, type PortRef } from './ports';
import { createFluidLoop, FLUID_LOOP } from './fluid-loop';
import { createEngine } from './engine-module';

/** What an `audio` port's normal gives this frame: Music's sound (the frame's samples); nothing else gives one yet. */
const audioOf = (n: PortRef | undefined, frame: FrameInfo) => (n?.module === 'music' && n.port === 'audio' ? frame.waveform : null);
/** Each source type's audio inputs (0057 §1), by type. */
const AUDIO_IN = new Map(SOURCE_TYPES.map((t) => [t.type, portsOf(t).in.filter((p) => p.type === 'audio')]));

/** The frame as a module of `type` takes it: with its typed inputs (its audio ports' samples, from their normals; no
 *  cable carries sound yet), or as it is where it has none. */
export function withInputs(type: string, frame: FrameInfo): FrameInfo {
  const ports = AUDIO_IN.get(type);
  if (!ports?.length) return frame;
  return { ...frame, inputs: Object.fromEntries(ports.map((p) => [p.id, audioOf(p.normal, frame)])) };
}

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

/** Each Mix mode's number in the shader: its place in MIX_MODES. */
const MODE = Object.fromEntries(MIX_MODES.map((m, i) => [m, i])) as Record<(typeof MIX_MODES)[number], number>;

/** The Mix of `n` layers over the picture in one pass (decision 0057 §4), each over the picture so far (both
 *  premultiplied) as its mode says: added, screened, laid over, multiplied, the picture seen through it (window:
 *  dimmed by its amount where the layer is not), the brighter of the two, or the picture less it (subtract: never
 *  below black), by its amount; the picture so far kept in the shader, not in a target
 *  between. Before that, a layer's glow (its light spread over a ring of taps) and the first's tint (its colours the
 *  picture's around it: the hue of a wider ring's mean, at its own brightness; its own where the picture is dark): a
 *  later layer that tints reads the picture so far around it, so it starts a pass of its own. Taps one by one, no
 *  arrays (Mali's float[] miscompiles). */
export function mixShader(n: number): string {
  const layer = (i: number) => `  {
    vec4 b = texture(uLayer${i}, vUv);
    if (uGlow${i} > 0.0) b += (ring(uLayer${i}, 0.012) * 0.5 + ring(uLayer${i}, 0.03) * 0.3) / 8.0 * 2.5 * uGlow${i};
${i === 0 ? `    if (uTint0 > 0.0) {
      vec3 m = ring(uBase, 0.06).rgb / 8.0 * uBaseGain;
      float top = max(max(m.r, m.g), m.b);
      float lit = max(max(b.r, b.g), b.b);
      b.rgb = mix(b.rgb, lit * m / max(top, 1e-3), uTint0 * smoothstep(0.01, 0.12, top));
    }
` : ''}    float cover = clamp(max(max(b.r, b.g), max(b.b, b.a)) * 1.5, 0.0, 1.0);
    b *= uAmount${i};
    if (uMode${i} == ${MODE.screen}) c = c + b.rgb - c * b.rgb;
    else if (uMode${i} == ${MODE.over}) c = b.rgb + c * (1.0 - b.a);
    else if (uMode${i} == ${MODE.multiply}) c = c * mix(vec3(1.0), b.rgb / max(b.a, 1e-4), b.a);
    else if (uMode${i} == ${MODE.window}) c = c * mix(1.0 - uAmount${i}, 1.0, cover);
    else if (uMode${i} == ${MODE.lighten}) c = max(c, b.rgb);
    else if (uMode${i} == ${MODE.subtract}) c = max(vec3(0.0), c - b.rgb);
    else c = c + b.rgb;
  }`;
  const each = (f: (i: number) => string) => Array.from({ length: n }, (_, i) => f(i)).join('\n');
  return `#version 300 es
precision highp float;
in vec2 vUv;
out vec4 outColor;
uniform sampler2D uBase;
uniform float uBaseGain;
uniform float uTint0;
uniform vec2 uAspect;
${each((i) => `uniform sampler2D uLayer${i};\nuniform int uMode${i};\nuniform float uAmount${i};\nuniform float uGlow${i};`)}
vec4 ring(sampler2D t, float r) {
  vec2 d = uAspect * r;
  float h = 0.7071;
  return texture(t, vUv + vec2(d.x, 0.0)) + texture(t, vUv - vec2(d.x, 0.0)) + texture(t, vUv + vec2(0.0, d.y)) +
    texture(t, vUv - vec2(0.0, d.y)) + texture(t, vUv + d * h) + texture(t, vUv - d * h) +
    texture(t, vUv + vec2(d.x, -d.y) * h) + texture(t, vUv + vec2(-d.x, d.y) * h);
}
void main() {
  vec3 c = texture(uBase, vUv).rgb * uBaseGain;
${each(layer)}
  outColor = vec4(c, 1.0);
}`;
}

/** The most layers a Mix takes in one pass (its inputs past the flame's). */
const MIX_PASS = 4;

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

/** Whether there is anything to compose: the flame layer less than whole, a layer past the flame with something of
 *  it, or the picture's loop. */
export function composeActive(extra: PatchExtra, knobsOf: (m: Module) => Record<string, number>, flame = 1): boolean {
  return flame < 1 || extra.layers.some((l) => l.blend.amount > 0) || extra.loop.some((m) => (knobsOf(m).amount ?? 0) > 0);
}

export class Compositor {
  private gl: WebGL2RenderingContext;
  private mixes = new Map<number, WebGLProgram>();
  private loop: WebGLProgram;
  private empty: WebGLVertexArrayObject;
  private spec: Spec = { w: 0, h: 0, format: 'rgba8' };
  /** Each layer's source, running, by its source module's id and type. */
  private sources = new Map<string, { type: string; module: PictureModule | null }>();
  /** Every other running module (a layer's, the loop's), by its module's id. */
  private running = new Map<string, { type: string; module: PictureModule | null }>();
  private pool: Pool<Target>;
  /** The loop's Feedback: the picture its loop gave last frame (the pass's own target, kept from the pool). */
  private held: Target | null = null;
  /** What the last run gave (read after it, by the effects or the screen): back to the pool at the next. */
  private given: Target | null = null;
  private time = 0;

  constructor(gl: WebGL2RenderingContext) {
    this.gl = gl;
    this.loop = program(gl, FULL, LOOP);
    this.empty = gl.createVertexArray()!;
    // Half floats where they can be drawn into (the loop's faint trails would band in 8 bits), else 8 bits.
    this.spec.format = gl.getExtension('EXT_color_buffer_float') || gl.getExtension('EXT_color_buffer_half_float') ? 'rgba16f' : 'rgba8';
    this.pool = new Pool<Target>(
      (s) => this.target(s),
      (t) => this.free(t),
    );
  }

  /** The picture composed from `picture` (the flame's, `width` × `height`; `flame` of it shown), the picture's own
   *  loop drawn where the graph has one (`loop`): the texture the effects or the screen read next. */
  run(picture: WebGLTexture, width: number, height: number, extra: PatchExtra, knobsOf: (m: Module) => Record<string, number>, frame: FrameInfo, flame = 1, loop = true): WebGLTexture {
    const gl = this.gl;
    this.sized(width, height);
    if (this.given && this.given !== this.held) this.pool.give(this.given);
    this.given = null;
    this.time += frame.dt;
    // The layers, each drawn alone, then over what there is so far; the flame's share taken with the first (or alone,
    // where no other layer draws).
    let base: WebGLTexture = picture;
    let mixed: Target | null = null;
    let gain = Math.min(1, Math.max(0, flame));
    type Drawn = { t: Target; mode: Layer['blend']['mode']; amount: number; tint: number; glow: number };
    const drawn: Drawn[] = [];
    for (const layer of extra.layers) {
      if (layer.blend.amount <= 0) continue;
      const t = this.drawLayer(layer, knobsOf, frame);
      const k = knobsOf(layer.source);
      if (t) drawn.push({ t, mode: layer.blend.mode, amount: layer.blend.amount, tint: k.tint ?? 0, glow: k.glow ?? 0 });
    }
    // The Mix: as many layers a pass as it takes, a pass ending before a later layer that tints (it reads the picture
    // so far around it), or with none, the flame's share alone.
    const pass = (layers: Drawn[]) => {
      const t = this.pool.take(this.spec);
      const p = this.mixProgram(layers.length);
      gl.bindFramebuffer(gl.FRAMEBUFFER, t.fbo);
      gl.viewport(0, 0, this.spec.w, this.spec.h);
      gl.disable(gl.BLEND);
      gl.useProgram(p);
      this.bind(0, base, 'uBase', p);
      gl.uniform1f(gl.getUniformLocation(p, 'uBaseGain'), gain);
      gl.uniform1f(gl.getUniformLocation(p, 'uTint0'), layers[0]?.tint ?? 0);
      gl.uniform2f(gl.getUniformLocation(p, 'uAspect'), Math.min(1, this.spec.h / this.spec.w), Math.min(1, this.spec.w / this.spec.h));
      layers.forEach((l, i) => {
        this.bind(1 + i, l.t.tex, `uLayer${i}`, p);
        gl.uniform1i(gl.getUniformLocation(p, `uMode${i}`), MODE[l.mode]);
        gl.uniform1f(gl.getUniformLocation(p, `uAmount${i}`), l.amount);
        gl.uniform1f(gl.getUniformLocation(p, `uGlow${i}`), l.glow);
      });
      this.full();
      if (mixed) this.pool.give(mixed);
      mixed = t;
      base = t.tex;
      gain = 1;
    };
    let run: Drawn[] = [];
    for (const l of drawn) {
      if (run.length && (l.tint > 0 || run.length === MIX_PASS)) (pass(run), (run = []));
      run.push(l);
    }
    if (run.length || gain < 1) pass(run);
    for (const l of drawn) this.pool.give(l.t);
    // The picture's own loop: its Warps summed, its Keeps multiplied (each by its Amount).
    let keep = 1;
    let kept = false;
    for (const m of loop ? extra.loop : []) {
      if (m.type !== 'keep') continue;
      const k = knobsOf(m);
      if ((k.amount ?? 0) <= 0) continue;
      kept = true;
      keep *= 1 - k.amount + k.amount * keptPerSecond(k.trails ?? 0.5) ** frame.dt;
    }
    if (!kept) {
      this.given = mixed;
      return base;
    }
    let zoom = 0, turn = 0, dx = 0, dy = 0, stretch = 0, wobble = 0, size = 0.5;
    for (const m of extra.loop) {
      if (m.type !== 'warp') continue;
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
    // The loop's other modules (claude-5f's fluid…), in order, on the last picture before it is warped and kept.
    let last = this.held!;
    const scratch: Target[] = [];
    for (const m of extra.loop) {
      if (m.type === 'warp' || m.type === 'keep') continue;
      const run = this.module(m.id, m.type);
      if (!run) continue;
      const out = this.pool.take(this.spec);
      run.draw(gl, last.tex, out.fbo, this.spec.w, this.spec.h, knobsOf(m), frame);
      done(gl);
      last = out;
      scratch.push(out);
    }
    const next = this.pool.take(this.spec);
    gl.bindFramebuffer(gl.FRAMEBUFFER, next.fbo);
    gl.viewport(0, 0, this.spec.w, this.spec.h);
    gl.disable(gl.BLEND);
    gl.bindVertexArray(null);
    gl.useProgram(this.loop);
    this.bind(0, base, 'uNow', this.loop);
    this.bind(1, last.tex, 'uLast', this.loop);
    const u = (n: string) => gl.getUniformLocation(this.loop, n);
    gl.uniform1f(u('uAspect'), this.spec.w / this.spec.h);
    // A second's worth at full: zoom half again (in or out), half a turn, a fifth of the picture across.
    gl.uniform1f(u('uZoom'), Math.exp(0.4 * zoom * dt));
    gl.uniform1f(u('uTurn'), Math.PI * turn * dt);
    gl.uniform2f(u('uDrift'), 0.2 * dx * dt, 0.2 * dy * dt);
    gl.uniform1f(u('uStretch'), Math.exp(0.2 * stretch * dt));
    gl.uniform3f(u('uWobble'), 0.02 * wobble * Math.min(1, dt * 30), 4 + 20 * size, this.time * 1.7);
    gl.uniform1f(u('uKeep'), keep);
    this.full();
    done(gl);
    // The Feedback adopts the pass's own target for the next frame; what it held is read by now.
    for (const t of scratch) this.pool.give(t);
    if (mixed) this.pool.give(mixed);
    this.pool.give(this.held!);
    this.held = next;
    this.given = next;
    return next.tex;
  }

  /** The one-pass Mix's program for `n` layers (made once each). */
  private mixProgram(n: number): WebGLProgram {
    let p = this.mixes.get(n);
    if (!p) this.mixes.set(n, (p = program(this.gl, FULL, mixShader(n))));
    return p;
  }

  /** A layer's picture: its source drawn into a target, then its modules, each from the last into another. */
  private drawLayer(layer: Layer, knobsOf: (m: Module) => Record<string, number>, frame: FrameInfo): Target | null {
    const gl = this.gl;
    let s = this.sources.get(layer.source.id);
    if (!s || s.type !== layer.source.type) {
      s?.module?.dispose(gl);
      let module: PictureModule | null = null;
      try {
        module = createSource(layer.source.type, gl, layer.source.id);
      } catch (e) {
        console.warn('Visuals: a layer could not be made here', layer.source.type, e);
      }
      s = { type: layer.source.type, module };
      this.sources.set(layer.source.id, s);
    }
    if (!s.module) return null;
    let t = this.pool.take(this.spec);
    s.module.draw(gl, null, t.fbo, this.spec.w, this.spec.h, knobsOf(layer.source), withInputs(layer.source.type, frame));
    done(gl);
    for (const m of layer.modules) {
      const run = this.module(m.id, m.type);
      if (!run) continue;
      const out = this.pool.take(this.spec);
      run.draw(gl, t.tex, out.fbo, this.spec.w, this.spec.h, knobsOf(m), frame);
      done(gl);
      this.pool.give(t);
      t = out;
    }
    return t;
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
    const modules = new Set([...extra.layers.flatMap((l) => l.modules.map((m) => m.id)), ...extra.loop.map((m) => m.id)]);
    for (const [id, r] of this.running) if (!modules.has(id)) (r.module?.dispose(this.gl), this.running.delete(id));
  }

  /** At a new size every target goes, and the loop starts from nothing. */
  private sized(width: number, height: number) {
    if (width === this.spec.w && height === this.spec.h && this.held) return;
    this.pool.clear();
    this.given = null;
    this.spec = { ...this.spec, w: width, h: height };
    this.held = this.pool.take(this.spec);
  }

  private target(s: Spec): Target {
    const gl = this.gl;
    const tex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, tex);
    if (s.format === 'rgba16f') gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA16F, s.w, s.h, 0, gl.RGBA, gl.HALF_FLOAT, null);
    else gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, s.w, s.h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
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
    this.pool.clear();
    for (const p of this.mixes.values()) gl.deleteProgram(p);
    gl.deleteProgram(this.loop);
    gl.deleteVertexArray(this.empty);
  }
}
